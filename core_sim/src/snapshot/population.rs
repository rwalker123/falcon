use super::*;

pub(crate) fn pending_migration_to_state(migration: &PendingMigration) -> PendingMigrationState {
    PendingMigrationState {
        destination: migration.destination.0,
        eta: migration.eta,
        fragments: fragments_to_contract(&migration.fragments),
    }
}

pub(crate) fn pending_migration_from_state(state: &PendingMigrationState) -> PendingMigration {
    PendingMigration {
        destination: FactionId(state.destination),
        eta: state.eta,
        fragments: fragments_from_contract(&state.fragments),
    }
}

/// Rebuild a [`LaborAllocation`] from its snapshot state (rollback restores the exact allocation,
/// as `harvest_task`/`scout_task` did for the retired single-task model). Unknown role strings are
/// skipped defensively; a hunt with an unparseable policy falls back to `FollowPolicy::Sustain`
/// (the assignment is kept, not dropped — we serialize valid policy strings, so this only guards
/// against a corrupt/forward-incompatible snapshot).
pub(crate) fn labor_allocation_from_state(states: &[LaborAssignmentState]) -> LaborAllocation {
    let assignments = states
        .iter()
        .filter_map(|state| {
            let target = match state.kind.as_str() {
                "forage" => LaborTarget::Forage {
                    tile: UVec2::new(state.target_x, state.target_y),
                    policy: FollowPolicy::from_str(&state.policy).unwrap_or_default(),
                },
                "hunt" => LaborTarget::Hunt {
                    fauna_id: state.fauna_id.clone(),
                    policy: FollowPolicy::from_str(&state.policy).unwrap_or_default(),
                },
                "scout" => LaborTarget::Scout,
                "warrior" => LaborTarget::Warrior,
                _ => return None,
            };
            Some(LaborAssignment {
                target,
                workers: state.workers,
            })
        })
        .collect();
    // `last_yields` is derived telemetry, not persisted — it stays empty on rehydrate and is
    // rebuilt by the next `advance_labor_allocation`.
    LaborAllocation {
        assignments,
        ..Default::default()
    }
}

/// Serialize one labor assignment for the snapshot (client readout + rollback persistence). The
/// `yields` carry this turn's actual/sustainable food income for the source (per-source breakdown;
/// derived, not part of the rollback-persisted intent — defaulted to `0` when telemetry is absent,
/// e.g. a rehydrated save before the next tick).
pub(crate) fn labor_assignment_to_state(
    assignment: &LaborAssignment,
    yields: &SourceYield,
) -> LaborAssignmentState {
    let mut state = LaborAssignmentState {
        kind: assignment.target.kind().to_string(),
        workers: assignment.workers,
        actual_yield: yields.actual,
        sustainable_yield: yields.sustainable,
        workers_needed: yields.workers_needed,
        wasted_yield: yields.wasted,
        overdraws: yields.overdraws,
        realized_yield: yields.realized,
        // The discrete arrival schedule: index `i` = the food landing `i + 1` turns ahead. Cloned
        // rather than moved so the caller's telemetry row (which the band roll-ups below still read)
        // is untouched.
        arrival_schedule: yields.arrivals.clone(),
        ..Default::default()
    };
    match &assignment.target {
        LaborTarget::Forage { tile, policy } => {
            state.target_x = tile.x;
            state.target_y = tile.y;
            state.policy = policy.as_str().to_string();
        }
        LaborTarget::Hunt { fauna_id, policy } => {
            state.fauna_id = fauna_id.clone();
            state.policy = policy.as_str().to_string();
        }
        LaborTarget::Scout | LaborTarget::Warrior => {}
    }
    state
}

/// `days_of_food` sentinel for a cohort that is **not food-limited** — no food demand at all (a
/// zero-population cohort), or income that meets or beats the drain so the larder never empties.
/// The client reads it as ∞.
pub(crate) const NOT_FOOD_LIMITED_DAYS: f32 = 999.0;

