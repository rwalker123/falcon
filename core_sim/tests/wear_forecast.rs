//! **"Coming up" craft suggestions - the kit that wears out before a new one could be made** (#778,
//! `.claude/rules/core_sim/crafting.md` -> "Craft suggestions").
//!
//! The fixtures stage the settled tool lines a turn would have left on the band's allocation (the way
//! `bench_queue.rs` does) and drive the wear through the ledger's own seam, `BandEquipment::wear_item`,
//! one turn at a time. Every suggestion assertion reads the ENCODED frame: the forecast is a wire
//! contract, and an in-process value that never reached the codec would pass an in-process one.
//!
//! The arithmetic the fixtures share: `hoes` wear 0.16 condition per work unit built over a durability
//! of 100, and `baskets` 0.04 per biomass gathered. A bare-handed crafter works bone and fibre at
//! `progress_per_worker_turn x hand_working.rate` = 0.5 a turn, so the hoes recipe (work 5) takes ten
//! crafter-turns and the baskets recipe (work 6) twelve.

use bevy::prelude::{App, Entity, UVec2, With};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

use core_sim::auto_craft::{auto_craft_fill, auto_craft_skip, set_auto_craft};
use core_sim::{
    build_test_app, recapture_snapshot_in_place, run_turn, scalar_from_f32, split_band_from_parent,
    BandBench, BandEquipment, BandId, BenchOrder, BuildSource, EquipmentBatch,
    EquipmentConfigHandle, KeepingIssue, LaborAllocation, MaterialsConfigHandle, PopulationCohort,
    RecipesConfigHandle, ResidentBand, SettleConfig, SnapshotHistory, WearQuantum,
};
use std::collections::BTreeMap;

const HOES: &str = "hoes";
const BASKETS: &str = "baskets";
const SPEARS: &str = "spears";
const PLAIN: &str = "plain";
const BONE: &str = "bone";
const HIDE: &str = "hide";
const FIBRE: &str = "fibre";
const PLENTY: f32 = 40.0;
/// Four crafters: the hoes recipe (work 5) at 0.5 a crafter-turn is 2.5 turns, quoted as 3.
const FOUR_CRAFTERS: u32 = 4;
/// Nobody at the bench: quoted at ONE crafter, so the hoes recipe is 10 turns.
const UNSTAFFED: u32 = 0;
/// Uses of `build_progress` a turn: x0.16 = 4 condition a turn.
const FOUR_A_TURN: f32 = 25.0;
/// Uses of `build_progress` a turn: x0.16 = 8 condition a turn.
const EIGHT_A_TURN: f32 = 50.0;
/// Uses of `biomass_gathered` a turn: x0.04 = 4 condition a turn.
const FOUR_A_TURN_GATHERING: f32 = 100.0;
const ITEM_SITE: UVec2 = UVec2::new(2, 3);

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

fn window_turns(app: &App) -> u32 {
    app.world
        .resource::<EquipmentConfigHandle>()
        .get()
        .wear_forecast
        .window_turns
}

/// A band holding NOTHING, no materials, an empty queue and a bench of `crew`.
fn bare_bench(app: &mut App, band: Entity, crew: u32) {
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
    bench.auto = false;
    bench.auto_skipped.clear();
    bench.workers = crew;
}

/// Settle the lines the turn would have left: `(item, required, issued)`, one keeping line each.
fn consumers(app: &mut App, band: Entity, lines: &[(&str, f32, f32)]) {
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("a spawned band carries an allocation");
    allocation.last_pool_toe.clear();
    allocation.last_queued_build_toe.clear();
    allocation.last_keeping_issued = lines
        .iter()
        .map(|(item, required, issued)| KeepingIssue {
            source: BuildSource::Patch(ITEM_SITE),
            item: (*item).to_string(),
            units: *issued,
            required: *required,
        })
        .collect();
}

