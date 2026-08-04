//! **The take resolves through the combat system** (`docs/plan_hunt_through_combat.md` §4, slice 4).
//!
//! One event used to be resolved twice — what happened to the hunters went through
//! `combat::resolve_fight` while what happened to the animals came out of the party's *carrying
//! capacity*, and nothing reconciled them (§0.1). The kill arm is now the resolver's enemy losses, so
//! this file pins the properties that only exist because of it, each one paired with the liveness
//! assertion that stops it passing on a dead model.
//!
//! **What is NOT here:** the resolver's own arithmetic (`core_sim/src/combat/mod.rs` unit tests own
//! the gate, spillover and the binomial at the `Force` level), `forecast == actual` (the existing
//! `hunt_yield_vector` / `expedition_hunt` suites own it, and they pass unchanged through this slice),
//! and the escapement floor's monotonicity (`forage::stance_probe`).

use core_sim::{
    animals_engaged, herd_capacity, hunt_take, quantise_animal_take, resolve_hunt_fight,
    CombatStats, FaunaConfig, FaunaConfigHandle, Herd, HuntingParty, LaborConfig, LadderConfig,
    SizeClass,
};

/// Standing stock far above anything a party can take, so **the escapement never binds** and the take
/// is decided by the fight — which is what every test here is about.
const FAT_HERD: f32 = 200_000.0;
/// Take everything the herd can spare: the floor is deliberately not a term in any of these tests.
const STRIP_IT_BARE: f32 = 0.0;
/// No rung transition underway, so the crew carries the identity build dip.
const NO_IMPROVEMENT: Option<core_sim::Improvement> = None;
/// A resident band eats/banks its whole take — no carry limit, so carry never binds either.
const NO_CARRY_LIMIT: f32 = f32::INFINITY;
/// Wariness is `0` across the shipped roster (slice 6 authors it), so the retreat draw is an exact
/// identity and the seed below is unobservable. Held fixed so these fixtures read as the pure
/// functions they are.
const FIXED_SEED: u64 = 0;

/// The shipped megafauna: `defense 12`, `durability 500`, `ferocity 0.9`, `engage_rate 0.05`.
const MAMMOTH: &str = "Thunder Mammoths";
/// The shipped mid-game quarry: `defense 1`, `durability 25`, `ferocity 0.15`, `engage_rate 1`.
const DEER: &str = "Red Deer";
/// Defenceless and harmless: `defense 0`, `durability 2`, **no `combat` block at all** so
/// `ferocity 0` — the one-sided engagement of §4.5.
const RABBIT: &str = "Rabbit Warren";

/// A herd of `species` fat enough that only the party's own limits can bind.
fn herd_of(fauna: &FaunaConfig, species: &str) -> Herd {
    let def = fauna
        .species_by_display(species)
        .expect("the fixture names a shipped species");
    Herd::new(
        "fight_probe".to_string(),
        species.to_string(),
        SizeClass::Big,
        vec![bevy::math::UVec2::new(1, 1)],
        FAT_HERD,
        FAT_HERD,
        def.fodder_per_biomass,
        def.regrowth_rate.unwrap_or(0.1),
        def.body_mass,
    )
}

/// A party at an arbitrary weapon tier, at the shipped resolver tuning. `attack 1` is the bare hand
/// (the `person` row's intrinsic); `20` is the shipped spear; anything else is a hypothetical tier
/// used to pin how the model *responds* to the weapon.
fn party_at(attack: f32) -> HuntingParty {
    let base = HuntingParty::builtin_equipped();
    HuntingParty {
        hunter: CombatStats {
            attack,
            ..base.hunter
        },
        ..base
    }
}

/// One turn of a resident band's hunt: the animals killed, and what the fight cost the party.
fn hunt_once(species: &str, workers: u32, party: &HuntingParty) -> (u32, f32, bool) {
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let labor = LaborConfig::builtin();
    let mut herd = herd_of(&fauna, species);
    let outcome = hunt_take(
        &mut herd,
        workers,
        STRIP_IT_BARE,
        NO_IMPROVEMENT,
        labor.hunt.per_worker_biomass_capacity,
        party,
        &fauna,
        &ladder,
        NO_CARRY_LIMIT,
        FIXED_SEED,
    );
    (
        outcome.take.killed,
        outcome.fight.casualties.killed + outcome.fight.casualties.wounded,
        outcome.fight.fought,
    )
}

