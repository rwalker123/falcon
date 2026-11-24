//! **`assign_labor` and `work_priority`, as functions that RETURN their refusal** (`docs/plan_scenarios.md`
//! section 2.9), shared by the server's command handlers and the scenario loader's `labor` section.
//!
//! These bodies lived in `bin/server.rs` and reported a refusal only on the event feed. They take a
//! `World` and return `Result<(), CommandRefusal>`; the server's handler converts the wire, calls the
//! function and on `Err` writes the same feed line it always wrote (`CommandRefusal::Refused` carries
//! the feed channel and the reason). **One body, two callers**: nothing here is a copy of a handler.
//!
//! The helpers those two verbs need (band selection, the take-selection resolution, the build queue
//! the first-build declaration goes through, the event writers) live here with them; the server keeps
//! thin wrappers with its `&mut App` signatures for the handlers that still call them.

use std::borrow::Cow;
use std::sync::Arc;

use bevy::prelude::*;

use crate::build_gates::{needs_lesson_message, site_refusal_message};
use crate::command_seam::CommandRefusal;
use crate::yield_seed::{
    band_allocation_mut, build_rung_on_source_at, restaff_and_reseed, seed_source_yield,
};
use crate::*;

/// **What a queue verb says when it was handed no source** — one sentence for every verb that reads
/// a [`BuildSourceRef`], so the grammar it names cannot drift between them.
pub const SITE_SOURCE_GRAMMAR: &str = "two numbers name a tile, one token names a herd, \
     `<x> <y> <material>` names a working, `<branch> <x> <y>` (`route <x> <y>`) names a road \
     or a tile improvement and a band improvement (`rack`'s branch) is `<band> <branch>` (the band \
     is the command's own where the verb already names one, so `<branch>` alone)";

/// How many bands may hold one herd's Corral in their queues at once - a herd cannot be in two
/// places.
const PEN_QUEUERS_PER_HERD: usize = 1;

/// The refusal a labor verb returns: the feed channel it reports on and the reason the feed shows.
pub fn refused(kind: CommandEventKind, reason: impl Into<String>) -> CommandRefusal {
    CommandRefusal::Refused {
        kind,
        reason: reason.into(),
    }
}

/// **The parameters of one `assign_labor`**, every field the command's own (`CommandPayload::AssignLabor`
/// less the faction, the band and the retired `policy`).
#[derive(Debug, Clone, Default)]
pub struct AssignLaborOrder {
    pub role: String,
    pub workers: u32,
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    pub fauna_id: Option<String>,
    pub species: Option<String>,
    pub floor: Option<f32>,
    pub kit_id: Option<String>,
    pub take_species: Vec<String>,
    pub move_with_herd: bool,
}

/// **A band a command resolved to**, with the one spelling of it every event line uses.
///
/// ⛔ `label` is `Band {id}` ([`crate::band_label`]) and NEVER the unit's kind. The kind
/// (`BandScout`, ...) names what sort of unit a band was founded as, so every band of a people shares
/// it: two bands' craft lines both read "BandScout queued Baskets…". An event names a band the way
/// every other event does - by the durable [`BandId`] - and carries the id as a `band=` detail token
/// ([`push_band_event`]), which is what the client joins on to put the band's real name in the line.
pub struct SelectedBand {
    pub entity: Entity,
    /// The durable id; `None` only for a hand-built fixture band that never minted one.
    pub id: Option<BandId>,
    pub label: String,
}

/// The [`SelectedBand`] for `entity`.
pub fn selected_band(world: &World, entity: Entity) -> SelectedBand {
    SelectedBand {
        entity,
        id: world.get::<BandId>(entity).copied(),
        label: starting_unit_label(world, entity),
    }
}

/// **How an event line names a band** - `Band {id}` off its durable [`BandId`], never the unit's
/// kind (see [`SelectedBand`]). Only a band that never minted an id (a hand-built fixture) falls
/// back to the kind, there being no honest id to state.
pub fn starting_unit_label(world: &World, entity: Entity) -> String {
    if let Some(id) = world.get::<BandId>(entity).copied() {
        return crate::band_label(id);
    }
    world
        .get::<StartingUnit>(entity)
        .map(|unit| unit.kind.clone())
        .unwrap_or_else(|| format!("starting_unit:{}", entity.index()))
}

/// Append an entry to the world's command event log, if it has one.
pub fn push_command_entry(world: &mut World, entry: CommandEventEntry) {
    if let Some(mut log) = world.get_resource_mut::<CommandEventLog>() {
        log.push(entry);
    }
}

/// Push a feed line with no band attached.
pub fn push_command_event(
    world: &mut World,
    tick: u64,
    kind: CommandEventKind,
    faction: FactionId,
    label: String,
    detail: Option<String>,
) {
    push_command_entry(
        world,
        CommandEventEntry::new(tick, kind, faction, label, detail),
    );
}

/// **Push an event that is about ONE band**, so the line names it the way every other event does:
/// the entry carries the durable [`BandId`] ([`CommandEventEntry::with_band`]) and its detail a
/// `band=<id>` token - appended here unless the writer already stated one - which is what the client
/// re-labels the row by (`event-feed.md` -> "The `band=` token is what makes a loss line clickable").
/// A band with no durable id (a fixture) publishes its line unchanged and renders linkless.
pub fn push_band_event(
    world: &mut World,
    tick: u64,
    kind: CommandEventKind,
    faction: FactionId,
    band: Option<BandId>,
    label: String,
    detail: Option<String>,
) {
    let detail = match (band, detail) {
        (Some(id), Some(detail)) if !has_band_token(&detail) => {
            Some(format!("{detail} band={}", id.0))
        }
        (Some(id), None) => Some(format!("band={}", id.0)),
        (_, detail) => detail,
    };
    let mut entry = CommandEventEntry::new(tick, kind, faction, label, detail);
    if let Some(id) = band {
        entry = entry.with_band(id);
    }
    push_command_entry(world, entry);
}

/// Whether a detail already carries a whole `band=` token.
fn has_band_token(detail: &str) -> bool {
    detail.split(' ').any(|token| token.starts_with("band="))
}

/// **Resolve the band a command names to its entity**, or the reason it cannot. A band must exist, be
/// a commandable unit and belong to `faction`: a band id is a durable, guessable handle, so a
/// resolver matching on the id alone would hand a caller another faction's band.
pub fn resolve_starting_unit_entity(
    world: &mut World,
    faction: FactionId,
    band_id: u64,
    command_label: &str,
) -> Result<Entity, String> {
    let wanted = BandId(band_id);
    let found = {
        let mut query = world.query::<(Entity, &BandId)>();
        query
            .iter(world)
            .find(|(_, id)| **id == wanted)
            .map(|(entity, _)| entity)
    };
    let Some(entity) = found else {
        // Loud, never a silent no-op — this covers live commands as well as replayed ones.
        error!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            band_id,
            "command.band.rejected=no_such_band"
        );
        return Err(format!("Band {band_id} does not exist in the simulation."));
    };
    if world.get::<StartingUnit>(entity).is_none() {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            band_id,
            "command.band.rejected=not_a_starting_unit"
        );
        return Err(format!("Band {band_id} is not a commandable unit."));
    }
    let owner = world.get::<PopulationCohort>(entity).map(|c| c.faction);
    if owner != Some(faction) {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            band_id,
            "command.band.rejected=wrong_faction"
        );
        return Err(format!("Band {band_id} belongs to another faction."));
    }
    Ok(entity)
}

/// **The band a command is addressed to**: the one `band_id` names, or — with no id — the first
/// resident band of `faction` (the default-band picker, which only ever grabs a real band so a
/// band-less command never commandeers a detached expedition). The `Err` is the reason the feed shows.
pub fn select_starting_band(
    world: &mut World,
    faction: FactionId,
    band_id: Option<u64>,
    command_label: &str,
) -> Result<SelectedBand, String> {
    if let Some(bits) = band_id {
        let entity = resolve_starting_unit_entity(world, faction, bits, command_label)?;
        return Ok(selected_band(world, entity));
    }
    let mut query = world
        .query_filtered::<(Entity, &PopulationCohort), (With<StartingUnit>, With<ResidentBand>)>();
    let picked = query
        .iter(world)
        .find(|(_, cohort)| cohort.faction == faction)
        .map(|(entity, _)| entity);
    if let Some(entity) = picked {
        return Ok(selected_band(world, entity));
    }
    warn!(
        target: "shadow_scale::command",
        command = command_label,
        faction = %faction.0,
        "command.starting_unit.rejected=none_available"
    );
    Err("No available bands can accept this order right now.".to_string())
}

/// **Read the ladder for a queue verb, refusing a source whose `branch` it cannot read.** The
/// refusal is a typed feed line on the `CancelOrder` channel, the queue family's own.
pub fn source_ladder(
    world: &World,
    source: &BuildSourceRef,
) -> Result<Arc<LadderConfig>, CommandRefusal> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    match source.branch_refusal(&ladder) {
        Some(reason) => Err(refused(CommandEventKind::CancelOrder, reason)),
        None => Ok(ladder),
    }
}

/// **A band's head-count, read off the world for a command to clamp against.** The arithmetic lives on
/// [`BandWorkforce`] — the one authority, shared with the snapshot that publishes `idleWorkers`.
pub fn band_workforce(world: &World, band: Entity) -> BandWorkforce {
    BandWorkforce::resolve(
        world.get::<PopulationCohort>(band),
        world.get::<LaborAllocation>(band),
        world.get::<BandBench>(band),
    )
}

/// The plant rung's site refusal over a live world (`build_gates::plant_rung_site_refusal`).
fn plant_rung_site_refusal(
    world: &World,
    faction: FactionId,
    rung_key: RungId,
    tile: UVec2,
) -> Option<SiteRefusal> {
    crate::build_gates::plant_rung_site_refusal(world, faction, rung_key, tile)
}

/// The knowledge gate (`build_gates::knowledge_refusal`) over a live world.
fn knowledge_refusal(world: &World, faction: FactionId, rung: &crate::RungDef) -> Option<String> {
    crate::build_gates::with_scene(world, |scene| {
        crate::build_gates::knowledge_refusal(scene, faction, rung)
    })
}

