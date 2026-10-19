//! Snapshot section -> `Dictionary` converters, partitioned along the same nine
//! domain sections `sim_schema/schemas/snapshot.fbs` uses. A new snapshot field's converter
//! belongs in its section's module here; only helpers with consumers in two or more
//! sections live in this file.

pub(crate) mod campaign;
pub(crate) mod connections;
pub(crate) mod culture;
pub(crate) mod dealings;
pub(crate) mod deposits;
pub(crate) mod economy;
pub(crate) mod governance;
pub(crate) mod knowledge;
pub(crate) mod map;
pub(crate) mod population;
pub(crate) mod routes;
pub(crate) mod subsistence;

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;

pub(crate) fn strings_to_variant_array(values: Vector<'_, ForwardsUOffset<&'_ str>>) -> VarArray {
    let mut array = VarArray::new();
    for value in values {
        array.push(&value.to_variant());
    }
    array
}

/// One `MaterialBatchState` row as a dictionary, the shape `material_batches` has always had: the
/// generic material id, the amount, EVERY reading (the band is the merge key, the exact value is
/// what crafting reads) and the nearest declared variety. Shared by the cohort's store and the
/// dealings goods lists, which are the same representation and so must decode the same way.
pub(crate) fn material_batch_to_dict(
    batch: &shadow_scale_flatbuffers::shadow_scale::sim::MaterialBatchState<'_>,
) -> VarDictionary {
    let mut row = VarDictionary::new();
    // The GENERIC material id - "hide", never "deer_hide".
    let _ = row.insert("material_id", batch.materialId().unwrap_or(""));
    let _ = row.insert("amount", batch.amount() as f64);
    let mut readings = VarArray::new();
    if let Some(list) = batch.readings() {
        for reading in list.iter() {
            let mut entry = VarDictionary::new();
            let _ = entry.insert("axis", reading.axis().unwrap_or(""));
            let _ = entry.insert("value", reading.value() as f64);
            let _ = entry.insert("band_name", reading.bandName().unwrap_or(""));
            readings.push(&entry.to_variant());
        }
    }
    let _ = row.insert("readings", &readings);
    // `""` when the material declares no variety, which is every shipped material.
    let _ = row.insert("variety_name", batch.varietyName().unwrap_or(""));
    row
}

pub(crate) fn fixed64_to_f32(value: i64) -> f32 {
    (value as f32) / 1_000_000.0
}

pub(crate) fn fixed64_to_f64(value: i64) -> f64 {
    (value as f64) / 1_000_000.0
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

pub(crate) fn u32_vector_to_packed_int32(
    list: Option<flatbuffers::Vector<'_, u32>>,
) -> PackedInt32Array {
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

pub(crate) fn u16_vector_to_packed_int32(
    list: Option<flatbuffers::Vector<'_, u16>>,
) -> PackedInt32Array {
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

pub(crate) fn u64_vector_to_packed_int64(
    list: Option<flatbuffers::Vector<'_, u64>>,
) -> PackedInt64Array {
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