/// The larder runway, in **TURNS until the larder is empty** — the value the wire's (misnamed)
/// `daysOfFood` carries.
///
/// **One formula, both actors.** `runway = larder / net drain`, `net drain = consumption +
/// pen_feed − income`. An **expedition** has no labor income and keeps no pens, so it reduces to
/// `provisions / consumption` — exactly the historical reading, unchanged. A resident band with
/// real income gets the honest number instead of the "we stop gathering and hunting" pessimism the
/// old `larder / demand` assumed.
///
/// Resolved the way the client's FOOD OUTLOOK chart resolves it, so the two cannot disagree by a
/// turn or two on the same panel:
/// 1. Walk the larder forward over the merged per-source **arrival schedules** (`arrivals[i]` = the
///    food landing `i + 1` turns from now), debiting `consumption + pen_feed` each turn and
///    clamping at zero. The first turn that reaches zero is the answer, counted from now.
/// 2. It never empties within the horizon (or no source was projected at all — an empty schedule
///    is "no data", never a famine): fall back to the smooth `larder / net_drain` on the **steady**
///    `realized` income, capped at the sentinel.
/// 3. `net_drain <= 0` (net-positive, not food-limited): the [`NOT_FOOD_LIMITED_DAYS`] sentinel.
pub(crate) fn larder_runway_turns(
    larder: f32,
    consumption: f32,
    pen_feed_upkeep: f32,
    steady_income: f32,
    arrivals: &[f32],
) -> f32 {
    let drain = consumption + pen_feed_upkeep;
    if !arrivals.is_empty() {
        let mut food = larder.max(0.0);
        for (turn, arrival) in arrivals.iter().enumerate() {
            food = (food + arrival - drain).max(0.0);
            if food <= 0.0 {
                // `turn` is 0-based over "turns from now", so the count is one more.
                return (turn + 1) as f32;
            }
        }
    }
    let net_drain = drain - steady_income;
    if net_drain <= 0.0 {
        return NOT_FOOD_LIMITED_DAYS;
    }
    (larder / net_drain).min(NOT_FOOD_LIMITED_DAYS)
}

/// The band-wide merged arrival schedule: element-wise sum of every source's `arrivals`, so slot
/// `i` is **all** the food landing `i + 1` turns from now (the client's `_merged_arrival_schedule`).
/// Empty when nothing was projected — Scout/Warrior only, or a rehydrated save before the next tick.
fn merged_arrival_schedule(allocation: Option<&LaborAllocation>) -> Vec<f32> {
    let mut merged: Vec<f32> = Vec::new();
    let Some(allocation) = allocation else {
        return merged;
    };
    for yields in &allocation.last_yields {
        if yields.arrivals.len() > merged.len() {
            merged.resize(yields.arrivals.len(), 0.0);
        }
        for (slot, arrival) in yields.arrivals.iter().enumerate() {
            merged[slot] += arrival;
        }
    }
    merged
}

/// Summarize a band's labor allocation into the legacy `activity`/`hunt_mode` strings (so the
/// pre-3b client keeps rendering): `activity` = the target-kind with the most workers (else
/// `"idle"`), `hunt_mode` = the policy of the largest Hunt assignment (else empty).
pub(crate) fn allocation_summary(allocation: Option<&LaborAllocation>) -> (String, String) {
    let Some(allocation) = allocation else {
        return ("idle".to_string(), String::new());
    };
    let dominant = allocation
        .assignments
        .iter()
        .filter(|a| a.workers > 0)
        .max_by_key(|a| a.workers);
    let activity = dominant
        .map(|a| a.target.kind().to_string())
        .unwrap_or_else(|| "idle".to_string());
    let hunt_mode = allocation
        .assignments
        .iter()
        .filter_map(|a| match &a.target {
            LaborTarget::Hunt { policy, .. } if a.workers > 0 => Some((a.workers, policy)),
            _ => None,
        })
        .max_by_key(|(workers, _)| *workers)
        .map(|(_, policy)| policy.as_str().to_string())
        .unwrap_or_default();
    (activity, hunt_mode)
}

