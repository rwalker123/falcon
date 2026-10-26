//! **THE STORAGE PIT IS ITS BAND'S SECOND LARDER** (#709, `docs/plan_civilization_steps.md` -> "The
//! storage pit is its band's second larder", `core_sim/src/fixed_store.rs`).
//!
//! Every claim is driven through the real systems against the SHIPPED config, in the Population
//! chain's order (`draw_from_stores` -> `simulate_population` -> `rot_band_larders` ->
//! `rot_store_contents` -> the take lands -> `deposit_into_stores`), and every claim about the wire is
//! read off the ENCODED snapshot. Each claim carries a liveness conjunct (the move happened at all)
//! beside its negative.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

use core_sim::fixed_store::{deposit_into_stores, draw_from_stores, rot_store_contents};
use core_sim::{
    build_test_app, recapture_snapshot_in_place, rot_band_larders, scalar_from_f32, scalar_one,
    scalar_zero, sedentarization_tick, simulate_population, tile_rung_span, BandEquipment, BandId,
    BranchId, DemographicsConfigHandle, EquipmentConfig, FactionId, FoodMix, GenerationId,
    LaborAllocation, LaborTarget, LadderConfigHandle, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, RungId, SedentarizationScore, SimulationConfig, SimulationTick, SnapshotHistory,
    SupplyNetworkConfigHandle, TileImprovementRegistry, TileRegistry, ViewerFaction,
};

const OWNER: u64 = 31;
const NEIGHBOUR: u64 = 32;
const FACTION: FactionId = FactionId(77);
const OTHER_PEOPLE: FactionId = FactionId(78);
const CAMP: (u32, u32) = (17, 24);
const CREW: f32 = 6.0;
const BAND_SIZE: u32 = 30;
/// Within the tolerance of the f32 wire and the fixed-point larder.
const EPSILON: f32 = 1.0e-3;
const DRY: &str = "dry";
const FLESH: &str = "flesh";
const FRESH: &str = "fresh_plant";
/// An age no shipped shelf life reaches in a few turns.
const SOME_AGE: u32 = 10;
/// A take far above the shipped pit's capacity.
const A_BIG_TAKE: f32 = 1_000.0;

fn food_stores() -> BranchId {
    BranchId::new("food_stores")
}
fn storage_pit() -> RungId {
    RungId::new(food_stores(), "storage_pit")
}
fn camp() -> UVec2 {
    UVec2::new(CAMP.0, CAMP.1)
}

fn spawn_band(app: &mut App, id: u64, faction: FactionId, at: UVec2) -> Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .expect("the harness map carries the tile");
    app.world
        .spawn((
            ResidentBand,
            BandId(id),
            BandEquipment::start_stocked_for(&EquipmentConfig::for_a_stocked_fixture(), CREW),
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: BAND_SIZE,
                children: scalar_zero(),
                working: scalar_from_f32(CREW),
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
                faction,
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
            },
        ))
        .id()
}

/// A world with one band of `FACTION` (`OWNER`) at the camp. Fog is off and the viewer is the band's
/// people, so the wire carries their rows.
fn world() -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    app.world.insert_resource(ViewerFaction(FACTION));
    let band = spawn_band(&mut app, OWNER, FACTION, camp());
    (app, band)
}

/// Dig a finished pit on `tile` for `faction`, owned by `owner`.
fn dig_pit(app: &mut App, tile: UVec2, faction: FactionId, owner: Option<BandId>) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (base, width) = tile_rung_span(storage_pit(), &ladder);
    let mut registry = app.world.resource_mut::<TileImprovementRegistry>();
    let pit = registry.place_owned(tile, food_stores(), faction, owner, &ladder);
    pit.set_position(base + width, &ladder);
}

fn capacity(app: &App) -> f32 {
    app.world
        .resource::<DemographicsConfigHandle>()
        .get()
        .keeping
        .fixed_stores
        .iter()
        .find(|store| store.rung == storage_pit())
        .expect("the shipped table has a pit")
        .capacity
}

fn reach(app: &App) -> u32 {
    app.world
        .resource::<SupplyNetworkConfigHandle>()
        .get()
        .reach_tiles
}

fn shelf(app: &App, class: &str) -> u32 {
    app.world
        .resource::<DemographicsConfigHandle>()
        .get()
        .keeping
        .shelf_life(class)
        .expect("the class is shipped") as u32
}

fn pit_contents(app: &App, tile: UVec2) -> f32 {
    app.world
        .resource::<TileImprovementRegistry>()
        .get(tile, food_stores())
        .expect("a pit is here")
        .contents
        .total()
        .to_f32()
}

