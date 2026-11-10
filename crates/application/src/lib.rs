//! Fluxul de dictare, independent de platforma.
//!
//! audio (16 kHz mono) -> transcriere -> dictionar -> curatare (optional) -> dictionar -> formatare -> livrare

pub mod command;
pub mod wer;

use std::time::{Duration, Instant};
use vf_domain::{styles, Dictionary};

pub const SAMPLE_RATE: u32 = 16_000;

/// Transcrie audio 16 kHz mono in text.
pub trait SpeechToText: Send + Sync {
    fn name(&self) -> &str;
    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String>;
}

impl SpeechToText for Box<dyn SpeechToText> {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        (**self).transcribe(samples)
    }
}

/// Curata textul (punctuatie, ezitari, termeni). Primeste termenii din dictionar ca indicii.
pub trait TextCleaner: Send + Sync {
    fn clean(&self, text: &str, terms: &[&str]) -> anyhow::Result<Cleaned>;
}

/// Textul curatat si cum se formateaza la final (depinde de stilul ales pentru dictarea asta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cleaned {
    pub text: String,
    /// `false`: o singura propozitie ramane fara punct la final (stilul "Messages").
    pub final_period: bool,
}

impl Cleaned {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            final_period: true,
        }
    }
}

/// Livreaza textul final (lipire in aplicatia activa, stdout...).
pub trait TextOutput: Send + Sync {
    fn deliver(&self, text: &str) -> anyhow::Result<()>;
}

#[derive(Debug, Clone)]
pub struct DictationResult {
    pub raw: String,
    pub text: String,
    pub audio: Duration,
    pub asr_time: Duration,
    pub cleanup_time: Option<Duration>,
    /// Daca s-a cerut curatare dar a esuat, motivul (textul livrat ramane cel necuratat).
    pub cleanup_error: Option<String>,
}

pub struct Dictation {
    pub asr: Box<dyn SpeechToText>,
    pub cleaner: Option<Box<dyn TextCleaner>>,
    pub dictionary: Dictionary,
    /// Sub aceasta durata inregistrarea e ignorata (apasari accidentale).
    pub min_audio: Duration,
}

impl Dictation {
    pub fn new(asr: Box<dyn SpeechToText>, dictionary: Dictionary) -> Self {
        Self {
            asr,
            cleaner: None,
            dictionary,
            min_audio: Duration::from_millis(300),
        }
    }

    pub fn with_cleaner(mut self, cleaner: Option<Box<dyn TextCleaner>>) -> Self {
        self.cleaner = cleaner;
        self
    }

    /// Proceseaza o inregistrare. `Ok(None)` daca e prea scurta sau nu s-a auzit nimic.
    pub fn process(&self, samples: &[f32]) -> anyhow::Result<Option<DictationResult>> {
        let audio = Duration::from_secs_f64(samples.len() as f64 / SAMPLE_RATE as f64);
        if audio < self.min_audio {
            return Ok(None);
        }

        let t = Instant::now();
        let raw = self.asr.transcribe(samples)?;
        let asr_time = t.elapsed();
        if raw.trim().is_empty() {
            return Ok(None);
        }

        let mut text = self.dictionary.apply(&raw);
        let mut cleanup_time = None;
        let mut cleanup_error = None;
        let mut final_period = true;
        if let Some(cleaner) = &self.cleaner {
            let t = Instant::now();
            match cleaner.clean(&text, &self.dictionary.terms()) {
                Ok(cleaned) if !cleaned.text.trim().is_empty() => {
                    text = self.dictionary.apply(&cleaned.text);
                    final_period = cleaned.final_period;
                }
                Ok(_) => cleanup_error = Some("empty answer".into()),
                Err(e) => cleanup_error = Some(e.to_string()),
            }
            cleanup_time = Some(t.elapsed());
        }

        Ok(Some(DictationResult {
            text: styles::finish_text(&text, final_period),
            raw,
            audio,
            asr_time,
            cleanup_time,
            cleanup_error,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeAsr(&'static str);
    impl SpeechToText for FakeAsr {
        fn name(&self) -> &str {
            "fake"
        }
        fn transcribe(&self, _: &[f32]) -> anyhow::Result<String> {
            Ok(self.0.into())
        }
    }

    struct FakeCleaner(Result<&'static str, &'static str>);
    impl TextCleaner for FakeCleaner {
        fn clean(&self, _: &str, terms: &[&str]) -> anyhow::Result<Cleaned> {
            assert!(terms.contains(&"NestJS"));
            self.0.map(Cleaned::new).map_err(|e| anyhow::anyhow!(e))
        }
    }

    fn secs(s: f32) -> Vec<f32> {
        vec![0.0; (SAMPLE_RATE as f32 * s) as usize]
    }

    fn dict() -> Dictionary {
        Dictionary::parse("NestJS | esges\ndeploy | de ploi\n")
    }

    #[test]
    fn applies_dictionary_and_format() {
        let d = Dictation::new(Box::new(FakeAsr("am făcut de ploi în esges")), dict());
        let r = d.process(&secs(1.0)).unwrap().unwrap();
        assert_eq!(r.raw, "am făcut de ploi în esges");
        assert_eq!(r.text, "Am făcut deploy în NestJS.");
        assert!(r.cleanup_time.is_none());
    }

    #[test]
    fn ignores_short_and_empty() {
        let d = Dictation::new(Box::new(FakeAsr("ceva")), dict());
        assert!(d.process(&secs(0.1)).unwrap().is_none());
        let d = Dictation::new(Box::new(FakeAsr("  ")), dict());
        assert!(d.process(&secs(1.0)).unwrap().is_none());
    }

    #[test]
    fn uses_cleaner_and_reapplies_dictionary() {
        let d = Dictation::new(Box::new(FakeAsr("ăăă esges e gata")), dict())
            .with_cleaner(Some(Box::new(FakeCleaner(Ok("esges e gata")))));
        let r = d.process(&secs(1.0)).unwrap().unwrap();
        assert_eq!(r.text, "NestJS e gata.");
        assert!(r.cleanup_error.is_none());
    }

    struct MessageCleaner(&'static str);
    impl TextCleaner for MessageCleaner {
        fn clean(&self, _: &str, _: &[&str]) -> anyhow::Result<Cleaned> {
            Ok(Cleaned {
                text: self.0.into(),
                final_period: false,
            })
        }
    }

    #[test]
    fn cleaner_decides_final_period_and_lines_survive() {
        let d = Dictation::new(Box::new(FakeAsr("ok ne vedem la cinci")), dict())
            .with_cleaner(Some(Box::new(MessageCleaner("Ok, ne vedem la cinci."))));
        assert_eq!(d.process(&secs(1.0)).unwrap().unwrap().text, "Ok, ne vedem la cinci");

        let d = Dictation::new(Box::new(FakeAsr("avem doua lucruri")), dict())
            .with_cleaner(Some(Box::new(FakeCleaner(Ok("Avem de făcut:\n1. de ploi\n2. teste")))));
        assert_eq!(
            d.process(&secs(1.0)).unwrap().unwrap().text,
            "Avem de făcut:\n1. deploy\n2. teste"
        );
    }

    #[test]
    fn falls_back_when_cleaner_fails() {
        let d = Dictation::new(Box::new(FakeAsr("esges e gata")), dict())
            .with_cleaner(Some(Box::new(FakeCleaner(Err("server oprit")))));
        let r = d.process(&secs(1.0)).unwrap().unwrap();
        assert_eq!(r.text, "NestJS e gata.");
        assert_eq!(r.cleanup_error.as_deref(), Some("server oprit"));
    }
}
