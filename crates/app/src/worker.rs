//! Firul care transcrie si lipeste. Primeste audio pe un canal, raporteaza rezultatul prin callback.

use std::sync::mpsc::{self, Sender};
use vf_application::{Dictation, TextOutput};

#[derive(Debug, Clone)]
pub enum Outcome {
    Pasted {
        text: String,
        raw: String,
        audio_ms: u64,
        asr_ms: u64,
        cleanup_ms: Option<u64>,
    },
    Nothing,
    Failed(String),
}

pub fn spawn(
    dictation: Dictation,
    paste: bool,
    on_done: impl Fn(Outcome) + Send + 'static,
) -> Sender<Vec<f32>> {
    spawn_with_snippets(dictation, paste, Default::default(), on_done)
}

/// Ca `spawn`, plus snippet-urile aplicate pe textul final (lista se poate schimba din mers).
pub fn spawn_with_snippets(
    dictation: Dictation,
    paste: bool,
    snippets: crate::snippets::Shared,
    on_done: impl Fn(Outcome) + Send + 'static,
) -> Sender<Vec<f32>> {
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    std::thread::Builder::new()
        .name("vf-worker".into())
        .spawn(move || {
            let output: Box<dyn TextOutput> = if paste {
                Box::new(vf_output_paste::PasteOutput::default())
            } else {
                Box::new(vf_output_paste::StdoutOutput)
            };
            for samples in rx {
                let outcome = match dictation.process(&samples) {
                    Ok(Some(mut r)) => {
                        // Dupa dictionar si curatare, inainte de lipire.
                        if let Some(expanded) = crate::snippets::expand(&snippets, &r.text) {
                            r.text = expanded;
                        }
                        let mut timing = format!(
                            "{:.1}s audio, transcription {:.2}s",
                            r.audio.as_secs_f32(),
                            r.asr_time.as_secs_f32()
                        );
                        if let (Some(c), None) = (r.cleanup_time, &r.cleanup_error) {
                            if c.as_millis() > 5 {
                                timing.push_str(&format!(", corrections {:.2}s", c.as_secs_f32()));
                            }
                        }
                        if let Some(e) = &r.cleanup_error {
                            eprintln!("  (corrections failed: {e}; using the uncorrected text)");
                        }
                        eprintln!("[{timing}] {}", r.text);
                        match output.deliver(&r.text) {
                            Ok(()) => Outcome::Pasted {
                                audio_ms: r.audio.as_millis() as u64,
                                asr_ms: r.asr_time.as_millis() as u64,
                                cleanup_ms: r.cleanup_time.map(|c| c.as_millis() as u64),
                                raw: r.raw,
                                text: r.text,
                            },
                            Err(e) => {
                                eprintln!("  could not paste the text: {e:#}");
                                Outcome::Failed(format!("{e:#}"))
                            }
                        }
                    }
                    Ok(None) => {
                        eprintln!("  (didn't hear anything)");
                        Outcome::Nothing
                    }
                    Err(e) => {
                        eprintln!("  transcription error: {e:#}");
                        Outcome::Failed(format!("{e:#}"))
                    }
                };
                on_done(outcome);
            }
        })
        .expect("starting the transcription thread");
    tx
}