fn fill_pit(app: &mut App, tile: UVec2, class: &str, amount: f32, age: u32) {
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .get_mut(tile, food_stores())
        .expect("a pit is here")
        .contents
        .add_aged(class, scalar_from_f32(amount), age);
}

fn larder(app: &App, band: Entity, class: &str) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .food()
        .get(class)
        .to_f32()
}

fn larder_total(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .food()
        .total()
        .to_f32()
}

fn cohort(app: &App, band: Entity) -> &PopulationCohort {
    app.world.get::<PopulationCohort>(band).unwrap()
}

/// The turn's opening, in the chain's order: the draw, the meal, the larder rot, the store rot.
fn opening(app: &mut App) {
    app.world.resource_mut::<SimulationTick>().0 += 1;
    app.world.run_system_once(draw_from_stores);
    app.world.run_system_once(simulate_population);
    app.world.run_system_once(rot_band_larders);
    app.world.run_system_once(rot_store_contents);
}

/// The turn's take lands in the larder (age 0, larder time 0, as a harvest or a landed pack does).
fn land(app: &mut App, band: Entity, class: &str, amount: f32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .stores
        .add_food_aged(class, scalar_from_f32(amount), 0);
}

fn deposit(app: &mut App) {
    app.world.run_system_once(deposit_into_stores);
}

/// Put exactly `amount` of `class` in the band's larder as old, kept food (it is not this turn's).
fn seat_larder(app: &mut App, band: Entity, class: &str, amount: f32) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort.stores = LocalStore::new();
    cohort
        .stores
        .add_food_aged(class, scalar_from_f32(amount), SOME_AGE);
    // Age the lot's larder clock past this turn's, as any older food has.
    cohort.stores.age_food(|_| Some(f32::MAX));
}

// ---------------------------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------------------------

struct BandWire {
    turns_of_food: f32,
    into_store: f32,
    from_store: f32,
    in_reach: f32,
}

struct PitWire {
    stored: f32,
    capacity: f32,
    has_owner: bool,
    owner: u64,
    spoiled: f32,
}

