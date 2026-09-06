//! **A band's name survives a checkpoint** — the half of issue #615 that a rollback would otherwise
//! undo.
//!
//! The whole reason the sim owns a band's name is that nothing about it may move when a *different*
//! band changes. A checkpoint that carried the id but not the name would restore every band nameless
//! and hand the naming back to the client's positional fallback, which is the exact defect being
//! removed — so the guard here is a **real** round trip through the shipped capture/restore path,
//! and it deliberately kills a band between the two.

use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    build_test_app, BandId, BandName, BandNameAllocator, FactionId, PopulationCohort, ResidentBand,
    SimulationConfig,
};

/// Workers the guard below sends out to form a second band. Small enough for any worldgen band.
const SPLINTER_WORKERS: u32 = 1;

/// Fission thresholds relaxed to the minimum, so the split is about *having two bands* and not
/// about the founding economy this file does not test.
const SETTLE: core_sim::SettleConfig = core_sim::SettleConfig {
    min_founding_workers: 1,
    parent_min_workers: 0,
};

/// A deterministic earthlike world, so the bands and their names are the same every run.
fn spawn_world() -> bevy::app::App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// Every band in the world, by durable id, with the name it currently holds.
fn names_by_band(app: &mut bevy::app::App) -> std::collections::BTreeMap<u64, String> {
    let mut query = app.world.query::<(&BandId, &BandName)>();
    query
        .iter(&app.world)
        .map(|(id, name)| (id.0, name.0.clone()))
        .collect()
}

/// The first resident band's entity, id and faction.
fn a_resident_band(app: &mut bevy::app::App) -> (Entity, u64, FactionId) {
    let mut query = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>();
    let (entity, id, cohort) = query
        .iter(&app.world)
        .next()
        .expect("the campaign spawns a resident band");
    (entity, id.0, cohort.faction)
}

/// The entity currently carrying `band_id`. A restore respawns every band, so an `Entity` grabbed
/// before the rollback names nothing afterwards — the durable id is the only handle that survives.
fn entity_for_band(app: &mut bevy::app::App, band_id: u64) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| id.0 == band_id)
        .map(|(entity, _)| entity)
        .expect("the band is in the world")
}

/// **Worldgen founds every band with a name**, which is the precondition everything below rests on.
#[test]
fn worldgen_founds_every_band_with_a_distinct_name() {
    let mut app = spawn_world();
    let names = names_by_band(&mut app);
    assert!(!names.is_empty(), "the campaign spawns at least one band");
    for (id, name) in &names {
        assert!(!name.is_empty(), "band {id} was founded nameless");
    }
    let distinct: std::collections::BTreeSet<&String> = names.values().collect();
    assert_eq!(
        distinct.len(),
        names.len(),
        "two bands of one faction share a name: {names:?}"
    );
}

/// **The names and the counters both come back.** The counters matter as much as the strings: a
/// restore that put the bands back but reset the per-faction slot would mint an already-issued name
/// for the next band founded, which is the aliasing case [`BandNameAllocator`] exists to prevent.
#[test]
fn a_checkpoint_round_trip_preserves_every_name_and_the_counters() {
    let mut app = spawn_world();
    let before = names_by_band(&mut app);
    let (_, _, faction) = a_resident_band(&mut app);
    let counter_before = app.world.resource::<BandNameAllocator>().peek(faction);
    assert!(
        counter_before > 0,
        "worldgen minted at least one name for the player faction"
    );

    let checkpoint = capture_sim_state(&app.world);
    restore_sim_state(&mut app.world, &checkpoint);

    assert_eq!(
        names_by_band(&mut app),
        before,
        "a rollback must hand every band back the name it had"
    );
    assert_eq!(
        app.world.resource::<BandNameAllocator>().peek(faction),
        counter_before,
        "the per-faction counter is checkpoint state; a reset one re-issues a live name"
    );
}

