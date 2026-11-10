//! Textul selectat in aplicatia activa (Command Mode): il citim simuland Cmd+C si il inlocuim
//! lipind peste el. Clipboard-ul utilizatorului e pus la loc dupa fiecare pas.

use anyhow::{anyhow, Context};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread::sleep;
use std::time::{Duration, Instant};
use vf_application::command::SelectionAccess;
use vf_application::TextOutput;

pub struct ClipboardSelection {
    /// Pauza inainte de Cmd+C (tastele scurtaturii trebuie sa apuce sa fie eliberate).
    pub before_copy: Duration,
    /// Cat asteptam ca aplicatia sa puna selectia in clipboard.
    pub copy_timeout: Duration,
}

impl Default for ClipboardSelection {
    fn default() -> Self {
        Self {
            before_copy: Duration::from_millis(60),
            copy_timeout: Duration::from_millis(700),
        }
    }
}

/// Ce a adus Cmd+C. `before` / `after` sunt contoarele de schimbari ale clipboard-ului
/// (`None` unde sistemul nu are asa ceva: acolo clipboard-ul a fost golit inainte de copiere).
/// Daca nimic nu e selectat, aplicatia nu scrie in clipboard si contorul ramane pe loc:
/// asa nu confundam vechiul continut al clipboard-ului cu o selectie.
pub fn copied_selection(before: Option<i64>, after: Option<i64>, text: Option<String>) -> Option<String> {
    if before.is_some() && before == after {
        return None;
    }
    text.filter(|t| !t.trim().is_empty())
}

impl SelectionAccess for ClipboardSelection {
    fn copy(&self) -> anyhow::Result<Option<String>> {
        let mut clipboard = arboard::Clipboard::new().context("Could not access the clipboard")?;
        let previous = clipboard.get_text().ok();
        let before = change_count();
        if before.is_none() {
            clipboard.clear().context("Could not clear the clipboard")?;
        }
        sleep(self.before_copy);
        press_with_command('c')?;

        let deadline = Instant::now() + self.copy_timeout;
        let mut selection = None;
        while selection.is_none() && Instant::now() < deadline {
            sleep(Duration::from_millis(20));
            let after = change_count();
            // Citim textul doar dupa ce contorul s-a miscat.
            let text = if before.is_none() || before != after {
                clipboard.get_text().ok()
            } else {
                None
            };
            selection = copied_selection(before, after, text);
        }

        match (&selection, previous) {
            // Punem la loc ce avea utilizatorul, daca in clipboard e inca selectia copiata de noi.
            (Some(sel), Some(prev)) => {
                if clipboard.get_text().ok().as_deref() == Some(sel.as_str()) {
                    let _ = clipboard.set_text(prev);
                }
            }
            // Fara contor am golit clipboard-ul inainte de copiere.
            (None, Some(prev)) if before.is_none() => {
                let _ = clipboard.set_text(prev);
            }
            _ => {}
        }
        Ok(selection)
    }

    fn replace(&self, text: &str) -> anyhow::Result<()> {
        // Aceeasi lipire ca la dictare (cu restaurarea clipboard-ului), fara spatiul de la sfarsit.
        crate::PasteOutput {
            trailing_space: false,
            ..Default::default()
        }
        .deliver(text)
    }
}

/// Cmd+tasta (Ctrl+tasta pe Windows / Linux).
fn press_with_command(key: char) -> anyhow::Result<()> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| anyhow!("Could not simulate the keyboard: {e:?}"))?;
    let modifier = if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    };
    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| anyhow!("{e:?}"))?;
    let click = enigo.key(Key::Unicode(key), Direction::Click);
    // Eliberam modificatorul chiar daca tasta a esuat, altfel ramane "apasat".
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| anyhow!("{e:?}"))?;
    click.map_err(|e| anyhow!("{e:?}"))
}

/// Contorul de schimbari al clipboard-ului: creste la fiecare scriere, a oricui.
pub fn change_count() -> Option<i64> {
    #[cfg(target_os = "macos")]
    {
        mac::change_count()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void};

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
        fn objc_autoreleasePoolPush() -> *mut c_void;
        fn objc_autoreleasePoolPop(pool: *mut c_void);
    }

    /// `[[NSPasteboard generalPasteboard] changeCount]`
    pub fn change_count() -> Option<i64> {
        unsafe {
            let class = objc_getClass(c"NSPasteboard".as_ptr());
            if class.is_null() {
                return None;
            }
            let send_id: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            let send_int: extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            let pool = objc_autoreleasePoolPush();
            let pasteboard = send_id(class, sel_registerName(c"generalPasteboard".as_ptr()));
            let count = if pasteboard.is_null() {
                None
            } else {
                Some(send_int(pasteboard, sel_registerName(c"changeCount".as_ptr())) as i64)
            };
            objc_autoreleasePoolPop(pool);
            count
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_clipboard_means_no_selection() {
        // Contorul nu s-a miscat: textul din clipboard e cel vechi, nu o selectie.
        assert_eq!(copied_selection(Some(7), Some(7), Some("vechi".into())), None);
        assert_eq!(copied_selection(Some(7), Some(7), None), None);
    }

    #[test]
    fn changed_clipboard_is_the_selection() {
        assert_eq!(
            copied_selection(Some(7), Some(8), Some("text selectat".into())).as_deref(),
            Some("text selectat")
        );
        // Selectia poate fi identica cu ce era deja in clipboard: conteaza contorul, nu textul.
        assert_eq!(copied_selection(Some(7), Some(9), Some("vechi".into())).as_deref(), Some("vechi"));
    }

    #[test]
    fn empty_or_non_text_copy_is_no_selection() {
        assert_eq!(copied_selection(Some(7), Some(8), None), None);
        assert_eq!(copied_selection(Some(7), Some(8), Some(" \n".into())), None);
    }

    #[test]
    fn without_counter_relies_on_cleared_clipboard() {
        assert_eq!(copied_selection(None, None, None), None);
        assert_eq!(copied_selection(None, None, Some("text".into())).as_deref(), Some("text"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_change_count() {
        // Doar citire: nu atinge continutul clipboard-ului.
        let a = change_count().expect("contorul clipboard-ului");
        let b = change_count().expect("contorul clipboard-ului");
        assert!(b >= a);
    }
}
