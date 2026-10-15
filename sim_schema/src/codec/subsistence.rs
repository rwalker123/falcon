//! Subsistence-section FlatBuffers serialization.

use crate::codec::{
    decode_scalars, decode_strings, map_rows, map_rows_if_present, text, FbBuilder,
};
use crate::state::subsistence::{
    BuildLegState, CharacteristicBandState, CraftKnowledgeState, DepositState, FloraShareInfo,
    FoodModuleState, ForagePatchState, HerdTelemetryState, IntensificationKnowledgeState,
    KitOptionState, LadderBranchState, LadderKnowledgeProgress, LadderKnowledgeState,
    LadderRungState, MaterialDefState, MaterialPayoff, RecipeDefState, RecipeInputState,
    RecipeOutputState, RungQuoteState, RungShareQuoteState, SedentarizationState,
    SpeciesMaterialRates,
};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{ForwardsUOffset, WIPOffset};

/// **A SITE'S KEEPING-TOOL LINES** — absent rather than empty where the keeping claimed no tool,
/// the `kitToe` convention.
pub(crate) fn create_upkeep_toe<'a>(
    builder: &mut FbBuilder<'a>,
    lines: &[crate::state::population::KitToeLineState],
) -> Option<WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KitToeLine<'a>>>>> {
    if lines.is_empty() {
        return None;
    }
    let rows: Vec<_> = lines
        .iter()
        .map(|line| {
            let item_id = builder.create_string(&line.item_id);
            fb::KitToeLine::create(
                builder,
                &fb::KitToeLineArgs {
                    itemId: Some(item_id),
                    required: line.required,
                    filled: line.filled,
                },
            )
        })
        .collect();
    Some(builder.create_vector(&rows))
}

/// The decode twin of [`create_upkeep_toe`].
pub(crate) fn decode_upkeep_toe(
    lines: Option<flatbuffers::Vector<'_, ForwardsUOffset<fb::KitToeLine<'_>>>>,
) -> Vec<crate::state::population::KitToeLineState> {
    map_rows(lines, |line| crate::state::population::KitToeLineState {
        item_id: text(line.itemId()),
        required: line.required(),
        filled: line.filled(),
    })
}
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
    let ladder_knowledge = create_ladder_knowledge(builder, &snapshot.ladder_knowledge);
    let ladder_areas = create_ladder_areas(builder, &snapshot.ladder_areas);
    let food_modules = create_food_modules(builder, &snapshot.food_modules);
    let kits = create_kits(builder, &snapshot.kits);
    let default_hunt_kit_id = builder.create_string(&snapshot.default_hunt_kit_id);
    let default_forage_kit_id = builder.create_string(&snapshot.default_forage_kit_id);
    let default_scout_kit_id = builder.create_string(&snapshot.default_scout_kit_id);
    let default_warrior_kit_id = builder.create_string(&snapshot.default_warrior_kit_id);
    let default_expedition_kit_id = builder.create_string(&snapshot.default_expedition_kit_id);
    let default_extract_kit_id = builder.create_string(&snapshot.default_extract_kit_id);
    let equipment_config_json = builder.create_string(&snapshot.equipment_config_json);
    // The crafting catalogues — TYPED, not a second `equipmentConfigJson`: that blob has no gameplay
    // consumer, and a gameplay readout gets a field of its own rather than reaching into a string.
    let materials = create_materials(builder, &snapshot.materials);
    let characteristic_bands = create_characteristic_bands(builder, &snapshot.characteristic_bands);
    let recipes = create_recipes(builder, &snapshot.recipes);
    let craft_knowledge = create_craft_knowledge(builder, &snapshot.craft_knowledge);
    let ladder_rungs = create_ladder_rungs(builder, &snapshot.ladder_rungs);
    let ladder_branches = create_ladder_branches(builder, &snapshot.ladder_branches);
    let deposits = create_deposits(builder, &snapshot.deposits);
    fb::SubsistenceSection::create(
        builder,
        &fb::SubsistenceSectionArgs {
            herds: Some(herds),
            foragePatches: Some(forage_patches),
            sedentarization: Some(sedentarization),
            intensificationKnowledge: Some(intensification_knowledge),
            ladderKnowledge: Some(ladder_knowledge),
            foodModules: Some(food_modules),
            kits: Some(kits),
            defaultHuntKitId: Some(default_hunt_kit_id),
            defaultForageKitId: Some(default_forage_kit_id),
            defaultScoutKitId: Some(default_scout_kit_id),
            defaultWarriorKitId: Some(default_warrior_kit_id),
            defaultExpeditionKitId: Some(default_expedition_kit_id),
            defaultExtractKitId: Some(default_extract_kit_id),
            // The designer surface's read-only catalogue — the whole TOE config as one JSON string.
            // Workbench-only; see the schema comment.
            equipmentConfigJson: Some(equipment_config_json),
            materials: Some(materials),
            characteristicBands: Some(characteristic_bands),
            recipes: Some(recipes),
            craftKnowledge: Some(craft_knowledge),
            ladderRungs: Some(ladder_rungs),
            deposits: Some(deposits),
            ladderBranches: Some(ladder_branches),
            ladderAreas: Some(ladder_areas),
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
    let ladder_knowledge = delta
        .ladder_knowledge
        .as_ref()
        .map(|entries| create_ladder_knowledge(builder, entries));
    let ladder_areas = delta
        .ladder_areas
        .as_ref()
        .map(|areas| create_ladder_areas(builder, areas));
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
    let default_scout_kit_id = delta
        .default_scout_kit_id
        .as_ref()
        .map(|id| builder.create_string(id));
    let default_warrior_kit_id = delta
        .default_warrior_kit_id
        .as_ref()
        .map(|id| builder.create_string(id));
    let default_expedition_kit_id = delta
        .default_expedition_kit_id
        .as_ref()
        .map(|id| builder.create_string(id));
    let default_extract_kit_id = delta
        .default_extract_kit_id
        .as_ref()
        .map(|id| builder.create_string(id));
    let equipment_config_json = delta
        .equipment_config_json
        .as_ref()
        .map(|json| builder.create_string(json));
    let materials = delta
        .materials
        .as_ref()
        .map(|entries| create_materials(builder, entries));
    let characteristic_bands = delta
        .characteristic_bands
        .as_ref()
        .map(|entries| create_characteristic_bands(builder, entries));
    let recipes = delta
        .recipes
        .as_ref()
        .map(|entries| create_recipes(builder, entries));
    let craft_knowledge = delta
        .craft_knowledge
        .as_ref()
        .map(|entries| create_craft_knowledge(builder, entries));
    let ladder_rungs = delta
        .ladder_rungs
        .as_ref()
        .map(|entries| create_ladder_rungs(builder, entries));
    let ladder_branches = delta
        .ladder_branches
        .as_ref()
        .map(|entries| create_ladder_branches(builder, entries));
    let deposits = delta
        .deposits
        .as_ref()
        .map(|entries| create_deposits(builder, entries));
    fb::SubsistenceSection::create(
        builder,
        &fb::SubsistenceSectionArgs {
            herds,
            foragePatches: forage_patches,
            sedentarization,
            intensificationKnowledge: intensification_knowledge,
            ladderKnowledge: ladder_knowledge,
            foodModules: food_modules,
            kits,
            defaultHuntKitId: default_hunt_kit_id,
            defaultForageKitId: default_forage_kit_id,
            defaultScoutKitId: default_scout_kit_id,
            defaultWarriorKitId: default_warrior_kit_id,
            defaultExpeditionKitId: default_expedition_kit_id,
            defaultExtractKitId: default_extract_kit_id,
            equipmentConfigJson: equipment_config_json,
            materials,
            characteristicBands: characteristic_bands,
            recipes,
            craftKnowledge: craft_knowledge,
            ladderRungs: ladder_rungs,
            deposits,
            ladderBranches: ladder_branches,
            ladderAreas: ladder_areas,
        },
    )
}

/// **The materials catalogue**, once per world. `axes` crosses in the material's declared order,
/// which is what a batch's readings are keyed by — the order is contract, not presentation.
fn create_materials<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[MaterialDefState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::MaterialDefState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let id = builder.create_string(&state.id);
        let craft = builder.create_string(&state.craft);
        let axes: Vec<_> = state
            .axes
            .iter()
            .map(|axis| builder.create_string(axis))
            .collect();
        let axes = builder.create_vector(&axes);
        let tool_item_id = builder.create_string(&state.tool_item_id);
        entries.push(fb::MaterialDefState::create(
            builder,
            &fb::MaterialDefStateArgs {
                id: Some(id),
                craft: Some(craft),
                axes: Some(axes),
                handWorkable: state.hand_workable,
                handWorkingRate: state.hand_working_rate,
                handWorkingQualityCeiling: state.hand_working_quality_ceiling,
                toolItemId: Some(tool_item_id),
            },
        ));
    }
    builder.create_vector(&entries)
}

