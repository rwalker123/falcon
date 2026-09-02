//! **Food that crossed between two bands has to appear in the ledger.**
//!
//! `PopulationCohortState`'s food ledger is documented, and pinned by two sibling files, as
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − raidForfeit
//! ```
//!
//! Every term on the right is about **this** band: what its own workers produced, what its own
//! people ate, what its own raid cost it. So food that *moves between larders* fits
//! nowhere in it — and two systems move food between larders every game:
//!
//! - **`balance_supply_networks`** equalises co-networked same-faction bands, every turn, since turn
//!   one. This was a **pre-existing hole**: the two sibling ledger tests each stand up a *single*
//!   band, no network forms (`MIN_NETWORK_MEMBERS` is 2), and the identity was therefore never
//!   exercised against a transfer at all. `the_pre_transfer_identity_is_short_by_exactly_the_move`
//!   below measures the gap rather than asserting it from the doc.
//! - **a trade expedition** (arc #527) draws cargo off the sending band at launch and hands it to
//!   the destination on arrival.
//! - **a band split** (`split_band_from_parent`) hands the new band its share of the parent's
//!   stores. It is a *command*, so it lands between two captures — see
//!   `the_food_ledger_reconciles_when_a_band_splits_mid_window`, which is deliberately the one case
//!   here that splits **inside** the measured window rather than while building its fixture.
//!
//! `transferReceived` / `transferSent` close both with **one pair of terms**, because they are one
//! fact — *food that crossed between bands outside income and consumption* — and the identity is now
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − raidForfeit
//!                 + transferReceived − transferSent
//! ```
//!
//! Asserted against **real turns through the real systems and the real exported snapshot**, the
//! shape `pen_food_ledger.rs` and `raid_food_ledger.rs` already use — never against a
//! re-derivation of the sim's own arithmetic.

use bevy::prelude::Entity;
use core_sim::{
    build_test_app, recapture_snapshot_in_place, run_turn, scalar_from_f32, split_band_from_parent,
    BandId, BandTravel, Expedition, ExpeditionMission, ExpeditionPhase, LaborAllocation,
    LocalStore, PopulationCohort, ResidentBand, Scalar, SettleConfig, SimulationConfig,
    SnapshotHistory, StartingUnit, Tile, TileRegistry, FOOD,
};

/// The shipped default `map_seed` is `0` ("seed from entropy"), so a test must pin its own or every
/// run lands on a different map.
const SEED: u64 = 119_304_647;
/// The exported floats are `f32` sums of `Scalar`-quantized moves; a few ULPs of slack, no more.
const EPSILON: f32 = 0.01;
/// Working-age people the parent is stocked with, so a split leaves two real bands on any seed.
const PARENT_WORKERS: f32 = 20.0;
/// Workers the second band is founded with.
const SPLIT_WORKERS: u32 = 5;
/// Food the **fed** band opens with. Far above the per-capita balance the network equalises toward,
/// so there is a large, unambiguous transfer to reconcile.
const FED_LARDER: f32 = 400.0;
/// And what the **hungry** one opens with: nothing, so every unit it holds afterwards arrived.
const HUNGRY_LARDER: f32 = 0.0;
/// The larder a band that published no earlier frame is measured against — a client's `larder_delta`
/// for a band appearing for the first time runs from nothing.
const NO_PRIOR_FRAME_LARDER: f32 = 0.0;
/// Workers a shipment party carries, and the food it hauls — inside `trade.per_worker_carry × 2`.
const PARTY_WORKERS: u32 = 2;
const CARGO_FOOD: f32 = 8.0;
/// The faction the *destination* of a shipment belongs to. A different one, deliberately: it is the
/// arc's own claim (faction is a property of the endpoint, never a branch) and it keeps the supply
/// network — which pools **same-faction** bands — out of a test about a shipment.
const FOREIGN_FACTION: core_sim::FactionId = core_sim::FactionId(9);

/// Every ledger term a band published this turn, read off the **exported snapshot** — the numbers a
/// client reads, not the sim's internals.
#[derive(Debug, Clone, Copy)]
struct Ledger {
    income: f32,
    consumption: f32,
    raid_forfeit: f32,
    received: f32,
    sent: f32,
}

