//! Population-section FlatBuffers serialization.

use crate::codec::{create_known_fragments, FbBuilder};
use crate::state::population::{
    AccessibleStockpileEntryState, GenerationState, MaterialShortfallState, PopulationCohortState,
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
                        // **Built before the parent table opens**, the ordinary FlatBuffers rule.
                        let material_yield = crate::codec::subsistence::create_material_payoffs(
                            builder,
                            &assignment.material_yield,
                        );
                        // The crop this crew asked for — `None` rather than an empty string, the
                        // `fauna_id`/`improvement` convention: an absent string is "no selection",
                        // which is what `""` means here.
                        let species = if assignment.species.is_empty() {
                            None
                        } else {
                            Some(builder.create_string(&assignment.species))
                        };
                        let kit_id = if assignment.kit_id.is_empty() {
                            None
                        } else {
                            Some(builder.create_string(&assignment.kit_id))
                        };
                        // The plants this crew carries home. **Absent rather than an empty vector**
                        // when the crew named none, the `species`/`faunaId` convention: an absent
                        // vector reads as empty, and empty *is* "the whole basket".
                        let take_species = if assignment.take_species.is_empty() {
                            None
                        } else {
                            let keys: Vec<_> = assignment
                                .take_species
                                .iter()
                                .map(|species| builder.create_string(species))
                                .collect();
                            Some(builder.create_vector(&keys))
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
                                // The band the two scalars above sit in the middle of — appended
                                // last, after `floor`, so the slots stay positional.
                                actualYieldLow: assignment.actual_yield_low,
                                actualYieldHigh: assignment.actual_yield_high,
                                // The improvement axis — appended (append-only wire).
                                improvement,
                                // THE HARVEST FLOOR — where this crew stops, as a fraction of `K`.
                                // The authority `policy` is a label for; appended last.
                                floor: assignment.floor,
                                // THE KIT this crew is working under — appended last.
                                kitId: kit_id,
                                // THE FEED CURRENCY (#449) — the second account beside
                                // `actualYield`, carried so a hay Field stops reading `+0.00`.
                                // Appended last.
                                fodderYield: assignment.fodder_yield,
                                // THE MATERIAL ACCOUNT (arc #527) — the third, and the only one a
                                // cash Field or an inedible quarry pays into. Appended last. An
                                // EMPTY vector is "no row", never "zero".
                                materialYield: Some(material_yield),
                                // `improvementWorkers` and `maintainWorkers` are both `(deprecated)`
                                // slots and are no longer written: the build and the keeping are
                                // band-level standing roles (`docs/plan_standing_upkeep.md` §2.5)
                                // and arrive as ordinary rows of this list. A reader that still
                                // inserts either key is publishing a per-source crew the sim has
                                // stopped having.
                                // THE CROP THIS CREW ASKED FOR — the player's stated intent, which
                                // the patch's own `committedSpecies` cannot stand in for: that one
                                // is set only once a crew has worked the ground. Appended last.
                                species,
                                // WHICH PLANTS THIS CREW CARRIES HOME — appended last. Empty is
                                // "the whole basket", which is what every crew took before this.
                                takeSpecies: take_species,
                                // **HOW MANY HANDS THIS QUARRY CAN USE, FIGHT INCLUDED** — the
                                // plateau of the sim's own crew-take curve, published so the Work
                                // board's `+` gate reads it instead of dividing by a fightless
                                // reach. `0` on every non-hunt row. Appended last.
                                huntUsefulWorkers: assignment.hunt_useful_workers,
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
            // The band's maintenance fund mode — a string, so a third mode needs no schema change.
            let upkeep_fund_mode = Some(builder.create_string(&cohort.upkeep_fund_mode));
            // **THE BAND'S OWN BUILD QUEUE, IN THE BAND'S OWN ORDER** (§4.9 item 9a) — the rank is
            // the index, so the vector is written verbatim and nothing here sorts or filters it.
            // **Absent rather than an empty vector** when the band has declared nothing, the
            // `laborAssignments`/`takeSpecies` convention: an absent vector reads as empty, and
            // empty *is* "this band is building nothing".
            // **Built before the parent table opens**, the ordinary FlatBuffers rule.
            let build_queue = if cohort.build_queue.is_empty() {
                None
            } else {
                let entries: Vec<_> = cohort
                    .build_queue
                    .iter()
                    .map(|entry| {
                        let kind = builder.create_string(&entry.kind);
                        // A forage entry names its tile and no herd — absent rather than `""`, the
                        // same reading `LaborAssignment::faunaId` takes.
                        let fauna_id = if entry.fauna_id.is_empty() {
                            None
                        } else {
                            Some(builder.create_string(&entry.fauna_id))
                        };
                        fb::BuildQueueEntryState::create(
                            builder,
                            &fb::BuildQueueEntryStateArgs {
                                kind: Some(kind),
                                targetX: entry.target_x,
                                targetY: entry.target_y,
                                faunaId: fauna_id,
                            },
                        )
                    })
                    .collect();
                Some(builder.create_vector(&entries))
            };
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
                                // **Ownership, stated.** `remaining == 0` means the band owns none,
                                // not "owns one that is dry" — a batch with no units left is
                                // removed — so no client has to infer ownership from a zero.
                                count: condition.count,
                                // **And how many people those units actually reach** — a unit arms
                                // one worker, so "owns 87 units" and "10 of 17 armed" are different
                                // facts and a client may not divide one into the other.
                                workersHolding: condition.workers_holding,
                                // **Its denominator**, so the pair is one sentence and only the
                                // hunt is not the one job with a head count on the wire.
                                workersOnQuotedJob: condition.workers_on_quoted_job,
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
                        // Which web this kit's build gear serves — `""` for a kit carrying none.
                        let build_work_branch = builder.create_string(&tiers.build_work_branch);
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
                                buildRate: tiers.build_rate,
                                buildWorkPerWorker: tiers.build_work_per_worker,
                                buildWorkSaturatingCrew: tiers.build_work_saturating_crew,
                                buildWorkBranch: Some(build_work_branch),
                            },
                        )
                    })
                    .collect();
                builder.create_vector(&rows)
            };
            // --- CRAFTING & MATERIALS ------------------------------------------------------------
            // All four built before the parent table, like every nested vector above: FlatBuffers
            // forbids writing a child table while a parent is open.
            //
            // **The refusal is already resolved into words here.** Nothing below is an input to a
            // client-side derivation — `reason`, `severity`, `outputGrade`, the shortfall numbers
            // and the life wording are the sim's answers, for the same reason `kitTiers` is.
            let material_batches = {
                let rows: Vec<_> = cohort
                    .material_batches
                    .iter()
                    .map(|batch| {
                        let material_id = builder.create_string(&batch.material_id);
                        let variety_name = builder.create_string(&batch.variety_name);
                        // The EXACT reading and its band name both — the band is the merge key and
                        // the panel's word, the value is what crafting actually reads.
                        let readings: Vec<_> = batch
                            .readings
                            .iter()
                            .map(|reading| {
                                let axis = builder.create_string(&reading.axis);
                                let band_name = builder.create_string(&reading.band_name);
                                fb::CharacteristicReading::create(
                                    builder,
                                    &fb::CharacteristicReadingArgs {
                                        axis: Some(axis),
                                        value: reading.value,
                                        bandName: Some(band_name),
                                    },
                                )
                            })
                            .collect();
                        let readings = builder.create_vector(&readings);
                        fb::MaterialBatchState::create(
                            builder,
                            &fb::MaterialBatchStateArgs {
                                materialId: Some(material_id),
                                amount: batch.amount,
                                readings: Some(readings),
                                varietyName: Some(variety_name),
                            },
                        )
                    })
                    .collect();
                builder.create_vector(&rows)
            };
            let bench = {
                let recipe_id = builder.create_string(&cohort.bench.recipe_id);
                let display_name = builder.create_string(&cohort.bench.display_name);
                let teaches = builder.create_string(&cohort.bench.teaches);
                let blocked_reason = builder.create_string(&cohort.bench.blocked_reason);
                // **Whether that reason is a fault or a prompt**, resolved sim-side beside it — a
                // crewless bench is the normal state after a Make and must not render as an alarm.
                let blocked_severity = builder.create_string(&cohort.bench.blocked_severity);
                let output_grade = builder.create_string(&cohort.bench.output_grade);
                let shortfalls = create_shortfalls(builder, &cohort.bench.shortfalls);
                // **The pile already withdrawn**, in the recipe's own input order — what a clear or
                // a swap will destroy, which the client cannot name from `drawn: bool` alone.
                let drawn_inputs: Vec<_> = cohort
                    .bench
                    .drawn_inputs
                    .iter()
                    .map(|input| {
                        let material_id = builder.create_string(&input.material_id);
                        fb::DrawnInput::create(
                            builder,
                            &fb::DrawnInputArgs {
                                materialId: Some(material_id),
                                amount: input.amount,
                            },
                        )
                    })
                    .collect();
                let drawn_inputs = builder.create_vector(&drawn_inputs);
                fb::BenchState::create(
                    builder,
                    &fb::BenchStateArgs {
                        recipeId: Some(recipe_id),
                        displayName: Some(display_name),
                        workers: cohort.bench.workers,
                        progress: cohort.bench.progress,
                        work: cohort.bench.work,
                        teaches: Some(teaches),
                        blockedReason: Some(blocked_reason),
                        shortfalls: Some(shortfalls),
                        itemsCompleted: cohort.bench.items_completed,
                        drawn: cohort.bench.drawn,
                        outputGrade: Some(output_grade),
                        ratePerTurn: cohort.bench.rate_per_turn,
                        drawnInputs: Some(drawn_inputs),
                        blockedSeverity: Some(blocked_severity),
                    },
                )
            };
            let craft_offers = {
                let rows: Vec<_> = cohort
                    .craft_offers
                    .iter()
                    .map(|offer| {
                        let recipe_id = builder.create_string(&offer.recipe_id);
                        let display_name = builder.create_string(&offer.display_name);
                        let group = builder.create_string(&offer.group);
                        let output_item_id = builder.create_string(&offer.output_item_id);
                        let reason = builder.create_string(&offer.reason);
                        let severity = builder.create_string(&offer.severity);
                        let output_grade = builder.create_string(&offer.output_grade);
                        let output_tier_name = builder.create_string(&offer.output_tier_name);
                        let owned_note = builder.create_string(&offer.owned_note);
                        let shortfalls = create_shortfalls(builder, &offer.shortfalls);
                        fb::CraftOffer::create(
                            builder,
                            &fb::CraftOfferArgs {
                                recipeId: Some(recipe_id),
                                displayName: Some(display_name),
                                group: Some(group),
                                outputItemId: Some(output_item_id),
                                available: offer.available,
                                reason: Some(reason),
                                severity: Some(severity),
                                shortfalls: Some(shortfalls),
                                outputGrade: Some(output_grade),
                                onBench: offer.on_bench,
                                outputTierName: Some(output_tier_name),
                                outputTierRank: offer.output_tier_rank,
                                ownedNote: Some(owned_note),
                            },
                        )
                    })
                    .collect();
                builder.create_vector(&rows)
            };
            let equipment_batches = {
                let rows: Vec<_> = cohort
                    .equipment_batches
                    .iter()
                    .map(|batch| {
                        let item_id = builder.create_string(&batch.item_id);
                        let tier_id = builder.create_string(&batch.tier_id);
                        let grade = builder.create_string(&batch.grade);
                        let quantum_noun = builder.create_string(&batch.quantum_noun);
                        let life = builder.create_string(&batch.life);
                        let life_severity = builder.create_string(&batch.life_severity);
                        fb::EquipmentBatchState::create(
                            builder,
                            &fb::EquipmentBatchStateArgs {
                                itemId: Some(item_id),
                                tierId: Some(tier_id),
                                grade: Some(grade),
                                count: batch.count,
                                remaining: batch.remaining,
                                quantaLeft: batch.quanta_left,
                                quantumNoun: Some(quantum_noun),
                                life: Some(life),
                                lifeSeverity: Some(life_severity),
                            },
                        )
                    })
                    .collect();
                builder.create_vector(&rows)
            };
            // **How the band's gear divides its hunters** — one row per crew, built before the
            // parent table like every other nested vector. Never empty: a uniform band is one row.
            let hunt_crews = {
                let rows: Vec<_> = cohort
                    .hunt_crews
                    .iter()
                    .map(|crew| {
                        let item_ids: Vec<_> = crew
                            .item_ids
                            .iter()
                            .map(|id| builder.create_string(id))
                            .collect();
                        let item_ids = builder.create_vector(&item_ids);
                        fb::BandKitCrew::create(
                            builder,
                            &fb::BandKitCrewArgs {
                                workers: crew.workers,
                                hunterAttack: crew.hunter_attack,
                                itemIds: Some(item_ids),
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
            // The destination NAME a shipment is bound for — absent rather than empty for every
            // non-trade cohort, the same convention the quarry's name above follows.
            let expedition_destination_name = if cohort.expedition_destination_name.is_empty() {
                None
            } else {
                Some(builder.create_string(&cohort.expedition_destination_name))
            };
            // The shipment's material rows. **Built before the parent table opens**, the ordinary
            // FlatBuffers rule; empty in, empty out, which is the "no row" reading the field's own
            // contract rests on.
            let expedition_cargo_materials = crate::codec::subsistence::create_material_payoffs(
                builder,
                &cohort.expedition_cargo_materials,
            );
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
                    // The raw fixed-point brackets are gone from here on purpose — see
                    // `childrenCount` / `eldersCount` at the end of these args.
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
                    // Crafting & materials — appended last. Always written: an absent vector and an
                    // empty one read the same to a consumer, and `bench` is a real state even when
                    // the bench is idle (`recipeId == ""`).
                    materialBatches: Some(material_batches),
                    bench: Some(bench),
                    craftOffers: Some(craft_offers),
                    equipmentBatches: Some(equipment_batches),
                    // **The age brackets in WHOLE PEOPLE** — appended last, and the only reading of
                    // them that crosses. The raw fixed-point `children`/`working`/`elders` slots are
                    // `(deprecated)` in the schema and are not written: the fraction is an internal
                    // growth accumulator, so a client rounding it invented a second answer beside
                    // `workingAge`. `childrenCount + workingAge + eldersCount == size`.
                    childrenCount: cohort.children_count,
                    eldersCount: cohort.elders_count,
                    expeditionTargetSpecies: expedition_target_species,
                    // The partly-equipped party — appended last. Always written, and never empty:
                    // a client must not have to tell "no crews" from "one crew holding nothing".
                    huntCrews: Some(hunt_crews),
                    // **The shipment a trade party is carrying** — appended last. The key and its
                    // display twin, then the two accounts a band store holds. All four are the
                    // absent/zero default for every other mission.
                    expeditionDestinationBand: cohort.expedition_destination_band,
                    expeditionDestinationName: expedition_destination_name,
                    expeditionCargoFood: cohort.expedition_cargo_food,
                    expeditionCargoMaterials: Some(expedition_cargo_materials),
                    // The food ledger's transfer pair — appended last. Always written; a `0` is a
                    // real reading ("nothing crossed"), not an absent one.
                    transferReceived: cohort.transfer_received,
                    transferSent: cohort.transfer_sent,
                    // One person's shipment pack — appended last. Always written: it is a global
                    // lever, so every cohort carries the same positive number.
                    expeditionTradePerWorkerCarry: cohort.expedition_trade_per_worker_carry,
                    // The other half of a shipment's mass — appended last, always written, and
                    // legitimately `0` (weightless materials) unlike the pack lever above it.
                    expeditionTradeMaterialCarryWeight: cohort
                        .expedition_trade_material_carry_weight,
                    // The transfer pair a client renders — appended last, always written. Per-turn
                    // state, so unlike the accumulating pair above it survives the sim's
                    // after-every-command recapture; on a turn frame the two read the same number.
                    transferReceivedTurn: cohort.transfer_received_turn,
                    transferSentTurn: cohort.transfer_sent_turn,
                    // How this band splits a maintenance pool it cannot stretch — appended last,
                    // always written, because "the sim did not state a mode" and "spread" must not
                    // be the same frame.
                    upkeepFundMode: upkeep_fund_mode,
                    // THE BAND'S OWN BUILD QUEUE — appended last. The rank is the index, so this
                    // vector's ORDER is the payload; a reader must not re-sort it.
                    buildQueue: build_queue,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

/// **What a draw is short, as numbers** — shared by the bench and by every craft offer, so the two
/// cannot state a shortfall in two different shapes.
fn create_shortfalls<'a>(
    builder: &mut FbBuilder<'a>,
    shortfalls: &[MaterialShortfallState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::MaterialShortfall<'a>>>> {
    let rows: Vec<_> = shortfalls
        .iter()
        .map(|shortfall| {
            let material_id = builder.create_string(&shortfall.material_id);
            fb::MaterialShortfall::create(
                builder,
                &fb::MaterialShortfallArgs {
                    materialId: Some(material_id),
                    required: shortfall.required,
                    held: shortfall.held,
                    short: shortfall.short,
                },
            )
        })
        .collect();
    builder.create_vector(&rows)
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
