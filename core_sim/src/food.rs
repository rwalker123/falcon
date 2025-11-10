use std::str::FromStr;

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use sim_runtime::{TerrainTags, TerrainType};

use crate::components::Tile;

pub const DEFAULT_HARVEST_TRAVEL_TILES_PER_TURN: f32 = 3.0;
pub const DEFAULT_HARVEST_WORK_TURNS: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoodModule {
    CoastalLittoral,
    RiverineDelta,
    SavannaGrassland,
    TemperateForest,
    BorealArctic,
    MontaneHighland,
    WetlandSwamp,
    SemiAridScrub,
    CoastalUpwelling,
    MixedWoodland,
}

impl FoodModule {
    pub const VARIANTS: [FoodModule; 10] = [
        FoodModule::CoastalLittoral,
        FoodModule::RiverineDelta,
        FoodModule::SavannaGrassland,
        FoodModule::TemperateForest,
        FoodModule::BorealArctic,
        FoodModule::MontaneHighland,
        FoodModule::WetlandSwamp,
        FoodModule::SemiAridScrub,
        FoodModule::CoastalUpwelling,
        FoodModule::MixedWoodland,
    ];

    pub const fn as_str(&self) -> &'static str {
        match self {
            FoodModule::CoastalLittoral => "coastal_littoral",
            FoodModule::RiverineDelta => "riverine_delta",
            FoodModule::SavannaGrassland => "savanna_grassland",
            FoodModule::TemperateForest => "temperate_forest",
            FoodModule::BorealArctic => "boreal_arctic",
            FoodModule::MontaneHighland => "montane_highland",
            FoodModule::WetlandSwamp => "wetland_swamp",
            FoodModule::SemiAridScrub => "semi_arid_scrub",
            FoodModule::CoastalUpwelling => "coastal_upwelling",
            FoodModule::MixedWoodland => "mixed_woodland",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            FoodModule::CoastalLittoral => 0,
            FoodModule::RiverineDelta => 1,
            FoodModule::SavannaGrassland => 2,
            FoodModule::TemperateForest => 3,
            FoodModule::BorealArctic => 4,
            FoodModule::MontaneHighland => 5,
            FoodModule::WetlandSwamp => 6,
            FoodModule::SemiAridScrub => 7,
            FoodModule::CoastalUpwelling => 8,
            FoodModule::MixedWoodland => 9,
        }
    }

    pub const fn variants() -> &'static [FoodModule; 10] {
        &Self::VARIANTS
    }

    pub const fn site_kind(&self) -> FoodSiteKind {
        match self {
            FoodModule::CoastalLittoral => FoodSiteKind::LittoralGathering,
            FoodModule::RiverineDelta => FoodSiteKind::RiverGarden,
            FoodModule::SavannaGrassland => FoodSiteKind::SavannaTrack,
            FoodModule::TemperateForest => FoodSiteKind::ForestForage,
            FoodModule::BorealArctic => FoodSiteKind::ArcticFishing,
            FoodModule::MontaneHighland => FoodSiteKind::HighlandGrove,
            FoodModule::WetlandSwamp => FoodSiteKind::WetlandHarvest,
            FoodModule::SemiAridScrub => FoodSiteKind::ScrubRoots,
            FoodModule::CoastalUpwelling => FoodSiteKind::UpwellingDrying,
            FoodModule::MixedWoodland => FoodSiteKind::WoodlandCache,
        }
    }
}

impl FromStr for FoodModule {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "coastal_littoral" => Ok(FoodModule::CoastalLittoral),
            "riverine_delta" => Ok(FoodModule::RiverineDelta),
            "savanna_grassland" => Ok(FoodModule::SavannaGrassland),
            "temperate_forest" => Ok(FoodModule::TemperateForest),
            "boreal_arctic" => Ok(FoodModule::BorealArctic),
            "montane_highland" => Ok(FoodModule::MontaneHighland),
            "wetland_swamp" => Ok(FoodModule::WetlandSwamp),
            "semi_arid_scrub" => Ok(FoodModule::SemiAridScrub),
            "coastal_upwelling" => Ok(FoodModule::CoastalUpwelling),
            "mixed_woodland" => Ok(FoodModule::MixedWoodland),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoodSiteKind {
    LittoralGathering,
    RiverGarden,
    SavannaTrack,
    ForestForage,
    ArcticFishing,
    HighlandGrove,
    WetlandHarvest,
    ScrubRoots,
    UpwellingDrying,
    WoodlandCache,
}

impl FoodSiteKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            FoodSiteKind::LittoralGathering => "littoral",
            FoodSiteKind::RiverGarden => "river_garden",
            FoodSiteKind::SavannaTrack => "savanna_track",
            FoodSiteKind::ForestForage => "forest_forage",
            FoodSiteKind::ArcticFishing => "arctic_fishing",
            FoodSiteKind::HighlandGrove => "highland_grove",
            FoodSiteKind::WetlandHarvest => "wetland_harvest",
            FoodSiteKind::ScrubRoots => "scrub_roots",
            FoodSiteKind::UpwellingDrying => "upwelling_drying",
            FoodSiteKind::WoodlandCache => "woodland_cache",
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct FoodModuleTag {
    pub module: FoodModule,
    pub seasonal_weight: f32,
}

impl FoodModuleTag {
    pub fn new(module: FoodModule, seasonal_weight: f32) -> Self {
        Self {
            module,
            seasonal_weight,
        }
    }
}

pub fn classify_food_module(tile: &Tile) -> Option<FoodModule> {
    classify_food_module_from_traits(tile.terrain, tile.terrain_tags)
}

pub fn classify_food_module_from_traits(
    terrain: TerrainType,
    tags: TerrainTags,
) -> Option<FoodModule> {
    use TerrainType::*;

    let module = match terrain {
        TidalFlat | RiverDelta | MangroveSwamp | CoralShelf => FoodModule::CoastalLittoral,
        Floodplain | AlluvialPlain => FoodModule::RiverineDelta,
        PrairieSteppe | SemiAridScrub | OasisBasin => FoodModule::SavannaGrassland,
        MixedWoodland | PeatHeath | ImpactCraterField => FoodModule::TemperateForest,
        BorealTaiga | Tundra | PeriglacialSteppe | SeasonalSnowfield => FoodModule::BorealArctic,
        AlpineMountain | KarstHighland | HighPlateau => FoodModule::MontaneHighland,
        HydrothermalVentField | FreshwaterMarsh => FoodModule::WetlandSwamp,
        HotDesertErg | RockyReg | SaltFlat => FoodModule::SemiAridScrub,
        ContinentalShelf | InlandSea => FoodModule::CoastalUpwelling,
        RollingHills | KarstCavernMouth | SinkholeField | AquiferCeiling => {
            FoodModule::MixedWoodland
        }
        _ => {
            if tags.contains(TerrainTags::COASTAL) {
                FoodModule::CoastalLittoral
            } else if tags.contains(TerrainTags::FERTILE) && tags.contains(TerrainTags::FRESHWATER)
            {
                FoodModule::RiverineDelta
            } else if tags.contains(TerrainTags::HIGHLAND) {
                FoodModule::MontaneHighland
            } else if tags.contains(TerrainTags::WETLAND) {
                FoodModule::WetlandSwamp
            } else if tags.contains(TerrainTags::ARID) {
                FoodModule::SemiAridScrub
            } else if tags.contains(TerrainTags::POLAR) {
                FoodModule::BorealArctic
            } else {
                return None;
            }
        }
    };

    Some(module)
}
