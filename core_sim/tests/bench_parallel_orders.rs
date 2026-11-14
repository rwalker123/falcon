//! **The bench runs several orders at once, sim-staffed, capped per item, materials set aside per
//! item** (#873, `.claude/rules/core_sim/crafting.md` -> "The queue" and "Staffing").
//!
//! Every fixture states the hands the band has (`working`) and the stock it holds, queues orders on
//! the bench component, restaffs the band (the same call every bench command makes) and reads what the
//! staffing dealt off the orders. The wire claims read the ENCODED frame.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{App, Entity, With};

use core_sim::auto_craft::auto_craft_fill;
use core_sim::staffing::restaff_band;
use core_sim::{
    advance_crafting, build_test_app, recapture_snapshot_in_place, scalar_from_f32, BandBench,
    BandEquipment, BenchOrder, BuildSource, KeepingIssue, LaborAllocation, LocalStore,
    MaterialsConfigHandle, PopulationCohort, ResidentBand, SimulationTick, SnapshotHistory,
    SourcePriority,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use std::collections::BTreeMap;

const SLED: &str = "sled";
const BASKETS: &str = "baskets";
const SPEARS: &str = "spears";
const HIDE: &str = "hide";
const FIBRE: &str = "fibre";
const BONE: &str = "bone";
/// The bench tool that bounds hide.
const TANNING_FRAME: &str = "tanning_frame";
const PLAIN_TIER: &str = "plain";
/// Stock enough for any number of items used here.
const PLENTY: f32 = 400.0;
/// One sled's fibre (the recipe's stated amount, the bare hand's efficiency being 1.0).
const FIBRE_PER_SLED: f32 = 2.0;
const HIDE_PER_SLED: f32 = 2.0;
const HIDE_AXES: [(&str, f32); 2] = [("toughness", 0.6), ("suppleness", 0.5)];
const FIBRE_AXES: [(&str, f32); 2] = [("fineness", 0.5), ("strength", 0.5)];
const BONE_AXES: [(&str, f32); 2] = [("density", 0.5), ("length", 0.5)];

fn bank(app: &mut App, band: Entity, material: &str, amount: f32, axes: &[(&str, f32)]) {
    let materials = app.world.resource::<MaterialsConfigHandle>().get();
    let readings: BTreeMap<String, f32> = axes
        .iter()
        .map(|(axis, value)| ((*axis).to_string(), *value))
        .collect();
    let key = materials
        .band_key(material, &readings)
        .expect("the shipped table carries this material");
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .deposit_material(material, key, scalar_from_f32(amount), &readings);
}

/// A band with exactly `hands` working-age people, no rows, no gear, no stock and an empty bench.
fn world(hands: u32) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let band = app
        .world
        .query_filtered::<Entity, With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .expect("the headless world spawns a resident band");
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.working = scalar_from_f32(hands as f32);
        cohort.children = scalar_from_f32(0.0);
        cohort.elders = scalar_from_f32(0.0);
        cohort.stores.clear_materials();
    }
    *app.world.get_mut::<BandEquipment>(band).unwrap() = BandEquipment::default();
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .clear();
    *app.world.get_mut::<BandBench>(band).unwrap() = BandBench::default();
    (app, band)
}

fn queue(app: &mut App, band: Entity, recipe: &str, count: u32, priority: SourcePriority) {
    let mut bench = app.world.get_mut::<BandBench>(band).unwrap();
    let mut order = BenchOrder::new(recipe, count);
    order.priority = priority;
    bench.orders.push(order);
}

fn staff(app: &mut App, band: Entity) {
    restaff_band(&mut app.world, band);
}

fn bench(app: &App, band: Entity) -> &BandBench {
    app.world.get::<BandBench>(band).unwrap()
}

fn total(app: &App, band: Entity, material: &str) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .material_total(material)
        .to_f32()
}

fn work_the_bench(app: &mut App) {
    app.world.run_system_once(advance_crafting);
}

/// **1. Every workable order runs at once; each gets its first crafter before either gets a second,
/// and a High order is filled to its cap before a Normal order's second crafter.**
#[test]
fn every_workable_order_gets_a_first_crafter_and_high_fills_before_normal_gets_a_second() {
    // Three hands, two orders: one each first, then the third goes to the High order.
    let (mut app, band) = world(3);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    queue(&mut app, band, SLED, 4, SourcePriority::Normal);
    queue(&mut app, band, BASKETS, 4, SourcePriority::High);
    staff(&mut app, band);
    let orders = &bench(&app, band).orders;
    assert_eq!(
        orders[0].crew.total(),
        1,
        "the Normal order has its first crafter and no more"
    );
    assert_eq!(
        orders[1].crew.total(),
        2,
        "the High order took the third hand"
    );
    // Liveness: both really work in one turn.
    work_the_bench(&mut app);
    let orders = &bench(&app, band).orders;
    assert!(
        orders.iter().all(|order| !order.items.is_empty()),
        "both orders started an item the same turn"
    );
}

