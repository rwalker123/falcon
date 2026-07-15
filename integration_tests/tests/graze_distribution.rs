//! **Look at the pasture layer on a real map** (Grazing Phase 2a — `docs/plan_grazing_foundation.md`
//! §7.1). The whole point of shipping the graze layer *inert* is to check its distribution before the
//! fauna model is bet on it: herd carrying capacity, competition, overgrazing, migration and spawn
//! placement all become functions of this layer in Phase 2b/2c. If prairie isn't pasture and forest
//! isn't poor, we need to know **now**, not after every herd in the game has resized.
//!
//! `graze_distribution_report` prints the measurement (run it with `--nocapture`); the assertions
//! around it are the guards that keep the model claims true as biomes and levers are retuned.
//!
//! **The two food webs, side by side** (`two_food_web_report`): the graze (animal) and forage (human)
//! per-biome capacity tables are *both* spatial now, and the design thesis — *your best farm is not
//! your best pasture* — is a **measurable** claim about their relationship, not a slogan. The report
//! prints the joint histogram and the guards assert the divergence: a tile-weighted correlation that
//! is not strongly positive, and a "top-quartile at both" overlap that stays near what independence
//! would give. If either fails, the split is decorative and the retune failed.

mod common;

use std::collections::BTreeMap;

use bevy::math::UVec2;
use core_sim::{
    build_headless_app, classify_food_module_from_traits, grid_utils::hex_distance_wrapped,
    FaunaConfig, ForageRegistry, GrazeRegistry, LaborConfig, SimulationConfig,
    SimulationConfigMetadata, SnapshotHistory, StartLocation,
};
use sim_runtime::{TerrainTags, TerrainType, WorldSnapshot};

/// The standard campaign map. Big enough for a real biome mix, small enough to run in a test.
const MAP_WIDTH: u32 = 80;
const MAP_HEIGHT: u32 = 52;

/// Seeds sampled so the measurement is a *distribution*, not one lucky map.
const SEEDS: [u64; 3] = [11, 4242, 90210];

/// A land tile with **no pasture at all** is a real thing (glacier, salt flat, lava), but it must stay
/// a minority: graze is the substrate herds live on, and a map that is mostly dead ground has no
/// fauna model on it. Well clear of the measured ~1-3%.
const MAX_ZERO_GRAZE_LAND_FRACTION: f64 = 0.15;

/// The **retired** flat forage capacity every food-module tile used to carry (`labor_config.json`
/// `forage.carrying_capacity`, now replaced by the per-biome `capacity_by_biome`). Kept here purely
/// so the report can price the retune against the economy PR #119 measured: `120 × patches` was the
/// whole human food budget of a map.
const RETIRED_FLAT_FORAGE_CAPACITY: f64 = 120.0;

/// The two webs must not be the same map wearing two hats. A tile-weighted Pearson correlation at or
/// above this would mean rich-for-animals still implies rich-for-humans — the pre-retune state, where
/// forage was a *constant* and the correlation was undefined-but-morally-1. Near zero is the target;
/// this is the ceiling the guard enforces, over **living land** (below).
const MAX_WEB_CORRELATION: f64 = 0.20;

/// **Living land**: a tile is a farm-vs-pasture *decision* only if it can feed somebody. A tile below
/// this capacity on **both** webs (bare rock, ice, lava, erg, badlands) is dead ground, and dead
/// ground is dead for humans *and* animals — a shared **zero**, not shared richness.
///
/// That shared zero is a real, irreducible positive term in a correlation taken over *all* land, and
/// it says nothing about the design claim: nobody chooses between farming and grazing a glacier. So
/// the report prints **both** correlations — over all land (context) and over living land (**the
/// claim**) — and the guard binds the one that means something. The threshold sits above the
/// marginal biomes (`RockyReg` 10/6, `CanyonBadlands` 12/8, `HotDesertErg` 8/5) and below the thinnest
/// biome anyone can actually use (`SeasonalSnowfield` 25 graze).
const LIVING_LAND_CAPACITY: f64 = 15.0;

