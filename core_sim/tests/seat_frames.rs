//! ⛔ **ONE FRAME PER SEAT — its own view, its own delta chain, its own event cursor.**
//!
//! PR #648 made a published frame *one viewer's view*, section by section. With one frame still
//! going to every client, that turned a limitation into a **wrong view**: a second connected client
//! saw its own people as a foreign band — redacted to identity and position, absent from its own
//! roster, demographics reading zero. This file pins the repair, and it pins it at the three places
//! it can silently come apart:
//!
//! 1. **the content** — each seat's frame carries its own faction and nobody else's;
//! 2. **the chain** — each seat numbers its own publications, so two seats do not draw from one
//!    counter and see gaps in their own stream (`docs/plan_delta_streaming.md` §3.1);
//! 3. **the cursor** — the event feed's `diff_appended` cursor is *"the highest seq this client has
//!    been sent"*, so a shared one would hand seat B only the events seat A had not already taken
//!    (`.claude/rules/core_sim/event-feed.md`).
//!
//! **Every assertion reads the ENCODED frame**, and the delta assertions read the frames the
//! **sink** was handed — the bytes a client would actually receive, tagged with the seat they were
//! addressed to. An in-process assertion passes on a field that never reached the codec, and a
//! per-seat assertion made off `SnapshotHistory`'s seat-blind accessors would be asserting about the
//! primary audience twice.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    recapture_snapshot_in_place, run_turn, CommandEventEntry, CommandEventKind, CommandEventLog,
    FactionId, FactionInventory, FrameSink, SnapshotAudiences, SnapshotHistory,
};
use faction_support::{world_with, HOME, ONE_RIVAL, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The stockpiled good each people is given, and how much — distinct amounts, so a row that reached
/// the wrong seat's frame is unmistakable in the failure message rather than a plausible number.
const STOCK_ITEM: &str = "provisions";
const HOME_STOCK: i64 = 111;
const RIVAL_STOCK: i64 = 222;

/// Turns the convergence arm runs. Long enough that a shared counter or a shared cursor would have
/// drifted several times over; short enough to keep the suite quick.
const CONVERGENCE_TURNS: usize = 12;

/// The world's first publication always carries this sequence number, per seat.
const FIRST_PUBLICATION_SEQ: u64 = 1;

/// A frame carrying a full snapshot names no base.
const NO_BASE_FRAME_SEQ: u64 = 0;

/// A feed window wider than this test's whole run, so no row can age out of the log while the
/// cursor is under examination.
const RETAIN_EVERYTHING_TURNS: u64 = 1_000;

// =================================================================================================
// The fixture
// =================================================================================================

/// **A two-faction world with both seats occupied**, each people holding something the other must
/// not see.
///
/// Both halves matter: state on the viewer alone would let *"the section is empty"* pass as
/// filtered, and state on the rival alone would let *"the section is the viewer's"* pass as
/// unfiltered-but-lucky.
fn a_world_with_two_seats() -> App {
    let mut app = world_with(ONE_RIVAL, |_| {});
    for (faction, stock) in [(HOME, HOME_STOCK), (RIVAL, RIVAL_STOCK)] {
        app.world.resource_mut::<FactionInventory>().add_stockpile(
            faction,
            STOCK_ITEM.to_string(),
            stock,
        );
    }
    app.world
        .resource_mut::<SnapshotAudiences>()
        .set(vec![HOME, RIVAL]);
    run_turn(&mut app);
    app
}

/// Every frame the publisher handed the socket, in publication order, with the seat it was addressed
/// to — the test's stand-in for `network::SnapshotServer`, which is the server binary's concern.
#[derive(Default)]
struct Recorder {
    frames: Mutex<Vec<(FactionId, Arc<Vec<u8>>)>>,
}

impl Recorder {
    /// The frames addressed to one seat, in order.
    fn for_seat(&self, seat: FactionId) -> Vec<Arc<Vec<u8>>> {
        self.frames
            .lock()
            .expect("the recorder's frames")
            .iter()
            .filter(|(addressed, _)| *addressed == seat)
            .map(|(_, frame)| Arc::clone(frame))
            .collect()
    }
}

impl FrameSink for Recorder {
    fn publish_frame(&self, seat: FactionId, frame: &Arc<Vec<u8>>) {
        self.frames
            .lock()
            .expect("the recorder's frames")
            .push((seat, Arc::clone(frame)));
    }
}

/// **Block until the publisher has finished every queued frame.** Every accessor on
/// `SnapshotHistory` drains the queue first, so asking for anything is the barrier — which is why
/// this reads something rather than sleeping.
fn drain_publisher(app: &App) {
    let _ = app.world.resource::<SnapshotHistory>().audiences();
}

// =================================================================================================
// Decoding — what one frame says about who it is for
// =================================================================================================

/// What a decoded frame carries, reduced to *whose* rows are in it.
#[derive(Debug, Default, PartialEq, Eq)]
struct FrameView {
    /// The faction of every published band row.
    band_factions: Vec<u32>,
    /// The bands whose row carries live internals — a size, a store, an activity. A redacted foreign
    /// row carries identity and position and nothing else.
    bands_in_full: Vec<u32>,
    /// The factions with a stockpile row, and what it said.
    stockpiles: Vec<(u32, i64)>,
    /// The factions with a demographics row.
    demographics: Vec<u32>,
    /// The factions of the command-event rows on this frame, with their sequence numbers.
    events: Vec<(u32, u64)>,
}

fn decode_frame(bytes: &[u8]) -> FrameView {
    let envelope = fb::root_as_envelope(bytes).expect("the frame is a valid envelope");
    let payload = envelope
        .payload_as_snapshot()
        .expect("the frame carries a snapshot");
    let mut view = FrameView::default();
    if let Some(rows) = payload
        .population()
        .and_then(|section| section.populations())
    {
        for row in rows.iter() {
            view.band_factions.push(row.faction());
            // The redaction is an allow-list: a foreign row keeps identity and position and zeroes
            // everything else, so a non-zero size is exactly *"this row was not redacted"*.
            if row.size() > 0 {
                view.bands_in_full.push(row.faction());
            }
        }
    }
    if let Some(rows) = payload
        .population()
        .and_then(|section| section.demographics())
    {
        view.demographics.extend(rows.iter().map(|r| r.faction()));
    }
    if let Some(rows) = payload
        .economy()
        .and_then(|section| section.factionInventory())
    {
        for row in rows.iter() {
            let quantity = row
                .inventory()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|entry| entry.item().unwrap_or_default() == STOCK_ITEM)
                        .map(|entry| entry.quantity())
                })
                .unwrap_or_default();
            view.stockpiles.push((row.faction(), quantity));
        }
    }
    if let Some(rows) = payload
        .campaign()
        .and_then(|section| section.commandEvents())
    {
        view.events
            .extend(rows.iter().map(|r| (r.faction(), r.seq())));
    }
    view
}

