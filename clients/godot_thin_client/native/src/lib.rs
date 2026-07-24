//! Godot native extension for the Shadow Scale thin client.
//!
//! The GDScript-facing surface lives in [`bridge`]; the snapshot decode path is split
//! into [`snapshot`] (assemblers, rasters, deltas) and [`dict`] (per-section
//! converters, one module per `snapshot.fbs` section).

use godot::prelude::*;

mod bridge;
mod dict;
mod runtime;
mod snapshot;

pub use bridge::{CommandBridge, ScriptHostBridge, SnapshotDecoder};
pub use runtime::{
    manifest_to_json as script_manifest_to_json, responses_to_json as script_responses_to_json,
    Manager as ScriptRuntimeManager, ScriptError as ScriptHostError,
    ScriptResponse as ScriptRuntimeResponse,
};
pub use sim_runtime::scripting::ScriptManifest;

struct ShadowScaleExtension;

#[gdextension(entry_symbol = godot_rs_shadow_scale_godot_init)]
unsafe impl ExtensionLibrary for ShadowScaleExtension {}
