//! **A PENNED HERD'S HOLD CREW TAKES THE REGROWTH AT THE FLOOR** (`work-party.md` -> "Hold crew").
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

const BELOW_FLOOR: f32 = 0.9;
const SEED: u64 = 119_304_647;
const PENNED: &str = "Wild Boar";
const CAPACITY: f32 = 1000.0;
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
        .species_by_display(PENNED)
        .expect("shipped species")
        .body_mass;
    let (id, pos) = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|h| h.id.starts_with("game_"))
            .expect("the map seeded game");
        herd.species = PENNED.to_string();
        herd.body_mass = body_mass;
        herd.fodder_per_biomass = 0.0;
        herd.carrying_capacity = CAPACITY;
        herd.biomass = CAPACITY * FLOOR;
        herd.biomass_before_regrowth = herd.biomass;
        // The startup turn may already have let a pack eat from this herd: the stamp is the
        // fixture's to set.
        herd.predator_eaten_this_turn = 0.0;
        herd.regrowth_rate = regrowth.unwrap_or(WARREN_REGROWTH_RATE);
        let ladder = core_sim::LadderConfig::builtin();
        herd.husbandry_ceiling = Some(core_sim::shipped::animal_pen());
        assert!(herd.tame_outright(FactionId(0), &ladder));
        let at = herd.position();
        assert!(herd.corral_at(at, &ladder));
        herd.carrying_capacity = CAPACITY;
        herd.biomass = CAPACITY * FLOOR * BELOW_FLOOR;
        herd.biomass_before_regrowth = herd.biomass;
        herd.upkeep_supplied = 1_000.0;
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
                last_leadership: Default::default(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    party: None,
                    muster_crew: 0,
                    planned_crew: 0,
                    scouting_range: 0,
                    scouting_reach: 0,
                    for_tribe: None,
                    scouting_stretch: 1.0,
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

/// The hands a command-time restaff settles on the band's one hunt row.
fn restaffed_crew(regrowth: f32) -> u32 {
    let (mut app, id, pos) = world(Some(regrowth));
    let entity = band(&mut app, pos, &id);
    core_sim::staffing::restaff_band(&mut app.world, entity);
    app.world
        .get::<LaborAllocation>(entity)
        .expect("allocation")
        .assignments
        .first()
        .map_or(0, |row| row.workers)
}

/// A pen regrowing more than one keeper's leftover reach can take: the keeping rides in his slack
/// only where the slack reaches the regrowth, so the hold crew is a second hand.
const REGROWTH_ONE_KEEPERS_SLACK_CANNOT_TAKE: f32 = 0.2;
/// A pen whose regrowth that slack covers: one keeper holds it.
const REGROWTH_ONE_KEEPERS_SLACK_COVERS: f32 = 0.06;

/// **A PEN'S HOLD CREW IS SIZED ON THE REGROWTH THE KEEPERS MUST TAKE AT THE FLOOR, NOT ON THIS
/// TURN'S ROOM** - the live Saltmarsh case: one keeper, 0.54 of a hand on the keeping, could take
/// 2.09 of a 2.80 line because the fit test passed on the (smaller) room above the floor. The
/// staffing's own restaff (the command path) on a penned boar herd standing a little under its floor,
/// the herd's room under what it regrows.
#[test]
fn a_pens_hold_crew_is_two_where_one_keepers_slack_cannot_take_the_regrowth() {
    assert_eq!(
        restaffed_crew(REGROWTH_ONE_KEEPERS_SLACK_COVERS),
        1,
        "the control: the slack covers the regrowth, so one keeper holds it"
    );
    assert!(
        restaffed_crew(REGROWTH_ONE_KEEPERS_SLACK_CANNOT_TAKE) >= 2,
        "one keeper's slack cannot take the regrowth: the hold crew is a second hand"
    );
}

/// The band's hunt row as the ENCODED snapshot carries it: `(workers, hold, reach, actual, sustainable)`.
fn published_row(app: &App) -> (u32, u32, u32, f32, f32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == "hunt")
        .expect("the hunt row is on the wire");
    (
        row.workers(),
        row.holdCrew(),
        row.reachCrew(),
        row.actualYield(),
        row.sustainableYield(),
    )
}

/// Turns the pen runs before its published take is read, so the stock has settled onto its floor.
const SETTLE_TURNS: u32 = 4;
/// A take within this share of the pen's MSY counts as paying it (the take is whole animals).
const PAYS_ITS_MSY: f32 = 0.95;

/// The row `(workers, hold crew, actual, sustainable)` off the encoded frame after the pen has run
/// `SETTLE_TURNS` turns with the row's Max at `max`.
fn settled_row(regrowth: f32, max: u32) -> (u32, u32, f32, f32) {
    let (mut app, id, pos) = world(Some(regrowth));
    let entity = band(&mut app, pos, &id);
    app.world
        .get_mut::<LaborAllocation>(entity)
        .expect("allocation")
        .assignments[0]
        .max_workers = Some(max);
    core_sim::staffing::restaff_band(&mut app.world, entity);
    for _ in 0..SETTLE_TURNS {
        app.update();
    }
    let (workers, hold, _reach, actual, sustainable) = published_row(&app);
    (workers, hold, actual, sustainable)
}

