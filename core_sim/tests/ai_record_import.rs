//! ⛔ **A PLAYED GAME BECOMES A VIEWABLE RUN — through the record, not through the AI's own logs.**
//!
//! The server, started with `SIM_RECORD_DIR` set, writes every frame it publishes to a seat and
//! every command the timeline logs (`core_sim/src/record.rs`). `sim_ai import-record` turns that
//! into the seat log directory `sim_ai viewer` reads — which is how the **human's** seat, whose
//! process writes no instruments, gets the same page a rival does. Nothing in-process can prove
//! the chain end to end: the recorder sits behind the real publisher and the real dispatch loop,
//! and the importer replays the real FlatBuffers frames. So this drives the built `server`, seats a
//! scripted `sim_ai` **without** `--log-dir` — it is standing in for the human here — and reads the
//! seat back from the record alone.
//!
//! The harness is `common::seat_harness`, shared with `ai_seat_scenario.rs`.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::ai_process::{log_tail, sim_ai_binary};
use common::seat_harness::{
    build_world, run_scripted_sim_ai, start_server, MAP_SEED, ONE_RIVAL, RIVAL_SEAT, SCRIPT,
    SPLIT_WORKERS,
};

/// How many resolved turns the AI plays before releasing its seat.
const AI_TURNS: u64 = 3;
/// This test's port block; `ai_seat_scenario.rs` holds the one below.
const TEST_PORT_BASE: u16 = 45400;
/// The record layout, restated from `core_sim::record` (the writer) for the assertions.
const RECORD_DIR: &str = "record";
const RUN_FILE: &str = "run.json";
const COMMANDS_FILE: &str = "commands.jsonl";
const SEAT_DIR_PREFIX: &str = "seat_";
const FRAMES_DIR: &str = "frames";
/// The keys a command record carries — and nothing else, in particular no token.
const COMMAND_RECORD_KEYS: [&str; 5] = ["tick", "faction", "connection", "verb", "command"];
/// The importer's specialist and intent prefix (`sim_ai/src/import_record.rs`).
const HUMAN_SPECIALIST: &str = "human";
/// The scripted order's verb, as the record and the intent name it.
const SPLIT_VERB: &str = "split_band";
/// The orders verb (`sim_runtime::ORDERS_VERB`).
const ORDERS_VERB: &str = "order";
/// The viewer's page, and how its inlined model is found (`sim_ai/src/viewer/page.html`).
const VIEWER_PAGE: &str = "seat_1.html";
const MODEL_ELEMENT_OPEN: &str = "<script id=\"run-model\"";
const MODEL_ELEMENT_CLOSE: &str = "</script>";

/// The `RunModel` JSON inlined in the page.
fn inlined_model(page: &str) -> serde_json::Value {
    let start = page.find(MODEL_ELEMENT_OPEN).expect("the model element");
    let json_start = page[start..].find('>').expect("the tag closes") + start + 1;
    let json_end = page[json_start..]
        .find(MODEL_ELEMENT_CLOSE)
        .expect("the element closes")
        + json_start;
    serde_json::from_str(&page[json_start..json_end]).expect("the inlined model is JSON")
}

