//! Core simulation crate for the Shadow-Scale headless prototype.
//!
//! Provides deterministic ECS systems that resolve a single turn of the
//! simulation when [`run_turn`] is invoked.

mod components;
mod crisis;
mod crisis_config;
mod culture;
mod culture_corruption_config;
mod espionage;
mod fauna;
mod food;
mod generations;
mod great_discovery;
pub mod hashing;
mod heightfield;
mod hydrology;
mod influencers;
mod knowledge_ledger;
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
mod snapshot;
mod snapshot_overlays_config;
mod start_profile;
mod systems;
mod terrain;
mod turn_pipeline_config;
mod victory;

use std::sync::Arc;

use crate::map_preset::load_map_presets_from_env;
use crate::start_profile::{
    load_start_profile_knowledge_tags_from_env, load_start_profiles_from_env,
};
use bevy::prelude::*;

pub use components::{
    ElementKind, HarvestAssignment, HarvestTaskKind, KnowledgeFragment, LogisticsLink,
    PendingMigration, PopulationCohort, PowerNode, ScoutAssignment, Settlement, StartingUnit, Tile,
    TownCenter, TradeLink,
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
pub use espionage::{
    AgentAssignment, CounterIntelBudgets, EspionageAgentHandle, EspionageCatalog,
    EspionageMissionId, EspionageMissionInstanceId, EspionageMissionKind, EspionageMissionState,
    EspionageMissionTemplate, EspionageRoster, FactionSecurityPolicies, QueueMissionError,
    QueueMissionParams, SecurityPolicy,
};
pub use fauna::{
    advance_herds, spawn_initial_herds, HerdDensityMap, HerdRegistry, HerdTelemetry,
    HerdTelemetryEntry,
};
pub use food::{
    classify_food_module, classify_food_module_from_traits, FoodModule, FoodModuleTag,
    FoodSiteKind, DEFAULT_HARVEST_TRAVEL_TILES_PER_TURN, DEFAULT_HARVEST_WORK_TURNS,
};
pub use generations::{GenerationBias, GenerationId, GenerationProfile, GenerationRegistry};
pub use great_discovery::{
    ConstellationRequirement, GreatDiscoveryCandidateEvent, GreatDiscoveryDefinition,
    GreatDiscoveryEffectEvent, GreatDiscoveryEffectKind, GreatDiscoveryFlag, GreatDiscoveryId,
    GreatDiscoveryLedger, GreatDiscoveryReadiness, GreatDiscoveryRegistry,
    GreatDiscoveryResolvedEvent, GreatDiscoveryTelemetry, ObservationLedger,
};
pub use hydrology::{generate_hydrology, HydrologyState};
pub use influencers::{
    tick_influencers, InfluencerBalanceConfig, InfluencerConfigHandle, InfluencerCultureResonance,
    InfluencerImpacts, InfluentialId, InfluentialRoster, SupportChannel, BUILTIN_INFLUENCER_CONFIG,
};
pub use knowledge_ledger::{
    CounterIntelSweepEvent, EspionageProbeEvent, KnowledgeCountermeasure, KnowledgeLedger,
    KnowledgeLedgerConfig, KnowledgeLedgerConfigHandle, KnowledgeLedgerEntry, KnowledgeModifier,
    KnowledgeTimelineEvent, BUILTIN_KNOWLEDGE_LEDGER_CONFIG,
};
pub use map_preset::{MapPreset, MapPresets, MapPresetsHandle};
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
pub use turn_pipeline_config::{
    load_turn_pipeline_config_from_env, LogisticsPhaseConfig, PopulationPhaseConfig,
    PowerPhaseConfig, TradePhaseConfig, TurnPipelineConfig, TurnPipelineConfigHandle,
    TurnPipelineConfigMetadata, BUILTIN_TURN_PIPELINE_CONFIG,
};
pub use victory::{
    load_victory_config_from_env, VictoryConfigHandle, VictoryModeId, VictoryModeKind,
    VictoryModeState, VictoryState,
};

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
    CommandEventEntry, CommandEventKind, CommandEventLog, CorruptionLedgers, CorruptionTelemetry,
    DiplomacyLeverage, DiscoveryProgressLedger, FactionInventory, FogRevealLedger, FoodSiteEntry,
    FoodSiteRegistry, HydrologyOverrides, PendingCrisisSeeds, PendingCrisisSpawns,
    SentimentAxisBias, SimulationConfig, SimulationConfigMetadata, SimulationTick, StartLocation,
    TileRegistry, TradeDiffusionRecord, TradeTelemetry,
};
pub use scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar};
pub use snapshot::{
    command_events_to_state, restore_world_from_snapshot, SnapshotHistory, StoredSnapshot,
};
pub use systems::spawn_initial_world;
pub use systems::{simulate_power, MigrationKnowledgeEvent, PowerSimParams, TradeDiffusionEvent};
pub use terrain::{
    classify_terrain, terrain_definition, terrain_for_position, MovementProfile, TerrainDefinition,
    TerrainResourceBias,
};

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum TurnStage {
    Influence,
    Logistics,
    Knowledge,
    GreatDiscovery,
    Population,
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
        .insert_resource(FogRevealLedger::default())
        .insert_resource(CommandEventLog::default())
        .insert_resource(FoodSiteRegistry::default())
        .insert_resource(snapshot_history)
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
                spawn_initial_herds,
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
                .in_set(TurnStage::Influence),
        )
        .add_systems(
            Update,
            (
                systems::simulate_materials,
                systems::simulate_logistics,
                advance_herds,
                systems::trade_knowledge_diffusion,
            )
                .chain()
                .in_set(TurnStage::Logistics),
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
                .in_set(TurnStage::Knowledge),
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
            )
                .chain()
                .in_set(TurnStage::GreatDiscovery),
        )
        .add_systems(
            Update,
            (
                systems::simulate_population,
                systems::advance_harvest_assignments,
                systems::advance_scout_assignments,
                systems::publish_trade_telemetry,
            )
                .chain()
                .in_set(TurnStage::Population),
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
                .in_set(TurnStage::Finalize),
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
