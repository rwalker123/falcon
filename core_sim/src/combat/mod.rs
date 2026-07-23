//! The combat subsystem — a first-class, DRY/SOLID fight resolver behind one stable seam.
//!
//! **A predator encounter, a dangerous hunt, and (one day) a TOE-vs-TOE battle are all just a
//! *fight*.** So the casualty math does not live in the hunt path as a bespoke formula — it lives
//! here, and the hunt/predator code is a thin adapter that builds a [`FightPayload`], calls
//! [`resolve_fight`], and applies the [`FightOutcome`]. Design: `docs/plan_predators.md`.
//!
//! **Dependency inversion is one-way: fauna/labor/population → combat, never back.** This module
//! imports NOTHING from those domains. Combat owns the *algorithm* and its neutral types
//! ([`CombatStats`]); domains adapt *into* it (fauna embeds `CombatStats` on its species table, the
//! creatures roster holds the base human's), so a new combatant kind (barbarians, armies, mechs) is
//! a new *adapter*, never an edit to combat.
//!
//! [`resolve_fight`] is a **pure function** (deterministic, rollback-safe, no RNG). Its internals are
//! a deliberate *placeholder* that nonetheless consumes the real contract shapes — per-contingent
//! composition, range bands, death/wound split — so when the real resolver lands (ranged pre-phase,
//! terrain cover, morale/break) only the function body changes and every caller stays put.

use serde::Deserialize;

/// **Which range band a unit fights in** — combat's neutral range type, persisted-enum convention
/// (`as_str` / `from_key`, default [`RangeBand::Melee`]). Artillery is `Ranged` (lethal at distance,
/// near-useless up close); a wolf or a spearman is `Melee`. **Accepted but ignored by the placeholder
/// resolver** — reserved for the ranged pre-phase the real resolver will add.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RangeBand {
    /// Fights up close. The default.
    #[default]
    Melee,
    /// Strikes at distance.
    Ranged,
}

impl RangeBand {
    /// Stable string key (the JSON spelling and any future snapshot field).
    pub fn as_str(&self) -> &'static str {
        match self {
            RangeBand::Melee => "melee",
            RangeBand::Ranged => "ranged",
        }
    }

    /// Parse the stable key back (inverse of [`RangeBand::as_str`]); unknown/empty → [`RangeBand::Melee`].
    pub fn from_key(key: &str) -> Self {
        match key {
            "ranged" => RangeBand::Ranged,
            _ => RangeBand::Melee,
        }
    }
}

/// **Combat's OWN neutral per-unit stat type.** The *same* struct describes a wolf's body and a
/// human's — that is the DRY core (`docs/plan_predators.md` "a wolf and a human are the same
/// combatant"). Domains adapt into it: [`crate::fauna_config::SpeciesDef`] embeds it, the creatures
/// roster ([`crate::creatures_config`]) holds the base human's. This is per-unit-type **data**, never
/// an aggregate outcome-power — combat is never handed "this side has power N".
///
/// `#[serde(default)]` on the struct fills any omitted field from [`CombatStats::default`], so a
/// partial JSON block (`{ "attack": 4.0, "defense": 6.0 }`) reads its `range` as `Melee`, and an
/// entirely-omitted `combat` field on a species reads the harmless default.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(default)]
pub struct CombatStats {
    /// Offensive / retaliation power when a fight happens. Default `0.0` — most prey just runs, so a
    /// hunt is harmless.
    pub attack: f32,
    /// How hard the unit is to bring down. **A denominator in the kill/wound split** (higher defense →
    /// more wounded, fewer killed — the equip-to-shift-severity lever), so it must be `> 0`. Default
    /// `1.0`.
    pub defense: f32,
    /// Which range band the unit fights in. Default [`RangeBand::Melee`].
    pub range: RangeBand,
}

impl Default for CombatStats {
    fn default() -> Self {
        Self {
            attack: 0.0,
            defense: 1.0,
            range: RangeBand::Melee,
        }
    }
}

/// **Whether a force started the fight, was jumped, or is holding.** Accepted but ignored by the
/// placeholder resolver — reserved for ambush/surprise modifiers the real resolver will add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    /// Started the fight (the hunting party; a raiding predator).
    Aggressor,
    /// Fighting back on its own ground (the hunted animal; a raided band).
    Defender,
    /// Caught by surprise.
    Ambushed,
}

