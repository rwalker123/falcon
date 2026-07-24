//! Knowledge-section state: the leak ledger, great discoveries, and discovered sites.

use serde::{Deserialize, Serialize};
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

/// One discovered Wondrous Site (position + catalog-resolved display fields) in a faction's
/// registry. Only sites the faction has revealed appear here — undiscovered sites never leave
/// the sim, so there is no fog leak.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DiscoveredSiteState {
    pub x: u32,
    pub y: u32,
    #[serde(default)]
    pub site_id: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub glyph: String,
}

/// Per-faction discovered-sites registry (mirrors `SedentarizationState`'s per-faction shape).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DiscoveredSitesState {
    pub faction: u32,
    #[serde(default)]
    pub sites: Vec<DiscoveredSiteState>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeField {
    #[default]
    Physics = 0,
    Chemistry = 1,
    Biology = 2,
    Data = 3,
    Communication = 4,
    Exotic = 5,
}

impl KnowledgeField {
    pub const VALUES: [KnowledgeField; 6] = [
        KnowledgeField::Physics,
        KnowledgeField::Chemistry,
        KnowledgeField::Biology,
        KnowledgeField::Data,
        KnowledgeField::Communication,
        KnowledgeField::Exotic,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeSecurityPosture {
    #[default]
    Minimal = 0,
    Standard = 1,
    Hardened = 2,
    BlackVault = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeCountermeasureKind {
    #[default]
    SecurityInvestment = 0,
    CounterIntelSweep = 1,
    Misinformation = 2,
    KnowledgeDebtRelief = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeModifierSource {
    #[default]
    Visibility = 0,
    Security = 1,
    Spycraft = 2,
    Culture = 3,
    Exposure = 4,
    Debt = 5,
    Treaty = 6,
    Event = 7,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KnowledgeTimelineEventKind {
    #[default]
    LeakProgress = 0,
    SpyProbe = 1,
    CounterIntel = 2,
    Exposure = 3,
    Treaty = 4,
    Cascade = 5,
    Digest = 6,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(transparent)]
pub struct KnowledgeLeakFlags(pub u32);

impl KnowledgeLeakFlags {
    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const COMMON_KNOWLEDGE: Self = Self(1 << 0);
    pub const FORCED_PUBLICATION: Self = Self(1 << 1);
    pub const CASCADE_PENDING: Self = Self(1 << 2);

    pub fn contains(self, rhs: Self) -> bool {
        (self.0 & rhs.0) == rhs.0
    }

    pub fn insert(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }

    pub fn remove(&mut self, rhs: Self) {
        self.0 &= !rhs.0;
    }

    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl BitOr for KnowledgeLeakFlags {
    type Output = KnowledgeLeakFlags;

    fn bitor(self, rhs: Self) -> Self::Output {
        KnowledgeLeakFlags(self.bits() | rhs.bits())
    }
}

impl BitOrAssign for KnowledgeLeakFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.bits();
    }
}

impl BitAnd for KnowledgeLeakFlags {
    type Output = KnowledgeLeakFlags;

    fn bitand(self, rhs: Self) -> Self::Output {
        KnowledgeLeakFlags(self.bits() & rhs.bits())
    }
}

impl BitAndAssign for KnowledgeLeakFlags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.bits();
    }
}

impl From<u32> for KnowledgeLeakFlags {
    fn from(value: u32) -> Self {
        KnowledgeLeakFlags::new(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeCountermeasureState {
    pub kind: KnowledgeCountermeasureKind,
    pub potency: i64,
    pub upkeep: i64,
    pub remaining_ticks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeInfiltrationState {
    pub faction: u32,
    pub blueprint_fidelity: i64,
    pub suspicion: i64,
    pub cells: u8,
    pub last_activity_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeModifierBreakdownState {
    pub source: KnowledgeModifierSource,
    pub delta_half_life: i16,
    pub delta_progress: i16,
    pub note_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeLedgerEntryState {
    pub discovery_id: u32,
    pub owner_faction: u32,
    pub tier: u8,
    pub progress_percent: u16,
    pub half_life_ticks: u16,
    pub time_to_cascade: u16,
    pub security_posture: KnowledgeSecurityPosture,
    pub countermeasures: Vec<KnowledgeCountermeasureState>,
    pub infiltrations: Vec<KnowledgeInfiltrationState>,
    pub modifiers: Vec<KnowledgeModifierBreakdownState>,
    pub flags: KnowledgeLeakFlags,
}

impl KnowledgeLedgerEntryState {
    pub fn has_flag(&self, flag: KnowledgeLeakFlags) -> bool {
        self.flags.contains(flag)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeMetricsState {
    pub leak_warnings: u32,
    pub leak_criticals: u32,
    pub countermeasures_active: u32,
    pub common_knowledge_total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KnowledgeTimelineEventState {
    pub tick: u64,
    pub kind: KnowledgeTimelineEventKind,
    pub source_faction: u32,
    pub delta_percent: i16,
    pub note_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GreatDiscoveryState {
    pub id: u16,
    pub faction: u32,
    pub field: KnowledgeField,
    pub tick: u64,
    pub publicly_deployed: bool,
    pub effect_flags: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GreatDiscoveryProgressState {
    pub faction: u32,
    pub discovery: u16,
    pub progress: i64,
    pub observation_deficit: u32,
    pub eta_ticks: u32,
    pub covert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GreatDiscoveryTelemetryState {
    pub total_resolved: u32,
    pub pending_candidates: u32,
    pub active_constellations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GreatDiscoveryRequirementState {
    pub discovery: u32,
    pub weight: f32,
    pub minimum_progress: f32,
    pub name: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GreatDiscoveryDefinitionState {
    pub id: u16,
    pub name: String,
    pub field: KnowledgeField,
    pub tier: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub observation_threshold: u32,
    pub cooldown_ticks: u16,
    pub freshness_window: Option<u16>,
    pub effect_flags: u32,
    pub covert_until_public: bool,
    pub effects_summary: Vec<String>,
    pub observation_notes: Option<String>,
    pub leak_profile: Option<String>,
    pub requirements: Vec<GreatDiscoveryRequirementState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DiscoveryProgressEntry {
    pub faction: u32,
    pub discovery: u32,
    pub progress: i64,
}
