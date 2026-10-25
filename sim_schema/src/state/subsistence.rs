//! Subsistence-section state: herds, forage, graze, food modules, and sedentarization.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// **"No estimate"** — the wire value of `buildTurnsRemaining` on either web when the sim cannot name
/// a number: the source is at the top of its ladder, the next rung's gates refuse it, no crew is
/// working the source, or the crew's output is zero and a running build is **stalled**. **It is NOT
/// "nothing is being built here"** — an unstarted source with a crew on it publishes the projected
/// turns for the rung it would climb next. It sits outside the `>= 0` range a real estimate lives in,
/// so the two states cannot be confused — the same convention `next_x`/`next_y` use for "no heading".
///
/// It lives here, on the **wire**, because the sentinel is a fact about the published contract rather
/// than about the sim's arithmetic (`core_sim`'s `build_turns_remaining` answers an `Option<u32>`).
pub const NO_BUILD_TURNS_ESTIMATE: i32 = -1;

/// **"The meter holds exactly where it is"** — the wire value of `buildTurnsRemaining` for a **real,
/// staffed, priced** build banking exactly what its meter is bleeding, so nothing is gained and
/// nothing is lost.
///
/// **The term it is struck from is the ROT, not the maintenance rate**
/// (`docs/plan_standing_upkeep.md` §4.6a): a build crew supplies nothing toward the rate, which the
/// band's keeping pool owes for every meter carrying work at any fullness. What a build can fail to
/// out-run is the ground going backwards under it, and `meterRotPerTurn` publishes that term.
/// **That is an answer, not the absence of one**, and it is the one the player can act on: staff the
/// keeping, or add builders. It shipped folded into [`NO_BUILD_TURNS_ESTIMATE`] for one slice, which
/// rendered as *no line at all* on the two surfaces a player reads every turn — visible only to a
/// compose sheet that redid the comparison itself.
///
/// **IT IS NOT ONLY A FAILURE, and a reader must not render it as a warning.** With **no builders
/// and the keeping met** the balance is exactly zero — which is a player **parking** a half-built
/// improvement, held indefinitely at no risk. That is the state `docs/plan_standing_upkeep.md` §2.4
/// exists to make possible, and on the shipped ladder it is where this sentinel mostly lives: both
/// plant rot rates are below one worker-turn, so a *staffed* build always out-runs its own rot.
///
/// **It is NOT [`BUILD_METER_ROTS`], and the difference is what the player is being told.** Holding
/// costs nothing; rotting destroys work already paid for. `-3` is the unambiguously bad one, and a
/// reader that renders them alike is back to one sentinel for two facts.
///
/// **The no-answer boundary is WORK BANKED, not hands.** A meter at **zero** with nobody on it reads
/// [`NO_BUILD_TURNS_ESTIMATE`], and so does a build the rung's own gate refuses — nothing has been
/// promised there. A meter *carrying* work has promised something, whoever is or is not on it.
///
/// Sits outside the `>= 0` range a real estimate lives in, beside its siblings, so no reader has to
/// guess which negative it is looking at.
pub const BUILD_METER_HOLDS: i32 = -2;

/// **"The meter is going backwards"** — the wire value of `buildTurnsRemaining` for a **real,
/// staffed, priced** build whose net supply is **negative**: the crew is under the rung's
/// maintenance rate, so past its grace the decay pass bleeds work the player has already bought
/// (`docs/plan_standing_upkeep.md` §2.4).
///
/// **It is the third state split out of the old `-2`**, which said only *"this staffing never
/// finishes"* and therefore said the same thing about a crew that is merely treading water and one
/// that is losing the build. Neither finishes; only one of them is destroying progress, and that is
/// the news a player has to be able to see (the client renders this **red** against
/// [`BUILD_METER_HOLDS`]'s yellow and a real count's green).
///
/// The same three conditions are load-bearing as for its sibling: an **unstaffed** source, and one
/// whose knowledge / site / species gate refuses the build, both read [`NO_BUILD_TURNS_ESTIMATE`] —
/// they accrue nothing for a reason that has nothing to do with staffing.
///
/// Appended below its siblings (append-only wire; the two existing values keep their numbers).
pub const BUILD_METER_ROTS: i32 = -3;

/// **"THE QUEUE IS BLOCKED AT THIS ENTRY"** — the wire value of `buildTurnsRemaining` when the band's
/// builders are **staffed and standing on this build** and the rung's own gate refuses it, so nothing
/// banks and nothing behind it moves (`docs/plan_standing_upkeep.md` §4.6b).
///
/// **It is NOT [`NO_BUILD_TURNS_ESTIMATE`], and that is the whole reason it exists.** `-1` is the
/// *absence* of an answer and renders as **no line at all**, which is exactly the silence this state
/// must not be read as: the player has committed a pool, the pool is producing nothing, and the game
/// is saying so.
///
/// **THE REMEDY IS OFF THE BUILD LINE ENTIRELY.** The measured case is a half-tamed herd with an
/// empty keeping: the flock stands on its floor, the unmet keeping suppresses
/// its regrowth, and the `Tame`'s own escapement gate never reopens. Adding builders does nothing.
/// A surface showing this must therefore pair it with the source's own `upkeepShortfall` /
/// `neglectGraceRemaining`, because *"staff the keeping"* is the sentence — on the animal web,
/// raising the herd's own hunt row (`docs/plan_site_crews.md` §2.1).
///
/// **Every entry BEHIND a blocked head publishes it too**, since nothing below a head that never
/// finishes finishes either. A *waiting* entry whose own gate refuses publishes the honest
/// [`NO_BUILD_TURNS_ESTIMATE`] instead: it may well be eligible by the time it reaches the head.
///
/// Appended below its siblings (append-only wire; the three existing values keep their numbers).
pub const BUILD_QUEUE_BLOCKED: i32 = -4;

/// **"THIS BUILD HAS NOT HAD A TURN YET"** — the wire value of `buildTurnsRemaining` for an entry the
/// player has queued since the last turn resolved, so **no estimate pass has ever run for it**.
///
/// **It is NOT [`NO_BUILD_TURNS_ESTIMATE`], and that is the whole reason it exists.** `-1` means the
/// sim looked and had no number: nobody is working the source, its gate refuses, or a running build
/// banked nothing and is genuinely **stalled**. This one means the sim has not looked. A build the
/// player queued a second ago is not a hazard, and folding the two together is what put
/// `⚠ Stalled 0%` on a freshly declared `Cultivate` with two builders standing on it — a warning
/// that cleared itself on the next turn, which is the worst shape a warning can have.
///
/// **The distinguishing fact is the ESTIMATE PASS, never the meter.** A genuinely stalled build also
/// sits at `0%`, so progress cannot tell them apart. What can is that `publish_build_chain` stamps
/// every entry it walks with its 0-based place in the line, and the Logistics decay passes clear that
/// place back to [`NOT_IN_ANY_BUILD_QUEUE`] every turn — so a source that is in a band's **live**
/// queue and still carries the cleared place is one no pass has reached since it was queued.
///
/// **Both webs**: a patch and a herd share `publish_build_chain` and the same per-turn reset, so a
/// fresh `Tame` or `Corral` reads this exactly as a fresh `Cultivate` does.
///
/// A reader should render it as **"queued — starts next turn"**, never as a warning, and never as the
/// silence `-1` earns. Appended below its siblings (append-only wire; the four existing values keep
/// their numbers).
pub const BUILD_NOT_YET_ESTIMATED: i32 = -5;

/// **"THIS SOURCE IS IN NO BAND'S BUILD QUEUE"** — the neutral of `buildQueuePosition`, whose real
/// values are **0-based** places in the winning band's queue.
///
/// It shares `-1` with [`NO_BUILD_TURNS_ESTIMATE`] by the same convention rather than by
/// coincidence: both say *"outside the range a real answer lives in"*, on two fields that are read
/// together.
pub const NOT_IN_ANY_BUILD_QUEUE: i32 = -1;

/// **"THIS SOURCE IS HEADING NOWHERE"** — the wire value of `buildDestinationCapacity` on a source
/// **no band has queued**, on the same *"outside the range a real answer lives in"* convention as
/// the three sentinels above.
///
/// **It cannot be `0`, and that is the whole reason it is a sentinel.** A carrying capacity of zero
/// is a **real reading a real source has** — barren ground, an overgrazed range, a rock pen — so a
/// zero here would say *"build this and it will hold nothing"* about every unqueued source on the
/// map. This is the ambiguity `huntUsefulWorkers` had to be untangled from in the other direction,
/// and the resolution is the same one: a value that means "no answer" must live where no answer can.
///
/// A capacity is never negative, so any `< 0` reading is this. A client renders **no destination
/// line at all** for it, exactly as it renders none for [`NO_BUILD_TURNS_ESTIMATE`].
pub const NO_BUILD_DESTINATION_CAPACITY: f32 = -1.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SedentarizationState {
    pub faction: u32,
    pub score: f32,
    #[serde(default)]
    pub stage: String,
    /// The belief input's raw points this turn (`100 × weights.belief × belief_norm`), before the
    /// EMA that smooths `score`. Appended last.
    #[serde(default)]
    pub belief_points: f32,
    /// The domestication input's raw points this turn, before the EMA. Appended last.
    #[serde(default)]
    pub domestication_points: f32,
    /// The surplus (larder) input's raw points this turn, before the EMA.
    #[serde(default)]
    pub surplus_points: f32,
    /// The land (resource density) input's raw points this turn, before the EMA.
    #[serde(default)]
    pub land_points: f32,
    /// The population input's raw points this turn, before the EMA.
    #[serde(default)]
    pub population_points: f32,
}

/// One faction's mobility score (#800) — the roaming counterpart of [`SedentarizationState`].
/// `skills_points` / `distance_points` are the two inputs' raw points this turn (`100 × weight ×
/// norm`), before the EMA that smooths `score`. No stage: mobility has no prompt thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MobilityState {
    pub faction: u32,
    pub score: f32,
    #[serde(default)]
    pub skills_points: f32,
    #[serde(default)]
    pub distance_points: f32,
}

/// A fully-fed pen — the neutral value of [`HerdTelemetryState::pen_fed_fraction`], so an un-penned
/// (or older-snapshot) herd never reads as starving.
fn pen_fully_fed() -> f32 {
    1.0
}

/// A fully-staffed herd — the neutral value of [`HerdTelemetryState::herded_fraction`], so an
/// unmanaged (or older-snapshot) herd never reads as under-herded.
fn fully_herded() -> f32 {
    1.0
}

