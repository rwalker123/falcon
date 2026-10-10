//! **HUNTING BY NEED** (`docs/plan_roaming_bands.md` §Hunting by need, #798).
//!
//! A hunt row in migration mode (`move_with_herd`) is a *need row*: it holds no standing workers.
//! At the foot of each labor pass the band decides whether a crew goes out next turn; when it does,
//! the crew is mustered from the band's own hands for that one turn — idle first, then the other
//! source rows, `Low` priority first — hunts through the ordinary hunt path, and every donor gets
//! its hands back. These tests drive the turn's real systems in the Population chain's order and
//! read the plan and the donors' shares back off the ENCODED snapshot.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::hunt_by_need::NO_HUNT_NEEDED;
use core_sim::{
    advance_band_movement, advance_herds, advance_labor_allocation, build_test_app,
    follow_hunted_herds, recapture_snapshot_in_place, rot_band_larders, scalar_from_f32,
    scalar_one, scalar_zero, simulate_population, BandEquipment, BandId, EquipmentConfig,
    FactionId, FaunaConfigHandle, GenerationId, Herd, HerdRegistry, LaborAllocation,
    LaborAssignment, LaborTarget, LocalStore, MoraleCause, PopulationCohort, ResidentBand,
    RoamState, SimulationTick, SizeClass, SnapshotHistory, SourcePriority, TileRegistry,
};

use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

const MAMMOTHS: &str = "Thunder Mammoths";
const NEED_HERD: &str = "herd_need";
const DONOR_SPECIES: &str = "Red Deer";
const LOW_DONOR_HERD: &str = "herd_donor_low";
const NORMAL_DONOR_HERD: &str = "herd_donor_normal";
const BAND: u64 = 23;
const FACTION: FactionId = FactionId(0);
const FLOOR: f32 = 0.5;
const ROW: u32 = 17;
const CAMP: UVec2 = UVec2::new(24, ROW);
/// A herd past `band_work_range` of the camp.
const FAR_HERD_AT: UVec2 = UVec2::new(32, ROW);
const FODDER_PER_BIOMASS: f32 = 0.0;
const REGROWTH_RATE: f32 = 0.04;
const MAMMOTH_BODY_MASS: f32 = 800.0;
const DONOR_BODY_MASS: f32 = 60.0;
const STANDING_STOCK: f32 = 12_000.0;
const DONOR_STOCK: f32 = 5_000.0;
/// The keeping class the test larders hold: `dry`, because food in the `flesh` class rots in four
/// turns and the runway is read less what would rot uneaten — a "full" larder of it is not full.
const STORE_CLASS: &str = "dry";
/// A larder far past any runway the band could be short on.
const FAT_LARDER: f32 = 100_000.0;
/// A larder under one turn's meal.
const LEAN_LARDER: f32 = 50.0;
/// A larder the people eat down over several turns before the runway calls for a hunt.
const COMFORTABLE_LARDER: f32 = 600.0;
/// How far off the turn a crew went may be from the turn `turnsUntilHunt` named: the runway is a
/// forecast of income that itself moves as the donors' herds are hunted down.
const FORECAST_SLACK_TURNS: usize = 2;
/// Mouths that eat but do not work, so the people's meal outweighs what the donor rows bring in
/// and the band is food-limited when its larder is short. Elders, not children: children grow into
/// the working pool and would move the hands the muster draws on.
const ELDERS: f32 = 400.0;
/// Hands the band can work with, and how the donors hold them.
const WORKING: u32 = 24;
const LOW_DONOR_HANDS: u32 = 3;
const NORMAL_DONOR_HANDS: u32 = 4;
const BAND_SIZE: u32 = 30;
/// Hands on the far forage row of the geometry test.
const FAR_ROW_HANDS: u32 = 5;
/// Turns a test watches a band that should stay quiet, or hunt to the end of an animal.
const TURNS_WATCHED: usize = 12;
/// A wait that is plainly long: more turns than any test watches.
const MANY_TURNS_AWAY: u32 = 20;
/// Turns a band is given to walk the eight hexes to a herd and start hunting it.
const TURNS_TO_CATCH_UP: usize = 12;
/// A loiter window long enough that the herd never tips into a migration leg mid-test.
const LOITER_WINDOW: u32 = 60;

/// How one world is stocked.
#[derive(Clone, Copy)]
struct Stock {
    /// Where the need row's herd stands.
    herd_at: UVec2,
    /// Where the band camps.
    band_at: UVec2,
    /// The band's working-age hands.
    working: u32,
    /// Hands standing on the `Low`-priority donor row.
    low_hands: u32,
    /// Hands standing on the `Normal`-priority donor row.
    normal_hands: u32,
    /// Non-working mouths: with [`ELDERS`] the meal outweighs what the donor rows bring in; with
    /// none the donors' income covers it.
    elders: f32,
}

