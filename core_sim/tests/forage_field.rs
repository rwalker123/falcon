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

use core_sim::TakeSelection;
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
    WellbeingConfigHandle, FOOD, RUNG_COST_UNSCALED, SEED_SELECTION_DISCOVERY_ID,
    WHOLLY_UNSUPPLIED,
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
/// **It moved again — to 0.25 — when the take gained its growth-share backstop** (§4.14). A stand
/// held near its floor while it is being sown now hands over the share of the turn's growth the
/// player's floor left takeable, instead of nothing; measured, the build's trickle went from ~19% of
/// the standing Field's yield to ~22.6%. That is the backstop working, not the rung getting cheaper,
/// and the claim this bounds — *"the build is a trickle beside what it buys"* — is unchanged at a
/// quarter.
const BUILD_TRICKLE_FRACTION: f32 = 0.25;

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
    // The build's crew is the fixture's to state now (`docs/plan_standing_upkeep.md` §2.2) — the
    // rung declares none. Two for a Cultivate and three for a Sow are the staffings the two shipped
    // `work_cost`s were priced against.
    const CULTIVATE_CREW: u32 = 2;
    let crew = match improvement {
        Improvement::Cultivate => CULTIVATE_CREW,
        _ => sow_crew(app, patch),
    };
    spawn_forager_of(app, tile, patch, Some(improvement), crew)
}

/// **A completed plant rung, seated at the LADDER's own cost.** The feral bleed is an absolute
/// number of work units per turn (a fraction of that rung's cost), so a fixture seated at a nominal
/// one-unit job would lapse to nothing in a single bleeding turn.
fn seat_completed_rung(app: &mut App, coord: UVec2, rung: RungKey) {
    // **One position states a completed rung: the top of its own span.** The retention bar the old
    // fixture stamped beside the cost is deleted (`docs/plan_standing_upkeep.md` §2.8) — a rung is
    // achieved exactly here — so the pair this used to carry has become one number.
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (base, width) = core_sim::plant_rung_span(rung, &ladder);
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    registry
        .patch_mut(coord)
        .expect("patch exists")
        .set_ladder_position(base + width, &ladder);
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
    // **THE BAND KEEPS WHAT IT WORKS** (`docs/plan_standing_upkeep.md` §4.6a). The keeping pool owes
    // a meter carrying work from the **first work banked**, so a fixture with the role empty is not
    // measuring a build's pace — it is measuring the build racing the rot. Sized at the dearer plant
    // rung's own count, so it covers a tended patch and a Field alike; the tests that want an
    // *unkept* patch unstaff the band or walk it away.
    let keepers = {
        let loads = tender_loads_at(app, patch);
        app.world
            .resource::<LadderConfigHandle>()
            .get()
            .rung(RungKey::PlantField)
            .upkeep_crew_needed(loads)
    };
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                // **THE BAND HAS TO AFFORD BOTH CREWS, WHETHER OR NOT ONE IS STAFFED YET.** The
                // take and the build draw on one pool (`docs/plan_standing_upkeep.md` §2.2), so a
                // band sized at the gathering crew alone is over-committed the moment a verb is
                // staffed beside it — and `LaborAllocation::normalize` then trims the build away,
                // leaving a fixture that measures a job nobody is doing. Idle hands cost these
                // fixtures nothing: every take is capped by the crew the assignment names.
                working: scalar_from_f32((foragers + foragers + keepers) as f32),
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
                assignments: vec![
                    LaborAssignment {
                        target: LaborTarget::Forage {
                            tile: patch,
                            floor: policy,
                            species: None,
                            take_species: TakeSelection::EVERYTHING,
                        },
                        workers: foragers,
                        kit: None,
                    },
                    LaborAssignment {
                        target: LaborTarget::Agriculture,
                        workers: keepers,
                        kit: None,
                    },
                    // **A pool of the same size staffs the build** — what this fixture meant when
                    // one crew did every job (`docs/plan_standing_upkeep.md` §2.5).
                    LaborAssignment {
                        target: LaborTarget::Builders,
                        workers: foragers,
                        kit: None,
                    },
                ],
                build_queue: improvement
                    .map(|declared| core_sim::BuildQueueEntry {
                        source: core_sim::BuildSource::Patch(patch),
                        declared: core_sim::BuildJob::Rung(declared),
                        kit: Some(bare_builders()),
                    })
                    .into_iter()
                    .collect(),
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
        .map(|patch| {
            core_sim::patch_rung_work_done(
                patch,
                core_sim::RungKey::PlantField,
                &core_sim::LadderConfig::builtin(),
            )
        })
        .unwrap_or(0.0)
}

/// The `plant:field` rung's build dials, read off the ladder — the same seam the sim drives sowing
/// with, so a retune moves the tests with the game rather than against it.
/// **The `plant:field` rung's neglect grace** — the consecutive turns of **shortfall** the feral
/// bleed forgives before it starts (`docs/plan_standing_upkeep.md` §2.4). Read off the ladder's
/// `upkeep` block, never restated; the build's own grace is absent on both plant rungs, because
/// this branch counts unmet demand rather than un-worked turns.
/// **THE MEASURE BOTH PLANT RUNGS QUOTE THEIR UPKEEP RATE PER** — this tile's own `K` over
/// `forage.cultivation.capacity_per_tender` (`forage::patch_tender_loads`). Resolved off the ground
/// worldgen handed the fixture rather than assumed to be the reference tile's one load: these tests
/// sow whichever site the map offers, and a rich tile costs more to hold than a thin one.
fn tender_loads_at(app: &App, coord: UVec2) -> f32 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let tile_entity = app
        .world
        .resource::<core_sim::TileRegistry>()
        .index(coord.x, coord.y)
        .expect("the fixture tile is on the map");
    let ground = app
        .world
        .get::<core_sim::Tile>(tile_entity)
        .expect("the fixture tile carries a Tile");
    core_sim::patch_tender_loads(
        core_sim::tile_forage_capacity(&labor.forage, ground),
        &labor.forage,
    )
}

