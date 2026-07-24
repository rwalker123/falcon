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
                    turnsOfFood: cohort.turns_of_food,
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
                    expeditionEtaTurns: cohort.expedition_eta_turns,
                    expeditionProjectedDelivery: cohort.expedition_projected_delivery,
                    expeditionRecurring: cohort.expedition_recurring,
                    // (`foodIncomeAverage` sits earlier on the wire but is `(deprecated)`, so flatc
                    // omits it from the generated Args — nothing to set.)
                    // The band's hay reserve (F3) — appended (append-only wire) after #165's trio.
                    fodderStore: cohort.fodder_store,
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
