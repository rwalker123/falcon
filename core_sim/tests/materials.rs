//! **Materials — the store, the pool and the yield edge** (`docs/plan_crafting_and_materials.md`).
//!
//! The merge rule and worst-first withdrawal are pinned as unit tests on `LocalStore` itself
//! (`components.rs`); what needs a world is everything the store cannot see on its own: that a real
//! take credits materials off what came **home**, that pooling between bands never averages one
//! rating into another, and that a checkpoint carries the batches back verbatim.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, balance_supply_networks, build_headless_app, scalar_from_f32,
    scalar_one, scalar_zero, spawn_initial_herds, spawn_initial_world, BandId, BandKey,
    CommandEventLog, ConnectionKey, ConnectionLedger, ConnectionsConfig, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, ForageRegistry,
    GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MaterialsConfigHandle, MoraleCause, PopulationCohort, ResidentBand, Scalar,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    SupplyNetworkConfigHandle, SupplyNetworkMembership, TileRegistry, WellbeingConfigHandle,
    FULL_TIE,
};
use std::collections::BTreeMap;

/// A crew big enough that the per-worker carry never binds, so the take is the floor's to decide.
const HUNT_WORKERS: u32 = 5000;
/// Leave the whole herd standing: the escapement room is zero, so nothing is killed and nothing is
/// hauled home. The "no take, no material" half of the yield pairing.
const LEAVE_IT_ALL: f32 = 1.0;
/// Take everything standing. The other half.
const TAKE_IT_ALL: f32 = 0.0;

const HIDE: &str = "hide";
const TOUGHNESS: &str = "toughness";
const SUPPLENESS: &str = "suppleness";

/// The faction the pooling bands belong to. It does **not** isolate them — faction is a property of
/// the endpoint and never a branch in the balancer — what keeps them off the spawned starting bands
/// is that no tie is ever seeded to one.
const TEST_FACTION: FactionId = FactionId(7);
const BAND_POP: u32 = 100;

/// The two pooling bands' ids, well clear of anything `spawn_initial_world` allocates so a seeded
/// tie cannot name a starting band by accident.
const STIFF_BAND: BandId = BandId(9_001);
const SUPPLE_BAND: BandId = BandId(9_002);

fn readings(tough: f32, supple: f32) -> BTreeMap<String, f32> {
    BTreeMap::from([
        (TOUGHNESS.to_string(), tough),
        (SUPPLENESS.to_string(), supple),
    ])
}

fn cohort(tile: Entity, working: f32, stores: LocalStore, faction: FactionId) -> PopulationCohort {
    PopulationCohort {
        home: tile,
        current_tile: tile,
        size: BAND_POP,
        children: scalar_zero(),
        working: scalar_from_f32(working),
        elders: scalar_zero(),
        stores,
        morale: scalar_one(),
        last_food_consumption: 0.0,
        last_turn_transfer_received: 0.0,
        last_turn_transfer_sent: 0.0,
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
        faction,
        knowledge: Vec::new(),
        migration: None,
    }
}

fn base_world() -> App {
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

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();
    app
}

/// `base_world` plus the herd registry and every config the labor arm reads.
fn hunting_world() -> App {
    let mut app = base_world();
    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    app.world.insert_resource(MaterialsConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// Hunt the first hide-bearing herd on the map at `floor`, and report the hunting band's hide stock
/// plus the material's exact reading on `toughness`.
fn hunt_and_read_hide(floor: f32) -> (Scalar, Option<f32>, f32) {
    let mut app = hunting_world();

    let (herd_id, herd_pos, biomass_before) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry
            .herds
            .iter()
            .find(|herd| {
                herd.id.starts_with("game_")
                    && fauna
                        .hunt_materials_for(&herd.species)
                        .iter()
                        .any(|row| row.material == HIDE)
            })
            .expect("the shipped roster gives hide on short-range game");
        (herd.id.clone(), herd.position(), herd.biomass)
    };

    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(herd_pos.x, herd_pos.y)
        .expect("herd tile resolves");

    let band = app
        .world
        .spawn((
            cohort(tile, HUNT_WORKERS as f32, LocalStore::new(), FactionId(0)),
            StartingUnit {
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: herd_id.clone(),
                        floor,
                    },
                    workers: HUNT_WORKERS,
                    improvement: None,
                    kit: None,
                }],
                ..Default::default()
            },
        ))
        .id();

    app.world.run_system_once(advance_labor_allocation);

    let biomass_after = app
        .world
        .resource::<HerdRegistry>()
        .find(&herd_id)
        .map(|herd| herd.biomass)
        .unwrap_or(0.0);
    let store = &app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band survives its own hunt")
        .stores;
    let reading = store
        .material_batches(HIDE)
        .next()
        .and_then(|(_, batch)| batch.characteristics.get(TOUGHNESS).copied());
    (
        store.material_total(HIDE),
        reading,
        biomass_before - biomass_after,
    )
}