/// **The neutral value of every retreat/hazard multiplier on this wire** —
/// [`HerdTelemetryState::stay_fraction`] and [`KitOptionState`]'s `dispersion` / `exposure`.
///
/// **It exists because `0` is the WRONG neutral for all three, and it is wrong in the reassuring
/// direction on two of them.** `stay_fraction 0` says every animal bolts before contact (a take of
/// nothing); `dispersion 0` says the party scares nothing, and `exposure 0` says nobody can be hurt
/// — i.e. a field that failed to arrive would silently hand every kit the passive device's whole
/// advantage. The FlatBuffers schema declares all three `= 1`, so a derived `Default` is the one
/// place the two representations of the same field could disagree.
///
/// **`attack_min_body_mass` / `attack_max_body_mass` deliberately keep the bare `#[serde(default)]`**:
/// `0` is their *sentinel* for "unbounded", it matches their schema default, and it is what every
/// weapon but the passive device ships.
fn multiplier_neutral() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HerdTelemetryState {
    pub id: String,
    pub label: String,
    pub species: String,
    pub x: u32,
    pub y: u32,
    pub biomass: f32,
    pub route_length: u32,
    pub next_x: i32,
    pub next_y: i32,
    pub size_class: String,
    pub huntable: bool,
    #[serde(default)]
    pub ecology_phase: String,
    /// Pre-commit yield forecast at the herd's current biomass (food/turn, `output_multiplier =
    /// 1`). The per-worker **rate**; the escapement ceiling that clamps it is composed at the
    /// crew's floor from [`Self::provisions_per_biomass`] and [`Self::carrying_capacity`], the
    /// per-policy ceiling rows having retired with the four harvest stances.
    #[serde(default)]
    pub per_worker_yield: f32,
    // **RETIRED: `corral_trade`** (arc #527), and off the wire.
    // **RETIRED: `pen_upkeep`** — `pen.upkeep_per_biomass × biomass`, the FOOD a pen drew from its
    // keeper's larder, quoted for every herd so a pre-commit `Corral` row could subtract the running
    // cost from the payoff. **Human food is not animal feed**: a pen eats the grass its fenced
    // footprint grows and the hay its keeper carries in, so there is no food-unit running cost left to
    // quote, and `penUpkeep` is off the wire. What a pen still demands is
    // fodder, and [`Self::pen_fed_fraction`] says whether it got it.
    /// The fraction of its **fodder** demand this pen's grass and hay actually covered last turn —
    /// `(footprint_intake + fodder_draw) / (fodder_per_biomass × biomass)`. `1.0` = fully fed (also
    /// the value for a herd that is not penned); `< 1` = **starving** — the
    /// herd is shrinking by `pen.starve_shrink_rate × (1 − this) × biomass` per turn, and its yield
    /// with it. It recovers when fed again (it never despawns and never loses the pen).
    #[serde(default = "pen_fully_fed")]
    pub pen_fed_fraction: f32,
    /// **The herd's current derived carrying capacity K** (`Herd::carrying_capacity`), recomputed each
    /// turn from the graze its range yields (Grazing Phase 2b-ii). For a mobile herd this is the
    /// ecological K; for a penned herd it is the pen-time frozen value. With `biomass` the client shows
    /// "caps at ~K on this range" and flags overgrazing as `biomass > carrying_capacity`. Derived at
    /// capture. Appended (append-only wire).
    #[serde(default)]
    pub carrying_capacity: f32,
    /// The hex radius of the herd's grazing range (`Herd::graze_range_radius`: small game `0`, big game
    /// `1`, migratory `loiter_radius`) — the exact ring the sim grazes/derives K over. Exported as the
    /// radius the sim uses (not from `size_class`, since migratory depends on `loiter_radius`, absent
    /// from the wire) so the client reproduces it with `hex_range_tiles`. Derived at capture. Appended
    /// last.
    #[serde(default)]
    pub graze_range_radius: u32,
    /// **The pen's fenced-footprint radius** (Grazing 2d) — `0` = the single corralled tile; each ring
    /// the `ExtendPen` command works off raises it. `0` for an unpenned herd. Appended (append-only).
    #[serde(default)]
    pub pen_radius: u32,
    /// **The count of in-bounds fenced tiles** in the pen's footprint — server-computed
    /// (`hex_range_tiles(corralled_at, pen_radius)` length), NOT the closed-form disk count `1,7,19,…`
    /// (which is wrong at map edges). `0` for an unpenned herd. Appended (append-only).
    #[serde(default)]
    pub pen_footprint_tiles: u32,
    /// **The share of a penned herd's feed its footprint covered** (`pasture_fraction`, Grazing 2d
    /// §2.3): `1.0` = the fenced pasture feeds it for free, `0.0` = a barren footprint lives entirely
    /// on hay. With [`Self::fodder_draw`] it is the whole feed split — grass and hay, one unit, one
    /// demand — and whatever the two leave uncovered is what [`Self::pen_fed_fraction`] falls short
    /// by. `0.0` for an unpenned herd. Appended (append-only).
    #[serde(default)]
    pub pen_pasture_fraction: f32,
    /// **The in-flight `ExtendPen` ring's build meter, in WORK UNITS** — the same unit-costed meter
    /// `Self::rungQuotes[animal:pastoral].workDone` / `Self::rungQuotes[animal:pen].workDone` carry, **not** a `0..1` fraction. It
    /// completes at [`Self::pen_extend_cost`] (→ `pen_radius` grows by one and both reset to `0.0`),
    /// so a "Fencing N%" badge is `pen_extend_progress / pen_extend_cost` with the zero denominator
    /// guarded. `0.0` when the pen is not extending. Appended (append-only).
    #[serde(default)]
    pub pen_extend_progress: f32,
    /// **How far up the husbandry ladder this species climbs** (Grazing 2d-δ): `wild` | `pastoral` |
    /// `pen`. The client hides the corral/extend affordance on a non-`pen` herd and the whole
    /// domestication track on a `wild` one. A free-form string like `species` (empty → `pen`, the full
    /// ladder). Appended last (append-only).
    #[serde(default)]
    pub husbandry_ceiling: String,
    /// **Biomass of one animal of this species** (`Herd::body_mass`, slice 8b). The client turns a
    /// per-turn biomass/food **rate** into a kill-**rhythm** with it: a hunt take is whole animals, so
    /// a herd whose MSY is lighter than one body pays a kill every `body_mass / rate` turns. Render
    /// "~1 animal / N turns" from `sustainable_yield` (or a `hunt_policy_ceilings` row) ÷ this — **not**
    /// the raw per-turn `actual_yield`, which is `0` on the wait turns of the pulse. Appended last
    /// (append-only). `0` if unknown.
    #[serde(default)]
    pub body_mass: f32,
    /// **One whole animal's worth of yield, in provisions** (`SourceYieldForecast::body_mass_yield` =
    /// `body_mass × provisions_per_biomass`, the same conversion every other yield field uses). The
    /// client's kill-rhythm is `food_per_animal / sustainable_yield` — both provisions, dimensionally
    /// clean — and it doubles as a "a mammoth is ~16 food" display. Appended last (append-only). `0` if
    /// unknown.
    #[serde(default)]
    pub food_per_animal: f32,
    // **RETIRED: `trade_per_animal`** (arc #527), and off the wire. A kill rhythm on an inedible species divides body mass by the herd's own biomass
    // terms, never by a currency that may be zero.
    /// **How many keepers a flock of this species and this size WANTS** (`fauna::herd_herders_needed`
    /// = `ceil((biomass / body_mass) / animals_per_herder)`) to hold its tameness. `0` for a
    /// wild/unmanaged herd (nobody to staff). The client pairs it with [`Self::herded_fraction`] for an
    /// honest "herders 1 / 6" readout the labor assignment's blended `workers_needed`
    /// (`max(herders_needed, haulers)`) cannot give. Appended last (append-only). `0` if unknown.
    ///
    /// **⛔ IT IS THE HEAD-COUNT REQUIREMENT, NOT [`Self::upkeep_workers_needed`].** This one is
    /// position-**independent** — it does not slide while a `Tame` fills — because
    /// [`Self::herders_needed_if_managed`] must quote a crew for a herd at position zero, and because
    /// `herdersNeededIfManaged == herdersNeeded` is pinned on every managed herd. The hands the herd's
    /// *bill* takes is `upkeepWorkersNeeded`, which is the `ceil` of [`Self::upkeep_demand`] and does
    /// move with the position. The two agree at the top of a rung and diverge below it.
    #[serde(default)]
    pub herders_needed: u32,
    /// **How well the herd is staffed** — `min(1, assigned / herders_needed)` (`Herd::herded_fraction`).
    /// `1.0` = fully staffed (and the value for a herd that needs nobody); `< 1` = under-herded, so
    /// `domestication` bleeds proportionally and the herd risks reclassifying as wild. Appended last
    /// (append-only).
    #[serde(default = "fully_herded")]
    pub herded_fraction: f32,
    // **RETIRED: `pastoral_trade`** (arc #527), and off the wire.
    /// The hay this pen drew from its keeper band's FODDER store last turn (Flora Roster F3), in
    /// fodder units. `0` for an unpenned herd, a keeper that has not learned Foddering, or a pen its
    /// own footprint already fed. With [`Self::pen_pasture_fraction`] it is the whole feed split —
    /// "fed by pasture NN% · hay X.X" — both in fodder units against one demand. Appended last
    /// (append-only).
    #[serde(default)]
    pub fodder_draw: f32,
    // **RETIRED: `pen_hay_need`** — the hay a pen was short per turn, `max(0, fodder_per_biomass ×
    // biomass − footprint_intake)`. Nothing read it: a pen row states how much MORE the pen needs,
    // which is [`Self::pen_fodder_shortfall`] below, and that is struck sim-side from the same
    // quantity. The sim still computes it (it is what the band-level `fodder_need` roll-up sums), but
    // it is not a per-pen wire field.
    /// **How much more fodder this pen needs per turn** — `max(0, hay need − fodder_draw)`, in fodder
    /// units, where the hay need is the gap the pen's own fenced footprint leaves. What the land did
    /// not grow *and* the keeper did not carry in, so a pen row reads *"40% pasture · 7% fodder ·
    /// needs 11.3 more/turn"* with no client-side subtraction.
    ///
    /// **The sim owns the arithmetic, and this is the only term published.** The gap it is taken from
    /// is not on the wire: it is stamped on the same pass as [`Self::fodder_draw`] and so is this,
    /// which is what makes it impossible for the difference to describe a different turn from its
    /// terms.
    ///
    /// **Not gated on Foddering**, unlike [`Self::fodder_draw`]: a band that cannot draw hay draws
    /// nothing, so its shortfall is its *whole* need — the case the readout is most for.
    ///
    /// **Clamped at zero**; `0` for an unpenned herd and for a pen its own land feeds. Appended last
    /// (append-only).
    #[serde(default)]
    pub pen_fodder_shortfall: f32,
    // **RETIRED: `pen_larder_bill` and `pen_hay_food`** — the FOOD-unit third term of the old feed
    // split (`pen_upkeep × pen_pasture_fraction + pen_hay_food + pen_larder_bill == pen_upkeep`) and
    // the conversion that restated hay in the units the *people* eat in so it could share that row.
    // Both die with the larder feed: the split is grass and hay, in fodder. Neither is on the wire.
    /// **The raw combat components of this herd's species** (Predators Phase 0, `docs/plan_predators.md`),
    /// so the client can DERIVE danger itself — it is never stored server-side, because strength ≠
    /// danger (hunt-danger ≈ `attack × ferocity`, camp-threat ≈ `attack × aggression`). `attack` /
    /// [`Self::defense`] are STRENGTH (open-ended, human = 1); [`Self::ferocity`] / [`Self::aggression`]
    /// are BEHAVIOUR probabilities (0..1). All `0` on a harmless animal. Appended last (append-only).
    #[serde(default)]
    pub attack: f32,
    /// STRENGTH — how hard the animal is to bring down. See [`Self::attack`].
    #[serde(default)]
    pub defense: f32,
    /// BEHAVIOUR — P(fights back when hunted, vs flees); scales hunt-danger. See [`Self::attack`].
    #[serde(default)]
    pub ferocity: f32,
    /// BEHAVIOUR — P(initiates a raid unprovoked); scales camp-threat. See [`Self::attack`].
    #[serde(default)]
    pub aggression: f32,
    /// **The herd's prey-SENSING radius in hexes** (Predators Phase 1a) — `fauna.predators.prey_sense_radius`
    /// when this herd's species is a CARNIVORE, else `0`. So `> 0` is BOTH the client's "this is a
    /// predator" signal AND its view-ring radius: a carnivore eats other herds, not the tile's graze, so
    /// its graze-range ring is meaningless and the client draws a prey-sense "view" ring of this radius
    /// instead. A herbivore reads `0` and the client keeps drawing its [`Self::graze_range_radius`] ring.
    /// Appended last (append-only).
    #[serde(default)]
    pub prey_sense_radius: u32,
    /// **The crew this herd WOULD owe if it were managed** (fauna neglect-escape, taming-startup-lag
    /// fix) — `fauna::herders_needed(biomass, body_mass, animals_per_herder)` for a tameable species,
    /// else `0` (a `wild`-ceiling mammoth/deer never tames). Ownership-**independent**, unlike
    /// [`Self::herders_needed`] (0 for a wild herd), so the client can floor the Tame-compose worker cap
    /// at it the turn taming starts — before ownership is set in the Population stage — killing the
    /// one-turn lag. Equals `herders_needed` for a herd already managed. Appended last (append-only).
    #[serde(default)]
    pub herders_needed_if_managed: u32,
    // **RETIRED: `tame_build_fraction` / `corral_build_fraction`** — the two animal rungs'
    // `yield_fraction_while_building`. The dip dissolved into the crew's one work budget
    // (`docs/plan_standing_upkeep.md` §2.2): a crew preparing spends its turn on the meter and takes
    // **nothing**, so `preparing(stance, rung)` is `0` from the model rather than from a published
    // factor. Neither `tameBuildFraction` nor `corralBuildFraction` is on the wire.
    // **RETIRED: `maintain`** — a per-source boolean toggle. *"Stop maintaining this"* is
    // `maintain <faction> hunt <herd> 0`: a flag beside a crew count would be a second way to say
    // what the number already says, and the two could disagree.
    /// **What holding this herd's rung DEMANDS this turn**, in work units. Follows `rungQuotes[animal:pen].foodYield`'s
    /// rule — **always meaningful, never a sentinel**: a rung that declares no upkeep reads an honest
    /// `0`, which is every shipped rung today.
    #[serde(default)]
    pub upkeep_demand: f32,
    /// **What the crew actually paid toward it** out of this turn's work budget.
    #[serde(default)]
    pub upkeep_supplied: f32,
    /// **What went unmet** — and therefore exactly what the improvement decays by, once the shortfall
    /// outlasts the rung's own grace (`docs/plan_standing_upkeep.md` §2.4). Published rather than
    /// left as `demand − supplied` because the sim answers and the client does zero arithmetic.
    #[serde(default)]
    pub upkeep_shortfall: f32,
    /// **HANDS TO MEET THE DEMAND** — `ceil(upkeep_demand / PER_WORKER_OUTPUT)`
    /// (`fauna::herd_upkeep_workers_needed`); the plant twin's doc has the reasoning. **Not**
    /// [`Self::herders_needed`], which answers the head-count question at the rung's bare rate and
    /// therefore contradicted this identity for most of a `Tame`.
    #[serde(default)]
    pub upkeep_workers_needed: u32,
    /// **Whole workers the keeping takes once the rung in flight is finished** — equal to
    /// `upkeep_workers_needed` where no build is in flight (`docs/plan_site_crews.md`).
    #[serde(default)]
    pub upkeep_workers_at_completion: u32,
    /// **The game turn the keeping first needs one more whole worker than now**, projected along
    /// the build's own pace; [`NO_NEXT_KEEPING_WORKER`] = never within the build.
    #[serde(default = "no_next_keeping_worker")]
    pub upkeep_next_worker_turn: i32,
    /// **WHICH KEEPING TOOLS THIS SITE IS SHORT OF, BY NAME** — one
    /// [`crate::state::population::KitToeLineState`] per tool the site's keeping claimed this turn,
    /// summed over the crews keeping it. Short is `filled < required`, and
    /// `upkeep_tools_short` is exactly *"some line is short"*. Empty = the keeping claimed no tool.
    /// Appended last (append-only).
    #[serde(default)]
    pub upkeep_toe: Vec<crate::state::population::KitToeLineState>,
    /// **The builders on this source's declared build this turn**, the bare-handed and the kitted
    /// together (`docs/plan_labor_at_scale.md`, "Builds join the site's crew"). The viewer's own
    /// bands only. Appended last (append-only).
    #[serde(default)]
    pub builders_staffed: u32,
    /// Of [`Self::builders_staffed`], the ones holding the build kit the rung uses (every builder
    /// when the rung asks for no tool).
    #[serde(default)]
    pub builders_kitted: u32,
    /// **The most builders the build could take this turn** — the rung's `maxBuilders` cut by the
    /// room the tile left. `0` where no build is declared.
    #[serde(default)]
    pub builders_cap: u32,
    /// **The tools the build's builders asked for and what the settlement gave them**, one
    /// [`crate::state::population::PoolToeLineState`] per tool (`pool` reads `"build"`). Empty where
    /// the rung asks for no tool.
    #[serde(default)]
    pub build_toe: Vec<crate::state::population::PoolToeLineState>,
    /// **Is there anything here to neglect?** `false` for a **wild** herd — nobody's to keep, so it
    /// never sheds and [`Self::neglect_grace_remaining`] means nothing. Read this first, exactly as
    /// [`ForagePatchState::owner`]'s `has_owner` companion is read first.
    #[serde(default)]
    pub has_neglect_grace: bool,
    /// **Turns of neglect this herd can still absorb before animals start leaving** — the countdown,
    /// not the counter, so no client subtracts anything: `0` = the shed is running *now*, `N > 0` =
    /// it starts in N more turns of too-few-keepers. A properly herded herd reads its rung's full
    /// grace + 1 ("let them go and you have this long"). The animal twin of
    /// [`ForagePatchState::neglect_grace_remaining`].
    #[serde(default)]
    pub neglect_grace_remaining: u32,
    /// **What ONE UNIT of this herd's biomass is worth**, in each account — the species' own
    /// `HuntYield`. The animal twin of `ForagePatchState::provisions_per_biomass`; an **inedible**
    /// species honestly reads `0` here, and what it is really worth is material batches this table
    /// cannot state. Appended (append-only).
    #[serde(default)]
    pub provisions_per_biomass: f32,
    /// No animal pays fodder, so this is `0` on every herd — present so both food webs publish the
    /// same pair and a reader needs one code path. Appended (append-only).
    #[serde(default)]
    pub fodder_per_biomass: f32,
    // **RETIRED: `trade_per_biomass`** (arc #527), and off the wire.
    /// **What ONE hunter moves this turn, in BIOMASS** — the **EQUIPPED REFERENCE** haul rate
    /// (`EquipmentConfig::equipped_reference(HuntCarry)`, the sled's own tier, `40`), the term
    /// `systems::hunt_take`'s collection multiplies by the head-count. **NOT**
    /// `labor_config.hunt.per_worker_biomass_capacity`, which is the *sledless* baseline (`12`)
    /// since the carries moved onto their tiers: a herd row is a fact about the herd and a herd has
    /// no band to resolve a kit against, so it quotes what a kitted party hauls. No seasonal factor
    /// (the animal web has none), so it is never `0` for a live source.
    ///
    /// It is what turns a ceiling into a **crew count**, and it is deliberately not derived from
    /// [`Self::provisions_per_biomass`] and `per_worker_yield`: that quotient is `0 / 0` on a wolf.
    /// It **supersedes `PopulationCohortState::hunt_per_worker_provisions` for a per-herd preview** —
    /// that field is a species-blind cohort echo and stays only as the expedition outfit lever.
    /// The animal twin of [`ForagePatchState::per_worker_biomass`]. Appended (append-only).
    #[serde(default)]
    pub per_worker_biomass: f32,
    /// **This herd's own per-turn regrowth, in biomass, sampled at evenly spaced fractions of `K`** —
    /// `fauna::net_biomass_delta` on the herd's `herd_ecology`/`herd_capacity`, the same seam
    /// `regrow_biomass` advances it with. Sample `i` of `n` is the delta at `B = i/(n−1) × K`; the
    /// x-axis is implicit and a client interpolates between samples.
    ///
    /// **The low samples are NEGATIVE** — below `collapse_fraction × K` a herd is past its Allee
    /// threshold and declines every turn, hunted or not. Render them as decline, never clamped: that
    /// crash is why floor `0` ends a herd while it only sets a patch back
    /// ([`ForagePatchState::regrowth_samples`] is non-negative at every sample).
    ///
    /// It is sampled rather than published as `r` + thresholds because the two webs are two different
    /// functions — see the schema comment. Appended (append-only).
    #[serde(default)]
    pub regrowth_samples: Vec<f32>,
    /// **The cut point below which this source reads `collapsing`**, as a fraction of
    /// [`Self::carrying_capacity`] — the band `fauna::classify_ecology_phase` cuts on, resolved
    /// through the *same* seam the published `ecology_phase` word is, so the two cannot disagree.
    /// Read it in the units the **floor** is in: both are fractions of `K`, which is why the phase
    /// bands are the chart's background for the floor line.
    #[serde(default)]
    pub collapse_fraction: f32,
    /// **The cut point below which this source reads `stressed`** (and at or above which it reads
    /// `thriving`) — see [`Self::collapse_fraction`].
    #[serde(default)]
    pub stressed_fraction: f32,
    /// **How many animals ONE hunter can bring into contact per turn** (`SpeciesDef::engage_rate`,
    /// `docs/plan_hunt_through_combat.md` §2) — the **third** bound on a hunt take, beside the stock
    /// standing above the floor and the party's carry. Without it the pre-commit curve a client
    /// composes from [`Self::per_worker_biomass`] and the per-biomass vector is carry-bound only, and
    /// overstates a small-bodied species' take by the ratio of the two (measured: ~30× on a Wild Fowl
    /// herd with one hunter). It ships as a **term** rather than an answer because the expression is
    /// linear and exact — see the schema comment for the composition and for the crew count that
    /// inverts it.
    ///
    /// **`0` means "no engagement stage", not "reaches nothing"** — the wire's finite stand-in for the
    /// sim's [`f32::INFINITY`]: a **pen** (a penned animal is not stalked) and a species the roster
    /// cannot resolve. Read `<= 0` as *unbounded* and drop the term. Appended (append-only).
    #[serde(default)]
    pub engage_rate: f32,
    /// **How much damage ONE animal of this species soaks before it goes down**
    /// (`SpeciesDef::combat.durability`, `docs/plan_hunt_through_combat.md` §4.2/§6.5) — the last
    /// term needed to explain the combat gate *before* a hunt is launched.
    ///
    /// The client already holds the other two — `PopulationCohortState::hunter_attack` and
    /// [`Self::defense`] — so **the gate itself is already composable** and the sim deliberately
    /// exports no *"can this band win"* boolean:
    /// `effective_attack = max(0, hunter_attack − defense)` (`0` ⇒ this species cannot be hunted at
    /// all), `hunter_turns = durability / effective_attack`. This field is what turns *"you cannot
    /// win"* into *"you cannot win, and with spears it would take 62 hunter-turns"*.
    ///
    /// **`defense` and `durability` are different axes**: defense is whether a hit counts at all,
    /// durability is how many counting hits it takes. Authored per species, never derived from
    /// [`Self::body_mass`]. `0` for a species the roster cannot resolve. Appended (append-only).
    #[serde(default)]
    pub durability: f32,
    /// **What fraction of the animals a party reaches actually STAYS to be fought** — `1 − wariness`.
    ///
    /// It ships as a **term** for the same reason `engage_rate` does: `stayers = reached ×
    /// stay_fraction` is one linear factor, exactly the shape of the carry and engagement terms
    /// beside it. What the schema previously refused to publish was a client-side copy of the *take
    /// model*; the non-linear halves — the whole-animal quantiser, the fight's damage/durability
    /// division, and `hit_chance` — remain the sim's answer and `hit_chance` is still unpublished.
    ///
    /// A kit multiplies it through `KitOptionState::dispersion`. `1.0` = nothing breaks off, which is
    /// the honest reading for a pen and for the whole plant web.
    #[serde(default = "multiplier_neutral")]
    pub stay_fraction: f32,
    /// **The kit this QUARRY wants** — the roster id the hunt compose sheet opens on for this herd,
    /// and the one `assign_labor … hunt <herd> <n>` resolves when the player names none.
    ///
    /// **Derived, never authored.** The sim scores every hunt-job kit's per-hunter-turn take against
    /// this species and publishes the winner, but only when it beats the hunt job's default by
    /// `equipment.json`'s `quarry_default_kit_margin`; otherwise the job default stands. Wear does
    /// not enter the score, so this is a per-world constant per herd and cannot reshuffle as a band's
    /// spears wear down.
    ///
    /// Empty only for a herd whose species the roster cannot resolve — the same "fall back to
    /// `SubsistenceSection::default_hunt_kit_id`" reading every other unresolved row gives.
    #[serde(default)]
    pub default_kit_id: String,
    /// **What ONE UNIT of this herd's biomass is MADE OF** (arc #527) — the material twin of
    /// [`Self::provisions_per_biomass`], and the replacement for the retired `trade_per_biomass`.
    ///
    /// It composes at **any** floor by the same rule the scalar rates do —
    /// `ceiling(floor) = max(0, B − floor·K) × rate` — which is what lets a client draw an inedible
    /// quarry's payoff curve at all: a wolf's food rate is honestly `0`, and this is its whole
    /// payload.
    ///
    /// **Empty is "no row", never zero.** Most species are made of nothing anyone builds with.
    /// **Never summed** into one figure — that is the retired trade axis under a new name. Appended
    /// (append-only).
    #[serde(default)]
    pub material_per_biomass: Vec<MaterialPayoff>,
    /// **What ONE HUNTER brings home per turn, per material** — the material twin of
    /// [`Self::per_worker_yield`], so a band preview clamps
    /// `min(workers × per_worker_material, ceiling)` per material exactly as it does for food.
    ///
    /// **THIS is the rate a per-herd preview uses**, not the cohort's species-blind
    /// `hunt_per_worker_provisions`. Same shape and same caveats as
    /// [`Self::material_per_biomass`]. Appended (append-only).
    #[serde(default)]
    pub per_worker_material: Vec<MaterialPayoff>,
    /// **How many more turns a build on this source needs**, at the crew, floor and kit that worked
    /// it this turn — and a **PROJECTION when nothing is being built**, which is by definition the
    /// state a compose sheet is looking at. With an improvement in flight it counts down the running
    /// meter; with none it is what the rung this source would climb **next** would take the crew
    /// currently working it, from the work already banked on that rung. Always meaningful, never
    /// `-1`-because-unstarted — the same always-meaningful rule `rungQuotes[animal:pen].foodYield` follows.
    ///
    /// **Which `*_work_cost` it belongs beside** is the assignment's own `improvement`, and the
    /// **next rung up** when that is empty (`rungQuotes[plant:tended].reached` / `rungQuotes[plant:farm].reached`, `domestication` /
    /// `corralled`): the pair is read as *"50 work, ≈13 turns"*, so they must name one rung.
    ///
    /// **IT IS A CHAINED DATE** (`docs/plan_standing_upkeep.md` §4.6b). The band's whole `builders`
    /// pool goes on the **head** of its queue until that entry's meter fills, then on the next — so
    /// a count here is *everything above this entry plus its own span at the full pool*, and
    /// A source with
    /// nothing queued is quoted at the **back of the line**, which is where a newly queued build
    /// would actually go.
    ///
    /// **FIVE NEGATIVES, FIVE FACTS.** [`crate::NO_BUILD_TURNS_ESTIMATE`] (`-1`) = **no estimate**,
    /// where there is genuinely no answer: the source is at the top of its ladder, the next rung's
    /// own gates refuse it for this faction (a projection must never quote a job the command would
    /// reject), or a gate refuses a *waiting* entry — which may well be eligible by the time it
    /// reaches the head. [`crate::BUILD_METER_HOLDS`] (`-2`) = a real, priced build whose net supply
    /// is exactly **zero**, so the meter holds where it is. [`crate::BUILD_METER_ROTS`] (`-3`) = the
    /// same build with a **negative** net: the meter is going backwards and banked work is being
    /// lost. [`crate::BUILD_QUEUE_BLOCKED`] (`-4`) = the band's builders are **staffed and standing
    /// on this entry** and its own gate refuses it, so nothing banks and nothing behind it moves.
    /// [`crate::BUILD_NOT_YET_ESTIMATED`] (`-5`) = the player queued this build **since the last turn
    /// resolved**, so no estimate pass has ever run for it — *"the sim has not looked"*, as against
    /// `-1`'s *"the sim looked and had no number"*.
    ///
    /// Render `-5` as **"queued — starts next turn"** and never as a warning: a build declared a
    /// second ago with a staffed pool on it is not a hazard, and folding it into `-1` put
    /// `⚠ Stalled 0%` on a fresh `Cultivate` until the next turn cleared it.
    ///
    /// Render `-2`/`-3` as **infinity**, and distinguish them — both are answers, and one of them is
    /// costing the player progress they already paid for. `-4` is neither: it is a **stuck queue**,
    /// and its remedy is off the build line entirely (pair it with this source's own
    /// `upkeep_shortfall` / `neglect_grace_remaining`).
    ///
    /// ⛔ **WHAT THE NET IS STRUCK FROM IS THE ROT, NOT THE MAINTENANCE RATE** (§4.6a). The band's
    /// keeping pool owes that rate for every meter carrying work, at any fullness, and a build
    /// supplies none of it — so a reader that nets `*_upkeep_demand` here prices the build against a
    /// bill it does not pay. The term is [`Self::meter_rot_per_turn`], and it does not vary with the
    /// pool.
    ///
    /// **The client cannot compute this** — it holds neither the pool's output, nor the queue, nor
    /// the kit's build rate — so the sim answers and the client does no arithmetic. One field for
    /// both of a web's rungs: at most one improvement is ever in flight on one source, and at most
    /// one rung is ever next. Appended (append-only).
    #[serde(default = "no_build_turns_estimate")]
    pub build_turns_remaining: i32,
    /// **What the pool's TOOLS add to what it delivers this turn**, in work units per turn — the
    /// `gear(w)` of the closed form under [`Self::build_work_per_worker_turn`], resolved at the crew
    /// that worked this source (`docs/plan_standing_upkeep.md` §4.8).
    ///
    /// ⛔ **A TOOL'S HELP LANDS ON THE CREW'S OUTPUT, NOT ON THE JOB, AND THIS FIELD CHANGED UNITS
    /// WITH IT.** It used to be the `t` in `effective_cost = work_cost − t`
    /// (`docs/plan_unit_costed_work.md` §6) — a one-time lump struck off the target, however long the
    /// job ran. `work_cost` is now the whole pile with tools and without, and this is a **rate** that
    /// raises how fast the pile is worked off: a readout says *"your hoes: +1.0 work/turn"* beside a
    /// price that does not move, where it used to say *"−17 work"* off the price itself. Netting this
    /// out of `work_cost` double-counts the kit.
    ///
    /// **Per equipped worker, summed**: a worker holding a tool delivers its worth on top of their
    /// own hands, a worker without one delivers only their hands. `0` = no build in flight, or the
    /// crew carries nothing that helps — which, since each web got its own builders kit, means a pool
    /// sent out bare or one carrying the **other** web's tool. Appended (append-only).
    #[serde(default)]
    pub build_work_from_gear: f32,
    /// **The work ONE worker banks on this source per turn at the food peak**, before the floor
    /// multiplier and before any gear — `intensification::build_work_per_worker_turn`, today
    /// `PER_WORKER_OUTPUT` (`1.0`).
    ///
    /// **Published rather than left as a client constant** because worker output is deliberately
    /// written as a **sum of terms** (`docs/plan_unit_costed_work.md` §5) with exactly one term
    /// today: the day a buff mechanic adds a second, a client hard-coding `1.0` would quote a turn
    /// count the sim disagrees with, and would need its own change to track it.
    ///
    /// **It is the crew-output half of the build's closed form**; the gear half is
    /// `build_work_per_worker` × `build_work_saturating_crew` on the band's own
    /// [`crate::state::BandKitTiersState`] row, because a kit's saturation point is a fact about the
    /// band's ledger and not about any source:
    ///
    /// ```text
    /// gear(w)  = min(w, build_work_saturating_crew) × build_work_per_worker
    /// turns(w) = ceil((work_cost − work_done)
    ///                 / (w × build_work_per_worker_turn + gear(w) − meter_rot_per_turn))
    /// ```
    ///
    /// ⛔ **`gear(w)` IS IN THE DIVISOR, and it moved there in `docs/plan_standing_upkeep.md` §4.8.**
    /// It used to sit in the numerator (`work_cost − work_done − gear(w)`), which granted the kit's
    /// help as a one-time lump against the target; `build_work_per_worker` is extra work delivered
    /// **per worker per turn** now, so it is an addend on the supply and the numerator is the job,
    /// whole. A consumer still subtracting it under-quotes every geared pool.
    ///
    /// **`meter_rot_per_turn` IS THE DIVISOR'S SECOND TERM, and `*_upkeep_demand` IS NOT**
    /// (`docs/plan_standing_upkeep.md` §4.6a). The maintenance rate is owed to the band's **keeping
    /// pool** for every meter carrying work, at any fullness, and a build crew supplies none of it —
    /// so the crew's whole output is progress and netting a *rate* here would price the build against
    /// a bill it does not pay. What a build can fail to out-run is the ground going backwards under
    /// it, which is exactly [`Self::meter_rot_per_turn`], and it does not vary with `w`.
    ///
    /// **THERE IS NO FLOOR FACTOR.** `learn_multiplier(floor)` came off the build accrual when the
    /// crews separated — a build crew is not pulling on the source — so the crew term is the head
    /// count and nothing else. A form still carrying `× floor / food_peak` disagrees with the sim at
    /// every floor but the food peak.
    ///
    /// **`*_upkeep_demand` still belongs beside `*_work_cost`**, as the **standing price** of the
    /// rung being quoted — what holding it will cost every turn, forever, against the one-off pile
    /// the `work_cost` names. Read it as the second half of the quote, never as a term of this form.
    ///
    /// Appended (append-only).
    #[serde(default)]
    pub build_work_per_worker_turn: f32,
    /// **What this herd's at-risk meter will lose on the next decay pass**, in work units — the
    /// plant twin's field on the same rule, so a client's build estimate is one expression across
    /// both webs (`docs/plan_standing_upkeep.md` §4.6a).
    ///
    /// **It is always `0` on the shipped ladder, and that is not an omission**: neither animal rung
    /// declares a `meter_decay`, because an under-kept flock **sheds animals** instead. Nothing eats
    /// an animal build. Appended (append-only).
    #[serde(default)]
    pub meter_rot_per_turn: f32,
    /// **WHY THE BAND'S BUILDERS ARE STUCK ON THIS SOURCE** — `""` whenever this source is not a
    /// blocked build, else a short lowercase cause key (`knowledge`, `escapement`, `no_crop`,
    /// `species_ceiling`, `rung_below`, `owned_by_other`, `site`, `ring_idle`, `undeclared`,
    /// `unworked`), on the free-form-string convention [`Self::ecology_phase`] already uses.
    ///
    /// **Read it beside [`Self::build_turns_remaining`]'s `-4`, never instead of it**: that field
    /// says the pool is stuck, this says which conjunct of the rung's own gate refused. The sim
    /// decides `eligible`, so the sim says why — a client re-deriving it would be a second producer
    /// of one verdict.
    ///
    /// **Carried down the queue with the sentinel**, and it rides the same winner as the three
    /// fields above. The `.fbs` comment on `buildBlockedReason` carries the whole key table.
    /// Appended (append-only).
    #[serde(default)]
    pub build_blocked_reason: String,
    /// **WHERE THE PLAYER SENT THIS SOURCE** — the queued entry's destination rung, as
    /// `<branch>:<id>`, or empty when no band has queued it. The entry retires when the source
    /// reaches this rung's **top**, not when an intermediate rung fills.
    /// Appended (append-only).
    #[serde(default)]
    pub build_destination_rung: String,
    /// **THE LEGS STILL TO LAY** ([`BuildLegState`]), in climb order, first-incomplete first. Empty
    /// when the source is not queued or has already arrived; the first entry is the leg in flight.
    /// Appended (append-only).
    #[serde(default)]
    pub build_legs: Vec<BuildLegState>,
    /// **THE CARRYING CAPACITY THIS SOURCE WILL HAVE AT THE RUNG ITS BUILD IS HEADING FOR** —
    /// `None` (→ [`crate::NO_BUILD_DESTINATION_CAPACITY`] on the wire) when no band has queued it,
    /// because then there is no destination to quote.
    ///
    /// # WHY IT SHIPS: THE FLOOR MOVES UNDER THE PLAYER WHILE THEY BUILD
    ///
    /// A take is held above `floor_fraction × K`, and a rung **raises `K`** — the plant web through
    /// `field_capacity_gain`, the animal web through `pastoral_density` / `pen_density`, all three
    /// interpolated on the source's ladder position. So the floor climbs every turn a build runs and
    /// the player's take falls. With only the live `K` on the wire, the client can mark that the
    /// floor is *moving* and nothing more, so a reduced take reads as the source being poor rather
    /// than as the player's own investment arriving. This is where it is going.
    ///
    /// # IT IS THE DESTINATION'S, NOT NEXT TURN'S
    ///
    /// Next turn's capacity is not well defined — next turn's position depends on work nobody has
    /// banked yet. The **destination** is exact: the queue entry already names the rung its climb
    /// ends on ([`Self::build_destination_rung`]), so the gain at that rung is known today. Read the
    /// two as one object; this is the capacity *of* that rung.
    ///
    /// # THE RUNG MOVES, THE LAND DOES NOT
    ///
    /// Struck at capture by the **same seam that writes the live `carrying_capacity`**, at the
    /// destination standing instead of the source's own — one expression at two standings, never a
    /// second formula. The land underneath it is today's: a herd's flow is summed over the graze as
    /// it stands now, a patch's is its tile's own `K`. So the figure moves if the land does, exactly
    /// as the live capacity beside it moves.
    ///
    /// Appended (append-only).
    #[serde(default)]
    pub build_destination_capacity: Option<f32>,
    /// **What the in-flight fence ring costs, in work units** — the *denominator* of
    /// [`Self::pen_extend_progress`], in the same units, so the pen ring ships the meter/pile pair
    /// every other build on this row already does.
    ///
    /// **It is the herd's STAMPED cost, not a live quote** — the one way it differs from
    /// `Self::rungQuotes[animal:pastoral].workCost` / `Self::rungQuotes[animal:pen].workCost`, which resolve at capture so a compose
    /// sheet can price a build before the player commits. `Herd::accrue_pen_extension` stamps it on
    /// the first worked turn, so it reads `0.0` both with no ring in flight and on the turn a ring is
    /// begun. Read the pair together: `0.0 / 0.0` is *"no ring"*, never *"0%"*.
    /// Appended (append-only).
    #[serde(default)]
    pub pen_extend_cost: f32,
    /// **HOW MUCH OF ITSELF THIS HERD GIVES AS STANDING OUTPUT** — milk, eggs and wool instead of
    /// meat, `0..1` (`docs/plan_pen_standing_yield.md`). The meat take is the ordinary take
    /// `× (1 − this)`; the standing yield is the species' per-head rates `× head count × this ×`
    /// the rung's share. `0.0` is every herd's reading until a player pays to change it.
    ///
    /// **At `1.0` the herd is taken from not at all**, so it rides at `K` and its surplus births
    /// are self-limiting — *the cull is the meat take*, and moving this below `1` is how one is
    /// performed. **One fraction, not one per output**: only meat trades off, so a herd that gives
    /// milk and wool gives both at this. Appended (append-only).
    #[serde(default)]
    pub standing_output_fraction: f32,
    /// **THE FRACTION THE PLAYER HAS ORDERED AND IS PAYING FOR**, or
    /// [`NO_OUTPUT_COMMITMENT_IN_FLIGHT`] when none is.
    ///
    /// **A negative sentinel rather than `0.0`**, because `0.0` is a real, ordinary order (*"go back
    /// to all meat"*) and a reader must be able to tell *committing to nothing* from *not
    /// committing*. Appended (append-only).
    #[serde(default = "no_output_commitment_in_flight")]
    pub standing_output_target: f32,
    /// **The in-flight commitment's build meter**, in work units — exactly what
    /// [`Self::pen_extend_progress`] is, read exactly the same way. Appended (append-only).
    #[serde(default)]
    pub output_recommit_progress: f32,
    /// **What the in-flight commitment costs**, in work units — the denominator of
    /// [`Self::output_recommit_progress`], and the herd's **stamped** cost rather than a live quote
    /// ([`Self::pen_extend_cost`]'s rule). `0.0 / 0.0` is *"no commitment"*, never *"0%"*.
    /// Appended (append-only).
    #[serde(default)]
    pub output_recommit_cost: f32,
    /// **WHAT RUNG THIS HERD IS STANDING ON NOW** — the animal twin of
    /// [`ForagePatchState::current_rung`], which carries the rationale: `<branch>:<id>`, the
    /// [`Self::build_destination_rung`] spelling, saying where the source **is** rather than where a
    /// queued climb is headed.
    ///
    /// Struck at capture by `core_sim`'s `fauna::herd_rung_key` — the single home of the *"penned →
    /// pen, tamed → pastoral, else wild"* test this row otherwise makes a client re-derive from
    /// `Self::domestication` against a threshold plus `Self::corralled`. **Never empty**; an
    /// untouched herd reads `animal:wild`. Appended (append-only).
    #[serde(default)]
    pub current_rung: String,
    /// **THE WHOLE MATERIAL PILE THE RUNG THE `⌃` TRACK WOULD OFFER SWALLOWS**, per material id
    /// (`docs/plan_standing_upkeep.md` §2.7) — the second currency beside that rung's `*_work_cost`.
    ///
    /// `work_cost`'s own rule: resolved at capture off the **ladder** and published whether or not a
    /// build is in flight, because a compose sheet has to quote a price before the player commits.
    /// The pile is drawn **in proportion to the work banked** rather than on completion, so a rung a
    /// third raised has swallowed a third of this.
    ///
    /// **Empty is "no row", never zero** — every rung on the shipped ladder but `animal:pen`.
    #[serde(default)]
    pub build_material_cost: Vec<MaterialPayoff>,
    /// **WHAT HOLDING THIS SOURCE'S OWN RUNG SWALLOWS PER TURN**, per material — interpolated on the
    /// position and scaled by the **same** `scaled_by` the work demand reads, so a pen holding twice
    /// the herd mends twice the fence. It is the **stamped** bill the keeping was judged against,
    /// exactly as `upkeep_demand` beside it is.
    #[serde(default)]
    pub upkeep_material_demand: Vec<MaterialPayoff>,
    /// **WHAT THE BAND'S STORE ACTUALLY PAID** toward it, per material — the good-side twin of
    /// `upkeep_supplied`. Both terms are published rather than their difference, on the work trio's
    /// own rule.
    ///
    /// ⛔ **A SHORTFALL OF EITHER KIND TRIPS THE SAME GRACE.** There is one neglect counter and one
    /// `upkeep.grace_turns`; the decay rides the **worst** of the work fraction and each good's, and
    /// the amounts are never summed — a full store must not paper over missing hands.
    #[serde(default)]
    pub upkeep_material_supplied: Vec<MaterialPayoff>,
    /// **The crew hands spent keeping this source this turn** (`docs/plan_site_crews.md` §2.1) —
    /// fractional, summed across every band whose crew keeps it. The row's `keeps N of M` reads it;
    /// `0` on a source that owes nothing or that nobody works.
    #[serde(default)]
    pub upkeep_hands: f32,
    /// **The band-wide settlement filled less than this site's keeping-tool claim**
    /// (`docs/plan_site_crews.md` §2.3) — its crew kept with more of its own hands. The row's `ⓘ`.
    #[serde(default)]
    pub upkeep_tools_short: bool,
    /// The predator species key that drew the largest share off this herd this turn; empty when
    /// nothing preyed on it. Appended last.
    #[serde(default)]
    pub predator_eaten_by: String,
    /// The biomass (the unit of `biomass`) predators drew off this herd this turn. Appended last.
    #[serde(default)]
    pub predator_eaten: f32,
    /// **The keeping class this herd's meat lands in** (#798) — pairs with the band's
    /// `hunt_meals` to size a hunting crew. Empty for a species the fauna table cannot resolve.
    /// Appended last.
    #[serde(default)]
    pub keeping: String,
    /// **The next seasonal ground of a migratory herd** (#799, the `herd_lore` knowledge): the
    /// route anchor it goes to after the one it is at, or is already walking to. `-1` unless the
    /// viewer's people know herd lore and the herd is visible (the ground tile is not fog-checked).
    /// Appended last.
    #[serde(default = "no_next_ground")]
    pub next_ground_x: i32,
    /// See [`Self::next_ground_x`].
    #[serde(default = "no_next_ground")]
    pub next_ground_y: i32,
    /// **Every rung of this source's branch above the root, quoted** - one [`RungQuoteState`] per
    /// rung in climb order. Replaces the per-step fields (`cultivation*`, `tended*`, `field*`,
    /// `sow*`, `tame*`, `corral*`, `pastoral*`, `domestication`): a rung is a row keyed by its
    /// `rung_key`, never a field, so a new step is a config edit. Appended last.
    #[serde(default)]
    pub rung_quotes: Vec<RungQuoteState>,
    /// The id of the size tier this herd's fence has reached (`ranch`), empty when it is in none.
    pub size_tier: String,
    /// **The tile this herd's queued Corral will pen it on** (`fauna::choose_pen_site`): the
    /// best-grazing land within the keeping band's work range. `-1` on both axes when no Corral is
    /// queued. Appended last.
    #[serde(default = "no_pen_site")]
    pub pen_site_x: i32,
    /// See [`Self::pen_site_x`].
    #[serde(default = "no_pen_site")]
    pub pen_site_y: i32,
    /// **The faction taming or keeping this herd** (`None` = an untouched herd). Rides the wire as the
    /// `hasOwner`/`owner` pair, the forage patch's spelling of the same `Option`. Appended last.
    #[serde(default)]
    pub owner: Option<u32>,
    /// The band whose work last advanced this herd's build meter (`0` = none). Appended last.
    #[serde(default)]
    pub builder_band: u64,
}

