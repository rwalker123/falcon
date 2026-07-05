mod common;

use core_sim::{build_headless_app, SimulationConfig, SimulationConfigMetadata, SnapshotHistory};
use sim_runtime::{decode_map_export_json, encode_map_export_json, MapExport, WorldSnapshot};

/// Seed used to generate a deterministic map for the round-trip fixture. Any
/// non-zero value works; a fixed one keeps the exported terrain reproducible.
const FIXTURE_SEED: u64 = 0x0FA1_C0DE;

/// Generate a world deterministically and return its snapshot plus the resolved
/// seed and preset id — the same three inputs the server bundles into a
/// `MapExport`.
fn generate_fixture_world() -> (u64, String, WorldSnapshot) {
    common::ensure_test_config();
    let mut app = build_headless_app();
    if let Some(mut metadata) = app.world.get_resource_mut::<SimulationConfigMetadata>() {
        metadata.set_seed_random(false);
    }
    if let Some(mut config) = app.world.get_resource_mut::<SimulationConfig>() {
        if config.map_seed == 0 {
            config.map_seed = FIXTURE_SEED;
        }
    }
    // A single update runs the startup worldgen and captures the first snapshot.
    app.update();

    let config = app.world.resource::<SimulationConfig>();
    let seed = config.map_seed;
    let preset = config.map_preset_id.clone();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .as_ref()
        .map(|snapshot| (**snapshot).clone())
        .expect("snapshot available after worldgen");
    (seed, preset, snapshot)
}

/// Exercises the full export → disk → decode pipeline the `export_map` command
/// relies on, then asserts on specific hexes by coordinate. This is the pattern
/// a real fixture test would follow: generate (or load) a map, then make claims
/// about individual tiles.
#[test]
fn map_export_round_trips_through_disk_and_indexes_by_coordinate() {
    let (seed, preset, snapshot) = generate_fixture_world();
    let expected_terrain = snapshot.terrain.clone();

    let export = MapExport::from_snapshot(seed, preset.clone(), snapshot);

    // Dimensions are derived from the terrain overlay, never desynced from it.
    assert_eq!(export.width, expected_terrain.width);
    assert_eq!(export.height, expected_terrain.height);
    assert!(
        export.width > 0 && export.height > 0,
        "map must be non-empty"
    );
    assert_eq!(
        export.snapshot.terrain.samples.len() as u32,
        export.width * export.height,
        "sample buffer must cover the whole grid"
    );

    // Write to disk and read back — the real path the server takes.
    let json = encode_map_export_json(&export).expect("encode map export");
    let path = std::env::temp_dir().join(format!("falcon-map-export-{seed:x}.json"));
    std::fs::write(&path, &json).expect("write export fixture");
    let raw = std::fs::read_to_string(&path).expect("read export fixture");
    let _ = std::fs::remove_file(&path);

    let decoded = decode_map_export_json(&raw).expect("decode map export");

    // Metadata survives the round trip.
    assert_eq!(decoded.seed, seed);
    assert_eq!(decoded.preset, preset);
    assert_eq!(decoded.width, export.width);
    assert_eq!(decoded.height, export.height);
    // Terrain is byte-for-byte identical after JSON round trip.
    assert_eq!(decoded.snapshot.terrain, expected_terrain);

    // `tile_at` resolves row-major (x, y) and agrees with raw indexing.
    let (w, h) = (decoded.width, decoded.height);
    for &(x, y) in &[(0u32, 0u32), (w / 2, h / 2), (w - 1, h - 1)] {
        let idx = (y as usize) * (w as usize) + (x as usize);
        let expected = &decoded.snapshot.terrain.samples[idx];
        let actual = decoded
            .tile_at(x, y)
            .unwrap_or_else(|| panic!("tile_at({x}, {y}) should be in range"));
        assert_eq!(
            actual, expected,
            "tile_at({x}, {y}) must match samples[{idx}]"
        );
    }

    // Out-of-range coordinates yield None rather than panicking or wrapping.
    assert!(decoded.tile_at(w, 0).is_none());
    assert!(decoded.tile_at(0, h).is_none());
}
