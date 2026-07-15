use super::*;

#[derive(Clone, Debug)]
struct TilePrototype {
    position: UVec2,
    element: ElementKind,
    terrain: sim_runtime::TerrainType,
    tags: sim_runtime::TerrainTags,
    mountain: Option<MountainMetadata>,
    food_module: Option<FoodModule>,
}

#[derive(Clone)]
struct FoodSiteCandidate {
    entry: FoodSiteEntry,
    seasonal_weight: f32,
    preferred: bool,
}

#[derive(Clone, Default)]
struct GridBucketStats {
    candidates: usize,
    selected: usize,
}

fn compare_food_site(a: &FoodSiteCandidate, b: &FoodSiteCandidate) -> Ordering {
    b.seasonal_weight
        .partial_cmp(&a.seasonal_weight)
        .unwrap_or(Ordering::Equal)
        .then_with(|| b.preferred.cmp(&a.preferred))
}

/// Spawn initial grid of tiles, logistics links, power nodes, and population cohorts.
#[allow(clippy::too_many_arguments)]
pub fn spawn_initial_world(
    mut commands: Commands,
    mut config: ResMut<SimulationConfig>,
    map_presets: Res<MapPresetsHandle>,
    registry: Res<GenerationRegistry>,
    knowledge_tags: Res<StartProfileKnowledgeTagsHandle>,
    tick: Res<SimulationTick>,
    mut culture: ResMut<CultureManager>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
    mut faction_inventory: ResMut<FactionInventory>,
    snapshot_overlays: Res<SnapshotOverlaysConfigHandle>,
) {
    let width = config.grid_size.x as usize;
    let height = config.grid_size.y as usize;
    let mut prototypes: Vec<TilePrototype> = Vec::with_capacity(width * height);
    let mut tiles: Vec<Entity> = Vec::with_capacity(width * height);
    let knowledge_catalog = knowledge_tags.get();
    let knowledge_fragments =
        starting_knowledge_fragments(&config.start_profile_overrides, knowledge_catalog.as_ref());
    let inventory_summary = seed_starting_inventory(
        PLAYER_FACTION,
        &config.start_profile_overrides,
        &mut faction_inventory,
    );
    let knowledge_seeded =
        seed_starting_knowledge(PLAYER_FACTION, &knowledge_fragments, &mut discovery);

    if let Some((entries, total_quantity)) = inventory_summary {
        info!(
            target: "shadow_scale::campaign",
            "start_profile.inventory.seeded entries={} total_quantity={}",
            entries,
            total_quantity
        );
    }
    if knowledge_seeded > 0 {
        info!(
            target: "shadow_scale::campaign",
            "start_profile.knowledge.seeded grants={} tags={}",
            knowledge_seeded,
            config.start_profile_overrides.starting_knowledge_tags.len()
        );
    }

    let _global_id = culture.ensure_global();
    let fallback_region = culture.upsert_regional(0);
    if let Some(region_layer) = culture.regional_layer_mut_by_region(0) {
        let modifiers = region_layer.traits.modifier_mut();
        modifiers[CultureTraitAxis::OpenClosed.index()] = scalar_from_f32(0.12);
        modifiers[CultureTraitAxis::TraditionalistRevisionist.index()] = scalar_from_f32(-0.08);
        modifiers[CultureTraitAxis::ExpansionistInsular.index()] = scalar_from_f32(0.15);
        modifiers[CultureTraitAxis::SecularDevout.index()] = scalar_from_f32(0.05);
    }

    let preset_handle = map_presets.get();
    let preset_ref = preset_handle.get(&config.map_preset_id);
    let default_classifier = TerrainClassifierConfig::default();
    let classifier_cfg = preset_ref
        .map(|preset| &preset.terrain_classifier)
        .unwrap_or(&default_classifier);
    let sea_level = preset_ref.map(|p| p.sea_level).unwrap_or(0.6);
    let preset_seed = preset_ref.and_then(|preset| preset.map_seed);
    let mut world_seed = preset_seed.unwrap_or(config.map_seed);

    if preset_seed.is_none() && world_seed == 0 {
        let mut rng = SmallRng::from_entropy();
        world_seed = loop {
            let candidate = rng.gen::<u64>();
            if candidate != 0 {
                break candidate;
            }
        };
        info!(
            "mapgen.seed_selected preset={} seed={}",
            config.map_preset_id, world_seed
        );
    }
    config.map_seed = world_seed;
    commands.insert_resource(WorldGenSeed(world_seed));

    // Per-map biome palette (`docs/plan_biome_palette.md`): built once here, seeded from
    // the resolved world seed, then enforced at the `bias_terrain_for_preset` seam below
    // and by the post-solver `apply_biome_palette_clamp` system. Preset-driven, so a
    // preset-less fallback map keeps its legacy (unrestricted) behavior.
    let tile_count = (width * height).max(1) as u32;
    let biome_palette =
        preset_ref.map(|preset| BiomePalette::build(preset, world_seed, tile_count));
    if let Some(ref palette) = biome_palette {
        commands.insert_resource(palette.clone());
    }

    let base_elevation_field = build_elevation_field(&config, preset_ref, world_seed);
    // Build coherent bands and restamped elevation (if preset available)
    let bands = preset_ref.map(|preset| {
        build_bands(
            &base_elevation_field,
            sea_level,
            &preset.macro_land,
            &preset.shelf,
            &preset.islands,
            &preset.inland_sea,
            &preset.ocean,
            preset.moisture_scale,
            &preset.biomes,
            world_seed,
            preset.mountain_scale,
            &preset.mountains,
            config.map_topology.wrap_horizontal,
        )
    });
    if let Some(ref bands_res) = bands {
        commands.insert_resource(bands_res.elevation.clone().with_sea_level(sea_level));
        commands.insert_resource(MoistureRaster::new(
            config.grid_size.x,
            config.grid_size.y,
            bands_res.moisture.clone(),
        ));
        validate_bands(bands_res, config.grid_size);
    } else {
        commands.insert_resource(base_elevation_field.clone().with_sea_level(sea_level));
        commands.insert_resource(MoistureRaster::new(
            config.grid_size.x,
            config.grid_size.y,
            vec![0.0; (config.grid_size.x * config.grid_size.y) as usize],
        ));
    }

    let mut tags_grid: Vec<sim_runtime::TerrainTags> = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let position = UVec2::new(x as u32, y as u32);
            let element = ElementKind::from_grid(position);
            let mut mountain_meta: Option<MountainMetadata> = None;
            let idx = y * width + x;
            let (terrain, terrain_tags) = if let Some(ref bands_res) = bands {
                match bands_res.terrain[idx] {
                    TerrainBand::Land => {
                        let mountain_cell = bands_res.mountains.get(idx);
                        let relief = bands_res.mountains.relief_scale(idx);
                        if let Some(cell) = mountain_cell {
                            mountain_meta = Some(MountainMetadata {
                                kind: cell.ty,
                                relief,
                            });
                        }
                        terrain_for_position_with_classifier(
                            position,
                            config.grid_size,
                            bands_res.moisture.get(idx).copied(),
                            Some(bands_res.elevation.sample(position.x, position.y)),
                            mountain_cell.map(|cell| (cell.ty, relief)),
                            classifier_cfg,
                        )
                    }
                    TerrainBand::ContinentalShelf => (
                        sim_runtime::TerrainType::ContinentalShelf,
                        terrain_definition(sim_runtime::TerrainType::ContinentalShelf).tags,
                    ),
                    TerrainBand::InlandSea => (
                        sim_runtime::TerrainType::InlandSea,
                        terrain_definition(sim_runtime::TerrainType::InlandSea).tags,
                    ),
                    TerrainBand::ContinentalSlope | TerrainBand::DeepOcean => (
                        sim_runtime::TerrainType::DeepOcean,
                        terrain_definition(sim_runtime::TerrainType::DeepOcean).tags,
                    ),
                }
            } else {
                let elevation = base_elevation_field.sample(position.x, position.y);
                if elevation <= sea_level {
                    if (tile_hash(position) & 1) == 0 {
                        (
                            sim_runtime::TerrainType::DeepOcean,
                            terrain_definition(sim_runtime::TerrainType::DeepOcean).tags,
                        )
                    } else {
                        (
                            sim_runtime::TerrainType::ContinentalShelf,
                            terrain_definition(sim_runtime::TerrainType::ContinentalShelf).tags,
                        )
                    }
                } else {
                    terrain_for_position_with_classifier(
                        position,
                        config.grid_size,
                        None,
                        None,
                        None,
                        &default_classifier,
                    )
                }
            };
            let (mut terrain, mut terrain_tags) = if let Some(preset) = preset_ref {
                bias_terrain_for_preset(terrain, terrain_tags, preset, position, config.grid_size.y)
            } else {
                (terrain, terrain_tags)
            };
            // Palette enforcement (`docs/plan_biome_palette.md` §3.5): the weight/climate
            // chains above cannot exclude highland/volcanic/polar/anomaly biomes, so any
            // off-palette result is remapped to the nearest allowed biome in its niche.
            // `is_polar` keeps the remap climate-safe (a polar wetland collapses to a
            // polar biome, not a temperate marsh).
            if let Some(ref palette) = biome_palette {
                let lat_denom = config.grid_size.y.saturating_sub(1).max(1) as f32;
                let dist_from_equator = (position.y as f32 / lat_denom - 0.5).abs();
                let is_polar = dist_from_equator >= classifier_cfg.polar_latitude_cutoff;
                let remapped = palette.remap(terrain, is_polar);
                if remapped != terrain {
                    terrain = remapped;
                    terrain_tags = terrain_definition(remapped).tags;
                }
            }
            let food_module = classify_food_module_from_traits(terrain, terrain_tags);
            let mountain = if matches!(
                terrain,
                sim_runtime::TerrainType::DeepOcean
                    | sim_runtime::TerrainType::InlandSea
                    | sim_runtime::TerrainType::ContinentalShelf
            ) {
                None
            } else {
                mountain_meta
            };
            tags_grid.push(terrain_tags);
            prototypes.push(TilePrototype {
                position,
                element,
                terrain,
                tags: terrain_tags,
                mountain,
                food_module,
            });
        }
    }

    let province_map = ProvinceMap::generate(
        config.grid_size.x,
        config.grid_size.y,
        &tags_grid,
        world_seed,
    );
    tracing::info!(
        target: "shadow_scale::mapgen",
        provinces = province_map.province_count(),
        land_tiles = province_map.land_tiles(),
        "mapgen.provinces.generated"
    );
    commands.insert_resource(province_map.clone());

    let food_module_grid: Vec<Option<FoodModule>> =
        prototypes.iter().map(|proto| proto.food_module).collect();

    let overlays_cfg = snapshot_overlays.get();
    let food_overlay_cfg = overlays_cfg.food();
    let preference = &config.start_profile_overrides.food_modules;
    let land_tiles = province_map.land_tiles().max(1);
    let baseline_total = food_overlay_cfg.max_total_sites();
    let scaled_total = (land_tiles / 120).max(24);
    let target_total = scaled_total.max(baseline_total).min(land_tiles);
    let mut module_candidates: std::collections::BTreeMap<FoodModule, Vec<FoodSiteCandidate>> =
        std::collections::BTreeMap::new();

    // Elevation field (with the active sea level attached) used to compute each tile's climate
    // temperature. Must exist before the tile loop so temperature is derived from real elevation —
    // hence computed here, after both the bands' restamped field and the base field are available.
    let climate_elevation = bands
        .as_ref()
        .map(|bands_res| bands_res.elevation.clone())
        .unwrap_or_else(|| base_elevation_field.clone())
        .with_sea_level(sea_level);

    let mut province_region_layers: HashMap<ProvinceId, CultureLayerId> = HashMap::new();
    for (idx, proto) in prototypes.iter().enumerate() {
        let (generation, demand, efficiency) = proto.element.power_profile();
        let sum = proto.position.x as usize + proto.position.y as usize;
        let base_mass = scalar_from_f32(1.0 + (sum % 5) as f32 * 0.35);
        let node_id = PowerNodeId(proto.position.y * config.grid_size.x + proto.position.x);
        let storage_capacity = (generation * scalar_from_f32(0.6) + scalar_from_f32(2.0))
            .clamp(scalar_from_f32(1.0), scalar_from_f32(40.0));
        let storage_level =
            (storage_capacity * scalar_from_f32(0.5)).clamp(scalar_zero(), storage_capacity);
        let above_sea = climate_elevation.above_sea_normalized(proto.position.x, proto.position.y);
        let tile_component = Tile {
            position: proto.position,
            element: proto.element,
            mass: base_mass,
            temperature: climate_temperature(
                proto.position.y,
                config.grid_size.y,
                above_sea,
                proto.element,
                &config.climate,
            ),
            terrain: proto.terrain,
            terrain_tags: proto.tags,
            // Captured by `generate_hydrology` when it stamps a navigable channel over this biome.
            underlying_terrain: None,
            mountain: proto.mountain,
            // Populated by `generate_hydrology`, which runs after the world is spawned.
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        };
        let power_component = PowerNode {
            id: node_id,
            base_generation: generation,
            base_demand: demand,
            generation,
            demand,
            efficiency,
            storage_capacity,
            storage_level,
            stability: scalar_from_f32(0.85),
            surplus: scalar_zero(),
            deficit: scalar_zero(),
            incident_count: 0,
        };
        let mut entity_commands = commands.spawn((tile_component.clone(), power_component));
        let module = proto
            .food_module
            .or_else(|| classify_food_module(&tile_component));
        if let Some(module) = module {
            let site_kind = module.site_kind();
            let seasonal_weight = 1.0;
            entity_commands.insert(FoodModuleTag::new(module, seasonal_weight, site_kind));
            module_candidates
                .entry(module)
                .or_default()
                .push(FoodSiteCandidate {
                    entry: FoodSiteEntry {
                        position: proto.position,
                        module,
                        kind: site_kind,
                        seasonal_weight,
                    },
                    seasonal_weight,
                    preferred: preference.matches(module),
                });
        }
        let tile_entity = entity_commands.id();
        tiles.push(tile_entity);

        let parent_region = if let Some(province_id) = province_map.province_at_index(idx) {
            *province_region_layers
                .entry(province_id)
                .or_insert_with(|| culture.upsert_regional(province_id))
        } else {
            fallback_region
        };
        culture.attach_local(tile_entity, parent_region);
        let modifiers = seeded_modifiers_for_position(proto.position);
        culture.apply_initial_modifiers(tile_entity, modifiers);
    }

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let from_entity = tiles[idx];
            if x + 1 < width {
                let to_entity = tiles[y * width + (x + 1)];
                commands.spawn(LogisticsLink {
                    from: from_entity,
                    to: to_entity,
                    capacity: config.base_link_capacity,
                    flow: scalar_zero(),
                });
            }
            if y + 1 < height {
                let to_entity = tiles[(y + 1) * width + x];
                commands.spawn(LogisticsLink {
                    from: from_entity,
                    to: to_entity,
                    capacity: config.base_link_capacity,
                    flow: scalar_zero(),
                });
            }
        }
    }

    // Pass all candidates to the spatial distribution system
    // We rely on the bucket/latitude quota system to select the best sites spatially
    let mut filtered_candidates: Vec<FoodSiteCandidate> = Vec::new();
    for candidates in module_candidates.into_values() {
        filtered_candidates.extend(candidates);
    }

    let bucket_cols = BUCKET_COLS.max(1);
    let bucket_rows = BUCKET_ROWS.max(1);
    let bucket_count = (bucket_cols * bucket_rows) as usize;
    let mut bucket_lists = vec![VecDeque::new(); bucket_count];
    let mut bucket_stats = vec![GridBucketStats::default(); bucket_count];
    let width_u32 = width.max(1) as u32;
    let height_u32 = height.max(1) as u32;

    // Phase 1: Distribute candidates into buckets and count viable tiles per bucket
    let mut bucket_viable_counts = vec![0usize; bucket_count];
    let mut latitude_viable_counts = [0usize; LATITUDE_BANDS]; // north, mid, south
    for proto in prototypes.iter() {
        let bx = ((proto.position.x * bucket_cols) / width_u32).min(bucket_cols - 1);
        let by = ((proto.position.y * bucket_rows) / height_u32).min(bucket_rows - 1);
        let bucket_idx = (by * bucket_cols + bx) as usize;

        // Count viable tiles (tiles that can support food)
        if proto.food_module.is_some() {
            bucket_viable_counts[bucket_idx] += 1;

            // Approximate latitude band for diagnostic logging
            let lat_band = (proto.position.y * LATITUDE_BANDS as u32) / height_u32;
            latitude_viable_counts[lat_band.min((LATITUDE_BANDS - 1) as u32) as usize] += 1;
        }
    }

    // Log viable tile distribution by latitude
    let total_viable_tiles: usize = latitude_viable_counts.iter().sum();
    info!(
        target: "shadow_scale::mapgen",
        "mapgen.food_sites.viable_distribution total={} north={} ({:.1}%) mid={} ({:.1}%) south={} ({:.1}%)",
        total_viable_tiles,
        latitude_viable_counts[0],
        (latitude_viable_counts[0] as f32 / total_viable_tiles.max(1) as f32) * 100.0,
        latitude_viable_counts[1],
        (latitude_viable_counts[1] as f32 / total_viable_tiles.max(1) as f32) * 100.0,
        latitude_viable_counts[2],
        (latitude_viable_counts[2] as f32 / total_viable_tiles.max(1) as f32) * 100.0
    );

    // Distribute candidates into buckets
    for candidate in filtered_candidates {
        let bx = ((candidate.entry.position.x * bucket_cols) / width_u32).min(bucket_cols - 1);
        let by = ((candidate.entry.position.y * bucket_rows) / height_u32).min(bucket_rows - 1);
        let bucket_idx = (by * bucket_cols + bx) as usize;
        if let Some(bucket) = bucket_lists.get_mut(bucket_idx) {
            bucket.push_back(candidate);
        }
    }

    // Sort each bucket by quality
    for bucket in bucket_lists.iter_mut() {
        bucket.make_contiguous().sort_by(compare_food_site);
    }
    for (idx, bucket) in bucket_lists.iter().enumerate() {
        bucket_stats[idx].candidates = bucket.len();
    }

    // Calculate bucket targets within each latitude band
    let mut bucket_targets = vec![0usize; bucket_count];

    // Assign each bucket to a latitude band based on its center Y coordinate
    let mut bucket_to_band: Vec<usize> = vec![0; bucket_count];
    for row in 0..bucket_rows {
        for col in 0..bucket_cols {
            let bucket_idx = (row * bucket_cols + col) as usize;
            // Calculate center Y of this bucket's tile range
            let bucket_y_start = (row * height_u32) / bucket_rows;
            let bucket_y_end = ((row + 1) * height_u32) / bucket_rows;
            let bucket_y_center = (bucket_y_start + bucket_y_end) / 2;

            // Assign to latitude band based on Y coordinate
            // We assume 3 bands: North, Mid, South
            let lat_band = if bucket_y_center < height_u32 / LATITUDE_BANDS as u32 {
                0 // North
            } else if bucket_y_center < (height_u32 * 2) / LATITUDE_BANDS as u32 {
                1 // Mid
            } else {
                2 // South
            };
            bucket_to_band[bucket_idx] = lat_band;
        }
    }

    // Group buckets by latitude band
    let mut band_buckets_vec: Vec<Vec<usize>> = vec![Vec::new(); LATITUDE_BANDS];
    for (bucket_idx, &band) in bucket_to_band.iter().enumerate().take(bucket_count) {
        band_buckets_vec[band].push(bucket_idx);
    }

    info!(
        target: "shadow_scale::mapgen",
        "mapgen.food_sites.band_buckets north={:?} mid={:?} south={:?}",
        band_buckets_vec[0],
        band_buckets_vec[1],
        band_buckets_vec[2]
    );

    // Calculate total viable tiles per band first
    let mut band_viable_counts = [0usize; LATITUDE_BANDS];
    let mut active_bands = 0;
    for lat_band in 0..LATITUDE_BANDS {
        let band_buckets = &band_buckets_vec[lat_band];
        let viable: usize = band_buckets
            .iter()
            .map(|&idx| bucket_viable_counts[idx])
            .sum();
        band_viable_counts[lat_band] = viable;
        if viable > 0 {
            active_bands += 1;
        }
    }

    // Calculate quotas based on active bands
    let mut latitude_targets = [0usize; LATITUDE_BANDS];
    if let Some(base_quota) = target_total.checked_div(active_bands) {
        let remainder = target_total % active_bands;
        let mut distributed_remainder = 0;

        for (lat_band, &viable) in band_viable_counts.iter().enumerate() {
            if viable > 0 {
                latitude_targets[lat_band] = base_quota;
                if distributed_remainder < remainder {
                    latitude_targets[lat_band] += 1;
                    distributed_remainder += 1;
                }
            }
        }
    }

    info!(
        target: "shadow_scale::mapgen",
        "mapgen.food_sites.latitude_quotas north={} mid={} south={} active_bands={}",
        latitude_targets[0],
        latitude_targets[1],
        latitude_targets[2],
        active_bands
    );

    for lat_band in 0..LATITUDE_BANDS {
        let band_viable = band_viable_counts[lat_band];
        if band_viable == 0 {
            continue; // Skip bands with no viable tiles
        }

        let band_buckets = &band_buckets_vec[lat_band];

        // Distribute band quota proportionally to viable tiles within band
        let band_quota = latitude_targets[lat_band];
        let mut allocated = 0;

        for &bucket_idx in band_buckets {
            let viable = bucket_viable_counts[bucket_idx];
            if viable > 0 {
                let proportion = (viable as f32) / (band_viable as f32);
                let target = ((band_quota as f32) * proportion).round() as usize;
                bucket_targets[bucket_idx] = target.min(bucket_stats[bucket_idx].candidates);
                allocated += bucket_targets[bucket_idx];
            }
        }

        // Distribute any remaining quota within this band
        if allocated < band_quota {
            let mut remaining = band_quota - allocated;
            for &bucket_idx in band_buckets {
                if remaining == 0 {
                    break;
                }
                if bucket_stats[bucket_idx].candidates > bucket_targets[bucket_idx] {
                    let can_add = (bucket_stats[bucket_idx].candidates
                        - bucket_targets[bucket_idx])
                        .min(remaining);
                    bucket_targets[bucket_idx] += can_add;
                    remaining -= can_add;
                }
            }
        }
    }

    // Phase 3: Select sites with minimum spacing enforcement
    let min_spacing = food_overlay_cfg.min_site_spacing().max(1);
    let min_spacing_sq = min_spacing * min_spacing;

    // Spatial grid for O(1) proximity checks
    // Cell size equals min_spacing, so we only need to check 3x3 neighborhood
    let grid_cell_size = min_spacing;
    let grid_cols = width_u32.div_ceil(grid_cell_size);
    let grid_rows = height_u32.div_ceil(grid_cell_size);
    let mut spatial_grid: Vec<Vec<UVec2>> = vec![Vec::new(); (grid_cols * grid_rows) as usize];

    let mut curated_entries: Vec<FoodSiteEntry> = Vec::new();
    let mut bucket_rng = SmallRng::seed_from_u64(world_seed ^ 0xF00D_CAFE);

    // Create randomized bucket order (all buckets with viable tiles)
    let mut bucket_order: Vec<usize> = bucket_viable_counts
        .iter()
        .enumerate()
        .filter(|(_, &viable)| viable > 0)
        .map(|(idx, _)| idx)
        .collect();
    bucket_order.shuffle(&mut bucket_rng);

    // Round-robin selection from buckets until all targets met
    let mut any_progress = true;
    while any_progress && curated_entries.len() < target_total {
        any_progress = false;

        for &bucket_idx in &bucket_order {
            if curated_entries.len() >= target_total {
                break;
            }

            // Skip if this bucket has met its target
            if bucket_stats[bucket_idx].selected >= bucket_targets[bucket_idx] {
                continue;
            }

            let bucket = &mut bucket_lists[bucket_idx];

            // Try to select one site from this bucket
            while bucket_stats[bucket_idx].selected < bucket_targets[bucket_idx] {
                if let Some(candidate) = bucket.pop_front() {
                    let pos = candidate.entry.position;

                    // Check proximity using spatial grid
                    let gx = pos.x / grid_cell_size;
                    let gy = pos.y / grid_cell_size;
                    let mut too_close = false;

                    'neighbor_check: for dy in -1..=1 {
                        for dx in -1..=1 {
                            let ny = gy as i32 + dy;
                            let nx = gx as i32 + dx;

                            if nx >= 0 && nx < grid_cols as i32 && ny >= 0 && ny < grid_rows as i32
                            {
                                let cell_idx = (ny as u32 * grid_cols + nx as u32) as usize;
                                for &existing_pos in &spatial_grid[cell_idx] {
                                    let dist_x =
                                        (pos.x as i32 - existing_pos.x as i32).unsigned_abs();
                                    let dist_y =
                                        (pos.y as i32 - existing_pos.y as i32).unsigned_abs();
                                    if dist_x * dist_x + dist_y * dist_y < min_spacing_sq {
                                        too_close = true;
                                        break 'neighbor_check;
                                    }
                                }
                            }
                        }
                    }

                    if !too_close {
                        curated_entries.push(candidate.entry);
                        bucket_stats[bucket_idx].selected += 1;

                        // Add to spatial grid
                        let cell_idx = (gy * grid_cols + gx) as usize;
                        spatial_grid[cell_idx].push(pos);

                        any_progress = true;
                        break; // Move to next bucket
                    }
                    // If too close, try next candidate from this bucket
                } else {
                    break; // Bucket exhausted
                }
            }
        }
    }

    // Phase 4 removed - respect latitude band quotas strictly
    // If we can't fill the quota due to spacing constraints, that's acceptable

    // Diagnostic logging
    let mut row_totals = [0usize; 3];
    for entry in &curated_entries {
        let row = ((entry.position.y.min(height_u32 - 1)) * 3 / height_u32) as usize;
        row_totals[row.min(2)] += 1;
    }
    let total_candidates: usize = bucket_stats.iter().map(|s| s.candidates).sum();
    info!(
        target: "shadow_scale::mapgen",
        "mapgen.food_sites.curated_summary grid={}x{} target={} curated={} candidates={} north={} mid={} south={} min_spacing={}",
        bucket_cols,
        bucket_rows,
        target_total,
        curated_entries.len(),
        total_candidates,
        row_totals[0],
        row_totals[1],
        row_totals[2],
        min_spacing
    );
    for (idx, stats) in bucket_stats.iter().enumerate() {
        if stats.candidates == 0 {
            continue;
        }
        let bucket_row = idx as u32 / bucket_cols;
        let bucket_col = idx as u32 % bucket_cols;
        let viable = bucket_viable_counts[idx];
        let target = bucket_targets[idx];
        info!(
            target: "shadow_scale::mapgen",
            "mapgen.food_sites.bucket_detail bucket={} row={} col={} viable={} target={} available={} selected={} leftover={}",
            idx,
            bucket_row,
            bucket_col,
            viable,
            target,
            stats.candidates,
            stats.selected,
            stats.candidates.saturating_sub(stats.selected)
        );
    }

    let food_radius = food_overlay_cfg.default_radius().max(4);
    let (start_x, start_y) = best_start_tile(
        width as u32,
        height as u32,
        &tags_grid,
        &food_module_grid,
        &config.start_profile_overrides.food_modules,
        &curated_entries,
        food_radius,
    );

    let mut cohort_index = 0usize;
    if config.start_profile_overrides.starting_units.is_empty() {
        spawn_default_population_clusters(
            &mut commands,
            &registry,
            &tiles,
            &tags_grid,
            width,
            height,
            start_x,
            start_y,
            config.population_cluster_stride,
            &mut cohort_index,
            &knowledge_fragments,
        );
    } else {
        spawn_profile_population(
            &mut commands,
            &registry,
            &tiles,
            &tags_grid,
            width,
            height,
            (start_x, start_y),
            &config.start_profile_overrides,
            &mut cohort_index,
            &knowledge_fragments,
        );
    }

    commands.insert_resource(StartLocation::from_profile(
        Some(UVec2::new(start_x, start_y)),
        &config.start_profile_overrides,
    ));
    commands.insert_resource(FoodSiteRegistry::new(curated_entries));

    // If we produced bands, use their restamped elevation field resource now
    if let Some(bands_res) = bands {
        commands.insert_resource(bands_res.elevation.clone());
        // Validate invariants and log
        validate_bands(&bands_res, config.grid_size);
    }

    let topology = PowerTopology::from_grid(
        &tiles,
        config.grid_size.x,
        config.grid_size.y,
        config.power_line_capacity,
    );
    commands.insert_resource(topology);

    commands.insert_resource(TileRegistry {
        tiles,
        width: config.grid_size.x,
        height: config.grid_size.y,
    });

    culture.reconcile(&tick, &InfluencerCultureResonance::default());
    let _ = culture.take_tension_events();
}

