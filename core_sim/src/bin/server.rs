use std::io::{self, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use bevy::{ecs::system::Resource, math::UVec2, prelude::Entity};
use crossbeam_channel::{unbounded, Receiver, Sender};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{info, warn};
use tracing_subscriber::prelude::*;

use core_sim::log_stream::start_log_stream_server;

use core_sim::metrics::SimulationMetrics;
use core_sim::network::{broadcast_latest, start_snapshot_server, SnapshotServer};
use core_sim::{
    build_headless_app, command_events_to_state, restore_world_from_snapshot, run_turn,
    scalar_from_f32, AgentAssignment, CommandEventEntry, CommandEventKind, CommandEventLog,
    CorruptionLedgers, CounterIntelBudgets, CrisisArchetypeCatalog, CrisisArchetypeCatalogHandle,
    CrisisArchetypeCatalogMetadata, CrisisModifierCatalog, CrisisModifierCatalogHandle,
    CrisisModifierCatalogMetadata, CrisisTelemetry, CrisisTelemetryConfig,
    CrisisTelemetryConfigHandle, CrisisTelemetryConfigMetadata, EspionageAgentHandle,
    EspionageCatalog, EspionageMissionId, EspionageMissionKind, EspionageMissionState,
    EspionageMissionTemplate, EspionageRoster, FactionId, FactionOrders, FactionRegistry,
    FactionSecurityPolicies, FaunaConfigHandle, FaunaPursuit, FaunaPursuitMode, FogRevealLedger,
    FollowPolicy, FoodModule, FoodModuleTag, GenerationId, GenerationRegistry, HarvestAssignment,
    HerdRegistry, InfluencerImpacts, InfluentialRoster, MapPresetsHandle, PendingCrisisSpawns,
    PopulationCohort, QueueMissionError, QueueMissionParams, Scalar, ScoutAssignment,
    SecurityPolicy, SentimentAxisBias, Settlement, SimulationConfig, SimulationConfigMetadata,
    SimulationTick, SnapshotHistory, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    SnapshotOverlaysConfigMetadata, StartLocation, StartProfileLookup, StartProfilesHandle,
    StartingUnit, StoredSnapshot, SubmitError, SubmitOutcome, SupportChannel, Tile, TileRegistry,
    TownCenter, TurnPipelineConfig, TurnPipelineConfigHandle, TurnPipelineConfigMetadata,
    TurnQueue, DEFAULT_HARVEST_TRAVEL_TILES_PER_TURN, DEFAULT_HARVEST_WORK_TURNS, FOOD,
};
use core_sim::{
    resolve_active_profile, ActiveStartProfile, CampaignLabel, HarvestTaskKind,
    StartProfileOverrides,
};
use sim_runtime::{
    commands::{EspionageGeneratorUpdate as CommandGeneratorUpdate, ReloadConfigKind},
    AxisBiasState, CommandEnvelope as ProtoCommandEnvelope, CommandPayload as ProtoCommandPayload,
    CorruptionEntry, CorruptionSubsystem, InfluenceScopeKind,
    OrdersDirective as ProtoOrdersDirective, SecurityPolicyKind,
    SupportChannel as ProtoSupportChannel, TerrainTags,
};
use sim_schema::{encode_map_export_json, MapExport};

/// Gitignored scratch directory that `export_map` writes into when the command
/// is invoked without an explicit path.
const DEFAULT_EXPORT_DIR: &str = "exports";

const MIN_SCOUT_REVEAL_RADIUS: u32 = 2;
const DEFAULT_SCOUT_REVEAL_RADIUS: u32 = 3;
const SCOUT_REVEAL_DURATION_TURNS: u64 = 8;
const SCOUT_MORALE_GAIN: f32 = 0.02;
const SCOUT_PROVISION_COST: i64 = 10;
const CAMP_PROVISION_COST: i64 = 40;
const SETTLEMENT_PROVISION_COST: i64 = 80;
const SETTLEMENT_CONSTRUCTION_RADIUS: u32 = 3;
const SETTLEMENT_LOGISTICS_RADIUS: u32 = 4;

fn main() {
    let mut app = build_headless_app();
    app.insert_resource(SimulationMetrics::default());

    let config = app.world.resource::<SimulationConfig>().clone();

    let log_stream = start_log_stream_server(config.log_bind);
    let log_stream_enabled = log_stream.is_some();
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if let Some(handle) = &log_stream {
        tracing_subscriber::registry()
            .with(env_filter.clone())
            .with(tracing_subscriber::fmt::layer())
            .with(handle.layer())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    if !log_stream_enabled {
        warn!(target: "shadow_scale::server", "log_stream.start_failed");
    }

    let snapshot_server = start_snapshot_server(config.snapshot_bind);
    let snapshot_flat_server = start_snapshot_server(config.snapshot_flat_bind);

    let config_watch_path = app
        .world
        .resource::<SimulationConfigMetadata>()
        .path()
        .cloned();
    let turn_pipeline_watch_path = app
        .world
        .resource::<TurnPipelineConfigMetadata>()
        .path()
        .cloned();
    let snapshot_overlays_watch_path = app
        .world
        .resource::<SnapshotOverlaysConfigMetadata>()
        .path()
        .cloned();
    let crisis_archetypes_watch_path = app
        .world
        .resource::<CrisisArchetypeCatalogMetadata>()
        .path()
        .cloned();
    let crisis_modifiers_watch_path = app
        .world
        .resource::<CrisisModifierCatalogMetadata>()
        .path()
        .cloned();
    let crisis_telemetry_watch_path = app
        .world
        .resource::<CrisisTelemetryConfigMetadata>()
        .path()
        .cloned();

    let (command_rx, command_tx) = spawn_command_listener(config.command_bind);
    app.world
        .insert_resource(CommandSenderResource(command_tx.clone()));
    app.world.insert_resource(ConfigWatcherRegistry::default());

    if let Some(path) = config_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_simulation(Some(path), command_tx.clone());
    }
    if let Some(path) = turn_pipeline_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_turn_pipeline(Some(path), command_tx.clone());
    }
    if let Some(path) = snapshot_overlays_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_snapshot_overlays(Some(path), command_tx.clone());
    }
    if let Some(path) = crisis_archetypes_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_crisis_archetypes(Some(path), command_tx.clone());
    }
    if let Some(path) = crisis_modifiers_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_crisis_modifiers(Some(path), command_tx.clone());
    }
    if let Some(path) = crisis_telemetry_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_crisis_telemetry(Some(path), command_tx.clone());
    }

    run_turn(&mut app);

    {
        let history = app.world.resource::<SnapshotHistory>();
        broadcast_latest(
            snapshot_server.as_ref(),
            snapshot_flat_server.as_ref(),
            history,
        );
    }

    info!(
        command_bind = %config.command_bind,
        snapshot_bind = %config.snapshot_bind,
        snapshot_flat_bind = %config.snapshot_flat_bind,
        log_bind = %config.log_bind,
        log_stream_enabled,
        "Shadow-Scale headless server ready"
    );

    while let Ok(command) = command_rx.recv() {
        let bin_server = snapshot_server.as_ref();
        let flat_server = snapshot_flat_server.as_ref();
        match command {
            Command::Turn(turns) => {
                for _ in 0..turns {
                    {
                        let mut queue = app.world.resource_mut::<TurnQueue>();
                        let awaiting = queue.awaiting();
                        for faction in &awaiting {
                            info!(
                                target: "shadow_scale::server",
                                %faction,
                                "orders.auto_generated=end_turn"
                            );
                        }
                        queue.force_submit_all(|_| FactionOrders::end_turn());
                    }
                    resolve_ready_turn(&mut app, bin_server, flat_server);
                }
            }
            Command::ResetMap { width, height } => {
                if width == 0 || height == 0 {
                    warn!(
                        target: "shadow_scale::server",
                        width,
                        height,
                        "map.reset.rejected=invalid_dimensions"
                    );
                    continue;
                }
                let command_sender = {
                    let res = app.world.resource::<CommandSenderResource>();
                    res.0.clone()
                };
                let current_config = app.world.resource::<SimulationConfig>().clone();
                let seed_random_requested = {
                    let metadata = app.world.resource::<SimulationConfigMetadata>();
                    metadata.seed_random()
                };
                let preset_seed = {
                    let presets = app.world.resource::<MapPresetsHandle>();
                    presets
                        .get()
                        .get(&current_config.map_preset_id)
                        .and_then(|preset| preset.map_seed)
                };
                let should_randomize_seed = seed_random_requested && preset_seed.is_none();
                let same_dimensions =
                    current_config.grid_size.x == width && current_config.grid_size.y == height;
                let sim_watch_path = app
                    .world
                    .resource::<SimulationConfigMetadata>()
                    .path()
                    .cloned();
                let turn_pipeline_watch_path = app
                    .world
                    .resource::<TurnPipelineConfigMetadata>()
                    .path()
                    .cloned();
                let snapshot_overlays_watch_path = app
                    .world
                    .resource::<SnapshotOverlaysConfigMetadata>()
                    .path()
                    .cloned();
                let crisis_archetypes_watch_path = app
                    .world
                    .resource::<CrisisArchetypeCatalogMetadata>()
                    .path()
                    .cloned();
                let crisis_modifiers_watch_path = app
                    .world
                    .resource::<CrisisModifierCatalogMetadata>()
                    .path()
                    .cloned();
                let crisis_telemetry_watch_path = app
                    .world
                    .resource::<CrisisTelemetryConfigMetadata>()
                    .path()
                    .cloned();
                info!(
                    target: "shadow_scale::server",
                    width,
                    height,
                    same_dimensions,
                    "map.reset.begin"
                );
                let mut new_config = current_config.clone();
                new_config.grid_size = UVec2::new(width, height);
                if should_randomize_seed {
                    new_config.map_seed = 0;
                }

                let mut new_app = build_headless_app();
                {
                    let mut config_res = new_app.world.resource_mut::<SimulationConfig>();
                    *config_res = new_config;
                }
                new_app.insert_resource(SimulationMetrics::default());
                new_app.insert_resource(CommandSenderResource(command_sender.clone()));
                new_app.insert_resource(ConfigWatcherRegistry::default());
                {
                    let mut metadata = new_app.world.resource_mut::<SimulationConfigMetadata>();
                    metadata.set_path(sim_watch_path.clone());
                    metadata.set_seed_random(seed_random_requested);
                }
                {
                    let mut metadata = new_app.world.resource_mut::<TurnPipelineConfigMetadata>();
                    metadata.set_path(turn_pipeline_watch_path.clone());
                }
                {
                    let mut metadata = new_app
                        .world
                        .resource_mut::<SnapshotOverlaysConfigMetadata>();
                    metadata.set_path(snapshot_overlays_watch_path.clone());
                }
                {
                    let mut metadata = new_app
                        .world
                        .resource_mut::<CrisisArchetypeCatalogMetadata>();
                    metadata.set_path(crisis_archetypes_watch_path.clone());
                }
                {
                    let mut metadata = new_app
                        .world
                        .resource_mut::<CrisisModifierCatalogMetadata>();
                    metadata.set_path(crisis_modifiers_watch_path.clone());
                }
                {
                    let mut metadata = new_app
                        .world
                        .resource_mut::<CrisisTelemetryConfigMetadata>();
                    metadata.set_path(crisis_telemetry_watch_path.clone());
                }
                {
                    let mut watcher_registry =
                        new_app.world.resource_mut::<ConfigWatcherRegistry>();
                    watcher_registry
                        .restart_simulation(sim_watch_path.clone(), command_sender.clone());
                    watcher_registry.restart_turn_pipeline(
                        turn_pipeline_watch_path.clone(),
                        command_sender.clone(),
                    );
                    watcher_registry.restart_snapshot_overlays(
                        snapshot_overlays_watch_path.clone(),
                        command_sender.clone(),
                    );
                    watcher_registry.restart_crisis_archetypes(
                        crisis_archetypes_watch_path.clone(),
                        command_sender.clone(),
                    );
                    watcher_registry.restart_crisis_modifiers(
                        crisis_modifiers_watch_path.clone(),
                        command_sender.clone(),
                    );
                    watcher_registry.restart_crisis_telemetry(
                        crisis_telemetry_watch_path.clone(),
                        command_sender.clone(),
                    );
                }
                run_turn(&mut new_app);

                {
                    let history = new_app.world.resource::<SnapshotHistory>();
                    broadcast_latest(bin_server, flat_server, history);
                }

                app = new_app;
                info!(
                    target: "shadow_scale::server",
                    width,
                    height,
                    same_dimensions,
                    "map.reset.completed"
                );
            }
            Command::ExportMap { path } => {
                write_map_export(&app, path);
            }
            Command::Heat { entity, delta } => {
                apply_heat(&mut app, entity, delta);
                info!(
                    target: "shadow_scale::server",
                    entity,
                    delta,
                    "command.applied=heat"
                );
            }
            Command::Orders { faction, orders } => {
                handle_order_submission(&mut app, faction, orders, bin_server, flat_server);
            }
            Command::Rollback { tick } => {
                handle_rollback(&mut app, tick, bin_server, flat_server);
            }
            Command::AxisBias { axis, value } => {
                handle_axis_bias(&mut app, axis, value, bin_server, flat_server);
            }
            Command::SupportInfluencer { id, magnitude } => {
                handle_influencer_command(
                    &mut app,
                    id,
                    magnitude,
                    InfluencerAction::Support,
                    bin_server,
                    flat_server,
                );
            }
            Command::SuppressInfluencer { id, magnitude } => {
                handle_influencer_command(
                    &mut app,
                    id,
                    magnitude,
                    InfluencerAction::Suppress,
                    bin_server,
                    flat_server,
                );
            }
            Command::SupportInfluencerChannel {
                id,
                channel,
                magnitude,
            } => {
                handle_influencer_channel_support(
                    &mut app,
                    id,
                    channel,
                    magnitude,
                    bin_server,
                    flat_server,
                );
            }
            Command::SpawnInfluencer { scope, generation } => {
                handle_influencer_spawn(&mut app, scope, generation, bin_server, flat_server);
            }
            Command::InjectCorruption {
                subsystem,
                intensity,
                exposure_timer,
            } => {
                handle_inject_corruption(
                    &mut app,
                    subsystem,
                    intensity,
                    exposure_timer,
                    bin_server,
                    flat_server,
                );
            }
            Command::UpdateEspionageGenerators { updates } => {
                handle_update_espionage_generators(&mut app, updates);
            }
            Command::QueueEspionageMission { params } => {
                handle_queue_espionage_mission(&mut app, params);
            }
            Command::UpdateEspionageQueueDefaults {
                scheduled_tick_offset,
                target_tier,
            } => {
                handle_update_queue_defaults(&mut app, scheduled_tick_offset, target_tier);
            }
            Command::UpdateCounterIntelPolicy { faction, policy } => {
                handle_update_counter_intel_policy(&mut app, faction, policy);
            }
            Command::AdjustCounterIntelBudget {
                faction,
                reserve,
                delta,
            } => {
                handle_adjust_counter_intel_budget(&mut app, faction, reserve, delta);
            }
            Command::ReloadConfig { kind, path } => {
                handle_reload_config(&mut app, kind, path);
            }
            Command::SetCrisisAutoSeed { enabled } => {
                {
                    let mut config_res = app.world.resource_mut::<SimulationConfig>();
                    config_res.crisis_auto_seed = enabled;
                }
                info!(
                    target: "shadow_scale::server",
                    enabled,
                    "crisis.autoseed.updated"
                );
            }
            Command::SpawnCrisis {
                faction,
                archetype_id,
            } => {
                let archetype = archetype_id.clone();
                {
                    let mut spawns = app.world.resource_mut::<PendingCrisisSpawns>();
                    spawns.push(faction, archetype);
                }
                info!(
                    target: "shadow_scale::server",
                    faction = %faction.0,
                    archetype = %archetype_id,
                    "crisis.spawn.enqueued"
                );
            }
            Command::SetStartProfile { profile_id } => {
                handle_set_start_profile(&mut app, profile_id);
            }
            Command::ScoutArea {
                faction,
                target_x,
                target_y,
                band_entity_bits,
            } => {
                handle_scout_area(&mut app, faction, target_x, target_y, band_entity_bits);
            }
            Command::FollowHerd {
                faction,
                herd_id,
                policy,
                band_entity_bits,
            } => {
                handle_follow_herd(&mut app, faction, herd_id, policy, band_entity_bits);
            }
            Command::FoundCamp {
                faction,
                target_x,
                target_y,
            } => {
                handle_found_camp(&mut app, faction, target_x, target_y);
            }
            Command::FoundSettlement {
                faction,
                target_x,
                target_y,
            } => {
                handle_found_settlement(&mut app, faction, target_x, target_y);
            }
            Command::ForageTile {
                faction,
                target_x,
                target_y,
                module,
                band_entity_bits,
            } => {
                handle_forage_tile(
                    &mut app,
                    faction,
                    target_x,
                    target_y,
                    module,
                    band_entity_bits,
                );
            }
            Command::HuntGame {
                faction,
                target_x,
                target_y,
                band_entity_bits,
            } => {
                handle_hunt_game(&mut app, faction, target_x, target_y, band_entity_bits);
            }
            Command::HuntFauna {
                faction,
                herd_id,
                band_entity_bits,
            } => {
                handle_hunt_fauna(&mut app, faction, herd_id, band_entity_bits);
            }
            Command::Domesticate { faction, herd_id } => {
                handle_domesticate(&mut app, faction, herd_id);
            }
        }

        broadcast_command_events_if_needed(&mut app, bin_server, flat_server);
    }
}

