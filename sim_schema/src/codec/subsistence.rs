//! Subsistence-section FlatBuffers serialization.

use crate::codec::FbBuilder;
use crate::state::subsistence::{
    FloraShareInfo, FoodModuleState, ForagePatchState, HerdTelemetryState,
    IntensificationKnowledgeState, KitOptionState, SedentarizationState,
};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

pub(crate) fn serialize_subsistence_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::SubsistenceSection<'a>> {
    let herds = create_herds(builder, &snapshot.herds);
    let forage_patches = create_forage_patches(builder, &snapshot.forage_patches);
    let sedentarization = create_sedentarization(builder, &snapshot.sedentarization);
    let intensification_knowledge =
        create_intensification_knowledge(builder, &snapshot.intensification_knowledge);
    let food_modules = create_food_modules(builder, &snapshot.food_modules);
    let kits = create_kits(builder, &snapshot.kits);
    let default_hunt_kit_id = builder.create_string(&snapshot.default_hunt_kit_id);
    let default_forage_kit_id = builder.create_string(&snapshot.default_forage_kit_id);
    fb::SubsistenceSection::create(
        builder,
        &fb::SubsistenceSectionArgs {
            herds: Some(herds),
            foragePatches: Some(forage_patches),
            sedentarization: Some(sedentarization),
            intensificationKnowledge: Some(intensification_knowledge),
            foodModules: Some(food_modules),
            kits: Some(kits),
            defaultHuntKitId: Some(default_hunt_kit_id),
            defaultForageKitId: Some(default_forage_kit_id),
        },
    )
}

pub(crate) fn serialize_subsistence_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::SubsistenceSection<'a>> {
    let herds = delta
        .herds
        .as_ref()
        .map(|entries| create_herds(builder, entries));
    let forage_patches = delta
        .forage_patches
        .as_ref()
        .map(|entries| create_forage_patches(builder, entries));
    let sedentarization = delta
        .sedentarization
        .as_ref()
        .map(|entries| create_sedentarization(builder, entries));
    let intensification_knowledge = delta
        .intensification_knowledge
        .as_ref()
        .map(|entries| create_intensification_knowledge(builder, entries));
    let food_modules = delta
        .food_modules
        .as_ref()
        .map(|entries| create_food_modules(builder, entries));
    let kits = delta
        .kits
        .as_ref()
        .map(|entries| create_kits(builder, entries));
    let default_hunt_kit_id = delta
        .default_hunt_kit_id
        .as_ref()
        .map(|id| builder.create_string(id));
    let default_forage_kit_id = delta
        .default_forage_kit_id
        .as_ref()
        .map(|id| builder.create_string(id));
    fb::SubsistenceSection::create(
        builder,
        &fb::SubsistenceSectionArgs {
            herds,
            foragePatches: forage_patches,
            sedentarization,
            intensificationKnowledge: intensification_knowledge,
            foodModules: food_modules,
            kits,
            defaultHuntKitId: default_hunt_kit_id,
            defaultForageKitId: default_forage_kit_id,
        },
    )
}

/// **The kit roster**, once per world — the picker's list plus the tiers each kit grants, so the
/// client never re-derives the TOE table. `jobs` crosses as free-form strings (the `species` /
/// `policy` convention), so adding a job needs no schema change.
fn create_kits<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[KitOptionState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KitOption<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let id = builder.create_string(&state.id);
        let display_name = builder.create_string(&state.display_name);
        let jobs: Vec<_> = state
            .jobs
            .iter()
            .map(|job| builder.create_string(job))
            .collect();
        let jobs = builder.create_vector(&jobs);
        entries.push(fb::KitOption::create(
            builder,
            &fb::KitOptionArgs {
                id: Some(id),
                displayName: Some(display_name),
                jobs: Some(jobs),
                attack: state.attack,
                huntCarryPerWorkerBiomass: state.hunt_carry_per_worker_biomass,
                forageCarryPerWorkerBiomass: state.forage_carry_per_worker_biomass,
            },
        ));
    }
    builder.create_vector(&entries)
}

