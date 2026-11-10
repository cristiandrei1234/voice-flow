//! Intrarea dictarii: amplificarea vorbirii incete, pusa in fata motorului de transcriere.

use std::sync::atomic::{AtomicBool, Ordering};
use vf_application::SpeechToText;
use vf_domain::dictation::{apply_gain, quiet_gain};
use vf_domain::Settings;

/// Pornita / oprita din interfata fara sa reincarcam modelul.
static NORMALIZE_QUIET: AtomicBool = AtomicBool::new(true);

pub fn set_normalize_quiet(enabled: bool) {
    NORMALIZE_QUIET.store(enabled, Ordering::Relaxed);
}

/// Motor de transcriere care ridica mai intai nivelul inregistrarilor incete (soapta).
struct QuietBoost<T> {
    inner: T,
}

impl<T: SpeechToText> SpeechToText for QuietBoost<T> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        if NORMALIZE_QUIET.load(Ordering::Relaxed) {
            if let Some(gain) = quiet_gain(samples) {
                eprintln!("  (quiet speech: amplifying by {:.0} dB)", 20.0 * gain.log10());
                return self.inner.transcribe(&apply_gain(samples, gain));
            }
        }
        self.inner.transcribe(samples)
    }
}

/// Imbraca motorul cu amplificarea vorbirii incete, dupa setari.
pub fn wrap(engine: impl SpeechToText + 'static, s: &Settings) -> Box<dyn SpeechToText> {
    set_normalize_quiet(s.dictation.normalize_quiet_audio);
    Box::new(QuietBoost { inner: engine })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Tine minte varful audio primit.
    struct Probe(Mutex<f32>);
    impl SpeechToText for &'static Probe {
        fn name(&self) -> &str {
            "proba"
        }
        fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
            *self.0.lock().unwrap() = samples.iter().fold(0.0, |m, x| m.max(x.abs()));
            Ok(String::new())
        }
    }

    #[test]
    fn boosts_quiet_audio_only_when_enabled() {
        static PROBE: Probe = Probe(Mutex::new(0.0));
        let seen = || *PROBE.0.lock().unwrap();
        let quiet: Vec<f32> = (0..16_000).map(|i| 0.05 * (i as f32 * 0.1).sin()).collect();
        let loud: Vec<f32> = quiet.iter().map(|x| x * 10.0).collect();

        let mut s = Settings::default();
        let asr = wrap(&PROBE, &s);
        assert_eq!(asr.name(), "proba");
        asr.transcribe(&quiet).unwrap();
        assert!((seen() - 0.708).abs() < 0.01, "{}", seen());
        asr.transcribe(&loud).unwrap();
        assert!((seen() - 0.5).abs() < 0.01, "{}", seen());
        asr.transcribe(&vec![0.0; 16_000]).unwrap();
        assert_eq!(seen(), 0.0);

        set_normalize_quiet(false);
        asr.transcribe(&quiet).unwrap();
        assert!((seen() - 0.05).abs() < 0.001, "{}", seen());

        s.dictation.normalize_quiet_audio = false;
        let asr = wrap(&PROBE, &s);
        asr.transcribe(&quiet).unwrap();
        assert!((seen() - 0.05).abs() < 0.001, "{}", seen());
        set_normalize_quiet(true);
    }
}
