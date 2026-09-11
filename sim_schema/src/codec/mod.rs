//! FlatBuffers encoders **and decoders** for the world payloads.
//!
//! `build_snapshot_flatbuffer` / `build_delta_flatbuffer` assemble the envelope from the
//! per-domain section serializers in the sibling modules, and `decode_snapshot_table` /
//! `decode_delta_table` take it apart again through the per-domain section decoders that sit
//! beside them; helpers shared by two or more sections live here.
//!
//! **The decoder mirrors the encoder file for file and field for field** (`docs/plan_ai_driver.md`
//! §2): every `serialize_<section>_section[_delta]` has a `decode_<section>_section[_delta]` in
//! the same module, every `to_fb_<enum>` has a `to_state_<enum>` beside it, and every state
//! struct is rebuilt with an **exhaustive** literal — no `..Default::default()` — so a field
//! appended to a struct and its serializer fails to compile until its decoder line exists. A
//! section left undecoded would read as empty, which is exactly what an unseen section also reads
//! as; that silence is the failure this layout exists to prevent.
//!
//! **The two roots are the exception, and carry their own witness.** `decode_snapshot_table` and
//! `decode_delta_table` *do* start from `..Default::default()`, because the section decoders fill a
//! `&mut WorldSnapshot` / `&mut WorldDelta` rather than returning a section struct — so the rule
//! above would not hold for a field appended to a **root**. `witness_snapshot_is_fully_decoded` /
//! `witness_delta_is_fully_decoded` restore it: they destructure the root exhaustively, grouped by
//! the section decoder that fills each field, and are called at the end of each decoder.
//!
//! What the decoder gives back is what the encoder was given, with the losses the wire itself
//! imposes: a delta's `Option` fields come back `None` exactly when the encoder left the slot
//! absent (or, for the two scalars whose absent encoding *is* `0`, when it wrote `0`), and a
//! field the encoder never writes at all — a `(deprecated)` slot, `start_marker` — comes back at
//! its zero value.

// ---------------------------------------------------------------------------
// Per-section FlatBuffers serializers (docs/plan_snapshot_and_systems_decomposition.md §1).
// Each root nests one section table per subsystem; one helper per section per
// root builds its child offsets then the section table, so a future field
// addition to a section localizes to a single helper instead of the mega
// `build_*_flatbuffer` bodies. The delta variants preserve the exact per-field
// Option/empty-vector handling the flat delta used; `removed*` lists and
// snapshot-only fields are left unset on the side that does not carry them.
// ---------------------------------------------------------------------------

mod campaign;
mod connections;
mod culture;
mod economy;
mod governance;
mod knowledge;
mod map;
mod population;
mod routes;
mod subsistence;
mod vision;

