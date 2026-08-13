//! **Cash crops — the F4 coupling** (Flora Roster F4, `docs/plan_flora_roster.md` §6).
//!
//! **The rung still exists; its payment changed** (arc #527). A cash crop (`cotton`/`flax`/
//! `tobacco`/`tea`/`grapevine`) is a `field`-ceiling species whose `provisions_per_biomass` is `0`
//! and which is paid entirely in **materials**: harvesting it as a Field credits the band's
//! **material batches** and (near) zero food. The abstract `trade_goods` scalar this file was
//! written around is retired — it was written by every take site and read by none, while the
//! material rows beside it named the same take's actual fibre and leaf — so the routing assertions
//! below read that account instead.
//!
//! F4 remains the exact twin of F3's fodder work: the *same* managed harvest, routed by the vector
//! with **no `role` branch**. This file pins that routing against the **loaded** configs, never a
//! literal, so a retune of a table fails the test instead of agreeing with a stale copy.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::NO_CREW_ON_THIS_ACTIVITY;
use core_sim::{
    advance_labor_allocation, commit_fodder_payoff, commit_payoff, generate_hydrology,
    scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage, spawn_initial_world,
    tile_forage_capacity, CommandEventLog, CultureManager, DiscoveryProgressLedger, FactionId,
    FactionInventory, FaunaConfigHandle, FloraConfig, ForageRegistry, GenerationId,
    GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborAssignment, LaborConfig, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore,
    MapPresets, MapPresetsHandle, MaterialPayoff, MoraleCause, PopulationCohort, RungKey,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit, Tile,
    TileRegistry, WellbeingConfigHandle, BUILTIN_LABOR_CONFIG, FODDER, FOOD,
};
use sim_runtime::TerrainType;

/// Head-count large enough that the per-worker collection cap never binds, so a Field harvest is
/// production-bound (the point of a managed rung).
const FORAGE_WORKERS: u32 = 5000;

/// The mechanic fixture's grid + seed — pinned here, immune to config edits (the `forage_field.rs`
/// lesson). Big enough to carry river-valley farmland with hydrology run.
const MECHANIC_GRID: UVec2 = UVec2::new(96, 64);
const PINNED_SEED: u64 = 119_304_647;

/// Float slack for a provisions/trade quote (a chain of ~4 multiplications).
const EPSILON: f32 = 1e-3;

/// The quotes are captured at neutral productivity (the client scales per-band), as the shipped
/// per-patch forecast is.
const QUOTE_MULTIPLIER: f32 = 1.0;

fn labor() -> LaborConfig {
    LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
        .expect("builtin labor config should parse and validate")
}

// ---------------------------------------------------------------------------------------------
// Config-only: the vector routes with NO role branch (against the loaded roster).
// ---------------------------------------------------------------------------------------------

