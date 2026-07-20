use std::cmp::min;
use std::collections::BTreeMap;
use std::str::FromStr;

use bevy::{math::UVec2, prelude::*};
use sim_runtime::{
    KnownTechFragment as ContractKnowledgeFragment, RiverChannel, RiverClass, TerrainTags,
    TerrainType,
};

use crate::{
    generations::GenerationId,
    grid_utils::{HEX_CORNER_COUNT, HEX_DIRECTION_COUNT},
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
    /// The biome this tile's RESOURCE yields are read from when it is not `terrain` itself.
    ///
    /// Set **only** on a `NavigableRiver` hex, at the hydrology stamp, to the biome the channel was
    /// cut through (`hydrology.rs`). A navigable river stays *mechanically* water — impassable to
    /// land, sailable, bisecting — but a giant river running through a valley still yields the
    /// valley's forage/graze, not open water. `None` everywhere else (the tile just is its
    /// `terrain`). Read through [`Tile::resource_terrain`]; movement/logistics/attrition keep
    /// keying on `terrain`.
    pub underlying_terrain: Option<TerrainType>,
    pub mountain: Option<MountainMetadata>,
    /// Packed per-side river classes — 2 bits per odd-r direction (see `RiverClass`). Populated
    /// by `generate_hydrology` for **both** hexes flanking every traced river edge, so a hex
    /// always agrees with its neighbour about the river between them.
    ///
    /// This is the gameplay primitive a future movement system reads: entering this hex across
    /// direction `d` crosses `river_class_on_side(d)`. Nothing consumes it yet — by design.
    pub river_edges: u16,
    /// Packed per-**corner** river inflow — the same 2-bits-per-slot layout as `river_edges`, but
    /// keyed by hex *corner* (`grid_utils::HEX_CORNER_COUNT`, the client's screen-space vertex
    /// order) instead of by side.
    ///
    /// `generate_hydrology` sets it on the **first hex of a `NavigableRiver` chain** only, at the
    /// corner where the edge-river chain terminated, with the class of the last edge that chain
    /// emitted. An edge river runs corner-to-corner *along* a side, so it ends at a **vertex** —
    /// this field is that vertex. `river_edges` records which sides carry a river and cannot say
    /// this: a trunk hex can flank three river edges, which have two candidate chain-ends between
    /// them, so the renderer would be guessing where the tributary actually arrives.
    ///
    /// Zero on every other tile, and zero for a river that was navigable from its first step (no
    /// edge chain, so no inflow to name).
    pub river_inflow: u16,
    /// Packed per-side **channel exits** — 1 bit per odd-r direction (see `RiverChannel`): does
    /// this hex's navigable channel flow out through side `dir`?
    ///
    /// A navigable river is a chain of water hexes, and a chain is a **path**: a hex connects to its
    /// upstream and downstream neighbours and to nothing else. Terrain alone cannot say which those
    /// are — a renderer that arms every navigable/water neighbour cross-links adjacent chains into a
    /// **web**. Only the tracer knows the chain, so `generate_hydrology` writes it here, symmetric
    /// across each shared side (both hexes of a consecutive pair agree), plus one exit on the final
    /// hex pointing at the water body/delta the river drains into — otherwise the drawn river stops
    /// one hex short of the sea. A confluence hex carries the **union** of the chains through it.
    pub river_channel: u8,
}

impl Tile {
    /// Terrain that drives this tile's RESOURCE yields. A navigable river yields the
    /// valley it cut, not open water; everywhere else it is just `terrain`.
    pub fn resource_terrain(&self) -> TerrainType {
        self.underlying_terrain.unwrap_or(self.terrain)
    }

    /// The class of river running along side `dir` (odd-r direction, `0..6`). An out-of-range
    /// direction reads `None` — this is a lookup, not an assertion site.
    pub fn river_class_on_side(&self, dir: u8) -> RiverClass {
        if usize::from(dir) >= HEX_DIRECTION_COUNT {
            return RiverClass::None;
        }
        RiverClass::from_bits(self.river_edges >> (u32::from(dir) * RiverClass::BITS_PER_DIR))
    }

    /// Set the class of river running along side `dir`. Out-of-range directions are ignored.
    pub fn set_river_class_on_side(&mut self, dir: u8, class: RiverClass) {
        if usize::from(dir) >= HEX_DIRECTION_COUNT {
            return;
        }
        let shift = u32::from(dir) * RiverClass::BITS_PER_DIR;
        self.river_edges &= !(RiverClass::SLOT_MASK << shift);
        self.river_edges |= class.bits() << shift;
    }

    /// Whether any of the six sides carries a river.
    pub fn has_any_river_edge(&self) -> bool {
        self.river_edges != 0
    }

    /// The class of the edge river arriving at hex corner `corner` (`0..6`, see
    /// `grid_utils::HEX_CORNER_COUNT`). An out-of-range corner reads `None` — this is a lookup,
    /// not an assertion site.
    pub fn river_class_at_corner(&self, corner: u8) -> RiverClass {
        if usize::from(corner) >= HEX_CORNER_COUNT {
            return RiverClass::None;
        }
        RiverClass::from_bits(
            self.river_inflow >> (u32::from(corner) * RiverClass::BITS_PER_CORNER),
        )
    }

    /// Set the class of the edge river arriving at hex corner `corner`. Out-of-range corners are
    /// ignored.
    pub fn set_river_class_at_corner(&mut self, corner: u8, class: RiverClass) {
        if usize::from(corner) >= HEX_CORNER_COUNT {
            return;
        }
        let shift = u32::from(corner) * RiverClass::BITS_PER_CORNER;
        self.river_inflow &= !(RiverClass::SLOT_MASK << shift);
        self.river_inflow |= class.bits() << shift;
    }

    /// Whether any of the six corners takes an edge river's inflow.
    pub fn has_any_river_inflow(&self) -> bool {
        self.river_inflow != 0
    }

