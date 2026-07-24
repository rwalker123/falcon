use super::*;

pub(crate) struct FogRasterInputs<'a> {
    pub(crate) tiles: &'a [TileState],
    pub(crate) populations: &'a [PopulationCohortState],
    pub(crate) discovery: &'a DiscoveryProgressLedger,
    pub(crate) grid_size: UVec2,
    pub(crate) overlays: &'a SnapshotOverlaysConfig,
    pub(crate) start_location: &'a StartLocation,
    pub(crate) fog_reveals: &'a FogRevealLedger,
    pub(crate) tick: u64,
}

pub(crate) fn fog_raster_from_discoveries(inputs: FogRasterInputs<'_>) -> ScalarRasterState {
    let FogRasterInputs {
        tiles,
        populations,
        discovery,
        grid_size,
        overlays,
        start_location,
        fog_reveals,
        tick,
    } = inputs;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for tile in tiles {
        max_x = max_x.max(tile.x);
        max_y = max_y.max(tile.y);
    }

    let width = grid_size.x.max(max_x.saturating_add(1)).max(1);
    let height = grid_size.y.max(max_y.saturating_add(1)).max(1);
    let total = (width as usize).saturating_mul(height as usize).max(1);

    if matches!(start_location.fog_mode(), FogMode::Revealed) {
        return ScalarRasterState {
            width,
            height,
            samples: vec![Scalar::zero().raw(); total],
        };
    }

    let mut samples = vec![Scalar::one().raw(); total];
    let skip_coverage = matches!(start_location.fog_mode(), FogMode::Shroud);

    if !skip_coverage {
        let mut tile_indices = HashMap::with_capacity(tiles.len());
        for tile in tiles {
            if tile.x < width && tile.y < height {
                let idx = (tile.y as usize) * (width as usize) + tile.x as usize;
                tile_indices.insert(tile.entity, idx);
            }
        }

        let mut tile_faction_sizes: HashMap<u64, HashMap<u32, u64>> = HashMap::new();
        let mut tile_local_weighted: HashMap<u64, (i128, i128)> = HashMap::new();

        for cohort in populations {
            let size = u64::from(cohort.size);
            if size > 0 {
                let faction_map = tile_faction_sizes.entry(cohort.home).or_default();
                *faction_map.entry(cohort.faction).or_insert(0) += size;
            }

            if size == 0 {
                continue;
            }

            let fragments = &cohort.knowledge_fragments;
            let fragment_average_raw = if fragments.is_empty() {
                0i64
            } else {
                let mut total = Scalar::zero();
                for fragment in fragments {
                    total +=
                        Scalar::from_raw(fragment.progress).clamp(Scalar::zero(), Scalar::one());
                }
                let count = fragments.len() as u32;
                (total / Scalar::from_u32(count))
                    .clamp(Scalar::zero(), Scalar::one())
                    .raw()
            };

            let weight = i128::from(size);
            let entry = tile_local_weighted.entry(cohort.home).or_insert((0, 0));
            entry.0 = entry
                .0
                .saturating_add(i128::from(fragment_average_raw) * weight);
            entry.1 = entry.1.saturating_add(weight);
        }

        let mut tile_local_average: HashMap<u64, Scalar> = HashMap::new();
        for (tile_entity, (weighted_sum, total_weight)) in tile_local_weighted {
            if total_weight <= 0 {
                continue;
            }
            let mut average = weighted_sum / total_weight;
            if average < 0 {
                average = 0;
            }
            let scale = i128::from(Scalar::SCALE);
            if average > scale {
                average = scale;
            }
            tile_local_average.insert(tile_entity, Scalar::from_raw(average as i64));
        }

        let mut tile_controllers: HashMap<u64, u32> = HashMap::new();
        for (tile_entity, faction_map) in &tile_faction_sizes {
            let mut best: Option<(u32, u64)> = None;
            for (&faction, &count) in faction_map.iter() {
                best = match best {
                    None => Some((faction, count)),
                    Some((best_faction, best_count)) => {
                        if count > best_count || (count == best_count && faction < best_faction) {
                            Some((faction, count))
                        } else {
                            Some((best_faction, best_count))
                        }
                    }
                };
            }
            if let Some((faction, _)) = best {
                tile_controllers.insert(*tile_entity, faction);
            }
        }

        let blend_weight = overlays.fog().global_local_blend();

        for tile in tiles {
            let Some(&idx) = tile_indices.get(&tile.entity) else {
                continue;
            };

            let global_cov = tile_controllers.get(&tile.entity).and_then(|&faction| {
                discovery
                    .progress
                    .get(&FactionId(faction))
                    .and_then(|entries| {
                        if entries.is_empty() {
                            return None;
                        }
                        let mut total = Scalar::zero();
                        let mut count = 0u32;
                        for value in entries.values() {
                            if value.raw() <= 0 {
                                continue;
                            }
                            total += (*value).clamp(Scalar::zero(), Scalar::one());
                            count = count.saturating_add(1);
                        }
                        if count == 0 {
                            None
                        } else {
                            Some(
                                (total / Scalar::from_u32(count))
                                    .clamp(Scalar::zero(), Scalar::one()),
                            )
                        }
                    })
            });

            let local_cov = tile_local_average.get(&tile.entity).copied();

            let coverage = match (global_cov, local_cov) {
                (Some(g), Some(l)) => ((g + l) * blend_weight).clamp(Scalar::zero(), Scalar::one()),
                (Some(g), None) => g,
                (None, Some(l)) => l,
                (None, None) => Scalar::zero(),
            };

            let fog = (Scalar::one() - coverage).clamp(Scalar::zero(), Scalar::one());
            samples[idx] = fog.raw();
        }
    }

    apply_start_location_reveal(&mut samples, width, height, start_location);
    apply_scout_reveals(&mut samples, width, height, fog_reveals, tick);

    ScalarRasterState {
        width,
        height,
        samples,
    }
}

