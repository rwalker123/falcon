use super::*;
use crate::components::FertilityFactors;
use crate::demographics_config::{DemographicsBirths, DemographicsTrend};

#[derive(Event, Debug, Clone)]
pub struct MigrationKnowledgeEvent {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
}

/// A cohort's age brackets + food larder at the start of a demographic turn.
#[derive(Debug, Clone, Copy)]
struct DemographicState {
    children: Scalar,
    working: Scalar,
    elders: Scalar,
    food_store: Scalar,
}

/// One turn's food demand for the given age brackets: per-capita draw × weighted mouths
/// (dependents eat less than a working adult). Shared by consumption and the campaign-start
/// larder seeding so they can never drift apart.
pub(crate) fn food_demand(
    children: Scalar,
    working: Scalar,
    elders: Scalar,
    consumption: &DemographicsConsumption,
) -> Scalar {
    let weighted_mouths = children * scalar_from_f32(consumption.child_factor)
        + working * scalar_from_f32(consumption.working_factor)
        + elders * scalar_from_f32(consumption.elder_factor);
    scalar_from_f32(consumption.per_capita_draw) * weighted_mouths
}

/// The starvation half of a bracket's per-turn death fraction: it scales with the food
/// `deficit_fraction` and the bracket's vulnerability but is **never allowed to exceed the deficit
/// itself** — a 10% food shortfall impacts at most 10% of the bracket.
fn starvation_fraction(
    deficit_fraction: Scalar,
    starvation_rate: Scalar,
    vulnerability: f32,
) -> Scalar {
    min(
        deficit_fraction * starvation_rate * scalar_from_f32(vulnerability),
        deficit_fraction,
    )
}

/// Combined per-turn death fraction for one age bracket: a starvation term plus a uniform cold
/// term, capped at 1.0. Cold is a separate, non-food mortality.
fn death_fraction(
    deficit_fraction: Scalar,
    starvation_rate: Scalar,
    vulnerability: f32,
    cold_fraction: Scalar,
) -> Scalar {
    min(
        starvation_fraction(deficit_fraction, starvation_rate, vulnerability) + cold_fraction,
        scalar_one(),
    )
}

/// Which term did most of this bracket's killing. The terms are compared as **per-capita
/// fractions**, which is the only form in which they are commensurable — each is the share of the
/// bracket that term removes this turn. Ties go to [`DeathCause::Hunger`] — a band starving *and*
/// freezing is a food problem the player can act on.
///
/// `age` is the flat `elder_mortality_rate`, and is `0` for every bracket but the elders: children
/// and workers have no old-age term at all. It is passed rather than looked up so this stays the
/// one place a cause is decided.
///
/// Resolved on the turn the deaths happen, because nothing afterwards can answer it: the post-turn
/// brackets carry no record of which term emptied them.
fn dominant_death_cause(starvation: Scalar, cold: Scalar, age: Scalar) -> DeathCause {
    let mut cause = DeathCause::Hunger;
    let mut worst = starvation;
    if cold > worst {
        cause = DeathCause::Cold;
        worst = cold;
    }
    if age > worst {
        cause = DeathCause::Age;
    }
    cause
}

/// A band's per-turn food **flow**, as of last turn's resolution — the input to the `trend`
/// fertility factor. Read off `LaborAllocation`, which resolves *after* `simulate_population`, so
/// the values are one turn stale by construction. That is correct: fertility should respond to the
/// trend a band has been living, not to a single turn's haul.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FoodFlow {
    /// Σ per-source [`SourceYield::realized`] — the **steady**, forward-projected food/turn, not the
    /// lumpy `actual`. A big-game hunt pays zero for six turns then spikes; fertility must not
    /// sawtooth with whole-animal timing.
    pub steady_income: Scalar,
    /// What the band's pens ate (`LaborAllocation::last_pen_feed_upkeep`). Subtracting it is what
    /// makes `net_flow` the negation of the same net drain the player-facing `turnsOfFood` runway
    /// divides by, so the two readouts can never disagree about which way a band is heading.
    pub pen_feed_upkeep: Scalar,
}

/// One turn of [`advance_demographics`]: the resolved bracket/larder state **plus the fertility
/// factors that produced its births**. The factors are returned rather than recomputed at capture
/// deliberately — they are resolved from the turn's *opening* brackets and *pre-meal* larder, so a
/// re-derivation on the post-turn state would report numbers that never drove a birth.
struct DemographicOutcome {
    pub state: DemographicState,
    pub fertility: FertilityFactors,
    pub flows: DemographicFlows,
}

/// The per-turn **flows** the bracket update is made of, returned instead of discarded.
///
/// `advance_demographics` applies births, maturation and the death terms and hands back only the
/// resulting brackets, so a band that lost two elders to cold and gained a child looks exactly like
/// a band that did neither — the demographic events the player reads are precisely these numbers.
/// Each is a `Scalar` because a band of thirty earns fractions of a person per turn; the whole
/// people are extracted by [`DemographicFlowAccumulator`], not here.
///
/// **Every term that removes a person from a bracket is here.** A death the model resolves but does
/// not route through this struct is a person the game never mentions losing — which is exactly what
/// `elder_mortality` was until it joined `elder_deaths`. `demographic_events`' ledger guard is what
/// makes that class of omission fail rather than ship: it closes over births, deaths, migration and
/// the carries, so an unrouted term shows up as residue no per-case assertion would catch.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct DemographicFlows {
    pub births: Scalar,
    pub maturations: Scalar,
    /// Working→elder transitions. The one flow here that **moves** a person rather than adding or
    /// removing one, so it is outside the ledger's head-count identity — but it is carried and
    /// reported like the rest, because a band whose workforce quietly drains into the elder bracket
    /// is the same silent transition `maturations` already fixed at the young end.
    pub agings: Scalar,
    pub child_deaths: Scalar,
    pub working_deaths: Scalar,
    /// Starvation + cold **plus** the flat old-age term (`elder_mortality_rate`). One number,
    /// because they are one thing to the player: elders the band lost this turn. Which of them did
    /// the most killing is `elder_death_cause`'s job, not a second flow's.
    pub elder_deaths: Scalar,
    /// The dominant cause behind each bracket's deaths **this turn**, meaningful only where the
    /// matching flow is positive.
    pub child_death_cause: DeathCause,
    pub working_death_cause: DeathCause,
    pub elder_death_cause: DeathCause,
}

/// The **flow** fertility factor. `None` flow means *not projected* — a band with no
/// `LaborAllocation` at all, or one whose `last_yields` no turn has written yet — and must read as
/// **no data → neutral**, never as zero income. Reading absent telemetry as a famine would suppress
/// births on a band that simply has not been resolved yet; this is the same trap already documented
/// for the arrivals schedule in `larder_runway_turns`.
fn trend_factor(flow: Option<FoodFlow>, demand: Scalar, cfg: &DemographicsTrend) -> Scalar {
    let Some(flow) = flow else {
        return scalar_one();
    };
    if demand <= scalar_zero() {
        return scalar_one();
    }
    let net_ratio = (flow.steady_income - demand - flow.pen_feed_upkeep) / demand;
    // A saturation of 0 would divide by zero; treat it as "any excursion is already full scale".
    let ramp = |excursion: Scalar, saturation: f32| {
        let saturation = scalar_from_f32(saturation);
        if saturation > scalar_zero() {
            min(excursion / saturation, scalar_one())
        } else {
            scalar_one()
        }
    };
    if net_ratio >= scalar_zero() {
        scalar_one() + scalar_from_f32(cfg.surplus_gain) * ramp(net_ratio, cfg.surplus_saturation)
    } else {
        // `net_ratio` has NO lower bound — `pen_feed_upkeep` is subtracted too, so a band whose pens
        // out-eat its income goes past -1. What caps the penalty is `ramp`'s own `min(.., 1)`, never a
        // floor on income: do not remove that clamp. The `max(.., 0)` below is belt-and-braces against
        // a config with `deficit_penalty > 1`.
        max(
            scalar_one()
                - scalar_from_f32(cfg.deficit_penalty) * ramp(-net_ratio, cfg.deficit_saturation),
            scalar_zero(),
        )
    }
}

/// Resolve the three fertility factors for a cohort's food position. Pure and shared by the birth
/// path and its tests so the model has exactly one definition.
fn fertility_factors(
    demand: Scalar,
    consumed: Scalar,
    larder_after_meal: Scalar,
    flow: Option<FoodFlow>,
    births: &DemographicsBirths,
) -> FertilityFactors {
    let has_demand = demand > scalar_zero();
    let hunger = if has_demand {
        consumed / demand
    } else {
        scalar_one()
    };
    let reserve_turns = if has_demand {
        larder_after_meal / demand
    } else {
        scalar_zero()
    };
    let saturation = scalar_from_f32(births.reserve.saturation_turns);
    let reserve_ramp = if saturation > scalar_zero() {
        min(reserve_turns / saturation, scalar_one())
    } else {
        scalar_one()
    };
    FertilityFactors {
        hunger,
        reserve: scalar_one() + scalar_from_f32(births.reserve.bonus) * reserve_ramp,
        trend: trend_factor(flow, demand, &births.trend),
    }
}

/// Read a band's food flow off last turn's labor telemetry, distinguishing **no data** from a
/// genuine zero. An empty `LaborAllocation::last_yields` is ambiguous, and the two readings are
/// opposite:
///
/// - **no `LaborAllocation`**, or **assignments staffed but `last_yields` empty** (telemetry no
///   `advance_labor_allocation` has written yet) → `None`, *not projected*. The trend factor stays
///   neutral; reading this as zero income would suppress births on a band that has not resolved yet.
/// - **`assignments` empty** → `Some` with zero income. An idle band really does produce nothing,
///   and that emptiness is a fact about the band, not missing telemetry.
fn band_food_flow(labor: Option<&LaborAllocation>) -> Option<FoodFlow> {
    let labor = labor?;
    if labor.last_yields.is_empty() && !labor.assignments.is_empty() {
        return None;
    }
    Some(FoodFlow {
        steady_income: scalar_from_f32(labor.last_yields.iter().map(|y| y.realized).sum::<f32>()),
        pen_feed_upkeep: scalar_from_f32(labor.last_pen_feed_upkeep),
    })
}

