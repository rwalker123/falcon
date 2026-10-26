//! **THE STORAGE PIT IS ITS BAND'S SECOND LARDER** (#709,
//! `docs/plan_civilization_steps.md` -> "The storage pit is its band's second larder").
//!
//! A tile improvement on a `food_stores`-style branch whose held rung has a `keeping.fixed_stores`
//! entry is a **store**: it holds food as the same aged lots a larder holds ([`FoodMix`]) and it has
//! one **owner**, the band whose crew placed it. A store is not a supply-network node and nothing
//! pools into it: only its owner puts food in and takes food out, and only while the owner stands
//! within `supply_network_config.reach_tiles` of it.
//!
//! # The three systems, in the Population stage
//!
//! | system | when | what |
//! |---|---|---|
//! | [`draw_from_stores`] | before `simulate_population` | the owner's larder shortfall against this turn's meal is drawn from its stores in reach, nearest first, oldest food first |
//! | [`rot_store_contents`] | beside `rot_band_larders` | every store's lots age; a lot at its in-store shelf life rots whole; a store that is no longer one loses its contents |
//! | [`deposit_into_stores`] | after `advance_labor_allocation` | owners are settled (a gone owner clears; the next crewing band adopts), then the larder lots of a stored class that entered the larder THIS turn move into the owner's stores in reach, up to each store's room |
//!
//! "THIS turn" is the larder clock: a lot enters a larder with `stored == JUST_STORED` and the rot pass
//! (which runs before the labor pass lands the turn's takes) advances every older lot, so at deposit
//! time `stored == 0` selects exactly what landed after the rot - takes, landed packs - and never the
//! opening reserve or earlier pooled food (pooling lands before the meal, and the rot pass then ages
//! it).
//!
//! Store rot is the store's own: it goes on no band's `foodSpoiled`, it does not teach storage, and
//! the ledger carries the moves as `foodIntoStore` / `foodFromStore`.

use bevy::ecs::system::SystemParam;
use bevy::prelude::{Entity, Query, Res, ResMut, UVec2, With, Without};
use std::collections::{BTreeMap, BTreeSet};

use crate::components::{
    BandId, Expedition, FoodMix, LaborAllocation, LaborTarget, PopulationCohort, ResidentBand,
    Tile, JUST_STORED,
};
use crate::demographics_config::{DemographicsConfigHandle, KeepingConfig, StoreSpec};
use crate::intensification::{BranchId, LadderConfig, LadderConfigHandle};
use crate::orders::FactionId;
use crate::resources::{SimulationConfig, TileRegistry};
use crate::scalar::{scalar_from_f32, scalar_zero, Scalar};
use crate::spoilage::{HeldPreservation, PreservationScene};
use crate::supply_network_config::SupplyNetworkConfigHandle;
use crate::tile_improvement::TileImprovementRegistry;

/// **ONE STORE A BAND OWNS AND CAN REACH**, with what its held rung makes of it.
#[derive(Debug, Clone)]
pub struct StoreSite {
    pub tile: UVec2,
    pub branch: BranchId,
    /// Hex steps from the band to the store.
    pub distance: u32,
    pub spec: StoreSpec,
}

/// **THE STORES `band` OWNS WITHIN REACH OF `at`**, nearest first (ties in tile order, row-major,
/// then branch). A store belongs to the band whose id is its `owner` AND whose people are the
/// improvement's `faction`; an improvement holding no fixed-store rung is not a store.
pub fn owned_stores_in_reach(
    scene: &PreservationScene,
    band: BandId,
    faction: FactionId,
    at: UVec2,
) -> Vec<StoreSite> {
    let (width, wrap) = scene.grid;
    let mut sites: Vec<StoreSite> = scene
        .improvements
        .iter()
        .filter(|improvement| improvement.owner == Some(band) && improvement.faction == faction)
        .filter_map(|improvement| {
            let spec = scene
                .keeping
                .store_spec(scene.ladder, improvement.held_rung())?;
            let distance =
                crate::grid_utils::hex_distance_wrapped(at, improvement.tile, width, wrap);
            (distance <= scene.reach_tiles).then_some(StoreSite {
                tile: improvement.tile,
                branch: improvement.branch,
                distance,
                spec,
            })
        })
        .collect();
    // The registry iterates row-major, so a stable sort by distance leaves ties in tile order.
    sites.sort_by_key(|site| site.distance);
    sites
}

