//! Dealings-section state: the pending offers between peoples
//! (`docs/plan_early_diplomacy.md` §1, issue #233).
//!
//! **Viewer-scoped**: a frame carries the dealings addressed to the viewer's people and the ones the
//! viewer's people sent — another pair's dealings are theirs to know.

use serde::{Deserialize, Serialize};

use crate::state::population::MaterialBatchState;

/// **A goods list** — food, hay and material batches, the shipment's own cargo representation.
/// Never a summed value: food and hay are two accounts that never convert, and each material keeps
/// every rating (the band store's [`MaterialBatchState`] row, readings and all).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DealingGoodsState {
    pub food: f32,
    pub fodder: f32,
    pub materials: Vec<MaterialBatchState>,
}

/// **One pending dealing** — addressed to a people, answered by `answer_dealing`, expiring on
/// silence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DealingState {
    /// Monotonic, never reused; the key `answer_dealing` addresses it by.
    pub id: u64,
    /// `"offer"` today. Later kinds are new tokens.
    pub kind: String,
    pub sender_faction: u32,
    /// The band that sent it (the party's home band) — a durable id a client swaps its own label in
    /// for.
    pub sender_band: u64,
    pub receiver_faction: u32,
    pub receiver_band: u64,
    /// What was sent.
    pub goods_sent: DealingGoodsState,
    /// What is asked or returned. Empty on a stage-2 offer: the receiver names the return in its
    /// answer.
    pub goods_asked: DealingGoodsState,
    /// How many resolves it stays open for, counting the next one: `1` = answer this turn or
    /// silence refuses it.
    pub turns_left: u32,
}