/// One turn of the demographic model for a single cohort (pure — no ECS): draw per-capita food
/// from the local larder, then resolve scarcity/cold deaths, births, maturation, aging, and
/// elder mortality. All bracket flows use the *opening* bracket values and are applied together,
/// so a newborn does not mature the same turn. The total is clamped to the global cap.
fn advance_demographics(
    state: DemographicState,
    flow: Option<FoodFlow>,
    temp_diff: Scalar,
    max_cap: Scalar,
    demo: &DemographicsConfig,
) -> DemographicOutcome {
    let DemographicState {
        children: children0,
        working: working0,
        elders: elders0,
        food_store,
    } = state;

    // 1. Food consumption from the band's own larder (dependents eat less than a worker).
    let demand = food_demand(children0, working0, elders0, &demo.consumption);
    let consumed = min(demand, food_store);
    let remaining_food = food_store - consumed;
    let has_demand = demand > scalar_zero();
    let deficit = demand - consumed; // >= 0 (consumed <= demand)
    let deficit_fraction = if has_demand {
        deficit / demand
    } else {
        scalar_zero()
    };

    // 2. Deaths: starvation (scales with the food deficit, dependents more vulnerable, but never
    // more than the deficit itself) plus cold (temperature deviation beyond tolerance).
    let scarcity = &demo.scarcity;
    let starvation_rate = scalar_from_f32(scarcity.starvation_mortality);
    let cold = &demo.cold;
    let cold_excess = temp_diff - scalar_from_f32(cold.temp_tolerance);
    let cold_fraction = if cold_excess > scalar_zero() {
        min(
            cold_excess * scalar_from_f32(cold.mortality_scale),
            scalar_from_f32(cold.max_mortality),
        )
    } else {
        scalar_zero()
    };
    let child_starvation = starvation_fraction(
        deficit_fraction,
        starvation_rate,
        scarcity.child_vulnerability,
    );
    let working_starvation = starvation_fraction(
        deficit_fraction,
        starvation_rate,
        scarcity.working_vulnerability,
    );
    let elder_starvation = starvation_fraction(
        deficit_fraction,
        starvation_rate,
        scarcity.elder_vulnerability,
    );
    let child_deaths = children0
        * death_fraction(
            deficit_fraction,
            starvation_rate,
            scarcity.child_vulnerability,
            cold_fraction,
        );
    let working_deaths = working0
        * death_fraction(
            deficit_fraction,
            starvation_rate,
            scarcity.working_vulnerability,
            cold_fraction,
        );
    let elder_deaths = elders0
        * death_fraction(
            deficit_fraction,
            starvation_rate,
            scarcity.elder_vulnerability,
            cold_fraction,
        );

    // 3. Births → children, from the working (reproductive) bracket. Fertility is a product of three
    // named factors — `hunger` (did we eat), `reserve` (is there a cushion), `trend` (is the cushion
    // growing or shrinking) — so a band bleeding out no longer breeds at full speed right up to the
    // cliff. See `docs/plan_population_growth_model.md` and `fertility_factors`.
    // Births are morale-INDEPENDENT (wellbeing model, `docs/plan_civ_wellbeing.md`): contentment
    // doesn't change procreation — low morale relocates people or drags output, it never suppresses
    // births or causes faction population loss.
    let births_cfg = &demo.births;
    let factors = fertility_factors(demand, consumed, remaining_food, flow, births_cfg);
    let fertility = scalar_from_f32(births_cfg.birth_rate) * factors.multiplier();
    let births = working0 * fertility;

    // 4. Aging flows. `maturation` and `aging` are the two ends of a working life and both ride out
    // in the flows; `elder_mortality` is a **death**, not a transition — it is the flat rate at
    // which elders simply grow too old, and it is reported alongside the elders' starvation/cold
    // term in `flows.elder_deaths`. In a fed band in fair weather it is the only mortality there
    // is, so leaving it out of the flows made a healthy band shrink in total silence.
    let maturation = children0 * scalar_from_f32(demo.maturation_rate);
    let aging = working0 * scalar_from_f32(demo.aging_rate);
    let elder_mortality_rate = scalar_from_f32(demo.elder_mortality_rate);
    let elder_mortality = elders0 * elder_mortality_rate;

    // Apply all flows simultaneously, flooring each bracket at zero.
    let mut children = max(
        children0 + births - maturation - child_deaths,
        scalar_zero(),
    );
    let mut working = max(
        working0 + maturation - aging - working_deaths,
        scalar_zero(),
    );
    let mut elders = max(
        elders0 + aging - elder_mortality - elder_deaths,
        scalar_zero(),
    );

    // Aggregate safety clamp to the global population cap.
    let total = children + working + elders;
    if total > max_cap && total > scalar_zero() {
        let scale = max_cap / total;
        children *= scale;
        working *= scale;
        elders *= scale;
    }

    DemographicOutcome {
        state: DemographicState {
            children,
            working,
            elders,
            food_store: remaining_food,
        },
        fertility: factors,
        // Reported **pre-clamp**: the population cap is an aggregate safety net that never fires
        // under shipped tuning, and the flows are what the turn's model actually resolved.
        flows: DemographicFlows {
            births,
            maturations: maturation,
            agings: aging,
            child_deaths,
            working_deaths,
            // Old age is one of the ways an elder dies, so it rides in the same flow rather than a
            // parallel one the accumulator would have to learn about.
            elder_deaths: elder_deaths + elder_mortality,
            child_death_cause: dominant_death_cause(child_starvation, cold_fraction, scalar_zero()),
            working_death_cause: dominant_death_cause(
                working_starvation,
                cold_fraction,
                scalar_zero(),
            ),
            elder_death_cause: dominant_death_cause(
                elder_starvation,
                cold_fraction,
                elder_mortality_rate,
            ),
        },
    }
}

/// Config levers for [`tile_morale_pressure`] — the place-based (negative) morale terms. Pulled
/// from `SimulationConfig` (temperature) and the population block of `turn_pipeline_config.json`
/// (terrain scales) so the sim and the snapshot's `habitability` read from one source.
pub struct MoralePressureConfig {
    pub ambient_temperature: Scalar,
    pub temperature_morale_penalty: Scalar,
    /// Dead-band (°) around `ambient_temperature` within which climate bleeds **no** morale — only
    /// the deviation beyond it is penalized, so temperate mid-latitudes hold morale.
    pub temperature_morale_tolerance: Scalar,
    pub attrition_penalty_scale: Scalar,
    pub hardness_penalty_scale: Scalar,
}

/// The tile-intrinsic, per-turn morale *drain* broken into its two place-based drivers (each ≥ 0;
/// bigger = worse). This is the "how harsh is it to live on this tile" signal — it excludes base
/// growth and crisis/sentiment (unrest), which are not properties of the place.
pub struct TileMoralePressure {
    /// Terrain attrition + logistics-hardness drain.
    pub terrain: Scalar,
    /// Temperature-difference (comfort) drain.
    pub cold: Scalar,
}

impl TileMoralePressure {
    /// Total tile-intrinsic morale drain (`terrain + cold`, ≥ 0). This is the snapshot's
    /// `habitability` value.
    pub fn total(&self) -> Scalar {
        self.terrain + self.cold
    }
}

/// Compute the tile-intrinsic per-turn morale drain for a tile's terrain + temperature. Shared by
/// `simulate_population` (for the actual morale update + dominant-cause attribution) and the
/// snapshot's `habitability` export so the two never drift.
pub fn tile_morale_pressure(
    terrain: &TerrainDefinition,
    temperature: Scalar,
    cfg: &MoralePressureConfig,
) -> TileMoralePressure {
    let terrain_attrition_penalty =
        scalar_from_f32(terrain.attrition_rate) * cfg.attrition_penalty_scale;
    let hardness_excess = (terrain.logistics_penalty - 1.0).max(0.0);
    let terrain_hardness_penalty = scalar_from_f32(hardness_excess) * cfg.hardness_penalty_scale;
    let temp_diff = (temperature - cfg.ambient_temperature).abs();
    let temp_excess = (temp_diff - cfg.temperature_morale_tolerance).max(scalar_zero());
    TileMoralePressure {
        terrain: terrain_attrition_penalty + terrain_hardness_penalty,
        cold: temp_excess * cfg.temperature_morale_penalty,
    }
}

/// Layer 2 (wellbeing) — map a band's morale to its discontented share. `0` at/above
/// `content_morale`, rising linearly to `1` at/below `floor_morale`. See
/// `docs/plan_civ_wellbeing.md`.
pub fn discontent_fraction(
    morale: Scalar,
    cfg: &crate::wellbeing_config::DiscontentConfig,
) -> Scalar {
    let content = scalar_from_f32(cfg.content_morale);
    let floor = scalar_from_f32(cfg.floor_morale);
    let span = content - floor;
    if span <= scalar_zero() {
        return scalar_zero();
    }
    ((content - morale) / span).clamp(scalar_zero(), scalar_one())
}

/// Layer 3a (wellbeing) — the discontent entry of the productivity modifier stack:
/// `max(floor_mult, 1 − discontent_fraction × discontent_weight)`. A fully-discontented band still
/// produces `floor_mult` of its base output (morale drags labor, never zeroes it).
pub fn discontent_output_modifier(discontent_fraction: Scalar, cfg: &ProductivityConfig) -> Scalar {
    (scalar_one() - discontent_fraction * scalar_from_f32(cfg.discontent_weight))
        .max(scalar_from_f32(cfg.floor_mult))
}

/// Layer 3a (wellbeing) — the band's output multiplier: the **product** of every active
/// productivity modifier (`output = base × Π(modifiers)`). Phase 1 has one entry (discontent);
/// future education / technology / government modifiers multiply in here with a one-line addition,
/// so every yield site (forage/hunt/follow/husbandry) stays a single `output_multiplier` call.
pub fn output_multiplier(cohort: &PopulationCohort, cfg: &WellbeingConfig) -> Scalar {
    let mut m = scalar_one();
    m *= discontent_output_modifier(cohort.discontent_fraction, &cfg.productivity);
    // future: education, technology, government modifiers multiply in here.
    m
}

/// Layer 3b (wellbeing) — migration's morale-scaled move fraction (decoupled from
/// `discontent_fraction`, which is productivity-only): `max_rate × clamp((morale_threshold − morale)
/// / morale_threshold, 0, 1)`. `0` at morale ≥ `morale_threshold` (0.25), ramping to `max_rate`
/// (0.15) at rock-bottom morale. The band sheds `total × move_fraction` people this turn.
pub fn migration_move_fraction(
    morale: Scalar,
    cfg: &crate::wellbeing_config::MigrationConfig,
) -> Scalar {
    let threshold = scalar_from_f32(cfg.morale_threshold);
    if threshold <= scalar_zero() {
        return scalar_zero();
    }
    let ramp = ((threshold - morale) / threshold).clamp(scalar_zero(), scalar_one());
    scalar_from_f32(cfg.max_rate) * ramp
}