/// The `-1` "no Corral queued" sentinel of the pen-site pair.
fn no_pen_site() -> i32 {
    -1
}

/// The `-1` "no ground to show" sentinel of the next-seasonal-ground pair — the same convention
/// `next_x`/`next_y` use for "no heading".
fn no_next_ground() -> i32 {
    -1
}

impl Default for HerdTelemetryState {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            species: String::new(),
            x: 0,
            y: 0,
            biomass: 0.0,
            route_length: 0,
            next_x: -1,
            next_y: -1,
            size_class: String::new(),
            huntable: false,
            ecology_phase: String::new(),
            per_worker_yield: 0.0,
            pen_fed_fraction: pen_fully_fed(),
            carrying_capacity: 0.0,
            graze_range_radius: 0,
            pen_radius: 0,
            pen_footprint_tiles: 0,
            pen_pasture_fraction: 0.0,
            pen_extend_progress: 0.0,
            husbandry_ceiling: String::new(),
            body_mass: 0.0,
            food_per_animal: 0.0,
            herders_needed: 0,
            herded_fraction: fully_herded(),
            fodder_draw: 0.0,
            pen_fodder_shortfall: 0.0,
            attack: 0.0,
            defense: 0.0,
            ferocity: 0.0,
            aggression: 0.0,
            prey_sense_radius: 0,
            herders_needed_if_managed: 0,
            upkeep_demand: 0.0,
            upkeep_supplied: 0.0,
            upkeep_shortfall: 0.0,
            upkeep_workers_needed: 0,
            upkeep_workers_at_completion: 0,
            upkeep_next_worker_turn: NO_NEXT_KEEPING_WORKER,
            upkeep_toe: Vec::new(),
            builders_staffed: 0,
            builders_kitted: 0,
            builders_cap: 0,
            build_toe: Vec::new(),
            has_neglect_grace: false,
            neglect_grace_remaining: 0,
            provisions_per_biomass: 0.0,
            fodder_per_biomass: 0.0,
            per_worker_biomass: 0.0,
            regrowth_samples: Vec::new(),
            collapse_fraction: 0.0,
            stressed_fraction: 0.0,
            // `0` is the "no engagement stage" reading, which is the honest default for a herd
            // nothing has described.
            engage_rate: 0.0,
            durability: 0.0,
            // **`1.0` — nothing breaks off.** The honest default for a source with no retreat stage
            // (a pen, the whole plant web); a `0.0` default would read as "every animal flees" and
            // silently zero a take.
            stay_fraction: multiplier_neutral(),
            // A herd nothing has described names no kit — the same "fall back to the hunt job's
            // default" reading the capture publishes for a species the roster cannot resolve.
            default_kit_id: String::new(),
            // No material — the ordinary case, and an EMPTY list rather than a row of zeros.
            material_per_biomass: Vec::new(),
            per_worker_material: Vec::new(),
            build_turns_remaining: no_build_turns_estimate(),
            build_work_from_gear: 0.0,
            build_work_per_worker_turn: 0.0,
            // A herd nothing has described quotes no rung, so neither rate is owed yet.
            meter_rot_per_turn: 0.0,
            build_blocked_reason: String::new(),
            build_destination_rung: String::new(),
            build_legs: Vec::new(),
            // **A herd nothing has described is heading nowhere** — the absent reading, never a
            // capacity of zero.
            build_destination_capacity: None,
            pen_extend_cost: 0.0,
            standing_output_fraction: 0.0,
            standing_output_target: NO_OUTPUT_COMMITMENT_IN_FLIGHT,
            output_recommit_progress: 0.0,
            output_recommit_cost: 0.0,
            // **A herd nothing has described names no rung** — the same "not described" reading
            // `build_destination_rung` above takes, and NOT a wire state: the capture strikes this
            // from `fauna::herd_rung_key` on every row, so an empty string only ever reaches a
            // hand-built fixture. Naming `animal:wild` here would put a second spelling of the
            // ladder in this crate, which is the duplication the field exists to remove.
            current_rung: String::new(),
            build_material_cost: Vec::new(),
            upkeep_material_demand: Vec::new(),
            upkeep_material_supplied: Vec::new(),
            upkeep_hands: NO_UPKEEP_HANDS,
            upkeep_tools_short: false,
            predator_eaten_by: String::new(),
            keeping: String::new(),
            next_ground_x: no_next_ground(),
            next_ground_y: no_next_ground(),
            predator_eaten: 0.0,
            size_tier: String::new(),
            rung_quotes: Vec::new(),
            pen_site_x: no_pen_site(),
            pen_site_y: no_pen_site(),
            owner: None,
            builder_band: 0,
        }
    }
}