use crate::codec::campaign::{
    create_campaign_label, create_victory_state, decode_campaign_label, decode_campaign_section,
    decode_campaign_section_delta, serialize_campaign_section, serialize_campaign_section_delta,
};
use crate::codec::connections::{
    decode_connection_section, decode_connection_section_delta, serialize_connection_section,
    serialize_connection_section_delta,
};
use crate::codec::culture::{
    decode_culture_section, decode_culture_section_delta, serialize_culture_section,
    serialize_culture_section_delta,
};
use crate::codec::economy::{
    decode_economy_section, decode_economy_section_delta, serialize_economy_section,
    serialize_economy_section_delta,
};
use crate::codec::governance::{
    decode_governance_section, decode_governance_section_delta, serialize_governance_section,
    serialize_governance_section_delta,
};
use crate::codec::knowledge::{
    decode_knowledge_section, decode_knowledge_section_delta, serialize_knowledge_section,
    serialize_knowledge_section_delta,
};
use crate::codec::map::{
    decode_map_section, decode_map_section_delta, serialize_map_section,
    serialize_map_section_delta,
};
use crate::codec::population::{
    decode_population_section, decode_population_section_delta, serialize_population_section,
    serialize_population_section_delta,
};
use crate::codec::routes::{
    decode_route_section, decode_route_section_delta, serialize_route_section,
    serialize_route_section_delta,
};
use crate::codec::subsistence::{
    decode_subsistence_section, decode_subsistence_section_delta, serialize_subsistence_section,
    serialize_subsistence_section_delta,
};
use crate::codec::vision::{
    decode_vision_section, decode_vision_section_delta, serialize_vision_section,
    serialize_vision_section_delta,
};
use crate::state::economy::KnownTechFragment;
use crate::state::map::{FloatRasterState, ScalarRasterState};
use crate::world::{SnapshotHeader, WorldDelta, WorldSnapshot};
use flatbuffers::{
    DefaultAllocator, FlatBufferBuilder, Follow, ForwardsUOffset, Vector, WIPOffset,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// Why a frame could not be turned back into a [`WorldSnapshot`] / [`WorldDelta`].
///
/// Every arm is a statement about the **bytes**, never a default quietly taken: an enum
/// discriminant this build does not know is an error, not the first variant, because a silent
/// default is indistinguishable from a real value of that variant.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The FlatBuffers verifier rejected the buffer — it is not an `Envelope` at all.
    #[error("the bytes are not a valid FlatBuffers envelope: {0}")]
    InvalidBuffer(#[from] flatbuffers::InvalidFlatbuffer),
    /// The `SnapshotPayload` union carries a variant this build does not know.
    #[error("the envelope's SnapshotPayload union carries unknown variant {0}")]
    UnknownPayload(u8),
    /// The caller asked for one payload kind and the envelope carried the other.
    #[error("expected a {expected} payload, found a {found}")]
    UnexpectedPayload {
        expected: &'static str,
        found: &'static str,
    },
    /// An enum field carries a discriminant this build does not know.
    #[error("unknown {field} discriminant {value}")]
    UnknownEnum { field: &'static str, value: i64 },
    /// A table the encoder always writes — the payload root, its header, a section — is absent.
    #[error("the envelope carries no {table} table")]
    MissingTable { table: &'static str },
}

/// A decoded frame of either kind, as the `Envelope`'s `SnapshotPayload` union names it.
#[derive(Debug, Clone)]
pub enum FramePayload {
    Snapshot(WorldSnapshot),
    Delta(WorldDelta),
}

const SNAPSHOT_PAYLOAD_NAME: &str = "snapshot";
const DELTA_PAYLOAD_NAME: &str = "delta";

pub(crate) type FbBuilder<'a> = FlatBufferBuilder<'a, DefaultAllocator>;

pub fn encode_snapshot_flatbuffer(snapshot: &WorldSnapshot) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let offset = build_snapshot_flatbuffer(&mut builder, snapshot);
    builder.finish(offset, None);
    builder.finished_data().to_vec()
}

pub fn encode_delta_flatbuffer(delta: &WorldDelta) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let offset = build_delta_flatbuffer(&mut builder, delta);
    builder.finish(offset, None);
    builder.finished_data().to_vec()
}

/// Reads the `Envelope` root (verified) and decodes whichever payload its union carries.
pub fn decode_frame_flatbuffer(bytes: &[u8]) -> Result<FramePayload, DecodeError> {
    let envelope = fb::root_as_envelope(bytes)?;
    match envelope.payload_type() {
        fb::SnapshotPayload::snapshot => {
            let table = required(envelope.payload_as_snapshot(), "WorldSnapshot")?;
            Ok(FramePayload::Snapshot(decode_snapshot_table(table)?))
        }
        fb::SnapshotPayload::delta => {
            let table = required(envelope.payload_as_delta(), "WorldDelta")?;
            Ok(FramePayload::Delta(decode_delta_table(table)?))
        }
        other => Err(DecodeError::UnknownPayload(other.0)),
    }
}

/// [`decode_frame_flatbuffer`], requiring the payload to be a full snapshot.
pub fn decode_snapshot_flatbuffer(bytes: &[u8]) -> Result<WorldSnapshot, DecodeError> {
    match decode_frame_flatbuffer(bytes)? {
        FramePayload::Snapshot(snapshot) => Ok(snapshot),
        FramePayload::Delta(_) => Err(DecodeError::UnexpectedPayload {
            expected: SNAPSHOT_PAYLOAD_NAME,
            found: DELTA_PAYLOAD_NAME,
        }),
    }
}

/// [`decode_frame_flatbuffer`], requiring the payload to be a delta.
pub fn decode_delta_flatbuffer(bytes: &[u8]) -> Result<WorldDelta, DecodeError> {
    match decode_frame_flatbuffer(bytes)? {
        FramePayload::Delta(delta) => Ok(delta),
        FramePayload::Snapshot(_) => Err(DecodeError::UnexpectedPayload {
            expected: DELTA_PAYLOAD_NAME,
            found: SNAPSHOT_PAYLOAD_NAME,
        }),
    }
}

/// The inverse of [`build_snapshot_flatbuffer`]: the header, then one section decoder per
/// section serializer, in the same order.
fn decode_snapshot_table(table: fb::WorldSnapshot<'_>) -> Result<WorldSnapshot, DecodeError> {
    let mut snapshot = WorldSnapshot {
        header: decode_header(required(table.header(), "SnapshotHeader")?),
        capability_flags: table.capabilityFlags(),
        ..Default::default()
    };
    decode_map_section(required(table.map(), "MapSection")?, &mut snapshot)?;
    decode_economy_section(required(table.economy(), "EconomySection")?, &mut snapshot);
    decode_population_section(
        required(table.population(), "PopulationSection")?,
        &mut snapshot,
    )?;
    decode_subsistence_section(
        required(table.subsistence(), "SubsistenceSection")?,
        &mut snapshot,
    );
    decode_knowledge_section(
        required(table.knowledge(), "KnowledgeSection")?,
        &mut snapshot,
    )?;
    decode_governance_section(
        required(table.governance(), "GovernanceSection")?,
        &mut snapshot,
    )?;
    decode_culture_section(required(table.culture(), "CultureSection")?, &mut snapshot)?;
    decode_vision_section(required(table.vision(), "VisionSection")?, &mut snapshot);
    decode_campaign_section(
        required(table.campaign(), "CampaignSection")?,
        &mut snapshot,
    );
    decode_connection_section(
        required(table.connections(), "ConnectionSection")?,
        &mut snapshot,
    );
    decode_route_section(required(table.routes(), "RouteSection")?, &mut snapshot);
    witness_snapshot_is_fully_decoded(&snapshot);
    Ok(snapshot)
}

/// The inverse of [`build_delta_flatbuffer`], preserving the serializers' absent/`None`
/// convention field by field.
fn decode_delta_table(table: fb::WorldDelta<'_>) -> Result<WorldDelta, DecodeError> {
    let mut delta = WorldDelta {
        header: decode_header(required(table.header(), "SnapshotHeader")?),
        // `0` IS the absent encoding for this scalar — see `build_delta_flatbuffer`.
        capability_flags: changed_scalar(table.capabilityFlags()),
        ..Default::default()
    };
    decode_map_section_delta(required(table.map(), "MapSection")?, &mut delta)?;
    decode_economy_section_delta(required(table.economy(), "EconomySection")?, &mut delta);
    decode_population_section_delta(
        required(table.population(), "PopulationSection")?,
        &mut delta,
    )?;
    decode_subsistence_section_delta(
        required(table.subsistence(), "SubsistenceSection")?,
        &mut delta,
    );
    decode_knowledge_section_delta(required(table.knowledge(), "KnowledgeSection")?, &mut delta)?;
    decode_governance_section_delta(
        required(table.governance(), "GovernanceSection")?,
        &mut delta,
    )?;
    decode_culture_section_delta(required(table.culture(), "CultureSection")?, &mut delta)?;
    decode_vision_section_delta(required(table.vision(), "VisionSection")?, &mut delta);
    decode_campaign_section_delta(required(table.campaign(), "CampaignSection")?, &mut delta);
    decode_connection_section_delta(
        required(table.connections(), "ConnectionSection")?,
        &mut delta,
    );
    decode_route_section_delta(required(table.routes(), "RouteSection")?, &mut delta);
    witness_delta_is_fully_decoded(&delta);
    Ok(delta)
}

// -------------------------------------------------------------------------------------------------
// The roots' forcing function
// -------------------------------------------------------------------------------------------------

/// ⛔ **EVERY FIELD OF THE ROOT IS NAMED HERE, UNDER THE DECODER THAT FILLS IT.**
///
/// The module header's rule — a state struct is rebuilt as an exhaustive literal, so a field
/// appended to it and its serializer fails to compile until its decoder line exists — is a property
/// of the **leaf** decoders. It cannot reach the two roots: [`decode_snapshot_table`] and
/// [`decode_delta_table`] start from `..Default::default()` and the section decoders assign into a
/// `&mut WorldSnapshot` / `&mut WorldDelta` field by field, so a field appended to a root and wired
/// into one serializer — and left out of the matching section decoder — compiles, and reads back at
/// its `Default`. For an `Option` or a vector the fixture cannot saturate, that is silent.
///
/// These two functions are the witness. They destructure the root **exhaustively**, so appending a
/// field to it stops the build here and names it; the fix is to add its line to the section decoder
/// commented beside the group it belongs to, and then name it in that group. They are called at the
/// end of each decoder rather than written as a test, so the obligation cannot be satisfied by
/// deleting a test file.
fn witness_snapshot_is_fully_decoded(snapshot: &WorldSnapshot) {
    let WorldSnapshot {
        // The root's own fields, set in the function above.
        header: _,
        capability_flags: _,
        // No FlatBuffers field at all: captured and diffed in `core_sim`, sent nowhere.
        start_marker: _,
        // `decode_map_section`
        climate_bands: _,
        elevation_overlay: _,
        moisture_raster: _,
        temperature_survivability: _,
        terrain: _,
        tiles: _,
        // `decode_economy_section`
        faction_inventory: _,
        // `decode_population_section`
        demographics: _,
        generations: _,
        populations: _,
        // `decode_subsistence_section`
        characteristic_bands: _,
        craft_knowledge: _,
        default_expedition_kit_id: _,
        default_forage_kit_id: _,
        default_hunt_kit_id: _,
        default_scout_kit_id: _,
        default_warrior_kit_id: _,
        equipment_config_json: _,
        food_modules: _,
        forage_patches: _,
        herds: _,
        intensification_knowledge: _,
        kits: _,
        ladder_knowledge: _,
        materials: _,
        recipes: _,
        route_rungs: _,
        deposits: _,
        deposit_rungs: _,
        sedentarization: _,
        // `decode_knowledge_section`
        discovered_sites: _,
        discovery_progress: _,
        great_discoveries: _,
        great_discovery_definitions: _,
        great_discovery_progress: _,
        great_discovery_telemetry: _,
        knowledge_ledger: _,
        knowledge_metrics: _,
        knowledge_timeline: _,
        // `decode_governance_section`
        corruption: _,
        corruption_raster: _,
        crisis_overlay: _,
        crisis_telemetry: _,
        power: _,
        power_metrics: _,
        // `decode_culture_section`
        axis_bias: _,
        culture_layers: _,
        culture_raster: _,
        culture_tensions: _,
        influencers: _,
        sentiment: _,
        sentiment_raster: _,
        // `decode_vision_section`
        fog_enabled: _,
        military_raster: _,
        visibility_raster: _,
        // `decode_campaign_section`
        campaign_profiles: _,
        command_events: _,
        command_events_retention_turns: _,
        opening_loadout: _,
        pending_forks: _,
        stance_axes: _,
        victory: _,
        voice_medium: _,
        // `decode_connections_section`
        connections: _,
        // `decode_routes_section`
        routes: _,
    } = snapshot;
}

fn witness_delta_is_fully_decoded(delta: &WorldDelta) {
    let WorldDelta {
        // The root's own fields, set in the function above.
        header: _,
        capability_flags: _,
        // No FlatBuffers field at all: captured and diffed in `core_sim`, sent nowhere.
        start_marker: _,
        // `decode_map_section_delta`
        climate_bands: _,
        elevation_overlay: _,
        moisture_raster: _,
        removed_tiles: _,
        temperature_survivability: _,
        terrain: _,
        tiles: _,
        // `decode_economy_section_delta`
        faction_inventory: _,
        // `decode_population_section_delta`
        demographics: _,
        generations: _,
        populations: _,
        removed_generations: _,
        removed_populations: _,
        // `decode_subsistence_section_delta`
        characteristic_bands: _,
        craft_knowledge: _,
        default_expedition_kit_id: _,
        default_forage_kit_id: _,
        default_hunt_kit_id: _,
        default_scout_kit_id: _,
        default_warrior_kit_id: _,
        equipment_config_json: _,
        food_modules: _,
        forage_patches: _,
        herds: _,
        intensification_knowledge: _,
        kits: _,
        ladder_knowledge: _,
        materials: _,
        recipes: _,
        route_rungs: _,
        deposits: _,
        deposit_rungs: _,
        sedentarization: _,
        // `decode_knowledge_section_delta`
        discovered_sites: _,
        discovery_progress: _,
        great_discoveries: _,
        great_discovery_definitions: _,
        great_discovery_progress: _,
        great_discovery_telemetry: _,
        knowledge_ledger: _,
        knowledge_metrics: _,
        knowledge_timeline: _,
        removed_knowledge_ledger: _,
        // `decode_governance_section_delta`
        corruption: _,
        corruption_raster: _,
        crisis_overlay: _,
        crisis_telemetry: _,
        power: _,
        power_metrics: _,
        removed_power: _,
        // `decode_culture_section_delta`
        axis_bias: _,
        culture_layers: _,
        culture_raster: _,
        culture_tensions: _,
        influencers: _,
        removed_culture_layers: _,
        removed_influencers: _,
        sentiment: _,
        sentiment_raster: _,
        // `decode_vision_section_delta`
        fog_enabled: _,
        military_raster: _,
        visibility_raster: _,
        // `decode_campaign_section_delta`
        campaign_profiles: _,
        command_events: _,
        command_events_retention_turns: _,
        opening_loadout: _,
        pending_forks: _,
        stance_axes: _,
        victory: _,
        voice_medium: _,
        // `decode_connections_section_delta`
        connections: _,
        // `decode_routes_section_delta`
        routes: _,
    } = delta;
}

/// The header both roots carry. A delta's `serverBuild` is left absent when empty
/// (`build_delta_flatbuffer`), and an absent string reads back as the empty one it stood for.
fn decode_header(header: fb::SnapshotHeader<'_>) -> SnapshotHeader {
    SnapshotHeader {
        tick: header.tick(),
        tile_count: header.tileCount(),
        population_count: header.populationCount(),
        power_count: header.powerCount(),
        influencer_count: header.influencerCount(),
        hash: header.hash(),
        campaign_label: header.campaignLabel().map(decode_campaign_label),
        wrap_horizontal: header.wrapHorizontal(),
        server_build: text(header.serverBuild()),
        world_epoch: header.worldEpoch(),
        frame_seq: header.frameSeq(),
        base_frame_seq: header.baseFrameSeq(),
    }
}

fn build_snapshot_flatbuffer<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::Envelope<'a>> {
    let campaign_label_fb = snapshot
        .header
        .campaign_label
        .as_ref()
        .and_then(|label| create_campaign_label(builder, label));
    let victory_state = create_victory_state(builder, &snapshot.victory);
    let server_build_fb = builder.create_string(&snapshot.header.server_build);

    let header = fb::SnapshotHeader::create(
        builder,
        &fb::SnapshotHeaderArgs {
            tick: snapshot.header.tick,
            tileCount: snapshot.header.tile_count,
            populationCount: snapshot.header.population_count,
            powerCount: snapshot.header.power_count,
            influencerCount: snapshot.header.influencer_count,
            hash: snapshot.header.hash,
            campaignLabel: campaign_label_fb,
            victory: Some(victory_state),
            wrapHorizontal: snapshot.header.wrap_horizontal,
            serverBuild: Some(server_build_fb),
            worldEpoch: snapshot.header.world_epoch,
            frameSeq: snapshot.header.frame_seq,
            // A full snapshot is applicable against any client state, so it names no base.
            baseFrameSeq: 0,
        },
    );

    let map = serialize_map_section(builder, snapshot);
    let economy = serialize_economy_section(builder, snapshot);
    let population = serialize_population_section(builder, snapshot);
    let subsistence = serialize_subsistence_section(builder, snapshot);
    let knowledge = serialize_knowledge_section(builder, snapshot);
    let governance = serialize_governance_section(builder, snapshot);
    let culture = serialize_culture_section(builder, snapshot);
    let vision = serialize_vision_section(builder, snapshot);
    let campaign = serialize_campaign_section(builder, snapshot, victory_state);
    let connections = serialize_connection_section(builder, snapshot);
    let routes = serialize_route_section(builder, snapshot);

    let snapshot_table = fb::WorldSnapshot::create(
        builder,
        &fb::WorldSnapshotArgs {
            header: Some(header),
            capabilityFlags: snapshot.capability_flags,
            map: Some(map),
            economy: Some(economy),
            population: Some(population),
            subsistence: Some(subsistence),
            knowledge: Some(knowledge),
            governance: Some(governance),
            culture: Some(culture),
            vision: Some(vision),
            campaign: Some(campaign),
            connections: Some(connections),
            routes: Some(routes),
        },
    );

    fb::Envelope::create(
        builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::snapshot,
            payload: Some(snapshot_table.as_union_value()),
        },
    )
}

