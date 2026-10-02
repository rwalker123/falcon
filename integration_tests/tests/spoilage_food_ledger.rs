//! **Food that rots must reconcile with the band's larder** (#706).
//!
//! Only food the band cannot eat in time rots: a larder above its keeping line loses the excess
//! right after the meal, and a caravan pack whose walk is longer than its class keeps loses that
//! class on the way. Both land on one exported term, `PopulationCohortState.foodSpoiled`, and the
//! larder ledger identity carries it:
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − raidForfeit − foodSpoiled
//!                 + transferReceived − transferSent
//! ```
//!
//! Pinned against a **real turn** through the real systems and the real snapshot export, with a
//! larder of flesh far above the band's flesh line — so `foodSpoiled` is genuinely non-zero
//! (liveness: an identity that only ever held at zero rot would not prove the term is wired), and
//! the rot is exactly the excess over the line.

use bevy::prelude::Entity;
use core_sim::{
    build_test_app, run_turn, scalar_from_f32, DemographicsConfigHandle, PopulationCohort,
    SimulationConfig, SnapshotHistory, FOOD,
};

/// The shipped default `map_seed` is `0` ("seed from entropy"), so a test must pin its own.
const SEED: u64 = 119_304_647;
/// The keeping class seeded — the fastest-rotting one the shipped table carries.
const FLESH: &str = "flesh";
/// Turns of the band's need the seeded flesh larder holds — far past the flesh line, so most of it
/// cannot be eaten before it rots.
const LARDER_IN_TURNS_OF_NEED: f32 = 40.0;
/// The exported floats are `f32` sums of `Scalar`-quantized amounts; a few ULPs of slack, no more.
const EPSILON: f32 = 0.01;

#[test]
fn the_food_ledger_reconciles_when_a_larder_above_its_line_rots() {
    let mut app = build_test_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();

    let band = {
        let mut q = app.world.query::<(Entity, &PopulationCohort)>();
        q.iter(&app.world).next().expect("a starting band").0
    };
    // The need this turn's meal will be measured against is not known until the meal; seed far
    // enough above any plausible need that the flesh line is crossed whatever it is.
    let flesh_life = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .keeping
        .shelf_life(FLESH)
        .expect("flesh is a shipped keeping class");
    let seeded = {
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).expect("band");
        let generous_need = cohort.total().to_f32();
        let larder = generous_need * LARDER_IN_TURNS_OF_NEED;
        cohort.stores.reset_food(FLESH, scalar_from_f32(larder));
        larder
    };
    let before = larder_of(&app, band);

    run_turn(&mut app);

    let after = larder_of(&app, band);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .clone()
        .expect("a snapshot was captured");
    let cohort = snapshot
        .populations
        .iter()
        .find(|c| c.entity == band.to_bits())
        .expect("the resident band is exported");

    // **Liveness: the rot fired.**
    assert!(
        cohort.food_spoiled > 0.0,
        "a flesh larder of {seeded} must rot past the band's flesh line (need {}, shelf {flesh_life})",
        cohort.food_need
    );
    // **And it is exactly the excess over the line** — the band eats `consumption` first, then
    // whatever flesh remains above `need × shelf_life` rots.
    let line = cohort.food_need * flesh_life;
    let expected_rot = (seeded - cohort.food_consumption - line).max(0.0);
    assert!(
        (cohort.food_spoiled - expected_rot).abs() < EPSILON,
        "the rot is the flesh above its line: spoiled {} vs expected {expected_rot}",
        cohort.food_spoiled
    );

    // **The identity, with the spoiled term, against the real larder movement.**
    let delta = after - before;
    let ledger = cohort.food_income - cohort.food_consumption - cohort.raid_forfeit
        + cohort.transfer_received
        - cohort.transfer_sent
        - cohort.food_spoiled;
    assert!(
        (delta - ledger).abs() < EPSILON,
        "larder_delta must equal foodIncome − foodConsumption − raidForfeit − foodSpoiled + \
         received − sent: delta={delta} vs ledger={ledger} (income={} consumption={} raid={} \
         spoiled={} received={} sent={})",
        cohort.food_income,
        cohort.food_consumption,
        cohort.raid_forfeit,
        cohort.food_spoiled,
        cohort.transfer_received,
        cohort.transfer_sent,
    );
}

/// The band's FOOD store, in `f32` — the number the ledger reconciles against.
fn larder_of(app: &bevy::app::App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .get(FOOD)
        .to_f32()
}