#[derive(Debug)]
enum Command {
    Turn(u32),
    ResetMap {
        width: u32,
        height: u32,
    },
    Heat {
        entity: u64,
        delta: i64,
    },
    Orders {
        faction: FactionId,
        orders: FactionOrders,
    },
    Rollback {
        tick: u64,
    },
    AxisBias {
        axis: usize,
        value: f32,
    },
    SupportInfluencer {
        id: u32,
        magnitude: f32,
    },
    SuppressInfluencer {
        id: u32,
        magnitude: f32,
    },
    SupportInfluencerChannel {
        id: u32,
        channel: SupportChannel,
        magnitude: f32,
    },
    SpawnInfluencer {
        scope: Option<InfluenceScopeKind>,
        generation: Option<GenerationId>,
    },
    InjectCorruption {
        subsystem: CorruptionSubsystem,
        intensity: f32,
        exposure_timer: u16,
    },
    UpdateEspionageGenerators {
        updates: Vec<CommandGeneratorUpdate>,
    },
    QueueEspionageMission {
        params: QueueMissionParams,
    },
    UpdateEspionageQueueDefaults {
        scheduled_tick_offset: Option<u64>,
        target_tier: Option<u8>,
    },
    UpdateCounterIntelPolicy {
        faction: FactionId,
        policy: SecurityPolicy,
    },
    AdjustCounterIntelBudget {
        faction: FactionId,
        reserve: Option<Scalar>,
        delta: Option<Scalar>,
    },
    ReloadConfig {
        kind: ReloadConfigKind,
        path: Option<String>,
    },
    SetCrisisAutoSeed {
        enabled: bool,
    },
    SpawnCrisis {
        faction: FactionId,
        archetype_id: String,
    },
    SetStartProfile {
        profile_id: String,
    },
    ScoutArea {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
        band_entity_bits: Option<u64>,
    },
    FollowHerd {
        faction: FactionId,
        herd_id: String,
        policy: FollowPolicy,
        band_entity_bits: Option<u64>,
    },
    FoundCamp {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    FoundSettlement {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    ForageTile {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
        module: String,
        band_entity_bits: Option<u64>,
    },
    HuntGame {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
        band_entity_bits: Option<u64>,
    },
    HuntFauna {
        faction: FactionId,
        herd_id: String,
        band_entity_bits: Option<u64>,
    },
    Domesticate {
        faction: FactionId,
        herd_id: String,
    },
    ExportMap {
        path: Option<String>,
    },
}

enum InfluencerAction {
    Support,
    Suppress,
}

#[derive(Resource, Clone)]
struct CommandSenderResource(Sender<Command>);

#[derive(Resource, Default)]
struct ConfigWatcherRegistry {
    simulation: Option<FileWatcherHandle>,
    turn_pipeline: Option<FileWatcherHandle>,
    snapshot_overlays: Option<FileWatcherHandle>,
    crisis_archetypes: Option<FileWatcherHandle>,
    crisis_modifiers: Option<FileWatcherHandle>,
    crisis_telemetry: Option<FileWatcherHandle>,
}

impl ConfigWatcherRegistry {
    fn restart_simulation(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.simulation.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::Simulation) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "simulation_config.watch_started"
                    );
                    self.simulation = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "simulation_config.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "simulation_config.watch_disabled"
            );
        }
    }

    fn restart_turn_pipeline(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.turn_pipeline.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::TurnPipeline) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "turn_pipeline_config.watch_started"
                    );
                    self.turn_pipeline = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "turn_pipeline_config.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "turn_pipeline_config.watch_disabled"
            );
        }
    }

    fn restart_snapshot_overlays(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.snapshot_overlays.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::SnapshotOverlays) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "snapshot_overlays_config.watch_started"
                    );
                    self.snapshot_overlays = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "snapshot_overlays_config.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "snapshot_overlays_config.watch_disabled"
            );
        }
    }

    fn restart_crisis_archetypes(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.crisis_archetypes.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::CrisisArchetypes) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "crisis_archetypes.watch_started"
                    );
                    self.crisis_archetypes = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "crisis_archetypes.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "crisis_archetypes.watch_disabled"
            );
        }
    }

    fn restart_crisis_modifiers(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.crisis_modifiers.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::CrisisModifiers) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "crisis_modifiers.watch_started"
                    );
                    self.crisis_modifiers = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "crisis_modifiers.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "crisis_modifiers.watch_disabled"
            );
        }
    }

    fn restart_crisis_telemetry(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.crisis_telemetry.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::CrisisTelemetry) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "crisis_telemetry_config.watch_started"
                    );
                    self.crisis_telemetry = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "crisis_telemetry_config.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "crisis_telemetry_config.watch_disabled"
            );
        }
    }
}

