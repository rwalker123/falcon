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

use crate::build_gates::{validate_improvement, PEN_TAKEN_HINT};
use crate::components::{BandBench, BandId, PopulationCohort, MIN_ORDER_COUNT};
use crate::equipment_config::EquipmentConfigHandle;
pub use crate::labor_commands::{
    apply_assign_labor, apply_band_order, apply_work_priority, forage_source, is_pen_climb,
    pen_queuer, queue_build_on_working_bands, AssignLaborOrder, SOURCE_NAMED_NOT_ASSIGNED,
};
use crate::orders::FactionId;
use crate::recipes_config::RecipesConfigHandle;
use crate::resources::CommandEventKind;
use crate::starting_loadout::{
    apply_starting_loadout, KitAllocation, LoadoutRejection, LoadoutSupply, MaterialAllocation,
    StartingLoadout,
};
use crate::{
    herd_rung_key, knows, BranchId, BuildJob, BuildSource, DiscoveryProgressLedger,
    FaunaConfigHandle, HerdRegistry, LaborAllocation, LaborTarget, LadderConfigHandle, RoadKeeper,
    RoadRegistry, RungDef, SourceKind, TakeSelection, Tile, VerbId, NO_STANDING_COMMITMENT,
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
    /// `BuildRung` was refused. `channel` is the feed channel the player's refusal rides, `verb`
    /// the verb as the handler lowercased it, `reason` the text the feed shows, and `log` which
    /// warn line the server's handler writes beside it.
    #[error("{reason}")]
    Build {
        channel: CommandEventKind,
        verb: String,
        reason: String,
        log: RefusalLog,
    },
    /// A labor verb (`assign_labor`, `work_priority`) was refused. `kind` is the feed channel the
    /// player's own refusal lands on and `reason` the text it shows; the server writes exactly that
    /// line, the scenario loader reports exactly that reason.
    #[error("{reason}")]
    Refused {
        kind: crate::CommandEventKind,
        reason: String,
    },
}

/// **Which warn line the server writes for a refused build** - the shapes the handler's refusals
/// have always logged under, so moving the body here changed no log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusalLog {
    /// `command.build.rejected`.
    Build,
    /// `command.deposit.rejected`, naming the working.
    Deposit { tile: UVec2, material: String },
    /// `command.road.rejected`, naming the keeper band and the tile.
    Road { band_id: u64, tile: UVec2 },
    /// A feed line only, no warn.
    FeedOnly,
}

/// **What a `build` line asked for** - the verb as the player typed it, and whichever source fields
/// the wire carried. The ladder reads the source kind off the verb's rung's branch, so nothing here
/// says what the verb is.
#[derive(Debug, Clone, Default)]
pub struct BuildRungRequest {
    /// The keeper band, for a source kind with no work row (a road).
    pub band_id: Option<u64>,
    pub verb: String,
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    pub herd_id: Option<String>,
    pub material: Option<String>,
    /// The ladder branch, when the verb alone does not name it.
    pub branch: Option<String>,
}

/// **What a declared build leaves for the caller to say**: the feed channel and verb, and the shape
/// of the declaration. The seam writes no log line and no feed event, so a scenario replay writes
/// none; the server's handler writes both from this.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildDeclared {
    pub channel: CommandEventKind,
    pub verb: String,
    pub kind: DeclaredKind,
}

