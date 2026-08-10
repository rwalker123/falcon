//! Economy-section FlatBuffers serialization.

use crate::codec::FbBuilder;
use crate::state::economy::FactionInventoryState;
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_economy_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::EconomySection<'a>> {
    let faction_inventory = create_faction_inventory(builder, &snapshot.faction_inventory);
    fb::EconomySection::create(
        builder,
        &fb::EconomySectionArgs {
            logisticsRaster: None,
            factionInventory: Some(faction_inventory),
        },
    )
}

pub(crate) fn serialize_economy_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::EconomySection<'a>> {
    let faction_inventory = delta
        .faction_inventory
        .as_ref()
        .map(|entries| create_faction_inventory(builder, entries));
    fb::EconomySection::create(
        builder,
        &fb::EconomySectionArgs {
            logisticsRaster: None,
            factionInventory: faction_inventory,
        },
    )
}

fn create_faction_inventory<'a>(
    builder: &mut FbBuilder<'a>,
    factions: &[FactionInventoryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::FactionInventoryState<'a>>>> {
    let mut entries = Vec::with_capacity(factions.len());
    for state in factions {
        let mut inventory_offsets = Vec::with_capacity(state.inventory.len());
        for entry in &state.inventory {
            let item = builder.create_string(entry.item.as_str());
            let entry_offset = fb::FactionInventoryEntry::create(
                builder,
                &fb::FactionInventoryEntryArgs {
                    item: Some(item),
                    quantity: entry.quantity,
                },
            );
            inventory_offsets.push(entry_offset);
        }
        let inventory_vec = builder.create_vector(&inventory_offsets);
        let faction_entry = fb::FactionInventoryState::create(
            builder,
            &fb::FactionInventoryStateArgs {
                faction: state.faction,
                inventory: Some(inventory_vec),
            },
        );
        entries.push(faction_entry);
    }
    builder.create_vector(&entries)
}
