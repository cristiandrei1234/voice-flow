//! `voice-flow daemon`: motorul pentru aplicatia macOS (SwiftUI).
//!
//! Protocol: un obiect JSON pe linie.
//!   stdin  -> comenzi:      {"cmd":"set_hotkey","hotkey":"fn, CmdRight"}
//!   stdout -> evenimente:   {"type":"state","state":"recording"}
//! Logurile merg pe stderr. Cand stdin se inchide (aplicatia a iesit), daemonul se opreste.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vf_domain::{Dictionary, EngineKind, HotkeyMode, Settings};
use vf_hotkey::HotkeyEvent;

use crate::worker::Outcome;

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Porneste / opreste inregistrarea din interfata (butonul).
    Start,
    Stop,
    Toggle,
    Cancel,
    SetHotkey {
        hotkey: String,
    },
    /// Asteapta urmatoarea tasta / combinatie apasata si o trimite ca eveniment `hotkey_recorded`.
    RecordHotkey,
    SetMode {
        mode: HotkeyMode,
    },
    SetEngine {
        engine: EngineKind,
    },
    SetCleanup {
        enabled: bool,
    },
    SetSounds {
        enabled: bool,
    },
    SetPaste {
        enabled: bool,
    },
    /// Blocare fara maini la apasare dubla (in modul "tii apasat").
    SetDoubleTapLock {
        enabled: bool,
    },
    /// Amplifica inregistrarile incete (soapta) inainte de transcriere.
    SetNormalizeQuiet {
        enabled: bool,
    },
    /// Limba pentru modelele Whisper; "" = detectie automata.
    SetWhisperLanguage {
        language: String,
    },
    ReloadDictionary,
    ListDictionary,
    /// Adauga (fara `line`) sau modifica (cu `line` + `expected`) un termen.
    SaveEntry {
        line: Option<usize>,
        #[serde(default)]
        expected: String,
        correct: String,
        #[serde(default)]
        aliases: Vec<String>,
        group: Option<String>,
    },
    DeleteEntry {
        line: usize,
        expected: String,
    },
    ListSnippets,
    /// Adauga (fara `id`) sau modifica (cu `id`) un snippet.
    SaveSnippet {
        id: Option<String>,
        trigger: String,
        #[serde(default)]
        variants: Vec<String>,
        text: String,
        #[serde(default = "crate::snippets::enabled_by_default")]
        enabled: bool,
    },
    DeleteSnippet {
        id: String,
    },
    ListLlmModels,
    DownloadLlm {
        repo: String,
    },
    CancelLlmDownload {
        repo: String,
    },
    DeleteLlm {
        repo: String,
    },
    SetCleanupModel {
        model: String,
    },
    ListModels,
    DownloadModel {
        engine: String,
    },
    CancelDownload {
        engine: String,
    },
    DeleteModel {
        engine: String,
    },
    RequestPermission,
    OpenPermissionSettings,
    GetState,
    Quit,
    /// O pagina din istoric, de la cea mai noua dictare; `before` = id-ul ultimei primite.
    ListHistory {
        query: Option<String>,
        limit: usize,
        before: Option<String>,
    },
    DeleteHistory {
        id: String,
    },
    ClearHistory,
    SetHistoryEnabled {
        enabled: bool,
    },
    SetHistoryRetention {
        days: u32,
    },
    /// `utc_offset` = decalajul fusului local in secunde (zilele se numara dupa ora locala).
    GetStats {
        #[serde(default)]
        utc_offset: i64,
    },
    /// Stiluri: aplicatia din fata (anuntata de interfata la fiecare schimbare).
    FrontmostApp {
        bundle_id: String,
        #[serde(default)]
        name: String,
    },
    SetCleanupLevel {
        level: vf_domain::CleanupLevel,
    },
    SetStylesEnabled {
        enabled: bool,
    },
    /// Schimba instructiunea unui stil; fara `instruction` revine la cea implicita.
    SaveStyle {
        id: vf_domain::StyleId,
        instruction: Option<String>,
    },
    AssignApp {
        bundle_id: String,
        #[serde(default)]
        name: String,
        style: vf_domain::StyleId,
    },
    UnassignApp {
        bundle_id: String,
    },
    ListStyles,
    /// Command Mode (vezi `impl Daemon` de la sfarsitul fisierului si command.rs).
    SetCommandEnabled {
        enabled: bool,
    },
    SetCommandHotkey {
        hotkey: String,
    },
    /// Prompt gol = revine la cel implicit.
    SetCommandPrompt {
        prompt: String,
    },
    /// Ca `RecordHotkey`, dar raspunde cu `command_hotkey_recorded`.
    RecordCommandHotkey,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event<'a> {
    Ready {
        settings: &'a Settings,
        config_path: &'a str,
        model: &'a str,
        microphone: Option<&'a str>,
        dictionary_terms: usize,
    },
    Permission {
        accessibility: bool,
        keyboard: bool,
    },
    State {
        state: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<&'a str>,
    },
    Level {
        value: f32,
    },
    /// Inregistrarea in curs a fost blocata cu apasare dubla (fara maini). Se termina odata cu
    /// inregistrarea: urmatorul `state` diferit de "recording" inseamna ca nu mai e blocata.
    HandsFree {
        active: bool,
    },
    Result {
        text: &'a str,
        raw: &'a str,
        audio_ms: u64,
        asr_ms: u64,
        cleanup_ms: Option<u64>,
    },
    HotkeyRecorded {
        hotkey: Option<&'a str>,
    },
    Settings {
        settings: &'a Settings,
    },
    Microphone {
        name: Option<&'a str>,
    },
    Dictionary {
        entries: &'a [vf_domain::FileEntry],
        groups: &'a [String],
        /// Fisierul s-a schimbat fata de ce foloseste motorul acum (se aplica la reincarcare).
        changed: bool,
    },
    Snippets {
        snippets: &'a [vf_domain::Snippet],
    },
    LlmModels {
        models: &'a [crate::llm::LlmInfo],
    },
    LlmProgress {
        repo: &'a str,
        received: u64,
        total: u64,
    },
    /// Starea serverului de corecturi: stopped / starting / ready / external / error.
    CleanupServer {
        state: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<&'a str>,
    },
    Models {
        models: &'a [crate::models::ModelInfo],
    },
    ModelProgress {
        id: &'a str,
        received: u64,
        total: u64,
    },
    /// Setarea cere repornirea motorului (ex. alt model); aplicatia reporneste daemonul.
    RestartRequired,
    Error {
        message: &'a str,
    },
    History {
        items: &'a [vf_domain::HistoryRecord],
        has_more: bool,
        query: &'a str,
    },
    HistoryStats {
        #[serde(flatten)]
        stats: &'a vf_domain::HistoryStats,
        /// Unde e fisierul cu istoricul.
        path: &'a str,
    },
    /// O dictare noua a intrat in istoric.
    HistoryAdded {
        item: &'a vf_domain::HistoryRecord,
    },
    /// Nivelul de corectare, stilurile cu aplicatiile lor si aplicatia din fata.
    Styles {
        level: vf_domain::CleanupLevel,
        enabled: bool,
        styles: &'a [vf_domain::styles::StyleView],
        recent_apps: &'a [vf_domain::styles::AppRef],
        current_app: Option<&'a vf_domain::styles::AppRef>,
        current_style: vf_domain::StyleId,
    },
    /// Stilul primit de dictarea al carei `result` urmeaza (doar cand corecturile sunt pornite).
    StyleApplied {
        style: vf_domain::StyleId,
        style_name: &'a str,
        level: vf_domain::CleanupLevel,
        /// `false`: textul nu a trecut prin model (stilul "Code").
        cleaned: bool,
        app: Option<&'a str>,
        bundle_id: Option<&'a str>,
    },
    /// Starea Command Mode; `problem` = de ce scurtatura nu e activa (ex. conflict cu dictarea).
    CommandMode {
        enabled: bool,
        hotkey: &'a str,
        prompt: &'a str,
        prompt_is_default: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        problem: Option<&'a str>,
    },
    CommandHotkeyRecorded {
        hotkey: Option<&'a str>,
    },
    CommandResult {
        instruction: &'a str,
        before: &'a str,
        after: &'a str,
    },
    CommandFailed {
        message: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        instruction: Option<&'a str>,
    },
}

