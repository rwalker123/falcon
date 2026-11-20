//! **The `herds` section of a scenario** (`docs/plan_scenarios.md` sections 2.2 and 2.8, slice 2c): the
//! wild herds a scenario puts on the map, or sets on the ones worldgen already made.
//!
//! A placed herd belongs to no faction, so the section is a top-level sibling of `factions`. Pass 1
//! ([`check`]) resolves names and ranges with no world; the fit ([`fit`]) runs on the built world and
//! collects every refusal BEFORE anything is applied ([`apply`]), so a refused file changes nothing.
//! Herds are a setter (step 5 of the apply order), so they land before the outfit replay.

use std::collections::{BTreeMap, BTreeSet};

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use rand::{rngs::SmallRng, SeedableRng};
use serde::Deserialize;

use super::{ScenarioContext, ScenarioRefusal, WorldSection};
use crate::fauna::{
    build_migratory_route, build_prey_index, build_short_route, habitat_refusal,
    herd_above_extinction_floor, herd_capacity, herd_ecology, herd_from_species,
    suitable_tiles_by_module, suitable_tiles_for, Herd, HerdDensityMap, HerdRegistry,
    HerdTelemetry, PreyReach, RoamState, SeedTile,
};
use crate::fauna_config::{FaunaConfigHandle, SpeciesDef};
use crate::grid_utils::hex_distance_wrapped;
use crate::intensification::{LadderConfigHandle, RungStanding, SourceKind};
use crate::resources::{SimulationConfig, TileRegistry};
use crate::Tile;

/// Salt mixed into the map seed for a placed herd's route rolls, so a scenario herd's rolls are a pure
/// function of the seed and its entry and never move a worldgen draw.
pub const SCENARIO_HERD_SEED_SALT: u64 = 0x5CE4_A410_4E2D_0001;

/// The prefix of a placed herd's runtime id (`scenario_<file id>`). Deliberately neither `game_` nor
/// `herd_`, so a scenario herd never counts against `abundance.max_total_game`.
pub const SCENARIO_HERD_ID_PREFIX: &str = "scenario_";

/// One herd entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HerdEntry {
    /// The herd's id inside this file. Later sections name a herd by it.
    pub id: String,
    /// A `fauna_config.json` species KEY (`aurochs`), not the display name.
    pub species: String,
    /// The tile, `[x, y]`.
    pub at: [u32; 2],
    /// Biomass as a fraction of the herd's carrying capacity, in `(0, 1]`.
    pub stock: f32,
    pub origin: HerdOrigin,
}

/// Where the herd comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HerdOrigin {
    /// A new wild herd of the species is put on the tile.
    Placed,
    /// The herd the generated world already has there is set to `stock`.
    Worldgen,
}

/// The herd entries a candidate world applies, installed with the bands' setup and removed by
/// [`super::finish_scenario`].
#[derive(Resource, Debug, Clone, Default)]
pub struct ScenarioHerdSetup {
    pub herds: Vec<HerdEntry>,
}

/// Pass 1 for the herds section: names and ranges, every fault collected.
pub(super) fn check(
    herds: &[HerdEntry],
    world: &WorldSection,
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (i, entry) in herds.iter().enumerate() {
        let at = format!("herds[{i}]");
        if entry.id.is_empty() {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.id"),
                "a herd needs an id",
            ));
        } else if !seen.insert(entry.id.as_str()) {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.id"),
                format!("herd id '{}' is used twice", entry.id),
            ));
        }
        if !ctx.fauna.species.contains_key(&entry.species) {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.species"),
                format!("'{}' is not a fauna species", entry.species),
            ));
        }
        if !entry.stock.is_finite() || entry.stock <= 0.0 || entry.stock > 1.0 {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.stock"),
                format!(
                    "stock {} must be above 0 and at most 1 (a fraction of capacity)",
                    entry.stock
                ),
            ));
        }
        if entry.at[0] >= world.width || entry.at[1] >= world.height {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.at"),
                format!(
                    "({}, {}) is off the {}x{} map",
                    entry.at[0], entry.at[1], world.width, world.height
                ),
            ));
        }
    }
}

/// A herd entry that fitted, ready to apply.
pub(super) enum Fitted {
    /// A new herd, fully built.
    Place { id: String, herd: Box<Herd> },
    /// An existing worldgen herd, set to `biomass`.
    Set {
        id: String,
        runtime_id: String,
        biomass: f32,
    },
}

