//! **The Field — the plant ladder's rung 3** (`docs/plan_intensification_ladder.md` §2, slice 5).
//!
//! `Sow` is the plant twin of `Corral`: it **places a food source where you want it**. Once a faction
//! knows **Seed Selection** (earned by working tended patches — slice 4 earned it, this slice spends
//! it), a crew working a tile under `Improvement::Sow` builds a Field on it over ~25 turns, and the
//! completed Field pays a *higher* managed harvest than the tended patch below it.
//!
//! What separates it from the rung below is **what it commits to, not where it may stand**:
//! - **It commits the ground to ONE crop**, where `Cultivate` merely weeds the basket in the tile's
//!   favour. Both stand on a **gathering site**; rung 3 adds fresh water on top, because seed travels
//!   and water does not.
//! - **It is still site-bound.** Ground nobody gathers needs rung 4 (Farm) — that is the first rung
//!   to drop `requires_gathering_site`, and dropping it is the whole of what Farm unlocks. The gate
//!   lives in the command layer — see `server::tests::sow_rejected_on_ground_nobody_gathers`.
//!
//! **§2 used to say the opposite** — *"`Sow` needs nothing … qualifying ground with no forage site is
//! a legal, indeed the interesting, target"* — and that was reversed deliberately: gathering itself is
//! site-bound, so ground a band could sow but never work existed on paper only. See `validate_sow`.
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
    FoodSiteRegistry, ForagePatch, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, Improvement, LaborAllocation, LaborAssignment,
    LaborConfig, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, RungKey, SimulationConfig, SimulationTick,
    SiteRefusal, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry,
    WellbeingConfigHandle, FOOD, NO_BUILD_GEAR, RUNG_COST_UNSCALED, SEED_SELECTION_DISCOVERY_ID,
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

/// **One forager**, so the crew's throughput is the binding term rather than the patch's standing
/// crop. Since `docs/plan_harvest_floor.md` §3.1 the build dip multiplies `workers × per_worker`
/// rather than the take ceiling, so it is invisible at a staffing the stock binds — a build costs
/// yield only while hands are the scarce thing.
const SOLE_FORAGER: u32 = 1;

/// Float slack for provisions comparisons (fixed-point conversion + multiplication order).
const EPSILON: f32 = 1e-4;

/// **The floor at which `learn_multiplier` is exactly ×1.0** — the food peak, and the floor a fresh
/// assignment carries. Passed wherever a build rate is read for its *stated* pace rather than for a
/// floor's fraction of it (`docs/plan_harvest_floor.md` §3).
const FOOD_PEAK_FLOOR: f32 = core_sim::MSY_BIOMASS_FRACTION;

/// What "pays nothing" means in provisions: freshly sown ground's take is a *fraction of the MSY of a
/// seed stock below its Allee threshold*, i.e. exactly zero — this is slack for the fixed-point grid,
/// not a tolerance for a real yield.
const NEAR_ZERO_PROVISIONS: f32 = 1e-3;

/// How small "a trickle" is: the whole bare-ground build averages under this fraction of the Field's
/// own per-turn harvest. Measured on the shipped dials it is **~13%** (0.19/turn across the 25-turn
/// build against 1.49/turn once the Field stands) — the bound is deliberately loose, since it is
/// asserting the *shape* (sowing bare ground is an investment, not a slow harvest), not a number.
///
/// **It read ~6% until the build crew came apart from [`FORAGE_WORKERS`]**
/// (`docs/plan_unit_costed_work.md` §1.2 — the crew is the build's throughput now, so a build
/// fixture staffs [`sow_crew`]). A three-hand Sow takes far less than the ceiling every turn, so the
/// young stand it is seeding *grows* under it instead of being held at its floor — and a bigger
/// standing crop means a bigger dipped take. The share moved because the fixture's staffing did, not
/// because the rung got cheaper.
const BUILD_TRICKLE_FRACTION: f32 = 0.2;

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
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
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
        app.world.resource::<FoodSiteRegistry>().is_site(coord),
        fresh_water,
    )
}

