use super::*;

pub(crate) fn pending_migration_to_state(migration: &PendingMigration) -> PendingMigrationState {
    PendingMigrationState {
        destination: migration.destination.0,
        eta: migration.eta,
        fragments: fragments_to_contract(&migration.fragments),
    }
}

/// Serialize one labor assignment for the client readout. The `yields` carry this turn's
/// actual/sustainable food income for the source (per-source breakdown; defaulted to `0` when the
/// telemetry row is absent, e.g. an assignment no `advance_labor_allocation` has resolved yet).
pub(crate) fn labor_assignment_to_state(
    assignment: &LaborAssignment,
    yields: &SourceYield,
    // **The RESOLVED job this row's source is being raised on** — see [`resolved_build_job`]. Passed
    // in rather than derived here because the resolution needs both webs' registries, which a row
    // has no business holding.
    build_job: String,
    // **The kit this row is priced at**, resolved by the caller — `LaborAssignment::kit_choice` for
    // every ordinary row, and the band's own builders resolution for the `builders` role.
    resolved_kit: crate::equipment_config::KitChoice,
    // **The crew beyond which more hands add nothing on this row, fight included** — see
    // [`assigned_hunt_useful_crew`], which is where it is answered. Passed in for `build_job`'s
    // reason: resolving it needs the herd registry and two configs a row has no business holding.
    hunt_useful_workers: u32,
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
        // The feed currency (#449) — the value the band's `FODDER` store was credited, published
        // verbatim so the compact readout can state a hay Field's whole product.
        fodder_yield: yields.fodder,
        // The material account (arc #527) — the amounts `credit_material_yield` actually deposited,
        // published verbatim so a cash Field's and a wolf hunt's rows state their whole product
        // instead of `+0.00`. Cloned rather than moved for `arrival_schedule`'s reason: the caller's
        // telemetry row is still read by the band roll-ups below.
        material_yield: yields
            .materials
            .iter()
            .map(|payoff| sim_runtime::MaterialPayoff {
                material_id: payoff.material.clone(),
                amount: payoff.amount,
            })
            .collect(),
        // **The band the scalar above sits in the middle of** (§6.4). A seeded row carries the
        // real distribution; a resolved row carries the point it paid.
        actual_yield_low: yields.range.low,
        actual_yield_high: yields.range.high,
        // **WHAT IS BEING RAISED ON THIS SOURCE**, `""` for a row with nothing queued. Written for
        // every kind, because a band-wide role never carries one.
        //
        // **It is DERIVED at capture from the band's build queue** (`docs/plan_standing_upkeep.md`
        // §2.4), which is the single authority: the row itself stopped carrying a verb when the
        // per-source build crew retired, so there is no second store for this to drift from.
        improvement: build_job,
        // **The kit this crew works under**, RESOLVED — the row's yields are priced at exactly it,
        // so the wire states the kit rather than "the player named none". **Every role now names
        // one**, including the two band-wide ones: this used to publish `""` for Scout/Warrior
        // because neither had a kit axis, and the roster giving them one is what changed.
        //
        // **The builders' resolution is handed in**, because theirs is the one default that is not
        // a property of the row: it is derived from the head queue entry's web, which a single
        // assignment cannot see.
        kit_id: resolved_kit.id().to_string(),
        // **HOW MANY HANDS THIS QUARRY CAN USE, FIGHT INCLUDED** — `0` on every non-hunt row, which
        // has no quarry to fight. Answered by the sim precisely because the client cannot: the
        // fight arm needs `combat_config.hit_chance`, which never crosses the wire.
        hunt_useful_workers,
        // **THE PLAYER'S OWN RANK ON THIS ROW**, captured live off the allocation exactly as the
        // other intent fields are — the mark is set at command time and the server re-captures after
        // every command, so it arrives on the command's own recapture with no optimistic overlay.
        // Mapped rather than cast: the wire puts the default at `0` and the shedding order does not.
        priority: match assignment.priority {
            SourcePriority::Normal => SourcePriorityState::Normal,
            SourcePriority::High => SourcePriorityState::High,
            SourcePriority::Low => SourcePriorityState::Low,
        },
        ..Default::default()
    };
    match &assignment.target {
        LaborTarget::Forage {
            tile,
            floor,
            species,
            take_species,
        } => {
            state.target_x = tile.x;
            state.target_y = tile.y;
            state.floor = *floor;
            state.species = species.clone().unwrap_or_default();
            // **WHICH PLANTS THIS CREW CARRIES HOME** — the take selection, published so it
            // round-trips (a compose sheet reopened on the row has no other way to show what the
            // crew was sent for) and in the collection's own ascending order, which is the only
            // order it has. **Empty is "take everything"**, the default, never "unknown".
            state.take_species = take_species.keys().map(str::to_string).collect();
        }
        LaborTarget::Hunt { fauna_id, floor } => {
            state.fauna_id = fauna_id.clone();
            state.floor = *floor;
        }
        // The five band-wide roles carry no source and no floor: their whole content is the head
        // count already on the row.
        LaborTarget::Scout
        | LaborTarget::Warrior
        | LaborTarget::Agriculture
        | LaborTarget::Husbandry
        | LaborTarget::Builders => {}
    }
    state
}

/// **THE CREW BEYOND WHICH MORE HANDS ADD NOTHING ON THIS ROW, *FIGHT INCLUDED*** — the sim's
/// answer to the Work board's `+` gate, published on the row so the board never derives it.
///
/// # It is the crew-take curve's plateau, and it is that number *by construction*
///
/// [`crate::fauna::hunt_useful_crew`] of [`crate::fauna::hunt_crew_take_curve`] — the very call
/// `forecast_query::answer_hunt_crew_take` makes to build the rows the compose sheet plateaus
/// itself. One producer, two transports: the sheet gets the curve because it is asking about a crew
/// it has not committed yet, and the board gets the scalar because it renders many rows a frame and
/// cannot round-trip for each of them.
///
/// **The defect this closes** is that the board's gate priced an already-worked row with no reply in
/// hand, so it divided the room by the *fightless* engagement reach — `body_mass × stayed`, with no
/// attack, no defense and no durability — and quoted a different ceiling from the compose sheet for
/// the same herd. On a fight-bound quarry that reads 2.3× high.
///
/// # The assignment fixes every term, which is why no ask is needed
///
/// The row already names the herd and the floor, the band already holds the kit and the wear, and
/// the reachable crew is the row's own hands plus the band's idle ones — the same pool the compose
/// sheet's stepper is capped at, and the same domain it asks its curve over.
///
/// [`crate::fauna::NO_USEFUL_CREW`] for every non-hunt row (no quarry to fight) and for a herd that
/// has left the registry.
fn assigned_hunt_useful_crew(
    target: &LaborTarget,
    // This source's own crew pool — the hands on the row plus the band's idle ones.
    crew_pool: u32,
    // The kit the row's yields are priced at, already resolved.
    kit: &crate::equipment_config::KitChoice,
    // The band's live wear ledger — the curve re-divides it per crew size, so a band with five
    // spears is quoted the mix it actually fields at each size.
    wear: &BandEquipment,
    kit_levers: &BandKitLevers<'_>,
    hunt_crew_levers: &HuntCrewLevers<'_>,
    herds: &crate::fauna::HerdRegistry,
) -> u32 {
    let LaborTarget::Hunt { fauna_id, floor } = target else {
        return crate::fauna::NO_USEFUL_CREW;
    };
    let Some(herd) = herds.find(fauna_id) else {
        return crate::fauna::NO_USEFUL_CREW;
    };
    crate::fauna::hunt_useful_crew(&crate::fauna::hunt_crew_take_curve(
        &crate::fauna::HuntCrewCurveInputs {
            herd,
            fauna: hunt_crew_levers.fauna,
            equipment: kit_levers.config,
            kit,
            wear,
            intrinsic: kit_levers.person_intrinsic,
            // **BASE, not `expedition_tuning`** — this is a band hunting its own range.
            tuning: hunt_crew_levers.combat.tuning(),
            hunt_injury_damage_per_animal: hunt_crew_levers.combat.hunt_injury_damage_per_animal,
            range_sigmas: hunt_crew_levers.combat.forecast_range_sigmas,
            floor: *floor,
            baseline_haul_rate: hunt_crew_levers.baseline_haul_rate,
            max_workers: crew_pool,
        },
    ))
}

/// Summarize a band's labor allocation into the `activity` string — the dominant assignment's kind,
/// or `"idle"`.
///
/// **The `hunt_mode` half is gone with the stances**: it named the take policy of the largest Hunt
/// assignment, and pressure is a per-source **floor** now (`LaborAssignmentState::floor`). One
/// band-wide string cannot summarise a continuous per-source dial, and rounding one to a label would
/// be the misdescription the floor's `""`-not-nearest rule exists to prevent.
fn allocation_summary(allocation: Option<&LaborAllocation>) -> String {
    allocation
        .into_iter()
        .flat_map(|allocation| allocation.assignments.iter())
        .filter(|a| a.workers > 0)
        .max_by_key(|a| a.workers)
        .map(|a| a.target.kind().to_string())
        .unwrap_or_else(|| "idle".to_string())
}

/// `turns_of_food` sentinel for a cohort that is **not food-limited** — no food demand at all (a
/// zero-population cohort), or income that meets or beats the drain so the larder never empties.
/// The client reads it as ∞.
///
/// **Public because it is the sentinel for BOTH runways** — `turns_of_food` and `turns_of_fodder`
/// are one concept in two currencies and share this one reading, so a test (or any consumer) names
/// the constant rather than the literal `999`.
pub const NOT_FOOD_LIMITED_TURNS: f32 = 999.0;

