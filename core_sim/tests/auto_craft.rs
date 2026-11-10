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
    advance_auto_craft, advance_auto_craft_after_bench, auto_craft_fill, set_auto_craft,
};
use core_sim::{
    advance_crafting, build_test_app, scalar_from_f32, BandBench, BandEquipment, BuildSource,
    DiscoveryProgressLedger, KeepingIssue, LaborAllocation, LadderConfigHandle,
    MaterialsConfigHandle, PopulationCohort, RecipesConfigHandle, ResidentBand,
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
    let _ = crew;
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

/// The state three tests below start from: the baskets running as an auto order while the clubs sit
/// in `auto_skipped`.
fn baskets_running_with_the_clubs_skipped(app: &mut App, band: Entity) {
    clubs_short_baskets_makeable(app, band);
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .auto_skipped
        .insert(CLUBS.to_string());
    set_auto_craft(&mut app.world, band, true);
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

/// The shipped baskets recipe.
const BASKETS_RECIPE_ID: &str = "baskets";

/// **TURNING AUTO OFF CLEARS THE SKIPPED SET AND LEAVES THE ORDERS ALONE.**
#[test]
fn toggling_off_clears_the_skips_and_keeps_every_order() {
    let (mut app, band) = world();
    baskets_running_with_the_clubs_skipped(&mut app, band);
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

/// Credit the band's people with a craft, past the ladder's completion threshold.
fn learn(app: &mut App, band: Entity, craft: &str) {
    let threshold = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .completion_threshold;
    let faction = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .faction;
    let id = core_sim::crafting::craft_discovery_id(craft).expect("a shipped craft");
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, id, scalar_from_f32(threshold));
}

/// **A BENCH TOOL THAT PAYS FOR ITSELF IS AUTO-CRAFTED FIRST, AND A LITTLE WORK DOES NOT GET ONE.**
/// Sleds queued behind no hide are blocked, so the bench can progress nothing and auto fills it. With
/// three sleds (24 hide work, 36 turns saved against 24 to build) the frame is queued; with one sled
/// the list is empty and nothing is. Tanning is known, and the frame's wood, fibre and bone are in
/// the store.
#[test]
fn a_tool_that_pays_for_itself_is_auto_crafted_and_a_little_work_gets_none() {
    for (sleds, expected) in [(3, vec![TANNING_FRAME.to_string()]), (1, Vec::new())] {
        let (mut app, band) = world();
        auto_bench(&mut app, band, STANDING_CREW);
        short_of(&mut app, band, &[]);
        learn(&mut app, band, "tanning");
        bank(
            &mut app,
            band,
            FIBRE,
            PLENTY,
            &[("fineness", 0.5), ("strength", 0.5)],
        );
        bank_bone(&mut app, band);
        bank(
            &mut app,
            band,
            "wood",
            PLENTY,
            &[("hardness", 0.5), ("pliancy", 0.5)],
        );
        app.world
            .get_mut::<BandBench>(band)
            .expect("bench")
            .enqueue("sled", sleds);

        set_auto_craft(&mut app.world, band, true);

        let queued: Vec<String> = queued_items(&app, band)
            .into_iter()
            .filter(|item| item != "sled")
            .collect();
        assert_eq!(queued, expected, "{sleds} sled(s) of blocked hide work");
    }
}

/// **THE BENCH'S AUTO STATE AND AN ORDER'S `auto` TAG SURVIVE A SAVE AND A LOAD.**
#[test]
fn the_auto_state_survives_a_save_round_trip() {
    let (mut app, band) = world();
    baskets_running_with_the_clubs_skipped(&mut app, band);
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
    baskets_running_with_the_clubs_skipped(&mut app, band);
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
        allocation.last_build_toe = vec![core_sim::BuildToeLine {
            source: BuildSource::Patch(FIRST_SITE),
            item: HOES.to_string(),
            required: 2.0,
            filled: 0.0,
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

/// **NO CHURN: A SUGGESTION ALREADY QUEUED - BLOCKED OR NOT - IS NEVER QUEUED AGAIN.** A pen short
/// of hurdles suggests hurdles; a blocked hurdles order is already on the bench, so auto adds none.
#[test]
fn a_suggestion_already_queued_is_not_queued_twice() {
    let (mut app, band) = world();
    auto_bench(&mut app, band, CREW);
    short_of(&mut app, band, &[(FIRST_SITE, BASKETS, 2.0)]);
    bank(
        &mut app,
        band,
        FIBRE,
        PLENTY,
        &[("fineness", 0.5), ("strength", 0.5)],
    );
    {
        let mut bench = app.world.get_mut::<BandBench>(band).expect("bench");
        bench.auto = true;
        // The player already queued baskets, short of hide: blocked, and the same row as the
        // suggestion.
        bench.enqueue(BASKETS_RECIPE_ID, 2);
    }
    for _ in 0..3 {
        advance_auto_craft(&mut app.world);
    }
    let baskets = bench(&app, band)
        .orders
        .iter()
        .filter(|order| order.recipe_id == BASKETS_RECIPE_ID)
        .count();
    assert_eq!(
        baskets, 1,
        "a blocked baskets order already stands for the suggestion"
    );
}
