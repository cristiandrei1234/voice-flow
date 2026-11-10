//! Fisierul cu istoricul dictarilor: `history.jsonl`, langa config.toml, o inregistrare pe linie.
//! Adaugarea scrie doar la sfarsit; stergerea rescrie fisierul prin fisier temporar + redenumire.
//! O linie stricata e sarita la citire si pastrata neatinsa la rescriere.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use vf_domain::history::{self, HistoryRecord, HistorySettings, HistoryStats};

pub const FILE_NAME: &str = "history.jsonl";

/// Cate inregistrari intoarce cel mult o pagina.
const MAX_PAGE: usize = 500;

pub struct Store {
    path: PathBuf,
}

pub struct Page {
    pub items: Vec<HistoryRecord>,
    pub has_more: bool,
}

impl Store {
    /// Istoricul sta in acelasi folder cu fisierul de setari.
    pub fn for_config(config_path: &Path) -> Self {
        Self {
            path: config_path.with_file_name(FILE_NAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Toate inregistrarile, de la cea mai veche la cea mai noua. Fisier lipsa = istoric gol.
    pub fn load(&self) -> Vec<HistoryRecord> {
        let bytes = std::fs::read(&self.path).unwrap_or_default();
        bytes.split(|b| *b == b'\n').filter_map(parse).collect()
    }

    pub fn append(&self, record: &HistoryRecord) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Daca ultima scriere a ramas pe jumatate (fara sfarsit de linie), incepem pe o linie noua,
        // ca inregistrarea asta sa nu se lipeasca de resturi.
        let needs_newline = std::fs::read(&self.path)
            .map(|b| b.last().is_some_and(|c| *c != b'\n'))
            .unwrap_or(false);
        let mut line = String::new();
        if needs_newline {
            line.push('\n');
        }
        line.push_str(&record.to_line());
        line.push('\n');
        private_file(std::fs::OpenOptions::new().create(true).append(true), &self.path)?.write_all(line.as_bytes())
    }

    /// O pagina, de la cea mai noua la cea mai veche. `before` = id-ul ultimei inregistrari
    /// primite deja; pagina continua cu cele mai vechi decat ea.
    pub fn list(&self, query: &str, limit: usize, before: Option<&str>) -> Page {
        let all = self.load();
        let older: Vec<&HistoryRecord> = match before {
            None => all.iter().collect(),
            Some(id) => match all.iter().position(|r| r.id == id) {
                Some(i) => all[..i].iter().collect(),
                // Inregistrarea a fost stearsa intre timp: id-urile incep cu momentul scrierii,
                // deci ordinea lor e ordinea in timp.
                None => all.iter().filter(|r| r.id.as_str() < id).collect(),
            },
        };
        let limit = limit.clamp(1, MAX_PAGE);
        let query = history::fold(query);
        let mut items: Vec<HistoryRecord> = older
            .into_iter()
            .rev()
            .filter(|r| r.matches(&query))
            .take(limit + 1)
            .cloned()
            .collect();
        let has_more = items.len() > limit;
        items.truncate(limit);
        Page { items, has_more }
    }

    /// `Ok(false)` daca nu exista o inregistrare cu id-ul dat.
    pub fn delete(&self, id: &str) -> std::io::Result<bool> {
        Ok(self.rewrite(|r| r.id != id)? > 0)
    }

    /// Sterge tot istoricul (fisierul cu totul, cu tot cu eventuale linii stricate).
    pub fn clear(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Scoate inregistrarile mai vechi decat perioada de pastrare; intoarce cate a scos.
    pub fn prune(&self, retention_days: u32, now: u64) -> std::io::Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        self.rewrite(|r| history::within_retention(r.ts, retention_days, now))
    }

    /// Pastreaza inregistrarile pentru care `keep` e adevarat si liniile pe care nu le intelegem.
    /// Nu scrie nimic daca nu e nimic de scos sau daca fisierul nu a putut fi citit.
    fn rewrite(&self, keep: impl Fn(&HistoryRecord) -> bool) -> std::io::Result<usize> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };
        let mut out = Vec::with_capacity(bytes.len());
        let mut removed = 0;
        for line in bytes.split(|b| *b == b'\n') {
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            if parse(line).is_some_and(|r| !keep(&r)) {
                removed += 1;
                continue;
            }
            out.extend_from_slice(line);
            out.push(b'\n');
        }
        if removed > 0 {
            let tmp = self.path.with_extension("jsonl.tmp");
            private_file(std::fs::OpenOptions::new().create(true).write(true).truncate(true), &tmp)?
                .write_all(&out)?;
            std::fs::rename(&tmp, &self.path)?;
        }
        Ok(removed)
    }
}