/// The shapes a declared build takes, each carrying what its log line and feed event name.
#[derive(Debug, Clone, PartialEq)]
pub enum DeclaredKind {
    /// A patch, herd, tile or band source: `label` is the place as the feed names it.
    Source { label: String, bands: usize },
    /// A deposit working.
    Deposit {
        tile: UVec2,
        material: String,
        bands: usize,
    },
    /// A road, with the keeper's quoted distance and remoteness.
    Road {
        band_id: u64,
        tile: UVec2,
        distance: u32,
        remoteness: f32,
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

/// **Gate a plant build verb against the crops the CREWS actually hold**, not against the auto-pick.
///
/// `cultivate`/`sow` name a tile and nothing else, so they used to hand [`validate_improvement`] a
/// `species: None` target - which judges what `resolve_committed_species` would pick *for* the
/// player. The crop that will really be committed is the one riding each band's
/// `LaborTarget::Forage::species`, and if this rung refuses it the labor arm silently declines to
/// commit: `patch.species` stays `None`, the rung's `eligible` gate is false, and **the build meter
/// never advances, with no player-facing feedback**. So every distinct crop held on the source is
/// judged, and the first refusal is the command's.
///
/// With **no band working the source** the list is `[None]`, so the auto-pick is judged exactly as
/// before and the gates above the species check (knowledge, phase, ownership, the site rule) keep
/// their precedence over the command's own "no band is foraging" rejection.
pub fn validate_forage_improvement_for_crews(
    world: &mut World,
    faction: FactionId,
    tile: UVec2,
    improvement: VerbId,
) -> Result<(), String> {
    for species in crops_named_on_forage_source(world, faction, tile) {
        let target = LaborTarget::Forage {
            tile,
            floor: SOURCE_NAMED_NOT_ASSIGNED,
            species,
            take_species: TakeSelection::EVERYTHING,
        };
        validate_improvement(world, faction, &target, improvement)?;
    }
    Ok(())
}

/// The distinct crop selections this faction's bands carry on the forage source at `tile`, in a
/// stable order. `[None]` when nobody works it - see [`validate_forage_improvement_for_crews`].
fn crops_named_on_forage_source(
    world: &mut World,
    faction: FactionId,
    tile: UVec2,
) -> Vec<Option<String>> {
    let source = forage_source(tile);
    let mut crops: Vec<Option<String>> = Vec::new();
    let mut query = world.query::<(&PopulationCohort, &LaborAllocation)>();
    for (_, allocation) in query
        .iter(world)
        .filter(|(cohort, _)| cohort.faction == faction)
    {
        for assignment in &allocation.assignments {
            if !assignment.target.same_source(&source) || assignment.workers == 0 {
                continue;
            }
            let LaborTarget::Forage { species, .. } = &assignment.target else {
                continue;
            };
            if !crops.contains(species) {
                crops.push(species.clone());
            }
        }
    }
    if crops.is_empty() {
        crops.push(None);
    }
    crops
}

/// **A band by its durable id, and the tile it is standing on** - the pair the road verbs need, so
/// the lookup and the position cannot come from two different frames.
///
/// **Resolves only the commanding faction's own bands.** A band id is a durable, guessable handle,
/// so a resolver that matches on the id alone hands another faction's band to whatever asked.
pub fn band_entity_and_tile(
    world: &mut World,
    faction: FactionId,
    band: BandId,
) -> Option<(Entity, UVec2)> {
    let mut query = world.query::<(Entity, &PopulationCohort, &BandId)>();
    let (entity, current_tile) = query
        .iter(world)
        .find(|(_, cohort, id)| **id == band && cohort.faction == faction)
        .map(|(entity, cohort, _)| (entity, cohort.current_tile))?;
    let position = world.get::<Tile>(current_tile)?.position;
    Some((entity, position))
}

/// **Every refusal `grade` / `pave` can produce, in the order they are asked.**
fn road_verb_refusal(
    world: &mut World,
    faction: FactionId,
    band_id: u64,
    tile: UVec2,
    improvement: VerbId,
) -> Result<(), String> {
    let band = BandId(band_id);
    let holds_band = {
        let mut query = world.query::<(&PopulationCohort, &BandId)>();
        query
            .iter(world)
            .any(|(cohort, id)| *id == band && cohort.faction == faction)
    };
    if !holds_band {
        return Err(format!(
            "Band {band_id} is not one of your people, so it cannot take on a road."
        ));
    }
    let ladder = world.resource::<LadderConfigHandle>().get();
    let Some(rung) = ladder.rung_for_verb(improvement) else {
        return Err(format!(
            "'{}' is not a build verb this ladder knows.",
            improvement.as_str()
        ));
    };
    crate::build_gates::validate_road_verb(world, faction, band, tile, rung)
}

fn build_refusal(
    channel: CommandEventKind,
    verb: &str,
    reason: impl Into<String>,
) -> CommandRefusal {
    CommandRefusal::Build {
        channel,
        verb: verb.to_string(),
        reason: reason.into(),
        log: RefusalLog::Build,
    }
}

/// **`grade <faction> <band> <x> <y>` / `pave ...` - THE ROUTE BRANCH'S TWO TILE VERBS.**
///
/// A road is a per-tile improvement with a **keeper** and no labor row, so issuing one **declares
/// the job** (a `BuildDeclaration` on that band's queue) and **names the keeper** - the same act.
/// The band's hex distance to the tile is quoted through [`crate::quote_road_remoteness`] and
/// **stamped on the road**, where it prices the build pile and the standing upkeep for the life of
/// the job. There is deliberately **no range refusal**.
fn apply_road_verb(
    world: &mut World,
    faction: FactionId,
    band_id: u64,
    tile: UVec2,
    improvement: VerbId,
) -> Result<BuildDeclared, CommandRefusal> {
    let verb = improvement.as_str();
    let road_refusal = |reason: String, log: RefusalLog| CommandRefusal::Build {
        channel: CommandEventKind::Road,
        verb: verb.to_string(),
        reason,
        log,
    };
    let ladder = world.resource::<LadderConfigHandle>().get();
    let Some(destination) = ladder.rung_for_verb(improvement).map(RungDef::rung_id) else {
        return Err(build_refusal(
            CommandEventKind::Road,
            verb,
            format!("'{verb}' is not a build verb this ladder knows."),
        ));
    };
    if let Err(reason) = road_verb_refusal(world, faction, band_id, tile, improvement) {
        return Err(road_refusal(reason, RefusalLog::Road { band_id, tile }));
    }

    // **The keeper and the remoteness quote, written together**, because the price is a fact about
    // the moment the band took the road on.
    let band = BandId(band_id);
    let Some((entity, band_tile)) = band_entity_and_tile(world, faction, band) else {
        return Err(road_refusal(
            format!("Band {band_id} is not standing anywhere the simulation can see."),
            RefusalLog::FeedOnly,
        ));
    };
    let (distance, remoteness) = crate::quote_road_remoteness(world, band_tile, tile, &ladder);
    {
        let mut roads = world.resource_mut::<RoadRegistry>();
        let Some(road) = roads.road_mut(tile) else {
            return Err(road_refusal(
                format!("There is no road at ({}, {}).", tile.x, tile.y),
                RefusalLog::FeedOnly,
            ));
        };
        road.take_keeper(RoadKeeper { faction, band }, remoteness, &ladder);
    }

    // **The declaration**, on that band's own queue. A road names no labor row, so this is the
    // band-addressed twin of `queue_build_on_working_bands`.
    {
        let mut allocation = crate::yield_seed::band_allocation_mut(world, entity);
        allocation.declare_build(BuildSource::Road(tile), BuildJob::Climb(destination));
    }
    crate::yield_seed::restaff_and_reseed(world, entity);
    Ok(BuildDeclared {
        channel: CommandEventKind::Road,
        verb: verb.to_string(),
        kind: DeclaredKind::Road {
            band_id,
            tile,
            distance,
            remoteness,
        },
    })
}

/// **`fell` / `coppice` / `quarry` - THE TWO DEPOSIT BRANCHES' THREE TILE VERBS.**
///
/// A working belongs to a camp as a patch does, so its keeper is whichever bands hold an `extract`
/// row on the source: the declaration goes on **every** band of the faction working it, through
/// [`queue_build_on_working_bands`]. The material is half the address (a hex can hold two
/// workings). The gates are [`validate_improvement`]'s.
fn apply_deposit_verb(
    world: &mut World,
    faction: FactionId,
    tile: UVec2,
    material: &str,
    improvement: VerbId,
) -> Result<BuildDeclared, CommandRefusal> {
    let verb = improvement.as_str();
    let target = LaborTarget::Extract {
        tile,
        material: material.to_string(),
        floor: SOURCE_NAMED_NOT_ASSIGNED,
    };
    let deposit_refusal = |reason: String, log: RefusalLog| CommandRefusal::Build {
        channel: CommandEventKind::Extraction,
        verb: verb.to_string(),
        reason,
        log,
    };
    if let Err(reason) = validate_improvement(world, faction, &target, improvement) {
        return Err(deposit_refusal(
            reason,
            RefusalLog::Deposit {
                tile,
                material: material.to_string(),
            },
        ));
    }
    let declared_rung = world
        .resource::<LadderConfigHandle>()
        .get()
        .rung_for_verb(improvement)
        .map(RungDef::rung_id);
    let Some(declared_rung) = declared_rung else {
        return Err(build_refusal(
            CommandEventKind::Extraction,
            verb,
            format!("'{verb}' is not a build verb this ladder knows."),
        ));
    };
    let declared =
        queue_build_on_working_bands(world, faction, &target, BuildJob::Climb(declared_rung));
    if declared == 0 {
        return Err(deposit_refusal(
            format!(
                "No band is working the {material} at ({}, {}). Put a crew on the deposit first, \
                 then {verb} it.",
                tile.x, tile.y
            ),
            RefusalLog::FeedOnly,
        ));
    }
    Ok(BuildDeclared {
        channel: CommandEventKind::Extraction,
        verb: verb.to_string(),
        kind: DeclaredKind::Deposit {
            tile,
            material: material.to_string(),
            bands: declared,
        },
    })
}

/// **DECLARE A BUILD - the one body behind `build <verb> ...`** (issue #175) and a scenario's
/// `build_rung` entry. The verb is resolved against the loaded ladder; **the rung's branch declares
/// the source kind**, and the kind - never the verb's name - decides what the request must carry:
///
/// | the verb's source kind | the request names | a band |
/// | --- | --- | --- |
/// | patch | a tile | refused: the keepers are the bands already foraging it |
/// | herd | a herd id, or the tile it stands on (a pen is a place) | refused, for the same reason |
/// | deposit | a tile **and** a material (one hex holds two workings) | refused, for the same reason |
/// | road | a tile | **required**: a road has no work row, so the keeper is said out loud |
///
/// Nothing is built outright: the declaration is declared on every band working the source and
/// raised by the builders staffing deals it. The gates are [`validate_improvement`]'s. Every
/// refusal and the declaration itself ride the **rung's** `feed_channel`; the caller writes the
/// log line and the feed event from the returned value.
pub fn apply_build_rung(
    world: &mut World,
    faction: FactionId,
    request: &BuildRungRequest,
) -> Result<BuildDeclared, CommandRefusal> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    let verb = request.verb.trim().to_ascii_lowercase();
    let Some(rung) = ladder.rung_for_verb(VerbId::new(&verb)) else {
        let known: Vec<&str> = ladder
            .branches_in_config_order()
            .into_iter()
            .flat_map(|branch| ladder.rungs_of(branch))
            .filter_map(|rung| rung.verb.as_deref())
            .collect();
        return Err(build_refusal(
            CommandEventKind::CancelOrder,
            &verb,
            format!(
                "There is no build verb '{verb}'. This ladder's verbs are: {}.",
                known.join(", ")
            ),
        ));
    };
    let channel = rung
        .feed_channel_kind()
        .expect("validate requires a feed channel on every built rung");
    let refuse = |reason: String| Err(build_refusal(channel, &verb, reason));
    if let Some(named) = request.branch.as_deref() {
        if BranchId::new(named) != rung.branch {
            return refuse(format!(
                "'{verb}' belongs to the {} branch, not '{named}'.",
                rung.branch.as_str()
            ));
        }
    }
    let Some(improvement) = rung.verb_id() else {
        return refuse(format!("The simulation has no builder for '{verb}' yet."));
    };
    let kind = ladder.source_kind(rung.branch);
    let tile = request
        .target_x
        .zip(request.target_y)
        .map(|(x, y)| UVec2::new(x, y));
    if request.band_id.is_some() && !matches!(kind, Some(SourceKind::Road | SourceKind::Band)) {
        return refuse(format!(
            "'{verb}' names no band: its keepers are the bands already working the source."
        ));
    }
    let target = match kind {
        Some(SourceKind::Patch) => match (tile, &request.herd_id, &request.material) {
            (Some(tile), None, None) => forage_source(tile),
            _ => {
                return refuse(format!(
                    "'{verb}' names a tile: `build {verb} <faction> <x> <y>`."
                ));
            }
        },
        Some(SourceKind::Herd) => {
            let herd_id = match (&request.herd_id, tile) {
                (Some(herd_id), None) => Some(herd_id.clone()),
                (None, Some(tile)) => world
                    .resource::<HerdRegistry>()
                    .herds
                    .iter()
                    .find(|herd| herd.position() == tile)
                    .map(|herd| herd.id.clone()),
                _ => None,
            };
            let Some(fauna_id) = herd_id else {
                return refuse(format!(
                    "'{verb}' names a herd, or the tile one stands on: \
                     `build {verb} <faction> <herd_id>` or `<x> <y>`."
                ));
            };
            LaborTarget::Hunt {
                fauna_id,
                floor: SOURCE_NAMED_NOT_ASSIGNED,
                move_with_herd: false,
            }
        }
        Some(SourceKind::Deposit) => {
            let (Some(tile), Some(material)) = (tile, request.material.as_deref()) else {
                return refuse(format!(
                    "'{verb}' names a tile AND a material, because one hex can hold two \
                     workings: `build {verb} <faction> <x> <y> <material>`."
                ));
            };
            return apply_deposit_verb(world, faction, tile, material, improvement);
        }
        Some(SourceKind::Road) => {
            let (Some(band_id), Some(tile)) = (request.band_id, tile) else {
                return refuse(format!(
                    "'{verb}' names the band that will keep the tile, because a road has no \
                     work row to read a keeper off: `build {verb} <faction> <band> <x> <y>`."
                ));
            };
            return apply_road_verb(world, faction, band_id, tile, improvement);
        }
        Some(SourceKind::Tile) => match tile {
            Some(tile) => LaborTarget::Tile {
                tile,
                branch: rung.branch,
            },
            None => {
                return refuse(format!(
                    "'{verb}' names a tile: `build {verb} <faction> <x> <y>`."
                ));
            }
        },
        // **A BAND IMPROVEMENT IS BUILT ON THE BAND ITSELF**: `build <verb> <faction> <band>`. The
        // text grammar reads a lone number as a herd id (`parse_build_rung_tail` cannot know which
        // verbs are band verbs), so the band is the explicit `band_id` or that lone number.
        Some(SourceKind::Band) => {
            let named = request.band_id.or_else(|| {
                request
                    .herd_id
                    .as_deref()
                    .and_then(|token| token.parse::<u64>().ok())
            });
            match (named, tile, &request.material) {
                (Some(band), None, None) => LaborTarget::Band {
                    band: BandId(band),
                    branch: rung.branch,
                },
                _ => {
                    return refuse(format!(
                        "'{verb}' names a band: `build {verb} <faction> <band>`."
                    ));
                }
            }
        }
        None => {
            return refuse(format!(
                "'{verb}' is not a verb this server can place on a source yet."
            ));
        }
    };

    let gate = match &target {
        LaborTarget::Forage { tile, .. } => {
            validate_forage_improvement_for_crews(world, faction, *tile, improvement)
        }
        _ => validate_improvement(world, faction, &target, improvement),
    };
    if let Err(reason) = gate {
        return refuse(reason);
    }
    let label = BuildSource::of(&target).map_or_else(
        || verb.clone(),
        |source| match source {
            BuildSource::Patch(tile) => format!("({}, {})", tile.x, tile.y),
            BuildSource::Herd(id) => id,
            BuildSource::Road(tile)
            | BuildSource::Deposit { tile, .. }
            | BuildSource::Tile { tile, .. } => {
                format!("({}, {})", tile.x, tile.y)
            }
            BuildSource::Band { band, branch } => {
                format!("Band {}'s {}", band.0, branch.as_str())
            }
        },
    );
    let switched =
        queue_build_on_working_bands(world, faction, &target, BuildJob::Climb(rung.rung_id()));
    if switched == 0 {
        let pen_taken = BuildSource::of(&target).is_some_and(|source| {
            is_pen_climb(world, &source, BuildJob::Climb(rung.rung_id()))
                && pen_queuer(world, &source).is_some()
        });
        return refuse(if pen_taken {
            PEN_TAKEN_HINT.to_string()
        } else {
            format!("No band of yours works {label}. Assign a crew to it first, then {verb} it.")
        });
    }
    Ok(BuildDeclared {
        channel,
        verb,
        kind: DeclaredKind::Source {
            label,
            bands: switched,
        },
    })
}

/// **A HERD COMMITTED ENTIRELY TO STANDING OUTPUT** - the top of `set_herd_output`'s accepted range,
/// where the herd is taken from not at all and rides at `K`. Named rather than a bare `1.0` because
/// it is a *state* the command's bound is stated in terms of, not a multiplier.
pub const WHOLE_HERD_COMMITTED: f32 = 1.0;

/// **What an extended pen leaves for the caller to say**: the herd whose pen is being widened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PenExtended {
    pub herd_id: String,
}

