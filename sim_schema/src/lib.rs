use ahash::RandomState;
use flatbuffers::{DefaultAllocator, FlatBufferBuilder, ForwardsUOffset, WIPOffset};
use serde::{Deserialize, Serialize};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use std::{
    hash::{BuildHasher, Hash, Hasher},
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign},
};

type FbBuilder<'a> = FlatBufferBuilder<'a, DefaultAllocator>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignLabel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_loc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_loc_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignProfileState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_loc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_loc_key: Option<String>,
    #[serde(default)]
    pub starting_units: Vec<CampaignStartingUnitState>,
    #[serde(default)]
    pub inventory: Vec<CampaignInventoryEntryState>,
    #[serde(default)]
    pub knowledge_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub survey_radius: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_food_module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_food_module: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignInventoryEntryState {
    pub item: String,
    pub quantity: i64,
}

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
pub struct HerdTelemetryState {
    pub id: String,
    pub label: String,
    pub species: String,
    pub x: u32,
    pub y: u32,
    pub biomass: f32,
    pub route_length: u32,
    pub next_x: i32,
    pub next_y: i32,
}

impl Default for HerdTelemetryState {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            species: String::new(),
            x: 0,
            y: 0,
            biomass: 0.0,
            route_length: 0,
            next_x: -1,
            next_y: -1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FoodModuleState {
    pub x: u32,
    pub y: u32,
    pub module: String,
    pub seasonal_weight: f32,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CommandEventState {
    pub tick: u64,
    pub kind: String,
    pub faction: u32,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignStartingUnitState {
    pub kind: String,
    pub count: u32,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictoryModeSnapshotState {
    pub id: String,
    pub kind: String,
    pub progress: f32,
    pub threshold: f32,
    pub achieved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictoryResultState {
    pub mode: String,
    pub faction: u32,
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictorySnapshotState {
    #[serde(default)]
    pub modes: Vec<VictoryModeSnapshotState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner: Option<VictoryResultState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotHeader {
    pub tick: u64,
    pub tile_count: u32,
    pub logistics_count: u32,
    pub trade_link_count: u32,
    pub population_count: u32,
    pub power_count: u32,
    pub influencer_count: u32,
    pub hash: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign_label: Option<CampaignLabel>,
}

impl SnapshotHeader {
    pub fn new(
        tick: u64,
        tile_count: usize,
        logistics_count: usize,
        trade_link_count: usize,
        population_count: usize,
        power_count: usize,
        influencer_count: usize,
    ) -> Self {
        Self {
            tick,
            tile_count: tile_count as u32,
            logistics_count: logistics_count as u32,
            trade_link_count: trade_link_count as u32,
            population_count: population_count as u32,
            power_count: power_count as u32,
            influencer_count: influencer_count as u32,
            hash: 0,
            campaign_label: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u16)]
pub enum TerrainType {
    DeepOcean = 0,
    ContinentalShelf = 1,
    InlandSea = 2,
    CoralShelf = 3,
    HydrothermalVentField = 4,
    TidalFlat = 5,
    RiverDelta = 6,
    MangroveSwamp = 7,
    FreshwaterMarsh = 8,
    Floodplain = 9,
    #[default]
    AlluvialPlain = 10,
    PrairieSteppe = 11,
    MixedWoodland = 12,
    BorealTaiga = 13,
    PeatHeath = 14,
    HotDesertErg = 15,
    RockyReg = 16,
    SemiAridScrub = 17,
    SaltFlat = 18,
    OasisBasin = 19,
    Tundra = 20,
    PeriglacialSteppe = 21,
    Glacier = 22,
    SeasonalSnowfield = 23,
    RollingHills = 24,
    HighPlateau = 25,
    AlpineMountain = 26,
    KarstHighland = 27,
    CanyonBadlands = 28,
    ActiveVolcanoSlope = 29,
    BasalticLavaField = 30,
    AshPlain = 31,
    FumaroleBasin = 32,
    ImpactCraterField = 33,
    KarstCavernMouth = 34,
    SinkholeField = 35,
    AquiferCeiling = 36,
}

impl TerrainType {
    pub const VALUES: [TerrainType; 37] = [
        TerrainType::DeepOcean,
        TerrainType::ContinentalShelf,
        TerrainType::InlandSea,
        TerrainType::CoralShelf,
        TerrainType::HydrothermalVentField,
        TerrainType::TidalFlat,
        TerrainType::RiverDelta,
        TerrainType::MangroveSwamp,
        TerrainType::FreshwaterMarsh,
        TerrainType::Floodplain,
        TerrainType::AlluvialPlain,
        TerrainType::PrairieSteppe,
        TerrainType::MixedWoodland,
        TerrainType::BorealTaiga,
        TerrainType::PeatHeath,
        TerrainType::HotDesertErg,
        TerrainType::RockyReg,
        TerrainType::SemiAridScrub,
        TerrainType::SaltFlat,
        TerrainType::OasisBasin,
        TerrainType::Tundra,
        TerrainType::PeriglacialSteppe,
        TerrainType::Glacier,
        TerrainType::SeasonalSnowfield,
        TerrainType::RollingHills,
        TerrainType::HighPlateau,
        TerrainType::AlpineMountain,
        TerrainType::KarstHighland,
        TerrainType::CanyonBadlands,
        TerrainType::ActiveVolcanoSlope,
        TerrainType::BasalticLavaField,
        TerrainType::AshPlain,
        TerrainType::FumaroleBasin,
        TerrainType::ImpactCraterField,
        TerrainType::KarstCavernMouth,
        TerrainType::SinkholeField,
        TerrainType::AquiferCeiling,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Hash)]
#[serde(transparent)]
pub struct TerrainTags(pub u16);

impl TerrainTags {
    pub const fn new(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const WATER: Self = Self(1 << 0);
    pub const FRESHWATER: Self = Self(1 << 1);
    pub const COASTAL: Self = Self(1 << 2);
    pub const WETLAND: Self = Self(1 << 3);
    pub const FERTILE: Self = Self(1 << 4);
    pub const ARID: Self = Self(1 << 5);
    pub const POLAR: Self = Self(1 << 6);
    pub const HIGHLAND: Self = Self(1 << 7);
    pub const VOLCANIC: Self = Self(1 << 8);
    pub const HAZARDOUS: Self = Self(1 << 9);
    pub const SUBSURFACE: Self = Self(1 << 10);
    pub const HYDROTHERMAL: Self = Self(1 << 11);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl BitOr for TerrainTags {
    type Output = TerrainTags;

    fn bitor(self, rhs: Self) -> Self::Output {
        TerrainTags(self.bits() | rhs.bits())
    }
}

impl BitOrAssign for TerrainTags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.bits();
    }
}

impl BitAnd for TerrainTags {
    type Output = TerrainTags;

    fn bitand(self, rhs: Self) -> Self::Output {
        TerrainTags(self.bits() & rhs.bits())
    }
}

impl BitAndAssign for TerrainTags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.bits();
    }
}

impl From<u16> for TerrainTags {
    fn from(value: u16) -> Self {
        TerrainTags::new(value)
    }
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CrisisMetricKind {
    #[default]
    R0 = 0,
    GridStressPct = 1,
    UnauthorizedQueuePct = 2,
    SwarmsActive = 3,
    PhageDensity = 4,
}

impl CrisisMetricKind {
    pub const VALUES: [CrisisMetricKind; 5] = [
        CrisisMetricKind::R0,
        CrisisMetricKind::GridStressPct,
        CrisisMetricKind::UnauthorizedQueuePct,
        CrisisMetricKind::SwarmsActive,
        CrisisMetricKind::PhageDensity,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CrisisSeverityBand {
    #[default]
    Safe = 0,
    Warn = 1,
    Critical = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisTrendSample {
    pub tick: u64,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisGaugeState {
    pub kind: CrisisMetricKind,
    pub raw: f32,
    pub ema: f32,
    pub trend_5t: f32,
    pub warn_threshold: f32,
    pub critical_threshold: f32,
    pub last_updated_tick: u64,
    pub stale_ticks: u64,
    pub band: CrisisSeverityBand,
    pub history: Vec<CrisisTrendSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisTelemetryState {
    pub gauges: Vec<CrisisGaugeState>,
    pub modifiers_active: u32,
    pub foreshock_incidents: u32,
    pub containment_incidents: u32,
    pub warnings_active: u32,
    pub criticals_active: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisOverlayAnnotationState {
    pub label: String,
    pub severity: CrisisSeverityBand,
    pub path: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CrisisOverlayState {
    pub heatmap: ScalarRasterState,
    pub annotations: Vec<CrisisOverlayAnnotationState>,
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

impl From<TerrainTags> for u16 {
    fn from(value: TerrainTags) -> Self {
        value.bits()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum MountainKind {
    #[default]
    None = 0,
    Fold = 1,
    Fault = 2,
    Volcanic = 3,
    Dome = 4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainSample {
    pub terrain: TerrainType,
    pub tags: TerrainTags,
    #[serde(default)]
    pub mountain_kind: MountainKind,
    #[serde(default = "default_relief_scale")]
    pub relief_scale: f32,
}

impl Default for TerrainSample {
    fn default() -> Self {
        Self {
            terrain: TerrainType::AlluvialPlain,
            tags: TerrainTags::empty(),
            mountain_kind: MountainKind::None,
            relief_scale: 1.0,
        }
    }
}

impl PartialEq for TerrainSample {
    fn eq(&self, other: &Self) -> bool {
        self.terrain == other.terrain
            && self.tags == other.tags
            && self.mountain_kind == other.mountain_kind
            && self.relief_scale.to_bits() == other.relief_scale.to_bits()
    }
}

impl Eq for TerrainSample {}

impl std::hash::Hash for TerrainSample {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.terrain.hash(state);
        self.tags.hash(state);
        self.mountain_kind.hash(state);
        self.relief_scale.to_bits().hash(state);
    }
}

const fn default_relief_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct TerrainOverlayState {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<TerrainSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct HydrologyPointState {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct RiverSegmentState {
    pub id: u32,
    pub order: u8,
    pub width: u8,
    #[serde(default)]
    pub points: Vec<HydrologyPointState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct HydrologyOverlayState {
    #[serde(default)]
    pub rivers: Vec<RiverSegmentState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ElevationOverlayState {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub min_value: f32,
    #[serde(default)]
    pub max_value: f32,
    #[serde(default)]
    pub samples: Vec<u16>,
    #[serde(default)]
    pub normals: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct StartMarkerState {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct ScalarRasterState {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FloatRasterState {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TileState {
    pub entity: u64,
    pub x: u32,
    pub y: u32,
    pub element: u8,
    pub mass: i64,
    pub temperature: i64,
    pub terrain: TerrainType,
    pub terrain_tags: TerrainTags,
    pub culture_layer: u32,
    #[serde(default)]
    pub mountain_kind: MountainKind,
    #[serde(default = "default_relief_scale")]
    pub mountain_relief: f32,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PopulationCohortState {
    pub entity: u64,
    pub home: u64,
    pub size: u32,
    pub morale: i64,
    pub generation: u16,
    pub faction: u32,
    pub knowledge_fragments: Vec<KnownTechFragment>,
    #[serde(default)]
    pub migration: Option<PendingMigrationState>,
    #[serde(default)]
    pub harvest_task: Option<HarvestTaskState>,
    #[serde(default)]
    pub scout_task: Option<ScoutTaskState>,
    #[serde(default)]
    pub accessible_stockpile: Option<AccessibleStockpileState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PendingMigrationState {
    pub destination: u32,
    pub eta: u16,
    #[serde(default)]
    pub fragments: Vec<KnownTechFragment>,
}

fn default_harvest_task_kind() -> String {
    "harvest".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HarvestTaskState {
    #[serde(default = "default_harvest_task_kind")]
    pub kind: String,
    pub module: String,
    pub band_label: String,
    pub target_tile: u64,
    pub target_x: u32,
    pub target_y: u32,
    pub travel_remaining: u32,
    pub travel_total: u32,
    pub gather_remaining: u32,
    pub gather_total: u32,
    pub provisions_reward: i64,
    pub trade_goods_reward: i64,
    pub started_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ScoutTaskState {
    pub band_label: String,
    pub target_tile: u64,
    pub target_x: u32,
    pub target_y: u32,
    pub travel_remaining: u32,
    pub travel_total: u32,
    pub reveal_radius: u32,
    pub reveal_duration: u64,
    pub morale_gain: f32,
    pub started_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AccessibleStockpileEntryState {
    pub item: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AccessibleStockpileState {
    pub radius: u32,
    #[serde(default)]
    pub entries: Vec<AccessibleStockpileEntryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DiscoveryProgressEntry {
    pub faction: u32,
    pub discovery: u32,
    pub progress: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PowerNodeState {
    pub entity: u64,
    pub node_id: u32,
    pub generation: i64,
    pub demand: i64,
    pub efficiency: i64,
    pub storage_level: i64,
    pub storage_capacity: i64,
    pub stability: i64,
    pub surplus: i64,
    pub deficit: i64,
    pub incident_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PowerIncidentSeverity {
    #[default]
    Warning = 0,
    Critical = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerIncidentState {
    pub node_id: u32,
    pub severity: PowerIncidentSeverity,
    pub deficit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerTelemetryState {
    pub total_supply: i64,
    pub total_demand: i64,
    pub total_storage: i64,
    pub total_capacity: i64,
    pub grid_stress_avg: f32,
    pub surplus_margin: f32,
    pub instability_alerts: u32,
    pub incidents: Vec<PowerIncidentState>,
}

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CorruptionSubsystem {
    #[default]
    Logistics = 0,
    Trade = 1,
    Military = 2,
    Governance = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CorruptionEntry {
    pub subsystem: CorruptionSubsystem,
    pub intensity: i64,
    pub incident_id: u64,
    pub exposure_timer: u16,
    pub restitution_window: u16,
    pub last_update_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CorruptionLedger {
    pub entries: Vec<CorruptionEntry>,
    pub reputation_modifier: i64,
    pub audit_capacity: u16,
}

impl CorruptionLedger {
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn register_incident(&mut self, entry: CorruptionEntry) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.incident_id == entry.incident_id)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn incident_mut(&mut self, incident_id: u64) -> Option<&mut CorruptionEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.incident_id == incident_id)
    }

    pub fn remove_incident(&mut self, incident_id: u64) -> Option<CorruptionEntry> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.incident_id == incident_id)?;
        Some(self.entries.remove(index))
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub logistics: Vec<LogisticsLinkState>,
    pub trade_links: Vec<TradeLinkState>,
    pub populations: Vec<PopulationCohortState>,
    pub power: Vec<PowerNodeState>,
    pub power_metrics: PowerTelemetryState,
    pub great_discovery_definitions: Vec<GreatDiscoveryDefinitionState>,
    pub great_discoveries: Vec<GreatDiscoveryState>,
    pub great_discovery_progress: Vec<GreatDiscoveryProgressState>,
    pub great_discovery_telemetry: GreatDiscoveryTelemetryState,
    pub knowledge_ledger: Vec<KnowledgeLedgerEntryState>,
    pub knowledge_timeline: Vec<KnowledgeTimelineEventState>,
    pub knowledge_metrics: KnowledgeMetricsState,
    pub crisis_telemetry: CrisisTelemetryState,
    pub crisis_overlay: CrisisOverlayState,
    pub victory: VictorySnapshotState,
    #[serde(default)]
    pub capability_flags: u32,
    #[serde(default)]
    pub campaign_profiles: Vec<CampaignProfileState>,
    #[serde(default)]
    pub command_events: Vec<CommandEventState>,
    #[serde(default)]
    pub herds: Vec<HerdTelemetryState>,
    #[serde(default)]
    pub food_modules: Vec<FoodModuleState>,
    #[serde(default)]
    pub faction_inventory: Vec<FactionInventoryState>,
    pub moisture_raster: FloatRasterState,
    pub hydrology_overlay: HydrologyOverlayState,
    pub elevation_overlay: ElevationOverlayState,
    pub start_marker: Option<StartMarkerState>,
    pub terrain: TerrainOverlayState,
    pub logistics_raster: ScalarRasterState,
    pub sentiment_raster: ScalarRasterState,
    pub corruption_raster: ScalarRasterState,
    pub fog_raster: ScalarRasterState,
    pub culture_raster: ScalarRasterState,
    pub military_raster: ScalarRasterState,
    #[serde(default)]
    pub visibility_raster: ScalarRasterState,
    pub axis_bias: AxisBiasState,
    pub sentiment: SentimentTelemetryState,
    pub generations: Vec<GenerationState>,
    pub corruption: CorruptionLedger,
    pub influencers: Vec<InfluentialIndividualState>,
    pub culture_layers: Vec<CultureLayerState>,
    pub culture_tensions: Vec<CultureTensionState>,
    pub discovery_progress: Vec<DiscoveryProgressEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldDelta {
    pub header: SnapshotHeader,
    pub tiles: Vec<TileState>,
    pub removed_tiles: Vec<u64>,
    pub logistics: Vec<LogisticsLinkState>,
    pub removed_logistics: Vec<u64>,
    pub trade_links: Vec<TradeLinkState>,
    pub removed_trade_links: Vec<u64>,
    pub populations: Vec<PopulationCohortState>,
    pub removed_populations: Vec<u64>,
    pub power: Vec<PowerNodeState>,
    pub removed_power: Vec<u64>,
    pub power_metrics: Option<PowerTelemetryState>,
    pub great_discovery_definitions: Option<Vec<GreatDiscoveryDefinitionState>>,
    pub great_discoveries: Vec<GreatDiscoveryState>,
    pub great_discovery_progress: Vec<GreatDiscoveryProgressState>,
    pub great_discovery_telemetry: Option<GreatDiscoveryTelemetryState>,
    pub knowledge_ledger: Vec<KnowledgeLedgerEntryState>,
    pub removed_knowledge_ledger: Vec<u64>,
    pub knowledge_metrics: Option<KnowledgeMetricsState>,
    pub victory: Option<VictorySnapshotState>,
    pub capability_flags: Option<u32>,
    pub command_events: Option<Vec<CommandEventState>>,
    pub knowledge_timeline: Vec<KnowledgeTimelineEventState>,
    pub crisis_telemetry: Option<CrisisTelemetryState>,
    pub crisis_overlay: Option<CrisisOverlayState>,
    pub herds: Option<Vec<HerdTelemetryState>>,
    pub food_modules: Option<Vec<FoodModuleState>>,
    pub faction_inventory: Option<Vec<FactionInventoryState>>,
    pub moisture_raster: Option<FloatRasterState>,
    pub hydrology_overlay: Option<HydrologyOverlayState>,
    pub elevation_overlay: Option<ElevationOverlayState>,
    pub start_marker: Option<StartMarkerState>,
    pub axis_bias: Option<AxisBiasState>,
    pub sentiment: Option<SentimentTelemetryState>,
    pub logistics_raster: Option<ScalarRasterState>,
    pub sentiment_raster: Option<ScalarRasterState>,
    pub corruption_raster: Option<ScalarRasterState>,
    pub fog_raster: Option<ScalarRasterState>,
    pub culture_raster: Option<ScalarRasterState>,
    pub military_raster: Option<ScalarRasterState>,
    pub visibility_raster: Option<ScalarRasterState>,
    pub generations: Vec<GenerationState>,
    pub removed_generations: Vec<u16>,
    pub corruption: Option<CorruptionLedger>,
    pub influencers: Vec<InfluentialIndividualState>,
    pub removed_influencers: Vec<u32>,
    pub terrain: Option<TerrainOverlayState>,
    pub culture_layers: Vec<CultureLayerState>,
    pub removed_culture_layers: Vec<u32>,
    pub culture_tensions: Vec<CultureTensionState>,
    pub discovery_progress: Vec<DiscoveryProgressEntry>,
}

impl WorldSnapshot {
    pub fn finalize(mut self) -> Self {
        let hash = hash_snapshot(&self);
        let mut header = self.header;
        header.hash = hash;
        self.header = header;
        self
    }
}

pub fn hash_snapshot(snapshot: &WorldSnapshot) -> u64 {
    let mut clone = snapshot.clone();
    clone.header.hash = 0;
    let encoded = bincode::serialize(&clone).expect("snapshot serialization for hashing");
    let mut hasher = RandomState::with_seeds(0, 0, 0, 0).build_hasher();
    hasher.write(&encoded);
    hasher.finish()
}

pub fn encode_snapshot(snapshot: &WorldSnapshot) -> bincode::Result<Vec<u8>> {
    bincode::serialize(snapshot)
}

pub fn encode_delta(delta: &WorldDelta) -> bincode::Result<Vec<u8>> {
    bincode::serialize(delta)
}

pub fn encode_snapshot_flatbuffer(snapshot: &WorldSnapshot) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let offset = build_snapshot_flatbuffer(&mut builder, snapshot);
    builder.finish(offset, None);
    builder.finished_data().to_vec()
}

pub fn encode_delta_flatbuffer(delta: &WorldDelta) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();
    let offset = build_delta_flatbuffer(&mut builder, delta);
    builder.finish(offset, None);
    builder.finished_data().to_vec()
}

pub fn encode_snapshot_json(snapshot: &WorldSnapshot) -> serde_json::Result<String> {
    serde_json::to_string(snapshot)
}

pub fn decode_snapshot_json(data: &str) -> serde_json::Result<WorldSnapshot> {
    serde_json::from_str(data)
}

pub fn encode_delta_json(delta: &WorldDelta) -> serde_json::Result<String> {
    serde_json::to_string(delta)
}

pub fn decode_delta_json(data: &str) -> serde_json::Result<WorldDelta> {
    serde_json::from_str(data)
}

fn build_snapshot_flatbuffer<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::Envelope<'a>> {
    let campaign_label_fb = snapshot
        .header
        .campaign_label
        .as_ref()
        .and_then(|label| create_campaign_label(builder, label));
    let victory_state = create_victory_state(builder, &snapshot.victory);

    let header = fb::SnapshotHeader::create(
        builder,
        &fb::SnapshotHeaderArgs {
            tick: snapshot.header.tick,
            tileCount: snapshot.header.tile_count,
            logisticsCount: snapshot.header.logistics_count,
            tradeLinkCount: snapshot.header.trade_link_count,
            populationCount: snapshot.header.population_count,
            powerCount: snapshot.header.power_count,
            influencerCount: snapshot.header.influencer_count,
            hash: snapshot.header.hash,
            campaignLabel: campaign_label_fb,
            victory: Some(victory_state),
        },
    );

    let tiles_vec = create_tiles(builder, &snapshot.tiles);
    let logistics_vec = create_logistics(builder, &snapshot.logistics);
    let trade_links_vec = create_trade_links(builder, &snapshot.trade_links);
    let populations_vec = create_populations(builder, &snapshot.populations);
    let power_vec = create_power(builder, &snapshot.power);
    let power_metrics = create_power_metrics(builder, &snapshot.power_metrics);
    let great_discovery_definitions_vec =
        create_great_discovery_definitions(builder, &snapshot.great_discovery_definitions);
    let great_discoveries_vec = create_great_discoveries(builder, &snapshot.great_discoveries);
    let great_discovery_progress_vec =
        create_great_discovery_progress(builder, &snapshot.great_discovery_progress);
    let great_discovery_telemetry =
        create_great_discovery_telemetry(builder, &snapshot.great_discovery_telemetry);
    let knowledge_ledger_vec = create_knowledge_ledger(builder, &snapshot.knowledge_ledger);
    let knowledge_timeline_vec = create_knowledge_timeline(builder, &snapshot.knowledge_timeline);
    let knowledge_metrics = create_knowledge_metrics(builder, &snapshot.knowledge_metrics);
    let crisis_telemetry = create_crisis_telemetry(builder, &snapshot.crisis_telemetry);
    let crisis_overlay = create_crisis_overlay(builder, &snapshot.crisis_overlay);
    let campaign_profiles_vec = create_campaign_profiles(builder, &snapshot.campaign_profiles);
    let command_events_vec = create_command_events(builder, &snapshot.command_events);
    let herds_vec = create_herds(builder, &snapshot.herds);
    let food_modules_vec = create_food_modules(builder, &snapshot.food_modules);
    let faction_inventory_vec = create_faction_inventory(builder, &snapshot.faction_inventory);
    let hydrology_overlay = create_hydrology_overlay(builder, &snapshot.hydrology_overlay);
    let moisture_raster = create_float_raster(builder, &snapshot.moisture_raster);
    let elevation_overlay = create_elevation_overlay(builder, &snapshot.elevation_overlay);
    let start_marker = snapshot
        .start_marker
        .as_ref()
        .map(|marker| create_start_marker(builder, marker));
    let terrain_overlay = create_terrain_overlay(builder, &snapshot.terrain);
    let logistics_raster = create_scalar_raster(builder, &snapshot.logistics_raster);
    let sentiment_raster = create_scalar_raster(builder, &snapshot.sentiment_raster);
    let corruption_raster = create_scalar_raster(builder, &snapshot.corruption_raster);
    let fog_raster = create_scalar_raster(builder, &snapshot.fog_raster);
    let culture_raster = create_scalar_raster(builder, &snapshot.culture_raster);
    let military_raster = create_scalar_raster(builder, &snapshot.military_raster);
    let visibility_raster = create_scalar_raster(builder, &snapshot.visibility_raster);
    let axis_bias = fb::AxisBiasState::create(
        builder,
        &fb::AxisBiasStateArgs {
            knowledge: snapshot.axis_bias.knowledge,
            trust: snapshot.axis_bias.trust,
            equity: snapshot.axis_bias.equity,
            agency: snapshot.axis_bias.agency,
        },
    );
    let sentiment = create_sentiment(builder, &snapshot.sentiment);
    let generations_vec = create_generations(builder, &snapshot.generations);
    let corruption = create_corruption(builder, &snapshot.corruption);
    let influencers_vec = create_influencers(builder, &snapshot.influencers);
    let culture_layers_vec = create_culture_layers(builder, &snapshot.culture_layers);
    let culture_tensions_vec = create_culture_tensions(builder, &snapshot.culture_tensions);
    let discovery_progress_vec = create_discovery_progress(builder, &snapshot.discovery_progress);

    let snapshot_table = fb::WorldSnapshot::create(
        builder,
        &fb::WorldSnapshotArgs {
            header: Some(header),
            tiles: Some(tiles_vec),
            logistics: Some(logistics_vec),
            tradeLinks: Some(trade_links_vec),
            populations: Some(populations_vec),
            power: Some(power_vec),
            powerMetrics: Some(power_metrics),
            greatDiscoveryDefinitions: Some(great_discovery_definitions_vec),
            greatDiscoveries: Some(great_discoveries_vec),
            greatDiscoveryProgress: Some(great_discovery_progress_vec),
            greatDiscoveryTelemetry: Some(great_discovery_telemetry),
            knowledgeLedger: Some(knowledge_ledger_vec),
            knowledgeTimeline: Some(knowledge_timeline_vec),
            knowledgeMetrics: Some(knowledge_metrics),
            crisisTelemetry: Some(crisis_telemetry),
            crisisOverlay: Some(crisis_overlay),
            victory: Some(victory_state),
            capabilityFlags: snapshot.capability_flags,
            campaignProfiles: Some(campaign_profiles_vec),
            commandEvents: Some(command_events_vec),
            herds: Some(herds_vec),
            foodModules: Some(food_modules_vec),
            factionInventory: Some(faction_inventory_vec),
            moistureRaster: Some(moisture_raster),
            hydrologyOverlay: Some(hydrology_overlay),
            elevationOverlay: Some(elevation_overlay),
            startMarker: start_marker,
            terrainOverlay: Some(terrain_overlay),
            logisticsRaster: Some(logistics_raster),
            sentimentRaster: Some(sentiment_raster),
            corruptionRaster: Some(corruption_raster),
            fogRaster: Some(fog_raster),
            cultureRaster: Some(culture_raster),
            militaryRaster: Some(military_raster),
            axisBias: Some(axis_bias),
            sentiment: Some(sentiment),
            generations: Some(generations_vec),
            corruption: Some(corruption),
            influencers: Some(influencers_vec),
            cultureLayers: Some(culture_layers_vec),
            cultureTensions: Some(culture_tensions_vec),
            discoveryProgress: Some(discovery_progress_vec),
            visibilityRaster: Some(visibility_raster),
        },
    );

    fb::Envelope::create(
        builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::snapshot,
            payload: Some(snapshot_table.as_union_value()),
        },
    )
}

fn build_delta_flatbuffer<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::Envelope<'a>> {
    let campaign_label_fb = delta
        .header
        .campaign_label
        .as_ref()
        .and_then(|label| create_campaign_label(builder, label));
    let victory_state = delta
        .victory
        .as_ref()
        .map(|state| create_victory_state(builder, state));
    let herds = delta
        .herds
        .as_ref()
        .map(|entries| create_herds(builder, entries));
    let faction_inventory = delta
        .faction_inventory
        .as_ref()
        .map(|entries| create_faction_inventory(builder, entries));
    let command_events = delta
        .command_events
        .as_ref()
        .map(|entries| create_command_events(builder, entries));
    let food_modules = delta
        .food_modules
        .as_ref()
        .map(|entries| create_food_modules(builder, entries));

    let header = fb::SnapshotHeader::create(
        builder,
        &fb::SnapshotHeaderArgs {
            tick: delta.header.tick,
            tileCount: delta.header.tile_count,
            logisticsCount: delta.header.logistics_count,
            tradeLinkCount: delta.header.trade_link_count,
            populationCount: delta.header.population_count,
            powerCount: delta.header.power_count,
            influencerCount: delta.header.influencer_count,
            hash: delta.header.hash,
            campaignLabel: campaign_label_fb,
            victory: victory_state,
        },
    );

    let tiles_vec = create_tiles(builder, &delta.tiles);
    let removed_tiles_vec = builder.create_vector(&delta.removed_tiles);
    let logistics_vec = create_logistics(builder, &delta.logistics);
    let removed_logistics_vec = builder.create_vector(&delta.removed_logistics);
    let trade_links_vec = create_trade_links(builder, &delta.trade_links);
    let removed_trade_links_vec = builder.create_vector(&delta.removed_trade_links);
    let populations_vec = create_populations(builder, &delta.populations);
    let removed_populations_vec = builder.create_vector(&delta.removed_populations);
    let power_vec = create_power(builder, &delta.power);
    let removed_power_vec = builder.create_vector(&delta.removed_power);
    let power_metrics = delta
        .power_metrics
        .as_ref()
        .map(|metrics| create_power_metrics(builder, metrics));
    let great_discovery_definitions_vec = delta
        .great_discovery_definitions
        .as_ref()
        .map(|definitions| create_great_discovery_definitions(builder, definitions));
    let great_discoveries_vec = create_great_discoveries(builder, &delta.great_discoveries);
    let great_discovery_progress_vec =
        create_great_discovery_progress(builder, &delta.great_discovery_progress);
    let great_discovery_telemetry = delta
        .great_discovery_telemetry
        .as_ref()
        .map(|telemetry| create_great_discovery_telemetry(builder, telemetry));
    let knowledge_ledger_vec = create_knowledge_ledger(builder, &delta.knowledge_ledger);
    let removed_knowledge_vec = builder.create_vector(&delta.removed_knowledge_ledger);
    let knowledge_timeline_vec = create_knowledge_timeline(builder, &delta.knowledge_timeline);
    let knowledge_metrics = delta
        .knowledge_metrics
        .as_ref()
        .map(|metrics| create_knowledge_metrics(builder, metrics));
    let crisis_telemetry = delta
        .crisis_telemetry
        .as_ref()
        .map(|telemetry| create_crisis_telemetry(builder, telemetry));
    let crisis_overlay = delta
        .crisis_overlay
        .as_ref()
        .map(|overlay| create_crisis_overlay(builder, overlay));
    let moisture_raster = delta
        .moisture_raster
        .as_ref()
        .map(|raster| create_float_raster(builder, raster));
    let hydrology_overlay = delta
        .hydrology_overlay
        .as_ref()
        .map(|overlay| create_hydrology_overlay(builder, overlay));
    let elevation_overlay = delta
        .elevation_overlay
        .as_ref()
        .map(|overlay| create_elevation_overlay(builder, overlay));
    let start_marker = delta
        .start_marker
        .as_ref()
        .map(|marker| create_start_marker(builder, marker));
    let terrain_overlay = delta
        .terrain
        .as_ref()
        .map(|overlay| create_terrain_overlay(builder, overlay));
    let logistics_raster = delta
        .logistics_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let sentiment_raster = delta
        .sentiment_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let corruption_raster = delta
        .corruption_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let fog_raster = delta
        .fog_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let culture_raster = delta
        .culture_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let military_raster = delta
        .military_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let visibility_raster = delta
        .visibility_raster
        .as_ref()
        .map(|raster| create_scalar_raster(builder, raster));
    let axis_bias = delta.axis_bias.as_ref().map(|axis| {
        fb::AxisBiasState::create(
            builder,
            &fb::AxisBiasStateArgs {
                knowledge: axis.knowledge,
                trust: axis.trust,
                equity: axis.equity,
                agency: axis.agency,
            },
        )
    });
    let sentiment = delta
        .sentiment
        .as_ref()
        .map(|s| create_sentiment(builder, s));
    let generations_vec = create_generations(builder, &delta.generations);
    let removed_generations_vec = builder.create_vector(&delta.removed_generations);
    let corruption = delta
        .corruption
        .as_ref()
        .map(|c| create_corruption(builder, c));
    let influencers_vec = create_influencers(builder, &delta.influencers);
    let removed_influencers_vec = builder.create_vector(&delta.removed_influencers);
    let culture_layers_vec = create_culture_layers(builder, &delta.culture_layers);
    let removed_culture_layers_vec = builder.create_vector(&delta.removed_culture_layers);
    let culture_tensions_vec = create_culture_tensions(builder, &delta.culture_tensions);
    let discovery_progress_vec = create_discovery_progress(builder, &delta.discovery_progress);

    let delta_table = fb::WorldDelta::create(
        builder,
        &fb::WorldDeltaArgs {
            header: Some(header),
            tiles: Some(tiles_vec),
            removedTiles: Some(removed_tiles_vec),
            logistics: Some(logistics_vec),
            removedLogistics: Some(removed_logistics_vec),
            tradeLinks: Some(trade_links_vec),
            removedTradeLinks: Some(removed_trade_links_vec),
            populations: Some(populations_vec),
            removedPopulations: Some(removed_populations_vec),
            power: Some(power_vec),
            removedPower: Some(removed_power_vec),
            powerMetrics: power_metrics,
            greatDiscoveryDefinitions: great_discovery_definitions_vec,
            greatDiscoveries: Some(great_discoveries_vec),
            greatDiscoveryProgress: Some(great_discovery_progress_vec),
            greatDiscoveryTelemetry: great_discovery_telemetry,
            knowledgeLedger: Some(knowledge_ledger_vec),
            removedKnowledgeLedger: Some(removed_knowledge_vec),
            knowledgeTimeline: Some(knowledge_timeline_vec),
            knowledgeMetrics: knowledge_metrics,
            victory: victory_state,
            capabilityFlags: delta.capability_flags.unwrap_or(0),
            commandEvents: command_events,
            crisisTelemetry: crisis_telemetry,
            crisisOverlay: crisis_overlay,
            herds,
            foodModules: food_modules,
            factionInventory: faction_inventory,
            moistureRaster: moisture_raster,
            elevationOverlay: elevation_overlay,
            axisBias: axis_bias,
            sentiment,
            generations: Some(generations_vec),
            removedGenerations: Some(removed_generations_vec),
            corruption,
            influencers: Some(influencers_vec),
            removedInfluencers: Some(removed_influencers_vec),
            terrainOverlay: terrain_overlay,
            hydrologyOverlay: hydrology_overlay,
            startMarker: start_marker,
            logisticsRaster: logistics_raster,
            sentimentRaster: sentiment_raster,
            corruptionRaster: corruption_raster,
            fogRaster: fog_raster,
            cultureRaster: culture_raster,
            militaryRaster: military_raster,
            cultureLayers: Some(culture_layers_vec),
            removedCultureLayers: Some(removed_culture_layers_vec),
            cultureTensions: Some(culture_tensions_vec),
            discoveryProgress: Some(discovery_progress_vec),
            visibilityRaster: visibility_raster,
        },
    );

    fb::Envelope::create(
        builder,
        &fb::EnvelopeArgs {
            payload_type: fb::SnapshotPayload::delta,
            payload: Some(delta_table.as_union_value()),
        },
    )
}

fn create_hydrology_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &HydrologyOverlayState,
) -> WIPOffset<fb::HydrologyOverlay<'a>> {
    let rivers_vec: Vec<_> = overlay
        .rivers
        .iter()
        .map(|river| {
            let points: Vec<_> = river
                .points
                .iter()
                .map(|p| {
                    fb::HydrologyPoint::create(builder, &fb::HydrologyPointArgs { x: p.x, y: p.y })
                })
                .collect();
            let points_vec = builder.create_vector(&points);
            fb::RiverSegment::create(
                builder,
                &fb::RiverSegmentArgs {
                    id: river.id,
                    order: river.order,
                    width: river.width,
                    points: Some(points_vec),
                },
            )
        })
        .collect();
    let rivers_fb = builder.create_vector(&rivers_vec);
    fb::HydrologyOverlay::create(
        builder,
        &fb::HydrologyOverlayArgs {
            rivers: Some(rivers_fb),
        },
    )
}

fn create_elevation_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &ElevationOverlayState,
) -> WIPOffset<fb::ElevationOverlay<'a>> {
    let samples_vec = builder.create_vector(&overlay.samples);
    let normals_vec = if overlay.normals.is_empty() {
        None
    } else {
        Some(builder.create_vector(&overlay.normals))
    };
    fb::ElevationOverlay::create(
        builder,
        &fb::ElevationOverlayArgs {
            width: overlay.width,
            height: overlay.height,
            minValue: overlay.min_value,
            maxValue: overlay.max_value,
            samples: Some(samples_vec),
            normals: normals_vec,
        },
    )
}

fn create_start_marker<'a>(
    builder: &mut FbBuilder<'a>,
    marker: &StartMarkerState,
) -> WIPOffset<fb::StartMarker<'a>> {
    fb::StartMarker::create(
        builder,
        &fb::StartMarkerArgs {
            x: marker.x,
            y: marker.y,
        },
    )
}

fn create_campaign_label<'a>(
    builder: &mut FbBuilder<'a>,
    label: &CampaignLabel,
) -> Option<WIPOffset<fb::CampaignLabel<'a>>> {
    let has_any = label.title.is_some()
        || label.title_loc_key.is_some()
        || label.subtitle.is_some()
        || label.subtitle_loc_key.is_some()
        || label.profile_id.is_some();
    if !has_any {
        return None;
    }

    let profile_id = label
        .profile_id
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let title = label
        .title
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let title_loc_key = label
        .title_loc_key
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let subtitle = label
        .subtitle
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));
    let subtitle_loc_key = label
        .subtitle_loc_key
        .as_ref()
        .map(|value| builder.create_string(value.as_str()));

    Some(fb::CampaignLabel::create(
        builder,
        &fb::CampaignLabelArgs {
            profileId: profile_id,
            title,
            titleLocKey: title_loc_key,
            subtitle,
            subtitleLocKey: subtitle_loc_key,
        },
    ))
}

fn create_campaign_profiles<'a>(
    builder: &mut FbBuilder<'a>,
    profiles: &[CampaignProfileState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CampaignProfile<'a>>>> {
    let mut entries = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let id = profile
            .id
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let title = profile
            .title
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let title_loc_key = profile
            .title_loc_key
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let subtitle = profile
            .subtitle
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let subtitle_loc_key = profile
            .subtitle_loc_key
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let starting_units = if profile.starting_units.is_empty() {
            None
        } else {
            let mut offsets = Vec::with_capacity(profile.starting_units.len());
            for unit in &profile.starting_units {
                let kind = builder.create_string(unit.kind.as_str());
                let tags = if unit.tags.is_empty() {
                    None
                } else {
                    let tag_offsets: Vec<_> = unit
                        .tags
                        .iter()
                        .map(|tag| builder.create_string(tag.as_str()))
                        .collect();
                    Some(builder.create_vector(&tag_offsets))
                };
                let unit_entry = fb::CampaignStartingUnit::create(
                    builder,
                    &fb::CampaignStartingUnitArgs {
                        kind: Some(kind),
                        count: unit.count,
                        tags,
                    },
                );
                offsets.push(unit_entry);
            }
            Some(builder.create_vector(&offsets))
        };
        let inventory = if profile.inventory.is_empty() {
            None
        } else {
            let mut offsets = Vec::with_capacity(profile.inventory.len());
            for entry in &profile.inventory {
                let item = builder.create_string(entry.item.as_str());
                let inv_entry = fb::CampaignInventoryEntry::create(
                    builder,
                    &fb::CampaignInventoryEntryArgs {
                        item: Some(item),
                        quantity: entry.quantity,
                    },
                );
                offsets.push(inv_entry);
            }
            Some(builder.create_vector(&offsets))
        };
        let knowledge_tags = if profile.knowledge_tags.is_empty() {
            None
        } else {
            let offsets: Vec<_> = profile
                .knowledge_tags
                .iter()
                .map(|tag| builder.create_string(tag.as_str()))
                .collect();
            Some(builder.create_vector(&offsets))
        };
        let fog_mode = profile
            .fog_mode
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let primary_food_module = profile
            .primary_food_module
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let secondary_food_module = profile
            .secondary_food_module
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let survey_radius = profile.survey_radius.unwrap_or(0);
        let entry = fb::CampaignProfile::create(
            builder,
            &fb::CampaignProfileArgs {
                id,
                title,
                titleLocKey: title_loc_key,
                subtitle,
                subtitleLocKey: subtitle_loc_key,
                startingUnits: starting_units,
                inventory,
                knowledgeTags: knowledge_tags,
                surveyRadius: survey_radius,
                fogMode: fog_mode,
                primaryFoodModule: primary_food_module,
                secondaryFoodModule: secondary_food_module,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_faction_inventory<'a>(
    builder: &mut FbBuilder<'a>,
    factions: &[FactionInventoryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::FactionInventoryState<'a>>>> {
    let mut entries = Vec::with_capacity(factions.len());
    for state in factions {
        let mut inventory_offsets = Vec::with_capacity(state.inventory.len());
        for entry in &state.inventory {
            let item = builder.create_string(entry.item.as_str());
            let entry_offset = fb::FactionInventoryEntry::create(
                builder,
                &fb::FactionInventoryEntryArgs {
                    item: Some(item),
                    quantity: entry.quantity,
                },
            );
            inventory_offsets.push(entry_offset);
        }
        let inventory_vec = builder.create_vector(&inventory_offsets);
        let faction_entry = fb::FactionInventoryState::create(
            builder,
            &fb::FactionInventoryStateArgs {
                faction: state.faction,
                inventory: Some(inventory_vec),
            },
        );
        entries.push(faction_entry);
    }
    builder.create_vector(&entries)
}

fn create_herds<'a>(
    builder: &mut FbBuilder<'a>,
    herds: &[HerdTelemetryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::HerdTelemetryState<'a>>>> {
    let mut entries = Vec::with_capacity(herds.len());
    for herd in herds {
        let id = builder.create_string(herd.id.as_str());
        let label = builder.create_string(herd.label.as_str());
        let species = builder.create_string(herd.species.as_str());
        let entry = fb::HerdTelemetryState::create(
            builder,
            &fb::HerdTelemetryStateArgs {
                id: Some(id),
                label: Some(label),
                species: Some(species),
                x: herd.x,
                y: herd.y,
                biomass: herd.biomass,
                routeLength: herd.route_length,
                nextX: herd.next_x,
                nextY: herd.next_y,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_food_modules<'a>(
    builder: &mut FbBuilder<'a>,
    modules: &[FoodModuleState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::FoodModuleState<'a>>>> {
    let mut entries = Vec::with_capacity(modules.len());
    for module in modules {
        let module_label = builder.create_string(module.module.as_str());
        let kind_label = builder.create_string(module.kind.as_str());
        let entry = fb::FoodModuleState::create(
            builder,
            &fb::FoodModuleStateArgs {
                x: module.x,
                y: module.y,
                module: Some(module_label),
                seasonalWeight: module.seasonal_weight,
                kind: Some(kind_label),
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_command_events<'a>(
    builder: &mut FbBuilder<'a>,
    events: &[CommandEventState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CommandEventState<'a>>>> {
    let mut entries = Vec::with_capacity(events.len());
    for event in events {
        let kind = builder.create_string(event.kind.as_str());
        let label = builder.create_string(event.label.as_str());
        let detail = event
            .detail
            .as_ref()
            .map(|value| builder.create_string(value.as_str()));
        let entry = fb::CommandEventState::create(
            builder,
            &fb::CommandEventStateArgs {
                tick: event.tick,
                kind: Some(kind),
                faction: event.faction,
                label: Some(label),
                detail,
            },
        );
        entries.push(entry);
    }
    builder.create_vector(&entries)
}

fn create_victory_state<'a>(
    builder: &mut FbBuilder<'a>,
    state: &VictorySnapshotState,
) -> WIPOffset<fb::VictoryState<'a>> {
    let mut mode_entries = Vec::with_capacity(state.modes.len());
    for mode in &state.modes {
        let id = builder.create_string(mode.id.as_str());
        let kind = builder.create_string(mode.kind.as_str());
        let entry = fb::VictoryModeState::create(
            builder,
            &fb::VictoryModeStateArgs {
                id: Some(id),
                kind: Some(kind),
                progress: mode.progress,
                threshold: mode.threshold,
                achieved: mode.achieved,
            },
        );
        mode_entries.push(entry);
    }
    let modes_vec = builder.create_vector(&mode_entries);

    let winner = state.winner.as_ref().map(|winner| {
        let mode = builder.create_string(winner.mode.as_str());
        fb::VictoryResult::create(
            builder,
            &fb::VictoryResultArgs {
                mode: Some(mode),
                faction: winner.faction,
                tick: winner.tick,
            },
        )
    });

    fb::VictoryState::create(
        builder,
        &fb::VictoryStateArgs {
            modes: Some(modes_vec),
            winner,
        },
    )
}

fn create_tiles<'a>(
    builder: &mut FbBuilder<'a>,
    tiles: &[TileState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::TileState<'a>>>> {
    let offsets: Vec<_> = tiles
        .iter()
        .map(|tile| {
            fb::TileState::create(
                builder,
                &fb::TileStateArgs {
                    entity: tile.entity,
                    x: tile.x,
                    y: tile.y,
                    element: tile.element,
                    mass: tile.mass,
                    temperature: tile.temperature,
                    terrain: to_fb_terrain_type(tile.terrain),
                    terrainTags: tile.terrain_tags.bits(),
                    cultureLayer: tile.culture_layer,
                    mountainKind: to_fb_mountain_kind(tile.mountain_kind),
                    mountainRelief: tile.mountain_relief,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_logistics<'a>(
    builder: &mut FbBuilder<'a>,
    links: &[LogisticsLinkState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::LogisticsLinkState<'a>>>> {
    let offsets: Vec<_> = links
        .iter()
        .map(|link| {
            fb::LogisticsLinkState::create(
                builder,
                &fb::LogisticsLinkStateArgs {
                    entity: link.entity,
                    from: link.from,
                    to: link.to,
                    capacity: link.capacity,
                    flow: link.flow,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_trade_links<'a>(
    builder: &mut FbBuilder<'a>,
    links: &[TradeLinkState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::TradeLinkState<'a>>>> {
    let offsets: Vec<_> = links
        .iter()
        .map(|link| {
            let knowledge = fb::TradeLinkKnowledge::create(
                builder,
                &fb::TradeLinkKnowledgeArgs {
                    openness: link.knowledge.openness,
                    leakTimer: link.knowledge.leak_timer,
                    lastDiscovery: link.knowledge.last_discovery,
                    decay: link.knowledge.decay,
                },
            );
            let pending_fragments = if link.pending_fragments.is_empty() {
                None
            } else {
                Some(create_known_fragments(builder, &link.pending_fragments))
            };
            fb::TradeLinkState::create(
                builder,
                &fb::TradeLinkStateArgs {
                    entity: link.entity,
                    fromFaction: link.from_faction,
                    toFaction: link.to_faction,
                    throughput: link.throughput,
                    tariff: link.tariff,
                    knowledge: Some(knowledge),
                    fromTile: link.from_tile,
                    toTile: link.to_tile,
                    pendingFragments: pending_fragments,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_populations<'a>(
    builder: &mut FbBuilder<'a>,
    cohorts: &[PopulationCohortState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PopulationCohortState<'a>>>> {
    let offsets: Vec<_> = cohorts
        .iter()
        .map(|cohort| {
            let knowledge = if cohort.knowledge_fragments.is_empty() {
                None
            } else {
                Some(create_known_fragments(builder, &cohort.knowledge_fragments))
            };
            let migration = cohort.migration.as_ref().map(|pending| {
                let fragments = if pending.fragments.is_empty() {
                    None
                } else {
                    Some(create_known_fragments(builder, &pending.fragments))
                };
                fb::PendingMigration::create(
                    builder,
                    &fb::PendingMigrationArgs {
                        destination: pending.destination,
                        eta: pending.eta,
                        fragments,
                    },
                )
            });
            let harvest = cohort.harvest_task.as_ref().map(|task| {
                let module = builder.create_string(&task.module);
                let band_label = builder.create_string(&task.band_label);
                let kind = builder.create_string(&task.kind);
                fb::HarvestTask::create(
                    builder,
                    &fb::HarvestTaskArgs {
                        kind: Some(kind),
                        module: Some(module),
                        bandLabel: Some(band_label),
                        targetTile: task.target_tile,
                        targetX: task.target_x,
                        targetY: task.target_y,
                        travelRemaining: task.travel_remaining,
                        travelTotal: task.travel_total,
                        gatherRemaining: task.gather_remaining,
                        gatherTotal: task.gather_total,
                        provisionsReward: task.provisions_reward,
                        tradeGoodsReward: task.trade_goods_reward,
                        startedTick: task.started_tick,
                    },
                )
            });
            let scout = cohort.scout_task.as_ref().map(|task| {
                let band_label = builder.create_string(&task.band_label);
                fb::ScoutTask::create(
                    builder,
                    &fb::ScoutTaskArgs {
                        bandLabel: Some(band_label),
                        targetTile: task.target_tile,
                        targetX: task.target_x,
                        targetY: task.target_y,
                        travelRemaining: task.travel_remaining,
                        travelTotal: task.travel_total,
                        revealRadius: task.reveal_radius,
                        revealDuration: task.reveal_duration,
                        moraleGain: task.morale_gain,
                        startedTick: task.started_tick,
                    },
                )
            });
            let accessible_stockpile_fb = cohort.accessible_stockpile.as_ref().map(|stockpile| {
                let entries = if stockpile.entries.is_empty() {
                    None
                } else {
                    Some(create_accessible_stockpile_entries(
                        builder,
                        &stockpile.entries,
                    ))
                };
                fb::AccessibleStockpile::create(
                    builder,
                    &fb::AccessibleStockpileArgs {
                        radius: stockpile.radius,
                        entries,
                    },
                )
            });
            fb::PopulationCohortState::create(
                builder,
                &fb::PopulationCohortStateArgs {
                    entity: cohort.entity,
                    home: cohort.home,
                    size: cohort.size,
                    morale: cohort.morale,
                    generation: cohort.generation,
                    faction: cohort.faction,
                    knowledgeFragments: knowledge,
                    migration,
                    harvestTask: harvest,
                    scoutTask: scout,
                    accessibleStockpile: accessible_stockpile_fb,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_known_fragments<'a>(
    builder: &mut FbBuilder<'a>,
    fragments: &[KnownTechFragment],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnownTechFragment<'a>>>> {
    let offsets: Vec<_> = fragments
        .iter()
        .map(|fragment| {
            fb::KnownTechFragment::create(
                builder,
                &fb::KnownTechFragmentArgs {
                    discoveryId: fragment.discovery_id,
                    progress: fragment.progress,
                    fidelity: fragment.fidelity,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_accessible_stockpile_entries<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[AccessibleStockpileEntryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::AccessibleStockpileEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let item = builder.create_string(&entry.item);
            fb::AccessibleStockpileEntry::create(
                builder,
                &fb::AccessibleStockpileEntryArgs {
                    item: Some(item),
                    quantity: entry.quantity,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_discovery_progress<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[DiscoveryProgressEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::DiscoveryProgressEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::DiscoveryProgressEntry::create(
                builder,
                &fb::DiscoveryProgressEntryArgs {
                    faction: entry.faction,
                    discovery: entry.discovery,
                    progress: entry.progress,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power<'a>(
    builder: &mut FbBuilder<'a>,
    power_nodes: &[PowerNodeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PowerNodeState<'a>>>> {
    let offsets: Vec<_> = power_nodes
        .iter()
        .map(|node| {
            fb::PowerNodeState::create(
                builder,
                &fb::PowerNodeStateArgs {
                    entity: node.entity,
                    nodeId: node.node_id,
                    generation: node.generation,
                    demand: node.demand,
                    efficiency: node.efficiency,
                    storageLevel: node.storage_level,
                    storageCapacity: node.storage_capacity,
                    stability: node.stability,
                    surplus: node.surplus,
                    deficit: node.deficit,
                    incidentCount: node.incident_count,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power_incidents<'a>(
    builder: &mut FbBuilder<'a>,
    incidents: &[PowerIncidentState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::PowerIncidentState<'a>>>> {
    let offsets: Vec<_> = incidents
        .iter()
        .map(|incident| {
            fb::PowerIncidentState::create(
                builder,
                &fb::PowerIncidentStateArgs {
                    nodeId: incident.node_id,
                    severity: match incident.severity {
                        PowerIncidentSeverity::Warning => fb::PowerIncidentSeverity::Warning,
                        PowerIncidentSeverity::Critical => fb::PowerIncidentSeverity::Critical,
                    },
                    deficit: incident.deficit,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_power_metrics<'a>(
    builder: &mut FbBuilder<'a>,
    metrics: &PowerTelemetryState,
) -> WIPOffset<fb::PowerTelemetryState<'a>> {
    let incidents = create_power_incidents(builder, &metrics.incidents);
    fb::PowerTelemetryState::create(
        builder,
        &fb::PowerTelemetryStateArgs {
            totalSupply: metrics.total_supply,
            totalDemand: metrics.total_demand,
            totalStorage: metrics.total_storage,
            totalCapacity: metrics.total_capacity,
            gridStressAvg: metrics.grid_stress_avg,
            surplusMargin: metrics.surplus_margin,
            instabilityAlerts: metrics.instability_alerts,
            incidents: Some(incidents),
        },
    )
}

fn to_fb_crisis_metric_kind(kind: CrisisMetricKind) -> fb::CrisisMetricKind {
    match kind {
        CrisisMetricKind::R0 => fb::CrisisMetricKind::R0,
        CrisisMetricKind::GridStressPct => fb::CrisisMetricKind::GridStressPct,
        CrisisMetricKind::UnauthorizedQueuePct => fb::CrisisMetricKind::UnauthorizedQueuePct,
        CrisisMetricKind::SwarmsActive => fb::CrisisMetricKind::SwarmsActive,
        CrisisMetricKind::PhageDensity => fb::CrisisMetricKind::PhageDensity,
    }
}

fn to_fb_crisis_severity_band(band: CrisisSeverityBand) -> fb::CrisisSeverityBand {
    match band {
        CrisisSeverityBand::Safe => fb::CrisisSeverityBand::Safe,
        CrisisSeverityBand::Warn => fb::CrisisSeverityBand::Warn,
        CrisisSeverityBand::Critical => fb::CrisisSeverityBand::Critical,
    }
}

fn create_crisis_trend_samples<'a>(
    builder: &mut FbBuilder<'a>,
    samples: &[CrisisTrendSample],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisTrendSample<'a>>>> {
    let offsets: Vec<_> = samples
        .iter()
        .map(|sample| {
            fb::CrisisTrendSample::create(
                builder,
                &fb::CrisisTrendSampleArgs {
                    tick: sample.tick,
                    value: sample.value,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_gauges<'a>(
    builder: &mut FbBuilder<'a>,
    gauges: &[CrisisGaugeState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisGaugeState<'a>>>> {
    let offsets: Vec<_> = gauges
        .iter()
        .map(|gauge| {
            let history = create_crisis_trend_samples(builder, &gauge.history);
            fb::CrisisGaugeState::create(
                builder,
                &fb::CrisisGaugeStateArgs {
                    kind: to_fb_crisis_metric_kind(gauge.kind),
                    raw: gauge.raw,
                    ema: gauge.ema,
                    trend5t: gauge.trend_5t,
                    warnThreshold: gauge.warn_threshold,
                    criticalThreshold: gauge.critical_threshold,
                    lastUpdatedTick: gauge.last_updated_tick,
                    staleTicks: gauge.stale_ticks,
                    band: to_fb_crisis_severity_band(gauge.band),
                    history: Some(history),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_telemetry<'a>(
    builder: &mut FbBuilder<'a>,
    telemetry: &CrisisTelemetryState,
) -> WIPOffset<fb::CrisisTelemetryState<'a>> {
    let gauges = create_crisis_gauges(builder, &telemetry.gauges);
    fb::CrisisTelemetryState::create(
        builder,
        &fb::CrisisTelemetryStateArgs {
            gauges: Some(gauges),
            modifiersActive: telemetry.modifiers_active,
            foreshockIncidents: telemetry.foreshock_incidents,
            containmentIncidents: telemetry.containment_incidents,
            warningsActive: telemetry.warnings_active,
            criticalsActive: telemetry.criticals_active,
        },
    )
}

fn create_crisis_overlay_annotations<'a>(
    builder: &mut FbBuilder<'a>,
    annotations: &[CrisisOverlayAnnotationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CrisisOverlayAnnotationState<'a>>>> {
    let offsets: Vec<_> = annotations
        .iter()
        .map(|annotation| {
            let path = builder.create_vector(&annotation.path);
            let label = builder.create_string(&annotation.label);
            fb::CrisisOverlayAnnotationState::create(
                builder,
                &fb::CrisisOverlayAnnotationStateArgs {
                    label: Some(label),
                    severity: to_fb_crisis_severity_band(annotation.severity),
                    path: Some(path),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_crisis_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &CrisisOverlayState,
) -> WIPOffset<fb::CrisisOverlayState<'a>> {
    let heatmap = create_scalar_raster(builder, &overlay.heatmap);
    let annotations = create_crisis_overlay_annotations(builder, &overlay.annotations);
    fb::CrisisOverlayState::create(
        builder,
        &fb::CrisisOverlayStateArgs {
            heatmap: Some(heatmap),
            annotations: Some(annotations),
        },
    )
}

fn to_fb_knowledge_field(field: KnowledgeField) -> fb::KnowledgeField {
    match field {
        KnowledgeField::Physics => fb::KnowledgeField::Physics,
        KnowledgeField::Chemistry => fb::KnowledgeField::Chemistry,
        KnowledgeField::Biology => fb::KnowledgeField::Biology,
        KnowledgeField::Data => fb::KnowledgeField::Data,
        KnowledgeField::Communication => fb::KnowledgeField::Communication,
        KnowledgeField::Exotic => fb::KnowledgeField::Exotic,
    }
}

fn create_great_discoveries<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::GreatDiscoveryState::create(
                builder,
                &fb::GreatDiscoveryStateArgs {
                    id: entry.id,
                    faction: entry.faction,
                    field: to_fb_knowledge_field(entry.field),
                    tick: entry.tick,
                    publiclyDeployed: entry.publicly_deployed,
                    effectFlags: entry.effect_flags,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_definition_requirements<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryRequirementState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryRequirementDefinition<'a>>>>
{
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let name = entry
                .name
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let summary = entry
                .summary
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            fb::GreatDiscoveryRequirementDefinition::create(
                builder,
                &fb::GreatDiscoveryRequirementDefinitionArgs {
                    discoveryId: entry.discovery,
                    weight: entry.weight,
                    minimumProgress: entry.minimum_progress,
                    name,
                    summary,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_definitions<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryDefinitionState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryDefinition<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let name = builder.create_string(entry.name.as_str());
            let tier = entry
                .tier
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let summary = entry
                .summary
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let tags = if entry.tags.is_empty() {
                None
            } else {
                let mut tag_offsets = Vec::with_capacity(entry.tags.len());
                for tag in &entry.tags {
                    tag_offsets.push(builder.create_string(tag.as_str()));
                }
                Some(builder.create_vector(&tag_offsets))
            };
            let effects_summary = if entry.effects_summary.is_empty() {
                None
            } else {
                let mut effect_offsets = Vec::with_capacity(entry.effects_summary.len());
                for line in &entry.effects_summary {
                    effect_offsets.push(builder.create_string(line.as_str()));
                }
                Some(builder.create_vector(&effect_offsets))
            };
            let observation_notes = entry
                .observation_notes
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let leak_profile = entry
                .leak_profile
                .as_ref()
                .map(|value| builder.create_string(value.as_str()));
            let requirements =
                create_great_discovery_definition_requirements(builder, &entry.requirements);

            fb::GreatDiscoveryDefinition::create(
                builder,
                &fb::GreatDiscoveryDefinitionArgs {
                    id: entry.id,
                    name: Some(name),
                    field: to_fb_knowledge_field(entry.field),
                    observationThreshold: entry.observation_threshold,
                    cooldownTicks: entry.cooldown_ticks,
                    freshnessWindow: entry.freshness_window.unwrap_or_default(),
                    hasFreshnessWindow: entry.freshness_window.is_some(),
                    effectFlags: entry.effect_flags,
                    covertUntilPublic: entry.covert_until_public,
                    tier,
                    summary,
                    tags,
                    effectsSummary: effects_summary,
                    observationNotes: observation_notes,
                    leakProfile: leak_profile,
                    requirements: Some(requirements),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_progress<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[GreatDiscoveryProgressState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GreatDiscoveryProgressState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::GreatDiscoveryProgressState::create(
                builder,
                &fb::GreatDiscoveryProgressStateArgs {
                    faction: entry.faction,
                    discovery: entry.discovery,
                    progress: entry.progress,
                    observationDeficit: entry.observation_deficit,
                    etaTicks: entry.eta_ticks,
                    covert: entry.covert,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_great_discovery_telemetry<'a>(
    builder: &mut FbBuilder<'a>,
    telemetry: &GreatDiscoveryTelemetryState,
) -> WIPOffset<fb::GreatDiscoveryTelemetryState<'a>> {
    fb::GreatDiscoveryTelemetryState::create(
        builder,
        &fb::GreatDiscoveryTelemetryStateArgs {
            totalResolved: telemetry.total_resolved,
            pendingCandidates: telemetry.pending_candidates,
            activeConstellations: telemetry.active_constellations,
        },
    )
}

fn create_knowledge_ledger<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeLedgerEntryState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeLedgerState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let countermeasures = create_knowledge_countermeasures(builder, &entry.countermeasures);
            let infiltrations = create_knowledge_infiltrations(builder, &entry.infiltrations);
            let modifiers = create_knowledge_modifiers(builder, &entry.modifiers);
            fb::KnowledgeLedgerState::create(
                builder,
                &fb::KnowledgeLedgerStateArgs {
                    discoveryId: entry.discovery_id,
                    ownerFaction: entry.owner_faction,
                    tier: entry.tier,
                    progressPercent: entry.progress_percent,
                    halfLifeTicks: entry.half_life_ticks,
                    timeToCascade: entry.time_to_cascade,
                    securityPosture: to_fb_knowledge_security_posture(entry.security_posture),
                    countermeasures: Some(countermeasures),
                    infiltrations: Some(infiltrations),
                    modifiers: Some(modifiers),
                    flags: entry.flags.bits(),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_countermeasures<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeCountermeasureState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeCountermeasureState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::KnowledgeCountermeasureState::create(
                builder,
                &fb::KnowledgeCountermeasureStateArgs {
                    kind: to_fb_knowledge_countermeasure(entry.kind),
                    potency: entry.potency,
                    upkeep: entry.upkeep,
                    remainingTicks: entry.remaining_ticks,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_infiltrations<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeInfiltrationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeInfiltrationState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::KnowledgeInfiltrationState::create(
                builder,
                &fb::KnowledgeInfiltrationStateArgs {
                    faction: entry.faction,
                    blueprintFidelity: entry.blueprint_fidelity,
                    suspicion: entry.suspicion,
                    cells: entry.cells,
                    lastActivityTick: entry.last_activity_tick,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_modifiers<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[KnowledgeModifierBreakdownState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeModifierBreakdownState<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            let note = entry
                .note_handle
                .as_ref()
                .map(|note| builder.create_string(note.as_str()));
            fb::KnowledgeModifierBreakdownState::create(
                builder,
                &fb::KnowledgeModifierBreakdownStateArgs {
                    source: to_fb_knowledge_modifier_source(entry.source),
                    deltaHalfLife: entry.delta_half_life,
                    deltaProgress: entry.delta_progress,
                    noteHandle: note,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_timeline<'a>(
    builder: &mut FbBuilder<'a>,
    events: &[KnowledgeTimelineEventState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::KnowledgeTimelineEventState<'a>>>> {
    let offsets: Vec<_> = events
        .iter()
        .map(|event| {
            let note = event
                .note_handle
                .as_ref()
                .map(|note| builder.create_string(note.as_str()));
            fb::KnowledgeTimelineEventState::create(
                builder,
                &fb::KnowledgeTimelineEventStateArgs {
                    tick: event.tick,
                    kind: to_fb_knowledge_timeline_kind(event.kind),
                    sourceFaction: event.source_faction,
                    deltaPercent: event.delta_percent,
                    noteHandle: note,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_knowledge_metrics<'a>(
    builder: &mut FbBuilder<'a>,
    metrics: &KnowledgeMetricsState,
) -> WIPOffset<fb::KnowledgeMetricsState<'a>> {
    fb::KnowledgeMetricsState::create(
        builder,
        &fb::KnowledgeMetricsStateArgs {
            leakWarnings: metrics.leak_warnings,
            leakCriticals: metrics.leak_criticals,
            countermeasuresActive: metrics.countermeasures_active,
            commonKnowledgeTotal: metrics.common_knowledge_total,
        },
    )
}

fn create_terrain_overlay<'a>(
    builder: &mut FbBuilder<'a>,
    overlay: &TerrainOverlayState,
) -> WIPOffset<fb::TerrainOverlay<'a>> {
    let sample_offsets: Vec<_> = overlay
        .samples
        .iter()
        .map(|sample| {
            fb::TerrainSample::create(
                builder,
                &fb::TerrainSampleArgs {
                    terrain: to_fb_terrain_type(sample.terrain),
                    tags: sample.tags.bits(),
                    mountainKind: to_fb_mountain_kind(sample.mountain_kind),
                    reliefScale: sample.relief_scale,
                },
            )
        })
        .collect();
    let samples = builder.create_vector(&sample_offsets);
    fb::TerrainOverlay::create(
        builder,
        &fb::TerrainOverlayArgs {
            width: overlay.width,
            height: overlay.height,
            samples: Some(samples),
        },
    )
}

fn create_scalar_raster<'a>(
    builder: &mut FbBuilder<'a>,
    raster: &ScalarRasterState,
) -> WIPOffset<fb::ScalarRaster<'a>> {
    let samples = builder.create_vector(&raster.samples);
    fb::ScalarRaster::create(
        builder,
        &fb::ScalarRasterArgs {
            width: raster.width,
            height: raster.height,
            samples: Some(samples),
        },
    )
}

fn create_float_raster<'a>(
    builder: &mut FbBuilder<'a>,
    raster: &FloatRasterState,
) -> WIPOffset<fb::FloatRaster<'a>> {
    let samples = builder.create_vector(&raster.samples);
    fb::FloatRaster::create(
        builder,
        &fb::FloatRasterArgs {
            width: raster.width,
            height: raster.height,
            samples: Some(samples),
        },
    )
}

fn create_sentiment<'a>(
    builder: &mut FbBuilder<'a>,
    sentiment: &SentimentTelemetryState,
) -> WIPOffset<fb::SentimentTelemetryState<'a>> {
    let knowledge = create_sentiment_axis(builder, &sentiment.knowledge);
    let trust = create_sentiment_axis(builder, &sentiment.trust);
    let equity = create_sentiment_axis(builder, &sentiment.equity);
    let agency = create_sentiment_axis(builder, &sentiment.agency);
    fb::SentimentTelemetryState::create(
        builder,
        &fb::SentimentTelemetryStateArgs {
            knowledge: Some(knowledge),
            trust: Some(trust),
            equity: Some(equity),
            agency: Some(agency),
        },
    )
}

fn create_sentiment_axis<'a>(
    builder: &mut FbBuilder<'a>,
    axis: &SentimentAxisTelemetry,
) -> WIPOffset<fb::SentimentAxisTelemetry<'a>> {
    let drivers: Vec<_> = axis
        .drivers
        .iter()
        .map(|driver| {
            let label = builder.create_string(driver.label.as_str());
            fb::SentimentDriverState::create(
                builder,
                &fb::SentimentDriverStateArgs {
                    category: to_fb_driver_category(driver.category),
                    label: Some(label),
                    value: driver.value,
                    weight: driver.weight,
                },
            )
        })
        .collect();
    let drivers_vec = builder.create_vector(&drivers);
    fb::SentimentAxisTelemetry::create(
        builder,
        &fb::SentimentAxisTelemetryArgs {
            policy: axis.policy,
            incidents: axis.incidents,
            influencers: axis.influencers,
            total: axis.total,
            drivers: Some(drivers_vec),
        },
    )
}

fn create_generations<'a>(
    builder: &mut FbBuilder<'a>,
    generations: &[GenerationState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::GenerationState<'a>>>> {
    let offsets: Vec<_> = generations
        .iter()
        .map(|generation| {
            let name = builder.create_string(generation.name.as_str());
            fb::GenerationState::create(
                builder,
                &fb::GenerationStateArgs {
                    id: generation.id,
                    name: Some(name),
                    biasKnowledge: generation.bias_knowledge,
                    biasTrust: generation.bias_trust,
                    biasEquity: generation.bias_equity,
                    biasAgency: generation.bias_agency,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_corruption<'a>(
    builder: &mut FbBuilder<'a>,
    ledger: &CorruptionLedger,
) -> WIPOffset<fb::CorruptionLedger<'a>> {
    let entries: Vec<_> = ledger
        .entries
        .iter()
        .map(|entry| {
            fb::CorruptionEntry::create(
                builder,
                &fb::CorruptionEntryArgs {
                    subsystem: to_fb_corruption_subsystem(entry.subsystem),
                    intensity: entry.intensity,
                    incidentId: entry.incident_id,
                    exposureTimer: entry.exposure_timer,
                    restitutionWindow: entry.restitution_window,
                    lastUpdateTick: entry.last_update_tick,
                },
            )
        })
        .collect();
    let entries_vec = builder.create_vector(&entries);
    fb::CorruptionLedger::create(
        builder,
        &fb::CorruptionLedgerArgs {
            entries: Some(entries_vec),
            reputationModifier: ledger.reputation_modifier,
            auditCapacity: ledger.audit_capacity,
        },
    )
}

fn create_influencers<'a>(
    builder: &mut FbBuilder<'a>,
    influencers: &[InfluentialIndividualState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::InfluentialIndividualState<'a>>>> {
    let offsets: Vec<_> = influencers
        .iter()
        .map(|inf| {
            let name = builder.create_string(inf.name.as_str());
            let audience_vec = builder.create_vector(&inf.audience_generations);
            let resonance_vec =
                create_influencer_culture_resonance(builder, &inf.culture_resonance);
            fb::InfluentialIndividualState::create(
                builder,
                &fb::InfluentialIndividualStateArgs {
                    id: inf.id,
                    name: Some(name),
                    influence: inf.influence,
                    growthRate: inf.growth_rate,
                    baselineGrowth: inf.baseline_growth,
                    notoriety: inf.notoriety,
                    sentimentKnowledge: inf.sentiment_knowledge,
                    sentimentTrust: inf.sentiment_trust,
                    sentimentEquity: inf.sentiment_equity,
                    sentimentAgency: inf.sentiment_agency,
                    sentimentWeightKnowledge: inf.sentiment_weight_knowledge,
                    sentimentWeightTrust: inf.sentiment_weight_trust,
                    sentimentWeightEquity: inf.sentiment_weight_equity,
                    sentimentWeightAgency: inf.sentiment_weight_agency,
                    logisticsBonus: inf.logistics_bonus,
                    moraleBonus: inf.morale_bonus,
                    powerBonus: inf.power_bonus,
                    logisticsWeight: inf.logistics_weight,
                    moraleWeight: inf.morale_weight,
                    powerWeight: inf.power_weight,
                    supportCharge: inf.support_charge,
                    suppressPressure: inf.suppress_pressure,
                    domains: inf.domains,
                    scope: to_fb_influence_scope(inf.scope),
                    generationScope: inf.generation_scope,
                    supported: inf.supported,
                    suppressed: inf.suppressed,
                    lifecycle: to_fb_influence_lifecycle(inf.lifecycle),
                    coherence: inf.coherence,
                    ticksInStatus: inf.ticks_in_status,
                    audienceGenerations: Some(audience_vec),
                    supportPopular: inf.support_popular,
                    supportPeer: inf.support_peer,
                    supportInstitutional: inf.support_institutional,
                    supportHumanitarian: inf.support_humanitarian,
                    weightPopular: inf.weight_popular,
                    weightPeer: inf.weight_peer,
                    weightInstitutional: inf.weight_institutional,
                    weightHumanitarian: inf.weight_humanitarian,
                    cultureResonance: Some(resonance_vec),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_influencer_culture_resonance<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[InfluencerCultureResonanceEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::InfluencerCultureResonanceEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::InfluencerCultureResonanceEntry::create(
                builder,
                &fb::InfluencerCultureResonanceEntryArgs {
                    axis: to_fb_culture_trait_axis(entry.axis),
                    weight: entry.weight,
                    output: entry.output,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_traits<'a>(
    builder: &mut FbBuilder<'a>,
    entries: &[CultureTraitEntry],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureTraitEntry<'a>>>> {
    let offsets: Vec<_> = entries
        .iter()
        .map(|entry| {
            fb::CultureTraitEntry::create(
                builder,
                &fb::CultureTraitEntryArgs {
                    axis: to_fb_culture_trait_axis(entry.axis),
                    baseline: entry.baseline,
                    modifier: entry.modifier,
                    value: entry.value,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_layers<'a>(
    builder: &mut FbBuilder<'a>,
    layers: &[CultureLayerState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureLayerState<'a>>>> {
    let offsets: Vec<_> = layers
        .iter()
        .map(|layer| {
            let traits_vec = create_culture_traits(builder, &layer.traits);
            fb::CultureLayerState::create(
                builder,
                &fb::CultureLayerStateArgs {
                    id: layer.id,
                    owner: layer.owner,
                    parent: layer.parent,
                    scope: to_fb_culture_layer_scope(layer.scope),
                    traits: Some(traits_vec),
                    divergence: layer.divergence,
                    softThreshold: layer.soft_threshold,
                    hardThreshold: layer.hard_threshold,
                    ticksAboveSoft: layer.ticks_above_soft,
                    ticksAboveHard: layer.ticks_above_hard,
                    lastUpdatedTick: layer.last_updated_tick,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_culture_tensions<'a>(
    builder: &mut FbBuilder<'a>,
    tensions: &[CultureTensionState],
) -> WIPOffset<flatbuffers::Vector<'a, ForwardsUOffset<fb::CultureTensionState<'a>>>> {
    let offsets: Vec<_> = tensions
        .iter()
        .map(|state| {
            fb::CultureTensionState::create(
                builder,
                &fb::CultureTensionStateArgs {
                    layerId: state.layer_id,
                    scope: to_fb_culture_layer_scope(state.scope),
                    owner: state.owner,
                    severity: state.severity,
                    timer: state.timer,
                    kind: to_fb_culture_tension_kind(state.kind),
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn to_fb_driver_category(category: SentimentDriverCategory) -> fb::SentimentDriverCategory {
    match category {
        SentimentDriverCategory::Policy => fb::SentimentDriverCategory::Policy,
        SentimentDriverCategory::Incident => fb::SentimentDriverCategory::Incident,
        SentimentDriverCategory::Influencer => fb::SentimentDriverCategory::Influencer,
    }
}

fn to_fb_terrain_type(terrain: TerrainType) -> fb::TerrainType {
    match terrain {
        TerrainType::DeepOcean => fb::TerrainType::DeepOcean,
        TerrainType::ContinentalShelf => fb::TerrainType::ContinentalShelf,
        TerrainType::InlandSea => fb::TerrainType::InlandSea,
        TerrainType::CoralShelf => fb::TerrainType::CoralShelf,
        TerrainType::HydrothermalVentField => fb::TerrainType::HydrothermalVentField,
        TerrainType::TidalFlat => fb::TerrainType::TidalFlat,
        TerrainType::RiverDelta => fb::TerrainType::RiverDelta,
        TerrainType::MangroveSwamp => fb::TerrainType::MangroveSwamp,
        TerrainType::FreshwaterMarsh => fb::TerrainType::FreshwaterMarsh,
        TerrainType::Floodplain => fb::TerrainType::Floodplain,
        TerrainType::AlluvialPlain => fb::TerrainType::AlluvialPlain,
        TerrainType::PrairieSteppe => fb::TerrainType::PrairieSteppe,
        TerrainType::MixedWoodland => fb::TerrainType::MixedWoodland,
        TerrainType::BorealTaiga => fb::TerrainType::BorealTaiga,
        TerrainType::PeatHeath => fb::TerrainType::PeatHeath,
        TerrainType::HotDesertErg => fb::TerrainType::HotDesertErg,
        TerrainType::RockyReg => fb::TerrainType::RockyReg,
        TerrainType::SemiAridScrub => fb::TerrainType::SemiAridScrub,
        TerrainType::SaltFlat => fb::TerrainType::SaltFlat,
        TerrainType::OasisBasin => fb::TerrainType::OasisBasin,
        TerrainType::Tundra => fb::TerrainType::Tundra,
        TerrainType::PeriglacialSteppe => fb::TerrainType::PeriglacialSteppe,
        TerrainType::Glacier => fb::TerrainType::Glacier,
        TerrainType::SeasonalSnowfield => fb::TerrainType::SeasonalSnowfield,
        TerrainType::RollingHills => fb::TerrainType::RollingHills,
        TerrainType::HighPlateau => fb::TerrainType::HighPlateau,
        TerrainType::AlpineMountain => fb::TerrainType::AlpineMountain,
        TerrainType::KarstHighland => fb::TerrainType::KarstHighland,
        TerrainType::CanyonBadlands => fb::TerrainType::CanyonBadlands,
        TerrainType::ActiveVolcanoSlope => fb::TerrainType::ActiveVolcanoSlope,
        TerrainType::BasalticLavaField => fb::TerrainType::BasalticLavaField,
        TerrainType::AshPlain => fb::TerrainType::AshPlain,
        TerrainType::FumaroleBasin => fb::TerrainType::FumaroleBasin,
        TerrainType::ImpactCraterField => fb::TerrainType::ImpactCraterField,
        TerrainType::KarstCavernMouth => fb::TerrainType::KarstCavernMouth,
        TerrainType::SinkholeField => fb::TerrainType::SinkholeField,
        TerrainType::AquiferCeiling => fb::TerrainType::AquiferCeiling,
    }
}

fn to_fb_mountain_kind(kind: MountainKind) -> fb::MountainKind {
    match kind {
        MountainKind::None => fb::MountainKind::None,
        MountainKind::Fold => fb::MountainKind::Fold,
        MountainKind::Fault => fb::MountainKind::Fault,
        MountainKind::Volcanic => fb::MountainKind::Volcanic,
        MountainKind::Dome => fb::MountainKind::Dome,
    }
}

fn to_fb_corruption_subsystem(subsystem: CorruptionSubsystem) -> fb::CorruptionSubsystem {
    match subsystem {
        CorruptionSubsystem::Logistics => fb::CorruptionSubsystem::Logistics,
        CorruptionSubsystem::Trade => fb::CorruptionSubsystem::Trade,
        CorruptionSubsystem::Military => fb::CorruptionSubsystem::Military,
        CorruptionSubsystem::Governance => fb::CorruptionSubsystem::Governance,
    }
}

fn to_fb_influence_scope(scope: InfluenceScopeKind) -> fb::InfluenceScopeKind {
    match scope {
        InfluenceScopeKind::Local => fb::InfluenceScopeKind::Local,
        InfluenceScopeKind::Regional => fb::InfluenceScopeKind::Regional,
        InfluenceScopeKind::Global => fb::InfluenceScopeKind::Global,
        InfluenceScopeKind::Generation => fb::InfluenceScopeKind::Generation,
    }
}

fn to_fb_influence_lifecycle(lifecycle: InfluenceLifecycle) -> fb::InfluenceLifecycle {
    match lifecycle {
        InfluenceLifecycle::Potential => fb::InfluenceLifecycle::Potential,
        InfluenceLifecycle::Active => fb::InfluenceLifecycle::Active,
        InfluenceLifecycle::Dormant => fb::InfluenceLifecycle::Dormant,
    }
}

fn to_fb_culture_layer_scope(scope: CultureLayerScope) -> fb::CultureLayerScope {
    match scope {
        CultureLayerScope::Global => fb::CultureLayerScope::Global,
        CultureLayerScope::Regional => fb::CultureLayerScope::Regional,
        CultureLayerScope::Local => fb::CultureLayerScope::Local,
    }
}

fn to_fb_culture_trait_axis(axis: CultureTraitAxis) -> fb::CultureTraitAxis {
    match axis {
        CultureTraitAxis::PassiveAggressive => fb::CultureTraitAxis::PassiveAggressive,
        CultureTraitAxis::OpenClosed => fb::CultureTraitAxis::OpenClosed,
        CultureTraitAxis::CollectivistIndividualist => {
            fb::CultureTraitAxis::CollectivistIndividualist
        }
        CultureTraitAxis::TraditionalistRevisionist => {
            fb::CultureTraitAxis::TraditionalistRevisionist
        }
        CultureTraitAxis::HierarchicalEgalitarian => fb::CultureTraitAxis::HierarchicalEgalitarian,
        CultureTraitAxis::SyncreticPurist => fb::CultureTraitAxis::SyncreticPurist,
        CultureTraitAxis::AsceticIndulgent => fb::CultureTraitAxis::AsceticIndulgent,
        CultureTraitAxis::PragmaticIdealistic => fb::CultureTraitAxis::PragmaticIdealistic,
        CultureTraitAxis::RationalistMystical => fb::CultureTraitAxis::RationalistMystical,
        CultureTraitAxis::ExpansionistInsular => fb::CultureTraitAxis::ExpansionistInsular,
        CultureTraitAxis::AdaptiveStubborn => fb::CultureTraitAxis::AdaptiveStubborn,
        CultureTraitAxis::HonorBoundOpportunistic => fb::CultureTraitAxis::HonorBoundOpportunistic,
        CultureTraitAxis::MeritOrientedLineageOriented => {
            fb::CultureTraitAxis::MeritOrientedLineageOriented
        }
        CultureTraitAxis::SecularDevout => fb::CultureTraitAxis::SecularDevout,
        CultureTraitAxis::PluralisticMonocultural => fb::CultureTraitAxis::PluralisticMonocultural,
    }
}

fn to_fb_culture_tension_kind(kind: CultureTensionKind) -> fb::CultureTensionKind {
    match kind {
        CultureTensionKind::DriftWarning => fb::CultureTensionKind::DriftWarning,
        CultureTensionKind::AssimilationPush => fb::CultureTensionKind::AssimilationPush,
        CultureTensionKind::SchismRisk => fb::CultureTensionKind::SchismRisk,
    }
}

fn to_fb_knowledge_security_posture(
    posture: KnowledgeSecurityPosture,
) -> fb::KnowledgeSecurityPosture {
    match posture {
        KnowledgeSecurityPosture::Minimal => fb::KnowledgeSecurityPosture::Minimal,
        KnowledgeSecurityPosture::Standard => fb::KnowledgeSecurityPosture::Standard,
        KnowledgeSecurityPosture::Hardened => fb::KnowledgeSecurityPosture::Hardened,
        KnowledgeSecurityPosture::BlackVault => fb::KnowledgeSecurityPosture::BlackVault,
    }
}

fn to_fb_knowledge_countermeasure(
    kind: KnowledgeCountermeasureKind,
) -> fb::KnowledgeCountermeasureKind {
    match kind {
        KnowledgeCountermeasureKind::SecurityInvestment => {
            fb::KnowledgeCountermeasureKind::SecurityInvestment
        }
        KnowledgeCountermeasureKind::CounterIntelSweep => {
            fb::KnowledgeCountermeasureKind::CounterIntelSweep
        }
        KnowledgeCountermeasureKind::Misinformation => {
            fb::KnowledgeCountermeasureKind::Misinformation
        }
        KnowledgeCountermeasureKind::KnowledgeDebtRelief => {
            fb::KnowledgeCountermeasureKind::KnowledgeDebtRelief
        }
    }
}

fn to_fb_knowledge_modifier_source(source: KnowledgeModifierSource) -> fb::KnowledgeModifierSource {
    match source {
        KnowledgeModifierSource::Visibility => fb::KnowledgeModifierSource::Visibility,
        KnowledgeModifierSource::Security => fb::KnowledgeModifierSource::Security,
        KnowledgeModifierSource::Spycraft => fb::KnowledgeModifierSource::Spycraft,
        KnowledgeModifierSource::Culture => fb::KnowledgeModifierSource::Culture,
        KnowledgeModifierSource::Exposure => fb::KnowledgeModifierSource::Exposure,
        KnowledgeModifierSource::Debt => fb::KnowledgeModifierSource::Debt,
        KnowledgeModifierSource::Treaty => fb::KnowledgeModifierSource::Treaty,
        KnowledgeModifierSource::Event => fb::KnowledgeModifierSource::Event,
    }
}

fn to_fb_knowledge_timeline_kind(
    kind: KnowledgeTimelineEventKind,
) -> fb::KnowledgeTimelineEventKind {
    match kind {
        KnowledgeTimelineEventKind::LeakProgress => fb::KnowledgeTimelineEventKind::LeakProgress,
        KnowledgeTimelineEventKind::SpyProbe => fb::KnowledgeTimelineEventKind::SpyProbe,
        KnowledgeTimelineEventKind::CounterIntel => fb::KnowledgeTimelineEventKind::CounterIntel,
        KnowledgeTimelineEventKind::Exposure => fb::KnowledgeTimelineEventKind::Exposure,
        KnowledgeTimelineEventKind::Treaty => fb::KnowledgeTimelineEventKind::Treaty,
        KnowledgeTimelineEventKind::Cascade => fb::KnowledgeTimelineEventKind::Cascade,
        KnowledgeTimelineEventKind::Digest => fb::KnowledgeTimelineEventKind::Digest,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerationState {
    pub id: u16,
    pub name: String,
    pub bias_knowledge: i64,
    pub bias_trust: i64,
    pub bias_equity: i64,
    pub bias_agency: i64,
}
