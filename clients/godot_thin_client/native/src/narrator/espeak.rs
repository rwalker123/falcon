//! espeak-ng, loaded at RUNTIME, and the phonemizer pipeline built on it.
//!
//! espeak-ng is GPL-3, so it is never linked or bundled: `libloading` opens the shared library
//! if it is on the machine, and narration is simply off if it is not. Nothing in the build
//! depends on it.
//!
//! The pipeline mirrors `phonemizer.phonemize(text, lang, preserve_punctuation=True,
//! with_stress=True)` as `kokoro_onnx.Tokenizer.phonemize` calls it: punctuation marks are cut
//! out of the text, each punctuation-free chunk goes through `espeak_TextToPhonemes` (IPA, `_`
//! as the phone separator), and the marks are put back where they were.

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;
use std::sync::Mutex;

use libloading::Library;

/// `espeak_Initialize` output mode: synchronous, no audio device is ever opened.
const AUDIO_OUTPUT_SYNCHRONOUS: c_int = 0x02;
/// `espeak_TextToPhonemes` text mode: the input is UTF-8.
const TEXT_MODE_UTF8: c_int = 1;
/// `espeak_TextToPhonemes` phoneme mode bit 1: IPA output.
const PHONEME_MODE_IPA: c_int = 0x02;
/// The byte that separates phones in the IPA output (carried in bits 8.. of the mode).
const PHONE_SEPARATOR: u8 = b'_';
/// Shift that places the separator character in the phoneme-mode word.
const PHONE_SEPARATOR_SHIFT: u32 = 8;

/// The marks phonemizer's `Punctuation.default_marks()` preserves.
const PUNCTUATION_MARKS: &str = ";:,.!?¡¿—…\"«»“”(){}[]";
/// Marks that double as decimal separators: they stay inside a number (`1,5`).
const DECIMAL_SEPARATORS: &str = ",.";

/// Serialises every call into the library: espeak keeps process-global state.
static ESPEAK_LOCK: Mutex<()> = Mutex::new(());

type InitializeFn = unsafe extern "C" fn(c_int, c_int, *const c_char, c_int) -> c_int;
type SetVoiceByNameFn = unsafe extern "C" fn(*const c_char) -> c_int;
type ListVoicesFn = unsafe extern "C" fn(*const c_void) -> *const *const VoiceRecord;
type TextToPhonemesFn = unsafe extern "C" fn(*mut *const c_void, c_int, c_int) -> *const c_char;

/// The leading fields of `espeak_VOICE` — all this module reads.
#[repr(C)]
struct VoiceRecord {
    name: *const c_char,
    /// A priority byte, then the language code, NUL-terminated.
    languages: *const c_char,
    identifier: *const c_char,
}

pub struct Espeak {
    // Keeps the library mapped for as long as the fn pointers below are alive.
    _lib: Library,
    set_voice_by_name: SetVoiceByNameFn,
    list_voices: ListVoicesFn,
    text_to_phonemes: TextToPhonemesFn,
}

impl Espeak {
    /// Opens the shared library and initialises it against `data_path`.
    pub fn load(lib_path: &Path, data_path: &Path) -> Result<Self, String> {
        let data = CString::new(data_path.to_string_lossy().as_bytes())
            .map_err(|_| "espeak data path contains a NUL".to_string())?;
        // SAFETY: loading a shared library runs its initialisers; it is the library the player
        // pointed us at (or a well-known system path), and the symbols are typed to its header.
        unsafe {
            let lib = Library::new(lib_path).map_err(|e| format!("cannot load espeak-ng: {e}"))?;
            let initialize: InitializeFn = *lib
                .get::<InitializeFn>(b"espeak_Initialize\0")
                .map_err(|e| e.to_string())?;
            let set_voice_by_name: SetVoiceByNameFn = *lib
                .get::<SetVoiceByNameFn>(b"espeak_SetVoiceByName\0")
                .map_err(|e| e.to_string())?;
            let list_voices: ListVoicesFn = *lib
                .get::<ListVoicesFn>(b"espeak_ListVoices\0")
                .map_err(|e| e.to_string())?;
            let text_to_phonemes: TextToPhonemesFn = *lib
                .get::<TextToPhonemesFn>(b"espeak_TextToPhonemes\0")
                .map_err(|e| e.to_string())?;
            let _guard = ESPEAK_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            if initialize(AUDIO_OUTPUT_SYNCHRONOUS, 0, data.as_ptr(), 0) <= 0 {
                return Err("espeak_Initialize failed (bad data directory?)".to_string());
            }
            Ok(Self {
                _lib: lib,
                set_voice_by_name,
                list_voices,
                text_to_phonemes,
            })
        }
    }

