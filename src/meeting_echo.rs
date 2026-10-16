//! Signal-level speaker echo detection. When a call plays through speakers,
//! the microphone hears the remote audio tens of milliseconds after the
//! system track carries it. Per-20 ms frame energies of both tracks on one
//! timeline let a microphone segment be classified as bleed (explained by
//! delayed, attenuated system audio) or as local speech, which survives
//! double-talk because it is louder than the predicted bleed.

use serde::{Deserialize, Serialize};

use super::MeetingSource;

const FRAME_MS: u64 = 20;
const SILENT_DB: f32 = -100.0;
/// A source whose next packet starts further than this from where the last
/// one ended is re-anchored instead of appended.
const DISCONTINUITY_MS: u64 = 40;
/// Lag search, in frames, of the microphone behind the system track. The
/// small negative margin absorbs arrival-clock jitter between the sources.
const LAG_MIN_FRAMES: i64 = -5;
const LAG_MAX_FRAMES: i64 = 20;
/// Bleed gain and the noise floor are estimated over this much audio on
/// each side of a segment.
const CONTEXT_MS: u64 = 5_000;
/// Whether and how late the microphone hears the call is estimated over
/// this much audio on each side; it changes only with the user's setup.
const COUPLING_CONTEXT_MS: u64 = 60_000;
/// Remote speech in the system track; digital silence is far below this.
const SYSTEM_ACTIVE_DB: f32 = -55.0;
const MIN_SYSTEM_FRAMES: usize = 25;
/// Frames this far above the microphone noise floor carry sound.
const ACTIVE_ABOVE_FLOOR_DB: f32 = 6.0;
const GAIN_PERCENTILE: f32 = 0.35;
/// Frames this far above the predicted bleed are local speech.
const LOCAL_MARGIN_DB: f32 = 9.0;
/// Reverberation keeps bleed audible after the system frame that caused it.
const REVERB_FRAMES: i64 = 8;
/// Below this envelope correlation the microphone does not hear the call.
const MIN_CORRELATION: f32 = 0.2;
const MIN_ACTIVE_FRAMES: usize = 10;
/// A segment is bleed when at most this share of its sound is local.
const BLEED_MAX_LOCAL: f32 = 0.2;
/// A segment is local speech when at least this share of its sound is local.
const LOCAL_MIN_LOCAL: f32 = 0.5;

/// What the signal says about one microphone segment.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Signal {
    /// Delayed system audio explains the microphone sound.
    Bleed,
    /// The microphone carries sound the system audio does not explain.
    Local,
}

#[derive(Debug, Default)]
struct Track {
    /// dB per frame; frame `i` starts at `i * FRAME_MS` on the timeline.
    frames: Vec<f32>,
    anchor_ms: u64,
    rate: u32,
    samples_since_anchor: u64,
    sum: f64,
    count: usize,
}

impl Track {
    fn expected_ms(&self) -> u64 {
        self.anchor_ms + self.samples_since_anchor * 1_000 / u64::from(self.rate.max(1))
    }

    fn push(&mut self, start_ms: u64, samples: &[f32], rate: u32) {
        if rate == 0 || samples.is_empty() {
            return;
        }
        if self.rate != rate || self.expected_ms().abs_diff(start_ms) > DISCONTINUITY_MS {
            self.anchor_ms = start_ms;
            self.rate = rate;
            self.samples_since_anchor = 0;
            self.sum = 0.0;
            self.count = 0;
        }
        let frame_len = (rate as usize * FRAME_MS as usize / 1_000).max(1);
        for &sample in samples {
            self.sum += f64::from(sample) * f64::from(sample);
            self.count += 1;
            self.samples_since_anchor += 1;
            if self.count == frame_len {
                let frame_start_ms = self.anchor_ms
                    + (self.samples_since_anchor - frame_len as u64) * 1_000 / u64::from(rate);
                let index = (frame_start_ms / FRAME_MS) as usize;
                if self.frames.len() <= index {
                    self.frames.resize(index + 1, SILENT_DB);
                }
                self.frames[index] = (10.0 * (self.sum / frame_len as f64 + 1e-10).log10()) as f32;
                self.sum = 0.0;
                self.count = 0;
            }
        }
    }

    fn at(&self, frame: i64) -> f32 {
        usize::try_from(frame)
            .ok()
            .and_then(|frame| self.frames.get(frame))
            .copied()
            .unwrap_or(SILENT_DB)
    }
}

/// Frame energies of both meeting tracks on one millisecond timeline.
#[derive(Debug, Default)]
pub(super) struct EchoEnvelope {
    tracks: [Track; 2],
}

impl EchoEnvelope {
    /// Appends audio that starts at `start_ms` on the timeline.
    pub(super) fn push(
        &mut self,
        source: MeetingSource,
        start_ms: u64,
        samples: &[f32],
        rate: u32,
    ) {
        self.tracks[index(source)].push(start_ms, samples, rate);
    }

