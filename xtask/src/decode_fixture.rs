//! The **client decode fixture** — a synthetic `WorldSnapshot` encoded to the exact FlatBuffers
//! envelope the Godot client reads off the snapshot socket.
//!
//! ## Why this exists
//!
//! The FlatBuffers → Godot `Dictionary` path in `clients/godot_thin_client/native/src/` had no
//! automated coverage: the `ui_preview` / `map_preview` PNG harnesses build hand-written GDScript
//! fixture dicts and hand them straight to `Hud`/`MapView`, so a fully green PNG run is compatible
//! with a completely broken decoder. `tools/decode_guard.gd` closes that by decoding a real
//! envelope through the real `SnapshotDecoder`; this module is where that envelope comes from.
//!
//! ## Why synthetic rather than captured from a running server
//!
//! A capture would be *realistic* but is the wrong instrument twice over. It is **sparse** — an
//! early-game world carries no crisis gauges, no great discoveries, no trade links, no influencers,
//! so most `dict/*` builders would go unexercised — and it is **unstable**, because this repo
//! retunes worldgen constantly (biome palette, climate, rivers), so a capture-derived golden would
//! churn on every tuning change and train reviewers to accept the diff blind. A synthetic snapshot
//! is deterministic, regenerable, and can make *every* section non-empty.
//!
//! ## How a value is chosen: saturation
//!
//! Hand-writing a literal for ~60 state structs would go stale the moment a field is appended — and
//! "the decoder silently dropped an appended field" is this codebase's most-repeated client bug. So
//! the fixture is built in two steps:
//!
//! 1. **Seed** — every `Vec` gets elements (this is the one thing serde cannot do for us: an empty
//!    array carries no element type). [`assert_no_empty_arrays`] fails the build, naming the JSON
//!    path, if a newly appended repeated field is left unseeded.
//! 2. **Saturate** — the snapshot is serialized to JSON and every *scalar leaf* is replaced with a
//!    deterministic, path-derived sentinel, then deserialized back. An appended scalar field is
//!    therefore populated automatically, forever, with no edit here.
//!
//! A **string** leaf becomes its own JSON path (`"populations[0].id"`), which is the highest-value
//! property of the whole scheme: the decoded dict then says, in plain text, which wire field landed
//! under which dictionary key, so an accessor wired to the wrong builder is legible in the golden
//! rather than merely different.
//!
//! Two rules keep saturation type-safe:
//! - A string leaf is replaced **only when its default is empty**. Rust `String` fields default to
//!   `""`; a serde-serialized enum defaults to a *variant name*, which is non-empty — so this one
//!   test distinguishes "free text" from "enum" without a field list.
//! - Integers stay in `1..=200` so a `u8` field cannot fail to deserialize. Distinctness comes from
//!   the string paths; the integers only need to be non-default.
//!
//! Structural fields (grid dimensions, raster lengths, tile coordinates, terrain enums) are set in
//! typed Rust *after* saturation, since a path-derived sentinel there would put a tile outside the
//! grid or a raster out of step with its width.

use flatbuffers::FlatBufferBuilder;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use sim_schema::codec::{encode_delta_flatbuffer, encode_snapshot_flatbuffer};
use sim_schema::state::campaign::*;
use sim_schema::state::culture::*;
use sim_schema::state::economy::*;
use sim_schema::state::governance::*;
use sim_schema::state::map::*;
use sim_schema::state::population::*;
use sim_schema::state::subsistence::{
    CharacteristicBandState, CraftKnowledgeState, MaterialDefState, RecipeDefState,
    RecipeInputState, RecipeOutputState,
};
use sim_schema::world::{SnapshotHeader, WorldDelta, WorldSnapshot};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// The fixture's grid. Deliberately tiny: the golden records rasters by summary rather than by
/// sample, but a small grid keeps the envelope a few KB and the failure output readable.
const GRID_W: u32 = 4;
const GRID_H: u32 = 3;
const GRID_CELLS: usize = (GRID_W * GRID_H) as usize;