/// One seat's own latest frame, encoded and decoded — the bytes that seat's client holds.
fn frame_for(app: &App, seat: FactionId) -> FrameView {
    let entry = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry_for(seat)
        .unwrap_or_else(|| panic!("seat {seat} was published a frame"));
    let bytes = sim_schema::encode_snapshot_flatbuffer(entry.snapshot.as_ref());
    decode_frame(&bytes)
}

/// `(frame_seq, base_frame_seq, is_full)` off an encoded frame of either kind.
fn chain_of(bytes: &[u8]) -> (u64, u64, bool) {
    let envelope = fb::root_as_envelope(bytes).expect("the frame is a valid envelope");
    match envelope.payload_type() {
        fb::SnapshotPayload::snapshot => {
            let header = envelope
                .payload_as_snapshot()
                .and_then(|p| p.header())
                .expect("a snapshot carries a header");
            (header.frameSeq(), header.baseFrameSeq(), true)
        }
        fb::SnapshotPayload::delta => {
            let header = envelope
                .payload_as_delta()
                .and_then(|p| p.header())
                .expect("a delta carries a header");
            (header.frameSeq(), header.baseFrameSeq(), false)
        }
        other => panic!("a published frame is a snapshot or a delta, not {other:?}"),
    }
}

/// The command-event rows of an encoded frame of either kind, as `(faction, seq)`.
fn events_on(bytes: &[u8]) -> Vec<(u32, u64)> {
    let envelope = fb::root_as_envelope(bytes).expect("the frame is a valid envelope");
    let rows = match envelope.payload_type() {
        fb::SnapshotPayload::snapshot => envelope
            .payload_as_snapshot()
            .and_then(|p| p.campaign())
            .and_then(|section| section.commandEvents()),
        fb::SnapshotPayload::delta => envelope
            .payload_as_delta()
            .and_then(|p| p.campaign())
            .and_then(|section| section.commandEvents()),
        other => panic!("a published frame is a snapshot or a delta, not {other:?}"),
    };
    rows.map(|rows| rows.iter().map(|r| (r.faction(), r.seq())).collect())
        .unwrap_or_default()
}

