//! Economy-section state: faction inventories, and the knowledge-fragment payload the migration
//! path carries.
//!
//! The logistics- and trade-link states that used to live here went with the dead trade slice
//! (`docs/plan_contact_and_logistics.md` §As-built), and their `.fbs` tables are deleted from the
//! schema.

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
}
