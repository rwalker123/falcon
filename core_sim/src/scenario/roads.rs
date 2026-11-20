//! **The `roads` section of a scenario** (`docs/plan_scenarios.md` section 4, slice 2e): the road
//! rungs a faction already holds when the game opens.
//!
//! A held rung is history, not a build: no declaration, no labor row, no material drawn. A trail is
//! laid on the free floor; a built rung replays the climb through [`validate_road_verb`] (the gate
//! `grade` / `pave` run) so a scenario passes the same knowledge and ladder gates the command would,
//! and a refusal speaks in the game's own words. Keeping labor is NOT here: it is a `roadwork` row of
//! the `labor` section.

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde::Deserialize;

use super::bands::{FactionSection, FactionSetup, ScenarioSetup};
use super::{ScenarioContext, ScenarioRefusal, WorldSection};
use crate::build_gates::validate_road_verb;
use crate::components::{BandId, PopulationCohort, Tile};
use crate::intensification::{LadderConfig, RungDef, RUNG_UNSTARTED};
use crate::orders::FactionId;
use crate::resources::TileRegistry;
use crate::routes::{
    quote_road_remoteness, road_rung_span, route_rungs_in_climb_order, RoadKeeper, RoadRegistry,
    NEAR_ENOUGH_TO_KEEP,
};
use crate::LadderConfigHandle;

/// One held road tile.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoadEntry {
    /// The tile, `[x, y]`, exactly.
    pub at: [u32; 2],
    /// A rung id on the `route` branch, as the ladder config spells it.
    pub rung: String,
    /// The band that keeps the road: a band id of this faction. Required on a rung that declares a
    /// verb (a built rung), refused on one that does not (the free floor has no keeper).
    pub keeper: Option<String>,
}

fn find_rung<'a>(ladder: &'a LadderConfig, id: &str) -> Option<&'a RungDef> {
    route_rungs_in_climb_order(ladder)
        .into_iter()
        .find(|rung| rung.id == id)
}

/// Where a free-floor rung is held: the top of its span, which no remoteness prices.
fn free_floor_position(ladder: &LadderConfig, rung: &RungDef) -> f32 {
    let (base, width) = road_rung_span(rung.rung_id(), ladder, NEAR_ENOUGH_TO_KEEP);
    base + width
}

fn spoken(rung: &str) -> String {
    rung.replace('_', " ")
}

/// Pass 1 for the `roads` sections: every fault collected.
pub(super) fn check(
    factions: &[FactionSection],
    world: &WorldSection,
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let mut seen_tiles: BTreeSet<[u32; 2]> = BTreeSet::new();
    for (i, section) in factions.iter().enumerate() {
        let Some(roads) = &section.roads else {
            continue;
        };
        for (j, entry) in roads.iter().enumerate() {
            let at = format!("factions[{i}].roads[{j}]");
            let [x, y] = entry.at;
            if x >= world.width || y >= world.height {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.at"),
                    format!(
                        "tile ({x},{y}) is outside the {}x{} map",
                        world.width, world.height
                    ),
                ));
            }
            if !seen_tiles.insert(entry.at) {
                refusals.push(ScenarioRefusal::new(
                    format!("{at}.at"),
                    format!("tile ({x},{y}) is listed twice: one road per tile"),
                ));
            }
            check_rung_and_keeper(section, entry, &at, ctx, refusals);
        }
    }
}

fn check_rung_and_keeper(
    section: &FactionSection,
    entry: &RoadEntry,
    at: &str,
    ctx: &ScenarioContext,
    refusals: &mut Vec<ScenarioRefusal>,
) {
    let ladder = &ctx.ladder;
    let Some(rung) = find_rung(ladder, &entry.rung) else {
        refusals.push(ScenarioRefusal::new(
            format!("{at}.rung"),
            format!("'{}' is not a rung of the route branch", entry.rung),
        ));
        return;
    };
    let name = spoken(&entry.rung);
    let built = rung.verb.is_some();
    if !built && free_floor_position(ladder, rung) <= RUNG_UNSTARTED {
        refusals.push(ScenarioRefusal::new(
            format!("{at}.rung"),
            format!("a {name} is unworn ground: name a trail or above"),
        ));
    }
    match (&entry.keeper, built) {
        (None, true) => refusals.push(ScenarioRefusal::new(
            format!("{at}.keeper"),
            format!("a {name} is kept: name the band that keeps it"),
        )),
        (Some(_), false) => refusals.push(ScenarioRefusal::new(
            format!("{at}.keeper"),
            format!("a {name} has no keeper: nobody keeps the free floor"),
        )),
        (Some(keeper), true) if !section.bands.iter().any(|band| &band.id == keeper) => {
            refusals.push(ScenarioRefusal::new(
                format!("{at}.keeper"),
                format!(
                    "'{keeper}' is not a band of this faction: you cannot commit somebody else's people"
                ),
            ));
        }
        _ => {}
    }
}

