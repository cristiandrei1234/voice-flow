//! Snippets: fisierul `snippets.json` (langa config.toml) si lista folosita la dictare.
//!
//! Logica (potrivire, validare) e in `vf_domain::snippets`. Aici e doar citirea / scrierea
//! si lista comuna cu firul de transcriere, care se schimba fara repornirea motorului.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use vf_domain::{Snippet, SnippetDraft, SnippetMatcher, SnippetSet};

/// Lista pregatita pentru cautare, citita de firul de transcriere la fiecare dictare.
pub type Shared = Arc<RwLock<SnippetMatcher>>;

/// Un snippet salvat fara `enabled` e activ.
pub fn enabled_by_default() -> bool {
    true
}

/// Aplica snippet-urile pe textul final al unei dictari.
pub fn expand(shared: &Shared, text: &str) -> Option<String> {
    shared.read().ok()?.expand(text)
}

/// Ce afla interfata dupa o comanda: lista curenta si, daca a fost cazul, eroarea.
pub struct Reply {
    pub snippets: Vec<Snippet>,
    pub error: Option<String>,
}

pub struct Store {
    path: PathBuf,
    shared: Shared,
}

impl Store {
    /// `snippets.json` sta in acelasi dosar cu fisierul de setari.
    pub fn new(config_path: &Path) -> Self {
        let dir = config_path.parent().filter(|d| !d.as_os_str().is_empty());
        Self {
            path: dir.unwrap_or(Path::new(".")).join("snippets.json"),
            shared: Shared::default(),
        }
    }

    pub fn shared(&self) -> Shared {
        self.shared.clone()
    }