/// Seed each freshly spawned cohort's demographics (age brackets + a carried food larder) and
/// apply the starting trade-goods bonus. Food is band-local from day one — every band opens the
/// game carrying its own reserve, so there is no faction provisions pool to distribute.
pub fn apply_starting_inventory_effects(
    mut inventory: ResMut<FactionInventory>,
    demographics: Res<DemographicsConfigHandle>,
    // `With<ResidentBand>`: only real bands are seeded with startup demographics + food reserves; an
    // expedition is seeded explicitly at launch from the home band's larder.
    mut cohorts: Query<&mut PopulationCohort, With<ResidentBand>>,
    mut trade_links: Query<&mut TradeLink>,
) {
    seed_cohort_demographics(&demographics.get(), &mut cohorts);
    apply_trade_goods_bonus(&mut inventory, &mut trade_links);
}

/// Split each cohort's head-count into the three age brackets, seed its larder with
/// `startup.food_reserve_days` turns of its own food demand, and apply the well-fed morale bonus.
fn seed_cohort_demographics(
    config: &DemographicsConfig,
    cohorts: &mut Query<&mut PopulationCohort, With<ResidentBand>>,
) {
    let dist = &config.initial_distribution;
    let reserve_days = scalar_from_f32(config.startup.food_reserve_days);
    let morale_bonus = scalar_from_f32(config.startup.well_fed_morale_bonus);
    for mut cohort in cohorts.iter_mut() {
        let size = cohort.size;
        cohort.set_brackets_from_size(size, dist.children, dist.working, dist.elders);
        let demand = food_demand(
            cohort.children,
            cohort.working,
            cohort.elders,
            &config.consumption,
        );
        cohort.stores.set(FOOD, demand * reserve_days);
        cohort.morale = (cohort.morale + morale_bonus).clamp(scalar_zero(), scalar_one());
    }
}

