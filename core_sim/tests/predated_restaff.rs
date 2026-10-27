//! **A COMMAND-TIME RESTAFF PRICES THE PREDATION THE TURN WILL SUFFER.**
//!
//! Live playtest: a band worked two Rabbit Warrens a Grey Wolf Pack was eating the whole regrowth of
//! every turn. The turn staffed each row against a warren standing at its floor (1 hand each); every
//! command then restaffed against `fauna::next_turns_quarry`, which regrew the warren and ignored the
//! pack, saw ~8.7 of room, and swallowed the idle hands into rows the turn would not use.
//!
//! The fixture stands a warren on its hunt floor with a predation stamp equal to one turn's
//! regrowth (what the pack ate), restaffs, and compares the crew with (a) the same warren unstamped
//! (the pre-fix view: more hands) and (b) a warren that does not regrow at all (the honest view of a
//! stock that is eaten back to the floor every turn).

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::{
    build_test_app, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_herds, FactionId,
    FaunaConfigHandle, GenerationId, HerdRegistry, LaborAllocation, LaborAssignment, LaborTarget,
    LocalStore, MoraleCause, PopulationCohort, ResidentBand, SourcePriority, TileRegistry,
};

const SEED: u64 = 119_304_647;
const WARREN: &str = "Rabbit Warren";
const CAPACITY: f32 = 100.0;
/// The shipped wild breeding rate of the rabbit warren (regrowth 8.75 a turn at `K/2`).
const WARREN_REGROWTH_RATE: f32 = 0.35;
/// The hunt row's floor: half the capacity, where the logistic regrows fastest.
const FLOOR: f32 = 0.5;
/// The row's Max, far above the hands the room can use, so the staffing is what is measured.
const ROW_MAX: u32 = 12;
const WORKING: u32 = 12;

/// A world with its first game herd re-shaped into a warren standing on its floor.
fn world(regrowth: Option<f32>) -> (App, String, UVec2) {
    let mut app = build_test_app();
    app.world
        .insert_resource(core_sim::labor_config_with_room_on_the_tile());
    app.world
        .resource_mut::<core_sim::SimulationConfig>()
        .map_seed = SEED;
    app.update();
    let body_mass = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .species_by_display(WARREN)
        .expect("shipped species")
        .body_mass;
    let (id, pos) = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|h| h.id.starts_with("game_"))
            .expect("the map seeded game");
        herd.species = WARREN.to_string();
        herd.body_mass = body_mass;
        herd.fodder_per_biomass = 0.0;
        herd.carrying_capacity = CAPACITY;
        herd.biomass = CAPACITY * FLOOR;
        herd.biomass_before_regrowth = herd.biomass;
        // The startup turn may already have let a pack eat from this herd: the stamp is the
        // fixture's to set.
        herd.predator_eaten_this_turn = 0.0;
        herd.regrowth_rate = regrowth.unwrap_or(WARREN_REGROWTH_RATE);
        (herd.id.clone(), herd.position())
    };
    app.world.run_system_once(spawn_initial_herds);
    (app, id, pos)
}

use bevy::ecs::system::RunSystemOnce;

fn band(app: &mut App, pos: UVec2, fauna_id: &str) -> bevy::prelude::Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the herd's tile resolves");
    app.world
        .spawn((
            ResidentBand,
            PopulationCohort {
                home: tile,
                current_tile: tile,
                last_fertility_factors: Default::default(),
                last_breeding: Default::default(),
                size: 200,
                children: scalar_zero(),
                working: scalar_from_f32(WORKING as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                fed_party_workers: Default::default(),
                last_food_spoiled: 0.0,
                last_food_into_store: 0.0,
                last_food_from_store: 0.0,
                last_food_kept: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
                predators_kept_off: 0,
                last_cohesion: Default::default(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    party: None,
                    muster_crew: 0,
                    planned_crew: 0,
                    scouting_range: 0,
                    scouting_reach: 0,
                    max_workers: Some(ROW_MAX),
                    staffing: Default::default(),
                    target: LaborTarget::Hunt {
                        fauna_id: fauna_id.to_string(),
                        floor: FLOOR,
                        move_with_herd: false,
                    },
                    workers: ROW_MAX,
                    kit: None,
                    priority: SourcePriority::default(),
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// The hands a command-time restaff settles on the band's one hunt row, with the herd carrying
/// `eaten` as last turn's predation stamp.
fn restaffed_crew(regrowth: Option<f32>, eaten: f32) -> u32 {
    let (mut app, id, pos) = world(regrowth);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry
            .herds
            .iter_mut()
            .find(|h| h.id == id)
            .expect("herd")
            .predator_eaten_this_turn = eaten;
    }
    let entity = band(&mut app, pos, &id);
    core_sim::staffing::restaff_band(&mut app.world, entity);
    app.world
        .get::<LaborAllocation>(entity)
        .expect("allocation")
        .assignments
        .first()
        .map_or(0, |row| row.workers)
}

/// One turn's regrowth of the warren standing on its floor, read off the shipped forecast.
fn one_turns_regrowth() -> f32 {
    let (app, id, _) = world(None);
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("herd");
    core_sim::next_turns_quarry(herd, &fauna).biomass - herd.biomass
}

#[test]
fn a_restaff_staffs_a_predated_row_the_crew_the_turn_staffed() {
    let regrowth = one_turns_regrowth();
    assert!(regrowth > 1.0, "liveness: the warren regrows ({regrowth})");

    let unpredated = restaffed_crew(None, 0.0);
    let predated = restaffed_crew(None, regrowth);
    // The turn staffs a warren the pack has eaten back to its floor: nothing regrows for the crew.
    let the_turns_view = restaffed_crew(Some(0.0), 0.0);

    assert!(
        unpredated > the_turns_view,
        "control: with no stamp the restaff sees the room ({unpredated} vs {the_turns_view})"
    );
    assert_eq!(
        predated, the_turns_view,
        "a restaff must staff the row the same crew the turn did"
    );
}
