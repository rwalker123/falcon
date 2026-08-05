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
mod config_load;
pub mod config_override;
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
pub mod metrics;
pub mod network;
mod orders;
pub mod port_alloc;
mod power;
mod provinces;
mod resources;
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
    available_workers, floor_is_valid, floor_overdraws, raid_is_recurring, BandEquipment, BandId,
    BandTravel, DeathCause, DemographicFlowAccumulator, ElementKind, Expedition, ExpeditionMission,
    ExpeditionPhase, Improvement, KnowledgeFragment, LaborAllocation, LaborAssignment, LaborTarget,
    LocalStore, LogisticsLink, MoraleCause, PendingMigration, PopulationCohort, PowerNode,
    ResidentBand, Settlement, SourceYield, StartingUnit, Tile, TownCenter, TradeLink, YieldRange,
    DEFAULT_ESCAPEMENT_FLOOR, FODDER, FOOD, NO_FILL_TARGET, NO_IMPROVEMENT_UNDERWAY, NO_RAID_FLOOR,
    STRIP_IT_BARE, TRADE_GOODS,
};
pub use config_load::ConfigLoadError;
pub use config_override::{
    clear_config_overrides, install_config_override, spec_for as config_override_spec_for,
    ConfigKindSpec, ConfigOverrideError, InstalledOverride,
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
    reconcile_band_culture_layers, reconcile_culture_layers, seeded_modifiers_for_band,
    CultureEffectsCache, CultureLayer, CultureLayerId, CultureLayerScope, CultureManager,
    CultureOwner, CultureSchismEvent, CultureTensionEvent, CultureTensionKind,
    CultureTensionRecord, CultureTraitAxis, CultureTraitVector, CULTURE_TRAIT_AXES,
    FALLBACK_CULTURE_REGION_ID,
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
    load_equipment_config_from_env, BasketKitConfig, EquipmentConfig, EquipmentConfigHandle,
    EquipmentConfigMetadata, HuntingKitConfig, SledKitConfig, BUILTIN_EQUIPMENT_CONFIG,
};
pub use espionage::{
    AgentAssignment, CounterIntelBudgets, EspionageAgentHandle, EspionageCatalog,
    EspionageMissionId, EspionageMissionInstanceId, EspionageMissionKind, EspionageMissionState,
    EspionageMissionTemplate, EspionageRoster, FactionSecurityPolicies, QueueMissionError,
    QueueMissionParams, SecurityPolicy,
};
pub use expedition_config::{
    load_expedition_config_from_env, ExpeditionConfig, ExpeditionConfigHandle,
    ExpeditionConfigMetadata, BUILTIN_EXPEDITION_CONFIG,
};
pub use fauna::{
    advance_herd_grazing, advance_herds, advance_husbandry, advance_predation, animals_affordable,
    animals_engaged, animals_that_stay, build_prey_index, carnivore_k_at, escapement_ceiling,
    forecast_expected_take, forecast_take_range, herd_capacity, herd_ecology, herd_herders_needed,
    herd_hunt_yield, herd_past_recovery, herd_quarry_fight, herded_fraction, herders_needed,
    hunt_engage_workers, hunt_escapement_ceiling, hunt_haul_workers, hunt_source_yield_preview,
    hunt_take_bound, hunt_take_workers, pen_upkeep, project_arrivals_hunt, project_realized_hunt,
    quantise_animal_take, repopulate_fauna, resolve_hunt_fight, retreat_seed, spawn_initial_herds,
    species_requires_denial, AnimalTake, EcologyPhase, EngagementStop, FightCasualties, Herd,
    HerdDensityMap, HerdRegistry, HerdTelemetry, HerdTelemetryEntry, HuntDraw, HuntFight,
    HuntTakeBound, HuntingParty, PreyDatum, QuarryFight, RoamState, SourceYieldForecast, TakeRange,
    FODDERING_DISCOVERY_ID, FULLY_HERDED, HERDING_DISCOVERY_ID, MSY_BIOMASS_FRACTION,
    NO_DEATHS_TO_REPORT, PENNING_DISCOVERY_ID,
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
    advance_cultivation, advance_forage_regrowth, commit_fodder_payoff, commit_payoff,
    commit_trade_payoff, commit_yield_ratio, composition_for_rung, default_species_for_rung,
    forage_per_worker_biomass, forage_provisions, forage_source_yield_preview, patch_composition,
    patch_provisions_per_biomass, patch_species_quality, patch_trade_per_biomass,
    project_arrivals_forage, project_realized_forage, resolve_committed_species, rung_payoff,
    rung_site_refusal, spawn_initial_forage, species_is_legal_here, tended_take_fodder,
    tended_take_trade_goods, tile_flora_composition, tile_forage_capacity, tile_is_fresh_watered,
    wild_payoff, ForagePatch, ForageRegistry, SpeciesRefusal, CANNOT_CLIMB_RATIO,
    CULTIVATION_DISCOVERY_ID, NO_FORAGE_SEASON, SEED_SELECTION_DISCOVERY_ID, WHOLE_BASKET,
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
    knows, learn_multiplier, load_intensification_ladder_from_env, BuildDips, LadderConfig,
    LadderConfigHandle, LadderConfigMetadata, RungBehavior, RungBranch, RungBuild, RungDef,
    RungFeeding, RungHarvest, RungKey, RungMovement, RungSiteRequirement, SiteRefusal,
    BUILTIN_INTENSIFICATION_LADDER, MANAGED_SOURCE_FLOOR, NO_BUILD_UNDERWAY_DIP, RUNG_COMPLETE,
    RUNG_TIMESCALE_UNSCALED, RUNG_UNSTARTED, SITE_ACCEPTED,
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
    resolve_active_profile, snapshot_profiles, ActiveStartProfile, CampaignLabel, StartProfile,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartProfileKnowledgeTagsMetadata,
    StartProfileLookup, StartProfileOverrides, StartProfilesHandle, StartProfilesMetadata,
    StartingUnitSpec,
};
pub use supply::{balance_supply_networks, SupplyNetworkMembership};
pub use supply_network_config::{
    load_supply_network_config_from_env, SupplyNetworkConfig, SupplyNetworkConfigHandle,
    SupplyNetworkConfigMetadata,
};
pub use turn_pipeline_config::{
    load_turn_pipeline_config_from_env, LogisticsPhaseConfig, PopulationPhaseConfig,
    PowerPhaseConfig, TradePhaseConfig, TurnPipelineConfig, TurnPipelineConfigHandle,
    TurnPipelineConfigMetadata, BUILTIN_TURN_PIPELINE_CONFIG,
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
    apply_port_base, apply_port_base_override, load_simulation_config_for_new_world,
    port_base_override, BandIdAllocator, CapabilityFlags, CommandEventEntry, CommandEventKind,
    CommandEventLog, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage,
    DiscoveryProgressLedger, FactionInventory, FoodSiteEntry, FoodSiteRegistry,
    FoodSiteWaterBiasReport, HydrologyOverrides, MapTopology, MoistureRaster, PendingCrisisSeeds,
    PendingCrisisSpawns, SentimentAxisBias, SimulationConfig, SimulationConfigMetadata,
    SimulationTick, StartLocation, TileRegistry, TradeDiffusionRecord, TradeTelemetry, WorldEpoch,
};
pub use scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar};
pub use snapshot::{
    command_events_to_state, recapture_snapshot_in_place, FrameSink, SnapshotHistory,
    StoredSnapshot,
};
pub use systems::spawn_initial_world;
pub use systems::{
    advance_band_movement, advance_expeditions, advance_labor_allocation, advance_predator_raids,
    denial_forecast, expedition_take_provisions, hunt_per_worker_provisions, hunt_report_event,
    hunt_take, hunt_trip_forecast, output_multiplier, simulate_power, DenialForecast,
    DenialOutcome, HuntOutcome, HuntTripBound, HuntTripForecast, MigrationKnowledgeEvent,
    PowerSimParams, TradeDiffusionEvent,
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
    let (fauna_config, fauna_metadata) = fauna_config::load_fauna_config_from_env();
    let fauna_handle = fauna_config::FaunaConfigHandle::new(fauna_config);
    let (labor_config, labor_metadata) = labor_config::load_labor_config_from_env();
    // The flora roster is validated AGAINST the human food web's own capacity table — every
    // food-bearing biome must be named, and no named plant may claim barren ground
    // (`FloraConfig::validate_against_forage`). The table is passed in rather than re-read so it
    // keeps exactly one copy.
    let (flora_config, flora_metadata) =
        flora_config::load_flora_config_from_env(&labor_config.forage.capacity_by_biome);
    let flora_handle = flora_config::FloraConfigHandle::new(flora_config);
    let labor_handle = labor_config::LaborConfigHandle::new(labor_config);
    let (ladder_config, ladder_metadata) = intensification::load_intensification_ladder_from_env();
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
    let equipment_handle = equipment_config::EquipmentConfigHandle::new(equipment_config);
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
                .chain(),
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
                    systems::simulate_logistics,
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
                advance_forage_regrowth.after(systems::simulate_logistics),
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
                systems::trade_knowledge_diffusion.after(repopulate_fauna),
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
                    visibility_systems::apply_trade_route_visibility,
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
                // Gated off `Replaying`: a rollback replaying the command log forward must not
                // re-publish frames the client already applied. Its stage-mates above are
                // deliberately NOT gated — see `sim_state::Replaying`.
                snapshot::capture_snapshot.run_if(sim_state::not_replaying),
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
