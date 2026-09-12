//! **What a crew can be put to work on** — the source vocabulary `Food` ranks and `Land` shares:
//! a forage patch or a huntable herd, its expected take for a crew, and the two eligibility
//! accessors both specialists must ask the same way.

use sim_runtime::{ForagePatchState, KitOptionState, LaborAssignmentState, PopulationCohortState};

use super::{Food, ROLE_FORAGE, ROLE_HUNT};
use crate::geometry::Tile;
use crate::view::{row_key, SeatMemory, SeatView};

/// **What one worker would take off `patch` this turn** — the rate the band has realized on that
/// ground, else the web's prior, else the frame's forecast ([`Food::rate`]).
///
/// ⛔ **This, not `carrying_capacity`, is what ground is worth to a crew.** The capacity is the
/// stand's standing biomass `K`; the two disagree by more than 2× on the shipped bench seeds, and in
/// the wrong direction — the richest stand in a neighbourhood can be the worst ground to work. Both
/// `Food` (ranking sources) and `Land` (ranking ground to move onto) read this one quantity, so the
/// two specialists cannot disagree about what a tile pays.
pub(crate) fn patch_per_worker_yield(
    memory: &SeatMemory,
    band: &PopulationCohortState,
    patch: &ForagePatchState,
) -> f32 {
    Food::rate(
        memory,
        band,
        &SourceKey::Patch(Tile::new(patch.x, patch.y)),
        patch.per_worker_yield,
    )
}

/// **A gathering site**: the plant rung's `site_requirement` (`plant_rung_site_refusal`,
/// `core_sim/src/bin/server.rs`) is the tile carrying a food module, and a patch row is published
/// for ground that carries none — a crew sent there is refused *"nobody gathers here"*.
///
/// ⛔ **`Food` AND `Land` MUST ASK THIS THE SAME WAY.** `Food` filters its sources on it, so ground
/// off a food module is ground the band cannot work; `Land` ranking a move on a patch that fails it
/// walks the band onto ground it will then be refused every assignment on, and counts the same patch
/// as *"something better in view"* so the `land_short` alarm never fires. One accessor, both
/// callers — the divergence [`patch_per_worker_yield`] already closed for the rate.
pub(crate) fn is_food_site(view: &SeatView, tile: Tile) -> bool {
    view.snapshot
        .food_modules
        .iter()
        .any(|site| site.x == tile.x && site.y == tile.y)
}

/// **The patch on `tile` a crew could actually be put to work on**: the published forage row, but
/// only where [`is_food_site`] holds. Bare ground and ground off a food module read the same way —
/// as nothing — because a crew takes nothing off either.
pub(crate) fn workable_patch_at(view: &SeatView, tile: Tile) -> Option<&ForagePatchState> {
    view.patch_at(tile).filter(|_| is_food_site(view, tile))
}

/// **The hunting gear `band` holds, in units** — the `count` summed over its `equipment_batches`
/// rows whose `item_id` is carried by a kit whose `jobs` include `hunt`. The join is
/// `EquipmentBatchState::item_id` ↔ `KitOptionState::item_ids` (the kit's `equipment.json` `uses`
/// list); a batch row with `count 0` is *"the band owns none of this item at all"*, and the `none`
/// kit carries no items, so a bare band reads `0`.
///
/// ⛔ **NO KIT, NO HUNT.** `equipment.json`: *"A SPAWNING BAND OWNS NO EQUIPMENT AT ALL … HUNTING
/// YIELDS NOTHING AT ANY CREW SIZE until a spear is crafted"*, and an AI seat never outfits (the
/// opening window closes with nothing applied, `starting_loadout::close_opening_window`). So a
/// herd is not a source for a band that reads `0` here, whatever the herd is forecast to pay: the
/// twelve hands sent to it were rejected next turn as *no useful crew*, every turn, for the first
/// ten-odd turns of every bench seed.
pub(crate) fn hunting_kits_held(view: &SeatView, band: &PopulationCohortState) -> u32 {
    let hunt_kits: Vec<&KitOptionState> = view
        .snapshot
        .kits
        .iter()
        .filter(|kit| kit.jobs.iter().any(|job| job == ROLE_HUNT))
        .collect();
    band.equipment_batches
        .iter()
        .filter(|batch| {
            hunt_kits
                .iter()
                .any(|kit| kit.item_ids.contains(&batch.item_id))
        })
        .map(|batch| batch.count)
        .sum()
}

