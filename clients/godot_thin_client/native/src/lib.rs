use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;
use std::collections::{BTreeSet, HashMap};
use std::sync::{
    mpsc::{self, Sender},
    OnceLock,
};
use std::thread;

mod runtime;

use runtime::{manifest_to_json, responses_to_json, transmit_proto_command, ScriptError};
pub use runtime::{
    manifest_to_json as script_manifest_to_json, responses_to_json as script_responses_to_json,
    Manager as ScriptRuntimeManager, ScriptError as ScriptHostError,
    ScriptResponse as ScriptRuntimeResponse,
};
use serde_json::{json, Map as JsonMap, Number as JsonNumber, Value as JsonValue};
pub use sim_runtime::scripting::ScriptManifest;
use sim_runtime::scripting::SimScriptState;
use sim_runtime::{parse_command_line, CommandEnvelope};

#[derive(GodotClass)]
#[class(base = RefCounted, init)]
pub struct CommandBridge;

static COMMAND_BRIDGE_SENDER: OnceLock<Sender<CommandRequest>> = OnceLock::new();

struct CommandRequest {
    host: String,
    port: u16,
    envelope: CommandEnvelope,
    callback: Sender<CommandResult>,
}

struct CommandResult {
    ok: bool,
    error: Option<String>,
}

#[godot_api]
impl CommandBridge {
    #[allow(dead_code)]
    fn init(_base: Base<RefCounted>) -> Self {
        let _ = command_sender();
        Self
    }

    #[func]
    pub fn send_line(&self, host: GString, proto_port: i64, line: GString) -> Dictionary {
        let mut dict = Dictionary::new();
        if proto_port <= 0 || proto_port > u16::MAX as i64 {
            let _ = dict.insert("ok", false);
            let _ = dict.insert("error", format!("invalid port {proto_port}"));
            return dict;
        }

        let host_str = host.to_string();
        let line_str = line.to_string();

        let envelope = match parse_command_line(&line_str) {
            Ok(payload) => CommandEnvelope {
                payload,
                correlation_id: None,
            },
            Err(err) => {
                let _ = dict.insert("ok", false);
                let _ = dict.insert("error", err.to_string());
                return dict;
            }
        };

        let sender = command_sender();

        let (tx, rx) = mpsc::channel();
        if let Err(err) = sender.send(CommandRequest {
            host: host_str,
            port: proto_port as u16,
            envelope,
            callback: tx,
        }) {
            let _ = dict.insert("ok", false);
            let _ = dict.insert("error", format!("dispatch error: {err}"));
            return dict;
        }

        match rx.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(result) => {
                let _ = dict.insert("ok", result.ok);
                if let Some(err) = result.error {
                    let _ = dict.insert("error", err);
                }
            }
            Err(_) => {
                let _ = dict.insert("ok", false);
                let _ = dict.insert("error", "command timed out");
            }
        }

        dict
    }
}

fn prototype_command_worker(receiver: mpsc::Receiver<CommandRequest>) {
    for request in receiver {
        let result = match transmit_proto_command(&request.host, request.port, &request.envelope) {
            Ok(_) => CommandResult {
                ok: true,
                error: None,
            },
            Err(err) => CommandResult {
                ok: false,
                error: Some(err),
            },
        };

        let _ = request.callback.send(result);
    }
}

fn command_sender() -> Sender<CommandRequest> {
    COMMAND_BRIDGE_SENDER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<CommandRequest>();
            thread::Builder::new()
                .name("command-bridge-worker".into())
                .spawn(move || prototype_command_worker(receiver))
                .expect("failed to spawn command bridge worker thread");
            sender
        })
        .clone()
}

fn resolve_entry_path(manifest_path: &str, entry: &str) -> String {
    if entry.starts_with("res://") || entry.starts_with("user://") {
        return entry.to_string();
    }
    let base = manifest_path
        .rfind('/')
        .map(|idx| &manifest_path[..=idx])
        .unwrap_or("");
    let trimmed = entry.strip_prefix("./").unwrap_or(entry);
    if base.is_empty() {
        trimmed.to_string()
    } else {
        format!("{base}{trimmed}")
    }
}

fn variant_to_string(value: &Variant) -> String {
    match value.get_type() {
        VariantType::BOOL => {
            let v: bool = value.to();
            v.to_string()
        }
        VariantType::INT => {
            let v: i64 = value.to();
            v.to_string()
        }
        VariantType::FLOAT => {
            let v: f64 = value.to();
            v.to_string()
        }
        VariantType::STRING | VariantType::STRING_NAME => {
            let v: GString = value.to();
            v.to_string()
        }
        _ => format!("{value:?}"),
    }
}

fn variant_to_json(value: &Variant) -> JsonValue {
    match value.get_type() {
        VariantType::NIL => JsonValue::Null,
        VariantType::BOOL => JsonValue::Bool(value.to()),
        VariantType::INT => {
            let v: i64 = value.to();
            JsonValue::Number(JsonNumber::from(v))
        }
        VariantType::FLOAT => {
            let v: f64 = value.to();
            JsonNumber::from_f64(v)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null)
        }
        VariantType::STRING | VariantType::STRING_NAME => {
            let v: GString = value.to();
            JsonValue::String(v.to_string())
        }
        VariantType::ARRAY => {
            let array: VariantArray = value.to();
            let mut result = Vec::with_capacity(array.len() as usize);
            for item in array.iter_shared() {
                result.push(variant_to_json(&item));
            }
            JsonValue::Array(result)
        }
        VariantType::DICTIONARY => {
            let dict: Dictionary = value.to();
            let mut map = JsonMap::new();
            for (k, v) in dict.iter_shared() {
                map.insert(variant_to_string(&k), variant_to_json(&v));
            }
            JsonValue::Object(map)
        }
        VariantType::PACKED_FLOAT32_ARRAY => {
            let arr: PackedFloat32Array = value.to();
            let mut result = Vec::with_capacity(arr.len() as usize);
            let len = arr.len();
            for idx in 0..len {
                if let Some(item) = arr.get(idx) {
                    let num =
                        JsonNumber::from_f64(item as f64).unwrap_or_else(|| JsonNumber::from(0));
                    result.push(JsonValue::Number(num));
                }
            }
            JsonValue::Array(result)
        }
        VariantType::PACKED_INT32_ARRAY => {
            let arr: PackedInt32Array = value.to();
            let mut result = Vec::with_capacity(arr.len() as usize);
            let len = arr.len();
            for idx in 0..len {
                if let Some(item) = arr.get(idx) {
                    result.push(JsonValue::Number(JsonNumber::from(item)));
                }
            }
            JsonValue::Array(result)
        }
        VariantType::PACKED_INT64_ARRAY => {
            let arr: PackedInt64Array = value.to();
            let mut result = Vec::with_capacity(arr.len() as usize);
            let len = arr.len();
            for idx in 0..len {
                if let Some(item) = arr.get(idx) {
                    result.push(JsonValue::Number(JsonNumber::from(item)));
                }
            }
            JsonValue::Array(result)
        }
        VariantType::PACKED_STRING_ARRAY => {
            let arr: PackedStringArray = value.to();
            let mut result = Vec::with_capacity(arr.len() as usize);
            let len = arr.len();
            for idx in 0..len {
                if let Some(item) = arr.get(idx) {
                    result.push(JsonValue::String(item.to_string()));
                }
            }
            JsonValue::Array(result)
        }
        _ => JsonValue::Null,
    }
}

fn json_to_variant(value: &JsonValue) -> Variant {
    match value {
        JsonValue::Null => Variant::nil(),
        JsonValue::Bool(b) => Variant::from(*b),
        JsonValue::Number(num) => {
            if let Some(i) = num.as_i64() {
                Variant::from(i)
            } else if let Some(u) = num.as_u64() {
                Variant::from(u as i64)
            } else if let Some(f) = num.as_f64() {
                Variant::from(f)
            } else {
                Variant::nil()
            }
        }
        JsonValue::String(s) => Variant::from(s.as_str()),
        JsonValue::Array(arr) => {
            let mut variant_array = VariantArray::new();
            for item in arr {
                let variant = json_to_variant(item);
                variant_array.push(&variant);
            }
            Variant::from(variant_array)
        }
        JsonValue::Object(map) => {
            let mut dict = Dictionary::new();
            for (key, value) in map {
                let _ = dict.insert(key.as_str(), json_to_variant(value));
            }
            Variant::from(dict)
        }
    }
}

fn json_to_variant_array(value: &JsonValue) -> VariantArray {
    match json_to_variant(value).try_to::<VariantArray>() {
        Ok(array) => array,
        Err(_) => VariantArray::new(),
    }
}

fn script_error_to_dict(err: ScriptError) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("ok", false);
    let _ = dict.insert("error", err.to_string());
    dict
}

#[derive(Default, GodotClass)]
#[class(init, base=RefCounted)]
pub struct ScriptHostBridge {
    manager: ScriptRuntimeManager,
}

#[godot_api]
impl ScriptHostBridge {
    #[func]
    pub fn parse_manifest(&self, manifest_path: GString, manifest_json: GString) -> Dictionary {
        let mut dict = Dictionary::new();
        let path_str = manifest_path.to_string();
        match ScriptManifest::parse_str(manifest_json.to_string().as_str()) {
            Ok(mut manifest) => {
                let entry_path = resolve_entry_path(&path_str, &manifest.entry);
                manifest.manifest_path = Some(path_str.clone());
                let manifest_variant = json_to_variant(&manifest_to_json(&manifest));
                let _ = dict.insert("ok", true);
                let _ = dict.insert("manifest", manifest_variant);
                let _ = dict.insert("manifest_path", path_str);
                let _ = dict.insert("entry_path", entry_path);
            }
            Err(err) => {
                let _ = dict.insert("ok", false);
                let _ = dict.insert("error", err.to_string());
            }
        }
        dict
    }

    #[func]
    pub fn spawn_script(
        &self,
        manifest_dict: Dictionary,
        manifest_path: GString,
        source: GString,
    ) -> Dictionary {
        let mut dict = Dictionary::new();
        let manifest_json = variant_to_json(&Variant::from(manifest_dict.clone()));
        match serde_json::from_value::<ScriptManifest>(manifest_json) {
            Ok(mut manifest) => {
                if let Err(err) = manifest.validate() {
                    let _ = dict.insert("ok", false);
                    let _ = dict.insert("error", err.to_string());
                    return dict;
                }
                let manifest_path_str = manifest_path.to_string();
                let manifest_path_opt = if manifest_path_str.is_empty() {
                    None
                } else {
                    Some(manifest_path_str)
                };
                manifest.manifest_path = manifest_path_opt.clone();
                match self
                    .manager
                    .spawn_script(manifest.clone(), source.to_string())
                {
                    Ok(id) => {
                        let _ = dict.insert("ok", true);
                        let _ = dict.insert("script_id", id);
                        let _ =
                            dict.insert("manifest", json_to_variant(&manifest_to_json(&manifest)));
                    }
                    Err(err) => {
                        let _ = dict.insert("ok", false);
                        let _ = dict.insert("error", err.to_string());
                    }
                }
            }
            Err(err) => {
                let _ = dict.insert("ok", false);
                let _ = dict.insert("error", format!("invalid manifest: {err}"));
            }
        }
        dict
    }

    #[func]
    pub fn shutdown_script(&self, script_id: i64) {
        self.manager.shutdown(script_id);
    }

    #[func]
    pub fn dispatch_event(&self, script_id: i64, event: GString, payload: Variant) -> Dictionary {
        let payload_json = variant_to_json(&payload);
        match self
            .manager
            .dispatch_event(script_id, &event.to_string(), payload_json)
        {
            Ok(_) => {
                let mut dict = Dictionary::new();
                let _ = dict.insert("ok", true);
                dict
            }
            Err(err) => script_error_to_dict(err),
        }
    }

    #[func]
    pub fn broadcast_event(&self, event: GString, payload: Variant) {
        let payload_json = variant_to_json(&payload);
        self.manager
            .broadcast_event(&event.to_string(), payload_json);
    }

    #[func]
    pub fn tick_script(&self, script_id: i64, delta: f64, budget_ms: f64) -> bool {
        self.manager.tick(script_id, delta, budget_ms).is_ok()
    }

    #[func]
    pub fn poll_responses(&self, script_id: i64) -> VariantArray {
        match self.manager.poll_responses(script_id) {
            Ok(responses) => json_to_variant_array(&responses_to_json(responses)),
            Err(err) => {
                let mut array = VariantArray::new();
                let variant =
                    json_to_variant(&json!({ "type": "error", "message": err.to_string() }));
                array.push(&variant);
                array
            }
        }
    }

    #[func]
    pub fn poll_all(&self) -> Dictionary {
        let mut dict = Dictionary::new();
        let map = self.manager.poll_all();
        for (id, responses) in map {
            let _ = dict.insert(id, json_to_variant_array(&responses_to_json(responses)));
        }
        dict
    }

    #[func]
    pub fn list_scripts(&self) -> VariantArray {
        let mut array = VariantArray::new();
        for (id, manifest) in self.manager.list_scripts() {
            let mut entry = Dictionary::new();
            let _ = entry.insert("script_id", id);
            let _ = entry.insert("manifest", json_to_variant(&manifest_to_json(&manifest)));
            let variant_entry = Variant::from(entry);
            array.push(&variant_entry);
        }
        array
    }

    #[func]
    pub fn subscriptions(&self, script_id: i64) -> VariantArray {
        match self.manager.subscriptions(script_id) {
            Ok(subs) => {
                let mut array = VariantArray::new();
                for sub in subs {
                    let variant = Variant::from(sub.as_str());
                    array.push(&variant);
                }
                array
            }
            Err(_) => VariantArray::new(),
        }
    }

    #[func]
    pub fn snapshot_session(&self, script_id: i64) -> Variant {
        match self.manager.snapshot_session(script_id) {
            Ok(value) => json_to_variant(&value),
            Err(_) => Variant::nil(),
        }
    }

    #[func]
    pub fn restore_session(&self, script_id: i64, data: Variant) -> bool {
        let json = variant_to_json(&data);
        self.manager.restore_session(script_id, json).is_ok()
    }

    #[func]
    pub fn snapshot_active_scripts(&self) -> VariantArray {
        let mut array = VariantArray::new();
        for state in self.manager.snapshot_states() {
            if let Ok(json) = serde_json::to_value(&state) {
                let variant = json_to_variant(&json);
                array.push(&variant);
            }
        }
        array
    }

    #[func]
    pub fn set_command_endpoint(&self, host: GString, proto_port: i64) {
        if proto_port <= 0 || proto_port > u16::MAX as i64 {
            self.manager.clear_command_endpoint();
            return;
        }
        self.manager
            .set_command_endpoint(host.to_string(), proto_port as u16);
    }

    #[func]
    pub fn apply_script_state(&self, script_id: i64, state: Variant) -> Dictionary {
        let mut dict = Dictionary::new();
        let json = variant_to_json(&state);
        match serde_json::from_value::<SimScriptState>(json) {
            Ok(state_struct) => match self.manager.apply_state(script_id, &state_struct) {
                Ok(_) => {
                    let _ = dict.insert("ok", true);
                }
                Err(err) => return script_error_to_dict(err),
            },
            Err(err) => {
                let _ = dict.insert("ok", false);
                let _ = dict.insert("error", format!("invalid script state payload: {err}"));
            }
        }
        dict
    }
}

fn packed_from_slice(values: &[f32]) -> PackedFloat32Array {
    if values.is_empty() {
        return PackedFloat32Array::new();
    }
    let mut array = PackedFloat32Array::new();
    array.resize(values.len());
    array.as_mut_slice().copy_from_slice(values);
    array
}

