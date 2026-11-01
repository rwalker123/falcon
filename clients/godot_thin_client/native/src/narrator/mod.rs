//! The spoken narrator's native half: Kokoro-82M over ONNX Runtime, fed phonemes by a
//! runtime-loaded espeak-ng. Presentation only — nothing here touches the simulation.
//!
//! * [`engine`] — `NarratorEngine`, the `GodotClass` GDScript drives.
//! * [`Narrator`] — the engine-free core: ONE background worker, a request queue, a finished
//!   list the main thread polls, and a bounded cache of synthesized lines.
//! * [`kokoro`] / [`espeak`] / [`tokenizer`] — inference, phonemizer, vocabulary + chunker.

pub(crate) mod engine;
pub(crate) mod espeak;
pub(crate) mod kokoro;
pub(crate) mod tokenizer;

pub use engine::NarratorEngine;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use kokoro::Kokoro;

/// Most distinct (voice, speed, text) lines kept; the oldest is evicted past this. A page is a
/// handful of lines at ~100 KB each, so this holds several pages without growing unbounded.
pub const CACHE_ENTRY_CAP: usize = 64;

#[derive(Clone, Debug)]
pub struct Paths {
    pub model: PathBuf,
    pub voices: PathBuf,
    pub espeak_lib: PathBuf,
    pub espeak_data: PathBuf,
}

impl Paths {
    /// Every file narration needs exists. Only paths are checked — the model is not opened.
    pub fn all_exist(&self) -> bool {
        self.model.is_file()
            && self.voices.is_file()
            && self.espeak_lib.is_file()
            && self.espeak_data.is_dir()
    }
}

pub struct Completed {
    pub id: i64,
    pub result: Result<Arc<Vec<u8>>, String>,
}

type CacheKey = (String, u32, String);

struct Job {
    id: i64,
    generation: u64,
    voice: String,
    speed: f32,
    text: String,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<VecDeque<Job>>,
    wake: Condvar,
    finished: Mutex<Vec<(u64, Completed)>>,
    cache: Mutex<Cache>,
    generation: AtomicU64,
    /// Set by `warm_up`: the worker loads the model on its next wake even with nothing queued.
    warm: AtomicBool,
    /// The model is loaded and the worker can synthesize at once.
    ready: AtomicBool,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<CacheKey, Arc<Vec<u8>>>,
    order: VecDeque<CacheKey>,
}

impl Cache {
    fn get(&self, key: &CacheKey) -> Option<Arc<Vec<u8>>> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: CacheKey, pcm: Arc<Vec<u8>>) {
        if self.entries.insert(key.clone(), pcm).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > CACHE_ENTRY_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

pub struct Narrator {
    paths: Paths,
    shared: Arc<Shared>,
    next_id: i64,
    worker_started: bool,
}

impl Narrator {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            shared: Arc::new(Shared::default()),
            next_id: 1,
            worker_started: false,
        }
    }

    /// Queues a line. The worker (and the model) start on the first call.
    pub fn request(&mut self, text: String, voice: String, speed: f32) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let generation = self.shared.generation.load(Ordering::SeqCst);
        let key = (voice.clone(), speed.to_bits(), text.clone());
        let cached = self
            .shared
            .cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&key));
        if let Some(pcm) = cached {
            if let Ok(mut finished) = self.shared.finished.lock() {
                finished.push((
                    generation,
                    Completed {
                        id,
                        result: Ok(pcm),
                    },
                ));
            }
            return id;
        }
        self.ensure_worker();
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.push_back(Job {
                id,
                generation,
                voice,
                speed,
                text,
            });
        }
        self.shared.wake.notify_one();
        id
    }

    /// Starts the worker and has it load the model now, so the first real line does not pay for
    /// the load. Idempotent; never blocks the caller.
    pub fn warm_up(&mut self) {
        self.ensure_worker();
        self.shared.warm.store(true, Ordering::SeqCst);
        // Take the queue lock so the notify cannot slip between the worker's check and its wait.
        drop(self.shared.queue.lock());
        self.shared.wake.notify_one();
    }

    /// True once the model is loaded.
    pub fn is_ready(&self) -> bool {
        self.shared.ready.load(Ordering::SeqCst)
    }

    /// Completed requests since the last call, in finishing order. Results of cancelled
    /// requests never appear.
    pub fn poll(&mut self) -> Vec<Completed> {
        let current = self.shared.generation.load(Ordering::SeqCst);
        match self.shared.finished.lock() {
            Ok(mut finished) => finished
                .drain(..)
                .filter(|(generation, _)| *generation == current)
                .map(|(_, done)| done)
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Drops queued requests; a running one finishes but its result is discarded.
    pub fn cancel_all(&mut self) {
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.clear();
        }
        if let Ok(mut finished) = self.shared.finished.lock() {
            finished.clear();
        }
    }

    fn ensure_worker(&mut self) {
        if self.worker_started {
            return;
        }
        self.worker_started = true;
        let shared = Arc::clone(&self.shared);
        let paths = self.paths.clone();
        let spawned = thread::Builder::new()
            .name("narrator-worker".to_string())
            .spawn(move || worker_loop(shared, paths));
        if spawned.is_err() {
            self.worker_started = false;
        }
    }
}

