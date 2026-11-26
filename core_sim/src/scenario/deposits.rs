//! **The `deposits` section of a scenario** (`docs/plan_scenarios.md` section 2.2, slice 2d): a setter
//! for a deposit's remaining stock, and only that.
//!
//! A deposit has no owner (the registry is keyed by tile and material), so the section is world
//! state at the top of the file. Pass 1 resolves the material names; the fit (section 2.6) reads
//! the built world; the apply opens each working through [`DepositRegistry::open`], the get-or-insert
//! every other setter on a working also goes through, so two setters compose in either order.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Deserialize;

use super::{ScenarioContext, ScenarioRefusal};
use crate::components::Tile;
use crate::extraction::{tile_deposit_capacity, DepositRegistry};
use crate::extraction_config::{ExtractionConfigHandle, NO_DEPOSIT};
use crate::intensification::LadderConfigHandle;
use crate::resources::TileRegistry;

/// One working's remaining stock.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositEntry {
    /// The tile the working stands on, `[x, y]`.
    pub at: [u32; 2],
    /// An `extraction.json` deposit id.
    pub material: String,
    /// The standing stock, in the material's own units. Within `0..=` the tile's capacity.
    pub stock: f32,
}

/// Pass 1: names resolved against the extraction config, every fault collected.
pub(super) fn check(
    entries: &[DepositEntry],
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let mut seen: BTreeMap<([u32; 2], &str), usize> = BTreeMap::new();
    for (i, entry) in entries.iter().enumerate() {
        let at = format!("deposits[{i}]");
        if ctx.extraction.deposit(&entry.material).is_none() {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.material"),
                format!("'{}' is not a deposit", entry.material),
            ));
        }
        if !entry.stock.is_finite() || entry.stock < 0.0 {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.stock"),
                "a stock cannot be negative",
            ));
        }
        if let Some(first) = seen.insert((entry.at, entry.material.as_str()), i) {
            refusals.push(ScenarioRefusal::new(
                &at,
                format!(
                    "({},{}) {} is already set at deposits[{first}]",
                    entry.at[0], entry.at[1], entry.material
                ),
            ));
        }
    }
}

/// The ground an entry names, if it is on the map.
fn ground_of(world: &World, at: [u32; 2]) -> Option<Tile> {
    let entity = world.resource::<TileRegistry>().index(at[0], at[1])?;
    world.get::<Tile>(entity).cloned()
}

/// **The fit (section 2.6), before anything is applied**: on the map, the tile holds the material,
/// and the stock fits under the tile's capacity.
pub(super) fn fit(world: &World, entries: &[DepositEntry], refusals: &mut Vec<ScenarioRefusal>) {
    let config = world.resource::<ExtractionConfigHandle>().get();
    let registry = world.resource::<TileRegistry>();
    let (width, height) = (registry.width, registry.height);
    for (i, entry) in entries.iter().enumerate() {
        let at = format!("deposits[{i}]");
        let [x, y] = entry.at;
        let Some(ground) = ground_of(world, entry.at) else {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.at"),
                format!("tile ({x},{y}) is outside the {width}x{height} map"),
            ));
            continue;
        };
        let capacity = tile_deposit_capacity(&config, &entry.material, &ground);
        if capacity <= NO_DEPOSIT {
            refusals.push(ScenarioRefusal::new(
                &at,
                format!("({x},{y}) holds no {}", entry.material),
            ));
        } else if entry.stock > capacity {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.stock"),
                format!(
                    "{} is over this tile's {} capacity of {capacity}",
                    entry.stock, entry.material
                ),
            ));
        }
    }
}

/// **Apply**: open each working and write its stock, nothing else.
pub(super) fn apply(
    world: &mut World,
    entries: &[DepositEntry],
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let config = world.resource::<ExtractionConfigHandle>().get();
    let ladder = world.resource::<LadderConfigHandle>().get();
    for (i, entry) in entries.iter().enumerate() {
        let Some(ground) = ground_of(world, entry.at) else {
            continue;
        };
        let tile = UVec2::new(entry.at[0], entry.at[1]);
        let mut registry = world.resource_mut::<DepositRegistry>();
        match registry.open(tile, &entry.material, &ground, &config, &ladder) {
            Some(source) => source.stock = entry.stock,
            // The fit has passed, so a tile that holds the material opens; this is a guard.
            None => refusals.push(ScenarioRefusal::new(
                format!("deposits[{i}]"),
                format!(
                    "({},{}) holds no {}",
                    entry.at[0], entry.at[1], entry.material
                ),
            )),
        }
    }
}
