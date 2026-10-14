//! **THE MOBILITY SCORE** (#800, `docs/plan_roaming_bands.md` §"The mobility score",
//! `.claude/rules/core_sim/campaign.md` -> "Mobility").
//!
//! The roaming counterpart of `SedentarizationScore`: a per-faction 0-100 readout, an EMA-smoothed
//! blend of the roaming skills a people has learned and the hexes its bands walked this turn
//! beside a migrating herd. These tests drive the turn's real movement systems in the Population
//! chain's order (the harness `roaming_skills` uses), then `mobility_tick`, and read the resource
//! and the ENCODED snapshot back. Every claim is paired with a control so a dead input cannot pass.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::roaming_skills::{DRIVES_DISCOVERY_ID, HERD_LORE_DISCOVERY_ID, TRAVOIS_DISCOVERY_ID};
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_band_movement, advance_herds, build_test_app, follow_hunted_herds, mobility_tick,
    publish_baseline_snapshot, scalar_from_f32, scalar_one, scalar_zero, BandEquipment, BandId,
    DiscoveryProgressLedger, EquipmentConfig, FactionId, FaunaConfigHandle, GenerationId, Herd,
    HerdFollowSteps, HerdRegistry, LaborAllocation, LaborAssignment, LaborTarget,
    LadderConfigHandle, LocalStore, MobilityConfigHandle, MobilityScore, MoraleCause,
    PopulationCohort, ResidentBand, RoamState, SimulationTick, SizeClass, SnapshotHistory,
    SourcePriority, TileRegistry,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

const MIGRATORY_SPECIES: &str = "Wild Reindeer";
const HERD_ID: &str = "herd_probe";
const HOME: FactionId = FactionId(0);
const RIVAL: FactionId = FactionId(1);
const HOME_BAND: u64 = 23;
const RIVAL_BAND: u64 = 24;
const BAND_SIZE: u32 = 30;
const CREW: u32 = 6;
const FLOOR: f32 = 0.5;
const REGROWTH_RATE: f32 = 0.04;
const BODY_MASS: f32 = 20.0;
const STANDING_STOCK: f32 = 5_000.0;
const ROW: u32 = 17;
const LEG_START: UVec2 = UVec2::new(24, ROW);
const LEG_END: UVec2 = UVec2::new(34, ROW);
const WALKING_TURNS: usize = 4;
const LOITER_WINDOW: u32 = 40;
const FAT_LARDER: f32 = 100_000.0;
const MEAT_CLASS: &str = "flesh";
const HALF: f32 = 0.5;
const THIRD: f32 = 1.0 / 3.0;
const TOLERANCE: f32 = 1e-3;
/// The score scale the points are struck on (the sim's `RAW_SCALE`).
const SCALE: f32 = 100.0;

/// What a band is doing in the fixture.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// Hunts the herd with `move_with_herd` on.
    Follows,
    /// Stands there with no hunt row at all.
    Idle,
}

struct Placed {
    faction: FactionId,
    id: u64,
    at: UVec2,
    role: Role,
}

/// A world with one herd and exactly the bands named: the worldgen opening bands are despawned so
/// the population weighting is the fixture's, not the start profile's.
fn world(roam: RoamState, herd_at: UVec2, bands: &[Placed]) -> (App, Vec<Entity>) {
    let mut app = build_test_app();
    app.update();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    let opening: Vec<Entity> = app
        .world
        .query_filtered::<Entity, bevy::prelude::With<PopulationCohort>>()
        .iter(&app.world)
        .collect();
    for entity in opening {
        app.world.despawn(entity);
    }
    let herd = {
        let mut herd = Herd::new(
            HERD_ID.to_string(),
            MIGRATORY_SPECIES.to_string(),
            SizeClass::Migratory,
            vec![herd_at, LEG_END],
            STANDING_STOCK,
            STANDING_STOCK,
            0.0,
            REGROWTH_RATE,
            BODY_MASS,
        );
        herd.roam = roam;
        herd.dwell_remaining = 0;
        herd
    };
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.clear();
        registry.herds.push(herd);
    }
    let entities = bands.iter().map(|b| spawn_band(&mut app, b)).collect();
    (app, entities)
}

