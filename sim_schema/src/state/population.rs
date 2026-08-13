//! Population-section state: cohorts, demographics, labor assignments, and tasks.

use crate::state::economy::KnownTechFragment;
use crate::state::subsistence::MaterialPayoff;
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
    // **RETIRED: `trade_yield` / `realized_trade_yield`** (arc #527), with the trade-goods axis they
    // reported. The wire slots `tradeYield` / `realizedTradeYield` are `(deprecated)` in place.
    /// **Fodder this source produced this turn** — the second account beside [`Self::actual_yield`]
    /// (issue #449), and exactly the `min(production, collection)` the
    /// band's `FODDER` store was credited with, the wild credit's *Foddering* knowledge gate
    /// included: a gated-off row reports `0.0` because the band was paid `0.0`. Reported, never
    /// recomputed.
    ///
    /// **Plant-only, structurally rather than by omission**: no animal pays fodder, so every hunt row
    /// is an honest `0.0`. What it exists for is the opposite case — a sown **hay Field**
    /// (`flora_config.json`'s `hay_grass`: no provisions, positive fodder) whose compact
    /// readout said `+0.00` while it fed the band's herds every turn.
    ///
    /// **NOT food income**: `food_income` stays `Σ actual_yield`; fodder credits the band's `FODDER`
    /// store and never touches the larder.
    ///
    /// There is deliberately no `realized_fodder_yield` twin — fodder is paid by the *plant* web
    /// alone, whose forward projection is food-only, so a projected-fodder field would be a constant
    /// zero on the only web that can pay it. Read this actual. Appended (append-only).
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
    // **RETIRED: `trade_yield_low` / `trade_yield_high`** (arc #527). The wire slots
    // `tradeYieldLow` / `tradeYieldHigh` are `(deprecated)` in place.
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
    /// **The MATERIALS this assignment credited this turn**, one entry per material id (arc #527) —
    /// the third account beside [`Self::actual_yield`] and [`Self::fodder_yield`], and the **only**
    /// one a cash Field or an inedible quarry pays into at all. Without it a wolf hunt's row and a
    /// cotton Field's row both publish their whole product as `+0.00`.
    ///
    /// **Reported, never recomputed** — exactly what `credit_material_yield` deposited at the take
    /// site, the discipline [`Self::fodder_yield`] already carries.
    ///
    /// **Empty is "no row", never zero.** Most sources pay no material. **Never summed** into one
    /// figure, and never into `food_income`, which stays `Σ actual_yield`.
    ///
    /// A **pre-commit** row publishes an empty list even where the turn will pay — see the schema
    /// comment. Appended (append-only).
    #[serde(default)]
    pub material_yield: Vec<MaterialPayoff>,
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
    /// Remaining condition on `equipment.json`'s 0–100 scale, clamped at `0`. Never a performance
    /// reading — see the field's docs.
    ///
    /// **`0` means the band OWNS NONE**, not *"owns one that is dry"*. A batch that runs out of
    /// units is removed from the ledger, so a dry item and one the band never had both read `0`
    /// here; [`Self::count`] is what says whether it owns any, and
    /// [`EquipmentBatchState::life`] is what tells *worn out* from *never made*.
    pub remaining: f32,
    /// **Whole units the band owns**, across every batch. The explicit ownership statement, so no
    /// client has to infer ownership from a condition of zero.
    #[serde(default)]
    pub count: u32,
    /// **Workers this item actually reaches** — what a *"spears 87 · 10 of 17"* row counts.
    ///
    /// A unit arms `workers_per_unit` people, so [`Self::count`] (units owned) and this (people
    /// reached) differ whenever the band is short — or holds the reserve above its head count that a
    /// spawn stocks. Resolved through `EquipmentConfig::coverage`, the same seam the take runs
    /// through; a client cannot compute it, because `workers_per_unit` and which job is staffed are
    /// both sim-side.
    ///
    /// **Quoted at the job whose kit carries the item**, and at the one
    /// [`PopulationCohortState::kit_id`] names for an item several jobs' kits carry — the same
    /// convention [`PopulationCohortState::pen_carry_per_worker_biomass`] already follows. An item
    /// no quoted kit carries reads `0`, a bench tool included.
    #[serde(default)]
    pub workers_holding: f32,
    /// **The denominator of [`Self::workers_holding`]** — the head count of the job this row is
    /// quoted at, so the two are one sentence: *"`workers_holding` of `workers_on_quoted_job`"*.
    ///
    /// Published rather than re-derived, and it is the very number the resolving pass divided
    /// against — only the hunt has a head count on the wire otherwise
    /// ([`PopulationCohortState::hunt_crews`]), so a spears shortfall could be stated and a basket's
    /// or a club's could not.
    ///
    /// **Two zeros a reader must not confuse.** `0` here means **nobody is staffed** on that job —
    /// *"0 of 0"* is not a shortfall, and nothing may divide by it. A positive value with
    /// `workers_holding == 0` is the real one: the job is staffed and every worker on it is at the
    /// unequipped tier.
    #[serde(default)]
    pub workers_on_quoted_job: f32,
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
    /// **RETIRED — it publishes [`RETIRED_BUILD_RATE`] and nothing else.** It carried a *multiplier*
    /// on the crew's build output; the stat is now an **additive per-worker contribution off the
    /// job** ([`Self::build_work_per_worker`] beside it), because a multiplier cancels the job's cost
    /// and so saves the same *percentage* of turns whatever the job's size.
    ///
    /// The slot is held at its neutral rather than removed because the FlatBuffers `(deprecated)`
    /// keyword drops the accessor and a client still calls it. A consumer rendering a build axis must
    /// switch to the successor.
    ///
    /// **There is still no flat [`PopulationCohortState`] twin, deliberately.** The flat per-band
    /// fields answer for a readout with *no* kit selected, and a build always has one (its job's
    /// default), so a reader wanting this band's own reading takes the row whose `kit_id` matches.
    #[serde(default = "kit_multiplier_neutral")]
    pub build_rate: f32,
    /// **The work units ONE EQUIPPED WORKER of this band takes off a build's cost**, at its live wear
    /// (`docs/plan_unit_costed_work.md` §6). Neutral `0.0`; the handling gear ships `8.5`.
    ///
    /// **It supersedes [`Self::build_rate`]**, which is retired and now publishes only its neutral —
    /// a multiplier on the crew's output cancels the job's cost, so it saved the same *percentage* of
    /// turns whatever the job's size. `0` here means *this crew's tools take nothing off a build*,
    /// which is the honest reading for every kit but `husbandry` on the shipped roster.
    #[serde(default)]
    pub build_work_per_worker: f32,
    /// **How many workers this kit can actually equip for a build out of what this band holds** —
    /// the head count at or **above** which extra hands take no further work off a job. `0` = the
    /// kit carries nothing live that helps, which is every row but the handling gear's today.
    ///
    /// **It is the other half of the gear term**, and it is what makes that term a closed form a
    /// client can evaluate against a crew the player is *proposing*:
    ///
    /// ```text
    /// gear(w) = min(w, build_work_saturating_crew) × build_work_per_worker
    /// ```
    ///
    /// Coverage arms a **prefix** of a party — each item reaches `live units × workers_per_unit`
    /// people — so the contribution rises with the crew until every unit is in somebody's hands and
    /// then stops.
    ///
    /// **It rides the KIT row rather than a source row**, because both terms behind it are facts
    /// about the band's ledger: a quote for a rung nobody has started still has one, and picking a
    /// different kit re-prices the whole estimate off that kit's row. A source's own
    /// `build_work_from_gear` is the **resolved** contribution for the crew that worked it this
    /// turn — a different question, and not one a stepper can move. Appended (append-only).
    #[serde(default)]
    pub build_work_saturating_crew: u32,
}