struct FileWatcherHandle {
    stop_tx: mpsc::Sender<()>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl FileWatcherHandle {
    fn stop(mut self) {
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for FileWatcherHandle {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

const MAX_PROTO_FRAME: usize = 64 * 1024;

fn spawn_command_listener(bind_addr: std::net::SocketAddr) -> (Receiver<Command>, Sender<Command>) {
    let listener = TcpListener::bind(bind_addr).expect("command listener bind failed");
    if let Err(err) = listener.set_nonblocking(true) {
        warn!("Failed to set nonblocking on command listener: {}", err);
    }

    let (sender, receiver) = unbounded::<Command>();
    let sender_for_thread = sender.clone();
    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                info!("Command client connected: {}", addr);
                let sender = sender_for_thread.clone();
                thread::spawn(move || handle_proto_client(stream, sender));
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(err) => {
                warn!("Error accepting command client: {}", err);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    });

    (receiver, sender)
}

fn handle_proto_client(stream: TcpStream, sender: Sender<Command>) {
    let mut reader = BufReader::new(stream);
    loop {
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf) {
            Ok(_) => {}
            Err(err) => {
                if err.kind() != io::ErrorKind::UnexpectedEof {
                    warn!("Proto command length read error: {}", err);
                }
                break;
            }
        }
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        if frame_len == 0 {
            warn!("Proto command received empty frame");
            continue;
        }
        if frame_len > MAX_PROTO_FRAME {
            warn!(
                "Proto command frame too large ({} bytes), dropping connection",
                frame_len
            );
            break;
        }
        let mut payload = vec![0u8; frame_len];
        if let Err(err) = reader.read_exact(&mut payload) {
            if err.kind() != io::ErrorKind::UnexpectedEof {
                warn!("Proto command payload read error: {}", err);
            }
            break;
        }
        match ProtoCommandEnvelope::decode(&payload) {
            Ok(envelope) => {
                if let Some(cmd) = command_from_payload(envelope.payload) {
                    if sender.send(cmd).is_err() {
                        break;
                    }
                }
            }
            Err(err) => {
                warn!("Proto command decode error: {}", err);
            }
        }
    }
}

fn start_file_watcher(
    path: PathBuf,
    sender: Sender<Command>,
    kind: ReloadConfigKind,
) -> notify::Result<FileWatcherHandle> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel();
    let watcher_path = path.clone();

    let handle = thread::spawn(move || {
        let (event_tx, event_rx) = mpsc::channel();
        match notify::recommended_watcher(move |res| {
            let _ = event_tx.send(res);
        }) {
            Ok(mut watcher) => {
                if let Err(err) = watcher.watch(&watcher_path, RecursiveMode::NonRecursive) {
                    let _ = ready_tx.send(Err(err));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                watch_config(watcher_path, watcher, event_rx, stop_rx, sender, kind);
            }
            Err(err) => {
                let _ = ready_tx.send(Err(err));
            }
        }
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(FileWatcherHandle {
            stop_tx,
            handle: Some(handle),
        }),
        Ok(Err(err)) => {
            let _ = stop_tx.send(());
            let _ = handle.join();
            Err(err)
        }
        Err(_) => {
            let _ = stop_tx.send(());
            let _ = handle.join();
            Err(notify::Error::generic(
                "config watcher initialization channel closed",
            ))
        }
    }
}

fn watch_config(
    path: PathBuf,
    mut watcher: RecommendedWatcher,
    event_rx: mpsc::Receiver<notify::Result<notify::Event>>,
    stop_rx: mpsc::Receiver<()>,
    sender: Sender<Command>,
    kind: ReloadConfigKind,
) {
    let debounce = Duration::from_millis(250);
    let mut last_emit = Instant::now() - debounce;

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        match event_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    if last_emit.elapsed() >= debounce =>
                {
                    if sender
                        .send(Command::ReloadConfig { kind, path: None })
                        .is_err()
                    {
                        break;
                    }
                    last_emit = Instant::now();
                }
                _ => {}
            },
            Ok(Err(err)) => {
                warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "simulation_config.watch_event_error"
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = watcher.unwatch(&path);
}

fn handle_reload_config(
    app: &mut bevy::prelude::App,
    kind: ReloadConfigKind,
    path: Option<String>,
) {
    match kind {
        ReloadConfigKind::Simulation => handle_reload_simulation_config(app, path),
        ReloadConfigKind::TurnPipeline => handle_reload_turn_pipeline_config(app, path),
        ReloadConfigKind::SnapshotOverlays => handle_reload_snapshot_overlays_config(app, path),
        ReloadConfigKind::CrisisArchetypes => handle_reload_crisis_archetypes_config(app, path),
        ReloadConfigKind::CrisisModifiers => handle_reload_crisis_modifiers_config(app, path),
        ReloadConfigKind::CrisisTelemetry => handle_reload_crisis_telemetry_config(app, path),
    }
}

/// Write the current world map (terrain snapshot + resolved seed/preset) to disk
/// as JSON for offline inspection and as a test fixture. Never panics: on any
/// failure it logs a warning and returns, leaving the simulation untouched.
fn write_map_export(app: &bevy::prelude::App, requested_path: Option<String>) {
    let snapshot = {
        let history = app.world.resource::<SnapshotHistory>();
        match history.last_snapshot.clone() {
            Some(snapshot) => snapshot,
            None => {
                warn!(
                    target: "shadow_scale::server",
                    "map.export.rejected=no_snapshot"
                );
                return;
            }
        }
    };

    // `spawn_initial_world` resolves the (possibly random) seed and writes it
    // back into `SimulationConfig.map_seed`, so the config is the seed's source
    // of truth by the time any command is handled.
    let (seed, preset) = {
        let config = app.world.resource::<SimulationConfig>();
        (config.map_seed, config.map_preset_id.clone())
    };
    let tick = snapshot.header.tick;

    let export = MapExport::from_snapshot(seed, preset, (*snapshot).clone());

    let path = match requested_path {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(DEFAULT_EXPORT_DIR).join(format!("map-tick{tick}-seed{seed}.json")),
    };

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                warn!(
                    target: "shadow_scale::server",
                    error = %err,
                    path = %path.display(),
                    "map.export.failed=create_dir"
                );
                return;
            }
        }
    }

    let json = match encode_map_export_json(&export) {
        Ok(json) => json,
        Err(err) => {
            warn!(
                target: "shadow_scale::server",
                error = %err,
                "map.export.failed=encode"
            );
            return;
        }
    };

    match std::fs::write(&path, json) {
        Ok(()) => info!(
            target: "shadow_scale::server",
            path = %path.display(),
            seed,
            tick,
            width = export.width,
            height = export.height,
            "map.export.completed"
        ),
        Err(err) => warn!(
            target: "shadow_scale::server",
            error = %err,
            path = %path.display(),
            "map.export.failed=write"
        ),
    }
}

fn handle_set_start_profile(app: &mut bevy::prelude::App, profile_id: String) {
    let handle = app.world.resource::<StartProfilesHandle>().clone();
    let (profile, used_fallback) = resolve_active_profile(&handle, &profile_id);

    {
        let mut config = app.world.resource_mut::<SimulationConfig>();
        config.start_profile_id = profile.id.clone();
        config.start_profile_overrides = StartProfileOverrides::from_profile(&profile);
    }
    {
        let mut lookup = app.world.resource_mut::<StartProfileLookup>();
        lookup.id = profile.id.clone();
    }
    {
        let mut active = app.world.resource_mut::<ActiveStartProfile>();
        *active = ActiveStartProfile::new(profile.clone());
    }
    {
        let mut label = app.world.resource_mut::<CampaignLabel>();
        *label = CampaignLabel::from_profile(&profile);
    }

    info!(
        target: "shadow_scale::campaign",
        requested = %profile_id,
        applied = %profile.id,
        fallback = used_fallback,
        "start_profile.updated"
    );

    if used_fallback {
        warn!(
            target: "shadow_scale::campaign",
            requested = %profile_id,
            applied = %profile.id,
            "start_profile.fallback_applied"
        );
    }
}

fn handle_scout_area(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target_x: u32,
    target_y: u32,
    band_entity_bits: Option<u64>,
) {
    let target = UVec2::new(target_x, target_y);
    let Some(tile_entity) = ensure_land_tile(
        app,
        faction,
        target,
        "scout_area",
        Some(CommandEventKind::Scout),
    ) else {
        return;
    };

    let Some(band) = select_starting_band(
        app,
        faction,
        band_entity_bits,
        "scout_area",
        CommandEventKind::Scout,
    ) else {
        return;
    };

    if !consume_faction_provisions(
        app,
        faction,
        SCOUT_PROVISION_COST,
        "scout_area",
        CommandEventKind::Scout,
    ) {
        return;
    }

    // Orders replace orders: this scout takes over the band from any prior task.
    reassign_band(app, band.entity);

    let home_coords = {
        let cohort = match app.world.get::<PopulationCohort>(band.entity) {
            Some(cohort) => cohort,
            None => {
                warn!(
                    target: "shadow_scale::command",
                    command = "scout_area",
                    faction = %faction.0,
                    band = %band.label,
                    "command.scout_area.rejected=band_missing"
                );
                return;
            }
        };
        app.world
            .get::<Tile>(cohort.home)
            .map(|tile| tile.position)
            .unwrap_or(target)
    };

    let radius_override = {
        let config = app.world.resource::<SimulationConfig>();
        config.start_profile_overrides.survey_radius
    };
    let reveal_radius = radius_override
        .unwrap_or(DEFAULT_SCOUT_REVEAL_RADIUS)
        .max(MIN_SCOUT_REVEAL_RADIUS);
    let tick = app.world.resource::<SimulationTick>().0;
    let distance = home_coords.x.abs_diff(target.x) + home_coords.y.abs_diff(target.y);
    let travel_turns = estimate_travel_turns(distance);
    let assignment = ScoutAssignment {
        faction,
        band_label: band.label.clone(),
        target_tile: tile_entity,
        target_coords: target,
        travel_remaining: travel_turns,
        travel_total: travel_turns,
        reveal_radius,
        reveal_duration: SCOUT_REVEAL_DURATION_TURNS,
        morale_gain: SCOUT_MORALE_GAIN.max(0.0),
        started_tick: tick,
    };
    app.world.entity_mut(band.entity).insert(assignment);
    let detail = format!(
        "band={} radius={} travel_turns={} morale_boost={:.2}",
        band.label,
        reveal_radius,
        travel_turns,
        SCOUT_MORALE_GAIN.max(0.0)
    );

    info!(
        target: "shadow_scale::command",
        command = "scout_area",
        faction = %faction.0,
        x = target_x,
        y = target_y,
        band = %band.label,
        radius = reveal_radius,
        travel_turns,
        "command.scout_area.enqueued"
    );

    push_command_event(
        app,
        tick,
        CommandEventKind::Scout,
        faction,
        format!("{} -> ({}, {})", band.label, target_x, target_y),
        Some(detail),
    );
}

/// Parse a follow policy string, warning (and defaulting to Sustain) when a
/// non-empty value fails to parse so a typo like `surpluss` is diagnosable rather
/// than silently accepted.
fn parse_follow_policy(policy: Option<&str>) -> FollowPolicy {
    match policy {
        Some(raw) if !raw.trim().is_empty() => raw.parse().unwrap_or_else(|_| {
            warn!(
                target: "shadow_scale::command",
                command = "follow_herd",
                policy = %raw,
                "command.follow_herd.policy_unrecognized=default_sustain"
            );
            FollowPolicy::default()
        }),
        _ => FollowPolicy::default(),
    }
}

fn handle_follow_herd(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    herd_id: String,
    policy: FollowPolicy,
    band_entity_bits: Option<u64>,
) {
    let Some(herd_position) = app
        .world
        .get_resource::<HerdRegistry>()
        .and_then(|registry| registry.find(&herd_id))
        .map(|herd| herd.position())
    else {
        warn!(
            target: "shadow_scale::command",
            command = "follow_herd",
            faction = %faction.0,
            herd = %herd_id,
            "command.follow_herd.rejected=unknown_herd"
        );
        emit_command_failure(
            app,
            CommandEventKind::FollowHerd,
            faction,
            format!("Herd '{}' no longer exists.", herd_id),
        );
        return;
    };

    let Some(band) = select_starting_band(
        app,
        faction,
        band_entity_bits,
        "follow_herd",
        CommandEventKind::FollowHerd,
    ) else {
        return;
    };

    // Orders replace orders: a band holds exactly one task at a time.
    reassign_band(app, band.entity);

    let tick = app.world.resource::<SimulationTick>().0;
    app.world.entity_mut(band.entity).insert(FaunaPursuit {
        faction,
        band_label: band.label.clone(),
        fauna_id: herd_id.clone(),
        mode: FaunaPursuitMode::Follow { policy },
        elapsed_turns: 0,
        started_tick: tick,
    });

    let detail = format!(
        "status=queued action=follow policy={} band={} herd={} herd_x={} herd_y={}",
        policy.as_str(),
        band.label,
        herd_id,
        herd_position.x,
        herd_position.y
    );
    info!(
        target: "shadow_scale::command",
        command = "follow_herd",
        faction = %faction.0,
        band = %band.label,
        herd = %herd_id,
        policy = policy.as_str(),
        "command.follow_herd.queued"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::FollowHerd,
        faction,
        format!("{} follow -> {}", band.label, herd_id),
        Some(detail),
    );
}