/// **What a recommitted herd leaves for the caller to say**: the herd being re-sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputRecommitted {
    pub herd_id: String,
}

fn corral_refusal(reason: impl Into<String>) -> CommandRefusal {
    CommandRefusal::Refused {
        kind: CommandEventKind::Corral,
        reason: reason.into(),
    }
}

/// The Hunt target a pen's keepers are matched by. `same_source` reads the herd id, so the floor is
/// irrelevant.
fn keeper_target(fauna_id: &str) -> LaborTarget {
    LaborTarget::Hunt {
        fauna_id: fauna_id.to_string(),
        floor: SOURCE_NAMED_NOT_ASSIGNED,
        move_with_herd: false,
    }
}

/// **A band of `faction` keeps `fauna_id`** (a Hunt assignment on it, any policy), or the ring or
/// the commitment's meter never accrues.
fn has_keeper(world: &mut World, faction: FactionId, fauna_id: &str) -> bool {
    let target = keeper_target(fauna_id);
    world
        .query::<(&PopulationCohort, &LaborAllocation)>()
        .iter(world)
        .filter(|(cohort, _)| cohort.faction == faction)
        .any(|(_, allocation)| allocation.workers_on(&target) > 0)
}

/// Grazing 2d-beta - the `ExtendPen` command. Put an owned, **built** pen at `tile` into the
/// "extending" state so its keeper band works off the next fenced ring (`pen_radius += 1` at
/// completion, ~25 turns at the corral build rate, with the harvest dipped to
/// `corralling_yield_fraction` while it fences). The pen's whole life rides
/// `CommandEventKind::Corral`, so the extend feed lines reuse it.
///
/// Validates: a herd penned **exactly at `tile`** (`corralled_at`, the fixed pen anchor - not the
/// roaming `position()` `corral` keys off), owned by `faction`, the faction knows **Penning** (a
/// ring rides the same `animal:pen` rung as the initial build, so it takes that rung's gate - not
/// Herding), `pen_radius` below `husbandry.pen_radius_max`, **no extension already in flight**, and
/// a band is keeping it (or the ring never accrues, and an untended pen escapes anyway).
///
/// Writes no log line and no feed event: the caller does, from the result.
pub fn apply_extend_pen(
    world: &mut World,
    faction: FactionId,
    tile: UVec2,
) -> Result<PenExtended, CommandRefusal> {
    let Some(fauna_id) = world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|herd| herd.corralled_at == Some(tile))
        .map(|herd| herd.id.clone())
    else {
        return Err(corral_refusal(format!(
            "No pen at ({}, {}) to extend.",
            tile.x, tile.y
        )));
    };

    let pen_radius_max = world
        .resource::<FaunaConfigHandle>()
        .get()
        .husbandry
        .pen_radius_max;
    // A fence ring rides the **same `animal:pen` rung** as the initial build (2d-beta: same labor,
    // same dip, same dials), so it takes that rung's gate too - **Penning** since the 4.3
    // reshuffle. Read off the rung, so a ring can never be gated differently from the pen it
    // extends.
    let (knowledge_threshold, pen_unlock) = {
        let ladder = world.resource::<LadderConfigHandle>().get();
        (
            ladder.knowledge.completion_threshold,
            ladder.enclosed_herd_rung().unlock_discovery_id(),
        )
    };
    let knows_penning = pen_unlock.is_none_or(|knowledge| {
        knows(
            world.resource::<DiscoveryProgressLedger>(),
            faction,
            knowledge,
            knowledge_threshold,
        )
    });
    let (owns, can_pen, species, at_max, already_extending) = {
        let herd = world
            .resource::<HerdRegistry>()
            .find(&fauna_id)
            .expect("herd resolved above");
        (
            herd.owner == Some(faction),
            herd.can_pen(&world.resource::<LadderConfigHandle>().get()),
            herd.species.clone(),
            herd.pen_radius >= pen_radius_max,
            herd.pen_extending,
        )
    };
    if !can_pen {
        // Grazing 2d-delta: belt-and-braces - a non-`Pen` species can never be penned, so this is
        // unreachable via the gated corral path, but the extend command states the rule explicitly.
        return Err(corral_refusal(format!("{species} cannot be penned.")));
    }
    if !knows_penning {
        return Err(corral_refusal(
            "Your people have not learned Penning yet. Tame and keep herds to learn it.",
        ));
    }
    if !owns {
        return Err(corral_refusal(format!(
            "You do not own the pen for {fauna_id}."
        )));
    }
    if already_extending {
        return Err(corral_refusal(format!(
            "The pen for {fauna_id} is already being extended."
        )));
    }
    if at_max {
        return Err(corral_refusal(format!(
            "The pen for {fauna_id} is already at its maximum size."
        )));
    }

    // A band must be keeping the pen (a Hunt assignment on it, any policy) or the ring never accrues.
    if !has_keeper(world, faction, &fauna_id) {
        return Err(corral_refusal(format!(
            "No band is keeping {fauna_id}. Assign herders to it first, then extend the pen."
        )));
    }

    // Enter the extending state - `begin_pen_extension` re-checks is_corralled / not-extending /
    // below-max, so the guard and the validation above can never disagree.
    let began = {
        let mut registry = world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|h| h.id == fauna_id)
            .expect("herd resolved above");
        herd.begin_pen_extension(pen_radius_max)
    };
    if !began {
        return Err(corral_refusal(format!(
            "Cannot extend the pen for {fauna_id} right now."
        )));
    }

    // **A RING IS A DECLARED BUILD LIKE EVERY OTHER BUILD** (`docs/plan_standing_upkeep.md` 2.5) - it
    // is fencing work on the same `animal:pen` rung as the pen it widens, so it waits its turn in
    // the same line and is funded from the same staffing. It is the one entry kind that names
    // no rung verb: a built pen carries no meter for the derived verb to name, which is exactly the
    // gap `BuildJob::ExtendPen` fills.
    queue_build_on_working_bands(
        world,
        faction,
        &keeper_target(&fauna_id),
        BuildJob::ExtendPen,
    );
    Ok(PenExtended { herd_id: fauna_id })
}

