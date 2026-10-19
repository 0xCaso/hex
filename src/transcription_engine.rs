//! The thread that owns the loaded GGUF model and runs all of its inference.
//!
//! A capture that the model can stream is fed while the shortcut is held, so
//! release only decodes the last second or two of audio. Every finish carries
//! the complete clip: a stream that failed, stalled, or produced no text is
//! replaced by an offline transcription of that clip, and a model or device
//! that cannot stream always takes the offline path. Commands are handled in
//! order on one thread, because a live stream holds the model's compute lease.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use color_eyre::eyre::{Result, eyre};
use transcribe_cpp::CancelToken;

use crate::local_transcriber::LocalTranscriber;

/// A healthy finish decodes at most a couple of seconds of buffered audio.
/// Past this the stream is aborted and the clip is transcribed offline.
const STREAM_FINISH_DEADLINE: Duration = Duration::from_millis(1_500);
/// Audio fed past the clip's trimmed end (a release handled late) that the
/// stream may still keep. Beyond it, the offline clip is authoritative.
const MAX_OVERFED_SAMPLES: usize = 16_000 / 4;

enum Command {
    Begin {
        stream: u64,
    },
    Feed {
        stream: u64,
        samples: Vec<f32>,
    },
    Cancel {
        stream: u64,
    },
    Finish {
        stream: Option<u64>,
        clip: Vec<f32>,
        reply: Sender<Result<Transcribed, String>>,
    },
}

#[derive(Debug)]
pub struct Transcribed {
    pub text: String,
    pub source: Source,
    /// Time from the finish request to the final text.
    pub finish_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Stream,
    Offline,
    /// The stream could not be used; the reason is logged with the result.
    Fallback(String),
}

impl Source {
    pub fn label(&self) -> &str {
        match self {
            Self::Stream => "stream",
            Self::Offline => "offline",
            Self::Fallback(_) => "fallback",
        }
    }
}

pub struct TranscriptionEngine {
    commands: Option<Sender<Command>>,
    cancel: CancelToken,
    streams: bool,
    next_stream: u64,
    thread: Option<JoinHandle<()>>,
}

/// A live stream for one capture. Feed it the capture's 16 kHz audio in order,
/// then either [`TranscriptionEngine::finish`] or drop it to cancel.
pub struct LiveStream {
    id: u64,
    fed: usize,
    finished: bool,
    commands: Sender<Command>,
}

impl LiveStream {
    /// Total samples handed to the stream so far.
    pub fn fed(&self) -> usize {
        self.fed
    }

    pub fn feed(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        self.fed += samples.len();
        let _ = self.commands.send(Command::Feed {
            stream: self.id,
            samples: samples.to_vec(),
        });
    }
}

impl Drop for LiveStream {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.commands.send(Command::Cancel { stream: self.id });
        }
    }
}

/// A finish in flight. Waiting enforces the stream deadline from the caller's
/// side, so a stalled native stream cannot hold the paste forever.
pub struct PendingTranscript {
    reply: Receiver<Result<Transcribed, String>>,
    cancel: CancelToken,
    streamed: bool,
}

impl PendingTranscript {
    pub fn wait(self) -> Result<Transcribed, String> {
        if self.streamed {
            match self.reply.recv_timeout(STREAM_FINISH_DEADLINE) {
                Ok(result) => return result,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("transcription engine stopped".into());
                }
                Err(RecvTimeoutError::Timeout) => {
                    tracing::warn!("live stream missed its finish deadline; aborting it");
                    self.cancel.cancel();
                }
            }
        }
        self.reply
            .recv()
            .unwrap_or_else(|_| Err("transcription engine stopped".into()))
    }
}

