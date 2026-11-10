//! Snippets: texte salvate care apar cand rostesti un declansator.
//!
//! "semnatura mea" -> semnatura de e-mail pe mai multe randuri; "linkul de programari" -> un URL.
//! Aici e doar logica pura: potrivirea in textul dictat, validarea si editarea listei.
//! Citirea / scrierea fisierului `snippets.json` e in `crates/app/src/snippets.rs`.

use serde::{Deserialize, Serialize};
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

fn yes() -> bool {
    true
}

fn version() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    /// Stabil: nu se schimba la editare.
    pub id: String,
    /// Ce spui ca sa apara textul.
    pub trigger: String,
    /// Cum mai aude modelul declansatorul (optional).
    #[serde(default)]
    pub variants: Vec<String>,
    /// Textul inserat, exact asa (poate avea mai multe randuri).
    pub text: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// Ce trimite interfata la salvare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetDraft {
    pub trigger: String,
    pub variants: Vec<String>,
    pub text: String,
    pub enabled: bool,
}

// ------------------------------------------------------------------ normalizare

/// Acelasi "caracter de cuvant" ca in dictionar (`Dictionary::apply`).
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Forma in care se compara cuvintele: litere mici, fara diacritice (si romanesti, si straine),
/// fara cratime. Modelul scrie cand "semnătura", cand "semnatura".
fn fold(word: &str) -> String {
    word.chars()
        .flat_map(char::to_lowercase)
        .nfd()
        .filter(|c| c.is_alphanumeric() && !is_combining_mark(*c))
        .collect()
}

struct Token {
    start: usize,
    end: usize,
    key: String,
}