fn parse(line: &[u8]) -> Option<HistoryRecord> {
    HistoryRecord::parse_line(std::str::from_utf8(line).ok()?)
}

/// Istoricul contine tot ce a dictat utilizatorul: fisierul il poate citi doar el.
fn private_file(options: &mut std::fs::OpenOptions, path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn now() -> std::time::Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()
}

/// Id unic care incepe cu momentul scrierii (milisecunde), deci se ordoneaza in timp.
fn new_id(ms: u128) -> String {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    format!("{ms:013}-{:03}", SEQ.fetch_add(1, Ordering::Relaxed) % 1000)
}

/// Duratele unei dictari, in milisecunde.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub audio_ms: u64,
    pub asr_ms: u64,
    pub cleanup_ms: Option<u64>,
}

/// Noteaza o dictare reusita. `None` daca istoricul e oprit, textul e gol sau scrierea a esuat.
pub fn record(
    config_path: &Path,
    settings: &HistorySettings,
    text: &str,
    raw: &str,
    timing: Timing,
    app: Option<String>,
) -> Option<HistoryRecord> {
    let Timing { audio_ms, asr_ms, cleanup_ms } = timing;
    if !settings.enabled || text.trim().is_empty() {
        return None;
    }
    let t = now();
    let mut item = HistoryRecord::new(new_id(t.as_millis()), t.as_secs(), text, raw, audio_ms, asr_ms, cleanup_ms);
    item.app = app;
    match Store::for_config(config_path).append(&item) {
        Ok(()) => Some(item),
        Err(e) => {
            eprintln!("History: could not write the dictation: {e}");
            None
        }
    }
}

/// La pornirea motorului: scoate ce a iesit din perioada de pastrare.
pub fn prune_at_start(config_path: &Path, settings: &HistorySettings) {
    match Store::for_config(config_path).prune(settings.retention_days, now().as_secs()) {
        Ok(0) => {}
        Ok(n) => eprintln!("History: deleted {n} dictations older than {} days", settings.retention_days),
        Err(e) => eprintln!("History: could not prune old dictations: {e}"),
    }
}