/// How a band is named in a world event's label.
///
/// The snapshot carries no band *name* — the client renders a positional "Band N"
/// (`HudFormat.band_display_name`) — so the sim names the band's durable id and every event also
/// carries it as a `band=` detail token, which is what lets the client re-label the row with
/// whatever it calls that band.
fn band_label(band: BandId) -> String {
    format!("Band {}", band.0)
}

/// The three age brackets as they appear in a `died` event: the `bracket=` token, the singular noun
/// with its article, and the plural noun.
#[derive(Debug, Clone, Copy)]
struct DeathBracket {
    token: &'static str,
    singular: &'static str,
    plural: &'static str,
}

const CHILD_BRACKET: DeathBracket = DeathBracket {
    token: "child",
    singular: "A child",
    plural: "children",
};
const WORKING_BRACKET: DeathBracket = DeathBracket {
    token: "working",
    singular: "A worker",
    plural: "workers",
};
const ELDER_BRACKET: DeathBracket = DeathBracket {
    token: "elder",
    singular: "An elder",
    plural: "elders",
};

/// Accrue one turn's demographic flows and push a feed event for each that crossed a whole person.
///
/// Every event names a **count** rather than firing once per person: three elders lost to one cold
/// snap are one line, not three. Deaths pool onto a single carry, so the **count is exact** across
/// brackets while `bracket=`/`cause=` name the dominant contributor to that crossing — an
/// attribution, not a claim that every one of them was an elder.
fn push_demographic_events(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: FactionId,
    band: BandId,
    accumulator: &mut DemographicFlowAccumulator,
    flows: &DemographicFlows,
) {
    let name = band_label(band);

    let born = DemographicFlowAccumulator::accrue(&mut accumulator.births, flows.births);
    if born > 0 {
        let label = if born == 1 {
            format!("A child was born in {name}")
        } else {
            format!("{born} children were born in {name}")
        };
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Born,
            faction,
            label,
            Some(format!("band={} count={}", band.0, born)),
        ));
    }

    let matured =
        DemographicFlowAccumulator::accrue(&mut accumulator.maturations, flows.maturations);
    if matured > 0 {
        let label = if matured == 1 {
            format!("A child came of age in {name}")
        } else {
            format!("{matured} children came of age in {name}")
        };
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::CameOfAge,
            faction,
            label,
            Some(format!("band={} count={}", band.0, matured)),
        ));
    }

    // The other end of a working life. No head-count moves — the person is still in the band — but
    // a pair of hands left the workforce, so it accrues and reports exactly like `maturations`.
    let aged = DemographicFlowAccumulator::accrue(&mut accumulator.agings, flows.agings);
    if aged > 0 {
        let label = if aged == 1 {
            format!("A worker joined the elders in {name}")
        } else {
            format!("{aged} workers joined the elders in {name}")
        };
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Aged,
            faction,
            label,
            Some(format!("band={} count={}", band.0, aged)),
        ));
    }

    // Deaths accrue on ONE carry across all three brackets, so every whole person the band loses is
    // announced exactly once however the loss was spread — see `DemographicFlowAccumulator::deaths`
    // for why three carries strand a remainder the moment a bracket's flow stops.
    //
    // The per-bracket contribution and cause are stamped whenever this turn killed anyone in that
    // bracket, and read at the crossing to LABEL the event. A turn with no deaths in a bracket
    // leaves both alone, so a crossing names the term that actually did the killing rather than
    // whatever the last (quiet) turn happened to compute.
    let brackets = [
        (
            CHILD_BRACKET,
            flows.child_deaths,
            flows.child_death_cause,
            &mut accumulator.child_death_contribution,
            &mut accumulator.child_death_cause,
        ),
        (
            WORKING_BRACKET,
            flows.working_deaths,
            flows.working_death_cause,
            &mut accumulator.working_death_contribution,
            &mut accumulator.working_death_cause,
        ),
        (
            ELDER_BRACKET,
            flows.elder_deaths,
            flows.elder_death_cause,
            &mut accumulator.elder_death_contribution,
            &mut accumulator.elder_death_cause,
        ),
    ];
    let mut turn_deaths = scalar_zero();
    // The dominant contributor since the last crossing: `(bracket, cause)`, first-max so ties
    // resolve in the declared child ≥ working ≥ elder order.
    let mut dominant: Option<(DeathBracket, DeathCause)> = None;
    let mut dominant_contribution = scalar_zero();
    for (bracket, flow, turn_cause, contribution, stored_cause) in brackets {
        if flow > scalar_zero() {
            *stored_cause = turn_cause;
            *contribution += flow;
        }
        turn_deaths += flow;
        if *contribution > dominant_contribution {
            dominant_contribution = *contribution;
            dominant = Some((bracket, *stored_cause));
        }
    }

    let died = DemographicFlowAccumulator::accrue(&mut accumulator.deaths, turn_deaths);
    if died == 0 {
        return;
    }
    // A crossing needs a positive flow this turn (the carry is always < 1 afterwards), and a
    // positive flow always stamps a contribution — so there is always a dominant bracket to name.
    let Some((bracket, cause)) = dominant else {
        return;
    };
    accumulator.child_death_contribution = scalar_zero();
    accumulator.working_death_contribution = scalar_zero();
    accumulator.elder_death_contribution = scalar_zero();

    let label = if died == 1 {
        format!(
            "{} died of {} in {name}",
            bracket.singular,
            cause.label_phrase()
        )
    } else {
        format!(
            "{died} {} died of {} in {name}",
            bracket.plural,
            cause.label_phrase()
        )
    };
    event_log.push(CommandEventEntry::new(
        tick,
        CommandEventKind::Died,
        faction,
        label,
        Some(format!(
            "band={} count={} bracket={} cause={}",
            band.0,
            died,
            bracket.token,
            cause.as_str()
        )),
    ));
}

/// The migration half of the demographic feed: `last_emigrated` / `last_immigrated` are already
/// whole people, so this needs no accumulator — a band either lost people this turn or it did not.
///
/// Lives beside its siblings above but is called from `advance_population_migration`, which is
/// where those counts are resolved; reporting them from `simulate_population` would announce the
/// *previous* turn's moves under the current tick.
pub(crate) fn push_migration_events(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: FactionId,
    band: BandId,
    emigrated: u32,
    immigrated: u32,
) {
    let name = band_label(band);
    if emigrated > 0 {
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Migrated,
            faction,
            format!("{emigrated} left {name}"),
            Some(format!("band={} count={} direction=out", band.0, emigrated)),
        ));
    }
    if immigrated > 0 {
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Migrated,
            faction,
            format!("{immigrated} joined {name}"),
            Some(format!("band={} count={} direction=in", band.0, immigrated)),
        ));
    }
}

/// The bands [`simulate_population`] resolves, and everything it needs off each one.
///
/// `With<ResidentBand>`: demographics run on real bands only — a detached expedition manages its own
/// larder/consumption in `advance_expeditions` and never grows/starves/migrates.
/// `Option<&LaborAllocation>` carries last turn's food-flow telemetry into the `trend` fertility
/// factor (see `band_food_flow` for why it is an `Option` all the way down).
/// `Option<&BandId>` / `Option<&mut DemographicFlowAccumulator>`: worldgen gives every real band
/// both, but a hand-built test world may spawn a bare cohort. A band missing either still runs the
/// full demographic model — it simply reports no whole-person events, because there is nothing
/// durable to name them after and nowhere to carry their fractions.
type DemographicBands<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut PopulationCohort,
        Option<&'static LaborAllocation>,
        Option<&'static BandId>,
        Option<&'static mut DemographicFlowAccumulator>,
    ),
    With<ResidentBand>,
