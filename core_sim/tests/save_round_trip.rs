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
use common::{canonical_tree, differing_paths};
use core_sim::{
    build_test_app, publish_baseline_snapshot, run_turn, scalar_one, BiomePalette,
    DiscoveryProgressLedger, FactionId, FoodSiteRegistry, HydrologyState, MoistureRaster,
    PowerTopology, ProvinceMap, SimulationConfig, SnapshotHistory, StartLocation, Tile,
    TileRegistry, WorldGenSeed, CULTIVATION_DISCOVERY_ID, HERDING_DISCOVERY_ID,
    SEED_SELECTION_DISCOVERY_ID,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// Turns resolved before saving, so the blob holds a world that has run rather than bare worldgen
/// output.
///
/// ⛔ **FOUR TURNS CANNOT EXHIBIT ANYTHING THE WORLD HAD TO EARN**, and that is why a whole suite
/// passed while a reported "everything the faction knew was announced again after a load" defect
/// went looking for a home here. A four-turn world has learned nothing, so a checkpoint that
/// dropped every ladder discovery would round-trip an empty ledger to an empty ledger and every
/// assertion in this file would still be green. The tests that need a world with *state worth
/// losing* build one deliberately — see [`a_world_with_earned_state`].
const TURNS_BEFORE_SAVE: usize = 4;

/// Turns resolved after [`a_world_with_earned_state`] seeds the ladder, so the knowledge is not the
/// only thing in the world that moved after worldgen and the save carries a world mid-flight.
const TURNS_AFTER_SEEDING: usize = 6;

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

/// The three ladder tracks a played world has behind it, and the ones a playtest reported being
/// announced as freshly learned on the turn after a load.
///
/// Named by id here because that is what the checkpoint carries; the wire names them by the
/// `knowledge_id` strings the ladder config declares, which is what [`ladder_progress_on_the_wire`]
/// reads back.
const EARNED_TRACKS: [(&str, u32); 3] = [
    ("cultivation", CULTIVATION_DISCOVERY_ID),
    ("herding", HERDING_DISCOVERY_ID),
    ("seed_selection", SEED_SELECTION_DISCOVERY_ID),
];

/// **A world that has something to lose** — worldgen, some turns, the three ladder tracks known,
/// and more turns on top so the knowledge is not the newest thing in it.
///
/// ⛔ **THE KNOWLEDGE IS SEEDED THROUGH THE LEDGER, NOT EARNED BY WORKING SOURCES**, and that is a
/// deliberate limit on what these tests claim. Earning it needs bands assigned to a thriving patch
/// and a thriving herd for ~20 turns each, which means picking tiles out of a generated map — a
/// fixture that would break whenever worldgen tuning moved, to prove something
/// `systems/labor.rs`'s own tests already prove. `add_progress` is the same seam
/// `RungDef::knowledge_accrual` writes through, and it is the idiom the forage and husbandry
/// suites already use (`forage_cultivation.rs::grant_cultivation_knowledge`).
///
/// What is under test here is the **checkpoint and the frame**, so a ledger that got there by hand
/// is the same ledger.
fn a_world_with_earned_state() -> App {
    let mut app = spawn_world();
    for _ in 0..TURNS_BEFORE_SAVE {
        run_turn(&mut app);
    }
    {
        let mut ledger = app.world.resource_mut::<DiscoveryProgressLedger>();
        for (_, discovery) in EARNED_TRACKS {
            ledger.add_progress(FactionId(0), discovery, scalar_one());
        }
    }
    for _ in 0..TURNS_AFTER_SEEDING {
        run_turn(&mut app);
    }
    app
}

/// The ladder progress the CLIENT reads, decoded from a published frame rather than read off the
/// struct that produced it.
///
/// Returns `None` when the frame carries no `intensificationKnowledge` at all — which on a delta is
/// the ordinary "this section did not change" and on a full snapshot is the defect.
fn ladder_progress_on_the_wire(frame: &[u8]) -> Option<Vec<(String, f32)>> {
    let envelope = fb::root_as_envelope(frame).expect("a published frame is a valid envelope");
    let subsistence = match envelope.payload_type() {
        fb::SnapshotPayload::snapshot => envelope
            .payload_as_snapshot()
            .expect("the envelope carries a full snapshot")
            .subsistence(),
        fb::SnapshotPayload::delta => envelope
            .payload_as_delta()
            .expect("the envelope carries a delta")
            .subsistence(),
        other => panic!("unexpected payload {other:?}"),
    };
    let rows = subsistence?.intensificationKnowledge()?;
    let row = rows.iter().find(|row| row.faction() == 0)?;
    Some(
        row.knowledges()?
            .iter()
            .map(|entry| {
                (
                    entry.knowledgeId().unwrap_or_default().to_string(),
                    entry.progress(),
                )
            })
            .collect(),
    )
}

fn assert_tracks_known_on_the_wire(frame: &[u8], moment: &str) {
    let published =
        ladder_progress_on_the_wire(frame).unwrap_or_else(|| panic!("{moment}: the frame carries no intensificationKnowledge at all, so the client has no ladder to read"));
    for (knowledge, _) in EARNED_TRACKS {
        let progress = published
            .iter()
            .find(|(id, _)| id == knowledge)
            .map(|(_, progress)| *progress)
            .unwrap_or_else(|| panic!("{moment}: `{knowledge}` is missing from the published roster; the whole roster was {published:?}"));
        assert_eq!(
            progress, 1.0,
            "{moment}: `{knowledge}` reads {progress} on the wire, so the client sees a track this \
             faction earned long ago as unlearned — and will announce it as new the moment the \
             section is published again"
        );
    }
}

/// **A world that KNOWS things still knows them after a load, on the wire the client reads.**
///
/// The regression for a playtest report: a save loaded at turn 71 announced *"Cultivation learned"*,
/// *"Seed Selection learned"* and *"Herding learned"* on the next turn, all earned dozens of turns
/// before the save. Three separate places have to carry the knowledge for that not to happen, and
/// each is asserted below, because they fail differently:
///
/// 1. the **checkpoint** inside the blob — `SimState::discovery_progress`;
/// 2. the **restored world** — the ledger `apply_save` leaves behind;
/// 3. the **published baseline frame** — the first thing a loaded world puts on the wire, captured
///    by `publish_baseline_snapshot` with no turn in between. This is the one that decides what the
///    client's "learned this turn" diff seeds itself from; a world that is right and a frame that is
///    empty produce exactly the reported symptom.
///
/// And then a turn is resolved, because the announcement the player saw came *after* pressing next
/// turn: a section that arrives late reads as a discovery, so the delta must either leave the
/// knowledge alone or restate it as known.
#[test]
fn a_world_that_has_learned_keeps_its_knowledge_across_a_load() {
    let original = a_world_with_earned_state();

    for (knowledge, discovery) in EARNED_TRACKS {
        assert_eq!(
            original
                .world
                .resource::<DiscoveryProgressLedger>()
                .get_progress(FactionId(0), discovery),
            scalar_one(),
            "the fixture must KNOW `{knowledge}` before the save, or this test cannot lose it"
        );
    }

    let blob = encode_save(&original.world).expect("the world encodes");

    // 1 — the checkpoint in the bytes.
    let (header, payload) = decode_save(&blob).expect("the save decodes");
    for (knowledge, discovery) in EARNED_TRACKS {
        assert_eq!(
            payload
                .sim
                .discovery_progress
                .get_progress(FactionId(0), discovery),
            scalar_one(),
            "the checkpoint dropped `{knowledge}`"
        );
    }

    // 2 — the world the checkpoint rebuilds.
    let (mut loaded, _) = load_save(&blob).expect("the save loads");
    for (knowledge, discovery) in EARNED_TRACKS {
        assert_eq!(
            loaded
                .world
                .resource::<DiscoveryProgressLedger>()
                .get_progress(FactionId(0), discovery),
            scalar_one(),
            "the restored world forgot `{knowledge}` — the checkpoint carried it and a pass \
             dropped it"
        );
    }

    // 3 — the first frame the loaded world publishes, which is what the client's diff seeds from.
    publish_baseline_snapshot(&mut loaded.world);
    let baseline = loaded
        .world
        .resource::<SnapshotHistory>()
        .encoded_snapshot_flat()
        .expect("a loaded world's first publication is a FULL frame, not a delta");
    assert_tracks_known_on_the_wire(&baseline, "the loaded world's baseline frame");

    // …and the turn the player presses next must not re-teach any of it.
    run_turn(&mut loaded);
    for (knowledge, discovery) in EARNED_TRACKS {
        assert_eq!(
            loaded
                .world
                .resource::<DiscoveryProgressLedger>()
                .get_progress(FactionId(0), discovery),
            scalar_one(),
            "the turn after the load un-learned `{knowledge}`"
        );
    }
    let history = loaded.world.resource::<SnapshotHistory>();
    if let Some(delta) = history.encoded_delta_flat() {
        // A delta that omits the section is the correct answer — nothing changed. One that carries
        // it must still say KNOWN, because a client applies what it is sent.
        if ladder_progress_on_the_wire(&delta).is_some() {
            assert_tracks_known_on_the_wire(&delta, "the delta for the turn after the load");
        }
    }
    // The fixture is deep enough to have earned something, which is the property four turns could
    // not carry. `spawn_world`'s own `update()` resolves the first turn, hence the `+ 1`.
    assert_eq!(
        header.turn,
        1 + TURNS_BEFORE_SAVE as u64 + TURNS_AFTER_SEEDING as u64,
        "the fixture must run past the point where a played world has knowledge to lose"
    );
}

/// **A loaded world publishes the frame the live one did**, field for field, at the same tick.
///
/// The general form of the test above, and the one that actually caught something: `CrisisOverlayCache`
/// was classified *derived*, so nothing carried it — and a load has no turn between `apply_save` and
/// the client's first frame, so that frame went out with a `0x0` crisis heatmap where the live world
/// had published an 80x52 one with 4,160 samples. A rollback could never show it, because a rollback
/// republishes a stored ring entry instead of re-capturing.
///
/// **`frame_seq` is the one exemption, and it is not a fudge**: it counts publications rather than
/// ticks and is reset with the world epoch, so a freshly loaded world's first publication is `1` by
/// design. Everything else in the frame is a statement about the world and must match.
#[test]
fn a_loaded_world_publishes_the_frame_the_live_one_did() {
    let original = a_world_with_earned_state();
    let live = original
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .expect("the live world has published its turn");

    let blob = encode_save(&original.world).expect("the world encodes");
    let (mut loaded, _) = load_save(&blob).expect("the save loads");
    publish_baseline_snapshot(&mut loaded.world);
    let restored = loaded
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .expect("the loaded world published a baseline");

    let differences: Vec<String> = differing_paths(live.as_ref(), restored.as_ref())
        .into_iter()
        .filter(|path| !path.starts_with(".header.frame_seq"))
        .collect();
    assert!(
        differences.is_empty(),
        "the loaded world's first frame differs from the live world's at the same tick, so the \
         client sees these fields change across a load and change back a turn later:\n  {}",
        differences.join("\n  ")
    );
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

    // **The bound the slot listing reads to.** `list_slots` reads a prefix rather than the file, so
    // the header has to fit inside it; a header that outgrew the bound costs a second, whole-file
    // read per row (`read_header_only` re-reads rather than dropping the slot). The header does not
    // scale with the map — this is the largest one the suite builds — so failing here means the
    // `config_fingerprint` grew past the headroom and `HEADER_PREFIX_BYTES` wants raising.
    assert!(
        (header_only.len() as u64) < core_sim::save_store::HEADER_PREFIX_BYTES,
        "the header is {} bytes and the listing reads only {} — raise HEADER_PREFIX_BYTES",
        header_only.len(),
        core_sim::save_store::HEADER_PREFIX_BYTES
    );
}
