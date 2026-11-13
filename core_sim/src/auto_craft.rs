//! **Auto-craft — the bench keeps its own queue filled from the ranked suggestions** (#779,
//! `.claude/rules/core_sim/crafting.md` → "Auto-craft").
//!
//! An opt-in per-band switch ([`BandBench::auto`]). While it is on and the queue is **empty**, the
//! bench queues the first usable entry of the band's craft-suggestion list at its whole netted
//! count, tagged [`BenchOrder::auto`](crate::components::BenchOrder::auto). Nothing else is ever
//! decided here:
//!
//! - **It adds one order, and only when the bench has room** - every workable order already at its
//!   crew cap (`bench_has_room`); the band is restaffed and the labor passes decide whether the new
//!   order gets a crafter (pass 2 opens with one on every workable order). An auto order that got
//!   none is below its cap, so the bench has no room and nothing more is queued.
//! - **It never walks down the list on its own.** An auto order short of materials simply waits
//!   (the ordinary skip/waiting semantics); only the player's skip ([`auto_craft_skip`]) moves on.
//! - **It never picks a crew.** The sim staffs every order in the labor passes ([`BandBench::crew_total`]).
//! - **A skipped item is passed over until one of it can be drawn from stock**, and returns only
//!   when the queue is next empty — it never preempts a running order.
//!
//! The suggestions are the snapshot's own: [`take_row_gear`] settles the gear and
//! [`band_suggestions`] ranks it, the same two functions `snapshot::population` calls, and the
//! recipe an item is queued with is the offer the crafting ledger marks `suggested` for it — the
//! recipe the panel's Queue button sends.

use bevy::prelude::*;

use crate::{
    components::{BandBench, BandEquipment, Expedition, LaborAllocation, PopulationCohort},
    craft_suggestions::{
        band_suggestions, bench_tool_suggestions, build_material_suggestions_in_world,
        merge_build_material_suggestions, take_row_gear, BenchToolInputs, CraftSuggestion,
        ForecastInputs, SuggestionConfigs,
    },
    equipment_config::{EquipmentConfig, EquipmentConfigHandle},
    intensification::LadderConfigHandle,
    materials_config::{MaterialsConfig, MaterialsConfigHandle},
    recipes_config::{RecipesConfig, RecipesConfigHandle},
    resources::DiscoveryProgressLedger,
    snapshot::crafting::{
        band_craft_state, known_crafts, plan_craft_offers, BandCraftInputs, EquippedElsewhere,
    },
    systems::{bench_plan, recipe_is_drawable, BenchPlan},
    take_claims::{row_claims_with_idle, with_world_sources},
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
    offers.iter().find(|offer| {
        offer.suggested && (offer.output_item_id == item || offer.output_material_id == item)
    })
}

/// **WHAT AUTO-CRAFT WOULD DO WITH THE LIST** - the one pick the fill and the capture's
/// [`auto_wait_reason`] both read, so the reason published is the decision the fill makes.
#[derive(Debug, Clone, PartialEq)]
pub enum AutoPick {
    /// The first usable suggestion: queue `recipe_id` at `count`.
    Queue {
        item: String,
        recipe_id: String,
        count: u32,
    },
    /// The first usable suggestion's row already has an **auto** order on the bench (not a kit part):
    /// raise that order's `count` by the suggestion's netted `count`. The order keeps its own recipe.
    TopUp { order: usize, count: u32 },
    /// Nothing is usable; `item` is the first suggestion passed over for want of something, with its
    /// offer's own `reason` (the CraftOffer vocabulary: `Short 0.4 bone`, `Needs Bone-working`).
    Blocked { item: String, reason: String },
    /// No suggestion to work down (empty list, all skipped or already queued).
    Nothing,
}

/// The separator between a suggestion's name and its offer's reason in a blocked pick.
const BLOCKED_REASON_SEPARATOR: &str = ": ";

/// **CAN THIS ITEM BE DRAWN FROM STOCK?** - one item of the recipe the ledger marks `suggested`.
fn item_is_drawable(
    offers: &[CraftOfferState],
    item: &str,
    store: &crate::components::LocalStore,
    wear: &BandEquipment,
    recipes: &RecipesConfig,
    materials: &MaterialsConfig,
    equipment: &EquipmentConfig,
) -> bool {
    suggested_offer(offers, item).is_some_and(|offer| {
        recipe_is_drawable(&offer.recipe_id, store, recipes, materials, equipment, wear)
    })
}

