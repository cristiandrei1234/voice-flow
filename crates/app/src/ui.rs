//! Modul cu interfata: pastila de inregistrare + iconita in bara de meniu.
//!
//! Firul principal ruleaza bucla Slint si tine microfonul. Tastatura are firul ei,
//! transcrierea are firul ei; amandoua trimit evenimente aici prin `invoke_from_event_loop`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use slint::winit_030::winit;
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Timer, TimerMode, VecModel};
use vf_audio_cpal::Recorder;
use vf_domain::{Dictionary, HotkeyMode, Settings};
use vf_hotkey::HotkeyEvent;
use vf_presentation::{Pill, PillState, Tray, BARS};

use crate::worker::Outcome;

struct UiState {
    recorder: Option<Recorder>,
    audio_tx: Sender<Vec<f32>>,
    pill: Pill,
    tray: Tray,
    levels: Rc<VecModel<f32>>,
    history: VecDeque<f32>,
    recording: bool,
    processing: bool,
    phase: f32,
    sounds: bool,
    hotkey: String,
    mode: HotkeyMode,
    listener: Option<vf_hotkey::HotkeyListener>,
    _timer: Timer,
    permission_timer: Option<Timer>,
}

thread_local! {
    static STATE: RefCell<Option<UiState>> = const { RefCell::new(None) };
}

fn with_state(f: impl FnOnce(&mut UiState)) {
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            f(st);
        }
    });
}