/// What the fit reads off the built world, in one parameter set.
type FitParams<'w, 's> = (
    Query<'w, 's, &'static Tile>,
    Res<'w, TileRegistry>,
    Res<'w, HerdRegistry>,
    Res<'w, FaunaConfigHandle>,
    Res<'w, LadderConfigHandle>,
    Res<'w, SimulationConfig>,
);

/// **Fit every herd entry to the built world, collecting every refusal. Nothing is changed.**
pub(super) fn fit(world: &mut World, entries: &[HerdEntry]) -> (Vec<Fitted>, Vec<ScenarioRefusal>) {
    let mut fitted = Vec::new();
    let mut refusals = Vec::new();
    if entries.is_empty() {
        return (fitted, refusals);
    }
    let mut state: SystemState<FitParams> = SystemState::new(world);
    let (tiles, registry, herds, fauna_handle, ladder_handle, config) = state.get(world);
    let fauna = fauna_handle.get();
    let ladder = ladder_handle.get();
    let root = RungStanding::unstarted_on(&ladder, ladder.branch_of_kind(SourceKind::Herd));
    let width = config.grid_size.x;
    let height = config.grid_size.y;
    let wrap = config.map_topology.wrap_horizontal;
    let prey_index = build_prey_index(&herds.herds, &fauna);
    let reach = PreyReach {
        index: &prey_index,
        radius: fauna.predators.prey_sense_radius,
    };
    let mut suitable_by_module = None;
    let mut targeted: BTreeMap<String, usize> = BTreeMap::new();
    for (i, entry) in entries.iter().enumerate() {
        let Some(def) = fauna.species.get(&entry.species) else {
            continue;
        };
        let at = UVec2::new(entry.at[0], entry.at[1]);
        if at.x >= width || at.y >= height {
            continue;
        }
        match entry.origin {
            HerdOrigin::Placed => {
                if let Some(reason) =
                    habitat_refusal(def, at, width, height, wrap, &registry, &tiles, &reach)
                {
                    refusals.push(ScenarioRefusal::new(
                        format!("herds[{i}].at"),
                        format!("{} at ({},{}): {reason}", entry.species, at.x, at.y),
                    ));
                    continue;
                }
                let mut rng =
                    SmallRng::seed_from_u64(config.map_seed ^ SCENARIO_HERD_SEED_SALT ^ i as u64);
                let steps = def.sample_route_len(&mut rng);
                let route = if def.migratory {
                    let by_module = suitable_by_module.get_or_insert_with(|| {
                        suitable_tiles_by_module(width, height, &registry, &tiles)
                    });
                    let suitable = suitable_tiles_for(def, by_module);
                    build_migratory_route(
                        at,
                        width,
                        height,
                        &registry,
                        &tiles,
                        &fauna.graze,
                        &suitable,
                        SeedTile::At(at),
                        &mut rng,
                        steps,
                        wrap,
                    )
                } else {
                    build_short_route(at, steps, width, height, &registry, &tiles, &mut rng)
                };
                let Some(route) = route.filter(|route| route.first() == Some(&at)) else {
                    refusals.push(ScenarioRefusal::new(
                        format!("herds[{i}].at"),
                        format!(
                            "{} at ({},{}): no route can be built that starts on this tile",
                            entry.species, at.x, at.y
                        ),
                    ));
                    continue;
                };
                let capacity = def.carrying_capacity();
                let mut herd = herd_from_species(
                    format!("{SCENARIO_HERD_ID_PREFIX}{}", entry.id),
                    def,
                    route,
                    entry.stock * capacity,
                    capacity,
                    &fauna,
                    root,
                );
                if def.migratory {
                    herd.roam = RoamState::Loiter {
                        turns_left: def.sample_loiter_turns(&mut rng),
                    };
                }
                if !herd_above_extinction_floor(&herd, &fauna) {
                    refusals.push(floor_refusal(i, entry, &herd, &fauna));
                    continue;
                }
                fitted.push(Fitted::Place {
                    id: entry.id.clone(),
                    herd: Box::new(herd),
                });
            }
            HerdOrigin::Worldgen => {
                let standing: Vec<&Herd> = herds
                    .herds
                    .iter()
                    .filter(|herd| herd.species == def.display_name && herd.position() == at)
                    .collect();
                let herd = match standing.as_slice() {
                    [] => {
                        refusals.push(ScenarioRefusal::new(
                            format!("herds[{i}].at"),
                            no_herd_there(def, at, &herds, width, wrap),
                        ));
                        continue;
                    }
                    [herd] => *herd,
                    many => {
                        refusals.push(ScenarioRefusal::new(
                            format!("herds[{i}].at"),
                            format!(
                                "{} herds of {} stand at ({},{}); the entry is ambiguous",
                                many.len(),
                                def.display_name,
                                at.x,
                                at.y
                            ),
                        ));
                        continue;
                    }
                };
                if herd.owner.is_some() {
                    refusals.push(ScenarioRefusal::new(
                        format!("herds[{i}].at"),
                        format!(
                            "the {} at ({},{}) is owned; a scenario sets only a wild herd",
                            def.display_name, at.x, at.y
                        ),
                    ));
                    continue;
                }
                if let Some(first) = targeted.insert(herd.id.clone(), i) {
                    refusals.push(ScenarioRefusal::new(
                        format!("herds[{i}].at"),
                        format!(
                            "the herd at ({},{}) is already set by herds[{first}]",
                            at.x, at.y
                        ),
                    ));
                    continue;
                }
                let mut set = herd.clone();
                set.biomass = entry.stock * herd_capacity(&set, &fauna);
                if !herd_above_extinction_floor(&set, &fauna) {
                    refusals.push(floor_refusal(i, entry, &set, &fauna));
                    continue;
                }
                fitted.push(Fitted::Set {
                    id: entry.id.clone(),
                    runtime_id: herd.id.clone(),
                    biomass: set.biomass,
                });
            }
        }
    }
    (fitted, refusals)
}

