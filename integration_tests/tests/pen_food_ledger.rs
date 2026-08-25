//! **The band's food ledger must reconcile with its larder — and a pen is not a line in it.**
//!
//! A penned herd eats the grass its fenced footprint grows and the hay its keeper carries in. Both are
//! `FODDER`, a store that never converts to `FOOD`. **Human food is not animal feed**, so no food
//! crosses from the people's larder to the animals and the identity has three terms, not four:
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − raidForfeit
//! ```
//!
//! asserted against a **real turn** through the real systems and the real snapshot export, not a
//! re-derivation. There is no raid in this fixture, so it reduces to `income − consumption` and any
//! third flow at all would break it — which is what makes this the strongest available statement that
//! the pens took nothing.
//!
//! # ⛔ WHAT THIS FILE USED TO ASSERT, AND WHY IT WAS WRONG
//!
//! It pinned `larder_delta == foodIncome − foodConsumption − penFeedUpkeep`: the pen's feed came
//! straight off `cohort.stores`, appeared in neither income nor consumption, and had to be exported
//! as its own negative row or the client's net-food line overstated the surplus. The *export* was
//! sound; the **draw** was the modelling error. Its real effect was to short-circuit starvation — a
//! pen whose pasture failed took food out of its keepers' mouths instead of shrinking. So
//! `penFeedUpkeep` stopped being written, and the fixture that had to defeat it (a keeper posed at
//! harvest floor `1.0`, taking nothing, so its own harvest could not feed the pen back) is back in
//! its natural shape.

use bevy::prelude::Entity;
use core_sim::{
    build_test_app, run_turn, scalar_from_f32, scalar_one, DiscoveryProgressLedger, FactionId,
    GrazeRegistry, HerdRegistry, LaborAllocation, LaborAssignment, LaborTarget, PopulationCohort,
    SimulationConfig, SnapshotHistory, SourcePriority, Tile, FODDER, FODDERING_DISCOVERY_ID, FOOD,
};

/// The shipped default `map_seed` is `0` ("seed from entropy"), so a test must pin its own or every
/// run lands on a different map.
const SEED: u64 = 119_304_647;
/// A larder deep enough that the band's own people are fed in full **and** that "nothing was taken
/// for the pen" is a claim with something to take. Every case here stocks it.
const AMPLE_LARDER: f32 = 500.0;
/// The exported floats are `f32` sums of `Scalar`-quantized takes; a few ULPs of slack, no more.
const EPSILON: f32 = 0.01;
/// The keeper's harvest floor: hold the herd on its most productive biomass and carry the surplus
/// home, which is what a keeper is for.
const SUSTAIN: f32 = 0.5;
/// No hay, and no Foddering to draw it with — the pen on a barren footprint has no feed at all.
const NO_HAY: f32 = 0.0;

/// Stand a band up with a **penned herd it keeps**, seed its larder, run one real turn, and return
/// `(larder_before, larder_after, food_income, food_consumption, pen_fed_fraction)`.
/// `pen_fed_fraction` (grass + hay ÷ demand, read off the live herd) is the feeding witness: `1.0` =
/// fully fed, `< 1.0` = the pen went short and the herd starves for the rest.
///
/// `hay > 0` (Flora Roster F3) grants the band **Foddering** and seeds its `FODDER` store with that
/// much hay, which is the only way a pen on a barren footprint can be fed. The ledger identity is over
/// the FOOD store alone and must read the same either way, because `FODDER` is a separate store that
/// never converts to `FOOD`.
fn run_one_turn_with_a_pen(larder: f32, hay: f32, floor: f32) -> (f32, f32, f32, f32, f32) {
    let mut app = build_test_app();
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
    // band's own tile, so the band's Hunt assignment TENDS it (and feeds it) rather than hunting it.
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
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        assert!(
            herd.is_domesticated(),
            "{} must actually tame — a pen is built on a herd you own",
            herd.species
        );
        herd.biomass = herd.carrying_capacity; // at capacity → the largest possible feed demand
                                               // Pen it ON the band's tile: in reach, and it no longer roams.
        assert!(
            herd.corral_at(band_pos, &core_sim::LadderConfig::builtin()),
            "{} must actually pen — the ledger identity is about a REAL pen",
            herd.species
        );
        herd.id.clone()
    };

    // **Pin the pen to a BARREN footprint (Grazing 2d).** A penned herd grazes its fenced footprint
    // and is fed hay for whatever the pasture cannot cover (§2.3) — so on real pasture the pen is fed
    // for free and there is no feed pressure at all. Stripping the graze patch under the pen tile
    // (`pen_radius = 0` → the footprint is exactly `band_pos`) puts the pen in its **hungriest**
    // state, which is the state the retired larder draw used to bill for: it is the case where a
    // fourth ledger term, if one still existed, would be at its largest.
    app.world
        .resource_mut::<GrazeRegistry>()
        .patches
        .remove(&band_pos);

    // The band's ONLY assignment: keep the pen. So every food flow this turn is one of the two the
    // identity names — the pen's harvest (income) and the people's demand (consumption).
    app.world.entity_mut(band).insert(LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Hunt {
                fauna_id: herd_id.clone(),
                floor,
            },
            workers: workers.max(1),
            kit: None,
            priority: SourcePriority::default(),
        }],
        ..Default::default()
    });
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("band")
        .stores
        .set(FOOD, scalar_from_f32(larder));

    // F3: a hayed pen. Grant Foddering and seed the FODDER store — hay is the pen's only feed here.
    if hay > 0.0 {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(FactionId(0), FODDERING_DISCOVERY_ID, scalar_one());
        app.world
            .get_mut::<PopulationCohort>(band)
            .expect("band")
            .stores
            .set(FODDER, scalar_from_f32(hay));
    }

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
        .last_snapshot()
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
        pen_fed_fraction,
    )
}