/// The neutral value of [`BandKitTiersState`]'s three multipliers — `1.0`, never `0`.
///
/// Same reason `KitOptionState` spells its own out: `0` is the *reassuring* wrong answer. A
/// `dispersion 0` says nothing breaks off at contact and an `exposure 0` says nobody can be hurt, so
/// a field that failed to arrive would hand every band the passive device's whole advantage.
///
fn kit_multiplier_neutral() -> f32 {
    1.0
}

/// **WHAT THE RETIRED `build_rate` SLOT PUBLISHES** — its own neutral, on both kit tables.
///
/// The stat it carried was a **multiplier on the crew's build output** and is gone
/// (`docs/plan_unit_costed_work.md` §6): a multiplier cancels the job's cost, so it saved the same
/// *percentage* of turns on a garden and on a farm alike, which is the defect the arc exists to
/// close. Its successor is `build_work_per_worker` beside it — work units taken **off the job**.
///
/// **The slot is held at the neutral rather than `(deprecated)`**, because the FlatBuffers keyword
/// drops the accessor and the client's native reader still calls `buildRate()`. Publishing the
/// successor's number under the old name would be worse than publishing nothing: the client renders
/// it as a factor, so `8.5` would read as *"×8.5 build speed"*.
pub const RETIRED_BUILD_RATE: f32 = 1.0;

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
            build_rate: kit_multiplier_neutral(),
            build_work_per_worker: 0.0,
            build_work_saturating_crew: 0,
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
    ///
    /// **Sim-internal: these three do NOT cross the wire.** Their FlatBuffers slots are
    /// `(deprecated)`; what a client reads is the whole-people triple
    /// [`Self::children_count`] / [`Self::working_age`] / [`Self::elders_count`]. The fraction is a
    /// growth accumulator, and a client rounding it for itself disagreed with the sim's rounding.
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
    /// **Hunt/deny mission only: the target herd's species DISPLAY NAME** (`"Red Deer"`), resolved at
    /// launch and carried for the party's life. Empty for scout/normal bands.
    ///
    /// It exists because [`Self::expedition_target_herd`] alone is not enough to *name* the quarry:
    /// the herd list the client would join it against is fog-filtered and extinction-pruned, and a
    /// detached party is not a vision source, so a party's own target routinely leaves that list while
    /// the party is still bound to it — leaving the client nothing to render but the raw id (issue
    /// #378). The id stays the key commands address; this is the string the player reads.
    #[serde(default)]
    pub expedition_target_species: String,
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
    // **RETIRED: `expedition_carried_trade`** (arc #527). It was persistence-only — the scalar the
    // party banked its pelts on between kills — and a raid's non-food haul is now **material
    // batches** in the party's own `stores`, which the checkpoint carries whole. There is nothing
    // left for a rollback to silently zero.
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
    /// **What this band HAS, per rating** — one row per (material, band key) batch it holds. Empty
    /// for a band that has banked no material at all, which is a real answer.
    #[serde(default)]
    pub material_batches: Vec<MaterialBatchState>,
    /// **What is on this band's bench.** `recipe_id` empty = idle; a *blocked* bench has a recipe
    /// and a [`BenchState::blocked_reason`].
    #[serde(default)]
    pub bench: BenchState,
    /// **One row per recipe, always** — with the refusal already resolved into words. See
    /// [`CraftOfferState`]: `reason`/`severity` are the contract, not `available`.
    #[serde(default)]
    pub craft_offers: Vec<CraftOfferState>,
    /// **What this band owns of each item, and how much life is in it** — one row per batch, plus
    /// one `count: 0` row for every config item it owns none of, so the ledger is never missing a
    /// row. See [`EquipmentBatchState`] for why the life wording is in use quanta and not percent.
    #[serde(default)]
    pub equipment_batches: Vec<EquipmentBatchState>,
    /// **Whole children**, the derived half of the published age triple.
    ///
    /// **The wire carries whole people.** The fractional brackets above are an internal *growth
    /// accumulator* — they exist so a slow birth rate does not round to zero every turn — with
    /// exactly one correct reading, which is the sim's; they are no longer serialized. Together with
    /// [`Self::working_age`] (the whole workers, already shipped) and [`Self::elders_count`] this
    /// satisfies `children_count + working_age + elders_count == size`, by construction: `size` is
    /// written as that sum. Derived by `core_sim::snapshot::population::whole_age_brackets`.
    #[serde(default)]
    pub children_count: u32,
    /// **Whole elders** — the remainder of the dependents after [`Self::children_count`] takes its
    /// round-half share, so the triple sums exactly. A cohort with no dependent *mass* has no
    /// dependents at all: an elder rounded into existence is not a person.
    #[serde(default)]
    pub elders_count: u32,
    /// **How this band's gear divides its HUNT workers** — best-equipped first, `Σ workers ==` the
    /// hunt head count, and **never empty**: a uniformly-equipped band publishes exactly one row.
    /// See [`BandKitCrewState`].
    #[serde(default)]
    pub hunt_crews: Vec<BandKitCrewState>,
    /// **The band a trade party's shipment is bound for** — the `BandId` every command addresses it
    /// by, and a key the player never sees. `0` for every other mission and for a resident band.
    ///
    /// Its display twin is [`Self::expedition_destination_name`], on exactly the rule
    /// [`Self::expedition_target_herd`] and [`Self::expedition_target_species`] follow: the party
    /// outlives its target's presence in the viewer's world, so the name is resolved at launch and
    /// carried rather than joined against a live list.
    ///
    /// **There is no faction beside it.** Faction is a property of the endpoint, never a branch, so
    /// a shipment to your own splinter and a shipment to another people are the same row.
    #[serde(default)]
    pub expedition_destination_band: u64,
    /// **The destination band's display name**, resolved at launch — and **empty today, because
    /// bands have no names in this game.** Empty for every non-trade party too.
    ///
    /// **Empty means "no name", not "unknown"** — the *"empty is no row, never a zero"* contract
    /// this arc's material readouts use. The sim declines to guess, and a client renders whatever it
    /// already calls that band, joined on [`Self::expedition_destination_band`].
    ///
    /// It was briefly filled from the sending path's `StartingUnit.kind` — the unit *archetype*
    /// (`"BandForager"`), the same string for every seeded band — which made every in-flight row
    /// read *"Bound for BandForager"* and disagree with the label the rest of the HUD gives that
    /// same band. A wrong name is worse than none: none has a fallback.
    ///
    /// The field is not cosmetic. When a second faction lands (#513) a foreign band's name has to
    /// come from the sim, the client holding no roster to resolve one from; filling it means
    /// designing a band naming scheme, which is its own piece of work.
    #[serde(default)]
    pub expedition_destination_name: String,
    /// **The FOOD the shipment holds.** It is a *separate* store from the party's own pack (which
    /// rides `stores`), because a hungry party must not be able to eat the shipment it is hauling.
    #[serde(default)]
    pub expedition_cargo_food: f32,
    /// **The shipment's materials, one row per material id.** Reuses [`MaterialPayoff`] rather than
    /// minting a second table, and carries the same three contracts as every material readout in
    /// this arc: **never summed**, **empty is "no row" not zero**, **key always present**.
    ///
    /// The per-material amount is the total over the batches the party holds; the *ratings* are not
    /// flattened here — they ride the batches themselves, which move into the receiving band's store
    /// unaveraged.
    #[serde(default)]
    pub expedition_cargo_materials: Vec<MaterialPayoff>,
    /// **Food this band received from another band this turn** — supply-network balancing, a trade
    /// shipment landing, or an expedition of its own handing its pack back.
    ///
    /// With [`Self::transfer_sent`] it completes the food-ledger identity
    ///
    /// ```text
    /// larder_delta == food_income − food_consumption − pen_feed_upkeep − raid_forfeit
    ///                 + transfer_received − transfer_sent
    /// ```
    ///
    /// Food crossing between larders passes through neither `food_income` (what *this* band's
    /// workers produced) nor `food_consumption` (what its people ate) — the same hole
    /// `pen_feed_upkeep` and `raid_forfeit` were each minted for. **One pair for every producer**,
    /// because they are all one fact; **two magnitudes rather than a signed net**, because a band
    /// that both sends and receives in one turn is doing something.
    #[serde(default)]
    pub transfer_received: f32,
    /// **Food this band gave up to another band this turn** — the other half of
    /// [`Self::transfer_received`]. Its window is the *snapshot* window rather than the turn: a
    /// `send_trade_expedition` command debits the larder between two published frames.
    #[serde(default)]
    pub transfer_sent: f32,
    /// **One person's SHIPMENT pack** — `expedition_config.trade.per_worker_carry`, a global lever
    /// echoed onto every cohort (the `expedition_per_worker_carry` / `hunt_per_worker_provisions`
    /// idiom).
    ///
    /// **This is the one the outfit UI needs**, because the player prices a manifest for a party
    /// that does not exist yet: the cap is `party_workers × this`, and `party_workers` is what the
    /// stepper is choosing. A party already on the map publishes its own pack as
    /// [`Self::expedition_carry_cap`].
    ///
    /// **It is not [`Self::expedition_per_worker_carry`]**, which is the *hunt* pack. Two packs, two
    /// levers; a client composing a trade cap from the raid's is one config edit away from quoting a
    /// cap the launch command will refuse.
    ///
    /// **Always positive** — the lever is validated `> 0` at load, and a `0` would let a client
    /// render a zero cap and refuse every manifest.
    #[serde(default)]
    pub expedition_trade_per_worker_carry: f32,
    /// **What one unit of a material costs in shipment pack space, relative to one unit of food** —
    /// `expedition_config.trade.material_carry_weight`, the other half of a shipment's mass and the
    /// same every-cohort lever echo as [`Self::expedition_trade_per_worker_carry`].
    ///
    /// Together they give a client the sim's own expression:
    ///
    /// ```text
    /// mass = expedition_cargo_food + this × Σ material amounts
    /// cap  = party_workers × expedition_trade_per_worker_carry
    /// ```
    ///
    /// **It ships because the sim otherwise refuses a manifest on a rule the client cannot
    /// evaluate.** The launch refusal is unchanged and remains the authority; without this lever the
    /// cargo picker is a guessing game the player only loses on submit.
    ///
    /// **Finite and `>= 0`, not positive** — `0` is a legitimate setting ("materials are
    /// weightless"), unlike the pack lever beside it.
    #[serde(default)]
    pub expedition_trade_material_carry_weight: f32,
    /// **Food this band received from another band ON THIS TURN** — the per-turn twin of
    /// [`Self::transfer_received`], and **the reading a panel renders**.
    ///
    /// The two answer different questions. [`Self::transfer_received`] covers the whole publication
    /// window (command-time draws included) and is **cleared once the turn's capture reads it**,
    /// which is the window the ledger identity closes over. This one is per-turn state on the cohort
    /// and is not cleared, so it survives a **recapture** — the sim re-runs its capture against live
    /// components after every dispatched command, and on such a refreshed frame the accumulating
    /// pair reads `0.0` while the four sibling terms (`food_income`, `food_consumption`,
    /// `pen_feed_upkeep`, `raid_forfeit`) re-read unchanged.
    ///
    /// **On a turn frame the two agree** — the copy is taken immediately before that capture, off
    /// the same counter.
    #[serde(default)]
    pub transfer_received_turn: f32,
    /// **Food this band gave up to another band on this turn** — the sent half of
    /// [`Self::transfer_received_turn`], on the same copy and for the same reason.
    #[serde(default)]
    pub transfer_sent_turn: f32,
}

