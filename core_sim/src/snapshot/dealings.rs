use super::*;

use crate::{
    dealings::{Dealing, GoodsLines, PendingDealings},
    materials_config::MaterialsConfig,
    LocalStore, FODDER, FOOD,
};
use sim_runtime::{DealingGoodsState, MaterialBatchState};

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
    party_terms: &std::collections::HashMap<crate::components::BandId, &'static str>,
) -> Vec<DealingState> {
    pending
        .visible_to(viewer)
        .map(|dealing| dealing_state(dealing, now, materials, party_terms))
        .collect()
}

fn dealing_state(
    dealing: &Dealing,
    now: u64,
    materials: &MaterialsConfig,
    party_terms: &std::collections::HashMap<crate::components::BandId, &'static str>,
) -> DealingState {
    // An offer states its goods as the shipment's own rated store; a proposal or counter states its
    // two legs as unrated lines, from the shipper's point of view.
    // An offer's party is the one waiting in the camp; a proposal's or counter's is the shipper's.
    let (goods_sent, goods_asked, shipper_band, party_workers) = match &dealing.agreement {
        Some(agreement) => (
            lines_state(&agreement.give),
            lines_state(&agreement.ask),
            agreement.shipper_band.0,
            agreement.party_workers,
        ),
        None => (
            goods_state(&dealing.goods_sent, materials),
            goods_state(&dealing.goods_asked, materials),
            0,
            dealing.party_workers,
        ),
    };
    DealingState {
        id: dealing.id.0,
        kind: dealing.kind.as_str().to_string(),
        sender_faction: dealing.sender_faction.0,
        sender_band: dealing.sender_band.0,
        receiver_faction: dealing.receiver_faction.0,
        receiver_band: dealing.receiver_band.0,
        goods_sent,
        goods_asked,
        turns_left: dealing.turns_left(now),
        shipper_band,
        party_workers,
        // A treaty proposal states its full terms and who carries it; every other kind leaves both
        // empty.
        treaty: dealing
            .treaty
            .as_ref()
            .map(|proposal| crate::snapshot::treaties::terms_state(&proposal.terms))
            .unwrap_or_default(),
        party_band: dealing.party.map_or(0, |party| party.0),
        // Only an offer-kind dealing carries terms (a reply is already past the receiver's naming).
        terms: match (dealing.kind, dealing.party) {
            (crate::dealings::DealingKind::Offer, Some(party)) => {
                party_terms.get(&party).copied().unwrap_or("").to_string()
            }
            _ => String::new(),
        },
        carrier: dealing
            .treaty
            .as_ref()
            .map(|proposal| proposal.carrier.as_str().to_string())
            .unwrap_or_default(),
    }
}

/// **A leg named as lines** — food, hay and one row per material with its total and no ratings, since
/// nothing has been drawn to be rated yet.
pub(crate) fn lines_state(lines: &GoodsLines) -> DealingGoodsState {
    DealingGoodsState {
        food: lines.food.to_f32(),
        fodder: lines.fodder.to_f32(),
        materials: lines
            .materials
            .iter()
            .map(|(material, amount)| MaterialBatchState {
                material_id: material.clone(),
                amount: amount.to_f32(),
                ..Default::default()
            })
            .collect(),
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