impl Stock {
    /// A band camped in the herd, with idle hands left over beside both donors.
    const ROOMY: Stock = Stock {
        herd_at: CAMP,
        band_at: CAMP,
        working: WORKING,
        low_hands: LOW_DONOR_HANDS,
        normal_hands: NORMAL_DONOR_HANDS,
        elders: ELDERS,
    };
    /// Donors with one hand each, so their income stays well under the meal while the people eat
    /// the larder down.
    const FEW_DONOR_HANDS: Stock = Stock {
        low_hands: 1,
        normal_hands: 1,
        ..Stock::ROOMY
    };
    /// The same band with nobody to feed but its hunters, so what the donors bring in covers the
    /// meal and the band is not food-limited.
    const SELF_FED: Stock = Stock {
        elders: 0.0,
        ..Stock::ROOMY
    };
}

fn resident_herd(
    id: &str,
    species: &str,
    class: SizeClass,
    at: UVec2,
    body: f32,
    stock: f32,
) -> Herd {
    let mut herd = Herd::new(
        id.to_string(),
        species.to_string(),
        class,
        vec![at],
        stock,
        stock,
        FODDER_PER_BIOMASS,
        REGROWTH_RATE,
        body,
    );
    herd.roam = RoamState::Loiter {
        turns_left: LOITER_WINDOW,
    };
    herd.dwell_remaining = 0;
    herd
}

/// A world with the need herd (a migratory mammoth herd), two donor herds, and one band hunting
/// all three — the mammoth on a need row (`follow`, no standing hands).
fn world(stock: Stock) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.clear();
        registry.herds.push(resident_herd(
            NEED_HERD,
            MAMMOTHS,
            SizeClass::Migratory,
            stock.herd_at,
            MAMMOTH_BODY_MASS,
            STANDING_STOCK,
        ));
        registry.herds.push(resident_herd(
            LOW_DONOR_HERD,
            DONOR_SPECIES,
            SizeClass::Big,
            stock.band_at,
            DONOR_BODY_MASS,
            DONOR_STOCK,
        ));
        registry.herds.push(resident_herd(
            NORMAL_DONOR_HERD,
            DONOR_SPECIES,
            SizeClass::Big,
            stock.band_at,
            DONOR_BODY_MASS,
            DONOR_STOCK,
        ));
    }
    let band = spawn_band(&mut app, stock);
    (app, band)
}

fn hunt_row(herd: &str, workers: u32, priority: SourcePriority, follow: bool) -> LaborAssignment {
    LaborAssignment {
        party: None,
        muster_crew: 0,
        target: LaborTarget::Hunt {
            fauna_id: herd.to_string(),
            floor: FLOOR,
            move_with_herd: follow,
        },
        workers,
        kit: None,
        priority,
    }
}

fn spawn_band(app: &mut App, stock: Stock) -> Entity {
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(stock.band_at.x, stock.band_at.y)
        .expect("the harness map carries the camp tile");
    let mut stores = LocalStore::new();
    stores.add_food(STORE_CLASS, scalar_from_f32(FAT_LARDER));
    app.world
        .spawn((
            ResidentBand,
            BandId(BAND),
            // Gear for twice the hands, so no row is ever short of a kit and the shares do not
            // move when a crew is mustered.
            BandEquipment::start_stocked_for(
                &EquipmentConfig::for_a_stocked_fixture(),
                (stock.working * 2) as f32,
            ),
            PopulationCohort {
                home: camp,
                current_tile: camp,
                size: BAND_SIZE,
                children: scalar_zero(),
                working: scalar_from_f32(stock.working as f32),
                elders: scalar_from_f32(stock.elders),
                stores,
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                last_food_spoiled: 0.0,
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
                assignments: vec![
                    hunt_row(NEED_HERD, 0, SourcePriority::Normal, true),
                    hunt_row(
                        NORMAL_DONOR_HERD,
                        stock.normal_hands,
                        SourcePriority::Normal,
                        false,
                    ),
                    hunt_row(LOW_DONOR_HERD, stock.low_hands, SourcePriority::Low, false),
                ],
                ..Default::default()
            },
        ))
        .id()
}