/// **One run of a band's hunt workers holding the same gear** — a row of
/// [`PopulationCohortState::hunt_crews`], and the sim's own answer rather than an input to a
/// client-side derivation.
///
/// # The hunt gate is why one number per band is not enough
///
/// `max(0, hunter_attack − defense)` decides whether a species can be taken **at all**. A band with
/// ten spears and seventeen hunters takes a Red Deer with ten of them and not with the other seven,
/// and [`PopulationCohortState::hunter_attack`] — the *best-equipped* crew's tier — says only the
/// reassuring half of that. This is the division `EquipmentConfig::coverage` already resolved for
/// the take.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BandKitCrewState {
    /// How many of the band's hunters are in this run. Fractional, because a forecast's head counts
    /// are.
    pub workers: f32,
    /// This run's own `attack` tier — **flat, never a blend of two tiers**.
    pub hunter_attack: f32,
    /// What this run is holding, by `equipment.json` item id. Empty for a bare-handed run, which is
    /// an ordinary crew and not a sentinel.
    pub item_ids: Vec<String>,
}

/// **One axis of one batch — the exact reading AND the band it falls in.**
///
/// Both, deliberately. The band is the merge key and the panel's word; the **exact** value is what
/// crafting reads, so two `good` hides are not interchangeable and a client with only the band could
/// not explain why one pile made a fine sled and another a standard one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CharacteristicReadingState {
    /// The material's own axis id — `toughness`, `suppleness`, `fineness`, …
    pub axis: String,
    /// The batch's **exact** amount-weighted average on this axis, `0..1`.
    pub value: f32,
    /// The `characteristic_bands` rung [`Self::value`] falls in — `poor` / `fair` / `good` /
    /// `excellent`. **It rates the AXIS, not the material.**
    pub band_name: String,
}