// =================================================================================================
// 1. The content
// =================================================================================================

/// ⛔ **THE HEADLINE: two seats, two frames, each its own people's.**
///
/// Stated as an A/B on **one world**, because the failure worth catching is a mis-addressed frame —
/// both seats handed the same view — and one world captured for two seats exposes it from both
/// sides. Against a recorded baseline it would not: two frames that were both the human's would
/// still match a human-shaped expectation.
#[test]
fn each_seat_receives_a_frame_carrying_only_its_own_faction() {
    let app = a_world_with_two_seats();

    let home = frame_for(&app, HOME);
    let rival = frame_for(&app, RIVAL);

    assert_ne!(
        home, rival,
        "the two seats were handed the same view: per-seat capture is not happening at all"
    );

    for (seat, view, own_stock, other) in [
        (HOME, &home, HOME_STOCK, RIVAL),
        (RIVAL, &rival, RIVAL_STOCK, HOME),
    ] {
        assert!(
            view.bands_in_full.iter().all(|faction| *faction == seat.0),
            "seat {seat}'s frame carries another people's band in full: {:?}",
            view.bands_in_full
        );
        assert!(
            view.bands_in_full.contains(&seat.0),
            "seat {seat} cannot see its OWN band in full — the liveness half, and exactly the \
             defect this arc closes: a client seeing its own people as a foreign band"
        );
        assert_eq!(
            view.stockpiles,
            vec![(seat.0, own_stock)],
            "seat {seat}'s roster must be its own stockpile and nobody else's"
        );
        assert_eq!(
            view.demographics,
            vec![seat.0],
            "seat {seat}'s demographics must be its own people's; a foreign faction aggregates to \
             nothing and publishes no row"
        );
        assert!(
            !view
                .stockpiles
                .iter()
                .any(|(faction, _)| *faction == other.0),
            "seat {seat} was shown {other}'s stockpile"
        );
    }
}

/// **A world nobody has claimed a seat in still publishes exactly one view** — the idle boot app,
/// every library test, and a server before its first claim. The audience list being empty is not
/// "publish nothing"; it is `ViewerFaction`, which is what every one of those has always got.
#[test]
fn a_world_with_no_claimed_seat_publishes_the_single_viewer() {
    let mut app = world_with(ONE_RIVAL, |_| {});
    run_turn(&mut app);

    let audiences = app.world.resource::<SnapshotHistory>().audiences();
    assert_eq!(
        audiences,
        vec![HOME],
        "with no seat claimed the capture publishes the one `ViewerFaction` view"
    );
}

// =================================================================================================
// 2. The chain
// =================================================================================================

/// ⛔ **EACH SEAT NUMBERS ITS OWN PUBLICATIONS, AND ITS CHAIN NEVER BREAKS.**
///
/// This is the N-baselines hazard made observable. A client applies a delta only if its
/// `base_frame_seq` names the frame it is holding, so two seats drawing from one counter would each
/// see every other frame as a gap — and a client that drops a delta is stuck until it resyncs. The
/// assertion is per seat and on the frames the **sink** received, because the seat a frame was
/// addressed to is only visible there.
#[test]
fn two_seats_each_chain_their_own_frames_across_many_turns() {
    let mut app = a_world_with_two_seats();
    let recorder = Arc::new(Recorder::default());
    app.world
        .resource::<SnapshotHistory>()
        .attach_sink(Arc::clone(&recorder) as Arc<dyn FrameSink>);

    for _ in 0..CONVERGENCE_TURNS {
        run_turn(&mut app);
    }
    drain_publisher(&app);

    for seat in [HOME, RIVAL] {
        let frames = recorder.for_seat(seat);
        assert_eq!(
            frames.len(),
            CONVERGENCE_TURNS,
            "seat {seat} was sent {} frames for {CONVERGENCE_TURNS} turns — one frame per seat per \
             turn is the whole shape of per-seat delivery",
            frames.len()
        );
        let mut held = None;
        for (index, frame) in frames.iter().enumerate() {
            let (frame_seq, base_frame_seq, full) = chain_of(frame);
            match held {
                None => assert!(
                    !full || base_frame_seq == NO_BASE_FRAME_SEQ,
                    "seat {seat}'s first recorded frame is a full snapshot and must name no base"
                ),
                Some(previous) => assert_eq!(
                    base_frame_seq, previous,
                    "seat {seat}'s frame {index} names base {base_frame_seq} while that seat is \
                     holding {previous}: the two seats are drawing from one counter, and this \
                     client would drop the delta and ask for a resync"
                ),
            }
            held = Some(frame_seq);
        }
    }
}

