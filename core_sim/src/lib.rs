//! Core simulation crate for the Shadow-Scale headless prototype.
//!
//! Provides deterministic ECS systems that resolve a single turn of the
//! simulation when [`run_turn`] is invoked.

mod components;
mod culture;
mod generations;
mod great_discovery;
mod influencers;
mod knowledge_ledger;
pub mod log_stream;
pub mod metrics;
pub mod network;
mod orders;
mod power;
mod resources;
mod scalar;
mod snapshot;
mod systems;
mod terrain;

use bevy::prelude::*;

pub use components::{
    ElementKind, KnowledgeFragment, LogisticsLink, PendingMigration, PopulationCohort, PowerNode,
    Tile, TradeLink,
};
pub use culture::{
    reconcile_culture_layers, CultureEffectsCache, CultureLayer, CultureLayerId, CultureLayerScope,
    CultureManager, CultureOwner, CultureSchismEvent, CultureTensionEvent, CultureTensionKind,
    CultureTensionRecord, CultureTraitAxis, CultureTraitVector, CULTURE_TRAIT_AXES,
};
pub use generations::{GenerationBias, GenerationId, GenerationProfile, GenerationRegistry};
pub use great_discovery::{
    ConstellationRequirement, GreatDiscoveryCandidateEvent, GreatDiscoveryDefinition,
    GreatDiscoveryEffectEvent, GreatDiscoveryEffectKind, GreatDiscoveryFlag, GreatDiscoveryId,
    GreatDiscoveryLedger, GreatDiscoveryReadiness, GreatDiscoveryRegistry,
    GreatDiscoveryResolvedEvent, GreatDiscoveryTelemetry, ObservationLedger,
};
pub use influencers::{
    tick_influencers, InfluencerCultureResonance, InfluencerImpacts, InfluentialId,
    InfluentialRoster, SupportChannel,
};
pub use knowledge_ledger::{
    CounterIntelSweepEvent, EspionageProbeEvent, KnowledgeCountermeasure, KnowledgeLedger,
    KnowledgeLedgerEntry, KnowledgeModifier, KnowledgeTimelineEvent,
};

pub use metrics::SimulationMetrics;
pub use orders::{
    FactionId, FactionOrders, FactionRegistry, Order, SubmitError, SubmitOutcome, TurnQueue,
};
pub use power::{
    PowerDiscoveryEffects, PowerGridNodeTelemetry, PowerGridState, PowerIncident,
    PowerIncidentSeverity, PowerNodeId, PowerTopology,
};
pub use resources::{
    CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage, DiscoveryProgressLedger,
    PendingCrisisSeeds, SentimentAxisBias, SimulationConfig, SimulationTick, TileRegistry,
    TradeDiffusionRecord, TradeTelemetry,
};
pub use scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar};
pub use snapshot::{restore_world_from_snapshot, SnapshotHistory, StoredSnapshot};
pub use systems::{MigrationKnowledgeEvent, TradeDiffusionEvent};
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
    Finalize,
    Snapshot,
}

/// Construct a Bevy [`App`] configured with the Shadow-Scale turn pipeline.
pub fn build_headless_app() -> App {
    let mut app = App::new();

    let config = SimulationConfig::default();
    let faction_registry = orders::FactionRegistry::default();
    let turn_queue = orders::TurnQueue::new(faction_registry.factions.clone());
    let snapshot_history = SnapshotHistory::with_capacity(config.snapshot_history_limit.max(1));
    let generation_registry = GenerationRegistry::with_seed(0xC0FEBABE, 6);
    let influencer_roster = InfluentialRoster::with_seed(0xA51C_E55E, &generation_registry);
    let culture_manager = CultureManager::new();
    let culture_effects = CultureEffectsCache::default();

    app.insert_resource(config)
        .insert_resource(PowerGridState::default())
        .insert_resource(PowerTopology::default())
        .insert_resource(SimulationTick::default())
        .insert_resource(SimulationMetrics::default())
        .insert_resource(SentimentAxisBias::default())
        .insert_resource(KnowledgeLedger::default())
        .insert_resource(CorruptionLedgers::default())
        .insert_resource(CorruptionTelemetry::default())
        .insert_resource(DiplomacyLeverage::default())
        .insert_resource(snapshot_history)
        .insert_resource(generation_registry)
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
                TurnStage::Finalize,
                TurnStage::Snapshot,
            )
                .chain(),
        )
        .add_systems(Startup, systems::spawn_initial_world)
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
                systems::trade_knowledge_diffusion,
            )
                .chain()
                .in_set(TurnStage::Logistics),
        )
        .add_systems(
            Update,
            (
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
                systems::publish_trade_telemetry,
            )
                .chain()
                .in_set(TurnStage::Population),
        )
        .add_systems(
            Update,
            (systems::simulate_power, systems::process_corruption)
                .chain()
                .in_set(TurnStage::Finalize),
        )
        .add_systems(
            Update,
            (systems::advance_tick, snapshot::capture_snapshot)
                .chain()
                .in_set(TurnStage::Snapshot),
        );

    {
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