/// **One pile of one material at one rating** — a row of
/// [`PopulationCohortState::material_batches`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MaterialBatchState {
    /// The `materials.json` id. **Generic** — `hide`, never `deer_hide`.
    pub material_id: String,
    pub amount: f32,
    pub readings: Vec<CharacteristicReadingState>,
    /// The nearest declared **variety** of this material, or `""` when it declares none — which is
    /// every shipped material. Varieties are naming, not materials.
    pub variety_name: String,
}

/// **What a draw is short, as a number.** The panel says *"Short 4.9 bone"*, never *"cannot craft"*,
/// so the arithmetic is done sim-side and never re-derived.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MaterialShortfallState {
    pub material_id: String,
    /// The recipe's stated amount **after** the bench tool's material efficiency.
    pub required: f32,
    /// What the band's store holds, summed over its batches.
    pub held: f32,
    /// `required − held`. Always `> 0` on a published row.
    pub short: f32,
}

/// **What is on a band's bench** — one job at a time, so no surface has to explain a queue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BenchState {
    /// `""` = an idle bench, which is a *different* state from a blocked one.
    pub recipe_id: String,
    pub display_name: String,
    pub workers: u32,
    /// Worker-turns accrued toward [`Self::work`].
    pub progress: f32,
    /// Worker-turns one pass of this recipe costs.
    pub work: f32,
    /// The craft id one completed item credits — crafting is the fourth teacher, and **what is being
    /// made decides what is learned**.
    pub teaches: String,
    /// **Why this bench is not moving**, resolved sim-side. `""` = it is working. Same vocabulary as
    /// [`CraftOfferState::reason`], plus the crew's own refusal: a bench with a full pile and nobody
    /// on it is also stopped.
    pub blocked_reason: String,
    pub shortfalls: Vec<MaterialShortfallState>,
    /// Items finished on **this** job — the same count the tool's wear and the craft's lesson were
    /// charged, so a readout of one is a readout of the others.
    pub items_completed: u32,
    /// The pile for the pass in flight is already cut. A short draw takes **nothing**, so this stays
    /// `false` rather than leaving a half-spent pile.
    pub drawn: bool,
    /// The grade that pile **fixed** — `""` before the draw or on a recipe that reads nothing. It
    /// never moves once set: a tool running dry mid-craft does not retroactively coarsen the thing
    /// on the bench.
    pub output_grade: String,
    /// **What this bench will accrue next turn** — `workers × progress_per_worker_turn ×
    /// craft_speed`, already resolved through the bounding tool (or the material's bare-handed rate,
    /// which is what makes a *worker-turn* not a worker's turn). `0` when nothing will accrue: no
    /// crew, no recipe, or a craft speed of zero.
    ///
    /// **A client must not re-derive it** — `craft_speed` is the tool-or-bare-hand join, the same
    /// one `kitTiers` exists to keep sim-side. It is a **term**, not an answer: the finish estimate
    /// is `ceil((work − progress) / rate_per_turn)`, exact arithmetic over three numbers all on this
    /// wire, and a turns-remaining field would be a second home for one fact. Appended last
    /// (append-only).
    #[serde(default)]
    pub rate_per_turn: f32,
    /// **The pile already cut for the job in flight**, so a clear or a swap can name what it
    /// destroys. Empty when nothing is drawn. One row per input material, in the recipe's own input
    /// order. Appended last (append-only).
    #[serde(default)]
    pub drawn_inputs: Vec<DrawnInputState>,
    /// **How [`Self::blocked_reason`] should read** — the same vocabulary as
    /// [`CraftOfferState::severity`] (`danger` / `neutral` / `good`), `""` whenever nothing is
    /// blocking.
    ///
    /// **A bench waiting for its crew is a prompt, not a fault.** The player staffs the bench, so
    /// *"No one at the bench"* is the normal state one click after **Make** — an instruction,
    /// `neutral`. A material shortage, an unknown craft or a zero craft rate are problems, `danger`;
    /// joined reasons take `danger` if any component does. Without the field a client renders every
    /// reason in one alarm colour and the expected state reads as an error — the same distinction
    /// `reason`/`severity` already draw on an offer row, and **a client must not re-derive it**.
    /// Appended last (append-only).
    #[serde(default)]
    pub blocked_severity: String,
}

