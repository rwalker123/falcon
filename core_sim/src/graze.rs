//! The **graze (pasture) layer** — the land's *animal-edible* vegetal stock.
//!
//! Authoritative design: `docs/plan_grazing_foundation.md`. Mirrors `forage.rs` (which mirrors the
//! herd biomass model) exactly: every eligible land tile carries a live per-tile
//! `{ biomass, carrying_capacity, ecology_phase }` (`GrazePatch`) held in the authoritative
//! [`GrazeRegistry`] resource, keyed by tile coord, round-tripped through the rollback snapshot via
//! the shared `sim_schema::GrazeState`/`EcologyState` records (`GrazeRegistry::update_from_states`).
//!
//! **Humans and animals do not eat the same things.** `ForagePatch.biomass` (`forage.rs`) is the
//! *human-edible* stock — seeds, nuts, tubers, fruit — and it exists only on `FoodModuleTag` tiles.
//! Graze is **grass, browse and forbs**: cellulose humans cannot digest at all, on **any vegetated
//! land**, with its own per-biome distribution. That is not flavor — it is the entire economic basis
//! of herding (a pastoralist converts a resource that is *worthless to humans* into meat and milk),
//! and it is why a closed-canopy forest is rich in forage and poor in graze while a prairie steppe is
//! the reverse. *Your best farm is usually not your best pasture.*
//!
//! **Phase 2a ships this layer INERT.** It seeds, regrows, persists and exports — and **nothing reads
//! it for gameplay**. No herd behaviour changes; zero balance impact. Herd carrying capacity,
//! competition, overgrazing, migration and spawn placement all become functions of it in Phase 2b/2c,
//! and this phase exists so the distribution can be *looked at on a real map* before the fauna model
//! is bet on it.
//!
//! Differences from `forage.rs`, each deliberate:
//! - **No Allee / collapse branch.** Grass has no depensation: regrowth is pure `logistic_regrowth`
//!   toward capacity, plus the reseed floor (via the shared `fauna::reseeding_logistic_regrowth`).
//! - **No cultivation.** Graze is wild ground; it is never owned, tended or improved. The
//!   `EcologyState` record's `progress`/`owner` fields therefore ride the snapshot as their defaults.
//! - **Density.** Forage patches are sparse (food-module tiles only); graze sits on *nearly every land
//!   tile*, so the wire readout is per-`TileState` (see `snapshot.rs`), not a per-patch list.

use std::collections::HashMap;

use bevy::prelude::*;
use sim_runtime::{TerrainTags, TerrainType};
use sim_schema::GrazeState;

use crate::{
    components::Tile,
    fauna::{classify_ecology_phase, reseeding_logistic_regrowth, EcologyPhase},
    fauna_config::{EcologyConfig, FaunaConfigHandle, GrazeConfig, NO_GRAZE_CAPACITY},
};

/// A live grazeable patch on one land tile — the animal-edible mirror of a [`crate::forage::ForagePatch`],
/// minus cultivation (graze is wild ground, never owned or tended).
#[derive(Debug, Clone)]
pub struct GrazePatch {
    /// Tile the patch sits on (its registry key).
    pub tile: UVec2,
    /// Live grazeable stock. Regrown by `advance_graze_regrowth`; drawn down by herds from Phase 2b.
    pub biomass: f32,
    /// The tile's biome-derived graze capacity (`graze.capacity_by_biome`) — the land's property, not
    /// any animal's. Phase 2b makes a herd's `K` a function of the capacity across its range.
    pub carrying_capacity: f32,
    /// Coarse health band (Thriving/Stressed/Collapsing), recomputed each turn from biomass vs
    /// `carrying_capacity`. This is the **overgrazing** readout.
    pub ecology_phase: EcologyPhase,
}

impl GrazePatch {
    /// A fresh patch at full biomass (= its biome's capacity). Phase is `Thriving` until refreshed
    /// against the graze ecology config.
    pub fn new(tile: UVec2, carrying_capacity: f32) -> Self {
        Self {
            tile,
            biomass: carrying_capacity,
            carrying_capacity,
            ecology_phase: EcologyPhase::Thriving,
        }
    }

    /// Recompute `ecology_phase` from the current biomass against the graze ecology config.
    pub(crate) fn refresh_ecology_phase(&mut self, ecology: &EcologyConfig) {
        self.ecology_phase = classify_ecology_phase(self.biomass, self.carrying_capacity, ecology);
    }
}

