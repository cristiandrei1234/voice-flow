//! Tasta globala pentru dictare, pe handy-keys (aceeasi biblioteca folosita de Handy).
//!
//! Suporta tasta Fn (ca Wispr Flow), combinatii doar de modificatori ("ctrl+opt"),
//! modificatori pe o parte ("ralt" = Option dreapta) si taste simple ("f13").
//! Ruleaza pe firul ei; nu blocheaza firul principal (unde ruleaza interfata).
//!
//! Pe macOS are nevoie de permisiunea Accessibility pentru aplicatia din care rulezi.

pub mod command;

use anyhow::{anyhow, Context};
use handy_keys::{Hotkey, HotkeyManager, HotkeyState as KeyState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use vf_domain::HotkeyMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Start,
    Stop,
    /// Escape in timpul inregistrarii: renunta fara sa transcrie.
    Cancel,
    /// Apasare dubla: inregistrarea in curs ramane pornita fara maini ("hands-free"),
    /// pana la urmatoarea apasare (Stop) sau Esc (Cancel).
    Lock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Down,
    Up,
    Escape,
}

/// Traduce numele prietenoase din setari in sintaxa handy-keys.
pub fn parse_hotkey(name: &str) -> anyhow::Result<Hotkey> {
    let n = name.trim().to_lowercase().replace(' ', "");
    let n = match n.as_str() {
        "right-option" | "right-alt" | "option-dreapta" => "ralt".to_string(),
        "left-option" | "left-alt" => "lalt".to_string(),
        "right-command" | "right-cmd" => "cmdright".to_string(),
        "right-control" | "right-ctrl" => "ctrlright".to_string(),
        "right-shift" => "rshift".to_string(),
        "globe" => "fn".to_string(),
        other => other.replace('-', "+"),
    };
    n.parse::<Hotkey>().map_err(|e| {
        anyhow!(
            "The key \"{name}\" is not valid ({e}). Examples: fn, ctrl+opt, ralt, f13, cmd+shift+space"
        )
    })
}

/// Are aplicatia (de fapt terminalul din care rulezi) permisiunea Accessibility?
pub fn has_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        handy_keys::check_accessibility()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Cere permisiunea Accessibility: macOS arata dialogul sistemului ("... would like to control
/// this computer" cu butonul "Open System Settings") si adauga aplicatia in lista din Setari,
/// ca sa nu mai trebuiasca adaugata manual cu "+". Intoarce `true` daca e deja acordata.
pub fn request_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::request_accessibility()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Deschide direct System Settings > Privacy & Security > Accessibility.
pub fn open_permission_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = handy_keys::open_accessibility_settings();
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        static kAXTrustedCheckOptionPrompt: *const c_void;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: *const c_void;
        // Folosim doar adresele acestor structuri.
        static kCFTypeDictionaryKeyCallBacks: u8;
        static kCFTypeDictionaryValueCallBacks: u8;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }

    pub fn request_accessibility() -> bool {
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let options = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                std::ptr::addr_of!(kCFTypeDictionaryKeyCallBacks) as *const c_void,
                std::ptr::addr_of!(kCFTypeDictionaryValueCallBacks) as *const c_void,
            );
            let trusted = AXIsProcessTrustedWithOptions(options);
            if !options.is_null() {
                CFRelease(options);
            }
            trusted
        }
    }
}

/// Mai multe taste, separate prin virgula: "fn, right-command" = oricare dintre ele.
pub fn parse_hotkeys(list: &str) -> anyhow::Result<Vec<Hotkey>> {
    let keys: Vec<Hotkey> = list
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(parse_hotkey)
        .collect::<anyhow::Result<_>>()?;
    anyhow::ensure!(!keys.is_empty(), "No key is set");
    Ok(keys)
}

