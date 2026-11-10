//! Partea de intrare a dictarii: setarile ei, limbile pentru Whisper si amplificarea vorbirii incete.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DictationSettings {
    /// In modul "tii apasat": doua atingeri rapide ale tastei blocheaza inregistrarea (fara maini).
    pub double_tap_lock: bool,
    /// Amplifica inregistrarile incete (vorbit in soapta) inainte de transcriere.
    pub normalize_quiet_audio: bool,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            double_tap_lock: true,
            normalize_quiet_audio: true,
        }
    }
}

/// Limbile oferite pentru Whisper; "" = detectie automata.
pub const WHISPER_LANGUAGES: [&str; 8] = ["", "ro", "en", "es", "fr", "de", "it", "pt"];

/// Curata si verifica o limba pentru Whisper. Intoarce codul de salvat sau mesajul de eroare.
pub fn parse_whisper_language(language: &str) -> Result<&'static str, String> {
    let wanted = language.trim().to_lowercase();
    let wanted = if wanted == "auto" { "" } else { wanted.as_str() };
    WHISPER_LANGUAGES
        .iter()
        .copied()
        .find(|l| *l == wanted)
        .ok_or_else(|| {
            format!(
                "The language \"{language}\" is not available for Whisper. Choose one of: auto, {}.",
                WHISPER_LANGUAGES[1..].join(", ")
            )
        })
}

/// Nivelul la care aducem varfurile unei inregistrari incete: aproximativ -3 dBFS.
pub const GAIN_TARGET: f32 = 0.708;
/// Amplificarea maxima: +30 dB.
pub const GAIN_MAX: f32 = 31.623;
/// Sub acest nivel (aprox. -46 dBFS) clipul e considerat liniste si nu e atins: altfel am ridica
/// zgomotul camerei pana la nivel de vorbire si motorul ar "auzi" cuvinte care nu exista.
pub const SILENCE_LEVEL: f32 = 0.005;
/// Peste acest nivel (aprox. -14 dBFS) vorbirea e destul de tare si ramane neschimbata.
pub const QUIET_LEVEL: f32 = 0.2;

/// Nivelul varfurilor: valoarea absoluta sub care stau 99,5% din esantioane. Nu maximul, pentru ca
/// un singur clic (tasta de dictare e apasata chiar langa microfon) ar ascunde o soapta.
pub fn speech_level(samples: &[f32]) -> f32 {
    let mut abs: Vec<f32> = samples
        .iter()
        .map(|x| x.abs())
        .filter(|x| x.is_finite())
        .collect();
    if abs.is_empty() {
        return 0.0;
    }
    let idx = ((abs.len() - 1) as f64 * 0.995).floor() as usize;
    let (_, level, _) = abs.select_nth_unstable_by(idx, f32::total_cmp);
    *level
}

/// Cu cat trebuie inmultit un clip incet; `None` daca ramane neschimbat (liniste sau destul de tare).
pub fn quiet_gain(samples: &[f32]) -> Option<f32> {
    let level = speech_level(samples);
    if !(SILENCE_LEVEL..QUIET_LEVEL).contains(&level) {
        return None;
    }
    Some((GAIN_TARGET / level).min(GAIN_MAX))
}

/// Aplica amplificarea; ce ar depasi scala (clicuri izolate) e taiat la ±1.
pub fn apply_gain(samples: &[f32], gain: f32) -> Vec<f32> {
    samples
        .iter()
        .map(|x| (x * gain).clamp(-1.0, 1.0))
        .collect()
}

