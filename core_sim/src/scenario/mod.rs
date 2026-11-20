//! **Scenarios: a recipe for a world, not a world** (`docs/plan_scenarios.md`).
//!
//! A scenario file says *generate this seed, put a band of 150 here*, and the current code carries
//! the instructions out, so the file never goes stale the way a save does. Loading has two passes:
//!
//! 1. [`parse_scenario`] parses the file and resolves every name against the loaded configs, with no
//!    world built. **It collects every refusal**, so one load lists all of a file's faults.
//! 2. The server builds a candidate world with the setup installed before `Startup`
//!    ([`ScenarioSetup`]: worldgen spawns the scenario's bands in place of the profile's), then
//!    [`finish_scenario`] replays the file's command entries through [`crate::command_seam`].
//!
//! **Fail closed and whole**: any refusal drops the candidate and the previous world stays.
//!
//! Each section of the file is its own module (`bands` today), so two worktrees adding two sections
//! touch two files.

pub mod bands;
pub mod herds;
pub mod labor;
pub mod rungs;
pub mod tiles;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use bevy::prelude::*;
use serde::Deserialize;

pub use bands::{
    Ages, BandEntry, FactionSection, FactionSetup, LarderEntry, Outfit, ScenarioSetup,
    AGES_SUM_TOLERANCE,
};
pub use herds::{HerdEntry, HerdOrigin, ScenarioHerdSetup};
pub use labor::LaborEntry;
pub use tiles::TileOverride;

use crate::command_seam::{
    apply_assign_labor, apply_set_starting_loadout, apply_work_priority, AssignLaborOrder,
};
use crate::components::BandId;
use crate::demographics_config::{DemographicsConfig, DemographicsConfigHandle};
use crate::equipment_config::{EquipmentConfig, EquipmentConfigHandle};
use crate::extraction_config::{ExtractionConfig, ExtractionConfigHandle};
use crate::fauna_config::{FaunaConfig, FaunaConfigHandle};
use crate::flora_config::{FloraConfig, FloraConfigHandle};
use crate::intensification::{LadderConfig, LadderConfigHandle};
use crate::labor_commands::BuildSourceRef;
use crate::map_preset::{MapPresets, MapPresetsHandle};
use crate::materials_config::{MaterialsConfig, MaterialsConfigHandle};
use crate::resources::SimulationConfig;
use crate::run_turn;
use crate::start_profile::{
    StartProfile, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartProfiles,
    StartProfilesHandle,
};
use crate::starting_loadout::{KitAllocation, MaterialAllocation};
use crate::world_roster::{apply_start_profile, seed_faction_roster};
use crate::{faction_start_land_fraction, granted_archetype_counts};

/// The path a refusal carries when the file itself could not be read.
pub const FILE_REFUSAL_PATH: &str = "<file>";

/// **One reason a scenario was refused**, named by where in the file it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioRefusal {
    /// `factions[0].bands[1].outfit.kits.big_gam`, `world.seed`, ...
    pub path: String,
    pub reason: String,
}

impl ScenarioRefusal {
    pub fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for ScenarioRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.reason)
    }
}

/// The scenario file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFile {
    pub name: String,
    pub note: String,
    pub world: WorldSection,
    pub factions: Vec<FactionSection>,
    /// The tile overrides, applied as a worldgen input. A required key: `[]` for none.
    pub tiles: Vec<tiles::TileOverride>,
    /// Wild herds placed or set. Required: an empty list is fine, an absent key is not.
    pub herds: Vec<HerdEntry>,
}

/// The `new_game` arguments, every one required.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldSection {
    pub preset: String,
    pub width: u32,
    pub height: u32,
    /// Never `0`: `new_game` reads `0` as "pick a random seed", and a scenario names a real one.
    pub seed: u64,
    pub profile: String,
    #[serde(deserialize_with = "bands::unique_keys")]
    pub archetypes: BTreeMap<String, u32>,
}