/// Rows per repeated section. Two, not one: a per-row sentinel varies with the row index, so a
/// builder that returns the first element for every row (or reuses one offset) is visible.
const ROWS: usize = 2;

/// The length of the seeded `regrowthSamples` curve — the **shipped** sample count
/// (`core_sim::snapshot::REGROWTH_CURVE_SAMPLES`), restated here rather than imported because
/// `xtask` does not depend on `core_sim`. It only has to be a plausible non-empty length: saturation
/// overwrites every value, and the guard is about the field being *present and repeated*.
const REGROWTH_CURVE_SAMPLES: usize = 11;

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
    let snapshot = build_fixture_snapshot()?;
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
    let snapshot = build_fixture_snapshot()?;
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

/// The delta the guard applies to [`build_fixture_snapshot`]'s world.
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
/// Applies to [`build_fixture_snapshot`]'s baseline, exactly like [`build_fixture_delta`] — it is an
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

/// Seed → saturate → fix up. See the module docs for why it is done in that order.
pub fn build_fixture_snapshot() -> Result<WorldSnapshot, Box<dyn Error>> {
    let seeded = seed_snapshot();

    let mut value = serde_json::to_value(&seeded)?;
    assert_no_empty_arrays(&value, "")?;
    saturate(&mut value, "");

    let mut snapshot: WorldSnapshot = serde_json::from_value(value)?;
    apply_structural_fixups(&mut snapshot);
    Ok(snapshot)
}

// ---------------------------------------------------------------------------
// Step 1 — seed every repeated field
// ---------------------------------------------------------------------------

fn rows<T: Default + Clone>() -> Vec<T> {
    vec![T::default(); ROWS]
}

fn rows_of<T: Clone>(blank: T) -> Vec<T> {
    vec![blank; ROWS]
}

// ---------------------------------------------------------------------------
// Blanks for the state structs that do not derive `Default`
// ---------------------------------------------------------------------------
//
// The values here are placeholders — saturation overwrites every one of them. What matters is that
// each literal is **exhaustive** (no `..Default::default()`), so appending a field to one of these
// structs breaks *this* build and whoever appended it is told, at compile time, that the client
// decode gate needs to know about it. That is the same forcing function `assert_no_empty_arrays`
// provides for repeated fields, bought here for free because these structs have no `Default` to
// fall back on.

fn blank_tile() -> TileState {
    TileState {
        entity: 0,
        x: 0,
        y: 0,
        element: 0,
        mass: 0,
        temperature: 0,
        terrain: TerrainType::AlluvialPlain,
        terrain_tags: TerrainTags::empty(),
        culture_layer: 0,
        mountain_kind: MountainKind::Fold,
        mountain_relief: 0.0,
        habitability: 0,
        river_edges: 0,
        river_inflow: 0,
        river_channel: 0,
        graze_biomass: 0.0,
        graze_capacity: 0.0,
        graze_ecology_phase: 0,
        forage_capacity: 0.0,
        underlying_terrain: TerrainType::AlluvialPlain,
    }
}

fn blank_logistics_link() -> LogisticsLinkState {
    LogisticsLinkState {
        entity: 0,
        from: 0,
        to: 0,
        capacity: 0,
        flow: 0,
    }
}

fn blank_generation() -> GenerationState {
    GenerationState {
        id: 0,
        name: String::new(),
        bias_knowledge: 0,
        bias_trust: 0,
        bias_equity: 0,
        bias_agency: 0,
    }
}

fn blank_power_node() -> PowerNodeState {
    PowerNodeState {
        entity: 0,
        node_id: 0,
        generation: 0,
        demand: 0,
        efficiency: 0,
        storage_level: 0,
        storage_capacity: 0,
        stability: 0,
        surplus: 0,
        deficit: 0,
        incident_count: 0,
    }
}

fn blank_culture_trait() -> CultureTraitEntry {
    CultureTraitEntry {
        axis: CultureTraitAxis::OpenClosed,
        baseline: 0,
        modifier: 0,
        value: 0,
    }
}

