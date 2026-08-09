//! Subsistence-section state: herds, forage, graze, food modules, and sedentarization.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

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
    /// **The same per-worker rate in TRADE GOODS/turn** (appended, issue #337). Read this and
    /// [`Self::per_worker_yield`] as one vector: a resident-band preview clamps
    /// `min(workers × per-worker, ceiling)` **per component**. An inedible species (a wolf) reads
    /// `per_worker_yield == 0` with this positive.
    ///
    /// **THIS is the rate a band preview uses, not `PopulationCohortState::hunt_per_worker_provisions`**
    /// — that one is a species-blind global echo (see its doc).
    pub per_worker_trade: f32,
    /// Food/turn the herd will pay **once penned** (the corral's managed harvest at its current
    /// biomass). With the `corral` row of [`Self::hunt_policy_ceilings`] (what the herd pays *while*
    /// the pen is being built), lets the client show "preparing X → then Y" pre-commit.
    /// **Gross** — the pen's feed (`pen_upkeep`) is a separate debit.
    #[serde(default)]
    pub corral_yield: f32,
    /// **The Corral rung's payoff in TRADE GOODS/turn** (appended, issue #397) — the trade half of
    /// the same `managed_yield` `YieldAccounts` [`Self::corral_yield`] reads its provisions from. Read
    /// the two as one vector, rendering each component only when non-zero.
    /// **Gross** like its food sibling: the pen's feed (`pen_upkeep`) is a *provisions* debit and
    /// never touches this. `0` on a herd that never offers Corral.
    #[serde(default)]
    pub corral_trade: f32,
    /// **The feed this pen demands — or WOULD demand once built** — at the herd's CURRENT biomass
    /// (`pen.upkeep_per_biomass × biomass`), because a confined herd cannot graze. A **projection**
    /// for an unpenned herd, the **live** demand for a penned one: always meaningful, never
    /// `0`-because-unpenned. Computed on the same biomass basis as [`Self::corral_yield`], so the two
    /// are a **matched pair** — the pre-commit `Corral` row must show the running cost beside the
    /// payoff, since the herd it is deciding about is by definition *not yet penned*.
    ///
    /// **Demanded, not paid.** A starving pen demands more than it is paid ([`Self::pen_fed_fraction`]
    /// is that ratio). The band's *actual* ledger debit is
    /// `PopulationCohortState::pen_feed_upkeep` — draw **that** in the food ledger, not this.
    #[serde(default)]
    pub pen_upkeep: f32,
    /// The fraction of `pen_upkeep` the keeper actually **paid** last turn. `1.0` = fully fed (also
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
    /// §2.3): `1.0` = the fenced pasture feeds it for free, `0.0` = a barren footprint pays the full
    /// larder bill. With `penUpkeep` the client shows "fed by pasture NN% · larder N/turn". `0.0` for an
    /// unpenned herd. Appended (append-only).
    #[serde(default)]
    pub pen_pasture_fraction: f32,
    /// **The in-flight `ExtendPen` ring's build meter** for a "Fencing N%" badge: `0.0` when the pen is
    /// not extending, otherwise the ring's build progress (`0..1`, completing at `1.0` → `pen_radius`
    /// grows by one). Appended last (append-only).
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
    /// **One animal's worth of TRADE GOODS** (appended, issue #337) — the twin of
    /// [`Self::food_per_animal`], and the only quantum an *inedible* species has: a wolf's
    /// `food_per_animal` is honestly `0`, so a client rendering a kill rhythm from food alone would
    /// divide by zero. The animal COUNT is the same on either component (a ratio is unit-free).
    pub trade_per_animal: f32,
    /// **How many herders this managed herd owes this turn** (`fauna::herd_herders_needed` =
    /// `ceil((biomass / body_mass) / animals_per_herder)`) to hold its tameness. `0` for a
    /// wild/unmanaged herd (nobody to staff). The client pairs it with [`Self::herded_fraction`] for an
    /// honest "herders 1 / 6" readout the labor assignment's blended `workers_needed`
    /// (`max(herders_needed, haulers)`) cannot give. Appended last (append-only). `0` if unknown.
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
    /// **The Tame rung's payoff in TRADE GOODS/turn** (appended, issue #397) — the trade half of the
    /// same `pastoral_yield` `YieldAccounts` [`Self::pastoral_yield`] reads its provisions from. Read the
    /// two as one vector, rendering each component only when non-zero. `0` on a herd that never
    /// offers Tame (already penned, or a `wild`-ceiling species).
    #[serde(default)]
    pub pastoral_trade: f32,
    /// The hay this pen drew from its keeper band's FODDER store last turn (Flora Roster F3), in
    /// fodder units. `0` for an unpenned herd, a keeper that has not learned Foddering, or a pen its
    /// own footprint already fed. Lets the client show "fed by hay" beside the `pen_upkeep` bread bill.
    /// Appended last (append-only).
    #[serde(default)]
    pub fodder_draw: f32,
    /// **The pen's NET larder bill after pasture + hay** (Flora Roster F3) — the food/turn its keeper
    /// hauls from the `FOOD` larder once the footprint's pasture and any drawn hay have covered their
    /// share (the corral-tend branch's own `demand` = `gross pen_upkeep × (1 − land_hay_fraction)`), in
    /// **food** units. `0.0` when fully fed by pasture + hay, or unpenned. The render-ready larder term
    /// of the feed split: with [`Self::pen_upkeep`] (gross) and [`Self::pen_pasture_fraction`],
    /// `pen_upkeep × pen_pasture_fraction + pen_hay_food + pen_larder_bill == pen_upkeep` — three terms
    /// of one demand, no double-count. Appended last (append-only).
    #[serde(default)]
    pub pen_larder_bill: f32,
    /// **Hay's contribution to the pen's feed, in food-equivalent units** (Flora Roster F3) — the food
    /// it *displaced* from the larder (`pen_upkeep × fodder_draw / grass_demand`). [`Self::fodder_draw`]
    /// is in grass units (~25× the food scale) and cannot share a row with the food-unit pasture/larder
    /// terms; this can. `0.0` when no hay was drawn, the keeper lacks Foddering, or the herd is
    /// unpenned. The hay term of the render-ready feed split (see [`Self::pen_larder_bill`]). Appended
    /// last (append-only).
    #[serde(default)]
    pub pen_hay_food: f32,
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
    /// **The `Tame` rung's build dip, as the FRACTION it is** (issue #442) — the `animal:pastoral`
    /// rung's `yield_fraction_while_building`.
    ///
    /// **The dip is no longer a `hunt_policy_ceilings` row.** It used to ride there as a fifth row
    /// (`tame`), which could only ever state the fraction against **Sustain** — correct while a build
    /// verb *was* the policy, and false the moment a builder could hold any stance. The improvement is
    /// its own axis now, so the client multiplies:
    /// `preparing(stance) = hunt_policy_ceilings[stance] × tame_build_fraction`, and pairs it with
    /// [`Self::pastoral_yield`] for the "Preparing +X → then +Y" line.
    #[serde(default)]
    pub tame_build_fraction: f32,
    /// **The `Corral` rung's build dip, as a fraction** — the twin of [`Self::tame_build_fraction`],
    /// paired with [`Self::corral_yield`]. Two fields because the rungs' dials are independently
    /// tunable; one shared number would agree by today's coincidence and lie after a retune.
    #[serde(default)]
    pub corral_build_fraction: f32,
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
    /// species reads `0` here with a positive [`Self::trade_per_biomass`]. Appended (append-only).
    #[serde(default)]
    pub provisions_per_biomass: f32,
    /// No animal pays fodder, so this is `0` on every herd — present so both food webs publish the
    /// same triple and a reader needs one code path. Appended (append-only).
    #[serde(default)]
    pub fodder_per_biomass: f32,
    /// The trade half of the same vector — the only positive account on an inedible species.
    /// Appended (append-only).
    #[serde(default)]
    pub trade_per_biomass: f32,
    /// **What ONE hunter moves this turn, in BIOMASS** — `labor_config.hunt.per_worker_biomass_capacity`,
    /// the term `systems::hunt_take`'s collection multiplies by the head-count. No seasonal factor
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
            per_worker_trade: 0.0,
            trade_per_animal: 0.0,
            corral_yield: 0.0,
            corral_trade: 0.0,
            pen_upkeep: 0.0,
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
            pastoral_trade: 0.0,
            fodder_draw: 0.0,
            pen_larder_bill: 0.0,
            pen_hay_food: 0.0,
            attack: 0.0,
            defense: 0.0,
            ferocity: 0.0,
            aggression: 0.0,
            prey_sense_radius: 0,
            herders_needed_if_managed: 0,
            tame_build_fraction: 0.0,
            corral_build_fraction: 0.0,
            has_neglect_grace: false,
            neglect_grace_remaining: 0,
            provisions_per_biomass: 0.0,
            fodder_per_biomass: 0.0,
            trade_per_biomass: 0.0,
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
        }
    }
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
    /// and a **Field** publishes a single 100% entry. That is the whole of what a rung below 4 does
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
    /// Trade goods/turn a **completed tended patch** would pay — the twin of [`Self::tended_yield`],
    /// as `pastoral_trade` is of `pastoral_yield`. Rung 2 is drawn down, so this rides the take.
    #[serde(default)]
    pub tended_trade: f32,
    /// Fodder/turn a **completed tended patch** would pay. `0` unless its basket holds a fodder crop.
    #[serde(default)]
    pub tended_fodder: f32,
    /// Trade goods/turn a **completed Field** would pay — the twin of [`Self::field_yield`]. A Field
    /// is one plant at 100% share, so this is that crop's own rate; for a cash crop it is the whole
    /// yield and [`Self::field_yield`] is `0`.
    #[serde(default)]
    pub field_trade: f32,
    /// Fodder/turn a **completed Field** would pay — the whole yield of a `hay_grass` Field.
    #[serde(default)]
    pub field_fodder: f32,
    /// **The `Cultivate` rung's build dip, as the FRACTION it is** (issue #442) — the `plant:tended`
    /// rung's `yield_fraction_while_building`, and the plant twin of
    /// [`HerdTelemetryState::tame_build_fraction`]. The dip stopped being a
    /// [`Self::forage_policy_ceilings`] row when the improvement became its own axis:
    /// `preparing(stance) = forage_policy_ceilings[stance] × cultivate_build_fraction`, paired with
    /// [`Self::tended_yield`] for "Preparing +X → then +Y".
    #[serde(default)]
    pub cultivate_build_fraction: f32,
    /// **The `Sow` rung's build dip, as a fraction** — the twin of
    /// [`Self::cultivate_build_fraction`], paired with [`Self::field_yield`].
    #[serde(default)]
    pub sow_build_fraction: f32,
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
    /// **The crew the `Cultivate` build wants** (`plant:tended`'s `crew_needed`). It floors the
    /// compose sheet's worker cap — while a build runs the ceiling is the *dip*, so inverting it
    /// alone asked for fewer hands than gathering the same ground — and the build's progress scales
    /// by `min(workers / this, 1)`, so the rung's stated 25 turns is its **full-crew** duration.
    #[serde(default)]
    pub cultivate_crew_needed: u32,
    /// **The crew the `Sow` build wants** (`plant:field`'s `crew_needed`) — the twin of
    /// [`Self::cultivate_crew_needed`]; two fields because the rungs' dials are independently tunable.
    #[serde(default)]
    pub sow_crew_needed: u32,
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
    /// The trade half of the same vector — see [`Self::provisions_per_biomass`].
    #[serde(default)]
    pub trade_per_biomass: f32,
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
    /// first) and converts the favored term at `tended_conversion_gain`
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
    /// Trade goods/turn a sown Field of this plant would credit to the faction `trade_goods` stockpile
    /// on this tile (Flora Roster F4). A cash crop's payoff is in this account, not provisions, so the
    /// picker can show a cash crop's value instead of the bare `0×` its `sow_yield_ratio` reads. `0`
    /// for a staple/hay or a plant that cannot climb to the Field rung here. Appended (append-only).
    #[serde(default)]
    pub sow_trade_payoff: f32,
    /// The **tended-rung** twin of [`Self::sow_fodder_payoff`] — fodder/turn a completed tended patch
    /// of this plant would pay here (issue #419). Its own field for the reason
    /// [`Self::cultivate_payoff`] is: rung 2 is a drawn-down MSY skim and rung 3 a managed rate, so one
    /// number cannot answer both rungs, and quoting the Sow figure on the Cultivate row overstated it.
    /// `0` where the vector pays no fodder or the plant cannot climb to the tended rung here. Appended
    /// (append-only).
    #[serde(default)]
    pub cultivate_fodder_payoff: f32,
    /// The **tended-rung** twin of [`Self::sow_trade_payoff`] — trade goods/turn a completed tended
    /// patch of this plant would credit here (issue #419). The quote a rung-2 cash crop never had: it
    /// has been *paid* trade since #433 while being *previewed* as `0`. Appended (append-only).
    #[serde(default)]
    pub cultivate_trade_payoff: f32,
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
}

