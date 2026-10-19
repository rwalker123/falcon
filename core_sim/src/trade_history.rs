//! **A trade history the sim keeps** (`docs/plan_early_diplomacy.md` §1, issue #233).
//!
//! The event feed keeps 20 turns; a player wants to see past trades. So each people has a list of
//! [`TradeRecord`]s, newest last, capped at `history.trade_history_len`. **One record per people per
//! trade, from that people's point of view**: a trade between two peoples writes a record into each, a
//! same-people shipment writes one. [`record_trade`] is the one seam every ending calls.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::BandId;
use crate::dealings::{DealingId, GoodsLines};
use crate::orders::FactionId;

/// How a trade ended, from one people's point of view (the wire's `outcome` token).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeOutcome {
    /// Goods both ways.
    Exchanged,
    /// This people gave goods and asked nothing back.
    GiftGiven,
    /// This people received goods and gave nothing back.
    GiftReceived,
    /// A same-people shipment landing under the standing answer.
    Delivered,
    /// The sender walked home from the return the receiver named.
    Declined,
    Refused,
    Expired,
    Withdrawn,
    Failed,
}

impl TradeOutcome {
    /// The wire's token.
    pub fn as_str(self) -> &'static str {
        match self {
            TradeOutcome::Exchanged => "exchanged",
            TradeOutcome::GiftGiven => "gift_given",
            TradeOutcome::GiftReceived => "gift_received",
            TradeOutcome::Delivered => "delivered",
            TradeOutcome::Declined => "declined",
            TradeOutcome::Refused => "refused",
            TradeOutcome::Expired => "expired",
            TradeOutcome::Withdrawn => "withdrawn",
            TradeOutcome::Failed => "failed",
        }
    }
}

/// How a trade ended, stated once from the SHIPPER's side; [`record_trade`] derives each people's own
/// [`TradeOutcome`] from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeEnd {
    /// Goods both ways.
    Exchanged,
    /// Goods one way (the shipper's), nothing back. Between one people this is `Delivered`.
    Gift,
    Declined,
    Refused,
    Expired,
    Withdrawn,
    Failed,
}

/// One past trade, from one people's point of view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeRecord {
    pub turn: u64,
    pub own_band: BandId,
    pub other_faction: FactionId,
    pub other_band: BandId,
    pub dealing: Option<DealingId>,
    pub outcome: TradeOutcome,
    /// What left this people — or, on a trade that did not happen, what this people was to give.
    pub goods_out: GoodsLines,
    /// What came in — or, on a trade that did not happen, what this people was to get.
    pub goods_in: GoodsLines,
}

/// **Every people's past trades**, oldest first. `SimState`.
#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TradeHistory {
    by_people: BTreeMap<FactionId, Vec<TradeRecord>>,
}

impl TradeHistory {
    /// Append a record to `faction`'s list, dropping the oldest while it is over `cap`.
    pub fn push(&mut self, faction: FactionId, record: TradeRecord, cap: usize) {
        let list = self.by_people.entry(faction).or_default();
        list.push(record);
        if list.len() > cap {
            let excess = list.len() - cap;
            list.drain(..excess);
        }
    }

    /// `faction`'s records, oldest first.
    pub fn of(&self, faction: FactionId) -> &[TradeRecord] {
        self.by_people.get(&faction).map_or(&[], Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.by_people.values().all(Vec::is_empty)
    }
}

/// A trade stated from the shipper's side.
#[derive(Debug, Clone)]
pub struct TradeFacts {
    pub turn: u64,
    pub dealing: Option<DealingId>,
    pub shipper_faction: FactionId,
    pub shipper_band: BandId,
    pub other_faction: FactionId,
    pub other_band: BandId,
    /// What the shipper sent (or was to send).
    pub sent: GoodsLines,
    /// What came back (or was to come back).
    pub back: GoodsLines,
}

/// **The one seam: write a finished trade into each people's history** — the shipper's record and the
/// other end's, or a single record when both ends are one people. `cap` is
/// `history.trade_history_len`.
pub fn record_trade(history: &mut TradeHistory, cap: usize, facts: &TradeFacts, end: TradeEnd) {
    let same_people = facts.shipper_faction == facts.other_faction;
    let (shipper_outcome, other_outcome) = match end {
        TradeEnd::Exchanged => (TradeOutcome::Exchanged, TradeOutcome::Exchanged),
        TradeEnd::Gift if same_people => (TradeOutcome::Delivered, TradeOutcome::Delivered),
        TradeEnd::Gift => (TradeOutcome::GiftGiven, TradeOutcome::GiftReceived),
        TradeEnd::Declined => (TradeOutcome::Declined, TradeOutcome::Declined),
        TradeEnd::Refused => (TradeOutcome::Refused, TradeOutcome::Refused),
        TradeEnd::Expired => (TradeOutcome::Expired, TradeOutcome::Expired),
        TradeEnd::Withdrawn => (TradeOutcome::Withdrawn, TradeOutcome::Withdrawn),
        TradeEnd::Failed => (TradeOutcome::Failed, TradeOutcome::Failed),
    };
    history.push(
        facts.shipper_faction,
        TradeRecord {
            turn: facts.turn,
            own_band: facts.shipper_band,
            other_faction: facts.other_faction,
            other_band: facts.other_band,
            dealing: facts.dealing,
            outcome: shipper_outcome,
            goods_out: facts.sent.clone(),
            goods_in: facts.back.clone(),
        },
        cap,
    );
    if !same_people {
        history.push(
            facts.other_faction,
            TradeRecord {
                turn: facts.turn,
                own_band: facts.other_band,
                other_faction: facts.shipper_faction,
                other_band: facts.shipper_band,
                dealing: facts.dealing,
                outcome: other_outcome,
                goods_out: facts.back.clone(),
                goods_in: facts.sent.clone(),
            },
            cap,
        );
    }
}