/// **The ground the ladder will take seed on** — a **watered gathering site that grows something
/// sowable** — scanned in a totally-ordered `(y, x)` sweep (never map iteration order, the lesson of
/// `7c09c7e`). Scarce by design: sites are few, and rung 3 narrows them to the watered ones, which is
/// exactly why *which* tile a band can farm is a decision.
///
/// **THE CROP TERM IS NOT PART OF THE SITE RULE, AND IS IN THIS FIXTURE ON PURPOSE.** `site_verdict`
/// answers about the GROUND; whether any plant in the tile's realized basket can climb to `field` is
/// a separate question the labor arm asks (`default_species_for_rung` → no commit, no accrual). The
/// two came apart when rung 3 swapped its 195 fertility floor for the gathering-site rule: the floor
/// used to admit only the river-deposit class, whose baskets are full of `field`-ceiling staples, so
/// "the ground takes seed" implied "something here can be sown". A site on a fishery or an alpine
/// shelf satisfies the site rule and grows nothing sowable. Every test below is about what a Sow
/// DOES once underway, so each needs ground where it can actually get underway.
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
            if app.world.get::<Tile>(entity).is_some()
                && site_verdict(app, coord).is_none()
                && default_sowable_species(app, coord).is_some()
            {
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
        // **Above Sustain's escapement floor** (`K/2`): at the floor exactly a Sustain gather has
        // nothing standing above it and every rung reads `+0.00`
        // (`docs/plan_harvest_floor.md` §1), which would make this ladder a comparison of zeros.
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
        patch.ecology_phase = EcologyPhase::Thriving;
    }
    (entity, coord)
}

/// **The patch's standing crop as a fraction of its capacity** — above Sustain's escapement floor
/// (`fauna::MSY_BIOMASS_FRACTION`, `K/2`), so a Sustain gather has stock standing above it.
const STOCKED_STANDING_CROP: f32 = 0.8;

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
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    default_species_for_rung(&composition, &flora, RungKey::PlantField)
}

/// **A sowable site with NO forage patch on it** — the create-from-nothing target, *constructed*.
///
/// **Read this before using it.** `Sow` creating a patch out of nothing was once the rung's headline
/// case: §2 read "seed travels", so any qualifying ground was a legal target whether or not anything
/// grew there. **That is no longer what rung 3 is for.** It now requires a gathering site, and a
/// gathering site is curated onto a tile that carries a food module — which is exactly the tile
/// `spawn_initial_forage` seeds a patch on. Moving "seed travels" up to rung 4 (Farm) is what gives
/// that rung its identity; see `validate_sow`.
///
/// So the create-from-nothing branch is now **near-dead rather than merely unexercised**, and the
/// tests below assert a code path the shipped game reaches only through one narrow gap: curation
/// admits any tile with a module, including the handful whose biome has **zero** forage capacity
/// (`SaltFlat` → SemiAridScrub, `HydrothermalVentField` → WetlandSwamp), and `spawn_initial_forage`
/// skips those. A site there is a gathering site with nothing to gather. That gap is a worldgen
/// question, not this rung's, and it is filed rather than fixed here.
///
/// The state is built by taking a real sowable site and *removing* its patch, which is the world the
/// code path is written for. See `docs/plan_intensification_ladder.md` §2.
fn find_bare_sowable_tile(app: &mut App) -> (bevy::prelude::Entity, UVec2) {
    let (entity, coord) = find_sowable_tile(app);
    app.world
        .resource_mut::<ForageRegistry>()
        .patches
        .remove(&coord);
    (entity, coord)
}

/// A band foraging `patch`. `improvement` is the second axis (issue #442): `None` for a plain
/// gather, `Some(Improvement::Sow)` for a crew putting a Field in. The **stance** stays `Sustain`
/// throughout this file — these tests measure the `Sow` build, not the harvest pressure beside it.
fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    improvement: Option<Improvement>,
) -> bevy::prelude::Entity {
    spawn_forager_of(app, tile, patch, improvement, FORAGE_WORKERS)
}

/// **A crew BUILDING the patch, staffed at the rung's own [`sow_crew`]** — not at
/// [`FORAGE_WORKERS`]. See `sow_crew` for why the two numbers had to come apart.
fn spawn_builder(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    improvement: Improvement,
) -> bevy::prelude::Entity {
    let crew = match improvement {
        Improvement::Cultivate => app
            .world
            .resource::<LadderConfigHandle>()
            .get()
            .rung(RungKey::PlantTended)
            .build_crew_needed()
            .expect("the tended rung declares a crew"),
        _ => sow_crew(app),
    };
    spawn_forager_of(app, tile, patch, Some(improvement), crew)
}

/// **A completed plant rung, seated at the LADDER's own cost.** The feral bleed is an absolute
/// number of work units per turn (a fraction of that rung's cost), so a fixture seated at a nominal
/// one-unit job would lapse to nothing in a single bleeding turn.
fn seat_completed_rung(app: &mut App, coord: UVec2, rung: RungKey) {
    let cost = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(rung)
        .build_cost(RUNG_COST_UNSCALED)
        .expect("the rung builds");
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch exists");
    match rung {
        RungKey::PlantField => {
            patch.field_progress = cost;
            patch.field_cost = cost;
        }
        _ => {
            patch.cultivation_progress = cost;
            patch.cultivation_cost = cost;
        }
    }
}

