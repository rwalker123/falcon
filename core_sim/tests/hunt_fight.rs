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
    CombatStats, FaunaConfig, FaunaConfigHandle, Herd, HuntDraw, HuntingParty, LaborConfig,
    LadderConfig, SizeClass,
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
/// Wariness is `0` across the shipped roster (slice 7 authors it), so the retreat draw is an exact
/// identity and the seed below is unobservable. Held fixed so these fixtures read as the pure
/// functions they are — and it is a **live** draw, not the forecast's quantile, because these
/// fixtures pin what the sim pays.
const FIXED_SEED: HuntDraw = HuntDraw::Seeded(0);

/// The shipped megafauna: `defense 12`, `durability 500`, `ferocity 0.9`, `engage_rate 0.05`.
const MAMMOTH: &str = "Thunder Mammoths";
/// The shipped mid-game quarry: `defense 1`, `durability 25`, `ferocity 0.15`, `engage_rate 1`.
const DEER: &str = "Red Deer";
/// Defenceless and harmless: `defense 0`, `durability 2`, **no `combat` block at all** so
/// `ferocity 0` — the one-sided engagement of §4.5.
const RABBIT: &str = "Rabbit Warren";

/// **The shipped roster with the retreat stage held at its identity** ([`FaunaConfig::without_retreat`]).
///
/// This file pins the **fight** — the gate, the spillover, the ceiling, the multi-turn wound ledger —
/// and every one of those is an exact-arithmetic claim. Slice 7 authored a non-zero `combat.wariness`
/// roster-wide (`docs/plan_hunt_through_combat.md` §3.1), which puts a binomial in front of the fight
/// and would turn "no headcount of bare hands kills a mammoth" and "the seed cannot move the shipped
/// take" into statements about a draw. The retreat is a different stage with its own suite
/// (`hunt_wariness.rs`); here it is held at `0`.
fn deterministic_fauna() -> std::sync::Arc<FaunaConfig> {
    std::sync::Arc::new(FaunaConfig::builtin().without_retreat())
}

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
    let fauna = deterministic_fauna();
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
    /// Sized past a soft gate for the same reason the sweep above is — see that comment. **Damage
    /// banks between turns now** (§4.2), so a soft gate's trickle would accumulate into kills here;
    /// the horde makes that certain rather than merely likely, and the hard gate still answers zero
    /// because banking exactly `0` forever is still `0`.
    const HORDE: u32 = 100_000;

    let fauna = deterministic_fauna();
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
            HuntDraw::Seeded(seed),
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
    let fauna = deterministic_fauna();
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

    let fauna = deterministic_fauna();
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
    let fauna = deterministic_fauna();
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

/// **The one-sided engagement is not a BATTLE** (§4.5) — no report, no ceremony — **but it is not
/// free** (§4.6): the hunt's own hazard hurts people whatever the quarry does.
///
/// `fought == false` is what the systems read to decide whether a battle happened; the `HuntDanger`
/// feed line is gated on a **death** (`NO_DEATHS_TO_REPORT`), which is what keeps a rabbit warren
/// from pushing "cost 0 lives" every turn.
#[test]
fn a_harmless_quarry_is_no_battle_but_still_hurts_someone() {
    const CREW: u32 = 16;
    let (killed, casualties, fought) = hunt_once(RABBIT, CREW, &HuntingParty::builtin_equipped());
    assert!(killed > 0, "liveness: the party must actually take rabbits");
    assert!(
        !fought,
        "a one-sided engagement is not a battle — no report, no ceremony"
    );
    assert!(
        casualties > 0.0,
        "a rabbit cannot swing at you, but the hunt itself can hurt you (got {casualties})"
    );
    // The same crew against a quarry that DOES fight back takes the other branch, so the flag is
    // discriminating rather than always-false.
    let (_, deer_casualties, deer_fought) =
        hunt_once(DEER, CREW, &HuntingParty::builtin_equipped());
    assert!(
        deer_fought,
        "a ferocity-bearing quarry must resolve as a real fight"
    );
    assert!(
        deer_casualties > 0.0,
        "…and it costs the same baseline injuries the rabbit hunt did"
    );
}

