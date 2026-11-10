//! A doua scurtatura globala, pentru Command Mode: o tii apasata, spui instructiunea, o eliberezi.
//! Aici: validarea scurtaturii si arbitrajul intre ea si tastele de dictare (logica pura).

use crate::{parse_hotkey, parse_hotkeys, HotkeyEvent, Input};
use handy_keys::Hotkey;

/// Ce face scurtatura de comenzi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKey {
    Start,
    Stop,
    /// Escape cat timp scurtatura e tinuta: renunta.
    Cancel,
}

/// Evenimentele ascultarii cu doua seturi de taste (`start_with_command`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Dictation(HotkeyEvent),
    Command(CommandKey),
}

/// Valideaza scurtatura de comenzi fata de tastele de dictare (`dictation` = lista din setari).
pub fn parse_command_hotkey(command: &str, dictation: &str) -> anyhow::Result<Hotkey> {
    let command = command.trim();
    anyhow::ensure!(!command.is_empty(), "Choose a shortcut for commands");
    anyhow::ensure!(
        !command.contains(','),
        "Commands use a single shortcut (no comma)"
    );
    let key = parse_hotkey(command)?;
    if parse_hotkey("escape").is_ok_and(|esc| esc == key) {
        anyhow::bail!("Esc cancels a recording; choose another shortcut for commands");
    }
    // O lista de dictare invalida e raportata de dictare, nu de aici.
    if parse_hotkeys(dictation).is_ok_and(|keys| keys.contains(&key)) {
        anyhow::bail!(
            "The shortcut \"{command}\" is already used for dictation; choose another one for commands"
        );
    }
    Ok(key)
}

/// Ce se intampla cu o apasare a tastelor de dictare, dupa arbitraj.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filtered {
    /// Merge mai departe la masina de stari a dictarii.
    Dictation(Input),
    Command(CommandKey),
    Ignore,
}

/// Arbitrul dintre cele doua scurtaturi. Scurtatura de comenzi contine de obicei o tasta de
/// dictare ("fn+ctrl" peste "fn"): intai porneste dictarea, apoi apare comanda. Cat timp comanda
/// e tinuta, ea are intaietate, iar dictarea inceputa din cauza prefixului e preluata, nu oprita.
#[derive(Debug, Default)]
pub struct CommandRouter {
    held: bool,
}

impl CommandRouter {
    /// Scurtatura de comenzi apasata / eliberata. Intoarce (dictarea trebuie adusa in repaus,
    /// evenimentul de trimis). Aducerea in repaus = `Input::Escape` dat masinii de stari a
    /// dictarii, cu rezultatul ignorat: altfel ea ar ramane "in inregistrare" dupa comanda.
    pub fn command_key(&mut self, down: bool) -> (bool, Option<CommandKey>) {
        match (down, self.held) {
            (true, false) => {
                self.held = true;
                (true, Some(CommandKey::Start))
            }
            (false, true) => {
                self.held = false;
                (false, Some(CommandKey::Stop))
            }
            // Repetitie automata, sau eliberare dupa ce comanda a fost anulata cu Esc.
            _ => (false, None),
        }
    }