/// **The rating vocabulary, once for the world** — the legend. Every published reading already
/// carries its own band name, so this is not what a client looks a reading up in.
fn create_characteristic_bands<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[CharacteristicBandState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CharacteristicBandState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let name = builder.create_string(&state.name);
        entries.push(fb::CharacteristicBandState::create(
            builder,
            &fb::CharacteristicBandStateArgs {
                name: Some(name),
                from: state.from,
            },
        ));
    }
    builder.create_vector(&entries)
}

/// **The recipe book**, once per world — the static half. Whether a given band can make a given
/// recipe, and what it would come out at, is that band's own `craftOffers`.
fn create_recipes<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[RecipeDefState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::RecipeDefState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let id = builder.create_string(&state.id);
        let display_name = builder.create_string(&state.display_name);
        let craft = builder.create_string(&state.craft);
        let group = builder.create_string(&state.group);
        let label = builder.create_string(&state.label);
        let requires_knowledge: Vec<_> = state
            .requires_knowledge
            .iter()
            .map(|craft| builder.create_string(craft))
            .collect();
        let requires_knowledge = builder.create_vector(&requires_knowledge);
        let inputs: Vec<_> = state
            .inputs
            .iter()
            .map(|input| {
                let material_id = builder.create_string(&input.material_id);
                let reads_axis = builder.create_string(&input.reads_axis);
                fb::RecipeInputState::create(
                    builder,
                    &fb::RecipeInputStateArgs {
                        materialId: Some(material_id),
                        amount: input.amount,
                        readsAxis: Some(reads_axis),
                    },
                )
            })
            .collect();
        let inputs = builder.create_vector(&inputs);
        let outputs: Vec<_> = state
            .outputs
            .iter()
            .map(|output| {
                let equipment_id = builder.create_string(&output.equipment_id);
                let material_id = builder.create_string(&output.material_id);
                fb::RecipeOutputState::create(
                    builder,
                    &fb::RecipeOutputStateArgs {
                        equipmentId: Some(equipment_id),
                        materialId: Some(material_id),
                        amount: output.amount,
                    },
                )
            })
            .collect();
        let outputs = builder.create_vector(&outputs);
        entries.push(fb::RecipeDefState::create(
            builder,
            &fb::RecipeDefStateArgs {
                id: Some(id),
                displayName: Some(display_name),
                craft: Some(craft),
                group: Some(group),
                work: state.work,
                requiresKnowledge: Some(requires_knowledge),
                inputs: Some(inputs),
                outputs: Some(outputs),
                label: Some(label),
            },
        ));
    }
    builder.create_vector(&entries)
}

