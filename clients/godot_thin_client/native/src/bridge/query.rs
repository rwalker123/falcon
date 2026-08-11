//! **The command socket's SECOND direction** — the forecast query round trip.
//!
//! Every other payload the client puts on this socket is an *order*: it is written, the world
//! changes, and the client learns what happened from the next snapshot. A `QueryCommand` is the one
//! payload the server *answers*, writing a `QueryReplyEnvelope` back on the same TCP stream
//! (`sim_runtime/proto/command.proto` → "THE QUERY CHANNEL").
//!
//! ## Why a query gets its own worker, and its own connection
//!
//! `transmit_proto_command` is fire-and-forget: connect, write one frame, drop the socket. A query
//! must **hold the connection open** until the answer arrives, because that is where the answer is
//! written. So it cannot ride the ordinary command path, and it must not ride the ordinary command
//! *thread* either — a query that waits on the sim would put every queued order behind it.
//!
//! ## **A QUERY TRIGGERS NO SNAPSHOT, so nothing else will arrive to render off**
//!
//! The server deliberately skips its re-capture for a query (it changed nothing, and that re-capture
//! is the expensive half of a turn). The reply IS the render input. A client that treated a query as
//! a command and waited for a frame would wait forever, which is why the answer comes back through
//! [`poll_replies`] rather than being inferred from the stream.
//!
//! ## Threading
//!
//! Godot forbids touching the scene tree off the main thread, so this deliberately does **not** emit
//! a signal from the worker. The worker pushes finished answers onto an `mpsc` queue and the main
//! thread DRAINS it once a frame through `CommandBridge.poll_query_replies`; the GDScript seam
//! (`ForecastQuery.gd`) turns each drained answer into the signal its consumers subscribe to. One
//! hop, on the thread that is allowed to make it.

use godot::prelude::*;
use sim_runtime::{
    CommandEncodeError, CommandEnvelope, CommandPayload, DenialRaidForecastQuery,
    HuntTripForecastQuery, QueryPayload, QueryReply, QueryReplyEnvelope, MAX_PROTO_FRAME,
};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

// The framing bound is `sim_runtime::MAX_PROTO_FRAME`, imported above and never restated. Both
// directions of this socket are framed as a 4-byte little-endian length followed by the encoded
// protobuf, and both ends refuse an oversized frame rather than trying to read it — so the two
// numbers must agree or one side drops a connection the other believes is healthy. It lives in
// `sim_runtime` because that is the crate that owns the protobuf contract, which both ends already
// depend on; a restated copy here could only ever drift from it.

/// **How long the worker waits for one answer before giving up on that connection.**
///
/// The round trip is local and the sim answers between turns, so a healthy reply lands in well under
/// a frame. This is not a latency budget — it is the bound on a socket that will never answer (the
/// server died mid-question, or a build that predates the query channel accepted the frame and said
/// nothing), so it is generous enough that a busy turn cannot trip it and short enough that the
/// worker is not wedged for a whole play session.
const QUERY_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// The two questions, spelled as the GDScript seam spells them. They are matched on rather than
/// compared to literals at the call site so a typo cannot become a silently unsent query.
pub(crate) const QUERY_KIND_HUNT_TRIP: &str = "hunt_trip_forecast";
pub(crate) const QUERY_KIND_DENIAL_RAID: &str = "denial_raid_forecast";

/// **The transport's OWN failure token**, and it is deliberately in the same vocabulary as the
/// server's `query_error` tokens rather than a free-text string: the seam renders one failure line
/// whatever went wrong, and a caller must not have to tell a refusal apart from a dead socket by
/// pattern-matching prose. Nothing else in the client may raise it.
pub(crate) const QUERY_ERROR_TRANSPORT: &str = "transport";

struct QueryRequest {
    host: String,
    port: u16,
    request_id: u64,
    query: QueryPayload,
}

/// One finished round trip, as the main thread will read it.
struct QueryAnswer {
    request_id: u64,
    reply: Result<QueryReply, String>,
}

static QUERY_SENDER: OnceLock<Sender<QueryRequest>> = OnceLock::new();
static QUERY_ANSWERS: OnceLock<Mutex<Receiver<QueryAnswer>>> = OnceLock::new();

