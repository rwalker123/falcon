//! Supply network integration: bands within `reach_tiles` that hold a **live connection** balance
//! their stores, so a fed band feeds an empty neighbour — but a band beyond reach, or one nobody
//! has met, is on its own. World setup mirrors `sedentarization.rs`.
//!
//! **Every fixture here seeds its own ties.** The pooling gate is `ConnectionLedger`, and these
//! tests run `balance_supply_networks` on its own rather than a whole turn, so no sight sweep ever
//! runs to record a contact for them.

use std::sync::atomic::{AtomicU64, Ordering};

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::{
    balance_supply_networks, scalar_from_f32, scalar_zero, spawn_initial_world, BandId, BandKey,
    ConnectionKey, ConnectionLedger, ConnectionsConfig, CultureManager, DiscoveryProgressLedger,
    FactionId, FactionInventory, GenerationId, GenerationRegistry, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, Scalar, SimulationConfig,
    SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, SupplyNetworkConfigHandle,
    SupplyNetworkMembership, Tile, TileRegistry, FOOD, FULL_TIE, NO_TIE,
};

/// The faction the test bands belong to. **It no longer isolates them** — faction is a property of
/// the endpoint and never a branch in the balancer — what keeps them off the spawned starting bands
/// is that no tie is ever seeded to one.
const TEST_FACTION: FactionId = FactionId(7);
const BAND_POP: u32 = 100;

/// Test bands own ids well clear of anything `spawn_initial_world` allocates, so a seeded tie can
/// never name a starting band by accident.
const TEST_BAND_ID_BASE: u64 = 9_000;
static NEXT_TEST_BAND_ID: AtomicU64 = AtomicU64::new(TEST_BAND_ID_BASE);

/// The material the per-rating half of the pin is measured in — a shipped `materials.json` id with
/// two axes, exactly as `tests/materials.rs` uses it.
const HIDE: &str = "hide";
const TOUGHNESS: &str = "toughness";
const SUPPLENESS: &str = "suppleness";

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));
    app.world
        .insert_resource(SupplyNetworkConfigHandle::default());
    app.world
        .insert_resource(SupplyNetworkMembership::default());
    // The pooling gate. Empty until a fixture seeds a tie — which is the shipped turn-1 state.
    app.world.insert_resource(ConnectionLedger::default());

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();
    app
}

/// Spawn a test band of `BAND_POP` working-age people on the tile at `(x, y)` carrying `food`.
fn spawn_band(app: &mut App, x: u32, y: u32, food: i64) -> Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(x, y)
        .expect("tile coords resolve");
    let mut stores = LocalStore::new();
    stores.set(FOOD, Scalar::from_i64(food));
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: BAND_POP,
                children: scalar_zero(),
                working: Scalar::from_u32(BAND_POP),
                elders: scalar_zero(),
                stores,
                morale: scalar_zero(),
                last_food_consumption: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: TEST_FACTION,
                knowledge: Vec::new(),
                migration: None,
            },
            // Real bands carry `ResidentBand`; the supply network filters `With<ResidentBand>`.
            ResidentBand,
            // **And a `BandId`** — a node with no id has no identity to tie, so it can never be an
            // endpoint of a logistics link and never joins a network.
            BandId(NEXT_TEST_BAND_ID.fetch_add(1, Ordering::Relaxed)),
        ))
        .id()
}

/// Add `amount` of `HIDE` at `rating` to a band's store, reading `(tough, supple)`.
fn stock_hide(app: &mut App, band: Entity, rating: &BandKey, amount: f32, tough: f32, supple: f32) {
    let readings = std::collections::BTreeMap::from([
        (TOUGHNESS.to_string(), tough),
        (SUPPLENESS.to_string(), supple),
    ]);
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .deposit_material(HIDE, rating.clone(), scalar_from_f32(amount), &readings);
}

fn band_id(app: &App, band: Entity) -> BandId {
    *app.world
        .get::<BandId>(band)
        .expect("a test band has an id")
}

fn position_of(app: &App, band: Entity) -> UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .current_tile;
    app.world.get::<Tile>(tile).expect("a real tile").position
}

/// Record `observer` finding `subject` until the tie saturates at [`FULL_TIE`].
///
/// `record_contact` is the ledger's only way in, so the fixture climbs the tie the way a band that
/// stands beside another for a few turns does.
fn seed_directed_tie(app: &mut App, observer: Entity, subject: Entity) {
    const SEEDED_ON_TURN: u64 = 0;
    let key = ConnectionKey::new(band_id(app, observer), band_id(app, subject));
    let position = position_of(app, subject);
    let cfg = ConnectionsConfig::default();
    let contacts_to_full = (FULL_TIE.to_f32() / cfg.strength.gain_per_contact).ceil() as u32;
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for _ in 0..contacts_to_full {
        ledger.record_contact(key, position, SEEDED_ON_TURN, SEEDED_ON_TURN, &cfg);
    }
}