impl Ledger {
    /// The identity's right-hand side, with the two new terms.
    fn expected_delta(&self) -> f32 {
        self.income - self.consumption - self.raid_forfeit + self.received - self.sent
    }

    /// The right-hand side **as it read before the transfer terms existed** — what a client
    /// computing the documented identity would have got.
    fn pre_transfer_delta(&self) -> f32 {
        self.income - self.consumption - self.raid_forfeit
    }
}

fn ledger_of(app: &bevy::prelude::App, band: BandId) -> Ledger {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .clone()
        .expect("a snapshot was captured");
    let cohort = snapshot
        .populations
        .iter()
        .find(|cohort| cohort.band_id == band.0)
        .expect("the band is exported");
    Ledger {
        income: cohort.food_income,
        consumption: cohort.food_consumption,
        raid_forfeit: cohort.raid_forfeit,
        received: cohort.transfer_received,
        sent: cohort.transfer_sent,
    }
}

fn larder(app: &bevy::prelude::App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .get(FOOD)
        .to_f32()
}

fn band_id(app: &bevy::prelude::App, band: Entity) -> BandId {
    *app.world.get::<BandId>(band).expect("a band has an id")
}

fn set_larder(app: &mut bevy::prelude::App, band: Entity, food: f32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .set(FOOD, scalar_from_f32(food));
}

fn first_band(app: &mut bevy::prelude::App) -> Entity {
    let mut query = app.world.query_filtered::<Entity, (
        bevy::prelude::With<PopulationCohort>,
        bevy::prelude::With<ResidentBand>,
    )>();
    query
        .iter(&app.world)
        .next()
        .expect("the campaign spawns a resident band")
}

fn entity_for_band(app: &mut bevy::prelude::App, wanted: BandId) -> Option<Entity> {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == wanted)
        .map(|(entity, _)| entity)
}

fn world() -> bevy::prelude::App {
    let mut app = build_test_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();
    app
}

/// Stock a band with enough working-age people that a split leaves two real bands on any seed.
fn stock_workers(app: &mut bevy::prelude::App, band: Entity) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists");
    cohort.working = Scalar::from_f32(PARENT_WORKERS);
    cohort.sync_size();
}

/// The floors lifted, so a fixture can split whatever it likes.
fn permissive_settle() -> SettleConfig {
    SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    }
}

/// **Two co-located bands of the same faction** — which is exactly the fixture that forms a supply
/// network (`MIN_NETWORK_MEMBERS` is 2, and they are well inside `reach_tiles`). One opens fed, the
/// other empty, so the balancing pass has a large move to make.
///
/// **A published frame stands between the split and the measurement, deliberately.** The split books
/// its dowry into the transfer pair (`split_band_from_parent`) — and the larders this fixture then
/// *forces* to `FED_LARDER`/`HUNGRY_LARDER` are not the larders that move produced, so leaving the
/// counters standing would have every test below open with a transfer that no longer describes
/// anything. One turn publishes and clears them (`reset_transfer_ledger`), so what these tests
/// measure is the balancing pass alone. The split's *own* transfer is
/// `the_food_ledger_reconciles_when_a_band_splits_mid_window`'s subject.
fn two_networked_bands(app: &mut bevy::prelude::App) -> (Entity, Entity) {
    let parent = first_band(app);
    stock_workers(app, parent);
    let settle = permissive_settle();
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle)
        .expect("a stocked parent can split");
    let child = entity_for_band(app, split.band).expect("the split allocated this id");
    run_turn(app);
    set_larder(app, parent, FED_LARDER);
    set_larder(app, child, HUNGRY_LARDER);
    (parent, child)
}