// ---------------------------------------------------------------------------------------------
// §0.2 — headcount cannot substitute for a weapon
// ---------------------------------------------------------------------------------------------

/// **Eight hundred bare-handed people cannot kill a mammoth.** Not slowly — not at all
/// (§0.2, the case the whole arc exists for). `attack 1` against `defense 12` is `max(0, −11) = 0`,
/// so the party does *no damage*, at any headcount, and the answer is exactly zero rather than
/// merely small.
///
/// **And it costs them people**, which is the other half: engaging a mammoth bare-handed is a way to
/// lose your band, not a slow way to feed it.
#[test]
fn no_headcount_of_bare_hands_kills_a_mammoth() {
    let bare = HuntingParty::builtin_unequipped();
    // §0.2's own number is eight hundred; the sweep runs far past it because *"exactly zero, not
    // merely small"* is the claim. A soft `p = a/(a+d)` gate gives a bare hand `1/13` of a point
    // against a mammoth, which stays under one 500-durability body at 800 hunters and clears **fifteen
    // of them** at the top of this sweep — so this is the headcount that tells the two models apart.
    for workers in [1, 20, 100, 800, 100_000] {
        let (killed, casualties, fought) = hunt_once(MAMMOTH, workers, &bare);
        assert_eq!(
            killed, 0,
            "{workers} bare-handed hunters must kill exactly zero mammoths"
        );
        assert!(
            fought,
            "a mammoth fights back — the engagement is real, the party simply cannot hurt it"
        );
        assert!(
            casualties > 0.0,
            "{workers} bare-handed hunters must still take losses (got {casualties})"
        );
    }
    // **Liveness**: the same party, same herd, same everything but the spear — and it eats. Without
    // this the zeros above would pass on a hunt path that had simply stopped working.
    let (killed, _, _) = hunt_once(MAMMOTH, 800, &HuntingParty::builtin_equipped());
    assert!(
        killed > 0,
        "the identical party WITH spears must take mammoths, or the zeros above prove nothing"
    );
}

/// **No quantity of attackers rolls through the gate over any horizon** (§4.7) — the property a
/// probabilistic gate would silently break. Eight hundred bare hands hunting the same mammoth herd
/// every turn for a long run take nothing on every single one of them.
#[test]
fn a_bare_handed_horde_takes_nothing_over_any_horizon() {
    /// Long enough that a `p = a/(a+d)` soft gate would have killed the mammoth many times over —
    /// §4.7 measures that model at sixteen turns for this exact party.
    const HORIZON: u32 = 200;
    /// Sized past a soft gate for the same reason the sweep above is — see that comment. At `800` a
    /// soft gate would *also* report zero here, because damage does not bank between turns (§7), and
    /// this test would pass on the model it exists to reject.
    const HORDE: u32 = 100_000;

    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let labor = LaborConfig::builtin();
    let bare = HuntingParty::builtin_unequipped();
    let mut herd = herd_of(&fauna, MAMMOTH);
    let standing = herd.biomass;

    for turn in 0..HORIZON {
        // Composed before the mutable borrow, exactly as every real take path composes it.
        let seed = core_sim::retreat_seed(u64::from(turn), u64::from(turn), &herd.id, HORDE);
        let outcome = hunt_take(
            &mut herd,
            HORDE,
            STRIP_IT_BARE,
            NO_IMPROVEMENT,
            labor.hunt.per_worker_biomass_capacity,
            &bare,
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
            // A real per-event seed, varying per turn exactly as the take path composes it — so this
            // is not passing because one lucky draw was reused.
            seed,
        );
        assert_eq!(outcome.take.killed, 0, "turn {turn} broke the gate");
    }
    assert_eq!(
        herd.biomass, standing,
        "the herd must not have lost a single unit of biomass over {HORIZON} turns"
    );
}

// ---------------------------------------------------------------------------------------------
// §4.6 — better weapons pay off on big game and nowhere else
// ---------------------------------------------------------------------------------------------

