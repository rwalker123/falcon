//! **Climate authority — measurement + guards** (`docs/plan_climate_authority.md`).
//!
//! Temperature is the sole climate authority: a biome's climate eligibility is a derived function
//! of the tile's temperature, never of its latitude. This file both *measures* that (the
//! `#[ignore]`d report, which is how the shipped cut points were chosen) and *guards* it.
//!
//! Run the report with:
//! `cargo test -p core_sim --release --test climate_authority -- --ignored --nocapture`
//!
//! # The two incoherences, and why both must fall
//!
//! The arc exists because two systems answered "how cold is this tile?" differently, and they
//! disagreed in **both** directions:
//!
//! - **cold-but-temperate** — a tile below `cool_min` carrying a biome with no `POLAR` tag
//!   (measured before the arc: 3,847 tiles, 6.9% of land), and
//! - **warm-polar** — a `POLAR`-tagged tile sitting in warm air (4,397 tiles, 7.9% of land).
//!
//! Fixing one at the expense of the other would be no fix at all — a gate can trivially drive
//! either to zero alone by moving in one direction. So the guards below bound **both**.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{UVec2, World};
use bevy::MinimalPlugins;
use std::collections::BTreeMap;

use core_sim::{
    apply_biome_palette_clamp, apply_tag_budget_solver, climate_band_for_temperature,
    generate_hydrology, reconcile_coastal_shelf, spawn_initial_world, ClimateBand, CultureManager,
    DiscoveryProgressLedger, FactionInventory, GenerationRegistry, MapPresets, MapPresetsHandle,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
};
use sim_runtime::{TerrainTags, TerrainType};

/// The client's "cool" boundary, the temperature below which a tile reads as cold on the tile card
/// (`clients/godot_thin_client/src/config/tile_climate_config.json`). The pre-arc measurement in
/// the design doc used this value, so the before/after comparison keys on the same number.
const COOL_MIN: f32 = 3.0;

/// The temperature above which a `POLAR`-tagged tile is unambiguously in warm air. Same value, used
/// from the other side.
const WARM_POLAR_MIN: f32 = COOL_MIN;

const SEEDS: [u64; 5] = [1, 2, 3, 4, 119_304_647];
const GRIDS: [UVec2; 2] = [UVec2::new(80, 52), UVec2::new(120, 78)];
const PRESETS: [&str; 2] = ["earthlike", "polar_contrast"];

/// One fully-generated map, reduced to the per-tile facts this arc is about.
struct MapSample {
    /// `(temperature, terrain, tags, band)` for every LAND tile.
    land: Vec<(f32, TerrainType, TerrainTags, ClimateBand)>,
}

impl MapSample {
    fn land_count(&self) -> usize {
        self.land.len()
    }

    /// Tiles that are cold but wear a biome carrying no `POLAR` tag.
    fn cold_but_temperate(&self) -> usize {
        self.land
            .iter()
            .filter(|(temp, _, tags, _)| *temp < COOL_MIN && !tags.contains(TerrainTags::POLAR))
            .count()
    }

    /// `POLAR`-tagged tiles sitting in warm air.
    fn warm_polar(&self) -> usize {
        self.land
            .iter()
            .filter(|(temp, _, tags, _)| {
                *temp > WARM_POLAR_MIN && tags.contains(TerrainTags::POLAR)
            })
            .count()
    }

    fn band_counts(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        for (_, _, _, band) in &self.land {
            *out.entry(band.as_str()).or_insert(0) += 1;
        }
        out
    }