/// **One material of the pile a bench has already withdrawn** for the item in flight — a row of
/// [`BenchState::drawn_inputs`].
///
/// It is the **withdrawn** amount, not the recipe's stated input and not a shortfall's `required`:
/// those differ once a bench tool's material efficiency applies, and the readout's whole job is to
/// name what will really be lost when the job is cleared.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DrawnInputState {
    /// The `materials.json` id. **Generic** — `hide`, never `deer_hide`.
    pub material_id: String,
    pub amount: f32,
}

/// **One recipe, offered or refused** — a row of [`PopulationCohortState::craft_offers`], and there
/// is one for **every** recipe in the book, always.
///
/// # `reason` and `severity` are the contract, not `available`
///
/// *"Not needed yet"* is a **shrug** and a shortage is a **problem**. They are different strings and
/// different severities on this wire precisely because a client deriving both from a boolean cannot
/// tell them apart. Render [`Self::reason`] verbatim; never substitute *"cannot craft"*, and never
/// re-derive a reason, a shortfall or a grade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CraftOfferState {
    pub recipe_id: String,
    pub display_name: String,
    /// `kit` (a party carries it) | `tool` (it bounds one material at the bench) | `stock` (it makes
    /// a material) — the three groups the design's ledger is drawn in.
    pub group: String,
    /// The equipment id this recipe makes, `""` for a material recipe. The join key from an offer
    /// row to its [`EquipmentBatchState`] rows.
    pub output_item_id: String,
    /// Would a pass make progress **right now**: the craft is known, the bench rate is non-zero, and
    /// the pile is there.
    pub available: bool,
    /// The resolved refusal or invitation. See the type docs.
    pub reason: String,
    /// `danger` | `neutral` | `good`.
    pub severity: String,
    pub shortfalls: Vec<MaterialShortfallState>,
    /// **The grade the draw would select** out of the band's stock right now, after the tool's
    /// quality ceiling. It is a `characteristic_bands` **name** — `poor` / `fair` / `good` /
    /// `excellent` — because there is one quality ladder for the whole game. `""` on a recipe that
    /// reads no characteristic.
    pub output_grade: String,
    /// This recipe is the running job — the row's button is spent (*"On the bench"*).
    pub on_bench: bool,
    /// **The tier a craft would produce right now** — `ItemDefinition::craftable_tier`, the best tier
    /// the faction knows. It is the ledger's **group head**, not a column: a head says *flint* once
    /// and can fold away, which is what a column spending its width on every row can never do. `""`
    /// on a material (stock) recipe.
    pub output_tier_name: String,
    /// Index of that tier within the item's own `tiers` list. **Heads order by rank descending** —
    /// newest first — because there is no other honest ordering for two tier heads and alphabetical
    /// would put Iron above Bronze.
    pub output_tier_rank: u32,
    /// **What the band CARRIES, said only when it disagrees with what it could now make.** `""` when
    /// there is no news, which is every row on the shipped one-tier roster. *"carrying flint ·
    /// poor"*, *"last flint set wore out"* — **render it verbatim**; the tier word reaches the Owned
    /// cell only through this field and only when it is news.
    pub owned_note: String,
}