/// Orders replace orders: strip any single-band task (fauna pursuit / harvest /
/// scout) so a newly issued verb fully takes over the band.
fn reassign_band(app: &mut bevy::prelude::App, band: Entity) {
    let mut entity = app.world.entity_mut(band);
    // Snap the travel origin to where the band actually is before dropping its old
    // task: ETA/pathing (`queue_food_assignment`, scout) and `simulate_population`
    // key off `cohort.home`, so a band re-tasked mid-travel must not fall back to a
    // stale home tile (which would "snap" it back and mis-compute distances).
    if let Some(mut cohort) = entity.get_mut::<PopulationCohort>() {
        cohort.home = cohort.current_tile;
    }
    entity.remove::<FaunaPursuit>();
    entity.remove::<HarvestAssignment>();
    entity.remove::<ScoutAssignment>();
}

fn handle_found_camp(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target_x: u32,
    target_y: u32,
) {
    let target = UVec2::new(target_x, target_y);
    let Some(_tile_entity) = ensure_land_tile(
        app,
        faction,
        target,
        "found_camp",
        Some(CommandEventKind::FoundCamp),
    ) else {
        return;
    };
    if !consume_faction_provisions(
        app,
        faction,
        CAMP_PROVISION_COST,
        "found_camp",
        CommandEventKind::FoundCamp,
    ) {
        return;
    }
    let (survey_radius_override, fog_mode_override) = {
        let config = app.world.resource::<SimulationConfig>();
        (
            config.start_profile_overrides.survey_radius,
            config.start_profile_overrides.fog_mode,
        )
    };
    let tick = app.world.resource::<SimulationTick>().0;
    let applied_radius =
        if let Some(mut start_location) = app.world.get_resource_mut::<StartLocation>() {
            start_location.relocate(target);
            if start_location.survey_radius().is_none() {
                start_location.set_survey_radius(survey_radius_override);
            }
            if let Some(fog_mode) = fog_mode_override {
                start_location.set_fog_mode(fog_mode);
            }
            info!(
                target: "shadow_scale::command",
                faction = %faction.0,
                x = target_x,
                y = target_y,
                radius = ?start_location.survey_radius(),
                "command.found_camp.applied"
            );
            start_location.survey_radius()
        } else {
            warn!(
                target: "shadow_scale::command",
                faction = %faction.0,
                "command.found_camp.rejected=start_location_missing"
            );
            return;
        };

    if let Some(radius) = applied_radius {
        let expires_at = tick.saturating_add(SCOUT_REVEAL_DURATION_TURNS * 2);
        let mut reveals = app.world.resource_mut::<FogRevealLedger>();
        reveals.queue(target, radius.max(MIN_SCOUT_REVEAL_RADIUS), expires_at);
        push_command_event(
            app,
            tick,
            CommandEventKind::FoundCamp,
            faction,
            format!("Camp -> ({}, {})", target_x, target_y),
            Some(format!(
                "radius={} expires={} cost={} provisions",
                radius, expires_at, CAMP_PROVISION_COST
            )),
        );
    } else {
        push_command_event(
            app,
            tick,
            CommandEventKind::FoundCamp,
            faction,
            format!("Camp -> ({}, {})", target_x, target_y),
            Some(format!("cost={} provisions", CAMP_PROVISION_COST)),
        );
    }
}

fn handle_found_settlement(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target_x: u32,
    target_y: u32,
) {
    let target = UVec2::new(target_x, target_y);
    let Some(_tile_entity) = ensure_land_tile(
        app,
        faction,
        target,
        "found_settlement",
        Some(CommandEventKind::FoundSettlement),
    ) else {
        return;
    };

    // Reject if a settlement already exists on this tile.
    {
        let mut query = app.world.query::<&Settlement>();
        if query
            .iter(&app.world)
            .any(|settlement| settlement.position == target)
        {
            emit_command_failure(
                app,
                CommandEventKind::FoundSettlement,
                faction,
                "A settlement already exists at that location.",
            );
            return;
        }
    }

    // Require a Founders band to be present and consume it.
    let Some(founders) = select_founder_band(app, faction, CommandEventKind::FoundSettlement)
    else {
        return;
    };

    if !consume_faction_provisions(
        app,
        faction,
        SETTLEMENT_PROVISION_COST,
        "found_settlement",
        CommandEventKind::FoundSettlement,
    ) {
        return;
    }

    let removed = app.world.despawn(founders.entity);
    if !removed {
        warn!(
            target: "shadow_scale::command",
            command = "found_settlement",
            faction = %faction.0,
            entity_bits = founders.entity.to_bits(),
            "command.found_settlement.rejected=despawn_failed"
        );
        emit_command_failure(
            app,
            CommandEventKind::FoundSettlement,
            faction,
            "Failed to consume the Founders unit.",
        );
        return;
    }

    let construction_radius = SETTLEMENT_CONSTRUCTION_RADIUS;
    let logistics_radius = SETTLEMENT_LOGISTICS_RADIUS;

    let settlement_entity = app.world.spawn((
        Settlement {
            faction,
            position: target,
        },
        TownCenter {
            construction_radius,
            logistics_radius,
        },
    ));
    let settlement_id = settlement_entity.id();

    // Update start location and fog reveal based on the new hub.
    let tick = app.world.resource::<SimulationTick>().0;
    let applied_radius = {
        let survey_override = app
            .world
            .resource::<SimulationConfig>()
            .start_profile_overrides
            .survey_radius;
        let mut start_location = match app.world.get_resource_mut::<StartLocation>() {
            Some(res) => res,
            None => {
                warn!(
                    target: "shadow_scale::command",
                    command = "found_settlement",
                    faction = %faction.0,
                    "command.found_settlement.rejected=start_location_missing"
                );
                return;
            }
        };
        start_location.relocate(target);
        start_location
            .survey_radius()
            .or(survey_override)
            .or(Some(logistics_radius))
    };

    if let Some(radius) = applied_radius {
        let expires_at = tick.saturating_add(SCOUT_REVEAL_DURATION_TURNS * 2);
        let mut reveals = app.world.resource_mut::<FogRevealLedger>();
        reveals.queue(target, radius.max(MIN_SCOUT_REVEAL_RADIUS), expires_at);
    }

    push_command_event(
        app,
        tick,
        CommandEventKind::CampaignFounded,
        faction,
        format!("Settlement -> ({}, {})", target_x, target_y),
        Some(format!(
            "construction_radius={} logistics_radius={} cost={} provisions founders_entity={}",
            construction_radius,
            logistics_radius,
            SETTLEMENT_PROVISION_COST,
            settlement_id.to_bits()
        )),
    );
}

fn handle_forage_tile(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target_x: u32,
    target_y: u32,
    module_key: String,
    band_entity_bits: Option<u64>,
) {
    let coords = UVec2::new(target_x, target_y);
    let module = match FoodModule::from_str(module_key.trim()) {
        Ok(module) => module,
        Err(_) => {
            warn!(
                target: "shadow_scale::command",
                command = "forage",
                faction = %faction.0,
                module = %module_key,
                "command.forage.rejected=invalid_module"
            );
            emit_command_failure(
                app,
                CommandEventKind::Forage,
                faction,
                format!("Unknown food module '{}'.", module_key.trim()),
            );
            return;
        }
    };
    let tile_entity = {
        let registry = app.world.resource::<TileRegistry>();
        registry.index(target_x, target_y)
    };
    let Some(tile_entity) = tile_entity else {
        log_tile_rejection(
            app,
            faction,
            coords,
            "forage",
            "out_of_bounds",
            Some(CommandEventKind::Forage),
        );
        return;
    };
    let (tag_module, seasonal_weight) = {
        let entity_ref = app.world.entity(tile_entity);
        match entity_ref.get::<FoodModuleTag>() {
            Some(tag) => (tag.module, tag.seasonal_weight.max(0.0)),
            None => {
                log_tile_rejection(
                    app,
                    faction,
                    coords,
                    "forage",
                    "no_food_module",
                    Some(CommandEventKind::Forage),
                );
                return;
            }
        }
    };
    if tag_module != module {
        log_tile_rejection(
            app,
            faction,
            coords,
            "forage",
            "module_mismatch",
            Some(CommandEventKind::Forage),
        );
        return;
    }
    queue_food_assignment(
        app,
        faction,
        target_x,
        target_y,
        tile_entity,
        module,
        seasonal_weight,
        band_entity_bits,
        "forage",
        "Harvest",
        CommandEventKind::Forage,
        HarvestTaskKind::Harvest,
    );
}

fn handle_hunt_game(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    _target_x: u32,
    _target_y: u32,
    _band_entity_bits: Option<u64>,
) {
    // Tile-based hunting retired alongside `game_trail` (Wildlife & Hunting Overlay,
    // Phase A). Hunt is being reworked to target wild-game fauna groups in Phase B;
    // the command stays registered but has no tile target to resolve until then.
    warn!(
        target: "shadow_scale::command",
        command = "hunt_game",
        faction = %faction.0,
        "command.hunt_game.rejected=tile_hunting_retired"
    );
    emit_command_failure(
        app,
        CommandEventKind::Hunt,
        faction,
        "Tile hunting is retired; use `hunt_fauna <faction> <herd_id>` to hunt a fauna group.",
    );
}

/// Order a band to pursue and hunt a fauna group (herd) by id. Validates the herd,
/// resolves the band (auto-picks the first faction cohort when none is given),
/// rejects a busy band, then attaches a `FaunaPursuit` component. The pursuit itself
/// (travel to ≤1 tile, then take biomass → provisions/trade) resolves each turn in
/// `advance_fauna_pursuits`.
fn handle_hunt_fauna(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    herd_id: String,
    band_entity_bits: Option<u64>,
) {
    let Some(herd_position) = app
        .world
        .get_resource::<HerdRegistry>()
        .and_then(|registry| registry.find(&herd_id))
        .map(|herd| herd.position())
    else {
        warn!(
            target: "shadow_scale::command",
            command = "hunt_fauna",
            faction = %faction.0,
            herd = %herd_id,
            "command.hunt_fauna.rejected=unknown_herd"
        );
        emit_command_failure(
            app,
            CommandEventKind::Hunt,
            faction,
            format!("Herd '{}' no longer exists.", herd_id),
        );
        return;
    };

    let Some(band) = select_starting_band(
        app,
        faction,
        band_entity_bits,
        "hunt_fauna",
        CommandEventKind::Hunt,
    ) else {
        return;
    };

    // Orders replace orders: this hunt takes over the band from any prior task.
    reassign_band(app, band.entity);

    let tick = app.world.resource::<SimulationTick>().0;
    app.world.entity_mut(band.entity).insert(FaunaPursuit {
        faction,
        band_label: band.label.clone(),
        fauna_id: herd_id.clone(),
        mode: FaunaPursuitMode::Hunt,
        elapsed_turns: 0,
        started_tick: tick,
    });

    let detail = format!(
        "status=queued action=hunt band={} herd={} herd_x={} herd_y={}",
        band.label, herd_id, herd_position.x, herd_position.y
    );
    info!(
        target: "shadow_scale::command",
        command = "hunt_fauna",
        faction = %faction.0,
        band = %band.label,
        herd = %herd_id,
        "command.hunt_fauna.queued"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Hunt,
        faction,
        format!("{} Hunt -> {}", band.label, herd_id),
        Some(detail),
    );
}

