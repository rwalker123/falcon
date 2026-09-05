//! **Hay that crossed between two bands by PARTY has to appear in the fodder ledger's route arm.**
//!
//! `snapshot.fbs`'s `fodderTransferRoute{Received,Sent}Turn` shipped ahead of anything that could
//! fill them: a shipment's manifest took `food` and `material` lines only, so the arms read `0` on
//! every frame and their comment said so. Issue #590 gave hay a currency in a shipment — a `fodder
//! <amount>` line, drawn from the same store, priced at `trade.fodder_carry_weight` — and this file
//! is what pins the arms live.
//!
//! ⛔ **THE HAY IS BOOKED ON ITS OWN LEDGER AND NOWHERE IN THE FOOD ONE.** The food identity
//!
//! ```text
//! larder_delta == foodIncome − foodConsumption − raidForfeit + transferReceived − transferSent
//! ```
//!
//! closes over the **food larder**, which a bale never enters, so a delivery of hay that touched
//! `transferReceived` would break it by exactly the bale. Every test below asserts the hay arm moved
//! **and** that the food identity did not — the two halves of one claim, because an implementation
//! that booked hay as food would satisfy either one alone.
//!
//! The sibling file `transfer_food_ledger.rs` is deliberately **food-only** and unchanged.
//!
//! **Asserted off the ENCODED envelope**, through the accessor chain a client uses: a field that
//! never reached the codec still passes an in-process assertion.

use bevy::prelude::Entity;
use core_sim::{
    build_test_app, run_turn, scalar_from_f32, split_band_from_parent, BandId, BandTravel,
    Expedition, ExpeditionMission, ExpeditionPhase, LaborAllocation, LocalStore, PopulationCohort,
    ResidentBand, Scalar, SettleConfig, SimulationConfig, SnapshotHistory, StartingUnit, Tile,
    TileRegistry, FODDER, FOOD,
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
/// Workers a shipment party carries.
const PARTY_WORKERS: u32 = 2;
/// The hay one shipment carries. Inside `PARTY_WORKERS × trade.per_worker_carry` at any sane
/// `fodder_carry_weight`, and a number nothing else in the fixture could coincidentally produce.
const CARGO_FODDER: f32 = 7.0;
/// The hay the sending band opens with. Nothing eats it — the fixture keeps no pens — so every unit
/// that leaves the band crossed to somebody.
const SENDER_HAYLOFT: f32 = 240.0;
/// Food the sending band opens with, so its people are not starving while the test runs.
const SENDER_LARDER: f32 = 400.0;
/// The faction the *destination* belongs to. A different one, deliberately: it keeps the supply
/// network — which pools same-faction bands, hay included — out of a test about a shipment.
const FOREIGN_FACTION: core_sim::FactionId = core_sim::FactionId(9);
/// How far the undelivered fixture puts the destination from the sender — well past the comm range
/// a delivery and a fold-back are gated on, so the party spends real turns on the road.
const OUT_OF_REACH_TILES: u32 = 20;
/// Turns the party walks before its destination is erased, so the turn-for-home is a real leg rather
/// than an instant cancel in camp.
const TURNS_TO_GET_CLEAR: u32 = 6;
/// Turns to let the party walk home. Generous — the claim is that it *arrives*, not how fast.
const MAX_TURNS_HOME: u32 = 60;

// ---------------------------------------------------------------------------------------------
// Reading the wire
// ---------------------------------------------------------------------------------------------

/// One band's **route** arm on one account, plus the food ledger's own five terms, all off the
/// encoded envelope.
#[derive(Debug, Clone, Copy)]
struct Rows {
    /// `fodderTransferRouteReceivedTurn` — hay a party carried in.
    hay_route_received: f32,
    /// `fodderTransferRouteSentTurn` — hay a party carried away.
    hay_route_sent: f32,
    /// `fodderTransferLocalReceivedTurn` — hay the supply network pooled in. Read so the tests can
    /// state that a shipment books on the *route* arm and not on this one.
    hay_local_received: f32,
    food_income: f32,
    food_consumption: f32,
    raid_forfeit: f32,
    food_received: f32,
    food_sent: f32,
    /// `expeditionCargoFodder` — the hay an in-flight party is carrying, on its own wire field.
    cargo_fodder: f32,
}

impl Rows {
    /// The food identity's right-hand side. Hay appears nowhere in it, which is the point.
    fn food_delta(&self) -> f32 {
        self.food_income - self.food_consumption - self.raid_forfeit + self.food_received
            - self.food_sent
    }
}

fn rows_of(app: &bevy::prelude::App, band: BandId) -> Rows {
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
    Rows {
        hay_route_received: row.fodderTransferRouteReceivedTurn(),
        hay_route_sent: row.fodderTransferRouteSentTurn(),
        hay_local_received: row.fodderTransferLocalReceivedTurn(),
        food_income: row.foodIncome(),
        food_consumption: row.foodConsumption(),
        raid_forfeit: row.raidForfeit(),
        food_received: row.transferReceived(),
        food_sent: row.transferSent(),
        cargo_fodder: row.expeditionCargoFodder(),
    }
}

// ---------------------------------------------------------------------------------------------
// The fixture
// ---------------------------------------------------------------------------------------------

fn world() -> bevy::prelude::App {
    let mut app = build_test_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = SEED;
    app.update();
    app
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

fn band_id(app: &bevy::prelude::App, band: Entity) -> BandId {
    *app.world.get::<BandId>(band).expect("a band has an id")
}

fn hay(app: &bevy::prelude::App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .get(FODDER)
        .to_f32()
}

fn larder(app: &bevy::prelude::App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .get(FOOD)
        .to_f32()
}

fn position_of(app: &bevy::prelude::App, band: Entity) -> bevy::math::UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band")
        .current_tile;
    app.world.get::<Tile>(tile).expect("a real tile").position
}

/// **A sender and a FOREIGN destination**, the pair a shipment crosses between. The foreign faction
/// keeps `balance_supply_networks` — which pools hay on the *local* arm — out of a test about the
/// route arm, so a non-zero route figure below cannot be pooling under another name.
///
/// One published turn stands between the split and the measurement: the split books its own dowry
/// into the transfer counters, and `reset_transfer_ledger` clears them at the end of a turn, so what
/// these tests measure is the shipment alone.
fn a_sender_and_a_foreign_destination(app: &mut bevy::prelude::App) -> (Entity, Entity) {
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
    app.world
        .get_mut::<PopulationCohort>(child)
        .expect("the band exists")
        .faction = FOREIGN_FACTION;
    run_turn(app);
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(parent)
            .expect("the band exists");
        cohort.stores.set(FOOD, scalar_from_f32(SENDER_LARDER));
        cohort.stores.set(FODDER, scalar_from_f32(SENDER_HAYLOFT));
    }
    (parent, child)
}