/// Drop expired fog-reveal pulses queued by scouting commands.
pub fn decay_fog_reveals(mut reveals: ResMut<FogRevealLedger>, tick: Res<SimulationTick>) {
    if reveals.is_empty() {
        return;
    }
    reveals.prune_expired(tick.0);
}

fn apply_trade_goods_bonus(
    inventory: &mut FactionInventory,
    trade_links: &mut Query<&mut TradeLink>,
) {
    const TRADE_GOODS_TO_OPENNESS: f32 = 1.0 / 5000.0;
    const OPENNESS_CAP: f32 = 0.12;
    let trade_goods = inventory.take_stockpile(PLAYER_FACTION, "trade_goods", i64::MAX);
    if trade_goods <= 0 {
        return;
    }
    let openness_delta =
        Scalar::from_f32((trade_goods as f32 * TRADE_GOODS_TO_OPENNESS).clamp(0.0, OPENNESS_CAP));
    if openness_delta <= Scalar::zero() {
        return;
    }
    let mut affected = 0u32;
    for mut link in trade_links.iter_mut() {
        if link.from_faction != PLAYER_FACTION {
            continue;
        }
        link.openness = (link.openness + openness_delta).clamp(scalar_zero(), scalar_one());
        affected = affected.saturating_add(1);
    }
    info!(
        target: "shadow_scale::campaign",
        "start_profile.inventory.trade_goods_applied trade_goods={} openness_delta={} links={}",
        trade_goods,
        openness_delta.to_f32(),
        affected
    );
}

fn tile_hash(position: UVec2) -> u32 {
    let mut n = position.x;
    n = n.wrapping_mul(0x6C8E_9CF5) ^ position.y.wrapping_mul(0xB529_7A4D);
    n ^= n >> 13;
    n = n.wrapping_mul(0x68E3_1DA4);
    n ^= n >> 11;
    n = n.wrapping_mul(0x1B56_C4E9);
    n ^ (n >> 16)
}

fn bias_terrain_for_preset(
    terrain: sim_runtime::TerrainType,
    tags: sim_runtime::TerrainTags,
    preset: &MapPreset,
    position: UVec2,
    grid_height: u32,
) -> (sim_runtime::TerrainType, sim_runtime::TerrainTags) {
    let key = format!("{:?}", terrain);
    let biome_weight = preset.biome_weights.get(&key).copied().unwrap_or(1.0);
    let climate_weight = climate_weight_for_tags(preset, tags, position, grid_height);
    let effective_weight = (biome_weight * climate_weight).clamp(0.0, 2.0);

    let noise = (tile_hash(position) & 0xFFFF) as f32 / 65535.0;
    let lat_denom = grid_height.saturating_sub(1).max(1) as f32;
    let lat = position.y as f32 / lat_denom;
    let dist_from_equator = (lat - 0.5).abs();
    let polar_cutoff = preset.terrain_classifier.polar_latitude_cutoff;
    let is_polar_lat = dist_from_equator >= polar_cutoff;
    let mut result = (terrain, tags);

    if effective_weight < 1.0 {
        if noise > effective_weight {
            if let Some(next) = biome_downgrade(terrain) {
                let def = terrain_definition(next);
                result = (next, def.tags);
            }
        }
    } else if effective_weight > 1.0 {
        let chance = (effective_weight - 1.0).clamp(0.0, 1.0);
        if noise < chance {
            if let Some(next) = biome_upgrade(terrain) {
                let def = terrain_definition(next);
                result = (next, def.tags);
            }
        }
    }

    if is_polar_lat && result.0 == sim_runtime::TerrainType::FreshwaterMarsh {
        let fallback = sim_runtime::TerrainType::PeatHeath;
        let def = terrain_definition(fallback);
        result = (fallback, def.tags);
    } else if is_polar_lat
        && result.1.contains(sim_runtime::TerrainTags::FERTILE)
        && !result.1.contains(sim_runtime::TerrainTags::POLAR)
        && !result.1.contains(sim_runtime::TerrainTags::HIGHLAND)
        && !result.1.contains(sim_runtime::TerrainTags::WATER)
    {
        let fallback = match result.0 {
            sim_runtime::TerrainType::MixedWoodland => sim_runtime::TerrainType::BorealTaiga,
            sim_runtime::TerrainType::PrairieSteppe
            | sim_runtime::TerrainType::AlluvialPlain
            | sim_runtime::TerrainType::Floodplain => sim_runtime::TerrainType::PeriglacialSteppe,
            _ => sim_runtime::TerrainType::BorealTaiga,
        };
        let def = terrain_definition(fallback);
        result = (fallback, def.tags);
    }

    result
}

fn biome_downgrade(terrain: sim_runtime::TerrainType) -> Option<sim_runtime::TerrainType> {
    use sim_runtime::TerrainType::*;
    match terrain {
        Floodplain => Some(AlluvialPlain),
        FreshwaterMarsh => Some(Floodplain),
        AlluvialPlain => Some(PrairieSteppe),
        PrairieSteppe => Some(SemiAridScrub),
        MixedWoodland => Some(PrairieSteppe),
        SemiAridScrub => Some(HotDesertErg),
        TidalFlat => Some(AlluvialPlain),
        MangroveSwamp => Some(Floodplain),
        _ => None,
    }
}

fn biome_upgrade(terrain: sim_runtime::TerrainType) -> Option<sim_runtime::TerrainType> {
    use sim_runtime::TerrainType::*;
    match terrain {
        AlluvialPlain => Some(Floodplain),
        PrairieSteppe => Some(MixedWoodland),
        SemiAridScrub => Some(PrairieSteppe),
        HotDesertErg => Some(SemiAridScrub),
        Floodplain => Some(FreshwaterMarsh),
        MixedWoodland => Some(Floodplain),
        // TidalFlat upgrades to MangroveSwamp, NOT RiverDelta: deltas are placed
        // only at river mouths by the hydrology pass, never by tag-budget noise.
        TidalFlat => Some(MangroveSwamp),
        MangroveSwamp => Some(FreshwaterMarsh),
        _ => None,
    }
}

fn climate_weight_for_tags(
    preset: &MapPreset,
    tags: sim_runtime::TerrainTags,
    position: UVec2,
    grid_height: u32,
) -> f32 {
    let band = climate_band_for_position(position, grid_height);
    let band_weight = preset
        .climate_band_weights
        .get(band)
        .copied()
        .unwrap_or(1.0);
    if (band_weight - 1.0).abs() < f32::EPSILON {
        return 1.0;
    }
    let alignment = climate_alignment_factor(band, tags);
    if band_weight > 1.0 {
        if alignment > 0.0 {
            1.0 + (band_weight - 1.0) * alignment
        } else {
            (1.0 - (band_weight - 1.0) * 0.5).clamp(0.2, 1.0)
        }
    } else if alignment > 0.0 {
        band_weight.max(0.1)
    } else {
        1.0
    }
}

fn climate_band_for_position(position: UVec2, grid_height: u32) -> &'static str {
    if grid_height <= 1 {
        return "temperate";
    }
    let lat = position.y as f32 / (grid_height.saturating_sub(1) as f32);
    let dist_from_equator = (lat - 0.5).abs();
    if dist_from_equator >= POLAR_LATITUDE_THRESHOLD {
        "polar"
    } else if dist_from_equator >= 0.18 {
        "temperate"
    } else {
        "tropical"
    }
}

fn climate_alignment_factor(band: &str, tags: sim_runtime::TerrainTags) -> f32 {
    use sim_runtime::TerrainTags as Tag;
    match band {
        "polar" => {
            if tags.contains(Tag::POLAR) {
                1.0
            } else if tags.contains(Tag::HIGHLAND) {
                0.5
            } else {
                0.0
            }
        }
        "tropical" => {
            if tags.contains(Tag::WETLAND) {
                1.0
            } else if tags.contains(Tag::FERTILE) && tags.contains(Tag::FRESHWATER) {
                0.6
            } else {
                0.0
            }
        }
        "arid" => {
            if tags.contains(Tag::ARID) {
                1.0
            } else {
                0.0
            }
        }
        _ => {
            if tags.contains(Tag::FERTILE)
                && !tags.contains(Tag::ARID)
                && !tags.contains(Tag::POLAR)
            {
                1.0
            } else if tags.contains(Tag::COASTAL) {
                0.5
            } else {
                0.0
            }
        }
    }
}