/// **2. The cap: Baskets x2 (2 per item) with 6 spare hands takes 4 and works 2 items side by side;
/// a Spears x1 order never takes more than 1 however many hands are idle.**
#[test]
fn an_order_takes_at_most_crafters_per_item_times_its_items() {
    let (mut app, band) = world(6);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    queue(&mut app, band, BASKETS, 2, SourcePriority::Normal);
    staff(&mut app, band);
    let order = &bench(&app, band).orders[0];
    assert_eq!(
        (order.crew.total(), order.crew_cap),
        (4, 4),
        "2 per item x 2 items"
    );
    work_the_bench(&mut app);
    assert_eq!(
        bench(&app, band).orders[0].items.len(),
        2,
        "two items side by side"
    );

    let (mut app, band) = world(9);
    bank(&mut app, band, BONE, PLENTY, &BONE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    queue(&mut app, band, SPEARS, 1, SourcePriority::Normal);
    staff(&mut app, band);
    assert_eq!(
        bench(&app, band).orders[0].crew.total(),
        1,
        "one spear, one crafter, however many hands are idle"
    );
}

/// **3. Set-aside is all or nothing**: an order short of one input starts nothing, sets nothing aside
/// and takes nobody; a batch whose stock covers 1 of 3 items starts 1 and its cap is
/// `crafters_per_item x 1`.
#[test]
fn an_item_is_set_aside_whole_or_not_at_all() {
    let (mut app, band) = world(10);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    queue(&mut app, band, SLED, 3, SourcePriority::Normal);
    staff(&mut app, band);
    work_the_bench(&mut app);
    let order = &bench(&app, band).orders[0];
    assert_eq!(
        (order.crew.total(), order.crew_cap, order.items.len()),
        (0, 0, 0)
    );
    assert_eq!(
        total(&app, band, HIDE),
        PLENTY,
        "the hide it had was not touched"
    );

    // Fibre for exactly one sled of three.
    let (mut app, band) = world(10);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, FIBRE_PER_SLED + 0.5, &FIBRE_AXES);
    queue(&mut app, band, SLED, 3, SourcePriority::Normal);
    staff(&mut app, band);
    assert_eq!(
        bench(&app, band).orders[0].crew_cap,
        2,
        "crafters per item x one item"
    );
    work_the_bench(&mut app);
    assert_eq!(bench(&app, band).orders[0].items.len(), 1);
    assert_eq!(total(&app, band, HIDE), PLENTY - HIDE_PER_SLED);
    assert_eq!(
        total(&app, band, FIBRE),
        0.5,
        "exactly one item's fibre left the store"
    );
}

/// **4. No deadlock**: two orders that both need hide + fibre with stock for exactly one item - the
/// higher-ranked order starts one, the other holds nothing and takes nobody, and the next turn is
/// consistent.
#[test]
fn two_orders_short_of_one_pile_do_not_both_claim_it() {
    let (mut app, band) = world(10);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, FIBRE_PER_SLED, &FIBRE_AXES);
    queue(&mut app, band, SLED, 2, SourcePriority::Normal);
    queue(&mut app, band, SLED, 2, SourcePriority::High);
    staff(&mut app, band);
    let orders = &bench(&app, band).orders;
    assert_eq!(
        orders[1].crew_cap, 2,
        "the High order is credited the one item"
    );
    assert_eq!(
        orders[0].crew_cap, 0,
        "the Normal order is not credited the same fibre"
    );
    assert_eq!(orders[0].crew.total(), 0);
    work_the_bench(&mut app);
    let orders = &bench(&app, band).orders;
    assert_eq!((orders[1].items.len(), orders[0].items.len()), (1, 0));
    // Next turn: restaff and work again - nothing new is claimed, nothing deadlocks.
    staff(&mut app, band);
    work_the_bench(&mut app);
    let orders = &bench(&app, band).orders;
    assert_eq!((orders[1].items.len(), orders[0].items.len()), (1, 0));
    assert_eq!(orders[0].crew.total(), 0);
}

/// **5. Cancelling returns what the order holds**: the store's totals and batch readings are exactly
/// what they were before the set-aside.
#[test]
fn removing_an_order_returns_its_set_aside_exactly() {
    let (mut app, band) = world(6);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    let before: LocalStore = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .clone();
    queue(&mut app, band, SLED, 2, SourcePriority::Normal);
    staff(&mut app, band);
    work_the_bench(&mut app);
    assert!(
        total(&app, band, HIDE) < PLENTY,
        "liveness: items were set aside"
    );
    let mut removed = app
        .world
        .get_mut::<BandBench>(band)
        .unwrap()
        .remove_order(0)
        .expect("the order is on the queue");
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    removed.return_set_aside(&mut cohort.stores);
    assert_eq!(
        cohort.stores, before,
        "the store is what it was, to the bit"
    );
}

/// **6. One tool, two orders**: the High order's crafter holds the tanning frame (the tool's rate and
/// grade ceiling, one wear on completion); the other works by hand at the material's bare rate.
#[test]
fn one_tool_goes_to_the_higher_order_and_the_other_works_by_hand() {
    let (mut app, band) = world(2);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    app.world
        .get_mut::<BandEquipment>(band)
        .unwrap()
        .stock(TANNING_FRAME, 1, PLAIN_TIER, None);
    queue(&mut app, band, SLED, 4, SourcePriority::Normal);
    queue(&mut app, band, SLED, 4, SourcePriority::High);
    staff(&mut app, band);
    let orders = &bench(&app, band).orders;
    assert_eq!(
        (orders[1].crew.at_tool, orders[1].crew.by_hand),
        (1, 0),
        "the High order's crafter holds the frame"
    );
    assert_eq!(
        (orders[0].crew.at_tool, orders[0].crew.by_hand),
        (0, 1),
        "the other works by hand"
    );
    work_the_bench(&mut app);
    let orders = &bench(&app, band).orders;
    assert!(
        orders[1].items[0].drawn.at_tool,
        "set aside with the tool in hand"
    );
    assert!(!orders[0].items[0].drawn.at_tool, "set aside by hand");
    assert!(
        orders[1].items[0].progress > orders[0].items[0].progress,
        "the tool's craft speed beats the bare hand's: {:?} vs {:?}",
        orders[1].items[0].progress,
        orders[0].items[0].progress
    );
}

