//! Core simulation crate for the Shadow-Scale headless prototype.
//!
//! Provides deterministic ECS systems that resolve a single turn of the
//! simulation when [`run_turn`] is invoked.

/// Human-readable build identifier for the server binary, **auto-generated at
/// compile time** by `build.rs` as `<commit-date>-<short-hash>` (e.g.
/// `2026-07-09-a1b2c3d`) so it always reflects the actual build and can never be
/// a stale hand-bumped constant. It is stamped onto each snapshot header
/// (`SnapshotHeader::server_build`) and shown in the client's version overlay so
/// the running server build can be confirmed at a glance. Falls back to
/// `dev-unknown` when git metadata is unavailable (offline/CI/exported source).
pub(crate) const BUILD_ID: &str = match option_env!("CORE_SIM_BUILD_ID") {
    Some(v) => v,
    None => "dev-unknown",
};

pub mod auto_craft;
pub mod band_improvement;
mod band_names;
pub mod belief;
mod belief_config;
pub mod belief_relay;
mod biome_palette;
pub mod carry;
pub mod climate;
pub mod combat;
mod combat_config;
mod components;
pub mod config_fingerprint;
mod config_load;
pub mod config_override;
pub mod connections;
mod connections_config;
pub mod craft_suggestions;
pub mod crafting;
mod creatures_config;
mod crisis;
mod crisis_config;
mod culture;
mod culture_corruption_config;
pub mod dealings;
mod demographics_config;
mod diplomacy_config;
mod equipment_config;
mod espionage;
mod expedition_config;
pub mod extraction;
mod extraction_config;
mod faction_names;
mod fauna;
mod fauna_config;
pub mod fixed_store;
mod flora_config;
mod food;
mod forage;
pub mod forecast_query;
mod generations;
pub mod goodwill;
mod graze;
mod great_discovery;
pub mod grid_utils;
pub mod hashing;
pub mod heightfield;
pub mod hunt_by_need;
mod hydrology;
mod influencers;
mod intensification;
pub mod knowledge_contact;
mod knowledge_contact_config;
mod knowledge_ledger;
mod labor_config;
pub mod ladder_ids;
mod leader_names;
mod leadership;
mod leadership_config;
mod lineage;
pub mod log_stream;
mod map_preset;
mod mapgen;
mod materials_config;
pub mod metrics;
mod mobility;
mod mobility_config;
pub mod network;
mod orders;
pub mod personalities;
pub mod port_alloc;
mod power;
mod provinces;
mod recipes_config;
pub mod record;
mod resources;
pub mod roaming_skills;
pub mod routes;
pub mod save;
pub mod save_store;
mod scalar;
pub mod seats;
mod sedentarization;
mod sedentarization_config;
mod settlement_stage_config;
pub mod sim_state;
mod sites;
mod sites_config;
mod snapshot;
mod snapshot_overlays_config;
pub mod spoilage;
pub mod staffing;
pub mod staffing_preview;
mod start_profile;
pub mod starting_loadout;
mod supply;
mod supply_network_config;
mod systems;
pub mod take_claims;
pub mod telling;
mod terrain;
pub mod tile_improvement;
pub mod trade_history;
pub mod treaties;
mod tribe;
mod turn_pipeline_config;
pub mod turn_profile;
mod victory;
mod visibility;
mod visibility_config;
mod visibility_systems;
mod wellbeing_config;
pub mod work_party;

use std::sync::Arc;

use crate::map_preset::load_map_presets_from_env;
use crate::start_profile::{
    load_start_profile_knowledge_tags_from_env, load_start_profiles_from_env,
};
use bevy::ecs::schedule::{LogLevel, ScheduleBuildSettings};
use bevy::prelude::*;