    /// `phonemizer.phonemize(text, lang, preserve_punctuation=True, with_stress=True)`, before
    /// the vocabulary filter. Multi-line input is phonemized line by line and concatenated, the
    /// `\n` joins phonemizer makes being dropped by that filter anyway.
    pub fn phonemize(&self, text: &str, lang: &str) -> Result<String, String> {
        let _guard = ESPEAK_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let identifier = self.voice_identifier(lang)?;
        let voice = CString::new(identifier).map_err(|_| "bad voice".to_string())?;
        // SAFETY: `voice` is a valid NUL-terminated string for the call's duration.
        if unsafe { (self.set_voice_by_name)(voice.as_ptr()) } != 0 {
            return Err(format!("espeak has no voice for '{lang}'"));
        }
        let mut out = String::new();
        for line in text.split('\n') {
            let line = line.trim_matches(['\n', '\r']);
            if line.trim().is_empty() {
                continue;
            }
            out.push_str(&self.phonemize_line(line)?);
        }
        Ok(out)
    }

    /// The first non-mbrola voice listed for a language code, as phonemizer's `set_voice`
    /// picks it (espeak sorts voices by relevancy).
    fn voice_identifier(&self, lang: &str) -> Result<String, String> {
        // SAFETY: espeak returns a NULL-terminated array of records it owns for the process's
        // lifetime; every string read here is NUL-terminated.
        unsafe {
            let mut cursor = (self.list_voices)(std::ptr::null());
            if cursor.is_null() {
                return Err("espeak lists no voices".to_string());
            }
            while !(*cursor).is_null() {
                let record = &**cursor;
                if !record.languages.is_null() && !record.identifier.is_null() {
                    let languages = std::ffi::CStr::from_ptr(record.languages).to_bytes();
                    let code = String::from_utf8_lossy(languages.get(1..).unwrap_or_default());
                    let identifier = std::ffi::CStr::from_ptr(record.identifier)
                        .to_string_lossy()
                        .into_owned();
                    if code == lang && !identifier.replace('\\', "/").starts_with("mb/") {
                        return Ok(identifier);
                    }
                }
                cursor = cursor.add(1);
            }
        }
        Err(format!("espeak has no voice for '{lang}'"))
    }

    fn phonemize_line(&self, line: &str) -> Result<String, String> {
        let (chunks, marks) = preserve_punctuation(line);
        let mut phonemized = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            let raw = self.text_to_phonemes_chunk(chunk)?;
            phonemized.push(postprocess_line(&raw));
        }
        Ok(restore_punctuation(phonemized, marks).concat())
    }

    /// `EspeakWrapper.text_to_phonemes`: call until espeak consumes the whole input, joining
    /// the pieces with a space.
    fn text_to_phonemes_chunk(&self, chunk: &str) -> Result<String, String> {
        let owned = CString::new(chunk).map_err(|_| "text contains a NUL".to_string())?;
        let mut cursor: *const c_void = owned.as_ptr().cast();
        let mode = (PHONE_SEPARATOR as c_int) << PHONE_SEPARATOR_SHIFT | PHONEME_MODE_IPA;
        let mut pieces: Vec<String> = Vec::new();
        while !cursor.is_null() {
            // SAFETY: `cursor` points into `owned`, which outlives the loop; espeak advances it
            // and sets it to NULL at the end of the input.
            let ptr = unsafe { (self.text_to_phonemes)(&mut cursor, TEXT_MODE_UTF8, mode) };
            if !ptr.is_null() {
                // SAFETY: espeak returns a NUL-terminated string valid until the next call.
                let s = unsafe { std::ffi::CStr::from_ptr(ptr) }
                    .to_string_lossy()
                    .into_owned();
                if !s.is_empty() {
                    pieces.push(s);
                }
            }
        }
        Ok(pieces.join(" "))
    }
}