    /// Whether this hex's navigable channel flows out through side `dir` (odd-r direction, `0..6`).
    /// An out-of-range direction reads `false` — this is a lookup, not an assertion site.
    pub fn channel_exits(&self, dir: u8) -> bool {
        if usize::from(dir) >= HEX_DIRECTION_COUNT {
            return false;
        }
        (self.river_channel >> (u32::from(dir) * RiverChannel::BITS_PER_DIR))
            & RiverChannel::SLOT_MASK
            != 0
    }

    /// Record a channel exit through side `dir`. Out-of-range directions are ignored. Bits are
    /// **OR-ed**: a hex where two chains meet carries the union of their exits, never the last one
    /// written.
    pub fn set_channel_exit(&mut self, dir: u8) {
        if usize::from(dir) >= HEX_DIRECTION_COUNT {
            return;
        }
        self.river_channel |=
            RiverChannel::SLOT_MASK << (u32::from(dir) * RiverChannel::BITS_PER_DIR);
    }

    /// Whether this hex carries a navigable channel at all.
    pub fn has_any_channel_exit(&self) -> bool {
        self.river_channel != 0
    }
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
    /// The food the band's people **actually ate** this turn (`min(food_demand, larder)` at the
    /// turn's *opening* brackets — the real `stores` debit `advance_demographics` took, before the
    /// same turn's births/aging change the head-count). This — not a re-derived `food_demand` on the
    /// *post*-turn brackets — is the consumption term of the larder ledger identity
    /// `larder_delta == food_income − food_consumption − pen_feed_upkeep`, so it holds by
    /// construction whether the band is fully fed or starving (the debit symmetry of
    /// `LaborAllocation::last_pen_feed_upkeep`, the food the pen actually paid). Derived each turn by
    /// `simulate_population`; not snapshot-persisted — a rehydrated cohort reads `0` until the next
    /// turn recomputes it.
    pub last_food_consumption: f32,
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

/// Positive marker for a **real** band — one that participates in the population/settlement arc
/// (demographics, migration, sedentarization, startup seeding, supply networks, default-band
/// command pickers). Attached to every band spawned by worldgen. A detached [`Expedition`]
/// deliberately **lacks** this marker, so it is excluded from those systems *by construction* — the
/// safe default survives new systems added to the settlement arc. A future breakaway-to-new-band is
/// an expedition that drops `Expedition` and gains `ResidentBand` (`docs/plan_exploration_and_sites.md`).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ResidentBand;

/// What an expedition was sent to do: `Scout` (explore + report the map, PR 1) or `Hunt` (follow a
/// migratory herd, harvest food, deliver it, PR 2) — two verbs on one traveling-party system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpeditionMission {
    /// Explore toward a target and report the map + any Wondrous Sites it uncovers.
    Scout,
    /// Follow the herd `fauna_id`, harvest a **productive** hunt's worth of food each turn into the
    /// party's larder, and deliver it back to the band. `fauna_id` keys `HerdRegistry::find`. The
    /// `policy` ([`FollowPolicy`], chosen at launch) governs the take floor + trip behaviour: Sustain
    /// = one conservative harvest to the sustain floor + done; Surplus = one full-cap haul + done;
    /// Market = repeated full-cap trips (grind down); Eradicate = hunt to extinction, delivers no food.
    Hunt {
        fauna_id: String,
        policy: FollowPolicy,
    },
}

impl ExpeditionMission {
    /// Stable wire/snapshot key for the mission (client discriminator).
    pub fn as_str(&self) -> &'static str {
        match self {
            ExpeditionMission::Scout => "scout",
            ExpeditionMission::Hunt { .. } => "hunt",
        }
    }

    /// Parse a mission from its wire keys (snapshot restore). `"hunt"` reconstructs `Hunt { fauna_id,
    /// policy }` from `target_herd` + `policy` (via `FollowPolicy::from_str`, default Sustain);
    /// anything else is `Scout`.
    pub fn from_wire(kind: &str, target_herd: &str, policy: &str) -> Self {
        match kind {
            "hunt" => ExpeditionMission::Hunt {
                fauna_id: target_herd.to_string(),
                policy: policy.parse().unwrap_or(FollowPolicy::Sustain),
            },
            _ => ExpeditionMission::Scout,
        }
    }

    /// The target herd id for a `Hunt` mission (empty for `Scout`) — the snapshot `expeditionTargetHerd`.
    pub fn target_herd(&self) -> &str {
        match self {
            ExpeditionMission::Hunt { fauna_id, .. } => fauna_id,
            ExpeditionMission::Scout => "",
        }
    }

    /// The take policy string for a `Hunt` mission (empty for `Scout`) — the snapshot
    /// `expeditionHuntPolicy`.
    pub fn hunt_policy_str(&self) -> &'static str {
        match self {
            ExpeditionMission::Hunt { policy, .. } => policy.as_str(),
            ExpeditionMission::Scout => "",
        }
    }
}

/// The expedition's lifecycle phase. Scout: `Outbound` toward a target; `AwaitingOrders` parked at
/// the target (the decision point — chain a `move_band` waypoint or `recall_expedition`). Hunt:
/// `Hunting` (chase the herd + harvest) and `Delivering` (run carried food to the band, then
/// auto-relaunch). Shared: `Returning` chasing the home band's live tile to fold back on recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpeditionPhase {
    Outbound,
    AwaitingOrders,
    Returning,
    Hunting,
    Delivering,
}

impl ExpeditionPhase {
    /// Stable wire/snapshot key for the phase (client marker state discriminator).
    pub fn as_str(&self) -> &'static str {
        match self {
            ExpeditionPhase::Outbound => "outbound",
            ExpeditionPhase::AwaitingOrders => "awaiting",
            ExpeditionPhase::Returning => "returning",
            ExpeditionPhase::Hunting => "hunting",
            ExpeditionPhase::Delivering => "delivering",
        }
    }

    /// Parse a phase from its wire key (snapshot restore). Unknown keys default to `Outbound`.
    pub fn from_wire(s: &str) -> Self {
        match s {
            "awaiting" => ExpeditionPhase::AwaitingOrders,
            "returning" => ExpeditionPhase::Returning,
            "hunting" => ExpeditionPhase::Hunting,
            "delivering" => ExpeditionPhase::Delivering,
            _ => ExpeditionPhase::Outbound,
        }
    }
}