/// [`spawn_forager`] with an explicit head-count — the dip test needs a crew the carry binds.
fn spawn_forager_of(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    improvement: Option<Improvement>,
    foragers: u32,
) -> bevy::prelude::Entity {
    let policy = 0.5;
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(foragers as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
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
                        floor: policy,
                        species: None,
                    },
                    workers: foragers,
                    improvement,
                    kit: None,
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
/// **The `plant:field` rung's neglect grace** — the consecutive un-worked turns the feral bleed
/// forgives before it starts. Read off the ladder, never restated.
fn field_grace(app: &App) -> u32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantField)
        .neglect_grace_turns()
}

/// **What this file's crew produces on the `plant:field` rung in one turn, and the rung's feral
/// bleed** — both in absolute work units. The accrual is read at [`SOW_CREW`], the head count the
/// build fixtures actually staff, because the crew *is* the throughput now
/// (`docs/plan_unit_costed_work.md` §1.2).
fn field_build(app: &App) -> (f32, f32) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let field = ladder.rung(RungKey::PlantField);
    (
        field.build_accrual(
            Some(Improvement::Sow),
            true,
            FOOD_PEAK_FLOOR,
            sow_crew(app),
            NO_BUILD_GEAR,
        ),
        field.build_decay(RUNG_COST_UNSCALED),
    )
}

/// **The crew a BUILD fixture staffs, and it is deliberately NOT [`FORAGE_WORKERS`].** 5000 is
/// chosen so a *take* is ceiling-bound rather than labor-bound; it became a **build-pacing** number
/// only when the crew stopped being capped, and at that head count a 75-unit Sow finishes in a
/// single turn — leaving no part-sown ground for a decay, a grace or a completion test to stand on.
/// The rung's own `crew_needed` is the staffing the shipped cost was priced against
/// (`docs/plan_unit_costed_work.md` §3). The one-turn over-crewed build is real and pinned on
/// purpose by `forage_cultivation::over_crewing_a_build_is_no_longer_capped`.
fn sow_crew(app: &App) -> u32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantField)
        .build_crew_needed()
        .expect("the field rung declares a crew")
}

/// The whole `plant:tended` job, in work units — the completed rung-2 meter's value.
fn tended_cost(app: &App) -> f32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .build_cost(RUNG_COST_UNSCALED)
        .expect("the tended rung builds")
}

/// The whole `plant:field` job, in work units.
fn field_cost(app: &App) -> f32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantField)
        .build_cost(RUNG_COST_UNSCALED)
        .expect("the field rung builds")
}

