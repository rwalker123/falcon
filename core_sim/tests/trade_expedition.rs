//! **A shipment is a party that walks it** — the connection primitive's first rider (arc #527,
//! issue #517, `docs/plan_contact_and_logistics.md` §Q5).
//!
//! The launch **gates** (the tie, the carry cap, the fail-closed manifest) are tested where the
//! command handler lives, in `core_sim/src/bin/server.rs`'s test module, beside the sibling raiding
//! verbs' gates. What is pinned here is everything the gates cannot see: that a shipment driven
//! through real turns actually **lands in the destination band's store**, that its material ratings
//! survive the trip unaveraged, that the cargo survives a rollback, and that a destination which
//! disappears mid-trip sends the party home carrying its goods rather than hanging.
//!
//! **Wire claims are asserted on the encoded envelope**, through the accessor chain a client uses —
//! a field that never reached the codec still passes an in-process assertion, and nothing consumes
//! these fields yet to notice.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::{
    build_headless_app, run_turn, scalar_from_f32, scalar_zero, split_band_from_parent, BandId,
    BandKey, BandTravel, Expedition, ExpeditionMission, ExpeditionPhase, LaborAllocation,
    LocalStore, PopulationCohort, ResidentBand, Scalar, SettleConfig, SimulationConfig,
    SnapshotHistory, StartingUnit, Tile, TileRegistry, ViewerFaction, FOOD,
};

/// A pinned earthlike world, so the terrain under every fixture is the same one every run.
const MAP_SEED: u64 = 119_304_647;
/// Working-age people the parent is stocked with, so a split leaves two real bands on any seed.
const PARENT_WORKERS: f32 = 20.0;
/// Workers the second band is founded with.
const SPLIT_WORKERS: u32 = 5;
/// Workers the shipment party carries. Two is enough pack for every manifest below.
const PARTY_WORKERS: u32 = 2;
/// The food a shipment carries. Well inside `PARTY_WORKERS × trade.per_worker_carry`.
const CARGO_FOOD: f32 = 8.0;
/// Turns to drive a co-located delivery. One is enough — the party stands in the destination's camp
/// already — and a couple more prove it does not deliver twice.
const DELIVERY_TURNS: u32 = 3;
/// `f32` sums of `Scalar`-quantised amounts; a few ULPs of slack, no more.
const EPSILON: f32 = 0.01;
/// How far the in-flight fixtures put the destination from the sender. Well past the comm range a
/// delivery is gated on (2), so the party spends real turns on the road.
const OUT_OF_REACH_TILES: u32 = 20;

/// The material a shipment carries, and the two **different ratings** of it the unaveraged
/// guarantee turns on. `hide` is a shipped `materials.json` id with two axes.
const HIDE: &str = "hide";
/// Two band keys that are genuinely different piles — a fine hide and a poor one. The store keys
/// batches by band, so two keys is two batches by construction.
const FINE_HIDE: [usize; 2] = [4, 4];
const POOR_HIDE: [usize; 2] = [1, 1];
const FINE_AMOUNT: f32 = 3.0;
const POOR_AMOUNT: f32 = 2.0;

fn spawn_world() -> App {
    let mut app = build_headless_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The campaign's first resident band.
fn first_band(app: &mut App) -> Entity {
    let mut query = app
        .world
        .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
    query
        .iter(&app.world)
        .next()
        .expect("the campaign spawns at least one resident band")
}

fn entity_for_band(app: &mut App, band_id: BandId) -> Option<Entity> {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == band_id)
        .map(|(entity, _)| entity)
}

fn stock(app: &mut App, band: Entity) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists");
    cohort.working = Scalar::from_f32(PARENT_WORKERS);
    cohort.sync_size();
}

/// Split a second resident band off `parent`. A split is co-located, which is the fixture a
/// shipment wants: the party arrives the turn it launches.
fn split_off(app: &mut App, parent: Entity) -> (Entity, BandId) {
    stock(app, parent);
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle)
        .expect("a stocked parent can split");
    let entity = entity_for_band(app, split.band).expect("the split allocated this id");
    (entity, split.band)
}

