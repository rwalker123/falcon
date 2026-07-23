//! **The Field — the plant ladder's rung 3** (`docs/plan_intensification_ladder.md` §2, slice 5).
//!
//! `Sow` is the plant twin of `Corral`: it **places a food source where you want it**. Once a faction
//! knows **Seed Selection** (earned by working tended patches — slice 4 earned it, this slice spends
//! it), a crew working a tile under `FollowPolicy::Sow` builds a Field on it over ~25 turns, and the
//! completed Field pays a *higher* managed harvest than the tended patch below it.
//!
//! Two things separate it from every other rung, and both are tested here:
//! - **It needs no source below it.** Seed travels, so hospitable ground with *no forage site at all*
//!   is a legal target and sowing it **creates** a patch. (`Corral`, by contrast, needs a herd you
//!   already tamed.)
//! - **It places, it does not conjure.** Only naturally food-bearing ground takes seed; rock, ice and
//!   desert need rung 4 (Worked Land). That gate lives in the command layer — see
//!   `server::tests::sow_rejected_on_ground_that_bears_no_food`.
//!
//! Harness mirrors `forage_cultivation.rs` (its rung-2 sibling) verbatim.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_cultivation, advance_forage_regrowth, advance_labor_allocation,
    default_species_for_rung, generate_hydrology, rung_site_refusal, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_forage, spawn_initial_world, tile_flora_composition,
    tile_forage_capacity, tile_is_fresh_watered, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, EcologyPhase, FactionId, FactionInventory, FaunaConfigHandle,
    FogRevealLedger, FollowPolicy, ForagePatch, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfig,
    LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, RungKey, SimulationConfig, SimulationTick, SiteRefusal,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle, FOOD,
    RUNG_TIMESCALE_UNSCALED, SEED_SELECTION_DISCOVERY_ID,
};

/// Grant faction-level **Seed Selection** directly via the ledger — the gate the `Sow` policy checks.
/// (How it is *earned* is slice 4's business and has its own tests; these are about spending it.)
fn grant_seed_selection(app: &mut App, faction: FactionId) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, SEED_SELECTION_DISCOVERY_ID, scalar_one());
}

/// Whole-worker head-count — large enough that the per-worker gather cap never binds, so every take
/// is **ceiling-bound**. (A managed harvest ignores head-count entirely, which is half the point.)
const FORAGE_WORKERS: u32 = 5000;

/// Float slack for provisions comparisons (fixed-point conversion + multiplication order).
const EPSILON: f32 = 1e-4;

/// What "pays nothing" means in provisions: freshly sown ground's take is a *fraction of the MSY of a
/// seed stock below its Allee threshold*, i.e. exactly zero — this is slack for the fixed-point grid,
/// not a tolerance for a real yield.
const NEAR_ZERO_PROVISIONS: f32 = 1e-3;

/// How small "a trickle" is: the whole bare-ground build averages under this fraction of the Field's
/// own per-turn harvest. Measured on the shipped dials it is ~6% (3.3 provisions across the 25-turn
/// build against 2.1/turn once the Field stands) — the bound is deliberately loose, since it is
/// asserting the *shape* (sowing bare ground is an investment, not a slow harvest), not a number.
const BUILD_TRICKLE_FRACTION: f32 = 0.1;

/// The **mechanic fixture's** grid — pinned *here*, deliberately not read from
/// `simulation_config.json`.
///
/// Every test below except the playability one asks a question about the *mechanic* ("does sowing
/// build a Field, does an abandoned Field go feral"), which has nothing to do with how big the
/// shipped map is. The fixture used to take its grid from the shipped config while pinning only the
/// seed, so **editing a gameplay lever silently changed what these tests measured** — and when
/// worldgen stopped putting sowable ground on the shipped map, all six mechanic tests failed for a
/// reason none of them was about. Pinning both halves makes the fixture immune to config edits; the
/// shipped map's playability is asserted separately, and *loudly*, by
/// [`the_shipped_map_carries_sowable_ground`].
const MECHANIC_GRID: UVec2 = UVec2::new(96, 64);

/// The seed both fixtures pin. The shipped `map_seed` is `0` = "roll from entropy", so a test that
/// did not pin one would ask a different question every run.
const PINNED_SEED: u64 = 119_304_647;

fn spawn_world() -> App {
    spawn_world_on(MECHANIC_GRID, PINNED_SEED)
}

