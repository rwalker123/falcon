//! The lake-abundance guard.
//!
//! A lake (`InlandSea`) is water the mask leaves unconnected to the ocean — a closed basin the
//! heightfield produced whose floor the contour anchor lands below `sea_level`. Nothing places or
//! repairs one; abundance is purely an outcome of the continental envelope (see `core_sim/CLAUDE.md`
//! → "Lakes are emergent").
//!
//! **Why this test exists.** For months `connect_inland_seas_via_straits` silently destroyed ~81% of
//! the map's lakes and *nothing measured it*, so the loss was invisible until a playtester noticed
//! maps read dry. Deleting the carver, then adding the `continental_basin_amplitude` interior-sink
//! term, lifted earthlike's 24-seed median lake share from ~1.5% (a third of maps under 1%) to
//! ~2.7% (mean ~3.4%, right-skewed — many maps 3–8%). This guard is the tripwire that keeps a future
//! worldgen change from quietly draining the map again — the lake analog of
//! `fauna_coastal_habitat::seals_clear_the_delta_pinhole`.
//!
//! It is a **distribution floor over a seed sweep**, not a per-seed pin: individual seeds range from
//! ~1% to ~9%, so any single-seed assertion would flake (though the sweep median is deterministic —
//! worldgen has no RNG for a fixed seed). The floor sits between the healthy median (~2.7%) and the
//! too-dry regime the map falls back into if the term is lost (~1.5%), so ordinary retuning does not
//! trip it but a real lake-draining regression does.

use bevy::app::App;
use bevy::prelude::{UVec2, World};
use bevy::MinimalPlugins;

use core_sim::{
    generate_hydrology, spawn_initial_world, CultureManager, DiscoveryProgressLedger,
    FactionInventory, GenerationRegistry, MapPresets, MapPresetsHandle, SimulationConfig,
    SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
};
use sim_runtime::TerrainType;

/// A wide spread of ordinary seeds (never 0 — that is the "roll from entropy" sentinel), at the
/// shipped Standard map size the default game uses. Deliberately many: per-seed lake share is noisy,
/// so a small set's median swings with a couple of lake-rich outliers. Across 24 the median is a
/// stable statistic — measured ~2.7% with the interior-sink term, ~1.5% without it.
const SEEDS: [u64; 24] = [
    1,
    2,
    3,
    4,
    5,
    6,
    7,
    8,
    9,
    10,
    11,
    12,
    13,
    14,
    15,
    16,
    17,
    18,
    19,
    20,
    21,
    22,
    23,
    119_304_647,
];
const GRID: UVec2 = UVec2::new(80, 52);

/// The sweep's **median** lake share (fraction of land) must be at least this. The interior-sink term
/// puts the median at ~2.7%; the ~1.5% the map produced before the term is the regime a regression
/// falls back into. This 2% floor sits between them — clearing healthy with headroom, tripping on a
/// fall back to the too-dry regime (proven: the median is 1.50% with the term disabled).
const MIN_MEDIAN_LAKE_FRACTION: f32 = 0.02;
/// Runaway guard: a term that floods the interior would read as a huge lake share. No seed may exceed
/// this. (Measured max is ~9%.)
const MAX_LAKE_FRACTION: f32 = 0.20;

fn earthlike_world(seed: u64, grid: UVec2) -> World {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = grid;
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
    generate_hydrology(&mut app.world);
    app.world
}

/// Lake tiles as a fraction of land tiles.
fn lake_fraction(world: &World, w: u32, h: u32) -> f32 {
    let registry = world.resource::<TileRegistry>();
    let (mut lake, mut land) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let Some(entity) = registry.index(x, y) else {
                continue;
            };
            let Some(tile) = world.get::<Tile>(entity) else {
                continue;
            };
            match tile.terrain {
                TerrainType::InlandSea => lake += 1,
                TerrainType::DeepOcean
                | TerrainType::ContinentalShelf
                | TerrainType::CoralShelf
                | TerrainType::HydrothermalVentField => {}
                _ => land += 1,
            }
        }
    }
    lake as f32 / land.max(1) as f32
}

#[test]
fn earthlike_generates_a_healthy_lake_share() {
    let mut fractions: Vec<f32> = SEEDS
        .iter()
        .map(|&seed| lake_fraction(&earthlike_world(seed, GRID), GRID.x, GRID.y))
        .collect();

    for (seed, frac) in SEEDS.iter().zip(&fractions) {
        assert!(
            *frac <= MAX_LAKE_FRACTION,
            "seed {seed}: lake share {:.2}% exceeds the runaway ceiling {:.0}% — a term is flooding \
             the interior",
            100.0 * frac,
            100.0 * MAX_LAKE_FRACTION,
        );
    }

    fractions.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = fractions[fractions.len() / 2];
    assert!(
        median >= MIN_MEDIAN_LAKE_FRACTION,
        "median lake share {:.2}% fell below the floor {:.0}% over {} seeds — the map is draining \
         dry again (see core_sim/CLAUDE.md → \"Lakes are emergent\"). Per-seed: {:?}",
        100.0 * median,
        100.0 * MIN_MEDIAN_LAKE_FRACTION,
        SEEDS.len(),
        fractions
            .iter()
            .map(|f| format!("{:.2}%", 100.0 * f))
            .collect::<Vec<_>>(),
    );
}
