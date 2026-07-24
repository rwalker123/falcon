//! Economy-section state: logistics links, trade links, and faction inventories.

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogisticsLinkState {
    pub entity: u64,
    pub from: u64,
    pub to: u64,
    pub capacity: i64,
    pub flow: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TradeLinkKnowledge {
    pub openness: i64,
    pub leak_timer: u32,
    pub last_discovery: u32,
    pub decay: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TradeLinkState {
    pub entity: u64,
    pub from_faction: u32,
    pub to_faction: u32,
    pub throughput: i64,
    pub tariff: i64,
    pub knowledge: TradeLinkKnowledge,
    pub from_tile: u64,
    pub to_tile: u64,
    #[serde(default)]
    pub pending_fragments: Vec<KnownTechFragment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KnownTechFragment {
    pub discovery_id: u32,
    pub progress: i64,
    pub fidelity: i64,
}