/// Marks a detached traveling party (a scouting/hunting expedition). Reuses `PopulationCohort` +
/// `BandTravel` + `LaborAllocation` + `StartingUnit` machinery, but is excluded from the
/// population/settlement arc (it lacks [`ResidentBand`]) and from live faction fog reveal
/// (`Without<Expedition>` in `calculate_visibility`). Discovery is **communication-range gated**: it
/// buffers the tiles it observes in `pending_reveal` and `advance_expeditions` flushes them to the
/// faction map as `Discovered` only while within comm range of the home band. Snapshot-persisted so
/// a rollback preserves an in-flight expedition and its unreported findings.
#[derive(Component, Debug, Clone)]
pub struct Expedition {
    /// The real band that outfitted this party. `Returning` chases this band's **live** tile (bands
    /// are nomadic), and fold-back deposits the party's workers + leftover provisions here.
    pub home_band: Entity,
    pub mission: ExpeditionMission,
    pub phase: ExpeditionPhase,
    /// Whether the arrival ("reached X — awaiting orders") feed line has fired for the current
    /// `AwaitingOrders` latch; reset to `false` when a new `move_band` order relaunches the party.
    pub announced: bool,
    /// Observed-but-unreported tile coordinates (deduped). Flushed to the faction map as
    /// `Discovered` when the party is within comm range of its home band, then cleared.
    pub pending_reveal: Vec<UVec2>,
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

/// Whole assignable workers a band's working-age bracket supplies this turn. Only *whole*
/// people can be staffed onto a source, so this floors the fractional `working` Scalar (which
/// otherwise carries sub-person demographic precision). The `Σ assignments ≤ available` invariant
/// on [`LaborAllocation`] is enforced against this count.
pub fn available_workers(working: Scalar) -> u32 {
    (working.raw().max(0) / Scalar::SCALE) as u32
}

/// A single labor demand a band can staff from its working-age pool (Early-Game Labor, slice 3a):
/// an in-range food source (Forage tile / Hunt herd) or a band-wide role (Scout / Warrior).
/// The band is a labor pool drawing subsistence from many sources at once
/// (`docs/plan_early_game_labor.md`).
#[derive(Debug, Clone, PartialEq)]
pub enum LaborTarget {
    /// Gather food from a food-module tile within `band_work_range` under a take policy. Stored as
    /// coordinates (not an entity) so a moving band re-resolves the tile each turn — an out-of-range
    /// tile simply yields 0 that turn without dropping the assignment. The `policy`
    /// (Sustain/Surplus/Market/Eradicate) sizes the per-turn draw on the tile's depletable forage
    /// patch, the plant mirror of the Hunt policy (§0-iii, parity with hunting).
    Forage { tile: UVec2, policy: FollowPolicy },
    /// Hunt a fauna group by id under a take policy. The band tracks a roaming herd up to
    /// `band_work_range + hunt_leash_tiles` (leashed follow); past that the assignment lapses.
    Hunt {
        fauna_id: String,
        policy: FollowPolicy,
    },
    /// Reveal fog outward from the band (band-wide role, no food yield).
    Scout,
    /// Guard the band (band-wide role). Inert until the predator slice consumes it — it only
    /// occupies workers against the Σ invariant.
    Warrior,
}

impl LaborTarget {
    /// The stable role key (also the snapshot `kind` string and the `activity` summary).
    pub fn kind(&self) -> &'static str {
        match self {
            LaborTarget::Forage { .. } => "forage",
            LaborTarget::Hunt { .. } => "hunt",
            LaborTarget::Scout => "scout",
            LaborTarget::Warrior => "warrior",
        }
    }

    /// Whether two targets name the **same source** (so re-assigning replaces rather than
    /// duplicates). Forage is keyed by tile and Hunt by herd id — for both, the take policy is a
    /// mutable property of the same source (a policy change on the same tile/herd replaces it) — and
    /// the band-wide roles are singletons.
    pub fn same_source(&self, other: &LaborTarget) -> bool {
        match (self, other) {
            (LaborTarget::Forage { tile: a, .. }, LaborTarget::Forage { tile: b, .. }) => a == b,
            (LaborTarget::Hunt { fauna_id: a, .. }, LaborTarget::Hunt { fauna_id: b, .. }) => {
                a == b
            }
            (LaborTarget::Scout, LaborTarget::Scout) => true,
            (LaborTarget::Warrior, LaborTarget::Warrior) => true,
            _ => false,
        }
    }
}

/// One staffed labor demand: a target and the whole-worker head-count assigned to it.
#[derive(Debug, Clone, PartialEq)]
pub struct LaborAssignment {
    pub target: LaborTarget,
    pub workers: u32,
}

