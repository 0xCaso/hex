use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use color_eyre::Result;
use color_eyre::eyre::eyre;

use crate::audio::{AudioInput, AudioInputEvent, CaptureInstant};
use crate::dictation::{DictationCapture, Finish, POST_RELEASE_TAIL};
use crate::events::{DictationPhase, EventLog, TranscriptPhase, VoiceEvent, VoiceState, now_ms};
use crate::feedback::{self, Tone};
use crate::linux_desktop::LinuxIndicator;
use crate::linux_input::{HotkeyEvent, HotkeyStatus, LinuxHotkeyMonitor};
use crate::linux_paste::LinuxPaster;
use crate::local_transcriber::LocalTranscriber;
use crate::transcription_engine::{PendingTranscript, TranscriptionEngine};

const UPDATE_INTERVAL: Duration = Duration::from_millis(20);
/// How long past the tail a release waits for audio that never arrives.
const TAIL_STALL_GRACE: Duration = Duration::from_millis(100);

struct Job {
    transcript: PendingTranscript,
    audio_ms: u64,
}

enum OutputJob {
    Dictation(Job),
    /// Pastes the newest completed dictation again.
    PasteLast,
}

#[derive(Clone, Copy, PartialEq)]
enum OutputKind {
    Dictation,
    PasteLast,
}

/// A release whose capture is still collecting its post-release tail.
struct Finishing {
    ended_at: CaptureInstant,
    deadline: Instant,
}

struct OutputWorker {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Drop for OutputWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// What happens to a transcript before and after it is pasted: the modes
/// that process it and the history that keeps it. The service shares it
/// with the listener so saved settings apply without a restart.
#[derive(Clone)]
pub struct DictationOutput {
    pub history: Option<crate::history::History>,
    pub modes: Arc<RwLock<crate::dictation_modes::ModeRuntime>>,
    pub transformations: Arc<crate::personal_commands::TransformationClient>,
}

impl DictationOutput {
    pub fn from_settings(
        settings: &crate::app_settings::AppSettings,
        history: Option<crate::history::History>,
    ) -> Self {
        Self {
            history,
            modes: Arc::new(RwLock::new(
                crate::dictation_modes::ModeRuntime::from_settings(settings),
            )),
            transformations: Arc::new(crate::personal_commands::TransformationClient::default()),
        }
    }

