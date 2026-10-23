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
use core_sim::shipped;

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
/// **The donors' herds regrow fast enough to need every hand the fixture puts on them** (#871): the
/// sim staffs a row with the crew that reaches its floor, capped by the row's Max, so a donor whose
/// herd barely regrows would be staffed with fewer hands than the muster is being tested over.
const DONOR_REGROWTH_RATE: f32 = 0.5;
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
    regrowth: f32,
) -> Herd {
    let mut herd = Herd::new(
        id.to_string(),
        species.to_string(),
        class,
        vec![at],
        stock,
        stock,
        FODDER_PER_BIOMASS,
        regrowth,
        body,
        shipped::root_standing(shipped::animal()),
    );
    herd.roam = RoamState::Loiter {
        turns_left: LOITER_WINDOW,
    };
    herd.dwell_remaining = 0;
    herd
}

/// What the need row's herd is.
#[derive(Clone, Copy)]
struct Quarry {
    species: &'static str,
    body_mass: f32,
    stock: f32,
}

/// The default quarry: a migratory mammoth herd.
const MAMMOTH_HERD: Quarry = Quarry {
    species: MAMMOTHS,
    body_mass: MAMMOTH_BODY_MASS,
    stock: STANDING_STOCK,
};

/// A world with the need herd (a migratory mammoth herd), two donor herds, and one band hunting
/// all three — the mammoth on a need row (`follow`, no standing hands).
fn world(stock: Stock) -> (App, Entity) {
    let (mut app, band) = world_hunting(stock, MAMMOTH_HERD);
    meat_keeps_like_the_stores(&mut app);
    (app, band)
}

/// **The need clock counts only the food that keeps no longer than the kill does** - so a fixture
/// whose subject is "the larder" holds it in a class the kill's own class outlasts. Stretching
/// `flesh` to the stores' shelf life makes the stored `dry` food count, as it did before the clock
/// separated meat from reserve. The marsh and shelf-life tests below leave the shipped table alone
/// and hold their larders as meat.
fn meat_keeps_like_the_stores(app: &mut App) {
    let stores_life = stores_shelf_life(app);
    set_flesh_shelf_life(app, stores_life);
}

/// Stretch (or shrink) how long `flesh` keeps - the stand-in for preservation.
fn set_flesh_shelf_life(app: &mut App, life: f32) {
    let handle = app.world.resource::<core_sim::DemographicsConfigHandle>();
    let mut config = (*handle.get()).clone();
    for class in config.keeping.classes.iter_mut().filter(|c| c.id == FLESH) {
        class.shelf_life_turns = life;
    }
    app.world
        .resource_mut::<core_sim::DemographicsConfigHandle>()
        .replace(std::sync::Arc::new(config));
}

/// [`world`] with the need row's herd stated.
fn world_hunting(stock: Stock, quarry: Quarry) -> (App, Entity) {
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
            quarry.species,
            SizeClass::Migratory,
            stock.herd_at,
            quarry.body_mass,
            quarry.stock,
            REGROWTH_RATE,
        ));
        registry.herds.push(resident_herd(
            LOW_DONOR_HERD,
            DONOR_SPECIES,
            SizeClass::Big,
            stock.band_at,
            DONOR_BODY_MASS,
            DONOR_STOCK,
            DONOR_REGROWTH_RATE,
        ));
        registry.herds.push(resident_herd(
            NORMAL_DONOR_HERD,
            DONOR_SPECIES,
            SizeClass::Big,
            stock.band_at,
            DONOR_BODY_MASS,
            DONOR_STOCK,
            DONOR_REGROWTH_RATE,
        ));
    }
    let band = spawn_band(&mut app, stock);
    (app, band)
}

