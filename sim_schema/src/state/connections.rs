//! Connection-section state: the directed, decaying ties contact leaves behind.
//!
//! `docs/plan_contact_and_logistics.md` §Q2. A connection is a **raw primitive** — it knows nothing
//! about goods, culture or knowledge, and this contract must stay that way: a rider's field here
//! (a tariff, an openness, a route) is how the retired `TradeLink` stopped being an edge. Faction is
//! a property of the endpoints and never a column on the row.

use serde::{Deserialize, Serialize};

/// One directed tie: `observer_band_id` knows `subject_band_id`. The reverse edge is a separate row.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub struct ConnectionState {
    pub observer_band_id: u64,
    pub subject_band_id: u64,
    /// Clock 2, `0..=1`. **Zero is a parked tie, not an absent one** — the row is still published.
    pub strength: f32,
    /// Clock 1 — where the subject was the last time the observer actually saw them…
    pub last_seen_x: u32,
    pub last_seen_y: u32,
    /// …and on which turn. Neither moves on a turn without contact.
    pub last_seen_turn: u64,
    /// The turn the tie was last refreshed by contact; drives clocks 2 and 3.
    pub last_contact_turn: u64,
    /// The turn the tie first formed. Never changes.
    pub first_contact_turn: u64,
}