/// Retained per-source food-yield telemetry for one labor assignment this turn (derived, not
/// persisted). `actual` = the provisions the source actually produced this turn; `sustainable` =
/// the provisions it could yield *without drawing down its stock*. Forage is inexhaustible in
/// today's model so its `sustainable` is defined equal to `actual`; a Hunt's `sustainable` is the
/// herd's net regrowth this turn (`net_biomass_delta(..).max(0) × provisions_per_biomass`, scaled
/// by the same output multiplier). A per-turn `actual > sustainable` is the (client-derived)
/// overhunting signal — a *leading* flow indicator, distinct from the stock-based `ecology_phase`.
///
/// `workers_needed` = the **minimum** assigned workers that would have produced the same take — the
/// **overstaffing** signal. A source's take is `min(production, workers × per_worker_capacity)`; when
/// the binding constraint is NOT labor, the extra workers were idle. It is
/// `ceil(actual / per_worker_capacity)` clamped into `[1, assigned]` when anything was taken, else
/// `0`. `workers_needed < assigned` ⇒ the source is overstaffed (client flags the wasted labor).
/// **Derived at every rung** since slice 7 — the hardcoded `1` a managed source used to report
/// (`TENDED_SOURCE_WORKERS_NEEDED`) claimed one worker could carry home whatever the land offered, so
/// "max N useful here" read `1` on a Field paying ten workers' worth.
///
/// `wasted` = **the understaffing signal, the exact mirror of `workers_needed`'s overstaffing one**:
/// `production − actual`, the food this source offered that the crew could not collect (`0` when
/// collection was not the binding constraint). *Production* is what the source hands over this turn —
/// the policy ceiling at rungs 1–2, the managed rate at rung 3 — and *collection* is
/// `workers × per_worker_capacity`, so the two signals answer the two halves of "is this source
/// correctly staffed?": `workers_needed < workers` ⇒ drop some, `wasted > 0` ⇒ add some. Derived
/// per-turn; on rung 3 (a Field) it is genuinely food left standing, on the drawn-down plant rungs it
/// stays in the stock and regrows, and **on any animal rung it is meat left to rot** — a hunt kills
/// *whole animals* (slice 8), so a party that cannot haul a whole one still takes it and wastes the
/// rest. On an animal source *production* is therefore the biomass of the animals **killed**, not the
/// escapement the herd could have spared: an animal you didn't kill was never produced, it is still
/// alive (`fauna::forecast_production_and_take`).
///
/// `overdraws` = **does this take draw the stock below what it sustains** — THE ⚠, answered by the sim
/// rather than derived by the client from `actual > sustainable`. That comparison stopped working when
/// the hunt began taking whole animals (slice 8): a Sustain hunt is **escapement to `K/2`**, so it
/// lands the herd exactly on its most-productive biomass and is *sustainable by construction* — but it
/// pays in **lumps** (nothing for 6 turns, then a whole mammoth), so `actual > sustainable` fires on
/// every kill turn. A ⚠ on the turn you correctly harvest a mammoth trains the player to ignore the
/// one signal that matters. So `sustainable` keeps reporting the honest **long-run MSY rate** ("this
/// herd sustains ~0.78/turn on average"), `actual` swings — that swing is *true*, and it is the
/// mechanic — and this flag says whether the policy overdraws at all. It is false for Sustain and the
/// investment rungs (which sit on Sustain's escapement floor) and for every managed rung-3 source;
/// true for Surplus/Market/Eradicate, which genuinely draw down toward the collapse threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceYield {
    pub actual: f32,
    pub sustainable: f32,
    pub wasted: f32,
    pub workers_needed: u32,
    pub overdraws: bool,
}

impl SourceYield {
    /// A source that has produced nothing: the row every assignment starts each turn's resolution
    /// with (so an arm that bails — out of range, module lost, herd gone — leaves a correct 0-yield
    /// row), and the row a freshly-staffed assignment carries until it is seeded from its pre-commit
    /// forecast (`set_source_yield`) or resolved by a turn.
    pub const ZERO: Self = Self {
        actual: 0.0,
        sustainable: 0.0,
        wasted: 0.0,
        workers_needed: 0,
        // Nothing was taken, so nothing was overdrawn.
        overdraws: false,
    };
}

/// A band's partition of its working-age pool across labor demands. Replaces the retired
/// single-task model (`HarvestAssignment`/`ScoutAssignment`/`FaunaPursuit`): a band now draws from
/// many sources at once, with the invariant `Σ assignments.workers ≤ available_workers(working)`.
/// Unassigned workers are **idle** — they eat but produce nothing (no auto-forage).
#[derive(Component, Debug, Clone, Default)]
pub struct LaborAllocation {
    pub assignments: Vec<LaborAssignment>,
    /// Per-turn, per-source yield telemetry — one entry per `assignments` in the **same iteration
    /// order** (so the snapshot zips by index). Rebuilt from scratch each turn in
    /// `advance_labor_allocation`; it is **derived, not persisted** (rollback restores only the
    /// assignments via `labor_allocation_from_state`, leaving this empty until the next tick) and is
    /// **excluded from equality** (see the manual `PartialEq` below) so it can never perturb the
    /// persisted-intent comparison.
    pub last_yields: Vec<SourceYield>,
    /// **The food this band actually PAID for pen feed this turn** — the summed `paid` returned by
    /// `LocalStore::take` in the corral-tend branch of `advance_labor_allocation`, across every pen it
    /// keeps. The *real debit*, not the demanded amount: a band that could only part-pay records only
    /// what it handed over (and its herds starve for the rest).
    ///
    /// **Why it must exist.** A pen's feed is taken straight off `cohort.stores`, so it appears in
    /// **neither** `food_income` (Σ per-source `actual`) nor `food_consumption` (the food the
    /// *people* actually ate, `PopulationCohort::last_food_consumption`). Without exporting it the
    /// band's net-food readout overstates the surplus by
    /// exactly the upkeep and the player watches the larder drain with no explanation. Exported as
    /// `PopulationCohortState.pen_feed_upkeep` so the client can render "my people ate X" and "my
    /// animals ate Y" as **separate lines** (deliberately NOT folded into `food_consumption` — that
    /// separation is the readout the corral arc exists to give), and so the sim, not the client, is the
    /// one doing the arithmetic. It closes the identity
    ///
    /// ```text
    /// larder_delta == food_income − food_consumption − pen_feed_upkeep
    /// ```
    ///
    /// which `core_sim/tests/fauna_husbandry.rs` pins against a real turn.
    ///
    /// Same treatment as `last_yields`: rebuilt from scratch each turn, **derived, not persisted** (a
    /// rehydrated cohort reads `0.0` until the next tick), and **excluded from equality** below so it
    /// can never perturb the persisted-intent comparison.
    pub last_pen_feed_upkeep: f32,
}