/// Post-stamping nudge toward target tag budgets using simple heuristics.
pub fn apply_tag_budget_solver(
    config: Res<SimulationConfig>,
    map_presets: Res<MapPresetsHandle>,
    hydro: Option<Res<HydrologyState>>,
    registry: Res<TileRegistry>,
    mut tiles: Query<&mut Tile>,
) {
    let presets = map_presets.get();
    let preset = match presets.get(&config.map_preset_id) {
        Some(p) => p,
        None => return,
    };

    let total = (registry.width * registry.height) as usize;
    if total == 0 {
        return;
    }

    #[derive(Clone, Copy)]
    struct TileInfo {
        entity: Entity,
        terrain: sim_runtime::TerrainType,
        tags: sim_runtime::TerrainTags,
        position: UVec2,
        mountain_kind: Option<MountainType>,
        mountain_relief: f32,
    }

    const NEIGHBOR_OFFSETS_4: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    const NEIGHBOR_OFFSETS_8: [(i32, i32); 8] = [
        (-1, 0),
        (1, 0),
        (0, -1),
        (0, 1),
        (-1, -1),
        (1, 1),
        (-1, 1),
        (1, -1),
    ];

    let width = registry.width as usize;
    let height = registry.height as usize;

    let mut tile_info: Vec<TileInfo> = Vec::with_capacity(total);
    for &entity in registry.tiles.iter() {
        if let Ok(tile) = tiles.get(entity) {
            tile_info.push(TileInfo {
                entity,
                terrain: tile.terrain,
                tags: tile.terrain_tags,
                position: tile.position,
                mountain_kind: tile.mountain.map(|m| m.kind),
                mountain_relief: tile.mountain.map(|m| m.relief).unwrap_or(1.0),
            });
        } else {
            tile_info.push(TileInfo {
                entity,
                terrain: sim_runtime::TerrainType::DeepOcean,
                tags: sim_runtime::TerrainTags::WATER,
                position: UVec2::ZERO,
                mountain_kind: None,
                mountain_relief: 1.0,
            });
        }
    }

    // River-adjacency: a hex flanks a river edge, or is part of a navigable river's hex chain.
    let river_mask = hydro
        .as_ref()
        .map(|hydro| {
            hydro.river_tile_mask(
                registry.width,
                registry.height,
                config.map_topology.wrap_horizontal,
            )
        })
        .unwrap_or_else(|| vec![false; total]);

    fn apply_tile_change(
        tiles: &mut Query<&mut Tile>,
        info: &mut [TileInfo],
        idx: usize,
        new_terrain: sim_runtime::TerrainType,
        mountain_kind: Option<MountainType>,
    ) -> bool {
        let entity = info[idx].entity;
        if let Ok(mut tile) = tiles.get_mut(entity) {
            tile.terrain = new_terrain;
            let def = terrain_definition(new_terrain);
            tile.terrain_tags = def.tags;
            tile.mountain = mountain_kind.map(|kind| MountainMetadata {
                kind,
                relief: info[idx].mountain_relief,
            });
            info[idx].terrain = new_terrain;
            info[idx].tags = def.tags;
            info[idx].mountain_kind = mountain_kind;
            if mountain_kind.is_none() {
                info[idx].mountain_relief = 1.0;
            }
            true
        } else {
            false
        }
    }

    let total_tiles = tile_info.len().max(1);
    let max_iterations = total_tiles * 2;
    let locked: HashSet<&str> = preset
        .locked_terrain_tags
        .iter()
        .map(String::as_str)
        .collect();
    let lock_water = locked.contains("Water");
    let lock_wetland = locked.contains("Wetland");
    let lock_fertile = locked.contains("Fertile");
    let lock_coastal = locked.contains("Coastal");
    let lock_highland = locked.contains("Highland");
    let lock_polar = locked.contains("Polar");
    let lock_arid = locked.contains("Arid");
    let lock_volcanic = locked.contains("Volcanic");
    let lock_hazard = locked.contains("Hazardous");

    let tolerance = preset.tolerance.max(0.0);

    let tag_ratio = |tiles: &[TileInfo], mask: sim_runtime::TerrainTags| -> f32 {
        let count = tiles.iter().filter(|info| info.tags.contains(mask)).count() as f32;
        count / tiles.len().max(1) as f32
    };

    let land_ratio = |tiles: &[TileInfo], mask: sim_runtime::TerrainTags| -> f32 {
        let land_total = tiles
            .iter()
            .filter(|info| !info.tags.contains(sim_runtime::TerrainTags::WATER))
            .count()
            .max(1) as f32;
        let count = tiles
            .iter()
            .filter(|info| {
                !info.tags.contains(sim_runtime::TerrainTags::WATER) && info.tags.contains(mask)
            })
            .count() as f32;
        count / land_total
    };

    let need_delta = |actual: f32, target: f32, denom: usize| -> isize {
        if denom == 0 {
            return 0;
        }
        if actual + tolerance < target {
            ((target - (actual + tolerance)) * denom as f32).ceil() as isize
        } else if actual > target + tolerance {
            -((actual - (target + tolerance)) * denom as f32).ceil() as isize
        } else {
            0
        }
    };

    fn has_neighbor(
        info: &[TileInfo],
        idx: usize,
        mask: sim_runtime::TerrainTags,
        width: usize,
        height: usize,
    ) -> bool {
        let pos = info[idx].position;
        let x = pos.x as i32;
        let y = pos.y as i32;
        for (dx, dy) in NEIGHBOR_OFFSETS_4 {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
                continue;
            }
            let nidx = ny as usize * width + nx as usize;
            if info[nidx].tags.contains(mask) {
                return true;
            }
        }
        false
    }

    fn has_neighbor_any(
        info: &[TileInfo],
        idx: usize,
        mask: sim_runtime::TerrainTags,
        width: usize,
        height: usize,
    ) -> bool {
        let pos = info[idx].position;
        let x = pos.x as i32;
        let y = pos.y as i32;
        for (dx, dy) in NEIGHBOR_OFFSETS_8 {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
                continue;
            }
            let nidx = ny as usize * width + nx as usize;
            if info[nidx].tags.contains(mask) {
                return true;
            }
        }
        false
    }

    let targets = &preset.terrain_tag_targets;
    let get_target = |name: &str| targets.get(name).copied().unwrap_or(0.0);

    if lock_water {
        // --- Water ---
        let want_water = get_target("Water");
        let mut water_iterations = 0usize;
        loop {
            let delta = need_delta(
                tag_ratio(&tile_info, sim_runtime::TerrainTags::WATER),
                want_water,
                total_tiles,
            );
            if delta == 0 {
                break;
            }
            if water_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                let mut candidates: Vec<usize> = (0..tile_info.len())
                    .filter(|&idx| {
                        !tile_info[idx]
                            .tags
                            .contains(sim_runtime::TerrainTags::WATER)
                            // Don't drown hydrology-placed river deltas.
                            && tile_info[idx].terrain
                                != sim_runtime::TerrainType::RiverDelta
                    })
                    .collect();
                candidates.sort_by_key(|idx| {
                    let info = &tile_info[*idx];
                    let priority = if info.tags.contains(sim_runtime::TerrainTags::COASTAL) {
                        0
                    } else if info.tags.contains(sim_runtime::TerrainTags::WETLAND) {
                        1
                    } else {
                        2
                    };
                    (priority, info.position.y, info.position.x)
                });
                for idx in candidates {
                    if remaining == 0 {
                        break;
                    }
                    if apply_tile_change(
                        &mut tiles,
                        &mut tile_info,
                        idx,
                        sim_runtime::TerrainType::DeepOcean,
                        None,
                    ) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::WATER)
                        // Preserve hydrology-placed navigable rivers: draining one back into land
                        // would cut a real waterway in half (same protection RiverDelta gets).
                        && tile_info[idx].terrain != sim_runtime::TerrainType::NavigableRiver
                    {
                        let is_polar =
                            climate_band_for_position(tile_info[idx].position, height as u32)
                                == "polar";
                        let replacement = if is_polar {
                            sim_runtime::TerrainType::SeasonalSnowfield
                        } else {
                            sim_runtime::TerrainType::AlluvialPlain
                        };
                        if apply_tile_change(&mut tiles, &mut tile_info, idx, replacement, None) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            }
            if changed == 0 {
                break;
            }
            water_iterations += 1;
        }
    }
    if lock_wetland {
        // --- Wetland ---
        let want_wetland = get_target("Wetland");
        let mut wetland_iterations = 0usize;
        loop {
            let delta = need_delta(
                tag_ratio(&tile_info, sim_runtime::TerrainTags::WETLAND),
                want_wetland,
                total_tiles,
            );
            if delta == 0 {
                break;
            }
            if wetland_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                let mut candidates: Vec<usize> = (0..tile_info.len())
                    .filter(|&idx| {
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::WETLAND)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                            || info.tags.contains(sim_runtime::TerrainTags::HIGHLAND)
                        {
                            return false;
                        }
                        has_neighbor_any(
                            &tile_info,
                            idx,
                            sim_runtime::TerrainTags::WATER
                                | sim_runtime::TerrainTags::FRESHWATER
                                | sim_runtime::TerrainTags::WETLAND,
                            width,
                            height,
                        )
                    })
                    .collect();
                candidates.sort_by_key(|idx| {
                    let info = &tile_info[*idx];
                    (
                        if river_mask[*idx] { 0 } else { 1 },
                        info.position.y,
                        info.position.x,
                    )
                });
                for idx in candidates {
                    if remaining == 0 {
                        break;
                    }
                    let is_polar =
                        climate_band_for_position(tile_info[idx].position, height as u32)
                            == "polar";
                    let replacement = if is_polar {
                        sim_runtime::TerrainType::PeatHeath
                    } else {
                        sim_runtime::TerrainType::FreshwaterMarsh
                    };
                    if apply_tile_change(&mut tiles, &mut tile_info, idx, replacement, None) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
                if remaining > 0 {
                    for idx in 0..tile_info.len() {
                        if remaining == 0 {
                            break;
                        }
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::WETLAND)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                        {
                            continue;
                        }
                        let is_polar =
                            climate_band_for_position(tile_info[idx].position, height as u32)
                                == "polar";
                        let replacement = if is_polar {
                            sim_runtime::TerrainType::PeatHeath
                        } else {
                            sim_runtime::TerrainType::FreshwaterMarsh
                        };
                        if apply_tile_change(&mut tiles, &mut tile_info, idx, replacement, None) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::WETLAND)
                        // River-mouth deltas are placed by the hydrology pass and
                        // must survive the tag solver; never reduce them away.
                        && tile_info[idx].terrain != sim_runtime::TerrainType::RiverDelta
                    {
                        let near_freshwater = has_neighbor(
                            &tile_info,
                            idx,
                            sim_runtime::TerrainTags::FRESHWATER,
                            width,
                            height,
                        );
                        let replacement =
                            if climate_band_for_position(tile_info[idx].position, height as u32)
                                == "polar"
                            {
                                if near_freshwater {
                                    sim_runtime::TerrainType::PeriglacialSteppe
                                } else {
                                    sim_runtime::TerrainType::BorealTaiga
                                }
                            } else if near_freshwater {
                                sim_runtime::TerrainType::PrairieSteppe
                            } else {
                                sim_runtime::TerrainType::AlluvialPlain
                            };
                        if apply_tile_change(&mut tiles, &mut tile_info, idx, replacement, None) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            }
            if changed == 0 {
                break;
            }
            wetland_iterations += 1;
        }
    }
    if lock_fertile {
        // --- Fertile ---
        let want_fertile = get_target("Fertile");
        let mut fertile_iterations = 0usize;
        loop {
            let delta = need_delta(
                tag_ratio(&tile_info, sim_runtime::TerrainTags::FERTILE),
                want_fertile,
                total_tiles,
            );
            if delta == 0 {
                break;
            }
            if fertile_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                let mut candidates: Vec<usize> = (0..tile_info.len())
                    .filter(|&idx| {
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::FERTILE)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                            || info.tags.contains(sim_runtime::TerrainTags::HIGHLAND)
                            || info.tags.contains(sim_runtime::TerrainTags::POLAR)
                            || info.tags.contains(sim_runtime::TerrainTags::HAZARDOUS)
                        {
                            return false;
                        }
                        if climate_band_for_position(info.position, height as u32) == "polar" {
                            return false;
                        }
                        has_neighbor_any(
                            &tile_info,
                            idx,
                            sim_runtime::TerrainTags::WATER
                                | sim_runtime::TerrainTags::FRESHWATER
                                | sim_runtime::TerrainTags::WETLAND
                                | sim_runtime::TerrainTags::COASTAL,
                            width,
                            height,
                        )
                    })
                    .collect();
                candidates.sort_by_key(|idx| {
                    let info = &tile_info[*idx];
                    (
                        if river_mask[*idx] { 0 } else { 1 },
                        info.position.y,
                        info.position.x,
                    )
                });
                for idx in candidates {
                    if remaining == 0 {
                        break;
                    }
                    let near_water = has_neighbor_any(
                        &tile_info,
                        idx,
                        sim_runtime::TerrainTags::WATER
                            | sim_runtime::TerrainTags::FRESHWATER
                            | sim_runtime::TerrainTags::WETLAND,
                        width,
                        height,
                    );
                    let terrain = if near_water {
                        sim_runtime::TerrainType::Floodplain
                    } else {
                        sim_runtime::TerrainType::AlluvialPlain
                    };
                    if apply_tile_change(&mut tiles, &mut tile_info, idx, terrain, None) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
                if remaining > 0 {
                    for idx in 0..tile_info.len() {
                        if remaining == 0 {
                            break;
                        }
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::FERTILE)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                        {
                            continue;
                        }
                        if climate_band_for_position(info.position, height as u32) == "polar" {
                            continue;
                        }
                        if apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::AlluvialPlain,
                            None,
                        ) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::FERTILE)
                        // Preserve hydrology-placed river deltas (see Wetland pass).
                        && tile_info[idx].terrain != sim_runtime::TerrainType::RiverDelta
                    {
                        let terrain = if river_mask[idx] {
                            sim_runtime::TerrainType::SemiAridScrub
                        } else {
                            sim_runtime::TerrainType::RockyReg
                        };
                        if apply_tile_change(&mut tiles, &mut tile_info, idx, terrain, None) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            }
            if changed == 0 {
                break;
            }
            fertile_iterations += 1;
        }
    }
    if lock_coastal {
        // --- Coastal ---
        let want_coastal = get_target("Coastal");
        let mut coastal_iterations = 0usize;
        loop {
            let delta = need_delta(
                tag_ratio(&tile_info, sim_runtime::TerrainTags::COASTAL),
                want_coastal,
                total_tiles,
            );
            if delta == 0 {
                break;
            }
            if coastal_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                let mut candidates: Vec<usize> = (0..tile_info.len())
                    .filter(|&idx| {
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::COASTAL)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                        {
                            return false;
                        }
                        has_neighbor(
                            &tile_info,
                            idx,
                            sim_runtime::TerrainTags::WATER,
                            width,
                            height,
                        )
                    })
                    .collect();
                candidates.sort_by_key(|idx| {
                    let info = &tile_info[*idx];
                    (info.position.y, info.position.x)
                });
                for idx in candidates {
                    if remaining == 0 {
                        break;
                    }
                    if apply_tile_change(
                        &mut tiles,
                        &mut tile_info,
                        idx,
                        sim_runtime::TerrainType::TidalFlat,
                        None,
                    ) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
                if remaining > 0 {
                    for idx in 0..tile_info.len() {
                        if remaining == 0 {
                            break;
                        }
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::COASTAL)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                        {
                            continue;
                        }
                        if apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::TidalFlat,
                            None,
                        ) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::COASTAL)
                        && !tile_info[idx]
                            .tags
                            .contains(sim_runtime::TerrainTags::WATER)
                        // Preserve hydrology-placed river deltas (see Wetland pass).
                        && tile_info[idx].terrain != sim_runtime::TerrainType::RiverDelta
                        && apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::AlluvialPlain,
                            None,
                        )
                    {
                        remaining -= 1;
                        changed += 1;
                    }
                }
            }
            if changed == 0 {
                break;
            }
            coastal_iterations += 1;
        }
    }
    if lock_highland {
        // --- Highland ---
        let want_highland = get_target("Highland");
        let mut highland_iterations = 0usize;
        loop {
            let delta = need_delta(
                tag_ratio(&tile_info, sim_runtime::TerrainTags::HIGHLAND),
                want_highland,
                total_tiles,
            );
            if delta == 0 {
                break;
            }
            if highland_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                let mut candidates: Vec<usize> = (0..tile_info.len())
                    .filter(|&idx| {
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::HIGHLAND)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                        {
                            return false;
                        }
                        has_neighbor_any(
                            &tile_info,
                            idx,
                            sim_runtime::TerrainTags::HIGHLAND,
                            width,
                            height,
                        ) || matches!(
                            info.mountain_kind,
                            Some(MountainType::Fold | MountainType::Fault)
                        )
                    })
                    .collect();
                candidates.sort_by_key(|idx| {
                    let info = &tile_info[*idx];
                    (info.position.y, info.position.x)
                });
                for idx in candidates {
                    if remaining == 0 {
                        break;
                    }
                    if apply_tile_change(
                        &mut tiles,
                        &mut tile_info,
                        idx,
                        sim_runtime::TerrainType::RollingHills,
                        Some(MountainType::Fold),
                    ) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
                if remaining > 0 {
                    for idx in 0..tile_info.len() {
                        if remaining == 0 {
                            break;
                        }
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::HIGHLAND)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                        {
                            continue;
                        }
                        if apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::RollingHills,
                            Some(MountainType::Fold),
                        ) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::HIGHLAND)
                        && apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::PrairieSteppe,
                            None,
                        )
                    {
                        remaining -= 1;
                        changed += 1;
                    }
                }
            }
            if changed == 0 {
                break;
            }
            highland_iterations += 1;
        }
    }
    if lock_polar {
        // --- Polar ---
        let want_polar = get_target("Polar");
        let polar_band = ((height as f32 * preset.mountains.polar_latitude_fraction)
            .ceil()
            .clamp(1.0, height as f32)) as usize;
        let mut polar_iterations = 0usize;
        loop {
            let delta = need_delta(
                tag_ratio(&tile_info, sim_runtime::TerrainTags::POLAR),
                want_polar,
                total_tiles,
            );
            if delta == 0 {
                break;
            }
            if polar_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                let mut candidates: Vec<usize> = (0..tile_info.len())
                    .filter(|&idx| {
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::POLAR)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                        {
                            return false;
                        }
                        let y = info.position.y as usize;
                        y < polar_band || y >= height - polar_band
                    })
                    .collect();
                candidates.sort_by_key(|idx| {
                    let info = &tile_info[*idx];
                    (info.position.y, info.position.x)
                });
                for idx in candidates {
                    if remaining == 0 {
                        break;
                    }
                    let terrain = if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::HIGHLAND)
                    {
                        sim_runtime::TerrainType::SeasonalSnowfield
                    } else {
                        sim_runtime::TerrainType::Tundra
                    };
                    let mount_kind = tile_info[idx].mountain_kind;
                    if apply_tile_change(&mut tiles, &mut tile_info, idx, terrain, mount_kind) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
                if remaining > 0 {
                    for idx in 0..tile_info.len() {
                        if remaining == 0 {
                            break;
                        }
                        if tile_info[idx]
                            .tags
                            .contains(sim_runtime::TerrainTags::POLAR)
                            || tile_info[idx]
                                .tags
                                .contains(sim_runtime::TerrainTags::WATER)
                        {
                            continue;
                        }
                        let terrain = if tile_info[idx]
                            .tags
                            .contains(sim_runtime::TerrainTags::HIGHLAND)
                        {
                            sim_runtime::TerrainType::SeasonalSnowfield
                        } else {
                            sim_runtime::TerrainType::Tundra
                        };
                        let mount_kind = tile_info[idx].mountain_kind;
                        if apply_tile_change(&mut tiles, &mut tile_info, idx, terrain, mount_kind) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::POLAR)
                    {
                        let mount_kind = tile_info[idx].mountain_kind;
                        if apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::PrairieSteppe,
                            mount_kind,
                        ) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            }
            if changed == 0 {
                break;
            }
            polar_iterations += 1;
        }
    }
    if lock_arid {
        // --- Arid ---
        let want_arid = get_target("Arid");
        let mut arid_iterations = 0usize;
        loop {
            let delta = need_delta(
                tag_ratio(&tile_info, sim_runtime::TerrainTags::ARID),
                want_arid,
                total_tiles,
            );
            if delta == 0 {
                break;
            }
            if arid_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                let mut candidates: Vec<usize> = (0..tile_info.len())
                    .filter(|&idx| {
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::ARID)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                            || info.tags.contains(sim_runtime::TerrainTags::WETLAND)
                            || info.tags.contains(sim_runtime::TerrainTags::FRESHWATER)
                            || info.tags.contains(sim_runtime::TerrainTags::POLAR)
                            || info.tags.contains(sim_runtime::TerrainTags::HIGHLAND)
                        {
                            return false;
                        }
                        true
                    })
                    .collect();
                candidates.sort_by_key(|idx| {
                    let info = &tile_info[*idx];
                    (
                        (info.position.y as i32 - height as i32 / 2).abs(),
                        info.position.y,
                        info.position.x,
                    )
                });
                for idx in candidates {
                    if remaining == 0 {
                        break;
                    }
                    let hash = tile_hash(tile_info[idx].position);
                    let terrain = match hash % 3 {
                        0 => sim_runtime::TerrainType::HotDesertErg,
                        1 => sim_runtime::TerrainType::SemiAridScrub,
                        _ => sim_runtime::TerrainType::RockyReg,
                    };
                    if apply_tile_change(&mut tiles, &mut tile_info, idx, terrain, None) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
                if remaining > 0 {
                    for idx in 0..tile_info.len() {
                        if remaining == 0 {
                            break;
                        }
                        let info = &tile_info[idx];
                        if info.tags.contains(sim_runtime::TerrainTags::ARID)
                            || info.tags.contains(sim_runtime::TerrainTags::WATER)
                        {
                            continue;
                        }
                        if apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::SemiAridScrub,
                            None,
                        ) {
                            remaining -= 1;
                            changed += 1;
                        }
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx].tags.contains(sim_runtime::TerrainTags::ARID)
                        && apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::PrairieSteppe,
                            None,
                        )
                    {
                        remaining -= 1;
                        changed += 1;
                    }
                }
            }
            if changed == 0 {
                break;
            }
            arid_iterations += 1;
        }
    }
    if lock_volcanic {
        // --- Volcanic ---
        let want_volcanic = get_target("Volcanic");
        let mut volcanic_iterations = 0usize;
        loop {
            let delta = need_delta(
                tag_ratio(&tile_info, sim_runtime::TerrainTags::VOLCANIC),
                want_volcanic,
                total_tiles,
            );
            if delta == 0 {
                break;
            }
            if volcanic_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    let info = tile_info[idx];
                    if info.tags.contains(sim_runtime::TerrainTags::VOLCANIC)
                        || info.tags.contains(sim_runtime::TerrainTags::WATER)
                    {
                        continue;
                    }
                    if !matches!(info.mountain_kind, Some(MountainType::Volcanic)) {
                        continue;
                    }
                    if apply_tile_change(
                        &mut tiles,
                        &mut tile_info,
                        idx,
                        sim_runtime::TerrainType::ActiveVolcanoSlope,
                        Some(MountainType::Volcanic),
                    ) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::VOLCANIC)
                        && apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::HighPlateau,
                            Some(MountainType::Dome),
                        )
                    {
                        remaining -= 1;
                        changed += 1;
                    }
                }
            }
            if changed == 0 {
                break;
            }
            volcanic_iterations += 1;
        }
    }
    if lock_hazard {
        // --- Hazardous (land-based ratio) ---
        let want_hazard = get_target("Hazardous");
        let mut hazard_iterations = 0usize;
        loop {
            let land_total = tile_info
                .iter()
                .filter(|info| !info.tags.contains(sim_runtime::TerrainTags::WATER))
                .count()
                .max(1);
            let delta = need_delta(
                land_ratio(&tile_info, sim_runtime::TerrainTags::HAZARDOUS),
                want_hazard,
                land_total,
            );
            if delta == 0 {
                break;
            }
            if hazard_iterations > max_iterations {
                break;
            }
            let mut changed = 0usize;
            if delta > 0 {
                let mut remaining = delta as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    let info = tile_info[idx];
                    if info.tags.contains(sim_runtime::TerrainTags::WATER)
                        || info.tags.contains(sim_runtime::TerrainTags::HAZARDOUS)
                    {
                        continue;
                    }
                    if apply_tile_change(
                        &mut tiles,
                        &mut tile_info,
                        idx,
                        sim_runtime::TerrainType::ImpactCraterField,
                        None,
                    ) {
                        remaining -= 1;
                        changed += 1;
                    }
                }
            } else {
                let mut remaining = (-delta) as usize;
                for idx in 0..tile_info.len() {
                    if remaining == 0 {
                        break;
                    }
                    if tile_info[idx]
                        .tags
                        .contains(sim_runtime::TerrainTags::HAZARDOUS)
                        && apply_tile_change(
                            &mut tiles,
                            &mut tile_info,
                            idx,
                            sim_runtime::TerrainType::PrairieSteppe,
                            None,
                        )
                    {
                        remaining -= 1;
                        changed += 1;
                    }
                }
            }
            if changed == 0 {
                break;
            }
            hazard_iterations += 1;
        }
    }
}

