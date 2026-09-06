//! **The opening outfitting window** — the one source of a faction's starting gear and material.
//!
//! A spawning band owns **nothing**: `equipment.json` ships `start_stock_fraction: 0.0`, and no
//! material declares a start stock (the mechanism that let one is deleted). Instead the player
//! composes an opening loadout on turn one, *after* seeing the generated map — a budget of one kit
//! per working-age hand spread across the kit roster, and a separate budget of material points
//! spread across the profile's pick list.
//!
//! **The window is open from world build until the first turn advance**, then it closes and anything
//! unspent is forfeited. Closing is a rule about the *turn*, not about the command: a
//! `SetStartingLoadout` arriving afterwards is rejected whole rather than partially honoured, and
//! nothing re-opens it.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{BandEquipment, BandId, PopulationCohort, StartingUnit},
    equipment_config::EquipmentConfigHandle,
    materials_config::MaterialsConfigHandle,
    orders::FactionId,
    recipes_config::RecipesConfigHandle,
    scalar::scalar_from_f32,
    start_profile::ActiveStartProfile,
};

/// **The reading every material picked in the opening loadout arrives at, on every axis that
/// material declares** — the middle of the range: what the band scavenged before setting out is
/// unremarkable.
///
/// One number rather than a per-material table, because a spread would be a claim nothing makes: the
/// player picks a *quantity* of a generic material, not a provenance. It is faithful to what the
/// retired per-material start stocks read (`stone` 0.5/0.5, `wood` 0.5/0.6).
pub const OPENING_MATERIAL_READING: f32 = 0.5;

/// **The opening outfitting window: open from world build until the first turn advance.**
///
/// Checkpoint state (`SimState`), because it is a fact about *this* world that nothing rebuilds — a
/// rollback into turn one must land back in a world whose window is still open, and a save must come
/// back with the budget it had.
///
/// [`Default`] is the **closed** window with no budget, which is what a world that never ran
/// worldgen (a load, a bare test `App`) correctly reads as: there is nothing to outfit.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StartingLoadout {
    pub open: bool,
    /// One per working-age hand of the starting band, stamped at spawn. **Derived, never
    /// configured** — see `start_profile::OpeningLoadoutConfig`.
    pub kit_budget: u32,
    /// `start_profiles.json` `opening_loadout.material_points`.
    pub material_budget: u32,
}

/// One line of the kit half of a loadout: `count` of `kit_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitAllocation {
    pub kit_id: String,
    pub count: u32,
}

/// One line of the material half: `units` of `material_id`, where one budget point buys one unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialAllocation {
    pub material_id: String,
    pub units: u32,
}

/// **Why a `SetStartingLoadout` was refused.** Every variant refuses the **whole** command: a
/// loadout is one composition against two budgets, so honouring the lines that happened to be legal
/// would spend the player's points on something they did not choose.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadoutRejection {
    #[error("the opening loadout window is closed - it shuts on the first turn advance")]
    WindowClosed,
    #[error("'{0}' is not a kit on the equipment roster")]
    UnknownKit(String),
    #[error("'{0}' carries no items, so allocating it would buy nothing")]
    KitBuysNothing(String),
    #[error("'{0}' is not one of this start profile's pickable materials")]
    UnpickableMaterial(String),
    #[error("{kits} kits allocated against a budget of {budget}")]
    OverKitBudget { kits: u32, budget: u32 },
    #[error("{units} material units allocated against a budget of {budget}")]
    OverMaterialBudget { units: u32, budget: u32 },
    #[error("'{0}' is allocated twice - one line per kit, one line per material")]
    DuplicateAllocation(String),
    #[error("this faction has no starting band to outfit")]
    NoStartingBand,
}

/// **The tick the world is BUILT on**, and the one turn that does not shut the window.
///
/// `SimulationTick` opens at `0`, and building a world runs exactly one `app.update()` — Startup
/// worldgen plus one pass of the turn schedule — which is what produces the baseline frame the
/// client first draws. The player has not advanced anything yet at that point, so shutting the
/// window there would close it before the map it is composed against had ever been seen. Every
/// later update is a turn the player asked for.
const WORLD_BUILD_TICK: u64 = 0;

