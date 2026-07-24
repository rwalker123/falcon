//! Subsistence-section FlatBuffers serialization.

use crate::codec::FbBuilder;
use crate::state::subsistence::{
    FloraShareInfo, FoodModuleState, ForagePatchState, HerdTelemetryState,
    IntensificationKnowledgeState, SedentarizationState,
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
    fb::SubsistenceSection::create(
        builder,
        &fb::SubsistenceSectionArgs {
            herds: Some(herds),
            foragePatches: Some(forage_patches),
            sedentarization: Some(sedentarization),
            intensificationKnowledge: Some(intensification_knowledge),
            foodModules: Some(food_modules),
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
    fb::SubsistenceSection::create(
        builder,
        &fb::SubsistenceSectionArgs {
            herds,
            foragePatches: forage_patches,
            sedentarization,
            intensificationKnowledge: intensification_knowledge,
            foodModules: food_modules,
        },
    )
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
        let hunt_policy_ceilings = if herd.hunt_policy_ceilings.is_empty() {
            None
        } else {
            let entries: Vec<_> = herd
                .hunt_policy_ceilings
                .iter()
                .map(|ceiling| {
                    let policy = builder.create_string(ceiling.policy.as_str());
                    fb::HuntPolicyCeiling::create(
                        builder,
                        &fb::HuntPolicyCeilingArgs {
                            policy: Some(policy),
                            provisionsPerTurn: ceiling.provisions_per_turn,
                        },
                    )
                })
                .collect();
            Some(builder.create_vector(&entries))
        };
        let hunt_trip_estimates = if herd.hunt_trip_estimates.is_empty() {
            None
        } else {
            let entries: Vec<_> = herd
                .hunt_trip_estimates
                .iter()
                .map(|estimate| {
                    let policy = builder.create_string(estimate.policy.as_str());
                    fb::HuntTripEstimate::create(
                        builder,
                        &fb::HuntTripEstimateArgs {
                            policy: Some(policy),
                            partyWorkers: estimate.party_workers,
                            turnsToFill: estimate.turns_to_fill,
                            deliversFood: estimate.delivers_food,
                            animalsTaken: estimate.animals_taken,
                            deliveredFood: estimate.delivered_food,
                            wastedFood: estimate.wasted_food,
                        },
                    )
                })
                .collect();
            Some(builder.create_vector(&entries))
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
                corralYield: herd.corral_yield,
                penUpkeep: herd.pen_upkeep,
                penFedFraction: herd.pen_fed_fraction,
                // Appended after every earlier-shipped field (append-only wire discipline).
                huntPolicyCeilings: hunt_policy_ceilings,
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
                ceilingSustain: patch.ceiling_sustain,
                ceilingSurplus: patch.ceiling_surplus,
                ceilingMarket: patch.ceiling_market,
                ceilingEradicate: patch.ceiling_eradicate,
                ceilingCultivate: patch.ceiling_cultivate,
                tendedYield: patch.tended_yield,
                fieldProgress: patch.field_progress,
                isField: patch.is_field,
                ceilingSow: patch.ceiling_sow,
                fieldYield: patch.field_yield,
                sowSiteRefusal: Some(sow_site_refusal),
                composition: Some(composition),
                // The committed crop — appended last (append-only wire).
                committedSpecies: Some(committed_species),
                committedDisplayName: Some(committed_display_name),
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
