//! Core simulation crate for the Shadow-Scale headless prototype.
//!
//! Provides deterministic ECS systems that resolve a single turn of the
//! simulation when [`run_turn`] is invoked.

mod components;
mod culture;
mod generations;
mod influencers;
pub mod log_stream;
pub mod metrics;
pub mod network;
mod orders;
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
pub use influencers::{
    tick_influencers, InfluencerCultureResonance, InfluencerImpacts, InfluentialId,
    InfluentialRoster, SupportChannel,
};

pub use metrics::SimulationMetrics;
pub use orders::{
    FactionId, FactionOrders, FactionRegistry, Order, SubmitError, SubmitOutcome, TurnQueue,
};
pub use resources::{
    CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage, DiscoveryProgressLedger,
    SentimentAxisBias, SimulationConfig, SimulationTick, TileRegistry, TradeDiffusionRecord,
    TradeTelemetry,
};
pub use scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar};
pub use snapshot::{restore_world_from_snapshot, SnapshotHistory, StoredSnapshot};
pub use systems::{MigrationKnowledgeEvent, TradeDiffusionEvent};
pub use terrain::{
    classify_terrain, terrain_definition, terrain_for_position, MovementProfile, TerrainDefinition,
    TerrainResourceBias,
};

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
        .insert_resource(SimulationTick::default())
        .insert_resource(SentimentAxisBias::default())
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
        .insert_resource(faction_registry)
        .insert_resource(turn_queue)
        .add_event::<CultureTensionEvent>()
        .add_event::<CultureSchismEvent>()
        .add_event::<systems::TradeDiffusionEvent>()
        .add_event::<systems::MigrationKnowledgeEvent>()
        .add_plugins(MinimalPlugins)
        .add_systems(Startup, systems::spawn_initial_world)
        .add_systems(
            Update,
            (
                tick_influencers,
                reconcile_culture_layers,
                systems::process_culture_events,
                systems::simulate_materials,
                systems::simulate_logistics,
                systems::trade_knowledge_diffusion,
                systems::simulate_population,
                systems::publish_trade_telemetry,
                systems::simulate_power,
                systems::process_corruption,
                systems::advance_tick,
                snapshot::capture_snapshot,
            )
                .chain(),
        );

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
