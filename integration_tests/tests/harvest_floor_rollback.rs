mod common;

use bevy::prelude::{Entity, UVec2};
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::NO_CREW_ON_THIS_ACTIVITY;
use core_sim::{
    build_headless_app, scalar_from_f32, scalar_one, scalar_zero, BandId, FactionId, GenerationId,
    LaborAllocation, LaborAssignment, LaborTarget, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, TileRegistry, DEFAULT_ESCAPEMENT_FLOOR,
};

/// The band id the fixture's cohort carries. A checkpoint keys bands by `BandId` — a cohort without
/// one is not captured at all — so this is load-bearing, not decoration.
const PROBE_BAND: BandId = BandId(9_042);

/// A floor no default and no stance names, so a rewind to it cannot be produced by accident — not by
/// a defaulted field, not by a stance mapping, not by a re-pick.
const CAPTURED_FLOOR: f32 = 0.42;

/// The floor the assignment is dragged to *after* the checkpoint. Also unnamed by any stance, so
/// neither end of the comparison can be reached by a fallback.
const DRAGGED_FLOOR: f32 = 0.07;

/// The quarry's display name the checkpointed party carries. **Deliberately not a name any fauna
/// roster holds**, for the same reason the floors are off-default: a restore that re-derived the name
/// from the registry instead of restoring it would come back empty or come back something else, and
/// either way could not accidentally equal this.
const CAPTURED_SPECIES: &str = "Checkpointed Quarry";

/// **A rollback rewinds the FLOOR** (`docs/plan_harvest_floor.md` §4).
///
/// The floor is the whole of what the player decides about harvest pressure, so a rewind that
/// restored the crew and the tile but re-picked the floor would hand back an assignment the player
/// never made. It rides `LaborTarget`, which `SimState` clones whole with the `LaborAllocation`
/// component — this pins that the clone actually carries it, on **both** food webs.
///
/// Asserted at floors **no stance names** (`0.42`, `0.07`): a rewind that quietly defaulted, or that
/// round-tripped through the four-value wire label still shipped beside the floor, would land on
/// `0.5` or `0.3` and be caught here rather than passing on a coincidence.
#[test]
fn a_rollback_rewinds_the_harvest_floor_on_both_webs() {
    common::ensure_test_config();
    let mut app = build_headless_app();
    app.update();

    let band = spawn_band_with_floors(&mut app, CAPTURED_FLOOR);
    assert_eq!(
        floors_of(&app, band),
        (CAPTURED_FLOOR, CAPTURED_FLOOR),
        "the fixture must start at the floor it claims to"
    );

    let checkpoint = capture_sim_state(&app.world);

    // Drag both assignments to a different floor — the edit the rollback has to undo.
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band carries its allocation");
        for assignment in allocation.assignments.iter_mut() {
            match &mut assignment.target {
                LaborTarget::Forage { floor, .. } | LaborTarget::Hunt { floor, .. } => {
                    *floor = DRAGGED_FLOOR
                }
                _ => {}
            }
        }
    }
    assert_eq!(
        floors_of(&app, band),
        (DRAGGED_FLOOR, DRAGGED_FLOOR),
        "the edit must land, or the rewind below asserts nothing"
    );

    restore_sim_state(&mut app.world, &checkpoint);

    // The band is a *new* entity after a restore (the world is rebuilt and every entity renumbered),
    // so the floors are read off whichever band carries the two assignments.
    assert_eq!(
        floors_of_the_only_band(&mut app),
        (CAPTURED_FLOOR, CAPTURED_FLOOR),
        "a rollback restores the floor the player set, on both webs"
    );
}

/// A resident band carrying one Forage and one Hunt assignment, both at `floor`.
fn spawn_band_with_floors(app: &mut bevy::prelude::App, floor: f32) -> Entity {
    // A **real** map tile: a checkpoint keys a band by its home tile's position, so a hand-spawned
    // tile the `TileRegistry` does not index would be recorded at `(0, 0)` and the restore would put
    // the band somewhere the fixture never placed it.
    let (tile, tile_pos) = {
        let registry = app.world.resource::<TileRegistry>();
        let entity = *registry.tiles.first().expect("worldgen seeded a map");
        (entity, UVec2::ZERO)
    };
    let tile_pos = app
        .world
        .get::<core_sim::Tile>(tile)
        .map(|t| t.position)
        .unwrap_or(tile_pos);
    app.world
        .spawn((
            PROBE_BAND,
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 20,
                children: scalar_zero(),
                working: scalar_from_f32(8.0),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            ResidentBand,
            LaborAllocation {
                assignments: vec![
                    LaborAssignment {
                        target: LaborTarget::Forage {
                            tile: tile_pos,
                            floor,
                            species: None,
                        },
                        workers: 2,
                        improvement: None,
                        kit: None,
                        improvement_workers: NO_CREW_ON_THIS_ACTIVITY,
                    },
                    LaborAssignment {
                        target: LaborTarget::Hunt {
                            fauna_id: "game_rollback_probe".to_string(),
                            floor,
                        },
                        workers: 2,
                        improvement: None,
                        kit: None,
                        improvement_workers: NO_CREW_ON_THIS_ACTIVITY,
                    },
                ],
                ..Default::default()
            },
        ))
        .id()
}

/// `(forage floor, hunt floor)` off a known band entity.
fn floors_of(app: &bevy::prelude::App, band: Entity) -> (f32, f32) {
    read_floors(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band carries its allocation"),
    )
}

