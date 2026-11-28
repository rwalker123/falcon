//! **The `tribes` section of a scenario** (`.claude/rules/core_sim/leadership.md` -> "Scenario
//! section"): a tribe that stands at load instead of being earned over about eighteen turns.
//!
//! `"tribes": [{ "bands": ["home", "east", "south"] }]` on a faction. Each entry is founded in
//! `finish_scenario` through the game's own formation body ([`crate::tribe::found_scenario_tribe`],
//! the one `advance_tribes` runs for a group of close bands), so the id, the name, the promoted
//! leader, the scouting range and the `TribeFormed` line are the game's. What the game would have to
//! earn first the load establishes (one belief anchor, a tie both ways, a full points bank), so the
//! next turn keeps the tribe.

use std::collections::{BTreeMap, BTreeSet};

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::bands::{FactionSection, ScenarioSetup};
use super::ScenarioRefusal;
use crate::belief::BeliefRegistry;
use crate::components::BandId;
use crate::tribe::{found_scenario_tribe, MIN_TRIBE_BANDS};

/// One tribe the file founds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TribeEntry {
    /// The scenario ids of the member bands, all of this faction.
    pub bands: Vec<String>,
}

/// Pass 1: a tribe is two or more bands of this faction, each listed once in the whole faction.
pub(super) fn check(factions: &[FactionSection], refusals: &mut Vec<ScenarioRefusal>) {
    for (i, section) in factions.iter().enumerate() {
        let declared: BTreeSet<&str> = section.bands.iter().map(|band| band.id.as_str()).collect();
        let mut claimed: BTreeMap<&str, usize> = BTreeMap::new();
        for (k, entry) in section.tribes.iter().flatten().enumerate() {
            let at = format!("factions[{i}].tribes[{k}]");
            if entry.bands.len() < MIN_TRIBE_BANDS {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.bands"),
                    format!("a tribe is {MIN_TRIBE_BANDS} or more bands"),
                ));
            }
            let mut within: BTreeSet<&str> = BTreeSet::new();
            for (j, band) in entry.bands.iter().enumerate() {
                let path = format!("{at}.bands[{j}]");
                if !declared.contains(band.as_str()) {
                    refusals.push(ScenarioRefusal::new(
                        path,
                        format!("'{band}' is not a band of this faction"),
                    ));
                } else if !within.insert(band.as_str()) {
                    refusals.push(ScenarioRefusal::new(
                        path,
                        format!("'{band}' is listed twice in this tribe"),
                    ));
                } else if let Some(first) = claimed.insert(band.as_str(), k) {
                    refusals.push(ScenarioRefusal::new(
                        path,
                        format!("'{band}' is already in factions[{i}].tribes[{first}]"),
                    ));
                }
            }
        }
    }
}

/// **Found every faction's tribes**, after the leaders are appointed (a tribe needs a chief among
/// its members) and before labor. The shared anchor is the strongest place of belief on the map
/// (a `tiles[].belief` entry, or the dead the build turn buried): a tribe is a people gathered
/// around one, so a file with none is refused. Refusals are collected.
pub(super) fn apply(world: &mut World, setup: &ScenarioSetup, refusals: &mut Vec<ScenarioRefusal>) {
    let mut founded = false;
    for faction_setup in &setup.factions {
        for (k, entry) in faction_setup.tribes.iter().flatten().enumerate() {
            let at = format!("factions[{}].tribes[{k}]", faction_setup.file_index);
            let members: BTreeSet<BandId> = entry
                .bands
                .iter()
                .filter_map(|id| setup.spawned.get(id).copied())
                .collect();
            let anchor = world
                .resource::<BeliefRegistry>()
                .iter()
                .fold(None::<(UVec2, f32)>, |best, (tile, belief)| match best {
                    Some((_, held)) if held >= belief => best,
                    _ => Some((tile, belief)),
                })
                .map(|(tile, _)| tile);
            let Some(anchor) = anchor else {
                refusals.push(ScenarioRefusal::new(
                    &at,
                    "a tribe gathers around a place of belief: give a `tiles` entry a `belief`",
                ));
                continue;
            };
            if let Err(reason) =
                found_scenario_tribe(world, faction_setup.faction, &members, anchor)
            {
                refusals.push(ScenarioRefusal::new(&at, reason));
                continue;
            }
            founded = true;
        }
    }
    // A tribe founded here has no resolved stage yet: resolve it now, through the game's own system,
    // so the world opens (and publishes) with its settlement's stage rather than a turn late.
    if founded {
        world.run_system_once(crate::advance_settlements);
    }
}
