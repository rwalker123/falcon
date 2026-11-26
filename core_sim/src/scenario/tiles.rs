//! **The `tiles` section of a scenario** (`docs/plan_scenarios.md` section 2.6, slice 2a): a tile the
//! scenario is about says what grows on it, instead of hoping the seed makes it so.
//!
//! `{ "at": [32, 18], "terrain": "AlluvialPlain", "flora": ["wild_emmer"] }`. An override is **a
//! worldgen input, not a post-hoc edit**: [`apply_scenario_tile_overrides`] runs in `Startup` after
//! every pass that rewrites terrain and before everything that derives from it, so the food module,
//! the forage patch, the herds and the graze layer are all built from the overridden tile.
//!
//! **Only land-to-land biome swaps and a flora basket have a re-derivation point.** Elevation is the
//! terrain authority and the land mask, the coast, the rivers and the mountain metadata derive from it,
//! so an override that made a tile water, or a mountain, or took either away, has nowhere to re-derive
//! those from and is refused.

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use sim_runtime::{TerrainTags, TerrainType};

use super::{ScenarioContext, ScenarioRefusal, ScenarioSetup};
use crate::components::Tile;
use crate::flora_config::FloraConfigHandle;
use crate::resources::TileRegistry;
use crate::{terrain_definition, BiomeNiche};

/// One tile the file overrides.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TileOverride {
    /// The tile, `[x, y]`.
    pub at: [u32; 2],
    /// A `TerrainType` name (`"AlluvialPlain"`). Absent keeps the generated biome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terrain: Option<String>,
    /// The plants that grow here, as `flora_config.json` species keys. Absent keeps the generated
    /// basket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flora: Option<Vec<String>>,
}

impl TileOverride {
    /// The terrain named, parsed by the type's own spelling (its serde variant name) — the one table.
    pub fn parsed_terrain(&self) -> Option<TerrainType> {
        self.terrain.as_deref().and_then(parse_terrain)
    }

    /// The flora keys, sorted and deduplicated — the form the tile stores.
    pub fn sorted_flora(&self) -> Option<Vec<String>> {
        self.flora.as_ref().map(|keys| {
            keys.iter()
                .cloned()
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect()
        })
    }
}

/// A terrain name as `TerrainType` itself spells it.
fn parse_terrain(name: &str) -> Option<TerrainType> {
    TerrainType::deserialize(IntoDeserializer::<serde::de::value::Error>::into_deserializer(name))
        .ok()
}

/// **Is `terrain` one the elevation field decides?** The highland and volcanic biomes: relief and the
/// mountain mask place them, so a tile may neither be swapped TO one nor, wherever the generated
/// map put one, swapped AWAY from it. The one predicate both directions of the override ask.
pub fn is_elevation_derived_highland(terrain: TerrainType) -> bool {
    matches!(
        terrain_definition(terrain).niche,
        BiomeNiche::Highland | BiomeNiche::Volcanic
    )
}

/// **Why a tile may not be CHANGED TO `terrain`**, or `None` when it may: water, a navigable river
/// and the highland and volcanic biomes (the ones the mountain mask places) all derive from
/// elevation, which an override does not touch.
pub fn terrain_target_refusal(terrain: TerrainType) -> Option<String> {
    let definition = terrain_definition(terrain);
    if terrain == TerrainType::NavigableRiver || definition.tags.contains(TerrainTags::WATER) {
        return Some(format!(
            "{terrain:?} is water, and the coast and the rivers derive from elevation: a tile can only be \
             swapped to another land biome"
        ));
    }
    if is_elevation_derived_highland(terrain) {
        return Some(format!(
            "{terrain:?} is a mountain biome, and mountains derive from elevation: a tile can only be \
             swapped to another lowland biome"
        ));
    }
    None
}