/// The tile a band of `faction` stands on.
fn band_tile(world: &mut World, faction: FactionId, band: BandId) -> Option<UVec2> {
    let current = world
        .query::<(&PopulationCohort, &BandId)>()
        .iter(world)
        .find(|(cohort, id)| **id == band && cohort.faction == faction)
        .map(|(cohort, _)| cohort.current_tile)?;
    world.get::<Tile>(current).map(|tile| tile.position)
}

/// **Apply every listed road to the built world**, collecting every refusal.
pub(super) fn apply(world: &mut World, setup: &ScenarioSetup, refusals: &mut Vec<ScenarioRefusal>) {
    let ladder = world.resource::<LadderConfigHandle>().get();
    for faction_setup in &setup.factions {
        for (j, entry) in faction_setup.roads.iter().enumerate() {
            if let Err(reason) = apply_one(world, setup, faction_setup, entry, &ladder) {
                refusals.push(ScenarioRefusal::new(
                    format!("factions[{}].roads[{j}]", faction_setup.file_index),
                    reason,
                ));
            }
        }
    }
}

/// The tile's terrain name when it is water, `None` when it is land.
fn water_terrain(world: &World, tile: UVec2) -> Result<Option<sim_runtime::TerrainType>, String> {
    let outside = || format!("road at ({},{}): tile is outside the map", tile.x, tile.y);
    let entity = world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .ok_or_else(outside)?;
    let data = world.get::<Tile>(entity).ok_or_else(outside)?;
    // The predicate the bands section refuses a water tile with.
    Ok(data
        .terrain_tags
        .contains(sim_runtime::TerrainTags::WATER)
        .then_some(data.terrain))
}

fn apply_one(
    world: &mut World,
    setup: &ScenarioSetup,
    faction_setup: &FactionSetup,
    entry: &RoadEntry,
    ladder: &LadderConfig,
) -> Result<(), String> {
    let tile = UVec2::new(entry.at[0], entry.at[1]);
    let rung = find_rung(ladder, &entry.rung)
        .ok_or_else(|| format!("'{}' is not a rung of the route branch", entry.rung))?;
    if let Some(terrain) = water_terrain(world, tile)? {
        return Err(format!(
            "road at ({},{}): tile is {terrain:?}",
            tile.x, tile.y
        ));
    }
    let Some(keeper_id) = entry.keeper.as_ref() else {
        // The free floor: a held rung, no keeper.
        world
            .resource_mut::<RoadRegistry>()
            .road_or_trail(tile, ladder)
            .set_position(free_floor_position(ladder, rung), ladder);
        return Ok(());
    };
    // A built rung stands on a trail, exactly as `grade` requires.
    world
        .resource_mut::<RoadRegistry>()
        .road_or_trail(tile, ladder)
        .set_position(
            free_floor_position(ladder, crate::routes::free_floor_top_rung(ladder)),
            ladder,
        );
    let band = *setup
        .spawned
        .get(keeper_id)
        .ok_or_else(|| format!("'{keeper_id}' is not a band of this faction"))?;
    let faction = faction_setup.faction;
    let band_at = band_tile(world, faction, band)
        .ok_or_else(|| format!("band '{keeper_id}' is not standing anywhere"))?;
    let (_, remoteness) = quote_road_remoteness(world, band_at, tile, ladder);
    for step in route_rungs_in_climb_order(ladder)
        .into_iter()
        .filter(|candidate| candidate.verb.is_some() && candidate.order <= rung.order)
    {
        validate_road_verb(world, faction, band, tile, step)?;
        let (base, width) = road_rung_span(step.rung_id(), ladder, remoteness);
        let mut roads = world.resource_mut::<RoadRegistry>();
        let road = roads.road_or_trail(tile, ladder);
        road.take_keeper(RoadKeeper { faction, band }, remoteness, ladder);
        road.set_position(base + width, ladder);
    }
    Ok(())
}