impl TranscriptionEngine {
    pub fn start(transcriber: LocalTranscriber) -> Result<Self> {
        let cancel = CancelToken::new();
        let streams = transcriber.stream_options().is_some();
        let (commands, receiver) = mpsc::channel();
        let worker_cancel = cancel.clone();
        let thread = thread::Builder::new()
            .name("transcription-engine".into())
            .spawn(move || Worker::new(transcriber, worker_cancel).run(receiver))?;
        Ok(Self {
            commands: Some(commands),
            cancel,
            streams,
            next_stream: 0,
            thread: Some(thread),
        })
    }

    /// Opens a live stream when the loaded model can stream on this device.
    pub fn begin(&mut self) -> Option<LiveStream> {
        if !self.streams {
            return None;
        }
        let commands = self.commands.as_ref()?.clone();
        self.next_stream += 1;
        let id = self.next_stream;
        commands.send(Command::Begin { stream: id }).ok()?;
        Some(LiveStream {
            id,
            fed: 0,
            finished: false,
            commands,
        })
    }

    /// Finishes a capture with its complete 16 kHz clip. Without a stream,
    /// or when the stream is unusable, the clip is transcribed offline.
    pub fn finish(&self, stream: Option<LiveStream>, clip: Vec<f32>) -> Result<PendingTranscript> {
        let commands = self
            .commands
            .as_ref()
            .ok_or_else(|| eyre!("transcription engine stopped"))?;
        let (reply, receiver) = mpsc::channel();
        let streamed = stream.is_some();
        let stream = stream.map(|mut stream| {
            stream.finished = true;
            stream.id
        });
        commands
            .send(Command::Finish {
                stream,
                clip,
                reply,
            })
            .map_err(|_| eyre!("transcription engine stopped"))?;
        Ok(PendingTranscript {
            reply: receiver,
            cancel: self.cancel.clone(),
            streamed,
        })
    }
}

impl Drop for TranscriptionEngine {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.commands.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Worker {
    transcriber: LocalTranscriber,
    cancel: CancelToken,
}

/// How a live stream ended inside the worker.
enum StreamEnd {
    /// A finish for this stream: the final text, or why the stream is unusable.
    Finished {
        result: Result<String, String>,
        clip: Vec<f32>,
        reply: Sender<Result<Transcribed, String>>,
        started: Instant,
    },
    Cancelled,
    /// Another command arrived first; it is handled after the stream closes.
    Interrupted(Command),
    Disconnected,
}

impl Worker {
    fn new(transcriber: LocalTranscriber, cancel: CancelToken) -> Self {
        Self {
            transcriber,
            cancel,
        }
    }

    fn run(mut self, commands: Receiver<Command>) {
        let mut next = None;
        loop {
            let Some(command) = next.take().or_else(|| commands.recv().ok()) else {
                return;
            };
            match command {
                Command::Begin { stream } => {
                    next = self.stream(stream, &commands);
                }
                Command::Finish {
                    stream,
                    clip,
                    reply,
                } => {
                    let started = Instant::now();
                    let source = match stream {
                        // The stream was already closed: its begin failed or
                        // another command interrupted it.
                        Some(_) => Source::Fallback("the live stream was not available".into()),
                        None => Source::Offline,
                    };
                    self.offline(clip, source, started, reply);
                }
                // Stale input for a stream that already ended.
                Command::Feed { .. } | Command::Cancel { .. } => {}
            }
        }
    }

