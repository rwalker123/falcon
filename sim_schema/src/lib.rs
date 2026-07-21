use ahash::RandomState;
use flatbuffers::{DefaultAllocator, FlatBufferBuilder, ForwardsUOffset, WIPOffset};
use serde::{Deserialize, Serialize};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use std::{
    hash::{BuildHasher, Hash, Hasher},
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign},
};

type FbBuilder<'a> = FlatBufferBuilder<'a, DefaultAllocator>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignLabel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_loc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_loc_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignProfileState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_loc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_loc_key: Option<String>,
    #[serde(default)]
    pub starting_units: Vec<CampaignStartingUnitState>,
    #[serde(default)]
    pub inventory: Vec<CampaignInventoryEntryState>,
    #[serde(default)]
    pub knowledge_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub survey_radius: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_food_module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_food_module: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignInventoryEntryState {
    pub item: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FactionInventoryEntryState {
    pub item: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FactionInventoryState {
    pub faction: u32,
    pub inventory: Vec<FactionInventoryEntryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SedentarizationState {
    pub faction: u32,
    pub score: f32,
    #[serde(default)]
    pub stage: String,
}

/// One discovered Wondrous Site (position + catalog-resolved display fields) in a faction's
/// registry. Only sites the faction has revealed appear here — undiscovered sites never leave
/// the sim, so there is no fog leak.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DiscoveredSiteState {
    pub x: u32,
    pub y: u32,
    #[serde(default)]
    pub site_id: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub glyph: String,
}

/// Per-faction discovered-sites registry (mirrors `SedentarizationState`'s per-faction shape).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DiscoveredSitesState {
    pub faction: u32,
    #[serde(default)]
    pub sites: Vec<DiscoveredSiteState>,
}

/// Per-faction age structure aggregated over the faction's population cohorts. The client
/// derives the dependency ratio `(children + elders) / working` for its HUD readout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PopulationDemographicsState {
    pub faction: u32,
    #[serde(default)]
    pub children: u32,
    #[serde(default)]
    pub working: u32,
    #[serde(default)]
    pub elders: u32,
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
    /// See `ForagePatchState`'s forecast fields — this is the herd-side twin.
    #[serde(default)]
    pub per_worker_yield: f32,
    #[serde(default)]
    pub ceiling_sustain: f32,
    #[serde(default)]
    pub ceiling_surplus: f32,
    #[serde(default)]
    pub ceiling_market: f32,
    #[serde(default)]
    pub ceiling_eradicate: f32,
    /// Food/turn under the **Corral** policy — what the herd pays *while the pen is being built*
    /// (`corralling_yield_fraction × the Sustain/MSY ceiling`, the investment dip).
    #[serde(default)]
    pub ceiling_corral: f32,
    /// Food/turn the herd will pay **once penned** (the corral's managed harvest at its current
    /// biomass). With `ceiling_corral`, lets the client show "preparing X → then Y" pre-commit.
    /// **Gross** — the pen's feed (`pen_upkeep`) is a separate debit.
    #[serde(default)]
    pub corral_yield: f32,
    /// Per-policy **band / local-hunt** take ceilings for this herd's current state — one entry per
    /// [`FollowPolicy`] valid on a Hunt assignment: the four extractive rungs **plus Corral**
    /// (`Cultivate` is forage-only, so a herd has no cultivate row). Phase-correct: a penned herd's
    /// rows all read its corral yield. The same numbers as the `ceiling_*` scalars above, projected
    /// as a list; with the cohort's `hunt_per_worker_provisions` and `output_multiplier` this is
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
            ceiling_sustain: 0.0,
            ceiling_surplus: 0.0,
            ceiling_market: 0.0,
            ceiling_eradicate: 0.0,
            ceiling_corral: 0.0,
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

/// Which ticks one narrative beat has fired on (`core_sim::telling::BeatLedger`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatFiredState {
    #[serde(default)]
    pub beat: String,
    #[serde(default)]
    pub ticks: Vec<u64>,
}

/// One `signal → value` pair in the beat ledger's edge state (or one `axis → value` pair of the
/// stance vector — the same shape, keyed differently). `value` is **fixed-point raw**
/// (`Scalar::SCALE` = 1.0), so a rollback restores bit-exact samples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatSignalValueState {
    #[serde(default)]
    pub signal: String,
    /// Fixed-point raw (`Scalar::SCALE` = 1.0).
    #[serde(default)]
    pub value: i64,
}

/// One signal's rolling sample history, oldest first, capped at `trend.max_history_turns`.
/// Samples are **fixed-point raw** (`Scalar::SCALE` = 1.0).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatSignalHistoryState {
    #[serde(default)]
    pub signal: String,
    #[serde(default)]
    pub samples: Vec<i64>,
}

/// When a wardrobe entry was last used (the novelty memory).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatWardrobeUsageState {
    #[serde(default)]
    pub wardrobe: String,
    #[serde(default)]
    pub last_used_tick: u64,
}

/// Authoritative mirror of The Telling's `BeatLedger` — the narrative memory (what fired, what the
/// signals read last turn, which dressings are stale). Round-tripped through the rollback snapshot
/// **including restore**, so a rollback past a beat lets that beat fire again instead of leaving it
/// wrongly marked fired. Every map crosses as a sorted `Vec` so the record is stable.
///
/// Per-turn scratch (the tier budget counters) is deliberately absent — it is recomputed each
/// turn, so a rehydrated ledger starts neutral. Sim-side only; not on the FlatBuffers client
/// stream (beats reach the client as `CommandEvent`s). See `docs/plan_the_telling.md` §3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatLedgerState {
    #[serde(default)]
    pub fired: Vec<BeatFiredState>,
    #[serde(default)]
    pub edge_state: Vec<BeatSignalValueState>,
    #[serde(default)]
    pub history: Vec<BeatSignalHistoryState>,
    #[serde(default)]
    pub wardrobe_usage: Vec<BeatWardrobeUsageState>,
    #[serde(default)]
    pub flags: Vec<String>,
    /// The player's **declared stance offsets** (the fork tier's write-back). Only the offsets are
    /// stored — the effective stance is `normalize(signal) + offset`, recomputed each turn.
    #[serde(default)]
    pub stance: Vec<BeatSignalValueState>,
    /// Forks posted and not yet answered.
    #[serde(default)]
    pub pending_forks: Vec<BeatPendingForkState>,
    /// Beat id → the choice id the player took, so later beats can call back to what was decided.
    #[serde(default)]
    pub answers: Vec<BeatAnswerState>,
    /// Beat id → the tick a `once` beat's guard lifts (the defer branch's `rearm_after_turns`).
    #[serde(default)]
    pub rearm: Vec<BeatRearmState>,
    /// The memory threads — durable nouns later beats can call back to. Flat and kind-grouped by
    /// construction (the ledger iterates a `BTreeMap<kind, Vec<Thread>>`), so the record is stable.
    #[serde(default)]
    pub threads: Vec<BeatThreadState>,
    /// Faction → the narrator's **attained** medium. Persisted because it is monotone: a people
    /// that learned to write does not forget, so the live evaluation takes the max against this.
    #[serde(default)]
    pub mediums: Vec<BeatVoiceMediumState>,
}

/// One memory thread: a noun **snapshotted at first sight and never re-resolved**, so a callback
/// still lands after the herd went extinct or the site fell four hundred turns behind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatThreadState {
    pub kind: String,
    /// Dedupe identity — the resolved noun's `name`.
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub plural: String,
    #[serde(default)]
    pub adjective: String,
    #[serde(default)]
    pub first_seen_tick: u64,
    /// The eviction clock: least recently *referenced* is what gets dropped, not oldest first-seen.
    #[serde(default)]
    pub last_referenced_tick: u64,
}

/// A faction's attained narrator medium, sim-side (the client-facing twin is [`VoiceMediumState`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatVoiceMediumState {
    pub faction: u32,
    pub medium_id: String,
    #[serde(default)]
    pub medium_index: u32,
}

/// A faction's narrator **medium** on the client stream: oral saga → painted chronicle → written
/// record. Presentational — it changes how the telling *looks*; it does **not** select different
/// copy (see `core_sim/src/telling/medium.rs`).
///
/// `mediumId` is a string (the `species` / `policy` / `register` convention) so adding a medium
/// needs no schema change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VoiceMediumState {
    pub faction: u32,
    pub medium_id: String,
    #[serde(default)]
    pub medium_index: u32,
}

/// One register's rendering of a player-visible narrative string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatVoiceLineState {
    pub register: String,
    pub text: String,
}

/// One answer offered by a pending fork, rendered at post time in every register.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatForkChoiceState {
    pub choice_id: String,
    #[serde(default)]
    pub is_defer: bool,
    #[serde(default)]
    pub label: Vec<BeatVoiceLineState>,
    /// The line pushed to the feed once this choice is taken. Rendered at post time so the nouns
    /// stay pinned to the moment the fork fired.
    #[serde(default)]
    pub echo: Vec<BeatVoiceLineState>,
}

/// A fork awaiting an answer. Every register is rendered up front, because the register is a live
/// user toggle — storing a single string would freeze the fork in whichever voice was active when
/// it fired.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatPendingForkState {
    pub beat_id: String,
    #[serde(default)]
    pub wardrobe_id: String,
    #[serde(default)]
    pub faction: u32,
    #[serde(default)]
    pub posted_tick: u64,
    #[serde(default)]
    pub narration: Vec<BeatVoiceLineState>,
    #[serde(default)]
    pub choices: Vec<BeatForkChoiceState>,
    /// The sampled signals behind the question ("the voice never lies"), fixed-point like every
    /// other persisted number.
    #[serde(default)]
    pub gloss: Vec<BeatSignalValueState>,
}

