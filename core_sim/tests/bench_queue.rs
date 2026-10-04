//! **The bench queue and the craft suggestions, read off the ENCODED frame**
//! (`docs/plan_crafting_and_materials.md` §7, "The queue" and "Suggestions").
//!
//! Every assertion here decodes the envelope a client receives — the queue a player edits and the
//! suggestion list the panel, the AI's Craft specialist and auto-craft all read are wire contracts,
//! and an in-process value that never reached the codec would pass an in-process assertion.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{App, Entity, UVec2};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

use core_sim::{
    advance_crafting, build_test_app, deliver_bench_output, recapture_snapshot_in_place,
    scalar_from_f32, BandBench, BandEquipment, BenchOrder, BuildSource, EquipmentConfigHandle,
    Expedition, ExpeditionMission, ExpeditionPhase, KeepingIssue, KitJob, LaborAllocation,
    LocalStore, MaterialsConfigHandle, PoolToeLine, PopulationCohort, ResidentBand,
    SnapshotHistory,
};
use std::collections::BTreeMap;

const SLED_RECIPE: &str = "sled";
const BASKETS_RECIPE: &str = "baskets";
const EARTHMOVING: &str = "earthmoving";
const HOES: &str = "hoes";
const HIDE: &str = "hide";
const FIBRE: &str = "fibre";
/// Material enough for several passes of either recipe.
const PLENTY: f32 = 40.0;
/// A crew that finishes a `work: 8` sled in one bare-handed turn.
const CREW_THAT_FINISHES_A_SLED_IN_ONE_TURN: u32 = 16;
/// One item.
const ONE: u32 = 1;
/// Nothing made yet.
const NONE_MADE: u32 = 0;
/// A forage tile for the site-crew fixture line — any tile; nothing here walks to it.
const PATCH: UVec2 = UVec2::new(2, 3);

/// A world with a resident band, captured once so the fixtures have a frame to read.
fn world() -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let band = app
        .world
        .query_filtered::<Entity, bevy::prelude::With<ResidentBand>>()
        .iter(&app.world)
        .next()
        .expect("the headless world spawns a resident band");
    (app, band)
}

/// `(recipe id, count, made, drawn)` for every order on the band's published bench, head first.
type PublishedOrder = (String, u32, u32, bool);
/// `(item id, count, workers without, work per turn, [(kind, job, workers without)])`.
type PublishedSuggestion = (String, u32, f32, f32, Vec<(String, String, f32)>);

