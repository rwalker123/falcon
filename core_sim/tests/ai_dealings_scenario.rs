//! ⛔ **AN AI PEOPLE ANSWERS A GIFT, OVER THE REAL SOCKETS** (`docs/plan_early_diplomacy.md` §1,
//! issue #233, slice 1d).
//!
//! The built `server` and the built `sim_ai` (utility brain, seat 1) play a small world while this
//! test plays the human seat 0 through a connection of its own. The human sends a scout east with a
//! food pack and the standing order **offer** (`send_expedition … strangers offer`): the scout walks
//! to the rival's camp, sights the stranger, and posts a pending offer addressed to the rival's
//! people. Nothing here answers it. The rival's `Dealings` specialist reads the offer off its own
//! frame, accepts it and names a return from its surplus — which the sender then takes: the accept
//! posts a **reply** to the human, who answers it — and the party, recalled once the exchange has
//! settled, walks the return home.
//!
//! **What it asserts, and through whom.** Everything on the encoded frames the human seat is sent:
//! the offer appears in the human's `dealings`, it is gone from them once answered, and the return
//! **lands at the sender's home band as an `ExchangeIn` crossing** on the home band's own row, with
//! the human people's live opinion of the rival (`own_goodwill`) lifted by what it received (credited
//! when the human takes the reply). The AI's
//! `decisions.jsonl` is read only to confirm the answer carried the worth numbers; it is not what the
//! scenario stands on.
//!
//! **One lever is moved for the run**: `dealings.food_runway_floor_turns` is `0`, so whether the
//! rival names food does not depend on how far its larder has fallen by the turn the offer lands.
//! The floor itself is pinned by the rule's unit tests.

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::ai_process::{jsonl, log_tail, sim_ai_binary, strip_ansi};
use common::seat_harness::{build_world, spawn_sim_ai, start_server, Link, RIVAL_SEAT};
use sim_runtime::{
    CommandPayload, FramePayload, OrdersDirective, StrangersKind, TradeCargoItem, WorldSnapshot,
    FOOD_CARGO_KEY,
};

/// The human's seat, and its claim ids (spaced by the claim retry count).
const HUMAN_SEAT: u32 = 0;
const HUMAN_CLAIM_ID: u64 = 100;
const RIVAL_CLAIM_ID: u64 = 200;
/// This scenario's port block (`ai_seat_scenario.rs` uses 45300/45500, `ai_record_import.rs` 45400).
const TEST_PORT_BASE: u16 = 45600;
/// The scout's party: the founding floor's size, legal for a 30-person starting band.
const PARTY_WORKERS: u32 = 4;
/// The pack the scout carries: well inside `PARTY_WORKERS × per_worker_carry`, and worth enough to
/// the rival's larder that the return it is sized to is not a rounding crumb.
const PACK_FOOD: f32 = 12.0;
/// The rival plays far more turns than the scenario needs; it is killed with the test.
const AI_TURNS: u64 = 200;
/// Turns the scenario gives each stage before it gives up on it: a walk of a dozen turns each way on
/// this map, with room for a chase. Counted in ticks, not frames — a turn is several frames (the
/// turn's own and a recapture after each command).
const MAX_TICKS_PER_STAGE: u64 = 40;
/// The `TransferCause` wire code of `ExchangeIn` (`components.rs`), and the direction `in`.
const CAUSE_EXCHANGE_IN: u8 = 10;
const DIRECTION_IN: u8 = 0;
/// How long the AI may take to claim its seat, and how often the log is polled for it.
const SEAT_CLAIM_TIMEOUT: Duration = Duration::from_secs(60);
const SEAT_CLAIM_POLL: Duration = Duration::from_millis(50);
const UTILITY_PROFILE: &str = "forager";
const UTILITY_DIFFICULTY: &str = "hard";
const ANSWER_INTENT_PREFIX: &str = "dealings:answer:";

/// The shipped profiles with the rival's food runway floor at `0` (module docs), written to the
/// scratch directory for `--profiles`.
fn write_profiles(dir: &std::path::Path) -> PathBuf {
    let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sim_ai/data/ai_profiles.json");
    let raw = fs::read_to_string(&shipped).expect("the shipped AI profiles read");
    let mut json: serde_json::Value = serde_json::from_str(&raw).expect("the profiles are JSON");
    json["profiles"][UTILITY_PROFILE]["dealings"]["food_runway_floor_turns"] =
        serde_json::json!(0.0);
    let path = dir.join("ai_profiles.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).expect("serialises"),
    )
    .expect("writes");
    path
}

/// How many times the server has logged a claim of the rival's seat.
fn rival_claims(log_path: &std::path::Path) -> usize {
    let log = fs::read_to_string(log_path).unwrap_or_default();
    strip_ansi(&log)
        .lines()
        .filter(|line| {
            line.contains("seat.claimed") && line.contains(&format!("faction={RIVAL_SEAT}"))
        })
        .count()
}