/// **The feed line for a committed `set_herd_output`**: the share as a whole percentage (`0.5` reads
/// `50%`), so a half share is not rounded to "0" or "1".
pub fn herd_output_feed_line(herd_id: &str, fraction: f32) -> String {
    let percent = (fraction * PERCENT_PER_UNIT).round();
    format!("Re-sorting {herd_id} to give {percent:.0}% of itself as milk, eggs and wool")
}

/// Percentage points in a whole (a `0..=1` share shown as a percentage).
const PERCENT_PER_UNIT: f32 = 100.0;

/// **COMMIT A KEPT HERD'S OUTPUT** - the `SetHerdOutput` command: the player's decision of what
/// share of a kept herd stands as milk, eggs and wool rather than being taken from. Priced as a
/// declared build on the herd's own rung, it mirrors [`apply_extend_pen`] step for step - the same
/// ownership check, the same keeper requirement, the same [`queue_build_on_working_bands`], the same
/// feed channel.
///
/// Validates: a herd with that id, owned by `faction`, **managed** (tamed or penned - a wild herd
/// stands on a rung with no build meter, so there is nothing to price the job against), a
/// `fraction` that is finite and inside `[0, 1]` and **different from the one the herd already
/// holds** (charging a rung's work for no change is the one thing the price must not buy), no
/// commitment already in flight, and a band keeping it (or the meter never accrues).
///
/// Writes no log line and no feed event: the caller does, from the result.
pub fn apply_set_herd_output(
    world: &mut World,
    faction: FactionId,
    herd_id: &str,
    fraction: f32,
) -> Result<OutputRecommitted, CommandRefusal> {
    let Some((fauna_id, owns, managed, already_committing, held)) =
        world.resource::<HerdRegistry>().find(herd_id).map(|herd| {
            (
                herd.id.clone(),
                herd.owner == Some(faction),
                herd.is_corralled() || herd.is_domesticated(),
                herd.standing_output_target.is_some(),
                herd.standing_output_fraction,
            )
        })
    else {
        return Err(corral_refusal(format!(
            "There is no herd {herd_id} to commit."
        )));
    };
    if !owns {
        return Err(corral_refusal(format!("You do not keep {fauna_id}.")));
    }
    if !managed {
        // A wild herd is not yours to re-sort, and its rung declares no build meter to price the
        // work against - two statements of the same fact, so one refusal covers both.
        return Err(corral_refusal(format!(
            "{fauna_id} is not a kept herd. Tame it before deciding what it produces."
        )));
    }
    if !fraction.is_finite() || !(NO_STANDING_COMMITMENT..=WHOLE_HERD_COMMITTED).contains(&fraction)
    {
        return Err(corral_refusal(format!(
            "A herd's standing share must be between 0 and 1, not {fraction}."
        )));
    }
    if fraction == held {
        return Err(corral_refusal(format!(
            "{fauna_id} already gives that share of itself as milk, eggs and wool."
        )));
    }
    if already_committing {
        return Err(corral_refusal(format!(
            "{fauna_id} is already being re-sorted."
        )));
    }

    // A band must be keeping the herd (a Hunt assignment on it, any policy) or the meter never
    // accrues - `extend_pen`'s own requirement, for its own reason.
    if !has_keeper(world, faction, &fauna_id) {
        return Err(corral_refusal(format!(
            "No band is keeping {fauna_id}. Assign herders to it first, then set its output."
        )));
    }

    // Enter the recommitting state - `begin_output_recommit` re-checks managed / not-in-flight /
    // in-range / actually-a-change, so the guard and the validation above can never disagree. The
    // herd's rung is read back afterwards so the declared build names the rung the order was priced
    // against.
    let (began, rung) = {
        let mut registry = world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|h| h.id == fauna_id)
            .expect("herd resolved above");
        (herd.begin_output_recommit(fraction), herd_rung_key(herd))
    };
    if !began {
        return Err(corral_refusal(format!(
            "Cannot change what {fauna_id} produces right now."
        )));
    }

    // **A COMMITMENT IS A DECLARED BUILD LIKE EVERY OTHER BUILD** - it is husbandry work on the rung
    // the herd already stands on, so it waits its turn in the same line and is funded from the same
    // build crew. It is the second entry kind that names no rung verb, and it carries its rung
    // because - unlike a ring, which is always fencing - it can be raised at either managed rung.
    queue_build_on_working_bands(
        world,
        faction,
        &keeper_target(&fauna_id),
        BuildJob::SetHerdOutput(rung),
    );
    Ok(OutputRecommitted { herd_id: fauna_id })
}