/// **The baseline injury NEVER kills** (§4.6) — the whole reason it can ride every hunt without
/// being a balance change. `available_workers` floors a cohort's working scalar, so one fractional
/// death costs a whole worker of throughput; a harmless quarry must therefore produce `wounded`
/// alone, while a quarry that genuinely fights back still buries people.
#[test]
fn the_baseline_injury_wounds_and_never_kills() {
    const CREW: u32 = 16;
    let fauna = deterministic_fauna();
    let party = HuntingParty::builtin_equipped();
    let harmless = fauna.quarry_fight_for(RABBIT);
    let injured = resolve_hunt_fight(8.0, CREW as f32, &party, &harmless, FIXED_SEED);
    assert_eq!(
        injured.casualties.killed, 0.0,
        "a hunting accident is recoverable — it must not touch the working-age bracket"
    );
    assert!(
        injured.casualties.wounded > 0.0,
        "…but it is real: someone got hurt"
    );

    // The control: a mammoth swings hard enough to clear a human's `defense`, and that DOES kill.
    let (_, _, fought) = hunt_once(MAMMOTH, CREW, &party);
    assert!(fought, "the control must be a real fight");
    let mammoth = fauna.quarry_fight_for(MAMMOTH);
    let mauled = resolve_hunt_fight(1.0, CREW as f32, &party, &mammoth, FIXED_SEED);
    assert!(
        mauled.casualties.killed > 0.0,
        "a mammoth still buries people — the baseline is an addition, not a replacement"
    );
}