pub fn stats(config_path: &Path, utc_offset_secs: i64) -> HistoryStats {
    history::stats(&Store::for_config(config_path).load(), now().as_secs(), utc_offset_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Folder temporar sters la sfarsitul testului.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "vf-history-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn config(&self) -> PathBuf {
            self.0.join("config.toml")
        }

        fn store(&self) -> Store {
            Store::for_config(&self.config())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn rec(id: &str, ts: u64, text: &str) -> HistoryRecord {
        HistoryRecord::new(id.into(), ts, text, text, 1000, 50, None)
    }

    fn ids(page: &Page) -> Vec<&str> {
        page.items.iter().map(|r| r.id.as_str()).collect()
    }

    fn fill(store: &Store, n: u64) {
        for i in 1..=n {
            store.append(&rec(&format!("{i:02}"), 1000 + i, &format!("dictarea {i}"))).unwrap();
        }
    }

    #[test]
    fn lives_next_to_the_config_file() {
        let store = Store::for_config(Path::new("/tmp/x/VoiceFlow/config.toml"));
        assert_eq!(store.path(), Path::new("/tmp/x/VoiceFlow/history.jsonl"));
    }

    #[test]
    fn missing_file_is_an_empty_history() {
        let dir = TempDir::new();
        let store = dir.store();
        assert!(store.load().is_empty());
        let page = store.list("", 10, None);
        assert!(page.items.is_empty() && !page.has_more);
        assert!(!store.delete("x").unwrap());
        assert_eq!(store.prune(30, 1_000_000).unwrap(), 0);
        store.clear().unwrap();
        assert!(!store.path().exists());
    }

    #[test]
    fn appends_one_line_per_record_and_creates_the_folder() {
        let dir = TempDir::new();
        let store = Store::for_config(&dir.0.join("sub/dir/config.toml"));
        store.append(&rec("a", 1, "prima\ndictare")).unwrap();
        store.append(&rec("b", 2, "a doua")).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.ends_with('\n'));
        let all = store.load();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].text, "prima\ndictare");
        assert_eq!(all[1].id, "b");
    }

    #[cfg(unix)]
    #[test]
    fn file_is_readable_only_by_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let store = dir.store();
        fill(&store, 2);
        assert_eq!(std::fs::metadata(store.path()).unwrap().permissions().mode() & 0o777, 0o600);
        store.delete("01").unwrap();
        assert_eq!(std::fs::metadata(store.path()).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn lists_newest_first_with_paging() {
        let dir = TempDir::new();
        let store = dir.store();
        fill(&store, 5);
        let first = store.list("", 2, None);
        assert_eq!(ids(&first), ["05", "04"]);
        assert!(first.has_more);
        let second = store.list("", 2, Some("04"));
        assert_eq!(ids(&second), ["03", "02"]);
        assert!(second.has_more);
        let last = store.list("", 2, Some("02"));
        assert_eq!(ids(&last), ["01"]);
        assert!(!last.has_more);
        // Exact cat incape: nu mai e nimic dupa.
        assert!(!store.list("", 5, None).has_more);
        // Limita 0 nu blocheaza paginarea.
        assert_eq!(store.list("", 0, None).items.len(), 1);
    }

    #[test]
    fn paging_survives_a_deleted_cursor() {
        let dir = TempDir::new();
        let store = dir.store();
        fill(&store, 5);
        assert!(store.delete("03").unwrap());
        assert_eq!(ids(&store.list("", 10, Some("03"))), ["02", "01"]);
    }

    #[test]
    fn search_filters_before_paging() {
        let dir = TempDir::new();
        let store = dir.store();
        store.append(&rec("1", 1, "Mergem la Brașov mâine")).unwrap();
        store.append(&rec("2", 2, "Trimite oferta")).unwrap();
        store.append(&rec("3", 3, "brasov e frumos")).unwrap();
        store.append(&rec("4", 4, "Altceva")).unwrap();
        let page = store.list("BRAȘOV", 1, None);
        assert_eq!(ids(&page), ["3"]);
        assert!(page.has_more);
        let next = store.list("brasov", 1, Some("3"));
        assert_eq!(ids(&next), ["1"]);
        assert!(!next.has_more);
        assert!(store.list("cluj", 10, None).items.is_empty());
    }

    #[test]
    fn deletes_one_record() {
        let dir = TempDir::new();
        let store = dir.store();
        fill(&store, 3);
        assert!(store.delete("02").unwrap());
        assert!(!store.delete("02").unwrap());
        assert_eq!(ids(&store.list("", 10, None)), ["03", "01"]);
        assert!(!store.path().with_extension("jsonl.tmp").exists());
    }

    #[test]
    fn clear_removes_everything() {
        let dir = TempDir::new();
        let store = dir.store();
        fill(&store, 3);
        store.clear().unwrap();
        assert!(!store.path().exists());
        assert!(store.load().is_empty());
        // Dupa stergere se poate scrie din nou.
        fill(&store, 1);
        assert_eq!(store.load().len(), 1);
    }

    #[test]
    fn corrupt_lines_are_skipped_and_never_lost() {
        let dir = TempDir::new();
        let store = dir.store();
        let good1 = rec("a", 10, "prima").to_line();
        let good2 = rec("b", 20, "a doua").to_line();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(good1.as_bytes());
        bytes.extend_from_slice(b"\nnu e json\n\n{\"id\":\"rupt\",\"ts\":\n\xff\xfe binar\n");
        bytes.extend_from_slice(good2.as_bytes());
        // Ultima scriere a ramas pe jumatate, fara sfarsit de linie.
        bytes.extend_from_slice(b"\n{\"id\":\"c\",\"ts\":30,\"te");
        std::fs::write(store.path(), &bytes).unwrap();

        assert_eq!(ids(&store.list("", 10, None)), ["b", "a"]);
        // Citirea nu atinge fisierul.
        assert_eq!(std::fs::read(store.path()).unwrap(), bytes);

        // O inregistrare noua incepe pe linia ei, nu lipita de resturi.
        store.append(&rec("d", 40, "a patra")).unwrap();
        assert_eq!(ids(&store.list("", 10, None)), ["d", "b", "a"]);

        // Rescrierea scoate doar ce i s-a cerut; liniile neintelese raman.
        assert!(store.delete("a").unwrap());
        let after = std::fs::read(store.path()).unwrap();
        let text = String::from_utf8_lossy(&after);
        assert!(text.contains("nu e json"));
        assert!(text.contains("{\"id\":\"rupt\",\"ts\":"));
        assert!(text.contains("{\"id\":\"c\",\"ts\":30,\"te"));
        assert!(after.windows(2).any(|w| w == b"\xff\xfe"));
        assert_eq!(ids(&store.list("", 10, None)), ["d", "b"]);
    }

    #[test]
    fn prune_removes_only_old_records() {
        let dir = TempDir::new();
        let store = dir.store();
        let day = 86_400;
        let now = 1_000 * day;
        store.append(&rec("vechi", now - 91 * day, "veche")).unwrap();
        store.append(&rec("limita", now - 90 * day, "la limita")).unwrap();
        store.append(&rec("nou", now - day, "noua")).unwrap();

        // 0 = pentru totdeauna.
        assert_eq!(store.prune(0, now).unwrap(), 0);
        assert_eq!(store.load().len(), 3);

        assert_eq!(store.prune(90, now).unwrap(), 1);
        assert_eq!(ids(&store.list("", 10, None)), ["nou", "limita"]);

        // Nimic de scos: fisierul nu e rescris.
        let before = std::fs::metadata(store.path()).unwrap().modified().unwrap();
        assert_eq!(store.prune(90, now).unwrap(), 0);
        assert_eq!(std::fs::metadata(store.path()).unwrap().modified().unwrap(), before);

        assert_eq!(store.prune(30, now + 40 * day).unwrap(), 2);
        assert!(store.load().is_empty());
    }

    #[test]
    fn record_respects_the_switch_and_skips_empty_text() {
        let dir = TempDir::new();
        let on = HistorySettings::default();
        let off = HistorySettings { enabled: false, ..on };

        assert!(record(&dir.config(), &off, "ceva", "ceva", Timing { audio_ms: 1000, asr_ms: 50, cleanup_ms: None }, None).is_none());
        assert!(!dir.store().path().exists());
        assert!(record(&dir.config(), &on, "  ", "", Timing { audio_ms: 1000, asr_ms: 50, cleanup_ms: None }, None).is_none());
        assert!(!dir.store().path().exists());

        let a = record(&dir.config(), &on, "Salut, lume!", "salut lume", Timing { audio_ms: 1500, asr_ms: 80, cleanup_ms: Some(300) }, None).unwrap();
        let b = record(&dir.config(), &on, "A doua.", "a doua", Timing { audio_ms: 900, asr_ms: 60, cleanup_ms: None }, None).unwrap();
        assert_eq!((a.words, a.audio_ms, a.cleanup_ms, a.app.as_deref()), (2, 1500, Some(300), None));
        assert!(a.ts > 1_600_000_000);
        assert!(a.id < b.id, "id-urile cresc in timp: {} {}", a.id, b.id);
        assert_eq!(dir.store().load(), [a, b]);

        let s = stats(&dir.config(), 0);
        assert_eq!((s.total.dictations, s.total.words, s.today.words), (2, 4, 4));
        assert_eq!(s.streak_days, 1);
    }

    #[test]
    fn prune_at_start_follows_the_settings() {
        let dir = TempDir::new();
        let store = dir.store();
        store.append(&rec("vechi", 1_000, "din 1970")).unwrap();
        store.append(&rec("nou", now().as_secs(), "de acum")).unwrap();
        prune_at_start(&dir.config(), &HistorySettings { enabled: true, retention_days: 0 });
        assert_eq!(store.load().len(), 2);
        // Curatarea se face si cu istoricul oprit: perioada de pastrare ramane valabila.
        prune_at_start(&dir.config(), &HistorySettings { enabled: false, retention_days: 30 });
        assert_eq!(ids(&store.list("", 10, None)), ["nou"]);
    }
}
