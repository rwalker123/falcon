//! The per-tile flora quote memo — what each patch tile's forage capacity decomposes into, and
//! what each named plant on it would pay per turn once committed to at each rung.
//!
//! # Why this is a memo and not a per-turn derivation
//!
//! The quotes are a **pure function of ground and config**. Every input to
//! [`derive_tile_quotes`] is one of: the tile's `position`, its `terrain`, its `resource_terrain()`,
//! the `map_seed`, the flora roster, the forage config, and the `FORECAST_OUTPUT_MULTIPLIER`
//! constant. No live `ForagePatch` state reaches any of them — the payoffs are taken against the
//! **tile's own `K`** through `forage::hypothetical_patch`, never the standing patch's, precisely so
//! that a 25-turn investment is not priced off one transient turn's biomass.
//!
//! Terrain is written only by worldgen and hydrology, both worldgen-time. So the derivation produced
//! byte-identical output every turn for the whole life of a world, at ~42% of `snapshot.build`
//! (issue #410) — and, worse, at a cost that **multiplies**: the inner loop prices every flora share
//! at every payoff account, so a fifth account cost ~+0.30 ms of *every turn, forever*. Behind this
//! memo it costs that once per world.
//!
//! # The invalidation is the complete input set, not a guess about what moves
//!
//! Nothing here relies on the claim "terrain never changes". The cached identity **is** the
//! function's input set, split by scope:
//!
//! - **World-level** (`map_seed`, `grid_size`, and the flora / labor config `Arc`s by pointer
//!   identity) — checked once per capture in [`FloraQuoteCache::sweep`], which clears everything on
//!   a mismatch. `Arc::ptr_eq` is exact for a config hot reload, because `*ConfigHandle::replace`
//!   swaps the `Arc` rather than mutating through it.
//! - **Per-tile** (`terrain`, `resource_terrain`) — checked on every lookup, so a tile whose ground
//!   is rewritten mid-game re-derives on the next capture whether or not anyone remembered this file
//!   existed.
//!
//! That is why the type is not sim state and needs no restore path: a rollback restores into the
//! same live `World` holding the same map, and anything that *did* move is caught by one of the two
//! checks above.
//!
//! # What is deliberately NOT memoized
//!
//! The `plant:field` **site refusal** stays a per-turn derivation at its call site, for a reason
//! worth stating: `forage::tile_is_fresh_watered` reads a tile's *neighbours'* tags, so its input set
//! is not local to the tile and could not be fingerprinted per entry the way the quotes are. It is
//! also cheap — a flat `TerrainTagGrid` probe per neighbour — so there is nothing to win by trying.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use sim_runtime::{FloraShareInfo, TerrainType};

use crate::components::Tile;
use crate::flora_config::FloraConfig;
use crate::forage::{
    commit_fodder_payoff, commit_payoff, commit_trade_payoff, commit_yield_ratio,
    tile_flora_composition, tile_forage_capacity, wild_payoff,
};
use crate::intensification::RungKey;
use crate::labor_config::{ForageLaborConfig, LaborConfig};

use super::FORECAST_OUTPUT_MULTIPLIER;

/// One tile's memoized quotes, beside the ground they were derived from.
struct CachedQuotes {
    terrain: TerrainType,
    resource_terrain: TerrainType,
    shares: Vec<FloraShareInfo>,
}

/// The memo. Keyed by tile coord; filled lazily by [`FloraQuoteCache::sweep`] during
/// `snapshot.build.patches` and read back by the `forage_patches` readout in the same capture.
///
/// **Not sim state, and not written by a system** — see the module docs. It holds no authority: every
/// entry is reproducible from the ground and the config at any time, which is what makes losing it
/// (a fresh app, a cleared identity) merely slow rather than wrong.
#[derive(Resource, Default)]
pub struct FloraQuoteCache {
    /// The world-level identity the held entries were derived under. `None` before the first sweep.
    identity: Option<QuoteIdentity>,
    entries: HashMap<UVec2, CachedQuotes>,
}

/// The world-level half of the input set. Config handles are compared by **pointer**, which is exact
/// for a hot reload and free for the common case where nothing moved.
struct QuoteIdentity {
    flora: Arc<FloraConfig>,
    labor: Arc<LaborConfig>,
    map_seed: u64,
    grid_size: UVec2,
}

