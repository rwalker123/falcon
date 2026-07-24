//! Subsistence-section state: herds, forage, graze, food modules, and sedentarization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SedentarizationState {
    pub faction: u32,
    pub score: f32,
    #[serde(default)]
    pub stage: String,
}

/// One take policy's per-turn **band / local-hunt** ceiling for a herd, in **provisions** (the sim
/// already converted from biomass). Worker-*independent*: the policy's cap on the take at the herd's
/// **current** state, before any party-throughput cap, and clamped to the herd's remaining biomass —
/// so it is a **true maximum take**. `0` = no take is possible under this policy (a collapsing
/// sub-Allee herd yields nothing under Sustain/Surplus). `policy` is a free-form string
/// (`sustain|surplus|market|eradicate|corral`, like `species`), so a new policy needs no schema
/// change.
///
/// **The rows are NOT all the same kind of quantity**, which is precisely why no one may divide by
/// them: Sustain is a per-turn **flow** (MSY), Surplus that flow × a multiplier, Corral a *fraction*
/// of it (the pen-building dip), while Market/Eradicate are shares of standing **stock** that shrink
/// as the herd is drawn down.
///
/// Consumer: the resident-band local-hunt yield preview —
/// `min(workers × hunt_per_worker_provisions, provisions_per_turn) × output_multiplier`, which is
/// arithmetically `core_sim::systems::hunt_take(..)` for a *single* turn against the herd's current
/// state (pinned by `core_sim/tests/expedition_hunt.rs`). That single-turn arithmetic is legitimate;
/// projecting it across turns is not.
///
/// **A hunting expedition must NOT forecast a trip from this number.** It is the **band / local-hunt**
/// per-turn ceiling, and even for the expedition's *own* ceilings there is no single rate to divide
/// by (see the kinds above). So `cap / rate` is wrong either way — the herd's state moves under the
/// party (the stock exhausts mid-trip) and the forecast horizon bounds the answer. **An expedition's
/// trip length comes from `HerdTelemetryState.hunt_trip_estimates`**, which the sim forward-simulates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HuntPolicyCeilingState {
    pub policy: String,
    /// BAND / local-hunt ceiling, in provisions/turn. Produced by projecting the herd's
    /// `fauna::hunt_forecast` through `SourceYieldForecast::ceiling_for(policy)` — i.e. the
    /// per-policy biomass ceiling `fauna::hunt_policy_ceiling` converted by `fauna::hunt_provisions`,
    /// the *same* helpers the take path pays with (forecast == actual). Never re-derive it.
    #[serde(default)]
    pub provisions_per_turn: f32,
}