/// **UN-SKIP**: an item whose recipe can draw ONE item from the stock on hand is eligible again, and
/// an item nobody is going without any more (no longer suggested) is dropped - a stale entry must
/// not survive. An item re-enters at its rank in the list, not at the head of anything.
fn unskip(bench: &mut BandBench, suggestions: &[CraftSuggestion], drawable: impl Fn(&str) -> bool) {
    bench.auto_skipped.retain(|item| {
        suggestions
            .iter()
            .any(|suggestion| &suggestion.item == item)
            && !drawable(item)
    });
}

/// **THE PICK**: walk the suggestions in rank order, passing over a skipped item and one whose row
/// already has a player's or kit-part order on the bench (an auto order on the row is topped up). The first whose suggested offer is `queueable` and drawable
/// for ONE item is queued (never a suggestion itself short of an input - it could not progress
/// either, and the next makeable one is worth more to idle crafters). With none, the first one
/// passed over for want of a craft or a material is reported as `Blocked`.
fn auto_pick(
    bench: &BandBench,
    suggestions: &[CraftSuggestion],
    offers: &[CraftOfferState],
    recipes: &RecipesConfig,
    drawable: impl Fn(&str) -> bool,
) -> AutoPick {
    // The first order on the row, if any: an auto order that is no kit part can be topped up, any
    // other (the player's, a kit part) keeps the row off-limits to auto-craft.
    let order_on_row = |item: &str| {
        bench.orders.iter().position(|order| {
            recipes
                .recipe(&order.recipe_id)
                .and_then(|recipe| recipe.row_key())
                == Some(item)
        })
    };
    let mut blocked: Option<AutoPick> = None;
    for suggestion in suggestions {
        if bench.auto_skipped.contains(&suggestion.item) {
            continue;
        }
        let on_row = order_on_row(&suggestion.item);
        if let Some(index) = on_row {
            let order = &bench.orders[index];
            if !order.auto || order.kit_order != crate::components::NOT_A_KIT_PART {
                continue;
            }
        }
        let Some(offer) = suggested_offer(offers, &suggestion.item) else {
            continue;
        };
        if offer.queueable && drawable(&suggestion.item) {
            if let Some(order) = on_row {
                return AutoPick::TopUp {
                    order,
                    count: suggestion.count,
                };
            }
            return AutoPick::Queue {
                item: suggestion.item.clone(),
                recipe_id: offer.recipe_id.clone(),
                count: suggestion.count,
            };
        }
        blocked.get_or_insert_with(|| AutoPick::Blocked {
            item: suggestion.item.clone(),
            reason: offer.reason.clone(),
        });
    }
    blocked.unwrap_or(AutoPick::Nothing)
}

/// **THE PURE FILL** - un-skip, pick, and queue the pick at its whole netted count. Returns whether
/// an order was queued.
///
/// An auto bench is filled **only when crafters are free** (see [`bench_has_room`]): a blocked
/// order keeps its place and resumes by itself, and one order is added per call. Never touches the
/// crew (the caller restaffs).
fn fill_idle_bench(
    bench: &mut BandBench,
    suggestions: &[CraftSuggestion],
    offers: &[CraftOfferState],
    store: &crate::components::LocalStore,
    wear: &BandEquipment,
    configs: &FillConfigs,
) -> bool {
    if !bench.auto {
        return false;
    }
    let drawable = |item: &str| {
        item_is_drawable(
            offers,
            item,
            store,
            wear,
            &configs.recipes,
            &configs.materials,
            &configs.equipment,
        )
    };
    unskip(bench, suggestions, drawable);
    match auto_pick(bench, suggestions, offers, &configs.recipes, drawable) {
        AutoPick::Queue {
            recipe_id, count, ..
        } => {
            bench.enqueue_auto(&recipe_id, count);
            true
        }
        AutoPick::TopUp { order, count } => {
            bench.orders[order].count += count;
            true
        }
        AutoPick::Blocked { .. } | AutoPick::Nothing => false,
    }
}

/// **IS EVERY WORKABLE ORDER AT ITS CREW CAP?** (Vacuously true with none workable.) When one is
/// below it the bench has room and any idle hand is idle for want of a seat, not an order.
fn bench_is_at_cap(bench: &BandBench, plan: &BenchPlan) -> bool {
    plan.orders
        .iter()
        .enumerate()
        .filter(|(_, order)| order.workable)
        .all(|(index, order)| bench.orders[index].crew.total() >= order.crew_cap)
}

/// **HAS THE BENCH ROOM FOR ANOTHER ORDER?** Every workable order is already at its crew cap (or
/// none is workable). An auto order that got no crafter is workable and below its cap, so the bench
/// has no room until it is crewed or done: that is what stops auto piling orders onto a bench that
/// cannot staff them. Whether the new order gets a crafter is the labor passes' decision.
fn bench_has_room(bench: &BandBench, plan: &BenchPlan) -> bool {
    bench_is_at_cap(bench, plan)
}

