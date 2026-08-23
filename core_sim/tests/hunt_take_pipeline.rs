//! **THE FOUR STEPS OF A HUNT TAKE, IN ORDER** — engage, retreat, fight, carry.
//!
//! ```text
//! 1. engage   reach (`workers × engage_rate`), bounded by what the herd can spare above the floor
//! 2. retreat  a fraction of what was reached gets away (`wariness`)
//! 3. fight    whole animals dead, the remainder banked on the quarry's wound ledger
//! 4. carry    min(pack, killed) in BIOMASS, unrounded — the rest is left on the ground
//! ```
//!
//! **An animal dies whole; meat divides.** The whole-animal quantum belongs on the *kill*, never on
//! the haul: hunters field-dress and take what they can carry, and what they cannot carry is waste
//! rather than an animal they declined to kill.
//!
//! Each test here pins one step against the defect it replaced. What is **not** here: the fight's own
//! arithmetic (`hunt_fight.rs`), the retreat's distribution (`hunt_wariness.rs`), and the crew curve's
//! two transports (`hunt_useful_crew_on_the_wire.rs`), which owns the published-row half of step 1.

use core_sim::{
    animals_engaged, hunt_take, hunt_take_bound, quantise_animal_take, EngagementStop, FaunaConfig,
    Herd, HuntDraw, HuntTakeBound, HuntingParty, SizeClass,
};

/// **The shipped EQUIPPED haul rate** — what a kitted band drags, off the sled's own tier.
/// `labor_config`'s `hunt.per_worker_biomass_capacity` is the *bare-handed* baseline since quality
/// tiers landed, so a fixture that wants "an ordinary band" asks the item table.
fn equipped_haul_rate() -> f32 {
    core_sim::EquipmentConfig::builtin().equipped_reference(
        core_sim::EquipmentStat::HuntCarry,
        core_sim::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity,
    )
}

/// The shipped quarry the **engagement** binds on, and the one the defect was reported from:
/// `engage_rate 0.33`, `wariness 0.25`, `body_mass 12`, `defense 2`, `durability 20`.
const BOAR: &str = "Wild Boar";

/// Standing stock far above anything a party can take, so the escapement room never binds and the
/// take is decided by the reach and the fight.
const FAT_HERD: f32 = 200_000.0;
/// Take everything the herd can spare — the floor is deliberately not a term in the sweep fixtures.
const STRIP_IT_BARE: f32 = 0.0;
/// A resident band eats/banks its whole take, so carry never binds either.
const NO_CARRY_LIMIT: f32 = f32::INFINITY;

/// Long enough that a lone hunter's sub-body reach completes many whole animals, so the average is
/// the *rate* rather than the accident of where the window ended.
const SUSTAINED_TURNS: u32 = 200;

/// A herd of `species` at a stated standing stock.
fn herd_of(fauna: &FaunaConfig, species: &str, stock: f32) -> Herd {
    let def = fauna
        .species_by_display(species)
        .expect("the fixture names a shipped species");
    Herd::new(
        "take_pipeline_probe".to_string(),
        species.to_string(),
        SizeClass::Big,
        vec![bevy::math::UVec2::new(1, 1)],
        stock,
        stock,
        def.fodder_per_biomass,
        def.regrowth_rate.unwrap_or(0.1),
        def.body_mass,
    )
}

/// **What a crew of `workers` actually brings home over [`SUSTAINED_TURNS`]** — `(carried biomass,
/// animals killed)`, run through `systems::hunt_take` itself so this is the take the sim pays and
/// not a re-derivation of it. The herd's wound ledger carries between turns exactly as it does live.
fn sustained_take(species: &str, workers: u32) -> (f32, u32) {
    let fauna = std::sync::Arc::new(FaunaConfig::builtin());
    let party = HuntingParty::builtin_equipped();
    let mut herd = herd_of(&fauna, species, FAT_HERD);
    let mut carried = 0.0;
    let mut killed = 0;
    for tick in 0..u64::from(SUSTAINED_TURNS) {
        let outcome = hunt_take(
            &mut herd,
            workers,
            STRIP_IT_BARE,
            equipped_haul_rate(),
            &party,
            &fauna,
            NO_CARRY_LIMIT,
            // A **live** draw, seeded per turn as the sim seeds it — not the forecast's quantile.
            HuntDraw::Seeded(tick),
        );
        carried += outcome.take.carried;
        killed += outcome.take.killed;
    }
    (carried, killed)
}

