//! **The `rungs` section of a scenario** (`docs/plan_scenarios.md` sections 2.2 and 2.8, slice 2b):
//! improvements a faction already holds when the world opens, and builds already declared.
//!
//! Two entry kinds, one list, in file order:
//!
//! - **`held`** is a setter. It names a source (a patch, a herd, a working) and a rung, and the
//!   source is climbed to that rung **through the same gate `build <verb>` runs**
//!   ([`crate::build_gates::validate_improvement`]), writing what the game's own completion of each rung leaves
//!   ([`crate::forage::ForagePatch::complete_rung_as_built`] and its two twins) - so a setter cannot reach a state
//!   the game cannot.
//! - **`build_rung`** is the `BuildRung` command, replayed through [`crate::command_seam`] after the
//!   outfit, so it is refused with the handler's own reason.
//!
//! A herd is named by its **scenario id** (the `herds` section's `id`) in both entry kinds; apply maps it
//! through the runtime id `herds::apply` returned.
//!
//! Pass 1 ([`check`]) resolves names and collects every refusal. Pass 2 is [`apply_held`] and
//! [`apply_builds`], called by [`super::finish_scenario`] in the apply order of section 2.8.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::bands::{FactionSection, FactionSetup};
use super::herds::HerdEntry;
use super::{round_export, ExportIds, ScenarioContext, ScenarioRefusal};
use crate::command_seam::SOURCE_NAMED_NOT_ASSIGNED;
use crate::command_seam::{apply_build_rung, BuildRungRequest};
use crate::components::{BandId, LaborTarget, TakeSelection};
use crate::extraction::DepositRegistry;
use crate::extraction_config::ExtractionConfigHandle;
use crate::fauna_config::FaunaConfigHandle;
use crate::flora_config::FloraConfigHandle;
use crate::forage::{
    patch_field_cost_multiplier, resolve_committed_species, tile_flora_composition, ForageRegistry,
};
use crate::grid_utils::hex_distance_wrapped;
use crate::intensification::{LadderConfig, RungDef, RungTrait, SourceKind};
use crate::labor_config::LaborConfigHandle;
use crate::orders::FactionId;
use crate::resources::{SimulationConfig, TileRegistry};
use crate::tile_improvement::{tile_rung_span, TileImprovementRegistry};
use crate::{
    pen_site_in_world, HerdRegistry, LadderConfigHandle, PopulationCohort, RungId, Tile, VerbId,
};

/// One entry of a faction's `rungs` list.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RungEntry {
    /// A source already holding a rung.
    Held(HeldRung),
    /// A `BuildRung` command, replayed after the outfit.
    BuildRung(BuildRungEntry),
}

/// A source held at a rung. Which fields it needs follows the rung's branch's source kind.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HeldRung {
    /// A rung id as the ladder spells it, `plant:tended`.
    pub rung: String,
    /// The tile, for a patch or a working.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("x-pick" = "tile"))]
    pub at: Option<[u32; 2]>,
    /// The herd id, for a herd.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("x-pick" = "herd"))]
    pub herd: Option<String>,
    /// The material, for a working.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// The crop a patch is committed to, where a rung on the chain `requires_crop`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub species: Option<String>,
}

/// `CommandPayload::BuildRung`, with no faction (the section's own) and the band named by its
/// scenario id.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("x-pick-tile" = ["target_x", "target_y"]))]
pub struct BuildRungEntry {
    /// The keeper band's scenario id.
    #[schemars(extend("x-pick" = "band"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
    pub verb: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_x: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_y: Option<u32>,
    #[schemars(extend("x-pick" = "herd"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub herd_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// **The source kinds a scenario can hold a rung on.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeldKind {
    Patch,
    Herd,
    Deposit,
    /// A fixed store on one tile (the `food_stores` branch).
    Tile,
}