/// **The identity holds on BOTH sides of a supply-network move.** The fed band's larder falls by
/// more than its people ate, the hungry one's rises with no income of its own, and each reconciles
/// exactly once the transfer terms are in.
#[test]
fn the_food_ledger_reconciles_across_a_supply_network_transfer() {
    let mut app = world();
    let (fed, hungry) = two_networked_bands(&mut app);
    let (fed_id, hungry_id) = (band_id(&app, fed), band_id(&app, hungry));

    let fed_before = larder(&app, fed);
    let hungry_before = larder(&app, hungry);
    run_turn(&mut app);
    let fed_after = larder(&app, fed);
    let hungry_after = larder(&app, hungry);

    let fed_ledger = ledger_of(&app, fed_id);
    let hungry_ledger = ledger_of(&app, hungry_id);

    // Liveness: a transfer really happened, in both directions, or every assertion below is vacuous.
    assert!(
        fed_ledger.sent > 0.0,
        "the fed band must report giving food up: {fed_ledger:?}"
    );
    assert!(
        hungry_ledger.received > 0.0,
        "and the hungry one receiving it: {hungry_ledger:?}"
    );

    let fed_delta = fed_after - fed_before;
    assert!(
        (fed_delta - fed_ledger.expected_delta()).abs() < EPSILON,
        "the SENDER's ledger must reconcile: delta={fed_delta} vs {} ({fed_ledger:?})",
        fed_ledger.expected_delta()
    );
    let hungry_delta = hungry_after - hungry_before;
    assert!(
        (hungry_delta - hungry_ledger.expected_delta()).abs() < EPSILON,
        "the RECEIVER's ledger must reconcile: delta={hungry_delta} vs {} ({hungry_ledger:?})",
        hungry_ledger.expected_delta()
    );
}

/// **The hole, measured rather than asserted from the doc.** Without the transfer terms the
/// documented identity is short by *exactly* the move — on both bands, in opposite directions.
///
/// This is the test that says the supply-network half was a pre-existing defect and not something
/// the trade arc introduced: it needs no expedition and no shipment, only two bands standing near
/// each other, which is the shipped opening the moment a band splits.
#[test]
fn the_pre_transfer_identity_is_short_by_exactly_the_move() {
    let mut app = world();
    let (fed, hungry) = two_networked_bands(&mut app);
    let (fed_id, hungry_id) = (band_id(&app, fed), band_id(&app, hungry));

    let fed_before = larder(&app, fed);
    let hungry_before = larder(&app, hungry);
    run_turn(&mut app);

    let fed_ledger = ledger_of(&app, fed_id);
    let hungry_ledger = ledger_of(&app, hungry_id);
    let fed_gap = (larder(&app, fed) - fed_before) - fed_ledger.pre_transfer_delta();
    let hungry_gap = (larder(&app, hungry) - hungry_before) - hungry_ledger.pre_transfer_delta();

    assert!(
        (fed_gap + fed_ledger.sent).abs() < EPSILON,
        "the old identity overstated the sender's larder by exactly what it gave away: \
         gap={fed_gap}, sent={}",
        fed_ledger.sent
    );
    assert!(
        (hungry_gap - hungry_ledger.received).abs() < EPSILON,
        "and understated the receiver's by exactly what arrived: gap={hungry_gap}, received={}",
        hungry_ledger.received
    );
    assert!(
        fed_gap.abs() > EPSILON,
        "the liveness half — a zero gap would let this test pass on a sim that moves no food"
    );
}