/// **A cash crop pays MATERIALS and nothing else; the vector routes.** On the same sowable ground,
/// the published Field payoffs split cleanly by account: a cash crop pays `0` food / `0` fodder and
/// names material rows; a grain pays food and no materials; hay pays fodder and (its straw aside) no
/// food. The two scalar payoffs go through the *same* commodity-generic seams — the only thing that
/// differs is which component of each species' vector is non-zero. Asserted against the LOADED
/// config, so a retune moves the test.
#[test]
fn the_yield_vector_routes_by_account_with_no_role_branch() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let forage = &labor.forage;

    // Sowable river-valley farmland (capacity 205 >= the 195 field floor) that cotton/hay/grain
    // all compete for. Cotton IS hosted on AlluvialPlain too (§10 per-tile realization keeps the
    // staples dominant on their own realized tiles); Floodplain just happens to carry all three here.
    let terrain = TerrainType::Floodplain;
    let tile = UVec2::new(terrain as u32, 0);
    let capacity = forage.capacity_for(terrain);

    let composition = flora.composition(terrain);
    let payoffs = |species: &str| {
        assert!(
            composition.iter().any(|entry| entry.species == species),
            "{species} must host {terrain:?}"
        );
        let food = commit_payoff(
            tile,
            capacity,
            species,
            composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            RungKey::PlantField,
        );
        let fodder = commit_fodder_payoff(
            tile,
            capacity,
            species,
            composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            RungKey::PlantField,
        );
        // **The material account has no per-turn QUOTE**, and that is a fact about the account
        // rather than a gap in this test: a material is a batch carrying a characteristic vector, so
        // there is no scalar for a picker row to state. What the roster can say is *which* material
        // and at what rate, which is what the third reading is.
        let material: f32 = flora.species[species]
            .yield_
            .materials
            .iter()
            .map(|row| row.per_biomass)
            .sum();
        (food, fodder, material)
    };

    // Cotton — the cash crop: worthless as food, no fodder, real fibre.
    let (cotton_food, cotton_fodder, cotton_material) = payoffs("cotton");
    assert!(
        cotton_food.abs() <= EPSILON,
        "a cash Field pays no food: {cotton_food}"
    );
    assert!(
        cotton_fodder.abs() <= EPSILON,
        "a cash Field pays no fodder: {cotton_fodder}"
    );
    assert!(
        cotton_material > 0.0,
        "a cash Field's whole payoff is what it is MADE OF: {cotton_material}/biomass"
    );

    // Wild Emmer — a grain: real food, no fodder, nothing anyone builds with.
    let (grain_food, grain_fodder, grain_material) = payoffs("wild_emmer");
    assert!(
        grain_food > EPSILON,
        "a grain Field pays food: {grain_food}"
    );
    assert!(
        grain_fodder.abs() <= EPSILON,
        "a grain Field pays no fodder: {grain_fodder}"
    );
    assert_eq!(
        grain_material, 0.0,
        "a grain Field pays no material — its account is the larder"
    );

    // Hay Grass — a fodder crop: no food, real fodder. It *does* pay straw, which is the honest
    // reading rather than an exception: hay straw is a real, poor fibre on the shipped roster.
    let (hay_food, hay_fodder, _hay_material) = payoffs("hay_grass");
    assert!(
        hay_food.abs() <= EPSILON,
        "a hay Field pays no food: {hay_food}"
    );
    assert!(
        hay_fodder > EPSILON,
        "a hay Field pays fodder: {hay_fodder}"
    );
}

// **RETIRED: `the_grain_trade_token_carries_the_field_dial_and_the_wild_baseline`** (arc #527). It
// pinned the flat `0.005` trade token every staple carried against the Field dial and the wild
// baseline; the token went with the account, and a grain Field's whole payoff is now its larder
// credit — which `a_hay_field_publishes_the_fodder_it_credits…` and the food half above already own.

// ---------------------------------------------------------------------------------------------
// Integration: the labor arm credits the BAND's own trade_goods store.
// ---------------------------------------------------------------------------------------------

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = PINNED_SEED;
    config.grid_size = MECHANIC_GRID;
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
    generate_hydrology(&mut app.world);

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
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// The first land tile carrying a forage patch, in a totally-ordered `(y, x)` sweep — any real
/// ground will do: a Field's trade payoff reads the committed **species** off the patch, never the
/// tile's biome, so the tile only has to be somewhere a band can stand and work.
fn first_patch_tile(app: &App) -> (bevy::prelude::Entity, UVec2) {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    for y in 0..height {
        for x in 0..width {
            let coord = UVec2::new(x, y);
            let Some(entity) = app.world.resource::<TileRegistry>().index(x, y) else {
                continue;
            };
            if app.world.get::<Tile>(entity).is_some()
                && app
                    .world
                    .resource::<ForageRegistry>()
                    .patch(coord)
                    .is_some()
            {
                return (entity, coord);
            }
        }
    }
    panic!("the pinned map must carry at least one forage patch");
}

