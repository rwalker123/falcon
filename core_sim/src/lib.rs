//! Core simulation crate for the Shadow-Scale headless prototype.
//!
//! Provides deterministic ECS systems that resolve a single turn of the
//! simulation when [`run_turn`] is invoked.

/// Human-readable build identifier for the server binary, **auto-generated at
/// compile time** by `build.rs` as `<commit-date>-<short-hash>` (e.g.
/// `2026-07-09-a1b2c3d`) so it always reflects the actual build and can never be
/// a stale hand-bumped constant. It is stamped onto each snapshot header
/// (`SnapshotHeader::server_build`) and shown in the client's version overlay so
/// the running server build can be confirmed at a glance. Falls back to
/// `dev-unknown` when git metadata is unavailable (offline/CI/exported source).
pub(crate) const BUILD_ID: &str = match option_env!("CORE_SIM_BUILD_ID") {
    Some(v) => v,
    None => "dev-unknown",
};

mod biome_palette;
pub mod climate;
pub mod combat;
mod combat_config;
mod components;
pub mod config_fingerprint;
mod config_load;
pub mod config_override;
pub mod connections;
mod connections_config;
pub mod crafting;
mod creatures_config;
mod crisis;
mod crisis_config;
mod culture;
mod culture_corruption_config;
mod demographics_config;
mod equipment_config;
mod espionage;
mod expedition_config;
mod fauna;
mod fauna_config;
mod flora_config;
mod food;
mod forage;
pub mod forecast_query;
mod generations;
mod graze;
mod great_discovery;
pub mod grid_utils;
pub mod hashing;
pub mod heightfield;
mod hydrology;
mod influencers;
mod intensification;
mod knowledge_ledger;
mod labor_config;
pub mod log_stream;
mod map_preset;
mod mapgen;
mod materials_config;
pub mod metrics;
pub mod network;
mod orders;
pub mod port_alloc;
mod power;
mod provinces;
mod recipes_config;
mod resources;
pub mod routes;
pub mod save;
pub mod save_store;
mod scalar;
mod sedentarization;
mod sedentarization_config;
mod settlement_stage_config;
pub mod sim_state;
mod sites;
mod sites_config;
mod snapshot;
mod snapshot_overlays_config;
mod start_profile;
mod supply;
mod supply_network_config;
mod systems;
pub mod telling;
mod terrain;
mod turn_pipeline_config;
pub mod turn_profile;
mod victory;
mod visibility;
mod visibility_config;
mod visibility_systems;
mod wellbeing_config;

use std::sync::Arc;

use crate::map_preset::load_map_presets_from_env;
use crate::start_profile::{
    load_start_profile_knowledge_tags_from_env, load_start_profiles_from_env,
};
use bevy::ecs::schedule::{LogLevel, ScheduleBuildSettings};
use bevy::prelude::*;