/// Encode a query from the Dictionary the GDScript seam composes, and hand it to the worker.
///
/// Returns immediately: the answer arrives later through [`poll_replies`]. A malformed ask (an
/// unknown `kind`) is refused HERE rather than being turned into a plausible default query, because
/// a query answered for something other than what the sheet composed is worse than no answer.
pub(crate) fn dispatch(
    host: &str,
    port: u16,
    request_id: u64,
    ask: &VarDictionary,
) -> Result<(), String> {
    let kind = ask
        .get("kind")
        .map(|value| value.to::<GString>().to_string())
        .unwrap_or_default();
    let query = match kind.as_str() {
        QUERY_KIND_HUNT_TRIP => QueryPayload::HuntTripForecast(HuntTripForecastQuery {
            faction_id: dict_u32(ask, "faction_id"),
            band_id: dict_u64(ask, "band_id"),
            herd_id: dict_string(ask, "herd_id"),
            kit_id: dict_string(ask, "kit_id"),
            party_workers: dict_u32(ask, "party_workers"),
            floor: dict_f32(ask, "floor"),
            preset_floors: dict_f32_array(ask, "preset_floors"),
            max_party_workers: dict_u32(ask, "max_party_workers"),
        }),
        QUERY_KIND_DENIAL_RAID => QueryPayload::DenialRaidForecast(DenialRaidForecastQuery {
            faction_id: dict_u32(ask, "faction_id"),
            band_id: dict_u64(ask, "band_id"),
            herd_id: dict_string(ask, "herd_id"),
            kit_id: dict_string(ask, "kit_id"),
            party_workers: dict_u32(ask, "party_workers"),
            max_party_workers: dict_u32(ask, "max_party_workers"),
        }),
        other => return Err(format!("unknown query kind {other:?}")),
    };
    query_sender()
        .send(QueryRequest {
            host: host.to_string(),
            port,
            request_id,
            query,
        })
        .map_err(|err| format!("query dispatch error: {err}"))
}

/// Drain every answer that has landed since the last call, as Dictionaries the seam can read.
///
/// **Non-blocking by construction.** It is called once a frame from `_process`; an empty array is
/// the ordinary answer and costs one `try_recv`.
pub(crate) fn poll_replies() -> VarArray {
    let mut out = VarArray::new();
    let receiver = query_answers().lock();
    let Ok(receiver) = receiver else {
        return out;
    };
    // A disconnected channel ends the drain exactly as an empty one does: the worker owns the
    // sender for the life of the process, so `Disconnected` means the worker died and no further
    // answer is coming — there is nothing different to do about it here.
    while let Ok(answer) = receiver.try_recv() {
        out.push(&answer_to_dict(&answer).to_variant());
    }
    out
}

fn query_worker(requests: Receiver<QueryRequest>, answers: Sender<QueryAnswer>) {
    for request in requests {
        let reply = round_trip(&request);
        let _ = answers.send(QueryAnswer {
            request_id: request.request_id,
            reply,
        });
    }
}

/// Write the question and read the one answer to it, on ONE connection.
///
/// The reply is correlated by `request_id` at the seam, not here: the server may answer questions
/// from other connections in between, but never on *this* socket, which carries exactly the replies
/// to what was asked on it.
fn round_trip(request: &QueryRequest) -> Result<QueryReply, String> {
    let envelope = CommandEnvelope {
        payload: CommandPayload::Query {
            request_id: request.request_id,
            query: request.query.clone(),
        },
        correlation_id: None,
    };
    let bytes = envelope
        .encode_to_vec()
        .map_err(|CommandEncodeError::Encode(err)| format!("encode error: {err}"))?;
    if bytes.len() > MAX_PROTO_FRAME {
        return Err(format!(
            "query frame {} exceeds the {MAX_PROTO_FRAME}-byte bound",
            bytes.len()
        ));
    }
    let addr = format!("{}:{}", request.host, request.port);
    let mut stream = TcpStream::connect(&addr).map_err(|err| format!("connect error: {err}"))?;
    let _ = stream.set_nodelay(true);
    // The bound applies to WAITING, not to the whole exchange: a read that returns some bytes
    // restarts it, which is what makes a long turn safe and a dead socket bounded.
    stream
        .set_read_timeout(Some(QUERY_REPLY_TIMEOUT))
        .map_err(|err| format!("timeout error: {err}"))?;
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|err| format!("length write error: {err}"))?;
    stream
        .write_all(&bytes)
        .map_err(|err| format!("payload write error: {err}"))?;
    stream
        .flush()
        .map_err(|err| format!("flush error: {err}"))?;

    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .map_err(|err| format!("reply length read error: {err}"))?;
    let frame_len = u32::from_le_bytes(len_buf) as usize;
    // Refused, not read: the server bounds its own writes by the same number, so a frame past it is
    // a desynchronised stream rather than a big answer, and reading it would consume the next one.
    if frame_len > MAX_PROTO_FRAME {
        return Err(format!(
            "reply frame {frame_len} exceeds the {MAX_PROTO_FRAME}-byte bound"
        ));
    }
    let mut payload = vec![0u8; frame_len];
    stream
        .read_exact(&mut payload)
        .map_err(|err| format!("reply read error: {err}"))?;
    let reply =
        QueryReplyEnvelope::decode(&payload).map_err(|err| format!("reply decode error: {err}"))?;
    // The server echoes the id it was given. A mismatch means this connection was answered something
    // it did not ask, which is a desynchronised stream and not a recoverable answer.
    if reply.request_id != request.request_id {
        return Err(format!(
            "reply correlates to request {}, not {}",
            reply.request_id, request.request_id
        ));
    }
    Ok(reply.reply)
}