/// `(forage floor, hunt floor)` off the one band in the world that carries both assignments — the
/// post-restore reader, since a restore renumbers entities.
fn floors_of_the_only_band(app: &mut bevy::prelude::App) -> (f32, f32) {
    let mut query = app.world.query::<&LaborAllocation>();
    let allocation = query
        .iter(&app.world)
        .find(|allocation| allocation.assignments.len() == 2)
        .expect("the restored world carries the band's two assignments");
    read_floors(allocation)
}

fn read_floors(allocation: &LaborAllocation) -> (f32, f32) {
    let mut forage = None;
    let mut hunt = None;
    for assignment in &allocation.assignments {
        match &assignment.target {
            LaborTarget::Forage { floor, .. } => forage = Some(*floor),
            LaborTarget::Hunt { floor, .. } => hunt = Some(*floor),
            _ => {}
        }
    }
    (
        forage.expect("a Forage assignment"),
        hunt.expect("a Hunt assignment"),
    )
}

/// **The default floor is the food peak** — `docs/plan_harvest_floor.md` §10 Q3's answer, pinned so
/// a retune has to be deliberate. A fresh assignment that named no floor gets the *sustainable* one,
/// which is what makes the common case one click.
#[test]
fn the_default_floor_is_the_food_peak() {
    assert_eq!(
        DEFAULT_ESCAPEMENT_FLOOR,
        core_sim::MSY_BIOMASS_FRACTION,
        "a fresh assignment defaults to the biomass at which a source grows fastest"
    );
    assert!(
        !core_sim::floor_overdraws(DEFAULT_ESCAPEMENT_FLOOR),
        "…so it does not overdraw"
    );
    assert_eq!(
        core_sim::learn_multiplier(DEFAULT_ESCAPEMENT_FLOOR),
        1.0,
        "…and it is the floor the learning curve is normalised on, so the ladder's stated build and \
         lesson lengths are the ones a fresh assignment gets"
    );
}

/// **A hunt expedition's floor round-trips through the mission and the rollback.**
///
/// The floor is the whole of what a raid's orders say — how deep to draw the herd — so it has to
/// survive the same hops the assignment's floor does. Pinned at a floor **no retired stance named**
/// (`0.42`), so a value that appears at the far end cannot have come from a default or from a
/// label.
#[test]
fn an_expedition_floor_round_trips_through_the_mission_and_the_rollback() {
    use core_sim::{Expedition, ExpeditionMission, ExpeditionPhase};

    common::ensure_test_config();
    let mut app = build_headless_app();
    app.update();

    let tile = *app
        .world
        .resource::<TileRegistry>()
        .tiles
        .first()
        .expect("worldgen seeded a map");
    // **A real home band with its own `BandId`.** A checkpoint names an expedition's home by band id,
    // and restore DROPS an expedition whose home cannot be resolved — so a party pointed at anything
    // but a captured band would vanish on the rewind and this test would pass vacuously.
    let home = spawn_band_with_floors(&mut app, CAPTURED_FLOOR);
    let party = app
        .world
        .spawn((
            BandId(9_043),
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 4,
                children: scalar_zero(),
                working: scalar_from_f32(4.0),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            Expedition {
                home_band: home,
                mission: ExpeditionMission::Hunt {
                    fauna_id: "game_raid_probe".to_string(),
                    target_species: CAPTURED_SPECIES.to_string(),
                    floor: CAPTURED_FLOOR,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
                cargo: core_sim::LocalStore::new(),
            },
        ))
        .id();
    assert_eq!(
        raid_floor_of(&app, party),
        CAPTURED_FLOOR,
        "the fixture must start at the floor it claims to"
    );

    let checkpoint = capture_sim_state(&app.world);
    {
        let mut expedition = app
            .world
            .get_mut::<Expedition>(party)
            .expect("the party carries its mission");
        if let ExpeditionMission::Hunt { floor, .. } = &mut expedition.mission {
            *floor = DRAGGED_FLOOR;
        }
    }
    assert_eq!(raid_floor_of(&app, party), DRAGGED_FLOOR);

    restore_sim_state(&mut app.world, &checkpoint);

    // A restore renumbers entities, so the orders are read off whichever party carries the mission.
    let mut query = app.world.query::<&Expedition>();
    let restored = query
        .iter(&app.world)
        .find_map(|expedition| match &expedition.mission {
            ExpeditionMission::Hunt {
                floor,
                target_species,
                ..
            } => Some((*floor, target_species.clone())),
            _ => None,
        })
        .expect("the restored world carries the hunt mission");
    assert_eq!(
        restored.0, CAPTURED_FLOOR,
        "a rollback restores the raid's orders, not re-picked ones"
    );
    // **The quarry's NAME survives the round trip too.** It is resolved once at launch and can never
    // be re-derived — the herd it names may be gone from the registry by now — so a checkpoint that
    // dropped it would leave a restored party rendering its raw fauna id, which is the whole of issue
    // #378 reintroduced by the persistence layer instead of by the wire.
    assert_eq!(
        restored.1, CAPTURED_SPECIES,
        "a rollback restores the quarry's display name, which cannot be looked up again"
    );
}

fn raid_floor_of(app: &bevy::prelude::App, party: Entity) -> f32 {
    match &app
        .world
        .get::<core_sim::Expedition>(party)
        .expect("the party carries its mission")
        .mission
    {
        core_sim::ExpeditionMission::Hunt { floor, .. } => *floor,
        other => panic!("expected a hunt mission, got {other:?}"),
    }
}