fn blank_culture_layer() -> CultureLayerState {
    CultureLayerState {
        id: 0,
        owner: 0,
        parent: 0,
        scope: CultureLayerScope::Regional,
        traits: Vec::new(),
        divergence: 0,
        soft_threshold: 0,
        hard_threshold: 0,
        ticks_above_soft: 0,
        ticks_above_hard: 0,
        last_updated_tick: 0,
    }
}

fn blank_culture_tension() -> CultureTensionState {
    CultureTensionState {
        layer_id: 0,
        scope: CultureLayerScope::Local,
        owner: 0,
        severity: 0,
        timer: 0,
        kind: CultureTensionKind::SchismRisk,
    }
}

fn blank_culture_resonance() -> InfluencerCultureResonanceEntry {
    InfluencerCultureResonanceEntry {
        axis: CultureTraitAxis::SecularDevout,
        weight: 0,
        output: 0,
    }
}

fn blank_influencer() -> InfluentialIndividualState {
    InfluentialIndividualState {
        id: 0,
        name: String::new(),
        influence: 0,
        growth_rate: 0,
        baseline_growth: 0,
        notoriety: 0,
        sentiment_knowledge: 0,
        sentiment_trust: 0,
        sentiment_equity: 0,
        sentiment_agency: 0,
        sentiment_weight_knowledge: 0,
        sentiment_weight_trust: 0,
        sentiment_weight_equity: 0,
        sentiment_weight_agency: 0,
        logistics_bonus: 0,
        morale_bonus: 0,
        power_bonus: 0,
        logistics_weight: 0,
        morale_weight: 0,
        power_weight: 0,
        support_charge: 0,
        suppress_pressure: 0,
        domains: 0,
        scope: InfluenceScopeKind::Generation,
        generation_scope: 0,
        supported: false,
        suppressed: false,
        lifecycle: InfluenceLifecycle::Active,
        coherence: 0,
        ticks_in_status: 0,
        audience_generations: Vec::new(),
        support_popular: 0,
        support_peer: 0,
        support_institutional: 0,
        support_humanitarian: 0,
        weight_popular: 0,
        weight_peer: 0,
        weight_institutional: 0,
        weight_humanitarian: 0,
        culture_resonance: Vec::new(),
    }
}

