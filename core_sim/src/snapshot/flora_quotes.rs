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
//! - **World-level** (`map_seed`, `grid_size`, and the flora / labor / ladder config `Arc`s by
//!   pointer identity — the ladder because each crop's published Sow price is the `plant:field`
//!   rung's own `work_cost` times what that crop's share earns) — checked once per capture in
//!   [`FloraQuoteCache::sweep`], which clears everything on a mismatch. `Arc::ptr_eq` is exact for a
//!   config hot reload, because `*ConfigHandle::replace` swaps the `Arc` rather than mutating
//!   through it.
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

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use bevy::prelude::*;
use sim_runtime::{FloraShareInfo, MaterialPayoff, TerrainType};

use crate::components::Tile;
use crate::flora_config::{FloraConfig, FloraShare};
use crate::forage::{
    commit_fodder_payoff, commit_material_payoff, commit_payoff, commit_yield_ratio,
    crop_field_cost_multiplier, tile_flora_composition, tile_forage_capacity, wild_payoff,
};
use crate::intensification::{LadderConfig, RungKey};
use crate::labor_config::{ForageLaborConfig, LaborConfig};

use super::FORECAST_OUTPUT_MULTIPLIER;

/// One tile's memoized quotes, beside the ground they were derived from.
///
/// `shares` is an `Arc<[_]>` because the readout hands this exact basket to the wire state
/// ([`FloraQuoteCache::composition`]) whenever the patch on it is wild: the tile owns it, so such a
/// patch row shares it rather than deep-copying two `String`s per named plant every turn.
///
/// `composition` is the same basket in its **raw** `FloraShare` form — the input every rate seam in
/// `forage.rs` takes, and the thing a *committed* patch reweights
/// ([`crate::forage::patch_composition`]). Held here rather than re-derived per patch because the
/// realization is a seeded draw over the whole affinity roster, which is exactly the work this memo
/// exists to do once.
struct CachedQuotes {
    terrain: TerrainType,
    resource_terrain: TerrainType,
    composition: Arc<[FloraShare]>,
    shares: Arc<[FloraShareInfo]>,
}

/// The basket handed back for ground the sweep never visited. Built once rather than per call —
/// `Arc<[_]>` allocates its header even when empty, and this is the miss path of a per-patch lookup.
fn no_plants_here() -> &'static Arc<[FloraShareInfo]> {
    static EMPTY: OnceLock<Arc<[FloraShareInfo]>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::from(Vec::new()))
}

/// The raw twin of [`no_plants_here`] — "unknown ground names no plants", for the seams that take a
/// `FloraShare` basket. Same once-built rationale.
fn no_shares_here() -> &'static [FloraShare] {
    static EMPTY: OnceLock<Vec<FloraShare>> = OnceLock::new();
    EMPTY.get_or_init(Vec::new)
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
    ladder: Arc<LadderConfig>,
    map_seed: u64,
    grid_size: UVec2,
}