/// Post-solver palette clamp (`docs/plan_biome_palette.md` §6 #2). Insurance behind the
/// build-time force-include of locked-tag fallbacks: after `apply_tag_budget_solver` runs,
/// remap any stray off-palette tile back onto the palette via `BiomePalette::remap`, so the
/// palette is a true invariant of the finished map. Cheap (one pass) and future-proofs the
/// invariant against any new locked tag or edge path. `RiverDelta` is `must_have` (hence
/// always on-palette) so genuine river mouths pass through untouched.
pub fn apply_biome_palette_clamp(
    palette: Option<Res<BiomePalette>>,
    config: Res<SimulationConfig>,
    map_presets: Res<MapPresetsHandle>,
    registry: Res<TileRegistry>,
    mut tiles: Query<&mut Tile>,
) {
    let Some(palette) = palette else {
        return;
    };
    let presets = map_presets.get();
    let polar_cutoff = presets
        .get(&config.map_preset_id)
        .map(|preset| preset.terrain_classifier.polar_latitude_cutoff)
        .unwrap_or(POLAR_LATITUDE_THRESHOLD);
    let lat_denom = registry.height.saturating_sub(1).max(1) as f32;
    for &entity in registry.tiles.iter() {
        if let Ok(mut tile) = tiles.get_mut(entity) {
            if palette.contains(tile.terrain) {
                continue;
            }
            let dist_from_equator = (tile.position.y as f32 / lat_denom - 0.5).abs();
            let is_polar = dist_from_equator >= polar_cutoff;
            let remapped = palette.remap(tile.terrain, is_polar);
            if remapped != tile.terrain {
                tile.terrain = remapped;
                tile.terrain_tags = terrain_definition(remapped).tags;
            }
        }
    }
}

/// Final coastal-shelf reconciliation — the last word on ocean tiles.
///
/// Runs in the Startup chain **after** `generate_hydrology`, `apply_tag_budget_solver`, and
/// `apply_biome_palette_clamp`, so it sees the FINAL land mask: the `RiverDelta`/`Floodplain`/
/// `FreshwaterMarsh` tiles hydrology stamps at river mouths and the polar `Tundra` the tag
/// solver paints over near-shore ocean. `classify_bands` decides the shelf early and hex-exactly,
/// so at that stage there are zero gentle-coast-vs-`DeepOcean` gaps — but those later stages
/// repaint terrain near the coast *after* the shelf exists, creating new land-vs-`DeepOcean`
/// adjacencies with no shelf between them. This pass closes that residual on the live map: every
/// `DeepOcean` tile odd-r hex-adjacent to a GENTLE land tile (rise `< coast_height_threshold`)
/// is reclassified to `ContinentalShelf`, using the SAME hex adjacency
/// (`grid_utils::hex_neighbors_wrapped`) and coast-height gate as `classify_bands` so the two
/// agree. STEEP (cliff/mountain) coasts — where every land hex-neighbour rises `>=` the threshold
/// — keep deep water right at the edge (the passive-vs-active-margin model). Tiles a later stage
/// repainted *as* land sit at or below sea level (rise `<= 0 < threshold`), so they read gentle
/// and their adjacent deep ocean correctly gains a shelf. `ContinentalShelf` is a `must_have`
/// palette biome, so this never conflicts with the palette clamp. Deterministic, no RNG.
pub fn reconcile_coastal_shelf(
    config: Res<SimulationConfig>,
    map_presets: Res<MapPresetsHandle>,
    elevation: Option<Res<ElevationField>>,
    registry: Res<TileRegistry>,
    mut tiles: Query<&mut Tile>,
) {
    let Some(elevation) = elevation else {
        return;
    };
    let width = registry.width as usize;
    let height = registry.height as usize;
    let total = width * height;
    if total == 0 {
        return;
    }

    // Coast-height gate: prefer the active preset's threshold, fall back to the `ShelfConfig`
    // default so the pass still runs when the preset is missing (mirrors `classify_bands`).
    let presets = map_presets.get();
    let coast_height_threshold = presets
        .get(&config.map_preset_id)
        .map(|preset| preset.shelf.coast_height_threshold)
        .unwrap_or_else(|| crate::map_preset::ShelfConfig::default().coast_height_threshold);
    let sea_level = elevation.sea_level;
    let wrap_horizontal = config.map_topology.wrap_horizontal;

    // Row-major snapshot of tags + DeepOcean flags so neighbour lookups don't fight the
    // `&mut Tile` borrow. `registry.tiles` is row-major (index i == position (i%w, i/w)) — the
    // same assumption `apply_tag_budget_solver` relies on for its neighbour indexing.
    let mut tags: Vec<sim_runtime::TerrainTags> = vec![sim_runtime::TerrainTags::WATER; total];
    let mut is_deep = vec![false; total];
    for (i, &entity) in registry.tiles.iter().enumerate().take(total) {
        if let Ok(tile) = tiles.get(entity) {
            tags[i] = tile.terrain_tags;
            is_deep[i] = tile.terrain == sim_runtime::TerrainType::DeepOcean;
        }
    }

    let idx = |x: usize, y: usize| y * width + x;
    let mut to_shelf: Vec<usize> = Vec::new();
    for (i, &deep) in is_deep.iter().enumerate() {
        if !deep {
            continue;
        }
        let x = i % width;
        let y = i / width;
        let gentle_land_neighbour = crate::grid_utils::hex_neighbors_wrapped(
            x as u32,
            y as u32,
            width as u32,
            height as u32,
            wrap_horizontal,
        )
        .any(|(nx, ny)| {
            let nidx = idx(nx as usize, ny as usize);
            // Land = not tagged WATER (treats deltas/marshes/tundra as land, excludes
            // DeepOcean/ContinentalShelf/InlandSea/CoralShelf/HydrothermalVentField). Gentle =
            // rise above sea level below the coast-height threshold (matches `classify_bands`).
            !tags[nidx].contains(sim_runtime::TerrainTags::WATER)
                && (elevation.sample(nx, ny) - sea_level) < coast_height_threshold
        });
        if gentle_land_neighbour {
            to_shelf.push(i);
        }
    }

    let shelf_tags = terrain_definition(sim_runtime::TerrainType::ContinentalShelf).tags;
    for i in to_shelf {
        if let Some(&entity) = registry.tiles.get(i) {
            if let Ok(mut tile) = tiles.get_mut(entity) {
                tile.terrain = sim_runtime::TerrainType::ContinentalShelf;
                tile.terrain_tags = shelf_tags;
            }
        }
    }
}