fn spawn_world_on(grid_size: UVec2, seed: u64) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = grid_size;
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
    // **Hydrology is not optional here, and leaving it out is what broke this file.**
    // `spawn_initial_world` builds terrain; `generate_hydrology` is a separate Startup system, and it
    // is the *only* stage that stamps `RiverDelta`/`Floodplain` and sets the `river_edges` bits.
    // `plant:field`'s site rule wants rich ground **and fresh water**, and `tile_is_fresh_watered`
    // reads exactly those river edges — so a fixture that skipped hydrology was asking whether a map
    // *with no rivers on it* had riverbank farmland. It does not, and cannot, at any grid size or any
    // seed. Run the pipeline the game runs.
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
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// **The land's own verdict on a tile**, resolved through the *real* seam the sim uses
/// (`rung_site_refusal` + `tile_is_fresh_watered` against the `plant:field` rung's own
/// `site_requirement`) — never a restatement of the rule, so a retune of the floor or the water rule
/// moves these fixtures with the game. `None` = the ground will take seed.
fn site_verdict(app: &App, coord: UVec2) -> Option<SiteRefusal> {
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    let ground = app.world.get::<Tile>(entity).expect("tile exists");
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let fresh_water = tile_is_fresh_watered(ground, width, height, wrap, |neighbor| {
        app.world
            .resource::<TileRegistry>()
            .index(neighbor.x, neighbor.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
            .map(|tile| tile.terrain_tags)
    });
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    rung_site_refusal(
        ladder.rung(RungKey::PlantField),
        ground,
        &labor.forage,
        fresh_water,
    )
}

/// **The ground the ladder will take seed on** — rich *and* watered — scanned in a totally-ordered
/// `(y, x)` sweep (never map iteration order, the lesson of `7c09c7e`). Scarce by design: this is the
/// river-valley set, which is exactly why *which* tile a band can farm is a decision.
fn find_sowable_tile(app: &App) -> (bevy::prelude::Entity, UVec2) {
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
            if app.world.get::<Tile>(entity).is_some() && site_verdict(app, coord).is_none() {
                return (entity, coord);
            }
        }
    }
    panic!("the pinned map must carry sowable ground — rung 3 is unreachable without it");
}

/// **Sowable ground carrying a live patch**, primed to half its cap (Thriving, with regrowth
/// headroom) — the wild stand rung 2 works and rung 3 upgrades.
fn prime_thriving_patch(app: &mut App) -> (bevy::prelude::Entity, UVec2) {
    let (entity, coord) = find_sowable_tile(app);
    if app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .is_none()
    {
        // Sowable ground with no patch is (measurably) unreachable on a generated map, but the
        // fixture must not silently depend on that: seed one at the tile's own capacity.
        let capacity = {
            let labor = app.world.resource::<LaborConfigHandle>().get();
            let ground = app.world.get::<Tile>(entity).expect("tile exists");
            tile_forage_capacity(&labor.forage, ground)
        };
        let patch = ForagePatch::new(coord, capacity);
        app.world
            .resource_mut::<ForageRegistry>()
            .patches
            .insert(coord, patch);
    }
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.biomass = patch.carrying_capacity * 0.5;
        patch.ecology_phase = EcologyPhase::Thriving;
    }
    (entity, coord)
}

/// The plant a `Sow` on this tile would commit to — the same `default_species_for_rung` answer the
/// labor arm reaches, so a fixture can put a baseline patch on the *same* crop.
fn default_sowable_species(app: &App, coord: UVec2) -> Option<String> {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)?;
    let ground = app.world.get::<Tile>(entity)?;
    let composition = tile_flora_composition(&flora, &labor.forage, ground);
    default_species_for_rung(&composition, &flora, RungKey::PlantField)
}

/// **Sowable ground with NO forage site** — the create-from-nothing target, *constructed*.
///
/// **Read this before using it.** `Sow`'s headline case is qualifying ground carrying no forage site
/// at all (§2 — seed travels). **No such tile exists on a generated map today**: `classify_food_module`
/// tags essentially every biome, and `spawn_initial_forage` seeds a patch on every module tile with a
/// positive capacity — measured on the standard map: **2328 food-bearing tiles, 2328 patches, zero
/// bare**. So the state is built here by taking a real sowable tile and *removing* its patch, which is
/// exactly the world the code path is written for. The path is real and correct; only worldgen
/// currently never produces its input. See `docs/plan_intensification_ladder.md` §2.
fn find_bare_sowable_tile(app: &mut App) -> (bevy::prelude::Entity, UVec2) {
    let (entity, coord) = find_sowable_tile(app);
    app.world
        .resource_mut::<ForageRegistry>()
        .patches
        .remove(&coord);
    (entity, coord)
}

fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    policy: FollowPolicy,
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
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
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
                    target: LaborTarget::Forage {
                        tile: patch,
                        policy,
                        species: None,
                    },
                    workers: FORAGE_WORKERS,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// One turn's forage pipeline in stage order: Logistics (regrowth, feral decay) then Population
/// (labor allocation resolves the take and accrues the investment).
fn run_turns_with_forage(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_forage_regrowth);
        app.world.run_system_once(advance_cultivation);
        app.world.run_system_once(advance_labor_allocation);
    }
}

/// Turns with no band working the ground: only the Logistics-stage systems run — the abandonment case.
fn run_turns_untended(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_forage_regrowth);
        app.world.run_system_once(advance_cultivation);
    }
}

fn provisions_f32(app: &mut App) -> f32 {
    let mut total = 0.0f32;
    let mut query = app.world.query::<&PopulationCohort>();
    for cohort in query.iter(&app.world) {
        if cohort.faction == FactionId(0) {
            total += cohort.stores.get(FOOD).to_f32();
        }
    }
    total
}

fn field_progress_of(app: &App, coord: UVec2) -> f32 {
    app.world
        .resource::<ForageRegistry>()
        .patch(coord)
        .map(|patch| patch.field_progress)
        .unwrap_or(0.0)
}

/// The `plant:field` rung's build dials, read off the ladder — the same seam the sim drives sowing
/// with, so a retune moves the tests with the game rather than against it.
fn field_build(app: &App) -> (f32, f32) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let field = ladder.rung(RungKey::PlantField);
    (
        field.build_accrual(FollowPolicy::Sow, true, RUNG_TIMESCALE_UNSCALED),
        field.build_decay(RUNG_TIMESCALE_UNSCALED),
    )
}

/// **PLAYABILITY, not mechanic — this is the check that caught worldgen dropping the Field rung.**
///
/// Every other test in this file runs on [`MECHANIC_GRID`], pinned so a config edit cannot move what
/// it measures. This one does the opposite **on purpose**: it reads the **shipped** `grid_size` out
/// of `simulation_config.json` and asks whether the map a player actually gets carries ground that
/// rung 3 can take seed on. If it does not, `plant:field` is unreachable in the real game — the
/// ladder's top plant rung is decoration — and no mechanic test would ever say so.
///
/// It asserts the *existence* of sowable ground, not a count: the count is an emergent property of
/// the heightfield and legitimately moves with worldgen tuning. Zero is the only unplayable answer.
#[test]
fn the_shipped_map_carries_sowable_ground() {
    let shipped_grid = SimulationConfig::builtin().grid_size;
    let app = spawn_world_on(shipped_grid, PINNED_SEED);

    let sowable = (0..shipped_grid.y)
        .flat_map(|y| (0..shipped_grid.x).map(move |x| UVec2::new(x, y)))
        .filter(|coord| {
            app.world
                .resource::<TileRegistry>()
                .index(coord.x, coord.y)
                .and_then(|entity| app.world.get::<Tile>(entity))
                .is_some()
                && site_verdict(&app, *coord).is_none()
        })
        .count();

    assert!(
        sowable > 0,
        "INTENSIFICATION RUNG 3 IS UNREACHABLE ON THE SHIPPED MAP.\n\
         The shipped grid ({}x{}, seed {PINNED_SEED}) carries 0 tiles that satisfy the \
         `plant:field` rung's site requirement (rich enough ground, on fresh water), so no band can \
         ever sow a Field and the plant ladder dead-ends at rung 2.\n\
         This is a WORLDGEN result, not a test-fixture problem: the rule is emergent from the \
         heightfield. Do NOT fix it by lowering `min_forage_capacity`, relaxing the fresh-water \
         rule, or stamping deltas anywhere but real river mouths — shape the generated field \
         instead (`heightfield::apply_continental_bias`).",
        shipped_grid.x,
        shipped_grid.y,
    );
}