fn field_grace(app: &App) -> u32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantField)
        .upkeep_grace_turns()
}

/// **What this file's crew produces on the `plant:field` rung in one turn, and the rung's feral
/// bleed** — both in absolute work units. The accrual is read at [`SOW_CREW`], the head count the
/// build fixtures actually staff, because the crew *is* the throughput now
/// (`docs/plan_unit_costed_work.md` §1.2).
fn field_build(app: &App, coord: UVec2) -> (f32, f32) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let field = ladder.rung(RungKey::PlantField);
    (
        field.build_accrual(
            Some(Improvement::Sow),
            true,
            sow_crew(app, coord),
            core_sim::NO_BUILD_GEAR,
        ),
        // **The feral bleed is the rung's own ROT RATE**, not the demand it goes short by: the two
        // are separate dials (`docs/plan_standing_upkeep.md` §2.4). Numerically the same as the
        // retired `decay_fraction_per_turn × work_cost`, which is why the paces below hold.
        field.upkeep_decay(WHOLLY_UNSUPPLIED, WELL_PAST_ANY_GRACE),
    )
}

/// The turn count a wholly unmaintained rung is on once every shipped grace is spent — larger than
/// any of them, so a bleed read at it is certainly biting.
const WELL_PAST_ANY_GRACE: u16 = 32;

/// **HOW MANY WHOLLY UNMAINTAINED TURNS A COMPLETED FIELD SURVIVES** — the grace, plus the turns its
/// own rot rate takes to erode the meter from its cost to below its retention bar.
///
/// **This is the number the reported bug is about**: a completed meter sits exactly at its cost, so
/// a `progress >= cost` predicate answered `grace + 1` and the rung was lost on the first bleed of
/// any size. Derived from the rung's own dials, so a retune of any of the three moves it.
fn unmaintained_field_turns_before_loss(app: &App, coord: UVec2) -> u32 {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let field = ladder.rung(RungKey::PlantField);
    let cost = field_cost(app);
    // The Field's whole span is erodable now: there is no retention bar below it
    // (`docs/plan_standing_upkeep.md` §2.8), so what a bleed has to eat before the rung is lost is
    // the rung's own cost.
    let erodable = cost;
    let (_, bleed) = field_build(app, coord);
    // Lost the turn the meter falls **below** the bar, so eroding exactly the erodable amount still
    // holds it — hence `floor + 1` rather than `ceil`.
    field.upkeep_grace_turns() + (erodable / bleed).floor() as u32 + 1
}

/// **The crew a BUILD fixture staffs, and it is deliberately NOT [`FORAGE_WORKERS`].** 5000 is
/// chosen so a *take* is ceiling-bound rather than labor-bound; it became a **build-pacing** number
/// only when the crew stopped being capped, and at that head count a 75-unit Sow finishes in a
/// single turn — leaving no part-sown ground for a decay, a grace or a completion test to stand on.
/// The rung's own `crew_needed` is the staffing the shipped cost was priced against
/// (`docs/plan_unit_costed_work.md` §3). The one-turn over-crewed build is real and pinned on
/// purpose by `forage_cultivation::over_crewing_a_build_is_no_longer_capped`.
fn sow_crew(app: &App, coord: UVec2) -> u32 {
    /// The net supply the `plant:field` rung's 75-unit `work_cost` was priced against — the
    /// staffing this file's paces are all quoted at.
    const NET_WORKER_TURNS: u32 = 3;
    // **Plus the rung's maintenance rate**, which the *keeping* pool owes while the meter is being
    // raised — the builders supply none of it (`docs/plan_standing_upkeep.md` §4.6a). The padding
    // survives because these fixtures size one band against everything it staffs: a bare three hands
    // leaves nothing for the `agriculture` row, and a Field rotting under its own builders is not
    // the pace this file measures.
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantField)
        .upkeep_crew_needed(tender_loads_at(app, coord))
        .saturating_add(NET_WORKER_TURNS)
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