/// Maps a side back to its band / herd / faction. Combat is agnostic to what it is; the caller owns
/// the mapping (a `u64` it can turn back into an entity or a herd id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForceId(pub u64);

/// Maps a block of casualties back to *what* took them — a species display name, or `"person"`. Kept
/// an owned `String` so an adapter can key results straight back to a herd or a cohort bracket.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContingentId(pub String);

impl ContingentId {
    /// Borrow the underlying key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ContingentId {
    fn from(value: &str) -> Self {
        ContingentId(value.to_string())
    }
}

/// **A block of like units fighting the same way** — the COMPOSITION, never a scalar. Humans: a squad
/// with one loadout. Animals: the herd's fighting stock. Combat reads these and does all the
/// aggregation itself.
#[derive(Debug, Clone)]
pub struct Contingent {
    /// Maps this block's casualties back (species, or role+equipment).
    pub kind: ContingentId,
    /// Operators present — the attrition quantum. Fractional, because the sim's population brackets
    /// are fractional `Scalar`s.
    pub count: f32,
    /// Per-UNIT stats, composed by the domain adapter (`intrinsic ⊕ equipment`).
    pub profile: CombatStats,
}

/// One side of a fight, described as a composition of [`Contingent`]s. Combat owns the aggregation,
/// range-phasing and attrition — the caller never pre-computes an aggregate power.
#[derive(Debug, Clone)]
pub struct Force {
    /// Maps the side back to its band / herd / faction.
    pub id: ForceId,
    /// Started it / holding / jumped. Accepted, ignored by the placeholder resolver.
    pub posture: Posture,
    /// The composition — one or more blocks of like units.
    pub contingents: Vec<Contingent>,
}

/// A hex in play. **Accepted, ignored by the placeholder resolver** — reserved for terrain cover the
/// real resolver will read. Structured now so adding it later is not a contract change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainContext {
    /// The hex coordinate this context describes.
    pub hex: (u32, u32),
}

/// Everything combat needs to resolve one fight. `sides` is `>= 2` (today exactly 2); combat is
/// agnostic to what they are.
#[derive(Debug, Clone)]
pub struct FightPayload {
    /// The forces present, each a composition of contingents.
    pub sides: Vec<Force>,
    /// The hexes in play — accepted, ignored by the placeholder resolver.
    pub terrain: Vec<TerrainContext>,
    /// Caller-supplied, hasher-independent → rollback-stable. **Reserved for future variance; the
    /// placeholder resolver does not read it** (it is pure arithmetic, no RNG). Passed as a real value
    /// so the seam is ready the day variance lands.
    pub seed: u64,
}

/// Per-contingent casualties, keyed `(ForceId, ContingentId)` so the caller maps them back.
#[derive(Debug, Clone, PartialEq)]
pub struct ContingentResult {
    /// The side these casualties belong to.
    pub force: ForceId,
    /// The contingent (species / role) these casualties belong to.
    pub kind: ContingentId,
    /// **Permanent** losses — removed at the population `death_fraction` seam.
    pub killed: f32,
    /// **Recoverable** losses — survive at reduced capacity while they heal. Modelled from day one so
    /// the real recovery slice is purely additive.
    pub wounded: f32,
}

/// The result of one fight: per-contingent casualties, who won, and whether the loser withdrew.
#[derive(Debug, Clone, PartialEq)]
pub struct FightOutcome {
    /// Per-contingent casualties, keyed `(force, kind)`.
    pub results: Vec<ContingentResult>,
    /// The side with strictly-higher power; `None` on a tie.
    pub victor: Option<ForceId>,
    /// The loser withdrew (yield forfeited) rather than being annihilated — true iff the loser's
    /// losses exceed [`CombatTuning::disengage_fraction`] of its headcount.
    pub disengaged: bool,
}