/// **Make the destination band another PEOPLE.** Two things at once, both deliberate:
///
/// - It is the arc's central claim under test. Faction is a property of the endpoint, never a
///   branch, so a shipment to strangers must work **by construction** — every delivery assertion in
///   this file is therefore a cross-faction delivery, and that is what makes #458 nearly free.
/// - It takes `balance_supply_networks` out of the picture. That system pools **same-faction** bands
///   within reach, so a shipment landing in a co-located band of one's own faction is immediately
///   re-equalised back toward the sender: the first cut of this test watched 3.0 hide arrive and
///   read 0.75 a moment later. The shipment is not what changed — the pooling is a separate and
///   correct mechanism — but it makes *"did the shipment land here"* unanswerable.
const FOREIGN_FACTION: core_sim::FactionId = core_sim::FactionId(9);

fn make_foreign(app: &mut App, band: Entity) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists")
        .faction = FOREIGN_FACTION;
}

/// Walk `band` well out of the sender's reach, so a shipment aimed at it is genuinely **in flight**
/// for the turns a test drives. A co-located destination is delivered to on the launch turn, which
/// is the right behaviour and the wrong fixture for anything about a party on the road.
fn walk_away(app: &mut App, band: Entity, from: UVec2) -> UVec2 {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let target = UVec2::new(
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
    cohort.current_tile = tile;
    cohort.home = tile;
    target
}

fn band_position(app: &App, band: Entity) -> UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .current_tile;
    app.world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position
}

fn band_food(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .get(FOOD)
        .to_f32()
}

