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

use crate::components::{BandBench, BandId, PopulationCohort, MIN_ORDER_COUNT};
use crate::equipment_config::EquipmentConfigHandle;
use crate::intensification::{knows, LadderConfigHandle};
use crate::orders::FactionId;
use crate::recipes_config::RecipesConfigHandle;
use crate::resources::DiscoveryProgressLedger;
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
    /// A bench verb was refused. The sentence is the one the event feed shows, verb prefix included.
    #[error("{0}")]
    Bench(String),
    /// The band a replayed verb names is not a band of that faction in the world.
    #[error("band {} does not exist for faction {}", .band.0, .faction.0)]
    NoSuchBand { faction: FactionId, band: BandId },
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

/// **ONE ORDER** - what an enqueue adds to the queue's length, so the new order's index is
/// `len - ONE_ORDER`.
pub const ONE_ORDER: usize = 1;

/// The band's bench, inserted on demand so a band spawned before the component existed still has
/// one - the same shape `band_allocation_mut` uses for the labor allocation.
pub fn band_bench_mut(world: &mut World, band: Entity) -> Mut<'_, BandBench> {
    if world.get::<BandBench>(band).is_none() {
        world.entity_mut(band).insert(BandBench::default());
    }
    world
        .get_mut::<BandBench>(band)
        .expect("bench inserted above")
}

/// The entity of `faction`'s band `band`, or [`CommandRefusal::NoSuchBand`].
pub fn resolve_band(
    world: &mut World,
    faction: FactionId,
    band: BandId,
) -> Result<Entity, CommandRefusal> {
    let mut query = world.query::<(Entity, &BandId, &PopulationCohort)>();
    query
        .iter(world)
        .find(|(_, id, cohort)| **id == band && cohort.faction == faction)
        .map(|(entity, _, _)| entity)
        .ok_or(CommandRefusal::NoSuchBand { faction, band })
}

/// **What a queued plain order did**, for the success line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchEnqueued {
    /// The recipe's display name (*Spears (Flint)*).
    pub display_name: String,
    /// The new order's index on the queue.
    pub position: usize,
    /// The crew the sim staffed it with.
    pub crew: u32,
}

/// **What a queued kit order did**, for the success line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchKitEnqueued {
    pub kit_display_name: String,
    /// The new kit order's id on the band's bench.
    pub order_id: u32,
    /// How many part orders it queued.
    pub parts: usize,
}

fn bench_refusal(sentence: String) -> CommandRefusal {
    CommandRefusal::Bench(sentence)
}

/// **The two recipe refusals every order-adding verb shares** - an unknown id and an unlearned
/// craft. Returns the recipe's display name (*Spears (Flint)*: the row's name plus its label,
/// because the item owns its name and a recipe owns only the word that tells it from its siblings).
fn bench_recipe_check(
    world: &World,
    faction: FactionId,
    recipe_id: &str,
    verb: &str,
) -> Result<String, CommandRefusal> {
    let recipes = world.resource::<RecipesConfigHandle>().get();
    let Some(recipe) = recipes.recipe(recipe_id) else {
        return Err(bench_refusal(format!(
            "{verb}: unknown recipe '{recipe_id}' — the book offers {}.",
            recipes.recipe_ids_for_message()
        )));
    };
    let threshold = world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .completion_threshold;
    let display_name = recipe.full_name(&world.resource::<EquipmentConfigHandle>().get());
    let ledger = world.resource::<DiscoveryProgressLedger>();
    // The one predicate the published `CraftOffer.queueable` also reads.
    let unknown_craft = crate::crafting::first_unknown_craft(recipe, |craft| {
        crate::crafting::craft_discovery_id(craft)
            .is_some_and(|id| knows(ledger, faction, id, threshold))
    });
    if let Some(craft) = unknown_craft {
        return Err(bench_refusal(format!(
            "{verb}: {display_name} needs {craft}, which this people has not learned — a \
             craft is learned by practising it bare-handed."
        )));
    }
    Ok(display_name)
}

/// [`apply_bench_enqueue_on`] for a band named by its id.
pub fn apply_bench_enqueue(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    recipe_id: &str,
    count: u32,
) -> Result<BenchEnqueued, CommandRefusal> {
    let entity = resolve_band(world, faction, band)?;
    apply_bench_enqueue_on(world, faction, entity, recipe_id, count)
}

/// **Add an order to a band's bench queue** (`docs/plan_crafting_and_materials.md` section 7, "The
/// queue"). The body of `bench_enqueue`, for the server's handler and a scenario's `bench` section.
///
/// Three refusals, all with a reason rather than silent no-ops: an id the book does not carry, a
/// recipe whose `requires_knowledge` this faction has not learned (only ever a **tool**), and a
/// count below [`MIN_ORDER_COUNT`]. **There is no refusal for material**: a band that is short makes
/// no progress, and the panel names the shortfall.
///
/// **The crew is the sim's**: the band is restaffed here, so the new order takes idle hands at once.
pub fn apply_bench_enqueue_on(
    world: &mut World,
    faction: FactionId,
    band: Entity,
    recipe_id: &str,
    count: u32,
) -> Result<BenchEnqueued, CommandRefusal> {
    const VERB: &str = "bench_enqueue";
    let display_name = bench_recipe_check(world, faction, recipe_id, VERB)?;
    if count < MIN_ORDER_COUNT {
        return Err(bench_refusal(format!(
            "{VERB}: an order of {display_name} needs a count of at least {MIN_ORDER_COUNT}."
        )));
    }
    // **Which row this order belongs to** - what the ledger's per-item suggestion is keyed by.
    let row_key = world
        .resource::<RecipesConfigHandle>()
        .get()
        .recipe(recipe_id)
        .and_then(|recipe| recipe.row_key().map(str::to_string));
    let position = {
        let mut bench = band_bench_mut(world, band);
        bench.enqueue(recipe_id, count);
        // **Written HERE and nowhere else** - queueing an order is the one event that says which of
        // an item's recipes this band chose, and the ledger suggests that one again next time. A
        // readout must never write it: it would be the panel deciding what the player picked.
        if let Some(row) = row_key.as_deref() {
            bench.record_started(row, recipe_id);
        }
        bench.orders.len() - ONE_ORDER
    };
    // **The sim staffs the order** in the same passes as the work rows.
    crate::yield_seed::restaff_and_reseed(world, band);
    let crew = world
        .get::<BandBench>(band)
        .and_then(|bench| bench.orders.get(position))
        .map_or(0, |order| order.crew.total());
    Ok(BenchEnqueued {
        display_name,
        position,
        crew,
    })
}

