//! **THE REFERENCE BASKET** — the one pinned patch of ground the plant web's figures are quoted on.
//!
//! `AlluvialPlain`, tile `(0, 0)`, under the shipped `sweep_tiles` fixture seed: `wild_emmer` 0.375 /
//! `wild_tubers` 0.292 / `tobacco` 0.208 / `wild_rice` 0.125. Every constant below is a **fixture
//! pin**, not a gameplay lever — the seed and the tile decide *which* realization is being measured,
//! and moving either would silently re-quote every figure taken against it.
//!
//! It lives here rather than in one harness because two now anchor on it (`field_reference_basket.rs`
//! pins rung 3's payoff, `food_economy_table.rs` prints the whole plant ladder beside the animal one),
//! and two copies of a pinned realization is exactly the drift a pin exists to prevent.

use bevy::math::UVec2;
use core_sim::{FloraConfig, FloraShare, LaborConfig};
use sim_runtime::TerrainType;

/// The pinned realization seed — the one the shipped `sweep_tiles` fixtures use.
pub const SEED: u64 = 0x_F10A_5EED_C011_0010;
/// The pinned tile.
pub const TILE: UVec2 = UVec2::new(0, 0);
/// The pinned ground.
pub const TERRAIN: TerrainType = TerrainType::AlluvialPlain;
/// The pinned crop — the basket's best staple on this ground.
pub const CROP: &str = "wild_emmer";

/// **This tile's realized basket** — resolved through the sim's own realization, so a retuned roster
/// moves the fixture with the game.
pub fn composition(flora: &FloraConfig) -> Vec<FloraShare> {
    composition_of(flora, TERRAIN)
}

/// **The patch's `K`** — the biome's own forage capacity, off the shipped per-biome table.
pub fn capacity(labor: &LaborConfig) -> f32 {
    capacity_of(labor, TERRAIN)
}

/// **The same pinned realization on ANY ground** — the seed and the tile are the pin; the terrain is
/// the question. `composition`/`capacity` above are this at [`TERRAIN`], and the validation block of
/// `food_economy_table.rs` asks it about the biome a live reading was taken on instead.
pub fn composition_of(flora: &FloraConfig, terrain: TerrainType) -> Vec<FloraShare> {
    flora.realized_composition(terrain, TILE, SEED)
}

/// [`capacity`] on any ground — the biome's own forage capacity, off the shipped per-biome table.
pub fn capacity_of(labor: &LaborConfig, terrain: TerrainType) -> f32 {
    labor.forage.capacity_for(terrain)
}