/// Spawn a loaded trade party bound for `destination`, the way the launch command builds one.
///
/// The launch command itself lives in the server **binary**, which an integration test cannot link
/// against; its own half of the ledger (the sender's route *debit*) is pinned beside it, in
/// `core_sim/src/bin/server.rs`'s test module. What this file is about is what the **turn** writes:
/// the delivery and the homecoming.
///
/// The hay is taken out of the band's own store first, exactly as a launch draws it, so the round
/// trip below is a real round trip rather than hay conjured into a party's hands.
fn spawn_hay_shipment(
    app: &mut bevy::prelude::App,
    home: Entity,
    destination: BandId,
    destination_pos: bevy::math::UVec2,
    fodder: f32,
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
    let drawn = app
        .world
        .get_mut::<PopulationCohort>(home)
        .expect("the home band exists")
        .stores
        .take(FODDER, scalar_from_f32(fodder));
    assert!(
        (drawn.to_f32() - fodder).abs() < EPSILON,
        "the fixture must be able to draw the whole manifest, as a launch does: {drawn:?}"
    );
    let mut cargo = LocalStore::new();
    cargo.add(FODDER, drawn);
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

/// Walk `band` well out of comm range of `from`, so a party bound for it spends real turns on the
/// road and is genuinely in flight when its destination is erased.
fn walk_away(
    app: &mut bevy::prelude::App,
    band: Entity,
    from: bevy::math::UVec2,
) -> bevy::math::UVec2 {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let target = bevy::math::UVec2::new(
        (from.x + OUT_OF_REACH_TILES) % width.max(1),
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
    cohort.home = tile;
    cohort.current_tile = tile;
    target
}

// ---------------------------------------------------------------------------------------------
// The delivery
// ---------------------------------------------------------------------------------------------

/// ⛔ **A HAY SHIPMENT LANDS ON THE FODDER LEDGER'S ROUTE ARM, AND NOWHERE ON THE FOOD ONE.**
///
/// Three claims in one turn, because an implementation can satisfy any two of them alone:
///
/// 1. the destination's **hay store** rises by the shipment, and its `fodderTransferRouteReceived`
///    row states it — the arm that read `0` on every frame before issue #590;
/// 2. the arrival books on the **route** arm rather than the `local` one — a shipment is an event, a
///    pooling neighbour is a rate, and the runway counts only the second;
/// 3. the **food identity is untouched**: `transferReceived` does not see the bale, and the
///    destination's food ledger still reconciles across the same turn.
#[test]
fn a_hay_shipment_lands_on_the_fodder_route_arm_and_leaves_the_food_identity_alone() {
    let mut app = world();
    let (sender, host) = a_sender_and_a_foreign_destination(&mut app);
    let host_id = band_id(&app, host);

    // A loaded party standing in the destination's camp: it hands the cargo over on the next turn.
    let host_pos = position_of(&app, host);
    let party = spawn_hay_shipment(&mut app, sender, host_id, host_pos, CARGO_FODDER);
    let party_id = band_id(&app, party);

    let host_hay_before = hay(&app, host);
    let host_larder_before = larder(&app, host);
    run_turn(&mut app);

    let host_rows = rows_of(&app, host_id);
    // (1) the store moved, and the row says so.
    assert!(
        (hay(&app, host) - host_hay_before - CARGO_FODDER).abs() < EPSILON,
        "the destination's hay store rises by the shipment: {host_hay_before} -> {}",
        hay(&app, host)
    );
    assert!(
        (host_rows.hay_route_received - CARGO_FODDER).abs() < EPSILON,
        "and the fodder ROUTE arm states it, in full: {host_rows:?}"
    );
    assert_eq!(
        host_rows.hay_route_sent, 0.0,
        "a band that only received must not report sending — two named magnitudes, never one          signed net: {host_rows:?}"
    );
    // (2) on the route arm, not the local one.
    assert!(
        host_rows.hay_local_received < EPSILON,
        "a shipment is not pooling — the LOCAL arm must stay empty, or the runway would annualise a \
         one-off delivery: {host_rows:?}"
    );
    // (3) and the food ledger neither sees it nor breaks.
    assert!(
        host_rows.food_received < EPSILON,
        "the FOOD ledger must not book a bale: the identity it closes is over the food larder, \
         which hay never enters ({host_rows:?})"
    );
    let food_delta = larder(&app, host) - host_larder_before;
    assert!(
        (food_delta - host_rows.food_delta()).abs() < EPSILON,
        "and the food identity still reconciles across the hay delivery: delta={food_delta} vs {} \
         ({host_rows:?})",
        host_rows.food_delta()
    );
    assert!(
        host_rows.food_consumption > 0.0,
        "the liveness half of that identity — a band that ate nothing makes it vacuous"
    );
    // The party is empty and gone from the wire's cargo field, having delivered once.
    let _ = party_id;
    let leftover = {
        let mut query = app.world.query::<&Expedition>();
        query
            .iter(&app.world)
            .next()
            .map(|expedition| expedition.cargo.get(FODDER).to_f32())
    };
    assert_eq!(
        leftover,
        Some(0.0),
        "a delivered shipment leaves the party's hay account empty"
    );
}

/// **The hay a party is carrying is legible on the wire while it is still in flight** —
/// `expeditionCargoFodder`, the third cargo account, beside the food one it must never be summed
/// into.
#[test]
fn an_in_flight_partys_hay_is_published_on_its_own_cargo_field() {
    let mut app = world();
    let (sender, host) = a_sender_and_a_foreign_destination(&mut app);
    let host_id = band_id(&app, host);
    // Out of reach, so the party is still holding the bales when the frame is published.
    let sender_pos = position_of(&app, sender);
    let far = walk_away(&mut app, host, sender_pos);
    let party = spawn_hay_shipment(&mut app, sender, host_id, far, CARGO_FODDER);
    let party_id = band_id(&app, party);

    run_turn(&mut app);

    let rows = rows_of(&app, party_id);
    assert!(
        (rows.cargo_fodder - CARGO_FODDER).abs() < EPSILON,
        "the in-flight party publishes the hay it is carrying: {rows:?}"
    );
    // The negative control: a resident band is not carrying a shipment, and says so with a zero.
    let sender_rows = rows_of(&app, band_id(&app, sender));
    assert_eq!(
        sender_rows.cargo_fodder, 0.0,
        "a band's own hayloft is not a shipment: {sender_rows:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The homecoming
// ---------------------------------------------------------------------------------------------

/// ⛔ **AN UNDELIVERED SHIPMENT'S HAY COMES HOME AND IS CREDITED BACK**, rather than being destroyed
/// on the party's return.
///
/// `fold_party_into_band` settled food and material batches only before issue #590, so a party that
/// turned back holding bales quietly lost them — the one thing that routine's own comment says a
/// homecoming must never do. And the ledger half matters as much as the store half: the launch books
/// a route **debit** against the sender, so a homecoming that moved the hay without crediting it
/// would leave a permanent phantom *sent-but-never-received* figure standing in the account.
///
/// The food identity is asserted across the same turn, for the delivery test's reason.
#[test]
fn an_undelivered_shipments_hay_comes_home_and_is_credited_back() {
    let mut app = world();
    let (sender, host) = a_sender_and_a_foreign_destination(&mut app);
    let host_id = band_id(&app, host);
    let sender_id = band_id(&app, sender);
    let sender_pos = position_of(&app, sender);
    let far = walk_away(&mut app, host, sender_pos);
    let party = spawn_hay_shipment(&mut app, sender, host_id, far, CARGO_FODDER);
    let hay_at_sea = hay(&app, sender);

    // Let it get clear of home, so the turn-for-home below is a real leg.
    for _ in 0..TURNS_TO_GET_CLEAR {
        run_turn(&mut app);
    }
    assert_eq!(
        app.world
            .get::<Expedition>(party)
            .map(|expedition| expedition.phase),
        Some(ExpeditionPhase::Outbound),
        "the party is walking before its destination goes"
    );
    assert!(
        (hay(&app, sender) - hay_at_sea).abs() < EPSILON,
        "liveness: nothing else credits this band's hay while the party is out, so the equality \
         below is a statement about the homecoming alone"
    );

    // The destination is erased under the party: it turns for home still holding the bales.
    app.world.despawn(host);
    let mut credited = 0.0;
    let mut food_reconciled = false;
    for _ in 0..MAX_TURNS_HOME {
        if app.world.get::<Expedition>(party).is_none() {
            break;
        }
        let larder_before = larder(&app, sender);
        run_turn(&mut app);
        let rows = rows_of(&app, sender_id);
        credited += rows.hay_route_received;
        // The food identity holds on every turn of the walk home, including the one the fold-back
        // lands on.
        let delta = larder(&app, sender) - larder_before;
        if (delta - rows.food_delta()).abs() >= EPSILON {
            panic!(
                "the sender's FOOD identity broke on the way home: delta={delta} vs {} ({rows:?})",
                rows.food_delta()
            );
        }
        food_reconciled = true;
    }
    assert!(
        app.world.get::<Expedition>(party).is_none(),
        "the party folds back rather than walking forever"
    );
    assert!(
        food_reconciled,
        "liveness: at least one turn of the walk home was actually measured"
    );

    assert!(
        (hay(&app, sender) - hay_at_sea - CARGO_FODDER).abs() < EPSILON,
        "the undelivered HAY comes home whole rather than being destroyed: the store was \
         {hay_at_sea} while the party was out and is {} now, against a {CARGO_FODDER} shipment",
        hay(&app, sender)
    );
    assert!(
        (credited - CARGO_FODDER).abs() < EPSILON,
        "and the homecoming is CREDITED on the fodder route arm, so the launch's debit is answered \
         rather than left standing as a phantom: {credited} vs {CARGO_FODDER}"
    );
}
