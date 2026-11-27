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

use serde::{Deserialize, Serialize};

use super::bands::FactionSection;
use super::herds::HerdEntry;
use super::rungs::refuse_undeclared_herd;
use bevy::prelude::{UVec2, World};

use super::{
    round_export, ExportIds, ExportedBand, ScenarioContext, ScenarioRefusal, WorldSection,
};
use crate::components::{
    LaborAllocation, LaborAssignment, LaborTarget, OrderOwner, SourcePriority, TakeSelection,
    DEFAULT_ESCAPEMENT_FLOOR,
};
use crate::labor_commands::{parse_work_priority_level, LaborRole};
use crate::orders::FactionId;
use crate::source_holders::{take_source_key, ScenarioLaborReservations};

/// A staffed row's `workers` when the player set no Max: the sim staffs the row.
const NO_MAX_WORKERS: u32 = 0;

/// One labor entry, tagged by its verb: `{ "assign_labor": { ... } }`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BandOrderEntry {
    /// The band's id in this file.
    pub band: String,
    pub kind: String,
    /// `"set"`, `"release"` or `"claim"`.
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_x: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_y: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub herd_id: Option<String>,
}

/// `CommandPayload::AssignLabor`, less `faction_id`, `band_id` (now `band`) and the retired `policy`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssignLaborEntry {
    /// The band's id in this file.
    pub band: String,
    pub role: String,
    pub workers: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_x: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_y: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fauna_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub species: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kit_id: Option<String>,
    pub take_species: Vec<String>,
    pub move_with_herd: bool,
}

/// `CommandPayload::WorkPriority`, less `faction_id` and with `band_id` replaced by `band`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkPriorityEntry {
    /// The band's id in this file.
    pub band: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_x: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_y: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub herd_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// **The take sources the file's own labor rows name, band by band** - what a chief's managing pass