/// The global expedition levers the snapshot echoes onto **every** cohort (resolved once per
/// capture, not per band). `max_party_size` pre-clamps the client's outfit stepper; the other three
/// are the linear constants the client's **pre-launch hunt forecast** multiplies against a herd's
/// exported `hunt_policy_ceilings` — so the outfit UI never re-derives the ecology model. See
/// `core_sim/CLAUDE.md` → Scouting & Hunting Expeditions → Snapshot.
pub(crate) struct ExpeditionLevers {
    pub(crate) max_party_size: u32,
    pub(crate) hunt_per_worker_carry: f32,
    pub(crate) hunt_per_worker_provisions: f32,
    pub(crate) hunt_viability_warn_turns: u32,
    /// `labor_config.band_move_tiles_per_turn` — a band's move speed, echoed per-cohort so the client
    /// can add a raid's round-trip travel (`ceil(2 × hex_distance / this)`) to the band-agnostic
    /// pre-launch `huntTripEstimates`. Same global-config-surfaced-per-band idiom as the others.
    pub(crate) band_move_tiles_per_turn: u32,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn population_state(
    entity: Entity,
    cohort: &PopulationCohort,
    allocation: Option<&LaborAllocation>,
    expedition: Option<&Expedition>,
    home_position: Option<UVec2>,
    current_position: Option<UVec2>,
    is_traveling: bool,
    stockpile_radius: u32,
    start_position: Option<UVec2>,
    inventory: &FactionInventory,
    demographics: &DemographicsConfig,
    wellbeing: &crate::wellbeing_config::WellbeingConfig,
    supply_membership: &SupplyNetworkMembership,
    work_range: u32,
    scout_vantage_distance: u32,
    expedition_levers: &ExpeditionLevers,
    settlement_stage_config: &crate::settlement_stage_config::SettlementStageConfig,
    travel_target: Option<UVec2>,
    hunt_reach: u32,
) -> PopulationCohortState {
    let migration = cohort.migration.as_ref().map(pending_migration_to_state);
    let (travel_target_x, travel_target_y) = travel_target.map(|t| (t.x, t.y)).unwrap_or((0, 0));
    let demand = food_demand(
        cohort.children,
        cohort.working,
        cohort.elders,
        &demographics.consumption,
    );
    let (activity, hunt_mode) = allocation_summary(allocation);
    let working_age = available_workers(cohort.working);
    let assigned = allocation.map(|a| a.assigned_total()).unwrap_or(0);
    let idle_workers = working_age.saturating_sub(assigned);
    // Zip each assignment with its retained per-source yield telemetry (same index order). A
    // rehydrated allocation has an empty `last_yields` until the next tick → default 0 yields.
    const NO_YIELD: SourceYield = SourceYield::ZERO;
    let labor_assignments = allocation
        .map(|a| {
            a.assignments
                .iter()
                .enumerate()
                .map(|(i, assignment)| {
                    labor_assignment_to_state(assignment, a.last_yields.get(i).unwrap_or(&NO_YIELD))
                })
                .collect()
        })
        .unwrap_or_default();
    // Band-level food flow: income = Σ per-source actual yield; consumption is the food the people
    // ACTUALLY ate this turn (`cohort.last_food_consumption`, the real `stores` debit at the turn's
    // opening brackets), NOT a `food_demand` re-derived here on the post-turn brackets — the same
    // turn's births would inflate that and break the larder ledger identity by exactly the growth.
    // (`demand` above stays post-turn for `days_of_food`, which is a forward "turns I can last".)
    let food_income = allocation
        .map(|a| a.last_yields.iter().map(|y| y.actual).sum())
        .unwrap_or(0.0);
    // The **steady** headline income = Σ per-source `realized` (the honest long-run average of the
    // lumpy `actual`). Distinct from `food_income` above precisely on whole-animal sources, where
    // `actual` pulses (0 on wait turns, spikes on kills) while `realized` holds steady. The client's
    // "Food /turn" uses this so the number stops swinging turn-to-turn; `food_income` stays the real
    // arrivals and preserves the `larder_delta == foodIncome − foodConsumption − penFeedUpkeep`
    // ledger identity. Derived per-turn, like `food_income` (0.0 on a rehydrated save until the next
    // tick).
    let food_income_average = allocation
        .map(|a| a.last_yields.iter().map(|y| y.realized).sum())
        .unwrap_or(0.0);
    let food_consumption = cohort.last_food_consumption;
    // The pen feed this band ACTUALLY paid this turn (the real `LocalStore::take` debit, summed across
    // its pens by `advance_labor_allocation`). It is in NEITHER of the two terms above — a pen's feed
    // comes straight off `cohort.stores` — so without exporting it the client's
    // `food_income − food_consumption` net-food row overstates the surplus by exactly the upkeep, and
    // the player watches the larder drain with no explanation. Derived, like `food_income`: `0.0` on a
    // rehydrated save until the next tick.
    let pen_feed_upkeep = allocation.map(|a| a.last_pen_feed_upkeep).unwrap_or(0.0);
    // The honest larder runway — turns until the larder empties, INCOME INCLUDED (the wire calls it
    // `days_of_food`; see `larder_runway_turns`). Consumption is the forward `demand` above (what
    // the people will want to eat), not `last_food_consumption`: `demand` is always resolvable,
    // where the actual debit is `0` on a rehydrated save and short of demand in a famine.
    let days_of_food = if demand.raw() <= 0 {
        NOT_FOOD_LIMITED_DAYS
    } else {
        larder_runway_turns(
            cohort.stores.get(FOOD).to_f32(),
            demand.to_f32(),
            pen_feed_upkeep,
            food_income_average,
            &merged_arrival_schedule(allocation),
        )
    };
    // Expedition discriminators + persistence fields (empty/false for a normal band).
    let (
        is_expedition,
        expedition_mission,
        expedition_phase,
        expedition_target_herd,
        expedition_hunt_policy,
        home_band_entity,
        expedition_announced,
        pending_reveal_x,
        pending_reveal_y,
    ) = match expedition {
        Some(exp) => (
            true,
            exp.mission.as_str().to_string(),
            exp.phase.as_str().to_string(),
            exp.mission.target_herd().to_string(),
            exp.mission.hunt_policy_str().to_string(),
            exp.home_band.to_bits(),
            exp.announced,
            exp.pending_reveal.iter().map(|p| p.x).collect(),
            exp.pending_reveal.iter().map(|p| p.y).collect(),
        ),
        None => (
            false,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            0,
            false,
            Vec::new(),
            Vec::new(),
        ),
    };
    // Resolve the band's settlement stage from the data-driven config (interim input: head-count).
    // Empty config would yield None; fall back to the empty view so the field is always present.
    let settlement_stage_inputs =
        crate::settlement_stage_config::SettlementStageInputs { size: cohort.size };
    let settlement_stage = crate::settlement_stage_config::resolve_settlement_stage(
        &settlement_stage_inputs,
        &settlement_stage_config.stages,
    )
    .map(|stage| SettlementStageViewState {
        id: stage.id.clone(),
        label: stage.label.clone(),
        icon: stage.icon.clone(),
    })
    .unwrap_or_default();
    // Hunt carry cap = party_workers × per_worker_carry (`0` for scouts + normal bands). The party's
    // worker count is its working-age head-count.
    let expedition_carry_cap = match expedition {
        Some(exp) if matches!(exp.mission, ExpeditionMission::Hunt { .. }) => {
            working_age as f32 * expedition_levers.hunt_per_worker_carry
        }
        _ => 0.0,
    };
    PopulationCohortState {
        entity: entity.to_bits(),
        home: cohort.home.to_bits(),
        current_x: current_position.map(|p| p.x).unwrap_or(0),
        current_y: current_position.map(|p| p.y).unwrap_or(0),
        is_traveling,
        size: cohort.size,
        children: cohort.children.raw(),
        working: cohort.working.raw(),
        elders: cohort.elders.raw(),
        stores: cohort
            .stores
            .iter()
            .map(|(item, qty)| CohortStoreState {
                item: item.to_string(),
                quantity: qty.raw(),
            })
            .collect(),
        age_turns: cohort.age_turns,
        days_of_food,
        activity,
        hunt_mode,
        labor_assignments,
        idle_workers,
        working_age,
        work_range,
        // Repurposed: carries the band's effective scout vantage distance (how far the forward-
        // observer vantage ring is posted, `0` with no scouts), not the retired fog-pulse radius.
        // See the field doc in `sim_schema`.
        scout_reveal_radius: scout_vantage_distance,
        is_expedition,
        expedition_mission,
        expedition_phase,
        home_band_entity,
        expedition_announced,
        pending_reveal_x,
        pending_reveal_y,
        max_expedition_party_size: expedition_levers.max_party_size,
        expedition_carry_cap,
        // Appended after every earlier-shipped field (append-only wire discipline; matches the
        // `.fbs` slot order for `expeditionTargetHerd`/`expeditionHuntPolicy`/`travelTargetX/Y`).
        expedition_target_herd,
        expedition_hunt_policy,
        travel_target_x,
        travel_target_y,
        hunt_reach,
        supply_network_id: supply_membership.network_of(entity),
        morale_delta: cohort.last_morale_delta.raw(),
        morale_cause: cohort.last_morale_cause.as_u8(),
        output_multiplier: crate::systems::output_multiplier(cohort, wellbeing).raw(),
        discontent_fraction: cohort.discontent_fraction.raw(),
        last_emigrated: cohort.last_emigrated,
        last_immigrated: cohort.last_immigrated,
        grievance: cohort.grievance.raw(),
        morale_settling: cohort.last_morale_contributions.settling.raw(),
        morale_terrain: cohort.last_morale_contributions.terrain.raw(),
        morale_climate: cohort.last_morale_contributions.climate.raw(),
        morale_unrest: cohort.last_morale_contributions.unrest.raw(),
        morale: cohort.morale.raw(),
        generation: cohort.generation,
        faction: cohort.faction.0,
        knowledge_fragments: fragments_to_contract(&cohort.knowledge),
        migration,
        // Retired single-task fields (kept in the schema for append-only compatibility; the
        // labor allocation replaces them). Always empty now.
        harvest_task: None,
        scout_task: None,
        accessible_stockpile: accessible_stockpile_state(
            inventory,
            cohort.faction,
            home_position,
            start_position,
            stockpile_radius,
        ),
        settlement_stage,
        food_income,
        food_income_average,
        food_consumption,
        pen_feed_upkeep,
        // Pre-launch hunt-forecast levers (global config, echoed onto every cohort — the outfit UI
        // reads them off the selected resident band).
        hunt_per_worker_provisions: expedition_levers.hunt_per_worker_provisions,
        expedition_viability_warn_turns: expedition_levers.hunt_viability_warn_turns,
        expedition_per_worker_carry: expedition_levers.hunt_per_worker_carry,
        band_move_tiles_per_turn: expedition_levers.band_move_tiles_per_turn as f32,
    }
}

pub(crate) fn accessible_stockpile_state(
    inventory: &FactionInventory,
    faction: FactionId,
    home_position: Option<UVec2>,
    start_position: Option<UVec2>,
    radius: u32,
) -> Option<AccessibleStockpileState> {
    let home = home_position?;
    let origin = start_position?;
    let distance = home.x.abs_diff(origin.x) + home.y.abs_diff(origin.y);
    if (radius == 0 && distance > 0) || (radius > 0 && distance > radius) {
        return None;
    }
    let stockpile = inventory.stockpile(faction)?;
    let mut entries: Vec<AccessibleStockpileEntryState> = Vec::new();
    for (item, quantity) in stockpile.iter() {
        if *quantity == 0 {
            continue;
        }
        entries.push(AccessibleStockpileEntryState {
            item: item.clone(),
            quantity: *quantity,
        });
    }
    if entries.is_empty() {
        return None;
    }
    Some(AccessibleStockpileState { radius, entries })
}

pub(crate) fn generation_state(profile: &GenerationProfile) -> GenerationState {
    let [knowledge, trust, equity, agency] = profile.bias.to_scaled();
    GenerationState {
        id: profile.id,
        name: profile.name.clone(),
        bias_knowledge: knowledge,
        bias_trust: trust,
        bias_equity: equity,
        bias_agency: agency,
    }
}

/// Aggregate the per-cohort age brackets into a per-faction age structure for the HUD readout,
/// reconciled so the three emitted head-counts agree with the per-band selection panel.
///
/// `working` is the sum of each cohort's floored `available_workers` (the exact assignable-worker
/// count the band panel shows), and the total head-count is the sum of each cohort's authoritative
/// `size`. Dependents (`total − working`, clamped ≥ 0) are split into `children` + `elders` in
/// proportion to the summed fixed-point child/elder masses, rounded so they sum *exactly* to the
/// dependents (round-half on children, elders takes the remainder). This guarantees
/// `children + working + elders == Σ size` and `working == Σ available_workers`, so the client's
/// `Pop = children + working + elders` matches the summed band sizes with no independent-rounding
/// overshoot.
pub(crate) fn snapshot_demographics(
    cohorts: &[PopulationCohortState],
) -> Vec<SchemaPopulationDemographicsState> {
    // Per faction: (Σ size, Σ available_workers, Σ children mass, Σ elders mass).
    let mut by_faction: std::collections::BTreeMap<u32, (u64, u64, i128, i128)> =
        std::collections::BTreeMap::new();
    for cohort in cohorts {
        let entry = by_faction.entry(cohort.faction).or_insert((0, 0, 0, 0));
        entry.0 += u64::from(cohort.size);
        entry.1 += u64::from(available_workers(Scalar::from_raw(cohort.working)));
        entry.2 += i128::from(cohort.children.max(0));
        entry.3 += i128::from(cohort.elders.max(0));
    }
    by_faction
        .into_iter()
        .map(|(faction, (total, workers, children_mass, elders_mass))| {
            // Clamp workers to the head-count so dependents never go negative.
            let working = workers.min(total);
            let dependents = total - working;
            let dependent_mass = children_mass + elders_mass;
            // Split dependents ∝ child:elder mass, round-half on children so the two brackets
            // sum exactly to `dependents`. i128 keeps the product overflow-free.
            let children = if dependent_mass == 0 {
                0
            } else {
                let dep = dependents as i128;
                ((dep * children_mass + dependent_mass / 2) / dependent_mass) as u64
            };
            let elders = dependents - children;
            SchemaPopulationDemographicsState {
                faction,
                children: children as u32,
                working: working as u32,
                elders: elders as u32,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};

    /// Enough larder that the runway is decided by the flows, not by an empty cupboard.
    const TEST_LARDER: f32 = 40.0;
    /// `f32` sums of `Scalar`-quantized values — a few ULPs of slack, no more.
    const EPSILON: f32 = 1e-4;

    fn levers() -> ExpeditionLevers {
        let cfg = ExpeditionConfig::builtin();
        ExpeditionLevers {
            max_party_size: cfg.max_party_size,
            hunt_per_worker_carry: cfg.hunt.per_worker_carry,
            hunt_per_worker_provisions: 0.0,
            hunt_viability_warn_turns: cfg.hunt.viability_warn_turns,
            band_move_tiles_per_turn: 1,
        }
    }

    /// A minimal content cohort with `larder` food and a working-age bracket that eats.
    fn cohort(larder: f32) -> PopulationCohort {
        let mut stores = LocalStore::new();
        stores.set(FOOD, scalar_from_f32(larder));
        PopulationCohort {
            home: Entity::from_raw(1),
            current_tile: Entity::from_raw(1),
            size: 30,
            children: scalar_zero(),
            working: scalar_from_f32(30.0),
            elders: scalar_zero(),
            stores,
            morale: scalar_one(),
            last_food_consumption: 0.0,
            last_morale_delta: scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: MoraleContributions::default(),
            discontent_fraction: scalar_zero(),
            grievance: scalar_zero(),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: 0,
            generation: 0,
            faction: crate::FactionId(0),
            knowledge: Vec::new(),
            migration: None,
        }
    }

    /// Capture one cohort exactly as the snapshot does, and return its `days_of_food`.
    fn captured_runway(
        cohort: &PopulationCohort,
        allocation: Option<&LaborAllocation>,
        expedition: Option<&Expedition>,
    ) -> f32 {
        population_state(
            Entity::from_raw(1),
            cohort,
            allocation,
            expedition,
            None,
            None,
            false,
            0,
            None,
            &FactionInventory::default(),
            &DemographicsConfig::builtin(),
            &crate::wellbeing_config::WellbeingConfig::builtin(),
            &SupplyNetworkMembership::default(),
            0,
            0,
            &levers(),
            &crate::settlement_stage_config::SettlementStageConfig::builtin(),
            None,
            0,
        )
        .days_of_food
    }

    /// The one-turn demand the runway divides by — the same helper the capture uses.
    fn demand_of(cohort: &PopulationCohort) -> f32 {
        food_demand(
            cohort.children,
            cohort.working,
            cohort.elders,
            &DemographicsConfig::builtin().consumption,
        )
        .to_f32()
    }

    /// A Hunt-shaped allocation carrying a hand-built arrival schedule + steady realized income, so
    /// the runway can be exercised without standing a herd up.
    fn allocation_with(arrivals: Vec<f32>, realized: f32) -> LaborAllocation {
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: "test-herd".to_string(),
                    policy: FollowPolicy::Sustain,
                },
                workers: 4,
            }],
            last_yields: vec![SourceYield {
                arrivals,
                realized,
                ..SourceYield::ZERO
            }],
            ..Default::default()
        }
    }

    /// **The compatibility guarantee.** An expedition has no labor income and keeps no pens, so the
    /// one shared formula reduces to exactly the historical `provisions / consumption`.
    #[test]
    fn an_expedition_reports_provisions_over_consumption() {
        let cohort = cohort(TEST_LARDER);
        let expedition = Expedition {
            home_band: Entity::from_raw(2),
            mission: ExpeditionMission::Scout,
            phase: ExpeditionPhase::Outbound,
            announced: false,
            pending_reveal: Vec::new(),
        };
        let runway = captured_runway(&cohort, None, Some(&expedition));
        let historical = TEST_LARDER / demand_of(&cohort);
        assert!(
            (runway - historical).abs() < EPSILON,
            "an expedition's runway must be unchanged: got {runway}, historical {historical}"
        );
    }

    /// A band with real income lasts LONGER than the old "we stop gathering and hunting" reading,
    /// and the number it reports is the turn the walked larder actually hits zero.
    #[test]
    fn a_band_with_income_outlasts_larder_over_consumption() {
        let cohort = cohort(TEST_LARDER);
        let demand = demand_of(&cohort);
        // Income covering ~half the drain: the larder still empties, but takes ~twice as long.
        let per_turn = demand * 0.5;
        let allocation = allocation_with(vec![per_turn; 20], per_turn);
        let runway = captured_runway(&cohort, Some(&allocation), None);
        let pessimistic = TEST_LARDER / demand;
        assert!(
            runway > pessimistic,
            "income must lengthen the runway: got {runway}, pessimistic {pessimistic}"
        );
        // Walk it by hand — the client's chart arithmetic — and land on the same turn.
        let mut food = TEST_LARDER;
        let mut expected = 0;
        for turn in 1..=20 {
            food = (food + per_turn - demand).max(0.0);
            if food <= 0.0 {
                expected = turn;
                break;
            }
        }
        assert_eq!(
            runway as u32, expected,
            "the reported runway must be the turn the walked larder empties"
        );
    }

    /// A net-positive band is **not food-limited**: it reports the sentinel (∞ on the client), never
    /// a huge finite number.
    #[test]
    fn a_net_positive_band_reports_the_not_food_limited_sentinel() {
        let cohort = cohort(TEST_LARDER);
        let demand = demand_of(&cohort);
        let per_turn = demand * 1.5;
        let allocation = allocation_with(vec![per_turn; 20], per_turn);
        let runway = captured_runway(&cohort, Some(&allocation), None);
        assert_eq!(runway, NOT_FOOD_LIMITED_DAYS);
    }

    /// **An empty schedule is "no data", never a famine.** A cohort whose sources were not projected
    /// (Scout/Warrior only, or a rehydrated save before the next tick) falls back to the smooth
    /// estimate on its steady income — and a band with no income at all still reports the honest
    /// `larder / consumption`, not `0`.
    #[test]
    fn an_unprojected_cohort_still_reports_a_sane_runway() {
        let cohort = cohort(TEST_LARDER);
        let demand = demand_of(&cohort);
        let scouting = LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Scout,
                workers: 4,
            }],
            last_yields: vec![SourceYield::ZERO],
            ..Default::default()
        };
        let runway = captured_runway(&cohort, Some(&scouting), None);
        assert!(runway > 0.0, "a missing schedule must never read as famine");
        assert!(
            (runway - TEST_LARDER / demand).abs() < EPSILON,
            "with no income the smooth estimate is larder / consumption: got {runway}"
        );
    }

    /// A cohort with no food demand at all keeps the historical zero-demand sentinel.
    #[test]
    fn a_cohort_with_no_demand_is_not_food_limited() {
        let mut empty = cohort(TEST_LARDER);
        empty.working = scalar_zero();
        empty.size = 0;
        assert_eq!(captured_runway(&empty, None, None), NOT_FOOD_LIMITED_DAYS);
    }
}