/// **One composition entry's material rates** — the wrapper the wire needs because FlatBuffers has
/// no vector-of-vectors, and the shape a reader should think in: `rows` is *that plant's* materials.
///
/// **Empty is "no row", never zero** — see
/// [`ForagePatchState::composition_material_per_biomass`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SpeciesMaterialRates {
    #[serde(default)]
    pub rows: Vec<MaterialPayoff>,
}

/// One depletable forage patch's cultivation + ecology state for the client tile card
/// (Intensification Phase 1a). Keyed by tile `(x, y)`. `rungQuotes[plant:tended].progress` is the 0..1 taming
/// meter; `rungQuotes[plant:tended].reached` = a completed tended patch. `owner` is the tending faction (`None` = a
/// wild/untended patch). `biomass`/`carrying_capacity`/`ecology_phase` let the client show patch
/// health. Mirrors `HerdTelemetryState`'s display-telemetry role for the plant side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ForagePatchState {
    pub x: u32,
    pub y: u32,
    #[serde(default)]
    pub owner: Option<u32>,
    /// The band whose work last advanced this patch's build meter (`0` = none). Appended last.
    #[serde(default)]
    pub builder_band: u64,
    #[serde(default)]
    pub biomass: f32,
    /// **WHAT THIS PATCH HOLDS NOW** — the ground's own `K` already multiplied by the **interpolated**
    /// `field_capacity_gain` this patch's ladder position has bought
    /// (`forage::patch_carrying_capacity`). It is **live state that carries the rung**, not a fact
    /// about the terrain: a standing Field reads ~2.53× the same ground left wild, and a half-raised
    /// one reads part-way between, continuously.
    ///
    /// # ⛔ REDACT IT UNDER FOG — [`Self::tile_capacity`] is what replaces it there
    ///
    /// `rungQuotes[plant:farm].reached` and `rungQuotes[plant:farm].progress` are already withheld on a remembered hex; publishing this one
    /// unredacted hands back the same ladder position, and in a finer grain than the boolean did —
    /// being interpolated it states *how far up*, not merely *whether*. Render this where the tile is
    /// **visible** and [`Self::tile_capacity`] where it is merely **discovered**.
    #[serde(default)]
    pub carrying_capacity: f32,
    #[serde(default)]
    pub ecology_phase: String,
    /// **Pre-commit yield forecast** at the patch's current biomass (per turn, captured at
    /// `output_multiplier = 1.0` — the client scales by the band's `outputMultiplier`). Lets the
    /// client state the take and cap its worker stepper *while the player is composing an
    /// assignment*, before anything is committed: `expected(workers, floor) = min(workers ×
    /// per_worker, ceiling)` and `max_useful_workers(floor) = ceil(ceiling / per_worker)`, **per
    /// account**, where the ceiling is composed at any floor from terms already on the wire —
    /// `(biomass − floor × carrying_capacity) × provisions_per_biomass`, see
    /// [`Self::provisions_per_biomass`]. The six flat `ceiling_*` scalars went first (#426) and the
    /// per-policy ceiling rows that replaced them retired with the four harvest stances. This field
    /// survives as the FOOD per-worker term only, because a patch-level scalar cannot state a
    /// floor-dependent rate and the non-food accounts genuinely have one. Provisions/turn one
    /// forager contributes (this tile's seasonal weight folded in, as the take does); `0.0` in a
    /// dead season — do not divide by it, and **do not read it as "the wire said nothing"**:
    /// whether a patch was DESCRIBED is the row's presence, not this number's size.
    #[serde(default)]
    pub per_worker_yield: f32,
    /// **What is actually growing here** — the named plants *this patch's* biomass is made of, as
    /// normalized shares (`docs/plan_flora_roster.md` §2: naming decomposes, it does not add).
    ///
    /// **The TILE names the plants and the RUNG says how much of each** (#433). The tile's own
    /// basket is a pure function of its terrain, the roster's affinity weights and its coordinate
    /// (per-tile realization, §10); the patch then reweights it: a **tended** patch's favored crop
    /// rises to `min(1, share × tended_weeding_gain)` at the expense of the least abundant members,
    /// and a **Field** publishes its crop alone. **Both reweights reach only what working the ground
    /// can clear**: a member that is not growing in the soil — a kelp bed, a mussel bed, a navigable
    /// river's fish — keeps its share through both, so a Field beside a channel publishes the crop
    /// *and* the fishery rather than a single 100% entry. That is the whole of what a rung below 4 does
    /// — the tile's capacity itself never moves — so this list is where the client can *see* a
    /// commitment take hold. Zero-share entries are filtered out. The shares sum to `1.0` on any
    /// forage-bearing tile, so `share × forage_capacity` is that plant's own capacity and the parts
    /// always re-sum to the whole. Empty on a biome that carries no forage. Deterministically sorted
    /// (share DESC, then species key ASC).
    ///
    /// **`Arc<[_]>`, not `Vec<_>`, because a WILD patch's basket belongs to the TILE and not to the
    /// patch or to the turn.** It is derived once per tile per world into the capture's flora-quote
    /// memo (`core_sim`'s `snapshot/flora_quotes.rs`, #410); a `Vec` here made every patch row
    /// deep-copy it — two `String`s per named plant, ~5,984 plants at 80×52 — on every turn, and
    /// again on every whole-section clone downstream. Shared, a wild row costs one refcount bump;
    /// only the few committed patches rebuild a list. Nothing mutates a published basket, so sharing
    /// is invisible to every reader.
    #[serde(default)]
    pub composition: Arc<[FloraShareInfo]>,
    /// **How much of each plant is STANDING here**, index-aligned with [`Self::composition`]:
    /// `share × biomass`, in the same units as [`Self::biomass`]. A crop chip reads *"70% (63)"* off
    /// the pair, and a selective gather is a decision about a **quantity** — so the sim states it
    /// rather than leaving the client to hold capacity arithmetic.
    ///
    /// **It rides the patch row rather than [`FloraShareInfo`] because the basket is a MEMO**: a
    /// composition entry is a pure function of ground and config, derived once per tile per world
    /// and shared by refcount, while a standing biomass moves every turn. Appended (append-only).
    #[serde(default)]
    pub composition_standing_biomass: Vec<f32>,
    /// **What one unit of each named plant's biomass converts at**, index-aligned with
    /// [`Self::composition`] exactly as [`Self::composition_standing_biomass`] is, at the patch's
    /// **current standing rung** (the favored crop's conversion gain already in its own term).
    ///
    /// [`Self::provisions_per_biomass`] beside it is the **basket average**, which cannot price a
    /// *narrowing*: a compose sheet holding only that one watches its forecast sit still while the
    /// player ticks crop chips, though the worker dial next to it quotes live. These are what let the
    /// sheet compose `Σ_S share × rate ÷ Σ_S share` itself — the same contract
    /// [`Self::material_per_biomass`] already carries, at finer grain.
    ///
    /// **Not scaled by share** (the share is on the entry beside it), so summing them across species
    /// without the shares totals nothing. **Empty is "no row", never zero.** The identity
    /// `Σ share × rate == provisions_per_biomass` holds by construction. Appended (append-only).
    #[serde(default)]
    pub composition_provisions_per_biomass: Vec<f32>,
    /// The **fodder** twin of [`Self::composition_provisions_per_biomass`] — same alignment, same
    /// rules, `0` for a plant whose vector pays no hay. Appended (append-only).
    #[serde(default)]
    pub composition_fodder_per_biomass: Vec<f32>,
    /// **What each named plant is MADE OF** — the material twin of the two rate vectors above, one
    /// entry per [`Self::composition`] entry and index-aligned with it, at the patch's standing rung
    /// and **not scaled by share**.
    ///
    /// [`Self::material_per_biomass`] beside it is the **basket average**, so a sheet holding only
    /// that one cannot price a narrowing to a **cash crop** — the headline case of the whole
    /// selective gather, since baskets are made of fibre and baskets are what let a gatherer carry
    /// more food.
    ///
    /// **Never summed across species by the sim**: rows merge by material id *within* one plant and
    /// stop there, because a characteristic reading belongs to the batch a take creates and
    /// averaging two species' would invent a plant that is not growing there. **Empty rows mean "no
    /// row", never zero** — a grain pays no material and says so. Appended (append-only).
    #[serde(default)]
    pub composition_material_per_biomass: Vec<SpeciesMaterialRates>,
    /// **Which ONE named plant this patch has been committed to** (Flora Roster S1) — the stable
    /// `flora_config.json` species key. **`""` means the wild mixed basket, not "unknown"**: it is a
    /// positive statement that the patch is gathered as the whole [`Self::composition`] above.
    ///
    /// Set on the first turn a crew works the patch under `Cultivate`/`Sow`, and cleared when both
    /// improvement meters lapse to zero (the patch goes fully feral). **Recorded before it takes
    /// effect** — a patch still being prepared names its crop here while still carrying the tile's
    /// full capacity and converting at the wild rate; both halves switch on when the rung completes
    /// (`docs/plan_flora_roster.md` §4.3). Appended (append-only).
    #[serde(default)]
    pub committed_species: String,
    /// The player-facing name of [`Self::committed_species`], resolved server-side because the client
    /// holds no roster — exactly as [`FloraShareInfo::display_name`] is. `""` alongside an empty
    /// species key. Appended (append-only).
    #[serde(default)]
    pub committed_display_name: String,
    // **RETIRED: `cultivate_build_fraction` / `sow_build_fraction`** — the plant twins of the animal
    // pair; see [`HerdTelemetryState`] for why the dip dissolved into the work budget. Neither is on
    // the wire.
    // **RETIRED: `maintain`** — see [`HerdTelemetryState`] for why the toggle became a crew count.
    /// **THE BILL THIS PATCH'S KEEPERS WERE HANDED this turn**, in work units — always meaningful,
    /// `0` on a rung that declares no upkeep and on ground nobody has started.
    ///
    /// **One field name, two meanings, and the herd twin's is not stale.**
    /// [`HerdTelemetryState::upkeep_demand`] is *what the rung demands this turn* and that is still
    /// exactly right for a herd. A patch's demand **interpolates on its position**, so the live cost
    /// rises between the moment the keepers are billed (before the turn's build accrual) and the
    /// moment the next Logistics pass judges what they paid — judged against the risen number, a
    /// correctly-staffed keeping reads permanently short. What ships here is therefore the *stamped*
    /// bill, which is what makes `demand − supplied == shortfall` hold and what
    /// [`Self::upkeep_workers_needed`] is the `ceil` of.
    #[serde(default)]
    pub upkeep_demand: f32,
    /// **What the crew actually paid toward it** out of this turn's work budget.
    #[serde(default)]
    pub upkeep_supplied: f32,
    /// **What went unmet**, and therefore what the improvement decays by past its grace.
    #[serde(default)]
    pub upkeep_shortfall: f32,
    /// **HANDS TO MEET THE DEMAND** — `ceil(upkeep_demand / PER_WORKER_OUTPUT)`, the **maintain**
    /// activity's own `workers_needed`, in its own unit. Its sibling is the **take** activity's
    /// (`SourceYield::workers_needed` = hands to haul the offer); a count blended across units is
    /// what a single worker allocation forced, and each activity answers for itself now.
    #[serde(default)]
    pub upkeep_workers_needed: u32,
    /// **Whole workers the keeping takes once the rung in flight is finished** — equal to
    /// `upkeep_workers_needed` where no build is in flight (`docs/plan_site_crews.md`).
    #[serde(default)]
    pub upkeep_workers_at_completion: u32,
    /// **The game turn the keeping first needs one more whole worker than now**, projected along
    /// the build's own pace; [`NO_NEXT_KEEPING_WORKER`] = never within the build.
    #[serde(default = "no_next_keeping_worker")]
    pub upkeep_next_worker_turn: i32,
    /// **WHICH KEEPING TOOLS THIS SITE IS SHORT OF, BY NAME** — one
    /// [`crate::state::population::KitToeLineState`] per tool the site's keeping claimed this turn,
    /// summed over the crews keeping it. Short is `filled < required`, and
    /// `upkeep_tools_short` is exactly *"some line is short"*. Empty = the keeping claimed no tool.
    /// Appended last (append-only).
    #[serde(default)]
    pub upkeep_toe: Vec<crate::state::population::KitToeLineState>,
    /// **The builders on this source's declared build this turn**, the bare-handed and the kitted
    /// together (`docs/plan_labor_at_scale.md`, "Builds join the site's crew"). The viewer's own
    /// bands only. Appended last (append-only).
    #[serde(default)]
    pub builders_staffed: u32,
    /// Of [`Self::builders_staffed`], the ones holding the build kit the rung uses (every builder
    /// when the rung asks for no tool).
    #[serde(default)]
    pub builders_kitted: u32,
    /// **The most builders the build could take this turn** — the rung's `maxBuilders` cut by the
    /// room the tile left. `0` where no build is declared.
    #[serde(default)]
    pub builders_cap: u32,
    /// **The tools the build's builders asked for and what the settlement gave them**, one
    /// [`crate::state::population::PoolToeLineState`] per tool (`pool` reads `"build"`). Empty where
    /// the rung asks for no tool.
    #[serde(default)]
    pub build_toe: Vec<crate::state::population::PoolToeLineState>,
    /// **Is there anything here to neglect?** `false` for a wild patch (both improvement meters at
    /// zero), which is most of them. Read this before [`Self::neglect_grace_remaining`].
    #[serde(default)]
    pub has_neglect_grace: bool,
    /// **Turns of neglect this patch can still absorb before its improvement starts reverting** — the
    /// countdown, not the counter: `0` = the ground is going feral *now*, `N > 0` = it starts in N
    /// more un-worked turns. Describes whichever rung would bleed next, since the plant web unwinds
    /// **newest-first** (a Field's meter empties before the tended ground beneath it loses anything).
    #[serde(default)]
    pub neglect_grace_remaining: u32,
    // **RETIRED: `cultivate_crew_needed` / `sow_crew_needed`** — each rung's `crew_needed`, a floor
    // under the compose sheet's worker cap. The cap was inverted out of the TAKE and a building crew
    // was paid a dipped take, so a 25-turn improvement asked for fewer hands than gathering the same
    // ground. **The player staffs the band's `builders` pool now** (`docs/plan_standing_upkeep.md`
    // §2.5), so there is no blended count for a rung-level floor to raise. Neither
    // `cultivateCrewNeeded` nor `sowCrewNeeded` is on the wire.
    /// **What ONE UNIT of this patch's standing crop is worth**, in each account, at the patch's own
    /// basket-averaged rates (`patch_provisions_per_biomass` and its siblings — the seams
    /// `forage_take` pays with). With [`Self::biomass`], [`Self::carrying_capacity`] and the build-dip
    /// fractions the client evaluates `max(0, B − floor·K) × dip × rate` at **any** floor, which is
    /// what makes the harvest floor draggable (`docs/plan_harvest_floor.md` §5). A deliberate, narrow
    /// exception to *"the sim exports the answer"*: this expression is linear and exact, unlike the
    /// whole-animal quantisation that rule protects, and `SourceYield::actual` remains the sim's
    /// answer for the committed assignment. Appended (append-only).
    #[serde(default)]
    pub provisions_per_biomass: f32,
    /// The fodder half of the same vector — see [`Self::provisions_per_biomass`].
    #[serde(default)]
    pub fodder_per_biomass: f32,
    // **RETIRED: `trade_per_biomass`** (arc #527), and off the wire.
    /// **What ONE gatherer moves this turn, in BIOMASS** — `per_worker_biomass_capacity ×
    /// seasonal_weight` (`forage::forage_per_worker_biomass`), the term `forage_take`'s worker cap
    /// multiplies by the head-count. It folds in the tile's seasonal weight, so it is **`0` in a dead
    /// season** — do not divide by it.
    ///
    /// It is what turns a ceiling into a **crew count**: `ceil(room / (per_worker_biomass × dip))`.
    /// Deliberately not derived from [`Self::provisions_per_biomass`] and `per_worker_yield` — that
    /// quotient is `0 / 0` on a Field of cotton, flax or hay, which is exactly where the panel most
    /// needs a crew number. Appended (append-only).
    #[serde(default)]
    pub per_worker_biomass: f32,
    /// **This patch's own per-turn regrowth, in biomass, sampled at evenly spaced fractions of `K`** —
    /// `fauna::reseeding_logistic_regrowth` on the patch's own `patch_ecology`, the same seam
    /// `regrow_patch` advances it with, so a tended patch's curve is the one its rung bought. Sample
    /// `i` of `n` is the delta at `B = i/(n−1) × K`; the x-axis is implicit and a client interpolates.
    ///
    /// **The `0.0` sample is the reseed floor's lift, not zero**, and **no sample is ever negative**:
    /// plants have no Allee crash, which is exactly the asymmetry
    /// [`HerdTelemetryState::regrowth_samples`] carries on the other side. Appended (append-only).
    #[serde(default)]
    pub regrowth_samples: Vec<f32>,
    /// **The cut point below which this source reads `collapsing`**, as a fraction of
    /// [`Self::carrying_capacity`] — the band `fauna::classify_ecology_phase` cuts on, resolved
    /// through the *same* seam the published `ecology_phase` word is, so the two cannot disagree.
    /// Read it in the units the **floor** is in: both are fractions of `K`, which is why the phase
    /// bands are the chart's background for the floor line.
    #[serde(default)]
    pub collapse_fraction: f32,
    /// **The cut point below which this source reads `stressed`** (and at or above which it reads
    /// `thriving`) — see [`Self::collapse_fraction`].
    #[serde(default)]
    pub stressed_fraction: f32,
    /// **What ONE UNIT of this patch's biomass is MADE OF** (arc #527) — the material twin of
    /// [`Self::provisions_per_biomass`], and the replacement for the retired `trade_per_biomass`.
    ///
    /// It is the **rung-1** half of the material story: `FloraShareInfo`'s two payoffs quote a
    /// commitment at rungs 2 and 3, and a *wild* gather had nothing at all — a tile whose basket
    /// carries a cash crop read food-and-fodder-only while the turn banked fibre. Composes at any
    /// floor by the rule the scalar rates use.
    ///
    /// **A patch is a MIXED basket.** The rows are decomposed per species
    /// ([`crate::ForagePatchState`]'s composition keeps each one's own reading) and merged **by
    /// material id** for the rate; the *readings* are never averaged. Appended (append-only).
    #[serde(default)]
    pub material_per_biomass: Vec<MaterialPayoff>,
    /// **What ONE GATHERER brings home per turn, per material** — the twin of
    /// [`Self::per_worker_yield`], so a sheet clamps `min(workers × rate, ceiling)` per material.
    ///
    /// Folds in the tile's **seasonal weight** exactly as [`Self::per_worker_yield`] does, so it is
    /// honestly **empty in a dead season**. Appended (append-only).
    #[serde(default)]
    pub per_worker_material: Vec<MaterialPayoff>,
    /// **How many more turns a build on this source needs**, at the crew, floor and kit that worked
    /// it this turn — and a **PROJECTION when nothing is being built**, which is by definition the
    /// state a compose sheet is looking at. With an improvement in flight it counts down the running
    /// meter; with none it is what the rung this source would climb **next** would take the crew
    /// currently working it, from the work already banked on that rung. Always meaningful, never
    /// `-1`-because-unstarted — the same always-meaningful rule `rungQuotes[animal:pen].foodYield` follows.
    ///
    /// **Which `*_work_cost` it belongs beside** is the assignment's own `improvement`, and the
    /// **next rung up** when that is empty (`rungQuotes[plant:tended].reached` / `rungQuotes[plant:farm].reached`, `domestication` /
    /// `corralled`): the pair is read as *"50 work, ≈13 turns"*, so they must name one rung.
    ///
    /// **IT IS A CHAINED DATE** (`docs/plan_standing_upkeep.md` §4.6b). The band's whole `builders`
    /// pool goes on the **head** of its queue until that entry's meter fills, then on the next — so
    /// a count here is *everything above this entry plus its own span at the full pool*, and
    /// A source with
    /// nothing queued is quoted at the **back of the line**, which is where a newly queued build
    /// would actually go.
    ///
    /// **FIVE NEGATIVES, FIVE FACTS.** [`crate::NO_BUILD_TURNS_ESTIMATE`] (`-1`) = **no estimate**,
    /// where there is genuinely no answer: the source is at the top of its ladder, the next rung's
    /// own gates refuse it for this faction (a projection must never quote a job the command would
    /// reject), or a gate refuses a *waiting* entry — which may well be eligible by the time it
    /// reaches the head. [`crate::BUILD_METER_HOLDS`] (`-2`) = a real, priced build whose net supply
    /// is exactly **zero**, so the meter holds where it is. [`crate::BUILD_METER_ROTS`] (`-3`) = the
    /// same build with a **negative** net: the meter is going backwards and banked work is being
    /// lost. [`crate::BUILD_QUEUE_BLOCKED`] (`-4`) = the band's builders are **staffed and standing
    /// on this entry** and its own gate refuses it, so nothing banks and nothing behind it moves.
    /// [`crate::BUILD_NOT_YET_ESTIMATED`] (`-5`) = the player queued this build **since the last turn
    /// resolved**, so no estimate pass has ever run for it — *"the sim has not looked"*, as against
    /// `-1`'s *"the sim looked and had no number"*.
    ///
    /// Render `-5` as **"queued — starts next turn"** and never as a warning: a build declared a
    /// second ago with a staffed pool on it is not a hazard, and folding it into `-1` put
    /// `⚠ Stalled 0%` on a fresh `Cultivate` until the next turn cleared it.
    ///
    /// Render `-2`/`-3` as **infinity**, and distinguish them — both are answers, and one of them is
    /// costing the player progress they already paid for. `-4` is neither: it is a **stuck queue**,
    /// and its remedy is off the build line entirely (pair it with this source's own
    /// `upkeep_shortfall` / `neglect_grace_remaining`).
    ///
    /// ⛔ **WHAT THE NET IS STRUCK FROM IS THE ROT, NOT THE MAINTENANCE RATE** (§4.6a). The band's
    /// keeping pool owes that rate for every meter carrying work, at any fullness, and a build
    /// supplies none of it — so a reader that nets `*_upkeep_demand` here prices the build against a
    /// bill it does not pay. The term is [`Self::meter_rot_per_turn`], and it does not vary with the
    /// pool.
    ///
    /// **The client cannot compute this** — it holds neither the pool's output, nor the queue, nor
    /// the kit's build rate — so the sim answers and the client does no arithmetic. One field for
    /// both of a web's rungs: at most one improvement is ever in flight on one source, and at most
    /// one rung is ever next. Appended (append-only).
    #[serde(default = "no_build_turns_estimate")]
    pub build_turns_remaining: i32,
    /// **What the pool's TOOLS add to what it delivers this turn**, in work units per turn — the
    /// `gear(w)` of the closed form under [`Self::build_work_per_worker_turn`], resolved at the crew
    /// that worked this source (`docs/plan_standing_upkeep.md` §4.8).
    ///
    /// ⛔ **A TOOL'S HELP LANDS ON THE CREW'S OUTPUT, NOT ON THE JOB, AND THIS FIELD CHANGED UNITS
    /// WITH IT.** It used to be the `t` in `effective_cost = work_cost − t`
    /// (`docs/plan_unit_costed_work.md` §6) — a one-time lump struck off the target, however long the
    /// job ran. `work_cost` is now the whole pile with tools and without, and this is a **rate** that
    /// raises how fast the pile is worked off: a readout says *"your hoes: +1.0 work/turn"* beside a
    /// price that does not move, where it used to say *"−17 work"* off the price itself. Netting this
    /// out of `work_cost` double-counts the kit.
    ///
    /// **Per equipped worker, summed**: a worker holding a tool delivers its worth on top of their
    /// own hands, a worker without one delivers only their hands. `0` = no build in flight, or the
    /// crew carries nothing that helps — which, since each web got its own builders kit, means a pool
    /// sent out bare or one carrying the **other** web's tool. Appended (append-only).
    #[serde(default)]
    pub build_work_from_gear: f32,
    /// **The work ONE worker banks on this source per turn at the food peak**, before the floor
    /// multiplier and before any gear — `intensification::build_work_per_worker_turn`, today
    /// `PER_WORKER_OUTPUT` (`1.0`).
    ///
    /// **Published rather than left as a client constant** because worker output is deliberately
    /// written as a **sum of terms** (`docs/plan_unit_costed_work.md` §5) with exactly one term
    /// today: the day a buff mechanic adds a second, a client hard-coding `1.0` would quote a turn
    /// count the sim disagrees with, and would need its own change to track it.
    ///
    /// **It is the crew-output half of the build's closed form**; the gear half is
    /// `build_work_per_worker` × `build_work_saturating_crew` on the band's own
    /// [`crate::state::BandKitTiersState`] row, because a kit's saturation point is a fact about the
    /// band's ledger and not about any source:
    ///
    /// ```text
    /// gear(w)  = min(w, build_work_saturating_crew) × build_work_per_worker
    /// turns(w) = ceil((work_cost − work_done)
    ///                 / (w × build_work_per_worker_turn + gear(w) − meter_rot_per_turn))
    /// ```
    ///
    /// ⛔ **`gear(w)` IS IN THE DIVISOR, and it moved there in `docs/plan_standing_upkeep.md` §4.8.**
    /// It used to sit in the numerator (`work_cost − work_done − gear(w)`), which granted the kit's
    /// help as a one-time lump against the target; `build_work_per_worker` is extra work delivered
    /// **per worker per turn** now, so it is an addend on the supply and the numerator is the job,
    /// whole. A consumer still subtracting it under-quotes every geared pool.
    ///
    /// **`meter_rot_per_turn` IS THE DIVISOR'S SECOND TERM, and `*_upkeep_demand` IS NOT**
    /// (`docs/plan_standing_upkeep.md` §4.6a). The maintenance rate is owed to the band's **keeping
    /// pool** for every meter carrying work, at any fullness, and a build crew supplies none of it —
    /// so the crew's whole output is progress and netting a *rate* here would price the build against
    /// a bill it does not pay. What a build can fail to out-run is the ground going backwards under
    /// it, which is exactly [`Self::meter_rot_per_turn`], and it does not vary with `w`.
    ///
    /// **THERE IS NO FLOOR FACTOR.** `learn_multiplier(floor)` came off the build accrual when the
    /// crews separated — a build crew is not pulling on the source — so the crew term is the head
    /// count and nothing else. A form still carrying `× floor / food_peak` disagrees with the sim at
    /// every floor but the food peak.
    ///
    /// **`*_upkeep_demand` still belongs beside `*_work_cost`**, as the **standing price** of the
    /// rung being quoted — what holding it will cost every turn, forever, against the one-off pile
    /// the `work_cost` names. Read it as the second half of the quote, never as a term of this form.
    ///
    /// Appended (append-only).
    #[serde(default)]
    pub build_work_per_worker_turn: f32,
    /// **What this patch's at-risk meter will lose on the next decay pass**, in work units — the
    /// rung's own `meter_decay.per_turn` scaled by how short the keeping fell
    /// (`docs/plan_standing_upkeep.md` §4.6a). `0.0` for as long as the rung's grace still forgives
    /// the shortfall.
    ///
    /// **It is a forecast the sim can make exactly.** The pass it describes judges the supply this
    /// turn has just stamped, so the bleed is already determined — nothing the player does next turn
    /// can prevent it, and a positive value here cannot fail to arrive.
    ///
    /// **It is therefore not "what the meter just did".** On a turn the keeping is restored the meter
    /// still loses the *previous* turn's shortfall while this reads `0`, correctly: that loss is
    /// already spent and the next pass will take nothing. A readout wanting the turn's realised cost
    /// must read the meter.
    ///
    /// **It is what a build's closed form nets, and `upkeep_demand` is not.** A build crew supplies
    /// nothing toward the maintenance rate — the site's own crew owes that for every meter carrying
    /// work, at any fullness — so what eats a build is the ground going backwards under it. The rot
    /// does not vary with the build crew, so a compose sheet re-prices a *proposed* crew against it
    /// and lands on the sim's own answer for the committed one; the client cannot derive it, holding
    /// neither the grace state nor the rung's decay rate.
    ///
    /// `0` when the keeping covers the demand, inside the grace, and on a rung with no `meter_decay`.
    /// Appended (append-only).
    #[serde(default)]
    pub meter_rot_per_turn: f32,
    /// **WHY THE BAND'S BUILDERS ARE STUCK ON THIS SOURCE** — `""` whenever this source is not a
    /// blocked build, else a short lowercase cause key (`knowledge`, `escapement`, `no_crop`,
    /// `species_ceiling`, `rung_below`, `owned_by_other`, `site`, `ring_idle`, `undeclared`,
    /// `unworked`), on the free-form-string convention [`Self::ecology_phase`] already uses.
    ///
    /// **Read it beside [`Self::build_turns_remaining`]'s `-4`, never instead of it**: that field
    /// says the pool is stuck, this says which conjunct of the rung's own gate refused. The sim
    /// decides `eligible`, so the sim says why — a client re-deriving it would be a second producer
    /// of one verdict.
    ///
    /// **Carried down the queue with the sentinel**, and it rides the same winner as the three
    /// fields above. The `.fbs` comment on `buildBlockedReason` carries the whole key table.
    /// Appended (append-only).
    #[serde(default)]
    pub build_blocked_reason: String,
    /// **WHERE THE PLAYER SENT THIS SOURCE** — the queued entry's destination rung, as
    /// `<branch>:<id>`, or empty when no band has queued it. The entry retires when the source
    /// reaches this rung's **top**, not when an intermediate rung fills.
    /// Appended (append-only).
    #[serde(default)]
    pub build_destination_rung: String,
    /// **THE LEGS STILL TO LAY** ([`BuildLegState`]), in climb order, first-incomplete first. Empty
    /// when the source is not queued or has already arrived; the first entry is the leg in flight.
    /// Appended (append-only).
    #[serde(default)]
    pub build_legs: Vec<BuildLegState>,
    /// **WHAT THE GROUND HOLDS** — this tile's own forage `K`, off `forage.capacity_by_biome` through
    /// `forage::tile_forage_capacity`, with **no rung gain in it**.
    ///
    /// # THE SPLIT, AND WHY BOTH SHIP
    ///
    /// - [`Self::carrying_capacity`] is what the **patch** holds now — this number × the interpolated
    ///   `field_capacity_gain`. It moves when the player builds, so it is live state.
    /// - This is a pure function of the tile's **terrain**. No player action moves it, which is
    ///   exactly what makes it safe to render on a hex the player has only ever *remembered*.
    ///
    /// **The client picks between them on visibility**, and neither is redundant: dropping this one
    /// as "the same number" is wrong on any patch above rung 2, and dropping
    /// [`Self::carrying_capacity`] as "derivable" is wrong because the client holds neither the gain
    /// nor the position it interpolates on. Collapsing either re-opens the fog leak this field closed
    /// while keeping issue #462's *"both webs state their capacity on a remembered tile"* intact.
    ///
    /// It is **also the denominator the plant standing upkeep is quoted per**:
    /// [`Self::upkeep_demand`] and the `*_upkeep_demand` quote pair are
    /// `rate × tile_capacity / cultivation.capacity_per_tender` (`forage::patch_tender_loads`), so
    /// this is the term that explains why two patches on one rung are billed differently.
    ///
    /// `0` where the tile is not on the map, the same absent-means-nothing reading
    /// [`Self::composition`] takes there — never a fabricated capacity. Appended (append-only).
    #[serde(default)]
    pub tile_capacity: f32,
    /// **THE CARRYING CAPACITY THIS PATCH WILL HAVE AT THE RUNG ITS BUILD IS HEADING FOR** — the
    /// plant twin of [`HerdTelemetryState::build_destination_capacity`], which carries the whole
    /// rationale. `None` (→ [`crate::NO_BUILD_DESTINATION_CAPACITY`] on the wire) when no band has
    /// queued it.
    ///
    /// **Only rung 3 moves it on this web.** A `Cultivate` buys the ground a faster curve, not a
    /// denser one, so a Cultivate destination quotes exactly [`Self::carrying_capacity`] — an equal
    /// reading, honestly, and not a stale one. It is the `Sow` that lifts it by
    /// `cultivation.field_capacity_gain`. Appended (append-only).
    #[serde(default)]
    pub build_destination_capacity: Option<f32>,
    /// **WHAT RUNG THIS PATCH IS STANDING ON NOW** — `<branch>:<id>`, the same spelling
    /// [`Self::build_destination_rung`] uses. That field says where a *queued climb is headed* and is
    /// empty when nothing is queued; this one says **where the source is**, which every source has.
    ///
    /// It ships so a consumer can answer *"has this been improved, and what is above it"* without
    /// knowing either web's private booleans (`Self::rungQuotes[plant:tended].reached` + `Self::rungQuotes[plant:farm].reached` here, a
    /// float and a flag on the herd row), and so a third branch costs it nothing. Struck at capture
    /// by `core_sim`'s `forage::patch_rung_key`, the single home of the *"sown → field, cultivated →
    /// tended, else wild"* test, so it cannot disagree with the rung the sim resolves.
    ///
    /// **Never empty** — the bottom rung is a rung, and an untouched patch reads `plant:wild`. The
    /// `.fbs` comment on `currentRung` carries the rest, including the client's fog obligation.
    /// Appended (append-only).
    #[serde(default)]
    pub current_rung: String,
    /// **THE WHOLE MATERIAL PILE THE RUNG THE `⌃` TRACK WOULD OFFER SWALLOWS**, per material id
    /// (`docs/plan_standing_upkeep.md` §2.7) — the second currency beside that rung's `*_work_cost`.
    ///
    /// `work_cost`'s own rule: resolved at capture off the **ladder** and published whether or not a
    /// build is in flight, because a compose sheet has to quote a price before the player commits.
    /// The pile is drawn **in proportion to the work banked** rather than on completion, so a rung a
    /// third raised has swallowed a third of this.
    ///
    /// **Empty is "no row", never zero** — every rung on the shipped ladder but `animal:pen`.
    #[serde(default)]
    pub build_material_cost: Vec<MaterialPayoff>,
    /// **WHAT HOLDING THIS SOURCE'S OWN RUNG SWALLOWS PER TURN**, per material — interpolated on the
    /// position and scaled by the **same** `scaled_by` the work demand reads, so a pen holding twice
    /// the herd mends twice the fence. It is the **stamped** bill the keeping was judged against,
    /// exactly as `upkeep_demand` beside it is.
    #[serde(default)]
    pub upkeep_material_demand: Vec<MaterialPayoff>,
    /// **WHAT THE BAND'S STORE ACTUALLY PAID** toward it, per material — the good-side twin of
    /// `upkeep_supplied`. Both terms are published rather than their difference, on the work trio's
    /// own rule.
    ///
    /// ⛔ **A SHORTFALL OF EITHER KIND TRIPS THE SAME GRACE.** There is one neglect counter and one
    /// `upkeep.grace_turns`; the decay rides the **worst** of the work fraction and each good's, and
    /// the amounts are never summed — a full store must not paper over missing hands.
    #[serde(default)]
    pub upkeep_material_supplied: Vec<MaterialPayoff>,
    /// **The crew hands spent keeping this source this turn** (`docs/plan_site_crews.md` §2.1) —
    /// fractional, summed across every band whose crew keeps it. The row's `keeps N of M` reads it;
    /// `0` on a source that owes nothing or that nobody works.
    #[serde(default)]
    pub upkeep_hands: f32,
    /// **The band-wide settlement filled less than this site's keeping-tool claim**
    /// (`docs/plan_site_crews.md` §2.3) — its crew kept with more of its own hands. The row's `ⓘ`.
    #[serde(default)]
    pub upkeep_tools_short: bool,
    /// **Every rung of this source's branch above the root, quoted** - one [`RungQuoteState`] per
    /// rung in climb order. Replaces the per-step fields (`cultivation*`, `tended*`, `field*`,
    /// `sow*`, `tame*`, `corral*`, `pastoral*`, `domestication`): a rung is a row keyed by its
    /// `rung_key`, never a field, so a new step is a config edit. Appended last.
    #[serde(default)]
    pub rung_quotes: Vec<RungQuoteState>,
}

