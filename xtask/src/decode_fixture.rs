//! The **client decode fixtures** — the saturated `WorldSnapshot` from `sim_schema::fixture`
//! encoded to the exact FlatBuffers envelope the Godot client reads off the snapshot socket, plus
//! the delta envelopes chained against it.
//!
//! ## Why this exists
//!
//! The FlatBuffers → Godot `Dictionary` path in `clients/godot_thin_client/native/src/` had no
//! automated coverage: the `ui_preview` / `map_preview` PNG harnesses build hand-written GDScript
//! fixture dicts and hand them straight to `Hud`/`MapView`, so a fully green PNG run is compatible
//! with a completely broken decoder. `tools/decode_guard.gd` closes that by decoding a real
//! envelope through the real `SnapshotDecoder`; this module is where that envelope comes from.
//!
//! ## Where the snapshot comes from
//!
//! **`sim_schema::fixture::saturated_snapshot`** — seed every repeated field, saturate every scalar
//! leaf with a path-derived sentinel, fix up the structural fields. The builder lives beside the
//! codec because the codec's own round trip test consumes the same snapshot, and a section left
//! empty there is a section neither gate tests; the rationale for the saturation scheme is on that
//! module. This file owns only what is specific to the *client* guard: the file paths, the
//! headerless envelope, and the delta fixtures.

use flatbuffers::FlatBufferBuilder;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use sim_schema::codec::{encode_delta_flatbuffer, encode_snapshot_flatbuffer};
use sim_schema::fixture::{saturated_snapshot, GRID_CELLS, GRID_H, GRID_W};
use sim_schema::state::population::PopulationCohortState;
use sim_schema::world::{SnapshotHeader, WorldDelta, WorldSnapshot};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the encoded envelope lands. **Gitignored, not committed** — it is a pure function of this
/// file, and `cargo xtask decode-guard` regenerates it before every run, so a committed copy would
/// never be the copy under test: it could only go stale or collide (binary, so unmergeable). Run
/// `tools/decode_guard.tscn` directly and the guard fails with the command that writes it.
pub fn fixture_path() -> PathBuf {
    Path::new("clients")
        .join("godot_thin_client")
        .join("tests")
        .join("fixtures")
        .join("snapshot_envelope.bin")
}

/// Builds the fixture snapshot and writes its FlatBuffers envelope to [`fixture_path`].
pub fn write_fixture() -> Result<(), Box<dyn Error>> {
    let snapshot = saturated_snapshot()?;
    let bytes = encode_snapshot_flatbuffer(&snapshot);
    let path = fixture_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &bytes)?;
    println!(
        "Wrote client decode fixture to {} ({} bytes)",
        path.display(),
        bytes.len()
    );
    Ok(())
}

/// Where the HEADERLESS envelope lands. Gitignored beside the main fixture, for the same reason.
pub fn headerless_fixture_path() -> PathBuf {
    Path::new("clients")
        .join("godot_thin_client")
        .join("tests")
        .join("fixtures")
        .join("snapshot_headerless_envelope.bin")
}

/// Writes a **malformed-but-verifiable** envelope: a `WorldSnapshot` carrying a real map section
/// and **no `header`**.
///
/// `header` has no `required` attribute in the schema and `root_as_envelope` verifies table
/// STRUCTURE only, so this parses cleanly and reaches `snapshot_to_dict` with the field absent —
/// which used to `unwrap()` and take the client down. The guard decodes this fixture and asserts
/// the decoder answers an EMPTY dictionary (the "no frame" contract `SnapshotLoader.poll_stream`
/// already skips on), so both halves are pinned: it must not panic, and it must not publish a
/// half-identified world either.
///
/// It is built with the FlatBuffers builder directly rather than through
/// `encode_snapshot_flatbuffer`, because that encoder always writes a header — which is correct,
/// and is exactly why the malformed case cannot come from it. The map section is deliberately
/// **non-empty**: a snapshot that decoded to nothing anyway would not distinguish "the frame was
/// dropped" from "there was nothing in it".
pub fn write_headerless_fixture() -> Result<(), Box<dyn Error>> {
    let bytes = encode_headerless_envelope();
    let path = headerless_fixture_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &bytes)?;
    println!(
        "Wrote headerless decode fixture to {} ({} bytes)",
        path.display(),
        bytes.len()
    );
    Ok(())
}

fn encode_headerless_envelope() -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();

    // One real raster, so the snapshot carries decodable content the header's absence must
    // suppress. Values are irrelevant — nothing asserts on them; only the drop is asserted.
    let samples = builder.create_vector(&[0u16; GRID_CELLS]);
    let elevation = fb::ElevationOverlay::create(
        &mut builder,
        &fb::ElevationOverlayArgs {
            width: GRID_W,
            height: GRID_H,
            samples: Some(samples),
            ..Default::default()
        },
    );
    let map = fb::MapSection::create(
        &mut builder,
        &fb::MapSectionArgs {
            elevationOverlay: Some(elevation),
            ..Default::default()
        },
    );
    let snapshot = fb::WorldSnapshot::create(
        &mut builder,
        &fb::WorldSnapshotArgs {
            // The whole point of this fixture.
            header: None,
            map: Some(map),
            ..Default::default()
        },
    );
    let envelope = fb::Envelope::create(
        &mut builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::snapshot,
            payload: Some(snapshot.as_union_value()),
        },
    );
    builder.finish(envelope, None);
    builder.finished_data().to_vec()
}

