//! **THE COMPOSE SHEET'S DRY RUN OF THE REAL STAFFING** — *"if this band added (or re-stated) this
//! row at this Priority / Max / floor / kit, what crew would the staffing give it, and which rows
//! lose hands?"*
//!
//! Pressing "Hunt here" restaffs the whole band by Priority (`staffing::staff_rows_settled`), so a
//! new row can take hands from lower-priority rows; a sheet that previewed the crew from the idle
//! hands alone would read `0` where the command gives a crew. This answers exactly: it builds the
//! row the way `assign_labor` does (the helpers below are the command's own, shared), writes it
//! onto a COPY of the band's allocation, and staffs that copy through
//! [`crate::staffing::staff_allocation_copy`] — the staffing `restaff_band` runs. The unmodified
//! allocation is staffed the same way for the "before" figures, so a loss compares like with like.
//!
//! Nothing is written to the world. See `.claude/rules/core_sim/work-party.md` → "The staffing
//! preview".

use bevy::math::UVec2;
use bevy::prelude::{Entity, World};
use sim_runtime::commands::{
    query_error, QueryReply, StaffingPreviewLoss, StaffingPreviewQuery, StaffingPreviewReply,
    WorkPartySource,
};

use crate::components::{
    floor_is_valid, BandBench, BandId, BandWorkforce, BuildSource, LaborAllocation, LaborTarget,
    PopulationCohort, SourcePriority, TakeSelection, Tile,
};
use crate::equipment_config::{EquipmentConfig, KitChoice, KitSelectionError};
use crate::extraction_config::ExtractionConfigHandle;
use crate::fauna::HerdRegistry;
use crate::forage::ForageRegistry;
use crate::intensification::LadderConfigHandle;
use crate::intensification::NO_CREW_ON_THIS_ACTIVITY;
use crate::orders::FactionId;
use crate::resources::TileRegistry;
use crate::EquipmentConfigHandle;

/// **THE WORKING ON `(tile, material)` AS IT STANDS NOW** — the registry's live working, or, where no
/// band has opened one, the derived opening state (`snapshot::deposits`' own rule). `None` for ground
/// holding none of the material or a tile off the map. The command path and the preview both read it.
pub fn working_as_it_stands(
    world: &World,
    tile: UVec2,
    material: &str,
) -> Option<crate::extraction::DepositSource> {
    if let Some(live) = world
        .resource::<crate::extraction::DepositRegistry>()
        .source(tile, material)
    {
        return Some(live.clone());
    }
    let entity = world.resource::<TileRegistry>().index(tile.x, tile.y)?;
    let ground = world.get::<Tile>(entity)?;
    let extraction = world.resource::<ExtractionConfigHandle>().get();
    let capacity = crate::extraction::tile_deposit_capacity(&extraction, material, ground);
    if capacity <= crate::NO_DEPOSIT {
        return None;
    }
    let branch = crate::extraction::deposit_branch(&extraction, material)?;
    let ladder = world.resource::<LadderConfigHandle>().get();
    Some(crate::extraction::DepositSource::opening(
        tile,
        material,
        capacity,
        crate::RungStanding::unstarted_on(&ladder, branch),
    ))
}

/// **A SEAM THAT NEVER RENEWS HAS NO REGROWTH TO STAFF AGAINST, SO IT NEEDS THE PLAYER'S COUNT.**
/// The sim sizes a staffed extract row's crew from what the deposit regrows; stone on a rock body
/// never does, so its crew is the Max in heads and a row with no Max would staff nobody forever.
/// Refused by name at `assign_labor` and at the preview. `None` for every other row, and for a count
/// of one or more.
pub fn never_renewing_dig_without_a_count(
    world: &World,
    target: &LaborTarget,
    workers: u32,
) -> Option<String> {
    let LaborTarget::Extract { tile, material, .. } = target else {
        return None;
    };
    if workers > NO_CREW_ON_THIS_ACTIVITY {
        return None;
    }
    let extraction = world.resource::<ExtractionConfigHandle>().get();
    let entity = world.resource::<TileRegistry>().index(tile.x, tile.y)?;
    let ground = world.get::<Tile>(entity)?;
    (crate::extraction::tile_deposit_regrowth(&extraction, material, ground) <= crate::NEVER_RENEWS)
        .then(|| {
            format!(
                "A {material} dig needs a digger count - this seam never renews, so the count you \
                 give is how many people dig it."
            )
        })
}