pub use band_names::{
    load_band_names_from_env, BandNameCatalog, BandNameCatalogHandle, BandNameCatalogMetadata,
    BandNamesError, BAND_NAME_SALT, BUILTIN_BAND_NAMES,
};
pub use belief::{
    advance_improvement_belief, advance_ritual_trickle, credit_ritual_lesson,
    credit_ritual_practice, BeliefRegistry, NO_BELIEF, RITUAL_DISCOVERY_ID, RITUAL_KNOWLEDGE,
};
pub use belief_config::{
    load_belief_config_from_env, BeliefConfig, BeliefConfigHandle, BeliefConfigMetadata,
    BUILTIN_BELIEF_CONFIG,
};
pub use belief_relay::{resolve_belief_relay, BeliefRelay, RelayBand};
pub use carry::{carry_capacity, per_worker_carry, CarryConfig, CarryLoad};
pub use combat::{
    attacks_landed_at, landed_strikes_seeded, resolve_fight, strike_damage, units_brought_down,
    CombatStats, CombatTuning, Contingent, ContingentId, ContingentResult, DamageLedger,
    FightOutcome, FightPayload, Force, ForceId, Posture, RangeBand, StrikeDraw, TerrainContext,
    EXPECTED_STRIKES,
};
pub use combat_config::{
    load_combat_config_from_env, CombatConfig, CombatConfigHandle, CombatConfigMetadata,
    BUILTIN_COMBAT_CONFIG,
};
pub use components::{
    available_workers, floor_is_valid, floor_overdraws, take_overdraws, BandBench, BandEquipment,
    BandId, BandName, BandTravel, BandWorkforce, BatchGrade, BenchOrder, BenchQueueError,
    BuildDeclaration, BuildJob, BuildSource, BuildToeLine, DeathCause, DemographicFlowAccumulator,
    DrawnInputs, DrawnMaterial, ElementKind, EquipmentBatch, Expedition, ExpeditionMission,
    ExpeditionPhase, FinishedBatch, FoodBatch, FoodMix, KeepingIssue, KeepingToolLine, KitOrder,
    KnowledgeFragment, LaborAllocation, LaborAssignment, LaborTarget, LocalStore, MaterialBatch,
    MaterialDraw, MoraleCause, MusterBench, MusterKit, MusterPlan, PoolToeLine, PopulationCohort,
    PowerNode, ResidentBand, ScoutPack, Settlement, ShedCrew, ShedFacts, ShedStep, ShedSubject,
    ShipmentTerms, SourcePriority, SourceShedFacts, SourceYield, StaffedCrew, StartingUnit,
    StationReport, StrangersOrder, TakeSelection, Tile, TownCenter, TransferCause,
    TransferCounterparty, TransferCrossing, TransferDirection, TransferLedger, TransferLink,
    YieldRange, DEFAULT_ESCAPEMENT_FLOOR, FODDER, FOOD, HEAD_ORDER, MIN_ORDER_COUNT,
    NO_IMPROVEMENT_UNDERWAY, STRIP_IT_BARE,
};
pub use config_fingerprint::{
    current_config_fingerprint, drift_between, ConfigDigest, ConfigFingerprint,
};
pub use config_load::ConfigLoadError;
pub use config_override::{
    clear_config_overrides, install_config_override, spec_for as config_override_spec_for,
    ConfigKindSpec, ConfigOverrideError, InstalledOverride,
};
pub use connections::{
    advance_connections, Connection, ConnectionKey, ConnectionLedger, ContactsThisTurn,
    GatheringLedger, Sighting, FULL_TIE, NO_TIE,
};
pub use connections_config::{
    load_connections_config_from_env, ConnectionStrengthConfig, ConnectionsConfig,
    ConnectionsConfigHandle, ConnectionsConfigMetadata, KinRelayConfig, BUILTIN_CONNECTIONS_CONFIG,
};
pub use creatures_config::{
    load_creatures_config_from_env, CreatureDef, CreaturesConfig, CreaturesConfigHandle,
    CreaturesConfigMetadata, BUILTIN_CREATURES_CONFIG, PERSON_ID,
};
pub use crisis::{
    ActiveCrisisLedger, CrisisGaugeSnapshot, CrisisMetricKind, CrisisMetricsSnapshot,
    CrisisOverlayCache, CrisisSeverityBand, CrisisTelemetry, CrisisTelemetrySample,
    CrisisTrendSample,
};
pub use crisis_config::{
    load_crisis_archetypes_from_env, load_crisis_modifiers_from_env,
    load_crisis_telemetry_config_from_env, CrisisArchetype, CrisisArchetypeCatalog,
    CrisisArchetypeCatalogHandle, CrisisArchetypeCatalogMetadata, CrisisModifier,
    CrisisModifierCatalog, CrisisModifierCatalogHandle, CrisisModifierCatalogMetadata,
    CrisisTelemetryConfig, CrisisTelemetryConfigHandle, CrisisTelemetryConfigMetadata,
    CrisisTelemetryThreshold, BUILTIN_CRISIS_ARCHETYPES, BUILTIN_CRISIS_MODIFIERS,
    BUILTIN_CRISIS_TELEMETRY_CONFIG,
};
pub use culture::{
    culture_region_at, max_axis_gap, may_break_away, mutual_contact_ties,
    reconcile_band_culture_layers, reconcile_culture_layers, seeded_modifiers_for_band,
    ContactPull, ContactTie, CultureEffectsCache, CultureLayer, CultureLayerId, CultureLayerScope,
    CultureManager, CultureOwner, CultureSchismEvent, CultureSplitQueue, CultureTensionEvent,
    CultureTensionKind, CultureTensionRecord, CultureTraitAxis, CultureTraitVector,
    StartingCultureLean, CULTURE_TRAIT_AXES, CULTURE_TRAIT_SPAN, FALLBACK_CULTURE_REGION_ID,
};
pub use culture_corruption_config::{
    CorruptionSeverityConfig, CultureCorruptionConfig, CultureCorruptionConfigHandle,
    CultureSeverityConfig, CultureTensionTuning, BUILTIN_CULTURE_CORRUPTION_CONFIG,
};
pub use dealings::{
    advance_dealings, answer_dealing, bands_have_standing_link, check_cargo, check_destination,
    propose_trade, propose_treaty, standing_answer_covers, Agreement, AnswerReport, Dealing,
    DealingAnswer, DealingId, DealingKind, GoodsLines, PendingDealings, ShipmentGate,
    TreatyCarrier, TreatyProposal,
};
pub use demographics_config::{
    load_demographics_config_from_env, DemographicsConfig, DemographicsConfigHandle,
    DemographicsConfigMetadata, KeepingClass, KeepingConfig,
};
pub use diplomacy_config::{
    load_diplomacy_config_from_env, DiplomacyConfig, DiplomacyConfigError, DiplomacyConfigHandle,
    DiplomacyConfigMetadata, DiplomacyRoadsConfig, GoodwillConfig, OffersConfig,
    BUILTIN_DIPLOMACY_CONFIG,
};
pub use equipment_config::{
    load_equipment_config_from_env, BandItemBudget, Crew, DefaultKitsConfig, EffectTier,
    EquipmentConfig, EquipmentConfigHandle, EquipmentConfigMetadata, EquipmentEffect,
    EquipmentStat, EquipmentTier, ItemDefinition, KitChoice, KitCoverage, KitDefinition, KitJob,
    KitSelectionError, LiveItem, Quarry, WearConfig, WearQuantum, BUILTIN_EQUIPMENT_CONFIG,
};
pub use espionage::{
    AgentAssignment, CounterIntelBudgets, EspionageAgentHandle, EspionageCatalog,
    EspionageMissionId, EspionageMissionInstanceId, EspionageMissionKind, EspionageMissionState,
    EspionageMissionTemplate, EspionageRoster, FactionSecurityPolicies, QueueMissionError,
    QueueMissionParams, SecurityPolicy,
};
pub use expedition_config::{
    load_expedition_config_from_env, DefectionConfig, ExpeditionConfig, ExpeditionConfigHandle,
    ExpeditionConfigMetadata, SettleConfig, BUILTIN_EXPEDITION_CONFIG,
};
pub use faction_names::{
    load_faction_names_from_env, FactionNameCatalog, FactionNameCatalogHandle,
    FactionNameCatalogMetadata, FactionNames, FactionNamesError, BUILTIN_FACTION_NAMES,
    FACTION_NAME_SALT,
};
pub use fauna::{
    advance_herd_grazing, advance_herds, advance_husbandry, advance_predation, animals_affordable,
    animals_engaged, animals_sparable, animals_that_stay, animals_that_stay_at_rate,
    build_prey_index, cancel_dropped_rings, carnivore_k_at, choose_pen_site, crew_keep_hands,
    crew_needed_with_keeping, drop_holding_and_cancel_ring, escapement_ceiling,
    expected_kill_over_retreat, expected_stayers, forecast_expected_take, forecast_take_range,
    herd_breeding_gain, herd_build_rung, herd_build_verb, herd_capacity, herd_carry_rate,
    herd_claims_keeping, herd_crew_keeping, herd_crew_keeping_next_turn, herd_default_hunt_kit,
    herd_density_gain, herd_destination_capacity, herd_ecology, herd_engage_rate, herd_fight_stage,
    herd_head_count, herd_herded_fraction, herd_herders_needed, herd_hunt_yield, herd_keeper_load,
    herd_keeper_loads, herd_keeping_basis, herd_meter_rot, herd_past_recovery, herd_quarry_fight,
    herd_rung_already_built, herd_rung_key, herd_size_tier, herd_space_capacity,
    herd_standing_provisions, herd_standing_rung_share, herd_standing_scale, herd_take_room,
    herd_upkeep_demand, herd_upkeep_shortfall, herd_upkeep_supply, herd_upkeep_workers_needed,
    herd_wariness, herd_work_keeper_load, hunt_armed_crew, hunt_crew_for_take, hunt_crew_needed,
    hunt_crew_room, hunt_crew_take_curve, hunt_crew_turns_to_kill, hunt_engage_workers,
    hunt_escapement_ceiling, hunt_fight_workers, hunt_haul_workers, hunt_kill_progress,
    hunt_source_yield_preview, hunt_take_bound, hunt_take_overdraws, hunt_take_workers,
    hunt_useful_crew, kill_carry_rate, kill_over_retreat, migratory_corridor_tiles,
    next_turns_quarry, pen_site_in_world, pen_site_is_free, pen_tiles_claimed_by,
    per_hunter_take_biomass, project_arrivals_hunt, project_realized_hunt, quantise_animal_take,
    quarry_default_hunt_kit, regrow_biomass, regrowth_delta_at, repopulate_fauna,
    resolve_hunt_engagement, resolve_hunt_fight, retreat_band_edge, retreat_mean, retreat_seed,
    spawn_initial_herds, species_requires_denial, stamp_breeding_gains,
    stamp_migratory_game_trails, stay_fraction, sustainable_yield, taken_pen_tiles,
    unqueue_build_and_cancel_ring, would_be_herders_needed, AnimalTake, Contact, CrewContact,
    CrewKeeping, EcologyPhase, EngagementQuantum, EngagementStop, FightCasualties, FightRate, Herd,
    HerdDensityMap, HerdPayoff, HerdRegistry, HerdTelemetry, HerdTelemetryEntry, HuntCrew,
    HuntCrewCurveInputs, HuntCrewTake, HuntDraw, HuntEngagement, HuntFight, HuntProjection,
    HuntTakeBound, HuntingParty, OutcomeKills, PartyResolution, PenSiteScene, PreyDatum,
    ProjectedHuntTurn, ProjectionStart, QuarryFight, RetreatOutcome, RoamState,
    SourceYieldForecast, TakeRange, TakeReading, FODDERING_DISCOVERY_ID, FULLY_HERDED,
    HERDING_DISCOVERY_ID, MSY_BIOMASS_FRACTION, NO_DEATHS_TO_REPORT, NO_HANDS,
    NO_STANDING_COMMITMENT, NO_USEFUL_CREW, ONE_KEEPER_LOAD, PENNING_DISCOVERY_ID,
    SELECTIVE_BREEDING_DISCOVERY_ID,
};
pub use fauna_config::{
    load_fauna_config_from_env, Diet, EcologyConfig, FaunaConfig, FaunaConfigHandle,
    FaunaConfigMetadata, GrazeConfig, HuntYield, HuntYieldDef, MigratoryAbundanceConfig,
    ShoreRequirement, SizeClass, SpeciesDef, YieldAccounts, BUILTIN_FAUNA_CONFIG,
    NO_GRAZE_CAPACITY, NO_RETREAT,
};
pub use flora_config::{
    load_flora_config_from_env, FloraConfig, FloraConfigHandle, FloraConfigMetadata, FloraDef,
    FloraRole, FloraShare, YieldVector, BUILTIN_FLORA_CONFIG,
};
pub use food::{
    classify_food_module, classify_food_module_from_traits, FoodModule, FoodModuleTag,
    FoodSiteKind, DEFAULT_HARVEST_TRAVEL_TILES_PER_TURN, DEFAULT_HARVEST_WORK_TURNS,
};
pub use forage::{
    advance_cultivation, advance_forage_regrowth, commit_fodder_payoff, commit_material_payoff,
    commit_payoff, commit_yield_ratio, composition_for_rung, crew_take_room,
    crop_field_cost_multiplier, default_species_for_rung, field_cost_multiplier_at_share,
    forage_crew_needed, forage_per_worker_biomass, forage_provisions, forage_source_yield_preview,
    forage_take_overdraws, next_turns_stand, patch_build_legs, patch_build_rung, patch_build_verb,
    patch_carrying_capacity, patch_claims_keeping, patch_composition, patch_crew_keeping,
    patch_destination_capacity, patch_ecology, patch_field_cost_multiplier, patch_keeping_basis,
    patch_land_capacity, patch_material_yields, patch_material_yields_taking, patch_meter_rot,
    patch_provisions_per_biomass, patch_provisions_per_biomass_taking, patch_rung_already_built,
    patch_rung_span, patch_rung_work_done, patch_tender_loads, patch_unwinding_key,
    patch_upkeep_demand, patch_upkeep_shortfall, patch_upkeep_workers_needed, plant_rung_span,
    project_arrivals_forage, project_realized_forage, resolve_committed_species,
    resolve_take_selection, rung_material_yields, rung_payoff, rung_site_refusal,
    selected_biomass_share, spawn_initial_forage, species_is_legal_here, species_stands_in,
    tended_take_fodder, tile_flora_composition, tile_forage_capacity, tile_is_fresh_watered,
    wild_payoff, ForagePatch, ForageRegistry, PlantPayoff, SpeciesRate, SpeciesRefusal,
    CANNOT_CLIMB_RATIO, CULTIVATION_DISCOVERY_ID, NO_FORAGE_SEASON, NO_TENDER_LOAD,
    ONE_TENDER_LOAD, SEED_SELECTION_DISCOVERY_ID, WHOLE_BASKET, WORKED_LAND_DISCOVERY_ID,
};
pub use generations::{GenerationBias, GenerationId, GenerationProfile, GenerationRegistry};
pub use goodwill::{
    cargo_worth, good_worth, worth, GoodAmount, Goodwill, GoodwillEntry, GoodwillNews,
    GoodwillSource, KnownGoodwill, KnownGoodwillEntry,
};
pub use graze::{advance_graze_regrowth, spawn_initial_graze, GrazePatch, GrazeRegistry};
pub use great_discovery::{
    ConstellationRequirement, GreatDiscoveryCandidateEvent, GreatDiscoveryDefinition,
    GreatDiscoveryEffectEvent, GreatDiscoveryEffectKind, GreatDiscoveryFlag, GreatDiscoveryId,
    GreatDiscoveryLedger, GreatDiscoveryReadiness, GreatDiscoveryRecord, GreatDiscoveryRegistry,
    GreatDiscoveryResolvedEvent, GreatDiscoveryTelemetry, ObservationLedger,
};
pub use hydrology::{generate_hydrology, HydrologyState};
pub use knowledge_contact::{
    apply_contact_lessons, contact_lessons, learn_over_connections, trade_landings, ContactBand,
    ContactChannel, ContactLesson, ContactLessons, ContactWorld, KnowledgeContactError,
    TeachableLesson, TeachableLessons, TradeLanding,
};
pub use knowledge_contact_config::{
    load_knowledge_contact_config_from_env, ContactChannelRates, KnowledgeContactConfig,
    KnowledgeContactConfigError, KnowledgeContactConfigHandle, KnowledgeContactConfigMetadata,
    BUILTIN_KNOWLEDGE_CONTACT_CONFIG,
};
pub use lineage::{
    breeding_ceiling, effective_breeding_ceiling, shared_breeding_ceiling, tie_joined_groups,
    BreedingMember, BreedingPeople, BreedingReading, FoundingLines, FreeBreedingPeoples, LineId,
    TieJoinedGroup, MIN_BAND_LINES, NO_INBREEDING_CEILING,
};
pub use personalities::{
    load_personalities_from_env, PersonalitiesError, Personality, PersonalityCatalog,
    PersonalityCatalogHandle, PersonalityCatalogMetadata, BUILTIN_PERSONALITIES, PERSONALITY_SALT,
};
pub use trade_history::{
    record_trade, TradeEnd, TradeFacts, TradeHistory, TradeOutcome, TradeRecord,
};
pub use treaties::{
    credit_treaty_goodwill, dissolve_extinct_peoples, end_treaty, match_bundles, Bundle,
    BundleMatch, EnvoyOutcome, Treaties, Treaty, TreatyTerms, NO_TREATIES,
};
// The drainage-network measurement instrument (consumed by the `#[ignore]`d census test).
pub use band_improvement::{
    advance_band_improvements, band_rung_cost, band_rung_span, band_standing_at, is_band_branch,
    BandImprovement, BandImprovementKey, BandImprovementRegistry, BAND_KEEPING_LOAD,
    NO_HANDS_ON_THE_BAND_IMPROVEMENT,
};
pub use extraction::{
    advance_deposits, deposit_at_risk_rung, deposit_keeper_loads, deposit_keeping_basis,
    deposit_measure, deposit_meter_rot, deposit_rungs_in_climb_order, deposit_upkeep_demand,
    renew_deposit, DepositRegistry, DepositSource, DEPOSIT_EMPTY,
};
pub use extraction_config::{
    load_extraction_config_from_env, DepositDef, DepositTerrain, ExtractionConfig,
    ExtractionConfigError, ExtractionConfigHandle, ExtractionConfigMetadata,
    BUILTIN_EXTRACTION_CONFIG, NEVER_RENEWS, NO_DEPOSIT,
};
pub use hydrology::{debug_drainage_census, DrainageCensus};
pub use influencers::{
    tick_influencers, InfluencerBalanceConfig, InfluencerConfigHandle, InfluencerCultureResonance,
    InfluencerImpacts, InfluentialId, InfluentialRoster, SupportChannel, BUILTIN_INFLUENCER_CONFIG,
};
pub use intensification::{
    activity_work, build_fraction, build_turns_estimate, build_turns_remaining,
    build_work_per_worker_turn, distribute_upkeep_pool, gear_work_supply, interpolate,
    knowledge_title_from_id, knows, knows_lesson, learn_multiplier,
    load_intensification_ladder_from_env, pool_work_supply, rung_work_done, upkeep_shortfall,
    upkeep_shortfall_fraction, BuildGate, BuildTurns, CostScale, LadderConfig, LadderConfigHandle,
    LadderConfigMetadata, RungBehavior, RungBuild, RungDef, RungExtractionPayoff, RungMeterDecay,
    RungMovement, RungPartialCredit, RungSiteRequirement, RungStanding, RungTrait, RungTraits,
    RungUpkeep, SiteRefusal, SourceKind, UpkeepFundMode, UpkeepScale,
    BUILTIN_INTENSIFICATION_LADDER, FABRICATED_BUILD_COST, FULLY_SUPPLIED, NOTHING_IN_FLIGHT,
    NO_BUILD_GEAR, NO_CREW_ON_THIS_ACTIVITY, NO_DEPOSIT_FLOOR, NO_DEPOSIT_REACHED, NO_RUNG_CREDIT,
    NO_RUNG_WORK_BANKED, NO_UPKEEP_DECAY, NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT,
    PRACTICE_AT_THE_PLAIN_RATE, REGROWTH_UNCHANGED, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
    SITE_ACCEPTED, WHOLE_DEPOSIT_REACHED, WHOLLY_UNSUPPLIED,
};
pub use knowledge_ledger::{
    CounterIntelSweepEvent, EspionageProbeEvent, KnowledgeCountermeasure, KnowledgeLedger,
    KnowledgeLedgerConfig, KnowledgeLedgerConfigHandle, KnowledgeLedgerEntry, KnowledgeModifier,
    KnowledgeTimelineEvent, BUILTIN_KNOWLEDGE_LEDGER_CONFIG,
};
pub use labor_config::{
    load_labor_config_from_env, LaborConfig, LaborConfigHandle, LaborConfigMetadata,
    BUILTIN_LABOR_CONFIG, NO_FORAGE_CAPACITY,
};
pub use ladder_ids::{shipped, BranchId, RungId, Sym, VerbId};
pub use leader_names::{
    load_leader_names_from_env, LeaderNameCatalog, LeaderNameCatalogHandle,
    LeaderNameCatalogMetadata, LeaderNamesError, BUILTIN_LEADER_NAMES, LEADER_NAME_SALT,
};
pub use leadership::{
    advance_leadership, BandLeader, BandLeadership, LeaderKind, LEADER_KIND_CHIEF,
    LEADER_KIND_ELDER_COUNCIL, LEADER_KIND_NONE,
};
pub use leadership_config::{
    load_leadership_config_from_env, LeadershipConfig, LeadershipConfigHandle,
    LeadershipConfigMetadata, BUILTIN_LEADERSHIP_CONFIG,
};
pub use map_preset::{ErosionConfig, MapPreset, MapPresets, MapPresetsHandle, BUILTIN_MAP_PRESETS};
pub use mapgen::WorldGenSeed;
pub use materials_config::{
    credit_material_yield, load_materials_config_from_env, material_yield_totals, BandKey,
    CharacteristicBand, HandWorking, MaterialDef, MaterialPayoff, MaterialYieldDef,
    MaterialYieldError, MaterialsConfig, MaterialsConfigError, MaterialsConfigHandle,
    MaterialsConfigMetadata, BUILTIN_MATERIALS_CONFIG,
};
pub use mobility::{mobility_tick, HerdFollowSteps, MobilityEntry, MobilityScore};
pub use mobility_config::{
    load_mobility_config_from_env, MobilityConfig, MobilityConfigHandle, MobilityConfigMetadata,
};
pub use recipes_config::{
    load_recipes_config_from_env, CraftingTuning, RecipeDef, RecipeGrade, RecipeInput,
    RecipeOutput, RecipesConfig, RecipesConfigError, RecipesConfigHandle, RecipesConfigMetadata,
    BUILTIN_RECIPES_CONFIG,
};
pub use routes::Walker as RoadWalker;
pub use routes::{
    advance_roads, credit_route_lessons, first_built_rung, foreign_roads, free_floor_top_rung,
    max_route_reach_tiles, path_friction_multiplier, path_lesson_rung, path_reach_tiles,
    remoteness_multiplier, road_access_in_world, road_at_risk_rung, road_build_fraction,
    road_keeping_basis, road_keeping_range, road_measure, road_neglect_grace_remaining,
    road_reach_for, road_rung_span, road_upkeep_demand, road_upkeep_measure,
    road_upkeep_workers_needed, route_rungs_in_climb_order, rung_grants_sight, standing_link,
    trace_path, traffic_ceiling, ForeignRoads, LinkFloor, Road, RoadAccess, RoadKeeper,
    RoadRegistry, RoadRights, RouteJourney, RouteTrafficLog, TrafficSource, METER_FULL,
    NEAR_ENOUGH_TO_KEEP, NO_HERD_HAS_CROSSED, NO_REACH_HELD_OPEN, PAVING_DISCOVERY_ID,
    ROADBUILDING_DISCOVERY_ID,
};
pub use sedentarization::{
    sedentarization_tick, SedentarizationEntry, SedentarizationScore, SedentarizationStage,
};
pub use sedentarization_config::{
    load_sedentarization_config_from_env, SedentarizationConfig, SedentarizationConfigHandle,
    SedentarizationConfigMetadata,
};
pub use settlement_stage_config::{
    load_settlement_stage_config_from_env, resolve_settlement_stage, SettlementStageConfig,
    SettlementStageConfigHandle, SettlementStageConfigMetadata, SettlementStageDef,
    SettlementStageInputs, StageCriteria, BUILTIN_SETTLEMENT_STAGE_CONFIG,
};
pub use sites::{
    discover_sites, place_wondrous_sites, DiscoveredSiteRecord, DiscoveredSites, SiteTag,
};
pub use sites_config::{
    load_sites_config_from_env, DiscoveryReward, PlacementRuleCfg, SiteDef, SitesConfig,
    SitesConfigHandle, SitesConfigMetadata, BUILTIN_SITES_CONFIG,
};
pub use snapshot_overlays_config::{
    load_snapshot_overlays_config_from_env, CorruptionOverlayConfig, CultureOverlayConfig,
    MilitaryOverlayConfig, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    SnapshotOverlaysConfigMetadata, BUILTIN_SNAPSHOT_OVERLAYS_CONFIG,
};
pub use spoilage::rot_band_larders;
pub use start_profile::{
    resolve_active_profile, snapshot_profiles, ActiveStartProfile, CampaignLabel, FactionControl,
    InventoryEntry, OpeningLoadoutConfig, StartProfile, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartProfileKnowledgeTagsMetadata, StartProfileLookup,
    StartProfileOverrides, StartProfiles, StartProfilesHandle, StartProfilesMetadata,
    StartingUnitSpec,
};
pub use start_profile::{StartArchetype, StartArchetypeAssignment};
pub use starting_loadout::{
    allocation_load, apply_starting_loadout, fit_to_carry, order_load, split_default_outfit,
    FittedLoadout, KitAllocation, LoadoutRejection, LoadoutSupply, LoadoutWindow,
    MaterialAllocation, SplitDowry, StartingLoadout, OPENING_MATERIAL_READING,
};
pub use supply::{
    balance_supply_networks, free_pooling_reach_tiles, BandSupplyMembership, PoolingLink,
    SupplyNetworkMembership, WalkReach,
};
pub use supply_network_config::{
    load_supply_network_config_from_env, SupplyNetworkConfig, SupplyNetworkConfigHandle,
    SupplyNetworkConfigMetadata,
};
pub use tile_improvement::{
    advance_tile_improvements, is_tile_branch, tile_rung_cost, tile_rung_span, tile_standing_at,
    TileImprovement, TileImprovementKey, TileImprovementRegistry, NO_HANDS_ON_THE_IMPROVEMENT,
    TILE_KEEPING_LOAD,
};
pub use tribe::{
    advance_tribes, closeness, CloseInputs, Tribe, TribeId, TribeLeader, TribeLeaderKind,
    TribeRegistry, TribeStanding, MIN_TRIBE_BANDS,
};
pub use turn_pipeline_config::{
    load_turn_pipeline_config_from_env, PopulationPhaseConfig, PowerPhaseConfig,
    TurnPipelineConfig, TurnPipelineConfigHandle, TurnPipelineConfigMetadata,
    BUILTIN_TURN_PIPELINE_CONFIG,
};
pub use victory::{
    load_victory_config_from_env, VictoryConfigHandle, VictoryModeId, VictoryModeKind,
    VictoryModeState, VictoryResult, VictoryState,
};
pub use visibility::{
    FactionVisibilityMap, TileVisibility, ViewerFaction, VisibilityLedger, VisibilitySource,
    VisibilityState,
};
pub use visibility_config::{
    load_visibility_config_from_env, DecayConfig, ElevationConfig, LineOfSightConfig,
    SightRangeConfig, SizeSightBonusConfig, TerrainDetectionConfig, VisibilityConfig,
    VisibilityConfigHandle, VisibilityConfigMetadata, BUILTIN_VISIBILITY_CONFIG,
};
pub use visibility_systems::terrain_sight_modifier;
pub use wellbeing_config::{
    load_wellbeing_config_from_env, CultureConfig, DiscontentConfig, MigrationConfig,
    ProductivityConfig, WellbeingConfig, WellbeingConfigHandle, WellbeingConfigMetadata,
    BUILTIN_WELLBEING_CONFIG,
};
pub use work_party::{CaravanForecast, SourceTake, Walker, WorkParty};

