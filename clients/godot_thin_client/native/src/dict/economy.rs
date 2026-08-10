//! `economy` section -- trade links, faction inventories, and known tech fragments.

use flatbuffers::{ForwardsUOffset, Vector};
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::fixed64_to_f64;

fn faction_inventory_entries_to_array(
    entries: Vector<'_, ForwardsUOffset<fb::FactionInventoryEntry<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for entry in entries {
        let mut dict = VarDictionary::new();
        if let Some(item) = entry.item() {
            let _ = dict.insert("item", item);
        }
        let _ = dict.insert("quantity", entry.quantity());
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn faction_inventory_to_array(
    inventory: Vector<'_, ForwardsUOffset<fb::FactionInventoryState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for state in inventory {
        let mut dict = VarDictionary::new();
        let _ = dict.insert("faction", state.faction() as i64);
        if let Some(entries) = state.inventory() {
            let entry_array = faction_inventory_entries_to_array(entries);
            if !entry_array.is_empty() {
                let _ = dict.insert("inventory", &entry_array);
            }
        }
        array.push(&dict.to_variant());
    }
    array
}

pub(crate) fn fragment_to_dict(fragment: fb::KnownTechFragment<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("discovery", fragment.discoveryId() as i64);
    let _ = dict.insert("progress", fixed64_to_f64(fragment.progress()));
    let _ = dict.insert("progress_raw", fragment.progress());
    let _ = dict.insert("fidelity", fixed64_to_f64(fragment.fidelity()));
    dict
}
