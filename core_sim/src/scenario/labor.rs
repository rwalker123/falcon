//! **The `labor` section of a scenario** (`docs/plan_scenarios.md` sections 2.2 and 2.8, slice 2a): a
//! faction's opening work rows, replayed through the game's own `assign_labor` and `work_priority`.
//!
//! A labor entry is the `CommandPayload` variant in snake_case with `faction_id` implied by the section
//! and `band_id` replaced by `band`, the band's id in this file. A field that is `Option` on the
//! payload stays `Option` here (that is the command's own contract); every other field is required.
//! `ForageTile` is not a variant: it is a retired verb, and `assign_labor` with the role `forage`
//! replaces it. The retired `policy` field is omitted.
//!
//! Pass 1 ([`check`]) resolves the names; everything that depends on the tile is the handler's job
//! at replay ([`super::finish_scenario`]), which stops the load at the first entry it refuses and
//! reports the handler's own reason.

use std::collections::BTreeSet;

use serde::Deserialize;

use super::bands::FactionSection;
use super::{ScenarioContext, ScenarioRefusal, WorldSection};
use crate::labor_commands::{parse_work_priority_level, LaborRole};

/// One labor entry, tagged by its verb: `{ "assign_labor": { ... } }`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaborEntry {
    AssignLabor(AssignLaborEntry),
    WorkPriority(WorkPriorityEntry),
}

/// `CommandPayload::AssignLabor`, less `faction_id`, `band_id` (now `band`) and the retired `policy`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignLaborEntry {
    /// The band's id in this file.
    pub band: String,
    pub role: String,
    pub workers: u32,
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    pub fauna_id: Option<String>,
    pub species: Option<String>,
    pub floor: Option<f32>,
    pub kit_id: Option<String>,
    pub take_species: Vec<String>,
    pub move_with_herd: bool,
}

/// `CommandPayload::WorkPriority`, less `faction_id` and with `band_id` replaced by `band`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkPriorityEntry {
    /// The band's id in this file.
    pub band: String,
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    pub herd_id: Option<String>,
    pub material: Option<String>,
    pub branch: Option<String>,
    /// `"high"`, `"normal"` or `"low"`.
    pub level: String,
}

impl LaborEntry {
    /// The key the entry is filed under in the file, for refusal paths.
    fn verb(&self) -> &'static str {
        match self {
            Self::AssignLabor(_) => "assign_labor",
            Self::WorkPriority(_) => "work_priority",
        }
    }

    pub fn band(&self) -> &str {
        match self {
            Self::AssignLabor(entry) => &entry.band,
            Self::WorkPriority(entry) => &entry.band,
        }
    }
}

/// Pass 1 for every faction's `labor` entries.
pub(super) fn check(
    factions: &[FactionSection],
    world: &WorldSection,
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    for (i, section) in factions.iter().enumerate() {
        let bands: BTreeSet<&str> = section.bands.iter().map(|band| band.id.as_str()).collect();
        for (k, entry) in section.labor.iter().enumerate() {
            let at = format!("factions[{i}].labor[{k}].{}", entry.verb());
            if !bands.contains(entry.band()) {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.band"),
                    format!("'{}' is not a band of this faction", entry.band()),
                ));
            }
            match entry {
                LaborEntry::AssignLabor(entry) => {
                    check_target(entry.target_x, entry.target_y, world, &at, refusals);
                    check_assign(entry, &at, ctx, refusals);
                }
                LaborEntry::WorkPriority(entry) => {
                    check_target(entry.target_x, entry.target_y, world, &at, refusals);
                    if parse_work_priority_level(&entry.level).is_none() {
                        refusals.push(ScenarioRefusal::new(
                            format!("{at}.level"),
                            format!(
                                "'{}' is not a work priority level (high, normal or low)",
                                entry.level
                            ),
                        ));
                    }
                }
            }
        }
    }
}

/// A target tile is both coordinates or neither, and on the map.
fn check_target(
    x: Option<u32>,
    y: Option<u32>,
    world: &WorldSection,
    at: &str,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    match (x, y) {
        (Some(x), Some(y)) => {
            if x >= world.width || y >= world.height {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.target_x"),
                    format!(
                        "tile ({x},{y}) is outside the {}x{} map",
                        world.width, world.height
                    ),
                ));
            }
        }
        (None, None) => {}
        (Some(_), None) => refusals.push(ScenarioRefusal::new(
            format!("{at}.target_y"),
            "a target names both target_x and target_y, or neither",
        )),
        (None, Some(_)) => refusals.push(ScenarioRefusal::new(
            format!("{at}.target_x"),
            "a target names both target_x and target_y, or neither",
        )),
    }
}

fn check_assign(
    entry: &AssignLaborEntry,
    at: &str,
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let role = LaborRole::from_token(&entry.role);
    if role.is_none() {
        refusals.push(ScenarioRefusal::new(
            format!("{at}.role"),
            format!("'{}' is not a labor role", entry.role),
        ));
    }
    if let Some(kit) = &entry.kit_id {
        if ctx.equipment.kit_definition(kit).is_none() {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.kit_id"),
                format!("'{kit}' is not a kit on the equipment roster"),
            ));
        }
    }
    // `species` names a plant only on a forage row: an extract row's rides it as a material and a
    // tile or band row's as a ladder branch, so those are the handler's to judge.
    if role == Some(LaborRole::Forage) {
        if let Some(species) = &entry.species {
            if !ctx.flora.species.contains_key(species.trim()) {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.species"),
                    format!("'{species}' is not a plant on the flora roster"),
                ));
            }
        }
    }
    for (l, species) in entry.take_species.iter().enumerate() {
        if !ctx.flora.species.contains_key(species) {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.take_species[{l}]"),
                format!("'{species}' is not a plant on the flora roster"),
            ));
        }
    }
}
