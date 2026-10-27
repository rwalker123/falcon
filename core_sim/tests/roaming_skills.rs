//! **THE ROAMING SKILLS — HERD LORE, TRAVOIS, DRIVES** (#799,
//! `docs/plan_roaming_bands.md` §"The roaming skills — decided", `.claude/rules/core_sim/work-party.md`).
//!
//! Each skill is a faction knowledge with its own practice signal. These tests drive the turn's real
//! systems in the Population chain's order (the harness `migration_mode` uses) and read the
//! discovery ledger and the ENCODED snapshot back: the signal credits the ledger, does not credit
//! when its condition is off, and the effect applies only to a faction that knows the lesson.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use core_sim::shipped;

use core_sim::roaming_skills::{DRIVES_DISCOVERY_ID, HERD_LORE_DISCOVERY_ID, TRAVOIS_DISCOVERY_ID};
use core_sim::{
    advance_band_movement, advance_herds, advance_labor_allocation, build_test_app,
    follow_hunted_herds, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    BandEquipment, BandId, BandTravel, DiscoveryProgressLedger, EquipmentConfig, FactionId,
    FaunaConfigHandle, GenerationId, Herd, HerdRegistry, LaborAllocation, LaborAssignment,
    LaborTarget, LadderConfig, LocalStore, MoraleCause, PopulationCohort, ResidentBand, RoamState,
    SimulationConfig, SimulationTick, SizeClass, SnapshotHistory, SourcePriority, Tile,
    TileRegistry,
};

const MIGRATORY_SPECIES: &str = "Wild Reindeer";
const HERD_ID: &str = "herd_probe";
const BAND: u64 = 23;
const FACTION: FactionId = FactionId(0);
const CREW: u32 = 6;
const FLOOR: f32 = 0.5;
const FODDER_PER_BIOMASS: f32 = 0.0;
const REGROWTH_RATE: f32 = 0.04;
const BODY_MASS: f32 = 20.0;
const STANDING_STOCK: f32 = 5_000.0;
const ROW: u32 = 17;
const LEG_START: UVec2 = UVec2::new(24, ROW);
const LEG_END: UVec2 = UVec2::new(34, ROW);
/// A third anchor, so the "next" ground of a herd mid-route is told apart from "the other end".
const FAR_ANCHOR: UVec2 = UVec2::new(40, ROW);
const TURNS: usize = 5;
const LOITER_WINDOW: u32 = 40;
const FAT_LARDER: f32 = 100_000.0;
/// A larder the packs hold whole: a long move sheds nothing.
const LIGHT_LARDER: f32 = 5.0;
const MEAT_CLASS: &str = "flesh";
/// The band starts this many hexes west of the herd — past the ferry reach.
const GAP: u32 = 6;
/// The shipped `lesson_costs` entry for the three skills and `learn_rate`: one practice unit is
/// `1 / LESSON_COST` of the ledger bar.
const LESSON_COST: f32 = 20.0;
const TOLERANCE: f32 = 1e-4;

fn world(roam: RoamState, herd_at: UVec2, band_at: UVec2, follow: bool) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    let herd = {
        let mut herd = Herd::new(
            HERD_ID.to_string(),
            MIGRATORY_SPECIES.to_string(),
            SizeClass::Migratory,
            vec![herd_at, LEG_END],
            STANDING_STOCK,
            STANDING_STOCK,
            FODDER_PER_BIOMASS,
            REGROWTH_RATE,
            BODY_MASS,
            shipped::root_standing(shipped::animal()),
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
    let band = spawn_band(&mut app, band_at, follow);
    (app, band)
}

fn spawn_band(app: &mut App, at: UVec2, follow: bool) -> Entity {
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .expect("the harness map carries the camp tile");
    let mut stores = LocalStore::new();
    stores.add_food(MEAT_CLASS, scalar_from_f32(FAT_LARDER));
    app.world
        .spawn((
            ResidentBand,
            BandId(BAND),
            BandEquipment::start_stocked_for(
                &EquipmentConfig::for_a_stocked_fixture(),
                CREW as f32,
            ),
            PopulationCohort {
                home: camp,
                current_tile: camp,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(CREW as f32),
                elders: scalar_zero(),
                stores,
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                last_food_spoiled: 0.0,
                last_food_into_store: 0.0,
                last_food_from_store: 0.0,
                last_food_kept: 0.0,
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
                faction: FACTION,
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
                    max_workers: Some(CREW),
                    staffing: Default::default(),
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: FLOOR,
                        move_with_herd: follow,
                    },
                    workers: CREW,
                    kit: None,
                    priority: SourcePriority::default(),
                }],
                ..Default::default()
            },
        ))
        .id()
}

