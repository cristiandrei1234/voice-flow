//! Tipuri pure, fara I/O de sistem: setari, dictionar personal, formatarea textului.

pub mod dictation;
pub mod command;
pub mod dictionary;
pub mod format;
pub mod history;
pub mod models;
pub mod settings;
pub mod snippets;
pub mod styles;

pub use dictation::DictationSettings;
pub use command::CommandSettings;
pub use dictionary::{Dictionary, DictionaryFile, FileEntry};
pub use history::{HistoryRecord, HistorySettings, HistoryStats};
pub use models::{ModelFamily, ModelFile, ModelSpec};
pub use settings::{CleanupSettings, EngineKind, HotkeyMode, Settings};
pub use snippets::{Snippet, SnippetDraft, SnippetMatcher, SnippetSet};
pub use styles::{CleanupLevel, StyleId, StylesSettings};
