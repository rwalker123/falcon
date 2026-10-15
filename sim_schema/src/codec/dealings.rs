//! Dealings-section FlatBuffers serialization — the pending offers between peoples.

use crate::codec::{map_rows, map_rows_if_present, text, FbBuilder};
use crate::state::dealings::{DealingGoodsState, DealingState};
use crate::state::population::{CharacteristicReadingState, MaterialBatchState};
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::WIPOffset;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

fn create_goods<'a>(
    builder: &mut FbBuilder<'a>,
    goods: &DealingGoodsState,
) -> WIPOffset<fb::DealingGoodsState<'a>> {
    let batches: Vec<_> = goods
        .materials
        .iter()
        .map(|batch| {
            let material_id = builder.create_string(&batch.material_id);
            let variety_name = builder.create_string(&batch.variety_name);
            let readings: Vec<_> = batch
                .readings
                .iter()
                .map(|reading| {
                    let axis = builder.create_string(&reading.axis);
                    let band_name = builder.create_string(&reading.band_name);
                    fb::CharacteristicReading::create(
                        builder,
                        &fb::CharacteristicReadingArgs {
                            axis: Some(axis),
                            value: reading.value,
                            bandName: Some(band_name),
                        },
                    )
                })
                .collect();
            let readings = builder.create_vector(&readings);
            fb::MaterialBatchState::create(
                builder,
                &fb::MaterialBatchStateArgs {
                    materialId: Some(material_id),
                    amount: batch.amount,
                    readings: Some(readings),
                    varietyName: Some(variety_name),
                },
            )
        })
        .collect();
    let materials = builder.create_vector(&batches);
    fb::DealingGoodsState::create(
        builder,
        &fb::DealingGoodsStateArgs {
            food: goods.food,
            fodder: goods.fodder,
            materials: Some(materials),
        },
    )
}

fn create_dealings<'a>(
    builder: &mut FbBuilder<'a>,
    dealings: &[DealingState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::DealingState<'a>>>> {
    let offsets: Vec<_> = dealings
        .iter()
        .map(|dealing| {
            let kind = builder.create_string(&dealing.kind);
            let goods_sent = create_goods(builder, &dealing.goods_sent);
            let goods_asked = create_goods(builder, &dealing.goods_asked);
            fb::DealingState::create(
                builder,
                &fb::DealingStateArgs {
                    id: dealing.id,
                    kind: Some(kind),
                    senderFaction: dealing.sender_faction,
                    senderBand: dealing.sender_band,
                    receiverFaction: dealing.receiver_faction,
                    receiverBand: dealing.receiver_band,
                    goodsSent: Some(goods_sent),
                    goodsAsked: Some(goods_asked),
                    turnsLeft: dealing.turns_left,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

pub(crate) fn serialize_dealings_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::DealingsSection<'a>> {
    let dealings = create_dealings(builder, &snapshot.dealings);
    fb::DealingsSection::create(
        builder,
        &fb::DealingsSectionArgs {
            dealings: Some(dealings),
        },
    )
}

pub(crate) fn serialize_dealings_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::DealingsSection<'a>> {
    // `None` means "unchanged this frame"; the whole list is re-sent when any row moves, like every
    // other `Whole` section.
    let dealings = delta
        .dealings
        .as_ref()
        .map(|dealings| create_dealings(builder, dealings));
    fb::DealingsSection::create(builder, &fb::DealingsSectionArgs { dealings })
}

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` above, in the same order.
// ---------------------------------------------------------------------------

pub(crate) fn decode_dealings_section(
    section: fb::DealingsSection<'_>,
    snapshot: &mut WorldSnapshot,
) {
    snapshot.dealings = map_rows(section.dealings(), decode_dealing);
}

pub(crate) fn decode_dealings_section_delta(
    section: fb::DealingsSection<'_>,
    delta: &mut WorldDelta,
) {
    // Absent is "unchanged this frame", exactly as the serializer left it.
    delta.dealings = map_rows_if_present(section.dealings(), decode_dealing);
}

fn decode_goods(goods: Option<fb::DealingGoodsState<'_>>) -> DealingGoodsState {
    let Some(goods) = goods else {
        return DealingGoodsState::default();
    };
    DealingGoodsState {
        food: goods.food(),
        fodder: goods.fodder(),
        materials: map_rows(goods.materials(), |batch| MaterialBatchState {
            material_id: text(batch.materialId()),
            amount: batch.amount(),
            readings: map_rows(batch.readings(), |reading| CharacteristicReadingState {
                axis: text(reading.axis()),
                value: reading.value(),
                band_name: text(reading.bandName()),
            }),
            variety_name: text(batch.varietyName()),
        }),
    }
}

fn decode_dealing(dealing: fb::DealingState<'_>) -> DealingState {
    DealingState {
        id: dealing.id(),
        kind: text(dealing.kind()),
        sender_faction: dealing.senderFaction(),
        sender_band: dealing.senderBand(),
        receiver_faction: dealing.receiverFaction(),
        receiver_band: dealing.receiverBand(),
        goods_sent: decode_goods(dealing.goodsSent()),
        goods_asked: decode_goods(dealing.goodsAsked()),
        turns_left: dealing.turnsLeft(),
    }
}