>;

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn simulate_population(
    config: Res<SimulationConfig>,
    registry: Res<FactionRegistry>,
    impacts: Res<InfluencerImpacts>,
    effects: Res<CultureEffectsCache>,
    pipeline_config: Res<TurnPipelineConfigHandle>,
    demographics: Res<DemographicsConfigHandle>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tiles: Query<&Tile>,
    mut cohorts: DemographicBands,
    mut discovery: ResMut<DiscoveryProgressLedger>,
    mut telemetry: ResMut<TradeTelemetry>,
    mut event_log: ResMut<CommandEventLog>,
    mut trade_events: EventWriter<TradeDiffusionEvent>,
    mut migration_events: EventWriter<MigrationKnowledgeEvent>,
    tick: Res<SimulationTick>,
) {
    // `TradeTelemetry` is a PER-TURN accumulator, so someone has to clear it before the turn's
    // records go in. That used to be `trade_knowledge_diffusion`, which ran a stage earlier and
    // died with the rest of the link-driven trade slice
    // (`docs/plan_contact_and_logistics.md` §As-built). The migration path below is now its only
    // writer, so the reset moves here — still ahead of every write, and still ahead of
    // `publish_trade_telemetry`, which is ordered after this system.
    telemetry.reset_turn();
    let population_cfg = pipeline_config.config().population();
    let demo = demographics.get();
    let wellbeing = wellbeing_config.get();
    let max_cap_scalar = scalar_from_u32(config.population_cap);
    let morale_pressure_cfg = MoralePressureConfig {
        ambient_temperature: config.ambient_temperature,
        temperature_morale_penalty: config.temperature_morale_penalty,
        temperature_morale_tolerance: config.temperature_morale_tolerance,
        attrition_penalty_scale: population_cfg.attrition_penalty_scale(),
        hardness_penalty_scale: population_cfg.hardness_penalty_scale(),
    };
    for (mut cohort, labor, band_id, mut accumulator) in cohorts.iter_mut() {
        // Age the band every turn (before any early-out) so the migration gate below sees an
        // accurate settled duration even for cohorts whose home tile briefly can't be resolved.
        cohort.age_turns = cohort.age_turns.saturating_add(1);
        let Ok(tile) = tiles.get(cohort.home) else {
            cohort.morale = scalar_zero();
            continue;
        };
        let terrain_profile = terrain_definition(tile.terrain);
        let temp_diff = (tile.temperature - config.ambient_temperature).abs();
        // Place-based (negative) morale terms, from the one shared source (also the snapshot's
        // `habitability`), so sim and snapshot never drift.
        let pressure =
            tile_morale_pressure(&terrain_profile, tile.temperature, &morale_pressure_cfg);
        // Layer 1 (wellbeing): the morale delta is the signed sum of named contributors, so a
        // future factor is a new `MoraleFactor` variant + one field here — not a rewrite. The
        // contribution set doubles as the client's per-band morale breakdown. `unrest` = crisis
        // impacts + cultural sentiment (signed; may be positive).
        let contributions = MoraleContributions {
            settling: config.population_growth_rate,
            terrain: -pressure.terrain,
            climate: -pressure.cold,
            unrest: impacts.morale_delta + effects.morale_bias,
        };
        let morale_delta = contributions.total();
        // Attribute the dominant *negative* driver when morale fell (else `None`). Starvation is
        // intentionally excluded — it is surfaced through the days-of-food path, not morale.
        cohort.last_morale_delta = morale_delta;
        cohort.last_morale_cause = if morale_delta < scalar_zero() {
            contributions.dominant_negative_cause()
        } else {
            MoraleCause::None
        };
        cohort.last_morale_contributions = contributions;
        cohort.morale = (cohort.morale + morale_delta).clamp(scalar_zero(), scalar_one());

        // Layer 2 (wellbeing): map morale → the discontented share of the band. `0` at/above
        // `content_morale`, rising to `1` at/below `floor_morale`. Drives the productivity
        // modifier stack (this turn's payouts) and discontent-driven migration (below).
        cohort.discontent_fraction = discontent_fraction(cohort.morale, &wellbeing.discontent);

        // Demographic model: consume the band's local food, then resolve deaths, births,
        // maturation, and aging (see `advance_demographics`).
        let food_before = cohort.stores.get(FOOD);
        let outcome = advance_demographics(
            DemographicState {
                children: cohort.children,
                working: cohort.working,
                elders: cohort.elders,
                food_store: food_before,
            },
            band_food_flow(labor),
            temp_diff,
            max_cap_scalar,
            &demo,
        );
        cohort.children = outcome.state.children;
        cohort.working = outcome.state.working;
        cohort.elders = outcome.state.elders;
        cohort.stores.set(FOOD, outcome.state.food_store);
        // The three factors behind this turn's births, parked for the snapshot exactly as
        // `last_morale_contributions` is: the player sees the inputs (larder, Food /turn) and the
        // effect (population), and this is the attribution between them.
        cohort.last_fertility_factors = outcome.fertility;
        // The food the people ACTUALLY ate this turn = the larder drop across `advance_demographics`
        // (consumption is its only `food_store` debit). This is the ledger's consumption term — it
        // reconciles the larder exactly, unlike a `food_demand` re-derived at capture on the *post*
        // turn brackets (which the same turn's births would inflate). See `last_food_consumption`.
        cohort.last_food_consumption = (food_before - outcome.state.food_store).to_f32();
        cohort.sync_size();

        // The flows the model just resolved become the player's world events, once each has
        // accumulated a whole person. A band with neither a durable id nor a carry cannot report
        // them — see the query's doc.
        if let (Some(band_id), Some(accumulator)) = (band_id, accumulator.as_mut()) {
            push_demographic_events(
                &mut event_log,
                tick.0,
                cohort.faction,
                *band_id,
                accumulator,
                &outcome.flows,
            );
        }

        // A band's population only emigrates once it has settled for a while — this gates the
        // high-morale knowledge-migration so a freshly-spawned (e.g. well-fed starting) band can't
        // defect to a neighbor on turn one.
        if cohort.migration.is_none()
            && cohort.age_turns >= population_cfg.migration_min_settled_turns() as u32
            && cohort.morale > population_cfg.migration_morale_threshold()
            && !cohort.knowledge.is_empty()
        {
            if let Some(&destination) = registry
                .factions
                .iter()
                .find(|&&faction| faction != cohort.faction)
            {
                let migration_eta = population_cfg.migration_eta_ticks();
                let source_contract = fragments_to_contract(&cohort.knowledge);
                let scaled = scale_migration_fragments(
                    &source_contract,
                    config.migration_fragment_scaling.raw(),
                    config.migration_fidelity_floor.raw(),
                );
                if !scaled.is_empty() {
                    cohort.migration = Some(PendingMigration {
                        destination,
                        eta: migration_eta,
                        fragments: fragments_from_contract(&scaled),
                    });
                }
            }
        }

        if let Some(mut migration) = cohort.migration.take() {
            if migration.eta > 0 {
                migration.eta -= 1;
            }

            if migration.eta == 0 {
                let source_faction = cohort.faction;
                for fragment in &migration.fragments {
                    if fragment.progress <= scalar_zero() {
                        continue;
                    }
                    let delta = fragment.progress;
                    discovery.add_progress(migration.destination, fragment.discovery_id, delta);
                    telemetry.tech_diffusion_applied =
                        telemetry.tech_diffusion_applied.saturating_add(1);
                    telemetry.migration_transfers = telemetry.migration_transfers.saturating_add(1);
                    telemetry.push_record(TradeDiffusionRecord {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                        via_migration: true,
                        herd_density: 0.0,
                    });
                    trade_events.send(TradeDiffusionEvent {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                        via_migration: true,
                    });
                    migration_events.send(MigrationKnowledgeEvent {
                        tick: tick.0,
                        from: source_faction,
                        to: migration.destination,
                        discovery_id: fragment.discovery_id,
                        delta,
                    });
                }

                let payload_contract = fragments_to_contract(&migration.fragments);
                let mut knowledge_contract = fragments_to_contract(&cohort.knowledge);
                merge_fragment_payload(
                    &mut knowledge_contract,
                    &payload_contract,
                    Scalar::one().raw(),
                );
                cohort.knowledge = fragments_from_contract(&knowledge_contract);
                cohort.faction = migration.destination;
            } else {
                cohort.migration = Some(migration);
            }
        }
    }
}

#[cfg(test)]
mod tile_morale_pressure_tests {
    use super::*;
    use crate::scalar::scalar_from_f32;
    use sim_runtime::TerrainType;

    /// Config matching the shipped defaults (`turn_pipeline_config.json` population block +
    /// `simulation_config.json` temperature levers) so the assertions track real tuning.
    fn shipped_cfg(ambient: f32) -> MoralePressureConfig {
        MoralePressureConfig {
            ambient_temperature: scalar_from_f32(ambient),
            temperature_morale_penalty: scalar_from_f32(0.004),
            temperature_morale_tolerance: scalar_from_f32(9.0),
            attrition_penalty_scale: scalar_from_f32(0.2),
            hardness_penalty_scale: scalar_from_f32(0.05),
        }
    }

    #[test]
    fn karst_cavern_mouth_is_harsh() {
        let terrain = terrain_definition(TerrainType::KarstCavernMouth);
        let ambient = 0.5;
        // Temperature matches ambient → cold term is zero, so the total is the terrain drain.
        let pressure =
            tile_morale_pressure(&terrain, scalar_from_f32(ambient), &shipped_cfg(ambient));
        assert_eq!(pressure.cold, scalar_zero());
        // attrition 0.30 * 0.2 + (1.45 - 1.0) * 0.05 = 0.0825.
        let expected = scalar_from_f32(0.0825);
        assert!(
            (pressure.total() - expected).abs() < scalar_from_f32(0.0005),
            "cavern habitability {:?} should be ~0.0825",
            pressure.total().to_f32()
        );
    }

    #[test]
    fn temperature_tolerance_dead_band_yields_no_cold_drain() {
        let terrain = terrain_definition(TerrainType::AlluvialPlain);
        let ambient = 18.0;
        // Deviation within the 9° tolerance (|Δ| = 8°) → zero climate morale drain.
        let temperate = scalar_from_f32(ambient + 8.0);
        let pressure = tile_morale_pressure(&terrain, temperate, &shipped_cfg(ambient));
        assert_eq!(pressure.cold, scalar_zero());
    }

    #[test]
    fn temperature_beyond_tolerance_drains_linearly() {
        let terrain = terrain_definition(TerrainType::AlluvialPlain);
        let ambient = 18.0;
        // Pole-like tile at −5°: |Δ| = 23°, excess beyond tolerance = 23 − 9 = 14°.
        let polar = scalar_from_f32(-5.0);
        let pressure = tile_morale_pressure(&terrain, polar, &shipped_cfg(ambient));
        // 14 * 0.004 = 0.056.
        let expected = scalar_from_f32(0.056);
        assert!(
            (pressure.cold - expected).abs() < scalar_from_f32(0.0005),
            "cold drain {:?} should be ~0.056",
            pressure.cold.to_f32()
        );
    }
}

#[cfg(test)]
mod demographics_tests {
    use super::{advance_demographics, death_fraction, DemographicState, FoodFlow};
    use crate::demographics_config::DemographicsConfig;
    use crate::scalar::{scalar_from_f32, scalar_from_u32, scalar_one, scalar_zero};

    const MILD_TEMP: f32 = 0.0;
    const NO_CAP: u32 = 1_000_000_000;

    fn state(children: f32, working: f32, elders: f32, food: f32) -> DemographicState {
        DemographicState {
            children: scalar_from_f32(children),
            working: scalar_from_f32(working),
            elders: scalar_from_f32(elders),
            food_store: scalar_from_f32(food),
        }
    }

    fn total(s: &DemographicState) -> f32 {
        (s.children + s.working + s.elders).to_f32()
    }

    /// Size of the sample cohorts below, in people. A round hundred so a bracket reads directly
    /// as a percentage.
    const COHORT: f32 = 100.0;

    /// A [`COHORT`]-person cohort in the shipped **opening shape** — `initial_distribution` from
    /// `demographics_config.json`, the same split `worldgen` seeds a band with. That split is the
    /// settled equilibrium of the shipped maturation / aging / elder-mortality rates, so a cohort
    /// built this way is not spending its first turns re-balancing brackets. Pinning a split here
    /// instead would measure the *seed*: an elder-heavy start sheds elders faster than workers age
    /// in, and a "does a fed band grow" case would read that transient as a shrinking band.
    fn shipped_cohort(food: f32) -> DemographicState {
        let dist = &DemographicsConfig::default().initial_distribution;
        state(
            COHORT * dist.children,
            COHORT * dist.working,
            COHORT * dist.elders,
            food,
        )
    }

    /// One turn with **no flow telemetry** — the neutral-trend path an unprojected cohort takes.
    fn run(s: DemographicState, temp: f32) -> DemographicState {
        run_with_flow(s, temp, None)
    }

    fn run_with_flow(s: DemographicState, temp: f32, flow: Option<FoodFlow>) -> DemographicState {
        advance_demographics(
            s,
            flow,
            scalar_from_f32(temp),
            scalar_from_u32(NO_CAP),
            &DemographicsConfig::default(),
        )
        .state
    }

    /// A **childless** adult cohort of a band's rough scale (W 16.5 / E 4.5) — a fixed shape, not
    /// the shipped `initial_distribution`, so a demographic re-tune cannot silently move the
    /// fertility numbers every case below is expressed in multiples of. With no
    /// children there is no maturation out and no child deaths, so `children` after one turn **is**
    /// exactly that turn's births — the cleanest way to read fertility, which is what every
    /// factor test below is actually measuring.
    fn breeders(larder_turns: f32) -> DemographicState {
        state(0.0, 16.5, 4.5, larder_turns * BREEDER_DEMAND)
    }