fn emit(ev: &Event) {
    static OUT: Mutex<()> = Mutex::new(());
    let _guard = OUT.lock().unwrap();
    let mut out = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut out, ev);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

enum Msg {
    Cmd(Command),
    Key(HotkeyEvent),
    Done(Outcome),
    Recorded(Option<String>),
    Tick,
    RetryKeyboard,
    Mic(Result<vf_audio_cpal::RecorderHandle, String>),
    LlmProgress(String, u64, u64),
    LlmDone(String, Result<(), String>),
    /// Serverul de corecturi a raspuns (sau nu) dupa pornire; `u64` e generatia pornirii.
    LlmReady(u64, bool),
    ModelProgress(&'static str, u64, u64),
    ModelDone(&'static str, Result<(), String>),
    StdinClosed,
    CommandKey(vf_hotkey::command::CommandKey),
    CommandDone(vf_application::command::CommandOutcome),
    CommandRecorded(Option<String>),
}

struct Daemon {
    s: Settings,
    config_path: PathBuf,
    tx: Sender<Msg>,
    recorder: Option<vf_audio_cpal::RecorderHandle>,
    audio_tx: Sender<Vec<f32>>,
    cleanup: Arc<AtomicBool>,
    snippets: crate::snippets::Store,
    listener: Option<vf_hotkey::HotkeyListener>,
    recording: bool,
    processing: bool,
    capturing_hotkey: bool,
    keyboard_error: Option<String>,
    /// Descarcarile in curs, cu steagul de anulare al fiecareia.
    downloads: HashMap<&'static str, Arc<AtomicBool>>,
    llm_downloads: HashMap<String, Arc<AtomicBool>>,
    llm_server: Option<crate::llm::Server>,
    /// Creste la fiecare pornire / oprire, ca raspunsurile vechi sa fie ignorate.
    llm_generation: u64,
    styles: crate::styles::Shared,
    command_tx: Sender<crate::command::Job>,
    /// Ultima eroare de microfon si momentul urmatoarei incercari de a-l deschide.
    mic_error: Option<String>,
    mic_retry_at: Option<std::time::Instant>,
}

impl Daemon {
    fn history(&self) -> crate::history::Store {
        crate::history::Store::for_config(&self.config_path)
    }

    fn read_dictionary(&self) -> vf_domain::DictionaryFile {
        vf_domain::DictionaryFile::parse(
            &std::fs::read_to_string(&self.s.dictionary_path).unwrap_or_default(),
        )
    }

    fn emit_dictionary(&self, changed: bool) {
        let file = self.read_dictionary();
        emit(&Event::Dictionary {
            entries: &file.entries(),
            groups: &file.groups(),
            changed,
        });
    }

    /// Aplica o modificare si scrie fisierul (prin fisier temporar, ca sa nu ramana pe jumatate).
    fn edit_dictionary(&mut self, edit: impl FnOnce(&mut vf_domain::DictionaryFile) -> Result<(), String>) {
        let path = self.s.dictionary_path.clone();
        let mut file = self.read_dictionary();
        let result = edit(&mut file).and_then(|()| {
            // O singura copie de siguranta, inainte de prima editare din aplicatie.
            let backup = path.with_extension("txt.bak");
            if path.exists() && !backup.exists() {
                let _ = std::fs::copy(&path, &backup);
            }
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let tmp = path.with_extension("txt.tmp");
            std::fs::write(&tmp, file.to_text())
                .and_then(|()| std::fs::rename(&tmp, &path))
                .map_err(|e| format!("Could not save the dictionary: {e}"))
        });
        match result {
            Ok(()) => self.emit_dictionary(true),
            Err(e) => {
                emit(&Event::Error { message: &e });
                self.emit_dictionary(false);
            }
        }
    }

    /// Eroarea (daca e) inaintea listei: interfata stie astfel ca schimbarea ceruta nu s-a facut.
    fn emit_snippets(&self, reply: crate::snippets::Reply) {
        if let Some(e) = &reply.error {
            emit(&Event::Error { message: e });
        }
        emit(&Event::Snippets { snippets: &reply.snippets });
    }

    fn emit_llm_models(&self) {
        let downloading: Vec<&str> = self.llm_downloads.keys().map(|s| s.as_str()).collect();
        let models = crate::llm::list(&self.s.models_dir, &self.s.cleanup.model, &downloading);
        emit(&Event::LlmModels { models: &models });
    }

    fn stop_llm(&mut self) {
        self.llm_generation += 1;
        if let Some(server) = self.llm_server.take() {
            eprintln!("Stopping the corrections server");
            server.stop();
        }
        emit(&Event::CleanupServer { state: "stopped", message: None });
    }

    /// Porneste serverul de corecturi daca e cazul (curatare activa, gestionat de noi).
    fn sync_llm(&mut self) {
        let c = self.s.cleanup.clone();
        if !crate::command::needs_llm(&self.s) {
            if self.llm_server.is_some() {
                self.stop_llm();
            }
            return;
        }
        if let Some(server) = &mut self.llm_server {
            if server.alive() && server.model == c.model {
                return;
            }
        }
        self.stop_llm();
        match crate::llm::start(&c.url, &c.model, &c.server_command) {
            Ok(crate::llm::Started::External) => {
                eprintln!("Corrections: using the server already running at {}", c.url);
                emit(&Event::CleanupServer { state: "external", message: None });
            }
            Ok(crate::llm::Started::Spawned(server)) => {
                eprintln!("Corrections: starting the server with {}", c.model);
                self.llm_server = Some(server);
                emit(&Event::CleanupServer { state: "starting", message: None });
                let generation = self.llm_generation;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let ok = crate::llm::wait_ready(&c.url, Duration::from_secs(240));
                    let _ = tx.send(Msg::LlmReady(generation, ok));
                });
            }
            Err(e) => {
                let msg = format!("{e:#}");
                eprintln!("Corrections: {msg}");
                emit(&Event::CleanupServer { state: "error", message: Some(&msg) });
            }
        }
    }

