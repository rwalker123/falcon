//! **Living near your dead teaches ritual, slowly** (`.claude/rules/core_sim/belief.md` -> "Ritual is
//! learned by gatherings, and by living near the dead"): each turn a people is credited
//! `ritual_lesson_per_turn_at_full_tie` x the strongest ancestor tie (`s x r`) among its resident
//! bands, through the ladder's one practice-to-ledger seam. Driven through the real
//! `advance_ritual_trickle` on a generated world, the ties staged on the cohort fields the culture
//! code reads (`belief_anchor`, `last_belief_relay_hops`).

mod faction_support;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use core_sim::belief_relay::DIRECT_HOPS;
use core_sim::{
    advance_ritual_trickle, BandId, BeliefRegistry, DiscoveryProgressLedger, FactionId,
    LadderConfigHandle, PopulationCohort, ResidentBand, WellbeingConfigHandle, RITUAL_DISCOVERY_ID,
};
use faction_support::{one_faction_world, HOME};

/// A cemetery of ten dead: the anchor's weight is exactly half (`belief_half_saturation` 10).
const TEN_DEAD: f32 = 10.0;
/// A thin cemetery, tied far more weakly than [`TEN_DEAD`].
const ONE_DEAD: f32 = 1.0;
/// Where the staged anchors sit (the ties are staged by hop count, so the tile is only a key).
const GRAVE: UVec2 = UVec2::new(3, 4);
const OTHER_GRAVE: UVec2 = UVec2::new(7, 2);
/// The id of the second band the fixture spawns.
const SECOND_BAND: BandId = BandId(9_000_001);
const TOLERANCE: f32 = 1.0e-5;

fn first_band(app: &mut App) -> Entity {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .min_by_key(|(_, id, _)| **id)
        .map(|(entity, _, _)| entity)
        .expect("the home faction has a band")
}

/// The band's tie staged on the fields the culture code reads.
fn stage(app: &mut App, band: Entity, anchor: Option<UVec2>, hops: u8) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort.belief_anchor = anchor;
    cohort.last_belief_relay_hops = hops;
}

/// A world with an empty belief registry and no ritual progress.
fn world() -> (App, Entity) {
    let mut app = one_faction_world();
    let band = first_band(&mut app);
    app.world.insert_resource(BeliefRegistry::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    (app, band)
}

fn turn(app: &mut App) {
    app.world.run_system_once(advance_ritual_trickle);
}

fn ritual(app: &App, faction: FactionId) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(faction, RITUAL_DISCOVERY_ID)
        .to_f32()
}

/// The ledger credit one turn at `tie` pays, derived from the config.
fn credit_at(app: &App, tie: f32) -> f32 {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let practice = ladder.knowledge.ritual_lesson_per_turn_at_full_tie * tie;
    ladder
        .knowledge
        .ledger_credit("ritual", practice)
        .expect("ritual is priced")
}

fn tie_of(app: &App, belief: f32) -> f32 {
    app.world
        .resource::<WellbeingConfigHandle>()
        .get()
        .culture
        .anchor_weight(belief)
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= TOLERANCE,
        "{what}: got {actual}, expected {expected}"
    );
}

/// **STANDING ON TEN DEAD PAYS HALF THE FULL-TIE RATE, THROUGH THE LEDGER SEAM.**
#[test]
fn a_band_on_its_dead_is_credited_in_proportion_to_its_tie() {
    let (mut app, band) = world();
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(GRAVE, TEN_DEAD);
    stage(&mut app, band, Some(GRAVE), DIRECT_HOPS as u8);
    turn(&mut app);
    let tie = tie_of(&app, TEN_DEAD);
    assert_close(tie, 0.5, "ten dead is half-saturated");
    assert!(credit_at(&app, tie) > 0.0, "the shipped lever is live");
    assert_close(
        ritual(&app, HOME),
        credit_at(&app, tie),
        "one turn's credit",
    );
}

/// **WALKED OUT OF REACH WITH NO KIN, NOTHING.**
#[test]
fn a_band_unreached_from_its_dead_learns_nothing() {
    let (mut app, band) = world();
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(GRAVE, TEN_DEAD);
    stage(
        &mut app,
        band,
        Some(GRAVE),
        sim_schema::BELIEF_RELAY_UNREACHED,
    );
    turn(&mut app);
    assert_eq!(ritual(&app, HOME), 0.0);
}

/// **NO ANCHOR, NOTHING.**
#[test]
fn a_band_with_no_anchor_learns_nothing() {
    let (mut app, band) = world();
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(GRAVE, TEN_DEAD);
    stage(&mut app, band, None, DIRECT_HOPS as u8);
    turn(&mut app);
    assert_eq!(ritual(&app, HOME), 0.0);
}

/// **A PEOPLE IS CREDITED ITS STRONGEST BAND'S TIE, NEVER THE SUM.**
#[test]
fn two_bands_of_one_people_pay_the_stronger_tie_alone() {
    let (mut app, band) = world();
    {
        let mut belief = app.world.resource_mut::<BeliefRegistry>();
        belief.add(GRAVE, TEN_DEAD);
        belief.add(OTHER_GRAVE, ONE_DEAD);
    }
    stage(&mut app, band, Some(GRAVE), DIRECT_HOPS as u8);
    let mut second = app.world.get::<PopulationCohort>(band).unwrap().clone();
    second.belief_anchor = Some(OTHER_GRAVE);
    second.last_belief_relay_hops = DIRECT_HOPS as u8;
    app.world.spawn((second, SECOND_BAND, ResidentBand));
    turn(&mut app);
    let strong = tie_of(&app, TEN_DEAD);
    let weak = tie_of(&app, ONE_DEAD);
    assert!(weak > 0.0 && weak < strong);
    assert_close(
        ritual(&app, HOME),
        credit_at(&app, strong),
        "the strong band alone",
    );
}

/// **LEVER 0 SWITCHES THE TRICKLE OFF.**
#[test]
fn a_zero_lever_pays_nothing() {
    let (mut app, band) = world();
    let mut ladder = (*app.world.resource::<LadderConfigHandle>().get()).clone();
    ladder.knowledge.ritual_lesson_per_turn_at_full_tie = 0.0;
    app.world
        .insert_resource(LadderConfigHandle(std::sync::Arc::new(ladder)));
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(GRAVE, TEN_DEAD);
    stage(&mut app, band, Some(GRAVE), DIRECT_HOPS as u8);
    turn(&mut app);
    assert_eq!(ritual(&app, HOME), 0.0);
}