/// **A SEAM THAT NEVER RENEWS HAS NO REGROWTH TO STAFF AGAINST, SO A HAND CREW NEEDS THE PLAYER'S
/// COUNT** (`staffing_preview::never_renewing_dig_without_a_count`).
fn never_renewing_dig_without_a_count(
    world: &World,
    target: &LaborTarget,
    workers: u32,
) -> Option<String> {
    crate::staffing_preview::never_renewing_dig_without_a_count(world, target, workers)
}

/// **What this row runs on when the player names no kit** (`staffing_preview::default_kit_for_target`).
fn default_kit_for_target(
    world: &World,
    equipment: &crate::EquipmentConfig,
    target: &LaborTarget,
) -> KitChoice {
    crate::staffing_preview::default_kit_for_target(world, equipment, target)
}

fn validate_improvement(
    world: &World,
    faction: FactionId,
    target: &LaborTarget,
    improvement: VerbId,
) -> Result<(), String> {
    crate::build_gates::validate_improvement(world, faction, target, improvement)
}

fn validate_species_selection(
    world: &World,
    tile: UVec2,
    species: Option<&str>,
    rung: RungId,
) -> Result<(), String> {
    crate::build_gates::validate_species_selection(world, tile, species, rung)
}

/// **The roles `assign_labor` accepts** — the ONE parser of the role token. The handler matches on
/// it, and the scenario file's pass 1 asks it, so the list is stated once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaborRole {
    Forage,
    Hunt,
    Scout,
    Warrior,
    Roadwork,
    Extract,
    Tile,
    Band,
}

impl LaborRole {
    /// The role a token names, case-insensitively, or `None` for a token no role answers to.
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token.to_ascii_lowercase().as_str() {
            "forage" => Self::Forage,
            "hunt" => Self::Hunt,
            "scout" => Self::Scout,
            "warrior" => Self::Warrior,
            "roadwork" => Self::Roadwork,
            "extract" => Self::Extract,
            "tile" => Self::Tile,
            "band" => Self::Band,
            _ => return None,
        })
    }
}

/// **The level a `work_priority` token names** — the ONE parser of it: the handler refuses `None` by
/// name, and the scenario file's pass 1 asks the same function.
pub fn parse_work_priority_level(level: &str) -> Option<SourcePriority> {
    SourcePriority::from_token(level.trim().to_ascii_lowercase().as_str())
}

/// The feed channel a labor command reports on, resolved from the **role token** rather than from a
/// built `LaborTarget` — the floor is validated before a target exists, and a rejection has to land
/// on the channel the player was looking at.
pub fn labor_event_kind(role: &str) -> CommandEventKind {
    match LaborRole::from_token(role) {
        Some(LaborRole::Forage) => CommandEventKind::Forage,
        Some(LaborRole::Hunt) => CommandEventKind::Hunt,
        Some(LaborRole::Scout) => CommandEventKind::Scout,
        Some(LaborRole::Extract) => CommandEventKind::Extraction,
        _ => CommandEventKind::CancelOrder,
    }
}

/// **WHAT AN ABSENT FLOOR TOKEN MEANS ON A DEPOSIT ROW** — [`DEFAULT_ESCAPEMENT_FLOOR`] where the
/// ground renews, [`crate::STRIP_IT_BARE`] where it never does (issue #650).
///
/// An escapement floor is a statement about **regrowth**: it names the stock a crew leaves standing
/// so that it comes back. A body at [`crate::NEVER_RENEWS`] has no regrowth for a floor to
/// protect, so *"leave half of it"* is not a policy anybody could hold there — and the client, which
/// offers the dial only where a deposit renews, sends no token on a quarry at all. Resolving that
/// silence to the shared `0.5` wrote a conservation choice onto a row where nobody made one, and
/// four separate readers believed it. **Absence on finite ground therefore means the honest zero**,
/// and a reader that reaches this row without asking about the rate is no longer misled by it.
///
/// ⛔ **THIS IS DEFENCE IN DEPTH, NOT A REPLACEMENT FOR THE FOUR GUARDS.**
/// [`crate::extraction::deposit_effective_floor`], [`crate::extraction::deposit_lesson_floor`]
/// and the client's `composed_floor` / `floor_mark` each keep their own *"does this deposit renew"*
/// condition, and each must: an **explicit** token is still stored exactly as sent on a finite
/// working (`components::LaborTarget::Extract::floor` — the grammar stays uniform across the three
/// webs on purpose), so the field can still be nonzero there and every reader has to stay
/// independently right about it. Do not simplify one away on the grounds that this now zeroes it.
///
/// ⛔ **A NAMED FLOOR IS NOT TOUCHED HERE.** This answers what *silence* means, nothing else — the
/// caller only reaches it when the command carried no token.
///
/// **The rate read is the GROUND's, un-scaled by the rung's `regrowth_multiplier`**, which is
/// [`crate::extraction::deposit_effective_floor`]'s own reading verbatim: a rung scales a rate,
/// it does not make the ground finite, so two places forking on one fact fork on one reading of it.
/// Ground the registry cannot resolve, or that holds none of this material, answers
/// [`crate::NEVER_RENEWS`] exactly as [`crate::extraction::tile_deposit_regrowth`] does — and
/// `validate_labor_policy` refuses that command anyway on any row with hands on it.
pub fn unnamed_deposit_floor(world: &World, tile: UVec2, material: &str) -> f32 {
    let extraction = world.resource::<ExtractionConfigHandle>().get();
    let regrowth_rate = world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .and_then(|entity| world.get::<Tile>(entity))
        .map(|ground| crate::extraction::tile_deposit_regrowth(&extraction, material, ground))
        .unwrap_or(crate::NEVER_RENEWS);
    if regrowth_rate > crate::NEVER_RENEWS {
        DEFAULT_ESCAPEMENT_FLOOR
    } else {
        crate::STRIP_IT_BARE
    }
}

