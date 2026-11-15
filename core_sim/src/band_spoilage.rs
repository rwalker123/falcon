//! **THE BAND'S SPOILAGE FORECAST, AND WHAT STORAGE WOULD SAVE** — the food a band's larder and
//! stores spoil a turn at its steady income, per keeping class, and per storage rung the band does
//! not hold what holding it would save.
//!
//! Surplus spoilage belongs to the BAND and its storage, never to a hunt row: a hunt row carries only
//! the rot of its own walk (`work_party::TransitRot`). The band reads it on the wire
//! (`PopulationCohortState.spoilAhead` / `storageSaves`); a chief with a food order builds the
//! rung (`food_orders`), and a rival's food specialist reads the same rows.
//!
//! **Data-driven**: no rung or class id is named here — every row comes off the keeping table's
//! `preservation` entries and the ladder.

use std::collections::BTreeMap;

use bevy::math::UVec2;

use crate::components::{
    BandId, BuildSource, LaborAllocation, LaborTarget, PopulationCohort, TakeSelection,
};
use crate::demographics_config::KeepingConfig;
use crate::fauna_config::FaunaConfig;
use crate::intensification::{LadderConfig, SourceKind};
use crate::spoilage::{self, HeldPreservation, IncomeByClass};
use crate::tile_improvement::TileImprovementRegistry;

/// The class shares (summing to one) of the basket a crew takes from the patch at a tile.
pub type PatchShares<'a> = dyn Fn(UVec2, &TakeSelection) -> Vec<(String, f32)> + 'a;

/// What a band's rows land per class: a hunt row by its herd's class, a forage row by the shares of
/// its basket.
pub struct IncomeSources<'a> {
    pub keeping: &'a KeepingConfig,
    pub fauna: &'a FaunaConfig,
    /// The species of the herd with this id, if it is still on the map.
    pub herd_species: &'a dyn Fn(&str) -> Option<String>,
    /// The class shares (summing to one) of the basket a crew takes from the patch at a tile.
    pub patch_shares: &'a PatchShares<'a>,
}

/// **THE BAND'S STEADY INCOME BY KEEPING CLASS** — each food row's `realized` (a need row at its
/// `sustainable`, as `band_food_flow` reads it) split by the class it lands in.
pub fn income_by_class(allocation: &LaborAllocation, sources: &IncomeSources<'_>) -> IncomeByClass {
    let mut income = IncomeByClass::new();
    for (row, yields) in allocation.assignments.iter().zip(&allocation.last_yields) {
        let amount = if row.is_need_row() {
            yields.sustainable
        } else {
            yields.realized
        };
        if amount <= 0.0 {
            continue;
        }
        match &row.target {
            LaborTarget::Hunt { fauna_id, .. } => {
                let class = (sources.herd_species)(fauna_id)
                    .and_then(|species| sources.fauna.keeping_for(&species).map(str::to_string))
                    .unwrap_or_else(|| sources.keeping.kill_fallback_class.clone());
                *income.entry(class).or_insert(0.0) += amount;
            }
            LaborTarget::Forage {
                tile, take_species, ..
            } => {
                let shares = (sources.patch_shares)(*tile, take_species);
                if shares.is_empty() {
                    *income
                        .entry(sources.keeping.plant_fallback_class.clone())
                        .or_insert(0.0) += amount;
                } else {
                    for (class, share) in shares {
                        *income.entry(class).or_insert(0.0) += amount * share;
                    }
                }
            }
            _ => {}
        }
    }
    income
}

/// Everything the forecast reads about one band.
pub struct SpoilScene<'a> {
    pub cohort: &'a PopulationCohort,
    pub allocation: &'a LaborAllocation,
    pub band: Option<BandId>,
    /// Where the band stands (the pit is built on its tile).
    pub at: Option<UVec2>,
    pub keeping: &'a KeepingConfig,
    pub ladder: &'a LadderConfig,
    pub improvements: &'a TileImprovementRegistry,
    pub held: &'a HeldPreservation,
    pub stores: &'a crate::fixed_store::ReachableStores,
    pub income: &'a IncomeByClass,
    /// The meal the band eats a turn.
    pub need: f32,
    /// `band_orders.storage_build_min_saved`.
    pub build_min_saved: f32,
    /// `labor_config.yield_average_horizon_turns`.
    pub horizon: u32,
    /// Does the band's faction know the discovery with this id?
    pub knows: &'a dyn Fn(u32) -> bool,
}