/// **The configs pass 1 resolves names against.**
pub struct ScenarioContext {
    pub presets: Arc<MapPresets>,
    pub profiles: Arc<StartProfiles>,
    pub tags: Arc<StartProfileKnowledgeTags>,
    pub demographics: Arc<DemographicsConfig>,
    pub equipment: Arc<EquipmentConfig>,
    pub materials: Arc<MaterialsConfig>,
    pub ladder: Arc<LadderConfig>,
    pub flora: Arc<FloraConfig>,
    pub extraction: Arc<ExtractionConfig>,
    pub fauna: Arc<FaunaConfig>,
}

impl ScenarioContext {
    /// The configs the world holds now.
    pub fn from_world(world: &World) -> Self {
        Self {
            presets: world.resource::<MapPresetsHandle>().get(),
            profiles: world.resource::<StartProfilesHandle>().get(),
            tags: world.resource::<StartProfileKnowledgeTagsHandle>().get(),
            demographics: world.resource::<DemographicsConfigHandle>().get(),
            equipment: world.resource::<EquipmentConfigHandle>().get(),
            materials: world.resource::<MaterialsConfigHandle>().get(),
            ladder: world.resource::<LadderConfigHandle>().get(),
            flora: world.resource::<FloraConfigHandle>().get(),
            extraction: world.resource::<ExtractionConfigHandle>().get(),
            fauna: world.resource::<FaunaConfigHandle>().get(),
        }
    }
}

/// **A scenario that passed pass 1**: the file, and the start profile its `world` names.
#[derive(Debug, Clone)]
pub struct ScenarioPlan {
    pub file: ScenarioFile,
    pub profile: StartProfile,
    /// The archetype counts the `world` section asks for, in catalog order, before the grid clamps
    /// them: a listed id takes its count, an unlisted one its unattended count.
    pub requested_counts: Vec<u32>,
}

impl ScenarioPlan {
    /// The setup worldgen reads while it spawns.
    pub fn setup(&self) -> ScenarioSetup {
        ScenarioSetup::from_sections(&self.file.factions, &self.file.tiles)
    }
}

/// **Pass 1: parse the file and resolve every name, building nothing.**
///
/// A parse error is the one refusal there is (serde cannot go on past it). Past it, every fault is
/// collected.
pub fn parse_scenario(
    text: &str,
    ctx: &ScenarioContext,
) -> Result<ScenarioPlan, Vec<ScenarioRefusal>> {
    let file: ScenarioFile = match serde_json::from_str(text) {
        Ok(file) => file,
        Err(err) => {
            return Err(vec![ScenarioRefusal::new(
                format!("line {}, column {}", err.line(), err.column()),
                err.to_string(),
            )]);
        }
    };
    let mut refusals = Vec::new();
    let world = &file.world;
    if world.seed == 0 {
        refusals.push(ScenarioRefusal::new(
            "world.seed",
            "a scenario must name a seed (0 means a random one to new_game)",
        ));
    }
    if world.width == 0 {
        refusals.push(ScenarioRefusal::new("world.width", "the map needs a width"));
    }
    if world.height == 0 {
        refusals.push(ScenarioRefusal::new(
            "world.height",
            "the map needs a height",
        ));
    }
    if ctx.presets.get(&world.preset).is_none() {
        refusals.push(ScenarioRefusal::new(
            "world.preset",
            format!("'{}' is not a map preset", world.preset),
        ));
    }
    // **No `resolve_active_profile` here**: it falls back silently on an unknown id, and a scenario
    // that names a profile that is not there must be refused, not given another one.
    let profile = ctx.profiles.get(&world.profile).cloned();
    if profile.is_none() {
        refusals.push(ScenarioRefusal::new(
            "world.profile",
            format!("'{}' is not a start profile", world.profile),
        ));
    }
    let mut requested_counts = Vec::new();
    if let Some(profile) = &profile {
        let archetypes = &profile.overrides().archetypes;
        let mut picked = Vec::new();
        for (id, count) in &world.archetypes {
            if archetypes.iter().any(|archetype| &archetype.id == id) {
                picked.push(sim_runtime::ArchetypeCount {
                    id: id.clone(),
                    count: *count,
                });
            } else {
                refusals.push(ScenarioRefusal::new(
                    format!("world.archetypes.{id}"),
                    format!("'{id}' is not an archetype of the '{}' profile", profile.id),
                ));
            }
        }
        // The shared resolver `new_game` uses; every id was checked above, so it cannot refuse.
        if let Ok(counts) = crate::requested_archetype_counts(&picked, archetypes) {
            requested_counts = counts;
        }
    }
    bands::check(&file.factions, ctx, &mut refusals);
    rungs::check(&file.factions, ctx, &mut refusals);
    labor::check(&file.factions, &file.world, ctx, &mut refusals);
    tiles::check(
        &file.tiles,
        (file.world.width, file.world.height),
        ctx,
        &mut refusals,
    );
    herds::check(&file.herds, &file.world, ctx, &mut refusals);
    match profile {
        Some(profile) if refusals.is_empty() => Ok(ScenarioPlan {
            file,
            profile,
            requested_counts,
        }),
        _ => Err(refusals),
    }
}