/// Per-faction pending narrative forks, on the client stream (the `SedentarizationState` /
/// `DiscoveredSitesState` per-faction shape). Distinct from the sim-side `BeatPendingForkState`:
/// this is what the client renders and answers with `answer_fork`.
///
/// **The turn gate is client-side.** The server never blocks turn resolution on a pending fork —
/// it auto-resolves one to its defer choice after `beat_config.budget.fork_expire_turns`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PendingForksState {
    pub faction: u32,
    #[serde(default)]
    pub forks: Vec<PendingForkState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PendingForkState {
    pub beat_id: String,
    #[serde(default)]
    pub wardrobe_id: String,
    #[serde(default)]
    pub posted_tick: u64,
    /// Every configured register, rendered when the fork fired.
    #[serde(default)]
    pub narration: Vec<VoiceLineState>,
    #[serde(default)]
    pub choices: Vec<ForkChoiceState>,
    #[serde(default)]
    pub gloss: Vec<GlossEntryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VoiceLineState {
    pub register: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ForkChoiceState {
    pub choice_id: String,
    #[serde(default)]
    pub label: Vec<VoiceLineState>,
    /// Computed server-side (the choice writes nothing) so the client never has to know what
    /// makes a choice a defer.
    #[serde(default)]
    pub is_defer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GlossEntryState {
    pub signal: String,
    pub value: f64,
}

/// A faction's **effective** stance per axis: normalized backing signal + declared offset, in
/// `[-1, 1]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StanceState {
    pub faction: u32,
    #[serde(default)]
    pub axes: Vec<StanceAxisState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StanceAxisState {
    pub axis: String,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatAnswerState {
    pub beat: String,
    pub choice: String,
    /// The tick the fork was answered on. Load-bearing: the `answered` predicate's
    /// `min_turns_since` reads it, so a callback can mean "some time after you said that".
    #[serde(default)]
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatRearmState {
    pub beat: String,
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FoodModuleState {
    pub x: u32,
    pub y: u32,
    pub module: String,
    pub seasonal_weight: f32,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CommandEventState {
    pub tick: u64,
    pub kind: String,
    pub faction: u32,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignStartingUnitState {
    pub kind: String,
    pub count: u32,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictoryModeSnapshotState {
    pub id: String,
    pub kind: String,
    pub progress: f32,
    pub threshold: f32,
    pub achieved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictoryResultState {
    pub mode: String,
    pub faction: u32,
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictorySnapshotState {
    #[serde(default)]
    pub modes: Vec<VictoryModeSnapshotState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner: Option<VictoryResultState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotHeader {
    pub tick: u64,
    pub tile_count: u32,
    pub logistics_count: u32,
    pub trade_link_count: u32,
    pub population_count: u32,
    pub power_count: u32,
    pub influencer_count: u32,
    pub hash: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_label: Option<CampaignLabel>,
    #[serde(default)]
    pub wrap_horizontal: bool,
    /// Build identifier of the server binary (see `snapshot.fbs`). Set by core_sim.
    #[serde(default)]
    pub server_build: String,
    /// Monotonic world-build counter (see `snapshot.fbs`). Incremented on every world (re)build,
    /// identical for every snapshot within one world; a client uses it to ignore a stale world the
    /// snapshot server replays to reconnecting subscribers. Set by core_sim.
    #[serde(default)]
    pub world_epoch: u32,
}

impl SnapshotHeader {
    pub fn new(
        tick: u64,
        tile_count: usize,
        logistics_count: usize,
        trade_link_count: usize,
        population_count: usize,
        power_count: usize,
        influencer_count: usize,
    ) -> Self {
        Self {
            tick,
            tile_count: tile_count as u32,
            logistics_count: logistics_count as u32,
            trade_link_count: trade_link_count as u32,
            population_count: population_count as u32,
            power_count: power_count as u32,
            influencer_count: influencer_count as u32,
            hash: 0,
            campaign_label: None,
            wrap_horizontal: false,
            server_build: String::new(),
            world_epoch: 0,
        }
    }

    /// Sets the server build identifier reported to clients.
    pub fn with_server_build(mut self, build: impl Into<String>) -> Self {
        self.server_build = build.into();
        self
    }

    /// Creates a header with wrap_horizontal set.
    pub fn with_wrap_horizontal(mut self, wrap: bool) -> Self {
        self.wrap_horizontal = wrap;
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u16)]
pub enum TerrainType {
    DeepOcean = 0,
    ContinentalShelf = 1,
    InlandSea = 2,
    CoralShelf = 3,
    HydrothermalVentField = 4,
    TidalFlat = 5,
    RiverDelta = 6,
    MangroveSwamp = 7,
    FreshwaterMarsh = 8,
    Floodplain = 9,
    #[default]
    AlluvialPlain = 10,
    PrairieSteppe = 11,
    MixedWoodland = 12,
    BorealTaiga = 13,
    PeatHeath = 14,
    HotDesertErg = 15,
    RockyReg = 16,
    SemiAridScrub = 17,
    SaltFlat = 18,
    OasisBasin = 19,
    Tundra = 20,
    PeriglacialSteppe = 21,
    Glacier = 22,
    SeasonalSnowfield = 23,
    RollingHills = 24,
    HighPlateau = 25,
    AlpineMountain = 26,
    KarstHighland = 27,
    CanyonBadlands = 28,
    ActiveVolcanoSlope = 29,
    BasalticLavaField = 30,
    AshPlain = 31,
    FumaroleBasin = 32,
    ImpactCraterField = 33,
    KarstCavernMouth = 34,
    SinkholeField = 35,
    AquiferCeiling = 36,
    /// A river so large it is a body of water in its own right: you need a boat to enter it.
    /// Stamped **only** by the hydrology pass, on the downstream tail of a river whose corner
    /// discharge crosses `river_class_navigable_min_discharge`. Reuses every existing water
    /// mechanic (it is `WATER | FRESHWATER`-tagged, mirroring `InlandSea`), which is exactly why
    /// it is a terrain and not a `RiverClass` — minor/major rivers are *edges* between hexes,
    /// a navigable river *is* the hex.
    NavigableRiver = 37,
}

impl TerrainType {
    pub const VALUES: [TerrainType; 38] = [
        TerrainType::DeepOcean,
        TerrainType::ContinentalShelf,
        TerrainType::InlandSea,
        TerrainType::CoralShelf,
        TerrainType::HydrothermalVentField,
        TerrainType::TidalFlat,
        TerrainType::RiverDelta,
        TerrainType::MangroveSwamp,
        TerrainType::FreshwaterMarsh,
        TerrainType::Floodplain,
        TerrainType::AlluvialPlain,
        TerrainType::PrairieSteppe,
        TerrainType::MixedWoodland,
        TerrainType::BorealTaiga,
        TerrainType::PeatHeath,
        TerrainType::HotDesertErg,
        TerrainType::RockyReg,
        TerrainType::SemiAridScrub,
        TerrainType::SaltFlat,
        TerrainType::OasisBasin,
        TerrainType::Tundra,
        TerrainType::PeriglacialSteppe,
        TerrainType::Glacier,
        TerrainType::SeasonalSnowfield,
        TerrainType::RollingHills,
        TerrainType::HighPlateau,
        TerrainType::AlpineMountain,
        TerrainType::KarstHighland,
        TerrainType::CanyonBadlands,
        TerrainType::ActiveVolcanoSlope,
        TerrainType::BasalticLavaField,
        TerrainType::AshPlain,
        TerrainType::FumaroleBasin,
        TerrainType::ImpactCraterField,
        TerrainType::KarstCavernMouth,
        TerrainType::SinkholeField,
        TerrainType::AquiferCeiling,
        TerrainType::NavigableRiver,
    ];

    /// Lowercase, human-readable adjective for the biome, reading naturally mid-sentence
    /// ("the *alluvial* ground", "the *high grassland* ground"). Written out rather than derived
    /// from the enum's debug name, which would produce copy like "AlluvialPlain ground".
    ///
    /// Consumed by The Telling's `biome.current_dominant` noun resolver (`core_sim/src/telling`).
    pub const fn as_adjective(self) -> &'static str {
        match self {
            TerrainType::DeepOcean => "deep water",
            TerrainType::ContinentalShelf => "shallow-sea",
            TerrainType::InlandSea => "inland-sea",
            TerrainType::CoralShelf => "coral",
            TerrainType::HydrothermalVentField => "vent-field",
            TerrainType::TidalFlat => "tidal",
            TerrainType::RiverDelta => "delta",
            TerrainType::MangroveSwamp => "mangrove",
            TerrainType::FreshwaterMarsh => "marsh",
            TerrainType::Floodplain => "floodplain",
            TerrainType::AlluvialPlain => "alluvial",
            TerrainType::PrairieSteppe => "grassland",
            TerrainType::MixedWoodland => "woodland",
            TerrainType::BorealTaiga => "taiga",
            TerrainType::PeatHeath => "peat",
            TerrainType::HotDesertErg => "desert",
            TerrainType::RockyReg => "stony",
            TerrainType::SemiAridScrub => "scrub",
            TerrainType::SaltFlat => "salt-flat",
            TerrainType::OasisBasin => "oasis",
            TerrainType::Tundra => "tundra",
            TerrainType::PeriglacialSteppe => "cold-steppe",
            TerrainType::Glacier => "glacier",
            TerrainType::SeasonalSnowfield => "snowfield",
            TerrainType::RollingHills => "hill",
            TerrainType::HighPlateau => "high grassland",
            TerrainType::AlpineMountain => "mountain",
            TerrainType::KarstHighland => "karst",
            TerrainType::CanyonBadlands => "badland",
            TerrainType::ActiveVolcanoSlope => "volcano-slope",
            TerrainType::BasalticLavaField => "lava-field",
            TerrainType::AshPlain => "ash",
            TerrainType::FumaroleBasin => "fumarole",
            TerrainType::ImpactCraterField => "crater",
            TerrainType::KarstCavernMouth => "cavern",
            TerrainType::SinkholeField => "sinkhole",
            TerrainType::AquiferCeiling => "aquifer",
            TerrainType::NavigableRiver => "river",
        }
    }
}

/// The class of river running along **one side of a hex** (an odd-r hex *edge*).
///
/// Packed 2 bits per direction into `Tile::river_edges` / `TileState::river_edges`, so a tile
/// carries the class of the river on each of its six sides. This is the primitive a movement
/// system reads: "entering hex H across direction d crosses `H.river_class_on_side(d)`".
///
/// A river that outgrows `Major` does **not** get a variant here — it becomes a
/// [`TerrainType::NavigableRiver`] hex instead (a body of water you need a boat to enter), so
/// value `3` is deliberately left reserved rather than spent on "navigable".
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, PartialOrd, Ord,
)]
#[repr(u8)]
pub enum RiverClass {
    #[default]
    None = 0,
    Minor = 1,
    Major = 2,
}

impl RiverClass {
    /// Bits per direction in a packed river-edge mask.
    pub const BITS_PER_DIR: u32 = 2;
    /// Bits per **corner** in a packed river-inflow mask. A corner slot holds the same class in the
    /// same 2 bits as a direction slot — one packing layout, keyed two ways (side vs. vertex).
    pub const BITS_PER_CORNER: u32 = Self::BITS_PER_DIR;
    /// Mask of a single direction's (or corner's) slot.
    pub const SLOT_MASK: u16 = 0b11;

    pub const fn bits(self) -> u16 {
        self as u16
    }

    /// Decode a 2-bit slot. The reserved value `3` decodes to `None` (no river) rather than
    /// panicking — an unknown class must never be read as a crossable river.
    pub const fn from_bits(bits: u16) -> Self {
        match bits & Self::SLOT_MASK {
            1 => RiverClass::Minor,
            2 => RiverClass::Major,
            _ => RiverClass::None,
        }
    }

    pub const fn is_some(self) -> bool {
        !matches!(self, RiverClass::None)
    }
}

/// The bit layout of a packed **channel-exit** mask (`Tile::river_channel` /
/// `TileState::river_channel`): one bit per odd-r direction, set when a hex's *navigable* channel
/// flows out through that side.
///
/// Why it exists: a navigable river is a chain of water **hexes**, and a chain is a PATH — hex `A`
/// connects to its upstream and downstream neighbours and to nothing else. The terrain alone cannot
/// say which neighbours those are, so a renderer that arms every navigable/water neighbour draws a
/// cross-linked **web** wherever two chains run side by side or a chain bends back on itself. Only
/// the tracer knows the chain, so it states it here. Symmetric across a shared side (like
/// `river_edges`), except at the mouth, where the exit points into the open water/delta the river
/// drains into and that water carries no channel of its own.
pub struct RiverChannel;

impl RiverChannel {
    /// Bits per direction: a channel either exits through a side or it does not — there is no class
    /// here (the *water* is the river; `RiverClass` grades only edge rivers). Callers range-check
    /// `dir` against `grid_utils::HEX_DIRECTION_COUNT`, exactly as they do for `RiverClass`.
    pub const BITS_PER_DIR: u32 = 1;
    /// Mask of a single direction's slot.
    pub const SLOT_MASK: u8 = 0b1;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Hash)]
#[serde(transparent)]
pub struct TerrainTags(pub u16);

impl TerrainTags {
    pub const fn new(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const WATER: Self = Self(1 << 0);
    pub const FRESHWATER: Self = Self(1 << 1);
    pub const COASTAL: Self = Self(1 << 2);
    pub const WETLAND: Self = Self(1 << 3);
    pub const FERTILE: Self = Self(1 << 4);
    pub const ARID: Self = Self(1 << 5);
    pub const POLAR: Self = Self(1 << 6);
    pub const HIGHLAND: Self = Self(1 << 7);
    pub const VOLCANIC: Self = Self(1 << 8);
    pub const HAZARDOUS: Self = Self(1 << 9);
    pub const SUBSURFACE: Self = Self(1 << 10);
    pub const HYDROTHERMAL: Self = Self(1 << 11);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl BitOr for TerrainTags {
    type Output = TerrainTags;

    fn bitor(self, rhs: Self) -> Self::Output {
        TerrainTags(self.bits() | rhs.bits())
    }
}

impl BitOrAssign for TerrainTags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.bits();
    }
}

impl BitAnd for TerrainTags {
    type Output = TerrainTags;

    fn bitand(self, rhs: Self) -> Self::Output {
        TerrainTags(self.bits() & rhs.bits())
    }
}

impl BitAndAssign for TerrainTags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.bits();
    }
}

impl From<u16> for TerrainTags {
    fn from(value: u16) -> Self {
        TerrainTags::new(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeField {
    #[default]
    Physics = 0,
    Chemistry = 1,
    Biology = 2,
    Data = 3,
    Communication = 4,
    Exotic = 5,
}

impl KnowledgeField {
    pub const VALUES: [KnowledgeField; 6] = [
        KnowledgeField::Physics,
        KnowledgeField::Chemistry,
        KnowledgeField::Biology,
        KnowledgeField::Data,
        KnowledgeField::Communication,
        KnowledgeField::Exotic,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeSecurityPosture {
    #[default]
    Minimal = 0,
    Standard = 1,
    Hardened = 2,
    BlackVault = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeCountermeasureKind {
    #[default]
    SecurityInvestment = 0,
    CounterIntelSweep = 1,
    Misinformation = 2,
    KnowledgeDebtRelief = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeModifierSource {
    #[default]
    Visibility = 0,
    Security = 1,
    Spycraft = 2,
    Culture = 3,
    Exposure = 4,
    Debt = 5,
    Treaty = 6,
    Event = 7,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeTimelineEventKind {
    #[default]
    LeakProgress = 0,
    SpyProbe = 1,
    CounterIntel = 2,
    Exposure = 3,
    Treaty = 4,
    Cascade = 5,
    Digest = 6,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(transparent)]
pub struct KnowledgeLeakFlags(pub u32);

impl KnowledgeLeakFlags {
    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const COMMON_KNOWLEDGE: Self = Self(1 << 0);
    pub const FORCED_PUBLICATION: Self = Self(1 << 1);
    pub const CASCADE_PENDING: Self = Self(1 << 2);

    pub fn contains(self, rhs: Self) -> bool {
        (self.0 & rhs.0) == rhs.0
    }

    pub fn insert(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }

    pub fn remove(&mut self, rhs: Self) {
        self.0 &= !rhs.0;
    }

    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl BitOr for KnowledgeLeakFlags {
    type Output = KnowledgeLeakFlags;

    fn bitor(self, rhs: Self) -> Self::Output {
        KnowledgeLeakFlags(self.bits() | rhs.bits())
    }
}

impl BitOrAssign for KnowledgeLeakFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.bits();
    }
}

impl BitAnd for KnowledgeLeakFlags {
    type Output = KnowledgeLeakFlags;

    fn bitand(self, rhs: Self) -> Self::Output {
        KnowledgeLeakFlags(self.bits() & rhs.bits())
    }
}

impl BitAndAssign for KnowledgeLeakFlags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.bits();
    }
}

impl From<u32> for KnowledgeLeakFlags {
    fn from(value: u32) -> Self {
        KnowledgeLeakFlags::new(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CrisisMetricKind {
    #[default]
    R0 = 0,
    GridStressPct = 1,
    UnauthorizedQueuePct = 2,
    SwarmsActive = 3,
    PhageDensity = 4,
}

impl CrisisMetricKind {
    pub const VALUES: [CrisisMetricKind; 5] = [
        CrisisMetricKind::R0,
        CrisisMetricKind::GridStressPct,
        CrisisMetricKind::UnauthorizedQueuePct,
        CrisisMetricKind::SwarmsActive,
        CrisisMetricKind::PhageDensity,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CrisisSeverityBand {
    #[default]
    Safe = 0,
    Warn = 1,
    Critical = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisTrendSample {
    pub tick: u64,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisGaugeState {
    pub kind: CrisisMetricKind,
    pub raw: f32,
    pub ema: f32,
    pub trend_5t: f32,
    pub warn_threshold: f32,
    pub critical_threshold: f32,
    pub last_updated_tick: u64,
    pub stale_ticks: u64,
    pub band: CrisisSeverityBand,
    pub history: Vec<CrisisTrendSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisTelemetryState {
    pub gauges: Vec<CrisisGaugeState>,
    pub modifiers_active: u32,
    pub foreshock_incidents: u32,
    pub containment_incidents: u32,
    pub warnings_active: u32,
    pub criticals_active: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisOverlayAnnotationState {
    pub label: String,
    pub severity: CrisisSeverityBand,
    pub path: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisOverlayState {
    pub heatmap: ScalarRasterState,
    pub annotations: Vec<CrisisOverlayAnnotationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeCountermeasureState {
    pub kind: KnowledgeCountermeasureKind,
    pub potency: i64,
    pub upkeep: i64,
    pub remaining_ticks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeInfiltrationState {
    pub faction: u32,
    pub blueprint_fidelity: i64,
    pub suspicion: i64,
    pub cells: u8,
    pub last_activity_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeModifierBreakdownState {
    pub source: KnowledgeModifierSource,
    pub delta_half_life: i16,
    pub delta_progress: i16,
    pub note_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeLedgerEntryState {
    pub discovery_id: u32,
    pub owner_faction: u32,
    pub tier: u8,
    pub progress_percent: u16,
    pub half_life_ticks: u16,
    pub time_to_cascade: u16,
    pub security_posture: KnowledgeSecurityPosture,
    pub countermeasures: Vec<KnowledgeCountermeasureState>,
    pub infiltrations: Vec<KnowledgeInfiltrationState>,
    pub modifiers: Vec<KnowledgeModifierBreakdownState>,
    pub flags: KnowledgeLeakFlags,
}

impl KnowledgeLedgerEntryState {
    pub fn has_flag(&self, flag: KnowledgeLeakFlags) -> bool {
        self.flags.contains(flag)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeMetricsState {
    pub leak_warnings: u32,
    pub leak_criticals: u32,
    pub countermeasures_active: u32,
    pub common_knowledge_total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeTimelineEventState {
    pub tick: u64,
    pub kind: KnowledgeTimelineEventKind,
    pub source_faction: u32,
    pub delta_percent: i16,
    pub note_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GreatDiscoveryState {
    pub id: u16,
    pub faction: u32,
    pub field: KnowledgeField,
    pub tick: u64,
    pub publicly_deployed: bool,
    pub effect_flags: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GreatDiscoveryProgressState {
    pub faction: u32,
    pub discovery: u16,
    pub progress: i64,
    pub observation_deficit: u32,
    pub eta_ticks: u32,
    pub covert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GreatDiscoveryTelemetryState {
    pub total_resolved: u32,
    pub pending_candidates: u32,
    pub active_constellations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GreatDiscoveryRequirementState {
    pub discovery: u32,
    pub weight: f32,
    pub minimum_progress: f32,
    pub name: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GreatDiscoveryDefinitionState {
    pub id: u16,
    pub name: String,
    pub field: KnowledgeField,
    pub tier: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub observation_threshold: u32,
    pub cooldown_ticks: u16,
    pub freshness_window: Option<u16>,
    pub effect_flags: u32,
    pub covert_until_public: bool,
    pub effects_summary: Vec<String>,
    pub observation_notes: Option<String>,
    pub leak_profile: Option<String>,
    pub requirements: Vec<GreatDiscoveryRequirementState>,
}

impl From<TerrainTags> for u16 {
    fn from(value: TerrainTags) -> Self {
        value.bits()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum MountainKind {
    #[default]
    None = 0,
    Fold = 1,
    Fault = 2,
    Volcanic = 3,
    Dome = 4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainSample {
    pub terrain: TerrainType,
    pub tags: TerrainTags,
    #[serde(default)]
    pub mountain_kind: MountainKind,
    #[serde(default = "default_relief_scale")]
    pub relief_scale: f32,
}

impl Default for TerrainSample {
    fn default() -> Self {
        Self {
            terrain: TerrainType::AlluvialPlain,
            tags: TerrainTags::empty(),
            mountain_kind: MountainKind::None,
            relief_scale: 1.0,
        }
    }
}

impl PartialEq for TerrainSample {
    fn eq(&self, other: &Self) -> bool {
        self.terrain == other.terrain
            && self.tags == other.tags
            && self.mountain_kind == other.mountain_kind
            && self.relief_scale.to_bits() == other.relief_scale.to_bits()
    }
}

impl Eq for TerrainSample {}

impl std::hash::Hash for TerrainSample {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.terrain.hash(state);
        self.tags.hash(state);
        self.mountain_kind.hash(state);
        self.relief_scale.to_bits().hash(state);
    }
}

const fn default_relief_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct TerrainOverlayState {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<TerrainSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ElevationOverlayState {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub min_value: f32,
    #[serde(default)]
    pub max_value: f32,
    #[serde(default)]
    pub samples: Vec<u16>,
    /// Sea level on the same normalized scale as `samples` (see `snapshot.fbs`).
    #[serde(default)]
    pub sea_level: f32,
}

/// The climate-band ladder cut points, published so the client renders the band it is told
/// (`docs/plan_climate_authority.md` §8.3). A per-map constant; each is the inclusive upper
/// temperature bound of a band. The client's retired `cool_min` equals `boreal_max_temp` (§5.2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct ClimateBandsState {
    #[serde(default)]
    pub polar_max_temp: f32,
    #[serde(default)]
    pub boreal_max_temp: f32,
    #[serde(default)]
    pub temperate_max_temp: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct StartMarkerState {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct ScalarRasterState {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FloatRasterState {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TileState {
    pub entity: u64,
    pub x: u32,
    pub y: u32,
    pub element: u8,
    pub mass: i64,
    pub temperature: i64,
    pub terrain: TerrainType,
    pub terrain_tags: TerrainTags,
    pub culture_layer: u32,
    #[serde(default)]
    pub mountain_kind: MountainKind,
    #[serde(default = "default_relief_scale")]
    pub mountain_relief: f32,
    /// Tile-intrinsic per-turn morale drain (fixed-point raw, `Scalar::SCALE` = 1.0; `>= 0`,
    /// bigger = harsher). Band-independent — a property of the place. Derived at capture.
    #[serde(default)]
    pub habitability: i64,
    /// Packed per-side river classes: `class = RiverClass::from_bits(river_edges >> (2 * dir))`
    /// for each odd-r direction `dir` (0=E, 1=SE, 2=SW, 3=W, 4=NW, 5=NE). Both hexes flanking a
    /// river edge carry it, each on their own side. Replaces the old polyline hydrology overlay:
    /// together with `TerrainType::NavigableRiver` this fully determines the river render.
    #[serde(default)]
    pub river_edges: u16,
    /// Packed per-**corner** river inflow: `class = RiverClass::from_bits(river_inflow >> (2 *
    /// corner))` for each hex corner (`0` lower-right, `1` bottom, `2` lower-left, `3` upper-left,
    /// `4` top, `5` upper-right — screen space, +y down).
    ///
    /// Set only on the **first hex of a `NavigableRiver` chain**, at the corner where the edge-river
    /// chain terminates and hands its water to the navigable trunk, with the class of the last edge
    /// it emitted. An edge river ends at a *vertex*, never mid-side, so this is where the renderer
    /// must join the tributary to the trunk hex. `0` everywhere else.
    #[serde(default)]
    pub river_inflow: u16,
    /// Packed per-side **channel exits** of a navigable river — 1 bit per odd-r direction (see
    /// [`RiverChannel`]): `exits(dir) = (river_channel >> dir) & 1`.
    ///
    /// The trunk channel is a **path**, and only the tracer knows which neighbours a navigable hex
    /// actually links to; a renderer that infers them from terrain draws a web. Arm only the sides
    /// whose bit is set. Symmetric across a shared side, except at the mouth (the exit into the
    /// ocean/inland sea/delta is not mirrored back). `0` on every hex with no navigable channel.
    #[serde(default)]
    pub river_channel: u8,
    /// **Graze (pasture) readout** — the tile's live *animal-edible* biomass (grass/browse), the stock
    /// herds eat. `0` on water/ice/rock and on any tile with no pasture. Distinct from the
    /// *human-edible* forage stock (`ForagePatchState`, food-module tiles only) — see
    /// `docs/plan_grazing_foundation.md`. Derived at capture from the `GrazeRegistry`.
    #[serde(default)]
    pub graze_biomass: f32,
    /// The tile's graze **capacity** — a property of the *land* (its biome), not of any animal. `0`
    /// means the biome carries no pasture at all; `graze_biomass / graze_capacity` is the pasture's
    /// health (and, from Phase 2b, the overgrazing signal).
    #[serde(default)]
    pub graze_capacity: f32,
    /// The tile's pasture phase, as [`GRAZE_PHASE_NONE`] / [`GRAZE_PHASE_THRIVING`] /
    /// [`GRAZE_PHASE_STRESSED`] / [`GRAZE_PHASE_COLLAPSING`]. A compact code rather than the string
    /// the sparse herd/forage payloads use, because this rides *every* tile (the `moraleCause:ubyte`
    /// idiom). `NONE` is the default, so "this biome has no pasture" is never confused with "this
    /// pasture is healthy".
    #[serde(default)]
    pub graze_ecology_phase: u8,
    /// **Forage potential** — the *human-edible* twin of [`graze_capacity`](Self::graze_capacity).
    /// The land's per-biome human-food capacity (`forage.capacity_by_biome`, `labor_config.json`),
    /// read from the config table for *every* tile — **not** from the sparse `ForagePatch`, which
    /// exists only on food-module tiles. That is the point: the client draws a Forage overlay of the
    /// biome's *potential* everywhere (the mirror of the pasture overlay), including the ~95% of tiles
    /// that carry no patch. Unlike graze this is **non-zero on fishery water** (`ContinentalShelf` /
    /// `CoralShelf` / `InlandSea`) — a fishery is a food module on water. Only a *stated-zero* biome
    /// (deep ocean, glacier, lava, salt flat) reads `0`. Derived at capture from
    /// `forage::tile_forage_capacity`, which keys off `resource_terrain()` (the underlying valley
    /// biome on a navigable hex, the tile's own terrain elsewhere); a `NavigableRiver` hex additionally
    /// earns the navigable fishing bonus — see `docs/plan_grazing_foundation.md` §1.1.
    #[serde(default)]
    pub forage_capacity: f32,
    /// The tile's **real ground** for resource reads. Equals `terrain` on every ordinary tile; on a
    /// `NavigableRiver` hex it is the biome the channel was cut through (the valley it yields, not
    /// open water). The client consults this **only** when `terrain == NavigableRiver` — elsewhere it
    /// is identical to `terrain` — so it is always meaningful even read unconditionally. Written from
    /// `Tile::resource_terrain()`.
    #[serde(default)]
    pub underlying_terrain: TerrainType,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogisticsLinkState {
    pub entity: u64,
    pub from: u64,
    pub to: u64,
    pub capacity: i64,
    pub flow: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TradeLinkKnowledge {
    pub openness: i64,
    pub leak_timer: u32,
    pub last_discovery: u32,
    pub decay: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TradeLinkState {
    pub entity: u64,
    pub from_faction: u32,
    pub to_faction: u32,
    pub throughput: i64,
    pub tariff: i64,
    pub knowledge: TradeLinkKnowledge,
    pub from_tile: u64,
    pub to_tile: u64,
    #[serde(default)]
    pub pending_fragments: Vec<KnownTechFragment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KnownTechFragment {
    pub discovery_id: u32,
    pub progress: i64,
    pub fidelity: i64,
}

/// One commodity entry in a band's local goods store (fixed-point raw quantity).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CohortStoreState {
    pub item: String,
    pub quantity: i64,
}

/// One staffed labor demand in a band's allocation (Early-Game Labor, slice 3a). `kind` is the
/// role (`"forage" | "hunt" | "scout" | "warrior"`); `target_x`/`target_y` locate a Forage tile or
/// a Hunt herd's position readout; `fauna_id`/`policy` carry the Hunt target + take policy. Doubles
/// as the client's allocation readout and the rollback-persisted staffing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LaborAssignmentState {
    pub kind: String,
    pub workers: u32,
    #[serde(default)]
    pub target_x: u32,
    #[serde(default)]
    pub target_y: u32,
    #[serde(default)]
    pub fauna_id: String,
    #[serde(default)]
    pub policy: String,
    /// Provisions this source actually produced this turn (per-source food-income breakdown). Derived
    /// per-turn at capture (0.0 on a rehydrated save before the next tick). Appended (append-only).
    #[serde(default)]
    pub actual_yield: f32,
    /// Provisions this source could yield without drawing down its stock this turn (forage: ≡
    /// `actual`; hunt: the herd's net regrowth). `actual > sustainable` is the overhunting signal.
    /// Derived per-turn at capture. Appended (append-only).
    #[serde(default)]
    pub sustainable_yield: f32,
    /// Minimum workers that would have produced this turn's take — the **overstaffing** signal.
    /// `workers > workers_needed` ⇒ the binding constraint was not labor, so the extra workers were
    /// idle. `0` when the source produced nothing. **Derived at every rung** since the intensification
    /// ladder's slice 7 — a tended patch / Field / corralled herd used to report a hardcoded `1`,
    /// which claimed one worker could carry home whatever the land offered. Derived per-turn at
    /// capture. Appended (append-only).
    #[serde(default)]
    pub workers_needed: u32,
    /// Provisions this source **offered that the crew could not collect** — the **understaffing**
    /// signal, and the exact mirror of [`Self::workers_needed`]'s overstaffing one:
    /// `production − actual_yield`, where *production* is what the source hands over this turn (the
    /// policy ceiling on a wild/tended source, the managed rate on a Field/pen) and *collection* is
    /// `workers × per-worker throughput`. Together the pair answers both halves of "is this source
    /// correctly staffed?": `workers > workers_needed` ⇒ drop some, `wasted_yield > 0` ⇒ add some.
    /// On a Field or a pen it is genuinely food left standing; on the drawn-down rungs it stays in the
    /// stock and regrows. Derived per-turn at capture. Appended (append-only).
    #[serde(default)]
    pub wasted_yield: f32,
    /// **THE overhunting ⚠, answered by the sim** — `SourceYield::overdraws` (`!managed &&
    /// policy.overdraws()`): does this take draw the stock below what it sustains? It replaces the
    /// client-derived `actual_yield > sustainable_yield` test, which mis-fires on a hunt's lumpy
    /// per-turn take (a kill turn cashes a whole banked animal, spiking `actual` above the steady
    /// sustainable rate even under Sustain). False for Sustain and the investment rungs
    /// (Cultivate/Tame/Corral/Sow) and every managed rung-3 source; true for Surplus/Market/Eradicate.
    /// A row with no yield (Scout/Warrior, or a rehydrated [`SourceYield::ZERO`]) is `false`. Derived
    /// per-turn at capture. Appended (append-only).
    #[serde(default)]
    pub overdraws: bool,
    /// **The steady per-turn income this source realizes** — the honest long-run average of the lumpy
    /// [`Self::actual_yield`]: `min(workers × per-worker throughput, this policy's steady per-turn
    /// ceiling)`, the pre-quantization rate the kill-credit bank is fed. On a whole-animal (hunt)
    /// source `actual_yield` pulses (0 on wait turns, spikes on kills) while this holds steady at
    /// ~`MSY`; on a continuous forage/Field source the two are equal. The client's headline "Food
    /// /turn" reads this instead of the jumpy `actual_yield`. Derived per-turn at capture (0 on a
    /// rehydrated save before the next tick). Appended (append-only).
    #[serde(default)]
    pub realized_yield: f32,
    /// **WHEN the food actually lands** — the discrete twin of [`Self::realized_yield`], from the
    /// *same* forward simulation run **with** the kill-credit bank. `arrival_schedule[i]` is the food
    /// delivered `i + 1` turns from now; the length is `labor_config.arrivals_horizon_turns` (20), and
    /// `0.0` marks a turn on which nothing lands. A big-game Sustain hunt reads lumpy — zeros between
    /// hauls, totalling ≈ `realized_yield × horizon`, because the bank moves the *timing* and not the
    /// total — while a forage patch or fast game is positive in every slot, a continuous source the
    /// client draws as a solid run. **Empty** on a row that was never projected (Scout/Warrior, or a
    /// rehydrated [`SourceYield::ZERO`]): read that as *no data*, never as famine. Derived per-turn at
    /// capture from the source's **post-take** state, so slot 0 is the *next* delivery. Appended
    /// (append-only).
    #[serde(default)]
    pub arrival_schedule: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PopulationCohortState {
    pub entity: u64,
    pub home: u64,
    #[serde(default)]
    pub current_x: u32,
    #[serde(default)]
    pub current_y: u32,
    #[serde(default)]
    pub is_traveling: bool,
    pub size: u32,
    /// Age brackets (fixed-point raw, `Scalar::SCALE` = 1.0) — persisted so a rollback restores
    /// the exact demographic structure. `children + working + elders` rounds to `size`.
    #[serde(default)]
    pub children: i64,
    #[serde(default)]
    pub working: i64,
    #[serde(default)]
    pub elders: i64,
    /// The band's local goods store — one entry per commodity (food under `"provisions"`),
    /// fixed-point raw quantities. Persisted so a rollback restores the exact larder.
    #[serde(default)]
    pub stores: Vec<CohortStoreState>,
    /// Turns this band has been simulated (settled duration). Gates knowledge-migration so a
    /// freshly-spawned band can't emigrate immediately; persisted so rollback preserves the gate.
    #[serde(default)]
    pub age_turns: u32,
    /// **TURNS until the larder is empty, income included** — the honest runway
    /// `larder / (consumption + pen_feed − income)`, resolved turn-by-turn off the sources'
    /// arrival schedules so it agrees with the client's FOOD OUTLOOK chart. `999.0` means "not
    /// food-limited" (no demand at all, or income that meets the drain). An expedition has no
    /// income, so it reduces to `provisions / consumption`. Computed at capture; see
    /// `core_sim::snapshot::population::larder_runway_turns`.
    ///
    /// **The `days` in the name is a MISNOMER pending a rename** — the sim has no days, only
    /// turns, and the client already renders it as "turns". Renaming the field across
    /// schema/native/client is a mechanical sweep held out of the arc that made it honest.
    #[serde(default)]
    pub days_of_food: f32,
    /// The command the band is running: one of `idle | harvest | hunt | follow | scout`.
    #[serde(default)]
    pub activity: String,
    /// The band's hunt/follow mode when pursuing fauna: `single` (one-shot hunt) or the follow
    /// policy (`sustain | surplus | market | eradicate`). Empty string when the band isn't
    /// pursuing fauna. Lets the client label a cancel button with the specific mode.
    #[serde(default)]
    pub hunt_mode: String,
    /// The band's per-source labor allocation (Early-Game Labor, slice 3a): one entry per staffed
    /// Forage tile / Hunt herd / Scout / Warrior demand. Doubles as the client readout and the
    /// rollback-persisted staffing.
    #[serde(default)]
    pub labor_assignments: Vec<LaborAssignmentState>,
    /// Whole working-age workers left unassigned (idle — they eat but produce nothing). Derived.
    #[serde(default)]
    pub idle_workers: u32,
    /// Whole assignable working-age workers this band supplies (the Σ-invariant ceiling). Derived.
    #[serde(default)]
    pub working_age: u32,
    /// The band's Chebyshev work radius (`LaborConfig.band_work_range`). Global labor config today
    /// (identical for every band); surfaced per-band so the client reads it off the selected band
    /// to draw the work-range ring. Sourced from `labor_config.json` at capture.
    #[serde(default)]
    pub work_range: u32,
    /// The band's effective **scout vantage distance** — how far its forward-observer vantage ring
    /// is posted out from the band (`min(vantage_distance_base + scouts × vantage_distance_per_scout,
    /// vantage_distance_max)` from `LaborConfig.scout`), `0` with no scouts. Derived per-band at
    /// capture. (Field name retained for wire compatibility; scouts now reveal by posting vantage
    /// points that see *around* obstacles, not the retired flat fog-pulse ring.)
    #[serde(default)]
    pub scout_reveal_radius: u32,
    /// Expedition discriminators (`docs/plan_exploration_and_sites.md` §2). `false`/`""`/empty for a
    /// normal band; a detached scouting party sets `is_expedition` and carries its mission/phase.
    /// Client-facing (distinct marker glyph/label, awaiting-orders state, Recall affordance).
    #[serde(default)]
    pub is_expedition: bool,
    /// `"scout"` (PR 2 adds `"hunt"`); empty for normal bands.
    #[serde(default)]
    pub expedition_mission: String,
    /// `"outbound"` | `"awaiting"` | `"returning"` | `"hunting"` | `"delivering"`; empty for normal
    /// bands.
    #[serde(default)]
    pub expedition_phase: String,
    /// Hunt mission only: target herd id (`HerdRegistry` fauna_id; a non-numeric string, so a
    /// string not a uint). Empty for scout/normal bands. Persisted so a rollback reconstructs
    /// `Hunt { fauna_id }`; also shown in the client hunt panel.
    #[serde(default)]
    pub expedition_target_herd: String,
    /// Hunt mission only: take policy string (`sustain|surplus|market|eradicate`; mirrors
    /// `hunt_mode`). Empty for scout/normal bands. Persisted so a rollback reconstructs
    /// `Hunt { fauna_id, policy }`; drives the client's per-policy label + policy-picker default.
    #[serde(default)]
    pub expedition_hunt_policy: String,
    /// The `BandTravel` destination tile while traveling (`is_traveling` gates it; `0,0` otherwise).
    /// Lets the client draw a destination hex + line from a selected band/expedition. Appended last
    /// in the FlatBuffers table (append-only wire discipline).
    #[serde(default)]
    pub travel_target_x: u32,
    #[serde(default)]
    pub travel_target_y: u32,
    /// Band's effective hunt reach = `band_work_range + hunt_leash_tiles` (the leash a Hunt
    /// assignment lapses past). Echoed per-cohort so the client offers a local hunt vs a hunting
    /// expedition by the clicked herd's distance. Appended last in the FlatBuffers table.
    #[serde(default)]
    pub hunt_reach: u32,
    /// Persistence-only: the real band (entity bits) that outfitted this party — a rollback
    /// re-attaches the expedition and resolves its home band from this.
    #[serde(default)]
    pub home_band_entity: u64,
    /// Persistence-only: whether the arrival ("awaiting orders") feed line has fired for the current
    /// `AwaitingOrders` latch.
    #[serde(default)]
    pub expedition_announced: bool,
    /// Persistence-only: observed-but-unreported tile coordinates (zipped `x`/`y`) — the expedition's
    /// comm-range-gated pending-reveal buffer, so a rollback preserves unreported findings.
    #[serde(default)]
    pub pending_reveal_x: Vec<u32>,
    #[serde(default)]
    pub pending_reveal_y: Vec<u32>,
    /// Server-side hard cap on an expedition party (`expedition_config.json` `max_party_size`). A
    /// global config lever echoed per-cohort (same idiom as `work_range`) so the client outfit
    /// stepper pre-clamps to `min(idle_workers, this)`. Populated for every cohort.
    #[serde(default)]
    pub max_expedition_party_size: u32,
    /// Hunt expedition only: the carry cap = `party_workers × expedition_config.hunt.per_worker_carry`
    /// (the provisions ceiling the party fills to before auto-Delivering). Capture-only, `0` for
    /// scouts + normal bands. Lets the client render carried/cap + a FULL state.
    #[serde(default)]
    pub expedition_carry_cap: f32,
    /// Which supply network this band belongs to this turn: `0` = not in a multi-band network,
    /// `>= 1` = a per-snapshot id shared by all bands in the same connected component. Derived and
    /// recomputed every turn (not persisted for rollback).
    #[serde(default)]
    pub supply_network_id: u32,
    /// This turn's signed morale delta (fixed-point raw, `Scalar::SCALE` = 1.0). The client renders
    /// it as a rising/falling trend arrow. Derived at capture (not persisted for rollback).
    #[serde(default)]
    pub morale_delta: i64,
    /// Dominant negative morale driver this turn: `0 = None, 1 = Terrain, 2 = Cold, 3 = Unrest`.
    /// Names *why* morale is falling. Derived at capture (not persisted for rollback).
    #[serde(default)]
    pub morale_cause: u8,
    /// Civilization Wellbeing (`docs/plan_civ_wellbeing.md`). Productivity modifier-stack result
    /// (`output = base × Π(modifiers)`), fixed-point raw (`Scalar::SCALE` = 100% output). Derived.
    #[serde(default = "default_output_multiplier")]
    pub output_multiplier: i64,
    /// Discontented share of the band this turn, fixed-point raw 0..1 (`Scalar::SCALE` = fully
    /// discontented). Derived at capture.
    #[serde(default)]
    pub discontent_fraction: i64,
    /// People who emigrated from / immigrated into this band last turn (discontent-driven
    /// migration). Derived at capture.
    #[serde(default)]
    pub last_emigrated: u32,
    #[serde(default)]
    pub last_immigrated: u32,
    /// Severity × duration grievance accumulator, fixed-point raw. Reserved for a future revolution
    /// consequence (Phase 1 only surfaces it). **Persisted** for rollback (unlike the other derived
    /// wellbeing fields here) so brewing unrest survives a rewind.
    #[serde(default)]
    pub grievance: i64,
    /// Layer-1 named morale contributions whose signed sum IS `morale_delta` — the itemized
    /// breakdown. Fixed-point raw. Derived at capture.
    #[serde(default)]
    pub morale_settling: i64,
    #[serde(default)]
    pub morale_terrain: i64,
    #[serde(default)]
    pub morale_climate: i64,
    #[serde(default)]
    pub morale_unrest: i64,
    pub morale: i64,
    pub generation: u16,
    pub faction: u32,
    pub knowledge_fragments: Vec<KnownTechFragment>,
    #[serde(default)]
    pub migration: Option<PendingMigrationState>,
    #[serde(default)]
    pub harvest_task: Option<HarvestTaskState>,
    #[serde(default)]
    pub scout_task: Option<ScoutTaskState>,
    #[serde(default)]
    pub accessible_stockpile: Option<AccessibleStockpileState>,
    /// The band's resolved settlement-progression stage (data-driven; resolved in the sim from the
    /// ordered `settlement_stage_config.json` list against the band's `size`). Pure presentation
    /// pass-through — the client draws `icon` and shows `label`. Appended last (append-only schema
    /// discipline); a pre-stage snapshot decodes to the empty default.
    #[serde(default)]
    pub settlement_stage: SettlementStageViewState,
    /// Band-level food income this turn = Σ of every worked source's `actual_yield` (the per-source
    /// breakdown summed). Derived per-turn at capture (0.0 on a rehydrated save before the next
    /// tick). Appended last (append-only schema discipline). Lets the client draw a food ledger
    /// footer without re-summing the assignment rows.
    #[serde(default)]
    pub food_income: f32,
    /// Band-level per-turn food consumption = `food_demand(children, working, elders)` (the same
    /// one-turn demand `days_of_food` divides by) — **the PEOPLE's food only**. Derived per-turn at
    /// capture. Appended last.
    #[serde(default)]
    pub food_consumption: f32,
    /// Hunt levers — global config echoed per-cohort (same idiom as `max_expedition_party_size`, and
    /// populated for **every** cohort, since the outfit/hunt UI lives on the resident-band panel).
    ///
    /// The pre-launch **expedition** trip length is **not** computed from these: the client reads the
    /// sim's simulated answer out of the target herd's [`HuntTripEstimateState`] table
    /// (policy × `party_workers` → `turns_to_fill`) and flags NOT VIABLE when `turns_to_fill >
    /// expedition_viability_warn_turns` (or `turns_to_fill == 0` → "won't fill"). An `eradicate`
    /// party has `delivers_food == false`: render "no food delivered (denial)", never an ETA.
    ///
    /// One hunter's per-turn provisions throughput (`labor_config.hunt.per_worker_biomass_capacity ×
    /// fauna_config.hunt.provisions_per_biomass`). With a herd's **band** ceiling this drives the
    /// resident-band local-hunt yield preview.
    #[serde(default)]
    pub hunt_per_worker_provisions: f32,
    /// Turns-to-fill past which a trip is flagged NOT VIABLE
    /// (`expedition_config.hunt.viability_warn_turns`).
    #[serde(default)]
    pub expedition_viability_warn_turns: u32,
    /// **The food this band actually PAID for pen feed this turn**, summed across every corral it
    /// keeps — the real `LocalStore::take` debit, not the demanded amount (a band that could only
    /// part-pay records only what it handed over, and its herds starve for the rest).
    ///
    /// A pen's feed comes straight off the band's stores, so it is in **neither** [`Self::food_income`]
    /// **nor** [`Self::food_consumption`]. Render it as its own **negative** row in the food ledger —
    /// "my people ate X" and "my animals ate Y" are deliberately separate lines, and it is *not* folded
    /// into `food_consumption`. The sim answers it so the client does no arithmetic:
    ///
    /// ```text
    /// larder_delta == food_income − food_consumption − pen_feed_upkeep
    /// ```
    ///
    /// (pinned by `core_sim/tests/fauna_husbandry.rs`). Derived per-turn, not persisted (a rehydrated
    /// cohort reads `0.0` until the next tick), exactly like `food_income`. Appended.
    #[serde(default)]
    pub pen_feed_upkeep: f32,
    /// One worker's carry contribution to a hunt expedition's haul
    /// (`expedition_config.hunt.per_worker_carry`). Global config echoed per-cohort (same idiom as
    /// [`Self::max_expedition_party_size`] / `expedition_viability_warn_turns` /
    /// `hunt_per_worker_provisions`), populated for **every** cohort since the outfit UI lives on the
    /// resident-band panel. The client computes a hypothetical party's pre-launch HAUL as
    /// `party_workers × expedition_per_worker_carry` (the carry cap the pack fills to before
    /// auto-Delivering; a launched party's own echo is [`Self::expedition_carry_cap`]). Appended.
    #[serde(default)]
    pub expedition_per_worker_carry: f32,
    /// A band's move speed (`labor_config.band_move_tiles_per_turn`). Global config echoed per-cohort
    /// (same idiom as the levers above). The client adds a raid's round-trip travel to the
    /// band-agnostic pre-launch `hunt_trip_estimates` as
    /// `ceil(2 × hex_distance(selected_band, herd) / band_move_tiles_per_turn)`. Appended.
    #[serde(default)]
    pub band_move_tiles_per_turn: f32,
    /// **The band's STEADY food income this turn = Σ of every worked source's [`LaborAssignmentState::realized_yield`]**
    /// (the per-source steady averages summed) — the honest long-run average of the lumpy
    /// [`Self::food_income`]. Because a whole-animal hunt pays in pulses, `food_income` swings
    /// turn-to-turn while this holds steady; the client's headline "Food /turn" reads this. Distinct
    /// from `food_income`, which stays the real arrivals and preserves the
    /// `larder_delta == food_income − food_consumption − pen_feed_upkeep` ledger identity. Derived
    /// per-turn at capture (0.0 on a rehydrated save before the next tick). Appended (append-only).
    #[serde(default)]
    pub food_income_average: f32,
}

/// Presentation view of a band's resolved settlement stage (mirror of the `SettlementStageView`
/// FlatBuffers sub-table). All three fields are opaque strings the sim never interprets: `id` is a
/// stable stage key, `label` a tooltip name, `icon` a presentation token (emoji now, asset key
/// later). Adding a stage is a pure `settlement_stage_config.json` edit — no code change here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SettlementStageViewState {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PendingMigrationState {
    pub destination: u32,
    pub eta: u16,
    #[serde(default)]
    pub fragments: Vec<KnownTechFragment>,
}

fn default_harvest_task_kind() -> String {
    "harvest".to_string()
}

/// Fixed-point 100% output (`Scalar::SCALE` = 1e6) — the neutral productivity multiplier a snapshot
/// without a `output_multiplier` field (pre-wellbeing) decodes to.
fn default_output_multiplier() -> i64 {
    1_000_000
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HarvestTaskState {
    #[serde(default = "default_harvest_task_kind")]
    pub kind: String,
    pub module: String,
    pub band_label: String,
    pub target_tile: u64,
    pub target_x: u32,
    pub target_y: u32,
    pub travel_remaining: u32,
    pub travel_total: u32,
    pub gather_remaining: u32,
    pub gather_total: u32,
    pub provisions_reward: i64,
    pub trade_goods_reward: i64,
    pub started_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ScoutTaskState {
    pub band_label: String,
    pub target_tile: u64,
    pub target_x: u32,
    pub target_y: u32,
    pub travel_remaining: u32,
    pub travel_total: u32,
    pub reveal_radius: u32,
    pub reveal_duration: u64,
    pub morale_gain: f32,
    pub started_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AccessibleStockpileEntryState {
    pub item: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AccessibleStockpileState {
    pub radius: u32,
    #[serde(default)]
    pub entries: Vec<AccessibleStockpileEntryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DiscoveryProgressEntry {
    pub faction: u32,
    pub discovery: u32,
    pub progress: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PowerNodeState {
    pub entity: u64,
    pub node_id: u32,
    pub generation: i64,
    pub demand: i64,
    pub efficiency: i64,
    pub storage_level: i64,
    pub storage_capacity: i64,
    pub stability: i64,
    pub surplus: i64,
    pub deficit: i64,
    pub incident_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PowerIncidentSeverity {
    #[default]
    Warning = 0,
    Critical = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerIncidentState {
    pub node_id: u32,
    pub severity: PowerIncidentSeverity,
    pub deficit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerTelemetryState {
    pub total_supply: i64,
    pub total_demand: i64,
    pub total_storage: i64,
    pub total_capacity: i64,
    pub grid_stress_avg: f32,
    pub surplus_margin: f32,
    pub instability_alerts: u32,
    pub incidents: Vec<PowerIncidentState>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CultureLayerScope {
    #[default]
    Global = 0,
    Regional = 1,
    Local = 2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum CultureTraitAxis {
    PassiveAggressive = 0,
    OpenClosed = 1,
    CollectivistIndividualist = 2,
    TraditionalistRevisionist = 3,
    HierarchicalEgalitarian = 4,
    SyncreticPurist = 5,
    AsceticIndulgent = 6,
    PragmaticIdealistic = 7,
    RationalistMystical = 8,
    ExpansionistInsular = 9,
    AdaptiveStubborn = 10,
    HonorBoundOpportunistic = 11,
    MeritOrientedLineageOriented = 12,
    SecularDevout = 13,
    PluralisticMonocultural = 14,
}

impl CultureTraitAxis {
    pub const ALL: [CultureTraitAxis; 15] = [
        CultureTraitAxis::PassiveAggressive,
        CultureTraitAxis::OpenClosed,
        CultureTraitAxis::CollectivistIndividualist,
        CultureTraitAxis::TraditionalistRevisionist,
        CultureTraitAxis::HierarchicalEgalitarian,
        CultureTraitAxis::SyncreticPurist,
        CultureTraitAxis::AsceticIndulgent,
        CultureTraitAxis::PragmaticIdealistic,
        CultureTraitAxis::RationalistMystical,
        CultureTraitAxis::ExpansionistInsular,
        CultureTraitAxis::AdaptiveStubborn,
        CultureTraitAxis::HonorBoundOpportunistic,
        CultureTraitAxis::MeritOrientedLineageOriented,
        CultureTraitAxis::SecularDevout,
        CultureTraitAxis::PluralisticMonocultural,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum CultureTensionKind {
    DriftWarning = 0,
    AssimilationPush = 1,
    SchismRisk = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CultureTraitEntry {
    pub axis: CultureTraitAxis,
    pub baseline: i64,
    pub modifier: i64,
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CultureLayerState {
    pub id: u32,
    pub owner: u64,
    pub parent: u32,
    pub scope: CultureLayerScope,
    pub traits: Vec<CultureTraitEntry>,
    pub divergence: i64,
    pub soft_threshold: i64,
    pub hard_threshold: i64,
    pub ticks_above_soft: u16,
    pub ticks_above_hard: u16,
    pub last_updated_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CultureTensionState {
    pub layer_id: u32,
    pub scope: CultureLayerScope,
    pub owner: u64,
    pub severity: i64,
    pub timer: u16,
    pub kind: CultureTensionKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CorruptionSubsystem {
    #[default]
    Logistics = 0,
    Trade = 1,
    Military = 2,
    Governance = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CorruptionEntry {
    pub subsystem: CorruptionSubsystem,
    pub intensity: i64,
    pub incident_id: u64,
    pub exposure_timer: u16,
    pub restitution_window: u16,
    pub last_update_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CorruptionLedger {
    pub entries: Vec<CorruptionEntry>,
    pub reputation_modifier: i64,
    pub audit_capacity: u16,
}

impl CorruptionLedger {
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn register_incident(&mut self, entry: CorruptionEntry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.incident_id == entry.incident_id)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn incident_mut(&mut self, incident_id: u64) -> Option<&mut CorruptionEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.incident_id == incident_id)
    }

    pub fn remove_incident(&mut self, incident_id: u64) -> Option<CorruptionEntry> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.incident_id == incident_id)?;
        Some(self.entries.remove(index))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceDomain {
    Sentiment = 0,
    Discovery = 1,
    Logistics = 2,
    Production = 3,
    Humanitarian = 4,
}

impl InfluenceDomain {
    pub fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

pub fn influence_domain_mask(domains: &[InfluenceDomain]) -> u32 {
    domains.iter().fold(0u32, |acc, domain| acc | domain.bit())
}

pub fn influence_domains_from_mask(mask: u32) -> Vec<InfluenceDomain> {
    let mut domains = Vec::new();
    for value in 0..=4 {
        let domain = match value {
            0 => InfluenceDomain::Sentiment,
            1 => InfluenceDomain::Discovery,
            2 => InfluenceDomain::Logistics,
            3 => InfluenceDomain::Production,
            4 => InfluenceDomain::Humanitarian,
            _ => continue,
        };
        if mask & domain.bit() != 0 {
            domains.push(domain);
        }
    }
    domains
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceScopeKind {
    Local = 0,
    Regional = 1,
    Global = 2,
    Generation = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceLifecycle {
    Potential = 0,
    Active = 1,
    Dormant = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfluencerCultureResonanceEntry {
    pub axis: CultureTraitAxis,
    pub weight: i64,
    pub output: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfluentialIndividualState {
    pub id: u32,
    pub name: String,
    pub influence: i64,
    pub growth_rate: i64,
    pub baseline_growth: i64,
    pub notoriety: i64,
    pub sentiment_knowledge: i64,
    pub sentiment_trust: i64,
    pub sentiment_equity: i64,
    pub sentiment_agency: i64,
    pub sentiment_weight_knowledge: i64,
    pub sentiment_weight_trust: i64,
    pub sentiment_weight_equity: i64,
    pub sentiment_weight_agency: i64,
    pub logistics_bonus: i64,
    pub morale_bonus: i64,
    pub power_bonus: i64,
    pub logistics_weight: i64,
    pub morale_weight: i64,
    pub power_weight: i64,
    pub support_charge: i64,
    pub suppress_pressure: i64,
    pub domains: u32,
    pub scope: InfluenceScopeKind,
    pub generation_scope: u16,
    pub supported: bool,
    pub suppressed: bool,
    pub lifecycle: InfluenceLifecycle,
    pub coherence: i64,
    pub ticks_in_status: u16,
    pub audience_generations: Vec<u16>,
    pub support_popular: i64,
    pub support_peer: i64,
    pub support_institutional: i64,
    pub support_humanitarian: i64,
    pub weight_popular: i64,
    pub weight_peer: i64,
    pub weight_institutional: i64,
    pub weight_humanitarian: i64,
    pub culture_resonance: Vec<InfluencerCultureResonanceEntry>,
}

impl InfluentialIndividualState {
    pub const NO_GENERATION_SCOPE: u16 = u16::MAX;

    pub fn generation_scope(&self) -> Option<u16> {
        if self.generation_scope == Self::NO_GENERATION_SCOPE {
            None
        } else {
            Some(self.generation_scope)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AxisBiasState {
    pub knowledge: i64,
    pub trust: i64,
    pub equity: i64,
    pub agency: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum SentimentDriverCategory {
    Policy = 0,
    Incident = 1,
    Influencer = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SentimentDriverState {
    pub category: SentimentDriverCategory,
    pub label: String,
    pub value: i64,
    pub weight: i64,
}

impl Default for SentimentDriverState {
    fn default() -> Self {
        Self {
            category: SentimentDriverCategory::Policy,
            label: String::new(),
            value: 0,
            weight: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SentimentAxisTelemetry {
    pub policy: i64,
    pub incidents: i64,
    pub influencers: i64,
    pub total: i64,
    pub drivers: Vec<SentimentDriverState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SentimentTelemetryState {
    pub knowledge: SentimentAxisTelemetry,
    pub trust: SentimentAxisTelemetry,
    pub equity: SentimentAxisTelemetry,
    pub agency: SentimentAxisTelemetry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub logistics: Vec<LogisticsLinkState>,
    pub trade_links: Vec<TradeLinkState>,
    pub populations: Vec<PopulationCohortState>,
    pub power: Vec<PowerNodeState>,
    pub power_metrics: PowerTelemetryState,
    pub great_discovery_definitions: Vec<GreatDiscoveryDefinitionState>,
    pub great_discoveries: Vec<GreatDiscoveryState>,
    pub great_discovery_progress: Vec<GreatDiscoveryProgressState>,
    pub great_discovery_telemetry: GreatDiscoveryTelemetryState,
    pub knowledge_ledger: Vec<KnowledgeLedgerEntryState>,
    pub knowledge_timeline: Vec<KnowledgeTimelineEventState>,
    pub knowledge_metrics: KnowledgeMetricsState,
    pub crisis_telemetry: CrisisTelemetryState,
    pub crisis_overlay: CrisisOverlayState,
    pub victory: VictorySnapshotState,
    #[serde(default)]
    pub capability_flags: u32,
    #[serde(default)]
    pub campaign_profiles: Vec<CampaignProfileState>,
    #[serde(default)]
    pub command_events: Vec<CommandEventState>,
    /// The Telling's fork tier, per faction: what is on the table right now.
    #[serde(default)]
    pub pending_forks: Vec<PendingForksState>,
    /// The Telling's effective stance per faction and axis.
    #[serde(default)]
    pub stance_axes: Vec<StanceState>,
    /// The Telling's narrator medium per faction (presentational — see `VoiceMediumState`).
    #[serde(default)]
    pub voice_medium: Vec<VoiceMediumState>,
    #[serde(default)]
    pub herds: Vec<HerdTelemetryState>,
    /// Authoritative herd sim state (`HerdRegistry`), round-tripped for rollback correctness —
    /// distinct from the lossy display `herds` above (which the client consumes). Not wired to the
    /// FlatBuffers client stream; rollback restore reads it via `HerdRegistry::update_from_states`.
    #[serde(default)]
    pub herd_registry: Vec<HerdState>,
    /// Authoritative depletable-forage sim state (`ForageRegistry`), round-tripped for rollback
    /// correctness (biomass / ecology phase per patch). Like `herd_registry`, this is not wired to
    /// the FlatBuffers client stream; rollback restore reads it via `ForageRegistry::update_from_states`.
    #[serde(default)]
    pub forage_registry: Vec<ForageState>,
    /// Authoritative graze/pasture sim state (`GrazeRegistry`), round-tripped for rollback correctness
    /// (biomass / ecology phase per land tile). Like `herd_registry` / `forage_registry` this is the
    /// *sim* record and is not on the FlatBuffers client stream — the client reads graze off the
    /// per-tile `TileState.graze_*` fields. Restore reads it via `GrazeRegistry::update_from_states`.
    #[serde(default)]
    pub graze_registry: Vec<GrazeState>,
    /// The Telling's narrative memory (`BeatLedger`), round-tripped for rollback correctness.
    /// Like the registries above this is the *sim* record and is not on the FlatBuffers client
    /// stream; restore reads it via `BeatLedger::from_state`.
    #[serde(default)]
    pub beat_ledger: BeatLedgerState,
    #[serde(default)]
    pub food_modules: Vec<FoodModuleState>,
    #[serde(default)]
    pub faction_inventory: Vec<FactionInventoryState>,
    #[serde(default)]
    pub sedentarization: Vec<SedentarizationState>,
    #[serde(default)]
    pub discovered_sites: Vec<DiscoveredSitesState>,
    #[serde(default)]
    pub demographics: Vec<PopulationDemographicsState>,
    /// Per-tile depletable-forage cultivation/ecology display state (Intensification Phase 1a).
    #[serde(default)]
    pub forage_patches: Vec<ForagePatchState>,
    /// Per-faction Cultivation/Herding knowledge progress (Intensification Rung 1b/1c).
    #[serde(default)]
    pub intensification_knowledge: Vec<IntensificationKnowledgeState>,
    pub moisture_raster: FloatRasterState,
    pub elevation_overlay: ElevationOverlayState,
    /// Climate-band cut points (`docs/plan_climate_authority.md` §8.3), a per-map constant.
    #[serde(default)]
    pub climate_bands: ClimateBandsState,
    pub start_marker: Option<StartMarkerState>,
    pub terrain: TerrainOverlayState,
    pub logistics_raster: ScalarRasterState,
    pub sentiment_raster: ScalarRasterState,
    pub corruption_raster: ScalarRasterState,
    pub fog_raster: ScalarRasterState,
    pub culture_raster: ScalarRasterState,
    pub military_raster: ScalarRasterState,
    #[serde(default)]
    pub visibility_raster: ScalarRasterState,
    pub axis_bias: AxisBiasState,
    pub sentiment: SentimentTelemetryState,
    pub generations: Vec<GenerationState>,
    pub corruption: CorruptionLedger,
    pub influencers: Vec<InfluentialIndividualState>,
    pub culture_layers: Vec<CultureLayerState>,
    pub culture_tensions: Vec<CultureTensionState>,
    pub discovery_progress: Vec<DiscoveryProgressEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldDelta {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub removed_tiles: Vec<u64>,
    pub logistics: Vec<LogisticsLinkState>,
    pub removed_logistics: Vec<u64>,
    pub trade_links: Vec<TradeLinkState>,
    pub removed_trade_links: Vec<u64>,
    pub populations: Vec<PopulationCohortState>,
    pub removed_populations: Vec<u64>,
    pub power: Vec<PowerNodeState>,
    pub removed_power: Vec<u64>,
    pub power_metrics: Option<PowerTelemetryState>,
    pub great_discovery_definitions: Option<Vec<GreatDiscoveryDefinitionState>>,
    pub great_discoveries: Vec<GreatDiscoveryState>,
    pub great_discovery_progress: Vec<GreatDiscoveryProgressState>,
    pub great_discovery_telemetry: Option<GreatDiscoveryTelemetryState>,
    pub knowledge_ledger: Vec<KnowledgeLedgerEntryState>,
    pub removed_knowledge_ledger: Vec<u64>,
    pub knowledge_metrics: Option<KnowledgeMetricsState>,
    pub victory: Option<VictorySnapshotState>,
    pub capability_flags: Option<u32>,
    pub command_events: Option<Vec<CommandEventState>>,
    pub pending_forks: Option<Vec<PendingForksState>>,
    pub stance_axes: Option<Vec<StanceState>>,
    pub voice_medium: Option<Vec<VoiceMediumState>>,
    pub knowledge_timeline: Vec<KnowledgeTimelineEventState>,
    pub crisis_telemetry: Option<CrisisTelemetryState>,
    pub crisis_overlay: Option<CrisisOverlayState>,
    pub herds: Option<Vec<HerdTelemetryState>>,
    pub food_modules: Option<Vec<FoodModuleState>>,
    pub faction_inventory: Option<Vec<FactionInventoryState>>,
    pub sedentarization: Option<Vec<SedentarizationState>>,
    pub discovered_sites: Option<Vec<DiscoveredSitesState>>,
    pub demographics: Option<Vec<PopulationDemographicsState>>,
    pub forage_patches: Option<Vec<ForagePatchState>>,
    pub intensification_knowledge: Option<Vec<IntensificationKnowledgeState>>,
    pub moisture_raster: Option<FloatRasterState>,
    pub elevation_overlay: Option<ElevationOverlayState>,
    /// Climate-band cut points; a per-map constant, so a delta re-sends it only when the map is
    /// (re)generated. `None` means unchanged.
    #[serde(default)]
    pub climate_bands: Option<ClimateBandsState>,
    pub start_marker: Option<StartMarkerState>,
    pub axis_bias: Option<AxisBiasState>,
    pub sentiment: Option<SentimentTelemetryState>,
    pub logistics_raster: Option<ScalarRasterState>,
    pub sentiment_raster: Option<ScalarRasterState>,
    pub corruption_raster: Option<ScalarRasterState>,
    pub fog_raster: Option<ScalarRasterState>,
    pub culture_raster: Option<ScalarRasterState>,
    pub military_raster: Option<ScalarRasterState>,
    pub visibility_raster: Option<ScalarRasterState>,
    pub generations: Vec<GenerationState>,
    pub removed_generations: Vec<u16>,
    pub corruption: Option<CorruptionLedger>,
    pub influencers: Vec<InfluentialIndividualState>,
    pub removed_influencers: Vec<u32>,
    pub terrain: Option<TerrainOverlayState>,
    pub culture_layers: Vec<CultureLayerState>,
    pub removed_culture_layers: Vec<u32>,
    pub culture_tensions: Vec<CultureTensionState>,
    pub discovery_progress: Vec<DiscoveryProgressEntry>,
}

impl WorldSnapshot {
    pub fn finalize(mut self) -> Self {
        let hash = hash_snapshot(&self);
        let mut header = self.header;
        header.hash = hash;
        self.header = header;
        self
    }
}

pub fn hash_snapshot(snapshot: &WorldSnapshot) -> u64 {
    let mut clone = snapshot.clone();
    clone.header.hash = 0;
    let encoded = bincode::serialize(&clone).expect("snapshot serialization for hashing");
    let mut hasher = RandomState::with_seeds(0, 0, 0, 0).build_hasher();
    hasher.write(&encoded);
    hasher.finish()
}

pub fn encode_snapshot(snapshot: &WorldSnapshot) -> bincode::Result<Vec<u8>> {
    bincode::serialize(snapshot)
}

pub fn encode_delta(delta: &WorldDelta) -> bincode::Result<Vec<u8>> {
    bincode::serialize(delta)
}

pub fn encode_snapshot_flatbuffer(snapshot: &WorldSnapshot) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let offset = build_snapshot_flatbuffer(&mut builder, snapshot);
    builder.finish(offset, None);
    builder.finished_data().to_vec()
}

pub fn encode_delta_flatbuffer(delta: &WorldDelta) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let offset = build_delta_flatbuffer(&mut builder, delta);
    builder.finish(offset, None);
    builder.finished_data().to_vec()
}

pub fn encode_snapshot_json(snapshot: &WorldSnapshot) -> serde_json::Result<String> {
    serde_json::to_string(snapshot)
}

pub fn decode_snapshot_json(data: &str) -> serde_json::Result<WorldSnapshot> {
    serde_json::from_str(data)
}

pub fn encode_delta_json(delta: &WorldDelta) -> serde_json::Result<String> {
    serde_json::to_string(delta)
}

pub fn decode_delta_json(data: &str) -> serde_json::Result<WorldDelta> {
    serde_json::from_str(data)
}

/// A self-describing on-disk export of a running game's map: the full
/// [`WorldSnapshot`] plus the resolved worldgen seed and preset needed to
/// reproduce it. Written by the `export_map` command and consumed as a test
/// fixture (see [`decode_map_export_json`]). Wrapping the snapshot rather than
/// adding a seed to [`SnapshotHeader`] keeps the wire schema untouched while
/// giving offline consumers everything in one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapExport {
    /// Resolved worldgen seed the running game was generated from.
    pub seed: u64,
    /// Preset id the map was generated with (empty when none was active).
    pub preset: String,
    /// Terrain grid width in tiles; mirrors `snapshot.terrain.width` so the
    /// row-major `(x, y)` indexing of the samples is self-documenting.
    pub width: u32,
    /// Terrain grid height in tiles; mirrors `snapshot.terrain.height`.
    pub height: u32,
    /// Full world snapshot captured at export time.
    pub snapshot: WorldSnapshot,
}

impl MapExport {
    /// Build an export from a captured snapshot, deriving the grid dimensions
    /// from the terrain overlay so callers cannot desync `width`/`height` from
    /// the sample buffer.
    pub fn from_snapshot(seed: u64, preset: impl Into<String>, snapshot: WorldSnapshot) -> Self {
        let width = snapshot.terrain.width;
        let height = snapshot.terrain.height;
        Self {
            seed,
            preset: preset.into(),
            width,
            height,
            snapshot,
        }
    }

    /// Return the terrain sample at row-major `(x, y)`, or `None` when the
    /// coordinate is outside the grid. This is the canonical way for offline
    /// consumers (tests, inspection) to reference a hex by coordinate.
    pub fn tile_at(&self, x: u32, y: u32) -> Option<&TerrainSample> {
        // Use the terrain overlay's own dimensions as canonical rather than the
        // top-level `width`/`height` mirrors: a hand-edited or corrupted export
        // could desync the mirrors from the sample buffer, and indexing off a
        // stale mirror would silently return the wrong (but in-bounds) tile.
        let width = self.snapshot.terrain.width;
        let height = self.snapshot.terrain.height;
        if x >= width || y >= height {
            return None;
        }
        let idx = (y as usize) * (width as usize) + (x as usize);
        self.snapshot.terrain.samples.get(idx)
    }
}

/// Encode a [`MapExport`] as pretty-printed JSON (human-readable for offline
/// inspection).
pub fn encode_map_export_json(export: &MapExport) -> serde_json::Result<String> {
    serde_json::to_string_pretty(export)
}

/// Decode a [`MapExport`] previously written by [`encode_map_export_json`].
pub fn decode_map_export_json(data: &str) -> serde_json::Result<MapExport> {
    serde_json::from_str(data)
}

fn build_snapshot_flatbuffer<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::Envelope<'a>> {
    let campaign_label_fb = snapshot
        .header
        .campaign_label
        .as_ref()
        .and_then(|label| create_campaign_label(builder, label));
    let victory_state = create_victory_state(builder, &snapshot.victory);
    let server_build_fb = builder.create_string(&snapshot.header.server_build);

    let header = fb::SnapshotHeader::create(
        builder,
        &fb::SnapshotHeaderArgs {
            tick: snapshot.header.tick,
            tileCount: snapshot.header.tile_count,
            logisticsCount: snapshot.header.logistics_count,
            tradeLinkCount: snapshot.header.trade_link_count,
            populationCount: snapshot.header.population_count,
            powerCount: snapshot.header.power_count,
            influencerCount: snapshot.header.influencer_count,
            hash: snapshot.header.hash,
            campaignLabel: campaign_label_fb,
            victory: Some(victory_state),
            wrapHorizontal: snapshot.header.wrap_horizontal,
            serverBuild: Some(server_build_fb),
            worldEpoch: snapshot.header.world_epoch,
        },
    );

    let map = serialize_map_section(builder, snapshot);
    let economy = serialize_economy_section(builder, snapshot);
    let population = serialize_population_section(builder, snapshot);
    let subsistence = serialize_subsistence_section(builder, snapshot);
    let knowledge = serialize_knowledge_section(builder, snapshot);
    let governance = serialize_governance_section(builder, snapshot);
    let culture = serialize_culture_section(builder, snapshot);
    let vision = serialize_vision_section(builder, snapshot);
    let campaign = serialize_campaign_section(builder, snapshot, victory_state);

    let snapshot_table = fb::WorldSnapshot::create(
        builder,
        &fb::WorldSnapshotArgs {
            header: Some(header),
            capabilityFlags: snapshot.capability_flags,
            map: Some(map),
            economy: Some(economy),
            population: Some(population),
            subsistence: Some(subsistence),
            knowledge: Some(knowledge),
            governance: Some(governance),
            culture: Some(culture),
            vision: Some(vision),
            campaign: Some(campaign),
        },
    );

    fb::Envelope::create(
        builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::snapshot,
            payload: Some(snapshot_table.as_union_value()),
        },
    )
}

fn build_delta_flatbuffer<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::Envelope<'a>> {
    let campaign_label_fb = delta
        .header
        .campaign_label
        .as_ref()
        .and_then(|label| create_campaign_label(builder, label));
    let victory_state = delta
        .victory
        .as_ref()
        .map(|state| create_victory_state(builder, state));

    // Deltas fire every turn and only full snapshots populate server_build, so omit the
    // field (leave it None) when empty instead of serializing an empty string each delta.
    let server_build_fb = (!delta.header.server_build.is_empty())
        .then(|| builder.create_string(&delta.header.server_build));
    let header = fb::SnapshotHeader::create(
        builder,
        &fb::SnapshotHeaderArgs {
            tick: delta.header.tick,
            tileCount: delta.header.tile_count,
            logisticsCount: delta.header.logistics_count,
            tradeLinkCount: delta.header.trade_link_count,
            populationCount: delta.header.population_count,
            powerCount: delta.header.power_count,
            influencerCount: delta.header.influencer_count,
            hash: delta.header.hash,
            campaignLabel: campaign_label_fb,
            victory: victory_state,
            wrapHorizontal: delta.header.wrap_horizontal,
            serverBuild: server_build_fb,
            worldEpoch: delta.header.world_epoch,
        },
    );

    let map = serialize_map_section_delta(builder, delta);
    let economy = serialize_economy_section_delta(builder, delta);
    let population = serialize_population_section_delta(builder, delta);
    let subsistence = serialize_subsistence_section_delta(builder, delta);
    let knowledge = serialize_knowledge_section_delta(builder, delta);
    let governance = serialize_governance_section_delta(builder, delta);
    let culture = serialize_culture_section_delta(builder, delta);
    let vision = serialize_vision_section_delta(builder, delta);
    let campaign = serialize_campaign_section_delta(builder, delta, victory_state);

    let delta_table = fb::WorldDelta::create(
        builder,
        &fb::WorldDeltaArgs {
            header: Some(header),
            capabilityFlags: delta.capability_flags.unwrap_or(0),
            map: Some(map),
            economy: Some(economy),
            population: Some(population),
            subsistence: Some(subsistence),
            knowledge: Some(knowledge),
            governance: Some(governance),
            culture: Some(culture),
            vision: Some(vision),
            campaign: Some(campaign),
        },
    );

    fb::Envelope::create(
        builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::delta,
            payload: Some(delta_table.as_union_value()),
        },
    )
}

// ---------------------------------------------------------------------------
// Per-section FlatBuffers serializers (docs/plan_snapshot_and_systems_decomposition.md §1).
// Each root nests one section table per subsystem; one helper per section per
// root builds its child offsets then the section table, so a future field
// addition to a section localizes to a single helper instead of the mega
// `build_*_flatbuffer` bodies. The delta variants preserve the exact per-field
// Option/empty-vector handling the flat delta used; `removed*` lists and
// snapshot-only fields are left unset on the side that does not carry them.
// ---------------------------------------------------------------------------

fn serialize_map_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::MapSection<'a>> {
    let tiles = create_tiles(builder, &snapshot.tiles);
    let terrain_overlay = create_terrain_overlay(builder, &snapshot.terrain);
    let elevation_overlay = create_elevation_overlay(builder, &snapshot.elevation_overlay);
    let moisture_raster = create_float_raster(builder, &snapshot.moisture_raster);
    let climate_bands = create_climate_bands(builder, &snapshot.climate_bands);
    fb::MapSection::create(
        builder,
        &fb::MapSectionArgs {
            tiles: Some(tiles),
            terrainOverlay: Some(terrain_overlay),
            elevationOverlay: Some(elevation_overlay),
            moistureRaster: Some(moisture_raster),
            removedTiles: None,
            climateBands: Some(climate_bands),
        },
    )
}

fn serialize_economy_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::EconomySection<'a>> {
    let logistics = create_logistics(builder, &snapshot.logistics);
    let trade_links = create_trade_links(builder, &snapshot.trade_links);
    let logistics_raster = create_scalar_raster(builder, &snapshot.logistics_raster);
    let faction_inventory = create_faction_inventory(builder, &snapshot.faction_inventory);
    fb::EconomySection::create(
        builder,
        &fb::EconomySectionArgs {
            logistics: Some(logistics),
            tradeLinks: Some(trade_links),
            logisticsRaster: Some(logistics_raster),
            factionInventory: Some(faction_inventory),
            removedLogistics: None,
            removedTradeLinks: None,
        },
    )
}

fn serialize_population_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::PopulationSection<'a>> {
    let populations = create_populations(builder, &snapshot.populations);
    let demographics = create_demographics(builder, &snapshot.demographics);
    let generations = create_generations(builder, &snapshot.generations);
    fb::PopulationSection::create(
        builder,
        &fb::PopulationSectionArgs {
            populations: Some(populations),
            demographics: Some(demographics),
            generations: Some(generations),
            removedPopulations: None,
            removedGenerations: None,
        },
    )
}

fn serialize_subsistence_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::SubsistenceSection<'a>> {
    let herds = create_herds(builder, &snapshot.herds);
    let forage_patches = create_forage_patches(builder, &snapshot.forage_patches);
    let sedentarization = create_sedentarization(builder, &snapshot.sedentarization);
    let intensification_knowledge =
        create_intensification_knowledge(builder, &snapshot.intensification_knowledge);
    let food_modules = create_food_modules(builder, &snapshot.food_modules);
    fb::SubsistenceSection::create(
        builder,
        &fb::SubsistenceSectionArgs {
            herds: Some(herds),
            foragePatches: Some(forage_patches),
            sedentarization: Some(sedentarization),
            intensificationKnowledge: Some(intensification_knowledge),
            foodModules: Some(food_modules),
        },
    )
}

fn serialize_knowledge_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::KnowledgeSection<'a>> {
    let great_discovery_definitions =
        create_great_discovery_definitions(builder, &snapshot.great_discovery_definitions);
    let great_discoveries = create_great_discoveries(builder, &snapshot.great_discoveries);
    let great_discovery_progress =
        create_great_discovery_progress(builder, &snapshot.great_discovery_progress);
    let great_discovery_telemetry =
        create_great_discovery_telemetry(builder, &snapshot.great_discovery_telemetry);
    let knowledge_ledger = create_knowledge_ledger(builder, &snapshot.knowledge_ledger);
    let knowledge_timeline = create_knowledge_timeline(builder, &snapshot.knowledge_timeline);
    let knowledge_metrics = create_knowledge_metrics(builder, &snapshot.knowledge_metrics);
    let discovered_sites = create_discovered_sites(builder, &snapshot.discovered_sites);
    let discovery_progress = create_discovery_progress(builder, &snapshot.discovery_progress);
    fb::KnowledgeSection::create(
        builder,
        &fb::KnowledgeSectionArgs {
            greatDiscoveryDefinitions: Some(great_discovery_definitions),
            greatDiscoveries: Some(great_discoveries),
            greatDiscoveryProgress: Some(great_discovery_progress),
            greatDiscoveryTelemetry: Some(great_discovery_telemetry),
            knowledgeLedger: Some(knowledge_ledger),
            knowledgeTimeline: Some(knowledge_timeline),
            knowledgeMetrics: Some(knowledge_metrics),
            discoveredSites: Some(discovered_sites),
            discoveryProgress: Some(discovery_progress),
            removedKnowledgeLedger: None,
        },
    )
}

fn serialize_governance_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::GovernanceSection<'a>> {
    let power = create_power(builder, &snapshot.power);
    let power_metrics = create_power_metrics(builder, &snapshot.power_metrics);
    let corruption = create_corruption(builder, &snapshot.corruption);
    let corruption_raster = create_scalar_raster(builder, &snapshot.corruption_raster);
    let crisis_telemetry = create_crisis_telemetry(builder, &snapshot.crisis_telemetry);
    let crisis_overlay = create_crisis_overlay(builder, &snapshot.crisis_overlay);
    fb::GovernanceSection::create(
        builder,
        &fb::GovernanceSectionArgs {
            power: Some(power),
            powerMetrics: Some(power_metrics),
            corruption: Some(corruption),
            corruptionRaster: Some(corruption_raster),
            crisisTelemetry: Some(crisis_telemetry),
            crisisOverlay: Some(crisis_overlay),
            removedPower: None,
        },
    )
}

fn serialize_culture_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::CultureSection<'a>> {
    let culture_layers = create_culture_layers(builder, &snapshot.culture_layers);
    let culture_tensions = create_culture_tensions(builder, &snapshot.culture_tensions);
    let culture_raster = create_scalar_raster(builder, &snapshot.culture_raster);
    let influencers = create_influencers(builder, &snapshot.influencers);
    let axis_bias = fb::AxisBiasState::create(
        builder,
        &fb::AxisBiasStateArgs {
            knowledge: snapshot.axis_bias.knowledge,
            trust: snapshot.axis_bias.trust,
            equity: snapshot.axis_bias.equity,
            agency: snapshot.axis_bias.agency,
        },
    );
    let sentiment = create_sentiment(builder, &snapshot.sentiment);
    let sentiment_raster = create_scalar_raster(builder, &snapshot.sentiment_raster);
    fb::CultureSection::create(
        builder,
        &fb::CultureSectionArgs {
            cultureLayers: Some(culture_layers),
            cultureTensions: Some(culture_tensions),
            cultureRaster: Some(culture_raster),
            influencers: Some(influencers),
            axisBias: Some(axis_bias),
            sentiment: Some(sentiment),
            sentimentRaster: Some(sentiment_raster),
            removedInfluencers: None,
            removedCultureLayers: None,
        },
    )
}

fn serialize_vision_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::VisionSection<'a>> {
    let fog_raster = create_scalar_raster(builder, &snapshot.fog_raster);
    let visibility_raster = create_scalar_raster(builder, &snapshot.visibility_raster);
    let military_raster = create_scalar_raster(builder, &snapshot.military_raster);
    fb::VisionSection::create(
        builder,
        &fb::VisionSectionArgs {
            fogRaster: Some(fog_raster),
            visibilityRaster: Some(visibility_raster),
            militaryRaster: Some(military_raster),
        },
    )
}

fn serialize_campaign_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
    victory_state: WIPOffset<fb::VictoryState<'a>>,
) -> WIPOffset<fb::CampaignSection<'a>> {
    let campaign_profiles = create_campaign_profiles(builder, &snapshot.campaign_profiles);
    let command_events = create_command_events(builder, &snapshot.command_events);
    let pending_forks = create_pending_forks(builder, &snapshot.pending_forks);
    let stance_axes = create_stance_axes(builder, &snapshot.stance_axes);
    let voice_medium = create_voice_medium(builder, &snapshot.voice_medium);
    fb::CampaignSection::create(
        builder,
        &fb::CampaignSectionArgs {
            campaignProfiles: Some(campaign_profiles),
            commandEvents: Some(command_events),
            victory: Some(victory_state),
            pendingForks: Some(pending_forks),
            stanceAxes: Some(stance_axes),
            voiceMedium: Some(voice_medium),
        },
    )
}

fn serialize_map_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::MapSection<'a>> {
    let tiles = create_tiles(builder, &delta.tiles);
    let removed_tiles = builder.create_vector(&delta.removed_tiles);
    let terrain_overlay = delta
        .terrain
        .as_ref()
        .map(|overlay| create_terrain_overlay(builder, overlay));
    let elevation_overlay = delta
        .elevation_overlay
        .as_ref()
        .map(|overlay| create_elevation_overlay(builder, overlay));
    let moisture_raster = delta
        .moisture_raster
        .as_ref()
        .map(|raster| create_float_raster(builder, raster));
    let climate_bands = delta
        .climate_bands
        .as_ref()
        .map(|bands| create_climate_bands(builder, bands));
    fb::MapSection::create(
        builder,
        &fb::MapSectionArgs {
            tiles: Some(tiles),
            terrainOverlay: terrain_overlay,
            elevationOverlay: elevation_overlay,
            moistureRaster: moisture_raster,
            removedTiles: Some(removed_tiles),
            climateBands: climate_bands,
        },
    )
}

fn serialize_economy_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::EconomySection<'a>> {
    let logistics = create_logistics(builder, &delta.logistics);
    let removed_logistics = builder.create_vector(&delta.removed_logistics);
    let trade_links = create_trade_links(builder, &delta.trade_links);
    let removed_trade_links = builder.create_vector(&delta.removed_trade_links);
    let logistics_raster = delta
        .logistics_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let faction_inventory = delta
        .faction_inventory
        .as_ref()
        .map(|entries| create_faction_inventory(builder, entries));
    fb::EconomySection::create(
        builder,
        &fb::EconomySectionArgs {
            logistics: Some(logistics),
            tradeLinks: Some(trade_links),
            logisticsRaster: logistics_raster,
            factionInventory: faction_inventory,
            removedLogistics: Some(removed_logistics),
            removedTradeLinks: Some(removed_trade_links),
        },
    )
}

fn serialize_population_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::PopulationSection<'a>> {
    let populations = create_populations(builder, &delta.populations);
    let removed_populations = builder.create_vector(&delta.removed_populations);
    let demographics = delta
        .demographics
        .as_ref()
        .map(|entries| create_demographics(builder, entries));
    let generations = create_generations(builder, &delta.generations);
    let removed_generations = builder.create_vector(&delta.removed_generations);
    fb::PopulationSection::create(
        builder,
        &fb::PopulationSectionArgs {
            populations: Some(populations),
            demographics,
            generations: Some(generations),
            removedPopulations: Some(removed_populations),
            removedGenerations: Some(removed_generations),
        },
    )
}

fn serialize_subsistence_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::SubsistenceSection<'a>> {
    let herds = delta
        .herds
        .as_ref()
        .map(|entries| create_herds(builder, entries));
    let forage_patches = delta
        .forage_patches
        .as_ref()
        .map(|entries| create_forage_patches(builder, entries));
    let sedentarization = delta
        .sedentarization
        .as_ref()
        .map(|entries| create_sedentarization(builder, entries));
    let intensification_knowledge = delta
        .intensification_knowledge
        .as_ref()
        .map(|entries| create_intensification_knowledge(builder, entries));
    let food_modules = delta
        .food_modules
        .as_ref()
        .map(|entries| create_food_modules(builder, entries));
    fb::SubsistenceSection::create(
        builder,
        &fb::SubsistenceSectionArgs {
            herds,
            foragePatches: forage_patches,
            sedentarization,
            intensificationKnowledge: intensification_knowledge,
            foodModules: food_modules,
        },
    )
}

fn serialize_knowledge_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::KnowledgeSection<'a>> {
    let great_discovery_definitions = delta
        .great_discovery_definitions
        .as_ref()
        .map(|definitions| create_great_discovery_definitions(builder, definitions));
    let great_discoveries = create_great_discoveries(builder, &delta.great_discoveries);
    let great_discovery_progress =
        create_great_discovery_progress(builder, &delta.great_discovery_progress);
    let great_discovery_telemetry = delta
        .great_discovery_telemetry
        .as_ref()
        .map(|telemetry| create_great_discovery_telemetry(builder, telemetry));
    let knowledge_ledger = create_knowledge_ledger(builder, &delta.knowledge_ledger);
    let removed_knowledge_ledger = builder.create_vector(&delta.removed_knowledge_ledger);
    let knowledge_timeline = create_knowledge_timeline(builder, &delta.knowledge_timeline);
    let knowledge_metrics = delta
        .knowledge_metrics
        .as_ref()
        .map(|metrics| create_knowledge_metrics(builder, metrics));
    let discovered_sites = delta
        .discovered_sites
        .as_ref()
        .map(|entries| create_discovered_sites(builder, entries));
    let discovery_progress = create_discovery_progress(builder, &delta.discovery_progress);
    fb::KnowledgeSection::create(
        builder,
        &fb::KnowledgeSectionArgs {
            greatDiscoveryDefinitions: great_discovery_definitions,
            greatDiscoveries: Some(great_discoveries),
            greatDiscoveryProgress: Some(great_discovery_progress),
            greatDiscoveryTelemetry: great_discovery_telemetry,
            knowledgeLedger: Some(knowledge_ledger),
            knowledgeTimeline: Some(knowledge_timeline),
            knowledgeMetrics: knowledge_metrics,
            discoveredSites: discovered_sites,
            discoveryProgress: Some(discovery_progress),
            removedKnowledgeLedger: Some(removed_knowledge_ledger),
        },
    )
}

fn serialize_governance_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::GovernanceSection<'a>> {
    let power = create_power(builder, &delta.power);
    let removed_power = builder.create_vector(&delta.removed_power);
    let power_metrics = delta
        .power_metrics
        .as_ref()
        .map(|metrics| create_power_metrics(builder, metrics));
    let corruption = delta
        .corruption
        .as_ref()
        .map(|c| create_corruption(builder, c));
    let corruption_raster = delta
        .corruption_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let crisis_telemetry = delta
        .crisis_telemetry
        .as_ref()
        .map(|telemetry| create_crisis_telemetry(builder, telemetry));
    let crisis_overlay = delta
        .crisis_overlay
        .as_ref()
        .map(|overlay| create_crisis_overlay(builder, overlay));
    fb::GovernanceSection::create(
        builder,
        &fb::GovernanceSectionArgs {
            power: Some(power),
            powerMetrics: power_metrics,
            corruption,
            corruptionRaster: corruption_raster,
            crisisTelemetry: crisis_telemetry,
            crisisOverlay: crisis_overlay,
            removedPower: Some(removed_power),
        },
    )
}

fn serialize_culture_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::CultureSection<'a>> {
    let culture_layers = create_culture_layers(builder, &delta.culture_layers);
    let removed_culture_layers = builder.create_vector(&delta.removed_culture_layers);
    let culture_tensions = create_culture_tensions(builder, &delta.culture_tensions);
    let culture_raster = delta
        .culture_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let influencers = create_influencers(builder, &delta.influencers);
    let removed_influencers = builder.create_vector(&delta.removed_influencers);
    let axis_bias = delta.axis_bias.as_ref().map(|axis| {
        fb::AxisBiasState::create(
            builder,
            &fb::AxisBiasStateArgs {
                knowledge: axis.knowledge,
                trust: axis.trust,
                equity: axis.equity,
                agency: axis.agency,
            },
        )
    });
    let sentiment = delta
        .sentiment
        .as_ref()
        .map(|s| create_sentiment(builder, s));
    let sentiment_raster = delta
        .sentiment_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    fb::CultureSection::create(
        builder,
        &fb::CultureSectionArgs {
            cultureLayers: Some(culture_layers),
            cultureTensions: Some(culture_tensions),
            cultureRaster: culture_raster,
            influencers: Some(influencers),
            axisBias: axis_bias,
            sentiment,
            sentimentRaster: sentiment_raster,
            removedInfluencers: Some(removed_influencers),
            removedCultureLayers: Some(removed_culture_layers),
        },
    )
}

fn serialize_vision_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::VisionSection<'a>> {
    let fog_raster = delta
        .fog_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let visibility_raster = delta
        .visibility_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let military_raster = delta
        .military_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    fb::VisionSection::create(
        builder,
        &fb::VisionSectionArgs {
            fogRaster: fog_raster,
            visibilityRaster: visibility_raster,
            militaryRaster: military_raster,
        },
    )
}

fn serialize_campaign_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
    victory_state: Option<WIPOffset<fb::VictoryState<'a>>>,
) -> WIPOffset<fb::CampaignSection<'a>> {
    let command_events = delta
        .command_events
        .as_ref()
        .map(|entries| create_command_events(builder, entries));
    let pending_forks = delta
        .pending_forks
        .as_ref()
        .map(|entries| create_pending_forks(builder, entries));
    let stance_axes = delta
        .stance_axes
        .as_ref()
        .map(|entries| create_stance_axes(builder, entries));
    let voice_medium = delta
        .voice_medium
        .as_ref()
        .map(|entries| create_voice_medium(builder, entries));
    fb::CampaignSection::create(
        builder,
        &fb::CampaignSectionArgs {
            campaignProfiles: None,
            commandEvents: command_events,
            victory: victory_state,
            pendingForks: pending_forks,
            stanceAxes: stance_axes,
            voiceMedium: voice_medium,
        },
    )
}

fn create_elevation_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &ElevationOverlayState,
) -> WIPOffset<fb::ElevationOverlay<'a>> {
    let samples_vec = builder.create_vector(&overlay.samples);
    fb::ElevationOverlay::create(
        builder,
        &fb::ElevationOverlayArgs {
            width: overlay.width,
            height: overlay.height,
            minValue: overlay.min_value,
            maxValue: overlay.max_value,
            samples: Some(samples_vec),
            seaLevel: overlay.sea_level,
        },
    )
}

fn create_climate_bands<'a>(
    builder: &mut FbBuilder<'a>,
    bands: &ClimateBandsState,
) -> WIPOffset<fb::ClimateBands<'a>> {
    fb::ClimateBands::create(
        builder,
        &fb::ClimateBandsArgs {
            polarMaxTemp: bands.polar_max_temp,
            borealMaxTemp: bands.boreal_max_temp,
            temperateMaxTemp: bands.temperate_max_temp,
        },
    )
}

fn create_campaign_label<'a>(
    builder: &mut FbBuilder<'a>,
    label: &CampaignLabel,
) -> Option<WIPOffset<fb::CampaignLabel<'a>>> {
    let has_any = label.title.is_some()
        || label.title_loc_key.is_some()
        || label.subtitle.is_some()
        || label.subtitle_loc_key.is_some()
        || label.profile_id.is_some();
    if !has_any {
        return None;
    }

    let profile_id = label
        .profile_id
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let title = label
        .title
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let title_loc_key = label
        .title_loc_key
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let subtitle = label
        .subtitle
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let subtitle_loc_key = label
        .subtitle_loc_key
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));

    Some(fb::CampaignLabel::create(
        builder,
        &fb::CampaignLabelArgs {
            profileId: profile_id,
            title,
            titleLocKey: title_loc_key,
            subtitle,
            subtitleLocKey: subtitle_loc_key,
        },
    ))
}

fn create_campaign_profiles<'a>(
    builder: &mut FbBuilder<'a>,
    profiles: &[CampaignProfileState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CampaignProfile<'a>>>> {
    let mut entries = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let id = profile
            .id
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let title = profile
            .title
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let title_loc_key = profile
            .title_loc_key
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let subtitle = profile
            .subtitle
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let subtitle_loc_key = profile
            .subtitle_loc_key
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let starting_units = if profile.starting_units.is_empty() {
            None
        } else {
            let mut offsets = Vec::with_capacity(profile.starting_units.len());
            for unit in &profile.starting_units {
                let kind = builder.create_string(unit.kind.as_str());
                let tags = if unit.tags.is_empty() {
                    None
                } else {
                    let tag_offsets: Vec<_> = unit
                        .tags
                        .iter()
                        .map(|tag| builder.create_string(tag.as_str()))
                        .collect();
                    Some(builder.create_vector(&tag_offsets))
                };
                let unit_entry = fb::CampaignStartingUnit::create(
                    builder,
                    &fb::CampaignStartingUnitArgs {
                        kind: Some(kind),
                        count: unit.count,
                        tags,
                    },
                );
                offsets.push(unit_entry);
            }
            Some(builder.create_vector(&offsets))
        };
        let inventory = if profile.inventory.is_empty() {
            None
        } else {
            let mut offsets = Vec::with_capacity(profile.inventory.len());
            for entry in &profile.inventory {
                let item = builder.create_string(entry.item.as_str());
                let inv_entry = fb::CampaignInventoryEntry::create(
                    builder,
                    &fb::CampaignInventoryEntryArgs {
                        item: Some(item),
                        quantity: entry.quantity,
                    },
                );
                offsets.push(inv_entry);
            }
            Some(builder.create_vector(&offsets))
        };
        let knowledge_tags = if profile.knowledge_tags.is_empty() {
            None
        } else {
            let offsets: Vec<_> = profile
                .knowledge_tags
                .iter()
                .map(|tag| builder.create_string(tag.as_str()))
                .collect();
            Some(builder.create_vector(&offsets))
        };
        let fog_mode = profile
            .fog_mode
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let primary_food_module = profile
            .primary_food_module
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let secondary_food_module = profile
            .secondary_food_module
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let survey_radius = profile.survey_radius.unwrap_or(0);
        let entry = fb::CampaignProfile::create(
            builder,
            &fb::CampaignProfileArgs {
                id,
                title,
                titleLocKey: title_loc_key,
                subtitle,
                subtitleLocKey: subtitle_loc_key,
                startingUnits: starting_units,
                inventory,
                knowledgeTags: knowledge_tags,
                surveyRadius: survey_radius,
                fogMode: fog_mode,
                primaryFoodModule: primary_food_module,
                secondaryFoodModule: secondary_food_module,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_faction_inventory<'a>(
    builder: &mut FbBuilder<'a>,
    factions: &[FactionInventoryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::FactionInventoryState<'a>>>> {
    let mut entries = Vec::with_capacity(factions.len());
    for state in factions {
        let mut inventory_offsets = Vec::with_capacity(state.inventory.len());
        for entry in &state.inventory {
            let item = builder.create_string(entry.item.as_str());
            let entry_offset = fb::FactionInventoryEntry::create(
                builder,
                &fb::FactionInventoryEntryArgs {
                    item: Some(item),
                    quantity: entry.quantity,
                },
            );
            inventory_offsets.push(entry_offset);
        }
        let inventory_vec = builder.create_vector(&inventory_offsets);
        let faction_entry = fb::FactionInventoryState::create(
            builder,
            &fb::FactionInventoryStateArgs {
                faction: state.faction,
                inventory: Some(inventory_vec),
            },
        );
        entries.push(faction_entry);
    }
    builder.create_vector(&entries)
}

fn create_voice_lines<'a>(
    builder: &mut FbBuilder<'a>,
    lines: &[VoiceLineState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::VoiceLine<'a>>>> {
    let mut entries = Vec::with_capacity(lines.len());
    for line in lines {
        let register = builder.create_string(line.register.as_str());
        let text = builder.create_string(line.text.as_str());
        entries.push(fb::VoiceLine::create(
            builder,
            &fb::VoiceLineArgs {
                register: Some(register),
                text: Some(text),
            },
        ));
    }
    builder.create_vector(&entries)
}

fn create_pending_forks<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[PendingForksState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PendingForksState<'a>>>> {
    let mut faction_entries = Vec::with_capacity(states.len());
    for state in states {
        let mut fork_entries = Vec::with_capacity(state.forks.len());
        for fork in &state.forks {
            let beat_id = builder.create_string(fork.beat_id.as_str());
            let wardrobe_id = builder.create_string(fork.wardrobe_id.as_str());
            let narration = create_voice_lines(builder, &fork.narration);
            let mut choice_entries = Vec::with_capacity(fork.choices.len());
            for choice in &fork.choices {
                let choice_id = builder.create_string(choice.choice_id.as_str());
                let label = create_voice_lines(builder, &choice.label);
                choice_entries.push(fb::ForkChoiceState::create(
                    builder,
                    &fb::ForkChoiceStateArgs {
                        choiceId: Some(choice_id),
                        label: Some(label),
                        isDefer: choice.is_defer,
                    },
                ));
            }
            let choices = builder.create_vector(&choice_entries);
            let mut gloss_entries = Vec::with_capacity(fork.gloss.len());
            for entry in &fork.gloss {
                let signal = builder.create_string(entry.signal.as_str());
                gloss_entries.push(fb::GlossEntry::create(
                    builder,
                    &fb::GlossEntryArgs {
                        signal: Some(signal),
                        value: entry.value,
                    },
                ));
            }
            let gloss = builder.create_vector(&gloss_entries);
            fork_entries.push(fb::PendingForkState::create(
                builder,
                &fb::PendingForkStateArgs {
                    beatId: Some(beat_id),
                    wardrobeId: Some(wardrobe_id),
                    postedTick: fork.posted_tick,
                    narration: Some(narration),
                    choices: Some(choices),
                    gloss: Some(gloss),
                },
            ));
        }
        let forks = builder.create_vector(&fork_entries);
        faction_entries.push(fb::PendingForksState::create(
            builder,
            &fb::PendingForksStateArgs {
                faction: state.faction,
                forks: Some(forks),
            },
        ));
    }
    builder.create_vector(&faction_entries)
}

fn create_stance_axes<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[StanceState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::StanceState<'a>>>> {
    let mut faction_entries = Vec::with_capacity(states.len());
    for state in states {
        let mut axis_entries = Vec::with_capacity(state.axes.len());
        for axis in &state.axes {
            let name = builder.create_string(axis.axis.as_str());
            axis_entries.push(fb::StanceAxisState::create(
                builder,
                &fb::StanceAxisStateArgs {
                    axis: Some(name),
                    value: axis.value,
                },
            ));
        }
        let axes = builder.create_vector(&axis_entries);
        faction_entries.push(fb::StanceState::create(
            builder,
            &fb::StanceStateArgs {
                faction: state.faction,
                axes: Some(axes),
            },
        ));
    }
    builder.create_vector(&faction_entries)
}

/// The narrator's medium, per faction. `mediumId` rides as a string so a new rung on the ladder
/// needs no schema change.
fn create_voice_medium<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[VoiceMediumState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::VoiceMediumState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let medium_id = builder.create_string(state.medium_id.as_str());
        entries.push(fb::VoiceMediumState::create(
            builder,
            &fb::VoiceMediumStateArgs {
                faction: state.faction,
                mediumId: Some(medium_id),
                mediumIndex: state.medium_index,
            },
        ));
    }
    builder.create_vector(&entries)
}

fn create_sedentarization<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[SedentarizationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::SedentarizationState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let stage = builder.create_string(state.stage.as_str());
        let entry = fb::SedentarizationState::create(
            builder,
            &fb::SedentarizationStateArgs {
                faction: state.faction,
                score: state.score,
                stage: Some(stage),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_discovered_sites<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[DiscoveredSitesState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::DiscoveredSitesState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let mut site_offsets = Vec::with_capacity(state.sites.len());
        for site in &state.sites {
            let site_id = builder.create_string(site.site_id.as_str());
            let category = builder.create_string(site.category.as_str());
            let display_name = builder.create_string(site.display_name.as_str());
            let glyph = builder.create_string(site.glyph.as_str());
            let site_offset = fb::DiscoveredSite::create(
                builder,
                &fb::DiscoveredSiteArgs {
                    x: site.x,
                    y: site.y,
                    site_id: Some(site_id),
                    display_name: Some(display_name),
                    category: Some(category),
                    glyph: Some(glyph),
                },
            );
            site_offsets.push(site_offset);
        }
        let sites_vec = builder.create_vector(&site_offsets);
        let entry = fb::DiscoveredSitesState::create(
            builder,
            &fb::DiscoveredSitesStateArgs {
                faction: state.faction,
                sites: Some(sites_vec),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_demographics<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[PopulationDemographicsState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PopulationDemographicsState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let entry = fb::PopulationDemographicsState::create(
            builder,
            &fb::PopulationDemographicsStateArgs {
                faction: state.faction,
                children: state.children,
                working: state.working,
                elders: state.elders,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_herds<'a>(
    builder: &mut FbBuilder<'a>,
    herds: &[HerdTelemetryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::HerdTelemetryState<'a>>>> {
    let mut entries = Vec::with_capacity(herds.len());
    for herd in herds {
        let id = builder.create_string(herd.id.as_str());
        let label = builder.create_string(herd.label.as_str());
        let species = builder.create_string(herd.species.as_str());
        let size_class = builder.create_string(herd.size_class.as_str());
        let ecology_phase = builder.create_string(herd.ecology_phase.as_str());
        let husbandry_ceiling = builder.create_string(herd.husbandry_ceiling.as_str());
        let hunt_policy_ceilings = if herd.hunt_policy_ceilings.is_empty() {
            None
        } else {
            let entries: Vec<_> = herd
                .hunt_policy_ceilings
                .iter()
                .map(|ceiling| {
                    let policy = builder.create_string(ceiling.policy.as_str());
                    fb::HuntPolicyCeiling::create(
                        builder,
                        &fb::HuntPolicyCeilingArgs {
                            policy: Some(policy),
                            provisionsPerTurn: ceiling.provisions_per_turn,
                        },
                    )
                })
                .collect();
            Some(builder.create_vector(&entries))
        };
        let hunt_trip_estimates = if herd.hunt_trip_estimates.is_empty() {
            None
        } else {
            let entries: Vec<_> = herd
                .hunt_trip_estimates
                .iter()
                .map(|estimate| {
                    let policy = builder.create_string(estimate.policy.as_str());
                    fb::HuntTripEstimate::create(
                        builder,
                        &fb::HuntTripEstimateArgs {
                            policy: Some(policy),
                            partyWorkers: estimate.party_workers,
                            turnsToFill: estimate.turns_to_fill,
                            deliversFood: estimate.delivers_food,
                            animalsTaken: estimate.animals_taken,
                            deliveredFood: estimate.delivered_food,
                            wastedFood: estimate.wasted_food,
                        },
                    )
                })
                .collect();
            Some(builder.create_vector(&entries))
        };
        let entry = fb::HerdTelemetryState::create(
            builder,
            &fb::HerdTelemetryStateArgs {
                id: Some(id),
                label: Some(label),
                species: Some(species),
                x: herd.x,
                y: herd.y,
                biomass: herd.biomass,
                routeLength: herd.route_length,
                nextX: herd.next_x,
                nextY: herd.next_y,
                sizeClass: Some(size_class),
                huntable: herd.huntable,
                ecologyPhase: Some(ecology_phase),
                domestication: herd.domestication,
                corralled: herd.corralled,
                corralProgress: herd.corral_progress,
                perWorkerYield: herd.per_worker_yield,
                ceilingSustain: herd.ceiling_sustain,
                ceilingSurplus: herd.ceiling_surplus,
                ceilingMarket: herd.ceiling_market,
                ceilingEradicate: herd.ceiling_eradicate,
                ceilingCorral: herd.ceiling_corral,
                corralYield: herd.corral_yield,
                penUpkeep: herd.pen_upkeep,
                penFedFraction: herd.pen_fed_fraction,
                // Appended after every earlier-shipped field (append-only wire discipline).
                huntPolicyCeilings: hunt_policy_ceilings,
                huntTripEstimates: hunt_trip_estimates,
                // Ecological K + grazing range (Grazing Phase 2b-iii) — appended last.
                carryingCapacity: herd.carrying_capacity,
                grazeRangeRadius: herd.graze_range_radius,
                // The pen economy (Grazing 2d) — appended last.
                penRadius: herd.pen_radius,
                penFootprintTiles: herd.pen_footprint_tiles,
                penPastureFraction: herd.pen_pasture_fraction,
                penExtendProgress: herd.pen_extend_progress,
                // Husbandry ceiling (Grazing 2d-δ) — appended last.
                husbandryCeiling: Some(husbandry_ceiling),
                // Body mass (slice 8b) — appended last (append-only wire).
                bodyMass: herd.body_mass,
                // Food per animal (slice 8b) — appended last (append-only wire).
                foodPerAnimal: herd.food_per_animal,
                // Herd staffing — appended last (append-only wire).
                herdersNeeded: herd.herders_needed,
                herdedFraction: herd.herded_fraction,
                // The Tame rung's payoff — appended last (append-only wire).
                pastoralYield: herd.pastoral_yield,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_forage_patches<'a>(
    builder: &mut FbBuilder<'a>,
    patches: &[ForagePatchState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::ForagePatchState<'a>>>> {
    let mut entries = Vec::with_capacity(patches.len());
    for patch in patches {
        let ecology_phase = builder.create_string(patch.ecology_phase.as_str());
        let sow_site_refusal = builder.create_string(patch.sow_site_refusal.as_str());
        let entry = fb::ForagePatchState::create(
            builder,
            &fb::ForagePatchStateArgs {
                x: patch.x,
                y: patch.y,
                cultivationProgress: patch.cultivation_progress,
                isCultivated: patch.is_cultivated,
                hasOwner: patch.owner.is_some(),
                owner: patch.owner.unwrap_or(0),
                biomass: patch.biomass,
                carryingCapacity: patch.carrying_capacity,
                ecologyPhase: Some(ecology_phase),
                perWorkerYield: patch.per_worker_yield,
                ceilingSustain: patch.ceiling_sustain,
                ceilingSurplus: patch.ceiling_surplus,
                ceilingMarket: patch.ceiling_market,
                ceilingEradicate: patch.ceiling_eradicate,
                ceilingCultivate: patch.ceiling_cultivate,
                tendedYield: patch.tended_yield,
                fieldProgress: patch.field_progress,
                isField: patch.is_field,
                ceilingSow: patch.ceiling_sow,
                fieldYield: patch.field_yield,
                sowSiteRefusal: Some(sow_site_refusal),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_intensification_knowledge<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[IntensificationKnowledgeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::IntensificationKnowledgeState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let entry = fb::IntensificationKnowledgeState::create(
            builder,
            &fb::IntensificationKnowledgeStateArgs {
                faction: state.faction,
                cultivation: state.cultivation,
                herding: state.herding,
                seedSelection: state.seed_selection,
                penning: state.penning,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_food_modules<'a>(
    builder: &mut FbBuilder<'a>,
    modules: &[FoodModuleState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::FoodModuleState<'a>>>> {
    let mut entries = Vec::with_capacity(modules.len());
    for module in modules {
        let module_label = builder.create_string(module.module.as_str());
        let kind_label = builder.create_string(module.kind.as_str());
        let entry = fb::FoodModuleState::create(
            builder,
            &fb::FoodModuleStateArgs {
                x: module.x,
                y: module.y,
                module: Some(module_label),
                seasonalWeight: module.seasonal_weight,
                kind: Some(kind_label),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_command_events<'a>(
    builder: &mut FbBuilder<'a>,
    events: &[CommandEventState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CommandEventState<'a>>>> {
    let mut entries = Vec::with_capacity(events.len());
    for event in events {
        let kind = builder.create_string(event.kind.as_str());
        let label = builder.create_string(event.label.as_str());
        let detail = event
            .detail
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let entry = fb::CommandEventState::create(
            builder,
            &fb::CommandEventStateArgs {
                tick: event.tick,
                kind: Some(kind),
                faction: event.faction,
                label: Some(label),
                detail,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_victory_state<'a>(
    builder: &mut FbBuilder<'a>,
    state: &VictorySnapshotState,
) -> WIPOffset<fb::VictoryState<'a>> {
    let mut mode_entries = Vec::with_capacity(state.modes.len());
    for mode in &state.modes {
        let id = builder.create_string(mode.id.as_str());
        let kind = builder.create_string(mode.kind.as_str());
        let entry = fb::VictoryModeState::create(
            builder,
            &fb::VictoryModeStateArgs {
                id: Some(id),
                kind: Some(kind),
                progress: mode.progress,
                threshold: mode.threshold,
                achieved: mode.achieved,
            },
        );
        mode_entries.push(entry);
    }
    let modes_vec = builder.create_vector(&mode_entries);

    let winner = state.winner.as_ref().map(|winner| {
        let mode = builder.create_string(winner.mode.as_str());
        fb::VictoryResult::create(
            builder,
            &fb::VictoryResultArgs {
                mode: Some(mode),
                faction: winner.faction,
                tick: winner.tick,
            },
        )
    });

    fb::VictoryState::create(
        builder,
        &fb::VictoryStateArgs {
            modes: Some(modes_vec),
            winner,
        },
    )
}

fn create_tiles<'a>(
    builder: &mut FbBuilder<'a>,
    tiles: &[TileState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::TileState<'a>>>> {
    let offsets: Vec<_> = tiles
        .iter()
        .map(|tile| {
            fb::TileState::create(
                builder,
                &fb::TileStateArgs {
                    entity: tile.entity,
                    x: tile.x,
                    y: tile.y,
                    element: tile.element,
                    mass: tile.mass,
                    temperature: tile.temperature,
                    terrain: to_fb_terrain_type(tile.terrain),
                    terrainTags: tile.terrain_tags.bits(),
                    cultureLayer: tile.culture_layer,
                    mountainKind: to_fb_mountain_kind(tile.mountain_kind),
                    mountainRelief: tile.mountain_relief,
                    habitability: tile.habitability,
                    grazeBiomass: tile.graze_biomass,
                    grazeCapacity: tile.graze_capacity,
                    grazeEcologyPhase: tile.graze_ecology_phase,
                    forageCapacity: tile.forage_capacity,
                    underlyingTerrain: to_fb_terrain_type(tile.underlying_terrain),
                    riverEdges: tile.river_edges,
                    riverInflow: tile.river_inflow,
                    riverChannel: tile.river_channel,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_logistics<'a>(
    builder: &mut FbBuilder<'a>,
    links: &[LogisticsLinkState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::LogisticsLinkState<'a>>>> {
    let offsets: Vec<_> = links
        .iter()
        .map(|link| {
            fb::LogisticsLinkState::create(
                builder,
                &fb::LogisticsLinkStateArgs {
                    entity: link.entity,
                    from: link.from,
                    to: link.to,
                    capacity: link.capacity,
                    flow: link.flow,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_trade_links<'a>(
    builder: &mut FbBuilder<'a>,
    links: &[TradeLinkState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::TradeLinkState<'a>>>> {
    let offsets: Vec<_> = links
        .iter()
        .map(|link| {
            let knowledge = fb::TradeLinkKnowledge::create(
                builder,
                &fb::TradeLinkKnowledgeArgs {
                    openness: link.knowledge.openness,
                    leakTimer: link.knowledge.leak_timer,
                    lastDiscovery: link.knowledge.last_discovery,
                    decay: link.knowledge.decay,
                },
            );
            let pending_fragments = if link.pending_fragments.is_empty() {
                None
            } else {
                Some(create_known_fragments(builder, &link.pending_fragments))
            };
            fb::TradeLinkState::create(
                builder,
                &fb::TradeLinkStateArgs {
                    entity: link.entity,
                    fromFaction: link.from_faction,
                    toFaction: link.to_faction,
                    throughput: link.throughput,
                    tariff: link.tariff,
                    knowledge: Some(knowledge),
                    fromTile: link.from_tile,
                    toTile: link.to_tile,
                    pendingFragments: pending_fragments,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_populations<'a>(
    builder: &mut FbBuilder<'a>,
    cohorts: &[PopulationCohortState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PopulationCohortState<'a>>>> {
    let offsets: Vec<_> = cohorts
        .iter()
        .map(|cohort| {
            let knowledge = if cohort.knowledge_fragments.is_empty() {
                None
            } else {
                Some(create_known_fragments(builder, &cohort.knowledge_fragments))
            };
            let stores = if cohort.stores.is_empty() {
                None
            } else {
                let entries: Vec<_> = cohort
                    .stores
                    .iter()
                    .map(|entry| {
                        let item = builder.create_string(&entry.item);
                        fb::CohortStore::create(
                            builder,
                            &fb::CohortStoreArgs {
                                item: Some(item),
                                quantity: entry.quantity,
                            },
                        )
                    })
                    .collect();
                Some(builder.create_vector(&entries))
            };
            let settlement_stage = {
                let stage = &cohort.settlement_stage;
                let id = builder.create_string(&stage.id);
                let label = builder.create_string(&stage.label);
                let icon = builder.create_string(&stage.icon);
                fb::SettlementStageView::create(
                    builder,
                    &fb::SettlementStageViewArgs {
                        id: Some(id),
                        label: Some(label),
                        icon: Some(icon),
                    },
                )
            };
            let migration = cohort.migration.as_ref().map(|pending| {
                let fragments = if pending.fragments.is_empty() {
                    None
                } else {
                    Some(create_known_fragments(builder, &pending.fragments))
                };
                fb::PendingMigration::create(
                    builder,
                    &fb::PendingMigrationArgs {
                        destination: pending.destination,
                        eta: pending.eta,
                        fragments,
                    },
                )
            });
            let harvest = cohort.harvest_task.as_ref().map(|task| {
                let module = builder.create_string(&task.module);
                let band_label = builder.create_string(&task.band_label);
                let kind = builder.create_string(&task.kind);
                fb::HarvestTask::create(
                    builder,
                    &fb::HarvestTaskArgs {
                        kind: Some(kind),
                        module: Some(module),
                        bandLabel: Some(band_label),
                        targetTile: task.target_tile,
                        targetX: task.target_x,
                        targetY: task.target_y,
                        travelRemaining: task.travel_remaining,
                        travelTotal: task.travel_total,
                        gatherRemaining: task.gather_remaining,
                        gatherTotal: task.gather_total,
                        provisionsReward: task.provisions_reward,
                        tradeGoodsReward: task.trade_goods_reward,
                        startedTick: task.started_tick,
                    },
                )
            });
            let scout = cohort.scout_task.as_ref().map(|task| {
                let band_label = builder.create_string(&task.band_label);
                fb::ScoutTask::create(
                    builder,
                    &fb::ScoutTaskArgs {
                        bandLabel: Some(band_label),
                        targetTile: task.target_tile,
                        targetX: task.target_x,
                        targetY: task.target_y,
                        travelRemaining: task.travel_remaining,
                        travelTotal: task.travel_total,
                        revealRadius: task.reveal_radius,
                        revealDuration: task.reveal_duration,
                        moraleGain: task.morale_gain,
                        startedTick: task.started_tick,
                    },
                )
            });
            let activity = Some(builder.create_string(&cohort.activity));
            let hunt_mode = if cohort.hunt_mode.is_empty() {
                None
            } else {
                Some(builder.create_string(&cohort.hunt_mode))
            };
            let labor_assignments = if cohort.labor_assignments.is_empty() {
                None
            } else {
                let entries: Vec<_> = cohort
                    .labor_assignments
                    .iter()
                    .map(|assignment| {
                        let kind = builder.create_string(&assignment.kind);
                        let fauna_id = if assignment.fauna_id.is_empty() {
                            None
                        } else {
                            Some(builder.create_string(&assignment.fauna_id))
                        };
                        let policy = if assignment.policy.is_empty() {
                            None
                        } else {
                            Some(builder.create_string(&assignment.policy))
                        };
                        // An unprojected row ships no vector at all, so the client can tell "no
                        // schedule" from "a schedule of zeros" (a real famine forecast).
                        let arrival_schedule = if assignment.arrival_schedule.is_empty() {
                            None
                        } else {
                            Some(builder.create_vector(&assignment.arrival_schedule))
                        };
                        fb::LaborAssignment::create(
                            builder,
                            &fb::LaborAssignmentArgs {
                                kind: Some(kind),
                                workers: assignment.workers,
                                targetX: assignment.target_x,
                                targetY: assignment.target_y,
                                faunaId: fauna_id,
                                policy,
                                actualYield: assignment.actual_yield,
                                sustainableYield: assignment.sustainable_yield,
                                workersNeeded: assignment.workers_needed,
                                wastedYield: assignment.wasted_yield,
                                overdraws: assignment.overdraws,
                                realizedYield: assignment.realized_yield,
                                arrivalSchedule: arrival_schedule,
                            },
                        )
                    })
                    .collect();
                Some(builder.create_vector(&entries))
            };
            let expedition_mission = if cohort.expedition_mission.is_empty() {
                None
            } else {
                Some(builder.create_string(&cohort.expedition_mission))
            };
            let expedition_phase = if cohort.expedition_phase.is_empty() {
                None
            } else {
                Some(builder.create_string(&cohort.expedition_phase))
            };
            let expedition_target_herd = if cohort.expedition_target_herd.is_empty() {
                None
            } else {
                Some(builder.create_string(&cohort.expedition_target_herd))
            };
            let expedition_hunt_policy = if cohort.expedition_hunt_policy.is_empty() {
                None
            } else {
                Some(builder.create_string(&cohort.expedition_hunt_policy))
            };
            let pending_reveal_x = if cohort.pending_reveal_x.is_empty() {
                None
            } else {
                Some(builder.create_vector(&cohort.pending_reveal_x))
            };
            let pending_reveal_y = if cohort.pending_reveal_y.is_empty() {
                None
            } else {
                Some(builder.create_vector(&cohort.pending_reveal_y))
            };
            let accessible_stockpile_fb = cohort.accessible_stockpile.as_ref().map(|stockpile| {
                let entries = if stockpile.entries.is_empty() {
                    None
                } else {
                    Some(create_accessible_stockpile_entries(
                        builder,
                        &stockpile.entries,
                    ))
                };
                fb::AccessibleStockpile::create(
                    builder,
                    &fb::AccessibleStockpileArgs {
                        radius: stockpile.radius,
                        entries,
                    },
                )
            });
            fb::PopulationCohortState::create(
                builder,
                &fb::PopulationCohortStateArgs {
                    entity: cohort.entity,
                    home: cohort.home,
                    currentX: cohort.current_x,
                    currentY: cohort.current_y,
                    isTraveling: cohort.is_traveling,
                    size: cohort.size,
                    morale: cohort.morale,
                    generation: cohort.generation,
                    faction: cohort.faction,
                    knowledgeFragments: knowledge,
                    migration,
                    harvestTask: harvest,
                    scoutTask: scout,
                    accessibleStockpile: accessible_stockpile_fb,
                    children: cohort.children,
                    working: cohort.working,
                    elders: cohort.elders,
                    stores,
                    ageTurns: cohort.age_turns,
                    daysOfFood: cohort.days_of_food,
                    activity,
                    huntMode: hunt_mode,
                    laborAssignments: labor_assignments,
                    idleWorkers: cohort.idle_workers,
                    workingAge: cohort.working_age,
                    workRange: cohort.work_range,
                    scoutRevealRadius: cohort.scout_reveal_radius,
                    isExpedition: cohort.is_expedition,
                    expeditionMission: expedition_mission,
                    expeditionPhase: expedition_phase,
                    homeBandEntity: cohort.home_band_entity,
                    expeditionAnnounced: cohort.expedition_announced,
                    pendingRevealX: pending_reveal_x,
                    pendingRevealY: pending_reveal_y,
                    maxExpeditionPartySize: cohort.max_expedition_party_size,
                    expeditionCarryCap: cohort.expedition_carry_cap,
                    // Appended after every earlier-shipped field (append-only wire discipline).
                    expeditionTargetHerd: expedition_target_herd,
                    expeditionHuntPolicy: expedition_hunt_policy,
                    travelTargetX: cohort.travel_target_x,
                    travelTargetY: cohort.travel_target_y,
                    huntReach: cohort.hunt_reach,
                    supplyNetworkId: cohort.supply_network_id,
                    moraleDelta: cohort.morale_delta,
                    moraleCause: cohort.morale_cause,
                    outputMultiplier: cohort.output_multiplier,
                    discontentFraction: cohort.discontent_fraction,
                    lastEmigrated: cohort.last_emigrated,
                    lastImmigrated: cohort.last_immigrated,
                    grievance: cohort.grievance,
                    moraleSettling: cohort.morale_settling,
                    moraleTerrain: cohort.morale_terrain,
                    moraleClimate: cohort.morale_climate,
                    moraleUnrest: cohort.morale_unrest,
                    settlementStage: Some(settlement_stage),
                    foodIncome: cohort.food_income,
                    penFeedUpkeep: cohort.pen_feed_upkeep,
                    foodConsumption: cohort.food_consumption,
                    huntPerWorkerProvisions: cohort.hunt_per_worker_provisions,
                    expeditionViabilityWarnTurns: cohort.expedition_viability_warn_turns,
                    expeditionPerWorkerCarry: cohort.expedition_per_worker_carry,
                    bandMoveTilesPerTurn: cohort.band_move_tiles_per_turn,
                    foodIncomeAverage: cohort.food_income_average,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_known_fragments<'a>(
    builder: &mut FbBuilder<'a>,
    fragments: &[KnownTechFragment],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnownTechFragment<'a>>>> {
    let offsets: Vec<_> = fragments
        .iter()
        .map(|fragment| {
            fb::KnownTechFragment::create(
                builder,
                &fb::KnownTechFragmentArgs {
                    discoveryId: fragment.discovery_id,
                    progress: fragment.progress,
                    fidelity: fragment.fidelity,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_accessible_stockpile_entries<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[AccessibleStockpileEntryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::AccessibleStockpileEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let item = builder.create_string(&entry.item);
            fb::AccessibleStockpileEntry::create(
                builder,
                &fb::AccessibleStockpileEntryArgs {
                    item: Some(item),
                    quantity: entry.quantity,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_discovery_progress<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[DiscoveryProgressEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::DiscoveryProgressEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::DiscoveryProgressEntry::create(
                builder,
                &fb::DiscoveryProgressEntryArgs {
                    faction: entry.faction,
                    discovery: entry.discovery,
                    progress: entry.progress,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power<'a>(
    builder: &mut FbBuilder<'a>,
    power_nodes: &[PowerNodeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PowerNodeState<'a>>>> {
    let offsets: Vec<_> = power_nodes
        .iter()
        .map(|node| {
            fb::PowerNodeState::create(
                builder,
                &fb::PowerNodeStateArgs {
                    entity: node.entity,
                    nodeId: node.node_id,
                    generation: node.generation,
                    demand: node.demand,
                    efficiency: node.efficiency,
                    storageLevel: node.storage_level,
                    storageCapacity: node.storage_capacity,
                    stability: node.stability,
                    surplus: node.surplus,
                    deficit: node.deficit,
                    incidentCount: node.incident_count,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power_incidents<'a>(
    builder: &mut FbBuilder<'a>,
    incidents: &[PowerIncidentState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PowerIncidentState<'a>>>> {
    let offsets: Vec<_> = incidents
        .iter()
        .map(|incident| {
            fb::PowerIncidentState::create(
                builder,
                &fb::PowerIncidentStateArgs {
                    nodeId: incident.node_id,
                    severity: match incident.severity {
                        PowerIncidentSeverity::Warning => fb::PowerIncidentSeverity::Warning,
                        PowerIncidentSeverity::Critical => fb::PowerIncidentSeverity::Critical,
                    },
                    deficit: incident.deficit,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power_metrics<'a>(
    builder: &mut FbBuilder<'a>,
    metrics: &PowerTelemetryState,
) -> WIPOffset<fb::PowerTelemetryState<'a>> {
    let incidents = create_power_incidents(builder, &metrics.incidents);
    fb::PowerTelemetryState::create(
        builder,
        &fb::PowerTelemetryStateArgs {
            totalSupply: metrics.total_supply,
            totalDemand: metrics.total_demand,
            totalStorage: metrics.total_storage,
            totalCapacity: metrics.total_capacity,
            gridStressAvg: metrics.grid_stress_avg,
            surplusMargin: metrics.surplus_margin,
            instabilityAlerts: metrics.instability_alerts,
            incidents: Some(incidents),
        },
    )
}

fn to_fb_crisis_metric_kind(kind: CrisisMetricKind) -> fb::CrisisMetricKind {
    match kind {
        CrisisMetricKind::R0 => fb::CrisisMetricKind::R0,
        CrisisMetricKind::GridStressPct => fb::CrisisMetricKind::GridStressPct,
        CrisisMetricKind::UnauthorizedQueuePct => fb::CrisisMetricKind::UnauthorizedQueuePct,
        CrisisMetricKind::SwarmsActive => fb::CrisisMetricKind::SwarmsActive,
        CrisisMetricKind::PhageDensity => fb::CrisisMetricKind::PhageDensity,
    }
}

fn to_fb_crisis_severity_band(band: CrisisSeverityBand) -> fb::CrisisSeverityBand {
    match band {
        CrisisSeverityBand::Safe => fb::CrisisSeverityBand::Safe,
        CrisisSeverityBand::Warn => fb::CrisisSeverityBand::Warn,
        CrisisSeverityBand::Critical => fb::CrisisSeverityBand::Critical,
    }
}

fn create_crisis_trend_samples<'a>(
    builder: &mut FbBuilder<'a>,
    samples: &[CrisisTrendSample],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisTrendSample<'a>>>> {
    let offsets: Vec<_> = samples
        .iter()
        .map(|sample| {
            fb::CrisisTrendSample::create(
                builder,
                &fb::CrisisTrendSampleArgs {
                    tick: sample.tick,
                    value: sample.value,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_gauges<'a>(
    builder: &mut FbBuilder<'a>,
    gauges: &[CrisisGaugeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisGaugeState<'a>>>> {
    let offsets: Vec<_> = gauges
        .iter()
        .map(|gauge| {
            let history = create_crisis_trend_samples(builder, &gauge.history);
            fb::CrisisGaugeState::create(
                builder,
                &fb::CrisisGaugeStateArgs {
                    kind: to_fb_crisis_metric_kind(gauge.kind),
                    raw: gauge.raw,
                    ema: gauge.ema,
                    trend5t: gauge.trend_5t,
                    warnThreshold: gauge.warn_threshold,
                    criticalThreshold: gauge.critical_threshold,
                    lastUpdatedTick: gauge.last_updated_tick,
                    staleTicks: gauge.stale_ticks,
                    band: to_fb_crisis_severity_band(gauge.band),
                    history: Some(history),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_telemetry<'a>(
    builder: &mut FbBuilder<'a>,
    telemetry: &CrisisTelemetryState,
) -> WIPOffset<fb::CrisisTelemetryState<'a>> {
    let gauges = create_crisis_gauges(builder, &telemetry.gauges);
    fb::CrisisTelemetryState::create(
        builder,
        &fb::CrisisTelemetryStateArgs {
            gauges: Some(gauges),
            modifiersActive: telemetry.modifiers_active,
            foreshockIncidents: telemetry.foreshock_incidents,
            containmentIncidents: telemetry.containment_incidents,
            warningsActive: telemetry.warnings_active,
            criticalsActive: telemetry.criticals_active,
        },
    )
}

fn create_crisis_overlay_annotations<'a>(
    builder: &mut FbBuilder<'a>,
    annotations: &[CrisisOverlayAnnotationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisOverlayAnnotationState<'a>>>> {
    let offsets: Vec<_> = annotations
        .iter()
        .map(|annotation| {
            let path = builder.create_vector(&annotation.path);
            let label = builder.create_string(&annotation.label);
            fb::CrisisOverlayAnnotationState::create(
                builder,
                &fb::CrisisOverlayAnnotationStateArgs {
                    label: Some(label),
                    severity: to_fb_crisis_severity_band(annotation.severity),
                    path: Some(path),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &CrisisOverlayState,
) -> WIPOffset<fb::CrisisOverlayState<'a>> {
    let heatmap = create_scalar_raster(builder, &overlay.heatmap);
    let annotations = create_crisis_overlay_annotations(builder, &overlay.annotations);
    fb::CrisisOverlayState::create(
        builder,
        &fb::CrisisOverlayStateArgs {
            heatmap: Some(heatmap),
            annotations: Some(annotations),
        },
    )
}

fn to_fb_knowledge_field(field: KnowledgeField) -> fb::KnowledgeField {
    match field {
        KnowledgeField::Physics => fb::KnowledgeField::Physics,
        KnowledgeField::Chemistry => fb::KnowledgeField::Chemistry,
        KnowledgeField::Biology => fb::KnowledgeField::Biology,
        KnowledgeField::Data => fb::KnowledgeField::Data,
        KnowledgeField::Communication => fb::KnowledgeField::Communication,
        KnowledgeField::Exotic => fb::KnowledgeField::Exotic,
    }
}

fn create_great_discoveries<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::GreatDiscoveryState::create(
                builder,
                &fb::GreatDiscoveryStateArgs {
                    id: entry.id,
                    faction: entry.faction,
                    field: to_fb_knowledge_field(entry.field),
                    tick: entry.tick,
                    publiclyDeployed: entry.publicly_deployed,
                    effectFlags: entry.effect_flags,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_definition_requirements<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryRequirementState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryRequirementDefinition<'a>>>>
{
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let name = entry
                .name
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let summary = entry
                .summary
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            fb::GreatDiscoveryRequirementDefinition::create(
                builder,
                &fb::GreatDiscoveryRequirementDefinitionArgs {
                    discoveryId: entry.discovery,
                    weight: entry.weight,
                    minimumProgress: entry.minimum_progress,
                    name,
                    summary,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_definitions<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryDefinitionState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryDefinition<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let name = builder.create_string(entry.name.as_str());
            let tier = entry
                .tier
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let summary = entry
                .summary
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let tags = if entry.tags.is_empty() {
                None
            } else {
                let mut tag_offsets = Vec::with_capacity(entry.tags.len());
                for tag in &entry.tags {
                    tag_offsets.push(builder.create_string(tag.as_str()));
                }
                Some(builder.create_vector(&tag_offsets))
            };
            let effects_summary = if entry.effects_summary.is_empty() {
                None
            } else {
                let mut effect_offsets = Vec::with_capacity(entry.effects_summary.len());
                for line in &entry.effects_summary {
                    effect_offsets.push(builder.create_string(line.as_str()));
                }
                Some(builder.create_vector(&effect_offsets))
            };
            let observation_notes = entry
                .observation_notes
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let leak_profile = entry
                .leak_profile
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let requirements =
                create_great_discovery_definition_requirements(builder, &entry.requirements);

            fb::GreatDiscoveryDefinition::create(
                builder,
                &fb::GreatDiscoveryDefinitionArgs {
                    id: entry.id,
                    name: Some(name),
                    field: to_fb_knowledge_field(entry.field),
                    observationThreshold: entry.observation_threshold,
                    cooldownTicks: entry.cooldown_ticks,
                    freshnessWindow: entry.freshness_window.unwrap_or_default(),
                    hasFreshnessWindow: entry.freshness_window.is_some(),
                    effectFlags: entry.effect_flags,
                    covertUntilPublic: entry.covert_until_public,
                    tier,
                    summary,
                    tags,
                    effectsSummary: effects_summary,
                    observationNotes: observation_notes,
                    leakProfile: leak_profile,
                    requirements: Some(requirements),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_progress<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryProgressState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryProgressState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::GreatDiscoveryProgressState::create(
                builder,
                &fb::GreatDiscoveryProgressStateArgs {
                    faction: entry.faction,
                    discovery: entry.discovery,
                    progress: entry.progress,
                    observationDeficit: entry.observation_deficit,
                    etaTicks: entry.eta_ticks,
                    covert: entry.covert,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_telemetry<'a>(
    builder: &mut FbBuilder<'a>,
    telemetry: &GreatDiscoveryTelemetryState,
) -> WIPOffset<fb::GreatDiscoveryTelemetryState<'a>> {
    fb::GreatDiscoveryTelemetryState::create(
        builder,
        &fb::GreatDiscoveryTelemetryStateArgs {
            totalResolved: telemetry.total_resolved,
            pendingCandidates: telemetry.pending_candidates,
            activeConstellations: telemetry.active_constellations,
        },
    )
}

fn create_knowledge_ledger<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeLedgerEntryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeLedgerState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let countermeasures = create_knowledge_countermeasures(builder, &entry.countermeasures);
            let infiltrations = create_knowledge_infiltrations(builder, &entry.infiltrations);
            let modifiers = create_knowledge_modifiers(builder, &entry.modifiers);
            fb::KnowledgeLedgerState::create(
                builder,
                &fb::KnowledgeLedgerStateArgs {
                    discoveryId: entry.discovery_id,
                    ownerFaction: entry.owner_faction,
                    tier: entry.tier,
                    progressPercent: entry.progress_percent,
                    halfLifeTicks: entry.half_life_ticks,
                    timeToCascade: entry.time_to_cascade,
                    securityPosture: to_fb_knowledge_security_posture(entry.security_posture),
                    countermeasures: Some(countermeasures),
                    infiltrations: Some(infiltrations),
                    modifiers: Some(modifiers),
                    flags: entry.flags.bits(),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_countermeasures<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeCountermeasureState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeCountermeasureState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::KnowledgeCountermeasureState::create(
                builder,
                &fb::KnowledgeCountermeasureStateArgs {
                    kind: to_fb_knowledge_countermeasure(entry.kind),
                    potency: entry.potency,
                    upkeep: entry.upkeep,
                    remainingTicks: entry.remaining_ticks,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_infiltrations<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeInfiltrationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeInfiltrationState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::KnowledgeInfiltrationState::create(
                builder,
                &fb::KnowledgeInfiltrationStateArgs {
                    faction: entry.faction,
                    blueprintFidelity: entry.blueprint_fidelity,
                    suspicion: entry.suspicion,
                    cells: entry.cells,
                    lastActivityTick: entry.last_activity_tick,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_modifiers<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeModifierBreakdownState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeModifierBreakdownState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let note = entry
                .note_handle
                .as_ref()
                .map(|note| builder.create_string(note.as_str()));
            fb::KnowledgeModifierBreakdownState::create(
                builder,
                &fb::KnowledgeModifierBreakdownStateArgs {
                    source: to_fb_knowledge_modifier_source(entry.source),
                    deltaHalfLife: entry.delta_half_life,
                    deltaProgress: entry.delta_progress,
                    noteHandle: note,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_timeline<'a>(
    builder: &mut FbBuilder<'a>,
    events: &[KnowledgeTimelineEventState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeTimelineEventState<'a>>>> {
    let offsets: Vec<_> = events
        .iter()
        .map(|event| {
            let note = event
                .note_handle
                .as_ref()
                .map(|note| builder.create_string(note.as_str()));
            fb::KnowledgeTimelineEventState::create(
                builder,
                &fb::KnowledgeTimelineEventStateArgs {
                    tick: event.tick,
                    kind: to_fb_knowledge_timeline_kind(event.kind),
                    sourceFaction: event.source_faction,
                    deltaPercent: event.delta_percent,
                    noteHandle: note,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_metrics<'a>(
    builder: &mut FbBuilder<'a>,
    metrics: &KnowledgeMetricsState,
) -> WIPOffset<fb::KnowledgeMetricsState<'a>> {
    fb::KnowledgeMetricsState::create(
        builder,
        &fb::KnowledgeMetricsStateArgs {
            leakWarnings: metrics.leak_warnings,
            leakCriticals: metrics.leak_criticals,
            countermeasuresActive: metrics.countermeasures_active,
            commonKnowledgeTotal: metrics.common_knowledge_total,
        },
    )
}

fn create_terrain_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &TerrainOverlayState,
) -> WIPOffset<fb::TerrainOverlay<'a>> {
    let sample_offsets: Vec<_> = overlay
        .samples
        .iter()
        .map(|sample| {
            fb::TerrainSample::create(
                builder,
                &fb::TerrainSampleArgs {
                    terrain: to_fb_terrain_type(sample.terrain),
                    tags: sample.tags.bits(),
                    mountainKind: to_fb_mountain_kind(sample.mountain_kind),
                    reliefScale: sample.relief_scale,
                },
            )
        })
        .collect();
    let samples = builder.create_vector(&sample_offsets);
    fb::TerrainOverlay::create(
        builder,
        &fb::TerrainOverlayArgs {
            width: overlay.width,
            height: overlay.height,
            samples: Some(samples),
        },
    )
}

fn create_scalar_raster<'a>(
    builder: &mut FbBuilder<'a>,
    raster: &ScalarRasterState,
) -> WIPOffset<fb::ScalarRaster<'a>> {
    let samples = builder.create_vector(&raster.samples);
    fb::ScalarRaster::create(
        builder,
        &fb::ScalarRasterArgs {
            width: raster.width,
            height: raster.height,
            samples: Some(samples),
        },
    )
}

fn create_float_raster<'a>(
    builder: &mut FbBuilder<'a>,
    raster: &FloatRasterState,
) -> WIPOffset<fb::FloatRaster<'a>> {
    let samples = builder.create_vector(&raster.samples);
    fb::FloatRaster::create(
        builder,
        &fb::FloatRasterArgs {
            width: raster.width,
            height: raster.height,
            samples: Some(samples),
        },
    )
}

fn create_sentiment<'a>(
    builder: &mut FbBuilder<'a>,
    sentiment: &SentimentTelemetryState,
) -> WIPOffset<fb::SentimentTelemetryState<'a>> {
    let knowledge = create_sentiment_axis(builder, &sentiment.knowledge);
    let trust = create_sentiment_axis(builder, &sentiment.trust);
    let equity = create_sentiment_axis(builder, &sentiment.equity);
    let agency = create_sentiment_axis(builder, &sentiment.agency);
    fb::SentimentTelemetryState::create(
        builder,
        &fb::SentimentTelemetryStateArgs {
            knowledge: Some(knowledge),
            trust: Some(trust),
            equity: Some(equity),
            agency: Some(agency),
        },
    )
}

fn create_sentiment_axis<'a>(
    builder: &mut FbBuilder<'a>,
    axis: &SentimentAxisTelemetry,
) -> WIPOffset<fb::SentimentAxisTelemetry<'a>> {
    let drivers: Vec<_> = axis
        .drivers
        .iter()
        .map(|driver| {
            let label = builder.create_string(driver.label.as_str());
            fb::SentimentDriverState::create(
                builder,
                &fb::SentimentDriverStateArgs {
                    category: to_fb_driver_category(driver.category),
                    label: Some(label),
                    value: driver.value,
                    weight: driver.weight,
                },
            )
        })
        .collect();
    let drivers_vec = builder.create_vector(&drivers);
    fb::SentimentAxisTelemetry::create(
        builder,
        &fb::SentimentAxisTelemetryArgs {
            policy: axis.policy,
            incidents: axis.incidents,
            influencers: axis.influencers,
            total: axis.total,
            drivers: Some(drivers_vec),
        },
    )
}

fn create_generations<'a>(
    builder: &mut FbBuilder<'a>,
    generations: &[GenerationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GenerationState<'a>>>> {
    let offsets: Vec<_> = generations
        .iter()
        .map(|generation| {
            let name = builder.create_string(generation.name.as_str());
            fb::GenerationState::create(
                builder,
                &fb::GenerationStateArgs {
                    id: generation.id,
                    name: Some(name),
                    biasKnowledge: generation.bias_knowledge,
                    biasTrust: generation.bias_trust,
                    biasEquity: generation.bias_equity,
                    biasAgency: generation.bias_agency,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_corruption<'a>(
    builder: &mut FbBuilder<'a>,
    ledger: &CorruptionLedger,
) -> WIPOffset<fb::CorruptionLedger<'a>> {
    let entries: Vec<_> = ledger
        .entries
        .iter()
        .map(|entry| {
            fb::CorruptionEntry::create(
                builder,
                &fb::CorruptionEntryArgs {
                    subsystem: to_fb_corruption_subsystem(entry.subsystem),
                    intensity: entry.intensity,
                    incidentId: entry.incident_id,
                    exposureTimer: entry.exposure_timer,
                    restitutionWindow: entry.restitution_window,
                    lastUpdateTick: entry.last_update_tick,
                },
            )
        })
        .collect();
    let entries_vec = builder.create_vector(&entries);
    fb::CorruptionLedger::create(
        builder,
        &fb::CorruptionLedgerArgs {
            entries: Some(entries_vec),
            reputationModifier: ledger.reputation_modifier,
            auditCapacity: ledger.audit_capacity,
        },
    )
}

fn create_influencers<'a>(
    builder: &mut FbBuilder<'a>,
    influencers: &[InfluentialIndividualState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::InfluentialIndividualState<'a>>>> {
    let offsets: Vec<_> = influencers
        .iter()
        .map(|inf| {
            let name = builder.create_string(inf.name.as_str());
            let audience_vec = builder.create_vector(&inf.audience_generations);
            let resonance_vec =
                create_influencer_culture_resonance(builder, &inf.culture_resonance);
            fb::InfluentialIndividualState::create(
                builder,
                &fb::InfluentialIndividualStateArgs {
                    id: inf.id,
                    name: Some(name),
                    influence: inf.influence,
                    growthRate: inf.growth_rate,
                    baselineGrowth: inf.baseline_growth,
                    notoriety: inf.notoriety,
                    sentimentKnowledge: inf.sentiment_knowledge,
                    sentimentTrust: inf.sentiment_trust,
                    sentimentEquity: inf.sentiment_equity,
                    sentimentAgency: inf.sentiment_agency,
                    sentimentWeightKnowledge: inf.sentiment_weight_knowledge,
                    sentimentWeightTrust: inf.sentiment_weight_trust,
                    sentimentWeightEquity: inf.sentiment_weight_equity,
                    sentimentWeightAgency: inf.sentiment_weight_agency,
                    logisticsBonus: inf.logistics_bonus,
                    moraleBonus: inf.morale_bonus,
                    powerBonus: inf.power_bonus,
                    logisticsWeight: inf.logistics_weight,
                    moraleWeight: inf.morale_weight,
                    powerWeight: inf.power_weight,
                    supportCharge: inf.support_charge,
                    suppressPressure: inf.suppress_pressure,
                    domains: inf.domains,
                    scope: to_fb_influence_scope(inf.scope),
                    generationScope: inf.generation_scope,
                    supported: inf.supported,
                    suppressed: inf.suppressed,
                    lifecycle: to_fb_influence_lifecycle(inf.lifecycle),
                    coherence: inf.coherence,
                    ticksInStatus: inf.ticks_in_status,
                    audienceGenerations: Some(audience_vec),
                    supportPopular: inf.support_popular,
                    supportPeer: inf.support_peer,
                    supportInstitutional: inf.support_institutional,
                    supportHumanitarian: inf.support_humanitarian,
                    weightPopular: inf.weight_popular,
                    weightPeer: inf.weight_peer,
                    weightInstitutional: inf.weight_institutional,
                    weightHumanitarian: inf.weight_humanitarian,
                    cultureResonance: Some(resonance_vec),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_influencer_culture_resonance<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[InfluencerCultureResonanceEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::InfluencerCultureResonanceEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::InfluencerCultureResonanceEntry::create(
                builder,
                &fb::InfluencerCultureResonanceEntryArgs {
                    axis: to_fb_culture_trait_axis(entry.axis),
                    weight: entry.weight,
                    output: entry.output,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_traits<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[CultureTraitEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureTraitEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::CultureTraitEntry::create(
                builder,
                &fb::CultureTraitEntryArgs {
                    axis: to_fb_culture_trait_axis(entry.axis),
                    baseline: entry.baseline,
                    modifier: entry.modifier,
                    value: entry.value,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_layers<'a>(
    builder: &mut FbBuilder<'a>,
    layers: &[CultureLayerState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureLayerState<'a>>>> {
    let offsets: Vec<_> = layers
        .iter()
        .map(|layer| {
            let traits_vec = create_culture_traits(builder, &layer.traits);
            fb::CultureLayerState::create(
                builder,
                &fb::CultureLayerStateArgs {
                    id: layer.id,
                    owner: layer.owner,
                    parent: layer.parent,
                    scope: to_fb_culture_layer_scope(layer.scope),
                    traits: Some(traits_vec),
                    divergence: layer.divergence,
                    softThreshold: layer.soft_threshold,
                    hardThreshold: layer.hard_threshold,
                    ticksAboveSoft: layer.ticks_above_soft,
                    ticksAboveHard: layer.ticks_above_hard,
                    lastUpdatedTick: layer.last_updated_tick,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_tensions<'a>(
    builder: &mut FbBuilder<'a>,
    tensions: &[CultureTensionState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureTensionState<'a>>>> {
    let offsets: Vec<_> = tensions
        .iter()
        .map(|state| {
            fb::CultureTensionState::create(
                builder,
                &fb::CultureTensionStateArgs {
                    layerId: state.layer_id,
                    scope: to_fb_culture_layer_scope(state.scope),
                    owner: state.owner,
                    severity: state.severity,
                    timer: state.timer,
                    kind: to_fb_culture_tension_kind(state.kind),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn to_fb_driver_category(category: SentimentDriverCategory) -> fb::SentimentDriverCategory {
    match category {
        SentimentDriverCategory::Policy => fb::SentimentDriverCategory::Policy,
        SentimentDriverCategory::Incident => fb::SentimentDriverCategory::Incident,
        SentimentDriverCategory::Influencer => fb::SentimentDriverCategory::Influencer,
    }
}

fn to_fb_terrain_type(terrain: TerrainType) -> fb::TerrainType {
    match terrain {
        TerrainType::DeepOcean => fb::TerrainType::DeepOcean,
        TerrainType::ContinentalShelf => fb::TerrainType::ContinentalShelf,
        TerrainType::InlandSea => fb::TerrainType::InlandSea,
        TerrainType::CoralShelf => fb::TerrainType::CoralShelf,
        TerrainType::HydrothermalVentField => fb::TerrainType::HydrothermalVentField,
        TerrainType::TidalFlat => fb::TerrainType::TidalFlat,
        TerrainType::RiverDelta => fb::TerrainType::RiverDelta,
        TerrainType::MangroveSwamp => fb::TerrainType::MangroveSwamp,
        TerrainType::FreshwaterMarsh => fb::TerrainType::FreshwaterMarsh,
        TerrainType::Floodplain => fb::TerrainType::Floodplain,
        TerrainType::AlluvialPlain => fb::TerrainType::AlluvialPlain,
        TerrainType::PrairieSteppe => fb::TerrainType::PrairieSteppe,
        TerrainType::MixedWoodland => fb::TerrainType::MixedWoodland,
        TerrainType::BorealTaiga => fb::TerrainType::BorealTaiga,
        TerrainType::PeatHeath => fb::TerrainType::PeatHeath,
        TerrainType::HotDesertErg => fb::TerrainType::HotDesertErg,
        TerrainType::RockyReg => fb::TerrainType::RockyReg,
        TerrainType::SemiAridScrub => fb::TerrainType::SemiAridScrub,
        TerrainType::SaltFlat => fb::TerrainType::SaltFlat,
        TerrainType::OasisBasin => fb::TerrainType::OasisBasin,
        TerrainType::Tundra => fb::TerrainType::Tundra,
        TerrainType::PeriglacialSteppe => fb::TerrainType::PeriglacialSteppe,
        TerrainType::Glacier => fb::TerrainType::Glacier,
        TerrainType::SeasonalSnowfield => fb::TerrainType::SeasonalSnowfield,
        TerrainType::RollingHills => fb::TerrainType::RollingHills,
        TerrainType::HighPlateau => fb::TerrainType::HighPlateau,
        TerrainType::AlpineMountain => fb::TerrainType::AlpineMountain,
        TerrainType::KarstHighland => fb::TerrainType::KarstHighland,
        TerrainType::CanyonBadlands => fb::TerrainType::CanyonBadlands,
        TerrainType::ActiveVolcanoSlope => fb::TerrainType::ActiveVolcanoSlope,
        TerrainType::BasalticLavaField => fb::TerrainType::BasalticLavaField,
        TerrainType::AshPlain => fb::TerrainType::AshPlain,
        TerrainType::FumaroleBasin => fb::TerrainType::FumaroleBasin,
        TerrainType::ImpactCraterField => fb::TerrainType::ImpactCraterField,
        TerrainType::KarstCavernMouth => fb::TerrainType::KarstCavernMouth,
        TerrainType::SinkholeField => fb::TerrainType::SinkholeField,
        TerrainType::AquiferCeiling => fb::TerrainType::AquiferCeiling,
        TerrainType::NavigableRiver => fb::TerrainType::NavigableRiver,
    }
}

fn to_fb_mountain_kind(kind: MountainKind) -> fb::MountainKind {
    match kind {
        MountainKind::None => fb::MountainKind::None,
        MountainKind::Fold => fb::MountainKind::Fold,
        MountainKind::Fault => fb::MountainKind::Fault,
        MountainKind::Volcanic => fb::MountainKind::Volcanic,
        MountainKind::Dome => fb::MountainKind::Dome,
    }
}

fn to_fb_corruption_subsystem(subsystem: CorruptionSubsystem) -> fb::CorruptionSubsystem {
    match subsystem {
        CorruptionSubsystem::Logistics => fb::CorruptionSubsystem::Logistics,
        CorruptionSubsystem::Trade => fb::CorruptionSubsystem::Trade,
        CorruptionSubsystem::Military => fb::CorruptionSubsystem::Military,
        CorruptionSubsystem::Governance => fb::CorruptionSubsystem::Governance,
    }
}

fn to_fb_influence_scope(scope: InfluenceScopeKind) -> fb::InfluenceScopeKind {
    match scope {
        InfluenceScopeKind::Local => fb::InfluenceScopeKind::Local,
        InfluenceScopeKind::Regional => fb::InfluenceScopeKind::Regional,
        InfluenceScopeKind::Global => fb::InfluenceScopeKind::Global,
        InfluenceScopeKind::Generation => fb::InfluenceScopeKind::Generation,
    }
}

fn to_fb_influence_lifecycle(lifecycle: InfluenceLifecycle) -> fb::InfluenceLifecycle {
    match lifecycle {
        InfluenceLifecycle::Potential => fb::InfluenceLifecycle::Potential,
        InfluenceLifecycle::Active => fb::InfluenceLifecycle::Active,
        InfluenceLifecycle::Dormant => fb::InfluenceLifecycle::Dormant,
    }
}

fn to_fb_culture_layer_scope(scope: CultureLayerScope) -> fb::CultureLayerScope {
    match scope {
        CultureLayerScope::Global => fb::CultureLayerScope::Global,
        CultureLayerScope::Regional => fb::CultureLayerScope::Regional,
        CultureLayerScope::Local => fb::CultureLayerScope::Local,
    }
}

fn to_fb_culture_trait_axis(axis: CultureTraitAxis) -> fb::CultureTraitAxis {
    match axis {
        CultureTraitAxis::PassiveAggressive => fb::CultureTraitAxis::PassiveAggressive,
        CultureTraitAxis::OpenClosed => fb::CultureTraitAxis::OpenClosed,
        CultureTraitAxis::CollectivistIndividualist => {
            fb::CultureTraitAxis::CollectivistIndividualist
        }
        CultureTraitAxis::TraditionalistRevisionist => {
            fb::CultureTraitAxis::TraditionalistRevisionist
        }
        CultureTraitAxis::HierarchicalEgalitarian => fb::CultureTraitAxis::HierarchicalEgalitarian,
        CultureTraitAxis::SyncreticPurist => fb::CultureTraitAxis::SyncreticPurist,
        CultureTraitAxis::AsceticIndulgent => fb::CultureTraitAxis::AsceticIndulgent,
        CultureTraitAxis::PragmaticIdealistic => fb::CultureTraitAxis::PragmaticIdealistic,
        CultureTraitAxis::RationalistMystical => fb::CultureTraitAxis::RationalistMystical,
        CultureTraitAxis::ExpansionistInsular => fb::CultureTraitAxis::ExpansionistInsular,
        CultureTraitAxis::AdaptiveStubborn => fb::CultureTraitAxis::AdaptiveStubborn,
        CultureTraitAxis::HonorBoundOpportunistic => fb::CultureTraitAxis::HonorBoundOpportunistic,
        CultureTraitAxis::MeritOrientedLineageOriented => {
            fb::CultureTraitAxis::MeritOrientedLineageOriented
        }
        CultureTraitAxis::SecularDevout => fb::CultureTraitAxis::SecularDevout,
        CultureTraitAxis::PluralisticMonocultural => fb::CultureTraitAxis::PluralisticMonocultural,
    }
}

fn to_fb_culture_tension_kind(kind: CultureTensionKind) -> fb::CultureTensionKind {
    match kind {
        CultureTensionKind::DriftWarning => fb::CultureTensionKind::DriftWarning,
        CultureTensionKind::AssimilationPush => fb::CultureTensionKind::AssimilationPush,
        CultureTensionKind::SchismRisk => fb::CultureTensionKind::SchismRisk,
    }
}

fn to_fb_knowledge_security_posture(
    posture: KnowledgeSecurityPosture,
) -> fb::KnowledgeSecurityPosture {
    match posture {
        KnowledgeSecurityPosture::Minimal => fb::KnowledgeSecurityPosture::Minimal,
        KnowledgeSecurityPosture::Standard => fb::KnowledgeSecurityPosture::Standard,
        KnowledgeSecurityPosture::Hardened => fb::KnowledgeSecurityPosture::Hardened,
        KnowledgeSecurityPosture::BlackVault => fb::KnowledgeSecurityPosture::BlackVault,
    }
}

fn to_fb_knowledge_countermeasure(
    kind: KnowledgeCountermeasureKind,
) -> fb::KnowledgeCountermeasureKind {
    match kind {
        KnowledgeCountermeasureKind::SecurityInvestment => {
            fb::KnowledgeCountermeasureKind::SecurityInvestment
        }
        KnowledgeCountermeasureKind::CounterIntelSweep => {
            fb::KnowledgeCountermeasureKind::CounterIntelSweep
        }
        KnowledgeCountermeasureKind::Misinformation => {
            fb::KnowledgeCountermeasureKind::Misinformation
        }
        KnowledgeCountermeasureKind::KnowledgeDebtRelief => {
            fb::KnowledgeCountermeasureKind::KnowledgeDebtRelief
        }
    }
}

fn to_fb_knowledge_modifier_source(source: KnowledgeModifierSource) -> fb::KnowledgeModifierSource {
    match source {
        KnowledgeModifierSource::Visibility => fb::KnowledgeModifierSource::Visibility,
        KnowledgeModifierSource::Security => fb::KnowledgeModifierSource::Security,
        KnowledgeModifierSource::Spycraft => fb::KnowledgeModifierSource::Spycraft,
        KnowledgeModifierSource::Culture => fb::KnowledgeModifierSource::Culture,
        KnowledgeModifierSource::Exposure => fb::KnowledgeModifierSource::Exposure,
        KnowledgeModifierSource::Debt => fb::KnowledgeModifierSource::Debt,
        KnowledgeModifierSource::Treaty => fb::KnowledgeModifierSource::Treaty,
        KnowledgeModifierSource::Event => fb::KnowledgeModifierSource::Event,
    }
}

fn to_fb_knowledge_timeline_kind(
    kind: KnowledgeTimelineEventKind,
) -> fb::KnowledgeTimelineEventKind {
    match kind {
        KnowledgeTimelineEventKind::LeakProgress => fb::KnowledgeTimelineEventKind::LeakProgress,
        KnowledgeTimelineEventKind::SpyProbe => fb::KnowledgeTimelineEventKind::SpyProbe,
        KnowledgeTimelineEventKind::CounterIntel => fb::KnowledgeTimelineEventKind::CounterIntel,
        KnowledgeTimelineEventKind::Exposure => fb::KnowledgeTimelineEventKind::Exposure,
        KnowledgeTimelineEventKind::Treaty => fb::KnowledgeTimelineEventKind::Treaty,
        KnowledgeTimelineEventKind::Cascade => fb::KnowledgeTimelineEventKind::Cascade,
        KnowledgeTimelineEventKind::Digest => fb::KnowledgeTimelineEventKind::Digest,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerationState {
    pub id: u16,
    pub name: String,
    pub bias_knowledge: i64,
    pub bias_trust: i64,
    pub bias_equity: i64,
    pub bias_agency: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `WorldSnapshot` carrying exactly one herd — the rest of the world is irrelevant to the herd
    /// telemetry's wire encoding.
    fn snapshot_with_herd(herd: HerdTelemetryState) -> WorldSnapshot {
        WorldSnapshot {
            herds: vec![herd],
            ..WorldSnapshot::default()
        }
    }

    /// **The pen-as-a-managed-population fields survive the wire.** `penUpkeep` (what the pen eats
    /// each turn) and `penFedFraction` (`< 1` = starving) are appended to `HerdTelemetryState`
    /// (append-only discipline), and the client renders the feed as a negative row against the
    /// **gross** `corralYield`. Encode → decode with the generated reader, so a field that silently
    /// failed to serialize cannot pass.
    #[test]
    fn herd_pen_upkeep_and_fed_fraction_round_trip_on_the_wire() {
        const UPKEEP: f32 = 1.2;
        const FED: f32 = 0.25;
        const CORRAL_YIELD: f32 = 3.6;

        let snapshot = snapshot_with_herd(HerdTelemetryState {
            id: "herd_pen".to_string(),
            species: "Red Deer".to_string(),
            corralled: true,
            corral_yield: CORRAL_YIELD,
            pen_upkeep: UPKEEP,
            pen_fed_fraction: FED,
            ..Default::default()
        });

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let herd = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present")
            .herds()
            .expect("herds present")
            .get(0);
        assert!(herd.corralled());
        assert!((herd.corralYield() - CORRAL_YIELD).abs() < 1e-6);
        assert!((herd.penUpkeep() - UPKEEP).abs() < 1e-6);
        assert!((herd.penFedFraction() - FED).abs() < 1e-6);
    }

    /// A herd that is **not** penned eats nothing and is never starving — it decodes to the neutral
    /// pair (the `= 0` / `= 1` schema defaults).
    #[test]
    fn an_unpenned_herd_defaults_to_no_upkeep_and_fully_fed() {
        let snapshot = snapshot_with_herd(HerdTelemetryState {
            id: "herd_wild".to_string(),
            ..Default::default()
        });

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let herd = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present")
            .herds()
            .expect("herds present")
            .get(0);
        assert_eq!(herd.penUpkeep(), 0.0);
        assert_eq!(herd.penFedFraction(), 1.0);
    }
}