struct OverlayChannelParams<'a> {
    key: &'a str,
    label: &'a str,
    description: Option<&'a str>,
    normalized: &'a PackedFloat32Array,
    raw: &'a PackedFloat32Array,
    contrast: &'a PackedFloat32Array,
    placeholder: bool,
}

fn insert_overlay_channel(
    channels: &mut Dictionary,
    order: &mut PackedStringArray,
    params: OverlayChannelParams<'_>,
) {
    let mut channel = Dictionary::new();
    let _ = channel.insert("label", params.label);
    if let Some(description) = params.description {
        let _ = channel.insert("description", description);
    }
    let _ = channel.insert("normalized", params.normalized.clone());
    let _ = channel.insert("raw", params.raw.clone());
    let _ = channel.insert("contrast", params.contrast.clone());
    if params.placeholder {
        let _ = channel.insert("placeholder", true);
    }
    let _ = channels.insert(params.key, channel);
    let key_str = GString::from(params.key);
    order.push(&key_str);
}

struct GridSize {
    width: u32,
    height: u32,
}

struct OverlaySlices<'a> {
    logistics: &'a [f32],
    sentiment: &'a [f32],
    corruption: &'a [f32],
    fog: &'a [f32],
    culture: &'a [f32],
    military: &'a [f32],
    crisis: &'a [f32],
    elevation: &'a [f32],
    moisture: &'a [f32],
}

struct TerrainSlices<'a> {
    terrain: Option<&'a [u16]>,
    tags: Option<&'a [u16]>,
}

#[derive(Clone, Default)]
struct CrisisAnnotationRecord {
    label: Option<String>,
    severity: fb::CrisisSeverityBand,
    path: Vec<i32>,
}

#[allow(clippy::too_many_arguments)]
fn snapshot_dict(
    tick: u64,
    grid_size: GridSize,
    overlays: OverlaySlices<'_>,
    terrain: TerrainSlices<'_>,
    crisis_annotations: &[CrisisAnnotationRecord],
    hydrology_rivers: Option<&VariantArray>,
    start_marker: Option<(u32, u32)>,
    campaign_label: Option<Dictionary>,
    campaign_profiles: Option<VariantArray>,
    victory_state: Option<Dictionary>,
    faction_inventory: Option<VariantArray>,
    command_events: Option<VariantArray>,
    herds: Option<VariantArray>,
    food_modules: Option<VariantArray>,
) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("turn", tick as i64);

    let mut grid_dict = Dictionary::new();
    let _ = grid_dict.insert("width", grid_size.width as i64);
    let _ = grid_dict.insert("height", grid_size.height as i64);
    let _ = dict.insert("grid", grid_dict);

    let size = (grid_size.width as usize)
        .saturating_mul(grid_size.height as usize)
        .max(1);

    let copy_into = |source: &[f32]| -> Vec<f32> {
        let mut dest = vec![0.0f32; size];
        let count = source.len().min(size);
        if count > 0 {
            dest[..count].copy_from_slice(&source[..count]);
        }
        dest
    };

    let logistics_base = copy_into(overlays.logistics);
    let sentiment_base = copy_into(overlays.sentiment);
    let corruption_base = copy_into(overlays.corruption);
    let fog_base = copy_into(overlays.fog);
    let culture_base = copy_into(overlays.culture);
    let military_base = copy_into(overlays.military);
    let crisis_base = copy_into(overlays.crisis);
    let elevation_base = copy_into(overlays.elevation);
    let moisture_base = copy_into(overlays.moisture);

    let mut logistics_normalized = logistics_base.clone();
    normalize_overlay(&mut logistics_normalized);
    let mut sentiment_normalized = sentiment_base.clone();
    normalize_overlay(&mut sentiment_normalized);
    let mut corruption_normalized = corruption_base.clone();
    normalize_overlay(&mut corruption_normalized);
    let mut fog_normalized = fog_base.clone();
    normalize_overlay(&mut fog_normalized);
    let mut culture_normalized = culture_base.clone();
    normalize_overlay(&mut culture_normalized);
    let mut military_normalized = military_base.clone();
    normalize_overlay(&mut military_normalized);
    let mut crisis_normalized = crisis_base.clone();
    normalize_overlay(&mut crisis_normalized);
    let mut elevation_normalized = elevation_base.clone();
    normalize_overlay(&mut elevation_normalized);
    let mut moisture_normalized = moisture_base.clone();
    normalize_overlay(&mut moisture_normalized);

    let mut logistics_contrast_vec = logistics_normalized.clone();
    for value in logistics_contrast_vec.iter_mut() {
        let v = *value;
        *value = v * (1.0 - v);
    }

    let mut sentiment_contrast_vec = sentiment_normalized.clone();
    for value in sentiment_contrast_vec.iter_mut() {
        *value = ((*value - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    }

    let corruption_contrast_vec = corruption_normalized.clone();
    let fog_contrast_vec = fog_normalized.clone();
    let culture_contrast_vec = culture_normalized.clone();
    let mut military_contrast_vec = military_normalized.clone();
    for value in military_contrast_vec.iter_mut() {
        *value = ((*value - 0.5).abs() * 2.0).clamp(0.0, 1.0);
    }
    let mut crisis_contrast_vec = crisis_normalized.clone();
    for value in crisis_contrast_vec.iter_mut() {
        let v = *value;
        *value = v * (1.0 - v);
    }
    let elevation_contrast_vec = elevation_normalized.clone();
    let moisture_contrast_vec = moisture_normalized.clone();

    let corruption_placeholder = overlays.corruption.is_empty();
    let fog_placeholder = overlays.fog.is_empty();
    let culture_placeholder = overlays.culture.is_empty();
    let military_placeholder = overlays.military.is_empty();
    let crisis_placeholder = overlays.crisis.is_empty();

    let logistics_array = packed_from_slice(&logistics_normalized);
    let logistics_raw_array = packed_from_slice(&logistics_base);
    let logistics_contrast_array = packed_from_slice(&logistics_contrast_vec);
    let sentiment_array = packed_from_slice(&sentiment_normalized);
    let sentiment_raw_array = packed_from_slice(&sentiment_base);
    let sentiment_contrast_array = packed_from_slice(&sentiment_contrast_vec);
    let corruption_array = packed_from_slice(&corruption_normalized);
    let corruption_raw_array = packed_from_slice(&corruption_base);
    let corruption_contrast_array = packed_from_slice(&corruption_contrast_vec);
    let fog_array = packed_from_slice(&fog_normalized);
    let fog_raw_array = packed_from_slice(&fog_base);
    let fog_contrast_array = packed_from_slice(&fog_contrast_vec);
    let culture_array = packed_from_slice(&culture_normalized);
    let culture_raw_array = packed_from_slice(&culture_base);
    let culture_contrast_array = packed_from_slice(&culture_contrast_vec);
    let military_array = packed_from_slice(&military_normalized);
    let military_raw_array = packed_from_slice(&military_base);
    let military_contrast_array = packed_from_slice(&military_contrast_vec);
    let crisis_array = packed_from_slice(&crisis_normalized);
    let crisis_raw_array = packed_from_slice(&crisis_base);
    let crisis_contrast_array = packed_from_slice(&crisis_contrast_vec);
    let elevation_array = packed_from_slice(&elevation_normalized);
    let elevation_raw_array = packed_from_slice(&elevation_base);
    let elevation_contrast_array = packed_from_slice(&elevation_contrast_vec);
    let moisture_array = packed_from_slice(&moisture_normalized);
    let moisture_raw_array = packed_from_slice(&moisture_base);
    let moisture_contrast_array = packed_from_slice(&moisture_contrast_vec);

    let elevation_placeholder = elevation_array.is_empty();
    let moisture_placeholder = overlays.moisture.is_empty();

    let mut overlays = Dictionary::new();
    let mut channels = Dictionary::new();
    let mut channel_order = PackedStringArray::new();

    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "logistics",
            label: "Logistics Throughput",
            description: Some(
                "Sum of supply flow touching the tile after current corruption multipliers.",
            ),
            normalized: &logistics_array,
            raw: &logistics_raw_array,
            contrast: &logistics_contrast_array,
            placeholder: false,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "crisis",
            label: "Crisis Stress",
            description: Some(
                "Normalized crisis pressure per tile derived from local grid stability and incidents.",
            ),
            normalized: &crisis_array,
            raw: &crisis_raw_array,
            contrast: &crisis_contrast_array,
            placeholder: crisis_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "sentiment",
            label: "Sentiment Morale",
            description: Some(
                "Average morale of population cohorts anchored to the tile (fixed-point scale).",
            ),
            normalized: &sentiment_array,
            raw: &sentiment_raw_array,
            contrast: &sentiment_contrast_array,
            placeholder: false,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "corruption",
            label: "Corruption Pressure",
            description: Some(
                "Composite pressure mixing active incidents with logistics, trade, military, and governance risk at each tile.",
            ),
            normalized: &corruption_array,
            raw: &corruption_raw_array,
            contrast: &corruption_contrast_array,
            placeholder: corruption_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "fog",
            label: "Fog of Knowledge",
            description: Some(
                "Knowledge gap for the controlling faction and local cohorts (1.0 = unknown, 0.0 = fully scouted).",
            ),
            normalized: &fog_array,
            raw: &fog_raw_array,
            contrast: &fog_contrast_array,
            placeholder: fog_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "culture",
            label: "Culture Divergence",
            description: Some(
                "Local layer divergence relative to schism thresholds (1.0 = schism risk).",
            ),
            normalized: &culture_array,
            raw: &culture_raw_array,
            contrast: &culture_contrast_array,
            placeholder: culture_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "military",
            label: "Force Readiness",
            description: Some("Composite of garrison morale, manpower, and local supply margin."),
            normalized: &military_array,
            raw: &military_raw_array,
            contrast: &military_contrast_array,
            placeholder: military_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "moisture",
            label: "Moisture & Rain Shadows",
            description: Some(
                "Humidity field after windward lift and leeward drying (0 = arid, 1 = saturated).",
            ),
            normalized: &moisture_array,
            raw: &moisture_raw_array,
            contrast: &moisture_contrast_array,
            placeholder: moisture_placeholder,
        },
    );
    insert_overlay_channel(
        &mut channels,
        &mut channel_order,
        OverlayChannelParams {
            key: "elevation",
            label: "Elevation Heatmap",
            description: Some(
                "Relative elevation above sea level after tectonic restamp (0 = coast, 1 = peaks).",
            ),
            normalized: &elevation_array,
            raw: &elevation_raw_array,
            contrast: &elevation_contrast_array,
            placeholder: elevation_base.is_empty(),
        },
    );

    let _ = overlays.insert("channels", channels);
    let _ = overlays.insert("channel_order", channel_order.clone());
    let _ = overlays.insert("default_channel", "logistics");

    if corruption_placeholder
        || fog_placeholder
        || culture_placeholder
        || military_placeholder
        || crisis_placeholder
        || elevation_placeholder
        || moisture_placeholder
    {
        let mut placeholder_keys = PackedStringArray::new();
        if corruption_placeholder {
            placeholder_keys.push(&GString::from("corruption"));
        }
        if fog_placeholder {
            placeholder_keys.push(&GString::from("fog"));
        }
        if culture_placeholder {
            placeholder_keys.push(&GString::from("culture"));
        }
        if military_placeholder {
            placeholder_keys.push(&GString::from("military"));
        }
        if crisis_placeholder {
            placeholder_keys.push(&GString::from("crisis"));
        }
        if elevation_placeholder {
            placeholder_keys.push(&GString::from("elevation"));
        }
        if moisture_placeholder {
            placeholder_keys.push(&GString::from("moisture"));
        }
        let _ = overlays.insert("placeholder_channels", placeholder_keys);
    }

    let _ = overlays.insert("logistics", logistics_array.clone());
    let _ = overlays.insert("logistics_raw", logistics_raw_array.clone());
    let _ = overlays.insert("logistics_contrast", logistics_contrast_array.clone());
    let _ = overlays.insert("contrast", logistics_contrast_array.clone());
    let _ = overlays.insert("sentiment", sentiment_array.clone());
    let _ = overlays.insert("sentiment_raw", sentiment_raw_array.clone());
    let _ = overlays.insert("sentiment_contrast", sentiment_contrast_array.clone());
    let _ = overlays.insert("corruption", corruption_array.clone());
    let _ = overlays.insert("corruption_raw", corruption_raw_array.clone());
    let _ = overlays.insert("corruption_contrast", corruption_contrast_array.clone());
    let _ = overlays.insert("fog", fog_array.clone());
    let _ = overlays.insert("fog_raw", fog_raw_array.clone());
    let _ = overlays.insert("fog_contrast", fog_contrast_array.clone());
    let _ = overlays.insert("culture", culture_array.clone());
    let _ = overlays.insert("culture_raw", culture_raw_array.clone());
    let _ = overlays.insert("culture_contrast", culture_contrast_array.clone());
    let _ = overlays.insert("military", military_array.clone());
    let _ = overlays.insert("military_raw", military_raw_array.clone());
    let _ = overlays.insert("military_contrast", military_contrast_array.clone());
    let _ = overlays.insert("crisis", crisis_array.clone());
    let _ = overlays.insert("crisis_raw", crisis_raw_array.clone());
    let _ = overlays.insert("crisis_contrast", crisis_contrast_array.clone());
    let _ = overlays.insert("elevation", elevation_array.clone());
    let _ = overlays.insert("elevation_raw", elevation_raw_array.clone());
    let _ = overlays.insert("elevation_contrast", elevation_contrast_array.clone());
    let _ = overlays.insert("moisture", moisture_array.clone());
    let _ = overlays.insert("moisture_raw", moisture_raw_array.clone());
    let _ = overlays.insert("moisture_contrast", moisture_contrast_array.clone());
    let mut crisis_annotation_array = VariantArray::new();
    for record in crisis_annotations {
        let dict = crisis_annotation_to_dict(record);
        crisis_annotation_array.push(&dict.to_variant());
    }
    let _ = overlays.insert("crisis_annotations", crisis_annotation_array);

    if let Some(terrain_data) = terrain.terrain {
        let mut terrain_array = PackedInt32Array::new();
        terrain_array.resize(size);
        if size > 0 {
            let slice = terrain_array.as_mut_slice();
            let count = terrain_data.len().min(slice.len());
            for i in 0..count {
                slice[i] = terrain_data[i] as i32;
            }
        }
        let _ = overlays.insert("terrain", terrain_array);

        if let Some(tag_data) = terrain.tags {
            let mut tag_array = PackedInt32Array::new();
            tag_array.resize(size);
            if size > 0 {
                let slice = tag_array.as_mut_slice();
                let count = tag_data.len().min(slice.len());
                for i in 0..count {
                    slice[i] = tag_data[i] as i32;
                }
            }
            let _ = overlays.insert("terrain_tags", tag_array);
        }

        let mut palette = Dictionary::new();
        let mut seen = BTreeSet::new();
        for &value in terrain_data {
            if seen.insert(value) {
                let _ = palette.insert(value as i64, terrain_label_from_id(value));
            }
        }
        let _ = overlays.insert("terrain_palette", palette);

        let mut tag_labels = Dictionary::new();
        for (mask, label) in TERRAIN_TAG_LABELS.iter() {
            let _ = tag_labels.insert(*mask as i64, *label);
        }
        let _ = overlays.insert("terrain_tag_labels", tag_labels);
    }

    if let Some(rivers) = hydrology_rivers {
        let _ = overlays.insert("hydrology_rivers", rivers.clone());
    }
    if let Some((sx, sy)) = start_marker {
        let mut marker = Dictionary::new();
        let _ = marker.insert("x", sx as i64);
        let _ = marker.insert("y", sy as i64);
        let _ = overlays.insert("start_marker", marker);
    }

    let _ = dict.insert("overlays", overlays);

    let _ = dict.insert("units", VariantArray::new());
    let _ = dict.insert("orders", VariantArray::new());

    if let Some(label) = campaign_label {
        let _ = dict.insert("campaign_label", label);
    }
    if let Some(profiles) = campaign_profiles {
        let _ = dict.insert("campaign_profiles", profiles);
    }
    if let Some(victory) = victory_state {
        let _ = dict.insert("victory", victory);
    }
    if let Some(inventory) = faction_inventory {
        if !inventory.is_empty() {
            let _ = dict.insert("faction_inventory", inventory);
        }
    }
    if let Some(events) = command_events {
        if !events.is_empty() {
            let _ = dict.insert("command_events", events);
        }
    }
    if let Some(herd_array) = herds {
        if !herd_array.is_empty() {
            let _ = dict.insert("herds", herd_array);
        }
    }
    if let Some(food_array) = food_modules {
        if !food_array.is_empty() {
            let _ = dict.insert("food_modules", food_array);
        }
    }

    dict
}
#[derive(Default, GodotClass)]
#[class(init, base=RefCounted)]
pub struct SnapshotDecoder;