/// One turn of the real systems in the Population chain's order, **without the meal**: the herd
/// moves, the band follows it, the labor pass resolves, the snapshot is captured. The larder is
/// whatever the test sets it to, and the working bracket does not drift under the muster.
fn turn(app: &mut App) {
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(follow_hunted_herds);
    app.world.run_system_once(advance_band_movement);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

/// The same turn **with the people's meal and the larder's rot** in front of the labor pass, as the
/// Population chain runs them — for the tests where the larder is what is being measured.
fn turn_with_meal(app: &mut App) {
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(follow_hunted_herds);
    app.world.run_system_once(advance_band_movement);
    app.world.run_system_once(simulate_population);
    app.world.run_system_once(rot_band_larders);
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

/// Set the larder to exactly `amount` of meat.
fn set_larder(app: &mut App, band: Entity, amount: f32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band keeps its cohort");
    cohort.stores = LocalStore::new();
    cohort.stores.add_food(STORE_CLASS, scalar_from_f32(amount));
}

fn larder(app: &App, band: Entity) -> core_sim::Scalar {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .stores
        .get(core_sim::FOOD)
}

fn allocation(app: &App, band: Entity) -> &LaborAllocation {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
}

fn index_of(app: &App, band: Entity, herd: &str) -> usize {
    allocation(app, band)
        .assignments
        .iter()
        .position(
            |row| matches!(&row.target, LaborTarget::Hunt { fauna_id, .. } if fauna_id == herd),
        )
        .expect("the row is on the board")
}

fn row<'a>(app: &'a App, band: Entity, herd: &str) -> &'a LaborAssignment {
    &allocation(app, band).assignments[index_of(app, band, herd)]
}

/// What the ENCODED frame says about one hunt row.
#[derive(Debug, Clone, Default, PartialEq)]
struct PublishedRow {
    herd: String,
    workers: u32,
    muster_crew: u32,
    lent_to_hunt: u32,
    turns_until_hunt: u32,
    turns_to_kill: u32,
    kill_progress: f32,
    hunt_useful_workers: u32,
    move_with_herd: bool,
}

#[derive(Debug, Clone, Default)]
struct Published {
    rows: Vec<PublishedRow>,
    idle_mustered: u32,
    idle_workers: u32,
    turns_of_food: f32,
}

impl Published {
    fn row(&self, herd: &str) -> &PublishedRow {
        self.rows
            .iter()
            .find(|row| row.herd == herd)
            .expect("the row is on the wire")
    }
}

/// Encode the latest snapshot and read the band's cohort back through the accessors a client uses.
fn published(app: &App) -> Published {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let cohort = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == BAND)
        .expect("the band is on the wire");
    Published {
        idle_mustered: cohort.idleMustered(),
        idle_workers: cohort.idleWorkers(),
        turns_of_food: cohort.turnsOfFood(),
        rows: cohort
            .laborAssignments()
            .into_iter()
            .flatten()
            .map(|row| PublishedRow {
                herd: row.faunaId().unwrap_or_default().to_string(),
                workers: row.workers(),
                muster_crew: row.musterCrew(),
                lent_to_hunt: row.lentToHunt(),
                turns_until_hunt: row.turnsUntilHunt(),
                turns_to_kill: row.turnsToKill(),
                kill_progress: row.killProgress(),
                hunt_useful_workers: row.huntUsefulWorkers(),
                move_with_herd: row.moveWithHerd(),
            })
            .collect(),
    }
}

/// Every row's head count, in board order.
fn hands(app: &App, band: Entity) -> Vec<u32> {
    allocation(app, band)
        .assignments
        .iter()
        .map(|row| row.workers)
        .collect()
}

fn need_herd(app: &App) -> &Herd {
    app.world
        .resource::<HerdRegistry>()
        .find(NEED_HERD)
        .expect("the need herd is still in the registry")
}

/// The hands the band could lend its need row right now: idle plus every donor row's.
fn musterable_pool(app: &App, band: Entity) -> u32 {
    let idle = published(app).idle_workers;
    let need = index_of(app, band, NEED_HERD);
    allocation(app, band).musterable_pool(idle, need, &|_| true)
}

/// What a row's source paid this turn (`SourceYield::actual`), by herd.
fn taken(app: &App, band: Entity, herd: &str) -> f32 {
    allocation(app, band).last_yields[index_of(app, band, herd)].actual
}

/// Write the plan by hand, as a loaded save would carry it, and republish.
fn set_muster(app: &mut App, band: Entity, crew: u32) {
    let need = index_of(app, band, NEED_HERD);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .assignments[need]
        .muster_crew = crew;
    recapture_snapshot_in_place(&mut app.world);
}

