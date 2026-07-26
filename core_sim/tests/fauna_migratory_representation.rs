//! **How often does each migratory species actually appear on a map?** (issue #290)
//!
//! The report sweeps seeds at the shipped standard map size and counts, per migratory species, how
//! many maps carry at least one herd of it — beside the *habitat* each species has to work with
//! (host-module land tiles). Habitat and appearance are separate questions: a species can be
//! plentiful in ground and still never seat, and vice versa.
//!
//! It reports **both** migratory budgets — the retired `determine_herd_count` parameters
//! (`area/3000` clamped `[2, 6]`) and the shipped `abundance.migratory` ones — over the same seeds on
//! the same pipeline, so the before/after is a controlled comparison rather than two readings taken
//! under different conditions.
//!
//! Run it with `cargo test -p core_sim --test fauna_migratory_representation --release --
//! --ignored --nocapture`.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::{
    build_headless_app, classify_food_module, FaunaConfig, FaunaConfigHandle, HerdRegistry,
    MigratoryAbundanceConfig, SimulationConfig, Tile, TileRegistry,
};

/// How many maps the sweep generates. Far wider than the six-seed guards elsewhere because the
/// quantity measured is a per-map *draw* from the migratory roster, not a terrain count — a handful
/// of maps says nothing about a slot lottery, and at 24 maps a uniform draw and a broken one are
/// still only ~2σ apart (measured: a 24-map read put Steppe Runners at 1 herd, which looks exactly
/// like a broken `host_biomes` key and is not).
const SWEEP_MAPS: u64 = 120;

/// Seed `i` of the sweep. `1..=SWEEP_MAPS` (never 0 — the "roll from entropy" sentinel).
fn sweep_seed(i: u64) -> u64 {
    i + 1
}

/// The shipped standard map dimensions.
const GRID: UVec2 = UVec2::new(80, 52);

/// The **retired** `fauna::determine_herd_count` literals, expressed in the config block that
/// replaced them: `area / 3000` clamped `[2, 6]`. Kept so the report can measure the before state on
/// the *same* pipeline as the after state — the whole point of issue #290's finding is a number, and
/// a before/after taken under two different pipelines is not a comparison.
fn retired_budget() -> MigratoryAbundanceConfig {
    MigratoryAbundanceConfig {
        tiles_per_herd: 3000,
        min_herds: 2,
        max_herds: 6,
    }
}

/// One map's reading: which migratory species seated, and how much host ground each had.
struct Survey {
    /// Herd count on this map, keyed by species `display_name` (migratory rows only).
    migratory_herds: BTreeMap<String, usize>,
    /// Land tiles whose food module is in the species' `host_biomes`, keyed by `display_name`.
    host_tiles: BTreeMap<String, usize>,
    /// Every migratory herd's species in spawn order, so a doubled draw is visible.
    draws: Vec<String>,
}

fn builtin_fauna() -> FaunaConfig {
    (*FaunaConfig::builtin()).clone()
}

/// Generate `seed`'s map through the **real** Startup chain — worldgen → hydrology → tag budget
/// solver → biome palette clamp → coastal shelf reconcile → `spawn_initial_herds` — by running the
/// shipped schedule rather than a hand-picked subset of it.
///
/// **This must not be hand-rolled**, and the sibling sweeps (`fauna_coastal_habitat.rs`,
/// `fauna_wet_biome_roster.rs`) say the same in their own headers. The skipped passes rewrite exactly
/// the quantity measured here: `earthlike` locks `Fertile` (0.22) and `Wetland` (0.06), so
/// `apply_tag_budget_solver` repaints tiles into `AlluvialPlain`/`Floodplain` (→ `riverine_delta`)
/// and `FreshwaterMarsh` (→ `wetland_swamp`) — the marsh grazer's two `host_biomes` — and
/// `generate_hydrology` stamps `RiverDelta`/`Floodplain`/`FreshwaterMarsh`/`NavigableRiver` into
/// those same modules. A pre-solver harness therefore reports a **pre-solve reading of a post-solve
/// quantity**, and `.claude/rules/core_sim/worldgen.md` records that exact defect faking a whole
/// finding once already (`forage_field.rs`'s "sowable tiles 46 → 0").
///
/// `migratory` overrides the shipped budget so one sweep can measure several slot counts.
fn spawn_world(seed: u64, migratory: MigratoryAbundanceConfig) -> App {
    let mut app = build_headless_app();

    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = GRID;
    app.world.insert_resource(config);

    // Override the fauna budget *before* Startup, since `spawn_initial_herds` runs inside it.
    let mut fauna = builtin_fauna();
    fauna.abundance.migratory = migratory;
    app.world
        .insert_resource(FaunaConfigHandle::new(Arc::new(fauna)));

    app.world.run_schedule(bevy::app::Startup);
    app
}