/// Where the DELTA envelope lands, gitignored beside the others for the same reason.
pub fn delta_fixture_path() -> PathBuf {
    fixture_dir().join("snapshot_delta_envelope.bin")
}

/// Where the SECOND delta lands — the one applied to the first delta's OUTPUT.
pub fn delta2_fixture_path() -> PathBuf {
    fixture_dir().join("snapshot_delta2_envelope.bin")
}

/// Where the PARTY-ARRIVAL delta lands — the one that appends a player band and its detached party.
pub fn party_delta_fixture_path() -> PathBuf {
    fixture_dir().join("snapshot_party_delta_envelope.bin")
}

/// Where the PARTY-REMOVAL delta lands — the one that names that party in `removedPopulations`.
pub fn party_removal_delta_fixture_path() -> PathBuf {
    fixture_dir().join("snapshot_party_removal_delta_envelope.bin")
}

fn fixture_dir() -> PathBuf {
    Path::new("clients")
        .join("godot_thin_client")
        .join("tests")
        .join("fixtures")
}

/// Which rows one fixture delta moves.
///
/// **The two plans below are DISJOINT, and that is the entire point of having two.** The chained
/// fixture exists to catch a merge that re-bases each delta on the ORIGINAL baseline instead of the
/// running cache — and if delta 2 rewrote the same rows as delta 1, losing delta 1 would leave no
/// trace: delta 2's values would be correct either way. Only a row that delta 1 moved and delta 2
/// did not can testify that delta 1 survived.
struct DeltaPlan {
    /// First baseline tile index this delta moves, and how many consecutive tiles.
    first_tile: usize,
    tile_count: usize,
    /// Offset within those tiles of the ONE that also moves its river + culture-layer fields.
    ///
    /// The decoder derives `tiles.rivers` / `tiles.culture_layer` for the change manifest by
    /// comparing each changed tile against the entry it replaces, and one-of-several is what
    /// exercises that comparison in **both** directions: the other changed tiles move only their
    /// readings, so a comparison that answered "changed" for any moved tile would be
    /// indistinguishable from a correct one.
    splatmap_offset: usize,
    /// The `populations` / `culture_layers` row this delta moves — one of the fixture's two, so the
    /// other stays untouched and the merged frame can be checked for keeping it.
    row: usize,
    /// How many appended `command_events` rows precede this delta's, so the two deltas' sequences
    /// are disjoint and CHAINED — delta 2 continues delta 1's numbering, exactly as the sim's
    /// monotonic log does.
    command_event_offset: u64,
    /// The retention window this delta reports. Different on the two deltas so the merged frame
    /// proves the scalar is actually carried and overwritten, not defaulted.
    retention_turns: u32,
}

#[cfg(test)]
impl DeltaPlan {
    /// Does this plan move any of the same rows as `other`? Only the CI test asks — it is the
    /// assertion that keeps the chained fixture from going vacuous, and nothing in the build path
    /// needs to know.
    fn overlaps(&self, other: &DeltaPlan) -> bool {
        let tiles_overlap = self.first_tile < other.first_tile + other.tile_count
            && other.first_tile < self.first_tile + self.tile_count;
        tiles_overlap || self.row == other.row
    }
}

/// Delta 1: the first three tiles, the first row of each keyed section.
const DELTA_ONE: DeltaPlan = DeltaPlan {
    first_tile: 0,
    tile_count: 3,
    splatmap_offset: 2,
    row: 0,
    command_event_offset: 0,
    retention_turns: 20,
};

/// Delta 2: the NEXT three tiles and the OTHER row — disjoint from [`DELTA_ONE`] in every section.
const DELTA_TWO: DeltaPlan = DeltaPlan {
    first_tile: DELTA_ONE.first_tile + DELTA_ONE.tile_count,
    tile_count: 3,
    splatmap_offset: 0,
    row: DELTA_ONE.row + 1,
    command_event_offset: DELTA_COMMAND_EVENT_ROWS,
    retention_turns: 24,
};

/// How far a delta moves each changed tile's graze/forage reading. Applied as an OFFSET from the
/// baseline value rather than as an absolute, so the moved value cannot accidentally coincide with
/// the saturated one the baseline carries — the guard's "this is not the baseline's value" check
/// would then pass while proving nothing.
const DELTA_BIOMASS_STEP: f32 = 100.0;

/// The same idea for `temperature`, which rides the wire as fixed-point (1e6) — a whole 5 °C, so a
/// dropped `fixed64_to_f64` divide is still visible in the guard's output.
const DELTA_TEMPERATURE_STEP: i64 = 5_000_000;

/// One Minor-river class bit on direction 0 of the packed `river_edges` mask (2 bits per odd-r
/// direction — see `tile_to_dict`). XORed in, so the mask provably differs whatever it was.
const DELTA_RIVER_EDGE_BIT: u16 = 0b01;