/// The two "best land" cuts the report measures the webs' overlap at.
const TOP_QUARTILE: f64 = 0.75;
const TOP_DECILE: f64 = 0.90;

/// Fraction of land that may be top-**decile** in **both** webs — *your best farm is not your best
/// pasture*, stated as a number. Under independence the overlap of two deciles is 0.10 × 0.10 =
/// **1%**; a perfectly-aligned pair would give 10%. This ceiling sits between them.
///
/// **Why the decile and not the quartile** (which the report still prints): `AlluvialPlain` is ~25% of
/// all land on an earthlike map, so the 75th-percentile *graze* cut lands **inside that single biome**
/// — and whether it reads "0% overlap" or "24% overlap" then turns on whether alluvial's graze number
/// sits a hair above or below `Tundra`'s, not on whether the two webs disagree. That is a cliff, not a
/// measurement, and tuning a capacity table to land on the right side of it would be tuning to the
/// metric. The decile cut asks the question the design actually asks — is the *best* farmland also the
/// *best* pasture — and no mega-biome straddles it.
const MAX_BOTH_TOP_DECILE_LAND_FRACTION: f64 = 0.03;

/// One generated world, reduced to what the two-web measurement needs.
struct GeneratedWorld {
    snapshot: WorldSnapshot,
    graze: GrazeRegistry,
    forage: ForageRegistry,
    /// Where the campaign's first band actually stands — the tiles whose food matters on turn 1.
    start: Option<UVec2>,
    wrap_horizontal: bool,
}

fn generate(seed: u64) -> GeneratedWorld {
    common::ensure_test_config();
    let mut app = build_headless_app();
    if let Some(mut md) = app.world.get_resource_mut::<SimulationConfigMetadata>() {
        md.set_seed_random(false);
    }
    if let Some(mut cfg) = app.world.get_resource_mut::<SimulationConfig>() {
        cfg.map_preset_id = "earthlike".to_string();
        cfg.grid_size = UVec2::new(MAP_WIDTH, MAP_HEIGHT);
        cfg.map_seed = seed;
    }
    app.update();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .as_ref()
        .map(|s| (**s).clone())
        .expect("snapshot after worldgen");
    GeneratedWorld {
        graze: app.world.resource::<GrazeRegistry>().clone(),
        forage: app.world.resource::<ForageRegistry>().clone(),
        start: app.world.resource::<StartLocation>().position(),
        wrap_horizontal: app
            .world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal,
        snapshot,
    }
}

/// Per-biome roll-up of the land the map actually generated.
#[derive(Default, Clone, Copy)]
struct BiomeRow {
    tiles: usize,
    total_capacity: f64,
}

fn land_biome_rows(snapshot: &WorldSnapshot) -> (BTreeMap<String, BiomeRow>, usize) {
    let mut rows: BTreeMap<String, BiomeRow> = BTreeMap::new();
    let mut land_tiles = 0usize;
    for tile in &snapshot.tiles {
        if tile.terrain_tags.contains(TerrainTags::WATER) {
            continue;
        }
        land_tiles += 1;
        let row = rows.entry(format!("{:?}", tile.terrain)).or_default();
        row.tiles += 1;
        row.total_capacity += f64::from(tile.graze_capacity);
    }
    (rows, land_tiles)
}