/// Claim a herd as domesticated livestock. Requires the faction to have built husbandry
/// progress by Sustain-following it (so it owns the herd) and for that progress to have
/// reached `husbandry.claim_threshold`; otherwise the command is rejected. On success the
/// herd is finalized to domesticated (`claim_domestication`), after which it yields steady
/// provisions and is collapse-immune.
fn handle_domesticate(app: &mut bevy::prelude::App, faction: FactionId, herd_id: String) {
    enum Outcome {
        UnknownHerd,
        AlreadyDomesticated,
        NotOwner,
        NotTame(f32),
        Claimed,
    }

    let claim_threshold = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .husbandry
        .claim_threshold;

    // Decide (and, on success, mutate the herd) inside a scope so the registry borrow ends
    // before we emit command events through `app`.
    let outcome = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        match registry.herds.iter_mut().find(|herd| herd.id == herd_id) {
            None => Outcome::UnknownHerd,
            Some(herd) if herd.is_domesticated() => Outcome::AlreadyDomesticated,
            // Only the tending faction may claim; report ownership and tameness distinctly.
            Some(herd) if herd.owner != Some(faction) => Outcome::NotOwner,
            Some(herd) if herd.domestication_progress < claim_threshold => {
                Outcome::NotTame(herd.domestication_progress)
            }
            Some(herd) => {
                herd.claim_domestication(faction);
                Outcome::Claimed
            }
        }
    };

    match outcome {
        Outcome::UnknownHerd => {
            warn!(
                target: "shadow_scale::command",
                command = "domesticate",
                faction = %faction.0,
                herd = %herd_id,
                "command.domesticate.rejected=unknown_herd"
            );
            emit_command_failure(
                app,
                CommandEventKind::Domesticate,
                faction,
                format!("Herd '{}' no longer exists.", herd_id),
            );
        }
        Outcome::AlreadyDomesticated => emit_command_failure(
            app,
            CommandEventKind::Domesticate,
            faction,
            format!("{} is already domesticated.", herd_id),
        ),
        Outcome::NotOwner => emit_command_failure(
            app,
            CommandEventKind::Domesticate,
            faction,
            format!(
                "You are not tending {}. Sustain-follow it to build husbandry before claiming it.",
                herd_id
            ),
        ),
        Outcome::NotTame(progress) => emit_command_failure(
            app,
            CommandEventKind::Domesticate,
            faction,
            format!(
                "{} is not tame enough to domesticate ({}%). Keep Sustain-following it to build husbandry.",
                herd_id,
                (progress * 100.0).round() as i64
            ),
        ),
        Outcome::Claimed => {
            let tick = app.world.resource::<SimulationTick>().0;
            info!(
                target: "shadow_scale::command",
                command = "domesticate",
                faction = %faction.0,
                herd = %herd_id,
                "command.domesticate.claimed"
            );
            push_command_event(
                app,
                tick,
                CommandEventKind::Domesticate,
                faction,
                format!("Domesticated {}", herd_id),
                Some(format!(
                    "status=complete action=domesticate herd={}",
                    herd_id
                )),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn queue_food_assignment(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target_x: u32,
    target_y: u32,
    tile_entity: Entity,
    module: FoodModule,
    raw_weight: f32,
    band_entity_bits: Option<u64>,
    command_label: &str,
    label_prefix: &str,
    event_kind: CommandEventKind,
    task_kind: HarvestTaskKind,
) {
    let coords = UVec2::new(target_x, target_y);
    let seasonal_weight = raw_weight.max(0.0);
    let Some(band) =
        select_starting_band(app, faction, band_entity_bits, command_label, event_kind)
    else {
        return;
    };

    // Orders replace orders: this harvest takes over the band from any prior task.
    reassign_band(app, band.entity);

    let home_coords = app
        .world
        .get::<PopulationCohort>(band.entity)
        .and_then(|cohort| app.world.get::<Tile>(cohort.home).map(|tile| tile.position))
        .unwrap_or(coords);
    let distance = home_coords.x.abs_diff(coords.x) + home_coords.y.abs_diff(coords.y);
    let travel_turns = estimate_travel_turns(distance);
    let gather_turns = DEFAULT_HARVEST_WORK_TURNS.max(1);

    let overlays_handle = app.world.resource::<SnapshotOverlaysConfigHandle>();
    let food_config = overlays_handle.get();
    let food_cfg = food_config.food();
    let provisions_gain = (seasonal_weight * food_cfg.provisions_per_weight()).round() as i64;
    let trade_weight = food_cfg.trade_goods_per_weight() + food_cfg.trade_bonus_for(&module);
    let trade_goods_gain = (seasonal_weight * trade_weight).round() as i64;
    if provisions_gain <= 0 && trade_goods_gain <= 0 {
        log_tile_rejection(
            app,
            faction,
            coords,
            command_label,
            "no_yield",
            Some(event_kind),
        );
        return;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    let eta_turns = travel_turns + gather_turns;
    {
        let assignment = HarvestAssignment {
            faction,
            band_label: band.label.clone(),
            module,
            target_tile: tile_entity,
            target_coords: coords,
            travel_remaining: travel_turns,
            travel_total: travel_turns,
            gather_remaining: gather_turns,
            gather_total: gather_turns,
            provisions_reward: provisions_gain.max(0),
            trade_goods_reward: trade_goods_gain.max(0),
            started_tick: tick,
            kind: task_kind,
        };
        app.world.entity_mut(band.entity).insert(assignment);
    }

    let detail = format!(
        "status=queued action={} module={} band={} provisions={} trade_goods={} travel_turns={} gather_turns={}",
        task_kind.as_str(),
        module.as_str(),
        band.label,
        provisions_gain.max(0),
        trade_goods_gain.max(0),
        travel_turns,
        gather_turns
    );
    info!(
        target: "shadow_scale::command",
        command = command_label,
        faction = %faction.0,
        band = %band.label,
        x = target_x,
        y = target_y,
        module = module.as_str(),
        travel_turns,
        gather_turns,
        eta_turns,
        "command.food.queued"
    );
    push_command_event(
        app,
        tick,
        event_kind,
        faction,
        format!("{} -> ({}, {})", label_prefix, target_x, target_y),
        Some(detail),
    );
}

fn handle_reload_simulation_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let current_config = app.world.resource::<SimulationConfig>().clone();

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<SimulationConfigMetadata>()
                .path()
                .cloned()
        });

    let (new_config, applied_path) = match requested_path {
        Some(path) => match SimulationConfig::from_file(&path) {
            Ok(cfg) => (cfg, Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "simulation_config.reload_failed"
                );
                return;
            }
        },
        None => (SimulationConfig::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<SimulationConfigMetadata>();
        metadata.set_path(applied_path.clone());
        metadata.set_seed_random(new_config.map_seed == 0);
    }

    {
        let mut config_res = app.world.resource_mut::<SimulationConfig>();
        *config_res = new_config.clone();
    }

    {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.set_capacity(new_config.snapshot_history_limit.max(1));
    }

    let watch_path = app
        .world
        .resource::<SimulationConfigMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_simulation(watch_path, command_sender);
    }

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        grid_width = new_config.grid_size.x,
        grid_height = new_config.grid_size.y,
        "simulation_config.reloaded"
    );

    if new_config.grid_size != current_config.grid_size {
        warn!(
            target: "shadow_scale::config",
            old = ?current_config.grid_size,
            new = ?new_config.grid_size,
            "simulation_config.grid_size_changed=map_reset_recommended"
        );
    }

    if new_config.command_bind != current_config.command_bind
        || new_config.snapshot_bind != current_config.snapshot_bind
        || new_config.snapshot_flat_bind != current_config.snapshot_flat_bind
        || new_config.log_bind != current_config.log_bind
    {
        warn!(
            target: "shadow_scale::config",
            "simulation_config.socket_changed=restart_required"
        );
    }
}

fn handle_reload_turn_pipeline_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<TurnPipelineConfigMetadata>()
                .path()
                .cloned()
        });

    let (new_config, applied_path) = match requested_path {
        Some(path) => match TurnPipelineConfig::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "turn_pipeline_config.reload_failed"
                );
                return;
            }
        },
        None => (TurnPipelineConfig::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<TurnPipelineConfigMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<TurnPipelineConfigHandle>();
        handle.replace(Arc::clone(&new_config));
    }

    let watch_path = app
        .world
        .resource::<TurnPipelineConfigMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_turn_pipeline(watch_path, command_sender);
    }

    let logistics = new_config.logistics();
    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        flow_gain_min = logistics.flow_gain_min().to_f32(),
        flow_gain_max = logistics.flow_gain_max().to_f32(),
        "turn_pipeline_config.reloaded"
    );
}

fn handle_reload_snapshot_overlays_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<SnapshotOverlaysConfigMetadata>()
                .path()
                .cloned()
        });

    let (new_config, applied_path) = match requested_path {
        Some(path) => match SnapshotOverlaysConfig::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "snapshot_overlays_config.reload_failed"
                );
                return;
            }
        },
        None => (SnapshotOverlaysConfig::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<SnapshotOverlaysConfigMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<SnapshotOverlaysConfigHandle>();
        handle.replace(Arc::clone(&new_config));
    }

    let watch_path = app
        .world
        .resource::<SnapshotOverlaysConfigMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_snapshot_overlays(watch_path, command_sender);
    }

    let corruption = new_config.corruption();
    let military = new_config.military();

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        corruption_logistics_weight = corruption.logistics_weight().to_f32(),
        corruption_trade_weight = corruption.trade_weight().to_f32(),
        corruption_military_weight = corruption.military_weight().to_f32(),
        corruption_governance_weight = corruption.governance_weight().to_f32(),
        military_presence_weight = military.presence_weight().to_f32(),
        military_support_weight = military.support_weight().to_f32(),
        "snapshot_overlays_config.reloaded"
    );
}

fn handle_reload_crisis_archetypes_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<CrisisArchetypeCatalogMetadata>()
                .path()
                .cloned()
        });

    let (new_catalog, applied_path) = match requested_path {
        Some(path) => match CrisisArchetypeCatalog::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "crisis_archetypes.reload_failed"
                );
                return;
            }
        },
        None => (CrisisArchetypeCatalog::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<CrisisArchetypeCatalogMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<CrisisArchetypeCatalogHandle>();
        handle.replace(Arc::clone(&new_catalog));
    }

    let watch_path = app
        .world
        .resource::<CrisisArchetypeCatalogMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_crisis_archetypes(watch_path, command_sender);
    }

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        archetype_count = new_catalog.archetypes.len(),
        "crisis_archetypes.reloaded"
    );
}

fn handle_reload_crisis_modifiers_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<CrisisModifierCatalogMetadata>()
                .path()
                .cloned()
        });

    let (new_catalog, applied_path) = match requested_path {
        Some(path) => match CrisisModifierCatalog::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "crisis_modifiers.reload_failed"
                );
                return;
            }
        },
        None => (CrisisModifierCatalog::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<CrisisModifierCatalogMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<CrisisModifierCatalogHandle>();
        handle.replace(Arc::clone(&new_catalog));
    }

    let watch_path = app
        .world
        .resource::<CrisisModifierCatalogMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_crisis_modifiers(watch_path, command_sender);
    }

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        modifier_count = new_catalog.modifiers.len(),
        "crisis_modifiers.reloaded"
    );
}

fn handle_reload_crisis_telemetry_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<CrisisTelemetryConfigMetadata>()
                .path()
                .cloned()
        });

    let (new_config, applied_path) = match requested_path {
        Some(path) => match CrisisTelemetryConfig::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "crisis_telemetry_config.reload_failed"
                );
                return;
            }
        },
        None => (CrisisTelemetryConfig::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<CrisisTelemetryConfigMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<CrisisTelemetryConfigHandle>();
        handle.replace(Arc::clone(&new_config));
    }

    {
        let mut telemetry = app.world.resource_mut::<CrisisTelemetry>();
        telemetry.apply_config(new_config.as_ref());
    }

    let watch_path = app
        .world
        .resource::<CrisisTelemetryConfigMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_crisis_telemetry(watch_path, command_sender);
    }

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        ema_alpha = new_config.ema_alpha,
        gauge_count = new_config.gauges.len(),
        "crisis_telemetry_config.reloaded"
    );
}

