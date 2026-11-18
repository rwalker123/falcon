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

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use bevy::prelude::*;
use serde::Deserialize;

pub use bands::{
    Ages, BandEntry, FactionSection, FactionSetup, LarderEntry, Outfit, ScenarioSetup,
    AGES_SUM_TOLERANCE,
};

use crate::command_seam::apply_set_starting_loadout;
use crate::components::BandId;
use crate::demographics_config::{DemographicsConfig, DemographicsConfigHandle};
use crate::equipment_config::{EquipmentConfig, EquipmentConfigHandle};
use crate::map_preset::{MapPresets, MapPresetsHandle};
use crate::materials_config::{MaterialsConfig, MaterialsConfigHandle};
use crate::start_profile::{
    StartProfile, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartProfiles,
    StartProfilesHandle,
};
use crate::starting_loadout::{KitAllocation, MaterialAllocation};

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
        ScenarioSetup::from_sections(&self.file.factions)
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
    match profile {
        Some(profile) if refusals.is_empty() => Ok(ScenarioPlan {
            file,
            profile,
            requested_counts,
        }),
        _ => Err(refusals),
    }
}

/// **What a successful load built**: the scenario ids of the bands, for the sections that name one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScenarioApplied {
    pub bands: BTreeMap<String, BandId>,
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
    let mut refusals = setup.take_refusals();
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
    if refusals.is_empty() {
        Ok(ScenarioApplied {
            bands: setup.spawned,
        })
    } else {
        Err(refusals)
    }
}