/// **THE ACCEPTANCE TEST: TWO HUNTERS TAKE MORE THAN ONE, THREE MORE THAN TWO, AND SO ON.**
///
/// The reach was `floor(workers × engage_rate).max(1)`, which on the shipped Wild Boar's `0.33`
/// answered **one animal for every crew from 1 to 6**. Reported from play: four hunters brought home
/// exactly what one did, `0.18 food/turn` either way. There is no flat region anywhere in the run
/// now, and it is asserted on the **take** rather than on the reach — a cap that rises while the
/// food does not is the same defect wearing a different hat.
#[test]
fn every_extra_hunter_brings_home_strictly_more() {
    let mut previous = 0.0;
    let mut takes = Vec::new();
    for workers in 1..=8u32 {
        let (carried, _) = sustained_take(BOAR, workers);
        assert!(
            carried > previous,
            "a crew of {workers} must bring home strictly more than {} did ({carried} vs \
             {previous}); the run so far is {takes:?}",
            workers - 1
        );
        previous = carried;
        takes.push(carried);
    }
    // Liveness: the run is a real take, not eight zeroes ordered by accident.
    assert!(
        takes[0] > 0.0,
        "the lone hunter must feed the band at all, or 'strictly more' compares nothing"
    );
}

/// **A LONE HUNTER EVENTUALLY KILLS — at exactly the rate its reach implies.**
///
/// One hunter reaches `0.33` of a boar a turn and keeps `1 − wariness` of that, so a `floor()` at
/// either the retreat or the fight would leave it **zero, for ever** — strictly worse than the
/// retired `max(1)` and the one regression the un-floored reach could have shipped. What carries the
/// part body between turns is the fight's own accumulator (`combat::DamageLedger` on `Herd::wounds`),
/// and this asserts the *cadence* it produces rather than merely that the total is non-zero: a bank
/// that leaked would still clear one animal occasionally and pass a `> 0` check.
#[test]
fn a_lone_hunter_kills_at_its_reach_rate_over_many_turns() {
    let fauna = FaunaConfig::builtin();
    let rate = fauna.engage_rate_for(BOAR);
    let stay = core_sim::stay_fraction(fauna.wariness_for(BOAR), NEUTRAL_DISPERSION);
    assert!(
        rate < 1.0,
        "the fixture must have a sub-body reach ({rate}) or this test is not about the part body"
    );

    let (_, killed) = sustained_take(BOAR, 1);
    let expected = SUSTAINED_TURNS as f32 * rate * stay;
    assert!(
        (killed as f32 - expected).abs() <= ONE_WHOLE_ANIMAL,
        "a lone hunter must take `turns × engage_rate × stay_fraction` boar within one animal — \
         killed {killed} over {SUSTAINED_TURNS} turns against {expected}"
    );
}

/// The kit's noise at the shipped spear tier — the neutral value, so `stay_fraction` here is the
/// species' own.
const NEUTRAL_DISPERSION: f32 = 1.0;
/// At most one body may still be standing wounded when the window closes, so a sustained total is
/// allowed to run that much under its rate and no further.
const ONE_WHOLE_ANIMAL: f32 = 1.0;

