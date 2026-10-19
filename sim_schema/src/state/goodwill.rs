//! Goodwill-section state: what other peoples are known to think of the viewer's
//! (`docs/plan_early_diplomacy.md` §2, issue #233).
//!
//! **Viewer-scoped and never live**: a row is another people's opinion of the viewer's, as last
//! reported by a party that was in contact with it, stamped with the turn it was observed.

use serde::{Deserialize, Serialize};

/// One other people's opinion of the viewer's, as its reasons. `total` is their sum, clamped to
/// `-1..=+1`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GoodwillState {
    pub other_faction: u32,
    pub gifts: f32,
    pub exchanges: f32,
    pub stationed: f32,
    pub link: f32,
    pub treaty: f32,
    pub breach: f32,
    pub total: f32,
    /// The turn the opinion was observed — when the party was in contact, not when it got home.
    pub reported_turn: u64,
}

/// **The word cuts**, published once so the client never restates them: below `wary_below` reads
/// *wary*, above `friendly_above` *friendly*, between *neutral*.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GoodwillCutsState {
    pub wary_below: f32,
    pub friendly_above: f32,
}