/// **The identity when the pen is at its HUNGRIEST.** A barren footprint, no hay: the pen's whole feed
/// demand goes unmet, which is precisely the turn the retired larder draw would have billed the band
/// its full `upkeep_per_biomass × biomass`. The two exported terms still reconcile with the larder
/// exactly, so **no third flow touched the FOOD store**.
#[test]
fn a_hungry_pen_takes_nothing_from_the_larder_and_the_ledger_reconciles() {
    let (before, after, income, consumption, pen_fed_fraction) =
        run_one_turn_with_a_pen(AMPLE_LARDER, NO_HAY, SUSTAIN);

    // **Not vacuous**: the pen genuinely went hungry, so there was a real feed shortfall to (wrongly)
    // pay for. A fully-fed pen would make the reconciliation below prove nothing.
    assert!(
        pen_fed_fraction < 1.0 - EPSILON,
        "a barren, hayless pen must read UNDERFED — that is the state a larder fallback would have \
         paid for (fed fraction {pen_fed_fraction})"
    );
    assert!(income > 0.0, "the pen pays its keeper (got {income})");
    assert!(consumption > 0.0, "the people eat (got {consumption})");
    assert!(
        before >= AMPLE_LARDER - EPSILON,
        "the larder really was stocked, or 'nothing was taken' is trivially true (held {before})"
    );

    let delta = after - before;
    let ledger = income - consumption;
    assert!(
        (delta - ledger).abs() < EPSILON,
        "larder_delta must equal foodIncome − foodConsumption with NO pen term: \
         delta={delta} vs ledger={ledger} (income={income} consumption={consumption})"
    );
}

/// **HAY FEEDS THE PEN, AND THE LARDER STILL DOES NOT.** The same barren pen with hay in the `FODDER`
/// store reads **fully fed** — so the feed really is being paid, just out of the right store — while
/// the FOOD-side identity reads exactly as it did when the pen went hungry.
///
/// This is the ledger half of *"FODDER never converts to FOOD"*: feeding the animals moves a
/// different store, and the food line is indifferent to whether they ate at all.
#[test]
fn hay_feeds_the_pen_while_the_food_ledger_reads_the_same() {
    const AMPLE_HAY: f32 = 10_000.0;

    let (hay_before, hay_after, hay_income, hay_consumption, hay_fed) =
        run_one_turn_with_a_pen(AMPLE_LARDER, AMPLE_HAY, SUSTAIN);
    // The same pen with NO hay, for contrast: it starves.
    let (_, _, _, _, hungry_fed) = run_one_turn_with_a_pen(AMPLE_LARDER, NO_HAY, SUSTAIN);

    assert!(
        (hay_fed - 1.0).abs() < EPSILON,
        "hay feeds the pen in full (fed fraction {hay_fed})"
    );
    assert!(
        hungry_fed < hay_fed - EPSILON,
        "and without it the same pen goes short — so the hay is what fed it, not something else \
         (hayed {hay_fed} vs hayless {hungry_fed})"
    );

    // The identity still holds — hay is off-ledger (a separate store), FODDER never became FOOD.
    let delta = hay_after - hay_before;
    let ledger = hay_income - hay_consumption;
    assert!(
        (delta - ledger).abs() < EPSILON,
        "the identity must hold for a HAY-fed pen too — FODDER is a separate store, never converted \
         to FOOD: delta={delta} vs ledger={ledger} \
         (income={hay_income} consumption={hay_consumption})"
    );
}

/// **FEEDING THE ANIMALS COSTS THE PEOPLE NOTHING AT ALL** — the two runs above move the FOOD store
/// by the *same* amount, though one pen ate its fill and the other starved.
///
/// The retired `penFeedUpkeep` is exactly the gap this asserts is zero: under it, the hungry pen's
/// band paid its whole bill in bread and the hayed pen's paid almost none, so the two larders moved
/// by visibly different amounts. That difference **was** the defect.
#[test]
fn a_fed_pen_and_a_starving_pen_move_their_keepers_larder_identically() {
    const AMPLE_HAY: f32 = 10_000.0;

    let (fed_before, fed_after, _, _, fed_fraction) =
        run_one_turn_with_a_pen(AMPLE_LARDER, AMPLE_HAY, SUSTAIN);
    let (hungry_before, hungry_after, _, _, hungry_fraction) =
        run_one_turn_with_a_pen(AMPLE_LARDER, NO_HAY, SUSTAIN);

    assert!(
        (fed_fraction - 1.0).abs() < EPSILON && hungry_fraction < 1.0 - EPSILON,
        "the two runs must genuinely differ in FEEDING (fed {fed_fraction}, hungry \
         {hungry_fraction}), or they cannot show that feeding costs the larder nothing"
    );

    let fed_delta = fed_after - fed_before;
    let hungry_delta = hungry_after - hungry_before;
    assert!(
        (fed_delta - hungry_delta).abs() < EPSILON,
        "whether the pen ate or starved, the band's larder moved by the same amount: \
         fed {fed_delta} vs hungry {hungry_delta}"
    );
}