#[godot_api]
impl SnapshotDecoder {
    #[func]
    pub fn decode_snapshot(&self, data: PackedByteArray) -> Dictionary {
        decode_snapshot(&data).unwrap_or_default()
    }

    #[func]
    pub fn decode_delta(&self, data: PackedByteArray) -> Dictionary {
        decode_delta(&data).unwrap_or_default()
    }
}

fn decode_snapshot(data: &PackedByteArray) -> Option<Dictionary> {
    if data.is_empty() {
        return None;
    }
    let bytes = data.as_slice();
    let envelope = fb::root_as_envelope(bytes).ok()?;
    match envelope.payload_type() {
        fb::SnapshotPayload::snapshot => envelope.payload_as_snapshot().map(snapshot_to_dict),
        fb::SnapshotPayload::delta => decode_delta(data),
        _ => None,
    }
}

fn decode_delta(data: &PackedByteArray) -> Option<Dictionary> {
    if data.is_empty() {
        return None;
    }
    let bytes = data.as_slice();
    let envelope = fb::root_as_envelope(bytes).ok()?;
    if envelope.payload_type() != fb::SnapshotPayload::delta {
        return None;
    }
    let delta = envelope.payload_as_delta()?;
    // For now, render deltas by synthesizing a snapshot-sized dictionary where only
    // updated tiles affect the overlays. This keeps the UI responsive while we pump
    // full snapshots on the same stream.
    let mut agg = DeltaAggregator::default();
    if let Some(header) = delta.header() {
        agg.tick = header.tick();
    }
    if let Some(tiles) = delta.tiles() {
        for tile in tiles {
            agg.update_tile(tile.x(), tile.y(), tile.temperature());
        }
    }
    if let Some(layer) = delta.terrainOverlay() {
        agg.apply_terrain_overlay(layer);
    }
    if let Some(raster) = delta.logisticsRaster() {
        agg.apply_logistics_raster(raster);
    }
    if let Some(raster) = delta.sentimentRaster() {
        agg.apply_sentiment_raster(raster);
    }
    if let Some(raster) = delta.corruptionRaster() {
        agg.apply_corruption_raster(raster);
    }
    if let Some(raster) = delta.fogRaster() {
        agg.apply_fog_raster(raster);
    }
    if let Some(raster) = delta.cultureRaster() {
        agg.apply_culture_raster(raster);
    }
    if let Some(raster) = delta.militaryRaster() {
        agg.apply_military_raster(raster);
    }
    if let Some(overlay) = delta.crisisOverlay() {
        agg.apply_crisis_overlay(overlay);
    }
    if let Some(overlay) = delta.elevationOverlay() {
        agg.apply_elevation_overlay(overlay);
    }
    if let Some(raster) = delta.moistureRaster() {
        agg.apply_moisture_raster(raster);
    }
    if let Some(marker) = delta.startMarker() {
        agg.start_marker = Some((marker.x(), marker.y()));
    }
    let mut dict = agg.into_dictionary();

    if let Some(victory) = delta.victory() {
        let _ = dict.insert("victory", victory_state_to_dict(victory));
    }

    if let Some(events) = delta.commandEvents() {
        let _ = dict.insert("command_events", command_events_to_array(events));
    }

    if let Some(herds) = delta.herds() {
        let _ = dict.insert("herds", herds_to_array(herds));
    }

    if let Some(definitions) = delta.greatDiscoveryDefinitions() {
        let _ = dict.insert(
            "great_discovery_definitions",
            great_discovery_definitions_to_array(definitions),
        );
    }

    if let Some(axis_bias) = delta.axisBias() {
        let _ = dict.insert("axis_bias", axis_bias_to_dict(axis_bias));
    }

    if let Some(sentiment) = delta.sentiment() {
        let _ = dict.insert("sentiment", sentiment_to_dict(sentiment));
    }

    if let Some(crisis) = delta.crisisTelemetry() {
        let _ = dict.insert("crisis_telemetry", crisis_telemetry_to_dict(crisis));
    }

    if let Some(crisis_overlay) = delta.crisisOverlay() {
        let _ = dict.insert("crisis_overlay", crisis_overlay_to_dict(crisis_overlay));
    }

    if let Some(great_discoveries) = delta.greatDiscoveries() {
        let updates = great_discovery_states_to_array(great_discoveries);
        if !updates.is_empty() {
            let _ = dict.insert("great_discovery_updates", updates);
        }
    }

    if let Some(great_progress) = delta.greatDiscoveryProgress() {
        let updates = great_discovery_progress_states_to_array(great_progress);
        if !updates.is_empty() {
            let _ = dict.insert("great_discovery_progress_updates", updates);
        }
    }

    if let Some(gd_telemetry) = delta.greatDiscoveryTelemetry() {
        let _ = dict.insert(
            "great_discovery_telemetry",
            great_discovery_telemetry_to_dict(gd_telemetry),
        );
    }

    if let Some(influencers) = delta.influencers() {
        let _ = dict.insert("influencer_updates", influencers_to_array(influencers));
    }

    let removed_influencers = u32_vector_to_packed_int32(delta.removedInfluencers());
    if !removed_influencers.is_empty() {
        let _ = dict.insert("influencer_removed", removed_influencers);
    }

    if let Some(ledger) = delta.corruption() {
        let _ = dict.insert("corruption", corruption_to_dict(ledger));
    }

    if let Some(populations) = delta.populations() {
        let _ = dict.insert("population_updates", populations_to_array(populations));
    }

    let removed_populations = u64_vector_to_packed_int64(delta.removedPopulations());
    if !removed_populations.is_empty() {
        let _ = dict.insert("population_removed", removed_populations);
    }

    if let Some(trade_links) = delta.tradeLinks() {
        let _ = dict.insert("trade_link_updates", trade_links_to_array(trade_links));
    }

    let removed_trade_links = u64_vector_to_packed_int64(delta.removedTradeLinks());
    if !removed_trade_links.is_empty() {
        let _ = dict.insert("trade_link_removed", removed_trade_links);
    }

    if let Some(power_nodes) = delta.power() {
        let _ = dict.insert("power_updates", power_nodes_to_array(power_nodes));
    }

    let removed_power = u64_vector_to_packed_int64(delta.removedPower());
    if !removed_power.is_empty() {
        let _ = dict.insert("power_removed", removed_power);
    }

    if let Some(power_metrics) = delta.powerMetrics() {
        let _ = dict.insert("power_metrics", power_metrics_to_dict(power_metrics));
    }

    if let Some(tiles) = delta.tiles() {
        let _ = dict.insert("tile_updates", tiles_to_array(tiles));
    }

    let removed_tiles = u64_vector_to_packed_int64(delta.removedTiles());
    if !removed_tiles.is_empty() {
        let _ = dict.insert("tile_removed", removed_tiles);
    }

    if let Some(generations) = delta.generations() {
        let _ = dict.insert("generation_updates", generations_to_array(generations));
    }

    let removed_generations = u16_vector_to_packed_int32(delta.removedGenerations());
    if !removed_generations.is_empty() {
        let _ = dict.insert("generation_removed", removed_generations);
    }

    if let Some(layers) = delta.cultureLayers() {
        let _ = dict.insert("culture_layer_updates", culture_layers_to_array(layers));
    }

    let removed_layers = u32_vector_to_packed_int32(delta.removedCultureLayers());
    if !removed_layers.is_empty() {
        let _ = dict.insert("culture_layer_removed", removed_layers);
    }

    if let Some(tensions) = delta.cultureTensions() {
        let _ = dict.insert("culture_tensions", culture_tensions_to_array(tensions));
    }

    if let Some(progress) = delta.discoveryProgress() {
        let _ = dict.insert(
            "discovery_progress_updates",
            discovery_progress_to_array(progress),
        );
    }

    Some(dict)
}

#[derive(Default)]
struct DeltaAggregator {
    tick: u64,
    width: u32,
    height: u32,
    tile_updates: HashMap<(u32, u32), f32>,
    terrain_width: u32,
    terrain_height: u32,
    terrain_types: Vec<u16>,
    terrain_tags: Vec<u16>,
    logistics_width: u32,
    logistics_height: u32,
    logistics_samples: Vec<f32>,
    sentiment_width: u32,
    sentiment_height: u32,
    sentiment_samples: Vec<f32>,
    corruption_width: u32,
    corruption_height: u32,
    corruption_samples: Vec<f32>,
    fog_width: u32,
    fog_height: u32,
    fog_samples: Vec<f32>,
    culture_width: u32,
    culture_height: u32,
    culture_samples: Vec<f32>,
    military_width: u32,
    military_height: u32,
    military_samples: Vec<f32>,
    crisis_width: u32,
    crisis_height: u32,
    crisis_samples: Vec<f32>,
    elevation_width: u32,
    elevation_height: u32,
    elevation_samples: Vec<f32>,
    moisture_width: u32,
    moisture_height: u32,
    moisture_samples: Vec<f32>,
    crisis_annotations: Vec<CrisisAnnotationRecord>,
    start_marker: Option<(u32, u32)>,
}

impl DeltaAggregator {
    fn update_tile(&mut self, x: u32, y: u32, temperature: i64) {
        self.width = self.width.max(x + 1);
        self.height = self.height.max(y + 1);
        self.tile_updates
            .insert((x, y), fixed64_to_f32(temperature));
    }