    /// Forgets one source, before rebuilding it from its recording.
    pub(super) fn clear(&mut self, source: MeetingSource) {
        self.tracks[index(source)] = Track::default();
    }

    /// Classifies the microphone between `start_ms` and `end_ms`, or `None`
    /// when there is too little sound or remote audio to tell.
    pub(super) fn assess(&self, start_ms: u64, end_ms: u64) -> Option<Signal> {
        let share = self.local_share(start_ms, end_ms)?;
        if share <= BLEED_MAX_LOCAL {
            Some(Signal::Bleed)
        } else if share >= LOCAL_MIN_LOCAL {
            Some(Signal::Local)
        } else {
            None
        }
    }

    /// The share of the segment's sound frames that delayed system audio
    /// does not explain, or `None` when the segment is nearly silent.
    pub(super) fn local_share(&self, start_ms: u64, end_ms: u64) -> Option<f32> {
        let [system, microphone] = &self.tracks;
        let first = (start_ms / FRAME_MS) as i64;
        let last = ((end_ms / FRAME_MS) as i64).max(first + 1);
        let around = |margin_ms: u64| {
            let start = (start_ms.saturating_sub(margin_ms) / FRAME_MS) as i64;
            let end = (((end_ms + margin_ms) / FRAME_MS) as i64)
                .min(microphone.frames.len() as i64)
                .max(last);
            start..end
        };
        let context = around(CONTEXT_MS);
        let coupling = around(COUPLING_CONTEXT_MS);

        let mut best: Option<(f32, i64)> = None;
        for lag in LAG_MIN_FRAMES..=LAG_MAX_FRAMES {
            let pairs = coupling
                .clone()
                .map(|frame| (microphone.at(frame), system.at(frame - lag)))
                .filter(|(_, system)| *system > SYSTEM_ACTIVE_DB)
                .collect::<Vec<_>>();
            if pairs.len() < MIN_SYSTEM_FRAMES {
                continue;
            }
            let correlation = pearson(&pairs);
            if best.is_none_or(|(best, _)| correlation > best) {
                best = Some((correlation, lag));
            }
        }

        let mut floor = context
            .clone()
            .map(|frame| microphone.at(frame))
            .collect::<Vec<_>>();
        let floor = percentile(&mut floor, 0.1)?;
        let active = (first..last)
            .filter(|&frame| microphone.at(frame) >= floor + ACTIVE_ABOVE_FLOOR_DB)
            .collect::<Vec<_>>();
        if active.len() < MIN_ACTIVE_FRAMES {
            return None;
        }
        // Without remote audio nearby, or when the microphone does not hear
        // it, nothing can explain the sound.
        let Some((_, lag)) = best.filter(|(correlation, _)| *correlation >= MIN_CORRELATION) else {
            return Some(1.0);
        };

        let mut loud = context
            .clone()
            .map(|frame| system.at(frame - lag))
            .filter(|level| *level > SYSTEM_ACTIVE_DB)
            .collect::<Vec<_>>();
        let loud_threshold = percentile(&mut loud, 0.5)?;
        let mut differences = context
            .clone()
            .filter(|&frame| system.at(frame - lag) >= loud_threshold)
            .map(|frame| microphone.at(frame) - system.at(frame - lag))
            .collect::<Vec<_>>();
        // A low percentile, so the user's own speech over loud remote frames
        // does not inflate the bleed estimate.
        let gain = percentile(&mut differences, GAIN_PERCENTILE)?;

        let local = active
            .iter()
            .filter(|&&frame| {
                let source = frame - lag;
                let remote = (source - REVERB_FRAMES..=source + 1)
                    .map(|frame| system.at(frame))
                    .fold(SILENT_DB, f32::max);
                microphone.at(frame) > (gain + remote).max(floor) + LOCAL_MARGIN_DB
            })
            .count();
        Some(local as f32 / active.len() as f32)
    }
}

fn index(source: MeetingSource) -> usize {
    match source {
        MeetingSource::System => 0,
        MeetingSource::Microphone => 1,
    }
}

fn pearson(pairs: &[(f32, f32)]) -> f32 {
    let count = pairs.len() as f32;
    let (mean_x, mean_y) = pairs
        .iter()
        .fold((0.0, 0.0), |(x, y), (a, b)| (x + a / count, y + b / count));
    let (mut covariance, mut variance_x, mut variance_y) = (0.0, 0.0, 0.0);
    for (x, y) in pairs {
        covariance += (x - mean_x) * (y - mean_y);
        variance_x += (x - mean_x) * (x - mean_x);
        variance_y += (y - mean_y) * (y - mean_y);
    }
    let denominator = (variance_x * variance_y).sqrt();
    if denominator <= f32::EPSILON {
        0.0
    } else {
        covariance / denominator
    }
}