pub use biome_palette::{BiomePalette, PALETTE_SEED_SALT};
pub use climate::{climate_band_for_temperature, ClimateBand};
pub use metrics::{FactionMetrics, SimulationMetrics};
pub use orders::{
    FactionId, FactionOrders, FactionRegistry, Order, SubmitError, SubmitOutcome, TurnQueue,
};
pub use power::{
    PowerDiscoveryEffects, PowerGridNodeTelemetry, PowerGridState, PowerIncident,
    PowerIncidentSeverity, PowerNodeId, PowerTopology,
};
pub use provinces::{ProvinceId, ProvinceMap};
pub use resources::{
    apply_port_base, apply_port_base_override, carry_runtime_owned_fields, carry_world_identity,
    load_simulation_config_for_new_world, port_base_override, BandIdAllocator, BandNameAllocator,
    CapabilityFlags, CommandEventEntry, CommandEventKind, CommandEventLog, CorruptionLedgers,
    CorruptionTelemetry, DiplomacyLeverage, DiscoveryProgressLedger, FactionBorderPolicies,
    FactionInventory, FoodSiteEntry, FoodSiteRegistry, FoodSiteWaterBiasReport, HydrologyOverrides,
    MapTopology, MoistureRaster, PendingCrisisSeeds, PendingCrisisSpawns, SentimentAxisBias,
    SimulationConfig, SimulationConfigMetadata, SimulationTick, StartLocation, TileRegistry,
    TradeDiffusionRecord, TradeTelemetry, WorldEpoch,
};
pub use scalar::{scalar_from_f32, scalar_one, scalar_zero, Scalar};
pub use seats::{
    ConnectionId, ConnectionIdAllocator, SeatClaimRefusal, SeatClaimant, SeatRegistry, SeatToken,
    SeatTurnGate, SeatTurnLimits, TurnWait,
};
pub use snapshot::{
    command_events_to_state, publish_baseline_snapshot, recapture_snapshot_in_place, FrameSink,
    SnapshotAudiences, SnapshotHistory, StoredSnapshot, NOT_FOOD_LIMITED_TURNS,
};
pub use systems::spawn_initial_world;
pub use systems::{
    advance_band_independence, advance_band_movement, advance_crafting, advance_culture_pressure,
    advance_culture_splits, advance_expeditions, advance_labor_allocation, advance_party_defection,
    advance_population_migration, advance_predator_raids, advance_tick,
    announce_bench_material_short, band_label, bench_material_rate, bench_tiers,
    bill_and_stock_roads, break_off_breeding_excess, bring_the_dropped_party_home,
    culture_strength, deliver_bench_output, denial_forecast, expedition_returned_event,
    fold_party_into_band, follow_hunted_herds, grow_faction_roster, hunt_per_worker_provisions,
    hunt_report_event, hunt_take, joining_pressure, output_multiplier, party_owes_a_report,
    prospective_keep_hands, publish_turn_transfers, settle_bands_roadwork, settle_scarce_tools,
    shed_breeding_excess, simulate_population, simulate_power, source_has_a_meter_at_risk,
    split_band_from_parent, split_band_from_parent_with, split_refusals, BenchTiers, BreakOffAsk,
    BreedingBreakOffs, CulturePressureLedger, CulturePressureReading, DenialForecast,
    DenialOutcome, HeartLedger, HeartReading, HuntOutcome, JoiningPhase, LineDivision, PartyGear,
    PartySightings, PoolToolPlan, PowerSimParams, RaidRoll, SplitBand, SplitRefusal, SplitRefusals,
    ToolClaimStage, TradeDiffusionEvent, POST_RECALLED,
};
pub use systems::{
    apply_biome_palette_clamp, apply_tag_budget_solver, bias_food_sites_toward_fresh_water,
    reconcile_coastal_shelf, reconcile_food_modules,
};
/// **The map's say in how many peoples it seats** — the per-archetype ceiling, the offered default,
/// and the clamp every path that takes archetype counts runs through. Exported because the boot
/// path, the `new_game` handler and the capacity query all ask the same functions.
pub use systems::{
    archetype_capacities, archetype_ceiling, faction_start_land_fraction, granted_archetype_counts,
    max_faction_starts, unattended_archetype_counts, ArchetypeCapacity,
};
pub use telling::{
    load_beat_catalog_from_env, load_beat_config_from_env, telling_tick, BeatCatalog,
    BeatCatalogHandle, BeatCatalogMetadata, BeatChoice, BeatConfig, BeatConfigHandle,
    BeatConfigMetadata, BeatDefinition, BeatLedger, BeatTier, ChoiceWrites, CompareOp, EdgeDir,
    ForkAnswerError, ForkResolution, Noun, NounField, PendingFork, Predicate, RenderedChoice,
    SignalSample, WardrobeEntry, TELLING_SEED_SALT,
};
pub use terrain::{
    biome_must_have, biome_niche, classify_terrain, terrain_definition, terrain_for_position,
    BathymetryContext, BiomeNiche, MovementProfile, TerrainDefinition, TerrainResourceBias,
};

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum TurnStage {
    Influence,
    Logistics,
    Knowledge,
    GreatDiscovery,
    Population,
    Visibility,
    Crisis,
    /// The Telling's beat engine. Between `Crisis` and `Finalize` so it sees population, fauna,
    /// sedentarization and crisis output, and lands before `Snapshot` so a beat reaches the client
    /// the same turn it fires.
    Telling,
    Finalize,
    Victory,
    Snapshot,
}

