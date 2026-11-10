//! Istoricul dictarilor: inregistrarea, cautarea, pastrarea pe zile si statisticile.
//! Logica pura; citirea si scrierea fisierului sunt in `crates/app/src/history.rs`.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// Viteza de tastare cu care comparam vorbitul (cuvinte pe minut). E o estimare, nu o masuratoare.
pub const TYPING_WPM: u64 = 40;

const DAY_SECS: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistorySettings {
    pub enabled: bool,
    /// Cate zile pastram dictarile; 0 = pentru totdeauna.
    pub retention_days: u32,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 90,
        }
    }
}

/// O dictare, o linie din `history.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub id: String,
    /// Secunde Unix.
    pub ts: u64,
    /// Textul lipit.
    pub text: String,
    /// Textul de dinainte de corecturi.
    #[serde(default)]
    pub raw: String,
    #[serde(default)]
    pub audio_ms: u64,
    #[serde(default)]
    pub asr_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_ms: Option<u64>,
    #[serde(default)]
    pub words: u32,
    /// Aplicatia in care s-a lipit textul (nume sau bundle id), cand e cunoscuta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

impl HistoryRecord {
    pub fn new(id: String, ts: u64, text: &str, raw: &str, audio_ms: u64, asr_ms: u64, cleanup_ms: Option<u64>) -> Self {
        Self {
            id,
            ts,
            words: count_words(text),
            text: text.to_string(),
            raw: raw.to_string(),
            audio_ms,
            asr_ms,
            cleanup_ms,
            app: None,
        }
    }

    /// Linia din fisier (JSON pe un singur rand, fara sfarsit de linie).
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("a record always serializes")
    }

    /// `None` pentru o linie goala sau stricata.
    pub fn parse_line(line: &str) -> Option<Self> {
        let record: Self = serde_json::from_str(line.trim()).ok()?;
        (!record.id.is_empty()).then_some(record)
    }

    /// Cautare fara majuscule si fara diacritice, in textul final si in cel brut.
    /// `folded_query` vine din `fold`; gol = se potriveste orice.
    pub fn matches(&self, folded_query: &str) -> bool {
        folded_query.is_empty() || fold(&self.text).contains(folded_query) || fold(&self.raw).contains(folded_query)
    }
}

/// Cuvintele dintr-un text: bucati despartite de spatii care au macar o litera sau o cifra
/// ("s-a" e un cuvant, o liniuta singura nu e).
pub fn count_words(text: &str) -> u32 {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count() as u32
}