    /// One turn's food demand for [`breeders`], derived from the same shared `food_demand` helper
    /// the sim uses rather than re-implemented here — `DemographicsConfig::default()` now parses
    /// the shipped `demographics_config.json` (issue #350), so deriving it from the config in hand
    /// tracks a re-tune of `per_capita_draw` automatically and only the pinned literal below has to
    /// move.
    fn breeder_demand() -> f32 {
        let cfg = DemographicsConfig::default();
        let s = state(0.0, 16.5, 4.5, 0.0);
        super::food_demand(s.children, s.working, s.elders, &cfg.consumption).to_f32()
    }

    /// = `0.16 × (16.5 + 4.5×0.8)` = `0.16 × 20.1`, pinned by `breeder_demand_matches` below.
    const BREEDER_DEMAND: f32 = 3.216;

    /// Births in one turn at the given steady income (no pens), off a childless cohort.
    fn births_at_income(larder_turns: f32, income_turns: f32) -> f32 {
        run_with_flow(
            breeders(larder_turns),
            MILD_TEMP,
            Some(FoodFlow {
                steady_income: scalar_from_f32(income_turns * BREEDER_DEMAND),
                pen_feed_upkeep: scalar_zero(),
            }),
        )
        .children
        .to_f32()
    }

    /// Guards the `BREEDER_DEMAND` literal the larder/income helpers are expressed in multiples of.
    #[test]
    fn breeder_demand_matches() {
        assert!(
            (breeder_demand() - BREEDER_DEMAND).abs() < 1e-4,
            "BREEDER_DEMAND is stale: {}",
            breeder_demand()
        );
    }

    /// A well-fed, temperate cohort grows and eats from its larder.
    #[test]
    fn fed_cohort_grows_and_consumes_food() {
        let start = shipped_cohort(1_000.0);
        let out = run(start, MILD_TEMP);
        assert!(
            total(&out) > COHORT,
            "a fed cohort should grow: {}",
            total(&out)
        );
        assert!(
            out.food_store.to_f32() < 1_000.0,
            "food should be consumed from the larder"
        );
        // Births land in the children bracket.
        assert!(
            out.children.to_f32() > start.children.to_f32(),
            "births should raise children"
        );
    }

    /// With an empty larder the cohort starves — deaths across brackets, no births, larder stays 0.
    #[test]
    fn empty_larder_starves_the_cohort() {
        let start = shipped_cohort(0.0);
        let out = run(start, MILD_TEMP);
        assert!(
            total(&out) < 0.8 * COHORT,
            "starvation should sharply cut population: {}",
            total(&out)
        );
        assert!(out.food_store.to_f32().abs() < 1e-4, "larder stays empty");
        // Dependents (1.5× vulnerability) fall harder than working-age (1.0×).
        let child_survival = out.children.to_f32() / start.children.to_f32();
        let working_survival = out.working.to_f32() / start.working.to_f32();
        assert!(
            child_survival < working_survival,
            "children should die faster than workers: {child_survival} vs {working_survival}"
        );
    }

    /// Extreme cold kills across brackets even when the larder is full.
    #[test]
    fn cold_kills_even_when_fed() {
        let warm = run(shipped_cohort(1_000.0), MILD_TEMP);
        let cold = run(shipped_cohort(1_000.0), 40.0);
        assert!(
            total(&cold) < total(&warm),
            "cold should reduce population vs temperate: {} vs {}",
            total(&cold),
            total(&warm)
        );
    }

    /// Births are morale-INDEPENDENT (wellbeing model): `advance_demographics` no longer takes
    /// morale, so a fed cohort still grows regardless of contentment — morale acts only through
    /// productivity + migration, never on births. This is the same fed grow case as
    /// `fed_cohort_grows_and_consumes_food`; it exists to lock the decoupling in place.
    #[test]
    fn births_are_morale_independent() {
        let start = shipped_cohort(1_000.0);
        let out = run(start, MILD_TEMP);
        assert!(
            out.children.to_f32() > start.children.to_f32(),
            "a fed cohort must still bear children with morale removed from the formula: {}",
            out.children.to_f32()
        );
    }

    /// The aggregate cap scales an over-large population back down.
    #[test]
    fn population_cap_clamps_total() {
        let start = state(100.0, 100.0, 100.0, 10_000.0);
        let out = advance_demographics(
            start,
            None,
            scalar_from_f32(MILD_TEMP),
            scalar_from_u32(50),
            &DemographicsConfig::default(),
        )
        .state;
        assert!(
            (total(&out) - 50.0).abs() < 1.0,
            "total should clamp to the cap of 50: {}",
            total(&out)
        );
    }

    /// Starvation deaths scale with the deficit × vulnerability but never exceed the deficit;
    /// cold adds on top, and the whole thing caps at 1.0.
    #[test]
    fn death_fraction_is_bounded_by_deficit_and_one() {
        // Full deficit, rate 0.2, vuln 1.5 → 0.30 (< deficit 1.0), no cold.
        let f = death_fraction(scalar_one(), scalar_from_f32(0.2), 1.5, scalar_zero());
        assert!((f.to_f32() - 0.30).abs() < 1e-4);
        // A 10% deficit with a steep rate×vuln (0.8×1.5=1.2) is still capped at the 10% deficit.
        let bounded = death_fraction(
            scalar_from_f32(0.1),
            scalar_from_f32(0.8),
            1.5,
            scalar_zero(),
        );
        assert!(
            (bounded.to_f32() - 0.1).abs() < 1e-4,
            "a 10% deficit must impact at most 10%: {}",
            bounded.to_f32()
        );
        // Full deficit + max cold overflow → capped at 1.0.
        let capped = death_fraction(
            scalar_one(),
            scalar_from_f32(0.8),
            1.5,
            scalar_from_f32(0.5),
        );
        assert!((capped.to_f32() - 1.0).abs() < 1e-4);
    }

    // ---- Fertility factors: stock, flow, and the gate (#286) ----

    /// **The bug in #286.** A band with a fat larder but *zero income* used to breed at the full
    /// maximum rate — `surplus_ratio` saturated at a two-turn buffer, so eighteen of its twenty
    /// turns of runway were spent at peak fertility, accelerating into the cliff it was causing.
    /// The `trend` factor now damps it to a quarter (`net_ratio = −1` → `1 − 0.75 = 0.25`).
    #[test]
    fn a_fat_larder_with_collapsed_income_no_longer_breeds_at_full_speed() {
        // Same fat larder in both, so `reserve` is identical (saturated at 1.5) and only `trend`
        // differs: 0.25 against 1.0.
        let bleeding = births_at_income(20.0, 0.0);
        let steady = births_at_income(20.0, 1.0);
        assert!(
            (bleeding / steady - 0.25).abs() < 1e-3,
            "collapsed income should breed at ~25% of break-even: {bleeding} vs {steady}"
        );
    }

    /// Damp, **not** stop: a band in total food collapse still bears children while its larder
    /// lasts. Starvation mortality is the real consequence of a deficit — `trend.deficit_penalty`
    /// is the lever that would make flow stop growth outright (set it to 1.0).
    #[test]
    fn negative_flow_damps_growth_without_stopping_it() {
        let born = births_at_income(20.0, 0.0);
        assert!(
            born > 0.0,
            "a band still eating from its larder must bear children: {born}"
        );
    }

    /// Turning `deficit_penalty` up to 1.0 **does** stop growth outright — the damp-vs-stop call is
    /// a config change, not a code change.
    #[test]
    fn deficit_penalty_of_one_stops_growth_outright() {
        let mut cfg = DemographicsConfig::default();
        cfg.births.trend.deficit_penalty = 1.0;
        let out = advance_demographics(
            breeders(20.0),
            Some(FoodFlow {
                steady_income: scalar_zero(),
                pen_feed_upkeep: scalar_zero(),
            }),
            scalar_from_f32(MILD_TEMP),
            scalar_from_u32(NO_CAP),
            &cfg,
        )
        .state;
        assert!(
            out.children.to_f32().abs() < 1e-6,
            "deficit_penalty=1.0 should zero births on a collapsed income: {}",
            out.children.to_f32()
        );
    }

    /// The `hunger` gate is the only factor that reaches zero, and it outranks everything: an empty
    /// larder bears nobody even with income generous enough to peg `trend` at its 1.25 ceiling.
    #[test]
    fn an_empty_larder_bears_nobody_however_the_modifiers_read() {
        let born = births_at_income(0.0, 3.0);
        assert!(
            born.abs() < 1e-6,
            "no food eaten → no births regardless of the modifiers: {born}"
        );
    }

    /// **The attribution must be honest.** The three factors are exported so the client can explain
    /// slow growth, which is only worth anything if they *are* the explanation: their product times
    /// `birth_rate` times the working bracket must reproduce the births the same call actually made.
    /// A drifting breakdown that adds up to the wrong answer is worse than no breakdown, so this
    /// pins the returned set against the observed outcome rather than against a re-derivation.
    ///
    /// Run on a **collapsed-income** band — the case the whole model exists for, and the one where
    /// all three factors are simultaneously off their neutral values.
    #[test]
    fn the_returned_factors_multiply_out_to_the_births_they_explain() {
        let cfg = DemographicsConfig::default();
        let start = breeders(20.0);
        let outcome = advance_demographics(
            start,
            Some(FoodFlow {
                steady_income: scalar_zero(),
                pen_feed_upkeep: scalar_zero(),
            }),
            scalar_from_f32(MILD_TEMP),
            scalar_from_u32(NO_CAP),
            &cfg,
        );
        // `breeders` is childless, so this turn's `children` IS the births (no maturation out, and a
        // fed band takes no child deaths).
        let births = outcome.state.children.to_f32();
        let explained = (start.working
            * scalar_from_f32(cfg.births.birth_rate)
            * outcome.fertility.multiplier())
        .to_f32();
        assert!(
            (births - explained).abs() < 1e-6,
            "the exported factors must explain the births they came with: {births} vs {explained}"
        );
        // ...and it is a real three-factor product, not a coincidence of neutral values.
        assert!(
            outcome.fertility.trend < scalar_one() && outcome.fertility.reserve > scalar_one(),
            "a collapsed-income band with a fat larder should read trend < 1 < reserve: {:?}",
            outcome.fertility
        );
    }