fn turn(app: &mut App) {
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(follow_hunted_herds);
    app.world.run_system_once(advance_band_movement);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

fn band_tile(app: &App, band: Entity) -> UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .current_tile;
    app.world
        .get::<Tile>(tile)
        .expect("the band stands on a tile")
        .position
}

fn progress(app: &App, id: u32) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FACTION, id)
        .to_f32()
}

fn teach(app: &mut App, id: u32) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FACTION, id, scalar_one());
}

fn practice_units(progress: f32) -> f32 {
    progress * LESSON_COST
}

// ---- Herd lore ----------------------------------------------------------------------------------

/// **EACH HEX WALKED BESIDE A MIGRATING HERD IS PRACTICE.**
#[test]
fn walking_beside_a_migrating_herd_teaches_herd_lore_a_hex_at_a_time() {
    let (mut app, band) = world(RoamState::Migrate, LEG_START, LEG_START, true);
    let mut hexes = 0;
    for _ in 0..TURNS {
        let before = band_tile(&app, band);
        turn(&mut app);
        hexes += core_sim::grid_utils::hex_distance_wrapped(
            before,
            band_tile(&app, band),
            app.world.resource::<TileRegistry>().width,
            app.world
                .resource::<SimulationConfig>()
                .map_topology
                .wrap_horizontal,
        );
    }
    assert!(hexes > 0, "liveness: the band walked with the herd");
    let learned = practice_units(progress(&app, HERD_LORE_DISCOVERY_ID));
    assert!(
        (learned - hexes as f32).abs() < TOLERANCE * LESSON_COST,
        "one learn_rate of practice per hex stepped: {hexes} hexes, learned {learned}"
    );
}

/// **A HERD THAT LOITERS TEACHES NOTHING** — the band walks a long way to reach it and learns no
/// lore from the walk.
#[test]
fn following_a_loitering_herd_teaches_no_herd_lore() {
    let band_start = UVec2::new(LEG_END.x - GAP, ROW);
    let (mut app, band) = world(
        RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        },
        LEG_END,
        band_start,
        true,
    );
    for _ in 0..3 {
        turn(&mut app);
    }
    assert!(
        band_tile(&app, band).x > band_start.x,
        "liveness: the band really walked toward the herd"
    );
    assert_eq!(progress(&app, HERD_LORE_DISCOVERY_ID), 0.0);
}

/// **THE BAND MUST BE FOLLOWING THE HERD.** A band under a plain move order beside a migrating herd
/// is just walking.
#[test]
fn a_band_that_is_not_following_the_herd_learns_no_herd_lore() {
    let (mut app, band) = world(RoamState::Migrate, LEG_START, LEG_START, false);
    app.world
        .entity_mut(band)
        .insert(BandTravel::to(UVec2::new(LEG_START.x + 3, ROW)));
    for _ in 0..3 {
        turn(&mut app);
    }
    assert_ne!(
        band_tile(&app, band),
        LEG_START,
        "liveness: the band really moved"
    );
    assert_eq!(progress(&app, HERD_LORE_DISCOVERY_ID), 0.0);
}

// ---- Travois ------------------------------------------------------------------------------------

fn far_start_world(larder: f32) -> (App, Entity) {
    far_start_world_with(larder, false)
}