/// Biomass per hunter-turn at a given weapon tier — §4.6's table, measured rather than restated.
fn biomass_per_hunter(species: &str, workers: u32, attack: f32) -> f32 {
    let fauna = FaunaConfig::builtin();
    let body = fauna
        .species_by_display(species)
        .expect("shipped species")
        .body_mass;
    let (killed, _, _) = hunt_once(species, workers, &party_at(attack));
    killed as f32 * body / workers as f32
}

/// **Raising `attack` must raise the biomass a hunter takes from HIGH-DEFENSE quarry and leave an
/// engagement-bound one flat** (§4.6) — the assertion that neither the defense subtraction nor the
/// engagement cap has been quietly linearised.
///
/// Two effects compound here and only one is obvious. The engagement cap is the visible half. The
/// other is that `max(0, attack − defense)` makes high-defense quarry gain **super-linearly**:
/// doubling `20 → 40` takes a mammoth's effective attack `8 → 28` (3.5×) while a rabbit's merely
/// doubles — and the rabbit cannot use it, because reach binds long before damage does.
#[test]
fn a_better_weapon_pays_off_on_big_game_and_not_on_small() {
    /// Enough hunters to engage several mammoths, so the *fight* is what the weapon moves.
    const CREW: u32 = 400;
    const SPEAR: f32 = 20.0;
    const BETTER: f32 = 40.0;

    let mammoth_spear = biomass_per_hunter(MAMMOTH, CREW, SPEAR);
    let mammoth_better = biomass_per_hunter(MAMMOTH, CREW, BETTER);
    assert!(
        mammoth_spear > 0.0,
        "liveness: the spear tier must take mammoths at all ({mammoth_spear})"
    );
    assert!(
        mammoth_better > mammoth_spear,
        "a better point must pay off on megafauna: {mammoth_better} vs {mammoth_spear} per hunter"
    );

    let rabbit_spear = biomass_per_hunter(RABBIT, CREW, SPEAR);
    let rabbit_better = biomass_per_hunter(RABBIT, CREW, BETTER);
    assert!(
        rabbit_spear > 0.0,
        "liveness: the spear tier must take rabbits at all ({rabbit_spear})"
    );
    assert_eq!(
        rabbit_better, rabbit_spear,
        "a better point buys NOTHING on engagement-bound small game — there are only so many \
         rabbits you can lay hands on"
    );
}