impl HeldKind {
    fn of(kind: Option<SourceKind>) -> Result<Self, String> {
        match kind {
            Some(SourceKind::Patch) => Ok(Self::Patch),
            Some(SourceKind::Herd) => Ok(Self::Herd),
            Some(SourceKind::Deposit) => Ok(Self::Deposit),
            Some(SourceKind::Tile) => Ok(Self::Tile),
            Some(other) => Err(format!(
                "a {} rung is not a `held` source a scenario can set",
                other.as_str()
            )),
            None => Err("that rung's branch declares no source kind".to_string()),
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Herd => "herd",
            Self::Deposit => "deposit",
            Self::Tile => "tile improvement",
        }
    }
}

/// The rungs a climb to `rung` lays, bottom first: every rung above the branch's root, up to and
/// including `rung`.
fn climb<'a>(ladder: &'a LadderConfig, rung: &RungDef) -> Vec<&'a RungDef> {
    let root_order = ladder
        .root_of(rung.branch)
        .map_or(rung.order, |root| root.order);
    ladder
        .rungs_of(rung.branch)
        .into_iter()
        .filter(|candidate| candidate.order > root_order && candidate.order <= rung.order)
        .collect()
}

/// **Pass 1 for the `rungs` section**: every name resolved, every fault collected.
pub(super) fn check(
    factions: &[FactionSection],
    herds: &[HerdEntry],
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let declared_bands: BTreeSet<&str> = factions
        .iter()
        .flat_map(|section| section.bands.iter().map(|band| band.id.as_str()))
        .collect();
    let declared_herds: BTreeSet<&str> = herds.iter().map(|herd| herd.id.as_str()).collect();
    for (i, section) in factions.iter().enumerate() {
        let mut seen_sources: BTreeSet<String> = BTreeSet::new();
        for (j, entry) in section.rungs.iter().enumerate() {
            let at = format!("factions[{i}].rungs[{j}]");
            match entry {
                RungEntry::Held(held) => {
                    check_held(held, &at, ctx, &declared_herds, &mut seen_sources, refusals);
                }
                RungEntry::BuildRung(build) => {
                    check_build(build, &at, ctx, &declared_bands, &declared_herds, refusals);
                }
            }
        }
    }
}

fn check_held(
    held: &HeldRung,
    at: &str,
    ctx: &ScenarioContext,
    declared_herds: &BTreeSet<&str>,
    seen_sources: &mut BTreeSet<String>,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let path = |field: &str| format!("{at}.held.{field}");
    let ladder = &ctx.ladder;
    let Ok(rung_id) = held.rung.parse::<RungId>() else {
        refusals.push(ScenarioRefusal::new(
            path("rung"),
            format!("'{}' is not a rung id (`<branch>:<rung>`)", held.rung),
        ));
        return;
    };
    let Some(rung) = ladder.rung_by_id(rung_id) else {
        refusals.push(ScenarioRefusal::new(
            path("rung"),
            format!("'{}' is not a rung on the ladder", held.rung),
        ));
        return;
    };
    if ladder
        .root_of(rung.branch)
        .is_some_and(|root| root.rung_id() == rung_id)
    {
        refusals.push(ScenarioRefusal::new(
            path("rung"),
            format!(
                "'{}' is its branch's root: every source already holds its root",
                held.rung
            ),
        ));
        return;
    }
    let kind = match HeldKind::of(ladder.source_kind(rung.branch)) {
        Ok(kind) => kind,
        Err(reason) => {
            refusals.push(ScenarioRefusal::new(path("rung"), reason));
            return;
        }
    };
    let mut need = |field: &str, present: bool, needed: bool| {
        if needed && !present {
            refusals.push(ScenarioRefusal::new(
                path(field),
                format!("a {} rung needs '{field}'", kind.noun()),
            ));
        } else if !needed && present {
            refusals.push(ScenarioRefusal::new(
                path(field),
                format!("a {} rung takes no '{field}'", kind.noun()),
            ));
        }
    };
    need("at", held.at.is_some(), kind != HeldKind::Herd);
    need("herd", held.herd.is_some(), kind == HeldKind::Herd);
    need(
        "material",
        held.material.is_some(),
        kind == HeldKind::Deposit,
    );
    // `species` is optional on a patch and refused on the other kinds.
    if kind != HeldKind::Patch && held.species.is_some() {
        need("species", true, false);
    }
    if let (HeldKind::Herd, Some(herd)) = (kind, &held.herd) {
        refuse_undeclared_herd(herd, &path("herd"), declared_herds, refusals);
    }
    if let Some(material) = &held.material {
        match ctx.extraction.deposit(material) {
            None => refusals.push(ScenarioRefusal::new(
                path("material"),
                format!("'{material}' is not a deposit in the extraction config"),
            )),
            Some(def) if kind == HeldKind::Deposit && def.branch != rung.branch => {
                refusals.push(ScenarioRefusal::new(
                    path("material"),
                    format!(
                        "'{material}' is worked on the {} branch, not '{}'",
                        def.branch.as_str(),
                        rung.branch.as_str()
                    ),
                ));
            }
            Some(_) => {}
        }
    }
    if let (Some(species), HeldKind::Patch) = (&held.species, kind) {
        if !ctx.flora.species.contains_key(species) {
            refusals.push(ScenarioRefusal::new(
                path("species"),
                format!("'{species}' is not a species on the flora roster"),
            ));
        }
    }
    let source = match (kind, held.at, &held.herd, &held.material) {
        (HeldKind::Patch, Some([x, y]), _, _) => Some(format!("patch ({x}, {y})")),
        (HeldKind::Herd, _, Some(herd), _) => Some(format!("herd {herd}")),
        (HeldKind::Tile, Some([x, y]), _, _) => Some(format!("{} at ({x}, {y})", held.rung)),
        (HeldKind::Deposit, Some([x, y]), _, Some(material)) => {
            Some(format!("{material} working at ({x}, {y})"))
        }
        _ => None,
    };
    if let Some(source) = source {
        if !seen_sources.insert(source.clone()) {
            refusals.push(ScenarioRefusal::new(
                at,
                format!("the {source} is listed twice"),
            ));
        }
    }
}