/// How far a delta moves the probe field on the `populations` / `culture_layers` row it carries.
/// An OFFSET, for the same reason as [`DELTA_BIOMASS_STEP`].
const DELTA_COUNT_STEP: u32 = 7;

/// How many newly-appended `command_events` rows each delta carries.
///
/// **This section's delta is APPEND-only** (`core_sim::snapshot::diff_appended`) — it ships the
/// rows whose `seq` is above the client's cursor, never the retained ring — so the fixture models
/// what the sim really sends: a couple of fresh rows whose `seq` sits above every row the baseline
/// holds. A decoder that *replaced* the section instead of appending, or dropped `seq`, is exactly
/// what this makes visible. (`assert_no_empty_arrays` also refuses an empty repeated field, so the
/// count cannot be zero.)
const DELTA_COMMAND_EVENT_ROWS: u64 = 2;

/// Which WHOLE-SECTION witnesses a delta restates.
///
/// **Delta 1 carries them and delta 2 does not, and that asymmetry covers a hole the keyed sections
/// cannot.** A keyed section's base key is republished every frame out of `SectionCaches`, so it
/// survives even a merge that re-bases the frame DICTIONARY on the original baseline. A
/// whole-section field does not: it lands in the merged dict once, when its delta carries it, and
/// stays only because the next delta merges into the frame before it. So these are the only
/// witnesses here that can testify about `decode_frame`'s `cache.dict.duplicate_shallow()` —
/// measured, not assumed: mutating that line while only the keyed sections were probed left the
/// guard PASSING.
///
/// The two are carried together rather than as two parallel `[bool; 2]` arrays so a delta's
/// witnesses cannot drift apart by a mis-indexed subscript at the call site.
#[derive(Clone, Copy)]
struct WholeSectionWitnesses {
    /// `demographics` — a whole-section VECTOR, replaced wholesale when carried.
    demographics: bool,
    /// `equipment_config_json` — a whole-section STRING, and the only one of the kit roster's four
    /// whole-section fields any fixture delta carries. It is a *different shape* of witness from
    /// `demographics` on purpose: the vector rides its own `Option<Vec<_>>` through a section
    /// converter, while this one is a bare `Option<String>` the decoder republishes opaquely, so a
    /// delta path that handled repeated fields and forgot the scalars would still be caught.
    equipment_config_json: bool,
}

/// Delta 1 states both witnesses; delta 2 states neither. See [`WholeSectionWitnesses`].
const DELTA_WHOLE_SECTION_WITNESSES: [WholeSectionWitnesses; 2] = [
    WholeSectionWitnesses {
        demographics: true,
        equipment_config_json: true,
    },
    WholeSectionWitnesses {
        demographics: false,
        equipment_config_json: false,
    },
];

/// What delta 1 restates `equipment_config_json` as.
///
/// **It must not be the baseline's value**, which saturation sets to the field's own path
/// (`"equipment_config_json"`): a decoder that ignored the delta and republished the baseline would
/// otherwise pass the guard's "the merged frame carries the delta's config" assertion while proving
/// nothing. Shaped as a small JSON object rather than another bare path sentinel so the guard also
/// sees the braces and quotes survive verbatim — the field is contractually **opaque** to this
/// decoder (`native-extension.md` → THE KIT ROSTER), and a value that re-serialized or re-escaped on
/// the way through would be visible in the merged frame rather than silent.
const DELTA_EQUIPMENT_CONFIG_JSON: &str = r#"{"fixture":"delta.equipment_config_json"}"#;

/// Writes the **DELTA** envelopes built against the same synthetic world [`write_fixture`] emits:
/// one applied to the baseline, and one applied to THAT delta's output.
///
/// The delta path had no fixture at all, and that is exactly why it shipped a world whose `tiles`
/// array never moved after the baseline snapshot: the decoder inserted the sparse `tile_updates`
/// list and left `tiles` standing, so every per-tile lookup in `MapView` was frozen for the life of
/// the world. Nothing in `cargo xtask decode-guard` could see it, because nothing decoded a delta.
///
/// **A second delta is not redundant with the first, and it was added after the fact because that
/// was mis-triaged once.** One delta only ever exercises baseline → delta; the client takes
/// delta → delta on every turn after the first, and the merge re-bases on the running cache
/// (`decode_frame`'s `cache.dict.duplicate_shallow()`, where `cache` is replaced after each merge).
/// Re-base that on the ORIGINAL baseline instead and delta 2 silently discards delta 1's changes —
/// no error, no symptom, the world just drifts. The opposite failure is already loud: a cache that
/// fails to advance `frame_seq` makes delta 2 unapplicable, which fires `resync_needed`.
///
/// Both are deliberately **sparse** — a header and a few rows and nothing else. That is what lets
/// the guard assert the change manifest does not name a section a delta left alone
/// (`forage_patches`), which a saturated everything-present delta could never show.
pub fn write_delta_fixtures() -> Result<(), Box<dyn Error>> {
    let snapshot = saturated_snapshot()?;
    let first = build_fixture_delta(&snapshot);
    write_delta(&first, delta_fixture_path(), "delta")?;
    write_delta(
        &build_fixture_delta2(&snapshot, &first),
        delta2_fixture_path(),
        "chained delta",
    )?;
    let arrival = build_fixture_party_delta(&snapshot);
    write_delta(&arrival, party_delta_fixture_path(), "party arrival delta")?;
    write_delta(
        &build_fixture_party_removal_delta(&arrival),
        party_removal_delta_fixture_path(),
        "party removal delta",
    )
}

