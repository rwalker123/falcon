use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::str::FromStr;
use std::sync::Arc;

use bevy::{
    ecs::system::{RunSystemOnce, SystemParam},
    prelude::*,
};
use log::warn;
use sim_runtime::{
    encode_delta, encode_delta_flatbuffer, encode_snapshot, encode_snapshot_flatbuffer,
    AccessibleStockpileEntryState, AccessibleStockpileState, AxisBiasState, CohortStoreState,
    CommandEventState, CorruptionLedger, CorruptionSubsystem, CrisisGaugeState,
    CrisisMetricKind as SchemaCrisisMetricKind, CrisisOverlayState,
    CrisisSeverityBand as SchemaCrisisSeverityBand, CrisisTelemetryState,
    CrisisTrendSample as SchemaCrisisTrendSample, CultureLayerState, CultureTensionState,
    CultureTraitEntry, DiscoveredSiteState as SchemaDiscoveredSiteState,
    DiscoveredSitesState as SchemaDiscoveredSitesState, DiscoveryProgressEntry, EcologyState,
    ElevationOverlayState, FactionInventoryEntryState as SchemaFactionInventoryEntryState,
    FactionInventoryState as SchemaFactionInventoryState, FloatRasterState, FoodModuleState,
    ForagePatchState, ForageState, GenerationState, GrazeState, GreatDiscoveryDefinitionState,
    GreatDiscoveryProgressState, GreatDiscoveryState, GreatDiscoveryTelemetryState, HerdRoamState,
    HerdState, HerdTelemetryState, HuntPolicyCeilingState, HuntTripEstimateState,
    InfluentialIndividualState, IntensificationKnowledgeState, KnowledgeLedgerEntryState,
    KnowledgeMetricsState, KnowledgeTimelineEventState, LaborAssignmentState, LogisticsLinkState,
    MountainKind, PendingMigrationState, PopulationCohortState,
    PopulationDemographicsState as SchemaPopulationDemographicsState, PowerIncidentSeverity,
    PowerIncidentState, PowerNodeState, PowerTelemetryState, ScalarRasterState,
    SedentarizationState as SchemaSedentarizationState, SentimentAxisTelemetry,
    SentimentDriverCategory, SentimentDriverState, SentimentTelemetryState,
    SettlementStageViewState, SnapshotHeader, StartMarkerState, TerrainOverlayState, TerrainSample,
    TileState, TradeLinkKnowledge, TradeLinkState, VictoryModeSnapshotState, VictoryResultState,
    VictorySnapshotState, WorldDelta, WorldSnapshot, GRAZE_PHASE_COLLAPSING, GRAZE_PHASE_NONE,
    GRAZE_PHASE_STRESSED, GRAZE_PHASE_THRIVING,
};

use crate::{
    components::{
        available_workers, fragments_from_contract, fragments_to_contract, BandTravel, ElementKind,
        Expedition, ExpeditionMission, ExpeditionPhase, FollowPolicy, LaborAllocation,
        LaborAssignment, LaborTarget, LocalStore, LogisticsLink, MoraleCause, MoraleContributions,
        MountainMetadata, PendingMigration, PopulationCohort, PowerNode, ResidentBand, SourceYield,
        Tile, TradeLink, FOOD,
    },
    culture::{
        CultureEffectsCache, CultureLayer, CultureLayerScope as SimCultureLayerScope,
        CultureManager, CultureOwner, CultureTensionKind as SimCultureTensionKind,
        CultureTensionRecord, CultureTraitAxis as SimCultureTraitAxis,
    },
    demographics_config::{DemographicsConfig, DemographicsConfigHandle},
    expedition_config::ExpeditionConfig,
    fauna::{
        hunt_forecast, pen_upkeep, EcologyPhase, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry,
        SourceYieldForecast, HERDING_DISCOVERY_ID, PENNING_DISCOVERY_ID, PEN_FULLY_FED,
    },
    fauna_config::FaunaConfig,
    food::FoodModuleTag,
    forage::{
        field_provisions, forage_forecast, rung_site_refusal, tile_is_fresh_watered, ForagePatch,
        ForageRegistry, CULTIVATION_DISCOVERY_ID, NO_FORAGE_SEASON, SEED_SELECTION_DISCOVERY_ID,
    },
    generations::{GenerationProfile, GenerationRegistry},
    graze::{GrazePatch, GrazeRegistry},
    great_discovery::{
        snapshot_definitions, snapshot_discoveries, snapshot_progress, snapshot_telemetry,
        GreatDiscoveryId, GreatDiscoveryLedger, GreatDiscoveryReadiness, GreatDiscoveryRegistry,
        GreatDiscoveryTelemetry,
    },
    heightfield::ElevationField,
    influencers::{
        InfluencerBalanceConfig, InfluencerConfigHandle, InfluencerImpacts, InfluentialRoster,
        BUILTIN_INFLUENCER_CONFIG,
    },
    intensification::{LadderConfig, RungKey, SiteRefusal, SITE_ACCEPTED},
    knowledge_ledger::{
        encode_ledger_key, KnowledgeLedger, KnowledgeLedgerConfig, KnowledgeLedgerConfigHandle,
        KnowledgeSnapshotPayload, BUILTIN_KNOWLEDGE_LEDGER_CONFIG,
    },
    labor_config::{ForageLaborConfig, LaborConfig},
    map_preset::MapPresetsHandle,
    metrics::SimulationMetrics,
    orders::FactionId,
    power::{PowerGridState, PowerIncidentSeverity as GridIncidentSeverity, PowerNodeId},
    resources::FoodSiteRegistry,
    resources::{
        CapabilityFlags, CommandEventLog, CorruptionLedgers, CorruptionTelemetry,
        DiscoveryProgressLedger, FactionInventory, FogRevealLedger, MoistureRaster,
        SentimentAxisBias, SimulationConfig, SimulationTick, StartLocation, TileRegistry,
    },
    scalar::{scalar_zero, Scalar},
    sedentarization::SedentarizationScore,
    sites::DiscoveredSites,
    sites_config::SitesConfigHandle,
    snapshot_overlays_config::{SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle},
    start_profile::{snapshot_profiles, CampaignLabel, FogMode, StartProfilesHandle},
    supply::SupplyNetworkMembership,
    systems::{
        food_demand, hunt_per_worker_provisions, hunt_trip_forecast, tile_morale_pressure,
        MoralePressureConfig,
    },
    terrain::terrain_definition,
    turn_pipeline_config::TurnPipelineConfigHandle,
    victory::VictoryState,
};

use crate::mapgen::MountainType;

use crate::crisis::{
    CrisisMetricKind as InternalCrisisMetricKind,
    CrisisMetricsSnapshot as InternalCrisisMetricsSnapshot, CrisisOverlayCache,
    CrisisSeverityBand as InternalCrisisSeverityBand,
    CrisisTrendSample as InternalCrisisTrendSample,
};

mod campaign;
mod capture;
mod culture;
mod economy;
mod governance;
mod knowledge;
mod map;
mod population;
mod subsistence;
mod vision;

pub use campaign::*;
pub use capture::*;
pub(crate) use culture::*;
pub(crate) use economy::*;
pub(crate) use governance::*;
pub(crate) use knowledge::*;
pub(crate) use map::*;
pub(crate) use population::*;
pub(crate) use subsistence::*;
pub(crate) use vision::*;

// --- shared cross-cutting helpers used by multiple snapshot domains ---

const AXIS_NAMES: [&str; 4] = ["Knowledge", "Trust", "Equity", "Agency"];

const CHANNEL_LABELS: [&str; 4] = ["Popular", "Peer", "Institutional", "Humanitarian"];

const DEFAULT_STOCKPILE_ACCESS_RADIUS: u32 = 0;