/// **"This working does not run out"** — the wire value of [`DepositState::turns_remaining`] on a
/// deposit whose ground **renews** (`regrowth_rate > 0`).
///
/// It is *not applicable* rather than *unknown*: a renewing working's warning is the
/// sustainable-versus-actual pair beside it (`docs/plan_extraction.md` §7), so a client reads the
/// other readout here rather than drawing an empty runway.
///
/// **Which sentence a working publishes is decided by the RATE, never by the branch** — surface
/// flint and a quarry are both `extraction` and only one of them runs out.
pub const DEPOSIT_RUNWAY_NOT_APPLICABLE: i32 = -1;

/// **"Nobody is cutting this working"** — the wire value of [`DepositState::turns_remaining`] on a
/// **finite** working with no take this turn, so there is no rate to carry forward.
///
/// It is **not** [`DEPOSIT_RUNWAY_NOT_APPLICABLE`], and the difference is what the player is being
/// told: this working *will* run out, just not while it stands idle. Sits outside the `>= 0` range a
/// real count lives in, beside its sibling, so no reader has to guess which negative it is looking
/// at.
pub const DEPOSIT_RUNWAY_NO_TAKE: i32 = -2;

/// **ONE ROW PER DEPOSIT-BEARING TILE** — the wire row of `extraction::DepositSource`, keyed by
/// `(tile, material)` because **one tile can hold two**: a wooded highland holds timber *and* rock,
/// and working one is not working the other.
///
/// **A row describes the GROUND, and the working is its state** — the forage patch row's shape. The
/// capture sweeps every **discovered** tile and publishes a row for each `(tile, material)` whose
/// capacity is above nothing; where a band has opened a working the registry's live source is
/// merged in, and where none stands the row is **derived** from the branch's opening state (full
/// stock, the free floor, no upkeep, no take) without seeding the registry.
///
/// ⛔ **So the absence of a row means *"this ground holds none of this material"***, or the viewer
/// has not explored the tile — **never** *"nobody has worked it"*. And the presence of one implies
/// nothing about a crew: it does not say the ground has ever been cut, that the working stands
/// above its free floor, or that any working's keeping bill was met. Those are
/// [`Self::ladder_position`], [`Self::rung`], [`Self::build_fraction`] and the upkeep fields, each
/// of which reads its opening default on untouched ground.
///
/// Published for the tiles the viewing faction has **explored** (the road row's `Discovered` gate
/// rather than the herd row's `Active`): a deposit does not wander off, so remembering one is
/// remembering something true, and the whole row passes the gate or none of it does.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DepositState {
    /// **The row's identity, both halves.** `tile_x`/`tile_y` alone cannot join this row to the crew
    /// standing on it — see [`crate::state::population::LaborAssignmentState::material`].
    pub tile_x: u32,
    pub tile_y: u32,
    /// The `extraction.json` deposit this working is on: `wood`, `stone`, and whatever the minerals
    /// arc adds.
    pub material: String,
    /// **Which LADDER works this deposit** — `"forestry"` or `"extraction"`, the branch's wire
    /// form. It is **not** what decides the readout; see [`Self::regrowth_rate`].
    pub branch: String,
    /// **What is standing here right now**, in the material's own units — the only *saved* thing
    /// about a working. It opens at capacity, is drawn down by the take and put back by the growth
    /// term.
    pub stock: f32,
    /// **What this ground holds when full** (`extraction::tile_deposit_capacity`), read **live off
    /// the tile** at capture rather than stored on the source. **No rung may raise it**: it is the
    /// terrain's, which is what keeps the floor below from climbing out from under a build.
    pub capacity: f32,
    /// **What the CREWS ON THIS WORKING can actually get at** (`extraction::deposit_reachable`) —
    /// the stock above the **composed** floor, and the numerator of [`Self::turns_remaining`].
    ///
    /// ⛔ **IT IS NEVER SIMPLY [`Self::stock`].** A rung that cannot reach the whole seam leaves
    /// stock it cannot take, and climbing the extraction ladder is precisely how you reach deeper —
    /// so a reader that rendered the stock as *"what you can have"* would promise the player rock
    /// the crew cannot cut.
    ///
    /// ⛔ **AND IT CARRIES THE CREW'S FLOOR TOO** (issue #650): the sim stops at
    /// `max(rung_floor_fraction, floor) × capacity`, a **maximum** and never a sum. See
    /// [`Self::rung_floor_fraction`].
    pub reachable: f32,
    /// **THE RUNG'S OWN FLOOR**, as a fraction of [`Self::capacity`] — `1 − recovery_fraction`, what
    /// this rung's reach cannot get at (`extraction::deposit_floor_fraction`). Gathering recovers
    /// `0.15`, so it strands 85% of a rock body and this reads `0.85`.
    ///
    /// ⛔ **COMPOSE IT WITH THE PLAYER'S FLOOR AS A MAXIMUM, NEVER AS A SUM AND NEVER AS TWO
    /// CLAMPS.** Both are the same kind of quantity — *an amount left standing* — so a crew stops at
    /// whichever is greater. A chart that added them would draw a gathering crew stopping 85% of a
    /// seam short of where it really stops, on every rung.
    ///
    /// **The player's half of that max is not on this row**: it is
    /// [`crate::state::population::LaborAssignmentState::floor`], per **band row**, on the band's own
    /// `extract` row. A source-level restatement of where the stock came to rest rode here for one
    /// arc and was read by nobody; [`Self::reachable`] is the sim's own answer at the composed floor
    /// and is what a readout quotes instead.
    ///
    /// It is published because the escapement chart's whole axis is fractions of capacity and this
    /// is the **second** line on it: without it a client's projection would walk the stock down to
    /// the player's floor on ground the rung cannot reach past.
    pub rung_floor_fraction: f32,
    /// **What ONE cutter moves per turn at the rung this working holds**, in the material's own
    /// units — the deposit twin of [`ForagePatchState::per_worker_biomass`], **named after it**
    /// because the crew arithmetic it feeds (*clear it now* / *hold it after*) is the same division
    /// on every web.
    ///
    /// There is no seasonal weight and no take kit on either deposit branch, so unlike a patch's it
    /// is the rung's rate flat and is never `0` on a live rung.
    pub per_worker_biomass: f32,
    /// **This deposit's own per-turn regrowth, sampled across its capacity** — the third curve on the
    /// wire beside [`ForagePatchState::regrowth_samples`] and [`HerdTelemetryState::regrowth_samples`],
    /// on the same implicit x-axis: sample `i` of `n` is the one-turn delta at
    /// `stock = i/(n−1) × capacity`.
    ///
    /// ⛔ **A QUARRY'S ARE ALL ZERO, AND THAT IS AN ANSWER RATHER THAN AN ABSENCE.** Rock's rate is
    /// zero, so the delta is exactly `0` everywhere — *this does not grow*. An **empty** vector is
    /// the different claim *no curve was sent*, which is what a client blanks its chart on.
    ///
    /// **No sample is ever negative**: a deposit has no Allee term, so this is the plant curve's
    /// shape rather than the herd curve's.
    pub regrowth_samples: Vec<f32>,
    /// **The ground's own renewal rate** (`extraction::tile_deposit_regrowth`), already scaled by
    /// what this working's rung bought.
    ///
    /// ⛔ **THIS IS THE FIELD THAT DECIDES WHICH READOUT A CLIENT DRAWS**
    /// (`docs/plan_extraction.md` §7): `> 0` → the over-cut pair ([`Self::sustainable_take`] against
    /// [`Self::actual_take`]); `== 0` → the runway ([`Self::turns_remaining`]).
    ///
    /// ⛔ **IT IS NOT [`Self::branch`].** Surface flint and a quarry are the same skill and the same
    /// branch, and only one of them runs out; what differs is the *terrain's* rate, which is this.
    pub regrowth_rate: f32,
    /// The rung this working **holds**, as `"<branch>:<id>"` (`RungId::wire_key`). **This string is
    /// the bool** — a rung is never to be inferred by thresholding [`Self::build_fraction`], which
    /// is a different rung's meter. [`crate::state::routes::RouteState::rung`] carries the same
    /// rule.
    pub rung: String,
    /// The meter on the rung being **raised**, `0..=1`, off the shared
    /// `intensification::build_fraction` seam both food webs and the road publish theirs from.
    /// **Never derived by subtraction.** A working that has just completed a rung reads exactly
    /// `1.0`, and so does one at the top of its ladder.
    pub build_fraction: f32,
    /// **How far up its branch this working has been raised, in cumulative work units**
    /// (`DepositSource::ladder_position`) — the one meter both deposit branches carry, published so
    /// a ladder card can place the working on the *whole* branch rather than only within one rung.
    pub ladder_position: f32,
    /// **What a crew could take every turn FOR EVER and still have a wood** — the deposit's MSY, in
    /// the material's own units per turn: the growth term read at the peak of its curve
    /// (`MSY_BIOMASS_FRACTION × capacity`), which is `fauna::sustainable_yield`'s own expression with
    /// the deposit's curve substituted for the food web's. A full forage patch answers its MSY here
    /// too, which is what makes this the *existing* breakdown pointed at a new source.
    ///
    /// ⛔ **IT IS NOT THE GROWTH AT TODAY'S STOCK, AND READING IT THAT WAY MIS-FIRED ON CORRECT
    /// PLAY** (issue #650). A mature wood stands at `K`, where `(1 − S/K)` is zero, so the very first
    /// cut read as over-drawing — and never cleared, because the stock converges on the take from
    /// above. The `actual_take > sustainable_take` test is only meaningful against the MSY reading.
    ///
    /// ⛔ **`0` ON A FINITE DEPOSIT, AND THAT IS THE HONEST ANSWER RATHER THAN A GAP.** Rock's rate
    /// is zero, so the growth term is zero wherever it is read: there is no take a quarry can
    /// sustain, and what a finite working publishes instead is [`Self::turns_remaining`].
    pub sustainable_take: f32,
    /// **What this working actually paid out last turn**, summed over every band that cut it.
    ///
    /// sustainable-versus-actual is the **existing** intensification income breakdown
    /// (`docs/plan_intensification.md`) pointed at a new source, not a new readout: actual above
    /// sustainable is over-cutting, which is possible on purpose and **warned about rather than
    /// refused**.
    pub actual_take: f32,
    /// **How many turns this working lasts at the current take** — `floor(reachable / actual_take)`.
    ///
    /// ⛔ **A FORWARD PROJECTION, NEVER A TRAILING AVERAGE AND NEVER AN EMA** (the food-arrivals
    /// rule): it is this turn's rate carried forward, so it moves the turn the crew does.
    ///
    /// `>= 0` is a real count; [`DEPOSIT_RUNWAY_NOT_APPLICABLE`] (`-1`) and
    /// [`DEPOSIT_RUNWAY_NO_TAKE`] (`-2`) are the two ways of having no runway to quote.
    pub turns_remaining: i32,
    /// **The standing bill**, in work units per turn, kept first by the working's own `extract` crew. All
    /// three read the **stamped** basis (`DepositSource::upkeep_demanded`), so
    /// `demand − supplied == shortfall` holds verbatim on the wire.
    ///
    /// `0` on **both free floors** (`forestry:deadfall`, `extraction:gathering`), which declare no
    /// upkeep at all — nobody built them, so there is nothing to hold, and that is the whole of what
    /// makes a floor free.
    pub upkeep_demand: f32,
    /// See [`Self::upkeep_demand`].
    pub upkeep_supplied: f32,
    /// See [`Self::upkeep_demand`].
    pub upkeep_shortfall: f32,
    /// Whole keeping hands the bill wants — `ceil(demand / per-worker output)`. `0` for a
    /// working that owes nothing.
    pub upkeep_workers_needed: u32,
    /// **Whole workers the keeping takes once the rung in flight is finished** — equal to
    /// `upkeep_workers_needed` where no build is in flight (`docs/plan_site_crews.md`).
    #[serde(default)]
    pub upkeep_workers_at_completion: u32,
    /// **The game turn the keeping first needs one more whole worker than now**, projected along
    /// the build's own pace; [`NO_NEXT_KEEPING_WORKER`] = never within the build.
    #[serde(default = "no_next_keeping_worker")]
    pub upkeep_next_worker_turn: i32,
    /// **WHICH KEEPING TOOLS THIS SITE IS SHORT OF, BY NAME** — one
    /// [`crate::state::population::KitToeLineState`] per tool the site's keeping claimed this turn,
    /// summed over the crews keeping it. Short is `filled < required`, and
    /// `upkeep_tools_short` is exactly *"some line is short"*. Empty = the keeping claimed no tool.
    /// Appended last (append-only).
    #[serde(default)]
    pub upkeep_toe: Vec<crate::state::population::KitToeLineState>,
    /// **The builders on this source's declared build this turn**, the bare-handed and the kitted
    /// together (`docs/plan_labor_at_scale.md`, "Builds join the site's crew"). The viewer's own
    /// bands only. Appended last (append-only).
    #[serde(default)]
    pub builders_staffed: u32,
    /// Of [`Self::builders_staffed`], the ones holding the build kit the rung uses (every builder
    /// when the rung asks for no tool).
    #[serde(default)]
    pub builders_kitted: u32,
    /// **The most builders the build could take this turn** — the rung's `maxBuilders` cut by the
    /// room the tile left. `0` where no build is declared.
    #[serde(default)]
    pub builders_cap: u32,
    /// **The tools the build's builders asked for and what the settlement gave them**, one
    /// [`crate::state::population::PoolToeLineState`] per tool (`pool` reads `"build"`). Empty where
    /// the rung asks for no tool.
    #[serde(default)]
    pub build_toe: Vec<crate::state::population::PoolToeLineState>,
    /// `false` = **nothing at risk here** (a working on either free floor, which declares no
    /// upkeep). Read this before the countdown beside it.
    pub has_neglect_grace: bool,
    /// The **countdown**, not the counter: turns of shortfall left before the rung bleeds, `0` =
    /// sliding now, and a working whose bill is met reads its rung's full grace + 1.
    pub neglect_grace_remaining: u32,
    /// **The chained countdown** — everything above this entry in its band's queue, plus this
    /// entry's own span. The **same quantity with the same sentinels** a patch, a herd and a road
    /// publish, through the same seam: [`crate::NO_BUILD_TURNS_ESTIMATE`] (`-1`),
    /// [`crate::BUILD_METER_HOLDS`] (`-2`), [`crate::BUILD_METER_ROTS`] (`-3`),
    /// [`crate::BUILD_QUEUE_BLOCKED`] (`-4`), [`crate::BUILD_NOT_YET_ESTIMATED`] (`-5`); `>= 0` is a
    /// real count. [`ForagePatchState::build_turns_remaining`] carries the full table.
    pub build_turns_remaining: i32,
    /// **Why the pool is stuck on this working**, `""` when it is not — the same `BuildGate`
    /// vocabulary [`ForagePatchState::build_blocked_reason`] uses, two branches over.
    /// **Empty is not "fine"**: a working nobody has queued is not blocked, it is simply not being
    /// built.
    pub build_blocked_reason: String,
    /// **Is this working in some band's build queue right now?** — the membership flag, and the term
    /// that separates [`crate::BUILD_NOT_YET_ESTIMATED`] from [`crate::NO_BUILD_TURNS_ESTIMATE`].
    pub is_queued: bool,
    /// **The `extract` kit a crew on this working is sent with when nobody names one** — derived
    /// from the roster for the rung the working holds (`extraction::working_default_kit`), the same
    /// id `assign_labor` stores on a row that named no kit: the tightest of [`Self::offered_kit_ids`]
    /// for that rung (`EquipmentConfig::deposit_kit_for`). `"sledding"` on deadfall and gathering,
    /// `"woodcutting"` on felling and coppice, `"stonework"` on the quarry; the job default
    /// (`"none"`) only where no single kit fits tightest.
    pub default_kit_id: String,
    /// **The `extract` kits a crew on this working may be sent with**, in roster order — every take
    /// kit whose items each declare `deposit_take` on the working's branch
    /// (`extraction::working_offered_kits`). The picker lists these beside the itemless `none`.
    #[serde(default)]
    pub offered_kit_ids: Vec<String>,
    /// **The crew hands spent keeping this source this turn** (`docs/plan_site_crews.md` §2.1) —
    /// fractional, summed across every band whose crew keeps it. The row's `keeps N of M` reads it;
    /// `0` on a source that owes nothing or that nobody works.
    #[serde(default)]
    pub upkeep_hands: f32,
    /// **The band-wide settlement filled less than this site's keeping-tool claim**
    /// (`docs/plan_site_crews.md` §2.3) — its crew kept with more of its own hands. The row's `ⓘ`.
    #[serde(default)]
    pub upkeep_tools_short: bool,
}

