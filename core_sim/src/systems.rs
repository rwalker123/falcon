use std::{
    cmp::{max, min, Ordering},
    collections::{HashMap, HashSet, VecDeque},
};

use bevy::{ecs::system::SystemParam, math::UVec2, prelude::*};
use log::{debug, info, warn};
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};
use serde_json::json;

use crate::map_preset::{MapPreset, MapPresetsHandle, TerrainClassifierConfig};
#[cfg(test)]
use crate::snapshot_overlays_config::SnapshotOverlaysConfig;
use crate::{
    components::{
        fragments_from_contract, fragments_to_contract, ElementKind, FaunaPursuit,
        FaunaPursuitMode, FollowPolicy, HarvestAssignment, HarvestTaskKind, KnowledgeFragment,
        LocalStore, LogisticsLink, MoraleCause, MoraleContributions, MountainMetadata,
        PendingMigration, PopulationCohort, PowerNode, ScoutAssignment, StartingUnit, Tile,
        TradeLink, FOOD,
    },
    culture::{
        CultureEffectsCache, CultureLayerId, CultureManager, CultureSchismEvent,
        CultureTensionEvent, CultureTensionKind, CultureTensionRecord, CultureTraitAxis,
        CULTURE_TRAIT_AXES,
    },
    culture_corruption_config::{CorruptionSeverityConfig, CultureCorruptionConfigHandle},
    demographics_config::{DemographicsConfig, DemographicsConfigHandle, DemographicsConsumption},
    fauna::{net_biomass_delta, EcologyPhase, HerdDensityMap, HerdRegistry},
    fauna_config::FaunaConfigHandle,
    food::{classify_food_module, classify_food_module_from_traits, FoodModule, FoodModuleTag},
    generations::GenerationRegistry,
    heightfield::{build_elevation_field, ElevationField},
    hydrology::HydrologyState,
    influencers::{InfluencerCultureResonance, InfluencerImpacts},
    mapgen::MountainType,
    mapgen::{build_bands, validate_bands, TerrainBand, WorldGenSeed},
    orders::{FactionId, FactionRegistry},
    power::{
        PowerGridNodeTelemetry, PowerGridState, PowerIncident, PowerIncidentSeverity, PowerNodeId,
        PowerTopology,
    },
    provinces::{ProvinceId, ProvinceMap},
    resources::{
        ClimateConfig, CommandEventEntry, CommandEventKind, CommandEventLog,
        CorruptionExposureRecord, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage,
        DiscoveryProgressLedger, FactionInventory, FogRevealLedger, FoodSiteEntry,
        FoodSiteRegistry, MoistureRaster, SentimentAxisBias, SimulationConfig, SimulationTick,
        StartLocation, TileRegistry, TradeDiffusionRecord, TradeTelemetry,
    },
    scalar::{scalar_from_f32, scalar_from_u32, scalar_one, scalar_zero, Scalar},
    snapshot_overlays_config::SnapshotOverlaysConfigHandle,
    start_profile::{
        FoodModulePreference, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle,
        StartProfileOverrides, StartingUnitSpec,
    },
    terrain::{terrain_definition, terrain_for_position_with_classifier, TerrainDefinition},
    turn_pipeline_config::TurnPipelineConfigHandle,
    wellbeing_config::{ProductivityConfig, WellbeingConfig, WellbeingConfigHandle},
};
use sim_runtime::{
    apply_openness_decay, merge_fragment_payload, scale_migration_fragments, CorruptionSubsystem,
    TradeLeakCurve,
};

const POLAR_LATITUDE_THRESHOLD: f32 =
    TerrainClassifierConfig::default_values().polar_latitude_cutoff;
const HERD_TRADE_DIFFUSION_BONUS: f32 = 0.25;
const PLAYER_FACTION: FactionId = FactionId(0);
const BUCKET_COLS: u32 = 6;
const BUCKET_ROWS: u32 = 6;
const LATITUDE_BANDS: usize = 3;
const MIN_NEARBY_CURATED_SITES: usize = 2;
const NO_FOOD_SITE_PENALTY: i32 = 18;
const LOW_FOOD_SITE_PENALTY: i32 = 6;

#[derive(Clone, Debug)]
struct TilePrototype {
    position: UVec2,
    element: ElementKind,
    terrain: sim_runtime::TerrainType,
    tags: sim_runtime::TerrainTags,
    mountain: Option<MountainMetadata>,
    food_module: Option<FoodModule>,
}