/// Atributele ferestrei pastilei: fara rama, transparenta, deasupra tuturor
/// si, foarte important, fara sa fure focusul (altfel textul s-ar lipi in fereastra gresita).
fn configure_window(a: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    let a = a
        .with_active(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
    #[cfg(target_os = "macos")]
    let a = {
        use winit::platform::macos::WindowAttributesExtMacOS;
        a.with_has_shadow(false)
    };
    a
}

fn select_backend() -> anyhow::Result<()> {
    #[allow(unused_mut)]
    let mut selector = slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(configure_window);
    #[cfg(target_os = "macos")]
    {
        // Aplicatie "accesoriu": fara iconita in Dock, nu devine aplicatia activa,
        // deci aplicatia in care dictezi ramane in fata.
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        let mut builder =
            winit::event_loop::EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_activate_ignoring_other_apps(false)
            .with_default_menu(false);
        selector = selector.with_winit_event_loop_builder(builder);
    }
    selector
        .select()
        .map_err(|e| anyhow::anyhow!("Could not start the UI: {e}"))
}

/// Pune pastila jos, centrat, deasupra Dock-ului, si o face transparenta la click.
fn position_pill(pill: &Pill) {
    pill.window()
        .with_winit_window(|w: &winit::window::Window| {
            let _ = w.set_cursor_hittest(false);
            let Some(monitor) = w.current_monitor().or_else(|| w.primary_monitor()) else {
                return;
            };
            let (mpos, msize, scale) = (monitor.position(), monitor.size(), monitor.scale_factor());
            let wsize = w.outer_size();
            let x = mpos.x + (msize.width as i32 - wsize.width as i32) / 2;
            let y = mpos.y + msize.height as i32 - wsize.height as i32 - (96.0 * scale) as i32;
            w.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        });
}

impl UiState {
    /// Porneste ascultarea tastaturii daca exista permisiunea. Intoarce `true` daca merge.
    fn start_keyboard(&mut self) -> bool {
        if self.listener.is_some() {
            return true;
        }
        if !vf_hotkey::has_permission() {
            return false;
        }
        match vf_hotkey::start(&self.hotkey, self.mode, |ev| {
            let _ = slint::invoke_from_event_loop(move || with_state(|st| st.on_hotkey(ev)));
        }) {
            Ok(l) => {
                self.listener = Some(l);
                self.permission_timer = None;
                if !self.recording && !self.processing {
                    self.pill.set_state(PillState::Idle);
                }
                self.set_status("Ready");
                eprintln!(
                    "Keyboard: OK. Hold {} and speak.",
                    vf_hotkey::display_name(&self.hotkey)
                );
                true
            }
            Err(e) => {
                eprintln!("{e:#}");
                false
            }
        }
    }

    fn set_status(&self, s: &str) {
        self.tray.set_status(s.into());
    }

    fn on_hotkey(&mut self, ev: HotkeyEvent) {
        let Some(rec) = &self.recorder else {
            self.set_status("no microphone");
            return;
        };
        match ev {
            HotkeyEvent::Start => {
                crate::play("Tink", self.sounds);
                rec.start();
                self.recording = true;
                self.history.clear();
                self.pill.set_state(PillState::Recording);
                self.set_status("recording");
            }
            HotkeyEvent::Stop => {
                if !self.recording {
                    return;
                }
                let samples = rec.stop();
                crate::play("Pop", self.sounds);
                self.recording = false;
                self.processing = true;
                self.pill.set_state(PillState::Processing);
                self.set_status("transcribing");
                let _ = self.audio_tx.send(samples);
            }
            HotkeyEvent::Cancel => {
                let _ = rec.stop();
                crate::play("Basso", self.sounds);
                self.recording = false;
                self.pill.set_state(PillState::Idle);
                self.set_status("Ready");
            }
            // Blocarea cu apasare dubla e pornita doar de daemon.
            HotkeyEvent::Lock => {}
        }
    }

    fn on_done(&mut self, outcome: Outcome) {
        self.processing = false;
        if self.recording {
            return; // a inceput deja o noua inregistrare
        }
        match outcome {
            Outcome::Pasted { .. } | Outcome::Nothing => {
                self.pill.set_state(PillState::Idle);
                self.set_status("Ready");
            }
            Outcome::Failed(e) => {
                self.pill.set_state(PillState::Error);
                self.set_status(&format!(
                    "error: {}",
                    e.chars().take(60).collect::<String>()
                ));
                Timer::single_shot(Duration::from_millis(1500), || {
                    with_state(|st| {
                        if !st.recording && !st.processing {
                            st.pill.set_state(PillState::Idle);
                        }
                    })
                });
            }
        }
    }

    /// Anima barele (~30 fps).
    fn tick(&mut self) {
        let center = (BARS / 2) as f32;
        let mut out = vec![0.0f32; BARS];
        if self.recording {
            let level = self.recorder.as_ref().map(|r| r.level()).unwrap_or(0.0);
            // RMS-ul vorbirii e mic (~0.02 - 0.2); il intindem pe 0..1.
            let v = (level * 14.0).min(1.0).powf(0.6);
            self.history.push_front(v);
            self.history.truncate(BARS);
            self.phase += 0.9;
            for (i, o) in out.iter_mut().enumerate() {
                let d = (i as f32 - center).abs();
                // Cel mai nou nivel in centru, cele vechi se propaga spre margini.
                let h = self.history.get(d as usize).copied().unwrap_or(0.0);
                let jitter = 0.85 + 0.15 * ((self.phase + i as f32 * 1.7).sin());
                *o = h * (1.0 - d * 0.05) * jitter;
            }
        } else if self.processing {
            // Un val mic care trece prin puncte cat timp transcrie.
            self.phase += 0.35;
            let pos = self.phase % (BARS as f32 + 4.0) - 2.0;
            for (i, o) in out.iter_mut().enumerate() {
                *o = 0.28 * (1.0 - (i as f32 - pos).abs() / 1.8).max(0.0);
            }
        }
        self.levels.set_vec(out);
    }
}

pub fn run(
    s: Settings,
    dict: Dictionary,
    mic: Option<&str>,
    config_path: PathBuf,
) -> anyhow::Result<()> {
    select_backend()?;

    let (dictation, cleanup_flag) = crate::build_dictation(&s, dict)?;
    let audio_tx = crate::worker::spawn(dictation, s.paste, |outcome| {
        let _ = slint::invoke_from_event_loop(move || with_state(|st| st.on_done(outcome)));
    });

    let pill = Pill::new()?;
    let levels = Rc::new(VecModel::from(vec![0.0f32; BARS]));
    pill.set_levels(levels.clone().into());
    pill.set_hands_free(s.hotkey_mode == HotkeyMode::Toggle);
    pill.show()?;

    let tray = Tray::new()?;
    tray.set_hotkey(vf_hotkey::display_name(&s.hotkey).into());
    tray.set_engine(
        vf_domain::ModelSpec::of(s.engine).title
        .into(),
    );
    tray.set_cleanup(s.cleanup.enabled);
    wire_tray(&tray, &s, cleanup_flag, config_path);

    let recorder = match Recorder::new(mic) {
        Ok(r) => {
            eprintln!("Microphone: {}", r.device_name());
            Some(r)
        }
        Err(e) => {
            eprintln!("Microphone unavailable: {e:#}");
            tray.set_status("no microphone".into());
            None
        }
    };

    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(33), || {
        with_state(|st| st.tick())
    });

    STATE.with(|st| {
        *st.borrow_mut() = Some(UiState {
            recorder,
            audio_tx,
            pill: pill.clone_strong(),
            tray: tray.clone_strong(),
            levels,
            history: VecDeque::with_capacity(BARS),
            recording: false,
            processing: false,
            phase: 0.0,
            sounds: s.sounds,
            hotkey: s.hotkey.clone(),
            mode: s.hotkey_mode,
            listener: None,
            _timer: timer,
            permission_timer: None,
        })
    });

    // Tastatura. Fara permisiunea Accessibility, macOS nu ne lasa sa ascultam tastele:
    // cerem permisiunea (dialogul sistemului) si reincercam singuri pana e acordata.
    let mut keyboard_ok = false;
    with_state(|st| keyboard_ok = st.start_keyboard());
    if !keyboard_ok {
        vf_hotkey::request_permission();
        eprintln!(
            "The Accessibility permission is missing. In the macOS dialog click \"Open System Settings\" and \
             enable the app you are running this from (Terminal). Dictation starts by itself once it is granted, no restart needed."
        );
        let retry = Timer::default();
        retry.start(TimerMode::Repeated, Duration::from_millis(1500), || {
            with_state(|st| {
                st.start_keyboard();
            })
        });
        with_state(|st| {
            st.pill.set_state(PillState::Error);
            st.set_status("Accessibility permission missing");
            st.permission_timer = Some(retry);
        });
    }

    // Fereastra winit exista abia dupa pornirea buclei; o pozitionam putin dupa.
    let weak = pill.as_weak();
    Timer::single_shot(Duration::from_millis(250), move || {
        if let Some(p) = weak.upgrade() {
            position_pill(&p);
        }
    });

    let mode = match s.hotkey_mode {
        HotkeyMode::Hold => "hold",
        HotkeyMode::Toggle => "press once to start and once more to stop",
    };
    eprintln!(
        "Ready. Hotkey: {} ({mode}). Esc cancels. The icon is in the menu bar.",
        s.hotkey
    );

    slint::run_event_loop_until_quit()?;
    STATE.with(|st| st.borrow_mut().take());
    Ok(())
}

fn wire_tray(tray: &Tray, s: &Settings, cleanup_flag: Arc<AtomicBool>, config_path: PathBuf) {
    let url = s.cleanup.url.clone();
    tray.on_cleanup_toggled(move |on| {
        cleanup_flag.store(on, Ordering::Relaxed);
        eprintln!("Corrections {} ({url})", if on { "on" } else { "off" });
    });
    let dict_path = s.dictionary_path.clone();
    tray.on_open_dictionary(move || crate::open_path(&dict_path, true));
    tray.on_open_settings(move || {
        if !config_path.exists() {
            let _ = crate::write_default_config(&config_path);
        }
        crate::open_path(&config_path, true);
    });
    let folder = s
        .models_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| s.models_dir.clone());
    tray.on_open_folder(move || crate::open_path(&folder, false));
    tray.on_open_permissions(|| {
        vf_hotkey::request_permission();
        vf_hotkey::open_permission_settings();
    });
    tray.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}