/// **Turns [`SOW_CREW`] needs to raise a whole Field**, `ceil(work_cost / work per turn)` — turns are
/// an output now, so a bare `1.0 / rate` no longer means anything.
fn turns_to_sow(app: &App) -> u32 {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let field = ladder.rung(RungKey::PlantField);
    core_sim::build_turns_remaining(
        field
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the rung builds"),
        core_sim::RUNG_UNSTARTED,
        field_build(app).0,
    )
    .expect("a staffed Sow finishes")
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
    spawn_builder(&mut app, tile, coord, Improvement::Sow);

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
    let turns_to_sow = turns_to_sow(&app);
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
    let band = spawn_builder(&mut app, tile, coord, Improvement::Sow);

    // The opening turns pay NOTHING: a fraction of the MSY of a seed stock below its Allee threshold
    // is a fraction of zero. There is nothing there yet — that is the whole cost of the rung.
    run_turns_with_forage(&mut app, 1);
    assert!(
        provisions_f32(&mut app) < NEAR_ZERO_PROVISIONS,
        "freshly sown ground has nothing to take a fraction of"
    );

    let turns_to_sow = turns_to_sow(&app);
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

    // **The payoff is read at a crew that can CARRY it.** A Field's managed harvest is capped by
    // collection (`managed_per_worker_yield`), and the build crew above is the *build's* staffing,
    // not the Field's — reading the payoff at three hands would compare an investment against a
    // labor-bound harvest rather than against the rung it bought.
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("band exists")
        .assignments[0]
        .workers = FORAGE_WORKERS;
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

/// **THE LADDER MUST CLIMB: wild ≤ tended < Field** (on a *bare* patch). Same tile, same biomass,
/// same workers, same policy — the only difference is which rung the patch stands on. Runs the labor
/// arm alone (no Logistics pass), so neither regrowth nor the feral decay can move one rung.
///
/// **Retargeted twice.** It once pinned `field / tended == 2.0`; slice 7 replaced that with each rung
/// against its own model (rung 2 a curve, rung 3 a managed rate). **Flora Roster S2 then retired the
/// tended regrowth boost** (`tended_regrowth_gain` → neutral `1.0`), so a *bare* tended patch — no crop
/// committed here — now regrows exactly as fast as wild: the wild↔tended step is `≤`, not `<`.
/// Tending's payoff over wild moved to **concentration + conversion** (a committed crop), pinned by
/// `flora_commitment.rs` / `flora_roster.rs`; this bare-rung test keeps the strict `tended < Field`
/// step (which the neutral gain only widens) and each rung against its own model.
#[test]
fn the_plant_ladder_climbs_wild_then_tended_then_field() {
    /// One turn's Sustain harvest from the same primed patch, standing on the given rung, plus the
    /// `(biomass, capacity)` it was taken from.
    fn rung_yield(rung: Option<bool>) -> (f32, f32, f32, f32) {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        match rung {
            Some(true) => seat_completed_rung(&mut app, coord, RungKey::PlantField),
            Some(false) => seat_completed_rung(&mut app, coord, RungKey::PlantTended),
            None => {}
        }
        let (biomass, capacity) = {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            if rung.is_some() {
                patch.owner = Some(FactionId(0));
            }
            (patch.biomass, patch.carrying_capacity)
        };
        // **The tile's own basket rate** (#433) — with no crop committed, every rung on this patch
        // converts at it, so it is the one term all three expectations below share.
        let basket_rate = {
            let labor = app.world.resource::<LaborConfigHandle>().get();
            let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
            let map_seed = app.world.resource::<SimulationConfig>().map_seed;
            let ground = app.world.get::<Tile>(tile).expect("the primed tile");
            let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
            let wild = core_sim::ForagePatch::new(coord, capacity);
            core_sim::patch_provisions_per_biomass(&wild, &composition, &flora, &labor.forage)
        };
        spawn_forager(&mut app, tile, coord, None);
        app.world.run_system_once(advance_labor_allocation);
        (provisions_f32(&mut app), biomass, capacity, basket_rate)
    }

    let (wild, biomass, capacity, basket_rate) = rung_yield(None);
    let (tended, _, _, _) = rung_yield(Some(false));
    let (field, _, _, _) = rung_yield(Some(true));

    let forage = &LaborConfig::builtin().forage;
    let gain = forage.cultivation.tended_regrowth_gain;
    let _ = capacity;

    // **Each rung against its own model — stated as what its config lever MEANS.**
    //
    // Rungs 1–2 are both *gathered*, off the same MSY curve at the same biomass, so the only thing
    // between them is the tended curve's `r` multiplier: the bare rung-2 payoff **is** the gain, exactly
    // and scale-freely. Since S2 that gain is neutral (`1.0`), so a bare tended patch reads exactly wild
    // here — tending's payoff over wild is carried by a committed crop (conversion), not this curve.
    assert!(wild > 0.0, "baseline wild skim must be positive");
    assert!(
        (tended - gain * wild).abs() < 1e-3,
        "a bare tended patch skims exactly `tended_regrowth_gain ×` the same patch wild — neutral at \
         S2's gain of {gain}: {tended} vs {}",
        gain * wild
    );
    // Rung 3 is *managed*: a flat rate on the standing crop, drawn from no curve at all — scaled by
    // the projected basket's quality against the wild baseline. With **no crop committed** that
    // basket is the tile's own, so the factor is this ground's own richness rather than a crop's
    // (a *sown* Field's basket is 100% its crop, and `flora_f4_cash.rs` pins that arm).
    let field_quality = basket_rate / forage.provisions_per_biomass;
    let expected_field = biomass * forage.cultivation.field_provisions_per_biomass * field_quality;
    assert!(
        (field - expected_field).abs() < 1e-3,
        "a Field pays its managed rate on the standing crop: {field} vs {expected_field}"
    );

    // **And the ladder climbs.** This is the claim; the three pins above are how it is bought. Since S2
    // the bare wild↔tended step is `≤` (a neutral tended patch with no crop equals wild); the strict
    // climb to the Field survives and the neutral gain only widens it.
    assert!(
        wild <= tended && tended < field,
        "the plant ladder must be monotone: wild {wild} → tended {tended} → field {field}"
    );
}

/// **Sowing a patch that is already tended still costs the rung's dip.** Upgrading rung 2 → rung 3 is
/// a Cultivate-shaped verb like every other rung-transition: the crew carries only a fraction of
/// what it would otherwise bring home while it works. (On bare ground that fraction is a fraction of
/// nothing — see above; here it bites a real harvest.)
///
/// **Both crews are [`SOLE_FORAGER`]**, because `docs/plan_harvest_floor.md` §3.1 put the dip on
/// crew throughput: a crew the standing crop binds pays nothing for the build, so the cost is only
/// observable where hands are the scarce thing.
#[test]
fn sowing_a_tended_patch_pays_the_dip_then_upgrades_it() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    seat_completed_rung(&mut app, coord, RungKey::PlantTended);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        registry.patch_mut(coord).unwrap().owner = Some(FactionId(0));
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
    // `turns == 0` measures the FIRST harvest of an untouched patch — the accumulated stock above the
    // escapement floor, which is the number the dip is a fraction of on the very same turn. A
    // positive `turns` runs it that many turns first and measures the last one: the STEADY rate, the
    // only fair comparison against a Field that has itself been worked for a while.
    let tended_baseline = |turns: u32, foragers: u32| {
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
        spawn_forager_of(&mut baseline, tile, coord, None, foragers);
        if turns == 0 {
            baseline.world.run_system_once(advance_labor_allocation);
            return provisions_f32(&mut baseline);
        }
        run_turns_with_forage(&mut baseline, turns);
        let before = provisions_f32(&mut baseline);
        run_turns_with_forage(&mut baseline, 1);
        provisions_f32(&mut baseline) - before
    };
    // Measured at the SAME crew the dip is measured at, or the ratio would be between two staffings
    // rather than between building and gathering.
    let tended_yield = tended_baseline(0, SOLE_FORAGER);

    // **The dip, measured in its own world at [`SOLE_FORAGER`].** It has to be a separate run: the
    // rung's `crew_needed` is 3, so a lone forager builds at a third of the rate and the completion
    // half of this test would need three times the turns for reasons that have nothing to do with
    // the dip.
    {
        let mut sparse = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut sparse);
        {
            let mut registry = sparse.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.cultivation_progress = 1.0;
            patch.owner = Some(FactionId(0));
        }
        grant_seed_selection(&mut sparse, FactionId(0));
        spawn_forager_of(
            &mut sparse,
            tile,
            coord,
            Some(Improvement::Sow),
            SOLE_FORAGER,
        );
        sparse.world.run_system_once(advance_labor_allocation);
        let while_sowing = provisions_f32(&mut sparse);
        assert!(
            (while_sowing - dip * tended_yield).abs() < EPSILON,
            "upgrading pays the rung's dip on what the same crew would gather: {while_sowing} vs {}",
            dip * tended_yield
        );
    }

    // Worked to completion the patch stands on rung 3 — and stops paying the dip.
    spawn_builder(&mut app, tile, coord, Improvement::Sow);
    let turns_to_sow = turns_to_sow(&app);
    run_turns_with_forage(&mut app, turns_to_sow);
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
    // Against a tended patch of the SAME age: both have been gathered down to their operating point,
    // so this compares the two rungs rather than one rung's opening stock against the other's steady
    // rate (`docs/plan_harvest_floor.md` §1).
    let tended_steady = tended_baseline(turns_to_sow, FORAGE_WORKERS);
    assert!(
        after_completion > tended_steady,
        "once the Field stands the dip stops and it out-pays the patch it replaced: \
         {after_completion} vs {tended_steady}"
    );
}

