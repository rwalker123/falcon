//! Map-section state: terrain, elevation, climate bands, rivers, and the raster carriers.

// `TileState::graze_ecology_phase` is documented in terms of the subsistence section's phase
// codes; imported so those intra-doc links keep resolving from this module.
#[allow(unused_imports)]
use crate::state::subsistence::{
    GRAZE_PHASE_COLLAPSING, GRAZE_PHASE_NONE, GRAZE_PHASE_STRESSED, GRAZE_PHASE_THRIVING,
};
use serde::{Deserialize, Serialize};
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

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
    /// A river so large it is a body of water in its own right: you need a boat to enter it.
    /// Stamped **only** by the hydrology pass, on the downstream tail of a river whose corner
    /// discharge crosses `river_class_navigable_min_discharge`. Reuses every existing water
    /// mechanic (it is `WATER | FRESHWATER`-tagged, mirroring `InlandSea`), which is exactly why
    /// it is a terrain and not a `RiverClass` — minor/major rivers are *edges* between hexes,
    /// a navigable river *is* the hex.
    NavigableRiver = 37,
}

impl TerrainType {
    pub const VALUES: [TerrainType; 38] = [
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
        TerrainType::NavigableRiver,
    ];

    /// Lowercase, human-readable adjective for the biome, reading naturally mid-sentence
    /// ("the *alluvial* ground", "the *high grassland* ground"). Written out rather than derived
    /// from the enum's debug name, which would produce copy like "AlluvialPlain ground".
    ///
    /// Consumed by The Telling's `biome.current_dominant` noun resolver (`core_sim/src/telling`).
    pub const fn as_adjective(self) -> &'static str {
        match self {
            TerrainType::DeepOcean => "deep water",
            TerrainType::ContinentalShelf => "shallow-sea",
            TerrainType::InlandSea => "lake",
            TerrainType::CoralShelf => "coral",
            TerrainType::HydrothermalVentField => "vent-field",
            TerrainType::TidalFlat => "tidal",
            TerrainType::RiverDelta => "delta",
            TerrainType::MangroveSwamp => "mangrove",
            TerrainType::FreshwaterMarsh => "marsh",
            TerrainType::Floodplain => "floodplain",
            TerrainType::AlluvialPlain => "alluvial",
            TerrainType::PrairieSteppe => "grassland",
            TerrainType::MixedWoodland => "woodland",
            TerrainType::BorealTaiga => "taiga",
            TerrainType::PeatHeath => "peat",
            TerrainType::HotDesertErg => "desert",
            TerrainType::RockyReg => "stony",
            TerrainType::SemiAridScrub => "scrub",
            TerrainType::SaltFlat => "salt-flat",
            TerrainType::OasisBasin => "oasis",
            TerrainType::Tundra => "tundra",
            TerrainType::PeriglacialSteppe => "cold-steppe",
            TerrainType::Glacier => "glacier",
            TerrainType::SeasonalSnowfield => "snowfield",
            TerrainType::RollingHills => "hill",
            TerrainType::HighPlateau => "high grassland",
            TerrainType::AlpineMountain => "mountain",
            TerrainType::KarstHighland => "karst",
            TerrainType::CanyonBadlands => "badland",
            TerrainType::ActiveVolcanoSlope => "volcano-slope",
            TerrainType::BasalticLavaField => "lava-field",
            TerrainType::AshPlain => "ash",
            TerrainType::FumaroleBasin => "fumarole",
            TerrainType::ImpactCraterField => "crater",
            TerrainType::KarstCavernMouth => "cavern",
            TerrainType::SinkholeField => "sinkhole",
            TerrainType::AquiferCeiling => "aquifer",
            TerrainType::NavigableRiver => "river",
        }
    }
}

/// The class of river running along **one side of a hex** (an odd-r hex *edge*).
///
/// Packed 2 bits per direction into `Tile::river_edges` / `TileState::river_edges`, so a tile
/// carries the class of the river on each of its six sides. This is the primitive a movement
/// system reads: "entering hex H across direction d crosses `H.river_class_on_side(d)`".
///
/// A river that outgrows `Major` does **not** get a variant here — it becomes a
/// [`TerrainType::NavigableRiver`] hex instead (a body of water you need a boat to enter), so
/// value `3` is deliberately left reserved rather than spent on "navigable".
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, PartialOrd, Ord,
)]
#[repr(u8)]
pub enum RiverClass {
    #[default]
    None = 0,
    Minor = 1,
    Major = 2,
}

