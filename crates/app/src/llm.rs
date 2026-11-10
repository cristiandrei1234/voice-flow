//! Modelul de corecturi (LLM local): lista de modele, descarcare si serverul `mlx_lm.server`,
//! pe care Voice Flow il porneste si il opreste singur.

use anyhow::{bail, Context};
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::Remote;

/// Modele recomandate pentru corectarea dictarii (toate in format MLX, 4 bit).
struct Recommended {
    repo: &'static str,
    title: &'static str,
    note: &'static str,
    bytes: u64,
}

const RECOMMENDED: &[Recommended] = &[
    Recommended {
        repo: "mlx-community/Qwen3-4B-4bit",
        title: "Qwen3 4B",
        note: "Small and fast: adds about half a second. Good for punctuation and hesitations.",
        bytes: 2_280_000_000,
    },
    Recommended {
        repo: "mlx-community/gemma-3-4b-it-qat-4bit",
        title: "Gemma 3 4B",
        note: "Small, with good Romanian. An alternative to Qwen3 4B.",
        bytes: 3_030_000_000,
    },
    Recommended {
        repo: "mlx-community/Qwen2.5-7B-Instruct-4bit",
        title: "Qwen2.5 7B",
        note: "Balanced: more careful with technical terms, about one extra second.",
        bytes: 4_300_000_000,
    },
    Recommended {
        repo: "divinetribe/Hermes-4-14B-abliterated-4bit-mlx",
        title: "Hermes 4 14B",
        note: "Best at corrections, but uses ~9 GB of memory and adds 1–2 seconds.",
        bytes: 8_320_000_000,
    },
];

/// Un model de corecturi, asa cum il vede interfata.
#[derive(Debug, Serialize, Clone)]
pub struct LlmInfo {
    /// Depozitul Hugging Face ("org/nume"): cheia modelului in interfata.
    pub repo: String,
    /// Ce se pune in `cleanup.model`: depozitul (pentru cache-ul HF) sau calea folderului nostru.
    pub model: String,
    pub title: String,
    pub note: String,
    pub size_bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub downloading: bool,
    /// Doar modelele descarcate de Voice Flow se pot sterge de aici (cache-ul HF nu il atingem).
    pub deletable: bool,
}

fn hf_cache() -> PathBuf {
    std::env::var_os("HF_HOME")
        .map(|h| PathBuf::from(h).join("hub"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".cache/huggingface/hub")
        })
}

/// Folderul in care Voice Flow descarca modelele de corecturi.
pub fn llm_dir(models_dir: &Path) -> PathBuf {
    models_dir.join("llm")
}

fn own_path(models_dir: &Path, repo: &str) -> PathBuf {
    llm_dir(models_dir).join(repo.replace('/', "--"))
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match std::fs::metadata(e.path()) {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Un folder arata ca un model MLX de text complet?
fn looks_like_text_model(dir: &Path) -> bool {
    let has = |n: &str| dir.join(n).exists();
    let weights = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".safetensors"))
        })
        .unwrap_or(false);
    has("config.json") && weights && (has("tokenizer.json") || has("tokenizer_config.json"))
}

/// Modelele MLX de text din cache-ul Hugging Face: (depozit, folderul snapshot-ului).
fn cached() -> Vec<(String, PathBuf)> {
    const NOT_TEXT: &[&str] = &[
        "tts", "audio", "whisper", "parakeet", "voice", "speech", "clip", "flux", "diffusion", "embed",
    ];
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(hf_cache()) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix("models--") else { continue };
        let Some((org, model)) = rest.split_once("--") else { continue };
        let lower = rest.to_lowercase();
        if !(lower.contains("mlx")) || NOT_TEXT.iter().any(|w| lower.contains(w)) {
            continue;
        }
        let Ok(snaps) = std::fs::read_dir(e.path().join("snapshots")) else { continue };
        if let Some(snap) = snaps.flatten().map(|s| s.path()).find(|p| looks_like_text_model(p)) {
            out.push((format!("{org}/{model}"), snap));
        }
    }
    out.sort();
    out
}