/// A live tie in **both** directions — two bands that have been standing in sight of each other.
fn seed_mutual_tie(app: &mut App, a: Entity, b: Entity) {
    seed_directed_tie(app, a, b);
    seed_directed_tie(app, b, a);
}

fn food_of(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0)
}

fn hide_of(app: &App, band: Entity, rating: &BandKey) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .and_then(|c| {
            c.stores
                .material_batches(HIDE)
                .find(|(key, _)| *key == rating)
                .map(|(_, batch)| batch.amount.to_f32())
        })
        .unwrap_or(0.0)
}

/// Two connected bands two tiles apart (within the default reach of 3) equalize their food.
#[test]
fn nearby_bands_share_food() {
    let mut app = spawn_world();
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let (cx, cy) = (w / 4, h / 2);
    let fed = spawn_band(&mut app, cx, cy, 1_000);
    let empty = spawn_band(&mut app, cx + 2, cy, 0);
    seed_mutual_tie(&mut app, fed, empty);

    app.world.run_system_once(balance_supply_networks);

    assert!(
        food_of(&app, empty) > 0.0,
        "an empty band near a fed one should receive food, got {}",
        food_of(&app, empty)
    );
    assert!(
        food_of(&app, fed) < 1_000.0,
        "the fed band should have shipped some of its surplus"
    );

    // Both bands land in the same multi-band supply network (a shared, non-zero id).
    let membership = app.world.resource::<SupplyNetworkMembership>();
    let fed_net = membership.network_of(fed);
    assert!(fed_net >= 1, "networked band should have a non-zero id");
    assert_eq!(
        fed_net,
        membership.network_of(empty),
        "bands that share food should carry the same supply-network id"
    );
}

/// Two connected bands straddling the horizontal wrap seam (wrapped distance 2 ≤ reach 3) still
/// share — guards the spatial-hash wrap folding (a band at the map's right edge networks with one
/// just past the left edge).
#[test]
fn seam_bands_share_food() {
    let mut app = spawn_world();
    app.world
        .resource_mut::<SimulationConfig>()
        .map_topology
        .wrap_horizontal = true;
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let cy = h / 2;
    let fed = spawn_band(&mut app, w - 1, cy, 1_000);
    let empty = spawn_band(&mut app, 1, cy, 0);
    seed_mutual_tie(&mut app, fed, empty);

    app.world.run_system_once(balance_supply_networks);

    assert!(
        food_of(&app, empty) > 0.0,
        "a band just past the seam should receive food, got {}",
        food_of(&app, empty)
    );
    assert!(
        food_of(&app, fed) < 1_000.0,
        "the fed band across the seam should have shipped some of its surplus"
    );
}

/// Beyond-reach control across the seam: wrapped distance 6 > reach 3, so nothing is shared even
/// with wrap on and a full tie between them.
#[test]
fn seam_bands_beyond_reach_do_not_share() {
    let mut app = spawn_world();
    app.world
        .resource_mut::<SimulationConfig>()
        .map_topology
        .wrap_horizontal = true;
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let cy = h / 2;
    let fed = spawn_band(&mut app, w - 1, cy, 1_000);
    let empty = spawn_band(&mut app, 5, cy, 0);
    seed_mutual_tie(&mut app, fed, empty);

    app.world.run_system_once(balance_supply_networks);

    assert_eq!(
        food_of(&app, empty),
        0.0,
        "a band beyond reach across the seam should receive nothing"
    );
    assert_eq!(
        food_of(&app, fed),
        1_000.0,
        "the fed band keeps all its food when no one is in reach across the seam"
    );
}

/// A band ten tiles away shares nothing even with a full tie — **distance still gates**, which is
/// the whole of a distant splinter needing a shipment rather than a free pool.
#[test]
fn distant_bands_do_not_share() {
    let mut app = spawn_world();
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let (cx, cy) = (w / 4, h / 2);
    let fed = spawn_band(&mut app, cx, cy, 1_000);
    let empty = spawn_band(&mut app, cx + 10, cy, 0);
    seed_mutual_tie(&mut app, fed, empty);

    app.world.run_system_once(balance_supply_networks);

    assert_eq!(
        food_of(&app, empty),
        0.0,
        "a band beyond reach should receive nothing"
    );
    assert_eq!(
        food_of(&app, fed),
        1_000.0,
        "the fed band keeps all its food when no one is in reach"
    );

    // Isolated bands are singletons — no shared network, so both read 0.
    let membership = app.world.resource::<SupplyNetworkMembership>();
    assert_eq!(
        membership.network_of(fed),
        0,
        "a band beyond reach is not in a multi-band network"
    );
    assert_eq!(membership.network_of(empty), 0);
}

