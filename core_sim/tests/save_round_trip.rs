//! A world becomes bytes and comes back **into a process where no worldgen has run**.
//!
//! That last clause is the whole point. `restore_sim_state` restores into the same live `World`,
//! which still holds the map worldgen built, so every world-static resource survives a rollback for
//! free — and a test that rolls back in-process therefore proves nothing about a save file. These
//! tests build a *second* app and load into that, which is the only arrangement that can catch a
//! forgotten raster.
//!
//! `a_loaded_world_simulates_forward_identically` is the one that matters most. A world that merely
//! *looks* restored at tick N is the failure mode this whole arc exists to catch: equality at N is
//! satisfied by any number of worlds that will disagree at N+1, because a missing input only shows
//! up once something reads it.

use bevy::prelude::*;

use core_sim::heightfield::ElevationField;
use core_sim::save::{
    decode_save, encode_save, load_save, read_save_header, SaveError, SAVE_FORMAT_VERSION,
    SAVE_MAGIC,
};
use core_sim::sim_state::capture_sim_state;

mod common;
use common::canonical_tree;
use core_sim::{
    build_test_app, run_turn, BiomePalette, FoodSiteRegistry, HydrologyState, MoistureRaster,
    PowerTopology, ProvinceMap, SimulationConfig, StartLocation, Tile, TileRegistry, WorldGenSeed,
};

/// Turns resolved before saving, so the blob holds a world that has run rather than bare worldgen
/// output.
const TURNS_BEFORE_SAVE: usize = 4;

/// Turns simulated on both sides after the load. Agreement at N is much weaker than agreement at
/// N+k, because a missing input only diverges once something reads it.
const TURNS_AFTER_LOAD: usize = 6;

/// The map the size measurement reports against.
const LARGE_GRID: UVec2 = UVec2::new(160, 104);

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The checkpoint as a canonical CBOR tree — the comparison currency.
///
/// **Not raw bytes**, and the difference is the whole reason `tests/common` exists: two worlds
/// holding equal `HashMap`s encode their entries in different orders, because a map serde rebuilt
/// and a map the sim grew have different table capacities. That is a difference in the encoding, not
/// in the world. Not `PartialEq` either — `LaborAllocation`'s hand-written impl skips its telemetry.
fn sim_tree(app: &App) -> ciborium::value::Value {
    canonical_tree(&capture_sim_state(&app.world))
}

fn live_tiles(app: &mut App) -> usize {
    app.world.query::<&Tile>().iter(&app.world).count()
}

#[test]
fn a_saved_world_loads_into_a_fresh_app() {
    let mut original = spawn_world();
    for _ in 0..TURNS_BEFORE_SAVE {
        run_turn(&mut original);
    }

    let blob = encode_save(&original.world).expect("the world encodes");
    let (mut loaded, header) = load_save(&blob).expect("the save loads");

    // The header describes the world it came from.
    assert_eq!(header.format_version, SAVE_FORMAT_VERSION);
    assert_eq!(
        header.world.world_seed,
        original.world.resource::<WorldGenSeed>().0
    );
    let grid = original.world.resource::<SimulationConfig>().grid_size;
    assert_eq!((header.world.width, header.world.height), (grid.x, grid.y));

    // The checkpoint half.
    assert_eq!(
        sim_tree(&loaded),
        sim_tree(&original),
        "the loaded world's checkpoint must encode identically to the saved one"
    );

    // The ground half — the part a same-process rollback never had to carry.
    assert_eq!(live_tiles(&mut loaded), live_tiles(&mut original));
    let (a, b) = (&loaded.world, &original.world);
    assert_eq!(
        a.resource::<ElevationField>().sea_level,
        b.resource::<ElevationField>().sea_level
    );
    assert_eq!(
        a.resource::<MoistureRaster>().values,
        b.resource::<MoistureRaster>().values
    );
    assert_eq!(
        a.resource::<HydrologyState>().rivers.len(),
        b.resource::<HydrologyState>().rivers.len()
    );
    assert_eq!(
        a.resource::<ProvinceMap>().province_count(),
        b.resource::<ProvinceMap>().province_count()
    );
    assert_eq!(
        a.resource::<FoodSiteRegistry>().sites().len(),
        b.resource::<FoodSiteRegistry>().sites().len()
    );
    assert_eq!(
        a.resource::<StartLocation>().position(),
        b.resource::<StartLocation>().position()
    );

    // `FoodSiteRegistry::positions` is not encoded — it is rebuilt from `sites` on decode. If that
    // reconstruction were skipped, `is_site` would answer `false` everywhere and the plant ladder's
    // site rule would silently forbid every rung.
    let a_sites = a.resource::<FoodSiteRegistry>();
    let b_sites = b.resource::<FoodSiteRegistry>();
    assert!(!b_sites.sites().is_empty(), "the map curates sites");
    for entry in b_sites.sites() {
        assert!(
            a_sites.is_site(entry.position),
            "the decoded registry must answer `is_site` for every site it holds"
        );
    }

    // Rebuilt rather than saved.
    assert_eq!(
        a.resource::<TileRegistry>().tiles.len(),
        b.resource::<TileRegistry>().tiles.len()
    );
    assert_eq!(
        a.resource::<PowerTopology>().node_count(),
        b.resource::<PowerTopology>().node_count()
    );
    // Re-derived rather than saved.
    assert_eq!(
        a.get_resource::<BiomePalette>().is_some(),
        b.get_resource::<BiomePalette>().is_some(),
        "the palette is a pure function of preset/seed/tile count and must come back"
    );
}