/// **THE FOOD A BAND CAN REACH IN ITS OWN STORES** - every lot, and the in-store shelf life of each
/// class as a [`HeldPreservation`] (the longest any reachable store gives it). What the runway and
/// the fertility reserve count beside the larder.
#[derive(Debug, Clone, Default)]
pub struct ReachableStores {
    pub food: FoodMix,
    pub held: HeldPreservation,
}

impl ReachableStores {
    /// Nothing reachable.
    pub fn none() -> Self {
        Self::default()
    }

    /// The reachable food, every class and age.
    pub fn total(&self) -> Scalar {
        self.food.total()
    }
}

/// **[`ReachableStores`] for one band** - `band`/`at` absent (a party, a hand-built row) reach
/// nothing. The one helper the runway and the fertility reserve both call.
pub fn reachable_stores(
    scene: &PreservationScene,
    band: Option<BandId>,
    faction: FactionId,
    at: Option<UVec2>,
) -> ReachableStores {
    let (Some(band), Some(at)) = (band, at) else {
        return ReachableStores::none();
    };
    let mut reachable = ReachableStores::none();
    for site in owned_stores_in_reach(scene, band, faction, at) {
        let Some(improvement) = scene.improvements.get(site.tile, site.branch) else {
            continue;
        };
        reachable.food.merge(&improvement.contents);
        for (class, life) in &site.spec.lives {
            reachable.held.raise(class, *life);
        }
    }
    reachable
}

/// **THE RESOURCES A STORE LOOKUP READS**, bundled so a system with a full parameter list can carry
/// them as one. Every one is optional: a hand-built world may lack any, and then no band reaches a
/// store.
#[derive(SystemParam)]
pub struct StoreContext<'w> {
    improvements: Option<Res<'w, TileImprovementRegistry>>,
    supply: Option<Res<'w, SupplyNetworkConfigHandle>>,
    tile_registry: Option<Res<'w, TileRegistry>>,
    sim_config: Option<Res<'w, SimulationConfig>>,
    ladder: Option<Res<'w, LadderConfigHandle>>,
}

impl StoreContext<'_> {
    /// The ladder, if the world has one.
    pub fn ladder(&self) -> Option<std::sync::Arc<LadderConfig>> {
        self.ladder.as_ref().map(|handle| handle.get())
    }

    /// Run `f` over a [`PreservationScene`] built from the bundle, or return `None` when a resource is
    /// missing.
    pub fn with_scene<R>(
        &self,
        keeping: &KeepingConfig,
        f: impl FnOnce(&PreservationScene) -> R,
    ) -> Option<R> {
        let ladder = self.ladder()?;
        let supply = self.supply.as_ref()?.get();
        let scene = PreservationScene {
            keeping,
            ladder: &ladder,
            improvements: self.improvements.as_deref()?,
            reach_tiles: supply.reach_tiles,
            grid: (
                self.tile_registry.as_deref()?.width,
                self.sim_config
                    .as_deref()
                    .is_some_and(|config| config.map_topology.wrap_horizontal),
            ),
        };
        Some(f(&scene))
    }
}

/// The scene over a registry the caller holds (possibly mutably, dropped before it mutates).
fn scene_over<'a>(
    keeping: &'a KeepingConfig,
    ladder: &'a LadderConfig,
    improvements: &'a TileImprovementRegistry,
    reach_tiles: u32,
    grid: (u32, bool),
) -> PreservationScene<'a> {
    PreservationScene {
        keeping,
        ladder,
        improvements,
        reach_tiles,
        grid,
    }
}

/// The resources the three systems resolve the same way.
struct StoreWorld {
    reach_tiles: u32,
    grid: (u32, bool),
}

fn store_world(
    supply: Option<&SupplyNetworkConfigHandle>,
    tile_registry: Option<&TileRegistry>,
    sim_config: Option<&SimulationConfig>,
) -> Option<StoreWorld> {
    Some(StoreWorld {
        reach_tiles: supply?.get().reach_tiles,
        grid: (
            tile_registry?.width,
            sim_config.is_some_and(|config| config.map_topology.wrap_horizontal),
        ),
    })
}