/// **A band that splits MID-WINDOW books the dowry on both ends.**
///
/// `split_band_from_parent` takes a share of every good off the parent and hands it to the child,
/// and a split is a *command*: it is applied between one capture and the next, inside the interval a
/// client's `larder_delta` measures. So the parent's larder falls by food its people never ate and
/// the child's opens at food it never grew — which is precisely what the transfer pair exists to
/// name, and the identity is false on that turn without it.
///
/// The other fixtures here split while *building* themselves and then overwrite both larders, which
/// is exactly why this hole survived them. This one splits after the first published frame and
/// touches nothing afterwards.
#[test]
fn the_food_ledger_reconciles_when_a_band_splits_mid_window() {
    let mut app = world();
    let parent = first_band(&mut app);
    stock_workers(&mut app, parent);
    set_larder(&mut app, parent, FED_LARDER);
    let parent_id = band_id(&app, parent);

    // One published frame first, so the window the split falls into has a defined start.
    run_turn(&mut app);
    let parent_before = larder(&app, parent);

    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &permissive_settle())
        .expect("a stocked parent can split");
    let child = entity_for_band(&mut app, split.band).expect("the split allocated this id");
    let dowry = split.provisions.to_f32();

    run_turn(&mut app);

    let parent_ledger = ledger_of(&app, parent_id);
    let child_ledger = ledger_of(&app, split.band);

    // Liveness: real food walked out with them, or both identities below are about nothing.
    assert!(
        dowry > EPSILON,
        "the split must hand over food for this test to say anything: dowry={dowry}"
    );
    assert!(
        parent_ledger.sent >= dowry - EPSILON,
        "the parent must report giving the dowry up: dowry={dowry} ({parent_ledger:?})"
    );
    assert!(
        child_ledger.received >= dowry - EPSILON,
        "and the child receiving it: dowry={dowry} ({child_ledger:?})"
    );

    let parent_delta = larder(&app, parent) - parent_before;
    assert!(
        (parent_delta - parent_ledger.expected_delta()).abs() < EPSILON,
        "the PARENT's ledger must reconcile across the split: delta={parent_delta} vs {} \
         ({parent_ledger:?})",
        parent_ledger.expected_delta()
    );

    // The child published no earlier frame, so the delta a client draws for it is measured from
    // nothing at all.
    let child_delta = larder(&app, child) - NO_PRIOR_FRAME_LARDER;
    assert!(
        (child_delta - child_ledger.expected_delta()).abs() < EPSILON,
        "the CHILD's ledger must reconcile on its first frame: delta={child_delta} vs {} \
         ({child_ledger:?})",
        child_ledger.expected_delta()
    );
}

/// **The identity holds across a trade shipment's arrival.** The destination band has no income of
/// its own, no pen and no raid, so the whole of its larder's rise is the shipment — and the ledger
/// says so.
///
/// The destination is a **different faction**, which is both the arc's claim (faction is never a
/// branch) and what keeps the supply network out of a test about a shipment.
#[test]
fn the_food_ledger_reconciles_when_a_shipment_lands() {
    let mut app = world();
    let (sender, host) = two_networked_bands(&mut app);
    app.world
        .get_mut::<PopulationCohort>(host)
        .expect("the band exists")
        .faction = FOREIGN_FACTION;
    let host_id = band_id(&app, host);

    // A loaded party standing in the destination's camp: it hands the cargo over on the next turn.
    let host_pos = {
        let tile = app
            .world
            .get::<PopulationCohort>(host)
            .expect("the band")
            .current_tile;
        app.world.get::<Tile>(tile).expect("a real tile").position
    };
    spawn_shipment(&mut app, sender, host_id, host_pos, CARGO_FOOD);

    let host_before = larder(&app, host);
    run_turn(&mut app);
    let host_after = larder(&app, host);

    let ledger = ledger_of(&app, host_id);
    assert!(
        (ledger.received - CARGO_FOOD).abs() < EPSILON,
        "the shipment is reported as received, in full: {ledger:?}"
    );
    assert_eq!(
        ledger.sent, 0.0,
        "a band that only received must not report sending: {ledger:?}"
    );
    let delta = host_after - host_before;
    assert!(
        (delta - ledger.expected_delta()).abs() < EPSILON,
        "the receiving band's ledger must reconcile: delta={delta} vs {} ({ledger:?})",
        ledger.expected_delta()
    );
}

/// Spawn a loaded trade party bound for `destination`, the way the launch command builds one. The
/// command handler lives in the server binary, which an integration test cannot link against; what
/// this file is about is the ledger, and the ledger's receiving half is written by the *turn*.
fn spawn_shipment(
    app: &mut bevy::prelude::App,
    home: Entity,
    destination: BandId,
    destination_pos: bevy::math::UVec2,
    food: f32,
) -> Entity {
    let mut cohort = app
        .world
        .get::<PopulationCohort>(home)
        .expect("the home band exists")
        .clone();
    cohort.working = Scalar::from_u32(PARTY_WORKERS);
    cohort.children = Scalar::from_i64(0);
    cohort.elders = Scalar::from_i64(0);
    cohort.stores = LocalStore::new();
    cohort.sync_size();
    let mut cargo = LocalStore::new();
    cargo.add(FOOD, scalar_from_f32(food));
    let id = app
        .world
        .resource_mut::<core_sim::BandIdAllocator>()
        .allocate();
    app.world
        .spawn((
            cohort,
            id,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: home,
                mission: ExpeditionMission::Trade {
                    destination_band: destination,
                    destination_name: "the neighbours".to_string(),
                },
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
                cargo,
            },
            BandTravel {
                target: destination_pos,
            },
        ))
        .id()
}