    fn emit_models(&self) {
        let downloading: Vec<&str> = self.downloads.keys().copied().collect();
        let models = crate::models::list(&self.s.models_dir, self.s.engine, &downloading);
        emit(&Event::Models { models: &models });
    }

    fn save(&self) {
        if let Some(dir) = self.config_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&self.config_path, self.s.to_toml()) {
            emit(&Event::Error {
                message: &format!("Could not save the settings: {e}"),
            });
        }
        emit(&Event::Settings { settings: &self.s });
    }

    fn state(&self, state: &str, message: Option<&str>) {
        emit(&Event::State { state, message });
    }

    fn start_keyboard(&mut self) {
        self.listener = None;
        vf_hotkey::set_double_tap_lock(self.s.dictation.double_tap_lock);
        let has = vf_hotkey::has_permission();
        if has {
            let tx = self.tx.clone();
            let command_hotkey = crate::command::hotkey(&self.s).unwrap_or(None);
            match vf_hotkey::start_with_command(&self.s.hotkey, self.s.hotkey_mode, command_hotkey.as_deref(), move |ev| {
                let _ = tx.send(Msg::from(ev));
            }) {
                Ok(l) => {
                    self.listener = Some(l);
                    self.keyboard_error = None;
                    eprintln!("Keyboard: OK ({})", self.s.hotkey);
                }
                Err(e) => {
                    // La reincercari nu repetam aceeasi eroare.
                    let msg = format!("{e:#}");
                    if self.keyboard_error.as_deref() == Some(msg.as_str()) {
                        return;
                    }
                    eprintln!("{msg}");
                    emit(&Event::Error { message: &msg });
                    self.keyboard_error = Some(msg);
                }
            }
        }
        emit(&Event::Permission {
            accessibility: has,
            keyboard: self.listener.is_some(),
        });
        self.emit_command_mode();
    }

    fn begin(&mut self) {
        if self.recording || self.capturing_hotkey {
            return;
        }
        let Some(rec) = &self.recorder else {
            self.state("error", Some("The microphone is not ready yet"));
            return;
        };
        crate::play("Tink", self.s.sounds);
        rec.start();
        self.recording = true;
        self.state("recording", None);
    }

    fn finish(&mut self) {
        if !self.recording {
            return;
        }
        let samples = self.recorder.as_ref().map(|r| r.stop()).unwrap_or_default();
        crate::play("Pop", self.s.sounds);
        self.recording = false;
        self.processing = true;
        self.state("processing", None);
        let _ = self.audio_tx.send(samples);
    }

    fn cancel(&mut self) {
        if !self.recording {
            return;
        }
        if let Some(r) = &self.recorder {
            let _ = r.stop();
        }
        crate::play("Basso", self.s.sounds);
        self.recording = false;
        self.state("idle", Some(CANCELLED_MESSAGE));
    }

    fn handle(&mut self, msg: Msg) -> bool {
        match msg {
            Msg::Key(ev) => {
                if self.capturing_hotkey {
                    return true;
                }
                match ev {
                    HotkeyEvent::Start => self.begin(),
                    HotkeyEvent::Stop => self.finish(),
                    HotkeyEvent::Cancel => self.cancel(),
                    HotkeyEvent::Lock => {
                        if self.recording {
                            emit(&Event::HandsFree { active: true });
                        }
                    }
                }
            }
            Msg::Tick => {
                if self.recording {
                    let level = self.recorder.as_ref().map(|r| r.level()).unwrap_or(0.0);
                    emit(&Event::Level {
                        value: (level * 14.0).min(1.0).powf(0.6),
                    });
                }
            }
            Msg::Done(outcome) => {
                self.processing = false;
                match outcome {
                    Outcome::Pasted {
                        text,
                        raw,
                        audio_ms,
                        asr_ms,
                        cleanup_ms,
                    } => {
                        self.emit_style_applied();
                        emit(&Event::Result {
                            text: &text,
                            raw: &raw,
                            audio_ms,
                            asr_ms,
                            cleanup_ms,
                        });
                        // Aplicatia in care s-a dictat (o stim de la detectarea pentru stiluri).
                        let app = self.styles.with(|st| st.current_app.as_ref().map(|a| a.name.clone()));
                        if let Some(item) = crate::history::record(&self.config_path, &self.s.history, &text, &raw, crate::history::Timing { audio_ms, asr_ms, cleanup_ms }, app) {
                            emit(&Event::HistoryAdded { item: &item });
                        }
                        if !self.recording {
                            self.state("idle", None);
                        }
                    }
                    Outcome::Nothing => {
                        if !self.recording {
                            self.state("idle", Some(NO_SPEECH_MESSAGE));
                        }
                    }
                    Outcome::Failed(e) => self.state("error", Some(&e)),
                }
            }
            Msg::Recorded(hotkey) => {
                self.capturing_hotkey = false;
                emit(&Event::HotkeyRecorded {
                    hotkey: hotkey.as_deref(),
                });
            }
            Msg::Mic(Ok(rec)) => {
                self.mic_error = None;
                self.mic_retry_at = None;
                eprintln!("Microphone: {}", rec.device_name());
                emit(&Event::Microphone {
                    name: Some(rec.device_name()),
                });
                self.recorder = Some(rec);
            }
            Msg::Mic(Err(e)) => {
                // Se reincearca singur (vezi RetryKeyboard): de ex. dupa deblocarea ecranului.
                // Aceeasi eroare nu o mai anuntam la fiecare incercare.
                if self.mic_error.as_deref() != Some(e.as_str()) {
                    eprintln!("Microphone unavailable: {e}");
                    emit(&Event::Microphone { name: None });
                    emit(&Event::Error { message: &format!("Microphone unavailable: {e}") });
                    self.mic_error = Some(e);
                }
                self.mic_retry_at = Some(std::time::Instant::now() + Duration::from_secs(5));
            }
            Msg::LlmProgress(repo, received, total) => {
                emit(&Event::LlmProgress { repo: &repo, received, total });
            }
            Msg::LlmDone(repo, result) => {
                self.llm_downloads.remove(&repo);
                match result {
                    Ok(()) => eprintln!("Corrections model downloaded: {repo}"),
                    Err(e) if e == crate::models::CANCELLED => eprintln!("Download cancelled: {repo}"),
                    Err(e) => {
                        eprintln!("Download failed ({repo}): {e}");
                        emit(&Event::Error { message: &e });
                    }
                }
                self.emit_llm_models();
            }
            Msg::LlmReady(generation, ok) => {
                if generation == self.llm_generation {
                    let alive = self.llm_server.as_mut().map(|s| s.alive()).unwrap_or(false);
                    if ok && alive {
                        eprintln!("Corrections: the server is ready");
                        emit(&Event::CleanupServer { state: "ready", message: None });
                    } else {
                        let msg = "The corrections server did not start; see ~/Library/Logs/VoiceFlowLLM.log";
                        eprintln!("Corrections: {msg}");
                        if let Some(server) = self.llm_server.take() {
                            server.stop();
                        }
                        emit(&Event::CleanupServer { state: "error", message: Some(msg) });
                    }
                }
            }
            Msg::ModelProgress(id, received, total) => {
                emit(&Event::ModelProgress { id, received, total });
            }
            Msg::ModelDone(id, result) => {
                self.downloads.remove(id);
                match result {
                    Ok(()) => {
                        eprintln!("Model downloaded: {id}");
                        // Modelul ales tocmai a devenit disponibil (prima pornire): il incarcam.
                        if vf_domain::ModelSpec::of(self.s.engine).id == id && !self.recording && !self.processing {
                            self.emit_models();
                            emit(&Event::RestartRequired);
                            return true;
                        }
                    }
                    Err(e) if e == crate::models::CANCELLED => eprintln!("Download cancelled: {id}"),
                    Err(e) => {
                        eprintln!("Download failed ({id}): {e}");
                        emit(&Event::Error { message: &e });
                    }
                }
                self.emit_models();
            }
            Msg::RetryKeyboard => {
                if self.recorder.is_none()
                    && self.mic_retry_at.is_some_and(|t| std::time::Instant::now() >= t)
                {
                    self.mic_retry_at = None;
                    let mic_tx = self.tx.clone();
                    vf_audio_cpal::spawn(None, move |r| {
                        let _ = mic_tx.send(Msg::Mic(r.map_err(|e| format!("{e:#}"))));
                    });
                }
                if self.listener.is_none() {
                    if vf_hotkey::has_permission() {
                        self.start_keyboard();
                    } else if !self.recording && !self.processing && granted_in_fresh_process() {
                        restart_for_permission();
                    }
                }
            }
            Msg::StdinClosed => return false,
            Msg::CommandKey(key) => self.command_key(key),
            Msg::CommandDone(outcome) => self.command_done(outcome),
            Msg::CommandRecorded(hotkey) => {
                self.capturing_hotkey = false;
                emit(&Event::CommandHotkeyRecorded { hotkey: hotkey.as_deref() });
            }
            Msg::Cmd(cmd) => return self.command(cmd),
        }
        true
    }

    fn command(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::Start => self.begin(),
            Command::Stop => self.finish(),
            Command::Toggle => {
                if self.recording {
                    self.finish()
                } else {
                    self.begin()
                }
            }
            Command::Cancel => self.cancel(),
            Command::SetHotkey { hotkey } => match vf_hotkey::parse_hotkeys(&hotkey) {
                Ok(_) => {
                    self.s.hotkey = hotkey;
                    self.save();
                    self.start_keyboard();
                }
                Err(e) => emit(&Event::Error {
                    message: &format!("{e:#}"),
                }),
            },
            Command::RecordHotkey => {
                if !self.capturing_hotkey {
                    self.capturing_hotkey = true;
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        let r =
                            vf_hotkey::record_once(Duration::from_secs(10)).unwrap_or_else(|e| {
                                eprintln!("{e:#}");
                                None
                            });
                        let _ = tx.send(Msg::Recorded(r));
                    });
                }
            }
            Command::SetMode { mode } => {
                self.s.hotkey_mode = mode;
                self.save();
                self.start_keyboard();
            }
            Command::SetEngine { engine } => {
                let spec = vf_domain::ModelSpec::of(engine);
                if !spec.installed(&self.s.models_dir) {
                    emit(&Event::Error {
                        message: &format!("{} is not installed yet", spec.title),
                    });
                } else if self.s.engine != engine {
                    self.s.engine = engine;
                    self.save();
                    emit(&Event::RestartRequired);
                }
            }
            Command::ListModels => self.emit_models(),
            Command::DownloadModel { engine } => {
                if let Some(spec) = vf_domain::ModelSpec::by_id(&engine) {
                    if !self.downloads.contains_key(spec.id) && !spec.installed(&self.s.models_dir) {
                        let cancel = Arc::new(AtomicBool::new(false));
                        self.downloads.insert(spec.id, cancel.clone());
                        let tx = self.tx.clone();
                        let dir = self.s.models_dir.clone();
                        std::thread::spawn(move || {
                            let ptx = tx.clone();
                            let r = crate::models::download(spec, &dir, &cancel, |got, total| {
                                let _ = ptx.send(Msg::ModelProgress(spec.id, got, total));
                            });
                            let _ = tx.send(Msg::ModelDone(spec.id, r.map_err(|e| format!("{e:#}"))));
                        });
                    }
                    self.emit_models();
                }
            }
            Command::CancelDownload { engine } => {
                if let Some(flag) = self.downloads.get(engine.as_str()) {
                    flag.store(true, Ordering::Relaxed);
                }
            }
            Command::DeleteModel { engine } => {
                if let Some(spec) = vf_domain::ModelSpec::by_id(&engine) {
                    if self.downloads.contains_key(spec.id) {
                        emit(&Event::Error { message: "Cancel the download first" });
                    } else if let Err(e) = crate::models::delete(spec, &self.s.models_dir, self.s.engine) {
                        emit(&Event::Error { message: &format!("{e:#}") });
                    }
                    self.emit_models();
                }
            }
            Command::ListDictionary => self.emit_dictionary(false),
            Command::SaveEntry { line, expected, correct, aliases, group } => {
                self.edit_dictionary(|f| match line {
                    Some(line) => f.update(line, &expected, &correct, &aliases),
                    None => f.add(&correct, &aliases, group.as_deref()),
                });
            }
            Command::DeleteEntry { line, expected } => {
                self.edit_dictionary(|f| f.delete(line, &expected));
            }
            Command::ListSnippets => self.emit_snippets(self.snippets.list()),
            Command::SaveSnippet { id, trigger, variants, text, enabled } => {
                let draft = vf_domain::SnippetDraft { trigger, variants, text, enabled };
                self.emit_snippets(self.snippets.save(id.as_deref(), &draft));
            }
            Command::DeleteSnippet { id } => self.emit_snippets(self.snippets.delete(&id)),
            Command::ListLlmModels => self.emit_llm_models(),
            Command::DownloadLlm { repo } => {
                if !self.llm_downloads.contains_key(&repo) {
                    let cancel = Arc::new(AtomicBool::new(false));
                    self.llm_downloads.insert(repo.clone(), cancel.clone());
                    let tx = self.tx.clone();
                    let dir = self.s.models_dir.clone();
                    std::thread::spawn(move || {
                        let ptx = tx.clone();
                        let key = repo.clone();
                        let r = crate::llm::download(&repo, &dir, &cancel, |got, total| {
                            let _ = ptx.send(Msg::LlmProgress(key.clone(), got, total));
                        });
                        let _ = tx.send(Msg::LlmDone(repo, r.map_err(|e| format!("{e:#}"))));
                    });
                }
                self.emit_llm_models();
            }
            Command::CancelLlmDownload { repo } => {
                if let Some(flag) = self.llm_downloads.get(&repo) {
                    flag.store(true, Ordering::Relaxed);
                }
            }
            Command::DeleteLlm { repo } => {
                if self.llm_downloads.contains_key(&repo) {
                    emit(&Event::Error { message: "Cancel the download first" });
                } else if let Err(e) = crate::llm::delete(&repo, &self.s.models_dir, &self.s.cleanup.model) {
                    emit(&Event::Error { message: &format!("{e:#}") });
                }
                self.emit_llm_models();
            }
            Command::SetCleanupModel { model } => {
                if self.s.cleanup.model != model {
                    self.s.cleanup.model = model;
                    self.save();
                    // Curatarea tine minte modelul de la pornire: repornim motorul cu cel nou.
                    self.stop_llm();
                    emit(&Event::RestartRequired);
                }
            }
            Command::SetCleanup { enabled } => {
                self.s.cleanup.enabled = enabled;
                self.cleanup.store(enabled, Ordering::Relaxed);
                self.sync_llm();
                self.save();
            }
            Command::SetSounds { enabled } => {
                self.s.sounds = enabled;
                self.save();
            }
            Command::SetPaste { enabled } => {
                self.s.paste = enabled;
                self.save();
                emit(&Event::RestartRequired);
            }
            Command::SetDoubleTapLock { enabled } => {
                self.s.dictation.double_tap_lock = enabled;
                self.save();
                self.start_keyboard();
            }
            Command::SetNormalizeQuiet { enabled } => {
                self.s.dictation.normalize_quiet_audio = enabled;
                crate::dictation_input::set_normalize_quiet(enabled);
                self.save();
            }
            Command::SetWhisperLanguage { language } => {
                match vf_domain::dictation::parse_whisper_language(&language) {
                    Ok(lang) if lang != self.s.whisper_language => {
                        self.s.whisper_language = lang.to_string();
                        self.save();
                        // Limba e data modelului la incarcare: doar Whisper trebuie repornit.
                        let spec = vf_domain::ModelSpec::of(self.s.engine);
                        if spec.family == vf_domain::ModelFamily::Whisper {
                            emit(&Event::RestartRequired);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => emit(&Event::Error { message: &e }),
                }
            }
            Command::ReloadDictionary => emit(&Event::RestartRequired),
            Command::RequestPermission => {
                vf_hotkey::request_permission();
            }
            Command::OpenPermissionSettings => {
                vf_hotkey::request_permission();
                vf_hotkey::open_permission_settings();
            }
            Command::GetState => {
                emit(&Event::Settings { settings: &self.s });
                emit(&Event::Permission {
                    accessibility: vf_hotkey::has_permission(),
                    keyboard: self.listener.is_some(),
                });
                let st = if self.recording {
                    "recording"
                } else if self.processing {
                    "processing"
                } else {
                    "idle"
                };
                self.state(st, None);
            }
            Command::Quit => return false,
            Command::ListHistory { query, limit, before } => {
                let query = query.unwrap_or_default();
                let page = self.history().list(&query, limit, before.as_deref());
                emit(&Event::History { items: &page.items, has_more: page.has_more, query: &query });
            }
            Command::DeleteHistory { id } => {
                if let Err(e) = self.history().delete(&id) {
                    emit(&Event::Error { message: &format!("Could not delete from the history: {e}") });
                }
            }
            Command::ClearHistory => {
                if let Err(e) = self.history().clear() {
                    emit(&Event::Error { message: &format!("Could not clear the history: {e}") });
                }
            }
            Command::SetHistoryEnabled { enabled } => {
                self.s.history.enabled = enabled;
                self.save();
            }
            Command::SetHistoryRetention { days } => {
                self.s.history.retention_days = days;
                self.save();
            }
            Command::GetStats { utc_offset } => {
                let store = self.history();
                emit(&Event::HistoryStats {
                    stats: &crate::history::stats(&self.config_path, utc_offset),
                    path: &store.path().to_string_lossy(),
                });
            }
            cmd @ (Command::FrontmostApp { .. }
            | Command::SetCleanupLevel { .. }
            | Command::SetStylesEnabled { .. }
            | Command::SaveStyle { .. }
            | Command::AssignApp { .. }
            | Command::UnassignApp { .. }
            | Command::ListStyles) => self.styles_command(cmd),
            Command::SetCommandEnabled { enabled } => self.set_command_enabled(enabled),
            Command::SetCommandHotkey { hotkey } => self.set_command_hotkey(hotkey),
            Command::SetCommandPrompt { prompt } => self.set_command_prompt(prompt),
            Command::RecordCommandHotkey => self.record_command_hotkey(),
        }
        true
    }
}

