//! **Auto-craft — the bench keeps its own queue filled from the ranked suggestions** (#779,
//! `.claude/rules/core_sim/crafting.md` → "Auto-craft").
//!
//! An opt-in per-band switch ([`BandBench::auto`]). While it is on and the queue is **empty**, the
//! bench queues the first usable entry of the band's craft-suggestion list at its whole netted
//! count, tagged [`BenchOrder::auto`](crate::components::BenchOrder::auto). Nothing else is ever
//! decided here:
//!
//! - **It fills an EMPTY queue only.** A player-queued order sitting there means auto adds nothing,
//!   so overriding is just queueing.
//! - **It never walks down the list on its own.** An auto order short of materials simply waits
//!   (the ordinary skip/waiting semantics); only the player's skip ([`auto_craft_skip`]) moves on.
//! - **It never picks the crew.** [`BandBench::workers`] is untouched.
//! - **A skipped item is passed over until one of it can be drawn from stock**, and returns only
//!   when the queue is next empty — it never preempts a running order.
//!
//! The suggestions are the snapshot's own: [`take_row_gear`] settles the gear and
//! [`band_suggestions`] ranks it, the same two functions `snapshot::population` calls, and the
//! recipe an item is queued with is the offer the crafting ledger marks `suggested` for it — the
//! recipe the panel's Queue button sends.

use bevy::prelude::*;

use crate::{
    components::{
        BandBench, BandEquipment, BenchOrder, Expedition, LaborAllocation, PopulationCohort,
        HEAD_ORDER, MIN_ORDER_COUNT,
    },
    craft_suggestions::{band_suggestions, take_row_gear, CraftSuggestion},
    equipment_config::{EquipmentConfig, EquipmentConfigHandle},
    intensification::LadderConfigHandle,
    materials_config::{MaterialsConfig, MaterialsConfigHandle},
    recipes_config::{RecipesConfig, RecipesConfigHandle},
    resources::DiscoveryProgressLedger,
    snapshot::crafting::{
        band_craft_state, known_crafts, plan_craft_offers, BandCraftInputs, EquippedElsewhere,
    },
    systems::{order_is_workable, worked_order},
    take_claims::{row_claims, with_world_sources},
};
use sim_schema::CraftOfferState;

/// **WHY A SKIP WAS REFUSED** — the reasons [`auto_craft_skip`] gives, so the command handler can
/// name the cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoSkipRefusal {
    /// The band has no bench, or the world lacks the configs a skip reads.
    NoBench,
    /// The queue is empty — nothing is being waited on.
    NothingQueued,
    /// The bench has an order it can work, so it is not waiting on anything.
    BenchIsWorking,
    /// The head order is the player's, not auto-craft's.
    HeadNotAuto,
}

/// The configs one band's fill reads, resolved once.
struct FillConfigs {
    recipes: std::sync::Arc<RecipesConfig>,
    materials: std::sync::Arc<MaterialsConfig>,
    equipment: std::sync::Arc<EquipmentConfig>,
}

impl FillConfigs {
    fn resolve(world: &World) -> Option<Self> {
        Some(Self {
            recipes: world.get_resource::<RecipesConfigHandle>()?.get(),
            materials: world.get_resource::<MaterialsConfigHandle>()?.get(),
            equipment: world.get_resource::<EquipmentConfigHandle>()?.get(),
        })
    }
}

/// **THE RECIPE AN ITEM WOULD BE QUEUED WITH** — the offer the crafting ledger marks `suggested` on
/// that item's row. The panel's Queue button queues exactly this one.
fn suggested_offer<'a>(offers: &'a [CraftOfferState], item: &str) -> Option<&'a CraftOfferState> {
    offers
        .iter()
        .find(|offer| offer.suggested && offer.output_item_id == item)
}