#[derive(Event, Debug, Clone)]
pub struct TradeDiffusionEvent {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
    pub via_migration: bool,
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

#[derive(Event, Debug, Clone)]
pub struct MigrationKnowledgeEvent {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
}

#[derive(SystemParam)]
pub struct LogisticsSimParams<'w, 's> {
    pub config: Res<'w, SimulationConfig>,
    pub impacts: Res<'w, InfluencerImpacts>,
    pub effects: Res<'w, CultureEffectsCache>,
    pub ledgers: Res<'w, CorruptionLedgers>,
    pub severity_config: Res<'w, CultureCorruptionConfigHandle>,
    pub pipeline_config: Res<'w, TurnPipelineConfigHandle>,
    pub links: Query<'w, 's, (Entity, &'static mut LogisticsLink)>,
    pub tiles: Query<'w, 's, &'static mut Tile>,
}

#[derive(SystemParam)]
pub struct TradeDiffusionParams<'w, 's> {
    pub config: Res<'w, SimulationConfig>,
    pub telemetry: ResMut<'w, TradeTelemetry>,
    pub discovery: ResMut<'w, DiscoveryProgressLedger>,
    pub ledgers: Res<'w, CorruptionLedgers>,
    pub severity_config: Res<'w, CultureCorruptionConfigHandle>,
    pub pipeline_config: Res<'w, TurnPipelineConfigHandle>,
    pub tick: Res<'w, SimulationTick>,
    pub events: EventWriter<'w, TradeDiffusionEvent>,
    pub links: Query<'w, 's, (&'static LogisticsLink, &'static mut TradeLink)>,
    pub tiles: Query<'w, 's, &'static Tile>,
    pub herd_density: Res<'w, HerdDensityMap>,
}

#[derive(SystemParam)]
pub struct PowerSimParams<'w, 's> {
    pub nodes: Query<'w, 's, (Entity, &'static Tile, &'static mut PowerNode)>,
    pub config: Res<'w, SimulationConfig>,
    pub topology: Res<'w, PowerTopology>,
    pub grid_state: ResMut<'w, PowerGridState>,
    pub impacts: Res<'w, InfluencerImpacts>,
    pub effects: Res<'w, CultureEffectsCache>,
    pub ledgers: Res<'w, CorruptionLedgers>,
    pub severity_config: Res<'w, CultureCorruptionConfigHandle>,
    pub pipeline_config: Res<'w, TurnPipelineConfigHandle>,
}

fn corruption_multiplier(
    ledgers: &CorruptionLedgers,
    subsystem: CorruptionSubsystem,
    penalty: Scalar,
    config: &CorruptionSeverityConfig,
) -> Scalar {
    let raw_intensity = ledgers.total_intensity(subsystem).max(0);
    if raw_intensity == 0 {
        return Scalar::one();
    }
    let intensity = Scalar::from_raw(raw_intensity).clamp(Scalar::zero(), Scalar::one());
    let mut reduction = intensity * penalty;
    reduction = reduction.clamp(Scalar::zero(), config.max_penalty_ratio());
    (Scalar::one() - reduction).clamp(config.min_output_multiplier(), Scalar::one())
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
            let (terrain, terrain_tags) = if let Some(preset) = preset_ref {
                bias_terrain_for_preset(terrain, terrain_tags, preset, position, config.grid_size.y)
            } else {
                (terrain, terrain_tags)
            };
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
            mountain: proto.mountain,
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
    mut cohorts: Query<&mut PopulationCohort>,
    mut trade_links: Query<&mut TradeLink>,
) {
    seed_cohort_demographics(&demographics.get(), &mut cohorts);
    apply_trade_goods_bonus(&mut inventory, &mut trade_links);
}

/// Split each cohort's head-count into the three age brackets, seed its larder with
/// `startup.food_reserve_days` turns of its own food demand, and apply the well-fed morale bonus.
fn seed_cohort_demographics(
    config: &DemographicsConfig,
    cohorts: &mut Query<&mut PopulationCohort>,
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

    let mut river_mask = vec![false; total];
    if let Some(hydro) = hydro.as_ref() {
        for river in hydro.rivers.iter() {
            for point in river.path.iter() {
                if point.x < registry.width && point.y < registry.height {
                    let idx = (point.y * registry.width + point.x) as usize;
                    if idx < river_mask.len() {
                        river_mask[idx] = true;
                    }
                }
            }
        }
    }

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

/// Relax material temperatures and adjust masses using deterministic rules. The relaxation target is
/// the tile's latitude + elevation + jitter climate temperature (recomputed deterministically from
/// its position/elevation/element), so the field converges to the climate model rather than the old
/// element checkerboard. Worldgen seeds each tile at exactly this value, so turn 1 has no jump.
pub fn simulate_materials(
    config: Res<SimulationConfig>,
    elevation: Res<ElevationField>,
    mut tiles: Query<&mut Tile>,
) {
    let grid_height = config.grid_size.y;
    for mut tile in tiles.iter_mut() {
        let above_sea = elevation.above_sea_normalized(tile.position.x, tile.position.y);
        let target = climate_temperature(
            tile.position.y,
            grid_height,
            above_sea,
            tile.element,
            &config.climate,
        );
        let delta = (target - tile.temperature) * config.temperature_lerp;
        let conductivity = tile.element.conductivity();
        tile.temperature += delta * conductivity;
        let flux = tile.element.mass_flux() * config.mass_flux_epsilon;
        let new_mass = tile.mass + flux;
        tile.mass = new_mass.clamp(config.mass_bounds.0, config.mass_bounds.1);
    }
}

/// Move resources along logistics links based on mass gradients.
pub fn simulate_logistics(mut params: LogisticsSimParams) {
    let logistics_cfg = params.pipeline_config.config().logistics();
    let corruption_cfg = params.severity_config.config().corruption();
    let corruption_factor = corruption_multiplier(
        &params.ledgers,
        CorruptionSubsystem::Logistics,
        params.config.corruption_logistics_penalty,
        corruption_cfg,
    );
    let flow_gain = (params.config.logistics_flow_gain
        * params.impacts.logistics_multiplier
        * params.effects.logistics_multiplier
        * corruption_factor)
        .clamp(logistics_cfg.flow_gain_min(), logistics_cfg.flow_gain_max());
    let mut links: Vec<_> = params.links.iter_mut().collect();
    links.sort_by_key(|(entity, _)| entity.to_bits());
    for (_, mut link) in links {
        let Ok([mut source, mut target]) = params.tiles.get_many_mut([link.from, link.to]) else {
            link.flow = scalar_zero();
            continue;
        };
        let source_profile = terrain_definition(source.terrain);
        let target_profile = terrain_definition(target.terrain);
        let penalty_avg = (source_profile.logistics_penalty + target_profile.logistics_penalty)
            .max(logistics_cfg.penalty_min());
        let attrition_avg = (source_profile.attrition_rate + target_profile.attrition_rate)
            .clamp(0.0, logistics_cfg.attrition_max());
        let penalty_scalar = Scalar::from_f32(penalty_avg.max(logistics_cfg.penalty_scalar_min()));
        let attrition_scalar = Scalar::from_f32(attrition_avg);
        let effective_gain =
            (flow_gain / penalty_scalar).clamp(logistics_cfg.effective_gain_min(), flow_gain);
        let capacity = ((link.capacity * corruption_factor) / penalty_scalar)
            .max(logistics_cfg.capacity_min());
        let gradient = source.mass - target.mass;
        let transfer_raw = (gradient * effective_gain).clamp(-capacity, capacity);
        let delivered = transfer_raw * (Scalar::one() - attrition_scalar);
        source.mass -= transfer_raw;
        target.mass += delivered;
        link.flow = delivered;
    }
}

/// Diffuse knowledge along trade links using openness-derived leak timers.
pub fn trade_knowledge_diffusion(mut params: TradeDiffusionParams) {
    params.telemetry.reset_turn();
    let leak_curve = TradeLeakCurve::new(
        params.config.trade_leak_min_ticks,
        params.config.trade_leak_max_ticks,
        params.config.trade_leak_exponent,
    );
    let corruption_cfg = params.severity_config.config().corruption();
    let trade_multiplier = corruption_multiplier(
        &params.ledgers,
        CorruptionSubsystem::Trade,
        params.config.corruption_trade_penalty,
        corruption_cfg,
    );
    let trade_cfg = params.pipeline_config.config().trade();
    let tariff_base = params.config.base_trade_tariff;

    for (logistics, mut trade) in params.links.iter_mut() {
        trade.throughput = logistics.flow * trade_multiplier;
        let tariff_max = tariff_base * trade_cfg.tariff_max_scalar();
        trade.tariff = (tariff_base * trade_multiplier).clamp(trade_cfg.tariff_min(), tariff_max);
        trade.openness = trade.openness.clamp(scalar_zero(), scalar_one());
        trade.openness = Scalar::from_raw(apply_openness_decay(
            trade.openness.raw(),
            trade.decay.raw(),
        ));

        if trade.leak_timer > 0 {
            trade.leak_timer = trade.leak_timer.saturating_sub(1);
        }

        let density_hint = match (
            params.tiles.get(logistics.from),
            params.tiles.get(logistics.to),
        ) {
            (Ok(from_tile), Ok(to_tile)) => params
                .herd_density
                .normalized_pair_average(from_tile.position, to_tile.position),
            _ => params.herd_density.normalized_average(),
        };

        if trade.leak_timer == 0 {
            let fragment = if !trade.pending_fragments.is_empty() {
                trade.pending_fragments.remove(0)
            } else {
                let discovery_id = trade
                    .last_discovery
                    .unwrap_or((trade.from_faction.0 << 8) | trade.to_faction.0);
                KnowledgeFragment::new(
                    discovery_id,
                    params.config.trade_leak_progress,
                    Scalar::one(),
                )
            };

            let delta = fragment.progress;
            if delta > scalar_zero() {
                let discovery_id = fragment.discovery_id;
                let density_multiplier =
                    scalar_from_f32(1.0 + density_hint * HERD_TRADE_DIFFUSION_BONUS);
                let adjusted_delta =
                    (delta * density_multiplier).clamp(Scalar::zero(), Scalar::one());
                let _ =
                    params
                        .discovery
                        .add_progress(trade.to_faction, discovery_id, adjusted_delta);
                params.telemetry.tech_diffusion_applied =
                    params.telemetry.tech_diffusion_applied.saturating_add(1);
                params.telemetry.push_record(TradeDiffusionRecord {
                    tick: params.tick.0,
                    from: trade.from_faction,
                    to: trade.to_faction,
                    discovery_id,
                    delta: adjusted_delta,
                    via_migration: false,
                    herd_density: density_hint,
                });
                params.events.send(TradeDiffusionEvent {
                    tick: params.tick.0,
                    from: trade.from_faction,
                    to: trade.to_faction,
                    discovery_id,
                    delta: adjusted_delta,
                    via_migration: false,
                });
                trade.last_discovery = Some(discovery_id);
            }

            trade.leak_timer = leak_curve.ticks_for_openness(trade.openness.raw());
            if trade.leak_timer == 0 {
                trade.leak_timer = params.config.trade_leak_min_ticks.max(1);
            }
        }
    }
}

/// Publish trade telemetry counters for downstream logging/metrics.
pub fn publish_trade_telemetry(telemetry: Res<TradeTelemetry>, tick: Res<SimulationTick>) {
    let snapshot = json!({
        "tick": tick.0,
        "tech_diffusion_applied": telemetry.tech_diffusion_applied,
        "migration_transfers": telemetry.migration_transfers,
        "records": telemetry
            .records
            .iter()
            .take(24)
            .map(|record| {
                    json!({
                        "from": record.from.0,
                        "to": record.to.0,
                        "discovery": record.discovery_id,
                        "delta": record.delta.to_f32(),
                        "via_migration": record.via_migration,
                        "herd_density": record.herd_density,
                    })
            })
            .collect::<Vec<_>>(),
        "records_truncated": telemetry.records.len().saturating_sub(24),
    });

    match serde_json::to_string(&snapshot) {
        Ok(payload) => debug!("trade.telemetry {}", payload),
        Err(_) => debug!(
            "trade.telemetry tick={} trade.tech_diffusion_applied={} trade.migration_transfers={} records={}",
            tick.0,
            telemetry.tech_diffusion_applied,
            telemetry.migration_transfers,
            telemetry.records.len()
        ),
    }
}

/// A cohort's age brackets + food larder at the start of a demographic turn.
#[derive(Debug, Clone, Copy)]
struct DemographicState {
    children: Scalar,
    working: Scalar,
    elders: Scalar,
    food_store: Scalar,
}

/// One turn's food demand for the given age brackets: per-capita draw × weighted mouths
/// (dependents eat less than a working adult). Shared by consumption and the campaign-start
/// larder seeding so they can never drift apart.
pub(crate) fn food_demand(
    children: Scalar,
    working: Scalar,
    elders: Scalar,
    consumption: &DemographicsConsumption,
) -> Scalar {
    let weighted_mouths = children * scalar_from_f32(consumption.child_factor)
        + working * scalar_from_f32(consumption.working_factor)
        + elders * scalar_from_f32(consumption.elder_factor);
    scalar_from_f32(consumption.per_capita_draw) * weighted_mouths
}

/// Combined per-turn death fraction for one age bracket: a starvation term plus a uniform cold
/// term, capped at 1.0. The starvation term scales with the food `deficit_fraction` and this
/// bracket's vulnerability but is **never allowed to exceed the deficit itself** — a 10% food
/// shortfall impacts at most 10% of the bracket. Cold is a separate, non-food mortality.
fn death_fraction(
    deficit_fraction: Scalar,
    starvation_rate: Scalar,
    vulnerability: f32,
    cold_fraction: Scalar,
) -> Scalar {
    let starvation = min(
        deficit_fraction * starvation_rate * scalar_from_f32(vulnerability),
        deficit_fraction,
    );
    min(starvation + cold_fraction, scalar_one())
}

/// One turn of the demographic model for a single cohort (pure — no ECS): draw per-capita food
/// from the local larder, then resolve scarcity/cold deaths, births, maturation, aging, and
/// elder mortality. All bracket flows use the *opening* bracket values and are applied together,
/// so a newborn does not mature the same turn. The total is clamped to the global cap.
fn advance_demographics(
    state: DemographicState,
    temp_diff: Scalar,
    max_cap: Scalar,
    demo: &DemographicsConfig,
) -> DemographicState {
    let DemographicState {
        children: children0,
        working: working0,
        elders: elders0,
        food_store,
    } = state;

    // 1. Food consumption from the band's own larder (dependents eat less than a worker).
    let demand = food_demand(children0, working0, elders0, &demo.consumption);
    let consumed = min(demand, food_store);
    let remaining_food = food_store - consumed;
    let has_demand = demand > scalar_zero();
    let deficit = demand - consumed; // >= 0 (consumed <= demand)
    let deficit_fraction = if has_demand {
        deficit / demand
    } else {
        scalar_zero()
    };
    let fed_ratio = if has_demand {
        consumed / demand
    } else {
        scalar_one()
    };
    // Larder buffer beyond one turn's demand → fertility bonus.
    let surplus_ratio = if has_demand {
        min(remaining_food / demand, scalar_one())
    } else {
        scalar_one()
    };

    // 2. Deaths: starvation (scales with the food deficit, dependents more vulnerable, but never
    // more than the deficit itself) plus cold (temperature deviation beyond tolerance).
    let scarcity = &demo.scarcity;
    let starvation_rate = scalar_from_f32(scarcity.starvation_mortality);
    let cold = &demo.cold;
    let cold_excess = temp_diff - scalar_from_f32(cold.temp_tolerance);
    let cold_fraction = if cold_excess > scalar_zero() {
        min(
            cold_excess * scalar_from_f32(cold.mortality_scale),
            scalar_from_f32(cold.max_mortality),
        )
    } else {
        scalar_zero()
    };
    let child_deaths = children0
        * death_fraction(
            deficit_fraction,
            starvation_rate,
            scarcity.child_vulnerability,
            cold_fraction,
        );
    let working_deaths = working0
        * death_fraction(
            deficit_fraction,
            starvation_rate,
            scarcity.working_vulnerability,
            cold_fraction,
        );
    let elder_deaths = elders0
        * death_fraction(
            deficit_fraction,
            starvation_rate,
            scarcity.elder_vulnerability,
            cold_fraction,
        );

    // 3. Births → children, from the working (reproductive) bracket, gated by food + surplus.
    // Births are morale-INDEPENDENT (wellbeing model, `docs/plan_civ_wellbeing.md`): contentment
    // doesn't change procreation — low morale relocates people or drags output, it never suppresses
    // births or causes faction population loss.
    let births_cfg = &demo.births;
    let fertility = scalar_from_f32(births_cfg.birth_rate)
        * fed_ratio
        * (scalar_one() + scalar_from_f32(births_cfg.surplus_bonus) * surplus_ratio);
    let births = working0 * fertility;

    // 4. Aging flows.
    let maturation = children0 * scalar_from_f32(demo.maturation_rate);
    let aging = working0 * scalar_from_f32(demo.aging_rate);
    let elder_mortality = elders0 * scalar_from_f32(demo.elder_mortality_rate);

    // Apply all flows simultaneously, flooring each bracket at zero.
    let mut children = max(
        children0 + births - maturation - child_deaths,
        scalar_zero(),
    );
    let mut working = max(
        working0 + maturation - aging - working_deaths,
        scalar_zero(),
    );
    let mut elders = max(
        elders0 + aging - elder_mortality - elder_deaths,
        scalar_zero(),
    );

    // Aggregate safety clamp to the global population cap.
    let total = children + working + elders;
    if total > max_cap && total > scalar_zero() {
        let scale = max_cap / total;
        children *= scale;
        working *= scale;
        elders *= scale;
    }

    DemographicState {
        children,
        working,
        elders,
        food_store: remaining_food,
    }
}

/// Config levers for [`tile_morale_pressure`] — the place-based (negative) morale terms. Pulled
/// from `SimulationConfig` (temperature) and the population block of `turn_pipeline_config.json`
/// (terrain scales) so the sim and the snapshot's `habitability` read from one source.
pub struct MoralePressureConfig {
    pub ambient_temperature: Scalar,
    pub temperature_morale_penalty: Scalar,
    /// Dead-band (°) around `ambient_temperature` within which climate bleeds **no** morale — only
    /// the deviation beyond it is penalized, so temperate mid-latitudes hold morale.
    pub temperature_morale_tolerance: Scalar,
    pub attrition_penalty_scale: Scalar,
    pub hardness_penalty_scale: Scalar,
}

/// The tile-intrinsic, per-turn morale *drain* broken into its two place-based drivers (each ≥ 0;
/// bigger = worse). This is the "how harsh is it to live on this tile" signal — it excludes base
/// growth and crisis/sentiment (unrest), which are not properties of the place.
pub struct TileMoralePressure {
    /// Terrain attrition + logistics-hardness drain.
    pub terrain: Scalar,
    /// Temperature-difference (comfort) drain.
    pub cold: Scalar,
}

impl TileMoralePressure {
    /// Total tile-intrinsic morale drain (`terrain + cold`, ≥ 0). This is the snapshot's
    /// `habitability` value.
    pub fn total(&self) -> Scalar {
        self.terrain + self.cold
    }
}

/// Compute the tile-intrinsic per-turn morale drain for a tile's terrain + temperature. Shared by
/// `simulate_population` (for the actual morale update + dominant-cause attribution) and the
/// snapshot's `habitability` export so the two never drift.
pub fn tile_morale_pressure(
    terrain: &TerrainDefinition,
    temperature: Scalar,
    cfg: &MoralePressureConfig,
) -> TileMoralePressure {
    let terrain_attrition_penalty =
        scalar_from_f32(terrain.attrition_rate) * cfg.attrition_penalty_scale;
    let hardness_excess = (terrain.logistics_penalty - 1.0).max(0.0);
    let terrain_hardness_penalty = scalar_from_f32(hardness_excess) * cfg.hardness_penalty_scale;
    let temp_diff = (temperature - cfg.ambient_temperature).abs();
    let temp_excess = (temp_diff - cfg.temperature_morale_tolerance).max(scalar_zero());
    TileMoralePressure {
        terrain: terrain_attrition_penalty + terrain_hardness_penalty,
        cold: temp_excess * cfg.temperature_morale_penalty,
    }
}

/// Layer 2 (wellbeing) — map a band's morale to its discontented share. `0` at/above
/// `content_morale`, rising linearly to `1` at/below `floor_morale`. See
/// `docs/plan_civ_wellbeing.md`.
pub fn discontent_fraction(
    morale: Scalar,
    cfg: &crate::wellbeing_config::DiscontentConfig,
) -> Scalar {
    let content = scalar_from_f32(cfg.content_morale);
    let floor = scalar_from_f32(cfg.floor_morale);
    let span = content - floor;
    if span <= scalar_zero() {
        return scalar_zero();
    }
    ((content - morale) / span).clamp(scalar_zero(), scalar_one())
}

/// Layer 3a (wellbeing) — the discontent entry of the productivity modifier stack:
/// `max(floor_mult, 1 − discontent_fraction × discontent_weight)`. A fully-discontented band still
/// produces `floor_mult` of its base output (morale drags labor, never zeroes it).
pub fn discontent_output_modifier(discontent_fraction: Scalar, cfg: &ProductivityConfig) -> Scalar {
    (scalar_one() - discontent_fraction * scalar_from_f32(cfg.discontent_weight))
        .max(scalar_from_f32(cfg.floor_mult))
}

/// Layer 3a (wellbeing) — the band's output multiplier: the **product** of every active
/// productivity modifier (`output = base × Π(modifiers)`). Phase 1 has one entry (discontent);
/// future education / technology / government modifiers multiply in here with a one-line addition,
/// so every yield site (forage/hunt/follow/husbandry) stays a single `output_multiplier` call.
pub fn output_multiplier(cohort: &PopulationCohort, cfg: &WellbeingConfig) -> Scalar {
    let mut m = scalar_one();
    m *= discontent_output_modifier(cohort.discontent_fraction, &cfg.productivity);
    // future: education, technology, government modifiers multiply in here.
    m
}

/// Layer 3b (wellbeing) — migration's morale-scaled move fraction (decoupled from
/// `discontent_fraction`, which is productivity-only): `max_rate × clamp((morale_threshold − morale)
/// / morale_threshold, 0, 1)`. `0` at morale ≥ `morale_threshold` (0.25), ramping to `max_rate`
/// (0.15) at rock-bottom morale. The band sheds `total × move_fraction` people this turn.
pub fn migration_move_fraction(
    morale: Scalar,
    cfg: &crate::wellbeing_config::MigrationConfig,
) -> Scalar {
    let threshold = scalar_from_f32(cfg.morale_threshold);
    if threshold <= scalar_zero() {
        return scalar_zero();
    }
    let ramp = ((threshold - morale) / threshold).clamp(scalar_zero(), scalar_one());
    scalar_from_f32(cfg.max_rate) * ramp
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn simulate_population(
    config: Res<SimulationConfig>,
    registry: Res<FactionRegistry>,
    impacts: Res<InfluencerImpacts>,
    effects: Res<CultureEffectsCache>,
    pipeline_config: Res<TurnPipelineConfigHandle>,
    demographics: Res<DemographicsConfigHandle>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tiles: Query<&Tile>,
    mut cohorts: Query<&mut PopulationCohort>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
    mut telemetry: ResMut<TradeTelemetry>,
    mut trade_events: EventWriter<TradeDiffusionEvent>,
    mut migration_events: EventWriter<MigrationKnowledgeEvent>,
    tick: Res<SimulationTick>,
) {
    let population_cfg = pipeline_config.config().population();
    let demo = demographics.get();
    let wellbeing = wellbeing_config.get();
    let max_cap_scalar = scalar_from_u32(config.population_cap);
    let morale_pressure_cfg = MoralePressureConfig {
        ambient_temperature: config.ambient_temperature,
        temperature_morale_penalty: config.temperature_morale_penalty,
        temperature_morale_tolerance: config.temperature_morale_tolerance,
        attrition_penalty_scale: population_cfg.attrition_penalty_scale(),
        hardness_penalty_scale: population_cfg.hardness_penalty_scale(),
    };
    for mut cohort in cohorts.iter_mut() {
        // Age the band every turn (before any early-out) so the migration gate below sees an
        // accurate settled duration even for cohorts whose home tile briefly can't be resolved.
        cohort.age_turns = cohort.age_turns.saturating_add(1);
        let Ok(tile) = tiles.get(cohort.home) else {
            cohort.morale = scalar_zero();
            continue;
        };
        let terrain_profile = terrain_definition(tile.terrain);
        let temp_diff = (tile.temperature - config.ambient_temperature).abs();
        // Place-based (negative) morale terms, from the one shared source (also the snapshot's
        // `habitability`), so sim and snapshot never drift.
        let pressure =
            tile_morale_pressure(&terrain_profile, tile.temperature, &morale_pressure_cfg);
        // Layer 1 (wellbeing): the morale delta is the signed sum of named contributors, so a
        // future factor is a new `MoraleFactor` variant + one field here — not a rewrite. The
        // contribution set doubles as the client's per-band morale breakdown. `unrest` = crisis
        // impacts + cultural sentiment (signed; may be positive).
        let contributions = MoraleContributions {
            settling: config.population_growth_rate,
            terrain: -pressure.terrain,
            climate: -pressure.cold,
            unrest: impacts.morale_delta + effects.morale_bias,
        };
        let morale_delta = contributions.total();
        // Attribute the dominant *negative* driver when morale fell (else `None`). Starvation is
        // intentionally excluded — it is surfaced through the days-of-food path, not morale.
        cohort.last_morale_delta = morale_delta;
        cohort.last_morale_cause = if morale_delta < scalar_zero() {
            contributions.dominant_negative_cause()
        } else {
            MoraleCause::None
        };
        cohort.last_morale_contributions = contributions;
        cohort.morale = (cohort.morale + morale_delta).clamp(scalar_zero(), scalar_one());

        // Layer 2 (wellbeing): map morale → the discontented share of the band. `0` at/above
        // `content_morale`, rising to `1` at/below `floor_morale`. Drives the productivity
        // modifier stack (this turn's payouts) and discontent-driven migration (below).
        cohort.discontent_fraction = discontent_fraction(cohort.morale, &wellbeing.discontent);

        // Demographic model: consume the band's local food, then resolve deaths, births,
        // maturation, and aging (see `advance_demographics`).
        let outcome = advance_demographics(
            DemographicState {
                children: cohort.children,
                working: cohort.working,
                elders: cohort.elders,
                food_store: cohort.stores.get(FOOD),
            },
            temp_diff,
            max_cap_scalar,
            &demo,
        );
        cohort.children = outcome.children;
        cohort.working = outcome.working;
        cohort.elders = outcome.elders;
        cohort.stores.set(FOOD, outcome.food_store);
        cohort.sync_size();

        // A band's population only emigrates once it has settled for a while — this gates the
        // high-morale knowledge-migration so a freshly-spawned (e.g. well-fed starting) band can't
        // defect to a neighbor on turn one.
        if cohort.migration.is_none()
            && cohort.age_turns >= population_cfg.migration_min_settled_turns() as u32
            && cohort.morale > population_cfg.migration_morale_threshold()
            && !cohort.knowledge.is_empty()
        {
            if let Some(&destination) = registry
                .factions
                .iter()
                .find(|&&faction| faction != cohort.faction)
            {
                let migration_eta = population_cfg.migration_eta_ticks();
                let source_contract = fragments_to_contract(&cohort.knowledge);
                let scaled = scale_migration_fragments(
                    &source_contract,
                    config.migration_fragment_scaling.raw(),
                    config.migration_fidelity_floor.raw(),
                );
                if !scaled.is_empty() {
                    cohort.migration = Some(PendingMigration {
                        destination,
                        eta: migration_eta,
                        fragments: fragments_from_contract(&scaled),
                    });
                }
            }
        }

        if let Some(mut migration) = cohort.migration.take() {
            if migration.eta > 0 {
                migration.eta -= 1;
            }

            if migration.eta == 0 {
                let source_faction = cohort.faction;
                for fragment in &migration.fragments {
                    if fragment.progress <= scalar_zero() {
                        continue;
                    }
                    let delta = fragment.progress;
                    discovery.add_progress(migration.destination, fragment.discovery_id, delta);
                    telemetry.tech_diffusion_applied =
                        telemetry.tech_diffusion_applied.saturating_add(1);
                    telemetry.migration_transfers = telemetry.migration_transfers.saturating_add(1);
                    telemetry.push_record(TradeDiffusionRecord {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                        via_migration: true,
                        herd_density: 0.0,
                    });
                    trade_events.send(TradeDiffusionEvent {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                        via_migration: true,
                    });
                    migration_events.send(MigrationKnowledgeEvent {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                    });
                }

                let payload_contract = fragments_to_contract(&migration.fragments);
                let mut knowledge_contract = fragments_to_contract(&cohort.knowledge);
                merge_fragment_payload(
                    &mut knowledge_contract,
                    &payload_contract,
                    Scalar::one().raw(),
                );
                cohort.knowledge = fragments_from_contract(&knowledge_contract);
                cohort.faction = migration.destination;
            } else {
                cohort.migration = Some(migration);
            }
        }
    }
}

/// Compute the tile position at a given progress fraction along a line.
fn interpolate_tile_position(from: UVec2, to: UVec2, remaining: u32, total: u32) -> UVec2 {
    if total == 0 || remaining >= total {
        return from;
    }
    if remaining == 0 {
        return to;
    }
    let progress = 1.0 - (remaining as f32 / total as f32);
    UVec2::new(
        (from.x as f32 + (to.x as f32 - from.x as f32) * progress).round() as u32,
        (from.y as f32 + (to.y as f32 - from.y as f32) * progress).round() as u32,
    )
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_harvest_assignments(
    mut commands: Commands,
    mut inventory: ResMut<FactionInventory>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
    tile_registry: Res<TileRegistry>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tiles: Query<&Tile>,
    mut cohorts: Query<(Entity, &mut PopulationCohort, &mut HarvestAssignment)>,
) {
    let wellbeing = wellbeing_config.get();
    for (entity, mut cohort, mut assignment) in cohorts.iter_mut() {
        if assignment.travel_remaining > 0 {
            assignment.travel_remaining -= 1;

            // Update current_tile to interpolated position
            if let Ok(home_tile) = tiles.get(cohort.home) {
                let current_pos = interpolate_tile_position(
                    home_tile.position,
                    assignment.target_coords,
                    assignment.travel_remaining,
                    assignment.travel_total,
                );
                if let Some(tile_entity) = tile_registry.index(current_pos.x, current_pos.y) {
                    cohort.current_tile = tile_entity;
                }
            }

            if assignment.travel_remaining == 0 {
                cohort.home = assignment.target_tile;
                cohort.current_tile = assignment.target_tile;
            }
            continue;
        }
        if assignment.gather_remaining > 0 {
            assignment.gather_remaining -= 1;
            if assignment.gather_remaining > 0 {
                continue;
            }
        } else {
            continue;
        }

        // Productivity modifier stack (wellbeing): scale the yield by the band's current output
        // multiplier at PAYOUT (discontent drags labor). One call — future modifiers slot into
        // `output_multiplier`, not here.
        let mult = output_multiplier(&cohort, &wellbeing);
        // FOOD income is fully fractional end-to-end: the reward is already `Scalar` (fractional at
        // source), so don't re-round the payout — a sub-1 forage yield must still reach the larder.
        let provisions = assignment.provisions_reward.max(scalar_zero()) * mult;
        let trade_goods = (Scalar::from_i64(assignment.trade_goods_reward.max(0)) * mult)
            .round()
            .to_i64_whole();

        // Provisions the band forages go into its own local larder (carried food), not the
        // faction pool. Trade goods remain a faction-global stockpile.
        if provisions > scalar_zero() {
            cohort.stores.add(FOOD, provisions);
        }
        if trade_goods > 0 {
            inventory.add_stockpile(assignment.faction, "trade_goods", trade_goods);
        }

        let (event_kind, label_display) = match assignment.kind {
            HarvestTaskKind::Harvest => (CommandEventKind::Forage, "Harvest"),
            HarvestTaskKind::Hunt => (CommandEventKind::Hunt, "Hunt"),
        };
        let detail = format!(
            "status=complete action={} module={} provisions={} trade_goods={} travel_turns={} gather_turns={} started_tick={}",
            assignment.kind.as_str(),
            assignment.module.as_str(),
            provisions.round().to_i64_whole().max(0),
            trade_goods.max(0),
            assignment.travel_total,
            assignment.gather_total,
            assignment.started_tick
        );

        event_log.push(CommandEventEntry::new(
            tick.0,
            event_kind,
            assignment.faction,
            format!(
                "{} {} -> ({}, {})",
                assignment.band_label,
                label_display.to_lowercase(),
                assignment.target_coords.x,
                assignment.target_coords.y
            ),
            Some(detail),
        ));

        commands.entity(entity).remove::<HarvestAssignment>();
    }
}

pub fn advance_scout_assignments(
    mut commands: Commands,
    mut fog: ResMut<FogRevealLedger>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    mut cohorts: Query<(Entity, &mut PopulationCohort, &mut ScoutAssignment)>,
) {
    for (entity, mut cohort, mut assignment) in cohorts.iter_mut() {
        if assignment.travel_remaining > 0 {
            assignment.travel_remaining -= 1;

            // Update current_tile to interpolated position
            if let Ok(home_tile) = tiles.get(cohort.home) {
                let current_pos = interpolate_tile_position(
                    home_tile.position,
                    assignment.target_coords,
                    assignment.travel_remaining,
                    assignment.travel_total,
                );
                if let Some(tile_entity) = tile_registry.index(current_pos.x, current_pos.y) {
                    cohort.current_tile = tile_entity;
                }
            }

            if assignment.travel_remaining == 0 {
                cohort.home = assignment.target_tile;
                cohort.current_tile = assignment.target_tile;
            }
            continue;
        }

        let expires_at = tick.0.saturating_add(assignment.reveal_duration);
        fog.queue(
            assignment.target_coords,
            assignment.reveal_radius,
            expires_at,
        );

        if assignment.morale_gain > 0.0 {
            cohort.morale = (cohort.morale + Scalar::from_f32(assignment.morale_gain))
                .clamp(Scalar::zero(), Scalar::one());
        }

        let detail = format!(
            "status=complete radius={} morale_boost={:.2} travel_turns={} started_tick={}",
            assignment.reveal_radius,
            assignment.morale_gain,
            assignment.travel_total,
            assignment.started_tick
        );

        event_log.push(CommandEventEntry::new(
            tick.0,
            CommandEventKind::Scout,
            assignment.faction,
            format!(
                "{} -> ({}, {})",
                assignment.band_label, assignment.target_coords.x, assignment.target_coords.y
            ),
            Some(detail.clone()),
        ));

        info!(
            "command.scout_area.completed faction={} band={} x={} y={} radius={}",
            assignment.faction.0,
            assignment.band_label,
            assignment.target_coords.x,
            assignment.target_coords.y,
            assignment.reveal_radius
        );

        commands.entity(entity).remove::<ScoutAssignment>();
    }
}

/// Chebyshev (king-move) distance in tiles.
fn chebyshev_tiles(a: UVec2, b: UVec2) -> u32 {
    a.x.abs_diff(b.x).max(a.y.abs_diff(b.y))
}

/// One pursuit step: move each axis toward `to` by up to `max_step` tiles.
fn step_toward(from: UVec2, to: UVec2, max_step: u32) -> UVec2 {
    let axis = |f: u32, t: u32| -> u32 {
        let delta = (t as i64 - f as i64).clamp(-(max_step as i64), max_step as i64);
        (f as i64 + delta).max(0) as u32
    };
    UVec2::new(axis(from.x, to.x), axis(from.y, to.y))
}

/// Advance fauna hunt pursuits: each turn the band re-targets the herd's *live*
/// position (herds already moved in `TurnStage::Logistics`), steps toward it, and on
/// closing to within `pursuit_radius` resolves a one-shot take of biomass →
/// provisions/trade. The group's biomass draws down here and regrows in `advance_herds`;
/// a group taken to zero despawns there (local extinction).
#[allow(clippy::too_many_arguments)]
pub fn advance_fauna_pursuits(
    mut commands: Commands,
    mut registry: ResMut<HerdRegistry>,
    mut inventory: ResMut<FactionInventory>,
    mut event_log: ResMut<CommandEventLog>,
    mut fog: ResMut<FogRevealLedger>,
    tick: Res<SimulationTick>,
    tile_registry: Res<TileRegistry>,
    fauna_config: Res<FaunaConfigHandle>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tiles: Query<&Tile>,
    mut cohorts: Query<(Entity, &mut PopulationCohort, &mut FaunaPursuit)>,
) {
    let fauna = fauna_config.get();
    let wellbeing = wellbeing_config.get();
    let hunt = &fauna.hunt;
    let follow = &fauna.follow;
    let ecology = &fauna.ecology;
    let husbandry = &fauna.husbandry;
    let market = &fauna.market;
    for (entity, mut cohort, mut pursuit) in cohorts.iter_mut() {
        // Herd still around? (may have despawned via extinction or another hunter).
        let Some(herd_pos) = registry.find(&pursuit.fauna_id).map(|herd| herd.position()) else {
            event_log.push(CommandEventEntry::new(
                tick.0,
                CommandEventKind::Hunt,
                pursuit.faction,
                format!(
                    "{} lost {} (herd dispersed)",
                    pursuit.band_label, pursuit.fauna_id
                ),
                Some("status=cancelled reason=herd_gone".to_string()),
            ));
            commands.entity(entity).remove::<FaunaPursuit>();
            continue;
        };

        pursuit.elapsed_turns += 1;
        if pursuit.elapsed_turns > hunt.max_pursuit_turns {
            event_log.push(CommandEventEntry::new(
                tick.0,
                CommandEventKind::Hunt,
                pursuit.faction,
                format!(
                    "{} gave up chasing {}",
                    pursuit.band_label, pursuit.fauna_id
                ),
                Some("status=cancelled reason=elusive".to_string()),
            ));
            commands.entity(entity).remove::<FaunaPursuit>();
            continue;
        }

        let band_pos = tiles
            .get(cohort.current_tile)
            .map(|tile| tile.position)
            .unwrap_or(herd_pos);

        // Not close enough: step toward the herd's live tile and wait for next turn.
        // Keep `home` on the band's real position so per-turn systems that read it
        // (e.g. `simulate_population`'s environmental inputs) don't use the stale
        // origin as the band ranges across the map.
        if chebyshev_tiles(band_pos, herd_pos) > hunt.pursuit_radius {
            let next = step_toward(band_pos, herd_pos, hunt.pursuit_tiles_per_turn);
            if let Some(tile_entity) = tile_registry.index(next.x, next.y) {
                cohort.current_tile = tile_entity;
                cohort.home = tile_entity;
            }
            continue;
        }

        // Close enough: resolve the take against the herd's live biomass. Hunt is a
        // one-shot; Follow keeps shadowing and auto-hunts per policy each turn.
        let Some(herd) = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == pursuit.fauna_id)
        else {
            commands.entity(entity).remove::<FaunaPursuit>();
            continue;
        };
        let take = match pursuit.mode {
            FaunaPursuitMode::Hunt => hunt.take_from(herd.biomass),
            // Sustain/Surplus take is sized off the group's net regrowth; a collapsing
            // (sub-threshold) group has no surplus to give, so `.max(0.0)` yields zero.
            FaunaPursuitMode::Follow { policy } => match policy {
                FollowPolicy::Sustain => {
                    net_biomass_delta(herd.biomass, herd.carrying_capacity, ecology).max(0.0)
                }
                FollowPolicy::Surplus => {
                    net_biomass_delta(herd.biomass, herd.carrying_capacity, ecology).max(0.0)
                        * follow.surplus_multiplier
                }
                // Market: a large commercial share each turn — drives the group down fast
                // (into the Phase D collapse) but yields boosted trade goods (see below).
                FollowPolicy::Market => market.take_fraction * herd.biomass,
                FollowPolicy::Eradicate => hunt.take_from(herd.biomass),
            },
        }
        .clamp(0.0, herd.biomass);
        herd.biomass -= take;
        // Phase E husbandry: a Sustain follow on a Thriving group tames it over time.
        // Progress accrues here (Population) for the following faction; untended groups
        // decay in `advance_husbandry` (Logistics), so domestication needs a *sustained*
        // Sustain-follow. Once progress reaches 1.0 the group is domesticated.
        if matches!(
            pursuit.mode,
            FaunaPursuitMode::Follow {
                policy: FollowPolicy::Sustain
            }
        ) && herd.ecology_phase == EcologyPhase::Thriving
        {
            herd.accrue_domestication(pursuit.faction, husbandry.progress_per_turn);
        }
        let biomass_left = herd.biomass;
        let species = herd.species.clone();

        // Market hunting sells its harvest — boosted trade goods; every other mode uses
        // the normal rate. Provisions stay at the normal rate for all modes.
        let trade_multiplier = if matches!(
            pursuit.mode,
            FaunaPursuitMode::Follow {
                policy: FollowPolicy::Market
            }
        ) {
            market.trade_goods_multiplier
        } else {
            1.0
        };
        // Productivity modifier stack (wellbeing): a discontented band works its kill less
        // effectively — scale the take's payout by the band's output multiplier at PAYOUT.
        let mult = output_multiplier(&cohort, &wellbeing).to_f32();
        // FOOD income is fully fractional: a sustainable take that yields < 1 provisions/turn must
        // still credit the larder (a Sustain follow yields ~0.3 food/turn — rounding drops it).
        let provisions = scalar_from_f32(take * hunt.provisions_per_biomass * mult);
        let trade_goods =
            (take * hunt.trade_goods_per_biomass * trade_multiplier * mult).round() as i64;
        // The pursuing band carries its kill in its own larder; trade goods are faction-global.
        if provisions > scalar_zero() {
            cohort.stores.add(FOOD, provisions);
        }
        if trade_goods > 0 {
            inventory.add_stockpile(pursuit.faction, "trade_goods", trade_goods);
        }

        match pursuit.mode {
            FaunaPursuitMode::Hunt => {
                // One-shot: a Hunt completion is a notable event → command feed.
                let detail = format!(
                    "status=complete action=hunt herd={} species={} take={:.0} biomass_left={:.0} provisions={} trade_goods={} elapsed_turns={} started_tick={}",
                    pursuit.fauna_id,
                    species,
                    take,
                    biomass_left,
                    provisions.max(scalar_zero()),
                    trade_goods.max(0),
                    pursuit.elapsed_turns,
                    pursuit.started_tick
                );
                event_log.push(CommandEventEntry::new(
                    tick.0,
                    CommandEventKind::Hunt,
                    pursuit.faction,
                    format!("{} hunt -> {}", pursuit.band_label, pursuit.fauna_id),
                    Some(detail),
                ));
                cohort.home = cohort.current_tile;
                commands.entity(entity).remove::<FaunaPursuit>();
            }
            FaunaPursuitMode::Follow { .. } => {
                // A follow yields every turn; log the tick to the analytics stream
                // rather than the small command feed (it would otherwise crowd out
                // one-shot events like harvest/scout/hunt completions). The queued
                // FollowHerd event (handle_follow_herd) already announces the order,
                // and cancellations still surface in the feed above.
                tracing::info!(
                    target: "shadow_scale::analytics",
                    event = "follow_tick",
                    herd = %pursuit.fauna_id,
                    band = %pursuit.band_label,
                    species = %species,
                    take,
                    biomass_left,
                    provisions = %provisions.max(scalar_zero()),
                    trade_goods = trade_goods.max(0),
                );
                // Reset the give-up counter and grant the small non-food tracking
                // benefit (fog reveal pulse + morale). Keep `home` on the band's
                // current tile so a persistent follow's population/environment inputs
                // track its real location rather than the starting tile.
                cohort.home = cohort.current_tile;
                pursuit.elapsed_turns = 0;
                cohort.morale = (cohort.morale + Scalar::from_f32(follow.morale_gain))
                    .clamp(Scalar::zero(), Scalar::one());
                let expires_at = tick.0.saturating_add(follow.reveal_duration_turns);
                fog.queue(herd_pos, follow.reveal_radius, expires_at);
            }
        }
    }
}

/// Layer 3b (wellbeing) — tech-gated migration: relocate-or-stay, population conserved within the
/// faction (`docs/plan_civ_wellbeing.md`). Runs in the Population stage **after** demographics so
/// morale is current. **Decoupled from `discontent_fraction`** (productivity-only): migration has its
/// own morale-scaled onset at `migration.morale_threshold` (0.25). Each band below the threshold
/// sheds `total × migration_move_fraction(morale)` people, composed mostly of working-age (the total
/// is split across brackets ∝ `bracket_size × weight`, working = 1.0, dependents =
/// `migration.dependent_weight`), who seek the highest-morale eligible same-faction band within
/// reach; found → they **relocate** (source shrinks, destination grows), none reachable → they
/// **stay** (grievance accrues faster via the trapped bonus). Morale NEVER causes faction population
/// loss.
///
/// Destinations are chosen from a single **pre-migration snapshot** of this turn's post-demographics
/// morale/brackets, and every move is computed before any is applied — so relocation is
/// order-independent (a band that receives immigrants this turn isn't re-evaluated as a fuller
/// source, and a source's outflow is unaffected by another source feeding it).
pub fn advance_population_migration(
    sim_config: Res<SimulationConfig>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    mut cohorts: Query<(Entity, &mut PopulationCohort)>,
) {
    let wellbeing = wellbeing_config.get();
    let disc_cfg = &wellbeing.discontent;
    let mig_cfg = &wellbeing.migration;
    let width = tile_registry.width;
    let wrap = sim_config.map_topology.wrap_horizontal;

    // Movement-tech reach factor. No concrete movement/transport tech signal exists in the sim yet
    // (capability flags cover construction/industry/power/naval/air/espionage/megaprojects, none of
    // which is a mobility tier), so Phase 1 keeps this at 1.0.
    // TODO(phase2): scale by the civilization's movement/transport tech tier (design doc defers
    // concrete tiers) so advanced factions send emigrants farther.
    let movement_tech_factor = 1.0_f32;
    let reach = mig_cfg.base_reach * movement_tech_factor;
    let reach_sq = (reach * reach) as i32;
    let attractive_morale = scalar_from_f32(mig_cfg.attractive_morale);
    let min_gap = scalar_from_f32(mig_cfg.min_morale_gap);
    let dependent_weight = scalar_from_f32(mig_cfg.dependent_weight);
    let morale_threshold = scalar_from_f32(mig_cfg.morale_threshold);

    // Pre-migration snapshot: everything the destination search + would-move sizing reads. The total
    // leaving is `total × move_fraction`, split across brackets ∝ `bracket_size × weight` so the
    // headline fraction is exact while working-age dominates the composition.
    struct Band {
        entity: Entity,
        faction: FactionId,
        pos: Option<UVec2>,
        morale: Scalar,
        wants_to_move: bool,
        move_working: Scalar,
        move_children: Scalar,
        move_elders: Scalar,
    }
    let mut bands: Vec<Band> = cohorts
        .iter()
        .map(|(entity, cohort)| {
            let move_fraction = migration_move_fraction(cohort.morale, mig_cfg);
            // Weighted bracket masses; the total is apportioned in proportion to these.
            let w_working = cohort.working;
            let w_children = cohort.children * dependent_weight;
            let w_elders = cohort.elders * dependent_weight;
            let denom = w_working + w_children + w_elders;
            // Clamp the headline leaving amount to the weighted denominator so no bracket can be
            // over-drafted (`move_x ≤ w_x ≤ bracket_x`), preserving faction population conservation.
            // A no-op under shipped tuning (`total × max_rate ≤ denom` always), but a safety net for
            // extreme-but-valid config (e.g. a very low `dependent_weight` on a dependent-heavy band).
            let total_leaving = (cohort.total() * move_fraction).min(denom);
            let (move_working, move_children, move_elders) = if denom > scalar_zero() {
                (
                    total_leaving * w_working / denom,
                    total_leaving * w_children / denom,
                    total_leaving * w_elders / denom,
                )
            } else {
                (scalar_zero(), scalar_zero(), scalar_zero())
            };
            Band {
                entity,
                faction: cohort.faction,
                pos: tiles.get(cohort.home).ok().map(|tile| tile.position),
                morale: cohort.morale,
                wants_to_move: total_leaving > scalar_zero(),
                move_working,
                move_children,
                move_elders,
            }
        })
        .collect();
    // Bevy query iteration order is not guaranteed stable across runs/rollback, but turn
    // resolution must be deterministic. Sort by entity id so the destination tie-break
    // (first-encountered wins on a morale tie) is reproducible.
    bands.sort_by_key(|b| b.entity.to_bits());

    // For each band that wants to move (morale below the migration threshold), find the
    // highest-morale eligible same-faction band within reach.
    let mut destination_of: Vec<Option<usize>> = vec![None; bands.len()];
    for i in 0..bands.len() {
        if !bands[i].wants_to_move {
            continue;
        }
        let Some(src_pos) = bands[i].pos else {
            continue;
        };
        let mut best: Option<(usize, Scalar)> = None;
        for (j, dest) in bands.iter().enumerate() {
            if j == i || dest.faction != bands[i].faction {
                continue;
            }
            let Some(dest_pos) = dest.pos else {
                continue;
            };
            // Eligible = meaningfully happier than a bare threshold AND than the source.
            if dest.morale < attractive_morale || dest.morale <= bands[i].morale + min_gap {
                continue;
            }
            if crate::grid_utils::wrapped_distance_sq(src_pos, dest_pos, width, wrap) > reach_sq {
                continue;
            }
            if best.is_none_or(|(_, m)| dest.morale > m) {
                best = Some((j, dest.morale));
            }
        }
        destination_of[i] = best.map(|(j, _)| j);
    }

    // Accumulate per-band bracket deltas + head-count tallies from all moves (computed against the
    // snapshot), then apply in one mutating pass so relocation is order-independent.
    let mut deltas: HashMap<Entity, (Scalar, Scalar, Scalar)> = HashMap::new();
    let mut emigrated: HashMap<Entity, u32> = HashMap::new();
    let mut immigrated: HashMap<Entity, u32> = HashMap::new();
    for (i, dest) in destination_of.iter().enumerate() {
        let Some(j) = *dest else { continue };
        let src_entity = bands[i].entity;
        let dest_entity = bands[j].entity;
        let (mw, mc, me) = (
            bands[i].move_working,
            bands[i].move_children,
            bands[i].move_elders,
        );
        let moved_head = (mw + mc + me).round().to_u32();
        if moved_head == 0 {
            continue;
        }
        let src = deltas.entry(src_entity).or_default();
        src.0 -= mw;
        src.1 -= mc;
        src.2 -= me;
        let dst = deltas.entry(dest_entity).or_default();
        dst.0 += mw;
        dst.1 += mc;
        dst.2 += me;
        *emigrated.entry(src_entity).or_default() += moved_head;
        *immigrated.entry(dest_entity).or_default() += moved_head;
    }

    // Apply relocation + refresh the derived per-turn emigrant/immigrant readouts + accrue/decay
    // the grievance accumulator. Base accrual is `grievance_gain × discontent_fraction` (the 0.6
    // discontent onset, unchanged); the trapped bonus applies specifically when the band is below
    // the migration threshold (people *want* to leave) AND has no reachable destination.
    let trapped_multiplier = scalar_from_f32(disc_cfg.trapped_multiplier);
    let grievance_gain = scalar_from_f32(disc_cfg.grievance_gain);
    let grievance_decay = scalar_from_f32(disc_cfg.grievance_decay);
    let index_of: HashMap<Entity, usize> = bands
        .iter()
        .enumerate()
        .map(|(i, b)| (b.entity, i))
        .collect();
    for (entity, mut cohort) in cohorts.iter_mut() {
        cohort.last_emigrated = emigrated.get(&entity).copied().unwrap_or(0);
        cohort.last_immigrated = immigrated.get(&entity).copied().unwrap_or(0);
        if let Some((dw, dc, de)) = deltas.get(&entity) {
            cohort.working = (cohort.working + *dw).max(scalar_zero());
            cohort.children = (cohort.children + *dc).max(scalar_zero());
            cohort.elders = (cohort.elders + *de).max(scalar_zero());
            cohort.sync_size();
        }
        if cohort.discontent_fraction <= scalar_zero() {
            cohort.grievance = (cohort.grievance - grievance_decay).max(scalar_zero());
        } else {
            // Trapped = wants to migrate (morale < threshold) but nowhere reachable to go.
            let trapped = cohort.morale < morale_threshold
                && index_of
                    .get(&entity)
                    .map(|&i| destination_of[i].is_none())
                    .unwrap_or(true);
            let mult = if trapped {
                trapped_multiplier
            } else {
                scalar_one()
            };
            let gain = grievance_gain * cohort.discontent_fraction * mult;
            cohort.grievance += gain;
        }
    }
}

/// Adjust power nodes in response to tile state and demand.
pub fn simulate_power(mut params: PowerSimParams) {
    #[derive(Clone, Copy)]
    struct NodeCalc {
        entity: Entity,
        id: PowerNodeId,
        generation: Scalar,
        demand: Scalar,
        storage_capacity: Scalar,
        storage_level: Scalar,
        net: Scalar,
        incident_count: u32,
    }

    #[derive(Clone, Copy)]
    struct NodeResult {
        entity: Entity,
        id: PowerNodeId,
        surplus: Scalar,
        deficit: Scalar,
        storage_level: Scalar,
        storage_capacity: Scalar,
        stability: Scalar,
        incident_count: u32,
        generation: Scalar,
        demand: Scalar,
    }

    let config = &*params.config;
    let impacts = &*params.impacts;
    let effects = &*params.effects;
    let topology = params.topology.as_ref();
    let grid_state = params.grid_state.as_mut();

    let corruption_cfg = params.severity_config.config().corruption();
    let power_cfg = params.pipeline_config.config().power();
    let corruption_factor = corruption_multiplier(
        &params.ledgers,
        CorruptionSubsystem::Military,
        config.corruption_military_penalty,
        corruption_cfg,
    );

    let mut node_calcs: Vec<NodeCalc> = Vec::with_capacity(params.nodes.iter().len());
    let mut node_index: HashMap<PowerNodeId, usize> =
        HashMap::with_capacity(params.nodes.iter().len());

    for (entity, tile, mut node) in params.nodes.iter_mut() {
        let efficiency_adjust =
            (config.ambient_temperature - tile.temperature) * config.power_adjust_rate;
        node.efficiency = (node.efficiency
            + efficiency_adjust * power_cfg.efficiency_adjust_scale())
        .clamp(power_cfg.efficiency_floor(), config.max_power_efficiency);

        let influence_bonus = (impacts.power_bonus + effects.power_bonus).clamp(
            scalar_from_f32(config.min_power_influence),
            scalar_from_f32(config.max_power_influence),
        );

        let effective_generation = (node.base_generation * node.efficiency + influence_bonus)
            .clamp(scalar_zero(), config.max_power_generation);
        let target_demand = (node.base_demand
            - influence_bonus * power_cfg.influence_demand_reduction())
        .clamp(scalar_zero(), config.max_power_generation);
        let net = (effective_generation - target_demand) * corruption_factor;

        node.generation = (node.base_generation
            + net * scalar_from_f32(config.power_generation_adjust_rate))
        .clamp(scalar_zero(), config.max_power_generation);
        node.demand = (node.base_demand - net * scalar_from_f32(config.power_demand_adjust_rate))
            .clamp(scalar_zero(), config.max_power_generation);

        let net_supply = node.generation - node.demand;

        let next_index = node_calcs.len();
        node_calcs.push(NodeCalc {
            entity,
            id: node.id,
            generation: node.generation,
            demand: node.demand,
            storage_capacity: node.storage_capacity,
            storage_level: node.storage_level,
            net: net_supply,
            incident_count: node.incident_count,
        });
        node_index.insert(node.id, next_index);
    }

    let node_count = node_calcs.len();
    if node_count == 0 {
        grid_state.reset();
        return;
    }

    let mut nets: Vec<Scalar> = node_calcs.iter().map(|node| node.net).collect();
    let mut storage_levels: Vec<Scalar> = node_calcs
        .iter()
        .map(|node| {
            node.storage_level
                .clamp(scalar_zero(), node.storage_capacity)
        })
        .collect();

    if topology.node_count() == node_count {
        for idx in 0..node_count {
            if nets[idx] <= scalar_zero() {
                continue;
            }
            for neighbour in topology.neighbours(node_calcs[idx].id) {
                let Some(&n_idx) = node_index.get(neighbour) else {
                    continue;
                };
                if nets[n_idx] >= scalar_zero() {
                    continue;
                }
                let needed = (-nets[n_idx]).clamp(scalar_zero(), topology.default_capacity);
                if needed <= scalar_zero() {
                    continue;
                }
                let available = nets[idx].min(topology.default_capacity);
                if available <= scalar_zero() {
                    continue;
                }
                let transfer = if available < needed {
                    available
                } else {
                    needed
                };
                if transfer > scalar_zero() {
                    nets[idx] -= transfer;
                    nets[n_idx] += transfer;
                }
            }
        }
    }

    let storage_efficiency = config.power_storage_efficiency.clamp(
        power_cfg.storage_efficiency_min(),
        power_cfg.storage_efficiency_max(),
    );
    let storage_bleed = config
        .power_storage_bleed
        .clamp(scalar_zero(), power_cfg.storage_bleed_max());

    for idx in 0..node_count {
        if nets[idx] > scalar_zero() {
            let capacity_left = (node_calcs[idx].storage_capacity - storage_levels[idx])
                .clamp(scalar_zero(), node_calcs[idx].storage_capacity);
            if capacity_left > scalar_zero() {
                let mut charge = nets[idx].min(capacity_left);
                charge *= storage_efficiency;
                storage_levels[idx] = (storage_levels[idx] + charge)
                    .clamp(scalar_zero(), node_calcs[idx].storage_capacity);
                nets[idx] -= charge;
            }
        } else if nets[idx] < scalar_zero() && storage_levels[idx] > scalar_zero() {
            let needed = (-nets[idx]).clamp(scalar_zero(), node_calcs[idx].storage_capacity);
            let discharge = storage_levels[idx].min(needed);
            let delivered = discharge * storage_efficiency;
            storage_levels[idx] = (storage_levels[idx] - discharge)
                .clamp(scalar_zero(), node_calcs[idx].storage_capacity);
            nets[idx] += delivered;
        }

        if storage_levels[idx] > scalar_zero() && storage_bleed > scalar_zero() {
            let bleed = storage_levels[idx] * storage_bleed;
            storage_levels[idx] = (storage_levels[idx] - bleed)
                .clamp(scalar_zero(), node_calcs[idx].storage_capacity);
        }
    }

    let warn_threshold = config
        .power_instability_warn
        .clamp(scalar_zero(), Scalar::one());
    let critical_threshold = config
        .power_instability_critical
        .clamp(scalar_zero(), Scalar::one());

    let mut results: Vec<NodeResult> = Vec::with_capacity(node_count);
    let mut incidents: Vec<PowerIncident> = Vec::new();
    let mut stress_sum = 0.0f32;
    let mut total_supply = scalar_zero();
    let mut total_demand = scalar_zero();
    let mut total_storage = scalar_zero();
    let mut total_capacity = scalar_zero();
    let mut alert_count: u32 = 0;

    for idx in 0..node_count {
        let demand = node_calcs[idx]
            .demand
            .clamp(scalar_zero(), config.max_power_generation);
        let generation = node_calcs[idx]
            .generation
            .clamp(scalar_zero(), config.max_power_generation);

        let surplus = if nets[idx] > scalar_zero() {
            nets[idx]
        } else {
            scalar_zero()
        };

        let deficit = if nets[idx] < scalar_zero() {
            -nets[idx]
        } else {
            scalar_zero()
        };

        let fulfilled = if deficit >= demand {
            scalar_zero()
        } else {
            demand - deficit
        };

        let mut stability = if demand > scalar_zero() {
            (fulfilled / demand).clamp(scalar_zero(), Scalar::one())
        } else {
            Scalar::one()
        };

        if storage_levels[idx] > scalar_zero() && demand > scalar_zero() {
            let reserve_ratio = (storage_levels[idx] / demand).clamp(scalar_zero(), Scalar::one());
            stability = (stability
                + reserve_ratio * scalar_from_f32(config.power_storage_stability_bonus))
            .clamp(scalar_zero(), Scalar::one());
        }

        let mut incident_count = node_calcs[idx].incident_count;
        if stability < critical_threshold {
            incidents.push(PowerIncident {
                node_id: node_calcs[idx].id,
                severity: PowerIncidentSeverity::Critical,
                deficit,
            });
            incident_count = incident_count.saturating_add(1);
            alert_count = alert_count.saturating_add(1);
        } else if stability < warn_threshold {
            incidents.push(PowerIncident {
                node_id: node_calcs[idx].id,
                severity: PowerIncidentSeverity::Warning,
                deficit,
            });
            alert_count = alert_count.saturating_add(1);
        }

        stress_sum += if demand > scalar_zero() {
            (deficit / demand).to_f32().clamp(0.0, 1.0)
        } else {
            0.0
        };

        total_supply += generation;
        total_demand += demand;
        total_storage += storage_levels[idx];
        total_capacity += node_calcs[idx].storage_capacity;

        results.push(NodeResult {
            entity: node_calcs[idx].entity,
            id: node_calcs[idx].id,
            surplus,
            deficit,
            storage_level: storage_levels[idx],
            storage_capacity: node_calcs[idx].storage_capacity,
            stability,
            incident_count,
            generation,
            demand,
        });
    }

    grid_state.reset();
    grid_state.total_supply = total_supply;
    grid_state.total_demand = total_demand;
    grid_state.total_storage = total_storage;
    grid_state.total_capacity = total_capacity;
    grid_state.instability_alerts = alert_count;
    grid_state.incidents = incidents;
    grid_state.grid_stress_avg = (stress_sum / node_count as f32).clamp(0.0, 1.0);
    let demand_f32 = total_demand.to_f32().max(1.0);
    let surplus_margin = ((total_supply + total_storage).to_f32() / demand_f32) - 1.0;
    grid_state.surplus_margin = surplus_margin;

    for node in &results {
        grid_state.nodes.insert(
            node.id,
            PowerGridNodeTelemetry {
                entity: node.entity,
                node_id: node.id,
                supply: node.generation,
                demand: node.demand,
                storage_level: node.storage_level,
                storage_capacity: node.storage_capacity,
                stability: node.stability,
                surplus: node.surplus,
                deficit: node.deficit,
                incident_count: node.incident_count,
            },
        );
    }

    let mut result_lookup: HashMap<Entity, NodeResult> = results
        .into_iter()
        .map(|node| (node.entity, node))
        .collect();

    for (entity, _tile, mut node) in params.nodes.iter_mut() {
        if let Some(result) = result_lookup.remove(&entity) {
            node.storage_level = result.storage_level;
            node.storage_capacity = result.storage_capacity;
            node.stability = result.stability;
            node.surplus = result.surplus;
            node.deficit = result.deficit;
            node.incident_count = result.incident_count;
        }
    }
}

/// React to culture tension events by nudging sentiment and diplomacy telemetry.
pub fn process_culture_events(
    mut sentiment: ResMut<SentimentAxisBias>,
    mut diplomacy: ResMut<DiplomacyLeverage>,
    mut tension_events: EventReader<CultureTensionEvent>,
    mut schism_events: EventReader<CultureSchismEvent>,
    severity_config: Res<CultureCorruptionConfigHandle>,
) {
    let config = severity_config.config();
    let culture_cfg = config.culture();
    let trust_axis = culture_cfg.trust_axis();
    let drift_tuning = culture_cfg.drift_warning();
    let assimilation_tuning = culture_cfg.assimilation_push();
    let schism_tuning = culture_cfg.schism_risk();

    for event in tension_events.read() {
        match event.kind {
            CultureTensionKind::DriftWarning => {
                let delta = drift_tuning.delta_for_magnitude(event.magnitude);
                sentiment.apply_incident_delta(trust_axis, -delta);
                diplomacy.push_culture_signal(CultureTensionRecord::from(event));
            }
            CultureTensionKind::AssimilationPush => {
                let delta = assimilation_tuning.delta_for_magnitude(event.magnitude);
                sentiment.apply_incident_delta(trust_axis, delta);
                diplomacy.push_culture_signal(CultureTensionRecord::from(event));
            }
            CultureTensionKind::SchismRisk => {
                // Handled in the dedicated schism pass below.
            }
        }
    }

    for event in schism_events.read() {
        let delta = schism_tuning.delta_for_magnitude(event.magnitude);
        sentiment.apply_incident_delta(trust_axis, -delta);
        diplomacy.push_culture_signal(CultureTensionRecord::from(event));
    }
}

/// Increment global tick counter after simulation step.
pub fn advance_tick(mut tick: ResMut<SimulationTick>) {
    tick.0 = tick.0.wrapping_add(1);
}

/// Resolve corruption timers, apply sentiment penalties, and emit telemetry.
pub fn process_corruption(
    mut ledgers: ResMut<CorruptionLedgers>,
    mut sentiment: ResMut<SentimentAxisBias>,
    mut telemetry: ResMut<CorruptionTelemetry>,
    mut diplomacy: ResMut<DiplomacyLeverage>,
    severity_config: Res<CultureCorruptionConfigHandle>,
    tick: Res<SimulationTick>,
) {
    telemetry.reset_turn();

    let ledger = ledgers.ledger_mut();
    let mut resolved: Vec<u64> = Vec::new();
    let corruption_cfg = severity_config.config().corruption();
    let trust_idx = corruption_cfg.trust_axis();
    let (delta_min, delta_max) = corruption_cfg.sentiment_delta_bounds();

    for entry in ledger.entries.iter_mut() {
        if entry.exposure_timer > 0 {
            entry.exposure_timer = entry.exposure_timer.saturating_sub(1);
        }

        if entry.exposure_timer == 0 {
            let mut record = CorruptionExposureRecord {
                incident_id: entry.incident_id,
                subsystem: entry.subsystem,
                intensity: entry.intensity,
                trust_delta: 0,
            };

            let delta = Scalar::from_raw(entry.intensity).clamp(delta_min, delta_max);
            record.trust_delta = (-delta).raw();
            telemetry.record_exposure(record.clone());
            diplomacy.push(record.clone());

            sentiment.apply_incident_delta(trust_idx, -delta);

            ledger.reputation_modifier = ledger.reputation_modifier.saturating_sub(entry.intensity);
            entry.last_update_tick = tick.0;
            resolved.push(entry.incident_id);
        }
    }

    for incident_id in resolved {
        ledger.remove_incident(incident_id);
    }

    telemetry.active_incidents = ledger.entry_count();
}

#[cfg(test)]
mod tile_morale_pressure_tests {
    use super::*;
    use crate::scalar::scalar_from_f32;
    use sim_runtime::TerrainType;

    /// Config matching the shipped defaults (`turn_pipeline_config.json` population block +
    /// `simulation_config.json` temperature levers) so the assertions track real tuning.
    fn shipped_cfg(ambient: f32) -> MoralePressureConfig {
        MoralePressureConfig {
            ambient_temperature: scalar_from_f32(ambient),
            temperature_morale_penalty: scalar_from_f32(0.004),
            temperature_morale_tolerance: scalar_from_f32(9.0),
            attrition_penalty_scale: scalar_from_f32(0.2),
            hardness_penalty_scale: scalar_from_f32(0.05),
        }
    }

    #[test]
    fn karst_cavern_mouth_is_harsh() {
        let terrain = terrain_definition(TerrainType::KarstCavernMouth);
        let ambient = 0.5;
        // Temperature matches ambient → cold term is zero, so the total is the terrain drain.
        let pressure =
            tile_morale_pressure(&terrain, scalar_from_f32(ambient), &shipped_cfg(ambient));
        assert_eq!(pressure.cold, scalar_zero());
        // attrition 0.30 * 0.2 + (1.45 - 1.0) * 0.05 = 0.0825.
        let expected = scalar_from_f32(0.0825);
        assert!(
            (pressure.total() - expected).abs() < scalar_from_f32(0.0005),
            "cavern habitability {:?} should be ~0.0825",
            pressure.total().to_f32()
        );
    }

    #[test]
    fn temperature_tolerance_dead_band_yields_no_cold_drain() {
        let terrain = terrain_definition(TerrainType::AlluvialPlain);
        let ambient = 18.0;
        // Deviation within the 9° tolerance (|Δ| = 8°) → zero climate morale drain.
        let temperate = scalar_from_f32(ambient + 8.0);
        let pressure = tile_morale_pressure(&terrain, temperate, &shipped_cfg(ambient));
        assert_eq!(pressure.cold, scalar_zero());
    }

    #[test]
    fn temperature_beyond_tolerance_drains_linearly() {
        let terrain = terrain_definition(TerrainType::AlluvialPlain);
        let ambient = 18.0;
        // Pole-like tile at −5°: |Δ| = 23°, excess beyond tolerance = 23 − 9 = 14°.
        let polar = scalar_from_f32(-5.0);
        let pressure = tile_morale_pressure(&terrain, polar, &shipped_cfg(ambient));
        // 14 * 0.004 = 0.056.
        let expected = scalar_from_f32(0.056);
        assert!(
            (pressure.cold - expected).abs() < scalar_from_f32(0.0005),
            "cold drain {:?} should be ~0.056",
            pressure.cold.to_f32()
        );
    }
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
                    mountain,
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
                        mountain: None,
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
        config.hydrology = crate::HydrologyOverrides {
            river_density: Some(1.4),
            river_min_count: Some(8),
            river_max_count: Some(24),
            accumulation_threshold_factor: Some(0.2),
            source_percentile: Some(0.55),
            source_sea_buffer: Some(0.04),
            min_length: Some(8),
            fallback_min_length: Some(4),
            spacing: Some(8.0),
            uphill_gain_pct: Some(0.07),
        };

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
        let height = registry.height as usize;

        // Every tile a river polyline passes through.
        let river_tiles: std::collections::HashSet<usize> = world
            .resource::<crate::HydrologyState>()
            .rivers
            .iter()
            .flat_map(|river| river.path.iter())
            .map(|pos| pos.y as usize * width + pos.x as usize)
            .collect();

        let is_water = |terrain: TerrainType| {
            matches!(
                terrain,
                TerrainType::DeepOcean
                    | TerrainType::ContinentalShelf
                    | TerrainType::CoralShelf
                    | TerrainType::HydrothermalVentField
                    | TerrainType::InlandSea
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
            if !river_tiles.contains(&idx) {
                orphan_deltas += 1;
            }
            let x = (idx % width) as i32;
            let y = (idx / width) as i32;
            let borders_water = [
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, 1),
                (-1, 1),
                (1, -1),
            ]
            .iter()
            .any(|(dx, dy)| {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
                    return false;
                }
                terrain_by_idx[ny as usize * width + nx as usize]
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

#[cfg(test)]
mod power_tests {
    use super::*;
    use crate::{CultureCorruptionConfig, TurnPipelineConfig};
    use bevy::{
        ecs::system::SystemState,
        prelude::{App, Entity, UVec2, World},
    };
    use sim_runtime::{TerrainTags, TerrainType};
    use std::sync::Arc;

    #[derive(Clone, Copy)]
    struct NodeSpec {
        base_generation: f32,
        base_demand: f32,
        storage_capacity: f32,
        storage_level: f32,
        incident_count: u32,
    }

    impl NodeSpec {
        fn new(base_generation: f32, base_demand: f32) -> Self {
            Self {
                base_generation,
                base_demand,
                storage_capacity: 0.0,
                storage_level: 0.0,
                incident_count: 0,
            }
        }
    }

    fn configure_simulation(app: &mut App, grid_size: UVec2) {
        app.insert_resource(SimulationConfig::builtin());
        {
            let mut config = app.world.resource_mut::<SimulationConfig>();
            config.grid_size = grid_size;
            config.power_generation_adjust_rate = 0.0;
            config.power_demand_adjust_rate = 0.0;
            config.power_storage_stability_bonus = 0.0;
            config.power_storage_efficiency = Scalar::one();
            config.power_storage_bleed = scalar_zero();
            config.power_adjust_rate = scalar_zero();
            config.max_power_generation = scalar_from_f32(100.0);
            config.power_instability_warn = scalar_from_f32(0.8);
            config.power_instability_critical = scalar_from_f32(0.5);
        }

        app.insert_resource(CultureEffectsCache::default());
        app.insert_resource(InfluencerImpacts::default());
        app.insert_resource(CorruptionLedgers::default());
        app.insert_resource(PowerGridState::default());
        app.insert_resource(CultureCorruptionConfigHandle::new(Arc::new(
            CultureCorruptionConfig::default(),
        )));
        app.insert_resource(TurnPipelineConfigHandle::new(Arc::new(
            TurnPipelineConfig::default(),
        )));
    }

    fn spawn_power_nodes(
        world: &mut World,
        width: u32,
        height: u32,
        specs: &[NodeSpec],
    ) -> Vec<Entity> {
        assert_eq!(specs.len(), (width * height) as usize);
        let ambient_temperature = world.resource::<SimulationConfig>().ambient_temperature;
        let mut entities = Vec::with_capacity(specs.len());

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let spec = specs[idx];
                let entity = world
                    .spawn((
                        Tile {
                            position: UVec2::new(x, y),
                            element: ElementKind::Ferrite,
                            mass: Scalar::one(),
                            temperature: ambient_temperature,
                            terrain: TerrainType::AlluvialPlain,
                            terrain_tags: TerrainTags::empty(),
                            mountain: None,
                        },
                        PowerNode {
                            id: PowerNodeId(idx as u32),
                            base_generation: Scalar::from_f32(spec.base_generation),
                            base_demand: Scalar::from_f32(spec.base_demand),
                            generation: Scalar::from_f32(spec.base_generation),
                            demand: Scalar::from_f32(spec.base_demand),
                            efficiency: Scalar::one(),
                            storage_capacity: Scalar::from_f32(spec.storage_capacity),
                            storage_level: Scalar::from_f32(spec.storage_level),
                            stability: Scalar::one(),
                            surplus: scalar_zero(),
                            deficit: scalar_zero(),
                            incident_count: spec.incident_count,
                        },
                    ))
                    .id();
                entities.push(entity);
            }
        }

        entities
    }

    fn run_power_system(app: &mut App) {
        let mut system_state = SystemState::<PowerSimParams>::new(&mut app.world);
        {
            let params = system_state.get_mut(&mut app.world);
            simulate_power(params);
        }
        system_state.apply(&mut app.world);
    }

    #[test]
    fn simulate_power_emits_expected_incidents_for_stability_thresholds() {
        let mut app = App::new();
        configure_simulation(&mut app, UVec2::new(3, 1));

        let specs = vec![
            NodeSpec::new(10.0, 6.0),
            NodeSpec::new(7.0, 10.0),
            NodeSpec::new(3.0, 10.0),
        ];

        let entities = spawn_power_nodes(&mut app.world, 3, 1, &specs);
        let topology = PowerTopology::from_grid(&entities, 3, 1, scalar_zero());
        app.insert_resource(topology);

        run_power_system(&mut app);

        let grid_state = app.world.resource::<PowerGridState>();
        assert_eq!(grid_state.instability_alerts, 2);
        assert_eq!(grid_state.incidents.len(), 2);

        let mut warning_count = 0;
        let mut critical_count = 0;
        for incident in &grid_state.incidents {
            match incident.severity {
                PowerIncidentSeverity::Warning => warning_count += 1,
                PowerIncidentSeverity::Critical => critical_count += 1,
            }
        }
        assert_eq!(warning_count, 1);
        assert_eq!(critical_count, 1);

        let warn_node = grid_state
            .nodes
            .get(&PowerNodeId(1))
            .expect("warn node telemetry present");
        let critical_node = grid_state
            .nodes
            .get(&PowerNodeId(2))
            .expect("critical node telemetry present");

        assert!((warn_node.stability.to_f32() - 0.7).abs() < 1e-6);
        assert!((critical_node.stability.to_f32() - 0.3).abs() < 1e-6);
        assert_eq!(critical_node.incident_count, 1);

        let _ = grid_state;

        let warn_component = app
            .world
            .entity(entities[1])
            .get::<PowerNode>()
            .expect("warn node component");
        let critical_component = app
            .world
            .entity(entities[2])
            .get::<PowerNode>()
            .expect("critical node component");

        assert_eq!(warn_component.incident_count, 0);
        assert_eq!(critical_component.incident_count, 1);
    }

    #[test]
    fn simulate_power_redistributes_surplus_along_topology() {
        let mut app = App::new();
        configure_simulation(&mut app, UVec2::new(2, 2));

        let specs = vec![
            NodeSpec::new(12.0, 4.0),
            NodeSpec::new(5.0, 10.0),
            NodeSpec::new(3.0, 6.0),
            NodeSpec::new(4.0, 4.0),
        ];

        let entities = spawn_power_nodes(&mut app.world, 2, 2, &specs);
        let topology = PowerTopology::from_grid(&entities, 2, 2, scalar_from_f32(4.0));
        app.insert_resource(topology);

        run_power_system(&mut app);

        let grid_state = app.world.resource::<PowerGridState>();
        let node_a = grid_state
            .nodes
            .get(&PowerNodeId(0))
            .expect("node A telemetry");
        let node_b = grid_state
            .nodes
            .get(&PowerNodeId(1))
            .expect("node B telemetry");
        let node_c = grid_state
            .nodes
            .get(&PowerNodeId(2))
            .expect("node C telemetry");

        assert!((node_a.surplus.to_f32() - 1.0).abs() < 1e-6);
        assert!((node_b.deficit.to_f32() - 1.0).abs() < 1e-6);
        assert!(node_c.deficit.to_f32().abs() < 1e-6);
        assert!(node_c.surplus.to_f32().abs() < 1e-6);
        assert_eq!(grid_state.instability_alerts, 0);
    }
}

#[cfg(test)]
mod demographics_tests {
    use super::{advance_demographics, death_fraction, DemographicState};
    use crate::demographics_config::DemographicsConfig;
    use crate::scalar::{scalar_from_f32, scalar_from_u32, scalar_one, scalar_zero};

    const MILD_TEMP: f32 = 0.0;
    const NO_CAP: u32 = 1_000_000_000;

    fn state(children: f32, working: f32, elders: f32, food: f32) -> DemographicState {
        DemographicState {
            children: scalar_from_f32(children),
            working: scalar_from_f32(working),
            elders: scalar_from_f32(elders),
            food_store: scalar_from_f32(food),
        }
    }

    fn total(s: &DemographicState) -> f32 {
        (s.children + s.working + s.elders).to_f32()
    }

    fn run(s: DemographicState, temp: f32) -> DemographicState {
        advance_demographics(
            s,
            scalar_from_f32(temp),
            scalar_from_u32(NO_CAP),
            &DemographicsConfig::default(),
        )
    }

    /// A well-fed, temperate cohort grows and eats from its larder.
    #[test]
    fn fed_cohort_grows_and_consumes_food() {
        let start = state(30.0, 55.0, 15.0, 1_000.0);
        let out = run(start, MILD_TEMP);
        assert!(
            total(&out) > 100.0,
            "a fed cohort should grow: {}",
            total(&out)
        );
        assert!(
            out.food_store.to_f32() < 1_000.0,
            "food should be consumed from the larder"
        );
        // Births land in the children bracket.
        assert!(out.children.to_f32() > 30.0, "births should raise children");
    }

    /// With an empty larder the cohort starves — deaths across brackets, no births, larder stays 0.
    #[test]
    fn empty_larder_starves_the_cohort() {
        let start = state(30.0, 55.0, 15.0, 0.0);
        let out = run(start, MILD_TEMP);
        assert!(
            total(&out) < 80.0,
            "starvation should sharply cut population: {}",
            total(&out)
        );
        assert!(out.food_store.to_f32().abs() < 1e-4, "larder stays empty");
        // Dependents (1.5× vulnerability) fall harder than working-age (1.0×).
        let child_survival = out.children.to_f32() / 30.0;
        let working_survival = out.working.to_f32() / 55.0;
        assert!(
            child_survival < working_survival,
            "children should die faster than workers: {child_survival} vs {working_survival}"
        );
    }

    /// Extreme cold kills across brackets even when the larder is full.
    #[test]
    fn cold_kills_even_when_fed() {
        let warm = run(state(30.0, 55.0, 15.0, 1_000.0), MILD_TEMP);
        let cold = run(state(30.0, 55.0, 15.0, 1_000.0), 40.0);
        assert!(
            total(&cold) < total(&warm),
            "cold should reduce population vs temperate: {} vs {}",
            total(&cold),
            total(&warm)
        );
    }

    /// Births are morale-INDEPENDENT (wellbeing model): `advance_demographics` no longer takes
    /// morale, so a fed cohort still grows regardless of contentment — morale acts only through
    /// productivity + migration, never on births. This is the same fed grow case as
    /// `fed_cohort_grows_and_consumes_food`; it exists to lock the decoupling in place.
    #[test]
    fn births_are_morale_independent() {
        let start = state(30.0, 55.0, 15.0, 1_000.0);
        let out = run(start, MILD_TEMP);
        assert!(
            out.children.to_f32() > 30.0,
            "a fed cohort must still bear children with morale removed from the formula: {}",
            out.children.to_f32()
        );
    }

    /// The aggregate cap scales an over-large population back down.
    #[test]
    fn population_cap_clamps_total() {
        let start = state(100.0, 100.0, 100.0, 10_000.0);
        let out = advance_demographics(
            start,
            scalar_from_f32(MILD_TEMP),
            scalar_from_u32(50),
            &DemographicsConfig::default(),
        );
        assert!(
            (total(&out) - 50.0).abs() < 1.0,
            "total should clamp to the cap of 50: {}",
            total(&out)
        );
    }

    /// Starvation deaths scale with the deficit × vulnerability but never exceed the deficit;
    /// cold adds on top, and the whole thing caps at 1.0.
    #[test]
    fn death_fraction_is_bounded_by_deficit_and_one() {
        // Full deficit, rate 0.2, vuln 1.5 → 0.30 (< deficit 1.0), no cold.
        let f = death_fraction(scalar_one(), scalar_from_f32(0.2), 1.5, scalar_zero());
        assert!((f.to_f32() - 0.30).abs() < 1e-4);
        // A 10% deficit with a steep rate×vuln (0.8×1.5=1.2) is still capped at the 10% deficit.
        let bounded = death_fraction(
            scalar_from_f32(0.1),
            scalar_from_f32(0.8),
            1.5,
            scalar_zero(),
        );
        assert!(
            (bounded.to_f32() - 0.1).abs() < 1e-4,
            "a 10% deficit must impact at most 10%: {}",
            bounded.to_f32()
        );
        // Full deficit + max cold overflow → capped at 1.0.
        let capped = death_fraction(
            scalar_one(),
            scalar_from_f32(0.8),
            1.5,
            scalar_from_f32(0.5),
        );
        assert!((capped.to_f32() - 1.0).abs() < 1e-4);
    }

    /// A childless cohort matures no one, but working-age still ages into elders.
    #[test]
    fn aging_moves_workers_into_elders() {
        let start = state(0.0, 100.0, 0.0, 10_000.0);
        let out = run(start, MILD_TEMP);
        assert!(out.elders.to_f32() > 0.0, "workers should age into elders");
    }
}

#[cfg(test)]
mod wellbeing_tests {
    use super::{
        advance_population_migration, discontent_fraction, discontent_output_modifier,
        migration_move_fraction, output_multiplier,
    };
    use crate::components::{MoraleCause, MoraleContributions, PopulationCohort, Tile};
    use crate::orders::FactionId;
    use crate::resources::{SimulationConfig, TileRegistry};
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};
    use crate::wellbeing_config::{WellbeingConfig, WellbeingConfigHandle};
    use crate::LocalStore;
    use bevy::prelude::{Entity, World};
    use bevy_ecs::system::RunSystemOnce;

    fn cfg() -> WellbeingConfig {
        WellbeingConfig::default()
    }

    /// Layer 2 discontent curve: 0 at/above `content_morale` (0.6), 1 at/below `floor_morale`
    /// (0.1), linear between. Locks the worked numbers reported for morale 0.9/0.6/0.38/0.25/0.1.
    #[test]
    fn discontent_fraction_curve() {
        let d = &cfg().discontent;
        let f = |m: f32| discontent_fraction(scalar_from_f32(m), d).to_f32();
        assert!((f(0.9) - 0.0).abs() < 1e-4, "content above 0.6");
        assert!((f(0.6) - 0.0).abs() < 1e-4, "content at the threshold");
        assert!(
            (f(0.38) - 0.44).abs() < 1e-3,
            "partial discontent: {}",
            f(0.38)
        );
        assert!(
            (f(0.25) - 0.70).abs() < 1e-3,
            "partial discontent: {}",
            f(0.25)
        );
        assert!(
            (f(0.1) - 1.0).abs() < 1e-4,
            "fully discontented at the floor"
        );
    }

    /// Layer 3a output stack: 100% at zero discontent, floored at `floor_mult` (0.5) once
    /// discontent × weight would push output below the floor.
    #[test]
    fn output_modifier_stack_bounds() {
        let p = &cfg().productivity;
        assert!((discontent_output_modifier(scalar_zero(), p).to_f32() - 1.0).abs() < 1e-4);
        // 44% discontent, weight 1.0 → 56% output.
        assert!(
            (discontent_output_modifier(scalar_from_f32(0.44), p).to_f32() - 0.56).abs() < 1e-3
        );
        // 70% discontent would give 30% but is floored to 50%.
        assert!((discontent_output_modifier(scalar_from_f32(0.70), p).to_f32() - 0.5).abs() < 1e-4);
        assert!((discontent_output_modifier(scalar_one(), p).to_f32() - 0.5).abs() < 1e-4);
    }

    /// Layer 3b migration onset (decoupled from discontent): `max_rate × clamp((0.25 − morale)/0.25,
    /// 0, 1)`. 0 at/above the 0.25 threshold, 7.5% at 0.125, 15% at rock-bottom. A morale-0.38 band
    /// (discontented for productivity, but above the migration onset) sheds nobody.
    #[test]
    fn migration_move_fraction_curve() {
        let m = &cfg().migration;
        let f = |v: f32| migration_move_fraction(scalar_from_f32(v), m).to_f32();
        assert!(
            (f(0.38) - 0.0).abs() < 1e-6,
            "above onset → stays: {}",
            f(0.38)
        );
        assert!((f(0.25) - 0.0).abs() < 1e-6, "exactly at onset → 0");
        assert!(
            (f(0.24) - 0.006).abs() < 1e-4,
            "just below onset: {}",
            f(0.24)
        );
        assert!((f(0.125) - 0.075).abs() < 1e-4, "half-ramp: {}", f(0.125));
        assert!((f(0.05) - 0.12).abs() < 1e-4, "steep: {}", f(0.05));
        assert!(
            (f(0.0) - 0.15).abs() < 1e-6,
            "cap at rock-bottom: {}",
            f(0.0)
        );
    }

    fn band(home: Entity, faction: u32, morale: f32, working: f32) -> PopulationCohort {
        let m = scalar_from_f32(morale);
        let mut cohort = PopulationCohort {
            home,
            current_tile: home,
            size: 0,
            children: scalar_zero(),
            working: scalar_from_f32(working),
            elders: scalar_zero(),
            stores: LocalStore::new(),
            morale: m,
            last_morale_delta: scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: MoraleContributions::default(),
            discontent_fraction: discontent_fraction(m, &cfg().discontent),
            grievance: scalar_zero(),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: 10,
            generation: 0,
            faction: FactionId(faction),
            knowledge: Vec::new(),
            migration: None,
        };
        cohort.sync_size();
        cohort
    }

    fn world_with_tiles(positions: &[(u32, u32)], width: u32) -> (World, Vec<Entity>) {
        let mut world = World::default();
        let mut config = SimulationConfig::builtin();
        config.map_topology.wrap_horizontal = false;
        world.insert_resource(config);
        world.insert_resource(WellbeingConfigHandle::default());
        let tiles: Vec<Entity> = positions
            .iter()
            .map(|&(x, y)| {
                let tile = Tile {
                    position: bevy::math::UVec2::new(x, y),
                    ..Default::default()
                };
                world.spawn(tile).id()
            })
            .collect();
        world.insert_resource(TileRegistry {
            tiles: tiles.clone(),
            width,
            height: 1,
        });
        (world, tiles)
    }

    /// Migration relocates the morale-scaled would-move head-count from a below-threshold band to
    /// the best reachable eligible same-faction band, and the faction total is conserved (morale
    /// never kills). At morale 0.1 the move fraction is `0.15 × (0.25−0.1)/0.25 = 0.09` → ~81 of 900.
    #[test]
    fn migration_relocates_and_conserves() {
        let (mut world, tiles) = world_with_tiles(&[(0, 0), (2, 0)], 8);
        let src = world.spawn(band(tiles[0], 0, 0.1, 900.0)).id();
        let dst = world.spawn(band(tiles[1], 0, 0.70, 900.0)).id();
        let before: f32 = {
            let a = world.get::<PopulationCohort>(src).unwrap();
            let b = world.get::<PopulationCohort>(dst).unwrap();
            a.total().to_f32() + b.total().to_f32()
        };
        world.run_system_once(advance_population_migration);
        let a = world.get::<PopulationCohort>(src).unwrap();
        let b = world.get::<PopulationCohort>(dst).unwrap();
        assert!(a.last_emigrated > 0, "source should shed emigrants");
        assert!(
            (a.last_emigrated as f32 - 81.0).abs() <= 1.0,
            "≈9% of 900 leave: {}",
            a.last_emigrated
        );
        assert_eq!(
            b.last_immigrated, a.last_emigrated,
            "everyone who left arrives — nobody vanishes"
        );
        assert!(
            a.working.to_f32() < 900.0 && b.working.to_f32() > 900.0,
            "source shrinks, destination grows"
        );
        let after = a.total().to_f32() + b.total().to_f32();
        assert!(
            (after - before).abs() < 1.0,
            "faction population conserved: {before} -> {after}"
        );
    }

    /// A band that is discontented (for productivity) but ABOVE the migration onset stays entirely
    /// put — morale 0.38 → discontent 0.44 (output 56%) yet move fraction 0.
    #[test]
    fn above_migration_threshold_stays() {
        let (mut world, tiles) = world_with_tiles(&[(0, 0), (2, 0)], 8);
        let src = world.spawn(band(tiles[0], 0, 0.38, 900.0)).id();
        let _dst = world.spawn(band(tiles[1], 0, 0.70, 900.0)).id();
        world.run_system_once(advance_population_migration);
        let a = world.get::<PopulationCohort>(src).unwrap();
        assert_eq!(a.last_emigrated, 0, "above the 0.25 onset → nobody leaves");
        assert!(
            (a.working.to_f32() - 900.0).abs() < 1e-3,
            "population stays put"
        );
    }

    /// Below-threshold band with no eligible/reachable destination → people STAY (no move) and
    /// grievance rises via the trapped multiplier.
    #[test]
    fn no_destination_stays_and_grievance_rises() {
        // Source below the migration onset; the only other band is not attractive (< 0.5).
        let (mut world, tiles) = world_with_tiles(&[(0, 0), (2, 0)], 8);
        let a = world.spawn(band(tiles[0], 0, 0.15, 900.0)).id();
        let _b = world.spawn(band(tiles[1], 0, 0.30, 900.0)).id();
        let working_before = world.get::<PopulationCohort>(a).unwrap().working.to_f32();
        world.run_system_once(advance_population_migration);
        let cohort = world.get::<PopulationCohort>(a).unwrap();
        assert_eq!(cohort.last_emigrated, 0, "nowhere to go → nobody leaves");
        assert!(
            (cohort.working.to_f32() - working_before).abs() < 1e-3,
            "population stays put"
        );
        // Trapped accrual = grievance_gain × discontent(0.15) × trapped_multiplier.
        let disc = &cfg().discontent;
        let f = discontent_fraction(scalar_from_f32(0.15), disc);
        let expected =
            scalar_from_f32(disc.grievance_gain) * f * scalar_from_f32(disc.trapped_multiplier);
        assert!(
            (cohort.grievance - expected).to_f32().abs() < 1e-4,
            "trapped grievance accrues at the boosted rate: {} vs {}",
            cohort.grievance.to_f32(),
            expected.to_f32()
        );
    }

    /// A discontented band with a reachable happier band accrues grievance at the un-trapped rate,
    /// strictly less than the trapped band above — the two rates differ by the trapped multiplier.
    #[test]
    fn grievance_trapped_bonus() {
        let disc = &cfg().discontent;
        let f = discontent_fraction(scalar_from_f32(0.25), disc).to_f32();
        let untrapped = disc.grievance_gain * f;
        let trapped = disc.grievance_gain * f * disc.trapped_multiplier;
        assert!(trapped > untrapped, "trapped grievance accrues faster");
    }

    /// Grievance decays while the band is content (discontent_fraction == 0).
    #[test]
    fn grievance_decays_when_content() {
        let (mut world, tiles) = world_with_tiles(&[(0, 0)], 8);
        let e = {
            let mut c = band(tiles[0], 0, 0.9, 900.0);
            c.grievance = scalar_from_f32(0.5);
            world.spawn(c).id()
        };
        world.run_system_once(advance_population_migration);
        let cohort = world.get::<PopulationCohort>(e).unwrap();
        assert!(
            cohort.grievance < scalar_from_f32(0.5),
            "content bands bleed off grievance"
        );
    }

    /// The output multiplier reads a cohort's discontent through the stack (integration of §4).
    #[test]
    fn output_multiplier_reads_discontent() {
        let content = band(Entity::from_raw(0), 0, 0.9, 100.0);
        let miserable = band(Entity::from_raw(1), 0, 0.1, 100.0);
        let wb = cfg();
        assert!(
            (output_multiplier(&content, &wb) - scalar_one())
                .to_f32()
                .abs()
                < 1e-4
        );
        assert!(output_multiplier(&miserable, &wb) < scalar_one());
    }
}
