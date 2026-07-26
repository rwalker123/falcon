use super::*;

pub(crate) fn military_raster_from_state(
    tiles: &[TileState],
    populations: &[PopulationCohortState],
    power_nodes: &[PowerNodeState],
    logistics_raster: &ScalarRasterState,
    grid_size: UVec2,
    overlays: &SnapshotOverlaysConfig,
) -> ScalarRasterState {
    let config = overlays.military();
    let size_factor_denominator = config.size_factor_denominator();
    let presence_clamp_max = config.presence_clamp_max();
    let heavy_size_threshold = config.heavy_size_threshold();
    let heavy_size_bonus = config.heavy_size_bonus();
    let support_clamp_max = config.support_clamp_max();
    let power_margin_max = config.power_margin_max();
    let presence_weight = config.presence_weight();
    let support_weight = config.support_weight();
    let combined_clamp_max = config.combined_clamp_max();

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
    let mut presence = vec![Scalar::zero(); total];
    let mut support = vec![Scalar::zero(); total];

    for cohort in populations {
        let Some(&(x, y)) = tile_positions.get(&cohort.home) else {
            continue;
        };
        if x >= width || y >= height {
            continue;
        }
        let idx = (y as usize) * (width as usize) + x as usize;
        if idx >= presence.len() {
            continue;
        }
        let morale = Scalar::from_raw(cohort.morale).clamp(Scalar::zero(), Scalar::one());
        if morale.raw() <= 0 {
            continue;
        }
        let size_factor = Scalar::from_f32((cohort.size as f32) / size_factor_denominator)
            .clamp(Scalar::zero(), presence_clamp_max);
        let mut contribution = (size_factor * morale).clamp(Scalar::zero(), presence_clamp_max);
        if cohort.size > heavy_size_threshold {
            contribution =
                (contribution + heavy_size_bonus).clamp(Scalar::zero(), presence_clamp_max);
        }
        presence[idx] += contribution;
    }

    if logistics_raster.width > 0
        && logistics_raster.height > 0
        && !logistics_raster.samples.is_empty()
    {
        let src_width = logistics_raster.width as usize;
        let src_height = logistics_raster.height as usize;
        let min_height = src_height.min(height as usize);
        let min_width = src_width.min(width as usize);
        for y in 0..min_height {
            let src_row = y * src_width;
            let dst_row = y * width as usize;
            for x in 0..min_width {
                let src_idx = src_row + x;
                if src_idx >= logistics_raster.samples.len() {
                    break;
                }
                let dst_idx = dst_row + x;
                if dst_idx >= support.len() {
                    break;
                }
                let value = Scalar::from_raw(logistics_raster.samples[src_idx]).abs();
                let clamped = value.clamp(Scalar::zero(), support_clamp_max);
                support[dst_idx] += clamped;
            }
        }
    }

    for node in power_nodes {
        let Some(&(x, y)) = tile_positions.get(&node.entity) else {
            continue;
        };
        if x >= width || y >= height {
            continue;
        }
        let idx = (y as usize) * (width as usize) + x as usize;
        if idx >= support.len() {
            continue;
        }
        let generation = Scalar::from_raw(node.generation).abs();
        let demand = Scalar::from_raw(node.demand).abs();
        let margin = (generation - demand).clamp(Scalar::zero(), power_margin_max);
        support[idx] += margin;
    }

    let mut samples = vec![0i64; total];
    for (idx, sample) in samples.iter_mut().enumerate() {
        let combined = (presence[idx] * presence_weight + support[idx] * support_weight)
            .clamp(Scalar::zero(), combined_clamp_max);
        *sample = combined.raw();
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

/// The client's fog-of-war raster for one viewer. `fog_enabled` is `SimulationConfig::fog_enabled`,
/// the server-owned master switch: with fog off every tile reads Active, matching the herd list,
/// which `HerdSnapshotInputs::herd_is_visible` stops filtering in the same state.
pub(crate) fn visibility_raster_from_ledger(
    ledger: &crate::visibility::VisibilityLedger,
    faction: FactionId,
    grid_size: UVec2,
    fog_enabled: bool,
) -> ScalarRasterState {
    let width = grid_size.x;
    let height = grid_size.y;
    let total = (width * height) as usize;
    let mut samples = vec![0i64; total];

    // Fog off: the whole map is Active before the ledger is even consulted, so a viewer with no
    // faction map (fresh world, or the turn after a rollback clears it) still sees everything.
    if !fog_enabled {
        samples.fill(Scalar::SCALE);
        return ScalarRasterState {
            width,
            height,
            samples,
        };
    }

    let faction_map = ledger.get_faction(faction);
    tracing::debug!(
        target: "shadow_scale::visibility",
        faction = faction.0,
        has_faction = faction_map.is_some(),
        width,
        height,
        "visibility_raster_from_ledger START"
    );

    if let Some(map) = faction_map {
        let mut active_count = 0u32;
        let mut discovered_count = 0u32;
        let mut unexplored_count = 0u32;

        for (pos, tile) in map.iter_tiles() {
            if pos.x >= width || pos.y >= height {
                continue;
            }
            let idx = (pos.y as usize) * (width as usize) + pos.x as usize;
            if idx >= samples.len() {
                continue;
            }
            // Visibility state as a fixed-point Scalar, where `Scalar::SCALE`
            // (1_000_000) is the raw value that represents 1.0. The client's
            // fixed64_to_f32 divides by that factor to recover the intended
            // 0.0 / 0.5 / 1.0 encoding. Higher = more visible:
            // Active -> 1.0 (fully visible, full terrain color)
            // Discovered -> 0.5 (remembered/cloudy terrain)
            // Unexplored -> 0.0 (black/hidden)
            let value = match tile.state {
                crate::visibility::VisibilityState::Active => {
                    active_count += 1;
                    Scalar::SCALE
                }
                crate::visibility::VisibilityState::Discovered => {
                    discovered_count += 1;
                    Scalar::SCALE / 2
                }
                crate::visibility::VisibilityState::Unexplored => {
                    unexplored_count += 1;
                    0
                }
            };
            samples[idx] = value;
        }

        tracing::debug!(
            target: "shadow_scale::visibility",
            active_count,
            discovered_count,
            unexplored_count,
            "visibility_raster_from_ledger faction_found"
        );
    } else {
        // No visibility data for this faction, all unexplored (0 = black)
        // samples already initialized to 0
        tracing::debug!(
            target: "shadow_scale::visibility",
            "visibility_raster_from_ledger NO_FACTION_DATA"
        );
    }

    ScalarRasterState {
        width,
        height,
        samples,
    }
}