/// **Build the candidate world for a scenario that passed pass 1, and finish it: the ONE body the
/// server's `load_scenario` and the tests both run.**
///
/// `app` is a freshly built headless app with no Startup run yet. In order: the `world` section's
/// grid, preset and seed go into its config; the profile the file names is applied
/// ([`apply_start_profile`]); the roster is seated from the `world.archetypes` counts, clamped to
/// what the grid seats ([`granted_archetype_counts`]) and re-seeded everywhere the boot path derives
/// from it ([`seed_faction_roster`]); the [`ScenarioSetup`] is installed before Startup; the build
/// turn runs; and [`finish_scenario`] collects the refusals and replays the command entries. The
/// caller owns what follows (the swap-in).
pub fn build_scenario_world(
    app: &mut App,
    plan: &ScenarioPlan,
) -> Result<ScenarioApplied, Vec<ScenarioRefusal>> {
    let world = &plan.file.world;
    let grid_size = UVec2::new(world.width, world.height);
    let land_fraction = {
        let presets = app.world.resource::<MapPresetsHandle>().get();
        faction_start_land_fraction(&presets, &world.preset)
    };
    {
        let mut config = app.world.resource_mut::<SimulationConfig>();
        config.grid_size = grid_size;
        config.map_preset_id = world.preset.clone();
        config.map_seed = world.seed;
    }
    let granted_counts = granted_archetype_counts(
        &plan.requested_counts,
        &plan.profile.overrides().archetypes,
        grid_size,
        land_fraction,
    );
    apply_start_profile(app, &plan.profile);
    seed_faction_roster(app, &granted_counts);
    app.insert_resource(plan.setup());
    app.insert_resource(ScenarioHerdSetup {
        herds: plan.file.herds.clone(),
    });
    run_turn(app);
    finish_scenario(&mut app.world)
}

/// **What a successful load built**: the scenario ids of the bands, for the sections that name one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScenarioApplied {
    pub bands: BTreeMap<String, BandId>,
    /// Scenario herd id to the runtime herd id, for placed and worldgen herds alike.
    pub herds: BTreeMap<String, String>,
}

