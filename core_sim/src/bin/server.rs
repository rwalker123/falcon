use std::io::{self, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use bevy::{
    ecs::system::Resource,
    math::UVec2,
    prelude::{Entity, With},
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{info, warn};
use tracing_subscriber::prelude::*;

use core_sim::log_stream::start_log_stream_server;

use core_sim::grid_utils::hex_distance_wrapped;
use core_sim::metrics::SimulationMetrics;
use core_sim::network::{broadcast_latest, start_snapshot_server, SnapshotServer};
use core_sim::{
    apply_port_base_override, available_workers, forage_source_yield_preview,
    hunt_source_yield_preview, output_multiplier, resolve_active_profile, ActiveStartProfile,
    BandTravel, CampaignLabel, Expedition, ExpeditionConfigHandle, ExpeditionMission,
    ExpeditionPhase, FoodModuleTag, LaborAllocation, LaborTarget, LocalStore, ResidentBand,
    StartProfileOverrides, WellbeingConfigHandle,
};
use core_sim::{
    build_headless_app, hunt_trip_forecast, recapture_snapshot_in_place,
    restore_world_from_snapshot, run_turn, scalar_from_f32, AgentAssignment, CommandEventEntry,
    CommandEventKind, CommandEventLog, CorruptionLedgers, CounterIntelBudgets,
    CrisisArchetypeCatalog, CrisisArchetypeCatalogHandle, CrisisArchetypeCatalogMetadata,
    CrisisModifierCatalog, CrisisModifierCatalogHandle, CrisisModifierCatalogMetadata,
    CrisisTelemetry, CrisisTelemetryConfig, CrisisTelemetryConfigHandle,
    CrisisTelemetryConfigMetadata, DiscoveryProgressLedger, EcologyPhase, EspionageAgentHandle,
    EspionageCatalog, EspionageMissionId, EspionageMissionKind, EspionageMissionState,
    EspionageMissionTemplate, EspionageRoster, FactionId, FactionOrders, FactionRegistry,
    FactionSecurityPolicies, FaunaConfigHandle, FogRevealLedger, FollowPolicy, ForageRegistry,
    GenerationId, GenerationRegistry, HerdRegistry, InfluencerImpacts, InfluentialRoster,
    LaborConfigHandle, MapPresetsHandle, PendingCrisisSpawns, PopulationCohort, QueueMissionError,
    QueueMissionParams, Scalar, SecurityPolicy, SentimentAxisBias, Settlement, SimulationConfig,
    SimulationConfigMetadata, SimulationTick, SnapshotHistory, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, SnapshotOverlaysConfigMetadata, StartLocation,
    StartProfileLookup, StartProfilesHandle, StartingUnit, StoredSnapshot, SubmitError,
    SubmitOutcome, SupportChannel, Tile, TileRegistry, TownCenter, TurnPipelineConfig,
    TurnPipelineConfigHandle, TurnPipelineConfigMetadata, TurnQueue, CULTIVATION_DISCOVERY_ID,
    FOOD, HERDING_DISCOVERY_ID,
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
const SCOUT_REVEAL_DURATION_TURNS: u64 = 8;
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
            Command::AssignLabor {
                faction,
                band_entity_bits,
                role,
                workers,
                target_x,
                target_y,
                fauna_id,
                policy,
            } => {
                handle_assign_labor(
                    &mut app,
                    faction,
                    band_entity_bits,
                    role,
                    workers,
                    target_x,
                    target_y,
                    fauna_id,
                    policy,
                );
            }
            Command::MoveBand {
                faction,
                band_entity_bits,
                target_x,
                target_y,
            } => {
                handle_move_band(&mut app, faction, band_entity_bits, target_x, target_y);
            }
            Command::SendExpedition {
                faction,
                band_entity_bits,
                party_workers,
                target_x,
                target_y,
            } => {
                handle_send_expedition(
                    &mut app,
                    faction,
                    band_entity_bits,
                    party_workers,
                    target_x,
                    target_y,
                );
            }
            Command::RecallExpedition {
                faction,
                expedition_entity_bits,
            } => {
                handle_recall_expedition(&mut app, faction, expedition_entity_bits);
            }
            Command::SendHuntExpedition {
                faction,
                band_entity_bits,
                party_workers,
                fauna_id,
                policy,
            } => {
                handle_send_hunt_expedition(
                    &mut app,
                    faction,
                    band_entity_bits,
                    party_workers,
                    fauna_id,
                    policy,
                );
            }
            Command::FoundSettlement {
                faction,
                target_x,
                target_y,
            } => {
                handle_found_settlement(&mut app, faction, target_x, target_y);
            }
            Command::Domesticate { faction, herd_id } => {
                handle_domesticate(&mut app, faction, herd_id);
            }
            Command::Cultivate {
                faction,
                target_x,
                target_y,
            } => {
                handle_cultivate(&mut app, faction, UVec2::new(target_x, target_y));
            }
            Command::Corral {
                faction,
                target_x,
                target_y,
            } => {
                handle_corral(&mut app, faction, UVec2::new(target_x, target_y));
            }
            Command::CancelOrder {
                faction,
                band_entity_bits,
            } => {
                handle_cancel_order(&mut app, faction, band_entity_bits);
            }
        }

        // Re-capture + broadcast the fresh world (incl. the feed) so an immediate, synchronous
        // command mutation (expedition launch, move_band, assign_labor, …) reaches the client now,
        // not only at the next turn (replaces the feed-only splice that reused last turn's world).
        recapture_and_broadcast(&mut app, bin_server, flat_server);
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
    AssignLabor {
        faction: FactionId,
        band_entity_bits: Option<u64>,
        role: String,
        workers: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        fauna_id: Option<String>,
        policy: Option<String>,
    },
    MoveBand {
        faction: FactionId,
        band_entity_bits: Option<u64>,
        target_x: u32,
        target_y: u32,
    },
    SendExpedition {
        faction: FactionId,
        band_entity_bits: Option<u64>,
        party_workers: u32,
        target_x: u32,
        target_y: u32,
    },
    RecallExpedition {
        faction: FactionId,
        expedition_entity_bits: u64,
    },
    SendHuntExpedition {
        faction: FactionId,
        band_entity_bits: Option<u64>,
        party_workers: u32,
        fauna_id: String,
        policy: Option<String>,
    },
    FoundSettlement {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    Domesticate {
        faction: FactionId,
        herd_id: String,
    },
    Cultivate {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    Corral {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    CancelOrder {
        faction: FactionId,
        band_entity_bits: Option<u64>,
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

/// Fetch (or insert a default) mutable [`LaborAllocation`] on a band entity.
fn band_allocation_mut(
    app: &mut bevy::prelude::App,
    band: Entity,
) -> bevy::prelude::Mut<'_, LaborAllocation> {
    if app.world.get::<LaborAllocation>(band).is_none() {
        app.world
            .entity_mut(band)
            .insert(LaborAllocation::default());
    }
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("labor allocation inserted above")
}

/// Seed the touched source's **yield telemetry** (`LaborAllocation.last_yields`) from its
/// **pre-commit forecast**, right after the allocation is mutated.
///
/// Without this, telemetry is only ever written during turn resolution (`advance_labor_allocation`),
/// so between "player assigns workers" and "player advances the turn" a brand-new source has no row
/// and the display snapshot serializes `actual_yield = 0.0` — the client cannot tell "0 because not
/// computed yet" from "0 because the source is barren", and every fresh assignment reads `+0.00`.
///
/// The seeded row is exactly what the turn will pay under unchanged conditions: it reuses the same
/// forecast helpers the take path reads (`forecast == actual` — see "Pre-commit Yield Forecast" in
/// `core_sim/CLAUDE.md`), and it is the same number the client's compose-time "Expected yield" row
/// promises — so there is no jump when the turn lands, and it is overwritten by the resolved take.
///
/// Only the **one source the command touched** is seeded (other sources keep their real actuals), and
/// only where the resolution path would actually pay: a source the turn would skip (out of the band's
/// work range / past the hunt leash, an unseeded patch, a vanished herd) keeps its zero row, and a
/// genuinely barren source seeds `0.0` — `+0.00` stays reachable, and correct, there.
fn seed_source_yield(
    app: &mut bevy::prelude::App,
    band: Entity,
    target: &LaborTarget,
    workers: u32,
) {
    // Unassigned (`workers == 0`): `set_assignment` already dropped the source's row with its
    // assignment. Scout/Warrior are band-wide roles with no food yield — the resolution path leaves
    // them at zero, so seeding must too.
    if workers == 0
        || !matches!(
            target,
            LaborTarget::Forage { .. } | LaborTarget::Hunt { .. }
        )
    {
        return;
    }
    let Some(cohort) = app.world.get::<PopulationCohort>(band) else {
        return;
    };
    let current_tile = cohort.current_tile;
    // The band's productivity multiplier is applied at payout in the resolution path, so the forecast
    // must fold it in too (the snapshot's per-source forecast is captured at 1.0 and scaled client-side
    // by this same multiplier).
    let wellbeing = app.world.resource::<WellbeingConfigHandle>().get();
    let output_mult = output_multiplier(cohort, &wellbeing).to_f32();
    let Some(band_pos) = app
        .world
        .get::<Tile>(current_tile)
        .map(|tile| tile.position)
    else {
        return;
    };
    let grid_width = app.world.resource::<TileRegistry>().width;
    let wrap_horizontal = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let labor = app.world.resource::<LaborConfigHandle>().get();

    let seeded = match target {
        LaborTarget::Forage { tile, policy } => {
            // Out of the band's work range → the turn pays 0 (assignment kept). Keep the zero row.
            if hex_distance_wrapped(band_pos, *tile, grid_width, wrap_horizontal)
                > labor.band_work_range
            {
                return;
            }
            let Some(tile_entity) = app.world.resource::<TileRegistry>().index(tile.x, tile.y)
            else {
                return;
            };
            let Some(module) = app.world.get::<FoodModuleTag>(tile_entity) else {
                return; // no food module on the tile → the turn pays 0.
            };
            let seasonal = module.seasonal_weight.max(0.0);
            let Some(patch) = app.world.resource::<ForageRegistry>().patch(*tile) else {
                return; // unseeded patch → the turn pays 0.
            };
            forage_source_yield_preview(
                patch,
                &labor.forage,
                seasonal,
                output_mult,
                workers,
                *policy,
            )
        }
        LaborTarget::Hunt { fauna_id, policy } => {
            let Some(herd) = app.world.resource::<HerdRegistry>().find(fauna_id) else {
                return; // herd gone → the assignment lapses next turn.
            };
            // Past the leash → the assignment lapses next turn; keep the zero row.
            if hex_distance_wrapped(band_pos, herd.position(), grid_width, wrap_horizontal)
                > labor.hunt_reach()
            {
                return;
            }
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            hunt_source_yield_preview(
                herd,
                &fauna,
                labor.hunt.per_worker_biomass_capacity,
                output_mult,
                workers,
                *policy,
            )
        }
        LaborTarget::Scout | LaborTarget::Warrior => return,
    };
    band_allocation_mut(app, band).set_source_yield(target, seeded);
}

/// Validate a labor target's **policy** against the source it names, returning a player-facing
/// rejection reason (`Err`) or `Ok`. Two independent checks:
///
/// 1. **Kind.** The two *investment* policies are kind-exclusive: `Cultivate` is Forage-only,
///    `Corral` is Hunt-only (`FollowPolicy::valid_for_forage` / `valid_for_hunt`). An invalid combo
///    is rejected outright rather than silently coerced.
/// 2. **Gates.** Cultivate requires the faction to **know Cultivation** and the patch to be
///    **Thriving** (and not already tended, and not someone else's). Corral requires the faction to
///    **know Herding** and to own the **domesticated** herd (and it not already be penned). These are
///    the same gates the retired early-claim commands enforced — they now guard the *policy* instead.
///
/// The extractive policies (Sustain/Surplus/Market/Eradicate) are always valid on either kind.
fn validate_labor_policy(
    app: &bevy::prelude::App,
    faction: FactionId,
    target: &LaborTarget,
) -> Result<(), String> {
    match target {
        LaborTarget::Forage { tile, policy } => {
            if !policy.valid_for_forage() {
                return Err(format!(
                    "'{}' is not a foraging policy — it applies to herds.",
                    policy.as_str()
                ));
            }
            if !matches!(policy, FollowPolicy::Cultivate) {
                return Ok(());
            }
            let cultivation = app
                .world
                .resource::<LaborConfigHandle>()
                .get()
                .forage
                .cultivation
                .clone();
            let knows_cultivation = app
                .world
                .resource::<DiscoveryProgressLedger>()
                .get_progress(faction, CULTIVATION_DISCOVERY_ID)
                >= scalar_from_f32(cultivation.knowledge_completion_threshold);
            if !knows_cultivation {
                return Err("Your people have not learned Cultivation yet. Sustain-forage thriving patches to learn it.".to_string());
            }
            let Some(patch) = app.world.resource::<ForageRegistry>().patch(*tile) else {
                return Err(format!("No forage patch at ({}, {}).", tile.x, tile.y));
            };
            if patch.is_cultivated() {
                return Err(format!(
                    "The patch at ({}, {}) is already cultivated — forage it to tend it.",
                    tile.x, tile.y
                ));
            }
            if patch.ecology_phase != EcologyPhase::Thriving {
                return Err(format!(
                    "The patch at ({}, {}) is not thriving — let it recover before cultivating it.",
                    tile.x, tile.y
                ));
            }
            if patch.owner.is_some_and(|owner| owner != faction) {
                return Err(format!(
                    "Another people are cultivating the patch at ({}, {}).",
                    tile.x, tile.y
                ));
            }
            Ok(())
        }
        LaborTarget::Hunt { fauna_id, policy } => {
            if !policy.valid_for_hunt() {
                return Err(format!(
                    "'{}' is not a hunting policy — it applies to forage patches.",
                    policy.as_str()
                ));
            }
            if !matches!(policy, FollowPolicy::Corral) {
                return Ok(());
            }
            let knowledge_threshold = app
                .world
                .resource::<FaunaConfigHandle>()
                .get()
                .husbandry
                .knowledge_completion_threshold;
            let knows_herding = app
                .world
                .resource::<DiscoveryProgressLedger>()
                .get_progress(faction, HERDING_DISCOVERY_ID)
                >= scalar_from_f32(knowledge_threshold);
            if !knows_herding {
                return Err("Your people have not learned Herding yet. Sustain-hunt thriving herds to learn it.".to_string());
            }
            let Some(herd) = app.world.resource::<HerdRegistry>().find(fauna_id) else {
                return Err(format!("No herd '{}' to corral.", fauna_id));
            };
            if herd.is_corralled() {
                return Err(format!("{} is already corralled.", fauna_id));
            }
            if !herd.is_domesticated() {
                return Err(format!(
                    "{} is not domesticated. Sustain-hunt it to tame it before building a pen.",
                    fauna_id
                ));
            }
            if herd.owner != Some(faction) {
                return Err(format!("You do not own {}.", fauna_id));
            }
            Ok(())
        }
        LaborTarget::Scout | LaborTarget::Warrior => Ok(()),
    }
}

/// Set the worker count for one labor target on a band (idempotent; `0` unassigns; clamps to the
/// band's free working-age headroom). Text forms:
///   `assign_labor <faction> <band> forage <x> <y> [policy] <workers>`
///   `assign_labor <faction> <band> hunt <herd_id> [policy] <workers>`
///   `assign_labor <faction> <band> scout <workers>`
///   `assign_labor <faction> <band> warrior <workers>`
///
/// `policy` accepts the four extractive rungs plus the kind-specific **investment** rungs
/// (`cultivate` on forage, `corral` on hunt) — see `validate_labor_policy` for the gates.
#[allow(clippy::too_many_arguments)]
fn handle_assign_labor(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_entity_bits: Option<u64>,
    role: String,
    workers: u32,
    target_x: Option<u32>,
    target_y: Option<u32>,
    fauna_id: Option<String>,
    policy: Option<String>,
) {
    let target = match role.to_ascii_lowercase().as_str() {
        "forage" => match (target_x, target_y) {
            (Some(x), Some(y)) => LaborTarget::Forage {
                tile: UVec2::new(x, y),
                policy: parse_follow_policy(policy.as_deref()),
            },
            _ => {
                emit_command_failure(
                    app,
                    CommandEventKind::Forage,
                    faction,
                    "assign_labor forage requires <x> <y>.".to_string(),
                );
                return;
            }
        },
        "hunt" => match fauna_id {
            Some(id) if !id.trim().is_empty() => LaborTarget::Hunt {
                fauna_id: id,
                policy: parse_follow_policy(policy.as_deref()),
            },
            _ => {
                emit_command_failure(
                    app,
                    CommandEventKind::Hunt,
                    faction,
                    "assign_labor hunt requires <herd_id>.".to_string(),
                );
                return;
            }
        },
        "scout" => LaborTarget::Scout,
        "warrior" => LaborTarget::Warrior,
        other => {
            emit_command_failure(
                app,
                CommandEventKind::CancelOrder,
                faction,
                format!("Unknown labor role '{}'.", other),
            );
            return;
        }
    };

    let event_kind = match &target {
        LaborTarget::Forage { .. } => CommandEventKind::Forage,
        LaborTarget::Hunt { .. } => CommandEventKind::Hunt,
        LaborTarget::Scout => CommandEventKind::Scout,
        LaborTarget::Warrior => CommandEventKind::CancelOrder,
    };

    // Kind + gate validation for the policy (notably the two investment policies, Cultivate/Corral).
    // Unassigning (`workers == 0`) is always allowed — a player must be able to abandon an
    // investment even if its gates have since lapsed.
    if workers > 0 {
        if let Err(reason) = validate_labor_policy(app, faction, &target) {
            emit_command_failure(app, event_kind, faction, reason);
            return;
        }
    }

    let Some(band) =
        select_starting_band(app, faction, band_entity_bits, "assign_labor", event_kind)
    else {
        return;
    };

    let available = app
        .world
        .get::<PopulationCohort>(band.entity)
        .map(|cohort| available_workers(cohort.working))
        .unwrap_or(0);

    let kind_label = target.kind();
    let (applied, assigned_total) = {
        let mut allocation = band_allocation_mut(app, band.entity);
        let applied = allocation.set_assignment(target.clone(), workers, available);
        (applied, allocation.assigned_total())
    };
    // Show the source's expected yield immediately (workers added/removed OR policy changed — every
    // shape of this command that moves the number): without the seed the row reads `+0.00` until the
    // player advances a turn.
    seed_source_yield(app, band.entity, &target, applied);

    let tick = app.world.resource::<SimulationTick>().0;
    let clamp_note = if applied < workers {
        format!(" (clamped from {} — only {} idle)", workers, available)
    } else {
        String::new()
    };
    push_command_event(
        app,
        tick,
        event_kind,
        faction,
        format!("{} {} x{}{}", band.label, kind_label, applied, clamp_note),
        Some(format!(
            "status=applied role={} workers={} assigned_total={} available={}",
            kind_label, applied, assigned_total, available
        )),
    );
}

/// Order a band to travel toward a target tile at `band_move_tiles_per_turn`/turn (Early-Game
/// Labor). In-range sources update as the band moves; Forage assignments naturally read 0 while
/// out of range. Text form: `move_band <faction> <band> <x> <y>`.
fn handle_move_band(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_entity_bits: Option<u64>,
    target_x: u32,
    target_y: u32,
) {
    let target = UVec2::new(target_x, target_y);
    if ensure_land_tile(
        app,
        faction,
        target,
        "move_band",
        Some(CommandEventKind::CancelOrder),
    )
    .is_none()
    {
        return;
    }
    let Some(band) = select_starting_band(
        app,
        faction,
        band_entity_bits,
        "move_band",
        CommandEventKind::CancelOrder,
    ) else {
        return;
    };
    app.world
        .entity_mut(band.entity)
        .insert(BandTravel { target });

    // If the moved entity is an expedition, a fresh `move_band` un-latches AwaitingOrders (or
    // redirects a Returning party back out to explore): re-arm it Outbound and re-open the
    // arrival announcement so reaching the new waypoint fires the feed line again.
    if let Some(mut expedition) = app.world.get_mut::<Expedition>(band.entity) {
        expedition.phase = ExpeditionPhase::Outbound;
        expedition.announced = false;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        format!("{} moving -> ({}, {})", band.label, target_x, target_y),
        Some(format!(
            "status=queued action=move_band band={}",
            band.label
        )),
    );
}

/// Outfit and launch a scouting expedition: draw `party_workers` off the resolved home band's
/// working pool and larder-drawn provisions, then spawn a detached `StartingUnit` band tagged
/// `Expedition` (deliberately no `ResidentBand`) traveling toward the target. v1 is deterministic
/// success. Text form: `send_expedition <faction> <band> <party_workers> <x> <y>`.
fn handle_send_expedition(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_entity_bits: Option<u64>,
    party_workers: u32,
    target_x: u32,
    target_y: u32,
) {
    let target = UVec2::new(target_x, target_y);
    if ensure_land_tile(
        app,
        faction,
        target,
        "send_expedition",
        Some(CommandEventKind::ExpeditionSent),
    )
    .is_none()
    {
        return;
    }
    let Some(band) = select_starting_band(
        app,
        faction,
        band_entity_bits,
        "send_expedition",
        CommandEventKind::ExpeditionSent,
    ) else {
        return;
    };
    // `select_starting_band` only filters `With<ResidentBand>` on the None-bits fallback; an
    // explicit `band_entity_bits` resolves on `StartingUnit` alone, which an expedition also carries
    // (kept so `move_band` can retarget it). A party can only be outfitted *from* a resident band —
    // reject anything else so `send_expedition` can't spawn a party off another expedition.
    if app.world.get::<ResidentBand>(band.entity).is_none() {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            "send_expedition: band is not a resident band.",
        );
        return;
    }

    let grid_width = app.world.resource::<TileRegistry>().width;
    let wrap_horizontal = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let cfg = app.world.resource::<ExpeditionConfigHandle>().get();

    // Snapshot the home band: its position, worker pool, and a clone we retask into the party.
    let Some(band_cohort) = app.world.get::<PopulationCohort>(band.entity) else {
        return;
    };
    let current_tile = band_cohort.current_tile;
    let band_working = band_cohort.working;
    let mut expedition_cohort = band_cohort.clone();
    let Some(band_tile) = app.world.get::<Tile>(current_tile) else {
        return;
    };
    let band_pos = band_tile.position;
    let (unit_kind, unit_tags) = app
        .world
        .get::<StartingUnit>(band.entity)
        .map(|unit| (unit.kind.clone(), unit.tags.clone()))
        .unwrap_or_else(|| ("expedition".to_string(), Vec::new()));

    let distance = hex_distance_wrapped(band_pos, target, grid_width, wrap_horizontal);
    let available = available_workers(band_working);
    let max_party = available.min(cfg.max_party_size);
    if party_workers < 1 || party_workers > max_party {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!(
                "Party of {} workers invalid — {} can outfit 1..{} workers.",
                party_workers, band.label, max_party
            ),
        );
        return;
    }

    // Draw provisions (partial OK — non-fatal in v1) and remove the party from the band's pool.
    let requested = scalar_from_f32(
        party_workers as f32 * distance as f32 * cfg.provision_draw_per_worker_per_tile,
    );
    let party_scalar = Scalar::from_u32(party_workers);
    let drawn = {
        // The `get`-guard above already confirmed the component; a synchronous handler can't
        // despawn it mid-call, so this re-fetch is unreachable-None. Match the sibling guards'
        // let-else style (no `expect` on a server path) and early-return if it somehow fails.
        let Some(mut band_cohort) = app.world.get_mut::<PopulationCohort>(band.entity) else {
            return;
        };
        let drawn = band_cohort.stores.take(FOOD, requested);
        band_cohort.working -= party_scalar;
        band_cohort.sync_size();
        drawn
    };

    // Retask the cloned cohort into a detached party co-located with the band.
    expedition_cohort.children = Scalar::from_i64(0);
    expedition_cohort.working = party_scalar;
    expedition_cohort.elders = Scalar::from_i64(0);
    expedition_cohort.stores = LocalStore::new();
    if drawn > Scalar::from_i64(0) {
        expedition_cohort.stores.add(FOOD, drawn);
    }
    expedition_cohort.age_turns = 0;
    expedition_cohort.migration = None;
    expedition_cohort.grievance = Scalar::from_i64(0);
    expedition_cohort.sync_size();

    let expedition_entity = app
        .world
        .spawn((
            expedition_cohort,
            LaborAllocation::default(),
            StartingUnit::new(unit_kind, unit_tags),
            Expedition {
                home_band: band.entity,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
            },
            BandTravel { target },
        ))
        .id();

    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionSent,
        faction,
        format!("{} expedition -> ({}, {})", band.label, target_x, target_y),
        Some(format!(
            "status=applied workers={} provisions_drawn={} distance={} expedition={}",
            party_workers,
            drawn.to_i64_whole(),
            distance,
            expedition_entity.to_bits()
        )),
    );
}

/// Outfit and launch a hunting expedition (PR 2): draw `party_workers` off the resolved home band
/// and send a detached party to follow the herd `fauna_id` under `policy` (Sustain when omitted).
/// Unlike the scouting expedition it draws **no** provisions (it lives off its kills) and starts in
/// the `Hunting` phase heading for the herd's live tile. Text form:
/// `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [policy]`.
fn handle_send_hunt_expedition(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_entity_bits: Option<u64>,
    party_workers: u32,
    fauna_id: String,
    policy: Option<String>,
) {
    // Take policy — parsed via `FollowPolicy::from_str`, default Sustain (conservative) when omitted.
    // An explicit but unparseable token is rejected rather than silently defaulting: Sustain and
    // Market are opposite ecological behaviors, so a typo must not silently flip the herd's fate.
    let policy: FollowPolicy = match policy.as_deref() {
        None => FollowPolicy::Sustain,
        // The two **investment** policies are place-bound improvements a *resident* band builds and
        // then tends — a detached expedition can't pen a herd and walk home, so they are rejected here
        // alongside an unparseable token (the four extractive rungs are the expedition's whole axis).
        Some(token) => match token.parse::<FollowPolicy>() {
            Ok(parsed) if !matches!(parsed, FollowPolicy::Cultivate | FollowPolicy::Corral) => {
                parsed
            }
            _ => {
                emit_command_failure(
                    app,
                    CommandEventKind::ExpeditionSent,
                    faction,
                    format!(
                        "send_hunt_expedition: unusable take policy '{}' — valid options are \
                         sustain, surplus, market, eradicate.",
                        token
                    ),
                );
                return;
            }
        },
    };
    let Some(band) = select_starting_band(
        app,
        faction,
        band_entity_bits,
        "send_hunt_expedition",
        CommandEventKind::ExpeditionSent,
    ) else {
        return;
    };
    // Same resident-band gate as `send_expedition`: a party can only be outfitted from a real band.
    if app.world.get::<ResidentBand>(band.entity).is_none() {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            "send_hunt_expedition: band is not a resident band.",
        );
        return;
    }

    // The target must resolve to a live herd; capture its current tile as the initial travel target.
    let herd_pos = {
        let registry = app.world.resource::<HerdRegistry>();
        registry.find(&fauna_id).map(|herd| herd.position())
    };
    let Some(herd_pos) = herd_pos else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("send_hunt_expedition: no live herd '{}'.", fauna_id),
        );
        return;
    };

    let cfg = app.world.resource::<ExpeditionConfigHandle>().get();
    let Some(band_cohort) = app.world.get::<PopulationCohort>(band.entity) else {
        return;
    };
    let band_working = band_cohort.working;
    let mut expedition_cohort = band_cohort.clone();
    let (unit_kind, unit_tags) = app
        .world
        .get::<StartingUnit>(band.entity)
        .map(|unit| (unit.kind.clone(), unit.tags.clone()))
        .unwrap_or_else(|| ("expedition".to_string(), Vec::new()));

    let available = available_workers(band_working);
    let max_party = available.min(cfg.max_party_size);
    if party_workers < 1 || party_workers > max_party {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!(
                "Party of {} workers invalid — {} can outfit 1..{} workers.",
                party_workers, band.label, max_party
            ),
        );
        return;
    }

    // Launch-time viability forecast — a bounded forward SIMULATION of the trip (`hunt_trip_forecast`),
    // not a division. A Sustain party skims the herd's Maximum Sustainable Yield (a *flow*), and a
    // Surplus/Market party eats *stock* headroom and then falls back to the regrowth trickle once it
    // is gone, so filling a carry cap off a small herd can genuinely take dozens of turns. That is
    // ecologically true, not a bug; the player must be told at launch rather than silently trapped,
    // so the forecast rides the `ExpeditionSent` feed entry (it still launches either way).
    let forecast = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .find(&fauna_id)
            .map(|herd| hunt_trip_forecast(party_workers, herd, policy, &fauna, &labor, &cfg))
    };
    let (viability_note, viability_detail) = match &forecast {
        // A denial mission (Eradicate) brings nothing home, so a "turns to fill" number would be
        // meaningless — say what it actually does instead of quoting a fillable-looking ETA.
        Some(f) if !f.delivers_food => (
            " — denial mission: the party delivers NO food; it hunts the herd toward extinction"
                .to_string(),
            " eta_turns=none viability=denial".to_string(),
        ),
        Some(f) => match f.turns_to_fill {
            Some(turns) if turns <= cfg.hunt.viability_warn_turns => (
                format!(" — est. ~{} turns to fill", turns),
                format!(" eta_turns={}", turns),
            ),
            Some(turns) => (
                format!(
                    " — est. ~{} turns to fill; NOT VIABLE at this herd's yield",
                    turns
                ),
                format!(" eta_turns={} viability=marginal", turns),
            ),
            // The herd yields nothing at all under this policy (sub-Allee / collapsing).
            None if f.first_turn_provisions <= 0.0 => (
                " — this herd is below its collapse threshold and yields no sustainable take; the \
                 party will return empty"
                    .to_string(),
                " eta_turns=none viability=impossible".to_string(),
            ),
            // It yields *something*, but not enough to fill a pack inside the forecast horizon —
            // the exact turn count past there carries no information a player can act on.
            None => (
                format!(
                    " — the party will NOT fill its pack within {} turns at this herd's yield; NOT \
                     VIABLE",
                    cfg.hunt.forecast_horizon_turns
                ),
                " eta_turns=none viability=marginal".to_string(),
            ),
        },
        None => (String::new(), String::new()),
    };

    // Remove the party from the band's pool — but draw NO provisions (it lives off its kills).
    let party_scalar = Scalar::from_u32(party_workers);
    {
        let Some(mut band_cohort) = app.world.get_mut::<PopulationCohort>(band.entity) else {
            return;
        };
        band_cohort.working -= party_scalar;
        band_cohort.sync_size();
    }

    // Retask the cloned cohort into a detached party co-located with the band, empty larder.
    expedition_cohort.children = Scalar::from_i64(0);
    expedition_cohort.working = party_scalar;
    expedition_cohort.elders = Scalar::from_i64(0);
    expedition_cohort.stores = LocalStore::new();
    expedition_cohort.age_turns = 0;
    expedition_cohort.migration = None;
    expedition_cohort.grievance = Scalar::from_i64(0);
    expedition_cohort.sync_size();

    let expedition_entity = app
        .world
        .spawn((
            expedition_cohort,
            LaborAllocation::default(),
            StartingUnit::new(unit_kind, unit_tags),
            Expedition {
                home_band: band.entity,
                mission: ExpeditionMission::Hunt {
                    fauna_id: fauna_id.clone(),
                    policy,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
            },
            BandTravel { target: herd_pos },
        ))
        .id();

    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionSent,
        faction,
        format!(
            "{} hunting expedition ({}) -> herd {}{}",
            band.label,
            policy.as_str(),
            fauna_id,
            viability_note
        ),
        Some(format!(
            "status=applied mission=hunt policy={} workers={} herd={} expedition={}{}",
            policy.as_str(),
            party_workers,
            fauna_id,
            expedition_entity.to_bits(),
            viability_detail
        )),
    );
}

