//! **A CHIEF WITH A FOOD ORDER BUILDS THE STORAGE THAT SAVES THE FOOD HIS BAND SPOILS.**
//!
//! A band's surplus spoilage is the band's (`band_spoilage`), not a hunt row's. When the band has a
//! leader whose `food` order is his, [`advance_chief_storage`] reads the band's `storageSaves` and,
//! for each rung whose lesson is known, that nothing is declared on and that would save at least
//! `labor_config.band_orders.storage_build_min_saved` food a turn, declares the build: the rack
//! through the band-improvement row, the pit through a tile row on the band's own tile — the rows the
//! player's `assign_labor ... band|tile ...` creates, listed under the `food` order
//! (`keep_on_hand::{band,tile}_improvement_key`) so the wire shows them as the chief's and
//! `release` / `claim` of a single row work on them. He never retires such a row
//! (`food_orders::row_is_improvement`).

use bevy::math::UVec2;
use bevy::prelude::{Query, Res, ResMut, With};

use crate::band_spoilage::{band_spoilage_of, BandSpoilInputs};
use crate::components::{
    BandId, BandName, BuildJob, BuildSource, LaborAllocation, LaborTarget, PopulationCohort,
    ResidentBand, Tile,
};
use crate::demographics_config::DemographicsConfigHandle;
use crate::fauna::HerdRegistry;
use crate::fauna_config::FaunaConfigHandle;
use crate::flora_config::FloraConfigHandle;
use crate::forage::ForageRegistry;
use crate::intensification::{LadderConfigHandle, SourceKind};
use crate::labor_config::LaborConfigHandle;
use crate::leadership::BandLeadership;
use crate::resources::{
    CommandEventLog, DiscoveryProgressLedger, SimulationConfig, SimulationTick, TileRegistry,
};
use crate::spoilage::PreservationScene;
use crate::supply_network_config::SupplyNetworkConfigHandle;
use crate::tile_improvement::TileImprovementRegistry;
use crate::{CommandEventEntry, CommandEventKind};

/// What the storage pass reads of a resident band.
type StorageBandItems = (
    bevy::prelude::Entity,
    &'static PopulationCohort,
    &'static BandId,
    Option<&'static BandName>,
    &'static BandLeadership,
    &'static mut LaborAllocation,
);

/// What one order does to the world, decided against the read-only world and applied after.
struct StorageOrder {
    band: BandId,
    entity: bevy::prelude::Entity,
    faction: crate::orders::FactionId,
    target: LaborTarget,
    first_rung: crate::intensification::RungId,
    rung_label: String,
    spoiling: f32,
    keeper_line: String,
    plural: bool,
}

/// "a" or "an" for a lower-case noun phrase.
fn with_article(noun: &str) -> String {
    let article = if noun.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {noun}")
}