fn check_build(
    build: &BuildRungEntry,
    at: &str,
    ctx: &ScenarioContext,
    declared_bands: &BTreeSet<&str>,
    declared_herds: &BTreeSet<&str>,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    if let Some(herd) = &build.herd_id {
        refuse_undeclared_herd(
            herd,
            &format!("{at}.build_rung.herd_id"),
            declared_herds,
            refusals,
        );
    }
    if let Some(band) = &build.band {
        if !declared_bands.contains(band.as_str()) {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.build_rung.band"),
                format!("'{band}' is not a band id declared in this file"),
            ));
        }
    }
    let verb = build.verb.trim().to_ascii_lowercase();
    if ctx.ladder.rung_for_verb(VerbId::new(&verb)).is_none() {
        refusals.push(ScenarioRefusal::new(
            format!("{at}.build_rung.verb"),
            format!("'{}' is not a build verb on the ladder", build.verb),
        ));
    }
}

/// **Refuse a herd id the file's `herds` section does not declare.** Every section names a herd by
/// its scenario id, so a runtime id (`game_wild_sheep_00`) is refused here.
pub(super) fn refuse_undeclared_herd(
    herd: &str,
    path: &str,
    declared_herds: &BTreeSet<&str>,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    if !declared_herds.contains(herd) {
        refusals.push(ScenarioRefusal::new(
            path,
            format!("'{herd}' is not a herd id declared in this file's herds section"),
        ));
    }
}

/// **Pass 2, step 5: the held rungs**, in file order, one faction's worth. Every refusal is
/// collected; a refused entry does not stop the others being tried.
pub(super) fn apply_held(
    world: &mut World,
    setup: &FactionSetup,
    herds: &BTreeMap<String, String>,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    for (j, entry) in setup.rungs.iter().enumerate() {
        let RungEntry::Held(held) = entry else {
            continue;
        };
        let at = format!("factions[{}].rungs[{j}]", setup.file_index);
        // The file names the herd by its scenario id; the climb reads the runtime one.
        let mut held = held.clone();
        if let Some(scenario_id) = &held.herd {
            match herds.get(scenario_id) {
                Some(runtime_id) => held.herd = Some(runtime_id.clone()),
                None => {
                    refusals.push(ScenarioRefusal::new(
                        format!("{at}.held.herd"),
                        format!("herd '{scenario_id}' was not applied"),
                    ));
                    continue;
                }
            }
        }
        if let Err(reason) = climb_held(world, setup.faction, &held) {
            refusals.push(ScenarioRefusal::new(&at, reason));
        }
    }
}