/// **NO HAND WAS SPENT KEEPING** — `upkeep_hands`' reading on a source nobody keeps.
pub const NO_UPKEEP_HANDS: f32 = 0.0;

/// **The keeping needs no more whole workers within the build** — the wire's `-1` on
/// `upkeepNextWorkerTurn`: nothing in flight, the build is not dated, or no whole-worker step
/// before it completes.
pub const NO_NEXT_KEEPING_WORKER: i32 = -1;

/// The serde default of `upkeep_next_worker_turn` — [`NO_NEXT_KEEPING_WORKER`], never a `0` that
/// would read as a turn.
fn no_next_keeping_worker() -> i32 {
    NO_NEXT_KEEPING_WORKER
}

impl Default for DepositState {
    fn default() -> Self {
        Self {
            tile_x: 0,
            tile_y: 0,
            material: String::new(),
            branch: String::new(),
            stock: 0.0,
            capacity: 0.0,
            reachable: 0.0,
            rung_floor_fraction: 0.0,
            per_worker_biomass: 0.0,
            regrowth_samples: Vec::new(),
            regrowth_rate: 0.0,
            rung: String::new(),
            build_fraction: 0.0,
            ladder_position: 0.0,
            sustainable_take: 0.0,
            actual_take: 0.0,
            // **The wire's own default, not `0`** — a zero here reads as *"it runs out this turn"*,
            // which is the one thing a defaulted row must not say.
            turns_remaining: DEPOSIT_RUNWAY_NOT_APPLICABLE,
            upkeep_demand: 0.0,
            upkeep_supplied: 0.0,
            upkeep_shortfall: 0.0,
            upkeep_workers_needed: 0,
            upkeep_workers_at_completion: 0,
            upkeep_next_worker_turn: NO_NEXT_KEEPING_WORKER,
            upkeep_toe: Vec::new(),
            builders_staffed: 0,
            builders_kitted: 0,
            builders_cap: 0,
            build_toe: Vec::new(),
            has_neglect_grace: false,
            neglect_grace_remaining: 0,
            // Same rule one field over: `0` renders as a finished build.
            build_turns_remaining: NO_BUILD_TURNS_ESTIMATE,
            build_blocked_reason: String::new(),
            is_queued: false,
            default_kit_id: String::new(),
            offered_kit_ids: Vec::new(),
            upkeep_hands: NO_UPKEEP_HANDS,
            upkeep_tools_short: false,
        }
    }
}

/// **ONE LEG OF A QUEUE ENTRY'S CLIMB** — a rung still to raise, and what it owes on that rung **from
/// where the source stands now** (`docs/plan_standing_upkeep.md` §2.8).
///
/// A queue entry names a **destination**, not a rung, so it lays every leg between the source's
/// position and where the player sent it. `sow` on untended ground is two legs and costs the whole
/// branch.
///
/// **The client must not re-derive these**: the rung spans are the sim's config, the position is the
/// sim's state, and [`Self::turns_remaining`] is chained against a queue the client cannot see.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildLegState {
    /// The rung, as `<branch>:<id>` — `plant:tended`, `plant:farm`.
    pub rung: String,
    /// Work still owed **on this leg**, from the source's current position — never the rung's full
    /// span. A patch 30 units into a Cultivate owes `20` here: a previous improvement is a receipt,
    /// not a discount.
    pub work_remaining: f32,
    /// Turns until this leg is done, **chained** exactly as `build_turns_remaining` is, so the last
    /// leg's number equals the entry's own. Carries the same negative vocabulary.
    #[serde(default = "no_build_turns_estimate")]
    pub turns_remaining: i32,
}