fn with_envelope<R>(app: &mut App, f: impl FnOnce(fb::WorldSnapshot<'_>) -> R) -> R {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    f(envelope.payload_as_snapshot().expect("a snapshot"))
}

fn band_wire(app: &mut App, band: u64) -> BandWire {
    with_envelope(app, |snapshot| {
        let row = snapshot
            .population()
            .and_then(|section| section.populations())
            .expect("the population section")
            .iter()
            .find(|cohort| cohort.bandId() == band)
            .expect("the band's own row is published");
        BandWire {
            turns_of_food: row.turnsOfFood(),
            into_store: row.foodIntoStore(),
            from_store: row.foodFromStore(),
            in_reach: row.foodInStoresInReach(),
        }
    })
}

fn pit_wire(app: &mut App, tile: UVec2) -> PitWire {
    with_envelope(app, |snapshot| {
        let row = snapshot
            .routes()
            .and_then(|section| section.tileImprovements())
            .expect("the improvement section")
            .iter()
            .find(|row| row.tileX() == tile.x && row.tileY() == tile.y)
            .expect("the pit's row is published");
        PitWire {
            stored: row.storedFood(),
            capacity: row.storeCapacity(),
            has_owner: row.hasStoreOwner(),
            owner: row.storeOwnerBand(),
            spoiled: row.storeSpoiled(),
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Deposit
// ---------------------------------------------------------------------------------------------

/// **DRY FOOD GOES IN THE PIT AS IT IS GATHERED; FRESH AND FLESH STAY; THE REST OVER CAPACITY STAYS.**
#[test]
fn a_dry_take_lands_in_the_pit_and_a_fresh_one_stays_in_the_larder() {
    let (mut app, band) = world();
    let cap = capacity(&app);
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
    opening(&mut app);

    land(&mut app, band, DRY, 100.0);
    land(&mut app, band, FLESH, 50.0);
    land(&mut app, band, FRESH, 20.0);
    deposit(&mut app);
    assert!(
        (pit_contents(&app, camp()) - 100.0).abs() < EPSILON,
        "liveness: the dry take went into the pit, got {}",
        pit_contents(&app, camp())
    );
    assert!(larder(&app, band, DRY) < EPSILON, "no dry food is left out");
    assert!(
        (larder(&app, band, FLESH) - 50.0).abs() < EPSILON,
        "flesh stays in the larder"
    );
    assert!(
        (larder(&app, band, FRESH) - 20.0).abs() < EPSILON,
        "fresh food stays in the larder"
    );
    assert!((cohort(&app, band).last_food_into_store - 100.0).abs() < EPSILON);

    // Over capacity: the pit fills to its capacity and the remainder stays in the larder.
    land(&mut app, band, DRY, A_BIG_TAKE);
    deposit(&mut app);
    assert!(
        (pit_contents(&app, camp()) - cap).abs() < EPSILON,
        "the pit holds exactly its capacity {cap}, got {}",
        pit_contents(&app, camp())
    );
    assert!(
        (larder(&app, band, DRY) - (100.0 + A_BIG_TAKE - cap)).abs() < EPSILON,
        "what does not fit stays in the larder: {}",
        larder(&app, band, DRY)
    );
}

/// **FOOD THAT WAS ALREADY IN THE LARDER IS NOT "THIS TURN'S TAKE".** The opening reserve and food
/// kept from earlier turns stay; only what entered after the turn's rot goes in.
#[test]
fn only_food_that_entered_the_larder_this_turn_is_deposited() {
    let (mut app, band) = world();
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
    seat_larder(&mut app, band, DRY, 200.0);
    opening(&mut app);
    land(&mut app, band, DRY, 30.0);
    deposit(&mut app);
    assert!(
        (pit_contents(&app, camp()) - 30.0).abs() < EPSILON,
        "liveness: the new take went in, got {}",
        pit_contents(&app, camp())
    );
    assert!(
        larder(&app, band, DRY) > 150.0,
        "the older reserve stayed in the larder, got {}",
        larder(&app, band, DRY)
    );
}

// ---------------------------------------------------------------------------------------------
// Reach
// ---------------------------------------------------------------------------------------------

/// **A BAND BEYOND REACH DEPOSITS NOTHING AND DRAWS NOTHING, AND ITS PIT ROTS ONLY AT THE PIT'S
/// LIFE.**
#[test]
fn a_band_beyond_reach_neither_deposits_nor_draws_and_its_pit_keeps_to_180() {
    let (mut app, band) = world();
    let far = UVec2::new(CAMP.0 + reach(&app) + 1, CAMP.1);
    dig_pit(&mut app, far, FACTION, Some(BandId(OWNER)));
    let base = shelf(&app, DRY);
    fill_pit(&mut app, far, DRY, 100.0, base - 1);

    opening(&mut app);
    land(&mut app, band, DRY, 40.0);
    deposit(&mut app);
    assert!(
        (larder(&app, band, DRY) - 40.0).abs() < EPSILON,
        "the take stays in the larder, out of reach"
    );
    assert!(
        (pit_contents(&app, far) - 100.0).abs() < EPSILON,
        "and the pit's food is untouched and has outlived the larder's {base}-turn life"
    );
    assert_eq!(cohort(&app, band).last_food_into_store, 0.0);
    assert_eq!(cohort(&app, band).last_food_from_store, 0.0);

    // Control: the same pit one step in does take the deposit (liveness of the mechanism).
    let (mut app, band) = world();
    let edge = UVec2::new(CAMP.0 + reach(&app), CAMP.1);
    dig_pit(&mut app, edge, FACTION, Some(BandId(OWNER)));
    opening(&mut app);
    land(&mut app, band, DRY, 40.0);
    deposit(&mut app);
    assert!(
        (pit_contents(&app, edge) - 40.0).abs() < EPSILON,
        "a pit exactly at the edge of reach takes the deposit"
    );
}

// ---------------------------------------------------------------------------------------------
// Draw and the runway
// ---------------------------------------------------------------------------------------------

/// **A BAND WITH AN EMPTY LARDER AND A FULL PIT EATS FROM THE PIT, AND ITS RUNWAY READS THE PIT FOOD.**
#[test]
fn an_empty_larder_beside_a_full_pit_eats_from_the_pit_and_does_not_read_as_starving() {
    let (mut app, band) = world();
    let cap = capacity(&app);
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
    fill_pit(&mut app, camp(), DRY, cap, SOME_AGE);

    opening(&mut app);
    let eaten = cohort(&app, band).last_food_consumption;
    assert!(eaten > 0.0, "liveness: the band ate");
    assert!(
        (cohort(&app, band).last_food_from_store - eaten).abs() < EPSILON,
        "everything it ate came out of the pit: ate {eaten}, drew {}",
        cohort(&app, band).last_food_from_store
    );
    assert!(
        (pit_contents(&app, camp()) - (cap - eaten)).abs() < EPSILON,
        "the pit is down by the meal"
    );
    assert!(
        cohort(&app, band).last_fertility_factors.hunger > scalar_zero(),
        "a fed band: hunger is not zero"
    );

    let wire = band_wire(&mut app, OWNER);
    assert!(
        wire.turns_of_food > 100.0,
        "the runway reads the pit's food, not starvation: {}",
        wire.turns_of_food
    );
    assert!(
        (wire.in_reach - (cap - eaten)).abs() < 0.01,
        "foodInStoresInReach is the pit's contents: {}",
        wire.in_reach
    );
    assert!((wire.from_store - eaten).abs() < 0.01);

    // Control: the same band and a pit that is NOT its own (a stranger's) reads as starving.
    let (mut app, band) = world();
    dig_pit(&mut app, camp(), OTHER_PEOPLE, None);
    fill_pit(&mut app, camp(), DRY, cap, SOME_AGE);
    opening(&mut app);
    assert_eq!(cohort(&app, band).last_food_from_store, 0.0);
    let wire = band_wire(&mut app, OWNER);
    assert!(
        wire.turns_of_food < 1.0,
        "no pit of its own: the empty larder is starvation, got {}",
        wire.turns_of_food
    );
}

/// **ANOTHER BAND OF THE SAME PEOPLE BESIDE THE PIT, NOT ITS OWNER, NEITHER DEPOSITS NOR DRAWS.**
#[test]
fn a_band_that_does_not_own_the_pit_never_touches_it() {
    let (mut app, owner) = world();
    let cap = capacity(&app);
    let neighbour = spawn_band(&mut app, NEIGHBOUR, FACTION, camp());
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
    fill_pit(&mut app, camp(), DRY, cap / 2.0, SOME_AGE);

    opening(&mut app);
    assert!(
        cohort(&app, owner).last_food_from_store > 0.0,
        "liveness: the OWNER draws"
    );
    assert_eq!(
        cohort(&app, neighbour).last_food_from_store,
        0.0,
        "the neighbour, with an empty larder beside the same pit, draws nothing"
    );
    let after_draw = pit_contents(&app, camp());

    land(&mut app, neighbour, DRY, 40.0);
    deposit(&mut app);
    assert!(
        (larder(&app, neighbour, DRY) - 40.0).abs() < EPSILON,
        "the neighbour's take stays in its own larder"
    );
    assert!(
        (pit_contents(&app, camp()) - after_draw).abs() < EPSILON,
        "and the pit did not take it"
    );
}

// ---------------------------------------------------------------------------------------------
// Ownership
// ---------------------------------------------------------------------------------------------

/// A tile crew of `hands` on the pit at `tile`, as the labor pass reads it.
fn crew_the_pit(app: &mut App, band: Entity, tile: UVec2, hands: u32) {
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(
        LaborTarget::Tile {
            tile,
            branch: food_stores(),
        },
        hands,
        hands,
        None,
    );
    app.world.entity_mut(band).insert(allocation);
}

/// **THE OWNER GONE (DESPAWNED, OR OF ANOTHER PEOPLE) CLEARS THE OWNER, AND THE NEXT CREWING BAND
/// ADOPTS IT.**
#[test]
fn a_pit_whose_owner_is_gone_is_adopted_by_the_next_band_that_crews_it() {
    for broke_away in [false, true] {
        let (mut app, owner) = world();
        let neighbour = spawn_band(&mut app, NEIGHBOUR, FACTION, camp());
        dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
        crew_the_pit(&mut app, neighbour, camp(), 2);

        // Control: with the owner alive nobody else is adopted.
        deposit(&mut app);
        let owner_now = |app: &App| {
            app.world
                .resource::<TileImprovementRegistry>()
                .get(camp(), food_stores())
                .unwrap()
                .owner
        };
        assert_eq!(owner_now(&app), Some(BandId(OWNER)), "the owner stands");

        if broke_away {
            app.world
                .get_mut::<PopulationCohort>(owner)
                .unwrap()
                .faction = OTHER_PEOPLE;
        } else {
            app.world.despawn(owner);
        }
        deposit(&mut app);
        assert_eq!(
            owner_now(&app),
            Some(BandId(NEIGHBOUR)),
            "the pit passes to the band that crews it (broke_away = {broke_away})"
        );
    }

    // No crew, no adoption: the pit stays unowned and nothing goes in or out.
    let (mut app, owner) = world();
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
    app.world.despawn(owner);
    let stray = spawn_band(&mut app, NEIGHBOUR, FACTION, camp());
    deposit(&mut app);
    assert_eq!(
        app.world
            .resource::<TileImprovementRegistry>()
            .get(camp(), food_stores())
            .unwrap()
            .owner,
        None,
        "an uncrewed pit has no owner"
    );
    land(&mut app, stray, DRY, 10.0);
    deposit(&mut app);
    assert!(
        pit_contents(&app, camp()) < EPSILON,
        "while it has no owner nothing goes in"
    );
}

/// **THE FIRST CREW'S BAND OWNS THE PIT IT PLACES.**
#[test]
fn placing_a_pit_names_the_placing_band_its_owner() {
    let (mut app, _) = world();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let mut registry = app.world.resource_mut::<TileImprovementRegistry>();
    registry.place_owned(camp(), food_stores(), FACTION, Some(BandId(OWNER)), &ladder);
    // A second crew placing the same improvement does not take it over.
    registry.place_owned(
        camp(),
        food_stores(),
        FACTION,
        Some(BandId(NEIGHBOUR)),
        &ladder,
    );
    assert_eq!(
        registry.get(camp(), food_stores()).unwrap().owner,
        Some(BandId(OWNER))
    );
}

// ---------------------------------------------------------------------------------------------
// Filling in
// ---------------------------------------------------------------------------------------------

/// **A PIT THAT FALLS BACK TO ITS ROOT LOSES ITS CONTENTS INTO `storeSpoiled`.**
#[test]
fn a_pit_that_fills_in_loses_its_contents_and_says_so_on_the_wire() {
    let (mut app, _) = world();
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
    fill_pit(&mut app, camp(), DRY, 120.0, SOME_AGE);
    let before = pit_wire(&mut app, camp());
    assert!(
        (before.stored - 120.0).abs() < 0.01,
        "liveness: it held food"
    );
    assert!(before.capacity > 0.0, "it is a store");

    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .get_mut(camp(), food_stores())
        .unwrap()
        .set_position(0.0, &ladder);
    app.world.run_system_once(rot_store_contents);
    assert!(
        pit_contents(&app, camp()) < EPSILON,
        "what was in it is gone"
    );

    let after = pit_wire(&mut app, camp());
    assert!(
        (after.spoiled - 120.0).abs() < 0.01,
        "all of it counted as lost: {}",
        after.spoiled
    );
    assert_eq!(after.stored, 0.0);
    assert_eq!(after.capacity, 0.0, "a filled-in pit is no longer a store");
}

// ---------------------------------------------------------------------------------------------
// The ledger
// ---------------------------------------------------------------------------------------------

/// **THE IDENTITY CLOSES ON A DEPOSIT TURN AND ON A DRAW TURN**, with the two store terms, read off the
/// encoded frame.
#[test]
fn the_ledger_identity_closes_on_a_deposit_turn_and_a_draw_turn() {
    let (mut app, band) = world();
    let cap = capacity(&app);
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));

    // --- a deposit turn: a dry take lands and goes into the pit -----------------------------
    seat_larder(&mut app, band, FLESH, 40.0);
    let before = larder_total(&app, band);
    opening(&mut app);
    let landed = 25.0;
    land(&mut app, band, DRY, landed);
    deposit(&mut app);
    let wire = band_wire(&mut app, OWNER);
    assert!(
        wire.into_store > 0.0,
        "liveness: something went into the pit"
    );
    let consumption = cohort(&app, band).last_food_consumption;
    let spoiled = cohort(&app, band).last_food_spoiled;
    let delta = larder_total(&app, band) - before;
    let ledger = landed - consumption - spoiled - wire.into_store + wire.from_store;
    assert!(
        (delta - ledger).abs() < EPSILON,
        "deposit turn: larder_delta {delta} vs ledger {ledger}"
    );

    // --- a draw turn: nothing in the larder, the pit feeds the meal --------------------------
    app.world.get_mut::<PopulationCohort>(band).unwrap().stores = LocalStore::new();
    fill_pit(&mut app, camp(), DRY, cap / 4.0, SOME_AGE);
    let before = larder_total(&app, band);
    opening(&mut app);
    let wire = band_wire(&mut app, OWNER);
    assert!(wire.from_store > 0.0, "liveness: the pit fed the meal");
    let consumption = cohort(&app, band).last_food_consumption;
    let spoiled = cohort(&app, band).last_food_spoiled;
    let delta = larder_total(&app, band) - before;
    let ledger = 0.0 - consumption - spoiled - wire.into_store + wire.from_store;
    assert!(
        (delta - ledger).abs() < EPSILON,
        "draw turn: larder_delta {delta} vs ledger {ledger}"
    );
}

// ---------------------------------------------------------------------------------------------
// The wire's disclosure
// ---------------------------------------------------------------------------------------------

/// **THE PIT'S CONTENTS, OWNER AND ROT ARE THE IMPROVEMENT'S OWN PEOPLE'S; THE CAPACITY FOLLOWS THE
/// ROW.**
#[test]
fn a_rivals_pit_discloses_its_capacity_and_nothing_inside_it() {
    let (mut app, _) = world();
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
    fill_pit(&mut app, camp(), DRY, 80.0, SOME_AGE);
    let own = pit_wire(&mut app, camp());
    assert!(own.stored > 0.0 && own.has_owner && own.owner == OWNER);

    // The same pit seen by a rival viewer.
    app.world.insert_resource(ViewerFaction(OTHER_PEOPLE));
    let seen = pit_wire(&mut app, camp());
    assert_eq!(seen.capacity, own.capacity, "capacity follows the row");
    assert_eq!(seen.stored, 0.0, "contents are withheld");
    assert!(!seen.has_owner);
    assert_eq!(seen.owner, 0);
    assert_eq!(seen.spoiled, 0.0);
}

// ---------------------------------------------------------------------------------------------
// Sedentarization
// ---------------------------------------------------------------------------------------------

/// **THE PIT'S CONTENTS COUNT IN THE PEOPLE'S SURPLUS**, beside the larders and whether or not any
/// band is in reach.
#[test]
fn the_sedentarization_surplus_counts_the_food_in_the_pit() {
    let surplus_points = |stored: f32, band_at: UVec2| {
        let (mut app, band) = world();
        dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
        fill_pit(&mut app, camp(), DRY, stored, SOME_AGE);
        let tile = app
            .world
            .resource::<TileRegistry>()
            .index(band_at.x, band_at.y)
            .unwrap();
        app.world
            .get_mut::<PopulationCohort>(band)
            .unwrap()
            .current_tile = tile;
        app.world.run_system_once(sedentarization_tick);
        app.world
            .resource::<SedentarizationScore>()
            .entry(FACTION)
            .expect("the people are scored")
            .surplus_points
    };
    let empty = surplus_points(0.0, camp());
    let full = surplus_points(100.0, camp());
    assert!(
        full > empty,
        "liveness: a pit with food raises the surplus ({full} vs {empty})"
    );
    let far = UVec2::new(CAMP.0 + 10, CAMP.1);
    assert!(
        (surplus_points(100.0, far) - full).abs() < 1.0e-4,
        "walking out of reach does not move it off the score"
    );
}

// ---------------------------------------------------------------------------------------------
// Save / load
// ---------------------------------------------------------------------------------------------

/// **A SAVE AND A CHECKPOINT CARRY THE OWNER AND THE CONTENTS.**
#[test]
fn a_save_and_a_checkpoint_carry_the_owner_and_the_contents() {
    use core_sim::save::{decode_save, encode_save};
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    let (mut app, _) = world();
    dig_pit(&mut app, camp(), FACTION, Some(BandId(OWNER)));
    fill_pit(&mut app, camp(), DRY, 77.0, SOME_AGE);

    let check = |registry: &TileImprovementRegistry, what: &str| {
        let pit = registry
            .get(camp(), food_stores())
            .unwrap_or_else(|| panic!("{what} carries the pit"));
        assert_eq!(pit.owner, Some(BandId(OWNER)), "{what}: owner");
        assert!(
            (pit.contents.total().to_f32() - 77.0).abs() < EPSILON,
            "{what}: contents"
        );
        let ages: Vec<u32> = pit.contents.batches().map(|(_, age, _)| age).collect();
        assert_eq!(ages, vec![SOME_AGE], "{what}: the lot keeps its age");
    };

    let checkpoint = capture_sim_state(&app.world);
    app.world
        .insert_resource(TileImprovementRegistry::default());
    restore_sim_state(&mut app.world, &checkpoint);
    check(
        app.world.resource::<TileImprovementRegistry>(),
        "the checkpoint",
    );

    let blob = encode_save(&app.world).expect("the world encodes");
    let (_, payload) = decode_save(&blob).expect("the save decodes");
    check(&payload.sim.tile_improvements, "the save");
    let _ = FoodMix::default();
}