/// Validate a labor target's **stance** against the source it names, returning a player-facing
/// rejection reason (`Err`) or `Ok`.
///
/// **Since issue #442 the stance axis has two rules, and neither is about a policy.**
/// - **Hunt:** `fauna::hunt_policies_for` prunes a [`crate::HuntYield::yields_nothing`] quarry —
///   worth neither meat nor pelt — down to `Eradicate` alone, because every other rung would be a
///   rate at which to collect nothing.
/// - **Forage:** the **crop the player named**, if they named one. `assign_labor` is the only
///   command that can *set* `LaborTarget::Forage::species`, so it is the only place a bad selection
///   can be caught at the moment it is made — and the only place that sees a re-selection dropped
///   onto a build already in flight.
///
/// Everything else this function used to check was about the **build verbs**, which are no longer
/// stances: the kind-exclusivity check (`Cultivate` on a herd) and the four per-rung gates moved to
/// [`validate_improvement`], where they belong to the axis that carries them. That split is what
/// makes a crew change a *stance-side* edit which never re-asserts a paused build's start gate
/// (`docs/plan_investment_rung_toggle.md` §6).
pub fn validate_labor_policy(
    world: &World,
    faction: FactionId,
    target: &LaborTarget,
) -> Result<(), String> {
    let _ = faction;
    match target {
        LaborTarget::Hunt {
            fauna_id,
            floor,
            move_with_herd,
        } => {
            // **MIGRATION MODE FOLLOWS A MIGRATORY HERD ONLY** (`docs/plan_roaming_bands.md`): a
            // resident herd never leaves its few tiles, so there is nothing to relocate for.
            if *move_with_herd {
                let migratory = world
                    .resource::<HerdRegistry>()
                    .find(fauna_id)
                    .is_some_and(|herd| herd.size_class == crate::SizeClass::Migratory);
                if !migratory {
                    return Err(
                        "Only a migratory herd can be followed — this one stays where it is."
                            .to_string(),
                    );
                }
            }
            // **A species that pays NOTHING may only be worked at floor `0`.** "Harvest it
            // sustainably" is meaningless for a quarry with no product: the only coherent reason to
            // put hunters on it is to remove it, so every floor that would leave some standing is
            // refused. The predicate is the species' own yield vector — the ONE seam this validator
            // shares with the snapshot's exported ladder (`fauna::hunt_policies_for`), so the picker
            // the client draws and the picker the sim accepts cannot drift into two lists.
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let species = world
                .resource::<HerdRegistry>()
                .find(fauna_id)
                .map(|herd| herd.species.clone());
            if let Some(species) = species {
                let denial_only = crate::species_requires_denial(fauna.hunt_yield_for(&species));
                if denial_only && *floor > crate::STRIP_IT_BARE {
                    return Err(format!(
                        "The {} yields neither food nor trade goods — the only thing to do with \
                         it is eradicate it.",
                        species
                    ));
                }
            }
            Ok(())
        }
        LaborTarget::Forage { tile, species, .. } => {
            // **CAN THEY GATHER HERE AT ALL?** The plant branch's rung 1 carries a
            // `site_requirement` of its own — the ground must be a **gathering site** — and this is
            // where it is enforced. It is the whole of the early game's scarcity: a `FoodModuleTag`
            // (and therefore a forage patch, and therefore a stand of named plants) sits on ~every
            // land tile, so without this rule a band gathers anywhere it stands and *which* ground
            // it can reach never matters.
            //
            // It was a CLIENT-SIDE rule until this arc — the tile card simply declined to offer the
            // compose sheet off-site — which meant the sim accepted a command no player could send,
            // the card advertised a stand nobody could work, and the rule lived nowhere the sim
            // could see it (issue #464).
            let ladder = world.resource::<LadderConfigHandle>().get();
            let root = ladder.kind_root(SourceKind::Patch).rung_id();
            if let Some(refusal) = plant_rung_site_refusal(world, faction, root, *tile) {
                // **GROUND A SOW COULD PLACE A SOURCE ON IS GROUND A CREW MAY BE SENT TO.** A sow is
                // queued on a tile a crew already works, so a tile that is not a gathering site but
                // that a higher placing rung admits for THIS faction (the Worked Land relaxation)
                // would otherwise have no way to receive its first crew. The tile is judged by the
                // highest placing rung: admitted, the crew is accepted; refused for want of a lesson,
                // the refusal names it; otherwise the root's ordinary refusal stands.
                let placing = ladder
                    .rungs_of(ladder.branch_of_kind(SourceKind::Patch))
                    .into_iter()
                    .rev()
                    .find(|rung| rung.places_source);
                let placing_refusal = placing.map(|rung| {
                    (
                        rung,
                        plant_rung_site_refusal(world, faction, rung.rung_id(), *tile),
                    )
                });
                match placing_refusal {
                    Some((rung, None)) => {
                        // **THE FARM'S OWN LESSON STILL GATES IT**: an off-site crew declares the
                        // sow with it, so it is judged by the same `unlock_knowledge` refusal every
                        // build verb is (`validate_patch_build`).
                        if let Some(refusal) = knowledge_refusal(world, faction, rung) {
                            return Err(refusal);
                        }
                        // **WORKED LAND OPENS FARMING, NOT GATHERING.** Off a gathering site the
                        // crew is a FARMING crew: it is accepted only where the patch already holds
                        // the placing rung or is being raised to it, or when it names the crop it is
                        // here to sow (`handle_assign_labor` then declares the sow with the crew, so
                        // the crew is a builder from its first turn). A bare gather is refused.
                        let farmed =
                            world
                                .resource::<ForageRegistry>()
                                .patch(*tile)
                                .is_some_and(|patch| {
                                    let standing = patch.standing();
                                    standing.holds_at_or_above(rung)
                                        || standing.raising == Some(rung.rung_id())
                                });
                        if !farmed && species.is_none() {
                            return Err(format!(
                                "({}, {}) is not a gathering site. What you have learned lets you \
                                 farm there, not gather: name the crop to {} and the crew will raise \
                                 a farm.",
                                tile.x,
                                tile.y,
                                rung.verb.as_deref().unwrap_or("sow")
                            ));
                        }
                    }
                    Some((rung, Some(SiteRefusal::NeedsLesson))) => {
                        let verb = rung.verb.as_deref().unwrap_or("gather");
                        let lesson = crate::build_gates::plant_rung_nearest_lesson(
                            world,
                            faction,
                            rung.rung_id(),
                            *tile,
                        );
                        return Err(needs_lesson_message(
                            &ladder,
                            lesson.as_deref(),
                            *tile,
                            verb,
                        ));
                    }
                    _ => return Err(site_refusal_message(refusal, *tile, "gather")),
                }
            }
            // **Judge the crop the player NAMED, and only then.** Absent means "pick the tile's
            // dominant legal plant for me", which cannot be wrong and — crucially — must not drag
            // an ordinary wild gather into the ladder's refusals: ground whose whole basket is
            // `wild` is perfectly gatherable, and rejecting `assign_labor` there would make the
            // open-water fisheries and the alpine peaks unworkable.
            //
            // **At `PlantTended`, the ladder's ENTRY rung.** `cultivation_ceiling` is a ladder
            // (`allows_sow` implies `allows_cultivate`), so tended is the weaker of the two gates:
            // judging at `PlantField` here would refuse a crop the player may legitimately intend
            // to `cultivate`, and the stance command does not yet know which verb will follow.
            // `handle_sow` re-judges the crew's crop at its own rung.
            // **WHICH PLANTS ARE THEY HERE FOR? — NOT ASKED HERE.** The take selection is
            // *resolved* rather than gated, because most of what is wrong with one is the ground
            // having moved under it rather than the player having mistyped it — see
            // [`resolve_take_for_ground`], which `handle_assign_labor` runs immediately after this
            // and which prunes the stale names instead of refusing the command that carries them.
            //
            // It lives outside this gate because this function only ever answers *may they*, and a
            // repair is not a verdict: threading a rewritten selection back out of a
            // `Result<(), String>` would make every other arm carry a value it has no opinion about.
            let Some(named) = species.as_deref() else {
                return Ok(());
            };
            validate_species_selection(
                world,
                *tile,
                Some(named),
                ladder.managed_patch_rung().rung_id(),
            )
        }
        // **IS THERE ANYTHING HERE TO WORK?** — the deposit branches' one stance rule, and it is
        // `absence is the answer` enforced at the command: a terrain carrying no row in
        // `extraction.json`'s `by_terrain` holds none of that material, so there is no working to
        // open and no crew to put on it. That is *also* what refuses the free floor on bare ground,
        // which is why neither rung 1 carries a `site_requirement` (a floor of ~0 admits every tile
        // and reads as a placement rule while being none).
        LaborTarget::Extract { tile, material, .. } => {
            let extraction = world.resource::<ExtractionConfigHandle>().get();
            if extraction.deposit(material).is_none() {
                return Err(format!(
                    "'{material}' is not something that comes out of the ground. Your people can \
                     work wood and stone."
                ));
            }
            let capacity = world
                .resource::<TileRegistry>()
                .index(tile.x, tile.y)
                .and_then(|entity| world.get::<Tile>(entity))
                .map(|ground| {
                    crate::extraction::tile_deposit_capacity(&extraction, material, ground)
                })
                .unwrap_or(crate::NO_DEPOSIT);
            if capacity <= crate::NO_DEPOSIT {
                return Err(format!(
                    "There is no {material} at ({}, {}) — that ground holds none. Move to ground \
                     that does.",
                    tile.x, tile.y
                ));
            }
            Ok(())
        }
        // **A tile improvement's crew has no stance to validate** - no floor, species or herd. What
        // `assign_labor` checks for it (a tile-kind branch, not another people's) is its own arm.
        LaborTarget::Tile { .. } | LaborTarget::Band { .. } => Ok(()),
        LaborTarget::Scout | LaborTarget::Warrior | LaborTarget::Roadwork => Ok(()),
    }
}

/// **THE TAKE SELECTION THIS COMMAND WILL STORE, and the names the ground has taken off it** — the
/// selection resolved against the mix the take will actually narrow.
///
/// `selection` is what the row must carry; `dropped` is the display names pruned out of it, empty
/// when the player's selection stood as given. The caller stores the one and announces the other.
pub struct ResolvedTake {
    selection: TakeSelection,
    dropped: Vec<String>,
}

/// **What may this crew gather HERE, given what it asked for?** — the take selection's command-side
/// resolution, phrased for the player.
///
/// It resolves through the *same* `forage::resolve_take_selection` seam the take path narrows with,
/// against **the mix that take will narrow** — `forage::patch_composition`, the patch's live
/// rung-reweighted basket, falling back to the tile's own realization
/// (`forage::tile_flora_composition`, never `FloraConfig::composition` on a raw terrain) where no
/// patch stands yet. **There is no rung *asked* in it**: a take selection says what the crew carries
/// home from the stand that is standing, so a `wild`-ceiling plant is a perfectly good answer — but
/// the stand that is standing on a tended or sown patch is the reweighted one, not the wild
/// realization it grew out of.
///
/// **Judging the wild basket was a defect, not a simplification.** The two baskets differ on every
/// committed patch, so a selection naming a plant weeding or sowing had already displaced was
/// accepted at the command boundary — freshly typed, no staleness involved — and then valued at
/// exactly zero by the very next turn's take.
///
/// > #### ⛔ AN ABSENT PLANT IS PRUNED. ONLY A PLANT NO ROSTER CARRIES IS REFUSED.
/// >
/// > Judging the patch's own mix is right; **hard-refusing on it was not.** The mix moves under a
/// > stored selection — that is what a `Cultivate`/`Sow` *is* — so the names this finds absent are
/// > typically ones that were legal when the player made them and that the player's own crop then
/// > weeded out.
/// >
/// > Refusing them refused the **whole `assign_labor`**, worker count included: reported from play on
/// > a Field at `Wild Emmer 100%` whose row still named Wild Pulses, where raising the tenders did
/// > nothing at all and the feed said only *"Harvest failed — Wild Pulses does not grow at (13,
/// > 10)"*. There was no way out of it from the panel either — a chip is only drawn for a plant the
/// > **current** mix carries, so the stale key had no control to clear it with.
/// >
/// > So the absent case **prunes** ([`TakeSelection::pruned_to`], the same narrowing the turn's own
/// > commitment repair runs) and the command lands. An **unknown** key is still refused by name: that
/// > is a typo, nothing can be inferred from it, and the player should be told.
pub fn resolve_take_for_ground(
    world: &World,
    tile: UVec2,
    take: &TakeSelection,
) -> Result<ResolvedTake, String> {
    let unchanged = || ResolvedTake {
        selection: take.clone(),
        dropped: Vec::new(),
    };
    if take.is_everything() {
        return Ok(unchanged());
    }
    // **No map, nothing to judge** — the same carve-out `validate_species_selection` makes, and for
    // the same reason: the command-unit harnesses and the idle boot carry no `TileRegistry`, and the
    // labor arm (which always has the real tiles) remains the authority.
    let Some(registry) = world.get_resource::<TileRegistry>() else {
        return Ok(unchanged());
    };
    let Some(ground) = registry
        .index(tile.x, tile.y)
        .and_then(|entity| world.get::<Tile>(entity))
    else {
        return Err(format!("There is no tile at ({}, {}).", tile.x, tile.y));
    };
    let labor = world.resource::<LaborConfigHandle>().get();
    let flora = world.resource::<FloraConfigHandle>().get();
    let map_seed = world.resource::<SimulationConfig>().map_seed;
    let tile_composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    // The patch's own mix where one stands — the same seam `forage_take` narrows against.
    let registry = world.resource::<ForageRegistry>();
    let composition = registry.patch(tile).map_or_else(
        || Cow::Borrowed(tile_composition.as_ref()),
        |patch| patch_composition(patch, &tile_composition, &flora, &labor.forage),
    );
    let absent = match resolve_take_selection(take, &composition, &flora) {
        Ok(absent) => absent,
        Err(species) => return Err(format!("Your people know no plant called '{species}'.")),
    };
    if absent.is_empty() {
        return Ok(unchanged());
    }
    let dropped = absent
        .iter()
        .map(|species| {
            flora
                .species
                .get(*species)
                .map_or_else(|| (*species).to_string(), |def| def.display_name.clone())
        })
        .collect();
    Ok(ResolvedTake {
        selection: take.pruned_to(|species| species_stands_in(&composition, species)),
        dropped,
    })
}

/// **THE RUNG THAT PLACES A SOURCE ON `tile` WHEN THE GATHERING RULE REFUSES IT** - `Some` only for
/// ground the root rung's site rule refuses for this faction and the highest placing rung (the
/// Farm, through its Worked Land relaxation) admits: off-site farming ground.
pub fn off_site_placing_rung(world: &World, faction: FactionId, tile: UVec2) -> Option<RungId> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    let root = ladder.kind_root(SourceKind::Patch).rung_id();
    plant_rung_site_refusal(world, faction, root, tile)?;
    let placing = ladder
        .rungs_of(ladder.branch_of_kind(SourceKind::Patch))
        .into_iter()
        .rev()
        .find(|rung| rung.places_source)?;
    plant_rung_site_refusal(world, faction, placing.rung_id(), tile)
        .is_none()
        .then(|| placing.rung_id())
}

