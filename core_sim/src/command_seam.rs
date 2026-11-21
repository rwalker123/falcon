//! **The seam the replayed verbs run through** (`docs/plan_scenarios.md` section 2.9).
//!
//! A scenario replays the game's own commands, and a replayed command that the handler refuses has
//! to stop the load with the handler's own reason. The server's handlers used to report a refusal
//! only on the event feed, so the verbs a scenario replays live here as functions that **return** the
//! refusal. The server's handler converts the wire, calls the function and, on `Err`, still writes
//! the feed event - so the two callers cannot differ, because there is one body.
//!
//! A later slice adds a verb as another function (and another [`CommandRefusal`] variant) beside
//! [`apply_set_starting_loadout`].

use bevy::prelude::*;

use crate::components::{BandId, PopulationCohort};
pub use crate::labor_commands::{apply_assign_labor, apply_work_priority, AssignLaborOrder};
use crate::orders::FactionId;
use crate::starting_loadout::{
    apply_starting_loadout, KitAllocation, LoadoutRejection, LoadoutSupply, MaterialAllocation,
    StartingLoadout,
};

/// **Why a replayed verb was refused.** `Display` is the reason text the event feed already shows
/// the player.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandRefusal {
    /// `SetStartingLoadout` was refused by the outfitting window's own checks.
    #[error("{0}")]
    StartingLoadout(#[from] LoadoutRejection),
    /// A labor verb (`assign_labor`, `work_priority`) was refused. `kind` is the feed channel the
    /// player's own refusal lands on and `reason` the text it shows; the server writes exactly that
    /// line, the scenario loader reports exactly that reason.
    #[error("{reason}")]
    Refused {
        kind: crate::CommandEventKind,
        reason: String,
    },
}

/// **Apply a composed opening loadout to one band, restaffing the crews it moved.**
///
/// Everything `set_starting_loadout` does to the world: the validation and the deposit
/// ([`apply_starting_loadout`]), then, because the outfit moved gear, a restaff and re-quote of the
/// outfitted band and (on a take) its parent
/// ([`crate::yield_seed::restaff_and_reseed`]).
pub fn apply_set_starting_loadout(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    kits: &[KitAllocation],
    materials: &[MaterialAllocation],
) -> Result<(), CommandRefusal> {
    apply_starting_loadout(world, faction, band, kits, materials)?;
    // **The outfit moved gear, so the crews are stale until restaffed**: the outfitted band's own
    // kit changed and, on a take, so did the parent's.
    let parent = world
        .resource::<StartingLoadout>()
        .window(band)
        .and_then(|window| match &window.supply {
            LoadoutSupply::Parent { parent, .. } => Some(*parent),
            LoadoutSupply::Grant { .. } => None,
        });
    let mut query = world.query::<(Entity, &BandId, &PopulationCohort)>();
    let changed: Vec<Entity> = query
        .iter(world)
        .filter(|(_, id, cohort)| {
            cohort.faction == faction && (**id == band || Some(**id) == parent)
        })
        .map(|(entity, _, _)| entity)
        .collect();
    for entity in changed {
        crate::yield_seed::restaff_and_reseed(world, entity);
    }
    Ok(())
}