/// **Turns [`SOW_CREW`] needs to raise a whole Field FROM BARE GROUND**, `ceil(work / work per
/// turn)` — turns are an output now, so a bare `1.0 / rate` no longer means anything.
///
/// **THE WORK IS THE WHOLE BRANCH, not the Field rung's own span** (`docs/plan_standing_upkeep.md`
/// §2.8, rule 1). A patch has one position, so there is no way to put work on the Field without the
/// tended rung beneath it being whole: a `sow` ordered on untended ground clears first, at
/// Cultivate's price and with Cultivate's tool. A bare-ground Sow is therefore `50 + 75` work units,
/// and a fixture that ran only the Field's own 75 would measure a **tended patch** and report it as
/// a failed Sow.
fn turns_to_sow(app: &App, coord: UVec2) -> u32 {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (base, width) = core_sim::plant_rung_span(RungKey::PlantField, &ladder);
    core_sim::build_turns_remaining(
        base + width,
        core_sim::RUNG_UNSTARTED,
        field_build(app, coord).0,
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
/// **What a band has queued on a patch** — the 6b reading of what a fixture used to get off the
/// row's `improvement` field (`docs/plan_standing_upkeep.md` §2.5).
fn queued_job(
    app: &App,
    band: bevy::prelude::Entity,
    tile: bevy::math::UVec2,
) -> Option<core_sim::BuildJob> {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the fixture band keeps its allocation")
        .build_queue_entry(&core_sim::BuildSource::Patch(tile))
        .map(|entry| entry.declared)
}

/// [`queued_job`] as a rung verb — `None` for a ring, which names no rung.
fn declared_rung(
    app: &App,
    band: bevy::prelude::Entity,
    tile: bevy::math::UVec2,
) -> Option<Improvement> {
    match queued_job(app, band, tile) {
        Some(core_sim::BuildJob::Rung(improvement)) => Some(improvement),
        Some(core_sim::BuildJob::ExtendPen) | None => None,
    }
}

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

    // **A QUEUE ENTRY NAMES A DESTINATION, so `sow` on untouched ground lays TWO LEGS** — `0 → 50`
    // tended, then `50 → 125` field (`docs/plan_standing_upkeep.md` §2.8). The player's order is
    // *take this land to Field*; the entry climbs everything between here and there and stays at the
    // head until it arrives.
    let ladder = core_sim::LadderConfig::builtin();
    let (tended_base, tended_width) =
        core_sim::plant_rung_span(core_sim::RungKey::PlantTended, &ladder);
    let (field_base, field_width) =
        core_sim::plant_rung_span(core_sim::RungKey::PlantField, &ladder);
    fn published_legs(app: &App, coord: UVec2) -> Vec<(String, f32)> {
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .build_legs
            .iter()
            .map(|published| (published.leg.rung.wire_key(), published.leg.work_remaining))
            .collect()
    }

    // **THE LEGS, ASSERTED AS PUBLISHED** — never re-derived from the ladder here, because the whole
    // point of publishing them is that nobody downstream has to.
    let legs = published_legs(&app, coord);
    assert_eq!(
        legs.len(),
        2,
        "sowing untouched ground is a two-leg climb, got {legs:?}"
    );
    assert_eq!(legs[0].0, "plant:tended", "…and it clears first: {legs:?}");
    assert_eq!(legs[1].0, "plant:field", "…then sows: {legs:?}");
    // **The legs sum to what is LEFT of the climb**, so the whole order is that plus what the first
    // turn above already banked — 125 work units on the shipped ladder, never the Field's own 75.
    let banked = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .ladder_position();
    let ordered: f32 = legs.iter().map(|(_, work)| work).sum::<f32>() + banked;
    assert!(
        (ordered - (tended_base + tended_width + field_width)).abs() < 1e-3,
        "the two legs plus the work already banked are the WHOLE BRANCH: {ordered} from \
         {legs:?} over {banked}"
    );

    // **IT PASSES THROUGH CULTIVATED ON THE WAY.** Enough turns to lay the first leg and no more.
    let first_leg_turns = core_sim::build_turns_remaining(
        legs[0].1,
        core_sim::RUNG_UNSTARTED,
        field_build(&app, coord).0,
    )
    .expect("a staffed Sow finishes");
    run_turns_with_forage(&mut app, first_leg_turns);
    {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).expect("patch persists");
        assert!(
            patch.is_cultivated(),
            "the first leg lands the tended rung: position {}",
            patch.ladder_position()
        );
        assert!(
            !patch.is_field(),
            "…and the Field is still to come — the entry has not arrived, so it holds the head"
        );
        assert_eq!(
            published_legs(&app, coord).len(),
            1,
            "…and one leg is left to lay"
        );
    }

    // Then the rest of the climb.
    let rest_of_the_climb = turns_to_sow(&app, coord);
    run_turns_with_forage(&mut app, rest_of_the_climb);
    let registry = app.world.resource::<ForageRegistry>();
    let patch = registry.patch(coord).expect("patch persists");
    assert!(
        patch.is_field(),
        "sustained Sow work completes the field: position {}",
        patch.ladder_position()
    );
    assert_eq!(patch.owner, Some(FactionId(0)), "the sower owns it");
    assert!(
        patch.is_cultivated(),
        "and a Field IS tended — the climb laid that ground on the way, so rung 3 stands on rung 2 \
         rather than beside it"
    );
    assert!(
        (patch.ladder_position() - (field_base + field_width)).abs() < 1e-3,
        "…at the top of the branch: {}",
        patch.ladder_position()
    );
    assert_eq!(
        registry.cultivated_count(FactionId(0)),
        1,
        "a Field is a completed plant improvement — it must read as domestication, not as less than \
         the rung below it"
    );
}

/// **THE SAME ORDER ON GROUND THAT IS ALREADY TENDED IS ONE LEG, AND IT OWES ONLY THE FIELD** — the
/// other half of *"a queue entry names a destination"*. The player types the same `sow`; what it
/// costs depends on where the land already stands.
#[test]
fn a_sow_ordered_on_tended_ground_lays_one_leg_owing_the_fields_own_span() {
    let mut app = spawn_world();
    let (tile, coord) = find_sowable_tile(&app);
    grant_seed_selection(&mut app, FactionId(0));
    seat_completed_rung(&mut app, coord, RungKey::PlantTended);
    spawn_builder(&mut app, tile, coord, Improvement::Sow);
    run_turns_with_forage(&mut app, 1);

    let ladder = core_sim::LadderConfig::builtin();
    let (_, field_width) = core_sim::plant_rung_span(core_sim::RungKey::PlantField, &ladder);
    let legs: Vec<_> = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .build_legs
        .clone();
    assert_eq!(
        legs.len(),
        1,
        "ground that already holds the tended rung has one leg left: {legs:?}"
    );
    assert_eq!(legs[0].leg.rung, RungKey::PlantField);
    // A turn of work has already landed, so the leg owes the Field's span less that turn's banking —
    // asserted as a bound rather than an equality, because the pool's output is the fixture's, not
    // this test's subject.
    assert!(
        legs[0].leg.work_remaining <= field_width && legs[0].leg.work_remaining > field_width * 0.5,
        "…owing the FIELD'S OWN span, not the whole branch: {} against {field_width}",
        legs[0].leg.work_remaining
    );
}