/// Climb one held source to its rung. `Err` is the refusal's text, already naming the rung.
fn climb_held(world: &mut World, faction: FactionId, held: &HeldRung) -> Result<(), String> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    let rung_id: RungId = held
        .rung
        .parse()
        .map_err(|_| format!("'{}' is not a rung id", held.rung))?;
    let rung = ladder
        .rung_by_id(rung_id)
        .ok_or_else(|| format!("'{}' is not a rung on the ladder", held.rung))?;
    let kind = HeldKind::of(ladder.source_kind(rung.branch))?;
    let tile = held.at.map(|[x, y]| UVec2::new(x, y));
    for laid in climb(&ladder, rung) {
        let laid_id = laid.rung_id();
        let named = laid_id.wire_key();
        let Some(verb) = laid.verb_id() else {
            return Err(format!("{named}: the rung has no build verb"));
        };
        let (holds, target) = match kind {
            HeldKind::Patch => {
                let tile = tile.ok_or_else(|| format!("{named}: a patch needs 'at'"))?;
                let patch = world
                    .resource::<ForageRegistry>()
                    .patch(tile)
                    .ok_or_else(|| {
                        format!(
                            "{named}: there is no forage patch at ({}, {})",
                            tile.x, tile.y
                        )
                    })?;
                let target = LaborTarget::Forage {
                    tile,
                    floor: SOURCE_NAMED_NOT_ASSIGNED,
                    species: held.species.clone(),
                    take_species: TakeSelection::EVERYTHING,
                };
                (ladder.at_or_above(patch.standing().held, laid_id), target)
            }
            HeldKind::Herd => {
                let id = held
                    .herd
                    .as_deref()
                    .ok_or_else(|| format!("{named}: a herd needs 'herd'"))?;
                let herd = world
                    .resource::<HerdRegistry>()
                    .find(id)
                    .ok_or_else(|| format!("{named}: there is no herd '{id}'"))?;
                let target = LaborTarget::Hunt {
                    fauna_id: id.to_string(),
                    floor: SOURCE_NAMED_NOT_ASSIGNED,
                    move_with_herd: false,
                };
                (ladder.at_or_above(herd.standing().held, laid_id), target)
            }
            HeldKind::Deposit => {
                let tile = tile.ok_or_else(|| format!("{named}: a working needs 'at'"))?;
                let material = held
                    .material
                    .as_deref()
                    .ok_or_else(|| format!("{named}: a working needs 'material'"))?;
                open_working(world, tile, material, &named)?;
                let working = world
                    .resource::<DepositRegistry>()
                    .source(tile, material)
                    .ok_or_else(|| {
                        format!("{named}: no working opened at ({}, {})", tile.x, tile.y)
                    })?;
                let target = LaborTarget::Extract {
                    tile,
                    material: material.to_string(),
                    floor: SOURCE_NAMED_NOT_ASSIGNED,
                };
                (ladder.at_or_above(working.rung(), laid_id), target)
            }
            HeldKind::Tile => {
                let tile = tile.ok_or_else(|| format!("{named}: a tile store needs 'at'"))?;
                let held_rung = world
                    .resource::<TileImprovementRegistry>()
                    .get(tile, laid_id.branch)
                    .map(|improvement| improvement.held_rung());
                let target = LaborTarget::Tile {
                    tile,
                    branch: laid_id.branch,
                };
                (
                    held_rung.is_some_and(|rung| ladder.at_or_above(rung, laid_id)),
                    target,
                )
            }
        };
        if holds {
            continue;
        }
        crate::build_gates::validate_improvement(world, faction, &target, verb)
            .map_err(|reason| format!("{named}: {reason}"))?;
        write_completion(world, faction, kind, held, tile, laid, &ladder)
            .map_err(|reason| format!("{named}: {reason}"))?;
    }
    Ok(())
}

