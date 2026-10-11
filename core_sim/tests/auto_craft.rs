//! **Auto-craft — the bench keeps an empty queue filled from the ranked suggestions** (#779,
//! `.claude/rules/core_sim/crafting.md` → "Auto-craft").
//!
//! The fixture stages the settled tool lines a turn would have left on the band's allocation (the
//! way `bench_queue.rs` does), so the suggestion list is exactly the one the panel reads. Items are
//! chosen so their materials separate cleanly: `clubs` need bone, `baskets` need only fibre and
//! hide, so a band holding no bone is short on clubs and can still make baskets.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{App, Entity, UVec2, With};

use core_sim::auto_craft::{
    advance_auto_craft, advance_auto_craft_after_bench, auto_craft_fill, auto_craft_skip,
    set_auto_craft, AutoSkipRefusal,
};
use core_sim::{
    advance_crafting, build_test_app, scalar_from_f32, BandBench, BandEquipment, BuildSource,
    KeepingIssue, LaborAllocation, MaterialsConfigHandle, PopulationCohort, RecipesConfigHandle,
    ResidentBand,
};
use std::collections::BTreeMap;

const CLUBS: &str = "clubs";
const BASKETS: &str = "baskets";
/// A bench tool whose recipe requires crafts nobody knows on turn one.
const TANNING_FRAME: &str = "tanning_frame";
const HOES: &str = "hoes";
const BONE: &str = "bone";
const HIDE: &str = "hide";
const FIBRE: &str = "fibre";
/// Material enough for several passes of any recipe used here.
const PLENTY: f32 = 40.0;
/// A crew that finishes any recipe used here in one bare-handed turn.
const CREW: u32 = 16;
/// The crew a fixture stands at the bench to check auto-craft never moves it.
const STANDING_CREW: u32 = 5;
/// Two keepers' sites; nothing walks to either.
const FIRST_SITE: UVec2 = UVec2::new(2, 3);
const SECOND_SITE: UVec2 = UVec2::new(5, 6);

fn world() -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let band = app
        .world
        .query_filtered::<Entity, With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .expect("the headless world spawns a resident band");
    (app, band)
}

/// Settle a keeping line: `item` required `required` units at `site`, issued none.
fn short_of(app: &mut App, band: Entity, lines: &[(UVec2, &str, f32)]) {
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("a spawned band carries an allocation");
    allocation.last_pool_toe.clear();
    allocation.last_keeping_issued = lines
        .iter()
        .map(|(site, item, required)| KeepingIssue {
            source: BuildSource::Patch(*site),
            item: (*item).to_string(),
            units: 0.0,
            required: *required,
        })
        .collect();
}

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

