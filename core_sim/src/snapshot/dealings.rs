use super::*;

use crate::{
    dealings::{Dealing, PendingDealings},
    materials_config::MaterialsConfig,
    LocalStore, FODDER, FOOD,
};
use sim_runtime::DealingGoodsState;

/// **The pending dealings one viewer's frame carries** (`docs/plan_early_diplomacy.md` §1): the ones
/// addressed to the viewer's people and the ones the viewer's people sent. Another pair's dealings
/// are theirs to know.
///
/// Order is the book's id order (a `BTreeMap`), so the section is stable frame to frame and diffs out
/// when nothing moved. `now` is the tick this frame is captured at, which is what `turns_left`
/// counts from.
pub(crate) fn dealing_states(
    pending: &PendingDealings,
    viewer: FactionId,
    now: u64,
    materials: &MaterialsConfig,
) -> Vec<DealingState> {
    pending
        .visible_to(viewer)
        .map(|dealing| dealing_state(dealing, now, materials))
        .collect()
}

fn dealing_state(dealing: &Dealing, now: u64, materials: &MaterialsConfig) -> DealingState {
    DealingState {
        id: dealing.id.0,
        kind: dealing.kind.as_str().to_string(),
        sender_faction: dealing.sender_faction.0,
        sender_band: dealing.sender_band.0,
        receiver_faction: dealing.receiver_faction.0,
        receiver_band: dealing.receiver_band.0,
        goods_sent: goods_state(&dealing.goods_sent, materials),
        goods_asked: goods_state(&dealing.goods_asked, materials),
        turns_left: dealing.turns_left(now),
    }
}

/// **A goods list, as the shipment holds it** — food and hay as two amounts, and every material
/// batch at its own rating (the band store's row shape, readings and all). Never a summed value.
fn goods_state(store: &LocalStore, materials: &MaterialsConfig) -> DealingGoodsState {
    DealingGoodsState {
        food: store.get(FOOD).to_f32(),
        fodder: store.get(FODDER).to_f32(),
        materials: crate::snapshot::crafting::material_batches(store, materials),
    }
}