/// **A PREVIOUS IMPROVEMENT IS A RECEIPT, NOT A DISCOUNT** — a patch part-way up its Cultivate owes
/// only what is *left* of that leg, and the Field's own span in full behind it.
#[test]
fn a_part_built_cultivate_owes_only_the_remainder_of_its_own_leg() {
    /// How far up the tended rung the fixture seats the patch, in work units — comfortably clear of
    /// both ends so a leg that quoted the whole span, or none of it, fails visibly.
    const BANKED_ON_THE_LEG: f32 = 30.0;

    let mut app = spawn_world();
    let (tile, coord) = find_sowable_tile(&app);
    grant_seed_selection(&mut app, FactionId(0));
    let ladder = core_sim::LadderConfig::builtin();
    let (tended_base, tended_width) =
        core_sim::plant_rung_span(core_sim::RungKey::PlantTended, &ladder);
    let (_, field_width) = core_sim::plant_rung_span(core_sim::RungKey::PlantField, &ladder);
    assert!(
        BANKED_ON_THE_LEG > tended_base && BANKED_ON_THE_LEG < tended_base + tended_width,
        "fixture: the seat must be INSIDE the tended rung's span, or the case is not the one \
         under test"
    );
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch");
        patch.set_ladder_position(BANKED_ON_THE_LEG, &ladder);
        patch.owner = Some(FactionId(0));
    }
    spawn_builder(&mut app, tile, coord, Improvement::Sow);
    // The published legs are struck by the labor pass, so let one turn run — and read them BEFORE
    // asserting, since that turn banks a little onto the first leg.
    let before = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .ladder_position();
    run_turns_with_forage(&mut app, 1);
    let banked_this_turn = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .ladder_position()
        - before;

    let legs: Vec<_> = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .build_legs
        .clone();
    assert_eq!(legs.len(), 2, "still two legs to lay: {legs:?}");
    assert_eq!(legs[0].leg.rung, RungKey::PlantTended);
    let owed_on_the_leg = tended_base + tended_width - BANKED_ON_THE_LEG - banked_this_turn;
    assert!(
        (legs[0].leg.work_remaining - owed_on_the_leg).abs() < 1e-2,
        "the part-built leg owes what is LEFT of it ({owed_on_the_leg}), not its whole span \
         ({tended_width}): {}",
        legs[0].leg.work_remaining
    );
    assert!(
        (legs[1].leg.work_remaining - field_width).abs() < 1e-3,
        "…and the leg above it owes its own span in full: {} against {field_width}",
        legs[1].leg.work_remaining
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

    let turns_to_sow = turns_to_sow(&app, coord);
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

/// ⛔ **THE BUILDERS' KIT WEARS ON THE FIRST LEG OF A TWO-LEG SOW.**
///
/// A `sow` on untended ground climbs `plant:tended` **through the Sow arm** before it ever reaches
/// the Field's own span, and the wear charge is a delta across the accrual. Measured against the
/// *Field rung's* clamped meter that delta is `0 → 0` for the whole of that leg — 40% of the shipped
/// two-leg climb — so the tools came home unworn from work they had done, against
/// `.claude/rules/core_sim/equipment.md`'s *"wear follows the work actually done"*.
///
/// **The keepers are deliberately sent out BARE.** An absent kit derives per job, and the roster's
/// `tillage` answer serves `agriculture` as well as `builders` — so keeping wear would spend the very
/// item this asserts on and the arm would pass with the build charging nothing at all.
#[test]
fn a_bare_ground_sow_wears_the_builders_kit_on_its_first_leg() {
    let mut app = spawn_world();
    let (tile, coord) = find_bare_sowable_tile(&mut app);
    grant_seed_selection(&mut app, FactionId(0));
    let band = spawn_builder(&mut app, tile, coord, Improvement::Sow);
    gear_the_builders_alone(&mut app, band);

    run_turns_with_forage(&mut app, 1);

    // **The fixture is still on the FIRST leg** — below the Field rung's base, which is exactly the
    // span the per-rung reading cannot see. Without this the arm would pass on a climb that had
    // already crossed into the Field.
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (field_base, _) = core_sim::plant_rung_span(RungKey::PlantField, &ladder);
    let position = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("the sow created a patch")
        .ladder_position();
    assert!(
        position > core_sim::RUNG_UNSTARTED && position < field_base,
        "fixture: one turn of a bare-ground sow stands on the tended leg, got {position}"
    );

    let spent = app
        .world
        .get::<core_sim::BandEquipment>(band)
        .expect("the fixture geared its builders")
        .wear_of(TILLAGE_ITEM);
    assert!(
        spent > 0.0,
        "the crew banked {position} work units this turn and the hoes must have paid for it, got \
         {spent} condition spent"
    );
}

/// **Put the shipped plant builders' kit on the QUEUE ENTRY, and NOTHING on the keeping row** —
/// the isolation the wear arm above needs, since both jobs derive the same kit when neither names
/// one.
///
/// **The entry, not the `builders` row.** A build's kit is a property of the job since
/// `docs/plan_standing_upkeep.md` §4.7a ②, and the row carries none at all.
fn gear_the_builders_alone(app: &mut App, band: bevy::prelude::Entity) {
    let equipment = core_sim::EquipmentConfig::builtin();
    let kit = equipment
        .kit(TILLAGE_KIT)
        .expect("the shipped roster carries the tillage kit");
    let builders = {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation");
        let mut builders = 0;
        for assignment in &mut allocation.assignments {
            match assignment.target {
                LaborTarget::Builders => builders = assignment.workers,
                LaborTarget::Agriculture => assignment.kit = Some(bare_builders()),
                _ => {}
            }
        }
        assert!(
            !allocation.build_queue.is_empty(),
            "fixture: the band must have declared a build for the kit to ride"
        );
        for entry in allocation.build_queue.iter_mut() {
            entry.kit = Some(kit.clone());
        }
        builders
    };
    app.world
        .entity_mut(band)
        .insert(core_sim::BandEquipment::start_stocked_for(
            &equipment,
            builders as f32,
        ));
}

/// The plant web's builders kit, and the one item it carries — what a build's wear is spent on.
const TILLAGE_KIT: &str = "tillage";
const TILLAGE_ITEM: &str = "hoes";

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

    // `basket_rate` was the retired managed rate's normalization baseline; the ladder is compared on
    // production now, so nothing here needs it.
    let (wild, biomass, capacity, _basket_rate) = rung_yield(None);
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
    // **RUNG 3 IS THE SAME SKIM, ON RICHER AND FASTER LAND.** The managed rate is retired — a rung
    // may change production, no rung changes the draw — so a Field is drawn down like everything else
    // and what it bought shows up in `r` and `K`.
    //
    // **Measured on the PRODUCTION side, because this fixture's single forager caps all three rungs
    // at the same carry.** That equality is itself the new model working: a Field holds far more
    // standing crop, so realizing it genuinely needs more hands, and one gatherer brings home a
    // gatherer's load off any of them.
    assert!(
        (field - tended).abs() < 1e-3,
        "one forager carries the same load off any rung — the collection cap binds, which is the \
         Field's new cost: {field} vs {tended}"
    );
    let ladder_climbs = |rung| {
        let labor = LaborConfig::builtin();
        let flora = core_sim::FloraConfig::builtin();
        let mut patch = core_sim::ForagePatch::new(UVec2::new(0, 0), capacity);
        patch.biomass = biomass;
        core_sim::rung_payoff(&patch, &[], &labor.forage, &flora, 1.0, rung)
    };
    let produced_tended = ladder_climbs(RungKey::PlantTended);
    let produced_field = ladder_climbs(RungKey::PlantField);
    assert!(
        produced_field > produced_tended,
        "a Field must out-PRODUCE the tended patch beneath it: {produced_field} vs \
         {produced_tended}"
    );
    let expected_field = produced_tended
        * forage.cultivation.field_regrowth_gain
        * forage.cultivation.field_capacity_gain
        / forage.cultivation.tended_regrowth_gain;
    assert!(
        (produced_field - expected_field).abs() < 1e-3,
        "…by exactly its two production gains, at this fixture's shared ground: {produced_field} \
         vs {expected_field}"
    );

    // **And the ladder climbs — on PRODUCTION, which is what a rung buys.** This is the claim; the
    // pins above are how it is bought. Since S2 the bare wild↔tended step is `≤` (a neutral tended
    // patch with no crop equals wild); the strict climb to the Field survives and the neutral gain
    // only widens it.
    let produced_wild = ladder_climbs(RungKey::PlantWild);
    assert!(
        produced_wild <= produced_tended && produced_tended < produced_field,
        "the plant ladder must be monotone in production: wild {produced_wild} → tended \
         {produced_tended} → field {produced_field}"
    );
}

