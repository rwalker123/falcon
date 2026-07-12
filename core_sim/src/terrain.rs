use bevy::prelude::UVec2;
use sim_runtime::{TerrainTags, TerrainType};

use crate::{map_preset::TerrainClassifierConfig, mapgen::MountainType};

#[derive(Debug, Clone, Copy)]
pub struct MovementProfile {
    pub foot: f32,
    pub pack: f32,
    pub mechanized: f32,
    pub naval: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainResourceBias {
    pub ore: i8,
    pub organics: i8,
    pub energy: i8,
}

/// The per-biome climate/relief partition the per-map biome palette thins within and
/// guarantees coverage across (see `docs/plan_biome_palette.md` §3.1). Every
/// [`TerrainType`] belongs to exactly one niche — an intrinsic biome property, unlike
/// [`TerrainTags`] which overlap. The palette samples up to `K` biomes per niche and
/// remaps off-palette biomes to an allowed member of the *same* niche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BiomeNiche {
    /// Open water + coast.
    Ocean,
    /// Water-adjacent lowland (deltas, marshes, mangroves).
    CoastWetland,
    /// The fertile lowland spine + the solver's universal fallback.
    FertileLowland,
    /// Dry lowland (deserts, scrub, salt/ash plains).
    AridLowland,
    /// High-latitude lowland (tundra, snowfield, glacier).
    PolarLowland,
    /// Relief-driven upland from the mountain mask.
    Highland,
    /// Volcanic mask / anomaly-slice volcanic biomes.
    Volcanic,
    /// Subsurface / hazard "discovery" flavor.
    Anomaly,
}

impl BiomeNiche {
    /// Every niche, in a stable order (used to iterate the palette build).
    pub const ALL: [BiomeNiche; 8] = [
        BiomeNiche::Ocean,
        BiomeNiche::CoastWetland,
        BiomeNiche::FertileLowland,
        BiomeNiche::AridLowland,
        BiomeNiche::PolarLowland,
        BiomeNiche::Highland,
        BiomeNiche::Volcanic,
        BiomeNiche::Anomaly,
    ];

    /// Config/serialization key — matches the `niches` map keys in `map_presets.json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            BiomeNiche::Ocean => "Ocean",
            BiomeNiche::CoastWetland => "CoastWetland",
            BiomeNiche::FertileLowland => "FertileLowland",
            BiomeNiche::AridLowland => "AridLowland",
            BiomeNiche::PolarLowland => "PolarLowland",
            BiomeNiche::Highland => "Highland",
            BiomeNiche::Volcanic => "Volcanic",
            BiomeNiche::Anomaly => "Anomaly",
        }
    }
}

/// The niche a biome belongs to (`docs/plan_biome_palette.md` §3.1). Total over all 37
/// biomes so the partition is exhaustive and disjoint.
pub const fn biome_niche(terrain: TerrainType) -> BiomeNiche {
    use TerrainType::*;
    match terrain {
        DeepOcean | ContinentalShelf | InlandSea | CoralShelf | HydrothermalVentField => {
            BiomeNiche::Ocean
        }
        RiverDelta | TidalFlat | MangroveSwamp | FreshwaterMarsh | PeatHeath => {
            BiomeNiche::CoastWetland
        }
        AlluvialPlain | PrairieSteppe | Floodplain | MixedWoodland => BiomeNiche::FertileLowland,
        HotDesertErg | RockyReg | SemiAridScrub | SaltFlat | OasisBasin | AshPlain => {
            BiomeNiche::AridLowland
        }
        // Deviation from `docs/plan_biome_palette.md` §3.1 (which lists BorealTaiga under
        // FertileLowland): BorealTaiga is POLAR-tagged, so grouping it with the temperate
        // fertile spine let an off-palette boreal tile remap to AlluvialPlain and stamp
        // temperate plains at the poles (violating the polar-no-alluvial contract — see
        // `polar_latitudes_avoid_alluvial_plain_regression`). Homing it in PolarLowland
        // keeps every POLAR-tagged biome in the polar niche, so it remaps to Tundra and
        // stays cold-climate. Honors the doc's own hard constraint (§1: never make a tile
        // "look wrong") over its illustrative table.
        Tundra | PeriglacialSteppe | SeasonalSnowfield | Glacier | BorealTaiga => {
            BiomeNiche::PolarLowland
        }
        RollingHills | HighPlateau | AlpineMountain | KarstHighland | CanyonBadlands => {
            BiomeNiche::Highland
        }
        ActiveVolcanoSlope | FumaroleBasin | BasalticLavaField => BiomeNiche::Volcanic,
        ImpactCraterField | SinkholeField | KarstCavernMouth | AquiferCeiling => {
            BiomeNiche::Anomaly
        }
    }
}

