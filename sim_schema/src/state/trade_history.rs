//! Trade-history state: what a people's past trades were
//! (`docs/plan_early_diplomacy.md` §1, issue #233).
//!
//! **Viewer-scoped**: a frame carries the viewer's own people's records only, oldest first.

use serde::{Deserialize, Serialize};

use crate::state::dealings::DealingGoodsState;

/// **One trade from the viewer's people's point of view** — an exchange, a gift, or a deal that
/// did not happen. The goods lists are the proposal-leg form: material rows carry no ratings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TradeRecordState {
    /// The turn the trade ended.
    pub turn: u64,
    /// The viewer's people's band in the trade (the sender's band, for a same-people shipment).
    pub own_band: u64,
    /// The other end's people (the viewer's own, for a same-people shipment).
    pub other_faction: u32,
    pub other_band: u64,
    /// The pending dealing it ran through; `0` = none.
    pub dealing_id: u64,
    /// `"exchanged"`, `"gift_given"`, `"gift_received"`, `"delivered"`, `"declined"`, `"refused"`,
    /// `"expired"`, `"withdrawn"` or `"failed"`.
    pub outcome: String,
    /// What left the viewer's people (what was offered or sent, on a trade that did not happen).
    pub goods_out: DealingGoodsState,
    /// What came in (what was asked or returned, on a trade that did not happen; empty if nothing).
    pub goods_in: DealingGoodsState,
}