/// A guard against the counters silently becoming cumulative: they describe **this** snapshot
/// window, so a quiet turn must publish zeroes even right after a busy one.
#[test]
fn the_transfer_terms_are_cleared_between_snapshots() {
    let mut app = world();
    let (fed, hungry) = two_networked_bands(&mut app);
    let (fed_id, hungry_id) = (band_id(&app, fed), band_id(&app, hungry));

    run_turn(&mut app);
    assert!(
        ledger_of(&app, fed_id).sent > 0.0,
        "the busy turn really moved food"
    );
    let _ = hungry_id;

    // Walk the neighbour out of `reach_tiles`, so no network forms and there is genuinely nothing
    // to move. The fed band still eats, so the identity on the quiet turn is a real statement about
    // a larder that changed rather than one that stood still.
    walk_out_of_reach(&mut app, hungry);
    let before = larder(&app, fed);
    run_turn(&mut app);

    let quiet = ledger_of(&app, fed_id);
    assert!(
        quiet.sent < EPSILON && quiet.received < EPSILON,
        "a turn with nothing to balance publishes zeroes, not last turn's numbers: {quiet:?}"
    );
    assert!(
        quiet.consumption > 0.0,
        "the liveness half — the quiet turn's identity must still be about a larder that moved"
    );
    let delta = larder(&app, fed) - before;
    assert!(
        (delta - quiet.expected_delta()).abs() < EPSILON,
        "and the identity still holds on the quiet turn: delta={delta} vs {} ({quiet:?})",
        quiet.expected_delta()
    );
}

// ---------------------------------------------------------------------------------------------
// A REFRESHED frame still states the turn's transfers
// ---------------------------------------------------------------------------------------------

/// **Both transfer pairs off the ENCODED envelope**, through the accessor chain a client uses:
/// `(transferReceived, transferSent, transferReceivedTurn, transferSentTurn)`. A field that never
/// reached the codec still passes an in-process assertion.
fn published_transfers(app: &bevy::prelude::App, band: BandId) -> (f32, f32, f32, f32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published")
        .iter()
        .find(|cohort| cohort.bandId() == band.0)
        .expect("the band's row is published");
    (
        row.transferReceived(),
        row.transferSent(),
        row.transferReceivedTurn(),
        row.transferSentTurn(),
    )
}