/// [`apply_bench_enqueue_kit_on`] for a band named by its id.
pub fn apply_bench_enqueue_kit(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    kit_id: &str,
    kits: u32,
    recipe_ids: &[String],
) -> Result<BenchKitEnqueued, CommandRefusal> {
    let entity = resolve_band(world, faction, band)?;
    apply_bench_enqueue_kit_on(world, faction, entity, kit_id, kits, recipe_ids)
}

/// **Order a whole kit** (issue #780, `.claude/rules/core_sim/crafting.md` -> "Kit orders").
///
/// **`kits` is a count of kits the player chose, and the sim never nets it down.** **Fails closed
/// and whole**: the kit must exist and carry something (the shipped `none` carries nothing and is
/// not orderable), `kits` must be at least [`MIN_ORDER_COUNT`], every recipe passes the same checks
/// `bench_enqueue` makes, every recipe's output item must be one the kit `uses`, no two recipes may
/// make the same item, and every item of the kit must have a recipe. The crew is untouched.
pub fn apply_bench_enqueue_kit_on(
    world: &mut World,
    faction: FactionId,
    band: Entity,
    kit_id: &str,
    kits: u32,
    recipe_ids: &[String],
) -> Result<BenchKitEnqueued, CommandRefusal> {
    const VERB: &str = "bench_enqueue_kit";
    let equipment = world.resource::<EquipmentConfigHandle>().get();
    let Some(kit) = equipment.orderable_kit(kit_id) else {
        return Err(bench_refusal(format!(
            "{VERB}: '{kit_id}' is not a kit that can be ordered."
        )));
    };
    if kits < MIN_ORDER_COUNT {
        return Err(bench_refusal(format!(
            "{VERB}: an order of the {} needs at least {MIN_ORDER_COUNT} kit.",
            kit.display_name
        )));
    }
    let recipes = world.resource::<RecipesConfigHandle>().get();
    let mut row_keys: Vec<Option<String>> = Vec::with_capacity(recipe_ids.len());
    let mut items_covered: Vec<String> = Vec::with_capacity(recipe_ids.len());
    for recipe_id in recipe_ids {
        let display_name = bench_recipe_check(world, faction, recipe_id, VERB)?;
        let recipe = recipes
            .recipe(recipe_id)
            .expect("bench_recipe_check found the recipe");
        let output = recipe
            .output_equipment_id()
            .filter(|item| kit.uses.iter().any(|used| used == item));
        let Some(item) = output else {
            return Err(bench_refusal(format!(
                "{VERB}: {display_name} makes nothing the {} uses.",
                kit.display_name
            )));
        };
        if items_covered.iter().any(|covered| covered == item) {
            return Err(bench_refusal(format!(
                "{VERB}: two recipes make the same item ({display_name})."
            )));
        }
        items_covered.push(item.to_string());
        row_keys.push(recipe.row_key().map(str::to_string));
    }
    if let Some(missing) = kit
        .uses
        .iter()
        .find(|used| !items_covered.iter().any(|covered| covered == *used))
    {
        return Err(bench_refusal(format!(
            "{VERB}: the {} needs a recipe for its {missing}.",
            kit.display_name
        )));
    }
    let queue: Vec<(String, u32)> = recipe_ids
        .iter()
        .map(|recipe_id| (recipe_id.clone(), kits))
        .collect();
    let order_id = {
        let mut bench = band_bench_mut(world, band);
        let id = bench
            .enqueue_kit(kit_id, kits, queue)
            .expect("the kit count was checked to be at least one");
        for (recipe_id, row) in recipe_ids.iter().zip(&row_keys) {
            if let Some(row) = row.as_deref() {
                bench.record_started(row, recipe_id);
            }
        }
        id
    };
    crate::yield_seed::restaff_and_reseed(world, band);
    Ok(BenchKitEnqueued {
        kit_display_name: kit.display_name.clone(),
        order_id,
        parts: recipe_ids.len(),
    })
}

/// [`apply_bench_auto_on`] for a band named by its id.
pub fn apply_bench_auto(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    enabled: bool,
) -> Result<(), CommandRefusal> {
    let entity = resolve_band(world, faction, band)?;
    apply_bench_auto_on(world, entity, enabled);
    Ok(())
}

/// **Turn a band's bench auto-craft on or off** (#779, `.claude/rules/core_sim/crafting.md` ->
/// "Auto-craft"). On: an empty queue is filled at once from the band's top craft suggestion. Off:
/// the skipped set is forgotten and every order is left as it is. **Never touches the crew.** It has
/// no refusal of its own.
pub fn apply_bench_auto_on(world: &mut World, band: Entity, enabled: bool) {
    // Inserted on demand, as every bench verb does, so a band spawned before the component existed
    // can still switch it on.
    let _ = band_bench_mut(world, band);
    crate::auto_craft::set_auto_craft(world, band, enabled);
}
