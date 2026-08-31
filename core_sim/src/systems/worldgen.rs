use super::*;
use crate::climate::{climate_band_for_temperature, ClimateBand};

/// Seasonal weight a freshly stamped `FoodModuleTag` carries: full strength, unmodulated. The
/// seasonal cycle is applied later by the food systems; worldgen only decides *which* module a tile
/// belongs to, never how strong its season currently is.
const INITIAL_SEASONAL_WEIGHT: f32 = 1.0;

#[derive(Clone, Debug)]
struct TilePrototype {
    position: UVec2,
    element: ElementKind,
    /// The tile's climate temperature, computed in the prototype loop **before** its biome — the
    /// biome is derived from it (`docs/plan_climate_authority.md` §4). Carried forward so the spawn
    /// loop seeds `Tile::temperature` from the very value the biome gate read, rather than
    /// recomputing it and risking drift.
    temperature: Scalar,
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

/// Spawn initial grid of tiles, power nodes, and population cohorts.
///
/// **Idempotent**, mirroring its three Startup siblings (`spawn_initial_herds`,
/// `spawn_initial_forage`, `spawn_initial_graze`): a second pass over an already-built world
/// early-returns instead of laying down a duplicate `width × height` tile set, a second batch of
/// starting cohorts, and a second helping of the start profile's inventory.
///
/// `tile_registry` is `Option<Res<..>>` and **must stay that way** — `TileRegistry` is inserted by
/// this system (via `Commands`), not `init_resource`'d by the plugin, so on the legitimate first
/// run (and on the shipped server, which boots idle) the resource does not exist at all and a bare
/// `Res<TileRegistry>` would panic.
///
/// The guard cannot collide with map regeneration: `rebuild_world_from_config` — the shared path
/// for both `new_game` and `ResetMap` — starts from `build_headless_app()`, i.e. a brand-new
/// `World` with no `TileRegistry`, rather than re-running Startup on the existing one.
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
    // **`Option`, like `tile_registry` beside it**: the handle only decides which items a spawned
    // band is stocked with, and a hand-rolled test `World` that never installs it would otherwise
    // panic worldgen outright. Absent reads as the builtin table — the same table
    // `EquipmentConfigHandle::default()` installs.
    equipment: Option<Res<crate::equipment_config::EquipmentConfigHandle>>,
    // **The other two halves of the start kit** — `Option` for the same reason `equipment` is: they
    // only decide the *grade* a spawned band's gear is stamped with, and a hand-rolled test `World`
    // that installs neither must not panic worldgen.
    recipes: Option<Res<crate::recipes_config::RecipesConfigHandle>>,
    materials: Option<Res<crate::materials_config::MaterialsConfigHandle>>,
    // **The fourth half of the start kit** — the working-age share a spawn's stock is sized against
    // (see [`StartKit::working_fraction`]). `Option` for the same reason the three above are.
    demographics: Option<Res<crate::demographics_config::DemographicsConfigHandle>>,
    tile_registry: Option<Res<TileRegistry>>,
) {
    // Guard FIRST: the starting inventory, knowledge and culture seeding below all run ahead of any
    // tile work, so a guard placed lower would still double the start profile's grants. Unlike the
    // silent siblings this warns — a second Startup pass is never intentional, it is the test trap
    // this guard exists to close, so it should leave a trace rather than no-op invisibly.
    if tile_registry.is_some_and(|registry| !registry.tiles.is_empty()) {
        tracing::warn!(
            target: "shadow_scale::worldgen",
            "worldgen.spawn_initial_world.skipped=already_built"
        );
        return;
    }

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
    let fallback_region = culture.upsert_regional(FALLBACK_CULTURE_REGION_ID);
    if let Some(region_layer) = culture.regional_layer_mut_by_region(FALLBACK_CULTURE_REGION_ID) {
        let modifiers = region_layer.traits.modifier_mut();
        modifiers[CultureTraitAxis::OpenClosed.index()] = scalar_from_f32(0.12);
        modifiers[CultureTraitAxis::TraditionalistRevisionist.index()] = scalar_from_f32(-0.08);
        modifiers[CultureTraitAxis::ExpansionistInsular.index()] = scalar_from_f32(0.15);
        modifiers[CultureTraitAxis::SecularDevout.index()] = scalar_from_f32(0.05);
    }

    let preset_handle = map_presets.get();
    let preset_ref = preset_handle.get(&config.map_preset_id);
    if preset_ref.is_none() {
        // A silent `None` here is far worse than it looks: the preset-less path skips erosion AND
        // the contour anchor entirely (`heightfield::build_elevation_field`), so the land mask no
        // longer sits on the `target_land_pct` contour — on top of falling back to the default sea
        // level. It must never happen quietly.
        tracing::warn!(
            target: "shadow_scale::worldgen",
            map_preset_id = %config.map_preset_id,
            "worldgen.map_preset.unresolved"
        );
    }
    let default_classifier = TerrainClassifierConfig::default();
    let classifier_cfg = preset_ref
        .map(|preset| &preset.terrain_classifier)
        .unwrap_or(&default_classifier);
    let sea_level = preset_ref.map(|p| p.sea_level).unwrap_or(DEFAULT_SEA_LEVEL);
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

    // Elevation field (with the active sea level attached) used to compute each tile's climate
    // temperature. **Must exist before the prototype loop**, because since the climate-authority arc
    // (`docs/plan_climate_authority.md` §5.1) temperature *decides the biome* — the classifier gates
    // on the tile's climate band, so the band has to be known before terrain is assigned. This was
    // purely an ordering problem, not a data dependency: both sources below are already available
    // here (terrain classification itself consumes `bands.elevation`).
    let climate_elevation = bands
        .as_ref()
        .map(|bands_res| bands_res.elevation.clone())
        .unwrap_or_else(|| base_elevation_field.clone())
        .with_sea_level(sea_level);

    let mut tags_grid: Vec<sim_runtime::TerrainTags> = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let position = UVec2::new(x as u32, y as u32);
            let element = ElementKind::from_grid(position);
            let mut mountain_meta: Option<MountainMetadata> = None;
            let idx = y * width + x;
            // The tile's climate, computed BEFORE its biome so the biome can be derived from it.
            // Deliberately the **jittered** temperature (§8.2): band boundaries must come out
            // ragged, because the retired latitude gate drew clean horizontal edges that read as
            // artificial on a real map.
            let above_sea = climate_elevation.above_sea_normalized(position.x, position.y);
            let temperature = climate_temperature(
                y as u32,
                config.grid_size.y,
                above_sea,
                element,
                &config.climate,
            );
            let band = climate_band_for_temperature(temperature.to_f32(), &config.climate);
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
                            band,
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
                        band,
                    )
                }
            };
            let (mut terrain, mut terrain_tags) = if let Some(preset) = preset_ref {
                bias_terrain_for_preset(terrain, terrain_tags, preset, position, band)
            } else {
                (terrain, terrain_tags)
            };
            // Palette enforcement (`docs/plan_biome_palette.md` §3.5): the weight/climate
            // chains above cannot exclude highland/volcanic/polar/anomaly biomes, so any
            // off-palette result is remapped to the nearest allowed biome in its niche.
            // `is_polar` keeps the remap climate-safe (a polar wetland collapses to a
            // polar biome, not a temperate marsh).
            if let Some(ref palette) = biome_palette {
                let remapped = palette.remap(terrain, band.admits_cold_biomes());
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
                temperature,
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
    // The site budget is a share of the map's LAND, resolved through the config's one seam so
    // curation and anything that reports the budget cannot disagree about it.
    let target_total = food_overlay_cfg.site_budget(land_tiles);
    let mut module_candidates: std::collections::BTreeMap<FoodModule, Vec<FoodSiteCandidate>> =
        std::collections::BTreeMap::new();

    let mut province_region_layers: HashMap<ProvinceId, CultureLayerId> = HashMap::new();
    for (idx, proto) in prototypes.iter().enumerate() {
        let (generation, demand, efficiency) = proto.element.power_profile();
        let node_id = PowerNodeId(proto.position.y * config.grid_size.x + proto.position.x);
        let storage_capacity = (generation * scalar_from_f32(0.6) + scalar_from_f32(2.0))
            .clamp(scalar_from_f32(1.0), scalar_from_f32(40.0));
        let storage_level =
            (storage_capacity * scalar_from_f32(0.5)).clamp(scalar_zero(), storage_capacity);
        let tile_component = Tile {
            position: proto.position,
            element: proto.element,
            // The very temperature the biome gate read in the prototype loop — one climate
            // per tile, decided once (`docs/plan_climate_authority.md` §4).
            temperature: proto.temperature,
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
            let seasonal_weight = INITIAL_SEASONAL_WEIGHT;
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
        culture.attach_local(proto.position, parent_region);
        let modifiers = seeded_modifiers_for_position(proto.position);
        culture.apply_initial_modifiers(proto.position, modifiers);
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
    // Worldgen creates the world, so it creates the band id space: the allocator is built here and
    // inserted below, rather than taken as a `ResMut`. Taking it as a system param would oblige
    // every hand-rolled test `World` in the crate to remember to insert one first — 33 of them —
    // and each of those is a place to forget, which is the omission failure this whole arc is about.
    let mut band_ids = BandIdAllocator::default();
    // Resolved once for both arms: which arm spawns the band does not change what it is stocked with.
    let start_kit_equipment = equipment
        .as_ref()
        .map(|handle| handle.get())
        .unwrap_or_else(crate::equipment_config::EquipmentConfig::builtin);
    let start_kit_recipes = recipes
        .as_ref()
        .map(|handle| handle.get())
        .unwrap_or_else(crate::recipes_config::RecipesConfig::builtin);
    let start_kit_materials = materials
        .as_ref()
        .map(|handle| handle.get())
        .unwrap_or_else(crate::materials_config::MaterialsConfig::builtin);
    let start_kit = StartKit {
        equipment: &start_kit_equipment,
        recipes: &start_kit_recipes,
        materials: &start_kit_materials,
        working_fraction: demographics
            .as_ref()
            .map(|handle| handle.get().initial_distribution.working)
            .unwrap_or_else(|| {
                crate::demographics_config::DemographicsConfig::builtin()
                    .initial_distribution
                    .working
            }),
    };
    if config.start_profile_overrides.starting_units.is_empty() {
        spawn_default_population_clusters(
            &mut commands,
            &registry,
            &mut band_ids,
            &tiles,
            &tags_grid,
            width,
            height,
            start_x,
            start_y,
            config.population_cluster_stride,
            &mut cohort_index,
            &knowledge_fragments,
            &start_kit,
        );
    } else {
        spawn_profile_population(
            &mut commands,
            &registry,
            &mut band_ids,
            &tiles,
            &tags_grid,
            width,
            height,
            (start_x, start_y),
            &config.start_profile_overrides,
            &mut cohort_index,
            &knowledge_fragments,
            &start_kit,
        );
    }

    // Publish the counter with the ids it just handed out, so the next band spawned (an
    // expedition, a rollback) continues the sequence instead of colliding with a living band.
    commands.insert_resource(band_ids);
    commands.insert_resource(StartLocation::new(Some(UVec2::new(start_x, start_y))));
    commands.insert_resource(FoodSiteRegistry::new(curated_entries));

    // If we produced bands, use their restamped elevation field resource now
    if let Some(bands_res) = bands {
        commands.insert_resource(bands_res.elevation.clone());
        // Validate invariants and log
        validate_bands(&bands_res, config.grid_size);
    }

    let topology = PowerTopology::from_grid(
        tiles.len(),
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

/// Seed each freshly spawned cohort's demographics — age brackets + a carried food larder. Food is
/// band-local from day one — every band opens the game carrying its own reserve, so there is no
/// faction provisions pool to distribute.
///
/// This also used to drain a start profile's `trade_goods` grant into an opening trade-link openness
/// bonus, on a component nothing ever spawned — so the grant was deleted at startup for no effect.
/// **The shipped profile no longer makes that grant**, because with the drain gone it would sit in
/// [`FactionInventory`] forever: nothing anywhere reads that resource for a decision, and the
/// Inspector's Map tab renders it, so the player would see a permanently frozen stockpile of 40.
/// The `trade_goods` commodity key itself is retired (arc #527); what a cash Field, a hunt or a pen
/// actually pays beyond food is **material batches** on the band's own store
/// (`docs/plan_contact_and_logistics.md` §As-built).
pub fn apply_starting_inventory_effects(
    demographics: Res<DemographicsConfigHandle>,
    // `With<ResidentBand>`: only real bands are seeded with startup demographics + food reserves; an
    // expedition is seeded explicitly at launch from the home band's larder.
    mut cohorts: Query<&mut PopulationCohort, With<ResidentBand>>,
) {
    seed_cohort_demographics(&demographics.get(), &mut cohorts);
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
    band: ClimateBand,
) -> (sim_runtime::TerrainType, sim_runtime::TerrainTags) {
    let key = format!("{:?}", terrain);
    let biome_weight = preset.biome_weights.get(&key).copied().unwrap_or(1.0);
    let climate_weight = climate_weight_for_tags(preset, tags, band);
    let effective_weight = (biome_weight * climate_weight).clamp(0.0, 2.0);

    let noise = (tile_hash(position) & 0xFFFF) as f32 / 65535.0;
    let is_cold = band.admits_cold_biomes();
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

    if is_cold && result.0 == sim_runtime::TerrainType::FreshwaterMarsh {
        let fallback = sim_runtime::TerrainType::PeatHeath;
        let def = terrain_definition(fallback);
        result = (fallback, def.tags);
    } else if is_cold
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
    band: ClimateBand,
) -> f32 {
    let band_weight = preset
        .climate_band_weights
        .get(band.as_str())
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

fn climate_alignment_factor(band: ClimateBand, tags: sim_runtime::TerrainTags) -> f32 {
    use sim_runtime::TerrainTags as Tag;
    match band {
        // Polar and boreal share an alignment shape: both admit the cold biome ladder, so a
        // POLAR-tagged biome is fully at home in either and highland is a partial fit.
        ClimateBand::Polar | ClimateBand::Boreal => {
            if tags.contains(Tag::POLAR) {
                1.0
            } else if tags.contains(Tag::HIGHLAND) {
                0.5
            } else {
                0.0
            }
        }
        ClimateBand::Tropical => {
            if tags.contains(Tag::WETLAND) {
                1.0
            } else if tags.contains(Tag::FERTILE) && tags.contains(Tag::FRESHWATER) {
                0.6
            } else {
                0.0
            }
        }
        ClimateBand::Temperate => {
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
        /// The tile's climate band, resolved once from its temperature. Terrain is repainted by
        /// this pass but temperature is not, so the band is stable for the whole solve.
        band: ClimateBand,
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
                band: climate_band_for_temperature(tile.temperature.to_f32(), &config.climate),
            });
        } else {
            tile_info.push(TileInfo {
                entity,
                terrain: sim_runtime::TerrainType::DeepOcean,
                tags: sim_runtime::TerrainTags::WATER,
                position: UVec2::ZERO,
                mountain_kind: None,
                mountain_relief: 1.0,
                band: ClimateBand::Temperate,
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

    // The tag solver has NO water branch. Water share is an ELEVATION outcome: the land mask is a
    // pure threshold of the heightfield, and the contour anchor already puts the `target_land_pct`
    // quantile exactly on sea level. The retired branch converted arbitrary land tiles to `DeepOcean`
    // (and ocean back to `Tundra`/`AlluvialPlain`) with no elevation term at all, which is precisely
    // how a "water" tile ended up above sea level. A `Water` entry in `locked_terrain_tags` is now
    // inert rather than authoritative.
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
                    let replacement = if tile_info[idx].band.admits_cold_biomes() {
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
                        let replacement = if tile_info[idx].band.admits_cold_biomes() {
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
                        let replacement = if tile_info[idx].band.admits_cold_biomes() {
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
                            // Preserve hydrology-placed river deltas (see Wetland pass).
                            || info.terrain == sim_runtime::TerrainType::RiverDelta
                        {
                            return false;
                        }
                        if info.band.admits_cold_biomes() {
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
                            // Preserve hydrology-placed river deltas (see Wetland pass).
                            || info.terrain == sim_runtime::TerrainType::RiverDelta
                        {
                            continue;
                        }
                        if info.band.admits_cold_biomes() {
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
                        // THE CLIMATE VETO (`docs/plan_climate_authority.md` §5.4). A polar
                        // biome may only be stamped on a tile whose temperature actually admits
                        // one. This replaces a latitude band, and it is the reason the pass can
                        // now under-fill (see below).
                        info.band.admits_cold_biomes()
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
                            // The veto binds here too. This fallback loop previously had NO
                            // climate test of any kind, and painted `Tundra` at any latitude to
                            // hit the target — 64% of the measured warm-polar tiles came from
                            // exactly this loop. A target share is an input to generation, never
                            // a reassignment applied afterward.
                            || !tile_info[idx].band.admits_cold_biomes()
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
                // UNDER-FILL AND REPORT, never repaint (`docs/plan_climate_authority.md` §5.4).
                // The pass ran out of climate-eligible tiles before reaching the `Polar` target.
                // That is the honest outcome: the map is not cold enough to carry that share of
                // polar biomes, and the lever is the climate inputs, not the output.
                if delta > 0 {
                    tracing::info!(
                        target: "shadow_scale::mapgen",
                        tag = "Polar",
                        shortfall = delta,
                        target = want_polar,
                        actual = tag_ratio(&tile_info, sim_runtime::TerrainTags::POLAR),
                        "mapgen.tag_solver.under_filled_climate_gated"
                    );
                }
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
    registry: Res<TileRegistry>,
    mut tiles: Query<&mut Tile>,
) {
    let Some(palette) = palette else {
        return;
    };
    for &entity in registry.tiles.iter() {
        if let Ok(mut tile) = tiles.get_mut(entity) {
            if palette.contains(tile.terrain) {
                continue;
            }
            // Keyed on the tile's TEMPERATURE band, exactly like the prototype-loop remap. If this
            // post-solver clamp kept deciding by latitude it would re-stamp temperate biomes onto
            // cold tiles and silently undo the whole arc (`docs/plan_climate_authority.md` §5.1).
            let band = climate_band_for_temperature(tile.temperature.to_f32(), &config.climate);
            let remapped = palette.remap(tile.terrain, band.admits_cold_biomes());
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

/// Final food-module reconciliation — the last word on which food web a tile belongs to.
///
/// `spawn_initial_world` stamps `FoodModuleTag` exactly once, from `classify_food_module` on the
/// terrain **as it stands at spawn time**. Every later Startup stage then repaints that terrain:
/// `generate_hydrology` stamps `RiverDelta`/`Floodplain`/`FreshwaterMarsh`/`NavigableRiver` along
/// its channels and mouths, `apply_tag_budget_solver` and `apply_biome_palette_clamp` swap biomes
/// wholesale, and `reconcile_coastal_shelf` converts `DeepOcean` to `ContinentalShelf`. Nothing
/// re-read the tag, so a tile could publish a module describing terrain it no longer had — and a
/// tile whose *pre-hydrology* biome classified to `None` kept **no tag at all** after hydrology
/// turned it into a river delta. `spawn_initial_forage` is gated on the tag, so those deltas — the
/// richest human ground on the map — were seeded no `ForagePatch` and read "No forage" in the
/// client (issue #330).
///
/// This pass closes that ordering gap: it re-runs `classify_food_module` — the *same* function and
/// the *same* `tile.terrain`/`tile.terrain_tags` inputs as the initial stamp, so no new
/// classification policy is introduced — over the FINAL terrain and brings each tile's tag into
/// agreement (insert where a tag is now owed, retag in place where the module changed, remove where
/// the tile now classifies to nothing). An in-place retag **keeps the tile's existing
/// `seasonal_weight`**; only `module`/`kind` are authored by terrain. A tile whose module is already
/// correct is left untouched, so this adds no change-detection churn.
///
/// **Chain position.** Registered immediately after `reconcile_coastal_shelf` — the last stage that
/// touches terrain — and immediately before the three consumers of the tag: `place_wondrous_sites`
/// (reads `Option<&FoodModuleTag>`), `spawn_initial_forage` (gated on it), and `spawn_initial_graze`.
///
/// **`FoodSiteRegistry` is reconciled, not re-curated.** The curated site list is built in
/// `spawn_initial_world` from the same pre-hydrology classification and is published on the wire as
/// `foodModules`, so a repainted tile would carry a wrong label there too. Existing entries are
/// re-read against the final terrain (relabelled where the module moved, dropped where the tile now
/// classifies to `None`), but **no entries are added**: *which* tiles are curated is a spatial
/// bucket/quota decision made once during worldgen, and re-running that curation here would change
/// the map's start-site geography, which is out of scope for a correctness pass.
///
/// Deterministic: pure row-major iteration over `TileRegistry.tiles` and the site vec's existing
/// order — no `HashMap` iteration, no RNG. A pure function of the final terrain.
pub fn reconcile_food_modules(
    mut commands: Commands,
    registry: Res<TileRegistry>,
    mut food_sites: ResMut<FoodSiteRegistry>,
    mut tiles: Query<(Entity, &Tile, Option<&mut FoodModuleTag>)>,
) {
    let mut inserted = 0usize;
    let mut updated = 0usize;
    let mut removed = 0usize;

    for &entity in registry.tiles.iter() {
        let Ok((entity, tile, tag)) = tiles.get_mut(entity) else {
            continue;
        };
        let classified = classify_food_module(tile);
        match (classified, tag) {
            (Some(module), None) => {
                commands.entity(entity).insert(FoodModuleTag::new(
                    module,
                    INITIAL_SEASONAL_WEIGHT,
                    module.site_kind(),
                ));
                inserted += 1;
            }
            (Some(module), Some(mut tag)) => {
                if tag.module != module || tag.kind != module.site_kind() {
                    // Terrain authors module + kind; the seasonal weight is the tile's own state.
                    tag.module = module;
                    tag.kind = module.site_kind();
                    updated += 1;
                }
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<FoodModuleTag>();
                removed += 1;
            }
            (None, None) => {}
        }
    }

    let mut sites_updated = 0usize;
    let mut sites_dropped = 0usize;
    let mut reconciled_sites: Vec<FoodSiteEntry> = Vec::with_capacity(food_sites.sites().len());
    for entry in food_sites.iter() {
        let classified = registry
            .index(entry.position.x, entry.position.y)
            .and_then(|entity| tiles.get(entity).ok())
            .and_then(|(_, tile, _)| classify_food_module(tile));
        match classified {
            Some(module) => {
                let mut reconciled = entry.clone();
                if reconciled.module != module || reconciled.kind != module.site_kind() {
                    reconciled.module = module;
                    reconciled.kind = module.site_kind();
                    sites_updated += 1;
                }
                reconciled_sites.push(reconciled);
            }
            None => sites_dropped += 1,
        }
    }
    if sites_updated > 0 || sites_dropped > 0 {
        food_sites.set_sites(reconciled_sites);
    }

    info!(
        target: "shadow_scale::mapgen",
        "mapgen.food_modules.reconciled inserted={} updated={} removed={} sites_updated={} sites_dropped={}",
        inserted, updated, removed, sites_updated, sites_dropped
    );
}

/// **Move each curated gathering marker onto the best ground in its own spatial bucket, counting
/// fresh water as quality** (issue #466).
///
/// **Why this exists at all.** The `FoodSiteRegistry` is not decoration: `food_modules` on the wire is
/// the *only* source the client's `_forage_compose_available` gate reads, so a hex without a marker
/// offers no Forage button — and `sow` requires a band already foraging the tile. The ~130 markers
/// (`site_land_fraction` 0.08 of land) are therefore the whole of the ground a player can climb the
/// plant ladder on, out of ~2,000 food-bearing hexes. Curation chose them by spatial bucket, latitude
/// quota and min-spacing — and fresh water, the thing `plant:field`'s site rule actually demands, was
/// invisible to it.
///
/// **What this pass overrides in curation's ranking.** `compare_food_site` sorts on `seasonal_weight`
/// DESC and then on `preferred` DESC. The *seasonal-weight* term is inert — every candidate ships
/// `INITIAL_SEASONAL_WEIGHT = 1.0`, so the first key is always `Equal` — but the `preferred` tie-break
/// is live: it is `FoodModulePreference::matches` against `start_profile_overrides.food_modules`, and
/// the shipped `start_profiles.json` names `savanna_grassland` primary and `riverine_delta` secondary.
/// So `preferred` is what actually decided which hex inside a bucket carried the marker, and this pass
/// **deliberately overrides it**, scoring on forage capacity + fresh water alone. The accepted
/// consequence: a marker may move off a start-profile-preferred module onto wetter ground. That is
/// tolerable because `riverine_delta` *is* the fresh-water module, so the bias largely reinforces the
/// preference rather than fighting it — and adding `preferred` as a second term in the score would
/// make "why did this marker move" far harder to reason about for a tie-break that only ever ran
/// because the term above it was constant.
///
/// **Why it is a separate pass rather than a fix inside curation.** Curation runs inside
/// `spawn_initial_world`, which is **before `generate_hydrology`** — at that moment the map has lakes
/// but no rivers, no deltas, no floodplains and no `river_edges`, i.e. none of the fresh water the rule
/// is about. Curation cannot simply be moved later either: `best_start_tile` consumes the curated list
/// and the starting population is spawned around its answer, all in the same system. So the selection
/// stays where it is (start geography is untouched) and this pass **re-ranks the result** over the
/// final terrain.
///
/// **It relocates, it never adds or drops.** The marker count, the per-bucket quota and the latitude
/// distribution are all preserved by construction, because every move stays inside the marker's own
/// bucket and the min-spacing rule is re-checked against every other marker. That is the "re-rank
/// only" decision: gathering stays exactly as scarce as it was, it just sits on the river valleys.
///
/// **Chain position.** After `reconcile_food_modules` — which is what guarantees every surviving entry
/// still classifies to a real module on the final terrain — and before the wire ever sees the list.
///
/// **Deterministic**: candidates are scored, then sorted by score descending with an explicit
/// `(y, x)` tie-break; markers are visited in their existing list order; no `HashMap` iteration and no
/// RNG.
///
/// **`fresh_water_site_weight = 0.0` is a no-op because of the early return, not because of the
/// score.** With the bonus off, quality is bare forage capacity — and curation did not pick the
/// highest-capacity hex in each bucket, so a pass that ran at weight 0 would still relocate (measured
/// at 82 of ~130 markers on one seed). The guard is therefore load-bearing, and
/// [`FoodSiteWaterBiasReport`] is what makes it *testable*: the pass publishes how many markers it
/// moved, on every path, and the zero-weight arm asserts that number is `0`.
pub fn bias_food_sites_toward_fresh_water(
    registry: Res<TileRegistry>,
    config: Res<SimulationConfig>,
    labor: Res<crate::LaborConfigHandle>,
    snapshot_overlays: Res<SnapshotOverlaysConfigHandle>,
    mut food_sites: ResMut<FoodSiteRegistry>,
    mut report: ResMut<FoodSiteWaterBiasReport>,
    tiles: Query<&Tile>,
) {
    let overlays = snapshot_overlays.get();
    let food_cfg = overlays.food();
    let water_weight = food_cfg.fresh_water_site_weight();

    let width = registry.width.max(1);
    let height = registry.height.max(1);
    let wrap = config.map_topology.wrap_horizontal;

    let tile_at = |pos: UVec2| -> Option<&Tile> {
        registry
            .index(pos.x, pos.y)
            .and_then(|entity| tiles.get(entity).ok())
    };
    let is_watered = |pos: UVec2| -> bool {
        tile_at(pos).is_some_and(|tile| {
            crate::tile_is_fresh_watered(tile, width, height, wrap, |neighbor| {
                tile_at(neighbor).map(|t| t.terrain_tags)
            })
        })
    };
    let count_watered = |entries: &[FoodSiteEntry]| -> usize {
        entries
            .iter()
            .filter(|entry| is_watered(entry.position))
            .count()
    };

    if water_weight <= 0.0 {
        // The report is written on this path too, and `moved`/`relabelled` written as **zero**: it is
        // the evidence that the kill switch really killed the pass, so a count left behind by a
        // previous build would be exactly the failure it exists to detect.
        *report = FoodSiteWaterBiasReport {
            moved: 0,
            relabelled: 0,
            watered: count_watered(food_sites.sites()),
            total: food_sites.sites().len(),
        };
        info!(
            target: "shadow_scale::mapgen",
            "mapgen.food_sites.water_bias.skipped=weight_zero"
        );
        return;
    }

    let forage = &labor.get().forage;
    let bucket_cols = BUCKET_COLS.max(1);
    let bucket_rows = BUCKET_ROWS.max(1);
    let bucket_of = |pos: UVec2| -> usize {
        let bx = ((pos.x * bucket_cols) / width).min(bucket_cols - 1);
        let by = ((pos.y * bucket_rows) / height).min(bucket_rows - 1);
        (by * bucket_cols + bx) as usize
    };

    // **Site quality** — the tile's own forage capacity plus what fresh water is worth, in the same
    // units. Read through `tile_forage_capacity`, never a private table, so a marker is ranked by the
    // very number that sizes the patch it will carry and that `plant:field`'s floor is compared against.
    let quality = |tile: &Tile| -> f32 {
        crate::tile_forage_capacity(forage, tile)
            + if is_watered(tile.position) {
                water_weight
            } else {
                0.0
            }
    };

    // One pass over the map builds every bucket's candidate list. A candidate must pass **both**
    // gates, and they are not redundant: `classify_food_module` says *which kind* of gathering a hex
    // offers — a marker on ground that classifies to nothing is exactly what `reconcile_food_modules`
    // drops — while the capacity gate says whether there is *any* food to gather at all. Four biomes
    // classify to a real module while `capacity_by_biome` reads `NO_FORAGE_CAPACITY`: `SaltFlat`
    // (→ SemiAridScrub), `HydrothermalVentField` (→ WetlandSwamp), `Glacier` (→ BorealArctic via the
    // POLAR tag) and `ActiveVolcanoSlope` (→ MontaneHighland via HIGHLAND). `spawn_initial_forage`
    // seeds them **no `ForagePatch`**. Since the water bonus alone can outscore a modest dry hex, a
    // marker relocated onto one of those would publish a Forage affordance — `food_modules` is the
    // only thing the client's gate reads — over ground the sim deliberately left patchless. Same
    // constant as `spawn_initial_forage`, so the two cannot drift.
    let bucket_count = (bucket_cols * bucket_rows) as usize;
    let mut buckets: Vec<Vec<(UVec2, f32)>> = vec![Vec::new(); bucket_count];
    for y in 0..height {
        for x in 0..width {
            let pos = UVec2::new(x, y);
            let Some(tile) = tile_at(pos) else {
                continue;
            };
            if classify_food_module(tile).is_none() {
                continue;
            }
            if crate::tile_forage_capacity(forage, tile) <= crate::NO_FORAGE_CAPACITY {
                continue;
            }
            buckets[bucket_of(pos)].push((pos, quality(tile)));
        }
    }
    for bucket in buckets.iter_mut() {
        // Score DESC, then `(y, x)` ASC — a total order, so two builds cannot disagree about which of
        // two equally good hexes a marker lands on.
        bucket.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(Ordering::Equal)
                .then_with(|| (a.0.y, a.0.x).cmp(&(b.0.y, b.0.x)))
        });
    }

    let min_spacing = food_cfg.min_site_spacing().max(1);
    let min_spacing_sq = (min_spacing * min_spacing) as i64;
    let mut positions: Vec<UVec2> = food_sites.iter().map(|entry| entry.position).collect();
    // Squared *offset-grid* distance, matching curation's own spacing test rather than inventing a
    // second one — the two rules have to agree or a relocation could land inside a gap curation forbade.
    let too_close = |a: UVec2, b: UVec2| -> bool {
        let dx = (a.x as i64 - b.x as i64).abs();
        let dy = (a.y as i64 - b.y as i64).abs();
        dx * dx + dy * dy < min_spacing_sq
    };

    let mut moved = 0usize;
    let mut relabelled = 0usize;
    let mut entries = food_sites.sites().to_vec();
    for idx in 0..entries.len() {
        let current = entries[idx].position;
        let Some(current_tile) = tile_at(current) else {
            continue;
        };
        let current_quality = quality(current_tile);

        let mut best: Option<(UVec2, f32)> = None;
        for &(pos, score) in buckets[bucket_of(current)].iter() {
            // The list is score-descending, so once it drops to what we already have nothing further
            // in this bucket can beat it.
            if score <= current_quality {
                break;
            }
            if pos == current {
                continue;
            }
            if positions
                .iter()
                .enumerate()
                .any(|(other, &taken)| other != idx && too_close(pos, taken))
            {
                continue;
            }
            best = Some((pos, score));
            break;
        }

        let Some((target, _)) = best else {
            continue;
        };
        let Some(target_tile) = tile_at(target) else {
            continue;
        };
        // Terrain authors module + kind at the destination — the same division of labour
        // `reconcile_food_modules` keeps — while the marker's own `seasonal_weight` travels with it.
        let Some(module) = classify_food_module(target_tile) else {
            continue;
        };
        if entries[idx].module != module || entries[idx].kind != module.site_kind() {
            entries[idx].module = module;
            entries[idx].kind = module.site_kind();
            relabelled += 1;
        }
        entries[idx].position = target;
        positions[idx] = target;
        moved += 1;
    }

    let watered_sites = count_watered(&entries);
    let total = entries.len();
    food_sites.set_sites(entries);
    *report = FoodSiteWaterBiasReport {
        moved,
        relabelled,
        watered: watered_sites,
        total,
    };

    info!(
        target: "shadow_scale::mapgen",
        "mapgen.food_sites.water_bias moved={} relabelled={} watered={}/{} weight={}",
        moved, relabelled, watered_sites, total, water_weight
    );
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

/// **The three tables a spawn's start kit is resolved from** — *what* the band owns, and the *grade*
/// its gear is stamped with. Bundled because they travel together down every spawn helper and are
/// read at exactly one place ([`BandEquipment::start_stocked_owned`]); passing three refs through
/// four signatures would say the same thing four times.
struct StartKit<'a> {
    equipment: &'a crate::equipment_config::EquipmentConfig,
    recipes: &'a crate::recipes_config::RecipesConfig,
    materials: &'a crate::materials_config::MaterialsConfig,
    /// **What share of a spawned band's head count is working-age** —
    /// `demographics.initial_distribution.working`, the very split
    /// [`crate::systems::apply_starting_inventory_effects`] applies at Startup.
    ///
    /// A spawn stocks a **party's worth** of each item now, and the party is the band's *workers*:
    /// children and elders hold nothing. The brackets are seeded a stage later than this spawn, so
    /// the head count is all there is here and the same distribution has to be read twice — from
    /// the one config that owns it, rather than from a second constant that would drift the moment
    /// the demographics were retuned.
    working_fraction: f32,
}

/// **THE MATERIALS A SPAWNED BAND IS SENT OUT HOLDING** — one batch per material whose roster entry
/// declares a `start_stock`, at `per_worker × workers` and the reading that block states
/// (`docs/plan_standing_upkeep.md` §4.9 item 12).
///
/// # ⛔ `StartKit::materials` WAS NEVER A STOCK, AND THIS IS THE PATH THAT MAKES IT ONE
///
/// That field is the materials **table**, carried into the spawn so an equipment batch can resolve
/// its anchor grade through `recipes.anchor_grade_for_item`. **Nothing in `StartKit` deposited a
/// material batch**, and a spawned band's `LocalStore.materials` was empty — which is why the pen's
/// hurdles have a recipe whose `wood` has no producer and no other way in.
///
/// **Today `wood` is the only declarer**, and the lever is on the *material* rather than in a start
/// profile because the roster is where a material is described — one home per fact. The amount is a
/// **config lever**, not a constant: it is the only thing between a band and its first pen until
/// forest foraging lands, so it has to be tunable without a rebuild.
///
/// **A band with no workers stocks nothing**, which needs no special case: the amount is `0` and
/// `LocalStore::deposit_material` refuses a non-positive deposit.
fn start_stocked_materials(
    materials: &crate::materials_config::MaterialsConfig,
    workers: f32,
) -> LocalStore {
    let mut store = LocalStore::new();
    for (id, def) in materials.materials() {
        let Some(stock) = def.start_stock.as_ref() else {
            continue;
        };
        // The batch's merge key is derived from the stated reading through the materials table's own
        // lookup, exactly as a yield edge's is — the store stores, it does not interpret.
        let Some(band) = materials.band_key(id, &stock.characteristics) else {
            continue;
        };
        store.deposit_material(
            id,
            band,
            scalar_from_f32(stock.per_worker * workers.max(0.0)),
            &stock.characteristics,
        );
    }
    store
}

#[allow(clippy::too_many_arguments)]
fn spawn_default_population_clusters(
    commands: &mut Commands,
    registry: &GenerationRegistry,
    band_ids: &mut BandIdAllocator,
    tiles: &[Entity],
    tags_grid: &[sim_runtime::TerrainTags],
    width: usize,
    height: usize,
    start_x: u32,
    start_y: u32,
    stride_tiles: u32,
    cohort_index: &mut usize,
    knowledge: &[KnowledgeFragment],
    start_kit: &StartKit<'_>,
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
                    band_ids,
                    tiles[idx],
                    1_000,
                    cohort_index,
                    None,
                    knowledge,
                    start_kit,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_profile_population(
    commands: &mut Commands,
    registry: &GenerationRegistry,
    band_ids: &mut BandIdAllocator,
    tiles: &[Entity],
    tags_grid: &[sim_runtime::TerrainTags],
    width: usize,
    height: usize,
    start: (u32, u32),
    overrides: &StartProfileOverrides,
    cohort_index: &mut usize,
    knowledge: &[KnowledgeFragment],
    start_kit: &StartKit<'_>,
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
                    band_ids,
                    tiles[idx],
                    spec.band_size(),
                    cohort_index,
                    Some(marker),
                    knowledge,
                    start_kit,
                );
                spawned_total += 1;
            }
        }
    }
    if spawned_total == 0 {
        spawn_default_population_clusters(
            commands,
            registry,
            band_ids,
            tiles,
            tags_grid,
            width,
            height,
            start.0,
            start.1,
            1,
            cohort_index,
            knowledge,
            start_kit,
        );
    } else {
        info!(
            target: "shadow_scale::campaign",
            "start_profile.units.spawned units={}",
            spawned_total
        );
    }
}

#[allow(clippy::too_many_arguments)] // one more id source than the linter's threshold likes
fn spawn_population_entity(
    commands: &mut Commands,
    registry: &GenerationRegistry,
    band_ids: &mut BandIdAllocator,
    tile_entity: Entity,
    size: u32,
    cohort_index: &mut usize,
    marker: Option<StartingUnit>,
    knowledge: &[KnowledgeFragment],
    start_kit: &StartKit<'_>,
) {
    let generation = registry.assign_for_index(*cohort_index);
    *cohort_index = cohort_index.saturating_add(1);
    // **Floored, because `available_workers` floors.** The stock is sized against the party the
    // band can actually field, so the two readings of "this band's workers" agree and the ledger
    // stays reproducible from the cohort. Resolved once, because the **material** stock and the
    // **equipment** stock are both sized against it and *"a party's worth"* has to mean one thing in
    // this function.
    let party_workers = (size as f32 * start_kit.working_fraction).floor();
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
        // **…but the MATERIAL stock is seeded HERE, beside the kit** — the material half of the
        // standing upkeep needs a band to have something to build a pen out of, and no producer
        // yields `wood` until forest foraging lands (`docs/plan_standing_upkeep.md` §4.9 item 12).
        // It rides the spawn rather than `apply_starting_inventory_effects` because it needs no
        // demographic split — the party's own worker count, the same one the kit is sized against —
        // and because the roster is where a material is described.
        stores: start_stocked_materials(start_kit.materials, party_workers),
        morale: scalar_from_f32(0.6),
        last_food_consumption: 0.0,
        last_turn_transfer_received: 0.0,
        last_turn_transfer_sent: 0.0,
        last_morale_delta: scalar_zero(),
        last_morale_cause: MoraleCause::None,
        last_morale_contributions: MoraleContributions::default(),
        last_fertility_factors: Default::default(),
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
    // **The band starts KITTED, and it is stated rather than implied**: one unit of every item some
    // kit carries, at the tier that ships known. An absent ledger entry means NOT OWNED since the
    // count slice, so a spawn that inserted `Default` would send the band out bare-handed — this is
    // the flip's load-bearing call site (`.claude/rules/core_sim/equipment.md`). One unit is one
    // item's `starting_durability`, which is exactly the life the shipped opening has always had.
    // **A PARTY'S WORTH, sized off the workers this band will have** — one unit of each item would
    // arm one person, so the band's own head count times the working-age share is what the stock is
    // measured against (`.claude/rules/core_sim/equipment.md` → "a spawn stocks a party's worth").
    entity.insert(BandEquipment::start_stocked_owned(
        start_kit.equipment,
        start_kit.recipes,
        start_kit.materials,
        party_workers,
    ));
    // **And an EMPTY BENCH.** `Default` is *no job*, so a fresh band crafts nothing until the
    // player puts a recipe on it — there is no opening move where everyone builds tools first
    // (`docs/plan_crafting_and_materials.md` §5). Inserted here rather than on first use so the
    // bench's crew comes out of the same worker pool the assignment loop reads, on turn one.
    entity.insert(crate::components::BandBench::default());
    // The band's durable identity — see `BandId`. Allocated here rather than derived from position
    // because several bands can share a hex and a band outlives the hex it started on.
    entity.insert(band_ids.allocate());
    // The fractional carry that turns the demographic rates into whole-person feed events. Spawned
    // empty here, beside the cohort whose flows it accumulates — a band without one runs the model
    // but reports no births/deaths (see `simulate_population`'s query).
    entity.insert(DemographicFlowAccumulator::default());
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
            polar_max_temp: 0.0,
            boreal_max_temp: 5.0,
            temperate_max_temp: 18.0,
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
        world.init_resource::<crate::equipment_config::EquipmentConfigHandle>();
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
    fn fertile_lock_skips_the_cold_band() {
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
                let is_cold_row = y == 0 || y == 5;
                let terrain = if is_cold_row {
                    sim_runtime::TerrainType::RockyReg
                } else {
                    sim_runtime::TerrainType::SemiAridScrub
                };
                // The fertile pass is gated on the tile's CLIMATE BAND, so the fixture states a
                // climate rather than relying on a row index. Every tile previously carried a
                // placeholder 0.5°, which is inside the cold ladder — the pass would (correctly)
                // skip the whole map.
                let temperature = if is_cold_row {
                    scalar_from_f32(-10.0)
                } else {
                    scalar_from_f32(15.0)
                };
                let def = terrain_definition(terrain);
                let entity = world
                    .spawn(Tile {
                        position,
                        element,
                        temperature,
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
                "cold-band tile should not be converted to fertile terrain"
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
        world.init_resource::<crate::equipment_config::EquipmentConfigHandle>();
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

        let mut polar_land = 0usize;
        let mut polar_alluvial = 0usize;
        let mut polar_freshwater_marsh = 0usize;

        for &entity in registry.tiles.iter() {
            let tile = query.get(&world, entity).expect("tile component");
            if tile.terrain_tags.contains(TerrainTags::WATER) {
                continue;
            }
            // Keyed on the tile's CLIMATE BAND, not its latitude — the invariant this arc
            // strengthens is "a cold tile carries a cold biome", which latitude never expressed.
            if !climate_band_for_temperature(tile.temperature.to_f32(), &config.climate)
                .admits_cold_biomes()
            {
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
            "expected cold land tiles to evaluate climate constraints"
        );
        assert_eq!(
            polar_alluvial, 0,
            "expected no alluvial plains in the cold band (found {} of {})",
            polar_alluvial, polar_land
        );
        assert_eq!(
            polar_freshwater_marsh, 0,
            "expected no freshwater marsh in the cold band (found {} of {})",
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
        config.map_seed = crate::HARNESS_MAP_SEED;
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
        world.init_resource::<crate::equipment_config::EquipmentConfigHandle>();
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

    fn configured_world(provisions: i64) -> World {
        let mut config = SimulationConfig::builtin();
        config.start_profile_overrides.inventory = vec![InventoryEntry {
            item: "provisions".to_string(),
            quantity: provisions,
        }];
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
        world.init_resource::<crate::equipment_config::EquipmentConfigHandle>();
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
        let mut world = configured_world(0);
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
}
