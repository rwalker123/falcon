//! **The band's food ledger must reconcile with its larder.**
//!
//! A pen's feed is taken straight off `cohort.stores` (`LocalStore::take`, the corral-tend branch of
//! `advance_labor_allocation`), so it appears in **neither** `foodIncome` (Σ per-source `actual`) nor
//! `foodConsumption` (the *people's* `food_demand`). Without exporting it, the client's net-food row
//! (`foodIncome − foodConsumption`) overstates the surplus by exactly the upkeep and the player watches
//! the larder drain with no explanation.
//!
//! `PopulationCohortState.penFeedUpkeep` closes it, and this is the identity that gives the field its
//! meaning — asserted against a **real turn** through the real systems and the real snapshot export,
//! not a re-derivation:
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − penFeedUpkeep
//! ```
//!
//! Pinned both when the band can pay the feed in full and when it can only **partially** pay (the
//! field is the *actual* debit, not the demanded amount).

use bevy::prelude::Entity;
use core_sim::{
    build_headless_app, run_turn, scalar_from_f32, FactionId, FollowPolicy, GrazeRegistry,
    HerdRegistry, LaborAllocation, LaborAssignment, LaborTarget, PopulationCohort,
    SimulationConfig, SnapshotHistory, Tile, FOOD, RUNG_COMPLETE,
};

/// The shipped default `map_seed` is `0` ("seed from entropy"), so a test must pin its own or every
/// run lands on a different map.
const SEED: u64 = 119_304_647;
/// Enough food that the pen's feed (and the people's demand) are both paid in full.
const AMPLE_LARDER: f32 = 500.0;
/// **The people eat first, off the same larder** (`simulate_population` runs before the corral-tend
/// branch of `advance_labor_allocation`). So a thin larder that only part-pays the *pen* must still
/// cover the band's own ~4/turn food demand in full — otherwise the humans drain it dry and the pen
/// is paid **zero**, which tests starvation, not a partial pen payment. This value feeds the people
/// fully and leaves a remainder that is a genuine fraction of a Red-Deer-sized pen's ~10.8/turn feed.
const THIN_LARDER: f32 = 8.0;
/// The exported floats are `f32` sums of `Scalar`-quantized takes; a few ULPs of slack, no more.
const EPSILON: f32 = 0.01;

/// Stand a band up with a **penned herd it keeps**, seed its larder, run one real turn, and return
/// `(larder_before, larder_after, food_income, food_consumption, pen_feed_upkeep, pen_fed_fraction)`.
/// `pen_fed_fraction` (paid ÷ demanded, read off the live herd) is the partial-payment witness: `1.0`
/// = fully fed, `< 1.0` = the pen only part-paid and the herd starves for the rest.
fn run_one_turn_with_a_pen(larder: f32) -> (f32, f32, f32, f32, f32, f32) {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();

    let (band, band_tile_entity, workers) = {
        let mut q = app.world.query::<(Entity, &PopulationCohort)>();
        let (e, c) = q.iter(&app.world).next().expect("a starting band");
        (e, c.current_tile, c.working.to_f32().floor() as u32)
    };
    let band_pos = app
        .world
        .get::<Tile>(band_tile_entity)
        .expect("band tile")
        .position;

    // Pen the biggest **pennable** herd: domesticate it for the band's faction and corral it on the
    // band's own tile, so the band's Hunt assignment TENDS it (and pays its feed) rather than hunting
    // it.
    //
    // **`can_pen()` is load-bearing, not defensive.** Picking the biggest herd outright picks a
    // **mammoth** (8000–12000 biomass dwarfs every other species; the best `pen`-ceiling animal, the
    // aurochs, tops out at 1300) — and a mammoth is `husbandry_ceiling: wild`, so
    // `accrue_domestication` early-returns and it can be neither tamed nor owned. This fixture used to
    // stand on a **wild, unowned, penned mammoth**: a state the real sim cannot produce, which only
    // existed because `corral_at` had no ceiling guard (it does now, and would refuse). Filtering to
    // `can_pen()` first is what makes the ledger identity below an assertion about the *game*.
    let herd_id = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .filter(|herd| herd.can_pen())
            // Biggest first — the largest feed demand, so the identity has the most to reconcile.
            // Tie-broken by position then id (the `graze::richest_patch` precedent): `herds` is an
            // ordered `Vec` and `SEED` is pinned, so this is not seed-dependent — but a `max_by` that
            // leans on iteration order is one roster edit away from becoming so.
            .max_by(|a, b| {
                a.biomass
                    .total_cmp(&b.biomass)
                    .then_with(|| b.position().y.cmp(&a.position().y))
                    .then_with(|| b.position().x.cmp(&a.position().x))
                    .then_with(|| b.id.cmp(&a.id))
            })
            .expect("the map must spawn at least one pennable herd");
        herd.accrue_domestication(FactionId(0), RUNG_COMPLETE);
        assert!(
            herd.is_domesticated(),
            "{} must actually tame — a pen is built on a herd you own",
            herd.species
        );
        herd.biomass = herd.carrying_capacity; // at capacity → the largest possible feed demand
                                               // Pen it ON the band's tile: in reach, and it no longer roams.
        assert!(
            herd.corral_at(band_pos),
            "{} must actually pen — the ledger identity is about a REAL pen",
            herd.species
        );
        herd.id.clone()
    };

    // **Pin the pen to a BARREN footprint (Grazing 2d).** A penned herd now grazes its fenced footprint
    // and the larder pays only what the pasture cannot cover (§2.3) — so on real pasture `penUpkeep → 0`
    // and there is no ongoing feed to reconcile. This test is about the LEDGER identity when the pen
    // DOES draw the larder, so we preserve the pre-2d worst case: strip the graze patch under the pen
    // tile (`pen_radius = 0` → the footprint is exactly `band_pos`). A wholly-barren footprint keeps the
    // herd's frozen K and is fully larder-fed (§2.3's preserved worst case), so the feed is the full
    // `upkeep × biomass` and the identity has a real, positive `penFeedUpkeep` to reconcile against.
    app.world
        .resource_mut::<GrazeRegistry>()
        .patches
        .remove(&band_pos);

    // The band's ONLY assignment: keep the pen. So every food flow this turn is one of the three the
    // identity names — the pen's harvest (income), the people's demand (consumption), the pen's feed.
    app.world.entity_mut(band).insert(LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Hunt {
                fauna_id: herd_id.clone(),
                policy: FollowPolicy::Sustain,
            },
            workers: workers.max(1),
        }],
        ..Default::default()
    });
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("band")
        .stores
        .set(FOOD, scalar_from_f32(larder));

    let before = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .get(FOOD)
        .to_f32();

    run_turn(&mut app);

    let after = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .get(FOOD)
        .to_f32();

    // Read the numbers the CLIENT reads — the exported snapshot, not the sim's internals.
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .clone()
        .expect("a snapshot was captured");
    let cohort = snapshot
        .populations
        .iter()
        .find(|c| !c.is_expedition)
        .expect("the resident band is exported");

    // The pen's fed fraction lives on the live herd (transient, set by the tend branch this turn).
    let pen_fed_fraction = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|h| h.id == herd_id)
        .expect("the penned herd is still alive")
        .pen_fed_fraction;

    (
        before,
        after,
        cohort.food_income,
        cohort.food_consumption,
        cohort.pen_feed_upkeep,
        pen_fed_fraction,
    )
}