pub use combat::{
    attacks_landed_at, landed_strikes_seeded, resolve_fight, strike_damage, units_brought_down,
    CombatStats, CombatTuning, Contingent, ContingentId, ContingentResult, DamageLedger,
    FightOutcome, FightPayload, Force, ForceId, Posture, RangeBand, StrikeDraw, TerrainContext,
    EXPECTED_STRIKES,
};
pub use combat_config::{
    load_combat_config_from_env, CombatConfig, CombatConfigHandle, CombatConfigMetadata,
    BUILTIN_COMBAT_CONFIG,
};
pub use components::{
    available_workers, floor_is_valid, floor_overdraws, raid_is_recurring, take_overdraws,
    BandBench, BandEquipment, BandId, BandTravel, BandWorkforce, BatchGrade, BuildJob,
    BuildQueueEntry, BuildSource, DeathCause, DemographicFlowAccumulator, DrawnInputs,
    DrawnMaterial, ElementKind, EquipmentBatch, Expedition, ExpeditionMission, ExpeditionPhase,
    Improvement, KnowledgeFragment, LaborAllocation, LaborAssignment, LaborTarget, LocalStore,
    MaterialBatch, MaterialDraw, MoraleCause, PendingMigration, PopulationCohort, PowerNode,
    ResidentBand, Settlement, ShedCrew, ShedFacts, ShedStep, ShedSubject, SourcePriority,
    SourceShedFacts, SourceYield, StartingUnit, TakeSelection, Tile, TownCenter, YieldRange,
    DEFAULT_ESCAPEMENT_FLOOR, FODDER, FOOD, NO_IMPROVEMENT_UNDERWAY, NO_RAID_FLOOR, STRIP_IT_BARE,
};
pub use config_fingerprint::{
    current_config_fingerprint, drift_between, ConfigDigest, ConfigFingerprint,
};
pub use config_load::ConfigLoadError;
pub use config_override::{
    clear_config_overrides, install_config_override, spec_for as config_override_spec_for,
    ConfigKindSpec, ConfigOverrideError, InstalledOverride,
};
pub use connections::{
    advance_connections, Connection, ConnectionKey, ConnectionLedger, ContactsThisTurn, FULL_TIE,
    NO_TIE,
};
pub use connections_config::{
    load_connections_config_from_env, ConnectionStrengthConfig, ConnectionsConfig,
    ConnectionsConfigHandle, ConnectionsConfigMetadata, BUILTIN_CONNECTIONS_CONFIG,
};
pub use creatures_config::{
    load_creatures_config_from_env, CreatureDef, CreaturesConfig, CreaturesConfigHandle,
    CreaturesConfigMetadata, BUILTIN_CREATURES_CONFIG, PERSON_ID,
};
pub use crisis::{
    ActiveCrisisLedger, CrisisGaugeSnapshot, CrisisMetricKind, CrisisMetricsSnapshot,
    CrisisOverlayCache, CrisisSeverityBand, CrisisTelemetry, CrisisTelemetrySample,
    CrisisTrendSample,
};
pub use crisis_config::{
    load_crisis_archetypes_from_env, load_crisis_modifiers_from_env,
    load_crisis_telemetry_config_from_env, CrisisArchetype, CrisisArchetypeCatalog,
    CrisisArchetypeCatalogHandle, CrisisArchetypeCatalogMetadata, CrisisModifier,
    CrisisModifierCatalog, CrisisModifierCatalogHandle, CrisisModifierCatalogMetadata,
    CrisisTelemetryConfig, CrisisTelemetryConfigHandle, CrisisTelemetryConfigMetadata,
    CrisisTelemetryThreshold, BUILTIN_CRISIS_ARCHETYPES, BUILTIN_CRISIS_MODIFIERS,
    BUILTIN_CRISIS_TELEMETRY_CONFIG,
};
pub use culture::{
    culture_region_at, reconcile_band_culture_layers, reconcile_culture_layers,
    seeded_modifiers_for_band, CultureEffectsCache, CultureLayer, CultureLayerId,
    CultureLayerScope, CultureManager, CultureOwner, CultureSchismEvent, CultureTensionEvent,
    CultureTensionKind, CultureTensionRecord, CultureTraitAxis, CultureTraitVector,
    CULTURE_TRAIT_AXES, FALLBACK_CULTURE_REGION_ID,
};
pub use culture_corruption_config::{
    CorruptionSeverityConfig, CultureCorruptionConfig, CultureCorruptionConfigHandle,
    CultureSeverityConfig, CultureTensionTuning, BUILTIN_CULTURE_CORRUPTION_CONFIG,
};
pub use demographics_config::{
    load_demographics_config_from_env, DemographicsConfig, DemographicsConfigHandle,
    DemographicsConfigMetadata,
};
pub use equipment_config::{
    load_equipment_config_from_env, Crew, DefaultKitsConfig, EffectTier, EquipmentConfig,
    EquipmentConfigHandle, EquipmentConfigMetadata, EquipmentEffect, EquipmentStat, EquipmentTier,
    ItemDefinition, KitChoice, KitCoverage, KitDefinition, KitJob, KitSelectionError, LiveItem,
    Quarry, WearConfig, WearQuantum, BUILTIN_EQUIPMENT_CONFIG,
};
pub use espionage::{
    AgentAssignment, CounterIntelBudgets, EspionageAgentHandle, EspionageCatalog,
    EspionageMissionId, EspionageMissionInstanceId, EspionageMissionKind, EspionageMissionState,
    EspionageMissionTemplate, EspionageRoster, FactionSecurityPolicies, QueueMissionError,
    QueueMissionParams, SecurityPolicy,
};
pub use expedition_config::{
    load_expedition_config_from_env, ExpeditionConfig, ExpeditionConfigHandle,
    ExpeditionConfigMetadata, SettleConfig, BUILTIN_EXPEDITION_CONFIG,
};
pub use fauna::{
    advance_herd_grazing, advance_herds, advance_husbandry, advance_predation, animals_affordable,
    animals_engaged, animals_sparable, animals_that_stay, animals_that_stay_at_rate,
    build_prey_index, cancel_dropped_rings, carnivore_k_at, drop_holding_and_cancel_ring,
    escapement_ceiling, forecast_expected_take, forecast_take_range, herd_build_verb,
    herd_capacity, herd_default_hunt_kit, herd_density_gain, herd_destination_capacity,
    herd_ecology, herd_engage_rate, herd_herded_fraction, herd_herders_needed, herd_hunt_yield,
    herd_keeper_load, herd_keeper_loads, herd_keeping_basis, herd_meter_rot, herd_past_recovery,
    herd_quarry_fight, herd_rung_already_built, herd_space_capacity, herd_take_room,
    herd_upkeep_demand, herd_upkeep_shortfall, herd_upkeep_supply, herd_upkeep_workers_needed,
    herd_wariness, hunt_crew_take_curve, hunt_engage_workers, hunt_escapement_ceiling,
    hunt_haul_workers, hunt_source_yield_preview, hunt_take_bound, hunt_take_overdraws,
    hunt_take_workers, hunt_useful_crew, next_turns_quarry, per_hunter_take_biomass,
    project_arrivals_hunt, project_realized_hunt, quantise_animal_take, quarry_default_hunt_kit,
    regrow_biomass, regrowth_delta_at, repopulate_fauna, resolve_hunt_engagement,
    resolve_hunt_fight, retreat_seed, spawn_initial_herds, species_requires_denial, stay_fraction,
    sustainable_yield, unqueue_build_and_cancel_ring, would_be_herders_needed, AnimalTake,
    EcologyPhase, EngagementQuantum, EngagementStop, FightCasualties, Herd, HerdDensityMap,
    HerdRegistry, HerdTelemetry, HerdTelemetryEntry, HuntCrew, HuntCrewCurveInputs, HuntCrewTake,
    HuntDraw, HuntEngagement, HuntFight, HuntTakeBound, HuntingParty, PartyResolution, PreyDatum,
    QuarryFight, RoamState, SourceYieldForecast, TakeRange, FODDERING_DISCOVERY_ID, FULLY_HERDED,
    HERDING_DISCOVERY_ID, MSY_BIOMASS_FRACTION, NO_DEATHS_TO_REPORT, NO_USEFUL_CREW,
    ONE_KEEPER_LOAD, PENNING_DISCOVERY_ID,
};
pub use fauna_config::{
    load_fauna_config_from_env, Diet, EcologyConfig, FaunaConfig, FaunaConfigHandle,
    FaunaConfigMetadata, GrazeConfig, HuntYield, HuntYieldDef, HusbandryCeiling,
    MigratoryAbundanceConfig, ShoreRequirement, SizeClass, SpeciesDef, YieldAccounts,
    BUILTIN_FAUNA_CONFIG, NO_GRAZE_CAPACITY, NO_RETREAT,
};
pub use flora_config::{
    load_flora_config_from_env, CultivationCeiling, FloraConfig, FloraConfigHandle,
    FloraConfigMetadata, FloraDef, FloraRole, FloraShare, YieldVector, BUILTIN_FLORA_CONFIG,
};
pub use food::{
    classify_food_module, classify_food_module_from_traits, FoodModule, FoodModuleTag,
    FoodSiteKind, DEFAULT_HARVEST_TRAVEL_TILES_PER_TURN, DEFAULT_HARVEST_WORK_TURNS,
};
pub use forage::{
    advance_cultivation, advance_forage_regrowth, commit_fodder_payoff, commit_material_payoff,
    commit_payoff, commit_yield_ratio, composition_for_rung, crop_field_cost_multiplier,
    default_species_for_rung, field_cost_multiplier_at_share, forage_per_worker_biomass,
    forage_provisions, forage_source_yield_preview, forage_take_overdraws, next_turns_stand,
    patch_build_legs, patch_build_verb, patch_carrying_capacity, patch_claims_keeping,
    patch_composition, patch_destination_capacity, patch_ecology, patch_field_cost_multiplier,
    patch_keeping_basis, patch_land_capacity, patch_material_yields, patch_material_yields_taking,
    patch_meter_rot, patch_provisions_per_biomass, patch_provisions_per_biomass_taking,
    patch_rung_already_built, patch_rung_span, patch_rung_work_done, patch_species_quality,
    patch_tender_loads, patch_unwinding_key, patch_upkeep_demand, patch_upkeep_shortfall,
    patch_upkeep_workers_needed, plant_rung_span, project_arrivals_forage, project_realized_forage,
    resolve_committed_species, resolve_take_selection, rung_material_yields, rung_payoff,
    rung_site_refusal, selected_biomass_share, spawn_initial_forage, species_is_legal_here,
    species_stands_in, tended_take_fodder, tile_flora_composition, tile_forage_capacity,
    tile_is_fresh_watered, wild_payoff, ForagePatch, ForageRegistry, SpeciesRate, SpeciesRefusal,
    CANNOT_CLIMB_RATIO, CULTIVATION_DISCOVERY_ID, NO_FORAGE_SEASON, NO_TENDER_LOAD,
    ONE_TENDER_LOAD, SEED_SELECTION_DISCOVERY_ID, WHOLE_BASKET,
};
pub use generations::{GenerationBias, GenerationId, GenerationProfile, GenerationRegistry};
pub use graze::{advance_graze_regrowth, spawn_initial_graze, GrazePatch, GrazeRegistry};
pub use great_discovery::{
    ConstellationRequirement, GreatDiscoveryCandidateEvent, GreatDiscoveryDefinition,
    GreatDiscoveryEffectEvent, GreatDiscoveryEffectKind, GreatDiscoveryFlag, GreatDiscoveryId,
    GreatDiscoveryLedger, GreatDiscoveryReadiness, GreatDiscoveryRegistry,
    GreatDiscoveryResolvedEvent, GreatDiscoveryTelemetry, ObservationLedger,
};
pub use hydrology::{generate_hydrology, HydrologyState};
// The drainage-network measurement instrument (consumed by the `#[ignore]`d census test).
pub use hydrology::{debug_drainage_census, DrainageCensus};
pub use influencers::{
    tick_influencers, InfluencerBalanceConfig, InfluencerConfigHandle, InfluencerCultureResonance,
    InfluencerImpacts, InfluentialId, InfluentialRoster, SupportChannel, BUILTIN_INFLUENCER_CONFIG,
};
pub use intensification::{
    activity_work, build_fraction, build_turns_estimate, build_turns_remaining,
    build_work_per_worker_turn, distribute_upkeep_pool, gear_work_supply, interpolate, knows,
    learn_multiplier, load_intensification_ladder_from_env, pool_work_supply, rung_work_done,
    upkeep_shortfall, upkeep_shortfall_fraction, BuildGate, BuildTurns, LadderConfig,
    LadderConfigHandle, LadderConfigMetadata, RungBehavior, RungBranch, RungBuild, RungDef,
    RungKey, RungMeterDecay, RungMovement, RungPartialCredit, RungSiteRequirement, RungStanding,
    RungUpkeep, SiteRefusal, UpkeepFundMode, UpkeepScale, BUILTIN_INTENSIFICATION_LADDER,
    FABRICATED_BUILD_COST, FULLY_SUPPLIED, NOTHING_IN_FLIGHT, NO_BUILD_GEAR,
    NO_CREW_ON_THIS_ACTIVITY, NO_RUNG_CREDIT, NO_RUNG_WORK_BANKED, NO_UPKEEP_DECAY,
    NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT, RUNG_COST_UNSCALED, RUNG_UNSTARTED, SITE_ACCEPTED,
    WHOLLY_UNSUPPLIED,
};
pub use knowledge_ledger::{
    CounterIntelSweepEvent, EspionageProbeEvent, KnowledgeCountermeasure, KnowledgeLedger,
    KnowledgeLedgerConfig, KnowledgeLedgerConfigHandle, KnowledgeLedgerEntry, KnowledgeModifier,
    KnowledgeTimelineEvent, BUILTIN_KNOWLEDGE_LEDGER_CONFIG,
};
pub use labor_config::{
    load_labor_config_from_env, LaborConfig, LaborConfigHandle, LaborConfigMetadata,
    BUILTIN_LABOR_CONFIG, NO_FORAGE_CAPACITY,
};
pub use map_preset::{ErosionConfig, MapPreset, MapPresets, MapPresetsHandle, BUILTIN_MAP_PRESETS};
pub use mapgen::WorldGenSeed;
pub use materials_config::{
    credit_material_yield, load_materials_config_from_env, material_yield_totals, BandKey,
    CharacteristicBand, HandWorking, MaterialDef, MaterialPayoff, MaterialYieldDef,
    MaterialYieldError, MaterialsConfig, MaterialsConfigError, MaterialsConfigHandle,
    MaterialsConfigMetadata, BUILTIN_MATERIALS_CONFIG,
};
pub use recipes_config::{
    load_recipes_config_from_env, CraftingTuning, RecipeDef, RecipeGrade, RecipeInput,
    RecipeOutput, RecipesConfig, RecipesConfigError, RecipesConfigHandle, RecipesConfigMetadata,
    BUILTIN_RECIPES_CONFIG,
};
pub use routes::{
    advance_roads, credit_route_lessons, max_route_reach_tiles, path_friction_multiplier,
    path_lesson_rung, path_reach_tiles, remoteness_multiplier, road_at_risk_rung,
    road_build_fraction, road_keeping_basis, road_keeping_range, road_measure,
    road_neglect_grace_remaining, road_rung_span, road_upkeep_demand, road_upkeep_measure,
    road_upkeep_workers_needed, route_rungs_in_climb_order, rung_grants_sight, trace_path,
    traffic_ceiling, Road, RoadKeeper, RoadRegistry, RouteJourney, RouteTrafficLog,
    FIRST_BUILT_RUNG, FREE_FLOOR_TOP_RUNG, METER_FULL, NEAR_ENOUGH_TO_KEEP, NO_REACH_HELD_OPEN,
    PAVING_DISCOVERY_ID, ROADBUILDING_DISCOVERY_ID,
};
pub use sedentarization::{
    sedentarization_tick, SedentarizationEntry, SedentarizationScore, SedentarizationStage,
};
pub use sedentarization_config::{
    load_sedentarization_config_from_env, SedentarizationConfig, SedentarizationConfigHandle,
    SedentarizationConfigMetadata,
};
pub use settlement_stage_config::{
    load_settlement_stage_config_from_env, resolve_settlement_stage, SettlementStageConfig,
    SettlementStageConfigHandle, SettlementStageConfigMetadata, SettlementStageDef,
    SettlementStageInputs, StageCriteria, BUILTIN_SETTLEMENT_STAGE_CONFIG,
};
pub use sites::{
    discover_sites, place_wondrous_sites, DiscoveredSiteRecord, DiscoveredSites, SiteTag,
};
pub use sites_config::{
    load_sites_config_from_env, DiscoveryReward, PlacementRuleCfg, SiteDef, SitesConfig,
    SitesConfigHandle, SitesConfigMetadata, BUILTIN_SITES_CONFIG,
};
pub use snapshot_overlays_config::{
    load_snapshot_overlays_config_from_env, CorruptionOverlayConfig, CultureOverlayConfig,
    MilitaryOverlayConfig, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    SnapshotOverlaysConfigMetadata, BUILTIN_SNAPSHOT_OVERLAYS_CONFIG,
};
pub use start_profile::{
    resolve_active_profile, snapshot_profiles, ActiveStartProfile, CampaignLabel, InventoryEntry,
    StartProfile, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle,
    StartProfileKnowledgeTagsMetadata, StartProfileLookup, StartProfileOverrides,
    StartProfilesHandle, StartProfilesMetadata, StartingUnitSpec,
};
pub use supply::{balance_supply_networks, SupplyNetworkMembership};
pub use supply_network_config::{
    load_supply_network_config_from_env, SupplyNetworkConfig, SupplyNetworkConfigHandle,
    SupplyNetworkConfigMetadata,
};
pub use turn_pipeline_config::{
    load_turn_pipeline_config_from_env, PopulationPhaseConfig, PowerPhaseConfig,
    TurnPipelineConfig, TurnPipelineConfigHandle, TurnPipelineConfigMetadata,
    BUILTIN_TURN_PIPELINE_CONFIG,
};
pub use victory::{
    load_victory_config_from_env, VictoryConfigHandle, VictoryModeId, VictoryModeKind,
    VictoryModeState, VictoryState,
};
pub use visibility::{
    FactionVisibilityMap, TileVisibility, ViewerFaction, VisibilityLedger, VisibilitySource,
    VisibilityState,
};
pub use visibility_config::{
    load_visibility_config_from_env, DecayConfig, ElevationConfig, LineOfSightConfig,
    SightRangeConfig, TerrainModifierConfig, VisibilityConfig, VisibilityConfigHandle,
    VisibilityConfigMetadata, BUILTIN_VISIBILITY_CONFIG,
};
pub use wellbeing_config::{
    load_wellbeing_config_from_env, DiscontentConfig, MigrationConfig, ProductivityConfig,
    WellbeingConfig, WellbeingConfigHandle, WellbeingConfigMetadata, BUILTIN_WELLBEING_CONFIG,
};