fn set_hands(app: &mut App, band: Entity, herd: &str, workers: u32) {
    let idx = index_of(app, band, herd);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .assignments[idx]
        .workers = workers;
}

/// ⛔ **A FULL LARDER SENDS NOBODY, AND THE DONORS ARE NEVER TOUCHED.** The sentinel runway never
/// triggers a hunt, however many turns pass. Paired with the lean larder below, which sends a crew
/// from the very same world — a plan that was always zero would pass this alone.
#[test]
fn a_full_larder_sends_no_crew_and_leaves_the_donors_alone() {
    let (mut app, band) = world(Stock::ROOMY);
    let board = hands(&app, band);
    let mut donors_worked = false;
    for step in 0..TURNS_WATCHED {
        set_larder(&mut app, band, FAT_LARDER);
        turn_with_meal(&mut app);
        let need = row(&app, band, NEED_HERD);
        assert_eq!(
            need.muster_crew, 0,
            "turn {step}: a full larder sends no crew"
        );
        assert_eq!(hands(&app, band), board, "turn {step}: nobody was moved");
        let wire = published(&app);
        let wire_need = wire.row(NEED_HERD);
        assert_eq!(wire_need.muster_crew, 0);
        assert!(
            wire_need.turns_until_hunt > MANY_TURNS_AWAY,
            "a long larder is a long wait ({} turns)",
            wire_need.turns_until_hunt
        );
        assert_eq!(wire.idle_mustered, 0);
        assert!(
            wire.rows.iter().all(|row| row.lent_to_hunt == 0),
            "no donor lends"
        );
        donors_worked |= taken(&app, band, NORMAL_DONOR_HERD) > 0.0;
    }
    assert!(
        donors_worked,
        "liveness: the donor rows really were hunting"
    );
    assert_eq!(
        need_herd(&app).wounds.pending(),
        0.0,
        "the mammoths were never touched"
    );
}

/// ⛔ **A BAND WHOSE INCOME COVERS ITS MEALS NEVER HUNTS BY NEED**, however lean its larder: the
/// runway reads the not-food-limited sentinel and the sentinel never triggers a crew.
#[test]
fn a_band_whose_income_covers_its_meals_never_hunts_by_need() {
    let (mut app, band) = world(Stock::SELF_FED);
    for step in 0..TURNS_WATCHED {
        set_larder(&mut app, band, LEAN_LARDER);
        turn(&mut app);
        let wire = published(&app);
        assert_eq!(
            wire.turns_of_food,
            core_sim::NOT_FOOD_LIMITED_TURNS,
            "turn {step}: the donors' income covers the meal"
        );
        let need = wire.row(NEED_HERD);
        assert_eq!(need.muster_crew, 0, "turn {step}: no crew");
        assert_eq!(need.turns_until_hunt, NO_HUNT_NEEDED, "and none will be");
    }
    // The control: the same band with mouths to feed does hunt on the same larder.
    let (mut hungry, hungry_band) = world(Stock::ROOMY);
    set_larder(&mut hungry, hungry_band, LEAN_LARDER);
    turn(&mut hungry);
    assert!(
        row(&hungry, hungry_band, NEED_HERD).muster_crew > 0,
        "liveness: a food-limited band on that larder sends a crew"
    );
}