impl RiverClass {
    /// Bits per direction in a packed river-edge mask.
    pub const BITS_PER_DIR: u32 = 2;
    /// Bits per **corner** in a packed river-inflow mask. A corner slot holds the same class in the
    /// same 2 bits as a direction slot — one packing layout, keyed two ways (side vs. vertex).
    pub const BITS_PER_CORNER: u32 = Self::BITS_PER_DIR;
    /// Mask of a single direction's (or corner's) slot.
    pub const SLOT_MASK: u16 = 0b11;

    pub const fn bits(self) -> u16 {
        self as u16
    }

    /// Decode a 2-bit slot. The reserved value `3` decodes to `None` (no river) rather than
    /// panicking — an unknown class must never be read as a crossable river.
    pub const fn from_bits(bits: u16) -> Self {
        match bits & Self::SLOT_MASK {
            1 => RiverClass::Minor,
            2 => RiverClass::Major,
            _ => RiverClass::None,
        }
    }

    pub const fn is_some(self) -> bool {
        !matches!(self, RiverClass::None)
    }
}

/// The bit layout of a packed **channel-exit** mask (`Tile::river_channel` /
/// `TileState::river_channel`): one bit per odd-r direction, set when a hex's *navigable* channel
/// flows out through that side.
///
/// Why it exists: a navigable river is a chain of water **hexes**, and a chain is a PATH — hex `A`
/// connects to its upstream and downstream neighbours and to nothing else. The terrain alone cannot
/// say which neighbours those are, so a renderer that arms every navigable/water neighbour draws a
/// cross-linked **web** wherever two chains run side by side or a chain bends back on itself. Only
/// the tracer knows the chain, so it states it here. Symmetric across a shared side (like
/// `river_edges`), except at the mouth, where the exit points into the open water/delta the river
/// drains into and that water carries no channel of its own.
pub struct RiverChannel;

impl RiverChannel {
    /// Bits per direction: a channel either exits through a side or it does not — there is no class
    /// here (the *water* is the river; `RiverClass` grades only edge rivers). Callers range-check
    /// `dir` against `grid_utils::HEX_DIRECTION_COUNT`, exactly as they do for `RiverClass`.
    pub const BITS_PER_DIR: u32 = 1;
    /// Mask of a single direction's slot.
    pub const SLOT_MASK: u8 = 0b1;
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
    /// Sea level on the same normalized scale as `samples` (see `snapshot.fbs`).
    #[serde(default)]
    pub sea_level: f32,
}

