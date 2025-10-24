use anyhow::Result;
use quick_js::{Arguments, Context, JsValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("manifest error: {0}")]
    Manifest(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub entry: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub subscriptions: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub config: Option<JsonValue>,
}

impl Manifest {
    pub fn parse_str(contents: &str) -> Result<Self, ScriptError> {
        let manifest: Manifest = serde_json::from_str(contents).map_err(|err| {
            ScriptError::Manifest(format!("failed to parse manifest JSON: {err}"))
        })?;
        if manifest.id.trim().is_empty() {
            return Err(ScriptError::Manifest("manifest id cannot be empty".into()));
        }
        if manifest.entry.trim().is_empty() {
            return Err(ScriptError::Manifest(
                "manifest entry cannot be empty".into(),
            ));
        }
        Ok(manifest)
    }
}

impl std::str::FromStr for Manifest {
    type Err = ScriptError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Manifest::parse_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScriptResponse {
    Ready,
    Log {
        level: LogLevel,
        message: String,
    },
    Error {
        message: String,
    },
    HostRequest {
        request_id: u64,
        op: String,
        payload: JsonValue,
    },
    Alert {
        level: String,
        title: String,
        message: String,
    },
    Event {
        event: String,
        payload: JsonValue,
    },
    TickMetrics {
        elapsed_ms: f64,
    },
    OverBudget {
        elapsed_ms: f64,
        budget_ms: f64,
    },
    Subscriptions {
        topics: Vec<String>,
    },
    SessionUpdated,
    Terminated,
}

#[derive(Debug, Clone)]
pub enum ScriptCommand {
    Dispatch { event: String, payload: JsonValue },
    Tick { delta: f64, budget_ms: f64 },
    Shutdown,
    RestoreSession(JsonValue),
}

struct ScriptSharedState {
    id: i64,
    capabilities: HashSet<String>,
    subscriptions: Mutex<HashSet<String>>,
    session: Mutex<JsonValue>,
    responses_tx: Sender<ScriptResponse>,
    on_event: Mutex<Option<String>>,
    on_tick: Mutex<Option<String>>,
    registered: AtomicBool,
    registration_cv: Condvar,
    request_counter: AtomicU64,
}

impl ScriptSharedState {
    fn new(
        id: i64,
        capabilities: HashSet<String>,
        responses_tx: Sender<ScriptResponse>,
    ) -> Arc<Self> {
        Arc::new(Self {
            id,
            capabilities,
            subscriptions: Mutex::new(HashSet::new()),
            session: Mutex::new(JsonValue::Object(JsonMap::new())),
            responses_tx,
            on_event: Mutex::new(None),
            on_tick: Mutex::new(None),
            registered: AtomicBool::new(false),
            registration_cv: Condvar::new(),
            request_counter: AtomicU64::new(1),
        })
    }