/// The Population-chain system: see the module documentation.
#[allow(clippy::too_many_arguments)] // a Bevy system: one parameter per resource it reads
pub fn advance_chief_storage(
    tick: Res<SimulationTick>,
    demographics: Res<DemographicsConfigHandle>,
    fauna: Res<FaunaConfigHandle>,
    flora: Res<FloraConfigHandle>,
    labor: Res<LaborConfigHandle>,
    ladder: Res<LadderConfigHandle>,
    supply: Option<Res<SupplyNetworkConfigHandle>>,
    sim_config: Res<SimulationConfig>,
    tile_registry: Res<TileRegistry>,
    herds: Res<HerdRegistry>,
    forage: Res<ForageRegistry>,
    discovery: Res<DiscoveryProgressLedger>,
    mut improvements: ResMut<TileImprovementRegistry>,
    mut event_log: ResMut<CommandEventLog>,
    tiles: Query<&Tile>,
    mut bands: Query<StorageBandItems, With<ResidentBand>>,
) {
    let demographics = demographics.get();
    let fauna = fauna.get();
    let flora = flora.get();
    let labor = labor.get();
    let ladder = ladder.get();
    let reach_tiles = supply
        .as_deref()
        .map_or(0, |handle| handle.get().reach_tiles);
    let threshold = labor.band_orders.storage_build_min_saved;
    let scene = PreservationScene {
        keeping: &demographics.keeping,
        ladder: &ladder,
        improvements: &improvements,
        reach_tiles,
        grid: (tile_registry.width, sim_config.map_topology.wrap_horizontal),
    };
    let composition = |tile: UVec2| {
        let entity = tile_registry.index(tile.x, tile.y)?;
        let ground = tiles.get(entity).ok()?;
        Some(
            crate::forage::tile_flora_composition(
                &flora,
                &labor.forage,
                ground,
                sim_config.map_seed,
            )
            .into_owned(),
        )
    };
    let mut orders: Vec<StorageOrder> = Vec::new();
    let mut ids: Vec<_> = bands
        .iter()
        .filter(|(_, _, _, _, leadership, allocation)| {
            leadership.leader.is_some() && allocation.chief_owns(crate::keep_on_hand::FOOD_KIND)
        })
        .map(|(entity, _, id, _, _, _)| (*id, entity))
        .collect();
    ids.sort();
    for (id, entity) in ids {
        let Ok((_, cohort, _, name, leadership, allocation)) = bands.get(entity) else {
            continue;
        };
        let at = tiles
            .get(cohort.current_tile)
            .ok()
            .map(|tile| tile.position);
        let held = scene.held_by(Some(id));
        let stores = crate::fixed_store::reachable_stores(&scene, Some(id), cohort.faction, at);
        let spoilage = band_spoilage_of(&BandSpoilInputs {
            cohort,
            allocation,
            band: Some(id),
            at,
            demographics: &demographics,
            fauna: &fauna,
            flora: &flora,
            labor: &labor,
            herds: &herds,
            forage: &forage,
            composition: &|tile| composition(tile),
            ladder: &ladder,
            improvements: &improvements,
            held: &held,
            stores: &stores,
            knows: &|discovery_id| {
                crate::intensification::knows(
                    &discovery,
                    cohort.faction,
                    discovery_id,
                    ladder.knowledge.completion_threshold,
                )
            },
        });
        let spoiling: f32 = spoilage.spoil_ahead.iter().map(|row| row.food).sum();
        let band_name = name.map_or_else(|| format!("Band {}", id.0), |name| name.0.clone());
        let Some(keeper) = crate::keep_on_hand::Keeper::of(Some(leadership), &band_name) else {
            continue;
        };
        for save in spoilage.actionable(threshold) {
            let Some(entry) = demographics
                .keeping
                .preservation
                .iter()
                .find(|entry| ladder.rung(entry.rung).wire_key() == save.rung)
            else {
                continue;
            };
            let branch = entry.rung.branch;
            let target = match ladder.source_kind(branch) {
                Some(SourceKind::Band) => LaborTarget::Band { band: id, branch },
                Some(SourceKind::Tile) => match at {
                    Some(tile) => LaborTarget::Tile { tile, branch },
                    None => continue,
                },
                _ => continue,
            };
            let Some(first) = ladder
                .root_of(branch)
                .and_then(|root| ladder.above_rung(root))
            else {
                continue;
            };
            orders.push(StorageOrder {
                band: id,
                entity,
                faction: cohort.faction,
                target,
                first_rung: first.rung_id(),
                rung_label: ladder.rung(entry.rung).label.to_lowercase(),
                spoiling,
                keeper_line: keeper.subject.clone(),
                plural: keeper.plural,
            });
        }
    }
    for order in orders {
        let Ok((_, _, _, _, _, mut allocation)) = bands.get_mut(order.entity) else {
            continue;
        };
        match &order.target {
            LaborTarget::Band { band, branch } => {
                improvements.bands_mut().place(*band, *branch, &ladder);
            }
            LaborTarget::Tile { tile, branch } => {
                improvements.place_owned(*tile, *branch, order.faction, Some(order.band), &ladder);
            }
            _ => continue,
        }
        let Some(source) = BuildSource::of(&order.target) else {
            continue;
        };
        allocation.set_staffed_row(order.target.clone(), None, None);
        if let Some(key) = crate::keep_on_hand::source_key(&order.target) {
            allocation
                .order_rows
                .insert(key, crate::keep_on_hand::FOOD_KIND.to_string());
        }
        allocation.declare_build(source, BuildJob::Climb(order.first_rung));
        let verb = if order.plural { "order" } else { "orders" };
        event_log.push(
            CommandEventEntry::new(
                tick.0,
                CommandEventKind::Extraction,
                order.faction,
                format!(
                    "{} {verb} {} built — {:.1} food a turn is spoiling.",
                    order.keeper_line,
                    with_article(&order.rung_label),
                    order.spoiling
                ),
                Some("status=applied action=band_order order=storage_build".to_string()),
            )
            .with_band(order.band),
        );
    }
}