/// Equality is **intent only** — two allocations with equal `assignments` are equal regardless of
/// the derived `last_yields` telemetry. This keeps `last_yields` out of any rollback / persisted-
/// state comparison (it is deliberately not part of the assignment's identity).
impl PartialEq for LaborAllocation {
    fn eq(&self, other: &Self) -> bool {
        self.assignments == other.assignments
    }
}

impl LaborAllocation {
    /// Total workers currently staffed across all assignments.
    pub fn assigned_total(&self) -> u32 {
        self.assignments.iter().map(|a| a.workers).sum()
    }

    /// Total workers staffed on the given source (matched by [`LaborTarget::same_source`], so a
    /// singleton role like `Scout`/`Warrior` sums its one assignment). Used by the visibility pass
    /// to read a band's Scout head-count for its sight-range bonus.
    pub fn workers_on(&self, target: &LaborTarget) -> u32 {
        self.assignments
            .iter()
            .filter(|a| a.target.same_source(target))
            .map(|a| a.workers)
            .sum()
    }

    /// Keep the derived `last_yields` the same length as `assignments` — the snapshot **zips the two
    /// by index**, so a mutation that adds/removes an assignment without touching the telemetry would
    /// hand one source's yield row to another. Padding with [`SourceYield::ZERO`] is the correct
    /// default: a source with no telemetry has produced nothing yet.
    fn align_yields(&mut self) {
        self.last_yields
            .resize(self.assignments.len(), SourceYield::ZERO);
    }

    /// Set/replace the worker count for `target`, keeping `Σ ≤ available`. `workers == 0` removes
    /// the assignment (per-source unassign — the new "cancel"). An over-budget request is
    /// **clamped** to the free headroom (not rejected). Returns the worker count actually applied
    /// so the caller can report a clamp.
    ///
    /// The touched source's yield telemetry is dropped alongside its assignment and a freshly-staffed
    /// source gets a [`SourceYield::ZERO`] row, which the command handler immediately overwrites with
    /// the source's pre-commit forecast (`set_source_yield`) so the client never displays `+0.00` for
    /// an assignment that will in fact produce next turn.
    pub fn set_assignment(&mut self, target: LaborTarget, workers: u32, available: u32) -> u32 {
        // Free headroom excludes any existing assignment on the same source (it is being replaced).
        let others: u32 = self
            .assignments
            .iter()
            .filter(|a| !a.target.same_source(&target))
            .map(|a| a.workers)
            .sum();
        let headroom = available.saturating_sub(others);
        let applied = workers.min(headroom);
        self.align_yields();
        // Drop any prior assignment on this source (and its now-stale telemetry row), then re-add if
        // non-zero (captures a new policy).
        if let Some(idx) = self
            .assignments
            .iter()
            .position(|a| a.target.same_source(&target))
        {
            self.assignments.remove(idx);
            self.last_yields.remove(idx);
        }
        if applied > 0 {
            self.assignments.push(LaborAssignment {
                target,
                workers: applied,
            });
            self.last_yields.push(SourceYield::ZERO);
        }
        applied
    }

    /// Overwrite one source's derived yield telemetry row (assign-time **forecast seeding**: the row
    /// is set to what the source is expected to produce next turn, so the map annotation and the band
    /// panel show the real number the moment workers are committed instead of `+0.00`). A no-op when
    /// the source is not staffed.
    pub fn set_source_yield(&mut self, target: &LaborTarget, yields: SourceYield) {
        self.align_yields();
        if let Some(idx) = self
            .assignments
            .iter()
            .position(|a| a.target.same_source(target))
        {
            self.last_yields[idx] = yields;
        }
    }

    /// Trim assignments so `Σ ≤ available` (called each turn in case `working` shrank). Reduces
    /// from the last assignment(s) first, dropping any that reach zero.
    pub fn normalize(&mut self, available: u32) {
        let mut total = self.assigned_total();
        while total > available {
            let excess = total - available;
            let Some(last) = self.assignments.last_mut() else {
                break;
            };
            if last.workers > excess {
                last.workers -= excess;
            } else {
                self.assignments.pop();
            }
            total = self.assigned_total();
        }
        self.align_yields();
    }

    /// Clear every assignment (the repurposed `cancel_order` — band goes fully idle).
    pub fn clear(&mut self) {
        self.assignments.clear();
        self.last_yields.clear();
    }
}

/// A pending `move_band` order: the band advances toward `target` at
/// `band_move_tiles_per_turn`/turn, updating `current_tile`/`home` until it arrives, then the
/// component is removed. Not snapshot-persisted (a rollback mid-move cancels the travel), mirroring
/// the retired pursuit's non-persistence.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BandTravel {
    pub target: UVec2,
}