/// `bare`: the band owns no gear either, so its whole load is the larder.
fn far_start_world_with(larder: f32, bare: bool) -> (App, Entity) {
    let band_start = UVec2::new(LEG_END.x - GAP, ROW);
    let (mut app, band) = world(
        RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        },
        LEG_END,
        band_start,
        true,
    );
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band keeps its cohort");
    cohort.stores = LocalStore::new();
    cohort.stores.add_food(MEAT_CLASS, scalar_from_f32(larder));
    if bare {
        *app.world
            .get_mut::<BandEquipment>(band)
            .expect("the band keeps its ledger") = BandEquipment::default();
    }
    (app, band)
}

fn food_left_behind(app: &App, band: Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_food_left_behind
}

/// **WHAT A LONG MOVE SHEDS TEACHES TRAVOIS**, in proportion to the share of the load shed, once per
/// departure.
#[test]
fn a_departure_that_sheds_teaches_travois_once() {
    let (mut app, _) = far_start_world(FAT_LARDER);
    turn(&mut app);
    let after_departure = progress(&app, TRAVOIS_DISCOVERY_ID);
    assert!(after_departure > 0.0, "the shed taught travois");
    assert!(
        practice_units(after_departure) <= 1.0 + TOLERANCE,
        "a departure pays at most one learn_rate of practice (share of the load shed, clamped)"
    );
    turn(&mut app);
    assert_eq!(
        progress(&app, TRAVOIS_DISCOVERY_ID),
        after_departure,
        "the standing order keeps `departed`: the walk after the departure sheds and teaches nothing"
    );
}

/// **A BAND THAT NEVER OVERLOADS NEVER LEARNS IT.**
#[test]
fn a_long_move_that_sheds_nothing_teaches_no_travois() {
    let (mut app, band) = far_start_world_with(LIGHT_LARDER, true);
    turn(&mut app);
    assert_eq!(food_left_behind(&app, band), 0.0, "fixture: nothing shed");
    assert_eq!(progress(&app, TRAVOIS_DISCOVERY_ID), 0.0);
}

/// **KNOWING TRAVOIS LIFTS THE PACK AT THE ONE CARRY SEAM**, so the same overfull band leaves less
/// behind, and the capacity the snapshot echoes agrees with what the shed was measured against.
#[test]
fn knowing_travois_sheds_less_and_lifts_the_published_pack() {
    let (mut unknowing, unknowing_band) = far_start_world(FAT_LARDER);
    turn(&mut unknowing);
    let (mut knowing, knowing_band) = far_start_world(FAT_LARDER);
    teach(&mut knowing, TRAVOIS_DISCOVERY_ID);
    turn(&mut knowing);

    let left_without = food_left_behind(&unknowing, unknowing_band);
    let left_with = food_left_behind(&knowing, knowing_band);
    assert!(left_without > 0.0, "fixture: the unknowing band sheds");
    let multiplier = core_sim::ExpeditionConfig::builtin()
        .carry
        .travois_carry_multiplier;
    let carried_without = FAT_LARDER - left_without;
    let carried_with = FAT_LARDER - left_with;
    assert!(
        (carried_with - carried_without * multiplier).abs() < carried_without * 0.01,
        "the pack the band walks off with is `travois_carry_multiplier` times larger: \
         {carried_with} vs {carried_without} x {multiplier}"
    );

    let echoed = |app: &App, band: Entity| {
        app.world
            .resource::<SnapshotHistory>()
            .last_snapshot()
            .expect("a capture")
            .populations
            .iter()
            .find(|cohort| cohort.entity == band.to_bits())
            .map(|cohort| cohort.carry_per_worker)
            .expect("the band's row carries the echo")
    };
    let bare = core_sim::ExpeditionConfig::builtin().carry.per_worker_carry;
    assert!((echoed(&unknowing, unknowing_band) - bare).abs() < TOLERANCE);
    assert!((echoed(&knowing, knowing_band) - bare * multiplier).abs() < TOLERANCE);
}

