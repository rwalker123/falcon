use super::*;

pub(crate) fn logistics_raster_from_links(
    tiles: &[TileState],
    logistics: &[LogisticsLinkState],
    grid_size: UVec2,
) -> ScalarRasterState {
    let mut tile_positions = HashMap::with_capacity(tiles.len());
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        tile_positions.insert(tile.entity, (tile.x, tile.y));
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }

    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);
    let mut samples = vec![0i64; total];
    let mut counts = vec![0u32; total];

    for link in logistics {
        let flow = Scalar::from_raw(link.flow).abs().raw();
        if flow == 0 {
            continue;
        }
        if let Some(&(x, y)) = tile_positions.get(&link.from) {
            let idx = (y as usize) * (width as usize) + x as usize;
            if idx < samples.len() {
                samples[idx] = samples[idx].saturating_add(flow);
                counts[idx] = counts[idx].saturating_add(1);
            }
        }
        if let Some(&(x, y)) = tile_positions.get(&link.to) {
            let idx = (y as usize) * (width as usize) + x as usize;
            if idx < samples.len() {
                samples[idx] = samples[idx].saturating_add(flow);
                counts[idx] = counts[idx].saturating_add(1);
            }
        }
    }

    for (value, count) in samples.iter_mut().zip(counts.iter()) {
        if *count > 0 {
            let divisor = i64::from(*count);
            *value = value.checked_div(divisor).unwrap_or_default();
        }
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

pub(crate) fn logistics_state(entity: Entity, link: &LogisticsLink) -> LogisticsLinkState {
    LogisticsLinkState {
        entity: entity.to_bits(),
        from: link.from.to_bits(),
        to: link.to.to_bits(),
        capacity: link.capacity.raw(),
        flow: link.flow.raw(),
    }
}

pub(crate) fn trade_link_state(
    entity: Entity,
    link: &LogisticsLink,
    trade: &TradeLink,
) -> TradeLinkState {
    TradeLinkState {
        entity: entity.to_bits(),
        from_faction: trade.from_faction.0,
        to_faction: trade.to_faction.0,
        throughput: trade.throughput.raw(),
        tariff: trade.tariff.raw(),
        knowledge: TradeLinkKnowledge {
            openness: trade.openness.raw(),
            leak_timer: trade.leak_timer,
            last_discovery: trade.last_discovery.unwrap_or_default(),
            decay: trade.decay.raw(),
        },
        from_tile: link.from.to_bits(),
        to_tile: link.to.to_bits(),
        pending_fragments: fragments_to_contract(&trade.pending_fragments),
    }
}

pub(crate) fn trade_link_from_state(state: &TradeLinkState) -> TradeLink {
    TradeLink {
        from_faction: FactionId(state.from_faction),
        to_faction: FactionId(state.to_faction),
        throughput: Scalar::from_raw(state.throughput),
        tariff: Scalar::from_raw(state.tariff),
        openness: Scalar::from_raw(state.knowledge.openness),
        decay: Scalar::from_raw(state.knowledge.decay),
        leak_timer: state.knowledge.leak_timer,
        last_discovery: if state.knowledge.last_discovery == 0 {
            None
        } else {
            Some(state.knowledge.last_discovery)
        },
        pending_fragments: fragments_from_contract(&state.pending_fragments),
    }
}

pub(crate) fn snapshot_faction_inventory(
    inventory: &FactionInventory,
) -> Vec<SchemaFactionInventoryState> {
    let mut states = Vec::new();
    for (faction, items) in inventory.iter() {
        if items.is_empty() {
            continue;
        }
        let mut entries: Vec<_> = items
            .iter()
            .map(|(item, quantity)| SchemaFactionInventoryEntryState {
                item: item.clone(),
                quantity: *quantity,
            })
            .collect();
        entries.sort_by(|a, b| a.item.cmp(&b.item));
        states.push(SchemaFactionInventoryState {
            faction: faction.0,
            inventory: entries,
        });
    }
    states.sort_by_key(|a| a.faction);
    states
}