/// Take policy for a worked food source — shared by the Forage and Hunt labor arms.
///
/// **The four extractive rungs** size how much biomass the band draws each turn: Sustain ≈ regrowth
/// (source stable), Surplus > regrowth (slow decline), Market = large commercial share (fast decline
/// → collapse, boosted trade goods), Eradicate = max (drives the source toward local extinction).
///
/// **The three investment rungs** (the intensification ladder's rung-transition verbs) are *not*
/// extractive: they spend the crew's turns **preparing the ground / taming the herd / building the
/// pen** instead of gathering. While preparing, the source's take ceiling is only the rung's
/// `yield_fraction_while_building` × its **Sustain (MSY)** ceiling — a deliberate **yield dip**,
/// drawn sustainably so the source stays healthy — and it accrues the source's build meter
/// (`ForagePatch::cultivation_progress` / `Herd::domestication_progress` / `Herd::corral_progress`)
/// at the rung's `progress_per_turn`. At progress `1.0` the source becomes a **tended patch /
/// pastoral herd / corralled herd** and pays the full managed yield. This makes intensifying an
/// *investment with a real up-front cost*, gated on the player's time horizon, instead of a free
/// by-product of Sustain.
///
/// All four are **kind-specific** (validated at `assign_labor`): `Cultivate` and `Sow` are
/// Forage-only, `Tame` and `Corral` are Hunt-only. See [`FollowPolicy::valid_for_forage`] /
/// [`FollowPolicy::valid_for_hunt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FollowPolicy {
    #[default]
    Sustain,
    Surplus,
    Market,
    Eradicate,
    /// **Forage-only.** Prepare the patch into a tended crop (see the enum docs).
    Cultivate,
    /// **Forage-only.** Sow a **Field** — the plant rung-3 verb, the twin of `Corral`
    /// (`docs/plan_intensification_ladder.md` §2). It **places** a food source: it builds a Field
    /// *even where no patch existed*, because seed travels — the one asymmetry with the animal
    /// branch, where `Corral` needs a herd you already tamed.
    ///
    /// **But only on ground the land itself will farm, and that is SCARCE.** Rung 3 knows how to move
    /// seed, not how to *fertilize*, so the ground must already do the fertilizing: the
    /// `plant:field` rung's **`site_requirement`** demands **very fertile** ground (`min_forage_capacity`
    /// 195 — the river-deposit class: delta / floodplain / alluvial plain) that is **near fresh water**
    /// (`requires_fresh_water` — a river along one of its sides, fresh-water ground, or a lake/channel
    /// next door; a salt coast does **not** count). Measured on the standard map: **46 sowable tiles of
    /// 4160**. Merely bearing *some* food is nowhere near enough (2328 tiles do). Making thin or dry
    /// ground farmable is rung 4 (Worked Land), a later arc — which will be a **looser copy of that
    /// same record**.
    ///
    /// The rule lives on the rung, never here: `forage::rung_site_refusal` is the one seam the `sow`
    /// command, the labor arm and the wire all resolve through.
    Sow,
    /// **Hunt-only.** Tame a wild herd into pastoral livestock (see the enum docs) — the animal
    /// rung-2 verb. Sustain no longer tames anything: it only *teaches* the faction Herding.
    Tame,
    /// **Hunt-only.** Build the pen for a domesticated herd (see the enum docs).
    Corral,
}

impl FollowPolicy {
    /// The four **extractive** rungs, in player-facing order (gentlest → harshest). This is the
    /// *expedition's whole axis*: a detached party can only take, never invest (the two investment
    /// policies are place-bound work a resident band does — `send_hunt_expedition` rejects them), so
    /// the snapshot's per-herd `hunt_trip_estimates` export walks exactly this list. Emitting a trip
    /// estimate for `Cultivate`/`Tame`/`Corral` would be a number for a trip that cannot be launched.
    pub const EXTRACTIVE: [FollowPolicy; 4] = [
        FollowPolicy::Sustain,
        FollowPolicy::Surplus,
        FollowPolicy::Market,
        FollowPolicy::Eradicate,
    ];

    /// Every policy a **Hunt** assignment accepts — the four extractive rungs **plus the two animal
    /// investment rungs `Tame` (rung 2) and `Corral` (rung 3)**; `Cultivate` is Forage-only. The
    /// single source for "iterate a herd's policies": the snapshot's per-herd `hunt_policy_ceilings`
    /// (the BAND / local-hunt yield preview) export walks this, so a player sees each investment's
    /// deliberately dipped yield *before* committing to it. Keep in sync with
    /// [`FollowPolicy::valid_for_hunt`].
    pub const HUNT_POLICIES: [FollowPolicy; 6] = [
        FollowPolicy::Sustain,
        FollowPolicy::Surplus,
        FollowPolicy::Market,
        FollowPolicy::Eradicate,
        FollowPolicy::Tame,
        FollowPolicy::Corral,
    ];

    /// **Is this an INVESTMENT rung — a rung-transition verb rather than a way of taking?**
    ///
    /// **THE definition, and the one place any site may ask.** "Investment" is defined as *the
    /// complement of [`FollowPolicy::EXTRACTIVE`]* — the enum docs already say so ("they are exactly
    /// the policies **not** in `EXTRACTIVE`"), so deriving it here rather than re-listing the verbs is
    /// the *statement*, not a shortcut.
    ///
    /// **It exists because hand-written lists of these verbs rot.** Two had already rotted before this
    /// was factored out: `send_hunt_expedition`'s launch gate silently accepted `tame`, and
    /// `hunt_expedition_ceiling`'s `matches!` was missing it too — so a Tame expedition sailed past the
    /// `debug_assert!` meant to catch it and quietly computed a *plausible* pastoral-dip ceiling, which
    /// is precisely the "fallback hiding the hole" the assert exists to prevent. Every predicate site
    /// now routes through here; a **new investment verb needs no edit at any of them**.
    ///
    /// Note the complement of a *const list* is deliberate here where
    /// [`FollowPolicy::teaches_knowledge`] is deliberately an exhaustive `match`: teaching is a
    /// *judgement* about a new verb that someone must make (so it should fail to compile), whereas
    /// "is it an investment" is a *fact* about which grouping it is in — and
    /// `follow_policy_teaching_matches_the_extractive_grouping` pins the two together, so they cannot
    /// disagree.
    pub fn is_investment(self) -> bool {
        !Self::EXTRACTIVE.contains(&self)
    }