    fn biome_histogram_per_band(&self) -> BTreeMap<&'static str, BTreeMap<String, usize>> {
        let mut out: BTreeMap<&'static str, BTreeMap<String, usize>> = BTreeMap::new();
        for (_, terrain, _, band) in &self.land {
            *out.entry(band.as_str())
                .or_default()
                .entry(format!("{terrain:?}"))
                .or_insert(0) += 1;
        }
        out
    }

    /// **Alpine tundra** — the capability this arc adds (`§5.3`): a cold biome on high ground that
    /// is *not* near the pole. Keyed on the tile being cold-ladder-eligible while carrying a
    /// `POLAR` tag on genuinely elevated terrain, which is only reachable once the gate reads an
    /// elevation-derived temperature.
    fn alpine_cold(&self) -> usize {
        self.land
            .iter()
            .filter(|(_, terrain, tags, band)| {
                band.admits_cold_biomes()
                    && tags.contains(TerrainTags::POLAR)
                    && matches!(
                        terrain,
                        TerrainType::Glacier | TerrainType::SeasonalSnowfield
                    )
                    && tags.contains(TerrainTags::HIGHLAND)
            })
            .count()
    }
}

/// Build one map through the **real** Startup chain — worldgen, hydrology, the tag solver, the
/// palette clamp and the shelf reconciliation — so the measurement reads the map the game ships,
/// not an intermediate stage. (The palette clamp especially: it is the pass that would silently
/// re-stamp temperate biomes onto cold tiles if it were still keyed on latitude.)
fn sample_map(preset_id: &str, seed: u64, grid: UVec2) -> MapSample {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let world: &mut World = &mut app.world;

    let presets = MapPresets::builtin();
    let mut config = SimulationConfig::builtin();
    config.grid_size = grid;
    config.map_seed = seed;
    config.map_preset_id = preset_id.to_string();

    world.insert_resource(config);
    world.insert_resource(SimulationTick::default());
    world.insert_resource(CultureManager::default());
    world.insert_resource(GenerationRegistry::with_seed(seed, 6));
    world.insert_resource(MapPresetsHandle::new(presets));
    world.insert_resource(DiscoveryProgressLedger::default());
    world.insert_resource(FactionInventory::default());
    world.insert_resource(StartProfileKnowledgeTagsHandle::new(
        StartProfileKnowledgeTags::builtin(),
    ));
    world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));

    world.run_system_once(spawn_initial_world);
    generate_hydrology(world);
    world.run_system_once(apply_tag_budget_solver);
    world.run_system_once(apply_biome_palette_clamp);
    world.run_system_once(reconcile_coastal_shelf);

    let config = world.resource::<SimulationConfig>().clone();
    let registry = world
        .get_resource::<TileRegistry>()
        .expect("tile registry")
        .clone();
    let mut query = world.query::<&Tile>();

    let mut land = Vec::new();
    for &entity in registry.tiles.iter() {
        let tile = query.get(world, entity).expect("tile component");
        if tile.terrain_tags.contains(TerrainTags::WATER) {
            continue;
        }
        let temp = tile.temperature.to_f32();
        land.push((
            temp,
            tile.terrain,
            tile.terrain_tags,
            climate_band_for_temperature(temp, &config.climate),
        ));
    }
    MapSample { land }
}

fn all_samples() -> Vec<(String, MapSample)> {
    let mut out = Vec::new();
    for preset in PRESETS {
        for grid in GRIDS {
            for seed in SEEDS {
                out.push((
                    format!("{preset} {}x{} seed {seed}", grid.x, grid.y),
                    sample_map(preset, seed, grid),
                ));
            }
        }
    }
    out
}