/// The larder runway, in **TURNS until the larder is empty** — the value the wire's `turnsOfFood`
/// carries.
///
/// **One formula, both actors.** `runway = larder / net drain`, `net drain = consumption − income`.
/// An **expedition** has no labor income, so it reduces to `provisions / consumption` — exactly the
/// historical reading, unchanged. A resident band with real income gets the honest number instead of
/// the "we stop gathering and hunting" pessimism the old `larder / demand` assumed.
///
/// **A band's PENS are not in the drain**, and were wrongly subtracted here while a pen drew the
/// larder: a pen eats grass and hay, so no number of animals shortens the people's runway.
///
/// Resolved the way the client's FOOD OUTLOOK chart resolves it, so the two cannot disagree by a
/// turn or two on the same panel:
/// 1. Walk the larder forward over the merged per-source **arrival schedules** (`arrivals[i]` = the
///    food landing `i + 1` turns from now), debiting `consumption` each turn and clamping at zero.
///    The first turn that reaches zero is the answer, counted from now.
/// 2. It never empties within the horizon (or no source was projected at all — an empty schedule
///    is "no data", never a famine): fall back to the smooth `larder / net_drain` on the **steady**
///    `realized` income, capped at the sentinel.
/// 3. `net_drain <= 0` (net-positive, not food-limited): the [`NOT_FOOD_LIMITED_TURNS`] sentinel.
pub(crate) fn larder_runway_turns(
    larder: f32,
    consumption: f32,
    steady_income: f32,
    arrivals: &[f32],
) -> f32 {
    let drain = consumption;
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
        return NOT_FOOD_LIMITED_TURNS;
    }
    (larder / net_drain).min(NOT_FOOD_LIMITED_TURNS)
}

/// The band-wide merged arrival schedule: element-wise sum of every source's `arrivals`, so slot
/// `i` is **all** the food landing `i + 1` turns from now (the client's `_merged_arrival_schedule`).
/// Empty when nothing was projected — Scout/Warrior only, or a source no turn has resolved yet.
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

/// The global expedition levers the snapshot echoes onto **every** cohort (resolved once per
/// capture, not per band) — the linear constants the client's **pre-launch hunt forecast**
/// multiplies against a herd's exported terms, so the outfit UI never re-derives the ecology model.
/// See `.claude/rules/core_sim/expeditions.md`.
///
/// **`max_estimated_party` is retired.** It echoed `estimate_party_sizes`' last rung — where the
/// pre-launch estimate tables stopped — and it capped nothing: the sim never read it, and all four
/// client sites that mention it say so in capitals ("IS NOT A RULES CAP AND MUST NOT BE APPLIED
/// HERE"). The tables are gone and the ladder with them, so the echo went too.
pub(crate) struct ExpeditionLevers {
    pub(crate) hunt_per_worker_carry: f32,
    /// `expedition_config.trade.per_worker_carry` — one person's **shipment** pack, echoed per-cohort
    /// so the outfit UI can price a manifest for a party that **does not exist yet**. That is why it
    /// is a global echo and not a per-party field: the player builds the shipment *before* there is a
    /// party to read a cap off. Same idiom as [`Self::hunt_per_worker_carry`] beside it, and a
    /// deliberately **separate** number — a shipment's pack and a raid's are different packs, and a
    /// client reaching for the hunt lever is one config edit away from quoting a cap the sim refuses.
    pub(crate) trade_per_worker_carry: f32,
    /// `expedition_config.trade.material_carry_weight` — what one unit of a material costs in pack
    /// space relative to one unit of food, so the cargo picker can run the **same** mass expression
    /// the launch command checks: `food + this × Σ material amounts`.
    ///
    /// **It ships because the sim otherwise refuses a manifest on a rule the client cannot
    /// evaluate.** Without it the picker is a guessing game — the player adds hide rows one at a
    /// time against a cap meter that cannot move, and finds out on submit. The refusal stays the
    /// authority; the meter is what stops the player ever meeting it.
    pub(crate) trade_material_carry_weight: f32,
    pub(crate) hunt_per_worker_provisions: f32,
    pub(crate) hunt_viability_warn_turns: u32,
    /// `expedition_config.hunt.forecast_horizon_turns` — how far *every* raid projection in the
    /// snapshot was simulated before giving up, echoed per-cohort so the client has a scale for the
    /// horizon-relative `0` sentinels (`turns_to_fill`, `turns_to_collapse*`) and for the
    /// `"horizon"` trip bound. The same lever drives the hunt and denial forecasts, so this one echo
    /// answers for both. **Not a trip length** — see
    /// [`sim_schema::state::PopulationCohortState::expedition_forecast_horizon_turns`].
    pub(crate) hunt_forecast_horizon_turns: u32,
    /// `labor_config.band_move_tiles_per_turn` — a band's move speed, echoed per-cohort so the client
    /// can add a raid's round-trip travel (`ceil(2 × hex_distance / this)`) to the band-agnostic
    /// pre-launch `huntTripEstimates`. Same global-config-surfaced-per-band idiom as the others.
    pub(crate) band_move_tiles_per_turn: u32,
    /// `expedition_config.settle.min_founding_workers` — the working-age floor the **new** band must
    /// clear, echoed per-cohort so the compose sheet can name the number (and word its own refusal)
    /// instead of keeping a second copy of the config. Same idiom as the four above.
    pub(crate) settle_min_founding_workers: u32,
    /// `expedition_config.settle.parent_min_workers` — the twin floor on what the **parent** keeps.
    /// Both floors ship because both are evaluated at the split and reported together.
    pub(crate) settle_parent_min_workers: u32,
}

/// **The TOE levers a cohort's kit readout is resolved against** — the config, plus the *equipped*
/// tiers that live outside `equipment.json` (one home per fact): the bare-handed `person` profile
/// from `creatures.json` and the kitted rates from `labor_config.json`. Bundled so the resolution
/// happens in exactly one place ([`population_state`]) rather than at the capture site.
pub(crate) struct BandKitLevers<'a> {
    pub(crate) config: &'a crate::equipment_config::EquipmentConfig,
    /// The base human's intrinsic combat profile — the *unequipped* attack tier, for a **hunter and
    /// a warrior alike**: `attack` is one stat and `creatures.json`'s `person` row is its one home,
    /// so both roles step up from this same number (`equipment_config::warrior_profile`).
    pub(crate) person_intrinsic: crate::combat::CombatStats,
    /// `labor_config.hunt.per_worker_biomass_capacity` — the **no-equipment** HUNT haul baseline,
    /// and the pen collection baseline with it. The *equipped* side of both lives on the sled's own
    /// tier now and is resolved through the item table, so a pen still shares the haul's one home.
    pub(crate) baseline_haul_rate: f32,
    /// `labor_config.forage.per_worker_biomass_capacity` — the **no-equipment** GATHER baseline.
    pub(crate) baseline_gather_rate: f32,
    /// `labor_config.scout.vantage_range` — the *equipped* vantage sight range (the wayfinding
    /// gear's). Carried as `f32` because the effects axis is continuous; the reveal path rounds.
    pub(crate) equipped_vantage_range: f32,
}

/// **The two configs an assigned hunt row's useful-crew cap needs beyond the band's own gear** —
/// the take model's roster and the fight's severity dials. Bundled exactly as [`BandKitLevers`] is,
/// and for the same reason: the capture resolves them once and hands one reference down, rather than
/// threading two config borrows through every per-band call.
pub(crate) struct HuntCrewLevers<'a> {
    /// The species roster the engagement, the retreat and the quarry's body all resolve through.
    pub(crate) fauna: &'a FaunaConfig,
    /// The severity dials. **The BASE tuning is what a resident band's row is priced at** — a hunt
    /// on the band's own range is not a raid, and `expedition_tuning` differs by half again in the
    /// fight term.
    pub(crate) combat: &'a crate::combat_config::CombatConfig,
    /// `labor_config.hunt.per_worker_biomass_capacity`, the bare carry rate both animal collection
    /// tiers are resolved against. **A CORRALLED row needs it and a stalked one does not** — a pen
    /// is collected rather than fought, so its curve's crew term is the keepers' throughput; see
    /// `fauna::pen_crew_take_curve`.
    pub(crate) baseline_haul_rate: f32,
}

pub(crate) struct PopulationStateInputs<'a> {
    pub(crate) entity: Entity,
    /// The band's durable id, published so a client can address it in a command without sending
    /// back an ECS handle that the next rollback renumbers.
    pub(crate) band_id: Option<&'a BandId>,
    pub(crate) cohort: &'a PopulationCohort,
    pub(crate) allocation: Option<&'a LaborAllocation>,
    pub(crate) expedition: Option<&'a Expedition>,
    pub(crate) current_position: Option<UVec2>,
    pub(crate) is_traveling: bool,
    pub(crate) demographics: &'a DemographicsConfig,
    pub(crate) wellbeing: &'a crate::wellbeing_config::WellbeingConfig,
    pub(crate) supply_membership: &'a SupplyNetworkMembership,
    pub(crate) work_range: u32,
    /// Echo of `fauna.predators.raid_radius` — surfaced per-cohort exactly like `work_range` (a global
    /// lever the client needs per-band to check whether a visible aggressive predator is in raid range).
    pub(crate) raid_radius: u32,
    pub(crate) scout_vantage_distance: u32,
    pub(crate) expedition_levers: &'a ExpeditionLevers,
    pub(crate) settlement_stage_config: &'a crate::settlement_stage_config::SettlementStageConfig,
    pub(crate) travel_target: Option<UVec2>,
    pub(crate) hunt_reach: u32,
    pub(crate) expedition_delivery: Option<crate::systems::ExpeditionDelivery>,
    /// The band's kit ledger (the minimal TOE). `None` = the ledger was never built (a hand-rolled
    /// fixture), which reads as a **start-stocked** band — the state every spawn path inserts.
    /// **Not `Default`**, which is an empty ledger owning nothing; an absent *entry inside* a ledger
    /// is what "not owned" looks like. See [`BandEquipment`].
    pub(crate) equipment: Option<&'a BandEquipment>,
    pub(crate) kit_levers: &'a BandKitLevers<'a>,
    /// The levers behind each hunt row's `hunt_useful_workers` — see [`HuntCrewLevers`].
    pub(crate) hunt_crew_levers: &'a HuntCrewLevers<'a>,
    /// **What is on this band's bench.** `None` = no component at all (a hand-rolled fixture), which
    /// reads as an idle bench — the same fallback `equipment` takes.
    pub(crate) bench: Option<&'a crate::components::BandBench>,
    /// The crafting readout's levers, bundled exactly as [`BandKitLevers`] is: the two configs, the
    /// per-recipe plan resolved once for the whole capture, and this faction's known crafts. See
    /// `snapshot::crafting` for why the recipe-only half is hoisted out of the per-band pass.
    pub(crate) craft_inputs: &'a crate::snapshot::crafting::BandCraftInputs<'a>,
    /// **What each queued source is actually being raised** — the three the `improvement` token is
    /// derived from at capture (`docs/plan_standing_upkeep.md` §2.4: the sim answers, the client does
    /// no arithmetic). A declaration answers only for a meter at **zero**, so the token a row
    /// publishes has to be resolved against the ground, not read off the entry.
    pub(crate) build_sources: &'a BuildSourceInputs<'a>,
}