/// Every `Vec` on the snapshot gets elements. Rasters are sized to the grid rather than to [`ROWS`]
/// — their length is structural, not decorative.
///
/// If you append a repeated field and forget it here, [`assert_no_empty_arrays`] names its path.
fn seed_snapshot() -> WorldSnapshot {
    // `WorldSnapshot`'s derived `Default` gives `false` for `fog_enabled` while the schema default is
    // `true`, so state it explicitly rather than letting the fixture claim fog of war is off.
    let mut s = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };

    // Every field is `Option` + `skip_serializing_if`, so a `None` never reaches the JSON and
    // saturation cannot reach it — and `create_campaign_label` drops an all-`None` label outright,
    // which is how the whole section went missing from the first recorded golden.
    s.header.campaign_label = Some(CampaignLabel {
        profile_id: Some(String::new()),
        title: Some(String::new()),
        title_loc_key: Some(String::new()),
        subtitle: Some(String::new()),
        subtitle_loc_key: Some(String::new()),
    });

    // --- map -------------------------------------------------------------
    s.tiles = vec![blank_tile(); GRID_CELLS];
    s.terrain.samples = vec![TerrainSample::default(); GRID_CELLS];
    s.elevation_overlay.samples = vec![0u16; GRID_CELLS];
    s.moisture_raster.samples = vec![0.0f32; GRID_CELLS];
    s.start_marker = Some(StartMarkerState::default());

    // --- vision / scalar rasters -----------------------------------------
    for raster in [
        &mut s.logistics_raster,
        &mut s.sentiment_raster,
        &mut s.corruption_raster,
        &mut s.culture_raster,
        &mut s.military_raster,
        &mut s.visibility_raster,
    ] {
        raster.samples = vec![0i64; GRID_CELLS];
    }

    // --- economy ---------------------------------------------------------
    s.logistics = rows_of(blank_logistics_link());
    s.trade_links = rows();
    for link in &mut s.trade_links {
        link.pending_fragments = rows();
    }
    s.faction_inventory = rows();
    for inv in &mut s.faction_inventory {
        inv.inventory = rows();
    }

    // --- population ------------------------------------------------------
    s.populations = rows();
    for cohort in &mut s.populations {
        cohort.stores = rows();
        // **The TOE, one row per item.** `rows()` would give every row the same default id, and a
        // list keyed by `item_id` with duplicate keys is not a thing the server can emit — so the
        // ids are spelled out. They are the shipped item table, which is what a client reading this
        // golden will actually be handed.
        cohort.kit_item_conditions = ["spears", "sled", "baskets", "traps"]
            .iter()
            .enumerate()
            .map(|(index, item)| KitItemConditionState {
                item_id: (*item).to_string(),
                // Distinct per row, and one of them DRY, so the golden exercises both sides of the
                // cliff rather than recording four healthy numbers.
                remaining: if index == 1 {
                    0.0
                } else {
                    90.0 - index as f32 * 7.5
                },
                // **Ownership, stated.** The dry row above owns none — `remaining 0` means "owns
                // none" since the count slice — so the golden carries both readings of a zero.
                count: if index == 1 { 0 } else { 1 },
            })
            .collect();
        // **The per-band resolved tiers**, one row per shipped kit. Spelled out for the same reason
        // the item conditions above are: a list keyed by `kit_id` cannot carry duplicate keys, which
        // is what `rows()` would produce. The values are inert here — saturation overwrites them —
        // but the row COUNT and the ids are what a client decodes against.
        cohort.kit_tiers = ["big_game", "trapping", "gathering", "none"]
            .iter()
            .map(|kit| BandKitTiersState {
                kit_id: (*kit).to_string(),
                ..Default::default()
            })
            .collect();
        // --- CRAFTING & MATERIALS ------------------------------------------------------------
        // Every one of these is a repeated field inside a repeated field, so both levels need
        // elements or the guard never exercises the inner ones. Saturation overwrites the values;
        // what matters here is the SHAPE and the ids, which cannot be `rows()` for the same reason
        // the item conditions above cannot: a list keyed by id has no duplicate keys.
        cohort.material_batches = ["hide", "fibre", "bone"]
            .iter()
            .map(|material| MaterialBatchState {
                material_id: (*material).to_string(),
                readings: vec![CharacteristicReadingState::default(); 2],
                ..Default::default()
            })
            .collect();
        cohort.bench = BenchState {
            shortfalls: rows_of(MaterialShortfallState::default()),
            ..Default::default()
        };
        // **One row per recipe, always** — the contract this field exists for, so the fixture
        // carries a real book's worth rather than one row.
        cohort.craft_offers = ["sled", "baskets", "spears", "loom"]
            .iter()
            .map(|recipe| CraftOfferState {
                recipe_id: (*recipe).to_string(),
                shortfalls: rows_of(MaterialShortfallState::default()),
                ..Default::default()
            })
            .collect();
        cohort.equipment_batches = ["spears", "sled", "baskets", "loom"]
            .iter()
            .map(|item| EquipmentBatchState {
                item_id: (*item).to_string(),
                ..Default::default()
            })
            .collect();
        cohort.labor_assignments = rows();
        for assignment in &mut cohort.labor_assignments {
            assignment.arrival_schedule = vec![0.0f32; 4];
        }
        cohort.pending_reveal_x = vec![0u32; ROWS];
        cohort.pending_reveal_y = vec![0u32; ROWS];
        cohort.knowledge_fragments = rows();
        cohort.migration = Some(PendingMigrationState {
            fragments: rows(),
            ..Default::default()
        });
        cohort.harvest_task = Some(HarvestTaskState::default());
        cohort.scout_task = Some(ScoutTaskState::default());
        cohort.accessible_stockpile = Some(AccessibleStockpileState {
            entries: rows(),
            ..Default::default()
        });
    }
    s.generations = rows_of(blank_generation());
    s.demographics = rows();

    // --- subsistence -----------------------------------------------------
    s.herds = rows();
    for herd in &mut s.herds {
        // The two pre-launch estimate tables that used to be seeded here are retired: the client
        // asks for a forecast now (`sim_runtime`'s `QueryCommand`) instead of reading a table the
        // capture pre-computed for every herd on every frame.
        // The sampled regrowth curve — a `[float]`, so it needs seeding like every other repeated
        // field or the decode guard cannot see it. Saturation overwrites the values; only the LENGTH
        // matters here, and it is the shipped one so the fixture exercises a real-shaped curve.
        herd.regrowth_samples = vec![0.0; REGROWTH_CURVE_SAMPLES];
    }
    s.food_modules = rows();
    // **The kit roster**, and each entry's `jobs` / `item_ids` — repeated fields inside a repeated
    // field, so both levels need elements or the decode guard never exercises the inner ones.
    s.kits = rows();
    for option in &mut s.kits {
        option.jobs = vec![String::new(); ROWS];
        // Which items the kit carries. Seeded for the reason every list here is: an empty one is
        // indistinguishable from a field that never reached the client, and this one exists
        // precisely so a durability readout stops guessing the gear from the tiers.
        option.item_ids = vec![String::new(); ROWS];
    }
    // The three per-world crafting catalogues and the learned one. `axes` / `requires_knowledge` /
    // `inputs` / `outputs` are repeated fields inside repeated fields, so each needs elements.
    s.materials = ["hide", "fibre", "bone"]
        .iter()
        .map(|material| MaterialDefState {
            id: (*material).to_string(),
            axes: vec![String::new(); 2],
            ..Default::default()
        })
        .collect();
    s.characteristic_bands = rows_of(CharacteristicBandState::default());
    s.recipes = ["sled", "baskets", "spears", "loom"]
        .iter()
        .map(|recipe| RecipeDefState {
            id: (*recipe).to_string(),
            requires_knowledge: vec![String::new(); 2],
            inputs: vec![RecipeInputState::default(); 2],
            outputs: vec![RecipeOutputState::default(); 1],
            ..Default::default()
        })
        .collect();
    s.craft_knowledge = ["tanning", "weaving", "bone_working"]
        .iter()
        .map(|craft| CraftKnowledgeState {
            craft_id: (*craft).to_string(),
            ..Default::default()
        })
        .collect();
    s.sedentarization = rows();
    s.forage_patches = rows();
    for patch in &mut s.forage_patches {
        patch.owner = Some(0);
        // Shared on the state struct (a tile's basket, not a frame's), so the fixture's rows are
        // handed over as one — the encoded bytes are identical either way.
        patch.composition = rows::<sim_schema::FloraShareInfo>().into();
        // The TILE's per-rung vector (#426) — the plant twin of `hunt_policy_ceilings` above, and
        // seeded for the same reason: a repeated field the fixture leaves empty is a field the decode
        // guard cannot exercise, which is how four appended fields reached the client as zeros.
        patch.regrowth_samples = vec![0.0; REGROWTH_CURVE_SAMPLES];
    }
    s.intensification_knowledge = rows();

    // --- knowledge -------------------------------------------------------
    s.discovered_sites = rows();
    for entry in &mut s.discovered_sites {
        entry.sites = rows();
    }
    s.discovery_progress = rows();
    s.great_discovery_definitions = rows();
    for def in &mut s.great_discovery_definitions {
        def.tier = Some(String::new());
        def.summary = Some(String::new());
        def.tags = vec![String::new(); ROWS];
        def.freshness_window = Some(0);
        def.effects_summary = vec![String::new(); ROWS];
        def.observation_notes = Some(String::new());
        def.leak_profile = Some(String::new());
        def.requirements = rows();
        for req in &mut def.requirements {
            req.name = Some(String::new());
            req.summary = Some(String::new());
        }
    }
    s.great_discoveries = rows();
    s.great_discovery_progress = rows();

    // --- governance ------------------------------------------------------
    s.power = rows_of(blank_power_node());
    s.power_metrics.incidents = rows();
    s.crisis_telemetry.gauges = rows();
    for gauge in &mut s.crisis_telemetry.gauges {
        gauge.history = rows();
    }
    s.crisis_overlay.heatmap.samples = vec![0i64; GRID_CELLS];
    s.crisis_overlay.annotations = rows();
    for annotation in &mut s.crisis_overlay.annotations {
        annotation.path = vec![0u32; 4];
    }
    s.corruption.entries = rows();

    // --- culture ---------------------------------------------------------
    s.culture_layers = rows_of(blank_culture_layer());
    for layer in &mut s.culture_layers {
        layer.traits = rows_of(blank_culture_trait());
    }
    s.culture_tensions = rows_of(blank_culture_tension());
    s.influencers = rows_of(blank_influencer());
    for influencer in &mut s.influencers {
        influencer.audience_generations = vec![0u16; ROWS];
        influencer.culture_resonance = rows_of(blank_culture_resonance());
    }
    for axis in [
        &mut s.sentiment.knowledge,
        &mut s.sentiment.trust,
        &mut s.sentiment.equity,
        &mut s.sentiment.agency,
    ] {
        axis.drivers = rows();
    }

    // --- campaign --------------------------------------------------------
    s.campaign_profiles = rows();
    for profile in &mut s.campaign_profiles {
        profile.id = Some(String::new());
        profile.title = Some(String::new());
        profile.title_loc_key = Some(String::new());
        profile.subtitle = Some(String::new());
        profile.subtitle_loc_key = Some(String::new());
        profile.starting_units = rows();
        for unit in &mut profile.starting_units {
            unit.tags = vec![String::new(); ROWS];
        }
        profile.inventory = rows();
        profile.knowledge_tags = vec![String::new(); ROWS];
        profile.primary_food_module = Some(String::new());
        profile.secondary_food_module = Some(String::new());
    }
    s.command_events = rows();
    for event in &mut s.command_events {
        event.detail = Some(String::new());
    }
    s.pending_forks = rows();
    for entry in &mut s.pending_forks {
        entry.forks = rows();
        for fork in &mut entry.forks {
            fork.narration = rows();
            fork.choices = rows();
            for choice in &mut fork.choices {
                choice.label = rows();
            }
            fork.gloss = rows();
        }
    }
    s.stance_axes = rows();
    for stance in &mut s.stance_axes {
        stance.axes = rows();
    }
    s.voice_medium = rows();
    s.victory.modes = rows();
    s.victory.winner = Some(VictoryResultState::default());

    s
}