/// **Within reach is not enough — the tie is the gate.** Two bands standing two tiles apart that
/// have never met pool nothing, which is what makes the ledger load-bearing rather than decorative.
#[test]
fn bands_within_reach_with_no_tie_do_not_share() {
    let mut app = spawn_world();
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let (cx, cy) = (w / 4, h / 2);
    let fed = spawn_band(&mut app, cx, cy, 1_000);
    let empty = spawn_band(&mut app, cx + 2, cy, 0);
    // Deliberately no `seed_mutual_tie` — this is the world's first turn, before any sight sweep.

    app.world.run_system_once(balance_supply_networks);

    assert_eq!(
        food_of(&app, empty),
        0.0,
        "strangers within reach must pool nothing"
    );
    assert_eq!(food_of(&app, fed), 1_000.0, "and the fed band keeps it all");
    let membership = app.world.resource::<SupplyNetworkMembership>();
    assert_eq!(membership.network_of(fed), 0);
    assert_eq!(membership.network_of(empty), 0);
}

/// **A parked tie does not pool.** The keystone's *"at zero nothing flows"*: an edge drained to
/// [`NO_TIE`] is still in the ledger — we know such a people exist — and it moves nothing.
#[test]
fn a_parked_tie_does_not_share() {
    let mut app = spawn_world();
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let (cx, cy) = (w / 4, h / 2);
    let fed = spawn_band(&mut app, cx, cy, 1_000);
    let empty = spawn_band(&mut app, cx + 2, cy, 0);
    seed_mutual_tie(&mut app, fed, empty);

    // Drain both edges to zero: quiet turns, but well inside `forget_turns` so the edges survive.
    let cfg = ConnectionsConfig::default();
    let quiet_turns = (FULL_TIE.to_f32() / cfg.strength.decay_per_turn).ceil() as u64;
    assert!(
        quiet_turns < cfg.forget_turns,
        "the fixture must park the edges, not reap them"
    );
    let key = ConnectionKey::new(band_id(&app, fed), band_id(&app, empty));
    {
        let mut ledger = app.world.resource_mut::<ConnectionLedger>();
        for turn in 1..=quiet_turns {
            ledger.decay_all(turn, &cfg);
        }
        assert_eq!(
            ledger.get(&key).expect("a drained edge parks").strength,
            NO_TIE,
            "the fixture must actually reach zero"
        );
    }

    app.world.run_system_once(balance_supply_networks);

    assert_eq!(
        food_of(&app, empty),
        0.0,
        "a parked tie moves nothing — at zero nothing flows"
    );
    assert_eq!(food_of(&app, fed), 1_000.0);
}

/// **One live direction is enough.** Pooling is one undirected mechanism; requiring both edges
/// would make the commonest traffic in the game wait on two independent sight sweeps agreeing.
#[test]
fn one_live_direction_is_enough_to_share() {
    let mut app = spawn_world();
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let (cx, cy) = (w / 4, h / 2);
    let fed = spawn_band(&mut app, cx, cy, 1_000);
    let empty = spawn_band(&mut app, cx + 2, cy, 0);
    // Only the fed band has seen the other — the scout-on-the-ridge case, one edge deep.
    seed_directed_tie(&mut app, fed, empty);
    assert!(
        app.world
            .resource::<ConnectionLedger>()
            .get(&ConnectionKey::new(
                band_id(&app, empty),
                band_id(&app, fed)
            ))
            .is_none(),
        "the reverse edge must genuinely be absent"
    );

    app.world.run_system_once(balance_supply_networks);

    assert!(
        food_of(&app, empty) > 0.0,
        "one live direction must pool, got {}",
        food_of(&app, empty)
    );
    assert!(food_of(&app, fed) < 1_000.0);
}