/// ⛔ **THE EVENT CURSOR IS PER SEAT, and a shared one loses rows silently.**
///
/// `diff_appended` ships the rows above *"the highest seq this client has been sent"* and advances
/// the cursor to the highest it shipped. Shared between two seats, whichever seat was published
/// first would take the new events and the other would be sent none of them — no error, no gap, just
/// an event feed missing the turns the other player's frame happened to cover.
#[test]
fn each_seat_is_sent_every_event_of_its_own_and_none_of_the_others() {
    let mut app = a_world_with_two_seats();
    let recorder = Arc::new(Recorder::default());
    app.world
        .resource::<SnapshotHistory>()
        .attach_sink(Arc::clone(&recorder) as Arc<dyn FrameSink>);

    // **Rows the recorder could not have seen are out of scope.** The fixture resolves a turn before
    // the sink is attached, and that turn's feed rows went out in a frame nobody recorded — so the
    // claim below is about everything published from here on.
    let already_published = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .map(|entry| entry.seq)
        .max()
        .unwrap_or_default();

    // **Nothing is evicted for the length of this test.** The feed's window is anchored on the
    // newest entry's tick, so a row that aged out before its frame was built would look exactly like
    // a row a shared cursor swallowed — and the claim here is about the cursor, not the window.
    app.world
        .resource_mut::<CommandEventLog>()
        .set_retention_turns(RETAIN_EVERYTHING_TURNS);

    // One event per faction per turn, so a cursor shared between the seats cannot pass by accident.
    let mut expected: Vec<(u32, u64)> = Vec::new();
    for turn in 0..CONVERGENCE_TURNS {
        for seat in [HOME, RIVAL] {
            let mut log = app.world.resource_mut::<CommandEventLog>();
            // The log stamps the sequence — no call site may hand out a number it has issued.
            log.push(CommandEventEntry::new(
                turn as u64,
                CommandEventKind::Forage,
                seat,
                format!("seat {seat} turn {turn}"),
                None,
            ));
            let seq = log.iter().last().expect("the row just pushed").seq;
            expected.push((seat.0, seq));
        }
        run_turn(&mut app);
    }
    drain_publisher(&app);

    for seat in [HOME, RIVAL] {
        let mut received: Vec<(u32, u64)> = recorder
            .for_seat(seat)
            .iter()
            .flat_map(|frame| events_on(frame))
            .collect();
        let others: Vec<(u32, u64)> = received
            .iter()
            .copied()
            .filter(|(faction, _)| *faction != seat.0)
            .collect();
        assert!(
            others.is_empty(),
            "seat {seat} was sent another people's feed rows: {others:?}"
        );

        // The event log retains a window, so what a seat must be sent is every row of its own that
        // was still retained when its frame was built — asserted as a superset of the rows the log
        // still holds, and as no duplicates within one stream.
        let mut seqs: Vec<u64> = received.iter().map(|(_, seq)| *seq).collect();
        let before = seqs.len();
        seqs.sort_unstable();
        seqs.dedup();
        assert_eq!(
            before,
            seqs.len(),
            "seat {seat} was sent the same feed row twice; the cursor did not advance"
        );

        let retained: Vec<u64> = app
            .world
            .resource::<CommandEventLog>()
            .iter()
            .filter(|entry| entry.faction == seat && entry.seq > already_published)
            .map(|entry| entry.seq)
            .collect();
        let missing: Vec<u64> = retained
            .iter()
            .copied()
            .filter(|seq| !seqs.contains(seq))
            .collect();
        assert!(
            missing.is_empty(),
            "seat {seat} was never sent its own feed rows {missing:?} — the cursor is shared, so \
             the other seat's frame took them"
        );
        received.clear();
    }
}

// =================================================================================================
// 3. Rollback and resync
// =================================================================================================