/// **7. A paused order keeps its items' progress and piles, and resumes.**
#[test]
fn an_order_with_no_crew_keeps_its_items_and_resumes() {
    let (mut app, band) = world(2);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    queue(&mut app, band, SLED, 4, SourcePriority::Normal);
    staff(&mut app, band);
    work_the_bench(&mut app);
    let progress = bench(&app, band).orders[0].progress_total();
    let hide = total(&app, band, HIDE);
    assert!(
        progress > scalar_from_f32(0.0),
        "liveness: an item is in flight and worked"
    );
    // Nobody is on it this turn.
    let crew = bench(&app, band).orders[0].crew;
    app.world.get_mut::<BandBench>(band).unwrap().orders[0].crew = Default::default();
    work_the_bench(&mut app);
    assert_eq!(
        bench(&app, band).orders[0].progress_total(),
        progress,
        "progress paused"
    );
    assert_eq!(total(&app, band, HIDE), hide, "no new pile was cut");
    assert_eq!(bench(&app, band).orders[0].items.len(), 1);
    // Staffed again: it resumes.
    app.world.get_mut::<BandBench>(band).unwrap().orders[0].crew = crew;
    work_the_bench(&mut app);
    assert!(bench(&app, band).orders[0].progress_total() > progress);
}

/// **10. Auto-craft adds no order while a workable order is below its cap; with room it adds one
/// even when no hand is idle** (the labor passes decide who crews it).
#[test]
fn auto_craft_adds_nothing_while_an_order_is_below_its_cap() {
    // One hand, an order that could take six: it is below its cap, so the bench has no room.
    let (mut app, band) = world(1);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    queue(&mut app, band, SLED, 3, SourcePriority::Normal);
    app.world.get_mut::<BandBench>(band).unwrap().auto = true;
    staff(&mut app, band);
    assert!(!auto_craft_fill(&mut app.world, band));
    assert_eq!(bench(&app, band).orders.len(), 1);
    // Plenty of hands but the order's crew cleared by hand: still below its cap, still no room.
    let (mut app, band) = world(20);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    queue(&mut app, band, SLED, 3, SourcePriority::Normal);
    app.world.get_mut::<BandBench>(band).unwrap().auto = true;
    staff(&mut app, band);
    app.world.get_mut::<BandBench>(band).unwrap().orders[0].crew = Default::default();
    assert!(
        !auto_craft_fill(&mut app.world, band),
        "an order below its cap means the bench has no room for another"
    );
    assert_eq!(bench(&app, band).orders.len(), 1);
}

/// **12. The encoded frame carries each order's priority, crew, cap, per-item crew, items in flight,
/// rate and set-aside, and the bench's `workers` and `setAside` agree with the sim.**
#[test]
fn the_encoded_frame_carries_the_staffed_orders() {
    let (mut app, band) = world(6);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    queue(&mut app, band, SLED, 3, SourcePriority::Low);
    queue(&mut app, band, BASKETS, 3, SourcePriority::High);
    staff(&mut app, band);
    work_the_bench(&mut app);
    recapture_snapshot_in_place(&mut app.world);
    let tick = app.world.resource::<SimulationTick>().0;
    let _ = tick;
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let band_id = app.world.get::<core_sim::BandId>(band).unwrap().0;
    let cohort = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == band_id)
        .expect("the band is published");
    let published = cohort.bench().expect("the bench row");
    let sim = bench(&app, band);
    assert_eq!(published.workers(), sim.crew_total());
    assert!(
        published.workers() > 0,
        "liveness: somebody is at the bench"
    );
    let rows = published.orders().expect("the orders ride the bench");
    assert_eq!(rows.len(), sim.orders.len());
    for (index, row) in rows.iter().enumerate() {
        let order = &sim.orders[index];
        assert_eq!(row.crewAtTool(), order.crew.at_tool);
        assert_eq!(row.crewByHand(), order.crew.by_hand);
        assert_eq!(row.crewCap(), order.crew_cap);
        assert_eq!(
            row.crewPerItem(),
            2,
            "sled and baskets both take two per item"
        );
        assert_eq!(row.itemsInFlight() as usize, order.items.len());
        assert!(row.ratePerTurn() > 0.0, "a crewed order publishes its rate");
        assert!(row.drawnInputs().is_some_and(|rows| !rows.is_empty()));
    }
    assert_eq!(rows.get(0).priority(), fb::SourcePriority::Low);
    assert_eq!(rows.get(1).priority(), fb::SourcePriority::High);
    let set_aside_hide: f32 = published
        .setAside()
        .expect("setAside")
        .iter()
        .filter(|row| row.materialId() == Some(HIDE))
        .map(|row| row.amount())
        .sum();
    assert!(
        (set_aside_hide - (PLENTY - total(&app, band, HIDE))).abs() < 1e-3,
        "setAside is what left the store: {set_aside_hide}"
    );
}