fn survey(seed: u64, migratory: MigratoryAbundanceConfig) -> Survey {
    let app = spawn_world(seed, migratory);
    let fauna = builtin_fauna();

    // One pass over the map, bucketing land tiles by food module.
    let mut module_tiles: BTreeMap<String, usize> = BTreeMap::new();
    {
        let registry = app.world.resource::<TileRegistry>();
        for y in 0..GRID.y {
            for x in 0..GRID.x {
                let Some(entity) = registry.index(x, y) else {
                    continue;
                };
                let Some(tile) = app.world.get::<Tile>(entity) else {
                    continue;
                };
                if let Some(module) = classify_food_module(tile) {
                    *module_tiles.entry(module.as_str().to_string()).or_default() += 1;
                }
            }
        }
    }

    let mut host_tiles = BTreeMap::new();
    for (_, def) in fauna.migratory_species() {
        let tiles = def
            .host_biomes
            .iter()
            .map(|b| module_tiles.get(b.as_str()).copied().unwrap_or(0))
            .sum();
        host_tiles.insert(def.display_name.clone(), tiles);
    }

    let migratory_names: Vec<String> = fauna
        .migratory_species()
        .iter()
        .map(|(_, def)| def.display_name.clone())
        .collect();

    let mut migratory_herds: BTreeMap<String, usize> = BTreeMap::new();
    let mut draws = Vec::new();
    for herd in &app.world.resource::<HerdRegistry>().herds {
        if migratory_names.contains(&herd.species) {
            *migratory_herds.entry(herd.species.clone()).or_default() += 1;
            draws.push(herd.species.clone());
        }
    }

    Survey {
        migratory_herds,
        host_tiles,
        draws,
    }
}

/// One budget's aggregate over the whole sweep.
struct Aggregate {
    label: String,
    maps_with: BTreeMap<String, usize>,
    total_herds: BTreeMap<String, usize>,
    habitat_sum: BTreeMap<String, usize>,
    habitat_min: BTreeMap<String, usize>,
    herds_per_map: Vec<usize>,
}

fn run_sweep(label: &str, migratory: MigratoryAbundanceConfig, species: &[String]) -> Aggregate {
    let mut agg = Aggregate {
        label: label.to_string(),
        maps_with: species.iter().cloned().map(|s| (s, 0)).collect(),
        total_herds: species.iter().cloned().map(|s| (s, 0)).collect(),
        habitat_sum: species.iter().cloned().map(|s| (s, 0)).collect(),
        habitat_min: species.iter().cloned().map(|s| (s, usize::MAX)).collect(),
        herds_per_map: Vec::new(),
    };

    for i in 0..SWEEP_MAPS {
        let seed = sweep_seed(i);
        let s = survey(seed, migratory.clone());
        agg.herds_per_map.push(s.draws.len());
        for name in species {
            let n = s.migratory_herds.get(name).copied().unwrap_or(0);
            if n > 0 {
                *agg.maps_with.get_mut(name).unwrap() += 1;
            }
            *agg.total_herds.get_mut(name).unwrap() += n;
            let h = s.host_tiles.get(name).copied().unwrap_or(0);
            *agg.habitat_sum.get_mut(name).unwrap() += h;
            let slot = agg.habitat_min.get_mut(name).unwrap();
            *slot = (*slot).min(h);
        }
    }
    agg
}