/// **What a crew of `hands` takes off a source in one turn**: `min(hands × rate, ceiling)`, the
/// ceiling being the take at a zero escapement floor (`biomass × provisions_per_biomass`; see the
/// module docs). A per-turn quantity on both sides, so it may be compared with a per-turn demand.
pub(crate) fn crew_take(hands: u32, per_worker_yield: f32, ceiling: f32) -> f32 {
    (hands as f32 * per_worker_yield).min(ceiling)
}

/// A source a band can be assigned to.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceKey {
    Patch(Tile),
    Herd(String),
}

impl SourceKey {
    /// The intent subject: `x,y` for a patch, the herd id for a herd.
    pub(super) fn subject(&self) -> String {
        match self {
            SourceKey::Patch(tile) => format!("{},{}", tile.x, tile.y),
            SourceKey::Herd(id) => id.clone(),
        }
    }

    /// The source as the decision log names it: `forage 5,4` / `hunt herd_9`.
    pub(super) fn describe(&self) -> String {
        match self {
            SourceKey::Patch(_) => format!("{ROLE_FORAGE} {}", self.subject()),
            SourceKey::Herd(_) => format!("{ROLE_HUNT} {}", self.subject()),
        }
    }

    /// The `assign_labor` role — the web — this source is worked under.
    pub(super) fn role(&self) -> &'static str {
        match self {
            SourceKey::Patch(_) => ROLE_FORAGE,
            SourceKey::Herd(_) => ROLE_HUNT,
        }
    }

    /// The memory key of this source's row on `band` ([`crate::view::row_key`]).
    pub(super) fn row_key(&self, band_id: u64) -> String {
        match self {
            SourceKey::Patch(tile) => row_key(band_id, ROLE_FORAGE, tile.x, tile.y, ""),
            SourceKey::Herd(id) => row_key(band_id, ROLE_HUNT, 0, 0, id),
        }
    }

    pub(super) fn of_row(row: &LaborAssignmentState) -> Option<Self> {
        match row.kind.as_str() {
            ROLE_FORAGE => Some(SourceKey::Patch(Tile::new(row.target_x, row.target_y))),
            ROLE_HUNT => Some(SourceKey::Herd(row.fauna_id.clone())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Source {
    pub key: SourceKey,
    /// Where the source stands — the patch's tile, or the herd's this frame.
    pub tile: Tile,
    /// The rate a crew is ranked on: what this band has **realized** on that source, if it has
    /// worked it, else the web's prior, else the frame's forecast ([`Food::rate`]).
    pub per_worker_yield: f32,
    /// The take at a zero floor: `biomass × provisions_per_biomass`.
    pub ceiling: f32,
}

impl Source {
    /// What a crew of `hands` takes: `min(hands × rate, ceiling)`.
    pub fn expected(&self, hands: u32) -> f32 {
        crew_take(hands, self.per_worker_yield, self.ceiling)
    }

    /// What `more` hands add on top of `existing` already on this source — the marginal take,
    /// which is what a reassignment onto it gains.
    pub fn marginal(&self, existing: u32, more: u32) -> f32 {
        self.expected(existing + more) - self.expected(existing)
    }

    /// The reach a band works this source from: `work_range` for a patch, `hunt_reach` for a herd.
    pub fn reach(&self, band: &PopulationCohortState) -> u32 {
        match self.key {
            SourceKey::Patch(_) => band.work_range,
            SourceKey::Herd(_) => band.hunt_reach,
        }
    }
}
