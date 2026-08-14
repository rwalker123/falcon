//! Rollback/load must NOT destroy tended improvements.
//!
//! Two transient within-turn "worked this improvement last turn" flags —
//! `ForagePatch::tended_this_turn` and `Herd::corralled_tended_this_turn` — are the one-turn-lag
//! signals the Logistics decay pass reads to spare a source a band is working
//! (`forage::advance_cultivation`, `fauna::advance_husbandry`).
//!
//! The failure this guards: on the very first Logistics pass after a restore — which runs *before*
//! the Population labor arm can re-mark them — a source whose flag came back `false` would have a
//! tended patch / Field decay one tick (`is_managed()` flips false, the improvement lost even with a
//! band working it every turn) and a corralled pen **escape outright** (`corralled_at = None`,
//! `pen_radius = 0`, throwing away the whole rebuild plus every ExtendPen ring).
//!
//! The test deliberately pins the **behaviour**, not the mechanism, so it survives the mechanism
//! changing — as it has: a pair of `*_from_state` constructors used to reconstruct each source from
//! a serde record and reseeded the flags, and `restore_sim_state` now clones the registries whole
//! out of the checkpoint instead, carrying the real flags across.
//!
//! It goes through a REAL round-trip — a live world captured by the shipped capture path, restored
//! by `restore_sim_state`, then advanced exactly one turn — not a hand-built rollback state.

use bevy::math::UVec2;

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    available_workers, build_headless_app, run_turn, FactionId, ForageRegistry, HerdRegistry,
    LaborAllocation, LaborTarget, PopulationCohort, ResidentBand, SimulationConfig,
};

