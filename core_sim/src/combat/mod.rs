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
//! [`resolve_fight`] is a **pure function of its payload** (deterministic and rollback-safe: any
//! randomness is drawn from [`FightPayload::seed`], never from a shared stream). Its internals stay a
//! deliberate *placeholder* in the parts this arc did not need — ranged pre-phase, terrain cover,
//! morale/break — so those land as a body change with every caller in place.
//!
//! **The attrition model is damage against durability, and the gate is hard**
//! (`docs/plan_hunt_through_combat.md` §4.2): a unit that cannot beat its target's `defense` does
//! **zero** damage, at any headcount, over any horizon. That is the one property a smooth hit
//! probability would silently break (§4.7 — `p = a/(a+d)` gives eight hundred bare-handed hunters a
//! dead mammoth in sixteen turns), and it is why the gate is a subtraction outside the dice rather
//! than a term inside them.

use rand::{rngs::SmallRng, Rng, SeedableRng};
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
    /// **Whether an incoming hit counts at all** — the hard gate of
    /// `docs/plan_hunt_through_combat.md` §4.2: an attacker lands `max(0, attack − defense)` per
    /// strike, so an attacker below this does **nothing**, however many of it there are. Also a
    /// denominator in the kill/wound split (higher defense → more wounded, fewer killed — the
    /// equip-to-shift-severity lever). Default `1.0`; `0` is legal and means *no protection at all*
    /// (a rabbit).
    ///
    /// **`defense` and [`CombatStats::durability`] blur easily and must not**: defense is whether a
    /// hit counts, durability is how many counting hits it takes.
    pub defense: f32,
    /// **How much damage the unit soaks before it goes down** — the §4.2 attrition denominator, so
    /// `units_down = damage / durability`. Default `1.0`.
    ///
    /// **Authored, never derived from body mass.** Durability is a defensive *strategy* and plenty of
    /// creatures do not use it: a gazelle is not one-third of a deer's toughness, it is not tough at
    /// all — it survives by not being there (`wariness`). Boar and seal are the same body mass and the
    /// boar is nearly twice as durable; a wolf is lighter than a wild sheep and tougher than one.
    /// Deriving it would have destroyed exactly that.
    ///
    /// **Excess damage spills to the next unit in the contingent**, because the division is
    /// continuous: 20 damage into 2-durability rabbits brings down ten of them, and the number rises
    /// on its own when the party's weapon improves. No creature carries a "how many of these per turn"
    /// figure.
    pub durability: f32,
    /// Which range band the unit fights in. Default [`RangeBand::Melee`].
    pub range: RangeBand,
    /// **The probability this combatant breaks off when contact is made** — engaged, then gone before
    /// the fight resolves (`docs/plan_hunt_through_combat.md` §3). `0.0..1.0`.
    ///
    /// **It is not a hunting concept**, which is why it lives here and not on
    /// [`crate::fauna_config::SpeciesDef`]. Shocked troops break on contact by the same mechanism;
    /// the only difference between an animal and a soldier is how the value is *maintained* —
    /// authored and **static** per species, **dynamic** from morale for troops — never what it means.
    /// Modelled as *"how many you can get near"* it would have been a hunt-only invention needing its
    /// own justification; as *"who stays when it starts"* it is one field serving both.
    ///
    /// **Default `0.0` is an exact identity, not a roll with probability zero**: no draw is made and
    /// no randomness is consumed, which is what lets this ship inert and keeps every existing yield
    /// test pinning the numbers it pins today. See [`crate::fauna::animals_that_stay`].
    pub wariness: f32,
}