/// The two published readings.
#[derive(Debug, Clone, Default)]
pub struct BandSpoilage {
    /// Per class, in the keeping table's order, non-zero classes only.
    pub spoil_ahead: Vec<sim_schema::KeepingSpoilState>,
    pub storage_saves: Vec<sim_schema::StorageSaveState>,
    /// `band_orders.storage_build_min_saved`, echoed so a rival reads the chief's own threshold.
    pub storage_build_min_saved: f32,
    /// Every preservation rung the band holds now.
    pub held_stores: Vec<sim_schema::HeldStoreState>,
}

impl BandSpoilage {
    /// The saves a chief or an AI may act on: lesson known, nothing declared, saving at least
    /// `min_saved` a turn.
    pub fn actionable(
        &self,
        min_saved: f32,
    ) -> impl Iterator<Item = &sim_schema::StorageSaveState> {
        self.storage_saves
            .iter()
            .filter(move |save| save.lesson_known && !save.declared && save.food_saved >= min_saved)
    }
}

fn total(per_class: &BTreeMap<String, f32>) -> f32 {
    per_class.values().sum()
}

/// **THE FORECAST** — see the module documentation.
///
/// `foodSaved` for a rung is the total spoil rate now less the same projection with that rung's
/// class held at its `shelf_life_turns`. **An approximation for a tile store** (the pit): its food
/// sits in the pit at the pit's life, which is not the larder's; treating its class as held in the
/// larder prices the same lengthening without modelling the deposit.
pub fn band_spoilage(scene: &SpoilScene<'_>) -> BandSpoilage {
    let walk = |held: &HeldPreservation| {
        spoilage::spoilage_ahead(
            scene.cohort.stores.food(),
            &scene.stores.food,
            scene.income,
            scene.need,
            scene.horizon,
            scene.keeping,
            held,
            &scene.stores.held,
        )
    };
    let now = walk(scene.held);
    let held_stores = scene
        .keeping
        .preservation
        .iter()
        .filter(|entry| {
            let held_at_or_above =
                |held: crate::intensification::RungId| scene.ladder.at_or_above(held, entry.rung);
            match scene.ladder.source_kind(entry.rung.branch) {
                Some(SourceKind::Band) => scene.band.is_some_and(|band| {
                    scene
                        .improvements
                        .bands()
                        .get(band, entry.rung.branch)
                        .is_some_and(|held| held_at_or_above(held.held_rung()))
                }),
                Some(SourceKind::Tile) => scene.at.is_some_and(|tile| {
                    scene
                        .improvements
                        .get(tile, entry.rung.branch)
                        .is_some_and(|held| held_at_or_above(held.held_rung()))
                }),
                _ => false,
            }
        })
        .map(|entry| sim_schema::HeldStoreState {
            rung: scene.ladder.rung(entry.rung).wire_key(),
            keeping: entry.class.clone(),
            shelf_life_turns: entry.shelf_life_turns,
        })
        .collect();
    let spoil_ahead = scene
        .keeping
        .classes
        .iter()
        .filter_map(|class| {
            now.get(&class.id)
                .map(|food| sim_schema::KeepingSpoilState {
                    keeping: class.id.clone(),
                    food: *food,
                })
        })
        .collect();
    let now_total = total(&now);
    let mut storage_saves = Vec::new();
    for entry in &scene.keeping.preservation {
        let holds = |lives: &HeldPreservation| {
            lives
                .life_of(&entry.class)
                .is_some_and(|life| life >= entry.shelf_life_turns)
        };
        if holds(scene.held) || holds(&scene.stores.held) {
            continue;
        }
        let rung = scene.ladder.rung(entry.rung);
        let mut with = scene.held.clone();
        with.raise(&entry.class, entry.shelf_life_turns);
        let food_saved = (now_total - total(&walk(&with))).max(0.0);
        let declared = match scene.ladder.source_kind(entry.rung.branch) {
            Some(SourceKind::Band) => scene.band.is_some_and(|band| {
                scene.allocation.has_build(&BuildSource::Band {
                    band,
                    branch: entry.rung.branch,
                })
            }),
            Some(SourceKind::Tile) => scene.at.is_some_and(|tile| {
                scene.allocation.has_build(&BuildSource::Tile {
                    tile,
                    branch: entry.rung.branch,
                })
            }),
            _ => false,
        };
        let (material_id, material_needed) = rung
            .build_materials()
            .next()
            .map(|(id, amount)| (id.to_string(), amount))
            .unwrap_or_default();
        let material_on_hand = if material_id.is_empty() {
            0.0
        } else {
            scene.cohort.stores.material_total(&material_id).to_f32()
        };
        storage_saves.push(sim_schema::StorageSaveState {
            rung: rung.wire_key(),
            keeping: entry.class.clone(),
            food_saved,
            lesson_known: rung
                .unlock_discovery_id()
                .is_none_or(|id| (scene.knows)(id)),
            declared,
            material_id,
            material_needed,
            material_on_hand,
            tile_kept: scene.ladder.source_kind(entry.rung.branch) == Some(SourceKind::Tile),
        });
    }
    BandSpoilage {
        spoil_ahead,
        storage_saves,
        storage_build_min_saved: scene.build_min_saved,
        held_stores,
    }
}