/// One answer, in the shape the seam reads: `{request_id, ok, error, …payload}`.
///
/// **A server REFUSAL and a transport failure land in the same `error` field, carrying tokens from
/// the same vocabulary** (`sim_runtime::query_error`, plus [`QUERY_ERROR_TRANSPORT`]). The seam
/// renders one honest failure line either way; splitting them would only invite per-token prose for
/// states the sheet is supposed to make unreachable.
fn answer_to_dict(answer: &QueryAnswer) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("request_id", answer.request_id as i64);
    match &answer.reply {
        Ok(QueryReply::HuntTripForecast(reply)) => {
            let _ = dict.insert("ok", true);
            let _ = dict.insert("kind", QUERY_KIND_HUNT_TRIP);
            let _ = dict.insert("at_composed", &hunt_row_to_dict(&reply.at_composed));
            let mut presets = VarArray::new();
            for row in &reply.per_preset {
                presets.push(&hunt_row_to_dict(row).to_variant());
            }
            let _ = dict.insert("per_preset", &presets);
            let _ = dict.insert("useful_cap", i64::from(reply.useful_cap));
        }
        Ok(QueryReply::DenialRaidForecast(reply)) => {
            let _ = dict.insert("ok", true);
            let _ = dict.insert("kind", QUERY_KIND_DENIAL_RAID);
            let _ = dict.insert("at_composed", &denial_row_to_dict(&reply.at_composed));
            let _ = dict.insert("party_needed", i64::from(reply.party_needed));
        }
        Ok(QueryReply::Error(reason)) => {
            let _ = dict.insert("ok", false);
            let _ = dict.insert("error", reason.as_str());
        }
        Err(detail) => {
            let _ = dict.insert("ok", false);
            let _ = dict.insert("error", QUERY_ERROR_TRANSPORT);
            // The token is what the UI renders; this is for the log, and it is the only place the
            // socket's own words survive.
            let _ = dict.insert("detail", detail.as_str());
        }
    }
    dict
}

/// **THE ROW KEYS ARE THE SNAPSHOT TABLE'S, DELIBERATELY.** `SourceForecast` shapes a raid readout
/// out of exactly these names, and it did so when they arrived on `HerdTelemetryState`; the query
/// moved *where the row comes from*, not what a row is. Renaming here would have rewritten every
/// reader for no gain and would have made the two eras impossible to diff.
fn hunt_row_to_dict(row: &sim_runtime::HuntTripRow) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("floor", f64::from(row.floor));
    let _ = dict.insert("party_workers", i64::from(row.party_workers));
    let _ = dict.insert("turns_to_fill", i64::from(row.turns_to_fill));
    let _ = dict.insert("bound", row.bound.as_str());
    let _ = dict.insert("delivers_food", row.delivers_food);
    let _ = dict.insert("animals_taken", i64::from(row.animals_taken));
    let _ = dict.insert("delivered_food", f64::from(row.delivered_food));
    let _ = dict.insert("wasted_food", f64::from(row.wasted_food));
    // WHAT THE TRIP LANDS, PER MATERIAL (arc #527) — and on an INEDIBLE quarry (a wolf) the ENTIRE
    // payload, since `delivered_food` is 0 there and nothing else on this row can say what comes
    // home. An ARRAY of `{ material_id, amount }` dicts, projected off the same carried biomass
    // `delivered_food` is, so the two readouts of one trip cannot disagree.
    //
    // **AN EMPTY ARRAY IS "NO ROW", NEVER "ZERO"** — most quarries are made of nothing anyone builds
    // with. The key is always inserted so a reader can tell "no projection sent" from "this raid
    // brings home no material". **DO NOT SUM** the rows into one figure.
    let mut materials = VarArray::new();
    for payoff in &row.delivered_material {
        let mut entry = VarDictionary::new();
        let _ = entry.insert("material_id", payoff.material_id.as_str());
        let _ = entry.insert("amount", f64::from(payoff.amount));
        materials.push(&entry.to_variant());
    }
    let _ = dict.insert("delivered_material", &materials);
    dict
}