/// **The point of the slice: `Sow` PLACES a source.** Hospitable ground carrying no forage site at
/// all is sown into a genuinely new patch — seed travels, so rung 3 needs no rung below it on the
/// tile (the one place the two food webs legitimately differ: `Corral` needs a herd you already
/// tamed). The new patch is an ordinary one: the **tile's own** biome capacity, a seed-stock standing
/// crop, normal logistic regrowth.
#[test]
fn sowing_bare_hospitable_ground_creates_a_patch_and_builds_a_field() {
    let mut app = spawn_world();
    let (tile, coord) = find_bare_sowable_tile(&mut app);
    grant_seed_selection(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sow);

    let expected_capacity = {
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let ground = app.world.get::<Tile>(tile).unwrap();
        tile_forage_capacity(&labor.forage, ground)
    };

    // One turn of work and the seed is in the ground.
    run_turns_with_forage(&mut app, 1);
    {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).expect("the sow created a patch");
        assert_eq!(
            patch.carrying_capacity, expected_capacity,
            "a sown patch takes the TILE's own biome capacity — the same table a wild patch reads"
        );
        assert!(
            patch.biomass > 0.0 && patch.biomass < expected_capacity * 0.5,
            "sown ground starts as a seed stock, not a standing crop: {}",
            patch.biomass
        );
    }

    // Sustained work completes the Field in the rung's own `1 / progress_per_turn` turns.
    let (progress_per_turn, _) = field_build(&app);
    let turns_to_sow = (1.0 / progress_per_turn).ceil() as u32;
    run_turns_with_forage(&mut app, turns_to_sow);
    let registry = app.world.resource::<ForageRegistry>();
    let patch = registry.patch(coord).expect("patch persists");
    assert!(
        patch.is_field(),
        "sustained Sow work completes the field: progress {}",
        patch.field_progress
    );
    assert_eq!(patch.owner, Some(FactionId(0)), "the sower owns it");
    assert!(
        !patch.is_cultivated(),
        "a bare-ground Field was never tended — rung 3 here stands on the tile, not on rung 2"
    );
    assert_eq!(
        registry.cultivated_count(FactionId(0)),
        1,
        "a Field is a completed plant improvement — it must read as domestication, not as less than \
         the rung below it"
    );
}

/// **A bare-ground sow is very nearly pure investment.** The rung's dip is a *fraction of what the
/// source would otherwise pay*, and ground you have only just seeded pays nothing at all — so the
/// build's opening turns buy no food whatever, and the whole 25-turn build buys a rounding error
/// against what the same ground yields once the Field stands. The crop grows *into* its dip as the
/// stand climbs past its Allee threshold, which is honest: by then there is a little something there.
#[test]
fn a_bare_ground_sow_pays_almost_nothing_while_it_builds_then_pays_the_field() {
    let mut app = spawn_world();
    let (tile, coord) = find_bare_sowable_tile(&mut app);
    grant_seed_selection(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sow);

    // The opening turns pay NOTHING: a fraction of the MSY of a seed stock below its Allee threshold
    // is a fraction of zero. There is nothing there yet — that is the whole cost of the rung.
    run_turns_with_forage(&mut app, 1);
    assert!(
        provisions_f32(&mut app) < NEAR_ZERO_PROVISIONS,
        "freshly sown ground has nothing to take a fraction of"
    );

    let (progress_per_turn, _) = field_build(&app);
    let turns_to_sow = (1.0 / progress_per_turn).ceil() as u32;
    run_turns_with_forage(&mut app, turns_to_sow);
    let while_building = provisions_f32(&mut app);
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_field(),
        "the field is standing at the end of the build"
    );

    // The payoff lands the moment the Field is complete — and dwarfs the whole build's takings.
    let before = provisions_f32(&mut app);
    run_turns_with_forage(&mut app, 1);
    let field_yield = provisions_f32(&mut app) - before;
    assert!(
        field_yield > NEAR_ZERO_PROVISIONS,
        "the completed Field pays a real harvest: {field_yield}"
    );
    let while_building_per_turn = while_building / turns_to_sow as f32;
    assert!(
        while_building_per_turn < BUILD_TRICKLE_FRACTION * field_yield,
        "the whole build is a trickle beside the Field it buys — {while_building_per_turn}/turn \
         over {turns_to_sow} turns against {field_yield}/turn once it stands"
    );
}