/// **Close the window.** Registered before the turn's first stage, so a turn advance always finds it
/// shut — the budget is spent before the first turn resolves or it is not spent at all. Idempotent:
/// every later turn re-closes an already-closed window.
///
/// Skips [`WORLD_BUILD_TICK`], which is the pass that *opened* it.
pub fn close_opening_window(
    tick: Res<crate::resources::SimulationTick>,
    mut loadout: ResMut<StartingLoadout>,
) {
    if tick.0 == WORLD_BUILD_TICK {
        return;
    }
    if loadout.open {
        loadout.open = false;
        info!(
            target: "shadow_scale::campaign",
            "starting_loadout.window.closed=first_turn_advance"
        );
    }
}

/// **Stamp the window open with the budgets the world just built.** A Startup system, chained after
/// the spawn, because the kit budget is the *spawned band's* worker count rather than anything a
/// config states.
///
/// A world with no starting band leaves the window shut: there is nobody to outfit.
pub fn stamp_starting_loadout(
    mut loadout: ResMut<StartingLoadout>,
    profile: Option<Res<ActiveStartProfile>>,
    bands: Query<(&BandId, &PopulationCohort), With<StartingUnit>>,
    demographics: Option<Res<crate::demographics_config::DemographicsConfigHandle>>,
) {
    let Some(profile) = profile else {
        return;
    };
    // The lowest `BandId` carrying `StartingUnit`, which is the same band `apply_starting_loadout`
    // outfits — resolved by the same rule in both places so the budget cannot describe one band and
    // the gear land on another.
    let Some((_, cohort)) = bands.iter().min_by_key(|(id, _)| **id) else {
        return;
    };
    let working_fraction = demographics
        .map(|handle| handle.get().initial_distribution.working)
        .unwrap_or_else(|| {
            crate::demographics_config::DemographicsConfig::builtin()
                .initial_distribution
                .working
        });
    let workers = crate::systems::party_workers(cohort.size, working_fraction);
    *loadout = StartingLoadout {
        open: true,
        kit_budget: workers as u32,
        material_budget: profile
            .profile()
            .overrides()
            .opening_loadout
            .material_points,
    };
    info!(
        target: "shadow_scale::campaign",
        kit_budget = loadout.kit_budget,
        material_budget = loadout.material_budget,
        "starting_loadout.window.opened"
    );
}