impl QuoteIdentity {
    fn matches(
        &self,
        flora: &Arc<FloraConfig>,
        labor: &Arc<LaborConfig>,
        seed: u64,
        grid: UVec2,
    ) -> bool {
        self.map_seed == seed
            && self.grid_size == grid
            && Arc::ptr_eq(&self.flora, flora)
            && Arc::ptr_eq(&self.labor, labor)
    }
}

impl FloraQuoteCache {
    /// Open a sweep over this capture's ground, clearing every entry first if any **world-level**
    /// input moved (a config reload, a re-seeded or re-sized map).
    ///
    /// Going through this is the only way to reach [`FloraQuoteSweep::quotes`], which is what makes
    /// the invalidation unskippable rather than a step a later caller has to remember.
    pub(crate) fn sweep<'a>(
        &'a mut self,
        flora: &'a Arc<FloraConfig>,
        labor: &'a Arc<LaborConfig>,
        map_seed: u64,
        grid_size: UVec2,
    ) -> FloraQuoteSweep<'a> {
        let held = self
            .identity
            .as_ref()
            .is_some_and(|identity| identity.matches(flora, labor, map_seed, grid_size));
        if !held {
            self.entries.clear();
            self.identity = Some(QuoteIdentity {
                flora: Arc::clone(flora),
                labor: Arc::clone(labor),
                map_seed,
                grid_size,
            });
        }
        FloraQuoteSweep {
            entries: &mut self.entries,
            flora,
            forage: &labor.forage,
            map_seed,
        }
    }

    /// What grows on this tile, for the readout. **Empty for a coord the sweep never visited** —
    /// "unknown ground names no plants", the same absent-means-nothing convention `seasonal_weights`
    /// and `sow_site_refusals` use, never a fabricated basket.
    pub(crate) fn composition(&self, tile: UVec2) -> &[FloraShareInfo] {
        self.entries
            .get(&tile)
            .map_or(&[][..], |cached| &cached.shares)
    }

    /// Entries currently held — for the guards, which assert on what the memo *did* rather than on
    /// the timings it changed.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// A borrow of the memo bound to one capture's world-level inputs. See [`FloraQuoteCache::sweep`].
pub(crate) struct FloraQuoteSweep<'a> {
    entries: &'a mut HashMap<UVec2, CachedQuotes>,
    flora: &'a FloraConfig,
    forage: &'a ForageLaborConfig,
    map_seed: u64,
}

impl FloraQuoteSweep<'_> {
    /// This tile's quotes, derived only if the ground under them has moved (or was never seen).
    pub(crate) fn quotes(&mut self, tile: &Tile) -> &[FloraShareInfo] {
        let resource_terrain = tile.resource_terrain();
        let stale = self.entries.get(&tile.position).is_none_or(|cached| {
            cached.terrain != tile.terrain || cached.resource_terrain != resource_terrain
        });
        if stale {
            let shares = derive_tile_quotes(self.flora, self.forage, tile, self.map_seed);
            self.entries.insert(
                tile.position,
                CachedQuotes {
                    terrain: tile.terrain,
                    resource_terrain,
                    shares,
                },
            );
        }
        &self.entries[&tile.position].shares
    }
}