impl QuoteIdentity {
    fn matches(
        &self,
        flora: &Arc<FloraConfig>,
        labor: &Arc<LaborConfig>,
        ladder: &Arc<LadderConfig>,
        seed: u64,
        grid: UVec2,
    ) -> bool {
        self.map_seed == seed
            && self.grid_size == grid
            && Arc::ptr_eq(&self.flora, flora)
            && Arc::ptr_eq(&self.labor, labor)
            && Arc::ptr_eq(&self.ladder, ladder)
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
        ladder: &'a Arc<LadderConfig>,
        map_seed: u64,
        grid_size: UVec2,
    ) -> FloraQuoteSweep<'a> {
        let held = self
            .identity
            .as_ref()
            .is_some_and(|identity| identity.matches(flora, labor, ladder, map_seed, grid_size));
        if !held {
            self.entries.clear();
            self.identity = Some(QuoteIdentity {
                flora: Arc::clone(flora),
                labor: Arc::clone(labor),
                ladder: Arc::clone(ladder),
                map_seed,
                grid_size,
            });
        }
        FloraQuoteSweep {
            entries: &mut self.entries,
            flora,
            forage: &labor.forage,
            ladder,
            map_seed,
        }
    }

    /// What grows on this tile, for the readout. **Empty for a coord the sweep never visited** —
    /// "unknown ground names no plants", the same absent-means-nothing convention `seasonal_weights`
    /// and `sow_site_refusals` use, never a fabricated basket.
    ///
    /// Hands back the memo's own `Arc`, which is what the wire state then holds: the basket is a
    /// property of the tile, so a patch row costs one refcount bump rather than a deep copy of every
    /// named plant. Safe to share because nothing downstream mutates a published composition.
    pub(crate) fn composition(&self, tile: UVec2) -> Arc<[FloraShareInfo]> {
        self.entries.get(&tile).map_or_else(
            || Arc::clone(no_plants_here()),
            |cached| Arc::clone(&cached.shares),
        )
    }

    /// **What is growing on this tile, in the raw form the rate seams read** — the same basket
    /// [`Self::composition`] publishes, before any patch reweight. Every forage payoff a capture
    /// quotes derives the *patch's* basket from this one, so both come out of one memo entry and
    /// cannot disagree about what the tile grows. **Empty for a coord the sweep never visited**, the
    /// same absent-means-nothing convention.
    pub(crate) fn tile_composition(&self, tile: UVec2) -> &[FloraShare] {
        self.entries
            .get(&tile)
            .map_or_else(|| no_shares_here(), |cached| &cached.composition)
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
    /// The rung price list — the `plant:field` `work_cost` each crop's published Sow price is a
    /// multiple of. Part of the memo's identity for the same reason the other two config handles
    /// are.
    ladder: &'a LadderConfig,
    map_seed: u64,
}

impl FloraQuoteSweep<'_> {
    /// This tile's quotes, derived only if the ground under them has moved (or was never seen).
    ///
    /// One `Entry` lookup, so the common case — the hit, once per patch tile per capture — costs a
    /// single hash probe and hands back the held entry directly rather than looking it up again.
    pub(crate) fn quotes(&mut self, tile: &Tile) -> &[FloraShareInfo] {
        // Destructured so the entry's mutable borrow of `entries` stays disjoint from the reads of
        // the config fields the derivation needs.
        let Self {
            entries,
            flora,
            forage,
            ladder,
            map_seed,
        } = self;
        let resource_terrain = tile.resource_terrain();
        let derive = || {
            let (composition, shares) = derive_tile_quotes(flora, forage, ladder, tile, *map_seed);
            CachedQuotes {
                terrain: tile.terrain,
                resource_terrain,
                composition: Arc::from(composition),
                shares: Arc::from(shares),
            }
        };
        let held = match entries.entry(tile.position) {
            Entry::Occupied(slot) => {
                let held = slot.into_mut();
                if held.terrain != tile.terrain || held.resource_terrain != resource_terrain {
                    *held = derive();
                }
                held
            }
            Entry::Vacant(slot) => slot.insert(derive()),
        };
        &held.shares
    }
}

