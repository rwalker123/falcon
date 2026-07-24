//! `ScriptHostBridge` -- the GDScript-facing wrapper around the embedded script runtime.

use godot::prelude::*;
use serde_json::json;
use sim_runtime::scripting::{ScriptManifest, SimScriptState};

use crate::bridge::command::resolve_entry_path;
use crate::bridge::variant::{
    json_to_variant, json_to_variant_array, script_error_to_dict, variant_to_json,
};
use crate::runtime::{manifest_to_json, responses_to_json, Manager as ScriptRuntimeManager};

#[derive(Default, GodotClass)]
#[class(init, base=RefCounted)]
pub struct ScriptHostBridge {
    manager: ScriptRuntimeManager,
}

#[godot_api]
impl ScriptHostBridge {
    #[func]
    pub fn parse_manifest(&self, manifest_path: GString, manifest_json: GString) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let path_str = manifest_path.to_string();
        match ScriptManifest::parse_str(manifest_json.to_string().as_str()) {
            Ok(mut manifest) => {
                let entry_path = resolve_entry_path(&path_str, &manifest.entry);
                manifest.manifest_path = Some(path_str.clone());
                let manifest_variant = json_to_variant(&manifest_to_json(&manifest));
                let _ = dict.insert("ok", true);
                let _ = dict.insert("manifest", &manifest_variant);
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
        manifest_dict: VarDictionary,
        manifest_path: GString,
        source: GString,
    ) -> VarDictionary {
        let mut dict = VarDictionary::new();
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
                            dict.insert("manifest", &json_to_variant(&manifest_to_json(&manifest)));
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
    pub fn dispatch_event(
        &self,
        script_id: i64,
        event: GString,
        payload: Variant,
    ) -> VarDictionary {
        let payload_json = variant_to_json(&payload);
        match self
            .manager
            .dispatch_event(script_id, &event.to_string(), payload_json)
        {
            Ok(_) => {
                let mut dict = VarDictionary::new();
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
    pub fn poll_responses(&self, script_id: i64) -> VarArray {
        match self.manager.poll_responses(script_id) {
            Ok(responses) => json_to_variant_array(&responses_to_json(responses)),
            Err(err) => {
                let mut array = VarArray::new();
                let variant =
                    json_to_variant(&json!({ "type": "error", "message": err.to_string() }));
                array.push(&variant);
                array
            }
        }
    }

    #[func]
    pub fn poll_all(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let map = self.manager.poll_all();
        for (id, responses) in map {
            let _ = dict.insert(id, &json_to_variant_array(&responses_to_json(responses)));
        }
        dict
    }

    #[func]
    pub fn list_scripts(&self) -> VarArray {
        let mut array = VarArray::new();
        for (id, manifest) in self.manager.list_scripts() {
            let mut entry = VarDictionary::new();
            let _ = entry.insert("script_id", id);
            let _ = entry.insert("manifest", &json_to_variant(&manifest_to_json(&manifest)));
            let variant_entry = Variant::from(entry);
            array.push(&variant_entry);
        }
        array
    }

    #[func]
    pub fn subscriptions(&self, script_id: i64) -> VarArray {
        match self.manager.subscriptions(script_id) {
            Ok(subs) => {
                let mut array = VarArray::new();
                for sub in subs {
                    let variant = Variant::from(sub.as_str());
                    array.push(&variant);
                }
                array
            }
            Err(_) => VarArray::new(),
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
    pub fn snapshot_active_scripts(&self) -> VarArray {
        let mut array = VarArray::new();
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
    pub fn apply_script_state(&self, script_id: i64, state: Variant) -> VarDictionary {
        let mut dict = VarDictionary::new();
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
