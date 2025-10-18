//! Shared serialization schemas and data definitions for the headless
//! simulation prototype. These types are consumed by both the core simulation
//! crate and external clients such as the CLI inspector.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotHeader {
    pub tick: u64,
    pub entity_count: u32,
    pub hash: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPayload {
    pub header: SnapshotHeader,
    pub entities: Vec<EntityRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRecord {
    pub id: u64,
    pub components: Vec<ComponentRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRecord {
    pub name: String,
    pub data: Vec<u8>,
}

impl SnapshotPayload {
    pub fn empty() -> Self {
        Self {
            header: SnapshotHeader { tick: 0, entity_count: 0, hash: 0 },
            entities: Vec::new(),
        }
    }
}