/// **A command after the turn must not blank the ⇄ rows.** `recapture_snapshot_in_place` re-runs the
/// capture against live components after every dispatched command, and by then
/// `reset_transfer_ledger` has cleared the accumulating pair — so the refreshed frame published
/// `0.0` for both terms and overwrote the correct turn-end frame in `SnapshotHistory`.
///
/// The two pairs are asserted against each other in both frames, which is what makes this a
/// statement about the *reset*, not about a number:
///
/// - on the **turn** frame `transferReceivedTurn == transferReceived` (they are one counter, read a
///   moment apart), and both are non-zero — the liveness half, without which every assertion here
///   would pass on a sim that moved no food at all;
/// - on the **refreshed** frame the accumulating pair reads zero and the per-turn pair is unchanged.
///
/// It must go through `recapture_snapshot_in_place` rather than a second `capture_snapshot`: the
/// recapture path is the one the defect lived on.
#[test]
fn a_recapture_still_publishes_the_turns_transfers() {
    let mut app = world();
    let (fed, hungry) = two_networked_bands(&mut app);
    let (fed_id, hungry_id) = (band_id(&app, fed), band_id(&app, hungry));

    run_turn(&mut app);

    // --- the turn's own frame -----------------------------------------------------------------
    let (fed_received, fed_sent, fed_received_turn, fed_sent_turn) =
        published_transfers(&app, fed_id);
    assert!(
        fed_sent > EPSILON,
        "liveness: the fed band must have shipped food on this turn, got {fed_sent}"
    );
    assert!(
        (fed_received_turn - fed_received).abs() < EPSILON
            && (fed_sent_turn - fed_sent).abs() < EPSILON,
        "on a turn frame the two pairs are one counter read twice: \
         {fed_received}/{fed_sent} vs {fed_received_turn}/{fed_sent_turn}"
    );

    let (got_received, got_sent, got_received_turn, got_sent_turn) =
        published_transfers(&app, hungry_id);
    assert!(
        got_received > EPSILON,
        "liveness: the hungry band must have received food, got {got_received}"
    );
    assert!(
        (got_received_turn - got_received).abs() < EPSILON
            && (got_sent_turn - got_sent).abs() < EPSILON,
        "on a turn frame the two pairs agree for the receiver too: \
         {got_received}/{got_sent} vs {got_received_turn}/{got_sent_turn}"
    );

    // --- what a command's refresh republishes --------------------------------------------------
    recapture_snapshot_in_place(&mut app.world);

    let (received, sent, received_turn, sent_turn) = published_transfers(&app, fed_id);
    assert_eq!(
        (received, sent),
        (0.0, 0.0),
        "the accumulating pair is cleared after the turn capture, so a refreshed frame reads zero"
    );
    assert!(
        (sent_turn - fed_sent).abs() < EPSILON,
        "the per-turn pair must survive the refresh: {sent_turn} vs {fed_sent}"
    );
    assert!(
        received_turn.abs() < EPSILON,
        "and a band that received nothing still reports nothing: {received_turn}"
    );

    let (received, sent, received_turn, sent_turn) = published_transfers(&app, hungry_id);
    assert_eq!((received, sent), (0.0, 0.0));
    assert!(
        (received_turn - got_received).abs() < EPSILON,
        "the receiver's row survives the refresh too: {received_turn} vs {got_received}"
    );
    assert!(sent_turn.abs() < EPSILON);
}

// ---------------------------------------------------------------------------------------------
// WHAT MOVED IT: the two link kinds, both accounts (issue #548)
// ---------------------------------------------------------------------------------------------

/// The hay the fed band opens with, so the fodder account has a move of its own to state. Nothing
/// eats it — the fixture keeps no pens — so every unit that leaves this band crossed to the other.
const FED_HAY: f32 = 240.0;

/// Every per-link figure one band published, for one account, off the **encoded envelope**.
#[derive(Debug, Clone, Copy)]
struct LinkSplit {
    local_received: f32,
    local_sent: f32,
    route_received: f32,
    route_sent: f32,
    /// The **per-turn** summed pair the split refines — `transferReceivedTurn` / `transferSentTurn`
    /// for the food account, which is the basis these four arms are on. `None` for fodder, which has
    /// no summed pair at all: the reconciliation identity is the food one.
    total_received: Option<f32>,
    total_sent: Option<f32>,
}

impl LinkSplit {
    /// ⛔ **THE CLAIM THE WHOLE SPLIT RESTS ON**: the two kinds are exhaustive, so they add back up
    /// to the pair they refine. A third mechanism booked outside `TransferLedger` shows up here as a
    /// shortfall and nowhere else.
    fn assert_is_the_whole_of_the_pair(&self, label: &str) {
        let (Some(total_received), Some(total_sent)) = (self.total_received, self.total_sent)
        else {
            return;
        };
        assert!(
            (self.local_received + self.route_received - total_received).abs() < EPSILON,
            "{label}: local + route must be the whole of what arrived — {} + {} vs {total_received}",
            self.local_received,
            self.route_received
        );
        assert!(
            (self.local_sent + self.route_sent - total_sent).abs() < EPSILON,
            "{label}: local + route must be the whole of what left — {} + {} vs {total_sent}",
            self.local_sent,
            self.route_sent
        );
    }
}

