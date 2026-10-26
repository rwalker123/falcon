//! **THE COMPOSE SHEET'S DRY RUN OF THE REAL STAFFING** (`.claude/rules/core_sim/work-party.md` →
//! "The staffing preview").
//!
//! A band camped on the harness map's `(1, 1)` holds two hunt rows — one Normal, one Low — and the
//! question is what a THIRD hunt row would be given. The oracle is the command itself: the same
//! row written onto the band and restaffed through `restaff_band` is the crew the preview must
//! quote, exactly.
//!
//! The seat refusal rides the real socket in `query_seat_gate.rs`
//! (`a_question_about_another_seats_faction_is_refused_over_the_socket`), because the gate lives in
//! the server binary's dispatch.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use core_sim::shipped;
use core_sim::{
    build_test_app, scalar_from_f32, scalar_one, scalar_zero, BandEquipment, BandId,
    EquipmentConfig, FactionId, FaunaConfigHandle, GenerationId, Herd, HerdRegistry,
    LaborAllocation, LaborAssignment, LaborTarget, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, SizeClass, SourcePriority, TileRegistry,
};
use sim_runtime::commands::{
    query_error, QueryPayload, QueryReply, StaffingPreviewQuery, StaffingPreviewReply,
    WorkPartySource,
};

const BOAR: &str = "Wild Boar";
const BAND: u64 = 41;
const FACTION: FactionId = FactionId(0);
/// Where the band camps - a tile the harness map carries.
const CAMP: UVec2 = UVec2::new(1, 1);
/// The band's working hands.
const HANDS: u32 = 6;
const FLOOR: f32 = 0.5;
/// A herd big enough that a crew's take is never what runs out.
const STANDING_STOCK: f32 = 5_000.0;
/// The three herds, each on a tile of its own inside the work range so no row posts a party.
const NORMAL_HERD: (&str, UVec2) = ("normal_herd", UVec2::new(2, 1));
const LOW_HERD: (&str, UVec2) = ("low_herd", UVec2::new(1, 2));
const NEW_HERD: (&str, UVec2) = ("new_herd", UVec2::new(3, 1));
/// The standing rows' Max: together they use every hand, so the band has none idle.
const NORMAL_MAX: u32 = 2;
const LOW_MAX: u32 = HANDS - NORMAL_MAX;
/// A Max small enough that idle hands cover it.
const SMALL_MAX: u32 = 3;

fn herd(id: &str, at: UVec2, app: &App) -> Herd {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let def = fauna
        .species_by_display(BOAR)
        .expect("the fixture names a shipped species");
    Herd::new(
        id.to_string(),
        BOAR.to_string(),
        SizeClass::Big,
        vec![at],
        STANDING_STOCK,
        STANDING_STOCK,
        def.fodder_per_biomass,
        def.regrowth_rate.unwrap_or(0.1),
        def.body_mass,
        shipped::root_standing(shipped::animal()),
    )
}

fn hunt(id: &str) -> LaborTarget {
    LaborTarget::Hunt {
        fauna_id: id.to_string(),
        floor: FLOOR,
        move_with_herd: false,
    }
}

fn row(id: &str, priority: SourcePriority, max: u32) -> LaborAssignment {
    LaborAssignment {
        party: None,
        muster_crew: 0,
        planned_crew: 0,
        max_workers: Some(max),
        staffing: Default::default(),
        target: hunt(id),
        workers: 0,
        kit: None,
        priority,
    }
}

/// The band, camped on [`CAMP`], holding a Normal row on [`NORMAL_HERD`] and a Low row on
/// [`LOW_HERD`], with the standing rows already staffed the way the command path leaves them.
fn world_with(rows: Vec<LaborAssignment>) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    let herds: Vec<Herd> = [NORMAL_HERD, LOW_HERD, NEW_HERD]
        .iter()
        .map(|(id, at)| herd(id, *at, &app))
        .collect();
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.clear();
        registry.herds.extend(herds);
    }
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(CAMP.x, CAMP.y)
        .expect("the harness map carries the camp tile");
    let band = app
        .world
        .spawn((
            ResidentBand,
            BandId(BAND),
            BandEquipment::start_stocked_for(
                &EquipmentConfig::for_a_stocked_fixture(),
                HANDS as f32,
            ),
            PopulationCohort {
                home: camp,
                current_tile: camp,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(HANDS as f32),
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
                last_fertility_factors: Default::default(),
                last_breeding: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FACTION,
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
            },
            LaborAllocation {
                assignments: rows,
                ..Default::default()
            },
        ))
        .id();
    core_sim::staffing::restaff_band(&mut app.world, band);
    (app, band)
}