    fn ensure_capability(&self, required: &str) -> Result<(), ScriptError> {
        if self
            .capabilities
            .iter()
            .any(|cap| cap == required || cap.starts_with(required))
        {
            Ok(())
        } else {
            Err(ScriptError::Runtime(format!(
                "script {0} missing capability {required}",
                self.id
            )))
        }
    }
}

#[derive(Clone)]
pub struct ScriptManager {
    inner: Arc<ScriptManagerInner>,
}

struct ScriptManagerInner {
    next_id: AtomicI64,
    scripts: Mutex<HashMap<i64, ManagedScript>>,
}

struct ManagedScript {
    manifest: Manifest,
    shared: Arc<ScriptSharedState>,
    command_tx: Sender<ScriptCommand>,
    responses_rx: Receiver<ScriptResponse>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Drop for ManagedScript {
    fn drop(&mut self) {
        let _ = self.command_tx.send(ScriptCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl ScriptManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ScriptManagerInner {
                next_id: AtomicI64::new(1),
                scripts: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn spawn_script(&self, manifest: Manifest, source: String) -> Result<i64, ScriptError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        let (command_tx, command_rx) = mpsc::channel::<ScriptCommand>();
        let (responses_tx, responses_rx) = mpsc::channel::<ScriptResponse>();
        let capabilities: HashSet<String> = manifest.capabilities.iter().cloned().collect();
        if !manifest.subscriptions.is_empty()
            && !manifest
                .capabilities
                .iter()
                .any(|cap| cap == "telemetry.subscribe")
        {
            return Err(ScriptError::Manifest(
                "subscriptions declared without telemetry.subscribe capability".into(),
            ));
        }

        let shared = ScriptSharedState::new(id, capabilities, responses_tx.clone());
        {
            let mut subs = shared.subscriptions.lock().unwrap();
            for topic in &manifest.subscriptions {
                subs.insert(topic.clone());
            }
        }

        let manifest_clone = manifest.clone();
        let shared_clone = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name(format!("script-worker-{}", manifest.id))
            .spawn(move || {
                if let Err(err) =
                    run_script_worker(manifest_clone, source, shared_clone, command_rx)
                {
                    let _ = responses_tx.send(ScriptResponse::Error {
                        message: err.to_string(),
                    });
                }
                let _ = responses_tx.send(ScriptResponse::Terminated);
            })
            .map_err(|err| ScriptError::Runtime(format!("failed to spawn script thread: {err}")))?;

        let mut guard = self.inner.scripts.lock().unwrap();
        guard.insert(
            id,
            ManagedScript {
                manifest,
                shared,
                command_tx,
                responses_rx,
                handle: Some(handle),
            },
        );
        Ok(id)
    }

    pub fn broadcast_event(&self, event: &str, payload: JsonValue) {
        let guard = self.inner.scripts.lock().unwrap();
        for script in guard.values() {
            let _ = script.command_tx.send(ScriptCommand::Dispatch {
                event: event.to_string(),
                payload: payload.clone(),
            });
        }
    }

    pub fn dispatch_event(
        &self,
        id: i64,
        event: &str,
        payload: JsonValue,
    ) -> Result<(), ScriptError> {
        let guard = self.inner.scripts.lock().unwrap();
        let script = guard
            .get(&id)
            .ok_or_else(|| ScriptError::Runtime(format!("unknown script id {id}")))?;
        script
            .command_tx
            .send(ScriptCommand::Dispatch {
                event: event.to_string(),
                payload,
            })
            .map_err(|err| ScriptError::Runtime(format!("failed to send dispatch: {err}")))
    }

    pub fn tick(&self, id: i64, delta: f64, budget_ms: f64) -> Result<(), ScriptError> {
        let guard = self.inner.scripts.lock().unwrap();
        let script = guard
            .get(&id)
            .ok_or_else(|| ScriptError::Runtime(format!("unknown script id {id}")))?;
        script
            .command_tx
            .send(ScriptCommand::Tick { delta, budget_ms })
            .map_err(|err| ScriptError::Runtime(format!("failed to send tick: {err}")))
    }

    pub fn shutdown(&self, id: i64) {
        let mut guard = self.inner.scripts.lock().unwrap();
        guard.remove(&id);
    }

    pub fn poll_responses(&self, id: i64) -> Result<Vec<ScriptResponse>, ScriptError> {
        let guard = self.inner.scripts.lock().unwrap();
        let script = guard
            .get(&id)
            .ok_or_else(|| ScriptError::Runtime(format!("unknown script id {id}")))?;
        Ok(collect_responses(&script.responses_rx))
    }

    pub fn poll_all(&self) -> HashMap<i64, Vec<ScriptResponse>> {
        let guard = self.inner.scripts.lock().unwrap();
        let mut output = HashMap::new();
        for (id, script) in guard.iter() {
            let responses = collect_responses(&script.responses_rx);
            if !responses.is_empty() {
                output.insert(*id, responses);
            }
        }
        output
    }

    pub fn get_manifest(&self, id: i64) -> Option<Manifest> {
        let guard = self.inner.scripts.lock().unwrap();
        guard.get(&id).map(|script| script.manifest.clone())
    }

    pub fn list_scripts(&self) -> Vec<(i64, Manifest)> {
        let guard = self.inner.scripts.lock().unwrap();
        guard
            .iter()
            .map(|(id, script)| (*id, script.manifest.clone()))
            .collect()
    }

    pub fn subscriptions(&self, id: i64) -> Result<Vec<String>, ScriptError> {
        let guard = self.inner.scripts.lock().unwrap();
        let script = guard
            .get(&id)
            .ok_or_else(|| ScriptError::Runtime(format!("unknown script id {id}")))?;
        let subs = script.shared.subscriptions.lock().unwrap();
        Ok(subs.iter().cloned().collect())
    }

    pub fn snapshot_session(&self, id: i64) -> Result<JsonValue, ScriptError> {
        let guard = self.inner.scripts.lock().unwrap();
        let script = guard
            .get(&id)
            .ok_or_else(|| ScriptError::Runtime(format!("unknown script id {id}")))?;
        let value = script.shared.session.lock().unwrap();
        Ok(value.clone())
    }

    pub fn restore_session(&self, id: i64, value: JsonValue) -> Result<(), ScriptError> {
        let guard = self.inner.scripts.lock().unwrap();
        let script = guard
            .get(&id)
            .ok_or_else(|| ScriptError::Runtime(format!("unknown script id {id}")))?;
        {
            let mut session = script.shared.session.lock().unwrap();
            *session = value.clone();
        }
        script
            .command_tx
            .send(ScriptCommand::RestoreSession(value))
            .map_err(|err| ScriptError::Runtime(format!("failed to queue session restore: {err}")))
    }
}

impl Default for ScriptManager {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_responses(rx: &Receiver<ScriptResponse>) -> Vec<ScriptResponse> {
    let mut responses = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(resp) => responses.push(resp),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
    responses
}

fn run_script_worker(
    _manifest: Manifest,
    source: String,
    shared: Arc<ScriptSharedState>,
    command_rx: Receiver<ScriptCommand>,
) -> Result<(), ScriptError> {
    let ctx = Context::new()
        .map_err(|err| ScriptError::Runtime(format!("failed to create JS context: {err}")))?;

    install_host(&ctx, Arc::clone(&shared))
        .map_err(|err| ScriptError::Runtime(format!("failed to install host bindings: {err}")))?;
    ctx.eval(source.as_str())
        .map_err(|err| ScriptError::Runtime(format!("script execution error: {err}")))?;

    wait_for_registration(&shared);
    let _ = shared.responses_tx.send(ScriptResponse::Ready);
    {
        let subs = shared.subscriptions.lock().unwrap();
        if !subs.is_empty() {
            let _ = shared.responses_tx.send(ScriptResponse::Subscriptions {
                topics: subs.iter().cloned().collect(),
            });
        }
    }

    loop {
        match command_rx.recv() {
            Ok(ScriptCommand::Dispatch { event, payload }) => {
                if let Err(err) =
                    invoke_callback(&ctx, &shared, CallbackKind::Event(event), payload)
                {
                    let _ = shared
                        .responses_tx
                        .send(ScriptResponse::Error { message: err });
                }
            }
            Ok(ScriptCommand::Tick { delta, budget_ms }) => {
                let start = std::time::Instant::now();
                if let Err(err) =
                    invoke_callback(&ctx, &shared, CallbackKind::Tick(delta), JsonValue::Null)
                {
                    let _ = shared
                        .responses_tx
                        .send(ScriptResponse::Error { message: err });
                }
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                let _ = shared.responses_tx.send(ScriptResponse::TickMetrics {
                    elapsed_ms: elapsed,
                });
                if budget_ms > 0.0 && elapsed > budget_ms {
                    let _ = shared.responses_tx.send(ScriptResponse::OverBudget {
                        elapsed_ms: elapsed,
                        budget_ms,
                    });
                }
            }
            Ok(ScriptCommand::RestoreSession(_value)) => {
                let _ = shared.responses_tx.send(ScriptResponse::SessionUpdated);
            }
            Ok(ScriptCommand::Shutdown) | Err(_) => break,
        }
    }

    Ok(())
}

fn wait_for_registration(shared: &ScriptSharedState) {
    if shared.registered.load(Ordering::SeqCst) {
        return;
    }
    let mutex = Mutex::new(());
    let guard = mutex.lock().unwrap();
    let _ = shared
        .registration_cv
        .wait_timeout(guard, Duration::from_secs(1));
}

fn install_host(ctx: &Context, shared: Arc<ScriptSharedState>) -> Result<(), String> {
    let log_state = Arc::clone(&shared);
    ctx.add_callback(
        "host_log",
        move |args: Arguments| -> Result<JsValue, String> {
            let mut values = args.into_vec().into_iter();
            let level = values.next().unwrap_or(JsValue::Null);
            let message = values.next().unwrap_or(JsValue::Null);
            let level_str = js_value_to_string(level).unwrap_or_else(|| "info".to_string());
            let message_str = js_value_to_string(message).unwrap_or_default();
            let lvl = match level_str.to_lowercase().as_str() {
                "trace" => LogLevel::Trace,
                "debug" => LogLevel::Debug,
                "warn" => LogLevel::Warn,
                "error" => LogLevel::Error,
                _ => LogLevel::Info,
            };
            let _ = log_state.responses_tx.send(ScriptResponse::Log {
                level: lvl,
                message: message_str,
            });
            Ok(JsValue::Null)
        },
    )
    .map_err(|err| err.to_string())?;

    let cap_state = Arc::clone(&shared);
    ctx.add_callback(
        "host_capabilities",
        move |_: Arguments| -> Result<JsValue, String> {
            let list: Vec<JsValue> = cap_state
                .capabilities
                .iter()
                .cloned()
                .map(JsValue::from)
                .collect();
            Ok(JsValue::Array(list))
        },
    )
    .map_err(|err| err.to_string())?;

    let req_state = Arc::clone(&shared);
    ctx.add_callback(
        "host_request",
        move |args: Arguments| -> Result<JsValue, String> {
            let mut values = args.into_vec().into_iter();
            let op = values.next().unwrap_or(JsValue::Null);
            let payload = values.next().unwrap_or(JsValue::Null);
            let op_str = js_value_to_string(op).unwrap_or_default();
            let payload_json = js_value_to_json(payload);
            if let Err(err) = handle_host_request(&req_state, &op_str, &payload_json) {
                let _ = req_state.responses_tx.send(ScriptResponse::Error {
                    message: err.to_string(),
                });
            }
            Ok(JsValue::Null)
        },
    )
    .map_err(|err| err.to_string())?;

    let emit_state = Arc::clone(&shared);
    ctx.add_callback(
        "host_emit",
        move |args: Arguments| -> Result<JsValue, String> {
            let mut values = args.into_vec().into_iter();
            let event = values.next().unwrap_or(JsValue::Null);
            let payload = values.next().unwrap_or(JsValue::Null);
            let event_name = js_value_to_string(event).unwrap_or_default();
            let payload_json = js_value_to_json(payload);
            // Emitting alerts requires the alerts capability; other custom events are allowed.
            if event_name.starts_with("alerts") {
                emit_state
                    .ensure_capability("alerts.emit")
                    .map_err(|err| err.to_string())?;
            }
            let _ = emit_state.responses_tx.send(ScriptResponse::Event {
                event: event_name,
                payload: payload_json,
            });
            Ok(JsValue::Null)
        },
    )
    .map_err(|err| err.to_string())?;

    let register_state = Arc::clone(&shared);
    ctx.add_callback(
        "host_register",
        move |args: Arguments| -> Result<JsValue, String> {
            let descriptor = args.into_vec().into_iter().next().unwrap_or(JsValue::Null);
            register_descriptor(&register_state, descriptor)?;
            register_state.registered.store(true, Ordering::SeqCst);
            register_state.registration_cv.notify_all();
            Ok(JsValue::Null)
        },
    )
    .map_err(|err| err.to_string())?;

    let get_state = Arc::clone(&shared);
    ctx.add_callback(
        "host_session_get",
        move |args: Arguments| -> Result<JsValue, String> {
            get_state
                .ensure_capability("storage.session")
                .map_err(|err| err.to_string())?;
            let key = args.into_vec().into_iter().next().unwrap_or(JsValue::Null);
            let key_str = js_value_to_string(key).unwrap_or_default();
            let session = get_state.session.lock().unwrap();
            let value = session
                .as_object()
                .and_then(|map| map.get(&key_str))
                .cloned()
                .unwrap_or(JsonValue::Null);
            Ok(json_to_js_value(&value))
        },
    )
    .map_err(|err| err.to_string())?;

    let set_state = Arc::clone(&shared);
    ctx.add_callback(
        "host_session_set",
        move |args: Arguments| -> Result<JsValue, String> {
            let mut values = args.into_vec().into_iter();
            let key = values.next().unwrap_or(JsValue::Null);
            let value = values.next().unwrap_or(JsValue::Null);
            set_state
                .ensure_capability("storage.session")
                .map_err(|err| err.to_string())?;
            let key_str = js_value_to_string(key).unwrap_or_default();
            let mut session = set_state.session.lock().unwrap();
            let map = session.as_object_mut().unwrap();
            map.insert(key_str, js_value_to_json(value));
            Ok(JsValue::Null)
        },
    )
    .map_err(|err| err.to_string())?;

    let clear_state = Arc::clone(&shared);
    ctx.add_callback(
        "host_session_clear",
        move |_: Arguments| -> Result<JsValue, String> {
            clear_state
                .ensure_capability("storage.session")
                .map_err(|err| err.to_string())?;
            let mut session = clear_state.session.lock().unwrap();
            if let Some(map) = session.as_object_mut() {
                map.clear();
            }
            Ok(JsValue::Null)
        },
    )
    .map_err(|err| err.to_string())?;

    ctx.eval(
        r#"
        globalThis.host = {
            register: host_register,
            log: host_log,
            request: host_request,
            capabilities: host_capabilities,
            sessionGet: host_session_get,
            sessionSet: host_session_set,
            sessionClear: host_session_clear,
            emit: host_emit
        };
    "#,
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

enum CallbackKind {
    Event(String),
    Tick(f64),
}

fn invoke_callback(
    ctx: &Context,
    shared: &ScriptSharedState,
    kind: CallbackKind,
    payload: JsonValue,
) -> Result<(), String> {
    match kind {
        CallbackKind::Event(event) => {
            let name = {
                let guard = shared.on_event.lock().unwrap();
                guard.clone()
            };
            if let Some(func) = name {
                let args = vec![JsValue::from(event), json_to_js_value(&payload)];
                ctx.call_function(func.as_str(), args)
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            } else {
                Ok(())
            }
        }
        CallbackKind::Tick(delta) => {
            let name = {
                let guard = shared.on_tick.lock().unwrap();
                guard.clone()
            };
            if let Some(func) = name {
                let args = vec![JsValue::from(delta)];
                ctx.call_function(func.as_str(), args)
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            } else {
                Ok(())
            }
        }
    }
}

fn handle_host_request(
    shared: &ScriptSharedState,
    op: &str,
    payload: &JsonValue,
) -> Result<(), ScriptError> {
    match op {
        "telemetry.subscribe" => {
            shared.ensure_capability("telemetry.subscribe")?;
            if let Some(topic) = payload.get("topic").and_then(|v| v.as_str()) {
                let mut subs = shared.subscriptions.lock().unwrap();
                subs.insert(topic.to_string());
                let _ = shared.responses_tx.send(ScriptResponse::Subscriptions {
                    topics: subs.iter().cloned().collect(),
                });
            }
            Ok(())
        }
        "telemetry.unsubscribe" => {
            shared.ensure_capability("telemetry.subscribe")?;
            if let Some(topic) = payload.get("topic").and_then(|v| v.as_str()) {
                let mut subs = shared.subscriptions.lock().unwrap();
                subs.remove(topic);
                let _ = shared.responses_tx.send(ScriptResponse::Subscriptions {
                    topics: subs.iter().cloned().collect(),
                });
            }
            Ok(())
        }
        "storage.session.set" | "storage.session.clear" | "storage.session.get" => {
            shared.ensure_capability("storage.session")?;
            Ok(())
        }
        "alerts.emit" => {
            shared.ensure_capability("alerts.emit")?;
            let title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Alert")
                .to_string();
            let level = payload
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string();
            let message = payload
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let _ = shared.responses_tx.send(ScriptResponse::Alert {
                level,
                title,
                message,
            });
            Ok(())
        }
        "commands.issue" => {
            shared.ensure_capability("commands.issue")?;
            queue_host_request(shared, op, payload)
        }
        _ => queue_host_request(shared, op, payload),
    }
}

fn queue_host_request(
    shared: &ScriptSharedState,
    op: &str,
    payload: &JsonValue,
) -> Result<(), ScriptError> {
    let request_id = shared.request_counter.fetch_add(1, Ordering::SeqCst);
    let _ = shared.responses_tx.send(ScriptResponse::HostRequest {
        request_id,
        op: op.to_string(),
        payload: payload.clone(),
    });
    Ok(())
}

fn register_descriptor(shared: &ScriptSharedState, descriptor: JsValue) -> Result<(), String> {
    let object = match descriptor {
        JsValue::Object(map) => map,
        _ => return Err("host.register expects an object".into()),
    };
    if let Some(on_event) = object.get("onEvent").and_then(|v| v.as_str()) {
        let mut guard = shared.on_event.lock().unwrap();
        *guard = Some(on_event.to_string());
    }
    if let Some(on_tick) = object.get("onTick").and_then(|v| v.as_str()) {
        let mut guard = shared.on_tick.lock().unwrap();
        *guard = Some(on_tick.to_string());
    }
    if let Some(JsValue::Array(items)) = object.get("subscriptions") {
        shared
            .ensure_capability("telemetry.subscribe")
            .map_err(|err| err.to_string())?;
        let mut subs = shared.subscriptions.lock().unwrap();
        subs.extend(
            items
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string()),
        );
    }
    Ok(())
}

fn js_value_to_string(value: JsValue) -> Option<String> {
    match value {
        JsValue::String(s) => Some(s),
        JsValue::Int(i) => Some(i.to_string()),
        JsValue::Float(f) => Some(f.to_string()),
        JsValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn js_value_to_json(value: JsValue) -> JsonValue {
    match value {
        JsValue::Undefined | JsValue::Null => JsonValue::Null,
        JsValue::Bool(b) => JsonValue::Bool(b),
        JsValue::Int(i) => JsonValue::Number(JsonNumber::from(i)),
        JsValue::Float(f) => JsonNumber::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        JsValue::String(s) => JsonValue::String(s),
        JsValue::Array(arr) => JsonValue::Array(arr.into_iter().map(js_value_to_json).collect()),
        JsValue::Object(map) => {
            let mut json_map = JsonMap::new();
            for (k, v) in map {
                json_map.insert(k, js_value_to_json(v));
            }
            JsonValue::Object(json_map)
        }
        _ => JsonValue::Null,
    }
}

fn json_to_js_value(value: &JsonValue) -> JsValue {
    match value {
        JsonValue::Null => JsValue::Null,
        JsonValue::Bool(b) => JsValue::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                JsValue::Int(i as i32)
            } else if let Some(f) = n.as_f64() {
                JsValue::Float(f)
            } else {
                JsValue::Null
            }
        }
        JsonValue::String(s) => JsValue::String(s.clone()),
        JsonValue::Array(arr) => JsValue::Array(arr.iter().map(json_to_js_value).collect()),
        JsonValue::Object(map) => {
            let mut obj = HashMap::new();
            for (k, v) in map {
                obj.insert(k.clone(), json_to_js_value(v));
            }
            JsValue::Object(obj)
        }
    }
}

pub fn responses_to_json(responses: Vec<ScriptResponse>) -> JsonValue {
    let mut list = Vec::with_capacity(responses.len());
    for resp in responses {
        let value = match resp {
            ScriptResponse::Ready => json!({"type": "ready"}),
            ScriptResponse::Log { level, message } => {
                json!({"type": "log", "level": level.as_str(), "message": message})
            }
            ScriptResponse::Error { message } => json!({"type": "error", "message": message}),
            ScriptResponse::HostRequest {
                request_id,
                op,
                payload,
            } => json!({"type": "request", "request_id": request_id, "op": op, "payload": payload}),
            ScriptResponse::Alert {
                level,
                title,
                message,
            } => json!({"type": "alert", "level": level, "title": title, "message": message}),
            ScriptResponse::Event { event, payload } => {
                json!({"type": "event", "event": event, "payload": payload})
            }
            ScriptResponse::TickMetrics { elapsed_ms } => {
                json!({"type": "tick_metrics", "elapsed_ms": elapsed_ms})
            }
            ScriptResponse::OverBudget {
                elapsed_ms,
                budget_ms,
            } => json!({"type": "over_budget", "elapsed_ms": elapsed_ms, "budget_ms": budget_ms}),
            ScriptResponse::Subscriptions { topics } => {
                json!({"type": "subscriptions", "topics": topics})
            }
            ScriptResponse::SessionUpdated => json!({"type": "session_updated"}),
            ScriptResponse::Terminated => json!({"type": "terminated"}),
        };
        list.push(value);
    }
    JsonValue::Array(list)
}

pub fn manifest_to_json(manifest: &Manifest) -> JsonValue {
    json!({
        "id": manifest.id,
        "version": manifest.version,
        "entry": manifest.entry,
        "capabilities": manifest.capabilities,
        "subscriptions": manifest.subscriptions,
        "description": manifest.description,
        "author": manifest.author,
        "config": manifest.config,
    })
}

pub use ScriptManager as Manager;