/// **The identity, fully fed.** The pen's feed is a real debit that shows up in the exported ledger,
/// and the three exported terms reconcile with the larder exactly.
#[test]
fn the_food_ledger_reconciles_with_the_larder_when_the_pen_is_fully_fed() {
    let (before, after, income, consumption, pen_feed, pen_fed_fraction) =
        run_one_turn_with_a_pen(AMPLE_LARDER);

    assert!(
        pen_feed > 0.0,
        "a band keeping a pen must report a real feed debit (got {pen_feed})"
    );
    assert!(
        (pen_fed_fraction - 1.0).abs() < EPSILON,
        "an ample larder pays the pen in full (fed fraction {pen_fed_fraction})"
    );
    assert!(income > 0.0, "the pen pays its keeper (got {income})");
    assert!(consumption > 0.0, "the people eat (got {consumption})");

    let delta = after - before;
    let ledger = income - consumption - pen_feed;
    assert!(
        (delta - ledger).abs() < EPSILON,
        "larder_delta must equal foodIncome − foodConsumption − penFeedUpkeep: \
         delta={delta} vs ledger={ledger} (income={income} consumption={consumption} feed={pen_feed})"
    );

    // The bug this field exists to kill: the naive net-food row the client used to draw overstates the
    // surplus by exactly the upkeep.
    let naive_net = income - consumption;
    assert!(
        (naive_net - delta - pen_feed).abs() < EPSILON,
        "the pre-fix readout (income − consumption) overstates the true change by exactly the feed"
    );
}

/// **The identity when the band can only PART-pay.** `penFeedUpkeep` is the food actually handed over
/// (`LocalStore::take`'s return), never the amount demanded — so the ledger still reconciles, and the
/// herd starves for the difference (its own `penFedFraction` carries that).
#[test]
fn the_food_ledger_reconciles_when_the_pen_is_only_partly_fed() {
    let (before, after, income, consumption, pen_feed, pen_fed_fraction) =
        run_one_turn_with_a_pen(THIN_LARDER);

    // The people ate first (in full — `THIN_LARDER` covers their demand), so the pen was paid only
    // the larder's *remainder*: a real, positive, but **partial** debit. `pen_fed_fraction < 1` is the
    // proof it is genuinely partial (the herd starves for the shortfall); `> 0` that it paid at all.
    assert!(
        consumption > 0.0 && consumption < THIN_LARDER,
        "the band eats its fill from the thin larder first: ate {consumption} of {THIN_LARDER}"
    );
    assert!(
        pen_feed > 0.0,
        "the larder's remainder still part-pays the pen: paid {pen_feed}"
    );
    assert!(
        pen_fed_fraction > 0.0 && pen_fed_fraction < 1.0,
        "a PARTIAL payment — the pen got some feed but not all it demanded (fed fraction \
         {pen_fed_fraction})"
    );

    let delta = after - before;
    let ledger = income - consumption - pen_feed;
    assert!(
        (delta - ledger).abs() < EPSILON,
        "the identity must hold on a PARTIAL payment too (penFeedUpkeep is the real debit, not the \
         demand): delta={delta} vs ledger={ledger} \
         (income={income} consumption={consumption} feed={pen_feed})"
    );
}