fn two_standing_rows() -> (App, Entity) {
    world_with(vec![
        row(NORMAL_HERD.0, SourcePriority::Normal, NORMAL_MAX),
        row(LOW_HERD.0, SourcePriority::Low, LOW_MAX),
    ])
}

fn crew_on(app: &App, band: Entity, id: &str) -> u32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .workers_on(&hunt(id))
}

fn ask_for(priority: &str, max_workers: u32, faction: u32) -> StaffingPreviewQuery {
    StaffingPreviewQuery {
        faction_id: faction,
        band_id: BAND,
        source: WorkPartySource::Hunt {
            herd_id: NEW_HERD.0.to_string(),
        },
        kit_id: String::new(),
        floor: FLOOR,
        priority: priority.to_string(),
        max_workers,
    }
}

fn preview(app: &mut App, ask: StaffingPreviewQuery) -> QueryReply {
    core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &QueryPayload::StaffingPreview(ask),
    )
}

fn preview_reply(app: &mut App, ask: StaffingPreviewQuery) -> StaffingPreviewReply {
    match preview(app, ask) {
        QueryReply::StaffingPreview(reply) => reply,
        other => panic!("the preview must answer with a crew, got {other:?}"),
    }
}

/// **What `assign_labor` does to the band for this row**, minus the socket: write the row (its Max,
/// its kit resolved the way the command resolves it, its Priority) and restaff.
fn send_the_matching_assign_labor(app: &mut App, band: Entity, priority: SourcePriority, max: u32) {
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        let target = hunt(NEW_HERD.0);
        allocation.set_staffed_row(target.clone(), (max > 0).then_some(max), None);
        allocation.set_source_priority(&target, priority);
    }
    core_sim::staffing::restaff_band(&mut app.world, band);
}

#[test]
fn a_new_normal_row_takes_from_the_low_row_and_quotes_what_the_command_gives() {
    let (mut app, band) = two_standing_rows();
    // Liveness: both standing rows hold hands and the band has none idle, so the question is real.
    assert_eq!(crew_on(&app, band, NORMAL_HERD.0), NORMAL_MAX);
    assert_eq!(crew_on(&app, band, LOW_HERD.0), LOW_MAX);

    let reply = preview_reply(&mut app, ask_for("normal", 0, FACTION.0));
    assert!(reply.workers > 0, "liveness: the new row is given a crew");
    assert_eq!(
        reply.keepers + reply.kitted + reply.bare,
        reply.workers,
        "the breakdown sums to the crew"
    );
    assert_eq!(reply.idle_before, 0);
    assert_eq!(
        reply.losses.len(),
        1,
        "only the Low row loses hands: {:?}",
        reply.losses
    );
    let loss = &reply.losses[0];
    assert_eq!(loss.kind, "hunt");
    assert_eq!(loss.herd_id, LOW_HERD.0);
    assert_eq!(
        (loss.before, loss.after),
        (LOW_MAX, LOW_MAX - reply.workers)
    );

    send_the_matching_assign_labor(&mut app, band, SourcePriority::Normal, 0);
    assert_eq!(
        reply.workers,
        crew_on(&app, band, NEW_HERD.0),
        "the preview IS the command's restaff"
    );
    assert_eq!(loss.after, crew_on(&app, band, LOW_HERD.0));
    assert_eq!(
        crew_on(&app, band, NORMAL_HERD.0),
        NORMAL_MAX,
        "the Normal row keeps its hands"
    );
}

#[test]
fn a_new_low_row_leaves_the_normal_row_alone() {
    let (mut app, band) = two_standing_rows();
    let reply = preview_reply(&mut app, ask_for("low", 0, FACTION.0));
    assert!(
        reply
            .losses
            .iter()
            .all(|loss| loss.herd_id != NORMAL_HERD.0),
        "a Low row never takes from a Normal one: {:?}",
        reply.losses
    );
    send_the_matching_assign_labor(&mut app, band, SourcePriority::Low, 0);
    assert_eq!(reply.workers, crew_on(&app, band, NEW_HERD.0));
    assert_eq!(crew_on(&app, band, NORMAL_HERD.0), NORMAL_MAX);
}

