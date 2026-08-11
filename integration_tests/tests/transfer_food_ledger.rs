//! **Food that crossed between two bands has to appear in the ledger.**
//!
//! `PopulationCohortState`'s food ledger is documented, and pinned by two sibling files, as
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − penFeedUpkeep − raidForfeit
//! ```
//!
//! Every term on the right is about **this** band: what its own workers produced, what its own
//! people ate, what its own pen and its own raid cost it. So food that *moves between larders* fits
//! nowhere in it — and two systems move food between larders every game:
//!
//! - **`balance_supply_networks`** equalises co-networked same-faction bands, every turn, since turn
//!   one. This was a **pre-existing hole**: the two sibling ledger tests each stand up a *single*
//!   band, no network forms (`MIN_NETWORK_MEMBERS` is 2), and the identity was therefore never
//!   exercised against a transfer at all. `the_pre_transfer_identity_is_short_by_exactly_the_move`
//!   below measures the gap rather than asserting it from the doc.
//! - **a trade expedition** (arc #527) draws cargo off the sending band at launch and hands it to
//!   the destination on arrival.
//!
//! `transferReceived` / `transferSent` close both with **one pair of terms**, because they are one
//! fact — *food that crossed between bands outside income and consumption* — and the identity is now
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − penFeedUpkeep − raidForfeit
//!                 + transferReceived − transferSent
//! ```
//!
//! Asserted against **real turns through the real systems and the real exported snapshot**, the
//! shape `pen_food_ledger.rs` and `raid_food_ledger.rs` already use — never against a
//! re-derivation of the sim's own arithmetic.

use bevy::prelude::Entity;
use core_sim::{
    build_headless_app, run_turn, scalar_from_f32, split_band_from_parent, BandId, BandTravel,
    Expedition, ExpeditionMission, ExpeditionPhase, LaborAllocation, LocalStore, PopulationCohort,
    ResidentBand, Scalar, SettleConfig, SimulationConfig, SnapshotHistory, StartingUnit, Tile,
    TileRegistry, FOOD,
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
    pen_feed: f32,
    raid_forfeit: f32,
    received: f32,
    sent: f32,
}

impl Ledger {
    /// The identity's right-hand side, with the two new terms.
    fn expected_delta(&self) -> f32 {
        self.income - self.consumption - self.pen_feed - self.raid_forfeit + self.received
            - self.sent
    }

    /// The right-hand side **as it read before the transfer terms existed** — what a client
    /// computing the documented identity would have got.
    fn pre_transfer_delta(&self) -> f32 {
        self.income - self.consumption - self.pen_feed - self.raid_forfeit
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
        pen_feed: cohort.pen_feed_upkeep,
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
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();
    app
}

/// **Two co-located bands of the same faction** — which is exactly the fixture that forms a supply
/// network (`MIN_NETWORK_MEMBERS` is 2, and they are well inside `reach_tiles`). One opens fed, the
/// other empty, so the balancing pass has a large move to make.
fn two_networked_bands(app: &mut bevy::prelude::App) -> (Entity, Entity) {
    let parent = first_band(app);
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(parent)
            .expect("the band exists");
        cohort.working = Scalar::from_f32(PARENT_WORKERS);
        cohort.sync_size();
    }
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle)
        .expect("a stocked parent can split");
    let child = entity_for_band(app, split.band).expect("the split allocated this id");
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