fn write_delta(delta: &WorldDelta, path: PathBuf, label: &str) -> Result<(), Box<dyn Error>> {
    let bytes = encode_delta_flatbuffer(delta);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &bytes)?;
    println!(
        "Wrote {} decode fixture to {} ({} bytes; frame {} on base {}; {} tiles, {} populations, {} culture layers changed)",
        label,
        path.display(),
        bytes.len(),
        delta.header.frame_seq,
        delta.header.base_frame_seq,
        delta.tiles.len(),
        delta.populations.len(),
        delta.culture_layers.len()
    );
    Ok(())
}

/// The delta the guard applies to [`saturated_snapshot`]'s world.
///
/// `..Default::default()` here is the opposite call from the exhaustive blanks above, and on
/// purpose: a delta's meaning is *what it carries*, so leaving a section at its `None`/empty
/// default is the fixture stating that the section did not change.
pub fn build_fixture_delta(snapshot: &WorldSnapshot) -> WorldDelta {
    let mut header = snapshot.header.clone();
    header.tick = snapshot.header.tick + 1;
    // The gate the client applies before merging: same world, and a base it actually holds.
    header.base_frame_seq = snapshot.header.frame_seq;
    header.frame_seq = snapshot.header.frame_seq + 1;
    build_planned_delta(
        snapshot,
        header,
        &DELTA_ONE,
        DELTA_WHOLE_SECTION_WITNESSES[0],
    )
}

/// The delta the guard applies to the FIRST delta's merged output.
///
/// Its rows are taken from the baseline rather than from delta 1's, which is sound precisely
/// because the plans are disjoint: [`DELTA_TWO`] moves rows delta 1 never touched, so their
/// pre-delta-2 state IS the baseline's.
pub fn build_fixture_delta2(snapshot: &WorldSnapshot, first: &WorldDelta) -> WorldDelta {
    let mut header = snapshot.header.clone();
    header.tick = first.header.tick + 1;
    // Chained: this one applies to the frame delta 1 PUBLISHED, not to the baseline.
    header.base_frame_seq = first.header.frame_seq;
    header.frame_seq = first.header.frame_seq + 1;
    build_planned_delta(
        snapshot,
        header,
        &DELTA_TWO,
        DELTA_WHOLE_SECTION_WITNESSES[1],
    )
}