/// Recapture, encode, and read `band`'s bench queue and suggestion list back off the envelope.
fn publish(app: &mut App, band: Entity) -> (Vec<PublishedOrder>, Vec<PublishedSuggestion>) {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let cohort = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.entity() == band.to_bits())
        .expect("the band is on the wire");
    let orders = cohort
        .bench()
        .and_then(|bench| bench.orders())
        .map(|orders| {
            orders
                .iter()
                .map(|order| {
                    (
                        order.recipeId().unwrap_or_default().to_string(),
                        order.count(),
                        order.made(),
                        order.drawn(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let suggestions = cohort
        .craftSuggestions()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    let sources = row
                        .sources()
                        .map(|sources| {
                            sources
                                .iter()
                                .map(|source| {
                                    (
                                        source.kind().unwrap_or_default().to_string(),
                                        source.job().unwrap_or_default().to_string(),
                                        source.workersWithout(),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    (
                        row.itemId().unwrap_or_default().to_string(),
                        row.count(),
                        row.workersWithout(),
                        row.workPerTurn(),
                        sources,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    (orders, suggestions)
}

/// Bank hide and fibre for several passes of either recipe, at fixed readings.
fn stock(app: &mut App, band: Entity) {
    let materials = app.world.resource::<MaterialsConfigHandle>().get();
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort");
    cohort.stores.clear_materials();
    for (material, axes) in [
        (HIDE, [("toughness", 0.6), ("suppleness", 0.5)]),
        (FIBRE, [("fineness", 0.5), ("strength", 0.5)]),
    ] {
        let readings: BTreeMap<String, f32> = axes
            .iter()
            .map(|(axis, value)| ((*axis).to_string(), *value))
            .collect();
        let key = materials
            .band_key(material, &readings)
            .expect("the shipped table carries this material");
        cohort
            .stores
            .deposit_material(material, key, scalar_from_f32(PLENTY), &readings);
    }
}

/// **THE QUEUE ADVANCES: a finished order leaves, and the next one starts DRAWING ITS OWN PILE.**
///
/// A sled ×1 at the head and baskets ×1 behind it, with a crew that finishes the sled in one turn.
/// Before the turn the wire carries both orders, head first; after it the sled order is gone, the
/// baskets are the head and have already cut their own pile, and the sled is parked for delivery.
#[test]
fn a_finished_order_leaves_the_queue_and_the_next_draws_its_own_inputs() {
    let (mut app, band) = world();
    stock(&mut app, band);
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("a spawned band carries a bench");
        bench.enqueue(SLED_RECIPE, ONE);
        bench.enqueue(BASKETS_RECIPE, ONE);
        bench.workers = CREW_THAT_FINISHES_A_SLED_IN_ONE_TURN;
    }
    let (before, _) = publish(&mut app, band);
    assert_eq!(
        before,
        vec![
            (SLED_RECIPE.to_string(), ONE, NONE_MADE, false),
            (BASKETS_RECIPE.to_string(), ONE, NONE_MADE, false),
        ],
        "the whole queue rides the wire, head first"
    );

    app.world.run_system_once(advance_crafting);
    let (after, _) = publish(&mut app, band);
    assert_eq!(
        after,
        vec![(BASKETS_RECIPE.to_string(), ONE, NONE_MADE, true)],
        "the sled order met its count and left; the baskets are the head and drew their own pile"
    );
    assert_eq!(
        app.world
            .get::<BandBench>(band)
            .expect("a spawned band carries a bench")
            .finished
            .len(),
        1,
        "the sled the popped order made is parked for delivery, not lost with the order"
    );
    assert_eq!(
        app.world
            .get::<BandBench>(band)
            .expect("a spawned band carries a bench")
            .workers,
        CREW_THAT_FINISHES_A_SLED_IN_ONE_TURN,
        "the crew stays with the bench across the change of order"
    );
}

/// **AN EMPTY QUEUE IS AN IDLE BENCH, AND IT MAKES NOTHING** — the liveness half of the claim
/// above: the last order popping leaves no job behind it, not a repeat of the one that finished.
#[test]
fn the_last_order_popping_leaves_an_idle_bench_that_makes_nothing_more() {
    /// Turns past the one that finishes the sled — a repeating job would make another on each.
    const MORE_TURNS: u32 = 3;
    let (mut app, band) = world();
    stock(&mut app, band);
    {
        let mut bench = app
            .world
            .get_mut::<BandBench>(band)
            .expect("a spawned band carries a bench");
        bench.enqueue(SLED_RECIPE, ONE);
        bench.workers = CREW_THAT_FINISHES_A_SLED_IN_ONE_TURN;
    }
    app.world.run_system_once(advance_crafting);
    app.world.run_system_once(deliver_bench_output);
    let held_after_one = held_hide(&app, band);
    for _ in 0..MORE_TURNS {
        app.world.run_system_once(advance_crafting);
        app.world.run_system_once(deliver_bench_output);
    }
    let (orders, _) = publish(&mut app, band);
    assert!(
        orders.is_empty(),
        "the queue is empty once its only order is met"
    );
    assert_eq!(
        held_hide(&app, band),
        held_after_one,
        "an idle bench draws nothing — the retired repeat-until-cleared job would have"
    );
}

fn held_hide(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .material_total(HIDE)
        .to_f32()
}

/// Write the settled tool lines a turn would have left on the band's allocation — a builders pool
/// short of earthmoving and a site crew short of hoes — so the capture scores exactly these.
fn short_of_earthmoving_and_hoes(allocation: &mut LaborAllocation) {
    /// The pool's requirement and what it was handed: two builders without.
    const POOL_REQUIRED: f32 = 3.0;
    const POOL_FILLED: f32 = 1.0;
    /// The site crew's claim, met with nothing: two keepers without.
    const SITE_REQUIRED: f32 = 2.0;
    const SITE_ISSUED: f32 = 0.0;
    allocation.last_pool_toe = vec![PoolToeLine {
        pool: KitJob::Builders,
        item: EARTHMOVING.to_string(),
        required: POOL_REQUIRED,
        filled: POOL_FILLED,
    }];
    allocation.last_keeping_issued = vec![KeepingIssue {
        source: BuildSource::Patch(PATCH),
        item: HOES.to_string(),
        units: SITE_ISSUED,
        required: SITE_REQUIRED,
    }];
}

/// **THE LIST IS RANKED BY WHO IS GOING WITHOUT, NETTED BY THE QUEUE, AND STATES WORK WHERE GEAR
/// ADDS WORK** — off the encoded frame.
///
/// Two builders without earthmoving and two keepers without hoes tie on the score, so the item id
/// orders them. Queueing one earthmoving set nets its count from 2 to 1 and leaves its score alone
/// (the people are still without it *this* turn). Both name their one source by kind and job, and
/// both state the work a turn the missing tools would add back.
#[test]
fn suggestions_rank_by_workers_without_and_net_out_the_queue() {
    /// Both items have two people going without.
    const TWO_WITHOUT: f32 = 2.0;
    const WHOLE_SHORTFALL: u32 = 2;
    const NETTED: u32 = 1;
    let (mut app, band) = world();
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("a spawned band carries an allocation");
        short_of_earthmoving_and_hoes(&mut allocation);
    }
    let (_, unqueued) = publish(&mut app, band);
    let ranked: Vec<(&str, u32)> = unqueued
        .iter()
        .map(|(item, count, ..)| (item.as_str(), *count))
        .collect();
    assert_eq!(
        ranked,
        vec![(EARTHMOVING, WHOLE_SHORTFALL), (HOES, WHOLE_SHORTFALL)],
        "a 2–2 tie on the score goes to the item id, and the count is the whole shortfall"
    );
    for (item, _, workers, work, sources) in &unqueued {
        assert_eq!(*workers, TWO_WITHOUT, "{item}: two people went without");
        assert!(
            *work > 0.0,
            "{item}: build/keeping gear states the work a turn it would add back"
        );
        assert_eq!(sources.len(), 1, "{item}: its one consumer is named");
    }
    assert_eq!(
        (unqueued[0].4[0].0.as_str(), unqueued[0].4[0].1.as_str()),
        ("pool", "builders"),
        "the pool's line is keyed by its pool token"
    );
    assert_eq!(
        (unqueued[1].4[0].0.as_str(), unqueued[1].4[0].1.as_str()),
        ("site", "forage"),
        "the site crew's line is keyed by the labor row that keeps it"
    );

    app.world
        .get_mut::<BandBench>(band)
        .expect("a spawned band carries a bench")
        .orders
        .push(BenchOrder::new(EARTHMOVING, ONE));
    let (_, queued) = publish(&mut app, band);
    let earthmoving = queued
        .iter()
        .find(|(item, ..)| item == EARTHMOVING)
        .expect("one set still short");
    assert_eq!(earthmoving.1, NETTED, "the queued set is netted out: 2 − 1");
    assert_eq!(
        earthmoving.2, TWO_WITHOUT,
        "…and the score is still who is without it this turn"
    );
}

/// **A DETACHED PARTY IS NOT A SOURCE** — it carries the kit it left with and is never
/// resupplied, so nothing crafted now reaches it. A party carrying the very lines that make its home
/// band publish suggestions publishes none. Paired with the home band, so a capture that published
/// nobody anything passes neither half.
#[test]
fn a_detached_party_publishes_no_suggestions() {
    let (mut app, band) = world();
    let (cohort, kit) = {
        let cohort = app
            .world
            .get::<PopulationCohort>(band)
            .expect("the band has a cohort")
            .clone();
        let kit = app.world.resource::<EquipmentConfigHandle>().get().no_kit();
        (cohort, kit)
    };
    let mut short = LaborAllocation::default();
    short_of_earthmoving_and_hoes(&mut short);
    let party = app
        .world
        .spawn((
            cohort,
            short.clone(),
            BandEquipment::default(),
            Expedition {
                home_band: band,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::AwaitingOrders,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit,
                cargo: LocalStore::new(),
                defection_pull: core_sim::Scalar::zero(),
            },
        ))
        .id();
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("a spawned band carries an allocation");
        short_of_earthmoving_and_hoes(&mut allocation);
    }

    let (_, home) = publish(&mut app, band);
    assert!(
        !home.is_empty(),
        "fixture: the home band, with the same lines, does publish suggestions"
    );
    let (_, detached) = publish(&mut app, party);
    assert!(
        detached.is_empty(),
        "a detached party is never resupplied, so it is suggested nothing: {detached:?}"
    );
}

/// **The bench row's worked-order reading**, off the encoded frame.
struct PublishedWorked {
    /// `BenchState.worked` — the index the bench row's scalars describe.
    worked: u32,
    /// `BenchState.recipeId` — the described order's recipe.
    recipe_id: String,
    /// `BenchState.blockedReason`.
    blocked_reason: String,
    /// `(recipe id, made, drawn, blockedReason, blockedSeverity)` per order, head first.
    orders: Vec<(String, u32, bool, String, String)>,
}

fn publish_worked(app: &mut App, band: Entity) -> PublishedWorked {
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let bench = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.entity() == band.to_bits())
        .expect("the band is on the wire")
        .bench()
        .expect("the band publishes its bench");
    PublishedWorked {
        worked: bench.worked(),
        recipe_id: bench.recipeId().unwrap_or_default().to_string(),
        blocked_reason: bench.blockedReason().unwrap_or_default().to_string(),
        orders: bench
            .orders()
            .map(|orders| {
                orders
                    .iter()
                    .map(|order| {
                        (
                            order.recipeId().unwrap_or_default().to_string(),
                            order.made(),
                            order.drawn(),
                            order.blockedReason().unwrap_or_default().to_string(),
                            order.blockedSeverity().unwrap_or_default().to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// **Fibre for two sled passes and short of one basket pass** — the sled takes 2 a pass bare-handed
/// and the baskets 5, so baskets at the head are short while the sled behind them can be made.
const FIBRE_FOR_TWO_SLEDS: f32 = 4.0;
/// The order index of the head and of the order behind it.
const HEAD: usize = 0;
const SECOND: usize = 1;
const SECOND_WORKED: u32 = 1;
const HEAD_WORKED: u32 = 0;
const THREE: u32 = 3;
/// The wire's severity for a skipped order.
const DANGER: &str = "danger";
/// The fineness / strength a fixture's fibre is banked at.
const FIBRE_AXES: [(&str, f32); 2] = [("fineness", 0.5), ("strength", 0.5)];

/// Bank `amount` of `material` at `axes`.
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

/// A band holding NO tools (so every draw is bare-handed, at the book's own amounts), plenty of hide
/// and `fibre` fibre, with `orders` queued and a crew that finishes either recipe in one turn.
fn bench_with(app: &mut App, band: Entity, fibre: f32, orders: &[(&str, u32)]) {
    *app.world
        .get_mut::<BandEquipment>(band)
        .expect("a spawned band carries an equipment ledger") = BandEquipment::default();
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .clear_materials();
    bank(
        app,
        band,
        HIDE,
        PLENTY,
        &[("toughness", 0.6), ("suppleness", 0.5)],
    );
    bank(app, band, FIBRE, fibre, &FIBRE_AXES);
    let mut bench = app
        .world
        .get_mut::<BandBench>(band)
        .expect("a spawned band carries a bench");
    for (recipe, count) in orders {
        bench.enqueue(recipe, *count);
    }
    bench.workers = CREW_THAT_FINISHES_A_SLED_IN_ONE_TURN;
}

/// **A SHORT HEAD NO LONGER STALLS THE QUEUE.** Baskets at the head are short of fibre; the sled
/// behind them can be made. The bench works the sled — it completes and leaves — while the baskets
/// keep index 0 with their skip reason on the wire. Once only the short head is left, nothing can be
/// worked, and the row reads as the blocked head: `worked` 0 and the head's reason.
#[test]
fn a_short_head_is_skipped_and_the_next_makeable_order_is_worked() {
    let (mut app, band) = world();
    bench_with(
        &mut app,
        band,
        FIBRE_FOR_TWO_SLEDS,
        &[(BASKETS_RECIPE, ONE), (SLED_RECIPE, ONE)],
    );

    let before = publish_worked(&mut app, band);
    assert_eq!(before.worked, SECOND_WORKED, "the sled is the worked order");
    assert_eq!(
        before.recipe_id, SLED_RECIPE,
        "and the bench row describes it"
    );
    let head = &before.orders[HEAD];
    assert_eq!(head.0, BASKETS_RECIPE);
    assert!(
        head.3.contains(FIBRE) && head.4 == DANGER,
        "the skipped head says why, in the bench's own words: {head:?}"
    );
    assert!(
        before.orders[SECOND].3.is_empty(),
        "an order that can draw carries no skip reason"
    );

    app.world.run_system_once(advance_crafting);
    let after = publish_worked(&mut app, band);
    assert_eq!(
        after.orders.len(),
        1,
        "the sled was made and left; the baskets are still queued"
    );
    assert_eq!(after.orders[HEAD].0, BASKETS_RECIPE, "…at index 0");
    assert_eq!(after.orders[HEAD].1, NONE_MADE, "…unmade");
    assert_eq!(
        app.world
            .get::<BandBench>(band)
            .expect("a spawned band carries a bench")
            .finished
            .len(),
        1,
        "the sled made past the short head is parked for delivery"
    );

    // A FULLY STUCK QUEUE reads as its blocked head.
    assert_eq!(
        after.worked, HEAD_WORKED,
        "nothing is workable: the row describes the head"
    );
    assert_eq!(after.recipe_id, BASKETS_RECIPE);
    assert!(
        !after.orders[HEAD].3.is_empty()
            && after.blocked_reason.contains(after.orders[HEAD].3.as_str()),
        "the bench's reason is the head's: bench {:?}, head {:?}",
        after.blocked_reason,
        after.orders[HEAD].3
    );
}

/// **THE HEAD TAKES PRIORITY BACK WHEN ITS STOCK ARRIVES**, and the order it overtakes mid-item is
/// paused with its pile. The sled (×3) makes one and re-draws while the baskets are short; fibre then
/// arrives, the baskets are worked again — made, and gone — and the sled still holds the pile it cut.
#[test]
fn the_head_resumes_priority_when_its_stock_arrives() {
    let (mut app, band) = world();
    bench_with(
        &mut app,
        band,
        FIBRE_FOR_TWO_SLEDS,
        &[(BASKETS_RECIPE, ONE), (SLED_RECIPE, THREE)],
    );
    app.world.run_system_once(advance_crafting);
    let skipped = publish_worked(&mut app, band);
    assert_eq!(
        skipped.worked, SECOND_WORKED,
        "fixture: the sled was worked"
    );
    assert_eq!(
        (skipped.orders[SECOND].1, skipped.orders[SECOND].2),
        (ONE, true),
        "fixture: one sled made and the next pass's pile already cut"
    );

    bank(&mut app, band, FIBRE, PLENTY, &FIBRE_AXES);
    let resumed = publish_worked(&mut app, band);
    assert_eq!(
        resumed.worked, HEAD_WORKED,
        "the head can draw again, so it is worked again"
    );
    assert!(
        resumed.orders[HEAD].3.is_empty(),
        "and it is no longer skipped"
    );

    app.world.run_system_once(advance_crafting);
    let done = publish_worked(&mut app, band);
    assert_eq!(
        done.orders.iter().map(|o| o.0.as_str()).collect::<Vec<_>>(),
        vec![SLED_RECIPE],
        "the baskets were worked to their count and left"
    );
    assert_eq!(
        (done.orders[HEAD].1, done.orders[HEAD].2),
        (ONE, true),
        "the overtaken sled kept its made count and the pile it had cut"
    );
}

/// **A SUGGESTION DOES NOT RISE ON THE TURN ITS ITEM IS MADE.** The labor pass settles the band's
/// tools before the bench runs, so on the completion turn the shortfall lines still read the item
/// missing — while its order has already counted it made, and here popped. The parked unit on
/// `BandBench::finished` is what keeps the netting whole until it is delivered and issued.
///
/// A site crew three sleds short and one sled queued: `3 − 1 = 2` before the turn. The bench makes the
/// sled (the order pops); the settled lines are untouched, exactly as on a real completion turn.
#[test]
fn a_suggestion_does_not_rise_on_the_turn_its_item_is_made() {
    /// Three sleds short at the site; one queued, so two suggested.
    const SITE_SHORT: f32 = 3.0;
    const NOTHING_ISSUED: f32 = 0.0;
    const SUGGESTED: u32 = 2;
    let (mut app, band) = world();
    bench_with(&mut app, band, FIBRE_FOR_TWO_SLEDS, &[(SLED_RECIPE, ONE)]);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("a spawned band carries an allocation")
        .last_keeping_issued = vec![KeepingIssue {
        source: BuildSource::Patch(PATCH),
        item: SLED_RECIPE.to_string(),
        units: NOTHING_ISSUED,
        required: SITE_SHORT,
    }];
    let sled_count = |suggestions: &[PublishedSuggestion]| {
        suggestions
            .iter()
            .find(|(item, ..)| item == SLED_RECIPE)
            .map(|(_, count, ..)| *count)
    };

    let (_, before) = publish(&mut app, band);
    assert_eq!(
        sled_count(&before),
        Some(SUGGESTED),
        "fixture: the queued sled is netted out of the three short"
    );

    app.world.run_system_once(advance_crafting);
    let (orders, after) = publish(&mut app, band);
    assert!(
        orders.is_empty()
            && !app
                .world
                .get::<BandBench>(band)
                .expect("a spawned band carries a bench")
                .finished
                .is_empty(),
        "fixture: the sled was made, its order popped, and the sled is parked for delivery"
    );
    assert_eq!(
        sled_count(&after),
        Some(SUGGESTED),
        "the sled just made is still coming — the suggestion must not rise by it"
    );
}
