//! Formatarea finala a textului si normalizarea folosita la masurarea erorilor.

/// Diacritice romanesti corecte: ş/ţ cu sedila devin ș/ț cu virgula.
pub fn fix_romanian_diacritics(text: &str) -> String {
    text.replace('ş', "ș")
        .replace('ţ', "ț")
        .replace('Ş', "Ș")
        .replace('Ţ', "Ț")
}

/// Formatare de baza: diacritice corecte, majuscula la inceput, punct la final.
pub fn basic_format(text: &str) -> String {
    let t = fix_romanian_diacritics(text.trim());
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = t.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(['.', '?', '!', '…', ':']) {
        out.push('.');
    }
    out
}

/// Normalizare pentru WER: litere mici, fara punctuatie, diacritice unificate.
pub fn normalize_for_wer(text: &str) -> String {
    fix_romanian_diacritics(&text.to_lowercase())
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_basic() {
        assert_eq!(basic_format("  salut  ce faci "), "Salut ce faci.");
        assert_eq!(basic_format("ştiu că e ţară"), "Știu că e țară.");
        assert_eq!(basic_format("vii mâine?"), "Vii mâine?");
        assert_eq!(basic_format(""), "");
    }

    #[test]
    fn normalizes_for_wer() {
        assert_eq!(
            normalize_for_wer("Bună ziua, site-ul e GATA!"),
            "bună ziua site ul e gata"
        );
    }
}