/// **Resolver tuning** — the severity constants the placeholder resolver reads, kept out of the
/// creature-identity data (`docs/plan_predators.md`: `combat_config.json` is resolver tuning, not
/// creature identity). Held here as combat's own type so [`resolve_fight`] stays a pure function
/// testable without a Bevy world; [`crate::combat_config`] loads it from JSON.
#[derive(Debug, Clone, Copy)]
pub struct CombatTuning {
    /// Scales every side's total losses. `1.0` is the shipped anchor.
    pub lethality: f32,
    /// A loser whose losses exceed this fraction of its headcount is driven off (`disengaged`) rather
    /// than annihilated. `0.5` is the shipped anchor.
    pub disengage_fraction: f32,
}

impl Default for CombatTuning {
    fn default() -> Self {
        Self {
            lethality: DEFAULT_LETHALITY,
            disengage_fraction: DEFAULT_DISENGAGE_FRACTION,
        }
    }
}

/// Shipped `lethality` — every side's losses scale linearly with this. See [`CombatTuning`].
pub(crate) const DEFAULT_LETHALITY: f32 = 1.0;
/// Shipped `disengage_fraction` — a loser past this loss share is driven off, not annihilated.
pub(crate) const DEFAULT_DISENGAGE_FRACTION: f32 = 0.5;

/// The smallest own-power we will divide by, so a zero-attack side (all defenders, no offense) does
/// not blow up the enemy-relative loss ratio. Named rather than bare so a reader sees it guards a
/// denominator, not a magic epsilon.
const POWER_EPS: f32 = 1e-6;

/// A force's aggregate power — `Σ count × attack` over its contingents. **Combat's job, never the
/// caller's**: a scalar handed in from outside cannot survive TOE (see `docs/plan_predators.md`).
fn force_power(force: &Force) -> f32 {
    force
        .contingents
        .iter()
        .map(|c| c.count * c.profile.attack)
        .sum()
}

/// A force's headcount — `Σ count` over its contingents.
fn force_count(force: &Force) -> f32 {
    force.contingents.iter().map(|c| c.count).sum()
}

/// The strictly-highest-power side, or `None` on a tie for the top.
fn strict_victor(sides: &[Force], powers: &[f32]) -> Option<ForceId> {
    let mut best: Option<usize> = None;
    let mut tied = false;
    for (i, &p) in powers.iter().enumerate() {
        match best {
            None => best = Some(i),
            Some(b) => {
                if p > powers[b] {
                    best = Some(i);
                    tied = false;
                } else if p == powers[b] {
                    tied = true;
                }
            }
        }
    }
    match best {
        Some(b) if !tied => Some(sides[b].id),
        _ => None,
    }
}