/// **What grows on this tile, and what each plant would pay once committed to** — the whole derived
/// block, in one pure function of ground and config so the memo above has something exact to key on.
/// Returns the tile's raw basket beside the quoted one, because both are memo entries and both come
/// out of the same realization draw.
///
/// The quotes are taken against **this tile's own `K`** — never the live patch's — and at the
/// standing crop each rung *settles* at, so they answer "what would this ground pay once this crop is
/// established here" rather than pricing a 25-turn investment off one transient turn.
fn derive_tile_quotes(
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    ladder: &LadderConfig,
    tile: &Tile,
    map_seed: u64,
) -> (Vec<FloraShare>, Vec<FloraShareInfo>) {
    let tile_capacity = tile_forage_capacity(forage, tile);
    let composition = tile_flora_composition(flora, forage, tile, map_seed).into_owned();
    // What this tile pays left wild — the denominator every ratio on this tile divides by, resolved
    // once. It reads the composition because a wild gather is the basket's own average (#433).
    let wild = wild_payoff(
        tile.position,
        tile_capacity,
        &composition,
        flora,
        forage,
        FORECAST_OUTPUT_MULTIPLIER,
    );
    let quotes = composition
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
                    &composition,
                    flora,
                    forage,
                    FORECAST_OUTPUT_MULTIPLIER,
                    rung,
                )
            };
            let cultivate = payoff(RungKey::PlantTended);
            let sow = payoff(RungKey::PlantField);
            // The two non-food accounts, per rung, through the same rung-parameterized seams — so the
            // Cultivate row of the crop picker states what a TENDED patch of this plant pays rather
            // than borrowing the Field's number (issue #419). Same closure shape as `payoff` above,
            // and for the same reason: the rung is an argument, never a hardcoded arm.
            let fodder_payoff = |rung| {
                commit_fodder_payoff(
                    tile.position,
                    tile_capacity,
                    &share.species,
                    &composition,
                    flora,
                    forage,
                    FORECAST_OUTPUT_MULTIPLIER,
                    rung,
                )
            };
            // The MATERIAL account, same shape again — and the one that answers per material rather
            // than with a single number, because that is what a material yield *is* (arc #527).
            let material_payoff = |rung| {
                commit_material_payoff(
                    tile.position,
                    tile_capacity,
                    &share.species,
                    &composition,
                    flora,
                    forage,
                    FORECAST_OUTPUT_MULTIPLIER,
                    rung,
                )
                .into_iter()
                .map(|payoff| MaterialPayoff {
                    material_id: payoff.material,
                    amount: payoff.amount,
                })
                .collect()
            };
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
                sow_fodder_payoff: fodder_payoff(RungKey::PlantField),
                // **The same account one rung down** (#419) — what a completed TENDED PATCH of
                // this plant would pay, through `tended_fodder`. The Cultivate row of the picker had
                // only the Field figure above and quoted that, which is a managed rate standing in
                // for an MSY skim on a rung the player commits 25 turns to.
                //
                // A cash crop quotes nothing *here* — its account is the material one below.
                cultivate_fodder_payoff: fodder_payoff(RungKey::PlantTended),
                // **What this plant is FOR** — the roster's own `role`, shipped verbatim as the
                // display tag it is. Taken off `def` rather than re-read from the yield vector here,
                // because a tag whose whole purpose is to be ONE definition must have exactly one
                // place that decides it (`FloraRole`). Nothing in the sim branches on it.
                role: def.role.as_str().to_string(),
                // **What a cash crop would pay, PER MATERIAL** (arc #527) — the replacement for the
                // retired `sow_trade_payoff` / `cultivate_trade_payoff`, and the only thing on this
                // row that can state a cotton Field's whole product. Through the same
                // `commit_material_payoff` seam the sim's `credit_material_yield` is paid off, at
                // each rung's own harvest, so quote and payout cannot drift.
                //
                // **Empty is "no row", never "zero"** — a food crop yields no material, and a `0`
                // would read as a cash crop that pays badly.
                sow_material_payoff: material_payoff(RungKey::PlantField),
                cultivate_material_payoff: material_payoff(RungKey::PlantTended),
                // **WHAT SOWING THIS CROP WOULD COST** (`docs/plan_standing_upkeep.md` §4.15) — the
                // cost half of the picker's decision, and the only figure on this row that is not a
                // payoff. A Sow is priced by how much of the tile the chosen crop still has to
                // replace, and the patch's own `fieldWorkCost` prices exactly ONE crop (its
                // commitment, or the rung's auto-pick), so every row of a crop list quoted that same
                // number while the payoffs beside it moved.
                //
                // Through `forage::crop_field_cost_multiplier` — the same expression
                // `patch_field_cost_multiplier` goes through — priced by the ladder's own
                // `build_cost`, exactly as the published `fieldWorkCost` is. So the figure this row
                // states for the crop a patch is committed to **is** that patch's `fieldWorkCost`,
                // rather than a second derivation that happens to agree.
                //
                // [`NO_SOW_WORK_COST`] where the plant cannot climb to a Field on this ground.
                sow_work_cost: crop_field_cost_multiplier(
                    &composition,
                    &share.species,
                    flora,
                    forage,
                )
                .and_then(|multiplier| ladder.rung(RungKey::PlantField).build_cost(multiplier))
                .unwrap_or(NO_SOW_WORK_COST),
            }
        })
        .collect();
    (composition, quotes)
}