/// **THE LADDER MUST CLIMB: wild < tended < Field.** Same tile, same biomass, same workers, same
/// policy — the only difference is which rung the patch stands on. Runs the labor arm alone (no
/// Logistics pass), so neither regrowth nor the feral decay can move one rung relative to another.
///
/// **Retargeted, not weakened** (slice 7). This test used to assert `field / tended == 2.0` exactly.
/// That ratio was never a design claim — it was an artifact of the two rungs sharing a *shape* (both
/// paid `biomass × rate`, so the ratio was the ratio of two levers). Rung 2 is now a **wild stand on
/// a boosted curve** and rung 3 a **managed rate**, so the ratio is a function of `B/K` rather than a
/// config identity, and pinning it would pin the very conflation this slice removed. In its place each
/// rung is pinned against **its own model** — strictly sharper than the ratio was — plus the
/// monotonicity, which is the actual claim. The wild rung joins them: the ladder's bottom step was
/// never covered here, and "tended beats wild" is exactly the incentive to cultivate at all.
#[test]
fn the_plant_ladder_climbs_wild_then_tended_then_field() {
    /// One turn's Sustain harvest from the same primed patch, standing on the given rung, plus the
    /// `(biomass, capacity)` it was taken from.
    fn rung_yield(rung: Option<bool>) -> (f32, f32, f32) {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        let (biomass, capacity) = {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            match rung {
                Some(true) => patch.field_progress = 1.0,
                Some(false) => patch.cultivation_progress = 1.0,
                None => {}
            }
            if rung.is_some() {
                patch.owner = Some(FactionId(0));
            }
            (patch.biomass, patch.carrying_capacity)
        };
        spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);
        app.world.run_system_once(advance_labor_allocation);
        (provisions_f32(&mut app), biomass, capacity)
    }

    let (wild, biomass, capacity) = rung_yield(None);
    let (tended, _, _) = rung_yield(Some(false));
    let (field, _, _) = rung_yield(Some(true));

    let forage = &LaborConfig::builtin().forage;
    let gain = forage.cultivation.tended_regrowth_gain;
    let _ = capacity;

    // **Each rung against its own model — stated as what its config lever MEANS.**
    //
    // Rungs 1–2 are both *gathered*, off the same MSY curve at the same biomass, so the only thing
    // between them is the tended curve's `r` multiplier: the rung-2 payoff **is** the gain, exactly and
    // scale-freely. (Asserting the two absolute MSYs instead would need a second copy of the logistic
    // curve out here — and pinning them against the forecast's own numbers would be two copies
    // agreeing with each other while both disagree with the take, which this repo has paid for once.)
    assert!(wild > 0.0, "baseline wild skim must be positive");
    assert!(
        (tended - gain * wild).abs() < 1e-3,
        "a tended patch skims exactly `tended_regrowth_gain ×` the same patch wild — the rung's whole \
         payoff, and the intensification incentive: {tended} vs {} (gain {gain})",
        gain * wild
    );
    // Rung 3 is *managed*: a flat rate on the standing crop, drawn from no curve at all.
    assert!(
        (field - biomass * forage.cultivation.field_provisions_per_biomass).abs() < 1e-3,
        "a Field pays its managed rate on the standing crop: {field} vs {}",
        biomass * forage.cultivation.field_provisions_per_biomass
    );

    // **And the ladder climbs.** This is the claim; the three pins above are how it is bought.
    assert!(
        wild < tended && tended < field,
        "the plant ladder must be monotone: wild {wild} → tended {tended} → field {field}"
    );
}

