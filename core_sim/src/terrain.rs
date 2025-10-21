use bevy::prelude::UVec2;
use sim_runtime::{TerrainTags, TerrainType};

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

pub fn classify_terrain(position: UVec2, grid_size: UVec2) -> TerrainType {
    let width = grid_size.x.max(1) as f32;
    let height = grid_size.y.max(1) as f32;
    let fx = position.x as f32 / width;
    let fy = position.y as f32 / height;
    let edge = fx.min(1.0 - fx).min(fy).min(1.0 - fy);
    let noise = tile_noise(position);
    let humidity = ((noise >> 8) & 0xFF) as f32 / 255.0;
    let elevation = ((noise >> 16) & 0xFF) as f32 / 255.0;
    let anomaly = (noise >> 4) & 0x0F;

    if edge < 0.04 {
        return pick(
            noise,
            &[TerrainType::DeepOcean, TerrainType::HydrothermalVentField],
        );
    }
    if edge < 0.08 {
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
    if edge < 0.12 {
        return pick(
            noise,
            &[
                TerrainType::InlandSea,
                TerrainType::RiverDelta,
                TerrainType::FreshwaterMarsh,
            ],
        );
    }

    if fy < 0.12 || fy > 0.88 {
        return pick(
            noise,
            &[
                TerrainType::Tundra,
                TerrainType::PeriglacialSteppe,
                TerrainType::SeasonalSnowfield,
            ],
        );
    }

    if anomaly == 0 {
        return TerrainType::ImpactCraterField;
    } else if anomaly == 1 {
        return TerrainType::SinkholeField;
    } else if anomaly == 2 {
        return TerrainType::KarstCavernMouth;
    } else if anomaly == 3 {
        return TerrainType::FumaroleBasin;
    } else if anomaly == 4 {
        return TerrainType::ActiveVolcanoSlope;
    }

    if elevation > 0.78 {
        return pick(
            noise,
            &[
                TerrainType::AlpineMountain,
                TerrainType::HighPlateau,
                TerrainType::CanyonBadlands,
            ],
        );
    }

    if elevation > 0.62 {
        return pick(
            noise,
            &[
                TerrainType::RollingHills,
                TerrainType::KarstHighland,
                TerrainType::BasalticLavaField,
            ],
        );
    }

    if humidity > 0.7 {
        return pick(
            noise,
            &[
                TerrainType::Floodplain,
                TerrainType::AlluvialPlain,
                TerrainType::PeatHeath,
            ],
        );
    }

    if humidity > 0.5 {
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

    if fy > 0.65 {
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

pub fn terrain_for_position(position: UVec2, grid_size: UVec2) -> (TerrainType, TerrainTags) {
    let terrain = classify_terrain(position, grid_size);
    let definition = terrain_definition(terrain);
    (terrain, definition.tags)
}

fn tile_noise(position: UVec2) -> u32 {
    let mut n = position.x as u32;
    n = n.wrapping_mul(0x6C8E_9CF5) ^ (position.y as u32).wrapping_mul(0xB529_7A4D);
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
