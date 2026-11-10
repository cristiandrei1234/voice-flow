//! Managerul de modele: lista pentru interfata, descarcare (cu reluare), stergere.

use anyhow::{bail, Context};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use vf_domain::{EngineKind, ModelSpec};

/// Un model, asa cum il vede interfata.
#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub languages: &'static str,
    pub note: &'static str,
    pub size_bytes: u64,
    pub installed: bool,
    pub active: bool,
    pub downloadable: bool,
    pub downloading: bool,
}

/// Textul erorii cu care se opreste o descarcare anulata de utilizator (daemonul o recunoaste dupa el).
pub const CANCELLED: &str = "cancelled";

pub fn list(models_dir: &Path, active: EngineKind, downloading: &[&str]) -> Vec<ModelInfo> {
    vf_domain::models::catalog()
        .iter()
        .map(|m| ModelInfo {
            id: m.id,
            title: m.title,
            languages: m.languages,
            note: m.note,
            size_bytes: m.size_bytes(models_dir),
            installed: m.installed(models_dir),
            active: m.kind == active,
            downloadable: m.repo.is_some(),
            downloading: downloading.contains(&m.id),
        })
        .collect()
}

/// Sterge fisierele modelului. Doar ce se poate descarca din nou; niciodata modelul activ.
pub fn delete(spec: &ModelSpec, models_dir: &Path, active: EngineKind) -> anyhow::Result<()> {
    if spec.kind == active {
        bail!("{} is the model in use; choose another model before deleting it", spec.title);
    }
    if spec.repo.is_none() {
        bail!("{} cannot be downloaded again, so it was not deleted", spec.title);
    }
    let dir = spec.path(models_dir);
    // Stergem doar fisierele din catalog (si resturile de descarcare), nu tot folderul.
    for f in spec.files {
        for p in [dir.join(f.name), part_path(&dir, f.name)] {
            if p.exists() {
                std::fs::remove_file(&p).with_context(|| format!("Could not delete {}", p.display()))?;
            }
        }
    }
    let _ = std::fs::remove_dir(&dir); // doar daca a ramas gol
    Ok(())
}

fn part_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.part"))
}

/// Un fisier de descarcat: nume (poate avea subfoldere), marime, adresa.
pub struct Remote {
    pub name: String,
    pub bytes: u64,
    pub url: String,
}

/// Descarca fisierele care lipsesc. `progress(primiti, total)` e apelat de cateva ori pe secunda.
/// O descarcare intrerupta se reia de unde a ramas (fisierele `.part`).
pub fn download(
    spec: &ModelSpec,
    models_dir: &Path,
    cancel: &Arc<AtomicBool>,
    progress: impl FnMut(u64, u64),
) -> anyhow::Result<()> {
    if spec.repo.is_none() {
        bail!("{} cannot be downloaded", spec.title);
    }
    let files: Vec<Remote> = spec
        .files
        .iter()
        .map(|f| Remote {
            name: f.name.to_string(),
            bytes: f.bytes,
            url: spec.url(f.name).expect("has a repo"),
        })
        .collect();
    download_files(&spec.path(models_dir), &files, cancel, progress)
}

pub fn download_files(
    dir: &Path,
    files: &[Remote],
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(u64, u64),
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("Could not create {}", dir.display()))?;
    let total: u64 = files.iter().map(|f| f.bytes).sum();
    let mut done: u64 = 0;
    let mut last = Instant::now() - Duration::from_secs(1);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .build();

    for f in files {
        let target = dir.join(&f.name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::metadata(&target).map(|m| m.len() == f.bytes).unwrap_or(false) {
            done += f.bytes;
            continue;
        }
        let part = part_path(dir, &f.name);
        let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if have > f.bytes {
            have = 0;
        }
        if have < f.bytes || f.bytes == 0 {
            let mut req = agent.get(&f.url);
            if have > 0 {
                req = req.set("Range", &format!("bytes={have}-"));
            }
            let resp = req.call().with_context(|| format!("Could not download {}", f.name))?;
            // Serverul poate ignora Range si trimite tot fisierul: atunci o luam de la zero.
            let resumed = resp.status() == 206;
            if !resumed {
                have = 0;
            }
            let mut out = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(resumed)
                .truncate(!resumed)
                .open(&part)
                .with_context(|| format!("Could not write {}", part.display()))?;
            let mut reader = resp.into_reader();
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                if cancel.load(Ordering::Relaxed) {
                    bail!(CANCELLED);
                }
                let n = reader
                    .read(&mut buf)
                    .with_context(|| format!("Connection interrupted while downloading {}", f.name))?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                have += n as u64;
                if last.elapsed() >= Duration::from_millis(200) {
                    last = Instant::now();
                    progress(done + have, total);
                }
            }
            out.flush()?;
        }
        if have != f.bytes {
            bail!("{} is incomplete ({have} of {} bytes); try again", f.name, f.bytes);
        }
        std::fs::rename(&part, &target)?;
        done += f.bytes;
        progress(done, total);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_to_delete_the_active_model() {
        let tmp = std::env::temp_dir().join(format!("vf-del-{}", std::process::id()));
        let v3 = ModelSpec::by_id("parakeet-v3").unwrap();
        assert!(delete(v3, &tmp, EngineKind::ParakeetV3).is_err());
    }

    #[test]
    fn delete_removes_only_catalog_files() {
        let tmp = std::env::temp_dir().join(format!("vf-del2-{}", std::process::id()));
        let v3 = ModelSpec::by_id("parakeet-v3").unwrap();
        let dir = v3.path(&tmp);
        std::fs::create_dir_all(&dir).unwrap();
        for f in v3.files {
            std::fs::write(dir.join(f.name), b"x").unwrap();
        }
        std::fs::write(dir.join("notite.txt"), b"ale mele").unwrap();
        delete(v3, &tmp, EngineKind::ParakeetRo).unwrap();
        assert!(!dir.join("encoder.int8.onnx").exists());
        assert!(dir.join("notite.txt").exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
