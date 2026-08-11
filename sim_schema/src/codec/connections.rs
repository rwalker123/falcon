//! Connection-section FlatBuffers serialization — the directed ties contact leaves behind.

use crate::codec::FbBuilder;
use crate::state::connections::ConnectionState;
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::WIPOffset;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

fn create_connections<'a>(
    builder: &mut FbBuilder<'a>,
    connections: &[ConnectionState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::ConnectionState<'a>>>> {
    let offsets: Vec<_> = connections
        .iter()
        .map(|connection| {
            fb::ConnectionState::create(
                builder,
                &fb::ConnectionStateArgs {
                    observerBandId: connection.observer_band_id,
                    subjectBandId: connection.subject_band_id,
                    strength: connection.strength,
                    lastSeenX: connection.last_seen_x,
                    lastSeenY: connection.last_seen_y,
                    lastSeenTurn: connection.last_seen_turn,
                    lastContactTurn: connection.last_contact_turn,
                    firstContactTurn: connection.first_contact_turn,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

pub(crate) fn serialize_connection_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::ConnectionSection<'a>> {
    let connections = create_connections(builder, &snapshot.connections);
    fb::ConnectionSection::create(
        builder,
        &fb::ConnectionSectionArgs {
            connections: Some(connections),
        },
    )
}

pub(crate) fn serialize_connection_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::ConnectionSection<'a>> {
    // `None` means "unchanged this frame"; the whole vector is re-sent when any edge moves, like
    // every other `Whole` section.
    let connections = delta
        .connections
        .as_ref()
        .map(|connections| create_connections(builder, connections));
    fb::ConnectionSection::create(builder, &fb::ConnectionSectionArgs { connections })
}