/// **A MATERIAL A LEADER KEEPS ON HAND IS NOT CREWED BY HAND** (#874): the refusal `assign_labor
/// extract`, `stop_labor` and `work_priority` give for a row on a material whose rows belong to the
/// band's keep-on-hand order. `None` for every other target and for a band that crews by hand.
pub fn kept_on_hand_refusal(
    world: &World,
    band: &SelectedBand,
    target: &LaborTarget,
) -> Option<String> {
    let allocation = world.get::<LaborAllocation>(band.entity)?;
    let keeper = band_keeper(world, band);
    // A forage, hunt or dig row the chief lists under an order is his; any other row is the player's.
    let kind = allocation.row_order(target)?;
    crate::keep_on_hand::order_refusal(allocation, kind, keeper.as_ref())
}

/// **ONE BAND OF A PEOPLE PER TAKE SOURCE**: the refusal for a new row on a source another band of
/// the band's people already works, naming that band (its `BandName`, else `Band <id>`). `None` for a
/// row the band already holds (editing it is never refused), for a target that is no take source and
/// where no other band of the people holds one.
pub fn another_band_refusal(world: &World, target: &LaborTarget, band: Entity) -> Option<String> {
    let other = crate::source_holders::other_band_working(world, band, target)?;
    let mut bands = world.iter_entities();
    let other_entity = bands.find(|entity| entity.get::<BandId>() == Some(&other))?;
    let name = other_entity
        .get::<BandName>()
        .map_or_else(|| crate::band_label(other), |name| name.0.clone());
    Some(match target {
        LaborTarget::Forage { tile, .. } => {
            format!("{name} already works ({}, {}).", tile.x, tile.y)
        }
        LaborTarget::Hunt { fauna_id, .. } => {
            let herd = world
                .resource::<HerdRegistry>()
                .find(fauna_id)
                .map_or_else(|| fauna_id.clone(), |herd| herd.species.clone());
            format!("{name} already hunts the {herd}.")
        }
        LaborTarget::Extract { tile, material, .. } => {
            format!(
                "{name} already works the {material} at ({}, {}).",
                tile.x, tile.y
            )
        }
        _ => return None,
    })
}

/// **WHO KEEPS THIS BAND'S ORDERS** - its chief by name, or its elders named for the band. `None`
/// for a band with no leader. The band's own name is used where it has one, else its label.
pub fn band_keeper(world: &World, band: &SelectedBand) -> Option<crate::keep_on_hand::Keeper> {
    let name = world
        .get::<BandName>(band.entity)
        .map_or_else(|| band.label.clone(), |name| name.0.clone());
    crate::keep_on_hand::Keeper::of(world.get::<crate::BandLeadership>(band.entity), &name)
}