/// **One batch of one item a band owns**, plus a `count: 0` row for every config item it owns none
/// of — so the ledger always has a row and an item can never simply go missing from it.
///
/// # The life meter is a fuel gauge, not a performance meter
///
/// A spear at 34% is exactly as deadly as one at 100%, so [`Self::life`] reads in the item's **own
/// use quanta** and never in percent — a single percentage bar would draw a taper this model does
/// not have. The quantum's noun is resolved sim-side off `wear.per`; **the client must not map
/// quanta to English**.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EquipmentBatchState {
    pub item_id: String,
    /// The `EquipmentTier` these units were made at; `""` when the band owns none.
    pub tier_id: String,
    /// The craft grade; `""` for a start-stocked unit — a shipped kit has a tier but was never on
    /// anyone's bench.
    pub grade: String,
    /// Whole units in this batch. **`0` = the band owns none of this item at all.**
    pub count: u32,
    /// Condition left on the unit **in hand**, `0..100`. `0` when [`Self::count`] is `0`.
    pub remaining: f32,
    /// Uses of this item's own quantum this batch still has in it.
    pub quanta_left: f32,
    /// The plural noun [`Self::quanta_left`] is counted in — `kills`, `raids`, `biomass hauled`, …
    pub quantum_noun: String,
    /// The row's wording, resolved: `Untouched` | `48 raids left` | `~1 raid left` | `Worn out` |
    /// `Never made`. **`Worn out` and `Never made` are different states** and this is where they are
    /// told apart — both read `count 0`, because a batch that runs out of units is removed.
    pub life: String,
    /// `healthy` | `warn` | `danger`.
    pub life_severity: String,
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
