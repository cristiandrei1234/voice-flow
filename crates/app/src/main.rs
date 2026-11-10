//! voice-flow: dictare locala.
//!
//!   voice-flow                      porneste dictarea cu pastila si iconita in bara de meniu (tine apasat Fn)
//!   voice-flow run --no-ui          doar in terminal
//!   voice-flow transcribe a.wav     transcrie fisiere
//!   voice-flow bench test-audio/ro  masoara rata de eroare pe inregistrari cu .txt alaturi
//!   voice-flow config               arata / creeaza fisierul de setari

mod bench;
mod command;
mod daemon;
mod history;
mod dictation_input;
mod llm;
mod models;
mod snippets;
mod styles;
mod terminal;
mod ui;
mod wav;
mod worker;

use anyhow::Context;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use vf_application::{Dictation, TextCleaner};
use vf_asr_sherpa::{EngineOptions, SherpaEngine};
use vf_domain::{Dictionary, EngineKind, HotkeyMode, Settings};

#[derive(Parser)]
#[command(
    name = "voice-flow",
    version,
    about = "Local dictation, 100% on your Mac"
)]
struct Cli {
    /// Settings file (default: ~/Library/Application Support/VoiceFlow/config.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Engine: parakeet-ro, parakeet-v3, parakeet-v2, whisper-turbo or whisper-small
    #[arg(long, global = true)]
    engine: Option<String>,
    /// Correct the text with the local model (mlx_lm.server / Ollama)
    #[arg(long, global = true)]
    clean: bool,
    /// Without the dictionary and hotwords
    #[arg(long, global = true)]
    no_dictionary: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start dictation with the global hotkey (default)
    Run {
        /// Hotkey: fn (default), ctrl+opt, ralt (right Option), f13, cmd+shift+space
        #[arg(long)]
        hotkey: Option<String>,
        /// Press once to start and once more to stop (instead of holding the key)
        #[arg(long)]
        toggle: bool,
        /// Only print the text, do not paste it
        #[arg(long)]
        no_paste: bool,
        /// Microphone (part of its name); default: the system microphone
        #[arg(long)]
        mic: Option<String>,
        /// No recording pill or menu bar icon: terminal only
        #[arg(long)]
        no_ui: bool,
    },
    /// Transcribe WAV files
    Transcribe { files: Vec<PathBuf> },
    /// Compare the results with the reference text (a .txt file next to each .wav)
    Bench { dir: PathBuf },
    /// The engine for the macOS app: JSON commands on stdin, JSON events on stdout
    Daemon,
    /// Print the keys you press (keyboard / permission debugging)
    Keys,
    /// Exit with 0 if the Accessibility permission is granted (asked by a fresh process)
    #[command(hide = true)]
    CheckPermission,
    /// Show the settings path; with --init, write the default settings
    Config {
        #[arg(long)]
        init: bool,
    },
}

fn config_path(cli: &Cli) -> PathBuf {
    cli.config.clone().unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("VoiceFlow/config.toml")
    })
}

fn load_settings(cli: &Cli) -> anyhow::Result<Settings> {
    let path = config_path(cli);
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut s = if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        Settings::from_toml(&text)
            .with_context(|| format!("Invalid settings in {}", path.display()))?
    } else {
        Settings::with_home(&home)
    };
    s.models_dir = expand_home(&s.models_dir, &home);
    s.dictionary_path = expand_home(&s.dictionary_path, &home);
    if let Some(e) = &cli.engine {
        s.engine = match e.as_str() {
            "ro" => EngineKind::ParakeetRo,
            "parakeet" | "v3" => EngineKind::ParakeetV3,
            other => match vf_domain::ModelSpec::by_id(other) {
                Some(m) => m.kind,
                None => {
                    let ids: Vec<_> = vf_domain::models::catalog().iter().map(|m| m.id).collect();
                    anyhow::bail!("Unknown engine: {other} ({})", ids.join(", "))
                }
            },
        };
    }
    if cli.clean {
        s.cleanup.enabled = true;
    }
    Ok(s)
}

fn expand_home(p: &Path, home: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => p.to_path_buf(),
    }
}

fn load_dictionary(cli: &Cli, s: &Settings) -> Dictionary {
    if cli.no_dictionary {
        return Dictionary::default();
    }
    std::fs::read_to_string(&s.dictionary_path)
        .map(|t| Dictionary::parse(&t))
        .unwrap_or_default()
}

/// Tine locul motorului de transcriere cat timp niciun model nu e instalat.
struct MissingModel;

impl vf_application::SpeechToText for MissingModel {
    fn name(&self) -> &str {
        "no model installed"
    }

    fn transcribe(&self, _samples: &[f32]) -> anyhow::Result<String> {
        anyhow::bail!("No transcription model is installed. Download one in Transcription.")
    }
}

/// Construieste fluxul de dictare. Al doilea element porneste / opreste curatarea.
fn build_dictation(s: &Settings, dict: Dictionary) -> anyhow::Result<(Dictation, Arc<AtomicBool>)> {
    build_dictation_styled(s, dict, styles::Shared::new(s))
}