/// **Sowing a patch that is already tended costs HANDS.** Upgrading rung 2 → rung 3 is a
/// Cultivate-shaped verb like every other rung-transition, and its price is the people in the seed
/// rather than in the baskets (`docs/plan_standing_upkeep.md` §2.2) — so the gatherers who stay on
/// the patch carry exactly what they carried before.
///
/// **Both crews are [`SOLE_FORAGER`]**, kept from when the retired `yield_fraction_while_building`
/// was only observable where hands were the scarce thing. The cost no longer depends on which side
/// binds, but the fixture stays at one hand so the baseline is the same staffing.
#[test]
fn sowing_a_tended_patch_leaves_the_gatherers_take_alone_then_upgrades_it() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    seat_completed_rung(&mut app, coord, RungKey::PlantTended);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        registry.patch_mut(coord).unwrap().owner = Some(FactionId(0));
    }
    grant_seed_selection(&mut app, FactionId(0));

    // The tended harvest this patch would pay if nobody were upgrading it. **Committed to the same
    // crop**: a Sow commits the ground to one named plant (Flora Roster S1), which changes its
    // conversion rate, so an uncommitted baseline would be measuring the commitment rather than the
    // rung's cost.
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
            patch.set_ladder_position(1.0, &core_sim::LadderConfig::builtin());
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

    // **The build turn, measured in its own world at [`SOLE_FORAGER`].** It has to be a separate
    // run: the rung's `crew_needed` is 3, so a lone forager builds at a third of the rate and the
    // completion half of this test would need three times the turns for reasons that have nothing to
    // do with what a building turn pays.
    {
        let mut sparse = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut sparse);
        {
            let mut registry = sparse.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.set_ladder_position(1.0, &core_sim::LadderConfig::builtin());
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
            tended_yield > 0.0,
            "the same crew must gather something when not sowing, or 'takes nothing' proves \
             nothing: {tended_yield}"
        );
        assert!(
            (while_sowing - tended_yield).abs() < EPSILON,
            "the gatherers are untouched by the Sow staffed beside them: {while_sowing} vs \
             {tended_yield}"
        );
    }

    // Worked to completion the patch stands on rung 3 — and its crew starts gathering again.
    spawn_builder(&mut app, tile, coord, Improvement::Sow);
    let turns_to_sow = turns_to_sow(&app, coord);
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
        "once the Field stands the build stops claiming the turn and it out-pays the patch it \
         replaced: {after_completion} vs {tended_steady}"
    );
}

