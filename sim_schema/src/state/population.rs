//! Population-section state: cohorts, demographics, labor assignments, and tasks.

use crate::state::economy::KnownTechFragment;
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
    /// **WHERE THIS CREW STOPS, as a fraction of the source's `K`** — the whole of what the player
    /// decides about pressure (`docs/plan_harvest_floor.md` §1), and the authority [`Self::policy`]
    /// is merely a label for. `0.5` holds a source on its most productive biomass; `0` takes
    /// everything. `0.0` on a band-wide role (Scout/Warrior), which carries no source to stop short
    /// of. Appended (append-only).
    #[serde(default)]
    pub floor: f32,
    /// **Which named plant a Forage assignment asks a `Cultivate`/`Sow` to commit its patch to**
    /// (Flora Roster S1) — a `flora_config.json` species key, or `""` for *"pick the tile's
    /// dominant legal plant for me"*. Persisted intent, exactly like [`Self::policy`]: it rides the
    /// rollback record so a rewind restores the selection the player made, not a re-picked one.
    /// Empty on every non-Forage row. Appended (append-only).
    #[serde(default)]
    pub species: String,
    /// Provisions this source actually produced this turn (per-source food-income breakdown). Derived
    /// per-turn at capture (0.0 on a row no turn has resolved yet). Appended (append-only).
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
    /// (Cultivate/Tame/Corral/Sow) and every managed rung-3 source; true for Surplus/Deplete/Eradicate.
    /// A row with no yield (Scout/Warrior, or an unresolved [`SourceYield::ZERO`]) is `false`. Derived
    /// per-turn at capture. Appended (append-only).
    #[serde(default)]
    pub overdraws: bool,
    /// **The steady per-turn income this source realizes** — the honest long-run average of the lumpy
    /// [`Self::actual_yield`]: `min(workers × per-worker throughput, this policy's steady per-turn
    /// ceiling)`, the pre-quantization rate the kill-credit bank is fed. On a whole-animal (hunt)
    /// source `actual_yield` pulses (0 on wait turns, spikes on kills) while this holds steady at
    /// ~`MSY`; on a continuous forage/Field source the two are equal. The client's headline "Food
    /// /turn" reads this instead of the jumpy `actual_yield`. Derived per-turn at capture (0 on a row
    /// no turn has resolved yet). Appended (append-only).
    #[serde(default)]
    pub realized_yield: f32,
    /// **WHEN the food actually lands** — the discrete twin of [`Self::realized_yield`], from the
    /// *same* forward simulation run **with** the kill-credit bank. `arrival_schedule[i]` is the food
    /// delivered `i + 1` turns from now; the length is `labor_config.arrivals_horizon_turns` (20), and
    /// `0.0` marks a turn on which nothing lands. A big-game Sustain hunt reads lumpy — zeros between
    /// hauls, totalling ≈ `realized_yield × horizon`, because the bank moves the *timing* and not the
    /// total — while a forage patch or fast game is positive in every slot, a continuous source the
    /// client draws as a solid run. **Empty** on a row that was never projected (Scout/Warrior, or an
    /// unresolved [`SourceYield::ZERO`]): read that as *no data*, never as famine. Derived per-turn at
    /// capture from the source's **post-take** state, so slot 0 is the *next* delivery. Appended
    /// (append-only).
    #[serde(default)]
    pub arrival_schedule: Vec<f32>,
    /// **Trade goods this source produced this turn** — the twin of [`Self::actual_yield`] in the
    /// other currency (issue #337). Every harvesting policy now sells the species' trade component,
    /// so this is non-zero on rungs that earned nothing before, and it is the ONLY thing a wolf hunt
    /// produces. Render a trade line **only when `> 0`**.
    ///
    /// **NOT food income.** `PopulationCohortState::food_income` stays `Σ actual_yield`; folding this
    /// in would break the pinned larder identity (trade goods credit the faction stockpile, never the
    /// larder).
    #[serde(default)]
    pub trade_yield: f32,
    /// **The steady forward-projected trade/turn** — the twin of [`Self::realized_yield`].
    ///
    /// `0.0` on every **forage** source: the plant web's trade *projection* is a known gap (#337
    /// vectorised the animal web), while the trade a gather actually earned *is* reported in
    /// [`Self::trade_yield`]. There is deliberately no trade *arrival schedule* — see
    /// [`Self::arrival_schedule`].
    #[serde(default)]
    pub realized_trade_yield: f32,
    /// **Fodder this source produced this turn** — the third account beside [`Self::actual_yield`]
    /// and [`Self::trade_yield`] (issue #449), and exactly the `min(production, collection)` the
    /// band's `FODDER` store was credited with, the wild credit's *Foddering* knowledge gate
    /// included: a gated-off row reports `0.0` because the band was paid `0.0`. Reported, never
    /// recomputed.
    ///
    /// **Plant-only, structurally rather than by omission**: no animal pays fodder, so every hunt row
    /// is an honest `0.0`. What it exists for is the opposite case — a sown **hay Field**
    /// (`flora_config.json`'s `hay_grass`: no provisions, no trade, positive fodder) whose compact
    /// readout said `+0.00` while it fed the band's herds every turn.
    ///
    /// **NOT food income**, the same rule [`Self::trade_yield`] carries: `food_income` stays
    /// `Σ actual_yield`; fodder credits the band's `FODDER` store and never touches the larder.
    ///
    /// There is deliberately no `realized_fodder_yield` twin — [`Self::realized_trade_yield`] exists
    /// because the *animal* web projects a steady rate, and fodder is paid by the *plant* web alone,
    /// whose projection is the very gap that field is already `0.0` for. Read this actual, exactly as
    /// a client already falls back to [`Self::trade_yield`] on every forage source. Appended
    /// (append-only).
    #[serde(default)]
    pub fodder_yield: f32,
    /// **The band [`Self::actual_yield`] sits in the middle of** — *"6–11, likely 9"*
    /// (`docs/plan_hunt_through_combat.md` §6.4).
    ///
    /// A hunt has two stochastic stages (the quarry's retreat, the fight's per-unit attack rolls), so
    /// a **pre-commit** row states an expectation rather than a promise, and this is the band the sim
    /// will pay inside. `forecast == actual` is restated accordingly: `actual_yield` is the take's
    /// **expectation** over the seed, and the take lies within `[low, high]`. **Where nothing is
    /// stochastic the range is a point** — `low == actual_yield == high`, bit-for-bit — which is the
    /// plant web, a pen, and every resolved row. A **wild hunt** is not one of them: the roster's
    /// `wariness` is authored, so an animal-web pre-commit row carries a real band. Render one number
    /// when the two agree, a range only when they differ — one rule, covering both.
    ///
    /// A **resolved** row reports the point it paid: the take happened, so there is no distribution
    /// left. Appended (append-only).
    #[serde(default)]
    pub actual_yield_low: f32,
    /// The optimistic bound — see [`Self::actual_yield_low`].
    #[serde(default)]
    pub actual_yield_high: f32,
    /// The trade-goods twin of [`Self::actual_yield_low`], carried because the forecast is a **pair**
    /// everywhere else (#337): a wolf's food band is honestly all-zero, so a food-only range could
    /// not state its take at all.
    #[serde(default)]
    pub trade_yield_low: f32,
    /// The optimistic bound on the trade component — see [`Self::trade_yield_low`].
    #[serde(default)]
    pub trade_yield_high: f32,
    /// **What this crew is BUILDING on the source** — the second, independent axis of an assignment
    /// (issue #442, `docs/plan_investment_rung_toggle.md`): `""` | `"cultivate"` | `"sow"` |
    /// `"tame"` | `"corral"`.
    ///
    /// [`Self::policy`] is now always one of the four harvest **stances** and is **never rewritten by
    /// the sim**. The four build verbs used to be values of `policy`, so committing to an improvement
    /// vacated the player's stated stance and completion had to hand one back; with the axes split,
    /// completion clears **this** field and leaves `policy` alone.
    ///
    /// Persisted intent, like [`Self::policy`] and [`Self::species`]: it rides the rollback record, so
    /// a rewind restores a half-finished build's verb rather than dropping it.
    #[serde(default)]
    pub improvement: String,
    /// **The `equipment.json` roster id this crew is working under** — what the player named on
    /// `assign_labor`, or the job's default when they named none, **resolved**: the sim never
    /// publishes "unspecified", so a forage/hunt row always names a real roster entry and the row's
    /// yields are priced at exactly it.
    ///
    /// `""` on a band-wide role (scout / warrior), which consumes no kit component and therefore has
    /// no kit axis — *"no selection to make"*, not *"no kit"*. Appended last.
    #[serde(default)]
    pub kit_id: String,
}