/// **The test that catches a world-static omission.** A missing raster does not change tick N; it
/// changes the first turn that reads it.
#[test]
fn a_loaded_world_simulates_forward_identically() {
    let mut original = spawn_world();
    for _ in 0..TURNS_BEFORE_SAVE {
        run_turn(&mut original);
    }

    let blob = encode_save(&original.world).expect("the world encodes");
    let (mut loaded, _) = load_save(&blob).expect("the save loads");

    for step in 1..=TURNS_AFTER_LOAD {
        run_turn(&mut original);
        run_turn(&mut loaded);
        // Compare EVERY step, not just the last: the first divergence is the readable one, and a
        // later tick's report is that drift plus everything it has since fed.
        assert_eq!(
            sim_tree(&loaded),
            sim_tree(&original),
            "a loaded world diverged from the saved one at step {step}"
        );
    }
}

/// Byte reproducibility, made true in the pass that removed two `HashSet`s from the checkpoint
/// closure. A save whose bytes move on their own cannot be compared, deduplicated or fingerprinted.
#[test]
fn encoding_one_world_twice_gives_identical_bytes() {
    let mut app = spawn_world();
    for _ in 0..TURNS_BEFORE_SAVE {
        run_turn(&mut app);
    }
    let first = encode_save(&app.world).expect("encodes");
    let second = encode_save(&app.world).expect("encodes again");
    assert_eq!(
        first, second,
        "two encodings of one world must agree byte for byte"
    );
}

/// A slot row costs a header, not a world.
#[test]
fn the_header_reads_without_decoding_the_payload() {
    let mut app = spawn_world();
    run_turn(&mut app);
    let blob = encode_save(&app.world).expect("encodes");

    let header = read_save_header(&blob).expect("the header reads");
    let (whole, _) = decode_save(&blob).expect("the whole save decodes");
    assert_eq!(header, whole);
    assert!(
        !header.campaign_title.is_empty(),
        "a slot row needs a title"
    );

    // Truncating to the header alone still reads, which is what proves the payload was never
    // touched rather than merely decoded and dropped.
    let mut header_bytes = Vec::from(SAVE_MAGIC);
    ciborium::into_writer(&header, &mut header_bytes).expect("the header re-encodes");
    assert_eq!(
        read_save_header(&header_bytes).expect("the header alone reads"),
        header
    );
}

/// A stale save is **refused**, never decoded. There is no migration code by design, so the version
/// is the only thing standing between an old blob and a plausible wrong world.
#[test]
fn a_save_from_another_format_version_is_refused() {
    let mut app = spawn_world();
    run_turn(&mut app);
    let blob = encode_save(&app.world).expect("encodes");

    let (mut header, payload) = decode_save(&blob).expect("decodes");
    let stale_version = SAVE_FORMAT_VERSION + 1;
    header.format_version = stale_version;
    let mut stale = Vec::from(SAVE_MAGIC);
    ciborium::into_writer(&header, &mut stale).expect("header encodes");
    ciborium::into_writer(&payload, &mut stale).expect("payload encodes");

    match read_save_header(&stale) {
        Err(SaveError::VersionMismatch { expected, found }) => {
            assert_eq!(expected, SAVE_FORMAT_VERSION);
            assert_eq!(found, stale_version);
        }
        other => panic!("expected a typed version mismatch, got {other:?}"),
    }
    assert!(
        matches!(decode_save(&stale), Err(SaveError::VersionMismatch { .. })),
        "the version must be checked before the payload is decoded"
    );
    assert!(matches!(
        load_save(&stale),
        Err(SaveError::VersionMismatch { .. })
    ));
}

/// Something that is not a save says so, rather than failing somewhere inside a CBOR document.
#[test]
fn a_blob_that_is_not_a_save_is_named_as_such() {
    assert!(matches!(
        read_save_header(b"this is not a save file at all"),
        Err(SaveError::BadMagic { .. })
    ));
    assert!(matches!(
        read_save_header(b"SH"),
        Err(SaveError::BadMagic { .. })
    ));
}

/// How big a save actually is, on the map size the arc cares about.
#[test]
fn a_large_map_save_is_measured() {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    config.grid_size = LARGE_GRID;
    app.world.insert_resource(config);
    app.update();
    for _ in 0..TURNS_BEFORE_SAVE {
        run_turn(&mut app);
    }

    let blob = encode_save(&app.world).expect("encodes");
    let header = read_save_header(&blob).expect("the header reads");
    let mut header_only = Vec::from(SAVE_MAGIC);
    ciborium::into_writer(&header, &mut header_only).expect("header encodes");

    println!(
        "save at {}x{} ({} tiles): {} bytes total, {} bytes of header",
        LARGE_GRID.x,
        LARGE_GRID.y,
        LARGE_GRID.x * LARGE_GRID.y,
        blob.len(),
        header_only.len()
    );
    assert!(blob.len() > header_only.len());
}