/// **What grows on this tile, and what each plant would pay once committed to** — the whole derived
/// block, in one pure function of ground and config so the memo above has something exact to key on.
///
/// The quotes are taken against **this tile's own `K`** — never the live patch's, which may already
/// be concentrated by an existing commitment — and at the standing crop each rung *settles* at, so
/// they answer "what would this ground pay once this crop is established here" rather than pricing a
/// 25-turn investment off one transient turn.
fn derive_tile_quotes(
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    tile: &Tile,
    map_seed: u64,
) -> Vec<FloraShareInfo> {
    let tile_capacity = tile_forage_capacity(forage, tile);
    // What this tile pays left wild — the denominator every ratio on this tile divides by, resolved
    // once.
    let wild = wild_payoff(
        tile.position,
        tile_capacity,
        flora,
        forage,
        FORECAST_OUTPUT_MULTIPLIER,
    );
    tile_flora_composition(flora, forage, tile, map_seed)
        .iter()
        .map(|share| {
            let def = &flora.species[&share.species];
            // **What this tile would pay per turn once committed to THIS plant**, per rung — through
            // `forage::commit_payoff`, which builds the patch the sim would have and asks the *same*
            // payoff functions the sim quotes and pays each rung with (`tended_provisions` /
            // `field_provisions`). Nothing is re-derived here, which is what stops the published
            // number and the payout from drifting.
            let payoff = |rung| {
                commit_payoff(
                    tile.position,
                    tile_capacity,
                    &share.species,
                    share.share,
                    flora,
                    forage,
                    FORECAST_OUTPUT_MULTIPLIER,
                    rung,
                )
            };
            let cultivate = payoff(RungKey::PlantTended);
            let sow = payoff(RungKey::PlantField);
            FloraShareInfo {
                species: share.species.clone(),
                display_name: def.display_name.clone(),
                share: share.share,
                // **Which rungs this plant can EVER climb** (Flora Roster S1) — its own
                // `cultivation_ceiling`, straight off the roster, so the client's crop picker can
                // grey out what is impossible without holding a roster of its own. Species-global:
                // it says nothing about whether this tile is a good place for it — the payoff/ratio
                // below answer that, and a legal-but-marginal crop is exactly the loss §4.3 leaves
                // the player free to choose.
                can_cultivate: def.cultivation_ceiling.allows_cultivate(),
                can_sow: def.cultivation_ceiling.allows_sow(),
                cultivate_payoff: cultivate,
                sow_payoff: sow,
                // **Is it worth it?** — the same payoffs over the same wild payoff, so the ratio can
                // never disagree with the numbers it relates.
                cultivate_yield_ratio: commit_yield_ratio(cultivate, wild),
                sow_yield_ratio: commit_yield_ratio(sow, wild),
                // **What a hay Field of this plant would pay into the FODDER account** (F3) —
                // through the same `commit_fodder_payoff` seam the sim's `field_fodder` pays with,
                // so the picker can show hay's value where `sow_yield_ratio` reads 0×. `0` for a
                // staple (no fodder in its vector) or a plant that cannot Sow here.
                sow_fodder_payoff: commit_fodder_payoff(
                    tile.position,
                    tile_capacity,
                    &share.species,
                    share.share,
                    flora,
                    forage,
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
                // **What a cash-crop Field of this plant would pay into the TRADE account** (F4) —
                // the exact trade twin, through the same `commit_trade_payoff` seam the sim's
                // `field_trade_goods` pays with, so the picker can show a cash crop's value where
                // `sow_yield_ratio` reads 0×. `0` for a staple/hay or a plant that cannot Sow here.
                sow_trade_payoff: commit_trade_payoff(
                    tile.position,
                    tile_capacity,
                    &share.species,
                    share.share,
                    flora,
                    forage,
                    FORECAST_OUTPUT_MULTIPLIER,
                ),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Tile;

    fn tile_at(position: UVec2, terrain: TerrainType) -> Tile {
        Tile {
            position,
            terrain,
            terrain_tags: crate::terrain::terrain_definition(terrain).tags,
            ..Default::default()
        }
    }

    fn configs() -> (Arc<FloraConfig>, Arc<LaborConfig>) {
        (FloraConfig::builtin(), LaborConfig::builtin())
    }

    /// The memo answers what the derivation answers — the property everything else here rests on.
    /// Asserted against `derive_tile_quotes` itself rather than against a recorded expectation, so a
    /// retune of the payoff tables moves both sides at once and the guard keeps its meaning.
    #[test]
    fn a_cached_tile_quotes_exactly_what_a_fresh_derivation_does() {
        let (flora, labor) = configs();
        let mut cache = FloraQuoteCache::default();
        let tiles: Vec<Tile> = (0..8)
            .map(|x| tile_at(UVec2::new(x, 3), TerrainType::MixedWoodland))
            .collect();

        let mut sweep = cache.sweep(&flora, &labor, 99, UVec2::new(16, 16));
        for tile in &tiles {
            let cached = sweep.quotes(tile).to_vec();
            let fresh = derive_tile_quotes(&flora, &labor.forage, tile, 99);
            assert_eq!(cached, fresh, "tile {:?}", tile.position);
        }

        // And again on the second turn, off the memo rather than the derivation.
        let mut sweep = cache.sweep(&flora, &labor, 99, UVec2::new(16, 16));
        for tile in &tiles {
            let cached = sweep.quotes(tile).to_vec();
            let fresh = derive_tile_quotes(&flora, &labor.forage, tile, 99);
            assert_eq!(
                cached, fresh,
                "tile {:?} on the second sweep",
                tile.position
            );
        }
    }

    /// Two tiles of one biome carry different baskets (per-tile realization is keyed on
    /// `(map_seed, tile)`), so a memo keyed on the biome alone would be wrong. This is what proves
    /// the key is the *tile*.
    #[test]
    fn the_memo_is_per_tile_not_per_biome() {
        let (flora, labor) = configs();
        let mut cache = FloraQuoteCache::default();
        let grid = UVec2::new(64, 64);
        let mut sweep = cache.sweep(&flora, &labor, 7, grid);
        let baskets: Vec<Vec<FloraShareInfo>> = (0..24)
            .map(|x| {
                sweep
                    .quotes(&tile_at(UVec2::new(x, 5), TerrainType::MixedWoodland))
                    .to_vec()
            })
            .collect();
        assert!(
            baskets.iter().any(|basket| basket != &baskets[0]),
            "every tile of one biome realized the same basket — the realization seam is not being reached"
        );
    }

    /// Ground rewritten under a held entry re-derives. Nothing in the shipped sim does this today —
    /// the guard exists so that the day something does, the memo is not the thing that goes stale
    /// silently.
    #[test]
    fn rewriting_a_tiles_ground_re_derives_that_entry() {
        let (flora, labor) = configs();
        let mut cache = FloraQuoteCache::default();
        let grid = UVec2::new(16, 16);
        let position = UVec2::new(2, 2);

        let mut sweep = cache.sweep(&flora, &labor, 5, grid);
        let forest = sweep
            .quotes(&tile_at(position, TerrainType::MixedWoodland))
            .to_vec();

        let rewritten = tile_at(position, TerrainType::HotDesertErg);
        let mut sweep = cache.sweep(&flora, &labor, 5, grid);
        let desert = sweep.quotes(&rewritten).to_vec();

        assert_eq!(
            desert,
            derive_tile_quotes(&flora, &labor.forage, &rewritten, 5)
        );
        assert_ne!(
            forest, desert,
            "the memo answered a forest basket for ground that is now desert"
        );
    }

    /// A config hot reload drops every entry. `Arc::ptr_eq` is the check, so this holds even when the
    /// reloaded file parses to an identical value — a false *clear* costs one re-derivation, where a
    /// false *hold* would publish the old tuning forever.
    #[test]
    fn a_config_reload_clears_the_whole_memo() {
        let (flora, labor) = configs();
        let mut cache = FloraQuoteCache::default();
        let grid = UVec2::new(16, 16);
        let mut sweep = cache.sweep(&flora, &labor, 5, grid);
        for x in 0..4 {
            sweep.quotes(&tile_at(UVec2::new(x, 0), TerrainType::MixedWoodland));
        }
        assert_eq!(cache.len(), 4);

        let reloaded = FloraConfig::builtin();
        cache.sweep(&reloaded, &labor, 5, grid);
        assert_eq!(cache.len(), 0, "a swapped flora config left entries behind");

        let mut sweep = cache.sweep(&reloaded, &labor, 5, grid);
        sweep.quotes(&tile_at(UVec2::new(0, 0), TerrainType::MixedWoodland));
        assert_eq!(cache.len(), 1);
        cache.sweep(&reloaded, &LaborConfig::builtin(), 5, grid);
        assert_eq!(cache.len(), 0, "a swapped labor config left entries behind");
    }

    /// A re-seeded or re-sized map drops every entry — the seed is an input to per-tile realization,
    /// and the size bounds which coords can ever be read again.
    #[test]
    fn a_reseeded_or_resized_map_clears_the_whole_memo() {
        let (flora, labor) = configs();
        let mut cache = FloraQuoteCache::default();
        let grid = UVec2::new(16, 16);
        let seed_tile = tile_at(UVec2::new(1, 1), TerrainType::MixedWoodland);

        let mut sweep = cache.sweep(&flora, &labor, 5, grid);
        sweep.quotes(&seed_tile);
        cache.sweep(&flora, &labor, 6, grid);
        assert_eq!(cache.len(), 0, "a new map seed left entries behind");

        let mut sweep = cache.sweep(&flora, &labor, 6, grid);
        sweep.quotes(&seed_tile);
        cache.sweep(&flora, &labor, 6, UVec2::new(32, 32));
        assert_eq!(cache.len(), 0, "a resized map left entries behind");
    }

    /// A coord the sweep never visited names no plants. The readout relies on this for a patch whose
    /// tile is absent from the map — an empty basket, never a fabricated one.
    #[test]
    fn an_unvisited_coord_names_no_plants() {
        let cache = FloraQuoteCache::default();
        assert!(cache.composition(UVec2::new(4, 4)).is_empty());
    }
}