/// Forma pentru cautare: litere mici, fara diacritice ("Știință" -> "stiinta").
pub fn fold(s: &str) -> String {
    s.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Inregistrarea mai e in perioada de pastrare? `retention_days` 0 = pastram tot.
pub fn within_retention(ts: u64, retention_days: u32, now: u64) -> bool {
    retention_days == 0 || ts >= now.saturating_sub(u64::from(retention_days) * DAY_SECS as u64)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Period {
    pub dictations: u32,
    pub words: u64,
}

impl Period {
    fn add(&mut self, r: &HistoryRecord) {
        self.dictations += 1;
        self.words += u64::from(r.words);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DayWords {
    /// Inceputul zilei (miezul noptii local), in secunde Unix.
    pub start: i64,
    pub words: u64,
    pub dictations: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryStats {
    pub today: Period,
    /// Ultimele 7 zile, cu azi.
    pub week: Period,
    pub total: Period,
    /// Cat s-a vorbit in total.
    pub audio_ms: u64,
    /// Cuvinte pe minut cat timp se vorbeste.
    pub wpm: f64,
    /// Estimare: cat ar fi durat tastarea la `TYPING_WPM` minus cat a durat vorbitul.
    pub saved_ms: u64,
    /// Zile la rand cu cel putin o dictare, pana azi (sau pana ieri, daca azi nu s-a dictat inca).
    pub streak_days: u32,
    /// Ultimele 14 zile, de la cea mai veche la azi.
    pub days: Vec<DayWords>,
}

pub const CHART_DAYS: i64 = 14;

/// Statisticile se calculeaza din inregistrari, nu din contoare separate.
/// `utc_offset_secs` e decalajul fusului local (ziua incepe la miezul noptii local).
pub fn stats(records: &[HistoryRecord], now: u64, utc_offset_secs: i64) -> HistoryStats {
    let day_of = |ts: u64| (ts as i64 + utc_offset_secs).div_euclid(DAY_SECS);
    let today = day_of(now);
    let mut out = HistoryStats {
        today: Period::default(),
        week: Period::default(),
        total: Period::default(),
        audio_ms: 0,
        wpm: 0.0,
        saved_ms: 0,
        streak_days: 0,
        days: (0..CHART_DAYS)
            .map(|i| DayWords {
                start: (today - (CHART_DAYS - 1) + i) * DAY_SECS - utc_offset_secs,
                words: 0,
                dictations: 0,
            })
            .collect(),
    };
    let mut active_days = std::collections::BTreeSet::new();
    for r in records {
        let day = day_of(r.ts);
        active_days.insert(day);
        out.total.add(r);
        out.audio_ms += r.audio_ms;
        if day == today {
            out.today.add(r);
        }
        if day <= today && day > today - 7 {
            out.week.add(r);
        }
        let slot = day - (today - (CHART_DAYS - 1));
        if (0..CHART_DAYS).contains(&slot) {
            let d = &mut out.days[slot as usize];
            d.words += u64::from(r.words);
            d.dictations += 1;
        }
    }
    if out.audio_ms > 0 {
        out.wpm = out.total.words as f64 / (out.audio_ms as f64 / 60_000.0);
    }
    out.saved_ms = (out.total.words * 60_000 / TYPING_WPM).saturating_sub(out.audio_ms);
    let mut day = if active_days.contains(&today) { today } else { today - 1 };
    while active_days.contains(&day) {
        out.streak_days += 1;
        day -= 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, ts: u64, text: &str, audio_ms: u64) -> HistoryRecord {
        HistoryRecord::new(id.into(), ts, text, text, audio_ms, 100, None)
    }

    #[test]
    fn settings_defaults() {
        let s = HistorySettings::default();
        assert!(s.enabled);
        assert_eq!(s.retention_days, 90);
        let partial: HistorySettings = toml::from_str("enabled = false\n").unwrap();
        assert_eq!(partial, HistorySettings { enabled: false, retention_days: 90 });
    }

    #[test]
    fn record_roundtrips_on_one_line() {
        let mut r = HistoryRecord::new("a1".into(), 1_700_000_000, "Salut,\n„lume”!", "salut lume", 2500, 120, Some(800));
        let line = r.to_line();
        assert!(!line.contains('\n'));
        assert!(!line.contains("\"app\""));
        assert_eq!(HistoryRecord::parse_line(&line), Some(r.clone()));

        r.cleanup_ms = None;
        r.app = Some("com.apple.Notes".into());
        assert_eq!(HistoryRecord::parse_line(&r.to_line()), Some(r));
    }

    #[test]
    fn parse_tolerates_missing_optional_fields_and_rejects_garbage() {
        let r = HistoryRecord::parse_line(r#"{"id":"x","ts":5,"text":"unu doi"}"#).unwrap();
        assert_eq!((r.raw.as_str(), r.audio_ms, r.words, r.app), ("", 0, 0, None));
        // Campuri necunoscute (versiuni viitoare) nu strica linia.
        assert!(HistoryRecord::parse_line(r#"{"id":"x","ts":5,"text":"a","nou":1}"#).is_some());
        for bad in ["", "   ", "{\"id\":\"x\",\"ts\":5,\"te", "nu e json", "{\"ts\":5,\"text\":\"a\"}", "{\"id\":\"\",\"ts\":5,\"text\":\"a\"}"] {
            assert_eq!(HistoryRecord::parse_line(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn counts_words_with_diacritics_and_punctuation() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("  \n "), 0);
        assert_eq!(count_words("Salut, lume!"), 2);
        assert_eq!(count_words("Știință și țară: în 2024 s-a întâmplat."), 7);
        assert_eq!(count_words("unu - doi — trei ..."), 3);
        assert_eq!(count_words("linia unu\nlinia doi\tgata"), 5);
        assert_eq!(count_words("Castellón, São Paulo, 東京"), 4);
    }

    #[test]
    fn search_ignores_case_and_diacritics() {
        let r = HistoryRecord::new("1".into(), 0, "Mâine mergem la Brașov.", "maine mergem la brasov cu trenu", 0, 0, None);
        for q in ["brasov", "BRAȘOV", "braşov", "Mâine", "maine merg", " la bra ", ""] {
            assert!(r.matches(&fold(q)), "{q:?}");
        }
        // Se cauta si in textul brut.
        assert!(r.matches(&fold("trenu")));
        assert!(!r.matches(&fold("cluj")));
        // Forma descompusa (s + virgula combinata) se potriveste cu cea compusa.
        assert!(r.matches(&fold("Bras\u{326}ov")));
    }

    #[test]
    fn retention_keeps_recent_and_zero_keeps_all() {
        let now = 1_000 * 86_400;
        assert!(within_retention(now - 89 * 86_400, 90, now));
        assert!(within_retention(now - 90 * 86_400, 90, now));
        assert!(!within_retention(now - 90 * 86_400 - 1, 90, now));
        assert!(within_retention(0, 0, now));
        assert!(within_retention(now + 50, 30, now));
        // Ceas pus gresit (inainte de 1970 + perioada): nu sterge nimic din greseala.
        assert!(within_retention(0, 30, 10));
    }

    const DAY: u64 = 86_400;
    /// Miezul zilei 1000 (UTC), ca testele sa nu depinda de marginea zilei.
    const NOON: u64 = 1_000 * DAY + 12 * 3_600;

    #[test]
    fn stats_of_an_empty_history() {
        let s = stats(&[], NOON, 0);
        assert_eq!((s.today, s.week, s.total), (Period::default(), Period::default(), Period::default()));
        assert_eq!((s.audio_ms, s.saved_ms, s.streak_days), (0, 0, 0));
        assert_eq!(s.wpm, 0.0);
        assert_eq!(s.days.len(), 14);
        assert_eq!(s.days[13].start, 1_000 * DAY as i64);
        assert_eq!(s.days[0].start, 987 * DAY as i64);
    }

    #[test]
    fn stats_periods_speed_and_time_saved() {
        let ten = "unu doi trei patru cinci șase șapte opt nouă zece";
        let records = [
            rec("a", NOON - 20 * DAY, ten, 6_000),
            rec("b", NOON - 7 * DAY, ten, 6_000),
            rec("c", NOON - 6 * DAY, ten, 6_000),
            rec("d", NOON - 3_600, ten, 6_000),
            rec("e", NOON, ten, 6_000),
        ];
        let s = stats(&records, NOON, 0);
        assert_eq!(s.today, Period { dictations: 2, words: 20 });
        assert_eq!(s.week, Period { dictations: 3, words: 30 });
        assert_eq!(s.total, Period { dictations: 5, words: 50 });
        assert_eq!(s.audio_ms, 30_000);
        // 50 de cuvinte in jumatate de minut.
        assert!((s.wpm - 100.0).abs() < 1e-9);
        // La 40 de cuvinte pe minut, 50 de cuvinte = 75 s de tastat; vorbitul a durat 30 s.
        assert_eq!(s.saved_ms, 45_000);
        assert_eq!(s.days[13], DayWords { start: 1_000 * DAY as i64, words: 20, dictations: 2 });
        assert_eq!(s.days[7].words, 10);
        assert_eq!(s.days[6].words, 10);
        assert_eq!(s.days.iter().map(|d| d.words).sum::<u64>(), 40);
    }

    #[test]
    fn slow_speech_never_gives_negative_savings() {
        let s = stats(&[rec("a", NOON, "un cuvant", 60_000)], NOON, 0);
        assert_eq!(s.saved_ms, 0);
        assert!((s.wpm - 2.0).abs() < 1e-9);
    }

    #[test]
    fn streak_counts_consecutive_days() {
        let on = |days_ago: &[u64]| -> Vec<HistoryRecord> {
            days_ago.iter().map(|d| rec(&d.to_string(), NOON - d * DAY, "da", 500)).collect()
        };
        assert_eq!(stats(&on(&[0, 1, 2, 4]), NOON, 0).streak_days, 3);
        // Azi inca nu s-a dictat: seria de pana ieri ramane in picioare.
        assert_eq!(stats(&on(&[1, 2]), NOON, 0).streak_days, 2);
        assert_eq!(stats(&on(&[2, 3]), NOON, 0).streak_days, 0);
        // Mai multe dictari in aceeasi zi numara o singura zi.
        assert_eq!(stats(&on(&[0, 0, 0]), NOON, 0).streak_days, 1);
    }

    #[test]
    fn days_follow_the_local_timezone() {
        // 23:30 UTC in ziua 999 este 01:30 in ziua 1000 la UTC+2.
        let late = 1_000 * DAY - 1_800;
        let now = 1_000 * DAY + 3_600;
        let records = [rec("a", late, "unu doi", 1_000)];
        assert_eq!(stats(&records, now, 0).today.words, 0);
        assert_eq!(stats(&records, now, 0).days[12].words, 2);
        let local = stats(&records, now, 7_200);
        assert_eq!(local.today.words, 2);
        assert_eq!(local.days[13].start, 1_000 * DAY as i64 - 7_200);
        // La vest de Greenwich ziua locala incepe mai tarziu.
        assert_eq!(stats(&records, now, -5 * 3_600).today.words, 2);
        assert_eq!(stats(&records, now + DAY, -5 * 3_600).today.words, 0);
    }
}