/// **Completion retires the build verb** (issue #420) — the `Sow` twin of the plant rung-2, animal
/// rung-2 and animal rung-3 cases pinned in `systems::labor::labor_yield_tests`. The turn a Field
/// finishes, the assignment is rewritten from `Sow` onto the harvest rung, carrying the tile, the
/// committed crop and the crew across: left on the build verb the band would go on paying
/// `yield_fraction_while_building` on ground with nothing left to sow.
#[test]
fn a_completed_field_retires_the_sow_verb_onto_the_harvest_rung() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_seed_selection(&mut app, FactionId(0));
    // Name the crop on the assignment (rather than leaving the auto-pick) so the retire can be
    // asserted to carry the *commitment* across, not merely the tile coordinate.
    let crop = default_sowable_species(&app, coord).expect("sowable ground grows a sowable plant");
    let band = spawn_builder(&mut app, tile, coord, Improvement::Sow);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band forages");
        let LaborTarget::Forage { species, .. } = &mut allocation.assignments[0].target else {
            panic!("the fixture band forages");
        };
        *species = Some(crop.clone());
    }

    // Every turn but the last: the meter fills and the verb stays put.
    let turns_to_sow = turns_to_sow(&app);
    let turns_to_build = turns_to_sow;
    run_turns_with_forage(&mut app, turns_to_build - 1);
    assert!(
        !app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_field(),
        "fixture: the Field must still be going in here (progress {})",
        field_progress_of(&app, coord)
    );
    assert_eq!(
        app.world.get::<LaborAllocation>(band).unwrap().assignments[0].improvement,
        Some(Improvement::Sow),
        "an unfinished build keeps its verb — only completion clears it"
    );

    run_turns_with_forage(&mut app, 1);
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_field(),
        "fixture: this is the completing turn"
    );
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert_eq!(
        allocation.assignments.len(),
        1,
        "completion edits a row, it never adds or drops one"
    );
    let assignment = &allocation.assignments[0];
    assert_eq!(
        assignment.workers,
        sow_crew(&app),
        "the crew stays on the ground it sowed"
    );
    assert_eq!(
        assignment.improvement, None,
        "completion clears the improvement — there is nothing left to sow here"
    );
    let LaborTarget::Forage {
        tile: sown_tile,
        floor,
        species,
    } = &assignment.target
    else {
        panic!("completion must not change the target's KIND: {assignment:?}");
    };
    assert_eq!(
        *floor, 0.5,
        "the crew's floor is left exactly as the player set it (issue #442)"
    );
    assert_eq!(*sown_tile, coord, "the same ground");
    assert_eq!(
        species.as_deref(),
        Some(crop.as_str()),
        "the crop the crew committed the build to survives the handoff"
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
    let band = spawn_builder(&mut app, tile, coord, Improvement::Sow);
    let turns_to_sow = turns_to_sow(&app);
    let (_, decay_per_turn) = field_build(&app);
    assert!(decay_per_turn > 0.0, "an unworked field must bleed");
    run_turns_with_forage(&mut app, turns_to_sow);
    assert!(app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .unwrap()
        .is_field());

    // The crew walks off.
    app.world.despawn(band);

    // The feral pass reads a flag the labor arm wrote **last** turn (Logistics runs before Population
    // — the deliberate one-turn lag), so the first pass after the crew leaves still sees the ground
    // as worked and spares it. On top of that the `plant:field` rung forgives `grace_turns` of
    // consecutive neglect. Both are read off the ladder rather than restated, so a retune moves this
    // test with the game.
    const SPARED_LAG_TURNS: u32 = 1;
    let grace = field_grace(&app);
    run_turns_untended(&mut app, SPARED_LAG_TURNS + grace);
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_field(),
        "the grace turns cost the Field nothing"
    );
    run_turns_untended(&mut app, 1);
    {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).unwrap();
        assert!(
            !patch.is_field(),
            "the first turn past the grace takes a field feral: progress {}",
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
    let lapse_turns = (field_cost(&app) / decay_per_turn).ceil() as u32 + 2;
    run_turns_untended(&mut app, lapse_turns);
    let registry = app.world.resource::<ForageRegistry>();
    let patch = registry.patch(coord).unwrap();
    assert_eq!(
        patch.field_progress,
        core_sim::RUNG_UNSTARTED,
        "the investment fully lapses"
    );
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
    spawn_builder(&mut app, tile, coord, Improvement::Sow);

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

/// **Two crews on one tile: the Field completes ONCE and clears BOTH verbs** (PR #448 review) — the
/// rung-3 twin of `forage_cultivation`'s
/// `a_completed_cultivation_announces_once_and_clears_every_bands_verb`, and the rung the defect was
/// *unrecoverable* on.
///
/// `handle_sow` sets the improvement on **every** band of the faction working the tile. Once the
/// Field stands, the Forage arm takes its managed branch and `continue`s **before** the `Sow` block,
/// so a band that did not finish the build never reached `accrue_field` — and therefore never
/// reached the completion seam that hands the verb back. Its `Sow` was permanent: only
/// `abandon_improvement` could clear it, and until someone did, the crew paid
/// `yield_fraction_while_building` forever on a finished Field. The rung-2 shape self-healed (a
/// tended patch still falls through to the wild path); this one could not, which is why the
/// "nothing left to build" test is asked once per source ahead of the rung branch rather than inside
/// each build block.
#[test]
fn a_completed_field_clears_the_sow_verb_for_every_band_that_was_building_it() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_seed_selection(&mut app, FactionId(0));
    let first = spawn_builder(&mut app, tile, coord, Improvement::Sow);
    let second = spawn_builder(&mut app, tile, coord, Improvement::Sow);
    // A token second crew — enough to hold an assignment (and therefore an improvement) without its
    // share of the draw-down changing what the first band is building against.
    app.world
        .get_mut::<LaborAllocation>(second)
        .expect("the second band forages")
        .assignments[0]
        .workers = TOKEN_SECOND_CREW;

    // Long enough for the meter to fill however the two crews' accruals interleave, plus the turn a
    // band that did not finish it needs to notice (its clear is decided at the top of its own
    // iteration, so a crew processed *before* the finisher clears on the following turn).
    let turns_to_sow = turns_to_sow(&app);
    run_turns_with_forage(&mut app, turns_to_sow + 1);

    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_field(),
        "fixture: the Field must stand by now (progress {})",
        field_progress_of(&app, coord)
    );
    for (label, band) in [("the finisher", first), ("the second crew", second)] {
        assert_eq!(
            app.world.get::<LaborAllocation>(band).unwrap().assignments[0].improvement,
            None,
            "{label} must hand the verb back — there is nothing left to sow here"
        );
    }
    assert_eq!(
        app.world
            .resource::<CommandEventLog>()
            .iter()
            .filter(|entry| entry.label.contains("Field sown at"))
            .count(),
        1,
        "one Field was sown, so the player is told once — not once per crew"
    );
}