/// Set the worker count for one labor target on a band (idempotent; `0` unassigns; clamps to the
/// band's free working-age headroom). Text forms:
///   `assign_labor <faction> <band> forage <x> <y> [floor] [species] <workers>`
///   `assign_labor <faction> <band> hunt <herd_id> [floor] <workers>`
///   `assign_labor <faction> <band> extract <x> <y> <material> [floor] <workers>`
///   `assign_labor <faction> <band> scout <tiles> [kit <k>]` (tiles past the local range; `0` stops)
///   `assign_labor <faction> <band> warrior <workers>`
///
/// `floor` is where the crew stops, as a fraction of the source's carrying capacity — **all three
/// worked rows carry one**, validated once at the top of this function and rejected rather than
/// clamped. A deposit's is composed with its *rung's* own floor as a maximum
/// (`extraction::deposit_effective_floor`), which is the one thing about it that differs from a
/// patch's.
///
/// It **never accepts a build verb**: an improvement is set by its own command
/// (`cultivate`/`sow`/`tame`/`corral`/`fell`/`coppice`/`quarry`) and **is never touched by this one**
/// (`docs/plan_investment_rung_toggle.md` §5), which is what makes a paused build's crew editable.
///
/// **A refusal is RETURNED** as [`CommandRefusal::Refused`] (the feed channel and the reason); the
/// server's handler writes the feed line from it, the scenario loader stops the load on it.
/// `band_id` `None` is the default-band picker (the first resident band).
pub fn apply_assign_labor(
    world: &mut World,
    faction: FactionId,
    band_id: Option<u64>,
    order: AssignLaborOrder,
) -> Result<(), CommandRefusal> {
    let AssignLaborOrder {
        role,
        workers,
        target_x,
        target_y,
        fauna_id,
        species,
        floor,
        kit_id,
        take_species,
        move_with_herd,
    } = order;
    // **DID THE PLAYER NAME A FLOOR AT ALL** — kept before the line below resolves absence to the
    // default, because the `extract` arm answers *absence* differently on ground that never renews
    // (issue #650, [`unnamed_deposit_floor`]). Every other row reads only the resolved value.
    let player_named_a_floor = floor.is_some();
    // **The floor FAILS CLOSED** (`docs/plan_harvest_floor.md` §4): absent means the default, but a
    // value outside `0.0..=1.0` is rejected with its own failure event rather than clamped. A clamp
    // would turn a typo into a quiet policy change on the one number the whole harvest model turns
    // on — the `cancel_order` scope precedent.
    let floor = match floor {
        None => DEFAULT_ESCAPEMENT_FLOOR,
        Some(value) if floor_is_valid(value) => value,
        Some(value) => {
            return Err(refused(labor_event_kind(&role), format!(
                    "assign_labor floor must be a fraction of carrying capacity in 0.0..=1.0; got {value}."
                )));
        }
    };
    let mut target = match LaborRole::from_token(&role) {
        Some(LaborRole::Forage) => match (target_x, target_y) {
            (Some(x), Some(y)) => LaborTarget::Forage {
                tile: UVec2::new(x, y),
                floor,
                // The optional species selection (Flora Roster S1): which named plant a
                // `Cultivate`/`Sow` here should commit the patch to. Absent/blank = "pick the tile's
                // dominant legal plant for me", the same absent-means-default convention the floor has.
                species: species
                    .as_deref()
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .map(str::to_string),
                // **WHICH PLANTS THIS CREW CARRIES HOME** (the selective gather) — sorted and
                // deduplicated by the constructor, so the order the player typed cannot reach the
                // snapshot. Empty = the whole basket, the same absent-means-default convention the
                // floor and the commit species have. Its legality is judged below, against this
                // tile's own basket, and **fails closed**.
                take_species: TakeSelection::from_keys(&take_species),
            },
            _ => {
                return Err(refused(
                    CommandEventKind::Forage,
                    "assign_labor forage requires <x> <y>.".to_string(),
                ));
            }
        },
        Some(LaborRole::Hunt) => match fauna_id {
            Some(id) if !id.trim().is_empty() => LaborTarget::Hunt {
                fauna_id: id,
                floor,
                move_with_herd,
            },
            _ => {
                return Err(refused(
                    CommandEventKind::Hunt,
                    "assign_labor hunt requires <herd_id>.".to_string(),
                ));
            }
        },
        Some(LaborRole::Scout) => LaborTarget::Scout,
        Some(LaborRole::Warrior) => LaborTarget::Warrior,
        // **The road-keeping role** (`docs/plan_standing_upkeep.md` §4.13) — the one keeping POOL
        // left (`docs/plan_site_crews.md` §1: a site's own crew keeps it). What its hands hold is not
        // a source row but the road TILES THIS BAND IS THE KEEPER OF — the ones it graded or paved,
        // wherever the band has since walked (`routes` rule 2; the catchment is the keeper, never the
        // band's own position) — and `0` stops keeping roads at all.
        Some(LaborRole::Roadwork) => LaborTarget::Roadwork,
        // **THE TWO DEPOSIT BRANCHES' TAKE ROW** (`docs/plan_extraction.md` §6) — the only row in
        // the game that pays no food, and the whole of what wood and stone cost: every hand here
        // comes out of the same pool a Forage row spends.
        //
        // **It names a TILE AND A MATERIAL**, because one tile can hold two deposits and working
        // the timber is not working the rock. The material rides the `species` token, which is the
        // one free-form string this command already carries for a Forage row and means the same
        // kind of thing there — *which of the things on this ground are you here for*.
        Some(LaborRole::Extract) => match (target_x, target_y, species.as_deref().map(str::trim)) {
            (Some(x), Some(y), Some(material)) if !material.is_empty() => LaborTarget::Extract {
                tile: UVec2::new(x, y),
                material: material.to_string(),
                // **The same validated floor a Forage or Hunt row gets when the player NAMED one** —
                // struck once at the top of this function, so all three webs fail closed on the
                // identical bound rather than three times over — and [`unnamed_deposit_floor`] when
                // they named none, because on this web alone *absence* depends on the ground.
                floor: if player_named_a_floor {
                    floor
                } else {
                    unnamed_deposit_floor(world, UVec2::new(x, y), material)
                },
            },
            _ => {
                return Err(refused(
                    CommandEventKind::Extraction,
                    "assign_labor extract requires <x> <y> and a material (wood, stone)."
                        .to_string(),
                ));
            }
        },
        // **A TILE IMPROVEMENT'S CREW** - the deposit row's twin for a source that yields nothing.
        // It names a TILE AND A BRANCH (one tile can carry improvements of several tile branches);
        // the branch rides the `species` token, as a deposit's material does. The crew keeps the
        // improvement first, and distance is nothing special: a far crew is a work party.
        Some(LaborRole::Tile) => match (target_x, target_y, species.as_deref().map(str::trim)) {
            (Some(x), Some(y), Some(branch)) if !branch.is_empty() => {
                let ladder = world.resource::<LadderConfigHandle>().get();
                let branch = BranchId::new(branch);
                if !crate::is_tile_branch(&ladder, branch) {
                    return Err(refused(
                        CommandEventKind::CancelOrder,
                        format!("'{branch}' is not a tile improvement branch this ladder knows."),
                    ));
                }
                LaborTarget::Tile {
                    tile: UVec2::new(x, y),
                    branch,
                }
            }
            _ => {
                return Err(refused(
                    CommandEventKind::CancelOrder,
                    "assign_labor tile requires <branch> <x> <y>.".to_string(),
                ));
            }
        },
        // **A BAND IMPROVEMENT'S CREW** - `band <branch> <workers>`: the tile row's twin for an
        // improvement the band holds at its camp. It names the commanded BAND (the improvement is
        // keyed by it and carried with it) and the ladder branch, which rides the `species` token as
        // a tile row's does.
        Some(LaborRole::Band) => match (band_id, species.as_deref().map(str::trim)) {
            (Some(band), Some(branch)) if !branch.is_empty() => {
                let ladder = world.resource::<LadderConfigHandle>().get();
                let branch = BranchId::new(branch);
                if !crate::is_band_branch(&ladder, branch) {
                    return Err(refused(
                        CommandEventKind::CancelOrder,
                        format!("'{branch}' is not a band improvement branch this ladder knows."),
                    ));
                }
                LaborTarget::Band {
                    band: BandId(band),
                    branch,
                }
            }
            _ => {
                return Err(refused(
                    CommandEventKind::CancelOrder,
                    "assign_labor band requires a band and a <branch>.".to_string(),
                ));
            }
        },
        None => {
            return Err(refused(
                CommandEventKind::CancelOrder,
                format!("Unknown labor role '{}'.", role.to_ascii_lowercase()),
            ));
        }
    };

    let event_kind = match &target {
        LaborTarget::Forage { .. } => CommandEventKind::Forage,
        LaborTarget::Hunt { .. } => CommandEventKind::Hunt,
        LaborTarget::Scout => CommandEventKind::Scout,
        LaborTarget::Warrior => CommandEventKind::CancelOrder,
        // **The road keepers have no web's channel to ride**, because the route branch declares no
        // verb at all — traffic is the crew — so they report on the generic one, as the builders and
        // the warriors do.
        LaborTarget::Roadwork => CommandEventKind::CancelOrder,
        LaborTarget::Extract { .. } => CommandEventKind::Extraction,
        // A tile or band improvement has no web's channel; it reports on the generic one, as the
        // pools do.
        LaborTarget::Tile { .. } | LaborTarget::Band { .. } => CommandEventKind::CancelOrder,
    };

    // **THE SCOUT ROW IS A RANGE, NOT A HEAD COUNT** (#875): the count slot names the tiles past the
    // band's local range the player wants to see, and the sim staffs the scouts that reach it.
    if matches!(target, LaborTarget::Scout) {
        return apply_scouting_range(world, faction, band_id, workers, kit_id, event_kind);
    }

    // Stance validation. Unassigning (`workers == 0`) is always allowed — a player must be able to
    // abandon an investment even if its gates have since lapsed. **The improvement's gates are NOT
    // re-run here** (issue #442): this command does not set an improvement, so re-asserting one the
    // band already carries would refuse a crew change on a paused build — the trap §6 removes.
    //
    // **THE TAKE SELECTION IS REPAIRED HERE, NOT REFUSED** — see [`resolve_take_for_ground`]. It
    // rides the same `workers > 0` guard as the gate above and for the same reason: an unassign must
    // never be refused, and a row going to zero gathers nothing for a selection to be wrong about.
    // The names it drops are announced once, beside the applied event at the foot of this function,
    // because a pruned selection is a change the player did not ask for.
    let mut pruned_take: Vec<String> = Vec::new();
    // **A NEED ROW IS VALIDATED AT ZERO TOO**: it holds no standing workers whatever count is sent,
    // so `workers == 0` is its ordinary shape and the migratory check must still run.
    if workers > 0 || target.is_need_row() || target.is_staffed() {
        if let Err(reason) = validate_labor_policy(world, faction, &target) {
            return Err(refused(event_kind, reason));
        }
        if let Some(reason) = never_renewing_dig_without_a_count(world, &target, workers) {
            return Err(refused(event_kind, reason));
        }
        if let LaborTarget::Forage {
            tile, take_species, ..
        } = &target
        {
            match resolve_take_for_ground(world, *tile, take_species) {
                Err(reason) => {
                    return Err(refused(event_kind, reason));
                }
                Ok(resolved) => {
                    if !resolved.dropped.is_empty() {
                        pruned_take = resolved.dropped;
                        if let LaborTarget::Forage { take_species, .. } = &mut target {
                            *take_species = resolved.selection;
                        }
                    }
                }
            }
        }
    }

    // **The kit this crew works under, resolved at the command boundary and FAILING CLOSED.** An
    // unknown id, or one whose `jobs` does not cover this role, is refused with a reason rather than
    // quietly becoming the default: naming a kit is how the player compares tiers, so a silent
    // substitution answers a different question than the one asked. **The band-wide roles resolve
    // one too** — `kit_job()` answers for all four roles now, so `assign_labor … scout 3 kit none`
    // is a real selection rather than a token ignored the way `species` and `floor` are on those
    // rows.
    //
    // **Unassigning (`workers == 0`) resolves NO kit**, the same rule the policy validation above
    // follows and for the same reason: a player must be able to abandon an investment even if what
    // it was staffed with has since lapsed. `LaborAllocation::set_assignment` *drops* the assignment
    // at zero workers and never reads the kit, so refusing here refused a command whose kit could
    // not be used either way — and a roster edit that removed an id left every crew still holding it
    // unclearable, locked in by a kit that no longer exists.
    //
    // **A HUNT row with no kit named resolves the HERD's default, not the job's.** The wire
    // publishes that per-herd id (`HerdTelemetryState::default_kit_id`) and the compose sheet opens
    // on it, so resolving the job default here would run Stalking on a warren whose sheet said
    // Trapping — the silent substitution the refusal above exists to prevent, arriving through the
    // absent-token door instead. `default_kits.hunt` stays the answer wherever there is no quarry
    // to score: every other role, and a Hunt row whose herd or species will not resolve.
    //
    // ⛔ **A STANDING POOL CARRIES NO KIT AT ALL, AND NAMING ONE IS REFUSED**
    // (`docs/plan_pool_toe.md` §4). A pool — `builders` or `roadwork` — says HOW MANY hands; the
    // tools they carry follow from the rung each of its sites stands on and are settled band-wide
    // by the player's marks. A kit stored on the row would reach nothing, so accepting the token
    // would be the worst version of the defect: the player names a tool, the sim stores it, and
    // nobody anywhere picks it up. **Refused rather than ignored**, by name.
    //
    // The fork is **here and not in `default_kit_for_target`**, because the question it answers is
    // *"what does this command STORE"*, not *"which kit is the absent one"*: that helper returns a
    // resolved `KitChoice` for the raid path too.
    // **A STAFFED ROW IS NEVER UNSTAFFED BY A COUNT** (#871): its count is the player's Max and `0`
    // means no Max; `stop_labor` ends the row.
    let unstaffing = workers == 0 && !target.is_need_row() && !target.is_staffed();
    let staffing_a_standing_pool = matches!(
        target,
        LaborTarget::Roadwork | LaborTarget::Tile { .. } | LaborTarget::Band { .. }
    );
    if staffing_a_standing_pool && kit_id.is_some() {
        let refusal = match target {
            LaborTarget::Tile { .. } => {
                "a tile crew names no kit — its keeping tools follow from the rung the improvement stands on"
            }
            LaborTarget::Band { .. } => {
                "a band crew names no kit — its keeping tools follow from the rung the improvement stands on"
            }
            _ => "a pool names no kit — the road keepers' tools follow from the rung each road stands on",
        }
        .to_string();
        return Err(refused(event_kind, format!("assign_labor: {refusal}.")));
    }
    let crew_kit = if unstaffing || staffing_a_standing_pool {
        None
    } else {
        let equipment_cfg = world.resource::<EquipmentConfigHandle>().get();
        let absent = default_kit_for_target(world, &equipment_cfg, &target);
        match equipment_cfg.resolve_kit_or(kit_id.as_deref(), target.kit_job(), absent) {
            Ok(kit) => Some(kit),
            Err(reason) => {
                return Err(refused(event_kind, format!("assign_labor: {reason}.")));
            }
        }
    };

    let band = select_starting_band(world, faction, band_id, "assign_labor")
        .map_err(|reason| refused(event_kind, reason))?;
    if let Some(reason) = kept_on_hand_refusal(world, &band, &target) {
        return Err(refused(event_kind, reason));
    }
    if let Some(reason) = another_band_refusal(world, &target, band.entity) {
        return Err(refused(event_kind, reason));
    }

    // **The bench's crew is off the table.** `Make` draws idle workers onto a recipe, so a band with
    // four hands at the bench has four fewer to send anywhere — see [`BandWorkforce::assignable`],
    // which nets the bench out for the command path and the published `idleWorkers` alike. Not
    // modelled as a `LaborTarget` because a bench is not an in-range source, and giving it one would
    // put a fictitious row on every yield readout in the game.
    let available = band_workforce(world, band.entity).assignable();

    // **"IS THERE STILL ANYTHING OF OURS HERE?"**, asked before the allocation is borrowed
    // (`docs/plan_standing_upkeep.md` §2.2). A source row survives losing its take crew — it is the
    // band's *holding*, and the site's crew keeps what a band holds, not what it happens to be
    // gathering — but a wild stand nobody has built anything on is not a holding, so unstaffing one
    // clears its row here rather than leaving a `+0.00` row for the turn to sweep up. The same
    // predicate retires a holding whose meter has finally rotted away, inside `advance_labor_allocation`.
    let source_holds_something = {
        let forage_registry = world.resource::<ForageRegistry>();
        let herds = world.resource::<HerdRegistry>();
        let deposits = world.resource::<crate::extraction::DepositRegistry>();
        let ladder = world.resource::<LadderConfigHandle>().get();
        let tile_improvements = world.resource::<crate::TileImprovementRegistry>();
        crate::source_has_a_meter_at_risk(
            &target,
            forage_registry,
            herds,
            deposits,
            tile_improvements,
            &ladder,
        )
    };

    // **THE FIRST CREW PLACES THE IMPROVEMENT** at its branch's floor, so the row, the wire and a
    // `build` all see ground something stands on. Another people's improvement is refused.
    if let (true, LaborTarget::Tile { tile, branch }) =
        (workers > 0 || target.is_staffed(), &target)
    {
        let placing_band = world.get::<BandId>(band.entity).copied();
        if let Err(reason) = crate::build_gates::place_tile_improvement_root(
            world,
            faction,
            placing_band,
            *tile,
            *branch,
        ) {
            return Err(refused(event_kind, reason));
        }
    }
    // **A BAND CREW PLACES THE BAND'S OWN IMPROVEMENT**, at the branch's floor. The row names the
    // band the command was addressed to (the selected band), never another.
    if let (
        true,
        LaborTarget::Band {
            band: held_by,
            branch,
        },
    ) = (workers > 0 || target.is_staffed(), &target)
    {
        let ladder = world.resource::<LadderConfigHandle>().get();
        if world.get::<BandId>(band.entity) != Some(held_by) {
            return Err(refused(
                event_kind,
                format!(
                    "Band {} is not the band this order was addressed to.",
                    held_by.0
                ),
            ));
        }
        world
            .resource_mut::<crate::TileImprovementRegistry>()
            .bands_mut()
            .place(*held_by, *branch, &ladder);
    }

    let kind_label = target.kind();
    // **AN OFF-SITE CREW IS A SOWING CREW**: the root rung's rule refuses the ground and the placing
    // rung admits it, so the crew exists to raise the farm - the sow is declared with it.
    let off_site_sow = match &target {
        LaborTarget::Forage { tile, .. } if workers > 0 || target.is_staffed() => {
            off_site_placing_rung(world, faction, *tile)
        }
        _ => None,
    };
    let prior_staffing;
    let (applied, assigned_total, dropped_row) = {
        let mut allocation = band_allocation_mut(world, band.entity);
        // The row's staffing BEFORE this order: `set_assignment`'s headroom counts these hands as
        // free (it replaces them), so the clamp note must net them back out.
        prior_staffing = allocation.workers_on(&target);
        // **A STAFFED ROW TAKES THE COUNT AS ITS MAX** (#871); the crew is the sim's to decide and
        // is restaffed immediately below. Every other kind keeps the crew the player named.
        let applied = if target.is_staffed() {
            allocation.set_staffed_row(target.clone(), (workers > 0).then_some(workers), crew_kit);
            allocation.workers_on(&target)
        } else {
            allocation.set_assignment(target.clone(), workers, available, crew_kit)
        };
        // **ONE HERD AT A TIME** — migration mode on this hunt row turns it off on the band's others.
        if let LaborTarget::Hunt {
            fauna_id,
            move_with_herd: true,
            ..
        } = &target
        {
            allocation.clear_move_with_herd_except(Some(fauna_id));
        }
        // **Nothing built, nothing DECLARED, nobody on it — the band's business here is over.**
        // The **declared build** is part of the test: a `Cultivate` declared this turn has no progress
        // on its meter yet, so dropping the row on the ground's answer alone would abandon a build
        // the player had just ordered (`docs/plan_standing_upkeep.md` §2.5 — an entry requires a
        // row, so dropping the row would drop the entry with it).
        let queued = BuildSource::of(&target).is_some_and(|source| allocation.has_build(&source));
        // **A NEED ROW IS KEPT BY ITS FLAG**, so a zero head count never drops it: it exists at zero
        // by design (`docs/plan_roaming_bands.md` §Hunting by need).
        let dropped_row = if applied == 0
            && !source_holds_something
            && !queued
            && !target.is_need_row()
            && !target.is_staffed()
        {
            allocation.drop_source_row(&target)
        } else {
            None
        };
        (applied, allocation.assigned_total(), dropped_row)
    };
    // **THE SNAPSHOT SHOWS THE NEW CREW THE MOMENT THE PLAYER ACTS** (#871): a staffed row's crew is
    // the sim's, so the order restaffs the band here instead of waiting for the turn.
    // A player-set crew (Scout, Warrior, Builders) shrinks the pool the sim staffs from, so it
    // restaffs too and the sources give up those hands at once.
    restaff_and_reseed(world, band.entity);
    let applied = if target.is_staffed() {
        band_allocation_mut(world, band.entity).workers_on(&target)
    } else {
        applied
    };
    // **An unassigned far row sends its whole caravan walking home** — the load and every walker's pack —
    // rather than losing what was on the road with the row.
    if let Some(row) = dropped_row.as_ref() {
        crate::bring_the_dropped_party_home(world, band.entity, row);
    }
    if let (Some(rung), true) = (off_site_sow, applied > 0 || target.is_staffed()) {
        let ladder = world.resource::<LadderConfigHandle>().get();
        let farmed = match &target {
            LaborTarget::Forage { tile, .. } => world
                .resource::<ForageRegistry>()
                .patch(*tile)
                .is_some_and(|patch| patch.standing().holds_at_or_above(ladder.rung(rung))),
            _ => false,
        };
        if !farmed {
            let mut allocation = band_allocation_mut(world, band.entity);
            if let Some(source) = BuildSource::of(&target) {
                allocation.declare_build(source, BuildJob::Climb(rung));
            }
            // The sim staffs the declared build from the band's idle hands.
            restaff_and_reseed(world, band.entity);
        }
    }
    // **A TILE CREW ROW IS THE ORDER FOR THE BRANCH'S FIRST BUILD** — see
    // [`declare_the_branchs_first_build`].
    if workers > 0 || target.is_staffed() {
        declare_the_branchs_first_build(world, faction, &target);
    }
    // The seed must price the dip of whatever this band has **queued** here, rather than the
    // undipped stance — a build in flight is the queue's, not the row's. Read after the allocation
    // borrow is released, because resolving the declaration against the ground needs both webs'
    // registries.
    let improvement = build_rung_on_source_at(world, band.entity, &target);
    // Show the source's expected yield immediately (workers added/removed OR stance changed — every
    // shape of this command that moves the number): without the seed the row reads `+0.00` until the
    // player advances a turn.
    seed_source_yield(world, band.entity, &target, improvement, applied);

    let tick = world.resource::<SimulationTick>().0;
    // **ONE LINE FOR THE NAMES THE GROUND TOOK OFF THE SELECTION**, ahead of the applied row it
    // explains. It is stated because the player did not ask for it; it is stated *briefly* because
    // the row underneath already says what the crew now carries.
    if !pruned_take.is_empty() {
        let names = pruned_take.join(", ");
        // **`band=` NAMES THE WORK BOARD THIS NARROWING HAPPENED ON** — the token the event dock's
        // per-row *"Work tab"* link reads (`systems::labor::band_detail_token` says why it is the
        // durable `BandId` and never the entity). Read off the selected band rather than the
        // command's own argument, because that argument is absent whenever the player let the
        // default-band picker choose.
        //
        // **It sits BEFORE `dropped=`, and it has to.** Detail tokens are space-delimited
        // `key=value`, so the comma-joined display names can only be the trailing remainder
        // (`.claude/rules/core_sim/event-feed.md`) — anything appended after them is swallowed by
        // that value.
        let band_token = world
            .get::<BandId>(band.entity)
            .map(|id| format!(" band={}", id.0))
            .unwrap_or_default();
        push_command_event(
            world,
            tick,
            event_kind,
            faction,
            format!("{} no longer stands here — dropped from the take", names),
            Some(format!(
                "status=pruned reason=not_here role={}{} dropped={}",
                kind_label, band_token, names
            )),
        );
    }
    // A need row takes no standing hands, so there is nothing to clamp: the count sent is ignored.
    let clamp_note = if target.is_staffed() {
        // **A staffed row's count is the Max and the crew is the sim's** (#871): the line states
        // both rather than a clamp that is not one.
        if workers > crate::NO_CREW_ON_THIS_ACTIVITY {
            format!(" (max {workers})")
        } else {
            " (no max)".to_string()
        }
    } else if applied < workers && !target.is_need_row() {
        // **`applied` IS THE HEADROOM when clamped**: the hands free for THIS source after the bench,
        // the walkers and every other row — but it includes the hands already on this row, which
        // were not idle. The idle count is the growth over the row's prior staffing.
        let free = applied.saturating_sub(prior_staffing);
        if free == 0 {
            format!(" (clamped from {} — no idle hands)", workers)
        } else {
            format!(" (clamped from {} — only {} idle)", workers, free)
        }
    } else {
        String::new()
    };
    push_band_event(
        world,
        tick,
        event_kind,
        faction,
        band.id,
        format!("{} {} x{}{}", band.label, kind_label, applied, clamp_note),
        Some(format!(
            "status=applied role={} workers={} assigned_total={} available={}",
            kind_label, applied, assigned_total, available
        )),
    );
    Ok(())
}