/// **Spawn a shipment party by hand**, the way the launch command builds one: a detached cohort
/// with the `Trade` mission, its cargo in a store of its own, and a travel order for the
/// destination's tile.
///
/// The command handler's own path (the tie gate, the draw, the cap) is tested in `bin/server.rs`;
/// this file is about what happens once a party is on the map, so the fixture states the party
/// directly rather than routing through a binary these tests cannot link against.
fn launch_shipment(
    app: &mut App,
    home: Entity,
    destination: BandId,
    destination_pos: UVec2,
    cargo: LocalStore,
) -> Entity {
    let mut cohort = app
        .world
        .get::<PopulationCohort>(home)
        .expect("the home band exists")
        .clone();
    cohort.working = Scalar::from_u32(PARTY_WORKERS);
    cohort.children = scalar_zero();
    cohort.elders = scalar_zero();
    cohort.stores = LocalStore::new();
    cohort.sync_size();
    let band_id = app
        .world
        .resource_mut::<core_sim::BandIdAllocator>()
        .allocate();
    app.world
        .spawn((
            cohort,
            band_id,
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

fn food_cargo(amount: f32) -> LocalStore {
    let mut store = LocalStore::new();
    store.add(FOOD, scalar_from_f32(amount));
    store
}

fn readings(a: f32, b: f32) -> std::collections::BTreeMap<String, f32> {
    let mut map = std::collections::BTreeMap::new();
    map.insert("toughness".to_string(), a);
    map.insert("suppleness".to_string(), b);
    map
}

/// A shipment of **two different ratings of one material** — the pile the unaveraged guarantee is
/// about.
fn two_rating_cargo() -> LocalStore {
    let mut store = LocalStore::new();
    store.deposit_material(
        HIDE,
        BandKey(FINE_HIDE.to_vec()),
        scalar_from_f32(FINE_AMOUNT),
        &readings(0.9, 0.9),
    );
    store.deposit_material(
        HIDE,
        BandKey(POOR_HIDE.to_vec()),
        scalar_from_f32(POOR_AMOUNT),
        &readings(0.1, 0.1),
    );
    store
}

/// **Every cohort's `(bandId, carry cap, trade per-worker carry, material carry weight)` off the
/// encoded envelope.** The pack fields, read through the accessor chain a client uses — a field that
/// never reached the codec still passes an in-process assertion.
fn published_packs(app: &App) -> Vec<(u64, f32, f32, f32)> {
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
    envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published")
        .iter()
        .map(|cohort| {
            (
                cohort.bandId(),
                cohort.expeditionCarryCap(),
                cohort.expeditionTradePerWorkerCarry(),
                cohort.expeditionTradeMaterialCarryWeight(),
            )
        })
        .collect()
}

/// The cohort row for `band_id` off the **encoded envelope**, as a client reads it:
/// `(destination band, destination name, cargo food, [(material, amount)])`.
fn published_cargo(app: &App, band_id: BandId) -> (u64, String, f32, Vec<(String, f32)>) {
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
    let populations = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published");
    let row = populations
        .iter()
        .find(|cohort| cohort.bandId() == band_id.0)
        .expect("the party's own row is published");
    let materials = row
        .expeditionCargoMaterials()
        .map(|rows| {
            rows.iter()
                .map(|payoff| {
                    (
                        payoff.materialId().unwrap_or("").to_string(),
                        payoff.amount(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    (
        row.expeditionDestinationBand(),
        row.expeditionDestinationName().unwrap_or("").to_string(),
        row.expeditionCargoFood(),
        materials,
    )
}

// ---------------------------------------------------------------------------------------------
// A shipment lands in the DESTINATION band's store
// ---------------------------------------------------------------------------------------------

/// **The rider's whole point.** A shipment walked to another people ends up in *that* band's larder,
/// and the delivery is exact once the one other thing that moves a larder — the people's own meal —
/// is accounted for.
///
/// **The destination is ANOTHER FACTION** ([`make_foreign`]), which is both the arc's claim under
/// test and what makes the assertion answerable at all.
#[test]
fn a_shipment_lands_in_the_destination_bands_store() {
    let mut app = spawn_world();
    let sender = first_band(&mut app);
    let (destination, destination_id) = split_off(&mut app, sender);
    make_foreign(&mut app, destination);
    let destination_pos = band_position(&app, destination);

    let host_before = band_food(&app, destination);
    launch_shipment(
        &mut app,
        sender,
        destination_id,
        destination_pos,
        food_cargo(CARGO_FOOD),
    );

    // One turn: the party is standing in their camp already, so it hands the cargo over at once.
    run_turn(&mut app);

    let host_after = band_food(&app, destination);
    let ate = app
        .world
        .get::<PopulationCohort>(destination)
        .expect("the band")
        .last_food_consumption;
    assert!(
        ate > 0.0,
        "the liveness half of the arithmetic below — a band that ate nothing makes the identity \
         vacuous"
    );
    assert!(
        ((host_after - host_before) - (CARGO_FOOD - ate)).abs() < EPSILON,
        "the destination's larder moves by exactly the shipment less its own meal: \
         {host_before} -> {host_after}, cargo {CARGO_FOOD}, ate {ate}"
    );

    // The party is empty afterwards, and does not deliver a second time on the turns it spends
    // walking home.
    let party_cargo = {
        let mut query = app.world.query::<&Expedition>();
        query
            .iter(&app.world)
            .next()
            .map(|expedition| expedition.cargo.get(FOOD).to_f32())
    };
    assert_eq!(
        party_cargo,
        Some(0.0),
        "a delivered shipment leaves the party's cargo empty — one-way, one delivery"
    );
    let after_one = band_food(&app, destination);
    run_turn(&mut app);
    let after_two = band_food(&app, destination);
    assert!(
        after_two < after_one + EPSILON,
        "the shipment lands ONCE: {after_one} -> {after_two}"
    );
}

/// **Two ratings of one material stay two batches.** A shipment carries goods, and a rating is what
/// makes them goods rather than a number — so a mammoth hide is never averaged into a hare pelt by
/// being carried somewhere.
///
/// Asserted against the receiving band's own store, which is the only place the merge could have
/// happened.
#[test]
fn two_ratings_of_one_material_arrive_as_two_batches() {
    let mut app = spawn_world();
    let sender = first_band(&mut app);
    let (destination, destination_id) = split_off(&mut app, sender);
    make_foreign(&mut app, destination);
    let destination_pos = band_position(&app, destination);

    // The receiving band holds none of it, so every batch found afterwards came off the shipment.
    assert_eq!(
        app.world
            .get::<PopulationCohort>(destination)
            .expect("the band")
            .stores
            .material_total(HIDE),
        scalar_zero(),
        "the fixture starts with no hide at the destination"
    );

    launch_shipment(
        &mut app,
        sender,
        destination_id,
        destination_pos,
        two_rating_cargo(),
    );
    for _ in 0..DELIVERY_TURNS {
        run_turn(&mut app);
    }

    let store = &app
        .world
        .get::<PopulationCohort>(destination)
        .expect("the band")
        .stores;
    let batches: Vec<(BandKey, f32, f32)> = store
        .material_batches(HIDE)
        .map(|(key, batch)| {
            (
                key.clone(),
                batch.amount.to_f32(),
                batch.characteristics["toughness"],
            )
        })
        .collect();
    assert_eq!(
        batches.len(),
        2,
        "the two ratings must arrive as TWO batches, not one averaged pile: {batches:?}"
    );
    let fine = batches
        .iter()
        .find(|(key, _, _)| key == &BandKey(FINE_HIDE.to_vec()))
        .expect("the fine hide arrived at its own rating");
    let poor = batches
        .iter()
        .find(|(key, _, _)| key == &BandKey(POOR_HIDE.to_vec()))
        .expect("the poor hide arrived at its own rating");
    assert!(
        (fine.1 - FINE_AMOUNT).abs() < EPSILON && (poor.1 - POOR_AMOUNT).abs() < EPSILON,
        "each batch arrives with its own amount: {batches:?}"
    );
    assert!(
        (fine.2 - 0.9).abs() < 1e-4 && (poor.2 - 0.1).abs() < 1e-4,
        "and its own EXACT reading, not a blend: {batches:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The cargo is real state
// ---------------------------------------------------------------------------------------------

/// **A rollback must not silently zero a shipment in flight.** In-flight cargo is state nothing can
/// re-derive: the party is halfway to somebody else's camp holding goods that have already left the
/// sender's store.
///
/// The wipe between capture and restore is the liveness half — without it a cargo that was never
/// touched would pass on any implementation.
#[test]
fn cargo_survives_a_checkpoint_round_trip() {
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    let mut app = spawn_world();
    let sender = first_band(&mut app);
    let (destination, destination_id) = split_off(&mut app, sender);
    // The destination stands well away, so the party is genuinely in flight at the checkpoint — a
    // co-located one is delivered to on the launch turn and there is no cargo left to round-trip.
    let sender_pos = band_position(&app, sender);
    let far = walk_away(&mut app, destination, sender_pos);
    let party = launch_shipment(&mut app, sender, destination_id, far, two_rating_cargo());
    {
        let mut expedition = app.world.get_mut::<Expedition>(party).expect("the party");
        expedition.cargo.add(FOOD, scalar_from_f32(CARGO_FOOD));
    }

    let checkpoint = capture_sim_state(&app.world);
    let party_band = *app.world.get::<BandId>(party).expect("the party has an id");

    // Wipe it, and prove it is gone before the restore, or the test proves nothing.
    {
        let mut expedition = app.world.get_mut::<Expedition>(party).expect("the party");
        let carried = expedition.cargo.get(FOOD);
        expedition.cargo.take(FOOD, carried);
        let mut sink = LocalStore::new();
        expedition.cargo.drain_materials_into(&mut sink);
        assert_eq!(
            expedition.cargo.get(FOOD),
            scalar_zero(),
            "the cargo is empty before the restore"
        );
    }

    restore_sim_state(&mut app.world, &checkpoint);

    let restored_entity =
        entity_for_band(&mut app, party_band).expect("the party comes back with the checkpoint");
    let expedition = app
        .world
        .get::<Expedition>(restored_entity)
        .expect("and it comes back carrying its mission");
    assert_eq!(
        expedition.mission.destination_band(),
        Some(destination_id),
        "a restored shipment still knows where it was going"
    );
    assert!(
        (expedition.cargo.get(FOOD).to_f32() - CARGO_FOOD).abs() < EPSILON,
        "the carried food comes back: {}",
        expedition.cargo.get(FOOD).to_f32()
    );
    let batches: Vec<f32> = expedition
        .cargo
        .material_batches(HIDE)
        .map(|(_, batch)| batch.characteristics["toughness"])
        .collect();
    assert_eq!(
        batches.len(),
        2,
        "both ratings come back as two batches — a rollback may not merge them either"
    );
    assert!(
        batches.iter().any(|t| (t - 0.9).abs() < 1e-4)
            && batches.iter().any(|t| (t - 0.1).abs() < 1e-4),
        "with their EXACT readings: {batches:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// A destination that disappears
// ---------------------------------------------------------------------------------------------

/// **A shipment whose destination is gone turns for home CARRYING IT**, and the goods land back in
/// the band that sent them. The party does not hang on the map holding goods nobody can reach.
///
/// The cargo is **material** rather than food, and deliberately: a material is not eaten, so *"the
/// hide came home"* is an exact statement about the sender's store rather than a number competing
/// with the band's own meal.
#[test]
fn a_destination_that_vanishes_sends_the_party_home_with_its_cargo() {
    let mut app = spawn_world();
    let sender = first_band(&mut app);
    let (destination, destination_id) = split_off(&mut app, sender);
    let sender_pos = band_position(&app, sender);
    let far = walk_away(&mut app, destination, sender_pos);
    let held_before = app
        .world
        .get::<PopulationCohort>(sender)
        .expect("the band")
        .stores
        .material_total(HIDE)
        .to_f32();
    let party = launch_shipment(&mut app, sender, destination_id, far, two_rating_cargo());

    // Let it get **out of comm range of home** first. A party that turns around while still in
    // camp folds back the same turn (correctly — there is nowhere to walk), which would make the
    // `Returning` phase below unobservable rather than absent.
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

    // The destination is erased under the party — the state the guard exists for.
    app.world.despawn(destination);
    run_turn(&mut app);

    assert_eq!(
        app.world
            .get::<Expedition>(party)
            .map(|expedition| expedition.phase),
        Some(ExpeditionPhase::Returning),
        "a shipment with nobody left to deliver to turns for home"
    );

    // And it really gets there: drive it until it folds back, then look for the goods.
    for _ in 0..MAX_TURNS_HOME {
        if app.world.get::<Expedition>(party).is_none() {
            break;
        }
        run_turn(&mut app);
    }
    assert!(
        app.world.get::<Expedition>(party).is_none(),
        "the party folds back rather than walking forever"
    );
    let held_after = app
        .world
        .get::<PopulationCohort>(sender)
        .expect("the band")
        .stores
        .material_total(HIDE)
        .to_f32();
    assert!(
        (held_after - held_before - (FINE_AMOUNT + POOR_AMOUNT)).abs() < EPSILON,
        "the undelivered shipment comes home whole rather than being destroyed: \
         {held_before} -> {held_after}"
    );
}

/// Turns to let a party walk home from [`OUT_OF_REACH_TILES`] out. Generous — the assertion is that
/// it *arrives*, not how fast.
const MAX_TURNS_HOME: u32 = 60;
/// Turns the party walks before its destination is erased — enough to put it past the comm range a
/// fold-back is gated on, so the turn-for-home is a real leg rather than an instant cancel.
const TURNS_TO_GET_CLEAR: u32 = 6;

// ---------------------------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------------------------

/// **The shipment is legible on the encoded wire** — the key, its display twin, and both cargo
/// accounts, read through the accessor chain a client uses.
///
/// The material rows are asserted **per material**, never as a total: a sum of hide and bone is the
/// retired trade axis under a new name.
#[test]
fn a_shipment_publishes_its_destination_and_its_cargo_on_the_wire() {
    let mut app = spawn_world();
    let sender = first_band(&mut app);
    let faction = app
        .world
        .get::<PopulationCohort>(sender)
        .expect("the band")
        .faction;
    app.world.insert_resource(ViewerFaction(faction));
    let (destination, destination_id) = split_off(&mut app, sender);
    // Still in flight after the turn below, so the row published is a *loaded* party's.
    let sender_pos = band_position(&app, sender);
    let far = walk_away(&mut app, destination, sender_pos);
    let mut cargo = two_rating_cargo();
    cargo.add(FOOD, scalar_from_f32(CARGO_FOOD));
    let party = launch_shipment(&mut app, sender, destination_id, far, cargo);
    let party_band = *app.world.get::<BandId>(party).expect("the party has an id");

    run_turn(&mut app);

    let (published_destination, name, food, materials) = published_cargo(&app, party_band);
    assert_eq!(
        published_destination, destination_id.0,
        "the KEY the command addresses the destination by is on the wire"
    );
    // **The name CARRIES when there is one.** The fixture supplies one because that is the claim
    // this assertion makes — the mission holds the name for the party's life and it reaches the
    // wire, which is what #513 will need. A **real launch** resolves no name at all and publishes
    // `""`; that is pinned where launches happen, in
    // `server::tests::a_real_launch_publishes_no_destination_name_rather_than_a_unit_kind`.
    assert_eq!(
        name, "the neighbours",
        "a name the mission carries reaches the wire verbatim"
    );
    assert!(
        (food - CARGO_FOOD).abs() < EPSILON,
        "the shipment's food account is published: {food}"
    );
    assert_eq!(
        materials.len(),
        1,
        "one row per MATERIAL — two batches of hide are one hide row: {materials:?}"
    );
    assert_eq!(materials[0].0, HIDE);
    assert!(
        (materials[0].1 - (FINE_AMOUNT + POOR_AMOUNT)).abs() < EPSILON,
        "the row states how much of that material the party holds: {materials:?}"
    );

    // The negative control: a resident band is not carrying a shipment, and says so with the
    // absent/zero default rather than by omission.
    let sender_band = *app.world.get::<BandId>(sender).expect("the band has an id");
    let (none_destination, none_name, none_food, none_materials) =
        published_cargo(&app, sender_band);
    assert_eq!(none_destination, 0);
    assert!(none_name.is_empty());
    assert_eq!(none_food, 0.0);
    assert!(
        none_materials.is_empty(),
        "empty means NO ROW, never a zero-valued one"
    );
}

/// **Every cohort publishes BOTH levers of the shipment mass rule**, so a cargo picker can run the
/// sim's own expression instead of guessing at it:
///
/// ```text
/// mass = expeditionCargoFood + expeditionTradeMaterialCarryWeight × Σ material amounts
/// cap  = party_workers × expeditionTradePerWorkerCarry
/// ```
///
/// This is the pair the outfit UI needs, and it must be a *global* echo: the player prices a
/// manifest for a party that **does not exist yet**, and `party_workers` is what the stepper is
/// choosing, so no per-party field can serve that screen. Same idiom as
/// `expeditionForecastHorizonTurns`.
///
/// **The two carry different bounds, deliberately.** The pack lever is asserted **positive** for the
/// horizon's reason — a `0` lets a client render a zero cap and refuse every manifest a player could
/// build. The material weight is asserted only **finite and `>= 0`**, because `0` is a legitimate
/// setting there ("materials are weightless") and asserting positivity would pin a tuning as a rule.
#[test]
fn every_cohort_publishes_the_shipment_mass_levers_on_the_wire() {
    let mut app = spawn_world();
    let sender = first_band(&mut app);
    let faction = app
        .world
        .get::<PopulationCohort>(sender)
        .expect("the band")
        .faction;
    app.world.insert_resource(ViewerFaction(faction));
    let (destination, destination_id) = split_off(&mut app, sender);
    let sender_pos = band_position(&app, sender);
    let far = walk_away(&mut app, destination, sender_pos);
    launch_shipment(
        &mut app,
        sender,
        destination_id,
        far,
        food_cargo(CARGO_FOOD),
    );

    run_turn(&mut app);

    let (expected_carry, expected_weight) = {
        let cfg = app
            .world
            .resource::<core_sim::ExpeditionConfigHandle>()
            .get();
        (cfg.trade.per_worker_carry, cfg.trade.material_carry_weight)
    };
    let packs = published_packs(&app);
    assert!(
        !packs.is_empty(),
        "the liveness half — no cohorts published means every assertion below is vacuous"
    );
    for (band, _, trade_per_worker, material_weight) in &packs {
        assert!(
            *trade_per_worker > 0.0,
            "band {band} published a shipment pack of {trade_per_worker} — a zero lever lets a \
             client render a zero cap and refuse every manifest"
        );
        assert!(
            (trade_per_worker - expected_carry).abs() < EPSILON,
            "band {band} must echo the pack lever verbatim: {trade_per_worker} vs {expected_carry}"
        );
        // Finite and `>= 0`, NOT positive: weightless materials is a real setting, so asserting
        // positivity here would pin a tuning as if it were a rule.
        assert!(
            material_weight.is_finite() && *material_weight >= 0.0,
            "band {band} published a material carry weight of {material_weight} — the mass \
             expression cannot be run against a negative or non-finite one"
        );
        assert!(
            (material_weight - expected_weight).abs() < EPSILON,
            "band {band} must echo the weight lever verbatim: {material_weight} vs \
             {expected_weight}"
        );
    }

    // **The client's expression is the sim's.** Compose the published mass of the shipment the
    // fixture launched and check it against the published cap — the arithmetic a cargo picker's
    // meter runs, on nothing but wire fields.
    let party_band = {
        let mut query = app.world.query::<(&BandId, &Expedition)>();
        *query
            .iter(&app.world)
            .next()
            .expect("the shipment is on the map")
            .0
    };
    let (_, party_cap, _, weight) = packs
        .iter()
        .find(|(band, ..)| *band == party_band.0)
        .copied()
        .expect("the party's own row is published");
    let (_, _, cargo_food, cargo_materials) = published_cargo(&app, party_band);
    let mass = cargo_food
        + weight
            * cargo_materials
                .iter()
                .map(|(_, amount)| amount)
                .sum::<f32>();
    assert!(
        (mass - CARGO_FOOD).abs() < EPSILON,
        "the fixture's shipment is pure food, so its composed mass is its food: {mass}"
    );
    assert!(
        mass <= party_cap + EPSILON,
        "a shipment the sim accepted must compose to a mass inside the published cap: \
         {mass} vs {party_cap}"
    );
}

/// **A live trade party's own pack is quoted at the TRADE lever, not the hunt one.**
///
/// The two are different packs — a raid's is the provisions ceiling it fills before delivering, a
/// shipment's is what its people can carry out — so `expeditionCarryCap` resolves per mission. The
/// contrast with a resident band's `0` is the negative control, and pinning the trade party's cap
/// against the *hunt* lever's product is what would catch the field being wired to the wrong one.
#[test]
fn a_trade_partys_carry_cap_is_quoted_at_the_shipment_lever() {
    let mut app = spawn_world();
    let sender = first_band(&mut app);
    let faction = app
        .world
        .get::<PopulationCohort>(sender)
        .expect("the band")
        .faction;
    app.world.insert_resource(ViewerFaction(faction));
    let (destination, destination_id) = split_off(&mut app, sender);
    let sender_pos = band_position(&app, sender);
    let far = walk_away(&mut app, destination, sender_pos);
    let party = launch_shipment(
        &mut app,
        sender,
        destination_id,
        far,
        food_cargo(CARGO_FOOD),
    );
    let party_band = *app.world.get::<BandId>(party).expect("the party has an id");
    let sender_band = *app.world.get::<BandId>(sender).expect("the band has an id");

    run_turn(&mut app);

    let (trade_carry, hunt_carry) = {
        let cfg = app
            .world
            .resource::<core_sim::ExpeditionConfigHandle>()
            .get();
        (cfg.trade.per_worker_carry, cfg.hunt.per_worker_carry)
    };
    // The two levers must genuinely differ, or "quoted at the right one" is unfalsifiable.
    assert!(
        (trade_carry - hunt_carry).abs() > EPSILON,
        "the fixture rests on the two packs being different numbers: trade {trade_carry}, hunt \
         {hunt_carry}"
    );

    let packs = published_packs(&app);
    let (_, party_cap, _, _) = packs
        .iter()
        .find(|(band, ..)| *band == party_band.0)
        .copied()
        .expect("the party's own row is published");
    let (_, band_cap, _, _) = packs
        .iter()
        .find(|(band, ..)| *band == sender_band.0)
        .copied()
        .expect("the resident band's row is published");

    assert!(
        (party_cap - PARTY_WORKERS as f32 * trade_carry).abs() < EPSILON,
        "a shipment party's pack is `workers × trade.per_worker_carry`: got {party_cap}, wanted {}",
        PARTY_WORKERS as f32 * trade_carry
    );
    assert!(
        (party_cap - PARTY_WORKERS as f32 * hunt_carry).abs() > EPSILON,
        "and emphatically NOT the raid's pack — a client reading the hunt lever would quote a cap \
         the launch command refuses: got {party_cap}"
    );
    assert_eq!(
        band_cap, 0.0,
        "a resident band is not a party and carries no pack"
    );
}
