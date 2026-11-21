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
//! Pass 1 ([`check`]) resolves names and collects every refusal. Pass 2 is [`apply_held`] and
//! [`apply_builds`], called by [`super::finish_scenario`] in the apply order of section 2.8.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::Deserialize;

use super::bands::{FactionSection, FactionSetup};
use super::{ScenarioContext, ScenarioRefusal};
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
use crate::intensification::{LadderConfig, RungDef, SourceKind};
use crate::labor_config::LaborConfigHandle;
use crate::orders::FactionId;
use crate::resources::{SimulationConfig, TileRegistry};
use crate::{HerdRegistry, LadderConfigHandle, RungId, Tile, VerbId};

/// One entry of a faction's `rungs` list.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungEntry {
    /// A source already holding a rung.
    Held(HeldRung),
    /// A `BuildRung` command, replayed after the outfit.
    BuildRung(BuildRungEntry),
}

/// A source held at a rung. Which fields it needs follows the rung's branch's source kind.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeldRung {
    /// A rung id as the ladder spells it, `plant:tended`.
    pub rung: String,
    /// The tile, for a patch or a working.
    pub at: Option<[u32; 2]>,
    /// The herd id, for a herd.
    pub herd: Option<String>,
    /// The material, for a working.
    pub material: Option<String>,
    /// The crop a patch is committed to, where a rung on the chain `requires_crop`.
    pub species: Option<String>,
}

/// `CommandPayload::BuildRung`, with no faction (the section's own) and the band named by its
/// scenario id.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRungEntry {
    /// The keeper band's scenario id.
    pub band: Option<String>,
    pub verb: String,
    pub target_x: Option<u32>,
    pub target_y: Option<u32>,
    pub herd_id: Option<String>,
    pub material: Option<String>,
    pub branch: Option<String>,
}

/// **The source kinds a scenario can hold a rung on.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeldKind {
    Patch,
    Herd,
    Deposit,
}

impl HeldKind {
    fn of(kind: Option<SourceKind>) -> Result<Self, String> {
        match kind {
            Some(SourceKind::Patch) => Ok(Self::Patch),
            Some(SourceKind::Herd) => Ok(Self::Herd),
            Some(SourceKind::Deposit) => Ok(Self::Deposit),
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
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let declared_bands: BTreeSet<&str> = factions
        .iter()
        .flat_map(|section| section.bands.iter().map(|band| band.id.as_str()))
        .collect();
    for (i, section) in factions.iter().enumerate() {
        let mut seen_sources: BTreeSet<String> = BTreeSet::new();
        for (j, entry) in section.rungs.iter().enumerate() {
            let at = format!("factions[{i}].rungs[{j}]");
            match entry {
                RungEntry::Held(held) => check_held(held, &at, ctx, &mut seen_sources, refusals),
                RungEntry::BuildRung(build) => {
                    check_build(build, &at, ctx, &declared_bands, refusals);
                }
            }
        }
    }
}

fn check_held(
    held: &HeldRung,
    at: &str,
    ctx: &ScenarioContext,
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
    refusals: &mut Vec<ScenarioRefusal>,
) {
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

/// **Pass 2, step 5: the held rungs**, in file order, one faction's worth. Every refusal is
/// collected; a refused entry does not stop the others being tried.
pub(super) fn apply_held(
    world: &mut World,
    setup: &FactionSetup,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    for (j, entry) in setup.rungs.iter().enumerate() {
        let RungEntry::Held(held) = entry else {
            continue;
        };
        let at = format!("factions[{}].rungs[{j}]", setup.file_index);
        if let Err(reason) = climb_held(world, setup.faction, held) {
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
            let mut registry = world.resource_mut::<HerdRegistry>();
            let herd = registry
                .herds
                .iter_mut()
                .find(|herd| herd.id == id)
                .ok_or("the herd vanished while it was being climbed")?;
            // A pen stands where the herd's own site says, else where the herd stands - the labor
            // arm's own fallback when no better ground is free.
            let pen_tile = herd.pen_site.unwrap_or_else(|| herd.position());
            if !herd.complete_rung_as_built(faction, laid, &fauna, ladder, pen_tile) {
                return Err("the herd could not be raised (another people owns it)".to_string());
            }
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

/// **Pass 2, step 6: the `build_rung` entries**, one faction's worth, in file order, through the
/// command seam. **A refusal stops the load at that entry**, with the handler's own reason.
pub(super) fn apply_builds(
    world: &mut World,
    setup: &FactionSetup,
    spawned: &BTreeMap<String, BandId>,
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
        let request = BuildRungRequest {
            band_id,
            verb: build.verb.clone(),
            target_x: build.target_x,
            target_y: build.target_y,
            herd_id: build.herd_id.clone(),
            material: build.material.clone(),
            branch: build.branch.clone(),
        };
        if let Err(refusal) = apply_build_rung(world, setup.faction, &request) {
            refusals.push(ScenarioRefusal::new(at, refusal.to_string()));
            return;
        }
    }
}