/// Open the working at `tile` for `material` (a working is opened lazily by the first crew that
/// cuts it), refusing ground that holds none.
fn open_working(world: &mut World, tile: UVec2, material: &str, named: &str) -> Result<(), String> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    let config = world.resource::<ExtractionConfigHandle>().get();
    let entity = world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .ok_or_else(|| format!("{named}: ({}, {}) is off the map", tile.x, tile.y))?;
    let ground = world
        .get::<Tile>(entity)
        .cloned()
        .ok_or_else(|| format!("{named}: ({}, {}) has no tile", tile.x, tile.y))?;
    world
        .resource_mut::<DepositRegistry>()
        .open(tile, material, &ground, &config, &ladder)
        .map(|_| ())
        .ok_or_else(|| {
            format!(
                "{named}: the ground at ({}, {}) holds no {material}",
                tile.x, tile.y
            )
        })
}

/// **The standing tile of the faction's band nearest `target`** by hex distance, ties to the lowest
/// [`BandId`]. A scenario queues no Corral, so this band stands in for the one that would.
fn nearest_band_tile(world: &mut World, faction: FactionId, target: UVec2) -> Option<UVec2> {
    nearest_band(world, faction, target).map(|(_, position)| position)
}

/// **The faction's band nearest `target`** and its standing tile, by hex distance, ties to the lowest
/// [`BandId`].
fn nearest_band(world: &mut World, faction: FactionId, target: UVec2) -> Option<(BandId, UVec2)> {
    let (width, wrap) = {
        let tiles = world.resource::<TileRegistry>();
        let wrap = world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        (tiles.width, wrap)
    };
    let mut query = world.query::<(&BandId, &PopulationCohort)>();
    let bands: Vec<(BandId, Entity)> = query
        .iter(world)
        .filter(|(_, cohort)| cohort.faction == faction)
        .map(|(id, cohort)| (*id, cohort.current_tile))
        .collect();
    bands
        .into_iter()
        .filter_map(|(id, tile)| Some((id, world.get::<Tile>(tile)?.position)))
        .min_by_key(|(id, position)| (hex_distance_wrapped(*position, target, width, wrap), *id))
}