/// Stiluri: nivelul de corectare si stilul pe aplicatie (logica e in `vf_domain::styles`).
impl Daemon {
    fn emit_styles(&self) {
        self.styles.with(|st| {
            emit(&Event::Styles {
                level: st.level,
                enabled: st.styles.enabled,
                styles: &st.styles.views(),
                recent_apps: st.recent.list(),
                current_app: st.current_app.as_ref(),
                current_style: st.current_style(),
            })
        });
    }

    fn emit_style_applied(&self) {
        if let Some(a) = self.styles.with(|st| st.last_applied.take()) {
            emit(&Event::StyleApplied {
                style: a.style,
                style_name: a.style.title(),
                level: a.level,
                cleaned: a.cleaned,
                app: a.app.as_ref().map(|app| app.name.as_str()),
                bundle_id: a.app.as_ref().map(|app| app.bundle_id.as_str()),
            });
        }
    }

    fn styles_command(&mut self, cmd: Command) {
        match cmd {
            // Se schimba des si nu e setare: nu scriem nimic pe disc.
            Command::FrontmostApp { bundle_id, name } => {
                let name = if name.is_empty() { bundle_id.clone() } else { name };
                self.styles
                    .with(|st| st.set_frontmost(vf_domain::styles::AppRef { bundle_id, name }));
                self.emit_styles();
                return;
            }
            Command::ListStyles => {
                self.emit_styles();
                return;
            }
            Command::SetCleanupLevel { level } => self.s.cleanup.level = level,
            Command::SetStylesEnabled { enabled } => self.s.styles.enabled = enabled,
            Command::SaveStyle { id, instruction } => {
                self.s.styles.set_instruction(id, instruction.as_deref())
            }
            Command::AssignApp { bundle_id, name, style } => {
                self.s.styles.assign(&bundle_id, &name, style)
            }
            Command::UnassignApp { bundle_id } => self.s.styles.unassign(&bundle_id),
            _ => return,
        }
        // Urmatoarea dictare foloseste deja noile setari: motorul nu trebuie repornit.
        self.styles.with(|st| {
            st.level = self.s.cleanup.level;
            st.styles = self.s.styles.clone();
        });
        self.save();
        self.emit_styles();
    }
}