/// **Resolve one fight — deterministic pure arithmetic, no RNG.** The placeholder model
/// (`docs/plan_predators.md` "The placeholder resolver"):
///
/// - `power(side) = Σ contingent.count × attack`; `victor` = strictly-higher power (`None` on a tie).
/// - Each side's **total losses depend on ENEMY power relative to OWN power**, scaled by `lethality`
///   and clamped to its headcount — so **more/stronger defenders take FEWER own casualties** (own
///   count does *not* scale losses; that would invert the mitigation).
/// - Losses distribute across a side's contingents ∝ `count`, capped per contingent.
/// - The kill/wound split is `killed_frac = incoming_per_defender / (incoming_per_defender +
///   own.defense)`, where `incoming_per_defender = power_enemy / count_self` — the enemy's total
///   attack **diluted across the defenders it lands on**. So **more defenders → more wounded, fewer
///   killed** (warriors shift severity by thinning the incoming blow) and **higher own defense → more
///   wounded, fewer killed** (the equip-to-shift-severity lever). Both are the design's whole thesis:
///   *"Warriors and equipment shift the kill↔wound split, not just the total."*
/// - `disengaged` iff the loser's losses exceed `disengage_fraction` of its headcount.
///
/// **Note on the seam** (`docs/plan_predators.md`): the design's prose spelled the split's denominator
/// as `power_enemy / count_enemy`. With the enemy a *single* beast (count 1, fixed) that would leave
/// the split constant no matter how many warriors join — which contradicts the very behaviour Phase 0
/// exists to demonstrate ("the split shifts toward wounded as warriors rise"). Dividing by the
/// **defenders** instead is what makes warriors move severity, so that is the model implemented here.
///
/// `range`, `terrain`, `posture` and `seed` are accepted and **ignored** — reserved for the real
/// resolver. Casualties stay `f32` (whole-unit quantization is a deliberate later refinement).
pub fn resolve_fight(payload: &FightPayload, tuning: &CombatTuning) -> FightOutcome {
    let powers: Vec<f32> = payload.sides.iter().map(force_power).collect();
    let counts: Vec<f32> = payload.sides.iter().map(force_count).collect();
    let total_power: f32 = powers.iter().sum();
    let total_count: f32 = counts.iter().sum();

    let victor = strict_victor(&payload.sides, &powers);

    let mut results = Vec::new();
    let mut loss_totals = vec![0.0_f32; payload.sides.len()];
    for (i, side) in payload.sides.iter().enumerate() {
        let power_self = powers[i];
        let count_self = counts[i];
        // "Enemy" = every OTHER side (generalizes cleanly to the >2-side future; today it is the one
        // opposing force). Losses key off enemy strength RELATIVE TO OWN, so a stronger own side is
        // mitigated — never scaled up by its own count.
        let power_enemy = total_power - power_self;
        let count_enemy = total_count - count_self;
        let losses_total =
            ((power_enemy / power_self.max(POWER_EPS)) * tuning.lethality).clamp(0.0, count_self);
        loss_totals[i] = losses_total;

        // The enemy's total attack **diluted across the defenders it lands on** drives the kill/wound
        // severity split — so more defenders (warriors) thin each blow toward wounding, not killing.
        // Zero own count, or a zero-attack enemy (→ zero power), means zero incoming lethality: nobody
        // dies. `count_enemy` is read only so the >2-side generalization stays explicit.
        let _ = count_enemy;
        let incoming_per_defender = if count_self > 0.0 {
            power_enemy / count_self
        } else {
            0.0
        };
        for contingent in &side.contingents {
            let down = if count_self > 0.0 {
                (losses_total * contingent.count / count_self).clamp(0.0, contingent.count)
            } else {
                0.0
            };
            let denom = incoming_per_defender + contingent.profile.defense;
            let killed_frac = if denom > 0.0 {
                incoming_per_defender / denom
            } else {
                0.0
            };
            let killed = down * killed_frac;
            let wounded = down - killed;
            results.push(ContingentResult {
                force: side.id,
                kind: contingent.kind.clone(),
                killed,
                wounded,
            });
        }
    }

    // The loser withdrew rather than being annihilated iff its losses cleared the disengage share.
    let disengaged = victor.is_some()
        && payload.sides.iter().enumerate().any(|(i, side)| {
            Some(side.id) != victor
                && counts[i] > 0.0
                && loss_totals[i] > tuning.disengage_fraction * counts[i]
        });

    FightOutcome {
        results,
        victor,
        disengaged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(count: f32, attack: f32, defense: f32) -> Contingent {
        Contingent {
            kind: ContingentId::from("person"),
            count,
            profile: CombatStats {
                attack,
                defense,
                range: RangeBand::Melee,
            },
        }
    }

    fn beast(count: f32, attack: f32, defense: f32) -> Contingent {
        Contingent {
            kind: ContingentId::from("beast"),
            count,
            profile: CombatStats {
                attack,
                defense,
                range: RangeBand::Melee,
            },
        }
    }

    fn payload(a: Vec<Contingent>, b: Vec<Contingent>) -> FightPayload {
        FightPayload {
            sides: vec![
                Force {
                    id: ForceId(1),
                    posture: Posture::Aggressor,
                    contingents: a,
                },
                Force {
                    id: ForceId(2),
                    posture: Posture::Defender,
                    contingents: b,
                },
            ],
            terrain: vec![],
            seed: 0,
        }
    }

    fn side_result(out: &FightOutcome, force: ForceId) -> &ContingentResult {
        out.results.iter().find(|r| r.force == force).unwrap()
    }

    #[test]
    fn an_even_fight_ties_with_no_victor() {
        let out = resolve_fight(
            &payload(vec![person(5.0, 1.0, 1.0)], vec![person(5.0, 1.0, 1.0)]),
            &CombatTuning::default(),
        );
        assert_eq!(out.victor, None);
        // Symmetric: equal power both ways → each side's losses_total == lethality (= 1.0), split
        // identically, so both results are equal.
        let a = side_result(&out, ForceId(1));
        let b = side_result(&out, ForceId(2));
        assert!((a.killed - b.killed).abs() < 1e-4);
        assert!((a.wounded - b.wounded).abs() < 1e-4);
        assert!((a.killed + a.wounded - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_five_to_one_mismatch_crushes_the_weak_side() {
        let out = resolve_fight(
            &payload(vec![person(5.0, 1.0, 1.0)], vec![person(1.0, 1.0, 1.0)]),
            &CombatTuning::default(),
        );
        assert_eq!(out.victor, Some(ForceId(1)));
        // Weak side (power 1) faces enemy power 5 → losses = 5/1 = 5, clamped to its headcount of 1.
        let weak = side_result(&out, ForceId(2));
        assert!((weak.killed + weak.wounded - 1.0).abs() < 1e-4);
        // Strong side (power 5) faces enemy power 1 → losses = 1/5 = 0.2 of its 5 — far fewer.
        let strong = side_result(&out, ForceId(1));
        assert!(strong.killed + strong.wounded < weak.killed + weak.wounded);
        // The loser lost its whole headcount (> 0.5) → driven off.
        assert!(out.disengaged);
    }

    #[test]
    fn adding_defenders_reduces_the_strong_sides_own_casualties() {
        let tuning = CombatTuning::default();
        // A lone human vs a dangerous beast (attack 8, defense 12).
        let solo = resolve_fight(
            &payload(vec![person(1.0, 1.0, 1.0)], vec![beast(1.0, 8.0, 12.0)]),
            &tuning,
        );
        // The same beast, but the party is reinforced with warriors (more count → more own power).
        let reinforced = resolve_fight(
            &payload(vec![person(6.0, 1.0, 1.0)], vec![beast(1.0, 8.0, 12.0)]),
            &tuning,
        );
        let solo_people = side_result(&solo, ForceId(1));
        let reinforced_people = side_result(&reinforced, ForceId(1));
        // FEWER own casualties as the party grows: losses key off enemy power ÷ own power.
        assert!(reinforced_people.killed < solo_people.killed);
        // ...and the split shifts toward WOUNDED — the enemy's average attack is spread thinner across
        // the larger party, so a smaller share of each casualty is fatal.
        let solo_wound_share = solo_people.wounded / (solo_people.killed + solo_people.wounded);
        let reinf_wound_share =
            reinforced_people.wounded / (reinforced_people.killed + reinforced_people.wounded);
        assert!(reinf_wound_share > solo_wound_share);
    }

    #[test]
    fn higher_defense_raises_the_wounded_share() {
        let tuning = CombatTuning::default();
        let soft = resolve_fight(
            &payload(vec![person(3.0, 1.0, 1.0)], vec![beast(1.0, 8.0, 6.0)]),
            &tuning,
        );
        let armored = resolve_fight(
            &payload(vec![person(3.0, 1.0, 5.0)], vec![beast(1.0, 8.0, 6.0)]),
            &tuning,
        );
        let soft_people = side_result(&soft, ForceId(1));
        let armored_people = side_result(&armored, ForceId(1));
        let soft_share = soft_people.wounded / (soft_people.killed + soft_people.wounded);
        let armored_share =
            armored_people.wounded / (armored_people.killed + armored_people.wounded);
        assert!(armored_share > soft_share);
    }

    #[test]
    fn a_zero_attack_enemy_inflicts_no_casualties() {
        let out = resolve_fight(
            // Side 2 is harmless (attack 0) — a deer that just runs.
            &payload(vec![person(4.0, 1.0, 1.0)], vec![beast(1.0, 0.0, 1.0)]),
            &CombatTuning::default(),
        );
        let people = side_result(&out, ForceId(1));
        assert_eq!(people.killed, 0.0);
        assert_eq!(people.wounded, 0.0);
        assert_eq!(out.victor, Some(ForceId(1)));
    }

    #[test]
    fn resolution_is_deterministic() {
        let p = payload(vec![person(4.0, 1.0, 2.0)], vec![beast(1.0, 8.0, 12.0)]);
        let a = resolve_fight(&p, &CombatTuning::default());
        let b = resolve_fight(&p, &CombatTuning::default());
        assert_eq!(a, b);
    }
}
