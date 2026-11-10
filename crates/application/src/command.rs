//! Fluxul Command Mode, independent de platforma:
//! instructiune rostita -> transcriere -> copierea selectiei -> model -> inlocuirea selectiei.

use crate::{SpeechToText, SAMPLE_RATE};
use std::time::Duration;
use vf_domain::{command, Dictionary};

/// Transforma un text dupa o instructiune (modelul local).
pub trait TextTransformer: Send + Sync {
    fn transform(&self, system_prompt: &str, selection: &str, instruction: &str) -> anyhow::Result<String>;
}

/// Accesul la textul selectat din aplicatia activa.
pub trait SelectionAccess: Send + Sync {
    /// Textul selectat acum; `None` daca nu e nimic selectat.
    fn copy(&self) -> anyhow::Result<Option<String>>;
    /// Pune `text` in locul selectiei.
    fn replace(&self, text: &str) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    Replaced {
        instruction: String,
        before: String,
        after: String,
    },
    /// Inregistrare prea scurta sau fara vorbire.
    NoSpeech,
    NoSelection {
        instruction: String,
    },
    Failed {
        instruction: Option<String>,
        message: String,
    },
}

pub struct CommandFlow<'a> {
    pub asr: &'a dyn SpeechToText,
    pub dictionary: &'a Dictionary,
    pub transformer: &'a dyn TextTransformer,
    pub selection: &'a dyn SelectionAccess,
    pub min_audio: Duration,
}

impl CommandFlow<'_> {
    pub fn run(&self, samples: &[f32], system_prompt: &str) -> CommandOutcome {
        let audio = Duration::from_secs_f64(samples.len() as f64 / SAMPLE_RATE as f64);
        if audio < self.min_audio {
            return CommandOutcome::NoSpeech;
        }
        // Instructiunea: transcriere + dictionar, fara pasul de curatare.
        let raw = match self.asr.transcribe(samples) {
            Ok(t) => t,
            Err(e) => return failed(None, format!("Transcription failed: {e:#}")),
        };
        let instruction = self.dictionary.apply(&raw).trim().to_string();
        if instruction.is_empty() {
            return CommandOutcome::NoSpeech;
        }

        let before = match self.selection.copy() {
            Ok(Some(text)) => text,
            Ok(None) => return CommandOutcome::NoSelection { instruction },
            Err(e) => return failed(Some(instruction), format!("Could not read the selection: {e:#}")),
        };
        if let Some(problem) = command::selection_problem(&before) {
            return failed(Some(instruction), problem);
        }

        let answer = match self.transformer.transform(system_prompt, &before, &instruction) {
            Ok(a) => a,
            Err(e) => return failed(Some(instruction), format!("{e:#}")),
        };
        // Un raspuns gol nu sterge selectia utilizatorului.
        let Some(after) = command::tidy_answer(&before, &answer) else {
            return failed(Some(instruction), "The model returned an empty answer; the text was left unchanged".into());
        };
        if let Err(e) = self.selection.replace(&after) {
            return failed(Some(instruction), format!("Could not replace the selection: {e:#}"));
        }
        CommandOutcome::Replaced {
            instruction,
            before,
            after,
        }
    }
}

