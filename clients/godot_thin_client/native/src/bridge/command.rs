//! `CommandBridge` -- the GDScript-facing command submission surface and the
//! background worker thread that carries envelopes to the server.

use godot::prelude::*;
use sim_runtime::{parse_command_line, CommandEnvelope};
use std::sync::mpsc::Sender;
use std::sync::{mpsc, OnceLock};
use std::thread;

use crate::runtime::transmit_proto_command;

#[derive(GodotClass)]
#[class(base = RefCounted, init)]
pub struct CommandBridge;

static COMMAND_BRIDGE_SENDER: OnceLock<Sender<CommandRequest>> = OnceLock::new();

struct CommandRequest {
    host: String,
    port: u16,
    envelope: CommandEnvelope,
    callback: Sender<CommandResult>,
}

struct CommandResult {
    ok: bool,
    error: Option<String>,
}

#[godot_api]
impl CommandBridge {
    #[allow(dead_code)]
    fn init(_base: Base<RefCounted>) -> Self {
        let _ = command_sender();
        Self
    }

    #[func]
    pub fn send_line(&self, host: GString, proto_port: i64, line: GString) -> VarDictionary {
        let mut dict = VarDictionary::new();
        if proto_port <= 0 || proto_port > u16::MAX as i64 {
            let _ = dict.insert("ok", false);
            let _ = dict.insert("error", format!("invalid port {proto_port}"));
            return dict;
        }

        let host_str = host.to_string();
        let line_str = line.to_string();

        let envelope = match parse_command_line(&line_str) {
            Ok(payload) => CommandEnvelope {
                payload,
                correlation_id: None,
            },
            Err(err) => {
                let _ = dict.insert("ok", false);
                let _ = dict.insert("error", err.to_string());
                return dict;
            }
        };

        let sender = command_sender();

        let (tx, rx) = mpsc::channel();
        if let Err(err) = sender.send(CommandRequest {
            host: host_str,
            port: proto_port as u16,
            envelope,
            callback: tx,
        }) {
            let _ = dict.insert("ok", false);
            let _ = dict.insert("error", format!("dispatch error: {err}"));
            return dict;
        }

        match rx.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(result) => {
                let _ = dict.insert("ok", result.ok);
                if let Some(err) = result.error {
                    let _ = dict.insert("error", err);
                }
            }
            Err(_) => {
                let _ = dict.insert("ok", false);
                let _ = dict.insert("error", "command timed out");
            }
        }

        dict
    }
}

fn prototype_command_worker(receiver: mpsc::Receiver<CommandRequest>) {
    for request in receiver {
        let result = match transmit_proto_command(&request.host, request.port, &request.envelope) {
            Ok(_) => CommandResult {
                ok: true,
                error: None,
            },
            Err(err) => CommandResult {
                ok: false,
                error: Some(err),
            },
        };

        let _ = request.callback.send(result);
    }
}

fn command_sender() -> Sender<CommandRequest> {
    COMMAND_BRIDGE_SENDER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<CommandRequest>();
            thread::Builder::new()
                .name("command-bridge-worker".into())
                .spawn(move || prototype_command_worker(receiver))
                .expect("failed to spawn command bridge worker thread");
            sender
        })
        .clone()
}

pub(crate) fn resolve_entry_path(manifest_path: &str, entry: &str) -> String {
    if entry.starts_with("res://") || entry.starts_with("user://") {
        return entry.to_string();
    }
    let base = manifest_path
        .rfind('/')
        .map(|idx| &manifest_path[..=idx])
        .unwrap_or("");
    let trimmed = entry.strip_prefix("./").unwrap_or(entry);
    if base.is_empty() {
        trimmed.to_string()
    } else {
        format!("{base}{trimmed}")
    }
}
