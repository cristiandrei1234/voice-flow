//! Command Mode: firul care transcrie instructiunea, citeste selectia, intreaba modelul local
//! si lipeste rezultatul peste selectie. Fluxul e in `vf_application::command`.

use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use vf_application::command::{CommandFlow, CommandOutcome};
use vf_application::{Dictation, SpeechToText};
use vf_domain::{CleanupSettings, Settings};

/// O instructiune inregistrata, cu promptul valabil in momentul acela.
pub struct Job {
    pub samples: Vec<f32>,
    pub prompt: String,
}

/// Motorul de transcriere, impartit intre dictare si comenzi (modelul se incarca o singura data).
struct SharedAsr(Arc<dyn SpeechToText>);

impl SpeechToText for SharedAsr {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        self.0.transcribe(samples)
    }
}

/// Tine locul motorului cat il mutam intr-un `Arc`.
struct NoAsr;

impl SpeechToText for NoAsr {
    fn name(&self) -> &str {
        ""
    }
    fn transcribe(&self, _: &[f32]) -> anyhow::Result<String> {
        anyhow::bail!("speech engine unavailable")
    }
}

/// Porneste firul de comenzi. Ia motorul de transcriere din `dictation` si il lasa acolo impartit.
pub fn spawn(
    dictation: &mut Dictation,
    llm: &CleanupSettings,
    on_done: impl Fn(CommandOutcome) + Send + 'static,
) -> Sender<Job> {
    let asr: Arc<dyn SpeechToText> = Arc::from(std::mem::replace(&mut dictation.asr, Box::new(NoAsr)));
    dictation.asr = Box::new(SharedAsr(asr.clone()));
    let dictionary = dictation.dictionary.clone();
    let min_audio = dictation.min_audio;
    let transformer = vf_llm_openai::command::OpenAiTransformer::new(llm);

    let (tx, rx) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("vf-command".into())
        .spawn(move || {
            let selection = vf_output_paste::selection::ClipboardSelection::default();
            for job in rx {
                let outcome = CommandFlow {
                    asr: asr.as_ref(),
                    dictionary: &dictionary,
                    transformer: &transformer,
                    selection: &selection,
                    min_audio,
                }
                .run(&job.samples, &job.prompt);
                // In log doar instructiunea si lungimile: textul selectat poate fi orice.
                match &outcome {
                    CommandOutcome::Replaced { instruction, before, after } => eprintln!(
                        "[command] \"{instruction}\": {} -> {} characters",
                        before.chars().count(),
                        after.chars().count()
                    ),
                    CommandOutcome::NoSpeech => eprintln!("[command] (didn't hear anything)"),
                    CommandOutcome::NoSelection { instruction } => {
                        eprintln!("[command] \"{instruction}\": no text selected")
                    }
                    CommandOutcome::Failed { message, .. } => eprintln!("[command] failed: {message}"),
                }
                on_done(outcome);
            }
        })
        .expect("starting the command thread");
    tx
}

/// Scurtatura de comenzi de inregistrat langa tastele de dictare, sau motivul pentru care nu se poate.
pub fn hotkey(s: &Settings) -> Result<Option<String>, String> {
    if !s.command.enabled {
        return Ok(None);
    }
    vf_hotkey::command::parse_command_hotkey(&s.command.hotkey, &s.hotkey)
        .map(|_| Some(s.command.hotkey.clone()))
        .map_err(|e| format!("{e:#}"))
}

/// Serverul modelului local trebuie pornit de daemon? (corecturi SAU comenzi active)
pub fn needs_llm(s: &Settings) -> bool {
    (s.cleanup.enabled || s.command.enabled) && s.cleanup.manage_server
}

pub const NO_SELECTION: &str = "No text is selected";

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings::with_home(std::path::Path::new("/tmp/vf-test"))
    }

    #[test]
    fn llm_needed_for_cleanup_or_commands() {
        let mut s = settings();
        assert!(!needs_llm(&s));
        s.command.enabled = true;
        assert!(needs_llm(&s));
        s.command.enabled = false;
        s.cleanup.enabled = true;
        assert!(needs_llm(&s));
        s.command.enabled = true;
        s.cleanup.manage_server = false;
        assert!(!needs_llm(&s));
    }

    #[test]
    fn hotkey_only_when_enabled_and_valid() {
        let mut s = settings();
        assert_eq!(hotkey(&s), Ok(None));
        s.command.enabled = true;
        assert_eq!(hotkey(&s), Ok(Some("fn+ctrl".into())));
        s.command.hotkey = "fn".into();
        assert!(hotkey(&s).unwrap_err().contains("already used for dictation"));
        // Dezactivat: o scurtatura in conflict nu deranjeaza.
        s.command.enabled = false;
        assert_eq!(hotkey(&s), Ok(None));
    }

    #[test]
    fn shares_the_speech_engine() {
        struct Fake;
        impl SpeechToText for Fake {
            fn name(&self) -> &str {
                "fake"
            }
            fn transcribe(&self, _: &[f32]) -> anyhow::Result<String> {
                Ok("salut".into())
            }
        }
        let mut d = Dictation::new(Box::new(Fake), vf_domain::Dictionary::default());
        let _tx = spawn(&mut d, &CleanupSettings::default(), |_| {});
        // Dictarea foloseste in continuare acelasi motor.
        assert_eq!(d.asr.name(), "fake");
        assert_eq!(d.asr.transcribe(&[]).unwrap(), "salut");
    }
}