/// **Completion retires the build verb** (issue #420) — the `Sow` twin of the plant rung-2, animal
/// rung-2 and animal rung-3 cases pinned in `systems::labor::labor_yield_tests`. The turn a Field
/// finishes, the assignment is rewritten from `Sow` onto the harvest rung, carrying the tile, the
/// committed crop and the crew across: left on the build verb the band would go on spending its
/// whole work budget on ground with nothing left to sow, and gathering none of it.
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
    let turns_to_sow = turns_to_sow(&app, coord);
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
        queued_job(&app, band, coord),
        Some(core_sim::BuildJob::Rung(Improvement::Sow)),
        "an unfinished build keeps its entry — only completion retires it"
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
    // **The worked source is still one row — and the band holds the standing-role rows beside it**
    // (`agriculture`, `builders`), which are band-wide rather than sources.
    let sources: Vec<&core_sim::LaborAssignment> = allocation
        .assignments
        .iter()
        .filter(|a| matches!(a.target, LaborTarget::Forage { .. }))
        .collect();
    assert_eq!(
        sources.len(),
        1,
        "completion edits the source's row, it never adds or drops one"
    );
    let assignment = sources[0];
    assert_eq!(
        assignment.workers,
        sow_crew(&app, coord),
        "the crew stays on the ground it sowed"
    );
    assert_eq!(
        queued_job(&app, band, coord),
        None,
        "completion retires the entry — there is nothing left to sow here"
    );
    let LaborTarget::Forage {
        tile: sown_tile,
        floor,
        species,
        ..
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
    let turns_to_sow = turns_to_sow(&app, coord);
    let (_, decay_per_turn) = field_build(&app, coord);
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
    // **AND THE FIRST BLEEDING TURN *DOES* TAKE IT — that is the retention bar's deletion, and what
    // makes it safe is the PAYOUT.** The bar was added because a completed meter sits exactly at its
    // cost, so the first bleed of any size flipped the rung and cost the player the whole of it. The
    // one-position ladder removes the cliff instead of patching it
    // (`docs/plan_standing_upkeep.md` §2.8/§4.10): everything the rung is worth interpolates on the
    // position, so a Field a hair below its top pays a hair under a whole Field.
    //
    // **Asserted as the PAIR** — the position AND what it pays there — because either alone is
    // satisfiable by the model being wrong: a rung that never bled would satisfy the payout clause,
    // and a rung that cliffed to nothing would satisfy the position clause.
    // > #### ⛔ WHAT THIS TEST CANNOT ASSERT, AND WHY IT ASSERTS THE KEEPING INSTEAD
    // >
    // > The bar's deletion is justified by *"everything the rung is worth interpolates on the
    // > position, so losing it at the boundary is a rounding"*. **That is true of the per-biomass
    // > rates and NOT of the Field's food harvest.** A Field is paid through a binary `is_field()`
    // > branch in `advance_labor_allocation` — a **managed rate on the whole standing crop**
    // > (`field_provisions`, capped by collection, never drawn down) — while everything below it is
    // > paid by an **MSY draw-down** (`forage_take`). Those are different *kinds* of harvest, not two
    // > values of one rate, so the rung-2→3 boundary is still a cliff in food.
    // >
    // > So this asserts the interpolation on the quantity that genuinely does cross that boundary
    // > continuously — **the keeping demand**, `2.0 → 4.0` across the Field's span — and the position
    // > pair beside it. The food cliff is flagged for a decision rather than asserted away here.
    let ladder = core_sim::LadderConfig::builtin();
    let (field_base, field_width) =
        core_sim::plant_rung_span(core_sim::RungKey::PlantField, &ladder);
    let billed_at = |app: &App, position: f32| -> f32 {
        let mut probe = app
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .clone();
        probe.set_ladder_position(position, &ladder);
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let tile_entity = app
            .world
            .resource::<core_sim::TileRegistry>()
            .index(coord.x, coord.y)
            .expect("the fixture tile is on the map");
        let ground = app
            .world
            .get::<core_sim::Tile>(tile_entity)
            .expect("the fixture tile carries a Tile");
        // **Billed per tender-load of THIS ground**, which is what makes the two readings below a
        // comparison of rungs rather than of tiles.
        core_sim::patch_upkeep_demand(
            &probe,
            &ladder,
            core_sim::tile_forage_capacity(&labor.forage, ground),
            &labor.forage,
        )
    };

    // **THE PRECONDITION.** A whole Field and the bare tended patch beneath it must owe materially
    // different numbers, or every comparison below passes by both collapsing onto one value.
    let whole_field = billed_at(&app, field_base + field_width);
    let bare_tended = billed_at(&app, field_base);
    assert!(
        whole_field > bare_tended && bare_tended > 0.0,
        "PRECONDITION: a whole Field must cost more to hold than the tended ground under it \
         ({whole_field} against {bare_tended}), or this test cannot tell a rounding from a cliff"
    );

    // One bleeding turn past the grace and the rung is gone…
    run_turns_untended(&mut app, 1);
    let landed = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .unwrap()
        .ladder_position();
    {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).unwrap();
        assert!(
            !patch.is_field(),
            "the first bleeding turn past the grace takes the Field: position {landed}"
        );
        assert!(
            patch.is_cultivated(),
            "…and it reverts to the TENDED ground it climbed through, not to wild — the position \
             eats the Field's span first and reaches the tended rung's only once the Field is gone"
        );
        assert_eq!(
            registry.cultivated_count(FactionId(0)),
            1,
            "which is still a plant improvement, because the tended rung under it stands"
        );
    }
    // …**and on the interpolated axis, what that loss cost is a fraction of a percent.** The position
    // sits a single turn's rot below the Field's top, so the ground is still billed almost exactly
    // what a whole Field is billed. That continuity is the shape the bar's deletion relies on; a
    // re-introduced step drops this to `bare_tended` and fails by the whole span.
    let lost_bill = billed_at(&app, landed);
    /// How much of the step from tended to Field crossing the boundary may cost. One turn's rot
    /// (`0.75`) against the Field's 75-unit span is 1%, so the bound is loose enough to survive a rot
    /// retune and tight enough that a step — which gives up the whole difference — fails outright.
    const A_ROUNDING_OF_THE_STEP: f32 = 0.05;
    assert!(
        lost_bill >= whole_field - A_ROUNDING_OF_THE_STEP * (whole_field - bare_tended),
        "losing the Field at {landed} must be a rounding on the keeping, not a step: {lost_bill} \
         against a whole Field's {whole_field} and the bare tended {bare_tended}"
    );

    // **Left alone it bleeds all the way to nothing, and ownership lapses with it — and "nothing" is
    // now the WHOLE BRANCH.** The source has one position, so it has to fall through the tended
    // rung's span as well as the Field's before the ground is unstarted; running only the Field's own
    // 75 units leaves it standing on tended ground with an owner, which is correct and is not the end
    // state this asserts.
    // **The rot rate CHANGES as the position falls between the rungs** — `plant:field` bleeds `0.75`
    // and `plant:tended` `0.5` — so there is no single divisor that predicts the span. Run it until
    // the ground is unstarted, under a bound generous enough for the slower rate and tight enough to
    // catch a decay that has stopped.
    let lapse_bound = ((field_base + field_width) / decay_per_turn).ceil() as u32 * 2 + 4;
    for _ in 0..lapse_bound {
        if app
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .ladder_position()
            <= core_sim::RUNG_UNSTARTED
        {
            break;
        }
        run_turns_untended(&mut app, 1);
    }
    let registry = app.world.resource::<ForageRegistry>();
    let patch = registry.patch(coord).unwrap();
    assert_eq!(
        patch.ladder_position(),
        core_sim::RUNG_UNSTARTED,
        "the investment fully lapses — both rungs of it"
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
    let turns_to_sow = turns_to_sow(&app, coord);
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
    // **NOBODY IS BUILDING HERE, and that is DERIVED rather than cleared** — a declaration counts
    // only for a meter at zero (`forage::patch_build_verb`), so a second crew's stale `Sow` on a
    // finished Field answers `None` on its own.
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    for (label, band) in [("the finisher", first), ("the second crew", second)] {
        let declared = declared_rung(&app, band, coord);
        assert_eq!(
            core_sim::patch_build_verb(&patch, declared),
            None,
            "{label} drives nothing — there is nothing left to sow here"
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
        patch.set_ladder_position(
            core_sim::plant_rung_span(
                core_sim::RungKey::PlantField,
                &core_sim::LadderConfig::builtin(),
            )
            .0 + (part_built_field),
            &core_sim::LadderConfig::builtin(),
        );
        patch.owner = Some(FactionId(0));
    }

    let (_, field_decay) = field_build(&app, coord);
    let grace = field_grace(&app);
    // Run right through the Field's whole bleed. Cultivation must not move by so much as one turn's
    // decay while the Field still has anything left.
    let field_bleed_turns = (part_built_field / field_decay).ceil() as u32;
    run_turns_untended(&mut app, grace + field_bleed_turns);
    {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).expect("patch");
        assert!(
            core_sim::patch_rung_work_done(
                patch,
                core_sim::RungKey::PlantField,
                &core_sim::LadderConfig::builtin()
            ) < field_decay,
            "the Field's own meter is spent (an `f32` subtracted turn by turn lands a few ULPs \
             above zero, which is still `> RUNG_UNSTARTED` — correctly, that is the guard): {}",
            core_sim::patch_rung_work_done(
                patch,
                core_sim::RungKey::PlantField,
                &core_sim::LadderConfig::builtin()
            )
        );
        assert_eq!(
            patch.ladder_position(),
            tended_cost,
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
        core_sim::patch_rung_work_done(
            patch,
            core_sim::RungKey::PlantField,
            &core_sim::LadderConfig::builtin()
        ),
        core_sim::RUNG_UNSTARTED,
        "the Field is fully gone"
    );
    assert!(
        patch.ladder_position() < tended_cost,
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
        .ladder_position();
    assert!(
        stranded > tended_cost,
        "the bleed ate the Sow's own progress and stopped at the ground beneath it: {stranded} \
         against a tended rung that ends at {tended_cost}"
    );
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "so the patch the player paid 25 turns for is still a tended patch"
    );

    // **AND THE STRANDING IS NOT MERELY AVOIDED, IT IS UNREPRESENTABLE.** The state this test was
    // filed against — a Field with progress above tended ground that had slipped below complete —
    // needed two independent meters to write down. With one position the Field's range **begins**
    // where the tended rung's ends, so no reachable position puts a Field over incomplete ground.
    // Swept rather than spot-checked, and past the top, because the bug was a boundary fault.
    let ladder = core_sim::LadderConfig::builtin();
    let (field_base, field_width) =
        core_sim::plant_rung_span(core_sim::RungKey::PlantField, &ladder);
    const SWEEP_STEPS: u32 = 400;
    const PAST_THE_TOP: f32 = 1.25;
    let mut probe = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    let (mut saw_field, mut saw_untended) = (false, false);
    for step in 0..=SWEEP_STEPS {
        let position =
            (field_base + field_width) * PAST_THE_TOP * (step as f32 / SWEEP_STEPS as f32);
        probe.set_ladder_position(position, &ladder);
        assert!(
            !probe.is_field() || probe.is_cultivated(),
            "a Field over incomplete tended ground at position {position} — the very state one \
             position exists to make unwritable"
        );
        saw_field |= probe.is_field();
        saw_untended |= !probe.is_cultivated();
    }
    // The liveness half: a sweep that never reached a Field, or never left the wild rung, would
    // satisfy the implication vacuously.
    assert!(saw_field, "the sweep never reached a Field");
    assert!(saw_untended, "the sweep never saw untended ground");
}

/// **Losing a Field is announced on the `sow` channel, ONCE** — the rung-3 twin of the tended patch's
/// feral line, pushed on the edge the position falls out of the Field's span and never again.
///
/// **The bleed does not stop there, and the second announcement is CORRECT.** One position means the
/// source goes on down through the tended rung's range and loses that rung too, on the `cultivate`
/// channel — the ground really did revert through both, and each 25-turn investment is announced
/// where it was lost. So this asserts the pair by channel rather than counting feral lines: exactly
/// one Sow line for the Field, exactly one Cultivate line for the ground, neither repeated over the
/// hundred bleeding turns between them.
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
        feral_lines_on(&app, "sow"),
        0,
        "nothing is lost, so nothing is announced, while the grace holds"
    );

    // **The Field goes on the FIRST bleeding turn past its grace** — the retention bar is deleted
    // (`docs/plan_standing_upkeep.md` §2.8), and what makes that a rounding rather than a cliff is
    // that the payout fades with the position.
    run_turns_untended(&mut app, 1);
    assert_eq!(
        feral_lines_on(&app, "sow"),
        1,
        "the Field's loss is announced on the turn it happens"
    );
    assert_eq!(
        feral_lines_on(&app, "cultivate"),
        0,
        "…and the ground beneath it is untouched — the position eats the Field first"
    );

    // **Then run it into the ground.** The source walks down through the tended rung's range and
    // loses that too, once, on its own channel — and the Field's line is NOT repeated over the
    // hundred bleeding turns in between, which is the thing this test exists to catch.
    let survives = unmaintained_field_turns_before_loss(&app, coord);
    run_turns_untended(&mut app, survives);
    assert_eq!(
        feral_lines_on(&app, "sow"),
        1,
        "the Field's loss is announced once, not every turn of the bleed that follows"
    );
    assert_eq!(
        feral_lines_on(&app, "cultivate"),
        1,
        "and the tended ground's loss is announced once too, on its own channel — the source really \
         did revert through both rungs"
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
/// **Feral lines on ONE verb's channel** — `action=sow` for a lost Field, `action=cultivate` for the
/// tended ground beneath it.
///
/// **The channel is not optional any more.** With one position a long bleed walks the source down
/// through *both* plant rungs, so it genuinely loses two and genuinely announces two — a counter that
/// matched only on `"gone feral"` reads `2` and looks like a double-fire on one rung. The rungs are
/// distinguishable exactly where the feed already distinguishes them: the `action=` token.
fn feral_lines_on(app: &App, action: &str) -> usize {
    let token = format!("action={action}");
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.label.contains("gone feral"))
        .filter(|entry| {
            entry
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains(&token))
        })
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
    // **THE FIXTURE'S SHAPE MOVED, THE CLAIM DID NOT.** "A Field on ground that was never tended"
    // was a two-meter state (`field_progress` full, `cultivation_progress` zero) and is now
    // unwritable: the Field's span sits above the tended rung's, so a Field **is** tended. What the
    // test is about is untouched — a `Cultivate` declared on a source with nothing left to cultivate
    // must be handed back rather than stalling — and the position that fixes is the top of the branch.
    assert_eq!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .ladder_position(),
        {
            let ladder = core_sim::LadderConfig::builtin();
            let (base, width) = core_sim::plant_rung_span(core_sim::RungKey::PlantField, &ladder);
            base + width
        },
        "the fixture stands at the top of the plant branch, where a Cultivate has nothing to do"
    );

    run_turns_with_forage(&mut app, 1);

    // **The declaration is DEAD, not stalled** — the field meter governs (newest first), so a
    // `Cultivate` on a Field derives to `None` and drives nothing. No clearing pass is involved.
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    let declared = declared_rung(&app, band, coord);
    assert_eq!(
        core_sim::patch_build_verb(&patch, declared),
        None,
        "the crew drives nothing on a rung it can never build on this source"
    );
    assert_eq!(
        patch.ladder_position(),
        {
            let ladder = core_sim::LadderConfig::builtin();
            let (base, width) = core_sim::plant_rung_span(core_sim::RungKey::PlantField, &ladder);
            base + width
        },
        "…and the position never moved — a handed-back verb banks nothing, and the source is still \
         exactly where the fixture seated it"
    );
}

/// **THE EMPTY KIT, NAMED ON A FIXTURE'S QUEUE ENTRY** — an isolation, not a default.
///
/// It rides the **entry** because that is where a build's kit lives
/// (`docs/plan_standing_upkeep.md` §4.7a ②); a kit on the `builders` row is not an input at all.
/// An absent kit means *derive from this entry's web*, and the roster's answer (`tillage` for a
/// patch, `hurdling` for a herd) adds `+0.5` work per covered worker per turn. A start-stocked band holds a
/// unit per worker and a half, so at the crews these fixtures staff every builder is geared and the
/// pool delivers half again what it asserts, moving every pacing claim below. Naming `none` holds
/// the gear axis at its identity so these arms measure the **crew**, exactly as
/// `FaunaConfig::without_retreat` holds the retreat at its identity across the hunt suites. The
/// geared default is pinned in `core_sim/tests/build_turns_closed_form.rs`.
fn bare_builders() -> core_sim::KitChoice {
    core_sim::EquipmentConfig::builtin()
        .kit("none")
        .expect("the shipped roster carries the empty kit")
}
