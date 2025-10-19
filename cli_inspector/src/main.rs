use std::io::Write;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use bincode::deserialize;
use clap::Parser;
use color_eyre::Result;
use sim_proto::WorldDelta;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, trace, warn};

mod app;
mod ui;

use app::{channel, ClientCommand, InspectorApp};

#[derive(Clone)]
struct ChannelWriter {
    sender: Sender<String>,
}

impl std::io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(text) = String::from_utf8(buf.to_vec()) {
            let _ = self.sender.send(text);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Shadow-Scale CLI inspector prototype", long_about = None)]
struct Cli {
    /// Address of the headless simulation server providing snapshot deltas.
    #[arg(long, default_value = "127.0.0.1:41000")]
    endpoint: String,
    /// Address for sending control commands to the simulation.
    #[arg(long, default_value = "127.0.0.1:41001")]
    command_endpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let (log_tx, log_rx) = mpsc::channel::<String>();
    let log_writer_tx = log_tx.clone();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .compact()
        .with_writer(move || ChannelWriter {
            sender: log_writer_tx.clone(),
        })
        .init();

    let cli = Cli::parse();
    info!("Connecting to simulation at {}", cli.endpoint);

    let (sender, receiver) = channel();
    let (command_tx, command_rx) = mpsc::channel::<ClientCommand>();
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    let command_endpoint = cli.command_endpoint.clone();
    let _command_handle =
        std::thread::spawn(move || run_command_pump(command_endpoint, command_rx));

    let _ui_handle = std::thread::spawn(move || -> color_eyre::Result<()> {
        let app = InspectorApp::new(receiver, command_tx, shutdown_tx, log_rx)?;
        app.run()
    });

    loop {
        if shutdown_rx.try_recv().is_ok() {
            info!("Inspector requested shutdown");
            break;
        }
        match TcpStream::connect(&cli.endpoint).await {
            Ok(mut stream) => {
                info!("Connected. Streaming snapshot deltas. Press Ctrl+C or 'q' to exit.");
                if let Err(err) = pump_deltas(&mut stream, &sender).await {
                    warn!("Connection error: {}", err);
                    info!("Reconnecting in 2 seconds...");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
            Err(err) => {
                warn!("Failed to connect: {}", err);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    Ok(())
}

async fn pump_deltas(stream: &mut TcpStream, sender: &UnboundedSender<WorldDelta>) -> Result<()> {
    let mut len_buf = [0u8; 4];
    loop {
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await?;
        let delta: WorldDelta = deserialize(&payload)?;
        trace!(tick = delta.header.tick, "snapshot.delta");
        if sender.send(delta).is_err() {
            break;
        }
    }
    Ok(())
}

fn run_command_pump(endpoint: String, receiver: Receiver<ClientCommand>) {
    for cmd in receiver {
        match send_command(&endpoint, &cmd) {
            Ok(_) => info!(?cmd, "command.sent"),
            Err(err) => warn!(?cmd, "Failed to send command: {}", err),
        }
    }
}

fn send_command(endpoint: &str, command: &ClientCommand) -> std::io::Result<()> {
    let mut stream = std::net::TcpStream::connect(endpoint)?;
    let line = match command {
        ClientCommand::Turn(amount) => format!("turn {}\n", amount),
        ClientCommand::Heat { entity, delta } => format!("heat {} {}\n", entity, delta),
        ClientCommand::SubmitOrders { faction } => format!("order {} ready\n", faction),
        ClientCommand::Rollback { tick } => format!("rollback {}\n", tick),
        ClientCommand::SetAxisBias { axis, value } => {
            format!("bias {} {:.6}\n", axis, value)
        }
    };
    stream.write_all(line.as_bytes())?;
    Ok(())
}