/// The world-reading wrapper of [`bench_has_room`].
fn bench_has_room_in_world(
    world: &World,
    band: Entity,
    bench: &BandBench,
    configs: &FillConfigs,
) -> bool {
    let Some(cohort) = world.get::<PopulationCohort>(band) else {
        return false;
    };
    let wear = band_wear(world, band, &configs.equipment);
    let plan = bench_plan(
        bench,
        &cohort.stores,
        &configs.recipes,
        &configs.materials,
        &configs.equipment,
        &wear,
    );
    bench_has_room(bench, &plan)
}

/// What [`auto_wait_reason`] reads: the capture's own view of one band.
pub struct AutoWaitInputs<'a> {
    pub bench: &'a BandBench,
    pub plan: &'a BenchPlan,
    pub suggestions: &'a [CraftSuggestion],
    pub offers: &'a [CraftOfferState],
    pub store: &'a crate::components::LocalStore,
    pub wear: &'a BandEquipment,
    pub recipes: &'a RecipesConfig,
    pub materials: &'a MaterialsConfig,
    pub equipment: &'a EquipmentConfig,
}

/// **WHY AUTO-CRAFT HAS NOT STARTED THE TOP SUGGESTION** - read by the capture off the same
/// [`auto_pick`] the fill uses, on a clone of the bench (the capture never mutates).
///
/// - `{Name}: {offer reason}` - the bench has room and nothing is usable; the first suggestion passed
///   over, with its own offer's reason.
/// - `""` otherwise: auto off, a workable order below its crew cap (no room; the order rows say why),
///   a usable pick (the next fill queues it) and nothing to make.
pub fn auto_wait_reason(inputs: &AutoWaitInputs) -> String {
    if !inputs.bench.auto || !bench_is_at_cap(inputs.bench, inputs.plan) {
        return String::new();
    }
    let mut bench = inputs.bench.clone();
    let drawable = |item: &str| {
        item_is_drawable(
            inputs.offers,
            item,
            inputs.store,
            inputs.wear,
            inputs.recipes,
            inputs.materials,
            inputs.equipment,
        )
    };
    unskip(&mut bench, inputs.suggestions, drawable);
    let name_of = |item: &str| {
        inputs
            .suggestions
            .iter()
            .find(|suggestion| suggestion.item == item)
            .map(|suggestion| suggestion.display_name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| crate::crafting::title_from_id(item))
    };
    match auto_pick(
        &bench,
        inputs.suggestions,
        inputs.offers,
        inputs.recipes,
        drawable,
    ) {
        AutoPick::Blocked { item, reason } => {
            format!("{}{BLOCKED_REASON_SEPARATOR}{reason}", name_of(&item))
        }
        AutoPick::Queue { .. } | AutoPick::TopUp { .. } | AutoPick::Nothing => String::new(),
    }
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
    // The asks follow the band's idle hands, as the snapshot reads them.
    let idle =
        crate::components::BandWorkforce::resolve(Some(cohort), Some(allocation), Some(bench))
            .idle();
    let claims = with_world_sources(world, cohort.faction, |sources| {
        row_claims_with_idle(sources, allocation, &wear, idle)
    });
    let gear = take_row_gear(&configs.equipment, allocation, &claims, &wear);
    let take_rows = gear.iter().enumerate().flat_map(|(row, gear)| {
        gear.toe
            .iter()
            .map(move |line| (row, line.item_id.as_str(), line.required, line.filled))
    });
    // **The hunt-turn pass**, so a kit a going-out hunt crew is short of is in the list auto-craft
    // works down exactly as it is in the panel's (`snapshot::population::hunt_turn_lines`).
    let hunt_turn = hunt_turn_pass(world, cohort, allocation, bench, &wear, configs);
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
    let offers = band_craft_state(&cohort.stores, Some(bench), &[], &wear, &inputs).craft_offers;
    // The offers come first: the wear forecast reads the recipe each item's row marks `suggested`.
    let mut suggestions = band_suggestions(
        allocation,
        take_rows,
        hunt_turn.as_deref(),
        Some(bench),
        SuggestionConfigs {
            recipes: &configs.recipes,
            equipment: &configs.equipment,
        },
        &wear,
        Some(ForecastInputs {
            offers: &offers,
            materials: &configs.materials,
        }),
    );
    // **A declared build short of the GOODS it draws** (hurdles for a pen) is a suggestion like any
    // other - the same wrapper the snapshot merges - so an empty auto bench works it down.
    let build_materials = with_world_sources(world, cohort.faction, |sources| {
        build_material_suggestions_in_world(
            allocation,
            &cohort.stores,
            &configs.recipes,
            Some(bench),
            sources,
        )
    });
    merge_build_material_suggestions(&mut suggestions, build_materials);
    // **A bench tool that pays for itself** is on the list auto-craft works down, ranked where the
    // snapshot ranks it.
    bench_tool_suggestions(
        &mut suggestions,
        &BenchToolInputs {
            bench: Some(bench),
            wear: &wear,
            recipes: &configs.recipes,
            equipment: &configs.equipment,
            materials: &configs.materials,
            offers: Some(&offers),
        },
    );
    Some((suggestions, offers))
}