    fn apply_terrain_overlay(&mut self, overlay: fb::TerrainOverlay<'_>) {
        self.terrain_width = overlay.width();
        self.terrain_height = overlay.height();
        let count = (self.terrain_width as usize)
            .saturating_mul(self.terrain_height as usize)
            .max(1);
        self.terrain_types.resize(count, 0);
        self.terrain_tags.resize(count, 0);
        if let Some(samples) = overlay.samples() {
            for (idx, sample) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.terrain_types[idx] = sample.terrain().0;
                self.terrain_tags[idx] = sample.tags();
            }
        }
    }

    fn apply_logistics_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.logistics_width = raster.width();
        self.logistics_height = raster.height();
        let count = (self.logistics_width as usize)
            .saturating_mul(self.logistics_height as usize)
            .max(1);
        self.logistics_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.logistics_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    fn apply_sentiment_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.sentiment_width = raster.width();
        self.sentiment_height = raster.height();
        let count = (self.sentiment_width as usize)
            .saturating_mul(self.sentiment_height as usize)
            .max(1);
        self.sentiment_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.sentiment_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    fn apply_corruption_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.corruption_width = raster.width();
        self.corruption_height = raster.height();
        let count = (self.corruption_width as usize)
            .saturating_mul(self.corruption_height as usize)
            .max(1);
        self.corruption_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.corruption_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    fn apply_fog_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.fog_width = raster.width();
        self.fog_height = raster.height();
        let count = (self.fog_width as usize)
            .saturating_mul(self.fog_height as usize)
            .max(1);
        self.fog_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.fog_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    fn apply_culture_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.culture_width = raster.width();
        self.culture_height = raster.height();
        let count = (self.culture_width as usize)
            .saturating_mul(self.culture_height as usize)
            .max(1);
        self.culture_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.culture_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    fn apply_military_raster(&mut self, raster: fb::ScalarRaster<'_>) {
        self.military_width = raster.width();
        self.military_height = raster.height();
        let count = (self.military_width as usize)
            .saturating_mul(self.military_height as usize)
            .max(1);
        self.military_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.military_samples[idx] = fixed64_to_f32(value);
            }
        }
    }

    fn apply_crisis_overlay(&mut self, overlay: fb::CrisisOverlayState<'_>) {
        if let Some(raster) = overlay.heatmap() {
            self.crisis_width = raster.width();
            self.crisis_height = raster.height();
            let count = (self.crisis_width as usize)
                .saturating_mul(self.crisis_height as usize)
                .max(1);
            self.crisis_samples.resize(count, 0.0);
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= count {
                        break;
                    }
                    self.crisis_samples[idx] = fixed64_to_f32(value);
                }
            }
        }
        self.crisis_annotations.clear();
        if let Some(entries) = overlay.annotations() {
            self.crisis_annotations.reserve(entries.len());
            for entry in entries {
                let mut path = Vec::new();
                if let Some(route) = entry.path() {
                    path.reserve(route.len());
                    for value in route {
                        path.push(value as i32);
                    }
                }
                self.crisis_annotations.push(CrisisAnnotationRecord {
                    label: entry.label().map(|value| value.to_string()),
                    severity: entry.severity(),
                    path,
                });
            }
        }
    }

    fn apply_elevation_overlay(&mut self, overlay: fb::ElevationOverlay<'_>) {
        self.elevation_width = overlay.width();
        self.elevation_height = overlay.height();
        let count = (self.elevation_width as usize)
            .saturating_mul(self.elevation_height as usize)
            .max(1);
        self.elevation_samples.resize(count, 0.0);
        if let Some(samples) = overlay.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.elevation_samples[idx] = (value as f32) / 255.0;
            }
        }
    }

    fn apply_moisture_raster(&mut self, raster: fb::FloatRaster<'_>) {
        self.moisture_width = raster.width();
        self.moisture_height = raster.height();
        let count = (self.moisture_width as usize)
            .saturating_mul(self.moisture_height as usize)
            .max(1);
        self.moisture_samples.resize(count, 0.0);
        if let Some(samples) = raster.samples() {
            for (idx, value) in samples.iter().enumerate() {
                if idx >= count {
                    break;
                }
                self.moisture_samples[idx] = value;
            }
        }
    }

    fn into_dictionary(self) -> Dictionary {
        let DeltaAggregator {
            tick,
            width,
            height,
            tile_updates,
            terrain_width,
            terrain_height,
            terrain_types,
            terrain_tags,
            logistics_width,
            logistics_height,
            logistics_samples,
            sentiment_width,
            sentiment_height,
            sentiment_samples,
            corruption_width,
            corruption_height,
            corruption_samples,
            fog_width,
            fog_height,
            fog_samples,
            culture_width,
            culture_height,
            culture_samples,
            military_width,
            military_height,
            military_samples,
            crisis_width,
            crisis_height,
            crisis_samples,
            elevation_width,
            elevation_height,
            elevation_samples,
            moisture_width,
            moisture_height,
            moisture_samples,
            crisis_annotations,
            start_marker,
        } = self;

        let mut final_width = terrain_width
            .max(width)
            .max(logistics_width)
            .max(sentiment_width)
            .max(corruption_width)
            .max(fog_width)
            .max(culture_width)
            .max(military_width)
            .max(crisis_width)
            .max(elevation_width)
            .max(moisture_width);
        let mut final_height = terrain_height
            .max(height)
            .max(logistics_height)
            .max(sentiment_height)
            .max(corruption_height)
            .max(fog_height)
            .max(culture_height)
            .max(military_height)
            .max(crisis_height)
            .max(elevation_height)
            .max(moisture_height);
        if final_width == 0 || final_height == 0 {
            final_width = final_width.max(1);
            final_height = final_height.max(1);
        }
        let total = (final_width as usize)
            .saturating_mul(final_height as usize)
            .max(1);

        let mut logistics = vec![0.0f32; total];
        if logistics_width > 0 && logistics_height > 0 && !logistics_samples.is_empty() {
            for y in 0..logistics_height {
                for x in 0..logistics_width {
                    let src_idx = (y as usize) * (logistics_width as usize) + x as usize;
                    if src_idx >= logistics_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    logistics[dst_idx] = logistics_samples[src_idx];
                }
            }
        } else {
            for ((x, y), value) in tile_updates {
                if x >= final_width || y >= final_height {
                    continue;
                }
                let idx = (y as usize) * (final_width as usize) + x as usize;
                logistics[idx] = value;
            }
        }

        let mut sentiment = vec![0.0f32; total];
        if sentiment_width > 0 && sentiment_height > 0 && !sentiment_samples.is_empty() {
            for y in 0..sentiment_height {
                for x in 0..sentiment_width {
                    let src_idx = (y as usize) * (sentiment_width as usize) + x as usize;
                    if src_idx >= sentiment_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    sentiment[dst_idx] = sentiment_samples[src_idx];
                }
            }
        }

        let mut corruption = vec![0.0f32; total];
        if corruption_width > 0 && corruption_height > 0 && !corruption_samples.is_empty() {
            for y in 0..corruption_height {
                for x in 0..corruption_width {
                    let src_idx = (y as usize) * (corruption_width as usize) + x as usize;
                    if src_idx >= corruption_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    corruption[dst_idx] = corruption_samples[src_idx];
                }
            }
        }

        let mut fog = vec![0.0f32; total];
        if fog_width > 0 && fog_height > 0 && !fog_samples.is_empty() {
            for y in 0..fog_height {
                for x in 0..fog_width {
                    let src_idx = (y as usize) * (fog_width as usize) + x as usize;
                    if src_idx >= fog_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    fog[dst_idx] = fog_samples[src_idx];
                }
            }
        }

        let mut culture = vec![0.0f32; total];
        if culture_width > 0 && culture_height > 0 && !culture_samples.is_empty() {
            for y in 0..culture_height {
                for x in 0..culture_width {
                    let src_idx = (y as usize) * (culture_width as usize) + x as usize;
                    if src_idx >= culture_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    culture[dst_idx] = culture_samples[src_idx];
                }
            }
        }

        let mut military = vec![0.0f32; total];
        if military_width > 0 && military_height > 0 && !military_samples.is_empty() {
            for y in 0..military_height {
                for x in 0..military_width {
                    let src_idx = (y as usize) * (military_width as usize) + x as usize;
                    if src_idx >= military_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    military[dst_idx] = military_samples[src_idx];
                }
            }
        }

        let mut crisis = vec![0.0f32; total];
        if crisis_width > 0 && crisis_height > 0 && !crisis_samples.is_empty() {
            for y in 0..crisis_height {
                for x in 0..crisis_width {
                    let src_idx = (y as usize) * (crisis_width as usize) + x as usize;
                    if src_idx >= crisis_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    crisis[dst_idx] = crisis_samples[src_idx];
                }
            }
        }

        let mut elevation = vec![0.0f32; total];
        if elevation_width > 0 && elevation_height > 0 && !elevation_samples.is_empty() {
            for y in 0..elevation_height {
                for x in 0..elevation_width {
                    let src_idx = (y as usize) * (elevation_width as usize) + x as usize;
                    if src_idx >= elevation_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    elevation[dst_idx] = elevation_samples[src_idx];
                }
            }
        }

        let mut moisture = vec![0.0f32; total];
        if moisture_width > 0 && moisture_height > 0 && !moisture_samples.is_empty() {
            for y in 0..moisture_height {
                for x in 0..moisture_width {
                    let src_idx = (y as usize) * (moisture_width as usize) + x as usize;
                    if src_idx >= moisture_samples.len() {
                        break;
                    }
                    if x >= final_width || y >= final_height {
                        continue;
                    }
                    let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                    moisture[dst_idx] = moisture_samples[src_idx];
                }
            }
        }

        let terrain_ref = if terrain_types.is_empty() {
            None
        } else {
            Some(terrain_types)
        };
        let tags_ref = if terrain_tags.is_empty() {
            None
        } else {
            Some(terrain_tags)
        };

        snapshot_dict(
            tick,
            GridSize {
                width: final_width,
                height: final_height,
            },
            OverlaySlices {
                logistics: &logistics,
                sentiment: &sentiment,
                corruption: &corruption,
                fog: &fog,
                culture: &culture,
                military: &military,
                crisis: &crisis,
                elevation: &elevation,
                moisture: &moisture,
            },
            TerrainSlices {
                terrain: terrain_ref.as_deref(),
                tags: tags_ref.as_deref(),
            },
            &crisis_annotations,
            None,
            start_marker,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }
}

const TERRAIN_TAG_LABELS: &[(u16, &str)] = &[
    (1 << 0, "Water"),
    (1 << 1, "Freshwater"),
    (1 << 2, "Coastal"),
    (1 << 3, "Wetland"),
    (1 << 4, "Fertile"),
    (1 << 5, "Arid"),
    (1 << 6, "Polar"),
    (1 << 7, "Highland"),
    (1 << 8, "Volcanic"),
    (1 << 9, "Hazardous"),
    (1 << 10, "Subsurface"),
    (1 << 11, "Hydrothermal"),
];

const CULTURE_AXIS_KEYS: [&str; 15] = [
    "PassiveAggressive",
    "OpenClosed",
    "CollectivistIndividualist",
    "TraditionalistRevisionist",
    "HierarchicalEgalitarian",
    "SyncreticPurist",
    "AsceticIndulgent",
    "PragmaticIdealistic",
    "RationalistMystical",
    "ExpansionistInsular",
    "AdaptiveStubborn",
    "HonorBoundOpportunistic",
    "MeritOrientedLineageOriented",
    "SecularDevout",
    "PluralisticMonocultural",
];

const CULTURE_AXIS_LABELS: [&str; 15] = [
    "Passive ↔ Aggressive",
    "Open ↔ Closed",
    "Collectivist ↔ Individualist",
    "Traditionalist ↔ Revisionist",
    "Hierarchical ↔ Egalitarian",
    "Syncretic ↔ Purist",
    "Ascetic ↔ Indulgent",
    "Pragmatic ↔ Idealistic",
    "Rationalist ↔ Mystical",
    "Expansionist ↔ Insular",
    "Adaptive ↔ Stubborn",
    "Honor-Bound ↔ Opportunistic",
    "Merit ↔ Lineage",
    "Secular ↔ Devout",
    "Pluralistic ↔ Monocultural",
];

const CULTURE_SCOPE_LABELS: [&str; 3] = ["Global", "Regional", "Local"];
const CULTURE_TENSION_LABELS: [&str; 3] = ["Drift Warning", "Assimilation Push", "Schism Risk"];