/// Mesajele de stare aratate o clipa in interfata dupa o inregistrare fara rezultat.
const CANCELLED_MESSAGE: &str = "Cancelled";
const NO_SPEECH_MESSAGE: &str = "Didn't hear anything";

/// Variabila de mediu care marcheaza un daemon deja repornit pentru permisiune (o singura data).
const PERMISSION_RESTART_ENV: &str = "VF_PERMISSION_RESTART";

/// macOS tine raspunsul la "am Accessibility?" in memoria procesului: un daemon pornit inainte
/// ca permisiunea sa fie acordata vede "nu" pana la repornire. Un proces proaspat vede adevarul.
fn granted_in_fresh_process() -> bool {
    if !cfg!(target_os = "macos") || std::env::var_os(PERMISSION_RESTART_ENV).is_some() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(exe)
        .arg("check-permission")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Inlocuieste procesul cu unul nou (aceleasi argumente, aceleasi stdin / stdout catre aplicatie).
fn restart_for_permission() {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        eprintln!("Accessibility permission granted: restarting the engine.");
        let err = std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .env(PERMISSION_RESTART_ENV, "1")
            .exec();
        eprintln!("Could not restart the engine: {err}");
    }
}

pub fn run(s: Settings, dict: Dictionary, config_path: PathBuf) -> anyhow::Result<()> {
    let terms = dict.terms().len();
    let styles = crate::styles::Shared::new(&s);
    let (mut dictation, cleanup) = crate::build_dictation_styled(&s, dict, styles.clone())?;
    let model = vf_application::SpeechToText::name(dictation.asr.as_ref()).to_string();
    let (tx, rx) = mpsc::channel::<Msg>();

    // Command Mode: firul lui foloseste acelasi motor de transcriere ca dictarea.
    let command_done_tx = tx.clone();
    let command_tx = crate::command::spawn(&mut dictation, &s.cleanup, move |o| {
        let _ = command_done_tx.send(Msg::CommandDone(o));
    });

    let done_tx = tx.clone();
    let snippets = crate::snippets::Store::new(&config_path);
    let audio_tx = crate::worker::spawn_with_snippets(dictation, s.paste, snippets.shared(), move |o| {
        let _ = done_tx.send(Msg::Done(o));
    });

    // Microfonul se deschide pe firul lui: daca macOS il tine pe loc (ecran blocat, dialog de
    // permisiune), restul aplicatiei porneste oricum. VF_NO_MIC il sare cu totul, pentru teste.
    if std::env::var_os("VF_NO_MIC").is_none() {
        let mic_tx = tx.clone();
        vf_audio_cpal::spawn(None, move |r| {
            let _ = mic_tx.send(Msg::Mic(r.map_err(|e| format!("{e:#}"))));
        });
    }

    // Comenzi de la aplicatie.
    let stdin_tx = tx.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Command>(&line) {
                Ok(c) => {
                    let _ = stdin_tx.send(Msg::Cmd(c));
                }
                Err(e) => emit(&Event::Error {
                    message: &format!("Invalid command: {e}"),
                }),
            }
        }
        let _ = stdin_tx.send(Msg::StdinClosed);
    });

    // Nivelul audio (~30/s) si reincercarea tastaturii cand lipseste permisiunea.
    let tick_tx = tx.clone();
    std::thread::spawn(move || {
        let mut n = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(33));
            n += 1;
            if tick_tx.send(Msg::Tick).is_err() {
                break;
            }
            if n.is_multiple_of(45) && tick_tx.send(Msg::RetryKeyboard).is_err() {
                break;
            }
        }
    });

    crate::history::prune_at_start(&config_path, &s.history);

    let mut d = Daemon {
        s,
        config_path,
        tx,
        recorder: None,
        audio_tx,
        cleanup,
        snippets,
        listener: None,
        recording: false,
        processing: false,
        capturing_hotkey: false,
        keyboard_error: None,
        downloads: HashMap::new(),
        llm_downloads: HashMap::new(),
        llm_server: None,
        llm_generation: 0,
        mic_error: None,
        mic_retry_at: None,
        styles,
        command_tx,
    };

    let config_str = d.config_path.to_string_lossy().into_owned();
    emit(&Event::Ready {
        settings: &d.s,
        config_path: &config_str,
        model: &model,
        microphone: d.recorder.as_ref().map(|r| r.device_name()),
        dictionary_terms: terms,
    });
    d.emit_models();
    d.emit_dictionary(false);
    d.emit_snippets(d.snippets.list());
    d.emit_llm_models();
    d.emit_styles();
    d.sync_llm();
    d.start_keyboard();
    if d.listener.is_none() && !vf_hotkey::has_permission() {
        vf_hotkey::request_permission();
    }
    d.state("idle", None);

    for msg in rx {
        if !d.handle(msg) {
            break;
        }
    }
    d.stop_llm();
    // Iesim direct: firul audio poate fi inca blocat in sistem si nu trebuie asteptat.
    std::process::exit(0)
}

