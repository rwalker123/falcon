//! **The band judges its leader by what he delivers** (issue #710,
//! `.claude/rules/core_sim/leadership.md` -> "The band judges its leader by what he delivers"): a
//! signed `leadership` morale term for a band WITH a leader. Met orders earn popularity, falling orders
//! and going short at the meal earn strain (scaled by the band's patience), and every met order drifts
//! the band's culture toward Hierarchical.
//!
//! Driven through the real `simulate_population` / `advance_leadership` on a generated world. Every
//! expected number is derived from the config the world loaded; each claim has its control.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::{
    advance_leadership, publish_baseline_snapshot, scalar_from_f32, simulate_population, BandId,
    BandLeader, BandLeadership, BandOrder, CultureManager, CultureOwner, CultureTraitAxis,
    LaborAllocation, LeaderKind, OrderOwner, PopulationCohort, ResidentBand, Scalar,
    SnapshotHistory, WellbeingConfig, WellbeingConfigHandle, CULTURE_TRAIT_SPAN,
};
use faction_support::{one_faction_world, HOME};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The food order's kind id.
const FOOD_KIND: &str = "food";
/// The order's target, in turns of food.
const TARGET_TURNS: u32 = 5;
/// A larder deep enough that the stock covers the target many times over and nobody goes short.
const DEEP_LARDER: f32 = 1.0e6;
/// A larder that covers the meal but sits below the target's turns of cover.
const BELOW_TARGET_TURNS: f32 = 2.0;
/// The share of the meal the staged hungry band is given (the rest is a deficit).
const HALF_MEAL: f32 = 0.5;
/// The wire value of `MoraleCause::Leadership`.
const LEADERSHIP_CAUSE_WIRE: u8 = 6;
/// A hunger strain that dwarfs every other morale contributor.
const OVERWHELMING_STRAIN: f32 = 1.0;
/// Fixed-point comparison tolerance, in raw units.
const RAW_TOLERANCE: i64 = 4;
/// The tile's turn the leader emerged on (any).
const SINCE_TURN: u64 = 1;

fn wellbeing(app: &App) -> std::sync::Arc<WellbeingConfig> {
    app.world.resource::<WellbeingConfigHandle>().get()
}

fn tune(app: &mut App, edit: impl FnOnce(&mut WellbeingConfig)) {
    let mut tuned = (*wellbeing(app)).clone();
    edit(&mut tuned);
    app.world
        .insert_resource(WellbeingConfigHandle::new(std::sync::Arc::new(tuned)));
}

fn lone_band() -> (App, Entity, BandId) {
    let mut app = one_faction_world();
    let (band, id) = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .map(|(entity, id, _)| (entity, *id))
        .min_by_key(|(_, id)| *id)
        .expect("the home faction has a band");
    (app, band, id)
}

fn cohort(app: &App, band: Entity) -> &PopulationCohort {
    app.world.get::<PopulationCohort>(band).unwrap()
}

/// Give the band a leader (or take it away) without going through emergence.
fn set_leader(app: &mut App, band: Entity, led: bool) {
    app.world.entity_mut(band).insert(BandLeadership {
        leader: led.then(|| BandLeader {
            name: "Orrin".to_string(),
            kind: LeaderKind::Chief,
            since_turn: SINCE_TURN,
        }),
        ..BandLeadership::default()
    });
}

/// Pin the band's own `HierarchicalEgalitarian` reading.
fn set_axis(app: &mut App, id: BandId, value: f32) {
    let mut culture = app.world.resource_mut::<CultureManager>();
    let layer = culture
        .band_layer_mut_by_owner(CultureOwner::from_band(id))
        .expect("a band has a culture layer by its first turn");
    layer.traits.update_value(
        CultureTraitAxis::HierarchicalEgalitarian.index(),
        scalar_from_f32(value),
    );
}

fn modifier(app: &App, id: BandId) -> Scalar {
    app.world
        .resource::<CultureManager>()
        .band_layer_by_owner(CultureOwner::from_band(id))
        .unwrap()
        .traits
        .modifier()[CultureTraitAxis::HierarchicalEgalitarian.index()]
}

