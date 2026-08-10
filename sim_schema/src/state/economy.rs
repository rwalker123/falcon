//! Economy-section state: faction inventories, and the knowledge-fragment payload the migration
//! path carries.
//!
//! The logistics- and trade-link states that used to live here went with the dead trade slice
//! (`docs/plan_contact_and_logistics.md` §As-built). Their `.fbs` tables and the two vector fields
//! that held them survive as `(deprecated)` slots — a freed field id is how two concurrent branches
//! collide on one position.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FactionInventoryEntryState {
    pub item: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FactionInventoryState {
    pub faction: u32,
    pub inventory: Vec<FactionInventoryEntryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KnownTechFragment {
    pub discovery_id: u32,
    pub progress: i64,
    pub fidelity: i64,
}