/// **10 (positive half). With idle hands and every workable order at its cap (here: none workable),
/// auto-craft adds exactly ONE order, and the band is restaffed so it takes the idle hands at once.**
#[test]
fn auto_craft_adds_exactly_one_order_when_crafters_are_free() {
    const HOES: &str = "hoes";
    let (mut app, band) = world(20);
    bank(&mut app, band, BONE, PLENTY, &BONE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.last_pool_toe.clear();
        allocation.last_keeping_issued = vec![KeepingIssue {
            source: BuildSource::Patch(bevy::prelude::UVec2::new(2, 3)),
            item: HOES.to_string(),
            units: 0.0,
            required: 3.0,
        }];
    }
    app.world.get_mut::<BandBench>(band).unwrap().auto = true;
    staff(&mut app, band);
    assert!(
        auto_craft_fill(&mut app.world, band),
        "idle hands and nothing to work: one order"
    );
    assert_eq!(bench(&app, band).orders.len(), 1);
    assert!(
        bench(&app, band).orders[0].crew.total() > 0,
        "the band was restaffed: the new order took hands this turn"
    );
    // The one order is at its cap and the item is already queued: nothing more is added.
    assert!(!auto_craft_fill(&mut app.world, band));
    assert_eq!(bench(&app, band).orders.len(), 1);
}

// ---------------------------------------------------------------------------------------------
// WHY AUTO-CRAFT HAS NOT STARTED THE TOP SUGGESTION (`BenchState.autoWaitReason`)
// ---------------------------------------------------------------------------------------------

const HOES: &str = "hoes";
/// The lead of the reason a crewless order publishes when every staffable hand was dealt first.
const EVERY_HAND_NEEDED_FIRST: &str = "Every hand is needed first";
/// The scouting range the fixture states: more tiles than the band has hands, so every staffable
/// hand scouts.
const SCOUTING_RANGE_WANTING_EVERY_HAND: u32 = 9;
/// A scouting range the band's hands more than cover.
const SCOUTING_RANGE_LEAVING_HANDS: u32 = 2;
/// The keeping line the fixture leaves short, which makes `hoes` the top suggestion.
const HOES_REQUIRED: f32 = 3.0;

/// What the encoded frame says about one band's auto bench.
struct WaitFrame {
    reason: String,
    idle_workers: u32,
    /// `(itemId, displayName)` of the published suggestions, in rank order.
    suggestions: Vec<(String, String)>,
    /// `(outputItemId, suggested, reason)` of every published offer.
    offers: Vec<(String, bool, String)>,
    orders: usize,
    /// `(blockedReason, crewAtTool + crewByHand)` of each published order.
    order_rows: Vec<(String, u32)>,
}

