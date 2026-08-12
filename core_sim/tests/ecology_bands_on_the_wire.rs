//! **The published ecology phase BANDS bracket the published phase WORD** — on both food webs.
//!
//! `ecologyPhase` ships *which* band a source is in; `collapseFraction`/`stressedFraction` ship
//! *where the bands are*, as fractions of `carryingCapacity` — the same units the harvest floor is
//! in, which is what lets a client draw them as the zones the floor line is dragged against
//! (`docs/plan_harvest_floor.md` §7.3).
//!
//! Two numbers describing one classification can drift, and the drift is silent: a stale band still
//! renders as a plausible zone. So this pins them **against each other** rather than each against a
//! literal — seat a source at a chosen `B/K`, read the triple off the **shipped snapshot**, and
//! assert the word is the one the published cuts imply. A rung's ecology can then be retuned freely
//! and this still holds; only a genuine disagreement fails it.
//!
//! **The herd side is the reason the fields are per-source at all.** `fauna::herd_ecology` resolves
//! wild / pastoral / pen, and the managed blocks carry their own cut points, so a single global pair
//! would be right for plants and wrong for a tamed or penned herd.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::{
    advance_forage_regrowth, FactionId, FaunaConfigHandle, ForageRegistry, HerdRegistry,
    HerdTelemetry, SimulationConfig, SnapshotHistory,
};

/// The map every fixture here stands on — the standard seed the rest of the suite quotes against.
const STANDARD_SEED: u64 = 119_304_647;

/// **The stocks each source is seated at, as fractions of its own `K`.** Chosen to straddle every
/// shipped cut point with room to spare rather than to sit on one: a boundary case would be
/// asserting `<` versus `<=`, which `fauna::classify_ecology_phase` owns and its own unit tests
/// pin. What is under test here is that the wire's two halves agree, at every band.
const SWEPT_STOCK_FRACTIONS: [f32; 6] = [0.02, 0.12, 0.28, 0.45, 0.75, 1.0];

fn headless_app() -> App {
    let mut app = core_sim::build_headless_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_seed = STANDARD_SEED;
    app.world.insert_resource(config);
    app.update(); // the real Startup chain: worldgen, source seeding, one capture.
    app
}

fn recapture(app: &mut App) -> sim_runtime::WorldSnapshot {
    app.world
        .run_system_once(core_sim::recapture_snapshot_in_place);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .expect("a capture after the update");
    (*snapshot).clone()
}

/// **The one assertion this file exists for**: the published word is the one the published cuts
/// imply for the published stock. Stated once so the two webs cannot be checked differently.
fn assert_bands_bracket_the_phase(
    web: &str,
    biomass: f32,
    carrying_capacity: f32,
    collapse_fraction: f32,
    stressed_fraction: f32,
    phase: &str,
) {
    assert!(
        carrying_capacity > 0.0,
        "{web}: the fixture source must have a capacity, or the fractions mean nothing"
    );
    assert!(
        collapse_fraction > 0.0 && collapse_fraction < stressed_fraction && stressed_fraction < 1.0,
        "{web}: the bands must be a real ordered pair inside (0, 1): collapse \
         {collapse_fraction}, stressed {stressed_fraction}"
    );

    let fraction = biomass / carrying_capacity;
    let implied = if fraction < collapse_fraction {
        "collapsing"
    } else if fraction < stressed_fraction {
        "stressed"
    } else {
        "thriving"
    };
    assert_eq!(
        phase, implied,
        "{web}: a source at {fraction} of K, with published cuts {collapse_fraction} / \
         {stressed_fraction}, must publish `{implied}` — it published `{phase}`"
    );
}

/// A patch's bands and its phase agree at every band, on the shipped wire.
#[test]
fn a_patchs_published_bands_bracket_its_published_phase() {
    let mut app = headless_app();
    let coord = a_seeded_patch(&app);
    let mut saw = std::collections::BTreeSet::new();

    for fraction in SWEPT_STOCK_FRACTIONS {
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).expect("the fixture patch");
            patch.biomass = patch.carrying_capacity * fraction;
        }
        // **The word is written by the SIM, not by the fixture.** `advance_forage_regrowth` is the
        // Logistics pass that classifies the phase off the patch's own ecology — the same seam the
        // published bands are read through — so the assertion below is about the export agreeing
        // with itself, never about a phase the test wrote. It also moves the biomass, which is fine
        // and is the point: the row publishes the post-regrowth stock the word was cut from.
        app.world.run_system_once(advance_forage_regrowth);
        let snapshot = recapture(&mut app);
        let row = snapshot
            .forage_patches
            .iter()
            .find(|patch| patch.x == coord.x && patch.y == coord.y)
            .expect("the fixture patch is on the wire");
        assert_bands_bracket_the_phase(
            "plant",
            row.biomass,
            row.carrying_capacity,
            row.collapse_fraction,
            row.stressed_fraction,
            &row.ecology_phase,
        );
        saw.insert(row.ecology_phase.clone());
    }

    assert_eq!(
        saw.len(),
        3,
        "the sweep must reach all three bands or it asserts nothing about the cuts: {saw:?}"
    );
}