/// `(food, fodder)` — both accounts' per-link figures for one band, read off the encoded envelope
/// through the accessor chain a client uses. A field that never reached the codec still passes an
/// in-process assertion, which is why nothing here reads the capture struct.
fn published_link_splits(app: &bevy::prelude::App, band: BandId) -> (LinkSplit, LinkSplit) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published")
        .iter()
        .find(|cohort| cohort.bandId() == band.0)
        .expect("the band's row is published");
    (
        LinkSplit {
            local_received: row.transferLocalReceivedTurn(),
            local_sent: row.transferLocalSentTurn(),
            route_received: row.transferRouteReceivedTurn(),
            route_sent: row.transferRouteSentTurn(),
            total_received: Some(row.transferReceivedTurn()),
            total_sent: Some(row.transferSentTurn()),
        },
        LinkSplit {
            local_received: row.fodderTransferLocalReceivedTurn(),
            local_sent: row.fodderTransferLocalSentTurn(),
            route_received: row.fodderTransferRouteReceivedTurn(),
            route_sent: row.fodderTransferRouteSentTurn(),
            total_received: None,
            total_sent: None,
        },
    )
}

/// Stock a band's `FODDER` store — the account that has always pooled and, until this arc, was never
/// counted.
fn set_hay(app: &mut bevy::prelude::App, band: Entity, hay: f32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .set(core_sim::FODDER, scalar_from_f32(hay));
}

/// **A fixture where one band both POOLS and RECEIVES A SHIPMENT on the same turn**, which is the
/// only arrangement that can tell the two link kinds apart: the receiving band's row has to carry a
/// non-zero figure on *each* arm, or an implementation that booked everything to one of them would
/// still satisfy the identity.
///
/// The hay rides along on the same pass — the balancer walks a band's whole store — so one turn
/// exercises all sixteen figures.
fn a_pooling_and_shipping_turn(app: &mut bevy::prelude::App) -> (Entity, Entity) {
    let (fed, hungry) = two_networked_bands(app);
    set_hay(app, fed, FED_HAY);
    let hungry_id = band_id(app, hungry);
    let hungry_pos = {
        let tile = app
            .world
            .get::<PopulationCohort>(hungry)
            .expect("the band")
            .current_tile;
        app.world.get::<Tile>(tile).expect("a real tile").position
    };
    spawn_shipment(app, fed, hungry_id, hungry_pos, CARGO_FOOD);
    run_turn(app);
    (fed, hungry)
}

/// ⛔ **THE ROWS SAY WHICH MECHANISM MOVED THE GOODS, AND THE TWO KINDS ARE THE WHOLE OF IT.**
///
/// One turn in which the same band is pooled with a neighbour *and* handed a shipment. The receiving
/// band's food row therefore carries **both** arms, and the shipment's arm is pinned at the cargo it
/// was loaded with — so a booking that folded a delivery into `local` (or pooling into `route`)
/// fails on a magnitude and not merely on a sum.
///
/// The hay account is asserted in the same breath: `balance_supply_networks` walks a band's whole
/// store, so fodder crosses on the `local` arm exactly as food does. Its `route` arm is `0` because
/// a shipment's manifest refuses fodder — a fact about shipments, asserted here so the day that
/// changes this test is what notices.
#[test]
fn both_accounts_state_which_link_moved_the_goods() {
    let mut app = world();
    let (fed, hungry) = a_pooling_and_shipping_turn(&mut app);
    let (fed_id, hungry_id) = (band_id(&app, fed), band_id(&app, hungry));

    let (fed_food, fed_hay) = published_link_splits(&app, fed_id);
    let (hungry_food, hungry_hay) = published_link_splits(&app, hungry_id);

    // --- the sender: pooling only, on both accounts ---------------------------------------------
    assert!(
        fed_food.local_sent > EPSILON && fed_hay.local_sent > EPSILON,
        "liveness: the fed band must have pooled BOTH accounts away (food {}, hay {})",
        fed_food.local_sent,
        fed_hay.local_sent
    );

    // --- the receiver: both arms at once --------------------------------------------------------
    assert!(
        hungry_food.local_received > EPSILON,
        "the pooled share arrives on the LOCAL arm, got {}",
        hungry_food.local_received
    );
    assert!(
        (hungry_food.route_received - CARGO_FOOD).abs() < EPSILON,
        "and the shipment arrives on the ROUTE arm, in full: {} vs {CARGO_FOOD}",
        hungry_food.route_received
    );
    assert!(
        hungry_hay.local_received > EPSILON,
        "hay pools where grain pools, so the receiver's fodder LOCAL arm is live, got {}",
        hungry_hay.local_received
    );
    assert_eq!(
        (hungry_hay.route_received, hungry_hay.route_sent),
        (0.0, 0.0),
        "a shipment cannot carry fodder today, so the hay ROUTE arm is zero on both sides"
    );

    // --- and the arms are the whole of the pair they refine, on every row ------------------------
    fed_food.assert_is_the_whole_of_the_pair("the sender's food row");
    hungry_food.assert_is_the_whole_of_the_pair("the receiver's food row");
}