fn hunt_row(herd: &str, workers: u32, priority: SourcePriority, follow: bool) -> LaborAssignment {
    LaborAssignment {
        party: None,
        muster_crew: 0,
        planned_crew: 0,
        max_workers: Some(workers),
        staffing: Default::default(),
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
                fed_party_workers: Default::default(),
                last_food_spoiled: 0.0,
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

/// Set the larder to exactly `amount` of long-keeping stores.
fn set_larder(app: &mut App, band: Entity, amount: f32) {
    set_larder_of(app, band, STORE_CLASS, amount);
}

/// Set the larder to exactly `amount` of fresh meat, the food a kill replaces.
fn set_meat(app: &mut App, band: Entity, amount: f32) {
    set_larder_of(app, band, FLESH, amount);
}

fn set_larder_of(app: &mut App, band: Entity, class: &str, amount: f32) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band keeps its cohort");
    cohort.stores = LocalStore::new();
    cohort.stores.add_food(class, scalar_from_f32(amount));
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
    kit_id: String,
    need_runway: f32,
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
                kit_id: row.kitId().unwrap_or_default().to_string(),
                need_runway: row.needRunway(),
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

/// The crafting bench as a muster donor, as the sim reads it (`None` for a band with no bench).
fn donor_bench(app: &App, band: Entity) -> Option<core_sim::MusterBench> {
    app.world
        .get::<core_sim::BandBench>(band)
        .and_then(core_sim::MusterBench::of)
}

/// The hands the band could lend its need row right now: idle plus every donor row's.
fn musterable_pool(app: &App, band: Entity) -> u32 {
    let idle = published(app).idle_workers;
    let need = index_of(app, band, NEED_HERD);
    allocation(app, band).musterable_pool(idle, need, &|_| true, donor_bench(app, band))
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
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation");
    allocation.assignments[idx].workers = workers;
    // **The crew is the sim's now** (#871): the Max is what pins a donor to the hands stated.
    allocation.assignments[idx].max_workers = Some(workers);
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
    // The gear covers the whole band, so the kit is not short and the order is Priority alone.
    let plan = allocation(&app, band).muster_donors(
        wire.idle_workers,
        idx,
        need.muster_crew,
        &|_| true,
        donor_bench(&app, band),
        &core_sim::MusterKit::default(),
    );
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
            planned_crew: 0,
            max_workers: Some(FAR_ROW_HANDS),
            staffing: Default::default(),
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
    let pool = allocation(&app, band).musterable_pool(
        wire.idle_workers,
        idx,
        &|_| true,
        donor_bench(&app, band),
    );
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
    // The band walks off the camp its donors stand in, so the gear it can equip a crew with must be
    // in hand: a mammoth crew is only sent when it can be armed.
    arm_the_band(&mut app, band, ENOUGH_UNITS);
    // **The board is read after the sim has staffed the rows** (#871): the Low donor's gathering kit
    // cannot wound a deer, so the passes put nobody on it, and that is where it starts.
    core_sim::staffing::restaff_band(&mut app.world, band);
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

// ---------------------------------------------------------------------------------------------
// SIZE THE CREW TO THE MEAL
//
// A crew is the smallest that brings down what the band can eat before the meat spoils — at least
// one animal, never more than the useful crew. Each test below reads the sizing back off the wire and
// compares it with an ORACLE built from the fixture's own numbers: the band's demand and income, the
// species' body and meat rate, the keeping class's shelf life, and the crew-take curve as the compose
// sheet asks for it.
// ---------------------------------------------------------------------------------------------

const MARSH_GRAZERS: &str = "Marsh Grazers";
const MARSH_BODY_MASS: f32 = 47.0;
/// Animals standing in the marsh herd: with the shipped 50% floor, room for half of them.
const MARSH_HEAD_COUNT: f32 = 74.0;
const MARSH_HERD: Quarry = Quarry {
    species: MARSH_GRAZERS,
    body_mass: MARSH_BODY_MASS,
    stock: MARSH_HEAD_COUNT * MARSH_BODY_MASS,
};
/// A camp of a few hundred people: enough hands that the useful crew on small game is far larger
/// than the meal calls for.
const BIG_CAMP_WORKING: u32 = 160;
const BIG_CAMP_ELDERS: f32 = 40.0;
const BIG_CAMP: Stock = Stock {
    working: BIG_CAMP_WORKING,
    elders: BIG_CAMP_ELDERS,
    ..Stock::ROOMY
};
/// The keeping class a kill lands in.
const FLESH: &str = "flesh";
/// Meat enough to cover the meal's whole window (over `flesh`'s shelf life the band's net eating is
/// ~55 food on the fixture camp) yet less than the people eat before it spoils, so none rots
/// uneaten: the meal is nothing and only the one whole animal remains.
const MEAL_COVERING_LARDER: f32 = 60.0;
/// A margin so wide the band hunts whatever its larder says, so a meal of nothing is still sent.
const ALWAYS_DUE_MARGIN_TURNS: f32 = 100.0;
/// How many times longer the stand-in for drying keeps meat.
const DRIED_SHELF_FACTOR: f32 = 2.0;
/// The same relative slack the sim's curve reads apart float noise with.
const RISE_TOLERANCE: f32 = 0.001;

/// The sizing, derived independently of the sim's own functions.
struct Sizing {
    /// The meal in food.
    food: f32,
    /// Whole animals the meal calls for.
    animals: f32,
    /// The crew-take curve over the musterable pool, as the compose sheet asks it.
    likely: Vec<f32>,
    /// The useful crew the wire publishes for the row.
    useful: u32,
    /// The crew the meal calls for: the smallest row landing the animals, else the useful crew.
    crew: u32,
    /// Whether any row of the curve reaches the animals.
    reached: bool,
}

fn config_shelf_life(app: &App) -> f32 {
    app.world
        .resource::<core_sim::DemographicsConfigHandle>()
        .get()
        .keeping
        .shelf_life(FLESH)
        .expect("flesh is a shipped class")
}

/// Food in one whole animal, from the species' own meat rate.
fn food_per_animal(app: &App, species: &str, body_mass: f32) -> f32 {
    body_mass
        * app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .hunt_yield_for(species)
            .provisions_per_biomass
}

/// Derive the sizing for the need row from the band as the labor pass left it.
fn sizing(app: &mut App, band: Entity, quarry: Quarry) -> Sizing {
    let wire = published(app);
    let need = wire.row(NEED_HERD);
    let pool = musterable_pool(app, band);
    // The standing income, summed from the two donor rows alone: the need row's own yield is a
    // one-turn crew's projection, not income (and is named here so the oracle does not share the
    // sim's own filter).
    let income: f32 = [NORMAL_DONOR_HERD, LOW_DONOR_HERD]
        .iter()
        .map(|herd| allocation(app, band).last_yields[index_of(app, band, herd)].realized)
        .sum();
    let net_eat = (meal(app, band) - income).max(0.0);
    let usable_larder = larder(app, band).to_f32();
    let food = (net_eat * config_shelf_life(app) - usable_larder).max(0.0);
    let per_animal = food_per_animal(app, quarry.species, quarry.body_mass);
    let animals = (food / per_animal).ceil().max(1.0);
    let reply = core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &sim_runtime::commands::QueryPayload::HuntCrewTake(
            sim_runtime::commands::HuntCrewTakeQuery {
                faction_id: FACTION.0,
                band_id: BAND,
                herd_id: NEED_HERD.to_string(),
                kit_id: need.kit_id.clone(),
                floor: FLOOR,
                max_workers: pool,
                // The sheet's ask for a need row: priced over the gear the muster brings.
                follow: true,
            },
        ),
    );
    let sim_runtime::commands::QueryReply::HuntCrewTake(curve) = reply else {
        panic!("the crew-take query answers with a curve");
    };
    let useful = need.hunt_useful_workers;
    let likely: Vec<f32> = curve
        .per_crew
        .iter()
        .map(|row| row.animals_likely)
        .collect();
    let smallest = likely
        .iter()
        .take(useful as usize)
        .position(|take| take * (1.0 + RISE_TOLERANCE) >= animals)
        .map(|index| index as u32 + 1);
    Sizing {
        food,
        animals,
        likely,
        useful,
        crew: smallest.unwrap_or(useful),
        reached: smallest.is_some(),
    }
}

/// Run one turn on `amount` of meat and size the need row.
fn sized_at(stock: Stock, quarry: Quarry, amount: f32) -> (App, Entity, Sizing) {
    let (mut app, band) = world_hunting(stock, quarry);
    set_meat(&mut app, band, amount);
    turn(&mut app);
    let sized = sizing(&mut app, band, quarry);
    (app, band, sized)
}

fn planned_crew(app: &App) -> u32 {
    published(app).row(NEED_HERD).muster_crew
}

/// ⛔ **A CREW IS SIZED TO THE MEAL, NOT TO THE PLATEAU.** Small easy game has a useful crew far past
/// anything the band can eat before the meat spoils: Marsh Grazers (47 biomass a head, ≈ 2.8 food an
/// animal, 37 animals of room) priced a crew of 131 on a camp of 200, which would kill dozens of
/// animals in one turn and watch most of them rot in four. The crew is the smallest row of the curve
/// that lands what the camp can eat of one kill, so it is far below the useful crew, and the wire's
/// `musterCrew` is that crew.
#[test]
fn a_crew_is_the_smallest_that_lands_the_meal_far_below_the_useful_crew() {
    let (app, _band, sized) = sized_at(BIG_CAMP, MARSH_HERD, 0.0);
    assert!(
        sized.reached,
        "liveness: some crew lands the meal's animals"
    );
    assert!(
        sized.food > 0.0 && sized.animals > 1.0,
        "liveness: the meal is a real one"
    );
    assert!(
        sized.crew < sized.useful,
        "the meal calls for fewer hands ({}) than the plateau ({})",
        sized.crew,
        sized.useful
    );
    assert_eq!(
        planned_crew(&app),
        sized.crew,
        "the published plan is the smallest crew landing {} animals",
        sized.animals
    );
    let below = sized.crew as usize - 1;
    assert!(
        below == 0 || sized.likely[below - 1] * (1.0 + RISE_TOLERANCE) < sized.animals,
        "one hand fewer would land too few"
    );
}

/// ⛔ **A MAMMOTH IS UNCHANGED**: no crew brings one down in a turn, so no row of the curve reaches
/// even the one animal the meal floors at, and the crew is the useful one.
#[test]
fn a_mammoth_is_still_sent_the_useful_crew() {
    let (app, band, sized) = sized_at(Stock::ROOMY, MAMMOTH_HERD, LEAN_LARDER);
    assert!(!sized.reached, "no crew lands a mammoth in a turn");
    assert!(
        sized.likely.iter().all(|take| *take < sized.animals),
        "every row falls short of the animals the meal calls for"
    );
    assert_eq!(
        planned_crew(&app),
        sized.useful.min(musterable_pool(&app, band)),
        "the crew is the useful crew"
    );
}

/// ⛔ **A BIGGER LARDER IS A SMALLER MEAL AND A SMALLER CREW.**
#[test]
fn a_bigger_larder_sends_a_smaller_crew() {
    let (empty_app, _, empty) = sized_at(BIG_CAMP, MARSH_HERD, 0.0);
    let (stocked_app, _, stocked) = sized_at(BIG_CAMP, MARSH_HERD, 20.0);
    assert!(
        planned_crew(&empty_app) > 0,
        "liveness: the empty larder sends"
    );
    assert!(
        planned_crew(&stocked_app) > 0,
        "liveness: the stocked larder still sends"
    );
    assert!(
        stocked.food < empty.food,
        "the larder pays part of the meal"
    );
    assert_eq!(planned_crew(&empty_app), empty.crew);
    assert_eq!(planned_crew(&stocked_app), stocked.crew);
    assert!(
        planned_crew(&stocked_app) < planned_crew(&empty_app),
        "the smaller meal needs fewer hands"
    );
}

/// ⛔ **THE MEAL FLOORS AT ONE ANIMAL.** An animal is indivisible: a band that needs no food from the
/// kill (its larder covers the whole window) and is sent anyway — a margin wide enough that it is
/// always due — still sends the crew that brings down one.
#[test]
fn a_meal_of_nothing_still_calls_for_one_whole_animal() {
    let (mut app, band) = world_hunting(BIG_CAMP, MARSH_HERD);
    let mut config = (*app.world.resource::<FaunaConfigHandle>().get()).clone();
    config.follow.need_margin_turns = ALWAYS_DUE_MARGIN_TURNS;
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .replace(std::sync::Arc::new(config));
    set_meat(&mut app, band, MEAL_COVERING_LARDER);
    turn(&mut app);
    let sized = sizing(&mut app, band, MARSH_HERD);
    assert_eq!(sized.food, 0.0, "the larder covers the window");
    assert_eq!(sized.animals, 1.0, "one whole animal, no fewer");
    assert!(planned_crew(&app) > 0, "liveness: the band is sent");
    assert_eq!(planned_crew(&app), sized.crew);
    let (_, _, hungry) = sized_at(BIG_CAMP, MARSH_HERD, 0.0);
    assert!(
        planned_crew(&app) < hungry.crew,
        "one animal is a smaller crew than a real meal"
    );
}

/// ⛔ **A LONGER SHELF LIFE IS A BIGGER MEAL AND A BIGGER CREW** — the seam drying extends. The rack
/// is not built; a longer `flesh` shelf life in config stands in for it. The meal, the crew and the
/// wire's `huntMeals` all move together because every reading of a shelf life goes through
/// `spoilage::band_shelf_life`.
#[test]
fn a_longer_shelf_life_sends_a_bigger_crew() {
    let (short_app, _, short) = sized_at(BIG_CAMP, MARSH_HERD, 0.0);

    let (mut long_app, band) = world_hunting(BIG_CAMP, MARSH_HERD);
    let mut config = (*long_app
        .world
        .resource::<core_sim::DemographicsConfigHandle>()
        .get())
    .clone();
    for class in config.keeping.classes.iter_mut().filter(|c| c.id == FLESH) {
        class.shelf_life_turns *= DRIED_SHELF_FACTOR;
    }
    long_app
        .world
        .resource_mut::<core_sim::DemographicsConfigHandle>()
        .replace(std::sync::Arc::new(config));
    set_meat(&mut long_app, band, 0.0);
    turn(&mut long_app);
    let long = sizing(&mut long_app, band, MARSH_HERD);

    assert!(
        long.food > short.food,
        "meat that keeps longer is a bigger meal"
    );
    assert_eq!(planned_crew(&long_app), long.crew);
    assert!(
        planned_crew(&long_app) > planned_crew(&short_app),
        "a bigger meal sends a bigger crew ({} against {})",
        planned_crew(&long_app),
        planned_crew(&short_app)
    );
    assert!(
        planned_crew(&long_app) <= long.useful,
        "never above the useful crew"
    );
}

/// ⛔ **THE WIRE CARRIES THE MEAL THE TURN SIZES THE CREW WITH**, per keeping class, and the herd
/// names its class, so a compose sheet can size the crew before the row exists — and the
/// `musterCrew` it publishes is what the sheet's arithmetic gives.
#[test]
fn the_wire_carries_the_meal_per_keeping_class_and_the_herds_class() {
    let (app, _band, sized) = sized_at(BIG_CAMP, MARSH_HERD, 0.0);
    let decoded = decode_latest(&app);
    let cohort = decoded
        .populations
        .iter()
        .find(|cohort| cohort.band_id == BAND)
        .expect("the band is on the wire");
    let classes: Vec<String> = app
        .world
        .resource::<core_sim::DemographicsConfigHandle>()
        .get()
        .keeping
        .classes
        .iter()
        .map(|class| class.id.clone())
        .collect();
    assert_eq!(
        cohort
            .hunt_meals
            .iter()
            .map(|meal| meal.keeping.clone())
            .collect::<Vec<_>>(),
        classes,
        "one meal per configured keeping class, in table order"
    );
    let herd = decoded
        .herds
        .iter()
        .find(|herd| herd.id == NEED_HERD)
        .expect("the need herd is on the wire");
    assert_eq!(herd.keeping, FLESH, "the herd's meat lands in flesh");
    let flesh_meal = cohort
        .hunt_meals
        .iter()
        .find(|meal| meal.keeping == herd.keeping)
        .expect("the herd's class has a meal");
    assert!(
        (flesh_meal.food - sized.food).abs() <= sized.food * RISE_TOLERANCE,
        "the published meal ({}) is the oracle's ({})",
        flesh_meal.food,
        sized.food
    );
    // The sheet's arithmetic from the wire alone gives the plan the turn stored.
    let from_wire = (flesh_meal.food / herd.food_per_animal).ceil().max(1.0);
    assert_eq!(from_wire, sized.animals);
    let dry = cohort
        .hunt_meals
        .iter()
        .find(|meal| meal.keeping == STORE_CLASS)
        .expect("the dry class has a meal");
    assert!(
        dry.food > flesh_meal.food,
        "meat that keeps longer is a bigger meal"
    );
    assert_eq!(
        cohort
            .labor_assignments
            .iter()
            .find(|row| row.fauna_id == NEED_HERD)
            .expect("the row decodes")
            .muster_crew,
        sized.crew,
        "the wire's musterCrew is the sized crew"
    );
}

/// The meal's arithmetic on its own: whole animals, floored at one, and an animal that pays no food
/// is more than any crew lands.
#[test]
fn the_meal_counts_whole_animals_and_floors_at_one() {
    use core_sim::hunt_by_need::NeedMeal;
    let per_animal = 3.0;
    let meal = |food: f32| NeedMeal {
        food,
        food_per_animal: per_animal,
    };
    assert_eq!(meal(0.0).animals(), 1.0, "a meal of nothing is one animal");
    assert_eq!(meal(per_animal).animals(), 1.0);
    assert_eq!(
        meal(per_animal + 0.01).animals(),
        2.0,
        "a part animal is a whole one"
    );
    assert!(
        NeedMeal {
            food: per_animal,
            food_per_animal: 0.0
        }
        .animals()
        .is_infinite(),
        "an inedible herd never lands the meal"
    );
}

// ---------------------------------------------------------------------------------------------
// THE NEED CLOCK COUNTS ONLY THE FOOD THE KILL REPLACES
// ---------------------------------------------------------------------------------------------

/// Turns of the camp's meals held as meat, and how long preserved meat keeps: the stock outlasts the
/// time a hunt takes to land, and is still shorter-keeping than the dried reserve beside it.
const TURNS_OF_MEAT: f32 = 12.0;
const PRESERVED_MEAT_SHELF_LIFE: f32 = 20.0;
/// Turns watched while the meat is eaten down.
const TURNS_TO_EAT_THE_MEAT: usize = 24;

/// The `dry` class's shelf life as shipped - what `flesh` must keep for the stores to count.
fn stores_shelf_life(app: &App) -> f32 {
    app.world
        .resource::<core_sim::DemographicsConfigHandle>()
        .get()
        .keeping
        .shelf_life(STORE_CLASS)
        .expect("the stores' class is shipped")
}

/// A band with a deep dried reserve and no meat at all.
fn dried_reserve_only(stock: Stock) -> (App, Entity) {
    let (mut app, band) = world_hunting(stock, MAMMOTH_HERD);
    set_larder(&mut app, band, FAT_LARDER);
    (app, band)
}

/// ⛔ **A BAND ARRIVING WITH A DRIED RESERVE AND NO MEAT HUNTS AT ONCE.** `dry` keeps far longer than
/// the mammoth's `flesh`, so it is a reserve the kill does not stand in for: the need clock reads the
/// meat on hand - none - and the first hunt goes out, where the band's own runway (`turnsOfFood`) is
/// still long.
#[test]
fn a_dried_reserve_with_no_meat_sends_a_crew_at_once() {
    let (mut app, band) = dried_reserve_only(Stock::ROOMY);
    turn(&mut app);
    let wire = published(&app);
    let need = wire.row(NEED_HERD);
    assert!(need.muster_crew > 0, "no meat: the first hunt goes out now");
    assert_eq!(need.turns_until_hunt, 0);
    assert!(
        need.need_runway < 1.0,
        "the meat on hand is under a turn: {}",
        need.need_runway
    );
    assert!(
        wire.turns_of_food > 1.0 + need.need_runway,
        "control: the band's own runway still counts the reserve ({} turns)",
        wire.turns_of_food
    );
    let _ = band;
}

/// ⛔ **WITH MEAT ON HAND THE BAND WAITS, AND HUNTS AGAIN AS THE MEAT RUNS LOW - NOT WHEN THE RESERVE
/// DOES.** The people eat the meat first (fastest-keeping class first); the clock waits while it
/// lasts and goes out while the dried stock is still deep.
#[test]
fn with_meat_on_hand_the_band_waits_and_hunts_as_the_meat_runs_low() {
    let (mut app, band) = dried_reserve_only(Stock::FEW_DONOR_HANDS);
    set_flesh_shelf_life(&mut app, PRESERVED_MEAT_SHELF_LIFE);
    assert!(PRESERVED_MEAT_SHELF_LIFE < stores_shelf_life(&app));
    let eaten_per_turn = meal(&app, band);
    let mut meat = eaten_per_turn * TURNS_OF_MEAT;
    let mut waited = false;
    let mut sent_at = None;
    for step in 0..TURNS_TO_EAT_THE_MEAT {
        // The people eat the meat first; the donors' kills are not banked, so the meat is only
        // what is left of the stock the band began with.
        meat = (meat - eaten_per_turn).max(0.0);
        {
            let mut cohort = app
                .world
                .get_mut::<PopulationCohort>(band)
                .expect("the band keeps its cohort");
            cohort.stores = LocalStore::new();
            cohort
                .stores
                .add_food(STORE_CLASS, scalar_from_f32(FAT_LARDER));
            cohort.stores.add_food(FLESH, scalar_from_f32(meat));
        }
        turn(&mut app);
        let need = published(&app).row(NEED_HERD).clone();
        if need.muster_crew > 0 {
            sent_at = Some(step);
            break;
        }
        waited = true;
        assert!(
            need.need_runway > 1.0,
            "turn {step}: the meat still lasts ({} turns)",
            need.need_runway
        );
    }
    assert!(waited, "it waited while the meat lasted");
    assert!(sent_at.is_some(), "liveness: it hunted as the meat ran low");
    assert!(
        meat < eaten_per_turn * TURNS_OF_MEAT * 0.5,
        "it went out with the meat low, not at the start"
    );
    assert!(
        larder(&app, band).to_f32() > FAT_LARDER * 0.9,
        "the dried reserve is still deep when the hunt goes out"
    );
}

/// ⛔ **WHEN THE KILL KEEPS AS LONG AS THE RESERVE, THE RESERVE COUNTS** - the seam drying extends. A
/// `flesh` shelf life stretched to `dry`'s (a stand-in for the rack) makes the same fat reserve the
/// food the kill replaces, so the band that hunted at once above is not food-limited at all.
#[test]
fn a_kill_that_keeps_as_long_as_the_reserve_counts_the_reserve() {
    let (mut app, band) = dried_reserve_only(Stock::ROOMY);
    meat_keeps_like_the_stores(&mut app);
    let _ = stores_shelf_life(&app);
    turn(&mut app);
    let need = published(&app).row(NEED_HERD).clone();
    assert_eq!(need.muster_crew, 0, "the reserve counts: nothing is due");
    assert!(
        need.turns_until_hunt > MANY_TURNS_AWAY,
        "a deep reserve is a long wait ({} turns)",
        need.turns_until_hunt
    );
    let _ = band;
}

// ---------------------------------------------------------------------------------------------
// KIT-AWARE DONORS
// ---------------------------------------------------------------------------------------------

/// Hands the gathering donor and the stalking donor hold, and the crew the tests plan.
const STALKING_HANDS: u32 = NORMAL_DONOR_HANDS;
/// Hands the muster must take from rows after idle hands are used up.
const HANDS_FROM_ROWS: u32 = 2;
/// Items a band holds of the stalking kit when it is SHORT: fewer than its own boar row claims plus
/// the crew would, but enough to arm the boar hunters it has.
const SHORT_STALKING_UNITS: u32 = STALKING_HANDS + 1;
const ENOUGH_UNITS: u32 = 100;
const GATHERING_KIT: &str = "gathering";

/// Give the band exactly this gear, and put the Low donor row on the gathering kit.
fn arm_the_band(app: &mut App, band: Entity, stalking_units: u32) {
    arm_the_band_with(app, band, stalking_units, stalking_units);
}

/// [`arm_the_band`] with the stalking kit's two items counted apart: `spears` of the weapon and
/// `sleds` of the hauler.
fn arm_the_band_with(app: &mut App, band: Entity, spears: u32, sleds: u32) {
    let mut kit = BandEquipment::default();
    if spears > 0 {
        kit.stock("spears", spears, "plain", None);
    }
    if sleds > 0 {
        kit.stock("sled", sleds, "plain", None);
    }
    kit.stock("baskets", ENOUGH_UNITS, "plain", None);
    *app.world
        .get_mut::<BandEquipment>(band)
        .expect("the band keeps its ledger") = kit;
    let gathering = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get()
        .kit(GATHERING_KIT)
        .expect("the gathering kit is shipped");
    let low = index_of(app, band, LOW_DONOR_HERD);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .assignments[low]
        .kit = Some(gathering);
}

/// Plan a crew that needs `HANDS_FROM_ROWS` hands from the donor rows after every idle one.
fn plan_a_crew_past_the_idle(app: &mut App, band: Entity) {
    recapture_snapshot_in_place(&mut app.world);
    let idle = published(app).idle_workers;
    set_muster(app, band, idle + HANDS_FROM_ROWS);
}

/// ⛔ **WHEN THE HUNT'S KIT IS SHORT, THE DONOR HOLDING IT LENDS FIRST.** A Low gathering row and a
/// Normal stalking row; the band holds barely more spears than the stalking row uses. Priority alone
/// would take the gatherers' hands and leave the crew to claim stalking kits off the budget, stripping
/// the stalking hunters of their gear while they keep their hands. The stalking row lends instead, so
/// its kit goes to the crew with it.
#[test]
fn when_the_hunts_kit_is_short_the_donor_holding_it_lends_first() {
    let (mut app, band) = world(Stock::ROOMY);
    arm_the_band(&mut app, band, SHORT_STALKING_UNITS);
    plan_a_crew_past_the_idle(&mut app, band);
    let wire = published(&app);
    // The kit is short by the crew less the one free set, and the stalking row holds `STALKING_HANDS`
    // of it: it lends all of them ahead of the bare idle hands, which fill the rest of the crew.
    assert_eq!(
        wire.row(NORMAL_DONOR_HERD).lent_to_hunt,
        STALKING_HANDS,
        "the stalking row lends first, though it is the higher Priority"
    );
    assert_eq!(wire.row(LOW_DONOR_HERD).lent_to_hunt, 0);
    assert_eq!(
        wire.idle_mustered,
        wire.idle_workers - (STALKING_HANDS - HANDS_FROM_ROWS),
        "the idle hands give way to the kit-carrying donor"
    );
}

/// The control: with the kit not short, the order is Priority alone.
#[test]
fn when_the_kit_is_not_short_the_lowest_priority_lends_first() {
    let (mut app, band) = world(Stock::ROOMY);
    arm_the_band(&mut app, band, ENOUGH_UNITS);
    plan_a_crew_past_the_idle(&mut app, band);
    let wire = published(&app);
    assert_eq!(wire.row(LOW_DONOR_HERD).lent_to_hunt, HANDS_FROM_ROWS);
    assert_eq!(wire.row(NORMAL_DONOR_HERD).lent_to_hunt, 0);
}

/// The turn lends what the board published: the stalking row, with its kit, is what is missing from
/// the donors' work - read against a control turn with nobody mustered.
#[test]
fn the_turn_lends_the_donors_the_board_named() {
    let (mut mustered, band_m) = world(Stock::ROOMY);
    arm_the_band(&mut mustered, band_m, SHORT_STALKING_UNITS);
    plan_a_crew_past_the_idle(&mut mustered, band_m);
    let (mut control, band_c) = world(Stock::ROOMY);
    arm_the_band(&mut control, band_c, SHORT_STALKING_UNITS);
    run_one_turn_fed(&mut mustered, band_m);
    run_one_turn_fed(&mut control, band_c);
    assert_ne!(
        taken(&mustered, band_m, NORMAL_DONOR_HERD),
        taken(&control, band_c, NORMAL_DONOR_HERD),
        "the stalking row really did give hands up on the turn"
    );
    assert_eq!(
        taken(&mustered, band_m, LOW_DONOR_HERD),
        taken(&control, band_c, LOW_DONOR_HERD),
        "the gathering row was left alone"
    );
    assert_eq!(hands(&mustered, band_m), hands(&control, band_c));
}

// ---------------------------------------------------------------------------------------------
// CRAFT SUGGESTIONS SEE THE HUNT TURN
// ---------------------------------------------------------------------------------------------

fn suggested(app: &App, item: &str) -> Option<u32> {
    decode_latest(app)
        .populations
        .iter()
        .find(|cohort| cohort.band_id == BAND)
        .expect("the band is on the wire")
        .craft_suggestions
        .iter()
        .find(|suggestion| suggestion.item_id == item)
        .map(|suggestion| suggestion.count)
}

/// Small game with no defence: bare hands still take it, so the crew the curve prices is wider than
/// the spears the band holds, and the whole crew claims the stalking kit.
const RABBIT_BODY_MASS: f32 = 0.27;
const RABBIT_HEAD_COUNT: f32 = 2000.0;
const RABBITS: Quarry = Quarry {
    species: "Rabbit Warren",
    body_mass: RABBIT_BODY_MASS,
    stock: RABBIT_HEAD_COUNT * RABBIT_BODY_MASS,
};

/// Small game bare hands CANNOT wound (Desert Gazelle, defense 1 against the bare hunter's attack
/// 1, so the gate deals nothing): the stalking kit is what clears it, so a kit shortage caps the
/// crew. The fixture every equipment-cap test hunts, beside [`RABBITS`] where bare hands already
/// take the animal and no shortage caps anything.
const GAZELLE_BODY_MASS: f32 = 3.3;
const GAZELLE_HEAD_COUNT: f32 = 2000.0;
const DEFENDED_GAME: Quarry = Quarry {
    species: "Desert Gazelle",
    body_mass: GAZELLE_BODY_MASS,
    stock: GAZELLE_HEAD_COUNT * GAZELLE_BODY_MASS,
};

/// A camp small enough that the idle hands cannot make the crew: both donors lend.
const CREW_NEEDS_THE_DONORS: Stock = Stock {
    working: 5 + LOW_DONOR_HANDS + NORMAL_DONOR_HANDS,
    ..Stock::ROOMY
};

/// ⛔ **A HUNT CREW'S MISSING KIT IS SUGGESTED, THOUGH EVERY STANDING ROW IS FULLY KITTED.** A need
/// row stands at zero hands, so the standing pass never sees what its crew would claim. The band holds
/// exactly the spears its stalking rows use; the mustered crew would need many more, and the shortfall
/// is the people mustered less the spears held - the stalking donors' own kit moving to the crew,
/// never counted twice. The quarry is one bare hands wound, so the whole crew goes out (the
/// unarmed hands bare) and the suggestions are what arms them.
#[test]
fn a_hunt_crews_missing_kit_is_suggested_though_the_standing_rows_are_kitted() {
    let (mut app, band) = world_hunting(CREW_NEEDS_THE_DONORS, RABBITS);
    arm_the_band(&mut app, band, STALKING_HANDS);
    set_meat(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    let sent = row(&app, band, NEED_HERD).muster_crew;
    let wanted = row(&app, band, NEED_HERD).planned_crew;

    assert_eq!(
        published(&app).row(NORMAL_DONOR_HERD).lent_to_hunt,
        STALKING_HANDS,
        "liveness: the stalking donor lends, so its kit moves to the crew"
    );
    assert_eq!(
        sent, wanted,
        "bare hands take this quarry, so the kit shortage does not cap the crew"
    );
    assert!(
        wanted > STALKING_HANDS,
        "liveness: the hunt wants more hands than the band has spears"
    );

    // The suggestions equip toward the crew the hunt WANTS.
    let spears = suggested(&app, "spears").expect("the crew's spears are suggested");
    assert!(spears > 0);
    assert!(
        spears + STALKING_HANDS <= wanted,
        "{spears} suggested over the {STALKING_HANDS} held cannot exceed the {wanted} wanted: the stalking donor's own kit is not counted twice"
    );
    // The control: the same band with the crew's gear in hand suggests nothing.
    let (mut armed, band_a) = world_hunting(CREW_NEEDS_THE_DONORS, RABBITS);
    arm_the_band(&mut armed, band_a, ENOUGH_UNITS);
    set_meat(&mut armed, band_a, LEAN_LARDER);
    turn(&mut armed);
    assert!(row(&armed, band_a, NEED_HERD).muster_crew > 0);
    assert_eq!(suggested(&armed, "spears"), None);
}

// ---------------------------------------------------------------------------------------------
// THE GEAR THE MUSTER BRINGS
// ---------------------------------------------------------------------------------------------

/// Idle hands left once both donor rows are staffed: fewer than the spears the stalking donor holds,
/// so a crew that arms anyone has to borrow from it.
const FEW_IDLE_HANDS: u32 = 2;
/// A camp whose every spare hand is on a donor row, with `FEW_IDLE_HANDS` idle.
const IDLE_HANDS_ARE_FEW: Stock = Stock {
    working: FEW_IDLE_HANDS + LOW_DONOR_HANDS + NORMAL_DONOR_HANDS,
    ..Stock::ROOMY
};

/// The band with every stalking kit claimed: the boar row is `High`, so it is served its spears
/// before any row that might ask for them, a new hunt included.
fn every_stalking_kit_is_claimed(app: &mut App, band: Entity) {
    arm_the_band(app, band, STALKING_HANDS);
    let boar = index_of(app, band, NORMAL_DONOR_HERD);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .assignments[boar]
        .priority = SourcePriority::High;
}

/// The crew-take curve the Assign hunters sheet asks for the need row's herd.
fn sheet_curve(
    app: &mut App,
    band: Entity,
    follow: bool,
) -> Vec<sim_runtime::commands::HuntCrewTakeRow> {
    let pool = musterable_pool(app, band);
    sheet_curve_asking(app, follow, pool)
}

/// The same ask with a `follow` ceiling of the caller's choosing.
fn sheet_curve_capped(app: &mut App, ceiling: u32) -> Vec<sim_runtime::commands::HuntCrewTakeRow> {
    sheet_curve_asking(app, true, ceiling)
}

fn sheet_curve_asking(
    app: &mut App,
    follow: bool,
    pool: u32,
) -> Vec<sim_runtime::commands::HuntCrewTakeRow> {
    let kit_id = published(app).row(NEED_HERD).kit_id.clone();
    let reply = core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &sim_runtime::commands::QueryPayload::HuntCrewTake(
            sim_runtime::commands::HuntCrewTakeQuery {
                faction_id: FACTION.0,
                band_id: BAND,
                herd_id: NEED_HERD.to_string(),
                kit_id,
                floor: FLOOR,
                max_workers: pool,
                follow,
            },
        ),
    );
    let sim_runtime::commands::QueryReply::HuntCrewTake(curve) = reply else {
        panic!("the crew-take query answers with a curve");
    };
    curve.per_crew
}

/// ⛔ **A CREW IS PRICED OVER THE GEAR THE MUSTER BRINGS, NOT OVER THE UNCLAIMED UNITS ALONE.** Marsh
/// Grazers have defense 3, so bare hands cannot wound one. Every spear the band owns is claimed by
/// its stalking row, so the free units arm nobody and a curve priced over them reads zero at every
/// crew - useful crew 0, the hunt never goes out. A hunter lent from the stalking row brings his
/// spear with him, so the crew is armed by exactly the hands the muster takes from that row.
#[test]
fn a_quarry_bare_hands_cannot_wound_is_hunted_with_the_kit_the_donors_bring() {
    let (mut app, band) = world_hunting(IDLE_HANDS_ARE_FEW, MARSH_HERD);
    every_stalking_kit_is_claimed(&mut app, band);
    set_meat(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    let wire = published(&app);
    let need = wire.row(NEED_HERD);

    assert!(
        row(&app, band, NEED_HERD).muster_crew > 0,
        "the need row sends a crew"
    );
    assert_eq!(
        need.muster_crew,
        row(&app, band, NEED_HERD).muster_crew,
        "the wire publishes the plan the turn made"
    );
    assert!(
        need.hunt_useful_workers > 0,
        "the useful crew is priced over the gear the muster brings"
    );
    assert!(
        need.turns_to_kill > 0,
        "the animal can be brought down, so there is a finite wait"
    );
    assert!(
        wire.row(NORMAL_DONOR_HERD).lent_to_hunt > 0,
        "the stalking row lends: its spears are the ones that arm the crew"
    );
    assert!(
        wire.row(NORMAL_DONOR_HERD).lent_to_hunt >= wire.row(LOW_DONOR_HERD).lent_to_hunt,
        "the row holding the kit lends before the one that does not"
    );
}

/// The sheet's curve agrees with the row: asked with `follow` it prices the same muster, and without
/// it, the free units alone - which arm nobody on this band.
#[test]
fn the_sheet_prices_a_need_row_over_the_muster_when_it_asks_with_follow() {
    let (mut app, band) = world_hunting(IDLE_HANDS_ARE_FEW, MARSH_HERD);
    every_stalking_kit_is_claimed(&mut app, band);
    set_meat(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    let crew = row(&app, band, NEED_HERD).muster_crew;

    let with_follow = sheet_curve(&mut app, band, true);
    let without = sheet_curve(&mut app, band, false);

    assert!(
        without.iter().all(|row| row.animals_likely == 0.0),
        "liveness of the control: priced over the free units, nothing can be wounded"
    );
    // An over-large ceiling (the client sends the whole workforce) is clamped to the pool.
    let pool = musterable_pool(&app, band);
    let over = sheet_curve_capped(&mut app, pool + 50);
    assert_eq!(
        over.len(),
        pool as usize,
        "the ask is clamped to the musterable pool"
    );
    let at_crew = with_follow
        .get(crew as usize - 1)
        .expect("the curve reaches the crew the row sends");
    assert!(
        at_crew.animals_likely > 0.0,
        "the crew the row sends lands animals on the sheet's curve too"
    );
}

// ---------------------------------------------------------------------------------------------
// THE NEED ROW'S OWN YIELD IS NOT STANDING INCOME
// ---------------------------------------------------------------------------------------------

/// Meals of meat left after the kill: well inside `flesh`'s shelf life, so the runway is a few turns.
const MEALS_OF_MEAT_AFTER_THE_KILL: f32 = 3.0;
/// Turns the harness lets a hunt go out and land.
const TURNS_TO_LAND_A_KILL: usize = 4;

/// ⛔ **A NEED ROW'S OWN YIELD IS NEVER STANDING INCOME.** On a hunt turn the labor pass lends the
/// row a one-turn crew and projects its `realized` and arrivals as if that crew worked the herd every
/// turn. Counted as income, a kill that just landed reads as a steady supply: the need clock sees net
/// eating near nothing and a long runway, and the next hunt waits a turn. The runway after a kill must
/// be the one the band would read with the row at zero.
#[test]
fn the_need_rows_own_projection_is_not_income_to_the_need_clock() {
    let (mut app, band) = world_hunting(Stock::FEW_DONOR_HANDS, RABBITS);
    set_meat(&mut app, band, LEAN_LARDER);
    let need = index_of(&app, band, NEED_HERD);

    let mut killed = false;
    for _ in 0..TURNS_TO_LAND_A_KILL {
        turn(&mut app);
        if allocation(&app, band).last_yields[need].realized > 0.0 {
            killed = true;
            break;
        }
    }
    assert!(
        killed,
        "liveness: the need row's crew went out and its projection is positive"
    );
    // The kill is in the larder; leave a few meals of it, so the runway is short but real.
    let meals = meal(&app, band);
    set_meat(&mut app, band, MEALS_OF_MEAT_AFTER_THE_KILL * meals);
    recapture_snapshot_in_place(&mut app.world);
    let with_projection = published(&app).row(NEED_HERD).need_runway;
    let turns_until = published(&app).row(NEED_HERD).turns_until_hunt;

    // The same band with the need row's projection erased - the row at zero hands.
    let zero = allocation(&app, band).last_yields[need].clone();
    assert!(zero.realized > 0.0);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_yields[need] = core_sim::SourceYield::ZERO;
    recapture_snapshot_in_place(&mut app.world);
    let at_zero = published(&app).row(NEED_HERD).need_runway;

    assert_eq!(
        with_projection, at_zero,
        "the runway ignores the need row's own projected yield (turns until hunt read {turns_until})"
    );
}

// ---------------------------------------------------------------------------------------------
// AUTO-CRAFT READS THE HUNT-TURN PASS
// ---------------------------------------------------------------------------------------------

/// Materials enough for any recipe the bench queues here.
const PLENTY_OF_MATERIAL: f32 = 40.0;

fn bank_material(app: &mut App, band: Entity, material: &str, axes: &[(&str, f32)]) {
    let materials = app
        .world
        .resource::<core_sim::MaterialsConfigHandle>()
        .get();
    let readings: std::collections::BTreeMap<String, f32> = axes
        .iter()
        .map(|(axis, value)| ((*axis).to_string(), *value))
        .collect();
    let key = materials
        .band_key(material, &readings)
        .expect("the shipped table carries this material");
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .stores
        .deposit_material(
            material,
            key,
            core_sim::scalar_from_f32(PLENTY_OF_MATERIAL),
            &readings,
        );
}

/// Give the band an empty auto bench and every material a spear needs.
fn auto_bench_with_materials(app: &mut App, band: Entity) {
    bank_material(app, band, "bone", &[("density", 0.5), ("length", 0.5)]);
    bank_material(app, band, "fibre", &[("fineness", 0.5), ("strength", 0.5)]);
    bank_material(
        app,
        band,
        "hide",
        &[("toughness", 0.6), ("suppleness", 0.5)],
    );
    app.world
        .entity_mut(band)
        .insert(core_sim::BandBench::default());
    core_sim::auto_craft::set_auto_craft(&mut app.world, band, true);
}

/// A recipe making what the stalking kit is made of (spears or the sled).
fn is_stalking_gear(recipe: &str) -> bool {
    recipe.starts_with("spears") || recipe.starts_with("sled")
}

fn queued_recipes(app: &App, band: Entity) -> Vec<String> {
    app.world
        .get::<core_sim::BandBench>(band)
        .expect("the band keeps its bench")
        .orders
        .iter()
        .map(|order| order.recipe_id.clone())
        .collect()
}

/// ⛔ **AUTO-CRAFT QUEUES THE KIT A GOING-OUT HUNT CREW IS SHORT OF.** Every standing row is fully
/// kitted, so the standing list is empty and an auto bench would sit idle; the hunt-turn pass makes
/// spears the panel's top suggestion, and auto-craft reads the same list (`band_suggestions`), so it
/// queues them. The control is the same band with the crew's gear in hand: nothing to make, nothing
/// queued.
#[test]
fn auto_craft_queues_the_kit_a_hunt_crew_is_short_of() {
    let (mut app, band) = world_hunting(CREW_NEEDS_THE_DONORS, RABBITS);
    arm_the_band(&mut app, band, STALKING_HANDS);
    set_meat(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    assert!(
        row(&app, band, NEED_HERD).muster_crew > 0,
        "liveness: a crew is going out"
    );
    assert!(
        suggested(&app, "spears").is_some(),
        "the panel suggests spears"
    );

    auto_bench_with_materials(&mut app, band);
    let queued = queued_recipes(&app, band);
    assert!(
        queued.iter().any(|recipe| is_stalking_gear(recipe)),
        "auto-craft queued the hunt crew's stalking gear, got {queued:?}"
    );

    let (mut armed, band_a) = world_hunting(CREW_NEEDS_THE_DONORS, RABBITS);
    arm_the_band(&mut armed, band_a, ENOUGH_UNITS);
    set_meat(&mut armed, band_a, LEAN_LARDER);
    turn(&mut armed);
    auto_bench_with_materials(&mut armed, band_a);
    assert!(
        !queued_recipes(&armed, band_a)
            .iter()
            .any(|recipe| is_stalking_gear(recipe)),
        "control: a crew with its gear in hand queues no stalking gear"
    );
}

// ---------------------------------------------------------------------------------------------
// THE KIT IS FLAGGED AHEAD OF THE HUNT: A WAITING ROW'S PLANNED CREW
// ---------------------------------------------------------------------------------------------

/// The band holds no stalking kit and neither donor row uses one, so the only crew that could claim
/// spears is the need row's: nothing standing is short, and nothing can be sent.
fn hold_no_spears_and_stalk_nowhere(app: &mut App, band: Entity) {
    arm_the_band(app, band, 0);
    gather_with_the_normal_donor(app, band);
}

/// Put the `Normal` donor row on the gathering kit, as [`arm_the_band`] does the `Low` one.
fn gather_with_the_normal_donor(app: &mut App, band: Entity) {
    let gathering = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get()
        .kit(GATHERING_KIT)
        .expect("the gathering kit is shipped");
    let normal = index_of(app, band, NORMAL_DONOR_HERD);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .assignments[normal]
        .kit = Some(gathering);
}

/// ⛔ **A WAITING NEED ROW'S CREW IS EQUIPPED AHEAD OF THE HUNT.** With meat in the larder the row
/// waits (`muster_crew` 0) but the labor pass still prices the crew it would send (`planned_crew`),
/// and the craft suggestions - and auto-craft, which reads the same list - flag the kit that crew
/// would claim now, so the band can make it before the hunt goes out. A row with no planned crew adds
/// nothing.
#[test]
fn a_waiting_rows_planned_crew_is_equipped_ahead_of_the_hunt() {
    let (mut app, band) = world_hunting(CREW_NEEDS_THE_DONORS, RABBITS);
    hold_no_spears_and_stalk_nowhere(&mut app, band);
    set_meat(&mut app, band, COMFORTABLE_LARDER);
    turn(&mut app);
    set_meat(&mut app, band, COMFORTABLE_LARDER);
    turn(&mut app);
    let need = row(&app, band, NEED_HERD);
    assert_eq!(need.muster_crew, 0, "liveness: the row is waiting");
    assert!(need.planned_crew > 0, "the crew it would send is planned");
    assert!(
        suggested(&app, "spears").is_some(),
        "the planned crew's spears are suggested before the hunt goes out"
    );

    auto_bench_with_materials(&mut app, band);
    let queued = queued_recipes(&app, band);
    assert!(
        queued.iter().any(|recipe| is_stalking_gear(recipe)),
        "auto-craft queues the planned crew's gear ahead of the hunt, got {queued:?}"
    );

    // The control: the same band with no planned crew adds nothing.
    let (mut none, band_n) = world_hunting(CREW_NEEDS_THE_DONORS, RABBITS);
    hold_no_spears_and_stalk_nowhere(&mut none, band_n);
    set_meat(&mut none, band_n, COMFORTABLE_LARDER);
    turn(&mut none);
    let need_n = index_of(&none, band_n, NEED_HERD);
    none.world
        .get_mut::<LaborAllocation>(band_n)
        .expect("the band keeps its allocation")
        .assignments[need_n]
        .planned_crew = 0;
    recapture_snapshot_in_place(&mut none.world);
    assert_eq!(suggested(&none, "spears"), None);
    auto_bench_with_materials(&mut none, band_n);
    assert!(
        !queued_recipes(&none, band_n)
            .iter()
            .any(|recipe| is_stalking_gear(recipe)),
        "control: no planned crew, nothing to equip"
    );
}

// ---------------------------------------------------------------------------------------------
// THE CRAFTING BENCH LENDS LIKE A ROW
// ---------------------------------------------------------------------------------------------

/// Hands at the crafting bench in the bench-donor worlds.
const BENCH_CREW: u32 = 4;
/// The bench's job: the sled (8 work; hide and fibre, both banked by the fixture).
const BENCH_RECIPE: &str = "sled";
/// An order the fixture never finishes, so a turn's progress is never a completion.
const UNFINISHED_ORDER: u32 = 1_000;
/// A band with no idle hands: the two donor rows and the bench hold everybody.
const NO_IDLE_HANDS: Stock = Stock {
    working: LOW_DONOR_HANDS + NORMAL_DONOR_HANDS + BENCH_CREW,
    ..Stock::ROOMY
};

/// The no-idle-hands world with a staffed bench of the given mark, its sled materials banked.
fn bench_world(priority: SourcePriority) -> (App, Entity) {
    let (mut app, band) = world(NO_IDLE_HANDS);
    bank_material(
        &mut app,
        band,
        "hide",
        &[("toughness", 0.6), ("suppleness", 0.5)],
    );
    bank_material(
        &mut app,
        band,
        "fibre",
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    app.world.entity_mut(band).insert(core_sim::BandBench {
        orders: vec![core_sim::BenchOrder::new(BENCH_RECIPE, UNFINISHED_ORDER)],
        workers: BENCH_CREW,
        priority,
        ..Default::default()
    });
    recapture_snapshot_in_place(&mut app.world);
    (app, band)
}

/// The bench's `lentToHunt` on the ENCODED frame.
fn published_bench_lent(app: &App) -> u32 {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == BAND)
        .and_then(|cohort| cohort.bench())
        .expect("the band publishes its bench")
        .lentToHunt()
}

fn bench_of(app: &App, band: Entity) -> &core_sim::BandBench {
    app.world
        .get::<core_sim::BandBench>(band)
        .expect("the band keeps its bench")
}

/// **A TURN WITH THE BENCH IN IT**: the labor pass lends and restores, then the bench works, in the
/// Population chain's order.
fn turn_with_bench(app: &mut App) {
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(follow_hunted_herds);
    app.world.run_system_once(advance_band_movement);
    app.world.run_system_once(advance_labor_allocation);
    app.world.run_system_once(core_sim::advance_crafting);
    recapture_snapshot_in_place(&mut app.world);
}

/// What the wire says the bench and each row give up for a crew of `crew`, as
/// `(bench, normal row, low row)`.
fn lent_for(priority: SourcePriority, crew: u32) -> (u32, u32, u32) {
    let (mut app, band) = bench_world(priority);
    set_muster(&mut app, band, crew);
    let wire = published(&app);
    (
        published_bench_lent(&app),
        wire.row(NORMAL_DONOR_HERD).lent_to_hunt,
        wire.row(LOW_DONOR_HERD).lent_to_hunt,
    )
}

/// ⛔ **THE BENCH IS A DONOR, RANKED BY ITS OWN MARK AND BEFORE ITS TIER'S ROWS** — on the encoded
/// frame, with no idle hand to spend.
#[test]
fn the_bench_lends_by_its_mark_and_before_the_rows_of_its_tier() {
    // A Low bench lends before a Normal row (and before the Low row beside it).
    assert_eq!(
        lent_for(SourcePriority::Low, BENCH_CREW),
        (BENCH_CREW, 0, 0),
        "a Low bench lends first of all"
    );
    // A Normal bench lends before the Normal row, after the Low row.
    assert_eq!(
        lent_for(SourcePriority::Normal, LOW_DONOR_HANDS + 2),
        (2, 0, LOW_DONOR_HANDS),
        "Low row whole, then the Normal bench ahead of the Normal row"
    );
    // A High bench lends only once every Low and Normal row is exhausted.
    assert_eq!(
        lent_for(SourcePriority::High, LOW_DONOR_HANDS + NORMAL_DONOR_HANDS),
        (0, NORMAL_DONOR_HANDS, LOW_DONOR_HANDS),
        "the High bench lends nothing while a lower row still has hands"
    );
    assert_eq!(
        lent_for(
            SourcePriority::High,
            LOW_DONOR_HANDS + NORMAL_DONOR_HANDS + 1
        ),
        (1, NORMAL_DONOR_HANDS, LOW_DONOR_HANDS),
        "then it lends"
    );
}

/// What one turn of the bench leaves with `lent` of its `BENCH_CREW` hands out hunting, read off the
/// bench itself: `(progress, drawn, crew after the turn, hide left in the store)`.
fn bench_turn_with_lent(lent: u32) -> (f32, bool, u32, f32) {
    let (mut app, band) = bench_world(SourcePriority::Low);
    set_muster(&mut app, band, lent);
    assert_eq!(
        published_bench_lent(&app),
        lent,
        "the board states the hands the turn will take"
    );
    turn_with_bench(&mut app);
    let bench = bench_of(&app, band);
    let hide_left = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .stores
        .material_total("hide")
        .to_f32();
    (
        bench.orders[0].progress.to_f32(),
        bench.orders[0].drawn.is_some(),
        bench.workers,
        hide_left,
    )
}

/// ⛔ **THIS TURN'S CRAFT REFLECTS THE LENT HANDS, AND THE CREW COMES BACK.** The bench runs after
/// the labor pass has handed the crew back, so the lent hands are recorded on the bench and the
/// craft pass works `workers − lent`. Lending half the crew banks half the progress; lending all of
/// it banks nothing and draws nothing; the full crew is back on the bench afterwards either way.
#[test]
fn this_turns_craft_progress_reflects_the_lent_hands() {
    let (whole, whole_drawn, whole_crew, hide_whole) = bench_turn_with_lent(0);
    assert!(
        whole > 0.0 && whole_drawn,
        "liveness: an unlent bench banks and draws"
    );
    assert_eq!(whole_crew, BENCH_CREW);

    let (half, half_drawn, half_crew, _) = bench_turn_with_lent(BENCH_CREW / 2);
    assert!(half_drawn, "a half-crewed bench still draws");
    assert!(
        (half * 2.0 - whole).abs() < 1e-4,
        "half the crew banks half the progress: {half} against {whole}"
    );
    assert_eq!(half_crew, BENCH_CREW, "the crew is back after the turn");

    let (none, none_drawn, none_crew, hide_left) = bench_turn_with_lent(BENCH_CREW);
    assert_eq!(none, 0.0, "a fully lent bench banks nothing");
    assert!(!none_drawn, "and draws nothing: the zero-crew gate holds");
    assert_eq!(none_crew, BENCH_CREW, "the whole crew is restored");
    assert!(
        hide_left > hide_whole,
        "the fully lent bench left the store alone ({hide_left} against {hide_whole})"
    );
}

/// ⛔ **THE LOAN IS NOT A REMOVAL**: the queue, the order's progress and its drawn pile all survive
/// a turn with the whole crew out hunting, and the crew works again the turn after.
#[test]
fn a_lent_bench_keeps_its_order_progress_and_drawn_pile() {
    let (mut app, band) = bench_world(SourcePriority::Low);
    turn_with_bench(&mut app);
    let before = bench_of(&app, band).orders[0].clone();
    assert!(
        before.progress.to_f32() > 0.0 && before.drawn.is_some(),
        "liveness: the first turn drew a pile and banked progress"
    );

    set_muster(&mut app, band, BENCH_CREW);
    turn_with_bench(&mut app);
    let during = bench_of(&app, band);
    assert_eq!(during.orders.len(), 1, "the queue is intact");
    assert_eq!(during.orders[0].recipe_id, BENCH_RECIPE);
    assert_eq!(
        during.orders[0].progress, before.progress,
        "no progress is lost or gained"
    );
    assert_eq!(during.orders[0].drawn, before.drawn, "the drawn pile stays");
    assert_eq!(during.workers, BENCH_CREW, "the crew is restored");
    assert_eq!(during.lent_to_hunt, 0, "the loan does not outlive the turn");

    set_muster(&mut app, band, 0);
    turn_with_bench(&mut app);
    assert!(
        bench_of(&app, band).orders[0].progress > before.progress,
        "the crew works again the turn after"
    );
}

/// The bench's `ratePerTurn` on the ENCODED frame.
fn published_bench_rate(app: &App) -> f32 {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == BAND)
        .and_then(|cohort| cohort.bench())
        .expect("the band publishes its bench")
        .ratePerTurn()
}

/// ⛔ **THE QUOTED RATE IS THE PROGRESS THE TURN BANKS, LOAN INCLUDED.** `ratePerTurn` describes next
/// turn, and next turn the bench works `workers − lentToHunt`: the published rate, read off the
/// encoded frame before the turn, equals the progress the turn then banks — whole, half lent, and
/// fully lent (quoted `0`, banks `0`).
#[test]
fn the_quoted_rate_is_the_progress_the_turn_banks_with_hands_lent() {
    let mut quoted = Vec::new();
    for lent in [0, BENCH_CREW / 2, BENCH_CREW] {
        let (mut app, band) = bench_world(SourcePriority::Low);
        set_muster(&mut app, band, lent);
        assert_eq!(published_bench_lent(&app), lent, "the plan lends {lent}");
        let rate = published_bench_rate(&app);
        turn_with_bench(&mut app);
        let banked = bench_of(&app, band).orders[0].progress.to_f32();
        assert!(
            (rate - banked).abs() < 1e-4,
            "lent {lent}: quoted {rate}, banked {banked}"
        );
        quoted.push(rate);
    }
    assert!(
        quoted[0] > quoted[1] && quoted[1] > 0.0 && quoted[2] == 0.0,
        "liveness: the quote falls with the loan, got {quoted:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// THE ROWS PUBLISH THE KIT SHORTAGE OF THE HUNT TURN ABOUT TO BE PLAYED
// ---------------------------------------------------------------------------------------------

/// Spears a sending crew claims beyond what the band holds.
const CREW_PAST_THE_SPEARS: u32 = 10;

/// Every `(herd, item, required, filled)` kit-toe line the ENCODED frame publishes on the band's
/// hunt rows.
fn published_kit_toe(app: &App) -> Vec<(String, String, f32, f32)> {
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
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == BAND)
        .expect("the band is on the wire");
    cohort
        .laborAssignments()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            let herd = row.faunaId().unwrap_or_default().to_string();
            row.kitToe()
                .into_iter()
                .flatten()
                .map(|line| {
                    (
                        herd.clone(),
                        line.itemId().unwrap_or_default().to_string(),
                        line.required(),
                        line.filled(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Each published row's `(herd, workers, kitWorkersHolding)`.
fn published_kit_holding(app: &App) -> Vec<(String, u32, f32)> {
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
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == BAND)
        .expect("the band is on the wire");
    cohort
        .laborAssignments()
        .into_iter()
        .flatten()
        .map(|row| {
            (
                row.faunaId().unwrap_or_default().to_string(),
                row.workers(),
                row.kitWorkersHolding(),
            )
        })
        .collect()
}

/// Is any published row short of spears?
fn some_row_is_short_of_spears(lines: &[(String, String, f32, f32)]) -> bool {
    lines
        .iter()
        .any(|(_, item, required, filled)| item == "spears" && filled + 1e-3 < *required)
}

/// ⛔ **A CREW GOING OUT SHOWS ITS KIT SHORTAGE ON THE ROWS.** The turn crews the need row before the
/// kit settles, so the rows genuinely go short; the capture publishes each take row's `kitToe` from
/// that mustered gear. Control: the same band with the need row waiting keeps the standing reading,
/// where every row is armed.
#[test]
fn a_crew_going_out_publishes_the_shortage_on_the_rows_and_a_waiting_row_does_not() {
    let (mut sending, band) = world_hunting(CREW_NEEDS_THE_DONORS, RABBITS);
    arm_the_band(&mut sending, band, STALKING_HANDS);
    set_muster(&mut sending, band, CREW_PAST_THE_SPEARS);
    let going = published_kit_toe(&sending);
    let going_holding = published_kit_holding(&sending);
    assert!(
        going
            .iter()
            .any(|(herd, item, required, _)| herd == NEED_HERD
                && item == "spears"
                && *required > 0.0),
        "liveness: the crew's own claim is published on the need row, got {going:?}"
    );
    assert!(
        some_row_is_short_of_spears(&going),
        "the crew claims spears the band does not hold: some row reads short, got {going:?}"
    );

    // **`kitWorkersHolding` is read off the same mustered pass as `kitToe`**, so the two agree: the
    // need row holds exactly the spears its `kitToe` line says it was filled, and the donor that
    // lent its hands (and its kit) holds fewer than the hands it still shows.
    let holding_of = |herd: &str| {
        going_holding
            .iter()
            .find(|(row_herd, _, _)| row_herd == herd)
            .map(|(_, workers, holding)| (*workers, *holding))
            .expect("the row is published")
    };
    let need_filled = going
        .iter()
        .find(|(herd, item, _, _)| herd == NEED_HERD && item == "spears")
        .map(|(_, _, _, filled)| *filled)
        .expect("the need row publishes a spears line");
    assert!(
        need_filled > 0.0,
        "liveness: the crew holds some of the spears"
    );
    assert_eq!(
        holding_of(NEED_HERD).1,
        need_filled,
        "the need row's holding agrees with its kitToe line: {going:?} {going_holding:?}"
    );
    let (donor_workers, donor_holding) = holding_of(NORMAL_DONOR_HERD);
    assert!(
        donor_workers > 0,
        "liveness: the donor row still shows hands"
    );
    assert!(
        donor_holding < donor_workers as f32,
        "the lent donor holds fewer than the hands it shows, as its kitToe says: {going_holding:?}"
    );

    let (mut waiting, band_w) = world_hunting(CREW_NEEDS_THE_DONORS, RABBITS);
    arm_the_band(&mut waiting, band_w, STALKING_HANDS);
    recapture_snapshot_in_place(&mut waiting.world);
    assert_eq!(row(&waiting, band_w, NEED_HERD).muster_crew, 0);
    let waiting_holding = published_kit_holding(&waiting);
    let waiting_donor = waiting_holding
        .iter()
        .find(|(herd, _, _)| herd == NORMAL_DONOR_HERD)
        .expect("the donor row is published");
    assert_eq!(
        waiting_donor.2, waiting_donor.1 as f32,
        "a waiting row keeps the standing reading for holding as for kitToe: {waiting_holding:?}"
    );
    let standing = published_kit_toe(&waiting);
    assert!(
        !some_row_is_short_of_spears(&standing),
        "a waiting row keeps the standing reading, where the rows are armed: {standing:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// THE CREW IS CAPPED AT THE HANDS THE BAND CAN FULLY EQUIP
// ---------------------------------------------------------------------------------------------

/// Idle hands in the equipment-cap worlds: few, so a crew sized to the meal would reach past them
/// into the donor rows and the crafting bench.
const CAP_WORLD_IDLE_HANDS: u32 = 2;
/// Hands the crafting bench holds in the equipment-cap worlds.
const CAP_WORLD_BENCH_HANDS: u32 = 3;
/// Free sets of the stalking kit (spear + sled) in the short-kit world.
const FREE_STALKING_SETS: u32 = 3;
/// A camp whose hands are the idle few, both donor rows and the bench.
const CAP_WORLD: Stock = Stock {
    working: CAP_WORLD_IDLE_HANDS + LOW_DONOR_HANDS + NORMAL_DONOR_HANDS + CAP_WORLD_BENCH_HANDS,
    ..Stock::ROOMY
};

/// A lean band with a bench of `CAP_WORLD_BENCH_HANDS`, `units` sets of the stalking kit and no donor
/// row holding any of it, after one turn.
fn capped_world(units: u32) -> (App, Entity) {
    capped_world_on(units, DEFENDED_GAME)
}

/// [`capped_world`] hunting `quarry`.
fn capped_world_on(units: u32, quarry: Quarry) -> (App, Entity) {
    capped_world_geared(units, units, quarry)
}

/// [`capped_world_on`] with `spears` and `sleds` counted apart.
fn capped_world_geared(spears: u32, sleds: u32, quarry: Quarry) -> (App, Entity) {
    let (mut app, band) = world_hunting(CAP_WORLD, quarry);
    arm_the_band_with(&mut app, band, spears, sleds);
    gather_with_the_normal_donor(&mut app, band);
    app.world.entity_mut(band).insert(core_sim::BandBench {
        orders: vec![core_sim::BenchOrder::new(BENCH_RECIPE, UNFINISHED_ORDER)],
        workers: CAP_WORLD_BENCH_HANDS,
        ..Default::default()
    });
    set_meat(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    (app, band)
}

/// ⛔ **A BAND SHORT OF THE KIT SENDS ONLY THE HANDS IT CAN EQUIP ON A QUARRY BARE HANDS CANNOT
/// WOUND, AND THE BENCH LENDS NOTHING.** A crew reaching past the kit took the idle hands, the donor
/// rows and the crafting bench, most of them bare - and a bench with no crafters never made the
/// spears. The band here holds three sets and no donor row holds any: the crew is three, every one
/// of them equipped, the bench is untouched. The paired control holds the kit for everyone and sends
/// the larger crew the hunt wants.
#[test]
fn a_band_short_of_the_kit_sends_only_the_hands_it_can_equip_and_leaves_the_bench_alone() {
    let (short, band) = capped_world(FREE_STALKING_SETS);
    let need = row(&short, band, NEED_HERD);

    assert_eq!(
        need.muster_crew, FREE_STALKING_SETS,
        "the crew is the hands the band can equip"
    );
    assert_eq!(
        published(&short).row(NEED_HERD).muster_crew,
        FREE_STALKING_SETS,
        "and the wire publishes it"
    );
    assert_eq!(
        published_bench_lent(&short),
        0,
        "the bench keeps its crafters, so the spears get made"
    );

    let toe = published_kit_toe(&short);
    let stalking: Vec<_> = toe
        .iter()
        .filter(|(herd, item, _, _)| herd == NEED_HERD && (item == "spears" || item == "sled"))
        .collect();
    assert!(
        !stalking.is_empty() && stalking.iter().all(|(_, _, required, _)| *required > 0.0),
        "liveness: the crew's kit claim is published on the need row, got {toe:?}"
    );
    assert!(
        stalking
            .iter()
            .all(|(_, _, required, filled)| (filled - required).abs() < 1e-3),
        "every hunter on the need row is equipped, got {stalking:?}"
    );

    // The control: the same band holding the kit for everyone sends the whole meal crew.
    let (armed, band_a) = capped_world(ENOUGH_UNITS);
    let need_a = row(&armed, band_a, NEED_HERD);
    assert_eq!(
        need_a.muster_crew, need_a.planned_crew,
        "with the kit in hand the whole meal crew goes"
    );
    assert!(
        need_a.muster_crew > FREE_STALKING_SETS,
        "liveness: the kit is what bounded the short band's crew"
    );
}

/// ⛔ **A BAND WITH NO KIT SENDS NOBODY ON A QUARRY BARE HANDS CANNOT WOUND, BUT THE HUNT STILL
/// ASKS FOR THE KIT.** No hand can be armed and an unarmed one deals nothing, so nothing goes out,
/// the row waits for ever ([`NO_HUNT_NEEDED`]) and the bench and idle hands stay home. The crew the
/// hunt WANTS is sized as if every hand held the kit, so `planned_crew` is positive, the spear
/// suggestion for it is on the panel, and auto-craft queues it. The control holds the kit and sends
/// a crew.
#[test]
fn a_band_with_no_kit_sends_nobody_on_a_quarry_bare_hands_cannot_wound() {
    let (mut app, band) = capped_world(0);
    let (muster_crew, planned_crew) = {
        let need = row(&app, band, NEED_HERD);
        (need.muster_crew, need.planned_crew)
    };

    assert_eq!(muster_crew, 0, "nothing can be equipped, so nobody goes");
    assert!(
        planned_crew > 0,
        "the hunt still wants a crew, sized as if it were equipped"
    );
    let wire = published(&app);
    assert_eq!(wire.row(NEED_HERD).muster_crew, 0);
    assert_eq!(wire.row(NEED_HERD).turns_to_kill, 0);
    assert_eq!(wire.row(NEED_HERD).turns_until_hunt, NO_HUNT_NEEDED);
    assert_eq!(wire.idle_mustered, 0, "no idle hand is mustered either");
    assert_eq!(published_bench_lent(&app), 0);
    assert!(
        suggested(&app, "spears").is_some_and(|count| count > 0),
        "the craft suggestions ask for the spears the wanted crew needs"
    );

    auto_bench_with_materials(&mut app, band);
    let queued = queued_recipes(&app, band);
    assert!(
        queued.iter().any(|recipe| is_stalking_gear(recipe)),
        "auto-craft queues the stalking gear, got {queued:?}"
    );

    let (armed, band_a) = capped_world(ENOUGH_UNITS);
    let need_a = row(&armed, band_a, NEED_HERD);
    assert!(need_a.muster_crew > 0, "control: with the kit a crew goes");
    assert_eq!(
        need_a.planned_crew, need_a.muster_crew,
        "and it is the crew the unarmed band wanted"
    );
    assert_eq!(need_a.planned_crew, planned_crew);
}

/// ⛔ **A BAND WITH NO KIT STILL HUNTS A QUARRY BARE HANDS WOUND, AND THE CRAFT SUGGESTIONS STILL ASK
/// FOR THE SPEARS.** The gate is cleared without gear, so a kit shortage must not stop the hunt: the
/// crew the hunt wants goes out bare, and the spear suggestion for it is on the panel - and
/// auto-craft queues it. Paired with the defended-game control above, where the same band sends
/// nobody.
#[test]
fn a_band_with_no_kit_hunts_bare_where_bare_hands_wound_and_still_asks_for_spears() {
    let (mut app, band) = capped_world_on(0, RABBITS);
    let need = row(&app, band, NEED_HERD);

    assert!(
        need.planned_crew > 0,
        "liveness: the hunt wants a crew of this quarry"
    );
    assert_eq!(
        need.muster_crew, need.planned_crew,
        "bare hands wound it, so the missing kit caps nothing"
    );
    assert!(
        suggested(&app, "spears").is_some_and(|count| count > 0),
        "the craft suggestions still ask for the spears the crew needs"
    );

    auto_bench_with_materials(&mut app, band);
    let queued = queued_recipes(&app, band);
    assert!(
        queued.iter().any(|recipe| is_stalking_gear(recipe)),
        "auto-craft queues the stalking gear, got {queued:?}"
    );
}

/// ⛔ **THE SLED DOES NOT BIND A CAMP KILL.** A need crew is only ever mustered for a kill inside
/// `band_work_range`, where nothing is hauled: the sled carries nothing and wears nothing there. So
/// a band holding spears and no sleds sends the speared crew - capped at the spears it holds - and
/// is not starved by the sled it lacks. The control holds neither and sends nobody.
#[test]
fn a_band_with_spears_and_no_sleds_sends_the_speared_crew() {
    let (speared, band) = capped_world_geared(FREE_STALKING_SETS, 0, DEFENDED_GAME);
    let need = row(&speared, band, NEED_HERD);
    assert_eq!(
        need.muster_crew, FREE_STALKING_SETS,
        "the crew is the spears the band holds, whatever sleds it lacks"
    );
    assert_eq!(
        published(&speared).row(NEED_HERD).muster_crew,
        FREE_STALKING_SETS
    );

    let (short_of_both, band_s) = capped_world_geared(0, 0, DEFENDED_GAME);
    assert_eq!(
        row(&short_of_both, band_s, NEED_HERD).muster_crew,
        0,
        "control: with no spears either, nobody goes"
    );
}

/// ⛔ **A QUARRY BARE HANDS WOUND IS NOT CAPPED BY THE KIT; ONE THEY CANNOT WOUND IS BOUND BY IT.**
/// The same band, three sets of the kit, the same lean larder: on a defense-0 quarry the bare hunter
/// already clears the gate, so the crew the hunt wants goes in full, kit short or not; on a quarry
/// the bare hunter cannot wound (defense 1 against the bare attack of 1) the spears are what clear
/// it, so the crew is the three that can be armed.
#[test]
fn a_kit_shortage_binds_only_where_bare_hands_cannot_wound() {
    let (undefended, band_u) = capped_world_on(FREE_STALKING_SETS, RABBITS);
    let need_u = row(&undefended, band_u, NEED_HERD);
    assert!(
        need_u.planned_crew > FREE_STALKING_SETS,
        "liveness: the hunt wants more hands than the band has sets, got {}",
        need_u.planned_crew
    );
    assert_eq!(
        need_u.muster_crew, need_u.planned_crew,
        "bare hands wound it, so the short kit does not cap the crew"
    );

    let (defended, band_d) = capped_world_on(FREE_STALKING_SETS, DEFENDED_GAME);
    assert_eq!(
        row(&defended, band_d, NEED_HERD).muster_crew,
        FREE_STALKING_SETS,
        "bare hands cannot wound it, so the crew is the hands that can be armed"
    );
}

/// ⛔ **THE DONOR HOLDING THE KIT LENDS BEFORE THE IDLE HANDS WHEN THE KIT IS SHORT.** No free set,
/// idle hands to spare, and a local hunt row of four holding the four sets: the crew is the four the
/// band can equip, all four from the donor row and none from idle.
#[test]
fn a_short_kit_is_lent_by_its_donor_before_any_idle_hand_goes() {
    let (mut app, band) = world_hunting(CREW_NEEDS_THE_DONORS, DEFENDED_GAME);
    arm_the_band(&mut app, band, STALKING_HANDS);
    set_meat(&mut app, band, LEAN_LARDER);
    turn(&mut app);
    let wire = published(&app);

    assert!(
        wire.idle_workers >= STALKING_HANDS,
        "liveness: idle hands could have made the crew"
    );
    assert_eq!(wire.row(NEED_HERD).muster_crew, STALKING_HANDS);
    assert_eq!(wire.row(NORMAL_DONOR_HERD).lent_to_hunt, STALKING_HANDS);
    assert_eq!(wire.row(LOW_DONOR_HERD).lent_to_hunt, 0);
    assert_eq!(wire.idle_mustered, 0, "no bare idle hand goes");
}
