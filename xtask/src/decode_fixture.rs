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
use sim_schema::world::{WorldDelta, WorldSnapshot};
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

/// Where the encoded envelope lands. Committed, so `tools/decode_guard.tscn` runs standalone;
/// `cargo xtask decode-guard` regenerates it first, so the gate can never run against a stale one.
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

/// Where the HEADERLESS envelope lands. Committed beside the main fixture for the same reason:
/// `tools/decode_guard.tscn` must run standalone.
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

/// Where the DELTA envelope lands, beside the other two for the same standalone-run reason.
pub fn delta_fixture_path() -> PathBuf {
    Path::new("clients")
        .join("godot_thin_client")
        .join("tests")
        .join("fixtures")
        .join("snapshot_delta_envelope.bin")
}

/// Tiles the delta moves. A handful, not all of them: the guard asserts the merged frame keeps the
/// **baseline's tile COUNT** while carrying the delta's values, and that assertion is only
/// meaningful if most tiles are untouched.
const DELTA_CHANGED_TILES: usize = 3;

/// How far the delta moves each changed tile's graze/forage reading. Applied as an OFFSET from the
/// baseline value rather than as an absolute, so the moved value cannot accidentally coincide with
/// the saturated one the baseline carries — the guard's "this is not the baseline's value" check
/// would then pass while proving nothing.
const DELTA_BIOMASS_STEP: f32 = 100.0;

/// The same idea for `temperature`, which rides the wire as fixed-point (1e6) — a whole 5 °C, so a
/// dropped `fixed64_to_f64` divide is still visible in the guard's output.
const DELTA_TEMPERATURE_STEP: i64 = 5_000_000;

/// The ONE changed tile whose river and culture-layer fields also move.
///
/// The decoder derives `tiles.rivers` / `tiles.culture_layer` for the change manifest by comparing
/// each changed tile against the entry it replaces, and this is what exercises that comparison in
/// **both** directions: the other changed tiles move only their readings, so a comparison that
/// answered "changed" for any moved tile would be indistinguishable from a correct one without a
/// tile that moves a reading and nothing else.
const DELTA_SPLATMAP_TILE: usize = DELTA_CHANGED_TILES - 1;

/// One Minor-river class bit on direction 0 of the packed `river_edges` mask (2 bits per odd-r
/// direction — see `tile_to_dict`). XORed in, so the mask provably differs whatever it was.
const DELTA_RIVER_EDGE_BIT: u16 = 0b01;

/// Rows the delta moves in the NON-tile keyed sections (`populations`, `culture_layers`).
///
/// One of the fixture's two rows per section, deliberately: it is what lets the guard check that
/// the merged frame carries the moved row AND kept the untouched one, which a delta that restated
/// every row could not show. Those two sections stand for the whole keyed-section family — they are
/// the ones with live consumers reading the base key (`Main`'s band alerts, `MapView`'s harvest and
/// culture overlays), so a regression there is player-visible.
const DELTA_CHANGED_ROWS: usize = 1;

/// How far the delta moves the probe field on those rows. An OFFSET from the baseline value, for
/// the same reason as [`DELTA_BIOMASS_STEP`]: the moved value then cannot coincide with the
/// saturated one, which would make the guard's "this is not the baseline's value" check vacuous.
const DELTA_COUNT_STEP: u32 = 7;