/// **Whose Field the fixture seats** — the harness's one faction, so the owner-lock the accrual
/// applies is satisfied.
const FIELD_OWNER: FactionId = FactionId(0);

/// Turn the patch at `coord` into a completed **Field** of `species` standing at `biomass`. Written
/// straight onto the registry: what is under test is the *harvest routing* of a finished rung, not
/// the build that gets there.
fn seat_field(app: &mut App, coord: UVec2, species: &str, biomass: f32) {
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch exists");
    patch.species = Some(species.to_string());
    // The rung is FINISHED here. A meter set to a bare `1.0` no longer completes anything now that a
    // job has a size (`docs/plan_unit_costed_work.md`), so this runs the real accrual.
    patch.complete_field(FIELD_OWNER);
    patch.carrying_capacity = biomass;
    patch.biomass = biomass;
}

/// Turn the patch at `coord` into a completed cotton **Field** standing at `biomass`.
fn seat_cotton_field(app: &mut App, coord: UVec2, biomass: f32) {
    seat_field(app, coord, "cotton", biomass);
}

fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
) -> bevy::prelude::Entity {
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(FORAGE_WORKERS as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
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
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    // Policy is irrelevant to a Field — the rung-3 branch resolves before the policy
                    // arms and `continue`s. Sustain is the harmless default.
                    target: LaborTarget::Forage {
                        tile: patch,
                        floor: 0.5,
                        species: None,
                    },
                    workers: FORAGE_WORKERS,
                    improvement: None,
                    kit: None,
                    improvement_workers: NO_CREW_ON_THIS_ACTIVITY,
                    maintain_workers: NO_CREW_ON_THIS_ACTIVITY,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// **Every material on the producing band's own `LocalStore`, summed** — batches beside the
/// `FOOD`/`FODDER` keys. **Fractional**: the batch store is fixed-point, so a sub-unit harvest
/// accumulates instead of rounding away.
fn band_materials(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the keeper band still exists")
        .stores
        .materials()
        .flat_map(|(_, batches)| batches.values())
        .map(|batch| batch.amount.to_f32())
        .sum()
}

/// **A cash Field credits the band's own MATERIAL store and (near) zero food** — and no fodder at
/// all. The batches sit on the *same* `LocalStore` as FOOD/FODDER: what a band grows stays where it
/// was grown until a supply network reaches it.
#[test]
fn a_cash_field_credits_its_materials_and_leaves_food_and_fodder_alone() {
    let mut app = spawn_world();
    let (tile, coord) = first_patch_tile(&app);
    let capacity = {
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let ground = app.world.get::<Tile>(tile).expect("tile exists");
        tile_forage_capacity(&labor.forage, ground)
    };
    seat_cotton_field(&mut app, coord, capacity);
    let keeper = spawn_forager(&mut app, tile, coord);

    assert_eq!(
        band_materials(&app, keeper),
        0.0,
        "no material before the turn"
    );
    app.world.run_system_once(advance_labor_allocation);

    assert!(
        band_materials(&app, keeper) > 0.0,
        "a cotton Field must credit the keeper band's own material batches"
    );
    let cohort = app.world.get::<PopulationCohort>(keeper).unwrap();
    assert!(
        cohort.stores.get(FOOD).to_f32() <= EPSILON,
        "a cash crop pays no food: {}",
        cohort.stores.get(FOOD).to_f32()
    );
    assert_eq!(
        cohort.stores.get(FODDER),
        scalar_zero(),
        "a cash crop pays no fodder"
    );
}

/// **A HAY FIELD'S WHOLE PRODUCT RIDES ITS OWN ROW** (issue #449) — the case the third account was
/// added for, and the mirror of the cash Field above.
///
/// `hay_grass` pays no provisions and no trade, so before `SourceYield::fodder` existed the row for
/// a fully productive Field read `actual 0 · trade 0` and every compact yield readout in the client
/// rendered `+0.00` — a live source indistinguishable from dead ground. The row now states its
/// fodder, and states **the number the band's `FODDER` store was actually credited**: asserted as
/// the store's own movement rather than against a re-derivation of `field_fodder`, per the §4.3
/// rule, so a readout that recomputed its own quote would fail here.
#[test]
fn a_hay_field_publishes_the_fodder_it_credits_and_nothing_in_the_other_two_accounts() {
    let mut app = spawn_world();
    let (tile, coord) = first_patch_tile(&app);
    let capacity = {
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let ground = app.world.get::<Tile>(tile).expect("tile exists");
        tile_forage_capacity(&labor.forage, ground)
    };
    seat_field(&mut app, coord, "hay_grass", capacity);
    let keeper = spawn_forager(&mut app, tile, coord);

    app.world.run_system_once(advance_labor_allocation);

    let credited = app
        .world
        .get::<PopulationCohort>(keeper)
        .expect("the keeper band still exists")
        .stores
        .get(FODDER)
        .to_f32();
    assert!(
        credited > 0.0,
        "the fixture must actually harvest hay, or the row assertions below are vacuous"
    );

    let row = app
        .world
        .get::<LaborAllocation>(keeper)
        .expect("the band forages")
        .last_yields
        .first()
        .expect("the Field assignment has a yield row")
        .clone();
    assert!(
        (row.fodder - credited).abs() <= EPSILON,
        "the row must publish the credited fodder, not recompute it: {} vs {credited}",
        row.fodder
    );
    assert_eq!(
        row.actual, 0.0,
        "hay is no food — this is the row that read +0.00"
    );
}

/// **THE PICKER'S MATERIAL QUOTE IS THE MATERIAL THE SIM CREDITS** (arc #527) — the material-side
/// restoration of the retired `the_picker_trade_payoff_matches_the_credited_store`, and the §4.3
/// "assert the quote against the payoff function" rule applied to the account a cash crop actually
/// pays into.
///
/// The labor arm's `credit_material_yield` and the crop picker's `commit_material_payoff` are one
/// seam: seed a Field at exactly the hypothetical patch the quote builds (this tile's own `K` for
/// the biome, at full standing crop — a Field neither raises nor lowers it, #433), staff it past the
/// collection cap so the quote's production basis and the payout's `min(production, collection)`
/// coincide, and the **credited store** must equal the quote per material.
///
/// **A quote that disagrees with what lands in the band's store is worse than no quote**, which is
/// why this is asserted against `LocalStore::material_total` — what the band ends up *holding* —
/// rather than against a re-derivation of the credit's arithmetic.
#[test]
fn the_picker_material_quote_is_the_material_the_sim_credits() {
    let mut app = spawn_world();
    let (tile, coord) = first_patch_tile(&app);

    // Build the quote for a biome cotton actually hosts, and reproduce the hypothetical patch's
    // standing crop so the sim's paid value and the quote read the identical biomass.
    let labor = labor();
    let flora = FloraConfig::builtin();
    let terrain = TerrainType::Floodplain;
    let quote_tile = UVec2::new(terrain as u32, 0);
    let quote_capacity = labor.forage.capacity_for(terrain);
    let composition = flora.composition(terrain);

    seat_cotton_field(&mut app, coord, quote_capacity);
    let keeper = spawn_forager(&mut app, tile, coord);
    app.world.run_system_once(advance_labor_allocation);

    let quoted = core_sim::commit_material_payoff(
        quote_tile,
        quote_capacity,
        "cotton",
        composition,
        &flora,
        &labor.forage,
        QUOTE_MULTIPLIER,
        RungKey::PlantField,
    );
    assert_eq!(
        quoted.len(),
        1,
        "a cotton Field's basket is 100% cotton, so it quotes exactly its one material: {quoted:?}"
    );
    let fibre = &quoted[0];
    assert_eq!(fibre.material, "fibre");
    assert!(
        fibre.amount > 0.0,
        "the fixture must quote a real material payoff, or the comparison below is two zeros"
    );

    let cohort = app
        .world
        .get::<PopulationCohort>(keeper)
        .expect("the keeper band still exists");
    let credited = cohort.stores.material_total(&fibre.material).to_f32();
    assert!(
        (credited - fibre.amount).abs() <= EPSILON * fibre.amount.max(1.0),
        "the credited {} must equal the picker's quote: credited {credited} vs quoted {}",
        fibre.material,
        fibre.amount
    );
}

/// **"NOTHING" IS AN EMPTY VECTOR, NEVER A ZERO — and a FIELD is where a food crop says it.**
///
/// The distinction is the field's whole contract: a client renders one row per entry, so an empty
/// quote is *no row* while a `0`-valued entry would read as a cash crop that pays badly.
///
/// **The rungs answer differently, and that is the model rather than an inconsistency.** A **Field**
/// is 100% its crop (#433), so a grain Field quotes nothing at all. A **tended patch** is a *weeded
/// basket* — the favored share rises but the volunteers are still standing — so committing to a
/// grain still quotes whatever fibre and leaf its neighbours pay, which is exactly what the turn
/// credits (`patch_material_yields` decomposes rather than averaging). That is the same fact the
/// food account already records for a rung-2 cash crop paying non-zero calories, read from the other
/// side.
#[test]
fn a_field_of_a_food_crop_quotes_an_empty_material_payoff_not_a_zero() {
    let labor = labor();
    let flora = FloraConfig::builtin();
    let forage = &labor.forage;
    let terrain = TerrainType::Floodplain;
    let tile = UVec2::new(terrain as u32, 0);
    let capacity = forage.capacity_for(terrain);
    let composition = flora.composition(terrain);
    let quote = |species: &str, rung| {
        core_sim::commit_material_payoff(
            tile,
            capacity,
            species,
            composition,
            &flora,
            forage,
            QUOTE_MULTIPLIER,
            rung,
        )
    };

    // A grain FIELD is one plant, and that plant is made of nothing anyone builds with.
    let grain_field = quote("wild_emmer", RungKey::PlantField);
    assert!(
        grain_field.is_empty(),
        "a grain Field names no material, so it must quote NO ROW rather than a zero: {grain_field:?}"
    );
    // …while the cash crop on the same ground at the same rung does quote one, or the assertion
    // above would pass against a seam that never returns anything.
    let cotton_field = quote("cotton", RungKey::PlantField);
    assert_eq!(
        cotton_field.len(),
        1,
        "a cotton Field is 100% cotton and quotes exactly its fibre: {cotton_field:?}"
    );
    assert!(cotton_field[0].amount > 0.0);

    // **A TENDED grain still quotes its neighbours' materials**, because weeding does not evict
    // them — and every row it does quote is strictly positive, never a published zero.
    let grain_tended = quote("wild_emmer", RungKey::PlantTended);
    assert!(
        grain_tended.iter().all(|row| row.amount > 0.0),
        "a quoted row is a row that pays: {grain_tended:?}"
    );

    // A plant that cannot climb the rung here quotes nothing either — the same "no row" reading,
    // reached down a different branch, and the one that makes `empty` unambiguous.
    let cannot_climb = quote_absent_species(&flora, tile, capacity);
    assert!(
        cannot_climb.is_empty(),
        "a species absent from this tile's basket cannot climb here, so it quotes no row"
    );
}

/// `commit_material_payoff` for a species this tile's basket does not carry — the `species_climbs`
/// refusal branch, kept out of the test body so the point above stays one line.
fn quote_absent_species(flora: &FloraConfig, tile: UVec2, capacity: f32) -> Vec<MaterialPayoff> {
    core_sim::commit_material_payoff(
        tile,
        capacity,
        "cotton",
        &[],
        flora,
        &labor().forage,
        QUOTE_MULTIPLIER,
        RungKey::PlantField,
    )
}