/// **STATE A BAND'S SCOUTING RANGE** (#875) - `assign_labor <faction> <band> scout <tiles> [kit <k>]`.
/// `tiles` is how far past the band's local range (`supply_network_config.reach_tiles`) the player
/// wants to see, clamped to `labor_config.scout.range_past_local_max`; `0` drops the Scout row. The
/// scouts that reach it are the sim's ([`crate::staffing`]); the band is restaffed here so the
/// snapshot shows the crew and the reach the moment the player acts.
pub fn apply_scouting_range(
    world: &mut World,
    faction: FactionId,
    band_id: Option<u64>,
    requested_tiles: u32,
    kit_id: Option<String>,
    event_kind: CommandEventKind,
) -> Result<(), CommandRefusal> {
    // A kit is resolved only when the range is above nothing (stopping scouting is never refused
    // over a kit that has since left the roster) and only when one was named: the standing row
    // keeps its kit otherwise, and a new row reads the job's default.
    let kit = match kit_id.as_deref() {
        Some(id) if requested_tiles > crate::NO_CREW_ON_THIS_ACTIVITY => {
            let equipment_cfg = world.resource::<EquipmentConfigHandle>().get();
            let absent = equipment_cfg.no_kit();
            match equipment_cfg.resolve_kit_or(Some(id), LaborTarget::Scout.kit_job(), absent) {
                Ok(kit) => Some(kit),
                Err(reason) => {
                    return Err(refused(event_kind, format!("assign_labor: {reason}.")));
                }
            }
        }
        _ => None,
    };
    let band = select_starting_band(world, faction, band_id, "assign_labor")
        .map_err(|reason| refused(event_kind, reason))?;
    let cap = world
        .resource::<LaborConfigHandle>()
        .get()
        .scout
        .range_past_local_max;
    let range = requested_tiles.min(cap);
    // **A tribe member's scouting RANGE is the tribe's** (#862): `tribe_scout` sets it. The kit is
    // still the band's, so a command that restates the band's current range (the kit picker's)
    // is accepted and applies the kit alone; one that asks for a different range is refused.
    let in_a_tribe = band.id.is_some_and(|id| {
        world
            .resource::<crate::TribeRegistry>()
            .tribe_of(id)
            .is_some()
    });
    let current_range = band_allocation_mut(world, band.entity).scouting_range_tiles();
    if in_a_tribe && range != current_range {
        return Err(refused(event_kind, format!(
                "{}'s scouting range is set by its tribe - use tribe_scout (scouting_set_by_tribe).",
                band.label
            )));
    }
    band_allocation_mut(world, band.entity).set_scouting_range(range, kit);
    restaff_and_reseed(world, band.entity);
    let (staffed, reach) = {
        let allocation = band_allocation_mut(world, band.entity);
        (
            allocation.workers_on(&LaborTarget::Scout),
            allocation.scouting_reach_tiles(),
        )
    };
    let tick = world.resource::<SimulationTick>().0;
    let clamp_note = if range < requested_tiles {
        format!(" (clamped from {requested_tiles} - the most is {cap})")
    } else {
        String::new()
    };
    push_band_event(
        world,
        tick,
        event_kind,
        faction,
        band.id,
        format!(
            "{} scouting range +{range} tiles{clamp_note}: {staffed} scouts reach +{reach}",
            band.label
        ),
        Some(format!(
            "status=applied role=scout range={range} reach={reach} workers={staffed}"
        )),
    );
    Ok(())
}