fn print_aggregate(agg: &Aggregate, species: &[String]) {
    let maps = SWEEP_MAPS as usize;
    let draws: usize = agg.herds_per_map.iter().sum();
    let min_per_map = agg.herds_per_map.iter().min().copied().unwrap_or(0);
    let max_per_map = agg.herds_per_map.iter().max().copied().unwrap_or(0);
    println!("\n--- {} ---", agg.label);
    println!(
        "migratory herds per map: min {min_per_map}, max {max_per_map}, total {draws} \
         ({:.2}/map over {} migratory rows)",
        draws as f64 / maps as f64,
        species.len()
    );
    // The uniform-draw expectation each species should hit if the roster pick is unbiased.
    let expected = draws as f64 / species.len() as f64;
    println!(
        "{:<18} {:>8} {:>8} {:>10} {:>11} {:>14}",
        "species", "herds", "vs exp", "maps with", "% of maps", "mean habitat"
    );
    let mut chi_square = 0.0f64;
    for name in species {
        let with = agg.maps_with[name];
        let herds = agg.total_herds[name];
        chi_square += (herds as f64 - expected).powi(2) / expected;
        println!(
            "{:<18} {:>8} {:>+8.1} {:>10} {:>10.0}% {:>9.0} (min {})",
            name,
            herds,
            herds as f64 - expected,
            with,
            100.0 * with as f64 / maps as f64,
            agg.habitat_sum[name] as f64 / maps as f64,
            agg.habitat_min[name],
        );
    }
    println!(
        "uniform-draw expectation {expected:.1}/species; chi-square {chi_square:.1} (df {}), \
         critical @ p=0.01 ~ 13.3",
        species.len() - 1
    );
}

/// Report only — never asserts a bound. Prints per-species map presence, total herds, and mean host
/// habitat across the sweep for **both** the retired and the shipped migratory budget, so the
/// representation question is answered with numbers.
///
/// It deliberately asserts nothing: a floor on a probabilistic per-map draw is the
/// sitting-on-its-own-floor trap the seal colony guard already paid for
/// (`fauna_coastal_habitat.rs`'s `MIN_SEALS_OVER_SWEEP`, lowered 8 → 4 after it fired on noise).
/// The *structural* claims are pinned as ordinary unit tests on
/// `MigratoryAbundanceConfig::herds_for_map` instead.
#[test]
#[ignore = "measurement report; run explicitly with --ignored --nocapture"]
fn migratory_representation_report() {
    let fauna = builtin_fauna();
    let species: Vec<String> = fauna
        .migratory_species()
        .iter()
        .map(|(_, def)| def.display_name.clone())
        .collect();

    println!(
        "=== migratory representation over {SWEEP_MAPS} maps ({}x{}), full Startup pipeline ===",
        GRID.x, GRID.y
    );

    let before = run_sweep(
        "RETIRED budget (area/3000, clamp [2,6]) — the pre-#290 shipped values",
        retired_budget(),
        &species,
    );
    print_aggregate(&before, &species);

    let after = run_sweep(
        "SHIPPED budget (abundance.migratory)",
        fauna.abundance.migratory.clone(),
        &species,
    );
    print_aggregate(&after, &species);

    println!("\n=== presence delta (maps carrying at least one herd) ===");
    for name in &species {
        let maps = SWEEP_MAPS as f64;
        println!(
            "{:<18} {:>5.0}% -> {:>5.0}%",
            name,
            100.0 * before.maps_with[name] as f64 / maps,
            100.0 * after.maps_with[name] as f64 / maps,
        );
    }
}
