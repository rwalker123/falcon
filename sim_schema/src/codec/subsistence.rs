//! Subsistence-section FlatBuffers serialization.

use crate::codec::FbBuilder;
use crate::state::subsistence::{
    CharacteristicBandState, CraftKnowledgeState, FloraShareInfo, FoodModuleState,
    ForagePatchState, HerdTelemetryState, IntensificationKnowledgeState, KitOptionState,
    LadderKnowledgeState, MaterialDefState, MaterialPayoff, RecipeDefState, RouteRungState,
    SedentarizationState,
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
    let ladder_knowledge = create_ladder_knowledge(builder, &snapshot.ladder_knowledge);
    let food_modules = create_food_modules(builder, &snapshot.food_modules);
    let kits = create_kits(builder, &snapshot.kits);
    let default_hunt_kit_id = builder.create_string(&snapshot.default_hunt_kit_id);
    let default_forage_kit_id = builder.create_string(&snapshot.default_forage_kit_id);
    let default_scout_kit_id = builder.create_string(&snapshot.default_scout_kit_id);
    let default_warrior_kit_id = builder.create_string(&snapshot.default_warrior_kit_id);
    let equipment_config_json = builder.create_string(&snapshot.equipment_config_json);
    // The crafting catalogues — TYPED, not a second `equipmentConfigJson`: that blob has no gameplay
    // consumer, and a gameplay readout gets a field of its own rather than reaching into a string.
    let materials = create_materials(builder, &snapshot.materials);
    let characteristic_bands = create_characteristic_bands(builder, &snapshot.characteristic_bands);
    let recipes = create_recipes(builder, &snapshot.recipes);
    let craft_knowledge = create_craft_knowledge(builder, &snapshot.craft_knowledge);
    let route_rungs = create_route_rungs(builder, &snapshot.route_rungs);
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
            // The designer surface's read-only catalogue — the whole TOE config as one JSON string.
            // Workbench-only; see the schema comment.
            equipmentConfigJson: Some(equipment_config_json),
            materials: Some(materials),
            characteristicBands: Some(characteristic_bands),
            recipes: Some(recipes),
            craftKnowledge: Some(craft_knowledge),
            routeRungs: Some(route_rungs),
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
    let route_rungs = delta
        .route_rungs
        .as_ref()
        .map(|entries| create_route_rungs(builder, entries));
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
            equipmentConfigJson: equipment_config_json,
            materials,
            characteristicBands: characteristic_bands,
            recipes,
            craftKnowledge: craft_knowledge,
            routeRungs: route_rungs,
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
                // What the kit DOES beyond the tiers — all three neutral at 1.0, so a kit declaring
                // none of them encodes exactly as it did before they existed.
                attackMinBodyMass: state.attack_min_body_mass,
                attackMaxBodyMass: state.attack_max_body_mass,
                dispersion: state.dispersion,
                exposure: state.exposure,
                buildRate: state.build_rate,
                buildWorkPerWorker: state.build_work_per_worker,
                buildWorkBranch: Some(build_work_branch),
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
        // **Built before the parent table opens**, the ordinary FlatBuffers rule.
        let material_per_biomass = create_material_payoffs(builder, &herd.material_per_biomass);
        let per_worker_material = create_material_payoffs(builder, &herd.per_worker_material);
        let corral_material = create_material_payoffs(builder, &herd.corral_material);
        let pastoral_material = create_material_payoffs(builder, &herd.pastoral_material);
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
        let upkeep_kit_id = builder.create_string(herd.upkeep_kit_id.as_str());
        // **Always written, `""` included** — the empty string is *"no band has this queued"*, and a
        // client comparing its own selection against an absent field would read every source as a
        // mismatch, exactly as it would for `defaultKitId` above.
        let build_kit_id = builder.create_string(herd.build_kit_id.as_str());
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
        let tame_upkeep_material_demand =
            create_material_payoffs(builder, &herd.tame_upkeep_material_demand);
        let corral_upkeep_material_demand =
            create_material_payoffs(builder, &herd.corral_upkeep_material_demand);
        let corral_build_material_cost =
            create_material_payoffs(builder, &herd.corral_build_material_cost);
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
                // The Tame rung's payoff — appended last (append-only wire).
                pastoralYield: herd.pastoral_yield,
                // Hay this pen drew last turn (F3) — appended last (append-only wire).
                fodderDraw: herd.fodder_draw,
                // **How much more fodder the pen still needs** — appended last (append-only wire).
                // `max(0, hay need − fodderDraw)`, struck sim-side on the same pass as both its terms
                // so the difference can never describe a different turn from them. The gap it is
                // taken from rode this row as `penHayNeed` and is `(deprecated)`: nothing read it.
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
                // The two build dips are RETIRED: `tameBuildFraction`/`corralBuildFraction` are
                // `(deprecated)` slots and flatc emits no `Args` field for them.
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
                // The two investment rungs' material payoffs — appended last (append-only wire).
                corralMaterial: Some(corral_material),
                pastoralMaterial: Some(pastoral_material),
                // **The build, priced in WORK** — appended last (append-only wire,
                // docs/plan_unit_costed_work.md §8). The `domestication`/`corralProgress` fractions
                // above are exactly `workDone / workCost`; these two are what let the UI say
                // "18 of 50 work", and `workCost` is quoted whether or not a build is in flight.
                tameWorkDone: herd.tame_work_done,
                tameWorkCost: herd.tame_work_cost,
                corralWorkDone: herd.corral_work_done,
                corralWorkCost: herd.corral_work_cost,
                buildTurnsRemaining: herd.build_turns_remaining,
                buildWorkFromGear: herd.build_work_from_gear,
                buildWorkPerWorkerTurn: herd.build_work_per_worker_turn,
                // **The standing upkeep** — appended last (append-only wire,
                // docs/plan_standing_upkeep.md §2). All three terms ship, so the client subtracts
                // nothing; `upkeepDemand` follows `corralYield`'s always-meaningful rule.
                upkeepDemand: herd.upkeep_demand,
                upkeepSupplied: herd.upkeep_supplied,
                upkeepShortfall: herd.upkeep_shortfall,
                upkeepWorkersNeeded: herd.upkeep_workers_needed,
                // **The PRE-COMMIT rate** — appended last (append-only wire). `upkeepDemand` above
                // is what the KEEPING rung bills today; these are what each rung would cost to
                // hold, so a sheet quoting a Tame on an unstarted herd nets a rate rather than
                // subtracting zero. Read beside the `*WorkCost` of the same rung.
                tameUpkeepDemand: herd.tame_upkeep_demand,
                corralUpkeepDemand: herd.corral_upkeep_demand,
                meterRotPerTurn: herd.meter_rot_per_turn,
                // Where this herd sits in the winning band's queue — read as one set with
                // `buildTurnsRemaining` and `buildWorkFromGear`, which is what makes a chained date
                // legible (docs/plan_standing_upkeep.md 4.6b).
                buildQueuePosition: herd.build_queue_position,
                // …and WHY it is stuck, when it is. `""` = not blocked.
                buildBlockedReason: Some(build_blocked_reason),
                buildDestinationRung: Some(build_destination_rung),
                buildLegs: build_legs,
                // **WHERE THAT DESTINATION WILL LEAVE THIS HERD'S `K`** — appended last
                // (append-only wire). `None` is *"no band has queued it"*, which crosses as the
                // [`crate::NO_BUILD_DESTINATION_CAPACITY`] sentinel rather than as `0`: a capacity
                // of zero is a real reading a real herd has.
                buildDestinationCapacity: herd
                    .build_destination_capacity
                    .unwrap_or(crate::NO_BUILD_DESTINATION_CAPACITY),
                // **What this herd's build is being raised with** — appended last (append-only
                // wire). The RESOLVED kit of the winning band's queue entry; `""` when no band has
                // it queued.
                buildKitId: Some(build_kit_id),
                // **The pen ring's DENOMINATOR** — appended last (append-only wire). Rides beside
                // `penExtendProgress` above in the same work units; `0` with no ring in flight.
                penExtendCost: herd.pen_extend_cost,
                // **The rung this herd STANDS on** — appended last (append-only wire), the
                // twin of `buildDestinationRung`'s spelling at the source's own position.
                currentRung: Some(current_rung),
                buildMaterialCost: Some(build_material_cost),
                upkeepMaterialDemand: Some(upkeep_material_demand),
                upkeepMaterialSupplied: Some(upkeep_material_supplied),
                // **THE PRE-COMMIT MATERIAL QUOTE, PER RUNG** — see the plant twin. The `corral` one
                // is the number the `⌃` track's aside needs on a PASTORAL herd, whose own rung
                // declares no material at all.
                tameUpkeepMaterialDemand: Some(tame_upkeep_material_demand),
                corralUpkeepMaterialDemand: Some(corral_upkeep_material_demand),
                // **WHAT THIS SITE IS KEPT WITH** — the resolved kit, and whether a band stated it.
                upkeepKitId: Some(upkeep_kit_id),
                upkeepKitNamed: herd.upkeep_kit_named,
                // **WHAT A PEN RING SWALLOWS TO RAISE** — appended last (append-only wire), and
                // the material twin of `corralWorkCost`. It carries what `buildMaterialCost` above
                // cannot on a CORRALLED herd, where the rung above the pen is none.
                corralBuildMaterialCost: Some(corral_build_material_cost),
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
        let sow_site_refusal = builder.create_string(patch.sow_site_refusal.as_str());
        // Always written, `""` included — see the herd twin.
        let build_blocked_reason = builder.create_string(patch.build_blocked_reason.as_str());
        // **THE DESTINATION AND THE LEGS** — always written for the string (`""` is *"not queued"*,
        // a statement), and the legs **absent when empty** on this table's repeated-field convention:
        // *"this source has nothing left to climb"* and *"this source is not queued"* both read as no
        // list, which is what the destination string beside it disambiguates.
        let build_destination_rung = builder.create_string(patch.build_destination_rung.as_str());
        // **WHERE THE PATCH IS**, beside where it is going — see the herd twin.
        let current_rung = builder.create_string(patch.current_rung.as_str());
        let upkeep_kit_id = builder.create_string(patch.upkeep_kit_id.as_str());
        // Always written, `""` included — see the herd twin.
        let build_kit_id = builder.create_string(patch.build_kit_id.as_str());
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
        let cultivation_upkeep_material_demand =
            create_material_payoffs(builder, &patch.cultivation_upkeep_material_demand);
        let field_upkeep_material_demand =
            create_material_payoffs(builder, &patch.field_upkeep_material_demand);
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
                tendedFodder: patch.tended_fodder,
                fieldFodder: patch.field_fodder,
                // The two build dips are RETIRED — `(deprecated)` slots, no `Args` field.
                // The neglect grace — appended last. The two build-crew slots retired with
                // `crew_needed`; they are `(deprecated)` and flatc emits no `Args` field for them.
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
                // **The build, priced in WORK** — appended last (append-only wire,
                // docs/plan_unit_costed_work.md §8). The plant twin of the herd's pairs.
                cultivationWorkDone: patch.cultivation_work_done,
                cultivationWorkCost: patch.cultivation_work_cost,
                fieldWorkDone: patch.field_work_done,
                fieldWorkCost: patch.field_work_cost,
                buildTurnsRemaining: patch.build_turns_remaining,
                buildWorkFromGear: patch.build_work_from_gear,
                buildWorkPerWorkerTurn: patch.build_work_per_worker_turn,
                // **The standing upkeep** — appended last (append-only wire,
                // docs/plan_standing_upkeep.md §2). All three terms ship, so the client subtracts
                // nothing; `upkeepDemand` follows `corralYield`'s always-meaningful rule.
                upkeepDemand: patch.upkeep_demand,
                upkeepSupplied: patch.upkeep_supplied,
                upkeepShortfall: patch.upkeep_shortfall,
                upkeepWorkersNeeded: patch.upkeep_workers_needed,
                // **The PRE-COMMIT rate** — appended last (append-only wire), the plant twin of the
                // herd's pair: `upkeepDemand` above is what the AT-RISK rung bills today, and these
                // are what each rung would cost to hold, so a sheet quoting a Cultivate on a wild
                // patch nets a rate rather than subtracting zero.
                cultivationUpkeepDemand: patch.cultivation_upkeep_demand,
                fieldUpkeepDemand: patch.field_upkeep_demand,
                meterRotPerTurn: patch.meter_rot_per_turn,
                // The plant twin — see the herd row above.
                buildQueuePosition: patch.build_queue_position,
                buildBlockedReason: Some(build_blocked_reason),
                buildDestinationRung: Some(build_destination_rung),
                buildLegs: build_legs,
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
                // **What this patch's build is being raised with** — appended last (append-only
                // wire); see the herd twin.
                buildKitId: Some(build_kit_id),
                // **The rung this patch STANDS on** — appended last (append-only wire); see the
                // herd twin.
                currentRung: Some(current_rung),
                buildMaterialCost: Some(build_material_cost),
                upkeepMaterialDemand: Some(upkeep_material_demand),
                upkeepMaterialSupplied: Some(upkeep_material_supplied),
                // **THE PRE-COMMIT MATERIAL QUOTE, PER RUNG** — the rung's own rate at this source's
                // scale, NOT the stamped bill beside it. The two disagree mid-climb, and that is the
                // point: one says what you were billed, the other what this rung costs.
                cultivationUpkeepMaterialDemand: Some(cultivation_upkeep_material_demand),
                fieldUpkeepMaterialDemand: Some(field_upkeep_material_demand),
                // **WHAT THIS SITE IS KEPT WITH** — the resolved kit, and whether a band stated it.
                upkeepKitId: Some(upkeep_kit_id),
                upkeepKitNamed: patch.upkeep_kit_named,
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
        // **Built before the parent table opens**, the ordinary FlatBuffers rule: a nested vector
        // cannot be written while `FloraShareInfo` is under construction.
        let sow_materials = create_material_payoffs(builder, &share.sow_material_payoff);
        let cultivate_materials =
            create_material_payoffs(builder, &share.cultivate_material_payoff);
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
                // The same two accounts at the TENDED rung — appended last (append-only wire, #419).
                cultivateFodderPayoff: share.cultivate_fodder_payoff,
                // What the plant is FOR — a display tag off the roster, appended last (append-only
                // wire). `""` is "unstated", never "staple".
                role: Some(role),
                // **What a cash crop would pay, per material** — appended last (append-only wire,
                // arc #527), replacing the retired `sowTradePayoff`/`cultivateTradePayoff`. An EMPTY
                // vector is "no row", never "zero": a food crop yields no material and must render
                // nothing at all.
                sowMaterialPayoff: Some(sow_materials),
                cultivateMaterialPayoff: Some(cultivate_materials),
                // **What sowing THIS crop would cost in work** — appended last (append-only wire,
                // §4.15). `0` is "no figure" (the plant cannot climb to a Field here), never a free
                // Sow.
                sowWorkCost: share.sow_work_cost,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

/// One rung's per-material quote, as a wire vector. Empty in, empty out — the "no row" reading the
/// field's own contract rests on, so an absent quote never becomes a zero-valued one.
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
        entries.push(fb::LadderKnowledgeState::create(
            builder,
            &fb::LadderKnowledgeStateArgs {
                knowledgeId: Some(knowledge_id),
                displayName: Some(display_name),
                branch: Some(branch),
                order: state.order,
                isStep: state.is_step,
            },
        ));
    }
    builder.create_vector(&entries)
}