fn seeded_modifiers_for_position(position: UVec2) -> [Scalar; CULTURE_TRAIT_AXES] {
    let mut modifiers = [Scalar::zero(); CULTURE_TRAIT_AXES];
    let seed = position.x as i32 * 31 + position.y as i32 * 17;
    for (idx, slot) in modifiers.iter_mut().enumerate() {
        let wave = (((seed + idx as i32 * 13) % 23) - 11) as f32;
        let scaled = (wave / 23.0).clamp(-1.0, 1.0) * 0.2;
        *slot = scalar_from_f32(scaled);
    }
    modifiers
}

fn best_start_tile(
    width: u32,
    height: u32,
    tags_grid: &[sim_runtime::TerrainTags],
    food_modules: &[Option<FoodModule>],
    preference: &FoodModulePreference,
    food_sites: &[FoodSiteEntry],
    food_radius: u32,
) -> (u32, u32) {
    let mut best_score: i32 = i32::MIN;
    let mut best_pos: (u32, u32) = (width / 2, height / 2);
    let idx_of = |x: u32, y: u32| -> usize { (y * width + x) as usize };
    for y in 0..height {
        for x in 0..width {
            let idx = idx_of(x, y);
            let tags = tags_grid.get(idx).copied().unwrap_or_default();
            if tags.contains(sim_runtime::TerrainTags::WATER) {
                continue;
            }
            let mut score: i32 = 0;
            // Local tile
            if tags.contains(sim_runtime::TerrainTags::FERTILE) {
                score += 5;
            }
            if tags.contains(sim_runtime::TerrainTags::FRESHWATER) {
                score += 5;
            }
            if tags.contains(sim_runtime::TerrainTags::HAZARDOUS) {
                score -= 6;
            }
            // Neighborhood
            for dy in -3i32..=3 {
                for dx in -3i32..=3 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    let nidx = idx_of(nx as u32, ny as u32);
                    let ntags = tags_grid.get(nidx).copied().unwrap_or_default();
                    if ntags.contains(sim_runtime::TerrainTags::FERTILE) {
                        score += 1;
                    }
                    if ntags.contains(sim_runtime::TerrainTags::FRESHWATER) {
                        score += 2;
                    }
                    if ntags.contains(sim_runtime::TerrainTags::HAZARDOUS) {
                        score -= 2;
                    }
                }
            }
            let center = UVec2::new(x, y);
            let mut food_score = 0.0;
            let mut nearby_sites = 0usize;
            for site in food_sites {
                if manhattan_distance(site.position, center) <= food_radius {
                    nearby_sites += 1;
                    let pref_bonus = if preference.matches(site.module) {
                        0.75
                    } else {
                        0.0
                    };
                    food_score += site.seasonal_weight + pref_bonus;
                }
            }
            if nearby_sites == 0 {
                score -= NO_FOOD_SITE_PENALTY;
            } else if nearby_sites < MIN_NEARBY_CURATED_SITES {
                score -= LOW_FOOD_SITE_PENALTY;
            }
            score += (food_score * 2.5).round() as i32;
            score += module_preference_bonus(x, y, width, height, food_modules, preference);
            if score > best_score {
                best_score = score;
                best_pos = (x, y);
            }
        }
    }
    best_pos
}

fn module_preference_bonus(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    food_modules: &[Option<FoodModule>],
    preference: &FoodModulePreference,
) -> i32 {
    if food_modules.is_empty() || food_modules.len() != (width * height) as usize {
        return 0;
    }
    let mut total = 0;
    if let Some(primary) = preference.primary {
        total += score_for_module(x, y, width, food_modules, primary, true);
    }
    if let Some(secondary) = preference.secondary {
        total += score_for_module(x, y, width, food_modules, secondary, false);
    }
    total
}

fn manhattan_distance(a: UVec2, b: UVec2) -> u32 {
    a.x.abs_diff(b.x) + a.y.abs_diff(b.y)
}

fn score_for_module(
    x: u32,
    y: u32,
    width: u32,
    food_modules: &[Option<FoodModule>],
    module: FoodModule,
    is_primary: bool,
) -> i32 {
    match nearest_module_distance(x, y, width, food_modules, module) {
        Some(distance) => module_distance_bonus(distance, is_primary),
        None if is_primary => -35,
        None => -12,
    }
}

fn nearest_module_distance(
    x: u32,
    y: u32,
    width: u32,
    food_modules: &[Option<FoodModule>],
    module: FoodModule,
) -> Option<u32> {
    let mut best: Option<u32> = None;
    for (idx, entry) in food_modules.iter().enumerate() {
        if *entry == Some(module) {
            let px = (idx as u32) % width;
            let py = (idx as u32) / width;
            let distance = x.abs_diff(px) + y.abs_diff(py);
            best = Some(match best {
                Some(current) => current.min(distance),
                None => distance,
            });
            if distance == 0 {
                break;
            }
        }
    }
    best
}