/// A token crew for the second band on a shared source: enough to hold an assignment (and therefore
/// an improvement) without its share of the draw-down changing what the first band is measuring.
const TOKEN_SECOND_CREW: u32 = 1;

// ---------------------------------------------------------------------------------------------
// The plant web unwinds NEWEST-FIRST
// ---------------------------------------------------------------------------------------------

/// **A lower rung does not decay while a higher one still has progress.** Rung 3 bleeds to nothing
/// first, and only then does the tended ground beneath it start to go — the least-established
/// improvement is the most fragile.
///
/// This is the fix for an **unrecoverable** state, and that is why it matters more than tidiness.
/// Bleeding both meters together knocked a completed tended patch to `0.99` during a gap in the Sow
/// work; once the crew came back, the running `Sow` marked the patch worked every turn, so
/// cultivation could neither decay further nor re-accrue (only `Cultivate` accrues it, and at most one
/// improvement is ever in flight). The patch was stranded one hundredth below a rung it had already
/// paid for, permanently.
#[test]
fn an_unworked_patch_unwinds_its_newest_rung_first() {
    let mut app = spawn_world();
    let (_tile, coord) = find_sowable_tile(&app);
    // A patch standing on rung 2 with a part-built Field on top of it — the state a crew that
    // started sowing a tended patch and then walked away leaves behind.
    seat_completed_rung(&mut app, coord, RungKey::PlantTended);
    let tended_cost = tended_cost(&app);
    let sow_cost = field_cost(&app);
    let part_built_field = sow_cost / 2.0;
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch");
        patch.field_progress = part_built_field;
        patch.field_cost = sow_cost;
        patch.owner = Some(FactionId(0));
    }

    let (_, field_decay) = field_build(&app);
    let grace = field_grace(&app);
    // Run right through the Field's whole bleed. Cultivation must not move by so much as one turn's
    // decay while the Field still has anything left.
    let field_bleed_turns = (part_built_field / field_decay).ceil() as u32;
    run_turns_untended(&mut app, grace + field_bleed_turns);
    {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).expect("patch");
        assert!(
            patch.field_progress < field_decay,
            "the Field's own meter is spent (an `f32` subtracted turn by turn lands a few ULPs \
             above zero, which is still `> RUNG_UNSTARTED` — correctly, that is the guard): {}",
            patch.field_progress
        );
        assert_eq!(
            patch.cultivation_progress, tended_cost,
            "and the tended ground under it lost NOTHING while that was happening"
        );
        assert!(
            patch.is_cultivated(),
            "so the patch is still a tended patch — the rung it paid for survives"
        );
    }

    // Only once nothing is left of the Field does rung 2 become the newest thing to lose, and start
    // to bleed. The neglect counter is long past every grace by now, so this is immediate.
    const FLOAT_RESIDUE_TURNS: u32 = 2;
    run_turns_untended(&mut app, FLOAT_RESIDUE_TURNS + 1);
    // The Field's meter is spent to the last ULP before rung 2 becomes the newest thing to lose.
    let patch_registry = app.world.resource::<ForageRegistry>();
    let patch = patch_registry.patch(coord).expect("patch");
    assert_eq!(
        patch.field_progress,
        core_sim::RUNG_UNSTARTED,
        "the Field is fully gone"
    );
    assert!(
        patch.cultivation_progress < tended_cost,
        "with the Field gone, the tended rung is the newest thing left and starts to bleed"
    );
}