/// The authoritative per-tile graze layer. Keyed by tile coord; **only tiles with a positive capacity
/// hold a patch**, so a barren biome (water, glacier, bare rock) is simply absent rather than present
/// with a zero — "no pasture here" and "eaten-out pasture" must never be the same reading.
#[derive(Resource, Debug, Clone, Default)]
pub struct GrazeRegistry {
    /// Live patches keyed by tile coord. Iteration order is non-deterministic; the snapshot capture
    /// sorts by coord for a stable rollback record.
    pub patches: HashMap<UVec2, GrazePatch>,
}

impl GrazeRegistry {
    pub fn patch(&self, tile: UVec2) -> Option<&GrazePatch> {
        self.patches.get(&tile)
    }

    pub fn patch_mut(&mut self, tile: UVec2) -> Option<&mut GrazePatch> {
        self.patches.get_mut(&tile)
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    pub fn len(&self) -> usize {
        self.patches.len()
    }

    /// Rebuild the authoritative patch map from a rollback snapshot's `GrazeState`s (clear + rebuild),
    /// mirroring `ForageRegistry::update_from_states`. Restores per-tile biomass / phase so a rollback
    /// rewinds grazing draw-down, not just display state.
    pub fn update_from_states(&mut self, states: &[GrazeState]) {
        self.patches = states
            .iter()
            .map(|state| {
                let patch = graze_patch_from_state(state);
                (patch.tile, patch)
            })
            .collect();
    }

    /// Construct a registry directly from snapshot `GrazeState`s (mirrors `ForageRegistry::from_states`).
    pub fn from_states(states: &[GrazeState]) -> Self {
        let mut registry = Self::default();
        registry.update_from_states(states);
        registry
    }

    /// Total graze capacity across every patch — the map's whole pasture budget. The measurement seam
    /// (and, from Phase 2b, the thing a herd's range takes a slice of).
    pub fn total_capacity(&self) -> f32 {
        self.patches
            .values()
            .map(|patch| patch.carrying_capacity)
            .sum()
    }

    /// **The richest pasture on the map** — the single source of "where is the best grass?", returning
    /// `(tile, carrying_capacity)`. `None` on a map with no pasture at all.
    ///
    /// **The tie-break is load-bearing, not decoration.** `capacity_by_biome` is a *per-biome* table, so
    /// on any real map the maximum is shared by **every tile of the richest biome** (a whole prairie
    /// steppe reads 240). `patches` is a `HashMap`, whose iteration order is randomised **per process**
    /// by Rust's default hasher — so a plain `max_by(capacity)` returns a *different tile every run*.
    /// Anything that then reads the winner's **surroundings** (a pen's fenced footprint, a herd's range)
    /// silently samples a different neighbourhood each time, which is how this produced a ~3-in-15
    /// flaky test. Ordering by `(capacity, then y, then x)` makes ties resolve to one stable tile
    /// regardless of hasher seed. **Do not "simplify" this back to a bare `max_by`.**
    pub fn richest_patch(&self) -> Option<(UVec2, f32)> {
        self.patches
            .iter()
            .map(|(tile, patch)| (*tile, patch.carrying_capacity))
            .max_by(|(a_tile, a_cap), (b_tile, b_cap)| {
                a_cap
                    .total_cmp(b_cap)
                    // Ties: the northern-most, then western-most tile wins. Any total order over the
                    // coord works — it only has to be independent of the hasher.
                    .then(b_tile.y.cmp(&a_tile.y))
                    .then(b_tile.x.cmp(&a_tile.x))
            })
    }
}

/// Reconstruct a live `GrazePatch` from its snapshot mirror (the rollback restore side of
/// `snapshot::graze_state`). The shared `EcologyState`'s `progress`/`owner` fields are unused by graze
/// (wild ground is never owned or improved) and are ignored here.
fn graze_patch_from_state(state: &GrazeState) -> GrazePatch {
    GrazePatch {
        tile: UVec2::new(state.x, state.y),
        biomass: state.ecology.biomass,
        carrying_capacity: state.ecology.carrying_capacity,
        ecology_phase: EcologyPhase::from_key(&state.ecology.ecology_phase),
    }
}

/// Seed a full patch on every **vegetated land** tile at Startup — one per tile whose biome has a
/// positive `graze.capacity_by_biome` entry, at `biomass = carrying_capacity`. Water is excluded by
/// its `TerrainTags::WATER` tag *and* by its zero capacity (belt and braces: the tag is the sim's
/// land/water authority, the table is the ecology's).
///
/// Idempotent — a world that already carries patches (a rollback restore) is skipped, the same guard
/// `spawn_initial_forage` uses. Runs in the Startup chain right after `spawn_initial_forage`, so the
/// biome stamping / tag solver / palette clamp have all had the last word on `Tile::terrain`.
pub fn spawn_initial_graze(
    mut registry: ResMut<GrazeRegistry>,
    fauna_config: Res<FaunaConfigHandle>,
    tiles: Query<&Tile>,
) {
    if !registry.patches.is_empty() {
        return;
    }
    let fauna = fauna_config.get();
    let graze = &fauna.graze;
    for tile in tiles.iter() {
        // Water carries no pasture — EXCEPT a navigable river, which yields the valley it was cut
        // through (its underlying biome, `resource_terrain()`), not open water: a navigable-over-
        // grassland hex grazes like grassland. No river fishing bonus here — you don't pasture
        // animals on the channel. Every other water tile is skipped.
        if tile.terrain_tags.contains(TerrainTags::WATER)
            && tile.terrain != TerrainType::NavigableRiver
        {
            continue;
        }
        let capacity = graze.capacity_for(tile.resource_terrain());
        if capacity <= NO_GRAZE_CAPACITY {
            continue;
        }
        let mut patch = GrazePatch::new(tile.position, capacity);
        patch.refresh_ecology_phase(&graze.ecology);
        registry.patches.insert(tile.position, patch);
    }
}

/// Per-turn graze regrowth (`TurnStage::Logistics`, alongside `advance_forage_regrowth`): regrow every
/// patch toward its biome capacity and refresh its ecology phase. Patches never despawn — grass
/// reseeds, so an eaten-out tile always recovers (slowly) rather than dying forever. Permanent
/// degradation (desertification) is a deliberate later lever, not this arc.
pub fn advance_graze_regrowth(
    mut registry: ResMut<GrazeRegistry>,
    fauna_config: Res<FaunaConfigHandle>,
) {
    let fauna = fauna_config.get();
    let graze = &fauna.graze;
    for patch in registry.patches.values_mut() {
        regrow_graze_patch(patch, graze);
    }
}

/// One turn of **pure logistic** regrowth toward the tile's capacity, over the shared reseed floor,
/// then a phase refresh. Unlike a wild herd (`fauna::regrow_biomass`, which crashes below the Allee
/// threshold and despawns) there is **no collapse branch**: grass has no depensation.
fn regrow_graze_patch(patch: &mut GrazePatch, graze: &GrazeConfig) {
    patch.biomass = reseeding_logistic_regrowth(
        patch.biomass,
        patch.carrying_capacity,
        graze.ecology.regrowth_rate,
        graze.reseed_floor_fraction,
    );
    patch.refresh_ecology_phase(&graze.ecology);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fauna_config::FaunaConfig;
    use sim_runtime::TerrainType;
    use sim_schema::EcologyState;

    fn test_graze_config() -> GrazeConfig {
        FaunaConfig::builtin().graze.clone()
    }

    #[test]
    fn the_biome_table_is_total_and_prairie_out_pastures_forest() {
        // The model claim the whole two-stock split rests on: open grassland is pasture, and a
        // closed-canopy forest is NOT (the canopy shades out the ground cover). If this ever
        // inverts, the agropastoral decision the layer exists to create has quietly evaporated.
        let graze = test_graze_config();
        for terrain in TerrainType::VALUES {
            assert!(
                graze.capacity_by_biome.contains_key(&terrain),
                "every biome needs a stated graze capacity (missing {terrain:?})"
            );
        }
        let prairie = graze.capacity_for(TerrainType::PrairieSteppe);
        assert!(prairie > graze.capacity_for(TerrainType::MixedWoodland));
        assert!(prairie > graze.capacity_for(TerrainType::BorealTaiga));
        assert!(prairie > graze.capacity_for(TerrainType::Tundra));
        assert!(prairie > graze.capacity_for(TerrainType::HotDesertErg));
        // Water, ice and bare rock carry nothing — a stated zero, not a defaulted one.
        for barren in [
            TerrainType::DeepOcean,
            TerrainType::ContinentalShelf,
            TerrainType::InlandSea,
            TerrainType::Glacier,
            TerrainType::BasalticLavaField,
            TerrainType::SaltFlat,
        ] {
            assert_eq!(graze.capacity_for(barren), NO_GRAZE_CAPACITY, "{barren:?}");
        }
    }

    #[test]
    fn graze_regrows_faster_than_forage_and_far_faster_than_fauna() {
        // The ladder of regrowth rates is a claim about biology, not a knob: grass is the quickest
        // vegetal stock in the model, which is what makes herding pay.
        let fauna = FaunaConfig::builtin();
        let forage = crate::labor_config::LaborConfig::builtin();
        assert!(fauna.graze.ecology.regrowth_rate > forage.forage.ecology.regrowth_rate);
        assert!(fauna.graze.ecology.regrowth_rate > fauna.ecology.regrowth_rate);
        // ...but below the fed pen's growth ceiling, which is a hyper-managed system rather than a
        // wild one. Since Grazing 2d the pen rate is per-species (`wild_r × pen_gain`, capped), so the
        // ceiling a fed pen can reach is `husbandry_regrowth_cap`.
        assert!(fauna.graze.ecology.regrowth_rate < fauna.husbandry.husbandry_regrowth_cap);
    }

    #[test]
    fn depleted_patch_regrows_toward_capacity() {
        let graze = test_graze_config();
        let cap = graze.capacity_for(TerrainType::PrairieSteppe);
        let mut patch = GrazePatch::new(UVec2::new(3, 4), cap);
        patch.biomass = 0.25 * cap;
        patch.refresh_ecology_phase(&graze.ecology);

        let mut prev = patch.biomass;
        for _ in 0..30 {
            regrow_graze_patch(&mut patch, &graze);
            assert!(patch.biomass >= prev, "regrowth must be monotonic upward");
            prev = patch.biomass;
        }
        assert!(patch.biomass > 0.9 * cap);
        assert!(patch.biomass <= cap);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
    }

    #[test]
    fn stripped_patch_reseeds_and_recovers_no_permanent_death() {
        // Grass has no Allee crash and no extinction: a tile eaten to *exactly* zero must escape
        // zero via the reseed floor (`logistic_regrowth(0, ..) == 0` would otherwise pin it there
        // forever) and climb all the way back. Overgrazing is recoverable, by design.
        let graze = test_graze_config();
        let cap = graze.capacity_for(TerrainType::PrairieSteppe);
        let floor = graze.reseed_floor_fraction * cap;
        assert!(floor > 0.0);

        let mut patch = GrazePatch::new(UVec2::new(5, 5), cap);
        patch.biomass = 0.0;
        patch.refresh_ecology_phase(&graze.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);

        regrow_graze_patch(&mut patch, &graze);
        assert!(patch.biomass >= floor, "a zeroed tile must escape 0");

        for _ in 0..60 {
            regrow_graze_patch(&mut patch, &graze);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass > graze.ecology.stressed_fraction * cap);
    }

    #[test]
    fn a_full_patch_stays_at_capacity() {
        // Phase 2a is INERT: nothing draws graze down, so a seeded map must sit at capacity forever
        // (logistic regrowth is 0 at K). If this test ever fails, something started eating.
        let graze = test_graze_config();
        let cap = graze.capacity_for(TerrainType::AlluvialPlain);
        let mut patch = GrazePatch::new(UVec2::new(1, 1), cap);
        patch.refresh_ecology_phase(&graze.ecology);
        for _ in 0..50 {
            regrow_graze_patch(&mut patch, &graze);
        }
        assert!((patch.biomass - cap).abs() < 1e-3);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
    }

    #[test]
    fn graze_state_roundtrip_is_identity() {
        let original = GrazeState {
            x: 9,
            y: 2,
            ecology: EcologyState {
                biomass: 61.5,
                carrying_capacity: 240.0,
                ecology_phase: "stressed".to_string(),
                // Graze is wild ground: never owned, never improved. The shared record's
                // cultivation fields ride at their defaults and must round-trip as such.
                progress: 0.0,
                owner: None,
            },
        };

        let registry = GrazeRegistry::from_states(std::slice::from_ref(&original));
        assert_eq!(registry.len(), 1);
        let patch = registry
            .patch(UVec2::new(9, 2))
            .expect("one patch restored");
        assert_eq!(patch.ecology_phase, EcologyPhase::Stressed);
        let restored = crate::snapshot::graze_state(patch);
        assert_eq!(restored, original);
    }
}