/// Place `count` units of `item` already worn to `wear`, without touching the wear window.
fn hold(app: &mut App, band: Entity, item: &str, count: u32, wear: f32) {
    app.world
        .get_mut::<BandEquipment>(band)
        .expect("ledger")
        .place_batches(
            item,
            vec![EquipmentBatch {
                count,
                tier: PLAIN.to_string(),
                grade: None,
                wear,
            }],
        );
}

/// One turn of wear: the per-turn system's slot, then a charge through the ledger's own seam.
fn wear_turn(app: &mut App, band: Entity, item: &str, quantum: WearQuantum, uses: f32) {
    let window = window_turns(app);
    let config = app.world.resource::<EquipmentConfigHandle>().get();
    let mut ledger = app.world.get_mut::<BandEquipment>(band).expect("ledger");
    ledger.open_wear_slot(window);
    ledger.wear_item(&config, item, quantum, uses);
}

/// A full window of constant wear.
fn warm_up(app: &mut App, band: Entity, item: &str, quantum: WearQuantum, uses: f32) {
    for _ in 0..window_turns(app) {
        wear_turn(app, band, item, quantum, uses);
    }
}

fn bank(app: &mut App, band: Entity, material: &str, axes: &[(&str, f32)]) {
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
        .deposit_material(material, key, scalar_from_f32(PLENTY), &readings);
}

fn bank_everything(app: &mut App, band: Entity) {
    bank(app, band, BONE, &[("density", 0.5), ("length", 0.5)]);
    bank(app, band, HIDE, &[("toughness", 0.6), ("suppleness", 0.5)]);
    bank(app, band, FIBRE, &[("fineness", 0.5), ("strength", 0.5)]);
}

fn ledger(app: &App, band: Entity) -> &BandEquipment {
    app.world.get::<BandEquipment>(band).expect("ledger")
}

/// What a client reads of one suggestion.
#[derive(Debug, Clone, PartialEq)]
struct Published {
    item: String,
    count: u32,
    upcoming: u32,
    expires: f32,
    lead: u32,
    sources: usize,
    workers_without: f32,
    work_per_turn: f32,
}