/// THE DELIVERABLE: total capacity, the zero-graze fraction of land, and the per-biome histogram.
/// Prints; asserts only the model claims (below), so retuning the table changes the numbers, not the
/// verdict.
#[test]
fn graze_distribution_report() {
    for seed in SEEDS {
        let world = generate(seed);
        let (snapshot, registry) = (world.snapshot, world.graze);
        let (rows, land_tiles) = land_biome_rows(&snapshot);

        let total_capacity: f64 = rows.values().map(|row| row.total_capacity).sum();
        let zero_graze_land: usize = rows
            .values()
            .filter(|row| row.total_capacity <= 0.0)
            .map(|row| row.tiles)
            .sum();
        let zero_fraction = zero_graze_land as f64 / land_tiles.max(1) as f64;

        println!("\n=== graze distribution — earthlike {MAP_WIDTH}x{MAP_HEIGHT} seed {seed} ===");
        println!(
            "land tiles          : {land_tiles}\n\
             graze patches       : {}  (one per land tile with positive capacity)\n\
             total capacity      : {total_capacity:.0}\n\
             zero-graze land     : {zero_graze_land} tiles ({:.1}% of land)",
            registry.len(),
            zero_fraction * 100.0
        );

        let mut ranked: Vec<(&String, &BiomeRow)> = rows.iter().collect();
        ranked.sort_by(|a, b| {
            b.1.total_capacity
                .partial_cmp(&a.1.total_capacity)
                .expect("finite capacities")
        });
        println!(
            "\n  {:<22} {:>6} {:>12} {:>8} {:>7}",
            "biome", "tiles", "capacity", "per-tile", "share"
        );
        for (biome, row) in ranked.iter() {
            println!(
                "  {:<22} {:>6} {:>12.0} {:>8.0} {:>6.1}%",
                biome,
                row.tiles,
                row.total_capacity,
                row.total_capacity / row.tiles.max(1) as f64,
                row.total_capacity / total_capacity.max(1.0) * 100.0
            );
        }

        // --- The model claims, guarded ---

        // **Phase 2b: herds now EAT.** Patches seed full, but the first turn's graze draw-down
        // (`advance_herd_grazing`) consumes a little where herds stand — so live biomass sits *just
        // below* capacity, never above (grazing only removes), and only a small fraction is gone after
        // one turn (herds occupy a sliver of the map, and the escapement floor bounds how deep any one
        // tile is drawn). This is the twin of the 2a inertness guard, inverted: it now asserts the
        // layer is *live* but its draw-down is modest, not that it is untouched. The distribution this
        // report measures is **capacity** (the biome table), which grazing never moves.
        let live_biomass: f64 = registry
            .patches
            .values()
            .map(|patch| f64::from(patch.biomass))
            .sum();
        assert!(
            live_biomass <= total_capacity + 1.0,
            "grazing only draws graze DOWN, never above capacity ({live_biomass:.0} vs \
             {total_capacity:.0})"
        );
        assert!(
            live_biomass > total_capacity * 0.9,
            "Phase 2b grazing is active but modest after one turn — most of the map is ungrazed \
             ({live_biomass:.0} vs {total_capacity:.0})"
        );

        // Dead ground exists, but the map is not made of it.
        assert!(
            zero_fraction < MAX_ZERO_GRAZE_LAND_FRACTION,
            "seed {seed}: {:.1}% of land carries no graze — the fauna model has no substrate",
            zero_fraction * 100.0
        );

        // Water is never pasture, and it is never a *patch* either (an absent reading, not a zero one).
        for tile in &snapshot.tiles {
            if tile.terrain_tags.contains(TerrainTags::WATER) {
                assert_eq!(tile.graze_capacity, 0.0, "water is not pasture");
                assert!(registry.patch(UVec2::new(tile.x, tile.y)).is_none());
            }
        }
    }
}

/// A tile's reading on **both** food webs, plus what the map actually rolled.
#[derive(Default, Clone, Copy)]
struct WebRow {
    tiles: usize,
    /// Land tiles only (the graze layer skips water outright).
    land_tiles: usize,
    graze_per_tile: f64,
    forage_per_tile: f64,
    /// Food-module tiles of this biome that were seeded a live `ForagePatch`.
    forage_patches: usize,
    /// Food-module tiles of this biome, whether or not they carry a patch — i.e. what the retired
    /// flat `carrying_capacity` used to pay 120 on.
    food_module_tiles: usize,
}

