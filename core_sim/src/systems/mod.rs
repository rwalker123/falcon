use std::{
    borrow::Cow,
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
    combat::{
        resolve_fight, CombatStats, Contingent, ContingentId, FightPayload, Force, ForceId,
        Posture, TerrainContext,
    },
    combat_config::CombatConfigHandle,
    components::{
        available_workers, fragments_from_contract, fragments_to_contract, raid_is_recurring,
        BandBench, BandEquipment, BandId, BandTravel, BuildJob, BuildQueueEntry, BuildSource,
        DeathCause, DemographicFlowAccumulator, ElementKind, Expedition, ExpeditionMission,
        ExpeditionPhase, Improvement, KnowledgeFragment, LaborAllocation, LaborAssignment,
        LaborTarget, LocalStore, MoraleCause, MoraleContributions, MountainMetadata,
        PendingMigration, PopulationCohort, PowerNode, ResidentBand, ShedCrew, ShedFacts,
        SourcePriority, SourceShedFacts, SourceYield, StartingUnit, TakeSelection, Tile,
        TransferLink, YieldRange, DEFAULT_ESCAPEMENT_FLOOR, FODDER, FOOD, STRIP_IT_BARE,
    },
    creatures_config::CreaturesConfigHandle,
    culture::{
        CultureEffectsCache, CultureLayerId, CultureManager, CultureSchismEvent,
        CultureTensionEvent, CultureTensionKind, CultureTensionRecord, CultureTraitAxis,
        CULTURE_TRAIT_AXES, FALLBACK_CULTURE_REGION_ID,
    },
    culture_corruption_config::{CorruptionSeverityConfig, CultureCorruptionConfigHandle},
    demographics_config::{DemographicsConfig, DemographicsConfigHandle, DemographicsConsumption},
    equipment_config::EquipmentConfigHandle,
    expedition_config::ExpeditionConfig,
    fauna::{
        self, herd_capacity, herd_ecology, herd_hunt_yield, sustainable_yield, Herd, HerdRegistry,
        FODDERING_DISCOVERY_ID,
    },
    fauna_config::{Diet, FaunaConfig, FaunaConfigHandle, HuntYield},
    flora_config::FloraConfigHandle,
    food::{classify_food_module, classify_food_module_from_traits, FoodModule, FoodModuleTag},
    forage::{
        forage_escapement_ceiling, forage_per_worker_biomass, forage_provisions, forage_take,
        patch_ecology, patch_provisions_per_biomass_taking, patch_rung, patch_rung_key,
        resolve_committed_species, rung_site_refusal, tended_take_fodder, tile_flora_composition,
        tile_forage_capacity, tile_is_fresh_watered, ForagePatch, ForageRegistry, NO_FORAGE_SEASON,
    },
    generations::GenerationRegistry,
    heightfield::{build_elevation_field, ElevationField, DEFAULT_SEA_LEVEL},
    hydrology::HydrologyState,
    influencers::{InfluencerCultureResonance, InfluencerImpacts},
    intensification::{
        distribute_upkeep_pool, gear_work_supply, knows, BuildGate, BuildQuote, BuildTurns,
        LadderConfig, LadderConfigHandle, LadderKnowledge, RungDef, RungKey,
        NO_CREW_ON_THIS_ACTIVITY, NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
    },
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
        BandIdAllocator, ClimateConfig, CommandEventEntry, CommandEventKind, CommandEventLog,
        CorruptionExposureRecord, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage,
        DiscoveryProgressLedger, FactionInventory, FoodSiteEntry, FoodSiteRegistry,
        FoodSiteWaterBiasReport, MoistureRaster, SentimentAxisBias, SimulationConfig,
        SimulationTick, StartLocation, TileRegistry, TradeDiffusionRecord, TradeTelemetry,
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
use sim_runtime::{merge_fragment_payload, scale_migration_fragments, CorruptionSubsystem};

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

/// **Copy the turn's band-to-band transfer counters onto the cohort, for the frame about to be
/// captured.**
///
/// [`LaborAllocation::last_food_transfers`] / `last_fodder_transfers` accumulate across the whole
/// snapshot window and are cleared by [`reset_transfer_ledger`] once the capture has read them.
/// [`PopulationCohort::last_turn_food_transfers`] / `last_turn_fodder_transfers` are the per-turn
/// twins, and this is the one system that writes them — which is why it runs **only on the turn
/// path**, between `advance_tick` and `capture_snapshot`.
///
/// **That placement is the whole fix.** `snapshot::recapture_snapshot_in_place` re-runs the capture
/// against live components after every dispatched command, by which time the accumulator has been
/// zeroed; a refreshed frame therefore published `0.0` for every term and overwrote the correct
/// turn-end frame. Everything else in the food ledger is a per-turn value on the cohort and re-reads
/// unchanged on a recapture — these join them, and a recapture never reaches this system.
///
/// **The whole accumulator is copied, not this turn's share of it.** At this moment it holds
/// *(command-time draws since the last turn capture) + (this turn's transfers)*, which is exactly the
/// interval a client's `larder_delta` measures, so the per-turn ledger and the accumulator cannot
/// disagree on a turn frame.
///
/// **BOTH ACCOUNTS RIDE ONE PASS.** Hay crosses between larders exactly as grain does, and the two
/// accounts are copied together so a reader never has to ask which of them a frame is current for.
///
/// Like [`reset_transfer_ledger`], and unlike their stage-mate `capture_snapshot`, it is **not**
/// gated on `not_replaying`: a replayed turn publishes nothing, and leaving the copy stale would let
/// the next frame that *is* published report the wrong turn's transfers.
pub fn publish_turn_transfers(mut bands: Query<(&mut PopulationCohort, Option<&LaborAllocation>)>) {
    for (mut cohort, allocation) in bands.iter_mut() {
        // `Option`, matching how the capture reads every ledger term — a band with no allocation
        // reports zero rather than being skipped.
        cohort.last_turn_food_transfers = allocation
            .map(|a| a.last_food_transfers)
            .unwrap_or_default();
        cohort.last_turn_fodder_transfers = allocation
            .map(|a| a.last_fodder_transfers)
            .unwrap_or_default();
    }
}

/// **Clear the band-to-band transfer counters, once the capture has published them.**
///
/// [`LaborAllocation::last_food_transfers`] / `last_fodder_transfers` are the ledgers' terms for
/// goods that crossed between larders, and they are the one telemetry with writers **outside**
/// `run_turn`: a `send_trade_expedition` or `send_expedition` command debits the sending band when it
/// is applied, which is between one snapshot and the next. So every writer *adds*, and exactly one
/// system clears — here, in the Snapshot stage after `capture_snapshot`.
///
/// **The reset point is what defines the window**, and it has to be the *snapshot* window rather than
/// the turn: a client's `larder_delta` is the difference between two published frames, so a term
/// cleared at the top of the turn would drop every command-time draw and leave the identity short by
/// exactly the shipments the player sent.
///
/// It is deliberately **not** gated on `not_replaying` (its stage-mate `capture_snapshot` is): a
/// replayed turn publishes nothing, so leaving the counters standing would carry one turn's
/// transfers into the next frame that *is* published.
pub fn reset_transfer_ledger(mut allocations: Query<&mut LaborAllocation>) {
    for mut allocation in allocations.iter_mut() {
        // Written through the two ledgers rather than `Default` — the rest of the allocation is
        // intent, not telemetry, and must survive.
        allocation.last_food_transfers.clear();
        allocation.last_fodder_transfers.clear();
    }
}

mod crafting;
mod expeditions;
mod fission;
pub(crate) mod labor;
mod population;
mod power;
mod trade;
mod worldgen;

pub use crafting::*;
pub use expeditions::*;
pub use fission::*;
pub use labor::*;
pub use population::*;
pub use power::*;
pub use trade::*;
pub use worldgen::*;