    /// Drives one live stream until it finishes, is cancelled, or another
    /// command interrupts it. Returns a command still to be handled.
    fn stream(&mut self, id: u64, commands: &Receiver<Command>) -> Option<Command> {
        let options = self.transcriber.stream_options().cloned()?;
        self.cancel.reset();
        let begun = Instant::now();
        let mut feed_ms = 0_u128;
        let mut feeds = 0_u32;
        let ended = self
            .transcriber
            .with_stream(&options, &self.cancel, |stream| {
                let mut failure: Option<String> = None;
                let mut fed = 0_usize;
                loop {
                    let Ok(command) = commands.recv() else {
                        return StreamEnd::Disconnected;
                    };
                    match command {
                        Command::Feed { stream: s, samples } if s == id => {
                            fed += samples.len();
                            if failure.is_none() {
                                let started = Instant::now();
                                if let Err(error) = stream.feed(&samples) {
                                    failure = Some(format!("stream feed failed: {error}"));
                                }
                                feed_ms += started.elapsed().as_millis();
                                feeds += 1;
                            }
                        }
                        Command::Feed { .. } => {}
                        Command::Cancel { stream: s } if s == id => return StreamEnd::Cancelled,
                        Command::Cancel { .. } => {}
                        Command::Finish {
                            stream: Some(s),
                            clip,
                            reply,
                        } if s == id => {
                            let started = Instant::now();
                            let result = match failure {
                                Some(failure) => Err(failure),
                                None => finish_stream(stream, &clip, fed),
                            };
                            return StreamEnd::Finished {
                                result,
                                clip,
                                reply,
                                started,
                            };
                        }
                        other => return StreamEnd::Interrupted(other),
                    }
                }
            });
        match ended {
            Ok(StreamEnd::Finished {
                result,
                clip,
                reply,
                started,
            }) => {
                tracing::info!(
                    audio_ms = clip.len() / 16,
                    stream_ms = begun.elapsed().as_millis(),
                    feeds,
                    feed_ms,
                    finish_ms = started.elapsed().as_millis(),
                    "finished live stream"
                );
                match result {
                    Ok(text) => {
                        let _ = reply.send(Ok(Transcribed {
                            text,
                            source: Source::Stream,
                            finish_ms: started.elapsed().as_millis() as u64,
                        }));
                    }
                    Err(reason) => {
                        tracing::warn!(%reason, "live stream unusable; transcribing the full clip");
                        self.offline(clip, Source::Fallback(reason), started, reply);
                    }
                }
                None
            }
            Ok(StreamEnd::Cancelled) | Ok(StreamEnd::Disconnected) => None,
            Ok(StreamEnd::Interrupted(command)) => Some(command),
            Err(error) => {
                tracing::warn!(%error, "could not begin a live stream");
                None
            }
        }
    }

    fn offline(
        &mut self,
        clip: Vec<f32>,
        source: Source,
        started: Instant,
        reply: Sender<Result<Transcribed, String>>,
    ) {
        self.cancel.reset();
        let mut samples = clip;
        crate::dictation::pad_for_parakeet(&mut samples);
        let result = self
            .transcriber
            .transcribe(&samples)
            .map(|text| Transcribed {
                text: text.trim().to_string(),
                source,
                finish_ms: started.elapsed().as_millis() as u64,
            })
            .map_err(|error| format!("{error:#}"));
        let _ = reply.send(result);
    }
}

/// Feeds whatever part of the clip the stream has not seen, then finalizes.
/// The clip is authoritative: a stream fed well past its trimmed end, or one
/// that produced nothing, is reported unusable so the caller falls back.
fn finish_stream(
    stream: &mut transcribe_cpp::Stream<'_>,
    clip: &[f32],
    fed: usize,
) -> Result<String, String> {
    if fed > clip.len() + MAX_OVERFED_SAMPLES {
        return Err(format!(
            "the stream received {} ms past the clip's end",
            (fed - clip.len()) / 16
        ));
    }
    if clip.len() > fed {
        stream
            .feed(&clip[fed..])
            .map_err(|error| format!("stream feed failed: {error}"))?;
    }
    stream
        .finalize()
        .map_err(|error| format!("stream finalize failed: {error}"))?;
    let text = stream.text().full.trim().to_string();
    if text.is_empty() {
        return Err("the stream returned no text".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_labels_distinguish_fallbacks() {
        assert_eq!(Source::Stream.label(), "stream");
        assert_eq!(Source::Offline.label(), "offline");
        assert_eq!(Source::Fallback("x".into()).label(), "fallback");
    }
}