/// **PUTTING A BAND ON A TILE OR BAND-IMPROVEMENT ROW IS THE ORDER FOR ITS FIRST BUILD.**
///
/// When a `tile` crew row is created or raised and the tile still stands at the branch's root rung
/// with no build declared on it, the branch's next rung up from the root is declared - exactly what
/// `build <verb> <faction> <x> <y>` would declare, through the same gate ([`validate_improvement`])
/// and the same queueing ([`queue_build_on_working_bands`]). A refusing gate (lesson unlearned, a
/// site term unmet) declares nothing and is no error: the row stays assigned. A tile already holding
/// the improvement is kept by the crew as before, and the climb never goes past the first rung above
/// the root. Driven by the catalog: no branch is named here.
pub fn declare_the_branchs_first_build(
    world: &mut World,
    faction: FactionId,
    target: &LaborTarget,
) {
    let (LaborTarget::Tile { branch, .. } | LaborTarget::Band { branch, .. }) = target else {
        return;
    };
    let branch = *branch;
    let ladder = world.resource::<LadderConfigHandle>().get();
    let Some(root) = ladder.root_of(branch) else {
        return;
    };
    let Some(first) = ladder.above_rung(root) else {
        return;
    };
    let Some(verb) = first.verb_id() else {
        return;
    };
    let registry = world.resource::<crate::TileImprovementRegistry>();
    let held = match target {
        LaborTarget::Tile { tile, .. } => registry.get(*tile, branch).map(|i| i.held_rung()),
        LaborTarget::Band { band, .. } => {
            registry.bands().get(*band, branch).map(|i| i.held_rung())
        }
        _ => None,
    };
    let stands_at_the_root = held.is_none_or(|rung| rung == root.rung_id());
    if !stands_at_the_root {
        return;
    }
    let Some(source) = BuildSource::of(target) else {
        return;
    };
    let already_declared = world
        .query::<(&PopulationCohort, &LaborAllocation)>()
        .iter(world)
        .any(|(cohort, allocation)| cohort.faction == faction && allocation.has_build(&source));
    if already_declared || validate_improvement(world, faction, target, verb).is_err() {
        return;
    }
    queue_build_on_working_bands(world, faction, target, BuildJob::Climb(first.rung_id()));
}

/// **QUEUE A BUILD ON EVERY BAND OF `faction` ALREADY WORKING `target`'s SOURCE** (matched by
/// `LaborTarget::same_source`, so the tile / herd id). Returns how many bands were updated (`0` =
/// nobody is working that source, which the callers report as *"staff it first"*). The shared body
/// of the four improvement verbs and of `extend_pen`.
///
/// # A VERB DECLARES; IT DOES NOT STAFF
///
/// It adds a [`crate::BuildDeclaration`] and restaffs the band: the sim puts builders on the
/// build from the band's idle hands (`docs/plan_labor_at_scale.md`, "Builds join the site's crew"),
/// so this command names no crew, refuses nothing on affordability, and cannot disband the
/// gathering it was meant to improve.
///
/// # RE-ISSUING KEEPS THE ENTRY'S PLACE IN THE LINE
///
/// `LaborAllocation::declare_build` replaces `declared` in place, so correcting `cultivate` → `sow`
/// on a declared source keeps its one declaration. Withdrawing one is `unqueue`.
///
/// **A verb reaches only bands already WORKING the source** — the rule this function has always
/// carried, and now the rule that keeps an entry paired with a row.
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
    // stance and crop that band actually holds — the command names neither.
    let bands: Vec<(Entity, u32, LaborTarget)> = world
        .query::<(Entity, &PopulationCohort, &LaborAllocation)>()
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
            let replaced = band_allocation_mut(world, *entity)
                .declare_build_replacing(source.clone(), declared)
                .flatten();
            // **A verb that replaces another takes its funding**, so the replaced job's herd state
            // goes with it (the same rule a drop runs).
            crate::fauna::cancel_replaced_job(
                &mut world.resource_mut::<HerdRegistry>(),
                replaced,
                declared,
            );
        }
        // **A queued Corral picks its pen's tile now** - the best-grazing land within this band's
        // work range, so the quotes price the tile the pen will really stand on.
        if let (BuildSource::Herd(id), BuildJob::Climb(rung)) = (&source, declared) {
            let ladder = world.resource::<LadderConfigHandle>().get();
            if ladder
                .rung(rung)
                .trait_set()
                .has(crate::RungTrait::Enclosed)
            {
                choose_the_pen_site(world, *entity, id);
            }
        }
        // Queueing a build does not change the take at all, but the source's dip does depend on
        // what is being raised on it, so re-seed the telemetry from the new forecast — the same
        // reason `handle_assign_labor` seeds after a stance edit.
        let improvement = build_rung_on_source_at(world, *entity, band_target);
        seed_source_yield(world, *entity, band_target, improvement, *workers);
        // A declared build is staffed by the sim, so the declaration restaffs the band: the new
        // build's builders show on the command's own frame.
        restaff_and_reseed(world, *entity);
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
        .has(crate::RungTrait::Enclosed)
}

/// **THE BAND, OF ANY FACTION, THAT HOLDS THIS HERD'S CORRAL IN ITS QUEUE** - `None` when no band
/// has one queued.
pub fn pen_queuer(world: &mut World, source: &BuildSource) -> Option<Entity> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    world
        .query::<(Entity, &LaborAllocation)>()
        .iter(world)
        .find(|(_, allocation)| {
            allocation
                .build_declaration(source)
                .and_then(|entry| entry.declared.climb())
                .is_some_and(|rung| {
                    ladder
                        .rung(rung)
                        .trait_set()
                        .has(crate::RungTrait::Enclosed)
                })
        })
        .map(|(entity, _)| entity)
}