fn build_delta_flatbuffer<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::Envelope<'a>> {
    let campaign_label_fb = delta
        .header
        .campaign_label
        .as_ref()
        .and_then(|label| create_campaign_label(builder, label));
    let victory_state = delta
        .victory
        .as_ref()
        .map(|state| create_victory_state(builder, state));

    // Deltas fire every turn and only full snapshots populate server_build, so omit the
    // field (leave it None) when empty instead of serializing an empty string each delta.
    let server_build_fb = (!delta.header.server_build.is_empty())
        .then(|| builder.create_string(&delta.header.server_build));
    let header = fb::SnapshotHeader::create(
        builder,
        &fb::SnapshotHeaderArgs {
            tick: delta.header.tick,
            tileCount: delta.header.tile_count,
            populationCount: delta.header.population_count,
            powerCount: delta.header.power_count,
            influencerCount: delta.header.influencer_count,
            hash: delta.header.hash,
            campaignLabel: campaign_label_fb,
            victory: victory_state,
            wrapHorizontal: delta.header.wrap_horizontal,
            serverBuild: server_build_fb,
            worldEpoch: delta.header.world_epoch,
            frameSeq: delta.header.frame_seq,
            baseFrameSeq: delta.header.base_frame_seq,
        },
    );

    let map = serialize_map_section_delta(builder, delta);
    let economy = serialize_economy_section_delta(builder, delta);
    let population = serialize_population_section_delta(builder, delta);
    let subsistence = serialize_subsistence_section_delta(builder, delta);
    let knowledge = serialize_knowledge_section_delta(builder, delta);
    let governance = serialize_governance_section_delta(builder, delta);
    let culture = serialize_culture_section_delta(builder, delta);
    let vision = serialize_vision_section_delta(builder, delta);
    let campaign = serialize_campaign_section_delta(builder, delta, victory_state);
    let connections = serialize_connection_section_delta(builder, delta);
    let routes = serialize_route_section_delta(builder, delta);

    let delta_table = fb::WorldDelta::create(
        builder,
        &fb::WorldDeltaArgs {
            header: Some(header),
            capabilityFlags: delta.capability_flags.unwrap_or(0),
            map: Some(map),
            economy: Some(economy),
            population: Some(population),
            subsistence: Some(subsistence),
            knowledge: Some(knowledge),
            governance: Some(governance),
            culture: Some(culture),
            vision: Some(vision),
            campaign: Some(campaign),
            connections: Some(connections),
            routes: Some(routes),
        },
    );

    fb::Envelope::create(
        builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::delta,
            payload: Some(delta_table.as_union_value()),
        },
    )
}