/// The climate-band ladder cut points, published so the client renders the band it is told
/// (`docs/plan_climate_authority.md` §8.3). A per-map constant; each is the inclusive upper
/// temperature bound of a band. The client's retired `cool_min` equals `boreal_max_temp` (§5.2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct ClimateBandsState {
    #[serde(default)]
    pub polar_max_temp: f32,
    #[serde(default)]
    pub boreal_max_temp: f32,
    #[serde(default)]
    pub temperate_max_temp: f32,
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
    /// Tile-intrinsic per-turn morale drain (fixed-point raw, `Scalar::SCALE` = 1.0; `>= 0`,
    /// bigger = harsher). Band-independent — a property of the place. Derived at capture.
    #[serde(default)]
    pub habitability: i64,
    /// Packed per-side river classes: `class = RiverClass::from_bits(river_edges >> (2 * dir))`
    /// for each odd-r direction `dir` (0=E, 1=SE, 2=SW, 3=W, 4=NW, 5=NE). Both hexes flanking a
    /// river edge carry it, each on their own side. Replaces the old polyline hydrology overlay:
    /// together with `TerrainType::NavigableRiver` this fully determines the river render.
    #[serde(default)]
    pub river_edges: u16,
    /// Packed per-**corner** river inflow: `class = RiverClass::from_bits(river_inflow >> (2 *
    /// corner))` for each hex corner (`0` lower-right, `1` bottom, `2` lower-left, `3` upper-left,
    /// `4` top, `5` upper-right — screen space, +y down).
    ///
    /// Set only on the **first hex of a `NavigableRiver` chain**, at the corner where the edge-river
    /// chain terminates and hands its water to the navigable trunk, with the class of the last edge
    /// it emitted. An edge river ends at a *vertex*, never mid-side, so this is where the renderer
    /// must join the tributary to the trunk hex. `0` everywhere else.
    #[serde(default)]
    pub river_inflow: u16,
    /// Packed per-side **channel exits** of a navigable river — 1 bit per odd-r direction (see
    /// [`RiverChannel`]): `exits(dir) = (river_channel >> dir) & 1`.
    ///
    /// The trunk channel is a **path**, and only the tracer knows which neighbours a navigable hex
    /// actually links to; a renderer that infers them from terrain draws a web. Arm only the sides
    /// whose bit is set. Symmetric across a shared side, except at the mouth (the exit into the
    /// ocean/inland sea/delta is not mirrored back). `0` on every hex with no navigable channel.
    #[serde(default)]
    pub river_channel: u8,
    /// **Graze (pasture) readout** — the tile's live *animal-edible* biomass (grass/browse), the stock
    /// herds eat. `0` on water/ice/rock and on any tile with no pasture. Distinct from the
    /// *human-edible* forage stock (`ForagePatchState`, food-module tiles only) — see
    /// `docs/plan_grazing_foundation.md`. Derived at capture from the `GrazeRegistry`.
    #[serde(default)]
    pub graze_biomass: f32,
    /// The tile's graze **capacity** — a property of the *land* (its biome), not of any animal. `0`
    /// means the biome carries no pasture at all; `graze_biomass / graze_capacity` is the pasture's
    /// health (and, from Phase 2b, the overgrazing signal).
    #[serde(default)]
    pub graze_capacity: f32,
    /// The tile's pasture phase, as [`GRAZE_PHASE_NONE`] / [`GRAZE_PHASE_THRIVING`] /
    /// [`GRAZE_PHASE_STRESSED`] / [`GRAZE_PHASE_COLLAPSING`]. A compact code rather than the string
    /// the sparse herd/forage payloads use, because this rides *every* tile (the `moraleCause:ubyte`
    /// idiom). `NONE` is the default, so "this biome has no pasture" is never confused with "this
    /// pasture is healthy".
    #[serde(default)]
    pub graze_ecology_phase: u8,
    /// **Forage potential** — the *human-edible* twin of [`graze_capacity`](Self::graze_capacity).
    /// The land's per-biome human-food capacity (`forage.capacity_by_biome`, `labor_config.json`),
    /// read from the config table for *every* tile — **not** from the sparse `ForagePatch`, which
    /// exists only on food-module tiles. That is the point: the client draws a Forage overlay of the
    /// biome's *potential* everywhere (the mirror of the pasture overlay), including the ~95% of tiles
    /// that carry no patch. Unlike graze this is **non-zero on fishery water** (`ContinentalShelf` /
    /// `CoralShelf` / `InlandSea`) — a fishery is a food module on water. Only a *stated-zero* biome
    /// (deep ocean, glacier, lava, salt flat) reads `0`. Derived at capture from
    /// `forage::tile_forage_capacity`, which keys off `resource_terrain()` (the underlying valley
    /// biome on a navigable hex, the tile's own terrain elsewhere); a `NavigableRiver` hex additionally
    /// earns the navigable fishing bonus — see `docs/plan_grazing_foundation.md` §1.1.
    #[serde(default)]
    pub forage_capacity: f32,
    /// The tile's **real ground** for resource reads. Equals `terrain` on every ordinary tile; on a
    /// `NavigableRiver` hex it is the biome the channel was cut through (the valley it yields, not
    /// open water). The client consults this **only** when `terrain == NavigableRiver` — elsewhere it
    /// is identical to `terrain` — so it is always meaningful even read unconditionally. Written from
    /// `Tile::resource_terrain()`.
    #[serde(default)]
    pub underlying_terrain: TerrainType,
}