/// **Apply a composed opening loadout to the faction's starting band, then shut the window.**
///
/// Every check runs before anything is written, so a refusal leaves the world byte-identical.
pub fn apply_starting_loadout(
    world: &mut World,
    faction: FactionId,
    kits: &[KitAllocation],
    materials: &[MaterialAllocation],
) -> Result<(), LoadoutRejection> {
    let window = *world.resource::<StartingLoadout>();
    if !window.open {
        return Err(LoadoutRejection::WindowClosed);
    }

    let equipment = world.resource::<EquipmentConfigHandle>().get();
    let recipes = world.resource::<RecipesConfigHandle>().get();
    let materials_table = world.resource::<MaterialsConfigHandle>().get();
    let pickable = world
        .resource::<ActiveStartProfile>()
        .profile()
        .overrides()
        .opening_loadout
        .pickable_materials
        .clone();

    let mut named = std::collections::BTreeSet::new();
    let mut kit_total: u32 = 0;
    for allocation in kits {
        if !named.insert(allocation.kit_id.as_str()) {
            return Err(LoadoutRejection::DuplicateAllocation(
                allocation.kit_id.clone(),
            ));
        }
        let Some(definition) = equipment.kit_definition(&allocation.kit_id) else {
            return Err(LoadoutRejection::UnknownKit(allocation.kit_id.clone()));
        };
        // **The `none` kit, refused by what makes it `none`.** A kit that puts nothing in anybody's
        // hands cannot be bought, and testing `uses` rather than the id keeps the rule true of any
        // future empty roster entry without minting a magic string.
        if definition.uses.is_empty() {
            return Err(LoadoutRejection::KitBuysNothing(allocation.kit_id.clone()));
        }
        kit_total = kit_total.saturating_add(allocation.count);
    }
    if kit_total > window.kit_budget {
        return Err(LoadoutRejection::OverKitBudget {
            kits: kit_total,
            budget: window.kit_budget,
        });
    }

    let mut named = std::collections::BTreeSet::new();
    let mut material_total: u32 = 0;
    for allocation in materials {
        if !named.insert(allocation.material_id.as_str()) {
            return Err(LoadoutRejection::DuplicateAllocation(
                allocation.material_id.clone(),
            ));
        }
        if !pickable.iter().any(|id| id == &allocation.material_id) {
            return Err(LoadoutRejection::UnpickableMaterial(
                allocation.material_id.clone(),
            ));
        }
        material_total = material_total.saturating_add(allocation.units);
    }
    if material_total > window.material_budget {
        return Err(LoadoutRejection::OverMaterialBudget {
            units: material_total,
            budget: window.material_budget,
        });
    }

    let Some(band) = starting_band(world, faction) else {
        return Err(LoadoutRejection::NoStartingBand);
    };

    // --- nothing above this line writes; nothing below it can fail -------------------------------

    // **Two kits that share an item ADD.** 3 `big_game` + 3 `trapping` is 3 spears, 3 traps and
    // **6** sleds: an allocation buys a kit's worth of gear per hand, and two hands carrying a sled
    // are two sleds however they were bought.
    let mut ledger = world
        .get_mut::<BandEquipment>(band)
        .map(|held| held.clone())
        .unwrap_or_default();
    for allocation in kits {
        let Some(definition) = equipment.kit_definition(&allocation.kit_id) else {
            continue;
        };
        for item_id in &definition.uses {
            let Some(item) = equipment.item(item_id) else {
                continue;
            };
            ledger.stock(
                item_id,
                allocation.count,
                &item.default_tier().id,
                BandEquipment::anchor_grade(&recipes, &materials_table, item_id),
            );
        }
    }
    world.entity_mut(band).insert(ledger);

    if let Some(mut cohort) = world.get_mut::<PopulationCohort>(band) {
        for allocation in materials {
            let Some(def) = materials_table.material(&allocation.material_id) else {
                continue;
            };
            let readings: BTreeMap<String, f32> = def
                .characteristics
                .iter()
                .map(|axis| (axis.clone(), OPENING_MATERIAL_READING))
                .collect();
            // The merge key comes from the table's own lookup, exactly as a yield edge's does — the
            // store stores, it does not interpret.
            let Some(key) = materials_table.band_key(&allocation.material_id, &readings) else {
                continue;
            };
            cohort.stores.deposit_material(
                &allocation.material_id,
                key,
                scalar_from_f32(allocation.units as f32),
                &readings,
            );
        }
    }

    world.resource_mut::<StartingLoadout>().open = false;
    info!(
        target: "shadow_scale::campaign",
        faction = faction.0,
        kits = kit_total,
        material_units = material_total,
        "starting_loadout.applied"
    );
    Ok(())
}

/// **The band an opening loadout outfits: the lowest `BandId` carrying `StartingUnit` for this
/// faction.** The shipped profile spawns exactly one; a profile that spawns several outfits the
/// first and says so, rather than splitting a budget the player composed as one.
fn starting_band(world: &mut World, faction: FactionId) -> Option<Entity> {
    let mut query =
        world.query_filtered::<(Entity, &BandId, &PopulationCohort), With<StartingUnit>>();
    let mut candidates: Vec<(BandId, Entity)> = query
        .iter(world)
        .filter(|(_, _, cohort)| cohort.faction == faction)
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    candidates.sort_unstable_by_key(|(id, _)| *id);
    if candidates.len() > 1 {
        warn!(
            target: "shadow_scale::campaign",
            faction = faction.0,
            bands = candidates.len(),
            "starting_loadout.multiple_starting_bands=outfitting the lowest BandId"
        );
    }
    candidates.first().map(|(_, entity)| *entity)
}