fn bank_hide_and_fibre(app: &mut App, band: Entity) {
    bank(
        app,
        band,
        HIDE,
        PLENTY,
        &[("toughness", 0.6), ("suppleness", 0.5)],
    );
    bank(
        app,
        band,
        FIBRE,
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
}

fn bank_bone(app: &mut App, band: Entity) {
    bank(
        app,
        band,
        BONE,
        PLENTY,
        &[("density", 0.5), ("length", 0.5)],
    );
}

/// A band holding NO tools and no materials, standing `crew` at an auto bench with an empty queue.
fn auto_bench(app: &mut App, band: Entity, crew: u32) {
    *app.world
        .get_mut::<BandEquipment>(band)
        .expect("a spawned band carries an equipment ledger") = BandEquipment::default();
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .clear_materials();
    let mut bench = app
        .world
        .get_mut::<BandBench>(band)
        .expect("a spawned band carries a bench");
    bench.orders.clear();
    bench.auto_skipped.clear();
    bench.workers = crew;
}

fn bench(app: &App, band: Entity) -> &BandBench {
    app.world
        .get::<BandBench>(band)
        .expect("a spawned band carries a bench")
}

/// The equipment item a recipe makes.
fn item_of(app: &App, recipe_id: &str) -> String {
    app.world
        .resource::<RecipesConfigHandle>()
        .get()
        .recipe(recipe_id)
        .and_then(|recipe| recipe.output_equipment_id().map(str::to_string))
        .expect("an equipment recipe")
}

/// The items on the queue, head first, by the equipment they make.
fn queued_items(app: &App, band: Entity) -> Vec<String> {
    bench(app, band)
        .orders
        .iter()
        .map(|order| item_of(app, &order.recipe_id))
        .collect()
}

/// One turn's bench path, in the schedule's order: fill, work, fill.
fn turn(app: &mut App) {
    advance_auto_craft(&mut app.world);
    app.world.run_system_once(advance_crafting);
    advance_auto_craft_after_bench(&mut app.world);
}

/// Clubs (needs bone) are the top suggestion; baskets (fibre and hide) are behind them. No bone is
/// banked, so the top is short while the second could be made.
fn clubs_short_baskets_makeable(app: &mut App, band: Entity) {
    auto_bench(app, band, STANDING_CREW);
    short_of(
        app,
        band,
        &[(FIRST_SITE, CLUBS, 4.0), (SECOND_SITE, BASKETS, 2.0)],
    );
    bank_hide_and_fibre(app, band);
}

/// **AUTO ON WITH AN EMPTY QUEUE QUEUES THE TOP SUGGESTION AT ITS WHOLE COUNT, TAGGED `auto`, AND
/// NEVER TOUCHES THE CREW.**
#[test]
fn the_top_suggestion_is_queued_at_its_whole_count_and_the_crew_is_untouched() {
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    assert!(
        bench(&app, band).orders.is_empty(),
        "LIVENESS: the queue starts empty"
    );

    set_auto_craft(&mut app.world, band, true);

    let bench = bench(&app, band);
    assert!(bench.auto, "the toggle is on");
    assert_eq!(bench.orders.len(), 1, "exactly one order is queued");
    let order = &bench.orders[0];
    assert!(order.auto, "the order is tagged as auto-craft's");
    assert_eq!(order.count, 4, "the whole shortfall, not one pass");
    assert_eq!(item_of(&app, &order.recipe_id), CLUBS, "the top suggestion");
    assert_eq!(bench.workers, STANDING_CREW, "the crew is never picked");
}

/// **A SHORT TOP SUGGESTION MAKES THE BENCH WAIT; A TURN NEVER WALKS DOWN THE LIST.**
#[test]
fn a_short_top_suggestion_waits_and_a_turn_does_not_queue_the_second() {
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    set_auto_craft(&mut app.world, band, true);

    for _ in 0..3 {
        turn(&mut app);
    }

    assert_eq!(
        queued_items(&app, band),
        vec![CLUBS.to_string()],
        "baskets could be made, and are NOT queued: the bench waits on the clubs"
    );
    assert_eq!(bench(&app, band).orders[0].made, 0, "nothing was made");
}

/// **SKIP PARKS THE HEAD'S ITEM, REMOVES THE HEAD AND QUEUES THE NEXT SUGGESTION** — and is refused
/// when the bench has a workable order or the head is the player's.
#[test]
fn skip_moves_on_and_is_refused_unless_the_bench_is_waiting_on_an_auto_head() {
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    set_auto_craft(&mut app.world, band, true);

    assert_eq!(
        auto_craft_skip(&mut app.world, band),
        Ok(CLUBS.to_string()),
        "the waiting auto head can be skipped"
    );
    let bench_after = bench(&app, band);
    assert!(
        bench_after.auto_skipped.contains(CLUBS),
        "the skipped item is parked"
    );
    assert_eq!(
        bench_after.orders.len(),
        1,
        "the head was replaced, not kept"
    );
    assert_eq!(
        queued_items(&app, band),
        vec![BASKETS.to_string()],
        "the next suggestion is queued"
    );
    assert!(bench_after.orders[0].auto);
    assert_eq!(bench_after.workers, STANDING_CREW, "the crew is untouched");

    // REFUSED: the bench has a workable order (the baskets can be drawn).
    assert_eq!(
        auto_craft_skip(&mut app.world, band),
        Err(AutoSkipRefusal::BenchIsWorking),
        "a bench that can work its head has nothing to skip past"
    );

    // REFUSED: nothing workable, but the head is the player's own order.
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .enqueue(CLUBS_RECIPE_ID, 1);
    assert!(
        !bench(&app, band).orders[0].auto,
        "LIVENESS: a player's order"
    );
    assert_eq!(
        auto_craft_skip(&mut app.world, band),
        Err(AutoSkipRefusal::HeadNotAuto),
        "only an auto head can be skipped"
    );

    // REFUSED: nothing is queued.
    let (mut app, band) = world();
    auto_bench(&mut app, band, STANDING_CREW);
    assert_eq!(
        auto_craft_skip(&mut app.world, band),
        Err(AutoSkipRefusal::NothingQueued)
    );
}

/// **A SKIPPED ITEM THAT BECOMES DRAWABLE DOES NOT PREEMPT A RUNNING ORDER; IT RETURNS WHEN THE
/// QUEUE IS NEXT EMPTY.**
#[test]
fn a_skipped_item_returns_only_when_the_queue_is_next_empty() {
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    set_auto_craft(&mut app.world, band, true);
    auto_craft_skip(&mut app.world, band).expect("skip the clubs");
    assert_eq!(queued_items(&app, band), vec![BASKETS.to_string()]);
    app.world.get_mut::<BandBench>(band).expect("bench").workers = CREW;

    // The bone arrives while the baskets (two of them) are running.
    bank_bone(&mut app, band);
    turn(&mut app);
    let after_one = bench(&app, band);
    assert_eq!(
        queued_items(&app, band),
        vec![BASKETS.to_string()],
        "the clubs are drawable now and do NOT preempt the running baskets"
    );
    assert_eq!(after_one.orders[0].made, 1, "the baskets are being made");
    assert!(
        after_one.auto_skipped.contains(CLUBS),
        "the skip stands while an order runs"
    );

    turn(&mut app);
    let after_two = bench(&app, band);
    assert_eq!(
        queued_items(&app, band),
        vec![CLUBS.to_string()],
        "the baskets finished, the queue emptied, and the clubs are back"
    );
    assert!(
        after_two.auto_skipped.is_empty(),
        "the item left the skipped set when it was queued again"
    );
}

/// **A PLAYER-QUEUED ORDER MEANS AUTO ADDS NOTHING.**
#[test]
fn a_player_order_on_the_queue_means_auto_adds_nothing() {
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .enqueue(BASKETS_RECIPE_ID, 1);
    set_auto_craft(&mut app.world, band, true);
    turn(&mut app);

    let bench = bench(&app, band);
    assert!(bench.auto, "LIVENESS: auto is on");
    assert!(
        bench.orders.iter().all(|order| !order.auto),
        "no auto order joined the player's"
    );
    assert!(
        !auto_craft_fill(&mut app.world, band),
        "and a direct fill agrees"
    );
}

/// The shipped bone clubs recipe.
const CLUBS_RECIPE_ID: &str = "clubs";
/// The shipped baskets recipe.
const BASKETS_RECIPE_ID: &str = "baskets";

/// **TURNING AUTO OFF CLEARS THE SKIPPED SET AND LEAVES THE ORDERS ALONE.**
#[test]
fn toggling_off_clears_the_skips_and_keeps_every_order() {
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    set_auto_craft(&mut app.world, band, true);
    auto_craft_skip(&mut app.world, band).expect("skip the clubs");
    assert!(
        !bench(&app, band).auto_skipped.is_empty(),
        "LIVENESS: something was skipped"
    );

    set_auto_craft(&mut app.world, band, false);

    let bench = bench(&app, band);
    assert!(!bench.auto, "off");
    assert!(bench.auto_skipped.is_empty(), "the skips are forgotten");
    assert_eq!(bench.orders.len(), 1, "the queue is left alone");
    assert!(
        bench.orders[0].auto,
        "an auto order stays what it was; the player can now edit it"
    );
    turn(&mut app);
    assert_eq!(
        queued_items(&app, band).len(),
        1,
        "and an off bench queues nothing further"
    );
}

/// **A COMPLETING ORDER LEAVES THE NEXT AUTO ORDER ALREADY QUEUED AFTER THE TURN.**
#[test]
fn a_completed_order_leaves_the_next_auto_order_queued_after_the_turn() {
    let (mut app, band) = world();
    auto_bench(&mut app, band, CREW);
    short_of(
        &mut app,
        band,
        &[(FIRST_SITE, CLUBS, 2.0), (SECOND_SITE, BASKETS, 1.0)],
    );
    bank_hide_and_fibre(&mut app, band);
    bank_bone(&mut app, band);
    set_auto_craft(&mut app.world, band, true);
    assert_eq!(queued_items(&app, band), vec![CLUBS.to_string()]);
    assert_eq!(bench(&app, band).orders[0].count, 2);

    turn(&mut app);
    assert_eq!(bench(&app, band).orders[0].made, 1, "one club made");
    turn(&mut app);

    assert_eq!(
        queued_items(&app, band),
        vec![BASKETS.to_string()],
        "the clubs completed this turn and the next auto order is already in the queue"
    );
    assert!(bench(&app, band).orders[0].auto);
}

/// **A SUGGESTION FOR A CRAFT THE PEOPLE HAVE NOT LEARNED IS PASSED OVER.** The tanning frame needs
/// crafts nobody knows on turn one; the queue would refuse it, so auto goes to the next suggestion.
#[test]
fn an_unlearned_craft_is_passed_over() {
    let (mut app, band) = world();
    auto_bench(&mut app, band, STANDING_CREW);
    short_of(
        &mut app,
        band,
        &[(FIRST_SITE, TANNING_FRAME, 5.0), (SECOND_SITE, HOES, 2.0)],
    );
    bank_hide_and_fibre(&mut app, band);
    bank_bone(&mut app, band);

    set_auto_craft(&mut app.world, band, true);

    assert_eq!(
        queued_items(&app, band),
        vec![HOES.to_string()],
        "the top suggestion needs an unlearned craft, so the next queueable one is queued"
    );
}

/// **THE BENCH'S AUTO STATE AND AN ORDER'S `auto` TAG SURVIVE A SAVE AND A LOAD.**
#[test]
fn the_auto_state_survives_a_save_round_trip() {
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    set_auto_craft(&mut app.world, band, true);
    auto_craft_skip(&mut app.world, band).expect("skip the clubs");
    let band_id = *app.world.get::<core_sim::BandId>(band).expect("band id");
    let before: BandBench = bench(&app, band).clone();
    assert!(
        before.auto && !before.auto_skipped.is_empty() && before.orders[0].auto,
        "LIVENESS: there is auto state to lose"
    );

    let blob = core_sim::save::encode_save(&app.world).expect("the world encodes");
    let (mut loaded, _) = core_sim::save::load_save(&blob).expect("the save loads");
    let mut restored = loaded
        .world
        .query::<(&core_sim::BandId, &core_sim::BandBench)>();
    let (_, after) = restored
        .iter(&loaded.world)
        .find(|(id, _)| **id == band_id)
        .expect("the band came back");
    assert_eq!(after.auto, before.auto);
    assert_eq!(after.auto_skipped, before.auto_skipped);
    assert_eq!(
        after.orders.iter().map(|o| o.auto).collect::<Vec<_>>(),
        before.orders.iter().map(|o| o.auto).collect::<Vec<_>>()
    );
}

/// **A SKIPPED ITEM NOBODY IS GOING WITHOUT ANY MORE LEAVES `auto_skipped`** at the next fill of an
/// empty queue - there is nothing left to skip.
#[test]
fn a_skipped_item_no_longer_needed_leaves_the_skipped_set() {
    let (mut app, band) = world();
    clubs_short_baskets_makeable(&mut app, band);
    set_auto_craft(&mut app.world, band, true);
    auto_craft_skip(&mut app.world, band).expect("skip the clubs");
    assert!(bench(&app, band).auto_skipped.contains(CLUBS));

    // The need is covered: no line is short of clubs any more. Empty the queue and fill.
    short_of(&mut app, band, &[(SECOND_SITE, BASKETS, 2.0)]);
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .orders
        .clear();
    auto_craft_fill(&mut app.world, band);

    assert!(
        bench(&app, band).auto_skipped.is_empty(),
        "the clubs are not suggested, so the stale skip is dropped"
    );
}

/// **AUTO-CRAFT CRAFTS FOR A QUEUED BUILD BEFORE IT REACHES THE HEAD.** Nothing is short now; the
/// second job in the build queue will want hoes. The top suggestion is those hoes, so an auto bench
/// queues them at the job's count ahead of time.
#[test]
fn auto_crafts_the_tools_a_queued_build_will_need() {
    let (mut app, band) = world();
    auto_bench(&mut app, band, STANDING_CREW);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("a spawned band carries an allocation");
        allocation.last_pool_toe.clear();
        allocation.last_keeping_issued.clear();
        allocation.last_queued_build_toe = vec![core_sim::QueuedBuildToe {
            position: 1,
            source: BuildSource::Patch(FIRST_SITE),
            item: HOES.to_string(),
            required: 2.0,
        }];
    }
    bank_hide_and_fibre(&mut app, band);
    bank_bone(&mut app, band);

    set_auto_craft(&mut app.world, band, true);

    let queued = bench(&app, band);
    assert_eq!(queued_items(&app, band), vec![HOES.to_string()]);
    assert!(queued.orders[0].auto);
    assert_eq!(queued.orders[0].count, 2, "the whole count the job needs");
}
