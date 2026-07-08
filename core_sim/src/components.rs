use std::str::FromStr;

use bevy::{math::UVec2, prelude::*};
use sim_runtime::{KnownTechFragment as ContractKnowledgeFragment, TerrainTags, TerrainType};

use crate::{
    food::FoodModule,
    generations::GenerationId,
    mapgen::MountainType,
    orders::FactionId,
    power::PowerNodeId,
    scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar},
};

/// Represents a discrete tile in the simulation grid.
#[derive(Component, Debug, Clone)]
pub struct Tile {
    pub position: UVec2,
    pub element: ElementKind,
    pub mass: Scalar,
    pub temperature: Scalar,
    pub terrain: TerrainType,
    pub terrain_tags: TerrainTags,
    pub mountain: Option<MountainMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MountainMetadata {
    pub kind: MountainType,
    pub relief: f32,
}

/// Procedural element categories used to vary material behavior.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementKind {
    Ferrite,
    Arborite,
    Zephyrite,
    Lumina,
}

impl ElementKind {
    pub fn thermal_bias(self) -> Scalar {
        match self {
            ElementKind::Ferrite => scalar_from_f32(-6.0),
            ElementKind::Arborite => scalar_from_f32(-2.5),
            ElementKind::Zephyrite => scalar_from_f32(1.5),
            ElementKind::Lumina => scalar_from_f32(4.0),
        }
    }

    pub fn conductivity(self) -> Scalar {
        match self {
            ElementKind::Ferrite => scalar_from_f32(0.35),
            ElementKind::Arborite => scalar_from_f32(0.2),
            ElementKind::Zephyrite => scalar_from_f32(0.65),
            ElementKind::Lumina => scalar_from_f32(0.5),
        }
    }

    pub fn mass_flux(self) -> Scalar {
        match self {
            ElementKind::Ferrite => scalar_from_f32(0.8),
            ElementKind::Arborite => scalar_from_f32(0.4),
            ElementKind::Zephyrite => scalar_from_f32(0.6),
            ElementKind::Lumina => scalar_from_f32(0.5),
        }
    }

    pub fn power_profile(self) -> (Scalar, Scalar, Scalar) {
        match self {
            ElementKind::Ferrite => (
                scalar_from_f32(8.0),
                scalar_from_f32(6.0),
                scalar_from_f32(0.95),
            ),
            ElementKind::Arborite => (
                scalar_from_f32(4.0),
                scalar_from_f32(3.5),
                scalar_from_f32(1.05),
            ),
            ElementKind::Zephyrite => (
                scalar_from_f32(6.5),
                scalar_from_f32(4.0),
                scalar_from_f32(1.1),
            ),
            ElementKind::Lumina => (
                scalar_from_f32(10.0),
                scalar_from_f32(7.0),
                scalar_from_f32(0.9),
            ),
        }
    }

    pub fn from_grid(position: UVec2) -> Self {
        match (position.x + position.y) % 4 {
            0 => ElementKind::Ferrite,
            1 => ElementKind::Arborite,
            2 => ElementKind::Zephyrite,
            _ => ElementKind::Lumina,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(ElementKind::Ferrite),
            1 => Some(ElementKind::Arborite),
            2 => Some(ElementKind::Zephyrite),
            3 => Some(ElementKind::Lumina),
            _ => None,
        }
    }
}

impl From<ElementKind> for u8 {
    fn from(value: ElementKind) -> Self {
        value as u8
    }
}

/// Directed link representing logistics throughput between two tiles.
#[derive(Component, Debug, Clone)]
pub struct LogisticsLink {
    pub from: Entity,
    pub to: Entity,
    pub capacity: Scalar,
    pub flow: Scalar,
}

/// Population representation bound to a home tile.
#[derive(Component, Debug, Clone)]
pub struct PopulationCohort {
    pub home: Entity,
    /// Current position during travel (equals home when stationary).
    pub current_tile: Entity,
    /// Cached total head-count (`= round(children + working + elders)`), kept in sync by
    /// `simulate_population` so the many `.size` readers stay valid.
    pub size: u32,
    /// Dependents — fed and housed, no labor. Fractional (fixed-point) so small per-turn flows
    /// accumulate without rounding to zero on a small band.
    pub children: Scalar,
    /// Working-age — the labor pool (the only bracket that produces).
    pub working: Scalar,
    /// Elders — dependents again, then mortality.
    pub elders: Scalar,
    /// The band's carried food larder (provisions). Filled by this band's foraging, drawn down
    /// per-capita each turn. Local from day one — the same store a settlement/storage-pit will
    /// hold later at larger scale (`docs/plan_settlement_population.md`).
    pub food_store: Scalar,
    pub morale: Scalar,
    pub generation: GenerationId,
    pub faction: FactionId,
    pub knowledge: Vec<KnowledgeFragment>,
    pub migration: Option<PendingMigration>,
}