// Command Mode: selectezi text, tii apasata a doua scurtatura, spui instructiunea.

impl From<vf_hotkey::command::KeyEvent> for Msg {
    fn from(ev: vf_hotkey::command::KeyEvent) -> Self {
        match ev {
            vf_hotkey::command::KeyEvent::Dictation(ev) => Msg::Key(ev),
            vf_hotkey::command::KeyEvent::Command(key) => Msg::CommandKey(key),
        }
    }
}

impl Daemon {
    fn emit_command_mode(&self) {
        let c = &self.s.command;
        let problem = crate::command::hotkey(&self.s).err();
        emit(&Event::CommandMode {
            enabled: c.enabled,
            hotkey: &c.hotkey,
            prompt: c.effective_prompt(),
            prompt_is_default: c.prompt_is_default(),
            problem: problem.as_deref(),
        });
    }

    fn command_key(&mut self, key: vf_hotkey::command::CommandKey) {
        use vf_hotkey::command::CommandKey;
        if self.capturing_hotkey {
            return;
        }
        match key {
            // Daca prefixul scurtaturii (ex. Fn) a pornit deja o dictare, o preluam ca instructiune.
            CommandKey::Start => {
                if !self.recording {
                    self.begin();
                }
            }
            CommandKey::Stop => self.command_finish(),
            CommandKey::Cancel => self.cancel(),
        }
    }