impl Default for BuildLegState {
    fn default() -> Self {
        Self {
            rung: String::new(),
            work_remaining: 0.0,
            turns_remaining: crate::NO_BUILD_TURNS_ESTIMATE,
        }
    }
}

/// The serde default of a `build_turns_remaining` field — [`crate::NO_BUILD_TURNS_ESTIMATE`], so an
/// absent value reads as *"no estimate"* rather than as a build finishing this turn.
fn no_build_turns_estimate() -> i32 {
    crate::NO_BUILD_TURNS_ESTIMATE
}

/// **One material a commitment would pay, and how much of it per turn** — a row of
/// [`FloraShareInfo::rungQuotes[plant:farm].materialPayoff`] / [`FloraShareInfo::rungQuotes[plant:tended].materialPayoff`].
///
/// **A vector of these rather than one scalar is the whole point.** It replaced
/// `sow_trade_payoff` / `cultivate_trade_payoff` (arc #527), which answered *"how much trade"* — a
/// number a market could total and a player could not act on. This answers *"0.29 fibre"*, which is
/// what a cash crop **is**. Do not sum them back into one figure for display: that is the retired
/// trade axis under a new name.
///
/// It carries **no quality reading**, deliberately: a rating is a characteristic vector on the batch
/// the harvest creates ([`MaterialBatchState`](crate::MaterialBatchState)), and a picker row asks the
/// flat question *"how much of what"*.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MaterialPayoff {
    /// The `materials.json` id — `fibre`, `tobacco`, `grape`. Resolved for display against the
    /// material catalogue this snapshot already ships.
    pub material_id: String,
    /// Units of that material per turn, at this rung, on this tile.
    pub amount: f32,
}

/// **ONE RUNG'S QUOTE ON ONE SOURCE** - what the rung `rung_key` would cost and pay for the patch or
/// herd that carries it, whether or not a build is in flight (`snapshot.fbs` -> `RungQuoteState`).
/// A source row carries one per rung of its branch above the root, so a step is a row and never a
/// field.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RungQuoteState {
    /// The rung's `branch:id` wire key - the join to `LadderRungState::rung_key`.
    pub rung_key: String,
    /// Whether the source stands at or above this rung.
    pub reached: bool,
    /// The rung's own build meter, `0..=1` (`work_done / work_cost`); `1.0` once `reached`.
    pub progress: f32,
    /// The meter in work units.
    pub work_done: f32,
    /// What the rung costs ON THIS SOURCE, in work units.
    pub work_cost: f32,
    /// The materials the rung's build pile eats at full coverage; empty = none.
    pub build_material_cost: Vec<MaterialPayoff>,
    /// What holding the rung costs per turn, in work, at this source's own measure.
    pub upkeep_demand: f32,
    /// The materials holding the rung eats per turn; empty = none.
    pub upkeep_material_demand: Vec<MaterialPayoff>,
    /// Food per turn the source pays once at this rung.
    pub food_yield: f32,
    /// Fodder per turn the source pays once at this rung (`0` on an animal).
    pub fodder: f32,
    /// The materials of the source's yield vector once at this rung (empty on a patch row).
    pub material_payoff: Vec<MaterialPayoff>,
    /// Why this ground will not take the rung; `""` when it will.
    pub site_refusal: String,
}

/// **THE QUOTE OF A RUNG A SOURCE SAYS NOTHING ABOUT** - every number zero, nothing reached. What
/// [`RungQuoteState::of`] answers for a root rung or a key off the branch, so a reader asking *"how
/// much work does this rung cost?"* gets a number and never has to branch on a missing row.
pub static NO_RUNG_QUOTE: RungQuoteState = RungQuoteState {
    rung_key: String::new(),
    reached: false,
    progress: 0.0,
    work_done: 0.0,
    work_cost: 0.0,
    build_material_cost: Vec::new(),
    upkeep_demand: 0.0,
    upkeep_material_demand: Vec::new(),
    food_yield: 0.0,
    fodder: 0.0,
    material_payoff: Vec::new(),
    site_refusal: String::new(),
};

/// **ONE LESSON-KEYED SITE RULE ON A RUNG** (`snapshot.fbs` -> `SiteRelaxationState`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SiteRelaxationState {
    /// The knowledge that selects this rule.
    pub knowledge: String,
    /// The rule a faction holding it is judged by instead of the rung's own.
    pub rule: SiteRuleState,
}

/// **ONE SIZE TIER OF AN ENCLOSED RUNG** (`snapshot.fbs` -> `SizeTierState`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SizeTierState {
    /// The tier's id (`ranch`).
    pub id: String,
    /// What the feed and panels call a source in this tier.
    pub label: String,
    /// The lower-case phrase with its article.
    pub label_fallback: String,
    /// The fewest extension rings the fence must have reached.
    pub min_rings: u32,
    /// How many times more head a keeper minds (the keeping's work term only).
    pub keeper_ratio_gain: f32,
    /// The lesson keeping a source in this tier teaches; empty = none.
    pub earns_knowledge: String,
}

/// **ONE LESSON'S EFFECT ON A HOLDER'S HERDS** (`snapshot.fbs` -> `KnowledgeGainState`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct KnowledgeGainState {
    /// The lesson.
    pub knowledge: String,
    /// The multiplier on the managed regrowth rate.
    pub regrowth_gain: f32,
}

/// **A SITE RULE** (`snapshot.fbs` -> `SiteRuleState`): what a tile must be for a rung to stand on it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SiteRuleState {
    /// The tile must be a gathering site.
    pub requires_gathering_site: bool,
    /// The fertility floor (`0` = none).
    pub min_forage_capacity: f32,
    /// The tile must be fresh-watered.
    pub requires_fresh_water: bool,
}

impl RungQuoteState {
    /// **The quote for `rung_key` (`branch:id`) among a source's quotes**, or `None` when the source
    /// has nothing to say about that rung (it is the branch's root, or off the branch).
    pub fn find<'a>(quotes: &'a [RungQuoteState], rung_key: &str) -> Option<&'a RungQuoteState> {
        quotes.iter().find(|quote| quote.rung_key == rung_key)
    }

    /// [`Self::find`], answering [`NO_RUNG_QUOTE`] for a rung the source says nothing about.
    pub fn of<'a>(quotes: &'a [RungQuoteState], rung_key: &str) -> &'a RungQuoteState {
        Self::find(quotes, rung_key).unwrap_or(&NO_RUNG_QUOTE)
    }

    /// **The quote for `rung_key`, inserting a blank one if the source has none** - how a fixture
    /// states one rung's numbers without rebuilding the whole vector.
    pub fn entry<'a>(
        quotes: &'a mut Vec<RungQuoteState>,
        rung_key: &str,
    ) -> &'a mut RungQuoteState {
        let at = quotes
            .iter()
            .position(|quote| quote.rung_key == rung_key)
            .unwrap_or_else(|| {
                quotes.push(RungQuoteState {
                    rung_key: rung_key.to_string(),
                    ..RungQuoteState::default()
                });
                quotes.len() - 1
            });
        &mut quotes[at]
    }
}

impl ForagePatchState {
    /// This patch's quote for `rung_key` ([`RungQuoteState::of`]).
    pub fn quote(&self, rung_key: &str) -> &RungQuoteState {
        RungQuoteState::of(&self.rung_quotes, rung_key)
    }

    /// A mutable quote for `rung_key`, inserted blank if absent ([`RungQuoteState::entry`]).
    pub fn quote_mut(&mut self, rung_key: &str) -> &mut RungQuoteState {
        RungQuoteState::entry(&mut self.rung_quotes, rung_key)
    }
}

impl HerdTelemetryState {
    /// This herd's quote for `rung_key` ([`RungQuoteState::of`]).
    pub fn quote(&self, rung_key: &str) -> &RungQuoteState {
        RungQuoteState::of(&self.rung_quotes, rung_key)
    }

    /// A mutable quote for `rung_key`, inserted blank if absent ([`RungQuoteState::entry`]).
    pub fn quote_mut(&mut self, rung_key: &str) -> &mut RungQuoteState {
        RungQuoteState::entry(&mut self.rung_quotes, rung_key)
    }
}

/// One named plant's share of a tile's forage capacity — see [`ForagePatchState::composition`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FloraShareInfo {
    /// The stable config key (`flora_config.json` → `species`), e.g. `"hazel"`.
    pub species: String,
    /// The player-facing name, e.g. `"Hazel"` — shipped because the client holds no roster.
    pub display_name: String,
    /// This plant's fraction of the tile's basket, `0..1`.
    pub share: f32,
    // **RETIRED: `cultivate_trade_payoff`** (arc #527), and off the wire — see `sow_trade_payoff`
    // above for the gap.
    /// **What this plant is for** — the species' own `role` (`flora_config.json` → `species`):
    /// `"staple" | "fodder" | "cash"`. A **display tag**: nothing in the sim branches on it and
    /// nothing on a client may either — the yield vector is the behaviour, and this only names which
    /// component of it dominates, so a tile card can show one icon per crop.
    ///
    /// `""` means **unstated** (a species the roster no longer knows), *not* `"staple"` — the same
    /// convention [`Self::display_name`] carries, and a client must not default a missing tag into a
    /// real category.
    ///
    /// **Not derivable from the payoffs above**: those are rung-2/rung-3 numbers that fold in the
    /// weeding and conversion gains rather than stating the plant's own vector, and they are all `0`
    /// for a species that cannot climb here — exactly the `Wild`-ceiling case where the role is still
    /// true. Appended (append-only).
    #[serde(default)]
    pub role: String,
    /// **This plant's quote at every rung of the plant branch above the root** - one
    /// [`RungShareQuoteState`] per rung in climb order, replacing the per-step `cultivate*` /
    /// `sow*` fields. Appended last.
    #[serde(default)]
    pub rung_quotes: Vec<RungShareQuoteState>,
}

/// **ONE PLANT'S QUOTE AT ONE RUNG** (`snapshot.fbs` -> `RungShareQuoteState`): what committing the
/// tile to this plant would be at the rung `rung_key`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RungShareQuoteState {
    /// The rung's `branch:id` wire key.
    pub rung_key: String,
    /// Whether the plant can ever climb the rung (its `cultivation_ceiling`).
    pub can_climb: bool,
    /// Food per turn a completed rung of this plant pays on this tile.
    pub food_payoff: f32,
    /// That payoff over the tile's wild payoff; `0` = cannot climb here.
    pub yield_ratio: f32,
    /// The fodder account at the rung.
    pub fodder_payoff: f32,
    /// The material account at the rung; empty for a food crop.
    pub material_payoff: Vec<MaterialPayoff>,
    /// What building the rung with this crop costs; `0` where the plant cannot climb to it here.
    pub work_cost: f32,
}

/// **THE QUOTE OF A RUNG A PLANT SAYS NOTHING ABOUT** - every number zero, nothing climbable. What
/// [`FloraShareInfo::quote`] answers for the root or a key off the branch.
pub static NO_RUNG_SHARE_QUOTE: RungShareQuoteState = RungShareQuoteState {
    rung_key: String::new(),
    can_climb: false,
    food_payoff: 0.0,
    yield_ratio: 0.0,
    fodder_payoff: 0.0,
    material_payoff: Vec::new(),
    work_cost: 0.0,
};

impl FloraShareInfo {
    /// This plant's quote for `rung_key`, or `None` for the root or a key off the branch.
    pub fn rung_quote(&self, rung_key: &str) -> Option<&RungShareQuoteState> {
        self.rung_quotes
            .iter()
            .find(|quote| quote.rung_key == rung_key)
    }

    /// [`Self::rung_quote`], answering [`NO_RUNG_SHARE_QUOTE`] where the plant has nothing to say.
    pub fn quote(&self, rung_key: &str) -> &RungShareQuoteState {
        self.rung_quote(rung_key).unwrap_or(&NO_RUNG_SHARE_QUOTE)
    }

    /// A mutable quote for `rung_key`, inserted blank if absent - how a fixture states one rung.
    pub fn quote_mut(&mut self, rung_key: &str) -> &mut RungShareQuoteState {
        let at = self
            .rung_quotes
            .iter()
            .position(|quote| quote.rung_key == rung_key)
            .unwrap_or_else(|| {
                self.rung_quotes.push(RungShareQuoteState {
                    rung_key: rung_key.to_string(),
                    ..RungShareQuoteState::default()
                });
                self.rung_quotes.len() - 1
            });
        &mut self.rung_quotes[at]
    }
}

/// **ONE LADDER KNOWLEDGE THE CONFIG DECLARES** — the *roster* half, once per world and deliberately
/// faction-independent.
///
/// ⛔ **EVERY FIELD IS DERIVED FROM `intensification_ladder.json`; NOTHING HERE IS AUTHORED
/// SEPARATELY.** The branch and the order are the rung that *teaches* the knowledge
/// (`earns_knowledge`), and [`Self::is_step`] is simply whether any rung's `unlock_knowledge` names
/// it. That is what lets a client's knowledge screen build its own columns: a knowledge added to the
/// ladder appears with no client edit and no schema edit, which the retired field-per-knowledge
/// shape could never do.
///
/// **It carries no faction, and the progress half does.** A faction that has learned nothing has no
/// ledger row at all, so a roster carried on that row would leave a new player's screen empty — with
/// nothing on it to say there is anything to learn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LadderKnowledgeState {
    /// The ladder config's own id (`"cultivation"`, `"roadbuilding"`, …) — the join key with
    /// [`LadderKnowledgeProgress::knowledge_id`].
    pub knowledge_id: String,
    /// `"Seed Selection"` — the id with underscores turned to spaces and each word capitalized,
    /// resolved sim-side so no client authors a second spelling of it.
    pub display_name: String,
    /// The branch of the rung that **teaches** it (`"plant"` | `"animal"` | `"route"`): which column.
    pub branch: String,
    /// …and that rung's `order`: the position within the column, bottom step first.
    pub order: u32,
    /// **Does any rung's `unlock_knowledge` name this?** A *step* in the chain when it does; a
    /// *capability* hanging off the bottom of its column when it does not. `foddering` is the
    /// shipped `false` — it changes what a pen may draw on rather than opening a further rung.
    #[serde(default)]
    pub is_step: bool,
    /// **The subject area of the branch that teaches it** (`"food"` | `"making"` | `"works"`): which
    /// *heading*, one level above [`Self::branch`]. Off `intensification_ladder.json`'s `branches`
    /// table and never a client-side list — a hard-coded area table is the retired `LADDER_DOMAINS`
    /// bug one level up.
    ///
    /// `""` when the branch's descriptor names none, which the client draws under a fallback
    /// heading rather than dropping the row.
    #[serde(default)]
    pub area: String,
}

/// One faction's progress on one ladder knowledge, `0..1` (`1.0` = known). Joined to the roster above
/// by [`Self::knowledge_id`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LadderKnowledgeProgress {
    pub knowledge_id: String,
    #[serde(default)]
    pub progress: f32,
}

/// Per-faction intensification-ladder knowledge: the faction's progress on each of the ladder's
/// knowledges, 0..1 (1.0 = known). Mirrors `SedentarizationState`'s per-faction shape; the client
/// renders learning/known meters. Only factions that have begun learning **something** appear here —
/// what there *is* to learn rides [`LadderKnowledgeState`] instead, precisely so a faction absent
/// from this list still has a screen to look at.
///
/// ⛔ **THE FIVE NAMED FLOATS ARE RETIRED** and off the wire. Adding a knowledge used to mean adding a schema field, which is
/// why the route branch's two lessons had nowhere to appear; [`Self::knowledges`] is the list that
/// replaced them and is the only authority on a faction's ladder progress.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IntensificationKnowledgeState {
    pub faction: u32,
    /// **Every knowledge the ladder declares**, in the roster's own order. Sparse in *value*, never
    /// in *membership*: a knowledge the faction has not begun reads `0` rather than being absent, so
    /// no reader has to tell *not begun* from *not published*.
    #[serde(default)]
    pub knowledges: Vec<LadderKnowledgeProgress>,
}

/// **ONE RUNG OF ANY BRANCH, AS THE CONFIG DECLARES IT** - the ladder's *catalog*, once per world and
/// carrying no tile, no source and no faction. It replaced the route and deposit catalogs (and
/// gave the plant and animal rungs one): a row per rung of `intensification_ladder.json`, grouped
/// by branch and in climb order within it. Every field is derived from the config through the sim's
/// own rung types, so a rung or a branch added there appears with no client edit and no schema edit.
///
/// **It rides the section and not a source row**: these are properties of the *branch*, identical
/// for every source on it. See `snapshot.fbs`'s `LadderRungState` for each field's contract; the
/// neutral values (`friction_multiplier` 1, `regrowth_multiplier` 1) are the wire defaults, so a
/// defaulted row never claims a rung stops the ground growing or that a road is free.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LadderRungState {
    /// The rung's wire key, `"<branch>:<id>"`.
    pub rung_key: String,
    /// The branch the rung is on; joins to [`LadderBranchState::branch`].
    pub branch: String,
    /// The config record's `order`: 1 at the branch's floor.
    pub order: u32,
    /// The rung id title-cased by the sim.
    pub display_name: String,
    /// The rung's player-facing label ("Tended patch").
    #[serde(default)]
    pub label: String,
    /// The article-bearing feed fallback ("a tended patch").
    #[serde(default)]
    pub label_fallback: String,
    /// The build verb that raises the rung, `""` where it declares none.
    pub verb: String,
    /// The kind of source on the rung's branch: `"patch"` | `"herd"` | `"road"` | `"deposit"` | `"tile"`.
    #[serde(default)]
    pub source_kind: String,
    /// The rung's trait names, in declaration order.
    #[serde(default)]
    pub traits: Vec<String>,
    /// The knowledge that gates the rung, `""` where none.
    pub unlock_knowledge: String,
    /// The knowledge the rung earns by being held, `""` where none.
    #[serde(default)]
    pub earns_knowledge: String,
    /// The wire key of the rung directly beneath, `""` at the floor.
    pub requires_rung: String,
    /// Work units to reach the rung, before per-source scaling.
    #[serde(default)]
    pub work_cost: f32,
    /// **How many hands can build this rung at once** (`docs/plan_labor_at_scale.md`) - the rung's
    /// `build.max_builders`; `0` on a rung with nothing to build. Appended last (append-only).
    #[serde(default)]
    pub max_builders: u32,
    /// Work units per turn a source at the rung owes its keeper, before per-source load.
    #[serde(default)]
    pub upkeep_work_per_turn: f32,
    /// The declared build pile, read with [`Self::build_material_id`].
    #[serde(default)]
    pub build_material_cost: f32,
    /// Which material the pile is of, `""` where the rung eats nothing.
    #[serde(default)]
    pub build_material_id: String,
    /// What one bare-handed worker banks per turn (the sim's rate, read never assumed).
    #[serde(default)]
    pub build_work_per_worker_turn: f32,
    /// Road payoff: the fraction of base pooling friction a haul pays (1.0 = no help).
    #[serde(default = "default_friction_multiplier")]
    pub friction_multiplier: f32,
    /// Road payoff: how far a tile at the rung holds a pooling link open.
    #[serde(default)]
    pub holds_link_to_tiles: u32,
    /// Road payoff: whether a road at the rung lights its tile.
    #[serde(default)]
    pub grants_sight: bool,
    /// Deposit payoff: what one worker takes per turn at the rung.
    #[serde(default)]
    pub yield_per_worker_turn: f32,
    /// Deposit payoff: how much of the deposit the rung can ever reach, 0..1.
    #[serde(default)]
    pub recovery_fraction: f32,
    /// Deposit payoff: what the rung multiplies the ground's regrowth by.
    #[serde(default = "default_regrowth_multiplier")]
    pub regrowth_multiplier: f32,
    /// Deposit site rule: the capacity the ground must hold for the rung to be placed there.
    #[serde(default)]
    pub min_deposit_capacity: f32,
    /// The rung directly beneath must already be held (`requires_held_rung_below`).
    pub requires_held_rung_below: bool,
    /// A plant rung needs a crop committed to the patch (`requires_crop`).
    pub requires_crop: bool,
    /// The stock must leave room to be worked (`requires_work_room`).
    pub requires_work_room: bool,
    /// The source must be this people's or unowned (`requires_ownership`).
    pub requires_ownership: bool,
    /// The rung places a source on bare ground rather than upgrading one (`places_source`).
    pub places_source: bool,
    /// The rung's own site rule (all false / `0` = it asks nothing of the site).
    pub site_rule: SiteRuleState,
    /// The lesson-keyed rules that loosen it; the last one a faction holds replaces the rung's own.
    pub site_relaxations: Vec<SiteRelaxationState>,
    /// The derived size tiers an enclosed rung declares (a pen grown to `min_rings` is a Ranch).
    pub size_tiers: Vec<SizeTierState>,
    /// What the lessons the rung names give the herds that hold it.
    pub knowledge_gains: Vec<KnowledgeGainState>,
    /// The belief the tile must hold for the rung to be placed there
    /// (`site_requirement.min_belief`); `0` = it asks nothing of the tile's belief.
    #[serde(default)]
    pub min_belief: f32,
}

