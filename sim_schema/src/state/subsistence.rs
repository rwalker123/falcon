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
/// empty `husbandry` role: the hunters draw the flock to their floor, the unmet keeping suppresses
/// its regrowth, and the `Tame`'s own escapement gate never reopens. Adding builders does nothing.
/// A surface showing this must therefore pair it with the source's own `upkeepShortfall` /
/// `neglectGraceRemaining`, because *"staff the keeping"* is the sentence — on the animal web,
/// `assign_labor <faction> <band> husbandry <n>`.
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
    #[serde(default)]
    pub domestication: f32,
    /// Intensification Rung 1c corral state: `true` iff the herd is penned (`corralled_at.is_some()`).
    #[serde(default)]
    pub corralled: bool,
    /// Pen-construction progress 0..1 (`1.0` = penned) while a keeper works the herd under the
    /// **Corral** policy. The animal twin of `ForagePatchState::cultivation_progress`.
    #[serde(default)]
    pub corral_progress: f32,
    /// Pre-commit yield forecast at the herd's current biomass (food/turn, `output_multiplier = 1`).
    /// The per-worker **rate**; the per-policy ceilings that clamp it live in
    /// [`Self::hunt_policy_ceilings`].
    #[serde(default)]
    pub per_worker_yield: f32,
    // **RETIRED: `per_worker_trade`** (arc #527). The wire slot `perWorkerTrade` is `(deprecated)`
    // in place. A band preview that needs a *crew* number on an inedible species reads
    // [`Self::per_worker_biomass`], which is positive there and always was.
    /// Food/turn the herd will pay **once penned** (the corral's managed harvest at its current
    /// biomass). With the `corral` row of [`Self::hunt_policy_ceilings`] (what the herd pays *while*
    /// the pen is being built), lets the client show "preparing X → then Y" pre-commit.
    /// **Gross and net both**: a pen's feed is grass and hay, so nothing is subtracted from this in
    /// food.
    #[serde(default)]
    pub corral_yield: f32,
    // **RETIRED: `corral_trade`** (arc #527). The wire slot `corralTrade` is `(deprecated)` in place.
    // **RETIRED: `pen_upkeep`** — `pen.upkeep_per_biomass × biomass`, the FOOD a pen drew from its
    // keeper's larder, quoted for every herd so a pre-commit `Corral` row could subtract the running
    // cost from the payoff. **Human food is not animal feed**: a pen eats the grass its fenced
    // footprint grows and the hay its keeper carries in, so there is no food-unit running cost left to
    // quote. The wire slot `penUpkeep` is `(deprecated)` in place. What a pen still demands is
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
    /// [`Self::tame_work_done`] / [`Self::corral_work_done`] carry, **not** a `0..1` fraction. It
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
    // **RETIRED: `trade_per_animal`** (arc #527). The wire slot `tradePerAnimal` is `(deprecated)`
    // in place. A kill rhythm on an inedible species divides body mass by the herd's own biomass
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
    /// **The Tame rung's payoff** — food/turn a Sustain hunt pays once this herd is tamed (the
    /// pastoral MSY at the herd's current biomass), the pastoral twin of [`Self::corral_yield`]. Its
    /// `ceilingTame` sibling (in [`Self::hunt_policy_ceilings`]) is Tame's *during-building* dip; this
    /// is what the herd pays *after* taming, so the client renders Tame as `→ +Y` (like
    /// Cultivate/Sow/Corral) instead of only the dip. `0` on a herd that never offers Tame (already
    /// penned, or a `wild`-ceiling species). Appended last (append-only).
    #[serde(default)]
    pub pastoral_yield: f32,
    // **RETIRED: `pastoral_trade`** (arc #527). The wire slot `pastoralTrade` is `(deprecated)` in
    // place.
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
    // it is no longer a per-pen wire field. The wire slot `penHayNeed` is `(deprecated)` in place.
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
    // Both die with the larder feed: the split is grass and hay, in fodder. The wire slots
    // `penLarderBill` / `penHayFood` are `(deprecated)` in place.
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
    // factor. The wire slots `tameBuildFraction` / `corralBuildFraction` stay `(deprecated)`.
    // **RETIRED: `maintain`** — a per-source boolean toggle. *"Stop maintaining this"* is
    // `maintain <faction> hunt <herd> 0`: a flag beside a crew count would be a second way to say
    // what the number already says, and the two could disagree.
    /// **What holding this herd's rung DEMANDS this turn**, in work units. Follows `corral_yield`'s
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
    // **RETIRED: `trade_per_biomass`** (arc #527). The wire slot `tradePerBiomass` on this table is
    // `(deprecated)` in place.
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
    /// **What the Corral rung would pay, per material** (arc #527) — the material twin of
    /// [`Self::corral_yield`] and the replacement for the retired `corral_trade`. Without it an
    /// **inedible** quarry's Corral rung quotes nothing at all: a wolf's `corral_yield` is honestly
    /// `0`, so the compose sheet's *"→ then +Y"* had no number.
    ///
    /// Priced on the **same** pen MSY biomass its food sibling is, so a rung's two readouts cannot
    /// describe different harvests. **Gross** like `corral_yield` — the pen's feed is a provisions
    /// debit and never touches it. **Empty is "no row"**, including on a herd that never offers the
    /// rung. Appended (append-only).
    #[serde(default)]
    pub corral_material: Vec<MaterialPayoff>,
    /// The **Tame** rung's twin of [`Self::corral_material`], priced on the pastoral MSY biomass
    /// [`Self::pastoral_yield`] reads its provisions from. Appended (append-only).
    #[serde(default)]
    pub pastoral_material: Vec<MaterialPayoff>,
    /// **The build, PRICED IN WORK** (`docs/plan_unit_costed_work.md` §8). An improvement costs a
    /// fixed amount of work now, not a fixed number of turns: a crew produces work units per turn
    /// (head count × floor discipline × kit) and **turns are the output**.
    ///
    /// `work_done` is the source's own meter, in work units. `work_cost` is what that job costs **on
    /// this source**, resolved off the ladder at capture and published **whether or not a build is in
    /// flight** — that is the point, since the compose sheet must quote the price *before* the player
    /// commits. The `*_progress` fraction beside it is exactly `work_done / work_cost`. Appended
    /// (append-only).
    ///
    /// The tame pair carries the **species' own** cost multiplier (a Steppe Runner is five times the
    /// work of a rabbit); the pen pair does not, because penning is a flat job for every species — a
    /// fence is a fence.
    #[serde(default)]
    pub tame_work_done: f32,
    /// See [`Self::tame_work_done`].
    #[serde(default)]
    pub tame_work_cost: f32,
    /// The rung-3 twin of [`Self::tame_work_done`].
    #[serde(default)]
    pub corral_work_done: f32,
    /// See [`Self::corral_work_done`].
    #[serde(default)]
    pub corral_work_cost: f32,
    /// **How many more turns a build on this source needs**, at the crew, floor and kit that worked
    /// it this turn — and a **PROJECTION when nothing is being built**, which is by definition the
    /// state a compose sheet is looking at. With an improvement in flight it counts down the running
    /// meter; with none it is what the rung this source would climb **next** would take the crew
    /// currently working it, from the work already banked on that rung. Always meaningful, never
    /// `-1`-because-unstarted — the same always-meaningful rule `corral_yield` follows.
    ///
    /// **Which `*_work_cost` it belongs beside** is the assignment's own `improvement`, and the
    /// **next rung up** when that is empty (`is_cultivated` / `is_field`, `domestication` /
    /// `corralled`): the pair is read as *"50 work, ≈13 turns"*, so they must name one rung.
    ///
    /// **IT IS A CHAINED DATE** (`docs/plan_standing_upkeep.md` §4.6b). The band's whole `builders`
    /// pool goes on the **head** of its queue until that entry's meter fills, then on the next — so
    /// a count here is *everything above this entry plus its own span at the full pool*, and
    /// [`Self::build_queue_position`] beside it is what makes that number explicable. A source with
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
    /// **WHAT EACH RUNG COSTS TO HOLD, PER TURN**, in work units — the **rate** half of the same
    /// quote [`Self::tame_work_cost`] / [`Self::corral_work_cost`] give the **pile** half of, and
    /// read beside them by the same rung-picking rule (the assignment's `improvement`, or the next
    /// rung up when that is empty).
    ///
    /// **It is not [`Self::upkeep_demand`], and the difference is the whole point.** That field
    /// answers *"what is this herd billed right now"* and resolves through the **keeping** rung — the
    /// one the herd stands on or is raising. A herd nobody has started taming stands on no managed
    /// rung, so it publishes an honest `0`, and a compose sheet quoting the Tame off it subtracts
    /// nothing and promises `work_cost / crew` turns for a build that will never move. This one
    /// answers *"what would the rung being quoted cost to hold"*, resolved at capture off the
    /// **ladder** whether or not a build is in flight — exactly the rule the `*_work_cost` beside it
    /// follows. On a herd mid-build the two agree; on an unstarted one they differ, and that
    /// difference was the trap.
    ///
    /// **Both carry this herd's own keeper load**, because both animal rungs quote their rate per
    /// keeper-load (`scaled_by: source_load`) — and the load is **ownership-independent**, like
    /// [`Self::herders_needed_if_managed`]: a quote has to exist before the herd is anyone's.
    /// Appended (append-only).
    #[serde(default)]
    pub tame_upkeep_demand: f32,
    /// The rung-3 twin of [`Self::tame_upkeep_demand`].
    #[serde(default)]
    pub corral_upkeep_demand: f32,
    /// **What this herd's at-risk meter will lose on the next decay pass**, in work units — the
    /// plant twin's field on the same rule, so a client's build estimate is one expression across
    /// both webs (`docs/plan_standing_upkeep.md` §4.6a).
    ///
    /// **It is always `0` on the shipped ladder, and that is not an omission**: neither animal rung
    /// declares a `meter_decay`, because an under-kept flock **sheds animals** instead. Nothing eats
    /// an animal build. Appended (append-only).
    #[serde(default)]
    pub meter_rot_per_turn: f32,
    /// **WHERE THIS SOURCE SITS IN THE WINNING BAND'S BUILD QUEUE** — 0-based, and
    /// [`crate::NOT_IN_ANY_BUILD_QUEUE`] (`-1`) when no band has queued it
    /// (`docs/plan_standing_upkeep.md` §4.6b).
    ///
    /// The whole `builders` pool goes on the **head** of a band's queue until that entry's meter
    /// fills, then on the next — so [`Self::build_turns_remaining`] beside it is a **chained** date:
    /// everything above this entry plus its own span at the full pool. **Without this field that
    /// date is an exact number with no explanation**, and the player cannot tell forty turns of work
    /// from eight turns of work behind four other jobs.
    ///
    /// **It rides the same winner** as [`Self::build_turns_remaining`] and
    /// [`Self::build_work_from_gear`]: several bands may work one source, the sooner estimate wins,
    /// and all three come from that band. Appended (append-only).
    ///
    /// **⛔ IT IS NOT THE RANK, and no band's queue may be ordered on it**
    /// (`docs/plan_standing_upkeep.md` §4.9 item 9a). It is a **source-addressed readout** of the
    /// *winning* band's place in the *winning* band's line — the map annotation, the tile card and
    /// the compose sheet all ask about the source and get the best answer anyone has. It is not an
    /// answer about any *particular* band, because the winner is routinely somebody else: a band's
    /// queue `[X, Y, Z]` sorted on this field came out `[Y, X, Z]` when a second band also held `Y`
    /// and had the sooner estimate, so the drag gesture took its insert index off a list that was
    /// not that band's.
    ///
    /// **[`crate::state::population::PopulationCohortState::build_queue`] is the rank** — the band's
    /// own entries in the band's own order, position being the vector index.
    #[serde(default = "not_in_any_build_queue")]
    pub build_queue_position: i32,
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
    /// **WHAT THIS HERD'S BUILD IS BEING RAISED WITH** — the animal twin of
    /// [`ForagePatchState::build_kit_id`], which carries the whole rationale: the wire states the
    /// **resolved** kit rather than the stored one, there is no second *"what the default would be"*
    /// field, and it is captured live off the winning band's queue. Appended (append-only).
    #[serde(default)]
    pub build_kit_id: String,
    /// **What the in-flight fence ring costs, in work units** — the *denominator* of
    /// [`Self::pen_extend_progress`], in the same units, so the pen ring ships the meter/pile pair
    /// every other build on this row already does.
    ///
    /// **It is the herd's STAMPED cost, not a live quote** — the one way it differs from
    /// [`Self::tame_work_cost`] / [`Self::corral_work_cost`], which resolve at capture so a compose
    /// sheet can price a build before the player commits. `Herd::accrue_pen_extension` stamps it on
    /// the first worked turn, so it reads `0.0` both with no ring in flight and on the turn a ring is
    /// begun. Read the pair together: `0.0 / 0.0` is *"no ring"*, never *"0%"*.
    /// Appended (append-only).
    #[serde(default)]
    pub pen_extend_cost: f32,
    /// **WHAT RUNG THIS HERD IS STANDING ON NOW** — the animal twin of
    /// [`ForagePatchState::current_rung`], which carries the rationale: `<branch>:<id>`, the
    /// [`Self::build_destination_rung`] spelling, saying where the source **is** rather than where a
    /// queued climb is headed.
    ///
    /// Struck at capture by `core_sim`'s `fauna::herd_rung_key` — the single home of the *"penned →
    /// pen, tamed → pastoral, else wild"* test this row otherwise makes a client re-derive from
    /// [`Self::domestication`] against a threshold plus [`Self::corralled`]. **Never empty**; an
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
    /// **WHAT EACH ANIMAL RUNG WOULD COST TO HOLD IN GOODS, PER TURN** — the material twin of the
    /// [`Self::tame_upkeep_demand`] / [`Self::corral_upkeep_demand`] pair, on the plant twin's own
    /// rules: the rung's **rate** rather than the stamped bill, resolved **live** at capture, and
    /// scaled by this herd's own keeper-load.
    ///
    /// ⛔ **THIS PAIR IS WHY THE FIELD EXISTS.** `tame_upkeep_material_demand` is always empty
    /// (`animal:pastoral` names no material) and `corral_upkeep_material_demand` names `hurdles` —
    /// and a **pastoral** herd is the only source a `⌃` track ever offers the Pen rung from. Its
    /// *current* rung declares nothing, so the stamped [`Self::upkeep_material_demand`] is empty on
    /// exactly the row where the player needs the number. Without this the material half of that
    /// aside is unreachable in play.
    #[serde(default)]
    pub tame_upkeep_material_demand: Vec<MaterialPayoff>,
    /// The rung-3 twin of [`Self::tame_upkeep_material_demand`], and the one that carries `hurdles`.
    #[serde(default)]
    pub corral_upkeep_material_demand: Vec<MaterialPayoff>,
    /// **THE WHOLE PILE A PEN RING SWALLOWS TO RAISE** — the material twin of
    /// [`Self::corral_work_cost`], resolved live off the **ladder** at capture and published
    /// whether or not a build is in flight, because the ring card quotes a price before the player
    /// commits.
    ///
    /// ⛔ **IT EXISTS FOR WORD-FOR-WORD THE REASON [`Self::corral_upkeep_material_demand`] DOES.**
    /// That field exists because a *pastoral* herd is the only source the `⌃` track ever offers the
    /// Pen rung from, and its own rung declares no material. This one exists because a *corralled*
    /// herd is the only source a **ring** is ever offered from, `animal:pen` is the top of its
    /// branch, and [`Self::build_material_cost`] — which publishes the rung *above* — is therefore
    /// empty on exactly the row the player is deciding on.
    ///
    /// **UNSCALED**, because the labor arm prices a ring's width at the pen rung's own unscaled
    /// build cost: penning takes no per-species multiplier, and a scaled quote would show one price
    /// and charge another. On a **pastoral** herd this equals [`Self::build_material_cost`] by
    /// construction — the same rung's pile reached through two selectors, never a second reading of
    /// the ladder.
    #[serde(default)]
    pub corral_build_material_cost: Vec<MaterialPayoff>,
    /// The animal twin of [`ForagePatchState::upkeep_kit_id`] — see it for the whole rationale.
    #[serde(default)]
    pub upkeep_kit_id: String,
    /// The animal twin of [`ForagePatchState::upkeep_kit_named`].
    #[serde(default)]
    pub upkeep_kit_named: bool,
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
            domestication: 0.0,
            corralled: false,
            corral_progress: 0.0,
            per_worker_yield: 0.0,
            corral_yield: 0.0,
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
            pastoral_yield: 0.0,
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
            tame_work_done: 0.0,
            tame_work_cost: 0.0,
            corral_work_done: 0.0,
            corral_work_cost: 0.0,
            build_turns_remaining: no_build_turns_estimate(),
            build_work_from_gear: 0.0,
            build_work_per_worker_turn: 0.0,
            // A herd nothing has described quotes no rung, so neither rate is owed yet.
            tame_upkeep_demand: 0.0,
            corral_upkeep_demand: 0.0,
            meter_rot_per_turn: 0.0,
            build_queue_position: crate::NOT_IN_ANY_BUILD_QUEUE,
            build_blocked_reason: String::new(),
            build_destination_rung: String::new(),
            build_legs: Vec::new(),
            // **A herd nothing has described is heading nowhere** — the absent reading, never a
            // capacity of zero.
            build_destination_capacity: None,
            // A herd in nobody's queue is being raised with nothing — the "not queued" reading, and
            // a different statement from the roster's own bare kit.
            build_kit_id: String::new(),
            pen_extend_cost: 0.0,
            // **A herd nothing has described names no rung** — the same "not described" reading
            // `build_destination_rung` above takes, and NOT a wire state: the capture strikes this
            // from `fauna::herd_rung_key` on every row, so an empty string only ever reaches a
            // hand-built fixture. Naming `animal:wild` here would put a second spelling of the
            // ladder in this crate, which is the duplication the field exists to remove.
            current_rung: String::new(),
            build_material_cost: Vec::new(),
            upkeep_material_demand: Vec::new(),
            upkeep_material_supplied: Vec::new(),
            tame_upkeep_material_demand: Vec::new(),
            corral_upkeep_material_demand: Vec::new(),
            corral_build_material_cost: Vec::new(),
            upkeep_kit_id: String::new(),
            upkeep_kit_named: false,
            corral_material: Vec::new(),
            pastoral_material: Vec::new(),
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
/// (Intensification Phase 1a). Keyed by tile `(x, y)`. `cultivation_progress` is the 0..1 taming
/// meter; `is_cultivated` = a completed tended patch. `owner` is the tending faction (`None` = a
/// wild/untended patch). `biomass`/`carrying_capacity`/`ecology_phase` let the client show patch
/// health. Mirrors `HerdTelemetryState`'s display-telemetry role for the plant side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ForagePatchState {
    pub x: u32,
    pub y: u32,
    #[serde(default)]
    pub cultivation_progress: f32,
    #[serde(default)]
    pub is_cultivated: bool,
    #[serde(default)]
    pub owner: Option<u32>,
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
    /// `is_field` and `field_progress` are already withheld on a remembered hex; publishing this one
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
    /// assignment*, before anything is committed:
    /// `expected(workers, policy) = min(workers × per_worker, ceiling)` and
    /// `max_useful_workers(policy) = ceil(ceiling / per_worker)`, **per account**, both terms read
    /// off [`ForagePolicyCeiling`] — the per-policy ROW that is now the patch's only wire
    /// representation of a ceiling, the six flat `ceiling_*` scalars it replaced having been removed
    /// (#426). This field survives as the FOOD per-worker term only, because a patch-level scalar
    /// cannot state a policy-dependent rate and the non-food accounts genuinely have one.
    /// Provisions/turn one forager contributes (this tile's seasonal weight folded in, as the take
    /// does); `0.0` in a dead season — do not divide by it, and **do not read it as "the wire said
    /// nothing"**: whether a patch was DESCRIBED is the row's presence, not this number's size.
    #[serde(default)]
    pub per_worker_yield: f32,
    /// Food/turn the patch will pay **once cultivated** (the tended harvest on its current standing
    /// crop). With `forage_policy_ceilings`' `cultivate` row, the client's "preparing X → then Y".
    #[serde(default)]
    pub tended_yield: f32,
    /// The per-patch **`plant:field` build meter**, `0..1` — the plant rung-3 twin of a herd's
    /// `corral_progress`. Independent of `cultivation_progress`: `Sow` needs no prior patch, so a
    /// Field may stand on ground that was never tended, and the client shows **two** meters.
    #[serde(default)]
    pub field_progress: f32,
    /// The completed rung 3 — a sown **Field**. Read this rather than inferring a rung from
    /// `field_progress`.
    #[serde(default)]
    pub is_field: bool,
    /// Food/turn the patch will pay **once sown** (the Field harvest on its current standing crop —
    /// 2× `tended_yield` on the shipped dials). With the `sow` row, Sow's "preparing X → then Y" pair.
    #[serde(default)]
    pub field_yield: f32,
    /// **Why this ground will not take seed** ([`SiteRefusal::as_str`]: `"too_poor"` / `"too_dry"` /
    /// `"too_poor_and_too_dry"`), or **`""`** when it will. Resolved through the same
    /// `RungSiteRequirement::refusal` seam the `sow` command and the labor arm gate on, so the wire
    /// cannot disagree with the gate. Shipped as an *answer* because the client can re-derive
    /// nothing: it holds neither the per-biome capacity table nor the hydrology.
    #[serde(default)]
    pub sow_site_refusal: String,
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
    // **RETIRED: `tended_trade`** (arc #527). The wire slot `tendedTrade` is `(deprecated)` in place.
    /// Fodder/turn a **completed tended patch** would pay. `0` unless its basket holds a fodder crop.
    #[serde(default)]
    pub tended_fodder: f32,
    // **RETIRED: `field_trade`** (arc #527). The wire slot `fieldTrade` is `(deprecated)` in place.
    // A cash Field's whole product is **material batches**, which this table cannot quote as a
    // per-turn number — see `MaterialBatchState`, which is what the band actually holds.
    /// Fodder/turn a **completed Field** would pay — the whole yield of a `hay_grass` Field.
    #[serde(default)]
    pub field_fodder: f32,
    // **RETIRED: `cultivate_build_fraction` / `sow_build_fraction`** — the plant twins of the animal
    // pair; see [`HerdTelemetryState`] for why the dip dissolved into the work budget. The wire slots
    // `cultivateBuildFraction` / `sowBuildFraction` stay `(deprecated)`.
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
    // §2.5), so there is no blended count for a rung-level floor to raise. The wire slots
    // `cultivateCrewNeeded` / `sowCrewNeeded` stay `(deprecated)`.
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
    // **RETIRED: `trade_per_biomass`** (arc #527). The wire slot `tradePerBiomass` on this table is
    // `(deprecated)` in place.
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
    /// **The build, PRICED IN WORK** (`docs/plan_unit_costed_work.md` §8). An improvement costs a
    /// fixed amount of work now, not a fixed number of turns: a crew produces work units per turn
    /// (head count × floor discipline × kit) and **turns are the output**.
    ///
    /// `work_done` is the source's own meter, in work units. `work_cost` is what that job costs **on
    /// this source**, resolved off the ladder at capture and published **whether or not a build is in
    /// flight** — that is the point, since the compose sheet must quote the price *before* the player
    /// commits. The `*_progress` fraction beside it is exactly `work_done / work_cost`. Appended
    /// (append-only).
    #[serde(default)]
    pub cultivation_work_done: f32,
    /// See [`Self::cultivation_work_done`].
    #[serde(default)]
    pub cultivation_work_cost: f32,
    /// The rung-3 twin of [`Self::cultivation_work_done`]. Two rungs keep two pairs, the
    /// `cultivate_build_fraction` / `sow_build_fraction` rule: independently tunable jobs must not
    /// share a number.
    #[serde(default)]
    pub field_work_done: f32,
    /// See [`Self::field_work_done`].
    #[serde(default)]
    pub field_work_cost: f32,
    /// **How many more turns a build on this source needs**, at the crew, floor and kit that worked
    /// it this turn — and a **PROJECTION when nothing is being built**, which is by definition the
    /// state a compose sheet is looking at. With an improvement in flight it counts down the running
    /// meter; with none it is what the rung this source would climb **next** would take the crew
    /// currently working it, from the work already banked on that rung. Always meaningful, never
    /// `-1`-because-unstarted — the same always-meaningful rule `corral_yield` follows.
    ///
    /// **Which `*_work_cost` it belongs beside** is the assignment's own `improvement`, and the
    /// **next rung up** when that is empty (`is_cultivated` / `is_field`, `domestication` /
    /// `corralled`): the pair is read as *"50 work, ≈13 turns"*, so they must name one rung.
    ///
    /// **IT IS A CHAINED DATE** (`docs/plan_standing_upkeep.md` §4.6b). The band's whole `builders`
    /// pool goes on the **head** of its queue until that entry's meter fills, then on the next — so
    /// a count here is *everything above this entry plus its own span at the full pool*, and
    /// [`Self::build_queue_position`] beside it is what makes that number explicable. A source with
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
    /// **WHAT EACH RUNG COSTS TO HOLD, PER TURN**, in work units — the **rate** half of the same
    /// quote [`Self::cultivation_work_cost`] / [`Self::field_work_cost`] give the **pile** half of,
    /// and read beside them by the same rung-picking rule (the assignment's `improvement`, or the
    /// next rung up when that is empty).
    ///
    /// **It is not [`Self::upkeep_demand`], and the difference is the whole point.** That field
    /// answers *"what is this patch billed right now"* and resolves through the **at-risk** rung —
    /// the newest meter carrying progress. A wild patch has progress on neither, so it publishes an
    /// honest `0`, and a compose sheet quoting a Cultivate off it subtracts nothing and promises
    /// `work_cost / crew` turns for a build that will never move. This one answers *"what would the
    /// rung being quoted cost to hold"*, resolved at capture off the **ladder** whether or not a
    /// build is in flight — exactly the rule the `*_work_cost` beside it follows. On a patch
    /// mid-build the two agree; on an unstarted one they differ, and that difference was the trap.
    ///
    /// **Both scale with the size of the land, through the same measure the bill uses.** Both plant
    /// rungs declare `scaled_by: source_load` and quote their rate **per tender-load** — the tile's
    /// own forage capacity over `forage.cultivation.capacity_per_tender` — so these are struck at
    /// capture off *this patch's* tile, exactly as [`Self::upkeep_demand`] is, and not off the
    /// ladder's bare rate. Appended (append-only).
    #[serde(default)]
    pub cultivation_upkeep_demand: f32,
    /// The rung-3 twin of [`Self::cultivation_upkeep_demand`].
    #[serde(default)]
    pub field_upkeep_demand: f32,
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
    /// nothing toward the maintenance rate — the keeping pool owes that for every meter carrying
    /// work, at any fullness — so what eats a build is the ground going backwards under it. The rot
    /// does not vary with the build crew, so a compose sheet re-prices a *proposed* crew against it
    /// and lands on the sim's own answer for the committed one; the client cannot derive it, holding
    /// neither the grace state nor the rung's decay rate.
    ///
    /// `0` when the keeping covers the demand, inside the grace, and on a rung with no `meter_decay`.
    /// Appended (append-only).
    #[serde(default)]
    pub meter_rot_per_turn: f32,
    /// **WHERE THIS PATCH SITS IN THE WINNING BAND'S BUILD QUEUE** — 0-based, and
    /// [`crate::NOT_IN_ANY_BUILD_QUEUE`] (`-1`) when no band has queued it
    /// (`docs/plan_standing_upkeep.md` §4.6b).
    ///
    /// The whole `builders` pool goes on the **head** of a band's queue until that entry's meter
    /// fills, then on the next — so [`Self::build_turns_remaining`] beside it is a **chained** date:
    /// everything above this entry plus its own span at the full pool. **Without this field that
    /// date is an exact number with no explanation**, and the player cannot tell forty turns of work
    /// from eight turns of work behind four other jobs.
    ///
    /// **It rides the same winner** as [`Self::build_turns_remaining`] and
    /// [`Self::build_work_from_gear`]: several bands may work one source, the sooner estimate wins,
    /// and all three come from that band. Appended (append-only).
    ///
    /// **⛔ IT IS NOT THE RANK, and no band's queue may be ordered on it**
    /// (`docs/plan_standing_upkeep.md` §4.9 item 9a). It is a **source-addressed readout** of the
    /// *winning* band's place in the *winning* band's line — the map annotation, the tile card and
    /// the compose sheet all ask about the source and get the best answer anyone has. It is not an
    /// answer about any *particular* band, because the winner is routinely somebody else: a band's
    /// queue `[X, Y, Z]` sorted on this field came out `[Y, X, Z]` when a second band also held `Y`
    /// and had the sooner estimate, so the drag gesture took its insert index off a list that was
    /// not that band's.
    ///
    /// **[`crate::state::population::PopulationCohortState::build_queue`] is the rank** — the band's
    /// own entries in the band's own order, position being the vector index.
    #[serde(default = "not_in_any_build_queue")]
    pub build_queue_position: i32,
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
    /// **WHAT THIS SOURCE'S BUILD IS BEING RAISED WITH** — the kit id the winning band's queue entry
    /// **resolves to**, and `""` when no band has this source queued
    /// (`docs/plan_standing_upkeep.md` §4.7a ②).
    ///
    /// # IT IS THE RESOLVED KIT, NEVER "the player named none"
    ///
    /// That is [`LaborAssignmentState::kit_id`](crate::LaborAssignmentState::kit_id)'s standing
    /// rule, and it matters more here: the builders' default is derived **per queue entry** from
    /// that entry's own food web — a hoe for a Cultivate, hurdles for a `Tame` — so an entry that
    /// named nothing would otherwise publish `""` while the pool was out with hurdles. An explicit
    /// bare-handed pick crosses as the roster's own bare kit id, which is a real selection and reads
    /// differently from the empty string.
    ///
    /// # THERE IS NO SECOND "what the default WOULD be" FIELD
    ///
    /// The client mirrors the same roster derivation (`KitRoster.build_kit_for_branch`) to draw its
    /// `(default)` mark, exactly as the hunt row does per quarry. A second field would be the same
    /// answer twice, from two producers.
    ///
    /// # CAPTURED LIVE, unlike [`Self::build_queue_position`] beside it
    ///
    /// The position is stamped by the turn; this is read off the band's live queue at capture. The
    /// server re-captures and broadcasts after **every** dispatched command, so a live read shows a
    /// kit pick immediately where a turn-written one would lag a whole turn.
    ///
    /// **It rides the same winning band** as the position: a kit taken from one band's queue beside
    /// a position from another's would be two answers pretending to be one. Appended (append-only).
    #[serde(default)]
    pub build_kit_id: String,
    /// **WHAT RUNG THIS PATCH IS STANDING ON NOW** — `<branch>:<id>`, the same spelling
    /// [`Self::build_destination_rung`] uses. That field says where a *queued climb is headed* and is
    /// empty when nothing is queued; this one says **where the source is**, which every source has.
    ///
    /// It ships so a consumer can answer *"has this been improved, and what is above it"* without
    /// knowing either web's private booleans ([`Self::is_cultivated`] + [`Self::is_field`] here, a
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
    /// **WHAT EACH PLANT RUNG WOULD COST TO HOLD IN GOODS, PER TURN** — the material twin of the
    /// [`Self::cultivation_upkeep_demand`] / [`Self::field_upkeep_demand`] pair, read beside them by
    /// the same rung-picking rule and quoted for the same reason: the `⌃` track's third aside prices
    /// the rung it is **offering**, before the player commits.
    ///
    /// ⛔ **IT IS THE RUNG'S RATE, NOT THE STAMPED BILL.** [`Self::upkeep_material_demand`] answers
    /// *"what was this source billed this turn"* through the rung it stands **on**; this answers
    /// *"what would this rung cost to hold"* for a rung it may not have reached. On a source
    /// mid-climb the two **disagree**, and that is correct — the work pair has exactly that
    /// relationship.
    ///
    /// **Resolved LIVE at capture** rather than off the stamp: the stamp exists to stop an
    /// interpolated demand moving across the Population→Logistics carry, and a pre-commit quote is
    /// carried nowhere. Reading the stamp would publish an empty list for every rung the source is
    /// not already on — which is every rung a track ever offers.
    ///
    /// **Scaled by the same tender-load measure the bill is**, since the rung reads one `scaled_by`
    /// for both currencies. **Empty is "no row", never zero** — which is both plant rungs today.
    #[serde(default)]
    pub cultivation_upkeep_material_demand: Vec<MaterialPayoff>,
    /// The rung-3 twin of [`Self::cultivation_upkeep_material_demand`].
    #[serde(default)]
    pub field_upkeep_material_demand: Vec<MaterialPayoff>,
    /// **THE KIT THE KEEPERS OF THIS SITE ARE ACTUALLY CARRYING**, resolved — never *"the player
    /// named none"* (`docs/plan_standing_upkeep.md` §2.7). `""` only when no band of the viewing
    /// faction works this source at all.
    ///
    /// **The keeping kit is per WORK SITE, not per band**, so this is a property of the worked row —
    /// `buildKitId`'s shape one account over, and read live off the bands' rows rather than stamped
    /// by the turn, so a pick is visible in the frame the command is answered in.
    #[serde(default)]
    pub upkeep_kit_id: String,
    /// **Is that id an OVERRIDE the player stated**, as against the site's own web derivation? Not
    /// recoverable from the id alone — a player may name the very kit the derivation would have
    /// picked — and a picker needs it to draw its `(default)` mark.
    #[serde(default)]
    pub upkeep_kit_named: bool,
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
    /// The rung, as `<branch>:<id>` — `plant:tended`, `plant:field`.
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

