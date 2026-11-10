//! Motoare de transcriere pe sherpa-onnx.

use anyhow::{bail, Context};
use std::io::Write;
use std::path::{Path, PathBuf};

use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
    OfflineWhisperModelConfig,
};
use vf_application::{SpeechToText, SAMPLE_RATE};
use vf_domain::{Dictionary, EngineKind, ModelFamily, ModelSpec};

pub struct SherpaEngine {
    name: String,
    recognizer: OfflineRecognizer,
    // Fisierul de hotwords trebuie sa existe cat traieste recunoscatorul.
    _hotwords: Option<tempfile::NamedTempFile>,
}

pub struct EngineOptions<'a> {
    pub kind: EngineKind,
    pub models_dir: &'a Path,
    pub threads: i32,
    /// Daca e dat si modelul suporta, termenii devin hotwords (modified_beam_search).
    pub dictionary: Option<&'a Dictionary>,
    pub hotwords_score: f32,
    /// Limba pentru Whisper; gol = detectie automata.
    pub whisper_language: &'a str,
}

/// Folderul modelului pentru un tip de motor.
pub fn model_dir(kind: EngineKind, models_dir: &Path) -> PathBuf {
    ModelSpec::of(kind).path(models_dir)
}

impl SherpaEngine {
    pub fn new(opts: EngineOptions) -> anyhow::Result<Self> {
        let spec = ModelSpec::of(opts.kind);
        let dir = spec.path(opts.models_dir);
        let file = |suffix: &str| -> anyhow::Result<String> {
            let name = spec
                .file(suffix)
                .with_context(|| format!("{} has no {suffix} file", spec.title))?;
            let p = dir.join(name);
            if !p.exists() {
                bail!(
                    "The model {} is not installed: {} is missing (install it from the Models section)",
                    spec.title,
                    p.display()
                );
            }
            Ok(p.to_string_lossy().into_owned())
        };

        let mut cfg = OfflineRecognizerConfig::default();
        cfg.model_config.tokens = Some(file("tokens.txt")?);
        cfg.model_config.num_threads = opts.threads;
        cfg.decoding_method = Some("greedy_search".into());
        let mut name = spec.title.to_string();
        let mut hotwords_file = None;

        match spec.family {
            ModelFamily::Whisper => {
                cfg.model_config.whisper = OfflineWhisperModelConfig {
                    encoder: Some(file("encoder.int8.onnx")?),
                    decoder: Some(file("decoder.int8.onnx")?),
                    language: Some(opts.whisper_language.to_string()),
                    task: Some("transcribe".into()),
                    tail_paddings: -1,
                    ..Default::default()
                };
                if !opts.whisper_language.is_empty() {
                    name.push_str(&format!(" ({})", opts.whisper_language));
                }
            }
            ModelFamily::NemoTransducer => {
                cfg.model_config.transducer = OfflineTransducerModelConfig {
                    encoder: Some(file("encoder.int8.onnx")?),
                    decoder: Some(file("decoder.int8.onnx")?),
                    joiner: Some(file("joiner.int8.onnx")?),
                };
                cfg.model_config.model_type = Some("nemo_transducer".into());

                // Hotwords: au nevoie de vocabularul BPE al modelului (bpe.vocab).
                let bpe_vocab = dir.join("bpe.vocab");
                if let Some(dict) = opts.dictionary.filter(|d| !d.is_empty()) {
                    if bpe_vocab.exists() {
                        let mut f = tempfile::Builder::new()
                            .prefix("vf-hotwords")
                            .suffix(".txt")
                            .tempfile()?;
                        f.write_all(dict.hotwords().as_bytes())?;
                        f.flush()?;
                        cfg.decoding_method = Some("modified_beam_search".into());
                        cfg.max_active_paths = 4;
                        cfg.hotwords_file = Some(f.path().to_string_lossy().into_owned());
                        cfg.hotwords_score = opts.hotwords_score;
                        cfg.model_config.modeling_unit = Some("bpe".into());
                        cfg.model_config.bpe_vocab =
                            Some(bpe_vocab.to_string_lossy().into_owned());
                        name.push_str(" + hotwords");
                        hotwords_file = Some(f);
                    }
                }
            }
        }

        let recognizer = OfflineRecognizer::create(&cfg)
            .with_context(|| format!("Could not load the model from {}", dir.display()))?;
        Ok(Self {
            name,
            recognizer,
            _hotwords: hotwords_file,
        })
    }
}

impl SpeechToText for SherpaEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, samples);
        self.recognizer.decode(&stream);
        let result = stream
            .get_result()
            .context("sherpa-onnx returned no result")?;
        Ok(result.text.trim().to_string())
    }
}