/// **Pass 2, after the candidate's first turn has built the world: collect what worldgen could not
/// place, replay the command entries, and remove the setup.**
///
/// The replayed `outfit` goes through [`apply_set_starting_loadout`], the function the server's own
/// handler calls, so it replaces the default committed on the world-build pass under the carry cap,
/// and a refusal (`OverCarry` and the rest) names its entry with the handler's own reason.
pub fn finish_scenario(world: &mut World) -> Result<ScenarioApplied, Vec<ScenarioRefusal>> {
    let Some(mut setup) = world.remove_resource::<ScenarioSetup>() else {
        return Ok(ScenarioApplied::default());
    };
    let herd_setup = world.remove_resource::<ScenarioHerdSetup>();
    let mut refusals = setup.take_refusals();
    // Herds are a setter (before the outfit replay). Every fit refusal is collected before any herd
    // is applied, so a refused file changes nothing.
    let (fitted, herd_refusals) = match &herd_setup {
        Some(herd_setup) => herds::fit(world, &herd_setup.herds),
        None => (Vec::new(), Vec::new()),
    };
    refusals.extend(herd_refusals);
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let herds = herds::apply(world, fitted);
    // **Held rungs (the setters, plan section 2.8 step 5)**, faction order then file order, BEFORE
    // the outfit replay. Every refusal is collected; a refused entry does not stop the others.
    for faction_setup in &setup.factions {
        rungs::apply_held(world, faction_setup, &mut refusals);
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }
    for faction_setup in &setup.factions {
        let faction = faction_setup.faction;
        for (j, entry) in faction_setup.bands.iter().enumerate() {
            let Some(outfit) = &entry.outfit else {
                continue;
            };
            let Some(band) = setup.spawned.get(&entry.id).copied() else {
                continue;
            };
            let kits: Vec<KitAllocation> = outfit
                .kits
                .iter()
                .map(|(kit_id, count)| KitAllocation {
                    kit_id: kit_id.clone(),
                    count: *count,
                })
                .collect();
            let materials: Vec<MaterialAllocation> = outfit
                .materials
                .iter()
                .map(|(material_id, units)| MaterialAllocation {
                    material_id: material_id.clone(),
                    units: *units,
                })
                .collect();
            if let Err(refusal) =
                apply_set_starting_loadout(world, faction, band, &kits, &materials)
            {
                refusals.push(ScenarioRefusal::new(
                    format!("factions[{}].bands[{j}].outfit", faction_setup.file_index),
                    refusal.to_string(),
                ));
            }
        }
    }
    // **Labor after every faction's outfit** (plan section 2.8 step 6): the outfit is an ordinary
    // revision of the opening window, and a work row's gear is read against the outfit it ends up with.
    if !refusals.is_empty() {
        return Err(refusals);
    }
    for faction_setup in &setup.factions {
        let faction = faction_setup.faction;
        for (k, entry) in faction_setup.labor.iter().enumerate() {
            let path = format!("factions[{}].labor[{k}]", faction_setup.file_index);
            let Some(band) = setup.spawned.get(entry.band()).copied() else {
                refusals.push(ScenarioRefusal::new(
                    path,
                    format!("'{}' is not a band this scenario spawned", entry.band()),
                ));
                return Err(refusals);
            };
            let result = match entry {
                LaborEntry::AssignLabor(order) => apply_assign_labor(
                    world,
                    faction,
                    Some(band.0),
                    AssignLaborOrder {
                        role: order.role.clone(),
                        workers: order.workers,
                        target_x: order.target_x,
                        target_y: order.target_y,
                        fauna_id: order.fauna_id.clone(),
                        species: order.species.clone(),
                        floor: order.floor,
                        kit_id: order.kit_id.clone(),
                        take_species: order.take_species.clone(),
                        move_with_herd: order.move_with_herd,
                    },
                ),
                LaborEntry::WorkPriority(order) => apply_work_priority(
                    world,
                    faction,
                    band.0,
                    BuildSourceRef {
                        target_x: order.target_x,
                        target_y: order.target_y,
                        herd_id: order.herd_id.clone(),
                        material: order.material.clone(),
                        branch: order.branch.clone(),
                    },
                    order.level.clone(),
                ),
            };
            if let Err(refusal) = result {
                // **A refused entry stops the load there**: later rows may stand on this one.
                refusals.push(ScenarioRefusal::new(path, refusal.to_string()));
                return Err(refusals);
            }
        }
    }
    // **Builds after labor** (plan section 2.8 step 6): a declared build needs a band already
    // working its source, which the labor rows put there. A refused build stops the load there.
    for faction_setup in &setup.factions {
        rungs::apply_builds(world, faction_setup, &setup.spawned, &mut refusals);
        if !refusals.is_empty() {
            return Err(refusals);
        }
    }
    Ok(ScenarioApplied {
        bands: setup.spawned,
        herds,
    })
}