/// ⛔ **A LEAN LARDER MUSTERS A CREW OF `hunt_useful_crew` CAPPED BY THE POOL**, the plan the wire
/// previews is the one the next turn applies, and every hand goes home afterwards.
#[test]
fn a_lean_larder_musters_the_useful_crew_and_hands_every_donor_back() {
    let (mut app, band) = world(Stock::ROOMY);
    set_larder(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    let pool = musterable_pool(&app, band);
    let wire = published(&app);
    let need = wire.row(NEED_HERD);
    assert!(need.muster_crew > 0, "a short larder sends a crew");
    assert_eq!(
        need.muster_crew,
        need.hunt_useful_workers.min(pool),
        "the crew is the useful crew priced over the musterable pool, capped by it"
    );
    assert_eq!(need.workers, 0, "the row holds no standing hands");
    assert_eq!(need.turns_until_hunt, 0, "a crew goes out now");
    assert!(need.turns_to_kill > 0, "the kill clock is published");

    // The preview is the muster function's own answer.
    let idx = index_of(&app, band, NEED_HERD);
    let plan =
        allocation(&app, band).muster_donors(wire.idle_workers, idx, need.muster_crew, &|_| true);
    assert_eq!(wire.idle_mustered, plan.from_idle);
    for herd in [NORMAL_DONOR_HERD, LOW_DONOR_HERD] {
        assert_eq!(
            wire.row(herd).lent_to_hunt,
            plan.lent_by(index_of(&app, band, herd)),
            "{herd}: the board states what the turn will take"
        );
    }
    assert_eq!(plan.total(), need.muster_crew, "the pool covers the crew");

    let board = hands(&app, band);
    set_larder(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    assert_eq!(hands(&app, band), board, "every donor has its hands back");
    assert!(
        need_herd(&app).wounds.pending() > 0.0 || taken(&app, band, NEED_HERD) > 0.0,
        "liveness: the crew really hunted the mammoths"
    );
}

/// Run one turn of `app` with a full larder, so the decision at its foot cannot move the plan under
/// test.
fn run_one_turn_fed(app: &mut App, band: Entity) {
    set_larder(app, band, FAT_LARDER);
    turn(app);
}

/// ⛔ **THE MUSTER TAKES IDLE FIRST, THEN `Low`, THEN `Normal` — AND THE TURN TAKES EXACTLY WHAT THE
/// BOARD PUBLISHED.** The crew is set by hand (as a loaded save would carry it) to every idle hand,
/// the whole `Low` row and one hand of the `Normal` row. Three worlds run the same turn:
///
/// * **A** — the real muster;
/// * **B** — no muster, the `Normal` donor pre-cut by the one hand the board published;
/// * **C** — no muster, nobody cut (the control).
///
/// A's `Normal` yield equals B's and differs from C's: the turn took exactly the published hand, and
/// the control proves the cut is visible at all. A's emptied `Low` donor took nothing, kept its row,
/// and has its three hands back.
#[test]
fn the_turn_takes_what_the_board_published_idle_then_low_then_normal() {
    let stock = Stock::ROOMY;
    let idle = stock.working - stock.low_hands - stock.normal_hands;
    let crew = idle + stock.low_hands + 1;

    let (mut a, band_a) = world(stock);
    set_muster(&mut a, band_a, crew);
    let wire = published(&a);
    assert_eq!(wire.idle_mustered, idle, "idle hands go first");
    assert_eq!(
        wire.row(LOW_DONOR_HERD).lent_to_hunt,
        stock.low_hands,
        "then Low, whole"
    );
    assert_eq!(
        wire.row(NORMAL_DONOR_HERD).lent_to_hunt,
        1,
        "then one from Normal"
    );

    let (mut b, band_b) = world(stock);
    set_hands(&mut b, band_b, NORMAL_DONOR_HERD, stock.normal_hands - 1);
    let (mut c, band_c) = world(stock);

    run_one_turn_fed(&mut a, band_a);
    run_one_turn_fed(&mut b, band_b);
    run_one_turn_fed(&mut c, band_c);

    let normal_a = taken(&a, band_a, NORMAL_DONOR_HERD);
    assert!(normal_a > 0.0, "liveness: the Normal donor still hunted");
    assert_eq!(
        normal_a,
        taken(&b, band_b, NORMAL_DONOR_HERD),
        "A's Normal donor worked with exactly the hands the board said it would keep"
    );
    assert_ne!(
        normal_a,
        taken(&c, band_c, NORMAL_DONOR_HERD),
        "control: one hand fewer is visible in the take, so the equality above says something"
    );
    assert_eq!(
        taken(&a, band_a, LOW_DONOR_HERD),
        0.0,
        "the emptied Low donor hunted nothing this turn"
    );
    assert!(
        taken(&c, band_c, LOW_DONOR_HERD) > 0.0,
        "control: it would have, had it kept its hands"
    );
    assert_eq!(
        hands(&a, band_a),
        vec![0, stock.normal_hands, stock.low_hands],
        "every donor is back at full strength — and the emptied row was not lapsed"
    );
    assert!(
        need_herd(&a).wounds.pending() > 0.0,
        "liveness: the mustered crew hunted the mammoths"
    );
    assert_eq!(need_herd(&b).wounds.pending(), 0.0, "B sent nobody");
}

/// ⛔ **A DONOR'S SOURCE MUST BE IN WORK RANGE, BY GEOMETRY.** A brand-new far forage row has no party
/// yet, so "carries a party" would let it lend; it is `Low`, so it would be the first donor. It lends
/// nothing — idle hands and the local rows are asked instead.
#[test]
fn a_far_row_with_no_party_yet_is_not_a_donor() {
    let stock = Stock::ROOMY;
    let idle = stock.working - stock.low_hands - stock.normal_hands - FAR_ROW_HANDS;
    let (mut app, band) = world(stock);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .assignments
        .push(LaborAssignment {
            party: None,
            muster_crew: 0,
            target: LaborTarget::Forage {
                tile: FAR_HERD_AT,
                floor: FLOOR,
                species: None,
                take_species: core_sim::TakeSelection::EVERYTHING,
            },
            workers: FAR_ROW_HANDS,
            kit: None,
            priority: SourcePriority::Low,
        });
    // Everything idle plus every local donor, and then some: only a far donor could cover the rest.
    set_muster(
        &mut app,
        band,
        idle + stock.low_hands + stock.normal_hands + FAR_ROW_HANDS,
    );
    let wire = published(&app);
    assert_eq!(wire.idle_mustered, idle, "idle hands go first");
    assert_eq!(wire.row(LOW_DONOR_HERD).lent_to_hunt, stock.low_hands);
    assert_eq!(wire.row(NORMAL_DONOR_HERD).lent_to_hunt, stock.normal_hands);
    let far = wire
        .rows
        .iter()
        .find(|row| row.herd.is_empty() && row.workers == FAR_ROW_HANDS)
        .expect("the far forage row is on the wire");
    assert_eq!(far.lent_to_hunt, 0, "a far row never lends, party or not");
    let idx = index_of(&app, band, NEED_HERD);
    let pool = allocation(&app, band).musterable_pool(wire.idle_workers, idx, &|_| true);
    assert_eq!(
        pool,
        wire.idle_workers + stock.low_hands + stock.normal_hands + FAR_ROW_HANDS,
        "control: without the geometry test the far row's hands would be in the pool"
    );
}

/// ⛔ **A SMALL BAND'S CREW IS CAPPED AT ITS POOL, AND EVERY SOURCE ROW CAN BE EMPTIED FOR THE TURN.**
#[test]
fn a_small_band_sends_its_whole_pool_and_empties_every_source_row() {
    let small = Stock {
        working: 6,
        low_hands: 2,
        normal_hands: 3,
        ..Stock::ROOMY
    };
    let (mut app, band) = world(small);
    set_larder(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    let wire = published(&app);
    let pool = small.working;
    assert_eq!(
        wire.row(NEED_HERD).muster_crew,
        pool,
        "the crew is the whole pool"
    );
    assert_eq!(wire.idle_mustered, 1, "the one idle hand");
    assert_eq!(wire.row(LOW_DONOR_HERD).lent_to_hunt, small.low_hands);
    assert_eq!(wire.row(NORMAL_DONOR_HERD).lent_to_hunt, small.normal_hands);

    set_larder(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    assert_eq!(taken(&app, band, LOW_DONOR_HERD), 0.0, "Low emptied");
    assert_eq!(taken(&app, band, NORMAL_DONOR_HERD), 0.0, "Normal emptied");
    assert_eq!(
        hands(&app, band),
        vec![0, small.normal_hands, small.low_hands],
        "and every hand came home"
    );
    assert!(
        need_herd(&app).wounds.pending() > 0.0,
        "liveness: they hunted"
    );

    // The cap is the pool: a roomy band prices a bigger useful crew over the same herd.
    let (mut roomy, roomy_band) = world(Stock::ROOMY);
    set_larder(&mut roomy, roomy_band, LEAN_LARDER);
    turn(&mut roomy);
    assert!(
        published(&roomy).row(NEED_HERD).hunt_useful_workers > pool,
        "the small band's crew is capped by its hands, not by the herd"
    );
}

/// ⛔ **WOUNDS PENDING KEEP A CREW GOING UNTIL THE ANIMAL IS DOWN, AND THE KILL LANDS WHOLE.** After
/// the first sending turn the larder is refilled to the brim every turn — the runway alone would
/// never send anybody — so the only thing that can keep the crew out is the animal already partly
/// down.
#[test]
fn wounds_keep_the_crew_going_until_the_animal_is_down_and_the_carcass_lands_whole() {
    let (mut app, band) = world(Stock::ROOMY);
    set_larder(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    assert!(
        row(&app, band, NEED_HERD).muster_crew > 0,
        "the hunt starts on need"
    );

    let provisions = MAMMOTH_BODY_MASS
        * app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .hunt
            .provisions_per_biomass;
    let mut kills = 0;
    for step in 0..TURNS_WATCHED {
        assert!(
            row(&app, band, NEED_HERD).muster_crew > 0,
            "turn {step}: the animal is not down yet, so the crew goes out again"
        );
        set_larder(&mut app, band, FAT_LARDER);
        let before = larder(&app, band);
        turn(&mut app);
        let need = allocation(&app, band).last_yields[index_of(&app, band, NEED_HERD)].clone();
        if need.actual > 0.0 {
            kills += 1;
            assert_eq!(
                larder(&app, band) - before,
                scalar_from_f32(need.actual),
                "the whole carcass is the larder's gain, in fixed point"
            );
            assert!(
                (need.actual - provisions).abs() < 1e-3,
                "one whole mammoth: {} vs {provisions}",
                need.actual
            );
            assert_eq!(need.wasted, 0.0, "nothing of the carcass is left behind");
            break;
        }
        assert!(
            need_herd(&app).wounds.pending() > 0.0,
            "turn {step}: a blow was struck and the animal stands wounded"
        );
    }
    assert_eq!(kills, 1, "liveness: the animal went down");
    assert_eq!(
        row(&app, band, NEED_HERD).muster_crew,
        0,
        "with the animal down and a full larder nobody is sent"
    );
    assert!(
        need_herd(&app).wounds.pending() > 0.0,
        "the crew's excess damage banks toward the next animal — which must not send a crew by itself"
    );
}

/// ⛔ **ONLY A CAMP KILL IS MUSTERED.** A herd beyond `band_work_range` is not hunted at all: no
/// crew, no party, the donors untouched — however lean the larder — until the band has caught up.
#[test]
fn a_herd_beyond_work_range_is_not_hunted_and_posts_no_party() {
    let far = Stock {
        herd_at: FAR_HERD_AT,
        ..Stock::ROOMY
    };
    let (mut app, band) = world(far);
    let width = app.world.resource::<TileRegistry>().width;
    let wrap = app
        .world
        .resource::<core_sim::SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let range = app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get()
        .band_work_range;
    let board = hands(&app, band);
    let (mut waited_far, mut hunted_near) = (0, 0);
    for _ in 0..TURNS_TO_CATCH_UP {
        set_larder(&mut app, band, LEAN_LARDER);
        let before = need_herd(&app).wounds.pending();
        turn(&mut app);
        let band_at = app
            .world
            .get::<core_sim::Tile>(
                app.world
                    .get::<PopulationCohort>(band)
                    .expect("the band keeps its cohort")
                    .current_tile,
            )
            .expect("the band stands on a tile")
            .position;
        let distance = core_sim::grid_utils::hex_distance_wrapped(
            band_at,
            need_herd(&app).current_pos,
            width,
            wrap,
        );
        let crew = row(&app, band, NEED_HERD).muster_crew;
        assert!(
            row(&app, band, NEED_HERD).party.is_none(),
            "a need row never posts a party"
        );
        if distance > range {
            waited_far += 1;
            assert_eq!(crew, 0, "{distance} hexes out: no crew");
            assert_eq!(hands(&app, band), board, "and nobody was moved");
            assert_eq!(
                published(&app).row(NEED_HERD).turns_until_hunt,
                NO_HUNT_NEEDED
            );
            assert_eq!(
                need_herd(&app).wounds.pending(),
                before,
                "the herd is untouched"
            );
        } else {
            hunted_near += 1;
            assert!(crew > 0, "inside the apron a lean band sends a crew");
        }
    }
    assert!(
        waited_far > 0,
        "liveness: the herd really started out of range"
    );
    assert!(hunted_near > 0, "liveness: the band caught up and hunted");
}

/// The people's meal for one turn at the band's brackets, from the shipped consumption table — what
/// `simulate_population` would debit, without also moving the brackets themselves.
fn meal(app: &App, band: Entity) -> f32 {
    let consumption = app
        .world
        .resource::<core_sim::DemographicsConfigHandle>()
        .get()
        .consumption
        .clone();
    let cohort = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort");
    consumption.per_capita_draw
        * (consumption.child_factor * cohort.children.to_f32()
            + consumption.working_factor * cohort.working.to_f32()
            + consumption.elder_factor * cohort.elders.to_f32())
}

/// ⛔ **A BAND THAT EATS ITS WAY DOWN HUNTS WHEN THE LARDER RUNS SHORT, NOT BEFORE — AND SAYS WHEN.**
/// Nothing is refilled: each turn the band eats its meal out of the larder and the donors bring in
/// what they bring, the runway falls, and a crew goes out only once it has. The `turnsUntilHunt` it
/// published while waiting names the turn it went (to within a turn: the runway is a forecast), and
/// every turn the donors end where they began.
#[test]
fn a_band_waits_while_the_larder_is_comfortable_and_hunts_when_it_is_not() {
    let (mut app, band) = world(Stock::FEW_DONOR_HANDS);
    set_larder(&mut app, band, COMFORTABLE_LARDER);
    let board = hands(&app, band);
    let mut first_forecast: Option<(usize, u32)> = None;
    let mut sent_at = None;
    for step in 0..TURNS_WATCHED * 2 {
        let after_meal = larder(&app, band).to_f32() - meal(&app, band);
        set_larder(&mut app, band, after_meal);
        turn(&mut app);
        assert_eq!(hands(&app, band), board, "turn {step}: hands are home");
        let wire = published(&app);
        let need = wire.row(NEED_HERD);
        if need.muster_crew > 0 {
            sent_at = Some(step);
            break;
        }
        assert!(
            need.turns_until_hunt > 0 && need.turns_until_hunt != NO_HUNT_NEEDED,
            "turn {step}: a food-limited band says when it will hunt"
        );
        first_forecast.get_or_insert((step, need.turns_until_hunt));
    }
    let (forecast_at, wait) = first_forecast.expect("it waited while the larder was comfortable");
    let sent_at = sent_at.expect("liveness: it hunted once the larder ran short");
    let predicted = forecast_at + wait as usize;
    assert!(
        sent_at.abs_diff(predicted) <= FORECAST_SLACK_TURNS,
        "forecast at turn {forecast_at} said {wait} more turns, it went at turn {sent_at}"
    );
}

/// ⛔ **A NEED ROW SURVIVES THE SAVE** — `muster_crew` is a fact the turn restamps, but a checkpoint
/// taken between the decision and its application must carry it or a reload forgets a hunt.
#[test]
fn the_plan_round_trips_through_the_save() {
    let (mut app, band) = world(Stock::ROOMY);
    set_larder(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    let planned = row(&app, band, NEED_HERD).muster_crew;
    assert!(planned > 0, "liveness: there is a plan to carry");

    let blob = core_sim::save::encode_save(&app.world).expect("the world encodes");
    let (_, payload) = core_sim::save::decode_save(&blob).expect("the save decodes");
    let saved = payload
        .sim
        .bands
        .iter()
        .find(|record| record.id == BandId(BAND))
        .expect("the band is in the save")
        .labor
        .as_ref()
        .expect("its allocation is in the save");
    let need = saved
        .assignments
        .iter()
        .find(|row| row.target.is_need_row())
        .expect("the need row survives");
    assert_eq!(
        need.muster_crew, planned,
        "the plan came back from the blob"
    );
}

/// ⛔ **THE WIRE CARRIES THE FIELDS BOTH WAYS** — encoded by the codec, decoded back into the state
/// types the native client mirrors.
#[test]
fn the_new_fields_encode_and_decode() {
    let (mut app, band) = world(Stock::ROOMY);
    set_larder(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    let decoded = decode_latest(&app);
    let cohort = decoded
        .populations
        .iter()
        .find(|cohort| cohort.band_id == BAND)
        .expect("the band is on the wire");
    let rows = |herd: &str| {
        cohort
            .labor_assignments
            .iter()
            .find(|row| row.fauna_id == herd)
            .expect("the row decodes")
    };
    let need = rows(NEED_HERD);
    assert!(need.muster_crew > 0, "the plan decodes");
    assert!(need.turns_to_kill > 0, "the kill clock decodes");
    assert_eq!(need.turns_until_hunt, 0);
    assert!(need.move_with_herd);
    assert!(cohort.idle_mustered > 0, "the idle share decodes");
    let lent: u32 = [NORMAL_DONOR_HERD, LOW_DONOR_HERD]
        .iter()
        .map(|herd| rows(herd).lent_to_hunt)
        .sum();
    assert_eq!(
        cohort.idle_mustered + lent,
        need.muster_crew,
        "idle plus what the donors lend is the crew, after the round trip"
    );

    // After a blow the wound's progress decodes as a fraction of the body.
    run_one_turn_fed(&mut app, band);
    let decoded = decode_latest(&app);
    let need = decoded
        .populations
        .iter()
        .find(|cohort| cohort.band_id == BAND)
        .expect("the band is on the wire")
        .labor_assignments
        .iter()
        .find(|row| row.fauna_id == NEED_HERD)
        .expect("the row decodes");
    assert!(
        need.kill_progress > 0.0 && need.kill_progress < 1.0,
        "the wound decodes: {}",
        need.kill_progress
    );
}

fn decode_latest(app: &App) -> sim_schema::WorldSnapshot {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    sim_schema::decode_snapshot_flatbuffer(bytes.as_ref()).expect("it decodes")
}