/// **The baseline injury scales with the ENGAGEMENT and not with the quarry** (§4.6), and it does
/// **not dominate a dangerous one** — the two halves that make it texture rather than a second
/// combat model.
#[test]
fn the_baseline_injury_tracks_the_engagement_and_never_dominates_a_real_fight() {
    const CREW: f32 = 16.0;
    let fauna = deterministic_fauna();
    let party = HuntingParty::builtin_equipped();
    let harmless = fauna.quarry_fight_for(RABBIT);

    // More animals worked → more chances to get hurt, on the same crew and the same quarry.
    let few = resolve_hunt_fight(2.0, CREW, &party, &harmless, FIXED_SEED);
    let many = resolve_hunt_fight(20.0, CREW, &party, &harmless, FIXED_SEED);
    assert!(
        many.casualties.wounded > few.casualties.wounded,
        "working ten times as many animals must cost more: {} vs {}",
        many.casualties.wounded,
        few.casualties.wounded
    );

    // ...and against a quarry that fights back it is a rounding error beside what the animal does.
    let mammoth = fauna.quarry_fight_for(MAMMOTH);
    let one_mammoth = resolve_hunt_fight(1.0, CREW, &party, &mammoth, FIXED_SEED);
    let baseline_only = resolve_hunt_fight(1.0, CREW, &party, &harmless, FIXED_SEED);
    let mammoth_cost = one_mammoth.casualties.killed + one_mammoth.casualties.wounded;
    let baseline_cost = baseline_only.casualties.killed + baseline_only.casualties.wounded;
    assert!(
        baseline_cost * 10.0 < mammoth_cost,
        "the activity's hazard ({baseline_cost}) must be far below what a mammoth does \
         ({mammoth_cost}) on the same engagement"
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
    let fauna = deterministic_fauna();
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
// §4.2 — damage carries between turns
// ---------------------------------------------------------------------------------------------

/// **A SUB-THRESHOLD PARTY EVENTUALLY KILLS** — the case the whole cross-turn ledger exists for
/// (§4.2). A party below `ceil(durability / (attack − defense))` brings down nothing on turn one and
/// a whole animal several turns later, because the damage it deals is banked on the quarry.
///
/// Without the ledger the gate is **absolute rather than steep**: 63 hunters for a mammoth at the
/// shipped spear would be a hard threshold, and 62 would take casualties every turn forever.
///
/// Paired both ways: the first turn is honestly zero (the gate has not been softened) **and** the
/// kill arrives (the bank is live).
#[test]
fn a_sub_threshold_party_kills_after_enough_turns() {
    /// Far below `ceil(500 / (20 − 12)) = 63`, so a stateless resolver answers zero forever.
    const SMALL_PARTY: u32 = 10;
    /// Generous room past the ~7 turns the arithmetic predicts, so the test measures the mechanic
    /// rather than an exact tuning.
    const PATIENCE: u32 = 40;

    let fauna = deterministic_fauna();
    let ladder = LadderConfig::builtin();
    let labor = LaborConfig::builtin();
    let party = HuntingParty::builtin_equipped();
    let threshold = {
        let quarry = fauna.species_by_display(MAMMOTH).expect("shipped species");
        let per_hunter = core_sim::strike_damage(party.hunter.attack, quarry.combat.defense);
        (quarry.combat.durability / per_hunter).ceil()
    };
    assert!(
        (SMALL_PARTY as f32) < threshold,
        "the fixture must be genuinely sub-threshold ({SMALL_PARTY} vs {threshold})"
    );

    let mut herd = herd_of(&fauna, MAMMOTH);
    let mut first_kill_turn = None;
    for turn in 1..=PATIENCE {
        let outcome = hunt_take(
            &mut herd,
            SMALL_PARTY,
            STRIP_IT_BARE,
            NO_IMPROVEMENT,
            labor.hunt.per_worker_biomass_capacity,
            &party,
            &fauna,
            &ladder,
            NO_CARRY_LIMIT,
            FIXED_SEED,
        );
        if turn == 1 {
            assert_eq!(
                outcome.take.killed, 0,
                "one turn of a sub-threshold party is still not a kill — the gate is steep, \
                 not softened"
            );
        }
        if outcome.take.killed > 0 {
            first_kill_turn = Some(turn);
            break;
        }
        assert!(
            herd.wounds.pending() > 0.0,
            "turn {turn}: the damage must be BANKED, not discarded"
        );
    }
    let first_kill_turn =
        first_kill_turn.expect("a sub-threshold party must eventually bring the mammoth down");
    assert!(
        first_kill_turn > 1,
        "the fixture is only interesting if the kill takes several turns (got {first_kill_turn})"
    );
    // The ledger holds at most ONE unfinished body — a completed kill spends its damage.
    let body = fauna
        .species_by_display(MAMMOTH)
        .expect("shipped species")
        .combat
        .durability;
    assert!(
        herd.wounds.pending() < body,
        "the bank must never hold more than one unfinished animal ({} vs {body})",
        herd.wounds.pending()
    );
}

/// **A bigger sub-threshold party kills SOONER** — the ledger integrates a rate, so the wait is
/// `durability / (hunters × effective_attack)` rather than an on/off threshold.
#[test]
fn more_hunters_shorten_the_wait_for_a_sub_threshold_kill() {
    let turns_to_first_kill = |workers: u32| -> u32 {
        let fauna = deterministic_fauna();
        let ladder = LadderConfig::builtin();
        let labor = LaborConfig::builtin();
        let party = HuntingParty::builtin_equipped();
        let mut herd = herd_of(&fauna, MAMMOTH);
        for turn in 1..=100 {
            let outcome = hunt_take(
                &mut herd,
                workers,
                STRIP_IT_BARE,
                NO_IMPROVEMENT,
                labor.hunt.per_worker_biomass_capacity,
                &party,
                &fauna,
                &ladder,
                NO_CARRY_LIMIT,
                FIXED_SEED,
            );
            if outcome.take.killed > 0 {
                return turn;
            }
        }
        panic!("{workers} hunters never brought a mammoth down");
    };
    let slow = turns_to_first_kill(5);
    let fast = turns_to_first_kill(20);
    assert!(slow > 1, "liveness: the small party must genuinely wait");
    assert!(
        fast < slow,
        "four times the hunters must land the kill sooner: {fast} vs {slow} turns"
    );
}

/// **Wounds heal when the party breaks off — and they heal to EXACTLY zero** (§4.2). Pinned in both
/// directions, because either failure mode is silent: instant forgetting makes a broken-off hunt
/// worthless, and never forgetting lets a party chip at a mammoth across fifty turns of unrelated
/// play.
#[test]
fn wounds_decay_out_of_contact_but_not_instantly() {
    let fauna = deterministic_fauna();
    let combat = core_sim::CombatConfig::builtin();
    let body = fauna
        .species_by_display(MAMMOTH)
        .expect("shipped species")
        .combat;
    let mut wounds = core_sim::DamageLedger::default();
    /// Enough to bank real damage without completing a body.
    const HALF_A_MAMMOTH: f32 = 250.0;
    assert_eq!(
        wounds.strike(HALF_A_MAMMOTH, &body, 1.0),
        0.0,
        "the fixture must bank rather than kill"
    );
    assert_eq!(wounds.pending(), HALF_A_MAMMOTH);

    // The turn the party is still in contact spends the grace, not the wound.
    wounds.recover(combat.wound_recovery_rate, &body);
    assert_eq!(
        wounds.pending(),
        HALF_A_MAMMOTH,
        "a herd struck this turn does not heal — it clears the contact flag"
    );

    // The first genuinely idle turn decays it, and by less than all of it.
    wounds.recover(combat.wound_recovery_rate, &body);
    let after_one = wounds.pending();
    assert!(
        after_one < HALF_A_MAMMOTH,
        "an idle turn must heal something ({after_one} vs {HALF_A_MAMMOTH})"
    );
    assert!(
        after_one > 0.0,
        "…and NOT everything: a hunt broken off for one turn must still be worth resuming"
    );

    // Left alone it reaches exactly zero — "something eventually clears it".
    for _ in 0..core_sim::CombatTuning::default()
        .wound_recovery_rate
        .recip()
        .ceil() as u32
    {
        wounds.recover(combat.wound_recovery_rate, &body);
    }
    assert_eq!(
        wounds.pending(),
        0.0,
        "linear decay must empty the ledger, not asymptote at a sliver"
    );
    assert!(wounds.is_clean(), "and leave no contact flag behind");
}

// ---------------------------------------------------------------------------------------------
// §10 — the pen is untouched, and replay is order-independent
// ---------------------------------------------------------------------------------------------

/// **A penned animal is not stalked, not fought and not wary** (§10). The pen's corral-tend branch
/// passes `f32::INFINITY` as its engagement, and the fight must hand it straight back untouched —
/// including for a species that would otherwise be the deadliest fight on the map.
#[test]
fn a_pen_has_no_fight_at_all() {
    let fauna = deterministic_fauna();
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
    let take = quantise_animal_take(
        PRODUCTION,
        COLLECTION,
        body,
        penned.brought_down,
        core_sim::EngagementStop::WhenPackFull,
    );
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
    let fauna = deterministic_fauna();
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
        resolve_hunt_fight(stayed, hunters, &party, &quarry, HuntDraw::Seeded(seed))
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

/// **The shipped FIGHT consumes no randomness at all** — `hit_chance` is `1.0`, so every strike lands
/// by identity and the seed cannot move the take (`docs/plan_hunt_through_combat.md` §6.4). It is the
/// fight's half of what keeps `forecast == actual` an exact identity where nothing else is stochastic.
///
/// **The retreat is the other half, and it is no longer inert** — slice 7 authored a real
/// `combat.wariness`, so the roster's own take *is* seed-dependent now. [`deterministic_fauna`] holds
/// it at `0` here precisely so this pin keeps measuring `hit_chance` rather than silently becoming a
/// statement about the retreat draw; `hunt_wariness.rs` asserts the seed-dependence deliberately.
#[test]
fn the_shipped_fight_is_seed_independent() {
    const CREW: u32 = 40;
    let fauna = deterministic_fauna();
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
            HuntDraw::Seeded(seed),
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
    let fauna = deterministic_fauna();
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
