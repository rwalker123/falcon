//! `NarratorEngine` — the GDScript-facing wrapper over [`super::Narrator`].

use std::path::PathBuf;

use godot::prelude::*;

use super::kokoro::SAMPLE_RATE;
use super::{Narrator, Paths};

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
pub struct NarratorEngine {
    narrator: Option<Narrator>,
}

#[godot_api]
impl NarratorEngine {
    /// Validates that every file exists. Does NOT load the model — that waits for the first
    /// `request`, on the worker. False means narration is off.
    #[func]
    pub fn configure(
        &mut self,
        model_path: GString,
        voices_path: GString,
        espeak_lib_path: GString,
        espeak_data_path: GString,
    ) -> bool {
        let paths = Paths {
            model: PathBuf::from(model_path.to_string()),
            voices: PathBuf::from(voices_path.to_string()),
            espeak_lib: PathBuf::from(espeak_lib_path.to_string()),
            espeak_data: PathBuf::from(espeak_data_path.to_string()),
        };
        if !paths.all_exist() {
            self.narrator = None;
            return false;
        }
        self.narrator = Some(Narrator::new(paths));
        true
    }

    /// Loads the model on the worker now, so the first line is not delayed by it. Returns at once.
    #[func]
    pub fn warm_up(&mut self) {
        if let Some(n) = self.narrator.as_mut() {
            n.warm_up();
        }
    }

    /// True once the model is loaded.
    #[func]
    pub fn is_ready(&self) -> bool {
        self.narrator.as_ref().is_some_and(Narrator::is_ready)
    }

    /// True once the worker tried to load the model and failed. Final: the load is never retried.
    #[func]
    pub fn is_failed(&self) -> bool {
        self.narrator
            .as_ref()
            .is_some_and(|n| n.load_error().is_some())
    }

    /// Why the model failed to load; empty while it has not failed.
    #[func]
    pub fn load_error(&self) -> GString {
        self.narrator
            .as_ref()
            .and_then(Narrator::load_error)
            .map(|e| GString::from(e.as_str()))
            .unwrap_or_default()
    }

    /// Queues synthesis on the background worker and returns the request id (-1 when
    /// narration is not configured).
    #[func]
    pub fn request(&mut self, text: GString, voice: GString, speed: f32) -> i64 {
        match self.narrator.as_mut() {
            Some(n) => n.request(text.to_string(), voice.to_string(), speed),
            None => -1,
        }
    }

    /// Finished requests: `{id, pcm16, sample_rate}` or `{id, error}`.
    #[func]
    pub fn poll(&mut self) -> VarArray {
        let mut out = VarArray::new();
        let Some(n) = self.narrator.as_mut() else {
            return out;
        };
        for done in n.poll() {
            let mut d = VarDictionary::new();
            d.set("id", done.id);
            match done.result {
                Ok(pcm) => {
                    d.set("pcm16", &PackedByteArray::from(pcm.as_slice()));
                    d.set("sample_rate", i64::from(SAMPLE_RATE));
                }
                Err(e) => d.set("error", &GString::from(e.as_str())),
            }
            out.push(&d.to_variant());
        }
        out
    }

    /// Drops queued requests; a running one's result is discarded.
    #[func]
    pub fn cancel_all(&mut self) {
        if let Some(n) = self.narrator.as_mut() {
            n.cancel_all();
        }
    }
}
