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

/// The sim's **pre-launch hunt-trip estimate** for one (policy, party size) against one herd — the
/// *answer*, so the client's outfit UI is a pure table lookup and does **zero** arithmetic.
///
/// Produced by `core_sim::hunt_trip_forecast`, a **bounded forward simulation** of the trip (herd
/// regrowth + the party's real take, turn by turn, on the sim's fixed-point grid) rather than a
/// closed-form `carry_cap / rate`. That division was wrong for Surplus/Deplete on a small herd, whose
/// per-policy ceiling is a *stock*, not a flow: the party strips the headroom in a turn or two and
/// then crawls at the regrowth trickle. It read a **4-worker party on a full Rabbit Warren (K = 200)
/// under Surplus as a ~5-turn trip**; the simulation says that party **never fills** within the
/// 60-turn horizon (only a *1-worker* party — a quarter the pack — fills, in **23 turns**).
///
/// The estimate covers only turns spent **hunting**, once the party is in reach — travel is not
/// counted — and assumes the herd stays put.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HuntTripEstimateState {
    /// **Where this sampled raid stops**, as a fraction of the herd's carrying capacity. The table
    /// **samples a continuum** — see `snapshot::RAID_FORECAST_FLOOR_SAMPLES` — because a raid's trip
    /// length is a forward simulation with no closed form, so the sim must export answers at chosen
    /// points rather than a formula. Appended (append-only).
    #[serde(default)]
    pub floor: f32,
    /// Party size, `1 ..= expedition_config.estimate_party_sizes` — a **sampling** axis, not a cap
    /// on what may be launched.
    pub party_workers: u32,
    /// Turns of hunting until the **raid completes** — the party comes home when the pack fills OR the
    /// standing surplus is spent (the herd is at the policy's floor) OR the herd is lost. **Not** "turns
    /// to fill the pack": a big party on a full herd strips the surplus and leaves with a *partial*
    /// pack, a successful short trip. **`0` = never completed** within `forecast_horizon_turns`.
    pub turns_to_fill: u32,
    /// **Does this trip bring home FOOD?** REDEFINED (issue #337): a fact about the **species**, not
    /// the policy — `false` means the quarry is *inedible* (a wolf), so render "no food delivered"
    /// and never an ETA. It used to read `false` for `eradicate` on the premise that denial carries
    /// nothing home; an Eradicate raid now banks the whole-stock windfall like every other rung.
    pub delivers_food: bool,
    /// **Does this trip bring home TRADE GOODS?** (appended) The sibling of `delivers_food` — the
    /// other component of the species' hunt-yield vector, so a wolf trip reads
    /// `delivers_food = false, delivers_trade = true` ("pelts, no meat") instead of being mistaken
    /// for a denial mission.
    pub delivers_trade: bool,
    /// **Whole animals the raid KILLS** (append-only) — the kill count. A party too small to seat a
    /// whole animal now kills one and wastes the rest (mirroring the resident band), so this is a kill
    /// count, not a delivered count. Bounded by the standing surplus, so it plateaus with `party_workers`
    /// once the surplus (not the pack) binds. `0` = the herd is at/below the policy's floor with no
    /// surplus to raid. The delivered payload is `delivered_food`, not `animals_taken × food_per_animal`.
    pub animals_taken: u32,
    /// **Food the party actually LANDS in its larder over the raid** (append-only) — the PRIMARY
    /// readout. A small party on a big animal brings home a partial (with waste), so "too lean to raid"
    /// is `delivered_food == 0` (no surplus at any party size), not "party too small to carry an animal".
    pub delivered_food: f32,
    /// **Food killed but not hauled home over the raid** (append-only). `wasted_food / (delivered_food +
    /// wasted_food)` is the waste fraction the client shows beside the delivered total.
    pub wasted_food: f32,
    /// **Trade goods the party actually LANDS over the raid** (appended, issue #337) — the twin of
    /// [`Self::delivered_food`], projected through the same species vector the take path pays with.
    /// For a **wolf** raid this is the only payload: `delivered_food == 0` and
    /// `delivers_food == false`.
    #[serde(default)]
    pub delivered_trade: f32,
    /// **WHICH stop ends this trip** (appended, `docs/plan_hunt_through_combat.md` §5.2) — the
    /// `core_sim::HuntTripBound` key: `"pack_full"`, `"fill_target"`, `"floor"`, `"herd_lost"` or
    /// `"horizon"`. A trip length alone cannot tell the player's two levers apart — *"you come home
    /// on your fill target in 4 turns"* and *"you reach the floor in 2 turns with the pack a third
    /// full"* are different decisions and the same kind of number — so the sim names the bound and
    /// the client composes nothing.
    ///
    /// **Every row here is the UNTARGETED raid**, so it reads `"fill_target"` for no row: a fill
    /// target is chosen at launch, and this band-agnostic table samples floor × party size only.
    /// A launched party's own bound is `PopulationCohortState::expedition_trip_bound`.
    #[serde(default)]
    pub bound: String,
}

