use std::io::{self, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use bevy::{app::Update, ecs::system::Resource, math::UVec2};
use crossbeam_channel::{unbounded, Receiver, Sender};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{info, warn};
use tracing_subscriber::prelude::*;

use core_sim::log_stream::start_log_stream_server;

use core_sim::metrics::{collect_metrics, SimulationMetrics};
use core_sim::network::{broadcast_latest, start_snapshot_server, SnapshotServer};
use core_sim::{
    build_headless_app, restore_world_from_snapshot, run_turn, CorruptionLedgers,
    EspionageAgentHandle, EspionageCatalog, EspionageMissionId, EspionageMissionState,
    EspionageRoster, FactionId, FactionOrders, FactionRegistry, GenerationId, GenerationRegistry,
    InfluencerImpacts, InfluentialRoster, QueueMissionParams, Scalar, SentimentAxisBias,
    SimulationConfig, SimulationConfigMetadata, SimulationTick, SnapshotHistory,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, SnapshotOverlaysConfigMetadata,
    StoredSnapshot, SubmitError, SubmitOutcome, SupportChannel, Tile, TurnPipelineConfig,
    TurnPipelineConfigHandle, TurnPipelineConfigMetadata, TurnQueue,
};
use sim_runtime::{
    commands::{EspionageGeneratorUpdate as CommandGeneratorUpdate, ReloadConfigKind},
    AxisBiasState, CommandEnvelope as ProtoCommandEnvelope, CommandPayload as ProtoCommandPayload,
    CorruptionEntry, CorruptionSubsystem, InfluenceScopeKind,
    OrdersDirective as ProtoOrdersDirective, SupportChannel as ProtoSupportChannel,
};

fn main() {
    let mut app = build_headless_app();
    app.insert_resource(SimulationMetrics::default());
    app.add_systems(Update, collect_metrics);

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
                let current_config = app.world.resource::<SimulationConfig>().clone();
                if current_config.grid_size.x == width && current_config.grid_size.y == height {
                    info!(
                        target: "shadow_scale::server",
                        width,
                        height,
                        "map.reset.skipped=dimensions_unchanged"
                    );
                    continue;
                }
                info!(
                    target: "shadow_scale::server",
                    width,
                    height,
                    "map.reset.begin"
                );
                let mut new_config = current_config.clone();
                new_config.grid_size = UVec2::new(width, height);

                let mut new_app = build_headless_app();
                {
                    let mut config_res = new_app.world.resource_mut::<SimulationConfig>();
                    *config_res = new_config;
                }
                new_app.insert_resource(SimulationMetrics::default());
                new_app.add_systems(Update, collect_metrics);
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
                    "map.reset.completed"
                );
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
            Command::ReloadConfig { kind, path } => {
                handle_reload_config(&mut app, kind, path);
            }
        }
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
    ReloadConfig {
        kind: ReloadConfigKind,
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
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                    if last_emit.elapsed() >= debounce {
                        if sender
                            .send(Command::ReloadConfig { kind, path: None })
                            .is_err()
                        {
                            break;
                        }
                        last_emit = Instant::now();
                    }
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
    }
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
        ProtoCommandPayload::ReloadConfig { kind, path } => {
            Some(Command::ReloadConfig { kind, path })
        }
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
    let agent_handle = params.agent.0;

    let queue_result = app.world.resource_scope(
        |world, mut missions: bevy::prelude::Mut<EspionageMissionState>| {
            let queued_params = params.clone();
            world.resource_scope(|world, mut roster: bevy::prelude::Mut<EspionageRoster>| {
                let catalog = world.resource::<EspionageCatalog>();
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
                agent_handle,
                target_tier = ?params.target_tier,
                scheduled_tick = params.scheduled_tick,
                instance = instance_id.0,
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
                agent_handle,
                target_tier = ?params.target_tier,
                scheduled_tick = params.scheduled_tick,
                %error,
                "espionage.mission.queue_failed"
            );
        }
    }
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