fn create_sedentarization<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[SedentarizationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::SedentarizationState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let stage = builder.create_string(state.stage.as_str());
        let entry = fb::SedentarizationState::create(
            builder,
            &fb::SedentarizationStateArgs {
                faction: state.faction,
                score: state.score,
                stage: Some(stage),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_herds<'a>(
    builder: &mut FbBuilder<'a>,
    herds: &[HerdTelemetryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::HerdTelemetryState<'a>>>> {
    let mut entries = Vec::with_capacity(herds.len());
    for herd in herds {
        let id = builder.create_string(herd.id.as_str());
        let label = builder.create_string(herd.label.as_str());
        let species = builder.create_string(herd.species.as_str());
        let size_class = builder.create_string(herd.size_class.as_str());
        let ecology_phase = builder.create_string(herd.ecology_phase.as_str());
        let husbandry_ceiling = builder.create_string(herd.husbandry_ceiling.as_str());
        // WHICH KIT EACH ESTIMATE TABLE IS QUOTED FOR — always written, even when the table beside it
        // is empty: "these rows would have been priced at X" is still the honest answer, and a
        // consumer comparing the player's selection against an absent string would read every herd
        // as a mismatch.
        let hunt_trip_estimates_kit_id =
            builder.create_string(herd.hunt_trip_estimates_kit_id.as_str());
        let denial_estimates_kit_id = builder.create_string(herd.denial_estimates_kit_id.as_str());
        let hunt_trip_estimates = if herd.hunt_trip_estimates.is_empty() {
            None
        } else {
            let entries: Vec<_> = herd
                .hunt_trip_estimates
                .iter()
                .map(|estimate| {
                    let bound = builder.create_string(estimate.bound.as_str());
                    fb::HuntTripEstimate::create(
                        builder,
                        &fb::HuntTripEstimateArgs {
                            // THE SAMPLED FLOOR — replaces the retired `policy` string.
                            floor: estimate.floor,
                            // WHICH STOP ENDS THE TRIP — a `HuntTripBound` key.
                            bound: Some(bound),
                            partyWorkers: estimate.party_workers,
                            turnsToFill: estimate.turns_to_fill,
                            deliversFood: estimate.delivers_food,
                            deliversTrade: estimate.delivers_trade,
                            deliveredTrade: estimate.delivered_trade,
                            animalsTaken: estimate.animals_taken,
                            deliveredFood: estimate.delivered_food,
                            wastedFood: estimate.wasted_food,
                        },
                    )
                })
                .collect();
            Some(builder.create_vector(&entries))
        };
        // The denial twin of the table above — one row per party size, no floor axis (the mission
        // carries no floor). Absent when empty, the same convention.
        let denial_estimates = if herd.denial_estimates.is_empty() {
            None
        } else {
            let entries: Vec<_> = herd
                .denial_estimates
                .iter()
                .map(|estimate| {
                    let outcome = builder.create_string(estimate.outcome.as_str());
                    fb::DenialEstimate::create(
                        builder,
                        &fb::DenialEstimateArgs {
                            partyWorkers: estimate.party_workers,
                            turnsToCollapse: estimate.turns_to_collapse,
                            turnsToCollapseLow: estimate.turns_to_collapse_low,
                            turnsToCollapseHigh: estimate.turns_to_collapse_high,
                            outcome: Some(outcome),
                            animalsKilled: estimate.animals_killed,
                            deliveredFood: estimate.delivered_food,
                            wastedFood: estimate.wasted_food,
                            deliveredTrade: estimate.delivered_trade,
                        },
                    )
                })
                .collect();
            Some(builder.create_vector(&entries))
        };
        // **An EMPTY curve is absent, not a vector of zeros** — the `hunt_trip_estimates` convention
        // above, and the one that lets a client tell "this source published no curve" from "this
        // source does not grow", which are different facts.
        let regrowth_samples = if herd.regrowth_samples.is_empty() {
            None
        } else {
            Some(builder.create_vector(&herd.regrowth_samples))
        };
        let entry = fb::HerdTelemetryState::create(
            builder,
            &fb::HerdTelemetryStateArgs {
                id: Some(id),
                label: Some(label),
                species: Some(species),
                x: herd.x,
                y: herd.y,
                biomass: herd.biomass,
                routeLength: herd.route_length,
                nextX: herd.next_x,
                nextY: herd.next_y,
                sizeClass: Some(size_class),
                huntable: herd.huntable,
                ecologyPhase: Some(ecology_phase),
                domestication: herd.domestication,
                corralled: herd.corralled,
                corralProgress: herd.corral_progress,
                perWorkerYield: herd.per_worker_yield,
                perWorkerTrade: herd.per_worker_trade,
                tradePerAnimal: herd.trade_per_animal,
                corralYield: herd.corral_yield,
                // The Corral rung's trade half (issue #397) — appended last (append-only wire).
                corralTrade: herd.corral_trade,
                penUpkeep: herd.pen_upkeep,
                penFedFraction: herd.pen_fed_fraction,
                // Appended after every earlier-shipped field (append-only wire discipline).
                // RETIRED: the four stance rows cannot express a continuous dial. The client
                // composes any floor's ceiling from `biomass`/`carryingCapacity`/`*PerBiomass`.
                provisionsPerBiomass: herd.provisions_per_biomass,
                fodderPerBiomass: herd.fodder_per_biomass,
                tradePerBiomass: herd.trade_per_biomass,
                huntTripEstimates: hunt_trip_estimates,
                // Ecological K + grazing range (Grazing Phase 2b-iii) — appended last.
                carryingCapacity: herd.carrying_capacity,
                grazeRangeRadius: herd.graze_range_radius,
                // The pen economy (Grazing 2d) — appended last.
                penRadius: herd.pen_radius,
                penFootprintTiles: herd.pen_footprint_tiles,
                penPastureFraction: herd.pen_pasture_fraction,
                penExtendProgress: herd.pen_extend_progress,
                // Husbandry ceiling (Grazing 2d-δ) — appended last.
                husbandryCeiling: Some(husbandry_ceiling),
                // Body mass (slice 8b) — appended last (append-only wire).
                bodyMass: herd.body_mass,
                // Food per animal (slice 8b) — appended last (append-only wire).
                foodPerAnimal: herd.food_per_animal,
                // Herd staffing — appended last (append-only wire).
                herdersNeeded: herd.herders_needed,
                herdedFraction: herd.herded_fraction,
                // The Tame rung's payoff — appended last (append-only wire).
                pastoralYield: herd.pastoral_yield,
                // The Tame rung's trade half (issue #397) — appended last (append-only wire).
                pastoralTrade: herd.pastoral_trade,
                // Hay this pen drew last turn (F3) — appended last (append-only wire).
                fodderDraw: herd.fodder_draw,
                // The render-ready feed split (F3) — appended last (append-only wire).
                penLarderBill: herd.pen_larder_bill,
                penHayFood: herd.pen_hay_food,
                // Raw combat components (Predators Phase 0) — the client derives danger itself.
                // Appended last (append-only wire).
                attack: herd.attack,
                defense: herd.defense,
                ferocity: herd.ferocity,
                aggression: herd.aggression,
                // Prey-sensing radius (Predators Phase 1a) — the predator's view-ring / "is a predator"
                // signal. Appended last (append-only wire).
                preySenseRadius: herd.prey_sense_radius,
                // Ownership-independent would-be herder count (taming-startup-lag fix) — appended last.
                herdersNeededIfManaged: herd.herders_needed_if_managed,
                // The two build dips as FRACTIONS (issue #442) — the dip multiplies the selected
                // stance's row, so it is no longer a row of its own. Appended last.
                tameBuildFraction: herd.tame_build_fraction,
                corralBuildFraction: herd.corral_build_fraction,
                // The neglect grace — appended last.
                hasNeglectGrace: herd.has_neglect_grace,
                neglectGraceRemaining: herd.neglect_grace_remaining,
                // One hunter's BIOMASS throughput — appended last (append-only wire). The term the
                // crew half of the compose sheet divides a ceiling by; see the schema comment for
                // why it is not derived from `perWorkerYield / provisionsPerBiomass`.
                perWorkerBiomass: herd.per_worker_biomass,
                // The sampled regrowth curve — appended last (append-only wire). Negative below the
                // Allee threshold, by design; see the schema comment.
                regrowthSamples: regrowth_samples,
                // The phase bands this herd's own rung cuts on — appended last (append-only wire).
                collapseFraction: herd.collapse_fraction,
                stressedFraction: herd.stressed_fraction,
                // The engagement throughput — appended last (append-only wire). `0` = no engagement
                // stage (a pen, an unresolvable species), which a reader treats as unbounded.
                engageRate: herd.engage_rate,
                // The attrition denominator — appended last, so the slot stays positional.
                durability: herd.durability,
                // The denial raid's pre-launch table — appended last.
                denialEstimates: denial_estimates,
                // The party that table's sheet opens on — appended last, so the slot stays
                // positional. `0` = no quoted party drives this herd down.
                denialPartyNeeded: herd.denial_party_needed,
                huntTripEstimatesKitId: Some(hunt_trip_estimates_kit_id),
                denialEstimatesKitId: Some(denial_estimates_kit_id),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_forage_patches<'a>(
    builder: &mut FbBuilder<'a>,
    patches: &[ForagePatchState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::ForagePatchState<'a>>>> {
    let mut entries = Vec::with_capacity(patches.len());
    for patch in patches {
        let ecology_phase = builder.create_string(patch.ecology_phase.as_str());
        let sow_site_refusal = builder.create_string(patch.sow_site_refusal.as_str());
        let composition = create_flora_shares(builder, &patch.composition);
        // The committed crop (S1) — both empty when the patch is the wild mixed basket.
        let committed_species = builder.create_string(patch.committed_species.as_str());
        let committed_display_name = builder.create_string(patch.committed_display_name.as_str());
        // Absent rather than a vector of zeros — see the herd twin.
        let regrowth_samples = if patch.regrowth_samples.is_empty() {
            None
        } else {
            Some(builder.create_vector(&patch.regrowth_samples))
        };
        let entry = fb::ForagePatchState::create(
            builder,
            &fb::ForagePatchStateArgs {
                x: patch.x,
                y: patch.y,
                cultivationProgress: patch.cultivation_progress,
                isCultivated: patch.is_cultivated,
                hasOwner: patch.owner.is_some(),
                owner: patch.owner.unwrap_or(0),
                biomass: patch.biomass,
                carryingCapacity: patch.carrying_capacity,
                ecologyPhase: Some(ecology_phase),
                perWorkerYield: patch.per_worker_yield,
                tendedYield: patch.tended_yield,
                fieldProgress: patch.field_progress,
                isField: patch.is_field,
                fieldYield: patch.field_yield,
                sowSiteRefusal: Some(sow_site_refusal),
                composition: Some(composition),
                // The committed crop — appended last (append-only wire).
                committedSpecies: Some(committed_species),
                committedDisplayName: Some(committed_display_name),
                // The TILE's yield vector — appended last (append-only wire, #426).
                // RETIRED: see the herd twin above.
                provisionsPerBiomass: patch.provisions_per_biomass,
                fodderPerBiomass: patch.fodder_per_biomass,
                tradePerBiomass: patch.trade_per_biomass,
                tendedTrade: patch.tended_trade,
                tendedFodder: patch.tended_fodder,
                fieldTrade: patch.field_trade,
                fieldFodder: patch.field_fodder,
                // The two build dips as FRACTIONS (issue #442) — appended last.
                cultivateBuildFraction: patch.cultivate_build_fraction,
                sowBuildFraction: patch.sow_build_fraction,
                // The neglect grace + the two build crews — appended last.
                hasNeglectGrace: patch.has_neglect_grace,
                neglectGraceRemaining: patch.neglect_grace_remaining,
                cultivateCrewNeeded: patch.cultivate_crew_needed,
                sowCrewNeeded: patch.sow_crew_needed,
                // One gatherer's BIOMASS throughput, seasonal weight folded in — appended last
                // (append-only wire). The plant twin of the herd field; `0` in a dead season.
                perWorkerBiomass: patch.per_worker_biomass,
                // The sampled regrowth curve — appended last (append-only wire). Never negative on
                // this web; see the schema comment.
                regrowthSamples: regrowth_samples,
                // The phase bands this patch's own rung cuts on — appended last (append-only wire).
                collapseFraction: patch.collapse_fraction,
                stressedFraction: patch.stressed_fraction,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

/// The per-tile flora composition (`ForagePatchState.composition`). Emitted in the order the sim
/// hands it over — already deterministic (share DESC, then species key ASC).
fn create_flora_shares<'a>(
    builder: &mut FbBuilder<'a>,
    shares: &[FloraShareInfo],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::FloraShareInfo<'a>>>> {
    let mut entries = Vec::with_capacity(shares.len());
    for share in shares {
        let species = builder.create_string(share.species.as_str());
        let display_name = builder.create_string(share.display_name.as_str());
        let role = builder.create_string(share.role.as_str());
        let entry = fb::FloraShareInfo::create(
            builder,
            &fb::FloraShareInfoArgs {
                species: Some(species),
                displayName: Some(display_name),
                share: share.share,
                // Which rungs this plant can climb — appended last (append-only wire).
                canCultivate: share.can_cultivate,
                canSow: share.can_sow,
                // Is committing this tile to this plant worth it — appended last (append-only wire).
                cultivateYieldRatio: share.cultivate_yield_ratio,
                sowYieldRatio: share.sow_yield_ratio,
                // What it would actually pay — appended last (append-only wire).
                cultivatePayoff: share.cultivate_payoff,
                sowPayoff: share.sow_payoff,
                // The fodder a hay Field would pay — appended last (append-only wire, F3).
                sowFodderPayoff: share.sow_fodder_payoff,
                // The trade goods a cash Field would pay — appended last (append-only wire, F4).
                sowTradePayoff: share.sow_trade_payoff,
                // The same two accounts at the TENDED rung — appended last (append-only wire, #419).
                cultivateFodderPayoff: share.cultivate_fodder_payoff,
                cultivateTradePayoff: share.cultivate_trade_payoff,
                // What the plant is FOR — a display tag off the roster, appended last (append-only
                // wire). `""` is "unstated", never "staple".
                role: Some(role),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_intensification_knowledge<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[IntensificationKnowledgeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::IntensificationKnowledgeState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let entry = fb::IntensificationKnowledgeState::create(
            builder,
            &fb::IntensificationKnowledgeStateArgs {
                faction: state.faction,
                cultivation: state.cultivation,
                herding: state.herding,
                seedSelection: state.seed_selection,
                penning: state.penning,
                foddering: state.foddering,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_food_modules<'a>(
    builder: &mut FbBuilder<'a>,
    modules: &[FoodModuleState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::FoodModuleState<'a>>>> {
    let mut entries = Vec::with_capacity(modules.len());
    for module in modules {
        let module_label = builder.create_string(module.module.as_str());
        let kind_label = builder.create_string(module.kind.as_str());
        let entry = fb::FoodModuleState::create(
            builder,
            &fb::FoodModuleStateArgs {
                x: module.x,
                y: module.y,
                module: Some(module_label),
                seasonalWeight: module.seasonal_weight,
                kind: Some(kind_label),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}