/// Hold one food order of `owner`, with the larder at `stock_turns` turns of the meal.
fn stage_order(app: &mut App, band: Entity, owner: OrderOwner, stock_turns: f32) {
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(band).unwrap();
        allocation.orders.clear();
        allocation.orders.insert(
            FOOD_KIND.to_string(),
            BandOrder {
                target: TARGET_TURNS,
                owner,
                ..BandOrder::default()
            },
        );
    }
    // The draw the order is judged against is the meal itself, so `stock_turns` is turns of cover.
    let meal = demand(app, band);
    let mut c = app.world.get_mut::<PopulationCohort>(band).unwrap();
    c.last_food_need = meal;
    c.stores
        .reset_food("dry", scalar_from_f32(stock_turns * meal));
}

/// The meal the band's people eat this turn, which is what the larder is staged against.
fn demand(app: &App, band: Entity) -> f32 {
    let c = cohort(app, band);
    let consumption = &app
        .world
        .resource::<core_sim::DemographicsConfigHandle>()
        .get()
        .consumption;
    (c.children.to_f32() * consumption.child_factor
        + c.working.to_f32() * consumption.working_factor
        + c.elders.to_f32() * consumption.elder_factor)
        * consumption.per_capita_draw
}

fn clear_orders(app: &mut App, band: Entity) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .unwrap()
        .orders
        .clear();
}

fn turn(app: &mut App) {
    app.world.run_system_once(simulate_population);
}

fn term(app: &App, band: Entity) -> f32 {
    cohort(app, band)
        .last_morale_contributions
        .leadership
        .to_f32()
}

fn patience(app: &App, axis: f32) -> f32 {
    wellbeing(app)
        .leadership
        .patience(axis / CULTURE_TRAIT_SPAN)
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    let tolerance = RAW_TOLERANCE as f32 / Scalar::SCALE as f32;
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: got {actual}, expected {expected}"
    );
}

/// A led band whose one order is below target and falling, on a larder that still covers the meal.
fn falling_band(axis: f32) -> (App, Entity) {
    let (mut app, band, id) = lone_band();
    set_leader(&mut app, band, true);
    set_axis(&mut app, id, axis);
    stage_order(&mut app, band, OrderOwner::Chief, BELOW_TARGET_TURNS);
    assert!(
        BELOW_TARGET_TURNS > 1.0 && BELOW_TARGET_TURNS < TARGET_TURNS as f32,
        "fixture: the larder covers the meal but not the target, so the strain is the order's alone"
    );
    turn(&mut app);
    (app, band)
}

/// **A band with no leader has no term**, even when its people are starving.
#[test]
fn a_leaderless_band_reads_zero_even_when_starving() {
    let (mut app, band, _) = lone_band();
    set_leader(&mut app, band, false);
    stage_order(&mut app, band, OrderOwner::Chief, DEEP_LARDER);
    let meal = demand(&app, band);
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .stores
        .reset_food("dry", scalar_from_f32(meal * HALF_MEAL));
    turn(&mut app);
    assert_eq!(term(&app, band), 0.0);
    assert_eq!(
        cohort(&app, band).last_leadership,
        core_sim::LeadershipReading::default()
    );
}

/// **Delivery earns popularity and drifts the band toward Hierarchical.**
#[test]
fn a_met_order_earns_popularity_and_drifts_the_band_hierarchical() {
    let (mut app, band, id) = lone_band();
    set_leader(&mut app, band, true);
    stage_order(&mut app, band, OrderOwner::Chief, DEEP_LARDER);
    let before = modifier(&app, id);
    turn(&mut app);
    let config = wellbeing(&app).leadership.clone();
    assert_eq!(cohort(&app, band).last_leadership.met_orders, 1);
    assert_close(
        term(&app, band),
        config.popularity_per_met_order,
        "one met order",
    );
    assert!(!cohort(&app, band).last_leadership.hungry);

    app.world.run_system_once(advance_leadership);
    let moved = (before - modifier(&app, id)).to_f32();
    assert_close(moved, config.drift_per_met_order, "toward hierarchical");
}

/// **A failing order does not drift the band either way.**
#[test]
fn a_failing_order_does_not_drift_the_band() {
    let (mut app, band) = falling_band(0.0);
    let id = *app.world.get::<BandId>(band).unwrap();
    let before = modifier(&app, id);
    app.world.run_system_once(advance_leadership);
    assert_eq!(modifier(&app, id), before);
}