fn load_model(paths: &Paths) -> Result<Kokoro, String> {
    Kokoro::load(
        &paths.model,
        &paths.voices,
        &paths.espeak_lib,
        &paths.espeak_data,
    )
}

fn worker_loop(shared: Arc<Shared>, paths: Paths) {
    // Loaded lazily on the first job, so a game that never narrates never pays for it.
    let mut model: Option<Result<Kokoro, String>> = None;
    loop {
        let job = {
            let Ok(mut queue) = shared.queue.lock() else {
                return;
            };
            loop {
                if shared.warm.swap(false, Ordering::SeqCst) {
                    break None;
                }
                if let Some(job) = queue.pop_front() {
                    break Some(job);
                }
                queue = match shared.wake.wait(queue) {
                    Ok(q) => q,
                    Err(_) => return,
                };
            }
        };
        let Some(job) = job else {
            let loaded = model.get_or_insert_with(|| load_model(&paths));
            shared.ready.store(loaded.is_ok(), Ordering::SeqCst);
            continue;
        };
        if job.generation != shared.generation.load(Ordering::SeqCst) {
            continue;
        }
        let loaded = model.get_or_insert_with(|| load_model(&paths));
        shared.ready.store(loaded.is_ok(), Ordering::SeqCst);
        let result = match loaded {
            Ok(kokoro) => kokoro
                .synthesize(&job.text, &job.voice, job.speed)
                .map(Arc::new),
            Err(e) => Err(e.clone()),
        };
        if let Ok(pcm) = &result {
            if let Ok(mut cache) = shared.cache.lock() {
                cache.insert(
                    (job.voice.clone(), job.speed.to_bits(), job.text.clone()),
                    Arc::clone(pcm),
                );
            }
        }
        if let Ok(mut finished) = shared.finished.lock() {
            finished.push((job.generation, Completed { id: job.id, result }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tokenizer::Vocab;

    const DEFAULT_ESPEAK_LIBS: [&str; 4] = [
        "/opt/homebrew/lib/libespeak-ng.dylib",
        "/usr/local/lib/libespeak-ng.dylib",
        "/usr/lib/x86_64-linux-gnu/libespeak-ng.so.1",
        "/usr/lib/libespeak-ng.so.1",
    ];
    const FIXTURE: &str = include_str!("../../tests/fixtures/phoneme_fixture.json");
    /// The first telling is a sentence of ordinary length: plausible audio is a few seconds.
    const PLAUSIBLE_SECONDS: std::ops::RangeInclusive<f32> = 2.0..=30.0;

    fn espeak_paths() -> Option<(PathBuf, PathBuf)> {
        let lib = std::env::var("NARRATOR_ESPEAK_LIB")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                DEFAULT_ESPEAK_LIBS
                    .iter()
                    .map(PathBuf::from)
                    .find(|p| p.is_file())
            })?;
        let data = std::env::var("NARRATOR_ESPEAK_DATA")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                lib.parent()?
                    .parent()
                    .map(|prefix| prefix.join("share/espeak-ng-data"))
            })?;
        (lib.is_file() && data.is_dir()).then_some((lib, data))
    }

    #[test]
    fn phonemes_and_tokens_match_the_python_reference() {
        let Some((lib, data)) = espeak_paths() else {
            println!("SKIP parity: espeak-ng not found (set NARRATOR_ESPEAK_LIB / _DATA)");
            return;
        };
        let espeak = espeak::Espeak::load(&lib, &data).expect("espeak loads");
        let vocab = Vocab::embedded();
        let rows: Vec<serde_json::Value> = serde_json::from_str(FIXTURE).expect("fixture parses");
        assert!(!rows.is_empty());
        let mut failures = Vec::new();
        for row in &rows {
            let text = row["text"].as_str().unwrap();
            let lang = row["lang"].as_str().unwrap();
            let want = row["phonemes"].as_str().unwrap();
            let want_tokens: Vec<i64> = row["tokens"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_i64().unwrap())
                .collect();
            let got = vocab.filter_known(&espeak.phonemize(text.trim(), lang).unwrap());
            if got != want || vocab.tokenize(&got) != want_tokens {
                failures.push(format!("[{lang}] {text}\n  want {want}\n  got  {got}"));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} rows differ:\n{}",
            failures.len(),
            rows.len(),
            failures.join("\n")
        );
        println!("parity: {} rows match", rows.len());
    }

    #[test]
    fn one_line_synthesizes_to_audible_audio() {
        let Some(dir) = std::env::var("NARRATOR_MODEL_DIR").ok().map(PathBuf::from) else {
            println!("SKIP synth: NARRATOR_MODEL_DIR is not set");
            return;
        };
        let Some((lib, data)) = espeak_paths() else {
            println!("SKIP synth: espeak-ng not found");
            return;
        };
        let model = dir.join("kokoro-v1.0.int8.onnx");
        let voices = dir.join("voices-v1.0.bin");
        if !Path::new(&model).is_file() || !voices.is_file() {
            println!("SKIP synth: model files not in {}", dir.display());
            return;
        }
        let rows: Vec<serde_json::Value> = serde_json::from_str(FIXTURE).unwrap();
        let text = rows[0]["text"].as_str().unwrap();
        let started = std::time::Instant::now();
        let mut kokoro = Kokoro::load(&model, &voices, &lib, &data).expect("model loads");
        let loaded = started.elapsed();
        let synth_started = std::time::Instant::now();
        let pcm = kokoro
            .synthesize(text, "bm_george", 0.85)
            .expect("synthesizes");
        println!(
            "synth: load {:?}, one line {:?}",
            loaded,
            synth_started.elapsed()
        );
        let samples: Vec<i16> = pcm
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect();
        let seconds = samples.len() as f32 / kokoro::SAMPLE_RATE as f32;
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        assert!(PLAUSIBLE_SECONDS.contains(&seconds), "{seconds}s of audio");
        assert!(peak > 1000, "audio is silent (peak {peak})");
    }

    #[test]
    fn the_cache_evicts_oldest_past_its_cap() {
        let mut cache = Cache::default();
        for n in 0..(CACHE_ENTRY_CAP + 5) {
            cache.insert(("v".into(), 0, n.to_string()), Arc::new(vec![0u8]));
        }
        assert_eq!(cache.entries.len(), CACHE_ENTRY_CAP);
        assert!(cache.get(&("v".into(), 0, "0".into())).is_none());
    }
}