#[test]
fn idle_hands_are_taken_first_and_cost_no_row() {
    // Only the Normal row stands, capped well under the band: the rest are idle.
    let (mut app, band) = world_with(vec![row(NORMAL_HERD.0, SourcePriority::Normal, NORMAL_MAX)]);
    let idle = HANDS - NORMAL_MAX;
    let reply = preview_reply(&mut app, ask_for("normal", SMALL_MAX, FACTION.0));
    assert_eq!(reply.idle_before, idle, "liveness: the band has idle hands");
    assert!(reply.workers > 0, "the new row is given a crew");
    assert!(reply.workers <= SMALL_MAX, "the Max bounds the crew");
    assert!(
        reply.losses.is_empty(),
        "idle hands cover it: {:?}",
        reply.losses
    );
    assert_eq!(reply.idle_after, idle - reply.workers);
    send_the_matching_assign_labor(&mut app, band, SourcePriority::Normal, SMALL_MAX);
    assert_eq!(reply.workers, crew_on(&app, band, NEW_HERD.0));
}

#[test]
fn the_query_writes_nothing_to_the_world() {
    let (mut app, band) = two_standing_rows();
    let before = app.world.get::<LaborAllocation>(band).cloned().unwrap();
    let reply = preview_reply(&mut app, ask_for("high", 0, FACTION.0));
    assert!(
        reply.workers > 0 && !reply.losses.is_empty(),
        "liveness: the preview did real work"
    );
    let after = app.world.get::<LaborAllocation>(band).cloned().unwrap();
    assert_eq!(before, after, "the intent is untouched");
    assert_eq!(
        before.assignments.len(),
        after.assignments.len(),
        "no row was added"
    );
    for (was, now) in before.assignments.iter().zip(&after.assignments) {
        assert_eq!(
            (was.workers, was.staffing, was.priority, was.max_workers),
            (now.workers, now.staffing, now.priority, now.max_workers),
            "a standing row's staffed crew is untouched"
        );
    }
}

#[test]
fn a_question_about_another_factions_band_or_a_bad_ask_is_refused_with_a_token() {
    let (mut app, _band) = two_standing_rows();
    let token = |app: &mut App, ask: StaffingPreviewQuery| match preview(app, ask) {
        QueryReply::Error(token) => token,
        other => panic!("expected a refusal, got {other:?}"),
    };
    // A band that is not that faction's: the sim cannot find it (the seat gate in the server binary
    // refuses a cross-seat ask before it gets here - query_seat_gate.rs).
    assert_eq!(
        token(&mut app, ask_for("normal", 0, FACTION.0 + 1)),
        query_error::UNKNOWN_BAND
    );
    let mut bad_floor = ask_for("normal", 0, FACTION.0);
    bad_floor.floor = 1.5;
    assert_eq!(token(&mut app, bad_floor), query_error::INVALID_FLOOR);
    assert_eq!(
        token(&mut app, ask_for("urgent", 0, FACTION.0)),
        query_error::INVALID_PRIORITY
    );
    let mut no_herd = ask_for("normal", 0, FACTION.0);
    no_herd.source = WorkPartySource::Hunt {
        herd_id: "nobody".to_string(),
    };
    assert_eq!(token(&mut app, no_herd), query_error::UNKNOWN_HERD);
    let mut no_patch = ask_for("normal", 0, FACTION.0);
    no_patch.source = WorkPartySource::Forage {
        x: 0,
        y: 0,
        take_species: Vec::new(),
    };
    assert_eq!(token(&mut app, no_patch), query_error::UNKNOWN_PATCH);
    let mut no_deposit = ask_for("normal", 0, FACTION.0);
    no_deposit.source = WorkPartySource::Extract {
        x: 0,
        y: 0,
        material: "nothing".to_string(),
    };
    assert_eq!(token(&mut app, no_deposit), query_error::UNKNOWN_DEPOSIT);
    let mut bad_kit = ask_for("normal", 0, FACTION.0);
    bad_kit.kit_id = "no_such_kit".to_string();
    assert_eq!(token(&mut app, bad_kit), query_error::UNKNOWN_KIT);
}
