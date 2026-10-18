//! Trade-history-section FlatBuffers serialization — a people's past trades.

use crate::codec::dealings::{create_goods, decode_goods};
use crate::codec::{map_rows, map_rows_if_present, text, FbBuilder};
use crate::state::trade_history::TradeRecordState;
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::WIPOffset;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

fn create_records<'a>(
    builder: &mut FbBuilder<'a>,
    records: &[TradeRecordState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::TradeRecordState<'a>>>> {
    let offsets: Vec<_> = records
        .iter()
        .map(|record| {
            let outcome = builder.create_string(&record.outcome);
            let goods_out = create_goods(builder, &record.goods_out);
            let goods_in = create_goods(builder, &record.goods_in);
            fb::TradeRecordState::create(
                builder,
                &fb::TradeRecordStateArgs {
                    turn: record.turn,
                    ownBand: record.own_band,
                    otherFaction: record.other_faction,
                    otherBand: record.other_band,
                    dealingId: record.dealing_id,
                    outcome: Some(outcome),
                    goodsOut: Some(goods_out),
                    goodsIn: Some(goods_in),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

pub(crate) fn serialize_trade_history_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::TradeHistorySection<'a>> {
    let records = create_records(builder, &snapshot.trade_history);
    fb::TradeHistorySection::create(
        builder,
        &fb::TradeHistorySectionArgs {
            records: Some(records),
        },
    )
}

pub(crate) fn serialize_trade_history_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::TradeHistorySection<'a>> {
    // `None` means "unchanged this frame"; the whole list is re-sent when it changes.
    let records = delta
        .trade_history
        .as_ref()
        .map(|records| create_records(builder, records));
    fb::TradeHistorySection::create(builder, &fb::TradeHistorySectionArgs { records })
}

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` above.
// ---------------------------------------------------------------------------

fn decode_record(record: fb::TradeRecordState<'_>) -> TradeRecordState {
    TradeRecordState {
        turn: record.turn(),
        own_band: record.ownBand(),
        other_faction: record.otherFaction(),
        other_band: record.otherBand(),
        dealing_id: record.dealingId(),
        outcome: text(record.outcome()),
        goods_out: decode_goods(record.goodsOut()),
        goods_in: decode_goods(record.goodsIn()),
    }
}

pub(crate) fn decode_trade_history_section(
    section: fb::TradeHistorySection<'_>,
    snapshot: &mut WorldSnapshot,
) {
    snapshot.trade_history = map_rows(section.records(), decode_record);
}

pub(crate) fn decode_trade_history_section_delta(
    section: fb::TradeHistorySection<'_>,
    delta: &mut WorldDelta,
) {
    // Absent is "unchanged this frame", exactly as the serializer left it.
    delta.trade_history = map_rows_if_present(section.records(), decode_record);
}
