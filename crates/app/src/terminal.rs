//! Modul fara interfata: tasta globala + microfon, rezultatele in terminal.

use std::sync::mpsc;
use vf_domain::{Dictionary, HotkeyMode, Settings};
use vf_hotkey::HotkeyEvent;

pub fn run(s: Settings, dict: Dictionary, mic: Option<&str>) -> anyhow::Result<()> {
    let (dictation, _cleanup) = crate::build_dictation(&s, dict)?;
    let recorder = vf_audio_cpal::Recorder::new(mic)?;
    eprintln!("Microphone: {}", recorder.device_name());
    let audio_tx = crate::worker::spawn(dictation, s.paste, |_| {});

    // Evenimentele de la tastatura vin pe firul lor; microfonul ramane pe firul principal.
    if !vf_hotkey::has_permission() {
        vf_hotkey::request_permission();
        eprintln!(
            "The Accessibility permission is missing. In the macOS dialog click \"Open System Settings\" and \
             enable the app you are running this from (Terminal). Waiting..."
        );
        while !vf_hotkey::has_permission() {
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
        eprintln!("Permission granted.");
    }
    let (key_tx, key_rx) = mpsc::channel::<HotkeyEvent>();
    let _listener = vf_hotkey::start(&s.hotkey, s.hotkey_mode, move |ev| {
        let _ = key_tx.send(ev);
    })?;

    let mode = match s.hotkey_mode {
        HotkeyMode::Hold => "hold",
        HotkeyMode::Toggle => "press once to start and once more to stop",
    };
    eprintln!(
        "Ready. Hotkey: {} ({mode}). Esc cancels. Ctrl+C quits.",
        s.hotkey
    );

    for ev in key_rx {
        match ev {
            HotkeyEvent::Start => {
                crate::play("Tink", s.sounds);
                recorder.start();
            }
            HotkeyEvent::Stop => {
                let samples = recorder.stop();
                crate::play("Pop", s.sounds);
                let _ = audio_tx.send(samples);
            }
            HotkeyEvent::Cancel => {
                let _ = recorder.stop();
                crate::play("Basso", s.sounds);
                eprintln!("  (cancelled)");
            }
            // Blocarea cu apasare dubla e pornita doar de daemon.
            HotkeyEvent::Lock => {}
        }
    }
    Ok(())
}