/// **Write what the game's own completion of `laid` leaves on the source**, through the source's
/// own `complete_rung_as_built`.
fn write_completion(
    world: &mut World,
    faction: FactionId,
    kind: HeldKind,
    held: &HeldRung,
    tile: Option<UVec2>,
    laid: &RungDef,
    ladder: &LadderConfig,
) -> Result<(), String> {
    let laid_id = laid.rung_id();
    match kind {
        HeldKind::Patch => {
            let tile = tile.ok_or("a patch needs 'at'")?;
            let flora = world.resource::<FloraConfigHandle>().get();
            let labor = world.resource::<LaborConfigHandle>().get();
            let map_seed = world.resource::<SimulationConfig>().map_seed;
            let entity = world
                .resource::<TileRegistry>()
                .index(tile.x, tile.y)
                .ok_or("the patch's tile is off the map")?;
            let composition = {
                let ground = world.get::<Tile>(entity).ok_or("the patch has no tile")?;
                tile_flora_composition(&flora, &labor.forage, ground, map_seed).into_owned()
            };
            // The crop is committed where the rung `requires_crop`: the named one, or the default
            // pick, resolved through the seam the labor arm and `assign_labor` both read.
            let crop = if laid.requires_crop {
                Some(
                    resolve_committed_species(
                        held.species.as_deref(),
                        &composition,
                        &flora,
                        ladder,
                        laid_id,
                    )
                    .map_err(|refusal| format!("no crop to commit ({})", refusal.as_str()))?,
                )
            } else {
                None
            };
            let mut registry = world.resource_mut::<ForageRegistry>();
            let patch = registry
                .patch_mut(tile)
                .ok_or("the patch vanished while it was being climbed")?;
            // The price is read after the commitment, as the labor arm reads it.
            if let Some(crop) = crop.as_deref() {
                patch.commit_species(crop);
            }
            let multiplier =
                patch_field_cost_multiplier(patch, &composition, &flora, &labor.forage, ladder);
            if !patch.complete_rung_as_built(faction, laid, crop.as_deref(), multiplier, ladder) {
                return Err("the patch could not be raised (another people owns it)".to_string());
            }
        }
        HeldKind::Herd => {
            let id = held.herd.as_deref().ok_or("a herd needs 'herd'")?;
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let standing = world
                .resource::<HerdRegistry>()
                .find(id)
                .cloned()
                .ok_or("the herd vanished while it was being climbed")?;
            // A pen is fenced where a Corral queued by the faction's band nearest the herd would
            // fence it: the same `pen_site_in_world` a queued Corral stamps, so the footprint (and
            // so the herd's K) is the one play gives, and the pens written earlier in this load are
            // in its taken list. The herd's own position is the fallback only where that pick finds
            // nothing, or the faction has no band.
            let pen_tile = if laid.trait_set().has(RungTrait::Enclosed) {
                standing
                    .pen_site
                    .or_else(|| {
                        nearest_band_tile(world, faction, standing.position())
                            .and_then(|band_tile| pen_site_in_world(world, &standing, band_tile))
                    })
                    .unwrap_or_else(|| standing.position())
            } else {
                standing.position()
            };
            let mut registry = world.resource_mut::<HerdRegistry>();
            let herd = registry
                .herds
                .iter_mut()
                .find(|herd| herd.id == id)
                .ok_or("the herd vanished while it was being climbed")?;
            if !herd.complete_rung_as_built(faction, laid, &fauna, ladder, pen_tile) {
                return Err("the herd could not be raised (another people owns it)".to_string());
            }
        }
        HeldKind::Tile => {
            let tile = tile.ok_or("a tile store needs 'at'")?;
            // The store is kept by the band of the faction nearest it, as `build_rung`'s keeper would
            // be: the owner a store needs to hold food (`settlement.rs` counts only an owned store).
            let owner = nearest_band(world, faction, tile).map(|(id, _)| id);
            let (base, width) = tile_rung_span(laid_id, ladder);
            world
                .resource_mut::<TileImprovementRegistry>()
                .place_owned(tile, laid_id.branch, faction, owner, ladder)
                .set_position(base + width, ladder);
        }
        HeldKind::Deposit => {
            let tile = tile.ok_or("a working needs 'at'")?;
            let material = held
                .material
                .as_deref()
                .ok_or("a working needs 'material'")?;
            world
                .resource_mut::<DepositRegistry>()
                .source_mut(tile, material)
                .ok_or("the working vanished while it was being climbed")?
                .complete_rung_as_built(laid_id, ladder);
        }
    }
    Ok(())
}

/// The held rung of a source, when it is above its branch's root: every source holds its root, and a
/// scenario cannot hold it twice.
fn held_above_root(ladder: &LadderConfig, standing: &crate::intensification::RungStanding) -> bool {
    ladder.rung_by_id(standing.held).is_some_and(|def| {
        ladder
            .root_of(def.branch)
            .is_some_and(|root| def.order > root.order)
    })
}