/// **What a crop that cannot be sown here quotes as its Sow price** — `0`, the wire's "no figure",
/// which a client renders as *no row* rather than as a free Sow. Named because a bare `0.0` at the
/// call site reads as a measurement, and a measured Sow price can never be one: the multiplier is
/// clamped at `field_share_cost_floor` precisely because laying the rows and putting the seed in
/// costs work even on ground already wholly the crop.
const NO_SOW_WORK_COST: f32 = 0.0;

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

    fn configs() -> (Arc<FloraConfig>, Arc<LaborConfig>, Arc<LadderConfig>) {
        (
            FloraConfig::builtin(),
            LaborConfig::builtin(),
            LadderConfig::builtin(),
        )
    }

    /// The memo answers what the derivation answers — the property everything else here rests on.
    /// Asserted against `derive_tile_quotes` itself rather than against a recorded expectation, so a
    /// retune of the payoff tables moves both sides at once and the guard keeps its meaning.
    #[test]
    fn a_cached_tile_quotes_exactly_what_a_fresh_derivation_does() {
        let (flora, labor, ladder) = configs();
        let mut cache = FloraQuoteCache::default();
        let tiles: Vec<Tile> = (0..8)
            .map(|x| tile_at(UVec2::new(x, 3), TerrainType::MixedWoodland))
            .collect();

        let mut sweep = cache.sweep(&flora, &labor, &ladder, 99, UVec2::new(16, 16));
        for tile in &tiles {
            let cached = sweep.quotes(tile).to_vec();
            let (_, fresh) = derive_tile_quotes(&flora, &labor.forage, &ladder, tile, 99);
            assert_eq!(cached, fresh, "tile {:?}", tile.position);
        }

        // And again on the second turn, off the memo rather than the derivation.
        let mut sweep = cache.sweep(&flora, &labor, &ladder, 99, UVec2::new(16, 16));
        for tile in &tiles {
            let cached = sweep.quotes(tile).to_vec();
            let (_, fresh) = derive_tile_quotes(&flora, &labor.forage, &ladder, tile, 99);
            assert_eq!(
                cached, fresh,
                "tile {:?} on the second sweep",
                tile.position
            );
        }
    }

    /// **The published `role` is the ROSTER's role, for every plant on every tile** — the display
    /// tag has exactly one definition (`FloraDef::role`) and the quote site copies it rather than
    /// re-reading which component of the yield vector dominates. Asserted as a *relation* against the
    /// loaded roster, so a re-tagged species moves both sides at once.
    ///
    /// The second half is what stops the guard passing blind: across a sweep of biomes the baskets
    /// must name **more than one** role, or a capture site that hardcoded one word would satisfy the
    /// first half everywhere.
    #[test]
    fn every_quoted_plant_carries_its_rosters_own_role() {
        let (flora, labor, ladder) = configs();
        let grid = UVec2::new(64, 64);
        let mut cache = FloraQuoteCache::default();
        let mut sweep = cache.sweep(&flora, &labor, &ladder, 11, grid);

        let mut roles_seen: Vec<&str> = Vec::new();
        for (index, terrain) in [
            TerrainType::MixedWoodland,
            TerrainType::Floodplain,
            TerrainType::AlluvialPlain,
            TerrainType::PrairieSteppe,
        ]
        .into_iter()
        .enumerate()
        {
            for x in 0..16 {
                let tile = tile_at(UVec2::new(x, index as u32), terrain);
                for share in sweep.quotes(&tile) {
                    let def = &flora.species[&share.species];
                    assert_eq!(
                        share.role,
                        def.role.as_str(),
                        "{} on {terrain:?} published a role its roster row does not state",
                        share.species
                    );
                    if !roles_seen.contains(&def.role.as_str()) {
                        roles_seen.push(def.role.as_str());
                    }
                }
            }
        }

        assert!(
            roles_seen.len() > 1,
            "one role across every biome — the tag is not being read per species: {roles_seen:?}"
        );
    }

    /// Two tiles of one biome carry different baskets (per-tile realization is keyed on
    /// `(map_seed, tile)`), so a memo keyed on the biome alone would be wrong. This is what proves
    /// the key is the *tile*.
    #[test]
    fn the_memo_is_per_tile_not_per_biome() {
        let (flora, labor, ladder) = configs();
        let mut cache = FloraQuoteCache::default();
        let grid = UVec2::new(64, 64);
        let mut sweep = cache.sweep(&flora, &labor, &ladder, 7, grid);
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
        let (flora, labor, ladder) = configs();
        let mut cache = FloraQuoteCache::default();
        let grid = UVec2::new(16, 16);
        let position = UVec2::new(2, 2);

        let mut sweep = cache.sweep(&flora, &labor, &ladder, 5, grid);
        let forest = sweep
            .quotes(&tile_at(position, TerrainType::MixedWoodland))
            .to_vec();

        let rewritten = tile_at(position, TerrainType::HotDesertErg);
        let mut sweep = cache.sweep(&flora, &labor, &ladder, 5, grid);
        let desert = sweep.quotes(&rewritten).to_vec();

        assert_eq!(
            desert,
            derive_tile_quotes(&flora, &labor.forage, &ladder, &rewritten, 5).1
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
        let (flora, labor, ladder) = configs();
        let mut cache = FloraQuoteCache::default();
        let grid = UVec2::new(16, 16);
        let mut sweep = cache.sweep(&flora, &labor, &ladder, 5, grid);
        for x in 0..4 {
            sweep.quotes(&tile_at(UVec2::new(x, 0), TerrainType::MixedWoodland));
        }
        assert_eq!(cache.len(), 4);

        let reloaded = FloraConfig::builtin();
        cache.sweep(&reloaded, &labor, &ladder, 5, grid);
        assert_eq!(cache.len(), 0, "a swapped flora config left entries behind");

        let mut sweep = cache.sweep(&reloaded, &labor, &ladder, 5, grid);
        sweep.quotes(&tile_at(UVec2::new(0, 0), TerrainType::MixedWoodland));
        assert_eq!(cache.len(), 1);
        cache.sweep(&reloaded, &LaborConfig::builtin(), &ladder, 5, grid);
        assert_eq!(cache.len(), 0, "a swapped labor config left entries behind");

        // …and the ladder, which prices the per-crop Sow figure every quoted plant carries.
        let mut sweep = cache.sweep(&reloaded, &labor, &ladder, 5, grid);
        sweep.quotes(&tile_at(UVec2::new(0, 0), TerrainType::MixedWoodland));
        assert_eq!(cache.len(), 1);
        cache.sweep(&reloaded, &labor, &LadderConfig::builtin(), 5, grid);
        assert_eq!(
            cache.len(),
            0,
            "a swapped ladder config left entries behind"
        );
    }

    /// A re-seeded or re-sized map drops every entry — the seed is an input to per-tile realization,
    /// and the size bounds which coords can ever be read again.
    #[test]
    fn a_reseeded_or_resized_map_clears_the_whole_memo() {
        let (flora, labor, ladder) = configs();
        let mut cache = FloraQuoteCache::default();
        let grid = UVec2::new(16, 16);
        let seed_tile = tile_at(UVec2::new(1, 1), TerrainType::MixedWoodland);

        let mut sweep = cache.sweep(&flora, &labor, &ladder, 5, grid);
        sweep.quotes(&seed_tile);
        cache.sweep(&flora, &labor, &ladder, 6, grid);
        assert_eq!(cache.len(), 0, "a new map seed left entries behind");

        let mut sweep = cache.sweep(&flora, &labor, &ladder, 6, grid);
        sweep.quotes(&seed_tile);
        cache.sweep(&flora, &labor, &ladder, 6, UVec2::new(32, 32));
        assert_eq!(cache.len(), 0, "a resized map left entries behind");
    }

    /// **THE CAPTURE STAMPS THE PER-MATERIAL QUOTE, at each rung, from the sim's own seam** (arc
    /// #527) — the row-level half of `flora_f4_cash::the_picker_material_quote_is_the_material_the_
    /// sim_credits`, which pins the seam against what a real turn credits.
    ///
    /// What this adds is that the **capture** carries it: a quote computed correctly and then not
    /// written onto the row is exactly the shape the retired trade quote's absence has, and the
    /// client cannot tell the two apart. Asserted as a *relation* against `commit_material_payoff`
    /// rather than a recorded number, so a rate retune moves both sides at once.
    ///
    /// **The empty half is the liveness half.** A staple must publish *no row*, and a capture that
    /// wrote an empty vector unconditionally would satisfy that everywhere — so the cash crop beside
    /// it has to be non-empty on the same tile.
    #[test]
    fn every_quoted_plant_carries_its_own_per_rung_material_payoff() {
        let (flora, labor, ladder) = configs();
        let forage = &labor.forage;
        // **Swept over several tiles, not pinned to one.** Per-tile realization (§10) draws a
        // different subset per coordinate, so whether any one tile happens to carry both a
        // material-bearing plant and a bare staple is a property of the seed rather than of the code
        // under test. The relation below holds per plant on every tile; the two liveness flags
        // accumulate across the sweep.
        let capacity_of = |tile: &Tile| tile_forage_capacity(forage, tile);
        let tiles: Vec<Tile> = (0..12)
            .map(|x| tile_at(UVec2::new(x, 3), TerrainType::Floodplain))
            .collect();

        let mut saw_a_material = false;
        let mut saw_a_bare_staple = false;
        for tile in &tiles {
            let (composition, quotes) =
                derive_tile_quotes(&flora, forage, &ladder, tile, SWEEP_SEED);
            let capacity = capacity_of(tile);
            for quote in &quotes {
                for (rung, published) in [
                    (RungKey::PlantField, &quote.sow_material_payoff),
                    (RungKey::PlantTended, &quote.cultivate_material_payoff),
                ] {
                    let expected = commit_material_payoff(
                        tile.position,
                        capacity,
                        &quote.species,
                        &composition,
                        &flora,
                        forage,
                        FORECAST_OUTPUT_MULTIPLIER,
                        rung,
                    );
                    assert_eq!(
                        published.len(),
                        expected.len(),
                        "{} @ {rung:?}: the row must carry the seam's own rows",
                        quote.species
                    );
                    for (row, want) in published.iter().zip(expected.iter()) {
                        assert_eq!(row.material_id, want.material);
                        assert_eq!(row.amount, want.amount);
                        assert!(
                            row.amount > 0.0,
                            "{} @ {rung:?}: a published row is a row that pays",
                            quote.species
                        );
                    }
                    saw_a_material |= !published.is_empty();
                }
                saw_a_bare_staple |= quote.sow_material_payoff.is_empty();
            }
        }
        assert!(
            saw_a_material,
            "the sweep must name at least one material-bearing plant, or every comparison above was \
             between two empty vectors"
        );
        assert!(
            saw_a_bare_staple,
            "…and at least one plant whose Field pays no material, or 'empty means no row' is \
             untested here"
        );
    }

    /// The seed the material-quote fixture sweeps under — any fixed value; realization is a pure
    /// function of it, and the assertions above are relations rather than recorded numbers.
    const SWEEP_SEED: u64 = 99;

    /// A coord the sweep never visited names no plants. The readout relies on this for a patch whose
    /// tile is absent from the map — an empty basket, never a fabricated one. The empty basket is
    /// **shared** too, so the miss path allocates nothing per lookup.
    #[test]
    fn an_unvisited_coord_names_no_plants() {
        let cache = FloraQuoteCache::default();
        let absent = UVec2::new(4, 4);
        assert!(cache.composition(absent).is_empty());
        assert!(
            Arc::ptr_eq(&cache.composition(absent), &cache.composition(absent)),
            "the empty basket allocated a fresh Arc per lookup"
        );
    }

    /// **Reading a tile's basket shares it — it does not copy it.** This is the whole point of the
    /// `Arc<[_]>`: the readout builds one row per patch per turn, so a copy here re-allocated two
    /// `String`s per named plant on every patch of the map, every turn, for a value that belongs to
    /// the tile and never changes while the ground does not.
    ///
    /// Asserted on the pointer rather than on a timing, because sharing is the property — a `to_vec`
    /// that reappeared would still compare `==` and would still pass every other guard in this file.
    #[test]
    fn reading_a_tiles_basket_shares_it_rather_than_copying_it() {
        let (flora, labor, ladder) = configs();
        let mut cache = FloraQuoteCache::default();
        let tile = tile_at(UVec2::new(2, 3), TerrainType::MixedWoodland);

        let mut sweep = cache.sweep(&flora, &labor, &ladder, 5, UVec2::new(16, 16));
        sweep.quotes(&tile);

        let first = cache.composition(tile.position);
        assert!(!first.is_empty(), "the fixture tile named no plants");
        assert!(
            Arc::ptr_eq(&first, &cache.composition(tile.position)),
            "two reads of one tile's basket handed back two allocations"
        );
    }
}