/// Order an expedition home: set its phase to `Returning` (it chases the home band's live tile and
/// folds its workers + leftover provisions back on arrival). Text form:
/// `recall_expedition <faction> <expedition_entity_bits>`.
fn handle_recall_expedition(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    expedition_entity_bits: u64,
) {
    let Some(entity) = resolve_expedition_entity(
        app,
        faction,
        expedition_entity_bits,
        "recall_expedition",
        CommandEventKind::ExpeditionRecalled,
    ) else {
        return;
    };
    let label = starting_unit_label(app, entity);
    if let Some(mut expedition) = app.world.get_mut::<Expedition>(entity) {
        expedition.phase = ExpeditionPhase::Returning;
    }
    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionRecalled,
        faction,
        format!("{} recalled — returning home", label),
        Some(format!("status=returning expedition={}", entity.to_bits())),
    );
}

/// Resolve an entity-bits reference to a faction's own [`Expedition`] (mirrors
/// [`resolve_starting_unit_entity`] but gates on the `Expedition` component + faction match rather
/// than merely `StartingUnit`).
fn resolve_expedition_entity(
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
            "command.expedition.rejected=invalid_entity_bits"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Expedition id {} is invalid.", entity_bits),
        );
        return None;
    };
    if !app.world.entities().contains(entity) {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            entity_bits,
            "command.expedition.rejected=entity_not_found"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!(
                "Expedition id {} does not exist in the simulation.",
                entity_bits
            ),
        );
        return None;
    }
    if app.world.get::<Expedition>(entity).is_none() {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            entity_bits,
            "command.expedition.rejected=entity_not_expedition"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Unit id {} is not an expedition.", entity_bits),
        );
        return None;
    }
    let faction_ok = app
        .world
        .get::<PopulationCohort>(entity)
        .map(|cohort| cohort.faction == faction)
        .unwrap_or(false);
    if !faction_ok {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            entity_bits,
            "command.expedition.rejected=wrong_faction"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Expedition id {} belongs to another faction.", entity_bits),
        );
        return None;
    }
    Some(entity)
}