/// **DRAW - before the meal, a band whose larder cannot cover its need draws the difference from
/// the stores it owns within reach** (nearest store first, oldest food first within one), keeping
/// each lot's age. Also resets the turn's `into`/`from` ledger terms.
#[allow(clippy::too_many_arguments)] // a Bevy system: one parameter per resource it reads
pub fn draw_from_stores(
    demographics: Res<DemographicsConfigHandle>,
    ladder: Option<Res<LadderConfigHandle>>,
    supply: Option<Res<SupplyNetworkConfigHandle>>,
    tile_registry: Option<Res<TileRegistry>>,
    sim_config: Option<Res<SimulationConfig>>,
    improvements: Option<ResMut<TileImprovementRegistry>>,
    tiles: Query<&Tile>,
    parties: Query<(&PopulationCohort, &Expedition), Without<ResidentBand>>,
    mut cohorts: Query<(Entity, &mut PopulationCohort, Option<&BandId>), With<ResidentBand>>,
) {
    for (_, mut cohort, _) in cohorts.iter_mut() {
        cohort.last_food_into_store = 0.0;
        cohort.last_food_from_store = 0.0;
    }
    let (Some(ladder), Some(mut improvements)) = (ladder, improvements) else {
        return;
    };
    let Some(world) = store_world(
        supply.as_deref(),
        tile_registry.as_deref(),
        sim_config.as_deref(),
    ) else {
        return;
    };
    let config = demographics.get();
    let ladder = ladder.get();
    let party_working = crate::systems::stationed_party_workers(&parties);
    for (entity, mut cohort, band_id) in cohorts.iter_mut() {
        let (Some(band_id), Ok(tile)) = (band_id, tiles.get(cohort.current_tile)) else {
            continue;
        };
        // The same demand the meal is measured against (`meal_and_births`): the opening brackets
        // and the parties the band feeds.
        let demand = crate::systems::food_demand(
            cohort.children,
            cohort.working + party_working.get(&entity).copied().unwrap_or_default(),
            cohort.elders,
            &config.consumption,
        );
        let mut shortfall = demand - cohort.stores.get(crate::components::FOOD);
        if shortfall <= scalar_zero() {
            continue;
        }
        let sites = owned_stores_in_reach(
            &scene_over(
                &config.keeping,
                &ladder,
                &improvements,
                world.reach_tiles,
                world.grid,
            ),
            *band_id,
            cohort.faction,
            tile.position,
        );
        let mut drawn = scalar_zero();
        for site in sites {
            if shortfall <= scalar_zero() {
                break;
            }
            let Some(improvement) = improvements.get_mut(site.tile, site.branch) else {
                continue;
            };
            let taken = improvement.contents.take_oldest(shortfall);
            let amount = taken.total();
            cohort.stores.add_food_mix(&taken);
            shortfall -= amount;
            drawn += amount;
        }
        cohort.last_food_from_store = drawn.to_f32();
    }
}

/// **ROT IN STORES - once a turn, beside the larder rot.** Every store's lots age a turn and a lot
/// reaching its class's in-store shelf life rots whole. A class the store does not hold cannot be in
/// it, and an improvement that no longer holds a store rung (unkept, fallen to its root) holds
/// nothing: both are lost, and everything that went counts in `spoiled_this_turn`.
pub fn rot_store_contents(
    demographics: Res<DemographicsConfigHandle>,
    ladder: Option<Res<LadderConfigHandle>>,
    improvements: Option<ResMut<TileImprovementRegistry>>,
) {
    let (Some(ladder), Some(mut improvements)) = (ladder, improvements) else {
        return;
    };
    let config = demographics.get();
    let ladder = ladder.get();
    for improvement in improvements.iter_mut() {
        improvement.spoiled_this_turn = crate::tile_improvement::NO_FOOD_SPOILED;
        if improvement.contents.is_empty() {
            continue;
        }
        let Some(spec) = config.keeping.store_spec(&ladder, improvement.held_rung()) else {
            improvement.spoiled_this_turn += improvement.contents.total().to_f32();
            improvement.contents = FoodMix::default();
            continue;
        };
        let lost = improvement
            .contents
            .retaining(|class| !spec.holds(class))
            .total();
        improvement.contents = improvement.contents.retaining(|class| spec.holds(class));
        let rotted = improvement
            .contents
            .age_and_expire(|class| spec.life_of(class));
        improvement.spoiled_this_turn += (lost + rotted).to_f32();
    }
}