pub(crate) fn create_known_fragments<'a>(
    builder: &mut FbBuilder<'a>,
    fragments: &[KnownTechFragment],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnownTechFragment<'a>>>> {
    let offsets: Vec<_> = fragments
        .iter()
        .map(|fragment| {
            fb::KnownTechFragment::create(
                builder,
                &fb::KnownTechFragmentArgs {
                    discoveryId: fragment.discovery_id,
                    progress: fragment.progress,
                    fidelity: fragment.fidelity,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

pub(crate) fn create_scalar_raster<'a>(
    builder: &mut FbBuilder<'a>,
    raster: &ScalarRasterState,
) -> WIPOffset<fb::ScalarRaster<'a>> {
    let samples = builder.create_vector(&raster.samples);
    fb::ScalarRaster::create(
        builder,
        &fb::ScalarRasterArgs {
            width: raster.width,
            height: raster.height,
            samples: Some(samples),
        },
    )
}

pub(crate) fn create_float_raster<'a>(
    builder: &mut FbBuilder<'a>,
    raster: &FloatRasterState,
) -> WIPOffset<fb::FloatRaster<'a>> {
    let samples = builder.create_vector(&raster.samples);
    fb::FloatRaster::create(
        builder,
        &fb::FloatRasterArgs {
            width: raster.width,
            height: raster.height,
            samples: Some(samples),
        },
    )
}