    /// The mirror bug: a band whose income exactly covers consumption is *fine*, and used to be
    /// read as poor purely for not hoarding. It now scores a `trend` of 1.0 and out-breeds an
    /// identically-provisioned band whose income has collapsed.
    #[test]
    fn a_self_sufficient_thin_larder_out_breeds_a_draining_one() {
        let fed = births_at_income(1.5, 1.0);
        let draining = births_at_income(1.5, 0.0);
        assert!(
            fed > draining,
            "break-even income should out-breed collapsed income at the same larder: {fed} vs {draining}"
        );
    }

    /// `trend` is two-sided: real surplus *raises* fertility above break-even, so provisioning well
    /// is rewarded rather than merely not-punished. Income at 1.5× demand is `net_ratio = 0.5`,
    /// exactly the shipped `surplus_saturation` → the full 1.25 bonus.
    #[test]
    fn surplus_income_breeds_faster_than_break_even() {
        let surplus = births_at_income(20.0, 1.5);
        let break_even = births_at_income(20.0, 1.0);
        assert!(
            (surplus / break_even - 1.25).abs() < 1e-3,
            "net-positive food should raise fertility by the surplus gain: {surplus} vs {break_even}"
        );
    }

    /// **No data is not a famine.** A cohort no turn has resolved has empty yield telemetry; reading
    /// that as zero income would suppress its births. `None` must score a neutral `trend` —
    /// *exactly* break-even, not merely "better than starving".
    #[test]
    fn missing_flow_telemetry_reads_neutral_not_starving() {
        let no_data = run(breeders(20.0), MILD_TEMP).children.to_f32();
        let zero_income = births_at_income(20.0, 0.0);
        let break_even = births_at_income(20.0, 1.0);
        assert!(
            no_data > zero_income,
            "unprojected flow must not be read as a deficit: {no_data} vs {zero_income}"
        );
        assert!(
            (no_data - break_even).abs() < 1e-6,
            "neutral trend should equal a break-even trend: {no_data} vs {break_even}"
        );
    }

    /// Pen feed is a real drain on the same larder, so it counts against the flow exactly as it
    /// does in the player-facing `turnsOfFood` runway — a band whose income is entirely eaten by
    /// its animals is in deficit, not at break-even.
    #[test]
    fn pen_feed_upkeep_counts_against_the_flow() {
        let with_pens = run_with_flow(
            breeders(20.0),
            MILD_TEMP,
            Some(FoodFlow {
                steady_income: scalar_from_f32(BREEDER_DEMAND),
                pen_feed_upkeep: scalar_from_f32(BREEDER_DEMAND),
            }),
        )
        .children
        .to_f32();
        let without = births_at_income(20.0, 1.0);
        assert!(
            with_pens < without,
            "pen upkeep should push an otherwise break-even band into deficit: {with_pens} vs {without}"
        );
    }

    /// The `reserve` saturation bar is a config lever, and `saturation_turns = 1.0` reproduces the
    /// retired hardcoded behaviour exactly — a two-turn buffer reading as maximum surplus.
    #[test]
    fn reserve_saturation_turns_reproduces_the_old_curve_at_one() {
        let mut cfg = DemographicsConfig::default();
        cfg.births.reserve.saturation_turns = 1.0;
        // Larder = 2 turns of demand: one eaten, one left → `reserve_turns == 1` → saturated.
        let old_curve = advance_demographics(
            breeders(2.0),
            None,
            scalar_from_f32(MILD_TEMP),
            scalar_from_u32(NO_CAP),
            &cfg,
        )
        .state
        .children
        .to_f32();
        let fat = run(breeders(20.0), MILD_TEMP).children.to_f32();
        assert!(
            (old_curve - fat).abs() < 1e-6,
            "at saturation_turns=1 a two-turn buffer should match a full larder: {old_curve} vs {fat}"
        );
        // ...and the shipped 10-turn bar is what makes those two differ.
        let thin_now = run(breeders(2.0), MILD_TEMP).children.to_f32();
        assert!(
            thin_now < fat,
            "at the shipped bar a two-turn buffer must NOT read as a full larder: {thin_now} vs {fat}"
        );
    }

    /// A childless cohort matures no one, but working-age still ages into elders.
    #[test]
    fn aging_moves_workers_into_elders() {
        let start = state(0.0, 100.0, 0.0, 10_000.0);
        let out = run(start, MILD_TEMP);
        assert!(out.elders.to_f32() > 0.0, "workers should age into elders");
    }

    /// Turns of burn-in before growth is read. Elders are a pure sink, so a band approaches its
    /// stable elder *share* at a rate set by `elder_mortality_rate` itself; reading growth before
    /// that transient has decayed would measure the transient rather than the model's rate.
    const BURN_IN_TURNS: u32 = 300;

    /// Growth-per-turn agreement bar. The two runs carry different absolute populations, so their
    /// per-turn flows quantize differently against `Scalar`'s six decimal places; this is that
    /// noise floor with room to spare, and is orders of magnitude below any real rate difference.
    const GROWTH_AGREEMENT: f32 = 1e-4;

    /// The elder shares must separate by at least this factor. The model puts them ~2.6× apart at
    /// the two rates below, so the bar leaves headroom while still refusing a run where the two
    /// shares have quietly converged.
    const MIN_SHARE_RATIO: f32 = 2.0;

    /// An elder share below this reads as "nobody is ageing" rather than "elders are short-lived".
    const LIVE_ELDER_SHARE: f32 = 0.01;

    /// Runs a permanently-fed band forward to its settled shape and returns
    /// `(growth per turn, elder share)`.
    ///
    /// The larder is topped up to **exactly** that turn's demand, so every run reads `hunger = 1`
    /// (eaten in full) and `reserve = 1` (nothing banked) and fertility is the bare `birth_rate`.
    /// A fixed fat larder instead would let the two runs' differing food *demands* move their
    /// reserve factor and hence their births — which would confound the very coupling this case
    /// exists to deny.
    fn settled_growth_and_elder_share(elder_mortality_rate: f32) -> (f32, f32) {
        let cfg = DemographicsConfig {
            elder_mortality_rate,
            ..DemographicsConfig::default()
        };
        let mut s = shipped_cohort(0.0);
        let mut before = total(&s);
        for _ in 0..=BURN_IN_TURNS {
            before = total(&s);
            s.food_store = super::food_demand(s.children, s.working, s.elders, &cfg.consumption);
            s = advance_demographics(
                s,
                None,
                scalar_from_f32(MILD_TEMP),
                scalar_from_u32(NO_CAP),
                &cfg,
            )
            .state;
        }
        (total(&s) / before, s.elders.to_f32() / total(&s))
    }

    /// **`elder_mortality_rate` sets how large the elder bracket is, not how fast the band grows.**
    /// Elders neither work nor bear children and nothing leaves the bracket except death, so
    /// births (`working × fertility`) and the child/working flows never read the elder count: the
    /// rate does not appear in the growth rate at all. That is what makes shortening old age a
    /// composition change rather than a growth re-tune in disguise.
    ///
    /// Asserted as a **pairing** — the growth rates must agree *while* the elder shares stay far
    /// apart and both stay real. A sim that had stopped ageing anyone into the elder bracket would
    /// satisfy "growth agrees" trivially, and that is the failure the pairing is here to catch.
    #[test]
    fn elder_mortality_moves_the_elder_share_and_not_the_growth_rate() {
        // The shipped rate against a much longer old age — a >3× spread in how fast elders die.
        let (short_growth, short_share) = settled_growth_and_elder_share(0.20);
        let (long_growth, long_share) = settled_growth_and_elder_share(0.06);

        assert!(
            (short_growth - long_growth).abs() < GROWTH_AGREEMENT,
            "elder mortality must not move growth: {short_growth} vs {long_growth} per turn"
        );
        assert!(
            long_share > MIN_SHARE_RATIO * short_share,
            "a longer old age must leave a materially larger elder share: {long_share} vs {short_share}"
        );
        assert!(
            short_share > LIVE_ELDER_SHARE,
            "both runs must still be ageing people into the elder bracket: {short_share}"
        );
    }
}

/// `band_food_flow`'s no-data-vs-genuine-zero disambiguation (#286). An empty `last_yields` alone
/// cannot tell an unresolved cohort from an idle one — and the two must read oppositely.
#[cfg(test)]
mod food_flow_tests {
    use super::band_food_flow;
    use crate::components::{LaborAllocation, LaborAssignment, LaborTarget, SourceYield};

    use bevy::math::UVec2;

    fn forage_assignment() -> LaborAssignment {
        LaborAssignment {
            target: LaborTarget::Forage {
                tile: UVec2::new(0, 0),
                floor: 0.5,
                species: None,
            },
            workers: 4,
            kit: None,
        }
    }

    /// A band with no labor component at all has no flow reading.
    #[test]
    fn no_labor_allocation_is_no_data() {
        assert!(band_food_flow(None).is_none());
    }

    /// **A cohort with intent but no reading**: staffed assignments, and no turn has written their
    /// telemetry yet. This must read as *not projected* rather than as zero income.
    #[test]
    fn staffed_assignments_without_telemetry_are_no_data() {
        let labor = LaborAllocation {
            assignments: vec![forage_assignment()],
            last_yields: Vec::new(),
            last_pen_feed_upkeep: 0.0,
            last_raid_forfeit: 0.0,
            last_transfer_received: 0.0,
            last_transfer_sent: 0.0,
            upkeep_fund_mode: crate::intensification::UpkeepFundMode::default(),
            build_queue: Vec::new(),
        };
        assert!(
            band_food_flow(Some(&labor)).is_none(),
            "unwritten telemetry must not be read as zero income"
        );
    }

    /// **A genuinely idle band**: no assignments means it really does produce nothing, so the same
    /// empty `last_yields` is a fact rather than missing data.
    #[test]
    fn an_idle_band_reports_a_real_zero_income() {
        let labor = LaborAllocation::default();
        let flow = band_food_flow(Some(&labor)).expect("an idle band has a real zero flow");
        assert_eq!(flow.steady_income.to_f32(), 0.0);
    }

    /// The steady income sums per-source `realized` — the forward-projected smooth value — and
    /// never the lumpy `actual`, so fertility can't sawtooth with whole-animal hunt timing.
    #[test]
    fn steady_income_sums_realized_not_actual() {
        let labor = LaborAllocation {
            assignments: vec![forage_assignment(), forage_assignment()],
            last_yields: vec![
                SourceYield {
                    actual: 12.0,
                    realized: 2.0,
                    ..SourceYield::ZERO
                },
                SourceYield {
                    actual: 0.0,
                    realized: 3.0,
                    ..SourceYield::ZERO
                },
            ],
            last_pen_feed_upkeep: 1.5,
            last_raid_forfeit: 0.0,
            last_transfer_received: 0.0,
            last_transfer_sent: 0.0,
            upkeep_fund_mode: crate::intensification::UpkeepFundMode::default(),
            build_queue: Vec::new(),
        };
        let flow = band_food_flow(Some(&labor)).expect("projected telemetry is real data");
        assert!(
            (flow.steady_income.to_f32() - 5.0).abs() < 1e-4,
            "should sum realized (2+3), not actual (12+0): {}",
            flow.steady_income.to_f32()
        );
        assert!((flow.pen_feed_upkeep.to_f32() - 1.5).abs() < 1e-4);
    }
}