/// Nume de afisat in meniu: "fn, right-command" -> "Fn or Right Cmd".
pub fn display_name(list: &str) -> String {
    list.split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(|k| match k.to_lowercase().replace(' ', "").as_str() {
            "fn" | "globe" => "Fn".to_string(),
            "right-command" | "right-cmd" | "cmdright" | "rcmd" => "Right Cmd".to_string(),
            "right-option" | "right-alt" | "ralt" => "Right Option".to_string(),
            "right-control" | "right-ctrl" | "ctrlright" => "Right Control".to_string(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" or ")
}

/// Blocarea cu apasare dubla e o setare a utilizatorului; `start` o citeste de aici la pornirea
/// ascultarii, ca semnatura lui sa ramana neschimbata.
static DOUBLE_TAP_LOCK: AtomicBool = AtomicBool::new(false);

/// Porneste / opreste blocarea cu apasare dubla pentru ascultarile pornite de acum incolo.
pub fn set_double_tap_lock(enabled: bool) {
    DOUBLE_TAP_LOCK.store(enabled, Ordering::Relaxed);
}

/// O apasare mai scurta de atat e o "atingere" (tap), nu o dictare cu tasta tinuta.
pub const TAP_MAX: Duration = Duration::from_millis(250);
/// Cat asteptam a doua atingere dupa ce s-a ridicat prima.
pub const DOUBLE_TAP_GAP: Duration = Duration::from_millis(400);

/// Unde suntem in modul "tii apasat" cu blocare la apasare dubla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    /// Tasta e jos si inregistram (prima apasare).
    Holding { since: Instant },
    /// Prima atingere s-a terminat; inregistrarea continua pana vedem daca vine a doua.
    TapWait { released: Instant },
    /// A doua apasare e jos; daca e tot o atingere, blocam.
    SecondDown { since: Instant },
    /// Fara maini: inregistram pana la urmatoarea apasare.
    Locked,
}

/// Masina de stari pentru apasari, separata de handy-keys ca sa poata fi testata.
pub struct HotkeyLogic {
    mode: HotkeyMode,
    pressed: bool,
    recording: bool,
    double_tap: bool,
    phase: Phase,
}

impl HotkeyLogic {
    pub fn new(mode: HotkeyMode) -> Self {
        Self {
            mode,
            pressed: false,
            recording: false,
            double_tap: false,
            phase: Phase::Idle,
        }
    }

    /// In modul "tii apasat": doua atingeri rapide blocheaza inregistrarea (fara maini).
    /// In modul "apesi o data" nu schimba nimic.
    pub fn with_double_tap(mut self, enabled: bool) -> Self {
        self.double_tap = enabled;
        self
    }

    fn uses_lock(&self) -> bool {
        self.double_tap && self.mode == HotkeyMode::Hold
    }

    pub fn handle(&mut self, input: Input) -> Option<HotkeyEvent> {
        self.handle_at(input, Instant::now())
    }

    /// Ca `handle`, cu momentul evenimentului dat din afara (pentru teste).
    pub fn handle_at(&mut self, input: Input, now: Instant) -> Option<HotkeyEvent> {
        if self.uses_lock() {
            return self.handle_lock(input, now);
        }
        match input {
            Input::Down => {
                if self.pressed {
                    return None; // repetitie automata
                }
                self.pressed = true;
                match self.mode {
                    HotkeyMode::Hold => {
                        self.recording = true;
                        Some(HotkeyEvent::Start)
                    }
                    HotkeyMode::Toggle => {
                        self.recording = !self.recording;
                        Some(if self.recording {
                            HotkeyEvent::Start
                        } else {
                            HotkeyEvent::Stop
                        })
                    }
                }
            }
            Input::Up => {
                self.pressed = false;
                if self.mode == HotkeyMode::Hold && self.recording {
                    self.recording = false;
                    Some(HotkeyEvent::Stop)
                } else {
                    None
                }
            }
            Input::Escape if self.recording => {
                self.recording = false;
                Some(HotkeyEvent::Cancel)
            }
            Input::Escape => None,
        }
    }

    /// De apelat periodic cand nu vine nicio tasta: o atingere singura, neurmata de a doua,
    /// se anuleaza (fara transcriere, deci fara text lipit din greseala).
    pub fn tick(&mut self, now: Instant) -> Option<HotkeyEvent> {
        match self.phase {
            Phase::TapWait { released }
                if now.saturating_duration_since(released) > DOUBLE_TAP_GAP =>
            {
                self.phase = Phase::Idle;
                Some(HotkeyEvent::Cancel)
            }
            _ => None,
        }
    }

    /// E blocata inregistrarea (fara maini)?
    pub fn is_locked(&self) -> bool {
        self.phase == Phase::Locked
    }

    fn handle_lock(&mut self, input: Input, now: Instant) -> Option<HotkeyEvent> {
        match input {
            Input::Down => {
                if self.pressed {
                    return None; // repetitie automata
                }
                self.pressed = true;
                match self.phase {
                    Phase::Idle => {
                        self.phase = Phase::Holding { since: now };
                        Some(HotkeyEvent::Start)
                    }
                    Phase::TapWait { released }
                        if now.saturating_duration_since(released) <= DOUBLE_TAP_GAP =>
                    {
                        self.phase = Phase::SecondDown { since: now };
                        None
                    }
                    // A doua apasare a venit prea tarziu (inainte de `tick`): e o dictare obisnuita,
                    // in aceeasi inregistrare.
                    Phase::TapWait { .. } => {
                        self.phase = Phase::Holding { since: now };
                        None
                    }
                    Phase::Locked => {
                        self.phase = Phase::Idle;
                        Some(HotkeyEvent::Stop)
                    }
                    // Nu ajungem aici cu tasta ridicata; pastram starea.
                    Phase::Holding { .. } | Phase::SecondDown { .. } => None,
                }
            }
            Input::Up => {
                self.pressed = false;
                match self.phase {
                    Phase::Holding { since } => {
                        if now.saturating_duration_since(since) < TAP_MAX {
                            self.phase = Phase::TapWait { released: now };
                            None
                        } else {
                            self.phase = Phase::Idle;
                            Some(HotkeyEvent::Stop)
                        }
                    }
                    Phase::SecondDown { since } => {
                        if now.saturating_duration_since(since) < TAP_MAX {
                            self.phase = Phase::Locked;
                            Some(HotkeyEvent::Lock)
                        } else {
                            // Atingere urmata de o apasare lunga: dictare obisnuita.
                            self.phase = Phase::Idle;
                            Some(HotkeyEvent::Stop)
                        }
                    }
                    Phase::Idle | Phase::TapWait { .. } | Phase::Locked => None,
                }
            }
            Input::Escape => {
                if self.phase == Phase::Idle {
                    None
                } else {
                    self.phase = Phase::Idle;
                    Some(HotkeyEvent::Cancel)
                }
            }
        }
    }
}

/// Tine ascultarea pornita cat timp exista; la `drop` se opreste.
pub struct HotkeyListener {
    stop: Arc<AtomicBool>,
    _thread: std::thread::JoinHandle<()>,
}

impl Drop for HotkeyListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Porneste ascultarea pe un fir separat. `on_event` e apelat de pe acel fir.
pub fn start(
    hotkey: &str,
    mode: HotkeyMode,
    on_event: impl Fn(HotkeyEvent) + Send + 'static,
) -> anyhow::Result<HotkeyListener> {
    start_with_command(hotkey, mode, None, move |ev| {
        if let command::KeyEvent::Dictation(ev) = ev {
            on_event(ev);
        }
    })
}

/// Ca `start`, plus o a doua scurtatura (Command Mode), validata cu `command::parse_command_hotkey`.
pub fn start_with_command(
    hotkey: &str,
    mode: HotkeyMode,
    command_hotkey: Option<&str>,
    on_event: impl Fn(command::KeyEvent) + Send + 'static,
) -> anyhow::Result<HotkeyListener> {
    let mains = parse_hotkeys(hotkey)?;
    let command_key = command_hotkey
        .map(|c| command::parse_command_hotkey(c, hotkey))
        .transpose()?;
    let escape: Hotkey = "escape".parse().map_err(|e| anyhow!("{e}"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let debug = std::env::var_os("VF_DEBUG_KEYS").is_some();

    // Managerul se creeaza pe firul lui; erorile de pornire (permisiuni) le trimitem inapoi.
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<anyhow::Result<()>>();
    let thread = std::thread::Builder::new()
        .name("vf-hotkey".into())
        .spawn(move || {
            let setup = (|| -> anyhow::Result<_> {
                let manager = HotkeyManager::new().map_err(|e| {
                    anyhow!(
                        "Could not listen to the keyboard ({e}). On macOS: System Settings > Privacy & Security > \
                         Accessibility, enable the app (Voice Flow or Terminal)."
                    )
                })?;
                let main_ids = mains
                    .into_iter()
                    .map(|k| manager.register(k).map_err(|e| anyhow!("{e}")))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let esc_id = manager.register(escape).map_err(|e| anyhow!("{e}"))?;
                let command_id = command_key
                    .map(|k| manager.register(k).map_err(|e| anyhow!("{e}")))
                    .transpose()?;
                Ok((manager, main_ids, esc_id, command_id))
            })();
            let (manager, main_ids, esc_id, command_id) = match setup {
                Ok(v) => {
                    let _ = ready_tx.send(Ok(()));
                    v
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let mut logic = HotkeyLogic::new(mode);
            logic = logic.with_double_tap(DOUBLE_TAP_LOCK.load(Ordering::Relaxed));
            let mut router = command::CommandRouter::default();
            // try_recv + pauza scurta, ca sa putem opri firul cand se schimba tasta.
            while !thread_stop.load(Ordering::SeqCst) {
                let Some(ev) = manager.try_recv() else {
                    if let Some(out) = logic.tick(Instant::now()) {
                        on_event(command::KeyEvent::Dictation(out));
                    }
                    std::thread::sleep(Duration::from_millis(8));
                    continue;
                };
                if debug {
                    eprintln!("[keys] id={} state={:?}", ev.id.as_u32(), ev.state);
                }
                if command_id == Some(ev.id) {
                    let (reset, key) = router.command_key(ev.state == KeyState::Pressed);
                    if reset {
                        let _ = logic.handle(Input::Escape);
                    }
                    if let Some(key) = key {
                        on_event(command::KeyEvent::Command(key));
                    }
                    continue;
                }
                let input = if main_ids.contains(&ev.id) {
                    match ev.state {
                        KeyState::Pressed => Input::Down,
                        KeyState::Released => Input::Up,
                    }
                } else if ev.id == esc_id && ev.state == KeyState::Pressed {
                    Input::Escape
                } else {
                    continue;
                };
                let input = match router.filter(input) {
                    command::Filtered::Dictation(input) => input,
                    command::Filtered::Command(key) => {
                        on_event(command::KeyEvent::Command(key));
                        continue;
                    }
                    command::Filtered::Ignore => continue,
                };
                if let Some(out) = logic.handle(input) {
                    on_event(command::KeyEvent::Dictation(out));
                }
            }
        })
        .context("Could not start the keyboard thread")?;

    ready_rx
        .recv()
        .context("The keyboard thread stopped")??;
    Ok(HotkeyListener {
        stop,
        _thread: thread,
    })
}

fn score(h: &Hotkey) -> u32 {
    h.modifiers.bits().count_ones() + u32::from(h.key.is_some()) * 2
}

/// Asteapta ca utilizatorul sa apese o tasta sau o combinatie si o intoarce ca text
/// (ex. "Fn", "CmdRight", "Ctrl+Opt", "Cmd+Shift+Space"). Combinatia se considera gata
/// cand sunt eliberate toate tastele. Esc singur sau expirarea timpului intorc `None`.
pub fn record_once(timeout: Duration) -> anyhow::Result<Option<String>> {
    let listener = handy_keys::KeyboardListener::new().map_err(|e| anyhow!("{e}"))?;
    let deadline = Instant::now() + timeout;
    let mut best: Option<Hotkey> = None;
    while Instant::now() < deadline {
        let Ok(ev) = listener.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        if ev.is_key_down && ev.modifiers.is_empty() && ev.key == Some(handy_keys::Key::Escape) {
            return Ok(None);
        }
        let key = if ev.is_key_down { ev.key } else { None };
        if let Ok(candidate) = Hotkey::new(ev.modifiers, key) {
            if best.as_ref().is_none_or(|b| score(&candidate) >= score(b)) {
                best = Some(candidate);
            }
        }
        let all_released = !ev.is_key_down && ev.modifiers.is_empty();
        if all_released {
            if let Some(b) = best.take() {
                return Ok(Some(b.to_string()));
            }
        }
    }
    Ok(None)
}

/// Afiseaza toate evenimentele de tastatura (pentru depanare: `voice-flow keys`).
pub fn debug_keys() -> anyhow::Result<()> {
    let listener = handy_keys::KeyboardListener::new().map_err(|e| anyhow!("{e}"))?;
    eprintln!("Press keys (Ctrl+C quits). If nothing shows up, the Accessibility / Input Monitoring permission is missing.");
    loop {
        if let Ok(ev) = listener.recv() {
            let as_hk = ev.as_hotkey().map(|h| h.to_string()).unwrap_or_default();
            println!(
                "{} modifiers={} key={:?} -> {}",
                if ev.is_key_down { "down" } else { "up  " },
                ev.modifiers,
                ev.key,
                as_hk
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_mode() {
        let mut s = HotkeyLogic::new(HotkeyMode::Hold);
        assert_eq!(s.handle(Input::Down), Some(HotkeyEvent::Start));
        assert_eq!(s.handle(Input::Down), None);
        assert_eq!(s.handle(Input::Up), Some(HotkeyEvent::Stop));
        assert_eq!(s.handle(Input::Escape), None);
    }

    #[test]
    fn toggle_mode_and_cancel() {
        let mut s = HotkeyLogic::new(HotkeyMode::Toggle);
        assert_eq!(s.handle(Input::Down), Some(HotkeyEvent::Start));
        assert_eq!(s.handle(Input::Up), None);
        assert_eq!(s.handle(Input::Escape), Some(HotkeyEvent::Cancel));
        assert_eq!(s.handle(Input::Down), Some(HotkeyEvent::Start));
        assert_eq!(s.handle(Input::Up), None);
        assert_eq!(s.handle(Input::Down), Some(HotkeyEvent::Stop));
    }

    fn lock(mode: HotkeyMode) -> (HotkeyLogic, Instant) {
        (HotkeyLogic::new(mode).with_double_tap(true), Instant::now())
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn lock_long_hold_is_unaffected() {
        let (mut s, t) = lock(HotkeyMode::Hold);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Down, t + ms(100)), None); // repetitie
        assert_eq!(s.tick(t + ms(1000)), None);
        assert_eq!(s.handle_at(Input::Up, t + ms(2000)), Some(HotkeyEvent::Stop));
        assert_eq!(s.tick(t + ms(5000)), None);
        assert_eq!(s.handle_at(Input::Escape, t + ms(5000)), None);
        // Exact la prag e deja apasare lunga.
        assert_eq!(s.handle_at(Input::Down, t + ms(6000)), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(6250)), Some(HotkeyEvent::Stop));
    }

    #[test]
    fn lock_single_tap_is_cancelled_quietly() {
        let (mut s, t) = lock(HotkeyMode::Hold);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        // Atingere: inca nu stim daca urmeaza a doua, deci nu oprim (si nu transcriem) nimic.
        assert_eq!(s.handle_at(Input::Up, t + ms(120)), None);
        assert_eq!(s.tick(t + ms(300)), None);
        assert_eq!(s.tick(t + ms(520)), None);
        // Dupa fereastra: anulare, nu Stop (fara lipire din greseala).
        assert_eq!(s.tick(t + ms(521)), Some(HotkeyEvent::Cancel));
        assert_eq!(s.tick(t + ms(900)), None);
        assert!(!s.is_locked());
        // Urmatoarea apasare porneste o dictare noua.
        assert_eq!(s.handle_at(Input::Down, t + ms(2000)), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(3000)), Some(HotkeyEvent::Stop));
    }

    #[test]
    fn lock_double_tap_then_stop() {
        let (mut s, t) = lock(HotkeyMode::Hold);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(100)), None);
        assert_eq!(s.tick(t + ms(200)), None);
        // A doua atingere: aceeasi inregistrare continua, fara un al doilea Start.
        assert_eq!(s.handle_at(Input::Down, t + ms(300)), None);
        assert!(!s.is_locked());
        assert_eq!(s.handle_at(Input::Up, t + ms(400)), Some(HotkeyEvent::Lock));
        assert!(s.is_locked());
        // Fara maini: timpul nu o opreste.
        assert_eq!(s.tick(t + ms(60_000)), None);
        // O apasare opreste; ridicarea tastei nu mai face nimic.
        assert_eq!(s.handle_at(Input::Down, t + ms(60_000)), Some(HotkeyEvent::Stop));
        assert_eq!(s.handle_at(Input::Down, t + ms(60_050)), None);
        assert_eq!(s.handle_at(Input::Up, t + ms(60_100)), None);
        assert!(!s.is_locked());
        assert_eq!(s.tick(t + ms(61_000)), None);
        assert_eq!(s.handle_at(Input::Down, t + ms(62_000)), Some(HotkeyEvent::Start));
    }

    #[test]
    fn lock_double_tap_then_escape() {
        let (mut s, t) = lock(HotkeyMode::Hold);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(80)), None);
        assert_eq!(s.handle_at(Input::Down, t + ms(200)), None);
        assert_eq!(s.handle_at(Input::Up, t + ms(280)), Some(HotkeyEvent::Lock));
        assert_eq!(s.handle_at(Input::Escape, t + ms(5000)), Some(HotkeyEvent::Cancel));
        assert!(!s.is_locked());
        assert_eq!(s.handle_at(Input::Escape, t + ms(5100)), None);
        assert_eq!(s.handle_at(Input::Down, t + ms(6000)), Some(HotkeyEvent::Start));
    }

    #[test]
    fn lock_escape_cancels_in_every_phase() {
        // Cu tasta tinuta.
        let (mut s, t) = lock(HotkeyMode::Hold);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Escape, t + ms(500)), Some(HotkeyEvent::Cancel));
        assert_eq!(s.handle_at(Input::Up, t + ms(900)), None);
        // In asteptarea celei de-a doua atingeri: o singura anulare, nu doua.
        assert_eq!(s.handle_at(Input::Down, t + ms(2000)), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(2100)), None);
        assert_eq!(s.handle_at(Input::Escape, t + ms(2200)), Some(HotkeyEvent::Cancel));
        assert_eq!(s.tick(t + ms(3000)), None);
    }

    #[test]
    fn lock_slow_double_tap_does_not_lock() {
        let (mut s, t) = lock(HotkeyMode::Hold);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(100)), None);
        assert_eq!(s.tick(t + ms(600)), Some(HotkeyEvent::Cancel));
        // A doua atingere vine prea tarziu: e o atingere noua, si ea anulata.
        assert_eq!(s.handle_at(Input::Down, t + ms(700)), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(800)), None);
        assert!(!s.is_locked());
        assert_eq!(s.tick(t + ms(1300)), Some(HotkeyEvent::Cancel));

        // La fel daca `tick` nu a apucat sa ruleze intre ele: nu blocheaza.
        let (mut s, t) = lock(HotkeyMode::Hold);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(100)), None);
        assert_eq!(s.handle_at(Input::Down, t + ms(700)), None);
        assert_eq!(s.handle_at(Input::Up, t + ms(1500)), Some(HotkeyEvent::Stop));
        assert!(!s.is_locked());
    }

    #[test]
    fn lock_tap_then_long_hold_is_a_normal_dictation() {
        let (mut s, t) = lock(HotkeyMode::Hold);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(100)), None);
        assert_eq!(s.handle_at(Input::Down, t + ms(250)), None);
        assert_eq!(s.tick(t + ms(1000)), None);
        assert_eq!(s.handle_at(Input::Up, t + ms(3000)), Some(HotkeyEvent::Stop));
        assert!(!s.is_locked());
    }

    #[test]
    fn toggle_mode_ignores_double_tap() {
        let (mut s, t) = lock(HotkeyMode::Toggle);
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(50)), None);
        assert_eq!(s.tick(t + ms(1000)), None);
        assert_eq!(s.handle_at(Input::Down, t + ms(150)), Some(HotkeyEvent::Stop));
        assert_eq!(s.handle_at(Input::Up, t + ms(200)), None);
        assert_eq!(s.handle_at(Input::Down, t + ms(900)), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Escape, t + ms(950)), Some(HotkeyEvent::Cancel));
        assert!(!s.is_locked());
    }

    #[test]
    fn hold_without_double_tap_is_the_old_behaviour() {
        let mut s = HotkeyLogic::new(HotkeyMode::Hold).with_double_tap(false);
        let t = Instant::now();
        assert_eq!(s.handle_at(Input::Down, t), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(50)), Some(HotkeyEvent::Stop));
        assert_eq!(s.tick(t + ms(1000)), None);
        assert_eq!(s.handle_at(Input::Down, t + ms(100)), Some(HotkeyEvent::Start));
        assert_eq!(s.handle_at(Input::Up, t + ms(150)), Some(HotkeyEvent::Stop));
    }

    #[test]
    fn parses_names() {
        for ok in [
            "fn",
            "Fn",
            "globe",
            "ctrl+opt",
            "right-option",
            "ralt",
            "f13",
            "cmd+shift+space",
            "right-command",
        ] {
            assert!(parse_hotkey(ok).is_ok(), "{ok}");
        }
        assert!(parse_hotkey("nu-exista-tasta").is_err());
        assert_eq!(parse_hotkeys("fn, right-command").unwrap().len(), 2);
        assert_eq!(display_name("fn, right-command"), "Fn or Right Cmd");
    }
}
