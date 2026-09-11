//! Economy-section FlatBuffers serialization.

use crate::codec::{map_rows, map_rows_if_present, text, FbBuilder};
use crate::state::economy::{FactionInventoryEntryState, FactionInventoryState};
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

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` above, in the same order.
// ---------------------------------------------------------------------------

pub(crate) fn decode_economy_section(
    section: fb::EconomySection<'_>,
    snapshot: &mut WorldSnapshot,
) {
    snapshot.faction_inventory = map_rows(section.factionInventory(), decode_faction_inventory);
}

pub(crate) fn decode_economy_section_delta(
    section: fb::EconomySection<'_>,
    delta: &mut WorldDelta,
) {
    delta.faction_inventory =
        map_rows_if_present(section.factionInventory(), decode_faction_inventory);
}

fn decode_faction_inventory(state: fb::FactionInventoryState<'_>) -> FactionInventoryState {
    FactionInventoryState {
        faction: state.faction(),
        inventory: map_rows(state.inventory(), |entry| FactionInventoryEntryState {
            item: text(entry.item()),
            quantity: entry.quantity(),
        }),
    }
}
