//! Core simulation crate for the Shadow-Scale headless prototype.
//!
//! Provides deterministic ECS systems that resolve a single turn of the
//! simulation when [`run_turn`] is invoked.

mod components;
pub mod metrics;
pub mod network;
mod orders;
mod resources;
mod scalar;
mod snapshot;
mod systems;

use bevy::prelude::*;

pub use components::{ElementKind, LogisticsLink, PopulationCohort, PowerNode, Tile};

pub use metrics::SimulationMetrics;
pub use orders::{
    FactionId, FactionOrders, FactionRegistry, Order, SubmitError, SubmitOutcome, TurnQueue,
};
pub use resources::{SimulationConfig, SimulationTick, TileRegistry};
pub use scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar};
pub use snapshot::{restore_world_from_snapshot, SnapshotHistory, StoredSnapshot};

/// Construct a Bevy [`App`] configured with the Shadow-Scale turn pipeline.
pub fn build_headless_app() -> App {
    let mut app = App::new();

    let config = SimulationConfig::default();
    let faction_registry = orders::FactionRegistry::default();
    let turn_queue = orders::TurnQueue::new(faction_registry.factions.clone());
    let snapshot_history = SnapshotHistory::with_capacity(config.snapshot_history_limit.max(1));

    app.insert_resource(config)
        .insert_resource(SimulationTick::default())
        .insert_resource(snapshot_history)
        .insert_resource(faction_registry)
        .insert_resource(turn_queue)
        .add_plugins(MinimalPlugins)
        .add_systems(Startup, systems::spawn_initial_world)
        .add_systems(
            Update,
            (
                systems::simulate_materials,
                systems::simulate_logistics,
                systems::simulate_population,
                systems::simulate_power,
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