/// Recapture, encode, and read `band`'s suggestion list back off the envelope.
fn publish(app: &mut App, band: Entity) -> Vec<Published> {
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
    cohort
        .craftSuggestions()
        .map(|rows| {
            rows.iter()
                .map(|row| Published {
                    item: row.itemId().unwrap_or_default().to_string(),
                    count: row.count(),
                    upcoming: row.upcomingCount(),
                    expires: row.expiresInTurns(),
                    lead: row.leadTurns(),
                    sources: row.sources().map_or(0, |sources| sources.len()),
                    workers_without: row.workersWithout(),
                    work_per_turn: row.workPerTurn(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn find<'a>(rows: &'a [Published], item: &str) -> Option<&'a Published> {
    rows.iter().find(|row| row.item == item)
}

/// **THE BASE FIXTURE**: two hoes, the unit in hand worn to 90 after a full window of 4 a turn, both
/// consumers' lines covered (so nothing is short TODAY), four crafters at the bench. The unit in hand
/// has 10 left, so at 4 a turn it runs out in 2.5 turns - inside the 3-turn lead.
fn two_hoes_wearing_out(app: &mut App, band: Entity) {
    bare_bench(app, band, FOUR_CRAFTERS);
    consumers(app, band, &[(HOES, 2.0, 2.0)]);
    let window = window_turns(app);
    hold(app, band, HOES, 2, 90.0 - 4.0 * window as f32);
    warm_up(app, band, HOES, WearQuantum::BuildProgress, FOUR_A_TURN);
}

/// **THE FORECAST IS THE ACTUAL RUN-OUT** on a deterministic fixture (constant staffing, no strikes):
/// the item forecast to run out within the lead time really retires a unit within it, and the
/// published `expiresInTurns` is the turn it retires, to the ceil. Paired with its control - a fresh
/// unit in hand is not forecast, and really does not retire in the lead time.
#[test]
fn a_unit_forecast_to_wear_out_within_the_lead_really_does() {
    let (mut app, band) = world();
    two_hoes_wearing_out(&mut app, band);
    assert_eq!(
        ledger(&app, band).live_units(HOES, &app.world.resource::<EquipmentConfigHandle>().get()),
        2,
        "LIVENESS: both hoes are live before the run-out"
    );

    let rows = publish(&mut app, band);
    let hoes = find(&rows, HOES).expect("the coming-up hoes are suggested");
    assert_eq!(hoes.upcoming, 1, "one unit runs out inside the lead time");
    assert_eq!(
        hoes.count, 1,
        "nothing is short today, so the count is the forecast's"
    );
    assert_eq!(
        hoes.lead, 3,
        "work 5 at four crafters x 0.5 = 2.5 turns, quoted whole"
    );
    assert!(
        (hoes.expires - 2.5).abs() < 1e-3,
        "10 condition left at 4 a turn: {}",
        hoes.expires
    );
    assert_eq!(
        (hoes.sources, hoes.workers_without, hoes.work_per_turn),
        (0, 0.0, 0.0)
    );

    // Run the real wear forward at the same staffing and see when the unit retires.
    let config = app.world.resource::<EquipmentConfigHandle>().get();
    let mut retired_on = None;
    for turn in 1..=hoes.lead {
        wear_turn(
            &mut app,
            band,
            HOES,
            WearQuantum::BuildProgress,
            FOUR_A_TURN,
        );
        if ledger(&app, band).retired_of(HOES) > 0 && retired_on.is_none() {
            retired_on = Some(turn);
        }
    }
    assert_eq!(
        retired_on,
        Some(hoes.expires.ceil() as u32),
        "the unit retires on the turn the published expiry rounds up to, inside the lead"
    );
    assert_eq!(ledger(&app, band).live_units(HOES, &config), 1);

    // CONTROL: the same band with a fresh unit in hand forecasts nothing, and retires nothing.
    let (mut control, band) = world();
    bare_bench(&mut control, band, FOUR_CRAFTERS);
    consumers(&mut control, band, &[(HOES, 2.0, 2.0)]);
    hold(&mut control, band, HOES, 2, 0.0);
    warm_up(
        &mut control,
        band,
        HOES,
        WearQuantum::BuildProgress,
        FOUR_A_TURN,
    );
    assert!(
        publish(&mut control, band).is_empty(),
        "a unit with most of its life left is not coming up"
    );
}

/// **A SPARE UNIT NOBODY HOLDS ABSORBS THE LOSS**: three hoes for two holders lose one inside the
/// lead and are still two. Paired with the two-hoe control above, which suggests.
#[test]
fn a_spare_unit_absorbs_the_forecast_loss() {
    let (mut app, band) = world();
    bare_bench(&mut app, band, FOUR_CRAFTERS);
    consumers(&mut app, band, &[(HOES, 2.0, 2.0)]);
    let window = window_turns(&app);
    hold(&mut app, band, HOES, 3, 90.0 - 4.0 * window as f32);
    warm_up(
        &mut app,
        band,
        HOES,
        WearQuantum::BuildProgress,
        FOUR_A_TURN,
    );
    assert!(
        publish(&mut app, band).is_empty(),
        "the third hoe covers the one that wears out"
    );

    // CONTROL: the same wear with no spare suggests.
    two_hoes_wearing_out(&mut app, band);
    assert!(
        find(&publish(&mut app, band), HOES).is_some(),
        "LIVENESS: without the spare the loss is suggested"
    );
}

/// **AN ITEM SHORT TODAY AND WEARING OUT IS ONE ENTRY WITH BOTH COUNTS**, ranked where today's
/// shortage puts it.
#[test]
fn an_item_short_today_and_wearing_out_is_one_entry_with_both_counts() {
    let (mut app, band) = world();
    bare_bench(&mut app, band, FOUR_CRAFTERS);
    // Three needed, two issued: one short today. Two held, the one in hand about to expire.
    consumers(&mut app, band, &[(HOES, 3.0, 2.0)]);
    let window = window_turns(&app);
    hold(&mut app, band, HOES, 2, 90.0 - 4.0 * window as f32);
    warm_up(
        &mut app,
        band,
        HOES,
        WearQuantum::BuildProgress,
        FOUR_A_TURN,
    );

    let rows = publish(&mut app, band);
    assert_eq!(
        rows.iter().filter(|row| row.item == HOES).count(),
        1,
        "one entry for the item"
    );
    let hoes = find(&rows, HOES).expect("suggested");
    assert_eq!(hoes.count, 2, "one short now and one more coming up");
    assert_eq!(hoes.upcoming, 1, "the forecast's share");
    assert_eq!(
        hoes.sources, 1,
        "the consumer going without today is still named"
    );
    assert!(hoes.workers_without > 0.0, "and it still scores");

    // The queue is netted ONCE, against today's shortfall first: a queued hoe covers the shortage and
    // leaves the forecast, a second covers the forecast too.
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .orders
        .push(BenchOrder::new(HOES, 1));
    let one_queued = publish(&mut app, band);
    let hoes = find(&one_queued, HOES).expect("still suggested");
    assert_eq!(
        (hoes.count, hoes.upcoming),
        (1, 1),
        "the queue struck today's shortage first"
    );
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .orders
        .push(BenchOrder::new(HOES, 1));
    assert!(
        find(&publish(&mut app, band), HOES).is_none(),
        "two queued cover both, and the suggestion drops"
    );
}

/// **COMING-UP-ONLY ITEMS RANK BELOW EVERY SHORTAGE, SOONEST FIRST.** Spears are short today with no
/// wear at all; the hoes run out in 2.5 turns and the baskets in 5.
#[test]
fn upcoming_only_items_rank_below_every_shortage_soonest_first() {
    let (mut app, band) = world();
    bare_bench(&mut app, band, FOUR_CRAFTERS);
    consumers(
        &mut app,
        band,
        &[(HOES, 2.0, 2.0), (BASKETS, 1.0, 1.0), (SPEARS, 2.0, 0.0)],
    );
    let window = window_turns(&app) as f32;
    hold(&mut app, band, HOES, 2, 90.0 - 4.0 * window);
    hold(&mut app, band, BASKETS, 1, 92.0 - 4.0 * window);
    for _ in 0..window_turns(&app) {
        let turn_config = app.world.resource::<EquipmentConfigHandle>().get();
        let window = window_turns(&app);
        let mut ledger = app.world.get_mut::<BandEquipment>(band).expect("ledger");
        ledger.open_wear_slot(window);
        ledger.wear_item(&turn_config, HOES, WearQuantum::BuildProgress, FOUR_A_TURN);
        ledger.wear_item(
            &turn_config,
            BASKETS,
            WearQuantum::BiomassGathered,
            FOUR_A_TURN_GATHERING,
        );
    }

    let rows = publish(&mut app, band);
    let order: Vec<&str> = rows.iter().map(|row| row.item.as_str()).collect();
    assert_eq!(
        order,
        vec![SPEARS, BASKETS, HOES],
        "the shortage first, then what is coming up soonest first (baskets in 2 turns, hoes in 2.5)"
    );
    assert!(
        rows[0].upcoming == 0 && rows[0].sources > 0,
        "spears are a plain shortage"
    );
    assert!(
        rows[1].expires < rows[2].expires && rows[1].expires > 0.0,
        "ordered by the turns to run out: {} < {}",
        rows[1].expires,
        rows[2].expires
    );
}

/// **AN UNSTAFFED BENCH QUOTES ITS LEAD AT ONE CRAFTER, AND A LONGER QUEUE AHEAD LENGTHENS IT AND
/// FLAGS MORE.** At 8 a turn the unit in hand (80 worn) lasts 2.5 turns and the next 12.5 after it.
#[test]
fn the_lead_time_follows_the_crew_and_the_queue() {
    let (mut app, band) = world();
    let fixture = |app: &mut App, crew: u32| {
        bare_bench(app, band, crew);
        consumers(app, band, &[(HOES, 2.0, 2.0)]);
        hold(app, band, HOES, 2, 0.0);
        warm_up(app, band, HOES, WearQuantum::BuildProgress, EIGHT_A_TURN);
    };

    fixture(&mut app, UNSTAFFED);
    let rows = publish(&mut app, band);
    let unstaffed = find(&rows, HOES).expect("an unstaffed bench still quotes");
    assert_eq!(unstaffed.lead, 10, "work 5 at ONE crafter x 0.5 = 10 turns");
    assert_eq!(unstaffed.upcoming, 1);

    fixture(&mut app, FOUR_CRAFTERS);
    let rows = publish(&mut app, band);
    let staffed = find(&rows, HOES).expect("suggested");
    assert_eq!(staffed.lead, 3, "four crafters: 2.5 turns, quoted whole");
    assert_eq!(
        staffed.upcoming, 1,
        "24 condition lost: the unit in hand and none of the next"
    );

    // Baskets ahead in the queue: 3 x work 6 at 2 a turn = 9 turns, plus the 2.5 for the hoe = 12.
    // 96 condition over 12 turns: the unit in hand (20 left) goes and 76 is taken off the next.
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .orders
        .push(BenchOrder::new("baskets", 3));
    let rows = publish(&mut app, band);
    let queued = find(&rows, HOES).expect("suggested");
    assert_eq!(queued.lead, 12, "the queue ahead lengthens the lead");
    assert!(queued.lead > staffed.lead);
    assert_eq!(
        queued.upcoming, 1,
        "96 condition does not reach the second unit's 120"
    );

    // Two more baskets: 5 x 6 / 2 = 15 turns + 2.5 = 17.5, quoted as 18 - 144 condition, which does.
    app.world.get_mut::<BandBench>(band).expect("bench").orders[0].count = 5;
    let rows = publish(&mut app, band);
    let longer = find(&rows, HOES).expect("suggested");
    assert_eq!(longer.lead, 18);
    assert_eq!(
        longer.upcoming, 2,
        "a longer queue ahead flags the second unit too"
    );
}

/// **NO WINDOW RECORDED, NO FORECAST** - however worn the unit in hand, a band that has recorded no
/// turn has no rate to project. Paired with a recorded-but-idle window, which forecasts nothing
/// either: a rate of zero is not a rate.
#[test]
fn no_recorded_wear_forecasts_nothing() {
    let (mut app, band) = world();
    bare_bench(&mut app, band, FOUR_CRAFTERS);
    consumers(&mut app, band, &[(HOES, 2.0, 2.0)]);
    hold(&mut app, band, HOES, 2, 99.0);
    assert_eq!(
        ledger(&app, band).wear_turns_recorded(),
        0,
        "LIVENESS: no window"
    );
    assert!(publish(&mut app, band).is_empty(), "no window, no forecast");

    let window = window_turns(&app);
    for _ in 0..window {
        app.world
            .get_mut::<BandEquipment>(band)
            .expect("ledger")
            .open_wear_slot(window);
    }
    assert!(
        publish(&mut app, band).is_empty(),
        "a window with nothing charged in it forecasts nothing"
    );
}

/// **THE WINDOW ROLLS**: only the last `window_turns` turns are averaged, so the rate follows a
/// change of staffing rather than remembering the old pace for ever.
#[test]
fn the_window_forgets_turns_beyond_its_length() {
    let (mut app, band) = world();
    bare_bench(&mut app, band, FOUR_CRAFTERS);
    hold(&mut app, band, HOES, 50, 0.0);
    warm_up(
        &mut app,
        band,
        HOES,
        WearQuantum::BuildProgress,
        EIGHT_A_TURN,
    );
    let busy = ledger(&app, band).wear_rate(HOES);
    assert!(
        (busy - 8.0).abs() < 1e-3,
        "8 a turn over a full window: {busy}"
    );

    // One idle turn out of ten: nine of the eight-a-turn turns remain in the window.
    let window = window_turns(&app);
    app.world
        .get_mut::<BandEquipment>(band)
        .expect("ledger")
        .open_wear_slot(window);
    let rested = ledger(&app, band).wear_rate(HOES);
    assert!(
        (rested - 7.2).abs() < 1e-3,
        "nine busy turns and one idle one average to 7.2: {rested}"
    );
    assert_eq!(
        ledger(&app, band).wear_turns_recorded(),
        window as usize,
        "the window never holds more than its length"
    );
}

/// **THE PER-TURN SYSTEM IS ON THE REAL SCHEDULE**: a band that never wears anything still records a
/// slot a turn, up to the window.
#[test]
fn the_schedule_opens_a_slot_every_turn_up_to_the_window() {
    let (mut app, band) = world();
    let before = ledger(&app, band).wear_turns_recorded();
    for _ in 0..3 {
        run_turn(&mut app);
    }
    let after = ledger(&app, band).wear_turns_recorded();
    assert_eq!(after, (before + 3).min(window_turns(&app) as usize));
    assert!(after > before, "LIVENESS: the system opened slots");
}

/// The queue-fill path auto-craft reads is the snapshot's own, so an auto bench queues a coming-up
/// item when nothing above it is usable - and a usable shortage above it wins.
#[test]
fn auto_craft_reaches_a_coming_up_item_only_when_nothing_above_it_is_usable() {
    let (mut app, band) = world();
    two_hoes_wearing_out(&mut app, band);
    bank_everything(&mut app, band);
    set_auto_craft(&mut app.world, band, true);
    let bench = app.world.get::<BandBench>(band).expect("bench");
    let recipes = app.world.resource::<RecipesConfigHandle>().get();
    let queued: Vec<String> = bench
        .orders
        .iter()
        .filter_map(|order| recipes.recipe(&order.recipe_id)?.output_equipment_id())
        .map(str::to_string)
        .collect();
    assert_eq!(
        queued,
        vec![HOES.to_string()],
        "the coming-up hoes are queued"
    );
    assert_eq!(bench.orders[0].count, 1, "at the forecast's count");
    assert!(bench.orders[0].auto);

    // CONTROL: a usable shortage above it is queued instead, and the hoes wait.
    two_hoes_wearing_out(&mut app, band);
    consumers(&mut app, band, &[(HOES, 2.0, 2.0), (BASKETS, 2.0, 0.0)]);
    {
        let mut bench = app.world.get_mut::<BandBench>(band).expect("bench");
        bench.orders.clear();
        bench.auto_skipped.clear();
        bench.auto = true;
    }
    auto_craft_fill(&mut app.world, band);
    let bench = app.world.get::<BandBench>(band).expect("bench");
    let queued: Vec<String> = bench
        .orders
        .iter()
        .filter_map(|order| recipes.recipe(&order.recipe_id)?.output_equipment_id())
        .map(str::to_string)
        .collect();
    assert_eq!(
        queued,
        vec![BASKETS.to_string()],
        "the usable shortage ranks above the forecast"
    );
}

/// **SKIP STILL WORKS ON A COMING-UP ITEM, and a stale skip is dropped once nothing is coming up.**
#[test]
fn a_skipped_coming_up_item_is_forgotten_when_it_is_no_longer_coming_up() {
    let (mut app, band) = world();
    two_hoes_wearing_out(&mut app, band);
    // No bone banked: the hoes order waits, and the player passes it over.
    set_auto_craft(&mut app.world, band, true);
    auto_craft_skip(&mut app.world, band).expect("the waiting hoes order is skipped");
    assert!(app
        .world
        .get::<BandBench>(band)
        .expect("bench")
        .auto_skipped
        .contains(HOES));

    // The unit is replaced: nothing is coming up, so the stale skip must not survive a fill.
    two_hoes_wearing_out(&mut app, band);
    hold(&mut app, band, HOES, 3, 0.0);
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .auto_skipped
        .insert(HOES.to_string());
    app.world.get_mut::<BandBench>(band).expect("bench").auto = true;
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench")
        .orders
        .clear();
    auto_craft_fill(&mut app.world, band);
    assert!(
        app.world
            .get::<BandBench>(band)
            .expect("bench")
            .auto_skipped
            .is_empty(),
        "the hoes are no longer suggested, so the skip is dropped"
    );
}

/// **THE WINDOW SURVIVES A SAVE AND A LOAD** - the rate a player was quoted does not reset.
#[test]
fn the_wear_window_survives_a_save_round_trip() {
    let (mut app, band) = world();
    two_hoes_wearing_out(&mut app, band);
    let band_id = *app.world.get::<BandId>(band).expect("band id");
    let before = ledger(&app, band).clone();
    assert!(
        before.wear_rate(HOES) > 0.0 && before.wear_turns_recorded() > 0,
        "LIVENESS: there is a window to lose"
    );

    let blob = core_sim::save::encode_save(&app.world).expect("the world encodes");
    let (mut loaded, _) = core_sim::save::load_save(&blob).expect("the save loads");
    let mut query = loaded.world.query::<(&BandId, &BandEquipment)>();
    let (_, after) = query
        .iter(&loaded.world)
        .find(|(id, _)| **id == band_id)
        .expect("the band came back");
    assert_eq!(after, &before, "the whole ledger, window included");
    assert_eq!(after.wear_rate(HOES), before.wear_rate(HOES));
}

/// **A SPLINTER STARTS WITH AN EMPTY WINDOW** and measures its own wear; the parent keeps its own.
#[test]
fn a_splinter_starts_with_an_empty_window() {
    let (mut app, parent) = world();
    run_turn(&mut app);
    two_hoes_wearing_out(&mut app, parent);
    assert!(
        ledger(&app, parent).wear_turns_recorded() > 0,
        "LIVENESS: the parent has a window to hand over"
    );
    let parent_rate = ledger(&app, parent).wear_rate(HOES);

    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split =
        split_band_from_parent(&mut app.world, parent, 3, &settle).expect("the split is admitted");
    let child = app
        .world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == split.band)
        .map(|(entity, _)| entity)
        .expect("the splinter exists");
    assert_eq!(
        ledger(&app, child).wear_turns_recorded(),
        0,
        "the splinter records nothing it did not wear"
    );
    assert_eq!(ledger(&app, child).wear_rate(HOES), 0.0);
    assert_eq!(
        ledger(&app, parent).wear_rate(HOES),
        parent_rate,
        "and the parent keeps its own window"
    );
}

/// The forecast spends the ledger's own units on a CLONE: publishing it never moves the real one.
#[test]
fn publishing_the_forecast_leaves_the_real_ledger_untouched() {
    let (mut app, band) = world();
    two_hoes_wearing_out(&mut app, band);
    let before = ledger(&app, band).clone();
    let _ = publish(&mut app, band);
    let _ = publish(&mut app, band);
    assert_eq!(
        ledger(&app, band),
        &before,
        "no charge, no retirement, no window write"
    );
}

/// Queue `count` spears ahead of everything: a long lead that nets none of the forecast items away.
fn long_queue(app: &mut App, band: Entity, count: u32) {
    let mut bench = app.world.get_mut::<BandBench>(band).expect("bench");
    bench.orders.clear();
    bench.orders.push(BenchOrder::new(SPEARS, count));
}

/// **`expiresInTurns` IS WHEN THE SHORTAGE STARTS, NOT WHEN THE UNIT IN HAND RUNS OUT.** Need 2, three
/// held, the one in hand 10 from spent at 4 a turn: the spare absorbs that loss, and nobody goes short
/// until the second unit is gone at (10 + 100) / 4 = 27.5 turns. Paired with the control without the
/// spare, which goes short at the first loss, 2.5 turns.
#[test]
fn expiry_is_the_turn_the_shortage_starts_and_a_spare_defers_it() {
    let (mut app, band) = world();
    let window = window_turns(&app) as f32;
    let fixture = |app: &mut App, held: u32| {
        bare_bench(app, band, FOUR_CRAFTERS);
        consumers(app, band, &[(HOES, 2.0, 2.0)]);
        hold(app, band, HOES, held, 90.0 - 4.0 * window);
        warm_up(app, band, HOES, WearQuantum::BuildProgress, FOUR_A_TURN);
        long_queue(app, band, 9);
    };
    fixture(&mut app, 3);
    let rows = publish(&mut app, band);
    let spared = find(&rows, HOES).expect("the second loss is still coming");
    assert!(
        spared.lead >= 28,
        "the lead reaches the second loss: {}",
        spared.lead
    );
    assert!(
        (spared.expires - 27.5).abs() < 1e-3,
        "the turn of the second loss: {}",
        spared.expires
    );
    assert_eq!(spared.upcoming, 1);

    fixture(&mut app, 2);
    let rows = publish(&mut app, band);
    let bare = find(&rows, HOES).expect("suggested");
    assert!(
        (bare.expires - 2.5).abs() < 1e-3,
        "the first loss: {}",
        bare.expires
    );
}

/// **AN EARLY EXPIRY WITH A LATE SHORTAGE RANKS BELOW AN ITEM THAT REALLY GOES SHORT SOONER.** The
/// hoes' unit in hand lasts 2.5 turns but a spare holds the line until turn 27.5; the baskets' only
/// unit goes at turn 3.
#[test]
fn ranking_follows_when_the_shortage_starts() {
    let (mut app, band) = world();
    let window = window_turns(&app) as f32;
    bare_bench(&mut app, band, FOUR_CRAFTERS);
    consumers(&mut app, band, &[(HOES, 2.0, 2.0), (BASKETS, 1.0, 1.0)]);
    hold(&mut app, band, HOES, 3, 90.0 - 4.0 * window);
    hold(&mut app, band, BASKETS, 1, 88.0 - 4.0 * window);
    for _ in 0..window_turns(&app) {
        wear_turn(
            &mut app,
            band,
            HOES,
            WearQuantum::BuildProgress,
            FOUR_A_TURN,
        );
        let config = app.world.resource::<EquipmentConfigHandle>().get();
        app.world
            .get_mut::<BandEquipment>(band)
            .expect("ledger")
            .wear_item(
                &config,
                BASKETS,
                WearQuantum::BiomassGathered,
                FOUR_A_TURN_GATHERING,
            );
    }
    long_queue(&mut app, band, 9);
    let rows = publish(&mut app, band);
    let order: Vec<&str> = rows.iter().map(|row| row.item.as_str()).collect();
    assert_eq!(order, vec![BASKETS, HOES], "baskets really go short first");
    assert!(rows[0].expires < rows[1].expires);
}

/// **A HUGE QUEUE CHANGES NOTHING ONCE EVERY UNIT IS GONE**: the walk stops, and the answer is the
/// one a queue just long enough gives.
#[test]
fn a_huge_queue_gives_the_answer_a_long_enough_one_does() {
    let (mut app, band) = world();
    let window = window_turns(&app) as f32;
    bare_bench(&mut app, band, FOUR_CRAFTERS);
    consumers(&mut app, band, &[(HOES, 2.0, 2.0)]);
    hold(&mut app, band, HOES, 2, 90.0 - 4.0 * window);
    warm_up(
        &mut app,
        band,
        HOES,
        WearQuantum::BuildProgress,
        FOUR_A_TURN,
    );
    long_queue(&mut app, band, 9);
    let enough = publish(&mut app, band);
    long_queue(&mut app, band, 1000);
    let huge = publish(&mut app, band);
    let (enough, huge) = (find(&enough, HOES).unwrap(), find(&huge, HOES).unwrap());
    assert!(
        huge.lead > enough.lead,
        "LIVENESS: the lead really is longer"
    );
    assert_eq!((huge.count, huge.upcoming), (enough.count, enough.upcoming));
    assert_eq!(huge.expires, enough.expires);
}
