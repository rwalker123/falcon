use std::cmp::min;
use std::collections::BTreeMap;
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

/// Commodity key for a band's food larder. `"provisions"` is the reward name foraging, hunt, and
/// husbandry income deposit into the band's local `stores` — provisions left `FactionInventory`
/// entirely (only trade goods stay faction-global); kept as a stable constant.
pub const FOOD: &str = "provisions";

/// A location-local store of goods held by a band (and, later, a populated tile or storage pit).
/// Keyed by commodity so the supply network can balance *any* good; a `BTreeMap` keeps iteration
/// deterministic for balancing and snapshotting. Quantities are fixed-point (`Scalar`) so small
/// per-turn flows accumulate without rounding to zero. An absent key reads as zero, and setting a
/// key to zero prunes it, so two stores with the same goods always compare equal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalStore {
    goods: BTreeMap<String, Scalar>,
}

impl LocalStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current quantity of `item` (zero if absent).
    pub fn get(&self, item: &str) -> Scalar {
        self.goods.get(item).copied().unwrap_or_else(scalar_zero)
    }

    /// Add `amount` (may be negative) to `item`, flooring the result at zero.
    pub fn add(&mut self, item: &str, amount: Scalar) {
        let updated = self.get(item) + amount;
        self.set(item, updated);
    }

    /// Set `item` to `amount` (floored at zero; a zero value prunes the key).
    pub fn set(&mut self, item: &str, amount: Scalar) {
        if amount > scalar_zero() {
            self.goods.insert(item.to_string(), amount);
        } else {
            self.goods.remove(item);
        }
    }

    /// Remove up to `amount` of `item`, returning how much was actually taken.
    pub fn take(&mut self, item: &str, amount: Scalar) -> Scalar {
        let taken = min(amount.max(scalar_zero()), self.get(item));
        self.add(item, -taken);
        taken
    }

    /// `(item, quantity)` pairs in deterministic (sorted-key) order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Scalar)> {
        self.goods.iter().map(|(k, v)| (k.as_str(), *v))
    }
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
    /// The band's carried goods store (food under the `FOOD` key, plus any future commodity).
    /// Filled by this band's foraging, drawn down per-capita each turn, and rebalanced with nearby
    /// bands by the supply network. Local from day one — the same store a settlement/storage-pit
    /// will hold later at larger scale (`docs/plan_settlement_population.md`).
    pub stores: LocalStore,
    pub morale: Scalar,
    /// This turn's signed morale delta (before clamping into `[0, 1]`). Derived each turn by
    /// `simulate_population`; the client renders it as a rising/falling trend arrow. Not
    /// snapshot-persisted — a rehydrated cohort reads `0` until the next turn recomputes it.
    pub last_morale_delta: Scalar,
    /// The dominant *negative* driver behind `last_morale_delta` when morale fell this turn
    /// (`None` when it rose or held), so the client can name *why* — e.g. "harsh terrain". Derived
    /// each turn alongside `last_morale_delta`; not snapshot-persisted.
    pub last_morale_cause: MoraleCause,
    /// The Layer-1 named morale contributors whose signed sum IS `last_morale_delta` (the wellbeing
    /// model's per-band morale breakdown — see `docs/plan_civ_wellbeing.md`). Derived each turn by
    /// `simulate_population`; not snapshot-persisted.
    pub last_morale_contributions: MoraleContributions,
    /// Layer 2 — the share of the band that is unhappy this turn, `g(morale)` (working-weighted at
    /// the migration/grievance stage). `0` = content, `1` = fully discontented. Drives the
    /// productivity modifier stack and migration. Derived each turn; not snapshot-persisted.
    pub discontent_fraction: Scalar,
    /// Layer 2 — the severity × duration grievance accumulator: rises with sustained discontent
    /// (faster when trapped with nowhere to migrate), decays while content. Phase 1 only populates
    /// it (reserved for a future revolution consequence — no consequence reads it yet). Persisted
    /// in the snapshot so rollback preserves the accumulation.
    pub grievance: Scalar,
    /// How many people emigrated **from** this band last turn via discontent-driven migration
    /// (relocated to a happier same-faction band). `0` = none. Derived each turn; not persisted.
    pub last_emigrated: u32,
    /// How many people immigrated **into** this band last turn (a high-morale band is a magnet).
    /// `0` = none. Derived each turn; not persisted.
    pub last_immigrated: u32,
    /// Turns this band has been simulated. Gates knowledge-migration (`simulate_population`) so a
    /// freshly-spawned band must settle for `migration_min_settled_turns` before its population can
    /// emigrate to a neighbor. Persisted in the snapshot so rollback preserves the gate.
    pub age_turns: u32,
    pub generation: GenerationId,
    pub faction: FactionId,
    pub knowledge: Vec<KnowledgeFragment>,
    pub migration: Option<PendingMigration>,
}