/// Cuvintele din text, cu pozitia lor. Tot ce nu e caracter de cuvant (spatii, punctuatie)
/// doar desparte cuvintele, deci "semnătura, mea." are aceleasi cuvinte ca "semnatura mea".
fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut push = |s: usize, e: usize| {
        let key = fold(&text[s..e]);
        if !key.is_empty() {
            out.push(Token { start: s, end: e, key });
        }
    };
    for (i, c) in text.char_indices() {
        match (is_word(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                push(s, i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        push(s, text.len());
    }
    out
}

fn keys(phrase: &str) -> Vec<String> {
    tokenize(phrase).into_iter().map(|t| t.key).collect()
}

// ------------------------------------------------------------------ potrivire

/// Lista de declansatoare pregatita pentru cautare. Se reconstruieste la fiecare schimbare.
#[derive(Debug, Clone, Default)]
pub struct SnippetMatcher {
    /// (cuvintele declansatorului, textul), cele mai lungi intai.
    patterns: Vec<(Vec<String>, String)>,
}

impl SnippetMatcher {
    pub fn new(snippets: &[Snippet]) -> Self {
        let mut patterns: Vec<(Vec<String>, String)> = Vec::new();
        for s in snippets.iter().filter(|s| s.enabled && !s.text.is_empty()) {
            for phrase in std::iter::once(&s.trigger).chain(&s.variants) {
                let k = keys(phrase);
                if !k.is_empty() && !patterns.iter().any(|(p, _)| *p == k) {
                    patterns.push((k, s.text.clone()));
                }
            }
        }
        // Cel mai lung declansator castiga: "linkul de programari online" inaintea lui "linkul de programari".
        patterns.sort_by_key(|(k, _)| {
            std::cmp::Reverse((k.len(), k.iter().map(|w| w.chars().count()).sum::<usize>()))
        });
        Self { patterns }
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Inlocuieste fiecare declansator gasit. `None` daca textul nu contine niciunul.
    ///
    /// Daca dictarea e doar declansatorul (plus punctuatie), rezultatul e exact textul salvat.
    pub fn expand(&self, text: &str) -> Option<String> {
        if self.patterns.is_empty() {
            return None;
        }
        let tokens = tokenize(text);
        // (inceput, sfarsit, textul snippet-ului)
        let mut found: Vec<(usize, usize, &str)> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let hit = self.patterns.iter().find(|(k, _)| {
                k.len() <= tokens.len() - i && k.iter().zip(&tokens[i..]).all(|(a, t)| *a == t.key)
            });
            match hit {
                Some((k, snippet)) => {
                    found.push((tokens[i].start, tokens[i + k.len() - 1].end, snippet));
                    i += k.len();
                }
                None => i += 1,
            }
        }
        let (first, last) = (found.first()?, tokens.last()?);
        if found.len() == 1 && first.0 == tokens[0].start && first.1 == last.end {
            return Some(first.2.to_string());
        }

        let mut out = String::with_capacity(text.len());
        let mut pos = 0;
        for (start, end, snippet) in found {
            out.push_str(&text[pos..start]);
            out.push_str(snippet);
            pos = end;
            // Snippet-ul isi aduce propriul punct: nu-l dublam cu cel pus de model dupa declansator.
            if snippet.ends_with(['.', '!', '?']) && text[pos..].starts_with('.') && !text[pos..].starts_with("..") {
                pos += 1;
            }
        }
        out.push_str(&text[pos..]);
        Some(out)
    }
}

// ------------------------------------------------------------------ validare si editare

fn squeeze(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Un singur cuvant scurt ("link", "salut") apare des in vorbire si ar declansa din greseala.
fn check_phrase(phrase: &str) -> Result<(), String> {
    let k = keys(phrase);
    let letters = k.first().map(|w| w.chars().count()).unwrap_or(0);
    if k.len() >= 2 || letters >= 6 {
        return Ok(());
    }
    Err(format!(
        "\"{phrase}\" is too short: a single short word comes up often in speech and would replace text by mistake. \
         Use at least two words, or one word of at least 6 letters."
    ))
}

fn validate(draft: &SnippetDraft) -> Result<SnippetDraft, String> {
    let trigger = squeeze(&draft.trigger);
    if keys(&trigger).is_empty() {
        return Err("Enter the phrase you say to insert the text".into());
    }
    check_phrase(&trigger)?;
    let mut variants: Vec<String> = Vec::new();
    let mut seen = vec![keys(&trigger)];
    for v in &draft.variants {
        let v = squeeze(v);
        let k = keys(&v);
        if k.is_empty() || seen.contains(&k) {
            continue;
        }
        check_phrase(&v)?;
        seen.push(k);
        variants.push(v);
    }
    // Fara randuri goale la capete: un Enter ratacit la sfarsit ar trimite mesajul in aplicatiile de chat.
    let text = draft.text.replace("\r\n", "\n").trim().to_string();
    if text.is_empty() {
        return Err("Enter the text to insert".into());
    }
    Ok(SnippetDraft {
        trigger,
        variants,
        text,
        enabled: draft.enabled,
    })
}

/// Continutul fisierului `snippets.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnippetSet {
    #[serde(default = "version")]
    version: u32,
    #[serde(default)]
    snippets: Vec<Snippet>,
}

impl Default for SnippetSet {
    fn default() -> Self {
        Self {
            version: version(),
            snippets: Vec::new(),
        }
    }
}

const GONE: &str = "The snippet no longer exists; reload the list";

impl SnippetSet {
    pub fn new(snippets: Vec<Snippet>) -> Self {
        Self {
            version: version(),
            snippets,
        }
    }

    pub fn snippets(&self) -> &[Snippet] {
        &self.snippets
    }

    pub fn matcher(&self) -> SnippetMatcher {
        SnippetMatcher::new(&self.snippets)
    }

    /// Acelasi declansator (dupa normalizare) la doua snippet-uri: n-am sti pe care sa-l punem.
    fn check_duplicates(&self, draft: &SnippetDraft, except: Option<&str>) -> Result<(), String> {
        for phrase in std::iter::once(&draft.trigger).chain(&draft.variants) {
            let k = keys(phrase);
            let taken = self
                .snippets
                .iter()
                .filter(|s| Some(s.id.as_str()) != except)
                .find(|s| std::iter::once(&s.trigger).chain(&s.variants).any(|p| keys(p) == k));
            if let Some(other) = taken {
                return Err(format!(
                    "\"{phrase}\" is already used by the snippet \"{}\"",
                    other.trigger
                ));
            }
        }
        Ok(())
    }

    /// Adauga un snippet. `now_ms` (ceasul) da id-ul; intoarce id-ul ales.
    pub fn add(&mut self, draft: &SnippetDraft, now_ms: u64) -> Result<String, String> {
        let d = validate(draft)?;
        self.check_duplicates(&d, None)?;
        let mut n = now_ms;
        let id = loop {
            let id = format!("s{n:x}");
            if !self.snippets.iter().any(|s| s.id == id) {
                break id;
            }
            n += 1;
        };
        self.snippets.push(Snippet {
            id: id.clone(),
            trigger: d.trigger,
            variants: d.variants,
            text: d.text,
            enabled: d.enabled,
        });
        Ok(id)
    }

    pub fn update(&mut self, id: &str, draft: &SnippetDraft) -> Result<(), String> {
        let i = self.snippets.iter().position(|s| s.id == id).ok_or(GONE)?;
        let d = validate(draft)?;
        self.check_duplicates(&d, Some(id))?;
        let s = &mut self.snippets[i];
        s.trigger = d.trigger;
        s.variants = d.variants;
        s.text = d.text;
        s.enabled = d.enabled;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let i = self.snippets.iter().position(|s| s.id == id).ok_or(GONE)?;
        self.snippets.remove(i);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNATURE: &str = "Cu drag,\nAna\nAcme";
    const LINK: &str = "https://cal.com/acme";

    fn snip(id: &str, trigger: &str, variants: &[&str], text: &str) -> Snippet {
        Snippet {
            id: id.into(),
            trigger: trigger.into(),
            variants: variants.iter().map(|v| v.to_string()).collect(),
            text: text.into(),
            enabled: true,
        }
    }

    fn draft(trigger: &str, variants: &[&str], text: &str) -> SnippetDraft {
        SnippetDraft {
            trigger: trigger.into(),
            variants: variants.iter().map(|v| v.to_string()).collect(),
            text: text.into(),
            enabled: true,
        }
    }

    fn sample() -> SnippetMatcher {
        SnippetMatcher::new(&[
            snip("1", "semnătura mea", &["semnatura me"], SIGNATURE),
            snip("2", "linkul de programări", &[], LINK),
            snip("3", "adresa firmei", &[], "Str. Lungă nr. 5, Cluj."),
            snip("4", "linkuri", &[], "LINKURI"),
        ])
    }

    #[test]
    fn dictation_that_is_only_the_trigger_becomes_exactly_the_text() {
        let m = sample();
        for said in [
            "semnătura mea",
            "Semnătura mea.",
            "semnatura mea",
            "SEMNĂTURA MEA!",
            "Semnatura, mea.",
            "  „semnătura mea”…  ",
            "semnatura me",
            "Semnătura\nmea.",
        ] {
            assert_eq!(m.expand(said).as_deref(), Some(SIGNATURE), "{said:?}");
        }
        assert_eq!(m.expand("Linkul de programari.").as_deref(), Some(LINK));
    }

    #[test]
    fn old_cedilla_diacritics_match_too() {
        // ş / ţ cu sedila (tastaturi vechi) si ș / ț cu virgula sunt acelasi cuvant.
        let m = SnippetMatcher::new(&[snip("1", "ședința de marți", &[], "X")]);
        assert_eq!(m.expand("şedinţa de marţi").as_deref(), Some("X"));
        assert_eq!(m.expand("sedinta de marti").as_deref(), Some("X"));
    }

    #[test]
    fn inside_a_sentence_it_is_replaced_in_place() {
        let m = sample();
        assert_eq!(
            m.expand("Îți trimit linkul de programări, alege o oră.").as_deref(),
            Some("Îți trimit https://cal.com/acme, alege o oră.")
        );
        assert_eq!(
            m.expand("Mulțumesc mult! Semnătura mea").as_deref(),
            Some("Mulțumesc mult! Cu drag,\nAna\nAcme")
        );
        // Punctuatia pusa de model in interiorul declansatorului dispare odata cu el.
        assert_eq!(
            m.expand("uite linkul, de programări acolo").as_deref(),
            Some("uite https://cal.com/acme acolo")
        );
    }

    #[test]
    fn every_occurrence_is_replaced() {
        let m = sample();
        assert_eq!(
            m.expand("linkul de programări și iar linkul de programari").as_deref(),
            Some("https://cal.com/acme și iar https://cal.com/acme")
        );
        // Doua declansatoare si nimic altceva: tot inlocuire pe loc.
        assert_eq!(
            m.expand("Adresa firmei. Semnătura mea.").as_deref(),
            Some("Str. Lungă nr. 5, Cluj. Cu drag,\nAna\nAcme.")
        );
    }

    #[test]
    fn only_whole_words_match() {
        let m = SnippetMatcher::new(&[snip("1", "link", &[], "L"), snip("2", "adresa firmei", &[], "A")]);
        assert_eq!(m.expand("am multe linkuri"), None);
        assert_eq!(m.expand("hyperlink"), None);
        assert_eq!(m.expand("link-ul meu"), None);
        assert_eq!(m.expand("adresa firmeilor"), None);
        assert_eq!(m.expand("preadresa firmei"), None);
        assert_eq!(m.expand("un link aici").as_deref(), Some("un L aici"));
        assert_eq!(m.expand("(link)").as_deref(), Some("L"));
    }

    #[test]
    fn longest_trigger_wins_at_the_same_place() {
        let m = SnippetMatcher::new(&[
            snip("1", "adresa firmei", &[], "SCURT"),
            snip("2", "adresa firmei din Cluj", &[], "LUNG"),
        ]);
        assert_eq!(m.expand("scrie adresa firmei din Cluj aici").as_deref(), Some("scrie LUNG aici"));
        assert_eq!(m.expand("scrie adresa firmei din Arad").as_deref(), Some("scrie SCURT din Arad"));
        // Un declansator inceput dar neterminat nu blocheaza unul care incepe mai tarziu.
        assert_eq!(m.expand("adresa adresa firmei").as_deref(), Some("adresa SCURT"));
    }

    #[test]
    fn inserted_text_is_not_scanned_again() {
        let m = SnippetMatcher::new(&[
            snip("1", "primul text", &[], "al doilea text"),
            snip("2", "al doilea text", &[], "gata"),
        ]);
        assert_eq!(m.expand("zic primul text acum").as_deref(), Some("zic al doilea text acum"));
    }

    #[test]
    fn a_snippet_ending_a_sentence_does_not_double_the_period() {
        let m = sample();
        assert_eq!(
            m.expand("Ne vedem la adresa firmei. Pe curând!").as_deref(),
            Some("Ne vedem la Str. Lungă nr. 5, Cluj. Pe curând!")
        );
        assert_eq!(
            m.expand("Vino la adresa firmei, te rog.").as_deref(),
            Some("Vino la Str. Lungă nr. 5, Cluj., te rog.")
        );
        assert_eq!(
            m.expand("hm, adresa firmei... nu mai știu").as_deref(),
            Some("hm, Str. Lungă nr. 5, Cluj.... nu mai știu")
        );
    }

    #[test]
    fn no_trigger_means_no_change() {
        let m = sample();
        assert_eq!(m.expand("o propoziție obișnuită, fără nimic."), None);
        assert_eq!(m.expand(""), None);
        assert_eq!(m.expand("... !"), None);
        assert_eq!(SnippetMatcher::default().expand("semnătura mea"), None);
        assert!(SnippetMatcher::default().is_empty());
    }

    #[test]
    fn disabled_snippets_do_not_fire() {
        let mut s = snip("1", "semnătura mea", &[], SIGNATURE);
        s.enabled = false;
        assert_eq!(SnippetMatcher::new(&[s]).expand("semnătura mea"), None);
    }

    #[test]
    fn add_cleans_up_and_gives_stable_unique_ids() {
        let mut set = SnippetSet::default();
        let a = set
            .add(&draft("  Semnătura   mea ", &["semnatura mea", "semnatura me", " ", "Semnatura  ME"], "\n Salut \r\n tu \n\n"), 255)
            .unwrap();
        let b = set.add(&draft("adresa firmei", &[], "x"), 255).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("sff", "s100"));
        let s = &set.snippets()[0];
        assert_eq!(s.trigger, "Semnătura mea");
        // Varianta identica cu declansatorul (dupa normalizare) si dublurile dispar.
        assert_eq!(s.variants, vec!["semnatura me"]);
        assert_eq!(s.text, "Salut \n tu");
        set.update(&a, &draft("semnătura mea", &[], "alt text")).unwrap();
        assert_eq!(set.snippets()[0].id, a);
        assert_eq!(set.snippets()[0].text, "alt text");
    }

    #[test]
    fn rejects_short_single_word_triggers_with_an_explanation() {
        let mut set = SnippetSet::default();
        let e = set.add(&draft("link", &[], "x"), 1).unwrap_err();
        assert!(e.contains("too short") && e.contains("two words"), "{e}");
        assert!(set.add(&draft("salut", &[], "x"), 1).is_err());
        assert!(set.add(&draft("adresa firmei", &["adr"], "x"), 1).is_err());
        assert!(set.snippets().is_empty());
        // 6 litere sau doua cuvinte sunt in regula.
        set.add(&draft("adresă", &[], "x"), 1).unwrap();
        set.add(&draft("de la", &[], "x"), 2).unwrap();
    }

    #[test]
    fn rejects_empty_trigger_or_text() {
        let mut set = SnippetSet::default();
        assert!(set.add(&draft("", &[], "x"), 1).is_err());
        assert!(set.add(&draft(" ?! ", &[], "x"), 1).is_err());
        assert!(set.add(&draft("adresa firmei", &[], " \n "), 1).is_err());
        assert!(set.snippets().is_empty());
    }

    #[test]
    fn rejects_duplicate_triggers_after_normalisation() {
        let mut set = SnippetSet::default();
        let a = set.add(&draft("semnătura mea", &["semnatura me"], "x"), 1).unwrap();
        assert!(set.add(&draft("Semnatura, mea!", &[], "y"), 2).is_err());
        assert!(set.add(&draft("alt declanșator", &["SEMNATURA ME"], "y"), 2).is_err());
        let b = set.add(&draft("adresa firmei", &[], "y"), 2).unwrap();
        let e = set.update(&b, &draft("semnatura mea", &[], "y")).unwrap_err();
        assert!(e.contains("already used"), "{e}");
        assert_eq!(set.snippets()[1].trigger, "adresa firmei");
        // Un snippet nu se ciocneste cu el insusi.
        set.update(&a, &draft("semnătura mea", &["semnatura me"], "z")).unwrap();
    }

    #[test]
    fn update_and_delete_of_a_missing_snippet_fail() {
        let mut set = SnippetSet::default();
        let a = set.add(&draft("adresa firmei", &[], "x"), 1).unwrap();
        assert!(set.update("nu-exista", &draft("alt text", &[], "x")).is_err());
        assert!(set.delete("nu-exista").is_err());
        set.delete(&a).unwrap();
        assert!(set.snippets().is_empty());
    }

    #[test]
    fn set_builds_the_matcher_it_describes() {
        let mut set = SnippetSet::new(vec![snip("1", "semnătura mea", &[], SIGNATURE)]);
        assert_eq!(set.matcher().expand("Semnatura mea.").as_deref(), Some(SIGNATURE));
        set.delete("1").unwrap();
        assert!(set.matcher().is_empty());
    }
}
