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

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::bands::FactionSection;
use super::herds::HerdEntry;
use super::rungs::refuse_undeclared_herd;
use super::{ScenarioContext, ScenarioRefusal, WorldSection};
use crate::labor_commands::{parse_work_priority_level, LaborRole};

/// One labor entry, tagged by its verb: `{ "assign_labor": { ... } }`.
#[derive(Debug, Clone, PartialEq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LaborEntry {
    AssignLabor(AssignLaborEntry),
    WorkPriority(WorkPriorityEntry),
    BandOrder(BandOrderEntry),
}

/// The `action` token of a `band_order` entry that stores a target and a priority.
pub const BAND_ORDER_SET: &str = "set";
/// The `action` token that hands a kind (or one row of it) to the player.
pub const BAND_ORDER_RELEASE: &str = "release";
/// The `action` token that hands a kind (or one row of it) back to the chief.
pub const BAND_ORDER_CLAIM: &str = "claim";

/// `CommandPayload::BandOrder`, less `faction_id` and with `band_id` replaced by `band`. `target` and
/// `priority` belong to `set` (the wire's `BandOrderAction::Set` fields); a row is named by
/// `target_x` + `target_y` (a patch or working) or `herd_id` (a scenario herd id) and belongs to
/// `release` and `claim` (the wire's `Option<BandOrderRow>`).
#[derive(Debug, Clone, PartialEq, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("x-pick-tile" = ["target_x", "target_y"]))]
pub struct BandOrderEntry {
    /// The band's id in this file.
    #[schemars(extend("x-pick" = "band"))]
    pub band: String,
    pub kind: String,
    /// `"set"`, `"release"` or `"claim"`.
    pub action: String,
    pub target: Option<u32>,
    pub priority: Option<String>,
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    #[schemars(extend("x-pick" = "herd"))]
    pub herd_id: Option<String>,
}

/// `CommandPayload::AssignLabor`, less `faction_id`, `band_id` (now `band`) and the retired `policy`.
#[derive(Debug, Clone, PartialEq, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("x-pick-tile" = ["target_x", "target_y"]))]
pub struct AssignLaborEntry {
    /// The band's id in this file.
    #[schemars(extend("x-pick" = "band"))]
    pub band: String,
    pub role: String,
    pub workers: u32,
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    #[schemars(extend("x-pick" = "herd"))]
    pub fauna_id: Option<String>,
    pub species: Option<String>,
    pub floor: Option<f32>,
    pub kit_id: Option<String>,
    pub take_species: Vec<String>,
    pub move_with_herd: bool,
}

/// `CommandPayload::WorkPriority`, less `faction_id` and with `band_id` replaced by `band`.
#[derive(Debug, Clone, PartialEq, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("x-pick-tile" = ["target_x", "target_y"]))]
pub struct WorkPriorityEntry {
    /// The band's id in this file.
    #[schemars(extend("x-pick" = "band"))]
    pub band: String,
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    #[schemars(extend("x-pick" = "herd"))]
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
            Self::BandOrder(_) => "band_order",
        }
    }

    pub fn band(&self) -> &str {
        match self {
            Self::AssignLabor(entry) => &entry.band,
            Self::WorkPriority(entry) => &entry.band,
            Self::BandOrder(entry) => &entry.band,
        }
    }
}

/// **The entry with its scenario herd id swapped for the runtime id** `herds` maps it to, for the
/// replay. `None` when the entry names a herd the map does not hold.
pub(super) fn with_runtime_herd(
    entry: &LaborEntry,
    herds: &BTreeMap<String, String>,
) -> Option<LaborEntry> {
    let swap = |id: &Option<String>| match id {
        Some(scenario_id) => herds.get(scenario_id).cloned().map(Some),
        None => Some(None),
    };
    Some(match entry {
        LaborEntry::AssignLabor(order) => LaborEntry::AssignLabor(AssignLaborEntry {
            fauna_id: swap(&order.fauna_id)?,
            ..order.clone()
        }),
        LaborEntry::WorkPriority(order) => LaborEntry::WorkPriority(WorkPriorityEntry {
            herd_id: swap(&order.herd_id)?,
            ..order.clone()
        }),
        LaborEntry::BandOrder(order) => LaborEntry::BandOrder(BandOrderEntry {
            herd_id: swap(&order.herd_id)?,
            ..order.clone()
        }),
    })
}