// ---------------------------------------------------------------------------
// Decode helpers shared by two or more sections — the inverses of the `create_*` helpers above,
// plus the small vocabulary every section decoder is written in.
// ---------------------------------------------------------------------------

/// A table the encoder always writes, or [`DecodeError::MissingTable`].
pub(crate) fn required<T>(table: Option<T>, name: &'static str) -> Result<T, DecodeError> {
    table.ok_or(DecodeError::MissingTable { table: name })
}

/// [`DecodeError::UnknownEnum`] for a discriminant no `to_state_*` arm matched.
pub(crate) fn unknown_enum(field: &'static str, value: impl Into<i64>) -> DecodeError {
    DecodeError::UnknownEnum {
        field,
        value: value.into(),
    }
}

/// A string field, absent reading as `""` — the FlatBuffers default, and the one every
/// serializer here relies on when it leaves an empty string unwritten.
pub(crate) fn text(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
}

/// A scalar the delta serializers encode as `0` when unchanged (`capabilityFlags`,
/// `commandEventsRetentionTurns`): `0` is not a legal live value for either, so it reads as
/// `None`.
pub(crate) fn changed_scalar(value: u32) -> Option<u32> {
    (value != 0).then_some(value)
}

/// A vector of scalars, absent reading as empty.
pub(crate) fn decode_scalars<'a, T>(vector: Option<Vector<'a, T>>) -> Vec<T::Inner>
where
    T: Follow<'a> + 'a,
{
    vector.map(|v| v.iter().collect()).unwrap_or_default()
}

/// A vector of strings, absent reading as empty.
pub(crate) fn decode_strings<'a>(
    vector: Option<Vector<'a, ForwardsUOffset<&'a str>>>,
) -> Vec<String> {
    vector
        .map(|v| v.iter().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// A vector of tables decoded by an infallible row decoder, absent reading as empty.
pub(crate) fn map_rows<'a, T, U>(
    rows: Option<Vector<'a, ForwardsUOffset<T>>>,
    decode: impl FnMut(T::Inner) -> U,
) -> Vec<U>
where
    T: Follow<'a> + 'a,
{
    rows.map(|v| v.iter().map(decode).collect())
        .unwrap_or_default()
}

/// [`map_rows`], keeping the vector's presence: `None` is the delta's "unchanged".
pub(crate) fn map_rows_if_present<'a, T, U>(
    rows: Option<Vector<'a, ForwardsUOffset<T>>>,
    decode: impl FnMut(T::Inner) -> U,
) -> Option<Vec<U>>
where
    T: Follow<'a> + 'a,
{
    rows.map(|v| v.iter().map(decode).collect())
}

/// A vector of tables decoded by a fallible row decoder (one carrying an enum), absent reading
/// as empty.
pub(crate) fn decode_rows<'a, T, U>(
    rows: Option<Vector<'a, ForwardsUOffset<T>>>,
    decode: impl FnMut(T::Inner) -> Result<U, DecodeError>,
) -> Result<Vec<U>, DecodeError>
where
    T: Follow<'a> + 'a,
{
    rows.map(|v| v.iter().map(decode).collect())
        .unwrap_or_else(|| Ok(Vec::new()))
}

/// [`decode_rows`], keeping the vector's presence: `None` is the delta's "unchanged".
pub(crate) fn decode_rows_if_present<'a, T, U>(
    rows: Option<Vector<'a, ForwardsUOffset<T>>>,
    decode: impl FnMut(T::Inner) -> Result<U, DecodeError>,
) -> Result<Option<Vec<U>>, DecodeError>
where
    T: Follow<'a> + 'a,
{
    rows.map(|v| v.iter().map(decode).collect()).transpose()
}

pub(crate) fn decode_known_fragments(
    fragments: Option<Vector<'_, ForwardsUOffset<fb::KnownTechFragment<'_>>>>,
) -> Vec<KnownTechFragment> {
    map_rows(fragments, |fragment| KnownTechFragment {
        discovery_id: fragment.discoveryId(),
        progress: fragment.progress(),
        fidelity: fragment.fidelity(),
    })
}

pub(crate) fn decode_scalar_raster(raster: fb::ScalarRaster<'_>) -> ScalarRasterState {
    ScalarRasterState {
        width: raster.width(),
        height: raster.height(),
        samples: decode_scalars(raster.samples()),
    }
}