    /// Fisier lipsa = niciun snippet. Fisier stricat = eroare, iar fisierul ramane neatins.
    fn read(&self) -> Result<SnippetSet, String> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SnippetSet::default()),
            Err(e) => return Err(format!("Could not read {}: {e}", self.path.display())),
        };
        if text.trim().is_empty() {
            return Ok(SnippetSet::default());
        }
        serde_json::from_str(&text).map_err(|e| {
            format!(
                "The snippets file is corrupted and was left untouched ({}): {e}. Fix it or move it out of the folder to be able to save new snippets.",
                self.path.display()
            )
        })
    }

    fn write(&self, set: &SnippetSet) -> Result<(), String> {
        let fail = |e: std::io::Error| format!("Could not save the snippets: {e}");
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(fail)?;
        }
        let mut json = serde_json::to_string_pretty(set).map_err(|e| format!("Could not save the snippets: {e}"))?;
        json.push('\n');
        // Fisier temporar + redenumire, ca sa nu ramana scris pe jumatate.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)
            .and_then(|()| std::fs::rename(&tmp, &self.path))
            .map_err(fail)
    }

    fn publish(&self, set: &SnippetSet) {
        if let Ok(mut m) = self.shared.write() {
            *m = set.matcher();
        }
    }

    /// Reciteste fisierul (prinde si editarile facute de mana) si improspateaza lista de la dictare.
    pub fn list(&self) -> Reply {
        match self.read() {
            Ok(set) => {
                self.publish(&set);
                Reply { snippets: set.snippets().to_vec(), error: None }
            }
            Err(e) => Reply { snippets: Vec::new(), error: Some(e) },
        }
    }

    fn edit(&self, change: impl FnOnce(&mut SnippetSet) -> Result<(), String>) -> Reply {
        let before = match self.read() {
            Ok(set) => set,
            Err(e) => return Reply { snippets: Vec::new(), error: Some(e) },
        };
        let mut set = before.clone();
        match change(&mut set).and_then(|()| self.write(&set)) {
            Ok(()) => {
                self.publish(&set);
                Reply { snippets: set.snippets().to_vec(), error: None }
            }
            Err(e) => Reply { snippets: before.snippets().to_vec(), error: Some(e) },
        }
    }

    /// Adauga (fara `id`) sau modifica (cu `id`) un snippet.
    pub fn save(&self, id: Option<&str>, draft: &SnippetDraft) -> Reply {
        self.edit(|set| match id {
            Some(id) => set.update(id, draft),
            None => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                set.add(draft, now).map(|_| ())
            }
        })
    }

    pub fn delete(&self, id: &str) -> Reply {
        self.edit(|set| set.delete(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dosar temporar propriu fiecarui test, sters la sfarsit.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("vf-snippets-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn store(&self) -> Store {
            Store::new(&self.0.join("config.toml"))
        }

        fn file(&self) -> PathBuf {
            self.0.join("snippets.json")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn draft(trigger: &str, text: &str) -> SnippetDraft {
        SnippetDraft {
            trigger: trigger.into(),
            variants: vec![],
            text: text.into(),
            enabled: true,
        }
    }

    #[test]
    fn missing_file_means_no_snippets_and_is_not_created() {
        let dir = TempDir::new("missing");
        let store = dir.store();
        let r = store.list();
        assert!(r.error.is_none() && r.snippets.is_empty());
        assert!(!dir.file().exists());
        assert_eq!(expand(&store.shared(), "semnătura mea"), None);
    }

    #[test]
    fn saving_writes_the_file_and_changes_dictation_at_once() {
        let dir = TempDir::new("save");
        let store = dir.store();
        let shared = store.shared();

        let r = store.save(None, &draft("semnătura mea", "Cu drag,\nAna"));
        assert!(r.error.is_none(), "{:?}", r.error);
        let id = r.snippets[0].id.clone();
        assert_eq!(expand(&shared, "Semnatura mea.").as_deref(), Some("Cu drag,\nAna"));
        assert!(!dir.0.join("snippets.json.tmp").exists());

        // Alt proces (motorul repornit) vede acelasi lucru.
        let again = dir.store();
        assert_eq!(again.list().snippets, r.snippets);
        assert_eq!(expand(&again.shared(), "semnătura mea").as_deref(), Some("Cu drag,\nAna"));

        let mut off = draft("semnătura mea", "Cu drag,\nAna");
        off.enabled = false;
        let r = store.save(Some(&id), &off);
        assert!(r.error.is_none() && !r.snippets[0].enabled && r.snippets[0].id == id);
        assert_eq!(expand(&shared, "semnătura mea"), None);

        let r = store.delete(&id);
        assert!(r.error.is_none() && r.snippets.is_empty());
        assert!(dir.store().list().snippets.is_empty());
    }

    #[test]
    fn a_rejected_change_leaves_the_file_and_the_list_alone() {
        let dir = TempDir::new("reject");
        let store = dir.store();
        store.save(None, &draft("adresa firmei", "Str. Lungă 5"));
        let on_disk = std::fs::read_to_string(dir.file()).unwrap();

        let r = store.save(None, &draft("link", "x"));
        assert!(r.error.unwrap().contains("too short"));
        assert_eq!(r.snippets.len(), 1);
        let r = store.save(None, &draft("Adresa firmei", "altceva"));
        assert!(r.error.is_some());
        let r = store.delete("nu-exista");
        assert!(r.error.is_some() && r.snippets.len() == 1);

        assert_eq!(std::fs::read_to_string(dir.file()).unwrap(), on_disk);
        assert_eq!(expand(&store.shared(), "adresa firmei").as_deref(), Some("Str. Lungă 5"));
    }

    #[test]
    fn a_corrupt_file_is_reported_and_never_overwritten() {
        let dir = TempDir::new("corrupt");
        let broken = "{ \"snippets\": [ { \"id\": \"a\", \"trigger\": \"semnătura mea\", ";
        std::fs::write(dir.file(), broken).unwrap();
        let store = dir.store();

        let r = store.list();
        assert!(r.snippets.is_empty());
        assert!(r.error.unwrap().contains("corrupted"));
        assert!(store.save(None, &draft("adresa firmei", "x")).error.is_some());
        assert!(store.delete("a").error.is_some());
        assert_eq!(std::fs::read_to_string(dir.file()).unwrap(), broken);
    }

    #[test]
    fn a_file_written_by_hand_loads_with_defaults() {
        let dir = TempDir::new("hand");
        std::fs::write(
            dir.file(),
            r#"{"snippets":[{"id":"a","trigger":"linkul de programări","text":"https://cal.com/x"}]}"#,
        )
        .unwrap();
        let store = dir.store();
        let r = store.list();
        assert!(r.error.is_none());
        assert!(r.snippets[0].enabled && r.snippets[0].variants.is_empty());
        assert_eq!(
            expand(&store.shared(), "uite linkul de programari").as_deref(),
            Some("uite https://cal.com/x")
        );
    }
}