/// The sim's **pre-launch denial-raid estimate** for one party size against one herd — the denial
/// twin of [`HuntTripEstimateState`] (`docs/plan_denial_raid.md` §1.1).
///
/// **It carries neither a floor nor a fill target**, and that absence is the design rather than an
/// omission: a denial mission has no floor and no rate — *"you choose a herd and a party size"* — so
/// there is nothing to sample and the table has one axis.
///
/// **The headline is [`Self::turns_to_collapse`], not a food total.** Success is pushing the herd
/// under `ecology.collapse_fraction`, the point of no return where the growth flow is zeroed and the
/// herd declines irreversibly with the party gone — never killing every animal. What comes home is a
/// rounding error against what was killed, which is the point, and [`Self::wasted_food`] is where the
/// rest of it went.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DenialEstimateState {
    /// Party size. The axis runs `1 ..=` *this herd's own requirement + `estimate_party_sizes` of
    /// headroom*, capped by `expedition_config` `deny.max_party_quoted` — **wider than the hunt
    /// table's**, so the row the sheet opens on ([`HerdTelemetryState::denial_party_needed`]) always
    /// exists. It is a **sampling** axis, not a cap on what may be launched.
    pub party_workers: u32,
    /// **Turns until the herd is past recovery** at the take's expectation — and therefore turns
    /// until the party comes home, because that is when a denial raid completes. **`0` = it never got
    /// there** within `forecast_horizon_turns`; [`Self::outcome`] says which *kind* of never, and
    /// must be rendered instead of a blank.
    pub turns_to_collapse: u32,
    /// The **optimistic** end of the range — the fewest turns
    /// (`docs/plan_hunt_through_combat.md` §6.4). More animals staying and more strikes landing is
    /// the good draw for a raid, and it drives the herd under sooner.
    pub turns_to_collapse_low: u32,
    /// The **pessimistic** end. `0` here beside a positive [`Self::turns_to_collapse`] is the honest
    /// *"only on a good run"* — not an error.
    pub turns_to_collapse_high: u32,
    /// The `core_sim::DenialOutcome` key: `"past_recovery"` / `"herd_lost"` / `"repelled"` /
    /// `"horizon"`. **`"repelled"` is the one the design insists on** — the party's kills per turn
    /// are at or below the herd's own regrowth, so it *cannot* get there. That is a verdict about the
    /// party; `"horizon"` is a statement about the clock.
    pub outcome: String,
    /// Whole animals the raid **kills** before it walks away.
    pub animals_killed: u32,
    /// Food landed in the pack over the raid — small, and non-zero.
    pub delivered_food: f32,
    /// Food killed and left on the range — **the bulk of a raid's take**, stated rather than hidden.
    pub wasted_food: f32,
    /// The trade half of the same carried biomass; the whole payload on an inedible quarry.
    pub delivered_trade: f32,
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
    /// The sim's **pre-launch trip estimates** for a hunting *expedition* against this herd — one
    /// entry per (stance × party size `1..=estimate_party_sizes`), so the outfit UI is a **table lookup**
    /// and the client does no arithmetic at all. The improvements are place-bound band work an
    /// expedition cannot do — since issue #442 its mission cannot even name one — so there is nothing
    /// to exclude. Empty for a non-huntable herd. See [`HuntTripEstimateState`] for why the trip is
    /// simulated rather than divided. Derived at capture. Appended last.
    #[serde(default)]
    pub hunt_trip_estimates: Vec<HuntTripEstimateState>,
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
    /// **The sim's pre-launch estimates for a DENIAL RAID against this herd** — one entry per party
    /// size, with no floor axis and no fill-target axis because the mission carries neither
    /// (`docs/plan_denial_raid.md`). The denial twin of [`Self::hunt_trip_estimates`]; empty for a
    /// non-huntable herd, exactly as that one is. Derived at capture. Appended last.
    ///
    /// **The party axis runs past `max_expedition_party_size`** where this herd needs it to — see
    /// [`Self::denial_party_needed`], and `expeditions.md` → "Denial is a MISSION, not a floor" for
    /// why a denial raid's outfit bound is the band's idle workers rather than that flat ceiling.
    #[serde(default)]
    pub denial_estimates: Vec<DenialEstimateState>,
    /// **The party the launch sheet OPENS on** — the smallest row in [`Self::denial_estimates`]
    /// whose raid is not `"repelled"`, and therefore the smallest party whose kills genuinely
    /// outpace this herd's regrowth (`docs/plan_denial_raid.md` §3.1).
    ///
    /// A denial raid is a **step function** in party size: below the requirement it accomplishes
    /// literally nothing however long it runs. Seeding the stepper here turns the control from a
    /// guessing game into an adjustment.
    ///
    /// **`0` = no quoted party drives this herd down**, never *"send nobody"*. Reached by a quarry
    /// nothing can bring into contact, by a requirement past the sim's quoting bound
    /// (`expedition_config` `deny.max_party_quoted`), and by a herd whose regrowth out-runs the whole
    /// table; the rows' own `outcome` says which. It may also legitimately exceed the launching
    /// band's idle workers — *"you need more people than you have"* is an answer.
    ///
    /// **Quoted for the EQUIPPED tier, like every other field on this table.** A herd row is a fact
    /// about the herd and has no band to ask, so the capture prices it with
    /// `hunter_profile(.., equipped = true)`. Since TOE the take depends on the band's own attack and
    /// carry tier, so a band whose kit has run dry is quoted a party it cannot achieve. That is a
    /// property of the whole herd table rather than of this field. Appended last.
    #[serde(default)]
    pub denial_party_needed: u32,
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
            hunt_trip_estimates: Vec::new(),
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
            denial_estimates: Vec::new(),
            // A herd nothing has described quotes no party — the same "no viable party" reading the
            // capture publishes for an unraidable one.
            denial_party_needed: 0,
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
