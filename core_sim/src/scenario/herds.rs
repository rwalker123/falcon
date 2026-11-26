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
    herd_above_extinction_floor, herd_capacity, herd_ecology, herd_from_species, regrow_biomass,
    suitable_tiles_by_module, suitable_tiles_for, wild_herd_capacity, Herd, HerdDensityMap,
    HerdRegistry, HerdTelemetry, PreyReach, RoamState, SeedTile,
};
use crate::fauna_config::{Diet, FaunaConfigHandle, SpeciesDef};
use crate::graze::GrazeRegistry;
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
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HerdEntry {
    /// The herd's id inside this file. Later sections name a herd by it.
    pub id: String,
    /// A `fauna_config.json` species KEY (`aurochs`), not the display name.
    pub species: String,
    /// The tile, `[x, y]`.
    #[schemars(extend("x-pick" = "tile"))]
    pub at: [u32; 2],
    /// Biomass as a fraction of the K the herd lives under on its first turn (the ecological K the
    /// turn loop writes, for a placed and a worldgen herd alike), in `(0, 1]`.
    pub stock: f32,
    pub origin: HerdOrigin,
}

/// Where the herd comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
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
    Option<Res<'w, GrazeRegistry>>,
);

/// **Fit every herd entry to the built world, collecting every refusal. Nothing is changed.**
pub(super) fn fit(world: &mut World, entries: &[HerdEntry]) -> (Vec<Fitted>, Vec<ScenarioRefusal>) {
    let mut fitted = Vec::new();
    let mut refusals = Vec::new();
    if entries.is_empty() {
        return (fitted, refusals);
    }
    let mut state: SystemState<FitParams> = SystemState::new(world);
    let (tiles, registry, herds, fauna_handle, ladder_handle, config, graze) = state.get(world);
    let fauna = fauna_handle.get();
    let ladder = ladder_handle.get();
    let root = RungStanding::unstarted_on(&ladder, ladder.branch_of_kind(SourceKind::Herd));
    let width = config.grid_size.x;
    let height = config.grid_size.y;
    let wrap = config.map_topology.wrap_horizontal;
    let empty_graze = GrazeRegistry::default();
    let graze = graze.as_deref().unwrap_or(&empty_graze);
    // **The herds as the file leaves them**, which is what a placed carnivore's prey gate and K read:
    // the registry with each worldgen entry's stock set and each placed herbivore added.
    let mut working: Vec<Herd> = herds.herds.clone();
    // The prey index is empty until the carnivores' turn; no other diet reads it.
    let mut prey_index = Vec::new();
    let mut prey_indexed = false;
    // Order: worldgen stocks, then placed non-carnivores, then placed carnivores. File order holds
    // within each group (the sort is stable). Refusals are re-sorted into file order at the end.
    let group_of = |entry: &HerdEntry| match (entry.origin, fauna.species.get(&entry.species)) {
        (HerdOrigin::Worldgen, _) => 0,
        (HerdOrigin::Placed, Some(def)) if def.diet == Diet::Carnivore => 2,
        (HerdOrigin::Placed, _) => 1,
    };
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&i| group_of(&entries[i]));
    let mut suitable_by_module = None;
    let mut targeted: BTreeMap<String, usize> = BTreeMap::new();
    for i in order {
        let entry = &entries[i];
        if group_of(entry) == 2 && !prey_indexed {
            prey_index = build_prey_index(&working, &fauna);
            prey_indexed = true;
        }
        let reach = PreyReach {
            index: &prey_index,
            radius: fauna.predators.prey_sense_radius,
        };
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
                let mut herd = herd_from_species(
                    format!("{SCENARIO_HERD_ID_PREFIX}{}", entry.id),
                    def,
                    route,
                    0.0,
                    def.carrying_capacity(),
                    &fauna,
                    root,
                );
                // **K is the one the turn loop will write**, resolved through the same seam at this
                // tile, so `stock` is a fraction of the K the herd lives under and the floor test
                // below is the one its first turn applies. No fallback to the species table's K.
                let Some(capacity) = wild_herd_capacity(
                    &herd,
                    Some(def),
                    graze,
                    &prey_index,
                    &fauna,
                    width,
                    height,
                    wrap,
                ) else {
                    refusals.push(ScenarioRefusal::new(
                        format!("herds[{i}].at"),
                        format!(
                            "{} at ({},{}): the land's carrying capacity cannot be resolved here",
                            entry.species, at.x, at.y
                        ),
                    ));
                    continue;
                };
                herd.carrying_capacity = capacity;
                herd.biomass = entry.stock * capacity;
                herd.refresh_ecology_phase(&fauna);
                if def.migratory {
                    herd.roam = RoamState::Loiter {
                        turns_left: def.sample_loiter_turns(&mut rng),
                    };
                }
                if !survives_first_turn(&herd, &fauna) {
                    refusals.push(floor_refusal(i, entry, &herd, &fauna));
                    continue;
                }
                working.push(herd.clone());
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
                if !survives_first_turn(&set, &fauna) {
                    refusals.push(floor_refusal(i, entry, &set, &fauna));
                    continue;
                }
                if let Some(slot) = working.iter_mut().find(|other| other.id == herd.id) {
                    slot.biomass = set.biomass;
                }
                fitted.push(Fitted::Set {
                    id: entry.id.clone(),
                    runtime_id: herd.id.clone(),
                    biomass: set.biomass,
                });
            }
        }
    }
    // The groups ran out of file order; report in file order.
    refusals.sort_by_key(|refusal| entry_index_of(&refusal.path));
    (fitted, refusals)
}

/// The entry index in a `herds[N].field` path, for putting refusals back into file order.
fn entry_index_of(path: &str) -> usize {
    path.strip_prefix("herds[")
        .and_then(|rest| rest.split_once(']'))
        .and_then(|(index, _)| index.parse().ok())
        .unwrap_or(usize::MAX)
}

/// **Is the herd still above the extinction floor after its own first-turn ecology step?** The turn
/// loop runs [`regrow_biomass`] (growth, or the depensation loss under `collapse_fraction`) on the
/// herd before its `retain` floor test, so the stock is tested as the step leaves it, at the K the
/// herd carries. Only that step is modelled, not hunting or predation. The step is the turn loop's
/// own function run on a copy, so there is no second formula.
fn survives_first_turn(herd: &Herd, fauna: &crate::fauna_config::FaunaConfig) -> bool {
    let mut stepped = herd.clone();
    regrow_biomass(&mut stepped, fauna);
    herd_above_extinction_floor(&stepped, fauna)
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
            "stock {} would be dispersed on the herd's first turn: after that turn's growth or \
             depensation it is at or under the extinction floor ({} of its capacity)",
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
