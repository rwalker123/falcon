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
use core_sim::TakeSelection;
use core_sim::{
    available_workers, build_test_app, run_turn, FactionId, ForageRegistry, HerdRegistry,
    LaborAllocation, LaborTarget, PopulationCohort, ResidentBand, SimulationConfig,
};

/// Build a headless world (one `update()` runs the whole Startup worldgen chain — including
/// `spawn_initial_herds` / `spawn_initial_forage` — and resolves turn 1), pinned to a deterministic
/// earthlike map so the registries are populated the same way every run.
fn spawn_world() -> bevy::app::App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
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
        patch.complete_cultivation(faction, &core_sim::LadderConfig::builtin());
        patch.complete_field(faction, &core_sim::LadderConfig::builtin());
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
        // **The bill is quoted per tender-load of this ground** (`forage::patch_tender_loads`), so
        // the fixture resolves the Field's own tile rather than handing the seam a bare load — a
        // rich tile owes more than a thin one, and supplying the reference rate on rich ground would
        // leave this Field short.
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let tile_entity = app
            .world
            .resource::<core_sim::TileRegistry>()
            .index(field_tile.x, field_tile.y)
            .expect("the Field's tile is on the map");
        let tile_capacity = core_sim::tile_forage_capacity(
            &labor.forage,
            app.world
                .get::<core_sim::Tile>(tile_entity)
                .expect("the Field's tile carries a Tile"),
        );
        let mut forage = app.world.resource_mut::<ForageRegistry>();
        let patch = forage.patch_mut(field_tile).expect("the Field persists");
        patch.upkeep_supplied =
            core_sim::patch_upkeep_demand(patch, &ladder, tile_capacity, &labor.forage);
    }

    // --- Set up a completed, worked corral (pen) on a real herd. --------------------------------
    // Domesticated + corralled at its tile with a grown (radius-1) fence.
    let (herd_id, pen_tile) = {
        let mut herds = app.world.resource_mut::<HerdRegistry>();
        // **A PENNABLE herd, not simply the first one** — the fixture runs the real accrual now, and
        // `tame_outright` / `corral_at` both refuse a species whose `husbandry_ceiling` forbids the
        // rung. Picking by the gate is what makes "you cannot fabricate a penned `wild` herd" hold
        // here as everywhere else.
        let index = herds
            .herds
            .iter()
            .position(|h| h.id.starts_with("game_") && h.can_pen())
            .expect(
                "the harness map must seed at least one pennable herd — this fixture stands up a \
                 real, completed pen",
            );
        let herd = &mut herds.herds[index];
        let tile = herd.current_pos;
        // **THE REAL ACCRUAL, BOTH RUNGS** — `tame_outright` then `corral_at`, which is what
        // `FABRICATED_BUILD_COST`'s own doc says a fixture wanting a finished rung must do.
        //
        // # ⛔ WRITING THE METER DID NOT STAND UP A PEN, AND THE TEST WENT QUIET
        //
        // Two byte-identical `set_ladder_position(FABRICATED_BUILD_COST, …)` calls stood here (the
        // second dead). `FABRICATED_BUILD_COST` is **one worker-turn** while `animal:pastoral`
        // costs 50 and `animal:pen` 75, so a position of `1.0` leaves the herd holding
        // `animal:wild`: `is_domesticated()` and `corral_meter_full()` were both **false**, where
        // the retired two-meter form made both true. What survived was `corralled_at` and
        // `pen_radius` — two plain fields the fixture had written by hand — so the pen half of this
        // test would have passed with the pen **rung's** capture and restore entirely broken.
        let ladder = core_sim::LadderConfig::builtin();
        assert!(
            herd.tame_outright(faction, &ladder),
            "fixture: the herd must be tameable, or there is no pastoral rung under the fence"
        );
        assert!(
            herd.corral_at(tile, &ladder),
            "fixture: the herd must be pennable, or there is no completed pen to round-trip"
        );
        herd.pen_radius = 1;
        herd.biomass = herd.carrying_capacity;
        // The one-turn "keeper tended it" grace the restore drops. `corral_at` grants it; restated
        // here because it is the signal under test rather than a side effect the fixture inherits.
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
                    take_species: TakeSelection::EVERYTHING,
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
    // **AND THE RUNG UNDER IT** — `corralled_at` and `pen_radius` are plain fields the fixture wrote
    // by hand, so a pen assertion that reads only those passes with the ladder position's capture or
    // restore entirely broken. The position is what says the fence was *built*.
    assert!(
        app.world
            .resource::<HerdRegistry>()
            .find(&herd_id)
            .expect("herd restored")
            .corral_meter_full(),
        "the pen RUNG must survive the round-trip, not just the two fence fields beside it \
         (ladder position = {})",
        app.world
            .resource::<HerdRegistry>()
            .find(&herd_id)
            .expect("herd restored")
            .ladder_position()
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
         (ladder position = {}, Field work done = {})",
        patch.ladder_position(),
        core_sim::patch_rung_work_done(
            &patch,
            core_sim::RungKey::PlantField,
            &core_sim::LadderConfig::builtin(),
        )
    );
    assert_eq!(
        herd.corralled_at,
        Some(pen_tile),
        "the pen escaped on the post-restore turn: corralled_at = {:?}, pen_radius = {}, \
         corral_progress = {}",
        herd.corralled_at,
        herd.pen_radius,
        herd.rung_work_done(
            core_sim::RungKey::AnimalPen,
            &core_sim::LadderConfig::builtin()
        )
    );
    assert_eq!(
        herd.pen_radius, 1,
        "the pen's ExtendPen radius was thrown away on the post-restore turn"
    );
    assert!(
        herd.corral_meter_full() && herd.is_domesticated(),
        "…and so was the ladder position both rungs stand on (position = {}, pen work done = {})",
        herd.ladder_position(),
        herd.rung_work_done(
            core_sim::RungKey::AnimalPen,
            &core_sim::LadderConfig::builtin()
        )
    );
}