pub(crate) fn apply_start_location_reveal(
    samples: &mut [i64],
    width: u32,
    height: u32,
    start_location: &StartLocation,
) {
    let Some(center) = start_location.position() else {
        return;
    };
    let radius = start_location.survey_radius().unwrap_or(0);
    clear_circle(samples, width, height, center, radius);
}

pub(crate) fn apply_scout_reveals(
    samples: &mut [i64],
    width: u32,
    height: u32,
    fog_reveals: &FogRevealLedger,
    tick: u64,
) {
    if fog_reveals.is_empty() {
        return;
    }
    for reveal in fog_reveals.iter_active(tick) {
        clear_circle(samples, width, height, reveal.center, reveal.radius);
    }
}

pub(crate) fn clear_circle(
    samples: &mut [i64],
    width: u32,
    height: u32,
    center: UVec2,
    radius: u32,
) {
    if samples.is_empty() {
        return;
    }
    let width = width.max(1) as usize;
    let height = height.max(1) as usize;
    let radius_i32 = radius.min(i32::MAX as u32) as i32;
    let radius_sq = radius_i32.pow(2);
    for y in 0..height {
        for x in 0..width {
            let dx = x as i32 - center.x as i32;
            let dy = y as i32 - center.y as i32;
            if dx * dx + dy * dy <= radius_sq {
                let idx = y * width + x;
                if idx < samples.len() {
                    samples[idx] = 0;
                }
            }
        }
    }
}

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

pub(crate) fn visibility_raster_from_ledger(
    ledger: &crate::visibility::VisibilityLedger,
    faction: FactionId,
    grid_size: UVec2,
) -> ScalarRasterState {
    let width = grid_size.x;
    let height = grid_size.y;
    let total = (width * height) as usize;
    let mut samples = vec![0i64; total];

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