/// **One item's remaining condition in a band's TOE** — a row of
/// [`PopulationCohortState::kit_item_conditions`].
///
/// `item_id` is `equipment.json`'s item key (`"spears"`, `"sled"`, `"baskets"`, `"traps"`), which is
/// also what a kit's `uses` list names. A client should render whatever rows arrive rather than
/// looking for a fixed set: the roster is config, and an item added there appears here with no
/// schema change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KitItemConditionState {
    pub item_id: String,
    /// Remaining condition on `equipment.json`'s 0–100 scale, clamped at `0`. **`0` = dry.** Never a
    /// performance reading — see the field's docs.
    pub remaining: f32,
}

/// **What one kit would grant THIS band, at its current wear** — a row of
/// [`PopulationCohortState::kit_tiers`], one per kit the roster offers.
///
/// # This is the RESOLVED answer. A client must not re-derive it.
///
/// The numbers here are the sim's, resolved through the same `equipment.*` seams the take path reads
/// — so the tier a picker shows is the tier a party sent with that kit actually fights and hauls at.
/// **Do not step a tier down from [`SubsistenceSnapshot::kits`] by looking at
/// [`PopulationCohortState::kit_item_conditions`]**: that is the derivation this field exists to
/// stop, and it cannot be done correctly from the wire.
///
/// **Why it cannot**: stepping a tier down needs to know *which item supplies which axis*, and that
/// mapping is per kit — `big_game` supplies `attack` from `spears`, `trapping` supplies it from
/// `traps`. A kit's `item_ids` names what it carries but not what each item is *for*, and no rule
/// over that list recovers it: set-cover and positional order both mis-assign, "any item live" keeps
/// a kit at full tier with its weapon dry, and "all items dry" keeps it at full tier with only the
/// sled left. The live symptom of guessing was a band with **fresh traps and dry spears** being
/// repriced to the bare hand under `trapping` — a fact the sim knew that the wire did not carry.
///
/// Empty for a cohort captured before the roster resolved (never in practice: the roster is config
/// and always present), which reads as "no per-band tiers published" rather than "every tier is
/// zero" — a consumer must fall back to the world roster's fresh-kit numbers, not to `0`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BandKitTiersState {
    /// The `equipment.json` roster id these tiers are for — pairs with `SubsistenceSnapshot::kits`.
    pub kit_id: String,
    /// This band's hunter `attack` under this kit. **Unbounded**; the mass window is beside it.
    pub attack: f32,
    /// This band's per-hunter HUNT haul rate (biomass/turn) under this kit.
    pub hunt_carry_per_worker_biomass: f32,
    /// This band's per-gatherer throughput (biomass/turn, **before** the tile's seasonal weight).
    pub forage_carry_per_worker_biomass: f32,
    /// **The range of quarry [`Self::attack`] applies to.** `0` on either end is unbounded. It rides
    /// per band because a spent item contributes no bound either — a kit whose mass-bounded weapon
    /// has run dry has no window, and the party is on the bare hand everywhere.
    #[serde(default)]
    pub attack_min_body_mass: f32,
    /// See [`Self::attack_min_body_mass`].
    #[serde(default)]
    pub attack_max_body_mass: f32,
    /// What this kit multiplies the quarry's own `wariness` by, at this band's wear. Neutral `1.0`.
    #[serde(default = "kit_multiplier_neutral")]
    pub dispersion: f32,
    /// What this kit multiplies the hunt's injury hazard by, at this band's wear. Neutral `1.0`.
    #[serde(default = "kit_multiplier_neutral")]
    pub exposure: f32,
    /// **This band's per-keeper PEN collection rate under this kit** (biomass/turn) — *not*
    /// [`Self::hunt_carry_per_worker_biomass`]. A sled drags a carcass in off the range and a pen
    /// stands at the camp, so a kit carrying a sled and no handling gear reads the bare rate here
    /// while its haul tier is the sledded one.
    #[serde(default)]
    pub pen_carry_per_worker_biomass: f32,
    /// **The sight range each posted vantage reveals at under this kit.** How far the vantages are
    /// *posted* is not a kit axis (three `labor_config.scout.*` dials); this is only how far each one
    /// sees, and the reveal path rounds it to whole tiles.
    #[serde(default)]
    pub scout_vantage_range: f32,
}

