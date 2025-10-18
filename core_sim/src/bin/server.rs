use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::thread;

use bevy::app::Update;
use crossbeam_channel::{unbounded, Receiver, Sender};
use tracing::{info, warn};

use core_sim::metrics::{collect_metrics, SimulationMetrics};
use core_sim::network::{broadcast_latest, start_snapshot_server};
use core_sim::{build_headless_app, run_turn, Scalar, SimulationConfig, SnapshotHistory, Tile};

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
                    run_turn(&mut app);
                    let history = app.world.resource::<SnapshotHistory>();
                    broadcast_latest(snapshot_server.as_ref(), &history);

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
        }
    }
}

#[derive(Debug)]
enum Command {
    Turn(u32),
    Heat { entity: u64, delta: i64 },
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