/// Pass 1 for the `tiles` section: names resolved, every fault collected, no world built.
pub(super) fn check(
    tiles: &[TileOverride],
    (width, height): (u32, u32),
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let mut seen: BTreeSet<[u32; 2]> = BTreeSet::new();
    for (k, entry) in tiles.iter().enumerate() {
        let at = format!("tiles[{k}]");
        let [x, y] = entry.at;
        if x >= width || y >= height {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.at"),
                format!("tile ({x},{y}) is outside the {width}x{height} map"),
            ));
        }
        if !seen.insert(entry.at) {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.at"),
                format!("tile ({x},{y}) is listed twice"),
            ));
        }
        if entry.terrain.is_none() && entry.flora.is_none() {
            refusals.push(ScenarioRefusal::new(
                &at,
                "name a terrain, a flora basket or both: an entry with neither changes nothing",
            ));
        }
        if let Some(name) = &entry.terrain {
            match parse_terrain(name) {
                None => refusals.push(ScenarioRefusal::new(
                    format!("{at}.terrain"),
                    format!("'{name}' is not a terrain"),
                )),
                Some(terrain) => {
                    if let Some(reason) = terrain_target_refusal(terrain) {
                        refusals.push(ScenarioRefusal::new(format!("{at}.terrain"), reason));
                    }
                }
            }
        }
        if let Some(keys) = &entry.flora {
            if keys.is_empty() {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.flora"),
                    "name at least one plant, or leave flora out to keep the generated basket",
                ));
            }
            let mut listed: BTreeSet<&str> = BTreeSet::new();
            for (l, key) in keys.iter().enumerate() {
                if !ctx.flora.species.contains_key(key) {
                    refusals.push(ScenarioRefusal::new(
                        format!("{at}.flora[{l}]"),
                        format!("'{key}' is not a plant on the flora roster"),
                    ));
                }
                if !listed.insert(key.as_str()) {
                    refusals.push(ScenarioRefusal::new(
                        format!("{at}.flora[{l}]"),
                        format!("'{key}' is listed twice"),
                    ));
                }
            }
        }
    }
}

/// **Startup, after every pass that rewrites terrain and before everything that derives from it**
/// (`lib.rs`): fit each override against the generated tile and apply it. A misfit is recorded on the
/// [`ScenarioSetup`] and the tile is left as generated; no check is relaxed to make an entry fit.
///
/// A no-op when no scenario is being loaded.
pub fn apply_scenario_tile_overrides(
    setup: Option<ResMut<ScenarioSetup>>,
    registry: Res<TileRegistry>,
    flora: Res<FloraConfigHandle>,
    mut tiles: Query<&mut Tile>,
) {
    let Some(mut setup) = setup else {
        return;
    };
    if setup.tiles.is_empty() {
        return;
    }
    let flora = flora.get();
    let overrides = setup.tiles.clone();
    for (k, entry) in overrides.iter().enumerate() {
        let at = format!("tiles[{k}]");
        let [x, y] = entry.at;
        let Some(entity) = registry.index(x, y) else {
            setup.refuse(
                &at,
                format!(
                    "tile ({x},{y}) is outside the {}x{} map",
                    registry.width, registry.height
                ),
            );
            continue;
        };
        let Ok(mut tile) = tiles.get_mut(entity) else {
            continue;
        };
        if let Some(terrain) = entry.parsed_terrain() {
            let generated_water = tile.terrain == TerrainType::NavigableRiver
                || tile.terrain_tags.contains(TerrainTags::WATER);
            let generated_mountain =
                tile.mountain.is_some() || is_elevation_derived_highland(tile.terrain);
            if generated_water || generated_mountain {
                let what = if generated_water {
                    format!("{:?}", tile.terrain)
                } else {
                    format!("a mountain biome ({:?})", tile.terrain)
                };
                setup.refuse(
                    &at,
                    format!(
                        "tile ({x},{y}) is {what}, which derives from elevation: only a lowland tile can change biome"
                    ),
                );
                continue;
            }
            tile.terrain = terrain;
            // The same re-derivation `apply_biome_palette_clamp` does for a repainted tile.
            tile.terrain_tags = terrain_definition(terrain).tags;
        }
        if let Some(keys) = entry.sorted_flora() {
            let biome = tile.resource_terrain();
            let hosted = flora.composition(biome);
            if let Some(missing) = keys
                .iter()
                .find(|key| !hosted.iter().any(|share| &share.species == *key))
            {
                setup.refuse(
                    &at,
                    format!("'{missing}' does not grow on {biome:?} at ({x},{y})"),
                );
                continue;
            }
            tile.flora_override = Some(keys);
        }
    }
}