/// The serde default of `build_queue_position` — [`crate::NOT_IN_ANY_BUILD_QUEUE`], because a source
/// nobody has described is in nobody's queue. It is named rather than left to a bare
/// `#[serde(default)]`, which would answer `0` — the **head** of the queue, the
/// reassuring-direction wrong answer and one a client would render as *"next up"*.
fn not_in_any_build_queue() -> i32 {
    crate::NOT_IN_ANY_BUILD_QUEUE
}

/// **One material a commitment would pay, and how much of it per turn** — a row of
/// [`FloraShareInfo::sow_material_payoff`] / [`FloraShareInfo::cultivate_material_payoff`].
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

/// One named plant's share of a tile's forage capacity — see [`ForagePatchState::composition`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FloraShareInfo {
    /// The stable config key (`flora_config.json` → `species`), e.g. `"hazel"`.
    pub species: String,
    /// The player-facing name, e.g. `"Hazel"` — shipped because the client holds no roster.
    pub display_name: String,
    /// This plant's fraction of the tile's basket, `0..1`.
    pub share: f32,
    /// **May a `Cultivate` commit a patch to this plant** (rung 2) — the species' own
    /// `cultivation_ceiling.allows_cultivate()`. Shipped for the reason [`Self::display_name`] is:
    /// the client holds no roster, so it cannot tell that oak mast is a wild harvest forever.
    ///
    /// **Species-global, not tile-specific** — the ceiling is a property of the plant, so it reads
    /// the same on every tile hosting it and cannot drift from a per-tile rule. The *other* half of
    /// legality ("does it grow here") is this entry existing in the tile's composition at all.
    ///
    /// It answers *"can this plant ever climb this rung"*, **not** *"is this a good idea here"* —
    /// [`Self::share`] answers that. Committing to a marginal-share plant is legal and is a real
    /// loss, and that loss is the decision the rung exists to make
    /// (`docs/plan_flora_roster.md` §4.3). Appended (append-only).
    #[serde(default)]
    pub can_cultivate: bool,
    /// **May a `Sow` commit a patch to this plant** (rung 3) — `cultivation_ceiling.allows_sow()`.
    /// The rung-3 twin of [`Self::can_cultivate`], same reading, same caveats. Appended
    /// (append-only).
    #[serde(default)]
    pub can_sow: bool,
    /// **What committing this tile to this plant pays, against just gathering it wild, at the tended
    /// rung** — the tended payoff over the wild payoff, where tending **weeds** the basket (the
    /// favored share rises to `min(1, share × tended_weeding_gain)`, taken from the least abundant
    /// of the members standing in the worked ground first) and converts the favored term at
    /// `tended_conversion_gain`
    /// (`docs/plan_flora_roster.md` §4.3).
    ///
    /// `> 1.0` committing beats gathering the whole basket; `< 1.0` it is a **loss the player stays
    /// free to choose**, which is the decision the rung exists to make — never clamped, never hidden.
    /// A *ratio against the wild basket*, not an absolute yield: it folds in both the plant's share
    /// of this tile and the plant's own conversion rate, which is why it is shipped instead of the
    /// raw rate (half the answer, and the rest of the formula would drift client-side).
    ///
    /// `0` means **cannot climb this rung**, mirroring [`Self::can_cultivate`] — distinct from a real
    /// ratio of `0`, which cannot occur. Appended (append-only).
    #[serde(default)]
    pub cultivate_yield_ratio: f32,
    /// The Field-rung twin of [`Self::cultivate_yield_ratio`] — same reading, on a basket forced to
    /// **100% the sown crop** and on `allows_sow`. Its own field because the two rungs differ in
    /// *both* reweight and legality, so one number would be ambiguous about which rung it answers.
    /// Appended (append-only).
    #[serde(default)]
    pub sow_yield_ratio: f32,
    /// **Provisions/turn this tile would pay once the tended rung is complete and committed to this
    /// plant** — the same units and output-multiplier convention as
    /// [`ForagePatchState::tended_yield`], so the client can substitute one for the other with no
    /// arithmetic of its own.
    ///
    /// **Per species, because the shipped per-patch quotes are species-blind**: they read whatever
    /// the patch is already committed to (usually nothing), so a player comparing crops sees one
    /// number for every option. Produced by the same payoff function the sim pays the rung with,
    /// against the patch the sim would have — this tile's own `K` concentrated by the rung, at the
    /// standing crop that rung settles at — so it answers *"what does this ground pay once the crop
    /// is established"* rather than pricing a 25-turn investment off one transient turn. `0` where
    /// the plant cannot climb the rung.
    /// [`Self::cultivate_yield_ratio`] is exactly this over the tile's wild payoff. Appended
    /// (append-only).
    #[serde(default)]
    pub cultivate_payoff: f32,
    /// The Field-rung twin of [`Self::cultivate_payoff`] — the counterpart of
    /// [`ForagePatchState::field_yield`]. Appended (append-only).
    #[serde(default)]
    pub sow_payoff: f32,
    /// Fodder/turn a sown Field of this plant would harvest into the band's FODDER store on this tile
    /// (Flora Roster F3). A fodder crop's payoff is in this account, not provisions, so the picker can
    /// show hay's value instead of the bare `0×` its `sow_yield_ratio` reads. `0` for a staple or a
    /// plant that cannot climb to the Field rung here. Appended (append-only).
    #[serde(default)]
    pub sow_fodder_payoff: f32,
    // **RETIRED: `sow_trade_payoff`** (arc #527). The wire slot `sowTradePayoff` is `(deprecated)`
    // in place. **The gap it leaves is real:** the crop picker's cash-crop row was the one surface
    // that told a player what sowing cotton is *for*, and a material yield cannot be quoted as one
    // per-turn number. Replacing it is client-side work with a per-material shape.
    /// The **tended-rung** twin of [`Self::sow_fodder_payoff`] — fodder/turn a completed tended patch
    /// of this plant would pay here (issue #419). Its own field for the reason
    /// [`Self::cultivate_payoff`] is: rung 2 is a drawn-down MSY skim and rung 3 a managed rate, so one
    /// number cannot answer both rungs, and quoting the Sow figure on the Cultivate row overstated it.
    /// `0` where the vector pays no fodder or the plant cannot climb to the tended rung here. Appended
    /// (append-only).
    #[serde(default)]
    pub cultivate_fodder_payoff: f32,
    // **RETIRED: `cultivate_trade_payoff`** (arc #527). The wire slot `cultivateTradePayoff` is
    // `(deprecated)` in place — see `sow_trade_payoff` above for the gap.
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
    /// **What a sown Field of this plant would pay, per material** (arc #527) — the replacement for
    /// the retired `sow_trade_payoff`, and the number the crop picker's cash-crop row states.
    ///
    /// **Empty means "no row", never "zero".** A food crop yields no material and must render
    /// nothing here — a `0` would read as a cash crop that pays badly. Empty is also what a plant
    /// that cannot climb the rung on this ground reports, the convention
    /// [`Self::sow_payoff`]'s `0` follows.
    ///
    /// Produced by `forage::commit_material_payoff` off the same per-rung harvest
    /// `credit_material_yield` is paid on, so the quote and the payout cannot drift. Appended
    /// (append-only).
    #[serde(default)]
    pub sow_material_payoff: Vec<MaterialPayoff>,
    /// The **tended-rung** twin of [`Self::sow_material_payoff`] — its own field for the reason
    /// [`Self::cultivate_payoff`] is: rung 2 is a drawn-down MSY skim and rung 3 a managed rate on
    /// the standing crop, so one number cannot answer both rungs. Appended (append-only).
    #[serde(default)]
    pub cultivate_material_payoff: Vec<MaterialPayoff>,
    /// **What a Sow committing this tile to THIS plant would cost, in work units**
    /// (`docs/plan_standing_upkeep.md` §4.15) — the rung's declared `work_cost` times the multiplier
    /// this crop's own share earns, in the same units as [`ForagePatchState::field_work_cost`].
    ///
    /// **The cost half of the crop decision.** `plant:field` is priced by how much of the tile the
    /// chosen crop still has to replace, and a patch prices exactly one crop — its commitment, or the
    /// rung's auto-pick — so every row of a crop picker quoted the *same* work figure and only the
    /// payoffs moved. This one moves with the crop, so work and payoff can be weighed against each
    /// other before anything is committed.
    ///
    /// `0` means **no figure** — the plant cannot climb to a Field on this ground — and must render
    /// as *no row*, never as a free Sow. A real price is never `0`: the multiplier is clamped at
    /// `field_share_cost_floor` precisely because ground already wholly the crop still has its rows
    /// laid and its seed put in.
    ///
    /// Produced by `forage::crop_field_cost_multiplier`, the same expression the patch's own price
    /// goes through, so the figure quoted for the committed crop **is**
    /// [`ForagePatchState::field_work_cost`]. Appended (append-only).
    #[serde(default)]
    pub sow_work_cost: f32,
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
/// ⛔ **THE FIVE NAMED FLOATS ARE RETIRED**, their FlatBuffers ids held and never reused (they are
/// `(deprecated)` in `snapshot.fbs`). Adding a knowledge used to mean adding a schema field, which is
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