/// The two webs' registries, for resolving a queue entry's **live** rung. No ladder: both
/// `patch_build_verb` and `herd_build_verb` derive the rung from the source's own meters, so the
/// config has nothing to say here.
pub(crate) struct BuildSourceInputs<'a> {
    pub(crate) forage: &'a crate::forage::ForageRegistry,
    pub(crate) herds: &'a crate::fauna::HerdRegistry,
}

/// **THE JOB TOKEN A ROW PUBLISHES** — the rung this band's queue entry for `source` is actually
/// raising, or `""` when the band has no entry for it.
///
/// **Resolved, not declared.** `BuildJob::Rung` is the player's declaration and it answers only
/// while the meter it names is at zero; `patch_build_verb` / `herd_build_verb` derive the live rung
/// from the meters otherwise. An entry on ground that has moved on therefore publishes `""` — it is
/// **dead**, which is what the countdown's `-1` beside it says too.
///
/// A ring names no rung (a built pen has no meter for a verb to name), so it publishes the command's
/// own name.
fn resolved_build_job(
    target: &LaborTarget,
    allocation: &LaborAllocation,
    sources: &BuildSourceInputs<'_>,
) -> String {
    let Some(source) = crate::components::BuildSource::of(target) else {
        return String::new();
    };
    let Some(entry) = allocation.build_queue_entry(&source) else {
        return String::new();
    };
    match (&entry.declared, &source) {
        (crate::components::BuildJob::ExtendPen, _) => {
            crate::systems::labor::EXTEND_PEN_ACTION.to_string()
        }
        (
            crate::components::BuildJob::Rung(declared),
            crate::components::BuildSource::Patch(tile),
        ) => sources
            .forage
            .patch(*tile)
            .and_then(|patch| crate::forage::patch_build_verb(patch, Some(*declared)))
            .map(|improvement| improvement.as_str().to_string())
            .unwrap_or_default(),
        (crate::components::BuildJob::Rung(declared), crate::components::BuildSource::Herd(id)) => {
            sources
                .herds
                .find(id)
                .and_then(|herd| crate::fauna::herd_build_verb(herd, Some(*declared)))
                .map(|improvement| improvement.as_str().to_string())
                .unwrap_or_default()
        }
    }
}

/// **EMPTY REGISTRIES, for a fixture that asserts on a band's derived readouts** — the two webs'
/// `BuildSourceInputs` when no source is seeded.
///
/// A queue entry on a source neither registry carries resolves to `""`, which is the honest answer:
/// the sim cannot say what is being raised on ground it does not have. Fixtures that assert on the
/// **job token** seed real registries instead.
#[cfg(test)]
pub(crate) fn empty_build_sources() -> &'static BuildSourceInputs<'static> {
    use std::sync::OnceLock;
    static FORAGE: OnceLock<crate::forage::ForageRegistry> = OnceLock::new();
    static HERDS: OnceLock<crate::fauna::HerdRegistry> = OnceLock::new();
    static INPUTS: OnceLock<BuildSourceInputs<'static>> = OnceLock::new();
    INPUTS.get_or_init(|| BuildSourceInputs {
        forage: FORAGE.get_or_init(Default::default),
        herds: HERDS.get_or_init(Default::default),
    })
}

/// **THE SHIPPED ROSTER AND DIALS, for a fixture that asserts on a band's derived readouts** — the
/// [`HuntCrewLevers`] every capture path resolves out of the loaded configs. Built once, exactly as
/// [`empty_build_sources`] is.
#[cfg(test)]
pub(crate) fn builtin_hunt_crew_levers() -> &'static HuntCrewLevers<'static> {
    use std::sync::OnceLock;
    static FAUNA: OnceLock<std::sync::Arc<FaunaConfig>> = OnceLock::new();
    static COMBAT: OnceLock<std::sync::Arc<crate::combat_config::CombatConfig>> = OnceLock::new();
    static LEVERS: OnceLock<HuntCrewLevers<'static>> = OnceLock::new();
    static LABOR: OnceLock<std::sync::Arc<crate::labor_config::LaborConfig>> = OnceLock::new();
    LEVERS.get_or_init(|| HuntCrewLevers {
        fauna: FAUNA.get_or_init(FaunaConfig::builtin),
        combat: COMBAT.get_or_init(crate::combat_config::CombatConfig::builtin),
        baseline_haul_rate: LABOR
            .get_or_init(crate::labor_config::LaborConfig::builtin)
            .hunt
            .per_worker_biomass_capacity,
    })
}