fn module_distance_bonus(distance: u32, is_primary: bool) -> i32 {
    let base = match distance {
        0 => 32,
        1 => 28,
        2 => 24,
        3 => 18,
        4 => 12,
        5 => 8,
        6 => 4,
        7..=10 => 2,
        _ => -6,
    };
    if is_primary {
        base
    } else {
        ((base as f32) * 0.6).round() as i32
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_default_population_clusters(
    commands: &mut Commands,
    registry: &GenerationRegistry,
    tiles: &[Entity],
    tags_grid: &[sim_runtime::TerrainTags],
    width: usize,
    height: usize,
    start_x: u32,
    start_y: u32,
    stride_tiles: u32,
    cohort_index: &mut usize,
    knowledge: &[KnowledgeFragment],
) {
    let stride = max(1, stride_tiles) as i32;
    let radius: i32 = (stride * 3).max(3);
    for dy in (-radius..=radius).step_by(stride as usize) {
        for dx in (-radius..=radius).step_by(stride as usize) {
            let x = start_x as i32 + dx;
            let y = start_y as i32 + dy;
            if let Some(idx) = tile_index_from_coords(x, y, width, height) {
                if tags_grid
                    .get(idx)
                    .copied()
                    .unwrap_or_default()
                    .contains(sim_runtime::TerrainTags::WATER)
                {
                    continue;
                }
                spawn_population_entity(
                    commands,
                    registry,
                    tiles[idx],
                    1_000,
                    cohort_index,
                    None,
                    knowledge,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_profile_population(
    commands: &mut Commands,
    registry: &GenerationRegistry,
    tiles: &[Entity],
    tags_grid: &[sim_runtime::TerrainTags],
    width: usize,
    height: usize,
    start: (u32, u32),
    overrides: &StartProfileOverrides,
    cohort_index: &mut usize,
    knowledge: &[KnowledgeFragment],
) {
    let mut spawned_total = 0u32;
    for spec in &overrides.starting_units {
        let count = spec.count.max(1);
        for _ in 0..count {
            if let Some((tx, ty)) =
                resolve_starting_unit_tile(spec, start, width, height, tags_grid)
            {
                let idx = (ty as usize) * width + tx as usize;
                let marker = StartingUnit::new(spec.kind.clone(), spec.tags.clone());
                spawn_population_entity(
                    commands,
                    registry,
                    tiles[idx],
                    spec.band_size(),
                    cohort_index,
                    Some(marker),
                    knowledge,
                );
                spawned_total += 1;
            }
        }
    }
    if spawned_total == 0 {
        spawn_default_population_clusters(
            commands,
            registry,
            tiles,
            tags_grid,
            width,
            height,
            start.0,
            start.1,
            1,
            cohort_index,
            knowledge,
        );
    } else {
        info!(
            target: "shadow_scale::campaign",
            "start_profile.units.spawned units={}",
            spawned_total
        );
    }
}

fn spawn_population_entity(
    commands: &mut Commands,
    registry: &GenerationRegistry,
    tile_entity: Entity,
    size: u32,
    cohort_index: &mut usize,
    marker: Option<StartingUnit>,
    knowledge: &[KnowledgeFragment],
) {
    let generation = registry.assign_for_index(*cohort_index);
    *cohort_index = cohort_index.saturating_add(1);
    // Brackets and larder are seeded at Startup by `apply_starting_inventory_effects`
    // (it splits `size` via the demographics config distribution and distributes start-grant
    // provisions into larders) — spawn them empty here.
    let mut entity = commands.spawn(PopulationCohort {
        home: tile_entity,
        current_tile: tile_entity,
        size,
        children: scalar_zero(),
        working: scalar_zero(),
        elders: scalar_zero(),
        stores: LocalStore::new(),
        morale: scalar_from_f32(0.6),
        last_food_consumption: 0.0,
        last_morale_delta: scalar_zero(),
        last_morale_cause: MoraleCause::None,
        last_morale_contributions: MoraleContributions::default(),
        discontent_fraction: scalar_zero(),
        grievance: scalar_zero(),
        last_emigrated: 0,
        last_immigrated: 0,
        age_turns: 0,
        generation,
        faction: FactionId(0),
        knowledge: knowledge.to_vec(),
        migration: None,
    });
    // Every band carries a labor allocation (default empty = fully idle). The client drives
    // assignment; the startup food reserve covers the ramp before the first orders land.
    entity.insert(LaborAllocation::default());
    // Positive `ResidentBand` marker: this is a real band and participates in the
    // population/settlement arc (demographics, migration, sedentarization, startup seeding, supply
    // networks, default-band command pickers). Detached expeditions are spawned separately and
    // deliberately lack it, so they are excluded from those systems by construction.
    entity.insert(ResidentBand);
    if let Some(marker) = marker {
        entity.insert(marker);
    }
}

fn starting_knowledge_fragments(
    overrides: &StartProfileOverrides,
    knowledge_tags: &StartProfileKnowledgeTags,
) -> Vec<KnowledgeFragment> {
    let mut fragments = Vec::new();
    for tag in &overrides.starting_knowledge_tags {
        if let Some(definition) = knowledge_tags.get(tag.as_str()) {
            fragments.push(KnowledgeFragment::new(
                definition.discovery_id(),
                scalar_from_f32(definition.progress()),
                scalar_from_f32(definition.fidelity()),
            ));
        } else {
            warn!(
                target: "shadow_scale::campaign",
                "start_profile.knowledge_tag.unknown tag={}",
                tag
            );
        }
    }
    fragments
}

fn seed_starting_knowledge(
    faction: FactionId,
    fragments: &[KnowledgeFragment],
    ledger: &mut DiscoveryProgressLedger,
) -> usize {
    for fragment in fragments {
        ledger.add_progress(faction, fragment.discovery_id, fragment.progress);
    }
    fragments.len()
}

fn seed_starting_inventory(
    faction: FactionId,
    overrides: &StartProfileOverrides,
    inventory: &mut FactionInventory,
) -> Option<(usize, i64)> {
    if overrides.inventory.is_empty() {
        return None;
    }
    let mut total_quantity = 0i64;
    for entry in &overrides.inventory {
        inventory.add_stockpile(faction, entry.item.clone(), entry.quantity);
        total_quantity += entry.quantity;
    }
    Some((overrides.inventory.len(), total_quantity))
}

fn resolve_starting_unit_tile(
    spec: &StartingUnitSpec,
    start: (u32, u32),
    width: usize,
    height: usize,
    tags_grid: &[sim_runtime::TerrainTags],
) -> Option<(u32, u32)> {
    let base_x = start.0 as i32;
    let base_y = start.1 as i32;
    let (target_x, target_y) = if let Some([ox, oy]) = spec.position {
        (base_x + ox, base_y + oy)
    } else {
        (base_x, base_y)
    };
    if let Some(idx) = tile_index_from_coords(target_x, target_y, width, height) {
        if !tags_grid
            .get(idx)
            .copied()
            .unwrap_or_default()
            .contains(sim_runtime::TerrainTags::WATER)
        {
            return Some((target_x as u32, target_y as u32));
        }
    }
    find_nearest_land_tile(target_x, target_y, width, height, tags_grid)
}

fn find_nearest_land_tile(
    start_x: i32,
    start_y: i32,
    width: usize,
    height: usize,
    tags_grid: &[sim_runtime::TerrainTags],
) -> Option<(u32, u32)> {
    let mut queue = VecDeque::new();
    let mut visited = vec![false; width * height];
    let idx = tile_index_from_coords(start_x, start_y, width, height)?;
    queue.push_back((start_x, start_y, idx));
    visited[idx] = true;
    const NEIGHBORS: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    while let Some((x, y, idx)) = queue.pop_front() {
        let tags = tags_grid.get(idx).copied().unwrap_or_default();
        if !tags.contains(sim_runtime::TerrainTags::WATER) {
            return Some((x as u32, y as u32));
        }
        for (dx, dy) in NEIGHBORS {
            let nx = x + dx;
            let ny = y + dy;
            if let Some(nidx) = tile_index_from_coords(nx, ny, width, height) {
                if !visited[nidx] {
                    visited[nidx] = true;
                    queue.push_back((nx, ny, nidx));
                }
            }
        }
    }
    None
}

fn tile_index_from_coords(x: i32, y: i32, width: usize, height: usize) -> Option<usize> {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return None;
    }
    Some((y as usize) * width + x as usize)
}

/// Latitude-driven base temperature (°): warmest at the center row (equator), symmetric cold toward
/// the top and bottom edges (poles). `lat_frac ∈ [0, 1]` is 0 at the equator and 1 at a pole.
pub(crate) fn latitude_base(y: u32, grid_height: u32, equator_temp: f32, polar_temp: f32) -> f32 {
    let half = grid_height.saturating_sub(1) as f32 / 2.0;
    let lat_frac = if half > 0.0 {
        ((y as f32 - half).abs() / half).clamp(0.0, 1.0)
    } else {
        0.0
    };
    equator_temp - lat_frac * (equator_temp - polar_temp)
}

/// Elevation lapse (°): how much colder a tile is than sea level at the same latitude. Scales the
/// tile's above-sea-level height (normalized to `[0, 1]`) by `elevation_lapse_span`.
pub(crate) fn elevation_lapse(above_sea_normalized: f32, span: f32) -> f32 {
    above_sea_normalized.max(0.0) * span
}

/// Full latitude + elevation climate temperature for a tile, plus a small element-driven local
/// jitter for intra-band texture. Single source shared by worldgen (the tile's initial temperature)
/// and `simulate_materials` (its per-turn relaxation target) so the two never drift.
pub(crate) fn climate_temperature(
    y: u32,
    grid_height: u32,
    above_sea_normalized: f32,
    element: ElementKind,
    climate: &ClimateConfig,
) -> Scalar {
    let base = latitude_base(y, grid_height, climate.equator_temp, climate.polar_temp);
    let lapse = elevation_lapse(above_sea_normalized, climate.elevation_lapse_span);
    let jitter = element.thermal_bias().to_f32() * climate.element_jitter_scale;
    scalar_from_f32(base - lapse + jitter)
}

#[cfg(test)]
mod climate_model_tests {
    use super::*;
    use crate::components::ElementKind;

    const EQUATOR: f32 = 30.0;
    const POLAR: f32 = -5.0;
    const H: u32 = 52;

    #[test]
    fn latitude_base_warmest_at_equator_coldest_at_poles() {
        let equator = latitude_base(H / 2, H, EQUATOR, POLAR);
        let mid = latitude_base(H / 4, H, EQUATOR, POLAR);
        let pole = latitude_base(0, H, EQUATOR, POLAR);
        assert!(equator > mid, "equator {equator} should exceed mid {mid}");
        assert!(mid > pole, "mid {mid} should exceed pole {pole}");
        // Center row is essentially the equator temperature; the true pole is the polar temperature.
        assert!(
            (equator - EQUATOR).abs() < 1.0,
            "equator ~= {EQUATOR}, got {equator}"
        );
        assert!((pole - POLAR).abs() < 0.01, "pole == {POLAR}, got {pole}");
    }

    #[test]
    fn latitude_base_symmetric_top_and_bottom() {
        for offset in 0..(H / 2) {
            let top = latitude_base(offset, H, EQUATOR, POLAR);
            let bottom = latitude_base(H - 1 - offset, H, EQUATOR, POLAR);
            assert!(
                (top - bottom).abs() < 1e-4,
                "row {offset} ({top}) should mirror row {} ({bottom})",
                H - 1 - offset
            );
        }
    }

    #[test]
    fn elevation_lapse_cools_high_ground() {
        let span = 12.0;
        assert_eq!(elevation_lapse(0.0, span), 0.0);
        assert_eq!(elevation_lapse(1.0, span), span);
        // Below sea level clamps to zero lapse (no bonus warmth from being underwater).
        assert_eq!(elevation_lapse(-0.5, span), 0.0);
        // A mountain is colder than sea level at the same latitude.
        let cfg = ClimateConfig {
            equator_temp: EQUATOR,
            polar_temp: POLAR,
            elevation_lapse_span: span,
            element_jitter_scale: 0.25,
        };
        let sea = climate_temperature(H / 2, H, 0.0, ElementKind::Ferrite, &cfg);
        let peak = climate_temperature(H / 2, H, 1.0, ElementKind::Ferrite, &cfg);
        assert!(
            peak < sea,
            "mountain {peak:?} should be colder than sea {sea:?}"
        );
    }
}

#[cfg(test)]
mod terrain_tag_tests {
    use super::*;
    use crate::{
        components::{ElementKind, MountainMetadata, Tile},
        culture::CultureManager,
        generations::GenerationRegistry,
        hydrology,
        map_preset::{MapPreset, MapPresets, MapPresetsHandle},
        mapgen::MountainType,
        resources::{SimulationConfig, SimulationTick, TileRegistry},
        scalar::scalar_from_f32,
        start_profile::{StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle},
    };
    use bevy::{
        ecs::system::SystemState,
        prelude::{UVec2, World},
    };
    use bevy_ecs::system::RunSystemOnce;
    use sim_runtime::{TerrainTags, TerrainType};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn tag_from_name(name: &str) -> TerrainTags {
        match name {
            "Water" => TerrainTags::WATER,
            "Coastal" => TerrainTags::COASTAL,
            "Wetland" => TerrainTags::WETLAND,
            "Fertile" => TerrainTags::FERTILE,
            "Arid" => TerrainTags::ARID,
            "Polar" => TerrainTags::POLAR,
            "Highland" => TerrainTags::HIGHLAND,
            "Volcanic" => TerrainTags::VOLCANIC,
            "Hazardous" => TerrainTags::HAZARDOUS,
            _ => TerrainTags::empty(),
        }
    }

    fn tag_ratios_for_preset(
        preset_id: &str,
        seed: u64,
    ) -> (HashMap<String, f32>, MapPreset, usize) {
        let presets = MapPresets::builtin();
        let preset = presets
            .get(preset_id)
            .unwrap_or_else(|| panic!("missing preset {}", preset_id))
            .clone();

        let mut config = SimulationConfig::builtin();
        config.map_preset_id = preset.id.clone();
        config.map_seed = seed;
        config.grid_size = UVec2::new(preset.dimensions.width, preset.dimensions.height);

        let mut world = World::default();
        world.insert_resource(config);
        world.insert_resource(SimulationTick::default());
        world.insert_resource(CultureManager::default());
        world.insert_resource(GenerationRegistry::with_seed(0xFACE_FEED, 6));
        world.insert_resource(MapPresetsHandle::new(presets));
        world.insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
        world.insert_resource(DiscoveryProgressLedger::default());
        world.insert_resource(FactionInventory::default());
        world.insert_resource(SnapshotOverlaysConfigHandle::new(
            SnapshotOverlaysConfig::builtin(),
        ));

        world.run_system_once(crate::systems::spawn_initial_world);
        hydrology::generate_hydrology(&mut world);
        world.run_system_once(crate::systems::apply_tag_budget_solver);

        let registry = world
            .get_resource::<TileRegistry>()
            .expect("tile registry")
            .clone();
        let mut query = world.query::<&Tile>();
        let total = registry.tiles.len().max(1);
        let mut ratios = HashMap::new();

        let mut land_total = total;
        let mut hazard_land = 0usize;

        for &entity in registry.tiles.iter() {
            if let Ok(tile) = query.get(&world, entity) {
                if tile.terrain_tags.contains(TerrainTags::WATER) {
                    land_total = land_total.saturating_sub(1);
                } else if tile.terrain_tags.contains(TerrainTags::HAZARDOUS) {
                    hazard_land += 1;
                }
            }
        }

        for name in preset.terrain_tag_targets.keys() {
            let tag = tag_from_name(name);
            if tag == TerrainTags::empty() {
                continue;
            }
            if name == "Hazardous" {
                let denominator = land_total.max(1);
                ratios.insert(name.to_string(), hazard_land as f32 / denominator as f32);
                continue;
            }
            let mut count = 0usize;
            for &entity in registry.tiles.iter() {
                if let Ok(tile) = query.get(&world, entity) {
                    if tile.terrain_tags.contains(tag) {
                        count += 1;
                    }
                }
            }
            ratios.insert(name.to_string(), count as f32 / total as f32);
        }

        (ratios, preset, total)
    }

    fn assert_locked_tags_within_tolerance(preset_id: &str, seed: u64) {
        let (ratios, preset, total_tiles) = tag_ratios_for_preset(preset_id, seed);
        let tolerance = preset.tolerance.max(0.01) + 0.02;
        if preset.locked_terrain_tags.is_empty() {
            panic!("preset {preset_id} has no locked terrain tags to verify");
        }
        for name in preset.locked_terrain_tags.iter() {
            let tag = tag_from_name(name);
            if tag == TerrainTags::empty() {
                panic!("preset {preset_id} references unknown locked tag {name}");
            }
            let target = preset.terrain_tag_targets.get(name).copied().unwrap_or(0.0);
            let actual = ratios.get(name).copied().unwrap_or(0.0);
            assert!(
                (actual - target).abs() <= tolerance,
                "{preset_id} locked tag '{name}' ratio out of tolerance: actual {actual:.4}, target {target:.4}, tolerance {tolerance:.4} (tiles={total_tiles})"
            );
        }
    }

    #[test]
    fn locked_tag_solver_respects_tolerances_across_representative_seeds() {
        let scenarios: [(&str, &[u64]); 2] = [
            ("earthlike", &[0xE47E_51DE_2024u64, 0xA17A_DA7A_5E7Du64]),
            ("polar_contrast", &[0x0001_1BAD_C0DEu64, 119_304_647u64]),
        ];

        for (preset_id, seeds) in scenarios {
            for &seed in seeds {
                assert_locked_tags_within_tolerance(preset_id, seed);
            }
        }
    }

    #[test]
    fn tag_solver_counts_existing_highland_tiles() {
        let preset_json = r#"
        {
            "presets": [
                {
                    "id": "test_highland_lock",
                    "name": "Test Highland",
                    "description": "Test preset for highland lock",
                    "seed_policy": "preset_fixed",
                    "map_seed": 42,
                    "dimensions": {"width": 4, "height": 1},
                    "sea_level": 0.4,
                    "continent_scale": 0.5,
                    "mountain_scale": 0.5,
                    "moisture_scale": 1.0,
                    "river_density": 0.0,
                    "lake_chance": 0.0,
                    "climate_band_weights": {},
                    "terrain_tag_targets": {"Highland": 0.25},
                    "biome_weights": {},
                    "postprocess": {},
                    "tolerance": 0.0,
                    "locked_terrain_tags": ["Highland"],
                    "mountains": {},
                    "macro_land": {},
                    "shelf": {},
                    "islands": {},
                    "inland_sea": {},
                    "ocean": {},
                    "biomes": {}
                }
            ]
        }
        "#;

        let presets = MapPresets::from_json_str(preset_json).expect("test preset parses");
        let presets_handle = MapPresetsHandle::new(Arc::new(presets));

        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(4, 1);
        config.map_preset_id = "test_highland_lock".to_string();
        config.map_seed = 42;

        let mut world = World::new();
        world.insert_resource(config);
        world.insert_resource(presets_handle);

        let mut tile_entities = Vec::new();
        for x in 0..4u32 {
            let position = UVec2::new(x, 0);
            let element = ElementKind::Ferrite;
            let (terrain, tags, mountain) = if x == 1 {
                let def = terrain_definition(sim_runtime::TerrainType::RollingHills);
                (
                    sim_runtime::TerrainType::RollingHills,
                    def.tags,
                    Some(MountainMetadata {
                        kind: MountainType::Fold,
                        relief: 1.4,
                    }),
                )
            } else {
                let def = terrain_definition(sim_runtime::TerrainType::PrairieSteppe);
                (sim_runtime::TerrainType::PrairieSteppe, def.tags, None)
            };

            let entity = world
                .spawn(Tile {
                    position,
                    element,
                    mass: scalar_from_f32(1.0),
                    temperature: scalar_from_f32(0.5),
                    terrain,
                    terrain_tags: tags,
                    underlying_terrain: None,
                    mountain,
                    river_edges: 0,
                    river_inflow: 0,
                    river_channel: 0,
                })
                .id();
            tile_entities.push(entity);
        }

        world.insert_resource(TileRegistry {
            tiles: tile_entities.clone(),
            width: 4,
            height: 1,
        });

        #[allow(clippy::type_complexity)]
        let mut system_state: SystemState<(
            Res<SimulationConfig>,
            Res<MapPresetsHandle>,
            Option<Res<HydrologyState>>,
            Res<TileRegistry>,
            Query<&mut Tile>,
        )> = SystemState::new(&mut world);

        {
            let (config_res, presets_res, hydro_res, registry_res, tiles_query) =
                system_state.get_mut(&mut world);
            apply_tag_budget_solver(
                config_res,
                presets_res,
                hydro_res,
                registry_res,
                tiles_query,
            );
        }
        system_state.apply(&mut world);

        let highland_tile = world.entity(tile_entities[1]).get::<Tile>().unwrap();
        assert!(highland_tile
            .terrain_tags
            .contains(sim_runtime::TerrainTags::HIGHLAND));
    }

    #[test]
    fn fertile_lock_skips_polar_latitudes() {
        let preset_json = r#"
        {
            "presets": [
                {
                    "id": "fertile_polar_guard",
                    "name": "Test Fertile Guard",
                    "description": "",
                    "seed_policy": "preset_fixed",
                    "map_seed": 1,
                    "dimensions": {"width": 2, "height": 2},
                    "sea_level": 0.4,
                    "continent_scale": 0.5,
                    "mountain_scale": 0.2,
                    "moisture_scale": 0.6,
                    "river_density": 0.0,
                    "lake_chance": 0.0,
                    "climate_band_weights": {},
                    "terrain_tag_targets": {"Fertile": 0.25},
                    "biome_weights": {},
                    "postprocess": {},
                    "tolerance": 0.0,
                    "locked_terrain_tags": ["Fertile"],
                    "mountains": {},
                    "macro_land": {},
                    "shelf": {},
                    "islands": {},
                    "inland_sea": {},
                    "ocean": {},
                    "biomes": {}
                }
            ]
        }
        "#;

        let presets = MapPresets::from_json_str(preset_json).expect("test preset parses");
        let presets_handle = MapPresetsHandle::new(Arc::new(presets));

        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(2, 6);
        config.map_preset_id = "fertile_polar_guard".to_string();
        config.map_seed = 1;

        let mut world = World::new();
        world.insert_resource(config);
        world.insert_resource(presets_handle);

        let mut tile_entities = Vec::new();
        for y in 0..6u32 {
            for x in 0..2u32 {
                let position = UVec2::new(x, y);
                let element = ElementKind::Ferrite;
                let terrain = if y == 0 || y == 5 {
                    sim_runtime::TerrainType::RockyReg
                } else {
                    sim_runtime::TerrainType::SemiAridScrub
                };
                let def = terrain_definition(terrain);
                let entity = world
                    .spawn(Tile {
                        position,
                        element,
                        mass: scalar_from_f32(1.0),
                        temperature: scalar_from_f32(0.5),
                        terrain,
                        terrain_tags: def.tags,
                        underlying_terrain: None,
                        mountain: None,
                        river_edges: 0,
                        river_inflow: 0,
                        river_channel: 0,
                    })
                    .id();
                tile_entities.push(entity);
            }
        }

        world.insert_resource(TileRegistry {
            tiles: tile_entities.clone(),
            width: 2,
            height: 6,
        });

        #[allow(clippy::type_complexity)]
        let mut system_state: SystemState<(
            Res<SimulationConfig>,
            Res<MapPresetsHandle>,
            Option<Res<HydrologyState>>,
            Res<TileRegistry>,
            Query<&mut Tile>,
        )> = SystemState::new(&mut world);

        {
            let (config_res, presets_res, hydro_res, registry_res, tiles_query) =
                system_state.get_mut(&mut world);
            apply_tag_budget_solver(
                config_res,
                presets_res,
                hydro_res,
                registry_res,
                tiles_query,
            );
        }
        system_state.apply(&mut world);

        for polar_entity in tile_entities.iter().take(2) {
            let tile = world.entity(*polar_entity).get::<Tile>().unwrap();
            assert!(
                !tile
                    .terrain_tags
                    .contains(sim_runtime::TerrainTags::FERTILE),
                "polar latitude tile should not be converted to fertile terrain"
            );
        }

        let fertile_midband = tile_entities[2..]
            .iter()
            .map(|entity| world.entity(*entity).get::<Tile>().unwrap())
            .filter(|tile| {
                tile.terrain_tags
                    .contains(sim_runtime::TerrainTags::FERTILE)
            })
            .count();
        assert!(
            fertile_midband > 0,
            "expected fertile conversion on non-polar tiles"
        );
    }

    #[test]
    fn polar_latitudes_avoid_alluvial_plain_regression() {
        let mut world = World::default();
        let presets = MapPresets::builtin();

        world.insert_resource(SimulationConfig::builtin());
        world.insert_resource(SimulationTick::default());
        world.insert_resource(CultureManager::default());
        world.insert_resource(GenerationRegistry::with_seed(0xFACE_FEED, 6));
        world.insert_resource(MapPresetsHandle::new(presets));
        world.insert_resource(DiscoveryProgressLedger::default());
        world.insert_resource(FactionInventory::default());
        world.insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
        world.insert_resource(SnapshotOverlaysConfigHandle::new(
            SnapshotOverlaysConfig::builtin(),
        ));

        world.run_system_once(crate::systems::spawn_initial_world);
        hydrology::generate_hydrology(&mut world);
        world.run_system_once(crate::systems::apply_tag_budget_solver);

        let config = world.resource::<SimulationConfig>().clone();
        let registry = world
            .get_resource::<TileRegistry>()
            .expect("tile registry after spawn")
            .clone();

        let mut query = world.query::<&Tile>();
        let lat_denom = config.grid_size.y.saturating_sub(1).max(1) as f32;

        let mut polar_land = 0usize;
        let mut polar_alluvial = 0usize;
        let mut polar_freshwater_marsh = 0usize;

        for &entity in registry.tiles.iter() {
            let tile = query.get(&world, entity).expect("tile component");
            if tile.terrain_tags.contains(TerrainTags::WATER) {
                continue;
            }
            let lat = tile.position.y as f32 / lat_denom;
            let dist_from_equator = (lat - 0.5).abs();
            if dist_from_equator < POLAR_LATITUDE_THRESHOLD {
                continue;
            }
            polar_land += 1;
            match tile.terrain {
                TerrainType::AlluvialPlain => polar_alluvial += 1,
                TerrainType::FreshwaterMarsh => polar_freshwater_marsh += 1,
                _ => {}
            }
        }

        assert!(
            polar_land > 0,
            "expected polar land tiles to evaluate latitude constraints"
        );
        assert_eq!(
            polar_alluvial, 0,
            "expected no alluvial plains in polar latitudes (found {} of {})",
            polar_alluvial, polar_land
        );
        assert_eq!(
            polar_freshwater_marsh, 0,
            "expected no freshwater marsh in polar latitudes (found {} of {})",
            polar_freshwater_marsh, polar_land
        );
    }

    #[test]
    fn river_deltas_only_appear_on_river_mouths() {
        // Regression: deltas must be a river-mouth feature only. Previously the
        // biome picker + tag solver stamped RiverDelta by noise along the coast,
        // scattering deltas with no river attached, while genuine river-mouth
        // deltas were culled by the solver's wetland/coastal/fertile reductions.
        let mut world = World::default();
        let presets = MapPresets::builtin();

        let mut config = SimulationConfig::builtin();
        config.map_preset_id = "earthlike".to_string();
        config.map_seed = 119304647;
        // The shipped hydrology config — the map a player actually gets. With a real drainage
        // network there is no override set that manufactures a different river count.

        world.insert_resource(config);
        world.insert_resource(SimulationTick::default());
        world.insert_resource(CultureManager::default());
        world.insert_resource(GenerationRegistry::with_seed(42, 8));
        world.insert_resource(MapPresetsHandle::new(presets));
        world.insert_resource(DiscoveryProgressLedger::default());
        world.insert_resource(FactionInventory::default());
        world.insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
        world.insert_resource(SnapshotOverlaysConfigHandle::new(
            SnapshotOverlaysConfig::builtin(),
        ));

        world.run_system_once(crate::systems::spawn_initial_world);
        hydrology::generate_hydrology(&mut world);
        world.run_system_once(crate::systems::apply_tag_budget_solver);

        let registry = world
            .get_resource::<TileRegistry>()
            .expect("tile registry after spawn")
            .clone();
        let width = registry.width as usize;

        // Every tile a river touches: flanking a river edge, or on a navigable river's hex chain.
        let wrap = world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        let river_mask = world.resource::<crate::HydrologyState>().river_tile_mask(
            registry.width,
            registry.height,
            wrap,
        );

        let is_water = |terrain: TerrainType| {
            matches!(
                terrain,
                TerrainType::DeepOcean
                    | TerrainType::ContinentalShelf
                    | TerrainType::CoralShelf
                    | TerrainType::HydrothermalVentField
                    | TerrainType::InlandSea
                    | TerrainType::NavigableRiver
            )
        };

        // Index -> terrain for neighbour lookups.
        let mut query = world.query::<&Tile>();
        let mut terrain_by_idx = vec![None; registry.tiles.len()];
        for (idx, &entity) in registry.tiles.iter().enumerate() {
            terrain_by_idx[idx] = Some(query.get(&world, entity).expect("tile component").terrain);
        }

        let mut delta_count = 0usize;
        let mut orphan_deltas = 0usize;
        let mut landlocked_deltas = 0usize;
        for (idx, terrain) in terrain_by_idx.iter().enumerate() {
            if *terrain != Some(TerrainType::RiverDelta) {
                continue;
            }
            delta_count += 1;
            if !river_mask[idx] {
                orphan_deltas += 1;
            }
            // The map's OWN topology: hex adjacency, honouring the horizontal wrap. A square 3x3
            // stencil would call a delta on the seam column landlocked when the water it drains
            // into is one hex away across the wrap — which is where hydrology legitimately puts
            // some of them.
            let x = (idx % width) as u32;
            let y = (idx / width) as u32;
            let borders_water = crate::grid_utils::hex_neighbors_wrapped(
                x,
                y,
                registry.width,
                registry.height,
                wrap,
            )
            .any(|(nx, ny)| {
                terrain_by_idx[(ny * registry.width + nx) as usize]
                    .map(is_water)
                    .unwrap_or(false)
            });
            if !borders_water {
                landlocked_deltas += 1;
            }
        }

        assert!(
            delta_count > 0,
            "expected at least one river-mouth delta to be placed"
        );
        assert_eq!(
            orphan_deltas, 0,
            "found {} RiverDelta tiles not on any river path (of {} total deltas)",
            orphan_deltas, delta_count
        );
        // Deltas must sit at a genuine mouth: bordering the ocean or an inland sea.
        assert_eq!(
            landlocked_deltas, 0,
            "found {} RiverDelta tiles not bordering any water body (of {} total deltas)",
            landlocked_deltas, delta_count
        );
    }

    #[test]
    #[ignore]
    fn debug_earthlike_ratios() {
        let (ratios, preset, total_tiles) = tag_ratios_for_preset("earthlike", 0xE47E_51DE_2024u64);
        println!("earthlike ratios (tiles={total_tiles}):");
        for (name, target) in preset.terrain_tag_targets.iter() {
            let actual = ratios.get(name).copied().unwrap_or(0.0);
            println!("  {name}: actual {actual:.4}, target {target:.4}");
        }
    }
}

#[cfg(test)]
mod inventory_effect_tests {
    use super::*;
    use crate::{
        components::PopulationCohort,
        map_preset::{MapPresets, MapPresetsHandle},
        resources::{SimulationConfig, SimulationTick},
        start_profile::{
            InventoryEntry, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle,
        },
    };
    use bevy::prelude::World;
    use bevy_ecs::system::RunSystemOnce;

    fn configured_world(provisions: i64, trade_goods: i64) -> World {
        let mut config = SimulationConfig::builtin();
        config.start_profile_overrides.inventory = vec![
            InventoryEntry {
                item: "provisions".to_string(),
                quantity: provisions,
            },
            InventoryEntry {
                item: "trade_goods".to_string(),
                quantity: trade_goods,
            },
        ];
        let mut world = World::default();
        world.insert_resource(config);
        world.insert_resource(SimulationTick::default());
        world.insert_resource(CultureManager::default());
        world.insert_resource(GenerationRegistry::with_seed(0xFACE_FEED, 6));
        world.insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
        world.insert_resource(DiscoveryProgressLedger::default());
        world.insert_resource(FactionInventory::default());
        world.insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
        world.insert_resource(SnapshotOverlaysConfigHandle::new(
            SnapshotOverlaysConfig::builtin(),
        ));
        world.insert_resource(DemographicsConfigHandle::default());
        world
    }

    /// Startup seeds every band with a carried food larder (its own multi-turn reserve) and a
    /// well-fed morale bonus — food is band-local, so nothing sits in the faction provisions pool.
    #[test]
    fn startup_seeds_larder_and_morale() {
        let mut world = configured_world(0, 0);
        world.run_system_once(crate::systems::spawn_initial_world);
        world.run_system_once(crate::systems::apply_starting_inventory_effects);
        let mut query = world.query::<&PopulationCohort>();
        let mut seeded = false;
        for cohort in query.iter(&world) {
            if cohort.faction != PLAYER_FACTION {
                continue;
            }
            // Well-fed morale bonus lifts the 0.6 spawn baseline, and the band carries food.
            if cohort.morale > scalar_from_f32(0.6) && cohort.stores.get(FOOD) > scalar_zero() {
                seeded = true;
                break;
            }
        }
        assert!(
            seeded,
            "expected startup to seed a food larder and raise morale"
        );
        // The faction provisions pool stays empty — food lives in the bands' larders.
        let provisions = world
            .resource::<FactionInventory>()
            .stockpile(PLAYER_FACTION)
            .and_then(|s| s.get("provisions").copied())
            .unwrap_or(0);
        assert_eq!(
            provisions, 0,
            "provisions should not sit in the faction pool"
        );
    }

    #[test]
    #[ignore = "TradeLinks are now only created when trade routes are established, not at world spawn"]
    fn trade_goods_raise_openness() {
        // TODO: Rewrite this test to establish trade routes first, then verify
        // that trade goods boost openness on those routes.
        let mut world = configured_world(0, 200);
        world.run_system_once(crate::systems::spawn_initial_world);
        let mut base_openness = Vec::new();
        {
            let mut query = world.query::<&TradeLink>();
            for link in query.iter(&world) {
                if link.from_faction == PLAYER_FACTION {
                    base_openness.push(link.openness);
                }
            }
        }
        world.run_system_once(crate::systems::apply_starting_inventory_effects);
        let mut query = world.query::<&TradeLink>();
        let mut increased = false;
        for (idx, link) in query
            .iter(&world)
            .filter(|link| link.from_faction == PLAYER_FACTION)
            .enumerate()
        {
            if idx < base_openness.len() && link.openness > base_openness[idx] {
                increased = true;
                break;
            }
        }
        assert!(increased, "expected trade goods to boost openness");
    }
}
