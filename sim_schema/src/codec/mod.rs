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

#[cfg(test)]
mod tests {
    use super::*;
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
    }

    /// …and PRESENT, empty, when the section really did empty out — the case a receiver has to be
    /// able to tell apart from the one above.
    #[test]
    fn an_emptied_whole_section_is_present_but_empty_in_the_encoded_delta() {
        let emptied = WorldDelta {
            culture_tensions: Some(Vec::new()),
            knowledge_timeline: Some(Vec::new()),
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
    }

    /// A populated section rides through unchanged — the guard above must not be satisfiable by
    /// simply never writing the vector.
    #[test]
    fn a_changed_whole_section_carries_its_rows_in_the_encoded_delta() {
        let changed = WorldDelta {
            culture_tensions: Some(vec![tension()]),
            knowledge_timeline: Some(vec![timeline_event()]),
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
    }
}