pub use biome_palette::{BiomePalette, PALETTE_SEED_SALT};
pub use climate::{climate_band_for_temperature, ClimateBand};
pub use metrics::SimulationMetrics;
pub use orders::{
    FactionId, FactionOrders, FactionRegistry, Order, SubmitError, SubmitOutcome, TurnQueue,
};
pub use power::{
    PowerDiscoveryEffects, PowerGridNodeTelemetry, PowerGridState, PowerIncident,
    PowerIncidentSeverity, PowerNodeId, PowerTopology,
};
pub use provinces::{ProvinceId, ProvinceMap};
pub use resources::{
    apply_port_base, apply_port_base_override, carry_runtime_owned_fields,
    load_simulation_config_for_new_world, port_base_override, BandIdAllocator, CapabilityFlags,
    CommandEventEntry, CommandEventKind, CommandEventLog, CorruptionLedgers, CorruptionTelemetry,
    DiplomacyLeverage, DiscoveryProgressLedger, FactionInventory, FoodSiteEntry, FoodSiteRegistry,
    FoodSiteWaterBiasReport, HydrologyOverrides, MapTopology, MoistureRaster, PendingCrisisSeeds,
    PendingCrisisSpawns, SentimentAxisBias, SimulationConfig, SimulationConfigMetadata,
    SimulationTick, StartLocation, TileRegistry, TradeDiffusionRecord, TradeTelemetry, WorldEpoch,
};
pub use scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar};
pub use snapshot::{
    command_events_to_state, publish_baseline_snapshot, recapture_snapshot_in_place, FrameSink,
    SnapshotHistory, StoredSnapshot, NOT_FOOD_LIMITED_TURNS,
};
pub use systems::spawn_initial_world;
pub use systems::{
    advance_band_movement, advance_crafting, advance_expeditions, advance_labor_allocation,
    advance_predator_raids, advance_tick, bench_material_rate, bench_tiers, bill_and_stock_roads,
    denial_forecast, expedition_returned_event, expedition_take_provisions, fold_party_into_band,
    hunt_per_worker_provisions, hunt_report_event, hunt_take, hunt_trip_forecast,
    output_multiplier, party_owes_a_report, settle_bands_roadwork, simulate_power,
    source_has_a_meter_at_risk, split_band_from_parent, split_refusals, BenchTiers, DenialForecast,
    DenialOutcome, HuntOutcome, HuntTripBound, HuntTripForecast, MigrationKnowledgeEvent,
    PowerSimParams, SplitBand, SplitRefusal, SplitRefusals, TradeDiffusionEvent,
};
pub use systems::{
    apply_biome_palette_clamp, apply_tag_budget_solver, bias_food_sites_toward_fresh_water,
    reconcile_coastal_shelf, reconcile_food_modules,
};
pub use telling::{
    load_beat_catalog_from_env, load_beat_config_from_env, telling_tick, BeatCatalog,
    BeatCatalogHandle, BeatCatalogMetadata, BeatChoice, BeatConfig, BeatConfigHandle,
    BeatConfigMetadata, BeatDefinition, BeatLedger, BeatTier, ChoiceWrites, CompareOp, EdgeDir,
    ForkAnswerError, ForkResolution, Noun, NounField, PendingFork, Predicate, RenderedChoice,
    SignalSample, WardrobeEntry, TELLING_SEED_SALT,
};
pub use terrain::{
    biome_must_have, biome_niche, classify_terrain, terrain_definition, terrain_for_position,
    BathymetryContext, BiomeNiche, MovementProfile, TerrainDefinition, TerrainResourceBias,
};

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum TurnStage {
    Influence,
    Logistics,
    Knowledge,
    GreatDiscovery,
    Population,
    Visibility,
    Crisis,
    /// The Telling's beat engine. Between `Crisis` and `Finalize` so it sees population, fauna,
    /// sedentarization and crisis output, and lands before `Snapshot` so a beat reaches the client
    /// the same turn it fires.
    Telling,
    Finalize,
    Victory,
    Snapshot,
}