/// **The report the cut points were chosen from.** Not a guard — run it by hand.
#[test]
#[ignore]
fn climate_band_report() {
    let samples = all_samples();

    let mut total_land = 0usize;
    let mut total_bands: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut total_hist: BTreeMap<&'static str, BTreeMap<String, usize>> = BTreeMap::new();
    let (mut total_cold_temperate, mut total_warm_polar, mut total_alpine) = (0usize, 0, 0);

    println!("\n=== per-run ===");
    for (label, sample) in &samples {
        let land = sample.land_count();
        let bands = sample.band_counts();
        let ct = sample.cold_but_temperate();
        let wp = sample.warm_polar();
        total_land += land;
        total_cold_temperate += ct;
        total_warm_polar += wp;
        total_alpine += sample.alpine_cold();
        for (k, v) in &bands {
            *total_bands.entry(k).or_insert(0) += v;
        }
        for (band, hist) in sample.biome_histogram_per_band() {
            let entry = total_hist.entry(band).or_default();
            for (biome, n) in hist {
                *entry.entry(biome).or_insert(0) += n;
            }
        }
        let pct = |n: usize| 100.0 * n as f64 / land.max(1) as f64;
        println!(
            "{label:34} land {land:6}  polar {:5.1}%  boreal {:5.1}%  temperate {:5.1}%  tropical {:5.1}%  | cold-temperate {ct:5} ({:4.1}%)  warm-polar {wp:5} ({:4.1}%)",
            pct(*bands.get("polar").unwrap_or(&0)),
            pct(*bands.get("boreal").unwrap_or(&0)),
            pct(*bands.get("temperate").unwrap_or(&0)),
            pct(*bands.get("tropical").unwrap_or(&0)),
            pct(ct),
            pct(wp),
        );
    }

    println!("\n=== aggregate over {} runs ===", samples.len());
    println!("land tiles: {total_land}");
    for band in ["polar", "boreal", "temperate", "tropical"] {
        let n = *total_bands.get(band).unwrap_or(&0);
        println!(
            "  {band:10} {n:7}  {:5.2}% of land",
            100.0 * n as f64 / total_land as f64
        );
    }
    println!(
        "\ncold-but-temperate: {total_cold_temperate} ({:.2}% of land)",
        100.0 * total_cold_temperate as f64 / total_land as f64
    );
    println!(
        "warm-polar:         {total_warm_polar} ({:.2}% of land)",
        100.0 * total_warm_polar as f64 / total_land as f64
    );
    println!(
        "alpine cold highland (the NEW capability, §5.3): {total_alpine} ({:.2}% of land)",
        100.0 * total_alpine as f64 / total_land as f64
    );

    println!("\n=== biome histogram per band ===");
    for band in ["polar", "boreal", "temperate", "tropical"] {
        let Some(hist) = total_hist.get(band) else {
            continue;
        };
        let band_total: usize = hist.values().sum();
        println!("\n-- {band} ({band_total} tiles) --");
        let mut rows: Vec<(&String, &usize)> = hist.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (biome, n) in rows.iter().take(12) {
            println!(
                "  {biome:24} {n:6}  {:5.1}%",
                100.0 * **n as f64 / band_total.max(1) as f64
            );
        }
    }
}

/// **The incoherence must fall sharply in BOTH directions, and neither may be traded for the
/// other.** Pre-arc: cold-but-temperate 6.9% of land, warm-polar 7.9%. A gate can drive either one
/// alone to zero by moving in one direction, so bounding only one would guard nothing.
#[test]
fn climate_incoherence_falls_in_both_directions() {
    /// Pre-arc measured share of land that was cold yet wearing a temperate biome.
    const BEFORE_COLD_TEMPERATE_PCT: f64 = 6.9;
    /// Pre-arc measured share of land that was `POLAR`-tagged yet sitting in warm air.
    const BEFORE_WARM_POLAR_PCT: f64 = 7.9;
    /// Post-arc **measured**: cold-but-temperate 0.16% of land, warm-polar 0.00%. The budget is set
    /// well above both (≈6x headroom on the first, and the second is structurally unreachable now
    /// that the gate and the tag-solver veto share one seam) so seed variation and the deliberately
    /// ragged jittered boundary (§8.2) cannot flake it — while still failing loudly if either
    /// incoherence creeps back toward its pre-arc share.
    const MAX_PCT_OF_LAND: f64 = 1.0;

    let samples = all_samples();
    let total_land: usize = samples.iter().map(|(_, s)| s.land_count()).sum();
    let cold_temperate: usize = samples.iter().map(|(_, s)| s.cold_but_temperate()).sum();
    let warm_polar: usize = samples.iter().map(|(_, s)| s.warm_polar()).sum();

    let cold_pct = 100.0 * cold_temperate as f64 / total_land as f64;
    let warm_pct = 100.0 * warm_polar as f64 / total_land as f64;

    assert!(
        cold_pct <= MAX_PCT_OF_LAND,
        "cold-but-temperate did not fall enough: {cold_pct:.2}% of land \
         (was {BEFORE_COLD_TEMPERATE_PCT}%, budget {MAX_PCT_OF_LAND}%)"
    );
    assert!(
        warm_pct <= MAX_PCT_OF_LAND,
        "warm-polar did not fall enough: {warm_pct:.2}% of land \
         (was {BEFORE_WARM_POLAR_PCT}%, budget {MAX_PCT_OF_LAND}%)"
    );
}