/// **A hunt credits the fourth account, and only when something came home.**
///
/// The pairing is the whole test: `floor 0` strips the herd and must bank hide *at the species' own
/// stated reading*, while `floor 1.0` leaves everything standing and must bank none. Asserting only
/// the second half would pass on a sim that never credited materials at all, and asserting only the
/// first would pass on one that credited them off the kill rather than off the haul.
#[test]
fn a_hunt_banks_the_hide_it_hauls_home_and_a_hunt_that_takes_nothing_banks_none() {
    let (taken_hide, reading, biomass_taken) = hunt_and_read_hide(TAKE_IT_ALL);
    assert!(
        biomass_taken > 0.0,
        "the stripping hunt must actually take biomass, or the credit half proves nothing"
    );
    assert!(
        taken_hide > scalar_zero(),
        "a hunt that hauled {biomass_taken} biomass home must bank hide, got {taken_hide:?}"
    );
    let reading = reading.expect("the batch carries the species' reading, not just a band");
    assert!(
        (0.0..=1.0).contains(&reading),
        "the batch reads a real characteristic, got {reading}"
    );

    let (untouched_hide, untouched_reading, untouched_biomass) = hunt_and_read_hide(LEAVE_IT_ALL);
    assert_eq!(
        untouched_biomass, 0.0,
        "leaving the whole herd standing takes nothing"
    );
    assert_eq!(
        untouched_hide,
        scalar_zero(),
        "a take that hauls nothing home yields no material"
    );
    assert!(untouched_reading.is_none());
}

/// Spawn a band at `(x, y)` holding one hide batch at `band_key`, reading `(tough, supple)`.
///
/// `band` is its [`BandId`] — the endpoint identity the pooling gate reads. A cohort without one
/// never joins a supply network.
#[allow(clippy::too_many_arguments)]
fn spawn_hide_band(
    app: &mut App,
    x: u32,
    y: u32,
    band: BandId,
    band_key: BandKey,
    amount: f32,
    tough: f32,
    supple: f32,
) -> Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(x, y)
        .expect("tile coords resolve");
    let mut stores = LocalStore::new();
    if amount > 0.0 {
        stores.deposit_material(
            HIDE,
            band_key,
            scalar_from_f32(amount),
            &readings(tough, supple),
        );
    }
    app.world
        .spawn((
            cohort(tile, BAND_POP as f32, stores, TEST_FACTION),
            ResidentBand,
            band,
        ))
        .id()
}

/// A live, mutual, full-strength tie between two bands — what standing beside each other for a few
/// turns leaves behind, and what the pooling pass requires before it will move anything.
fn seed_mutual_tie(app: &mut App, a: BandId, b: BandId) {
    const SEEDED_ON_TURN: u64 = 0;
    /// Clock 1 is not read by pooling, so the remembered position is immaterial here.
    const SOMEWHERE: bevy::math::UVec2 = bevy::math::UVec2::ZERO;

    let cfg = ConnectionsConfig::default();
    let contacts_to_full = (FULL_TIE.to_f32() / cfg.strength.gain_per_contact).ceil() as u32;
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for key in [ConnectionKey::new(a, b), ConnectionKey::new(b, a)] {
        for _ in 0..contacts_to_full {
            ledger.record_contact(key, SOMEWHERE, SEEDED_ON_TURN, SEEDED_ON_TURN, &cfg);
        }
    }
}

fn hide_at(app: &App, band: Entity, key: &BandKey) -> Option<(Scalar, f32, f32)> {
    app.world
        .get::<PopulationCohort>(band)?
        .stores
        .material_batches(HIDE)
        .find(|(band_key, _)| *band_key == key)
        .map(|(_, batch)| {
            (
                batch.amount,
                batch.characteristics[TOUGHNESS],
                batch.characteristics[SUPPLENESS],
            )
        })
}

