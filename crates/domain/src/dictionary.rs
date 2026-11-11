//! Dictionarul personal (`dictionary.txt`).
//!
//! Format, un termen pe linie:
//! ```text
//! # comentariu
//! NestJS | esges, nes ges, nest js
//! Lanzadera
//! ```
//! Forma corecta devine hotword pentru model si tinta inlocuirilor; variantele de dupa `|`
//! sunt ce aude modelul gresit si sunt inlocuite automat.

use regex::{Regex, RegexBuilder};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub correct: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    entries: Vec<Entry>,
    patterns: Vec<(Regex, String)>,
}

impl Dictionary {
    pub fn parse(text: &str) -> Self {
        let entries: Vec<Entry> = text
            .lines()
            .filter_map(|raw| {
                let line = raw.split('#').next().unwrap_or("").trim();
                if line.is_empty() {
                    return None;
                }
                let (correct, aliases) = match line.split_once('|') {
                    Some((c, a)) => (c.trim(), a),
                    None => (line, ""),
                };
                if correct.is_empty() {
                    return None;
                }
                let aliases = aliases
                    .split(',')
                    .map(str::trim)
                    .filter(|a| !a.is_empty())
                    .map(String::from)
                    .collect();
                Some(Entry {
                    correct: correct.to_string(),
                    aliases,
                })
            })
            .collect();

        // Toate variantele: alias-uri + forma corecta scrisa cum o scoate modelul
        // (litere mici, fara accente straine, cu spatii in loc de . si -).
        let mut pairs: Vec<(String, String)> = Vec::new();
        for e in &entries {
            let mut variants: Vec<String> = e.aliases.clone();
            let lower = e.correct.to_lowercase();
            variants.push(lower.clone());
            variants.push(strip_foreign_accents(&lower));
            variants.push(lower.replace(['.', '-'], " "));
            variants.sort();
            variants.dedup();
            for v in variants {
                if !v.trim().is_empty() {
                    pairs.push((v, e.correct.clone()));
                }
            }
        }
        // Intai variantele lungi, ca "google ladesese" sa fie prinsa inaintea lui "google".
        pairs.sort_by_key(|p| std::cmp::Reverse(p.0.chars().count()));

        let patterns = pairs
            .into_iter()
            .filter_map(|(variant, correct)| {
                let body = variant
                    .split_whitespace()
                    .map(regex::escape)
                    .collect::<Vec<_>>()
                    .join(r"\s+");
                // Limitele de cuvant se verifica manual in `apply` (regex nu are lookaround).
                RegexBuilder::new(&body)
                    .case_insensitive(true)
                    .build()
                    .ok()
                    .map(|re| (re, correct))
            })
            .collect();

        Self { entries, patterns }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn terms(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.correct.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The hotwords file for sherpa-onnx: one term per line, lowercase, without foreign accents.
    ///
    /// Only terms WITHOUT variants are boosted. A term that has variants is fixed by replacement
    /// after transcription, and boosting it too does more harm than good: every extra hotword
    /// bends the decoding of the words around it, and a model that writes by sound (the Romanian
    /// one writes "riviu" for "review") never produces the English spelling anyway. Measured on
    /// the test recordings: adding 16 English terms as hotwords took the word error rate from
    /// 8.0% to 10.7%; with this rule it stays where it was.
    pub fn hotwords(&self) -> String {
        self.entries
            .iter()
            .filter(|e| e.aliases.is_empty())
            .map(|e| strip_foreign_accents(&e.correct.to_lowercase()).replace('.', " "))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Inlocuieste variantele gresite cu forma corecta.
    pub fn apply(&self, text: &str) -> String {
        let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
        let mut out = text.to_string();
        for (re, correct) in &self.patterns {
            let mut result = String::with_capacity(out.len());
            let mut last = 0;
            for m in re.find_iter(&out) {
                let before_ok = out[..m.start()]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !is_word(c));
                let after_ok = out[m.end()..].chars().next().is_none_or(|c| !is_word(c));
                if before_ok && after_ok {
                    result.push_str(&out[last..m.start()]);
                    result.push_str(correct);
                    last = m.end();
                }
            }
            result.push_str(&out[last..]);
            out = result;
        }
        out
    }
}

/// Scoate accentele straine (í, ó, é, ñ...) dar pastreaza diacriticele romanesti.
pub fn strip_foreign_accents(s: &str) -> String {
    const KEEP: &str = "ăâîșțşţĂÂÎȘȚŞŢ";
    s.chars()
        .flat_map(|c| {
            if KEEP.contains(c) {
                vec![c]
            } else {
                c.to_string().nfd().take(1).collect::<Vec<_>>()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# test\nNestJS | esges, nest js\ndeploy | de ploi\nGoogle Ads | google adses\nPDF | pedef\ngestoría\nNext.js\nCastellón | castelon\nLanzadera\n";

    #[test]
    fn parses_entries_and_comments() {
        let d = Dictionary::parse(SAMPLE);
        assert_eq!(
            d.terms(),
            vec![
                "NestJS",
                "deploy",
                "Google Ads",
                "PDF",
                "gestoría",
                "Next.js",
                "Castellón",
                "Lanzadera"
            ]
        );
        assert_eq!(d.entries()[0].aliases, vec!["esges", "nest js"]);
    }

    #[test]
    fn hotwords_are_lowercase_without_foreign_accents() {
        let d = Dictionary::parse(SAMPLE);
        let hw = d.hotwords();
        assert!(hw.contains("gestoria"));
        assert!(hw.contains("next js"));
        assert!(hw.contains("lanzadera"));
    }

    #[test]
    fn terms_with_variants_are_not_hotwords() {
        let hw = Dictionary::parse(SAMPLE).hotwords();
        let lines: Vec<&str> = hw.lines().collect();
        // Au variante: se repara prin inlocuire, nu prin indicii catre model.
        for term in ["nestjs", "deploy", "google ads", "pdf", "castellon"] {
            assert!(!lines.contains(&term), "{term} nu trebuie sa fie hotword");
        }
        assert_eq!(lines, vec!["gestoria", "next js", "lanzadera"]);
    }

    #[test]
    fn applies_replacements() {
        let d = Dictionary::parse(SAMPLE);
        assert_eq!(
            d.apply("am terminat în nestjs și am făcut de ploi pe staging"),
            "am terminat în NestJS și am făcut deploy pe staging"
        );
        assert_eq!(
            d.apply("campania de google adses din castellon"),
            "campania de Google Ads din Castellón"
        );
        assert_eq!(
            d.apply("sun la gestoria, format pdf"),
            "sun la gestoría, format PDF"
        );
        assert_eq!(d.apply("next js e ok"), "Next.js e ok");
    }

    #[test]
    fn respects_word_boundaries() {
        let d = Dictionary::parse("PDF | pedef\ndeploy | de ploi\n");
        // "pedeferat" nu trebuie atins; "de ploile" nici.
        assert_eq!(d.apply("pedeferat si de ploile"), "pedeferat si de ploile");
        assert_eq!(d.apply("pdf pdf"), "PDF PDF");
    }

    #[test]
    fn keeps_romanian_diacritics() {
        assert_eq!(
            strip_foreign_accents("gestoría ședință țară"),
            "gestoria ședință țară"
        );
    }
}

// ------------------------------------------------------------------ editare

/// O intrare asa cum apare in fisier, cu linia ei (pentru editare) si grupul din care face parte.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileEntry {
    pub line: usize,
    pub correct: String,
    pub aliases: Vec<String>,
    pub group: Option<String>,
}

/// Fisierul dictionarului, linie cu linie. Editarea schimba doar linia atinsa: comentariile,
/// grupurile (`# Tehnic`) si ordinea scrise de mana raman exact cum erau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryFile {
    lines: Vec<String>,
}

/// (forma corecta, variante, comentariul de la sfarsitul liniei)
fn parse_line(raw: &str) -> Option<(String, Vec<String>, Option<String>)> {
    let (body, comment) = match raw.split_once('#') {
        Some((b, c)) => (b, Some(c.trim().to_string())),
        None => (raw, None),
    };
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    let (correct, aliases) = match body.split_once('|') {
        Some((c, a)) => (c.trim(), a),
        None => (body, ""),
    };
    if correct.is_empty() {
        return None;
    }
    let aliases = aliases
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(String::from)
        .collect();
    Some((correct.to_string(), aliases, comment))
}

fn format_line(correct: &str, aliases: &[String], comment: Option<&str>) -> String {
    let mut s = correct.to_string();
    if !aliases.is_empty() {
        s.push_str(" | ");
        s.push_str(&aliases.join(", "));
    }
    if let Some(c) = comment.filter(|c| !c.is_empty()) {
        s.push_str("  # ");
        s.push_str(c);
    }
    s
}

/// Curata si verifica ce a scris utilizatorul. Caracterele `|`, `#`, `,` au rol in fisier.
fn validate(correct: &str, aliases: &[String]) -> Result<(String, Vec<String>), String> {
    let correct = correct.split_whitespace().collect::<Vec<_>>().join(" ");
    if correct.is_empty() {
        return Err("Enter the correct form of the term".into());
    }
    if correct.contains(['|', '#']) {
        return Err("The term cannot contain | or #".into());
    }
    let mut out: Vec<String> = Vec::new();
    for a in aliases {
        let a = a.split_whitespace().collect::<Vec<_>>().join(" ");
        if a.is_empty() {
            continue;
        }
        if a.contains(['|', '#', ',']) {
            return Err(format!("The variant \"{a}\" cannot contain a comma, | or #"));
        }
        if !out.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
            out.push(a);
        }
    }
    Ok((correct, out))
}

impl DictionaryFile {
    pub fn parse(text: &str) -> Self {
        Self {
            lines: text.lines().map(String::from).collect(),
        }
    }

    pub fn to_text(&self) -> String {
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }

    fn is_entry(&self, i: usize) -> bool {
        self.lines.get(i).map(|l| parse_line(l).is_some()).unwrap_or(false)
    }

    /// Titlu de grup: un comentariu urmat imediat de un termen (`# Tehnic`).
    fn group_title(&self, i: usize) -> Option<String> {
        let l = self.lines.get(i)?.trim();
        let title = l.strip_prefix('#')?.trim();
        (!title.is_empty() && self.is_entry(i + 1)).then(|| title.to_string())
    }

    pub fn entries(&self) -> Vec<FileEntry> {
        let mut group: Option<String> = None;
        let mut out = Vec::new();
        for (i, raw) in self.lines.iter().enumerate() {
            if let Some(g) = self.group_title(i) {
                group = Some(g);
            } else if let Some((correct, aliases, _)) = parse_line(raw) {
                out.push(FileEntry {
                    line: i,
                    correct,
                    aliases,
                    group: group.clone(),
                });
            }
        }
        out
    }

    pub fn groups(&self) -> Vec<String> {
        (0..self.lines.len()).filter_map(|i| self.group_title(i)).collect()
    }

    fn find_duplicate(&self, correct: &str, except: Option<usize>) -> bool {
        self.entries()
            .iter()
            .any(|e| Some(e.line) != except && e.correct.to_lowercase() == correct.to_lowercase())
    }

    /// Adauga un termen la sfarsitul grupului dat (sau la sfarsitul fisierului).
    pub fn add(&mut self, correct: &str, aliases: &[String], group: Option<&str>) -> Result<(), String> {
        let (correct, aliases) = validate(correct, aliases)?;
        if self.find_duplicate(&correct, None) {
            return Err(format!("\"{correct}\" is already in the dictionary"));
        }
        let line = format_line(&correct, &aliases, None);
        let last_in_group = group.and_then(|g| {
            self.entries()
                .iter()
                .filter(|e| e.group.as_deref() == Some(g))
                .map(|e| e.line)
                .max()
        });
        match last_in_group {
            Some(i) => self.lines.insert(i + 1, line),
            None => {
                if let Some(g) = group.filter(|g| !g.trim().is_empty()) {
                    // Grup nou, la sfarsit.
                    if self.lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                        self.lines.push(String::new());
                    }
                    self.lines.push(format!("# {}", g.trim()));
                }
                self.lines.push(line);
            }
        }
        Ok(())
    }

    /// Verifica faptul ca linia mai contine termenul asteptat (fisierul putea fi schimbat intre timp).
    fn check(&self, line: usize, expected: &str) -> Result<Option<String>, String> {
        match self.lines.get(line).and_then(|l| parse_line(l)) {
            Some((c, _, comment)) if c == expected => Ok(comment),
            _ => Err("The dictionary changed in the meantime; reload the list".into()),
        }
    }

    pub fn update(&mut self, line: usize, expected: &str, correct: &str, aliases: &[String]) -> Result<(), String> {
        let comment = self.check(line, expected)?;
        let (correct, aliases) = validate(correct, aliases)?;
        if self.find_duplicate(&correct, Some(line)) {
            return Err(format!("\"{correct}\" is already in the dictionary"));
        }
        self.lines[line] = format_line(&correct, &aliases, comment.as_deref());
        Ok(())
    }

    pub fn delete(&mut self, line: usize, expected: &str) -> Result<(), String> {
        self.check(line, expected)?;
        self.lines.remove(line);
        // Un titlu de grup ramas fara niciun termen dispare si el.
        if line > 0 {
            let prev = self.lines[line - 1].trim().to_string();
            let next_is_entry = self.is_entry(line);
            if prev.starts_with('#') && !next_is_entry && line >= 2 && self.lines[line - 2].trim().is_empty() {
                let was_title = !prev.trim_start_matches('#').trim().is_empty()
                    && self.lines.get(line).map(|l| l.trim().is_empty()).unwrap_or(true);
                if was_title {
                    self.lines.remove(line - 1);
                    if self.lines.get(line - 2).map(|l| l.trim().is_empty()).unwrap_or(false)
                        && self.lines.get(line - 1).map(|l| l.trim().is_empty()).unwrap_or(true)
                    {
                        self.lines.remove(line - 2);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;

    const SAMPLE: &str = "# Dictionar\n#\n# Format: corect | variante\n\n# Tehnic\nNestJS | esges, nest js\nReact\n\n# Locuri\nCastellón | castellon  # oras\n";

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn reads_entries_with_groups_and_lines() {
        let f = DictionaryFile::parse(SAMPLE);
        let e = f.entries();
        assert_eq!(e.len(), 3);
        assert_eq!((e[0].line, e[0].correct.as_str(), e[0].group.as_deref()), (5, "NestJS", Some("Tehnic")));
        assert_eq!(e[0].aliases, s(&["esges", "nest js"]));
        assert_eq!(e[2].group.as_deref(), Some("Locuri"));
        assert_eq!(f.groups(), s(&["Tehnic", "Locuri"]));
    }

    #[test]
    fn untouched_file_roundtrips_exactly() {
        assert_eq!(DictionaryFile::parse(SAMPLE).to_text(), SAMPLE);
    }

    #[test]
    fn editing_changes_only_its_line_and_keeps_the_comment() {
        let mut f = DictionaryFile::parse(SAMPLE);
        f.update(9, "Castellón", "Castellón", &s(&["castellon", "castelon"])).unwrap();
        let expected = SAMPLE.replace("Castellón | castellon  # oras", "Castellón | castellon, castelon  # oras");
        assert_eq!(f.to_text(), expected);
        // Parserul folosit la dictare vede acelasi lucru.
        assert_eq!(Dictionary::parse(&f.to_text()).entries()[2].aliases, s(&["castellon", "castelon"]));
    }

    #[test]
    fn adds_at_the_end_of_the_group_or_of_the_file() {
        let mut f = DictionaryFile::parse(SAMPLE);
        f.add("Tailwind", &[], Some("Tehnic")).unwrap();
        assert_eq!(f.entries()[2].correct, "Tailwind");
        assert_eq!(f.entries()[2].group.as_deref(), Some("Tehnic"));
        f.add("Ana", &s(&["anna"]), None).unwrap();
        assert!(f.to_text().ends_with("Ana | anna\n"));
        f.add("Lanzadera", &[], Some("Proiecte")).unwrap();
        let last = f.entries().pop().unwrap();
        assert_eq!((last.correct.as_str(), last.group.as_deref()), ("Lanzadera", Some("Proiecte")));
    }

    #[test]
    fn rejects_duplicates_bad_characters_and_stale_lines() {
        let mut f = DictionaryFile::parse(SAMPLE);
        assert!(f.add("nestjs", &[], None).is_err());
        assert!(f.add("", &[], None).is_err());
        assert!(f.add("a|b", &[], None).is_err());
        assert!(f.add("Ok", &s(&["a,b"]), None).is_err());
        assert!(f.update(6, "NestJS", "X", &[]).is_err()); // linia 6 e React
        assert!(f.delete(5, "React").is_err());
        assert_eq!(f.to_text(), SAMPLE);
    }

    #[test]
    fn delete_removes_the_line_and_an_emptied_group_title() {
        let mut f = DictionaryFile::parse(SAMPLE);
        f.delete(6, "React").unwrap();
        assert_eq!(f.entries().len(), 2);
        assert_eq!(f.groups(), s(&["Tehnic", "Locuri"]));
        f.delete(8, "Castellón").unwrap();
        assert_eq!(f.groups(), s(&["Tehnic"]));
        assert!(!f.to_text().contains("Locuri"));
        // Antetul cu explicatii ramane.
        assert!(f.to_text().starts_with("# Dictionar\n#\n# Format: corect | variante\n"));
    }
}