/// The per-source **yield forecast** (`ForagePatchState`/`HerdTelemetryState` `per_worker_yield` +
/// policy ceilings) is captured band-agnostically: the productivity multiplier is a per-band value
/// (`PopulationCohortState.output_multiplier`) that scales every forecast field linearly, so the
/// snapshot exports the un-scaled forecast and the client multiplies by the acting band's own.
const FORECAST_OUTPUT_MULTIPLIER: f32 = 1.0;

fn diff_new<K, T>(previous: &HashMap<K, T>, current: &HashMap<K, T>) -> Vec<T>
where
    K: Eq + Hash,
    T: Clone + PartialEq,
{
    current
        .iter()
        .filter_map(|(id, state)| match previous.get(id) {
            Some(prev) if prev == state => None,
            _ => Some(state.clone()),
        })
        .collect()
}

fn diff_removed<K, T>(previous: &HashMap<K, T>, current: &HashMap<K, T>) -> Vec<K>
where
    K: Eq + Hash + Copy,
{
    previous
        .keys()
        .filter(|id| !current.contains_key(id))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        intensification::RUNG_COMPLETE,
        labor_config::LaborConfig,
        orders::FactionId,
        power::PowerIncidentSeverity as GridIncidentSeverity,
        resources::{CorruptionTelemetry, DiscoveryProgressLedger},
        scalar::Scalar,
        start_profile::StartProfileOverrides,
        PowerIncident,
    };
    use bevy::math::UVec2;
    use sim_runtime::{
        CorruptionEntry, CorruptionSubsystem, GreatDiscoveryProgressState, GreatDiscoveryState,
        GreatDiscoveryTelemetryState, KnowledgeField, KnownTechFragment, TerrainTags, TerrainType,
        TradeLinkKnowledge,
    };

    #[test]
    fn herd_state_roundtrip_is_identity() {
        // A herd with every field non-default so movement + ecology + domestication all round-trip.
        let original = HerdState {
            id: "herd_test".to_string(),
            label: "Test Herd (herd_test)".to_string(),
            species: "Red Deer".to_string(),
            size_class: "migratory".to_string(),
            route: vec![(3, 4), (5, 6), (7, 2)],
            step_index: 2,
            current_pos: (5, 6),
            dwell_remaining: 3,
            roam: HerdRoamState {
                mode: "loiter".to_string(),
                loiter_turns_left: 9,
            },
            next_pos: Some((7, 2)),
            // Rung 1c: a corralled (penned) herd round-trips its pen tile AND its pen-construction
            // progress through the snapshot (a rollback must not lose a half-built — or finished — pen).
            corralled_at: Some((5, 6)),
            corral_progress: 1.0,
            // Grazing 2d: the pen's fenced footprint radius + the in-flight extend meter/state round-trip.
            pen_radius: 1,
            pen_extend_progress: 0.5,
            pen_extending: true,
            // Grazing 2b-i: the cached per-species eating rate round-trips too, so a rollback restores
            // the draw-down rate rather than leaving a rehydrated herd grazing at 0.
            fodder_per_biomass: 0.05,
            // Grazing 2b-ii: the cached per-species wild regrowth rate round-trips too.
            regrowth_rate: 0.04,
            // Grazing 2d-δ: the species' husbandry ceiling round-trips (mammoth = wild → hunt-only).
            husbandry_ceiling: "wild".to_string(),
            ecology: EcologyState {
                biomass: 4321.0,
                carrying_capacity: 8000.0,
                ecology_phase: "stressed".to_string(),
                progress: 1.0,
                owner: Some(1),
            },
        };

        // HerdState -> Herd (restore side) -> HerdState (capture side) must be an identity.
        let registry = HerdRegistry::from_states(std::slice::from_ref(&original));
        let herd = registry.entries().first().expect("one herd restored");
        assert!(herd.is_corralled(), "corralled_at restores the pen");
        let restored = herd_state(herd);

        assert_eq!(restored, original);
    }

    /// A **half-built** pen (the Corral investment mid-flight) must round-trip through the rollback
    /// snapshot: a rollback rewinds the investment, it never silently loses it. The herd is still
    /// mobile (`corralled_at` is `None`) until `corral_progress` reaches `1.0`.
    #[test]
    fn half_built_corral_progress_round_trips() {
        const HALF_BUILT: f32 = 0.44;
        let original = HerdState {
            id: "herd_penning".to_string(),
            size_class: "small".to_string(),
            route: vec![(2, 2)],
            current_pos: (2, 2),
            roam: HerdRoamState {
                mode: "graze_wander".to_string(),
                loiter_turns_left: 0,
            },
            // Mid-build: the pen is not finished, so the herd is still mobile.
            corralled_at: None,
            corral_progress: HALF_BUILT,
            // Set explicitly (like `size_class`): an empty default normalizes to "pen" and would break
            // the round-trip identity.
            husbandry_ceiling: "pen".to_string(),
            ecology: EcologyState {
                biomass: 60.0,
                carrying_capacity: 100.0,
                ecology_phase: "thriving".to_string(),
                // Domesticated + owned — the gates the Corral policy requires to keep building.
                progress: 1.0,
                owner: Some(2),
            },
            ..Default::default()
        };

        let registry = HerdRegistry::from_states(std::slice::from_ref(&original));
        let herd = registry.entries().first().expect("one herd restored");
        assert!(!herd.is_corralled(), "a half-built pen is not yet a corral");
        assert_eq!(herd.corral_progress, HALF_BUILT);
        assert_eq!(herd_state(herd), original);
    }

    fn tile(entity: u64, x: u32, y: u32) -> TileState {
        TileState {
            entity,
            x,
            y,
            element: 0,
            mass: 0,
            temperature: 0,
            terrain: TerrainType::AlluvialPlain,
            terrain_tags: TerrainTags::empty(),
            culture_layer: 0,
            mountain_kind: MountainKind::None,
            mountain_relief: 1.0,
            habitability: 0,
            graze_biomass: 0.0,
            graze_capacity: 0.0,
            graze_ecology_phase: GRAZE_PHASE_NONE,
            forage_capacity: 0.0,
            underlying_terrain: TerrainType::AlluvialPlain,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        }
    }

    /// `TileState::forage_capacity` is the biome's HUMAN-food potential, read straight from
    /// `forage.capacity_by_biome` for EVERY tile (not from the sparse `ForagePatch`). Confirms the
    /// four contract cases + the no-drift consistency check against a seeded patch.
    #[test]
    fn tile_state_exports_forage_potential_from_biome_table() {
        let labor = LaborConfig::builtin();
        let forage = &labor.forage;
        // The place-based morale terms are irrelevant to the forage readout; zero them out.
        let morale_cfg = MoralePressureConfig {
            ambient_temperature: Scalar::zero(),
            temperature_morale_penalty: Scalar::zero(),
            temperature_morale_tolerance: Scalar::zero(),
            attrition_penalty_scale: Scalar::zero(),
            hardness_penalty_scale: Scalar::zero(),
        };
        let at = |terrain: TerrainType| Tile {
            position: UVec2::new(1, 1),
            element: ElementKind::Arborite,
            mass: Scalar::zero(),
            temperature: Scalar::zero(),
            terrain,
            terrain_tags: TerrainTags::empty(),
            underlying_terrain: None,
            mountain: None,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        };
        let entity = Entity::from_raw(1);
        let capture = |terrain: TerrainType, graze: Option<&GrazePatch>| {
            tile_state(entity, &at(terrain), &morale_cfg, graze, forage).forage_capacity
        };

        // (a) A food-module tile that DOES hold a `ForagePatch` — the patch was seeded at
        //     `capacity_for(biome)`, so the exported potential must equal its carrying capacity (no
        //     drift between the potential and the realized patch).
        let module_terrain = TerrainType::MixedWoodland;
        let seeded = ForagePatch::new(
            at(module_terrain).position,
            forage.capacity_for(module_terrain),
        );
        assert_eq!(capture(module_terrain, None), seeded.carrying_capacity);

        // (b) A non-food-module LAND tile with a positive-forage biome still exports a NON-ZERO
        //     potential — the whole point: the client sees the biome's potential everywhere, not only
        //     where a patch happens to sit.
        assert!(capture(TerrainType::PrairieSteppe, None) > 0.0);

        // (c) Fishery WATER carries a non-zero fishing value — the deliberate divergence from graze
        //     (where all water is zero): a fishery is a food module on water.
        assert!(capture(TerrainType::ContinentalShelf, None) > 0.0);

        // (d) A genuinely-zero biome reads a STATED zero.
        assert_eq!(capture(TerrainType::Glacier, None), 0.0);
        assert_eq!(capture(TerrainType::DeepOcean, None), 0.0);
    }

    fn snapshot_with_overlay(
        tick: u64,
        tile: TileState,
        overlay: TerrainOverlayState,
    ) -> WorldSnapshot {
        let tiles = vec![tile];
        let header = SnapshotHeader::new(tick, tiles.len(), 0, 0, 0, 0, 0);
        WorldSnapshot {
            header,
            tiles,
            logistics: Vec::new(),
            trade_links: Vec::new(),
            populations: Vec::new(),
            power: Vec::new(),
            power_metrics: PowerTelemetryState::default(),
            great_discovery_definitions: Vec::new(),
            great_discoveries: Vec::new(),
            great_discovery_progress: Vec::new(),
            great_discovery_telemetry: GreatDiscoveryTelemetryState::default(),
            knowledge_ledger: Vec::new(),
            knowledge_timeline: Vec::new(),
            knowledge_metrics: KnowledgeMetricsState::default(),
            victory: VictorySnapshotState::default(),
            crisis_telemetry: CrisisTelemetryState::default(),
            crisis_overlay: CrisisOverlayState::default(),
            capability_flags: 0,
            campaign_profiles: Vec::new(),
            command_events: Vec::new(),
            herds: Vec::new(),
            herd_registry: Vec::new(),
            forage_registry: Vec::new(),
            graze_registry: Vec::new(),
            food_modules: Vec::new(),
            faction_inventory: Vec::new(),
            sedentarization: Vec::new(),
            discovered_sites: Vec::new(),
            demographics: Vec::new(),
            forage_patches: Vec::new(),
            intensification_knowledge: Vec::new(),
            terrain: overlay,
            moisture_raster: FloatRasterState::default(),
            elevation_overlay: ElevationOverlayState::default(),
            start_marker: None,
            logistics_raster: ScalarRasterState::default(),
            sentiment_raster: ScalarRasterState::default(),
            corruption_raster: ScalarRasterState::default(),
            fog_raster: ScalarRasterState::default(),
            culture_raster: ScalarRasterState::default(),
            military_raster: ScalarRasterState::default(),
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
            generations: Vec::new(),
            corruption: CorruptionLedger::default(),
            influencers: Vec::new(),
            culture_layers: Vec::new(),
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: ScalarRasterState::default(),
        }
        .finalize()
    }

    fn snapshot_with_discoveries(
        tick: u64,
        great_discoveries: Vec<GreatDiscoveryState>,
        great_discovery_progress: Vec<GreatDiscoveryProgressState>,
        great_discovery_telemetry: GreatDiscoveryTelemetryState,
    ) -> WorldSnapshot {
        let header = SnapshotHeader::new(tick, 0, 0, 0, 0, 0, 0);
        WorldSnapshot {
            header,
            tiles: Vec::new(),
            logistics: Vec::new(),
            trade_links: Vec::new(),
            populations: Vec::new(),
            power: Vec::new(),
            power_metrics: PowerTelemetryState::default(),
            great_discovery_definitions: Vec::new(),
            great_discoveries,
            great_discovery_progress,
            great_discovery_telemetry,
            knowledge_ledger: Vec::new(),
            knowledge_timeline: Vec::new(),
            knowledge_metrics: KnowledgeMetricsState::default(),
            victory: VictorySnapshotState::default(),
            crisis_telemetry: CrisisTelemetryState::default(),
            crisis_overlay: CrisisOverlayState::default(),
            capability_flags: 0,
            campaign_profiles: Vec::new(),
            command_events: Vec::new(),
            herds: Vec::new(),
            herd_registry: Vec::new(),
            forage_registry: Vec::new(),
            graze_registry: Vec::new(),
            food_modules: Vec::new(),
            faction_inventory: Vec::new(),
            sedentarization: Vec::new(),
            discovered_sites: Vec::new(),
            demographics: Vec::new(),
            forage_patches: Vec::new(),
            intensification_knowledge: Vec::new(),
            moisture_raster: FloatRasterState::default(),
            elevation_overlay: ElevationOverlayState::default(),
            start_marker: None,
            terrain: TerrainOverlayState::default(),
            logistics_raster: ScalarRasterState::default(),
            sentiment_raster: ScalarRasterState::default(),
            corruption_raster: ScalarRasterState::default(),
            fog_raster: ScalarRasterState::default(),
            culture_raster: ScalarRasterState::default(),
            military_raster: ScalarRasterState::default(),
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
            generations: Vec::new(),
            corruption: CorruptionLedger::default(),
            influencers: Vec::new(),
            culture_layers: Vec::new(),
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: ScalarRasterState::default(),
        }
        .finalize()
    }

    fn snapshot_with_power_metrics(tick: u64, power_metrics: PowerTelemetryState) -> WorldSnapshot {
        let header = SnapshotHeader::new(tick, 0, 0, 0, 0, 0, 0);
        WorldSnapshot {
            header,
            tiles: Vec::new(),
            logistics: Vec::new(),
            trade_links: Vec::new(),
            populations: Vec::new(),
            power: Vec::new(),
            power_metrics,
            great_discovery_definitions: Vec::new(),
            great_discoveries: Vec::new(),
            great_discovery_progress: Vec::new(),
            great_discovery_telemetry: GreatDiscoveryTelemetryState::default(),
            knowledge_ledger: Vec::new(),
            knowledge_timeline: Vec::new(),
            knowledge_metrics: KnowledgeMetricsState::default(),
            victory: VictorySnapshotState::default(),
            crisis_telemetry: CrisisTelemetryState::default(),
            crisis_overlay: CrisisOverlayState::default(),
            capability_flags: 0,
            campaign_profiles: Vec::new(),
            command_events: Vec::new(),
            herds: Vec::new(),
            herd_registry: Vec::new(),
            forage_registry: Vec::new(),
            graze_registry: Vec::new(),
            food_modules: Vec::new(),
            faction_inventory: Vec::new(),
            sedentarization: Vec::new(),
            discovered_sites: Vec::new(),
            demographics: Vec::new(),
            forage_patches: Vec::new(),
            intensification_knowledge: Vec::new(),
            moisture_raster: FloatRasterState::default(),
            elevation_overlay: ElevationOverlayState::default(),
            start_marker: None,
            terrain: TerrainOverlayState::default(),
            logistics_raster: ScalarRasterState::default(),
            sentiment_raster: ScalarRasterState::default(),
            corruption_raster: ScalarRasterState::default(),
            fog_raster: ScalarRasterState::default(),
            culture_raster: ScalarRasterState::default(),
            military_raster: ScalarRasterState::default(),
            axis_bias: AxisBiasState::default(),
            sentiment: SentimentTelemetryState::default(),
            generations: Vec::new(),
            corruption: CorruptionLedger::default(),
            influencers: Vec::new(),
            culture_layers: Vec::new(),
            culture_tensions: Vec::new(),
            discovery_progress: Vec::new(),
            visibility_raster: ScalarRasterState::default(),
        }
        .finalize()
    }

    /// Build a minimal content band for the food-flow snapshot test, with the given age brackets
    /// (fixed-point) and labor allocation.
    fn food_test_cohort(
        children: Scalar,
        working: Scalar,
        elders: Scalar,
        allocation: LaborAllocation,
    ) -> (PopulationCohort, LaborAllocation) {
        let cohort = PopulationCohort {
            home: Entity::from_raw(2),
            current_tile: Entity::from_raw(2),
            size: 30,
            children,
            working,
            elders,
            stores: LocalStore::new(),
            morale: crate::scalar::scalar_one(),
            last_food_consumption: 0.0,
            last_morale_delta: crate::scalar::scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: Default::default(),
            discontent_fraction: crate::scalar::scalar_zero(),
            grievance: crate::scalar::scalar_zero(),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: 0,
            generation: 0,
            faction: FactionId(0),
            knowledge: Vec::new(),
            migration: None,
        };
        (cohort, allocation)
    }

    /// Capture a cohort's `PopulationCohortState` with all-default configs (isolates the food-flow
    /// wiring). Returns the built state.
    fn capture_food_state(
        cohort: &PopulationCohort,
        allocation: &LaborAllocation,
    ) -> PopulationCohortState {
        let inventory = FactionInventory::default();
        let demographics = crate::demographics_config::DemographicsConfig::default();
        let wellbeing = crate::wellbeing_config::WellbeingConfig::default();
        let membership = crate::supply::SupplyNetworkMembership::default();
        let stages = crate::settlement_stage_config::SettlementStageConfig::default();
        // The expedition levers are irrelevant to the food-flow wiring under test.
        let levers = ExpeditionLevers {
            max_party_size: 0,
            hunt_per_worker_carry: 0.0,
            hunt_per_worker_provisions: 0.0,
            hunt_viability_warn_turns: 0,
        };
        population_state(
            Entity::from_raw(1),
            cohort,
            Some(allocation),
            None,
            Some(UVec2::new(0, 0)),
            None,
            false,
            0,
            None,
            &inventory,
            &demographics,
            &wellbeing,
            &membership,
            0,
            0,
            &levers,
            &stages,
            None,
            0,
        )
    }

    /// (d) `food_income` = Σ per-source `actual_yield`, `food_consumption` = the food the people
    /// actually ate this turn (`cohort.last_food_consumption`), and each labor-assignment row carries
    /// its matching actual/sustainable yield (zipped by index).
    #[test]
    fn population_state_reports_food_income_and_consumption() {
        let working = Scalar::from_f32(30.0);
        let allocation = LaborAllocation {
            assignments: vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: UVec2::new(0, 0),
                        policy: crate::components::FollowPolicy::Sustain,
                    },
                    workers: 10,
                },
                LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: "game_1".to_string(),
                        policy: crate::components::FollowPolicy::Sustain,
                    },
                    workers: 5,
                },
            ],
            last_yields: vec![
                SourceYield {
                    actual: 2.5,
                    sustainable: 2.5,
                    workers_needed: 1,
                },
                SourceYield {
                    actual: 0.5,
                    sustainable: 0.25,
                    workers_needed: 5,
                },
            ],
            last_pen_feed_upkeep: 0.0,
        };
        let (mut cohort, allocation) = food_test_cohort(
            Scalar::from_f32(0.0),
            working,
            Scalar::from_f32(0.0),
            allocation,
        );
        // The food the people actually ate this turn (the real `stores` debit `simulate_population`
        // records), which the ledger's `food_consumption` term echoes verbatim — NOT a `food_demand`
        // re-derived at capture on the post-turn brackets (that would break the larder identity by
        // the same turn's population growth).
        const CONSUMED: f32 = 4.13;
        cohort.last_food_consumption = CONSUMED;
        let state = capture_food_state(&cohort, &allocation);

        // food_income = Σ actual (2.5 + 0.5).
        assert!(
            (state.food_income - 3.0).abs() < 1e-5,
            "food_income sums per-source actual: {}",
            state.food_income
        );
        // food_consumption == the food actually eaten (`cohort.last_food_consumption`).
        assert!(
            (state.food_consumption - CONSUMED).abs() < 1e-5,
            "food_consumption == last_food_consumption: {} vs {}",
            state.food_consumption,
            CONSUMED
        );
        // Each assignment row carries its zipped actual/sustainable.
        assert_eq!(state.labor_assignments.len(), 2);
        assert!((state.labor_assignments[0].actual_yield - 2.5).abs() < 1e-5);
        assert!((state.labor_assignments[0].sustainable_yield - 2.5).abs() < 1e-5);
        assert!((state.labor_assignments[1].actual_yield - 0.5).abs() < 1e-5);
        assert!((state.labor_assignments[1].sustainable_yield - 0.25).abs() < 1e-5);
        // The overstaffing signal (workers_needed) carries onto the display state, zipped by index.
        assert_eq!(state.labor_assignments[0].workers_needed, 1);
        assert_eq!(state.labor_assignments[1].workers_needed, 5);
    }

    /// A rehydrated allocation (empty `last_yields`) reports zero food income and zero per-row yields
    /// — the default-0.0 branch — while still exporting the assignment rows.
    #[test]
    fn population_state_food_income_defaults_to_zero_without_telemetry() {
        let allocation = LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    policy: crate::components::FollowPolicy::Sustain,
                },
                workers: 10,
            }],
            last_yields: Vec::new(),
            last_pen_feed_upkeep: 0.0,
        };
        let (cohort, allocation) = food_test_cohort(
            Scalar::from_f32(0.0),
            Scalar::from_f32(30.0),
            Scalar::from_f32(0.0),
            allocation,
        );
        let state = capture_food_state(&cohort, &allocation);
        assert_eq!(state.food_income, 0.0, "no telemetry → zero income");
        assert_eq!(state.labor_assignments.len(), 1);
        assert_eq!(state.labor_assignments[0].actual_yield, 0.0);
        assert_eq!(state.labor_assignments[0].sustainable_yield, 0.0);
        assert_eq!(state.labor_assignments[0].workers_needed, 0);
    }

    /// A `LaborTarget::Forage` policy round-trips through the snapshot (§0-iii): `to_state` writes
    /// the policy string and `from_state` parses it back, so a rollback preserves the gather policy
    /// (parity with the Hunt arm). A non-Sustain policy is the interesting case (empty string would
    /// silently default to Sustain).
    #[test]
    fn forage_policy_roundtrips_through_snapshot() {
        use crate::components::FollowPolicy;
        let target = LaborTarget::Forage {
            tile: UVec2::new(7, 9),
            policy: FollowPolicy::Market,
        };
        let assignment = LaborAssignment { target, workers: 6 };
        let state = labor_assignment_to_state(
            &assignment,
            SourceYield {
                actual: 0.0,
                sustainable: 0.0,
                workers_needed: 0,
            },
        );
        assert_eq!(state.policy, "market", "policy serialized");

        let restored = labor_allocation_from_state(std::slice::from_ref(&state));
        assert_eq!(restored.assignments.len(), 1);
        assert_eq!(restored.assignments[0], assignment, "policy round-trips");
    }

    #[test]
    fn power_metrics_from_grid_tracks_totals() {
        let mut grid = PowerGridState {
            total_supply: Scalar::from_f32(12.5),
            total_demand: Scalar::from_f32(10.0),
            total_storage: Scalar::from_f32(4.5),
            total_capacity: Scalar::from_f32(18.0),
            grid_stress_avg: 0.35,
            surplus_margin: 0.22,
            instability_alerts: 3,
            ..Default::default()
        };
        grid.incidents.push(PowerIncident {
            node_id: PowerNodeId(42),
            severity: GridIncidentSeverity::Critical,
            deficit: Scalar::from_f32(1.2),
        });
        grid.incidents.push(PowerIncident {
            node_id: PowerNodeId(99),
            severity: GridIncidentSeverity::Warning,
            deficit: Scalar::from_f32(0.4),
        });

        let telemetry = power_metrics_from_grid(&grid);
        assert_eq!(telemetry.total_supply, Scalar::from_f32(12.5).raw());
        assert_eq!(telemetry.total_demand, Scalar::from_f32(10.0).raw());
        assert_eq!(telemetry.total_storage, Scalar::from_f32(4.5).raw());
        assert_eq!(telemetry.total_capacity, Scalar::from_f32(18.0).raw());
        assert!((telemetry.grid_stress_avg - 0.35).abs() < f32::EPSILON);
        assert!((telemetry.surplus_margin - 0.22).abs() < f32::EPSILON);
        assert_eq!(telemetry.instability_alerts, 3);
        assert_eq!(telemetry.incidents.len(), 2);

        let mut saw_critical = false;
        let mut saw_warning = false;
        for incident in &telemetry.incidents {
            match incident.severity {
                PowerIncidentSeverity::Critical => {
                    saw_critical = true;
                    assert_eq!(incident.node_id, 42);
                    assert_eq!(incident.deficit, Scalar::from_f32(1.2).raw());
                }
                PowerIncidentSeverity::Warning => {
                    saw_warning = true;
                    assert_eq!(incident.node_id, 99);
                    assert_eq!(incident.deficit, Scalar::from_f32(0.4).raw());
                }
            }
        }
        assert!(saw_critical, "expected critical incident serialized");
        assert!(saw_warning, "expected warning incident serialized");
    }

    #[test]
    fn terrain_overlay_delta_updates_on_biome_change() {
        let base_tile = TileState {
            entity: 1,
            x: 0,
            y: 0,
            element: 0,
            mass: 0,
            temperature: 0,
            terrain: TerrainType::AlluvialPlain,
            terrain_tags: TerrainTags::FERTILE,
            culture_layer: 0,
            mountain_kind: MountainKind::None,
            mountain_relief: 1.0,
            habitability: 0,
            graze_biomass: 0.0,
            graze_capacity: 0.0,
            graze_ecology_phase: GRAZE_PHASE_NONE,
            forage_capacity: 0.0,
            underlying_terrain: TerrainType::AlluvialPlain,
            river_edges: 0,
            river_inflow: 0,
            river_channel: 0,
        };
        let base_overlay = TerrainOverlayState {
            width: 1,
            height: 1,
            samples: vec![TerrainSample {
                terrain: base_tile.terrain,
                tags: base_tile.terrain_tags,
                mountain_kind: base_tile.mountain_kind,
                relief_scale: base_tile.mountain_relief,
            }],
        };
        let base_snapshot = snapshot_with_overlay(1, base_tile.clone(), base_overlay);

        let mut history = SnapshotHistory::default();
        history.update(base_snapshot);

        let updated_tile = TileState {
            terrain: TerrainType::MangroveSwamp,
            terrain_tags: TerrainTags::COASTAL | TerrainTags::WETLAND,
            ..base_tile
        };
        let updated_overlay = TerrainOverlayState {
            width: 1,
            height: 1,
            samples: vec![TerrainSample {
                terrain: updated_tile.terrain,
                tags: updated_tile.terrain_tags,
                mountain_kind: updated_tile.mountain_kind,
                relief_scale: updated_tile.mountain_relief,
            }],
        };
        let updated_snapshot =
            snapshot_with_overlay(2, updated_tile.clone(), updated_overlay.clone());

        history.update(updated_snapshot);

        let delta = history
            .last_delta
            .as_ref()
            .expect("delta captured after terrain change");
        let terrain_delta = delta
            .terrain
            .as_ref()
            .expect("terrain overlay delta emitted");

        assert_eq!(terrain_delta, &updated_overlay);
        assert_eq!(terrain_delta.samples.len(), 1);
        let sample = &terrain_delta.samples[0];
        assert_eq!(sample.terrain, updated_tile.terrain);
        assert_eq!(sample.tags, updated_tile.terrain_tags);

        let latest_snapshot = history
            .last_snapshot
            .as_ref()
            .expect("latest snapshot retained");
        assert_eq!(latest_snapshot.terrain, updated_overlay);
    }

    #[test]
    fn snapshot_history_records_power_metrics_delta() {
        let mut history = SnapshotHistory::default();

        let baseline = snapshot_with_power_metrics(1, PowerTelemetryState::default());
        history.update(baseline);

        let updated_metrics = PowerTelemetryState {
            total_supply: Scalar::from_f32(20.0).raw(),
            total_demand: Scalar::from_f32(15.0).raw(),
            total_storage: Scalar::from_f32(5.0).raw(),
            total_capacity: Scalar::from_f32(25.0).raw(),
            grid_stress_avg: 0.42,
            surplus_margin: -0.1,
            instability_alerts: 4,
            incidents: vec![
                PowerIncidentState {
                    node_id: 7,
                    severity: PowerIncidentSeverity::Critical,
                    deficit: Scalar::from_f32(2.3).raw(),
                },
                PowerIncidentState {
                    node_id: 11,
                    severity: PowerIncidentSeverity::Warning,
                    deficit: Scalar::from_f32(0.8).raw(),
                },
            ],
        };
        let updated_snapshot = snapshot_with_power_metrics(2, updated_metrics.clone());
        history.update(updated_snapshot);

        let delta = history
            .last_delta
            .as_ref()
            .expect("delta captured after power metrics change");
        let power_delta = delta
            .power_metrics
            .as_ref()
            .expect("power metrics delta emitted");

        assert_eq!(
            power_delta.instability_alerts,
            updated_metrics.instability_alerts
        );
        assert_eq!(power_delta.incidents.len(), updated_metrics.incidents.len());
        assert!(
            (power_delta.grid_stress_avg - updated_metrics.grid_stress_avg).abs() < f32::EPSILON
        );
        assert!((power_delta.surplus_margin - updated_metrics.surplus_margin).abs() < f32::EPSILON);

        let latest_snapshot = history
            .last_snapshot
            .as_ref()
            .expect("latest snapshot retained");
        assert_eq!(latest_snapshot.power_metrics, updated_metrics);
    }

    #[test]
    fn great_discovery_snapshot_delta_tracks_changes() {
        let mut history = SnapshotHistory::default();

        let baseline = snapshot_with_discoveries(
            1,
            Vec::new(),
            Vec::new(),
            GreatDiscoveryTelemetryState::default(),
        );
        history.update(baseline);

        let discovery = GreatDiscoveryState {
            id: 7,
            faction: 3,
            field: KnowledgeField::Physics,
            tick: 2,
            publicly_deployed: true,
            effect_flags: 0b0101,
        };
        let progress = GreatDiscoveryProgressState {
            faction: 3,
            discovery: 7,
            progress: 500_000,
            observation_deficit: 2,
            eta_ticks: 4,
            covert: false,
        };
        let telemetry = GreatDiscoveryTelemetryState {
            total_resolved: 1,
            pending_candidates: 2,
            active_constellations: 1,
        };

        let updated = snapshot_with_discoveries(
            2,
            vec![discovery.clone()],
            vec![progress.clone()],
            telemetry.clone(),
        );
        history.update(updated);

        let delta = history
            .last_delta
            .as_ref()
            .expect("delta captured after great discovery changes");

        assert_eq!(delta.great_discoveries, vec![discovery.clone()]);
        assert_eq!(delta.great_discovery_progress, vec![progress.clone()]);
        assert_eq!(delta.great_discovery_telemetry.as_ref(), Some(&telemetry));

        let latest = history
            .last_snapshot
            .as_ref()
            .expect("latest snapshot stored");
        assert_eq!(latest.great_discoveries, vec![discovery]);
        assert_eq!(latest.great_discovery_progress, vec![progress]);
        assert_eq!(latest.great_discovery_telemetry, telemetry);
    }

    #[test]
    fn corruption_raster_allocates_intensity_and_baseline() {
        let tiles = vec![tile(1, 0, 0), tile(2, 1, 0)];

        let logistics_raster = ScalarRasterState {
            width: 2,
            height: 1,
            samples: vec![Scalar::from_f32(1.2).raw(), Scalar::from_f32(0.2).raw()],
        };

        let trade_links = vec![TradeLinkState {
            entity: 10,
            from_faction: 0,
            to_faction: 1,
            throughput: Scalar::from_f32(0.6).raw(),
            tariff: 0,
            knowledge: TradeLinkKnowledge::default(),
            from_tile: 2,
            to_tile: 2,
            pending_fragments: Vec::new(),
        }];

        let populations = vec![
            PopulationCohortState {
                entity: 100,
                home: 1,
                current_x: 0,
                current_y: 0,
                is_traveling: false,
                size: 120,
                children: 0,
                working: 0,
                elders: 0,
                stores: Vec::new(),
                age_turns: 0,
                days_of_food: 0.0,
                activity: String::new(),
                supply_network_id: 0,
                morale_delta: 0,
                morale_cause: 0,
                morale: Scalar::from_f32(0.3).raw(),
                generation: 0,
                faction: 0,
                knowledge_fragments: Vec::new(),
                migration: None,
                harvest_task: None,
                scout_task: None,
                accessible_stockpile: None,
                ..Default::default()
            },
            PopulationCohortState {
                entity: 101,
                home: 2,
                current_x: 0,
                current_y: 0,
                is_traveling: false,
                size: 80,
                children: 0,
                working: 0,
                elders: 0,
                stores: Vec::new(),
                age_turns: 0,
                days_of_food: 0.0,
                activity: String::new(),
                supply_network_id: 0,
                morale_delta: 0,
                morale_cause: 0,
                morale: Scalar::from_f32(0.8).raw(),
                generation: 0,
                faction: 1,
                knowledge_fragments: Vec::new(),
                migration: None,
                harvest_task: None,
                scout_task: None,
                accessible_stockpile: None,
                ..Default::default()
            },
        ];

        let power_nodes = vec![
            PowerNodeState {
                entity: 1,
                node_id: 1,
                generation: Scalar::from_f32(0.9).raw(),
                demand: Scalar::from_f32(0.4).raw(),
                efficiency: Scalar::one().raw(),
                storage_level: Scalar::zero().raw(),
                storage_capacity: Scalar::zero().raw(),
                stability: Scalar::one().raw(),
                surplus: Scalar::zero().raw(),
                deficit: Scalar::zero().raw(),
                incident_count: 0,
            },
            PowerNodeState {
                entity: 2,
                node_id: 2,
                generation: Scalar::from_f32(0.4).raw(),
                demand: Scalar::from_f32(0.2).raw(),
                efficiency: Scalar::one().raw(),
                storage_level: Scalar::zero().raw(),
                storage_capacity: Scalar::zero().raw(),
                stability: Scalar::one().raw(),
                surplus: Scalar::zero().raw(),
                deficit: Scalar::zero().raw(),
                incident_count: 0,
            },
        ];

        let mut ledger = CorruptionLedger::default();
        ledger.entries.push(CorruptionEntry {
            subsystem: CorruptionSubsystem::Logistics,
            intensity: Scalar::from_f32(0.6).raw(),
            ..CorruptionEntry::default()
        });
        ledger.entries.push(CorruptionEntry {
            subsystem: CorruptionSubsystem::Trade,
            intensity: Scalar::from_f32(0.3).raw(),
            ..CorruptionEntry::default()
        });

        let telemetry = CorruptionTelemetry::default();

        let overlays_config = SnapshotOverlaysConfig::default();
        let raster = corruption_raster_from_simulation(CorruptionRasterInputs {
            tiles: &tiles,
            trade_links: &trade_links,
            populations: &populations,
            power_nodes: &power_nodes,
            logistics_raster: &logistics_raster,
            corruption_signals: CorruptionSignals {
                ledger: &ledger,
                telemetry: &telemetry,
            },
            grid_size: UVec2::new(2, 1),
            overlays: &overlays_config,
        });

        assert_eq!(raster.width, 2);
        assert_eq!(raster.height, 1);
        assert_eq!(raster.samples.len(), 2);
        assert!(raster.samples[0] > 0);
        assert!(raster.samples[1] > 0);
        assert!(raster.samples[0] > raster.samples[1]);
    }

    #[test]
    fn fog_raster_reflects_discovery_progress() {
        let tiles = vec![tile(1, 0, 0), tile(2, 1, 0)];

        let populations = vec![
            PopulationCohortState {
                entity: 200,
                home: 1,
                current_x: 0,
                current_y: 0,
                is_traveling: false,
                size: 150,
                children: 0,
                working: 0,
                elders: 0,
                stores: Vec::new(),
                age_turns: 0,
                days_of_food: 0.0,
                activity: String::new(),
                supply_network_id: 0,
                morale_delta: 0,
                morale_cause: 0,
                morale: Scalar::from_f32(0.5).raw(),
                generation: 0,
                faction: 0,
                knowledge_fragments: vec![KnownTechFragment {
                    discovery_id: 1,
                    progress: Scalar::from_f32(0.6).raw(),
                    fidelity: Scalar::one().raw(),
                }],
                migration: None,
                harvest_task: None,
                scout_task: None,
                accessible_stockpile: None,
                ..Default::default()
            },
            PopulationCohortState {
                entity: 201,
                home: 2,
                current_x: 0,
                current_y: 0,
                is_traveling: false,
                size: 60,
                children: 0,
                working: 0,
                elders: 0,
                stores: Vec::new(),
                age_turns: 0,
                days_of_food: 0.0,
                activity: String::new(),
                supply_network_id: 0,
                morale_delta: 0,
                morale_cause: 0,
                morale: Scalar::from_f32(0.7).raw(),
                generation: 0,
                faction: 1,
                knowledge_fragments: Vec::new(),
                migration: None,
                harvest_task: None,
                scout_task: None,
                accessible_stockpile: None,
                ..Default::default()
            },
        ];

        let mut discovery = DiscoveryProgressLedger::default();
        discovery.add_progress(FactionId(0), 1, Scalar::from_f32(0.8));
        discovery.add_progress(FactionId(0), 2, Scalar::from_f32(0.4));

        let overlays_config = SnapshotOverlaysConfig::default();
        let start_location = StartLocation::default();
        let fog_reveals = FogRevealLedger::default();
        let fog = fog_raster_from_discoveries(
            &tiles,
            &populations,
            &discovery,
            UVec2::new(2, 1),
            &overlays_config,
            &start_location,
            &fog_reveals,
            0,
        );

        assert_eq!(fog.width, 2);
        assert_eq!(fog.height, 1);
        assert!(fog.samples[0] < Scalar::one().raw());
        assert_eq!(fog.samples[1], Scalar::one().raw());
    }

    #[test]
    fn fog_raster_revealed_mode_clears_samples() {
        let tiles = vec![tile(1, 0, 0), tile(2, 1, 0)];
        let populations = Vec::new();
        let discovery = DiscoveryProgressLedger::default();
        let overlays_config = SnapshotOverlaysConfig::default();
        let overrides = StartProfileOverrides {
            fog_mode: Some(FogMode::Revealed),
            ..Default::default()
        };
        let start_location = StartLocation::from_profile(Some(UVec2::new(0, 0)), &overrides);
        let fog_reveals = FogRevealLedger::default();

        let fog = fog_raster_from_discoveries(
            &tiles,
            &populations,
            &discovery,
            UVec2::new(2, 1),
            &overlays_config,
            &start_location,
            &fog_reveals,
            0,
        );

        assert!(fog
            .samples
            .iter()
            .all(|sample| *sample == Scalar::zero().raw()));
    }

    #[test]
    fn fog_raster_shroud_only_reveals_radius() {
        let tiles = vec![tile(1, 0, 0), tile(2, 1, 0)];
        let populations = Vec::new();
        let discovery = DiscoveryProgressLedger::default();
        let overlays_config = SnapshotOverlaysConfig::default();
        let overrides = StartProfileOverrides {
            fog_mode: Some(FogMode::Shroud),
            survey_radius: Some(0),
            ..Default::default()
        };
        let start_location = StartLocation::from_profile(Some(UVec2::new(0, 0)), &overrides);
        let fog_reveals = FogRevealLedger::default();

        let fog = fog_raster_from_discoveries(
            &tiles,
            &populations,
            &discovery,
            UVec2::new(2, 1),
            &overlays_config,
            &start_location,
            &fog_reveals,
            0,
        );

        assert_eq!(fog.samples[0], Scalar::zero().raw());
        assert_eq!(fog.samples[1], Scalar::one().raw());
    }

    fn demographics_cohort(
        faction: u32,
        size: u32,
        children: f32,
        working: f32,
        elders: f32,
    ) -> PopulationCohortState {
        PopulationCohortState {
            faction,
            size,
            children: Scalar::from_f32(children).raw(),
            working: Scalar::from_f32(working).raw(),
            elders: Scalar::from_f32(elders).raw(),
            ..Default::default()
        }
    }

    #[test]
    fn snapshot_demographics_reconciles_with_band_totals() {
        // Independent rounding of 8.9/16.5/4.6 would overshoot to 9+17+5 = 31, but the band's
        // authoritative size is 30 and available_workers floors 16.5 to 16.
        let cohorts = vec![demographics_cohort(0, 30, 8.9, 16.5, 4.6)];
        let demographics = snapshot_demographics(&cohorts);
        assert_eq!(demographics.len(), 1);
        let d = &demographics[0];
        assert_eq!(d.faction, 0);
        assert_eq!(d.working, 16, "working matches Σ available_workers (floor)");
        assert_eq!(
            d.children + d.working + d.elders,
            30,
            "brackets sum to Σ size (client Pop matches band size)"
        );
        // Dependents 14 split ∝ 8.9:4.6 → children round(9.23)=9, elders remainder 5.
        assert_eq!(d.children, 9);
        assert_eq!(d.elders, 5);
    }

    #[test]
    fn snapshot_demographics_sums_multiple_bands_per_faction() {
        let cohorts = vec![
            demographics_cohort(2, 30, 8.9, 16.5, 4.6),
            demographics_cohort(2, 20, 5.4, 10.5, 4.1),
            // A different faction stays separate.
            demographics_cohort(7, 10, 2.0, 6.5, 1.5),
        ];
        let demographics = snapshot_demographics(&cohorts);
        assert_eq!(demographics.len(), 2);

        let f2 = demographics.iter().find(|d| d.faction == 2).unwrap();
        // Σ available_workers = floor(16.5) + floor(10.5) = 16 + 10 = 26.
        assert_eq!(f2.working, 26);
        // Σ size = 50.
        assert_eq!(f2.children + f2.working + f2.elders, 50);

        let f7 = demographics.iter().find(|d| d.faction == 7).unwrap();
        assert_eq!(f7.working, 6);
        assert_eq!(f7.children + f7.working + f7.elders, 10);
    }

    #[test]
    fn snapshot_demographics_clamps_workers_above_headcount() {
        // Degenerate: floored workers exceed size — dependents must clamp to zero, not underflow.
        let cohorts = vec![demographics_cohort(1, 5, 0.0, 9.9, 0.0)];
        let demographics = snapshot_demographics(&cohorts);
        let d = &demographics[0];
        assert_eq!(d.working, 5);
        assert_eq!(d.children, 0);
        assert_eq!(d.elders, 0);
    }

    #[test]
    fn snapshot_forage_patches_reports_cultivation_and_owner() {
        let mut registry = ForageRegistry::default();
        // A wild, untended patch: no cultivation, no owner.
        let wild = ForagePatch::new(UVec2::new(1, 0), 100.0);
        // A tended (cultivated) patch owned by faction 3.
        let mut tended = ForagePatch::new(UVec2::new(0, 1), 100.0);
        tended.cultivation_progress = 1.0;
        tended.owner = Some(FactionId(3));
        registry.patches.insert(wild.tile, wild);
        registry.patches.insert(tended.tile, tended);

        let labor = LaborConfig::builtin();
        let patches = snapshot_forage_patches(
            &registry,
            &labor.forage,
            &LadderConfig::builtin(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(patches.len(), 2);
        // Emitted in stable (y, x) order: (1,0) then (0,1).
        assert_eq!((patches[0].x, patches[0].y), (1, 0));
        assert_eq!((patches[1].x, patches[1].y), (0, 1));

        let w = &patches[0];
        assert!(!w.is_cultivated);
        assert_eq!(w.cultivation_progress, 0.0);
        assert_eq!(w.owner, None);

        let t = &patches[1];
        assert!(t.is_cultivated);
        assert!((t.cultivation_progress - 1.0).abs() < 1e-6);
        assert_eq!(t.owner, Some(3));
    }

    #[test]
    fn snapshot_intensification_knowledge_reports_learned_ladders() {
        let mut ledger = DiscoveryProgressLedger::default();
        // Faction 2 fully knows Cultivation and is partway to Herding.
        ledger.add_progress(FactionId(2), CULTIVATION_DISCOVERY_ID, Scalar::one());
        ledger.add_progress(FactionId(2), HERDING_DISCOVERY_ID, Scalar::from_f32(0.5));
        // Faction 5 has only unrelated discovery progress → no intensification row.
        ledger.add_progress(FactionId(5), 1, Scalar::one());

        let rows = snapshot_intensification_knowledge(&ledger);
        assert_eq!(rows.len(), 1, "only factions on the ladders appear");
        let f2 = &rows[0];
        assert_eq!(f2.faction, 2);
        assert!((f2.cultivation - 1.0).abs() < 1e-6);
        assert!((f2.herding - 0.5).abs() < 1e-6);
    }

    #[test]
    fn herd_snapshot_reports_corralled_state() {
        use crate::fauna_config::SizeClass;
        let mut registry = HerdRegistry::default();
        let mut penned = Herd::new(
            "herd_pen".to_string(),
            "Aurochs".to_string(),
            SizeClass::Big,
            vec![UVec2::new(4, 4)],
            50.0,
            100.0,
            0.0,
            0.05,
        );
        penned.corral_at(UVec2::new(4, 4));
        registry.herds.push(penned);
        // A second, un-penned herd stays mobile (corralled = false).
        registry.herds.push(Herd::new(
            "herd_wild".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(1, 1)],
            50.0,
            100.0,
            0.0,
            0.05,
        ));

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();
        let expedition = ExpeditionConfig::builtin();
        let states = herd_snapshot_entries(
            &telemetry,
            &registry,
            &fauna,
            &LadderConfig::builtin(),
            &labor,
            &expedition,
            bevy::math::UVec2::new(64, 64),
            false,
        );
        let pen = states.iter().find(|h| h.id == "herd_pen").unwrap();
        assert!(pen.corralled, "a penned herd reports corralled");
        let wild = states.iter().find(|h| h.id == "herd_wild").unwrap();
        assert!(!wild.corralled, "a mobile herd reports not corralled");
    }

    /// **Grazing 2b-iii — the ecological readout on the wire.** A herd exports its live derived
    /// carrying capacity K and the exact hex radius the sim grazes/derives K over. The radius is
    /// resolved from the `SpeciesDef` (migratory `loiter_radius`) exactly as `advance_herds` does, so a
    /// small/big/migratory species each reports the footprint the sim actually uses (0 / 1 /
    /// `loiter_radius`), and the client can reproduce the ring with `hex_range_tiles`.
    #[test]
    fn herd_snapshot_reports_carrying_capacity_and_graze_range_radius() {
        use crate::fauna_config::SizeClass;

        let fauna = FaunaConfig::builtin();
        let labor = LaborConfig::builtin();
        let expedition = ExpeditionConfig::builtin();

        // One mobile herd per size class, each a real species so `species_by_display` resolves the
        // migratory `loiter_radius`. Distinct carrying capacities so the assertion is meaningful.
        let mut registry = HerdRegistry::default();
        registry.herds.push(Herd::new(
            "herd_small".to_string(),
            "Rabbit Warren".to_string(),
            SizeClass::Small,
            vec![UVec2::new(2, 2)],
            120.0,
            163.0,
            0.10,
            0.35,
        ));
        registry.herds.push(Herd::new(
            "herd_big".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(4, 4)],
            900.0,
            1352.0,
            0.05,
            0.10,
        ));
        registry.herds.push(Herd::new(
            "herd_migratory".to_string(),
            "Thunder Mammoths".to_string(),
            SizeClass::Migratory,
            vec![UVec2::new(6, 6)],
            8000.0,
            9000.0,
            0.011,
            0.04,
        ));

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let states = herd_snapshot_entries(
            &telemetry,
            &registry,
            &fauna,
            &LadderConfig::builtin(),
            &labor,
            &expedition,
            bevy::math::UVec2::new(64, 64),
            false,
        );

        for herd in &registry.herds {
            let exported = states.iter().find(|h| h.id == herd.id).unwrap();
            assert!(
                (exported.carrying_capacity - herd.carrying_capacity).abs() < 1e-6,
                "{}: exported K {} should equal the live K {}",
                herd.id,
                exported.carrying_capacity,
                herd.carrying_capacity,
            );
            let expected_radius = herd.graze_range_radius(fauna.species_by_display(&herd.species));
            assert_eq!(
                exported.graze_range_radius, expected_radius,
                "{}: exported graze range radius should equal graze_range_radius(def)",
                herd.id,
            );
        }

        // Pin the per-size expectations so a regression in the size_class → radius mapping is caught.
        let small = states.iter().find(|h| h.id == "herd_small").unwrap();
        assert_eq!(
            small.graze_range_radius, 0,
            "small game grazes its one tile"
        );
        let big = states.iter().find(|h| h.id == "herd_big").unwrap();
        assert_eq!(big.graze_range_radius, 1, "big game grazes radius 1");
        let migratory = states.iter().find(|h| h.id == "herd_migratory").unwrap();
        assert_eq!(
            migratory.graze_range_radius,
            fauna
                .species_by_display("Thunder Mammoths")
                .unwrap()
                .loiter_radius,
            "a migratory herd grazes its loiter_radius",
        );
    }

    /// **The pen as a managed population, on the wire.** A penned herd exports what it EATS
    /// (`pen_upkeep = pen.upkeep_per_biomass × biomass`) alongside its **gross** `corral_yield`, plus
    /// last turn's `pen_fed_fraction` (`< 1` = starving) — what the client needs for the herd drawer
    /// and the starving warning. A herd that is not penned is never starving.
    #[test]
    fn herd_snapshot_reports_the_pens_upkeep_and_fed_fraction() {
        use crate::fauna_config::SizeClass;
        const PEN_BIOMASS: f32 = 60.0;
        const HALF_FED: f32 = 0.5;

        let mut registry = HerdRegistry::default();
        let mut penned = Herd::new(
            "herd_pen".to_string(),
            "Aurochs".to_string(),
            SizeClass::Big,
            vec![UVec2::new(4, 4)],
            PEN_BIOMASS,
            100.0,
            0.0,
            0.05,
        );
        penned.corral_at(UVec2::new(4, 4));
        // The keeper could only pay half the feed last turn → the herd is starving.
        penned.pen_fed_fraction = HALF_FED;
        registry.herds.push(penned);
        registry.herds.push(Herd::new(
            "herd_wild".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(1, 1)],
            50.0,
            100.0,
            0.0,
            0.05,
        ));

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();
        let expedition = ExpeditionConfig::builtin();
        let states = herd_snapshot_entries(
            &telemetry,
            &registry,
            &fauna,
            &LadderConfig::builtin(),
            &labor,
            &expedition,
            bevy::math::UVec2::new(64, 64),
            false,
        );

        let pen = states.iter().find(|h| h.id == "herd_pen").unwrap();
        let expected_upkeep = fauna.husbandry.pen.upkeep_per_biomass * PEN_BIOMASS;
        assert!(
            (pen.pen_upkeep - expected_upkeep).abs() < 1e-6,
            "the pen exports its feed demand at the herd's current biomass: {} vs {expected_upkeep}",
            pen.pen_upkeep
        );
        assert!((pen.pen_fed_fraction - HALF_FED).abs() < 1e-6);
        assert!(
            pen.corral_yield > 0.0,
            "the pen's gross managed yield rides alongside its upkeep"
        );

        let wild = states.iter().find(|h| h.id == "herd_wild").unwrap();
        assert_eq!(
            wild.pen_fed_fraction, 1.0,
            "a mobile herd is never starving"
        );
    }

    /// **The feed must be known at the moment the player DECIDES.** `penUpkeep` is answered for an
    /// **unpenned** herd too — the feed the pen *would* demand once built, at the herd's current
    /// biomass — because the pre-commit `Corral` row is by definition looking at a herd that is not yet
    /// penned. Quoting `corralYield` (the payoff) while reporting `penUpkeep = 0` (the running cost)
    /// would advertise a number the player will never bank: the same defect class as quoting the gross
    /// yield. The two are computed on the **same biomass basis**, so the client can just subtract.
    #[test]
    fn an_unpenned_herd_exports_the_feed_its_pen_would_demand() {
        use crate::fauna_config::SizeClass;
        /// Above the managed harvest's escapement point (`K/2`), so the pen has a positive projected
        /// yield to sit the projected feed *next to* — at or below `K/2` a pen honestly pays nothing
        /// until the herd rebuilds, and the row would be `0 → 0`.
        const BIOMASS: f32 = 900.0;
        const CAP: f32 = 1200.0;

        let mut registry = HerdRegistry::default();
        // A tamed but MOBILE herd — exactly what a player inspects while deciding whether to corral.
        let mut mobile = Herd::new(
            "herd_mobile".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(2, 2)],
            BIOMASS,
            CAP,
            0.0,
            0.05,
        );
        mobile.accrue_domestication(FactionId(0), RUNG_COMPLETE);
        registry.herds.push(mobile);
        // The same herd, penned — its upkeep must read the same at the same biomass.
        let mut penned = Herd::new(
            "herd_penned".to_string(),
            "Red Deer".to_string(),
            SizeClass::Big,
            vec![UVec2::new(3, 3)],
            BIOMASS,
            CAP,
            0.0,
            0.05,
        );
        penned.accrue_domestication(FactionId(0), RUNG_COMPLETE);
        penned.corral_at(UVec2::new(3, 3));
        registry.herds.push(penned);

        let telemetry = HerdTelemetry {
            entries: registry.snapshot_entries(),
        };
        let labor = LaborConfig::builtin();
        let fauna = FaunaConfig::builtin();
        let expedition = ExpeditionConfig::builtin();
        let states = herd_snapshot_entries(
            &telemetry,
            &registry,
            &fauna,
            &LadderConfig::builtin(),
            &labor,
            &expedition,
            bevy::math::UVec2::new(64, 64),
            false,
        );

        let expected = fauna.husbandry.pen.upkeep_per_biomass * BIOMASS;
        assert!(expected > 0.0);

        let mobile = states.iter().find(|h| h.id == "herd_mobile").unwrap();
        assert!(
            !mobile.corralled,
            "the herd under consideration is NOT penned"
        );
        assert!(
            (mobile.pen_upkeep - expected).abs() < 1e-6,
            "an unpenned herd must export the feed its pen WOULD demand \
             (upkeep_per_biomass × biomass = {expected}): got {}",
            mobile.pen_upkeep
        );
        assert!(
            mobile.corral_yield > 0.0,
            "the payoff is already projected for an unpenned herd — the cost must be too"
        );

        // A penned herd is unchanged, and reads the SAME upkeep at the same biomass: one field, one
        // meaning, so `corralYield − penUpkeep` is a valid subtraction on either side of the decision.
        let penned = states.iter().find(|h| h.id == "herd_penned").unwrap();
        assert!(penned.corralled);
        assert!(
            (penned.pen_upkeep - mobile.pen_upkeep).abs() < 1e-6,
            "penned and unpenned must agree at the same biomass: {} vs {}",
            penned.pen_upkeep,
            mobile.pen_upkeep
        );
    }
}
