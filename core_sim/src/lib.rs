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
mod components;
mod crisis;
mod crisis_config;
mod culture;
mod culture_corruption_config;
mod demographics_config;
mod espionage;
mod expedition_config;
mod fauna;
mod fauna_config;
mod food;
mod forage;
mod generations;
mod great_discovery;
pub mod grid_utils;
pub mod hashing;
mod heightfield;
mod hydrology;
mod influencers;
mod knowledge_ledger;
mod labor_config;
pub mod log_stream;
mod map_preset;
mod mapgen;
pub mod metrics;
pub mod network;
mod orders;
mod power;
mod provinces;
mod resources;
mod scalar;
mod sedentarization;
mod sedentarization_config;
mod settlement_stage_config;
mod sites;
mod sites_config;
mod snapshot;
mod snapshot_overlays_config;
mod start_profile;
mod supply;
mod supply_network_config;
mod systems;
mod terrain;
mod turn_pipeline_config;
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
use bevy::prelude::*;

pub use components::{
    available_workers, BandTravel, ElementKind, Expedition, ExpeditionMission, ExpeditionPhase,
    FollowPolicy, KnowledgeFragment, LaborAllocation, LaborAssignment, LaborTarget, LocalStore,
    LogisticsLink, MoraleCause, PendingMigration, PopulationCohort, PowerNode, ResidentBand,
    Settlement, SourceYield, StartingUnit, Tile, TownCenter, TradeLink, FOOD,
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
    reconcile_culture_layers, CultureEffectsCache, CultureLayer, CultureLayerId, CultureLayerScope,
    CultureManager, CultureOwner, CultureSchismEvent, CultureTensionEvent, CultureTensionKind,
    CultureTensionRecord, CultureTraitAxis, CultureTraitVector, CULTURE_TRAIT_AXES,
};
pub use culture_corruption_config::{
    CorruptionSeverityConfig, CultureCorruptionConfig, CultureCorruptionConfigHandle,
    CultureSeverityConfig, CultureTensionTuning, BUILTIN_CULTURE_CORRUPTION_CONFIG,
};
pub use demographics_config::{
    load_demographics_config_from_env, DemographicsConfig, DemographicsConfigHandle,
    DemographicsConfigMetadata,
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
    advance_herds, advance_husbandry, forecast_expected_take, hunt_policy_ceiling, hunt_provisions,
    hunt_source_yield_preview, repopulate_fauna, spawn_initial_herds, EcologyPhase, Herd,
    HerdDensityMap, HerdRegistry, HerdTelemetry, HerdTelemetryEntry, RoamState,
    SourceYieldForecast, HERDING_DISCOVERY_ID,
};
pub use fauna_config::{
    load_fauna_config_from_env, FaunaConfig, FaunaConfigHandle, FaunaConfigMetadata, SizeClass,
    SpeciesDef, BUILTIN_FAUNA_CONFIG,
};
pub use food::{
    classify_food_module, classify_food_module_from_traits, FoodModule, FoodModuleTag,
    FoodSiteKind, DEFAULT_HARVEST_TRAVEL_TILES_PER_TURN, DEFAULT_HARVEST_WORK_TURNS,
};
pub use forage::{
    advance_cultivation, advance_forage_regrowth, forage_source_yield_preview,
    spawn_initial_forage, ForagePatch, ForageRegistry, CULTIVATION_DISCOVERY_ID,
};
pub use generations::{GenerationBias, GenerationId, GenerationProfile, GenerationRegistry};
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
pub use knowledge_ledger::{
    CounterIntelSweepEvent, EspionageProbeEvent, KnowledgeCountermeasure, KnowledgeLedger,
    KnowledgeLedgerConfig, KnowledgeLedgerConfigHandle, KnowledgeLedgerEntry, KnowledgeModifier,
    KnowledgeTimelineEvent, BUILTIN_KNOWLEDGE_LEDGER_CONFIG,
};
pub use labor_config::{
    load_labor_config_from_env, LaborConfig, LaborConfigHandle, LaborConfigMetadata,
    BUILTIN_LABOR_CONFIG,
};
pub use map_preset::{MapPreset, MapPresets, MapPresetsHandle};
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
    FogOverlayConfig, MilitaryOverlayConfig, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    SnapshotOverlaysConfigMetadata, BUILTIN_SNAPSHOT_OVERLAYS_CONFIG,
};
pub use start_profile::{
    resolve_active_profile, snapshot_profiles, ActiveStartProfile, CampaignLabel, FogMode,
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
    apply_port_base_override, CapabilityFlags, CommandEventEntry, CommandEventKind,
    CommandEventLog, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage,
    DiscoveryProgressLedger, FactionInventory, FogRevealLedger, FoodSiteEntry, FoodSiteRegistry,
    HydrologyOverrides, MapTopology, PendingCrisisSeeds, PendingCrisisSpawns, SentimentAxisBias,
    SimulationConfig, SimulationConfigMetadata, SimulationTick, StartLocation, TileRegistry,
    TradeDiffusionRecord, TradeTelemetry,
};
pub use scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar};
pub use snapshot::{
    command_events_to_state, recapture_snapshot_in_place, restore_world_from_snapshot,
    SnapshotHistory, StoredSnapshot,
};
pub use systems::spawn_initial_world;
pub use systems::{
    advance_band_movement, advance_expeditions, advance_labor_allocation,
    expedition_take_provisions, hunt_per_worker_provisions, hunt_take, hunt_trip_forecast,
    output_multiplier, simulate_power, HuntTripForecast, MigrationKnowledgeEvent, PowerSimParams,
    TradeDiffusionEvent,
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
    let snapshot_history = SnapshotHistory::with_capacity(config.snapshot_history_limit.max(1));
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
    let labor_handle = labor_config::LaborConfigHandle::new(labor_config);
    let (sedentarization_config, sedentarization_metadata) =
        sedentarization_config::load_sedentarization_config_from_env();
    let sedentarization_handle =
        sedentarization_config::SedentarizationConfigHandle::new(sedentarization_config);
    let (settlement_stage_config, settlement_stage_metadata) =
        settlement_stage_config::load_settlement_stage_config_from_env();
    let settlement_stage_handle =
        settlement_stage_config::SettlementStageConfigHandle::new(settlement_stage_config);
    let (sites_config, sites_metadata) = sites_config::load_sites_config_from_env();
    let sites_handle = sites_config::SitesConfigHandle::new(sites_config);
    let (expedition_config, expedition_metadata) =
        expedition_config::load_expedition_config_from_env();
    let expedition_handle = expedition_config::ExpeditionConfigHandle::new(expedition_config);
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
        .insert_resource(labor_handle)
        .insert_resource(labor_metadata)
        .insert_resource(sedentarization_handle)
        .insert_resource(sedentarization_metadata)
        .insert_resource(sedentarization::SedentarizationScore::default())
        .insert_resource(settlement_stage_handle)
        .insert_resource(settlement_stage_metadata)
        .insert_resource(sites_handle)
        .insert_resource(sites_metadata)
        .insert_resource(sites::DiscoveredSites::default())
        .insert_resource(expedition_handle)
        .insert_resource(expedition_metadata)
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
        .insert_resource(FogRevealLedger::default())
        .insert_resource(CommandEventLog::default())
        .insert_resource(FoodSiteRegistry::default())
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
                TurnStage::Finalize,
                TurnStage::Victory,
                TurnStage::Snapshot,
            )
                .chain(),
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
                sites::place_wondrous_sites,
                spawn_initial_herds,
                spawn_initial_forage,
                espionage::initialise_espionage_roster,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                tick_influencers,
                reconcile_culture_layers,
                systems::process_culture_events,
            )
                .chain()
                .in_set(TurnStage::Influence)
                .run_if(capability_enabled(CapabilityFlags::ALWAYS_ON)),
        )
        .add_systems(
            Update,
            (
                systems::simulate_materials,
                systems::simulate_logistics,
                advance_herds,
                advance_forage_regrowth,
                advance_cultivation,
                repopulate_fauna,
                advance_husbandry,
                supply::balance_supply_networks,
                systems::trade_knowledge_diffusion,
            )
                .chain()
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
                .chain()
                .in_set(TurnStage::Knowledge)
                .run_if(capability_enabled(
                    CapabilityFlags::ESPIONAGE_T2 | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                great_discovery::collect_observation_signals,
                great_discovery::update_constellation_progress,
                great_discovery::screen_great_discovery_candidates,
                great_discovery::resolve_great_discovery,
                great_discovery::propagate_diffusion_impacts,
                great_discovery::export_great_discovery_metrics,
                great_discovery::apply_capability_effects,
            )
                .chain()
                .in_set(TurnStage::GreatDiscovery)
                .run_if(capability_enabled(
                    CapabilityFlags::MEGAPROJECTS | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
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
                // Wellbeing migration runs after demographics + this turn's yield payouts so
                // morale/discontent are current and productivity has already been applied at each
                // yield site; it then relocates discontented people (population conserved).
                systems::advance_population_migration,
                sedentarization::sedentarization_tick,
                systems::publish_trade_telemetry,
            )
                .chain()
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
                visibility_systems::clear_active_visibility,
                visibility_systems::prune_sweep_tracker,
                visibility_systems::calculate_visibility,
                visibility_systems::apply_trade_route_visibility,
                visibility_systems::apply_visibility_decay,
                sites::discover_sites,
            )
                .chain()
                .in_set(TurnStage::Visibility)
                .run_if(capability_enabled(CapabilityFlags::ALWAYS_ON)),
        )
        .add_systems(
            Update,
            crisis::advance_crisis_system.in_set(TurnStage::Crisis),
        )
        .add_systems(
            Update,
            (
                systems::simulate_power,
                systems::process_corruption,
                systems::decay_fog_reveals,
            )
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
                snapshot::capture_snapshot,
            )
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

    app
}

/// Execute a single simulation turn.
///
/// Each call processes the chained systems configured in [`build_headless_app`]
/// (materials → logistics → population → power → tick increment → snapshot).
/// Callers are responsible for snapshot broadcasting and command handling.
pub fn run_turn(app: &mut App) {
    app.update();
}

fn capability_enabled(flags: CapabilityFlags) -> impl FnMut(Res<CapabilityFlags>) -> bool {
    move |current: Res<CapabilityFlags>| current.intersects(flags)
}