// ---- Drives -------------------------------------------------------------------------------------

/// **A CAMP KILL ON A FOLLOWED HERD IS A DRIVE** — one learn_rate of practice per turn worked.
#[test]
fn camp_kills_on_a_followed_herd_teach_drives_a_turn_at_a_time() {
    let (mut app, _) = world(
        RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        },
        LEG_START,
        LEG_START,
        true,
    );
    for _ in 0..TURNS {
        turn(&mut app);
    }
    let learned = practice_units(progress(&app, DRIVES_DISCOVERY_ID));
    assert!(
        (learned - TURNS as f32).abs() < TOLERANCE * LESSON_COST,
        "one learn_rate per turn the crew works the followed herd: {learned} over {TURNS} turns"
    );
}

/// **THE HERD MUST BE FOLLOWED** — the same camp kill from a band that is not in migration mode is an
/// ordinary hunt.
#[test]
fn a_camp_kill_on_a_herd_nobody_follows_teaches_no_drives() {
    let (mut app, _) = world(
        RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        },
        LEG_START,
        LEG_START,
        false,
    );
    for _ in 0..TURNS {
        turn(&mut app);
    }
    assert_eq!(progress(&app, DRIVES_DISCOVERY_ID), 0.0);
}

/// **A FAR BAND KILLS NOTHING, SO IT DRIVES NOTHING** — while the band is still catching up to the
/// herd there is no camp kill to practise on.
#[test]
fn a_band_still_catching_up_to_the_herd_teaches_no_drives() {
    let (mut app, band) = far_start_world(FAT_LARDER);
    turn(&mut app);
    assert!(
        band_tile(&app, band).x < LEG_END.x,
        "fixture: the band is still short of the herd"
    );
    assert_eq!(progress(&app, DRIVES_DISCOVERY_ID), 0.0);
}

// ---- The wire -----------------------------------------------------------------------------------

fn make_herd_visible(app: &mut App) {
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the probe herd")
        .current_pos;
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    let mut ledger = app.world.resource_mut::<core_sim::VisibilityLedger>();
    ledger
        .ensure_faction(FACTION, grid.x, grid.y)
        .mark_active(pos.x, pos.y, 0);
}

/// The published snapshot, round-tripped through the FlatBuffers codec the clients read.
fn wire(app: &mut App) -> sim_schema::WorldSnapshot {
    app.world.run_system_once(recapture_snapshot_in_place);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .expect("a capture");
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    sim_schema::decode_snapshot_flatbuffer(&bytes).expect("the buffer decodes")
}

fn next_ground(snapshot: &sim_schema::WorldSnapshot) -> Option<(i32, i32)> {
    let row = snapshot.herds.iter().find(|herd| herd.id == HERD_ID)?;
    Some((row.next_ground_x, row.next_ground_y))
}

const NO_GROUND: (i32, i32) = (-1, -1);

fn three_anchor_world(roam: RoamState) -> App {
    let (mut app, _) = world(roam, LEG_END, LEG_START, false);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == HERD_ID)
            .expect("the probe herd");
        herd.route = vec![LEG_START, LEG_END, FAR_ANCHOR];
        herd.step_index = 1;
    }
    app.world.resource_mut::<SimulationConfig>().fog_enabled = true;
    // The herd's display telemetry is written by its own movement system; step it once so the row
    // exists, then let the viewer see where it stands.
    app.world.run_system_once(advance_herds);
    make_herd_visible(&mut app);
    app
}