fn snapshot_to_dict(snapshot: fb::WorldSnapshot<'_>) -> Dictionary {
    let header = snapshot.header().unwrap();

    let mut logistics_grid: Vec<f32> = Vec::new();
    let mut logistics_dims = (0u32, 0u32);
    let mut corruption_grid: Vec<f32> = Vec::new();
    let mut corruption_dims = (0u32, 0u32);
    let mut fog_grid: Vec<f32> = Vec::new();
    let mut fog_dims = (0u32, 0u32);
    let mut culture_grid: Vec<f32> = Vec::new();
    let mut culture_dims = (0u32, 0u32);
    let mut military_grid: Vec<f32> = Vec::new();
    let mut military_dims = (0u32, 0u32);
    let mut crisis_grid: Vec<f32> = Vec::new();
    let mut crisis_dims = (0u32, 0u32);
    let mut elevation_grid: Vec<f32> = Vec::new();
    let mut elevation_dims = (0u32, 0u32);
    let mut moisture_grid: Vec<f32> = Vec::new();
    let mut moisture_dims = (0u32, 0u32);
    let mut crisis_annotations: Vec<CrisisAnnotationRecord> = Vec::new();
    if let Some(raster) = snapshot.logisticsRaster() {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            logistics_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    logistics_grid[idx] = fixed64_to_f32(value);
                }
            }
            logistics_dims = (width, height);
        }
    }

    if logistics_grid.is_empty() {
        let mut width = 0u32;
        let mut height = 0u32;
        let mut fallback: HashMap<(u32, u32), f32> = HashMap::new();
        if let Some(tiles) = snapshot.tiles() {
            for tile in tiles {
                let x = tile.x();
                let y = tile.y();
                width = width.max(x + 1);
                height = height.max(y + 1);
                fallback.insert((x, y), fixed64_to_f32(tile.temperature()));
            }
        }
        let width = width.max(1);
        let height = height.max(1);
        let total = (width as usize).saturating_mul(height as usize);
        logistics_grid = vec![0.0f32; total];
        for ((x, y), value) in fallback.into_iter() {
            if x >= width || y >= height {
                continue;
            }
            let idx = (y as usize) * (width as usize) + x as usize;
            logistics_grid[idx] = value;
        }
        logistics_dims = (width, height);
    }

    let mut terrain_width = 0u32;
    let mut terrain_height = 0u32;
    let mut terrain_samples: Vec<(u16, u16)> = Vec::new();
    if let Some(layer) = snapshot.terrainOverlay() {
        terrain_width = layer.width();
        terrain_height = layer.height();
        if let Some(samples) = layer.samples() {
            terrain_samples.reserve(samples.len());
            for sample in samples {
                terrain_samples.push((sample.terrain().0, sample.tags()));
            }
        }
    }

    if let Some(raster) = snapshot.corruptionRaster() {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            corruption_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    corruption_grid[idx] = fixed64_to_f32(value);
                }
            }
            corruption_dims = (width, height);
        }
    }

    if corruption_grid.is_empty() {
        let fallback_width = logistics_dims.0.max(terrain_width).max(1);
        let fallback_height = logistics_dims.1.max(terrain_height).max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        corruption_grid = vec![0.0f32; total];
        corruption_dims = (fallback_width, fallback_height);
    }

    let mut sentiment_grid: Vec<f32> = Vec::new();
    let mut sentiment_dims = (0u32, 0u32);
    if let Some(raster) = snapshot.sentimentRaster() {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            sentiment_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    sentiment_grid[idx] = fixed64_to_f32(value);
                }
            }
            sentiment_dims = (width, height);
        }
    }

    if sentiment_grid.is_empty() {
        let fallback_width = if logistics_dims.0 > 0 {
            logistics_dims.0
        } else if terrain_width > 0 {
            terrain_width
        } else {
            1
        };
        let fallback_height = if logistics_dims.1 > 0 {
            logistics_dims.1
        } else if terrain_height > 0 {
            terrain_height
        } else {
            1
        };
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        sentiment_grid = vec![0.0f32; total];
        sentiment_dims = (fallback_width, fallback_height);
    }

    if let Some(raster) = snapshot.fogRaster() {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            fog_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    fog_grid[idx] = fixed64_to_f32(value);
                }
            }
            fog_dims = (width, height);
        }
    }

    if fog_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(corruption_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(corruption_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        fog_grid = vec![0.0f32; total];
        fog_dims = (fallback_width, fallback_height);
    }

    if let Some(raster) = snapshot.cultureRaster() {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            culture_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    culture_grid[idx] = fixed64_to_f32(value);
                }
            }
            culture_dims = (width, height);
        }
    }

    if culture_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(terrain_width)
            .max(corruption_dims.0)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(terrain_height)
            .max(corruption_dims.1)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        culture_grid = vec![0.0f32; total];
        culture_dims = (fallback_width, fallback_height);
    }

    if let Some(raster) = snapshot.militaryRaster() {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            military_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    military_grid[idx] = fixed64_to_f32(value);
                }
            }
            military_dims = (width, height);
        }
    }

    if let Some(overlay) = snapshot.crisisOverlay() {
        if let Some(raster) = overlay.heatmap() {
            let width = raster.width();
            let height = raster.height();
            if width > 0 && height > 0 {
                let total = (width as usize).saturating_mul(height as usize);
                crisis_grid = vec![0.0f32; total];
                if let Some(samples) = raster.samples() {
                    for (idx, value) in samples.iter().enumerate() {
                        if idx >= total {
                            break;
                        }
                        crisis_grid[idx] = fixed64_to_f32(value);
                    }
                }
                crisis_dims = (width, height);
            }
        }
        if let Some(entries) = overlay.annotations() {
            for annotation in entries {
                let mut record = CrisisAnnotationRecord {
                    label: annotation.label().map(|value| value.to_string()),
                    severity: annotation.severity(),
                    path: Vec::new(),
                };
                if let Some(path) = annotation.path() {
                    record.path.reserve(path.len());
                    for value in path {
                        record.path.push(value as i32);
                    }
                }
                crisis_annotations.push(record);
            }
        }
    }

    if let Some(raster) = snapshot.moistureRaster() {
        let width = raster.width();
        let height = raster.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            moisture_grid = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    moisture_grid[idx] = value;
                }
            }
            moisture_dims = (width, height);
        }
    }

    if let Some(overlay) = snapshot.elevationOverlay() {
        let width = overlay.width();
        let height = overlay.height();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            elevation_grid = vec![0.0f32; total];
            if let Some(samples) = overlay.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    elevation_grid[idx] = (value as f32) / 255.0;
                }
            }
            elevation_dims = (width, height);
        }
    }

    if military_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(culture_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(culture_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        military_grid = vec![0.0f32; total];
        military_dims = (fallback_width, fallback_height);
    }

    if crisis_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(military_dims.0)
            .max(culture_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(military_dims.1)
            .max(culture_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        crisis_grid = vec![0.0f32; total];
        crisis_dims = (fallback_width, fallback_height);
    }

    if elevation_grid.is_empty() {
        let fallback_width = logistics_dims
            .0
            .max(sentiment_dims.0)
            .max(corruption_dims.0)
            .max(terrain_width)
            .max(1);
        let fallback_height = logistics_dims
            .1
            .max(sentiment_dims.1)
            .max(corruption_dims.1)
            .max(terrain_height)
            .max(1);
        let total = (fallback_width as usize)
            .saturating_mul(fallback_height as usize)
            .max(1);
        elevation_grid = vec![0.0f32; total];
        elevation_dims = (fallback_width, fallback_height);
    }

    let final_width = logistics_dims
        .0
        .max(sentiment_dims.0)
        .max(terrain_width)
        .max(corruption_dims.0)
        .max(fog_dims.0)
        .max(culture_dims.0)
        .max(military_dims.0)
        .max(crisis_dims.0)
        .max(elevation_dims.0)
        .max(moisture_dims.0)
        .max(1);
    let final_height = logistics_dims
        .1
        .max(sentiment_dims.1)
        .max(terrain_height)
        .max(corruption_dims.1)
        .max(fog_dims.1)
        .max(culture_dims.1)
        .max(military_dims.1)
        .max(crisis_dims.1)
        .max(elevation_dims.1)
        .max(moisture_dims.1)
        .max(1);
    let total = (final_width as usize)
        .saturating_mul(final_height as usize)
        .max(1);

    let mut logistics_resized = vec![0.0f32; total];
    if logistics_dims.0 > 0 && logistics_dims.1 > 0 {
        for y in 0..logistics_dims.1 {
            for x in 0..logistics_dims.0 {
                let src_idx = (y as usize) * (logistics_dims.0 as usize) + x as usize;
                if src_idx >= logistics_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                logistics_resized[dst_idx] = logistics_grid[src_idx];
            }
        }
    }

    let mut sentiment_resized = vec![0.0f32; total];
    if sentiment_dims.0 > 0 && sentiment_dims.1 > 0 {
        for y in 0..sentiment_dims.1 {
            for x in 0..sentiment_dims.0 {
                let src_idx = (y as usize) * (sentiment_dims.0 as usize) + x as usize;
                if src_idx >= sentiment_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                sentiment_resized[dst_idx] = sentiment_grid[src_idx];
            }
        }
    }

    let mut corruption_resized = vec![0.0f32; total];
    if corruption_dims.0 > 0 && corruption_dims.1 > 0 {
        for y in 0..corruption_dims.1 {
            for x in 0..corruption_dims.0 {
                let src_idx = (y as usize) * (corruption_dims.0 as usize) + x as usize;
                if src_idx >= corruption_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                corruption_resized[dst_idx] = corruption_grid[src_idx];
            }
        }
    }

    let mut fog_resized = vec![0.0f32; total];
    if fog_dims.0 > 0 && fog_dims.1 > 0 {
        for y in 0..fog_dims.1 {
            for x in 0..fog_dims.0 {
                let src_idx = (y as usize) * (fog_dims.0 as usize) + x as usize;
                if src_idx >= fog_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                fog_resized[dst_idx] = fog_grid[src_idx];
            }
        }
    }

    let mut culture_resized = vec![0.0f32; total];
    if culture_dims.0 > 0 && culture_dims.1 > 0 {
        for y in 0..culture_dims.1 {
            for x in 0..culture_dims.0 {
                let src_idx = (y as usize) * (culture_dims.0 as usize) + x as usize;
                if src_idx >= culture_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                culture_resized[dst_idx] = culture_grid[src_idx];
            }
        }
    }

    let mut military_resized = vec![0.0f32; total];
    if military_dims.0 > 0 && military_dims.1 > 0 {
        for y in 0..military_dims.1 {
            for x in 0..military_dims.0 {
                let src_idx = (y as usize) * (military_dims.0 as usize) + x as usize;
                if src_idx >= military_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                military_resized[dst_idx] = military_grid[src_idx];
            }
        }
    }

    let mut crisis_resized = vec![0.0f32; total];
    if crisis_dims.0 > 0 && crisis_dims.1 > 0 {
        for y in 0..crisis_dims.1 {
            for x in 0..crisis_dims.0 {
                let src_idx = (y as usize) * (crisis_dims.0 as usize) + x as usize;
                if src_idx >= crisis_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                crisis_resized[dst_idx] = crisis_grid[src_idx];
            }
        }
    }

    let mut elevation_resized = vec![0.0f32; total];
    if elevation_dims.0 > 0 && elevation_dims.1 > 0 {
        for y in 0..elevation_dims.1 {
            for x in 0..elevation_dims.0 {
                let src_idx = (y as usize) * (elevation_dims.0 as usize) + x as usize;
                if src_idx >= elevation_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                elevation_resized[dst_idx] = elevation_grid[src_idx];
            }
        }
    }
    let mut moisture_resized = vec![0.0f32; total];
    if moisture_dims.0 > 0 && moisture_dims.1 > 0 {
        for y in 0..moisture_dims.1 {
            for x in 0..moisture_dims.0 {
                let src_idx = (y as usize) * (moisture_dims.0 as usize) + x as usize;
                if src_idx >= moisture_grid.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                moisture_resized[dst_idx] = moisture_grid[src_idx];
            }
        }
    }

    let mut terrain_vec: Vec<u16> = Vec::new();
    let mut tag_vec: Vec<u16> = Vec::new();
    if terrain_width > 0 && terrain_height > 0 && !terrain_samples.is_empty() {
        terrain_vec = vec![0u16; total];
        tag_vec = vec![0u16; total];
        for y in 0..terrain_height {
            for x in 0..terrain_width {
                let src_idx = (y as usize) * (terrain_width as usize) + x as usize;
                if src_idx >= terrain_samples.len() {
                    break;
                }
                if x >= final_width || y >= final_height {
                    continue;
                }
                let dst_idx = (y as usize) * (final_width as usize) + x as usize;
                let (terrain, tags) = terrain_samples[src_idx];
                terrain_vec[dst_idx] = terrain;
                tag_vec[dst_idx] = tags;
            }
        }
    }

    let terrain_slice = if terrain_vec.is_empty() {
        None
    } else {
        Some(terrain_vec.as_slice())
    };
    let terrain_tag_slice = if tag_vec.is_empty() {
        None
    } else {
        Some(tag_vec.as_slice())
    };

    // Construct hydrology rivers array for overlays.
    let mut hydrology_rivers = VariantArray::new();
    if let Some(hydro) = snapshot.hydrologyOverlay() {
        if let Some(rivers) = hydro.rivers() {
            for river in rivers {
                let mut points_array = VariantArray::new();
                if let Some(points) = river.points() {
                    for p in points {
                        let mut pt = Dictionary::new();
                        let _ = pt.insert("x", p.x() as i64);
                        let _ = pt.insert("y", p.y() as i64);
                        let variant = pt.to_variant();
                        points_array.push(&variant);
                    }
                }
                let mut rdict = Dictionary::new();
                let _ = rdict.insert("id", river.id() as i64);
                let _ = rdict.insert("order", river.order() as i64);
                let _ = rdict.insert("width", river.width() as i64);
                let _ = rdict.insert("points", points_array);
                let river_variant = rdict.to_variant();
                hydrology_rivers.push(&river_variant);
            }
        }
    }

    let start_marker_tuple = snapshot
        .startMarker()
        .map(|marker| (marker.x(), marker.y()));

    let campaign_label_dict = header.campaignLabel().map(campaign_label_to_dict);
    let mut campaign_profiles_array: Option<VariantArray> = None;
    if let Some(profiles) = snapshot.campaignProfiles() {
        let mut arr = VariantArray::new();
        for profile in profiles {
            let dict = campaign_profile_to_dict(profile);
            arr.push(&dict.to_variant());
        }
        if !arr.is_empty() {
            campaign_profiles_array = Some(arr);
        }
    }
    let victory_dict = snapshot.victory().map(victory_state_to_dict);
    let faction_inventory_array = snapshot.factionInventory().map(faction_inventory_to_array);
    let herds_array = snapshot.herds().map(herds_to_array);

    let mut dict = snapshot_dict(
        header.tick(),
        GridSize {
            width: final_width,
            height: final_height,
        },
        OverlaySlices {
            logistics: &logistics_resized,
            sentiment: &sentiment_resized,
            corruption: &corruption_resized,
            fog: &fog_resized,
            culture: &culture_resized,
            military: &military_resized,
            crisis: &crisis_resized,
            elevation: &elevation_resized,
            moisture: &moisture_resized,
        },
        TerrainSlices {
            terrain: terrain_slice,
            tags: terrain_tag_slice,
        },
        &crisis_annotations,
        if hydrology_rivers.is_empty() {
            None
        } else {
            Some(&hydrology_rivers)
        },
        start_marker_tuple,
        campaign_label_dict,
        campaign_profiles_array,
        victory_dict,
        faction_inventory_array,
        snapshot.commandEvents().map(command_events_to_array),
        herds_array,
        snapshot.foodModules().map(food_modules_to_array),
    );

    if let Some(axis_bias) = snapshot.axisBias() {
        let _ = dict.insert("axis_bias", axis_bias_to_dict(axis_bias));
    }

    if let Some(sentiment) = snapshot.sentiment() {
        let _ = dict.insert("sentiment", sentiment_to_dict(sentiment));
    }

    if let Some(influencers) = snapshot.influencers() {
        let _ = dict.insert("influencers", influencers_to_array(influencers));
    }

    if let Some(ledger) = snapshot.corruption() {
        let _ = dict.insert("corruption", corruption_to_dict(ledger));
    }

    if let Some(populations) = snapshot.populations() {
        let _ = dict.insert("populations", populations_to_array(populations));
    }

    if let Some(power_nodes) = snapshot.power() {
        let _ = dict.insert("power_nodes", power_nodes_to_array(power_nodes));
    }

    if let Some(power_metrics) = snapshot.powerMetrics() {
        let _ = dict.insert("power_metrics", power_metrics_to_dict(power_metrics));
    }

    if let Some(crisis) = snapshot.crisisTelemetry() {
        let _ = dict.insert("crisis_telemetry", crisis_telemetry_to_dict(crisis));
    }

    if let Some(crisis_overlay) = snapshot.crisisOverlay() {
        let _ = dict.insert("crisis_overlay", crisis_overlay_to_dict(crisis_overlay));
    }

    if let Some(trade_links) = snapshot.tradeLinks() {
        let _ = dict.insert("trade_links", trade_links_to_array(trade_links));
    }

    if let Some(definitions) = snapshot.greatDiscoveryDefinitions() {
        let _ = dict.insert(
            "great_discovery_definitions",
            great_discovery_definitions_to_array(definitions),
        );
    }

    if let Some(great_discoveries) = snapshot.greatDiscoveries() {
        let _ = dict.insert(
            "great_discoveries",
            great_discovery_states_to_array(great_discoveries),
        );
    }

    if let Some(great_progress) = snapshot.greatDiscoveryProgress() {
        let _ = dict.insert(
            "great_discovery_progress",
            great_discovery_progress_states_to_array(great_progress),
        );
    }

    if let Some(gd_telemetry) = snapshot.greatDiscoveryTelemetry() {
        let _ = dict.insert(
            "great_discovery_telemetry",
            great_discovery_telemetry_to_dict(gd_telemetry),
        );
    }

    if let Some(tiles_fb) = snapshot.tiles() {
        let _ = dict.insert("tiles", tiles_to_array(tiles_fb));
    }

    if let Some(generations) = snapshot.generations() {
        let _ = dict.insert("generations", generations_to_array(generations));
    }

    if let Some(layers) = snapshot.cultureLayers() {
        let _ = dict.insert("culture_layers", culture_layers_to_array(layers));
    }

    if let Some(tensions) = snapshot.cultureTensions() {
        let _ = dict.insert("culture_tensions", culture_tensions_to_array(tensions));
    }

    if let Some(progress) = snapshot.discoveryProgress() {
        let _ = dict.insert("discovery_progress", discovery_progress_to_array(progress));
    }

    dict
}

fn campaign_label_to_dict(label: fb::CampaignLabel<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    if let Some(profile_id) = label.profileId() {
        let _ = dict.insert("profile_id", profile_id);
    }
    if let Some(title) = label.title() {
        let _ = dict.insert("title", title);
    }
    if let Some(loc_key) = label.titleLocKey() {
        let _ = dict.insert("title_loc_key", loc_key);
    }
    if let Some(subtitle) = label.subtitle() {
        let _ = dict.insert("subtitle", subtitle);
    }
    if let Some(loc_key) = label.subtitleLocKey() {
        let _ = dict.insert("subtitle_loc_key", loc_key);
    }
    dict
}

fn campaign_profile_to_dict(profile: fb::CampaignProfile<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    if let Some(id) = profile.id() {
        let _ = dict.insert("id", id);
    }
    if let Some(title) = profile.title() {
        let _ = dict.insert("title", title);
    }
    if let Some(loc_key) = profile.titleLocKey() {
        let _ = dict.insert("title_loc_key", loc_key);
    }
    if let Some(subtitle) = profile.subtitle() {
        let _ = dict.insert("subtitle", subtitle);
    }
    if let Some(loc_key) = profile.subtitleLocKey() {
        let _ = dict.insert("subtitle_loc_key", loc_key);
    }
    if let Some(units) = profile.startingUnits() {
        let units_array = campaign_starting_units_to_array(units);
        if !units_array.is_empty() {
            let _ = dict.insert("starting_units", units_array);
        }
    }
    if let Some(entries) = profile.inventory() {
        let inventory_array = campaign_inventory_to_array(entries);
        if !inventory_array.is_empty() {
            let _ = dict.insert("inventory", inventory_array);
        }
    }
    if let Some(tags) = profile.knowledgeTags() {
        let tag_array = strings_to_variant_array(tags);
        if !tag_array.is_empty() {
            let _ = dict.insert("knowledge_tags", tag_array);
        }
    }
    let radius = profile.surveyRadius();
    if radius > 0 {
        let _ = dict.insert("survey_radius", radius as i64);
    }
    if let Some(mode) = profile.fogMode() {
        if !mode.is_empty() {
            let _ = dict.insert("fog_mode", mode);
        }
    }
    if let Some(primary) = profile.primaryFoodModule() {
        if !primary.is_empty() {
            let _ = dict.insert("primary_food_module", primary);
        }
    }
    if let Some(secondary) = profile.secondaryFoodModule() {
        if !secondary.is_empty() {
            let _ = dict.insert("secondary_food_module", secondary);
        }
    }
    dict
}

fn campaign_starting_units_to_array(
    units: Vector<'_, ForwardsUOffset<fb::CampaignStartingUnit<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for unit in units {
        let mut dict = Dictionary::new();
        if let Some(kind) = unit.kind() {
            let _ = dict.insert("kind", kind);
        }
        let _ = dict.insert("count", unit.count() as i64);
        if let Some(tags) = unit.tags() {
            let tag_array = strings_to_variant_array(tags);
            if !tag_array.is_empty() {
                let _ = dict.insert("tags", tag_array);
            }
        }
        array.push(&dict.to_variant());
    }
    array
}

fn campaign_inventory_to_array(
    inventory: Vector<'_, ForwardsUOffset<fb::CampaignInventoryEntry<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for entry in inventory {
        let mut dict = Dictionary::new();
        if let Some(item) = entry.item() {
            let _ = dict.insert("item", item);
        }
        let _ = dict.insert("quantity", entry.quantity());
        array.push(&dict.to_variant());
    }
    array
}

fn faction_inventory_entries_to_array(
    entries: Vector<'_, ForwardsUOffset<fb::FactionInventoryEntry<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for entry in entries {
        let mut dict = Dictionary::new();
        if let Some(item) = entry.item() {
            let _ = dict.insert("item", item);
        }
        let _ = dict.insert("quantity", entry.quantity());
        array.push(&dict.to_variant());
    }
    array
}

fn faction_inventory_to_array(
    inventory: Vector<'_, ForwardsUOffset<fb::FactionInventoryState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for state in inventory {
        let mut dict = Dictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        if let Some(entries) = state.inventory() {
            let entry_array = faction_inventory_entries_to_array(entries);
            if !entry_array.is_empty() {
                let _ = dict.insert("inventory", entry_array);
            }
        }
        array.push(&dict.to_variant());
    }
    array
}

fn herds_to_array(herds: Vector<'_, ForwardsUOffset<fb::HerdTelemetryState<'_>>>) -> VariantArray {
    let mut array = VariantArray::new();
    for herd in herds {
        let mut dict = Dictionary::new();
        if let Some(id) = herd.id() {
            let _ = dict.insert("id", id);
        }
        if let Some(label) = herd.label() {
            let _ = dict.insert("label", label);
        }
        if let Some(species) = herd.species() {
            let _ = dict.insert("species", species);
        }
        let _ = dict.insert("x", herd.x() as i64);
        let _ = dict.insert("y", herd.y() as i64);
        let _ = dict.insert("biomass", herd.biomass());
        let _ = dict.insert("route_length", herd.routeLength() as i64);
        array.push(&dict.to_variant());
    }
    array
}