fn build_planned_delta(
    snapshot: &WorldSnapshot,
    header: SnapshotHeader,
    plan: &DeltaPlan,
    witnesses: WholeSectionWitnesses,
) -> WorldDelta {
    let tiles = snapshot
        .tiles
        .iter()
        .skip(plan.first_tile)
        .take(plan.tile_count)
        .enumerate()
        .map(|(i, tile)| {
            let mut moved = tile.clone();
            moved.graze_biomass += DELTA_BIOMASS_STEP;
            moved.forage_capacity += DELTA_BIOMASS_STEP;
            moved.temperature += DELTA_TEMPERATURE_STEP;
            if i == plan.splatmap_offset {
                moved.river_edges ^= DELTA_RIVER_EDGE_BIT;
                moved.culture_layer = moved.culture_layer.wrapping_add(1);
            }
            moved
        })
        .collect();

    // The two non-tile keyed sections with live consumers reading the BASE key. `size` and
    // `parent` are arbitrary probes — what matters is that each moves on exactly one row.
    let populations = snapshot
        .populations
        .iter()
        .skip(plan.row)
        .take(1)
        .map(|cohort| {
            let mut moved = cohort.clone();
            moved.size = moved.size.wrapping_add(DELTA_COUNT_STEP);
            moved
        })
        .collect();
    let culture_layers = snapshot
        .culture_layers
        .iter()
        .skip(plan.row)
        .take(1)
        .map(|layer| {
            let mut moved = layer.clone();
            moved.parent = moved.parent.wrapping_add(DELTA_COUNT_STEP);
            moved
        })
        .collect();

    // The whole-section witnesses: replaced wholesale when carried, absent (= unchanged) when not.
    // See `WholeSectionWitnesses` for what their asymmetry across the two deltas proves.
    let demographics = witnesses.demographics.then(|| {
        snapshot
            .demographics
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let mut moved = entry.clone();
                if i == 0 {
                    moved.children = moved.children.wrapping_add(DELTA_COUNT_STEP);
                }
                moved
            })
            .collect()
    });
    let equipment_config_json = witnesses
        .equipment_config_json
        .then(|| DELTA_EQUIPMENT_CONFIG_JSON.to_string());

    // The append-only section: rows the baseline has never seen, numbered above every seq it holds.
    // The two deltas are disjoint here as everywhere else — delta 2 continues delta 1's numbering.
    let first_new_seq = snapshot
        .command_events
        .iter()
        .map(|event| event.seq)
        .max()
        .unwrap_or(0)
        + 1
        + plan.command_event_offset;
    let command_events = Some(
        (0..DELTA_COMMAND_EVENT_ROWS)
            .map(|i| {
                let mut appended = snapshot.command_events.first().cloned().unwrap_or_default();
                appended.seq = first_new_seq + i;
                appended.tick = header.tick;
                appended.label = format!("{}#{}", appended.label, appended.seq);
                appended
            })
            .collect(),
    );

    WorldDelta {
        header,
        tiles,
        populations,
        culture_layers,
        demographics,
        equipment_config_json,
        command_events,
        command_events_retention_turns: Some(plan.retention_turns),
        // Carried on every delta rather than diffed (see `WorldDelta::fog_enabled`); the derived
        // `Default` says `false`, which would silently flip the merged world's fog.
        fog_enabled: snapshot.fog_enabled,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// The PARTY-REMOVAL pair (`tools/party_removal_guard.gd`)
// ---------------------------------------------------------------------------
//
// A party that a `send_hunt_expedition` spawned and an in-camp `recall_expedition` despawned inside
// one tick used to be published on a HELD frame and then never retracted, so the client carried a
// ghost row the sim refused every order for. The sim sends the removal now; these two deltas are
// what let the client half be ASSERTED rather than read — arrival on one frame, removal on the
// next, which is the two-frame shape the live bug wore.
//
// The rows are APPENDED by a delta rather than seeded into the baseline snapshot on purpose. The
// saturated baseline's cohorts carry a path-hashed `faction` (always ≥ 1, see `saturate`), so the
// client's player-faction filter never sees them and the golden the baseline is diffed against does
// not move; a delta appending rows the baseline never held is also the very `SectionCache::patch`
// branch a spawned party takes in play.

/// The player faction. Mirrors the client's `HudConst.PLAYER_FACTION_ID` — the roster split in
/// `HudLayer.update_band_alerts` drops every cohort that is not this faction, so a party staged at
/// any other value would never reach the panel and every assertion downstream would be vacuous.
const PARTY_FIXTURE_FACTION: u32 = 0;

/// The home band's ECS entity and its durable `band_id`. **Deliberately different values**, the
/// `command_guard` rule: a fixture whose two handles agree cannot show a surface reading the wrong
/// one. Numbered far above the baseline's `100 + i` cohorts so an appended row can never collide.
const PARTY_FIXTURE_BAND_ENTITY: u64 = 9001;
const PARTY_FIXTURE_BAND_ID: u64 = 79001;

/// The detached party's pair — this entity is what the removal delta names in `removedPopulations`,
/// and what the guard hunts for afterwards in the panel, the map and the selection.
const PARTY_FIXTURE_PARTY_ENTITY: u64 = 9002;
const PARTY_FIXTURE_PARTY_ID: u64 = 79002;

/// Where the two stand. Both inside the 4×3 fixture grid, and **different tiles**: co-location is
/// what the client's `party_cancels_in_camp` reads, so a party sharing its home band's hex would
/// quietly change which recall verb the row renders and make the fixture about something else.
const PARTY_FIXTURE_BAND_X: u32 = 1;
const PARTY_FIXTURE_BAND_Y: u32 = 1;
const PARTY_FIXTURE_PARTY_X: u32 = 2;
const PARTY_FIXTURE_PARTY_Y: u32 = 1;

/// The band's head-count, its whole assignable workers, and how many of those are unassigned. The
/// idle count is what the parties zone's footer gates its Scout/Hunt buttons on, so it is non-zero.
const PARTY_FIXTURE_BAND_SIZE: u32 = 30;
const PARTY_FIXTURE_BAND_WORKING_AGE: u32 = 16;
const PARTY_FIXTURE_BAND_IDLE_WORKERS: u32 = 6;

/// The party's head-count — the number the parties header's `n out · m workers` clause states, so
/// it is distinct from every other count in the fixture and cannot be matched by accident.
const PARTY_FIXTURE_PARTY_SIZE: u32 = 4;

/// `Scalar::SCALE` = 1.0 output. The derived `Default` says `0`, i.e. a band that produces nothing —
/// legal on the wire and misleading in a panel.
const PARTY_FIXTURE_OUTPUT_MULTIPLIER: i64 = 1_000_000;

/// The herd the party is hunting. A string id, matching `HerdRegistry`'s fauna ids — the panel row
/// resolves it against the (absent) herd roster and falls back to the id, which is fine: what the
/// guard reads off the row is the party's identity, not its quarry's label.
const PARTY_FIXTURE_TARGET_HERD: &str = "game_boar_04";

/// The delta that brings the player band and its detached party into the world.
///
/// Applies to [`saturated_snapshot`]'s baseline, exactly like [`build_fixture_delta`] — it is an
/// alternative first frame, not a continuation, so the two never appear in the same chain.
pub fn build_fixture_party_delta(snapshot: &WorldSnapshot) -> WorldDelta {
    let mut header = snapshot.header.clone();
    header.tick = snapshot.header.tick + 1;
    header.base_frame_seq = snapshot.header.frame_seq;
    header.frame_seq = snapshot.header.frame_seq + 1;
    WorldDelta {
        header,
        populations: vec![party_fixture_band(), party_fixture_party()],
        // Carried on every delta rather than diffed, same as `build_planned_delta`.
        fog_enabled: snapshot.fog_enabled,
        ..Default::default()
    }
}

/// The delta that RETRACTS the party — the frame the sim now sends and the client has never been
/// asserted against.
///
/// It carries the removal and **nothing else**: no `populations` rows at all. That is the honest
/// shape (the sim's `diff_removed` emits exactly this) and it is also the sharper fixture — a
/// consumer that rebuilt its roster from the sparse `population_updates` list rather than from the
/// merged `populations` array would see an empty update list and leave the ghost standing.
pub fn build_fixture_party_removal_delta(arrival: &WorldDelta) -> WorldDelta {
    let mut header = arrival.header.clone();
    header.tick = arrival.header.tick + 1;
    header.base_frame_seq = arrival.header.frame_seq;
    header.frame_seq = arrival.header.frame_seq + 1;
    WorldDelta {
        header,
        removed_populations: vec![PARTY_FIXTURE_PARTY_ENTITY],
        fog_enabled: arrival.fog_enabled,
        ..Default::default()
    }
}

/// The resident band the party is homed on. Sparse by design: only the fields the parties zone, the
/// map marker and the selection card actually read.
fn party_fixture_band() -> PopulationCohortState {
    PopulationCohortState {
        entity: PARTY_FIXTURE_BAND_ENTITY,
        band_id: PARTY_FIXTURE_BAND_ID,
        faction: PARTY_FIXTURE_FACTION,
        size: PARTY_FIXTURE_BAND_SIZE,
        current_x: PARTY_FIXTURE_BAND_X,
        current_y: PARTY_FIXTURE_BAND_Y,
        working_age: PARTY_FIXTURE_BAND_WORKING_AGE,
        idle_workers: PARTY_FIXTURE_BAND_IDLE_WORKERS,
        output_multiplier: PARTY_FIXTURE_OUTPUT_MULTIPLIER,
        ..Default::default()
    }
}

/// The detached hunting party, grouped under the band above by `home_band_entity` — which is the
/// key `HudBandLaborState.band_parties` groups on and therefore the field that puts the row in the
/// parties zone at all.
fn party_fixture_party() -> PopulationCohortState {
    PopulationCohortState {
        entity: PARTY_FIXTURE_PARTY_ENTITY,
        band_id: PARTY_FIXTURE_PARTY_ID,
        faction: PARTY_FIXTURE_FACTION,
        size: PARTY_FIXTURE_PARTY_SIZE,
        current_x: PARTY_FIXTURE_PARTY_X,
        current_y: PARTY_FIXTURE_PARTY_Y,
        is_expedition: true,
        expedition_mission: "hunt".to_string(),
        expedition_phase: "hunting".to_string(),
        expedition_target_herd: PARTY_FIXTURE_TARGET_HERD.to_string(),
        home_band_entity: PARTY_FIXTURE_BAND_ENTITY,
        output_multiplier: PARTY_FIXTURE_OUTPUT_MULTIPLIER,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The headerless fixture must stay **verifiable AND header-less** — that pair IS the case
    /// under test, and it reaches CI where the Godot-side assertion cannot.
    ///
    /// Both halves can rot silently. If `header` ever gained a `required` attribute (or the builder
    /// started emitting one) the guard's assertion would keep passing, vacuously, against a fixture
    /// that no longer reproduces the bug; and if the map section were dropped, the guard could no
    /// longer distinguish "the frame was dropped" from "the snapshot was empty anyway".
    #[test]
    fn the_headerless_fixture_verifies_and_carries_no_header() {
        let bytes = encode_headerless_envelope();
        let envelope = fb::root_as_envelope(&bytes).expect(
            "the headerless envelope must still VERIFY — an unparseable one proves nothing",
        );
        assert_eq!(envelope.payload_type(), fb::SnapshotPayload::snapshot);
        let snapshot = envelope
            .payload_as_snapshot()
            .expect("payload reads back as a WorldSnapshot");
        assert!(
            snapshot.header().is_none(),
            "the fixture must carry NO header — that absence is the whole case under test"
        );
        assert!(
            snapshot.map().is_some(),
            "the fixture must carry real content, so a dropped frame is distinguishable from an \
             empty one"
        );
    }

    /// The delta fixtures must stay **applicable** — the first to the snapshot, the second to the
    /// first — must stay **sparse**, must be **disjoint**, and must actually MOVE every row the
    /// guard probes. This is the half of the delta gate that reaches CI, where the Godot-side merge
    /// assertions cannot.
    ///
    /// Every clause rots silently. A base sequence that stopped naming its predecessor's frame
    /// would make the client DROP the frame, and the guard would then be asserting against an empty
    /// dictionary; rows that stopped moving would let a decoder that ignores a delta's rows
    /// entirely pass; and **two deltas that stopped being disjoint would make the whole chained
    /// fixture vacuous** — the cumulative assertion can only fail if delta 2 leaves delta 1's rows
    /// alone.
    #[test]
    fn the_delta_fixtures_chain_and_move_disjoint_rows() {
        let snapshot = saturated_snapshot().expect("snapshot fixture builds");
        let first = build_fixture_delta(&snapshot);
        let second = build_fixture_delta2(&snapshot, &first);

        assert!(
            !DELTA_ONE.overlaps(&DELTA_TWO),
            "the two delta plans must move DISJOINT rows, or 'delta 1's values survived delta 2' \
             cannot fail"
        );

        assert_eq!(
            first.header.base_frame_seq, snapshot.header.frame_seq,
            "delta 1 must name the snapshot's frame as its base, or the client drops it"
        );
        assert_eq!(
            second.header.base_frame_seq, first.header.frame_seq,
            "delta 2 must name DELTA 1's frame as its base — that chaining is the whole point of \
             the second fixture; based on the snapshot it would only re-test baseline → delta"
        );
        assert_eq!(
            second.header.frame_seq,
            first.header.frame_seq + 1,
            "delta 2 must publish the frame after delta 1"
        );
        for delta in [&first, &second] {
            assert_eq!(
                delta.header.world_epoch, snapshot.header.world_epoch,
                "a delta from another world is never merged"
            );
            assert!(
                delta.tiles.len() < snapshot.tiles.len(),
                "each delta must be SPARSE ({} of {} tiles), or 'the merged frame kept the \
                 baseline's row count' proves nothing",
                delta.tiles.len(),
                snapshot.tiles.len()
            );
            assert_eq!(delta.populations.len(), 1);
            assert_eq!(delta.culture_layers.len(), 1);
        }

        // Disjointness restated on the ENCODED rows, not just the plans: the plans are what the
        // builder reads, these are what the guard actually sees.
        let first_tiles: HashSet<u64> = first.tiles.iter().map(|t| t.entity).collect();
        let second_tiles: HashSet<u64> = second.tiles.iter().map(|t| t.entity).collect();
        assert!(
            first_tiles.is_disjoint(&second_tiles),
            "the two deltas moved overlapping tiles ({first_tiles:?} vs {second_tiles:?})"
        );
        assert_ne!(
            first.populations[0].entity, second.populations[0].entity,
            "the two deltas must move DIFFERENT population rows"
        );
        assert_ne!(
            first.culture_layers[0].id, second.culture_layers[0].id,
            "the two deltas must move DIFFERENT culture-layer rows"
        );

        // The whole-section witnesses: carried by delta 1 ONLY, and visibly moved, so that after the
        // second merge they can testify that the frame dictionary carried delta 1's values forward.
        let carried = first
            .demographics
            .as_ref()
            .expect("delta 1 must carry the whole-section witness");
        assert!(
            second.demographics.is_none(),
            "delta 2 must NOT carry `demographics` — absent means unchanged, and it is that absence \
             that makes the frame-dictionary assertion possible"
        );
        assert_ne!(
            snapshot.demographics[0].children, carried[0].children,
            "delta 1's `demographics` row must be visibly different from the baseline's"
        );

        // The SCALAR whole-section witness, same shape of claim. Both clauses go vacuous silently if
        // they rot — a delta-1 value that drifted back to the baseline's would let a decoder that
        // never reads `equipmentConfigJson` off a delta satisfy the guard, and a delta 2 that
        // started restating it would prove the merge re-published the value rather than kept it.
        let carried_config = first
            .equipment_config_json
            .as_deref()
            .expect("delta 1 must carry `equipment_config_json`");
        assert!(
            second.equipment_config_json.is_none(),
            "delta 2 must NOT carry `equipment_config_json` — it survives the second merge only \
             because that merge starts from delta 1's frame, which is the property under test"
        );
        assert_ne!(
            snapshot.equipment_config_json, carried_config,
            "delta 1's `equipment_config_json` must DIFFER from the baseline's, or a decoder that \
             ignored the delta and republished the baseline would pass"
        );
        let factions: HashSet<u32> = snapshot.demographics.iter().map(|d| d.faction).collect();
        assert_eq!(
            factions.len(),
            snapshot.demographics.len(),
            "the fixture's demographics rows must have DISTINCT factions — the guard indexes by \
             them, so a collision would probe the wrong row"
        );

        for delta in [&first, &second] {
            for moved in &delta.tiles {
                let baseline = snapshot
                    .tiles
                    .iter()
                    .find(|tile| tile.entity == moved.entity)
                    .expect("every changed tile exists in the baseline");
                assert_ne!(
                    baseline.graze_biomass, moved.graze_biomass,
                    "tile {} must carry a visibly different graze reading",
                    moved.entity
                );
                assert_ne!(
                    baseline.temperature, moved.temperature,
                    "tile {} must carry a visibly different temperature",
                    moved.entity
                );
            }
            let baseline_cohort = snapshot
                .populations
                .iter()
                .find(|cohort| cohort.entity == delta.populations[0].entity)
                .expect("the changed cohort exists in the baseline");
            assert_ne!(
                baseline_cohort.size, delta.populations[0].size,
                "the changed cohort must carry a visibly different size"
            );
            let baseline_layer = snapshot
                .culture_layers
                .iter()
                .find(|layer| layer.id == delta.culture_layers[0].id)
                .expect("the changed culture layer exists in the baseline");
            assert_ne!(
                baseline_layer.parent, delta.culture_layers[0].parent,
                "the changed culture layer must carry a visibly different parent"
            );
        }

        // One tile per delta moves the splatmap fields and the rest move only their readings —
        // which is what exercises the decoder's old-vs-new comparison in BOTH directions.
        for (delta, plan) in [(&first, &DELTA_ONE), (&second, &DELTA_TWO)] {
            for (i, moved) in delta.tiles.iter().enumerate() {
                let baseline = snapshot
                    .tiles
                    .iter()
                    .find(|tile| tile.entity == moved.entity)
                    .expect("every changed tile exists in the baseline");
                let splatmap_moved = (baseline.river_edges, baseline.culture_layer)
                    != (moved.river_edges, moved.culture_layer);
                assert_eq!(
                    splatmap_moved,
                    i == plan.splatmap_offset,
                    "tile {} must move its river/culture fields IFF it is the plan's splatmap \
                     tile, or `tiles.rivers` is exercised in only one direction",
                    moved.entity
                );
            }
        }

        // Restated on the ENCODED bytes, because the wire is what the guard decodes: an `Option`
        // that stopped reaching `SubsistenceSectionArgs` would leave every claim above intact.
        for (delta, expect_forage_absent, expect_config) in [
            (&first, true, Some(DELTA_EQUIPMENT_CONFIG_JSON)),
            (&second, true, None),
        ] {
            let bytes = encode_delta_flatbuffer(delta);
            let envelope = fb::root_as_envelope(&bytes)
                .expect("the delta envelope must verify as FlatBuffers");
            assert_eq!(envelope.payload_type(), fb::SnapshotPayload::delta);
            let subsistence = envelope
                .payload_as_delta()
                .and_then(|d| d.subsistence())
                .expect("a delta always carries a subsistence table");
            assert_eq!(
                subsistence.foragePatches().is_none(),
                expect_forage_absent,
                "the delta must leave `forage_patches` ABSENT — the guard asserts the change \
                 manifest does not name it"
            );
            assert_eq!(
                subsistence.equipmentConfigJson(),
                expect_config,
                "the delta's `equipmentConfigJson` must reach the WIRE exactly as planned — \
                 present and verbatim on delta 1, absent on delta 2"
            );
        }
    }

    /// The PARTY-REMOVAL pair must stay **applicable**, must stay **staged**, and must stay **the
    /// only player-faction cohorts in the world** — the half of `tools/party_removal_guard.gd` that
    /// reaches CI, where the Godot-side panel assertions cannot.
    ///
    /// Every clause rots into a VACUOUS guard rather than a failing one, which is why each is
    /// asserted here rather than left to the harness:
    ///
    /// - a base sequence that stopped naming its predecessor makes the client DROP the frame, and
    ///   every panel assertion downstream would then be made against a party that never arrived;
    /// - a removal list that stopped naming the party would leave the guard asserting that a row
    ///   nobody asked to remove is still present — a green run proving nothing;
    /// - the removal delta must carry **no `populations` rows at all**, because that is the sim's
    ///   own shape *and* the sharper fixture: a consumer rebuilding its roster from the sparse
    ///   `population_updates` list rather than from the merged array would see an empty list and
    ///   leave the ghost standing;
    /// - and the baseline must contribute **no** player-faction cohort of its own, or the harness's
    ///   "the home band survived" assertion could be satisfied by a row this fixture never staged.
    #[test]
    fn the_party_fixtures_chain_and_stage_exactly_one_removable_party() {
        let snapshot = saturated_snapshot().expect("snapshot fixture builds");
        let arrival = build_fixture_party_delta(&snapshot);
        let removal = build_fixture_party_removal_delta(&arrival);

        assert_eq!(
            arrival.header.base_frame_seq, snapshot.header.frame_seq,
            "the arrival delta must name the snapshot's frame as its base, or the client drops it"
        );
        assert_eq!(
            removal.header.base_frame_seq, arrival.header.frame_seq,
            "the removal delta must name the ARRIVAL delta's frame as its base — it retracts a row \
             only that frame introduced"
        );
        for delta in [&arrival, &removal] {
            assert_eq!(
                delta.header.world_epoch, snapshot.header.world_epoch,
                "a delta from another world is never merged"
            );
        }

        assert_eq!(
            arrival.populations.len(),
            2,
            "the arrival delta stages exactly the home band and its party"
        );
        let party = arrival
            .populations
            .iter()
            .find(|cohort| cohort.entity == PARTY_FIXTURE_PARTY_ENTITY)
            .expect("the arrival delta must carry the party");
        assert!(
            party.is_expedition,
            "the party must be an EXPEDITION cohort"
        );
        assert_eq!(
            party.home_band_entity, PARTY_FIXTURE_BAND_ENTITY,
            "the party must be homed on the fixture's band — `band_parties` groups on exactly this"
        );

        assert!(
            removal.populations.is_empty(),
            "the removal delta must carry NO population rows — see this test's doc comment"
        );
        assert_eq!(
            removal.removed_populations,
            vec![PARTY_FIXTURE_PARTY_ENTITY],
            "the removal delta must name the party, and only the party"
        );

        assert!(
            snapshot
                .populations
                .iter()
                .all(|cohort| cohort.faction != PARTY_FIXTURE_FACTION),
            "the saturated baseline must contribute no player-faction cohort of its own, or the \
             guard's roster assertions could be satisfied by a row this fixture never staged"
        );
    }
}