/// The sim's **pre-launch hunt-trip estimate** for one (policy, party size) against one herd — the
/// *answer*, so the client's outfit UI is a pure table lookup and does **zero** arithmetic.
///
/// Produced by `core_sim::hunt_trip_forecast`, a **bounded forward simulation** of the trip (herd
/// regrowth + the party's real take, turn by turn, on the sim's fixed-point grid) rather than a
/// closed-form `carry_cap / rate`. That division was wrong for Surplus/Market on a small herd, whose
/// per-policy ceiling is a *stock*, not a flow: the party strips the headroom in a turn or two and
/// then crawls at the regrowth trickle. It read a **4-worker party on a full Rabbit Warren (K = 200)
/// under Surplus as a ~5-turn trip**; the simulation says that party **never fills** within the
/// 60-turn horizon (only a *1-worker* party — a quarter the pack — fills, in **23 turns**).
///
/// The estimate covers only turns spent **hunting**, once the party is in reach — travel is not
/// counted — and assumes the herd stays put.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HuntTripEstimateState {
    /// Free-form take policy (`sustain|surplus|market|eradicate`), like `species` — a new policy
    /// needs no schema change.
    pub policy: String,
    /// Party size, `1 ..= expedition_config.max_party_size`.
    pub party_workers: u32,
    /// Turns of hunting until the **raid completes** — the party comes home when the pack fills OR the
    /// standing surplus is spent (the herd is at the policy's floor) OR the herd is lost. **Not** "turns
    /// to fill the pack": a big party on a full herd strips the surplus and leaves with a *partial*
    /// pack, a successful short trip. **`0` = never completed** within `forecast_horizon_turns`.
    pub turns_to_fill: u32,
    /// Does this mission bring food home? `false` for `eradicate` (denial) — render "no food
    /// delivered", never an ETA.
    pub delivers_food: bool,
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
    /// Food/turn the herd will pay **once penned** (the corral's managed harvest at its current
    /// biomass). With the `corral` row of [`Self::hunt_policy_ceilings`] (what the herd pays *while*
    /// the pen is being built), lets the client show "preparing X → then Y" pre-commit.
    /// **Gross** — the pen's feed (`pen_upkeep`) is a separate debit.
    #[serde(default)]
    pub corral_yield: f32,
    /// Per-policy **band / local-hunt** take ceilings for this herd's current state — one entry per
    /// [`FollowPolicy`] valid on a Hunt assignment: the four extractive rungs **plus Corral**
    /// (`Cultivate` is forage-only, so a herd has no cultivate row). Phase-correct: a penned herd's
    /// rows all read its corral yield. The **only** wire representation of a herd's per-policy
    /// ceilings — a free-form `policy` string means a new policy needs no schema change. With the
    /// cohort's `hunt_per_worker_provisions` and `output_multiplier` this is
    /// everything the client needs to preview a *resident band's* hunt yield as pure arithmetic — it
    /// must never re-derive the ecology model. Derived at capture. Appended (append-only wire).
    #[serde(default)]
    pub hunt_policy_ceilings: Vec<HuntPolicyCeilingState>,
    /// The sim's **pre-launch trip estimates** for a hunting *expedition* against this herd — one
    /// entry per (**extractive** policy × party size `1..=max_party_size`), so the outfit UI is a
    /// **table lookup** and the client does no arithmetic at all. The investment policies
    /// (Cultivate/Corral) are place-bound band work that `send_hunt_expedition` rejects, so they get
    /// no rows here. Empty for a non-huntable herd. See [`HuntTripEstimateState`] for why the trip is
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
    /// the value for a herd that is not penned, and for a rehydrated one); `< 1` = **starving** — the
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
            hunt_policy_ceilings: Vec::new(),
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
            fodder_draw: 0.0,
            pen_larder_bill: 0.0,
            pen_hay_food: 0.0,
            attack: 0.0,
            defense: 0.0,
            ferocity: 0.0,
            aggression: 0.0,
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
    /// **Pre-commit yield forecast** at the patch's current biomass (food/turn, captured at
    /// `output_multiplier = 1.0` — the client scales by the band's `outputMultiplier`). Lets the
    /// client show "Expected yield: +X.XX /turn" and cap its worker stepper *while the player is
    /// composing an assignment*, before anything is committed:
    /// `expected(workers, policy) = min(workers × per_worker_yield, ceiling_<policy>)` and
    /// `max_useful_workers(policy) = ceil(ceiling_<policy> / per_worker_yield)`.
    /// Food/turn one forager contributes (this tile's seasonal weight folded in, as the take does);
    /// `0.0` in a dead season — do not divide by it.
    #[serde(default)]
    pub per_worker_yield: f32,
    /// Food/turn ceiling under Sustain (MSY), already clamped to the patch's remaining biomass.
    #[serde(default)]
    pub ceiling_sustain: f32,
    /// Food/turn ceiling under Surplus, biomass-clamped.
    #[serde(default)]
    pub ceiling_surplus: f32,
    /// Food/turn ceiling under Market, biomass-clamped.
    #[serde(default)]
    pub ceiling_market: f32,
    /// Food/turn ceiling under Eradicate, biomass-clamped.
    #[serde(default)]
    pub ceiling_eradicate: f32,
    /// Food/turn under the **Cultivate** policy — what the patch pays *while it is being prepared*
    /// (`cultivating_yield_fraction × the Sustain/MSY ceiling`, the investment dip).
    #[serde(default)]
    pub ceiling_cultivate: f32,
    /// Food/turn the patch will pay **once cultivated** (the tended harvest on its current standing
    /// crop). With `ceiling_cultivate`, lets the client show "preparing X → then Y" pre-commit.
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
    /// Food/turn under the **Sow** policy — what the ground pays *while it is being sown* (the
    /// `plant:field` rung's `yield_fraction_while_building ×` whatever it would otherwise pay). Its
    /// own field, not `ceiling_cultivate`'s: the two plant investment rungs are independently tunable.
    #[serde(default)]
    pub ceiling_sow: f32,
    /// Food/turn the patch will pay **once sown** (the Field harvest on its current standing crop —
    /// 2× `tended_yield` on the shipped dials). With `ceiling_sow`, Sow's "preparing X → then Y" pair.
    #[serde(default)]
    pub field_yield: f32,
    /// **Why this ground will not take seed** ([`SiteRefusal::as_str`]: `"too_poor"` / `"too_dry"` /
    /// `"too_poor_and_too_dry"`), or **`""`** when it will. Resolved through the same
    /// `RungSiteRequirement::refusal` seam the `sow` command and the labor arm gate on, so the wire
    /// cannot disagree with the gate. Shipped as an *answer* because the client can re-derive
    /// nothing: it holds neither the per-biome capacity table nor the hydrology.
    #[serde(default)]
    pub sow_site_refusal: String,
    /// **What grows here** — the named plants this tile's forage capacity is *made of*, as
    /// normalized shares (`docs/plan_flora_roster.md` §2: naming decomposes, it does not add).
    ///
    /// **Derived from the biome, not per-patch state**: a pure function of the tile's terrain and
    /// the roster's affinity weights, so every tile of a biome reads the same composition and
    /// nothing on the patch can change it. The shares sum to `1.0` on any forage-bearing tile, so
    /// `share × forage_capacity` is that plant's own capacity and the parts always re-sum to the
    /// whole. Empty on a biome that carries no forage. Deterministically sorted (share DESC, then
    /// species key ASC).
    #[serde(default)]
    pub composition: Vec<FloraShareInfo>,
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
    /// rung** — `min(1, share × tended_concentration_gain) × species_rate ÷
    /// forage.provisions_per_biomass` (`docs/plan_flora_roster.md` §4.3).
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
    /// The Field-rung twin of [`Self::cultivate_yield_ratio`] — same reading, on
    /// `field_concentration_gain` and `allows_sow`. Its own field because the two rungs differ in
    /// *both* gain and legality, so one number would be ambiguous about which rung it answers.
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
}