fn command_from_payload(payload: ProtoCommandPayload) -> Option<Command> {
    match payload {
        ProtoCommandPayload::Turn { steps } => Some(Command::Turn(steps)),
        ProtoCommandPayload::ResetMap { width, height } => {
            Some(Command::ResetMap { width, height })
        }
        ProtoCommandPayload::Heat { entity_bits, delta } => Some(Command::Heat {
            entity: entity_bits,
            delta,
        }),
        ProtoCommandPayload::Orders {
            faction_id,
            directive,
        } => match directive {
            ProtoOrdersDirective::Ready => Some(Command::Orders {
                faction: FactionId(faction_id),
                orders: FactionOrders::end_turn(),
            }),
        },
        ProtoCommandPayload::Rollback { tick } => Some(Command::Rollback { tick }),
        ProtoCommandPayload::AxisBias { axis, value } => Some(Command::AxisBias {
            axis: axis as usize,
            value,
        }),
        ProtoCommandPayload::SupportInfluencer { id, magnitude } => {
            Some(Command::SupportInfluencer { id, magnitude })
        }
        ProtoCommandPayload::SuppressInfluencer { id, magnitude } => {
            Some(Command::SuppressInfluencer { id, magnitude })
        }
        ProtoCommandPayload::SupportInfluencerChannel {
            id,
            channel,
            magnitude,
        } => {
            let mapped = map_support_channel(channel)?;
            Some(Command::SupportInfluencerChannel {
                id,
                channel: mapped,
                magnitude,
            })
        }
        ProtoCommandPayload::SpawnInfluencer { scope, generation } => {
            let generation = generation.map(|value| value as GenerationId);
            Some(Command::SpawnInfluencer { scope, generation })
        }
        ProtoCommandPayload::InjectCorruption {
            subsystem,
            intensity,
            exposure_timer,
        } => {
            let exposure = if exposure_timer > u16::MAX as u32 {
                warn!(
                    "Proto command exposure_timer {} exceeds u16::MAX; clamping",
                    exposure_timer
                );
                u16::MAX
            } else {
                exposure_timer as u16
            };
            Some(Command::InjectCorruption {
                subsystem,
                intensity,
                exposure_timer: exposure,
            })
        }
        ProtoCommandPayload::UpdateEspionageGenerators { updates } => {
            Some(Command::UpdateEspionageGenerators { updates })
        }
        ProtoCommandPayload::QueueEspionageMission {
            mission_id,
            owner_faction,
            target_owner_faction,
            discovery_id,
            agent_handle,
            target_tier,
            scheduled_tick,
        } => {
            let params = QueueMissionParams {
                mission_id: EspionageMissionId::new(mission_id),
                owner: FactionId(owner_faction),
                target_owner: FactionId(target_owner_faction),
                discovery_id,
                agent: EspionageAgentHandle(agent_handle),
                target_tier,
                scheduled_tick: scheduled_tick.unwrap_or(0),
            };
            Some(Command::QueueEspionageMission { params })
        }
        ProtoCommandPayload::UpdateEspionageQueueDefaults {
            scheduled_tick_offset,
            target_tier,
        } => Some(Command::UpdateEspionageQueueDefaults {
            scheduled_tick_offset: scheduled_tick_offset.map(|value| value as u64),
            target_tier,
        }),
        ProtoCommandPayload::UpdateCounterIntelPolicy { faction, policy } => {
            match map_security_policy(policy) {
                Some(mapped) => Some(Command::UpdateCounterIntelPolicy {
                    faction: FactionId(faction),
                    policy: mapped,
                }),
                None => {
                    warn!(
                        target: "shadow_scale::server",
                        faction,
                        policy = ?policy,
                        "counter_intel_policy.update.invalid"
                    );
                    None
                }
            }
        }
        ProtoCommandPayload::AdjustCounterIntelBudget {
            faction,
            reserve,
            delta,
        } => {
            if reserve.is_none() && delta.is_none() {
                warn!(
                    target: "shadow_scale::server",
                    faction,
                    "counter_intel_budget.adjust.ignore_empty"
                );
                None
            } else {
                Some(Command::AdjustCounterIntelBudget {
                    faction: FactionId(faction),
                    reserve: reserve.map(scalar_from_f32),
                    delta: delta.map(scalar_from_f32),
                })
            }
        }
        ProtoCommandPayload::ReloadConfig { kind, path } => {
            Some(Command::ReloadConfig { kind, path })
        }
        ProtoCommandPayload::SetCrisisAutoSeed { enabled } => {
            Some(Command::SetCrisisAutoSeed { enabled })
        }
        ProtoCommandPayload::SpawnCrisis {
            faction_id,
            archetype_id,
        } => Some(Command::SpawnCrisis {
            faction: FactionId(faction_id),
            archetype_id,
        }),
        ProtoCommandPayload::SetStartProfile { profile_id } => {
            Some(Command::SetStartProfile { profile_id })
        }
        ProtoCommandPayload::ScoutArea {
            faction_id,
            target_x,
            target_y,
            band_entity_bits,
        } => Some(Command::ScoutArea {
            faction: FactionId(faction_id),
            target_x,
            target_y,
            band_entity_bits,
        }),
        ProtoCommandPayload::FollowHerd {
            faction_id,
            herd_id,
            policy,
            band_entity_bits,
        } => Some(Command::FollowHerd {
            faction: FactionId(faction_id),
            herd_id,
            policy: parse_follow_policy(policy.as_deref()),
            band_entity_bits,
        }),
        ProtoCommandPayload::FoundCamp {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::FoundCamp {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        ProtoCommandPayload::FoundSettlement {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::FoundSettlement {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        ProtoCommandPayload::ForageTile {
            faction_id,
            target_x,
            target_y,
            module,
            band_entity_bits,
        } => Some(Command::ForageTile {
            faction: FactionId(faction_id),
            target_x,
            target_y,
            module,
            band_entity_bits,
        }),
        ProtoCommandPayload::HuntGame {
            faction_id,
            target_x,
            target_y,
            band_entity_bits,
        } => Some(Command::HuntGame {
            faction: FactionId(faction_id),
            target_x,
            target_y,
            band_entity_bits,
        }),
        ProtoCommandPayload::HuntFauna {
            faction_id,
            herd_id,
            band_entity_bits,
        } => Some(Command::HuntFauna {
            faction: FactionId(faction_id),
            herd_id,
            band_entity_bits,
        }),
        ProtoCommandPayload::Domesticate {
            faction_id,
            herd_id,
        } => Some(Command::Domesticate {
            faction: FactionId(faction_id),
            herd_id,
        }),
        ProtoCommandPayload::ExportMap { path } => Some(Command::ExportMap { path }),
    }
}

fn map_support_channel(channel: ProtoSupportChannel) -> Option<SupportChannel> {
    match channel {
        ProtoSupportChannel::Popular => Some(SupportChannel::Popular),
        ProtoSupportChannel::Peer => Some(SupportChannel::Peer),
        ProtoSupportChannel::Institutional => Some(SupportChannel::Institutional),
        ProtoSupportChannel::Humanitarian => Some(SupportChannel::Humanitarian),
    }
}

fn map_security_policy(kind: SecurityPolicyKind) -> Option<SecurityPolicy> {
    Some(match kind {
        SecurityPolicyKind::Lenient => SecurityPolicy::Lenient,
        SecurityPolicyKind::Standard => SecurityPolicy::Standard,
        SecurityPolicyKind::Hardened => SecurityPolicy::Hardened,
        SecurityPolicyKind::Crisis => SecurityPolicy::Crisis,
    })
}

fn apply_heat(app: &mut bevy::prelude::App, entity_bits: u64, delta_raw: i64) {
    let entity_result = std::panic::catch_unwind(|| bevy::prelude::Entity::from_bits(entity_bits));
    let entity = match entity_result {
        Ok(entity) => entity,
        Err(_) => {
            warn!("Invalid entity bits for heat command: {}", entity_bits);
            return;
        }
    };
    if let Some(mut tile) = app.world.get_mut::<Tile>(entity) {
        tile.temperature += Scalar::from_raw(delta_raw);
    } else {
        warn!("Entity {} not found for heat command", entity_bits);
    }
}

fn ensure_land_tile(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    coords: UVec2,
    command_label: &str,
    event_kind: Option<CommandEventKind>,
) -> Option<Entity> {
    let tile_entity = {
        let registry = app.world.resource::<TileRegistry>();
        registry.index(coords.x, coords.y)
    };
    let Some(tile_entity) = tile_entity else {
        log_tile_rejection(
            app,
            faction,
            coords,
            command_label,
            "out_of_bounds",
            event_kind,
        );
        return None;
    };
    let Some(tile) = app.world.get::<Tile>(tile_entity) else {
        log_tile_rejection(
            app,
            faction,
            coords,
            command_label,
            "tile_missing",
            event_kind,
        );
        return None;
    };
    if tile.terrain_tags.contains(TerrainTags::WATER) {
        log_tile_rejection(
            app,
            faction,
            coords,
            command_label,
            "water_tile",
            event_kind,
        );
        return None;
    }
    Some(tile_entity)
}

fn resolve_starting_unit_entity(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    entity_bits: u64,
    command_label: &str,
    event_kind: CommandEventKind,
) -> Option<Entity> {
    let Some(entity) = entity_from_bits(entity_bits) else {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            "command.starting_unit.rejected=invalid_entity_bits"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Unit id {} is invalid.", entity_bits),
        );
        return None;
    };
    if !app.world.entities().contains(entity) {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            entity_bits,
            "command.starting_unit.rejected=entity_not_found"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Unit id {} does not exist in the simulation.", entity_bits),
        );
        return None;
    }
    if app.world.get::<StartingUnit>(entity).is_none() {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            entity_bits,
            "command.starting_unit.rejected=entity_not_starting_unit"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Unit id {} is not a controllable band.", entity_bits),
        );
        return None;
    }
    Some(entity)
}

struct SelectedBand {
    entity: Entity,
    label: String,
}

fn select_starting_band(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_entity_bits: Option<u64>,
    command_label: &str,
    event_kind: CommandEventKind,
) -> Option<SelectedBand> {
    if let Some(bits) = band_entity_bits {
        let entity = resolve_starting_unit_entity(app, faction, bits, command_label, event_kind)?;
        return Some(SelectedBand {
            entity,
            label: starting_unit_label(app, entity),
        });
    }

    let mut query = app
        .world
        .query::<(Entity, &PopulationCohort, &StartingUnit)>();
    for (entity, cohort, unit) in query.iter(&app.world) {
        if cohort.faction == faction {
            return Some(SelectedBand {
                entity,
                label: unit.kind.clone(),
            });
        }
    }

    warn!(
        target: "shadow_scale::command",
        command = command_label,
        faction = %faction.0,
        "command.starting_unit.rejected=none_available"
    );
    emit_command_failure(
        app,
        event_kind,
        faction,
        "No available bands can accept this order right now.",
    );
    None
}

fn select_founder_band(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    event_kind: CommandEventKind,
) -> Option<SelectedBand> {
    let mut query = app
        .world
        .query::<(Entity, &PopulationCohort, &StartingUnit)>();
    for (entity, cohort, unit) in query.iter(&app.world) {
        if cohort.faction == faction && unit.kind.eq_ignore_ascii_case("founders") {
            return Some(SelectedBand {
                entity,
                label: unit.kind.clone(),
            });
        }
    }

    emit_command_failure(
        app,
        event_kind,
        faction,
        "No Founders unit is available to found a settlement.",
    );
    None
}

fn starting_unit_label(app: &bevy::prelude::App, entity: Entity) -> String {
    app.world
        .get::<StartingUnit>(entity)
        .map(|unit| unit.kind.clone())
        .unwrap_or_else(|| format!("starting_unit:{}", entity.index()))
}

/// Charge a provisions cost from the faction's bands' local larders (food is band-local now — the
/// supply network keeps networked bands topped up). Sums the faction's carried food; on shortfall
/// rejects with a command-feed failure, otherwise draws the cost greedily across bands in a
/// deterministic order.
fn consume_faction_provisions(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    amount: i64,
    command_label: &str,
    event_kind: CommandEventKind,
) -> bool {
    if amount <= 0 {
        return true;
    }
    let mut bands: Vec<(Entity, Scalar)> = Vec::new();
    {
        let mut query = app.world.query::<(Entity, &PopulationCohort)>();
        for (entity, cohort) in query.iter(&app.world) {
            if cohort.faction == faction {
                bands.push((entity, cohort.stores.get(FOOD)));
            }
        }
    }
    bands.sort_by_key(|(entity, _)| entity.to_bits());
    let available = bands
        .iter()
        .fold(Scalar::from_i64(0), |acc, (_, food)| acc + *food);
    let cost = Scalar::from_i64(amount);
    if available < cost {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            item = "provisions",
            required = amount,
            available = available.to_i64_whole(),
            "command.inventory.rejected=insufficient"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!(
                "{} provisions required but only {} available.",
                amount,
                available.to_i64_whole()
            ),
        );
        return false;
    }
    let mut remaining = cost;
    for (entity, _) in bands {
        if remaining <= Scalar::from_i64(0) {
            break;
        }
        if let Some(mut cohort) = app.world.get_mut::<PopulationCohort>(entity) {
            remaining -= cohort.stores.take(FOOD, remaining);
        }
    }
    true
}

