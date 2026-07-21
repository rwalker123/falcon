//! The coastal seal colony — a **hunt-only marine forager** whose population is decoupled from the
//! land's graze layer. Two things are proven here:
//!
//! 1. **The roster row loads** with the intended shape (`wild` / non-migratory / `big`, a display name
//!    the client icon resolves off, and a live `coastal_littoral` spawn host).
//! 2. **The non-grazing constant-K path.** A seal herd omits `fodder_per_biomass` (→ `0.0`), so
//!    `fauna::ecological_carrying_capacity` returns `None` and the per-turn K recompute is skipped —
//!    the colony holds a **constant** carrying capacity (= its biomass max) fed by the sea, instead of
//!    the range-derived K a grazer's would be shrunk to. Run against a **non-empty** graze layer, so
//!    the constancy is the `fodder == 0` decoupling, not the empty-registry short-circuit the isolated
//!    fauna harnesses lean on.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_graze_regrowth, advance_herd_grazing, advance_herds, classify_food_module,
    spawn_initial_graze, spawn_initial_herds, spawn_initial_world, CultureManager,
    DiscoveryProgressLedger, FactionInventory, FaunaConfig, FaunaConfigHandle, FoodModule,
    GenerationRegistry, GrazeRegistry, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry,
    HusbandryCeiling, LadderConfigHandle, MapPresets, MapPresetsHandle, SimulationConfig,
    SimulationTick, SizeClass, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile,
};
use sim_runtime::TerrainType;

/// A pinned land-rich earthlike map — coasts (and so `coastal_littoral` tiles) exist on it, and
/// `spawn_initial_graze` seeds a real, non-empty `GrazeRegistry`.
const MAP_SEED: u64 = 119304647;

/// Turns to run the coupled fauna chain — long enough that a grazing herd's K would have been driven
/// well down from its spawn max by an overgrazed range, so a K that never moves is meaningful.
const TURNS: u32 = 30;

/// Stand up the world with a seeded graze layer (mirrors `grazing_2b_convergence`'s harness — the seal
/// proof is about the herd↔graze *decoupling*, so it needs a live graze registry present).
fn base_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
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

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(GrazeRegistry::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_graze);
    app
}

/// One turn of the coupled fauna Logistics chain, in the live stage order.
fn run_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_herd_grazing);
    app.world.run_system_once(advance_graze_regrowth);
}

/// The Grey Seals roster row loads with the intended hunt-only marine-forager shape.
#[test]
fn seal_row_loads_as_wild_marine_forager() {
    let fauna = FaunaConfig::builtin();
    let seal = fauna
        .species
        .get("seal")
        .expect("the fauna roster defines the coastal seal colony");

    // Hunt-only: never climbs the husbandry ladder (mirrors mammoth/deer).
    assert_eq!(seal.husbandry_ceiling, HusbandryCeiling::Wild);
    assert!(
        !seal.migratory,
        "a haul-out colony is a stationary big-game herd, not migratory"
    );
    assert_eq!(seal.size_class, SizeClass::Big);

    // The client resolves the herd icon off the display name — it MUST embed "seal".
    assert!(
        seal.display_name.to_lowercase().contains("seal"),
        "display_name {:?} must embed the client icon keyword \"seal\"",
        seal.display_name
    );

    // Omitting `fodder_per_biomass` is what makes the colony non-grazing (constant K = biomass max).
    assert_eq!(
        seal.fodder_per_biomass, 0.0,
        "the seal omits fodder_per_biomass → a non-grazing herd fed by the sea"
    );
    assert_eq!(
        seal.carrying_capacity(),
        1000.0,
        "constant K is the biomass max"
    );

    // It spawns via the short-range game pass on the coast: a live host + a positive abundance density.
    assert!(
        seal.hosts_biome("coastal_littoral"),
        "seals host the littoral"
    );
    assert!(
        fauna.abundance.probability_for("coastal_littoral") > 0.0,
        "coastal_littoral must have a positive abundance or the seal never spawns"
    );
    assert!(
        fauna
            .game_species_for_biome("coastal_littoral")
            .iter()
            .any(|(k, _)| k.as_str() == "seal"),
        "the coastal spawn picker must offer the seal"
    );
}

/// A seal colony on a `coastal_littoral` tile keeps a **constant** carrying capacity across turns — the
/// non-grazing (`fodder == 0`) path skips the range-derived K recompute a grazer gets, even with a live
/// graze layer under it.
#[test]
fn seal_colony_keeps_constant_carrying_capacity() {
    let mut app = base_world();

    // The graze layer must be non-empty, or the constancy proves nothing (an empty registry
    // short-circuits `ecological_carrying_capacity` for EVERY species, grazer or not).
    assert!(
        !app.world.resource::<GrazeRegistry>().is_empty(),
        "the earthlike map must seed a non-empty graze layer"
    );

    // Seat the colony on a **coastal_littoral** tile that has real graze under it, so the constancy is
    // the seal's `fodder == 0` decoupling and not a barren footprint. The `earthlike` preset renders
    // its coasts as `continental_shelf` (→ `coastal_upwelling`), not the tidal/mangrove terrains that
    // read `coastal_littoral`, so we take the richest *grazed* pasture tile (guaranteed present) and
    // stamp it a `TidalFlat` — a littoral terrain — keeping its already-seeded graze patch.
    let coastal = app
        .world
        .resource::<GrazeRegistry>()
        .richest_patch()
        .expect("the earthlike map seeds graze patches")
        .0;
    {
        let mut tiles = app.world.query::<&mut Tile>();
        let mut tile = tiles
            .iter_mut(&mut app.world)
            .find(|tile| tile.position == coastal)
            .expect("the grazed tile has a Tile entity");
        tile.terrain = TerrainType::TidalFlat;
        assert_eq!(
            classify_food_module(&tile),
            Some(FoodModule::CoastalLittoral),
            "the stamped tile must read as the seal's coastal_littoral host biome"
        );
    }
    assert!(
        app.world
            .resource::<GrazeRegistry>()
            .patch(coastal)
            .is_some(),
        "the seal's tile must keep a live graze patch (so a grazer WOULD recompute K here)"
    );

    // Seat a single seal colony built straight from the roster row (no hand-tuned literals).
    let fauna = FaunaConfig::builtin();
    let seal = fauna.species.get("seal").expect("seal row present");
    let k0 = seal.carrying_capacity();
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.herds.clear();
        let mut herd = Herd::new(
            "seal_test".to_string(),
            seal.display_name.clone(),
            seal.size_class,
            vec![coastal],
            k0, // start at capacity — a settled colony
            k0,
            seal.fodder_per_biomass, // 0.0 → non-grazing
            seal.regrowth_rate_or(fauna.ecology.regrowth_rate),
            seal.body_mass,
        );
        herd.husbandry_ceiling = seal.husbandry_ceiling;
        registry.herds.push(herd);
    }

    // Run the coupled chain: K must hold dead flat at the biomass max every turn.
    for turn in 0..TURNS {
        run_turn(&mut app);
        let k = app
            .world
            .resource::<HerdRegistry>()
            .find("seal_test")
            .map(|h| h.carrying_capacity)
            .expect("the seal colony never disperses at capacity");
        assert_eq!(
            k, k0,
            "turn {turn}: a non-grazing seal colony's K must stay constant at its biomass max \
             ({k0}), not be recomputed from the graze layer (got {k})"
        );
    }
}