/// Per-faction intensification-ladder knowledge: the faction's progress on each of the ladder's
/// knowledges, 0..1 (1.0 = known). Mirrors `SedentarizationState`'s per-faction shape; the client
/// renders learning/known meters.
///
/// One field per rung-transition — *"practice rung N unlocks rung N+1"*
/// (`docs/plan_intensification_ladder.md` §4) — so the struct reads as the ladder itself:
/// `wild --cultivation--> tended --seed_selection--> field` and
/// `wild --herding--> pastoral --penning--> pen`.
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
}

/// Shared depletable-ecology record round-tripped through the rollback snapshot. Mirrors the
/// mutable biomass state a resource carries — herds today (`HerdState.ecology`), forage patches
/// in the next intensification slice (`ForageState`). `ecology_phase` crosses as a stable string
/// (`EcologyPhase::as_str` / parse) so the live enum stays serde-free per the codebase convention;
/// `progress` = a herd's `domestication_progress` (forage will reuse it for cultivation);
/// `owner` = the tending faction id (`FactionId`'s inner `u32`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EcologyState {
    #[serde(default)]
    pub biomass: f32,
    #[serde(default)]
    pub carrying_capacity: f32,
    #[serde(default)]
    pub ecology_phase: String,
    #[serde(default)]
    pub progress: f32,
    #[serde(default)]
    pub owner: Option<u32>,
}

