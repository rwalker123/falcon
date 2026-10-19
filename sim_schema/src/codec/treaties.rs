//! Treaty-section FlatBuffers serialization — the treaties the viewer's people is party to and the
//! standing links its bands hold (`docs/plan_early_diplomacy.md` §3).

use crate::codec::dealings::{create_goods, decode_goods};
use crate::codec::{map_rows, map_rows_if_present, FbBuilder};
use crate::state::treaties::{StandingLinkState, TreatyBundleState, TreatyState, TreatyTermsState};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::WIPOffset;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

fn create_bundles<'a>(
    builder: &mut FbBuilder<'a>,
    bundles: &[TreatyBundleState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::TreatyBundleState<'a>>>> {
    let offsets: Vec<_> = bundles
        .iter()
        .map(|bundle| {
            let sent = create_goods(builder, &bundle.sent);
            let returned = create_goods(builder, &bundle.returned);
            fb::TreatyBundleState::create(
                builder,
                &fb::TreatyBundleStateArgs {
                    sent: Some(sent),
                    returned: Some(returned),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

/// A treaty's terms — shared with a dealing's proposal and an envoy party's errand.
pub(crate) fn create_terms<'a>(
    builder: &mut FbBuilder<'a>,
    terms: &TreatyTermsState,
) -> WIPOffset<fb::TreatyTermsState<'a>> {
    let bundles = create_bundles(builder, &terms.bundles);
    fb::TreatyTermsState::create(
        builder,
        &fb::TreatyTermsStateArgs {
            roadRights: terms.road_rights,
            openBorders: terms.open_borders,
            bundles: Some(bundles),
        },
    )
}

pub(crate) fn decode_terms(terms: Option<fb::TreatyTermsState<'_>>) -> TreatyTermsState {
    let Some(terms) = terms else {
        return TreatyTermsState::default();
    };
    TreatyTermsState {
        road_rights: terms.roadRights(),
        open_borders: terms.openBorders(),
        bundles: map_rows(terms.bundles(), |bundle| TreatyBundleState {
            sent: decode_goods(bundle.sent()),
            returned: decode_goods(bundle.returned()),
        }),
    }
}

fn create_treaties<'a>(
    builder: &mut FbBuilder<'a>,
    treaties: &[TreatyState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::TreatyState<'a>>>> {
    let offsets: Vec<_> = treaties
        .iter()
        .map(|treaty| {
            let terms = create_terms(builder, &treaty.terms);
            fb::TreatyState::create(
                builder,
                &fb::TreatyStateArgs {
                    otherFaction: treaty.other_faction,
                    terms: Some(terms),
                    formedTurn: treaty.formed_turn,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_links<'a>(
    builder: &mut FbBuilder<'a>,
    links: &[StandingLinkState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::StandingLinkState<'a>>>> {
    let offsets: Vec<_> = links
        .iter()
        .map(|link| {
            fb::StandingLinkState::create(
                builder,
                &fb::StandingLinkStateArgs {
                    viewerBand: link.viewer_band,
                    otherBand: link.other_band,
                    otherFaction: link.other_faction,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

pub(crate) fn serialize_treaties_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::TreatySection<'a>> {
    let treaties = create_treaties(builder, &snapshot.treaties);
    let standing_links = create_links(builder, &snapshot.standing_links);
    fb::TreatySection::create(
        builder,
        &fb::TreatySectionArgs {
            treaties: Some(treaties),
            standingLinks: Some(standing_links),
        },
    )
}

pub(crate) fn serialize_treaties_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::TreatySection<'a>> {
    // `None` means "unchanged this frame"; each list is re-sent whole when it moves.
    let treaties = delta
        .treaties
        .as_ref()
        .map(|treaties| create_treaties(builder, treaties));
    let standing_links = delta
        .standing_links
        .as_ref()
        .map(|links| create_links(builder, links));
    fb::TreatySection::create(
        builder,
        &fb::TreatySectionArgs {
            treaties,
            standingLinks: standing_links,
        },
    )
}

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` above.
// ---------------------------------------------------------------------------

fn decode_treaty(treaty: fb::TreatyState<'_>) -> TreatyState {
    TreatyState {
        other_faction: treaty.otherFaction(),
        terms: decode_terms(treaty.terms()),
        formed_turn: treaty.formedTurn(),
    }
}

fn decode_link(link: fb::StandingLinkState<'_>) -> StandingLinkState {
    StandingLinkState {
        viewer_band: link.viewerBand(),
        other_band: link.otherBand(),
        other_faction: link.otherFaction(),
    }
}

pub(crate) fn decode_treaties_section(
    section: fb::TreatySection<'_>,
    snapshot: &mut WorldSnapshot,
) {
    snapshot.treaties = map_rows(section.treaties(), decode_treaty);
    snapshot.standing_links = map_rows(section.standingLinks(), decode_link);
}

pub(crate) fn decode_treaties_section_delta(
    section: fb::TreatySection<'_>,
    delta: &mut WorldDelta,
) {
    // Absent is "unchanged this frame", exactly as the serializer left it.
    delta.treaties = map_rows_if_present(section.treaties(), decode_treaty);
    delta.standing_links = map_rows_if_present(section.standingLinks(), decode_link);
}