/// Whether a biome must remain reachable on *every* map regardless of palette/seed
/// (`docs/plan_biome_palette.md` §3.2). These anchor their niche or serve as a
/// successional/solver fallback: they are always in the palette and count toward — but
/// are never dropped from — their niche's chosen set. `InlandSea` is must-have so that
/// lakes/inland seas always survive the palette remap: without it an off-palette InlandSea
/// tile falls to the first allowed Ocean-niche member (`DeepOcean`) and renders inland
/// water as ocean.
///
/// `Glacier` is must-have for the same surgical reason as `InlandSea`: it is the polar
/// analog of `AlpineMountain`, placed only on polar tiles whose relief clears
/// `alpine_relief_threshold` (`select_mountain_terrain`). Protecting just this one
/// extreme-relief member keeps a tall polar peak as ice rather than remapping it down to
/// flat `Tundra`, without un-thinning the whole (interchangeable) flat PolarLowland set.
/// This mirrors the physically-gated-vs-interchangeable principle: `must_have` is reserved
/// for a single physically-gated member inside an otherwise-thinnable niche (`InlandSea` in
/// Ocean, `Glacier` in PolarLowland). The fully physically-gated niches (Highland, Volcanic)
/// are instead kept intact via their palette `K` (full membership), not via `must_have`.
pub const fn biome_must_have(terrain: TerrainType) -> bool {
    use TerrainType::*;
    matches!(
        terrain,
        DeepOcean
            | ContinentalShelf
            | InlandSea
            | AlluvialPlain
            | PrairieSteppe
            | Tundra
            | RiverDelta
            | Glacier
    )
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainDefinition {
    pub terrain: TerrainType,
    pub tags: TerrainTags,
    pub movement: MovementProfile,
    pub logistics_penalty: f32,
    pub attrition_rate: f32,
    pub detection_modifier: f32,
    pub infrastructure_cost: f32,
    pub resource_bias: TerrainResourceBias,
    /// Palette partition (`docs/plan_biome_palette.md` §3.1).
    pub niche: BiomeNiche,
    /// Always-in-palette anchor flag (`docs/plan_biome_palette.md` §3.2).
    pub must_have: bool,
}

fn mp(foot: f32, pack: f32, mechanized: f32, naval: f32) -> MovementProfile {
    MovementProfile {
        foot,
        pack,
        mechanized,
        naval,
    }
}

fn rb(ore: i8, organics: i8, energy: i8) -> TerrainResourceBias {
    TerrainResourceBias {
        ore,
        organics,
        energy,
    }
}

#[allow(clippy::too_many_arguments)] // Terrain definitions require explicit parameterization
fn def(
    terrain: TerrainType,
    tags: TerrainTags,
    movement: MovementProfile,
    logistics_penalty: f32,
    attrition_rate: f32,
    detection_modifier: f32,
    infrastructure_cost: f32,
    resource_bias: TerrainResourceBias,
) -> TerrainDefinition {
    TerrainDefinition {
        terrain,
        tags,
        movement,
        logistics_penalty,
        attrition_rate,
        detection_modifier,
        infrastructure_cost,
        resource_bias,
        // Intrinsic palette properties, derived from the biome itself so the 37 call
        // sites need no extra arguments (`docs/plan_biome_palette.md` §4.1).
        niche: biome_niche(terrain),
        must_have: biome_must_have(terrain),
    }
}

pub fn terrain_definition(terrain: TerrainType) -> TerrainDefinition {
    use TerrainTags as Tag;

    match terrain {
        TerrainType::DeepOcean => def(
            terrain,
            Tag::WATER | Tag::HAZARDOUS,
            mp(4.5, 4.5, 4.8, 0.7),
            1.7,
            0.40,
            -0.20,
            3.2,
            rb(3, 1, 5),
        ),
        TerrainType::ContinentalShelf => def(
            terrain,
            Tag::WATER | Tag::COASTAL,
            mp(4.0, 3.8, 4.2, 0.8),
            1.5,
            0.32,
            -0.10,
            2.4,
            rb(2, 2, 4),
        ),
        TerrainType::InlandSea => def(
            terrain,
            Tag::WATER | Tag::FRESHWATER,
            mp(3.5, 3.3, 3.6, 0.8),
            1.3,
            0.28,
            -0.05,
            2.0,
            rb(1, 3, 3),
        ),
        TerrainType::CoralShelf => def(
            terrain,
            Tag::WATER | Tag::COASTAL | Tag::FERTILE,
            mp(3.6, 3.4, 3.7, 0.9),
            1.4,
            0.30,
            -0.15,
            2.2,
            rb(2, 3, 3),
        ),
        TerrainType::HydrothermalVentField => def(
            terrain,
            Tag::WATER | Tag::HAZARDOUS | Tag::HYDROTHERMAL,
            mp(4.6, 4.5, 4.9, 0.7),
            1.9,
            0.45,
            -0.25,
            3.4,
            rb(4, 1, 5),
        ),
        TerrainType::TidalFlat => def(
            terrain,
            Tag::COASTAL | Tag::WETLAND,
            mp(1.5, 1.3, 2.0, 1.1),
            1.3,
            0.18,
            0.00,
            1.4,
            rb(0, 3, 2),
        ),
        TerrainType::RiverDelta => def(
            terrain,
            Tag::COASTAL | Tag::WETLAND | Tag::FERTILE | Tag::FRESHWATER,
            mp(1.4, 1.2, 1.8, 1.0),
            1.2,
            0.12,
            0.05,
            1.2,
            rb(1, 4, 2),
        ),
        TerrainType::MangroveSwamp => def(
            terrain,
            Tag::COASTAL | Tag::WETLAND | Tag::FERTILE,
            mp(1.7, 1.5, 2.4, 1.2),
            1.35,
            0.22,
            -0.20,
            1.5,
            rb(0, 4, 1),
        ),
        TerrainType::FreshwaterMarsh => def(
            terrain,
            Tag::FRESHWATER | Tag::WETLAND,
            mp(1.6, 1.4, 2.3, 1.3),
            1.3,
            0.20,
            -0.15,
            1.4,
            rb(0, 4, 1),
        ),
        TerrainType::Floodplain => def(
            terrain,
            Tag::FERTILE | Tag::FRESHWATER,
            mp(1.1, 1.0, 1.4, 1.5),
            1.05,
            0.08,
            0.05,
            1.1,
            rb(0, 4, 2),
        ),
        TerrainType::AlluvialPlain => def(
            terrain,
            Tag::FERTILE,
            mp(1.0, 0.9, 1.1, 2.0),
            0.95,
            0.05,
            0.15,
            0.9,
            rb(0, 3, 1),
        ),
        TerrainType::PrairieSteppe => def(
            terrain,
            Tag::FERTILE,
            mp(1.05, 0.95, 1.0, 2.0),
            0.98,
            0.06,
            0.20,
            0.95,
            rb(1, 3, 1),
        ),
        TerrainType::MixedWoodland => def(
            terrain,
            Tag::FERTILE,
            mp(1.2, 1.05, 1.35, 2.2),
            1.00,
            0.10,
            -0.15,
            1.05,
            rb(1, 3, 1),
        ),
        TerrainType::BorealTaiga => def(
            terrain,
            Tag::FERTILE | Tag::POLAR,
            mp(1.35, 1.20, 1.50, 2.3),
            1.15,
            0.16,
            -0.20,
            1.20,
            rb(1, 2, 1),
        ),
        TerrainType::PeatHeath => def(
            terrain,
            Tag::WETLAND | Tag::FERTILE,
            mp(1.4, 1.2, 1.6, 2.0),
            1.20,
            0.14,
            -0.10,
            1.30,
            rb(-1, 3, 1),
        ),
        TerrainType::HotDesertErg => def(
            terrain,
            Tag::ARID,
            mp(1.4, 1.15, 1.5, 2.4),
            1.25,
            0.30,
            0.10,
            1.25,
            rb(2, -1, 3),
        ),
        TerrainType::RockyReg => def(
            terrain,
            Tag::ARID,
            mp(1.3, 1.1, 1.4, 2.3),
            1.20,
            0.26,
            0.05,
            1.20,
            rb(3, -1, 2),
        ),
        TerrainType::SemiAridScrub => def(
            terrain,
            Tag::ARID,
            mp(1.25, 1.05, 1.35, 2.2),
            1.15,
            0.20,
            0.00,
            1.10,
            rb(1, 1, 2),
        ),
        TerrainType::SaltFlat => def(
            terrain,
            Tag::ARID | Tag::HAZARDOUS,
            mp(1.6, 1.3, 1.8, 2.6),
            1.35,
            0.32,
            0.05,
            1.35,
            rb(2, -2, 3),
        ),
        TerrainType::OasisBasin => def(
            terrain,
            Tag::ARID | Tag::FRESHWATER | Tag::FERTILE,
            mp(1.2, 1.0, 1.3, 2.0),
            1.05,
            0.12,
            0.00,
            1.10,
            rb(0, 3, 2),
        ),
        TerrainType::Tundra => def(
            terrain,
            Tag::POLAR,
            mp(1.45, 1.25, 1.8, 2.4),
            1.30,
            0.24,
            -0.05,
            1.35,
            rb(1, -1, 2),
        ),
        TerrainType::PeriglacialSteppe => def(
            terrain,
            Tag::POLAR | Tag::FERTILE,
            mp(1.35, 1.2, 1.6, 2.3),
            1.25,
            0.20,
            0.00,
            1.30,
            rb(1, 1, 2),
        ),
        TerrainType::Glacier => def(
            terrain,
            Tag::POLAR | Tag::HAZARDOUS,
            mp(1.9, 1.7, 2.4, 2.8),
            1.50,
            0.40,
            -0.20,
            1.60,
            rb(1, -3, 2),
        ),
        TerrainType::SeasonalSnowfield => def(
            terrain,
            Tag::POLAR,
            mp(1.6, 1.4, 2.1, 2.5),
            1.35,
            0.28,
            -0.10,
            1.45,
            rb(0, -2, 2),
        ),
        TerrainType::RollingHills => def(
            terrain,
            Tag::HIGHLAND | Tag::FERTILE,
            mp(1.2, 1.05, 1.45, 2.3),
            1.12,
            0.14,
            0.10,
            1.15,
            rb(1, 2, 1),
        ),
        TerrainType::HighPlateau => def(
            terrain,
            Tag::HIGHLAND,
            mp(1.4, 1.2, 1.8, 2.5),
            1.25,
            0.20,
            0.00,
            1.25,
            rb(2, 1, 2),
        ),
        TerrainType::AlpineMountain => def(
            terrain,
            Tag::HIGHLAND | Tag::HAZARDOUS,
            mp(1.8, 1.6, 2.4, 3.0),
            1.45,
            0.36,
            -0.25,
            1.60,
            rb(3, 0, 2),
        ),
        TerrainType::KarstHighland => def(
            terrain,
            Tag::HIGHLAND | Tag::SUBSURFACE,
            mp(1.6, 1.4, 2.2, 2.8),
            1.40,
            0.28,
            -0.10,
            1.45,
            rb(2, 0, 2),
        ),
        TerrainType::CanyonBadlands => def(
            terrain,
            Tag::HIGHLAND | Tag::ARID,
            mp(1.7, 1.4, 2.2, 2.9),
            1.42,
            0.30,
            -0.05,
            1.50,
            rb(3, -1, 2),
        ),
        TerrainType::ActiveVolcanoSlope => def(
            terrain,
            Tag::HIGHLAND | Tag::VOLCANIC | Tag::HAZARDOUS,
            mp(2.0, 1.8, 2.6, 3.2),
            1.60,
            0.42,
            -0.20,
            1.70,
            rb(4, -2, 5),
        ),
        TerrainType::BasalticLavaField => def(
            terrain,
            Tag::VOLCANIC | Tag::HAZARDOUS,
            mp(1.9, 1.7, 2.5, 3.1),
            1.55,
            0.38,
            -0.20,
            1.65,
            rb(4, -2, 4),
        ),
        TerrainType::AshPlain => def(
            terrain,
            Tag::VOLCANIC,
            mp(1.6, 1.4, 2.0, 2.7),
            1.40,
            0.30,
            -0.05,
            1.45,
            rb(2, -1, 3),
        ),
        TerrainType::FumaroleBasin => def(
            terrain,
            Tag::VOLCANIC | Tag::HYDROTHERMAL | Tag::HAZARDOUS,
            mp(1.9, 1.7, 2.4, 2.9),
            1.60,
            0.40,
            -0.15,
            1.70,
            rb(3, -1, 5),
        ),
        TerrainType::ImpactCraterField => def(
            terrain,
            Tag::HAZARDOUS,
            mp(1.8, 1.6, 2.3, 3.0),
            1.50,
            0.35,
            -0.10,
            1.60,
            rb(3, -1, 3),
        ),
        TerrainType::KarstCavernMouth => def(
            terrain,
            Tag::SUBSURFACE | Tag::FERTILE,
            mp(1.7, 1.5, 2.2, 2.6),
            1.45,
            0.30,
            -0.15,
            1.55,
            rb(2, 1, 2),
        ),
        TerrainType::SinkholeField => def(
            terrain,
            Tag::SUBSURFACE | Tag::HAZARDOUS,
            mp(1.9, 1.7, 2.5, 2.9),
            1.55,
            0.38,
            -0.20,
            1.60,
            rb(1, 0, 2),
        ),
        TerrainType::AquiferCeiling => def(
            terrain,
            Tag::SUBSURFACE | Tag::FRESHWATER,
            mp(1.8, 1.6, 2.3, 2.8),
            1.50,
            0.32,
            -0.10,
            1.55,
            rb(1, 2, 2),
        ),
    }
}

/// Anomaly ("discovery" biome) hashing. The **rarity roll** — whether an eligible lowland
/// tile becomes an anomaly at all — reads a fresh 8-bit field (bits 16-23), disjoint from
/// the humidity field (bits 8-15), and is compared against `classifier.anomaly_fraction`.
/// A separate 4-bit field (bits 4-7) then picks *which* of the six anomaly biomes, so all
/// six stay reachable independent of the (low) rarity gate.
const ANOMALY_RARITY_SHIFT: u32 = 16;
const ANOMALY_RARITY_MASK: u32 = 0xFF;
/// Denominator for the 8-bit rarity roll → a fraction in `[0, 1)`.
const ANOMALY_RARITY_SCALE: f32 = 256.0;
const ANOMALY_SELECT_SHIFT: u32 = 4;
const ANOMALY_SELECT_MASK: u32 = 0x0F;
/// Number of distinct anomaly biomes the selection field cycles across.
const ANOMALY_BIOME_COUNT: u32 = 6;

pub fn classify_terrain(
    position: UVec2,
    grid_size: UVec2,
    classifier: &TerrainClassifierConfig,
) -> TerrainType {
    let width = grid_size.x.max(1) as f32;
    let height = grid_size.y.max(1) as f32;
    let fx = position.x as f32 / width;
    let fy = position.y as f32 / height;
    let edge = fx.min(1.0 - fx).min(fy).min(1.0 - fy);
    let dist_from_equator = (fy - 0.5).abs();
    let is_high_latitude = dist_from_equator >= classifier.high_latitude_threshold;
    let noise = tile_noise(position);
    let humidity = ((noise >> 8) & 0xFF) as f32 / 255.0;

    if edge < classifier.coastal_deep_ocean_edge {
        return pick(
            noise,
            &[TerrainType::DeepOcean, TerrainType::HydrothermalVentField],
        );
    }
    if edge < classifier.coastal_shelf_edge {
        return pick(
            noise,
            &[
                TerrainType::ContinentalShelf,
                TerrainType::CoralShelf,
                TerrainType::TidalFlat,
                TerrainType::MangroveSwamp,
            ],
        );
    }
    if edge < classifier.coastal_inland_edge {
        // NOTE: RiverDelta is intentionally NOT a candidate here. Deltas are a
        // river-mouth feature and are stamped exclusively by the hydrology pass
        // (see `generate_hydrology` in hydrology.rs). Picking them by noise here
        // scattered deltas across the coast with no relation to actual rivers.
        return pick(
            noise,
            &[TerrainType::InlandSea, TerrainType::FreshwaterMarsh],
        );
    }

    if dist_from_equator >= classifier.polar_latitude_cutoff {
        return pick(
            noise,
            &[
                TerrainType::Tundra,
                TerrainType::PeriglacialSteppe,
                TerrainType::SeasonalSnowfield,
            ],
        );
    }

    // Rare "discovery" biomes. Only a low `anomaly_fraction` of eligible lowland tiles pass
    // the rarity roll; those that do split evenly across the six anomaly biomes (incl. the
    // revived AquiferCeiling, `docs/plan_biome_palette.md` §3.6). Total anomaly coverage is
    // ≈ `anomaly_fraction` of eligible lowland — genuinely rare, not the old ~37% blanket.
    let anomaly_roll = (noise >> ANOMALY_RARITY_SHIFT) & ANOMALY_RARITY_MASK;
    if (anomaly_roll as f32) / ANOMALY_RARITY_SCALE < classifier.anomaly_fraction {
        let which = ((noise >> ANOMALY_SELECT_SHIFT) & ANOMALY_SELECT_MASK) % ANOMALY_BIOME_COUNT;
        return match which {
            0 => TerrainType::ImpactCraterField,
            1 => TerrainType::SinkholeField,
            2 => TerrainType::KarstCavernMouth,
            3 => TerrainType::FumaroleBasin,
            4 => TerrainType::ActiveVolcanoSlope,
            // 5 → AquiferCeiling, the revived subsurface "discovery" biome (§3.6). The
            // `_` also catches the (structurally-unreachable) tail of the modulo.
            _ => TerrainType::AquiferCeiling,
        };
    }

    // NOTE: highland/mountain biomes (AlpineMountain, HighPlateau, CanyonBadlands,
    // RollingHills, KarstHighland, ...) are intentionally NOT picked here. The base
    // classifier has no access to the real ElevationField, so it used to invent a
    // per-tile "elevation" from a tile hash and stamp mountains on flat lowland tiles —
    // decoupling the biome from actual height (mask-less "Alpine Mountains" at Height 0).
    // Those biomes now come exclusively from the tectonic mountain mask
    // (`select_mountain_terrain`) and the real-elevation branches in
    // `terrain_for_position_with_classifier`, so they always sit on genuinely high
    // ground. The base classifier only assigns climate/humidity lowland biomes.
    if humidity > 0.7 && !is_high_latitude {
        return pick(
            noise,
            &[
                TerrainType::Floodplain,
                TerrainType::AlluvialPlain,
                TerrainType::PeatHeath,
            ],
        );
    }

    if humidity > 0.5 && !is_high_latitude {
        return pick(
            noise,
            &[
                TerrainType::AlluvialPlain,
                TerrainType::PrairieSteppe,
                TerrainType::MixedWoodland,
            ],
        );
    }

    if humidity < 0.2 {
        return pick(
            noise,
            &[
                TerrainType::HotDesertErg,
                TerrainType::RockyReg,
                TerrainType::SaltFlat,
                TerrainType::OasisBasin,
                TerrainType::AshPlain,
            ],
        );
    }

    if humidity < 0.35 {
        return pick(
            noise,
            &[
                TerrainType::SemiAridScrub,
                TerrainType::PrairieSteppe,
                TerrainType::RollingHills,
            ],
        );
    }

    if is_high_latitude {
        return pick(
            noise,
            &[
                TerrainType::BorealTaiga,
                TerrainType::MixedWoodland,
                TerrainType::PeatHeath,
            ],
        );
    }

    pick(
        noise,
        &[
            TerrainType::PrairieSteppe,
            TerrainType::AlluvialPlain,
            TerrainType::MixedWoodland,
        ],
    )
}

fn select_mountain_terrain(
    kind: MountainType,
    relief: f32,
    moisture: f32,
    alpine_relief_threshold: f32,
    basaltic_relief_threshold: f32,
) -> TerrainType {
    use sim_runtime::TerrainType::*;
    let relief = relief.clamp(0.5, 3.0);
    let moisture = moisture.clamp(0.0, 1.0);
    match kind {
        MountainType::Fold => {
            if relief >= alpine_relief_threshold {
                AlpineMountain
            } else if moisture >= 0.6 {
                RollingHills
            } else if moisture >= 0.4 {
                HighPlateau
            } else {
                CanyonBadlands
            }
        }
        MountainType::Fault => {
            if moisture >= 0.5 {
                KarstHighland
            } else {
                CanyonBadlands
            }
        }
        // Revived flavor biome (`docs/plan_biome_palette.md` §3.6): a cooled-flow
        // BasalticLavaField on lower-relief volcanic tiles beside the towering
        // ActiveVolcanoSlope cores, so the volcanic mask now emits both. Volcanic
        // tiles are already rare, so the rate stays flavor-low.
        MountainType::Volcanic => {
            if relief >= basaltic_relief_threshold {
                ActiveVolcanoSlope
            } else {
                BasalticLavaField
            }
        }
        MountainType::Dome => {
            if moisture >= 0.55 {
                HighPlateau
            } else {
                RollingHills
            }
        }
    }
}

pub fn terrain_for_position_with_classifier(
    position: UVec2,
    grid_size: UVec2,
    moisture: Option<f32>,
    elevation: Option<f32>,
    mountain: Option<(MountainType, f32)>,
    classifier: &TerrainClassifierConfig,
) -> (TerrainType, TerrainTags) {
    let mut terrain = classify_terrain(position, grid_size, classifier);
    let mut definition = terrain_definition(terrain);
    let mut tags = definition.tags;
    let moisture = moisture.unwrap_or(0.5);
    let lat_denom = grid_size.y.saturating_sub(1).max(1) as f32;
    let lat = position.y as f32 / lat_denom;
    let dist_from_equator = (lat - 0.5).abs();
    let is_polar_lat = dist_from_equator >= classifier.polar_latitude_cutoff;

    if let Some((kind, relief)) = mountain {
        if is_polar_lat {
            // Revived flavor biome (`docs/plan_biome_palette.md` §3.6): the coldest,
            // highest polar tiles (relief clearing the Alpine threshold) ice over into
            // a Glacier; lower polar relief stays a wind-scoured SeasonalSnowfield.
            terrain = if relief >= classifier.alpine_relief_threshold {
                sim_runtime::TerrainType::Glacier
            } else {
                sim_runtime::TerrainType::SeasonalSnowfield
            };
            definition = terrain_definition(terrain);
            tags = definition.tags | TerrainTags::HIGHLAND;
        } else {
            terrain = select_mountain_terrain(
                kind,
                relief,
                moisture,
                classifier.alpine_relief_threshold,
                classifier.basaltic_relief_threshold,
            );
            definition = terrain_definition(terrain);
            tags = definition.tags;
        }
    } else if let Some(elev) = elevation {
        if !is_polar_lat
            && elev >= classifier.high_dry_elevation
            && moisture < classifier.high_dry_moisture
        {
            terrain = sim_runtime::TerrainType::CanyonBadlands;
            definition = terrain_definition(terrain);
            tags = definition.tags;
        } else if elev >= classifier.high_wet_elevation
            && moisture >= 0.6
            && !definition.tags.contains(TerrainTags::HIGHLAND)
        {
            terrain = sim_runtime::TerrainType::RollingHills;
            definition = terrain_definition(terrain);
            tags = definition.tags;
        }
    }

    (terrain, tags)
}

pub fn terrain_for_position_with_context(
    position: UVec2,
    grid_size: UVec2,
    moisture: Option<f32>,
    elevation: Option<f32>,
    mountain: Option<(MountainType, f32)>,
) -> (TerrainType, TerrainTags) {
    terrain_for_position_with_classifier(
        position,
        grid_size,
        moisture,
        elevation,
        mountain,
        &TerrainClassifierConfig::default(),
    )
}

pub fn terrain_for_position(position: UVec2, grid_size: UVec2) -> (TerrainType, TerrainTags) {
    terrain_for_position_with_context(position, grid_size, None, None, None)
}

fn tile_noise(position: UVec2) -> u32 {
    let mut n = position.x;
    n = n.wrapping_mul(0x6C8E_9CF5) ^ position.y.wrapping_mul(0xB529_7A4D);
    n ^= n >> 13;
    n = n.wrapping_mul(0x68E3_1DA4);
    n ^= n >> 11;
    n = n.wrapping_mul(0x1B56_C4E9);
    n ^ (n >> 16)
}

fn pick(noise: u32, options: &[TerrainType]) -> TerrainType {
    if options.is_empty() {
        return TerrainType::AlluvialPlain;
    }
    let idx = (noise as usize) % options.len();
    options[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_mountains_become_alpine_highland() {
        let position = UVec2::new(8, 8);
        let grid = UVec2::new(32, 32);
        let (terrain, tags) = terrain_for_position_with_context(
            position,
            grid,
            Some(0.55),
            Some(0.9),
            Some((MountainType::Fold, 1.6)),
        );
        assert_eq!(terrain, TerrainType::AlpineMountain);
        assert!(tags.contains(TerrainTags::HIGHLAND));
    }

    #[test]
    fn dome_mountains_respect_moisture_bias() {
        let position = UVec2::new(4, 12);
        let grid = UVec2::new(32, 32);
        let (wet_terrain, wet_tags) = terrain_for_position_with_context(
            position,
            grid,
            Some(0.7),
            Some(0.75),
            Some((MountainType::Dome, 1.1)),
        );
        assert_eq!(wet_terrain, TerrainType::HighPlateau);
        assert!(wet_tags.contains(TerrainTags::HIGHLAND));

        let (dry_terrain, _) = terrain_for_position_with_context(
            position,
            grid,
            Some(0.25),
            Some(0.72),
            Some((MountainType::Dome, 1.1)),
        );
        assert_eq!(dry_terrain, TerrainType::RollingHills);
    }

    #[test]
    fn every_terrain_type_maps_to_a_niche_and_partition_is_exhaustive() {
        // Each niche's membership, unioned, must be exactly the 37 biomes, disjoint.
        let mut seen = std::collections::HashSet::new();
        for niche in BiomeNiche::ALL {
            for terrain in TerrainType::VALUES {
                if biome_niche(terrain) == niche {
                    assert!(seen.insert(terrain), "{terrain:?} appears in two niches");
                }
            }
        }
        assert_eq!(seen.len(), TerrainType::VALUES.len());
        // Every niche has at least one member (no empty niche).
        for niche in BiomeNiche::ALL {
            assert!(
                TerrainType::VALUES.iter().any(|&t| biome_niche(t) == niche),
                "niche {niche:?} has no members"
            );
        }
    }

    #[test]
    fn must_have_set_is_exactly_the_documented_eight() {
        let mut must: Vec<TerrainType> = TerrainType::VALUES
            .into_iter()
            .filter(|&t| biome_must_have(t))
            .collect();
        must.sort_by_key(|t| *t as u16);
        assert_eq!(
            must,
            vec![
                TerrainType::DeepOcean,
                TerrainType::ContinentalShelf,
                TerrainType::InlandSea,
                TerrainType::RiverDelta,
                TerrainType::AlluvialPlain,
                TerrainType::PrairieSteppe,
                TerrainType::Tundra,
                TerrainType::Glacier,
            ]
        );
    }

    /// The six rare "discovery" biomes the anomaly gate cycles across (incl. the revived
    /// AquiferCeiling, §3.6).
    const ANOMALY_BIOMES: [TerrainType; 6] = [
        TerrainType::ImpactCraterField,
        TerrainType::SinkholeField,
        TerrainType::KarstCavernMouth,
        TerrainType::FumaroleBasin,
        TerrainType::ActiveVolcanoSlope,
        TerrainType::AquiferCeiling,
    ];

    #[test]
    fn every_anomaly_biome_including_aquifer_is_still_reachable() {
        // Under the rarer config-driven gate, scan a large eligible interior and assert all
        // six anomaly biomes — especially the revived AquiferCeiling (§3.6) — still appear.
        let grid = UVec2::new(96, 96);
        let classifier = TerrainClassifierConfig::default();
        let mut seen = std::collections::HashSet::new();
        for y in 20..76 {
            for x in 20..76 {
                let terrain = classify_terrain(UVec2::new(x, y), grid, &classifier);
                if ANOMALY_BIOMES.contains(&terrain) {
                    seen.insert(terrain);
                }
            }
        }
        for biome in ANOMALY_BIOMES {
            assert!(
                seen.contains(&biome),
                "anomaly biome {biome:?} never produced"
            );
        }
    }

    #[test]
    fn anomaly_footprint_is_a_low_fraction_of_eligible_lowland() {
        // The interior scan window is fully eligible (non-coastal, non-polar lowland), so the
        // anomaly count over it IS the anomaly footprint fraction. It must sit near — and well
        // under a few × — `anomaly_fraction` (rare discovery sites, not the old ~37% blanket).
        let grid = UVec2::new(96, 96);
        let classifier = TerrainClassifierConfig::default();
        let mut anomaly = 0usize;
        let mut total = 0usize;
        for y in 20..76 {
            for x in 20..76 {
                let terrain = classify_terrain(UVec2::new(x, y), grid, &classifier);
                if ANOMALY_BIOMES.contains(&terrain) {
                    anomaly += 1;
                }
                total += 1;
            }
        }
        let fraction = anomaly as f32 / total as f32;
        assert!(fraction > 0.0, "no anomalies at all ({anomaly}/{total})");
        assert!(
            fraction <= classifier.anomaly_fraction * 3.0,
            "anomaly footprint {fraction} far exceeds anomaly_fraction {}",
            classifier.anomaly_fraction
        );
    }

    #[test]
    fn basaltic_lava_field_is_reachable_from_low_relief_volcanic() {
        let grid = UVec2::new(32, 32);
        let position = UVec2::new(12, 16);
        let (low_relief, _) = terrain_for_position_with_context(
            position,
            grid,
            Some(0.3),
            Some(0.8),
            Some((MountainType::Volcanic, 0.6)),
        );
        assert_eq!(low_relief, TerrainType::BasalticLavaField);
        let (high_relief, _) = terrain_for_position_with_context(
            position,
            grid,
            Some(0.3),
            Some(0.9),
            Some((MountainType::Volcanic, 2.0)),
        );
        assert_eq!(high_relief, TerrainType::ActiveVolcanoSlope);
    }

    #[test]
    fn glacier_is_reachable_from_high_relief_polar_mountains() {
        let grid = UVec2::new(64, 48);
        let position = UVec2::new(8, 4); // polar latitude row
        let (terrain, tags) = terrain_for_position_with_context(
            position,
            grid,
            Some(0.15),
            Some(0.95),
            Some((MountainType::Fold, 1.6)), // relief clears alpine_relief_threshold (1.45)
        );
        assert_eq!(terrain, TerrainType::Glacier);
        assert!(tags.contains(TerrainTags::POLAR));
        assert!(tags.contains(TerrainTags::HIGHLAND));
    }

    #[test]
    fn polar_mountains_preserve_polar_tags() {
        let grid = UVec2::new(64, 48);
        let position = UVec2::new(8, 4);
        let (terrain, tags) = terrain_for_position_with_context(
            position,
            grid,
            Some(0.18),
            Some(0.88),
            Some((MountainType::Fold, 1.4)),
        );
        assert_eq!(terrain, TerrainType::SeasonalSnowfield);
        assert!(tags.contains(TerrainTags::POLAR));
        assert!(tags.contains(TerrainTags::HIGHLAND));
    }

    /// Regression for the "AlpineMountain seems very rare" report: a high-relief Fold tile
    /// classifies to AlpineMountain (production) AND survives palette build + remap as
    /// AlpineMountain even on a small map where the Highland niche `K` used to round to 1
    /// and remap towering tiles down to RollingHills. Highland is now un-thinned (K = full),
    /// so every highland member is always on-palette.
    #[test]
    fn alpine_is_produced_and_survives_palette_on_small_map() {
        use crate::biome_palette::BiomePalette;
        use crate::map_preset::MapPresets;

        // Production: a towering Fold tile is classified AlpineMountain.
        let grid = UVec2::new(32, 32);
        let (terrain, tags) = terrain_for_position_with_context(
            UVec2::new(8, 8),
            grid,
            Some(0.55),
            Some(0.9),
            Some((MountainType::Fold, 1.6)),
        );
        assert_eq!(terrain, TerrainType::AlpineMountain);
        assert!(tags.contains(TerrainTags::HIGHLAND));

        // Palette: on a small map (128×80 = 10240 tiles, where Highland K previously rounded
        // to 1) every Highland member — and all three Volcanic members — is on-palette and
        // passes through remap unchanged, for both climate zones.
        let preset = MapPresets::builtin()
            .get("earthlike")
            .expect("earthlike preset")
            .clone();
        let small_tiles = 128 * 80;
        for seed in 0..16u64 {
            let palette = BiomePalette::build(&preset, seed, small_tiles);
            for member in [
                TerrainType::RollingHills,
                TerrainType::HighPlateau,
                TerrainType::AlpineMountain,
                TerrainType::KarstHighland,
                TerrainType::CanyonBadlands,
                TerrainType::ActiveVolcanoSlope,
                TerrainType::BasalticLavaField,
                TerrainType::FumaroleBasin,
            ] {
                assert!(
                    palette.contains(member),
                    "seed {seed}: physically-gated {member:?} missing from small-map palette"
                );
                for is_polar in [false, true] {
                    assert_eq!(
                        palette.remap(member, is_polar),
                        member,
                        "seed {seed}: {member:?} remapped away (polar={is_polar})"
                    );
                }
            }
        }
    }
}