/// `EspeakBackend._postprocess_line` with `strip=False`, stress kept, no tie: every word ends
/// up followed by exactly one space, and the `_` phone separators are removed.
fn postprocess_line(line: &str) -> String {
    let line = line.trim().replace('\n', " ").replace("  ", " ");
    // `re.sub(r'_+', '_')` then `re.sub(r'_ ', ' ')`.
    let mut collapsed = String::with_capacity(line.len());
    let mut prev_underscore = false;
    for c in line.chars() {
        if c == '_' {
            if !prev_underscore {
                collapsed.push('_');
            }
            prev_underscore = true;
        } else {
            collapsed.push(c);
            prev_underscore = false;
        }
    }
    let line = collapsed.replace("_ ", " ");
    let mut out = String::new();
    for word in line.split(' ') {
        // `word.strip()`, then `+ '_'` and `.replace('_', '')` cancel out.
        out.push_str(word.trim().replace('_', "").as_str());
        out.push(' ');
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
enum Position {
    Begin,
    Inside,
    End,
    Alone,
}

#[derive(Debug, Clone)]
struct Mark {
    text: String,
    position: Position,
}

fn is_mark_char(chars: &[char], i: usize) -> bool {
    let c = chars[i];
    if PUNCTUATION_MARKS.contains(c) && !DECIMAL_SEPARATORS.contains(c) {
        return true;
    }
    if DECIMAL_SEPARATORS.contains(c) {
        // A decimal separator is a mark unless it sits between two digits.
        let digit_before = i > 0 && chars[i - 1].is_ascii_digit();
        let digit_after = i + 1 < chars.len() && chars[i + 1].is_ascii_digit();
        return !(digit_before && digit_after);
    }
    false
}

/// `Punctuation.preserve` for one line: the text chunks (empties dropped) and the marks.
fn preserve_punctuation(line: &str) -> (Vec<String>, Vec<Mark>) {
    let chars: Vec<char> = line.chars().collect();
    // Each match is the maximal run of whitespace and marks that contains a mark.
    let mut matches: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() || is_mark_char(&chars, i) {
            let start = i;
            let mut has_mark = false;
            while i < chars.len() && (chars[i].is_whitespace() || is_mark_char(&chars, i)) {
                if !chars[i].is_whitespace() {
                    has_mark = true;
                }
                i += 1;
            }
            if has_mark {
                matches.push((start, i));
            }
        } else {
            i += 1;
        }
    }
    if matches.is_empty() {
        return (vec![line.to_string()], Vec::new());
    }
    let slice = |(a, b): (usize, usize)| chars[a..b].iter().collect::<String>();
    if matches.len() == 1 && matches[0] == (0, chars.len()) {
        return (
            Vec::new(),
            vec![Mark {
                text: line.to_string(),
                position: Position::Alone,
            }],
        );
    }
    let mut marks = Vec::new();
    let last = matches.len() - 1;
    for (n, &m) in matches.iter().enumerate() {
        let position = if n == 0 && m.0 == 0 {
            Position::Begin
        } else if n == last && m.1 == chars.len() {
            Position::End
        } else {
            Position::Inside
        };
        marks.push(Mark {
            text: slice(m),
            position,
        });
    }
    // Split the line at the first remaining occurrence of each mark's text.
    let mut chunks = Vec::new();
    let mut rest = line.to_string();
    for mark in &marks {
        match rest.find(&mark.text) {
            Some(at) => {
                chunks.push(rest[..at].to_string());
                rest = rest[at + mark.text.len()..].to_string();
            }
            None => chunks.push(String::new()),
        }
    }
    chunks.push(rest);
    (
        chunks.into_iter().filter(|c| !c.is_empty()).collect(),
        marks,
    )
}

/// `Punctuation.restore` for a single line with `strip=False` and `sep.word == " "`.
fn restore_punctuation(text: Vec<String>, marks: Vec<Mark>) -> Vec<String> {
    let mut text: VecDeque<String> = text.into();
    let mut marks: VecDeque<Mark> = marks.into();
    let mut out: Vec<String> = Vec::new();
    // Marks all carry line index 0 here, so `pos` only matters once an End mark advances it.
    let mut pos = 0usize;
    while !text.is_empty() || !marks.is_empty() {
        if marks.is_empty() {
            for mut line in text.drain(..) {
                if !line.ends_with(' ') {
                    line.push(' ');
                }
                out.push(line);
            }
        } else if text.is_empty() {
            out.push(marks.drain(..).map(|m| m.text).collect::<String>());
        } else if pos == 0 {
            let mark = marks.pop_front().expect("checked non-empty");
            if let Some(head) = text.front_mut() {
                if head.ends_with(' ') {
                    head.pop();
                }
            }
            match mark.position {
                Position::Begin => {
                    if let Some(head) = text.front_mut() {
                        *head = format!("{}{}", mark.text, head);
                    }
                }
                Position::End => {
                    let head = text.pop_front().expect("checked non-empty");
                    let tail = if mark.text.ends_with(' ') { "" } else { " " };
                    out.push(format!("{head}{}{tail}", mark.text));
                    pos += 1;
                }
                Position::Alone => {
                    let tail = if mark.text.ends_with(' ') { "" } else { " " };
                    out.push(format!("{}{tail}", mark.text));
                    pos += 1;
                }
                Position::Inside => {
                    if text.len() == 1 {
                        if let Some(head) = text.front_mut() {
                            head.push_str(&mark.text);
                        }
                    } else {
                        let first = text.pop_front().expect("checked non-empty");
                        if let Some(head) = text.front_mut() {
                            *head = format!("{first}{}{head}", mark.text);
                        }
                    }
                }
            }
        } else {
            out.push(text.pop_front().expect("checked non-empty"));
            pos += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuation_round_trips_around_chunks() {
        let (chunks, marks) = preserve_punctuation("Hello, brave world.");
        assert_eq!(chunks, vec!["Hello", "brave world"]);
        assert_eq!(marks.len(), 2);
        assert_eq!(marks[0].position, Position::Inside);
        assert_eq!(marks[1].position, Position::End);
        let restored = restore_punctuation(vec!["a ".into(), "b c ".into()], marks);
        assert_eq!(restored.concat(), "a, b c. ");
    }

    #[test]
    fn a_decimal_separator_between_digits_is_not_a_mark() {
        let (chunks, marks) = preserve_punctuation("1.5 days");
        assert_eq!(chunks, vec!["1.5 days"]);
        assert!(marks.is_empty());
    }

    #[test]
    fn a_line_of_only_punctuation_comes_back_alone() {
        let (chunks, marks) = preserve_punctuation("...");
        assert!(chunks.is_empty());
        assert_eq!(restore_punctuation(Vec::new(), marks).concat(), "...");
    }
}