fn read_wait_frame(app: &mut App, band: Entity) -> WaitFrame {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let band_id = app.world.get::<core_sim::BandId>(band).unwrap().0;
    let cohort = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == band_id)
        .expect("the band is published");
    let bench = cohort.bench().expect("the bench row");
    WaitFrame {
        reason: bench.autoWaitReason().unwrap_or_default().to_string(),
        idle_workers: cohort.idleWorkers(),
        suggestions: cohort
            .craftSuggestions()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        (
                            row.itemId().unwrap_or_default().to_string(),
                            row.displayName().unwrap_or_default().to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        offers: cohort
            .craftOffers()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        (
                            row.outputItemId().unwrap_or_default().to_string(),
                            row.suggested(),
                            row.reason().unwrap_or_default().to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        orders: bench.orders().map_or(0, |rows| rows.len()),
        order_rows: bench
            .orders()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        (
                            row.blockedReason().unwrap_or_default().to_string(),
                            row.crewAtTool() + row.crewByHand(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// An auto band whose top suggestion is `hoes` (a short keeping line), `hands` working-age people,
/// and a scouting range of `scouting_range` tiles (0 = none), which the staffing deals first.
fn auto_band(hands: u32, scouting_range: u32, stocked: bool) -> (App, Entity) {
    let (mut app, band) = world(hands);
    if stocked {
        bank(&mut app, band, BONE, PLENTY, &BONE_AXES);
        bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    }
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.last_pool_toe.clear();
        allocation.last_keeping_issued = vec![KeepingIssue {
            source: BuildSource::Patch(bevy::prelude::UVec2::new(2, 3)),
            item: HOES.to_string(),
            units: 0.0,
            required: HOES_REQUIRED,
        }];
        allocation.set_scouting_range(scouting_range, None);
    }
    app.world.get_mut::<BandBench>(band).unwrap().auto = true;
    staff(&mut app, band);
    (app, band)
}

/// **Ray's case: every hand is taken before the bench is dealt any.** Auto still queues the top
/// suggestion; the order has no crafter and its `blockedReason` names what took the hands.
#[test]
fn auto_queues_with_no_hand_to_spare_and_the_order_names_what_took_the_hands() {
    let (mut app, band) = auto_band(6, SCOUTING_RANGE_WANTING_EVERY_HAND, true);
    assert!(
        auto_craft_fill(&mut app.world, band),
        "no idle hand is not a reason to hold the order back"
    );
    let frame = read_wait_frame(&mut app, band);
    assert_eq!(frame.idle_workers, 0, "liveness: every hand is spoken for");
    assert_eq!(frame.orders, 1, "the auto order is on the bench");
    let (reason, crew) = frame.order_rows[0].clone();
    assert_eq!(crew, 0, "it got no crafter");
    let scouts = app
        .world
        .get::<BandBench>(band)
        .unwrap()
        .first_call
        .scouting;
    assert!(scouts > 0, "liveness: the scouts took hands");
    assert_eq!(
        reason,
        format!("{EVERY_HAND_NEEDED_FIRST}: {scouts} scouting")
    );
    assert_eq!(frame.reason, "", "auto is not blocked on a suggestion");
}

/// **Control: with hands to spare the same auto order is queued and crewed**, and says nothing.
#[test]
fn auto_with_hands_to_spare_queues_a_crewed_order() {
    let (mut app, band) = auto_band(6, SCOUTING_RANGE_LEAVING_HANDS, true);
    assert!(auto_craft_fill(&mut app.world, band));
    let frame = read_wait_frame(&mut app, band);
    assert_eq!(frame.orders, 1);
    let (reason, crew) = frame.order_rows[0].clone();
    assert!(crew > 0, "the order took a crafter");
    assert_eq!(reason, "");
}

/// **Anti-flood: an auto order that got no crafter blocks a second one.**
#[test]
fn an_uncrewed_auto_order_blocks_another() {
    let (mut app, band) = auto_band(6, SCOUTING_RANGE_WANTING_EVERY_HAND, true);
    assert!(auto_craft_fill(&mut app.world, band));
    assert!(
        !auto_craft_fill(&mut app.world, band),
        "the uncrewed order is below its cap: the bench has no room"
    );
    assert_eq!(bench(&app, band).orders.len(), 1);
}

/// **A top suggestion short of a material reads `<Name>: <that offer's own reason>`.**
#[test]
fn auto_blocked_on_a_short_suggestion_publishes_the_offers_own_reason() {
    let (mut app, band) = auto_band(6, 0, false);
    assert!(
        !auto_craft_fill(&mut app.world, band),
        "nothing drawable: no order"
    );
    let frame = read_wait_frame(&mut app, band);
    let (item, name) = frame
        .suggestions
        .first()
        .cloned()
        .expect("liveness: a suggestion is published");
    let offer_reason = frame
        .offers
        .iter()
        .find(|(output, suggested, _)| output == &item && *suggested)
        .map(|(_, _, reason)| reason.clone())
        .expect("the suggested offer of the item");
    assert!(
        offer_reason.starts_with("Short") || offer_reason.starts_with("Needs"),
        "liveness: the top suggestion's offer is refused, got {offer_reason:?}"
    );
    assert_eq!(frame.reason, format!("{name}: {offer_reason}"));
}

/// **With auto off the reason is empty, whatever the band is short of.**
#[test]
fn auto_off_publishes_no_wait_reason() {
    let (mut app, band) = auto_band(6, SCOUTING_RANGE_WANTING_EVERY_HAND, true);
    app.world.get_mut::<BandBench>(band).unwrap().auto = false;
    let frame = read_wait_frame(&mut app, band);
    assert!(
        !frame.suggestions.is_empty(),
        "liveness: the same band has a suggestion"
    );
    assert_eq!(frame.reason, "");
}

/// The forage kit item: kitted hold heads are dealt in pass 1b, ahead of the bench's first crafter.
const BASKETS_ITEM: &str = "baskets";
/// More baskets than any fixture band has hands, so the shelf never limits a kitted head.
const BASKETS_ON_THE_SHELF: u32 = 40;
/// Extra hands past a hold crew in the control band.
const HANDS_BEYOND_THE_HOLD_CREW: u32 = 4;

/// An auto band (top suggestion `hoes`) with `hands` people, a shelf of baskets and ONE forage row on
/// the nearest patch, staffed. Returns the row's hold crew (the heads pass 1b deals it).
fn auto_band_with_a_kitted_forage_row(hands: u32) -> (App, Entity, u32) {
    let (mut app, band) = auto_band(hands, 0, true);
    let band_tile = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .current_tile;
    let band_pos = app.world.get::<core_sim::Tile>(band_tile).unwrap().position;
    let patch = app
        .world
        .resource::<core_sim::ForageRegistry>()
        .patches
        .keys()
        .copied()
        .min_by_key(|tile| {
            (
                tile.x.abs_diff(band_pos.x) + tile.y.abs_diff(band_pos.y),
                tile.x,
                tile.y,
            )
        })
        .expect("the world has forage patches");
    app.world.get_mut::<BandEquipment>(band).unwrap().stock(
        BASKETS_ITEM,
        BASKETS_ON_THE_SHELF,
        PLAIN_TIER,
        None,
    );
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .set_assignment(
            core_sim::LaborTarget::Forage {
                tile: patch,
                floor: 0.5,
                species: None,
                take_species: core_sim::TakeSelection::EVERYTHING,
            },
            hands,
            hands,
            None,
        );
    staff(&mut app, band);
    let hold = app
        .world
        .get::<LaborAllocation>(band)
        .unwrap()
        .assignments
        .iter()
        .map(|row| row.staffing.hold_crew)
        .sum();
    (app, band, hold)
}

/// **Ray's case: kitted hold crews take every staffable hand.** Auto still queues the top suggestion;
/// the order has no crafter and its `blockedReason` names the hold crews, counted off the staffing.
#[test]
fn a_hold_crew_that_takes_every_hand_is_named_on_the_uncrewed_auto_order() {
    // Learn the row's hold crew on a roomy band, then give a band exactly that many hands.
    let (_, _, hold) = auto_band_with_a_kitted_forage_row(HANDS_BEYOND_THE_HOLD_CREW * 4);
    assert!(hold > 0, "liveness: a kitted row holds a crew");
    let (mut app, band, same_hold) = auto_band_with_a_kitted_forage_row(hold);
    assert_eq!(
        same_hold, hold,
        "the hold crew does not depend on the pool here"
    );
    assert!(auto_craft_fill(&mut app.world, band), "auto queues anyway");
    let frame = read_wait_frame(&mut app, band);
    assert_eq!(frame.idle_workers, 0, "liveness: no hand is idle");
    assert_eq!(frame.orders, 1, "the auto order is on the bench");
    let (reason, crew) = frame.order_rows[0].clone();
    assert_eq!(crew, 0, "the hold crew took every hand before the bench");
    assert_eq!(
        reason,
        format!("{EVERY_HAND_NEEDED_FIRST}: {hold} holding rows")
    );
}

/// **Control: the same row with hands to spare leaves the order a crafter**, and nothing is blocked.
#[test]
fn a_hold_crew_that_leaves_a_hand_leaves_the_auto_order_a_crafter() {
    let (_, _, hold) = auto_band_with_a_kitted_forage_row(HANDS_BEYOND_THE_HOLD_CREW * 4);
    let (mut app, band, _) = auto_band_with_a_kitted_forage_row(hold + HANDS_BEYOND_THE_HOLD_CREW);
    assert!(auto_craft_fill(&mut app.world, band));
    let frame = read_wait_frame(&mut app, band);
    assert_eq!(frame.orders, 1);
    let (reason, crew) = frame.order_rows[0].clone();
    assert!(crew > 0, "the order took a crafter");
    assert_eq!(reason, "");
}

// ---------------------------------------------------------------------------------------------
// A High bench order takes its FIRST crafter ahead of the hold crews, and the row that paid says so.
// ---------------------------------------------------------------------------------------------

/// The recipes the High-order fixtures queue; both draw only bone, fibre and hide, which the band banks.
const FIRST_ORDER_RECIPE: &str = "hoes";
const SECOND_ORDER_RECIPE: &str = "traps";
/// Items per fixture order: enough that no order's cap is one crafter.
const ITEMS_PER_ORDER: u32 = 3;
/// The kind the encoded forage row publishes.
const FORAGE_KIND: &str = "forage";
/// The reason's name for a High opener's hands.
const HIGH_CRAFTING_WORD: &str = "High crafting";

/// What the encoded frame says about the forage row and the bench orders.
struct FirstFrame {
    /// `(workers, handsToBench)` of the forage row.
    forage: (u32, String),
    /// `handsToBench` of every published row.
    every_hands_to_bench: Vec<String>,
    /// `(displayName, crew, blockedReason)` per order, queue order.
    orders: Vec<(String, u32, String)>,
}

fn read_first_frame(app: &mut App, band: Entity) -> FirstFrame {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let band_id = app.world.get::<core_sim::BandId>(band).unwrap().0;
    let cohort = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == band_id)
        .expect("the band is published");
    let rows = cohort.laborAssignments().expect("the labor rows");
    let forage = rows
        .iter()
        .find(|row| row.kind() == Some(FORAGE_KIND))
        .map(|row| {
            (
                row.workers(),
                row.handsToBench().unwrap_or_default().to_string(),
            )
        })
        .expect("the forage row is published");
    let bench = cohort.bench().expect("the bench row");
    FirstFrame {
        forage,
        every_hands_to_bench: rows
            .iter()
            .map(|row| row.handsToBench().unwrap_or_default().to_string())
            .collect(),
        orders: bench
            .orders()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        (
                            row.displayName().unwrap_or_default().to_string(),
                            row.crewAtTool() + row.crewByHand(),
                            row.blockedReason().unwrap_or_default().to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// A hold-crew band (exactly `hold` hands plus `spare`) with the given orders queued and staffed.
fn hold_band_with_orders(spare: u32, orders: &[(&str, SourcePriority)]) -> (App, Entity, u32) {
    let (_, _, hold) = auto_band_with_a_kitted_forage_row(HANDS_BEYOND_THE_HOLD_CREW * 4);
    assert!(hold > 0, "liveness: a kitted row holds a crew");
    let (mut app, band, _) = auto_band_with_a_kitted_forage_row(hold + spare);
    app.world.get_mut::<BandBench>(band).unwrap().auto = false;
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    for (recipe, priority) in orders {
        queue(&mut app, band, recipe, ITEMS_PER_ORDER, *priority);
    }
    staff(&mut app, band);
    (app, band, hold)
}

/// **1. A High order takes one crafter off the hold crew, and the row names the order.**
#[test]
fn a_high_order_takes_a_hand_from_the_hold_crew_and_the_row_names_it() {
    let (mut app, band, hold) =
        hold_band_with_orders(0, &[(FIRST_ORDER_RECIPE, SourcePriority::High)]);
    let frame = read_first_frame(&mut app, band);
    let (name, crew, reason) = frame.orders[0].clone();
    assert_eq!(crew, 1, "the High order has its first crafter");
    assert_eq!(reason, "");
    assert_eq!(frame.forage.0, hold - 1, "the hold crew gave up one hand");
    assert_eq!(frame.forage.1, format!("1 to the {name} order"));
}

/// **2. Control: the same band with the order at Normal gives it nobody.**
#[test]
fn a_normal_order_takes_no_hand_from_the_hold_crew() {
    let (mut app, band, hold) =
        hold_band_with_orders(0, &[(FIRST_ORDER_RECIPE, SourcePriority::Normal)]);
    let frame = read_first_frame(&mut app, band);
    let (_, crew, reason) = frame.orders[0].clone();
    assert_eq!(crew, 0, "a Normal order waits behind the hold crew");
    assert!(
        reason.starts_with(EVERY_HAND_NEEDED_FIRST),
        "got {reason:?}"
    );
    assert_eq!(frame.forage.0, hold);
    assert_eq!(frame.forage.1, "");
}

/// **3. Two High orders each take one hand, and the row names both in bench order.**
#[test]
fn two_high_orders_each_take_one_hand_and_the_row_names_both() {
    let (mut app, band, hold) = hold_band_with_orders(
        0,
        &[
            (FIRST_ORDER_RECIPE, SourcePriority::High),
            (SECOND_ORDER_RECIPE, SourcePriority::High),
        ],
    );
    let frame = read_first_frame(&mut app, band);
    assert_eq!(frame.orders[0].1, 1, "the first order has one crafter");
    assert_eq!(frame.orders[1].1, 1, "the second order has one crafter");
    assert_eq!(frame.forage.0, hold - 2);
    assert_eq!(
        frame.forage.1,
        format!(
            "1 to the {} order, 1 to the {} order",
            frame.orders[0].0, frame.orders[1].0
        )
    );
}

/// **4. A High order with hands to spare is crewed and no row paid for it.**
#[test]
fn a_high_order_with_hands_to_spare_costs_no_row_anything() {
    let (mut app, band, hold) = hold_band_with_orders(
        HANDS_BEYOND_THE_HOLD_CREW,
        &[(FIRST_ORDER_RECIPE, SourcePriority::High)],
    );
    let frame = read_first_frame(&mut app, band);
    assert!(frame.orders[0].1 >= 1, "the High order is crewed");
    assert!(frame.forage.0 >= hold, "the hold crew is whole");
    assert!(
        frame.every_hands_to_bench.iter().all(String::is_empty),
        "no row gave a hand up: {:?}",
        frame.every_hands_to_bench
    );
}

/// **5. A crewless Normal order behind a High opener names the opener in its reason.**
#[test]
fn a_crewless_normal_order_names_the_high_crafting_ahead_of_it() {
    let (mut app, band, _) = hold_band_with_orders(
        0,
        &[
            (FIRST_ORDER_RECIPE, SourcePriority::High),
            (SECOND_ORDER_RECIPE, SourcePriority::Normal),
        ],
    );
    let frame = read_first_frame(&mut app, band);
    assert_eq!(frame.orders[1].1, 0, "the Normal order has no crafter");
    let first_call = app.world.get::<BandBench>(band).unwrap().first_call;
    assert_eq!(first_call.crafting, 1, "one High opener dealt");
    assert!(
        frame.orders[1].2.ends_with(HIGH_CRAFTING_WORD),
        "got {:?}",
        frame.orders[1].2
    );
}

// ---------------------------------------------------------------------------------------------
// THE END-OF-TURN RESTAFF (`systems::restaff_freed_hands`)
// ---------------------------------------------------------------------------------------------

/// The forage row's Max when it has room for a freed hand: the whole band.
const ROW_MAX_WITH_ROOM: u32 = 6;
/// The band's hands: one crafter on the order, the rest on the row.
const END_OF_TURN_HANDS: u32 = 6;
/// Turns the fixture may run before the one order must have finished (`spears` work 6 at 1 a turn).
const TURNS_TO_FINISH_ORDER: u32 = 20;
/// The forage row's escapement floor.
const END_OF_TURN_FLOOR: f32 = 0.5;

/// What the turn's own published frame says about the band: idle hands and the forage row's crew.
fn published_idle_and_row(app: &App, band: Entity) -> (u32, u32) {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let band_id = app.world.get::<core_sim::BandId>(band).unwrap().0;
    let cohort = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section")
        .iter()
        .find(|cohort| cohort.bandId() == band_id)
        .expect("the band is published");
    let row_workers = cohort
        .laborAssignments()
        .into_iter()
        .flatten()
        .map(|row| row.workers())
        .sum();
    (cohort.idleWorkers(), row_workers)
}

/// The band's forage row on the nearest patch with a Max of `row_max`: a bare hand works it, and it
/// has room for a freed hand exactly when `row_max` exceeds the hands it holds.
fn add_a_forage_row(app: &mut App, band: Entity, row_max: u32) {
    let band_tile = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .current_tile;
    let band_pos = app.world.get::<core_sim::Tile>(band_tile).unwrap().position;
    let patch = app
        .world
        .resource::<core_sim::ForageRegistry>()
        .patches
        .keys()
        .copied()
        .min_by_key(|tile| {
            (
                tile.x.abs_diff(band_pos.x) + tile.y.abs_diff(band_pos.y),
                tile.x,
                tile.y,
            )
        })
        .expect("the world has forage patches");
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .set_assignment(
            core_sim::LaborTarget::Forage {
                tile: patch,
                floor: END_OF_TURN_FLOOR,
                species: None,
                take_species: core_sim::TakeSelection::EVERYTHING,
            },
            row_max,
            END_OF_TURN_HANDS,
            None,
        );
}

/// A band holding one Spears order and one forage row of `row_max`, run turn by turn until the order
/// is retired. Returns `(frame before the retiring turn, frame after it)` as `(idle, row workers)`.
fn run_until_the_order_retires(row_max: u32) -> ((u32, u32), (u32, u32)) {
    let (mut app, band) = world(END_OF_TURN_HANDS);
    bank(&mut app, band, BONE, PLENTY, &BONE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    add_a_forage_row(&mut app, band, row_max);
    queue(&mut app, band, SPEARS, 1, SourcePriority::Normal);
    staff(&mut app, band);
    // The first frame predates the fixture; the turn after it is the first one the staffing shapes.
    app.update();
    let mut before = published_idle_and_row(&app, band);
    for _ in 0..TURNS_TO_FINISH_ORDER {
        app.update();
        if bench(&app, band).orders.is_empty() {
            return (before, published_idle_and_row(&app, band));
        }
        before = published_idle_and_row(&app, band);
    }
    panic!("liveness: the order never finished");
}

/// **A bench order that finishes and is retired this turn frees its crafter; the band is restaffed at
/// the end of the turn, so the frame shows the hand on the row with room and `idleWorkers` 0.**
/// Control: a row already at its Max has no room, so the freed hand stays idle (idle 1) - the
/// restaff does not conjure work.
#[test]
fn a_hand_freed_by_a_retired_order_is_dealt_before_the_frame_is_cut() {
    let ((idle_before, row_before), (idle_after, row_after)) =
        run_until_the_order_retires(ROW_MAX_WITH_ROOM);
    assert_eq!(idle_before, 0, "liveness: the crafter was working");
    assert_eq!(idle_after, 0, "the freed hand was dealt in the same turn");
    assert_eq!(row_after, row_before + 1, "it went onto the row with room");

    // Control: the row's Max is the crew it already holds, so it has no room for the freed hand.
    let (_, (idle_full, row_full)) = run_until_the_order_retires(row_before);
    assert_eq!(
        (idle_full, row_full),
        (1, row_before),
        "control: no row has room, so the hand stays idle"
    );
}

// ---------------------------------------------------------------------------------------------
// AUTO-CRAFT TOPS UP ITS OWN ORDER INSTEAD OF SKIPPING THE ROW
// ---------------------------------------------------------------------------------------------

/// The keeping line's new requirement after the first order is queued: more hoes wanted than the
/// order makes, so the row is still suggested.
const HOES_REQUIRED_AFTER: f32 = 9.0;

/// Queue the hoes order through one fill (tagged `auto` or not), raise what the keeping line wants,
/// restaff, and return the app with `(count, crew_cap, idle)` of the order before the second fill.
fn hoes_order_then_a_larger_need(auto: bool) -> (App, Entity, (u32, u32, u32)) {
    let (mut app, band) = auto_band(20, 0, true);
    assert!(
        auto_craft_fill(&mut app.world, band),
        "the first fill queues"
    );
    app.world.get_mut::<BandBench>(band).unwrap().orders[0].auto = auto;
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .last_keeping_issued[0]
        .required = HOES_REQUIRED_AFTER;
    staff(&mut app, band);
    let frame = read_wait_frame(&mut app, band);
    assert!(
        frame.suggestions.iter().any(|(item, _)| item == HOES),
        "liveness: hoes are still suggested"
    );
    let order = &bench(&app, band).orders[0];
    let before = (order.count, order.crew_cap, frame.idle_workers);
    (app, band, before)
}

/// **An auto order at its cap whose row is still suggested is raised by the suggestion's count**, its
/// crew cap rises and the extra hands are dealt; the same order placed by the player is not raised.
#[test]
fn auto_tops_up_its_own_order_when_the_row_is_still_suggested() {
    let (mut app, band, (count, cap, idle)) = hoes_order_then_a_larger_need(true);
    assert!(idle > 0, "liveness: hands are idle before the top-up");
    assert_eq!(
        read_wait_frame(&mut app, band).reason,
        "",
        "a pick that would top up reads no wait reason"
    );
    assert!(auto_craft_fill(&mut app.world, band), "the fill tops up");
    let bench = bench(&app, band);
    assert_eq!(bench.orders.len(), 1, "no second order");
    assert!(bench.orders[0].count > count, "the count was raised");
    assert!(bench.orders[0].crew_cap > cap, "the cap rose with it");
    let after = read_wait_frame(&mut app, band);
    assert!(after.idle_workers < idle, "the extra hands were dealt");
}

/// **Control: the player's order on the row is never edited by auto-craft.**
#[test]
fn auto_never_raises_a_players_order() {
    let (mut app, band, (count, _, _)) = hoes_order_then_a_larger_need(false);
    assert!(!auto_craft_fill(&mut app.world, band));
    assert_eq!(bench(&app, band).orders.len(), 1);
    assert_eq!(bench(&app, band).orders[0].count, count);
}