pub fn list(models_dir: &Path, active_model: &str, downloading: &[&str]) -> Vec<LlmInfo> {
    let cache = cached();
    let mut out: Vec<LlmInfo> = Vec::new();
    let mut push = |repo: &str, title: String, note: String, bytes: u64| {
        let own = own_path(models_dir, repo);
        let own_ok = own.join(".complete").exists();
        let in_cache = cache.iter().find(|(r, _)| r == repo);
        let (model, size, installed) = if own_ok {
            (own.to_string_lossy().into_owned(), dir_size(&own), true)
        } else if let Some((_, snap)) = in_cache {
            (repo.to_string(), dir_size(snap), true)
        } else {
            (own.to_string_lossy().into_owned(), bytes, false)
        };
        out.push(LlmInfo {
            repo: repo.to_string(),
            active: installed && (active_model == model || active_model == repo),
            model,
            title,
            note,
            size_bytes: size,
            installed,
            downloading: downloading.contains(&repo),
            deletable: own_ok,
        });
    };
    for r in RECOMMENDED {
        push(r.repo, r.title.to_string(), r.note.to_string(), r.bytes);
    }
    // Restul modelelor de text gasite in cache: le poti folosi, fara sa le descarci.
    for (repo, _) in &cache {
        if RECOMMENDED.iter().any(|r| r.repo == repo) {
            continue;
        }
        let title = repo.split('/').next_back().unwrap_or(repo).to_string();
        push(repo, title, "Found in the Hugging Face cache on this Mac.".to_string(), 0);
    }
    out
}

/// Descarca un model in folderul Voice Flow (lista de fisiere vine de la Hugging Face).
pub fn download(
    repo: &str,
    models_dir: &Path,
    cancel: &Arc<AtomicBool>,
    progress: impl FnMut(u64, u64),
) -> anyhow::Result<()> {
    if !RECOMMENDED.iter().any(|r| r.repo == repo) {
        bail!("{repo} is not in the list of recommended models");
    }
    let api = format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=1");
    let tree: serde_json::Value = ureq::get(&api)
        .timeout(Duration::from_secs(30))
        .call()
        .with_context(|| format!("Could not read the file list for {repo}"))?
        .into_json()?;
    let files: Vec<Remote> = tree
        .as_array()
        .context("Unexpected response from Hugging Face")?
        .iter()
        .filter(|f| f["type"] == "file")
        .filter_map(|f| {
            let name = f["path"].as_str()?;
            let skip = name.starts_with('.') || name.eq_ignore_ascii_case("README.md");
            (!skip).then(|| Remote {
                name: name.to_string(),
                bytes: f["size"].as_u64().unwrap_or(0),
                url: format!("https://huggingface.co/{repo}/resolve/main/{name}"),
            })
        })
        .collect();
    if !files.iter().any(|f| f.name.ends_with(".safetensors")) {
        bail!("{repo} does not look like an MLX model (it has no .safetensors files)");
    }
    let dir = own_path(models_dir, repo);
    crate::models::download_files(&dir, &files, cancel, progress)?;
    std::fs::write(dir.join(".complete"), repo)?;
    Ok(())
}

/// Sterge un model descarcat de Voice Flow. Cache-ul Hugging Face nu e atins niciodata.
pub fn delete(repo: &str, models_dir: &Path, active_model: &str) -> anyhow::Result<()> {
    let dir = own_path(models_dir, repo);
    if !dir.starts_with(llm_dir(models_dir)) || !dir.join(".complete").exists() {
        bail!("{repo} was not downloaded by Voice Flow, so it was not deleted");
    }
    if dir.to_string_lossy() == active_model {
        bail!("This is the model in use; choose another model before deleting it");
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("Could not delete {}", dir.display()))
}

// ---------------------------------------------------------------- serverul

