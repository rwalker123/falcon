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
use crate::components::{BandId, PopulationCohort};
use crate::grid_utils::hex_distance_wrapped;
use crate::orders::FactionId;
use crate::resources::{CommandEventKind, SimulationConfig};
use crate::starting_loadout::{
    apply_starting_loadout, KitAllocation, LoadoutRejection, LoadoutSupply, MaterialAllocation,
    StartingLoadout,
};
use crate::{
    BranchId, BuildJob, BuildSource, HerdRegistry, LaborAllocation, LaborTarget,
    LadderConfigHandle, RoadKeeper, RoadRegistry, RungDef, RungTrait, SourceKind, TakeSelection,
    Tile, TileRegistry, VerbId, DEFAULT_ESCAPEMENT_FLOOR,
};

/// **Why a replayed verb was refused.** `Display` is the reason text the event feed already shows
/// the player.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandRefusal {
    /// `SetStartingLoadout` was refused by the outfitting window's own checks.
    #[error("{0}")]
    StartingLoadout(#[from] LoadoutRejection),
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

/// **The placeholder floor a command's target carries when it names a source rather than assigns
/// one.** `LaborTarget::same_source` keys on the tile, the herd id and (on a deposit) the material
/// alone, so the floor on a target built to identify a source is matched by nothing and read by
/// nothing. It is [`DEFAULT_ESCAPEMENT_FLOOR`] so a reader who does look at it sees the sustainable
/// value, not a strip order.
pub const SOURCE_NAMED_NOT_ASSIGNED: f32 = DEFAULT_ESCAPEMENT_FLOOR;

/// How many bands may hold one herd's Corral in their queues at once - a herd cannot be in two
/// places.
const PEN_QUEUERS_PER_HERD: usize = 1;

/// **The forage source at `tile`, as the improvement commands name it.** `same_source` matches on
/// the tile alone, so the stance, crop and take-selection slots are placeholders - this exists so
/// the build verbs cannot *carry* a stance, a crop or a selection into
/// [`queue_build_on_working_bands`], and so "the command names a source, not an assignment" is
/// stated once.
pub fn forage_source(tile: UVec2) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor: SOURCE_NAMED_NOT_ASSIGNED,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
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
/// The band's hex distance to the tile is quoted through [`crate::remoteness_multiplier`] and
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
    let (width, wrap) = {
        let registry = world.resource::<TileRegistry>();
        let config = world.resource::<SimulationConfig>();
        (registry.width, config.map_topology.wrap_horizontal)
    };
    let distance = hex_distance_wrapped(band_tile, tile, width, wrap);
    let remoteness = crate::remoteness_multiplier(distance, &ladder);
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

/// **QUEUE A BUILD ON EVERY BAND OF `faction` ALREADY WORKING `target`'s SOURCE** (matched by
/// `LaborTarget::same_source`, so the tile / herd id). Returns how many bands were updated (`0` =
/// nobody is working that source, which the callers report as *"staff it first"*). The shared body
/// of the improvement verbs and of `extend_pen`.
///
/// # A VERB DECLARES; IT DOES NOT STAFF
///
/// It adds a [`crate::BuildDeclaration`] and restaffs the band: the sim puts builders on the build
/// from the band's idle hands, so this command names no crew, refuses nothing on affordability, and
/// cannot disband the gathering it was meant to improve.
///
/// # RE-ISSUING KEEPS THE ENTRY'S PLACE IN THE LINE
///
/// `LaborAllocation::declare_build` replaces `declared` in place, so correcting `cultivate` ->
/// `sow` on a declared source keeps its one declaration. Withdrawing one is `unqueue`.
///
/// **A verb reaches only bands already WORKING the source** - the rule that keeps an entry paired
/// with a row.
pub fn queue_build_on_working_bands(
    world: &mut World,
    faction: FactionId,
    target: &LaborTarget,
    declared: BuildJob,
) -> usize {
    let Some(source) = BuildSource::of(target) else {
        return 0;
    };
    // Each band's own assignment target, because the re-seed below must price the source under the
    // stance and crop that band actually holds - the command names neither.
    let mut query = world.query::<(Entity, &PopulationCohort, &LaborAllocation)>();
    let bands: Vec<(Entity, u32, LaborTarget)> = query
        .iter(world)
        .filter(|(_, cohort, _)| cohort.faction == faction)
        .filter_map(|(entity, _, allocation)| {
            allocation
                .assignments
                .iter()
                .find(|assignment| {
                    // **A staffed row is the band's holding at any crew** (#871): the sim staffs it
                    // from the floor, so a stand at its floor reads nobody on it while the band
                    // still works the source.
                    assignment.target.same_source(target)
                        && (assignment.workers > 0 || assignment.target.is_staffed())
                })
                .map(|assignment| (entity, assignment.workers, assignment.target.clone()))
        })
        .collect();
    let bands = if is_pen_climb(world, &source, declared) {
        bands_that_may_queue_the_pen(world, &source, bands)
    } else {
        bands
    };
    for (entity, workers, band_target) in &bands {
        {
            let mut allocation = crate::yield_seed::band_allocation_mut(world, *entity);
            allocation.declare_build(source.clone(), declared);
        }
        // **A queued Corral picks its pen's tile now** - the best-grazing land within this band's
        // work range, so the quotes price the tile the pen will really stand on.
        if let (BuildSource::Herd(id), BuildJob::Climb(rung)) = (&source, declared) {
            let ladder = world.resource::<LadderConfigHandle>().get();
            if ladder.rung(rung).trait_set().has(RungTrait::Enclosed) {
                choose_the_pen_site(world, *entity, id);
            }
        }
        // Queueing a build does not change the take at all, but the source's dip does depend on
        // what is being raised on it, so re-seed the telemetry from the new forecast.
        let improvement = crate::yield_seed::build_rung_on_source_at(world, *entity, band_target);
        crate::yield_seed::seed_source_yield(world, *entity, band_target, improvement, *workers);
        // A declared build is staffed by the sim, so the declaration restaffs the band: the new
        // build's builders show on the command's own frame.
        crate::yield_seed::restaff_and_reseed(world, *entity);
    }
    bands.len()
}

/// **IS `job` ON `source` A CORRAL?** - a climb onto an `enclosed` rung of a herd. The pen is the one
/// build a herd can hold in one place only, so it is the one the queue admits a single band for.
pub fn is_pen_climb(world: &World, source: &BuildSource, job: BuildJob) -> bool {
    let (BuildSource::Herd(_), BuildJob::Climb(rung)) = (source, job) else {
        return false;
    };
    world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(rung)
        .trait_set()
        .has(RungTrait::Enclosed)
}

/// **THE BAND, OF ANY FACTION, THAT HOLDS THIS HERD'S CORRAL IN ITS QUEUE** - `None` when no band
/// has one queued.
pub fn pen_queuer(world: &mut World, source: &BuildSource) -> Option<Entity> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    let mut query = world.query::<(Entity, &LaborAllocation)>();
    query
        .iter(world)
        .find(|(_, allocation)| {
            allocation
                .build_declaration(source)
                .and_then(|entry| entry.declared.climb())
                .is_some_and(|rung| ladder.rung(rung).trait_set().has(RungTrait::Enclosed))
        })
        .map(|(entity, _)| entity)
}

