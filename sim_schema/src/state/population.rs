//! Population-section state: cohorts, demographics, labor assignments, and tasks.

use crate::state::economy::KnownTechFragment;
// `HarvestTaskState` is documented against the subsistence section's hunt-trip estimate; imported
// so that intra-doc link keeps resolving from this module.
#[allow(unused_imports)]
use crate::state::subsistence::HuntTripEstimateState;
use serde::{Deserialize, Serialize};

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
    /// **Which named plant a Forage assignment asks a `Cultivate`/`Sow` to commit its patch to**
    /// (Flora Roster S1) — a `flora_config.json` species key, or `""` for *"pick the tile's
    /// dominant legal plant for me"*. Persisted intent, exactly like [`Self::policy`]: it rides the
    /// rollback record so a rewind restores the selection the player made, not a re-picked one.
    /// Empty on every non-Forage row. Appended (append-only).
    #[serde(default)]
    pub species: String,
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
    #[serde(default)]
    pub turns_of_food: f32,
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
    /// one-turn demand `turns_of_food` divides by) — **the PEOPLE's food only**. Derived per-turn at
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
    /// In-flight hunt-party delivery forecast — the in-flight twin of the pre-launch
    /// `hunt_trip_estimates`. Turns until the carried food reaches the home larder (`0` = unknown /
    /// n/a). Computed at capture by `systems::expeditions::expedition_delivery`. Appended.
    #[serde(default)]
    pub expedition_eta_turns: u32,
    /// The food that in-flight delivery will contain (carried + still-to-take, pack-capped). `0` for a
    /// scout, a normal band, or a party whose delivery can't be projected. Appended.
    #[serde(default)]
    pub expedition_projected_delivery: f32,
    /// Whether the party relaunches for repeated trips after delivering (only `Market`). Appended.
    #[serde(default)]
    pub expedition_recurring: bool,
    /// The band's FODDER larder — the hay it has stored (Flora Roster F3). A second commodity key on
    /// the same `LocalStore` as provisions; a hay Field harvests into it, a pen that knows Foddering
    /// draws it, and it never converts to provisions. Appended (append-only) after #165's expedition
    /// trio. (The deprecated `foodIncomeAverage` slot sits earlier on the wire but is not carried on
    /// the Rust side.)
    #[serde(default)]
    pub fodder_store: f32,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerationState {
    pub id: u16,
    pub name: String,
    pub bias_knowledge: i64,
    pub bias_trust: i64,
    pub bias_equity: i64,
    pub bias_agency: i64,
}