/// must leave alone while the replay runs ([`ScenarioLaborReservations`]). A forage row names its
/// tile, a hunt row its herd (the runtime id `herds` maps the file's id to), an extract row its tile
/// and material; any other row is no take source.
pub(super) fn reservations(
    setup: &super::bands::ScenarioSetup,
    herds: &BTreeMap<String, String>,
) -> ScenarioLaborReservations {
    let mut reserved = ScenarioLaborReservations::default();
    for faction_setup in &setup.factions {
        for entry in &faction_setup.labor {
            let LaborEntry::AssignLabor(row) = entry else {
                continue;
            };
            let Some(band) = setup.spawned.get(&row.band) else {
                continue;
            };
            let tile = row
                .target_x
                .zip(row.target_y)
                .map(|(x, y)| UVec2::new(x, y));
            let target = match (LaborRole::from_token(&row.role), tile) {
                (Some(LaborRole::Forage), Some(tile)) => LaborTarget::Forage {
                    tile,
                    floor: DEFAULT_ESCAPEMENT_FLOOR,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                (Some(LaborRole::Hunt), _) => {
                    let Some(runtime_id) = row.fauna_id.as_ref().and_then(|id| herds.get(id))
                    else {
                        continue;
                    };
                    LaborTarget::Hunt {
                        fauna_id: runtime_id.clone(),
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        move_with_herd: false,
                    }
                }
                (Some(LaborRole::Extract), Some(tile)) => {
                    let Some(material) = row.species.clone() else {
                        continue;
                    };
                    LaborTarget::Extract {
                        tile,
                        material,
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                    }
                }
                _ => continue,
            };
            if let Some(key) = take_source_key(&target) {
                reserved.reserve(key, *band, faction_setup.faction);
            }
        }
    }
    reserved
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

/// **One faction's standing rows, as entries**: per band in ascending `BandId` order, every row as an
/// `assign_labor`, then a `work_priority` for each row ranked off Normal.
///
/// A staffed row's count is the player's Max (`0` is no Max, the sim staffs it), a scouting row's is its
/// range in tiles, and any other row's is its crew. A band's chief orders are written as `band_order`
/// entries (a `set` per order, and a whole-kind or one-row `release` where the player crews it), ahead
/// of that band's `assign_labor` rows; `band_order` needs a leader, which the band's `leader: true` key
/// gives it on load. The rows a leader lists are the leader's to place: they are not written, and a
/// note counts them. The unled bands' rows come first, then each led band's whole sequence, in `BandId`
/// order.
pub(super) fn export(
    world: &World,
    faction: FactionId,
    ids: &ExportIds,
    notes: &mut Vec<String>,
) -> Vec<LaborEntry> {
    // **Two phases, so no chief's managing pass runs before the other bands' own rows exist.** Any
    // command on a led band restaffs it and runs its chief's pass, which lists every source no band of
    // its people holds yet; a band whose row was written after that would be refused "already hunts".
    // So the bands without a leader come first, in `BandId` order, and each led band's whole sequence
    // (its orders, the releases, its own rows) follows, also in `BandId` order.
    let mut unled = Vec::new();
    let mut led = Vec::new();
    for band in ids.faction_bands.get(&faction).into_iter().flatten() {
        let Some(allocation) = world.get::<LaborAllocation>(band.entity) else {
            continue;
        };
        let is_led = world
            .get::<crate::leadership::BandLeadership>(band.entity)
            .is_some_and(|leadership| leadership.leader.is_some());
        let mut entries = Vec::new();
        let chief_rows = allocation
            .assignments
            .iter()
            .filter(|row| allocation.order_of_row(&row.target).is_some())
            .count();
        if chief_rows > 0 {
            notes.push(format!(
                "{}: the {chief_rows} row(s) its leader lists are not exported (the leader places \
                 them again)",
                band.scenario_id
            ));
        }
        // **The band's orders first**: a band with a leader holds an order per ordered kind, and a
        // scenario band is given the same orders when it loads, so each is stated (a `set`, and a
        // whole-kind `release` where the player crews the kind).
        for (kind, order) in &allocation.orders {
            entries.push(LaborEntry::BandOrder(BandOrderEntry {
                band: band.scenario_id.clone(),
                kind: kind.clone(),
                action: BAND_ORDER_SET.to_string(),
                target: Some(order.target),
                priority: Some(order.priority.as_str().to_string()),
                target_x: None,
                target_y: None,
                herd_id: None,
            }));
            if order.owner == OrderOwner::Player {
                entries.push(release_entry(band, kind, None, None));
            }
        }
        let mut priorities = Vec::new();
        for row in &allocation.assignments {
            if allocation.order_of_row(&row.target).is_some() {
                continue;
            }
            let Some(entry) = assign_entry(row, allocation, band, ids, notes) else {
                continue;
            };
            // **A player's row under a kind the leader owns is handed over first**, as the checked-in
            // fixtures do: the leader lists the sources he wants the moment he arrives.
            if let Some(kind) = row_kind(&row.target) {
                if allocation
                    .order_for(&kind)
                    .is_some_and(|order| order.owner == OrderOwner::Chief)
                {
                    entries.push(release_entry(
                        band,
                        &kind,
                        entry.target_x.zip(entry.target_y),
                        entry.fauna_id.clone(),
                    ));
                }
            }
            if row.priority != SourcePriority::Normal {
                match priority_entry(&row.target, row.priority, band, ids) {
                    Some(priority) => priorities.push(priority),
                    None => notes.push(format!(
                        "{}: a {} row is ranked {}, which a scenario cannot say",
                        band.scenario_id,
                        row.target.kind(),
                        row.priority.as_str()
                    )),
                }
            }
            entries.push(LaborEntry::AssignLabor(entry));
        }
        entries.extend(priorities.into_iter().map(LaborEntry::WorkPriority));
        if is_led {
            led.push(entries);
        } else {
            unled.push(entries);
        }
    }
    unled.into_iter().chain(led).flatten().collect()
}

/// The order kind a forage, hunt or extract row answers to: food, or the material dug.
fn row_kind(target: &LaborTarget) -> Option<String> {
    match target {
        LaborTarget::Forage { .. } | LaborTarget::Hunt { .. } => {
            Some(crate::keep_on_hand::FOOD_KIND.to_string())
        }
        LaborTarget::Extract { material, .. } => Some(material.clone()),
        _ => None,
    }
}

/// A `band_order release` entry, of a whole kind or of the one row (a tile or a herd).
fn release_entry(
    band: &ExportedBand,
    kind: &str,
    tile: Option<(u32, u32)>,
    herd: Option<String>,
) -> LaborEntry {
    LaborEntry::BandOrder(BandOrderEntry {
        band: band.scenario_id.clone(),
        kind: kind.to_string(),
        action: BAND_ORDER_RELEASE.to_string(),
        target: None,
        priority: None,
        target_x: tile.map(|(x, _)| x),
        target_y: tile.map(|(_, y)| y),
        herd_id: herd,
    })
}

fn assign_entry(
    row: &LaborAssignment,
    allocation: &LaborAllocation,
    band: &ExportedBand,
    ids: &ExportIds,
    notes: &mut Vec<String>,
) -> Option<AssignLaborEntry> {
    let mut entry = AssignLaborEntry {
        band: band.scenario_id.clone(),
        role: row.target.kind().to_string(),
        workers: if row.target.is_staffed() {
            row.max_workers.unwrap_or(NO_MAX_WORKERS)
        } else {
            row.workers
        },
        target_x: None,
        target_y: None,
        fauna_id: None,
        species: None,
        floor: None,
        kit_id: row.kit.as_ref().map(|kit| kit.id().to_string()),
        take_species: Vec::new(),
        move_with_herd: false,
    };
    match &row.target {
        LaborTarget::Forage {
            tile,
            floor,
            species,
            take_species,
        } => {
            entry.target_x = Some(tile.x);
            entry.target_y = Some(tile.y);
            entry.floor = Some(round_export(*floor));
            entry.species = species.clone();
            entry.take_species = take_species.keys().map(str::to_string).collect();
        }
        LaborTarget::Hunt {
            fauna_id,
            floor,
            move_with_herd,
        } => {
            let Some(herd) = ids.herd(fauna_id) else {
                notes.push(format!(
                    "{}: a hunt row on herd {fauna_id}, which is not exported, is left out",
                    band.scenario_id
                ));
                return None;
            };
            entry.fauna_id = Some(herd.to_string());
            entry.floor = Some(round_export(*floor));
            entry.move_with_herd = *move_with_herd;
        }
        LaborTarget::Scout => entry.workers = allocation.scouting_range_tiles(),
        LaborTarget::Warrior | LaborTarget::Roadwork => {}
        LaborTarget::Extract {
            tile,
            material,
            floor,
        } => {
            entry.target_x = Some(tile.x);
            entry.target_y = Some(tile.y);
            entry.species = Some(material.clone());
            entry.floor = Some(round_export(*floor));
        }
        LaborTarget::Tile { tile, branch } => {
            entry.target_x = Some(tile.x);
            entry.target_y = Some(tile.y);
            entry.species = Some(branch.as_str().to_string());
        }
        LaborTarget::Band { branch, .. } => {
            entry.species = Some(branch.as_str().to_string());
        }
    }
    Some(entry)
}

fn priority_entry(
    target: &LaborTarget,
    priority: SourcePriority,
    band: &ExportedBand,
    ids: &ExportIds,
) -> Option<WorkPriorityEntry> {
    let mut entry = WorkPriorityEntry {
        band: band.scenario_id.clone(),
        target_x: None,
        target_y: None,
        herd_id: None,
        material: None,
        branch: None,
        level: priority.as_str().to_string(),
    };
    match target {
        LaborTarget::Forage { tile, .. } => {
            entry.target_x = Some(tile.x);
            entry.target_y = Some(tile.y);
        }
        LaborTarget::Hunt { fauna_id, .. } => entry.herd_id = Some(ids.herd(fauna_id)?.to_string()),
        LaborTarget::Extract { tile, material, .. } => {
            entry.target_x = Some(tile.x);
            entry.target_y = Some(tile.y);
            entry.material = Some(material.clone());
        }
        LaborTarget::Tile { tile, branch } => {
            entry.target_x = Some(tile.x);
            entry.target_y = Some(tile.y);
            entry.branch = Some(branch.as_str().to_string());
        }
        LaborTarget::Band { branch, .. } => entry.branch = Some(branch.as_str().to_string()),
        LaborTarget::Scout | LaborTarget::Warrior | LaborTarget::Roadwork => return None,
    }
    Some(entry)
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