/// **The no-regression pin.** Three connected bands with unequal opening food *and* two ratings of
/// one material: the post-transfer stores must match, to the last unit, what the proximity-derived
/// balancer produced before the network was re-founded on the connection primitive.
///
/// The literals below are the **pre-refactor values**, captured by running this fixture against the
/// implicit-edge implementation. Nothing about the balancing math, the per-rating pass or the
/// throughput/friction/dead-band levers changed — only which pairs are edges — so a drift here is a
/// regression, not a re-tune.
///
/// **Paired with a liveness assertion**, because "the two runs agree" is also what a mechanism that
/// has gone entirely dead reports.
#[test]
fn a_connected_network_moves_exactly_what_the_proximity_network_moved() {
    /// The bands' opening larders. Unequal by design: the balancer has a large move to make.
    const RICH_FOOD: i64 = 1_200;
    const MIDDLING_FOOD: i64 = 300;
    const EMPTY_FOOD: i64 = 0;
    /// And their opening hide piles, across two different ratings of the one material.
    const RICH_HIDE: f32 = 90.0;
    const MIDDLING_HIDE: f32 = 10.0;
    const OTHER_RATING_HIDE: f32 = 40.0;
    /// `f32` sums of `Scalar`-quantised amounts; a few ULPs of slack, no more.
    const EPSILON: f32 = 1e-3;

    let mut app = spawn_world();
    let (w, h) = {
        let reg = app.world.resource::<TileRegistry>();
        (reg.width, reg.height)
    };
    let (cx, cy) = (w / 4, h / 2);
    let rich = spawn_band(&mut app, cx, cy, RICH_FOOD);
    let middling = spawn_band(&mut app, cx + 2, cy, MIDDLING_FOOD);
    let poor = spawn_band(&mut app, cx + 1, cy + 1, EMPTY_FOOD);

    // Two ratings — a stiff hide and a supple one — so the per-rating pass is exercised and a
    // scalar pool would show up immediately.
    let stiff = BandKey(vec![3, 0]);
    let supple = BandKey(vec![0, 3]);
    stock_hide(&mut app, rich, &stiff, RICH_HIDE, 0.92, 0.10);
    stock_hide(&mut app, middling, &stiff, MIDDLING_HIDE, 0.92, 0.10);
    stock_hide(&mut app, poor, &supple, OTHER_RATING_HIDE, 0.14, 0.92);

    // A chain of ties, not a clique: the union-find is what makes the third band a member.
    seed_mutual_tie(&mut app, rich, middling);
    seed_mutual_tie(&mut app, middling, poor);

    app.world.run_system_once(balance_supply_networks);

    // Per commodity: the rich band ships one throughput's worth, and the two short bands split the
    // arrivals in proportion to how short each is, less friction.
    let expected_food = [(rich, 1_150.0), (middling, 323.750_03), (poor, 23.75)];
    for (band, expected) in expected_food {
        let held = food_of(&app, band);
        assert!(
            (held - expected).abs() < EPSILON,
            "food must match the pre-refactor value: got {held}, expected {expected}"
        );
    }

    // Per material rating, each balanced as its own commodity — the stiff pile pools one way, the
    // supple pile the other, and neither is ever averaged into the other.
    let expected_stiff = [(rich, 40.0), (middling, 29.558_79), (poor, 27.941_162)];
    for (band, expected) in expected_stiff {
        let held = hide_of(&app, band, &stiff);
        assert!(
            (held - expected).abs() < EPSILON,
            "the stiff rating must match the pre-refactor value: got {held}, expected {expected}"
        );
    }
    let expected_supple = [
        (rich, 12.666_673),
        (middling, 12.666_673),
        (poor, 13.333_32),
    ];
    for (band, expected) in expected_supple {
        let held = hide_of(&app, band, &supple);
        assert!(
            (held - expected).abs() < EPSILON,
            "the supple rating must match the pre-refactor value: got {held}, expected {expected}"
        );
    }

    // **Liveness.** Every assertion above also passes on a balancer that has stopped balancing and
    // a fixture whose baseline was captured from the same dead code, so the magnitudes have to be
    // asserted non-zero in their own right.
    assert!(
        (food_of(&app, rich) - RICH_FOOD as f32).abs() > EPSILON
            && food_of(&app, poor) > EPSILON
            && hide_of(&app, poor, &stiff) > EPSILON
            && hide_of(&app, rich, &supple) > EPSILON,
        "the pin must sit on a network that genuinely moved food AND both hide ratings"
    );

    // And all three bands are one network — the chain of ties, unioned.
    let membership = app.world.resource::<SupplyNetworkMembership>();
    let network = membership.network_of(rich);
    assert!(network >= 1);
    assert_eq!(network, membership.network_of(middling));
    assert_eq!(
        network,
        membership.network_of(poor),
        "a chain of ties makes one network, not two pairs"
    );
}
