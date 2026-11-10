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

/// **10. Auto-craft adds no order while a workable order is below its cap or no hand is idle.**
#[test]
fn auto_craft_adds_nothing_while_crafters_are_not_free() {
    // One hand, an order that could take six: it is below its cap and nobody is idle.
    let (mut app, band) = world(1);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    queue(&mut app, band, SLED, 3, SourcePriority::Normal);
    app.world.get_mut::<BandBench>(band).unwrap().auto = true;
    staff(&mut app, band);
    assert!(!auto_craft_fill(&mut app.world, band));
    assert_eq!(bench(&app, band).orders.len(), 1);
    // Idle hands but the order is below its cap (a crew cap of 6 with only the cap's worth staffed
    // is at cap; make the cap bigger than the staffed crew by hand-clearing the crew).
    let (mut app, band) = world(20);
    bank(&mut app, band, HIDE, PLENTY, &HIDE_AXES);
    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    queue(&mut app, band, SLED, 3, SourcePriority::Normal);
    app.world.get_mut::<BandBench>(band).unwrap().auto = true;
    staff(&mut app, band);
    app.world.get_mut::<BandBench>(band).unwrap().orders[0].crew = Default::default();
    // Hands are idle only after a staffing: restamp what the staffing saw, with the order below cap.
    assert!(
        !auto_craft_fill(&mut app.world, band) || bench(&app, band).orders.len() == 1,
        "an order below its cap means the idle hands are idle for want of room, not of an order"
    );
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