    /// **Does a take under this policy draw the stock below what it sustains?** — THE ⚠ predicate
    /// ([`SourceYield::overdraws`]), and the exact inverse of "is this policy stewardship" (see
    /// [`FollowPolicy::teaches_knowledge`], which turns on the same restraint/overdraw split — the two
    /// are pinned against each other by `follow_policy_overdrawing_is_the_inverse_of_stewardship`).
    ///
    /// Since slice 8 every hunt policy is **escapement to a floor** (`fauna::hunt_policy_floor`), so
    /// this is simply *"is that floor below the source's sustainable operating point?"*:
    /// - **`Sustain`** — floor `K/2`, the MSY point itself. It cannot overdraw: the take lands the
    ///   herd **exactly** on its most-productive biomass and never below. Sustainable *by
    ///   construction*, so a ⚠ there would be meaningless — which is the whole reason this predicate
    ///   exists rather than the client comparing `actual > sustainable` (a lumpy whole-animal take
    ///   exceeds the long-run MSY rate on every kill turn while being perfectly sustainable).
    /// - **`Tame` / `Corral`** — the investment rungs sit on Sustain's floor and take a *fraction* of
    ///   it. Strictly gentler than Sustain; they cannot overdraw either.
    /// - **`Surplus` / `Market`** — floor at the collapse (Allee) threshold: a real draw-down.
    /// - **`Eradicate`** — no floor at all.
    /// - **`Cultivate` / `Sow`** — the plant investment rungs, dips on the patch's MSY. Plants stay
    ///   flow-based (they don't quantise), but the answer is the same: a fraction of a sustainable
    ///   draw does not overdraw.
    ///
    /// Exhaustive for `teaches_knowledge`'s reason: a new `FollowPolicy` must **fail to compile** here
    /// rather than inherit a plausible answer from a catch-all.
    pub fn overdraws(self) -> bool {
        match self {
            // Escapement to K/2 — the MSY point. Sustainable by construction.
            FollowPolicy::Sustain => false,
            // Fractions of that same sustainable escapement — gentler still.
            FollowPolicy::Tame
            | FollowPolicy::Corral
            | FollowPolicy::Cultivate
            | FollowPolicy::Sow => false,
            // Drawn down toward the collapse threshold, or past it.
            FollowPolicy::Surplus | FollowPolicy::Market | FollowPolicy::Eradicate => true,
        }
    }

    /// **Does working a source under this policy teach the faction anything?**
    /// (`docs/plan_intensification_ladder.md` §4.2 — *"only stewardship policies teach"*.) THE single
    /// source of that rule: [`crate::intensification::RungDef::knowledge_earned`] is its only reader,
    /// so the ladder's whole earn path turns on this one predicate.
    ///
    /// **Stewardship = restraint.** You learn husbandry by *managing* a source, not by slaughtering
    /// it — the same "restraint is the path" principle the corral arc established:
    /// - **`Sustain`** teaches. It is [`FollowPolicy::EXTRACTIVE`]'s gentlest rung — it takes only the
    ///   MSY, i.e. exactly what the source regrew — so it is the one *extractive* policy that is also
    ///   stewardship. This is what keeps rung 1 teaching (Sustain-hunt a wild herd → Herding), the
    ///   shipped behaviour §0 built.
    /// - **`Surplus` / `Market` / `Eradicate`** teach nothing. They all **overdraw** — the rest of
    ///   `EXTRACTIVE` — and running a source down is not practice.
    /// - **`Cultivate` / `Sow` / `Tame` / `Corral`** teach. The investment rungs are stewardship by
    ///   construction (they *are* the managing), and they are exactly the policies **not** in
    ///   `EXTRACTIVE`. (Whether a rung teaches *anything* is the **rung's** call — its
    ///   `earns_knowledge`, `null` on `plant:field` until rung 4 exists — so `Sow` joining this list
    ///   is a statement about the *policy*, not a promise of a lesson.)
    ///
    /// Deliberately an exhaustive `match` rather than a second const list beside `EXTRACTIVE`: a
    /// parallel list can silently drift, whereas this makes a new `FollowPolicy` **fail to compile**
    /// until someone decides whether it is stewardship. `follow_policy_teaching_matches_the_extractive_grouping`
    /// pins it against `EXTRACTIVE` so the two groupings can never disagree.
    pub fn teaches_knowledge(self) -> bool {
        match self {
            // The restrained extractive rung: takes the regrowth, leaves the stock.
            FollowPolicy::Sustain => true,
            // The overdrawing extractive rungs.
            FollowPolicy::Surplus | FollowPolicy::Market | FollowPolicy::Eradicate => false,
            // The investment rungs — managing IS the practice.
            FollowPolicy::Cultivate
            | FollowPolicy::Sow
            | FollowPolicy::Tame
            | FollowPolicy::Corral => true,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FollowPolicy::Sustain => "sustain",
            FollowPolicy::Surplus => "surplus",
            FollowPolicy::Market => "market",
            FollowPolicy::Eradicate => "eradicate",
            FollowPolicy::Cultivate => "cultivate",
            FollowPolicy::Sow => "sow",
            FollowPolicy::Tame => "tame",
            FollowPolicy::Corral => "corral",
        }
    }

    /// Does a take under this policy put food in the taker's larder? **Eradicate is denial** — it
    /// depletes the herd and carries nothing home — so every provisions-side path (the expedition's
    /// payout, its launch forecast, and the exported per-policy ceiling) reads `0` for it. THE single
    /// source of that rule, so a "turns to fill" number can never be quoted for a mission that
    /// delivers nothing.
    ///
    /// The four **investment** rungs DO deliver food: while the improvement is prepared the source
    /// is still worked, at a reduced but sustainable `yield_fraction_while_building × its MSY
    /// ceiling` (the yield dip that buys the tended patch / the field / the tamed herd / the pen).
    /// Only denial withholds food. (A `Sow` on **bare** ground has no standing crop to dip, so that
    /// fraction of nothing is honestly ~0 — a pure investment, not a withheld one.)
    pub fn delivers_food(self) -> bool {
        !matches!(self, FollowPolicy::Eradicate)
    }

    /// Policies a **Forage** assignment accepts: the four extractive rungs plus the plant branch's
    /// two investment rungs, `Cultivate` (rung 2) and `Sow` (rung 3). `Tame`/`Corral` are animal-only
    /// investments — a forage assignment carrying one is rejected at `assign_labor` (and defensively
    /// yields nothing in `forage_policy_ceiling`).
    pub fn valid_for_forage(self) -> bool {
        !matches!(self, FollowPolicy::Tame | FollowPolicy::Corral)
    }