impl Default for CombatStats {
    fn default() -> Self {
        Self {
            attack: 0.0,
            defense: 1.0,
            durability: 1.0,
            range: RangeBand::Melee,
            wariness: 0.0,
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
    /// **How much damage landed on this contingent this turn** — after [`CombatTuning::lethality`]
    /// and before the division by `durability`, so it is the raw *flow* rather than a body count.
    ///
    /// It exists because a fight that spans turns has to bank what did not finish a body
    /// ([`DamageLedger`]), and only the resolver knows how much that was: `killed + wounded` has
    /// already been through the division **and** the clamp to `count`, so it cannot be inverted.
    pub damage_dealt: f32,
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombatTuning {
    /// Scales every side's total losses. `1.0` is the shipped anchor.
    pub lethality: f32,
    /// A loser whose losses exceed this fraction of its headcount is driven off (`disengaged`) rather
    /// than annihilated. `0.5` is the shipped anchor.
    pub disengage_fraction: f32,
    /// **The probability one unit's attack lands** — where variance lives
    /// (`docs/plan_hunt_through_combat.md` §4.7). Drawn per *unit*, so it is **binomial in force
    /// size**: three hunters are a gamble, thirty are reliable, and party size is a real decision
    /// rather than a threshold to clear. A flat ±X% on the damage total cannot buy that.
    ///
    /// **It never softens the gate.** A unit below its target's `defense` deals `max(0, a − d) = 0`
    /// per landed strike, so no number of successful rolls produces damage — the requirement §4.7
    /// calls the trap.
    ///
    /// **`1.0` is an exact identity, not a roll that always succeeds**: no draw is made and no
    /// randomness is consumed, which is what lets this ship inert — the forecast's reported range
    /// collapses to a point and `forecast == actual` stays an exact identity — with the machinery in
    /// place and tested. Same discipline as [`CombatStats::wariness`]'s `0`.
    pub hit_chance: f32,
    /// **How much of its own `durability` a wounded body knits back per turn OUT OF CONTACT** — the
    /// decay half of [`DamageLedger`], as a share of `durability` rather than of the banked damage.
    ///
    /// Linear in `durability`, deliberately: a geometric decay of the *bank* never reaches zero, so a
    /// mammoth would carry a sliver of a hunt forever and "something eventually clears it" could not
    /// be pinned. This shape empties the ledger in exactly `ceil(1 / rate)` idle turns whatever it
    /// held. Ships **0.2** — five quiet turns and the animal is whole again.
    ///
    /// **It only applies out of contact** (see [`DamageLedger::recover`]). Healing every turn
    /// regardless would make `hunters × effective_attack > rate × durability` a *second* absolute
    /// threshold, which is exactly the shape the accumulator exists to remove.
    pub wound_recovery_rate: f32,
    /// **Whether this side's attack rolls are DRAWN or read off the distribution** — see
    /// [`StrikeDraw`]. A live fight is [`StrikeDraw::Seeded`]; a forecast is
    /// [`StrikeDraw::Quantile`], because a projection has no event seed to draw with.
    pub draw: StrikeDraw,
}

/// **How a fight resolves the per-unit attack roll of `docs/plan_hunt_through_combat.md` §4.7** —
/// the one place the model's variance is either *drawn* or *read off its own distribution*.
///
/// # Why a forecast cannot simply pass a seed
///
/// [`crate::fauna::retreat_seed`] is composed from `(map_seed, tick, herd, party)` and a projection
/// is projecting into ticks that have not happened, so it cannot name the tick the live fight will
/// draw with — a forecast that guessed one would report a *different* roll from the take with
/// exactly the same confidence (§6.4). So a forecast makes **no draw at all** and reads the binomial
/// analytically instead, which is what lets `forecast == actual` be restated as a claim about the
/// distribution rather than abandoned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StrikeDraw {
    /// **A live fight**: draw each unit's attack from the fight's own per-event stream
    /// ([`FightPayload::seed`]), so resolving the same fights in a different order gives identical
    /// outcomes (§6.2).
    Seeded,
    /// **A forecast**: make no draw, and take the binomial `sigmas` standard deviations from its
    /// mean. [`EXPECTED_STRIKES`] is the point estimate; `±k` are the bounds the range readout
    /// reports.
    ///
    /// At `hit_chance >= 1` (the shipped tuning) every unit lands whatever this says, so the two
    /// variants are the *same function* there — which is what makes the shipped forecast an exact
    /// identity with the take rather than an approximation of it.
    Quantile { sigmas: f32 },
}

/// [`StrikeDraw::Quantile`]'s **point estimate** — zero standard deviations from the mean, i.e. the
/// expectation. Named because `0.0` at a call site reads like "no variance" when it means "the
/// middle of it".
pub const EXPECTED_STRIKES: f32 = 0.0;

impl Default for CombatTuning {
    fn default() -> Self {
        Self {
            lethality: DEFAULT_LETHALITY,
            disengage_fraction: DEFAULT_DISENGAGE_FRACTION,
            hit_chance: DEFAULT_HIT_CHANCE,
            wound_recovery_rate: DEFAULT_WOUND_RECOVERY_RATE,
            // A tuning describes a **live** fight unless a forecast says otherwise: the default must
            // be the one that draws, so a path that forgets to state its mode still reproduces.
            draw: StrikeDraw::Seeded,
        }
    }
}

/// Shipped `lethality` — every side's losses scale linearly with this. See [`CombatTuning`].
pub(crate) const DEFAULT_LETHALITY: f32 = 1.0;
/// Shipped `disengage_fraction` — a loser past this loss share is driven off, not annihilated.
pub(crate) const DEFAULT_DISENGAGE_FRACTION: f32 = 0.5;
/// Shipped `hit_chance` — **certainty**, so the resolver draws nothing and consumes no randomness.
/// See [`CombatTuning::hit_chance`].
pub(crate) const DEFAULT_HIT_CHANCE: f32 = 1.0;
/// Shipped `wound_recovery_rate` — a fifth of a body per idle turn, so five quiet turns clear any
/// wound. See [`CombatTuning::wound_recovery_rate`].
pub(crate) const DEFAULT_WOUND_RECOVERY_RATE: f32 = 0.2;

/// The smallest `durability` [`resolve_fight`] will divide damage by. Named rather than bare so a
/// reader sees it guards a denominator: every config path validates `durability > 0`, and this stops
/// a hand-built [`CombatStats`] from turning one point of damage into infinitely many casualties.
const DURABILITY_EPS: f32 = 1e-6;

/// **The most individual attack rolls one contingent will make.** Above this the binomial is
/// indistinguishable from its mean at `f32` precision, and rolling it would cost more than the fight
/// is worth — so a contingent larger than this takes its expectation (`count × hit_chance`) instead.
/// The shipped `hit_chance` of `1.0` draws nothing at all, so nothing reaches this today; it exists
/// so a future sub-1 chance cannot turn a hundred-thousand-strong force into a hang.
const MAX_INDIVIDUAL_ATTACK_ROLLS: f32 = 100_000.0;

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

/// **How many of `attackers` land a blow this round** — the binomial of
/// `docs/plan_hunt_through_combat.md` §4.7, drawn per *unit* so variance shrinks as the force grows.
///
/// `chance >= 1` is an **exact identity**: every unit lands, no draw is made and **no randomness is
/// consumed**, which is what lets the shipped tuning keep `resolve_fight` a pure arithmetic function.
/// `chance <= 0` is the mirror identity — nobody lands, and again no draw.
///
/// The fractional remainder of a `count` (the sim's cohorts are fractional) contributes its
/// expectation rather than a half-roll; and a force past [`MAX_INDIVIDUAL_ATTACK_ROLLS`] takes its
/// expectation whole, because at that size the binomial is its mean.
pub fn attacks_landed(attackers: f32, chance: f32, rng: &mut SmallRng) -> f32 {
    if attackers <= 0.0 {
        return 0.0;
    }
    if chance >= 1.0 {
        return attackers;
    }
    if chance <= 0.0 {
        return 0.0;
    }
    if attackers > MAX_INDIVIDUAL_ATTACK_ROLLS {
        return attackers * chance;
    }
    let whole = attackers.floor();
    let odds = f64::from(chance);
    let landed = (0..whole as u32).filter(|_| rng.gen_bool(odds)).count() as f32;
    landed + (attackers - whole) * chance
}

/// **[`attacks_landed`] read off its own distribution instead of drawn** — the binomial's mean plus
/// `sigmas` of its standard deviation, clamped to the support `0..=attackers`.
///
/// This is what a **forecast** calls, because a projection has no event seed (see [`StrikeDraw`]).
/// `sigmas = `[`EXPECTED_STRIKES`] is the point estimate; `±k` are the range's bounds.
///
/// **Its identities are [`attacks_landed`]'s identities, deliberately.** `chance >= 1` answers
/// `attackers` and `chance <= 0` answers `0` *whatever* `sigmas` says — a degenerate distribution has
/// no spread to quantile — which is exactly what makes the shipped tuning's forecast **bit-identical**
/// to the take rather than merely close to it. Any divergence here would surface as a forecast that
/// disagrees with the number the sim pays on a roster with no randomness in it at all.
pub fn attacks_landed_at(attackers: f32, chance: f32, sigmas: f32) -> f32 {
    if attackers <= 0.0 {
        return 0.0;
    }
    if chance >= 1.0 {
        return attackers;
    }
    if chance <= 0.0 {
        return 0.0;
    }
    let mean = attackers * chance;
    let deviation = (attackers * chance * (1.0 - chance)).sqrt();
    (mean + sigmas * deviation).clamp(0.0, attackers)
}

/// **How many of `attackers` land, resolved the way `tuning` says** — the ONE seam a fight's variance
/// passes through, live or forecast, so a projection cannot grow a second copy of the roll.
pub fn landed_strikes(attackers: f32, tuning: &CombatTuning, rng: &mut SmallRng) -> f32 {
    match tuning.draw {
        StrikeDraw::Seeded => attacks_landed(attackers, tuning.hit_chance, rng),
        StrikeDraw::Quantile { sigmas } => attacks_landed_at(attackers, tuning.hit_chance, sigmas),
    }
}

/// [`landed_strikes`] for a caller that owns no stream — a **provably one-sided** engagement resolving
/// without a [`FightPayload`] (see [`units_brought_down`]). Seeded per event exactly as a fight is, so
/// it is order-independent for the same reason; the `seed` is unread in
/// [`StrikeDraw::Quantile`] mode, which makes no draw at all.
pub fn landed_strikes_seeded(attackers: f32, tuning: &CombatTuning, seed: u64) -> f32 {
    let mut rng = SmallRng::seed_from_u64(seed);
    landed_strikes(attackers, tuning, &mut rng)
}

/// **THE GATE** — what one landed strike does to a target of this `defense`:
/// `max(0, attack − defense)` (`docs/plan_hunt_through_combat.md` §4.2). Below it the answer is
/// **exactly** `0`, which is what makes headcount unable to substitute for a weapon.
///
/// Exposed because [`resolve_fight`] is not the only shape a fight takes: a caller whose enemy
/// contributes no attack at all can read the same numbers without building a payload for casualties
/// that are structurally zero. **It is the only definition of the gate in the crate** — that is why it
/// is a function rather than a subtraction repeated at two sites.
pub fn strike_damage(attack: f32, defense: f32) -> f32 {
    (attack - defense).max(0.0)
}

/// **How many units `damage` brings down** — `damage / durability`, capped at the units present.
///
/// The division is continuous, so **excess damage spills to the next unit** for free: that is what
/// makes "many small animals per turn" a consequence of the weapon rather than an authored
/// per-species count. [`resolve_fight`]'s companion primitive to [`strike_damage`]; see that function
/// for why both are public.
pub fn units_brought_down(damage: f32, profile: &CombatStats, count: f32) -> f32 {
    (damage / profile.durability.max(DURABILITY_EPS)).clamp(0.0, count.max(0.0))
}

/// **THE kill↔wound severity split** — `incoming_per_defender / (incoming_per_defender + defense)`,
/// stated once (`docs/plan_predators.md`: *"warriors and equipment shift the kill↔wound split, not
/// just the total"*).
///
/// More defenders thin the blow each of them takes, so a bigger force loses the same number of bodies
/// but buries fewer of them; higher own `defense` does the same, which is the equip-to-shift-severity
/// lever. **It takes an ATTACKER's damage**, which is why the hunt's own hazards
/// (`docs/plan_hunt_through_combat.md` §4.6) do not pass through it at all — see
/// `fauna::hunt_injuries`.
fn killed_share(incoming_per_defender: f32, defense: f32) -> f32 {
    let denom = incoming_per_defender + defense;
    if denom > 0.0 {
        (incoming_per_defender / denom).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The relative slop [`DamageLedger::strike`] allows when asking whether banked damage has completed
/// a whole body. Named because it guards a `floor()` on a quotient built by repeated addition: the
/// blow that exactly finishes a unit must count as finishing it, not land one ulp short and leave a
/// body standing on a rounding error.
const WHOLE_UNIT_EPSILON: f32 = 1e-4;

/// **Damage a body has soaked in an ONGOING engagement, carried between turns** — combat's own state
/// type, so a hunt, a predator raid and a future TOE-vs-TOE battle bank a multi-turn fight the same
/// way instead of each growing a private meter.
///
/// # Without it the gate is absolute rather than steep
///
/// A stateless resolver makes `ceil(durability / (attack − defense))` a **hard threshold** — 63
/// hunters for a mammoth at the shipped spear — so a party of 62 takes casualties every turn and
/// kills nothing, on any horizon (`docs/plan_hunt_through_combat.md` §4.2). *"Twenty weak spears and
/// then follow it around for days"* is the intended low-tech experience, and it requires the days to
/// count for something. The **gate itself is untouched**: below it [`strike_damage`] is exactly `0`,
/// and banking zero forever is still zero.
///
/// # Banking damage is legitimate where banking a ceiling was not
///
/// `Herd::hunt_credit`'s resident-band arm was deleted because the escapement ceiling is a **stock**
/// — the biomass standing above the floor — and accumulating a stock compounds it: the herd would
/// hand over its whole surplus every turn *plus* everything it had already given (§7). **Damage is a
/// flow**, a rate of harm per turn, and an accumulator is the correct integral of a rate. A stock
/// must not bank; a rate must. The two are easy to confuse and are opposites, which is why this
/// paragraph sits with the state rather than in a design document.
///
/// # The invariant that keeps it honest
///
/// [`DamageLedger::pending`] is always **below one body's `durability`** and never above what the
/// units actually standing there could soak: whole units are handed back the moment their damage is
/// complete, and a blow struck at bodies that are not there falls on the ground rather than being
/// banked and spent on the next thing that walks past.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DamageLedger {
    /// Damage banked toward the next body, always `< durability`.
    pending: f32,
    /// Was this block struck this turn — the gate on [`DamageLedger::recover`].
    in_contact: bool,
}

impl DamageLedger {
    /// Damage banked toward the next body. `0` for a block nobody is fighting.
    pub fn pending(&self) -> f32 {
        self.pending
    }

    /// Nothing banked and nothing in flight — a block that carries no memory of a fight.
    pub fn is_clean(&self) -> bool {
        self.pending <= 0.0 && !self.in_contact
    }

    /// **Bank this turn's `damage` and hand back the WHOLE units it brings down**, keeping the
    /// unfinished remainder for next turn.
    ///
    /// Whole units, because the caller that needs an accumulator at all is the one that cannot spend
    /// a fraction — you cannot take half a mammoth home, and a fractional kill silently discarded
    /// every turn is exactly the leak this type closes. Excess beyond what the `standing` units could
    /// soak is dropped, the same clamp [`units_brought_down`] applies.
    pub fn strike(&mut self, damage: f32, profile: &CombatStats, standing: f32) -> f32 {
        let damage = damage.max(0.0);
        if damage > 0.0 {
            self.in_contact = true;
        }
        let durability = profile.durability.max(DURABILITY_EPS);
        let soakable = standing.max(0.0) * durability;
        let total = (self.pending + damage).min(soakable);
        let down = (total / durability * (1.0 + WHOLE_UNIT_EPSILON)).floor();
        self.pending = (total - down * durability).max(0.0);
        down
    }

    /// **One turn's healing** — a body left alone knits back `recovery_rate × durability`
    /// ([`CombatTuning::wound_recovery_rate`]).
    ///
    /// **A block struck this turn does not heal**, it simply clears the contact flag: a wound is
    /// forgotten only once the party breaks off. Because the strike lands in the Population stage and
    /// this runs in the *next* turn's Logistics, a party that keeps hunting never lets a turn's
    /// healing through, and one that walks away gets a single turn of grace before the decay starts.
    pub fn recover(&mut self, recovery_rate: f32, profile: &CombatStats) {
        if self.in_contact {
            self.in_contact = false;
            return;
        }
        if self.pending <= 0.0 {
            return;
        }
        let per_turn = recovery_rate.max(0.0) * profile.durability.max(DURABILITY_EPS);
        self.pending = (self.pending - per_turn).max(0.0);
    }
}

/// **Resolve one fight — a pure function of `(payload, tuning)`**, deterministic and rollback-safe:
/// any variance is drawn from [`FightPayload::seed`], never a shared stream.
///
/// # The attrition model — damage against durability (`docs/plan_hunt_through_combat.md` §4.2)
///
/// ```text
/// per-strike damage = max(0, attacker.attack − target.defense)     // THE GATE, and it is hard
/// damage into a target contingent = Σ landed strikes × per-strike damage × lethality
/// units down = min(count, damage / durability)                     // excess spills to the next unit
/// ```
///
/// Three properties this shape exists to guarantee, each of which a more "natural" formulation
/// silently breaks:
/// - **Headcount cannot substitute for a weapon.** Below the gate the per-strike damage is exactly
///   `0`, so no quantity of attackers and no length of horizon produces a single casualty — eight
///   hundred bare-handed people cannot kill a mammoth (§0.2). A smooth `p = a/(a+d)` hit probability
///   would let enough dice roll through, and does it in sixteen turns (§4.7).
/// - **"Many small animals per turn" is not authored.** Excess damage spills to the next unit in the
///   contingent because the division is continuous, so twenty damage into 2-durability quarry brings
///   down ten of them — and the number rises on its own with a better weapon.
/// - **Above the gate, high-`defense` quarry gains SUPER-linearly from a better weapon** (§4.6):
///   doubling `attack` 20 → 40 takes a `defense 12` target's effective attack 8 → 28, while a
///   `defense 0` target's merely doubles.
///
/// # What is unchanged from the placeholder
///
/// - `power(side) = Σ contingent.count × attack`; `victor` = strictly-higher power (`None` on a tie).
/// - The kill/wound split is `killed_frac = incoming_per_defender / (incoming_per_defender +
///   own.defense)`, where `incoming_per_defender = power_enemy / count_self` — the enemy's total
///   attack **diluted across the defenders it lands on**. So **more defenders → more wounded, fewer
///   killed** (warriors shift severity by thinning the incoming blow) and **higher own defense → more
///   wounded, fewer killed** (the equip-to-shift-severity lever): *"Warriors and equipment shift the
///   kill↔wound split, not just the total."* (`docs/plan_predators.md`).
/// - `disengaged` iff the loser's losses exceed `disengage_fraction` of its headcount.
///
/// **A side's own losses no longer shrink with its own headcount, and that is the model change.** The
/// placeholder scaled losses by `power_enemy / power_self`; damage is a quantity the enemy *deals*, so
/// a bigger party takes the same absolute damage and simply spreads it thinner — which is what the
/// split above then reports as a shift toward wounded. Mitigation by numbers survives as severity, not
/// as an exemption from being hit.
///
/// # Allocation across contingents
///
/// Enemy attackers are split across a side's contingents ∝ `count`, and each pair is gated by **that
/// target's** own defense. Damage is **not** redistributed when a contingent is annihilated — one more
/// piece of the placeholder that the real resolver will own; every shipped caller fields a single
/// target contingent per side except the predator raid, whose two contingents share one `defense`.
///
/// `range`, `terrain` and `posture` are accepted and **ignored** — reserved for the real resolver.
/// Casualties stay `f32` (whole-unit quantization belongs to the caller: a hunt floors the animals it
/// brings down, a cohort's brackets are genuinely fractional).
///
/// # A fight that spans turns
///
/// This function resolves **one turn**. A fight that runs longer banks its unfinished damage in a
/// [`DamageLedger`], fed from each result's [`ContingentResult::damage_dealt`] — see that type for
/// why banking a *flow* is right where banking a *stock* was not.
pub fn resolve_fight(payload: &FightPayload, tuning: &CombatTuning) -> FightOutcome {
    let powers: Vec<f32> = payload.sides.iter().map(force_power).collect();
    let counts: Vec<f32> = payload.sides.iter().map(force_count).collect();
    let total_power: f32 = powers.iter().sum();

    let victor = strict_victor(&payload.sides, &powers);

    // One stream per fight, seeded per event by the caller (`docs/plan_hunt_through_combat.md` §6.2)
    // — so resolving the same fights in a different order gives identical outcomes. At the shipped
    // `hit_chance` of `1.0` nothing draws from it at all.
    let mut rng = SmallRng::seed_from_u64(payload.seed);

    let mut results = Vec::new();
    let mut loss_totals = vec![0.0_f32; payload.sides.len()];
    for (i, side) in payload.sides.iter().enumerate() {
        let power_self = powers[i];
        let count_self = counts[i];
        // "Enemy" = every OTHER side (generalizes cleanly to the >2-side future; today it is the one
        // opposing force).
        let power_enemy = total_power - power_self;
        // The enemy's total attack **diluted across the defenders it lands on** drives the kill/wound
        // severity split — so more defenders (warriors) thin each blow toward wounding, not killing.
        // Zero own count, or a zero-attack enemy (→ zero power), means zero incoming lethality: nobody
        // dies.
        let incoming_per_defender = if count_self > 0.0 {
            power_enemy / count_self
        } else {
            0.0
        };
        let mut losses_total = 0.0_f32;
        for contingent in &side.contingents {
            // The enemy's units are split across this side's contingents ∝ their counts, and each
            // pairing is gated by THIS contingent's defense.
            let share = if count_self > 0.0 {
                contingent.count / count_self
            } else {
                0.0
            };
            let mut damage = 0.0_f32;
            for (j, enemy) in payload.sides.iter().enumerate() {
                if j == i {
                    continue;
                }
                for attacker in &enemy.contingents {
                    // **THE GATE.** Below the target's defense a strike does nothing at all — not a
                    // small amount, nothing — so no headcount clears it, and no draw is even made.
                    let per_strike =
                        strike_damage(attacker.profile.attack, contingent.profile.defense);
                    if per_strike <= 0.0 {
                        continue;
                    }
                    let landed = landed_strikes(attacker.count * share, tuning, &mut rng);
                    damage += landed * per_strike;
                }
            }
            let damage_dealt = damage * tuning.lethality;
            let down = units_brought_down(damage_dealt, &contingent.profile, contingent.count);
            losses_total += down;
            let killed = down * killed_share(incoming_per_defender, contingent.profile.defense);
            let wounded = down - killed;
            results.push(ContingentResult {
                force: side.id,
                kind: contingent.kind.clone(),
                killed,
                wounded,
                damage_dealt,
            });
        }
        loss_totals[i] = losses_total;
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

    /// **A degenerate distribution has no spread to quantile, and that is what keeps the shipped
    /// forecast EXACT** (`docs/plan_hunt_through_combat.md` §6.4).
    ///
    /// At the shipped `hit_chance` of `1.0` every unit lands, so the reported low, likely and high
    /// must be *the same bits* — otherwise the pre-commit readout reports a range on a roster with no
    /// randomness in it, and `forecast == actual` stops being an identity. Asserted at the extremes
    /// too (`chance <= 0`), which is the mirror identity.
    ///
    /// **This is the sensitive half of the guard.** The end-to-end wire test
    /// (`hunt_forecast_range::the_degenerate_range_is_a_point_and_it_is_the_take_the_sim_pays`) reads
    /// the take *after* the whole-animal quantiser, which absorbs a small perturbation; here nothing
    /// rounds, so a single ulp of drift fails.
    #[test]
    fn a_certain_hit_chance_has_no_spread_to_quantile() {
        for attackers in [1.0_f32, 2.5, 40.0, 4000.0] {
            for sigmas in [-3.0_f32, -2.0, 0.0, 2.0, 3.0] {
                assert_eq!(
                    attacks_landed_at(attackers, DEFAULT_HIT_CHANCE, sigmas),
                    attackers,
                    "hit_chance 1.0 must land every attacker at every quantile"
                );
                assert_eq!(
                    attacks_landed_at(attackers, 0.0, sigmas),
                    0.0,
                    "hit_chance 0 must land nobody at every quantile"
                );
            }
        }
    }

    /// **The quantile IS the binomial the live draw samples** — same mean, and a band that widens
    /// with `sigmas`. Paired with the liveness half (`0` sigmas is the mean, and the mean is *not*
    /// the whole count), because a quantile function stuck at `attackers` would satisfy an ordering
    /// check alone.
    #[test]
    fn the_quantile_brackets_the_mean_of_the_draw_it_replaces() {
        const ATTACKERS: f32 = 100.0;
        const CHANCE: f32 = 0.25;
        let mean = attacks_landed_at(ATTACKERS, CHANCE, EXPECTED_STRIKES);
        assert!(
            (mean - ATTACKERS * CHANCE).abs() < 1e-4,
            "zero sigmas must be the binomial's mean: {mean}"
        );
        assert!(
            mean > 0.0 && mean < ATTACKERS,
            "liveness: a sub-1 chance must land some and not all: {mean}"
        );
        let narrow =
            attacks_landed_at(ATTACKERS, CHANCE, 1.0) - attacks_landed_at(ATTACKERS, CHANCE, -1.0);
        let wide =
            attacks_landed_at(ATTACKERS, CHANCE, 3.0) - attacks_landed_at(ATTACKERS, CHANCE, -3.0);
        assert!(
            wide > narrow && narrow > 0.0,
            "the band must widen with sigmas: 1σ {narrow} vs 3σ {wide}"
        );
        // ...and it can never leave the support, however many sigmas are asked for.
        assert_eq!(attacks_landed_at(ATTACKERS, CHANCE, -99.0), 0.0);
        assert_eq!(attacks_landed_at(ATTACKERS, CHANCE, 99.0), ATTACKERS);
    }

    /// **A live fight still DRAWS, and a forecast still does not** — the two `StrikeDraw` modes are
    /// observably different, which is the property the whole seam exists for. At a sub-1 chance the
    /// seeded mode varies with its seed while the quantile mode is seed-blind.
    #[test]
    fn the_seeded_mode_draws_and_the_quantile_mode_does_not() {
        const ATTACKERS: f32 = 60.0;
        let live = CombatTuning {
            hit_chance: 0.5,
            ..CombatTuning::default()
        };
        let forecast = CombatTuning {
            draw: StrikeDraw::Quantile {
                sigmas: EXPECTED_STRIKES,
            },
            ..live
        };
        let seeded: Vec<f32> = (0..8)
            .map(|seed| landed_strikes_seeded(ATTACKERS, &live, seed))
            .collect();
        assert!(
            seeded.iter().any(|landed| *landed != seeded[0]),
            "a live fight must actually draw: {seeded:?}"
        );
        let quantiled: Vec<f32> = (0..8)
            .map(|seed| landed_strikes_seeded(ATTACKERS, &forecast, seed))
            .collect();
        assert!(
            quantiled.iter().all(|landed| *landed == quantiled[0]),
            "a forecast must make no draw at all: {quantiled:?}"
        );
    }

    /// The durability every fixture below leaves at the neutral `1.0` unless it is the thing under
    /// test — one damage brings one unit down, so a fixture's arithmetic reads straight off `attack −
    /// defense`.
    const UNIT_DURABILITY: f32 = 1.0;

    /// The shipped `person` row's `combat.durability` (`creatures.json`) and the mammoth's
    /// (`fauna_config.json`) — used where the fixture needs bodies that do **not** clamp to headcount
    /// on the first blow, so an assertion measures the model rather than the clamp.
    const PERSON_DURABILITY: f32 = 20.0;
    const MEGAFAUNA_DURABILITY: f32 = 500.0;

    fn person(count: f32, attack: f32, defense: f32) -> Contingent {
        soldier("person", count, attack, defense, UNIT_DURABILITY)
    }

    fn beast(count: f32, attack: f32, defense: f32) -> Contingent {
        soldier("beast", count, attack, defense, UNIT_DURABILITY)
    }

    fn soldier(kind: &str, count: f32, attack: f32, defense: f32, durability: f32) -> Contingent {
        Contingent {
            kind: ContingentId::from(kind),
            count,
            profile: CombatStats {
                attack,
                defense,
                durability,
                range: RangeBand::Melee,
                wariness: 0.0,
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
        // Attack 2 against defense 1 — both sides clear the gate by exactly one point, so each deals
        // `5 × 1 = 5` damage into 1-durability units and the whole five-strong side goes down.
        let out = resolve_fight(
            &payload(vec![person(5.0, 2.0, 1.0)], vec![person(5.0, 2.0, 1.0)]),
            &CombatTuning::default(),
        );
        assert_eq!(out.victor, None);
        let a = side_result(&out, ForceId(1));
        let b = side_result(&out, ForceId(2));
        assert!((a.killed - b.killed).abs() < 1e-4);
        assert!((a.wounded - b.wounded).abs() < 1e-4);
        assert!((a.killed + a.wounded - 5.0).abs() < 1e-4);
    }

    /// **Nobody is hurt by an enemy that cannot beat their defense**, at any headcount — the gate
    /// (`docs/plan_hunt_through_combat.md` §0.2), asserted here at the resolver rather than only
    /// through the hunt adapter.
    #[test]
    fn an_evenly_matched_attack_and_defense_draws_no_blood() {
        let out = resolve_fight(
            &payload(vec![person(800.0, 1.0, 1.0)], vec![beast(1.0, 1.0, 1.0)]),
            &CombatTuning::default(),
        );
        for result in &out.results {
            assert_eq!(result.killed, 0.0, "{:?} should be untouched", result.kind);
            assert_eq!(result.wounded, 0.0, "{:?} should be untouched", result.kind);
        }
        // Power still decides the victor — the gate is about damage, not about who is winning.
        assert_eq!(out.victor, Some(ForceId(1)));
    }

    #[test]
    fn a_five_to_one_mismatch_crushes_the_weak_side() {
        let out = resolve_fight(
            &payload(vec![person(5.0, 2.0, 1.0)], vec![person(1.0, 2.0, 1.0)]),
            &CombatTuning::default(),
        );
        assert_eq!(out.victor, Some(ForceId(1)));
        // Weak side takes 5 damage into its single 1-durability unit → annihilated.
        let weak = side_result(&out, ForceId(2));
        assert!((weak.killed + weak.wounded - 1.0).abs() < 1e-4);
        // **Compared as a SHARE of headcount, not as an absolute** — and that is the model, not a
        // weakened assertion. Damage is a quantity the enemy *deals*: the strong side takes the one
        // unit of damage its lone opponent can deal whatever its own size, so its absolute loss is
        // flat and its *share* is what a bigger party buys. Reading these as absolutes would assert
        // that a crowd makes the enemy swing less, which is exactly what it does not do.
        let strong = side_result(&out, ForceId(1));
        let strong_share = (strong.killed + strong.wounded) / 5.0;
        let weak_share = weak.killed + weak.wounded; // headcount 1
        assert!(
            strong_share < weak_share,
            "strong side lost {strong_share} of itself, weak side {weak_share}"
        );
        // The loser lost its whole headcount (> 0.5) → driven off.
        assert!(out.disengaged);
    }

    /// **Excess damage spills to the next unit, exactly** — the property that makes "many small
    /// animals per turn" fall out of the weapon instead of being authored (§4.2). One attacker doing
    /// 20 against 2-durability quarry brings down ten, and doubling its attack doubles that.
    #[test]
    fn excess_damage_spills_to_the_next_unit_in_the_contingent() {
        let tuning = CombatTuning::default();
        let quarry = || soldier("quarry", 40.0, 0.0, 0.0, 2.0);
        let spear = resolve_fight(
            &payload(vec![person(1.0, 20.0, 1.0)], vec![quarry()]),
            &tuning,
        );
        assert!((side_result(&spear, ForceId(2)).killed - 10.0).abs() < 1e-4);
        let better = resolve_fight(
            &payload(vec![person(1.0, 40.0, 1.0)], vec![quarry()]),
            &tuning,
        );
        assert!((side_result(&better, ForceId(2)).killed - 20.0).abs() < 1e-4);
    }

    /// **The spill stops at the contingent's headcount** — twenty damage cannot bring down more
    /// quarry than is standing there.
    #[test]
    fn spillover_never_exceeds_the_units_present() {
        let out = resolve_fight(
            &payload(
                vec![person(1.0, 20.0, 1.0)],
                vec![soldier("quarry", 3.0, 0.0, 0.0, 2.0)],
            ),
            &CombatTuning::default(),
        );
        let quarry = side_result(&out, ForceId(2));
        assert!((quarry.killed + quarry.wounded - 3.0).abs() < 1e-4);
    }

    /// **The shipped `hit_chance` of `1.0` consumes no randomness** — the seed cannot change the
    /// outcome, which is what keeps `forecast == actual` an exact identity and the forecast's
    /// reported range a point.
    #[test]
    fn the_shipped_hit_chance_is_an_exact_identity_across_seeds() {
        let tuning = CombatTuning::default();
        assert_eq!(tuning.hit_chance, DEFAULT_HIT_CHANCE);
        let baseline = resolve_fight(
            &payload(vec![person(9.0, 20.0, 1.0)], vec![beast(4.0, 6.0, 2.0)]),
            &tuning,
        );
        for seed in [1_u64, 7, 9_999, u64::MAX] {
            let mut p = payload(vec![person(9.0, 20.0, 1.0)], vec![beast(4.0, 6.0, 2.0)]);
            p.seed = seed;
            assert_eq!(resolve_fight(&p, &tuning), baseline);
        }
    }

    /// **Variance is binomial in force size** (§4.7): three attackers are a gamble, thirty are
    /// reliable. Asserted as a spread across many seeds — paired with a liveness assertion that the
    /// small force's outcome moves at all, because a dead draw would "pass" a spread check.
    #[test]
    fn variance_shrinks_as_the_force_grows() {
        let tuning = CombatTuning {
            hit_chance: 0.5,
            ..CombatTuning::default()
        };
        let spread = |attackers: f32| {
            let mut seen: Vec<f32> = Vec::new();
            for seed in 0..200_u64 {
                // Attack 1 against defense 0 → exactly one damage per landed strike, into
                // 1-durability quarry: one unit down per hit, so the per-attacker figure below IS the
                // realised hit rate. Quarry deep enough that the headcount clamp never binds.
                let mut p = payload(
                    vec![person(attackers, 1.0, 1.0)],
                    vec![soldier("quarry", 1_000.0, 0.0, 0.0, UNIT_DURABILITY)],
                );
                p.seed = seed;
                let out = resolve_fight(&p, &tuning);
                let down = side_result(&out, ForceId(2));
                seen.push((down.killed + down.wounded) / attackers);
            }
            let mean = seen.iter().sum::<f32>() / seen.len() as f32;
            let var = seen.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / seen.len() as f32;
            (mean, var.sqrt())
        };
        let (small_mean, small_sd) = spread(3.0);
        let (large_mean, large_sd) = spread(300.0);
        // Liveness: the draw is real, and it is centred on the hit chance.
        assert!(small_sd > 0.0, "a hit chance below 1 must actually roll");
        assert!((small_mean - 0.5).abs() < 0.1, "mean was {small_mean}");
        assert!((large_mean - 0.5).abs() < 0.02, "mean was {large_mean}");
        // The property: per-attacker variance falls as the force grows.
        assert!(
            large_sd < small_sd * 0.5,
            "large force sd {large_sd} should be far below the small force's {small_sd}"
        );
    }

    /// **A sub-gate force rolls nothing, however many dice it would otherwise throw** (§4.7): the
    /// probability below the gate is *exactly* zero, not merely small, so no seed produces a casualty.
    #[test]
    fn no_seed_lets_a_sub_gate_force_through() {
        let tuning = CombatTuning {
            hit_chance: 0.5,
            ..CombatTuning::default()
        };
        for seed in 0..500_u64 {
            let mut p = payload(
                vec![person(800.0, 1.0, 1.0)],
                vec![soldier("mammoth", 1.0, 0.0, 12.0, 500.0)],
            );
            p.seed = seed;
            let out = resolve_fight(&p, &tuning);
            let quarry = side_result(&out, ForceId(2));
            assert_eq!(
                quarry.killed + quarry.wounded,
                0.0,
                "seed {seed} broke the gate"
            );
        }
    }

    /// **A bigger party still buries fewer of its own** — but it buys that through the kill/wound
    /// *split*, not by making the beast swing less. The bodies here carry the roster's real
    /// `durability`, deliberately: at the fixture's neutral `1.0` the beast's damage exceeds a lone
    /// human entirely, the clamp-to-headcount binds, and the comparison measures the clamp instead of
    /// the property.
    #[test]
    fn adding_defenders_reduces_the_strong_sides_own_casualties() {
        let tuning = CombatTuning::default();
        let human = |count: f32| soldier("person", count, 1.0, 1.0, PERSON_DURABILITY);
        let dangerous_beast = || soldier("beast", 1.0, 8.0, 12.0, MEGAFAUNA_DURABILITY);
        // A lone human vs a dangerous beast (attack 8, defense 12).
        let solo = resolve_fight(&payload(vec![human(1.0)], vec![dangerous_beast()]), &tuning);
        // The same beast, but the party is reinforced with warriors.
        let reinforced =
            resolve_fight(&payload(vec![human(6.0)], vec![dangerous_beast()]), &tuning);
        let solo_people = side_result(&solo, ForceId(1));
        let reinforced_people = side_result(&reinforced, ForceId(1));
        // The beast deals the SAME total damage either way — it is one animal — so the same number of
        // people go down...
        assert!(
            ((solo_people.killed + solo_people.wounded)
                - (reinforced_people.killed + reinforced_people.wounded))
                .abs()
                < 1e-4,
            "one beast deals one beast's damage, whatever the party size"
        );
        // ...but FEWER of them are buried, because the blow is thinned across more defenders.
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

    /// **Damage banks toward the next body and hands back only WHOLE units** — the ledger's core
    /// contract (`docs/plan_hunt_through_combat.md` §4.2). Three sub-threshold turns bring nothing
    /// down and the fourth lands the kill, which a stateless `units_brought_down` never would.
    #[test]
    fn a_ledger_turns_sub_threshold_damage_into_a_kill() {
        let body = CombatStats {
            durability: MEGAFAUNA_DURABILITY,
            ..CombatStats::default()
        };
        /// A third of a body per turn — never enough on its own.
        const PER_TURN: f32 = MEGAFAUNA_DURABILITY / 3.0;
        let mut ledger = DamageLedger::default();
        assert_eq!(units_brought_down(PER_TURN, &body, 1.0).floor(), 0.0);

        assert_eq!(ledger.strike(PER_TURN, &body, 1.0), 0.0, "turn 1 banks");
        assert_eq!(ledger.strike(PER_TURN, &body, 1.0), 0.0, "turn 2 banks");
        assert!(ledger.pending() > 0.0, "the bank is real");
        assert_eq!(
            ledger.strike(PER_TURN, &body, 1.0),
            1.0,
            "the third turn completes the body"
        );
        assert_eq!(ledger.pending(), 0.0, "and the kill spends the bank");
    }

    /// **The bank never exceeds the bodies standing there.** A huge blow at a single animal does not
    /// bank a surplus to spend on whatever walks past next — the same clamp
    /// [`units_brought_down`] applies, so the two cannot disagree about a herd of one.
    #[test]
    fn a_ledger_never_banks_damage_the_bodies_could_not_soak() {
        let body = CombatStats {
            durability: MEGAFAUNA_DURABILITY,
            ..CombatStats::default()
        };
        let mut ledger = DamageLedger::default();
        let down = ledger.strike(MEGAFAUNA_DURABILITY * 50.0, &body, 1.0);
        assert_eq!(down, 1.0, "one animal standing, one animal down");
        assert_eq!(ledger.pending(), 0.0, "and nothing carried over");
    }

    /// **Banking zero forever is still zero** — the gate is untouched by the accumulator, which is
    /// the one property a cross-turn ledger could plausibly have broken (§0.2).
    #[test]
    fn a_ledger_cannot_accumulate_a_sub_gate_partys_nothing() {
        let mammoth = CombatStats {
            defense: 12.0,
            durability: MEGAFAUNA_DURABILITY,
            ..CombatStats::default()
        };
        let bare_hand = strike_damage(1.0, mammoth.defense);
        assert_eq!(bare_hand, 0.0, "the fixture must be below the gate");
        let mut ledger = DamageLedger::default();
        for _ in 0..1_000 {
            assert_eq!(ledger.strike(800.0 * bare_hand, &mammoth, 1.0), 0.0);
        }
        assert_eq!(ledger.pending(), 0.0);
        assert!(ledger.is_clean(), "and no contact was ever recorded");
    }

    /// **`resolve_fight` reports the damage it dealt**, which is what a caller banking across turns
    /// needs: `killed + wounded` has already been divided by `durability` and clamped, so it cannot
    /// be inverted back into a flow.
    #[test]
    fn a_result_carries_the_damage_that_landed_not_only_the_bodies() {
        let out = resolve_fight(
            &payload(
                vec![person(10.0, 20.0, 1.0)],
                vec![soldier("mammoth", 1.0, 0.0, 12.0, MEGAFAUNA_DURABILITY)],
            ),
            &CombatTuning::default(),
        );
        let quarry = side_result(&out, ForceId(2));
        // The resolver reports a FRACTION of a body, which is exactly the problem: a caller that
        // quantises to whole animals discards it, and the discarded part is what has to be banked.
        assert!(
            quarry.killed + quarry.wounded < 1.0,
            "no whole body goes down yet"
        );
        assert!(
            (quarry.damage_dealt - 10.0 * (20.0 - 12.0)).abs() < 1e-4,
            "…but the damage that landed is reported: {}",
            quarry.damage_dealt
        );
    }

    #[test]
    fn resolution_is_deterministic() {
        let p = payload(vec![person(4.0, 1.0, 2.0)], vec![beast(1.0, 8.0, 12.0)]);
        let a = resolve_fight(&p, &CombatTuning::default());
        let b = resolve_fight(&p, &CombatTuning::default());
        assert_eq!(a, b);
    }
}