/// **A restore rewinds the counter to the checkpoint's, so a re-founded band re-mints its name.**
///
/// This is the determinism guarantee made executable: replaying the command log from a checkpoint
/// in a process that already ran past it must reproduce the run being replayed, name for name. The
/// world here plays a founding forward, rolls back, and founds again — and the second splinter has
/// to be handed the *same* name as the first. A restore that carried the live high-water mark
/// across instead of installing the checkpoint's counters would mint the next name down and the
/// replay would silently diverge from the run it is replaying.
#[test]
fn a_restore_rewinds_the_counter_so_a_re_founded_band_mints_the_same_name() {
    let mut app = spawn_world();
    let (_, parent_id, faction) = a_resident_band(&mut app);
    let counter_at_checkpoint = app.world.resource::<BandNameAllocator>().peek(faction);
    let checkpoint = capture_sim_state(&app.world);

    // The run being replayed: a founding past the checkpoint moves the faction's counter on.
    let parent = entity_for_band(&mut app, parent_id);
    let first = core_sim::split_band_from_parent(&mut app.world, parent, SPLINTER_WORKERS, &SETTLE)
        .expect("a worldgen band can spare a splinter");
    let first_name = names_by_band(&mut app)
        .get(&first.band.0)
        .cloned()
        .expect("the splinter is a band with a name");
    assert!(
        app.world.resource::<BandNameAllocator>().peek(faction) > counter_at_checkpoint,
        "the founding has to move the counter, or this proves nothing"
    );

    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(
        app.world.resource::<BandNameAllocator>().peek(faction),
        counter_at_checkpoint,
        "the checkpoint is the authority on where the counter stood, not the high-water mark"
    );

    // The replay: the same founding runs again and must land on the same name.
    let parent = entity_for_band(&mut app, parent_id);
    let second =
        core_sim::split_band_from_parent(&mut app.world, parent, SPLINTER_WORKERS, &SETTLE)
            .expect("the restored parent can spare the same splinter");
    assert_eq!(
        second.band.0, first.band.0,
        "the id allocator rewound, so the replayed splinter is the same band"
    );
    assert_eq!(
        names_by_band(&mut app).get(&second.band.0),
        Some(&first_name),
        "a replayed founding must re-mint the name that band actually had"
    );
}

/// **A band dying does NOT rename the survivors** — the defect in one sentence.
///
/// The old client-side naming was a row index, so removing a band shifted every band after it. Here
/// a band is captured, killed, and the world restored from the checkpoint: the survivors' names are
/// asserted **against the pre-death capture**, so a positional scheme sneaking back in fails.
#[test]
fn a_band_dying_and_a_rollback_leave_the_survivors_named_as_they_were() {
    let mut app = spawn_world();
    let (parent, _, _) = a_resident_band(&mut app);
    // A second band, through the real fission verb — which also puts the splinter's own fresh mint
    // on the hook: a splinter that inherited or blanked its name shows up in the distinctness
    // assertion below.
    let split = core_sim::split_band_from_parent(&mut app.world, parent, SPLINTER_WORKERS, &SETTLE)
        .expect("a worldgen band can spare a splinter");
    let before = names_by_band(&mut app);
    assert!(
        before.len() > 1,
        "the split gave the world a survivor beside the band this kills"
    );
    let splinter_name = before
        .get(&split.band.0)
        .expect("the splinter is a band with a name");
    assert!(
        !splinter_name.is_empty(),
        "a fission splinter is a NEW band and mints its own name"
    );

    let (doomed, doomed_id, _) = a_resident_band(&mut app);
    let survivors: std::collections::BTreeMap<u64, String> = before
        .iter()
        .filter(|(id, _)| **id != doomed_id)
        .map(|(id, name)| (*id, name.clone()))
        .collect();

    let checkpoint = capture_sim_state(&app.world);
    app.world.despawn(doomed);

    let after_death = names_by_band(&mut app);
    assert!(
        !after_death.contains_key(&doomed_id),
        "the band really is gone"
    );
    for (id, name) in &survivors {
        assert_eq!(
            after_death.get(id),
            Some(name),
            "band {id} was renamed by another band's death"
        );
    }

    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(
        names_by_band(&mut app),
        before,
        "the rollback restores the dead band's name too, and moves nobody else's"
    );
}