impl PopulationCohort {
    /// Fixed-point sum of the three age brackets (the authoritative head-count; `size` is its
    /// rounded `u32` cache).
    pub fn total(&self) -> Scalar {
        self.children + self.working + self.elders
    }

    /// Split a head-count into the three brackets by the configured fractions and resync `size`.
    /// Used when spawning a fresh cohort (rehydration restores exact brackets from the snapshot).
    pub fn set_brackets_from_size(&mut self, size: u32, children: f32, working: f32, elders: f32) {
        let total = scalar_from_f32(size as f32);
        self.children = total * scalar_from_f32(children);
        self.working = total * scalar_from_f32(working);
        self.elders = total * scalar_from_f32(elders);
        self.size = self.total().to_u32();
    }

    /// Recompute the `size` cache from the current brackets.
    pub fn sync_size(&mut self) {
        self.size = self.total().to_u32();
    }
}

/// Power node metadata bound to a tile entity.
#[derive(Component, Debug, Clone)]
pub struct PowerNode {
    pub id: PowerNodeId,
    pub base_generation: Scalar,
    pub base_demand: Scalar,
    pub generation: Scalar,
    pub demand: Scalar,
    pub efficiency: Scalar,
    pub storage_capacity: Scalar,
    pub storage_level: Scalar,
    pub stability: Scalar,
    pub surplus: Scalar,
    pub deficit: Scalar,
    pub incident_count: u32,
}

/// Marks a starting population cohort spawned from a scenario profile.
#[derive(Component, Debug, Clone)]
pub struct StartingUnit {
    pub kind: String,
    pub tags: Vec<String>,
}

impl StartingUnit {
    pub fn new(kind: String, tags: Vec<String>) -> Self {
        Self { kind, tags }
    }
}

/// Permanent settlement seeded by a founding action.
#[derive(Component, Debug, Clone)]
pub struct Settlement {
    pub faction: FactionId,
    pub position: UVec2,
}

/// Anchor component for the initial hub within a settlement.
#[derive(Component, Debug, Clone)]
pub struct TownCenter {
    pub construction_radius: u32,
    pub logistics_radius: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HarvestTaskKind {
    #[default]
    Harvest,
    Hunt,
}

impl HarvestTaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HarvestTaskKind::Harvest => "harvest",
            HarvestTaskKind::Hunt => "hunt",
        }
    }
}

impl FromStr for HarvestTaskKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "hunt" | "hunt_game" => Ok(HarvestTaskKind::Hunt),
            _ => Ok(HarvestTaskKind::Harvest),
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct HarvestAssignment {
    pub faction: FactionId,
    pub band_label: String,
    pub module: FoodModule,
    pub target_tile: Entity,
    pub target_coords: UVec2,
    pub travel_remaining: u32,
    pub travel_total: u32,
    pub gather_remaining: u32,
    pub gather_total: u32,
    pub provisions_reward: i64,
    pub trade_goods_reward: i64,
    pub started_tick: u64,
    pub kind: HarvestTaskKind,
}

#[derive(Component, Debug, Clone)]
pub struct ScoutAssignment {
    pub faction: FactionId,
    pub band_label: String,
    pub target_tile: Entity,
    pub target_coords: UVec2,
    pub travel_remaining: u32,
    pub travel_total: u32,
    pub reveal_radius: u32,
    pub reveal_duration: u64,
    pub morale_gain: f32,
    pub started_tick: u64,
}

/// Auto-hunt policy for a Follow: how much biomass the band takes each turn once
/// adjacent. Sustain ≈ regrowth (group stable), Surplus > regrowth (slow decline),
/// Market = large commercial share (fast decline → collapse, boosted trade goods),
/// Eradicate = max (drives the group toward local extinction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FollowPolicy {
    #[default]
    Sustain,
    Surplus,
    Market,
    Eradicate,
}

impl FollowPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            FollowPolicy::Sustain => "sustain",
            FollowPolicy::Surplus => "surplus",
            FollowPolicy::Market => "market",
            FollowPolicy::Eradicate => "eradicate",
        }
    }
}

