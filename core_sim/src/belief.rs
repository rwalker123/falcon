//! **BELIEF ON A PLACE** — a per-tile stock, not a faction's (`docs/plan_civilization_steps.md`
//! §"Belief is a property of a place", §"The first pulls are not productive").
//!
//! A tile accrues belief from the things that happen *there*: deaths while a band stands on it (the
//! cemetery — evidence of returning), gatherings held on it, and a monument raised on it. Only the
//! first is wired; every source writes through the one seam, [`BeliefRegistry::add`].
//!
//! # Belief is monotone
//!
//! **Nothing ever subtracts from it or decays it.** An abandoned place keeps its dead: a band that
//! walks away leaves the belief where it accrued, and the stock reads the same a hundred turns
//! later. There is no decay lever in `belief_config.json` on purpose, and there is no `remove` /
//! `set` on the registry — a new source adds, it never rewrites.
//!
//! # The unit
//!
//! Dead-equivalents: at the shipped `belief_per_death` of `1.0`, a place reading `12` holds twelve
//! people's worth of ancestors. Later sources are priced in that unit.

use bevy::prelude::{Resource, UVec2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::belief_config::BeliefConfig;

/// What a tile with no entry holds — no belief has ever accrued there.
pub const NO_BELIEF: f32 = 0.0;

/// **EVERY PLACE THAT HOLDS BELIEF, ONE VALUE PER TILE** — `routes::RoadRegistry`'s shape.
///
/// **Sparse**: only a tile whose belief is above [`NO_BELIEF`] has an entry, so an untouched map
/// checkpoints an empty map. **`BTreeMap`, not `HashMap`** — the iteration order is observed by the
/// checkpoint, so it has to be an order and not an accident. Keyed `(y, x)` so that order is
/// row-major, like every other tile sweep in the engine.
#[derive(Resource, Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeliefRegistry {
    tiles: BTreeMap<(u32, u32), f32>,
}

impl BeliefRegistry {
    /// **THE SEAM EVERY BELIEF SOURCE WRITES THROUGH** — deaths now
    /// ([`BeliefRegistry::credit_deaths`]); gatherings (#698) and the monument (#692) later. Adds
    /// `amount` to the tile at `position`. An `amount` at or below [`NO_BELIEF`] is ignored, which
    /// is what keeps belief monotone and the registry sparse: no caller can draw a place down, and
    /// a zero accrual never mints an entry.
    pub fn add(&mut self, position: UVec2, amount: f32) {
        if amount <= NO_BELIEF {
            return;
        }
        *self
            .tiles
            .entry((position.y, position.x))
            .or_insert(NO_BELIEF) += amount;
    }

    /// The belief standing on the tile at `position`; [`NO_BELIEF`] where none has ever accrued.
    pub fn get(&self, position: UVec2) -> f32 {
        self.tiles
            .get(&(position.y, position.x))
            .copied()
            .unwrap_or(NO_BELIEF)
    }

    /// **The deaths source.** `deaths` people died while their band stood on `position` — credit
    /// the place at `belief_per_death` each. Fractional deaths accrue exactly: belief is a
    /// continuous stock, so a third of a person three turns running is one death's worth.
    ///
    /// The caller decides *where*: the tile the band **stands on**, never its home, and only for
    /// people who died there. A detached expedition's casualties and a far work party's (a row
    /// posted past `band_work_range`) are not where the band stands, and credit nothing.
    pub fn credit_deaths(&mut self, position: UVec2, deaths: f32, config: &BeliefConfig) {
        self.add(position, deaths * config.belief_per_death);
    }

    /// Every place holding belief, in row-major order.
    pub fn iter(&self) -> impl Iterator<Item = (UVec2, f32)> + '_ {
        self.tiles
            .iter()
            .map(|(&(y, x), &belief)| (UVec2::new(x, y), belief))
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACE: UVec2 = UVec2::new(3, 7);
    const ELSEWHERE: UVec2 = UVec2::new(7, 3);
    /// A whole death's belief at the shipped `belief_per_death`.
    const ONE_DEATH: f32 = 1.0;
    /// A third of a person — the fractional flow the demographic model produces.
    const A_THIRD_OF_A_DEATH: f32 = 1.0 / 3.0;
    const TOLERANCE: f32 = 1e-5;

    #[test]
    fn an_untouched_place_holds_no_belief_and_no_entry() {
        let registry = BeliefRegistry::default();
        assert_eq!(registry.get(PLACE), NO_BELIEF);
        assert!(registry.is_empty());
    }

    #[test]
    fn add_accrues_on_the_named_tile_only() {
        let mut registry = BeliefRegistry::default();
        registry.add(PLACE, ONE_DEATH);
        registry.add(PLACE, ONE_DEATH);
        assert_eq!(registry.get(PLACE), ONE_DEATH + ONE_DEATH);
        assert_eq!(registry.get(ELSEWHERE), NO_BELIEF);
    }

    /// The seam cannot draw a place down, and a zero accrual mints no entry.
    #[test]
    fn a_non_positive_amount_is_ignored() {
        let mut registry = BeliefRegistry::default();
        registry.add(PLACE, NO_BELIEF);
        assert!(registry.is_empty(), "a zero accrual must not mint an entry");
        registry.add(PLACE, ONE_DEATH);
        registry.add(PLACE, -ONE_DEATH);
        assert_eq!(registry.get(PLACE), ONE_DEATH);
    }

    #[test]
    fn fractional_deaths_accrue_exactly() {
        let mut registry = BeliefRegistry::default();
        let config = BeliefConfig::default();
        for _ in 0..3 {
            registry.credit_deaths(PLACE, A_THIRD_OF_A_DEATH, &config);
        }
        assert!((registry.get(PLACE) - ONE_DEATH).abs() < TOLERANCE);
    }

    #[test]
    fn belief_per_death_scales_the_deaths_source() {
        /// A retuned lever, distinct from the shipped `1.0`.
        const TRIPLED: f32 = 3.0;
        let mut registry = BeliefRegistry::default();
        let config = BeliefConfig {
            belief_per_death: TRIPLED,
        };
        registry.credit_deaths(PLACE, ONE_DEATH, &config);
        assert_eq!(registry.get(PLACE), TRIPLED);
    }

    #[test]
    fn iteration_is_row_major() {
        let mut registry = BeliefRegistry::default();
        registry.add(UVec2::new(1, 2), ONE_DEATH);
        registry.add(UVec2::new(5, 0), ONE_DEATH);
        registry.add(UVec2::new(0, 2), ONE_DEATH);
        let order: Vec<UVec2> = registry.iter().map(|(tile, _)| tile).collect();
        assert_eq!(
            order,
            vec![UVec2::new(5, 0), UVec2::new(0, 2), UVec2::new(1, 2)]
        );
    }
}