fn food_modules_to_array(
    modules: Vector<'_, ForwardsUOffset<fb::FoodModuleState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for module in modules {
        let mut dict = Dictionary::new();
        let _ = dict.insert("x", module.x() as i64);
        let _ = dict.insert("y", module.y() as i64);
        if let Some(label) = module.module() {
            let _ = dict.insert("module", label);
        }
        let _ = dict.insert("seasonal_weight", module.seasonalWeight());
        if let Some(kind) = module.kind() {
            let _ = dict.insert("kind", kind);
        }
        array.push(&dict.to_variant());
    }
    array
}

fn command_events_to_array(
    events: Vector<'_, ForwardsUOffset<fb::CommandEventState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for event in events {
        let mut dict = Dictionary::new();
        let _ = dict.insert("tick", event.tick() as i64);
        if let Some(kind) = event.kind() {
            let _ = dict.insert("kind", kind);
        }
        let _ = dict.insert("faction", event.faction() as i64);
        if let Some(label) = event.label() {
            let _ = dict.insert("label", label);
        }
        if let Some(detail) = event.detail() {
            let _ = dict.insert("detail", detail);
        }
        array.push(&dict.to_variant());
    }
    array
}

fn strings_to_variant_array(values: Vector<'_, ForwardsUOffset<&'_ str>>) -> VariantArray {
    let mut array = VariantArray::new();
    for value in values {
        array.push(&value.to_variant());
    }
    array
}

fn victory_state_to_dict(state: fb::VictoryState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let mut modes_array = VariantArray::new();
    let winner_mode_id = state
        .winner()
        .and_then(|winner| winner.mode())
        .map(|raw| raw.to_owned());
    let mut winner_label: Option<String> = None;

    if let Some(modes) = state.modes() {
        for mode in modes {
            let id = mode.id().unwrap_or("");
            let kind = mode.kind().unwrap_or("");
            let label_source = if id.is_empty() { kind } else { id };
            let label_text = format_victory_label(label_source);
            let kind_label = format_victory_label(kind);
            let progress = mode.progress();
            let threshold = mode.threshold();
            let mut progress_pct = 0.0f32;
            if threshold > f32::EPSILON {
                progress_pct = (progress / threshold).clamp(0.0, 1.0);
            }

            let mut entry = Dictionary::new();
            let _ = entry.insert("id", id);
            let _ = entry.insert("kind", kind);
            let _ = entry.insert("label", label_text.clone());
            let _ = entry.insert("kind_label", kind_label);
            let _ = entry.insert("progress", f64::from(progress));
            let _ = entry.insert("threshold", f64::from(threshold));
            let _ = entry.insert("achieved", mode.achieved());
            let _ = entry.insert("progress_pct", f64::from(progress_pct));
            modes_array.push(&entry.to_variant());

            if let Some(target) = winner_mode_id.as_ref() {
                if !target.is_empty() && target == id {
                    winner_label = Some(label_text);
                }
            }
        }
    }

    if !modes_array.is_empty() {
        let _ = dict.insert("modes", modes_array);
    }

    if let Some(winner) = state.winner() {
        let mut winner_dict = Dictionary::new();
        if let Some(mode) = winner.mode() {
            let _ = winner_dict.insert("mode", mode);
        }
        let label_text = winner_label
            .or_else(|| winner.mode().map(format_victory_label))
            .unwrap_or_else(|| "Victory".to_string());
        let _ = winner_dict.insert("label", label_text);
        let _ = winner_dict.insert("faction", winner.faction() as i64);
        let _ = winner_dict.insert("tick", winner.tick() as i64);
        let _ = dict.insert("winner", winner_dict);
    }

    dict
}

fn format_victory_label(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    raw.split(['_', '-', '.'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            let mut formatted = String::new();
            if let Some(first) = chars.next() {
                formatted.extend(first.to_uppercase());
            }
            formatted.extend(chars.flat_map(|c| c.to_lowercase()));
            formatted
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn terrain_label_from_id(id: u16) -> &'static str {
    match id {
        0 => "Deep Ocean",
        1 => "Continental Shelf",
        2 => "Inland Sea",
        3 => "Coral Shelf",
        4 => "Hydrothermal Vent Field",
        5 => "Tidal Flat",
        6 => "River Delta",
        7 => "Mangrove Swamp",
        8 => "Freshwater Marsh",
        9 => "Floodplain",
        10 => "Alluvial Plain",
        11 => "Prairie Steppe",
        12 => "Mixed Woodland",
        13 => "Boreal Taiga",
        14 => "Peatland/Heath",
        15 => "Hot Desert Erg",
        16 => "Rocky Reg Desert",
        17 => "Semi-Arid Scrub",
        18 => "Salt Flat",
        19 => "Oasis Basin",
        20 => "Tundra",
        21 => "Periglacial Steppe",
        22 => "Glacier",
        23 => "Seasonal Snowfield",
        24 => "Rolling Hills",
        25 => "High Plateau",
        26 => "Alpine Mountain",
        27 => "Karst Highland",
        28 => "Canyon Badlands",
        29 => "Active Volcano Slope",
        30 => "Basaltic Lava Field",
        31 => "Ash Plain",
        32 => "Fumarole Basin",
        33 => "Impact Crater Field",
        34 => "Karst Cavern Mouth",
        35 => "Sinkhole Field",
        36 => "Aquifer Ceiling",
        _ => "Unknown",
    }
}

fn fixed64_to_f32(value: i64) -> f32 {
    (value as f32) / 1_000_000.0
}

fn fixed64_to_f64(value: i64) -> f64 {
    (value as f64) / 1_000_000.0
}

fn normalize_overlay(values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &v in values.iter() {
        if !v.is_finite() {
            continue;
        }
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    if !min.is_finite() || !max.is_finite() || (max - min).abs() < f32::EPSILON {
        values.fill(0.0);
        return;
    }
    let range = max - min;
    for v in values.iter_mut() {
        if v.is_finite() {
            *v = ((*v - min) / range).clamp(0.0, 1.0);
        } else {
            *v = 0.0;
        }
    }
}

fn knowledge_field_label(field: fb::KnowledgeField) -> &'static str {
    match field {
        fb::KnowledgeField::Physics => "Physics",
        fb::KnowledgeField::Chemistry => "Chemistry",
        fb::KnowledgeField::Biology => "Biology",
        fb::KnowledgeField::Data => "Data",
        fb::KnowledgeField::Communication => "Communication",
        fb::KnowledgeField::Exotic => "Exotic",
        _ => "Unknown",
    }
}

fn great_discovery_effect_labels(mask: u32) -> PackedStringArray {
    let mut labels = PackedStringArray::new();
    if mask & (1 << 0) != 0 {
        labels.push(&GString::from("Power"));
    }
    if mask & (1 << 1) != 0 {
        labels.push(&GString::from("Crisis"));
    }
    if mask & (1 << 2) != 0 {
        labels.push(&GString::from("Diplomacy"));
    }
    if mask & (1 << 3) != 0 {
        labels.push(&GString::from("Forced Publication"));
    }
    labels
}

fn axis_bias_to_dict(axis: fb::AxisBiasState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("knowledge", fixed64_to_f64(axis.knowledge()));
    let _ = dict.insert("trust", fixed64_to_f64(axis.trust()));
    let _ = dict.insert("equity", fixed64_to_f64(axis.equity()));
    let _ = dict.insert("agency", fixed64_to_f64(axis.agency()));
    dict
}

fn sentiment_driver_category_label(category: fb::SentimentDriverCategory) -> &'static str {
    match category {
        fb::SentimentDriverCategory::Policy => "Policy",
        fb::SentimentDriverCategory::Incident => "Incident",
        fb::SentimentDriverCategory::Influencer => "Influencer",
        _ => "Unknown",
    }
}

fn sentiment_axis_to_dict(axis: fb::SentimentAxisTelemetry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("policy", fixed64_to_f64(axis.policy()));
    let _ = dict.insert("incidents", fixed64_to_f64(axis.incidents()));
    let _ = dict.insert("influencers", fixed64_to_f64(axis.influencers()));
    let _ = dict.insert("total", fixed64_to_f64(axis.total()));

    let mut drivers = VariantArray::new();
    if let Some(list) = axis.drivers() {
        for driver in list {
            let mut driver_dict = Dictionary::new();
            let _ = driver_dict.insert(
                "category",
                sentiment_driver_category_label(driver.category()),
            );
            let _ = driver_dict.insert("label", driver.label().unwrap_or_default());
            let _ = driver_dict.insert("value", fixed64_to_f64(driver.value()));
            let _ = driver_dict.insert("weight", fixed64_to_f64(driver.weight()));
            let variant = driver_dict.to_variant();
            drivers.push(&variant);
        }
    }
    let _ = dict.insert("drivers", drivers);
    dict
}

fn sentiment_to_dict(sentiment: fb::SentimentTelemetryState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    if let Some(axis) = sentiment.knowledge() {
        let _ = dict.insert("knowledge", sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.trust() {
        let _ = dict.insert("trust", sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.equity() {
        let _ = dict.insert("equity", sentiment_axis_to_dict(axis));
    }
    if let Some(axis) = sentiment.agency() {
        let _ = dict.insert("agency", sentiment_axis_to_dict(axis));
    }
    dict
}

fn influence_scope_label(scope: fb::InfluenceScopeKind) -> &'static str {
    match scope {
        fb::InfluenceScopeKind::Local => "Local",
        fb::InfluenceScopeKind::Regional => "Regional",
        fb::InfluenceScopeKind::Global => "Global",
        fb::InfluenceScopeKind::Generation => "Generation",
        _ => "Unknown",
    }
}

fn influence_lifecycle_label(lifecycle: fb::InfluenceLifecycle) -> &'static str {
    match lifecycle {
        fb::InfluenceLifecycle::Potential => "Potential",
        fb::InfluenceLifecycle::Active => "Active",
        fb::InfluenceLifecycle::Dormant => "Dormant",
        _ => "Unknown",
    }
}

fn influence_domain_labels(mask: u32) -> PackedStringArray {
    let mut labels = PackedStringArray::new();
    for value in 0..=4 {
        let bit = 1u32 << value;
        if mask & bit == 0 {
            continue;
        }
        let label = match value {
            0 => "Sentiment",
            1 => "Discovery",
            2 => "Logistics",
            3 => "Production",
            4 => "Humanitarian",
            _ => continue,
        };
        let gstring = GString::from(label);
        labels.push(&gstring);
    }
    labels
}

fn audience_generations_to_array(
    generations: Option<flatbuffers::Vector<'_, u16>>,
) -> PackedInt32Array {
    let mut array = PackedInt32Array::new();
    if let Some(list) = generations {
        array.resize(list.len());
        let slice = array.as_mut_slice();
        for (index, value) in list.iter().enumerate() {
            slice[index] = value as i32;
        }
    }
    array
}

fn influencer_to_dict(state: fb::InfluentialIndividualState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("id", state.id() as i64);
    let _ = dict.insert("name", state.name().unwrap_or_default());
    let _ = dict.insert("influence", fixed64_to_f64(state.influence()));
    let _ = dict.insert("growth_rate", fixed64_to_f64(state.growthRate()));
    let _ = dict.insert("baseline_growth", fixed64_to_f64(state.baselineGrowth()));
    let _ = dict.insert("notoriety", fixed64_to_f64(state.notoriety()));
    let _ = dict.insert(
        "sentiment_knowledge",
        fixed64_to_f64(state.sentimentKnowledge()),
    );
    let _ = dict.insert("sentiment_trust", fixed64_to_f64(state.sentimentTrust()));
    let _ = dict.insert("sentiment_equity", fixed64_to_f64(state.sentimentEquity()));
    let _ = dict.insert("sentiment_agency", fixed64_to_f64(state.sentimentAgency()));
    let _ = dict.insert(
        "sentiment_weight_knowledge",
        fixed64_to_f64(state.sentimentWeightKnowledge()),
    );
    let _ = dict.insert(
        "sentiment_weight_trust",
        fixed64_to_f64(state.sentimentWeightTrust()),
    );
    let _ = dict.insert(
        "sentiment_weight_equity",
        fixed64_to_f64(state.sentimentWeightEquity()),
    );
    let _ = dict.insert(
        "sentiment_weight_agency",
        fixed64_to_f64(state.sentimentWeightAgency()),
    );
    let _ = dict.insert("logistics_bonus", fixed64_to_f64(state.logisticsBonus()));
    let _ = dict.insert("morale_bonus", fixed64_to_f64(state.moraleBonus()));
    let _ = dict.insert("power_bonus", fixed64_to_f64(state.powerBonus()));
    let _ = dict.insert("logistics_weight", fixed64_to_f64(state.logisticsWeight()));
    let _ = dict.insert("morale_weight", fixed64_to_f64(state.moraleWeight()));
    let _ = dict.insert("power_weight", fixed64_to_f64(state.powerWeight()));
    let _ = dict.insert("support_charge", fixed64_to_f64(state.supportCharge()));
    let _ = dict.insert(
        "suppress_pressure",
        fixed64_to_f64(state.suppressPressure()),
    );
    let domains_mask = state.domains();
    let _ = dict.insert("domains_mask", domains_mask as i64);
    let _ = dict.insert("domains", influence_domain_labels(domains_mask));
    let _ = dict.insert("scope", influence_scope_label(state.scope()));
    let generation_scope = state.generationScope();
    if generation_scope != u16::MAX {
        let _ = dict.insert("generation_scope", generation_scope as i64);
    }
    let _ = dict.insert("supported", state.supported());
    let _ = dict.insert("suppressed", state.suppressed());
    let _ = dict.insert("lifecycle", influence_lifecycle_label(state.lifecycle()));
    let _ = dict.insert("coherence", fixed64_to_f64(state.coherence()));
    let _ = dict.insert("ticks_in_status", state.ticksInStatus() as i64);
    let audience = audience_generations_to_array(state.audienceGenerations());
    let _ = dict.insert("audience_generations", audience);
    let _ = dict.insert("support_popular", fixed64_to_f64(state.supportPopular()));
    let _ = dict.insert("support_peer", fixed64_to_f64(state.supportPeer()));
    let _ = dict.insert(
        "support_institutional",
        fixed64_to_f64(state.supportInstitutional()),
    );
    let _ = dict.insert(
        "support_humanitarian",
        fixed64_to_f64(state.supportHumanitarian()),
    );
    let _ = dict.insert("weight_popular", fixed64_to_f64(state.weightPopular()));
    let _ = dict.insert("weight_peer", fixed64_to_f64(state.weightPeer()));
    let _ = dict.insert(
        "weight_institutional",
        fixed64_to_f64(state.weightInstitutional()),
    );
    let _ = dict.insert(
        "weight_humanitarian",
        fixed64_to_f64(state.weightHumanitarian()),
    );
    if let Some(resonance) = state.cultureResonance() {
        let array = culture_resonance_to_array(resonance);
        let _ = dict.insert("culture_resonance", array);
    }
    dict
}

fn influencers_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::InfluentialIndividualState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for state in list {
        let dict = influencer_to_dict(state);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_resonance_entry_to_dict(entry: fb::InfluencerCultureResonanceEntry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let axis = entry.axis();
    let _ = dict.insert("axis", culture_axis_to_key(axis));
    let _ = dict.insert("label", culture_axis_to_label(axis));
    let _ = dict.insert("weight", fixed64_to_f64(entry.weight()));
    let _ = dict.insert("output", fixed64_to_f64(entry.output()));
    dict
}

fn culture_resonance_to_array(
    list: flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<fb::InfluencerCultureResonanceEntry<'_>>,
    >,
) -> VariantArray {
    let mut array = VariantArray::new();
    for entry in list {
        let dict = culture_resonance_entry_to_dict(entry);
        array.push(&dict.to_variant());
    }
    array
}

fn corruption_subsystem_label(subsystem: fb::CorruptionSubsystem) -> &'static str {
    match subsystem {
        fb::CorruptionSubsystem::Logistics => "Logistics",
        fb::CorruptionSubsystem::Trade => "Trade",
        fb::CorruptionSubsystem::Military => "Military",
        fb::CorruptionSubsystem::Governance => "Governance",
        _ => "Unknown",
    }
}

fn corruption_entry_to_dict(entry: fb::CorruptionEntry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("subsystem", corruption_subsystem_label(entry.subsystem()));
    let _ = dict.insert("intensity", fixed64_to_f64(entry.intensity()));
    let _ = dict.insert("incident_id", entry.incidentId() as i64);
    let _ = dict.insert("exposure_timer", entry.exposureTimer() as i64);
    let _ = dict.insert("restitution_window", entry.restitutionWindow() as i64);
    let _ = dict.insert("last_update_tick", entry.lastUpdateTick() as i64);
    dict
}

fn corruption_to_dict(ledger: fb::CorruptionLedger<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let mut entries = VariantArray::new();
    if let Some(list) = ledger.entries() {
        for entry in list {
            let dict = corruption_entry_to_dict(entry);
            let variant = dict.to_variant();
            entries.push(&variant);
        }
    }
    let _ = dict.insert("entries", entries);
    let _ = dict.insert(
        "reputation_modifier",
        fixed64_to_f64(ledger.reputationModifier()),
    );
    let _ = dict.insert("audit_capacity", ledger.auditCapacity() as i64);
    dict
}

fn population_to_dict(cohort: fb::PopulationCohortState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("entity", cohort.entity() as i64);
    let _ = dict.insert("home", cohort.home() as i64);
    let _ = dict.insert("size", cohort.size() as i64);
    let _ = dict.insert("morale", fixed64_to_f64(cohort.morale()));
    let _ = dict.insert("generation", cohort.generation() as i64);
    let _ = dict.insert("faction", cohort.faction() as i64);

    if let Some(fragments) = cohort.knowledgeFragments() {
        let mut array = VariantArray::new();
        for fragment in fragments {
            let dict = fragment_to_dict(fragment);
            array.push(&dict.to_variant());
        }
        let _ = dict.insert("knowledge_fragments", array);
    }

    if let Some(migration) = cohort.migration() {
        let mut migration_dict = Dictionary::new();
        let _ = migration_dict.insert("destination", migration.destination() as i64);
        let _ = migration_dict.insert("eta", migration.eta() as i64);
        if let Some(fragments) = migration.fragments() {
            let mut fragment_array = VariantArray::new();
            for fragment in fragments {
                let dict = fragment_to_dict(fragment);
                fragment_array.push(&dict.to_variant());
            }
            let _ = migration_dict.insert("fragments", fragment_array);
        } else {
            let _ = migration_dict.insert("fragments", VariantArray::new());
        }
        let _ = dict.insert("migration", migration_dict);
    }

    if let Some(harvest) = cohort.harvestTask() {
        let mut harvest_dict = Dictionary::new();
        if let Some(module) = harvest.module() {
            let _ = harvest_dict.insert("module", module);
        }
        if let Some(label) = harvest.bandLabel() {
            let _ = harvest_dict.insert("band_label", label);
        }
        let _ = harvest_dict.insert("target_tile", harvest.targetTile() as i64);
        let _ = harvest_dict.insert("target_x", harvest.targetX() as i64);
        let _ = harvest_dict.insert("target_y", harvest.targetY() as i64);
        let _ = harvest_dict.insert("travel_remaining", harvest.travelRemaining() as i64);
        let _ = harvest_dict.insert("travel_total", harvest.travelTotal() as i64);
        let _ = harvest_dict.insert("gather_remaining", harvest.gatherRemaining() as i64);
        let _ = harvest_dict.insert("gather_total", harvest.gatherTotal() as i64);
        let _ = harvest_dict.insert("provisions_reward", harvest.provisionsReward());
        let _ = harvest_dict.insert("trade_goods_reward", harvest.tradeGoodsReward());
        let _ = harvest_dict.insert("started_tick", harvest.startedTick() as i64);
        let _ = dict.insert("harvest", harvest_dict);
    }

    if let Some(scout) = cohort.scoutTask() {
        let mut scout_dict = Dictionary::new();
        if let Some(label) = scout.bandLabel() {
            let _ = scout_dict.insert("band_label", label);
        }
        let _ = scout_dict.insert("target_tile", scout.targetTile() as i64);
        let _ = scout_dict.insert("target_x", scout.targetX() as i64);
        let _ = scout_dict.insert("target_y", scout.targetY() as i64);
        let _ = scout_dict.insert("travel_remaining", scout.travelRemaining() as i64);
        let _ = scout_dict.insert("travel_total", scout.travelTotal() as i64);
        let _ = scout_dict.insert("reveal_radius", scout.revealRadius() as i64);
        let _ = scout_dict.insert("reveal_duration", scout.revealDuration() as i64);
        let _ = scout_dict.insert("morale_gain", scout.moraleGain());
        let _ = scout_dict.insert("started_tick", scout.startedTick() as i64);
        let _ = dict.insert("scout", scout_dict);
    }

    dict
}

fn populations_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::PopulationCohortState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for cohort in list {
        let dict = population_to_dict(cohort);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn fragment_to_dict(fragment: fb::KnownTechFragment<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("discovery", fragment.discoveryId() as i64);
    let _ = dict.insert("progress", fixed64_to_f64(fragment.progress()));
    let _ = dict.insert("progress_raw", fragment.progress());
    let _ = dict.insert("fidelity", fixed64_to_f64(fragment.fidelity()));
    dict
}

fn discovery_progress_entry_to_dict(entry: fb::DiscoveryProgressEntry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("faction", entry.faction() as i64);
    let _ = dict.insert("discovery", entry.discovery() as i64);
    let _ = dict.insert("progress", fixed64_to_f64(entry.progress()));
    let _ = dict.insert("progress_raw", entry.progress());
    dict
}

fn discovery_progress_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::DiscoveryProgressEntry<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for entry in list {
        let dict = discovery_progress_entry_to_dict(entry);
        array.push(&dict.to_variant());
    }
    array
}

fn great_discovery_state_to_dict(state: fb::GreatDiscoveryState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("id", state.id() as i64);
    let _ = dict.insert("faction", state.faction() as i64);
    let _ = dict.insert("field_label", knowledge_field_label(state.field()));
    let _ = dict.insert("tick", state.tick() as i64);
    let _ = dict.insert("publicly_deployed", state.publiclyDeployed());
    let effect_flags = state.effectFlags();
    let _ = dict.insert("effect_flags", effect_flags as i64);
    let _ = dict.insert("effects", great_discovery_effect_labels(effect_flags));
    dict
}

fn great_discovery_states_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::GreatDiscoveryState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for state in list {
        let dict = great_discovery_state_to_dict(state);
        array.push(&dict.to_variant());
    }
    array
}

fn great_discovery_requirement_definition_to_dict(
    req: fb::GreatDiscoveryRequirementDefinition<'_>,
) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("discovery_id", req.discoveryId() as i64);
    let _ = dict.insert("weight", f64::from(req.weight()));
    let _ = dict.insert("minimum_progress", f64::from(req.minimumProgress()));
    if let Some(name) = req.name() {
        let _ = dict.insert("name", GString::from(name));
    }
    if let Some(summary) = req.summary() {
        let _ = dict.insert("summary", GString::from(summary));
    }
    dict
}

fn great_discovery_requirements_to_array(
    list: flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<fb::GreatDiscoveryRequirementDefinition<'_>>,
    >,
) -> VariantArray {
    let mut array = VariantArray::new();
    for req in list {
        let dict = great_discovery_requirement_definition_to_dict(req);
        array.push(&dict.to_variant());
    }
    array
}

fn string_vector_to_packed(
    strings: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<&str>>,
) -> PackedStringArray {
    let mut array = PackedStringArray::new();
    for value in strings {
        array.push(&GString::from(value));
    }
    array
}

fn great_discovery_definition_to_dict(definition: fb::GreatDiscoveryDefinition<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("id", definition.id() as i64);
    if let Some(name) = definition.name() {
        let _ = dict.insert("name", GString::from(name));
    }
    let field = definition.field();
    let _ = dict.insert("field", GString::from(knowledge_field_label(field)));
    let _ = dict.insert(
        "observation_threshold",
        definition.observationThreshold() as i64,
    );
    let _ = dict.insert("cooldown_ticks", definition.cooldownTicks() as i64);
    if definition.hasFreshnessWindow() {
        let _ = dict.insert("freshness_window", definition.freshnessWindow() as i64);
    }
    let _ = dict.insert("effect_flags", definition.effectFlags() as i64);
    let _ = dict.insert("covert_until_public", definition.covertUntilPublic());
    if let Some(tier) = definition.tier() {
        let _ = dict.insert("tier", GString::from(tier));
    }
    if let Some(summary) = definition.summary() {
        let _ = dict.insert("summary", GString::from(summary));
    }
    if let Some(tags) = definition.tags() {
        let packed = string_vector_to_packed(tags);
        let _ = dict.insert("tags", packed);
    } else {
        let _ = dict.insert("tags", PackedStringArray::new());
    }
    if let Some(effects) = definition.effectsSummary() {
        let packed = string_vector_to_packed(effects);
        let _ = dict.insert("effects_summary", packed);
    } else {
        let _ = dict.insert("effects_summary", PackedStringArray::new());
    }
    if let Some(notes) = definition.observationNotes() {
        let _ = dict.insert("observation_notes", GString::from(notes));
    }
    if let Some(profile) = definition.leakProfile() {
        let _ = dict.insert("leak_profile", GString::from(profile));
    }
    if let Some(requirements) = definition.requirements() {
        let array = great_discovery_requirements_to_array(requirements);
        let _ = dict.insert("requirements", array);
    } else {
        let _ = dict.insert("requirements", VariantArray::new());
    }
    dict
}

fn great_discovery_definitions_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::GreatDiscoveryDefinition<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for definition in list {
        let dict = great_discovery_definition_to_dict(definition);
        array.push(&dict.to_variant());
    }
    array
}