    /// Ca `finish`, dar inregistrarea merge la firul de comenzi, nu la dictare.
    fn command_finish(&mut self) {
        if !self.recording {
            return;
        }
        let samples = self.recorder.as_ref().map(|r| r.stop()).unwrap_or_default();
        crate::play("Pop", self.s.sounds);
        self.recording = false;
        self.processing = true;
        self.state("processing", None);
        let _ = self.command_tx.send(crate::command::Job {
            samples,
            prompt: self.s.command.effective_prompt().to_string(),
        });
    }

    fn command_done(&mut self, outcome: vf_application::command::CommandOutcome) {
        use vf_application::command::CommandOutcome;
        self.processing = false;
        match outcome {
            CommandOutcome::Replaced { instruction, before, after } => {
                emit(&Event::CommandResult {
                    instruction: &instruction,
                    before: &before,
                    after: &after,
                });
                if !self.recording {
                    self.state("idle", None);
                }
            }
            CommandOutcome::NoSpeech => {
                if !self.recording {
                    self.state("idle", Some(NO_SPEECH_MESSAGE));
                }
            }
            CommandOutcome::NoSelection { instruction } => {
                crate::play("Basso", self.s.sounds);
                emit(&Event::CommandFailed {
                    message: crate::command::NO_SELECTION,
                    instruction: Some(&instruction),
                });
                if !self.recording {
                    self.state("idle", Some(crate::command::NO_SELECTION));
                }
            }
            CommandOutcome::Failed { instruction, message } => {
                crate::play("Basso", self.s.sounds);
                emit(&Event::CommandFailed {
                    message: &message,
                    instruction: instruction.as_deref(),
                });
                self.state("error", Some(&message));
            }
        }
    }

