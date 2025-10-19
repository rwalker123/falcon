use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::thread;

use bevy::app::Update;
use crossbeam_channel::{unbounded, Receiver, Sender};
use tracing::{info, warn};

use core_sim::metrics::{collect_metrics, SimulationMetrics};
use core_sim::network::{broadcast_latest, start_snapshot_server, SnapshotServer};
use core_sim::{
    build_headless_app, restore_world_from_snapshot, run_turn, FactionId, FactionOrders, Scalar,
    SentimentAxisBias, SimulationConfig, SimulationTick, SnapshotHistory, StoredSnapshot,
    SubmitError, SubmitOutcome, Tile, TurnQueue,
};
use sim_proto::AxisBiasState;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut app = build_headless_app();
    app.insert_resource(SimulationMetrics::default());
    app.add_systems(Update, collect_metrics);

    let config = app.world.resource::<SimulationConfig>().clone();

    let snapshot_server = start_snapshot_server(config.snapshot_bind);
    let command_rx = spawn_command_listener(config.command_bind);

    info!(
        command_bind = %config.command_bind,
        snapshot_bind = %config.snapshot_bind,
        "Shadow-Scale headless server ready"
    );

    while let Ok(command) = command_rx.recv() {
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
                    resolve_ready_turn(&mut app, snapshot_server.as_ref());
                }
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
                handle_order_submission(&mut app, faction, orders, snapshot_server.as_ref());
            }
            Command::Rollback { tick } => {
                handle_rollback(&mut app, tick, snapshot_server.as_ref());
            }
            Command::AxisBias { axis, value } => {
                handle_axis_bias(&mut app, axis, value, snapshot_server.as_ref());
            }
        }
    }
}

#[derive(Debug)]
enum Command {
    Turn(u32),
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
}

fn spawn_command_listener(bind_addr: std::net::SocketAddr) -> Receiver<Command> {
    let listener = TcpListener::bind(bind_addr).expect("command listener bind failed");
    listener
        .set_nonblocking(true)
        .expect("set_nonblocking failed");

    let (sender, receiver) = unbounded::<Command>();
    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                info!("Command client connected: {}", addr);
                let sender = sender.clone();
                thread::spawn(move || handle_client(stream, sender));
            }
            Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(err) => {
                warn!("Error accepting command client: {}", err);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    });

    receiver
}

fn handle_client(stream: std::net::TcpStream, sender: Sender<Command>) {
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
                match parse_command(trimmed) {
                    Some(cmd) => {
                        if sender.send(cmd).is_err() {
                            break;
                        }
                    }
                    None => warn!("Invalid command: {}", trimmed),
                }
            }
            Err(err) => {
                warn!("Command read error: {}", err);
                break;
            }
        }
    }
}

fn parse_command(input: &str) -> Option<Command> {
    let mut parts = input.split_whitespace();
    match parts.next()? {
        "turn" => {
            let amount = parts.next().unwrap_or("1").parse().ok()?;
            Some(Command::Turn(amount))
        }
        "heat" => {
            let entity: u64 = parts.next()?.parse().ok()?;
            let delta: i64 = parts.next().unwrap_or("100000").parse().ok()?;
            Some(Command::Heat { entity, delta })
        }
        "order" => {
            let faction: u32 = parts.next()?.parse().ok()?;
            let directive = parts.next().unwrap_or("ready");
            match directive {
                "ready" | "end" | "commit" => Some(Command::Orders {
                    faction: FactionId(faction),
                    orders: FactionOrders::end_turn(),
                }),
                other => {
                    warn!("Unsupported order directive: {}", other);
                    None
                }
            }
        }
        "rollback" => {
            let target: u64 = parts.next()?.parse().ok()?;
            Some(Command::Rollback { tick: target })
        }
        "bias" => {
            let axis: usize = parts.next()?.parse().ok()?;
            let value: f32 = parts.next()?.parse().ok()?;
            Some(Command::AxisBias { axis, value })
        }
        _ => None,
    }
}

fn apply_heat(app: &mut bevy::prelude::App, entity_bits: u64, delta_raw: i64) {
    let entity = bevy::prelude::Entity::from_bits(entity_bits);
    if let Some(mut tile) = app.world.get_mut::<Tile>(entity) {
        tile.temperature = tile.temperature + Scalar::from_raw(delta_raw);
    } else {
        warn!("Entity {} not found for heat command", entity_bits);
    }
}

fn handle_order_submission(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    orders: FactionOrders,
    snapshot_server: Option<&SnapshotServer>,
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
            resolve_ready_turn(app, snapshot_server);
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
    snapshot_server: Option<&SnapshotServer>,
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
        bias_res.set_axis(axis, Scalar::from_f32(clamped));
    }

    let bias_state = {
        let bias_res = app.world.resource::<SentimentAxisBias>();
        AxisBiasState {
            knowledge: bias_res.values[0].raw(),
            trust: bias_res.values[1].raw(),
            equity: bias_res.values[2].raw(),
            agency: bias_res.values[3].raw(),
        }
    };

    let broadcast_payload = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.update_axis_bias(bias_state)
    };

    if let (Some(server), Some(payload)) = (snapshot_server, broadcast_payload) {
        server.broadcast(payload.as_ref());
    }

    info!(
        target: "shadow_scale::server",
        axis,
        value = clamped,
        "axis_bias.updated"
    );
}

fn resolve_ready_turn(app: &mut bevy::prelude::App, snapshot_server: Option<&SnapshotServer>) {
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
    broadcast_latest(snapshot_server, history);

    let metrics = app.world.resource::<SimulationMetrics>();
    info!(
        target: "shadow_scale::server",
        turn = metrics.turn,
        grid_width = metrics.grid_size.0,
        grid_height = metrics.grid_size.1,
        total_mass = metrics.total_mass,
        avg_temp = metrics.avg_temperature,
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
    snapshot_server: Option<&SnapshotServer>,
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

    if let Some(server) = snapshot_server {
        server.broadcast(entry.encoded_snapshot.as_ref());
    }
}