/// **Sowing a patch that is already tended still costs the rung's dip.** Upgrading rung 2 → rung 3 is
/// a Cultivate-shaped verb like every other rung-transition: the source pays only a fraction of what
/// it would otherwise hand you while the crew works. (On bare ground that fraction is a fraction of
/// nothing — see above; here it bites a real harvest.)
#[test]
fn sowing_a_tended_patch_pays_the_dip_then_upgrades_it() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.cultivation_progress = 1.0;
        patch.owner = Some(FactionId(0));
    }
    grant_seed_selection(&mut app, FactionId(0));
    let dip = {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        ladder
            .rung(RungKey::PlantField)
            .yield_fraction_while_building()
            .expect("the field rung is an investment")
    };

    // The tended harvest this patch would pay if nobody were upgrading it. **Committed to the same
    // crop**: a Sow commits the ground to one named plant (Flora Roster S1), which changes its
    // conversion rate, so an uncommitted baseline would be measuring the commitment rather than the
    // rung's dip.
    let tended_yield = {
        let mut baseline = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut baseline);
        let crop = default_sowable_species(&baseline, coord);
        {
            let mut registry = baseline.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.cultivation_progress = 1.0;
            patch.owner = Some(FactionId(0));
            patch.species = crop;
        }
        spawn_forager(&mut baseline, tile, coord, FollowPolicy::Sustain);
        baseline.world.run_system_once(advance_labor_allocation);
        provisions_f32(&mut baseline)
    };

    spawn_forager(&mut app, tile, coord, FollowPolicy::Sow);
    app.world.run_system_once(advance_labor_allocation);
    let while_sowing = provisions_f32(&mut app);
    assert!(
        (while_sowing - dip * tended_yield).abs() < EPSILON,
        "upgrading pays the rung's dip on the tended harvest: {while_sowing} vs {}",
        dip * tended_yield
    );

    // Worked to completion the patch stands on rung 3 — and stops paying the dip.
    let (progress_per_turn, _) = field_build(&app);
    run_turns_with_forage(&mut app, (1.0 / progress_per_turn).ceil() as u32);
    let patch_is_field = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .unwrap()
        .is_field();
    assert!(patch_is_field, "sustained Sow upgrades the tended patch");
    let before = provisions_f32(&mut app);
    run_turns_with_forage(&mut app, 1);
    let after_completion = provisions_f32(&mut app) - before;
    assert!(
        after_completion > tended_yield,
        "once the Field stands the dip stops and it out-pays the patch it replaced: \
         {after_completion} vs {tended_yield}"
    );
}

/// **An abandoned Field goes feral — one rule for the whole plant web.** Walk away and it reverts to
/// a wild gather patch after a single untended turn (exactly as an abandoned tended patch does), then
/// bleeds to nothing over ~`1 / decay_per_turn` turns, ownership lapsing at zero. It does *not* step
/// down to a tended patch on the way: that would pay the deserter rung 2's managed yield for free.
#[test]
fn an_abandoned_field_goes_feral_and_fully_lapses() {
    let mut app = spawn_world();
    let (tile, coord) = find_bare_sowable_tile(&mut app);
    grant_seed_selection(&mut app, FactionId(0));
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sow);
    let (progress_per_turn, decay_per_turn) = field_build(&app);
    assert!(decay_per_turn > 0.0, "an unworked field must bleed");
    run_turns_with_forage(&mut app, (1.0 / progress_per_turn).ceil() as u32);
    assert!(app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .unwrap()
        .is_field());

    // The crew walks off.
    app.world.despawn(band);

    // Two untended turns revert it to a wild gather patch: the feral pass reads a flag the labor arm
    // wrote **last** turn (Logistics runs before Population — the deliberate one-turn lag), so the
    // first pass after the crew leaves still sees the ground as worked and spares it.
    run_turns_untended(&mut app, 2);
    {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).unwrap();
        assert!(
            !patch.is_field(),
            "one untended turn takes a field feral: progress {}",
            patch.field_progress
        );
        assert!(
            !patch.is_cultivated(),
            "it reverts to WILD, not to a free tended patch"
        );
        assert_eq!(
            registry.cultivated_count(FactionId(0)),
            0,
            "a feral field is no longer a plant improvement"
        );
    }

    // Left alone it bleeds all the way to nothing, and ownership lapses with it.
    run_turns_untended(&mut app, (1.0 / decay_per_turn).ceil() as u32 + 2);
    let registry = app.world.resource::<ForageRegistry>();
    let patch = registry.patch(coord).unwrap();
    assert_eq!(patch.field_progress, 0.0, "the investment fully lapses");
    assert_eq!(patch.owner, None, "ownership lapses once nothing is left");
    // The patch itself survives — plants reseed, so the stand you planted stays on the map as wild
    // ground (patches never despawn).
    assert!(patch.biomass > 0.0);
}

/// The `Sow` gate at the sim level: without **Seed Selection** the ground takes no seed at all —
/// neither a patch nor progress. (The command layer refuses it up front with a reason naming the
/// knowledge; this guards the system underneath it, which is what an `assign_labor … sow` reaches.)
#[test]
fn sow_seeds_nothing_without_seed_selection() {
    let mut app = spawn_world();
    let (tile, coord) = find_bare_sowable_tile(&mut app);
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sow);

    run_turns_with_forage(&mut app, 30);

    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .is_none(),
        "an ungated sow must not put seed in the ground"
    );
    assert_eq!(field_progress_of(&app, coord), 0.0);
}