/// Construct a Bevy [`App`] configured with the Shadow-Scale turn pipeline.
pub fn build_headless_app() -> App {
    let mut app = App::new();

    let (mut config, config_metadata) = resources::load_simulation_config_from_env();
    let (map_presets, map_presets_metadata) = load_map_presets_from_env();
    let victory_config = load_victory_config_from_env();
    let preset_count = map_presets.len();
    if let Some(path) = map_presets_metadata.path() {
        tracing::debug!(
            target: "shadow_scale::mapgen",
            presets = preset_count,
            path = %path.display(),
            "map_presets.metadata.available"
        );
    } else {
        tracing::debug!(
            target: "shadow_scale::mapgen",
            presets = preset_count,
            "map_presets.metadata.builtin"
        );
    }
    let (start_profiles, start_profiles_metadata) = load_start_profiles_from_env();
    let start_profiles_handle = StartProfilesHandle::new(start_profiles.clone());
    let (knowledge_tags, knowledge_tags_metadata) = load_start_profile_knowledge_tags_from_env();
    let knowledge_tags_handle = StartProfileKnowledgeTagsHandle::new(knowledge_tags.clone());

    let profile_id = config.start_profile_id.clone();
    let (active_profile, used_fallback) =
        start_profile::resolve_active_profile(&start_profiles_handle, &profile_id);

    config.start_profile_overrides =
        start_profile::StartProfileOverrides::from_profile(&active_profile);

    if used_fallback {
        tracing::warn!(
            target: "shadow_scale::campaign",
            requested = %profile_id,
            fallback = %active_profile.id,
            "start_profiles.lookup.fallback"
        );
    }

    let campaign_label = CampaignLabel::from_profile(&active_profile);
    tracing::info!(
        target: "shadow_scale::campaign",
        profile = %active_profile.id,
        title = campaign_label.title.text_as_str().unwrap_or(""),
        title_loc_key = campaign_label.title.loc_key().unwrap_or(""),
        subtitle = campaign_label.subtitle.text_as_str().unwrap_or(""),
        subtitle_loc_key = campaign_label.subtitle.loc_key().unwrap_or(""),
        fallback = used_fallback,
        "campaign.label.active"
    );

    let active_profile_resource = ActiveStartProfile::new(active_profile.clone());
    let profile_lookup = StartProfileLookup::new(active_profile.id.clone());

    let faction_registry = orders::FactionRegistry::default();
    let turn_queue = orders::TurnQueue::new(faction_registry.factions.clone());
    // Depth is decided in ONE place — `snapshot::PUBLICATION_RING_DEPTH`, which
    // `capture_snapshot` no longer has to re-assert every turn.
    let snapshot_history = SnapshotHistory::with_capacity(snapshot::PUBLICATION_RING_DEPTH);
    let generation_registry = GenerationRegistry::with_seed(0xC0FEBABE, 6);
    let influencer_config = Arc::new(
        InfluencerBalanceConfig::from_json_str(BUILTIN_INFLUENCER_CONFIG)
            .expect("influencer config should parse"),
    );
    let influencer_roster =
        InfluentialRoster::with_seed(0xA51C_E55E, &generation_registry, influencer_config.clone());
    let influencer_config_handle = InfluencerConfigHandle::new(influencer_config);
    let knowledge_config = Arc::new(
        KnowledgeLedgerConfig::from_json_str(BUILTIN_KNOWLEDGE_LEDGER_CONFIG)
            .expect("knowledge ledger config should parse"),
    );
    let knowledge_ledger = KnowledgeLedger::with_config(knowledge_config.clone());
    let knowledge_config_handle = KnowledgeLedgerConfigHandle::new(knowledge_config);
    let culture_corruption_config = Arc::new(
        CultureCorruptionConfig::from_json_str(BUILTIN_CULTURE_CORRUPTION_CONFIG)
            .expect("culture corruption config should parse"),
    );
    let culture_manager =
        CultureManager::from_config(culture_corruption_config.culture().propagation());
    let culture_corruption_handle = CultureCorruptionConfigHandle::new(culture_corruption_config);
    let (turn_pipeline_config, turn_pipeline_metadata) = load_turn_pipeline_config_from_env();
    let turn_pipeline_handle = TurnPipelineConfigHandle::new(turn_pipeline_config.clone());
    let (snapshot_overlays_config, snapshot_overlays_metadata) =
        load_snapshot_overlays_config_from_env();
    let snapshot_overlays_handle = SnapshotOverlaysConfigHandle::new(snapshot_overlays_config);
    let (crisis_archetypes, crisis_archetypes_metadata) = load_crisis_archetypes_from_env();
    let crisis_archetypes_handle = CrisisArchetypeCatalogHandle::new(crisis_archetypes.clone());
    let (crisis_modifiers, crisis_modifiers_metadata) = load_crisis_modifiers_from_env();
    let crisis_modifiers_handle = CrisisModifierCatalogHandle::new(crisis_modifiers.clone());
    let (crisis_telemetry_config, crisis_telemetry_metadata) =
        load_crisis_telemetry_config_from_env();
    let crisis_telemetry_handle = CrisisTelemetryConfigHandle::new(crisis_telemetry_config.clone());
    let crisis_telemetry_resource = CrisisTelemetry::from_config(crisis_telemetry_config.as_ref());
    let (visibility_config, visibility_metadata) =
        visibility_config::load_visibility_config_from_env();
    let visibility_handle = visibility_config::VisibilityConfigHandle::new(visibility_config);
    // The connection primitive's three clocks. It loads beside visibility because contact is found
    // inside that sweep (`connections::ContactsThisTurn`).
    let (connections_config, connections_metadata) =
        connections_config::load_connections_config_from_env();
    let connections_handle = connections_config::ConnectionsConfigHandle::new(connections_config);
    // **The materials table loads FIRST of the three**, because both food webs' yield edges are
    // reconciled against it: a species (plant or animal) naming a material that does not exist, or
    // stating a reading on an axis that material does not declare, is a boot panic rather than a
    // source that silently yields nothing (`docs/plan_crafting_and_materials.md` §2).
    let (materials_config, materials_metadata) = materials_config::load_materials_config_from_env();
    let materials_handle = materials_config::MaterialsConfigHandle::new(materials_config.clone());
    let (fauna_config, fauna_metadata) =
        fauna_config::load_fauna_config_from_env(&materials_config);
    let fauna_handle = fauna_config::FaunaConfigHandle::new(fauna_config);
    let (labor_config, labor_metadata) = labor_config::load_labor_config_from_env();
    // The flora roster is validated AGAINST the human food web's own capacity table — every
    // food-bearing biome must be named, and no named plant may claim barren ground
    // (`FloraConfig::validate_against_forage`) — and against the materials table beside it. Both
    // tables are passed in rather than re-read so each keeps exactly one copy.
    let (flora_config, flora_metadata) = flora_config::load_flora_config_from_env(
        &labor_config.forage.capacity_by_biome,
        &materials_config,
    );
    let flora_handle = flora_config::FloraConfigHandle::new(flora_config);
    let labor_handle = labor_config::LaborConfigHandle::new(labor_config);
    let (ladder_config, ladder_metadata) = intensification::load_intensification_ladder_from_env();
    // **The ladder's MATERIAL half is reconciled against the materials table**, the same
    // `UnknownItem` debt the equipment roster and the two food webs' yield edges pay: a rung whose
    // pile or upkeep names a material that does not exist would otherwise parse, validate, and then
    // be raised and held for free for ever with no fault reported anywhere
    // (`docs/plan_standing_upkeep.md` §2.7).
    if let Err(err) = ladder_config.validate_against_materials(&materials_config) {
        panic!("intensification ladder does not reconcile with the materials table: {err}");
    }
    let ladder_handle = intensification::LadderConfigHandle::new(ladder_config);
    let (sedentarization_config, sedentarization_metadata) =
        sedentarization_config::load_sedentarization_config_from_env();
    let sedentarization_handle =
        sedentarization_config::SedentarizationConfigHandle::new(sedentarization_config);
    let (settlement_stage_config, settlement_stage_metadata) =
        settlement_stage_config::load_settlement_stage_config_from_env();
    let settlement_stage_handle =
        settlement_stage_config::SettlementStageConfigHandle::new(settlement_stage_config);
    let (beat_config, beat_config_metadata) = telling::load_beat_config_from_env();
    let (beat_catalog, beat_catalog_metadata) = telling::load_beat_catalog_from_env(&beat_config);
    let beat_config_handle = telling::BeatConfigHandle::new(beat_config);
    let beat_catalog_handle = telling::BeatCatalogHandle::new(beat_catalog);
    let (sites_config, sites_metadata) = sites_config::load_sites_config_from_env();
    let sites_handle = sites_config::SitesConfigHandle::new(sites_config);
    let (expedition_config, expedition_metadata) =
        expedition_config::load_expedition_config_from_env();
    let expedition_handle = expedition_config::ExpeditionConfigHandle::new(expedition_config);
    let (combat_config, combat_metadata) = combat_config::load_combat_config_from_env();
    let combat_handle = combat_config::CombatConfigHandle::new(combat_config);
    let (creatures_config, creatures_metadata) = creatures_config::load_creatures_config_from_env();
    let creatures_handle = creatures_config::CreaturesConfigHandle::new(creatures_config);
    let (equipment_config, equipment_metadata) = equipment_config::load_equipment_config_from_env();
    // **The item table's `bounds_material` is reconciled against the materials table**, the same
    // `UnknownItem` debt the two food webs' yield edges pay: a tool bounding `hyde` would parse,
    // validate, and then be the bench tool for nothing.
    if let Err(err) = equipment_config.validate_against_materials(&materials_config) {
        panic!("equipment config does not reconcile with the materials table: {err}");
    }
    let equipment_handle = equipment_config::EquipmentConfigHandle::new(equipment_config.clone());
    // **The recipe book is reconciled against BOTH tables**, here and only here, because this is the
    // one place all three configs are in scope at once — a recipe naming a material or an item that
    // does not exist would otherwise parse, validate, and make nothing forever.
    let (recipes_config, recipes_metadata) = recipes_config::load_recipes_config_from_env();
    if let Err(err) = recipes_config.validate_against(&materials_config, &equipment_config) {
        panic!("recipes config does not reconcile with the materials and equipment tables: {err}");
    }
    let recipes_handle = recipes_config::RecipesConfigHandle::new(recipes_config);
    let (demographics_config, demographics_metadata) =
        demographics_config::load_demographics_config_from_env();
    let demographics_handle =
        demographics_config::DemographicsConfigHandle::new(demographics_config);
    let (supply_network_config, supply_network_metadata) =
        supply_network_config::load_supply_network_config_from_env();
    let supply_network_handle =
        supply_network_config::SupplyNetworkConfigHandle::new(supply_network_config);
    let (wellbeing_config, wellbeing_metadata) = wellbeing_config::load_wellbeing_config_from_env();
    let wellbeing_handle = wellbeing_config::WellbeingConfigHandle::new(wellbeing_config);
    let culture_effects = CultureEffectsCache::default();
    let espionage_catalog =
        espionage::EspionageCatalog::load_builtin().expect("espionage catalog should parse");
    let mut espionage_roster = espionage::EspionageRoster::default();
    espionage_roster.seed_from_catalog(&faction_registry.factions, &espionage_catalog);
    let counter_intel_budgets = espionage::CounterIntelBudgets::new(
        &faction_registry.factions,
        espionage_catalog.config().counter_intel_budget(),
    );
    let security_policies = espionage::FactionSecurityPolicies::new(
        &faction_registry.factions,
        espionage::SecurityPolicy::Standard,
    );

    // Read before `config` is moved into the world: the log's turn window is a config lever, and
    // `CommandEventLog::default()` only knows the builtin default.
    let command_event_log =
        CommandEventLog::with_retention_turns(config.command_events_retention_turns);

    app.insert_resource(config)
        .insert_resource(config_metadata)
        .insert_resource(MapPresetsHandle::new(map_presets.clone()))
        .insert_resource(map_presets_metadata)
        .insert_resource(VictoryConfigHandle::new(victory_config.clone()))
        .insert_resource(VictoryState::new(victory_config.continue_after_win))
        .insert_resource(start_profiles_handle)
        .insert_resource(start_profiles_metadata)
        .insert_resource(knowledge_tags_handle)
        .insert_resource(knowledge_tags_metadata)
        // Snapshotted AFTER every `load_*_from_env` above, so it describes the tuning this world
        // actually booted on. Staging an override later moves the process-global registry — what the
        // NEXT `new_game` will boot on — and deliberately leaves this resource alone.
        .insert_resource(config_fingerprint::current_config_fingerprint())
        .insert_resource(active_profile_resource)
        .insert_resource(profile_lookup)
        .insert_resource(campaign_label)
        .insert_resource(resources::StartLocation::default())
        .insert_resource(hydrology::HydrologyState::default())
        .insert_resource(PowerGridState::default())
        .insert_resource(PowerTopology::default())
        .insert_resource(SimulationTick::default())
        // Default epoch (0) so `capture_snapshot` always finds the resource. The server overwrites
        // it with the live counter on every world (re)build; the idle boot app never captures.
        .insert_resource(WorldEpoch::default())
        .insert_resource(BandIdAllocator::default())
        .insert_resource(sim_state::Replaying::default())
        .insert_resource(CapabilityFlags::default())
        .insert_resource(SimulationMetrics::default())
        .insert_resource(crisis_telemetry_resource)
        .insert_resource(SentimentAxisBias::default())
        .insert_resource(knowledge_config_handle)
        .insert_resource(knowledge_ledger)
        .insert_resource(culture_corruption_handle)
        .insert_resource(snapshot_overlays_handle)
        .insert_resource(crisis_archetypes_handle)
        .insert_resource(crisis_modifiers_handle)
        .insert_resource(crisis_telemetry_handle)
        .insert_resource(ActiveCrisisLedger::default())
        .insert_resource(CrisisOverlayCache::default())
        .insert_resource(visibility_handle)
        .insert_resource(visibility_metadata)
        .insert_resource(connections_handle)
        .insert_resource(connections_metadata)
        .insert_resource(materials_handle)
        .insert_resource(materials_metadata)
        .insert_resource(recipes_handle)
        .insert_resource(recipes_metadata)
        .insert_resource(fauna_handle)
        .insert_resource(fauna_metadata)
        .insert_resource(flora_handle)
        .insert_resource(flora_metadata)
        .insert_resource(snapshot::FloraQuoteCache::default())
        .insert_resource(labor_handle)
        .insert_resource(labor_metadata)
        .insert_resource(ladder_handle)
        .insert_resource(ladder_metadata)
        .insert_resource(sedentarization_handle)
        .insert_resource(sedentarization_metadata)
        .insert_resource(sedentarization::SedentarizationScore::default())
        .insert_resource(beat_config_handle)
        .insert_resource(beat_config_metadata)
        .insert_resource(beat_catalog_handle)
        .insert_resource(beat_catalog_metadata)
        .insert_resource(telling::BeatLedger::default())
        .insert_resource(settlement_stage_handle)
        .insert_resource(settlement_stage_metadata)
        .insert_resource(sites_handle)
        .insert_resource(sites_metadata)
        .insert_resource(sites::DiscoveredSites::default())
        .insert_resource(expedition_handle)
        .insert_resource(expedition_metadata)
        .insert_resource(combat_handle)
        .insert_resource(combat_metadata)
        .insert_resource(creatures_handle)
        .insert_resource(creatures_metadata)
        .insert_resource(equipment_handle)
        .insert_resource(equipment_metadata)
        .insert_resource(demographics_handle)
        .insert_resource(demographics_metadata)
        .insert_resource(supply_network_handle)
        .insert_resource(supply_network_metadata)
        .insert_resource(wellbeing_handle)
        .insert_resource(wellbeing_metadata)
        .insert_resource(supply::SupplyNetworkMembership::default())
        .insert_resource(visibility::VisibilityLedger::default())
        .insert_resource(visibility::VisibilitySweepTracker::default())
        .insert_resource(connections::ConnectionLedger::default())
        // **The roads and this turn's traffic** (`docs/plan_standing_upkeep.md` §4.13). The registry
        // is world state; the traffic log is a within-turn hand-off from `balance_supply_networks`,
        // which knows which pairs pooled, to `routes::advance_roads`, which spends them.
        .insert_resource(routes::RoadRegistry::default())
        .insert_resource(routes::RouteTrafficLog::default())
        .insert_resource(connections::ContactsThisTurn::default())
        .insert_resource(visibility::ViewerFaction::default())
        .insert_resource(turn_pipeline_handle)
        .insert_resource(turn_pipeline_metadata)
        .insert_resource(snapshot_overlays_metadata)
        .insert_resource(crisis_archetypes_metadata)
        .insert_resource(crisis_modifiers_metadata)
        .insert_resource(crisis_telemetry_metadata)
        .insert_resource(CorruptionLedgers::default())
        .insert_resource(CorruptionTelemetry::default())
        .insert_resource(DiplomacyLeverage::default())
        .insert_resource(FactionInventory::default())
        .insert_resource(HerdRegistry::default())
        .insert_resource(HerdTelemetry::default())
        .insert_resource(HerdDensityMap::default())
        .insert_resource(ForageRegistry::default())
        .insert_resource(GrazeRegistry::default())
        .insert_resource(command_event_log)
        .insert_resource(FoodSiteRegistry::default())
        .init_resource::<FoodSiteWaterBiasReport>()
        .insert_resource(snapshot_history)
        .insert_resource(snapshot::SnapshotCaptureMode::default())
        .insert_resource(generation_registry)
        .insert_resource(espionage_catalog)
        .insert_resource(espionage_roster)
        .insert_resource(espionage::EspionageMissionState::default())
        .insert_resource(counter_intel_budgets)
        .insert_resource(security_policies)
        .insert_resource(influencer_config_handle)
        .insert_resource(influencer_roster)
        .insert_resource(InfluencerImpacts::default())
        .insert_resource(culture_manager)
        .insert_resource(culture_effects)
        .insert_resource(DiscoveryProgressLedger::default())
        .insert_resource(TradeTelemetry::default())
        .insert_resource(GreatDiscoveryRegistry::default())
        .insert_resource(GreatDiscoveryReadiness::default())
        .insert_resource(ObservationLedger::default())
        .insert_resource(GreatDiscoveryLedger::default())
        .insert_resource(GreatDiscoveryTelemetry::default())
        .insert_resource(PowerDiscoveryEffects::default())
        .insert_resource(PendingCrisisSeeds::default())
        .insert_resource(PendingCrisisSpawns::default())
        .insert_resource(faction_registry)
        .insert_resource(turn_queue)
        .add_event::<CultureTensionEvent>()
        .add_event::<CultureSchismEvent>()
        .add_event::<systems::TradeDiffusionEvent>()
        .add_event::<systems::MigrationKnowledgeEvent>()
        .add_event::<EspionageProbeEvent>()
        .add_event::<CounterIntelSweepEvent>()
        .add_event::<GreatDiscoveryCandidateEvent>()
        .add_event::<GreatDiscoveryResolvedEvent>()
        .add_event::<great_discovery::GreatDiscoveryEffectEvent>()
        .add_plugins(MinimalPlugins)
        .configure_sets(
            Update,
            (
                TurnStage::Influence,
                TurnStage::Logistics,
                TurnStage::Knowledge,
                TurnStage::GreatDiscovery,
                TurnStage::Population,
                TurnStage::Visibility,
                TurnStage::Crisis,
                TurnStage::Telling,
                TurnStage::Finalize,
                TurnStage::Victory,
                TurnStage::Snapshot,
            )
                .chain(),
        )
        // Per-stage profiler boundaries. Each marker is pinned between two neighbouring stage sets,
        // so `enter_stage` closes the stage that just finished and opens the next one — an RAII
        // guard cannot span two Bevy systems, which is why the boundaries are systems.
        //
        // Deliberately ungated: unlike the stages themselves these carry no capability `run_if`, so
        // a stage whose systems are gated off records ~0 rather than disappearing from the profile.
        // `begin_turn` is *not* called here — the server owns it, because order application and
        // snapshot broadcast happen outside `app.update()` and belong to the same turn's profile.
        .add_systems(
            Update,
            (
                turn_profile::stage_marker("influence").before(TurnStage::Influence),
                turn_profile::stage_marker("logistics")
                    .after(TurnStage::Influence)
                    .before(TurnStage::Logistics),
                turn_profile::stage_marker("knowledge")
                    .after(TurnStage::Logistics)
                    .before(TurnStage::Knowledge),
                turn_profile::stage_marker("great_discovery")
                    .after(TurnStage::Knowledge)
                    .before(TurnStage::GreatDiscovery),
                turn_profile::stage_marker("population")
                    .after(TurnStage::GreatDiscovery)
                    .before(TurnStage::Population),
                turn_profile::stage_marker("visibility")
                    .after(TurnStage::Population)
                    .before(TurnStage::Visibility),
                turn_profile::stage_marker("crisis")
                    .after(TurnStage::Visibility)
                    .before(TurnStage::Crisis),
                turn_profile::stage_marker("telling")
                    .after(TurnStage::Crisis)
                    .before(TurnStage::Telling),
                turn_profile::stage_marker("finalize")
                    .after(TurnStage::Telling)
                    .before(TurnStage::Finalize),
                turn_profile::stage_marker("victory")
                    .after(TurnStage::Finalize)
                    .before(TurnStage::Victory),
                turn_profile::stage_marker("snapshot")
                    .after(TurnStage::Victory)
                    .before(TurnStage::Snapshot),
                turn_profile::close_stages_system.after(TurnStage::Snapshot),
            ),
        )
        .add_systems(
            Startup,
            (
                systems::spawn_initial_world,
                systems::apply_starting_inventory_effects,
                hydrology::generate_hydrology,
                systems::apply_tag_budget_solver,
                systems::apply_biome_palette_clamp,
                systems::reconcile_coastal_shelf,
                systems::reconcile_food_modules,
                systems::bias_food_sites_toward_fresh_water,
                sites::place_wondrous_sites,
                spawn_initial_herds,
                spawn_initial_forage,
                spawn_initial_graze,
                espionage::initialise_espionage_roster,
            )
                .chain()
                .run_if(save::worldgen_wanted),
        )
        .add_systems(
            Update,
            (
                tick_influencers,
                // Must precede the reconcile: it is what gives a band its layer (and re-homes a
                // moved one) before the layer is resolved against its province.
                culture::reconcile_band_culture_layers,
                reconcile_culture_layers,
                systems::process_culture_events,
            )
                // Fully serial by data: every pair conflicts (the culture layers each writes).
                // `.chain()` here is the honest encoding, not a leftover default.
                .chain()
                .in_set(TurnStage::Influence)
                .run_if(capability_enabled(CapabilityFlags::ALWAYS_ON)),
        )
        .add_systems(
            Update,
            (
                // The serial backbone. Each of these conflicts with the next on the registry it
                // mutates — `HerdRegistry` down the fauna run, `FactionInventory` at the ends —
                // so the executor could not overlap them even if they were left unordered.
                (
                    systems::simulate_materials,
                    advance_herds,
                    advance_herd_grazing,
                    advance_predation,
                    repopulate_fauna,
                    advance_husbandry,
                )
                    .chain(),
                // The flora/pasture half touches `ForageRegistry`/`GrazeRegistry`, which the fauna
                // backbone above does not, so it runs alongside it. Each declares the one edge it
                // actually has rather than inheriting the whole chain.
                advance_forage_regrowth.after(systems::simulate_materials),
                // The second edge is the feed line: `advance_cultivation` announces a lost plant
                // rung and `advance_husbandry` a lost pen / an under-herded flock, so the two now
                // share `CommandEventLog` and the order they append in is observable. The plant
                // pass goes first, matching the order the two webs already read in the Population
                // stage.
                advance_cultivation
                    .after(advance_forage_regrowth)
                    .before(advance_husbandry),
                advance_graze_regrowth.after(advance_herd_grazing),
                supply::balance_supply_networks.after(advance_herds),
                // ⛔ **AFTER THE POOLING, ALWAYS.** This spends the links that pass recorded, so it
                // has to see them — and the ordering is what makes the payoff a *previous*-turn
                // reading, the same one-turn lag `balance_supply_networks` already accepts against
                // the connection ledger. Reversing it would let this turn's pooling read a road
                // this turn's pooling created.
                routes::advance_roads.after(supply::balance_supply_networks),
                // ⛔ **AFTER THE ROAD PASS, ALWAYS** — declared rather than left to the ambiguity
                // gate. A connection's lesson is read off the road standing `advance_roads` has just
                // produced, so this turn's decay, banking and prune are all already in the registry
                // when the lesson rung is resolved.
                routes::credit_route_lessons.after(routes::advance_roads),
            )
                .in_set(TurnStage::Logistics)
                .run_if(capability_enabled(
                    CapabilityFlags::CONSTRUCTION
                        | CapabilityFlags::INDUSTRY_T1
                        | CapabilityFlags::INDUSTRY_T2
                        | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                espionage::refresh_counter_intel_budgets,
                espionage::schedule_counter_intel_missions,
                espionage::resolve_espionage_missions,
                knowledge_ledger::process_espionage_events,
                knowledge_ledger::knowledge_ledger_tick,
            )
                // `refresh_counter_intel_budgets` is free of the last three, but it must precede
                // `schedule_counter_intel_missions`, which precedes them — so the freedom is
                // already implied and a chain costs nothing. Measured, not assumed.
                .chain()
                .in_set(TurnStage::Knowledge)
                .run_if(capability_enabled(
                    CapabilityFlags::ESPIONAGE_T2 | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                (
                    great_discovery::collect_observation_signals,
                    great_discovery::update_constellation_progress,
                    great_discovery::screen_great_discovery_candidates,
                    great_discovery::resolve_great_discovery,
                    great_discovery::propagate_diffusion_impacts,
                    great_discovery::export_great_discovery_metrics,
                )
                    .chain(),
                // Writes `CapabilityFlags`, which nothing else in the stage reads — so it only
                // needs the discovery that grants the capability, not the reporting tail.
                great_discovery::apply_capability_effects
                    .after(great_discovery::resolve_great_discovery),
            )
                .in_set(TurnStage::GreatDiscovery)
                .run_if(capability_enabled(
                    CapabilityFlags::MEGAPROJECTS | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                // This whole run is serial by data — every pair conflicts on `PopulationCohort`,
                // and the two `Commands`-using systems (`advance_band_movement`,
                // `advance_expeditions`) sit inside it, so the auto-inserted `apply_deferred`
                // sync points between them are preserved exactly as before.
                (
                    systems::simulate_population,
                    // Move first so the band's `current_tile` is current before labor reads its
                    // in-range sources, then resolve per-worker Forage/Hunt/Scout yields.
                    systems::advance_band_movement,
                    // Expedition per-turn logic (observe into the pending-reveal buffer, comm-range
                    // flush-to-Discovered, return-retarget, arrival/fold-back). Runs right after
                    // movement so it reads the party's fresh position, and before the Visibility stage's
                    // `discover_sites` picks up any site on the newly-flushed Discovered tiles.
                    systems::advance_expeditions,
                    systems::advance_labor_allocation,
                    // The bench runs right after labor, for two reasons that both have to hold: it
                    // draws on the materials THIS turn's take just delivered, and its crew came out
                    // of the same worker pool the assignment loop above spends.
                    systems::advance_crafting,
                    // Predator raids fire right after labor so warrior counts and band positions are
                    // current: a carnivore with `aggression > 0` within `predators.raid_radius` of a band
                    // raids its camp, the band defended by its Warriors (the role's first live consumer).
                    systems::advance_predator_raids,
                    // Wellbeing migration runs after demographics + this turn's yield payouts so
                    // morale/discontent are current and productivity has already been applied at each
                    // yield site; it then relocates discontented people (population conserved).
                    systems::advance_population_migration,
                    sedentarization::sedentarization_tick,
                )
                    .chain(),
                // Pure telemetry: writes `TradeTelemetry`, which only `simulate_population`
                // touches. It rides alongside the whole movement/labor run instead of tailing it.
                systems::publish_trade_telemetry.after(systems::simulate_population),
                // **RETIRED: `settle_route_keeping`, the third keeping pool as a SYSTEM OF ITS
                // OWN.** It ran `.after(advance_labor_allocation)` because the `roadwork` head count
                // it divides has to be the one the shedding order left — and that put the payment a
                // whole system *behind* the road build quote, which reads `Road::upkeep_supplied`
                // through `routes::road_meter_rot`. `routes::advance_roads` clears that field a
                // stage earlier and this was its only writer, so every road quoted its rot at a work
                // shortfall of `1.0` and a fully funded road past its grace published the full rot
                // instead of `0`. The payment is now `systems::settle_bands_roadwork`, called from
                // inside `advance_labor_allocation` after the shed and ahead of that pass's
                // `continue`s — the seat both food webs already settle their keeping from, and the
                // only one that is after the shed and before the quote. Its keeping wear still lands
                // before `advance_crafting` for the same reason it did as a system: it is charged
                // inside the labour pass, which the bench already follows.
                // ⛔ **HOLDING WHAT YOU HAVE OUTRANKS EXPANDING** — the roads' bill and the STONE
                // that pays it are struck BEFORE the builders run, so a band's standing paved roads
                // take their material before a new paving build may touch the store. While the
                // build pile settled inside `advance_labor_allocation` and the standing rate
                // settled after it, the build simply got there first: pushing a road out quietly
                // starved the roads already under it. See `bill_and_stock_roads` for why the DRAW
                // moved rather than the STAMP being split.
                //
                // Both edges are declared rather than left to the ambiguity gate — it takes
                // `PopulationCohort` mutably to spend the stone, so its order against the movement
                // chain is not the scheduler's to guess:
                //  - `.after(advance_expeditions)` — it slots into the chain immediately before
                //    labor, so the bill is struck on the positions this turn's movement left.
                //  - `.before(advance_labor_allocation)` — the ordering this system exists for.
                systems::bill_and_stock_roads
                    .after(systems::advance_expeditions)
                    .before(systems::advance_labor_allocation),
            )
                .in_set(TurnStage::Population)
                .run_if(capability_enabled(
                    CapabilityFlags::CONSTRUCTION
                        | CapabilityFlags::INDUSTRY_T1
                        | CapabilityFlags::INDUSTRY_T2
                        | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                // Everything here funnels through `VisibilityLedger`, so it is a serial run.
                (
                    visibility_systems::clear_active_visibility,
                    visibility_systems::calculate_visibility,
                    // Right behind the sweep that FOUND the contacts: it consumes
                    // `ContactsThisTurn` (which `calculate_visibility` and the expedition flush
                    // both fill) and clears it, so the set is rebuilt from scratch every turn.
                    connections::advance_connections,
                    // ⛔ **A KEPT ROAD IS ITS OWN VISIBILITY SOURCE, beside a band's presence and
                    // never through the connection grant** — see `light_kept_routes`. It runs after
                    // the sweep (the fog it writes into is the sweep's) and before the decay, so a
                    // road's tiles are `Active` for the same turn a band's own camp is.
                    visibility_systems::light_kept_routes,
                    visibility_systems::apply_visibility_decay,
                    sites::discover_sites,
                )
                    .chain(),
                // Touches only `VisibilitySweepTracker`, so it overlaps the ledger clear rather
                // than sitting between the two.
                visibility_systems::prune_sweep_tracker
                    .before(visibility_systems::calculate_visibility),
            )
                .in_set(TurnStage::Visibility)
                .run_if(capability_enabled(CapabilityFlags::ALWAYS_ON)),
        )
        .add_systems(
            Update,
            crisis::advance_crisis_system.in_set(TurnStage::Crisis),
        )
        .add_systems(Update, telling::telling_tick.in_set(TurnStage::Telling))
        .add_systems(
            Update,
            // Both write `CorruptionLedgers`; nothing to overlap.
            (systems::simulate_power, systems::process_corruption)
                .chain()
                .in_set(TurnStage::Finalize)
                .run_if(capability_enabled(
                    CapabilityFlags::POWER | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                metrics::collect_metrics,
                systems::advance_tick,
                // **Before the capture, and only on the turn path.** The transfer counters are the
                // one ledger pair that resets, so the frame a *recapture* rebuilds after a command
                // would publish them blank; this copies them onto the cohort, where they are a
                // per-turn value like the other four terms and survive a rebuild.
                systems::publish_turn_transfers,
                // Gated off `Replaying`: a rollback replaying the command log forward must not
                // re-publish frames the client already applied. Its stage-mates above are
                // deliberately NOT gated — see `sim_state::Replaying`.
                snapshot::capture_snapshot.run_if(sim_state::not_replaying),
                // **After the capture has read them.** The band-to-band transfer counters accumulate
                // across the whole snapshot window — commands included — so the one place they may
                // be cleared is behind the publication that reports them.
                systems::reset_transfer_ledger,
            )
                // `SimulationTick` then `SimulationMetrics` — a genuine sequence, and the one
                // stage where running out of order would publish the wrong tick.
                .chain()
                .in_set(TurnStage::Snapshot),
        );

    app.add_systems(Update, victory::victory_tick.in_set(TurnStage::Victory));

    {
        // Log chosen map preset id; worldgen consumes later.
        if let Some(preset) = map_presets.get(
            &app.world
                .resource::<resources::SimulationConfig>()
                .map_preset_id,
        ) {
            tracing::info!(
                target: "shadow_scale::mapgen",
                preset_id = %preset.id,
                name = %preset.name,
                "mapgen.preset.selected"
            );
        } else {
            tracing::warn!(
                target: "shadow_scale::mapgen",
                preset_id = %app.world.resource::<resources::SimulationConfig>().map_preset_id,
                "mapgen.preset.missing_using_first"
            );
        }
        let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
        let loaded = registry
            .load_catalog_from_str(great_discovery::BUILTIN_GREAT_DISCOVERY_CATALOG)
            .expect("Great Discovery catalog should parse");
        tracing::info!(
            target: "shadow_scale::great_discovery",
            loaded_definitions = loaded,
            "great_discovery.catalog.loaded"
        );
    }

    // Ambiguity = two systems with conflicting data access and no ordering edge between them.
    // With the multi-threaded executor that is precisely the determinism hazard: the pair cannot
    // run in parallel anyway (the executor serializes conflicting access), so leaving them
    // unordered doesn't buy a core — it just lets the executor pick a winner, and
    // `integration_tests/tests/determinism.rs` stops holding.
    //
    // Set to `Error`, the schedule build *panics* on such a pair. That is what makes de-chaining
    // safe to do at all: dropping an edge that mattered fails loudly at startup instead of
    // silently making a turn order-dependent. It is also what keeps the property — a system added
    // later without declaring its real edges cannot boot.
    //
    // Scoped to `Update` (the turn schedule) on purpose: `Startup` runs once and its systems stay
    // `.chain()`-ed, and bevy's own schedules are not ours to hold to this bar.
    app.edit_schedule(Update, |schedule| {
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            ..default()
        });
    });

    app
}

/// **THE MAP EVERY TEST RUNS ON.** A `map_seed` a test can rely on, so a fixture that searches the
/// world for ground with a property finds the *same* ground on every run.
///
/// # Why a harness needs one at all
///
/// `simulation_config.json` ships `map_seed: 0`, and worldgen reads that as *"draw a seed from
/// entropy"* (`systems::worldgen`, the `world_seed == 0` branch) — which is the game working
/// correctly: a New Game gets a random map. But it means [`build_headless_app`] generates **a
/// different world every call**, and a test that asks the map for "a cultivable gathering site" or
/// "a bare sowable tile" gets different terrain each run. That produced real intermittent failures:
/// an assertion satisfied by whichever biome the search happened to land on passes most runs and
/// fails on the map where it doesn't.
///
/// Measured rather than assumed — with the seed free, a hash over every `(x, y, terrain)` differed
/// on all ten runs and the curated site list moved between 129 and 133 entries; with it pinned, that
/// hash, the site list (ordered *and* as a set) and the tile a fixture picks are byte-identical
/// across runs. **Worldgen is deterministic per seed**; it was only ever the seed that moved.
///
/// # The number
///
/// `119304647` is the seed `map_presets.json`'s `polar_contrast` preset already names and the one
/// `tests/forage_cultivation.rs` had pinned by hand long before this helper existed — reused rather
/// than invented so the repo has **one** harness map instead of two.
///
/// **⛔ IT IS NOT A TUNING DIAL.** When a test fails on this map, the map is not wrong: the test was
/// resting on map luck, and the fix is to give that fixture the ground it needs (state its terrain,
/// the way `tests/build_turns_on_the_wire.rs` does) — never to shop for a seed on which everything
/// happens to pass, which would put the harness right back on the map it just got off.
pub const HARNESS_MAP_SEED: u64 = 119304647;

/// **THE ONE WORLD BUILDER A TEST MAY USE** — [`build_headless_app`] with [`HARNESS_MAP_SEED`]
/// pinned, so the map is the same on every run.
///
/// # Why it can pin the seed from out here
///
/// [`build_headless_app`] **runs no `update()`** — it only assembles the `App` and inserts the
/// resources — and it never reads or resolves `map_seed` itself. The seed is resolved inside
/// `spawn_initial_world`, a **Startup** system, which first runs at the caller's first `update()`.
/// So writing the resource between the two is in time, and worldgen's `world_seed == 0` branch never
/// fires. (A preset may still override it — `polar_contrast` names its own seed — and that is
/// deliberate: a test that asks for a preset is asking for that preset's map.)
///
/// **`build_headless_app` is deliberately left exactly as it is.** It is the *production* world
/// builder (`bin/server.rs` boots the real server with it), so pinning a seed in there would make
/// every New Game deterministic — changing the game to fix the tooling. The pin belongs here, on the
/// path only tests take.
/// # ⛔ IT ALSO STOCKS THE BAND'S GEAR, FOR THE SEED'S OWN REASON
///
/// `equipment.json`'s `start_stock_fraction` ships **`0.0`** — a spawning band owns nothing
/// (`equipment.md` → "A SPAWNING BAND OWNS NO EQUIPMENT AT ALL"). That is the *game's* opening, and
/// it is not a fixture input: a hundred-odd tests that mean *"a band with working gear"* used to say
/// so by **saying nothing**, so a tuning lever nobody thought was a test input moved a hundred
/// expected values at once.
///
/// So this builder installs [`EquipmentConfigHandle::for_a_stocked_fixture`] — the pre-change spawn,
/// `ceil(workers × 1.5 / workers_per_unit)` — exactly as it pins the map seed, and for the same
/// stated reason: **a test resting on shipped luck is the test that is wrong.** A fixture whose
/// subject genuinely *is* the shipped opening overrides the resource back, the way a fixture that
/// wants a particular map states its terrain.
pub fn build_test_app() -> App {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = HARNESS_MAP_SEED;
    app.world
        .insert_resource(crate::equipment_config::EquipmentConfigHandle::for_a_stocked_fixture());
    app
}

/// Execute a single simulation turn.
///
/// Each call runs the [`TurnStage`] sets chained in [`build_headless_app`], in order:
/// Influence → Logistics → Knowledge → GreatDiscovery → Population → Visibility → Crisis →
/// Telling → Finalize → Victory → Snapshot.
///
/// Individual systems are not stages: `simulate_materials` runs inside `Logistics`,
/// `simulate_power` inside `Finalize`, and `advance_tick` inside `Snapshot`.
///
/// Callers are responsible for snapshot broadcasting and command handling.
pub fn run_turn(app: &mut App) {
    app.update();
}

fn capability_enabled(flags: CapabilityFlags) -> impl FnMut(Res<CapabilityFlags>) -> bool {
    move |current: Res<CapabilityFlags>| current.intersects(flags)
}