/// The neutral value of [`BandKitTiersState`]'s two multipliers — `1.0`, never `0`.
///
/// Same reason `KitOptionState` spells its own out: `0` is the *reassuring* wrong answer. A
/// `dispersion 0` says nothing breaks off at contact and an `exposure 0` says nobody can be hurt, so
/// a field that failed to arrive would hand every band the passive device's whole advantage.
fn kit_multiplier_neutral() -> f32 {
    1.0
}

/// Hand-written for the reason [`KitOptionState`]'s is: two of these fields are multipliers whose
/// neutral is `1`, and a derived `Default` would answer `0`.
impl Default for BandKitTiersState {
    fn default() -> Self {
        Self {
            kit_id: String::new(),
            attack: 0.0,
            hunt_carry_per_worker_biomass: 0.0,
            forage_carry_per_worker_biomass: 0.0,
            attack_min_body_mass: 0.0,
            attack_max_body_mass: 0.0,
            dispersion: kit_multiplier_neutral(),
            exposure: kit_multiplier_neutral(),
            pen_carry_per_worker_biomass: 0.0,
            scout_vantage_range: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PopulationCohortState {
    pub entity: u64,
    /// The band's durable identity — the handle a client sends back in a command.
    ///
    /// `entity` beside it is an ECS handle and is renumbered by every rollback, so it names a band
    /// only until the next one. Commands address bands by this.
    #[serde(default)]
    pub band_id: u64,
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
    /// Persistence-only: the fractional **trade goods the party is carrying home** — the pelt/hide
    /// half of every kill's hunt yield, banked until the next drop-off/fold-back settles it into the
    /// faction stockpile (`docs/plan_hunt_yield_model.md`, issue #337). Not on the FlatBuffers wire:
    /// the client already reads the raid's *promised* trade off the forecast query's reply
    /// (`HuntTripRow::delivered_trade`), and this is server state a rollback must not silently zero
    /// (the provisions half round-trips for free in `stores`, so without this a rewind would drop
    /// the pelts and only the pelts).
    #[serde(default)]
    pub expedition_carried_trade: f32,
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
    /// it as a rising/falling trend arrow. Recomputed each turn by `simulate_population`
    /// (`PopulationCohort::last_morale_delta`).
    #[serde(default)]
    pub morale_delta: i64,
    /// Dominant negative morale driver this turn: `0 = None, 1 = Terrain, 2 = Cold, 3 = Unrest`.
    /// Names *why* morale is falling. Recomputed each turn alongside [`Self::morale_delta`].
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
    /// breakdown summed). Derived per-turn at capture (0.0 on a band no turn has resolved yet).
    /// Appended last (append-only schema discipline). Lets the client draw a food ledger
    /// footer without re-summing the assignment rows.
    #[serde(default)]
    pub food_income: f32,
    /// Band-level per-turn food consumption = `food_demand(children, working, elders)` (the same
    /// one-turn demand `turns_of_food` divides by) — **the PEOPLE's food only**. Derived per-turn at
    /// capture. Appended last.
    #[serde(default)]
    pub food_consumption: f32,
    /// Hunt levers — global config echoed per-cohort (same idiom as
    /// [`Self::expedition_viability_warn_turns`], and populated for **every** cohort, since the
    /// outfit/hunt UI lives on the resident-band panel).
    ///
    /// The pre-launch **expedition** trip length is **not** computed from these: the client **asks**
    /// for the sim's simulated answer (`sim_runtime`'s `QueryCommand`, answered by
    /// `core_sim::forecast_query` for an exact band, kit, party and floor) and flags NOT VIABLE when
    /// `turns_to_fill > expedition_viability_warn_turns` (or `turns_to_fill == 0` → "won't fill").
    /// A party after an INEDIBLE quarry gets `delivers_food == false` (a species fact since #337,
    /// not a denial policy): render "no food delivered", never an ETA.
    ///
    /// It used to read that answer out of a `huntTripEstimates` table on the herd row. The table was
    /// pre-computed for every huntable herd every frame, at one kit over a fresh component set, and
    /// sampled on both axes — so it could not answer for the band actually asking.
    ///
    /// One hunter's per-turn provisions throughput (`labor_config.hunt.per_worker_biomass_capacity ×
    /// fauna_config.hunt.provisions_per_biomass`).
    ///
    /// **SPECIES-BLIND — never use it for a per-herd preview** (#337). It is a per-cohort echo of the
    /// GLOBAL `hunt.provisions_per_biomass`; the cohort has no herd, so there is no species to resolve
    /// a hunt-yield vector from. A wolf's per-policy ceilings are all `0` food, and quoting a positive
    /// per-hunter food rate against them is a contradiction. The per-herd, species-aware rates are
    /// `HerdTelemetryState::per_worker_yield` / `per_worker_trade` — clamp a band preview with THOSE.
    /// This survives as the expedition **outfit** lever (rough carry arithmetic before a target is
    /// chosen); for a chosen target the client asks for the answer (`sim_runtime`'s `QueryCommand`,
    /// answered by `core_sim::forecast_query`), exactly as the prose above describes.
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
    /// (pinned by `core_sim/tests/fauna_husbandry.rs`). Derived per-turn, exactly like `food_income`.
    /// Appended.
    #[serde(default)]
    pub pen_feed_upkeep: f32,
    /// One worker's carry contribution to a hunt expedition's haul
    /// (`expedition_config.hunt.per_worker_carry`). Global config echoed per-cohort (same idiom as
    /// [`Self::expedition_viability_warn_turns`] / [`Self::hunt_per_worker_provisions`]), populated
    /// for **every** cohort since the outfit UI lives on the resident-band panel. The client computes a hypothetical party's pre-launch HAUL as
    /// `party_workers × expedition_per_worker_carry` (the carry cap the pack fills to before
    /// auto-Delivering; a launched party's own echo is [`Self::expedition_carry_cap`]). Appended.
    #[serde(default)]
    pub expedition_per_worker_carry: f32,
    /// A band's move speed (`labor_config.band_move_tiles_per_turn`). Global config echoed per-cohort
    /// (same idiom as the levers above). The client adds a raid's round-trip travel to the queried
    /// pre-launch forecast as
    /// `ceil(2 × hex_distance(selected_band, herd) / band_move_tiles_per_turn)` — the forecast
    /// projects the hunting itself, never the walk. Appended.
    #[serde(default)]
    pub band_move_tiles_per_turn: f32,
    /// In-flight hunt-party delivery forecast — the in-flight twin of the queried pre-launch
    /// forecast. Turns until the carried food reaches the home larder (`0` = unknown /
    /// n/a). Computed at capture by `systems::expeditions::expedition_delivery`. Appended.
    #[serde(default)]
    pub expedition_eta_turns: u32,
    /// The food that in-flight delivery will contain (carried + still-to-take, pack-capped). `0` for a
    /// scout, a normal band, or a party whose delivery can't be projected. Appended.
    #[serde(default)]
    pub expedition_projected_delivery: f32,
    /// Whether the party relaunches for repeated trips after delivering (only `Deplete`). Appended.
    #[serde(default)]
    pub expedition_recurring: bool,
    /// The band's FODDER larder — the hay it has stored (Flora Roster F3). A second commodity key on
    /// the same `LocalStore` as provisions; a hay Field harvests into it, a pen that knows Foddering
    /// draws it, and it never converts to provisions. Appended (append-only) after #165's expedition
    /// trio. (The deprecated `foodIncomeAverage` slot sits earlier on the wire but is not carried on
    /// the Rust side.)
    #[serde(default)]
    pub fodder_store: f32,
    /// The three named fertility factors behind this turn's births — the `birth_rate` multiplier
    /// `fertility = birth_rate × hunger × reserve × trend`
    /// (`docs/plan_population_growth_model.md`), the birth path's equivalent of the four
    /// `morale_*` contributions above. Fixed-point raw (`Scalar::SCALE`), **neutral at 1.0, not at
    /// 0** — these are multiplicative factors, not signed contributions.
    ///
    /// Derived per-turn: a cohort that has not yet been through a turn publishes the all-zero
    /// default. **Zero `fertility_reserve` is the NOT-PROJECTED sentinel**: a
    /// computed `reserve` is ≥ 1 by construction, while `hunger` and `trend` both legitimately
    /// reach 0. Appended (append-only schema discipline).
    #[serde(default)]
    pub fertility_hunger: i64,
    #[serde(default)]
    pub fertility_reserve: i64,
    #[serde(default)]
    pub fertility_trend: i64,
    /// Echo of `fauna.predators.raid_radius` — how close (odd-r hex distance) an aggressive carnivore
    /// must be to raid this band's camp. A global lever surfaced per-cohort (same idiom as
    /// [`Self::work_range`]) so the client can check whether a visible aggressive predator is within
    /// **exact** raid range of the band. Appended (append-only).
    #[serde(default)]
    pub raid_radius: u32,
    /// Food the band **forfeited** to predator raids this turn (Predators Phase 3) — a negative
    /// food-ledger line, the raid twin of [`Self::pen_feed_upkeep`]. A casualty-causing raid costs the
    /// band `predators.raid_yield_forfeit_fraction` of that turn's food income (its people were
    /// defending or fleeing, not gathering), debited from the larder and capped at what it held. It
    /// extends the ledger identity to
    ///
    /// ```text
    /// larder_delta == food_income − food_consumption − pen_feed_upkeep − raid_forfeit
    /// ```
    ///
    /// (pinned by `integration_tests/tests/raid_food_ledger.rs`). It is a **past-turn** stochastic
    /// debit, NOT a recurring cost, so it is deliberately absent from the `turns_of_food` runway drain.
    /// Derived per-turn by `advance_predator_raids`. Appended.
    #[serde(default)]
    pub raid_forfeit: f32,
    /// **Where a hunt expedition's raid stops**, as a fraction of the herd's carrying capacity —
    /// the raid's whole statement of pressure (`docs/plan_harvest_floor.md`). It governs the take
    /// *and* the trip's shape: a floor below the food peak leaves more standing than one pack holds,
    /// so the party runs repeated trips (`systems::raid_is_recurring`).
    ///
    /// **`1.0` on a Scout party and on a resident band** — they harvest no herd, and an absent floor
    /// must not read as *"take everything"*, which is the one value that would be dangerous if a
    /// reader acted on it. Replaces the retired [`Self::expedition_hunt_policy`]. Appended
    /// (append-only).
    #[serde(default)]
    pub expedition_floor: f32,
    /// **Which stop will end this party's raid** — the `core_sim::HuntTripBound` key
    /// (`"pack_full"` / `"floor"` / `"herd_lost"` / `"horizon"`), read off the same in-flight
    /// forward simulation [`Self::expedition_eta_turns`] comes from, so it answers for the party's
    /// *real* orders (its own floor, against the herd's live stock) rather than for the
    /// band-agnostic pre-launch table.
    ///
    /// **`""` = not raiding** — a resident band, a scout, or a party already walking a load home.
    /// That is a different statement from `"horizon"`, which means the projection ran and found no
    /// stop inside `hunt.forecast_horizon_turns`. Appended (append-only).
    #[serde(default)]
    pub expedition_trip_bound: String,
    /// **Remaining condition on each item in the band's TOE**, one row per item the config carries
    /// (`docs/plan_hunt_through_combat.md` §4.8, `docs/plan_early_game_labor.md`). On
    /// `equipment.json`'s 0–100 scale; **`0` = dry**, at which point any role resolving through that
    /// item has stepped down to its unequipped tier and **stays there** (nothing replenishes an item
    /// in this slice — running dry is the intended pressure).
    ///
    /// **Performance is FLAT until expiry**: durability and performance are deliberately orthogonal
    /// axes, so **no readout may be scaled by these numbers** — they say how much life is left, never
    /// how well the item is working.
    ///
    /// **A list rather than one field per item**, which is what the three named
    /// `hunting`/`sled`/`basket` floats here used to be. Each item runs down on its **own quantum**
    /// (§4.8, "one item, one job"), so a band can be out of baskets with its sled untouched — and a
    /// fixed field set could not carry the traps the trapping kit added without a schema edit per
    /// item. **Driven by the CONFIG's item table, not by the band's ledger**, whose absent entries
    /// mean *no wear* — so an item the band has never used reads as full rather than going missing.
    #[serde(default)]
    pub kit_item_conditions: Vec<KitItemConditionState>,
    /// **What every offered kit would grant THIS band right now** — one row per roster kit,
    /// resolved against this band's live wear. See [`BandKitTiersState`]: it is the resolved answer,
    /// and a client must not re-derive a tier from the roster plus
    /// [`Self::kit_item_conditions`].
    ///
    /// Small by construction (bands × kits, a handful each) and it diffs out between frames when
    /// nothing wears.
    #[serde(default)]
    pub kit_tiers: Vec<BandKitTiersState>,
    /// **This band's per-hunter combat `attack`**, kit resolved in — `1.0` bare-handed (the
    /// `creatures.json` `person` row) and `20.0` with the hunting kit
    /// (`equipment.json` `hunting_kit.equipped_attack`).
    ///
    /// It is the left-hand side of the fight's gate, `max(0, attack − defense)`, against a herd's
    /// [`crate::state::HerdTelemetryState::defense`] — **below a species' `defense` that species
    /// cannot be hunted at all**, which is why the TOE had to land before the hunt resolves through
    /// combat. **Published and inert in the fight itself**: the resolver still fields the intrinsic
    /// `person` profile until the slice that moves the kill into `combat::resolve_fight`. Appended
    /// (append-only).
    #[serde(default)]
    pub hunter_attack: f32,
    /// **This band's per-worker HUNT haul rate** (biomass/turn), sled resolved in — the term every
    /// hunt take, crew-size figure and hunt forecast is capped by. Equipped it is
    /// `labor_config.json`'s `hunt.per_worker_biomass_capacity`; sledless it is `equipment.json`'s
    /// `sled_kit.unequipped_per_worker_biomass_capacity`.
    ///
    /// **Band-scoped, unlike [`crate::state::HerdTelemetryState::per_worker_biomass`]**, which stays
    /// the *equipped reference* rate because a herd has no band to resolve a tier against. Appended
    /// (append-only).
    #[serde(default)]
    pub hunt_carry_per_worker_biomass: f32,
    /// **This band's per-gatherer FORAGE throughput** (biomass/turn *before* the tile's seasonal
    /// weight), baskets resolved in — the term every gather take and gather forecast is capped by.
    /// Equipped it is `labor_config.json`'s `forage.per_worker_biomass_capacity`; bare-handed it is
    /// `equipment.json`'s `basket_kit.unequipped_per_worker_biomass_capacity`.
    ///
    /// **The forage web's own number, and before §4.8 there was none** — the field beside it answers
    /// only for the hunt, and the client must not render one as the other. Band-scoped for the same
    /// reason: `ForagePatchState::per_worker_biomass` stays the equipped reference rate because a
    /// patch has no band. Appended (append-only).
    #[serde(default)]
    pub forage_carry_per_worker_biomass: f32,
    /// **The `equipment.json` roster id the three kit tiers above are resolved through.**
    ///
    /// For an **in-flight party** it is the kit it was *sent out with*, decided at launch and carried
    /// for the party's whole life — the drawer's answer to *"what did I send them with?"*, and the
    /// tier it really fights and hauls at. For a **resident band** it is the job's **default**,
    /// because a band holds one kit per assignment and this row is per cohort; the per-crew truth is
    /// [`LaborAssignmentState::kit_id`] beside that row's own yields. Never empty. Appended last.
    #[serde(default)]
    pub kit_id: String,
    /// **How far every pre-launch raid projection in this snapshot was simulated before giving up**
    /// (`expedition_config.hunt.forecast_horizon_turns`). Global config echoed per-cohort — same idiom
    /// as [`Self::expedition_viability_warn_turns`] / [`Self::hunt_per_worker_provisions`] /
    /// [`Self::expedition_per_worker_carry`] — and populated for **every** cohort, since the
    /// outfit/hunt UI lives on the resident-band panel.
    ///
    /// It is the **scale for the projections' "never completed" sentinels**, which are all
    /// horizon-relative and none of which carried the horizon before this field:
    /// the query reply's `HuntTripRow::turns_to_fill` `== 0`, its `DenialRow::turns_to_collapse`
    /// (and its two range ends) `== 0`, and [`Self::expedition_trip_bound`] `== "horizon"`. **One
    /// lever serves all of them**: the denial forecast and the hunt forecast run over the *same*
    /// horizon (`core_sim`'s `denial_projection_at` and `hunt_trip_forecast_seeded` both read this
    /// one config field), so there is deliberately no second horizon on the wire.
    ///
    /// **It is NOT the trip length and must never be quoted as one.** A bounded raid reads *"Away
    /// ≈36 turns — 18 hunting, 18 travel"*; the unbounded case has to be a **lower bound on that
    /// same span** or the two are not comparable and the player is worse off than with *"many"*. The
    /// hunting alone is at least this many turns and the round-trip travel is a separate,
    /// already-known term (`ceil(2 × hex_distance / band_move_tiles_per_turn)`), so the floor on the
    /// whole trip is
    ///
    /// ```text
    /// forecast_horizon_turns + round-trip travel      e.g. "Away more than 78 turns"
    /// ```
    ///
    /// Quoting the horizon alone understates the trip by the entire walk — a number wrong in the
    /// *reassuring* direction, which is worse than the "many" it replaces. Appended last
    /// (append-only).
    #[serde(default)]
    pub expedition_forecast_horizon_turns: u32,
    /// **This band's per-KEEPER pen collection rate** (biomass/turn), husbandry gear resolved in —
    /// the term a corralled herd's harvest is capped by. Equipped it is `labor_config.json`'s
    /// `hunt.per_worker_biomass_capacity` (the rate a pen has always collected at); bare it is
    /// `equipment.json`'s `husbandry_gear` unequipped tier.
    ///
    /// **Not a second reading of [`Self::hunt_carry_per_worker_biomass`]**: a sled drags a carcass in
    /// off the range and a pen stands at the camp, so a band whose Hunt row is on the stalking kit
    /// collects a pen at the bare rate. Quoted at the **hunt** job's default, which is the one job
    /// [`Self::kit_id`] answers for. Appended last (append-only).
    #[serde(default)]
    pub pen_carry_per_worker_biomass: f32,
    /// **The sight range each of this band's posted scout vantages reveals at**, wayfinding gear
    /// resolved in. Equipped it is `labor_config.json`'s `scout.vantage_range`; bare it is
    /// `equipment.json`'s `wayfinding` unequipped tier. **How far the vantages are *posted* is not a
    /// kit axis** — that is three separate `labor_config` dials — and the sim rounds this to whole
    /// tiles when it reveals.
    ///
    /// Quoted at the **scout** job's default, *not* at [`Self::kit_id`]. Appended last
    /// (append-only).
    #[serde(default)]
    pub scout_vantage_range: f32,
    /// **This band's per-warrior combat `attack`**, clubs resolved in — the defending contingent's
    /// side of a predator raid (`1.0` bare-handed, `6.0` with the warrior kit).
    ///
    /// The *same* `attack` stat and the same seam [`Self::hunter_attack`] resolves through — what
    /// keeps a spear out of a raid is the kit's `jobs` list, not the stat — so the two are different
    /// numbers on the same band and a readout must not render one as the other. Quoted at the
    /// **warrior** job's default, *not* at [`Self::kit_id`]. Appended last (append-only).
    #[serde(default)]
    pub warrior_attack: f32,
    /// **`settle.min_founding_workers`** — the working-age floor the **new** band must clear when a
    /// band splits. A global config lever echoed onto *every* cohort, the same idiom as
    /// [`Self::expedition_forecast_horizon_turns`], so the compose sheet can state the number
    /// without keeping a second copy of the config. Appended last (append-only).
    #[serde(default)]
    pub founding_min_workers: u32,
    /// **`settle.parent_min_workers`** — the workers the **parent** must still hold after the split.
    ///
    /// **The two floors cross the wire; the verdict does not.** The split sheet moves a stepper, so
    /// publishing an answer would mean one field per possible composition. What crosses is the pair
    /// of thresholds the sim owns — the same shape as the per-source forecast, which publishes rates
    /// rather than an answer per party size. Appended last (append-only).
    #[serde(default)]
    pub founding_parent_min_workers: u32,
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