// ---------------------------------------------------------------------------
// Step 2 — saturate every scalar leaf
// ---------------------------------------------------------------------------

/// Fails the fixture build when a repeated field carries no elements — i.e. when a `Vec` was
/// appended to the schema and not seeded in [`seed_snapshot`]. An empty array is invisible to the
/// golden (the decoder emits an empty array either way), so it is exactly the gap that would let an
/// appended repeated field ship untested; refusing to build is the point.
/// Subtrees of `WorldSnapshot` the client decoder **never reads**, so seeding one would grow the
/// fixture without exercising a single line of the decoder.
///
/// Both survivors **are** encoded (`sim_schema/src/codec/knowledge.rs` writes them onto a delta) —
/// they simply have no converter on the client side, so nothing in `native/src/dict/` would run.
/// Being on the wire is not what earns a gate here; being decoded is.
///
/// This list used to also hold four sim-only rollback records (`herd_registry`, `forage_registry`,
/// `graze_registry`, `beat_ledger`). They were never *encoded* at all — being neither on the wire
/// nor read, they were deleted from `WorldSnapshot` rather than gated, so the list is back to
/// meaning one thing: encoded, but not decoded.
///
/// Named individually rather than skipped by heuristic: if one of these ever *does* get wired to
/// the client, deleting its entry here is what turns the gate back on.
const OFF_WIRE_SUBTREES: [&str; 2] = ["knowledge_ledger", "knowledge_timeline"];