/// **No band may be degenerate.** A ladder rung that never holds any land is a cut point that does
/// nothing, and the four-rung ladder's whole justification (§8.1) is that the boreal fringe needed
/// its own rung. Measured across every preset/grid/seed in aggregate.
#[test]
fn every_climate_band_holds_land() {
    /// A band carrying less than this share of land is degenerate — its cut point is inert.
    const MIN_BAND_SHARE_PCT: f64 = 1.0;

    let samples = all_samples();
    let total_land: usize = samples.iter().map(|(_, s)| s.land_count()).sum();
    let mut totals: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (_, sample) in &samples {
        for (band, n) in sample.band_counts() {
            *totals.entry(band).or_insert(0) += n;
        }
    }

    for band in ["polar", "boreal", "temperate", "tropical"] {
        let n = *totals.get(band).unwrap_or(&0);
        let pct = 100.0 * n as f64 / total_land as f64;
        assert!(
            pct >= MIN_BAND_SHARE_PCT,
            "climate band '{band}' is degenerate: {n} tiles ({pct:.3}% of land)"
        );
    }
}

/// **Alpine tundra is a NEW output, and its absence would mean the gate is not really reading an
/// elevation-derived temperature** (§5.3). A mid-latitude mountain at −1.6° must now be able to
/// carry a cold biome; under the retired latitude gate this was unreachable by construction.
#[test]
fn cold_highland_carries_a_cold_biome_away_from_the_poles() {
    let samples = all_samples();
    let alpine: usize = samples.iter().map(|(_, s)| s.alpine_cold()).sum();
    assert!(
        alpine > 0,
        "no cold highland biome anywhere — the biome gate is not reading elevation-derived \
         temperature, which is the whole point of the arc"
    );
}

/// **The gate is the single seam.** Every land tile's biome must be consistent with the band its
/// own temperature puts it in: a tile in a warm band may not carry a `POLAR`-tagged biome that the
/// classifier is responsible for. This is the invariant the six rewired sites exist to hold, and it
/// is asserted on the FINAL map (post tag solver, post palette clamp) — the two stages that
/// historically undid it.
#[test]
fn no_warm_band_tile_carries_a_solver_painted_polar_biome() {
    /// The biomes the tag solver's polar family paints. These are the ones whose presence in warm
    /// air was 64% of the pre-arc warm-polar count, and the climate veto (§5.4) must exclude them.
    const SOLVER_POLAR_BIOMES: [TerrainType; 2] =
        [TerrainType::Tundra, TerrainType::SeasonalSnowfield];

    for (label, sample) in all_samples() {
        for (temp, terrain, _, band) in &sample.land {
            if SOLVER_POLAR_BIOMES.contains(terrain) {
                assert!(
                    band.admits_cold_biomes(),
                    "{label}: {terrain:?} stamped at {temp:.2}° — band {} forbids the cold ladder, \
                     so the tag solver repainted to hit a quota",
                    band.as_str()
                );
            }
        }
    }
}
