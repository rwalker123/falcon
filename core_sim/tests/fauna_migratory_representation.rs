//! **How often does each migratory species actually appear on a map?** (issue #290)
//!
//! The report sweeps seeds at the shipped standard map size and counts, per migratory species, how
//! many maps carry at least one herd of it — beside the *habitat* each species has to work with
//! (host-module land tiles). Habitat and appearance are separate questions: a species can be
//! plentiful in ground and still never seat, and vice versa.
//!
//! Run it with `cargo test -p core_sim --test fauna_migratory_representation -- --ignored
//! --nocapture`.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    classify_food_module, spawn_initial_herds, spawn_initial_world, CultureManager,
    DiscoveryProgressLedger, FactionInventory, FaunaConfig, FaunaConfigHandle, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, MapPresets, MapPresetsHandle, SimulationConfig,
    SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
};

/// How many maps the sweep generates. Far wider than the six-seed guards elsewhere because the
/// quantity measured is a per-map *draw* from the migratory roster, not a terrain count — a handful
/// of maps says nothing about a 2-slot lottery, and at 24 maps a uniform draw and a broken one are
/// still only ~2σ apart.
const SWEEP_MAPS: u64 = 120;

/// Seed `i` of the sweep. `1..=SWEEP_MAPS` (never 0 — the "roll from entropy" sentinel).
fn sweep_seed(i: u64) -> u64 {
    i + 1
}

/// The shipped standard map dimensions.
const GRID: UVec2 = UVec2::new(80, 52);

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

/// Stand up an earthlike map on `seed` and run the real `spawn_initial_herds`.
fn spawn_world(seed: u64) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = GRID;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world
        .insert_resource(FaunaConfigHandle::new(Arc::new(builtin_fauna())));
    app.world.run_system_once(spawn_initial_herds);
    app
}

fn survey(seed: u64) -> Survey {
    let app = spawn_world(seed);
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

/// Report only — never asserts a bound. Prints per-species map presence, total herds, and mean host
/// habitat across the sweep, so the representation question is answered with numbers.
#[test]
#[ignore = "measurement report; run explicitly with --ignored --nocapture"]
fn migratory_representation_report() {
    let fauna = builtin_fauna();
    let species: Vec<String> = fauna
        .migratory_species()
        .iter()
        .map(|(_, def)| def.display_name.clone())
        .collect();

    let mut maps_with: BTreeMap<String, usize> = species.iter().cloned().map(|s| (s, 0)).collect();
    let mut total_herds: BTreeMap<String, usize> =
        species.iter().cloned().map(|s| (s, 0)).collect();
    let mut habitat_sum: BTreeMap<String, usize> =
        species.iter().cloned().map(|s| (s, 0)).collect();
    let mut habitat_min: BTreeMap<String, usize> =
        species.iter().cloned().map(|s| (s, usize::MAX)).collect();
    let mut herds_per_map = Vec::new();

    for i in 0..SWEEP_MAPS {
        let seed = sweep_seed(i);
        let s = survey(seed);
        herds_per_map.push(s.draws.len());
        println!("seed {seed:>10}: {:?}", s.draws);
        for name in &species {
            let n = s.migratory_herds.get(name).copied().unwrap_or(0);
            if n > 0 {
                *maps_with.get_mut(name).unwrap() += 1;
            }
            *total_herds.get_mut(name).unwrap() += n;
            let h = s.host_tiles.get(name).copied().unwrap_or(0);
            *habitat_sum.get_mut(name).unwrap() += h;
            let slot = habitat_min.get_mut(name).unwrap();
            *slot = (*slot).min(h);
        }
    }

    let maps = SWEEP_MAPS as usize;
    let draws: usize = herds_per_map.iter().sum();
    println!(
        "\n=== migratory representation over {maps} maps ({}x{}) ===",
        GRID.x, GRID.y
    );
    let min_per_map = herds_per_map.iter().min().copied().unwrap_or(0);
    let max_per_map = herds_per_map.iter().max().copied().unwrap_or(0);
    println!(
        "migratory herds per map: min {min_per_map}, max {max_per_map}, total {draws} \
         ({:.2}/map over {} migratory rows)",
        draws as f64 / maps as f64,
        species.len()
    );
    // The uniform-draw expectation each species should hit if the roster pick is unbiased.
    let expected = draws as f64 / species.len() as f64;
    println!("uniform-draw expectation: {expected:.1} herds/species");
    println!(
        "{:<18} {:>8} {:>8} {:>10} {:>11} {:>14}",
        "species", "herds", "vs exp", "maps with", "% of maps", "mean habitat"
    );
    let mut chi_square = 0.0f64;
    for name in &species {
        let with = maps_with[name];
        let herds = total_herds[name];
        chi_square += (herds as f64 - expected).powi(2) / expected;
        println!(
            "{:<18} {:>8} {:>+8.1} {:>10} {:>10.0}% {:>9.0} (min {})",
            name,
            herds,
            herds as f64 - expected,
            with,
            100.0 * with as f64 / maps as f64,
            habitat_sum[name] as f64 / maps as f64,
            habitat_min[name],
        );
    }
    println!(
        "chi-square vs uniform: {chi_square:.1} (df {}), critical @ p=0.01 ~ 13.3",
        species.len() - 1
    );
}