/// ⛔ **THE FIELD IS THE GROUNDS THE HERD GOES TO NEXT, IN BOTH PHASES, AND ONLY FOR A PEOPLE WHO
/// KNOW THE LORE.** `step_index` is the anchor the herd is at (loitering) or has left (mid-leg), so
/// `route[(step_index + 1) % len]` is "the grounds it goes to next" in both.
#[test]
fn the_next_ground_is_published_only_with_herd_lore_and_is_the_next_anchor_in_both_phases() {
    for roam in [
        RoamState::Loiter {
            turns_left: LOITER_WINDOW,
        },
        RoamState::Migrate,
    ] {
        let mut app = three_anchor_world(roam);
        assert_eq!(
            next_ground(&wire(&mut app)),
            Some(NO_GROUND),
            "{roam:?}: without the lesson the visible herd carries no next ground"
        );
        teach(&mut app, HERD_LORE_DISCOVERY_ID);
        assert_eq!(
            next_ground(&wire(&mut app)),
            Some((FAR_ANCHOR.x as i32, FAR_ANCHOR.y as i32)),
            "{roam:?}: with herd lore the herd's next anchor is published"
        );
    }
}

/// A wrap: the last anchor's next ground is the first.
#[test]
fn the_next_ground_wraps_the_route() {
    let mut app = three_anchor_world(RoamState::Migrate);
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .iter_mut()
        .find(|herd| herd.id == HERD_ID)
        .expect("the probe herd")
        .step_index = 2;
    teach(&mut app, HERD_LORE_DISCOVERY_ID);
    assert_eq!(
        next_ground(&wire(&mut app)),
        Some((LEG_START.x as i32, LEG_START.y as i32))
    );
}

/// **A HERD THAT DOES NOT MIGRATE HAS NO SEASONAL GROUND.**
#[test]
fn a_graze_wandering_herd_publishes_no_next_ground() {
    let mut app = three_anchor_world(RoamState::GrazeWander);
    teach(&mut app, HERD_LORE_DISCOVERY_ID);
    assert_eq!(next_ground(&wire(&mut app)), Some(NO_GROUND));
}

/// **A HERD THE VIEWER CANNOT SEE IS NOT ON THE WIRE AT ALL**, so its ground cannot leak.
#[test]
fn a_herd_in_the_fog_carries_no_next_ground() {
    // The band camps at the leg's start; the herd stands at its far end, beyond anything it sees.
    let (mut app, _) = world(RoamState::Migrate, LEG_END, LEG_START, false);
    app.world.resource_mut::<SimulationConfig>().fog_enabled = true;
    app.world.run_system_once(advance_herds);
    teach(&mut app, HERD_LORE_DISCOVERY_ID);
    assert_eq!(
        next_ground(&wire(&mut app)),
        None,
        "fog hides the herd row, and the ground with it"
    );
}

/// ⛔ **THE ROSTER CARRIES THE THREE SKILLS AS A ROAMING BRANCH IN THE REACH AREA**, off the config
/// and not a client list, and the per-faction progress rides the same knowledge list.
#[test]
fn the_roaming_skills_ride_the_roster_as_a_branch_in_the_reach_area() {
    let (mut app, _) = world(RoamState::Migrate, LEG_START, LEG_START, false);
    teach(&mut app, HERD_LORE_DISCOVERY_ID);
    let snapshot = wire(&mut app);
    for (id, order) in [("herd_lore", 1), ("travois", 2), ("drives", 3)] {
        let row = snapshot
            .ladder_knowledge
            .iter()
            .find(|row| row.knowledge_id == id)
            .unwrap_or_else(|| panic!("{id} is on the roster"));
        assert_eq!(row.branch, "roam");
        assert_eq!(row.area, "reach");
        assert_eq!(row.order, order);
        assert!(!row.is_step, "no rung waits on a roaming skill");
    }
    assert!(
        LadderConfig::builtin()
            .areas
            .iter()
            .any(|area| area == "reach"),
        "the reach area has a place in the display order"
    );
    let progress_of = |id: &str| {
        snapshot
            .intensification_knowledge
            .iter()
            .find(|row| row.faction == FACTION.0)
            .and_then(|row| row.knowledges.iter().find(|k| k.knowledge_id == id))
            .map(|k| k.progress)
    };
    assert_eq!(progress_of("herd_lore"), Some(1.0));
    assert_eq!(
        progress_of("travois"),
        Some(0.0),
        "not begun reads 0, never absent"
    );
}