/// **What a row runs on when the player names no kit** — the herd's own default for a Hunt on a
/// resolvable quarry, the working's branch default for an Extract, the job's default for everything
/// else. The same id the wire publishes (`HerdTelemetryState::default_kit_id`,
/// `DepositState.defaultKitId`), so `assign_labor`, both raiding verbs and the preview cannot
/// disagree about what "no kit named" means.
pub fn default_kit_for_target(
    world: &World,
    equipment: &EquipmentConfig,
    target: &LaborTarget,
) -> KitChoice {
    let job = target.kit_job();
    if let LaborTarget::Extract { tile, material, .. } = target {
        return working_as_it_stands(world, *tile, material).map_or_else(
            || equipment.default_kit(job),
            |working| crate::extraction::working_default_kit(equipment, &working),
        );
    }
    let LaborTarget::Hunt { fauna_id, .. } = target else {
        return equipment.default_kit(job);
    };
    let fauna = world.resource::<crate::FaunaConfigHandle>().get();
    let resolved = world
        .resource::<HerdRegistry>()
        .find(fauna_id)
        .and_then(|herd| {
            fauna
                .species_by_display(&herd.species)
                .map(|species| (species, herd.is_corralled()))
        });
    let Some((species, corralled)) = resolved else {
        return equipment.default_kit(job);
    };
    crate::herd_default_hunt_kit(
        equipment,
        world
            .resource::<crate::CreaturesConfigHandle>()
            .get()
            .person(),
        species,
        corralled,
    )
}

fn refusal(token: &str) -> QueryReply {
    QueryReply::Error(token.to_string())
}

/// The labor row the ask names, built as `assign_labor` builds it.
fn target_of(world: &World, ask: &StaffingPreviewQuery) -> Result<LaborTarget, QueryReply> {
    match &ask.source {
        WorkPartySource::Hunt { herd_id } => {
            if world
                .get_resource::<HerdRegistry>()
                .and_then(|herds| herds.find(herd_id))
                .is_none()
            {
                return Err(refusal(query_error::UNKNOWN_HERD));
            }
            Ok(LaborTarget::Hunt {
                fauna_id: herd_id.clone(),
                floor: ask.floor,
                move_with_herd: false,
            })
        }
        WorkPartySource::Forage { x, y, take_species } => {
            let tile = UVec2::new(*x, *y);
            if world
                .get_resource::<ForageRegistry>()
                .and_then(|patches| patches.patch(tile))
                .is_none()
            {
                return Err(refusal(query_error::UNKNOWN_PATCH));
            }
            Ok(LaborTarget::Forage {
                tile,
                floor: ask.floor,
                species: None,
                take_species: TakeSelection::from_keys(take_species),
            })
        }
        WorkPartySource::Extract { x, y, material } => {
            let tile = UVec2::new(*x, *y);
            if working_as_it_stands(world, tile, material).is_none() {
                return Err(refusal(query_error::UNKNOWN_DEPOSIT));
            }
            Ok(LaborTarget::Extract {
                tile,
                material: material.clone(),
                floor: ask.floor,
            })
        }
    }
}

/// The identity fields of a labor target, as the loss row states them.
fn loss_of_target(target: &LaborTarget, before: u32, after: u32) -> StaffingPreviewLoss {
    let mut loss = StaffingPreviewLoss {
        kind: target.kind().to_string(),
        before,
        after,
        ..StaffingPreviewLoss::default()
    };
    match target {
        LaborTarget::Forage { tile, .. } => (loss.x, loss.y) = (tile.x, tile.y),
        LaborTarget::Hunt { fauna_id, .. } => loss.herd_id = fauna_id.clone(),
        LaborTarget::Extract { tile, material, .. } => {
            (loss.x, loss.y) = (tile.x, tile.y);
            loss.material = material.clone();
        }
        LaborTarget::Tile { tile, branch } => {
            (loss.x, loss.y) = (tile.x, tile.y);
            loss.branch = branch.to_string();
        }
        LaborTarget::Band { branch, .. } => loss.branch = branch.to_string(),
        LaborTarget::Scout | LaborTarget::Warrior | LaborTarget::Roadwork => {}
    }
    loss
}

/// The identity fields of a declared build's source, as the loss row states them.
fn loss_of_build(source: &BuildSource, before: u32, after: u32) -> StaffingPreviewLoss {
    let mut loss = StaffingPreviewLoss {
        kind: format!("build:{}", source.kind()),
        before,
        after,
        ..StaffingPreviewLoss::default()
    };
    match source {
        BuildSource::Patch(tile) | BuildSource::Road(tile) => (loss.x, loss.y) = (tile.x, tile.y),
        BuildSource::Herd(id) => loss.herd_id = id.clone(),
        BuildSource::Deposit { tile, material } => {
            (loss.x, loss.y) = (tile.x, tile.y);
            loss.material = material.clone();
        }
        BuildSource::Tile { tile, branch } => {
            (loss.x, loss.y) = (tile.x, tile.y);
            loss.branch = branch.to_string();
        }
        BuildSource::Band { branch, .. } => loss.branch = branch.to_string(),
    }
    loss
}

fn find_band(world: &mut World, faction: FactionId, band_id: u64) -> Option<Entity> {
    let wanted = BandId(band_id);
    world
        .query::<(Entity, &BandId, &PopulationCohort)>()
        .iter(world)
        .find(|(_, id, cohort)| **id == wanted && cohort.faction == faction)
        .map(|(entity, _, _)| entity)
}

