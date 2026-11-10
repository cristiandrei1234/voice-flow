//! Setarile aplicatiei, salvate ca TOML.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineKind {
    /// Parakeet Ro 110M (TDT), dedicat romanei.
    ParakeetRo,
    /// Parakeet TDT 0.6B v3, multilingv (25 de limbi europene).
    ParakeetV3,
    /// Parakeet TDT 0.6B v2, doar engleza.
    ParakeetV2,
    /// Whisper large-v3-turbo.
    WhisperTurbo,
    /// Whisper small.
    WhisperSmall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HotkeyMode {
    /// Tii apasat cat vorbesti.
    Hold,
    /// O apasare porneste, inca una opreste.
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanupSettings {
    pub enabled: bool,
    /// Endpoint compatibil OpenAI (mlx_lm.server, Ollama, LM Studio).
    pub url: String,
    pub model: String,
    pub timeout_secs: u64,
    /// Voice Flow porneste si opreste singur serverul (mlx_lm.server) cu modelul ales.
    pub manage_server: bool,
    /// Comanda serverului; gol = cauta `mlx_lm.server` in locurile obisnuite.
    pub server_command: String,
    /// Regulile de baza ale promptului; gol = cele implicite. `{terms}` e inlocuit cu termenii din dictionar.
    /// Nivelul, stilul si exemplele se adauga dupa ele (vezi `styles::build_prompt`).
    pub prompt: String,
    pub level: crate::styles::CleanupLevel,
}

impl Default for CleanupSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "http://127.0.0.1:8080/v1/chat/completions".into(),
            model: "divinetribe/Hermes-4-14B-abliterated-4bit-mlx".into(),
            timeout_secs: 30,
            manage_server: true,
            server_command: String::new(),
            prompt: String::new(),
            level: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub engine: EngineKind,
    /// Folderul cu modelele (contine parakeet-ro-tdt/, parakeet-v3/).
    pub models_dir: PathBuf,
    pub dictionary_path: PathBuf,
    /// Tasta sau taste separate prin virgula: "fn, right-command" (implicit), "ctrl+opt", "ralt" (Option dreapta), "f13", "cmd+shift+space"...
    pub hotkey: String,
    pub hotkey_mode: HotkeyMode,
    pub threads: i32,
    /// Limba pentru modelele Whisper ("ro", "en", "es"...); gol = detectie automata.
    pub whisper_language: String,
    /// Scorul pentru hotwords; 2.0 a iesit cel mai bun in bench (3.0 strica alte cuvinte).
    pub hotwords_score: f32,
    pub sounds: bool,
    pub paste: bool,
    pub cleanup: CleanupSettings,
    pub history: crate::history::HistorySettings,
    /// Intrarea dictarii: blocare cu apasare dubla, amplificarea vorbirii incete.
    pub dictation: crate::dictation::DictationSettings,
    pub styles: crate::styles::StylesSettings,
    /// Command Mode: a doua scurtatura, care transforma textul selectat cu modelul local.
    pub command: crate::command::CommandSettings,
}

impl Settings {
    pub fn with_home(home: &std::path::Path) -> Self {
        // Totul sta langa config.toml: ~/Library/Application Support/VoiceFlow.
        let base = home.join("Library/Application Support/VoiceFlow");
        Self {
            engine: EngineKind::ParakeetRo,
            models_dir: base.join("models"),
            dictionary_path: base.join("dictionary.txt"),
            hotkey: "fn, right-command".into(),
            hotkey_mode: HotkeyMode::Hold,
            threads: 4,
            whisper_language: "ro".into(),
            hotwords_score: 2.0,
            sounds: true,
            paste: true,
            cleanup: CleanupSettings::default(),
            history: Default::default(),
            dictation: Default::default(),
            styles: Default::default(),
            command: Default::default(),
        }
    }

    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("settings always serialize")
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::with_home(std::path::Path::new("~"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_toml() {
        let s = Settings::with_home(std::path::Path::new("/Users/test"));
        let back = Settings::from_toml(&s.to_toml()).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn partial_toml_uses_defaults() {
        let s =
            Settings::from_toml("engine = \"parakeet-v3\"\n[cleanup]\nenabled = true\n").unwrap();
        assert_eq!(s.engine, EngineKind::ParakeetV3);
        assert!(s.cleanup.enabled);
        assert_eq!(s.hotwords_score, 2.0);
        assert!(s.cleanup.prompt.is_empty());
    }
}