/// **Pooling moves a material WITH its characteristics, and never averages one rating into
/// another.**
///
/// Two neighbouring bands, each holding one hide batch of the *opposite* rating: a mammoth-grade
/// hide (`tough` high, `supple` low) and a hare-grade one. The balancer runs per rating, so each
/// band should end up holding **both** batches, each still reading exactly what it read before.
///
/// Three assertions, and each kills a different wrong implementation: the amounts moved at all (a
/// no-op balancer), both ratings survive as separate batches (a scalar pool would leave one), and
/// the readings are unmoved (a pool that averaged them would land both on the midpoint).
#[test]
fn pooling_a_material_between_bands_preserves_its_characteristics() {
    let mut app = base_world();
    app.world
        .insert_resource(SupplyNetworkConfigHandle::default());
    app.world
        .insert_resource(SupplyNetworkMembership::default());
    // The pooling gate: bands pool only where the ledger holds a live tie between them.
    app.world.insert_resource(ConnectionLedger::default());

    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let (cx, cy) = (width / 4, height / 2);

    // `excellent toughness / poor suppleness` against its exact reverse — two ratings, so the
    // balancer must treat them as two commodities.
    let tough_key = BandKey(vec![3, 0]);
    let supple_key = BandKey(vec![0, 3]);
    let stiff = spawn_hide_band(
        &mut app,
        cx,
        cy,
        STIFF_BAND,
        tough_key.clone(),
        100.0,
        0.92,
        0.10,
    );
    let soft = spawn_hide_band(
        &mut app,
        cx + 2,
        cy,
        SUPPLE_BAND,
        supple_key.clone(),
        100.0,
        0.14,
        0.92,
    );
    seed_mutual_tie(&mut app, STIFF_BAND, SUPPLE_BAND);

    app.world.run_system_once(balance_supply_networks);

    for (holder, name) in [
        (stiff, "the stiff-hide band"),
        (soft, "the supple-hide band"),
    ] {
        let (tough_amount, tough_reading, tough_supple) = hide_at(&app, holder, &tough_key)
            .unwrap_or_else(|| panic!("{name} should hold some of the stiff rating after pooling"));
        let (supple_amount, supple_tough, supple_reading) = hide_at(&app, holder, &supple_key)
            .unwrap_or_else(|| {
                panic!("{name} should hold some of the supple rating after pooling")
            });
        assert!(
            tough_amount > scalar_zero() && supple_amount > scalar_zero(),
            "{name} must end up holding BOTH ratings, or nothing moved"
        );
        assert!(
            (tough_reading - 0.92).abs() < 1e-4 && (tough_supple - 0.10).abs() < 1e-4,
            "{name}: the stiff hide must still read 0.92/0.10, got {tough_reading}/{tough_supple}"
        );
        assert!(
            (supple_tough - 0.14).abs() < 1e-4 && (supple_reading - 0.92).abs() < 1e-4,
            "{name}: the supple hide must still read 0.14/0.92, got {supple_tough}/{supple_reading}"
        );
    }

    // Liveness: the two bands genuinely traded rather than each keeping only its own pile.
    let (stiff_own, _, _) = hide_at(&app, stiff, &tough_key).expect("the stiff band keeps some");
    assert!(
        stiff_own < scalar_from_f32(100.0),
        "the stiff band must have shipped some of its own rating away, still holds {stiff_own:?}"
    );
}

