use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use color_eyre::eyre::{Result, WrapErr, eyre};
use transcribe_cpp::{Backend, Model, ModelOptions, RunOptions, TimestampKind};

use crate::gguf_session::OfflineGgufSession;
use crate::transcription_models::{
    ModelDefinition, TranscriptionModelId, TranscriptionSelection, definition,
    download_with_progress, model_path, validate,
};

pub struct LocalTranscriber {
    session: OfflineGgufSession,
    options: RunOptions,
    device_label: String,
    max_audio_samples: Option<usize>,
    #[cfg(target_os = "windows")]
    stream: Option<transcribe_cpp::StreamOptions>,
}

pub fn default_model() -> &'static ModelDefinition {
    definition(TranscriptionModelId::default())
}

pub fn install_default(canceled: &AtomicBool) -> Result<()> {
    install(&TranscriptionSelection::default(), canceled)
}

pub fn install(selection: &TranscriptionSelection, canceled: &AtomicBool) -> Result<()> {
    let model = validate(selection)?;
    let downloaded = AtomicU64::new(0);
    println!("Installing {} ({})...", model.name, model.size_label());
    let path = download_with_progress(model, canceled, &downloaded)?;
    println!(
        "Installed {} bytes at {}",
        downloaded.load(Ordering::Relaxed),
        path.display()
    );
    Ok(())
}

pub fn devices() -> Result<Vec<String>> {
    transcribe_cpp::init_logging();
    transcribe_cpp::init_backends_default()
        .wrap_err("could not initialize transcription backends")?;
    Ok(transcribe_cpp::devices()
        .into_iter()
        .map(|device| {
            format!(
                "{}\t{}\t{} MB\t{}",
                device
                    .index
                    .map_or_else(|| "-".into(), |index| index.to_string()),
                device.kind,
                device.memory_total / (1024 * 1024),
                if device.description.is_empty() {
                    device.name
                } else {
                    device.description
                }
            )
        })
        .collect())
}

pub fn transcribe_wav(path: &Path) -> Result<String> {
    transcribe_wav_with_selection(path, &TranscriptionSelection::default())
}

pub fn transcribe_wav_with_selection(
    path: &Path,
    selection: &TranscriptionSelection,
) -> Result<String> {
    let samples = read_wav_for_parakeet(path)?;
    LocalTranscriber::load(selection)?.transcribe(&samples)
}

pub fn read_wav_for_parakeet(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .wrap_err_with(|| format!("could not open {}", path.display()))?;
    let spec = reader.spec();
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => reader
            .samples::<i16>()
            .map(|sample| sample.map(|sample| sample as f32 / i16::MAX as f32))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => reader
            .samples::<i32>()
            .map(|sample| sample.map(|sample| sample as f32 / i32::MAX as f32))
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    let channels = usize::from(spec.channels);
    let mono = samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect::<Vec<_>>();
    let mut samples = crate::dictation::resample_for_parakeet(&mono, spec.sample_rate);
    crate::dictation::pad_for_parakeet(&mut samples);
    Ok(samples)
}

impl LocalTranscriber {
    pub fn load_default() -> Result<Self> {
        Self::load(&TranscriptionSelection::default())
    }

    pub fn load(selection: &TranscriptionSelection) -> Result<Self> {
        transcribe_cpp::init_logging();
        transcribe_cpp::init_backends_default()
            .wrap_err("could not initialize transcription backends")?;
        let definition = validate(selection)?;
        if !crate::transcription_models::is_installed(definition, &selection.language) {
            return Err(eyre!(
                "{} is not installed; run `cargo run -- model install`",
                definition.name
            ));
        }
        let path = model_path(definition)?;
        let model = Model::load_with(
            &path,
            &ModelOptions {
                backend: Backend::Auto,
                device: None,
            },
        )
        .wrap_err_with(|| format!("could not load transcription model from {}", path.display()))?;
        let device = model.device()?;
        let variant = model.variant();
        let architecture = model.arch();
        let capabilities = model.capabilities();
        let language = crate::gguf_session::validate_gguf_artifact(
            &path,
            definition,
            &architecture,
            &variant,
            selection,
            capabilities.supports_language_detect,
        )?;
        // Recognition hints softly prime Whisper's decoder as its initial
        // prompt; validate() already rejects hints on unsupported models.
        let recognition_hints = selection.recognition_hints.trim();
        let family =
            (definition.supports_recognition_hints && !recognition_hints.is_empty()).then(|| {
                transcribe_cpp::RunExtension::Whisper(transcribe_cpp::WhisperRunOptions {
                    initial_prompt: Some(recognition_hints.to_string()),
                    ..Default::default()
                })
            });
        let device_label = format!("{} ({})", device.description, device.kind);
        tracing::info!(
            backend = device.kind,
            device = device.description,
            variant,
            "loaded local transcription model"
        );
        let session = OfflineGgufSession::new(model)?;
        let mut transcriber = Self {
            session,
            options: RunOptions {
                timestamps: if capabilities.max_timestamp_kind == TimestampKind::None {
                    TimestampKind::None
                } else {
                    TimestampKind::Segment
                },
                language,
                family,
                ..Default::default()
            },
            device_label,
            max_audio_samples: crate::transcription_models::max_audio_chunk_samples(
                &architecture,
                capabilities.max_audio_ms,
            ),
            #[cfg(target_os = "windows")]
            stream: None,
        };
        let started = Instant::now();
        transcriber.transcribe(&vec![0.0; 24_000])?;
        tracing::info!(
            prewarm_ms = started.elapsed().as_millis(),
            "prewarmed local transcription model"
        );
        #[cfg(target_os = "windows")]
        if capabilities.supports_streaming {
            transcriber.stream = transcriber.probe_stream(&architecture);
        }
        Ok(transcriber)
    }