/// La fel, cu starea stilurilor data din afara (daemonul o schimba intre dictari).
fn build_dictation_styled(
    s: &Settings,
    dict: Dictionary,
    shared: styles::Shared,
) -> anyhow::Result<(Dictation, Arc<AtomicBool>)> {
    let t = Instant::now();
    // La prima pornire nu exista niciun model: aplicatia trebuie sa porneasca oricum,
    // ca sa poti descarca unul din Transcription.
    let engine: Box<dyn vf_application::SpeechToText> =
        if vf_domain::ModelSpec::of(s.engine).installed(&s.models_dir) {
            Box::new(SherpaEngine::new(EngineOptions {
                kind: s.engine,
                models_dir: &s.models_dir,
                threads: s.threads,
                dictionary: Some(&dict),
                hotwords_score: s.hotwords_score,
                whisper_language: &s.whisper_language,
            })?)
        } else {
            eprintln!(
                "Model: {} is not installed; download it in Transcription",
                vf_domain::ModelSpec::of(s.engine).title
            );
            Box::new(MissingModel)
        };
    eprintln!(
        "Model: {} (loaded in {:.1}s)",
        engine.name(),
        t.elapsed().as_secs_f32()
    );
    eprintln!("Dictionary: {} terms", dict.terms().len());
    let enabled = Arc::new(AtomicBool::new(s.cleanup.enabled));
    if s.cleanup.enabled {
        eprintln!("Corrections: {} at {}", s.cleanup.model, s.cleanup.url);
    }
    let cleaner: Box<dyn TextCleaner> = Box::new(styles::StyleCleaner {
        inner: vf_llm_openai::OpenAiCleaner::new(&s.cleanup),
        enabled: enabled.clone(),
        shared,
    });
    // Incalzire: primul apel e mai lent.
    let _ = engine.transcribe(&vec![0.0; 8000]);
    Ok((
        Dictation::new(dictation_input::wrap(engine, s), dict).with_cleaner(Some(cleaner)),
        enabled,
    ))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Inainte de setari: e pornit des si trebuie sa fie rapid.
    if matches!(cli.cmd, Some(Cmd::CheckPermission)) {
        std::process::exit(if vf_hotkey::has_permission() { 0 } else { 1 });
    }
    let mut settings = load_settings(&cli)?;

    match &cli.cmd {
        Some(Cmd::Config { init }) => {
            let path = config_path(&cli);
            if *init && !path.exists() {
                write_default_config(&path)?;
                println!("Wrote the default settings to {}", path.display());
            } else {
                println!("{}", path.display());
                println!("\n{}", settings.to_toml());
            }
            Ok(())
        }
        Some(Cmd::Transcribe { files }) => {
            let dict = load_dictionary(&cli, &settings);
            let (d, _) = build_dictation(&settings, dict)?;
            for f in files {
                let samples = wav::read_16k_mono(f)?;
                match d.process(&samples)? {
                    Some(r) => println!("{}\t{}", f.display(), r.text),
                    None => println!("{}\t(nothing)", f.display()),
                }
            }
            Ok(())
        }
        Some(Cmd::Daemon) => {
            let dict = load_dictionary(&cli, &settings);
            daemon::run(settings, dict, config_path(&cli))
        }
        Some(Cmd::CheckPermission) => unreachable!("handled above"),
        Some(Cmd::Keys) => {
            if !vf_hotkey::has_permission() {
                vf_hotkey::request_permission();
                eprintln!("The Accessibility permission is missing for the app you are running this from.");
            }
            vf_hotkey::debug_keys()
        }
        Some(Cmd::Bench { dir }) => bench::run(dir, &settings, load_dictionary(&cli, &settings)),
        Some(Cmd::Run {
            hotkey,
            toggle,
            no_paste,
            mic,
            no_ui,
        }) => {
            let dict = load_dictionary(&cli, &settings);
            if let Some(h) = hotkey {
                settings.hotkey = h.clone();
            }
            if *toggle {
                settings.hotkey_mode = HotkeyMode::Toggle;
            }
            if *no_paste {
                settings.paste = false;
            }
            if *no_ui {
                terminal::run(settings, dict, mic.as_deref())
            } else {
                ui::run(settings, dict, mic.as_deref(), config_path(&cli))
            }
        }
        None => {
            let dict = load_dictionary(&cli, &settings);
            ui::run(settings, dict, None, config_path(&cli))
        }
    }
}

fn write_default_config(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    std::fs::write(path, Settings::with_home(&home).to_toml())?;
    Ok(())
}

fn play(sound: &str, enabled: bool) {
    if enabled && cfg!(target_os = "macos") {
        let _ = std::process::Command::new("afplay")
            .arg(format!("/System/Library/Sounds/{sound}.aiff"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Deschide un fisier sau folder cu aplicatia implicita (TextEdit pentru .txt / .toml).
fn open_path(path: &Path, text: bool) {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        if text {
            c.arg("-t");
        }
        c
    };
    #[cfg(not(target_os = "macos"))]
    let mut cmd = {
        let _ = text;
        std::process::Command::new("xdg-open")
    };
    let _ = cmd.arg(path).spawn();
}
