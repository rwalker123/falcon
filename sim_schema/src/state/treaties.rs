//! Treaty-section state: the treaties the viewer's people is party to, and the standing links that
//! let it propose one (`docs/plan_early_diplomacy.md` §3, issue #233).
//!
//! **Viewer-scoped**: a frame carries the treaties the viewer's people is party to and the standing
//! links between its own bands and other peoples' — another pair's treaties are theirs to know.

use serde::{Deserialize, Serialize};

use crate::state::dealings::DealingGoodsState;

/// **One standing exchange** of an agreed trade: a shipment carrying `sent` between the two peoples
/// is paid `returned` on arrival. Stated from the shipping people's point of view; the lines carry no
/// ratings (nothing has been drawn).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TreatyBundleState {
    pub sent: DealingGoodsState,
    pub returned: DealingGoodsState,
}

/// **The terms** a treaty holds or a proposal asks for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TreatyTermsState {
    pub road_rights: bool,
    pub open_borders: bool,
    pub bundles: Vec<TreatyBundleState>,
}

/// **One treaty the viewer's people is party to.**
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TreatyState {
    pub other_faction: u32,
    pub terms: TreatyTermsState,
    /// The turn it took effect (an amendment restarts it).
    pub formed_turn: u64,
}

/// **One standing link** between a band of the viewer's people and a band of another people — a
/// kept road on every tile between the two camps, which is what `propose_treaty` needs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StandingLinkState {
    pub viewer_band: u64,
    pub other_band: u64,
    pub other_faction: u32,
}