fn great_discovery_progress_state_to_dict(
    state: fb::GreatDiscoveryProgressState<'_>,
) -> Dictionary {
    let mut dict = Dictionary::new();
    let progress_raw = state.progress();
    let _ = dict.insert("faction", state.faction() as i64);
    let _ = dict.insert("discovery", state.discovery() as i64);
    let _ = dict.insert("progress_raw", progress_raw);
    let _ = dict.insert("progress", fixed64_to_f64(progress_raw));
    let _ = dict.insert("observation_deficit", state.observationDeficit() as i64);
    let _ = dict.insert("eta_ticks", state.etaTicks() as i64);
    let _ = dict.insert("covert", state.covert());
    dict
}

fn great_discovery_progress_states_to_array(
    list: flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<fb::GreatDiscoveryProgressState<'_>>,
    >,
) -> VariantArray {
    let mut array = VariantArray::new();
    for state in list {
        let dict = great_discovery_progress_state_to_dict(state);
        array.push(&dict.to_variant());
    }
    array
}

fn great_discovery_telemetry_to_dict(state: fb::GreatDiscoveryTelemetryState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("total_resolved", state.totalResolved() as i64);
    let _ = dict.insert("pending_candidates", state.pendingCandidates() as i64);
    let _ = dict.insert("active_constellations", state.activeConstellations() as i64);
    dict
}

fn tile_to_dict(tile: fb::TileState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("entity", tile.entity() as i64);
    let _ = dict.insert("x", tile.x() as i64);
    let _ = dict.insert("y", tile.y() as i64);
    let _ = dict.insert("element", tile.element() as i64);
    let _ = dict.insert("mass", fixed64_to_f64(tile.mass()));
    let _ = dict.insert("temperature", fixed64_to_f64(tile.temperature()));
    let _ = dict.insert("terrain", tile.terrain().0 as i64);
    let _ = dict.insert("terrain_tags", tile.terrainTags() as i64);
    let _ = dict.insert("culture_layer", tile.cultureLayer() as i64);
    let _ = dict.insert("mountain_kind", i64::from(tile.mountainKind().0));
    let _ = dict.insert("mountain_relief", tile.mountainRelief());
    dict
}

fn tiles_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::TileState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for tile in list {
        let dict = tile_to_dict(tile);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn trade_link_to_dict(link: fb::TradeLinkState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("entity", link.entity() as i64);
    let _ = dict.insert("from_faction", link.fromFaction() as i64);
    let _ = dict.insert("to_faction", link.toFaction() as i64);
    let _ = dict.insert("throughput", fixed64_to_f64(link.throughput()));
    let _ = dict.insert("tariff", fixed64_to_f64(link.tariff()));
    let _ = dict.insert("from_tile", link.fromTile() as i64);
    let _ = dict.insert("to_tile", link.toTile() as i64);

    if let Some(knowledge) = link.knowledge() {
        let mut knowledge_dict = Dictionary::new();
        let _ = knowledge_dict.insert("openness", fixed64_to_f64(knowledge.openness()));
        let _ = knowledge_dict.insert("openness_raw", knowledge.openness());
        let _ = knowledge_dict.insert("leak_timer", knowledge.leakTimer() as i64);
        let _ = knowledge_dict.insert("last_discovery", knowledge.lastDiscovery() as i64);
        let _ = knowledge_dict.insert("decay", fixed64_to_f64(knowledge.decay()));
        let _ = knowledge_dict.insert("decay_raw", knowledge.decay());
        let _ = dict.insert("knowledge", knowledge_dict);
    }

    if let Some(pending) = link.pendingFragments() {
        let mut pending_array = VariantArray::new();
        for fragment in pending {
            let fragment_dict = fragment_to_dict(fragment);
            pending_array.push(&fragment_dict.to_variant());
        }
        let _ = dict.insert("pending_fragments", pending_array);
    } else {
        let _ = dict.insert("pending_fragments", VariantArray::new());
    }

    dict
}

fn trade_links_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::TradeLinkState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for link in list {
        let dict = trade_link_to_dict(link);
        array.push(&dict.to_variant());
    }
    array
}

fn power_node_to_dict(node: fb::PowerNodeState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("entity", node.entity() as i64);
    let _ = dict.insert("node_id", node.nodeId() as i64);

    let generation_raw = node.generation();
    let demand_raw = node.demand();
    let efficiency_raw = node.efficiency();
    let storage_level_raw = node.storageLevel();
    let storage_capacity_raw = node.storageCapacity();
    let stability_raw = node.stability();
    let surplus_raw = node.surplus();
    let deficit_raw = node.deficit();

    let _ = dict.insert("generation", fixed64_to_f64(generation_raw));
    let _ = dict.insert("generation_raw", generation_raw);
    let _ = dict.insert("demand", fixed64_to_f64(demand_raw));
    let _ = dict.insert("demand_raw", demand_raw);
    let _ = dict.insert("efficiency", fixed64_to_f64(efficiency_raw));
    let _ = dict.insert("efficiency_raw", efficiency_raw);
    let _ = dict.insert("storage_level", fixed64_to_f64(storage_level_raw));
    let _ = dict.insert("storage_level_raw", storage_level_raw);
    let _ = dict.insert("storage_capacity", fixed64_to_f64(storage_capacity_raw));
    let _ = dict.insert("storage_capacity_raw", storage_capacity_raw);
    let _ = dict.insert("stability", fixed64_to_f64(stability_raw));
    let _ = dict.insert("stability_raw", stability_raw);
    let _ = dict.insert("surplus", fixed64_to_f64(surplus_raw));
    let _ = dict.insert("surplus_raw", surplus_raw);
    let _ = dict.insert("deficit", fixed64_to_f64(deficit_raw));
    let _ = dict.insert("deficit_raw", deficit_raw);
    let _ = dict.insert("incident_count", node.incidentCount() as i64);

    dict
}