fn push_command_event(
    app: &mut bevy::prelude::App,
    tick: u64,
    kind: CommandEventKind,
    faction: FactionId,
    label: String,
    detail: Option<String>,
) {
    if let Some(mut log) = app.world.get_resource_mut::<CommandEventLog>() {
        log.push(CommandEventEntry::new(tick, kind, faction, label, detail));
    }
}

fn emit_command_failure(
    app: &mut bevy::prelude::App,
    kind: CommandEventKind,
    faction: FactionId,
    detail: impl Into<String>,
) {
    let tick = app.world.resource::<SimulationTick>().0;
    let summary = format!("{} failed", command_kind_display(kind));
    push_command_event(app, tick, kind, faction, summary, Some(detail.into()));
}

fn command_kind_display(kind: CommandEventKind) -> &'static str {
    match kind {
        CommandEventKind::Scout => "Scout",
        CommandEventKind::FollowHerd => "Follow herd",
        CommandEventKind::FoundCamp => "Found camp",
        CommandEventKind::FoundSettlement => "Found settlement",
        CommandEventKind::CampaignFounded => "Campaign founded",
        CommandEventKind::CampaignMilestone => "Campaign milestone",
        CommandEventKind::CampaignVictory => "Campaign victory",
        CommandEventKind::Forage => "Harvest",
        CommandEventKind::Hunt => "Hunt",
        CommandEventKind::Domesticate => "Domesticate",
        CommandEventKind::SedentarizationPrompt => "Sedentarization",
    }
}

fn entity_from_bits(bits: u64) -> Option<Entity> {
    std::panic::catch_unwind(|| bevy::prelude::Entity::from_bits(bits)).ok()
}

fn log_tile_rejection(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    coords: UVec2,
    command_label: &str,
    reason: &str,
    event_kind: Option<CommandEventKind>,
) {
    warn!(
        target: "shadow_scale::command",
        command = command_label,
        faction = %faction.0,
        x = coords.x,
        y = coords.y,
        reason,
        "command.tile_validation.failed"
    );
    if let Some(kind) = event_kind {
        let human_reason = describe_tile_rejection(reason);
        let detail = format!(
            "Tile ({}, {}): {} ({})",
            coords.x, coords.y, human_reason, reason
        );
        emit_command_failure(app, kind, faction, detail);
    }
}

fn describe_tile_rejection(reason: &str) -> &'static str {
    match reason {
        "out_of_bounds" => "Destination is outside the playable area",
        "tile_missing" => "Tile data is unavailable",
        "water_tile" => "Cannot perform this action on a water tile",
        "no_food_module" => "Tile lacks a harvestable food source",
        "module_mismatch" => "Food source does not match the requested type",
        "no_yield" => "This site has no remaining seasonal yield",
        _ => "Tile is not valid for this command",
    }
}

fn broadcast_command_events_if_needed(
    app: &mut bevy::prelude::App,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let events_state = {
        let log = app.world.resource::<CommandEventLog>();
        command_events_to_state(log)
    };

    let mut history = app.world.resource_mut::<SnapshotHistory>();
    if let Some((binary, flat)) = history.update_command_events(events_state) {
        if let Some(server) = snapshot_server_bin {
            server.broadcast(binary.as_ref());
        }
        if let Some(server) = snapshot_server_flat {
            server.broadcast(flat.as_ref());
        }
    }
}

fn estimate_travel_turns(distance: u32) -> u32 {
    if distance == 0 {
        return 0;
    }
    let speed = DEFAULT_HARVEST_TRAVEL_TILES_PER_TURN.max(0.25);
    ((distance as f32 / speed).ceil() as u32).max(1)
}

fn handle_order_submission(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    orders: FactionOrders,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let order_count = orders.orders.len();
    let result = {
        let mut queue = app.world.resource_mut::<TurnQueue>();
        queue.submit_orders(faction, orders)
    };

    match result {
        Ok(SubmitOutcome::Accepted { remaining }) => info!(
            target: "shadow_scale::server",
            %faction,
            order_count,
            remaining,
            "orders.accepted"
        ),
        Ok(SubmitOutcome::ReadyToResolve) => {
            info!(
                target: "shadow_scale::server",
                %faction,
                order_count,
                "orders.ready_to_resolve"
            );
            resolve_ready_turn(app, snapshot_server_bin, snapshot_server_flat);
        }
        Err(SubmitError::UnknownFaction(f)) => warn!(
            target: "shadow_scale::server",
            %f,
            "orders.rejected=unknown_faction"
        ),
        Err(SubmitError::DuplicateSubmission(f)) => warn!(
            target: "shadow_scale::server",
            %f,
            "orders.rejected=duplicate_submission"
        ),
    }
}

fn handle_axis_bias(
    app: &mut bevy::prelude::App,
    axis: usize,
    value: f32,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    if axis >= 4 {
        warn!(
            target: "shadow_scale::server",
            axis,
            "axis_bias.rejected=invalid_axis"
        );
        return;
    }

    let clamped = value.clamp(-1.0, 1.0);
    {
        let mut bias_res = app.world.resource_mut::<SentimentAxisBias>();
        bias_res.set_policy_axis(axis, Scalar::from_f32(clamped));
    }

    let bias_state = {
        let bias_res = app.world.resource::<SentimentAxisBias>();
        let raw = bias_res.as_raw();
        AxisBiasState {
            knowledge: raw[0],
            trust: raw[1],
            equity: raw[2],
            agency: raw[3],
        }
    };

    let broadcast_payload = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.update_axis_bias(bias_state)
    };

    if let Some((binary, flat)) = broadcast_payload {
        if let Some(server) = snapshot_server_bin {
            server.broadcast(binary.as_ref());
        }
        if let Some(server) = snapshot_server_flat {
            server.broadcast(flat.as_ref());
        }
    }

    if let Some(server) = snapshot_server_flat {
        let history = app.world.resource::<SnapshotHistory>();
        if let Some(snapshot_bytes) = history.encoded_snapshot_flat.as_ref() {
            server.broadcast(snapshot_bytes.as_ref());
        }
    }

    info!(
        target: "shadow_scale::server",
        axis,
        value = clamped,
        "axis_bias.updated"
    );
}

fn handle_influencer_channel_support(
    app: &mut bevy::prelude::App,
    id: u32,
    channel: SupportChannel,
    magnitude: f32,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let clamped = magnitude.clamp(0.1, 5.0);
    let scalar_amount = Scalar::from_f32(clamped);
    let applied = {
        let mut roster = app.world.resource_mut::<InfluentialRoster>();
        roster.apply_channel_support(id, channel, scalar_amount)
    };

    if !applied {
        warn!(
            target: "shadow_scale::server",
            id,
            channel = channel.as_str(),
            magnitude = clamped,
            "influencer.channel_support.rejected=unknown_id"
        );
        return;
    }

    broadcast_influencer_update(app, snapshot_server_bin, snapshot_server_flat);

    info!(
        target: "shadow_scale::server",
        id,
        channel = channel.as_str(),
        magnitude = clamped,
        "influencer.channel_support.applied"
    );
}

fn handle_influencer_spawn(
    app: &mut bevy::prelude::App,
    scope: Option<InfluenceScopeKind>,
    generation: Option<GenerationId>,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let registry_snapshot = app.world.resource::<GenerationRegistry>().clone();
    let spawned = {
        let mut roster = app.world.resource_mut::<InfluentialRoster>();
        roster.force_spawn(scope, generation, &registry_snapshot)
    };

    let Some(new_id) = spawned else {
        warn!(
            target: "shadow_scale::server",
            scope = ?scope,
            generation = ?generation,
            "influencer.spawn.rejected"
        );
        return;
    };

    broadcast_influencer_update(app, snapshot_server_bin, snapshot_server_flat);

    let label = {
        let roster = app.world.resource::<InfluentialRoster>();
        roster
            .states()
            .into_iter()
            .find(|state| state.id == new_id)
            .map(|state| state.name)
            .unwrap_or_else(|| "unknown".to_string())
    };

    info!(
        target: "shadow_scale::server",
        id = new_id,
        scope = ?scope,
        generation = ?generation,
        name = label.as_str(),
        "influencer.spawn.manual"
    );
}

fn broadcast_influencer_update(
    app: &mut bevy::prelude::App,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let (states, sentiment_totals, logistics_total, morale_total, power_total) = {
        let roster = app.world.resource::<InfluentialRoster>();
        (
            roster.states(),
            roster.sentiment_totals(),
            roster.logistics_total(),
            roster.morale_total(),
            roster.power_total(),
        )
    };

    {
        let mut impacts = app.world.resource_mut::<InfluencerImpacts>();
        impacts.set_from_totals(logistics_total, morale_total, power_total);
    }

    {
        let mut bias_res = app.world.resource_mut::<SentimentAxisBias>();
        bias_res.set_influencer(sentiment_totals);
    }

    let bias_state = {
        let bias_res = app.world.resource::<SentimentAxisBias>();
        let raw = bias_res.as_raw();
        AxisBiasState {
            knowledge: raw[0],
            trust: raw[1],
            equity: raw[2],
            agency: raw[3],
        }
    };

    let (influencer_delta, bias_delta) = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        let influencer_delta = history.update_influencers(states);
        let bias_delta = history.update_axis_bias(bias_state);
        (influencer_delta, bias_delta)
    };

    if let Some((bin, flat)) = influencer_delta {
        if let Some(server) = snapshot_server_bin {
            server.broadcast(bin.as_ref());
        }
        if let Some(server) = snapshot_server_flat {
            server.broadcast(flat.as_ref());
        }
    }
    if let Some((bin, flat)) = bias_delta {
        if let Some(server) = snapshot_server_bin {
            server.broadcast(bin.as_ref());
        }
        if let Some(server) = snapshot_server_flat {
            server.broadcast(flat.as_ref());
        }
    }

    if let Some(server) = snapshot_server_flat {
        let history = app.world.resource::<SnapshotHistory>();
        if let Some(snapshot_bytes) = history.encoded_snapshot_flat.as_ref() {
            server.broadcast(snapshot_bytes.as_ref());
        }
    }
}

fn handle_influencer_command(
    app: &mut bevy::prelude::App,
    id: u32,
    magnitude: f32,
    action: InfluencerAction,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let clamped = magnitude.clamp(0.1, 5.0);
    let scalar_amount = Scalar::from_f32(clamped);

    let applied = {
        let mut roster = app.world.resource_mut::<InfluentialRoster>();
        match action {
            InfluencerAction::Support => roster.apply_support(id, scalar_amount),
            InfluencerAction::Suppress => roster.apply_suppress(id, scalar_amount),
        }
    };

    if !applied {
        warn!(
            target: "shadow_scale::server",
            id,
            magnitude = clamped,
            "influencer.command.rejected=unknown_id"
        );
        return;
    }

    broadcast_influencer_update(app, snapshot_server_bin, snapshot_server_flat);

    match action {
        InfluencerAction::Support => info!(
            target: "shadow_scale::server",
            id,
            magnitude = clamped,
            "influencer.support.applied"
        ),
        InfluencerAction::Suppress => info!(
            target: "shadow_scale::server",
            id,
            magnitude = clamped,
            "influencer.suppress.applied"
        ),
    }
}

