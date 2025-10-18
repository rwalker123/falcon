//! Core simulation crate for Shadow-Scale prototype.
//!
//! Provides the Bevy ECS app, deterministic schedules, and serialization hooks
//! for streaming state to external clients. The initial prototype runs in
//! headless mode and exposes a simplified material/logistics simulation.

use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, prelude::*};

/// Entry point to create the headless simulation app configured for a
/// deterministic fixed-step loop.
pub fn build_headless_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(
        Duration::from_secs_f64(1.0 / 60.0),
    )))
        .add_systems(Update, advance_simulation);
    app
}

fn advance_simulation(_world: &mut World) {
    // placeholder for ECS system schedule wiring.
}