fn spawn_band(app: &mut App, placed: &Placed) -> Entity {
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(placed.at.x, placed.at.y)
        .expect("the harness map carries the camp tile");
    let mut stores = LocalStore::new();
    stores.add_food(MEAT_CLASS, scalar_from_f32(FAT_LARDER));
    let assignments = match placed.role {
        Role::Follows => vec![LaborAssignment {
            party: None,
            muster_crew: 0,
            planned_crew: 0,
            target: LaborTarget::Hunt {
                fauna_id: HERD_ID.to_string(),
                floor: FLOOR,
                move_with_herd: true,
            },
            workers: CREW,
            kit: None,
            priority: SourcePriority::default(),
        }],
        Role::Idle => Vec::new(),
    };
    app.world
        .spawn((
            ResidentBand,
            BandId(placed.id),
            BandEquipment::start_stocked_for(
                &EquipmentConfig::for_a_stocked_fixture(),
                CREW as f32,
            ),
            PopulationCohort {
                home: camp,
                current_tile: camp,
                size: BAND_SIZE,
                children: scalar_zero(),
                working: scalar_from_f32(CREW as f32),
                elders: scalar_zero(),
                stores,
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                last_food_spoiled: 0.0,
                fed_party_workers: Default::default(),
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
                faction: placed.faction,
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
            },
            LaborAllocation {
                assignments,
                ..Default::default()
            },
        ))
        .id()
}

/// One turn of the Population chain's movement half, then the mobility tick.
fn turn(app: &mut App) {
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(follow_hunted_herds);
    app.world.run_system_once(advance_band_movement);
    app.world.run_system_once(mobility_tick);
}

fn home_follower() -> Placed {
    Placed {
        faction: HOME,
        id: HOME_BAND,
        at: LEG_START,
        role: Role::Follows,
    }
}

fn entry(app: &App, faction: FactionId) -> core_sim::MobilityEntry {
    *app.world
        .resource::<MobilityScore>()
        .entry(faction)
        .expect("a faction with a resident band is scored")
}

fn app_config(app: &App) -> std::sync::Arc<core_sim::MobilityConfig> {
    app.world.resource::<MobilityConfigHandle>().get()
}

fn teach(app: &mut App, faction: FactionId, id: u32, progress: f32) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, id, scalar_from_f32(progress));
}

// ---- Distance ------------------------------------------------------------------------------------

/// **WALKING BESIDE A MIGRATING HERD RAISES THE DISTANCE POINTS; A LOITERING HERD DOES NOT.**
/// Paired, so a dead input cannot pass: the migrating arm is non-zero, the control is exactly zero.
#[test]
fn a_band_following_a_migrating_herd_earns_distance_points_and_one_beside_a_loitering_herd_does_not(
) {
    let (mut walking, _) = world(RoamState::Migrate, LEG_START, &[home_follower()]);
    for _ in 0..WALKING_TURNS {
        turn(&mut walking);
    }
    let cfg = app_config(&walking);
    let full = SCALE * cfg.weights.distance;
    let walked = entry(&walking, HOME);
    assert!(
        walked.distance_points > 0.0,
        "liveness: the band walked with the herd and earned distance points"
    );
    assert!(
        (walked.distance_points - full).abs() < TOLERANCE,
        "a band stepping the reference pace every turn saturates the input: {} vs {full}",
        walked.distance_points
    );
    assert!(walked.score > 0.0);
    assert!(
        walking.world.resource::<HerdFollowSteps>().is_empty(),
        "the per-turn step record is drained by the tick"
    );

    // Control: the same band standing in a loitering herd walks nowhere.
    let (mut loitering, _) = world(
        RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        },
        LEG_START,
        &[home_follower()],
    );
    for _ in 0..WALKING_TURNS {
        turn(&mut loitering);
    }
    let still = entry(&loitering, HOME);
    assert_eq!(still.distance_points, 0.0);
    assert_eq!(still.score, 0.0);
}

/// **A BAND NOT FOLLOWING THE HERD EARNS NOTHING** even beside a migrating one.
#[test]
fn a_band_that_is_not_following_the_migrating_herd_earns_no_distance_points() {
    let idle = Placed {
        role: Role::Idle,
        ..home_follower()
    };
    let (mut app, _) = world(RoamState::Migrate, LEG_START, &[idle]);
    for _ in 0..WALKING_TURNS {
        turn(&mut app);
    }
    assert_eq!(entry(&app, HOME).distance_points, 0.0);
}

