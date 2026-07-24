//! FlatBuffers encoders for the world payloads.
//!
//! `build_snapshot_flatbuffer` / `build_delta_flatbuffer` assemble the envelope from the nine
//! per-domain section serializers in the sibling modules; helpers shared by two or more sections
//! live here.

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
mod culture;
mod economy;
mod governance;
mod knowledge;
mod map;
mod population;
mod subsistence;
mod vision;

use crate::codec::campaign::{
    create_campaign_label, create_victory_state, serialize_campaign_section,
    serialize_campaign_section_delta,
};
use crate::codec::culture::{serialize_culture_section, serialize_culture_section_delta};
use crate::codec::economy::{serialize_economy_section, serialize_economy_section_delta};
use crate::codec::governance::{serialize_governance_section, serialize_governance_section_delta};
use crate::codec::knowledge::{serialize_knowledge_section, serialize_knowledge_section_delta};
use crate::codec::map::{serialize_map_section, serialize_map_section_delta};
use crate::codec::population::{serialize_population_section, serialize_population_section_delta};
use crate::codec::subsistence::{
    serialize_subsistence_section, serialize_subsistence_section_delta,
};
use crate::codec::vision::{serialize_vision_section, serialize_vision_section_delta};
use crate::state::economy::KnownTechFragment;
use crate::state::map::{FloatRasterState, ScalarRasterState};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::{DefaultAllocator, FlatBufferBuilder, ForwardsUOffset, WIPOffset};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

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
            logisticsCount: snapshot.header.logistics_count,
            tradeLinkCount: snapshot.header.trade_link_count,
            populationCount: snapshot.header.population_count,
            powerCount: snapshot.header.power_count,
            influencerCount: snapshot.header.influencer_count,
            hash: snapshot.header.hash,
            campaignLabel: campaign_label_fb,
            victory: Some(victory_state),
            wrapHorizontal: snapshot.header.wrap_horizontal,
            serverBuild: Some(server_build_fb),
            worldEpoch: snapshot.header.world_epoch,
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
            logisticsCount: delta.header.logistics_count,
            tradeLinkCount: delta.header.trade_link_count,
            populationCount: delta.header.population_count,
            powerCount: delta.header.power_count,
            influencerCount: delta.header.influencer_count,
            hash: delta.header.hash,
            campaignLabel: campaign_label_fb,
            victory: victory_state,
            wrapHorizontal: delta.header.wrap_horizontal,
            serverBuild: server_build_fb,
            worldEpoch: delta.header.world_epoch,
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