    /// Policies a **Hunt** assignment accepts: the four extractive rungs plus the animal branch's two
    /// investment rungs, `Tame` and `Corral` ([`FollowPolicy::HUNT_POLICIES`]). `Cultivate`/`Sow` are
    /// plant-only investments — see [`FollowPolicy::valid_for_forage`]. Note this is the **band's**
    /// axis: an *expedition* accepts only [`FollowPolicy::EXTRACTIVE`] (every rung-transition is
    /// place-bound work).
    pub fn valid_for_hunt(self) -> bool {
        !matches!(self, FollowPolicy::Cultivate | FollowPolicy::Sow)
    }
}

impl FromStr for FollowPolicy {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "surplus" => Ok(FollowPolicy::Surplus),
            "market" => Ok(FollowPolicy::Market),
            "eradicate" => Ok(FollowPolicy::Eradicate),
            "cultivate" => Ok(FollowPolicy::Cultivate),
            "sow" => Ok(FollowPolicy::Sow),
            "tame" => Ok(FollowPolicy::Tame),
            "corral" => Ok(FollowPolicy::Corral),
            "sustain" | "" => Ok(FollowPolicy::Sustain),
            _ => Err(()),
        }
    }
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
            underlying_terrain: None,
            mountain: None,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The stewardship rule, pinned against the `EXTRACTIVE` grouping it is defined in terms of**
    /// (`docs/plan_intensification_ladder.md` §4.2). `teaches_knowledge` is an exhaustive match — so
    /// a new policy cannot *forget* to decide — but a match can still silently disagree with the
    /// const list beside it. This asserts the two groupings say the same thing:
    /// - **Sustain** is the one extractive rung that teaches (it takes only the regrowth).
    /// - Every **other** extractive rung overdraws, so it teaches nothing.
    /// - Every **non**-extractive policy is an investment rung — managing IS the practice — so it
    ///   teaches.
    #[test]
    fn follow_policy_teaching_matches_the_extractive_grouping() {
        for policy in FollowPolicy::EXTRACTIVE {
            let expected = matches!(policy, FollowPolicy::Sustain);
            assert_eq!(
                policy.teaches_knowledge(),
                expected,
                "{policy:?}: within EXTRACTIVE, Sustain alone is stewardship — the rest overdraw"
            );
        }
        // Every extractive rung is, by definition, NOT an investment — `is_investment` and
        // `EXTRACTIVE` are the same statement and must stay so.
        for policy in FollowPolicy::EXTRACTIVE {
            assert!(
                !policy.is_investment(),
                "{policy:?} is in EXTRACTIVE — it cannot also be an investment rung"
            );
        }
        // The investment rungs: everything HUNT_POLICIES/forage offer that isn't extractive.
        for policy in [
            FollowPolicy::Cultivate,
            FollowPolicy::Sow,
            FollowPolicy::Tame,
            FollowPolicy::Corral,
        ] {
            assert!(
                !FollowPolicy::EXTRACTIVE.contains(&policy),
                "{policy:?} is an investment rung — it must not be in EXTRACTIVE"
            );
            assert!(
                policy.is_investment(),
                "{policy:?} is a rung-transition verb — `is_investment` is what every launch gate and \
                 unreachable-arm asks, so it must say so"
            );
            // **Kind-exclusivity, pinned.** Every investment rung is place-bound work on ONE food web
            // (`Cultivate`/`Sow` prepare ground; `Tame`/`Corral` work a herd) — never both. The two
            // `valid_for_*` predicates are hand-written `!matches!` complements, so a NEW investment
            // verb would default to legal on *both* kinds; this is what catches that.
            assert_ne!(
                policy.valid_for_forage(),
                policy.valid_for_hunt(),
                "{policy:?} is an investment rung — it must be legal on exactly ONE kind"
            );
            assert!(
                policy.teaches_knowledge(),
                "{policy:?} is stewardship by construction — it must teach"
            );
        }
    }

    #[test]
    fn workers_on_counts_scout_headcount() {
        let mut allocation = LaborAllocation::default();
        // No Scout assignment → zero scouts.
        assert_eq!(allocation.workers_on(&LaborTarget::Scout), 0);

        let available = 10;
        allocation.set_assignment(LaborTarget::Scout, 3, available);
        allocation.set_assignment(LaborTarget::Warrior, 2, available);
        // Only the Scout assignment is counted (Warrior is a different singleton source).
        assert_eq!(allocation.workers_on(&LaborTarget::Scout), 3);
        assert_eq!(allocation.workers_on(&LaborTarget::Warrior), 2);
    }

    /// A Forage policy change on the **same tile** is the same source (§0-iii, parity with the Hunt
    /// arm's policy): re-assigning replaces rather than duplicating. A different tile is a different
    /// source regardless of policy.
    #[test]
    fn forage_same_source_ignores_policy_matches_tile() {
        let tile = UVec2::new(3, 4);
        let sustain = LaborTarget::Forage {
            tile,
            policy: FollowPolicy::Sustain,
        };
        let market = LaborTarget::Forage {
            tile,
            policy: FollowPolicy::Market,
        };
        let other_tile = LaborTarget::Forage {
            tile: UVec2::new(5, 6),
            policy: FollowPolicy::Sustain,
        };
        // Same tile, different policy → same source (policy is a mutable property).
        assert!(sustain.same_source(&market));
        // Different tile → different source even at the same policy.
        assert!(!sustain.same_source(&other_tile));

        // set_assignment on the same tile with a new policy replaces (no duplicate row) and updates
        // the stored policy.
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(sustain, 4, 10);
        allocation.set_assignment(market.clone(), 3, 10);
        assert_eq!(allocation.assignments.len(), 1, "policy change replaces");
        assert_eq!(allocation.assignments[0].workers, 3);
        assert_eq!(allocation.assignments[0].target, market);
    }
}