/// **Per faction, per craft.** Diffed as a whole vector each frame rather than held as a world
/// constant, because a craft is *learned* — the meter moves when a bench delivers an item.
fn create_craft_knowledge<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[CraftKnowledgeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CraftKnowledgeState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let craft_id = builder.create_string(&state.craft_id);
        let display_name = builder.create_string(&state.display_name);
        entries.push(fb::CraftKnowledgeState::create(
            builder,
            &fb::CraftKnowledgeStateArgs {
                faction: state.faction,
                craftId: Some(craft_id),
                displayName: Some(display_name),
                known: state.known,
                progress: state.progress,
                completionThreshold: state.completion_threshold,
            },
        ));
    }
    builder.create_vector(&entries)
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
        // The kit's `uses` list, in config order — see `KitOptionState::item_ids` for why the tiers
        // above cannot stand in for it.
        let item_ids: Vec<_> = state
            .item_ids
            .iter()
            .map(|item| builder.create_string(item))
            .collect();
        let item_ids = builder.create_vector(&item_ids);
        // Which web this kit's build gear serves — `""` for a kit carrying none, which is most of
        // the roster.
        let build_work_branch = builder.create_string(&state.build_work_branch);
        // Which rung of that web — `""` for a tool bound to none, i.e. all of them.
        let build_work_rung = builder.create_string(&state.build_work_rung);
        entries.push(fb::KitOption::create(
            builder,
            &fb::KitOptionArgs {
                id: Some(id),
                displayName: Some(display_name),
                jobs: Some(jobs),
                itemIds: Some(item_ids),
                attack: state.attack,
                huntCarryPerWorkerBiomass: state.hunt_carry_per_worker_biomass,
                forageCarryPerWorkerBiomass: state.forage_carry_per_worker_biomass,
                // The scout vantage's tier — the role the roster gained with wayfinding gear.
                scoutVantageRange: state.scout_vantage_range,
                expeditionSightRange: state.expedition_sight_range,
                // What the kit DOES beyond the tiers — all three neutral at 1.0, so a kit declaring
                // none of them encodes exactly as it did before they existed.
                attackMinBodyMass: state.attack_min_body_mass,
                attackMaxBodyMass: state.attack_max_body_mass,
                dispersion: state.dispersion,
                exposure: state.exposure,
                buildRate: state.build_rate,
                buildWorkPerWorker: state.build_work_per_worker,
                buildWorkBranch: Some(build_work_branch),
                buildWorkRung: Some(build_work_rung),
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
                beliefPoints: state.belief_points,
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
        // **Built before the parent table opens**, the ordinary FlatBuffers rule.
        let material_per_biomass = create_material_payoffs(builder, &herd.material_per_biomass);
        let per_worker_material = create_material_payoffs(builder, &herd.per_worker_material);
        let id = builder.create_string(herd.id.as_str());
        let label = builder.create_string(herd.label.as_str());
        let species = builder.create_string(herd.species.as_str());
        let size_class = builder.create_string(herd.size_class.as_str());
        let ecology_phase = builder.create_string(herd.ecology_phase.as_str());
        let husbandry_ceiling = builder.create_string(herd.husbandry_ceiling.as_str());
        // THE KIT THIS QUARRY WANTS — always written, even when it names the hunt job's default: a
        // consumer comparing its player's selection against an absent string would read every herd
        // as a mismatch.
        let default_kit_id = builder.create_string(herd.default_kit_id.as_str());
        // **Always written, `""` included** — the empty string is *"this herd is not a blocked
        // build"*, which is a statement, and an absent field would make a reader guess.
        let build_blocked_reason = builder.create_string(herd.build_blocked_reason.as_str());
        // **THE DESTINATION AND THE LEGS** — always written for the string (`""` is *"not queued"*,
        // a statement), and the legs **absent when empty** on this table's repeated-field convention:
        // *"this source has nothing left to climb"* and *"this source is not queued"* both read as no
        // list, which is what the destination string beside it disambiguates.
        let build_destination_rung = builder.create_string(herd.build_destination_rung.as_str());
        // **WHERE THE HERD IS**, beside where it is going. Always written: every herd stands on a
        // rung, so unlike the destination there is no "not queued" reading to encode.
        let current_rung = builder.create_string(herd.current_rung.as_str());
        let build_legs = if herd.build_legs.is_empty() {
            None
        } else {
            let rows: Vec<_> = herd
                .build_legs
                .iter()
                .map(|leg| {
                    let rung = builder.create_string(leg.rung.as_str());
                    fb::BuildLegState::create(
                        builder,
                        &fb::BuildLegStateArgs {
                            rung: Some(rung),
                            workRemaining: leg.work_remaining,
                            turnsRemaining: leg.turns_remaining,
                        },
                    )
                })
                .collect();
            Some(builder.create_vector(&rows))
        };

        // **An EMPTY curve is absent, not a vector of zeros** — the convention every repeated field
        // on this table follows, and the one that lets a client tell "this source published no
        // curve" from "this source does not grow", which are different facts.
        let regrowth_samples = if herd.regrowth_samples.is_empty() {
            None
        } else {
            Some(builder.create_vector(&herd.regrowth_samples))
        };
        // **THE MATERIAL HALF OF THE LADDER'S PRICE** — three per-good vectors, built before the
        // parent table opens (the ordinary FlatBuffers rule). An EMPTY vector is *"this rung eats no
        // material"*, never *"zero of something"*.
        let build_material_cost = create_material_payoffs(builder, &herd.build_material_cost);
        let upkeep_material_demand = create_material_payoffs(builder, &herd.upkeep_material_demand);
        let upkeep_material_supplied =
            create_material_payoffs(builder, &herd.upkeep_material_supplied);
        let upkeep_toe = create_upkeep_toe(builder, &herd.upkeep_toe);
        let rung_quotes = create_rung_quotes(builder, &herd.rung_quotes);
        let predator_eaten_by = builder.create_string(&herd.predator_eaten_by);
        let keeping = builder.create_string(&herd.keeping);
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
                perWorkerYield: herd.per_worker_yield,
                penFedFraction: herd.pen_fed_fraction,
                // Appended after every earlier-shipped field (append-only wire discipline).
                // RETIRED: the four stance rows cannot express a continuous dial. The client
                // composes any floor's ceiling from `biomass`/`carryingCapacity`/`*PerBiomass`.
                provisionsPerBiomass: herd.provisions_per_biomass,
                fodderPerBiomass: herd.fodder_per_biomass,
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
                // Hay this pen drew last turn (F3) — appended last (append-only wire).
                fodderDraw: herd.fodder_draw,
                // **How much more fodder the pen still needs** — appended last (append-only wire).
                // `max(0, hay need − fodderDraw)`, struck sim-side on the same pass as both its terms
                // so the difference can never describe a different turn from them. The gap it is
                // taken from is not a wire field: nothing read it.
                penFodderShortfall: herd.pen_fodder_shortfall,
                // The render-ready feed split (F3) — appended last (append-only wire).
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
                stayFraction: herd.stay_fraction,
                // The quarry's own default kit — appended last, so the slot stays positional.
                defaultKitId: Some(default_kit_id),
                // **What a hunt of this herd is MADE OF** — appended last (append-only wire, arc
                // #527), the replacement for the retired `tradePerBiomass`/`perWorkerTrade`. An
                // EMPTY vector is "no row", never "zero".
                materialPerBiomass: Some(material_per_biomass),
                perWorkerMaterial: Some(per_worker_material),
                buildTurnsRemaining: herd.build_turns_remaining,
                buildWorkFromGear: herd.build_work_from_gear,
                buildWorkPerWorkerTurn: herd.build_work_per_worker_turn,
                // **The standing upkeep** — appended last (append-only wire,
                // docs/plan_standing_upkeep.md §2). All three terms ship, so the client subtracts
                // nothing; `upkeepDemand` follows `rungQuotes[animal:pen].foodYield`'s always-meaningful rule.
                upkeepDemand: herd.upkeep_demand,
                upkeepSupplied: herd.upkeep_supplied,
                upkeepShortfall: herd.upkeep_shortfall,
                upkeepWorkersNeeded: herd.upkeep_workers_needed,
                upkeepWorkersAtCompletion: herd.upkeep_workers_at_completion,
                upkeepNextWorkerTurn: herd.upkeep_next_worker_turn,
                upkeepToe: upkeep_toe,
                meterRotPerTurn: herd.meter_rot_per_turn,
                // Where this herd sits in the winning band's queue — read as one set with
                // `buildTurnsRemaining` and `buildWorkFromGear`, which is what makes a chained date
                // legible (docs/plan_standing_upkeep.md 4.6b).
                buildQueuePosition: herd.build_queue_position,
                // …and WHY it is stuck, when it is. `""` = not blocked.
                buildBlockedReason: Some(build_blocked_reason),
                buildDestinationRung: Some(build_destination_rung),
                buildLegs: build_legs,
                rungQuotes: Some(rung_quotes),
                // **WHERE THAT DESTINATION WILL LEAVE THIS HERD'S `K`** — appended last
                // (append-only wire). `None` is *"no band has queued it"*, which crosses as the
                // [`crate::NO_BUILD_DESTINATION_CAPACITY`] sentinel rather than as `0`: a capacity
                // of zero is a real reading a real herd has.
                buildDestinationCapacity: herd
                    .build_destination_capacity
                    .unwrap_or(crate::NO_BUILD_DESTINATION_CAPACITY),
                // **The pen ring's DENOMINATOR** — appended last (append-only wire). Rides beside
                // `penExtendProgress` above in the same work units; `0` with no ring in flight.
                penExtendCost: herd.pen_extend_cost,
                // **What this herd is committed to producing, and the order in flight** — appended
                // last (append-only wire). See `HerdTelemetryState::standing_output_fraction`.
                standingOutputFraction: herd.standing_output_fraction,
                standingOutputTarget: herd.standing_output_target,
                outputRecommitProgress: herd.output_recommit_progress,
                outputRecommitCost: herd.output_recommit_cost,
                // **The rung this herd STANDS on** — appended last (append-only wire), the
                // twin of `buildDestinationRung`'s spelling at the source's own position.
                currentRung: Some(current_rung),
                buildMaterialCost: Some(build_material_cost),
                upkeepMaterialDemand: Some(upkeep_material_demand),
                upkeepMaterialSupplied: Some(upkeep_material_supplied),
                // **WHAT THE HERD'S OWN CREW SPENT KEEPING IT** (`docs/plan_site_crews.md` §2.2).
                upkeepHands: herd.upkeep_hands,
                upkeepToolsShort: herd.upkeep_tools_short,
                predatorEatenBy: Some(predator_eaten_by),
                keeping: Some(keeping),
                nextGroundX: herd.next_ground_x,
                nextGroundY: herd.next_ground_y,
                predatorEaten: herd.predator_eaten,
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
        // **Built before the parent table opens**, the ordinary FlatBuffers rule.
        let material_per_biomass = create_material_payoffs(builder, &patch.material_per_biomass);
        let per_worker_material = create_material_payoffs(builder, &patch.per_worker_material);
        let ecology_phase = builder.create_string(patch.ecology_phase.as_str());
        // Always written, `""` included — see the herd twin.
        let build_blocked_reason = builder.create_string(patch.build_blocked_reason.as_str());
        // **THE DESTINATION AND THE LEGS** — always written for the string (`""` is *"not queued"*,
        // a statement), and the legs **absent when empty** on this table's repeated-field convention:
        // *"this source has nothing left to climb"* and *"this source is not queued"* both read as no
        // list, which is what the destination string beside it disambiguates.
        let build_destination_rung = builder.create_string(patch.build_destination_rung.as_str());
        // **WHERE THE PATCH IS**, beside where it is going — see the herd twin.
        let current_rung = builder.create_string(patch.current_rung.as_str());
        let rung_quotes = create_rung_quotes(builder, &patch.rung_quotes);
        let build_legs = if patch.build_legs.is_empty() {
            None
        } else {
            let rows: Vec<_> = patch
                .build_legs
                .iter()
                .map(|leg| {
                    let rung = builder.create_string(leg.rung.as_str());
                    fb::BuildLegState::create(
                        builder,
                        &fb::BuildLegStateArgs {
                            rung: Some(rung),
                            workRemaining: leg.work_remaining,
                            turnsRemaining: leg.turns_remaining,
                        },
                    )
                })
                .collect();
            Some(builder.create_vector(&rows))
        };

        let composition = create_flora_shares(builder, &patch.composition);
        // Index-aligned with the basket above — absent for a tile that names no plants, never a
        // vector of zeros, so "no basket" and "a basket of nothing" stay distinguishable.
        let composition_standing_biomass = if patch.composition_standing_biomass.is_empty() {
            None
        } else {
            Some(builder.create_vector(&patch.composition_standing_biomass))
        };
        // The per-species conversion rates, same alignment and the same absent-not-zeros rule.
        let composition_provisions = if patch.composition_provisions_per_biomass.is_empty() {
            None
        } else {
            Some(builder.create_vector(&patch.composition_provisions_per_biomass))
        };
        let composition_fodder = if patch.composition_fodder_per_biomass.is_empty() {
            None
        } else {
            Some(builder.create_vector(&patch.composition_fodder_per_biomass))
        };
        // **The per-species MATERIAL rates** — a vector of one-field tables, because FlatBuffers has
        // no vector-of-vectors. Built before the parent table opens, the ordinary rule; an entry's
        // `rows` may legitimately be **empty** (a plant that pays no material), which is why the
        // emptiness test is on the outer vector alone.
        let composition_materials = if patch.composition_material_per_biomass.is_empty() {
            None
        } else {
            let entries: Vec<_> = patch
                .composition_material_per_biomass
                .iter()
                .map(|entry| {
                    let rows = create_material_payoffs(builder, &entry.rows);
                    fb::SpeciesMaterialRates::create(
                        builder,
                        &fb::SpeciesMaterialRatesArgs { rows: Some(rows) },
                    )
                })
                .collect();
            Some(builder.create_vector(&entries))
        };
        // The committed crop (S1) — both empty when the patch is the wild mixed basket.
        let committed_species = builder.create_string(patch.committed_species.as_str());
        let committed_display_name = builder.create_string(patch.committed_display_name.as_str());
        // Absent rather than a vector of zeros — see the herd twin.
        let regrowth_samples = if patch.regrowth_samples.is_empty() {
            None
        } else {
            Some(builder.create_vector(&patch.regrowth_samples))
        };
        // **THE MATERIAL HALF OF THE LADDER'S PRICE** — three per-good vectors, built before the
        // parent table opens (the ordinary FlatBuffers rule). An EMPTY vector is *"this rung eats no
        // material"*, never *"zero of something"*.
        let build_material_cost = create_material_payoffs(builder, &patch.build_material_cost);
        let upkeep_material_demand =
            create_material_payoffs(builder, &patch.upkeep_material_demand);
        let upkeep_material_supplied =
            create_material_payoffs(builder, &patch.upkeep_material_supplied);
        let upkeep_toe = create_upkeep_toe(builder, &patch.upkeep_toe);
        let entry = fb::ForagePatchState::create(
            builder,
            &fb::ForagePatchStateArgs {
                x: patch.x,
                y: patch.y,
                hasOwner: patch.owner.is_some(),
                owner: patch.owner.unwrap_or(0),
                biomass: patch.biomass,
                carryingCapacity: patch.carrying_capacity,
                ecologyPhase: Some(ecology_phase),
                perWorkerYield: patch.per_worker_yield,
                composition: Some(composition),
                // The committed crop — appended last (append-only wire).
                committedSpecies: Some(committed_species),
                committedDisplayName: Some(committed_display_name),
                // The TILE's yield vector — appended last (append-only wire, #426).
                // RETIRED: see the herd twin above.
                provisionsPerBiomass: patch.provisions_per_biomass,
                fodderPerBiomass: patch.fodder_per_biomass,
                // The neglect grace — appended last.
                hasNeglectGrace: patch.has_neglect_grace,
                neglectGraceRemaining: patch.neglect_grace_remaining,
                // One gatherer's BIOMASS throughput, seasonal weight folded in — appended last
                // (append-only wire). The plant twin of the herd field; `0` in a dead season.
                perWorkerBiomass: patch.per_worker_biomass,
                // The sampled regrowth curve — appended last (append-only wire). Never negative on
                // this web; see the schema comment.
                regrowthSamples: regrowth_samples,
                // The phase bands this patch's own rung cuts on — appended last (append-only wire).
                collapseFraction: patch.collapse_fraction,
                stressedFraction: patch.stressed_fraction,
                // **What a gather of this patch is MADE OF** — appended last (append-only wire, arc
                // #527), the replacement for the retired `tradePerBiomass` and the RUNG-1 half of
                // the material story. An EMPTY vector is "no row", never "zero".
                materialPerBiomass: Some(material_per_biomass),
                perWorkerMaterial: Some(per_worker_material),
                buildTurnsRemaining: patch.build_turns_remaining,
                buildWorkFromGear: patch.build_work_from_gear,
                buildWorkPerWorkerTurn: patch.build_work_per_worker_turn,
                // **The standing upkeep** — appended last (append-only wire,
                // docs/plan_standing_upkeep.md §2). All three terms ship, so the client subtracts
                // nothing; `upkeepDemand` follows `rungQuotes[animal:pen].foodYield`'s always-meaningful rule.
                upkeepDemand: patch.upkeep_demand,
                upkeepSupplied: patch.upkeep_supplied,
                upkeepShortfall: patch.upkeep_shortfall,
                upkeepWorkersNeeded: patch.upkeep_workers_needed,
                upkeepWorkersAtCompletion: patch.upkeep_workers_at_completion,
                upkeepNextWorkerTurn: patch.upkeep_next_worker_turn,
                upkeepToe: upkeep_toe,
                meterRotPerTurn: patch.meter_rot_per_turn,
                // The plant twin — see the herd row above.
                buildQueuePosition: patch.build_queue_position,
                buildBlockedReason: Some(build_blocked_reason),
                buildDestinationRung: Some(build_destination_rung),
                buildLegs: build_legs,
                rungQuotes: Some(rung_quotes),
                // **How much of each plant is standing** — appended last (append-only wire),
                // index-aligned with `composition`.
                compositionStandingBiomass: composition_standing_biomass,
                // **What each plant converts at** — appended last (append-only wire), index-aligned
                // with `composition` so a sheet can price a narrowing before committing to it.
                compositionProvisionsPerBiomass: composition_provisions,
                compositionFodderPerBiomass: composition_fodder,
                // …and what each of them is made of — appended last (append-only wire).
                compositionMaterialPerBiomass: composition_materials,
                // **The GROUND's own K** — appended last (append-only wire), with no rung gain in
                // it, beside the `carryingCapacity` that carries one. The pair is what lets a
                // remembered hex state a capacity without stating the ladder position the fog
                // redaction exists to hide.
                tileCapacity: patch.tile_capacity,
                // **WHERE THIS PATCH'S BUILD IS TAKING ITS `K`** — appended last (append-only
                // wire); see the herd twin for the sentinel.
                buildDestinationCapacity: patch
                    .build_destination_capacity
                    .unwrap_or(crate::NO_BUILD_DESTINATION_CAPACITY),
                // **The rung this patch STANDS on** — appended last (append-only wire); see the
                // herd twin.
                currentRung: Some(current_rung),
                buildMaterialCost: Some(build_material_cost),
                upkeepMaterialDemand: Some(upkeep_material_demand),
                upkeepMaterialSupplied: Some(upkeep_material_supplied),
                // **WHAT THE PATCH'S OWN CREW SPENT KEEPING IT** (`docs/plan_site_crews.md` §2.1).
                upkeepHands: patch.upkeep_hands,
                upkeepToolsShort: patch.upkeep_tools_short,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

/// The per-tile flora composition (`ForagePatchState.composition`). Emitted in the order the sim
/// hands it over — already deterministic (share DESC, then species key ASC).
/// **A PLANT'S QUOTE AT EVERY RUNG**, children built before the row opens.
fn create_rung_share_quotes<'a>(
    builder: &mut FbBuilder<'a>,
    quotes: &[RungShareQuoteState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::RungShareQuoteState<'a>>>> {
    let mut entries = Vec::with_capacity(quotes.len());
    for quote in quotes {
        let rung_key = builder.create_string(quote.rung_key.as_str());
        let material_payoff = create_material_payoffs(builder, &quote.material_payoff);
        let entry = fb::RungShareQuoteState::create(
            builder,
            &fb::RungShareQuoteStateArgs {
                rungKey: Some(rung_key),
                canClimb: quote.can_climb,
                foodPayoff: quote.food_payoff,
                yieldRatio: quote.yield_ratio,
                fodderPayoff: quote.fodder_payoff,
                materialPayoff: Some(material_payoff),
                workCost: quote.work_cost,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

/// The decode twin of [`create_rung_share_quotes`].
fn decode_rung_share_quotes(
    quotes: Option<flatbuffers::Vector<'_, ForwardsUOffset<fb::RungShareQuoteState<'_>>>>,
) -> Vec<RungShareQuoteState> {
    map_rows(quotes, |quote| RungShareQuoteState {
        rung_key: text(quote.rungKey()),
        can_climb: quote.canClimb(),
        food_payoff: quote.foodPayoff(),
        yield_ratio: quote.yieldRatio(),
        fodder_payoff: quote.fodderPayoff(),
        material_payoff: decode_material_payoffs(quote.materialPayoff()),
        work_cost: quote.workCost(),
    })
}

fn create_flora_shares<'a>(
    builder: &mut FbBuilder<'a>,
    shares: &[FloraShareInfo],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::FloraShareInfo<'a>>>> {
    let mut entries = Vec::with_capacity(shares.len());
    for share in shares {
        let species = builder.create_string(share.species.as_str());
        let display_name = builder.create_string(share.display_name.as_str());
        let role = builder.create_string(share.role.as_str());
        // **Built before the parent table opens**, the ordinary FlatBuffers rule: a nested vector
        // cannot be written while `FloraShareInfo` is under construction.
        let rung_quotes = create_rung_share_quotes(builder, &share.rung_quotes);
        let entry = fb::FloraShareInfo::create(
            builder,
            &fb::FloraShareInfoArgs {
                species: Some(species),
                displayName: Some(display_name),
                share: share.share,
                // What the plant is FOR — a display tag off the roster, appended last (append-only
                // wire). `""` is "unstated", never "staple".
                role: Some(role),
                rungQuotes: Some(rung_quotes),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

/// One rung's per-material quote, as a wire vector. Empty in, empty out — the "no row" reading the
/// field's own contract rests on, so an absent quote never becomes a zero-valued one.
/// **A SOURCE'S RUNG QUOTES** - one `RungQuoteState` row per rung of its branch, children built
/// before the row opens (the ordinary FlatBuffers rule).
fn create_rung_quotes<'a>(
    builder: &mut FbBuilder<'a>,
    quotes: &[RungQuoteState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::RungQuoteState<'a>>>> {
    let mut entries = Vec::with_capacity(quotes.len());
    for quote in quotes {
        let rung_key = builder.create_string(quote.rung_key.as_str());
        let build_material_cost = create_material_payoffs(builder, &quote.build_material_cost);
        let upkeep_material_demand =
            create_material_payoffs(builder, &quote.upkeep_material_demand);
        let material_payoff = create_material_payoffs(builder, &quote.material_payoff);
        let site_refusal = builder.create_string(quote.site_refusal.as_str());
        let entry = fb::RungQuoteState::create(
            builder,
            &fb::RungQuoteStateArgs {
                rungKey: Some(rung_key),
                reached: quote.reached,
                progress: quote.progress,
                workDone: quote.work_done,
                workCost: quote.work_cost,
                buildMaterialCost: Some(build_material_cost),
                upkeepDemand: quote.upkeep_demand,
                upkeepMaterialDemand: Some(upkeep_material_demand),
                foodYield: quote.food_yield,
                fodder: quote.fodder,
                materialPayoff: Some(material_payoff),
                siteRefusal: Some(site_refusal),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

/// The decode twin of [`create_rung_quotes`].
fn decode_rung_quotes(
    quotes: Option<flatbuffers::Vector<'_, ForwardsUOffset<fb::RungQuoteState<'_>>>>,
) -> Vec<RungQuoteState> {
    map_rows(quotes, |quote| RungQuoteState {
        rung_key: text(quote.rungKey()),
        reached: quote.reached(),
        progress: quote.progress(),
        work_done: quote.workDone(),
        work_cost: quote.workCost(),
        build_material_cost: decode_material_payoffs(quote.buildMaterialCost()),
        upkeep_demand: quote.upkeepDemand(),
        upkeep_material_demand: decode_material_payoffs(quote.upkeepMaterialDemand()),
        food_yield: quote.foodYield(),
        fodder: quote.fodder(),
        material_payoff: decode_material_payoffs(quote.materialPayoff()),
        site_refusal: text(quote.siteRefusal()),
    })
}

pub(crate) fn create_material_payoffs<'a>(
    builder: &mut FbBuilder<'a>,
    payoffs: &[MaterialPayoff],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::MaterialPayoff<'a>>>> {
    let mut entries = Vec::with_capacity(payoffs.len());
    for payoff in payoffs {
        let material_id = builder.create_string(payoff.material_id.as_str());
        let entry = fb::MaterialPayoff::create(
            builder,
            &fb::MaterialPayoffArgs {
                materialId: Some(material_id),
                amount: payoff.amount,
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
        let mut rows = Vec::with_capacity(state.knowledges.len());
        for knowledge in &state.knowledges {
            let knowledge_id = builder.create_string(&knowledge.knowledge_id);
            rows.push(fb::LadderKnowledgeProgress::create(
                builder,
                &fb::LadderKnowledgeProgressArgs {
                    knowledgeId: Some(knowledge_id),
                    progress: knowledge.progress,
                },
            ));
        }
        let knowledges = builder.create_vector(&rows);
        let entry = fb::IntensificationKnowledgeState::create(
            builder,
            &fb::IntensificationKnowledgeStateArgs {
                faction: state.faction,
                knowledges: Some(knowledges),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

/// **THE LADDER KNOWLEDGE ROSTER** — what there *is* to learn, once per world. A per-world constant,
/// so it is written whole on a snapshot and only when it moved on a delta, exactly like `kits`.
fn create_ladder_knowledge<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[LadderKnowledgeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::LadderKnowledgeState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let knowledge_id = builder.create_string(&state.knowledge_id);
        let display_name = builder.create_string(&state.display_name);
        let branch = builder.create_string(&state.branch);
        let area = builder.create_string(&state.area);
        entries.push(fb::LadderKnowledgeState::create(
            builder,
            &fb::LadderKnowledgeStateArgs {
                knowledgeId: Some(knowledge_id),
                displayName: Some(display_name),
                branch: Some(branch),
                order: state.order,
                isStep: state.is_step,
                area: Some(area),
            },
        ));
    }
    builder.create_vector(&entries)
}

/// **THE SUBJECT AREAS' DISPLAY ORDER**, once per world and beside the roster above. A per-world
/// constant, so it is written whole on a snapshot and only when it moved on a delta.
///
/// **A plain list of strings** — the shape `ladderAreas` and `DepositState.offeredKitIds` both
/// write, so it serves both.
fn create_ladder_areas<'a>(
    builder: &mut FbBuilder<'a>,
    areas: &[String],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<&'a str>>> {
    let entries: Vec<_> = areas
        .iter()
        .map(|area| builder.create_string(area))
        .collect();
    builder.create_vector(&entries)
}

/// **THE LADDER'S RUNG CATALOG** - every rung of every branch, grouped by branch and in climb order
/// within it, once per world. A per-world constant, written whole on a snapshot and only when it
/// moved on a delta, exactly like the roster above.
fn create_ladder_rungs<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[LadderRungState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::LadderRungState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let rung_key = builder.create_string(&state.rung_key);
        // Which ladder the row is on - the field a reader groups the one vector by.
        let branch = builder.create_string(&state.branch);
        let display_name = builder.create_string(&state.display_name);
        let label = builder.create_string(&state.label);
        let label_fallback = builder.create_string(&state.label_fallback);
        let verb = builder.create_string(&state.verb);
        let source_kind = builder.create_string(&state.source_kind);
        let trait_offsets: Vec<_> = state
            .traits
            .iter()
            .map(|name| builder.create_string(name))
            .collect();
        let traits = builder.create_vector(&trait_offsets);
        let unlock_knowledge = builder.create_string(&state.unlock_knowledge);
        let earns_knowledge = builder.create_string(&state.earns_knowledge);
        let requires_rung = builder.create_string(&state.requires_rung);
        // The noun the pile beside it is counted in - `""` for a rung that eats nothing.
        let build_material_id = builder.create_string(&state.build_material_id);
        entries.push(fb::LadderRungState::create(
            builder,
            &fb::LadderRungStateArgs {
                rungKey: Some(rung_key),
                branch: Some(branch),
                order: state.order,
                displayName: Some(display_name),
                label: Some(label),
                labelFallback: Some(label_fallback),
                verb: Some(verb),
                sourceKind: Some(source_kind),
                traits: Some(traits),
                unlockKnowledge: Some(unlock_knowledge),
                earnsKnowledge: Some(earns_knowledge),
                requiresRung: Some(requires_rung),
                workCost: state.work_cost,
                upkeepWorkPerTurn: state.upkeep_work_per_turn,
                buildMaterialCost: state.build_material_cost,
                buildMaterialId: Some(build_material_id),
                buildWorkPerWorkerTurn: state.build_work_per_worker_turn,
                frictionMultiplier: state.friction_multiplier,
                holdsLinkToTiles: state.holds_link_to_tiles,
                grantsSight: state.grants_sight,
                yieldPerWorkerTurn: state.yield_per_worker_turn,
                recoveryFraction: state.recovery_fraction,
                regrowthMultiplier: state.regrowth_multiplier,
                minDepositCapacity: state.min_deposit_capacity,
                requiresHeldRungBelow: state.requires_held_rung_below,
                requiresCrop: state.requires_crop,
                requiresWorkRoom: state.requires_work_room,
                requiresOwnership: state.requires_ownership,
                placesSource: state.places_source,
            },
        ));
    }
    builder.create_vector(&entries)
}

/// **THE LADDER'S BRANCHES** - the grouping the rung catalog above is drawn under, once per world.
fn create_ladder_branches<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[LadderBranchState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::LadderBranchState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let branch = builder.create_string(&state.branch);
        let area = builder.create_string(&state.area);
        let source_kind = builder.create_string(&state.source_kind);
        let label = builder.create_string(&state.label);
        entries.push(fb::LadderBranchState::create(
            builder,
            &fb::LadderBranchStateArgs {
                branch: Some(branch),
                area: Some(area),
                sourceKind: Some(source_kind),
                label: Some(label),
            },
        ));
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

/// **The deposits under the viewer's eye** — one row per `(tile, material)` on a discovered,
/// deposit-bearing tile, built by `core_sim::snapshot::deposits::deposit_states` off the capture's
/// tile sweep rather than off the registry, and handed here already sorted **`(tile_y, tile_x,
/// material)`** — `snapshot_forage_patches`' own row order, so the section is stable frame to frame
/// and diffs out when nothing moved.
///
/// ⛔ **IT IS NOT THE REGISTRY'S KEY ORDER**, which is `(x, y, material)`: the rows come from the
/// sweep, and a working the registry holds is merged into the sweep's row rather than the other way
/// round. This function's only job is preserving the order it was given.
fn create_deposits<'a>(
    builder: &mut FbBuilder<'a>,
    deposits: &[DepositState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::DepositState<'a>>>> {
    let entries: Vec<_> = deposits
        .iter()
        .map(|deposit| {
            // **Built before the parent table opens**, the ordinary FlatBuffers rule.
            let material = builder.create_string(&deposit.material);
            let branch = builder.create_string(&deposit.branch);
            let rung = builder.create_string(&deposit.rung);
            // Always written, `""` included: *"not blocked"* is a statement about this turn, not an
            // absent field.
            let build_blocked_reason = builder.create_string(&deposit.build_blocked_reason);
            let default_kit_id = builder.create_string(&deposit.default_kit_id);
            let offered_kit_ids = create_ladder_areas(builder, &deposit.offered_kit_ids);
            // **Absent, not empty, where the sim published no curve** — `regrowthSamples`' own rule
            // one table over: an empty vector is *"no curve was sent"* and a client blanks its chart
            // on it, where a quarry's all-zero curve is the live reading *"this does not grow"*.
            let regrowth_samples = if deposit.regrowth_samples.is_empty() {
                None
            } else {
                Some(builder.create_vector(&deposit.regrowth_samples))
            };
            let upkeep_toe = create_upkeep_toe(builder, &deposit.upkeep_toe);
            fb::DepositState::create(
                builder,
                &fb::DepositStateArgs {
                    tileX: deposit.tile_x,
                    tileY: deposit.tile_y,
                    material: Some(material),
                    branch: Some(branch),
                    stock: deposit.stock,
                    capacity: deposit.capacity,
                    reachable: deposit.reachable,
                    rungFloorFraction: deposit.rung_floor_fraction,
                    perWorkerBiomass: deposit.per_worker_biomass,
                    regrowthSamples: regrowth_samples,
                    regrowthRate: deposit.regrowth_rate,
                    rung: Some(rung),
                    buildFraction: deposit.build_fraction,
                    ladderPosition: deposit.ladder_position,
                    sustainableTake: deposit.sustainable_take,
                    actualTake: deposit.actual_take,
                    turnsRemaining: deposit.turns_remaining,
                    upkeepDemand: deposit.upkeep_demand,
                    upkeepSupplied: deposit.upkeep_supplied,
                    upkeepShortfall: deposit.upkeep_shortfall,
                    upkeepWorkersNeeded: deposit.upkeep_workers_needed,
                    upkeepWorkersAtCompletion: deposit.upkeep_workers_at_completion,
                    upkeepNextWorkerTurn: deposit.upkeep_next_worker_turn,
                    upkeepToe: upkeep_toe,
                    hasNeglectGrace: deposit.has_neglect_grace,
                    neglectGraceRemaining: deposit.neglect_grace_remaining,
                    buildTurnsRemaining: deposit.build_turns_remaining,
                    buildBlockedReason: Some(build_blocked_reason),
                    isQueued: deposit.is_queued,
                    defaultKitId: Some(default_kit_id),
                    offeredKitIds: Some(offered_kit_ids),
                    // **WHAT THE WORKING'S OWN CREW SPENT KEEPING IT** (`docs/plan_site_crews.md`
                    // §2.5).
                    upkeepHands: deposit.upkeep_hands,
                    upkeepToolsShort: deposit.upkeep_tools_short,
                },
            )
        })
        .collect();
    builder.create_vector(&entries)
}

/// The inverse of [`create_deposits`]. **Exhaustive on purpose**, like the herd and patch literals
/// beside it: a field appended to `DepositState` and its serializer does not compile until its line
/// exists here, which is what stops an appended field reaching the client as nothing at all.
fn decode_deposit(deposit: fb::DepositState<'_>) -> DepositState {
    DepositState {
        tile_x: deposit.tileX(),
        tile_y: deposit.tileY(),
        material: text(deposit.material()),
        branch: text(deposit.branch()),
        stock: deposit.stock(),
        capacity: deposit.capacity(),
        reachable: deposit.reachable(),
        rung_floor_fraction: deposit.rungFloorFraction(),
        per_worker_biomass: deposit.perWorkerBiomass(),
        regrowth_samples: decode_scalars(deposit.regrowthSamples()),
        regrowth_rate: deposit.regrowthRate(),
        rung: text(deposit.rung()),
        build_fraction: deposit.buildFraction(),
        ladder_position: deposit.ladderPosition(),
        sustainable_take: deposit.sustainableTake(),
        actual_take: deposit.actualTake(),
        turns_remaining: deposit.turnsRemaining(),
        upkeep_demand: deposit.upkeepDemand(),
        upkeep_supplied: deposit.upkeepSupplied(),
        upkeep_shortfall: deposit.upkeepShortfall(),
        upkeep_workers_needed: deposit.upkeepWorkersNeeded(),
        upkeep_workers_at_completion: deposit.upkeepWorkersAtCompletion(),
        upkeep_next_worker_turn: deposit.upkeepNextWorkerTurn(),
        upkeep_toe: decode_upkeep_toe(deposit.upkeepToe()),
        has_neglect_grace: deposit.hasNeglectGrace(),
        neglect_grace_remaining: deposit.neglectGraceRemaining(),
        build_turns_remaining: deposit.buildTurnsRemaining(),
        build_blocked_reason: text(deposit.buildBlockedReason()),
        is_queued: deposit.isQueued(),
        default_kit_id: text(deposit.defaultKitId()),
        offered_kit_ids: decode_strings(deposit.offeredKitIds()),
        upkeep_hands: deposit.upkeepHands(),
        upkeep_tools_short: deposit.upkeepToolsShort(),
    }
}

/// The inverse of [`create_ladder_branches`] - the ladder's branches, once per world.
fn decode_ladder_branch(branch: fb::LadderBranchState<'_>) -> LadderBranchState {
    LadderBranchState {
        branch: text(branch.branch()),
        area: text(branch.area()),
        source_kind: text(branch.sourceKind()),
        label: text(branch.label()),
    }
}

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` above, in the same order. The herd and patch
// literals are EXHAUSTIVE on purpose: a field appended to either struct and its serializer does
// not compile until its line exists here.
// ---------------------------------------------------------------------------

pub(crate) fn decode_subsistence_section(
    section: fb::SubsistenceSection<'_>,
    snapshot: &mut WorldSnapshot,
) {
    snapshot.herds = map_rows(section.herds(), decode_herd);
    snapshot.forage_patches = map_rows(section.foragePatches(), decode_forage_patch);
    snapshot.sedentarization = map_rows(section.sedentarization(), decode_sedentarization);
    snapshot.intensification_knowledge = map_rows(
        section.intensificationKnowledge(),
        decode_intensification_knowledge,
    );
    snapshot.ladder_knowledge = map_rows(section.ladderKnowledge(), decode_ladder_knowledge);
    snapshot.ladder_areas = decode_strings(section.ladderAreas());
    snapshot.food_modules = map_rows(section.foodModules(), decode_food_module);
    snapshot.kits = map_rows(section.kits(), decode_kit);
    snapshot.default_hunt_kit_id = text(section.defaultHuntKitId());
    snapshot.default_forage_kit_id = text(section.defaultForageKitId());
    snapshot.default_scout_kit_id = text(section.defaultScoutKitId());
    snapshot.default_warrior_kit_id = text(section.defaultWarriorKitId());
    snapshot.default_expedition_kit_id = text(section.defaultExpeditionKitId());
    snapshot.default_extract_kit_id = text(section.defaultExtractKitId());
    snapshot.equipment_config_json = text(section.equipmentConfigJson());
    snapshot.materials = map_rows(section.materials(), decode_material);
    snapshot.characteristic_bands =
        map_rows(section.characteristicBands(), decode_characteristic_band);
    snapshot.recipes = map_rows(section.recipes(), decode_recipe);
    snapshot.craft_knowledge = map_rows(section.craftKnowledge(), decode_craft_knowledge);
    snapshot.ladder_rungs = map_rows(section.ladderRungs(), decode_ladder_rung);
    snapshot.deposits = map_rows(section.deposits(), decode_deposit);
    snapshot.ladder_branches = map_rows(section.ladderBranches(), decode_ladder_branch);
}

pub(crate) fn decode_subsistence_section_delta(
    section: fb::SubsistenceSection<'_>,
    delta: &mut WorldDelta,
) {
    delta.herds = map_rows_if_present(section.herds(), decode_herd);
    delta.forage_patches = map_rows_if_present(section.foragePatches(), decode_forage_patch);
    delta.sedentarization = map_rows_if_present(section.sedentarization(), decode_sedentarization);
    delta.intensification_knowledge = map_rows_if_present(
        section.intensificationKnowledge(),
        decode_intensification_knowledge,
    );
    delta.ladder_knowledge =
        map_rows_if_present(section.ladderKnowledge(), decode_ladder_knowledge);
    delta.ladder_areas = section
        .ladderAreas()
        .map(|areas| areas.iter().map(str::to_owned).collect());
    delta.food_modules = map_rows_if_present(section.foodModules(), decode_food_module);
    delta.kits = map_rows_if_present(section.kits(), decode_kit);
    delta.default_hunt_kit_id = section.defaultHuntKitId().map(str::to_owned);
    delta.default_forage_kit_id = section.defaultForageKitId().map(str::to_owned);
    delta.default_scout_kit_id = section.defaultScoutKitId().map(str::to_owned);
    delta.default_warrior_kit_id = section.defaultWarriorKitId().map(str::to_owned);
    delta.default_expedition_kit_id = section.defaultExpeditionKitId().map(str::to_owned);
    delta.default_extract_kit_id = section.defaultExtractKitId().map(str::to_owned);
    delta.equipment_config_json = section.equipmentConfigJson().map(str::to_owned);
    delta.materials = map_rows_if_present(section.materials(), decode_material);
    delta.characteristic_bands =
        map_rows_if_present(section.characteristicBands(), decode_characteristic_band);
    delta.recipes = map_rows_if_present(section.recipes(), decode_recipe);
    delta.craft_knowledge = map_rows_if_present(section.craftKnowledge(), decode_craft_knowledge);
    delta.ladder_rungs = map_rows_if_present(section.ladderRungs(), decode_ladder_rung);
    delta.deposits = map_rows_if_present(section.deposits(), decode_deposit);
    delta.ladder_branches = map_rows_if_present(section.ladderBranches(), decode_ladder_branch);
}

fn decode_material(state: fb::MaterialDefState<'_>) -> MaterialDefState {
    MaterialDefState {
        id: text(state.id()),
        craft: text(state.craft()),
        axes: decode_strings(state.axes()),
        hand_workable: state.handWorkable(),
        hand_working_rate: state.handWorkingRate(),
        hand_working_quality_ceiling: state.handWorkingQualityCeiling(),
        tool_item_id: text(state.toolItemId()),
    }
}

fn decode_characteristic_band(state: fb::CharacteristicBandState<'_>) -> CharacteristicBandState {
    CharacteristicBandState {
        name: text(state.name()),
        from: state.from(),
    }
}

fn decode_recipe(state: fb::RecipeDefState<'_>) -> RecipeDefState {
    RecipeDefState {
        id: text(state.id()),
        display_name: text(state.displayName()),
        craft: text(state.craft()),
        group: text(state.group()),
        work: state.work(),
        requires_knowledge: decode_strings(state.requiresKnowledge()),
        inputs: map_rows(state.inputs(), |input| RecipeInputState {
            material_id: text(input.materialId()),
            amount: input.amount(),
            reads_axis: text(input.readsAxis()),
        }),
        outputs: map_rows(state.outputs(), |output| RecipeOutputState {
            equipment_id: text(output.equipmentId()),
            material_id: text(output.materialId()),
            amount: output.amount(),
        }),
        label: text(state.label()),
    }
}

fn decode_craft_knowledge(state: fb::CraftKnowledgeState<'_>) -> CraftKnowledgeState {
    CraftKnowledgeState {
        faction: state.faction(),
        craft_id: text(state.craftId()),
        display_name: text(state.displayName()),
        known: state.known(),
        progress: state.progress(),
        completion_threshold: state.completionThreshold(),
    }
}

fn decode_kit(state: fb::KitOption<'_>) -> KitOptionState {
    KitOptionState {
        id: text(state.id()),
        display_name: text(state.displayName()),
        jobs: decode_strings(state.jobs()),
        attack: state.attack(),
        hunt_carry_per_worker_biomass: state.huntCarryPerWorkerBiomass(),
        forage_carry_per_worker_biomass: state.forageCarryPerWorkerBiomass(),
        scout_vantage_range: state.scoutVantageRange(),
        attack_min_body_mass: state.attackMinBodyMass(),
        attack_max_body_mass: state.attackMaxBodyMass(),
        dispersion: state.dispersion(),
        exposure: state.exposure(),
        item_ids: decode_strings(state.itemIds()),
        build_rate: state.buildRate(),
        build_work_per_worker: state.buildWorkPerWorker(),
        build_work_branch: text(state.buildWorkBranch()),
        build_work_rung: text(state.buildWorkRung()),
        expedition_sight_range: state.expeditionSightRange(),
    }
}

fn decode_sedentarization(state: fb::SedentarizationState<'_>) -> SedentarizationState {
    SedentarizationState {
        faction: state.faction(),
        score: state.score(),
        stage: text(state.stage()),
        belief_points: state.beliefPoints(),
    }
}

/// `None` crosses as [`crate::NO_BUILD_DESTINATION_CAPACITY`] rather than as `0` (a real
/// capacity a real source can have) — see the herd serializer.
fn decode_build_destination_capacity(raw: f32) -> Option<f32> {
    (raw != crate::NO_BUILD_DESTINATION_CAPACITY).then_some(raw)
}

fn decode_build_legs(
    legs: Option<flatbuffers::Vector<'_, ForwardsUOffset<fb::BuildLegState<'_>>>>,
) -> Vec<BuildLegState> {
    map_rows(legs, |leg| BuildLegState {
        rung: text(leg.rung()),
        work_remaining: leg.workRemaining(),
        turns_remaining: leg.turnsRemaining(),
    })
}

fn decode_herd(herd: fb::HerdTelemetryState<'_>) -> HerdTelemetryState {
    HerdTelemetryState {
        id: text(herd.id()),
        label: text(herd.label()),
        species: text(herd.species()),
        x: herd.x(),
        y: herd.y(),
        biomass: herd.biomass(),
        route_length: herd.routeLength(),
        next_x: herd.nextX(),
        next_y: herd.nextY(),
        size_class: text(herd.sizeClass()),
        huntable: herd.huntable(),
        ecology_phase: text(herd.ecologyPhase()),
        per_worker_yield: herd.perWorkerYield(),
        pen_fed_fraction: herd.penFedFraction(),
        carrying_capacity: herd.carryingCapacity(),
        graze_range_radius: herd.grazeRangeRadius(),
        pen_radius: herd.penRadius(),
        pen_footprint_tiles: herd.penFootprintTiles(),
        pen_pasture_fraction: herd.penPastureFraction(),
        pen_extend_progress: herd.penExtendProgress(),
        husbandry_ceiling: text(herd.husbandryCeiling()),
        body_mass: herd.bodyMass(),
        food_per_animal: herd.foodPerAnimal(),
        herders_needed: herd.herdersNeeded(),
        herded_fraction: herd.herdedFraction(),
        fodder_draw: herd.fodderDraw(),
        pen_fodder_shortfall: herd.penFodderShortfall(),
        attack: herd.attack(),
        defense: herd.defense(),
        ferocity: herd.ferocity(),
        aggression: herd.aggression(),
        prey_sense_radius: herd.preySenseRadius(),
        herders_needed_if_managed: herd.herdersNeededIfManaged(),
        upkeep_demand: herd.upkeepDemand(),
        upkeep_supplied: herd.upkeepSupplied(),
        upkeep_shortfall: herd.upkeepShortfall(),
        upkeep_workers_needed: herd.upkeepWorkersNeeded(),
        upkeep_workers_at_completion: herd.upkeepWorkersAtCompletion(),
        upkeep_next_worker_turn: herd.upkeepNextWorkerTurn(),
        upkeep_toe: decode_upkeep_toe(herd.upkeepToe()),
        has_neglect_grace: herd.hasNeglectGrace(),
        neglect_grace_remaining: herd.neglectGraceRemaining(),
        provisions_per_biomass: herd.provisionsPerBiomass(),
        fodder_per_biomass: herd.fodderPerBiomass(),
        per_worker_biomass: herd.perWorkerBiomass(),
        // Absent when empty on the wire; absent reads back as the empty curve it stood for.
        regrowth_samples: decode_scalars(herd.regrowthSamples()),
        collapse_fraction: herd.collapseFraction(),
        stressed_fraction: herd.stressedFraction(),
        engage_rate: herd.engageRate(),
        durability: herd.durability(),
        stay_fraction: herd.stayFraction(),
        default_kit_id: text(herd.defaultKitId()),
        material_per_biomass: decode_material_payoffs(herd.materialPerBiomass()),
        per_worker_material: decode_material_payoffs(herd.perWorkerMaterial()),
        build_turns_remaining: herd.buildTurnsRemaining(),
        build_work_from_gear: herd.buildWorkFromGear(),
        build_work_per_worker_turn: herd.buildWorkPerWorkerTurn(),
        meter_rot_per_turn: herd.meterRotPerTurn(),
        build_queue_position: herd.buildQueuePosition(),
        build_blocked_reason: text(herd.buildBlockedReason()),
        build_destination_rung: text(herd.buildDestinationRung()),
        build_legs: decode_build_legs(herd.buildLegs()),
        rung_quotes: decode_rung_quotes(herd.rungQuotes()),
        build_destination_capacity: decode_build_destination_capacity(
            herd.buildDestinationCapacity(),
        ),
        pen_extend_cost: herd.penExtendCost(),
        standing_output_fraction: herd.standingOutputFraction(),
        standing_output_target: herd.standingOutputTarget(),
        output_recommit_progress: herd.outputRecommitProgress(),
        output_recommit_cost: herd.outputRecommitCost(),
        current_rung: text(herd.currentRung()),
        build_material_cost: decode_material_payoffs(herd.buildMaterialCost()),
        upkeep_material_demand: decode_material_payoffs(herd.upkeepMaterialDemand()),
        upkeep_material_supplied: decode_material_payoffs(herd.upkeepMaterialSupplied()),
        upkeep_hands: herd.upkeepHands(),
        upkeep_tools_short: herd.upkeepToolsShort(),
        predator_eaten_by: text(herd.predatorEatenBy()),
        keeping: text(herd.keeping()),
        next_ground_x: herd.nextGroundX(),
        next_ground_y: herd.nextGroundY(),
        predator_eaten: herd.predatorEaten(),
    }
}

fn decode_forage_patch(patch: fb::ForagePatchState<'_>) -> ForagePatchState {
    ForagePatchState {
        x: patch.x(),
        y: patch.y(),
        // The pair `hasOwner`/`owner` is how the serializer spells `Option`.
        owner: patch.hasOwner().then(|| patch.owner()),
        biomass: patch.biomass(),
        carrying_capacity: patch.carryingCapacity(),
        ecology_phase: text(patch.ecologyPhase()),
        per_worker_yield: patch.perWorkerYield(),
        composition: decode_flora_shares(patch.composition()).into(),
        composition_standing_biomass: decode_scalars(patch.compositionStandingBiomass()),
        composition_provisions_per_biomass: decode_scalars(patch.compositionProvisionsPerBiomass()),
        composition_fodder_per_biomass: decode_scalars(patch.compositionFodderPerBiomass()),
        composition_material_per_biomass: map_rows(
            patch.compositionMaterialPerBiomass(),
            |entry| SpeciesMaterialRates {
                rows: decode_material_payoffs(entry.rows()),
            },
        ),
        committed_species: text(patch.committedSpecies()),
        committed_display_name: text(patch.committedDisplayName()),
        upkeep_demand: patch.upkeepDemand(),
        upkeep_supplied: patch.upkeepSupplied(),
        upkeep_shortfall: patch.upkeepShortfall(),
        upkeep_workers_needed: patch.upkeepWorkersNeeded(),
        upkeep_workers_at_completion: patch.upkeepWorkersAtCompletion(),
        upkeep_next_worker_turn: patch.upkeepNextWorkerTurn(),
        upkeep_toe: decode_upkeep_toe(patch.upkeepToe()),
        has_neglect_grace: patch.hasNeglectGrace(),
        neglect_grace_remaining: patch.neglectGraceRemaining(),
        provisions_per_biomass: patch.provisionsPerBiomass(),
        fodder_per_biomass: patch.fodderPerBiomass(),
        per_worker_biomass: patch.perWorkerBiomass(),
        regrowth_samples: decode_scalars(patch.regrowthSamples()),
        collapse_fraction: patch.collapseFraction(),
        stressed_fraction: patch.stressedFraction(),
        material_per_biomass: decode_material_payoffs(patch.materialPerBiomass()),
        per_worker_material: decode_material_payoffs(patch.perWorkerMaterial()),
        build_turns_remaining: patch.buildTurnsRemaining(),
        build_work_from_gear: patch.buildWorkFromGear(),
        build_work_per_worker_turn: patch.buildWorkPerWorkerTurn(),
        meter_rot_per_turn: patch.meterRotPerTurn(),
        build_queue_position: patch.buildQueuePosition(),
        build_blocked_reason: text(patch.buildBlockedReason()),
        build_destination_rung: text(patch.buildDestinationRung()),
        build_legs: decode_build_legs(patch.buildLegs()),
        rung_quotes: decode_rung_quotes(patch.rungQuotes()),
        tile_capacity: patch.tileCapacity(),
        build_destination_capacity: decode_build_destination_capacity(
            patch.buildDestinationCapacity(),
        ),
        current_rung: text(patch.currentRung()),
        build_material_cost: decode_material_payoffs(patch.buildMaterialCost()),
        upkeep_material_demand: decode_material_payoffs(patch.upkeepMaterialDemand()),
        upkeep_material_supplied: decode_material_payoffs(patch.upkeepMaterialSupplied()),
        upkeep_hands: patch.upkeepHands(),
        upkeep_tools_short: patch.upkeepToolsShort(),
    }
}

/// The inverse of [`create_flora_shares`], in the order the wire carries them.
fn decode_flora_shares(
    shares: Option<flatbuffers::Vector<'_, ForwardsUOffset<fb::FloraShareInfo<'_>>>>,
) -> Vec<FloraShareInfo> {
    map_rows(shares, |share| FloraShareInfo {
        species: text(share.species()),
        display_name: text(share.displayName()),
        share: share.share(),
        role: text(share.role()),
        rung_quotes: decode_rung_share_quotes(share.rungQuotes()),
    })
}

/// The inverse of [`create_material_payoffs`]: empty in, empty out, and absent reads as empty.
pub(crate) fn decode_material_payoffs(
    payoffs: Option<flatbuffers::Vector<'_, ForwardsUOffset<fb::MaterialPayoff<'_>>>>,
) -> Vec<MaterialPayoff> {
    map_rows(payoffs, |payoff| MaterialPayoff {
        material_id: text(payoff.materialId()),
        amount: payoff.amount(),
    })
}

fn decode_intensification_knowledge(
    state: fb::IntensificationKnowledgeState<'_>,
) -> IntensificationKnowledgeState {
    IntensificationKnowledgeState {
        faction: state.faction(),
        knowledges: map_rows(state.knowledges(), |knowledge| LadderKnowledgeProgress {
            knowledge_id: text(knowledge.knowledgeId()),
            progress: knowledge.progress(),
        }),
    }
}

fn decode_ladder_knowledge(state: fb::LadderKnowledgeState<'_>) -> LadderKnowledgeState {
    LadderKnowledgeState {
        knowledge_id: text(state.knowledgeId()),
        display_name: text(state.displayName()),
        branch: text(state.branch()),
        order: state.order(),
        is_step: state.isStep(),
        area: text(state.area()),
    }
}

/// The inverse of [`create_ladder_rungs`] - the ladder's rung catalog, once per world. Exhaustive
/// for [`decode_deposit`]'s reason: a field appended to the struct breaks this build.
fn decode_ladder_rung(state: fb::LadderRungState<'_>) -> LadderRungState {
    LadderRungState {
        rung_key: text(state.rungKey()),
        branch: text(state.branch()),
        order: state.order(),
        display_name: text(state.displayName()),
        label: text(state.label()),
        label_fallback: text(state.labelFallback()),
        verb: text(state.verb()),
        source_kind: text(state.sourceKind()),
        traits: state
            .traits()
            .map(|names| names.iter().map(str::to_string).collect())
            .unwrap_or_default(),
        unlock_knowledge: text(state.unlockKnowledge()),
        earns_knowledge: text(state.earnsKnowledge()),
        requires_rung: text(state.requiresRung()),
        work_cost: state.workCost(),
        upkeep_work_per_turn: state.upkeepWorkPerTurn(),
        build_material_cost: state.buildMaterialCost(),
        build_material_id: text(state.buildMaterialId()),
        build_work_per_worker_turn: state.buildWorkPerWorkerTurn(),
        friction_multiplier: state.frictionMultiplier(),
        holds_link_to_tiles: state.holdsLinkToTiles(),
        grants_sight: state.grantsSight(),
        yield_per_worker_turn: state.yieldPerWorkerTurn(),
        recovery_fraction: state.recoveryFraction(),
        regrowth_multiplier: state.regrowthMultiplier(),
        min_deposit_capacity: state.minDepositCapacity(),
        requires_held_rung_below: state.requiresHeldRungBelow(),
        requires_crop: state.requiresCrop(),
        requires_work_room: state.requiresWorkRoom(),
        requires_ownership: state.requiresOwnership(),
        places_source: state.placesSource(),
    }
}

fn decode_food_module(module: fb::FoodModuleState<'_>) -> FoodModuleState {
    FoodModuleState {
        x: module.x(),
        y: module.y(),
        module: text(module.module()),
        seasonal_weight: module.seasonalWeight(),
        kind: text(module.kind()),
    }
}