/// Fold a frame into the human's view of the world.
fn fold(snapshot: &mut WorldSnapshot, frame: FramePayload) {
    match frame {
        FramePayload::Snapshot(full) => *snapshot = full,
        FramePayload::Delta(delta) => snapshot
            .apply_delta(&delta)
            .unwrap_or_else(|err| panic!("a delta did not apply: {err:?}")),
    }
}

#[test]
fn an_ai_people_answers_a_scouts_offer_and_the_return_lands_at_the_senders_home() {
    let server = start_server("ai_dealings_scenario", TEST_PORT_BASE, None);
    let sim_ai = sim_ai_binary();
    build_world(&server);

    // Where the rival's camp is — read through its own seat, then released so the AI can claim it.
    let (rival_x, rival_y) = {
        let mut rival = Link::open(server.ports.command, &server.log_path);
        let claim = rival.claim_seat(RIVAL_CLAIM_ID, RIVAL_SEAT);
        assert!(
            claim.ok,
            "seat {RIVAL_SEAT} could not be claimed: {}",
            claim.error
        );
        let snapshot = rival.full_frame_for(server.ports.stream, claim.seat_token);
        let band = snapshot
            .populations
            .iter()
            .find(|cohort| cohort.faction == RIVAL_SEAT && !cohort.is_expedition)
            .expect("the world seats a rival band");
        (band.current_x, band.current_y)
    };

    // The human's seat, held for the whole run: its turn waits for this seat's `ready`.
    let mut human = Link::open(server.ports.command, &server.log_path);
    let claim = human.claim_seat(HUMAN_CLAIM_ID, HUMAN_SEAT);
    assert!(
        claim.ok,
        "seat {HUMAN_SEAT} could not be claimed: {}",
        claim.error
    );
    let mut view = human.full_frame_for(server.ports.stream, claim.seat_token);
    let home = view
        .populations
        .iter()
        .find(|cohort| cohort.faction == HUMAN_SEAT && !cohort.is_expedition)
        .expect("the human people has a band");
    let home_band = home.band_id;
    assert!(
        home.working_age >= PARTY_WORKERS,
        "a band that can field the scouts"
    );

    human.write(
        CommandPayload::SendExpedition {
            faction_id: HUMAN_SEAT,
            band_id: Some(home_band),
            party_workers: PARTY_WORKERS,
            target_x: rival_x,
            target_y: rival_y,
            kit_id: None,
            pack: vec![TradeCargoItem {
                id: FOOD_CARGO_KEY.to_owned(),
                is_material: false,
                amount: PACK_FOOD,
            }],
            strangers: StrangersKind::Offer,
        },
        "the scout's launch",
    );

    // The probe above already claimed and released the rival's seat once.
    let claims_before_the_ai = rival_claims(&server.log_path);
    let profiles = write_profiles(&server.scratch.dir);
    let profiles_arg = profiles.to_string_lossy().into_owned();
    let log_dir = server.scratch.dir.join("seat_logs");
    let ai = spawn_sim_ai(
        &sim_ai,
        &server,
        RIVAL_SEAT,
        &[
            "--brain",
            "utility",
            "--profile",
            UTILITY_PROFILE,
            "--difficulty",
            UTILITY_DIFFICULTY,
            "--profiles",
            &profiles_arg,
        ],
        AI_TURNS,
        Some(&log_dir),
    );

    // The rival must be seated before the human readies: a vacant seat never holds the turn, so the
    // world would otherwise resolve — and the offer would lapse unanswered — before the AI joined.
    let deadline = Instant::now() + SEAT_CLAIM_TIMEOUT;
    loop {
        if rival_claims(&server.log_path) > claims_before_the_ai {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the AI never claimed seat {RIVAL_SEAT}\n--- sim_ai log ---\n{}",
            log_tail(&ai.log_path)
        );
        std::thread::sleep(SEAT_CLAIM_POLL);
    }

    // Play the human's turns: ready on every new tick, and watch the frames for each stage.
    let mut readied: Option<u64> = None;
    let mut ready_if_new = |human: &mut Link, view: &WorldSnapshot| {
        let tick = view.header.tick;
        if readied.is_none_or(|last| tick > last) {
            human.write(
                CommandPayload::Orders {
                    faction_id: HUMAN_SEAT,
                    directive: OrdersDirective::Ready,
                },
                "the human's ready",
            );
            readied = Some(tick);
        }
    };
    ready_if_new(&mut human, &view);

    let failure_context = |what: &str| {
        format!(
            "{what}\n--- sim_ai log ---\n{}\n--- server log ---\n{}",
            log_tail(&ai.log_path),
            log_tail(&server.log_path)
        )
    };

    let stage_ends = |view: &WorldSnapshot| view.header.tick + MAX_TICKS_PER_STAGE;

    // Stage 1: the scout reaches the rival's camp and its offer appears in the human's dealings.
    let mut dealing_id = None;
    let deadline = stage_ends(&view);
    while view.header.tick < deadline {
        let frame = human.next_frame();
        fold(&mut view, frame);
        ready_if_new(&mut human, &view);
        if let Some(dealing) = view.dealings.iter().find(|dealing| {
            dealing.sender_faction == HUMAN_SEAT && dealing.receiver_faction == RIVAL_SEAT
        }) {
            dealing_id = Some(dealing.id);
            break;
        }
    }
    let dealing_id = dealing_id.unwrap_or_else(|| {
        panic!(
            "{}",
            failure_context("the scout never posted an offer to the rival")
        )
    });

    // Stage 2: the rival answers — the offer leaves the human's dealings and a REPLY addressed to the
    // human people takes its place: the sender asked for something back, so the human decides.
    let mut reply_id = None;
    let deadline = stage_ends(&view);
    while view.header.tick < deadline {
        let frame = human.next_frame();
        fold(&mut view, frame);
        ready_if_new(&mut human, &view);
        if let Some(reply) = view.dealings.iter().find(|dealing| {
            dealing.kind == "reply"
                && dealing.sender_faction == RIVAL_SEAT
                && dealing.receiver_faction == HUMAN_SEAT
        }) {
            reply_id = Some(reply.id);
            break;
        }
    }
    let reply_id = reply_id.unwrap_or_else(|| {
        panic!(
            "{}",
            failure_context("the rival never answered the offer with a reply")
        )
    });
    assert!(
        !view.dealings.iter().any(|dealing| dealing.id == dealing_id),
        "the answered offer left the book"
    );

    // The human takes the return the rival named; the exchange settles now.
    human.write(
        CommandPayload::AnswerDealing {
            faction: HUMAN_SEAT,
            dealing_id: reply_id,
            reply: sim_runtime::DealingReply::Accept {
                returned: Vec::new(),
            },
        },
        "the human takes the rival's return",
    );
    let mut taken = false;
    let deadline = stage_ends(&view);
    while view.header.tick < deadline {
        let frame = human.next_frame();
        fold(&mut view, frame);
        ready_if_new(&mut human, &view);
        if !view.dealings.iter().any(|dealing| dealing.id == reply_id) {
            taken = true;
            break;
        }
    }
    assert!(
        taken,
        "{}",
        failure_context("the reply never left the book after the human took it")
    );

    // The party carries the return; send it home.
    let party = view
        .populations
        .iter()
        .find(|cohort| cohort.faction == HUMAN_SEAT && cohort.is_expedition)
        .map(|cohort| cohort.band_id)
        .unwrap_or_else(|| {
            panic!(
                "{}",
                failure_context(
                    "the scout is not in the human's frame once the offer was answered"
                )
            )
        });
    human.write(
        CommandPayload::RecallExpedition {
            faction_id: HUMAN_SEAT,
            expedition_band_id: party,
        },
        "the scout's recall",
    );

    // Stage 3: the return lands at the sender's home band as an `ExchangeIn` crossing.
    let mut landed = 0.0_f32;
    let deadline = stage_ends(&view);
    while view.header.tick < deadline {
        let frame = human.next_frame();
        fold(&mut view, frame);
        ready_if_new(&mut human, &view);
        landed += view
            .populations
            .iter()
            .find(|cohort| cohort.band_id == home_band)
            .map(|cohort| {
                cohort
                    .transfer_crossings
                    .iter()
                    .filter(|crossing| {
                        crossing.cause == CAUSE_EXCHANGE_IN && crossing.direction == DIRECTION_IN
                    })
                    .map(|crossing| crossing.amount)
                    .sum::<f32>()
            })
            .unwrap_or_default();
        if landed > 0.0 {
            break;
        }
    }
    assert!(
        landed > 0.0,
        "{}",
        failure_context("the rival's return never landed at the sender's home as an ExchangeIn")
    );

    // The exchange settled: the human people thinks better of the rival for what it was sent.
    let opinion = view
        .own_goodwill
        .iter()
        .find(|row| row.other_faction == RIVAL_SEAT)
        .map_or(0.0, |row| row.exchanges);
    assert!(
        opinion > 0.0,
        "the human people's own opinion of the rival carries no exchange credit: {:?}",
        view.own_goodwill
    );

    // The AI's log of the answer: one accepted decision under the offer's intent, with its numbers.
    let decisions = jsonl(&log_dir.join("decisions.jsonl"));
    let answer = decisions
        .iter()
        .find(|record| {
            record["kind"] == "decision"
                && record["intent"]
                    .as_str()
                    .is_some_and(|intent| intent == format!("{ANSWER_INTENT_PREFIX}{dealing_id}"))
        })
        .unwrap_or_else(|| {
            panic!(
                "{}",
                failure_context("the AI logged no answer to the offer")
            )
        });
    assert_eq!(answer["outcome"], "accepted");
    assert_eq!(answer["specialist"], "dealings");
    let line = answer["commands_text"][0]
        .as_str()
        .expect("the answer's command line");
    assert!(
        line.starts_with("answer_dealing") && line.contains("accept"),
        "the answer is in the grammar: {line}"
    );
    assert!(
        answer["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("cargo worth")),
        "the answer's reason carries the worth numbers: {}",
        answer["reason"]
    );
}