fn power_nodes_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::PowerNodeState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for node in list {
        let dict = power_node_to_dict(node);
        array.push(&dict.to_variant());
    }
    array
}

fn power_incident_to_dict(incident: fb::PowerIncidentState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("node_id", incident.nodeId() as i64);
    let severity = match incident.severity() {
        fb::PowerIncidentSeverity::Critical => "critical",
        _ => "warning",
    };
    let _ = dict.insert("severity", severity);
    let deficit_raw = incident.deficit();
    let _ = dict.insert("deficit", fixed64_to_f64(deficit_raw));
    let _ = dict.insert("deficit_raw", deficit_raw);
    dict
}

fn power_metrics_to_dict(metrics: fb::PowerTelemetryState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let total_supply_raw = metrics.totalSupply();
    let total_demand_raw = metrics.totalDemand();
    let total_storage_raw = metrics.totalStorage();
    let total_capacity_raw = metrics.totalCapacity();
    let _ = dict.insert("total_supply", fixed64_to_f64(total_supply_raw));
    let _ = dict.insert("total_supply_raw", total_supply_raw);
    let _ = dict.insert("total_demand", fixed64_to_f64(total_demand_raw));
    let _ = dict.insert("total_demand_raw", total_demand_raw);
    let _ = dict.insert("total_storage", fixed64_to_f64(total_storage_raw));
    let _ = dict.insert("total_storage_raw", total_storage_raw);
    let _ = dict.insert("total_capacity", fixed64_to_f64(total_capacity_raw));
    let _ = dict.insert("total_capacity_raw", total_capacity_raw);
    let _ = dict.insert("grid_stress_avg", metrics.gridStressAvg() as f64);
    let _ = dict.insert("surplus_margin", metrics.surplusMargin() as f64);
    let _ = dict.insert("instability_alerts", metrics.instabilityAlerts() as i64);

    let mut incidents_array = VariantArray::new();
    if let Some(incidents) = metrics.incidents() {
        for incident in incidents {
            let dict = power_incident_to_dict(incident);
            incidents_array.push(&dict.to_variant());
        }
    }
    let _ = dict.insert("incidents", incidents_array);

    dict
}

fn crisis_metric_kind_to_str(kind: fb::CrisisMetricKind) -> &'static str {
    match kind {
        fb::CrisisMetricKind::R0 => "r0",
        fb::CrisisMetricKind::GridStressPct => "grid_stress_pct",
        fb::CrisisMetricKind::UnauthorizedQueuePct => "unauthorized_queue_pct",
        fb::CrisisMetricKind::SwarmsActive => "swarms_active",
        fb::CrisisMetricKind::PhageDensity => "phage_density",
        _ => "unknown",
    }
}

fn crisis_metric_label(kind: fb::CrisisMetricKind) -> &'static str {
    match kind {
        fb::CrisisMetricKind::R0 => "R₀",
        fb::CrisisMetricKind::GridStressPct => "Grid Stress %",
        fb::CrisisMetricKind::UnauthorizedQueuePct => "Unauthorized Queue %",
        fb::CrisisMetricKind::SwarmsActive => "Swarms Active",
        fb::CrisisMetricKind::PhageDensity => "Phage Density",
        _ => "Metric",
    }
}

fn crisis_severity_band_to_str(band: fb::CrisisSeverityBand) -> &'static str {
    match band {
        fb::CrisisSeverityBand::Warn => "warn",
        fb::CrisisSeverityBand::Critical => "critical",
        _ => "safe",
    }
}

fn crisis_history_to_array(
    history: Vector<'_, flatbuffers::ForwardsUOffset<fb::CrisisTrendSample<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for sample in history {
        let mut entry = Dictionary::new();
        let _ = entry.insert("tick", sample.tick() as i64);
        let _ = entry.insert("value", sample.value());
        array.push(&entry.to_variant());
    }
    array
}

fn crisis_gauge_to_dict(gauge: fb::CrisisGaugeState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let kind = gauge.kind();
    let _ = dict.insert("kind", crisis_metric_kind_to_str(kind));
    let _ = dict.insert("label", crisis_metric_label(kind));
    let _ = dict.insert("raw", gauge.raw());
    let _ = dict.insert("ema", gauge.ema());
    let _ = dict.insert("trend_5t", gauge.trend5t());
    let _ = dict.insert("warn_threshold", gauge.warnThreshold());
    let _ = dict.insert("critical_threshold", gauge.criticalThreshold());
    let _ = dict.insert("last_updated_tick", gauge.lastUpdatedTick() as i64);
    let _ = dict.insert("stale_ticks", gauge.staleTicks() as i64);
    let _ = dict.insert("band", crisis_severity_band_to_str(gauge.band()));
    if let Some(history) = gauge.history() {
        let _ = dict.insert("history", crisis_history_to_array(history));
    } else {
        let _ = dict.insert("history", VariantArray::new());
    }
    dict
}

fn crisis_telemetry_to_dict(telemetry: fb::CrisisTelemetryState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let mut gauges_array = VariantArray::new();
    if let Some(gauges) = telemetry.gauges() {
        for gauge in gauges {
            let dict = crisis_gauge_to_dict(gauge);
            gauges_array.push(&dict.to_variant());
        }
    }
    let _ = dict.insert("gauges", gauges_array);
    let _ = dict.insert("modifiers_active", telemetry.modifiersActive() as i64);
    let _ = dict.insert("foreshock_incidents", telemetry.foreshockIncidents() as i64);
    let _ = dict.insert(
        "containment_incidents",
        telemetry.containmentIncidents() as i64,
    );
    let _ = dict.insert("warnings_active", telemetry.warningsActive() as i64);
    let _ = dict.insert("criticals_active", telemetry.criticalsActive() as i64);
    dict
}

fn crisis_overlay_to_dict(overlay: fb::CrisisOverlayState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let mut heatmap_dict = Dictionary::new();
    if let Some(raster) = overlay.heatmap() {
        let width = raster.width();
        let height = raster.height();
        let mut data = Vec::new();
        if width > 0 && height > 0 {
            let total = (width as usize).saturating_mul(height as usize);
            data = vec![0.0f32; total];
            if let Some(samples) = raster.samples() {
                for (idx, value) in samples.iter().enumerate() {
                    if idx >= total {
                        break;
                    }
                    data[idx] = fixed64_to_f32(value);
                }
            }
        }
        let _ = heatmap_dict.insert("width", width as i64);
        let _ = heatmap_dict.insert("height", height as i64);
        let _ = heatmap_dict.insert("samples", packed_from_slice(&data));
    } else {
        let _ = heatmap_dict.insert("width", 0);
        let _ = heatmap_dict.insert("height", 0);
        let _ = heatmap_dict.insert("samples", PackedFloat32Array::new());
    }
    let _ = dict.insert("heatmap", heatmap_dict);

    let mut annotations = VariantArray::new();
    if let Some(entries) = overlay.annotations() {
        for entry in entries {
            let mut annotation = Dictionary::new();
            if let Some(label) = entry.label() {
                let _ = annotation.insert("label", label);
            }
            let _ = annotation.insert("severity", crisis_severity_band_to_str(entry.severity()));
            if let Some(path) = entry.path() {
                let mut packed = PackedInt32Array::new();
                packed.resize(path.len());
                let slice = packed.as_mut_slice();
                for (idx, value) in path.iter().enumerate() {
                    slice[idx] = value as i32;
                }
                let _ = annotation.insert("path", packed);
            } else {
                let _ = annotation.insert("path", PackedInt32Array::new());
            }
            annotations.push(&annotation.to_variant());
        }
    }
    let _ = dict.insert("annotations", annotations);
    dict
}

fn crisis_annotation_to_dict(record: &CrisisAnnotationRecord) -> Dictionary {
    let mut dict = Dictionary::new();
    if let Some(label) = &record.label {
        let _ = dict.insert("label", label.clone());
    }
    let _ = dict.insert("severity", crisis_severity_band_to_str(record.severity));
    if record.path.is_empty() {
        let _ = dict.insert("path", PackedInt32Array::new());
    } else {
        let mut packed = PackedInt32Array::new();
        packed.resize(record.path.len());
        let slice = packed.as_mut_slice();
        slice.copy_from_slice(&record.path);
        let _ = dict.insert("path", packed);
    }
    dict
}

fn generation_to_dict(state: fb::GenerationState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let _ = dict.insert("id", state.id() as i64);
    let _ = dict.insert("name", state.name().unwrap_or_default());
    let _ = dict.insert("bias_knowledge", fixed64_to_f64(state.biasKnowledge()));
    let _ = dict.insert("bias_trust", fixed64_to_f64(state.biasTrust()));
    let _ = dict.insert("bias_equity", fixed64_to_f64(state.biasEquity()));
    let _ = dict.insert("bias_agency", fixed64_to_f64(state.biasAgency()));
    dict
}

fn culture_layer_to_dict(layer: fb::CultureLayerState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let id = layer.id();
    let scope = layer.scope();
    let scope_label = culture_scope_to_label(scope);
    let owner = layer.owner();
    let parent = layer.parent();
    let baseline = layer.divergence();
    let soft = layer.softThreshold();
    let hard = layer.hardThreshold();
    let _ = dict.insert("id", id as i64);
    let _ = dict.insert("scope", culture_scope_to_key(scope));
    let _ = dict.insert("scope_label", scope_label);
    let _ = dict.insert("owner", format!("{owner:016X}"));
    if owner <= i64::MAX as u64 {
        let _ = dict.insert("owner_value", owner as i64);
    }
    let _ = dict.insert("parent", parent as i64);
    let _ = dict.insert("divergence", fixed64_to_f64(baseline));
    let _ = dict.insert("soft_threshold", fixed64_to_f64(soft));
    let _ = dict.insert("hard_threshold", fixed64_to_f64(hard));
    let _ = dict.insert("ticks_above_soft", layer.ticksAboveSoft() as i64);
    let _ = dict.insert("ticks_above_hard", layer.ticksAboveHard() as i64);
    let _ = dict.insert("last_updated_tick", layer.lastUpdatedTick() as i64);

    let mut traits_array = VariantArray::new();
    if let Some(traits) = layer.traits() {
        for trait_entry in traits {
            let trait_dict = culture_trait_to_dict(trait_entry);
            traits_array.push(&trait_dict.to_variant());
        }
    }
    let _ = dict.insert("traits", traits_array);

    dict
}

fn culture_trait_to_dict(entry: fb::CultureTraitEntry<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let axis = entry.axis();
    let _ = dict.insert("axis", culture_axis_to_key(axis));
    let _ = dict.insert("label", culture_axis_to_label(axis));
    let _ = dict.insert("baseline", fixed64_to_f64(entry.baseline()));
    let _ = dict.insert("modifier", fixed64_to_f64(entry.modifier()));
    let _ = dict.insert("value", fixed64_to_f64(entry.value()));
    dict
}

fn culture_tension_to_dict(state: fb::CultureTensionState<'_>) -> Dictionary {
    let mut dict = Dictionary::new();
    let scope = state.scope();
    let kind = state.kind();
    let _ = dict.insert("layer_id", state.layerId() as i64);
    let _ = dict.insert("scope", culture_scope_to_key(scope));
    let _ = dict.insert("scope_label", culture_scope_to_label(scope));
    let _ = dict.insert("kind", culture_tension_to_key(kind));
    let _ = dict.insert("kind_label", culture_tension_to_label(kind));
    let _ = dict.insert("severity", fixed64_to_f64(state.severity()));
    let _ = dict.insert("timer", state.timer() as i64);
    dict
}

fn generations_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::GenerationState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for state in list {
        let dict = generation_to_dict(state);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_layers_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::CultureLayerState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for layer in list {
        let dict = culture_layer_to_dict(layer);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_tensions_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::CultureTensionState<'_>>>,
) -> VariantArray {
    let mut array = VariantArray::new();
    for tension in list {
        let dict = culture_tension_to_dict(tension);
        let variant = dict.to_variant();
        array.push(&variant);
    }
    array
}

fn culture_scope_to_key(scope: fb::CultureLayerScope) -> &'static str {
    match scope {
        fb::CultureLayerScope::Global => "Global",
        fb::CultureLayerScope::Regional => "Regional",
        fb::CultureLayerScope::Local => "Local",
        _ => "Unknown",
    }
}

fn culture_scope_to_label(scope: fb::CultureLayerScope) -> &'static str {
    match scope {
        fb::CultureLayerScope::Global => CULTURE_SCOPE_LABELS[0],
        fb::CultureLayerScope::Regional => CULTURE_SCOPE_LABELS[1],
        fb::CultureLayerScope::Local => CULTURE_SCOPE_LABELS[2],
        _ => "Unknown",
    }
}

fn culture_axis_to_key(axis: fb::CultureTraitAxis) -> &'static str {
    let idx = axis.0 as usize;
    CULTURE_AXIS_KEYS.get(idx).copied().unwrap_or("Trait")
}

fn culture_axis_to_label(axis: fb::CultureTraitAxis) -> &'static str {
    let idx = axis.0 as usize;
    CULTURE_AXIS_LABELS.get(idx).copied().unwrap_or("Trait")
}

fn culture_tension_to_key(kind: fb::CultureTensionKind) -> &'static str {
    match kind {
        fb::CultureTensionKind::DriftWarning => "DriftWarning",
        fb::CultureTensionKind::AssimilationPush => "AssimilationPush",
        fb::CultureTensionKind::SchismRisk => "SchismRisk",
        _ => "Unknown",
    }
}

fn culture_tension_to_label(kind: fb::CultureTensionKind) -> &'static str {
    match kind {
        fb::CultureTensionKind::DriftWarning => CULTURE_TENSION_LABELS[0],
        fb::CultureTensionKind::AssimilationPush => CULTURE_TENSION_LABELS[1],
        fb::CultureTensionKind::SchismRisk => CULTURE_TENSION_LABELS[2],
        _ => "Unknown",
    }
}

fn u32_vector_to_packed_int32(list: Option<flatbuffers::Vector<'_, u32>>) -> PackedInt32Array {
    let mut array = PackedInt32Array::new();
    if let Some(values) = list {
        array.resize(values.len());
        let slice = array.as_mut_slice();
        for (index, value) in values.iter().enumerate() {
            slice[index] = value as i32;
        }
    }
    array
}

fn u16_vector_to_packed_int32(list: Option<flatbuffers::Vector<'_, u16>>) -> PackedInt32Array {
    let mut array = PackedInt32Array::new();
    if let Some(values) = list {
        array.resize(values.len());
        let slice = array.as_mut_slice();
        for (index, value) in values.iter().enumerate() {
            slice[index] = value as i32;
        }
    }
    array
}

fn u64_vector_to_packed_int64(list: Option<flatbuffers::Vector<'_, u64>>) -> PackedInt64Array {
    let mut array = PackedInt64Array::new();
    if let Some(values) = list {
        array.resize(values.len());
        let slice = array.as_mut_slice();
        for (index, value) in values.iter().enumerate() {
            slice[index] = value as i64;
        }
    }
    array
}

struct ShadowScaleExtension;

#[gdextension(entry_symbol = godot_rs_shadow_scale_godot_init)]
unsafe impl ExtensionLibrary for ShadowScaleExtension {}