/// **EVERYTHING ONE BAND'S SPOILAGE READS**, off the world — shared by the capture (which publishes
/// it) and the chief's storage pass (which acts on it), so the two cannot read different numbers.
pub struct BandSpoilInputs<'a> {
    pub cohort: &'a PopulationCohort,
    pub allocation: &'a LaborAllocation,
    pub band: Option<BandId>,
    pub at: Option<UVec2>,
    pub demographics: &'a crate::demographics_config::DemographicsConfig,
    pub fauna: &'a FaunaConfig,
    pub flora: &'a crate::flora_config::FloraConfig,
    pub labor: &'a crate::labor_config::LaborConfig,
    pub herds: &'a crate::fauna::HerdRegistry,
    pub forage: &'a crate::forage::ForageRegistry,
    /// The flora basket realised on the tile of a patch.
    pub composition: &'a dyn Fn(UVec2) -> Option<Vec<crate::flora_config::FloraShare>>,
    pub ladder: &'a LadderConfig,
    pub improvements: &'a TileImprovementRegistry,
    pub held: &'a HeldPreservation,
    pub stores: &'a crate::fixed_store::ReachableStores,
    pub knows: &'a dyn Fn(u32) -> bool,
}

/// **ONE BAND'S SPOILAGE AND ITS STORAGE SAVES** — [`income_by_class`] then [`band_spoilage`].
pub fn band_spoilage_of(inputs: &BandSpoilInputs<'_>) -> BandSpoilage {
    let keeping = &inputs.demographics.keeping;
    let income = income_by_class(
        inputs.allocation,
        &IncomeSources {
            keeping,
            fauna: inputs.fauna,
            herd_species: &|id| inputs.herds.find(id).map(|herd| herd.species.clone()),
            patch_shares: &|tile, take| {
                let (Some(patch), Some(composition)) =
                    (inputs.forage.patch(tile), (inputs.composition)(tile))
                else {
                    return Vec::new();
                };
                let mix = crate::forage::patch_food_mix(
                    patch,
                    &composition,
                    inputs.flora,
                    &inputs.labor.forage,
                    take,
                    crate::scalar::scalar_one(),
                    keeping,
                );
                let sum = mix.total().to_f32();
                if sum <= 0.0 {
                    return Vec::new();
                }
                mix.lots()
                    .into_iter()
                    .map(|(class, lot)| (class, lot.amount.to_f32() / sum))
                    .collect()
            },
        },
    );
    let cohort = inputs.cohort;
    let need = crate::systems::food_demand(
        cohort.children,
        cohort.working + cohort.fed_party_workers,
        cohort.elders,
        &inputs.demographics.consumption,
    )
    .to_f32();
    band_spoilage(&SpoilScene {
        cohort,
        allocation: inputs.allocation,
        band: inputs.band,
        at: inputs.at,
        keeping,
        ladder: inputs.ladder,
        improvements: inputs.improvements,
        held: inputs.held,
        stores: inputs.stores,
        income: &income,
        need,
        build_min_saved: inputs.labor.band_orders.storage_build_min_saved,
        horizon: inputs.labor.yield_average_horizon_turns,
        knows: inputs.knows,
    })
}