/// Run a `sim_ai` subcommand, failing with its output.
fn sim_ai_command(sim_ai: &Path, args: &[&str]) {
    let output = Command::new(sim_ai)
        .args(args)
        .output()
        .expect("the built sim_ai runs");
    assert!(
        output.status.success(),
        "sim_ai {} exited {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        args[0],
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// ⛔ **The record of a played seat imports into a page with one turn per tick and one `human`
/// decision per command the seat sent.**
#[test]
fn a_recorded_seat_imports_into_the_viewer_page() {
    let sim_ai = sim_ai_binary();
    // The launcher's layout: `<run>/record/` for the server, `<run>/seat_<f>/` per seat.
    let run_dir =
        std::env::temp_dir().join(format!("shadow_scale_ai_record_run_{}", std::process::id()));
    let _ = fs::remove_dir_all(&run_dir);
    let record_dir = run_dir.join(RECORD_DIR);
    let server = start_server("ai_record_import", TEST_PORT_BASE, Some(&record_dir));
    build_world(&server);

    // The seat plays unmeasured — no `--log-dir` — so everything the page shows comes from the
    // record.
    let ai = run_scripted_sim_ai(&sim_ai, &server, RIVAL_SEAT, SCRIPT, AI_TURNS, None);
    assert!(ai.status.success());

    // The record is self-describing.
    let run_info: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(record_dir.join(RUN_FILE)).expect("run.json was written"),
    )
    .expect("run.json is JSON");
    assert_eq!(run_info["map_seed"], MAP_SEED);
    assert_eq!(
        run_info["roster"].as_array().map(Vec::len),
        Some((ONE_RIVAL + 1) as usize),
        "the human and one rival"
    );
    let frames_dir = record_dir
        .join(format!("{SEAT_DIR_PREFIX}{RIVAL_SEAT}"))
        .join(FRAMES_DIR);
    let frame_count = fs::read_dir(&frames_dir)
        .expect("the seat's frames")
        .count();
    assert!(
        frame_count > AI_TURNS as usize,
        "a full frame and one delta per turn at least, got {frame_count}"
    );

    // Every command line carries exactly the record's keys — a token is not one of them — and the
    // seat's lines are the script's order and its readies.
    let commands: Vec<serde_json::Value> = fs::read_to_string(record_dir.join(COMMANDS_FILE))
        .expect("commands.jsonl was written")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record per line"))
        .collect();
    for record in &commands {
        let mut keys: Vec<&str> = record
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut expected = COMMAND_RECORD_KEYS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            keys, expected,
            "a command record carries exactly these keys"
        );
    }
    let seat_commands: Vec<&serde_json::Value> = commands
        .iter()
        .filter(|record| record["faction"] == RIVAL_SEAT)
        .collect();
    let splits = seat_commands
        .iter()
        .filter(|record| record["verb"] == SPLIT_VERB)
        .count();
    let readies = seat_commands
        .iter()
        .filter(|record| record["verb"] == ORDERS_VERB)
        .count();
    assert_eq!(
        splits,
        1,
        "the script fires once\n{}",
        log_tail(&ai.log_path)
    );
    assert_eq!(
        readies,
        AI_TURNS as usize,
        "one ready per acted turn\n{}",
        log_tail(&ai.log_path)
    );

    // Import the seat, then view the run directory as the launcher lays it out.
    let seat_dir = run_dir.join(format!("{SEAT_DIR_PREFIX}{RIVAL_SEAT}"));
    sim_ai_command(
        &sim_ai,
        &[
            "import-record",
            record_dir.to_str().unwrap(),
            "--seat",
            &RIVAL_SEAT.to_string(),
            "--out",
            seat_dir.to_str().unwrap(),
        ],
    );
    let page_path = run_dir.join(VIEWER_PAGE);
    sim_ai_command(
        &sim_ai,
        &[
            "viewer",
            run_dir.to_str().unwrap(),
            "--seat",
            &RIVAL_SEAT.to_string(),
            "--out",
            page_path.to_str().unwrap(),
        ],
    );
    let page = fs::read_to_string(&page_path).expect("the page was written");
    let model = inlined_model(&page);
    assert_eq!(model["faction"], RIVAL_SEAT);
    assert_eq!(
        model["specialists"],
        serde_json::json!([HUMAN_SPECIALIST]),
        "the only specialist a recorded seat has"
    );
    let turns = model["turns"].as_array().expect("the model carries turns");
    // The seat saw the tick it claimed at and one more per turn it played; it exits on seeing
    // the last, which the record still holds.
    assert_eq!(
        turns.len(),
        (AI_TURNS + 1) as usize,
        "one turn per tick the seat was sent"
    );
    for turn in turns {
        assert!(turn["score"].is_object(), "a score row per tick");
        assert!(turn["observation"].is_object(), "an observation per tick");
        assert!(
            turn["observation"]["plan"].is_null(),
            "no brain lens on an imported seat"
        );
    }
    let first = &turns[0];
    let decisions = first["decisions"].as_array().expect("decisions");
    assert_eq!(
        decisions.len(),
        1,
        "the script's one order, on the first tick"
    );
    let decision = &decisions[0];
    assert_eq!(decision["specialist"], HUMAN_SPECIALIST);
    assert_eq!(
        decision["intent"],
        format!("{HUMAN_SPECIALIST}:{SPLIT_VERB}")
    );
    assert_eq!(decision["outcome"], "accepted");
    let line = decision["commands_text"][0]
        .as_str()
        .expect("the recorded line");
    assert!(
        line.starts_with(&format!("{SPLIT_VERB} {RIVAL_SEAT} "))
            && line.ends_with(&format!(" {SPLIT_WORKERS}")),
        "the line is the grammar's: {line}"
    );
    let ready_ticks = turns.iter().filter(|turn| turn["ready"] == true).count();
    assert_eq!(
        ready_ticks, AI_TURNS as usize,
        "a ready record per acted tick"
    );
    assert!(
        turns.last().unwrap()["ready"] == false,
        "the tick the seat left on was never acted on"
    );
    let _ = fs::remove_dir_all(&run_dir);
}