/// **A HERD CAN BE PENNED IN ONE PLACE ONLY**, so the Corral is queued on exactly one band: the one
/// that already holds it (a re-issue replaces its entry in place), else the lowest `BandId` of the
/// bands working the herd. Another band's hold - of any faction - leaves none, and the caller
/// refuses with [`PEN_TAKEN_HINT`](crate::build_gates::PEN_TAKEN_HINT).
pub fn bands_that_may_queue_the_pen(
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
pub fn choose_the_pen_site(world: &mut World, band: Entity, herd_id: &str) {
    let Some(band_tile) = world
        .get::<PopulationCohort>(band)
        .and_then(|cohort| world.get::<crate::Tile>(cohort.current_tile))
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

/// **MARK ONE WORKED ROW WITH THE PLAYER'S OWN RANK** — `work_priority <faction> <band> <x> <y>
/// <level>` / `work_priority <faction> <band> <herd_id> <level>` / `work_priority <faction> <band>
/// <x> <y> <material> <level>` for a working's extract row (`docs/plan_standing_upkeep.md` §4.9
/// item 9b, `docs/plan_site_crews.md` §2.4). A road (`road <x> <y>`) has no row and is refused.
///
/// Sets [`crate::SourcePriority`] on the named band's assignment for that source. The take crew,
/// the floor, the kit and the declared build are untouched — this says only *where this row stands when
/// the band runs short*.
///
/// # IT IS ONE BAND'S STATEMENT, WHICH IS WHY IT NAMES ONE
///
/// The ordering it feeds is a **band's**: the shedding walk partitions that band's own rows, and the
/// pen-feed split serves that band's own stores. `build_order` names a band for exactly the same
/// reason. The source-addressed verbs that reach *every* band holding a source (`unqueue`,
/// `abandon`) are the ones whose subject is the ground rather than the holding.
///
/// # AN UNKNOWN LEVEL IS REFUSED BY NAME
///
/// `upkeep_mode`'s rule. A rank the player mistyped must fail loudly rather than silently landing on
/// the default, which is the one value that would look like it worked.
pub fn apply_work_priority(
    world: &mut World,
    faction: FactionId,
    band_id: u64,
    source: BuildSourceRef,
    level: String,
) -> Result<(), CommandRefusal> {
    let ladder = source_ladder(world, &source)?;
    let source = match source.bound_to_band(&ladder, band_id) {
        Ok(source) => source,
        Err(reason) => {
            return Err(refused(CommandEventKind::CancelOrder, reason));
        }
    };
    let label = source.label(&ladder);
    // ⛔ **A ROAD HAS NO CREW, SO NO ROW TO RANK** (`docs/plan_site_crews.md` §2.4): its only mark
    // is the Build mark on a queued road build. Refused by name rather than read as the tile's patch,
    // which is the one outcome that would look like it worked.
    if source.is_road(&ladder) {
        return Err(refused(CommandEventKind::CancelOrder, format!(
                "{label} has no crew to rank — a road's only mark is its declared build's, set with \
                 build_priority."
            )));
    }
    let Some(target) = source.target(&ladder) else {
        return Err(refused(
            CommandEventKind::CancelOrder,
            format!("work_priority needs a source: {SITE_SOURCE_GRAMMAR}."),
        ));
    };
    let Some(priority) = parse_work_priority_level(&level) else {
        return Err(refused(
            CommandEventKind::CancelOrder,
            format!(
                "Unknown work priority '{}' — expected {}, {} or {}.",
                level.trim(),
                SourcePriority::High.as_str(),
                SourcePriority::Normal.as_str(),
                SourcePriority::Low.as_str()
            ),
        ));
    };
    let band = select_starting_band(world, faction, Some(band_id), "work_priority")
        .map_err(|reason| refused(CommandEventKind::CancelOrder, reason))?;
    if let Some(reason) = kept_on_hand_refusal(world, &band, &target) {
        return Err(refused(CommandEventKind::Extraction, reason));
    }
    let marked = band_allocation_mut(world, band.entity).set_source_priority(&target, priority);
    // **A rank decides who the sim arms first** (#871), so the crews are re-decided at once.
    restaff_and_reseed(world, band.entity);
    if !marked {
        return Err(refused(
            CommandEventKind::CancelOrder,
            format!("{} works nothing at {label} to rank.", band.label),
        ));
    }
    let tick = world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "work_priority",
        faction = %faction.0,
        band = band_id,
        source = %label,
        level = priority.as_str(),
        "command.work_priority.applied"
    );
    let sentence = match priority {
        SourcePriority::High => format!("{}: {label} is held before anything else", band.label),
        SourcePriority::Normal => format!("{}: {label} takes its turn like the rest", band.label),
        SourcePriority::Low => format!("{}: {label} is the first thing given up", band.label),
    };
    push_command_event(
        world,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        sentence,
        Some(format!(
            "status=applied action=work_priority source={label} level={} band={band_id}",
            priority.as_str()
        )),
    );
    Ok(())
}

/// **HOW THE QUEUE VERBS NAME A SOURCE** — a tile, a herd id, a working (`material` on the tile
/// pair) or a branch's own per-tile source (`branch` on the tile pair — the road branch's road)
/// (`docs/plan_standing_upkeep.md` §2.5, `docs/plan_site_crews.md` §2.4). Every queue verb reads it:
/// `abandon`, `unqueue`, `build_order`, `work_priority` and `build_priority`.
///
/// One type rather than per-verb copies of the same optional fields, because the queue verbs address
/// a source identically and a per-verb spelling is how one of them comes to accept a shape the
/// others reject. It resolves to a [`LaborTarget`] once, in [`BuildSourceRef::target`], and to a
/// queue key once, in [`BuildSourceRef::build_source`].
///
/// **`branch` is a ladder branch id and is resolved against the ladder, never matched by name**: it
/// names a road when the branch's source kind is [`SourceKind::Road`] ([`Self::is_road`]), and a
/// branch the ladder does not know — or one whose source is not kept per tile — is refused by name
/// ([`Self::branch_refusal`]) rather than read as the tile's patch, which would be a destructive
/// verb quietly landing on the wrong source.
#[derive(Debug, Clone, Default)]
pub struct BuildSourceRef {
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    pub herd_id: Option<String>,
    /// With the tile pair, the deposit a **working** is keyed by — one hex can hold two.
    pub material: Option<String>,
    /// With the tile pair, the ladder branch whose own source on the tile is named, rather than the
    /// tile's patch.
    pub branch: Option<String>,
}

impl BuildSourceRef {
    /// **Does this name the tile's road?** — the named branch is the ladder's road-kind branch.
    pub fn is_road(&self, ladder: &LadderConfig) -> bool {
        self.branch.as_deref().is_some_and(|branch| {
            ladder.source_kind(BranchId::new(branch)) == Some(SourceKind::Road)
        })
    }

    /// **Does this name a band improvement?** - the named branch is a band-kind branch of the ladder
    /// (a drying rack). The band rides `herd_id`, as `build rack <faction> <band>` carries it.
    pub fn is_band_improvement(&self, ladder: &LadderConfig) -> bool {
        self.branch.as_deref().is_some_and(|branch| {
            ladder.source_kind(BranchId::new(branch)) == Some(SourceKind::Band)
        })
    }

    /// **Pin a band improvement to the band a queue verb already names.** `build_order`,
    /// `build_priority` and `work_priority` carry the band in the command, so their source is
    /// `<branch>` alone; a band named a second time that disagrees is refused rather than
    /// silently preferred. Any other source is returned as it was.
    pub fn bound_to_band(mut self, ladder: &LadderConfig, band_id: u64) -> Result<Self, String> {
        if !self.is_band_improvement(ladder) {
            return Ok(self);
        }
        match self.herd_id.as_deref().map(str::parse::<u64>) {
            None => self.herd_id = Some(band_id.to_string()),
            Some(Ok(named)) if named == band_id => {}
            Some(_) => {
                return Err(format!(
                    "a band improvement belongs to the band named in the command ({band_id}), \
                     not to {}.",
                    self.herd_id.as_deref().unwrap_or_default()
                ));
            }
        }
        Ok(self)
    }

    /// **Does this name a tile improvement?** - the named branch is a tile-kind branch of the ladder.
    pub fn is_tile_improvement(&self, ladder: &LadderConfig) -> bool {
        self.branch.as_deref().is_some_and(|branch| {
            ladder.source_kind(BranchId::new(branch)) == Some(SourceKind::Tile)
        })
    }

    /// **Why this source's `branch` cannot be read**, or `None` when it names a road or names none.
    /// An unknown id and a branch whose source is not per-tile are both refused by name.
    pub fn branch_refusal(&self, ladder: &LadderConfig) -> Option<String> {
        let branch = self.branch.as_deref()?;
        match ladder.source_kind(BranchId::new(branch)) {
            Some(SourceKind::Road | SourceKind::Tile | SourceKind::Band) => None,
            Some(_) => Some(format!(
                "'{branch}' is a branch whose source is worked by a crew, not kept per tile - \
                 name it by its tile, herd or `<x> <y> <material>`."
            )),
            None => Some(format!(
                "'{branch}' is not a branch this ladder knows. {SITE_SOURCE_GRAMMAR}."
            )),
        }
    }

    /// The labor target this names, or `None` when the wire carried neither shape — **and for a
    /// road**, which has no labor row ([`BuildSource::Road`]). A herd id wins a malformed
    /// both-shapes message only because the tile pair is tested first and requires both halves —
    /// the text grammar cannot produce one at all.
    pub fn target(&self, ladder: &LadderConfig) -> Option<LaborTarget> {
        if self.is_road(ladder) {
            return None;
        }
        if self.is_band_improvement(ladder) {
            let band = self.herd_id.as_deref()?.parse::<u64>().ok()?;
            return Some(LaborTarget::Band {
                band: BandId(band),
                branch: BranchId::new(self.branch.as_deref()?),
            });
        }
        if self.is_tile_improvement(ladder) {
            return match (self.target_x, self.target_y, self.branch.as_deref()) {
                (Some(x), Some(y), Some(branch)) => Some(LaborTarget::Tile {
                    tile: UVec2::new(x, y),
                    branch: BranchId::new(branch),
                }),
                _ => None,
            };
        }
        match (self.target_x, self.target_y, self.herd_id.as_ref()) {
            (Some(x), Some(y), _) => Some(match &self.material {
                Some(material) => LaborTarget::Extract {
                    tile: UVec2::new(x, y),
                    material: material.clone(),
                    floor: SOURCE_NAMED_NOT_ASSIGNED,
                },
                None => forage_source(UVec2::new(x, y)),
            }),
            (_, _, Some(herd_id)) => Some(LaborTarget::Hunt {
                fauna_id: herd_id.clone(),
                floor: SOURCE_NAMED_NOT_ASSIGNED,
                move_with_herd: false,
            }),
            _ => None,
        }
    }

    /// **The build-source key this names** — a road's own [`BuildSource::Road`], else the queue key
    /// of the labor row it names ([`BuildSource::of`]).
    pub fn build_source(&self, ladder: &LadderConfig) -> Option<BuildSource> {
        match (self.is_road(ladder), self.target_x, self.target_y) {
            (true, Some(x), Some(y)) => Some(BuildSource::Road(UVec2::new(x, y))),
            (true, _, _) => None,
            (false, _, _) => self.target(ladder).as_ref().and_then(BuildSource::of),
        }
    }

    /// What the feed line calls it.
    pub fn label(&self, ladder: &LadderConfig) -> String {
        match (self.target_x, self.target_y, self.herd_id.as_ref()) {
            (_, _, Some(band)) if self.is_band_improvement(ladder) => format!(
                "the {} of band {band}",
                self.branch.as_deref().unwrap_or_default()
            ),
            (Some(x), Some(y), _) if self.is_road(ladder) => {
                format!("the road at ({x}, {y})")
            }
            (Some(x), Some(y), _) if self.is_tile_improvement(ladder) => {
                format!(
                    "the {} improvement at ({x}, {y})",
                    self.branch.as_deref().unwrap_or_default()
                )
            }
            (Some(x), Some(y), _) => match &self.material {
                Some(material) => format!("the {material} at ({x}, {y})"),
                None => format!("({x}, {y})"),
            },
            (_, _, Some(herd_id)) => herd_id.clone(),
            _ => "that source".to_string(),
        }
    }
}

/// **The forage source at `tile`, as the improvement commands name it.** `same_source` matches on
/// the tile alone, so the stance, crop and take-selection slots are placeholders — this exists so
/// the two build verbs cannot accidentally *carry* a stance, a crop or a selection into
/// `queue_build_on_working_bands`, and so "the command names a source, not an assignment" is stated
/// once.
pub fn forage_source(tile: UVec2) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor: SOURCE_NAMED_NOT_ASSIGNED,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
}

/// **The floor a `LaborTarget` built to NAME A SOURCE carries** — the improvement commands
/// (`cultivate`/`sow`/`tame`/`corral`/`fell`/`coppice`/`quarry`), the abandon path and the
/// pen-keeper lookup all construct a target to identify a tile, a herd or a working, never to state
/// an assignment. [`LaborTarget::same_source`] keys on the tile/herd id (and, on a deposit, the
/// material) alone, so the floor here is matched by nothing and read by nothing.
///
/// It is [`DEFAULT_ESCAPEMENT_FLOOR`] rather than an arbitrary number so that a future reader who
/// *does* look at it sees the sustainable value, not a strip order.
pub const SOURCE_NAMED_NOT_ASSIGNED: f32 = DEFAULT_ESCAPEMENT_FLOOR;