fn idle_hands(world: &World, band: Entity, allocation: &LaborAllocation) -> u32 {
    BandWorkforce::resolve(
        world.get::<PopulationCohort>(band),
        Some(allocation),
        world.get::<BandBench>(band),
    )
    .idle()
}

/// **THE STAFFING DRY RUN**: refuse with a [`query_error`] token or answer with the previewed row's
/// crew and the rows that lose hands to it. Reads the world; writes nothing.
pub fn answer_staffing_preview(world: &mut World, ask: &StaffingPreviewQuery) -> QueryReply {
    if !floor_is_valid(ask.floor) {
        return refusal(query_error::INVALID_FLOOR);
    }
    let priority = if ask.priority.trim().is_empty() {
        SourcePriority::default()
    } else {
        match SourcePriority::from_token(&ask.priority.trim().to_ascii_lowercase()) {
            Some(priority) => priority,
            None => return refusal(query_error::INVALID_PRIORITY),
        }
    };
    let Some(band) = find_band(world, FactionId(ask.faction_id), ask.band_id) else {
        return refusal(query_error::UNKNOWN_BAND);
    };
    let world: &World = world;
    let target = match target_of(world, ask) {
        Ok(target) => target,
        Err(reply) => return reply,
    };
    if let LaborTarget::Extract { .. } = &target {
        // Only a row the chief LISTS is his; a stand the player crews (or has none on) is not.
        if world
            .get::<LaborAllocation>(band)
            .is_some_and(|allocation| {
                allocation
                    .row_order(&target)
                    .is_some_and(|kind| allocation.chief_owns(kind))
            })
        {
            return refusal(query_error::MATERIAL_IS_KEPT_ON_HAND);
        }
    }
    if never_renewing_dig_without_a_count(world, &target, ask.max_workers).is_some() {
        return refusal(query_error::NEVER_RENEWING_DIG_WITHOUT_A_COUNT);
    }
    let equipment = world.resource::<EquipmentConfigHandle>().get();
    let absent = default_kit_for_target(world, &equipment, &target);
    let named = (!ask.kit_id.is_empty()).then_some(ask.kit_id.as_str());
    let kit = match equipment.resolve_kit_or(named, target.kit_job(), absent) {
        Ok(kit) => kit,
        Err(KitSelectionError::Unknown { .. }) => return refusal(query_error::UNKNOWN_KIT),
        Err(KitSelectionError::WrongJob { .. }) => return refusal(query_error::KIT_WRONG_JOB),
    };

    let held = world
        .get::<LaborAllocation>(band)
        .cloned()
        .unwrap_or_default();
    let mut proposed = held.clone();
    proposed.set_staffed_row(
        target.clone(),
        (ask.max_workers > NO_CREW_ON_THIS_ACTIVITY).then_some(ask.max_workers),
        Some(kit),
    );
    proposed.set_source_priority(&target, priority);

    let (Some(before), Some(after)) = (
        crate::staffing::staff_allocation_copy(world, band, held),
        crate::staffing::staff_allocation_copy(world, band, proposed),
    ) else {
        return refusal(query_error::UNKNOWN_BAND);
    };

    let Some(row) = after
        .assignments
        .iter()
        .find(|row| row.target.same_source(&target))
    else {
        return refusal(query_error::UNKNOWN_BAND);
    };

    // **Rows whose crew falls**, High first (the order a scarce hand is served in), then the
    // allocation's own row order; declared builds follow.
    let crew_before = |other: &LaborTarget| {
        before
            .assignments
            .iter()
            .find(|candidate| candidate.target.same_source(other))
            .map_or(0, |candidate| candidate.workers)
    };
    let mut falling: Vec<(SourcePriority, StaffingPreviewLoss)> = after
        .assignments
        .iter()
        .filter(|candidate| !candidate.target.same_source(&target))
        .filter_map(|candidate| {
            let was = crew_before(&candidate.target);
            (candidate.workers < was).then(|| {
                (
                    candidate.priority,
                    loss_of_target(&candidate.target, was, candidate.workers),
                )
            })
        })
        .collect();
    falling.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));
    let mut losses: Vec<StaffingPreviewLoss> = falling.into_iter().map(|(_, loss)| loss).collect();
    for (source, crew) in &after.build_crews {
        let was = before
            .build_crews
            .iter()
            .find(|(candidate, _)| candidate == source)
            .map_or(0, |(_, crew)| crew.total());
        if crew.total() < was {
            losses.push(loss_of_build(source, was, crew.total()));
        }
    }

    QueryReply::StaffingPreview(StaffingPreviewReply {
        workers: row.workers,
        keepers: row.staffing.keepers,
        kitted: row.staffing.kitted,
        bare: row.staffing.bare,
        idle_before: idle_hands(world, band, &before),
        idle_after: idle_hands(world, band, &after),
        losses,
    })
}