    fn set_command_enabled(&mut self, enabled: bool) {
        self.s.command.enabled = enabled;
        self.save();
        // Comenzile au nevoie de modelul local chiar daca corecturile sunt oprite.
        self.sync_llm();
        self.start_keyboard();
    }

    fn set_command_hotkey(&mut self, hotkey: String) {
        match vf_hotkey::command::parse_command_hotkey(&hotkey, &self.s.hotkey) {
            Ok(_) => {
                self.s.command.hotkey = hotkey.trim().to_string();
                self.save();
                self.start_keyboard();
            }
            Err(e) => {
                emit(&Event::Error { message: &format!("{e:#}") });
                self.emit_command_mode();
            }
        }
    }

    fn set_command_prompt(&mut self, prompt: String) {
        self.s.command.prompt = if prompt.trim().is_empty() {
            vf_domain::command::DEFAULT_COMMAND_PROMPT.to_string()
        } else {
            prompt
        };
        self.save();
        self.emit_command_mode();
    }

    fn record_command_hotkey(&mut self) {
        if self.capturing_hotkey {
            return;
        }
        self.capturing_hotkey = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let r = vf_hotkey::record_once(Duration::from_secs(10)).unwrap_or_else(|e| {
                eprintln!("{e:#}");
                None
            });
            let _ = tx.send(Msg::CommandRecorded(r));
        });
    }
}
