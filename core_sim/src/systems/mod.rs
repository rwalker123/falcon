use std::{
    cmp::{max, min, Ordering},
    collections::{HashMap, HashSet, VecDeque},
};

use bevy::{ecs::system::SystemParam, math::UVec2, prelude::*};
use log::{debug, info, warn};
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};
use serde_json::json;

use crate::map_preset::{MapPreset, MapPresetsHandle, TerrainClassifierConfig};
#[cfg(test)]
use crate::snapshot_overlays_config::SnapshotOverlaysConfig;
use crate::{
    biome_palette::BiomePalette,
    components::{
        available_workers, fragments_from_contract, fragments_to_contract, BandTravel, ElementKind,
        Expedition, ExpeditionMission, ExpeditionPhase, FollowPolicy, KnowledgeFragment,
        LaborAllocation, LaborTarget, LocalStore, LogisticsLink, MoraleCause, MoraleContributions,
        MountainMetadata, PendingMigration, PopulationCohort, PowerNode, ResidentBand, SourceYield,
        StartingUnit, Tile, TradeLink, FOOD,
    },
    culture::{
        CultureEffectsCache, CultureLayerId, CultureManager, CultureSchismEvent,
        CultureTensionEvent, CultureTensionKind, CultureTensionRecord, CultureTraitAxis,
        CULTURE_TRAIT_AXES,
    },
    culture_corruption_config::{CorruptionSeverityConfig, CultureCorruptionConfigHandle},
    demographics_config::{DemographicsConfig, DemographicsConfigHandle, DemographicsConsumption},
    expedition_config::ExpeditionConfig,
    fauna::{
        self, herd_capacity, herd_ecology, hunt_policy_ceiling, hunt_provisions, pen_upkeep,
        sustainable_yield, EcologyPhase, Herd, HerdDensityMap, HerdRegistry, HERDING_DISCOVERY_ID,
    },
    fauna_config::{EcologyConfig, FaunaConfig, FaunaConfigHandle},
    food::{classify_food_module, classify_food_module_from_traits, FoodModule, FoodModuleTag},
    forage::{forage_take, tended_provisions, ForageRegistry, CULTIVATION_DISCOVERY_ID},
    generations::GenerationRegistry,
    heightfield::{build_elevation_field, ElevationField},
    hydrology::HydrologyState,
    influencers::{InfluencerCultureResonance, InfluencerImpacts},
    labor_config::{LaborConfig, LaborConfigHandle},
    mapgen::MountainType,
    mapgen::{build_bands, validate_bands, TerrainBand, WorldGenSeed},
    orders::{FactionId, FactionRegistry},
    power::{
        PowerGridNodeTelemetry, PowerGridState, PowerIncident, PowerIncidentSeverity, PowerNodeId,
        PowerTopology,
    },
    provinces::{ProvinceId, ProvinceMap},
    resources::{
        ClimateConfig, CommandEventEntry, CommandEventKind, CommandEventLog,
        CorruptionExposureRecord, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage,
        DiscoveryProgressLedger, FactionInventory, FogRevealLedger, FoodSiteEntry,
        FoodSiteRegistry, MoistureRaster, SentimentAxisBias, SimulationConfig, SimulationTick,
        StartLocation, TileRegistry, TradeDiffusionRecord, TradeTelemetry,
    },
    scalar::{scalar_from_f32, scalar_from_u32, scalar_one, scalar_zero, Scalar},
    snapshot_overlays_config::SnapshotOverlaysConfigHandle,
    start_profile::{
        FoodModulePreference, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle,
        StartProfileOverrides, StartingUnitSpec,
    },
    terrain::{terrain_definition, terrain_for_position_with_classifier, TerrainDefinition},
    turn_pipeline_config::TurnPipelineConfigHandle,
    wellbeing_config::{ProductivityConfig, WellbeingConfig, WellbeingConfigHandle},
};
use sim_runtime::{
    apply_openness_decay, merge_fragment_payload, scale_migration_fragments, CorruptionSubsystem,
    TradeLeakCurve,
};

const POLAR_LATITUDE_THRESHOLD: f32 =
    TerrainClassifierConfig::default_values().polar_latitude_cutoff;
const HERD_TRADE_DIFFUSION_BONUS: f32 = 0.25;
const PLAYER_FACTION: FactionId = FactionId(0);
const BUCKET_COLS: u32 = 6;
const BUCKET_ROWS: u32 = 6;
const LATITUDE_BANDS: usize = 3;
const MIN_NEARBY_CURATED_SITES: usize = 2;
const NO_FOOD_SITE_PENALTY: i32 = 18;
const LOW_FOOD_SITE_PENALTY: i32 = 6;

// --- cross-cutting helpers shared by multiple submodules (hoisted per decomposition plan) ---

fn corruption_multiplier(
    ledgers: &CorruptionLedgers,
    subsystem: CorruptionSubsystem,
    penalty: Scalar,
    config: &CorruptionSeverityConfig,
) -> Scalar {
    let raw_intensity = ledgers.total_intensity(subsystem).max(0);
    if raw_intensity == 0 {
        return Scalar::one();
    }
    let intensity = Scalar::from_raw(raw_intensity).clamp(Scalar::zero(), Scalar::one());
    let mut reduction = intensity * penalty;
    reduction = reduction.clamp(Scalar::zero(), config.max_penalty_ratio());
    (Scalar::one() - reduction).clamp(config.min_output_multiplier(), Scalar::one())
}

/// One travel step toward `to`, up to `max_step` tiles per axis. The **x** axis is horizontal-wrap
/// aware: it takes the shortest signed delta (`shortest_delta_x`) so a target across the seam is
/// reached the short way (e.g. left from x=3 to x=73 on an 80-wide wrapping map goes 3→2→1→0→79…),
/// and wraps the result with `wrap_x`. The **y** axis has no wrap (clamped ≥ 0).
fn step_toward(from: UVec2, to: UVec2, max_step: u32, width: u32, wrap_horizontal: bool) -> UVec2 {
    let max = max_step as i32;
    let dx =
        crate::grid_utils::shortest_delta_x(from.x, to.x, width, wrap_horizontal).clamp(-max, max);
    let nx = crate::grid_utils::wrap_x(from.x as i32 + dx, width, wrap_horizontal);
    let dy = (to.y as i64 - from.y as i64).clamp(-(max_step as i64), max_step as i64);
    let ny = (from.y as i64 + dy).max(0) as u32;
    UVec2::new(nx, ny)
}

mod expeditions;
mod labor;
mod population;
mod power;
mod trade;
mod worldgen;

pub use expeditions::*;
pub use labor::*;
pub use population::*;
pub use power::*;
pub use trade::*;
pub use worldgen::*;
