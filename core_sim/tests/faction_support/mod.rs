//! **The repo's two-faction world**, and the one-faction control arm it is measured against.
//!
//! Every claim about per-faction placement is a *comparison*: the second faction must get its own
//! ground, its own opening band and its own opening budgets, and the first faction must get exactly
//! what a one-faction world has always given it. A fixture that only built the two-faction arm could
//! not state the second half, so both arms live here and are built the same way.
//!
//! **The roster is installed before the first `update()`**, which is when `Startup` — and therefore
//! `spawn_initial_world` — runs. `build_headless_app` seeds the registry from the active start
//! profile, and the shipped profile declares one human faction; overwriting the resource before the
//! world is generated is what makes worldgen place two, without editing a shipped config file or
//! setting a process-global env var that a parallel test would race on.

// Compiled whole into each suite that uses it, and each uses the subset it needs — same idiom and
// rationale as `telling_support/mod.rs`.
#![allow(dead_code)]

use bevy::prelude::*;

use core_sim::{
    build_test_app, FactionControl, FactionId, FactionRegistry, FactionSpec, SimulationConfig,
    TurnQueue,
};

/// The human faction every world has — `FactionId(0)`, the profile's first entry.
pub const HOME: FactionId = FactionId(0);

/// The second people, present only in the two-faction arm.
pub const RIVAL: FactionId = FactionId(1);

/// One human faction: what `start_profiles.json` ships.
pub fn human_only() -> Vec<FactionSpec> {
    vec![FactionSpec {
        control: FactionControl::Human,
    }]
}

/// Two peoples on one map — a human and an AI, the roster `factions.md` documents.
pub fn human_and_ai() -> Vec<FactionSpec> {
    vec![
        FactionSpec {
            control: FactionControl::Human,
        },
        FactionSpec {
            control: FactionControl::Ai,
        },
    ]
}

/// A world generated for exactly the roster in `factions`, with `tune` applied to the config first.
///
/// Both edits land **before** the first `update()`, because worldgen reads the roster and the grid
/// at `Startup`. The `TurnQueue` is rebuilt from that roster too, for the reason `factions.md`
/// gives: the await set is derived from the registry, and re-seeding one without the other is the
/// roster-drift defect that rule file is about.
pub fn world_with(factions: &[FactionSpec], tune: impl FnOnce(&mut SimulationConfig)) -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    tune(&mut config);
    app.world.insert_resource(config);
    let registry = FactionRegistry::new(factions);
    app.world
        .insert_resource(TurnQueue::new(registry.factions().to_vec()));
    app.world.insert_resource(registry);
    app.update();
    app
}

/// The control arm.
pub fn one_faction_world() -> App {
    world_with(&human_only(), |_| {})
}

/// The subject.
pub fn two_faction_world() -> App {
    world_with(&human_and_ai(), |_| {})
}