/// **THE ROUTE BRANCH'S RUNG CATALOG** — every rung the route branch declares, in climb order and
/// once per world. A per-world constant, written whole on a snapshot and only when it moved on a
/// delta, exactly like the roster above.
fn create_route_rungs<'a>(
    builder: &mut FbBuilder<'a>,
    states: &[RouteRungState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::RouteRungState<'a>>>> {
    let mut entries = Vec::with_capacity(states.len());
    for state in states {
        let rung_key = builder.create_string(&state.rung_key);
        let display_name = builder.create_string(&state.display_name);
        let verb = builder.create_string(&state.verb);
        let unlock_knowledge = builder.create_string(&state.unlock_knowledge);
        let requires_rung = builder.create_string(&state.requires_rung);
        entries.push(fb::RouteRungState::create(
            builder,
            &fb::RouteRungStateArgs {
                rungKey: Some(rung_key),
                order: state.order,
                displayName: Some(display_name),
                verb: Some(verb),
                unlockKnowledge: Some(unlock_knowledge),
                requiresRung: Some(requires_rung),
                workCost: state.work_cost,
                upkeepWorkPerTurn: state.upkeep_work_per_turn,
                frictionMultiplier: state.friction_multiplier,
                holdsLinkToTiles: state.holds_link_to_tiles,
                grantsSight: state.grants_sight,
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