/// Per-faction intensification-ladder knowledge: the faction's progress on each of the ladder's
/// knowledges, 0..1 (1.0 = known). Mirrors `SedentarizationState`'s per-faction shape; the client
/// renders learning/known meters.
///
/// One field per rung-transition — *"practice rung N unlocks rung N+1"*
/// (`docs/plan_intensification_ladder.md` §4) — so the struct reads as the ladder itself:
/// `wild --cultivation--> tended --seed_selection--> field` and
/// `wild --herding--> pastoral --penning--> pen`. [`IntensificationKnowledgeState::foddering`] is the
/// one exception — a *capability* the top animal rung teaches rather than a gate on reaching a rung.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IntensificationKnowledgeState {
    pub faction: u32,
    /// Gates `cultivate`. Earned by working a **wild** patch under a stewardship policy.
    #[serde(default)]
    pub cultivation: f32,
    /// Gates `tame` — and `tame` **only**, since the §4.3 reshuffle. Earned by working a **wild** herd.
    #[serde(default)]
    pub herding: f32,
    /// Gates `sow` (slice 5 — earned now, spent later). Earned by working a **tended** patch.
    #[serde(default)]
    pub seed_selection: f32,
    /// Gates `corral` + `extend_pen` (the §4.3 reshuffle took this off `herding`). Earned by working
    /// a **pastoral** herd.
    #[serde(default)]
    pub penning: f32,
    /// **Not a rung transition** — no rung waits on it; it is the capability the **pen** rung teaches
    /// (`intensification_ladder.json`, corral's `earns_knowledge: "foddering"`). It gates every
    /// fodder seam: a penned herd's hay *draw*, the pen's `K` fodder term, and the **wild** forage
    /// patch's fodder credit. So it is the other half of the fodder answer — `ForagePatchState`
    /// states what the land pays, this states whether the faction can bank it.
    #[serde(default)]
    pub foddering: f32,
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
    /// Per-keeper **PEN** collection rate (biomass/turn) under this kit — the husbandry gear's tier.
    /// **Not [`Self::hunt_carry_per_worker_biomass`]**: a sled drags a carcass in off the range and
    /// a pen stands at the camp, so a kit carrying only a sled collects a pen at the bare rate.
    #[serde(default)]
    pub pen_carry_per_worker_biomass: f32,
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
}

/// **Hand-written rather than derived, for the same reason [`HerdTelemetryState`]'s is**: two of these
/// fields are multipliers whose neutral is `1`, and a `#[derive(Default)]` would answer `0` — the
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
            pen_carry_per_worker_biomass: 0.0,
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