fn is_off_wire(path: &str) -> bool {
    OFF_WIRE_SUBTREES.iter().any(|root| {
        path == *root
            || path.starts_with(&format!("{root}."))
            || path.starts_with(&format!("{root}["))
    })
}

fn assert_no_empty_arrays(value: &serde_json::Value, path: &str) -> Result<(), Box<dyn Error>> {
    if is_off_wire(path) {
        return Ok(());
    }
    match value {
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return Err(format!(
                    "decode fixture: `{path}` is an empty array — seed it in seed_snapshot() so the \
                     decode guard actually exercises the repeated field (see xtask/src/decode_fixture.rs)"
                )
                .into());
            }
            for (i, item) in items.iter().enumerate() {
                assert_no_empty_arrays(item, &format!("{path}[{i}]"))?;
            }
        }
        serde_json::Value::Object(fields) => {
            for (key, field) in fields {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                assert_no_empty_arrays(field, &child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Replaces every scalar leaf with a deterministic, path-derived sentinel. See the module docs for
/// the two type-safety rules (empty-string-only replacement; integers capped at 200).
fn saturate(value: &mut serde_json::Value, path: &str) {
    match value {
        serde_json::Value::Array(items) => {
            for (i, item) in items.iter_mut().enumerate() {
                saturate(item, &format!("{path}[{i}]"));
            }
        }
        serde_json::Value::Object(fields) => {
            for (key, field) in fields.iter_mut() {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                saturate(field, &child);
            }
        }
        serde_json::Value::Bool(flag) => *flag = true,
        serde_json::Value::String(text) => {
            // Only free text. A non-empty default is a serde-serialized enum variant name, and
            // replacing that with a path would fail to deserialize.
            if text.is_empty() {
                *text = path.to_string();
            }
        }
        serde_json::Value::Number(number) => {
            *value = if number.is_f64() {
                // Fractional, so a dropped `fixed64_to_f32` divide or an `int()` narrowing shows.
                let scaled = (hash(path) % 90_000) as f64 / 800.0;
                serde_json::json!((1.0 + scaled * 100.0).round() / 100.0)
            } else {
                // Capped so a `u8` field cannot fail to deserialize; distinctness lives in the
                // string paths, not here.
                serde_json::json!(1 + hash(path) % 200)
            };
        }
        // `null` is `Option::None`, whose inner type is unknowable here. Options worth covering are
        // seeded to `Some(Default)` in `seed_snapshot` and saturate as ordinary values.
        serde_json::Value::Null => {}
    }
}

/// FNV-1a over the path. Any stable hash would do; this one keeps the fixture reproducible across
/// machines and Rust versions (`DefaultHasher` guarantees neither).
fn hash(path: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_bytes() {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------
// Step 3 — structural fixups
// ---------------------------------------------------------------------------

/// Restores the fields whose values are *structural* rather than arbitrary: a raster's dimensions
/// must match its sample count, and a tile's coordinates must land inside the grid, or the decoder
/// indexes a raster out of step with the grid it was told about.
fn apply_structural_fixups(s: &mut WorldSnapshot) {
    s.header.tick = 41;
    s.header.tile_count = GRID_CELLS as u32;
    s.header.wrap_horizontal = true;

    s.terrain.width = GRID_W;
    s.terrain.height = GRID_H;
    // Vary the biome per cell so `terrain_label_from_id` and the tag-label table are exercised
    // rather than answering one question 12 times.
    for (i, sample) in s.terrain.samples.iter_mut().enumerate() {
        sample.terrain = TerrainType::VALUES[i % TerrainType::VALUES.len()];
    }

    s.elevation_overlay.width = GRID_W;
    s.elevation_overlay.height = GRID_H;
    s.moisture_raster.width = GRID_W;
    s.moisture_raster.height = GRID_H;

    for raster in [
        &mut s.logistics_raster,
        &mut s.sentiment_raster,
        &mut s.corruption_raster,
        &mut s.culture_raster,
        &mut s.military_raster,
        &mut s.visibility_raster,
        &mut s.crisis_overlay.heatmap,
    ] {
        raster.width = GRID_W;
        raster.height = GRID_H;
    }

    for (i, tile) in s.tiles.iter_mut().enumerate() {
        tile.entity = i as u64;
        tile.x = (i as u32) % GRID_W;
        tile.y = (i as u32) / GRID_W;
        tile.terrain = TerrainType::VALUES[i % TerrainType::VALUES.len()];
        tile.underlying_terrain = TerrainType::VALUES[(i + 1) % TerrainType::VALUES.len()];
    }

    if let Some(marker) = s.start_marker.as_mut() {
        marker.x = 1;
        marker.y = 1;
    }

    // Keep every located row on the map. A saturated coordinate would sit far outside a 4×3 grid,
    // which is legal on the wire but makes the golden read as nonsense.
    for (i, cohort) in s.populations.iter_mut().enumerate() {
        cohort.entity = 100 + i as u64;
        cohort.current_x = i as u32 % GRID_W;
        cohort.current_y = i as u32 % GRID_H;
    }
    for (i, herd) in s.herds.iter_mut().enumerate() {
        herd.x = i as u32 % GRID_W;
        herd.y = i as u32 % GRID_H;
    }
    for (i, module) in s.food_modules.iter_mut().enumerate() {
        module.x = i as u32 % GRID_W;
        module.y = i as u32 % GRID_H;
    }
    for (i, patch) in s.forage_patches.iter_mut().enumerate() {
        patch.x = i as u32 % GRID_W;
        patch.y = i as u32 % GRID_H;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The half of the decode gate that runs **without Godot**, and therefore in CI.
    ///
    /// `tools/decode_guard.gd` needs a live engine (`VarDictionary` cannot be constructed outside
    /// one) and CI has no Godot, so the golden diff is a local gate. This test carries what does
    /// travel: `build_fixture_snapshot` runs [`assert_no_empty_arrays`], so **appending a repeated
    /// field to the schema and forgetting to seed it fails CI**, naming the path — the schema-drift
    /// alarm, without the engine.
    #[test]
    fn the_fixture_covers_every_repeated_field_and_encodes() {
        let snapshot = build_fixture_snapshot().expect("fixture builds");
        let bytes = encode_snapshot_flatbuffer(&snapshot);
        assert!(
            bytes.len() > 1024,
            "fixture envelope is suspiciously small ({} bytes) — did a section stop encoding?",
            bytes.len()
        );
    }

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
        let snapshot = build_fixture_snapshot().expect("snapshot fixture builds");
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
        let snapshot = build_fixture_snapshot().expect("snapshot fixture builds");
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

    /// Saturation must be **deterministic**, or the committed golden would drift with no decoder
    /// change and every reader would learn to ignore the diff. Path-derived FNV, no clock, no RNG.
    #[test]
    fn the_fixture_is_byte_identical_across_builds() {
        let first = encode_snapshot_flatbuffer(&build_fixture_snapshot().expect("first build"));
        let second = encode_snapshot_flatbuffer(&build_fixture_snapshot().expect("second build"));
        assert_eq!(first, second, "fixture encoding is not deterministic");
    }

    /// The two saturation rules the whole scheme rests on: an enum's variant name must survive
    /// (only an *empty* string is free text), and integers must stay inside `u8` so a narrow field
    /// cannot fail to deserialize.
    #[test]
    fn saturation_preserves_enum_variants_and_stays_in_u8_range() {
        let mut value = serde_json::json!({
            "free_text": "",
            "an_enum": "AlluvialPlain",
            "a_count": 0,
            "a_rate": 0.0,
        });
        saturate(&mut value, "root");

        assert_eq!(value["free_text"], serde_json::json!("root.free_text"));
        assert_eq!(value["an_enum"], serde_json::json!("AlluvialPlain"));

        let count = value["a_count"].as_u64().expect("count stays an integer");
        assert!(
            (1..=200).contains(&count),
            "integer sentinel {count} would not fit a u8 field"
        );
        let rate = value["a_rate"].as_f64().expect("rate stays a float");
        assert!(
            rate.fract() != 0.0,
            "float sentinel {rate} has no fraction, so an int() narrowing would be invisible"
        );
    }
}