/// The hunt-turn shortfall lines for a band whose need row is sending a crew, resolved from the world
/// exactly as the capture resolves them: idle hands through [`crate::components::BandWorkforce`], the donor predicate from
/// the band's tile and the work range.
fn hunt_turn_pass(
    world: &World,
    cohort: &PopulationCohort,
    allocation: &LaborAllocation,
    bench: &BandBench,
    wear: &BandEquipment,
    configs: &FillConfigs,
) -> Option<Vec<crate::craft_suggestions::ToolShortfallLine>> {
    let herds = world.get_resource::<crate::HerdRegistry>()?;
    let labor = world.get_resource::<crate::LaborConfigHandle>()?.get();
    let width = world
        .get_resource::<crate::resources::TileRegistry>()?
        .width;
    let wrap = world
        .get_resource::<crate::SimulationConfig>()?
        .map_topology
        .wrap_horizontal;
    let band_pos = world
        .get::<crate::components::Tile>(cohort.current_tile)
        .map(|tile| tile.position);
    let idle =
        crate::components::BandWorkforce::resolve(Some(cohort), Some(allocation), Some(bench))
            .idle();
    let is_local = |target: &crate::components::LaborTarget| {
        crate::hunt_by_need::source_is_local(target, herds, band_pos, &labor, (width, wrap))
    };
    with_world_sources(world, cohort.faction, |sources| {
        crate::snapshot::population::hunt_turn_lines(
            allocation,
            idle,
            &is_local,
            crate::components::MusterBench::of(bench),
            sources,
            wear,
            &configs.equipment,
            None,
        )
    })
}

/// The band's kit ledger — a band with none reads as start-stocked, as the snapshot's does.
fn band_wear(world: &World, band: Entity, equipment: &EquipmentConfig) -> BandEquipment {
    world
        .get::<BandEquipment>(band)
        .cloned()
        .unwrap_or_else(|| BandEquipment::start_stocked(equipment))
}

/// **FILL A BAND'S IDLE BENCH FROM ITS SUGGESTIONS** when auto-craft is on - whenever no order can
/// progress (`worked_order` is `None`), which an empty queue is one case of. Returns whether an
/// order was queued. A no-op for a bench that is off, a bench with an order that can progress, a
/// detached party, or a band with no usable suggestion.
///
/// Called by the turn system ([`advance_auto_craft`]) and at the end of every command that can
/// leave the bench with nothing workable or switch auto on.
pub fn auto_craft_fill(world: &mut World, band: Entity) -> bool {
    let Some(original) = world.get::<BandBench>(band) else {
        return false;
    };
    if !original.auto {
        return false;
    }
    let Some(configs) = FillConfigs::resolve(world) else {
        return false;
    };
    // A bench with no room for another order needs no suggestions computed at all.
    if !bench_has_room_in_world(world, band, original, &configs) {
        return false;
    }
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
        fill_idle_bench(
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
    // **A new order takes the idle hands THIS turn**: the band is restaffed so the snapshot (and the
    // turn's craft pass) show the new crew at once.
    if queued {
        crate::staffing::restaff_band(world, band);
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
    let head = bench.orders.first().ok_or(AutoSkipRefusal::NothingQueued)?;
    let wear = band_wear(world, band, &configs.equipment);
    if bench_plan(
        bench,
        &cohort.stores,
        &configs.recipes,
        &configs.materials,
        &configs.equipment,
        &wear,
    )
    .orders
    .iter()
    .any(|order| order.workable)
    {
        return Err(AutoSkipRefusal::BenchIsWorking);
    }
    if !head.auto {
        return Err(AutoSkipRefusal::HeadNotAuto);
    }
    let item = configs
        .recipes
        .recipe(&head.recipe_id)
        .and_then(|recipe| recipe.row_key())
        .map(str::to_string)
        .unwrap_or_else(|| head.recipe_id.clone());
    let mut bench = world
        .get_mut::<BandBench>(band)
        .ok_or(AutoSkipRefusal::NoBench)?;
    bench.auto_skipped.insert(item.clone());
    bench
        .remove_order(0)
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
        .filter(|(_, bench)| bench.auto)
        .map(|(entity, _)| entity)
        .collect();
    for band in bands {
        auto_craft_fill(world, band);
    }
}