/// Clear every labor assignment on a band and stop any in-progress move — the band goes fully
/// idle (the repurposed per-source unassign of the retired single-task `cancel_order`). Rejects an
/// already-idle band so a stray invocation reports a failure rather than a misleading "stood down".
fn handle_cancel_order(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_entity_bits: Option<u64>,
) {
    let Some(band) = select_starting_band(
        app,
        faction,
        band_entity_bits,
        "cancel_order",
        CommandEventKind::CancelOrder,
    ) else {
        return;
    };

    let has_task = {
        let entity = app.world.entity(band.entity);
        entity.contains::<BandTravel>()
            || app
                .world
                .get::<LaborAllocation>(band.entity)
                .map(|allocation| !allocation.assignments.is_empty())
                .unwrap_or(false)
    };
    if !has_task {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!("{} has no active order to cancel.", band.label),
        );
        return;
    }

    {
        let mut entity = app.world.entity_mut(band.entity);
        entity.remove::<BandTravel>();
        if let Some(mut allocation) = entity.get_mut::<LaborAllocation>() {
            allocation.clear();
        }
    }

    let tick = app.world.resource::<SimulationTick>().0;
    let detail = format!("status=cancelled band={}", band.label);
    info!(
        target: "shadow_scale::command",
        command = "cancel_order",
        faction = %faction.0,
        band = %band.label,
        "command.cancel_order.applied"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        format!("{} stood down", band.label),
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
                "You are not tending {}. Sustain-hunt it to build husbandry before claiming it.",
                herd_id
            ),
        ),
        Outcome::NotTame(progress) => emit_command_failure(
            app,
            CommandEventKind::Domesticate,
            faction,
            format!(
                "{} is not tame enough to domesticate ({}%). Keep Sustain-hunting it to build husbandry.",
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

/// **Set the Cultivate policy** on the forage patch at `tile` for the band(s) already working it
/// (Intensification — "Cultivate & Corral as explicit policies"). This is the command form of what
/// the client's policy picker does; it does **not** claim or complete anything.
///
/// The old early-claim (snap `cultivation_progress` to `1.0` once past a `claim_threshold`) is
/// **gone**: it would let the player skip the investment, which is the entire decision. Cultivating
/// now costs a real yield dip — while preparing, the patch pays only
/// `cultivation.cultivating_yield_fraction × its Sustain (MSY) ceiling` — and takes
/// `1 / progress_per_turn` turns of sustained work.
///
/// Gates (via the shared `validate_labor_policy`): the faction must know **Cultivation**, and the
/// patch must be **Thriving**, not already cultivated, and not another faction's.
fn handle_cultivate(app: &mut bevy::prelude::App, faction: FactionId, tile: UVec2) {
    let target = LaborTarget::Forage {
        tile,
        policy: FollowPolicy::Cultivate,
    };
    if let Err(reason) = validate_labor_policy(app, faction, &target) {
        warn!(
            target: "shadow_scale::command",
            command = "cultivate",
            faction = %faction.0,
            x = tile.x,
            y = tile.y,
            reason = %reason,
            "command.cultivate.rejected"
        );
        emit_command_failure(app, CommandEventKind::Cultivate, faction, reason);
        return;
    }

    let switched = set_policy_on_working_bands(app, faction, &target);
    if switched == 0 {
        emit_command_failure(
            app,
            CommandEventKind::Cultivate,
            faction,
            format!(
                "No band is foraging ({}, {}). Assign foragers to the patch first, then cultivate it.",
                tile.x, tile.y
            ),
        );
        return;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "cultivate",
        faction = %faction.0,
        x = tile.x,
        y = tile.y,
        bands = switched,
        "command.cultivate.preparing"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Cultivate,
        faction,
        format!(
            "Preparing patch at ({}, {}) for cultivation",
            tile.x, tile.y
        ),
        Some(format!(
            "status=preparing action=cultivate x={} y={} bands={}",
            tile.x, tile.y, switched
        )),
    );
}

/// **Set the Corral policy** on the domesticated herd standing at `tile` for the band(s) already
/// hunting it — the animal mirror of `handle_cultivate`, and the command form of the client's policy
/// picker. While the pen is built the keeper takes only
/// `husbandry.corralling_yield_fraction × the herd's Sustain (MSY) ceiling`; at
/// `corral_progress == 1.0` the herd is penned (`Herd::corral_at`), stops roaming, and pays the
/// higher place-local corral yield. There is no early claim.
///
/// Gates (via the shared `validate_labor_policy`): the faction must know **Herding** and own the
/// **domesticated**, not-yet-penned herd.
fn handle_corral(app: &mut bevy::prelude::App, faction: FactionId, tile: UVec2) {
    let Some(fauna_id) = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|herd| herd.position() == tile)
        .map(|herd| herd.id.clone())
    else {
        warn!(
            target: "shadow_scale::command",
            command = "corral",
            faction = %faction.0,
            x = tile.x,
            y = tile.y,
            "command.corral.rejected=unknown_herd"
        );
        emit_command_failure(
            app,
            CommandEventKind::Corral,
            faction,
            format!("No herd at ({}, {}) to corral.", tile.x, tile.y),
        );
        return;
    };

    let target = LaborTarget::Hunt {
        fauna_id: fauna_id.clone(),
        policy: FollowPolicy::Corral,
    };
    if let Err(reason) = validate_labor_policy(app, faction, &target) {
        warn!(
            target: "shadow_scale::command",
            command = "corral",
            faction = %faction.0,
            herd = %fauna_id,
            reason = %reason,
            "command.corral.rejected"
        );
        emit_command_failure(app, CommandEventKind::Corral, faction, reason);
        return;
    }

    let switched = set_policy_on_working_bands(app, faction, &target);
    if switched == 0 {
        emit_command_failure(
            app,
            CommandEventKind::Corral,
            faction,
            format!(
                "No band is hunting {}. Assign herders to it first, then corral it.",
                fauna_id
            ),
        );
        return;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "corral",
        faction = %faction.0,
        herd = %fauna_id,
        x = tile.x,
        y = tile.y,
        bands = switched,
        "command.corral.building"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Corral,
        faction,
        format!(
            "Building a corral for {} at ({}, {})",
            fauna_id, tile.x, tile.y
        ),
        Some(format!(
            "status=building action=corral herd={} x={} y={} bands={}",
            fauna_id, tile.x, tile.y, switched
        )),
    );
}

/// Re-point every band of `faction` **already working** `target`'s source (matched by
/// `LaborTarget::same_source`, so the tile / herd id) at `target`'s policy, keeping each band's
/// worker count. Returns how many bands were switched (`0` = nobody is working that source, which the
/// callers report as a rejection). The shared body of the repurposed `cultivate` / `corral` commands:
/// both now *set a policy* on an existing assignment rather than claiming the improvement outright.
fn set_policy_on_working_bands(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target: &LaborTarget,
) -> usize {
    let bands: Vec<(Entity, u32, u32)> = app
        .world
        .query::<(Entity, &PopulationCohort, &LaborAllocation)>()
        .iter(&app.world)
        .filter(|(_, cohort, _)| cohort.faction == faction)
        .filter_map(|(entity, cohort, allocation)| {
            let workers = allocation.workers_on(target);
            (workers > 0).then(|| (entity, workers, available_workers(cohort.working)))
        })
        .collect();
    for (entity, workers, available) in &bands {
        let applied = {
            let mut allocation = band_allocation_mut(app, *entity);
            allocation.set_assignment(target.clone(), *workers, *available)
        };
        // A policy switch changes the expected yield (e.g. Sustain → Cultivate drops to the preparing
        // bite), so re-seed the source's telemetry from the new policy's forecast — same reason as in
        // `handle_assign_labor`, which this command is the shorthand for.
        seed_source_yield(app, *entity, target, applied);
    }
    bands.len()
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

    let (mut new_config, applied_path) = match requested_path {
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

    // Reapply the SIM_PORT_BASE shift the same way startup does, so a reload of
    // an unchanged file keeps the shifted binds and doesn't spuriously trip the
    // socket_changed=restart_required warning below.
    apply_port_base_override(&mut new_config);

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
        ProtoCommandPayload::AssignLabor {
            faction_id,
            band_entity_bits,
            role,
            workers,
            target_x,
            target_y,
            fauna_id,
            policy,
        } => Some(Command::AssignLabor {
            faction: FactionId(faction_id),
            band_entity_bits,
            role,
            workers,
            target_x,
            target_y,
            fauna_id,
            policy,
        }),
        ProtoCommandPayload::MoveBand {
            faction_id,
            band_entity_bits,
            target_x,
            target_y,
        } => Some(Command::MoveBand {
            faction: FactionId(faction_id),
            band_entity_bits,
            target_x,
            target_y,
        }),
        ProtoCommandPayload::SendExpedition {
            faction_id,
            band_entity_bits,
            party_workers,
            target_x,
            target_y,
        } => Some(Command::SendExpedition {
            faction: FactionId(faction_id),
            band_entity_bits,
            party_workers,
            target_x,
            target_y,
        }),
        ProtoCommandPayload::RecallExpedition {
            faction_id,
            expedition_entity_bits,
        } => Some(Command::RecallExpedition {
            faction: FactionId(faction_id),
            expedition_entity_bits,
        }),
        ProtoCommandPayload::SendHuntExpedition {
            faction_id,
            band_entity_bits,
            party_workers,
            fauna_id,
            policy,
        } => Some(Command::SendHuntExpedition {
            faction: FactionId(faction_id),
            band_entity_bits,
            party_workers,
            fauna_id,
            policy,
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
        // Retired single-task band orders (Early-Game Labor slice 3a): the source-centric
        // `assign_labor` / `move_band` replace them. Ignored if a stale client still sends one.
        ProtoCommandPayload::ScoutArea { .. }
        | ProtoCommandPayload::FollowHerd { .. }
        | ProtoCommandPayload::ForageTile { .. }
        | ProtoCommandPayload::HuntGame { .. }
        | ProtoCommandPayload::HuntFauna { .. } => {
            warn!(
                target: "shadow_scale::server",
                "command.retired=ignored (replaced by assign_labor/move_band)"
            );
            None
        }
        ProtoCommandPayload::Domesticate {
            faction_id,
            herd_id,
        } => Some(Command::Domesticate {
            faction: FactionId(faction_id),
            herd_id,
        }),
        ProtoCommandPayload::Cultivate {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::Cultivate {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        ProtoCommandPayload::Corral {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::Corral {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        ProtoCommandPayload::CancelOrder {
            faction_id,
            band_entity_bits,
        } => Some(Command::CancelOrder {
            faction: FactionId(faction_id),
            band_entity_bits,
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

    // Default-band picker: only ever auto-grab a real band (`With<ResidentBand>`) so a band-less
    // command never silently commandeers a detached expedition (which keeps `StartingUnit`).
    let mut query = app
        .world
        .query_filtered::<(Entity, &PopulationCohort, &StartingUnit), With<ResidentBand>>();
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
    // Founders picker: real bands only (`With<ResidentBand>`) — an expedition can never found.
    let mut query = app
        .world
        .query_filtered::<(Entity, &PopulationCohort, &StartingUnit), With<ResidentBand>>();
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
        CommandEventKind::FollowHerd => "Hunt (ongoing)",
        CommandEventKind::FoundSettlement => "Found settlement",
        CommandEventKind::CampaignFounded => "Campaign founded",
        CommandEventKind::CampaignMilestone => "Campaign milestone",
        CommandEventKind::CampaignVictory => "Campaign victory",
        CommandEventKind::Forage => "Harvest",
        CommandEventKind::Hunt => "Hunt",
        CommandEventKind::Domesticate => "Domesticate",
        CommandEventKind::Cultivate => "Cultivate",
        CommandEventKind::Corral => "Corral",
        CommandEventKind::CancelOrder => "Cancel order",
        CommandEventKind::SedentarizationPrompt => "Sedentarization",
        CommandEventKind::SiteDiscovered => "Site discovered",
        CommandEventKind::ExpeditionSent => "Expedition sent",
        CommandEventKind::ExpeditionArrived => "Expedition arrived",
        CommandEventKind::ExpeditionRecalled => "Expedition recalled",
        CommandEventKind::ExpeditionReturned => "Expedition returned",
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

/// Re-capture a fresh world snapshot (current ECS state, **including** the command-event feed) and
/// broadcast it — WITHOUT advancing the turn or pushing a rollback ring entry. Runs after every
/// dispatched command so a world-mutating command (expedition launch, `move_band`, `assign_labor`,
/// …) is reflected in the client's snapshot immediately, not only after the next turn resolves.
/// Toggles `SnapshotCaptureMode::refresh_in_place` so `capture_snapshot` refreshes the latest
/// broadcast + back ring entry in place instead of recording a new ring entry. Re-capturing on a
/// genuinely non-mutating command is merely slightly wasteful (commands are human-issued, low
/// frequency) — the robust uniform path, no hand-curated "which commands mutate" list.
fn recapture_and_broadcast(
    app: &mut bevy::prelude::App,
    snapshot_server_bin: Option<&SnapshotServer>,
    snapshot_server_flat: Option<&SnapshotServer>,
) {
    recapture_snapshot_in_place(&mut app.world);

    let history = app.world.resource::<SnapshotHistory>();
    if let Some(server) = snapshot_server_bin {
        if let Some(bytes) = history.encoded_snapshot.as_ref() {
            server.broadcast(bytes.as_ref());
        }
    }
    if let Some(server) = snapshot_server_flat {
        if let Some(bytes) = history.encoded_snapshot_flat.as_ref() {
            server.broadcast(bytes.as_ref());
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::UVec2;
    use core_sim::{build_headless_app, ForagePatch};

    /// Insert a **Thriving, wild** patch — a valid Cultivate target (there is no early claim any
    /// more; progress must be earned under the Cultivate policy).
    fn seed_thriving_patch(app: &mut bevy::prelude::App, coord: UVec2) {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = ForagePatch::new(coord, 100.0);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        registry.patches.insert(coord, patch);
    }

    /// A band of `faction` sitting on tile entity `home` with one labor assignment (the band the
    /// repurposed `cultivate` / `corral` commands re-point at the investment policy).
    fn spawn_working_band(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        target: LaborTarget,
    ) -> Entity {
        let home = app.world.spawn_empty().id();
        app.world
            .spawn((
                PopulationCohort {
                    home,
                    current_tile: home,
                    size: 30,
                    children: core_sim::scalar_zero(),
                    working: scalar_from_f32(30.0),
                    elders: core_sim::scalar_zero(),
                    stores: LocalStore::new(),
                    morale: core_sim::scalar_one(),
                    last_morale_delta: core_sim::scalar_zero(),
                    last_morale_cause: Default::default(),
                    last_morale_contributions: Default::default(),
                    discontent_fraction: core_sim::scalar_zero(),
                    grievance: core_sim::scalar_zero(),
                    last_emigrated: 0,
                    last_immigrated: 0,
                    age_turns: 0,
                    generation: 0,
                    faction,
                    knowledge: Vec::new(),
                    migration: None,
                },
                LaborAllocation {
                    assignments: vec![core_sim::LaborAssignment {
                        target,
                        workers: BAND_WORKERS,
                    }],
                    ..Default::default()
                },
            ))
            .id()
    }

    /// Workers each test band staffs on its source.
    const BAND_WORKERS: u32 = 5;

    /// The policy the band's single assignment currently carries.
    fn band_policy(app: &bevy::prelude::App, band: Entity) -> FollowPolicy {
        match &app
            .world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .assignments[0]
            .target
        {
            LaborTarget::Forage { policy, .. } | LaborTarget::Hunt { policy, .. } => *policy,
            other => panic!("unexpected labor target {other:?}"),
        }
    }

    fn cultivate_rejected_for_unknown(app: &bevy::prelude::App) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Cultivate)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("learned Cultivation"))
        })
    }

    /// Rung 1b gate: `cultivate` is rejected when the faction has not learned Cultivation, and the
    /// band's Forage policy is left untouched.
    #[test]
    fn cultivate_rejected_when_cultivation_unknown() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                policy: FollowPolicy::Sustain,
            },
        );

        handle_cultivate(&mut app, faction, coord);

        assert!(
            cultivate_rejected_for_unknown(&app),
            "cultivate must emit a NotKnown failure when Cultivation is unknown"
        );
        assert_eq!(
            band_policy(&app, band),
            FollowPolicy::Sustain,
            "a rejected cultivate must not switch the band's policy"
        );
    }

    /// `cultivate` is rejected on a **non-Thriving** patch (the second gate) even when known.
    #[test]
    fn cultivate_rejected_on_a_stressed_patch() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.ecology_phase = EcologyPhase::Stressed;
        }
        grant_cultivation(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                policy: FollowPolicy::Sustain,
            },
        );

        handle_cultivate(&mut app, faction, coord);

        assert!(
            cultivate_failure_detail_contains(&app, "not thriving"),
            "cultivate must reject a stressed patch"
        );
        assert_eq!(band_policy(&app, band), FollowPolicy::Sustain);
    }

    /// The repurposed `cultivate`: with Cultivation known and a Thriving patch, it **sets the
    /// Cultivate policy** on the band already foraging the tile (it claims nothing — the investment
    /// must still be worked off).
    #[test]
    fn cultivate_sets_the_cultivate_policy_on_the_working_band() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        grant_cultivation(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                policy: FollowPolicy::Sustain,
            },
        );

        handle_cultivate(&mut app, faction, coord);

        assert_eq!(
            band_policy(&app, band),
            FollowPolicy::Cultivate,
            "cultivate switches the working band onto the investment policy"
        );
        assert!(
            !app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .unwrap()
                .is_cultivated(),
            "there is no early claim — the patch must still be prepared"
        );
    }

    /// With nobody foraging the tile there is no assignment to re-point: `cultivate` is rejected and
    /// tells the player to staff the patch first.
    #[test]
    fn cultivate_rejected_when_no_band_is_foraging_the_patch() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        grant_cultivation(&mut app, faction);

        handle_cultivate(&mut app, faction, coord);

        assert!(cultivate_failure_detail_contains(
            &app,
            "No band is foraging"
        ));
    }

    fn grant_cultivation(app: &mut bevy::prelude::App, faction: FactionId) {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, CULTIVATION_DISCOVERY_ID, scalar_from_f32(1.0));
    }

    fn cultivate_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Cultivate)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    /// Seed a herd standing on `coord`, optionally domesticated + owned by `owner`. Returns its id.
    fn seed_herd(app: &mut bevy::prelude::App, coord: UVec2, owner: Option<FactionId>) -> String {
        use core_sim::{Herd, SizeClass};
        let mut herd = Herd::new(
            "game_corral_test".to_string(),
            "Test Deer".to_string(),
            SizeClass::Small,
            vec![coord],
            60.0,
            100.0,
        );
        if let Some(faction) = owner {
            herd.claim_domestication(faction);
        }
        let id = herd.id.clone();
        app.world.resource_mut::<HerdRegistry>().herds.push(herd);
        id
    }

    fn grant_herding(app: &mut bevy::prelude::App, faction: FactionId) {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, HERDING_DISCOVERY_ID, scalar_from_f32(1.0));
    }

    fn herd_is_corralled(app: &bevy::prelude::App, id: &str) -> bool {
        app.world
            .resource::<HerdRegistry>()
            .find(id)
            .is_some_and(|herd| herd.is_corralled())
    }

    fn corral_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Corral)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    /// Rung 1c gate: `corral` is rejected when the faction has not learned Herding, even on a
    /// domesticated herd it owns, and the herd stays mobile.
    #[test]
    fn corral_rejected_when_herding_unknown() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, Some(faction));

        handle_corral(&mut app, faction, coord);

        assert!(
            corral_failure_detail_contains(&app, "learned Herding"),
            "corral must emit a NotKnown failure when Herding is unknown"
        );
        assert!(
            !herd_is_corralled(&app, &id),
            "a rejected corral leaves the herd mobile"
        );
    }

    /// `corral` is rejected on a herd that isn't domesticated (needs husbandry first), even when the
    /// faction knows Herding.
    #[test]
    fn corral_rejected_when_not_domesticated() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, None);
        grant_herding(&mut app, faction);

        handle_corral(&mut app, faction, coord);

        assert!(
            corral_failure_detail_contains(&app, "not domesticated"),
            "corral must reject a wild herd as NotDomesticated"
        );
        assert!(!herd_is_corralled(&app, &id));
    }

    /// `corral` is rejected for a faction that doesn't own the domesticated herd.
    #[test]
    fn corral_rejected_for_non_owner() {
        let mut app = build_headless_app();
        let owner = FactionId(0);
        let intruder = FactionId(1);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, Some(owner));
        grant_herding(&mut app, intruder);

        handle_corral(&mut app, intruder, coord);

        assert!(
            corral_failure_detail_contains(&app, "do not own"),
            "corral must reject a non-owner"
        );
        assert!(!herd_is_corralled(&app, &id));
    }

    /// The repurposed `corral`: a faction that knows Herding and owns the domesticated herd on the
    /// tile **sets the Corral policy** on the band already hunting it. The pen is not built yet — that
    /// costs `1 / corral_build_progress_per_turn` turns of the reduced Corral take.
    #[test]
    fn corral_sets_the_corral_policy_on_the_working_band() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, Some(faction));
        grant_herding(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                policy: FollowPolicy::Sustain,
            },
        );

        handle_corral(&mut app, faction, coord);

        assert_eq!(
            band_policy(&app, band),
            FollowPolicy::Corral,
            "corral switches the working band onto the investment policy"
        );
        assert!(
            !herd_is_corralled(&app, &id),
            "there is no early claim — the pen must still be built"
        );
    }

    /// With nobody hunting the herd there is no assignment to re-point: `corral` is rejected.
    #[test]
    fn corral_rejected_when_no_band_is_hunting_the_herd() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_herd(&mut app, coord, Some(faction));
        grant_herding(&mut app, faction);

        handle_corral(&mut app, faction, coord);

        assert!(corral_failure_detail_contains(&app, "No band is hunting"));
    }

    /// **The investment policies never reach an expedition.** Penning a herd (or preparing a patch)
    /// is place-bound work a *resident* band does — a detached party cannot pen a herd and walk home
    /// — so `send_hunt_expedition` refuses `corral`/`cultivate` at launch, alongside an unparseable
    /// token. This rejection is load-bearing: it is the ONLY thing standing between the player and
    /// `hunt_expedition_ceiling`'s unreachable arm (which takes nothing and `debug_assert!`s). No
    /// party may be spawned, and the failure must name the four policies that ARE valid.
    #[test]
    fn send_hunt_expedition_rejects_the_investment_policies() {
        for token in ["corral", "cultivate"] {
            let mut app = build_headless_app();
            let faction = FactionId(0);
            let herd_id = seed_herd(&mut app, UVec2::new(1, 1), Some(faction));

            handle_send_hunt_expedition(
                &mut app,
                faction,
                None,
                1,
                herd_id,
                Some(token.to_string()),
            );

            let rejected = app.world.resource::<CommandEventLog>().iter().any(|entry| {
                matches!(entry.kind, CommandEventKind::ExpeditionSent)
                    && entry.detail.as_deref().is_some_and(|detail| {
                        detail.contains("unusable take policy") && detail.contains(token)
                    })
            });
            assert!(
                rejected,
                "{token} is not an expedition policy — the launch must be refused with a clear reason"
            );
            let parties = app
                .world
                .query::<&Expedition>()
                .iter(&app.world)
                .peekable()
                .peek()
                .is_some();
            assert!(!parties, "{token}: no expedition may be spawned");
        }
    }

    /// The kind gates: `Cultivate` on a Hunt assignment and `Corral` on a Forage assignment are both
    /// rejected outright by `validate_labor_policy` (the `assign_labor` guard).
    #[test]
    fn cross_kind_investment_policies_are_rejected() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        let id = seed_herd(&mut app, coord, Some(faction));

        let corral_on_forage = validate_labor_policy(
            &app,
            faction,
            &LaborTarget::Forage {
                tile: coord,
                policy: FollowPolicy::Corral,
            },
        );
        assert!(
            corral_on_forage
                .as_ref()
                .is_err_and(|reason| reason.contains("not a foraging policy")),
            "Corral is not a forage policy: {corral_on_forage:?}"
        );

        let cultivate_on_hunt = validate_labor_policy(
            &app,
            faction,
            &LaborTarget::Hunt {
                fauna_id: id,
                policy: FollowPolicy::Cultivate,
            },
        );
        assert!(
            cultivate_on_hunt
                .as_ref()
                .is_err_and(|reason| reason.contains("not a hunting policy")),
            "Cultivate is not a hunt policy: {cultivate_on_hunt:?}"
        );
    }

    // --- Assign-time yield seeding (the `+0.00` fix) ----------------------------------------------
    //
    // `LaborAllocation.last_yields` used to be written ONLY during turn resolution, so between
    // "player assigns workers" and "player advances the turn" a brand-new source had no telemetry row
    // and the display snapshot serialized `actual_yield = 0.0` — every fresh assignment read `+0.00`.
    // `handle_assign_labor` now seeds the touched source's row from its pre-commit forecast, which
    // (by the forecast == actual invariant) is exactly what the turn then pays: no jump.

    /// f32 slack between the seeded forecast (provisions, direct f32 math) and the resolved take
    /// (biomass → fixed-point provisions): different multiplication order + a 1e-6 fixed-point grid.
    const SEED_EPSILON: f32 = 1e-4;
    /// Side of the square tile grid the seeding tests build.
    const GRID: u32 = 3;
    /// The biome the harness grid stands on — grassland, matching the `FoodModule::SavannaGrassland`
    /// tag its source tile carries. A forage patch's cap is the **tile's**
    /// (`forage.capacity_by_biome`), so the harness names a biome rather than reading a constant.
    const SOURCE_BIOME: sim_runtime::TerrainType = sim_runtime::TerrainType::PrairieSteppe;

    /// A `GRID`×`GRID` tile world + its `TileRegistry` (labor commands resolve band/source positions
    /// through it), with a full-weight `FoodModuleTag` on `source` so a Forage assignment there
    /// resolves. Returns the tile entity at `source`.
    fn seed_tile_grid(app: &mut bevy::prelude::App, source: UVec2) -> Entity {
        use core_sim::{FoodModule, FoodSiteKind};
        let tiles: Vec<Entity> = (0..GRID)
            .flat_map(|y| (0..GRID).map(move |x| UVec2::new(x, y)))
            .map(|position| {
                app.world
                    .spawn(Tile {
                        position,
                        terrain: SOURCE_BIOME,
                        ..Default::default()
                    })
                    .id()
            })
            .collect();
        let source_tile = tiles[(source.y * GRID + source.x) as usize];
        app.world.entity_mut(source_tile).insert(FoodModuleTag {
            module: FoodModule::SavannaGrassland,
            seasonal_weight: 1.0,
            kind: FoodSiteKind::SavannaTrack,
        });
        app.world.insert_resource(TileRegistry {
            tiles,
            width: GRID,
            height: GRID,
        });
        source_tile
    }

    /// A resident band standing on `tile` with **no** assignments — the state `assign_labor` acts on.
    fn spawn_idle_band(app: &mut bevy::prelude::App, faction: FactionId, tile: Entity) -> Entity {
        let band = spawn_working_band(app, faction, LaborTarget::Scout);
        app.world
            .entity_mut(band)
            .insert((
                StartingUnit::new("test_band".to_string(), Vec::new()),
                ResidentBand,
            ))
            .insert(LaborAllocation::default());
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.home = tile;
        cohort.current_tile = tile;
        band
    }

    /// Insert a **wild** patch at `coord` with the given biomass (`0.0` = barren) and ecology phase.
    fn seed_patch_with_biomass(
        app: &mut bevy::prelude::App,
        coord: UVec2,
        biomass: f32,
        phase: EcologyPhase,
    ) {
        let cap = forage_carrying_capacity(app);
        let mut patch = ForagePatch::new(coord, cap);
        patch.biomass = biomass;
        patch.ecology_phase = phase;
        app.world
            .resource_mut::<ForageRegistry>()
            .patches
            .insert(coord, patch);
    }

    /// The harness grid's forage carrying capacity: **the tile's**, from
    /// `forage.capacity_by_biome[SOURCE_BIOME]` (the human food web's per-biome table — no longer a
    /// global constant). The tests stock patches as a fraction of it rather than hard-coding biomass.
    fn forage_carrying_capacity(app: &bevy::prelude::App) -> f32 {
        app.world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME)
    }

    /// Drive the real command handler (band resolved by the default resident-band picker).
    fn assign_forage(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        coord: UVec2,
        policy: &str,
        workers: u32,
    ) {
        handle_assign_labor(
            app,
            faction,
            None,
            "forage".to_string(),
            workers,
            Some(coord.x),
            Some(coord.y),
            None,
            Some(policy.to_string()),
        );
    }

    fn assign_hunt(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        fauna_id: &str,
        policy: &str,
        workers: u32,
    ) {
        handle_assign_labor(
            app,
            faction,
            None,
            "hunt".to_string(),
            workers,
            None,
            None,
            Some(fauna_id.to_string()),
            Some(policy.to_string()),
        );
    }

    /// The single source's seeded/resolved `actual` yield.
    fn source_actual(app: &bevy::prelude::App, band: Entity) -> f32 {
        app.world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .last_yields
            .first()
            .expect("the staffed source has a telemetry row")
            .actual
    }

    /// Resolve one turn of labor (the only system that used to write yield telemetry).
    fn resolve_labor(app: &mut bevy::prelude::App) {
        use bevy_ecs::system::RunSystemOnce;
        app.world
            .run_system_once(core_sim::advance_labor_allocation);
    }

    /// **Forage.** A brand-new assignment reports its expected yield immediately — BEFORE any turn is
    /// advanced — and that seed is exactly what the pre-commit forecast promises.
    #[test]
    fn assigning_forage_workers_seeds_the_expected_yield_before_the_turn() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        // Half cap → a clear positive MSY skim; Thriving is the phase that biomass implies.
        let stocked = forage_carrying_capacity(&app) * 0.5;
        seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, "sustain", BAND_WORKERS);

        let seeded = source_actual(&app, band);
        assert!(
            seeded > 0.0,
            "a staffed, stocked forage patch must not read +0.00 before the turn: {seeded}"
        );
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let patch = app.world.resource::<ForageRegistry>().patch(coord).unwrap();
        let expected = forage_source_yield_preview(
            patch,
            &labor.forage,
            1.0,
            1.0,
            BAND_WORKERS,
            FollowPolicy::Sustain,
        );
        assert!(
            (seeded - expected.actual).abs() < SEED_EPSILON,
            "seed {seeded} must equal the forecast {}",
            expected.actual
        );
    }

    /// **Forage, no jump.** Advancing the turn pays exactly the seeded number (the forecast == actual
    /// invariant): the displayed yield does not move when the turn lands.
    #[test]
    fn resolved_forage_yield_equals_the_seeded_yield() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let stocked = forage_carrying_capacity(&app) * 0.5;
        seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, "sustain", BAND_WORKERS);
        let seeded = source_actual(&app, band);
        resolve_labor(&mut app);
        let resolved = source_actual(&app, band);

        assert!(
            (resolved - seeded).abs() < SEED_EPSILON,
            "the turn must pay the seeded yield (seed {seeded}, resolved {resolved})"
        );
    }

    /// **Hunt.** Same seed-before-the-turn guarantee on the animal side.
    #[test]
    fn assigning_hunt_workers_seeds_the_expected_yield_before_the_turn() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let id = seed_herd(&mut app, coord, None);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_hunt(&mut app, faction, &id, "sustain", BAND_WORKERS);

        let seeded = source_actual(&app, band);
        assert!(
            seeded > 0.0,
            "a staffed, thriving herd must not read +0.00 before the turn: {seeded}"
        );
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
        let expected = hunt_source_yield_preview(
            herd,
            &fauna,
            labor.hunt.per_worker_biomass_capacity,
            1.0,
            BAND_WORKERS,
            FollowPolicy::Sustain,
        );
        assert!(
            (seeded - expected.actual).abs() < SEED_EPSILON,
            "seed {seeded} must equal the forecast {}",
            expected.actual
        );
    }

    /// **Hunt, no jump.** The resolved take equals the seed.
    #[test]
    fn resolved_hunt_yield_equals_the_seeded_yield() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let id = seed_herd(&mut app, coord, None);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_hunt(&mut app, faction, &id, "sustain", BAND_WORKERS);
        let seeded = source_actual(&app, band);
        resolve_labor(&mut app);
        let resolved = source_actual(&app, band);

        assert!(
            (resolved - seeded).abs() < SEED_EPSILON,
            "the turn must pay the seeded yield (seed {seeded}, resolved {resolved})"
        );
    }

    /// **Policy change re-seeds.** Switching an existing assignment from Sustain (the MSY skim) to
    /// Eradicate (strip the patch) raises the displayed expectation immediately — the seed tracks
    /// every shape of the command that moves the number, not just a fresh staffing.
    #[test]
    fn changing_the_policy_reseeds_the_expected_yield() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let stocked = forage_carrying_capacity(&app) * 0.5;
        seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, "sustain", BAND_WORKERS);
        let sustain = source_actual(&app, band);
        assign_forage(&mut app, faction, coord, "eradicate", BAND_WORKERS);
        let eradicate = source_actual(&app, band);

        assert!(
            eradicate > sustain,
            "Eradicate must re-seed a higher expectation than Sustain (sustain {sustain}, eradicate {eradicate})"
        );
    }

    /// **A barren source still reads `+0.00`.** The seed is a forecast, not a fiction: a patch with no
    /// biomass yields nothing, so `+0.00` stays reachable — and correct — there.
    #[test]
    fn a_barren_source_seeds_zero() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        seed_patch_with_biomass(&mut app, coord, 0.0, EcologyPhase::Collapsing);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, "sustain", BAND_WORKERS);

        assert_eq!(
            source_actual(&app, band),
            0.0,
            "a barren patch must still seed a zero yield"
        );
    }

    /// **Unassigning drops the row.** Setting a source to zero workers removes its assignment *and* its
    /// telemetry row, so the derived `last_yields` stays index-aligned with `assignments` (the snapshot
    /// zips the two by index — a stale row would be attributed to another source).
    #[test]
    fn unassigning_a_source_drops_its_yield_row() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let stocked = forage_carrying_capacity(&app) * 0.5;
        seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, "sustain", BAND_WORKERS);
        assign_forage(&mut app, faction, coord, "sustain", 0);

        let allocation = app.world.get::<LaborAllocation>(band).unwrap();
        assert!(allocation.assignments.is_empty(), "the source is unstaffed");
        assert!(
            allocation.last_yields.is_empty(),
            "its telemetry row must go with it"
        );
    }
}