/// The wire's own default for [`LadderRungState::friction_multiplier`]: no help.
pub const LADDER_RUNG_FRICTION_NEUTRAL: f32 = 1.0;

/// **"THIS RUNG LEAVES THE GROUND GROWING AS IT WAS"** - the wire default of
/// [`LadderRungState::regrowth_multiplier`]. A zero there is the one value that means *this rung
/// stops the ground growing*, a claim no shipped rung makes.
pub const DEPOSIT_RUNG_REGROWTH_UNCHANGED: f32 = 1.0;

fn default_friction_multiplier() -> f32 {
    LADDER_RUNG_FRICTION_NEUTRAL
}

fn default_regrowth_multiplier() -> f32 {
    DEPOSIT_RUNG_REGROWTH_UNCHANGED
}

impl Default for LadderRungState {
    fn default() -> Self {
        Self {
            rung_key: String::new(),
            branch: String::new(),
            order: 0,
            display_name: String::new(),
            label: String::new(),
            label_fallback: String::new(),
            verb: String::new(),
            source_kind: String::new(),
            traits: Vec::new(),
            unlock_knowledge: String::new(),
            earns_knowledge: String::new(),
            requires_rung: String::new(),
            work_cost: 0.0,
            max_builders: 0,
            upkeep_work_per_turn: 0.0,
            build_material_cost: 0.0,
            build_material_id: String::new(),
            build_work_per_worker_turn: 0.0,
            friction_multiplier: LADDER_RUNG_FRICTION_NEUTRAL,
            holds_link_to_tiles: 0,
            grants_sight: false,
            yield_per_worker_turn: 0.0,
            recovery_fraction: 0.0,
            regrowth_multiplier: DEPOSIT_RUNG_REGROWTH_UNCHANGED,
            min_deposit_capacity: 0.0,
            requires_held_rung_below: false,
            requires_crop: false,
            requires_work_room: false,
            requires_ownership: false,
            places_source: false,
            site_rule: SiteRuleState::default(),
            site_relaxations: Vec::new(),
            size_tiers: Vec::new(),
            knowledge_gains: Vec::new(),
            min_belief: 0.0,
        }
    }
}

/// **ONE BRANCH OF THE LADDER, AS THE CONFIG DECLARES IT** - once per world, beside
/// [`LadderRungState`]: the grouping a client draws a ladder under.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LadderBranchState {
    /// The branch id; joins to [`LadderRungState::branch`].
    pub branch: String,
    /// The subject area the branch files under (`ladder_areas` vocabulary).
    pub area: String,
    /// The kind of source on the branch.
    pub source_kind: String,
    /// The branch's name in player copy.
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FoodModuleState {
    pub x: u32,
    pub y: u32,
    pub module: String,
    pub seasonal_weight: f32,
    pub kind: String,
}

/// **One named kit a party may be sent out with** (`equipment.json`'s `kits`) — a **mask** over the
/// item table (spears / sled / baskets / traps), published once per world so the client's picker
/// renders real numbers without a second copy of the TOE table.
///
/// The tiers are what this kit grants a party whose items are all **fresh**. What a given band's
/// *wear* then does to them is that band's own row (`PopulationCohortState`'s `hunter_attack` /
/// `hunt_carry_per_worker_biomass` / `forage_carry_per_worker_biomass`).
///
/// **`none` is an ordinary roster entry, not a sentinel**: it grants nothing, so its tiers are the
/// unequipped ones throughout and a party sent with it spends no durability on any item. No consumer
/// should special-case its id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KitOptionState {
    pub id: String,
    pub display_name: String,
    /// Which verbs this kit may be sent on — any of `"hunt"`, `"forage"`, `"scout"`, `"warrior"`. A
    /// kit named for a job outside this list is a **command failure**, never a silent fall back to
    /// the default, so a picker must filter by the job it is composing.
    ///
    /// **The band-wide roles have a kit axis now.** Scout and Warrior used to list no kit on the
    /// grounds that they consumed no component; the expanded roster gave them one each
    /// (`wayfinding` → `["scout"]`, `warrior` → `["warrior"]`), and `none` lists all four, so going
    /// bare is a real selection on every role rather than the only reading.
    pub jobs: Vec<String>,
    /// A fresh-kit hunter's combat `attack` under this kit — what the gate
    /// `max(0, attack − defense)` compares against a herd's `defense`. **Below a species' defense
    /// that species cannot be hunted at all**, which is why `none` is a real decision rather than a
    /// discount.
    pub attack: f32,
    /// Per-hunter HUNT haul rate (biomass/turn) under this kit — the sled's tier.
    pub hunt_carry_per_worker_biomass: f32,
    /// Per-gatherer throughput (biomass/turn, **before** the tile's seasonal weight) under this kit —
    /// the baskets' tier.
    pub forage_carry_per_worker_biomass: f32,
    /// The sight range each posted scout vantage reveals at under this kit — the wayfinding gear's
    /// tier. How far the vantages are *posted* is not a kit axis.
    #[serde(default)]
    pub scout_vantage_range: f32,
    /// **The range of quarry [`Self::attack`] applies to**, by body mass. `0` on either end means
    /// unbounded. Outside the range the kit grants no attack at all and the party falls back to the
    /// bare hand's, so the ordinary `max(0, attack − defense)` gate refuses the hunt.
    #[serde(default)]
    pub attack_min_body_mass: f32,
    /// See [`Self::attack_min_body_mass`].
    #[serde(default)]
    pub attack_max_body_mass: f32,
    /// **What this kit multiplies the quarry's own `wariness` by** at the retreat — `1.0` leaves the
    /// species' flight response alone, `0` means nothing breaks off at contact.
    ///
    /// It is a *multiplier*, so a picker must not render it as a flat "scares N% away": the same kit
    /// costs a jumpy gazelle (`wariness 0.85`) almost its whole engagement and a mammoth (`0.10`)
    /// nearly nothing. Pair it with the herd's own reading to say anything about a specific hunt.
    #[serde(default = "multiplier_neutral")]
    pub dispersion: f32,
    /// **What this kit multiplies the hunt's baseline injury hazard by.** `1.0` is neutral; `0` is a
    /// stand-off kit whose users are never in reach of the animal, and which therefore pays its cost
    /// in durability instead of in people.
    #[serde(default = "multiplier_neutral")]
    pub exposure: f32,
    /// **WHICH ITEMS THIS KIT ACTUALLY CARRIES** — the `equipment.json` kit's `uses` list verbatim,
    /// in config order (`big_game` → `["spears", "sled"]`, `trapping` → `["traps", "sled"]`).
    ///
    /// **It exists because the tiers above do not say what produced them.** A kit's `attack` is a
    /// number; nothing on the wire said which *item* granted it, so a durability readout had to guess
    /// — and the guess was a hardcoded `attack → "spears"`, which told a Trapping-kit party it
    /// carried spears and quoted the spears' condition instead of the traps'. Two kits with the same
    /// attack tier are indistinguishable to a consumer without this list.
    ///
    /// **Config order is meaningful and is preserved**: the weapon comes first, the haul aid after,
    /// so a consumer rendering the list reads as the roster does. An **empty** list is a real answer
    /// (`none` carries nothing and wears nothing), never "unknown".
    #[serde(default)]
    pub item_ids: Vec<String>,
    /// **RETIRED — it publishes [`crate::RETIRED_BUILD_RATE`] and nothing else.** It carried a
    /// *multiplier* on the crew's build output; the stat is now an **additive per-worker contribution
    /// per equipped worker per turn** ([`Self::build_work_per_worker`] beside it). The field is held
    /// at its neutral rather than removed because a client still calls its accessor.
    ///
    /// **ITS SUCCESSOR IS WHY A PICKER MUST ASK ACROSS EVERY AXIS**, and that argument transfers
    /// verbatim. The husbandry gear's other axis was read on a corralled herd and nowhere else, so a
    /// picker testing that axis alone withheld the kit on the very herd the player was taming —
    /// which is the work hurdles and halters are physically for. A consumer deciding whether to
    /// offer a kit must ask what the kit can change on *this* source across **every** axis it
    /// declares, never off one hardcoded key — and this is the axis it must now read as
    /// [`Self::build_work_per_worker`].
    #[serde(default = "multiplier_neutral")]
    pub build_rate: f32,
    /// **The extra work ONE EQUIPPED WORKER carrying this kit delivers per turn on a build**
    /// (`docs/plan_standing_upkeep.md` §4.8). Neutral `0.0`; **hoes are `+0.5` per worker per turn on
    /// the plant web and hurdles `+0.5` on the animal one**.
    ///
    /// **It supersedes [`Self::build_rate`]**, which is retired and now publishes only its neutral:
    /// that stat multiplied the *crew's output* and this is an addend on the same account, which is
    /// what lets a tool state its worth in the game's own work unit. It is **summed over the equipped
    /// workers**, never averaged over the crew.
    ///
    /// ⛔ **THE UNITS CHANGED.** It shipped as *"work units taken **off the job**, summed over the
    /// crew"* at `8.5` (`docs/plan_unit_costed_work.md` §6); a job's work requirement never changes
    /// now, so `0.5` is a 50% uplift on what each builder delivers and not half a work unit off a
    /// 50-unit job. The `8.5 -> 0.5` is an exact unit conversion, not a retune.
    #[serde(default)]
    pub build_work_per_worker: f32,
    /// **WHICH FOOD WEB [`Self::build_work_per_worker`] IS FOR** — `"plant"` or `"animal"`, and
    /// **`""`** when this kit carries no build tool at all.
    ///
    /// **The pair is one reading.** A hoe adds work to a Cultivate and *nothing* to a `Tame`, so
    /// a picker must compare this against the branch of the build it is offering the kit for and
    /// grey the kit where they disagree — the same discipline
    /// [`Self::attack_max_body_mass`] imposes on [`Self::attack`], and for the same reason: the
    /// number is real, it is simply not real *here*.
    ///
    /// A free-form string on the `species` / `ecology_phase` convention. Appended (append-only).
    #[serde(default)]
    pub build_work_branch: String,
    /// **WHICH RUNG OF THAT WEB [`Self::build_work_per_worker`] IS FOR** — a `"<branch>:<id>"` rung
    /// key such as `"route:paved_road"`, and **`""`** when the kit's build tool serves *every* rung
    /// on its branch, which is every kit that ships today but the two road tools.
    ///
    /// ⛔ **THE READING IS A TRIPLE — worth, branch AND rung, or none of them.** A consumer reading
    /// the worth alone is wrong, and one reading worth + branch is wrong on exactly the branch this
    /// field was added for: the route ladder is the first whose rungs want *different* tools, so a
    /// picker must grey a road kit against the **rung** of the entry in front of it rather than
    /// against its branch.
    ///
    /// **It has to be here and not only on [`crate::state::BandKitTiersState`].** That is the band's
    /// **resolved** row; this is the **roster** the picker reads, so a client without this field
    /// sees an unbound tool on both road kits and answers with whichever the roster lists first —
    /// offering the *grading* kit for a `pave`. An absent field decodes as `""` = *serves every
    /// rung*, the generous answer the bound exists to refuse. Appended (append-only).
    #[serde(default)]
    pub build_work_rung: String,
    /// **The per-turn observation radius a DETACHED PARTY carrying this kit maps the ground at** —
    /// what the launch sheet's gear line quotes. Appended (append-only).
    ///
    /// ⛔ **NOT [`Self::scout_vantage_range`] READ TWICE.** A posted vantage and a whole ranging
    /// party are different observers with different **bare** readings — the vantage's bare is `1`
    /// tile, the party's is `6` — so a sheet quoting the vantage's number would tell a bare ranging
    /// party it sees one tile when it sees six. The `wayfinding` item lifts both, which is why one
    /// kit row carries both numbers.
    #[serde(default)]
    pub expedition_sight_range: f32,
}

/// **Hand-written rather than derived, for the same reason [`HerdTelemetryState`]'s is**: three of
/// these fields are multipliers whose neutral is `1`, and a `#[derive(Default)]` would answer `0` — the
/// value that means *this kit scares nothing and exposes nobody*, i.e. the passive device's entire
/// advantage handed out by omission. `serde`'s missing-field default is spelled separately on each
/// field, so this impl is what keeps the two agreeing with the schema's `= 1`.
impl Default for KitOptionState {
    fn default() -> Self {
        Self {
            id: String::new(),
            display_name: String::new(),
            jobs: Vec::new(),
            attack: 0.0,
            hunt_carry_per_worker_biomass: 0.0,
            forage_carry_per_worker_biomass: 0.0,
            scout_vantage_range: 0.0,
            expedition_sight_range: 0.0,
            // `0` is the *sentinel* on these two — "unbounded", the schema's own default and what
            // every weapon but the passive device ships. Not a multiplier, so not neutral-at-one.
            attack_min_body_mass: 0.0,
            attack_max_body_mass: 0.0,
            dispersion: multiplier_neutral(),
            exposure: multiplier_neutral(),
            // An empty carry list, matching the schema's absent vector — the `none` kit's honest
            // reading, and the only safe one: inventing an item here would attribute wear to gear
            // the kit does not hold.
            item_ids: Vec::new(),
            build_rate: multiplier_neutral(),
            build_work_per_worker: 0.0,
            // Empty is the honest reading of a kit with no build tool — naming a web by omission
            // would price a build off gear the kit does not hold.
            build_work_branch: String::new(),
            // Empty is *"bound to no rung"*, which is what every kit but the two road tools
            // declares — so the default is also the shipped answer nearly everywhere.
            build_work_rung: String::new(),
        }
    }
}

/// **One rung of the shared rating vocabulary** — `poor · fair · good · excellent`, ascending.
///
/// Published **once for the world**, not per material: it is one vocabulary, and every published
/// reading already carries its own band name, so a copy per material row would be a second home for
/// one fact. This is the legend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CharacteristicBandState {
    pub name: String,
    /// The reading at which this band opens. The first is `0.0` and the seams strictly ascend, so
    /// every reading in `0..=1` selects exactly one band.
    pub from: f32,
}

/// **One material the world contains** — a row of `SubsistenceSnapshot::materials`, the per-world
/// catalogue. A `Whole` baseline like the kit roster: re-sent only on a world rebuild.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MaterialDefState {
    pub id: String,
    /// The knowledge track that works it — `hide` → `tanning`. One craft per material.
    pub craft: String,
    /// The axes it is rated on, **in the order** a batch's readings are keyed by. The order is part
    /// of the contract, not presentation.
    pub axes: Vec<String>,
    /// Whether it can be worked with **no tool at all**. `false` is the whole refusal mechanism for
    /// a material with no bench tool present: the rate is `0` and nothing branches.
    pub hand_workable: bool,
    /// Bench progress multiplier bare-handed; `0` when not hand-workable.
    pub hand_working_rate: f32,
    /// The best reading a bare-handed craft can realize — fine flax with no loom still makes a
    /// standard basket.
    pub hand_working_quality_ceiling: f32,
    /// The equipment item that **bounds** this material at the bench, or `""` when the roster has
    /// none. It is what the *"No loom"* refusal names.
    pub tool_item_id: String,
}

/// One input row of a published recipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RecipeInputState {
    pub material_id: String,
    pub amount: f32,
    /// The **one** characteristic this recipe judges, `""` on every other input row. It is the whole
    /// of what separates two recipes over the same material.
    pub reads_axis: String,
}

/// One output row of a published recipe. Exactly one of the two ids is set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RecipeOutputState {
    pub equipment_id: String,
    pub material_id: String,
    pub amount: f32,
}

/// **One recipe in the book** — a row of `SubsistenceSnapshot::recipes`, the per-world catalogue.
/// The band-relative half (can it be made, what would it come out at, what is missing) is the
/// cohort's own `craft_offers`; this is the static half.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RecipeDefState {
    pub id: String,
    /// **The output's name** — the item's own `display_name`, or the material's title — shared by
    /// every recipe making that output. [`Self::label`] is the recipe's own word.
    pub display_name: String,
    pub craft: String,
    /// `kit` | `tool` | `stock` — the same three groups the ledger is drawn in.
    pub group: String,
    /// Worker-turns one pass costs.
    pub work: f32,
    /// Empty on every ordinary kit recipe. **Tools are earned, never a prerequisite**: a tool recipe
    /// is gated on the crafts of what it is *made from*, never on the craft it unlocks.
    pub requires_knowledge: Vec<String>,
    pub inputs: Vec<RecipeInputState>,
    pub outputs: Vec<RecipeOutputState>,
    /// **The recipe's short name among the recipes making the same output** — *Bone*, *Flint*. `""`
    /// on a recipe that is the only one making its output.
    pub label: String,
}

/// **One faction's standing in one craft.** The lesson is charged **per item completed**, so this
/// meter moves when a bench delivers, not when a turn passes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CraftKnowledgeState {
    pub faction: u32,
    pub craft_id: String,
    /// `Bone-working` — the id, hyphenated and capitalized, resolved sim-side so the client never
    /// maps an id to English.
    pub display_name: String,
    pub known: bool,
    pub progress: f32,
    /// What [`Self::progress`] has to reach. Published so the client draws no scale of its own.
    pub completion_threshold: f32,
}

/// `TileState::graze_ecology_phase` — the biome carries no pasture at all (water, ice, bare rock).
/// Deliberately the zero/default value: an absent reading must never masquerade as a healthy one.
pub const GRAZE_PHASE_NONE: u8 = 0;

/// `TileState::graze_ecology_phase` — pasture at or above the stressed band (healthy).
pub const GRAZE_PHASE_THRIVING: u8 = 1;

/// `TileState::graze_ecology_phase` — pasture drawn down into the stressed band (overgrazed).
pub const GRAZE_PHASE_STRESSED: u8 = 2;

/// `TileState::graze_ecology_phase` — pasture stripped below the collapse band (severely overgrazed;
/// it still recovers — grass reseeds — but slowly).
pub const GRAZE_PHASE_COLLAPSING: u8 = 3;

/// **NO OUTPUT COMMITMENT IS IN FLIGHT** — the sentinel
/// [`HerdTelemetryState::standing_output_target`] carries when the player has ordered nothing.
///
/// **It is negative on purpose.** `0.0` is a real order — *"go back to all meat"* — so it cannot
/// double as *"no order"*, and a client that tested `target == 0.0` for absence would hide exactly
/// the commitment a dairy herd's owner is waiting on.
pub const NO_OUTPUT_COMMITMENT_IN_FLIGHT: f32 = -1.0;

fn no_output_commitment_in_flight() -> f32 {
    NO_OUTPUT_COMMITMENT_IN_FLIGHT
}
