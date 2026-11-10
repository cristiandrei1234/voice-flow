//! Lipeste textul in aplicatia activa.
//!
//! Pune textul in clipboard, simuleaza Cmd+V (Ctrl+V pe Windows/Linux), apoi pune la loc
//! textul care era inainte in clipboard. Daca in clipboard era altceva decat text
//! (o imagine), deocamdata nu se restaureaza: de rezolvat in faza 2 cu NSPasteboard.

pub mod selection;

use anyhow::{anyhow, Context};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread::sleep;
use std::time::Duration;
use vf_application::TextOutput;

pub struct PasteOutput {
    /// Pauza intre scrierea in clipboard si Cmd+V.
    pub before: Duration,
    /// Pauza dupa Cmd+V, inainte de restaurarea clipboard-ului (aplicatia trebuie sa apuce sa citeasca).
    pub after: Duration,
    /// Spatiu adaugat dupa text, ca dictarile succesive sa nu se lipeasca.
    pub trailing_space: bool,
}

impl Default for PasteOutput {
    fn default() -> Self {
        Self {
            before: Duration::from_millis(40),
            after: Duration::from_millis(350),
            trailing_space: true,
        }
    }
}

impl TextOutput for PasteOutput {
    fn deliver(&self, text: &str) -> anyhow::Result<()> {
        let mut clipboard = arboard::Clipboard::new().context("Could not access the clipboard")?;
        let previous = clipboard.get_text().ok();

        let payload = if self.trailing_space {
            format!("{text} ")
        } else {
            text.to_string()
        };
        clipboard
            .set_text(payload.clone())
            .context("Could not write to the clipboard")?;
        sleep(self.before);

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| anyhow!("Could not simulate the keyboard: {e:?}"))?;
        let modifier = if cfg!(target_os = "macos") {
            Key::Meta
        } else {
            Key::Control
        };
        enigo
            .key(modifier, Direction::Press)
            .map_err(|e| anyhow!("{e:?}"))?;
        let click = enigo.key(Key::Unicode('v'), Direction::Click);
        // Eliberam modificatorul chiar daca V a esuat, altfel ramane "apasat".
        enigo
            .key(modifier, Direction::Release)
            .map_err(|e| anyhow!("{e:?}"))?;
        click.map_err(|e| anyhow!("{e:?}"))?;

        sleep(self.after);
        // Restauram doar daca intre timp nimeni altcineva n-a schimbat clipboard-ul.
        if let Some(prev) = previous {
            if clipboard.get_text().ok().as_deref() == Some(payload.as_str()) {
                let _ = clipboard.set_text(prev);
            }
        }
        Ok(())
    }
}

/// Doar afiseaza textul (pentru `--no-paste`).
pub struct StdoutOutput;

impl TextOutput for StdoutOutput {
    fn deliver(&self, _text: &str) -> anyhow::Result<()> {
        Ok(())
    }
}