/// Pearson correlation between the two webs over a per-tile sample of `(graze, forage)` pairs — one
/// entry per land tile, so it is tile-count-weighted by construction. `None` if either web is
/// constant across the sample (the *old* world: a flat forage table made this literally undefined —
/// and that was the bug).
fn correlation(pairs: &[(f64, f64)]) -> Option<f64> {
    let n = pairs.len() as f64;
    if n <= 0.0 {
        return None;
    }
    let mean_g = pairs.iter().map(|(g, _)| g).sum::<f64>() / n;
    let mean_f = pairs.iter().map(|(_, f)| f).sum::<f64>() / n;
    let mut cov = 0.0;
    let (mut var_g, mut var_f) = (0.0, 0.0);
    for (graze, forage) in pairs {
        let (dg, df) = (graze - mean_g, forage - mean_f);
        cov += dg * df;
        var_g += dg * dg;
        var_f += df * df;
    }
    if var_g <= 0.0 || var_f <= 0.0 {
        return None;
    }
    Some(cov / (var_g.sqrt() * var_f.sqrt()))
}

/// The value at the `percentile` cut of a per-tile distribution (ties resolved by `>=`, so a biome
/// sitting exactly *on* the cut counts as "top" — the conservative direction: it can only inflate an
/// overlap, never hide one).
fn percentile(samples: &[f64], percentile: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite capacities"));
    let index = (sorted.len() as f64 * percentile).floor() as usize;
    sorted[index.min(sorted.len().saturating_sub(1))]
}

/// Fraction of land that is in the top `1 - cut` of **both** webs.
fn both_top_fraction(pairs: &[(f64, f64)], cut: f64) -> (usize, f64, f64, f64) {
    let graze_cut = percentile(&pairs.iter().map(|(g, _)| *g).collect::<Vec<f64>>(), cut);
    let forage_cut = percentile(&pairs.iter().map(|(_, f)| *f).collect::<Vec<f64>>(), cut);
    let both = pairs
        .iter()
        .filter(|(g, f)| *g >= graze_cut && *f >= forage_cut)
        .count();
    (
        both,
        both as f64 / pairs.len().max(1) as f64,
        graze_cut,
        forage_cut,
    )
}