/// Serde mirror of a herd's live `RoamState` movement mode. The variant crosses as a stable
/// string (`"graze_wander" | "loiter" | "migrate"`) with the loiter countdown alongside it, so the
/// live enum stays serde-free (same convention as `ecology_phase`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HerdRoamState {
    #[serde(default)]
    pub mode: String,
    /// Loiter only: remaining loiter turns (`0` for graze-wander / migrate).
    #[serde(default)]
    pub loiter_turns_left: u32,
}

/// Full authoritative mirror of a live `Herd`, round-tripped through the rollback snapshot so a
/// rollback rewinds herd biomass / position / movement — not just the lossy display telemetry
/// (`HerdTelemetryState`). Identity + movement fields are mirrored directly; the depletable-ecology
/// subset (biomass / carrying_capacity / phase / domestication progress / owner) lives in the
/// embedded `EcologyState`. Coordinates cross as `(x, y)` pairs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HerdState {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub species: String,
    /// Coarse size band (`SizeClass::as_str` / parse: `"small" | "big" | "migratory"`).
    #[serde(default)]
    pub size_class: String,
    /// Sparse anchor list (movement waypoints), each an `(x, y)` tile.
    #[serde(default)]
    pub route: Vec<(u32, u32)>,
    #[serde(default)]
    pub step_index: u32,
    #[serde(default)]
    pub current_pos: (u32, u32),
    #[serde(default)]
    pub dwell_remaining: u32,
    #[serde(default)]
    pub roam: HerdRoamState,
    /// Next intended hex (client heading arrow); `None` while loitering/grazing.
    #[serde(default)]
    pub next_pos: Option<(u32, u32)>,
    /// Corral (Rung 1c): the tile a **penned** herd is fixed at, or `None` for a mobile herd. A
    /// corralled herd doesn't roam and is paid its keeper place-local. Authoritative sim state —
    /// persisted so a rollback preserves the pen.
    #[serde(default)]
    pub corralled_at: Option<(u32, u32)>,
    /// Pen-construction progress 0..1 accrued under the **Corral** policy (`1.0` = penned). Persisted
    /// alongside `corralled_at` so a rollback rewinds a half-built pen rather than losing the
    /// investment.
    #[serde(default)]
    pub corral_progress: f32,
    /// The pen's **footprint radius** (Grazing 2d) — the fenced land a penned herd grazes / derives K
    /// over (`hex_range_tiles(corralled_at, pen_radius)`). `0` = the single corralled tile. Persisted so
    /// a rollback preserves a fence the `ExtendPen` command (2d-β) grew.
    #[serde(default)]
    pub pen_radius: u32,
    /// Pen-**extension** build progress `0..1` for the in-flight ring (2d-β). Persisted alongside
    /// `pen_radius` so a rollback rewinds a partly-fenced ring.
    #[serde(default)]
    pub pen_extend_progress: f32,
    /// The `ExtendPen` "extending" state (2d-β) — `true` while a ring is being worked off. Persisted so
    /// a rollback preserves the in-flight extension rather than stranding a half-progress meter.
    #[serde(default)]
    pub pen_extending: bool,
    /// Per-species fodder demand per unit biomass (Grazing Phase 2b-i), cached on the live `Herd` at
    /// spawn from its `SpeciesDef`. Round-tripped here (sim-side rollback only, not on the client wire)
    /// so a rollback restores the eating rate rather than leaving a rehydrated herd grazing at `0`.
    #[serde(default)]
    pub fodder_per_biomass: f32,
    /// Per-species **wild logistic regrowth rate** (Grazing Phase 2b-ii), cached on the live `Herd` at
    /// spawn from its `SpeciesDef` (falling back to the global wild rate when the row omits it).
    /// Round-tripped here (sim-side rollback only, not on the client wire) so a rollback restores the
    /// herd's breeding rate rather than leaving a rehydrated herd growing at the wrong `r`.
    #[serde(default)]
    pub regrowth_rate: f32,
    /// **Biomass of one animal** (intensification ladder slice 8), cached on the live `Herd` at spawn
    /// from its `SpeciesDef`. Round-tripped here (sim-side rollback only, not on the client wire) so a
    /// rollback restores the herd's take quantum — a rehydrated herd reading `0` would be a herd with
    /// infinitely many animals, and `floor(escapement / 0)` would strip it whole in one turn.
    #[serde(default)]
    pub body_mass: f32,
    /// **Kill-credit accumulator** (intensification ladder slice 8b) — biomass a hunt has earned toward
    /// its next whole animal but not yet spent (`Herd::hunt_credit`). Round-tripped here (sim-side
    /// rollback only, not on the client wire) so a rollback rewinds a herd's progress toward its next
    /// kill; a rehydrated herd reading `0` just restarts the wait, never a burst.
    #[serde(default)]
    pub hunt_credit: f32,
    /// How far up the husbandry ladder the herd's species climbs (Grazing 2d-δ): `wild` | `pastoral` |
    /// `pen` (`HusbandryCeiling::as_str`/`from_key`). Cached on the live `Herd` at spawn; round-tripped
    /// so a rollback restores a wild herd as hunt-only. Empty/unknown → `pen` (the full ladder).
    #[serde(default)]
    pub husbandry_ceiling: String,
    /// **The hysteresis-stabilized herder requirement** (`Herd::herders_needed`) — the remembered,
    /// deadband-stabilized `herders_needed` for a managed herd (`0` = wild, or a managed herd not yet
    /// stabilized). Persisted (sim-side rollback only, not on the client wire) so a rollback restores
    /// the remembered count rather than re-flickering ±1 for a turn as the herd breathes across an
    /// `animals_per_herder` head-count boundary.
    #[serde(default)]
    pub herders_needed: u32,
    #[serde(default)]
    pub ecology: EcologyState,
}

