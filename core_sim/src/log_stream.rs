use crossbeam_channel::{unbounded, Receiver, Sender};
use serde::Serialize;
use std::io::{self, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// ⛔ **THE LAUNCHER'S CHANNEL, AND IT IS HAND-TARGETED.** The server's operator-facing events name
/// this target rather than the crate, and the launcher supervises its `sim_ai` children off one of
/// them ([`emit_seats_roster`]) — so the log-forward path must admit it whatever `RUST_LOG` says
/// (`core_sim/src/bin/server.rs` → `log_forward_filter`).
pub const LAUNCHER_LOG_TARGET: &str = "shadow_scale::server";

/// **The launcher's supervisor contract, emitted from exactly one place.** `retain_claimed_seats`
/// calls this at every world rebuild; the launcher's `parse_roster_event`
/// (`launcher/src/main.rs`) is its twin, and its test drives *this function* so that changing a
/// field's sigil here fails there instead of shipping a game with no rivals.
///
/// ⛔ **`factions` GOES ON THE WIRE AS A JSON ARRAY INSIDE A STRING**, because `tracing` fields
/// carry no arrays. `%` (Display) is what puts the bare `[0,1,2]` there; `?` (Debug) would quote
/// and escape it into `"\"[0,1,2]\""`, which the launcher's `from_str::<Vec<u32>>` cannot read.
pub fn emit_seats_roster(faction_ids: &[u32], world_epoch: u32) {
    let factions = serde_json::to_string(faction_ids).expect("a list of ids serialises");
    info!(
        target: LAUNCHER_LOG_TARGET,
        %factions,
        world_epoch,
        "seats.roster"
    );
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEnvelope {
    pub timestamp_ms: u64,
    pub level: String,
    pub target: String,
    pub message: String,
    #[serde(skip_serializing_if = "map_is_empty")]
    pub fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone)]
pub struct LogForwardLayer {
    sender: Sender<LogEnvelope>,
}

pub struct LogStreamHandle {
    sender: Sender<LogEnvelope>,
}

/// Starts the log stream server on an already-bound listener.
///
/// Binding happens up front in `port_alloc::allocate`, so the only remaining
/// failure mode is `set_nonblocking` — hence the `Option` return stays.
pub fn start_log_stream_server(listener: TcpListener) -> Option<LogStreamHandle> {
    if let Err(err) = listener.set_nonblocking(true) {
        eprintln!("set_nonblocking failed for log stream listener: {}", err);
        return None;
    }

    let (sender, receiver) = unbounded::<LogEnvelope>();
    let clients: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    let accept_clients = Arc::clone(&clients);

    thread::spawn(move || run_log_stream(listener, accept_clients, receiver));

    Some(LogStreamHandle { sender })
}

fn run_log_stream(
    listener: TcpListener,
    clients: Arc<Mutex<Vec<TcpStream>>>,
    receiver: Receiver<LogEnvelope>,
) {
    loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                if let Err(err) = stream.set_nodelay(true) {
                    warn!("Failed to set TCP_NODELAY for log client {}: {}", addr, err);
                }
                let mut guard = clients.lock().expect("log clients mutex poisoned");
                guard.push(stream);
                drop(guard);
                info!("Log stream client connected: {}", addr);
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => {
                error!("Error accepting log client: {}", err);
                thread::sleep(Duration::from_millis(200));
            }
        }

        while let Ok(event) = receiver.try_recv() {
            if let Ok(bytes) = serde_json::to_vec(&event) {
                broadcast_payload(&clients, &bytes);
            }
        }

        thread::sleep(Duration::from_millis(16));
    }
}

fn broadcast_payload(clients: &Arc<Mutex<Vec<TcpStream>>>, payload: &[u8]) {
    let mut guard = clients.lock().expect("log clients mutex poisoned");
    guard.retain_mut(|stream| {
        let len = payload.len() as u32;
        let mut buffer = Vec::with_capacity(4 + payload.len());
        buffer.extend_from_slice(&len.to_le_bytes());
        buffer.extend_from_slice(payload);
        match stream.write_all(&buffer) {
            Ok(_) => true,
            Err(err) => {
                warn!("Dropping log client: {}", err);
                false
            }
        }
    });
}

impl LogStreamHandle {
    pub fn layer(&self) -> LogForwardLayer {
        LogForwardLayer {
            sender: self.sender.clone(),
        }
    }

    pub fn sender(&self) -> Sender<LogEnvelope> {
        self.sender.clone()
    }
}

impl LogForwardLayer {
    pub fn new(sender: Sender<LogEnvelope>) -> Self {
        Self { sender }
    }
}

impl<S> Layer<S> for LogForwardLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let message = visitor
            .message
            .clone()
            .unwrap_or_else(|| metadata.target().to_string());
        let envelope = LogEnvelope {
            timestamp_ms,
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message,
            fields: visitor.fields,
        };
        let _ = self.sender.send(envelope);
    }
}

#[derive(Default)]
struct LogVisitor {
    message: Option<String>,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl LogVisitor {
    fn record_value(&mut self, field: &tracing::field::Field, value: serde_json::Value) {
        if field.name() == "message" {
            if let serde_json::Value::String(text) = value {
                self.message = Some(text);
            } else {
                self.message = Some(value.to_string());
            }
        } else {
            self.fields.insert(field.name().to_string(), value);
        }
    }
}

impl tracing::field::Visit for LogVisitor {
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.record_value(field, serde_json::Value::Bool(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.record_value(field, value.into());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.record_value(field, value.into());
    }

    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        self.record_value(field, serde_json::Value::String(value.to_string()));
    }

    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        self.record_value(field, serde_json::Value::String(value.to_string()));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        if let Some(number) = serde_json::Number::from_f64(value) {
            self.record_value(field, serde_json::Value::Number(number));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_value(field, serde_json::Value::String(value.to_string()));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.record_value(field, serde_json::Value::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record_value(field, serde_json::Value::String(format!("{:?}", value)));
    }
}

fn map_is_empty(map: &serde_json::Map<String, serde_json::Value>) -> bool {
    map.is_empty()
}
