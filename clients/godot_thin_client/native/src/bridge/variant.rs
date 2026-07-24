//! Godot `Variant` <-> `serde_json::Value` marshalling shared by the bridges.

use godot::prelude::*;
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use crate::runtime::ScriptError;

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

pub(crate) fn variant_to_json(value: &Variant) -> JsonValue {
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
            let array: VarArray = value.to();
            let mut result = Vec::with_capacity(array.len() as usize);
            for item in array.iter_shared() {
                result.push(variant_to_json(&item));
            }
            JsonValue::Array(result)
        }
        VariantType::DICTIONARY => {
            let dict: VarDictionary = value.to();
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

pub(crate) fn json_to_variant(value: &JsonValue) -> Variant {
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
            let mut variant_array = VarArray::new();
            for item in arr {
                let variant = json_to_variant(item);
                variant_array.push(&variant);
            }
            Variant::from(variant_array)
        }
        JsonValue::Object(map) => {
            let mut dict = VarDictionary::new();
            for (key, value) in map {
                let _ = dict.insert(key.as_str(), &json_to_variant(value));
            }
            Variant::from(dict)
        }
    }
}

pub(crate) fn json_to_variant_array(value: &JsonValue) -> VarArray {
    match json_to_variant(value).try_to::<VarArray>() {
        Ok(array) => array,
        Err(_) => VarArray::new(),
    }
}

pub(crate) fn script_error_to_dict(err: ScriptError) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("ok", false);
    let _ = dict.insert("error", err.to_string());
    dict
}