/// **THE DISTANCE INPUT IS POPULATION-WEIGHTED.** A same-sized band that stays put halves the
/// faction's distance points.
#[test]
fn the_distance_input_is_weighted_by_population() {
    let stayer = Placed {
        faction: HOME,
        id: RIVAL_BAND,
        at: LEG_START,
        role: Role::Idle,
    };
    let (mut app, _) = world(RoamState::Migrate, LEG_START, &[home_follower(), stayer]);
    for _ in 0..WALKING_TURNS {
        turn(&mut app);
    }
    let full = SCALE * app_config(&app).weights.distance;
    let points = entry(&app, HOME).distance_points;
    assert!(points > 0.0, "liveness");
    assert!(
        (points - full * HALF).abs() < TOLERANCE,
        "one of two equal bands walking earns half: {points} vs {}",
        full * HALF
    );
}

// ---- Skills --------------------------------------------------------------------------------------

/// **SKILLS PROGRESS RAISES THE SKILLS POINTS PROPORTIONALLY, PARTIAL PROGRESS COUNTS** and a
/// people that knows all three earns the whole skills weight.
#[test]
fn skills_progress_raises_skills_points_in_proportion() {
    let idle = Placed {
        role: Role::Idle,
        ..home_follower()
    };
    let (mut app, _) = world(RoamState::Migrate, LEG_START, &[idle]);
    let full = SCALE * app_config(&app).weights.skills;

    turn(&mut app);
    assert_eq!(
        entry(&app, HOME).skills_points,
        0.0,
        "control: nothing learned"
    );

    teach(&mut app, HOME, HERD_LORE_DISCOVERY_ID, HALF);
    turn(&mut app);
    let partial = entry(&app, HOME).skills_points;
    assert!(
        (partial - full * HALF * THIRD).abs() < TOLERANCE,
        "half of one skill of three: {partial}"
    );

    teach(&mut app, HOME, HERD_LORE_DISCOVERY_ID, HALF);
    turn(&mut app);
    let one = entry(&app, HOME).skills_points;
    assert!(
        (one - full * THIRD).abs() < TOLERANCE,
        "one skill known of three: {one}"
    );

    teach(&mut app, HOME, TRAVOIS_DISCOVERY_ID, 1.0);
    teach(&mut app, HOME, DRIVES_DISCOVERY_ID, 1.0);
    turn(&mut app);
    let all = entry(&app, HOME).skills_points;
    assert!((all - full).abs() < TOLERANCE, "all three known: {all}");
}

/// **A SKILL IS CAPPED AT KNOWN.** With the ladder's completion bar lowered, progress past the bar
/// earns no more than a known skill does.
#[test]
fn progress_past_the_completion_bar_is_capped_at_known() {
    let idle = Placed {
        role: Role::Idle,
        ..home_follower()
    };
    let (mut app, _) = world(RoamState::Migrate, LEG_START, &[idle]);
    let mut ladder = (*app.world.resource::<LadderConfigHandle>().get()).clone();
    let bar = HALF;
    ladder.knowledge.completion_threshold = bar;
    app.world
        .resource_mut::<LadderConfigHandle>()
        .replace(std::sync::Arc::new(ladder));
    let full = SCALE * app_config(&app).weights.skills;

    teach(&mut app, HOME, HERD_LORE_DISCOVERY_ID, bar);
    turn(&mut app);
    let at_bar = entry(&app, HOME).skills_points;
    teach(&mut app, HOME, HERD_LORE_DISCOVERY_ID, 1.0 - bar);
    turn(&mut app);
    let past_bar = entry(&app, HOME).skills_points;
    assert!(
        (at_bar - full * THIRD).abs() < TOLERANCE,
        "liveness: {at_bar}"
    );
    assert_eq!(at_bar, past_bar, "progress past the bar earns nothing more");
}

// ---- Smoothing -----------------------------------------------------------------------------------

/// **THE EMA REMEMBERS THE LEGS**: the first reading is `(1 - smoothing) x raw`, and once the
/// walking stops the score decays by `smoothing` a turn rather than dropping to zero.
#[test]
fn the_score_decays_when_walking_stops() {
    let (mut app, _) = world(RoamState::Migrate, LEG_START, &[home_follower()]);
    let cfg = app_config(&app);
    let smoothing = cfg.smoothing;

    turn(&mut app);
    let first = entry(&app, HOME);
    // The walk also teaches herd lore, so the raw reading is both inputs' points.
    let raw = first.distance_points + first.skills_points;
    assert!(first.skills_points > 0.0 && first.distance_points > 0.0);
    assert!(
        (first.score - (1.0 - smoothing) * raw).abs() < TOLERANCE,
        "a faction's first reading starts from zero: {} vs {}",
        first.score,
        (1.0 - smoothing) * raw
    );
    for _ in 0..WALKING_TURNS {
        turn(&mut app);
    }
    let walking = entry(&app, HOME).score;
    assert!(
        walking > first.score,
        "liveness: the score builds while walking"
    );

    // The herd settles into a loiter: the band is in it and walks no further.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.herds[0].roam = RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        };
    }
    turn(&mut app);
    let stopped = entry(&app, HOME);
    assert_eq!(stopped.distance_points, 0.0, "the raw reading fell to zero");
    // Only the learned skills still feed the raw reading; the remembered legs decay by `smoothing`.
    let expected = smoothing * walking + (1.0 - smoothing) * stopped.skills_points;
    assert!(
        (stopped.score - expected).abs() < TOLERANCE,
        "the score decays by the smoothing factor, not to zero: {} vs {expected}",
        stopped.score
    );
    assert!(stopped.score > 0.0 && stopped.score < walking);
}