/// ⛔ **A ROLLBACK REWINDS EVERY SEAT, AND ANSWERS EACH WITH ITS OWN WORLD.**
///
/// The world moves under all the occupants at once, so all their baselines rewind — and each is
/// re-baselined on **its own** recaptured frame, never on another seat's. The sequence is
/// deliberately *not* rewound: a frame carrying a stale number leaves the client baselined behind
/// the server, which is the defect `delta_streaming.rs` pins for one seat.
#[test]
fn a_rollback_answers_every_seat_with_a_fresh_full_frame_of_its_own_world() {
    let mut app = a_world_with_two_seats();
    for _ in 0..2 {
        run_turn(&mut app);
    }

    let before: Vec<(FactionId, u64)> = [HOME, RIVAL]
        .into_iter()
        .map(|seat| {
            let entry = app
                .world
                .resource::<SnapshotHistory>()
                .latest_entry_for(seat)
                .expect("both seats have been published to");
            (seat, entry.snapshot.header.frame_seq)
        })
        .collect();

    let frames = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.reset_all_to_latest_entry();
        history.publish_full_frame_for_all()
    };

    assert_eq!(
        frames.iter().map(|(seat, _)| *seat).collect::<Vec<_>>(),
        vec![HOME, RIVAL],
        "a rollback owes every occupied seat an answer, not just the first"
    );
    for (seat, frame) in &frames {
        let (frame_seq, base_frame_seq, full) = chain_of(frame);
        assert!(full, "seat {seat}'s rollback answer must be a FULL frame");
        assert_eq!(
            base_frame_seq, NO_BASE_FRAME_SEQ,
            "a full frame names no base"
        );
        let previous = before
            .iter()
            .find(|(published, _)| published == seat)
            .map(|(_, seq)| *seq)
            .expect("every seat was published to before the rollback");
        assert!(
            frame_seq > previous,
            "seat {seat}'s rollback frame claims {frame_seq}, which does not lead its own last \
             publication ({previous}) — the counter is monotonic per seat and is never rewound"
        );

        let view = decode_frame(frame);
        assert!(
            view.bands_in_full.iter().all(|faction| *faction == seat.0),
            "seat {seat}'s rollback frame carries another people's band in full: {:?} — a rollback \
             must answer each seat with ITS world, never with the frame another seat was rewound to",
            view.bands_in_full
        );
    }
}

/// **A resync answers the ASKING seat**, with that seat's own world and a live sequence number.
#[test]
fn a_resync_answers_the_asking_seat_with_its_own_world() {
    let mut app = a_world_with_two_seats();
    run_turn(&mut app);

    let home_before = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry_for(HOME)
        .expect("the human seat has a frame")
        .snapshot
        .header
        .frame_seq;

    let answer = app
        .world
        .resource_mut::<SnapshotHistory>()
        .publish_full_frame_for(RIVAL)
        .expect("the rival seat has a frame to republish");

    let (frame_seq, _, full) = chain_of(&answer);
    assert!(full, "a resync answers with a full frame");
    let view = decode_frame(&answer);
    assert_eq!(
        view.stockpiles,
        vec![(RIVAL.0, RIVAL_STOCK)],
        "the rival's resync must carry the RIVAL's roster: {view:?}"
    );

    let home_after = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry_for(HOME)
        .expect("the human seat still has its frame")
        .snapshot
        .header
        .frame_seq;
    assert_eq!(
        home_before, home_after,
        "answering one seat's resync must not touch another seat's stream"
    );
    assert!(
        frame_seq > 0,
        "the answer claims a live publication number of its own"
    );
}

/// **A seat that joins late is baselined on a FULL frame**, not on a delta against rows it never
/// received. Its publication state is created by its first publication, so `frame_seq == 0` means
/// "this client holds nothing" and the publication rule does the rest.
#[test]
fn a_seat_that_joins_late_is_baselined_on_a_full_frame() {
    let mut app = a_world_with_two_seats();
    let recorder = Arc::new(Recorder::default());
    app.world
        .resource::<SnapshotHistory>()
        .attach_sink(Arc::clone(&recorder) as Arc<dyn FrameSink>);
    run_turn(&mut app);
    drain_publisher(&app);

    // A third seat could not exist in a two-faction world, so the joiner here is the RIVAL seat
    // arriving after the human has been published to for several turns: its state is dropped and
    // rebuilt exactly as a fresh claim's would be.
    app.world
        .resource_mut::<SnapshotHistory>()
        .drop_audience(RIVAL);
    run_turn(&mut app);
    drain_publisher(&app);

    let rival_frames = recorder.for_seat(RIVAL);
    let (frame_seq, base_frame_seq, full) = chain_of(
        rival_frames
            .last()
            .expect("the rejoining seat was published to"),
    );
    assert!(
        full,
        "a seat whose publication state was dropped must be re-baselined on a FULL frame; it was \
         sent a delta against rows it does not hold"
    );
    assert_eq!(
        base_frame_seq, NO_BASE_FRAME_SEQ,
        "a full frame names no base"
    );
    assert_eq!(
        frame_seq, FIRST_PUBLICATION_SEQ,
        "a fresh publication state starts its own chain at {FIRST_PUBLICATION_SEQ}"
    );
}