/// Construct a Bevy [`App`] configured with the Shadow-Scale turn pipeline.
pub fn build_headless_app() -> App {
    let mut app = App::new();

    let (mut config, config_metadata) = resources::load_simulation_config_from_env();
    let (map_presets, map_presets_metadata) = load_map_presets_from_env();
    let victory_config = load_victory_config_from_env();
    let preset_count = map_presets.len();
    if let Some(path) = map_presets_metadata.path() {
        tracing::debug!(
            target: "shadow_scale::mapgen",
            presets = preset_count,
            path = %path.display(),
            "map_presets.metadata.available"
        );
    } else {
        tracing::debug!(
            target: "shadow_scale::mapgen",
            presets = preset_count,
            "map_presets.metadata.builtin"
        );
    }
    let (start_profiles, start_profiles_metadata) = load_start_profiles_from_env();
    let start_profiles_handle = StartProfilesHandle::new(start_profiles.clone());
    let (knowledge_tags, knowledge_tags_metadata) = load_start_profile_knowledge_tags_from_env();
    let knowledge_tags_handle = StartProfileKnowledgeTagsHandle::new(knowledge_tags.clone());

    let profile_id = config.start_profile_id.clone();
    let (active_profile, used_fallback) =
        start_profile::resolve_active_profile(&start_profiles_handle, &profile_id);

    config.start_profile_overrides =
        start_profile::StartProfileOverrides::from_profile(&active_profile);

    if used_fallback {
        tracing::warn!(
            target: "shadow_scale::campaign",
            requested = %profile_id,
            fallback = %active_profile.id,
            "start_profiles.lookup.fallback"
        );
    }

    let campaign_label = CampaignLabel::from_profile(&active_profile);
    tracing::info!(
        target: "shadow_scale::campaign",
        profile = %active_profile.id,
        title = campaign_label.title.text_as_str().unwrap_or(""),
        title_loc_key = campaign_label.title.loc_key().unwrap_or(""),
        subtitle = campaign_label.subtitle.text_as_str().unwrap_or(""),
        subtitle_loc_key = campaign_label.subtitle.loc_key().unwrap_or(""),
        fallback = used_fallback,
        "campaign.label.active"
    );

    let active_profile_resource = ActiveStartProfile::new(active_profile.clone());
    let profile_lookup = StartProfileLookup::new(active_profile.id.clone());

    // **Who plays this world is a COUNT OF RIVALS PER ARCHETYPE, not a profile declaration**: one
    // human — the player — plus however many AI factions of each start archetype were asked for. A
    // world built here was asked by nobody: it is the UNATTENDED roster, so an archetype with no
    // `default_count` pin means no rivals rather than the map-scaled count a New Game screen is
    // offered (`unattended_archetype_counts` vs `archetype_capacities`). A `new_game` re-seeds this
    // from the counts the player picked. The counts are clamped to what the grid can seat before the registry is built, so no path can
    // register a faction worldgen has nowhere to put; the turn queue then awaits exactly the roster
    // the registry was seeded with.
    let archetype_counts = granted_archetype_counts(
        &unattended_archetype_counts(&config.start_profile_overrides.archetypes),
        &config.start_profile_overrides.archetypes,
        config.grid_size,
        faction_start_land_fraction(&map_presets, &config.map_preset_id),
    );
    config.start_archetype_assignment = StartArchetypeAssignment::from_counts(&archetype_counts);
    let faction_registry = orders::FactionRegistry::with_ai_factions(archetype_counts.iter().sum());
    let turn_queue = orders::TurnQueue::new(faction_registry.factions().to_vec());
    // Depth is decided in ONE place — `snapshot::PUBLICATION_RING_DEPTH`, which
    // `capture_snapshot` no longer has to re-assert every turn.
    let snapshot_history = SnapshotHistory::with_capacity(snapshot::PUBLICATION_RING_DEPTH);
    let generation_registry = GenerationRegistry::with_seed(0xC0FEBABE, 6);
    let influencer_config = Arc::new(
        InfluencerBalanceConfig::from_json_str(BUILTIN_INFLUENCER_CONFIG)
            .expect("influencer config should parse"),
    );
    let influencer_roster =
        InfluentialRoster::with_seed(0xA51C_E55E, &generation_registry, influencer_config.clone());
    let influencer_config_handle = InfluencerConfigHandle::new(influencer_config);
    let knowledge_config = Arc::new(
        KnowledgeLedgerConfig::from_json_str(BUILTIN_KNOWLEDGE_LEDGER_CONFIG)
            .expect("knowledge ledger config should parse"),
    );
    let knowledge_ledger = KnowledgeLedger::with_config(knowledge_config.clone());
    let knowledge_config_handle = KnowledgeLedgerConfigHandle::new(knowledge_config);
    let culture_corruption_config = Arc::new(
        CultureCorruptionConfig::from_json_str(BUILTIN_CULTURE_CORRUPTION_CONFIG)
            .expect("culture corruption config should parse"),
    );
    let culture_manager =
        CultureManager::from_config(culture_corruption_config.culture().propagation());
    let culture_corruption_handle = CultureCorruptionConfigHandle::new(culture_corruption_config);
    let (turn_pipeline_config, turn_pipeline_metadata) = load_turn_pipeline_config_from_env();
    let turn_pipeline_handle = TurnPipelineConfigHandle::new(turn_pipeline_config.clone());
    let (snapshot_overlays_config, snapshot_overlays_metadata) =
        load_snapshot_overlays_config_from_env();
    let snapshot_overlays_handle = SnapshotOverlaysConfigHandle::new(snapshot_overlays_config);
    let (crisis_archetypes, crisis_archetypes_metadata) = load_crisis_archetypes_from_env();
    let crisis_archetypes_handle = CrisisArchetypeCatalogHandle::new(crisis_archetypes.clone());
    let (crisis_modifiers, crisis_modifiers_metadata) = load_crisis_modifiers_from_env();
    let crisis_modifiers_handle = CrisisModifierCatalogHandle::new(crisis_modifiers.clone());
    let (crisis_telemetry_config, crisis_telemetry_metadata) =
        load_crisis_telemetry_config_from_env();
    let crisis_telemetry_handle = CrisisTelemetryConfigHandle::new(crisis_telemetry_config.clone());
    let crisis_telemetry_resource = CrisisTelemetry::from_config(crisis_telemetry_config.as_ref());
    let (visibility_config, visibility_metadata) =
        visibility_config::load_visibility_config_from_env();
    let visibility_handle = visibility_config::VisibilityConfigHandle::new(visibility_config);
    // The connection primitive's three clocks. It loads beside visibility because contact is found
    // inside that sweep (`connections::ContactsThisTurn`).
    let (connections_config, connections_metadata) =
        connections_config::load_connections_config_from_env();
    let connections_handle = connections_config::ConnectionsConfigHandle::new(connections_config);
    // The knowledge rider's channel rates — a knowledge config, never `connections_config.json`,
    // which has no rider vocabulary.
    let (knowledge_contact_config, knowledge_contact_metadata) =
        knowledge_contact_config::load_knowledge_contact_config_from_env();
    let knowledge_contact_handle =
        knowledge_contact_config::KnowledgeContactConfigHandle::new(knowledge_contact_config);
    // Early diplomacy: how long a pending dealing waits, and the goodwill / road / AI levers the
    // later slices read. No hot reload — it loads here and on every world rebuild.
    let (diplomacy_config, diplomacy_metadata) = diplomacy_config::load_diplomacy_config_from_env();
    let diplomacy_handle = diplomacy_config::DiplomacyConfigHandle::new(diplomacy_config);
    // Belief on a place — what each source adds to a tile's stock (`belief::BeliefRegistry`).
    let (belief_config, belief_metadata) = belief_config::load_belief_config_from_env();
    let belief_handle = belief_config::BeliefConfigHandle::new(belief_config);
    // The pool a band's name is drawn from. Content rather than tuning, but it loads on the same
    // boot seam as everything else so an operator can point a campaign at a different name list.
    let (band_names_catalog, band_names_metadata) = band_names::load_band_names_from_env();
    let band_names_handle = band_names::BandNameCatalogHandle::new(band_names_catalog);
    let (faction_names_catalog, faction_names_metadata) =
        faction_names::load_faction_names_from_env();
    let faction_names_handle = faction_names::FactionNameCatalogHandle::new(faction_names_catalog);
    // The lean a starting faction is born with. Content on the same boot seam as the name pools.
    let (personalities_catalog, personalities_metadata) =
        personalities::load_personalities_from_env();
    let personalities_handle = personalities::PersonalityCatalogHandle::new(personalities_catalog);
    // **The materials table loads FIRST of the three**, because both food webs' yield edges are
    // reconciled against it: a species (plant or animal) naming a material that does not exist, or
    // stating a reading on an axis that material does not declare, is a boot panic rather than a
    // source that silently yields nothing (`docs/plan_crafting_and_materials.md` §2).
    let (materials_config, materials_metadata) = materials_config::load_materials_config_from_env();
    let materials_handle = materials_config::MaterialsConfigHandle::new(materials_config.clone());
    // **The deposits table loads beside the two food webs' rosters and is reconciled against the
    // same materials table** (`docs/plan_extraction.md` §2). A deposit of a material that does not
    // exist — or one rated on an axis that material does not declare — would otherwise parse,
    // validate, and then pay a band a pile nothing can hold or merge.
    let (extraction_config, extraction_metadata) =
        extraction_config::load_extraction_config_from_env();
    if let Err(err) = extraction_config.validate_against_materials(&materials_config) {
        panic!("extraction config does not reconcile with the materials table: {err}");
    }
    let extraction_handle = extraction_config::ExtractionConfigHandle::new(extraction_config);
    let (fauna_config, fauna_metadata) =
        fauna_config::load_fauna_config_from_env(&materials_config);
    let fauna_handle = fauna_config::FaunaConfigHandle::new(fauna_config);
    let (labor_config, labor_metadata) = labor_config::load_labor_config_from_env();
    // The flora roster is validated AGAINST the human food web's own capacity table — every
    // food-bearing biome must be named, and no named plant may claim barren ground
    // (`FloraConfig::validate_against_forage`) — and against the materials table beside it. Both
    // tables are passed in rather than re-read so each keeps exactly one copy.
    let (flora_config, flora_metadata) = flora_config::load_flora_config_from_env(
        &labor_config.forage.capacity_by_biome,
        &materials_config,
    );
    let flora_handle = flora_config::FloraConfigHandle::new(flora_config);
    let labor_handle = labor_config::LaborConfigHandle::new(labor_config);
    let (ladder_config, ladder_metadata) = intensification::load_intensification_ladder_from_env();
    // **The ladder's MATERIAL half is reconciled against the materials table**, the same
    // `UnknownItem` debt the equipment roster and the two food webs' yield edges pay: a rung whose
    // pile or upkeep names a material that does not exist would otherwise parse, validate, and then
    // be raised and held for free for ever with no fault reported anywhere
    // (`docs/plan_standing_upkeep.md` §2.7).
    if let Err(err) = ladder_config.validate_against_materials(&materials_config) {
        panic!("intensification ladder does not reconcile with the materials table: {err}");
    }
    // **Every deposit in the extraction table is worked by a `deposit` branch** — the cross-file half
    // of the ladder's kind-level rule. A deposit naming a food web's branch (or a branch the ladder
    // does not declare) would otherwise parse on both sides and then be raised by a ladder whose
    // rungs are about something else.
    if let Err(err) = ladder_config.validate_against_extraction(&extraction_handle.get()) {
        panic!("intensification ladder does not reconcile with the extraction table: {err}");
    }
    // **Every knowledge name the ladder uses is a tag in the table that loaded**, mapped to the id
    // the ladder resolves it to - the ladder reads names, the tag table owns what they mean.
    if let Err(err) = ladder_config.validate_against_knowledge_tags(&knowledge_tags) {
        panic!("intensification ladder does not reconcile with the knowledge tag table: {err}");
    }
    // **Every knowledge tag contact can teach is priced** (`docs/plan_contact_and_logistics.md`
    // §Settled by #531): the rider can teach any tag in `start_profile_knowledge_tags.json`, so a
    // tag the ladder's `lesson_costs` does not price is a boot panic rather than a lesson paced off a
    // default nobody chose.
    if let Err(err) =
        knowledge_contact::TeachableLessons::resolve(&knowledge_tags, &ladder_config.knowledge)
    {
        panic!("knowledge tags do not reconcile with the intensification ladder: {err}");
    }
    let ladder_for_reconcile = ladder_config.clone();
    let ladder_handle = intensification::LadderConfigHandle::new(ladder_config);
    let (sedentarization_config, sedentarization_metadata) =
        sedentarization_config::load_sedentarization_config_from_env();
    let sedentarization_handle =
        sedentarization_config::SedentarizationConfigHandle::new(sedentarization_config);
    let (leadership_config, leadership_metadata) =
        leadership_config::load_leadership_config_from_env();
    let leadership_handle = leadership_config::LeadershipConfigHandle::new(leadership_config);
    let (leader_names_catalog, leader_names_metadata) = leader_names::load_leader_names_from_env();
    let leader_names_handle = leader_names::LeaderNameCatalogHandle::new(leader_names_catalog);
    let (mobility_config, mobility_metadata) = mobility_config::load_mobility_config_from_env();
    let mobility_handle = mobility_config::MobilityConfigHandle::new(mobility_config);
    let (settlement_stage_config, settlement_stage_metadata) =
        settlement_stage_config::load_settlement_stage_config_from_env();
    let settlement_stage_handle =
        settlement_stage_config::SettlementStageConfigHandle::new(settlement_stage_config);
    let (beat_config, beat_config_metadata) = telling::load_beat_config_from_env();
    let (beat_catalog, beat_catalog_metadata) = telling::load_beat_catalog_from_env(&beat_config);
    let beat_config_handle = telling::BeatConfigHandle::new(beat_config);
    let beat_catalog_handle = telling::BeatCatalogHandle::new(beat_catalog);
    let (sites_config, sites_metadata) = sites_config::load_sites_config_from_env();
    let sites_handle = sites_config::SitesConfigHandle::new(sites_config);
    let (expedition_config, expedition_metadata) =
        expedition_config::load_expedition_config_from_env();
    let expedition_handle = expedition_config::ExpeditionConfigHandle::new(expedition_config);
    let (combat_config, combat_metadata) = combat_config::load_combat_config_from_env();
    let combat_handle = combat_config::CombatConfigHandle::new(combat_config);
    let (creatures_config, creatures_metadata) = creatures_config::load_creatures_config_from_env();
    let creatures_handle = creatures_config::CreaturesConfigHandle::new(creatures_config);
    let (equipment_config, equipment_metadata) = equipment_config::load_equipment_config_from_env();
    // **The item table's `bounds_material` is reconciled against the materials table**, the same
    // `UnknownItem` debt the two food webs' yield edges pay: a tool bounding `hyde` would parse,
    // validate, and then be the bench tool for nothing.
    if let Err(err) = equipment_config.validate_against_materials(&materials_config) {
        panic!("equipment config does not reconcile with the materials table: {err}");
    }
    // **Every branch and rung a tool is bound to exists in the ladder**, and a `deposit_take` tool
    // is bound to a `deposit` branch: a tool bound to a rung nobody defines would take nothing off
    // any build, for ever, with no fault reported.
    if let Err(err) = equipment_config.validate_against_ladder(&ladder_for_reconcile) {
        panic!("equipment config does not reconcile with the intensification ladder: {err}");
    }
    // **Every species' ceiling names a rung the ladder defines** - a flora `cultivation_ceiling` or a
    // fauna `husbandry_ceiling` is a rung id, and one nobody defines (or one on the other web's
    // branch) would parse and then be a ceiling no rung is ever compared with.
    if let Err(err) = flora_handle
        .get()
        .validate_against_ladder(&ladder_for_reconcile)
    {
        panic!("flora config does not reconcile with the intensification ladder: {err}");
    }
    if let Err(err) = fauna_handle
        .get()
        .validate_against_ladder(&ladder_for_reconcile)
    {
        panic!("fauna config does not reconcile with the intensification ladder: {err}");
    }
    // **The opening loadout's pick list is reconciled against the materials table**, the same
    // `UnknownItem` debt every other roster pays: a profile offering `hyde` would parse, validate,
    // and be a row the player can spend points on that deposits nothing. Checked here rather than at
    // load because the profiles are read before the materials table exists.
    if let Err(err) = start_profiles.validate_against_materials(&materials_config) {
        panic!("start profiles do not reconcile with the materials table: {err}");
    }
    // **And the kit half against the roster**, for the same debt: a `kit_defaults` key naming a kit
    // that does not exist — or one that carries nothing — would draw the picker opening on a row the
    // `set_starting_loadout` handler then refuses, which reports nothing to anybody.
    if let Err(err) = start_profiles.validate_against_equipment(&equipment_config) {
        panic!("start profiles do not reconcile with the equipment roster: {err}");
    }
    let equipment_handle = equipment_config::EquipmentConfigHandle::new(equipment_config.clone());
    // **The recipe book is reconciled against BOTH tables**, here and only here, because this is the
    // one place all three configs are in scope at once — a recipe naming a material or an item that
    // does not exist would otherwise parse, validate, and make nothing forever.
    let (recipes_config, recipes_metadata) = recipes_config::load_recipes_config_from_env();
    if let Err(err) = recipes_config.validate_against(&materials_config, &equipment_config) {
        panic!("recipes config does not reconcile with the materials and equipment tables: {err}");
    }
    let recipes_handle = recipes_config::RecipesConfigHandle::new(recipes_config);
    let (demographics_config, demographics_metadata) =
        demographics_config::load_demographics_config_from_env();
    // **Every species names a configured keeping class** (#706) — the two food webs' rosters are
    // reconciled against the class table here, the one place all three are in scope. A misspelt
    // class would otherwise land a species' food in a class nothing eats in order or rots.
    if let Err(err) = fauna_handle
        .get()
        .validate_keeping(&demographics_config.keeping)
    {
        panic!("fauna config does not reconcile with the keeping classes: {err}");
    }
    if let Err(err) = flora_handle
        .get()
        .validate_keeping(&demographics_config.keeping)
    {
        panic!("flora config does not reconcile with the keeping classes: {err}");
    }
    // **Every preservation names a rung the ladder defines** (#708) - the keeping table's
    // cross-config half, run where the ladder and the class table are both in scope. A rung nobody
    // defines would otherwise parse, validate and preserve nothing for ever.
    if let Err(err) = demographics_config
        .keeping
        .validate_preservation(&ladder_for_reconcile)
    {
        panic!("keeping preservation does not reconcile with the intensification ladder: {err}");
    }
    // **Every fixed store names a tile rung the ladder defines, and the preservation table states
    // what it holds** (#709) - the other half of the same reconciliation.
    if let Err(err) = demographics_config
        .keeping
        .validate_fixed_stores(&ladder_for_reconcile)
    {
        panic!("keeping fixed_stores does not reconcile with the intensification ladder: {err}");
    }
    // **Every improvement that adds belief names a tile rung the ladder defines** (#692) - the belief
    // config's cross-config half, run where the ladder and the belief config are both in scope.
    if let Err(err) = belief_handle
        .get()
        .validate_improvement_belief(&ladder_for_reconcile)
    {
        panic!("belief config does not reconcile with the intensification ladder: {err}");
    }
    let demographics_handle =
        demographics_config::DemographicsConfigHandle::new(demographics_config);
    let (supply_network_config, supply_network_metadata) =
        supply_network_config::load_supply_network_config_from_env();
    let supply_network_handle =
        supply_network_config::SupplyNetworkConfigHandle::new(supply_network_config);
    let (wellbeing_config, wellbeing_metadata) = wellbeing_config::load_wellbeing_config_from_env();
    let wellbeing_handle = wellbeing_config::WellbeingConfigHandle::new(wellbeing_config);
    let culture_effects = CultureEffectsCache::default();
    let espionage_catalog =
        espionage::EspionageCatalog::load_builtin().expect("espionage catalog should parse");
    let mut espionage_roster = espionage::EspionageRoster::default();
    espionage_roster.seed_from_catalog(faction_registry.factions(), &espionage_catalog);
    let counter_intel_budgets = espionage::CounterIntelBudgets::new(
        faction_registry.factions(),
        espionage_catalog.config().counter_intel_budget(),
    );
    let security_policies = espionage::FactionSecurityPolicies::new(
        faction_registry.factions(),
        espionage::SecurityPolicy::Standard,
    );
    let border_policies = resources::FactionBorderPolicies::new(faction_registry.factions());

    // Read before `config` is moved into the world: the log's turn window is a config lever, and
    // `CommandEventLog::default()` only knows the builtin default.
    let command_event_log =
        CommandEventLog::with_retention_turns(config.command_events_retention_turns);

    app.insert_resource(config)
        .insert_resource(config_metadata)
        .insert_resource(MapPresetsHandle::new(map_presets.clone()))
        .insert_resource(map_presets_metadata)
        .insert_resource(VictoryConfigHandle::new(victory_config.clone()))
        .insert_resource(VictoryState::new(victory_config.continue_after_win))
        .insert_resource(start_profiles_handle)
        .insert_resource(start_profiles_metadata)
        .insert_resource(knowledge_tags_handle)
        .insert_resource(knowledge_tags_metadata)
        // Snapshotted AFTER every `load_*_from_env` above, so it describes the tuning this world
        // actually booted on. Staging an override later moves the process-global registry — what the
        // NEXT `new_game` will boot on — and deliberately leaves this resource alone.
        .insert_resource(config_fingerprint::current_config_fingerprint())
        .insert_resource(active_profile_resource)
        .insert_resource(profile_lookup)
        .insert_resource(campaign_label)
        .insert_resource(resources::StartLocation::default())
        .insert_resource(hydrology::HydrologyState::default())
        .insert_resource(PowerGridState::default())
        .insert_resource(PowerTopology::default())
        .insert_resource(SimulationTick::default())
        // Default epoch (0) so `capture_snapshot` always finds the resource. The server overwrites
        // it with the live counter on every world (re)build; the idle boot app never captures.
        .insert_resource(WorldEpoch::default())
        .insert_resource(BandIdAllocator::default())
        // Empty at boot: every counter is minted by worldgen, which inserts its own filled copy the
        // way it does for `BandIdAllocator`. Present here so a hand-rolled test `World` and the
        // idle boot app always find the resource.
        .insert_resource(BandNameAllocator::default())
        .insert_resource(band_names_handle)
        .insert_resource(band_names_metadata)
        .insert_resource(faction_names_handle)
        .insert_resource(faction_names_metadata)
        .insert_resource(personalities_handle)
        .insert_resource(personalities_metadata)
        // Empty until worldgen mints it from the roster and the final map seed (Startup).
        .insert_resource(faction_names::FactionNames::default())
        .insert_resource(sim_state::Replaying::default())
        .insert_resource(CapabilityFlags::default())
        .insert_resource(SimulationMetrics::default())
        .insert_resource(crisis_telemetry_resource)
        .insert_resource(SentimentAxisBias::default())
        .insert_resource(knowledge_config_handle)
        .insert_resource(knowledge_ledger)
        .insert_resource(culture_corruption_handle)
        .insert_resource(snapshot_overlays_handle)
        .insert_resource(crisis_archetypes_handle)
        .insert_resource(crisis_modifiers_handle)
        .insert_resource(crisis_telemetry_handle)
        .insert_resource(ActiveCrisisLedger::default())
        .insert_resource(CrisisOverlayCache::default())
        .insert_resource(visibility_handle)
        .insert_resource(visibility_metadata)
        .insert_resource(connections_handle)
        .insert_resource(connections_metadata)
        .insert_resource(knowledge_contact_handle)
        .insert_resource(knowledge_contact_metadata)
        .insert_resource(diplomacy_handle)
        .insert_resource(diplomacy_metadata)
        .insert_resource(dealings::PendingDealings::default())
        .insert_resource(goodwill::Goodwill::default())
        .insert_resource(treaties::Treaties::default())
        .insert_resource(trade_history::TradeHistory::default())
        .insert_resource(goodwill::KnownGoodwill::default())
        .insert_resource(belief_handle)
        .insert_resource(belief_metadata)
        .insert_resource(materials_handle)
        .insert_resource(extraction_handle)
        .insert_resource(extraction_metadata)
        // **The live workings on the two deposit branches.** Empty at boot and filled lazily — a
        // deposit nobody has worked stands at exactly its tile's capacity, which is a pure function
        // of the tile, so there is nothing to seed (`extraction::DepositRegistry::open`).
        .insert_resource(extraction::DepositRegistry::default())
        .insert_resource(materials_metadata)
        .insert_resource(recipes_handle)
        .insert_resource(recipes_metadata)
        .insert_resource(fauna_handle)
        .insert_resource(fauna_metadata)
        .insert_resource(flora_handle)
        .insert_resource(flora_metadata)
        .insert_resource(snapshot::FloraQuoteCache::default())
        .insert_resource(labor_handle)
        .insert_resource(labor_metadata)
        .insert_resource(ladder_handle)
        .insert_resource(ladder_metadata)
        .insert_resource(sedentarization_handle)
        .insert_resource(sedentarization_metadata)
        .insert_resource(sedentarization::SedentarizationScore::default())
        .insert_resource(leadership_handle)
        .insert_resource(leadership_metadata)
        .insert_resource(leader_names_handle)
        .insert_resource(leader_names_metadata)
        .insert_resource(mobility_handle)
        .insert_resource(mobility_metadata)
        .insert_resource(mobility::MobilityScore::default())
        .insert_resource(mobility::HerdFollowSteps::default())
        .insert_resource(beat_config_handle)
        .insert_resource(beat_config_metadata)
        .insert_resource(beat_catalog_handle)
        .insert_resource(beat_catalog_metadata)
        .insert_resource(telling::BeatLedger::default())
        .insert_resource(settlement_stage_handle)
        .insert_resource(settlement_stage_metadata)
        .insert_resource(sites_handle)
        .insert_resource(sites_metadata)
        .insert_resource(sites::DiscoveredSites::default())
        .insert_resource(expedition_handle)
        .insert_resource(expedition_metadata)
        .insert_resource(combat_handle)
        .insert_resource(combat_metadata)
        .insert_resource(creatures_handle)
        .insert_resource(creatures_metadata)
        .insert_resource(equipment_handle)
        .insert_resource(equipment_metadata)
        .insert_resource(demographics_handle)
        .insert_resource(demographics_metadata)
        .insert_resource(supply_network_handle)
        .insert_resource(supply_network_metadata)
        .insert_resource(wellbeing_handle)
        .insert_resource(wellbeing_metadata)
        .insert_resource(supply::SupplyNetworkMembership::default())
        .insert_resource(visibility::VisibilityLedger::default())
        .insert_resource(visibility::VisibilitySweepTracker::default())
        .insert_resource(connections::ConnectionLedger::default())
        .insert_resource(connections::GatheringLedger::default())
        .insert_resource(tribe::TribeRegistry::default())
        .insert_resource(systems::HeartLedger::default())
        .insert_resource(systems::CulturePressureLedger::default())
        .insert_resource(systems::BreedingBreakOffs::default())
        .insert_resource(culture::CultureSplitQueue::default())
        .insert_resource(lineage::FreeBreedingPeoples::default())
        // **The roads and this turn's traffic** (`docs/plan_standing_upkeep.md` §4.13). The registry
        // is world state; the traffic log is a hand-off from the three things that move —
        // `balance_supply_networks` (which pairs pooled), `advance_band_movement` (who marched) and
        // `advance_herds` (which migratory herds walked their corridor) — to `routes::advance_roads`,
        // which spends them.
        .insert_resource(routes::RoadRegistry::default())
        // **The improvements standing on the tile-kind branches** - the road's shape for a thing that
        // is not a road. World state, empty until a band places one.
        .insert_resource(tile_improvement::TileImprovementRegistry::default())
        .insert_resource(routes::RouteTrafficLog::default())
        .insert_resource(connections::ContactsThisTurn::default())
        .insert_resource(visibility::ViewerFaction::default())
        .insert_resource(turn_pipeline_handle)
        .insert_resource(turn_pipeline_metadata)
        .insert_resource(snapshot_overlays_metadata)
        .insert_resource(crisis_archetypes_metadata)
        .insert_resource(crisis_modifiers_metadata)
        .insert_resource(crisis_telemetry_metadata)
        .insert_resource(CorruptionLedgers::default())
        .insert_resource(CorruptionTelemetry::default())
        .insert_resource(DiplomacyLeverage::default())
        .insert_resource(FactionInventory::default())
        .insert_resource(HerdRegistry::default())
        .insert_resource(HerdTelemetry::default())
        .insert_resource(HerdDensityMap::default())
        .insert_resource(ForageRegistry::default())
        .insert_resource(GrazeRegistry::default())
        // **Belief on every place** (`docs/plan_civilization_steps.md` §"Belief is a property of a
        // place"). World state that only ever grows — an abandoned place keeps its dead.
        .insert_resource(belief::BeliefRegistry::default())
        .insert_resource(command_event_log)
        .insert_resource(FoodSiteRegistry::default())
        .init_resource::<FoodSiteWaterBiasReport>()
        // The opening outfitting window. `Default` is CLOSED with no budget, which is the right
        // reading for a world that never ran worldgen — a load restores the saved window, and a
        // fresh world has `stamp_starting_loadout` open it.
        .init_resource::<starting_loadout::StartingLoadout>()
        .init_resource::<staffing::WorkKitWants>()
        .insert_resource(snapshot_history)
        .insert_resource(snapshot::SnapshotCaptureMode::default())
        // **Who the world publishes a frame to.** Empty at boot: a world with no claimed seat
        // publishes the single `ViewerFaction` view, which is every test and every single-player
        // session before its client claims. The server rewrites it from `SeatRegistry`.
        .insert_resource(snapshot::SnapshotAudiences::default())
        .insert_resource(generation_registry)
        .insert_resource(espionage_catalog)
        .insert_resource(espionage_roster)
        .insert_resource(espionage::EspionageMissionState::default())
        .insert_resource(counter_intel_budgets)
        .insert_resource(security_policies)
        .insert_resource(border_policies)
        .insert_resource(influencer_config_handle)
        .insert_resource(influencer_roster)
        .insert_resource(InfluencerImpacts::default())
        .insert_resource(culture_manager)
        .insert_resource(culture_effects)
        .insert_resource(DiscoveryProgressLedger::default())
        .insert_resource(TradeTelemetry::default())
        .insert_resource(systems::PartySightings::default())
        .insert_resource(GreatDiscoveryRegistry::default())
        .insert_resource(GreatDiscoveryReadiness::default())
        .insert_resource(ObservationLedger::default())
        .insert_resource(GreatDiscoveryLedger::default())
        .insert_resource(GreatDiscoveryTelemetry::default())
        .insert_resource(PowerDiscoveryEffects::default())
        .insert_resource(PendingCrisisSeeds::default())
        .insert_resource(PendingCrisisSpawns::default())
        .insert_resource(faction_registry)
        .insert_resource(turn_queue)
        .add_event::<CultureTensionEvent>()
        .add_event::<CultureSchismEvent>()
        .add_event::<systems::TradeDiffusionEvent>()
        .add_event::<EspionageProbeEvent>()
        .add_event::<CounterIntelSweepEvent>()
        .add_event::<GreatDiscoveryCandidateEvent>()
        .add_event::<GreatDiscoveryResolvedEvent>()
        .add_event::<great_discovery::GreatDiscoveryEffectEvent>()
        .add_plugins(MinimalPlugins)
        .configure_sets(
            Update,
            (
                TurnStage::Influence,
                TurnStage::Logistics,
                TurnStage::Knowledge,
                TurnStage::GreatDiscovery,
                TurnStage::Population,
                TurnStage::Visibility,
                TurnStage::Crisis,
                TurnStage::Telling,
                TurnStage::Finalize,
                TurnStage::Victory,
                TurnStage::Snapshot,
            )
                .chain(),
        )
        // Per-stage profiler boundaries. Each marker is pinned between two neighbouring stage sets,
        // so `enter_stage` closes the stage that just finished and opens the next one — an RAII
        // guard cannot span two Bevy systems, which is why the boundaries are systems.
        //
        // Deliberately ungated: unlike the stages themselves these carry no capability `run_if`, so
        // a stage whose systems are gated off records ~0 rather than disappearing from the profile.
        // `begin_turn` is *not* called here — the server owns it, because order application and
        // snapshot broadcast happen outside `app.update()` and belong to the same turn's profile.
        // **The opening window shuts before the turn's first stage runs**, not after it: the
        // budget is spent before the first turn resolves or it is not spent at all, and a system
        // ordered later would leave a window one stage wide where a loadout could still land on a
        // world that had already begun moving.
        .add_systems(
            Update,
            starting_loadout::close_opening_window.before(TurnStage::Influence),
        )
        // ⛔ **WHAT A BENCH FINISHED LAST TURN REACHES THE STORE BEFORE ANYTHING READS THE STORE.**
        // `advance_crafting` runs after the labour pass that settles the band's tools, so it parks
        // a finished tool on the bench rather than stocking it into a ledger that turn's settlement
        // has already read. This drains it at the top of the next turn, ahead of the first stage,
        // so every `BandEquipment` reader in the turn — the husbandry and deposit passes in
        // Logistics, `bill_and_stock_roads` and the labour settlement in Population, the sight
        // sweep's wayfinding wear, the capture — sees one ledger, and the turn the tool first
        // shows in the store is the turn it is first issued. See `deliver_bench_output`.
        .add_systems(
            Update,
            systems::deliver_bench_output.before(TurnStage::Influence),
        )
        // **Each ledger opens this turn's wear slot** - the window the "coming up" craft suggestions
        // read their rate off (#778). After the delivery only because both write `BandEquipment`.
        .add_systems(
            Update,
            systems::open_wear_slots
                .after(systems::deliver_bench_output)
                .before(TurnStage::Influence),
        )
        .add_systems(
            Update,
            (
                turn_profile::stage_marker("influence").before(TurnStage::Influence),
                turn_profile::stage_marker("logistics")
                    .after(TurnStage::Influence)
                    .before(TurnStage::Logistics),
                turn_profile::stage_marker("knowledge")
                    .after(TurnStage::Logistics)
                    .before(TurnStage::Knowledge),
                turn_profile::stage_marker("great_discovery")
                    .after(TurnStage::Knowledge)
                    .before(TurnStage::GreatDiscovery),
                turn_profile::stage_marker("population")
                    .after(TurnStage::GreatDiscovery)
                    .before(TurnStage::Population),
                turn_profile::stage_marker("visibility")
                    .after(TurnStage::Population)
                    .before(TurnStage::Visibility),
                turn_profile::stage_marker("crisis")
                    .after(TurnStage::Visibility)
                    .before(TurnStage::Crisis),
                turn_profile::stage_marker("telling")
                    .after(TurnStage::Crisis)
                    .before(TurnStage::Telling),
                turn_profile::stage_marker("finalize")
                    .after(TurnStage::Telling)
                    .before(TurnStage::Finalize),
                turn_profile::stage_marker("victory")
                    .after(TurnStage::Finalize)
                    .before(TurnStage::Victory),
                turn_profile::stage_marker("snapshot")
                    .after(TurnStage::Victory)
                    .before(TurnStage::Snapshot),
                turn_profile::close_stages_system.after(TurnStage::Snapshot),
            ),
        )
        .add_systems(
            Startup,
            (
                systems::spawn_initial_world,
                // **The brackets and the larder first** — the window's carry is the band's
                // working-age value × one pack, and its fixed larder counts against it, so both
                // must exist before the window is struck.
                systems::apply_starting_inventory_effects,
                // **After the spawn and the seeding, because the carry is the spawned band's own
                // working-age value** — derived from the band, not configured.
                starting_loadout::stamp_starting_loadout,
                hydrology::generate_hydrology,
                systems::apply_tag_budget_solver,
                systems::apply_biome_palette_clamp,
                systems::reconcile_coastal_shelf,
                systems::reconcile_food_modules,
                systems::bias_food_sites_toward_fresh_water,
                sites::place_wondrous_sites,
                spawn_initial_herds,
                spawn_initial_forage,
                spawn_initial_graze,
                // **The game trails the herds wore before the game began** (#215) — after the graze
                // layer, because the live Migrate step it traces reads that layer.
                fauna::stamp_migratory_game_trails,
                espionage::initialise_espionage_roster,
            )
                .chain()
                .run_if(save::worldgen_wanted),
        )
        .add_systems(
            Update,
            (
                tick_influencers,
                // Must precede the reconcile: it is what gives a band its layer (and re-homes a
                // moved one) before the layer is resolved against its province.
                culture::reconcile_band_culture_layers,
                reconcile_culture_layers,
                systems::process_culture_events,
            )
                // Fully serial by data: every pair conflicts (the culture layers each writes).
                // `.chain()` here is the honest encoding, not a leftover default.
                .chain()
                .in_set(TurnStage::Influence)
                .run_if(capability_enabled(CapabilityFlags::ALWAYS_ON)),
        )
        .add_systems(
            Update,
            (
                // The serial backbone. Each of these conflicts with the next on the registry it
                // mutates — `HerdRegistry` down the fauna run, `FactionInventory` at the ends —
                // so the executor could not overlap them even if they were left unordered.
                (
                    systems::simulate_materials,
                    // **The lessons a people holds, folded into its herds' breeding** - before the
                    // herds grow, so this turn's regrowth reads what the last Population stage
                    // taught (`fauna::stamp_breeding_gains`).
                    stamp_breeding_gains,
                    advance_herds,
                    advance_herd_grazing,
                    advance_predation,
                    repopulate_fauna,
                    advance_husbandry,
                )
                    .chain(),
                // The flora/pasture half touches `ForageRegistry`/`GrazeRegistry`, which the fauna
                // backbone above does not, so it runs alongside it. Each declares the one edge it
                // actually has rather than inheriting the whole chain.
                advance_forage_regrowth.after(systems::simulate_materials),
                // The second edge is the feed line: `advance_cultivation` announces a lost plant
                // rung and `advance_husbandry` a lost pen / an under-herded flock, so the two now
                // share `CommandEventLog` and the order they append in is observable. The plant
                // pass goes first, matching the order the two webs already read in the Population
                // stage.
                //
                // **And before the supply pass**, which writes the bands' rows the plant pass reads
                // to name the band a decay line links to (`band=`).
                advance_cultivation
                    .after(advance_forage_regrowth)
                    .before(advance_husbandry)
                    .before(supply::balance_supply_networks),
                advance_graze_regrowth.after(advance_herd_grazing),
                supply::balance_supply_networks.after(advance_herds),
                // ⛔ **AFTER THE POOLING, ALWAYS.** This spends the links that pass recorded, so it
                // has to see them — and the ordering is what makes the payoff a *previous*-turn
                // reading, the same one-turn lag `balance_supply_networks` already accepts against
                // the connection ledger. Reversing it would let this turn's pooling read a road
                // this turn's pooling created.
                //
                // **And after the herd pass**: a kept road's slip or loss is announced on the
                // `CommandEventLog` the plant and herd passes write, so the order the feed appends
                // in is declared — plant, herd, road, working.
                routes::advance_roads
                    .after(supply::balance_supply_networks)
                    .after(advance_husbandry),
                // ⛔ **AFTER THE ROAD PASS, ALWAYS** — declared rather than left to the ambiguity
                // gate. A connection's lesson is read off the road standing `advance_roads` has just
                // produced, so this turn's decay, banking and prune are all already in the registry
                // when the lesson rung is resolved.
                routes::credit_route_lessons.after(routes::advance_roads),
                // **THE WORKINGS' DECAY AND THIS TURN'S BILL** — the deposit branches' twin of
                // `advance_roads`. What it needs of the stage is only to run **before** the
                // Population stage's labour pass, whose `extract` crews keep against the bill it
                // stamps, and a stage boundary already guarantees that.
                //
                // **The one edge it does have is declared** rather than left to the ambiguity gate:
                // it reads `&Tile` (a working's keeper-load is its ground's capacity) and
                // `simulate_materials` writes them, which is the identical edge
                // `advance_forage_regrowth` declares one line up.
                //
                // **And after the road pass**: a working's slip or loss is announced on the feed the
                // three passes above write, and it reads the bands' rows the supply pass writes —
                // both edges reached through `advance_roads`.
                extraction::advance_deposits
                    .after(systems::simulate_materials)
                    .after(routes::advance_roads),
                // **THE TILE IMPROVEMENTS' DECAY, THIS TURN'S BILL AND THE ONE-TURN CYCLE** - the
                // tile kind's twin of `advance_deposits`. It announces a slip or a loss on the feed
                // the passes above write, so it takes its place after the last of them.
                tile_improvement::advance_tile_improvements.after(extraction::advance_deposits),
                // **AND THE BAND KIND'S**, on the same registry resource, so it takes its place after
                // the tile kind's pass rather than racing it for the write.
                band_improvement::advance_band_improvements
                    .after(tile_improvement::advance_tile_improvements),
                // **A HELD MONUMENT ADDS BELIEF** (#692) - the third source on the belief seam,
                // credited right after the tile improvements have settled the turn (decay included)
                // so a rung that just slipped back to its root pays nothing.
                belief::advance_improvement_belief
                    .after(tile_improvement::advance_tile_improvements)
                    .after(band_improvement::advance_band_improvements),
            )
                .in_set(TurnStage::Logistics)
                .run_if(capability_enabled(
                    CapabilityFlags::CONSTRUCTION
                        | CapabilityFlags::INDUSTRY_T1
                        | CapabilityFlags::INDUSTRY_T2
                        | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                espionage::refresh_counter_intel_budgets,
                espionage::schedule_counter_intel_missions,
                espionage::resolve_espionage_missions,
                knowledge_ledger::process_espionage_events,
                knowledge_ledger::knowledge_ledger_tick,
            )
                // `refresh_counter_intel_budgets` is free of the last three, but it must precede
                // `schedule_counter_intel_missions`, which precedes them — so the freedom is
                // already implied and a chain costs nothing. Measured, not assumed.
                .chain()
                .in_set(TurnStage::Knowledge)
                .run_if(capability_enabled(
                    CapabilityFlags::ESPIONAGE_T2 | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                (
                    great_discovery::collect_observation_signals,
                    great_discovery::update_constellation_progress,
                    great_discovery::screen_great_discovery_candidates,
                    great_discovery::resolve_great_discovery,
                    great_discovery::propagate_diffusion_impacts,
                    great_discovery::export_great_discovery_metrics,
                )
                    .chain(),
                // Writes `CapabilityFlags`, which nothing else in the stage reads — so it only
                // needs the discovery that grants the capability, not the reporting tail.
                great_discovery::apply_capability_effects
                    .after(great_discovery::resolve_great_discovery),
            )
                .in_set(TurnStage::GreatDiscovery)
                .run_if(capability_enabled(
                    CapabilityFlags::MEGAPROJECTS | CapabilityFlags::ALWAYS_ON,
                )),
        )
        // **The Population stage is declared by edges, not chained** (`schedule-parallelism.md`).
        // Nearly every pair of its systems conflicts on `PopulationCohort`, so the edges below are
        // close to a total order — but each one is written where it is needed, and a new system
        // declares its own rather than joining a tuple (a tuple stops at twenty members). The
        // ambiguity gate is what keeps this honest: a conflicting pair with no path between them
        // is a boot panic.
        //
        // The four `Commands`-using systems (`follow_hunted_herds`, `advance_band_movement`,
        // `advance_expeditions`, `advance_party_defection`) each have a DIRECT edge to the system
        // that follows them, so the `apply_deferred` sync point sits between them exactly as it did
        // in the chain — `follow_hunted_herds` needs one so `advance_band_movement` sees a freshly
        // inserted `BandTravel` the same turn. The gate cannot see that hazard; these edges are
        // the only thing that pins it.
        //
        // The opening of the stage: the meal, the larder, the outfit, then the movement run.
        .add_systems(
            Update,
            (
                // **A band eats from the larder first and its own stores last** (#709): the
                // shortfall against this turn's meal is drawn from the stores it owns within
                // reach before the meal and before the births are priced off the larder.
                fixed_store::draw_from_stores.before(systems::simulate_population),
                systems::simulate_population,
                // The larder rots right after the meal and before the turn's take lands
                // (#706): what the band could not eat in time is measured against the food it
                // carried into the turn, and resets this turn's `last_food_spoiled`, which the
                // labor pass's caravan transit rot then adds to.
                spoilage::rot_band_larders
                    .after(systems::simulate_population)
                    .after(belief::advance_ritual_trickle),
                // **LIVING NEAR YOUR DEAD TEACHES RITUAL** - reads the ancestor hop counts
                // `simulate_population` just stored, and sits before the rest of the stage's
                // writers of the cohort and the discovery ledger.
                belief::advance_ritual_trickle.after(systems::simulate_population),
                // **Food in a store rots at the store's own shelf life** (#709), beside the larder's
                // rot; it is on no band's Food line and teaches nothing. Ordered ahead of the
                // outfit so it joins the spine (it shares the improvement registry with the labor
                // pass that follows).
                fixed_store::rot_store_contents
                    .after(spoilage::rot_band_larders)
                    .before(starting_loadout::outfit_opening_bands),
                // **The opening band is outfitted on the world-build pass, after its meal** —
                // its fixed larder counts against its carry, and this is the larder the first
                // frame publishes (`starting_loadout::outfit_opening_bands`). A splinter's
                // default is the split's own rule (`split_default_outfit`).
                starting_loadout::outfit_opening_bands
                    .run_if(starting_loadout::on_the_world_build_pass)
                    .after(spoilage::rot_band_larders),
                // Move first so the band's `current_tile` is current before labor reads its
                // in-range sources, then resolve per-worker Forage/Hunt/Scout yields.
                //
                // **Migration mode re-aims the band at its herd first** — the herds moved this
                // turn in Logistics, so the band steps right after its herd and a band camped in
                // it never falls a hex behind (`follow_hunted_herds`).
                systems::follow_hunted_herds.after(starting_loadout::outfit_opening_bands),
                // **`Commands`: the edge from `follow_hunted_herds` is what carries its deferred
                // `BandTravel` insert/remove to this system** — it reads the component the same turn.
                systems::advance_band_movement.after(systems::follow_hunted_herds),
                // Expedition per-turn logic (observe into the pending-reveal buffer, comm-range
                // flush-to-Discovered, return-retarget, arrival/fold-back). Runs right after
                // movement so it reads the party's fresh position, and before the Visibility stage's
                // `discover_sites` picks up any site on the newly-flushed Discovered tiles.
                // **`Commands`**: it re-aims and despawns parties, and the edge to `advance_dealings`
                // is the sync point that lands them.
                systems::advance_expeditions.after(systems::advance_band_movement),
                // Pending dealings: an offer `advance_expeditions` just posted is judged against
                // its deadline and its waiting party in the same turn; silence is a refusal.
                dealings::advance_dealings.after(systems::advance_expeditions),
                // **A stationed party's standing in a foreign host's camp raises the host people's
                // opinion of its own.** It reads every party's phase (settled by
                // `advance_expeditions`) and `PopulationCohort`, which the rest of the stage
                // writes, so it is declared after the dealings pass that follows the expeditions
                // and before the road bill that opens the labour run.
                goodwill::credit_stationed_goodwill.after(dealings::advance_dealings),
            )
                .in_set(TurnStage::Population)
                .run_if(population_stage_enabled()),
        )
        .add_systems(
            Update,
            (
                // **RETIRED: `settle_route_keeping`, the third keeping pool as a SYSTEM OF ITS
                // OWN.** It ran `.after(advance_labor_allocation)` because the `roadwork` head count
                // it divides has to be the one the shedding order left — and that put the payment a
                // whole system *behind* the road build quote, which reads `Road::upkeep_supplied`
                // through `routes::road_meter_rot`. `routes::advance_roads` clears that field a
                // stage earlier and this was its only writer, so every road quoted its rot at a work
                // shortfall of `1.0` and a fully funded road past its grace published the full rot
                // instead of `0`. The payment is now `systems::settle_bands_roadwork`, called from
                // inside `advance_labor_allocation` after the shed and ahead of that pass's
                // `continue`s — the seat both food webs already settle their keeping from, and the
                // only one that is after the shed and before the quote. Its keeping wear still lands
                // before `advance_crafting` for the same reason it did as a system: it is charged
                // inside the labour pass, which the bench already follows.
                // ⛔ **HOLDING WHAT YOU HAVE OUTRANKS EXPANDING** — the roads' bill and the STONE
                // that pays it are struck BEFORE the builders run, so a band's standing paved roads
                // take their material before a new paving build may touch the store. While the
                // build pile settled inside `advance_labor_allocation` and the standing rate
                // settled after it, the build simply got there first: pushing a road out quietly
                // starved the roads already under it. See `bill_and_stock_roads` for why the DRAW
                // moved rather than the STAMP being split.
                //
                // Both edges are declared rather than left to the ambiguity gate — it takes
                // `PopulationCohort` mutably to spend the stone, so its order against the movement
                // run is not the scheduler's to guess:
                //  - after `credit_stationed_goodwill`, which itself follows `advance_expeditions`
                //    and `advance_dealings` — it slots into the run immediately before labor, so
                //    the bill is struck on the positions this turn's movement left.
                //  - `.before(advance_labor_allocation)` — the ordering this system exists for.
                systems::bill_and_stock_roads
                    .after(goodwill::credit_stationed_goodwill)
                    .before(systems::advance_labor_allocation),
                systems::advance_labor_allocation,
                // The bench runs right after labor, for two reasons that both have to hold: it
                // draws on the materials THIS turn's take just delivered, and its crew came out
                // of the same worker pool the assignment loop above spends.
                // **Auto-craft fills an EMPTY queue** right before the bench works, and again
                // right after it (so an order that completes this turn has its successor in
                // the snapshot). See `auto_craft`.
                // **A chief emerges from the practice this turn's labour and expeditions banked**
                // (`leadership`): after labor, which follows the expeditions, and before anything
                // that reads the event log after it.
                //
                // **A band's dry take goes into its pits as it is gathered** (#709): the deposit
                // follows the labor pass that landed the take and precedes everything after it.
                fixed_store::deposit_into_stores.after(systems::advance_labor_allocation),
                leadership::advance_leadership.after(fixed_store::deposit_into_stores),
                auto_craft::advance_auto_craft.after(leadership::advance_leadership),
                systems::advance_crafting.after(auto_craft::advance_auto_craft),
                auto_craft::advance_auto_craft_after_bench.after(systems::advance_crafting),
                // The bench-queue shortage notice reads the stock the draw just left, so it
                // follows the draw directly.
                systems::announce_bench_material_short
                    .after(auto_craft::advance_auto_craft_after_bench),
                // Predator raids fire right after labor so warrior counts and band positions are
                // current: a carnivore with `aggression > 0` within `predators.raid_radius` of a band
                // raids its camp, the band defended by its Warriors (the role's first live consumer).
                systems::advance_predator_raids.after(systems::announce_bench_material_short),
                // Wellbeing migration runs after demographics + this turn's yield payouts so
                // morale/discontent are current and productivity has already been applied at each
                // yield site; it then relocates discontented people (population conserved).
                systems::advance_population_migration.after(systems::advance_predator_raids),
                // A breeding population above its ceiling sheds the excess (#690): after
                // migration, which overwrites each band's `last_emigrated` / `last_immigrated`,
                // and before independence, which judges the roster the break-off may grow.
                systems::shed_breeding_excess.after(systems::advance_population_migration),
                // EXCLUSIVE (it spawns a band), so it conflicts with every system and takes its
                // place in the order the stage always had: after the shed, before the defection
                // and independence passes that judge the roster it may grow.
                systems::break_off_breeding_excess.after(systems::shed_breeding_excess),
                // Pure telemetry: reads `TradeTelemetry`, which two systems write (the cross-people
                // knowledge credit): `advance_population_migration` and `shed_breeding_excess`
                // (through `MigrationKnowledgeSinks`). Both precede `break_off_breeding_excess`, so
                // that one edge orders it after both writers and nothing else — it stays free of
                // the tail below. `break_off_breeding_excess` is an EXCLUSIVE system and so in
                // conflict with every system; the telemetry reads nothing it writes, so it simply
                // takes its place after it.
                systems::publish_trade_telemetry.after(systems::break_off_breeding_excess),
            )
                .in_set(TurnStage::Population)
                .run_if(population_stage_enabled()),
        )
        .add_systems(
            Update,
            (
                // A detached party defects whole (`docs/plan_band_fission.md` §Defection): it
                // reads its home band's morale, which demographics made current, and the
                // sightings `advance_expeditions` recorded this turn; it runs after the trickle
                // so a band that changed people this turn is judged as the people it now is.
                // **`Commands`**: it despawns a party that joined another people, and the edge to
                // `advance_band_independence` is the sync point that lands the despawn before any
                // later pass reads the roster.
                systems::advance_party_defection.after(systems::break_off_breeding_excess),
                // Independence (#284): every band's people is final for the turn and its
                // grievance is this turn's, so a cut-off, aggrieved group is judged as what it
                // now is — and a band that went over this turn is not judged by the people it
                // left.
                systems::advance_band_independence.after(systems::advance_party_defection),
                // A purist band whose cultural strain held splits off (#702): after
                // independence, so every band's people is final and one independence took
                // this turn is skipped.
                systems::advance_culture_splits.after(systems::advance_band_independence),
                // Culture pressure (#820): read-only. After every band's people is final for the
                // turn — the last system that changes a band's people is the culture split —
                // measure each people's strength and each band's pull under another. It shares the
                // event log with `sedentarization_tick`, so that edge is declared too.
                systems::advance_culture_pressure
                    .after(systems::advance_culture_splits)
                    .before(sedentarization::sedentarization_tick),
                sedentarization::sedentarization_tick,
                // **Tribes are read off what the whole stage settled**: the belief anchors
                // (refreshed inside `simulate_population`), this turn's supply networks
                // (`balance_supply_networks`, Logistics) and every band's final people, size and
                // leader. It reads `PopulationCohort` and files on `CommandEventLog`, which most of
                // the stage writes, so it takes the end of the spine and of the pressure/sedentarization
                // tail: that one edge puts it after `advance_leadership`, migration, breeding shed,
                // defection, independence and the culture splits.
                tribe::advance_tribes.after(sedentarization::sedentarization_tick),
                // The roaming counterpart: after `advance_band_movement`, which records the turn's
                // steps following a herd (any roam state) and which this drains — that order is
                // carried by the whole run between them, and the one edge declared here is the
                // culture split, the last pass that changes a band's people.
                mobility::mobility_tick
                    .after(systems::advance_culture_splits)
                    // Leadership reads the herd-following steps this drains.
                    .after(leadership::advance_leadership),
            )
                .in_set(TurnStage::Population)
                .run_if(population_stage_enabled()),
        )
        .add_systems(
            Update,
            (
                // Everything here funnels through `VisibilityLedger`, so it is a serial run.
                (
                    visibility_systems::clear_active_visibility,
                    visibility_systems::calculate_visibility,
                    // Right behind the sweep that FOUND the contacts: it consumes
                    // `ContactsThisTurn` (which `calculate_visibility` and the expedition flush
                    // both fill) and clears it, so the set is rebuilt from scratch every turn.
                    connections::advance_connections,
                    // ⛔ **A KEPT ROAD IS ITS OWN VISIBILITY SOURCE, beside a band's presence and
                    // never through the connection grant** — see `light_kept_routes`. It runs after
                    // the sweep (the fog it writes into is the sweep's) and before the decay, so a
                    // road's tiles are `Active` for the same turn a band's own camp is.
                    visibility_systems::light_kept_routes,
                    visibility_systems::apply_visibility_decay,
                    sites::discover_sites,
                )
                    .chain(),
                // Touches only `VisibilitySweepTracker`, so it overlaps the ledger clear rather
                // than sitting between the two.
                visibility_systems::prune_sweep_tracker
                    .before(visibility_systems::calculate_visibility),
                // **The knowledge rider** reads the tie this turn's contact just refreshed, so it
                // follows the system that folds contact into the ledger. Its other two channels'
                // inputs — a shipment landing, a road's keeping paid — are final by this stage
                // (both settle in `Population`). It sits behind the whole chain rather than
                // directly behind `advance_connections` because `discover_sites` takes
                // `PopulationCohort` mutably, and the ambiguity gate wants that edge declared; the
                // site sweep touches nothing the rider reads. See
                // `knowledge_contact::learn_over_connections`.
                knowledge_contact::learn_over_connections.after(sites::discover_sites),
                // **A standing link raises each people's opinion of the other, and word of it
                // travels along the road** — read at this stage because the roads' keeping has been
                // judged by now (the moment `light_kept_routes` reads it). It takes
                // `PopulationCohort` shared, which `discover_sites` writes, so that edge is stated.
                goodwill::credit_link_goodwill.after(sites::discover_sites),
                // **A treaty in force is the other half of the credit**: a people that has ended
                // goes first (its treaties dissolve with no breach), then each surviving treaty
                // credits both ways. Both write what `credit_link_goodwill` writes, so they are
                // ordered behind it.
                treaties::dissolve_extinct_peoples
                    .after(sites::discover_sites)
                    .before(treaties::credit_treaty_goodwill),
                treaties::credit_treaty_goodwill.after(goodwill::credit_link_goodwill),
            )
                .in_set(TurnStage::Visibility)
                .run_if(capability_enabled(CapabilityFlags::ALWAYS_ON)),
        )
        .add_systems(
            Update,
            crisis::advance_crisis_system.in_set(TurnStage::Crisis),
        )
        .add_systems(Update, telling::telling_tick.in_set(TurnStage::Telling))
        // Opinions fade once a turn, in the FIRST stage, so the fade precedes every credit the turn's
        // systems make (stage sets are chained: Influence < ... < Population). It writes only
        // `Goodwill`, which nothing else in Influence touches, so it declares no further edge.
        .add_systems(Update, goodwill::fade_goodwill.in_set(TurnStage::Influence))
        .add_systems(
            Update,
            // Both write `CorruptionLedgers`; nothing to overlap.
            (systems::simulate_power, systems::process_corruption)
                .chain()
                .in_set(TurnStage::Finalize)
                .run_if(capability_enabled(
                    CapabilityFlags::POWER | CapabilityFlags::ALWAYS_ON,
                )),
        )
        .add_systems(
            Update,
            (
                metrics::collect_metrics,
                systems::advance_tick,
                // **Before the capture, and only on the turn path.** The transfer counters are the
                // one ledger pair that resets, so the frame a *recapture* rebuilds after a command
                // would publish them blank; this copies them onto the cohort, where they are a
                // per-turn value like the other four terms and survive a rebuild.
                systems::publish_turn_transfers,
                // Gated off `Replaying`: a rollback replaying the command log forward must not
                // re-publish frames the client already applied. Its stage-mates above are
                // deliberately NOT gated — see `sim_state::Replaying`.
                snapshot::capture_snapshot.run_if(sim_state::not_replaying),
                // **After the capture has read them.** The band-to-band transfer counters accumulate
                // across the whole snapshot window — commands included — so the one place they may
                // be cleared is behind the publication that reports them.
                systems::reset_transfer_ledger,
            )
                // `SimulationTick` then `SimulationMetrics` — a genuine sequence, and the one
                // stage where running out of order would publish the wrong tick.
                .chain()
                .in_set(TurnStage::Snapshot),
        );

    app.add_systems(Update, victory::victory_tick.in_set(TurnStage::Victory));

    {
        // Log chosen map preset id; worldgen consumes later.
        if let Some(preset) = map_presets.get(
            &app.world
                .resource::<resources::SimulationConfig>()
                .map_preset_id,
        ) {
            tracing::info!(
                target: "shadow_scale::mapgen",
                preset_id = %preset.id,
                name = %preset.name,
                "mapgen.preset.selected"
            );
        } else {
            tracing::warn!(
                target: "shadow_scale::mapgen",
                preset_id = %app.world.resource::<resources::SimulationConfig>().map_preset_id,
                "mapgen.preset.missing_using_first"
            );
        }
        let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
        let loaded = registry
            .load_catalog_from_str(great_discovery::BUILTIN_GREAT_DISCOVERY_CATALOG)
            .expect("Great Discovery catalog should parse");
        tracing::info!(
            target: "shadow_scale::great_discovery",
            loaded_definitions = loaded,
            "great_discovery.catalog.loaded"
        );
    }

    // Ambiguity = two systems with conflicting data access and no ordering edge between them.
    // With the multi-threaded executor that is precisely the determinism hazard: the pair cannot
    // run in parallel anyway (the executor serializes conflicting access), so leaving them
    // unordered doesn't buy a core — it just lets the executor pick a winner, and
    // `integration_tests/tests/determinism.rs` stops holding.
    //
    // Set to `Error`, the schedule build *panics* on such a pair. That is what makes de-chaining
    // safe to do at all: dropping an edge that mattered fails loudly at startup instead of
    // silently making a turn order-dependent. It is also what keeps the property — a system added
    // later without declaring its real edges cannot boot.
    //
    // Scoped to `Update` (the turn schedule) on purpose: `Startup` runs once and its systems stay
    // `.chain()`-ed, and bevy's own schedules are not ours to hold to this bar.
    app.edit_schedule(Update, |schedule| {
        schedule.set_build_settings(ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            ..default()
        });
    });

    app
}

/// **THE MAP EVERY TEST RUNS ON.** A `map_seed` a test can rely on, so a fixture that searches the
/// world for ground with a property finds the *same* ground on every run.
///
/// # Why a harness needs one at all
///
/// `simulation_config.json` ships `map_seed: 0`, and worldgen reads that as *"draw a seed from
/// entropy"* (`systems::worldgen`, the `world_seed == 0` branch) — which is the game working
/// correctly: a New Game gets a random map. But it means [`build_headless_app`] generates **a
/// different world every call**, and a test that asks the map for "a cultivable gathering site" or
/// "a bare sowable tile" gets different terrain each run. That produced real intermittent failures:
/// an assertion satisfied by whichever biome the search happened to land on passes most runs and
/// fails on the map where it doesn't.
///
/// Measured rather than assumed — with the seed free, a hash over every `(x, y, terrain)` differed
/// on all ten runs and the curated site list moved between 129 and 133 entries; with it pinned, that
/// hash, the site list (ordered *and* as a set) and the tile a fixture picks are byte-identical
/// across runs. **Worldgen is deterministic per seed**; it was only ever the seed that moved.
///
/// # The number
///
/// `119304647` is the seed `map_presets.json`'s `polar_contrast` preset already names and the one
/// `tests/forage_cultivation.rs` had pinned by hand long before this helper existed — reused rather
/// than invented so the repo has **one** harness map instead of two.
///
/// **⛔ IT IS NOT A TUNING DIAL.** When a test fails on this map, the map is not wrong: the test was
/// resting on map luck, and the fix is to give that fixture the ground it needs (state its terrain,
/// the way `tests/build_turns_on_the_wire.rs` does) — never to shop for a seed on which everything
/// happens to pass, which would put the harness right back on the map it just got off.
pub const HARNESS_MAP_SEED: u64 = 119304647;

/// **THE ONE WORLD BUILDER A TEST MAY USE** — [`build_headless_app`] with [`HARNESS_MAP_SEED`]
/// pinned, so the map is the same on every run.
///
/// # Why it can pin the seed from out here
///
/// [`build_headless_app`] **runs no `update()`** — it only assembles the `App` and inserts the
/// resources — and it never reads or resolves `map_seed` itself. The seed is resolved inside
/// `spawn_initial_world`, a **Startup** system, which first runs at the caller's first `update()`.
/// So writing the resource between the two is in time, and worldgen's `world_seed == 0` branch never
/// fires. (A preset may still override it — `polar_contrast` names its own seed — and that is
/// deliberate: a test that asks for a preset is asking for that preset's map.)
///
/// **`build_headless_app` is deliberately left exactly as it is.** It is the *production* world
/// builder (`bin/server.rs` boots the real server with it), so pinning a seed in there would make
/// every New Game deterministic — changing the game to fix the tooling. The pin belongs here, on the
/// path only tests take.
/// # ⛔ IT ALSO STOCKS THE BAND'S GEAR, FOR THE SEED'S OWN REASON
///
/// `equipment.json`'s `start_stock_fraction` ships **`0.0`** — a spawning band owns nothing
/// (`equipment.md` → "A SPAWNING BAND OWNS NO EQUIPMENT AT ALL"). That is the *game's* opening, and
/// it is not a fixture input: a hundred-odd tests that mean *"a band with working gear"* used to say
/// so by **saying nothing**, so a tuning lever nobody thought was a test input moved a hundred
/// expected values at once.
///
/// So this builder installs [`EquipmentConfigHandle::for_a_stocked_fixture`] — the pre-change spawn,
/// `ceil(workers × 1.5 / workers_per_unit)` — exactly as it pins the map seed, and for the same
/// stated reason: **a test resting on shipped luck is the test that is wrong.** A fixture whose
/// subject genuinely *is* the shipped opening overrides the resource back, the way a fixture that
/// wants a particular map states its terrain.
pub fn build_test_app() -> App {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = HARNESS_MAP_SEED;
    app.world
        .insert_resource(crate::equipment_config::EquipmentConfigHandle::for_a_stocked_fixture());
    app
}

/// **HOLD A PACE FIXTURE'S GEAR AXIS AT ITS IDENTITY** — take the tools `rung` wants off this band's
/// ledger and leave everything else exactly as the turn would have found it.
///
/// A rung's own tool adds `+0.5` work **per covered worker per turn** on top of the builders' own
/// hands, so a band that holds one delivers `n × 1.5` a turn against `n`: any fixture measuring the
/// *meter* rather than the tool runs half again as fast as the number it asserts.
///
/// ⛔ **THE AXIS IS HELD ON THE LEDGER, NOT ON A KIT NAMED ON THE DECLARED BUILD.** It used to be
/// `BuildDeclaration { kit: Some(the empty kit) }`, because a build's gear was resolved from one kit
/// the entry named. A pool's tools follow from the **rung** since `docs/plan_pool_toe.md`, so an
/// entry's kit prices nothing and a bare one there holds nothing at its identity.
///
/// ⛔ **AND IT STRIPS THIS RUNG'S TOOLS ONLY, NEVER THE WHOLE LEDGER.** An empty ledger disarms the
/// **take** crews beside the build — no spears, no baskets — which moves the very yields such
/// fixtures are measuring. It also cannot be *inserted* blind: an absent [`BandEquipment`] is filled
/// in by the labour pass from [`BandEquipment::start_stocked_for`] at the band's own head count, so
/// a fixture that inserted a differently-sized ledger would quietly re-stock every take row. So the
/// ledger the pass would have resolved is rebuilt here and only this rung's requirement is emptied
/// out of it.
pub fn disarm_the_builders(world: &mut World, band: Entity, rung: crate::intensification::RungId) {
    let equipment = world
        .resource::<crate::equipment_config::EquipmentConfigHandle>()
        .get();
    let available = available_workers(
        world
            .get::<PopulationCohort>(band)
            .expect("the fixture band has a cohort")
            .working,
    );
    let mut ledger = world
        .get::<BandEquipment>(band)
        .cloned()
        .unwrap_or_else(|| BandEquipment::start_stocked_for(&equipment, available as f32));
    for tool in equipment
        .pool_toe(rung.branch, Some(&rung.wire_key()))
        .tools()
    {
        ledger.restore_batches(&tool.item, Vec::new());
    }
    world.entity_mut(band).insert(ledger);
}

/// **LEAVE EXACTLY `idle` HANDS UNCOMMITTED ON A FIXTURE BAND** - the builders a declared build is
/// staffed from. A build has no `builders` row to put a count on: the sim staffs it from the hands
/// the band's other crews, its bench and its walkers leave (`docs/plan_labor_at_scale.md`, "Builds
/// join the site's crew"), so a fixture that wants N builders states it as N people beyond what is
/// already committed. Sets the band's working-age head count to `committed + idle`.
pub fn seat_builders(world: &mut World, band: Entity, idle: u32) {
    world.entity_mut(band).insert(SeatedBuilders(idle));
    let committed = world
        .get::<crate::components::LaborAllocation>(band)
        .map_or(0, |allocation| {
            // A staffed row is restaffed to its Max each turn, so the hands it WILL hold are the
            // committed ones - not what it held when the fixture last looked.
            allocation
                .assignments
                .iter()
                .map(|row| row.max_workers.unwrap_or(row.staffed_total()))
                .sum::<u32>()
        })
        + world
            .get::<crate::components::BandBench>(band)
            .map_or(0, |bench| bench.workers);
    let mut cohort = world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band has a cohort");
    cohort.working = crate::scalar::Scalar::from_u32(committed + idle);
}

/// **A labor config with room for every crew a fixture stands on one hex** - the shipped config
/// with `tile_worker_cap` lifted past any fixture crew, for the harnesses whose one source row plus
/// its builders would otherwise meet the per-tile cap rather than the thing under test. A test
/// helper; nothing in the sim calls it.
pub fn labor_config_with_room_on_the_tile() -> LaborConfigHandle {
    /// More hands than any fixture stands on one hex.
    const ROOM_FOR_EVERY_FIXTURE_CREW: u32 = 1000;
    let mut labor = (*LaborConfig::builtin()).clone();
    labor.tile_worker_cap = ROOM_FOR_EVERY_FIXTURE_CREW;
    LaborConfigHandle::new(std::sync::Arc::new(labor))
}

/// **The idle hands a fixture stated for a band's builds** - recorded by [`seat_builders`] so
/// [`reseat_builders`] can restate them after a fixture resizes the band or changes its rows.
#[derive(bevy::prelude::Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatedBuilders(pub u32);

/// Restate every seated band's idle hands (see [`SeatedBuilders`]): working-age becomes what the band
/// has committed plus the idle count the fixture asked for. A test helper; nothing in the sim calls
/// it.
pub fn reseat_builders(world: &mut World) {
    let seated: Vec<(Entity, u32)> = world
        .query::<(Entity, &SeatedBuilders)>()
        .iter(world)
        .map(|(entity, seated)| (entity, seated.0))
        .collect();
    for (band, idle) in seated {
        seat_builders(world, band, idle);
    }
}

/// Execute a single simulation turn.
///
/// Each call runs the [`TurnStage`] sets chained in [`build_headless_app`], in order:
/// Influence → Logistics → Knowledge → GreatDiscovery → Population → Visibility → Crisis →
/// Telling → Finalize → Victory → Snapshot.
///
/// Individual systems are not stages: `simulate_materials` runs inside `Logistics`,
/// `simulate_power` inside `Finalize`, and `advance_tick` inside `Snapshot`.
///
/// Callers are responsible for snapshot broadcasting and command handling.
pub fn run_turn(app: &mut App) {
    app.update();
}

/// The capability gate every `TurnStage::Population` group shares. The stage is declared across
/// several `add_systems` calls (a tuple stops at twenty members), and each needs the same
/// condition; one function keeps them from drifting apart.
fn population_stage_enabled() -> impl FnMut(Res<CapabilityFlags>) -> bool {
    capability_enabled(
        CapabilityFlags::CONSTRUCTION
            | CapabilityFlags::INDUSTRY_T1
            | CapabilityFlags::INDUSTRY_T2
            | CapabilityFlags::ALWAYS_ON,
    )
}

fn capability_enabled(flags: CapabilityFlags) -> impl FnMut(Res<CapabilityFlags>) -> bool {
    move |current: Res<CapabilityFlags>| current.intersects(flags)
}