/// **No species exceeds `engage_rate × body_mass` per hunter, at any weapon tier** (§4.6) — the
/// ceiling is real, so arbitrarily good kit cannot turn small game into a food engine.
#[test]
fn no_weapon_tier_beats_the_engagement_ceiling() {
    /// A tier far past anything the design contemplates — if the cap holds here it holds anywhere.
    const ABSURD_ATTACK: f32 = 10_000.0;
    const CREW: u32 = 64;

    let fauna = FaunaConfig::builtin();
    for (species, def) in fauna.species.iter() {
        let ceiling = def.engage_rate * def.body_mass;
        let mut ever_took = false;
        for attack in [1.0, 20.0, 40.0, ABSURD_ATTACK] {
            let taken = biomass_per_hunter(&def.display_name, CREW, attack);
            ever_took |= taken > 0.0;
            assert!(
                taken <= ceiling + f32::EPSILON * ceiling.max(1.0),
                "{species} at attack {attack}: {taken} biomass/hunter exceeds the ceiling \
                 {ceiling} (= engage_rate {} × body_mass {})",
                def.engage_rate,
                def.body_mass
            );
        }
        // **Liveness, per species**: a ceiling nothing ever reaches is satisfied by a dead take path.
        assert!(
            ever_took,
            "{species} must be huntable at SOME tier, or the ceiling above asserts nothing"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// §4.2 — the kill rate responds to all three of its inputs
// ---------------------------------------------------------------------------------------------

/// **Turns-to-kill responds to the party, the weapon and the quarry** (§10) — the assertion that no
/// per-species turn count got baked in anywhere.
///
/// Measured as its reciprocal, **animals brought down per turn**, because a party below the gate
/// never kills at all and "turns to kill" is then infinite rather than large — the gate is pinned on
/// its own above, and this pins the slope.
#[test]
fn the_kill_rate_responds_to_party_weapon_and_quarry() {
    const SPEAR: f32 = 20.0;
    /// A crew big enough that the *fight*, not the engagement, is the binding term on an aurochs
    /// (`engage_rate 0.17`, `durability 150`): reach allows 6, damage allows ~3.
    const CREW: u32 = 40;
    const AUROCHS: &str = "Wild Aurochs";

    let base = hunt_once(AUROCHS, CREW, &party_at(SPEAR)).0;
    assert!(
        base > 0,
        "liveness: the reference party must kill something"
    );

    // (a) The PARTY — twice the hunters, more animals down.
    let bigger = hunt_once(AUROCHS, CREW * 2, &party_at(SPEAR)).0;
    assert!(
        bigger > base,
        "doubling the party must bring more down: {bigger} vs {base}"
    );

    // (b) The WEAPON — same party, a better point.
    let better = hunt_once(AUROCHS, CREW, &party_at(SPEAR * 2.0)).0;
    assert!(
        better > base,
        "a better weapon must bring more down: {better} vs {base}"
    );

    // (c) The QUARRY — the same party against a tougher body takes fewer. A mammoth is `defense 12 /
    // durability 500` against the aurochs' `6 / 150`, so it is tougher on both axes.
    let tougher = hunt_once(MAMMOTH, CREW, &party_at(SPEAR)).0;
    assert!(
        tougher < base,
        "a tougher quarry must come down more slowly: {tougher} mammoths vs {base} aurochs"
    );
}

/// **A fractional engagement reaches ONE animal, not zero** (§10) — contact is not the gate. Three
/// hunters do walk up to a mammoth (`engage_rate 0.05` would floor to `0`); they then fail at the
/// *fight*, with casualties, which is a different and legible failure.
#[test]
fn a_fractional_engagement_reaches_one_animal_and_fails_at_the_fight() {
    const TINY_PARTY: u32 = 3;
    let fauna = FaunaConfig::builtin();
    let engage = fauna
        .species_by_display(MAMMOTH)
        .expect("shipped species")
        .engage_rate;
    assert!(
        (TINY_PARTY as f32 * engage) < 1.0,
        "the fixture must actually be in the fractional regime ({TINY_PARTY} × {engage})"
    );
    assert_eq!(
        animals_engaged(TINY_PARTY, engage, 1.0),
        1.0,
        "a party that exists reaches one animal"
    );

    let (killed, casualties, fought) =
        hunt_once(MAMMOTH, TINY_PARTY, &HuntingParty::builtin_equipped());
    assert_eq!(killed, 0, "three spears cannot bring a mammoth down");
    assert!(fought, "...but they did fight it");
    assert!(
        casualties > 0.0,
        "and it cost them people (got {casualties}) — the failure is the fight, not the search"
    );
}

// ---------------------------------------------------------------------------------------------
// §4.5 — most hunts must not feel like battles
// ---------------------------------------------------------------------------------------------

/// **The one-sided engagement is free** (§4.5): a quarry that contributes no attack costs the party
/// nothing, produces no battle report, and still hands over everything it engaged. Snaring rabbits is
/// not a war.
///
/// `fought == false` is what the systems read to decide whether a `HuntDanger` line is emitted, so
/// this is the report assertion, not a proxy for it.
#[test]
fn a_harmless_quarry_costs_nothing_and_reports_nothing() {
    const CREW: u32 = 16;
    let (killed, casualties, fought) = hunt_once(RABBIT, CREW, &HuntingParty::builtin_equipped());
    assert!(killed > 0, "liveness: the party must actually take rabbits");
    assert_eq!(casualties, 0.0, "a rabbit cannot hurt anyone");
    assert!(
        !fought,
        "a one-sided engagement is not a battle — no report, no ceremony"
    );
    // The same crew against a quarry that DOES fight back takes the other branch, so the flag is
    // discriminating rather than always-false.
    let (_, deer_casualties, deer_fought) =
        hunt_once(DEER, CREW, &HuntingParty::builtin_equipped());
    assert!(
        deer_fought,
        "a ferocity-bearing quarry must resolve as a real fight"
    );
    // A Red Deer's `attack 0.8 × ferocity 0.15 = 0.12` does not clear a human's `defense 1`, so the
    // fight happens and costs nothing — the gate applied to the ANIMAL side.
    assert_eq!(
        deer_casualties, 0.0,
        "a deer swings, and the gate says it does not connect"
    );
}

/// **The fast path is exactly the model's answer, not a second one.** The one-sided branch skips
/// building the payload; it must bring down the same animals the full resolver would. Pinned by
/// giving the same quarry a hair of ferocity — which forces the general path — and comparing.
#[test]
fn the_fast_path_agrees_with_the_full_resolver() {
    /// Deliberately **too small to clear the engagement** — `2 × 20 / durability 2 = 20` of the 40
    /// standing. A crew that could kill everything it engaged would make this fixture agree with a
    /// "the fast path just returns `stayed`" mutant, which is the exact second-model error §4.5
    /// forbids.
    const CREW: f32 = 2.0;
    const STAYED: f32 = 40.0;
    let fauna = FaunaConfig::builtin();
    let party = HuntingParty::builtin_equipped();
    let harmless = fauna.quarry_fight_for(RABBIT);
    assert_eq!(
        harmless.effective_attack(),
        0.0,
        "the fixture's quarry must genuinely be harmless, or the branch under test is not taken"
    );

    let fast = resolve_hunt_fight(STAYED, CREW, &party, &harmless, FIXED_SEED);
    assert!(!fast.fought, "a harmless quarry takes the fast path");
    assert!(
        fast.brought_down < STAYED,
        "the fixture must be in the regime where damage — not reach — is what binds, or a fast path \
         that simply returned `stayed` would agree with the resolver by accident"
    );
    assert!(
        fast.brought_down > 0.0,
        "liveness: it still kills something"
    );

    // The same body, made barely dangerous — enough to force the general path, far too little to
    // change how many of them go down.
    let mut dangerous = harmless;
    dangerous.profile.attack = 1.0;
    dangerous.ferocity = 1.0;
    let full = resolve_hunt_fight(STAYED, CREW, &party, &dangerous, FIXED_SEED);
    assert!(full.fought, "the control must take the general path");
    assert_eq!(
        fast.brought_down, full.brought_down,
        "the fast path is an optimisation, not a second model"
    );
}

// ---------------------------------------------------------------------------------------------
// §10 — the pen is untouched, and replay is order-independent
// ---------------------------------------------------------------------------------------------

/// **A penned animal is not stalked, not fought and not wary** (§10). The pen's corral-tend branch
/// passes `f32::INFINITY` as its engagement, and the fight must hand it straight back untouched —
/// including for a species that would otherwise be the deadliest fight on the map.
#[test]
fn a_pen_has_no_fight_at_all() {
    let fauna = FaunaConfig::builtin();
    let party = HuntingParty::builtin_equipped();
    let mammoth = fauna.quarry_fight_for(MAMMOTH);
    assert!(
        mammoth.effective_attack() > 0.0,
        "the fixture's species must be one that WOULD fight, or this asserts nothing"
    );

    let penned = resolve_hunt_fight(f32::INFINITY, 1.0, &party, &mammoth, FIXED_SEED);
    assert_eq!(penned.brought_down, f32::INFINITY);
    assert_eq!(penned.casualties.killed, 0.0);
    assert_eq!(penned.casualties.wounded, 0.0);
    assert!(!penned.fought);

    // ...and the quantiser therefore pays the pen exactly what it paid before the fight existed:
    // production against the keeper's collection, with no third bound.
    /// One body's worth and a little over, so the pen can spare exactly one animal.
    const PRODUCTION: f32 = 900.0;
    /// Less than one body — the keeper cannot haul the whole beast, which is where `max(1, carryable)`
    /// lives. That rule is untouched by this slice and the pen is where it stays reachable.
    const COLLECTION: f32 = 120.0;
    let body = fauna
        .species_by_display(MAMMOTH)
        .expect("shipped species")
        .body_mass;
    let take = quantise_animal_take(PRODUCTION, COLLECTION, body, penned.brought_down);
    assert_eq!(take.killed, 1, "the keeper butchers what the pen produced");
    assert_eq!(take.carried, COLLECTION);
    assert_eq!(take.wasted, body - COLLECTION);
}

/// **Replay determinism ACROSS HUNT ORDERING** (§6.2) — the assertion that per-event seeding is real
/// and no shared RNG stream crept in. Two herds hunted in opposite orders must produce identical
/// outcomes.
///
/// Run with a **sub-1 `hit_chance`**, so the draw is genuinely live: at the shipped `1.0` the fight
/// consumes no randomness and the property would hold for the wrong reason.
#[test]
fn hunt_ordering_does_not_change_outcomes() {
    let fauna = FaunaConfig::builtin();
    let base = HuntingParty::builtin_equipped();
    let party = HuntingParty {
        tuning: core_sim::CombatTuning {
            hit_chance: 0.5,
            ..base.tuning
        },
        ..base
    };
    let quarry = fauna.quarry_fight_for(DEER);

    // Two distinct engagements, each with its own per-event seed.
    let a = (30.0_f32, 24.0_f32, 0x5EED_A5EE_u64);
    let b = (17.0_f32, 11.0_f32, 0xBEEF_B00F_u64);
    let resolve = |(stayed, hunters, seed): (f32, f32, u64)| {
        resolve_hunt_fight(stayed, hunters, &party, &quarry, seed)
    };

    let forward = (resolve(a), resolve(b));
    let reversed = {
        let second = resolve(b);
        let first = resolve(a);
        (first, second)
    };
    assert_eq!(
        forward, reversed,
        "resolving the same hunts in a different order must give identical outcomes"
    );

    // **Liveness**: the draw is actually live, so the equality above is not the trivial one. A
    // different seed on the same engagement must be able to give a different answer.
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..64_u64 {
        seen.insert(resolve((30.0, 24.0, seed)).brought_down.to_bits());
    }
    assert!(
        seen.len() > 1,
        "a sub-1 hit chance must produce more than one outcome across seeds — otherwise the \
         ordering assertion above holds for the wrong reason"
    );
}

/// **The shipped tuning consumes no randomness at all**, which is what keeps `forecast == actual`
/// per component exact until slice 5 teaches the forecast to report a range. Pinned directly: the
/// seed cannot move the take.
#[test]
fn the_shipped_fight_is_seed_independent() {
    const CREW: u32 = 40;
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let labor = LaborConfig::builtin();
    let party = HuntingParty::builtin_equipped();

    let take_at = |seed: u64| {
        let mut herd = herd_of(&fauna, DEER);
        hunt_take(
            &mut herd,
            CREW,
            STRIP_IT_BARE,
            NO_IMPROVEMENT,
            labor.hunt.per_worker_biomass_capacity,
            &party,
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
            seed,
        )
        .take
    };
    let baseline = take_at(0);
    assert!(baseline.killed > 0, "liveness: the reference take is real");
    for seed in [1_u64, 7, 999, u64::MAX] {
        assert_eq!(
            take_at(seed),
            baseline,
            "seed {seed} moved the shipped take"
        );
    }
}

/// **The take is bounded by the herd's own capacity seam, not by a raw field** — a guard that the
/// fight was slotted in beside the existing bounds rather than in place of one of them. A herd whose
/// escapement room is a single body hands over exactly one animal, however large the party or its
/// weapon.
#[test]
fn the_escapement_floor_still_bounds_a_party_that_could_take_far_more() {
    const HUGE_CREW: u32 = 500;
    let fauna = FaunaConfig::builtin();
    let ladder = LadderConfig::builtin();
    let labor = LaborConfig::builtin();
    let mut herd = herd_of(&fauna, DEER);
    let capacity = herd_capacity(&herd, &fauna);
    // Leave exactly one body standing above a `0.5` floor.
    herd.biomass = 0.5 * capacity + herd.body_mass;

    let outcome = hunt_take(
        &mut herd,
        HUGE_CREW,
        0.5,
        NO_IMPROVEMENT,
        labor.hunt.per_worker_biomass_capacity,
        &HuntingParty::builtin_equipped(),
        &fauna,
        &ladder,
        NO_CARRY_LIMIT,
        FIXED_SEED,
    );
    assert_eq!(
        outcome.take.killed, 1,
        "the floor still decides what the herd can spare"
    );
}

/// The registry handle exists so this file links the same config surface the sim boots with; a
/// compile-time guard that the fixtures above are reading the shipped roster and not a stub.
#[test]
fn the_fixtures_read_the_shipped_roster() {
    let handle = FaunaConfigHandle::default();
    for species in [MAMMOTH, DEER, RABBIT] {
        assert!(
            handle.get().species_by_display(species).is_some(),
            "{species} must be in the shipped roster"
        );
    }
}