/// **STEP 4: THE PACK'S LAST PART-LOAD IS USED, NOT WASTED BY ROUNDING.**
///
/// The carry bound was `floor(collection ÷ body_mass)`, so a party able to carry **1.5** animals
/// killed one, carried one, and left half its capacity idle every turn of the game. It now kills the
/// animal at the top of its load, carries what fits and leaves the remainder on the ground — which
/// is not a new rule but the general form of the one that has always let a party unable to seat even
/// one still take one.
#[test]
fn a_pack_that_seats_one_and_a_half_animals_carries_one_and_a_half() {
    const BODY_MASS: f32 = 12.0;
    /// A pack that holds exactly one and a half bodies — the case the floor threw away.
    const PACK: f32 = BODY_MASS * 1.5;
    /// Room for far more animals than the pack, so the herd is never the binding term.
    const AMPLE_CEILING: f32 = BODY_MASS * 100.0;
    /// What the fight put on the ground.
    const BROUGHT_DOWN: f32 = 2.0;

    let take = quantise_animal_take(
        AMPLE_CEILING,
        PACK,
        BODY_MASS,
        BROUGHT_DOWN,
        EngagementStop::WhenPackFull,
    );
    assert_eq!(
        take.killed, 2,
        "the animal the pack cannot seat whole is still killed whole"
    );
    assert_eq!(
        take.carried, PACK,
        "…and the WHOLE pack comes home — the haul is unrounded biomass, not whole animals"
    );
    assert_eq!(
        take.wasted,
        BODY_MASS * 0.5,
        "…with the half body it could not hold left on the ground"
    );
    // The waste is real loss the herd pays for: killed is carried plus wasted, by construction.
    assert_eq!(take.killed_biomass(), BODY_MASS * BROUGHT_DOWN);
    // And the report names the arm that actually bound it, off the same helper the take used.
    assert_eq!(
        hunt_take_bound(
            AMPLE_CEILING,
            AMPLE_CEILING,
            PACK,
            BODY_MASS,
            BROUGHT_DOWN,
            BROUGHT_DOWN,
            EngagementStop::WhenPackFull,
        ),
        HuntTakeBound::Carry,
        "the pack is what stopped this take, and the row must say so"
    );
}

/// **STEP 1: THE ESCAPEMENT FLOOR STOPS THE ENGAGEMENT, NOT THE KILL.**
///
/// A herd standing on the floor its hunters named hands over nothing — and the party never goes after
/// it, so restraint costs no casualties and no wear. That the *engagement* is where the room binds is
/// the assertion: a take that engaged normally and then declined to kill what it had already fought
/// would satisfy "yields nothing" while being denial rather than restraint.
#[test]
fn the_floor_stops_the_engagement_so_a_herd_at_it_yields_nothing() {
    /// Leave the whole herd standing — the floor sits exactly on the stock.
    const LEAVE_IT_ALL: f32 = 1.0;
    let fauna = std::sync::Arc::new(FaunaConfig::builtin());
    let party = HuntingParty::builtin_equipped();
    let mut herd = herd_of(&fauna, BOAR, FAT_HERD);
    // Liveness: this same crew takes a great deal from the same herd at a floor that leaves room.
    let mut spare = herd.clone();
    let taken = hunt_take(
        &mut spare,
        8,
        STRIP_IT_BARE,
        equipped_haul_rate(),
        &party,
        &fauna,
        NO_CARRY_LIMIT,
        HuntDraw::Seeded(0),
    );
    assert!(
        taken.take.killed > 0 && taken.engaged > 0.0,
        "the fixture crew must be able to hunt this herd at all"
    );

    let outcome = hunt_take(
        &mut herd,
        8,
        LEAVE_IT_ALL,
        equipped_haul_rate(),
        &party,
        &fauna,
        NO_CARRY_LIMIT,
        HuntDraw::Seeded(0),
    );
    assert_eq!(outcome.take.killed, 0, "a herd at its floor pays nothing");
    assert_eq!(
        outcome.engaged, 0.0,
        "…and the party never cornered one — the room bounds the ENGAGEMENT, so restraint is free"
    );
    assert_eq!(
        outcome.fight.casualties.killed + outcome.fight.casualties.wounded,
        0.0,
        "…which is exactly why it costs nobody: there was no fight to be hurt in"
    );
    assert_eq!(outcome.bound, HuntTakeBound::Floor);
    assert_eq!(
        herd.biomass, FAT_HERD,
        "the herd is untouched, to the biomass"
    );
}

/// **The reach a party of nobody has is nothing** — the one arm of `animals_engaged` that is not a
/// plain multiply, pinned beside the pipeline it opens because an unstaffed row must not manufacture
/// a hunter.
#[test]
fn an_unstaffed_row_engages_nothing() {
    let fauna = FaunaConfig::builtin();
    assert_eq!(animals_engaged(0, fauna.engage_rate_for(BOAR)), 0.0);
}