/// The bands the deposit pass walks: the larder it moves out of, the id that names its stores, and the
/// labor rows that say who crews a store.
type DepositBands<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut PopulationCohort,
        Option<&'static BandId>,
        Option<&'static LaborAllocation>,
    ),
    With<ResidentBand>,
>;

/// **SETTLE OWNERS, THEN DEPOSIT.** Owners first: a store's owner is invalid when no resident band
/// with that id exists or that band is not of the improvement's people, and is cleared; an unowned
/// store is adopted by the lowest `BandId` of its people with a `tile` labor row crewing it. Then
/// each band moves the larder lots that entered it this turn, of a class the store holds, into the
/// stores it owns in reach, nearest first, up to each store's room; the rest stays in the larder.
#[allow(clippy::too_many_arguments)] // a Bevy system: one parameter per resource it reads
pub fn deposit_into_stores(
    demographics: Res<DemographicsConfigHandle>,
    ladder: Option<Res<LadderConfigHandle>>,
    supply: Option<Res<SupplyNetworkConfigHandle>>,
    tile_registry: Option<Res<TileRegistry>>,
    sim_config: Option<Res<SimulationConfig>>,
    improvements: Option<ResMut<TileImprovementRegistry>>,
    tiles: Query<&Tile>,
    mut cohorts: DepositBands,
) {
    let (Some(ladder), Some(mut improvements)) = (ladder, improvements) else {
        return;
    };
    let Some(world) = store_world(
        supply.as_deref(),
        tile_registry.as_deref(),
        sim_config.as_deref(),
    ) else {
        return;
    };
    let config = demographics.get();
    let ladder = ladder.get();

    // --- owners -------------------------------------------------------------------------------
    let bands: BTreeMap<BandId, FactionId> = cohorts
        .iter()
        .filter_map(|(cohort, id, _)| id.map(|id| (*id, cohort.faction)))
        .collect();
    let crews: BTreeSet<(BandId, u32, u32, BranchId)> = cohorts
        .iter()
        .filter_map(|(_, id, allocation)| Some((*id?, allocation?)))
        .flat_map(|(id, allocation)| {
            allocation
                .assignments
                .iter()
                .filter(|row| row.workers > 0)
                .filter_map(move |row| match &row.target {
                    LaborTarget::Tile { tile, branch } => Some((id, tile.y, tile.x, *branch)),
                    _ => None,
                })
        })
        .collect();
    for improvement in improvements.iter_mut() {
        if config
            .keeping
            .store_spec(&ladder, improvement.held_rung())
            .is_none()
        {
            continue;
        }
        if let Some(owner) = improvement.owner {
            if bands.get(&owner) != Some(&improvement.faction) {
                improvement.owner = None;
            }
        }
        if improvement.owner.is_none() {
            improvement.owner = bands
                .iter()
                .filter(|(_, faction)| **faction == improvement.faction)
                .map(|(id, _)| *id)
                .find(|id| {
                    crews.contains(&(
                        *id,
                        improvement.tile.y,
                        improvement.tile.x,
                        improvement.branch,
                    ))
                });
        }
    }

    // --- deposits -----------------------------------------------------------------------------
    for (mut cohort, band_id, _) in cohorts.iter_mut() {
        let (Some(band_id), Ok(tile)) = (band_id, tiles.get(cohort.current_tile)) else {
            continue;
        };
        let sites = owned_stores_in_reach(
            &scene_over(
                &config.keeping,
                &ladder,
                &improvements,
                world.reach_tiles,
                world.grid,
            ),
            *band_id,
            cohort.faction,
            tile.position,
        );
        let mut deposited = scalar_zero();
        for site in sites {
            let Some(improvement) = improvements.get_mut(site.tile, site.branch) else {
                continue;
            };
            let room = scalar_from_f32(site.spec.capacity) - improvement.contents.total();
            if room <= scalar_zero() {
                continue;
            }
            let moved = cohort.stores.take_food_lots_where(
                |class, lot| lot.stored == JUST_STORED && site.spec.holds(class),
                room,
            );
            deposited += moved.total();
            improvement.contents.merge(&moved);
        }
        cohort.last_food_into_store = deposited.to_f32();
    }
}