/// Writes a **DELTA** envelope built against the same synthetic world [`write_fixture`] emits.
///
/// The delta path had no fixture at all, and that is exactly why it shipped a world whose `tiles`
/// array never moved after the baseline snapshot: the decoder inserted the sparse `tile_updates`
/// list and left `tiles` standing, so every per-tile lookup in `MapView` was frozen for the life of
/// the world. Nothing in `cargo xtask decode-guard` could see it, because nothing decoded a delta.
///
/// Deliberately **sparse** — it carries a header and a few tiles and nothing else. That is what
/// lets the guard assert the change manifest does not name a section this delta left alone
/// (`forage_patches`), which a saturated everything-present delta could never show.
pub fn write_delta_fixture() -> Result<(), Box<dyn Error>> {
    let delta = build_fixture_delta()?;
    let bytes = encode_delta_flatbuffer(&delta);
    let path = delta_fixture_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &bytes)?;
    println!(
        "Wrote delta decode fixture to {} ({} bytes; {} tiles, {} populations, {} culture layers changed)",
        path.display(),
        bytes.len(),
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
pub fn build_fixture_delta() -> Result<WorldDelta, Box<dyn Error>> {
    let snapshot = build_fixture_snapshot()?;

    let mut header = snapshot.header.clone();
    header.tick = snapshot.header.tick + 1;
    // The gate the client applies before merging: same world, and a base it actually holds.
    header.base_frame_seq = snapshot.header.frame_seq;
    header.frame_seq = snapshot.header.frame_seq + 1;

    let tiles = snapshot
        .tiles
        .iter()
        .take(DELTA_CHANGED_TILES)
        .enumerate()
        .map(|(i, tile)| {
            let mut moved = tile.clone();
            moved.graze_biomass += DELTA_BIOMASS_STEP;
            moved.forage_capacity += DELTA_BIOMASS_STEP;
            moved.temperature += DELTA_TEMPERATURE_STEP;
            if i == DELTA_SPLATMAP_TILE {
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
        .take(DELTA_CHANGED_ROWS)
        .map(|cohort| {
            let mut moved = cohort.clone();
            moved.size = moved.size.wrapping_add(DELTA_COUNT_STEP);
            moved
        })
        .collect();
    let culture_layers = snapshot
        .culture_layers
        .iter()
        .take(DELTA_CHANGED_ROWS)
        .map(|layer| {
            let mut moved = layer.clone();
            moved.parent = moved.parent.wrapping_add(DELTA_COUNT_STEP);
            moved
        })
        .collect();

    Ok(WorldDelta {
        header,
        tiles,
        populations,
        culture_layers,
        // Carried on every delta rather than diffed (see `WorldDelta::fog_enabled`); the derived
        // `Default` says `false`, which would silently flip the merged world's fog.
        fog_enabled: snapshot.fog_enabled,
        ..Default::default()
    })
}

/// Seed → saturate → fix up. See the module docs for why it is done in that order.
pub fn build_fixture_snapshot() -> Result<WorldSnapshot, Box<dyn Error>> {
    let seeded = seed_snapshot();

    let mut value = serde_json::to_value(&seeded)?;
    assert_no_empty_arrays(&value, "")?;
    saturate(&mut value, "");

    let mut snapshot: WorldSnapshot = serde_json::from_value(value)?;
    apply_structural_fixups(&mut snapshot);
    Ok(snapshot.finalize())
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
        herd.hunt_policy_ceilings = rows();
        herd.hunt_trip_estimates = rows();
    }
    s.food_modules = rows();
    s.sedentarization = rows();
    s.forage_patches = rows();
    for patch in &mut s.forage_patches {
        patch.owner = Some(0);
        patch.composition = rows();
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
/// Two distinct reasons, and the distinction matters: the first four are the sim's own rollback
/// records, round-tripped through the bincode save but never serialized into the envelope at all.
/// `knowledge_ledger` and `knowledge_timeline` **are** encoded (`sim_schema/src/codec/knowledge.rs`
/// writes both onto a delta) — they simply have no converter on the client side, so nothing in
/// `native/src/dict/` would run. Being on the wire is not what earns a gate here; being decoded is.
///
/// Named individually rather than skipped by heuristic: if one of these ever *does* get wired to
/// the client, deleting its entry here is what turns the gate back on.
const OFF_WIRE_SUBTREES: [&str; 6] = [
    "herd_registry",
    "forage_registry",
    "graze_registry",
    "beat_ledger",
    "knowledge_ledger",
    "knowledge_timeline",
];

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

    /// The delta fixture must stay **applicable** to the snapshot fixture and must actually MOVE
    /// every row the guard probes — the half of the delta gate that reaches CI, where the
    /// Godot-side merge assertions cannot.
    ///
    /// Both halves rot silently. A base sequence that stopped naming the snapshot's frame would
    /// make the client drop the frame, and the guard would then be asserting against an empty
    /// dictionary; tile values that stopped moving would let a decoder that ignores the delta's
    /// tiles entirely — the bug this fixture exists for — pass.
    #[test]
    fn the_delta_fixture_applies_to_the_snapshot_and_moves_every_probed_row() {
        let snapshot = build_fixture_snapshot().expect("snapshot fixture builds");
        let delta = build_fixture_delta().expect("delta fixture builds");

        assert_eq!(
            delta.header.base_frame_seq, snapshot.header.frame_seq,
            "the delta must name the snapshot's frame as its base, or the client drops it"
        );
        assert_eq!(
            delta.header.world_epoch, snapshot.header.world_epoch,
            "a delta from another world is never merged"
        );
        assert!(
            delta.tiles.len() < snapshot.tiles.len(),
            "the delta must be SPARSE ({} of {} tiles), or 'the merged frame kept the baseline's \
             tile count' proves nothing",
            delta.tiles.len(),
            snapshot.tiles.len()
        );
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

        let splatmap_tile = &delta.tiles[DELTA_SPLATMAP_TILE];
        let splatmap_baseline = snapshot
            .tiles
            .iter()
            .find(|tile| tile.entity == splatmap_tile.entity)
            .expect("the splatmap tile exists in the baseline");
        assert_ne!(
            splatmap_baseline.river_edges, splatmap_tile.river_edges,
            "one changed tile must move `river_edges`, or `tiles.rivers` is never exercised"
        );
        assert_ne!(
            splatmap_baseline.culture_layer, splatmap_tile.culture_layer,
            "one changed tile must move `culture_layer`, or `tiles.culture_layer` is never exercised"
        );
        for (i, moved) in delta.tiles.iter().enumerate() {
            if i == DELTA_SPLATMAP_TILE {
                continue;
            }
            let baseline = snapshot
                .tiles
                .iter()
                .find(|tile| tile.entity == moved.entity)
                .expect("every changed tile exists in the baseline");
            assert_eq!(
                (baseline.river_edges, baseline.culture_layer),
                (moved.river_edges, moved.culture_layer),
                "tile {} must move ONLY its readings, so a comparison that flags every changed \
                 tile is distinguishable from a correct one",
                moved.entity
            );
        }

        // The two non-tile keyed sections: one row moved, one row left alone, in both.
        assert_eq!(delta.populations.len(), DELTA_CHANGED_ROWS);
        assert_eq!(delta.culture_layers.len(), DELTA_CHANGED_ROWS);
        assert!(
            snapshot.populations.len() > DELTA_CHANGED_ROWS
                && snapshot.culture_layers.len() > DELTA_CHANGED_ROWS,
            "each section must keep an UNTOUCHED row, or 'the merged frame kept the baseline's \
             count' proves nothing"
        );
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
        let layer_ids: HashSet<u32> = snapshot.culture_layers.iter().map(|l| l.id).collect();
        assert_eq!(
            layer_ids.len(),
            snapshot.culture_layers.len(),
            "the fixture's culture layers must have DISTINCT ids — the merge indexes by them, so a \
             collision would silently patch the wrong row"
        );

        let bytes = encode_delta_flatbuffer(&delta);
        let envelope =
            fb::root_as_envelope(&bytes).expect("the delta envelope must verify as FlatBuffers");
        assert_eq!(envelope.payload_type(), fb::SnapshotPayload::delta);
        assert!(
            envelope
                .payload_as_delta()
                .and_then(|d| d.subsistence())
                .and_then(|s| s.foragePatches())
                .is_none(),
            "the delta must leave `forage_patches` ABSENT — the guard asserts the change manifest \
             does not name it"
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
