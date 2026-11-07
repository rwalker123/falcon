//! Kokoro-82M inference through ONNX Runtime — mirrors `kokoro_onnx.Kokoro.create`.

use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;

use ndarray::Array3;
use ndarray_npy::NpzReader;
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;

use super::espeak::Espeak;
use super::tokenizer::{pause_after, split_phonemes, Vocab, MAX_PHONEME_LENGTH};

pub const SAMPLE_RATE: u32 = 24_000;
/// Silence after a batch that ends a sentence / a clause (kokoro_onnx defaults).
const SENTENCE_PAUSE_SECONDS: f32 = 0.25;
const CLAUSE_PAUSE_SECONDS: f32 = 0.1;
/// The legal `speed` range the reference rejects outside of.
const SPEED_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.0;
/// `librosa.effects.trim` parameters as `kokoro_onnx.trim` uses them.
const TRIM_TOP_DB: f32 = 60.0;
const TRIM_FRAME_LENGTH: usize = 2048;
const TRIM_HOP_LENGTH: usize = 512;
/// Floor of the amplitude → dB conversion.
const TRIM_AMIN: f32 = 1e-5;
/// Scale of a 16-bit signed sample.
const PCM16_SCALE: f32 = 32_767.0;

pub struct Kokoro {
    session: Session,
    voices: NpzReader<File>,
    vocab: Vocab,
    espeak: Espeak,
}

/// Language code the voice prefix selects: `a*` American, `b*` British.
pub fn lang_for_voice(voice: &str) -> &'static str {
    if voice.starts_with('b') {
        "en-gb"
    } else {
        "en-us"
    }
}

impl Kokoro {
    pub fn load(
        model: &Path,
        voices: &Path,
        espeak_lib: &Path,
        espeak_data: &Path,
    ) -> Result<Self, String> {
        let espeak = Espeak::load(espeak_lib, espeak_data)?;
        let env = environment()?;
        let session = Session::builder(env)
            .and_then(|mut b| b.commit_from_file(model))
            .map_err(|e| format!("cannot load the narrator model: {e}"))?;
        let file = File::open(voices).map_err(|e| format!("cannot open voices: {e}"))?;
        let voices = NpzReader::new(file).map_err(|e| format!("cannot read voices: {e}"))?;
        Ok(Self {
            session,
            voices,
            vocab: Vocab::embedded(),
            espeak,
        })
    }

    /// Text → phonemes the model knows (the fixture-parity step).
    pub fn phonemize(&self, text: &str, lang: &str) -> Result<String, String> {
        let raw = self.espeak.phonemize(text.trim(), lang)?;
        Ok(self.vocab.filter_known(&raw))
    }