/// **THE PURE PICK** — given the band's suggestions and offers, drop from `auto_skipped` every item
/// that can now be drawn, then queue the first suggestion that is not skipped and whose craft is
/// known. Returns whether an order was queued.
///
/// Only an empty queue on an auto bench is filled. Never touches the crew.
#[allow(clippy::too_many_arguments)]
fn fill_empty_queue(
    bench: &mut BandBench,
    suggestions: &[CraftSuggestion],
    offers: &[CraftOfferState],
    store: &crate::components::LocalStore,
    wear: &BandEquipment,
    configs: &FillConfigs,
) -> bool {
    if !bench.auto || !bench.orders.is_empty() {
        return false;
    }
    // **Un-skip first**: an item whose recipe can draw ONE item from the stock on hand is eligible
    // again. It re-enters at its rank in the list, not at the head of anything.
    let drawable = |item: &str| {
        suggested_offer(offers, item).is_some_and(|offer| {
            order_is_workable(
                &BenchOrder::new(&offer.recipe_id, MIN_ORDER_COUNT),
                store,
                &configs.recipes,
                &configs.materials,
                &configs.equipment,
                wear,
            )
        })
    };
    // An item nobody is going without any more is not suggested, so there is nothing to skip: a
    // stale entry must not survive.
    bench.auto_skipped.retain(|item| {
        suggestions
            .iter()
            .any(|suggestion| &suggestion.item == item)
            && !drawable(item)
    });
    let pick = suggestions.iter().find_map(|suggestion| {
        if bench.auto_skipped.contains(&suggestion.item) {
            return None;
        }
        let offer = suggested_offer(offers, &suggestion.item)?;
        offer
            .queueable
            .then(|| (offer.recipe_id.clone(), suggestion.count))
    });
    let Some((recipe_id, count)) = pick else {
        return false;
    };
    bench.enqueue_auto(&recipe_id, count);
    true
}

/// **A BAND'S SUGGESTIONS AND OFFERS, EXACTLY AS THE SNAPSHOT READS THEM** — `None` for a band the
/// snapshot publishes none for (a detached party), or a world missing what the readout needs.
fn band_suggestions_and_offers(
    world: &World,
    band: Entity,
    bench: &BandBench,
    configs: &FillConfigs,
) -> Option<(Vec<CraftSuggestion>, Vec<CraftOfferState>)> {
    if world.get::<Expedition>(band).is_some() {
        return None;
    }
    let cohort = world.get::<PopulationCohort>(band)?;
    let allocation = world.get::<LaborAllocation>(band)?;
    let wear = band_wear(world, band, &configs.equipment);
    let claims = with_world_sources(world, |sources| row_claims(sources, allocation, &wear));
    let gear = take_row_gear(&configs.equipment, allocation, &claims, &wear);
    let take_rows = gear.iter().enumerate().flat_map(|(row, gear)| {
        gear.toe
            .iter()
            .map(move |line| (row, line.item_id.as_str(), line.required, line.filled))
    });
    let suggestions = band_suggestions(
        allocation,
        take_rows,
        Some(bench),
        &configs.recipes,
        &configs.equipment,
        &wear,
    );

    let ladder = world.get_resource::<LadderConfigHandle>()?.get();
    let labor = world.get_resource::<crate::LaborConfigHandle>()?.get();
    let expedition = world.get_resource::<crate::ExpeditionConfigHandle>()?.get();
    let plans = plan_craft_offers(&configs.recipes, &configs.equipment);
    let known = world
        .get_resource::<DiscoveryProgressLedger>()
        .map(|ledger| {
            known_crafts(
                &configs.materials,
                ledger,
                cohort.faction,
                ladder.knowledge.completion_threshold,
            )
        })
        .unwrap_or_default();
    let inputs = BandCraftInputs {
        materials: &configs.materials,
        equipment: &configs.equipment,
        plans: &plans,
        known_crafts: &known,
        recipes: &configs.recipes,
        reference_build_cost: ladder.reference_build_cost(),
        equipped_elsewhere: EquippedElsewhere {
            scout_vantage_range: labor.scout.vantage_range as f32,
            expedition_sight_range: expedition.observe_sight_range as f32,
        },
    };
    let offers = band_craft_state(&cohort.stores, Some(bench), &wear, &inputs).craft_offers;
    Some((suggestions, offers))
}

/// The band's kit ledger — a band with none reads as start-stocked, as the snapshot's does.
fn band_wear(world: &World, band: Entity, equipment: &EquipmentConfig) -> BandEquipment {
    world
        .get::<BandEquipment>(band)
        .cloned()
        .unwrap_or_else(|| BandEquipment::start_stocked(equipment))
}