/// **THE DELIVERABLE (two-web edition).** Per biome: what the map rolled, its graze capacity, its
/// forage capacity — side by side, so the two tables can be *seen* to disagree — plus the correlation
/// between them, the "top-quartile at both" overlap, and the total forage capacity before vs after the
/// retune (the balance impact on the human food economy, which PR #119 measured under the old flat
/// 120). Prints; asserts only the model claims, so retuning changes the numbers, not the verdict.
#[test]
fn two_food_web_report() {
    let graze_table = &FaunaConfig::builtin().graze;
    let forage_table = &LaborConfig::builtin().forage;

    for seed in SEEDS {
        let world = generate(seed);
        let (snapshot, graze_registry, forage_registry) =
            (world.snapshot, world.graze, world.forage);
        let (start_location, wrap_horizontal) = (world.start, world.wrap_horizontal);
        let work_range = LaborConfig::builtin().band_work_range;

        let mut rows: BTreeMap<String, WebRow> = BTreeMap::new();
        let mut land_tiles = 0usize;
        let mut land_pairs: Vec<(f64, f64)> = Vec::new();
        for tile in &snapshot.tiles {
            let is_land = !tile.terrain_tags.contains(TerrainTags::WATER);
            let graze = f64::from(graze_table.capacity_for(tile.terrain));
            let forage = f64::from(forage_table.capacity_for(tile.terrain));
            let row = rows.entry(format!("{:?}", tile.terrain)).or_default();
            row.tiles += 1;
            row.graze_per_tile = graze;
            row.forage_per_tile = forage;
            if is_land {
                land_tiles += 1;
                row.land_tiles += 1;
                land_pairs.push((graze, forage));
            }
            // A food module is what makes a tile *gatherable at all* — the classifier reads terrain +
            // tags, and it tags water (shelf/inland sea/coral = fisheries) as readily as land.
            if classify_food_module_from_traits(tile.terrain, tile.terrain_tags).is_some() {
                row.food_module_tiles += 1;
                if forage > 0.0 {
                    row.forage_patches += 1;
                }
            }
        }

        let total_graze: f64 = rows
            .values()
            .map(|r| r.graze_per_tile * r.land_tiles as f64)
            .sum();
        let total_forage: f64 = forage_registry
            .patches
            .values()
            .map(|p| f64::from(p.carrying_capacity))
            .sum();
        let food_module_tiles: usize = rows.values().map(|r| r.food_module_tiles).sum();
        let total_forage_before = RETIRED_FLAT_FORAGE_CAPACITY * food_module_tiles as f64;

        println!("\n=== two food webs — earthlike {MAP_WIDTH}x{MAP_HEIGHT} seed {seed} ===");
        println!(
            "land tiles          : {land_tiles}\n\
             graze patches       : {}   total graze capacity : {total_graze:.0}\n\
             forage patches      : {}   total forage capacity: {total_forage:.0}",
            graze_registry.len(),
            forage_registry.len(),
        );

        // (1) The joint histogram, sorted by tile count — the map as it actually rolled.
        let mut ranked: Vec<(&String, &WebRow)> = rows.iter().collect();
        ranked.sort_by_key(|(_, row)| std::cmp::Reverse(row.tiles));
        println!(
            "\n  {:<22} {:>6} {:>6} {:>8} {:>8} {:>9} {:>8}",
            "biome", "tiles", "land", "graze/t", "forage/t", "patches", "verdict"
        );
        for (biome, row) in ranked.iter() {
            let verdict = match (row.graze_per_tile, row.forage_per_tile) {
                (g, f) if g <= 0.0 && f <= 0.0 => "barren",
                // Dead-ish ground: the ratio between two near-zeros carries no information.
                (g, f) if g < LIVING_LAND_CAPACITY && f < LIVING_LAND_CAPACITY => "marginal",
                (g, f) if f > g * 1.5 => "FARM",
                (g, f) if g > f * 1.5 => "PASTURE",
                _ => "both",
            };
            println!(
                "  {:<22} {:>6} {:>6} {:>8.0} {:>8.0} {:>9} {:>8}",
                biome,
                row.tiles,
                row.land_tiles,
                row.graze_per_tile,
                row.forage_per_tile,
                row.forage_patches,
                verdict
            );
        }

        // (2) Do the two webs actually disagree? A strongly positive correlation means they do not,
        // and the split is decorative. Measured twice: over all land (where bare rock's shared zero
        // is an irreducible positive term) and over LIVING land (the actual design claim).
        let all_land_r = correlation(&land_pairs).expect("both webs vary across the land");
        let living: Vec<(f64, f64)> = land_pairs
            .iter()
            .copied()
            .filter(|(g, f)| *g >= LIVING_LAND_CAPACITY || *f >= LIVING_LAND_CAPACITY)
            .collect();
        let living_r = correlation(&living).expect("both webs vary across living land");

        // (3) "Best farm ≠ best pasture", measured — at two cuts, because the quartile cut is a
        // **cliff on this map**: `AlluvialPlain` is ~25% of all land, so the 75th-percentile graze
        // threshold lands *inside it*, and the whole biome flips in or out of "top-quartile pasture"
        // on a few tiles of ranking. The DECILE cut is the sentence the design actually makes — your
        // *best* farm is not your *best* pasture — and it is not hostage to one mega-biome straddling
        // a threshold. Both are printed; the decile is guarded.
        let (both_quartile, quartile_fraction, graze_q, forage_q) =
            both_top_fraction(&land_pairs, TOP_QUARTILE);
        let (both_decile, decile_fraction, graze_d, forage_d) =
            both_top_fraction(&land_pairs, TOP_DECILE);

        // (4) The balance impact on the human food economy, priced explicitly — map-wide AND where it
        // is actually felt: the patches a starting band can reach. (Start placement targets food
        // weight, i.e. fertile lowland — exactly the biomes the table made RICHER — so the map-wide
        // total and the early-game economy move in *opposite* directions. Only the second one is a
        // balance risk to PR #119's measured campaign.)
        let start = start_location.expect("worldgen places a start location");
        let reachable: Vec<f64> = forage_registry
            .patches
            .values()
            .filter(|patch| {
                hex_distance_wrapped(start, patch.tile, MAP_WIDTH, wrap_horizontal) <= work_range
            })
            .map(|patch| f64::from(patch.carrying_capacity))
            .collect();
        let reachable_mean = reachable.iter().sum::<f64>() / reachable.len().max(1) as f64;

        println!(
            "\n  graze/forage correlation, all land       : {all_land_r:+.3}   \
             (bare rock is a shared ZERO — see LIVING_LAND_CAPACITY)\n  \
             graze/forage correlation, LIVING land    : {living_r:+.3}   \
             (THE CLAIM — target: near zero or negative)\n  \
             top-QUARTILE cuts                        : graze >= {graze_q:.0}, forage >= {forage_q:.0}\n  \
             land top-quartile in BOTH webs           : {both_quartile} tiles ({:.1}% of land; \
             independence 6.2% — a CLIFF, see the code)\n  \
             top-DECILE cuts                          : graze >= {graze_d:.0}, forage >= {forage_d:.0}\n  \
             land top-decile in BOTH webs             : {both_decile} tiles ({:.1}% of land; \
             independence 1.0%)\n\n  \
             forage capacity  before (flat 120 x {food_module_tiles} module tiles) : {total_forage_before:.0}\n  \
             forage capacity  after  (per-biome table, {} patches)   : {total_forage:.0}\n  \
             map-wide human food capacity             : {:+.1}%\n  \
             patches within band work range of start  : {} (mean cap {reachable_mean:.0} vs the old flat 120 \
             = {:+.1}%)",
            quartile_fraction * 100.0,
            decile_fraction * 100.0,
            forage_registry.len(),
            (total_forage / total_forage_before.max(1.0) - 1.0) * 100.0,
            reachable.len(),
            (reachable_mean / RETIRED_FLAT_FORAGE_CAPACITY - 1.0) * 100.0,
        );

        // --- The model claims, guarded ---

        assert!(
            living_r < MAX_WEB_CORRELATION,
            "seed {seed}: graze and forage correlate at {living_r:+.3} across living land — the two \
             food webs are the same map wearing two hats, and 'your best farm is not your best \
             pasture' is false"
        );
        assert!(
            decile_fraction < MAX_BOTH_TOP_DECILE_LAND_FRACTION,
            "seed {seed}: {:.1}% of land is top-decile in BOTH webs — the best farmland and the best \
             pasture are the same tiles",
            decile_fraction * 100.0
        );
        // Every live patch is seeded FULL, and the layer is measured at worldgen: biomass == capacity.
        let live_forage: f64 = forage_registry
            .patches
            .values()
            .map(|p| f64::from(p.biomass))
            .sum();
        assert!(
            (live_forage - total_forage).abs() < 1.0,
            "a freshly generated map's patches sit at capacity"
        );
        // "No food here" is an ABSENT reading, never a zero one — a zero-capacity biome (glacier,
        // salt pan, vent field) carries no patch even though the module classifier tags it.
        for patch in forage_registry.patches.values() {
            assert!(
                patch.carrying_capacity > 0.0,
                "a seeded forage patch must carry a positive capacity"
            );
        }
    }
}

/// **Prairie is pasture; forest is poor.** The inversion the whole two-stock split exists to create —
/// a closed canopy shades out the ground cover, so your best farm is usually not your best pasture.
/// Asserted on the *per-tile* capacity (the land's property), not on totals, which merely reflect how
/// much of each biome the map happened to roll.
#[test]
fn prairie_out_pastures_forest_per_tile() {
    let graze = &FaunaConfig::builtin().graze;
    let prairie = graze.capacity_for(TerrainType::PrairieSteppe);
    for poor in [
        TerrainType::MixedWoodland,
        TerrainType::BorealTaiga,
        TerrainType::Tundra,
        TerrainType::HotDesertErg,
        TerrainType::AlpineMountain,
    ] {
        assert!(
            prairie > graze.capacity_for(poor),
            "prairie ({prairie}) must out-pasture {poor:?} ({})",
            graze.capacity_for(poor)
        );
    }
}