/// Build a headless world (one `update()` runs the whole Startup worldgen chain — including
/// `spawn_initial_herds` / `spawn_initial_forage` — and resolves turn 1), pinned to a deterministic
/// earthlike map so the registries are populated the same way every run.
fn spawn_world() -> bevy::app::App {
    let mut app = build_headless_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The first resident band's faction + its available worker count.
fn resident_band(app: &mut bevy::app::App) -> (FactionId, u32) {
    let mut query = app
        .world
        .query_filtered::<&PopulationCohort, bevy::prelude::With<ResidentBand>>();
    let cohort = query
        .iter(&app.world)
        .next()
        .expect("the campaign spawns at least one resident band");
    (cohort.faction, available_workers(cohort.working))
}

#[test]
fn a_snapshot_round_trip_keeps_a_worked_field_and_pen() {
    let mut app = spawn_world();
    let (faction, available) = resident_band(&mut app);

    // --- Set up a completed, worked Field on a real forage patch. ------------------------------
    // A Field (rung 3): both improvement meters paid off, so `is_managed()` (and `is_field()`) holds.
    // The meters are in WORK UNITS now, so a completed rung reads its own cost rather than `1.0`.
    let field_tile: UVec2 = {
        let mut forage = app.world.resource_mut::<ForageRegistry>();
        let patch = forage
            .patches
            .values_mut()
            .next()
            .expect("worldgen seeds forage patches");
        patch.complete_cultivation(faction);
        patch.complete_field(faction);
        patch.owner = Some(faction);
        patch.biomass = patch.carrying_capacity;
        patch.tile
    };
    // As a band whose KEEPERS held it this turn would have left it — the one-turn-lag signal the
    // restore must carry (`docs/plan_standing_upkeep.md` §2.4; it was the retired `tended_this_turn`
    // flag before the upkeep replaced it). Stamped through the shipped seam so a retune of either
    // plant rung's demand cannot leave this fixture supplying too little.
    {
        let ladder = core_sim::LadderConfig::builtin();
        let mut forage = app.world.resource_mut::<ForageRegistry>();
        let patch = forage.patch_mut(field_tile).expect("the Field persists");
        patch.upkeep_supplied = core_sim::patch_upkeep_demand(patch, &ladder);
    }

    // --- Set up a completed, worked corral (pen) on a real herd. --------------------------------
    // Domesticated + corralled at its tile with a grown (radius-1) fence.
    let (herd_id, pen_tile) = {
        let mut herds = app.world.resource_mut::<HerdRegistry>();
        let index = herds
            .herds
            .iter()
            .position(|h| h.id.starts_with("game_"))
            .or(if herds.herds.is_empty() {
                None
            } else {
                Some(0)
            })
            .expect("worldgen seeds herds");
        let herd = &mut herds.herds[index];
        let tile = herd.current_pos;
        // **Written straight onto the herd, gates and all** — worldgen picks whichever species is
        // first on the map and it may be `wild`-ceiling, while what is under test is that the
        // *checkpoint* carries a finished rung. A completed meter is `progress >= cost > 0`, so both
        // halves are set: the fabricated one-worker-turn job (`FABRICATED_BUILD_COST`) says "this
        // rung is paid for" without pretending to the ladder's own price.
        herd.domestication_progress = core_sim::FABRICATED_BUILD_COST;
        herd.domestication_cost = core_sim::FABRICATED_BUILD_COST;
        herd.owner = Some(faction);
        herd.corralled_at = Some(tile);
        herd.pen_radius = 1;
        herd.corral_progress = core_sim::FABRICATED_BUILD_COST;
        herd.corral_cost = core_sim::FABRICATED_BUILD_COST;
        herd.biomass = herd.carrying_capacity;
        // The one-turn "keeper tended it" grace the restore drops:
        herd.corralled_tended_this_turn = true;
        (herd.id.clone(), tile)
    };

    // Keep the band assigned to work BOTH sources — the bug destroys them anyway, because the
    // restored turn's Logistics decay runs before the Population labor arm can re-mark them.
    {
        let mut query = app
            .world
            .query_filtered::<&mut LaborAllocation, bevy::prelude::With<ResidentBand>>();
        if let Some(mut alloc) = query.iter_mut(&mut app.world).next() {
            alloc.set_assignment(
                LaborTarget::Forage {
                    tile: field_tile,
                    floor: 0.5,
                    species: None,
                },
                1,
                available,
                None,
            );
            alloc.set_assignment(
                LaborTarget::Hunt {
                    fauna_id: herd_id.clone(),
                    floor: 0.5,
                },
                1,
                available,
                None,
            );
        }
    }

    // --- Capture a REAL published snapshot of this world, then restore it. ----------------------
    let checkpoint = capture_sim_state(&app.world);
    restore_sim_state(&mut app.world, &checkpoint);

    // The durable state survives the round-trip (sanity — the improvement is intact right after
    // restore, so any loss below is the post-restore turn, not the capture).
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(field_tile)
            .expect("patch restored")
            .is_managed(),
        "the Field should still be managed immediately after restore"
    );
    assert_eq!(
        app.world
            .resource::<HerdRegistry>()
            .find(&herd_id)
            .expect("herd restored")
            .corralled_at,
        Some(pen_tile),
        "the pen should still be corralled immediately after restore"
    );

    // --- Advance exactly one turn: the post-restore Logistics pass. ------------------------------
    run_turn(&mut app);

    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(field_tile)
        .expect("the patch never despawns")
        .clone();
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(&herd_id)
        .expect("a corralled herd is retained")
        .clone();

    // The improvement must survive one post-restore turn while a band works it.
    assert!(
        patch.is_managed(),
        "the worked Field was destroyed by the restore: is_managed() = false \
         (cultivation_progress = {}, field_progress = {})",
        patch.cultivation_progress,
        patch.field_progress
    );
    assert_eq!(
        herd.corralled_at,
        Some(pen_tile),
        "the pen escaped on the post-restore turn: corralled_at = {:?}, pen_radius = {}, \
         corral_progress = {}",
        herd.corralled_at,
        herd.pen_radius,
        herd.corral_progress
    );
    assert_eq!(
        herd.pen_radius, 1,
        "the pen's ExtendPen radius was thrown away on the post-restore turn"
    );
}