fn denial_row_to_dict(row: &sim_runtime::DenialRow) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let _ = dict.insert("party_workers", i64::from(row.party_workers));
    let _ = dict.insert("turns_to_collapse", i64::from(row.turns_to_collapse));
    let _ = dict.insert(
        "turns_to_collapse_low",
        i64::from(row.turns_to_collapse_low),
    );
    let _ = dict.insert(
        "turns_to_collapse_high",
        i64::from(row.turns_to_collapse_high),
    );
    let _ = dict.insert("outcome", row.outcome.as_str());
    let _ = dict.insert("animals_killed", i64::from(row.animals_killed));
    let _ = dict.insert("delivered_food", f64::from(row.delivered_food));
    let _ = dict.insert("wasted_food", f64::from(row.wasted_food));
    // WHAT THE RAID LANDS, PER MATERIAL (arc #527) — the same haul `delivered_food` converts, and on
    // an inedible quarry the whole of it. An ARRAY of `{ material_id, amount }` dicts; EMPTY means
    // "no row", never "zero". Its WASTE twin does not exist by decision — the waste is already
    // legible as a percentage — so do not synthesize one.
    let mut materials = VarArray::new();
    for payoff in &row.delivered_material {
        let mut entry = VarDictionary::new();
        let _ = entry.insert("material_id", payoff.material_id.as_str());
        let _ = entry.insert("amount", f64::from(payoff.amount));
        materials.push(&entry.to_variant());
    }
    let _ = dict.insert("delivered_material", &materials);
    dict
}

fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| value.to::<GString>().to_string())
        .unwrap_or_default()
}

fn dict_u32(dict: &VarDictionary, key: &str) -> u32 {
    dict.get(key)
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or_default()
        .clamp(0, i64::from(u32::MAX)) as u32
}

fn dict_u64(dict: &VarDictionary, key: &str) -> u64 {
    // A `BandId` is a u64 on the wire but arrives through GDScript's signed 64-bit int, so the top
    // bit round-trips as a negative. Reinterpret rather than clamp: clamping would silently address
    // a different band.
    dict.get(key)
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or_default() as u64
}

fn dict_f32(dict: &VarDictionary, key: &str) -> f32 {
    dict.get(key)
        .and_then(|value| value.try_to::<f64>().ok())
        .unwrap_or_default() as f32
}

fn dict_f32_array(dict: &VarDictionary, key: &str) -> Vec<f32> {
    let Some(value) = dict.get(key) else {
        return Vec::new();
    };
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .filter_map(|entry| entry.try_to::<f64>().ok())
        .map(|entry| entry as f32)
        .collect()
}

fn query_sender() -> Sender<QueryRequest> {
    QUERY_SENDER
        .get_or_init(|| {
            let (requests_tx, requests_rx) = mpsc::channel::<QueryRequest>();
            let (answers_tx, answers_rx) = mpsc::channel::<QueryAnswer>();
            let _ = QUERY_ANSWERS.set(Mutex::new(answers_rx));
            thread::Builder::new()
                .name("forecast-query-worker".into())
                .spawn(move || query_worker(requests_rx, answers_tx))
                .expect("failed to spawn forecast query worker thread");
            requests_tx
        })
        .clone()
}

fn query_answers() -> &'static Mutex<Receiver<QueryAnswer>> {
    // Standing the sender up is what creates the answer channel, so ask for it first — otherwise a
    // poll before the first query would find nothing to lock and silently drop every later answer.
    let _ = query_sender();
    QUERY_ANSWERS
        .get()
        .expect("query answer channel is created with the sender")
}
