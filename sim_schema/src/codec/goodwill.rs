//! Goodwill-section FlatBuffers serialization — what other peoples are known to think of the viewer.

use crate::codec::{map_rows, map_rows_if_present, FbBuilder};
use crate::state::goodwill::{GoodwillCutsState, GoodwillState};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::WIPOffset;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

fn create_rows<'a>(
    builder: &mut FbBuilder<'a>,
    rows: &[GoodwillState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::GoodwillState<'a>>>> {
    let offsets: Vec<_> = rows
        .iter()
        .map(|row| {
            fb::GoodwillState::create(
                builder,
                &fb::GoodwillStateArgs {
                    otherFaction: row.other_faction,
                    gifts: row.gifts,
                    exchanges: row.exchanges,
                    stationed: row.stationed,
                    link: row.link,
                    treaty: row.treaty,
                    breach: row.breach,
                    total: row.total,
                    reportedTurn: row.reported_turn,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_cuts<'a>(
    builder: &mut FbBuilder<'a>,
    cuts: &GoodwillCutsState,
) -> WIPOffset<fb::GoodwillCutsState<'a>> {
    fb::GoodwillCutsState::create(
        builder,
        &fb::GoodwillCutsStateArgs {
            waryBelow: cuts.wary_below,
            friendlyAbove: cuts.friendly_above,
        },
    )
}

pub(crate) fn serialize_goodwill_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::GoodwillSection<'a>> {
    let goodwill = create_rows(builder, &snapshot.goodwill);
    let cuts = create_cuts(builder, &snapshot.goodwill_cuts);
    let own = create_rows(builder, &snapshot.own_goodwill);
    fb::GoodwillSection::create(
        builder,
        &fb::GoodwillSectionArgs {
            goodwill: Some(goodwill),
            cuts: Some(cuts),
            own: Some(own),
        },
    )
}

pub(crate) fn serialize_goodwill_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::GoodwillSection<'a>> {
    // `None` means "unchanged this frame" for the rows and the cuts alike.
    let goodwill = delta
        .goodwill
        .as_ref()
        .map(|rows| create_rows(builder, rows));
    let cuts = delta
        .goodwill_cuts
        .as_ref()
        .map(|cuts| create_cuts(builder, cuts));
    let own = delta
        .own_goodwill
        .as_ref()
        .map(|rows| create_rows(builder, rows));
    fb::GoodwillSection::create(
        builder,
        &fb::GoodwillSectionArgs {
            goodwill,
            cuts,
            own,
        },
    )
}

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` above.
// ---------------------------------------------------------------------------

pub(crate) fn decode_goodwill_section(
    section: fb::GoodwillSection<'_>,
    snapshot: &mut WorldSnapshot,
) {
    snapshot.goodwill = map_rows(section.goodwill(), decode_row);
    snapshot.goodwill_cuts = section.cuts().map(decode_cuts).unwrap_or_default();
    snapshot.own_goodwill = map_rows(section.own(), decode_row);
}

pub(crate) fn decode_goodwill_section_delta(
    section: fb::GoodwillSection<'_>,
    delta: &mut WorldDelta,
) {
    delta.goodwill = map_rows_if_present(section.goodwill(), decode_row);
    delta.goodwill_cuts = section.cuts().map(decode_cuts);
    delta.own_goodwill = map_rows_if_present(section.own(), decode_row);
}

fn decode_cuts(cuts: fb::GoodwillCutsState<'_>) -> GoodwillCutsState {
    GoodwillCutsState {
        wary_below: cuts.waryBelow(),
        friendly_above: cuts.friendlyAbove(),
    }
}

fn decode_row(row: fb::GoodwillState<'_>) -> GoodwillState {
    GoodwillState {
        other_faction: row.otherFaction(),
        gifts: row.gifts(),
        exchanges: row.exchanges(),
        stationed: row.stationed(),
        link: row.link(),
        treaty: row.treaty(),
        breach: row.breach(),
        total: row.total(),
        reported_turn: row.reportedTurn(),
    }
}
