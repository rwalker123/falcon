//! Kokoro's vocabulary, the phoneme → token map, and the 510-token chunker.
//!
//! Mirrors `kokoro_onnx` (`config.py`, `tokenizer.py`, `chunker.py`). The vocabulary is the
//! package's own `config.json` `vocab` table, embedded verbatim (`vocab.json`).

use std::collections::HashMap;

/// The model's context: a batch never holds more phonemes than this.
pub const MAX_PHONEME_LENGTH: usize = 510;

const SENTENCE_MARKS: &str = ".!?…";
const CLAUSE_MARKS: &str = ",;:";

pub struct Vocab {
    ids: HashMap<char, i64>,
}

impl Vocab {
    pub fn embedded() -> Self {
        let raw: HashMap<String, i64> = serde_json::from_str(include_str!("vocab.json"))
            .expect("embedded narrator vocab is valid JSON");
        let ids = raw
            .into_iter()
            .filter_map(|(k, v)| {
                let mut chars = k.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Some((c, v)),
                    _ => None,
                }
            })
            .collect();
        Self { ids }
    }

    /// `Tokenizer.phonemize`'s tail: keep only phonemes the model knows, then `strip()`.
    pub fn filter_known(&self, phonemes: &str) -> String {
        let kept: String = phonemes
            .chars()
            .filter(|c| self.ids.contains_key(c))
            .collect();
        kept.trim().to_string()
    }

    /// `Tokenizer.tokenize`: phonemes → ids, dropping anything outside the vocabulary.
    pub fn tokenize(&self, phonemes: &str) -> Vec<i64> {
        phonemes
            .chars()
            .filter_map(|c| self.ids.get(&c).copied())
            .collect()
    }
}

/// Splits after every whitespace run that follows one of `marks` (`(?<=[marks])\s+`).
fn split_after_marks(s: &str, marks: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() && i > 0 && marks.contains(chars[i - 1]) {
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            pieces.push(std::mem::take(&mut current));
        } else {
            current.push(c);
            i += 1;
        }
    }
    pieces.push(current);
    pieces
}

fn split_on_whitespace_runs(s: &str) -> Vec<String> {
    // `re.split(r"\s+")` keeps empty edge pieces; the caller drops empties.
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut in_run = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_run {
                pieces.push(std::mem::take(&mut current));
                in_run = true;
            }
        } else {
            in_run = false;
            current.push(c);
        }
    }
    pieces.push(current);
    pieces
}

fn split_at_level(s: &str, level: usize) -> Vec<String> {
    match level {
        0 => split_after_marks(s, SENTENCE_MARKS),
        1 => split_after_marks(s, CLAUSE_MARKS),
        _ => split_on_whitespace_runs(s),
    }
}

const BOUNDARY_LEVELS: usize = 3;

fn atoms(phonemes: &str, max_length: usize, level: usize, out: &mut Vec<String>) {
    let len = phonemes.chars().count();
    if len <= max_length {
        if !phonemes.is_empty() {
            out.push(phonemes.to_string());
        }
        return;
    }
    for index in level..BOUNDARY_LEVELS {
        let pieces = split_at_level(phonemes, index);
        if pieces.len() > 1 {
            for piece in pieces {
                atoms(piece.trim(), max_length, index + 1, out);
            }
            return;
        }
    }
    // One unbroken run longer than the context: slice it rather than drop the tail.
    let chars: Vec<char> = phonemes.chars().collect();
    for part in chars.chunks(max_length) {
        out.push(part.iter().collect());
    }
}

fn pack(lengths: &[usize], limit: usize) -> Vec<(usize, usize)> {
    let mut batches = Vec::new();
    let (mut start, mut size) = (0usize, 0usize);
    for (index, &length) in lengths.iter().enumerate() {
        let candidate = if index == start {
            length
        } else {
            size + 1 + length
        };
        if candidate > limit && index > start {
            batches.push((start, index));
            start = index;
            size = length;
        } else {
            size = candidate;
        }
    }
    if !lengths.is_empty() {
        batches.push((start, lengths.len()));
    }
    batches
}

/// `split_phonemes`: balanced batches of at most `max_length` phonemes.
pub fn split_phonemes(phonemes: &str, max_length: usize) -> Vec<String> {
    let mut parts = Vec::new();
    atoms(phonemes.trim(), max_length, 0, &mut parts);
    if parts.is_empty() {
        return Vec::new();
    }
    let lengths: Vec<usize> = parts.iter().map(|a| a.chars().count()).collect();
    let fewest = pack(&lengths, max_length).len();
    let mut low = lengths.iter().copied().max().unwrap_or(0);
    let mut high = max_length;
    while low < high {
        let middle = (low + high) / 2;
        if pack(&lengths, middle).len() <= fewest {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    pack(&lengths, low)
        .into_iter()
        .map(|(a, b)| parts[a..b].join(" "))
        .collect()
}

/// Seconds of silence a batch ending with this text is followed by.
pub fn pause_after(phonemes: &str, sentence: f32, clause: f32) -> f32 {
    match phonemes.trim_end().chars().last() {
        Some(c) if SENTENCE_MARKS.contains(c) => sentence,
        Some(c) if CLAUSE_MARKS.contains(c) => clause,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_batch() {
        assert_eq!(split_phonemes("abc def.", 510), vec!["abc def."]);
    }

    #[test]
    fn long_text_splits_at_sentences_and_stays_within_the_limit() {
        let sentence = "a".repeat(30) + ".";
        let text = vec![sentence; 40].join(" ");
        let batches = split_phonemes(&text, 100);
        assert!(batches.len() > 1);
        assert!(batches.iter().all(|b| b.chars().count() <= 100));
        assert!(batches.iter().all(|b| b.ends_with('.')));
    }
}