#[cfg(test)]
mod death_event_tests {
    //! The pooled death carry: one carry across three brackets, labelled by the dominant
    //! contributor. These drive `push_demographic_events` directly, because the property at stake
    //! is about *how the carries combine* and a full turn would hide it behind the model's own
    //! rates.
    use super::push_demographic_events;
    use crate::components::{DeathCause, DemographicFlowAccumulator};
    use crate::orders::FactionId;
    use crate::resources::CommandEventLog;
    use crate::scalar::{scalar_from_f32, scalar_zero};
    use crate::systems::population::DemographicFlows;
    use crate::BandId;

    const TICK: u64 = 7;
    const FACTION: FactionId = FactionId(1);
    const BAND: BandId = BandId(3);

    fn deaths(child: f32, working: f32, elder: f32) -> DemographicFlows {
        DemographicFlows {
            births: scalar_zero(),
            maturations: scalar_zero(),
            agings: scalar_zero(),
            child_deaths: scalar_from_f32(child),
            working_deaths: scalar_from_f32(working),
            elder_deaths: scalar_from_f32(elder),
            child_death_cause: DeathCause::Cold,
            working_death_cause: DeathCause::Cold,
            elder_death_cause: DeathCause::Age,
        }
    }

    fn push(
        log: &mut CommandEventLog,
        carry: &mut DemographicFlowAccumulator,
        flows: &DemographicFlows,
    ) {
        push_demographic_events(log, TICK, FACTION, BAND, carry, flows);
    }

    fn died_details(log: &CommandEventLog) -> Vec<String> {
        log.iter()
            .filter(|entry| entry.kind == crate::resources::CommandEventKind::Died)
            .filter_map(|entry| entry.detail.clone())
            .collect()
    }

    /// **A loss spread across all three brackets is announced when the BAND loses a person**, not
    /// when one bracket alone does. 0.4 + 0.4 + 0.3 is 1.1 people gone; three separate carries each
    /// sit below 1 and say nothing.
    #[test]
    fn a_loss_spread_across_brackets_still_reports_a_whole_person() {
        let mut log = CommandEventLog::default();
        let mut carry = DemographicFlowAccumulator::default();

        push(&mut log, &mut carry, &deaths(0.4, 0.4, 0.3));

        let details = died_details(&log);
        assert_eq!(
            details.len(),
            1,
            "1.1 people lost is one event: {details:?}"
        );
        assert!(
            details[0].contains("count=1"),
            "the count is the whole people the band lost: {details:?}"
        );
        assert!(
            carry.deaths.to_f32() > 0.09 && carry.deaths.to_f32() < 0.11,
            "and the 0.1 remainder rides on: {}",
            carry.deaths.to_f32()
        );
    }

    /// **A remainder is never stranded by a flow that stops.** The cold snap kills 0.6 of a person
    /// and ends; months later hunger takes 0.5 more. Pooled, that is a person and it is reported.
    /// With a carry per bracket the cold 0.6 would sit unreported forever, because nothing further
    /// ever accrues to that bracket to push it over.
    #[test]
    fn a_remainder_left_by_a_stopped_flow_is_paid_off_by_a_later_one() {
        let mut log = CommandEventLog::default();
        let mut carry = DemographicFlowAccumulator::default();

        push(&mut log, &mut carry, &deaths(0.0, 0.0, 0.6));
        assert!(
            died_details(&log).is_empty(),
            "0.6 of a person is still nobody"
        );

        // The cold term is gone for good; a different bracket starves much later.
        push(&mut log, &mut carry, &deaths(0.5, 0.0, 0.0));

        let details = died_details(&log);
        assert_eq!(
            details.len(),
            1,
            "the stranded 0.6 is paid off by the later 0.5: {details:?}"
        );
        assert!(details[0].contains("count=1"), "{details:?}");
    }

    /// **The dominant contributor since the last crossing names the row.** The count is exact
    /// across brackets; `bracket=`/`cause=` are an attribution, and they name the bracket that
    /// actually did most of the dying — with the cause recorded on the turn it happened.
    #[test]
    fn the_largest_contributor_names_the_row_and_supplies_the_cause() {
        let mut log = CommandEventLog::default();
        let mut carry = DemographicFlowAccumulator::default();

        push(&mut log, &mut carry, &deaths(0.1, 0.2, 0.8));

        let details = died_details(&log);
        assert_eq!(details.len(), 1);
        assert!(
            details[0].contains("bracket=elder") && details[0].contains("cause=age"),
            "the elders contributed most and their recorded cause was old age: {details:?}"
        );
    }

    /// **Contributions reset at the crossing**, so the next event describes the deaths *it* is
    /// announcing rather than every death since the band was founded.
    #[test]
    fn the_contributions_reset_when_a_crossing_reports() {
        let mut log = CommandEventLog::default();
        let mut carry = DemographicFlowAccumulator::default();

        // Elders dominate the first crossing.
        push(&mut log, &mut carry, &deaths(0.0, 0.0, 1.0));
        // Then a starving winter takes children, in less total than the elders ever lost.
        push(&mut log, &mut carry, &deaths(0.6, 0.0, 0.0));
        push(&mut log, &mut carry, &deaths(0.6, 0.0, 0.0));

        let details = died_details(&log);
        assert_eq!(details.len(), 2, "{details:?}");
        assert!(
            details[1].contains("bracket=child"),
            "the second event is about the children it announced, not the elders of the first: \
             {details:?}"
        );
    }
}

#[cfg(test)]
mod aging_event_tests {
    //! The working→elder carry. Driven through `push_demographic_events` directly, because the
    //! property at stake is *when a fractional flow becomes an announcement* — a full turn would
    //! bury it behind the model's own rates.
    use super::push_demographic_events;
    use crate::components::{DeathCause, DemographicFlowAccumulator};
    use crate::orders::FactionId;
    use crate::resources::{CommandEventKind, CommandEventLog};
    use crate::scalar::{scalar_from_f32, scalar_zero};
    use crate::systems::population::DemographicFlows;
    use crate::BandId;

    const TICK: u64 = 11;
    const FACTION: FactionId = FactionId(1);
    const BAND: BandId = BandId(4);

    fn aging(flow: f32) -> DemographicFlows {
        DemographicFlows {
            births: scalar_zero(),
            maturations: scalar_zero(),
            agings: scalar_from_f32(flow),
            child_deaths: scalar_zero(),
            working_deaths: scalar_zero(),
            elder_deaths: scalar_zero(),
            child_death_cause: DeathCause::default(),
            working_death_cause: DeathCause::default(),
            elder_death_cause: DeathCause::default(),
        }
    }

    fn aged_events(log: &CommandEventLog) -> Vec<(String, String)> {
        log.iter()
            .filter(|entry| entry.kind == CommandEventKind::Aged)
            .map(|entry| {
                (
                    entry.label.clone(),
                    entry.detail.clone().unwrap_or_default(),
                )
            })
            .collect()
    }

    /// **A fraction of a worker is nobody.** The shipped `aging_rate` moves a fraction of a person
    /// per turn out of the working bracket; rounding that per turn would announce an elder the band
    /// never gained.
    #[test]
    fn a_fraction_of_a_worker_announces_nothing() {
        let mut log = CommandEventLog::default();
        let mut carry = DemographicFlowAccumulator::default();

        push_demographic_events(&mut log, TICK, FACTION, BAND, &mut carry, &aging(0.3));
        push_demographic_events(&mut log, TICK, FACTION, BAND, &mut carry, &aging(0.3));

        assert!(
            aged_events(&log).is_empty(),
            "0.6 of a worker is still nobody: {:?}",
            aged_events(&log)
        );
    }

    /// **The crossing reports whole people and the remainder rides on**, which is what makes a
    /// small band's transitions *late* rather than absent.
    #[test]
    fn the_crossing_reports_whole_workers_and_keeps_the_remainder() {
        let mut log = CommandEventLog::default();
        let mut carry = DemographicFlowAccumulator::default();

        // 0.6 + 0.6 crosses one person with 0.2 owed.
        push_demographic_events(&mut log, TICK, FACTION, BAND, &mut carry, &aging(0.6));
        push_demographic_events(&mut log, TICK, FACTION, BAND, &mut carry, &aging(0.6));

        let events = aged_events(&log);
        assert_eq!(events.len(), 1, "one crossing, one event: {events:?}");
        assert_eq!(
            events[0].0,
            format!("A worker joined the elders in Band {}", BAND.0)
        );
        assert_eq!(events[0].1, format!("band={} count=1", BAND.0));
        assert!(
            (carry.agings.to_f32() - 0.2).abs() < 1e-5,
            "1.2 reported one worker and kept 0.2: {}",
            carry.agings.to_f32()
        );
    }

    /// **One event names a COUNT.** Two workers crossing on one turn is one line, in the plural.
    #[test]
    fn several_workers_in_one_turn_are_one_pluralized_line() {
        let mut log = CommandEventLog::default();
        let mut carry = DemographicFlowAccumulator::default();

        push_demographic_events(&mut log, TICK, FACTION, BAND, &mut carry, &aging(2.5));

        let events = aged_events(&log);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(
            events[0].0,
            format!("2 workers joined the elders in Band {}", BAND.0)
        );
        assert_eq!(events[0].1, format!("band={} count=2", BAND.0));
    }
}

