//! Population-section FlatBuffers serialization.

use crate::codec::{create_known_fragments, FbBuilder};
use crate::state::population::{
    AccessibleStockpileEntryState, GenerationState, PopulationCohortState,
    PopulationDemographicsState,
};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_population_section<'a>(
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

pub(crate) fn serialize_population_section_delta<'a>(
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
                        // The second axis (issue #442). Absent rather than `""` when the crew is
                        // building nothing, matching how `policy`/`faunaId` treat an empty value.
                        let improvement = if assignment.improvement.is_empty() {
                            None
                        } else {
                            Some(builder.create_string(&assignment.improvement))
                        };
                        // An unprojected row ships no vector at all, so the client can tell "no
                        // schedule" from "a schedule of zeros" (a real famine forecast).
                        let arrival_schedule = if assignment.arrival_schedule.is_empty() {
                            None
                        } else {
                            Some(builder.create_vector(&assignment.arrival_schedule))
                        };
                        // The kit this crew works under. Absent rather than `""` on a band-wide
                        // role, matching how `improvement`/`faunaId` treat an empty value — the
                        // FlatBuffers default for an absent string is `""`, so the two readings
                        // coincide for a consumer.
                        let kit_id = if assignment.kit_id.is_empty() {
                            None
                        } else {
                            Some(builder.create_string(&assignment.kit_id))
                        };
                        fb::LaborAssignment::create(
                            builder,
                            &fb::LaborAssignmentArgs {
                                kind: Some(kind),
                                workers: assignment.workers,
                                targetX: assignment.target_x,
                                targetY: assignment.target_y,
                                faunaId: fauna_id,
                                actualYield: assignment.actual_yield,
                                sustainableYield: assignment.sustainable_yield,
                                workersNeeded: assignment.workers_needed,
                                wastedYield: assignment.wasted_yield,
                                overdraws: assignment.overdraws,
                                realizedYield: assignment.realized_yield,
                                arrivalSchedule: arrival_schedule,
                                tradeYield: assignment.trade_yield,
                                realizedTradeYield: assignment.realized_trade_yield,
                                // The band the two scalars above sit in the middle of — appended
                                // last, after `floor`, so the slots stay positional.
                                actualYieldLow: assignment.actual_yield_low,
                                actualYieldHigh: assignment.actual_yield_high,
                                tradeYieldLow: assignment.trade_yield_low,
                                tradeYieldHigh: assignment.trade_yield_high,
                                // The improvement axis — appended (append-only wire).
                                improvement,
                                // THE HARVEST FLOOR — where this crew stops, as a fraction of `K`.
                                // The authority `policy` is a label for; appended last.
                                floor: assignment.floor,
                                // THE KIT this crew is working under — appended last.
                                kitId: kit_id,
                                // THE FEED CURRENCY (#449) — the third account beside
                                // actual/trade, carried so a hay Field stops reading `+0.00`.
                                // Appended last.
                                fodderYield: assignment.fodder_yield,
                            },
                        )
                    })
                    .collect();
                Some(builder.create_vector(&entries))
            };
            // Always written: this cohort's tiers are always quoted at *some* roster kit, and a
            // consumer comparing a selection against an absent string would read every band as a
            // mismatch.
            let kit_id = builder.create_string(&cohort.kit_id);
            // **The TOE, one row per item.** Built before the cohort table like every other nested
            // vector: FlatBuffers forbids writing a child table while a parent is open.
            let kit_item_conditions = {
                let rows: Vec<_> = cohort
                    .kit_item_conditions
                    .iter()
                    .map(|condition| {
                        let item_id = builder.create_string(&condition.item_id);
                        fb::KitItemCondition::create(
                            builder,
                            &fb::KitItemConditionArgs {
                                itemId: Some(item_id),
                                remaining: condition.remaining,
                            },
                        )
                    })
                    .collect();
                builder.create_vector(&rows)
            };
            // **What each offered kit grants THIS band** — the resolved answer, so the client does no
            // tier stepping. Nested vector, built before the parent table like the one above.
            let kit_tiers = {
                let rows: Vec<_> = cohort
                    .kit_tiers
                    .iter()
                    .map(|tiers| {
                        let kit_id = builder.create_string(&tiers.kit_id);
                        fb::BandKitTiers::create(
                            builder,
                            &fb::BandKitTiersArgs {
                                kitId: Some(kit_id),
                                attack: tiers.attack,
                                huntCarryPerWorkerBiomass: tiers.hunt_carry_per_worker_biomass,
                                forageCarryPerWorkerBiomass: tiers.forage_carry_per_worker_biomass,
                                attackMinBodyMass: tiers.attack_min_body_mass,
                                attackMaxBodyMass: tiers.attack_max_body_mass,
                                dispersion: tiers.dispersion,
                                exposure: tiers.exposure,
                                penCarryPerWorkerBiomass: tiers.pen_carry_per_worker_biomass,
                                scoutVantageRange: tiers.scout_vantage_range,
                            },
                        )
                    })
                    .collect();
                builder.create_vector(&rows)
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
            // The NAME beside that key — absent rather than empty for a non-raiding cohort, the same
            // convention the id above follows.
            let expedition_target_species = if cohort.expedition_target_species.is_empty() {
                None
            } else {
                Some(builder.create_string(&cohort.expedition_target_species))
            };
            // `""` = "not raiding" (a resident band, a scout, a party walking a load home) — absent
            // rather than an empty string, the convention every discriminator above follows.
            let expedition_trip_bound = if cohort.expedition_trip_bound.is_empty() {
                None
            } else {
                Some(builder.create_string(&cohort.expedition_trip_bound))
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
                    bandId: cohort.band_id,
                    // THE RAID'S FLOOR — replaces the retired `expeditionHuntPolicy`.
                    expeditionFloor: cohort.expedition_floor,
                    // WHICH STOP the in-flight projection says will end this party's raid.
                    // (`expeditionFillTarget` is a retired `(deprecated)` slot — see `snapshot.fbs`.)
                    expeditionTripBound: expedition_trip_bound,
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
                    turnsOfFood: cohort.turns_of_food,
                    activity,
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
                    expeditionCarryCap: cohort.expedition_carry_cap,
                    // Appended after every earlier-shipped field (append-only wire discipline).
                    expeditionTargetHerd: expedition_target_herd,
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
                    expeditionEtaTurns: cohort.expedition_eta_turns,
                    expeditionProjectedDelivery: cohort.expedition_projected_delivery,
                    expeditionRecurring: cohort.expedition_recurring,
                    // (`foodIncomeAverage` sits earlier on the wire but is `(deprecated)`, so flatc
                    // omits it from the generated Args — nothing to set.)
                    // The band's hay reserve (F3) — appended (append-only wire) after #165's trio.
                    fodderStore: cohort.fodder_store,
                    // The birth path's itemized breakdown, the parallel of the morale contributions
                    // above (append-only wire discipline — these follow every earlier field).
                    fertilityHunger: cohort.fertility_hunger,
                    fertilityReserve: cohort.fertility_reserve,
                    fertilityTrend: cohort.fertility_trend,
                    // Predators Phase 3 — the raid legibility pair, appended after fodderStore.
                    raidRadius: cohort.raid_radius,
                    raidForfeit: cohort.raid_forfeit,
                    // The TOE's resolved tiers. The three fixed durability floats that used to sit
                    // here are DEPRECATED in the schema and replaced by `kitItemConditions` below —
                    // one row per item, so a config that adds an item needs no schema edit.
                    hunterAttack: cohort.hunter_attack,
                    huntCarryPerWorkerBiomass: cohort.hunt_carry_per_worker_biomass,
                    forageCarryPerWorkerBiomass: cohort.forage_carry_per_worker_biomass,
                    // The kit the two HUNT tiers above (and `penCarryPerWorkerBiomass` below) are
                    // resolved through — appended last.
                    kitId: Some(kit_id),
                    // The projections' horizon, so the client can put a number on their
                    // "never completed" sentinels — appended after the kit.
                    expeditionForecastHorizonTurns: cohort.expedition_forecast_horizon_turns,
                    kitItemConditions: Some(kit_item_conditions),
                    kitTiers: Some(kit_tiers),
                    // The remaining three resolved tiers, one per role the expanded roster gave a
                    // kit axis. Each answers for its OWN job's default on a resident band, so none
                    // of the three may be read against `kitId` except the pen (a Hunt row).
                    penCarryPerWorkerBiomass: cohort.pen_carry_per_worker_biomass,
                    scoutVantageRange: cohort.scout_vantage_range,
                    warriorAttack: cohort.warrior_attack,
                    // The two split floors — appended last. Both are always written; a zero
                    // `parent_min_workers` is a real setting ("the parent may give everything"),
                    // not an absent one.
                    foundingMinWorkers: cohort.founding_min_workers,
                    foundingParentMinWorkers: cohort.founding_parent_min_workers,
                    expeditionTargetSpecies: expedition_target_species,
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