    /// O apasare a tastelor de dictare (sau Esc).
    pub fn filter(&mut self, input: Input) -> Filtered {
        if !self.held {
            return Filtered::Dictation(input);
        }
        match input {
            Input::Escape => {
                self.held = false;
                Filtered::Command(CommandKey::Cancel)
            }
            // Eliberarea prefixului doar readuce dictarea in starea "neapasat".
            Input::Up => Filtered::Dictation(Input::Up),
            Input::Down => Filtered::Ignore,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HotkeyLogic;
    use vf_domain::HotkeyMode;

    /// Ruleaza o secventa prin arbitru + masina de stari a dictarii, ca in firul de ascultare.
    enum Raw {
        Main(Input),
        Command(bool),
    }

    fn run(mode: HotkeyMode, seq: &[Raw]) -> Vec<KeyEvent> {
        let mut logic = HotkeyLogic::new(mode);
        let mut router = CommandRouter::default();
        let mut out = Vec::new();
        for raw in seq {
            match raw {
                Raw::Command(down) => {
                    let (reset, key) = router.command_key(*down);
                    if reset {
                        let _ = logic.handle(Input::Escape);
                    }
                    out.extend(key.map(KeyEvent::Command));
                }
                Raw::Main(input) => match router.filter(*input) {
                    Filtered::Dictation(input) => out.extend(logic.handle(input).map(KeyEvent::Dictation)),
                    Filtered::Command(key) => out.push(KeyEvent::Command(key)),
                    Filtered::Ignore => {}
                },
            }
        }
        out
    }

    use CommandKey as C;
    use HotkeyEvent as D;
    use KeyEvent::{Command, Dictation};

    #[test]
    fn default_hotkey_is_valid() {
        assert!(parse_command_hotkey("fn+ctrl", "fn, right-command").is_ok());
        assert!(parse_command_hotkey(" Fn+Ctrl ", "fn").is_ok());
    }

    #[test]
    fn rejects_dictation_key_and_bad_input() {
        let err = parse_command_hotkey("fn", "fn, right-command").unwrap_err().to_string();
        assert!(err.contains("already used for dictation"), "{err}");
        // Aceeasi tasta scrisa altfel.
        assert!(parse_command_hotkey("globe", "fn").is_err());
        assert!(parse_command_hotkey("right-cmd", "fn, right-command").is_err());
        assert!(parse_command_hotkey("opt+ctrl", "ctrl+opt").is_err());
        assert!(parse_command_hotkey("", "fn").is_err());
        assert!(parse_command_hotkey("fn+ctrl, f13", "fn").is_err());
        assert!(parse_command_hotkey("escape", "fn").is_err());
        assert!(parse_command_hotkey("nu-exista", "fn").is_err());
    }

    #[test]
    fn command_alone() {
        // Ctrl apasat inaintea lui Fn: tasta de dictare nu apare deloc.
        let out = run(HotkeyMode::Hold, &[Raw::Command(true), Raw::Command(true), Raw::Command(false)]);
        assert_eq!(out, vec![Command(C::Start), Command(C::Stop)]);
    }

    #[test]
    fn command_takes_over_dictation_started_by_its_prefix() {
        // Fn jos (dictare), Ctrl jos (comanda), Ctrl sus, Fn sus.
        let out = run(
            HotkeyMode::Hold,
            &[
                Raw::Main(Input::Down),
                Raw::Command(true),
                Raw::Command(false),
                Raw::Main(Input::Up),
            ],
        );
        // Dictarea nu primeste Stop: inregistrarea ei e preluata de comanda.
        assert_eq!(out, vec![Dictation(D::Start), Command(C::Start), Command(C::Stop)]);
    }

    #[test]
    fn prefix_released_first() {
        let out = run(
            HotkeyMode::Hold,
            &[
                Raw::Main(Input::Down),
                Raw::Command(true),
                Raw::Main(Input::Up),
                Raw::Command(false),
            ],
        );
        assert_eq!(out, vec![Dictation(D::Start), Command(C::Start), Command(C::Stop)]);
    }

    #[test]
    fn dictation_works_normally_after_a_command() {
        for mode in [HotkeyMode::Hold, HotkeyMode::Toggle] {
            let out = run(
                mode,
                &[
                    Raw::Main(Input::Down),
                    Raw::Command(true),
                    Raw::Command(false),
                    Raw::Main(Input::Up),
                    Raw::Main(Input::Down),
                    Raw::Main(Input::Up),
                ],
            );
            // A doua apasare porneste din nou dictarea (nu o "opreste" pe cea preluata).
            assert_eq!(out[..4], [Dictation(D::Start), Command(C::Start), Command(C::Stop), Dictation(D::Start)]);
        }
    }

    #[test]
    fn escape_cancels_command() {
        let out = run(
            HotkeyMode::Hold,
            &[
                Raw::Command(true),
                Raw::Main(Input::Escape),
                Raw::Command(false),
                Raw::Main(Input::Escape),
            ],
        );
        assert_eq!(out, vec![Command(C::Start), Command(C::Cancel)]);
    }

    #[test]
    fn dictation_keys_ignored_while_command_is_held() {
        let out = run(
            HotkeyMode::Toggle,
            &[Raw::Command(true), Raw::Main(Input::Down), Raw::Command(false)],
        );
        assert_eq!(out, vec![Command(C::Start), Command(C::Stop)]);
    }

    #[test]
    fn dictation_untouched_without_command() {
        let out = run(HotkeyMode::Hold, &[Raw::Main(Input::Down), Raw::Main(Input::Up), Raw::Main(Input::Escape)]);
        assert_eq!(out, vec![Dictation(D::Start), Dictation(D::Stop)]);
    }
}