/// **The frozen-at-0.99 state is unreachable by construction** — the concrete case the ordering rule
/// exists for, driven through the real pipeline rather than asserted on the rule.
///
/// A completed tended patch, a gap in the Sow work, then the crew returns: cultivation must still be
/// exactly `1.0`, because it cannot move while `field_progress > 0`. Under the old both-meters bleed it
/// landed just below `1.0` and could never come back.
#[test]
fn a_gap_in_a_sow_cannot_strand_the_tended_rung_below_complete() {
    let mut app = spawn_world();
    let (tile, coord) = find_sowable_tile(&app);
    grant_seed_selection(&mut app, FactionId(0));
    seat_completed_rung(&mut app, coord, RungKey::PlantTended);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        registry.patch_mut(coord).expect("patch").owner = Some(FactionId(0));
    }
    let tended_cost = tended_cost(&app);
    let band = spawn_builder(&mut app, tile, coord, Improvement::Sow);
    run_turns_with_forage(&mut app, 5);
    assert!(
        field_progress_of(&app, coord) > 0.0,
        "the Sow is genuinely underway"
    );

    // The crew is pulled away for long enough to bleed, then comes back.
    app.world.despawn(band);
    let grace = field_grace(&app);
    run_turns_untended(&mut app, grace + 3);
    let stranded = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .cultivation_progress;
    assert_eq!(
        stranded, tended_cost,
        "cultivation cannot move while a Field meter still stands: {stranded}"
    );
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "so the patch the player paid 25 turns for is still a tended patch"
    );
}