fn failed(instruction: Option<String>, message: String) -> CommandOutcome {
    CommandOutcome::Failed { instruction, message }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeAsr(&'static str);
    impl SpeechToText for FakeAsr {
        fn name(&self) -> &str {
            "fake"
        }
        fn transcribe(&self, _: &[f32]) -> anyhow::Result<String> {
            Ok(self.0.into())
        }
    }

    #[derive(Default)]
    struct FakeLlm {
        answer: Option<&'static str>,
        calls: Mutex<Vec<(String, String, String)>>,
    }
    impl TextTransformer for FakeLlm {
        fn transform(&self, prompt: &str, selection: &str, instruction: &str) -> anyhow::Result<String> {
            self.calls
                .lock()
                .unwrap()
                .push((prompt.into(), selection.into(), instruction.into()));
            self.answer.map(String::from).ok_or_else(|| anyhow::anyhow!("server oprit"))
        }
    }

    #[derive(Default)]
    struct FakeSelection {
        selected: Option<&'static str>,
        replaced: Mutex<Vec<String>>,
        copies: Mutex<u32>,
    }
    impl SelectionAccess for FakeSelection {
        fn copy(&self) -> anyhow::Result<Option<String>> {
            *self.copies.lock().unwrap() += 1;
            Ok(self.selected.map(String::from))
        }
        fn replace(&self, text: &str) -> anyhow::Result<()> {
            self.replaced.lock().unwrap().push(text.into());
            Ok(())
        }
    }

    fn secs(s: f32) -> Vec<f32> {
        vec![0.0; (SAMPLE_RATE as f32 * s) as usize]
    }

    fn run(asr: &'static str, llm: &FakeLlm, sel: &FakeSelection, audio: f32) -> CommandOutcome {
        let dictionary = Dictionary::parse("NestJS | esges\n");
        CommandFlow {
            asr: &FakeAsr(asr),
            dictionary: &dictionary,
            transformer: llm,
            selection: sel,
            min_audio: Duration::from_millis(300),
        }
        .run(&secs(audio), "PROMPT")
    }

    #[test]
    fn replaces_selection_with_answer() {
        let llm = FakeLlm { answer: Some("```\nSalut, NestJS!\n```"), ..Default::default() };
        let sel = FakeSelection { selected: Some("salut esges \n"), ..Default::default() };
        let out = run("scrie corect esges", &llm, &sel, 1.0);
        assert_eq!(
            out,
            CommandOutcome::Replaced {
                instruction: "scrie corect NestJS".into(),
                before: "salut esges \n".into(),
                after: "Salut, NestJS! \n".into(),
            }
        );
        assert_eq!(*sel.replaced.lock().unwrap(), vec!["Salut, NestJS! \n".to_string()]);
        let calls = llm.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], ("PROMPT".into(), "salut esges \n".into(), "scrie corect NestJS".into()));
    }

    #[test]
    fn short_or_silent_audio_touches_nothing() {
        let llm = FakeLlm { answer: Some("x"), ..Default::default() };
        let sel = FakeSelection { selected: Some("text"), ..Default::default() };
        assert_eq!(run("fă-l scurt", &llm, &sel, 0.1), CommandOutcome::NoSpeech);
        assert_eq!(run("   ", &llm, &sel, 1.0), CommandOutcome::NoSpeech);
        assert_eq!(*sel.copies.lock().unwrap(), 0);
        assert!(llm.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn no_selection_skips_model_and_paste() {
        let llm = FakeLlm { answer: Some("x"), ..Default::default() };
        let sel = FakeSelection::default();
        assert_eq!(
            run("fă-l scurt", &llm, &sel, 1.0),
            CommandOutcome::NoSelection { instruction: "fă-l scurt".into() }
        );
        assert!(llm.calls.lock().unwrap().is_empty());
        assert!(sel.replaced.lock().unwrap().is_empty());
    }

    #[test]
    fn model_error_or_empty_answer_keeps_selection() {
        let sel = FakeSelection { selected: Some("text"), ..Default::default() };
        let down = FakeLlm::default();
        match run("fă-l scurt", &down, &sel, 1.0) {
            CommandOutcome::Failed { instruction, message } => {
                assert_eq!(instruction.as_deref(), Some("fă-l scurt"));
                assert!(message.contains("server oprit"));
            }
            other => panic!("{other:?}"),
        }
        let empty = FakeLlm { answer: Some("  "), ..Default::default() };
        assert!(matches!(run("fă-l scurt", &empty, &sel, 1.0), CommandOutcome::Failed { .. }));
        assert!(sel.replaced.lock().unwrap().is_empty());
    }

    #[test]
    fn too_long_selection_is_not_sent() {
        let long: &'static str = Box::leak("a".repeat(command::MAX_SELECTION_CHARS + 1).into_boxed_str());
        let llm = FakeLlm { answer: Some("x"), ..Default::default() };
        let sel = FakeSelection { selected: Some(long), ..Default::default() };
        assert!(matches!(run("tradu", &llm, &sel, 1.0), CommandOutcome::Failed { .. }));
        assert!(llm.calls.lock().unwrap().is_empty());
    }
}