/// **What a switched auto-craft leaves for the caller to say**: the band it was set on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoCraftSet {
    pub id: Option<BandId>,
    pub label: String,
}

/// **TURN A BAND'S BENCH AUTO-CRAFT ON OR OFF** - the body of `bench_auto <faction> <band> on|off`
/// (`.claude/rules/core_sim/crafting.md` -> "Auto-craft"): the band is resolved among the faction's
/// own starting bands, the bench component is inserted on demand (as every bench verb does, so a band
/// spawned before the component existed can still switch it on), and
/// [`crate::auto_craft::set_auto_craft`] does the rest. **Never touches the crew.**
///
/// A refusal is the band lookup's own, on the `Craft` channel. Writes no log line and no feed event.
pub fn apply_set_auto_craft(
    world: &mut World,
    faction: FactionId,
    band: u64,
    enabled: bool,
) -> Result<AutoCraftSet, CommandRefusal> {
    let selected =
        crate::labor_commands::select_starting_band(world, faction, Some(band), "bench_auto")
            .map_err(|reason| CommandRefusal::Refused {
                kind: CommandEventKind::Craft,
                reason,
            })?;
    if world.get::<crate::BandBench>(selected.entity).is_none() {
        world
            .entity_mut(selected.entity)
            .insert(crate::BandBench::default());
    }
    crate::auto_craft::set_auto_craft(world, selected.entity, enabled);
    Ok(AutoCraftSet {
        id: selected.id,
        label: selected.label,
    })
}
