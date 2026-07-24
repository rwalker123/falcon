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

fn trade_link_to_dict(link: fb::TradeLinkState<'_>) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("entity", link.entity() as i64);
    let _ = dict.insert("from_faction", link.fromFaction() as i64);
    let _ = dict.insert("to_faction", link.toFaction() as i64);
    let _ = dict.insert("throughput", fixed64_to_f64(link.throughput()));
    let _ = dict.insert("tariff", fixed64_to_f64(link.tariff()));
    let _ = dict.insert("from_tile", link.fromTile() as i64);
    let _ = dict.insert("to_tile", link.toTile() as i64);

    if let Some(knowledge) = link.knowledge() {
        let mut knowledge_dict = VarDictionary::new();
        let _ = knowledge_dict.insert("openness", fixed64_to_f64(knowledge.openness()));
        let _ = knowledge_dict.insert("openness_raw", knowledge.openness());
        let _ = knowledge_dict.insert("leak_timer", knowledge.leakTimer() as i64);
        let _ = knowledge_dict.insert("last_discovery", knowledge.lastDiscovery() as i64);
        let _ = knowledge_dict.insert("decay", fixed64_to_f64(knowledge.decay()));
        let _ = knowledge_dict.insert("decay_raw", knowledge.decay());
        let _ = dict.insert("knowledge", &knowledge_dict);
    }

    if let Some(pending) = link.pendingFragments() {
        let mut pending_array = VarArray::new();
        for fragment in pending {
            let fragment_dict = fragment_to_dict(fragment);
            pending_array.push(&fragment_dict.to_variant());
        }
        let _ = dict.insert("pending_fragments", &pending_array);
    } else {
        let _ = dict.insert("pending_fragments", &VarArray::new());
    }

    dict
}

pub(crate) fn trade_links_to_array(
    list: flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fb::TradeLinkState<'_>>>,
) -> VarArray {
    let mut array = VarArray::new();
    for link in list {
        let dict = trade_link_to_dict(link);
        array.push(&dict.to_variant());
    }
    array
}
