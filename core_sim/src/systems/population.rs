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

/// Combined per-turn death fraction for one age bracket: a starvation term plus a uniform cold
/// term, capped at 1.0. The starvation term scales with the food `deficit_fraction` and this
/// bracket's vulnerability but is **never allowed to exceed the deficit itself** — a 10% food
/// shortfall impacts at most 10% of the bracket. Cold is a separate, non-food mortality.
fn death_fraction(
    deficit_fraction: Scalar,
    starvation_rate: Scalar,
    vulnerability: f32,
    cold_fraction: Scalar,
) -> Scalar {
    let starvation = min(
        deficit_fraction * starvation_rate * scalar_from_f32(vulnerability),
        deficit_fraction,
    );
    min(starvation + cold_fraction, scalar_one())
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

    // 4. Aging flows.
    let maturation = children0 * scalar_from_f32(demo.maturation_rate);
    let aging = working0 * scalar_from_f32(demo.aging_rate);
    let elder_mortality = elders0 * scalar_from_f32(demo.elder_mortality_rate);

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
    // `With<ResidentBand>`: demographics run on real bands only — a detached expedition manages its
    // own larder/consumption in `advance_expeditions` and never grows/starves/migrates.
    // `Option<&LaborAllocation>` carries last turn's food-flow telemetry into the `trend` fertility
    // factor (see `band_food_flow` for why it is an `Option` all the way down).
    mut cohorts: Query<(&mut PopulationCohort, Option<&LaborAllocation>), With<ResidentBand>>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
    mut telemetry: ResMut<TradeTelemetry>,
    mut trade_events: EventWriter<TradeDiffusionEvent>,
    mut migration_events: EventWriter<MigrationKnowledgeEvent>,
    tick: Res<SimulationTick>,
) {
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
    for (mut cohort, labor) in cohorts.iter_mut() {
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

    /// A **childless** working cohort (the 30-person opening band's adults: W 16.5 / E 4.5). With no
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
        let start = state(30.0, 55.0, 15.0, 1_000.0);
        let out = run(start, MILD_TEMP);
        assert!(
            total(&out) > 100.0,
            "a fed cohort should grow: {}",
            total(&out)
        );
        assert!(
            out.food_store.to_f32() < 1_000.0,
            "food should be consumed from the larder"
        );
        // Births land in the children bracket.
        assert!(out.children.to_f32() > 30.0, "births should raise children");
    }

    /// With an empty larder the cohort starves — deaths across brackets, no births, larder stays 0.
    #[test]
    fn empty_larder_starves_the_cohort() {
        let start = state(30.0, 55.0, 15.0, 0.0);
        let out = run(start, MILD_TEMP);
        assert!(
            total(&out) < 80.0,
            "starvation should sharply cut population: {}",
            total(&out)
        );
        assert!(out.food_store.to_f32().abs() < 1e-4, "larder stays empty");
        // Dependents (1.5× vulnerability) fall harder than working-age (1.0×).
        let child_survival = out.children.to_f32() / 30.0;
        let working_survival = out.working.to_f32() / 55.0;
        assert!(
            child_survival < working_survival,
            "children should die faster than workers: {child_survival} vs {working_survival}"
        );
    }

    /// Extreme cold kills across brackets even when the larder is full.
    #[test]
    fn cold_kills_even_when_fed() {
        let warm = run(state(30.0, 55.0, 15.0, 1_000.0), MILD_TEMP);
        let cold = run(state(30.0, 55.0, 15.0, 1_000.0), 40.0);
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
        let start = state(30.0, 55.0, 15.0, 1_000.0);
        let out = run(start, MILD_TEMP);
        assert!(
            out.children.to_f32() > 30.0,
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
}

/// `band_food_flow`'s no-data-vs-genuine-zero disambiguation (#286). An empty `last_yields` alone
/// cannot tell an unresolved cohort from an idle one — and the two must read oppositely.
#[cfg(test)]
mod food_flow_tests {
    use super::band_food_flow;
    use crate::components::{LaborAllocation, LaborAssignment, LaborTarget, SourceYield};
    use crate::FollowPolicy;
    use bevy::math::UVec2;

    fn forage_assignment() -> LaborAssignment {
        LaborAssignment {
            target: LaborTarget::Forage {
                tile: UVec2::new(0, 0),
                floor: FollowPolicy::Sustain.escapement_floor(),
                species: None,
            },
            workers: 4,
            improvement: None,
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
mod wellbeing_tests {
    use super::{
        advance_population_migration, discontent_fraction, discontent_output_modifier,
        migration_move_fraction, output_multiplier,
    };
    use crate::components::{
        MoraleCause, MoraleContributions, PopulationCohort, ResidentBand, Tile,
    };
    use crate::orders::FactionId;
    use crate::resources::{SimulationConfig, TileRegistry};
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