/// **ON THE ENCODED FRAME: A PEN STAFFED AT 2 PAYS ITS MSY, ONE KEEPER DOES NOT, AND A PEN ONE
/// KEEPER'S SLACK COVERS STAYS AT ONE.**
#[test]
fn a_penned_herd_staffed_at_two_pays_its_msy_where_one_keeper_does_not() {
    let (workers, hold, actual, sustainable) =
        settled_row(REGROWTH_ONE_KEEPERS_SLACK_CANNOT_TAKE, ROW_MAX);
    assert_eq!(workers, 2, "the hold crew is two keepers");
    assert_eq!(hold, 2);
    assert!(sustainable > 0.0, "liveness: the pen has a line to pay");
    assert!(
        actual >= PAYS_ITS_MSY * sustainable,
        "two keepers pay the pen's MSY: {actual} of {sustainable}"
    );
    // The control: the same pen held to one keeper pays under its MSY.
    let (one, _, one_actual, _) = settled_row(REGROWTH_ONE_KEEPERS_SLACK_CANNOT_TAKE, 1);
    assert_eq!(one, 1);
    assert!(
        one_actual < PAYS_ITS_MSY * sustainable,
        "one keeper cannot take the regrowth: {one_actual} of the pen's {sustainable}"
    );
    // A pen one keeper's slack covers is staffed at one and pays its MSY there.
    let (covered, _, covered_actual, covered_sustainable) =
        settled_row(REGROWTH_ONE_KEEPERS_SLACK_COVERS, ROW_MAX);
    assert_eq!(covered, 1);
    assert!(covered_actual >= PAYS_ITS_MSY * covered_sustainable);
}

// ---------------------------------------------------------------------------------------------
// A WILD HERD STANDING AT ITS FLOOR DOES NOT DRAW THE HANDS A PRODUCING ROW COULD USE
// ---------------------------------------------------------------------------------------------

/// Hands the band can field in the two-herd fixture.
const BAND_HANDS: u32 = 8;
/// A herd standing under its floor by a share of the floor stock that its regrowth just closes: the
/// turn staffs it after the regrowth, so its room this turn is a sliver of what it regrows at the floor.
const REGROWS_ONTO_ITS_FLOOR: f32 = 0.84;
/// The most hands a row for a herd with only a sliver of room may be dealt.
const A_SLIVERS_HANDS: u32 = 2;

/// **A WILD HERD AT ITS FLOOR TAKES THE HANDS ITS ROOM CAN USE, NOT THE REGROWTH'S WORTH OF BARE
/// HANDS** (seed 37 of the AI bench: a boar herd with 0.00 take drew 6 of a band's 8 hands off a
/// producing forage row). The pen's hold crew is sized on the regrowth at the floor because its
/// keepers must take it; a wild herd keeps nobody, and its take this turn is its room, so pass 3 must
/// not deal it bare hands sized on a regrowth its room cannot yield. Two wild warrens, one full (room
/// to spare) and one a hair over its floor, on the real restaff path.
#[test]
fn a_wild_herd_at_its_floor_does_not_draw_hands_off_a_herd_with_room() {
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
        .species_by_display(PENNED)
        .expect("shipped species")
        .body_mass;
    let warren = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .species_by_display("Rabbit Warren")
        .expect("shipped species")
        .body_mass;
    let _ = body_mass;
    let (full_id, thin_id, pos) = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let ids: Vec<String> = registry
            .herds
            .iter()
            .filter(|h| h.id.starts_with("game_"))
            .map(|h| h.id.clone())
            .take(2)
            .collect();
        assert_eq!(ids.len(), 2, "the map seeded two game herds");
        let pos = registry
            .herds
            .iter()
            .find(|h| h.id == ids[0])
            .expect("herd")
            .position();
        for (index, id) in ids.iter().enumerate() {
            let herd = registry
                .herds
                .iter_mut()
                .find(|h| &h.id == id)
                .expect("herd");
            herd.species = "Rabbit Warren".to_string();
            herd.body_mass = warren;
            herd.fodder_per_biomass = 0.0;
            herd.carrying_capacity = CAPACITY;
            herd.regrowth_rate = WARREN_REGROWTH_RATE;
            herd.predator_eaten_this_turn = 0.0;
            herd.current_pos = pos;
            herd.route.clear();
            herd.biomass = if index == 0 {
                CAPACITY
            } else {
                CAPACITY * FLOOR * REGROWS_ONTO_ITS_FLOOR
            };
            herd.biomass_before_regrowth = herd.biomass;
        }
        (ids[0].clone(), ids[1].clone(), pos)
    };
    app.world.run_system_once(spawn_initial_herds);
    let entity = band(&mut app, pos, &full_id);
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(entity)
            .expect("cohort");
        cohort.working = scalar_from_f32(BAND_HANDS as f32);
        let mut allocation = app.world.get_mut::<LaborAllocation>(entity).expect("a");
        let template = allocation.assignments[0].clone();
        allocation.assignments.clear();
        for id in [&full_id, &thin_id] {
            let mut row = template.clone();
            row.max_workers = None;
            row.workers = 0;
            row.target = LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: FLOOR,
                move_with_herd: false,
            };
            allocation.assignments.push(row);
        }
    }
    core_sim::staffing::restaff_band(&mut app.world, entity);
    let crews: Vec<(String, u32)> = app
        .world
        .get::<LaborAllocation>(entity)
        .expect("allocation")
        .assignments
        .iter()
        .filter_map(|row| match &row.target {
            LaborTarget::Hunt { fauna_id, .. } => Some((fauna_id.clone(), row.workers)),
            _ => None,
        })
        .collect();
    let crew_of = |id: &str| {
        crews
            .iter()
            .find(|(held, _)| held == id)
            .map_or(0, |(_, workers)| *workers)
    };
    assert!(
        crew_of(&full_id) > 0,
        "**LIVENESS**: the herd with room is worked: {crews:?}"
    );
    assert!(
        crew_of(&thin_id) <= A_SLIVERS_HANDS && crew_of(&thin_id) <= crew_of(&full_id),
        "a herd with a sliver of room is not dealt the regrowth's worth of bare hands: {crews:?}"
    );
}