/// **ONE RUNG OF THE ROUTE BRANCH, AS THE CONFIG DECLARES IT** — the branch's *catalog*, once per
/// world and carrying no tile.
///
/// ⛔ **THIS IS WHAT LETS A CLIENT DRAW A ROAD LADDER OF RUNGS NOTHING HAS BUILT YET.**
/// [`crate::state::routes::RouteState`] answers *where does this tile stand and what is that worth*;
/// it says nothing about the rungs above it, so no readout could state what a paved road would cost
/// or what it would buy until the tile already held one.
///
/// ⛔ **EVERY FIELD IS DERIVED FROM `intensification_ladder.json`; NOTHING HERE IS AUTHORED
/// SEPARATELY** — the same discipline [`LadderKnowledgeState`] follows, and for the same reason: a
/// rung added to that config appears in the ladder with no client edit and no schema edit. The one
/// exception is [`Self::build_work_per_worker_turn`], which is the *sim's* rate and not the rung's.
///
/// **It rides the section and not the tile row.** These are properties of the *branch*, identical
/// for every road in the world; carried on `RouteState` they would repeat the same four rows on
/// every road tile — which is the same reason the bare work rate rides here rather than on the
/// road.
///
/// **Route branch only, deliberately.** A generic `LadderRungState` filled for one branch is a
/// promise the code does not keep; the plant and animal branches publishing the same is their own
/// change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RouteRungState {
    /// The rung's name on the wire, `"<branch>:<id>"` (`"route:dirt_road"`) — the same spelling
    /// `RouteState::rung` carries, so a tile's standing joins to its row here.
    pub rung_key: String,
    /// The config record's own `order`: `1` at the branch's floor, climbing. **The row order is this
    /// order** — the catalog is published bottom rung first.
    pub order: u32,
    /// `"Dirt Road"` — the rung id with underscores turned to spaces and each word capitalized,
    /// resolved sim-side so no client authors a second spelling of it.
    pub display_name: String,
    /// The **tile command** that raises this rung (`"grade"` / `"pave"`), `""` where the rung
    /// declares none. An empty verb is the free floor: a path and a trail are formed by *use*, so
    /// there is no command to name the job and no crew to staff it.
    pub verb: String,
    /// The ladder knowledge that gates this rung (`"roadbuilding"`), joining to
    /// [`LadderKnowledgeState::knowledge_id`] for the faction's own progress; `""` where the rung
    /// waits on nothing.
    pub unlock_knowledge: String,
    /// The wire key of the rung **directly beneath** this one, `""` at the branch's floor — the
    /// chain, so a client renders the climb without holding a second copy of the order.
    pub requires_rung: String,
    /// What reaching this rung costs, in work units, **before** the tile's own remoteness quote
    /// (`RouteState::keeper_remoteness` is that multiplier). `0` where the rung declares no build at
    /// all, which on the shipped ladder is the floor and nothing else. Inside the free floor the
    /// figure is a duration in **traffic** rather than a crew's job.
    #[serde(default)]
    pub work_cost: f32,
    /// The standing bill a road at this rung owes its keeper, in work units per turn, **before** the
    /// tile's own load scales it (`RouteState::upkeep_demand` is the resolved reading). `0` where
    /// the rung declares no upkeep — the free floor, which costs nothing to hold and can therefore
    /// never be short.
    #[serde(default)]
    pub upkeep_work_per_turn: f32,
    /// The fraction of the base pooling friction a haul over a tile at this rung pays; `1.0` = no
    /// help. `RouteState::friction_multiplier` is the same figure for the rung a tile **holds**.
    #[serde(default)]
    pub friction_multiplier: f32,
    /// How far a tile at this rung holds a pooling link open, in tiles. `0` = none beyond the free
    /// reach.
    #[serde(default)]
    pub holds_link_to_tiles: u32,
    /// **Does a road at this rung light its own tile?** Answered from whether the rung declares an
    /// upkeep, because *paying the bill is the presence* — which is also why the free floor lights
    /// nothing however worn it is. A road at a rung that grants sight still goes dark while its
    /// keeping is unmet; `RouteState::grants_sight` is that resolved per-tile answer.
    #[serde(default)]
    pub grants_sight: bool,
    /// **The ladder knowledge standing at this rung TEACHES** (`"roadbuilding"` on the trail), `""`
    /// where the rung teaches nothing — the floor, and the top of the branch, which has nothing
    /// above it to open.
    ///
    /// ⛔ **IT IS THE REMEDY, AND IT CANNOT BE INFERRED FROM [`Self::requires_rung`].** A gate says
    /// *you do not know `paving` yet*; what a player does about it is stand on the rung that
    /// **teaches** paving, which is a different fact from the rung directly beneath. The two
    /// coincide on the shipped four rungs and the config is free to break that pairing — an
    /// inference would then name the wrong rung in the one place it matters.
    ///
    /// Joins to [`LadderKnowledgeState::knowledge_id`], the same vocabulary
    /// [`Self::unlock_knowledge`] uses.
    #[serde(default)]
    pub earns_knowledge: String,
    /// **What one bare-handed worker banks in a turn** — `intensification::PER_WORKER_OUTPUT`,
    /// unscaled: before gear and before any multiplier, in work units per worker per turn.
    ///
    /// **The same figure for every rung, which is why it rides the catalog.** These rows are the
    /// branch's own numbers, identical for every road in the world; a per-tile copy on
    /// `RouteState` would repeat one constant once per road on the map.
    ///
    /// ⛔ **IT IS READ, NEVER ASSUMED.** Every source row publishes its own
    /// `build_work_per_worker_turn` because the sim writes worker output as a *sum of terms* — a
    /// road's own row ([`crate::state::RouteState`]) does not repeat it, for the reason above, so a
    /// client without this has to transcribe the constant and goes stale in silence the day a second
    /// term lands. A reader that finds it missing or `0` states *no estimate* rather than
    /// substituting a rate of its own.
    #[serde(default)]
    pub build_work_per_worker_turn: f32,
    /// **WHAT REACHING THIS RUNG COSTS IN MATERIAL** — the rung's declared build pile, in units of
    /// the material it eats. `20` on `route:paved_road`; `0` on every other rung of the branch,
    /// which eat nothing at all.
    ///
    /// ⛔ **IT IS FLAT, WHERE [`Self::work_cost`] BESIDE IT IS NOT.** A tile's own remoteness
    /// multiplies the work (`RouteState::keeper_remoteness`) and does **not** multiply this: a tile
    /// of road needs the same twenty stone wherever it lies, and remoteness already taxes the
    /// getting there. So this figure is the whole truth for every tile on the map, while `work_cost`
    /// is a base the tile's own multiplier still has to be applied to — the one asymmetry between
    /// the branch's two prices, and why this needs no per-tile twin.
    ///
    /// **Spent as the meter climbs, never on completion, and decay refunds nothing**
    /// (`docs/plan_standing_upkeep.md` §2.7): a roadbed a third laid has swallowed a third of the
    /// pile, and if it then washes out the stone is gone.
    /// `RouteState::build_material_demand` / `build_material_supplied` are this turn's share of it.
    ///
    /// **Which material it is** rides beside it in [`Self::build_material_id`].
    #[serde(default)]
    pub build_material_cost: f32,
    /// **WHICH MATERIAL [`Self::build_material_cost`] IS OF** — `"stone"` on `route:paved_road`, and
    /// `""` on every rung that eats nothing. Joins to the ids a band's own stores list publishes.
    ///
    /// ⛔ **READ IT WITH THE AMOUNT OR NOT AT ALL**, exactly as `build_work_per_worker` /
    /// `build_work_branch` / `build_work_rung` are one reading: an amount with no noun cannot be
    /// rendered into a sentence and a noun with no amount says nothing. Shipped without this, the
    /// client's rung row read *"+ 20 to raise it"* — twenty of what?
    ///
    /// ⛔ **AND THE CLIENT MUST NOT SUPPLY THE NOUN ITSELF.** *"The route branch eats stone"* is a
    /// fact about the **config**, so a client holding it is a second authority that goes stale in
    /// silence the day a rung is retuned to eat something else — the same transcription mistake
    /// [`Self::build_work_per_worker_turn`] exists to have prevented.
    ///
    /// **One id rather than a `MaterialPayoff` list**, unlike the plant and animal branches' piles:
    /// one material per rung *is* the model here, [`Self::build_material_cost`] is a single float so
    /// a second material would make the **amount** meaningless before the name mattered, and the
    /// flat-pile arithmetic has no defined reading for two. The pair is resolved from **one** lookup
    /// at capture so the two cannot disagree. Appended (append-only).
    #[serde(default)]
    pub build_material_id: String,
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
    /// per equipped worker per turn** ([`Self::build_work_per_worker`] beside it). The slot is held
    /// at its neutral
    /// rather than removed because the FlatBuffers `(deprecated)` keyword drops the accessor and a
    /// client still calls it.
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