/// The refusal for a stock the herd would be dispersed under on its first turn.
fn floor_refusal(
    index: usize,
    entry: &HerdEntry,
    herd: &Herd,
    fauna: &crate::fauna_config::FaunaConfig,
) -> ScenarioRefusal {
    ScenarioRefusal::new(
        format!("herds[{index}].stock"),
        format!(
            "stock {} is at or under the extinction floor ({}) the herd is dispersed below",
            entry.stock,
            herd_ecology(herd, fauna).extinction_floor
        ),
    )
}

/// `no <display name> at (x,y); the nearest is at (a,b)`, or `... none on the map`.
fn no_herd_there(
    def: &SpeciesDef,
    at: UVec2,
    herds: &HerdRegistry,
    width: u32,
    wrap: bool,
) -> String {
    let nearest = herds
        .herds
        .iter()
        .filter(|herd| herd.species == def.display_name)
        .map(|herd| herd.position())
        .min_by_key(|pos| (hex_distance_wrapped(at, *pos, width, wrap), pos.y, pos.x));
    match nearest {
        Some(pos) => format!(
            "no {} at ({},{}); the nearest is at ({},{})",
            def.display_name, at.x, at.y, pos.x, pos.y
        ),
        None => format!(
            "no {} at ({},{}); none on the map",
            def.display_name, at.x, at.y
        ),
    }
}

/// **Apply the fitted herds**, then republish them the way `spawn_initial_herds` does. Returns the
/// scenario id to runtime herd id map.
pub(super) fn apply(world: &mut World, fitted: Vec<Fitted>) -> BTreeMap<String, String> {
    let mut applied = BTreeMap::new();
    if fitted.is_empty() {
        return applied;
    }
    let fauna = world.resource::<FaunaConfigHandle>().get();
    let ladder = world.resource::<LadderConfigHandle>().get();
    let grid_size = world.resource::<SimulationConfig>().grid_size;
    let entries = {
        let mut registry = world.resource_mut::<HerdRegistry>();
        for item in fitted {
            match item {
                Fitted::Place { id, herd } => {
                    applied.insert(id, herd.id.clone());
                    registry.herds.push(*herd);
                }
                Fitted::Set {
                    id,
                    runtime_id,
                    biomass,
                } => {
                    if let Some(herd) = registry.herds.iter_mut().find(|herd| herd.id == runtime_id)
                    {
                        herd.biomass = biomass;
                        herd.refresh_ecology_phase(&fauna);
                    }
                    applied.insert(id, runtime_id);
                }
            }
        }
        registry.snapshot_entries(&ladder)
    };
    world.resource_mut::<HerdTelemetry>().entries = entries;
    world.resource_scope(|world, mut density: Mut<HerdDensityMap>| {
        density.rebuild(grid_size, world.resource::<HerdRegistry>());
    });
    applied
}