    /// One line → 16-bit little-endian mono PCM at [`SAMPLE_RATE`].
    pub fn synthesize(&mut self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, String> {
        if !SPEED_RANGE.contains(&speed) {
            return Err(format!("speed {speed} is outside 0.5..2.0"));
        }
        let style_bank: Array3<f32> = self
            .voices
            .by_name(voice)
            .map_err(|e| format!("voice '{voice}' not found: {e}"))?;
        let phonemes = self.phonemize(text, lang_for_voice(voice))?;
        let phonemes = phonemes.split_whitespace().collect::<Vec<_>>().join(" ");
        let batches = split_phonemes(&phonemes, MAX_PHONEME_LENGTH);
        if batches.is_empty() {
            return Err(format!("nothing to synthesize in {text:?}"));
        }
        let mut audio: Vec<f32> = Vec::new();
        let last = batches.len() - 1;
        for (i, batch) in batches.iter().enumerate() {
            let tokens = self.vocab.tokenize(batch);
            if tokens.is_empty() {
                continue;
            }
            // One style vector per token count: n tokens use row n - 1.
            let rows = style_bank.shape()[0];
            let row = tokens.len().min(rows) - 1;
            let style: Vec<f32> = style_bank
                .slice(ndarray::s![row, .., ..])
                .iter()
                .copied()
                .collect();
            let mut samples = self.infer(&tokens, style, speed)?;
            samples = trim_silence(&samples);
            audio.extend_from_slice(&samples);
            if i != last {
                let pause = pause_after(batch, SENTENCE_PAUSE_SECONDS, CLAUSE_PAUSE_SECONDS);
                audio.extend(std::iter::repeat_n(
                    0.0f32,
                    (pause * SAMPLE_RATE as f32) as usize,
                ));
            }
        }
        if audio.is_empty() {
            return Err("the model produced no audio".to_string());
        }
        Ok(to_pcm16(&audio))
    }

    fn infer(&mut self, tokens: &[i64], style: Vec<f32>, speed: f32) -> Result<Vec<f32>, String> {
        // The model pads the token row with a 0 on each side.
        let mut padded = Vec::with_capacity(tokens.len() + 2);
        padded.push(0i64);
        padded.extend_from_slice(tokens);
        padded.push(0);
        let style_len = style.len();
        let tokens_t = Tensor::from_array(([1usize, padded.len()], padded)).map_err(err)?;
        let style_t = Tensor::from_array(([1usize, style_len], style)).map_err(err)?;
        let speed_t = Tensor::from_array(([1usize], vec![speed])).map_err(err)?;
        let outputs = self
            .session
            .run(ort::inputs!["tokens" => tokens_t, "style" => style_t, "speed" => speed_t])
            .map_err(err)?;
        let (_, data) = outputs[0].try_extract_tensor::<f32>().map_err(err)?;
        Ok(data.to_vec())
    }
}

/// ONNX Runtime allows ONE environment per process; it is built on first use and kept.
fn environment() -> Result<&'static Environment, String> {
    static ENV: OnceLock<Environment> = OnceLock::new();
    if let Some(env) = ENV.get() {
        return Ok(env);
    }
    let env = ort::init()
        .with_name("narrator")
        .with_telemetry(false)
        .build()
        .map_err(err)?;
    Ok(ENV.get_or_init(|| env))
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// `librosa.effects.trim`: drop leading / trailing frames more than 60 dB under the loudest.
fn trim_silence(y: &[f32]) -> Vec<f32> {
    let pad = TRIM_FRAME_LENGTH / 2;
    let mut padded = vec![0.0f32; pad];
    padded.extend_from_slice(y);
    padded.extend(std::iter::repeat_n(0.0f32, pad));
    let frames = (padded.len() - TRIM_FRAME_LENGTH) / TRIM_HOP_LENGTH + 1;
    let rms: Vec<f32> = (0..frames)
        .map(|f| {
            let start = f * TRIM_HOP_LENGTH;
            let sum: f32 = padded[start..start + TRIM_FRAME_LENGTH]
                .iter()
                .map(|s| s * s)
                .sum();
            (sum / TRIM_FRAME_LENGTH as f32).sqrt()
        })
        .collect();
    let reference = rms.iter().copied().fold(0.0f32, f32::max).max(TRIM_AMIN);
    let to_db = |m: f32| 20.0 * m.max(TRIM_AMIN).log10();
    let floor = to_db(reference) - TRIM_TOP_DB;
    let loud: Vec<usize> = rms
        .iter()
        .enumerate()
        .filter(|(_, &m)| to_db(m) > floor)
        .map(|(i, _)| i)
        .collect();
    match (loud.first(), loud.last()) {
        (Some(&first), Some(&last)) => {
            let start = first * TRIM_HOP_LENGTH;
            let end = ((last + 1) * TRIM_HOP_LENGTH).min(y.len());
            if start < end {
                y[start..end].to_vec()
            } else {
                y.to_vec()
            }
        }
        _ => y.to_vec(),
    }
}

fn to_pcm16(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * PCM16_SCALE) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}