/// **FILL A BAND'S EMPTY QUEUE FROM ITS SUGGESTIONS** when auto-craft is on. Returns whether an
/// order was queued. A no-op for a bench that is off, a queue that is not empty, a detached party,
/// or a band with no usable suggestion.
///
/// Called by the turn system ([`advance_auto_craft`]) and at the end of every command that can
/// leave the queue empty or switch auto on.
pub fn auto_craft_fill(world: &mut World, band: Entity) -> bool {
    let Some(original) = world.get::<BandBench>(band) else {
        return false;
    };
    if !original.auto || !original.orders.is_empty() {
        return false;
    }
    let Some(configs) = FillConfigs::resolve(world) else {
        return false;
    };
    let mut bench = original.clone();
    let queued = {
        let Some((suggestions, offers)) =
            band_suggestions_and_offers(world, band, &bench, &configs)
        else {
            return false;
        };
        let Some(cohort) = world.get::<PopulationCohort>(band) else {
            return false;
        };
        let wear = band_wear(world, band, &configs.equipment);
        fill_empty_queue(
            &mut bench,
            &suggestions,
            &offers,
            &cohort.stores,
            &wear,
            &configs,
        )
    };
    // The un-skip may have changed the set even when nothing was queued.
    if let Some(mut live) = world.get_mut::<BandBench>(band) {
        if *live != bench {
            *live = bench;
        }
    }
    queued
}

/// **SWITCH AUTO-CRAFT ON OR OFF.** Off clears [`BandBench::auto_skipped`] and leaves every order
/// alone (an auto order is then just an order the player can edit); on fills an empty queue at once.
pub fn set_auto_craft(world: &mut World, band: Entity, enabled: bool) {
    let Some(mut bench) = world.get_mut::<BandBench>(band) else {
        return;
    };
    bench.auto = enabled;
    if !enabled {
        bench.auto_skipped.clear();
        return;
    }
    auto_craft_fill(world, band);
}

/// **SKIP THE AUTO ORDER THE BENCH IS WAITING ON.** Valid only when the bench has no order it can
/// work **and** the head order is auto-craft's. The head's item is parked in
/// [`BandBench::auto_skipped`], the head order is removed (it has drawn nothing — a drawn order is
/// workable — so nothing is forfeited), and the queue is refilled from the next suggestion.
///
/// Returns the skipped item's id.
pub fn auto_craft_skip(world: &mut World, band: Entity) -> Result<String, AutoSkipRefusal> {
    let configs = FillConfigs::resolve(world).ok_or(AutoSkipRefusal::NoBench)?;
    let cohort = world
        .get::<PopulationCohort>(band)
        .ok_or(AutoSkipRefusal::NoBench)?;
    let bench = world
        .get::<BandBench>(band)
        .ok_or(AutoSkipRefusal::NoBench)?;
    let head = bench.head().ok_or(AutoSkipRefusal::NothingQueued)?;
    let wear = band_wear(world, band, &configs.equipment);
    if worked_order(
        bench,
        &cohort.stores,
        &configs.recipes,
        &configs.materials,
        &configs.equipment,
        &wear,
    )
    .is_some()
    {
        return Err(AutoSkipRefusal::BenchIsWorking);
    }
    if !head.auto {
        return Err(AutoSkipRefusal::HeadNotAuto);
    }
    let item = configs
        .recipes
        .recipe(&head.recipe_id)
        .and_then(|recipe| recipe.output_equipment_id())
        .map(str::to_string)
        .unwrap_or_else(|| head.recipe_id.clone());
    let mut bench = world
        .get_mut::<BandBench>(band)
        .ok_or(AutoSkipRefusal::NoBench)?;
    bench.auto_skipped.insert(item.clone());
    bench
        .remove_order(HEAD_ORDER)
        .map_err(|_| AutoSkipRefusal::NothingQueued)?;
    auto_craft_fill(world, band);
    Ok(item)
}

/// **THE TURN SYSTEM** — fills every auto bench's empty queue. Scheduled right before
/// `advance_crafting` and again right after it, so an order that completes this turn has its
/// successor in the snapshot.
pub fn advance_auto_craft(world: &mut World) {
    fill_every_empty_auto_bench(world);
}

/// **THE SAME FILL, AFTER THE BENCH HAS WORKED** — a second function so the two schedule slots are
/// two distinct systems, not one system added twice.
pub fn advance_auto_craft_after_bench(world: &mut World) {
    fill_every_empty_auto_bench(world);
}

fn fill_every_empty_auto_bench(world: &mut World) {
    let bands: Vec<Entity> = world
        .query::<(Entity, &BandBench)>()
        .iter(world)
        .filter(|(_, bench)| bench.auto && bench.orders.is_empty())
        .map(|(entity, _)| entity)
        .collect();
    for band in bands {
        auto_craft_fill(world, band);
    }
}
