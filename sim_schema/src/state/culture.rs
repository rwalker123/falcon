//! Culture-section state: culture layers, influential individuals, and sentiment.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CultureLayerScope {
    #[default]
    Global = 0,
    Regional = 1,
    Local = 2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum CultureTraitAxis {
    PassiveAggressive = 0,
    OpenClosed = 1,
    CollectivistIndividualist = 2,
    TraditionalistRevisionist = 3,
    HierarchicalEgalitarian = 4,
    SyncreticPurist = 5,
    AsceticIndulgent = 6,
    PragmaticIdealistic = 7,
    RationalistMystical = 8,
    ExpansionistInsular = 9,
    AdaptiveStubborn = 10,
    HonorBoundOpportunistic = 11,
    MeritOrientedLineageOriented = 12,
    SecularDevout = 13,
    PluralisticMonocultural = 14,
}

impl CultureTraitAxis {
    pub const ALL: [CultureTraitAxis; 15] = [
        CultureTraitAxis::PassiveAggressive,
        CultureTraitAxis::OpenClosed,
        CultureTraitAxis::CollectivistIndividualist,
        CultureTraitAxis::TraditionalistRevisionist,
        CultureTraitAxis::HierarchicalEgalitarian,
        CultureTraitAxis::SyncreticPurist,
        CultureTraitAxis::AsceticIndulgent,
        CultureTraitAxis::PragmaticIdealistic,
        CultureTraitAxis::RationalistMystical,
        CultureTraitAxis::ExpansionistInsular,
        CultureTraitAxis::AdaptiveStubborn,
        CultureTraitAxis::HonorBoundOpportunistic,
        CultureTraitAxis::MeritOrientedLineageOriented,
        CultureTraitAxis::SecularDevout,
        CultureTraitAxis::PluralisticMonocultural,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum CultureTensionKind {
    DriftWarning = 0,
    AssimilationPush = 1,
    SchismRisk = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CultureTraitEntry {
    pub axis: CultureTraitAxis,
    pub baseline: i64,
    pub modifier: i64,
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CultureLayerState {
    pub id: u32,
    pub owner: u64,
    pub parent: u32,
    pub scope: CultureLayerScope,
    pub traits: Vec<CultureTraitEntry>,
    pub divergence: i64,
    pub soft_threshold: i64,
    pub hard_threshold: i64,
    pub ticks_above_soft: u16,
    pub ticks_above_hard: u16,
    pub last_updated_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CultureTensionState {
    pub layer_id: u32,
    pub scope: CultureLayerScope,
    pub owner: u64,
    pub severity: i64,
    pub timer: u16,
    pub kind: CultureTensionKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceDomain {
    Sentiment = 0,
    Discovery = 1,
    Logistics = 2,
    Production = 3,
    Humanitarian = 4,
}

impl InfluenceDomain {
    pub fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

pub fn influence_domain_mask(domains: &[InfluenceDomain]) -> u32 {
    domains.iter().fold(0u32, |acc, domain| acc | domain.bit())
}

pub fn influence_domains_from_mask(mask: u32) -> Vec<InfluenceDomain> {
    let mut domains = Vec::new();
    for value in 0..=4 {
        let domain = match value {
            0 => InfluenceDomain::Sentiment,
            1 => InfluenceDomain::Discovery,
            2 => InfluenceDomain::Logistics,
            3 => InfluenceDomain::Production,
            4 => InfluenceDomain::Humanitarian,
            _ => continue,
        };
        if mask & domain.bit() != 0 {
            domains.push(domain);
        }
    }
    domains
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceScopeKind {
    Local = 0,
    Regional = 1,
    Global = 2,
    Generation = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum InfluenceLifecycle {
    Potential = 0,
    Active = 1,
    Dormant = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfluencerCultureResonanceEntry {
    pub axis: CultureTraitAxis,
    pub weight: i64,
    pub output: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfluentialIndividualState {
    pub id: u32,
    pub name: String,
    pub influence: i64,
    pub growth_rate: i64,
    pub baseline_growth: i64,
    pub notoriety: i64,
    pub sentiment_knowledge: i64,
    pub sentiment_trust: i64,
    pub sentiment_equity: i64,
    pub sentiment_agency: i64,
    pub sentiment_weight_knowledge: i64,
    pub sentiment_weight_trust: i64,
    pub sentiment_weight_equity: i64,
    pub sentiment_weight_agency: i64,
    pub logistics_bonus: i64,
    pub morale_bonus: i64,
    pub power_bonus: i64,
    pub logistics_weight: i64,
    pub morale_weight: i64,
    pub power_weight: i64,
    pub support_charge: i64,
    pub suppress_pressure: i64,
    pub domains: u32,
    pub scope: InfluenceScopeKind,
    pub generation_scope: u16,
    pub supported: bool,
    pub suppressed: bool,
    pub lifecycle: InfluenceLifecycle,
    pub coherence: i64,
    pub ticks_in_status: u16,
    pub audience_generations: Vec<u16>,
    pub support_popular: i64,
    pub support_peer: i64,
    pub support_institutional: i64,
    pub support_humanitarian: i64,
    pub weight_popular: i64,
    pub weight_peer: i64,
    pub weight_institutional: i64,
    pub weight_humanitarian: i64,
    pub culture_resonance: Vec<InfluencerCultureResonanceEntry>,
}

impl InfluentialIndividualState {
    pub const NO_GENERATION_SCOPE: u16 = u16::MAX;

    pub fn generation_scope(&self) -> Option<u16> {
        if self.generation_scope == Self::NO_GENERATION_SCOPE {
            None
        } else {
            Some(self.generation_scope)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AxisBiasState {
    pub knowledge: i64,
    pub trust: i64,
    pub equity: i64,
    pub agency: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum SentimentDriverCategory {
    Policy = 0,
    Incident = 1,
    Influencer = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SentimentDriverState {
    pub category: SentimentDriverCategory,
    pub label: String,
    pub value: i64,
    pub weight: i64,
}

impl Default for SentimentDriverState {
    fn default() -> Self {
        Self {
            category: SentimentDriverCategory::Policy,
            label: String::new(),
            value: 0,
            weight: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SentimentAxisTelemetry {
    pub policy: i64,
    pub incidents: i64,
    pub influencers: i64,
    pub total: i64,
    pub drivers: Vec<SentimentDriverState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SentimentTelemetryState {
    pub knowledge: SentimentAxisTelemetry,
    pub trust: SentimentAxisTelemetry,
    pub equity: SentimentAxisTelemetry,
    pub agency: SentimentAxisTelemetry,
}