    /// Processes and pastes one transcript, then records it.
    fn deliver(
        &self,
        raw_text: String,
        paster: &mut LinuxPaster,
        audio_ms: u64,
        inference_ms: u64,
        started: Instant,
    ) -> Result<String> {
        let context = crate::linux_context::capture();
        let modes = self
            .modes
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let processed = modes.process(&raw_text, &context, &self.transformations);
        if processed.text.trim().is_empty() {
            return Err(eyre!("text replacements produced empty output"));
        }
        paster.paste(&processed.text)?;
        if let Some(history) = &self.history {
            let draft = crate::dictation_modes::history_draft(
                raw_text,
                &processed,
                context.application,
                audio_ms,
                inference_ms,
                started.elapsed().as_millis() as u64,
            );
            if let Err(error) = history.record(draft) {
                tracing::warn!(%error, "could not retain Linux dictation history");
            }
        }
        Ok(processed.text)
    }
}

/// Runs hotkey dictation until `shutdown`. A prepared transcriber skips the
/// model load; otherwise the supplied transcription selection is loaded here.
pub fn run(
    event_path: &Path,
    device: Option<&str>,
    shutdown: &AtomicBool,
    settings: &crate::app_settings::AppSettings,
    output_settings: DictationOutput,
    transcriber: Option<LocalTranscriber>,
    hotkey_status: HotkeyStatus,
) -> Result<()> {
    let transcriber =
        transcriber.map_or_else(|| LocalTranscriber::load(&settings.transcription), Ok)?;
    // The engine outlives the output worker, which waits on its transcripts.
    let engine = TranscriptionEngine::start(transcriber)?;
    // Drop audio/hotkeys and close the job queue before joining slow output on
    // every exit, including failures during startup or recording.
    let mut output = OutputWorker {
        stop: Arc::new(AtomicBool::new(false)),
        worker: None,
    };
    let input = match device.or(settings.microphone.as_deref()) {
        Some(name) => AudioInput::open_matching(name)?,
        None => AudioInput::open(&[])?,
    };
    let binding = crate::linux_input::LinuxHotkey::from(&settings.dictation_hotkey);
    let hotkey_label = binding.label();
    let toggle = settings.shortcut_mode == crate::dictation::ShortcutMode::Toggle;
    let paste_with_shift = settings.paste_with_shift;
    let options = crate::linux_input::HotkeyOptions {
        double_tap: settings.double_tap_lock,
        // Double-tap-only needs the double-tap window and a non-modifier key.
        double_tap_only: settings.double_tap_lock
            && settings.double_tap_only
            && settings.dictation_hotkey.key.is_some(),
        toggle,
        paste_last: settings
            .paste_last_hotkey
            .as_ref()
            .map(crate::linux_input::LinuxHotkey::from),
    };
    let hotkey = LinuxHotkeyMonitor::start_with(binding, options, hotkey_status)?;
    let wayland_modifiers = hotkey.wayland_modifiers();
    let indicator = LinuxIndicator::new();
    let (jobs, job_receiver) = mpsc::sync_channel::<OutputJob>(2);
    let (result_sender, results) = mpsc::channel();
    let worker_stop = output.stop.clone();
    output.worker = Some(thread::spawn(move || {
        let mut paster = LinuxPaster::new(worker_stop.clone(), paste_with_shift, wayland_modifiers);
        let mut last_dictation: Option<String> = None;
        while let Ok(job) = job_receiver.recv() {
            if worker_stop.load(Ordering::Acquire) {
                break;
            }
            let job = match job {
                OutputJob::Dictation(job) => job,
                OutputJob::PasteLast => {
                    let result = last_dictation
                        .clone()
                        .ok_or_else(|| eyre!("there is no completed dictation to paste"))
                        .and_then(|text| {
                            paster
                                .as_mut()
                                .map_err(|error| eyre!("{error:#}"))?
                                .paste(&text)
                        })
                        .map(|()| String::new())
                        .map_err(|error| format!("{error:#}"));
                    if result_sender.send((OutputKind::PasteLast, result)).is_err() {
                        break;
                    }
                    continue;
                }
            };
            let started = Instant::now();
            let result = (|| -> Result<String> {
                let transcribed = job.transcript.wait().map_err(|error| eyre!(error))?;
                let inference_ms = started.elapsed().as_millis() as u64;
                tracing::info!(
                    source = transcribed.source.label(),
                    finish_ms = transcribed.finish_ms,
                    audio_ms = job.audio_ms,
                    "transcribed Linux dictation"
                );
                let text = transcribed.text;
                if text.is_empty() {
                    return Err(eyre!("transcription was empty"));
                }
                let paster = paster.as_mut().map_err(|error| eyre!("{error:#}"))?;
                output_settings.deliver(text, paster, job.audio_ms, inference_ms, started)
            })()
            .map_err(|error| format!("{error:#}"));
            tracing::info!(
                audio_ms = job.audio_ms,
                elapsed_ms = started.elapsed().as_millis(),
                "completed Linux dictation job"
            );
            if let Ok(text) = &result {
                last_dictation = Some(text.clone());
            }
            if result_sender.send((OutputKind::Dictation, result)).is_err() {
                break;
            }
        }
    }));

    let mut events = EventLog::create(event_path)?;
    let mut capture = DictationCapture::new(input.sample_rate);
    let mut captured_through = CaptureInstant::ZERO;
    let mut finishing: Option<Finishing> = None;
    let mut pending = 0_usize;
    events.emit(&VoiceEvent::SessionStarted {
        timestamp_ms: now_ms(),
    })?;
    emit_state(&mut events, VoiceState::Listening, &input.device_name)?;
    println!(
        "HEX dictation is ready on {}. Hold {} to dictate; Ctrl-C stops.",
        input.device_name, hotkey_label
    );

    while !shutdown.load(Ordering::Relaxed) {
        if let Ok(error) = hotkey.errors.try_recv() {
            return Err(eyre!("Linux hotkey monitor stopped: {error}"));
        }
        // A release keeps recording through its tail; shortcut events wait,
        // so a press inside the tail starts after this clip is submitted.
        if let Some(release) = &finishing
            && (captured_through >= release.ended_at + POST_RELEASE_TAIL
                || Instant::now() >= release.deadline)
        {
            let ended_at = release.ended_at;
            finishing = None;
            submit_capture(
                &mut capture,
                ended_at,
                &engine,
                &jobs,
                &mut events,
                &mut pending,
            )?;
            emit_state(
                &mut events,
                active_state(capture.is_recording(), pending),
                &input.device_name,
            )?;
        }
        while finishing.is_none()
            && let Ok(action) = hotkey.events.try_recv()
        {
            match action {
                HotkeyEvent::Start if !capture.is_recording() => {
                    start_capture(&mut capture, captured_through, feedback::play);
                    stream_when_due(&mut capture, &engine, toggle);
                    events.dictation(DictationPhase::Started, "")?;
                    emit_state(&mut events, VoiceState::Dictating, &input.device_name)?;
                }
                HotkeyEvent::Finish if capture.is_recording() => {
                    finishing = Some(begin_finishing(&capture, captured_through));
                }
                HotkeyEvent::PasteLast if !capture.is_recording() => {
                    match jobs.try_send(OutputJob::PasteLast) {
                        Ok(()) | Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => {
                            return Err(eyre!("dictation worker is unavailable"));
                        }
                    }
                }
                HotkeyEvent::Cancel if capture.is_recording() => {
                    capture.cancel();
                    feedback::play(Tone::Cancel);
                    events.dictation(DictationPhase::Cancelled, "")?;
                    emit_state(
                        &mut events,
                        active_state(capture.is_recording(), pending),
                        &input.device_name,
                    )?;
                }
                _ => {}
            }
        }
        while let Ok((kind, result)) = results.try_recv() {
            // A repeated paste has no transcript and is not pending work.
            if kind == OutputKind::PasteLast {
                if let Err(error) = result {
                    events.dictation(DictationPhase::Failed(error), "")?;
                }
                continue;
            }
            pending = pending.saturating_sub(1);
            match result {
                Ok(text) => {
                    events.emit(&VoiceEvent::Transcript {
                        timestamp_ms: now_ms(),
                        phase: TranscriptPhase::Completed,
                        latency_ms: 0,
                        text: text.clone(),
                    })?;
                    events.dictation(DictationPhase::Pasted, text)?;
                }
                Err(error) => events.dictation(DictationPhase::Failed(error), "")?,
            }
            emit_state(
                &mut events,
                active_state(capture.is_recording(), pending),
                &input.device_name,
            )?;
        }

        indicator.update(capture.is_recording(), pending);
        let chunk = match input.recv_timeout(UPDATE_INTERVAL) {
            AudioInputEvent::Chunk {
                samples,
                captured_through: chunk_captured_through,
            } => {
                captured_through = chunk_captured_through;
                samples
            }
            AudioInputEvent::Timeout => continue,
            AudioInputEvent::StreamFailed(error) => {
                return Err(eyre!("microphone stream stopped: {error}"));
            }
        };
        capture.ingest(&chunk, captured_through);
        capture.become_intentional(captured_through);
        stream_when_due(&mut capture, &engine, toggle);
    }

    if capture.is_recording() {
        capture.cancel();
        events.dictation(DictationPhase::Cancelled, "")?;
    }
    indicator.update(false, pending);
    emit_state(&mut events, VoiceState::Stopping, &input.device_name)?;
    output.stop.store(true, Ordering::Release);
    drop(hotkey);
    drop(input);
    drop(jobs);
    if output
        .worker
        .take()
        .expect("dictation output worker exists")
        .join()
        .is_err()
    {
        return Err(eyre!("Linux dictation worker panicked"));
    }
    println!("Stopped.");
    Ok(())
}

/// Applies the saved sound settings and preloads the tones.
pub(crate) fn initialize_feedback(settings: &crate::app_settings::AppSettings) {
    feedback::set_enabled(settings.sound_effects);
    feedback::set_volume(settings.sound_effect_volume.clamp(0.0, 1.0));
    if let Err(error) = feedback::preload() {
        tracing::warn!(%error, "recording sounds are unavailable; continuing without feedback");
    }
}

fn start_capture(
    capture: &mut DictationCapture,
    started_at: CaptureInstant,
    play: impl FnOnce(Tone),
) {
    capture.start(started_at);
    play(Tone::DictationStart);
}

/// Streams once the hold counts, so short taps and chords never reach the
/// model; a Toggle press is deliberate and streams at once.
fn stream_when_due(capture: &mut DictationCapture, engine: &TranscriptionEngine, toggle: bool) {
    if capture.is_recording() && (toggle || capture.is_intentional()) {
        capture.begin_stream(engine);
    }
}

fn begin_finishing(capture: &DictationCapture, ended_at: CaptureInstant) -> Finishing {
    let now = Instant::now();
    Finishing {
        ended_at,
        // A short tap is discarded on its release alone and needs no tail.
        deadline: if capture.would_transcribe(ended_at) {
            now + POST_RELEASE_TAIL + TAIL_STALL_GRACE
        } else {
            now
        },
    }
}

fn submit_capture(
    capture: &mut DictationCapture,
    ended_at: CaptureInstant,
    engine: &TranscriptionEngine,
    jobs: &SyncSender<OutputJob>,
    events: &mut EventLog,
    pending: &mut usize,
) -> Result<()> {
    match capture.finish_with_tail(ended_at, POST_RELEASE_TAIL) {
        Finish::Discard => events.dictation(DictationPhase::Discarded, "")?,
        Finish::Transcribe(clip) if clip.is_near_silent() => {
            events.dictation(DictationPhase::Discarded, "")?
        }
        Finish::Transcribe(mut clip) => {
            feedback::play(Tone::DictationStop);
            let audio_ms = clip.duration_ms();
            let job = Job {
                transcript: engine.finish(clip.take_stream(), clip.into_transcription_samples())?,
                audio_ms,
            };
            match jobs.try_send(OutputJob::Dictation(job)) {
                Ok(()) => {
                    *pending += 1;
                    events.dictation(DictationPhase::Transcribing, "")?;
                }
                Err(TrySendError::Full(_)) => events
                    .dictation(DictationPhase::Failed("dictation queue is full".into()), "")?,
                Err(TrySendError::Disconnected(_)) => {
                    return Err(eyre!("dictation worker is unavailable"));
                }
            }
        }
    }
    Ok(())
}

fn active_state(recording: bool, pending: usize) -> VoiceState {
    if recording {
        VoiceState::Dictating
    } else if pending > 0 {
        VoiceState::Transcribing
    } else {
        VoiceState::Listening
    }
}

fn emit_state(events: &mut EventLog, state: VoiceState, device: &str) -> Result<()> {
    events.emit(&VoiceEvent::State {
        timestamp_ms: now_ms(),
        state,
        device: device.into(),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription_engine::Command;

    fn capture_time() -> CaptureInstant {
        CaptureInstant::from_nanos(60_000_000_000)
    }

    fn event_log() -> (EventLog, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "hex-linux-dictation-{}-{}.ndjson",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (EventLog::create(&path).unwrap(), path)
    }

    /// Records `seconds` of speech-level audio in 100 ms chunks, streaming
    /// as the live loop would.
    fn record(
        capture: &mut DictationCapture,
        engine: &TranscriptionEngine,
        toggle: bool,
        seconds: u64,
    ) -> CaptureInstant {
        let start = capture_time();
        start_capture(capture, start, |_| {});
        stream_when_due(capture, engine, toggle);
        let mut through = start;
        for _ in 0..seconds * 10 {
            through = through + Duration::from_millis(100);
            let tone = (0..1_600)
                .map(|index| 0.1 * (index as f32 * 0.05).sin())
                .collect::<Vec<_>>();
            capture.ingest(&tone, through);
            capture.become_intentional(through);
            stream_when_due(capture, engine, toggle);
        }
        through
    }

    #[test]
    fn start_sound_precedes_audio_and_does_not_change_short_tap_discard() {
        let mut capture = DictationCapture::new(16_000);
        let started_at = capture_time();
        let mut sounds = Vec::new();

        start_capture(&mut capture, started_at, |tone| sounds.push(tone));

        assert!(capture.is_recording());
        assert_eq!(sounds, [Tone::DictationStart]);
        assert!(matches!(
            capture.finish(started_at + Duration::from_millis(100)),
            Finish::Discard
        ));
    }

    #[test]
    fn hold_streams_after_the_threshold_and_toggle_streams_from_the_press() {
        for toggle in [false, true] {
            let (engine, commands) = TranscriptionEngine::for_test();
            let mut capture = DictationCapture::new(16_000);
            let start = capture_time();
            start_capture(&mut capture, start, |_| {});
            stream_when_due(&mut capture, &engine, toggle);
            assert_eq!(capture.is_streaming(), toggle);
            capture.ingest(&[0.1; 1_600], start + Duration::from_millis(100));
            capture.become_intentional(start + Duration::from_millis(100));
            stream_when_due(&mut capture, &engine, toggle);
            assert_eq!(capture.is_streaming(), toggle, "toggle={toggle}");
            capture.ingest(&[0.1; 3_200], start + MINIMUM_HOLD);
            capture.become_intentional(start + MINIMUM_HOLD);
            stream_when_due(&mut capture, &engine, toggle);
            assert!(capture.is_streaming());
            let begun = commands
                .try_iter()
                .filter(|command| matches!(command, Command::Begin { .. }))
                .count();
            assert_eq!(begun, 1);
        }
    }

    const MINIMUM_HOLD: Duration = crate::dictation::MINIMUM_HOLD_DURATION;

    #[test]
    fn a_release_finishes_its_stream_with_the_complete_clip() {
        let (engine, commands) = TranscriptionEngine::for_test();
        let (mut events, path) = event_log();
        let (jobs, queued) = mpsc::sync_channel(2);
        let mut capture = DictationCapture::new(16_000);
        let released = record(&mut capture, &engine, false, 2);
        let mut pending = 0;
        // The release waits for its tail before it is submitted.
        let finishing = begin_finishing(&capture, released.checked_sub(POST_RELEASE_TAIL).unwrap());
        assert!(finishing.deadline > Instant::now());
        submit_capture(
            &mut capture,
            finishing.ended_at,
            &engine,
            &jobs,
            &mut events,
            &mut pending,
        )
        .unwrap();
        assert_eq!(pending, 1);
        let Ok(OutputJob::Dictation(job)) = queued.try_recv() else {
            panic!("a dictation job");
        };
        assert_eq!(job.audio_ms, 2_000);
        let received = commands.try_iter().collect::<Vec<_>>();
        let fed: usize = received
            .iter()
            .map(|command| match command {
                Command::Feed { samples, .. } => samples.len(),
                _ => 0,
            })
            .sum();
        assert!(matches!(
            received.first(),
            Some(Command::Begin { stream: 1 })
        ));
        assert!(matches!(
            received.last(),
            Some(Command::Finish { stream: Some(1), clip, .. }) if clip.len() == 32_000
        ));
        assert!(fed > 16_000 && fed <= 32_000);
        assert!(
            !received
                .iter()
                .any(|command| matches!(command, Command::Cancel { .. }))
        );
        drop(events);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn escape_and_short_taps_cancel_the_stream_and_submit_nothing() {
        let (engine, commands) = TranscriptionEngine::for_test();
        let (mut events, path) = event_log();
        let (jobs, queued) = mpsc::sync_channel(2);
        let mut pending = 0;

        let mut cancelled = DictationCapture::new(16_000);
        record(&mut cancelled, &engine, true, 1);
        cancelled.cancel();

        let mut short = DictationCapture::new(16_000);
        let start = capture_time();
        start_capture(&mut short, start, |_| {});
        stream_when_due(&mut short, &engine, true);
        short.ingest(&[0.1; 1_600], start + Duration::from_millis(100));
        let finishing = begin_finishing(&short, start + Duration::from_millis(100));
        assert!(
            finishing.deadline <= Instant::now(),
            "a short tap needs no tail"
        );
        submit_capture(
            &mut short,
            finishing.ended_at,
            &engine,
            &jobs,
            &mut events,
            &mut pending,
        )
        .unwrap();

        assert_eq!(pending, 0);
        assert!(queued.try_recv().is_err());
        let received = commands.try_iter().collect::<Vec<_>>();
        let count = |matches: fn(&Command) -> bool| received.iter().filter(|c| matches(c)).count();
        assert_eq!(count(|c| matches!(c, Command::Begin { .. })), 2);
        assert_eq!(count(|c| matches!(c, Command::Cancel { .. })), 2);
        assert_eq!(count(|c| matches!(c, Command::Finish { .. })), 0);
        drop(events);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_model_that_cannot_stream_transcribes_the_whole_clip_after_release() {
        let (engine, commands) = TranscriptionEngine::for_test_without_streaming();
        let (mut events, path) = event_log();
        let (jobs, queued) = mpsc::sync_channel(2);
        let mut capture = DictationCapture::new(16_000);
        let released = record(&mut capture, &engine, true, 2);
        assert!(!capture.is_streaming());
        let mut pending = 0;
        submit_capture(
            &mut capture,
            released.checked_sub(POST_RELEASE_TAIL).unwrap(),
            &engine,
            &jobs,
            &mut events,
            &mut pending,
        )
        .unwrap();
        assert!(queued.try_recv().is_ok());
        assert!(matches!(
            commands.try_iter().collect::<Vec<_>>().as_slice(),
            [Command::Finish { stream: None, clip, .. }] if clip.len() == 32_000
        ));
        drop(events);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cancelled_capture_keeps_pending_output_visible() {
        assert_eq!(active_state(true, 2), VoiceState::Dictating);
        assert_eq!(active_state(false, 2), VoiceState::Transcribing);
        assert_eq!(active_state(false, 1), VoiceState::Transcribing);
        assert_eq!(active_state(false, 0), VoiceState::Listening);
        assert_eq!(active_state(true, 0), VoiceState::Dictating);
    }

    #[test]
    fn output_guard_cancels_and_joins_even_on_listener_error() {
        let stopped = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let result: Result<()> = {
            let mut output = OutputWorker {
                stop: stopped.clone(),
                worker: None,
            };
            let (_jobs, receiver) = mpsc::channel::<()>();
            let stop = stopped.clone();
            let completed = finished.clone();
            output.worker = Some(thread::spawn(move || {
                // The sender must close before the guard joins this worker.
                assert!(receiver.recv().is_err());
                while !stop.load(Ordering::Acquire) {
                    thread::yield_now();
                }
                completed.store(true, Ordering::Release);
            }));
            Err(eyre!("listener failure"))
        };
        assert!(result.is_err());
        assert!(stopped.load(Ordering::Acquire));
        assert!(finished.load(Ordering::Acquire));
    }
}