pub(crate) fn decode_float_raster(raster: fb::FloatRaster<'_>) -> FloatRasterState {
    FloatRasterState {
        width: raster.width(),
        height: raster.height(),
        samples: decode_scalars(raster.samples()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::connections::ConnectionState;
    use crate::state::culture::{CultureLayerScope, CultureTensionKind, CultureTensionState};
    use crate::state::knowledge::{KnowledgeTimelineEventKind, KnowledgeTimelineEventState};

    fn tension() -> CultureTensionState {
        CultureTensionState {
            layer_id: 1,
            scope: CultureLayerScope::Global,
            owner: 1,
            severity: 1,
            timer: 1,
            kind: CultureTensionKind::DriftWarning,
        }
    }

    fn connection() -> ConnectionState {
        ConnectionState {
            observer_band_id: 1,
            subject_band_id: 2,
            strength: 1.0,
            last_seen_x: 1,
            last_seen_y: 1,
            last_seen_turn: 1,
            last_contact_turn: 1,
            first_contact_turn: 1,
        }
    }

    fn timeline_event() -> KnowledgeTimelineEventState {
        KnowledgeTimelineEventState {
            tick: 1,
            kind: KnowledgeTimelineEventKind::LeakProgress,
            source_faction: 1,
            delta_percent: 1,
            note_handle: None,
        }
    }

    /// **The whole-section fields must be ABSENT on the wire when unchanged.**
    ///
    /// `cultureTensions` and `knowledgeTimeline` carry no removal vector, so an unconditionally
    /// written (possibly empty) vector makes "unchanged" and "the last row just went away"
    /// byte-identical and the receiver has to guess. Asserted on the encoded envelope rather than
    /// the `WorldDelta` struct because it is the *encoding* that has to preserve the distinction —
    /// the `Option` is worth nothing if the codec flattens it back out.
    #[test]
    fn an_unchanged_whole_section_is_absent_from_the_encoded_delta() {
        let bytes = encode_delta_flatbuffer(&WorldDelta::default());
        let envelope = fb::root_as_envelope(&bytes).expect("a decodable delta envelope");
        let delta = envelope.payload_as_delta().expect("a delta payload");

        assert!(
            delta
                .culture()
                .expect("a culture section")
                .cultureTensions()
                .is_none(),
            "an unchanged tension roster must not be written at all"
        );
        assert!(
            delta
                .knowledge()
                .expect("a knowledge section")
                .knowledgeTimeline()
                .is_none(),
            "an unchanged knowledge timeline must not be written at all"
        );
        assert!(
            delta
                .connections()
                .expect("a connection section")
                .connections()
                .is_none(),
            "an unchanged connection ledger must not be written at all"
        );
    }

    /// …and PRESENT, empty, when the section really did empty out — the case a receiver has to be
    /// able to tell apart from the one above.
    #[test]
    fn an_emptied_whole_section_is_present_but_empty_in_the_encoded_delta() {
        let emptied = WorldDelta {
            culture_tensions: Some(Vec::new()),
            knowledge_timeline: Some(Vec::new()),
            connections: Some(Vec::new()),
            ..Default::default()
        };
        let bytes = encode_delta_flatbuffer(&emptied);
        let envelope = fb::root_as_envelope(&bytes).expect("a decodable delta envelope");
        let delta = envelope.payload_as_delta().expect("a delta payload");

        let tensions = delta
            .culture()
            .expect("a culture section")
            .cultureTensions()
            .expect("an emptied tension roster must still be written");
        assert_eq!(tensions.len(), 0);

        let timeline = delta
            .knowledge()
            .expect("a knowledge section")
            .knowledgeTimeline()
            .expect("an emptied knowledge timeline must still be written");
        assert_eq!(timeline.len(), 0);

        let connections = delta
            .connections()
            .expect("a connection section")
            .connections()
            .expect("an emptied connection ledger must still be written");
        assert_eq!(connections.len(), 0);
    }

    /// A populated section rides through unchanged — the guard above must not be satisfiable by
    /// simply never writing the vector.
    #[test]
    fn a_changed_whole_section_carries_its_rows_in_the_encoded_delta() {
        let changed = WorldDelta {
            culture_tensions: Some(vec![tension()]),
            knowledge_timeline: Some(vec![timeline_event()]),
            connections: Some(vec![connection()]),
            ..Default::default()
        };
        let bytes = encode_delta_flatbuffer(&changed);
        let envelope = fb::root_as_envelope(&bytes).expect("a decodable delta envelope");
        let delta = envelope.payload_as_delta().expect("a delta payload");

        assert_eq!(
            delta
                .culture()
                .expect("a culture section")
                .cultureTensions()
                .expect("a changed tension roster is written")
                .len(),
            1
        );
        assert_eq!(
            delta
                .knowledge()
                .expect("a knowledge section")
                .knowledgeTimeline()
                .expect("a changed knowledge timeline is written")
                .len(),
            1
        );
        assert_eq!(
            delta
                .connections()
                .expect("a connection section")
                .connections()
                .expect("a changed connection ledger is written")
                .len(),
            1
        );
    }
}

/// **The definition of done for the decoder** (`docs/plan_ai_driver.md` §2): what the encoder
/// was given comes back, section by section, from the bytes it wrote. Three comparisons, because
/// the payloads derive no `PartialEq`: the serde JSON text, `hash_snapshot`, and — the strongest
/// — the re-encoded bytes, which catch a field decoded into the wrong slot as well as one dropped.
///
/// The snapshot under test is the **saturated fixture** (`crate::fixture`), the one world in the
/// repo with every section and every repeated field non-empty; its builder refuses to build with
/// an empty array anywhere, so a section this round trip does not cover is a section that does
/// not exist.
#[cfg(test)]
mod round_trip_tests {
    use super::*;
    use crate::fixture::saturated_snapshot;
    use crate::state::campaign::VictorySnapshotState;
    use crate::state::knowledge::KnowledgeLedgerEntryState;
    use crate::world::{encode_delta_json, encode_snapshot_json, hash_snapshot};

    /// A live capability bit and a live retention window, so both `0`-is-absent scalars are
    /// exercised on their `Some` side (`None` is the empty delta's job).
    const A_CAPABILITY_FLAG: u32 = 1;
    const A_RETENTION_WINDOW_TURNS: u32 = 20;

    /// What a full snapshot's header carries as its base: none.
    const NO_BASE_FRAME_SEQ: u64 = 0;

    /// **What the wire does not carry**, zeroed on the input so the comparison is about the codec
    /// and not about slots the encoder never writes:
    ///
    /// - a full snapshot's `header.base_frame_seq` is written as `0` whatever the struct holds
    ///   (`build_snapshot_flatbuffer`: a full frame names no base), so the saturated value cannot
    ///   come back;
    /// - `start_marker` has no FlatBuffers field at all (captured and diffed in `core_sim`, sent
    ///   nowhere);
    /// - a cohort's raw fixed-point `children`/`working`/`elders` are `(deprecated)` slots the
    ///   serializer stopped writing when the whole-people counts replaced them;
    /// - a culture layer's traits and divergence fields are not published (#386 — topology only).
    ///
    /// Everything else the saturated fixture populates goes through untouched.
    fn without_the_fields_the_wire_does_not_carry(mut snapshot: WorldSnapshot) -> WorldSnapshot {
        snapshot.header.base_frame_seq = NO_BASE_FRAME_SEQ;
        snapshot.start_marker = None;
        for cohort in &mut snapshot.populations {
            cohort.children = 0;
            cohort.working = 0;
            cohort.elders = 0;
        }
        for layer in &mut snapshot.culture_layers {
            layer.traits.clear();
            layer.divergence = 0;
            layer.soft_threshold = 0;
            layer.hard_threshold = 0;
            layer.ticks_above_soft = 0;
            layer.ticks_above_hard = 0;
            layer.last_updated_tick = 0;
        }
        snapshot
    }

    fn the_saturated_world() -> WorldSnapshot {
        without_the_fields_the_wire_does_not_carry(
            saturated_snapshot().expect("the fixture snapshot builds"),
        )
    }

    /// A delta carrying **every** field: each `Option` is `Some`, each vector — the `removed_*`
    /// lists included — is non-empty, taken from the saturated world so the rows themselves are
    /// fully populated. Written as an exhaustive literal so a field appended to `WorldDelta` fails
    /// here until the test says what it carries.
    fn a_delta_carrying_everything(world: &WorldSnapshot) -> WorldDelta {
        let mut header = world.header.clone();
        header.base_frame_seq = world.header.frame_seq;
        header.frame_seq = world.header.frame_seq + 1;
        WorldDelta {
            header,
            tiles: world.tiles.clone(),
            removed_tiles: world.tiles.iter().map(|tile| tile.entity).collect(),
            populations: world.populations.clone(),
            removed_populations: world.populations.iter().map(|c| c.entity).collect(),
            power: world.power.clone(),
            removed_power: world.power.iter().map(|node| node.entity).collect(),
            power_metrics: Some(world.power_metrics.clone()),
            great_discovery_definitions: Some(world.great_discovery_definitions.clone()),
            great_discoveries: world.great_discoveries.clone(),
            great_discovery_progress: world.great_discovery_progress.clone(),
            great_discovery_telemetry: Some(world.great_discovery_telemetry.clone()),
            knowledge_ledger: world.knowledge_ledger.clone(),
            // **The shipped key, not a bare discovery id.** A ledger row is named by
            // `knowledge_ledger_wire_key(owner_faction, discovery_id)` — the shape `core_sim`'s
            // producer and `WorldSnapshot::apply_delta` both use. The codec passes the `u64`
            // through opaquely, so a bare id would still round trip; it would just not be the
            // shape the wire carries.
            removed_knowledge_ledger: world
                .knowledge_ledger
                .iter()
                .map(KnowledgeLedgerEntryState::wire_key)
                .collect(),
            knowledge_metrics: Some(world.knowledge_metrics.clone()),
            victory: Some(world.victory.clone()),
            capability_flags: Some(A_CAPABILITY_FLAG),
            command_events: Some(world.command_events.clone()),
            command_events_retention_turns: Some(A_RETENTION_WINDOW_TURNS),
            campaign_profiles: Some(world.campaign_profiles.clone()),
            pending_forks: Some(world.pending_forks.clone()),
            stance_axes: Some(world.stance_axes.clone()),
            voice_medium: Some(world.voice_medium.clone()),
            opening_loadout: Some(world.opening_loadout.clone()),
            knowledge_timeline: Some(world.knowledge_timeline.clone()),
            crisis_telemetry: Some(world.crisis_telemetry.clone()),
            crisis_overlay: Some(world.crisis_overlay.clone()),
            herds: Some(world.herds.clone()),
            food_modules: Some(world.food_modules.clone()),
            faction_inventory: Some(world.faction_inventory.clone()),
            sedentarization: Some(world.sedentarization.clone()),
            discovered_sites: Some(world.discovered_sites.clone()),
            demographics: Some(world.demographics.clone()),
            forage_patches: Some(world.forage_patches.clone()),
            intensification_knowledge: Some(world.intensification_knowledge.clone()),
            ladder_knowledge: Some(world.ladder_knowledge.clone()),
            kits: Some(world.kits.clone()),
            default_hunt_kit_id: Some(world.default_hunt_kit_id.clone()),
            default_forage_kit_id: Some(world.default_forage_kit_id.clone()),
            default_scout_kit_id: Some(world.default_scout_kit_id.clone()),
            default_warrior_kit_id: Some(world.default_warrior_kit_id.clone()),
            default_expedition_kit_id: Some(world.default_expedition_kit_id.clone()),
            equipment_config_json: Some(world.equipment_config_json.clone()),
            materials: Some(world.materials.clone()),
            characteristic_bands: Some(world.characteristic_bands.clone()),
            recipes: Some(world.recipes.clone()),
            craft_knowledge: Some(world.craft_knowledge.clone()),
            route_rungs: Some(world.route_rungs.clone()),
            deposits: Some(world.deposits.clone()),
            deposit_rungs: Some(world.deposit_rungs.clone()),
            moisture_raster: Some(world.moisture_raster.clone()),
            elevation_overlay: Some(world.elevation_overlay.clone()),
            climate_bands: Some(world.climate_bands),
            temperature_survivability: Some(world.temperature_survivability),
            // No wire slot — see `without_the_fields_the_wire_does_not_carry`.
            start_marker: None,
            axis_bias: Some(world.axis_bias.clone()),
            sentiment: Some(world.sentiment.clone()),
            sentiment_raster: Some(world.sentiment_raster.clone()),
            corruption_raster: Some(world.corruption_raster.clone()),
            culture_raster: Some(world.culture_raster.clone()),
            military_raster: Some(world.military_raster.clone()),
            visibility_raster: Some(world.visibility_raster.clone()),
            fog_enabled: world.fog_enabled,
            generations: world.generations.clone(),
            removed_generations: world.generations.iter().map(|g| g.id).collect(),
            corruption: Some(world.corruption.clone()),
            influencers: world.influencers.clone(),
            removed_influencers: world.influencers.iter().map(|inf| inf.id).collect(),
            terrain: Some(world.terrain.clone()),
            culture_layers: world.culture_layers.clone(),
            removed_culture_layers: world.culture_layers.iter().map(|l| l.id).collect(),
            culture_tensions: Some(world.culture_tensions.clone()),
            discovery_progress: world.discovery_progress.clone(),
            connections: Some(world.connections.clone()),
            routes: Some(world.routes.clone()),
        }
    }

    /// Every whole-section `Option` on its **present-but-empty** side — the reading the encoder
    /// distinguishes from absence (`an_emptied_whole_section_is_present_but_empty_in_the_encoded_delta`)
    /// and the decoder therefore has to hand back as `Some(vec![])`, never `None`. The strings
    /// take their empty value for the same reason.
    fn a_delta_whose_whole_sections_all_emptied() -> WorldDelta {
        WorldDelta {
            power_metrics: Some(Default::default()),
            great_discovery_definitions: Some(Vec::new()),
            great_discovery_telemetry: Some(Default::default()),
            knowledge_metrics: Some(Default::default()),
            victory: Some(VictorySnapshotState::default()),
            command_events: Some(Vec::new()),
            campaign_profiles: Some(Vec::new()),
            pending_forks: Some(Vec::new()),
            stance_axes: Some(Vec::new()),
            voice_medium: Some(Vec::new()),
            opening_loadout: Some(Default::default()),
            knowledge_timeline: Some(Vec::new()),
            crisis_telemetry: Some(Default::default()),
            crisis_overlay: Some(Default::default()),
            herds: Some(Vec::new()),
            food_modules: Some(Vec::new()),
            faction_inventory: Some(Vec::new()),
            sedentarization: Some(Vec::new()),
            discovered_sites: Some(Vec::new()),
            demographics: Some(Vec::new()),
            forage_patches: Some(Vec::new()),
            intensification_knowledge: Some(Vec::new()),
            ladder_knowledge: Some(Vec::new()),
            kits: Some(Vec::new()),
            default_hunt_kit_id: Some(String::new()),
            default_forage_kit_id: Some(String::new()),
            default_scout_kit_id: Some(String::new()),
            default_warrior_kit_id: Some(String::new()),
            default_expedition_kit_id: Some(String::new()),
            equipment_config_json: Some(String::new()),
            materials: Some(Vec::new()),
            characteristic_bands: Some(Vec::new()),
            recipes: Some(Vec::new()),
            craft_knowledge: Some(Vec::new()),
            route_rungs: Some(Vec::new()),
            moisture_raster: Some(Default::default()),
            elevation_overlay: Some(Default::default()),
            climate_bands: Some(Default::default()),
            temperature_survivability: Some(Default::default()),
            axis_bias: Some(Default::default()),
            sentiment: Some(Default::default()),
            sentiment_raster: Some(Default::default()),
            corruption_raster: Some(Default::default()),
            culture_raster: Some(Default::default()),
            military_raster: Some(Default::default()),
            visibility_raster: Some(Default::default()),
            corruption: Some(Default::default()),
            terrain: Some(Default::default()),
            culture_tensions: Some(Vec::new()),
            connections: Some(Vec::new()),
            routes: Some(Vec::new()),
            ..Default::default()
        }
    }

    fn assert_snapshot_round_trips(world: &WorldSnapshot) {
        let bytes = encode_snapshot_flatbuffer(world);
        let decoded = decode_snapshot_flatbuffer(&bytes).expect("the encoder's bytes decode");
        assert_eq!(
            encode_snapshot_json(&decoded).expect("json"),
            encode_snapshot_json(world).expect("json"),
            "the decoded snapshot must serialize identically to the one encoded"
        );
        assert_eq!(hash_snapshot(&decoded), hash_snapshot(world));
        assert_eq!(
            encode_snapshot_flatbuffer(&decoded),
            bytes,
            "re-encoding the decoded snapshot must reproduce the bytes byte for byte"
        );
    }

    fn assert_delta_round_trips(delta: &WorldDelta) {
        let bytes = encode_delta_flatbuffer(delta);
        let decoded = decode_delta_flatbuffer(&bytes).expect("the encoder's bytes decode");
        assert_eq!(
            encode_delta_json(&decoded).expect("json"),
            encode_delta_json(delta).expect("json"),
            "the decoded delta must serialize identically to the one encoded"
        );
        assert_eq!(
            encode_delta_flatbuffer(&decoded),
            bytes,
            "re-encoding the decoded delta must reproduce the bytes byte for byte"
        );
    }

    /// ⛔ The saturated world survives encode → decode → encode with every section populated.
    #[test]
    fn the_saturated_snapshot_round_trips_through_the_flatbuffers_codec() {
        assert_snapshot_round_trips(&the_saturated_world());
    }

    /// A world with nothing in it survives too — the empty side of every vector and `Option`.
    #[test]
    fn an_empty_snapshot_round_trips_through_the_flatbuffers_codec() {
        assert_snapshot_round_trips(&WorldSnapshot::default());
    }

    /// ⛔ A delta carrying every field, every `removed_*` list included, comes back with each
    /// `Option` still `Some` and each vector still full.
    #[test]
    fn a_delta_carrying_every_field_round_trips_through_the_flatbuffers_codec() {
        assert_delta_round_trips(&a_delta_carrying_everything(&the_saturated_world()));
    }

    /// The unchanged delta: every `Option` `None`, every vector empty, the `0`-is-absent scalars
    /// at `0`, `serverBuild` left unwritten — and all of it read back as exactly that.
    #[test]
    fn an_unchanged_delta_round_trips_through_the_flatbuffers_codec() {
        assert_delta_round_trips(&WorldDelta::default());
    }

    /// The distinction the encoder was rebuilt to keep (`WorldDelta::culture_tensions`): a
    /// whole section that emptied out is `Some(vec![])` on the wire and must come back as such.
    #[test]
    fn an_emptied_whole_section_decodes_as_present_and_empty_not_as_unchanged() {
        assert_delta_round_trips(&a_delta_whose_whole_sections_all_emptied());
    }

    /// `decode_frame_flatbuffer` names the payload the union carries, and the two typed entry
    /// points refuse the other kind rather than decoding it as an empty world.
    #[test]
    fn the_envelope_decoder_returns_the_variant_the_union_names() {
        let world = the_saturated_world();
        let snapshot_bytes = encode_snapshot_flatbuffer(&world);
        let delta_bytes = encode_delta_flatbuffer(&a_delta_carrying_everything(&world));

        assert!(matches!(
            decode_frame_flatbuffer(&snapshot_bytes).expect("decodes"),
            FramePayload::Snapshot(_)
        ));
        assert!(matches!(
            decode_frame_flatbuffer(&delta_bytes).expect("decodes"),
            FramePayload::Delta(_)
        ));
        assert!(matches!(
            decode_snapshot_flatbuffer(&delta_bytes),
            Err(DecodeError::UnexpectedPayload {
                expected: SNAPSHOT_PAYLOAD_NAME,
                found: DELTA_PAYLOAD_NAME,
            })
        ));
        assert!(matches!(
            decode_delta_flatbuffer(&snapshot_bytes),
            Err(DecodeError::UnexpectedPayload {
                expected: DELTA_PAYLOAD_NAME,
                found: SNAPSHOT_PAYLOAD_NAME,
            })
        ));
    }

    /// Bytes that are not an envelope are refused by the verifier, not read as an empty world.
    #[test]
    fn bytes_that_are_not_an_envelope_are_an_error() {
        let not_a_frame = b"not a flatbuffer";
        assert!(matches!(
            decode_frame_flatbuffer(not_a_frame),
            Err(DecodeError::InvalidBuffer(_))
        ));
    }
}