/// Amplifica un clip incet. `None` daca nu e nevoie (clipul se foloseste asa cum e).
pub fn normalize_quiet(samples: &[f32]) -> Option<Vec<f32>> {
    quiet_gain(samples).map(|g| apply_gain(samples, g))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un ton de 220 Hz la 16 kHz, cu amplitudinea data.
    fn tone(amplitude: f32, secs: f32) -> Vec<f32> {
        (0..(16_000.0 * secs) as usize)
            .map(|i| amplitude * (i as f32 * 2.0 * std::f32::consts::PI * 220.0 / 16_000.0).sin())
            .collect()
    }

    /// Zgomot determinist in [-amplitude, amplitude].
    fn noise(amplitude: f32, n: usize) -> Vec<f32> {
        let mut seed = 12345u32;
        (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                amplitude * ((seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0)
            })
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn quiet_clip_is_brought_to_target() {
        let clip = tone(0.05, 1.0);
        let gain = quiet_gain(&clip).unwrap();
        assert!((gain - 14.16).abs() < 0.1, "{gain}");
        let out = normalize_quiet(&clip).unwrap();
        assert_eq!(out.len(), clip.len());
        assert!((peak(&out) - GAIN_TARGET).abs() < 0.01, "{}", peak(&out));
        // Forma semnalului ramane aceeasi: doar inmultire.
        assert!((out[10] / clip[10] - gain).abs() < 1e-3);
    }

    #[test]
    fn loud_clip_is_untouched() {
        assert_eq!(quiet_gain(&tone(0.5, 1.0)), None);
        assert_eq!(quiet_gain(&tone(0.9, 1.0)), None);
        assert!(normalize_quiet(&tone(QUIET_LEVEL + 0.01, 1.0)).is_none());
    }

    #[test]
    fn silence_is_untouched() {
        assert_eq!(quiet_gain(&[]), None);
        assert_eq!(quiet_gain(&vec![0.0; 16_000]), None);
        // Zgomotul camerei nu e ridicat la nivel de vorbire.
        assert_eq!(quiet_gain(&noise(0.002, 16_000)), None);
        assert!(normalize_quiet(&noise(SILENCE_LEVEL * 0.9, 16_000)).is_none());
    }

    #[test]
    fn silence_with_a_key_click_is_still_silence() {
        let mut clip = noise(0.001, 32_000);
        for s in clip.iter_mut().take(60) {
            *s = 0.4;
        }
        assert_eq!(quiet_gain(&clip), None);
    }

    #[test]
    fn gain_is_capped_at_30_db() {
        let clip = tone(0.01, 1.0);
        assert_eq!(quiet_gain(&clip), Some(GAIN_MAX));
        let out = normalize_quiet(&clip).unwrap();
        assert!((peak(&out) - 0.01 * GAIN_MAX).abs() < 0.005);
        assert!(peak(&out) < GAIN_TARGET);
    }

    #[test]
    fn never_clips() {
        // Soapta cu un clic puternic: clicul nu impiedica amplificarea si nu iese din scala.
        let mut clip = tone(0.03, 2.0);
        for s in clip.iter_mut().skip(100).take(40) {
            *s = 0.6;
        }
        clip[5000] = -0.9;
        let out = normalize_quiet(&clip).unwrap();
        assert!(out.iter().all(|x| (-1.0..=1.0).contains(x)));
        assert_eq!(peak(&out), 1.0);
        // Restul semnalului a ajuns la tinta.
        assert!((out[12_018].abs() - clip[12_018].abs() * (GAIN_TARGET / 0.03)).abs() < 0.02);

        for a in [0.006, 0.02, 0.1, 0.19] {
            let out = normalize_quiet(&tone(a, 0.5)).unwrap();
            assert!(peak(&out) <= GAIN_TARGET + 0.01, "{a}: {}", peak(&out));
        }
    }

    #[test]
    fn ignores_non_finite_samples() {
        let mut clip = tone(0.05, 1.0);
        clip[3] = f32::NAN;
        clip[4] = f32::INFINITY;
        assert!(quiet_gain(&clip).is_some());
    }

    #[test]
    fn whisper_languages() {
        assert_eq!(parse_whisper_language("ro"), Ok("ro"));
        assert_eq!(parse_whisper_language(" EN "), Ok("en"));
        assert_eq!(parse_whisper_language(""), Ok(""));
        assert_eq!(parse_whisper_language("auto"), Ok(""));
        for l in WHISPER_LANGUAGES {
            assert_eq!(parse_whisper_language(l), Ok(l));
        }
        let err = parse_whisper_language("klingon").unwrap_err();
        assert!(err.contains("klingon") && err.contains("ro, en, es, fr, de, it, pt"), "{err}");
        assert!(parse_whisper_language("ro; rm -rf").is_err());
    }

    #[test]
    fn settings_default_on_and_partial_toml() {
        let d = DictationSettings::default();
        assert!(d.double_tap_lock && d.normalize_quiet_audio);
        let d: DictationSettings = toml::from_str("double_tap_lock = false\n").unwrap();
        assert!(!d.double_tap_lock && d.normalize_quiet_audio);
    }
}