/// **The held rungs of one faction**, as `held` entries: its patches (with the crop they are committed
/// to), its herds, and the workings it has a crew row on. Patches by `(y, x)`, herds in export order,
/// workings by `(x, y, material)`. A working has no owner, so it is the faction whose band works it.
pub(super) fn export_held(
    world: &World,
    faction: FactionId,
    ids: &ExportIds,
    notes: &mut Vec<String>,
) -> Vec<RungEntry> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    let mut entries = Vec::new();
    let mut patches: Vec<(UVec2, &crate::forage::ForagePatch)> = world
        .resource::<ForageRegistry>()
        .patches
        .iter()
        .filter(|(_, patch)| patch.owner == Some(faction))
        .map(|(tile, patch)| (*tile, patch))
        .collect();
    patches.sort_by_key(|(tile, _)| (tile.y, tile.x));
    for (tile, patch) in patches {
        let standing = patch.standing();
        if held_above_root(&ladder, &standing) {
            entries.push(RungEntry::Held(HeldRung {
                rung: standing.held.wire_key(),
                at: Some([tile.x, tile.y]),
                herd: None,
                material: None,
                species: patch.species.clone(),
            }));
        }
    }
    let mut herds: Vec<(&str, &crate::fauna::Herd)> = world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .filter(|herd| herd.owner == Some(faction))
        .filter_map(|herd| Some((ids.herd(&herd.id)?, herd)))
        .collect();
    herds.sort_by_key(|(scenario_id, _)| {
        scenario_id
            .trim_start_matches(super::HERD_ID_PREFIX)
            .parse::<usize>()
            .unwrap_or(usize::MAX)
    });
    for (scenario_id, herd) in herds {
        let standing = herd.standing();
        if held_above_root(&ladder, &standing) {
            entries.push(RungEntry::Held(HeldRung {
                rung: standing.held.wire_key(),
                at: None,
                herd: Some(scenario_id.to_string()),
                material: None,
                species: None,
            }));
        }
    }
    let worked: BTreeSet<(u32, u32, String)> = ids
        .faction_bands
        .get(&faction)
        .into_iter()
        .flatten()
        .filter_map(|band| world.get::<crate::components::LaborAllocation>(band.entity))
        .flat_map(|allocation| allocation.assignments.iter())
        .filter_map(|row| match &row.target {
            LaborTarget::Extract { tile, material, .. } => Some((tile.x, tile.y, material.clone())),
            _ => None,
        })
        .collect();
    for (key, source) in &world.resource::<DepositRegistry>().sources {
        if worked.contains(key) && held_above_root(&ladder, source.standing()) {
            entries.push(RungEntry::Held(HeldRung {
                rung: source.rung().wire_key(),
                at: Some([key.0, key.1]),
                herd: None,
                material: Some(key.2.clone()),
                species: None,
            }));
        }
    }
    let _ = notes;
    entries
}