/// The animal twin — **and the case that justifies the fields being per-source**: a herd's cuts come
/// from the rung it stands on, so the same herd publishes a different pair once it is tamed.
#[test]
fn a_herds_published_bands_bracket_its_published_phase_at_every_rung() {
    let mut app = headless_app();
    let id = a_seeded_herd(&app);
    let mut saw = std::collections::BTreeSet::new();
    let mut wild_bands = None;
    let mut pastoral_bands = None;

    for tamed in [false, true] {
        if tamed {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry
                .herds
                .iter_mut()
                .find(|herd| herd.id == id)
                .expect("the fixture herd");
            herd.tame_outright(FactionId(0));
            assert!(herd.is_domesticated(), "the herd stands on rung 2");
        }
        for fraction in SWEPT_STOCK_FRACTIONS {
            {
                // `Herd::refresh_ecology_phase` is the sim's own classifier and takes the whole
                // config, resolving the herd's RUNG ecology internally — the same seam the published
                // bands come from, which is what makes this an export check rather than a restatement.
                let fauna = app.world.resource::<FaunaConfigHandle>().get();
                let mut registry = app.world.resource_mut::<HerdRegistry>();
                let herd = registry
                    .herds
                    .iter_mut()
                    .find(|herd| herd.id == id)
                    .expect("the fixture herd");
                herd.biomass = herd.carrying_capacity * fraction;
                herd.refresh_ecology_phase(&fauna);
            }
            // **The display list is what the snapshot publishes, not the registry** — so refresh it
            // the way `advance_herds` ends its own pass, through `HerdRegistry::snapshot_entries`.
            // Driving the whole Logistics system instead would regrow (and, in the collapsing band,
            // eventually despawn) the herd, which is a different test.
            let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
            app.world.resource_mut::<HerdTelemetry>().entries = entries;
            reveal_herd(&mut app, &id);
            let snapshot = recapture(&mut app);
            let row = snapshot
                .herds
                .iter()
                .find(|herd| herd.id == id)
                .expect("the fixture herd is on the wire");
            assert_bands_bracket_the_phase(
                if tamed {
                    "animal (pastoral)"
                } else {
                    "animal (wild)"
                },
                row.biomass,
                row.carrying_capacity,
                row.collapse_fraction,
                row.stressed_fraction,
                &row.ecology_phase,
            );
            saw.insert(row.ecology_phase.clone());
            let bands = (row.collapse_fraction, row.stressed_fraction);
            if tamed {
                pastoral_bands = Some(bands);
            } else {
                wild_bands = Some(bands);
            }
        }
    }

    assert_eq!(
        saw.len(),
        3,
        "the sweep must reach all three bands or it asserts nothing about the cuts: {saw:?}"
    );
    // The rung-awareness itself. The shipped `pastoral` block may or may not currently differ from
    // the wild one — that is a tuning question — so this asserts only that BOTH were published as a
    // real ordered pair, which `assert_bands_bracket_the_phase` already required of each. Naming the
    // two here is what makes a future retune of one block visible as a diff in this test rather than
    // as a silently wrong background on the chart.
    let wild = wild_bands.expect("a wild reading was taken");
    let pastoral = pastoral_bands.expect("a pastoral reading was taken");
    assert!(
        wild.0 > 0.0 && pastoral.0 > 0.0,
        "each rung publishes its OWN cuts, read through `herd_ecology`: wild {wild:?}, pastoral \
         {pastoral:?}"
    );
}

/// The richest in-season patch on the standard map — the same "biggest `K`, deterministic
/// tie-break" pick the other forage fixtures make.
fn a_seeded_patch(app: &App) -> UVec2 {
    app.world
        .resource::<ForageRegistry>()
        .patches
        .values()
        .filter(|patch| patch.carrying_capacity > 0.0)
        .max_by(|a, b| {
            a.carrying_capacity
                .total_cmp(&b.carrying_capacity)
                .then_with(|| b.tile.y.cmp(&a.tile.y))
                .then_with(|| b.tile.x.cmp(&a.tile.x))
        })
        .map(|patch| patch.tile)
        .expect("the standard map seeds forage patches")
}

/// **Mark the herd's tile actively visible to the viewer**, so it reaches the wire at all — herd
/// telemetry is fog-filtered, and an unseen herd is correctly absent rather than published with
/// stale bands.
fn reveal_herd(app: &mut App, id: &str) {
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|herd| herd.position())
        .expect("the fixture herd is in the registry");
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    let mut ledger = app.world.resource_mut::<core_sim::VisibilityLedger>();
    ledger
        .ensure_faction(viewer, grid.x, grid.y)
        .mark_active(pos.x, pos.y, 0);
}

/// A stationary game herd the ladder can actually climb — **`can_domesticate()` is not decoration**:
/// the rung half of this test needs a herd whose species' `husbandry_ceiling` reaches `pastoral`, and
/// a `wild`-ceiling species (deer, mammoth) never leaves rung 1 however much progress is credited.
fn a_seeded_herd(app: &App) -> String {
    app.world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|herd| {
            herd.id.starts_with("game_") && herd.carrying_capacity > 0.0 && herd.can_domesticate()
        })
        .map(|herd| herd.id.clone())
        .expect("the standard map seeds a tameable game herd")
}