    /// Streams only on a GPU: a CPU stream re-encodes each overlapping window
    /// and would cost several times the offline compute. The probe also
    /// prewarms the stream graph. Without an extension Parakeet buffers with
    /// its largest left, chunk, and right context (Unified English: 5.6 s,
    /// 1.04 s, 1.04 s), its most accurate and least compute-intensive setting.
    /// `HEX_STREAMING=0` keeps the model offline; `HEX_STREAM_CHUNK_MS`
    /// overrides the chunk and right context for benchmarks.
    #[cfg(target_os = "windows")]
    fn probe_stream(&mut self, architecture: &str) -> Option<transcribe_cpp::StreamOptions> {
        use transcribe_cpp::DeviceType;
        if std::env::var("HEX_STREAMING").is_ok_and(|value| value == "0") {
            return None;
        }
        match self.session.device_type() {
            Ok(DeviceType::Gpu | DeviceType::Igpu) => {}
            _ => return None,
        }
        let chunk_ms = std::env::var("HEX_STREAM_CHUNK_MS")
            .ok()
            .and_then(|value| value.parse::<i32>().ok());
        let options = transcribe_cpp::StreamOptions {
            family: (architecture == "parakeet" && chunk_ms.is_some()).then_some(
                transcribe_cpp::StreamExtension::ParakeetBuffered(
                    transcribe_cpp::ParakeetBufferedStreamOptions {
                        left_ms: None,
                        chunk_ms,
                        right_ms: chunk_ms,
                    },
                ),
            ),
            ..Default::default()
        };
        let started = Instant::now();
        let probe = self.with_stream(&options, &transcribe_cpp::CancelToken::new(), |stream| {
            stream.feed(&vec![0.0; 16_000])?;
            stream.finalize().map(|_| ())
        });
        match probe {
            Ok(Ok(())) => {
                tracing::info!(
                    chunk_ms,
                    prewarm_ms = started.elapsed().as_millis(),
                    "streaming transcription is available"
                );
                Some(options)
            }
            Ok(Err(error)) | Err(error) => {
                tracing::warn!(%error, "model advertises streaming but could not stream");
                None
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn stream_options(&self) -> Option<&transcribe_cpp::StreamOptions> {
        self.stream.as_ref()
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn with_stream<R>(
        &mut self,
        stream_options: &transcribe_cpp::StreamOptions,
        cancel: &transcribe_cpp::CancelToken,
        drive: impl FnOnce(&mut transcribe_cpp::Stream<'_>) -> R,
    ) -> transcribe_cpp::Result<R> {
        // Decode exactly as offline; a run-slot family extension (Whisper's
        // prompt) does not apply to a stream.
        let options = RunOptions {
            family: None,
            ..self.options.clone()
        };
        self.session
            .with_stream(&options, stream_options, cancel, drive)
    }

    pub fn device_label(&self) -> &str {
        &self.device_label
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        let Some(max_samples) = self
            .max_audio_samples
            .filter(|&max_samples| samples.len() > max_samples)
        else {
            return self.run(samples);
        };
        let mut output = Vec::new();
        for samples in samples.chunks(max_samples) {
            let mut chunk = samples.to_vec();
            crate::dictation::pad_for_parakeet(&mut chunk);
            let text = self.run(&chunk)?;
            let text = text.trim();
            if !text.is_empty() {
                output.push(text.to_string());
            }
        }
        Ok(output.join(" "))
    }

    fn run(&mut self, samples: &[f32]) -> Result<String> {
        self.session
            .run(samples, &self.options)
            .map(|transcript| transcript.text)
            .wrap_err("transcription failed")
    }
}
