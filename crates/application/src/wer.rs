//! Rata de eroare la nivel de cuvant (WER) si caracter (CER), pentru bancul de test.

use vf_domain::format::normalize_for_wer;

fn edit_distance<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Erori si lungimea referintei, la nivel de cuvant.
pub fn word_errors(reference: &str, hypothesis: &str) -> (usize, usize) {
    let r = normalize_for_wer(reference);
    let h = normalize_for_wer(hypothesis);
    let r: Vec<&str> = r.split_whitespace().collect();
    let h: Vec<&str> = h.split_whitespace().collect();
    (edit_distance(&r, &h), r.len())
}

/// Erori si lungimea referintei, la nivel de caracter.
pub fn char_errors(reference: &str, hypothesis: &str) -> (usize, usize) {
    let r: Vec<char> = normalize_for_wer(reference).chars().collect();
    let h: Vec<char> = normalize_for_wer(hypothesis).chars().collect();
    (edit_distance(&r, &h), r.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_word_errors() {
        assert_eq!(word_errors("Salut, ce faci?", "salut ce faci"), (0, 3));
        assert_eq!(word_errors("a b c d", "a x c"), (2, 4));
    }

    #[test]
    fn counts_char_errors() {
        assert_eq!(char_errors("abc", "abd"), (1, 3));
    }
}