/// **Losing a Field is announced on the `sow` channel** — the rung-3 twin of the tended patch's feral
/// line, and pushed on the same edge (the turn it crosses back below `1.0`), once.
#[test]
fn losing_a_field_pushes_one_feed_line_on_the_sow_channel() {
    let mut app = spawn_world();
    let (_tile, coord) = find_sowable_tile(&app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        registry.patch_mut(coord).expect("patch").owner = Some(FactionId(0));
    }
    seat_completed_rung(&mut app, coord, RungKey::PlantField);

    let grace = field_grace(&app);
    run_turns_untended(&mut app, grace);
    assert_eq!(
        feral_lines(&app),
        0,
        "nothing is lost, so nothing is announced, while the grace holds"
    );

    let (_, decay) = field_build(&app);
    run_turns_untended(&mut app, (1.0 / decay).ceil() as u32 + 2);
    assert_eq!(
        feral_lines(&app),
        1,
        "the loss is announced once, on the turn it happens — not every turn of the bleed"
    );
    let detail = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .find(|entry| entry.label.contains("gone feral"))
        .expect("the feral line")
        .detail
        .clone()
        .unwrap_or_default();
    assert!(
        detail.contains("action=sow"),
        "a lost Field reads on the `sow` channel, not `cultivate`: {detail}"
    );
}

/// Feed lines announcing a plant rung going feral.
fn feral_lines(app: &App) -> usize {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.label.contains("gone feral"))
        .count()
}

/// **A `Cultivate` on a Field is handed back, not stalled forever.**
///
/// `Sow` needs no prior patch, so a Field routinely stands on ground that was never tended — and on
/// such a patch `is_cultivated()` is false while the Field arm of the labor loop `continue`s past the
/// Cultivate block entirely. The verb was therefore neither cleared nor accrued: the meter never
/// moved, nothing was said, and only `abandon_improvement` could get the crew off the dead rung. The
/// "nothing left to build" seam now reads the patch's whole managed state, so the verb comes back on
/// the first worked turn.
#[test]
fn a_cultivate_on_a_field_is_handed_back_rather_than_stalling_forever() {
    let mut app = spawn_world();
    let (tile, coord) = find_sowable_tile(&app);
    grant_seed_selection(&mut app, FactionId(0));
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        registry.patch_mut(coord).expect("patch").owner = Some(FactionId(0));
    }
    seat_completed_rung(&mut app, coord, RungKey::PlantField);
    let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    assert_eq!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .cultivation_progress,
        0.0,
        "the fixture is the reachable case: a Field on ground that was never tended"
    );

    run_turns_with_forage(&mut app, 1);

    assert_eq!(
        app.world.get::<LaborAllocation>(band).unwrap().assignments[0].improvement,
        None,
        "the crew is taken off a rung it can never build on this source"
    );
}