fn percentile(values: &mut [f32], fraction: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let position = ((values.len() - 1) as f32 * fraction).round() as usize;
    Some(values[position])
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    /// Deterministic pseudo-speech: noise shaped by syllable-like bursts
    /// whose rhythm depends on `seed`.
    pub(crate) fn speech(seconds: f32, seed: u32) -> Vec<f32> {
        let mut state = seed.wrapping_mul(2_654_435_761).max(1);
        let mut noise = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let samples = (seconds * RATE as f32) as usize;
        let syllable = 0.11 + (seed % 7) as f32 * 0.013;
        (0..samples)
            .map(|index| {
                let time = index as f32 / RATE as f32;
                let phase = (time / syllable).fract();
                let word =
                    ((time / (syllable * (3.0 + (seed % 3) as f32))).floor() as u32 + seed) % 4;
                let envelope = if word == 0 {
                    0.02
                } else {
                    (phase * std::f32::consts::PI).sin()
                };
                noise() * 0.2 * envelope
            })
            .collect()
    }

    pub(crate) fn room_noise(samples: usize, seed: u32) -> Vec<f32> {
        let mut state = seed.wrapping_mul(747_796_405).max(1);
        (0..samples)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                ((state as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.001
            })
            .collect()
    }

    /// The microphone hears the system audio `delay_ms` later, attenuated.
    pub(crate) fn bleed(system: &[f32], delay_ms: usize, gain: f32) -> Vec<f32> {
        let delay = delay_ms * RATE as usize / 1_000;
        let mut microphone = room_noise(system.len(), 3);
        for (index, sample) in system.iter().enumerate() {
            if let Some(target) = microphone.get_mut(index + delay) {
                *target += sample * gain;
            }
        }
        microphone
    }

    pub(crate) fn envelope(system: &[f32], microphone: &[f32]) -> EchoEnvelope {
        let mut envelope = EchoEnvelope::default();
        // Packets of uneven size, as ScreenCaptureKit delivers them.
        for (source, samples) in [
            (MeetingSource::System, system),
            (MeetingSource::Microphone, microphone),
        ] {
            let mut offset = 0;
            for size in [1_024, 333, 2_048, 160].iter().cycle() {
                if offset >= samples.len() {
                    break;
                }
                let end = (offset + size).min(samples.len());
                envelope.push(
                    source,
                    offset as u64 * 1_000 / u64::from(RATE),
                    &samples[offset..end],
                    RATE,
                );
                offset = end;
            }
        }
        envelope
    }

    #[test]
    fn speaker_bleed_is_classified_as_bleed() {
        let system = speech(12.0, 11);
        let microphone = bleed(&system, 60, 0.08);
        let envelope = envelope(&system, &microphone);
        assert_eq!(envelope.assess(2_000, 5_000), Some(Signal::Bleed));
        assert_eq!(envelope.assess(6_000, 9_500), Some(Signal::Bleed));
    }

    #[test]
    fn talking_over_the_remote_side_is_local() {
        let system = speech(12.0, 11);
        let mut microphone = bleed(&system, 60, 0.08);
        let local = speech(3.0, 29);
        let start = 5 * RATE as usize;
        for (offset, sample) in local.iter().enumerate() {
            microphone[start + offset] += sample * 0.9;
        }
        let envelope = envelope(&system, &microphone);
        assert_eq!(envelope.assess(5_000, 8_000), Some(Signal::Local));
        assert_eq!(envelope.assess(1_000, 4_500), Some(Signal::Bleed));
    }

    #[test]
    fn speech_during_remote_silence_is_local() {
        let mut system = speech(12.0, 11);
        system[6 * RATE as usize..].fill(0.0);
        let mut microphone = bleed(&system, 60, 0.08);
        for (offset, sample) in speech(4.0, 5).iter().enumerate() {
            microphone[7 * RATE as usize + offset] += sample * 0.5;
        }
        let envelope = envelope(&system, &microphone);
        assert_eq!(envelope.assess(7_200, 10_800), Some(Signal::Local));
    }

    #[test]
    fn headphones_make_every_microphone_sound_local() {
        let system = speech(12.0, 11);
        let mut microphone = room_noise(system.len(), 3);
        for (offset, sample) in speech(3.0, 17).iter().enumerate() {
            microphone[4 * RATE as usize + offset] += sample * 0.4;
        }
        let envelope = envelope(&system, &microphone);
        assert_eq!(envelope.assess(4_000, 7_000), Some(Signal::Local));
        assert_eq!(envelope.assess(9_000, 11_000), None, "silence is undecided");
    }

    #[test]
    fn discontinuous_packets_are_placed_on_the_timeline() {
        let mut track = Track::default();
        track.push(0, &vec![0.1; 320], RATE);
        track.push(1_000, &vec![0.1; 320], RATE);
        assert_eq!(track.frames.len(), 51);
        assert!(track.frames[0] > -30.0 && track.frames[50] > -30.0);
        assert_eq!(track.frames[25], SILENT_DB);
    }
}