/// Authoritative mirror of a live depletable forage patch (`ForageRegistry`), round-tripped through
/// the rollback snapshot so a rollback rewinds patch biomass / phase — the forage counterpart of
/// `HerdState`. Reuses the shared `EcologyState` (biomass / carrying_capacity / phase), whose
/// `progress` / `owner` carry the patch's **cultivation** meter (rung 2) and its tending faction. The
/// `(x, y)` tile key is the patch's location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ForageState {
    #[serde(default)]
    pub x: u32,
    #[serde(default)]
    pub y: u32,
    /// **Field**-build progress `0..1` accrued under the `Sow` policy (`1.0` = a sown Field, the
    /// plant ladder's rung 3). Its own field rather than a second `EcologyState.progress`, exactly as
    /// `HerdState.corral_progress` sits beside the herd's `ecology.progress` (domestication): a source
    /// on a two-investment branch carries **two** meters, one per rung. Persisted so a rollback
    /// rewinds a half-sown field rather than losing the investment.
    #[serde(default)]
    pub field_progress: f32,
    /// **The named plant this patch is COMMITTED to** (Flora Roster S1) — a `flora_config.json`
    /// species key, or `""` for the wild mixed basket. Set on the first turn a crew works the patch
    /// under `Cultivate`/`Sow`, cleared when both improvement meters lapse to zero (the patch goes
    /// fully feral and the tile returns to its wild basket).
    ///
    /// **Persisted because it is authoritative state, not a derivation**: the commitment decides
    /// both the patch's effective carrying capacity (concentration) and its biomass→provisions rate
    /// (conversion), so a rollback that lost it would rewind a farm into a differently-shaped one.
    /// `""` rather than an `Option` for the same reason `fauna_id` is a bare `String` here.
    #[serde(default)]
    pub species: String,
    #[serde(default)]
    pub ecology: EcologyState,
}

/// Authoritative mirror of a live **graze (pasture) patch** (`GrazeRegistry`), round-tripped through
/// the rollback snapshot so a rollback rewinds grazing draw-down — the animal-edible counterpart of
/// `ForageState`, on the same shared `EcologyState` record. Graze is **wild ground**: it is never
/// owned, tended or improved, so `EcologyState`'s `progress` / `owner` ride at their defaults.
/// The `(x, y)` tile key is the patch's location. See `docs/plan_grazing_foundation.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GrazeState {
    #[serde(default)]
    pub x: u32,
    #[serde(default)]
    pub y: u32,
    #[serde(default)]
    pub ecology: EcologyState,
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