impl FromStr for FollowPolicy {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "surplus" => Ok(FollowPolicy::Surplus),
            "market" => Ok(FollowPolicy::Market),
            "eradicate" => Ok(FollowPolicy::Eradicate),
            "sustain" | "" => Ok(FollowPolicy::Sustain),
            _ => Err(()),
        }
    }
}

/// What a band does once it catches the fauna group it is pursuing. `Hunt` is a
/// one-shot take (Phase B); `Follow` shadows the group and auto-hunts per policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FaunaPursuitMode {
    #[default]
    Hunt,
    Follow {
        policy: FollowPolicy,
    },
}

/// A band pursuing a moving fauna **group** (herd) by id. Unlike `HarvestAssignment`
/// (fixed tile, precomputed reward), the target and yield are resolved against the
/// live `HerdRegistry` each turn, so the band chases a genuinely moving herd.
#[derive(Component, Debug, Clone)]
pub struct FaunaPursuit {
    pub faction: FactionId,
    pub band_label: String,
    pub fauna_id: String,
    pub mode: FaunaPursuitMode,
    pub elapsed_turns: u32,
    pub started_tick: u64,
}

impl Default for PowerNode {
    fn default() -> Self {
        Self {
            id: PowerNodeId(0),
            base_generation: scalar_zero(),
            base_demand: scalar_zero(),
            generation: scalar_zero(),
            demand: scalar_zero(),
            efficiency: Scalar::one(),
            storage_capacity: scalar_zero(),
            storage_level: scalar_zero(),
            stability: Scalar::one(),
            surplus: scalar_zero(),
            deficit: scalar_zero(),
            incident_count: 0,
        }
    }
}

/// Trade link metadata attached to logistics edges.
#[derive(Component, Debug, Clone)]
pub struct TradeLink {
    pub from_faction: FactionId,
    pub to_faction: FactionId,
    pub throughput: Scalar,
    pub tariff: Scalar,
    pub openness: Scalar,
    pub decay: Scalar,
    pub leak_timer: u32,
    pub last_discovery: Option<u32>,
    pub pending_fragments: Vec<KnowledgeFragment>,
}

impl Default for TradeLink {
    fn default() -> Self {
        Self {
            from_faction: FactionId(0),
            to_faction: FactionId(0),
            throughput: scalar_zero(),
            tariff: scalar_zero(),
            openness: scalar_from_f32(0.25),
            decay: scalar_from_f32(0.01),
            leak_timer: 0,
            last_discovery: None,
            pending_fragments: Vec::new(),
        }
    }
}

/// Knowledge fragment payload carried by trade leaks or migrations.
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeFragment {
    pub discovery_id: u32,
    pub progress: Scalar,
    pub fidelity: Scalar,
}

impl KnowledgeFragment {
    pub fn new(discovery_id: u32, progress: Scalar, fidelity: Scalar) -> Self {
        Self {
            discovery_id,
            progress,
            fidelity,
        }
    }

    pub fn from_contract(fragment: &ContractKnowledgeFragment) -> Self {
        Self {
            discovery_id: fragment.discovery_id,
            progress: Scalar::from_raw(fragment.progress),
            fidelity: Scalar::from_raw(fragment.fidelity),
        }
    }

    pub fn to_contract(&self) -> ContractKnowledgeFragment {
        ContractKnowledgeFragment {
            discovery_id: self.discovery_id,
            progress: self.progress.raw(),
            fidelity: self.fidelity.raw(),
        }
    }
}

pub fn fragments_to_contract(fragments: &[KnowledgeFragment]) -> Vec<ContractKnowledgeFragment> {
    fragments
        .iter()
        .map(|fragment| fragment.to_contract())
        .collect()
}

pub fn fragments_from_contract(fragments: &[ContractKnowledgeFragment]) -> Vec<KnowledgeFragment> {
    fragments
        .iter()
        .map(KnowledgeFragment::from_contract)
        .collect()
}

/// Pending migration payload queued on a population cohort.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingMigration {
    pub destination: FactionId,
    pub eta: u16,
    pub fragments: Vec<KnowledgeFragment>,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            position: UVec2::ZERO,
            element: ElementKind::Ferrite,
            mass: scalar_one(),
            temperature: scalar_zero(),
            terrain: TerrainType::AlluvialPlain,
            terrain_tags: TerrainTags::empty(),
            mountain: None,
        }
    }
}