/// ⛔ **A SEAT WHOSE STATE IS FRESH AT A *RECAPTURE* IS BASELINED TOO — THE PUBLICATION RULE HOLDS
/// WHATEVER THE `Publication` KIND.**
///
/// The turn path is the one the case above covers, and it is not the only way a fresh publication
/// state meets its first frame. A **command-link reconnect** drops the seat's state
/// (`sync_seat_delivery`) and the very next thing the loop does with a world-mutating command is
/// *recapture* — no turn in between. A recapture deliberately holds its baseline, which is what makes
/// its deltas cumulative; held on a state holding nothing it produced a delta naming
/// `base_frame_seq == 0`, a frame the client cannot apply. It dropped it, asked to resync, and got
/// `resync.no_world` — because the same arm pushed no ring entry either — until its retry budget ran
/// out and it declared a live world gone.
///
/// Both halves are asserted, and the second is what stops the fix over-reaching: the *established*
/// seat's frame on that same recapture is still a delta, so the cumulative-delta rule is untouched
/// for every seat that has a baseline to hold.
#[test]
fn a_seat_whose_state_is_fresh_at_a_recapture_is_baselined_on_a_full_frame() {
    let mut app = a_world_with_two_seats();
    let recorder = Arc::new(Recorder::default());
    app.world
        .resource::<SnapshotHistory>()
        .attach_sink(Arc::clone(&recorder) as Arc<dyn FrameSink>);
    run_turn(&mut app);
    drain_publisher(&app);

    // The reconnect: `sync_seat_delivery` drops the released seat's publication state, and the
    // re-claim builds a fresh one. Same effect, without standing the server loop up.
    app.world
        .resource_mut::<SnapshotHistory>()
        .drop_audience(RIVAL);

    // And what happens next is a world-mutating command, not a turn.
    recapture_snapshot_in_place(&mut app.world);
    drain_publisher(&app);

    let rival_frames = recorder.for_seat(RIVAL);
    let (frame_seq, base_frame_seq, full) = chain_of(
        rival_frames
            .last()
            .expect("the rejoining seat was published to"),
    );
    assert!(
        full,
        "the rejoining seat's first frame came out of a RECAPTURE and must still be a full \
         baseline; it was sent a delta against rows it does not hold, and a resync would have \
         answered `resync.no_world`"
    );
    assert_eq!(
        base_frame_seq, NO_BASE_FRAME_SEQ,
        "a full frame names no base"
    );
    assert_eq!(
        frame_seq, FIRST_PUBLICATION_SEQ,
        "a fresh publication state starts its own chain at {FIRST_PUBLICATION_SEQ}"
    );

    // The ring entry is the other half of the same defect: a recapture that pushed none left
    // `latest_entry` empty, so the resync the dropped delta provoked had nothing to answer with.
    assert!(
        app.world
            .resource_mut::<SnapshotHistory>()
            .publish_full_frame_for(RIVAL)
            .is_some(),
        "a first publication must push a ring entry, or `Command::Resync` answers \
         `resync.no_world` for a live world"
    );

    // The established seat is untouched: it holds a baseline, so its recapture frame is the
    // cumulative delta it always was.
    let (_, home_base, home_full) = chain_of(
        recorder
            .for_seat(HOME)
            .last()
            .expect("the seated player was published to"),
    );
    assert!(
        !home_full,
        "a recapture must stay a DELTA for a seat that holds a baseline — the first-publication \
         rule is about having nothing to hold, not about recaptures"
    );
    assert!(
        home_base > NO_BASE_FRAME_SEQ,
        "and it names the frame that seat is holding"
    );
}