/// **A HERD CAN BE PENNED IN ONE PLACE ONLY**, so the Corral is queued on exactly one band: the one
/// that already holds it (a re-issue replaces its entry in place), else the lowest `BandId` of the
/// bands working the herd. Another band's hold - of any faction - leaves none, and the caller
/// refuses with [`PEN_TAKEN_HINT`].
fn bands_that_may_queue_the_pen(
    world: &mut World,
    source: &BuildSource,
    mut bands: Vec<(Entity, u32, LaborTarget)>,
) -> Vec<(Entity, u32, LaborTarget)> {
    match pen_queuer(world, source) {
        Some(holder) => bands.retain(|(entity, ..)| *entity == holder),
        None => {
            bands.sort_by_key(|(entity, ..)| world.get::<BandId>(*entity).copied());
            bands.truncate(PEN_QUEUERS_PER_HERD);
        }
    }
    bands
}

/// **STAMP THE TILE A QUEUED CORRAL'S PEN WILL STAND ON** - `fauna::pen_site_in_world` for the herd
/// as the keeping band at `band` sees it, written to the herd's `pen_site`.
fn choose_the_pen_site(world: &mut World, band: Entity, herd_id: &str) {
    let Some(band_tile) = world
        .get::<PopulationCohort>(band)
        .and_then(|cohort| world.get::<Tile>(cohort.current_tile))
        .map(|tile| tile.position)
    else {
        return;
    };
    let Some(herd) = world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .filter(|herd| !herd.is_corralled())
        .cloned()
    else {
        return;
    };
    let site = crate::pen_site_in_world(world, &herd, band_tile);
    if let Some(herd) = world
        .resource_mut::<HerdRegistry>()
        .herds
        .iter_mut()
        .find(|herd| herd.id == herd_id)
    {
        herd.pen_site = site;
    }
}