// ---- Per faction ---------------------------------------------------------------------------------

/// **A RIVAL'S WALKING DOES NOT MOVE YOUR SCORE.** The rival follows the herd and learns the skills;
/// the home band stands still.
#[test]
fn a_rivals_walking_does_not_move_your_score() {
    let home_idle = Placed {
        role: Role::Idle,
        ..home_follower()
    };
    let rival = Placed {
        faction: RIVAL,
        id: RIVAL_BAND,
        at: LEG_START,
        role: Role::Follows,
    };
    let (mut app, _) = world(RoamState::Migrate, LEG_START, &[home_idle, rival]);
    teach(&mut app, RIVAL, HERD_LORE_DISCOVERY_ID, 1.0);
    for _ in 0..WALKING_TURNS {
        turn(&mut app);
    }
    let rival_entry = entry(&app, RIVAL);
    assert!(
        rival_entry.score > 0.0
            && rival_entry.distance_points > 0.0
            && rival_entry.skills_points > 0.0,
        "liveness: the rival walked and learned"
    );
    let home = entry(&app, HOME);
    assert_eq!(home.score, 0.0);
    assert_eq!(home.distance_points, 0.0);
    assert_eq!(home.skills_points, 0.0);
}

// ---- Wire ----------------------------------------------------------------------------------------

/// **THE PUBLISHED ROWS ARE THE RESOURCE'S, FOR THE VIEWER ALONE.** Read off the ENCODED envelope:
/// the home row equals the resource, and the rival's row (live in the resource) is not in the frame.
#[test]
fn the_published_mobility_row_is_the_viewers_and_matches_the_resource() {
    let rival = Placed {
        faction: RIVAL,
        id: RIVAL_BAND,
        at: LEG_START,
        role: Role::Idle,
    };
    let (mut app, _) = world(RoamState::Migrate, LEG_START, &[home_follower(), rival]);
    teach(&mut app, HOME, HERD_LORE_DISCOVERY_ID, HALF);
    teach(&mut app, RIVAL, DRIVES_DISCOVERY_ID, 1.0);
    for _ in 0..WALKING_TURNS {
        turn(&mut app);
    }
    let home = entry(&app, HOME);
    let rival_entry = entry(&app, RIVAL);
    assert!(
        home.score > 0.0 && rival_entry.score > 0.0,
        "liveness: both peoples are scored"
    );

    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows: Vec<_> = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .subsistence()
        .and_then(|section| section.mobility())
        .expect("the mobility rows are published")
        .iter()
        .collect();
    assert_eq!(rows.len(), 1, "only the viewer's row rides the frame");
    let row = rows[0];
    assert_eq!(row.faction(), HOME.0);
    assert_eq!(row.score(), home.score);
    assert_eq!(row.skillsPoints(), home.skills_points);
    assert_eq!(row.distancePoints(), home.distance_points);
    assert!(
        rows.iter().all(|r| r.faction() != RIVAL.0),
        "a rival's row is not in the viewer's frame"
    );
}

// ---- Checkpoint ----------------------------------------------------------------------------------

/// **THE SCORE IS CHECKPOINT STATE**: a restore puts back the EMA the walking built.
#[test]
fn the_score_round_trips_the_checkpoint() {
    let (mut app, _) = world(RoamState::Migrate, LEG_START, &[home_follower()]);
    for _ in 0..WALKING_TURNS {
        turn(&mut app);
    }
    let before = entry(&app, HOME);
    assert!(before.score > 0.0, "liveness");
    let state = capture_sim_state(&app.world);
    app.world.insert_resource(MobilityScore::default());
    restore_sim_state(&mut app.world, &state);
    let after = entry(&app, HOME);
    assert_eq!(after.score, before.score);
    assert_eq!(after.distance_points, before.distance_points);
}