pub(crate) fn population_state(inputs: PopulationStateInputs<'_>) -> PopulationCohortState {
    let PopulationStateInputs {
        entity,
        band_id,
        cohort,
        allocation,
        expedition,
        current_position,
        is_traveling,
        demographics,
        wellbeing,
        supply_membership,
        work_range,
        raid_radius,
        scout_vantage_distance,
        expedition_levers,
        settlement_stage_config,
        travel_target,
        hunt_reach,
        expedition_delivery,
        equipment,
        kit_levers,
        hunt_crew_levers,
        bench,
        craft_inputs,
        build_sources,
    } = inputs;
    // **The minimal TOE, resolved for the wire.** An absent component means the ledger was never
    // built, which reads as **start-stocked** — the same fallback `advance_labor_allocation`,
    // `advance_expeditions` and the party-wear site in `capture.rs` take, and it has to be, or this
    // band would be published owning nothing while the labor pass pays it an equipped rate.
    // `Default` is the *empty* ledger and would say exactly that. Durability and performance stay
    // ORTHOGONAL: the tiers below are read off each kit's equipped/dry *predicate*, never scaled by
    // the remaining condition. **Three kits, three independent readouts** (§4.8): the sled's tier
    // says nothing about the basket's, so they are published as separate fields rather than one
    // "carry" number the client would have to guess the job of.
    let kit = equipment
        .cloned()
        .unwrap_or_else(|| BandEquipment::start_stocked(kit_levers.config));
    // **WHICH kit these tiers are quoted for.** A detached party has one, decided at launch, so its
    // row states the tier it will actually fight and haul at. A **resident band** has one per
    // assignment, and this row is per *cohort* — so it is quoted at the job's **default** kit, the
    // same reading the per-herd estimate tables take. The per-assignment truth rides
    // `LaborAssignmentState::kit_id` and that row's own yields.
    //
    // **The choices diverge for a band and coincide for a party**, which is why only the hunt one is
    // published (see `kit_id` below): a party carries one kit across every job, a band does not.
    //
    // **One choice per JOB, not one per tier.** A resident band's tier is quoted at the default of
    // the job that tier belongs to — the pen is a Hunt row, so it shares `hunt_choice`; the vantage
    // and the warrior's `attack` get the Scout and Warrior defaults, which exist only because the
    // expanded roster gave those two roles a kit axis.
    let job_choice = |job| {
        expedition
            .map(|exp| exp.kit.clone())
            .unwrap_or_else(|| kit_levers.config.default_kit(job))
    };
    let hunt_choice = job_choice(crate::equipment_config::KitJob::Hunt);
    let forage_choice = job_choice(crate::equipment_config::KitJob::Forage);
    let scout_choice = job_choice(crate::equipment_config::KitJob::Scout);
    let warrior_choice = job_choice(crate::equipment_config::KitJob::Warrior);
    // **One row per ITEM the config carries, not three named floats.** The list is driven by the
    // *config* rather than by the band's sparse ledger, so an item the band has never held still has
    // a row — it reads `count 0` rather than going missing.
    //
    // **`count` is what stops a client inferring ownership from a condition of zero.** Since the
    // count slice an absent entry is NOT OWNED, so `remaining` is `0` for an item the band has none
    // of — the same `0` a reader used to take for *"dry"*. Which of the two it is now rides beside
    // it, and *worn out* versus *never made* rides on `equipment_batches`.
    // **HOW EACH JOB'S GEAR DIVIDES ITS PEOPLE** (`equipment.md` → "the partly-equipped party"),
    // through the same `coverage` seam the take runs through rather than a second path to the same
    // answer. One per job, because the head count and the quoted kit are both per job.
    //
    // **An in-flight PARTY is all on one kit**, so every job's coverage is over the party's whole
    // head count: its `LaborAllocation` is empty (it works no sources), and reading a `0` off that
    // would publish an outfitted raid as holding nothing.
    let job_workers = |job| match expedition {
        Some(_) => available_workers(cohort.working),
        None => allocation.map_or(0, |alloc| alloc.workers_on_job(job)),
    };
    let coverage_for = |job, choice: &crate::equipment_config::KitChoice| {
        kit_levers
            .config
            .coverage(choice, job_workers(job) as f32, &kit)
    };
    let hunt_coverage = coverage_for(crate::equipment_config::KitJob::Hunt, &hunt_choice);
    // **All four, HUNT FIRST** — an item is quoted at the job whose kit carries it, and at the
    // hunt's for an item several of them carry (`kit_id`'s tie-break, the same one
    // `pen_carry_per_worker_biomass` follows).
    let quoted_coverages = [
        &hunt_coverage,
        &coverage_for(crate::equipment_config::KitJob::Forage, &forage_choice),
        &coverage_for(crate::equipment_config::KitJob::Scout, &scout_choice),
        &coverage_for(crate::equipment_config::KitJob::Warrior, &warrior_choice),
    ];
    let kit_item_conditions = kit_levers
        .config
        .items()
        .map(|(id, _)| {
            // **The job is chosen by WHICH QUOTED KIT CARRIES THE ITEM, not by which coverage
            // happens to hold somebody.** Both published numbers then come from that one coverage,
            // so the pair is one sentence — *"`workers_holding` of `workers_on_quoted_job`"* — and
            // cannot describe two different jobs. Picking the first *positive* holding instead
            // would leave the denominator undefined for the case that matters most: a staffed job
            // whose gear the band owns none of.
            let quoted = quoted_coverages
                .iter()
                .find(|coverage| coverage.kit().uses().any(|used| used == id));
            sim_schema::state::KitItemConditionState {
                item_id: id.to_string(),
                remaining: kit.remaining(id, kit_levers.config),
                count: kit.count_of(id),
                // An item no quoted kit carries — a bench tool, or a basket on a band running the
                // `none` forage kit — reads `0` on both, and `count` beside it is what tells that
                // from "the band owns none".
                workers_holding: quoted.map_or(0.0, |coverage| coverage.workers_holding(id)),
                // **The denominator, off the same coverage.** `0` here means *nobody is staffed on
                // that job* — a different sentence from a staffed job holding none of the item, and
                // a client must not divide by it.
                workers_on_quoted_job: quoted.map_or(0.0, |coverage| coverage.workers()),
            }
        })
        .collect();
    // **The party's runs, published as the sim resolved them.** Best-equipped first, workers summing
    // to the hunt head count. **Never empty**: a band with nobody on the hunt job still publishes one
    // row, at `workers 0` and the tier one hunter *would* be at — which is exactly what
    // `hunter_attack` below states for that band, so the two cannot disagree.
    let hunt_crews: Vec<_> = if hunt_coverage.crews().is_empty() {
        vec![sim_schema::state::BandKitCrewState {
            workers: 0.0,
            hunter_attack: kit_levers
                .config
                .hunter_profile_unbounded(kit_levers.person_intrinsic, &hunt_choice, &kit)
                .attack,
            item_ids: hunt_choice
                .uses()
                .filter(|item| hunt_choice.item_live(item, &kit, kit_levers.config))
                .map(str::to_string)
                .collect(),
        }]
    } else {
        hunt_coverage
            .crews()
            .iter()
            .map(|crew| sim_schema::state::BandKitCrewState {
                workers: crew.workers,
                hunter_attack: kit_levers
                    .config
                    .hunter_profile_unbounded(kit_levers.person_intrinsic, &crew.kit, &kit)
                    .attack,
                item_ids: crew.kit.uses().map(str::to_string).collect(),
            })
            .collect()
    };
    // **READ OFF THE BEST-EQUIPPED CREW, so the schema's promise is true by construction.** The
    // field's meaning is unchanged — it always was the tier the band's best-armed hunters fight at —
    // but deriving it from the crews is what stops the two rows drifting the day either resolution
    // moves. `hunt_crews` is never empty, so the index is safe.
    let hunter_attack = hunt_crews[0].hunter_attack;
    let hunt_carry_per_worker_biomass = kit_levers.config.hunt_per_worker_biomass_capacity(
        kit_levers.baseline_haul_rate,
        &hunt_choice,
        &kit,
    );
    let forage_carry_per_worker_biomass = kit_levers.config.forage_per_worker_biomass_capacity(
        kit_levers.baseline_gather_rate,
        &forage_choice,
        &kit,
    );
    // **The pen collects against the HUNT haul's equipped rate** — the number `advance_labor_allocation`
    // has always capped a pen harvest by — but through the `PenCarry` stat, so a Hunt row on the
    // stalking kit works the pen bare-handed rather than at the sled's tier.
    let pen_carry_per_worker_biomass = kit_levers.config.pen_per_worker_biomass_capacity(
        kit_levers.baseline_haul_rate,
        &hunt_choice,
        &kit,
    );
    let scout_vantage_range = kit_levers.config.scout_vantage_range(
        kit_levers.equipped_vantage_range,
        &scout_choice,
        &kit,
    );
    // **The same `attack` stat and the same seam the hunter's resolves through** — a weapon is a
    // weapon whichever role carries it, and what keeps a spear out of a raid is the kit's `jobs`
    // list. So this is *not* `hunter_attack` read twice: the two resolve through different kits.
    let warrior_attack = kit_levers
        .config
        .warrior_profile(kit_levers.person_intrinsic, &warrior_choice, &kit)
        .attack;
    // **WHAT EVERY OFFERED KIT WOULD GRANT THIS BAND, at its live wear** — the picker's real
    // numbers, resolved here so the client never steps a tier down for itself.
    //
    // It cannot do that correctly from the wire even in principle: stepping down needs the axis→item
    // mapping, which is per kit (`big_game` takes `attack` from `spears`, `trapping` from `traps`)
    // and is not recoverable from a kit's `item_ids`. Guessing repriced a band with fresh traps and
    // dry spears to the bare hand under `trapping`. Same shape as the retired estimate tables: a fact
    // the sim knows that the wire did not carry.
    //
    // **Through the same `resolve_kit_tiers` seam `kit_roster_states` uses** — that one over a fresh
    // ledger (the reference), this one over `kit`. One arithmetic, two readings, no third copy.
    let kit_tiers = kit_levers
        .config
        .kits()
        .iter()
        .filter_map(|definition| {
            let choice = kit_levers.config.kit(&definition.id)?;
            let tiers = kit_levers.config.resolve_kit_tiers(
                kit_levers.person_intrinsic,
                kit_levers.baseline_haul_rate,
                kit_levers.baseline_gather_rate,
                kit_levers.equipped_vantage_range,
                &choice,
                &kit,
            );
            Some(sim_schema::state::BandKitTiersState {
                kit_id: definition.id.clone(),
                attack: tiers.attack,
                hunt_carry_per_worker_biomass: tiers.hunt_carry_per_worker_biomass,
                forage_carry_per_worker_biomass: tiers.forage_carry_per_worker_biomass,
                attack_min_body_mass: tiers.attack_min_body_mass,
                attack_max_body_mass: tiers.attack_max_body_mass,
                dispersion: tiers.dispersion,
                exposure: tiers.exposure,
                // **The two axes the flat fields answer only at the JOB DEFAULT.** They ride here per
                // kit as well, because a picker asks about the kit under the cursor and a readout
                // that fell back to the roster's FRESH tier for them quoted a pen 40/keeper while the
                // sim collected 12, and a vantage of 2 tiles against a reveal at 1.
                pen_carry_per_worker_biomass: tiers.pen_carry_per_worker_biomass,
                scout_vantage_range: tiers.scout_vantage_range,
                // **The retired multiplier's slot, held at its neutral** — the stat is an
                // additive per-worker contribution now (`buildWorkPerWorker` beside it), and a
                // number in these units would read as a rate on a field the client renders as one.
                build_rate: sim_schema::RETIRED_BUILD_RATE,
                build_work_per_worker: tiers.build_work_per_worker,
                // **WHICH WEB THAT WORTH IS FOR**, `""` when the kit carries no build tool. The two
                // are one reading: a hoe takes nothing off a `Tame`, so a sheet that priced a build
                // off the worth alone would quote a saving the sim will never pay.
                build_work_branch: tiers
                    .build_work_branch
                    .map(|branch| branch.as_str().to_string())
                    .unwrap_or_default(),
                // **The gear term's other half** — how many of this band's workers this kit could
                // actually equip for a build, out of what the band holds. Resolved **beside**
                // `resolve_kit_tiers` rather than inside it, and deliberately: the tiers describe
                // *what a kit grants a worker* and are quoted over a fresh ledger by
                // `kit_roster_states`, where a unit count is not a fact about the kit at all. This
                // is a fact about **this band's ledger**, so it is answered only here.
                //
                // **At the kit's own branch**, so it caps the worth published beside it; a kit with
                // no build tool has no branch and saturates nobody.
                build_work_saturating_crew: tiers
                    .build_work_branch
                    .map(|branch| {
                        kit_levers
                            .config
                            .build_work_saturating_crew(&choice, &kit, branch)
                    })
                    .unwrap_or(crate::equipment_config::NO_SATURATING_CREW),
            })
        })
        .collect();
    let migration = cohort.migration.as_ref().map(pending_migration_to_state);
    let (travel_target_x, travel_target_y) = travel_target.map(|t| (t.x, t.y)).unwrap_or((0, 0));
    let demand = food_demand(
        cohort.children,
        cohort.working,
        cohort.elders,
        &demographics.consumption,
    );
    let activity = allocation_summary(allocation);
    // **The head-count, through the one seam the COMMANDS clamp against**
    // ([`crate::components::BandWorkforce`]). The bench's crew is spent labor that is not a
    // `LaborTarget`, so it is nowhere in `assigned`; publishing `working_age − assigned` counted
    // those hands as free, and every "n idle of m" readout in the game over-reported in the
    // reassuring direction — a compose sheet sized against it could not be staffed.
    let workforce = crate::components::BandWorkforce::resolve(Some(cohort), allocation, bench);
    // **The published age triple, in whole people** — the workers are the pool above (already
    // floored; never re-floored here), the dependents are what the head-count has left over. See
    // [`whole_age_brackets`]: the fractional brackets are a growth accumulator and are not published.
    let age_brackets = whole_age_brackets(
        cohort.size,
        workforce.pool,
        i128::from(cohort.children.raw()),
        i128::from(cohort.elders.raw()),
    );
    let working_age = age_brackets.working;
    let idle_workers = workforce.idle();
    // Zip each assignment with its retained per-source yield telemetry (same index order). An
    // assignment with no telemetry row yet → default 0 yields rather than a panic.
    const NO_YIELD: SourceYield = SourceYield::ZERO;
    let labor_assignments = allocation
        .map(|a| {
            a.assignments
                .iter()
                .enumerate()
                .map(|(i, assignment)| {
                    // **The builders row is the one whose default is not on the row.** Its kit is
                    // derived from the head queue entry's web, so a card reading this field states
                    // what the pool is holding this turn rather than a stored `none`.
                    let resolved_kit = match assignment.target {
                        LaborTarget::Builders => a.builders_kit(kit_levers.config),
                        _ => assignment.kit_choice(kit_levers.config),
                    };
                    // **The row's own useful-crew ceiling**, over the crew this source can actually
                    // reach: the hands standing on it plus the band's idle ones. That is the pool
                    // `assign_labor` judges an add against and the domain the compose sheet asks its
                    // curve over, so the two surfaces answer the same question.
                    let hunt_useful_workers = assigned_hunt_useful_crew(
                        &assignment.target,
                        assignment.workers.saturating_add(idle_workers),
                        &resolved_kit,
                        &kit,
                        kit_levers,
                        hunt_crew_levers,
                        build_sources.herds,
                    );
                    labor_assignment_to_state(
                        assignment,
                        a.last_yields.get(i).unwrap_or(&NO_YIELD),
                        resolved_build_job(&assignment.target, a, build_sources),
                        resolved_kit,
                        hunt_useful_workers,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    // Band-level food flow: income = Σ per-source actual yield; consumption is the food the people
    // ACTUALLY ate this turn (`cohort.last_food_consumption`, the real `stores` debit at the turn's
    // opening brackets), NOT a `food_demand` re-derived here on the post-turn brackets — the same
    // turn's births would inflate that and break the larder ledger identity by exactly the growth.
    // (`demand` above stays post-turn for `turns_of_food`, which is a forward "turns I can last".)
    let food_income = allocation
        .map(|a| a.last_yields.iter().map(|y| y.actual).sum())
        .unwrap_or(0.0);
    // The **steady** income = Σ per-source `realized` (the honest long-run average of the lumpy
    // `actual`). Distinct from `food_income` above precisely on whole-animal sources, where `actual`
    // pulses (0 on wait turns, spikes on kills) while `realized` holds steady, so it is the right
    // income term for the forward-looking runway below — `turns_of_food` must not swing with the
    // pulses. Purely local: it is no longer exported, because the client sums the same quantity from
    // the per-source `realized_yield` of the breakdown rows so its headline cannot disagree with the
    // rows it sits above (see `core_sim/CLAUDE.md`).
    let steady_food_income = allocation
        .map(|a| a.last_yields.iter().map(|y| y.realized).sum())
        .unwrap_or(0.0);
    let food_consumption = cohort.last_food_consumption;
    // The food this band forfeited to a predator raid this turn (the real `LocalStore::take` debit
    // `advance_predator_raids` levied on a casualty-causing raid). It is in NEITHER food term — a
    // negative ledger row the client draws separately — and it is derived per-turn by
    // `advance_predator_raids` (`0.0` on a band not raided this turn). It is the ledger's only
    // remaining third term: the retired `pen_feed_upkeep` beside it priced a pen's feed in the food
    // the *people* eat, which is not what an animal eats.
    let raid_forfeit = allocation.map(|a| a.last_raid_forfeit).unwrap_or(0.0);
    // The honest larder runway — turns until the larder empties, INCOME INCLUDED (the wire calls it
    // `turns_of_food`; see `larder_runway_turns`). Consumption is the forward `demand` above (what
    // the people will want to eat), not `last_food_consumption`: `demand` is always resolvable,
    // where the actual debit is `0` before a band's first turn and short of demand in a famine.
    let turns_of_food = if demand.raw() <= 0 {
        NOT_FOOD_LIMITED_TURNS
    } else {
        larder_runway_turns(
            cohort.stores.get(FOOD).to_f32(),
            demand.to_f32(),
            steady_food_income,
            &merged_arrival_schedule(allocation),
        )
    };
    // **THE HAY LEDGER, in fodder units** — the pens' unmet feed against the Fields' harvest, both
    // read off the allocation the way `raid_forfeit` is, and `0.0` for a band with no allocation at
    // all. A pen eats grass and hay and never the people's bread, so none of this touches a food
    // term: it is its own ledger beside its own store.
    //
    // **The need is the GAP the footprints leave, summed by the sim.** A client cannot sum it — herd
    // rows are fog-filtered, so a pen out of sight would silently leave a client-side total the band
    // still owes.
    let fodder_need = allocation.map(|a| a.last_fodder_need).unwrap_or(0.0);
    let fodder_income = allocation.map(|a| a.last_fodder_inflow).unwrap_or(0.0);
    // **The fodder runway, through the LARDER'S OWN function and the larder's own sentinel** — one
    // phrasing for one concept, so a client reads `turns_of_fodder` exactly as it reads
    // `turns_of_food` and never branches two ways on "turns of buffer left".
    //
    // **No arrival schedule.** The food runway walks per-source arrivals because a hunt lands in
    // lumps; hay is a Field's steady harvest into a stock, so the smooth `store ÷ net drain` arm is
    // the whole of it — and an empty schedule is exactly how that function is asked for that arm.
    //
    // **A band with nothing draining reads [`NOT_FOOD_LIMITED_TURNS`]**, which is the same ∞ a
    // well-fed larder publishes: no pens, or an income that meets the need, are both *not limited*
    // rather than a number of turns.
    let turns_of_fodder = larder_runway_turns(
        cohort.stores.get(FODDER).to_f32(),
        fodder_need,
        fodder_income,
        &[],
    );
    // Expedition discriminators + persistence fields (empty/false for a normal band).
    let (
        is_expedition,
        expedition_mission,
        expedition_phase,
        expedition_target_herd,
        expedition_target_species,
        expedition_floor,
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
            exp.mission.target_species().to_string(),
            exp.mission.hunt_floor(),
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
            // A resident band raids nothing, so it reports the floor that takes nothing — never `0`,
            // which would read as "take everything" if anything ever acted on it.
            NO_RAID_FLOOR,
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
    // **THIS PARTY'S PACK** = `party_workers × the per-worker carry of the pack its MISSION fills`
    // (`0` for a scout and for a normal band). The party's worker count is its working-age head-count.
    //
    // **Which lever fills it is a fact about the mission, not about expeditions.** A raid's pack is
    // measured in food it can haul home; a shipment's is measured in what its people can carry out.
    // They are different numbers on different levers, so the cap resolves per mission rather than
    // quoting one of them at every party — a client reading the hunt lever for a trade party would be
    // one config edit away from quoting a cap the launch command refuses.
    //
    // **A denial party has a pack too** — it does not clamp to carry, but it still hauls home
    // whatever it can (`docs/plan_denial_raid.md` §1), so its cap is the hunt's.
    let expedition_carry_cap = match expedition.map(|exp| &exp.mission) {
        Some(ExpeditionMission::Hunt { .. } | ExpeditionMission::Deny { .. }) => {
            working_age as f32 * expedition_levers.hunt_per_worker_carry
        }
        Some(ExpeditionMission::Trade { .. }) => {
            working_age as f32 * expedition_levers.trade_per_worker_carry
        }
        // A scout hauls nothing it was sent for, and a resident band is not a party.
        Some(ExpeditionMission::Scout) | None => 0.0,
    };
    // **The shipment a trade party is carrying** — read off `Expedition::cargo`, which is a store of
    // its own and deliberately *not* the party's pack (`cohort.stores`, published as `stores` and
    // `material_batches`): a hungry party must not be able to eat the goods it is hauling, and the
    // two accounts must not be readable as one on the wire either.
    //
    // **The material rows are per material id, never one total** — the arc's contract: a sum of hide
    // and bone is the retired trade axis under a new name. An empty vector is "no row", not zero.
    let (
        expedition_destination_band,
        expedition_destination_name,
        expedition_cargo_food,
        expedition_cargo_materials,
    ) = match expedition {
        Some(exp) if exp.mission.destination_band().is_some() => (
            exp.mission
                .destination_band()
                .map(|band| band.0)
                .unwrap_or_default(),
            // **The raw name, not the display fallback.** `destination_display` prints the band's
            // id when there is no name, which is right for the sim's own feed prose and wrong for a
            // wire field: the client already has a label for a band, and an id-shaped string would
            // fight it. Empty means "no name" — see `ExpeditionMission::Trade::destination_name`.
            exp.mission.destination_name().to_string(),
            exp.cargo.get(FOOD).to_f32(),
            exp.cargo
                .materials()
                .map(|(material, batches)| sim_runtime::MaterialPayoff {
                    material_id: material.to_string(),
                    amount: batches
                        .values()
                        .fold(crate::scalar::scalar_zero(), |total, batch| {
                            total + batch.amount
                        })
                        .to_f32(),
                })
                .collect(),
        ),
        _ => (0, String::new(), 0.0, Vec::new()),
    };
    // **The crafting half of the row, resolved together** — the four readouts share this band's
    // store, its ledger and its bench tiers, so resolving them apart would walk the same three
    // structures four times. See `snapshot::crafting`: the refusal is turned into words *here*, not
    // on the client.
    let crate::snapshot::crafting::BandCraftState {
        material_batches,
        bench: bench_state,
        craft_offers,
        equipment_batches,
    } = crate::snapshot::crafting::band_craft_state(&cohort.stores, bench, &kit, craft_inputs);
    PopulationCohortState {
        entity: entity.to_bits(),
        band_id: band_id.map(|id| id.0).unwrap_or_default(),
        home: cohort.home.to_bits(),
        current_x: current_position.map(|p| p.x).unwrap_or(0),
        current_y: current_position.map(|p| p.y).unwrap_or(0),
        is_traveling,
        // **The head-count is the sum of the parts published beneath it** — the client sees no other
        // one, so it must not come from a second rounding of its own (`cohort.size` caches
        // `round(children + working + elders)`, which can exceed the whole people that exist).
        size: age_brackets.head_count(),
        // The raw fixed-point brackets stay on the struct — `food_demand`, the fission split and the
        // JSON map export all read masses — but their FlatBuffers slots are `(deprecated)`: what the
        // wire carries is `children_count` / `working_age` / `elders_count`.
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
        turns_of_food,
        activity,
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
        expedition_floor,
        expedition_carry_cap,
        // Appended after every earlier-shipped field (append-only wire discipline; matches the
        // `.fbs` slot order for `expeditionTargetHerd`/`expeditionHuntPolicy`/`travelTargetX/Y`).
        expedition_target_herd,
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
        // Retired proximity readout: it published the faction stockpile to bands near the faction's
        // START position, which is not a rule the game has. The band-to-band radius that does exist
        // is `SupplyNetworkConfig.reach_tiles`, and it equalizes `stores` rather than publishing a
        // second store. The table stays in the schema (append-only) and always serializes absent.
        accessible_stockpile: None,
        settlement_stage,
        food_income,
        food_consumption,
        raid_forfeit,
        // Pre-launch hunt-forecast levers (global config, echoed onto every cohort — the outfit UI
        // reads them off the selected resident band).
        hunt_per_worker_provisions: expedition_levers.hunt_per_worker_provisions,
        expedition_viability_warn_turns: expedition_levers.hunt_viability_warn_turns,
        expedition_forecast_horizon_turns: expedition_levers.hunt_forecast_horizon_turns,
        expedition_per_worker_carry: expedition_levers.hunt_per_worker_carry,
        // **The SHIPMENT pack's lever, echoed onto every cohort** — the outfit UI prices a manifest
        // for a party that does not exist yet, so no per-party field can serve that screen. Same
        // idiom as the hunt lever above it.
        expedition_trade_per_worker_carry: expedition_levers.trade_per_worker_carry,
        // The second half of the mass expression, so the cargo picker runs the sim's own rule
        // rather than watching a meter that cannot move.
        expedition_trade_material_carry_weight: expedition_levers.trade_material_carry_weight,
        band_move_tiles_per_turn: expedition_levers.band_move_tiles_per_turn as f32,
        // In-flight hunt-party delivery forecast (`0`/false for a scout, a normal band, or a party
        // whose delivery can't be projected).
        expedition_eta_turns: expedition_delivery
            .as_ref()
            .and_then(|d| d.eta_turns)
            .unwrap_or(0),
        expedition_projected_delivery: expedition_delivery
            .as_ref()
            .map(|d| d.projected_food)
            .unwrap_or(0.0),
        expedition_recurring: expedition_delivery
            .as_ref()
            .map(|d| d.recurring)
            .unwrap_or(false),
        // Which stop will end THIS party's raid. `""` = not raiding at all (a resident band, a
        // scout, or a party already walking a load home) — never confused with `"horizon"`, which is
        // a projection that ran and found no stop.
        expedition_trip_bound: expedition_delivery
            .as_ref()
            .and_then(|d| d.trip_bound)
            .map(|bound| bound.as_str().to_string())
            .unwrap_or_default(),
        // The band's hay reserve (Flora Roster F3) — the FODDER key of the same `LocalStore` its
        // provisions ride, surfaced as a scalar so the client can show it beside the food reserve. It
        // also rides the full `stores` list above, but a named scalar spares the client a key lookup.
        fodder_store: cohort.stores.get(FODDER).to_f32(),
        // The three fertility factors this turn's births were actually resolved from — read off the
        // cohort, never re-derived here: they are computed on the turn's *opening* brackets and
        // *pre-meal* larder, so recomputing on the post-turn state would publish numbers that never
        // drove a birth. All-zero on a cohort that has not ticked (the not-projected sentinel).
        fertility_hunger: cohort.last_fertility_factors.hunger.raw(),
        fertility_reserve: cohort.last_fertility_factors.reserve.raw(),
        fertility_trend: cohort.last_fertility_factors.trend.raw(),
        // Predators Phase 3 — the raid legibility pair. `raid_radius` echoes the global lever
        // (like `work_range`); `raid_forfeit` is this band's past-turn raid debit (set above).
        raid_radius,
        // The minimal TOE — the three kits' remaining condition and the three tiers they resolve to
        // (resolved above, off the band's own wear).
        kit_item_conditions,
        kit_tiers,
        hunter_attack,
        hunt_carry_per_worker_biomass,
        forage_carry_per_worker_biomass,
        // **Which roster kit the HUNT tiers above are quoted at** — the party's own for an
        // expedition (a party has one kit, so it covers every tier on the row), the **hunt** job's
        // default for a resident band. `pen_carry_per_worker_biomass` below is a Hunt-row tier and
        // so is quoted at this id too.
        //
        // **It deliberately does not answer for the other three tiers.** `forage_choice` /
        // `scout_choice` / `warrior_choice` above are *different* kits for a band, so pairing
        // `forage_carry_per_worker_biomass` with this id would quote a gathering rate against
        // `big_game` (no basket component at all) and pairing `warrior_attack` with it would read a
        // warrior's tier off the hunt kit's spears. A per-tier `*_kit_id` field was considered and
        // rejected: each of those defaults already rides the wire once as
        // `SubsistenceSnapshot::default_{forage,scout,warrior}_kit_id`, and the per-crew truth is the
        // assignment row's own `kit_id`, so a per-cohort copy would be a third home for a fact that
        // has two. The `.fbs` states the narrowed scope for readers.
        kit_id: hunt_choice.id().to_string(),
        // The three tiers the expanded roster added, each resolved above through its own job's
        // default (the pen through the hunt's — see `job_choice`).
        pen_carry_per_worker_biomass,
        scout_vantage_range,
        warrior_attack,
        // **The two split floors, echoed off config.** The sheet composes its own forecast from
        // them (`sim_schema` → `founding_parent_min_workers` for why the verdict itself does not
        // cross), and `systems::fission::split_refusals` is the one rule set the command runs.
        founding_min_workers: expedition_levers.settle_min_founding_workers,
        founding_parent_min_workers: expedition_levers.settle_parent_min_workers,
        material_batches,
        bench: bench_state,
        craft_offers,
        equipment_batches,
        // The two derived halves of the published triple; `working_age` above is the third.
        children_count: age_brackets.children,
        elders_count: age_brackets.elders,
        // **The name of the quarry, beside the id that keys it.** Appended last (append-only wire
        // discipline) — the client renders this and joins on `expedition_target_herd` only for the
        // herd's *live position*, which is the one fact that genuinely needs live telemetry.
        expedition_target_species,
        hunt_crews,
        // **The shipment a trade party is carrying** (appended last). Zero/empty for every other
        // mission, including a resident band — a band's own store is not a shipment.
        expedition_destination_band,
        expedition_destination_name,
        expedition_cargo_food,
        expedition_cargo_materials,
        // The food ledger's last two terms — read off the allocation like `raid_forfeit` beside
        // them, and `0.0` for a band that has none. **These answer "what has
        // crossed since the last published frame"**, the window the ledger identity closes over, and
        // they are cleared right after this capture (`systems::reset_transfer_ledger`).
        transfer_received: allocation.map(|a| a.last_transfer_received).unwrap_or(0.0),
        transfer_sent: allocation.map(|a| a.last_transfer_sent).unwrap_or(0.0),
        // **And these answer "what crossed on this turn"** — the same two facts as per-turn state on
        // the cohort, copied there by `systems::publish_turn_transfers` just before the turn's
        // capture. A recapture rebuilds this frame from live components *after* the pair above has
        // been cleared, so it is these that survive one and these a client renders. On a turn frame
        // the two pairs read the same number.
        transfer_received_turn: cohort.last_turn_transfer_received,
        transfer_sent_turn: cohort.last_turn_transfer_sent,
        // **How this band splits a maintenance pool it cannot stretch** — the player's own choice
        // (`docs/plan_standing_upkeep.md` §2.5), published as the token the command takes so the two
        // are one language. A band with no allocation states the default rather than an empty
        // string: *"the sim did not say"* is a frame nobody should have to interpret.
        upkeep_fund_mode: allocation
            .map(|a| a.upkeep_fund_mode)
            .unwrap_or_default()
            .as_str()
            .to_string(),
        // **THE BAND'S OWN BUILD QUEUE, IN THE BAND'S OWN ORDER** (`docs/plan_standing_upkeep.md`
        // §4.9 item 9a) — the rank a client reads, where position is the vector INDEX.
        //
        // **It is the answer `ForagePatchState::build_queue_position` cannot give.** That one is
        // source-addressed and rides the *winning* band, so it states another band's place in
        // another band's line whenever two bands hold the source — which is ordinary. One int per
        // source cannot carry two bands' ranks; this list is per band and carries each.
        //
        // **Captured LIVE off the allocation, never turn-written** — the same discipline as
        // `build_kit_id` on the source rows. A `build_order` / `unqueue` / declaration mutates the
        // allocation at command time and `recapture_snapshot_in_place` re-reads it, so the new order
        // ships on that command's own frame and the client needs no optimistic ordering overlay.
        //
        // **Unfiltered and unsorted**: exactly what the band holds, in the band's order.
        // `prune_build_queue` is what keeps it honest, and re-deriving that here would be a second
        // producer of one verdict.
        build_queue: allocation
            .map(|alloc| {
                alloc
                    .build_queue
                    .iter()
                    .map(|entry| build_queue_entry_to_state(&entry.source))
                    .collect()
            })
            .unwrap_or_default(),
        // **THE BAND'S HAY LEDGER** — the fodder twins of `food_income` / `food_consumption` /
        // `turns_of_food` above, resolved here for the same reason they are: the client renders, it
        // does not sum. `fodder_need` is the roll-up the labor pass struck across every pen this
        // band keeps, `fodder_income` the raw harvest its Fields paid in, and the runway below is
        // the two against the store.
        fodder_need,
        fodder_income,
        turns_of_fodder,
    }
}

/// **Publish one build-queue entry's SOURCE** — the whole of what a row of
/// [`PopulationCohortState::build_queue`] carries.
///
/// The declared job, the kit, the destination rung and the estimate are all published on the
/// **source** row and agree across every band holding the source by construction, so an entry that
/// repeated them would be a second copy of a fact that already has a home.
fn build_queue_entry_to_state(source: &BuildSource) -> SchemaBuildQueueEntryState {
    let mut state = SchemaBuildQueueEntryState {
        // The same token the band's Forage/Hunt labor row publishes for this source
        // (`LaborTarget::kind`), so a client joins the two lists on one spelling.
        kind: source.kind().to_string(),
        ..Default::default()
    };
    match source {
        BuildSource::Patch(tile) => {
            state.target_x = tile.x;
            state.target_y = tile.y;
        }
        BuildSource::Herd(fauna_id) => fauna_id.clone_into(&mut state.fauna_id),
    }
    state
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

/// **A cohort's age brackets as WHOLE PEOPLE** — the only reading of them that crosses the wire.
///
/// The sim keeps the brackets in fixed point because the fraction is a *growth accumulator*: a slow
/// birth rate has to be able to add a tenth of a person a turn without rounding to nothing. That
/// fraction is not a fact about people, and it has exactly one correct resolution into people —
/// this one. Publishing the raw Scalars let a client invent a second: a band of 16.6 working-age
/// people rendered "17" in the PEOPLE bar beside "0 idle of 16" in the WORKFORCE header, the same
/// frame, off the same band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct WholeAgeBrackets {
    pub(crate) children: u32,
    /// The floored assignable pool — the *same* number `idle of N` counts against, never a
    /// separately rounded twin of it.
    pub(crate) working: u32,
    pub(crate) elders: u32,
}

impl WholeAgeBrackets {
    /// **The head-count the triple sits under, and the published `size`.** A total that is anything
    /// but the sum of its parts is the bug this whole derivation exists to prevent.
    pub(crate) fn head_count(self) -> u32 {
        self.children + self.working + self.elders
    }

    /// Add another cohort's triple in — the faction roll-up is a sum of *band* answers, so the
    /// faction page cannot disagree with the bands it aggregates.
    fn add(&mut self, other: Self) {
        self.children = self.children.saturating_add(other.children);
        self.working = self.working.saturating_add(other.working);
        self.elders = self.elders.saturating_add(other.elders);
    }
}

/// Round-half bias for an integer division: adding half the divisor to the numerator before
/// dividing rounds to nearest instead of truncating.
const ROUND_HALF_DIVISOR: i128 = 2;

/// **Resolve one cohort's fractional brackets into whole people** — the one arithmetic the band
/// panel and the faction roll-up both go through.
///
/// - `working_whole` is the *floored* `available_workers` (`BandWorkforce::pool`), the exact count
///   every command clamps against, so the published workers and the staffable workers are one
///   number. It is clamped to `head_count` belt-and-braces: `floor(working) ≤ round(total)` holds
///   for any brackets, so the clamp is inert today and only stops a future skew going negative.
/// - Dependents are what is left of the head-count, split between children and elders in proportion
///   to their fixed-point masses — round-half on children, elders taking the remainder, so the two
///   sum to the dependents *exactly* rather than each rounding on its own.
/// - **No dependent mass means no dependents.** With `children == elders == 0` and `working == 16.6`
///   the cached `size` is 17 while the workers floor to 16, and the leftover person is a rounding
///   artefact of the accumulator, not a person: putting them in `elders` invented an elder the sim
///   has no record of. The triple therefore reports `working` alone, and [`Self::head_count`] makes
///   the published size agree.
pub(crate) fn whole_age_brackets(
    head_count: u32,
    working_whole: u32,
    children_mass: i128,
    elders_mass: i128,
) -> WholeAgeBrackets {
    let working = working_whole.min(head_count);
    let dependents = i128::from(head_count - working);
    let children_mass = children_mass.max(0);
    let elders_mass = elders_mass.max(0);
    let dependent_mass = children_mass + elders_mass;
    if dependent_mass == 0 {
        return WholeAgeBrackets {
            children: 0,
            working,
            elders: 0,
        };
    }
    // i128 keeps the mass × head-count product overflow-free.
    let children =
        (dependents * children_mass + dependent_mass / ROUND_HALF_DIVISOR) / dependent_mass;
    WholeAgeBrackets {
        children: children as u32,
        working,
        elders: (dependents - children) as u32,
    }
}

/// Aggregate the per-cohort age brackets into a per-faction age structure for the HUD readout.
///
/// **It sums the bands' own published whole people** ([`whole_age_brackets`], resolved once per band
/// in [`population_state`]) rather than re-deriving anything from the fixed-point masses. One
/// derivation, so the faction page and the sum of the band panels agree by construction instead of
/// by two roundings happening to land together.
pub(crate) fn snapshot_demographics(
    cohorts: &[PopulationCohortState],
) -> Vec<SchemaPopulationDemographicsState> {
    let mut by_faction: std::collections::BTreeMap<u32, WholeAgeBrackets> =
        std::collections::BTreeMap::new();
    for cohort in cohorts {
        by_faction
            .entry(cohort.faction)
            .or_default()
            .add(WholeAgeBrackets {
                children: cohort.children_count,
                working: cohort.working_age,
                elders: cohort.elders_count,
            });
    }
    by_faction
        .into_iter()
        .map(|(faction, brackets)| SchemaPopulationDemographicsState {
            faction,
            children: brackets.children,
            working: brackets.working,
            elders: brackets.elders,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expedition_config::ExpeditionConfig;

    // Test-only since the restore path that shared them was deleted.
    use crate::components::{
        ExpeditionPhase, FertilityFactors, LocalStore, MoraleCause, MoraleContributions,
        SourcePriority,
    };
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};

    /// Enough larder that the runway is decided by the flows, not by an empty cupboard.
    const TEST_LARDER: f32 = 40.0;
    /// `f32` sums of `Scalar`-quantized values — a few ULPs of slack, no more.
    const EPSILON: f32 = 1e-4;

    /// The TOE levers, resolved off the builtins. The `EquipmentConfig` is parked in a `OnceLock` so
    /// the returned borrow is `'static` — the fixtures pass `&kit_levers()` as a temporary.
    fn kit_levers() -> BandKitLevers<'static> {
        static EQUIPMENT: std::sync::OnceLock<
            std::sync::Arc<crate::equipment_config::EquipmentConfig>,
        > = std::sync::OnceLock::new();
        let config = EQUIPMENT.get_or_init(crate::equipment_config::EquipmentConfig::builtin);
        BandKitLevers {
            config,
            person_intrinsic: crate::creatures_config::CreaturesConfig::builtin().person(),
            baseline_haul_rate: crate::labor_config::LaborConfig::builtin()
                .hunt
                .per_worker_biomass_capacity,
            baseline_gather_rate: crate::labor_config::LaborConfig::builtin()
                .forage
                .per_worker_biomass_capacity,
            equipped_vantage_range: crate::labor_config::LaborConfig::builtin()
                .scout
                .vantage_range as f32,
        }
    }

    fn levers() -> ExpeditionLevers {
        let cfg = ExpeditionConfig::builtin();
        ExpeditionLevers {
            hunt_per_worker_carry: cfg.hunt.per_worker_carry,
            trade_per_worker_carry: cfg.trade.per_worker_carry,
            trade_material_carry_weight: cfg.trade.material_carry_weight,
            hunt_per_worker_provisions: 0.0,
            hunt_viability_warn_turns: cfg.hunt.viability_warn_turns,
            hunt_forecast_horizon_turns: cfg.hunt.forecast_horizon_turns,
            band_move_tiles_per_turn: 1,
            settle_min_founding_workers: cfg.settle.min_founding_workers,
            settle_parent_min_workers: cfg.settle.parent_min_workers,
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
            last_turn_transfer_received: 0.0,
            last_turn_transfer_sent: 0.0,
            last_morale_delta: scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: MoraleContributions::default(),
            last_fertility_factors: Default::default(),
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

    /// Capture one cohort exactly as the snapshot does, and return its `turns_of_food`.
    fn captured_runway(
        cohort: &PopulationCohort,
        allocation: Option<&LaborAllocation>,
        expedition: Option<&Expedition>,
    ) -> f32 {
        captured(cohort, allocation, expedition).turns_of_food
    }

    /// Capture one cohort exactly as the snapshot does, and return the whole published state.
    fn captured(
        cohort: &PopulationCohort,
        allocation: Option<&LaborAllocation>,
        expedition: Option<&Expedition>,
    ) -> PopulationCohortState {
        population_state(PopulationStateInputs {
            entity: Entity::from_raw(1),
            // These fixtures assert on the derived readouts, not on band identity.
            band_id: None,
            cohort,
            allocation,
            expedition,
            current_position: None,
            is_traveling: false,
            demographics: &DemographicsConfig::builtin(),
            wellbeing: &crate::wellbeing_config::WellbeingConfig::builtin(),
            supply_membership: &SupplyNetworkMembership::default(),
            work_range: 0,
            raid_radius: 0,
            scout_vantage_distance: 0,
            expedition_levers: &levers(),
            settlement_stage_config:
                &crate::settlement_stage_config::SettlementStageConfig::builtin(),
            travel_target: None,
            hunt_reach: 0,
            expedition_delivery: None,
            // These fixtures assert on the food ledger, not the TOE.
            equipment: None,
            kit_levers: &kit_levers(),
            hunt_crew_levers: builtin_hunt_crew_levers(),
            // These fixtures assert on the food ledger, not the bench.
            bench: None,
            craft_inputs: crate::snapshot::crafting::builtin_craft_inputs(),
            build_sources: empty_build_sources(),
        })
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
                    floor: 0.5,
                },
                workers: 4,
                kit: None,
                priority: SourcePriority::default(),
            }],
            last_yields: vec![SourceYield {
                arrivals,
                realized,
                ..SourceYield::ZERO
            }],
            ..Default::default()
        }
    }

    /// The three fertility factors are published **verbatim from the cohort**, not re-derived at
    /// capture: they were resolved on the turn's *opening* brackets and *pre-meal* larder, so any
    /// recomputation here would publish numbers that never drove a birth. The cohort below carries a
    /// larder and brackets that would recompute to a *different* set, which is exactly what makes
    /// this a re-derivation guard rather than a restatement.
    #[test]
    fn the_capture_publishes_the_factors_that_actually_drove_the_births() {
        let mut cohort = cohort(TEST_LARDER);
        let factors = FertilityFactors {
            hunger: scalar_from_f32(0.6),
            reserve: scalar_from_f32(1.5),
            trend: scalar_from_f32(0.25),
        };
        cohort.last_fertility_factors = factors;
        let state = captured(&cohort, None, None);
        assert_eq!(state.fertility_hunger, factors.hunger.raw());
        assert_eq!(state.fertility_reserve, factors.reserve.raw());
        assert_eq!(state.fertility_trend, factors.trend.raw());
    }

    /// **The no-data rule on the wire.** A cohort that has not yet been through a turn has no
    /// reading — and it must publish the all-zero NOT-PROJECTED sentinel rather than a fabricated
    /// one. `reserve == 0` is what makes it unambiguous: a computed `reserve` is
    /// ≥ 1 by construction, while `hunger` and `trend` both legitimately reach 0. The client reads a
    /// zero reserve as "no reading", never as a famine.
    #[test]
    fn a_cohort_that_has_not_ticked_publishes_the_not_projected_sentinel() {
        let state = captured(&cohort(TEST_LARDER), None, None);
        assert_eq!(
            (
                state.fertility_hunger,
                state.fertility_reserve,
                state.fertility_trend
            ),
            (0, 0, 0),
            "a cohort that has not ticked must publish no reading, not a fabricated one"
        );
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
            pending_contacts: Default::default(),
            kit: crate::equipment_config::EquipmentConfig::builtin()
                .default_kit(crate::equipment_config::KitJob::Hunt),
            cargo: LocalStore::new(),
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
        assert_eq!(runway, NOT_FOOD_LIMITED_TURNS);
    }

    /// **An empty schedule is "no data", never a famine.** A cohort whose sources were not projected
    /// (Scout/Warrior only, or a band no turn has resolved yet) falls back to the smooth
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
                kit: None,
                priority: SourcePriority::default(),
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
        assert_eq!(captured_runway(&empty, None, None), NOT_FOOD_LIMITED_TURNS);
    }

    /// **The bug this arc exists to end.** A band of 16.6 working-age people has *sixteen* people
    /// who can be staffed; the cached `size` rounds their fixed-point sum to 17. Published raw, the
    /// client rendered "17" in the PEOPLE bar beside "0 idle of 16" in the WORKFORCE header — the
    /// same band, the same frame. The head count on the wire is now the sum of the whole brackets,
    /// and the 17th person — a rounding artefact of the growth accumulator — never appears.
    #[test]
    fn a_fractional_working_bracket_publishes_only_whole_people() {
        let mut cohort = cohort(TEST_LARDER);
        cohort.working = scalar_from_f32(16.6);
        cohort.sync_size();
        assert_eq!(
            cohort.size, 17,
            "the cached head count still rounds the masses"
        );

        let state = captured(&cohort, None, None);
        assert_eq!(
            state.working_age, 16,
            "the staffable pool is the floored bracket"
        );
        assert_eq!(
            state.size, 16,
            "the published head count is the whole people"
        );
        assert_eq!(
            state.children_count + state.working_age + state.elders_count,
            state.size,
            "the triple must sum to the head count it sits under"
        );
    }

    /// **No dependent mass means no dependents.** The leftover person of the case above is a
    /// remainder of the accumulator, not an elder: banking it in `elders` invented a person the sim
    /// has no record of, who ate nothing, worked nothing and could never die.
    #[test]
    fn a_cohort_with_no_dependent_mass_invents_no_elder() {
        let brackets = whole_age_brackets(17, 16, 0, 0);
        assert_eq!(brackets.children, 0);
        assert_eq!(brackets.working, 16);
        assert_eq!(brackets.elders, 0, "a phantom elder is not a person");
        assert_eq!(brackets.head_count(), 16);
    }

    /// The ordinary case: dependents split ∝ their masses, round-half on children, and the three
    /// sum **exactly** to the band's head count rather than each rounding independently.
    #[test]
    fn the_dependents_split_by_mass_and_sum_to_the_head_count() {
        let mut cohort = cohort(TEST_LARDER);
        cohort.children = scalar_from_f32(8.9);
        cohort.working = scalar_from_f32(16.5);
        cohort.elders = scalar_from_f32(4.6);
        cohort.sync_size();
        assert_eq!(cohort.size, 30);

        let state = captured(&cohort, None, None);
        // Dependents 30 − 16 = 14, split ∝ 8.9 : 4.6 → children round(9.23) = 9, elders the rest.
        assert_eq!(
            (state.children_count, state.working_age, state.elders_count),
            (9, 16, 5)
        );
        assert_eq!(state.size, 30);
    }

    /// An allocation holding `queue` as its build queue, with a take row on each source so the
    /// fixture is a band that could really have declared them.
    fn allocation_queueing(queue: &[BuildSource]) -> LaborAllocation {
        LaborAllocation {
            assignments: queue
                .iter()
                .map(|source| LaborAssignment {
                    target: match source {
                        BuildSource::Patch(tile) => LaborTarget::Forage {
                            tile: *tile,
                            floor: 0.5,
                            species: None,
                            take_species: crate::components::TakeSelection::EVERYTHING,
                        },
                        BuildSource::Herd(fauna_id) => LaborTarget::Hunt {
                            fauna_id: fauna_id.clone(),
                            floor: 0.5,
                        },
                    },
                    workers: 1,
                    kit: None,
                    priority: SourcePriority::default(),
                })
                .collect(),
            build_queue: queue
                .iter()
                .map(|source| crate::components::BuildQueueEntry {
                    source: source.clone(),
                    declared: crate::components::BuildJob::Rung(
                        crate::components::Improvement::Cultivate,
                    ),
                    kit: None,
                })
                .collect(),
            ..Default::default()
        }
    }

    /// The published queue as `(kind, x, y, fauna_id)` tuples, in the order the wire carries them.
    fn published_queue(state: &PopulationCohortState) -> Vec<(String, u32, u32, String)> {
        state
            .build_queue
            .iter()
            .map(|entry| {
                (
                    entry.kind.clone(),
                    entry.target_x,
                    entry.target_y,
                    entry.fauna_id.clone(),
                )
            })
            .collect()
    }

    /// **TWO BANDS SHARING ONE SOURCE PUBLISH TWO DIFFERENT ORDERS, and each publishes its own**
    /// (`docs/plan_standing_upkeep.md` §4.9 item 9a).
    ///
    /// This is the defect pinned rather than merely fixed. `ForagePatchState::build_queue_position`
    /// is **one int per source**, written by whichever band has the sooner estimate — so with band B
    /// holding `Y` second and band C holding it first, that int can state `1` or `0` and *cannot
    /// state both*. Whichever it states, the other band's queue block draws a list that is not its
    /// own, and the drag gesture computes its insert index off that wrong list. The per-band vector
    /// carries both answers because there are two vectors.
    #[test]
    fn two_bands_sharing_a_source_each_publish_their_own_queue_order() {
        let cohort = cohort(TEST_LARDER);
        // **The tiles are deliberately out of key order.** B declares X, Y, Z in that order, and
        // their coordinates sort the other way — so a capture that published *any* order derived
        // from the sources rather than from the band (a global rank, the client's old tie-break on
        // the key string) lands on `[Y, Z, X]` and is caught. Equal-and-ascending coordinates would
        // make this fixture pass under exactly the defect it exists to forbid.
        let x = UVec2::new(3, 3);
        let y = UVec2::new(1, 1);
        let z = UVec2::new(2, 2);

        let band_b = allocation_queueing(&[
            BuildSource::Patch(x),
            BuildSource::Patch(y),
            BuildSource::Patch(z),
        ]);
        let band_c = allocation_queueing(&[BuildSource::Patch(y)]);

        let published_b = captured(&cohort, Some(&band_b), None);
        let published_c = captured(&cohort, Some(&band_c), None);

        assert_eq!(
            published_queue(&published_b),
            vec![
                ("forage".to_string(), x.x, x.y, String::new()),
                ("forage".to_string(), y.x, y.y, String::new()),
                ("forage".to_string(), z.x, z.y, String::new()),
            ],
            "band B publishes B's queue, in B's order"
        );
        assert_eq!(
            published_queue(&published_c),
            vec![("forage".to_string(), y.x, y.y, String::new())],
            "band C publishes C's queue, in C's order"
        );

        // …and the thing the retired signal cannot express: the SAME source ranks differently in
        // the two bands, so no single per-source int is a correct rank for both.
        let rank_of = |state: &PopulationCohortState, tile: UVec2| {
            state
                .build_queue
                .iter()
                .position(|entry| (entry.target_x, entry.target_y) == (tile.x, tile.y))
                .expect("the fixture band has this source queued")
        };
        assert_eq!(rank_of(&published_b, y), 1, "Y is second in B's line");
        assert_eq!(rank_of(&published_c, y), 0, "…and first in C's");
        assert_ne!(
            rank_of(&published_b, y),
            rank_of(&published_c, y),
            "ONE per-source int cannot carry both ranks — that is the defect, and the per-band \
             vector is what carries them"
        );
    }

    /// **A HERD ENTRY NAMES ITS HERD, a patch entry names its tile**, on the same `kind` vocabulary
    /// the band's own labor rows publish — so a client joins the queue to the rows on one spelling
    /// rather than two that happen to match.
    #[test]
    fn a_queue_entry_names_its_source_in_the_labor_row_vocabulary() {
        let cohort = cohort(TEST_LARDER);
        let tile = UVec2::new(4, 7);
        let allocation = allocation_queueing(&[
            BuildSource::Herd("aurochs-3".to_string()),
            BuildSource::Patch(tile),
        ]);
        let state = captured(&cohort, Some(&allocation), None);
        assert_eq!(
            published_queue(&state),
            vec![
                ("hunt".to_string(), 0, 0, "aurochs-3".to_string()),
                ("forage".to_string(), tile.x, tile.y, String::new()),
            ]
        );
        // The tokens are the labor rows' own, not a second copy that happens to spell the same.
        assert_eq!(
            state.build_queue[0].kind, state.labor_assignments[0].kind,
            "the herd entry and the Hunt row say the same word"
        );
        assert_eq!(
            state.build_queue[1].kind, state.labor_assignments[1].kind,
            "the patch entry and the Forage row say the same word"
        );
    }

    /// A band with no allocation at all publishes an empty queue, not a phantom entry.
    #[test]
    fn a_band_with_no_allocation_publishes_an_empty_queue() {
        let cohort = cohort(TEST_LARDER);
        assert!(captured(&cohort, None, None).build_queue.is_empty());
    }
}
