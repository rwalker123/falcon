use std::io::{self, BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::thread;

use bevy::{app::Update, math::UVec2};
use crossbeam_channel::{unbounded, Receiver, Sender};
use tracing::{info, warn};
use tracing_subscriber::prelude::*;

use core_sim::log_stream::start_log_stream_server;

use core_sim::metrics::{collect_metrics, SimulationMetrics};
use core_sim::network::{broadcast_latest, start_snapshot_server, SnapshotServer};
use core_sim::{
    build_headless_app, restore_world_from_snapshot, run_turn, CorruptionLedgers, FactionId,
    FactionOrders, GenerationId, GenerationRegistry, InfluencerImpacts, InfluentialRoster, Scalar,
    SentimentAxisBias, SimulationConfig, SimulationTick, SnapshotHistory, StoredSnapshot,
    SubmitError, SubmitOutcome, SupportChannel, Tile, TurnQueue,
};
use sim_runtime::{
    parse_command_line, AxisBiasState, CommandEnvelope as ProtoCommandEnvelope, CommandParseError,
    CommandPayload as ProtoCommandPayload, CorruptionEntry, CorruptionSubsystem,
    InfluenceScopeKind, OrdersDirective as ProtoOrdersDirective,
    SupportChannel as ProtoSupportChannel,
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
    let command_rx = spawn_command_listener(config.command_bind, config.command_proto_bind);

    info!(
        command_bind = %config.command_bind,
        command_proto_bind = %config.command_proto_bind,
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
}

enum InfluencerAction {
    Support,
    Suppress,
}

const MAX_PROTO_FRAME: usize = 64 * 1024;

fn spawn_command_listener(
    text_bind: std::net::SocketAddr,
    proto_bind: std::net::SocketAddr,
) -> Receiver<Command> {
    let (sender, receiver) = unbounded::<Command>();
    spawn_text_command_listener(text_bind, sender.clone());
    spawn_proto_command_listener(proto_bind, sender);
    receiver
}

fn spawn_text_command_listener(bind_addr: std::net::SocketAddr, sender: Sender<Command>) {
    let listener = match TcpListener::bind(bind_addr) {
        Ok(listener) => listener,
        Err(err) => {
            warn!(
                "Text command listener bind failed at {}: {}",
                bind_addr, err
            );
            return;
        }
    };
    if let Err(err) = listener.set_nonblocking(true) {
        warn!(
            "Failed to set nonblocking on text command listener: {}",
            err
        );
    }

    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                info!("Text command client connected: {}", addr);
                let sender = sender.clone();
                thread::spawn(move || handle_text_client(stream, sender));
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(err) => {
                warn!("Error accepting text command client: {}", err);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    });
}

fn spawn_proto_command_listener(bind_addr: std::net::SocketAddr, sender: Sender<Command>) {
    let listener = match TcpListener::bind(bind_addr) {
        Ok(listener) => listener,
        Err(err) => {
            warn!(
                "Proto command listener bind failed at {}: {}",
                bind_addr, err
            );
            return;
        }
    };
    if let Err(err) = listener.set_nonblocking(true) {
        warn!(
            "Failed to set nonblocking on proto command listener: {}",
            err
        );
    }

    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                info!("Proto command client connected: {}", addr);
                let sender = sender.clone();
                thread::spawn(move || handle_proto_client(stream, sender));
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(err) => {
                warn!("Error accepting proto command client: {}", err);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    });
}

fn handle_text_client(stream: TcpStream, sender: Sender<Command>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match parse_command_line(trimmed) {
                    Ok(payload) => {
                        if let Some(cmd) = command_from_payload(payload) {
                            if sender.send(cmd).is_err() {
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        log_parse_error(trimmed, &err);
                    }
                }
            }
            Err(err) => {
                warn!("Text command read error: {}", err);
                break;
            }
        }
    }
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

fn log_parse_error(command: &str, error: &CommandParseError) {
    warn!("Invalid command '{}': {}", command, error);
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