/// "http://127.0.0.1:8080/v1/chat/completions" -> ("127.0.0.1", 8080)
pub fn host_port(url: &str) -> Option<(String, u16)> {
    let rest = url.split("://").nth(1)?;
    let authority = rest.split('/').next()?;
    let (host, port) = authority.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

fn responds(host: &str, port: u16) -> bool {
    ureq::get(&format!("http://{host}:{port}/v1/models"))
        .timeout(Duration::from_secs(2))
        .call()
        .is_ok()
}

fn find_server(configured: &str) -> Option<PathBuf> {
    if !configured.trim().is_empty() {
        return Some(PathBuf::from(configured.trim()));
    }
    let home = dirs::home_dir().unwrap_or_default();
    let mut dirs_: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    // O aplicatie pornita din Finder nu are PATH-ul din terminal.
    dirs_.extend([
        home.join(".local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    dirs_.into_iter().map(|d| d.join("mlx_lm.server")).find(|p| p.is_file())
}

/// Serverul pornit de noi. Se opreste cand acest obiect dispare SAU cand procesul nostru moare
/// din orice motiv: un mic script sta cu `cat` pe un pipe tinut de noi si opreste serverul
/// cand pipe-ul se inchide. Asa nu ramane niciodata un model de cativa GB uitat in memorie.
pub struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    pub model: String,
}

impl Server {
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn stop(mut self) {
        drop(self.stdin.take());
        let until = Instant::now() + Duration::from_secs(4);
        while Instant::now() < until {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub enum Started {
    /// Am pornit noi serverul; mai trebuie asteptat pana incarca modelul.
    Spawned(Server),
    /// Pe acel port raspunde deja un server (pornit de tine): il folosim asa cum e.
    External,
}

pub fn start(url: &str, model: &str, server_command: &str) -> anyhow::Result<Started> {
    let (host, port) = host_port(url).with_context(|| format!("The server address is not valid: {url}"))?;
    if responds(&host, port) {
        return Ok(Started::External);
    }
    let exe = find_server(server_command).context(
        "mlx_lm.server was not found. Install it with: uv tool install mlx-lm",
    )?;
    let log = dirs::home_dir()
        .unwrap_or_default()
        .join("Library/Logs/VoiceFlowLLM.log");
    let mut logf = std::fs::OpenOptions::new().create(true).append(true).open(&log)?;
    let _ = writeln!(logf, "--- starting {} with {model}", exe.display());
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(r#""$0" "$@" & c=$!; cat >/dev/null; kill $c 2>/dev/null; wait $c"#)
        .arg(&exe)
        .args(["--model", model, "--host", &host, "--port", &port.to_string()])
        // Modelele care "gandesc" (Qwen3) ar intarzia fiecare dictare; pentru corecturi nu ajuta.
        .args(["--chat-template-args", r#"{"enable_thinking":false}"#])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(logf.try_clone()?))
        .stderr(Stdio::from(logf))
        .spawn()
        .with_context(|| format!("Could not start {}", exe.display()))?;
    let stdin = child.stdin.take();
    Ok(Started::Spawned(Server {
        child,
        stdin,
        model: model.to_string(),
    }))
}

/// Asteapta pana raspunde serverul (incarcarea modelului poate dura zeci de secunde).
pub fn wait_ready(url: &str, timeout: Duration) -> bool {
    let Some((host, port)) = host_port(url) else { return false };
    let until = Instant::now() + timeout;
    while Instant::now() < until {
        if responds(&host, port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(700));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_and_port() {
        assert_eq!(
            host_port("http://127.0.0.1:8080/v1/chat/completions"),
            Some(("127.0.0.1".into(), 8080))
        );
        assert_eq!(host_port("http://localhost/v1"), None);
    }

    #[test]
    fn delete_refuses_what_voice_flow_did_not_download() {
        let tmp = std::env::temp_dir().join(format!("vf-llm-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        // Nu exista deloc.
        assert!(delete("mlx-community/Qwen3-4B-4bit", &tmp, "").is_err());
        // Exista, dar fara marcajul de descarcare completa (nu e al nostru / e pe jumatate).
        let dir = own_path(&tmp, "mlx-community/Qwen3-4B-4bit");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(delete("mlx-community/Qwen3-4B-4bit", &tmp, "").is_err());
        // Complet, dar activ.
        std::fs::write(dir.join(".complete"), "x").unwrap();
        assert!(delete("mlx-community/Qwen3-4B-4bit", &tmp, &dir.to_string_lossy()).is_err());
        // Complet si nefolosit: se sterge.
        delete("mlx-community/Qwen3-4B-4bit", &tmp, "altceva").unwrap();
        assert!(!dir.exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn server_dies_when_our_pipe_closes() {
        // Acelasi script ca in `start`, cu `sleep` in loc de server.
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(r#""$0" "$@" & c=$!; cat >/dev/null; kill $c 2>/dev/null; wait $c"#)
            .args(["/bin/sleep", "60"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let s = Server { child, stdin, model: String::new() };
        let t = Instant::now();
        s.stop();
        assert!(t.elapsed() < Duration::from_secs(3), "serverul nu s-a oprit la inchiderea pipe-ului");
    }
}