/// **A falling order is a small strain**, scaled by the band's patience.
#[test]
fn a_falling_order_is_a_minor_negative_term() {
    let (app, band) = falling_band(0.0);
    let config = wellbeing(&app).leadership.clone();
    assert_eq!(cohort(&app, band).last_leadership.falling_orders, 1);
    assert_close(
        term(&app, band),
        -config.strain_per_falling_order * patience(&app, 0.0),
        "one falling order",
    );
}

/// **Going short at the meal is the major strain**, larger than a falling order's.
#[test]
fn a_band_that_goes_short_at_the_meal_is_strained_more_than_by_a_falling_order() {
    let (mut app, band, id) = lone_band();
    set_leader(&mut app, band, true);
    set_axis(&mut app, id, 0.0);
    clear_orders(&mut app, band);
    let meal = demand(&app, band);
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .stores
        .reset_food("dry", scalar_from_f32(meal * (1.0 - HALF_MEAL)));
    turn(&mut app);
    let config = wellbeing(&app).leadership.clone();
    assert!(cohort(&app, band).last_leadership.hungry);
    // Half the meal missing is far past `starvation_full_at`, so the strain is whole.
    assert!(HALF_MEAL >= config.starvation_full_at);
    let hungry = term(&app, band);
    assert_close(
        hungry,
        -config.starvation_strain * patience(&app, 0.0),
        "a whole hunger strain",
    );
    let (falling_app, falling_band) = falling_band(0.0);
    assert!(
        hungry < term(&falling_app, falling_band),
        "hunger ({hungry}) is the major term, a falling order the minor one"
    );
}

/// **An egalitarian band turns on a failing leader sooner than a hierarchical one bears it.**
#[test]
fn patience_is_lower_in_an_egalitarian_band_than_a_hierarchical_one() {
    let (egalitarian_app, egalitarian) = falling_band(CULTURE_TRAIT_SPAN);
    let (hierarchical_app, hierarchical) = falling_band(-CULTURE_TRAIT_SPAN);
    let egalitarian_term = term(&egalitarian_app, egalitarian);
    let hierarchical_term = term(&hierarchical_app, hierarchical);
    assert!(
        egalitarian_term < hierarchical_term,
        "the same failure strains the egalitarian band ({egalitarian_term}) more than the \
         hierarchical one ({hierarchical_term})"
    );
    assert!(
        hierarchical_term < 0.0,
        "the hierarchical band still strains"
    );
}

/// **A player-crewed order that is met credits the leader exactly as a chief-owned one does.**
#[test]
fn a_player_owned_met_order_credits_the_leader() {
    let (mut app, band, _) = lone_band();
    set_leader(&mut app, band, true);
    stage_order(&mut app, band, OrderOwner::Player, DEEP_LARDER);
    turn(&mut app);
    assert_eq!(cohort(&app, band).last_leadership.met_orders, 1);
    assert_close(
        term(&app, band),
        wellbeing(&app).leadership.popularity_per_met_order,
        "a player-owned met order",
    );
}

/// **The contributions still sum to the morale delta**, the leadership term among them.
#[test]
fn the_contributions_sum_to_the_morale_delta() {
    let (app, band) = falling_band(0.0);
    let c = cohort(&app, band);
    assert!(c.last_morale_contributions.leadership < Scalar::from_i64(0));
    assert_eq!(c.last_morale_contributions.total(), c.last_morale_delta);
}

/// **`moraleLeadership` and `moraleCause` are on the encoded frame**, equal to what the turn priced.
#[test]
fn the_term_and_its_cause_are_on_the_encoded_snapshot() {
    let (mut app, band, _) = lone_band();
    tune(&mut app, |config| {
        config.leadership.starvation_strain = OVERWHELMING_STRAIN;
    });
    set_leader(&mut app, band, true);
    clear_orders(&mut app, band);
    let meal = demand(&app, band);
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .stores
        .reset_food("dry", scalar_from_f32(meal * (1.0 - HALF_MEAL)));
    turn(&mut app);
    let priced = cohort(&app, band).last_morale_contributions.leadership;
    assert!(priced < Scalar::from_i64(0), "fixture: a live term");

    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published")
        .iter()
        .find(|row| row.entity() == band.to_bits())
        .expect("the band is on the wire");
    assert_eq!(row.moraleLeadership(), priced.raw());
    assert_eq!(row.moraleCause(), LEADERSHIP_CAUSE_WIRE);
}
