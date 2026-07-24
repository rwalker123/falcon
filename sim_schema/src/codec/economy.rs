//! Economy-section FlatBuffers serialization.

use crate::codec::{create_known_fragments, create_scalar_raster, FbBuilder};
use crate::state::economy::{FactionInventoryState, LogisticsLinkState, TradeLinkState};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_economy_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::EconomySection<'a>> {
    let logistics = create_logistics(builder, &snapshot.logistics);
    let trade_links = create_trade_links(builder, &snapshot.trade_links);
    let logistics_raster = create_scalar_raster(builder, &snapshot.logistics_raster);
    let faction_inventory = create_faction_inventory(builder, &snapshot.faction_inventory);
    fb::EconomySection::create(
        builder,
        &fb::EconomySectionArgs {
            logistics: Some(logistics),
            tradeLinks: Some(trade_links),
            logisticsRaster: Some(logistics_raster),
            factionInventory: Some(faction_inventory),
            removedLogistics: None,
            removedTradeLinks: None,
        },
    )
}

pub(crate) fn serialize_economy_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::EconomySection<'a>> {
    let logistics = create_logistics(builder, &delta.logistics);
    let removed_logistics = builder.create_vector(&delta.removed_logistics);
    let trade_links = create_trade_links(builder, &delta.trade_links);
    let removed_trade_links = builder.create_vector(&delta.removed_trade_links);
    let logistics_raster = delta
        .logistics_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let faction_inventory = delta
        .faction_inventory
        .as_ref()
        .map(|entries| create_faction_inventory(builder, entries));
    fb::EconomySection::create(
        builder,
        &fb::EconomySectionArgs {
            logistics: Some(logistics),
            tradeLinks: Some(trade_links),
            logisticsRaster: logistics_raster,
            factionInventory: faction_inventory,
            removedLogistics: Some(removed_logistics),
            removedTradeLinks: Some(removed_trade_links),
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

fn create_logistics<'a>(
    builder: &mut FbBuilder<'a>,
    links: &[LogisticsLinkState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::LogisticsLinkState<'a>>>> {
    let offsets: Vec<_> = links
        .iter()
        .map(|link| {
            fb::LogisticsLinkState::create(
                builder,
                &fb::LogisticsLinkStateArgs {
                    entity: link.entity,
                    from: link.from,
                    to: link.to,
                    capacity: link.capacity,
                    flow: link.flow,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_trade_links<'a>(
    builder: &mut FbBuilder<'a>,
    links: &[TradeLinkState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::TradeLinkState<'a>>>> {
    let offsets: Vec<_> = links
        .iter()
        .map(|link| {
            let knowledge = fb::TradeLinkKnowledge::create(
                builder,
                &fb::TradeLinkKnowledgeArgs {
                    openness: link.knowledge.openness,
                    leakTimer: link.knowledge.leak_timer,
                    lastDiscovery: link.knowledge.last_discovery,
                    decay: link.knowledge.decay,
                },
            );
            let pending_fragments = if link.pending_fragments.is_empty() {
                None
            } else {
                Some(create_known_fragments(builder, &link.pending_fragments))
            };
            fb::TradeLinkState::create(
                builder,
                &fb::TradeLinkStateArgs {
                    entity: link.entity,
                    fromFaction: link.from_faction,
                    toFaction: link.to_faction,
                    throughput: link.throughput,
                    tariff: link.tariff,
                    knowledge: Some(knowledge),
                    fromTile: link.from_tile,
                    toTile: link.to_tile,
                    pendingFragments: pending_fragments,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}
