//! **The `pens` section of a scenario** (`docs/plan_scenarios.md` sections 2.2 and 2.8, slice 2c): a
//! faction's opening pen verbs, replayed through the game's own `ExtendPen` and `SetHerdOutput`.
//!
//! An entry is the verb in snake_case with `faction_id` implied by the section. **A herd is named by
//! its scenario id** (the `herds` section's `id`), because a pen's tile is chosen at runtime: the
//! `extend_pen` entry has no `target_x` / `target_y`, the runtime pen tile is the herd's
//! `corralled_at`.
//!
//! Pass 1 ([`check`]) resolves the herd id and the fraction's range with no world. Replay
//! ([`apply`]) runs after the builds through [`crate::command_seam`] and stops the load at the first
//! entry the verb refuses, with the verb's own reason.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::bands::{FactionSection, FactionSetup};
use super::herds::HerdEntry;
use super::rungs::refuse_undeclared_herd;
use super::ScenarioRefusal;
use crate::command_seam::{apply_extend_pen, apply_set_herd_output, WHOLE_HERD_COMMITTED};
use crate::{HerdRegistry, NO_STANDING_COMMITMENT};

/// One pen entry, tagged by its verb: `{ "extend_pen": { "herd": "sheep" } }`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PenEntry {
    ExtendPen(ExtendPenEntry),
    SetHerdOutput(SetHerdOutputEntry),
}

/// `CommandPayload::ExtendPen`, naming the herd rather than the pen's tile.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExtendPenEntry {
    /// The herd's scenario id.
    pub herd: String,
}

/// `CommandPayload::SetHerdOutput`, naming the herd by its scenario id.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetHerdOutputEntry {
    /// The herd's scenario id.
    pub herd: String,
    /// The share of the herd that stands as milk, eggs and wool, in `[0, 1]`.
    pub fraction: f32,
}

impl PenEntry {
    fn herd(&self) -> &str {
        match self {
            Self::ExtendPen(entry) => &entry.herd,
            Self::SetHerdOutput(entry) => &entry.herd,
        }
    }

    /// The key the entry is filed under in the file, for refusal paths.
    fn verb(&self) -> &'static str {
        match self {
            Self::ExtendPen(_) => "extend_pen",
            Self::SetHerdOutput(_) => "set_herd_output",
        }
    }
}

/// **Pass 1 for every faction's `pens` entries**: the herd is declared, the fraction is in range.
/// Every fault is collected.
pub(super) fn check(
    factions: &[FactionSection],
    herds: &[HerdEntry],
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let declared_herds: BTreeSet<&str> = herds.iter().map(|herd| herd.id.as_str()).collect();
    for (i, section) in factions.iter().enumerate() {
        for (k, entry) in section.pens.iter().enumerate() {
            let at = format!("factions[{i}].pens[{k}].{}", entry.verb());
            refuse_undeclared_herd(
                entry.herd(),
                &format!("{at}.herd"),
                &declared_herds,
                refusals,
            );
            if let PenEntry::SetHerdOutput(output) = entry {
                let fraction = output.fraction;
                if !fraction.is_finite()
                    || !(NO_STANDING_COMMITMENT..=WHOLE_HERD_COMMITTED).contains(&fraction)
                {
                    refusals.push(ScenarioRefusal::new(
                        format!("{at}.fraction"),
                        format!(
                            "a herd's standing share must be between {NO_STANDING_COMMITMENT} and \
                             {WHOLE_HERD_COMMITTED}, not {fraction}"
                        ),
                    ));
                }
            }
        }
    }
}

/// **Replay one faction's `pens` entries**, in file order, through the command seam. **A refusal
/// stops the load at that entry**, with the verb's own reason.
pub(super) fn apply(
    world: &mut World,
    setup: &FactionSetup,
    herds: &BTreeMap<String, String>,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    for (k, entry) in setup.pens.iter().enumerate() {
        let at = format!("factions[{}].pens[{k}]", setup.file_index);
        let Some(runtime_id) = herds.get(entry.herd()) else {
            refusals.push(ScenarioRefusal::new(
                at,
                format!("herd '{}' was not applied", entry.herd()),
            ));
            return;
        };
        let result = match entry {
            PenEntry::ExtendPen(_) => {
                let pen = world
                    .resource::<HerdRegistry>()
                    .find(runtime_id)
                    .and_then(|herd| herd.corralled_at);
                match pen {
                    Some(tile) => apply_extend_pen(world, setup.faction, tile).map(|_| ()),
                    None => {
                        refusals.push(ScenarioRefusal::new(
                            at,
                            format!("{runtime_id} has no pen to extend."),
                        ));
                        return;
                    }
                }
            }
            PenEntry::SetHerdOutput(output) => {
                apply_set_herd_output(world, setup.faction, runtime_id, output.fraction).map(|_| ())
            }
        };
        if let Err(refusal) = result {
            refusals.push(ScenarioRefusal::new(at, refusal.to_string()));
            return;
        }
    }
}