fn handle_inject_corruption(
    app: &mut bevy::prelude::App,
    subsystem: CorruptionSubsystem,
    intensity: f32,
    exposure_timer: u16,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let clamped_intensity = intensity.clamp(-5.0, 5.0);
    let timer = exposure_timer.max(1);
    let restitution = timer.saturating_add(4);
    let tick = app.world.resource::<SimulationTick>().0;

    let (ledger_clone, incident_id) = {
        let mut ledgers = app.world.resource_mut::<CorruptionLedgers>();
        let ledger = ledgers.ledger_mut();
        let incident_id = (tick << 32) | (((ledger.entry_count() as u64) + 1) & 0xFFFF_FFFF);
        let entry = CorruptionEntry {
            subsystem,
            intensity: Scalar::from_f32(clamped_intensity).raw(),
            incident_id,
            exposure_timer: timer,
            restitution_window: restitution,
            last_update_tick: tick,
        };
        ledger.register_incident(entry);
        (ledger.clone(), incident_id)
    };

    let delta_payload = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.update_corruption(ledger_clone)
    };

    if let Some((binary, flat)) = delta_payload {
        if let Some(server) = snapshot_server_bin {
            server.broadcast(binary.as_ref());
        }
        if let Some(server) = snapshot_server_flat {
            server.broadcast(flat.as_ref());
        }
    }

    if let Some(server) = snapshot_server_flat {
        let history = app.world.resource::<SnapshotHistory>();
        if let Some(snapshot_bytes) = history.encoded_snapshot_flat.as_ref() {
            server.broadcast(snapshot_bytes.as_ref());
        }
    }

    info!(
        target: "shadow_scale::server",
        ?subsystem,
        intensity = clamped_intensity,
        exposure_timer = timer,
        incident_id,
        "corruption.injected"
    );
}

fn handle_update_espionage_generators(
    app: &mut bevy::prelude::App,
    updates: Vec<CommandGeneratorUpdate>,
) {
    if updates.is_empty() {
        info!(
            target: "shadow_scale::espionage",
            "espionage.generator.update_skipped=no_updates"
        );
        return;
    }

    let factions: Vec<FactionId> = {
        let registry = app.world.resource::<FactionRegistry>();
        registry.factions.clone()
    };

    let mut catalog = app.world.resource_mut::<EspionageCatalog>();
    let mut changed = false;

    for update in updates {
        let template_id = update.template_id;
        let enabled = update.enabled;
        let per_faction = update.per_faction;
        let applied = catalog.update_agent_generator(template_id.as_str(), enabled, per_faction);
        if applied {
            changed = true;
            info!(
                target: "shadow_scale::espionage",
                template_id,
                enabled = ?enabled,
                per_faction = ?per_faction,
                "espionage.generator.updated"
            );
        } else {
            warn!(
                target: "shadow_scale::espionage",
                template_id,
                "espionage.generator.update_failed=unknown_template"
            );
        }
    }
    if !changed {
        return;
    }

    app.world
        .resource_scope(|world, mut roster: bevy::prelude::Mut<EspionageRoster>| {
            let catalog = world.resource::<EspionageCatalog>();
            roster.refresh_generated_agents(catalog, &factions);
        });

    info!(
        target: "shadow_scale::espionage",
        factions = factions.len(),
        "espionage.generators.reseeded"
    );
}

fn handle_update_queue_defaults(
    app: &mut bevy::prelude::App,
    scheduled_tick_offset: Option<u64>,
    target_tier: Option<u8>,
) {
    if scheduled_tick_offset.is_none() && target_tier.is_none() {
        info!(
            target: "shadow_scale::espionage",
            "espionage.queue_defaults.update_skipped=no_fields"
        );
        return;
    }

    let mut catalog = app.world.resource_mut::<EspionageCatalog>();
    let mut defaults = catalog.config().queue_defaults().clone();

    if let Some(offset) = scheduled_tick_offset {
        defaults.scheduled_tick_offset = offset;
    }

    if target_tier.is_some() {
        defaults.target_tier = target_tier;
    }

    catalog.update_queue_defaults(defaults.clone());

    info!(
        target: "shadow_scale::espionage",
        scheduled_tick_offset = defaults.scheduled_tick_offset,
        target_tier = ?defaults.target_tier,
        "espionage.queue_defaults.updated"
    );
}

fn handle_update_counter_intel_policy(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    policy: SecurityPolicy,
) {
    let mut policies = app.world.resource_mut::<FactionSecurityPolicies>();
    policies.set_policy(faction, policy);
    info!(
        target: "shadow_scale::espionage",
        %faction,
        ?policy,
        "counter_intel.policy.updated"
    );
}

fn handle_adjust_counter_intel_budget(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    reserve: Option<Scalar>,
    delta: Option<Scalar>,
) {
    if reserve.is_none() && delta.is_none() {
        warn!(
            target: "shadow_scale::espionage",
            %faction,
            "counter_intel_budget.adjust.noop"
        );
        return;
    }

    let budget_config = {
        let catalog = app.world.resource::<EspionageCatalog>();
        catalog.config().counter_intel_budget().clone()
    };

    let mut budgets = app.world.resource_mut::<CounterIntelBudgets>();
    let mut updated = budgets.available(faction);

    if let Some(value) = reserve {
        updated = budgets.set_reserve(faction, value, &budget_config);
    }
    if let Some(value) = delta {
        updated = budgets.adjust_reserve(faction, value, &budget_config);
    }

    info!(
        target: "shadow_scale::espionage",
        %faction,
        reserve = reserve.map(|v| v.to_f32()),
        delta = delta.map(|v| v.to_f32()),
        available = updated.to_f32(),
        "counter_intel_budget.adjusted"
    );
}

const AUTO_AGENT_HANDLE: u32 = u32::MAX;

fn handle_queue_espionage_mission(app: &mut bevy::prelude::App, mut params: QueueMissionParams) {
    let current_tick = app.world.resource::<SimulationTick>().0;
    let defaults = {
        let catalog = app.world.resource::<EspionageCatalog>();
        catalog.config().queue_defaults().clone()
    };

    if params.scheduled_tick == 0 {
        params.scheduled_tick = current_tick.saturating_add(defaults.scheduled_tick_offset);
    }

    if params.target_tier.is_none() {
        params.target_tier = defaults.target_tier;
    }

    let mission_id = params.mission_id.0.clone();
    let owner = params.owner.0;
    let target_owner = params.target_owner.0;
    let auto_agent_requested = params.agent.0 == AUTO_AGENT_HANDLE;
    let mut selected_agent = params.agent;

    let queue_result = app.world.resource_scope(
        |world, mut missions: bevy::prelude::Mut<EspionageMissionState>| {
            let mut queued_params = params.clone();
            world.resource_scope(|world, mut roster: bevy::prelude::Mut<EspionageRoster>| {
                let catalog = world.resource::<EspionageCatalog>();

                if queued_params.agent.0 == AUTO_AGENT_HANDLE {
                    let template = match catalog.mission(&queued_params.mission_id) {
                        Some(template) => template,
                        None => {
                            return Err(QueueMissionError::UnknownMission(
                                queued_params.mission_id.0.clone(),
                            ));
                        }
                    };

                    let Some(handle) =
                        pick_best_agent_for_mission(&roster, queued_params.owner, template)
                    else {
                        return Err(QueueMissionError::NoAgentAvailable {
                            faction: queued_params.owner,
                        });
                    };

                    queued_params.agent = handle;
                }

                selected_agent = queued_params.agent;
                missions.queue_mission(catalog, &mut roster, queued_params)
            })
        },
    );

    match queue_result {
        Ok(instance_id) => {
            info!(
                target: "shadow_scale::espionage",
                mission_id,
                owner_faction = owner,
                target_owner,
                discovery_id = params.discovery_id,
                agent_handle = selected_agent.0,
                target_tier = ?params.target_tier,
                scheduled_tick = params.scheduled_tick,
                instance = instance_id.0,
                auto_agent = auto_agent_requested,
                "espionage.mission.queued"
            );
        }
        Err(error) => {
            warn!(
                target: "shadow_scale::espionage",
                mission_id,
                owner_faction = owner,
                target_owner,
                discovery_id = params.discovery_id,
                agent_handle = selected_agent.0,
                target_tier = ?params.target_tier,
                scheduled_tick = params.scheduled_tick,
                %error,
                "espionage.mission.queue_failed"
            );
        }
    }
}

fn pick_best_agent_for_mission(
    roster: &EspionageRoster,
    faction: FactionId,
    template: &EspionageMissionTemplate,
) -> Option<EspionageAgentHandle> {
    let mut best: Option<(EspionageAgentHandle, f32)> = None;

    for agent in roster.agents_for(faction) {
        if agent.assignment != AgentAssignment::Available {
            continue;
        }

        let score = match template.kind {
            EspionageMissionKind::Probe => {
                agent.stealth.to_f32() * template.stealth_weight.to_f32()
                    + agent.recon.to_f32() * template.recon_weight.to_f32()
            }
            EspionageMissionKind::CounterIntel => {
                agent.counter_intel.to_f32() * template.counter_intel_weight.to_f32()
            }
        };

        let is_better = match &best {
            Some((_, best_score)) => score > *best_score,
            None => true,
        };

        if is_better {
            best = Some((agent.handle, score));
        }
    }

    best.map(|(handle, _)| handle)
}

fn resolve_ready_turn(
    app: &mut bevy::prelude::App,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let turn_start = std::time::Instant::now();
    let ready_orders = {
        let mut queue = app.world.resource_mut::<TurnQueue>();
        if !queue.is_ready() {
            warn!(
                target: "shadow_scale::server",
                awaiting = ?queue.awaiting(),
                "turn.resolve_skipped=awaiting_orders"
            );
            return;
        }
        queue.drain_ready_orders()
    };

    apply_orders(&ready_orders);
    run_turn(app);

    {
        let mut queue = app.world.resource_mut::<TurnQueue>();
        queue.advance_turn();
    }

    let history = app.world.resource::<SnapshotHistory>();
    broadcast_latest(snapshot_server_bin, snapshot_server_flat, history);

    let metrics = app.world.resource::<SimulationMetrics>();
    let duration_ms = turn_start.elapsed().as_secs_f64() * 1000.0;
    info!(
        target: "shadow_scale::server",
        turn = metrics.turn,
        grid_width = metrics.grid_size.0,
        grid_height = metrics.grid_size.1,
        total_mass = metrics.total_mass,
        avg_temp = metrics.avg_temperature,
        duration_ms,
        "turn.completed"
    );
}

fn apply_orders(submissions: &[(FactionId, FactionOrders)]) {
    for (faction, orders) in submissions {
        info!(
            target: "shadow_scale::server",
            %faction,
            directives = orders.orders.len(),
            "orders.applied"
        );
    }
}

fn handle_rollback(
    app: &mut bevy::prelude::App,
    tick: u64,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    let entry: Option<StoredSnapshot> = {
        let history = app.world.resource::<SnapshotHistory>();
        history.entry(tick)
    };

    let Some(entry) = entry else {
        warn!(
            target: "shadow_scale::server",
            tick,
            "rollback.failed=missing_snapshot"
        );
        return;
    };

    restore_world_from_snapshot(&mut app.world, entry.snapshot.as_ref());
    {
        let mut tick_res = app.world.resource_mut::<SimulationTick>();
        tick_res.0 = tick;
    }
    {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.reset_to_entry(&entry);
    }

    warn!(
        target: "shadow_scale::server",
        tick,
        "rollback.completed -- clients should reconnect to receive fresh state"
    );

    if let Some(server) = snapshot_server_bin {
        server.broadcast(entry.encoded_snapshot.as_ref());
    }
    if let Some(server) = snapshot_server_flat {
        server.broadcast(entry.encoded_snapshot_flat.as_ref());
    }
}