/// **The declared builds of one faction, and its pen verbs in flight.**
///
/// A climb declared on a patch, herd, working, road, tile or band improvement is a `build_rung` entry
/// (deduplicated across the bands the declaration is queued on, the first band in `BandId` order
/// naming a road or a band improvement). A pen ring in flight is an `extend_pen` and an output
/// commitment in flight a `set_herd_output` at its target share. A finished ring or commitment has
/// no key: it is noted.
pub(super) fn export_builds_and_pens(
    world: &World,
    faction: FactionId,
    ids: &ExportIds,
    notes: &mut Vec<String>,
) -> (Vec<RungEntry>, Vec<super::pens::PenEntry>) {
    use super::pens::{ExtendPenEntry, PenEntry, SetHerdOutputEntry};
    use crate::components::{BuildJob, BuildSource, LaborAllocation};
    let ladder = world.resource::<LadderConfigHandle>().get();
    let mut builds = Vec::new();
    let mut pens: Vec<PenEntry> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for band in ids.faction_bands.get(&faction).into_iter().flatten() {
        let Some(allocation) = world.get::<LaborAllocation>(band.entity) else {
            continue;
        };
        for declaration in &allocation.builds {
            let key = format!("{:?}/{:?}", declaration.source, declaration.declared);
            if !seen.insert(key) {
                continue;
            }
            match declaration.declared {
                BuildJob::Climb(rung) => {
                    let Some(verb) = ladder.rung_by_id(rung).and_then(|def| def.verb.clone())
                    else {
                        continue;
                    };
                    let mut entry = BuildRungEntry {
                        band: None,
                        verb,
                        target_x: None,
                        target_y: None,
                        herd_id: None,
                        material: None,
                        branch: None,
                    };
                    match &declaration.source {
                        BuildSource::Patch(tile) | BuildSource::Tile { tile, .. } => {
                            entry.target_x = Some(tile.x);
                            entry.target_y = Some(tile.y);
                        }
                        BuildSource::Herd(id) => match ids.herd(id) {
                            Some(scenario_id) => entry.herd_id = Some(scenario_id.to_string()),
                            None => {
                                notes.push(format!(
                                    "a build declared on herd {id}, which is not exported, is left out"
                                ));
                                continue;
                            }
                        },
                        BuildSource::Road(tile) => {
                            entry.band = Some(band.scenario_id.clone());
                            entry.target_x = Some(tile.x);
                            entry.target_y = Some(tile.y);
                        }
                        BuildSource::Deposit { tile, material } => {
                            entry.target_x = Some(tile.x);
                            entry.target_y = Some(tile.y);
                            entry.material = Some(material.clone());
                        }
                        BuildSource::Band { band: held_by, .. } => match ids.band(*held_by) {
                            Some(scenario_id) => entry.band = Some(scenario_id.to_string()),
                            None => {
                                notes.push(format!(
                                    "a build declared on band {}, which is not exported, is left out",
                                    held_by.0
                                ));
                                continue;
                            }
                        },
                    }
                    builds.push(RungEntry::BuildRung(entry));
                }
                BuildJob::ExtendPen | BuildJob::SetHerdOutput(_) => {
                    let BuildSource::Herd(id) = &declaration.source else {
                        continue;
                    };
                    let Some(scenario_id) = ids.herd(id) else {
                        notes.push(format!(
                            "a pen build declared on herd {id}, which is not exported, is left out"
                        ));
                        continue;
                    };
                    if declaration.declared == BuildJob::ExtendPen {
                        pens.push(PenEntry::ExtendPen(ExtendPenEntry {
                            herd: scenario_id.to_string(),
                        }));
                    } else if let Some(target) = world
                        .resource::<HerdRegistry>()
                        .find(id)
                        .and_then(|herd| herd.standing_output_target)
                    {
                        pens.push(PenEntry::SetHerdOutput(SetHerdOutputEntry {
                            herd: scenario_id.to_string(),
                            fraction: round_export(target),
                        }));
                    }
                }
            }
        }
    }
    for herd in &world.resource::<HerdRegistry>().herds {
        if herd.owner != Some(faction) {
            continue;
        }
        let Some(scenario_id) = ids.herd(&herd.id) else {
            continue;
        };
        if herd.pen_radius > 0 {
            notes.push(format!(
                "{scenario_id}: its pen is {} ring(s) wide, which a scenario cannot say (an \
                 extend_pen starts one ring)",
                herd.pen_radius
            ));
        }
        if herd.standing_output_fraction > 0.0 {
            notes.push(format!(
                "{scenario_id}: {:.2} of it stands committed to milk, eggs and wool, which a \
                 scenario cannot say (a set_herd_output starts one commitment)",
                herd.standing_output_fraction
            ));
        }
    }
    (builds, pens)
}

/// **Pass 2, step 6: the `build_rung` entries**, one faction's worth, in file order, through the
/// command seam. **A refusal stops the load at that entry**, with the handler's own reason.
pub(super) fn apply_builds(
    world: &mut World,
    setup: &FactionSetup,
    spawned: &BTreeMap<String, BandId>,
    herds: &BTreeMap<String, String>,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    for (j, entry) in setup.rungs.iter().enumerate() {
        let RungEntry::BuildRung(build) = entry else {
            continue;
        };
        let at = format!("factions[{}].rungs[{j}]", setup.file_index);
        let band_id = match &build.band {
            Some(id) => match spawned.get(id) {
                Some(band) => Some(band.0),
                None => {
                    refusals.push(ScenarioRefusal::new(
                        format!("{at}.build_rung.band"),
                        format!("band '{id}' was not spawned"),
                    ));
                    return;
                }
            },
            None => None,
        };
        let herd_id = match &build.herd_id {
            Some(id) => match herds.get(id) {
                Some(runtime_id) => Some(runtime_id.clone()),
                None => {
                    refusals.push(ScenarioRefusal::new(
                        format!("{at}.build_rung.herd_id"),
                        format!("herd '{id}' was not applied"),
                    ));
                    return;
                }
            },
            None => None,
        };
        let request = BuildRungRequest {
            band_id,
            verb: build.verb.clone(),
            target_x: build.target_x,
            target_y: build.target_y,
            herd_id,
            material: build.material.clone(),
            branch: build.branch.clone(),
        };
        if let Err(refusal) = apply_build_rung(world, setup.faction, &request) {
            refusals.push(ScenarioRefusal::new(at, refusal.to_string()));
            return;
        }
    }
}