/// The dominant negative driver of a cohort's morale on a given turn, surfaced so the client can
/// name *why* morale (and thus population) is falling instead of reporting a vague "low morale".
/// Starvation is deliberately excluded — it is surfaced through the days-of-food path, not morale.
///
/// Snapshot wire encoding (see [`MoraleCause::as_u8`]): `0 = None, 1 = Terrain, 2 = Cold,
/// 3 = Unrest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoraleCause {
    /// Morale rose or held this turn — no dominant negative driver.
    #[default]
    None,
    /// Terrain attrition + logistics hardness dominated — the hex is harsh to live on.
    Terrain,
    /// The temperature-difference penalty dominated.
    Cold,
    /// Crisis impacts + cultural sentiment (unrest) dominated.
    Unrest,
}

impl MoraleCause {
    /// Encode for the snapshot's `moraleCause:ubyte` field: `0=None, 1=Terrain, 2=Cold, 3=Unrest`.
    pub fn as_u8(self) -> u8 {
        match self {
            MoraleCause::None => 0,
            MoraleCause::Terrain => 1,
            MoraleCause::Cold => 2,
            MoraleCause::Unrest => 3,
        }
    }
}

/// Layer 1 of the Civilization Wellbeing model (`docs/plan_civ_wellbeing.md`): the named factors
/// that converge into a band's morale. Morale trends by the **signed sum** of the active
/// contributions each turn; adding a future factor (nutrition/education/technology/government/…)
/// is a new variant plus one contribution — the morale update itself never gets rewritten. The
/// contribution set *is* the per-band morale breakdown the client can itemize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoraleFactor {
    /// Base settling growth (`+population_growth_rate`) — always non-negative.
    Settling,
    /// Terrain attrition + logistics hardness drain (≤ 0).
    Terrain,
    /// Temperature-vs-tolerance climate drain (≤ 0).
    Climate,
    /// Crisis impacts + cultural sentiment (signed).
    Unrest,
}

/// The Phase-1 named morale contributions for a cohort this turn (each signed; their sum IS
/// `last_morale_delta`). A fixed struct rather than a `Vec` to stay allocation-free; a future
/// factor adds a field + a `MoraleFactor` variant. Surfaced in the snapshot so the client can
/// itemize *why* morale is moving.
#[derive(Debug, Clone, Copy, Default)]
pub struct MoraleContributions {
    /// `+population_growth_rate` (base settling growth).
    pub settling: Scalar,
    /// `−terrain pressure` (≤ 0).
    pub terrain: Scalar,
    /// `−climate/cold pressure` (≤ 0).
    pub climate: Scalar,
    /// crisis impacts + cultural sentiment bias (signed).
    pub unrest: Scalar,
}

impl MoraleContributions {
    /// The active contributions as `(factor, signed value)` pairs — the itemized breakdown the
    /// client can render and the single source both `total` and cause attribution iterate. Ordered
    /// by the historical tie-break priority (Terrain ≥ Climate ≥ Unrest) so the dominant-cause scan
    /// is a stable first-max.
    pub fn contributions(&self) -> [(MoraleFactor, Scalar); 4] {
        [
            (MoraleFactor::Terrain, self.terrain),
            (MoraleFactor::Climate, self.climate),
            (MoraleFactor::Unrest, self.unrest),
            (MoraleFactor::Settling, self.settling),
        ]
    }

    /// The signed morale delta this turn — the sum of every contribution.
    pub fn total(&self) -> Scalar {
        self.contributions()
            .iter()
            .fold(scalar_zero(), |acc, (_, value)| acc + *value)
    }

    /// The dominant *negative* contributor as a [`MoraleCause`] (the "why morale fell" label). The
    /// most-negative labeled contribution wins; `Settling` is base growth (never a negative cause),
    /// and ties resolve by `contributions()` order (Terrain ≥ Climate ≥ Unrest).
    pub fn dominant_negative_cause(&self) -> MoraleCause {
        let mut best: Option<(MoraleFactor, Scalar)> = None;
        for (factor, value) in self.contributions() {
            if matches!(factor, MoraleFactor::Settling) || value >= scalar_zero() {
                continue;
            }
            if best.is_none_or(|(_, worst)| value < worst) {
                best = Some((factor, value));
            }
        }
        match best {
            Some((MoraleFactor::Terrain, _)) => MoraleCause::Terrain,
            Some((MoraleFactor::Climate, _)) => MoraleCause::Cold,
            Some((MoraleFactor::Unrest, _)) => MoraleCause::Unrest,
            _ => MoraleCause::None,
        }
    }
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
        let total = Scalar::from_u32(size);
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