/// ⛔ **ALL SIXTEEN FIGURES ARE PER-TURN STATE, NOT ACCUMULATORS.**
///
/// `recapture_snapshot_in_place` re-runs the capture against live components after every dispatched
/// command, by which time `reset_transfer_ledger` has cleared the accumulating pair. A row read off
/// an accumulator therefore blanks on the first frame a command refreshes — the defect issue #517
/// fixed on the generic pair, and the reason every one of these is copied onto the cohort before the
/// turn's capture instead.
///
/// The accumulating pair reading zero in the same frame is what makes this a statement about the
/// **reset** rather than about a number that happens not to have changed.
#[test]
fn a_recapture_still_publishes_every_link_kind() {
    let mut app = world();
    let (fed, hungry) = a_pooling_and_shipping_turn(&mut app);
    let (fed_id, hungry_id) = (band_id(&app, fed), band_id(&app, hungry));

    let before = [
        published_link_splits(&app, fed_id),
        published_link_splits(&app, hungry_id),
    ];

    recapture_snapshot_in_place(&mut app.world);

    let after = [
        published_link_splits(&app, fed_id),
        published_link_splits(&app, hungry_id),
    ];
    for ((food_before, hay_before), (food_after, hay_after)) in before.iter().zip(after.iter()) {
        for (label, was, now) in [
            ("food", food_before, food_after),
            ("fodder", hay_before, hay_after),
        ] {
            assert_eq!(
                (
                    now.local_received,
                    now.local_sent,
                    now.route_received,
                    now.route_sent
                ),
                (
                    was.local_received,
                    was.local_sent,
                    was.route_received,
                    was.route_sent
                ),
                "the {label} account's four arms must survive a command's refresh"
            );
        }
        assert_eq!(
            (food_after.total_received, food_after.total_sent),
            (food_before.total_received, food_before.total_sent),
            "and so does the per-turn pair they add up to"
        );
    }
    // Meanwhile the ACCUMULATING pair is cleared once the turn's capture has read it — which is the
    // whole reason the sixteen figures above are per-turn state and not read off it.
    for band in [fed_id, hungry_id] {
        let (received, sent, _, _) = published_transfers(&app, band);
        assert_eq!(
            (received, sent),
            (0.0, 0.0),
            "the accumulating pair reads zero on a refreshed frame, as it always has"
        );
    }
    assert!(
        before[1].0.local_received > EPSILON && before[1].0.route_received > EPSILON,
        "liveness: the receiver's two arms were non-zero before the refresh ({:?})",
        before[1].0
    );
}

/// How far to walk a band so no supply network forms with it. Well past the shipped
/// `supply_network_config.reach_tiles`.
const OUT_OF_NETWORK_TILES: u32 = 20;

fn walk_out_of_reach(app: &mut bevy::prelude::App, band: Entity) {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let from = {
        let tile = app
            .world
            .get::<PopulationCohort>(band)
            .expect("the band")
            .current_tile;
        app.world.get::<Tile>(tile).expect("a real tile").position
    };
    let target = bevy::math::UVec2::new(
        (from.x + OUT_OF_NETWORK_TILES) % width.max(1),
        from.y.min(height.saturating_sub(1)),
    );
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(target.x, target.y)
        .expect("the target tile is on the map");
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists");
    cohort.current_tile = tile;
    cohort.home = tile;
}