/// **A checkpoint carries the batch map verbatim, both directions.** A rollback that forgot a
/// batch's characteristics would silently re-grade the band's whole stock.
///
/// The wipe between capture and restore is the liveness half: without it a store that was never
/// touched would pass on any implementation.
#[test]
fn material_batches_survive_a_checkpoint_round_trip() {
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    let mut app = build_headless_app();
    core_sim::run_turn(&mut app);

    let band = app
        .world
        .query_filtered::<Entity, (
            bevy::prelude::With<PopulationCohort>,
            bevy::prelude::With<core_sim::BandId>,
        )>()
        .iter(&app.world)
        .next()
        .expect("the shipped opening spawns a band");

    let key = BandKey(vec![3, 0]);
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .deposit_material(
            HIDE,
            key.clone(),
            scalar_from_f32(37.5),
            &readings(0.91, 0.12),
        );

    let checkpoint = capture_sim_state(&app.world);

    // Wipe it, and prove it is gone before the restore, or the test proves nothing.
    let drawn = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .take_material(HIDE, TOUGHNESS, scalar_from_f32(1_000.0));
    assert!(!drawn.is_empty(), "there was something to wipe");
    assert_eq!(
        app.world
            .get::<PopulationCohort>(band)
            .expect("cohort")
            .stores
            .material_total(HIDE),
        scalar_zero(),
        "the store is empty before the restore"
    );

    restore_sim_state(&mut app.world, &checkpoint);

    let restored = app
        .world
        .query_filtered::<(Entity, &PopulationCohort), bevy::prelude::With<core_sim::BandId>>()
        .iter(&app.world)
        .find_map(|(_, cohort)| {
            cohort
                .stores
                .material_batches(HIDE)
                .find(|(band_key, _)| *band_key == &key)
                .map(|(_, batch)| batch.clone())
        })
        .expect("the batch comes back with the checkpoint");
    assert_eq!(restored.amount, scalar_from_f32(37.5));
    assert!(
        (restored.characteristics[TOUGHNESS] - 0.91).abs() < 1e-6
            && (restored.characteristics[SUPPLENESS] - 0.12).abs() < 1e-6,
        "the EXACT reading must survive, not just the band: got {:?}",
        restored.characteristics
    );
}

/// **EQUIPMENT BATCHES RIDE THE CHECKPOINT — count, tier, grade and wear.**
///
/// A rollback that forgot how many spears a band held would silently re-stock it, and one that
/// forgot a batch's *grade* would re-grade the band's gear — a fine sled quietly becoming standard.
/// The ledger is cloned whole, so this is free; it is pinned because "free" is a property of the
/// current shape and the wipe below is what makes the assertion mean anything.
#[test]
fn equipment_batches_survive_a_checkpoint_round_trip() {
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    let mut app = build_headless_app();
    core_sim::run_turn(&mut app);

    let band = app
        .world
        .query_filtered::<Entity, (
            bevy::prelude::With<PopulationCohort>,
            bevy::prelude::With<core_sim::BandId>,
        )>()
        .iter(&app.world)
        .next()
        .expect("the shipped opening spawns a band");

    const SPEARS: &str = "spears";
    const FINE: &str = "fine";
    let equipment = core_sim::EquipmentConfig::builtin();
    let graded = core_sim::BatchGrade {
        id: FINE.to_string(),
        effects: Vec::new(),
    };
    // A spawn stocks a party's worth, so the count this fixture asserts on is that stock plus what
    // the bench added — read rather than written, because the stock is sized off the band's own
    // head count.
    let spawned = app
        .world
        .get::<core_sim::BandEquipment>(band)
        .expect("a spawned band carries a ledger")
        .count_of(SPEARS);
    const MADE: u32 = 4;
    {
        let mut ledger = app
            .world
            .get_mut::<core_sim::BandEquipment>(band)
            .expect("a spawned band carries a ledger");
        ledger.stock(SPEARS, MADE, "flint", Some(graded.clone()));
    }
    let before = app
        .world
        .get::<core_sim::BandEquipment>(band)
        .expect("ledger")
        .clone();
    assert!(spawned > 0, "the spawn really stocked spears");
    assert_eq!(
        before.count_of(SPEARS),
        spawned + MADE,
        "the spawned stock plus the four made"
    );

    let checkpoint = capture_sim_state(&app.world);

    // Wipe it, and prove it is gone before the restore, or the test proves nothing.
    app.world
        .get_mut::<core_sim::BandEquipment>(band)
        .expect("ledger")
        .restore_batches(SPEARS, Vec::new());
    assert_eq!(
        app.world
            .get::<core_sim::BandEquipment>(band)
            .expect("ledger")
            .count_of(SPEARS),
        0,
        "the ledger is empty before the restore"
    );

    restore_sim_state(&mut app.world, &checkpoint);

    let band = app
        .world
        .query_filtered::<Entity, (
            bevy::prelude::With<PopulationCohort>,
            bevy::prelude::With<core_sim::BandId>,
        )>()
        .iter(&app.world)
        .next()
        .expect("the restore re-spawns the band");
    let after = app
        .world
        .get::<core_sim::BandEquipment>(band)
        .expect("the restored band carries a ledger");
    assert_eq!(
        after, &before,
        "every batch comes back verbatim — count, tier, grade and wear"
    );
    assert_eq!(
        after.remaining(SPEARS, &equipment),
        before.remaining(SPEARS, &equipment)
    );
}