/// Pass 1 for every faction's `labor` entries.
pub(super) fn check(
    factions: &[FactionSection],
    herds: &[HerdEntry],
    world: &WorldSection,
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let declared_herds: BTreeSet<&str> = herds.iter().map(|herd| herd.id.as_str()).collect();
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
                    if let Some(herd) = &entry.fauna_id {
                        refuse_undeclared_herd(
                            herd,
                            &format!("{at}.fauna_id"),
                            &declared_herds,
                            refusals,
                        );
                    }
                }
                LaborEntry::BandOrder(entry) => {
                    check_target(entry.target_x, entry.target_y, world, &at, refusals);
                    check_band_order(entry, &at, refusals);
                    if let Some(herd) = &entry.herd_id {
                        refuse_undeclared_herd(
                            herd,
                            &format!("{at}.herd_id"),
                            &declared_herds,
                            refusals,
                        );
                    }
                }
                LaborEntry::WorkPriority(entry) => {
                    check_target(entry.target_x, entry.target_y, world, &at, refusals);
                    if let Some(herd) = &entry.herd_id {
                        refuse_undeclared_herd(
                            herd,
                            &format!("{at}.herd_id"),
                            &declared_herds,
                            refusals,
                        );
                    }
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

/// A `band_order`'s action is one of three tokens; `set` carries a target and a priority (the wire's
/// `Set` fields, both required) and no row, `release` and `claim` carry neither and may name one row.
fn check_band_order(entry: &BandOrderEntry, at: &str, refusals: &mut Vec<ScenarioRefusal>) {
    match entry.action.as_str() {
        BAND_ORDER_SET => {
            if entry.target.is_none() {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.target"),
                    "a set names a target",
                ));
            }
            match &entry.priority {
                None => refusals.push(ScenarioRefusal::new(
                    format!("{at}.priority"),
                    "a set names a priority",
                )),
                Some(level) if parse_work_priority_level(level).is_none() => {
                    refusals.push(ScenarioRefusal::new(
                        format!("{at}.priority"),
                        format!("'{level}' is not a work priority level (high, normal or low)"),
                    ));
                }
                Some(_) => {}
            }
            if entry.target_x.is_some() || entry.target_y.is_some() || entry.herd_id.is_some() {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.action"),
                    "a set names no row; release or claim moves one",
                ));
            }
        }
        BAND_ORDER_RELEASE | BAND_ORDER_CLAIM => {
            if entry.target.is_some() || entry.priority.is_some() {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.action"),
                    "only a set names a target and a priority",
                ));
            }
            if entry.herd_id.is_some() && (entry.target_x.is_some() || entry.target_y.is_some()) {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.herd_id"),
                    "a row is a tile or a herd, not both",
                ));
            }
        }
        other => refusals.push(ScenarioRefusal::new(
            format!("{at}.action"),
            format!("'{other}' is not a band order action (set, release or claim)"),
        )),
    }
}

/// The wire's `BandOrderAction` and row for a checked entry; the row is `None` for a whole kind.
pub(super) fn band_order_wire(
    entry: &BandOrderEntry,
) -> Option<(
    sim_runtime::BandOrderAction,
    Option<sim_runtime::BandOrderRow>,
)> {
    use sim_runtime::{BandOrderAction, BandOrderRow};
    let action = match entry.action.as_str() {
        BAND_ORDER_SET => BandOrderAction::Set {
            target: entry.target?,
            priority: entry.priority.clone()?,
        },
        BAND_ORDER_RELEASE => BandOrderAction::Release,
        BAND_ORDER_CLAIM => BandOrderAction::Claim,
        _ => return None,
    };
    let row = match (&entry.herd_id, entry.target_x, entry.target_y) {
        (Some(id), None, None) => Some(BandOrderRow::Herd { id: id.clone() }),
        (None, Some(x), Some(y)) => Some(BandOrderRow::Tile { x, y }),
        _ => None,
    };
    Some((action, row))
}