#[cfg(test)]
mod wellbeing_tests {
    use super::{
        advance_population_migration, discontent_fraction, discontent_output_modifier,
        migration_move_fraction, output_multiplier,
    };
    use crate::components::{
        MoraleCause, MoraleContributions, PopulationCohort, ResidentBand, Tile,
    };
    use crate::orders::FactionId;
    use crate::resources::{CommandEventLog, SimulationConfig, SimulationTick, TileRegistry};
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};
    use crate::wellbeing_config::{WellbeingConfig, WellbeingConfigHandle};
    use crate::LocalStore;
    use bevy::prelude::{Entity, World};
    use bevy_ecs::system::RunSystemOnce;

    fn cfg() -> WellbeingConfig {
        WellbeingConfig::default()
    }

    /// Layer 2 discontent curve: 0 at/above `content_morale` (0.6), 1 at/below `floor_morale`
    /// (0.1), linear between. Locks the worked numbers reported for morale 0.9/0.6/0.38/0.25/0.1.
    #[test]
    fn discontent_fraction_curve() {
        let d = &cfg().discontent;
        let f = |m: f32| discontent_fraction(scalar_from_f32(m), d).to_f32();
        assert!((f(0.9) - 0.0).abs() < 1e-4, "content above 0.6");
        assert!((f(0.6) - 0.0).abs() < 1e-4, "content at the threshold");
        assert!(
            (f(0.38) - 0.44).abs() < 1e-3,
            "partial discontent: {}",
            f(0.38)
        );
        assert!(
            (f(0.25) - 0.70).abs() < 1e-3,
            "partial discontent: {}",
            f(0.25)
        );
        assert!(
            (f(0.1) - 1.0).abs() < 1e-4,
            "fully discontented at the floor"
        );
    }

    /// Layer 3a output stack: 100% at zero discontent, floored at `floor_mult` (0.5) once
    /// discontent × weight would push output below the floor.
    #[test]
    fn output_modifier_stack_bounds() {
        let p = &cfg().productivity;
        assert!((discontent_output_modifier(scalar_zero(), p).to_f32() - 1.0).abs() < 1e-4);
        // 44% discontent, weight 1.0 → 56% output.
        assert!(
            (discontent_output_modifier(scalar_from_f32(0.44), p).to_f32() - 0.56).abs() < 1e-3
        );
        // 70% discontent would give 30% but is floored to 50%.
        assert!((discontent_output_modifier(scalar_from_f32(0.70), p).to_f32() - 0.5).abs() < 1e-4);
        assert!((discontent_output_modifier(scalar_one(), p).to_f32() - 0.5).abs() < 1e-4);
    }

    /// Layer 3b migration onset (decoupled from discontent): `max_rate × clamp((0.25 − morale)/0.25,
    /// 0, 1)`. 0 at/above the 0.25 threshold, 7.5% at 0.125, 15% at rock-bottom. A morale-0.38 band
    /// (discontented for productivity, but above the migration onset) sheds nobody.
    #[test]
    fn migration_move_fraction_curve() {
        let m = &cfg().migration;
        let f = |v: f32| migration_move_fraction(scalar_from_f32(v), m).to_f32();
        assert!(
            (f(0.38) - 0.0).abs() < 1e-6,
            "above onset → stays: {}",
            f(0.38)
        );
        assert!((f(0.25) - 0.0).abs() < 1e-6, "exactly at onset → 0");
        assert!(
            (f(0.24) - 0.006).abs() < 1e-4,
            "just below onset: {}",
            f(0.24)
        );
        assert!((f(0.125) - 0.075).abs() < 1e-4, "half-ramp: {}", f(0.125));
        assert!((f(0.05) - 0.12).abs() < 1e-4, "steep: {}", f(0.05));
        assert!(
            (f(0.0) - 0.15).abs() < 1e-6,
            "cap at rock-bottom: {}",
            f(0.0)
        );
    }

    fn band(home: Entity, faction: u32, morale: f32, working: f32) -> PopulationCohort {
        let m = scalar_from_f32(morale);
        let mut cohort = PopulationCohort {
            home,
            current_tile: home,
            size: 0,
            children: scalar_zero(),
            working: scalar_from_f32(working),
            elders: scalar_zero(),
            stores: LocalStore::new(),
            morale: m,
            last_food_consumption: 0.0,
            last_turn_transfer_received: 0.0,
            last_turn_transfer_sent: 0.0,
            last_morale_delta: scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: MoraleContributions::default(),
            last_fertility_factors: Default::default(),
            discontent_fraction: discontent_fraction(m, &cfg().discontent),
            grievance: scalar_zero(),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: 10,
            generation: 0,
            faction: FactionId(faction),
            knowledge: Vec::new(),
            migration: None,
        };
        cohort.sync_size();
        cohort
    }

    fn world_with_tiles(positions: &[(u32, u32)], width: u32) -> (World, Vec<Entity>) {
        let mut world = World::default();
        let mut config = SimulationConfig::builtin();
        config.map_topology.wrap_horizontal = false;
        world.insert_resource(config);
        world.insert_resource(WellbeingConfigHandle::default());
        // Migration now narrates itself into the feed, so it needs the tick + the log.
        world.insert_resource(SimulationTick::default());
        world.insert_resource(CommandEventLog::default());
        let tiles: Vec<Entity> = positions
            .iter()
            .map(|&(x, y)| {
                let tile = Tile {
                    position: bevy::math::UVec2::new(x, y),
                    ..Default::default()
                };
                world.spawn(tile).id()
            })
            .collect();
        world.insert_resource(TileRegistry {
            tiles: tiles.clone(),
            width,
            height: 1,
        });
        (world, tiles)
    }

    /// Migration relocates the morale-scaled would-move head-count from a below-threshold band to
    /// the best reachable eligible same-faction band, and the faction total is conserved (morale
    /// never kills). At morale 0.1 the move fraction is `0.15 × (0.25−0.1)/0.25 = 0.09` → ~81 of 900.
    #[test]
    fn migration_relocates_and_conserves() {
        let (mut world, tiles) = world_with_tiles(&[(0, 0), (2, 0)], 8);
        let src = world
            .spawn((band(tiles[0], 0, 0.1, 900.0), ResidentBand))
            .id();
        let dst = world
            .spawn((band(tiles[1], 0, 0.70, 900.0), ResidentBand))
            .id();
        let before: f32 = {
            let a = world.get::<PopulationCohort>(src).unwrap();
            let b = world.get::<PopulationCohort>(dst).unwrap();
            a.total().to_f32() + b.total().to_f32()
        };
        world.run_system_once(advance_population_migration);
        let a = world.get::<PopulationCohort>(src).unwrap();
        let b = world.get::<PopulationCohort>(dst).unwrap();
        assert!(a.last_emigrated > 0, "source should shed emigrants");
        assert!(
            (a.last_emigrated as f32 - 81.0).abs() <= 1.0,
            "≈9% of 900 leave: {}",
            a.last_emigrated
        );
        assert_eq!(
            b.last_immigrated, a.last_emigrated,
            "everyone who left arrives — nobody vanishes"
        );
        assert!(
            a.working.to_f32() < 900.0 && b.working.to_f32() > 900.0,
            "source shrinks, destination grows"
        );
        let after = a.total().to_f32() + b.total().to_f32();
        assert!(
            (after - before).abs() < 1.0,
            "faction population conserved: {before} -> {after}"
        );
    }

    /// A band that is discontented (for productivity) but ABOVE the migration onset stays entirely
    /// put — morale 0.38 → discontent 0.44 (output 56%) yet move fraction 0.
    #[test]
    fn above_migration_threshold_stays() {
        let (mut world, tiles) = world_with_tiles(&[(0, 0), (2, 0)], 8);
        let src = world
            .spawn((band(tiles[0], 0, 0.38, 900.0), ResidentBand))
            .id();
        let _dst = world
            .spawn((band(tiles[1], 0, 0.70, 900.0), ResidentBand))
            .id();
        world.run_system_once(advance_population_migration);
        let a = world.get::<PopulationCohort>(src).unwrap();
        assert_eq!(a.last_emigrated, 0, "above the 0.25 onset → nobody leaves");
        assert!(
            (a.working.to_f32() - 900.0).abs() < 1e-3,
            "population stays put"
        );
    }

    /// Below-threshold band with no eligible/reachable destination → people STAY (no move) and
    /// grievance rises via the trapped multiplier.
    #[test]
    fn no_destination_stays_and_grievance_rises() {
        // Source below the migration onset; the only other band is not attractive (< 0.5).
        let (mut world, tiles) = world_with_tiles(&[(0, 0), (2, 0)], 8);
        let a = world
            .spawn((band(tiles[0], 0, 0.15, 900.0), ResidentBand))
            .id();
        let _b = world
            .spawn((band(tiles[1], 0, 0.30, 900.0), ResidentBand))
            .id();
        let working_before = world.get::<PopulationCohort>(a).unwrap().working.to_f32();
        world.run_system_once(advance_population_migration);
        let cohort = world.get::<PopulationCohort>(a).unwrap();
        assert_eq!(cohort.last_emigrated, 0, "nowhere to go → nobody leaves");
        assert!(
            (cohort.working.to_f32() - working_before).abs() < 1e-3,
            "population stays put"
        );
        // Trapped accrual = grievance_gain × discontent(0.15) × trapped_multiplier.
        let disc = &cfg().discontent;
        let f = discontent_fraction(scalar_from_f32(0.15), disc);
        let expected =
            scalar_from_f32(disc.grievance_gain) * f * scalar_from_f32(disc.trapped_multiplier);
        assert!(
            (cohort.grievance - expected).to_f32().abs() < 1e-4,
            "trapped grievance accrues at the boosted rate: {} vs {}",
            cohort.grievance.to_f32(),
            expected.to_f32()
        );
    }

    /// A discontented band with a reachable happier band accrues grievance at the un-trapped rate,
    /// strictly less than the trapped band above — the two rates differ by the trapped multiplier.
    #[test]
    fn grievance_trapped_bonus() {
        let disc = &cfg().discontent;
        let f = discontent_fraction(scalar_from_f32(0.25), disc).to_f32();
        let untrapped = disc.grievance_gain * f;
        let trapped = disc.grievance_gain * f * disc.trapped_multiplier;
        assert!(trapped > untrapped, "trapped grievance accrues faster");
    }

    /// Grievance decays while the band is content (discontent_fraction == 0).
    #[test]
    fn grievance_decays_when_content() {
        let (mut world, tiles) = world_with_tiles(&[(0, 0)], 8);
        let e = {
            let mut c = band(tiles[0], 0, 0.9, 900.0);
            c.grievance = scalar_from_f32(0.5);
            world.spawn((c, ResidentBand)).id()
        };
        world.run_system_once(advance_population_migration);
        let cohort = world.get::<PopulationCohort>(e).unwrap();
        assert!(
            cohort.grievance < scalar_from_f32(0.5),
            "content bands bleed off grievance"
        );
    }

    /// The output multiplier reads a cohort's discontent through the stack (integration of §4).
    #[test]
    fn output_multiplier_reads_discontent() {
        let content = band(Entity::from_raw(0), 0, 0.9, 100.0);
        let miserable = band(Entity::from_raw(1), 0, 0.1, 100.0);
        let wb = cfg();
        assert!(
            (output_multiplier(&content, &wb) - scalar_one())
                .to_f32()
                .abs()
                < 1e-4
        );
        assert!(output_multiplier(&miserable, &wb) < scalar_one());
    }
}
