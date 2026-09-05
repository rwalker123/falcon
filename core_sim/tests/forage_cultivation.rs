//! Cultivation as an **explicit policy with an investment cost** (Intensification Rung 1a/1b).
//!
//! Sustain-foraging a Thriving patch **teaches the faction Cultivation** (Rung 1b knowledge, earned by
//! doing) but no longer tames the patch — the old free auto-accrual is gone, because "same labor, same
//! tile, no cost" made cultivating unconditionally correct and erased the decision. Cultivating is now
//! the `Cultivate` improvement: while preparing, the patch yields only
//! `cultivating_yield_fraction × its Sustain (MSY) ceiling` (the crew is clearing and planting, not
//! gathering) and accrues `cultivation_progress` at `progress_per_turn`. At `1.0` it becomes a
//! **tended patch**: worked, place-local, paying the full managed yield without being drawn down, and
//! going **feral** if abandoned. The plant mirror of `fauna_husbandry.rs`; world setup mirrors it too.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::TakeSelection;
use core_sim::NO_CREW_ON_THIS_ACTIVITY;
use core_sim::{
    advance_cultivation, advance_forage_regrowth, advance_labor_allocation, commit_payoff,
    commit_yield_ratio, default_species_for_rung, scalar_from_f32, scalar_one, scalar_zero,
    spawn_initial_forage, spawn_initial_world, tile_flora_composition, tile_forage_capacity,
    wild_payoff, CommandEventLog, CultureManager, DiscoveryProgressLedger, EcologyPhase, FactionId,
    FactionInventory, FaunaConfigHandle, FoodModule, FoodModuleTag, FoodSiteEntry,
    FoodSiteRegistry, ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap,
    HerdRegistry, HerdTelemetry, Improvement, LaborAllocation, LaborAssignment, LaborConfigHandle,
    LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause,
    PopulationCohort, RungKey, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, SourcePriority, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle,
    CULTIVATION_DISCOVERY_ID, FOOD, ONE_TENDER_LOAD, PER_WORKER_OUTPUT, RUNG_COST_UNSCALED,
    WHOLLY_UNSUPPLIED,
};

/// Grant faction-level **Cultivation** knowledge (Rung 1b) directly via the ledger — the gate the
/// `Cultivate` policy checks. Tests of the *investment* mechanic seed it; the earned-knowledge ladder
/// itself has its own test below.
fn grant_cultivation_knowledge(app: &mut App, faction: FactionId) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, CULTIVATION_DISCOVERY_ID, scalar_one());
}

/// Whole-worker head-count assigned to the forage — large enough that the per-worker gather cap never
/// binds, so every take is **ceiling-bound** (which is what makes the Cultivate dip measurable as a
/// clean fraction of the Sustain ceiling).
const FORAGE_WORKERS: u32 = 5000;

/// **One forager**, so the crew's throughput is the binding term rather than the patch's standing
/// stock. Since `docs/plan_harvest_floor.md` §3.1 the build dip multiplies `workers × per_worker`
/// rather than the take ceiling, so it is invisible at a staffing the stock binds — a build costs
/// yield only while hands are the scarce thing, which is the legible "hire four times the people"
/// half of the change.
const SOLE_FORAGER: u32 = 1;

/// Float slack for provisions comparisons (fixed-point conversion + multiplication order).
const EPSILON: f32 = 1e-4;

/// **The floor at which `learn_multiplier` is exactly ×1.0** — the food peak, and the floor a fresh
/// assignment carries. Passed wherever a build rate is read for its *stated* pace rather than for a
/// floor's fraction of it (`docs/plan_harvest_floor.md` §3).
const FOOD_PEAK_FLOOR: f32 = core_sim::MSY_BIOMASS_FRACTION;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    // **THE SHARED HARNESS MAP.** This file pinned the seed by hand long before there was a name for
    // it; the literal is now `core_sim::HARNESS_MAP_SEED`, so the repo has one harness map and not
    // two that happen to agree.
    config.map_seed = core_sim::HARNESS_MAP_SEED;
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
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    // **The road ledger `advance_labor_allocation` counts spare road keepers against.** Empty
    // is the shipped turn-1 state — no traffic has worn anything in yet — so this harness's
    // keeping numbers are the roadless reading they have always been.
    app.world.insert_resource(core_sim::RoadRegistry::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::for_a_stocked_fixture());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// **The patch's standing crop as a fraction of its capacity** — above Sustain's escapement floor
/// (`fauna::MSY_BIOMASS_FRACTION`, `K/2`), so a Sustain gather has stock standing above it. At the
/// floor exactly a Sustain take is honestly zero, which is the one reading these fixtures must not
/// measure.
const STOCKED_STANDING_CROP: f32 = 0.8;

/// A `FoodModuleTag` tile that carries a seeded patch. Primes the patch above its escapement floor
/// (Thriving, with regrowth headroom) so the take is a real, ceiling-bound number. Returns the tile
/// entity + its coord.
fn prime_thriving_patch(app: &mut App) -> (bevy::prelude::Entity, UVec2) {
    let coord = {
        // The tile must grow something the **tended** rung can commit to (Flora Roster S1): a basket
        // whose whole `cultivation_ceiling` is `wild` is legitimately uncultivable, so a fixture that
        // grabbed the first seeded patch would test the refusal rather than the rung.
        //
        // **And since S2 the default crop must actually WIN there** (`docs/plan_flora_roster.md` §4.3).
        // The tended regrowth boost is retired to a neutral `1.0`, so tending pays only through
        // concentration + conversion — which loses on marginal ground (a low-share crop, e.g. hazel on
        // RollingHills at ~0.68×). The tests below assert `tended > wild`, so the fixture is pinned to
        // ground where the default crop's Cultivate ratio exceeds `1.0` — computed with the sim's own
        // payoff functions, so "the crop wins" is the sim's verdict, not a re-derivation.
        //
        // # ⛔ AND IT TAKES THE RICHEST SUCH TILE, NOT THE FIRST
        //
        // The plant upkeep is quoted **per tender-load** — the tile's own `K` over
        // `forage.cultivation.capacity_per_tender` — so what a patch owes is a fact about the
        // ground. Taking the first qualifying tile in query order landed this file on 25-capacity
        // tundra, where a tended patch's whole bill is a quarter of one hand: *"covering the demand
        // takes more than one keeper"*, *"a lone builder is under the rate"* and *"the richer of two
        // positions owes more"* are then unreachable, and the fixtures that assert them pass or fail
        // for reasons that have nothing to do with the mechanic. The richest qualifying tile is real
        // farmland and reaches every one of those states.
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .filter(|(tile, _)| registry.patch(tile.position).is_some())
            .filter(|(tile, _)| {
                let composition = tile_flora_composition(&flora, &labor.forage, tile, map_seed);
                let Some(species) =
                    default_species_for_rung(&composition, &flora, RungKey::PlantTended)
                else {
                    return false;
                };
                let capacity = tile_forage_capacity(&labor.forage, tile);
                let payoff = commit_payoff(
                    tile.position,
                    capacity,
                    &species,
                    &composition,
                    &flora,
                    &labor.forage,
                    1.0,
                    RungKey::PlantTended,
                );
                let wild = wild_payoff(
                    tile.position,
                    capacity,
                    &composition,
                    &flora,
                    &labor.forage,
                    1.0,
                );
                commit_yield_ratio(payoff, wild) > 1.0
            })
            .max_by(|(a, _), (b, _)| {
                tile_forage_capacity(&labor.forage, a)
                    .total_cmp(&tile_forage_capacity(&labor.forage, b))
                    // Ties broken on the coord, so the pick is deterministic whatever order the
                    // query hands the tiles back in.
                    .then_with(|| (a.position.y, a.position.x).cmp(&(b.position.y, b.position.x)))
            })
            .map(|(tile, _)| tile.position)
            .expect("a FoodModuleTag tile whose default crop out-yields the wild basket")
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        // **Above Sustain's escapement floor** (`K/2`), so a Sustain gather has standing stock to
        // take: at the floor exactly, a Sustain row is honestly `+0.00`
        // (`docs/plan_harvest_floor.md` §1) and these fixtures would measure an empty turn.
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
    }
    declare_gathering_site(app, coord);
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    (entity, coord)
}

/// **STATE THE FIXTURE'S GATHERING SITE.** Every plant rung carries
/// `RungSiteRequirement::requires_gathering_site`, so a patch seeded on ground the curated
/// `FoodSiteRegistry` does not name is ground no band may legally be put to work on — a world the sim
/// cannot produce (`.claude/rules/core_sim/cultivation.md` → "Gathering is SITE-BOUND").
///
/// It went unnoticed here while the only reader of the rule was the `assign_labor` / `cultivate`
/// command path, which these fixtures bypass by writing the `LaborAllocation` directly. The
/// **projection** reads it — a quote for a rung the command would refuse is the defect it exists to
/// avoid — so the fixture has to say what the map would have said.
fn declare_gathering_site(app: &mut App, coord: UVec2) {
    let mut sites = app.world.resource_mut::<FoodSiteRegistry>();
    if sites.is_site(coord) {
        return;
    }
    let module = FoodModule::SavannaGrassland;
    let mut entries = sites.sites().to_vec();
    entries.push(FoodSiteEntry {
        position: coord,
        module,
        kind: module.site_kind(),
        seasonal_weight: 1.0,
    });
    sites.set_sites(entries);
}

/// One Forage row, so the two shapes above cannot drift.
fn forage_row(patch: UVec2, policy: f32, foragers: u32) -> LaborAssignment {
    LaborAssignment {
        target: LaborTarget::Forage {
            tile: patch,
            floor: policy,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        },
        workers: foragers,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    }
}

/// **Declare or withdraw the build** on a band's (single) Forage assignment — what the client's
/// checkbox does. Since `docs/plan_standing_upkeep.md` §2.5 that is TWO facts and a fixture needs
/// both: the **queue entry** (what is being raised) and the band's **`builders` pool** (the hands
/// raising it). The take crew and the stance stay put, and completion retires the entry itself.
fn set_forage_improvement(
    app: &mut App,
    band: bevy::prelude::Entity,
    improvement: Option<Improvement>,
) {
    let builders = build_crew(app);
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("band forages");
    let LaborTarget::Forage { tile, .. } = allocation
        .assignments
        .first()
        .expect("a Forage assignment")
        .target
        .clone()
    else {
        panic!("the fixture band forages");
    };
    let source = core_sim::BuildSource::Patch(tile);
    match improvement {
        Some(declared) => {
            assert!(allocation.enqueue_build(source, core_sim::BuildJob::Rung(declared)));
            match allocation
                .assignments
                .iter_mut()
                .find(|assignment| assignment.target == LaborTarget::Builders)
            {
                Some(row) => row.workers = builders,
                None => allocation.assignments.push(LaborAssignment {
                    target: LaborTarget::Builders,
                    workers: builders,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }),
            }
        }
        None => {
            allocation.unqueue_build(&source);
            allocation
                .assignments
                .retain(|assignment| assignment.target != LaborTarget::Builders);
        }
    }
}

/// **What a band has queued on a patch, as a rung verb** — the 6b reading of what a fixture used to
/// get off the row's `improvement` field (`docs/plan_standing_upkeep.md` §2.5).
fn declared_rung(app: &App, band: bevy::prelude::Entity, tile: UVec2) -> Option<Improvement> {
    match app
        .world
        .get::<LaborAllocation>(band)
        .expect("the fixture band keeps its allocation")
        .build_queue_entry(&core_sim::BuildSource::Patch(tile))
        .map(|entry| entry.declared)
    {
        Some(core_sim::BuildJob::Rung(improvement)) => Some(improvement),
        Some(core_sim::BuildJob::ExtendPen) | None => None,
    }
}

/// A band foraging `patch`. `improvement` is the second axis; the **stance** is `Sustain` throughout
/// this file, which measures the `Cultivate` build rather than the harvest pressure beside it.
fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    improvement: Option<Improvement>,
) -> bevy::prelude::Entity {
    spawn_forager_of(app, tile, patch, improvement, FORAGE_WORKERS)
}

/// **A crew BUILDING the patch, staffed at the rung's own [`build_crew`]** — not at
/// [`FORAGE_WORKERS`]. See `build_crew` for why the two numbers had to come apart.
///
/// **It staffs the band's keeping too**, because since `docs/plan_standing_upkeep.md` §4.6a the
/// keeping pool owes a meter carrying work from the **first work banked**: a build fixture with the
/// `agriculture` role empty is not running at its stated pace, it is racing its own rot, and
/// `turns_to_prepare` — a pure `cost / accrual` — would then over-promise on every caller. A test
/// that wants an **unkept** build restates the role with `set_maintain_workers(…, 0)`, which is what
/// `a_kept_cultivate_finishes_in_its_stated_turns_and_an_unkept_one_is_slower` does from both sides.
fn spawn_builder(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    improvement: Improvement,
) -> bevy::prelude::Entity {
    let crew = build_crew(app);
    let band = spawn_forager_of(app, tile, patch, Some(improvement), crew);
    set_maintain_workers(app, band, tended_keeping_crew());
    band
}

/// **One keeper** — what either plant rung's sub-worker demand rounds up to
/// (`the_upkeep_crew_needed_is_the_demand_in_whole_workers`), and therefore the whole cost of holding
/// a completed improvement.
///
/// **A BUILD FIXTURE HAS TO STAFF IT** (`docs/plan_standing_upkeep.md` §4.6a). The keeping pool owes
/// a meter carrying work from the first work banked, so a Cultivate with the role empty is racing its
/// own rot rather than running at its stated pace — which is what
/// `a_kept_cultivate_finishes_in_its_stated_turns_and_an_unkept_one_is_slower` pins from both sides.
const A_KEEPER: u32 = 1;

/// [`spawn_forager`] with an explicit head-count — the dip test needs a crew the carry binds.
fn spawn_forager_of(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    improvement: Option<Improvement>,
    foragers: u32,
) -> bevy::prelude::Entity {
    spawn_forager_at(app, tile, patch, improvement, foragers, FOOD_PEAK_FLOOR)
}

/// [`spawn_forager_of`] with an explicit **floor** — the pressure dial, which since
/// `docs/plan_harvest_floor.md` §3 also paces the build.
fn spawn_forager_at(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    improvement: Option<Improvement>,
    foragers: u32,
    policy: f32,
) -> bevy::prelude::Entity {
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
                working: scalar_from_f32((foragers * 3) as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
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
                // **A pool of the same size staffs the build, and ONLY where one is declared** —
                // what this fixture meant when one crew did every job
                // (`docs/plan_standing_upkeep.md` §2.5). A role row standing at zero would eat
                // headroom the keeping arms need.
                //
                // **NO KEEPER, and that is the point.** A meter still being raised is owed its
                // keeping (§2.4), so these fixtures measure a build's stated pace with nobody on
                // that role and let the caller staff it when the measurement needs it.
                assignments: improvement
                    .map(|_| {
                        vec![
                            forage_row(patch, policy, foragers),
                            LaborAssignment {
                                target: LaborTarget::Builders,
                                workers: foragers,
                                kit: None,
                                priority: SourcePriority::default(),
                                upkeep_kit: None,
                            },
                        ]
                    })
                    .unwrap_or_else(|| vec![forage_row(patch, policy, foragers)]),
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

/// One turn's forage pipeline in stage order: Logistics (regrowth, cultivation decay) then Population
/// (labor allocation resolves the take and accrues the investment).
fn run_turns_with_forage(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_forage_regrowth);
        app.world.run_system_once(advance_cultivation);
        app.world.run_system_once(advance_labor_allocation);
    }
}

/// **The `plant:tended` rung's neglect grace**, read off the shipped ladder rather than restated as a
/// literal — the consecutive un-worked turns the feral bleed forgives before it starts.
fn tended_grace(app: &App) -> u32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        // **The UPKEEP's grace** — the plant branch counts consecutive turns of *shortfall* now, so
        // the build's own grace is absent on both plant rungs and this is the live number.
        .upkeep_grace_turns()
}

/// **Hold `coord` for one turn** — stamp what a keeper crew supplied, exactly as the labor arm
/// would, so the next `advance_cultivation` reads a met demand. The fixture's stand-in for
/// `maintain <faction> forage <x> <y> <workers>`.
fn keep_patch_for_a_turn(app: &mut App, coord: UVec2) {
    let ladder = app.world.resource::<LadderConfigHandle>().get().clone();
    let forage = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .forage
        .clone();
    // **The bill is quoted per tender-load of this ground**, so the stand-in has to resolve the
    // tile the way the labor arm does, not hand the seam a bare one.
    let tile_capacity = plant_tile_capacity(app, coord);
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch");
    patch.upkeep_supplied = core_sim::patch_upkeep_demand(patch, &ladder, tile_capacity, &forage);
}

/// Turns with no active band: only the Logistics-stage systems run.
fn run_turns_untended(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_forage_regrowth);
        app.world.run_system_once(advance_cultivation);
    }
}

fn progress_of(app: &App, coord: UVec2) -> f32 {
    app.world
        .resource::<ForageRegistry>()
        .patch(coord)
        .map(|p| p.ladder_position())
        .unwrap_or(0.0)
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

/// The plant rung-2 build dials — **what [`build_crew`] produces in one turn** and the feral rate
/// in absolute work units — read off the ladder's `plant:tended` rung
/// (`intensification_ladder.json`), the same seam the sim drives cultivation with.
///
/// **The feral rate is the rung's own `upkeep.meter_decay.per_turn`** — what a *wholly*
/// unmaintained patch loses per turn past its grace (`docs/plan_standing_upkeep.md` §2.4). It is
/// deliberately **not** the demand: the demand is what holding costs and the rate is how fast it
/// rots, and welding them was what made either one unretunable. It is the same number the retired
/// `decay_fraction_per_turn` produced, which is why every pace below is unchanged.
fn cultivation_config(app: &App) -> (f32, f32) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let tended = ladder.rung(RungKey::PlantTended);
    assert!(
        tended.build_accrual(
            Some(Improvement::Cultivate),
            true,
            build_crew(app),
            core_sim::NO_BUILD_GEAR
        ) > 0.0,
        "the build fixtures must staff a crew, or every pace assertion below compares nothing to \
         nothing"
    );
    (
        // The crew IS the throughput now (`docs/plan_unit_costed_work.md` §1.2), so this reads at
        // the head count the build fixtures actually staff — computing it at any other would
        // describe a build nobody here is running.
        tended.build_accrual(
            Some(Improvement::Cultivate),
            true,
            build_crew(app),
            core_sim::NO_BUILD_GEAR,
        ),
        unmaintained_bleed(tended),
    )
}

/// **What a rung loses on a turn nobody holds it at all** — its `meter_decay.per_turn`, read through
/// the seam the sim bleeds with rather than off the config, so a retune of either dial reaches every
/// pace in this file.
fn unmaintained_bleed(rung: &core_sim::RungDef) -> f32 {
    rung.upkeep_decay(WHOLLY_UNSUPPLIED, WELL_PAST_ANY_GRACE)
}

/// The turn count a wholly unmaintained rung is on once every shipped grace is spent — larger than
/// any of them, so [`unmaintained_bleed`] is certainly reading a biting decay.
const WELL_PAST_ANY_GRACE: u16 = 32;

/// **The crew a BUILD test staffs, and it is deliberately NOT [`FORAGE_WORKERS`].**
///
/// The two numbers exist for different reasons and only one of them is about the build. 5000 is
/// chosen so a *take* is ceiling-bound rather than labor-bound; at that head count a 50-unit
/// Cultivate finishes in a single turn — leaving no part-prepared patch for a decay, a grace or a
/// completion test to stand on. So the build fixtures staff **two**, the staffing the shipped cost
/// was priced against (`docs/plan_unit_costed_work.md` §3) and, until the ladder stopped declaring
/// one, the `plant:tended` rung's own `crew_needed`. **The one-turn over-crewed build is real and is
/// pinned on purpose**, by `over_crewing_a_build_is_no_longer_capped`.
///
/// It is a **fixture** number now rather than a config reading, because the player states a build's
/// crew (`docs/plan_standing_upkeep.md` §2.2) and there is no rung-level staffing left to read.
///
/// # AND NOTHING IS ADDED TO IT FOR THE MAINTENANCE RATE
///
/// It carried `+ upkeep_crew_needed` for one slice, because the rate was then a tax on building and a
/// bare two hands netted zero. **A build crew supplies none of the rate**
/// (`docs/plan_standing_upkeep.md` §4.6a) — the band's keeping pool owes it at any meter fullness —
/// so two hands bank two worker-turns and the padding was correcting for nothing. Leaving it in would
/// have meant every pace in this file measured **four** builders while calling it the reference, and
/// the plant web's pacing-neutrality claim (`50 work / crew 2 = 25 turns`) is exactly the number that
/// would have gone missing.
fn build_crew(_app: &App) -> u32 {
    /// The staffing the shipped `plant:tended` `work_cost` was priced at, so `50 / 2 = 25` turns is
    /// what this file measures.
    const THE_REFERENCE_CREW: u32 = 2;
    THE_REFERENCE_CREW
}

/// **A FINITE COUNT, OR NOTHING** — the count half of a source's published countdown
/// (`intensification::BuildTurns`). The *never* / *no-estimate* pair is a wire fact and is asserted
/// on the encoded snapshot in `build_turns_on_the_wire.rs`; the paces in this file read counts.
fn published_count(turns: Option<core_sim::BuildTurns>) -> Option<u32> {
    match turns {
        Some(core_sim::BuildTurns::Turns(count)) => Some(count),
        _ => None,
    }
}

/// **THE KEEPERS THAT EXACTLY COVER `plant:tended`'S DEMAND** — what a fixture puts on the band's
/// `agriculture` role so the rung it is testing is **held**, from the first work banked
/// (`docs/plan_standing_upkeep.md` §4.6a).
///
/// It used to be read as the *minimum viable build crew* — the threshold at or below which a build
/// banked nothing — and that threshold does not exist: a build crew supplies none of the rate, so a
/// lone builder banks a whole worker-turn on this rung. What the number still is, exactly, is the
/// keeping's own `workers_needed`.
fn tended_keeping_crew() -> u32 {
    core_sim::LadderConfig::builtin()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(fixture_tender_loads())
}

/// **THE MEASURE THIS FILE'S PLANT FIXTURES ARE BILLED PER** — both plant rungs quote their
/// `upkeep.work_per_turn` per **tender-load** (`forage::patch_tender_loads`), so what a fixture owes
/// depends on the ground [`prime_thriving_patch`] found: the tile's own `K` over
/// `forage.cultivation.capacity_per_tender`.
///
/// **Derived from the fixture's own tile, never assumed to be [`ONE_TENDER_LOAD`].** The reference
/// tile reads exactly one load by construction, and this file's search does *not* land there — it
/// takes the first site whose default crop wins, which on the fixture map is thin ground. Memoised
/// because the search costs a whole worldgen and the answer is a constant of the fixture.
fn fixture_tender_loads() -> f32 {
    static LOADS: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *LOADS.get_or_init(|| {
        let mut app = spawn_world();
        let (_tile, coord) = prime_thriving_patch(&mut app);
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let loads = core_sim::patch_tender_loads(plant_tile_capacity(&app, coord), &labor.forage);
        assert!(
            loads > core_sim::NO_TENDER_LOAD,
            "fixture: the ground these tests work must present land to tend, or every upkeep \
             number in this file is an honest zero and nothing here asserts anything"
        );
        loads
    })
}

/// **The ground under `coord`, in the unit the `patch_*` upkeep seams take** — the tile's own forage
/// `K` (`forage::tile_forage_capacity`), which each of them divides by `capacity_per_tender` to reach
/// the tender-loads the bill is quoted per.
fn plant_tile_capacity(app: &App, coord: UVec2) -> f32 {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let tile_entity = app
        .world
        .resource::<core_sim::TileRegistry>()
        .index(coord.x, coord.y)
        .expect("the fixture tile is on the map");
    let ground = app
        .world
        .get::<Tile>(tile_entity)
        .expect("the fixture tile carries a Tile");
    tile_forage_capacity(&labor.forage, ground)
}

/// **THE FIXTURE GROUND IS NOT THE REFERENCE TILE, and that is worth pinning** — every upkeep number
/// in this file is the rung's declared rate times [`fixture_tender_loads`], and a fixture that
/// happened to sit at exactly one load would make the scaling invisible: every assertion would pass
/// against the retired flat rate too.
#[test]
fn the_fixture_ground_is_priced_off_its_own_tile_and_not_the_reference_one() {
    let loads = fixture_tender_loads();
    assert!(
        (loads - ONE_TENDER_LOAD).abs() > 1e-3,
        "this file's ground reads {loads} tender-loads — indistinguishable from the reference tile, \
         so nothing here can tell a scaled bill from a flat one"
    );
}

/// The whole `plant:tended` job, in work units — what a build test divides by to get its turns.
fn cultivate_cost(app: &App) -> f32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .build_cost(RUNG_COST_UNSCALED)
        .expect("the tended rung builds")
}

/// **Turns [`build_crew`] needs to prepare a whole patch**, `ceil(work_cost / work per turn)` —
/// turns are an output now, so a bare `1.0 / rate` no longer means anything.
fn turns_to_prepare(app: &App) -> u32 {
    core_sim::build_turns_remaining(
        cultivate_cost(app),
        core_sim::RUNG_UNSTARTED,
        cultivation_config(app).0,
    )
    .expect("a staffed Cultivate finishes")
}

/// **Turns a fully-untended patch takes to bleed a completed rung all the way back to nothing** —
/// `ceil(cost / decay)`, plus slack for the one-turn flag lag and the rung's grace.
fn turns_to_go_fully_feral(app: &App) -> u32 {
    let (_, decay) = cultivation_config(app);
    (cultivate_cost(app) / decay).ceil() as u32 + 2
}

/// **HOW MANY WHOLLY UNMAINTAINED TURNS A COMPLETED TENDED PATCH SURVIVES** — the grace, plus the
/// turns its own rot rate takes to erode the meter from its cost to below its retention bar
/// (`docs/plan_standing_upkeep.md` §2.4).
///
/// **This is the number the reported bug is about.** A completed meter sits *exactly* at its own
/// cost, so a `progress >= cost` predicate made that answer `grace + 1` on every rung — finish a
/// Cultivate and the patch could be out of *tended* before its keepers were assigned, which no grace
/// and no rate could fix because the loss was a **threshold test**.
///
/// Derived from the rung's own dials rather than written down, so a retune of the bar, the rate or
/// the grace moves every test that reads it.
fn unmaintained_turns_before_the_rung_is_lost(app: &App) -> u32 {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let rung = ladder.rung(RungKey::PlantTended);
    let cost = cultivate_cost(app);
    // **THE WHOLE SPAN IS ERODABLE.** The retention bar is deleted with the cliff it patched
    // (`docs/plan_standing_upkeep.md` §2.8) — a rung is achieved at the top of its span and lost the
    // instant the position dips — so what a bleed must eat before the rung goes is the rung's own
    // cost, and it goes on the FIRST bleeding turn past the grace.
    let erodable = cost;
    let bleed = unmaintained_bleed(rung);
    // The rung is lost the turn the position falls **below** the top of its span, which is the
    // first bleeding turn past the grace — the `erodable / bleed` term is now inert (the position
    // sits exactly at the top, so any bleed at all takes it below) and is kept only so a future
    // rung whose loss point moves back down has somewhere to say so.
    let _ = erodable / bleed;
    rung.upkeep_grace_turns() + 1
}

/// One turn of the pipeline under `improvement` on a fresh identical world; returns the provisions
/// the band was paid. Lets a test compare the Cultivate **dip** against the undipped Sustain baseline
/// without re-deriving the MSY formula anywhere.
fn one_turn_yield(improvement: Option<Improvement>) -> f32 {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, improvement);
    run_turns_with_forage(&mut app, 1);
    provisions_f32(&mut app)
}

/// **The yield of the LAST of `turns` worked turns** — the rate a patch pays once a gather has held
/// it at its escapement floor, rather than the one-off windfall of the first harvest of an untouched
/// stand (`docs/plan_harvest_floor.md` §1). Any comparison between two *rungs* has to be taken here:
/// the opening stock is the same on both (it is `B − K/2`, which knows nothing about the rung), so
/// only the steady state can show what tending bought.
fn steady_turn_yield(improvement: Option<Improvement>, turns: u32) -> f32 {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, improvement);
    run_turns_with_forage(&mut app, turns.saturating_sub(1));
    let before = provisions_f32(&mut app);
    run_turns_with_forage(&mut app, 1);
    provisions_f32(&mut app) - before
}

/// **The free path is gone.** Sustain-foraging a Thriving patch still teaches the faction Cultivation
/// (knowledge is earned by doing), but it never accrues `cultivation_progress` — not even once the
/// faction knows Cultivation. Cultivating costs something now, and the player must choose to pay it.
#[test]
fn sustain_forage_teaches_cultivation_but_never_tames_the_patch() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    spawn_forager(&mut app, tile, coord, None);

    run_turns_with_forage(&mut app, 5);
    let learned = app
        .world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FactionId(0), CULTIVATION_DISCOVERY_ID)
        .to_f32();
    assert!(
        learned > 0.0 && learned < 1.0,
        "Sustain-forage still earns Cultivation knowledge: {learned}"
    );
    assert_eq!(
        progress_of(&app, coord),
        0.0,
        "Sustain must not silently tame the patch"
    );

    // Even with the knowledge complete, Sustain accrues nothing — Cultivate is the only path.
    grant_cultivation_knowledge(&mut app, FactionId(0));
    run_turns_with_forage(&mut app, 10);
    assert_eq!(
        progress_of(&app, coord),
        0.0,
        "knowing Cultivation must not resurrect the free auto-accrual"
    );
    assert!(!app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .unwrap()
        .is_cultivated());
}

/// **The investment cost is HANDS, and the gatherers beside them are untouched**
/// (`docs/plan_standing_upkeep.md` §2.2). A Cultivate is staffed in its own right, so what it costs
/// is the people who are clearing instead of gathering — a number the player typed — and the patch
/// keeps paying whatever the crew still on it can carry.
///
/// **Both staffings are asserted, and they agree**: the price is in the allocation, so it does not
/// depend on whether hands or the patch's standing stock is the binding term. Under the retired
/// `yield_fraction_while_building` an ample crew paid nothing at all for the build, which is the
/// asymmetry this test used to pin.
#[test]
fn cultivate_leaves_the_gatherers_take_alone_and_keeps_the_patch_healthy() {
    let sparse_yield = |improvement| {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        spawn_forager_of(&mut app, tile, coord, improvement, SOLE_FORAGER);
        run_turns_with_forage(&mut app, 1);
        provisions_f32(&mut app)
    };
    let sustain_yield = sparse_yield(None);
    let cultivating_yield = sparse_yield(Some(Improvement::Cultivate));
    assert!(
        sustain_yield > 0.0,
        "baseline Sustain yield must be positive"
    );

    let mut app = spawn_world();
    assert!(
        (cultivating_yield - sustain_yield).abs() < EPSILON,
        "the lone forager gathers exactly what they gathered before the Cultivate was staffed \
         beside them: {cultivating_yield} vs {sustain_yield}"
    );
    // …and the ample crew reads the same way, which is what the separate allocation bought: an
    // ample crew used to escape the dip entirely, because the patch's stock — not the crew — was
    // what bound, so the cost depended on a regime the player could not see.
    assert!(
        one_turn_yield(None) > 0.0,
        "the ample-crew baseline must be a real take"
    );
    assert!(
        (one_turn_yield(Some(Improvement::Cultivate)) - one_turn_yield(None)).abs() < EPSILON,
        "an ample crew reads the same, build or no build"
    );

    // Over a full preparation the patch never leaves Thriving — a preparing crew draws nothing at
    // all, so there is no depletion to survive.
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    let turns = turns_to_prepare(&app);
    run_turns_with_forage(&mut app, turns);
    assert_eq!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .ecology_phase,
        EcologyPhase::Thriving,
        "the crew works the patch at the floor they chose throughout — preparing changes nothing \
         about the draw"
    );
}

// **RETIRED: `a_low_floor_cultivate_takes_materially_longer_than_a_food_peak_one`** — §0.3's
// measurement inverted: the harshest draw used to be strictly dominant while building (dipped ×0.25,
// every stance completed a 25-turn Cultivate on schedule and the deepest paid 3.8× the food), and
// `learn_multiplier` on the accrual made pulling harder cost turns instead.
//
// **That rule was written when one crew did both jobs.** A Cultivate is staffed in its own right now
// (`docs/plan_standing_upkeep.md` §2.2), so the builders are not pulling on the patch and there is no
// pressure of theirs for a floor to describe — and a build crew on a patch nobody is gathering has no
// floor to read at all. §0.3's defect cannot recur for the stronger reason that there is no shared
// crew for a deep draw to build with for free. The floor still paces the LESSON
// (`RungDef::knowledge_accrual`), and the pacing-neutrality of taking it off the build is pinned by
// `intensification::taking_the_floor_off_the_build_rate_is_pacing_neutral_at_the_food_peak`.

/// **The first worked turn commits the ground to one named plant** (Flora Roster S1) — and, until
/// the improvement completes, that commitment costs and buys nothing: the patch still carries the
/// tile's full `K`. Rung 1's neutrality is the claim this asserts from the other side.
#[test]
fn cultivate_commits_the_ground_to_a_plant_and_leaves_rung_one_untouched() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let capacity_before = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .unwrap()
        .carrying_capacity;

    // A wild Sustain gather commits nothing — rung 1 never picks a crop.
    let band = spawn_forager(&mut app, tile, coord, None);
    run_turns_with_forage(&mut app, 1);
    assert_eq!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .species,
        None,
        "gathering the wild basket is not a commitment"
    );

    set_forage_improvement(&mut app, band, Some(Improvement::Cultivate));
    // **Re-staffed to the build crew**: [`FORAGE_WORKERS`] would finish the whole 50-unit job in
    // this one turn, and what is under test is a patch *still being prepared*.
    let crew = build_crew(&app);
    set_forage_workers(&mut app, band, crew);
    run_turns_with_forage(&mut app, 1);
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .unwrap()
        .clone();
    let committed = patch
        .species
        .as_deref()
        .expect("the first Cultivate turn commits the ground to a plant");
    let (flora, labor) = (
        app.world.resource::<core_sim::FloraConfigHandle>().get(),
        app.world.resource::<LaborConfigHandle>().get(),
    );
    assert!(
        flora.species[committed]
            .cultivation_ceiling
            .allows_cultivate(),
        "the auto-pick must be a plant that can actually be tended"
    );
    assert_eq!(
        patch.carrying_capacity, capacity_before,
        "a patch still being prepared carries the tile's full K — nothing is displaced yet"
    );
    // The tile's own basket, resolved the way the sim does — the *from* end of the interpolation
    // the part-built patch reads between.
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    let ground = app.world.get::<Tile>(tile_entity).expect("the tile");
    let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    // **THE BASKET SLIDES AND SO DOES THE RATE** (`docs/plan_standing_upkeep.md` §2.8). One turn of
    // weeding is one turn's *fraction* of the weeding: the favored crop's share has started to climb
    // and the volunteers' to fall, so the basket is neither the mixed stand it started as nor the
    // weeded one it is heading for.
    let in_flight =
        core_sim::patch_composition(&patch, &composition, &flora, &labor.forage).into_owned();
    let weeded = core_sim::composition_for_rung(
        &patch,
        &composition,
        &flora,
        &labor.forage,
        core_sim::RungKey::PlantTended,
    );
    assert_ne!(
        in_flight.as_slice(),
        composition.as_ref(),
        "one turn of weeding has already moved the mix off the stand it started as"
    );
    assert_ne!(
        in_flight.as_slice(),
        weeded.as_ref(),
        "…and has not finished the job either"
    );
    let favored_before = composition
        .iter()
        .find(|entry| entry.species == committed)
        .map_or(0.0, |entry| entry.share);
    let favored_now = in_flight
        .iter()
        .find(|entry| entry.species == committed)
        .map_or(0.0, |entry| entry.share);
    let favored_weeded = weeded
        .iter()
        .find(|entry| entry.species == committed)
        .map_or(0.0, |entry| entry.share);
    assert!(
        favored_before < favored_now && favored_now < favored_weeded,
        "the crop's share is part-way up: {favored_before} < {favored_now} < {favored_weeded}"
    );
    // A basket that stopped summing to one would silently rescale every rate derived from it.
    let total: f32 = in_flight.iter().map(|entry| entry.share).sum();
    assert!(
        (total - core_sim::WHOLE_BASKET).abs() <= 1e-5,
        "a part-weeded basket is still a whole basket, not {total}"
    );
    // And the *rate* climbs with it, strictly between the two rungs and never either of them.
    let wild_patch = core_sim::ForagePatch::new(coord, patch.carrying_capacity);
    let wild_rate =
        core_sim::patch_provisions_per_biomass(&wild_patch, &composition, &flora, &labor.forage);
    let mut finished = wild_patch.clone();
    finished.species = patch.species.clone();
    finished.complete_cultivation(FactionId(0), &core_sim::LadderConfig::builtin());
    let tended_rate =
        core_sim::patch_provisions_per_biomass(&finished, &composition, &flora, &labor.forage);
    let rate = core_sim::patch_provisions_per_biomass(&patch, &composition, &flora, &labor.forage);
    assert!(
        tended_rate > wild_rate,
        "fixture: tending must pay more, or there is no step to be part-way up"
    );
    assert!(
        rate > wild_rate && rate < tended_rate,
        "a build one turn in converts between the two rungs: {wild_rate} < {rate} < {tended_rate}"
    );
}

/// The Cultivate policy banks its crew's **whole** output while worked (the decay pass spares a
/// patch under active preparation), completes in `work_cost / that output` turns, and the completed
/// patch then pays the full tended yield — strictly more than the wild Sustain skim it replaced.
#[test]
fn cultivate_completes_then_pays_the_tended_yield() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    let (work_per_turn, _) = cultivation_config(&app);
    let turns = turns_to_prepare(&app);

    // Progress accrues at the crew's full output — no net-of-decay drag while it is working.
    const MEASURED_TURNS: u32 = 3;
    assert!(
        turns > MEASURED_TURNS,
        "fixture: the build must still be running after {MEASURED_TURNS} turns, takes {turns}"
    );
    run_turns_with_forage(&mut app, MEASURED_TURNS);
    let built = progress_of(&app, coord);
    assert!(
        (built - MEASURED_TURNS as f32 * work_per_turn).abs() < 1e-5,
        "an actively-prepared patch banks its crew's whole output every turn: {built}"
    );

    run_turns_with_forage(&mut app, turns);
    {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).expect("patch persists");
        assert!(
            patch.is_cultivated(),
            "sustained Cultivate work completes the patch: progress {}",
            patch.ladder_position()
        );
        assert_eq!(patch.owner, Some(FactionId(0)), "the preparer owns it");
        assert_eq!(registry.cultivated_count(FactionId(0)), 1);
    }

    // **Harvest it to read the payoff.** Since issue #420 completion clears the improvement itself,
    // so this is a no-op re-assert — kept because this test measures the *payoff* and must read it
    // off an undipped harvest whatever put the band there. The clearing itself is pinned in
    // `systems::labor::labor_yield_tests`.
    set_forage_improvement(&mut app, band, None);
    let before = provisions_f32(&mut app);
    run_turns_with_forage(&mut app, 1);
    let tended_yield = provisions_f32(&mut app) - before;
    // **The wild baseline is taken at the same age**, on ground held at its floor for as many turns.
    // A one-turn baseline would be the untouched patch's opening windfall — the accumulated stock,
    // which is `B − K/2` on every rung and therefore says nothing about what tending bought.
    let sustain_yield = steady_turn_yield(None, MEASURED_TURNS + turns + 1);
    assert!(
        tended_yield > sustain_yield,
        "a tended patch out-pays the wild Sustain gather — the payoff the 25 turns bought: \
         {tended_yield} vs {sustain_yield}"
    );
    // **One telemetry row per assignment, and the band holds one more assignment than it started
    // with**: the finished Cultivate handed its builders to the band's `agriculture` keeping role
    // (`docs/plan_standing_upkeep.md` §2.5), which is a row of its own.
    let allocation = app.world.get::<LaborAllocation>(band).unwrap();
    assert_eq!(allocation.last_yields.len(), allocation.assignments.len());
    assert_eq!(
        allocation
            .assignments
            .iter()
            .filter(|a| matches!(a.target, LaborTarget::Forage { .. }))
            .count(),
        1,
        "the source itself is still one row"
    );
}

/// Both Cultivate gates, at the sim level: without the **Cultivation knowledge**, and on a
/// **non-Thriving** patch, the investment accrues nothing (the command layer rejects the assignment
/// outright; this guards the system underneath it). Progress is held, not lost, when a gate lapses.
#[test]
fn cultivate_accrues_nothing_without_knowledge_or_on_a_stressed_patch() {
    // (a) No knowledge.
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    run_turns_with_forage(&mut app, 5);
    assert_eq!(
        progress_of(&app, coord),
        0.0,
        "Cultivate without Cultivation knowledge accrues nothing"
    );

    // (b) Knowledge, but the patch is Stressed (another band overdrew it): accrual stops, and the
    // progress already banked is *held* — **because the band staffs the keeping**. Since §4.6a the
    // keeping pool owes a meter carrying work from the first work banked, so an unkept fixture would
    // be measuring the rot rather than the accrual gate under test.
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    set_maintain_workers(&mut app, band, tended_keeping_crew());
    run_turns_with_forage(&mut app, 3);
    let banked = progress_of(&app, coord);
    assert!(banked > 0.0);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.biomass = patch.carrying_capacity * 0.15;
        // The phase is derived in the Logistics regrowth pass; set it directly so the patch reads
        // Stressed for the labor arm without a regrowth turn lifting it back to Thriving.
        patch.ecology_phase = EcologyPhase::Stressed;
    }
    app.world.run_system_once(advance_cultivation);
    app.world.run_system_once(advance_labor_allocation);
    assert_eq!(
        progress_of(&app, coord),
        banked,
        "a stressed patch stops accruing — progress is held, not lost"
    );
}

/// Rung 2: a **tended** (completed) patch pays the band that tends it — **place-local, via the labor
/// arm** — and, since slice 7, **draws down like the wild stand it still is**. `advance_cultivation`
/// itself pays nothing (the retired even-split); it only decays *unworked* patches.
///
/// **Retargeted, not weakened** (slice 7): the no-drawdown assertion this test carried was the defect
/// — it pinned rung 2 as a *managed* rung, one step earlier than the animal side's, which is what made
/// a tended patch un-over-farmable and every policy pay the same number. The place-locality and
/// "advance_cultivation pays nothing" claims are untouched.
#[test]
fn tended_patch_pays_its_tending_band_place_local_and_draws_down() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);

    // The state a completed preparation leaves behind: cultivated, owned, and **kept** — the
    // completing turn's crew carries on to the keeping (`docs/plan_standing_upkeep.md` §2.2), so the
    // next Logistics decay pass reads a met demand and the patch does not bleed under the test.
    let biomass_before = {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.complete_cultivation(FactionId(0), &core_sim::LadderConfig::builtin());
        patch.owner = Some(FactionId(0));
        patch.biomass
    };
    keep_patch_for_a_turn(&mut app, coord);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    // Sustain, not Cultivate: this test reads the finished rung's *harvest*, on a patch seated
    // already-complete — the rung a band that really built it is retired onto (issue #420).
    spawn_forager(&mut app, tile, coord, None);
    assert_eq!(provisions_f32(&mut app), 0.0, "larder starts empty");

    // The decay pass pays nothing and spares the KEPT patch.
    app.world.run_system_once(advance_cultivation);
    assert_eq!(
        provisions_f32(&mut app),
        0.0,
        "advance_cultivation no longer pays a cultivated patch's owner (even-split retired)"
    );
    assert!(app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .unwrap()
        .is_cultivated());

    // The tending band's labor resolves the tended yield place-local — and gathers it out of a real
    // stock, which is what makes rung 2 over-farmable at all.
    app.world.run_system_once(advance_labor_allocation);
    let paid = provisions_f32(&mut app);
    assert!(
        paid > 0.0,
        "the tending band is paid the tended yield via its Forage assignment: {paid}"
    );
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .biomass
            < biomass_before,
        "a tended patch is still a wild stand — gathering it draws it down"
    );
}

/// Rung 1a feral loop: a cultivated patch with no band tending it goes feral through the real
/// Logistics pipeline — `advance_cultivation` decays it below the cultivated threshold (reverting to a
/// wild gather patch) and it fully reverts over ~`1/decay_per_turn` turns (owner cleared).
#[test]
fn untended_cultivated_patch_goes_feral() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.complete_cultivation(FactionId(0), &core_sim::LadderConfig::builtin());
        patch.owner = Some(FactionId(0));
    }

    // No forager band → the patch is never worked. Nothing happens for the rung's grace; the turn
    // after it, the bleed starts and the patch drops below `1.0` — it reverts to wild.
    let grace = tended_grace(&app);
    run_turns_untended(&mut app, grace);
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_cultivated(),
        "the grace turns cost the patch nothing — it is still a tended patch"
    );
    run_turns_untended(&mut app, 1);
    assert!(
        !app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_cultivated(),
        "the first turn past the grace reverts a farm to a wild gather patch"
    );

    // Keep neglecting it → progress fully decays and ownership lapses (~cost/decay turns).
    let feral_turns = turns_to_go_fully_feral(&app);
    run_turns_untended(&mut app, feral_turns);
    let patch_registry = app.world.resource::<ForageRegistry>();
    let patch = patch_registry.patch(coord).unwrap();
    assert_eq!(
        patch.ladder_position(),
        core_sim::RUNG_UNSTARTED,
        "feral patch fully reverts"
    );
    assert_eq!(patch.owner, None, "ownership lapses once fully feral");
    assert_eq!(patch_registry.cultivated_count(FactionId(0)), 0);
}

/// Abandoning a **part-prepared** patch loses the investment: with nobody working it, the partial
/// progress decays at `decay_per_turn` back toward zero (the cleared ground grows over).
#[test]
fn abandoned_preparation_decays() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    // **The band keeps what it is building** (§4.6a), so the neglect counter is at zero when the
    // band walks away and the grace below is the fresh one the arithmetic assumes. An unkept build
    // is *already* neglected, and abandoning it would bleed from the first turn.
    set_maintain_workers(&mut app, band, tended_keeping_crew());

    run_turns_with_forage(&mut app, 5);
    let banked = progress_of(&app, coord);
    assert!(
        banked > 0.0 && banked < cultivate_cost(&app),
        "part-prepared: {banked} work units of {}",
        cultivate_cost(&app)
    );

    // `upkeep_supplied` is a deliberate one-turn-lag signal (Logistics runs before Population), so
    // the first Logistics pass after the band leaves still reads the keeping it paid on its last
    // worked turn and spares the patch. Decay bites from the turn after that.
    //
    // **And the rung's `grace_turns` sit on top of that lag**: the neglect counter has to exceed the
    // grace before anything bleeds, so the bleeding turns are `turns − lag − grace`.
    app.world.despawn(band);
    const ABANDONED_TURNS: u32 = 6;
    const SPARED_LAG_TURNS: u32 = 1;
    let grace = tended_grace(&app);
    assert!(
        ABANDONED_TURNS > SPARED_LAG_TURNS + grace,
        "the fixture must run past the grace, or it would pin nothing"
    );
    run_turns_untended(&mut app, ABANDONED_TURNS);
    let (_, decay) = cultivation_config(&app);
    let decayed = progress_of(&app, coord);
    let expected_decay = decay * (ABANDONED_TURNS - SPARED_LAG_TURNS - grace) as f32;
    assert!(
        (banked - decayed - expected_decay).abs() < 1e-5,
        "an abandoned preparation decays by decay_per_turn/turn (after the one-turn flag lag): \
         {banked} -> {decayed}"
    );
}

/// **Two crews on one patch: the rung completes ONCE and clears BOTH verbs** (PR #448 review).
///
/// `handle_cultivate` sets the improvement on **every** band of the faction working the tile, so a
/// completion is always a many-bands event even though only one crew's accrual crosses `1.0`. Two
/// things had to be separated for that to read correctly, and this pins both:
///
/// - **The feed line rides the TRANSITION.** It used to fire on a post-accrual `is_cultivated()`,
///   which is true for every band once *anyone* has finished — so the player was told "Cultivated
///   patch at (x, y)" once per crew. `ForagePatch::accrue_cultivation` now answers *"did this call
///   finish it"*, `Herd::accrue_corral`'s convention.
/// - **Clearing the verb does NOT.** Whoever finished it, a rung with nothing left to build must
///   hand the verb back — otherwise the crew that lost the race keeps spending its whole work budget
///   on prepared ground and gathering none of it, which is issue #420 all over again for the second
///   band.
#[test]
fn a_completed_cultivation_announces_once_and_clears_every_bands_verb() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let first = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    let second = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    // A token second crew. `Cultivate` accrues per *assignment*, not per worker, so one hand is
    // enough to hold the verb — and two full `FORAGE_WORKERS` crews draw the patch out of Thriving,
    // which stalls the very meter this test needs to finish.
    set_forage_workers(&mut app, second, TOKEN_SECOND_CREW);

    // Long enough for the meter to fill however the two crews' accruals interleave, plus the turn a
    // band that did not finish it needs to notice (its clear is decided at the top of its own
    // iteration, so a crew processed *before* the finisher clears on the following turn).
    let turns = turns_to_prepare(&app) + 1;
    run_turns_with_forage(&mut app, turns);

    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_cultivated(),
        "fixture: the patch must be tended by now (progress {})",
        progress_of(&app, coord)
    );
    // **NOBODY IS BUILDING HERE ANY MORE, and that is now DERIVED rather than cleared.** A
    // declaration counts only where the meter it names is at zero (`forage::patch_build_verb`), so a
    // second crew's stale `Cultivate` on a finished rung answers `None` on its own — the pass that
    // used to hunt down and clear it is retired with the authority it was cleaning up after.
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
            "{label} drives nothing — there is nothing left to cultivate here"
        );
    }
    assert_eq!(
        completion_announcements(&app, "Cultivated patch at"),
        1,
        "one patch was cultivated, so the player is told once — not once per crew"
    );
}

/// How many times the feed log announced `needle`. The player-facing half of the completion seam:
/// the event log is what the notification system reads, so a duplicate there is a duplicate on
/// screen.
fn completion_announcements(app: &App, needle: &str) -> usize {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.label.contains(needle))
        .count()
}

/// A token crew for the second band on a shared source: enough to hold an assignment (and therefore
/// an improvement) without its share of the draw-down changing what the first band is measuring.
const TOKEN_SECOND_CREW: u32 = 1;

/// Re-staff a band's Forage assignment in place — the test-side twin of `assign_labor`'s worker
/// count, which since issue #442 never touches the improvement beside it.
fn set_forage_workers(app: &mut App, band: bevy::prelude::Entity, workers: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band forages")
        .assignments[0]
        .workers = workers;
}

// ---------------------------------------------------------------------------------------------
// The neglect grace, the newest-first unwind, the feral feed line, and the build crew
// ---------------------------------------------------------------------------------------------

/// Read a patch's live neglect counter — the field the grace is measured against.
fn neglect_turns_of(app: &App, coord: UVec2) -> u16 {
    app.world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .neglect_turns
}

/// Seat a completed tended patch at `coord`, owned by faction 0, with a clean neglect counter — the
/// fixture every grace test below starts from.
fn seat_tended_patch(app: &mut App, coord: UVec2) {
    // **At the LADDER's own cost, not the fabricated one.** The feral bleed is an absolute number of
    // work units per turn (a fraction of the rung's cost), so a patch seated at a nominal one-unit
    // job would lapse to nothing in a single bleeding turn — this fixture is about the *pace* of the
    // bleed, so its job has to be the real one.
    let cost = cultivate_cost(app);
    // **AND THE RETENTION BAR the completion would have stamped** — the rung is *earned* at its cost
    // and *held* down to this bar, so a fixture that fills the meter by hand and leaves the bar at
    // `RUNG_UNSTARTED` seats a patch that is not tended at all
    // (`docs/plan_standing_upkeep.md` §2.4).
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch");
    patch.set_ladder_position(cost, &core_sim::LadderConfig::builtin());
    patch.owner = Some(FactionId(0));
    patch.neglect_turns = 0;
}

/// **The grace is CONSECUTIVE neglect, not a lifetime budget** — one worked turn wipes the counter,
/// so a crew that comes and goes never accumulates its way into the bleed.
///
/// The counter is asserted directly rather than through the meter, because the meter cannot
/// distinguish "the grace reset" from "the grace has not run out yet".
#[test]
fn working_a_patch_resets_its_neglect_counter() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    seat_tended_patch(&mut app, coord);
    let grace = tended_grace(&app);
    assert!(grace > 0, "this test needs a rung that forgives something");

    run_turns_untended(&mut app, grace);
    assert_eq!(
        u32::from(neglect_turns_of(&app, coord)),
        grace,
        "the counter climbs one per un-worked turn"
    );

    // One KEPT turn — the supply the labor arm would stamp — and the counter is back to nothing.
    keep_patch_for_a_turn(&mut app, coord);
    run_turns_untended(&mut app, 1);
    assert_eq!(
        neglect_turns_of(&app, coord),
        0,
        "a turn whose demand was met forgives the neglect outright"
    );

    // ...and the full grace is available again from scratch: still tended after another `grace` turns.
    run_turns_untended(&mut app, grace);
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "the grace is spent from zero again, not from where it left off"
    );
}

/// **The turn the bleed bites, and the turn before it** — the boundary, pinned from both sides on the
/// meter itself. A grace test that only ever looks at one side of the boundary proves nothing.
#[test]
fn the_feral_bleed_starts_exactly_one_turn_past_the_grace() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    seat_tended_patch(&mut app, coord);
    let seated_cost = cultivate_cost(&app);
    let grace = tended_grace(&app);

    run_turns_untended(&mut app, grace);
    assert_eq!(
        progress_of(&app, coord),
        seated_cost,
        "the last forgiven turn leaves the meter untouched"
    );

    run_turns_untended(&mut app, 1);
    let (_, decay) = cultivation_config(&app);
    assert!(
        (progress_of(&app, coord) - (seated_cost - decay)).abs() < 1e-6,
        "the first turn past the grace bleeds exactly one turn's decay: {}",
        progress_of(&app, coord)
    );
}

/// **GATHERING A PATCH NO LONGER HOLDS IT — the behavioural headline of the upkeep arc**
/// (`docs/plan_standing_upkeep.md` §2.4).
///
/// The retired `tended_this_turn` flag was set by *any* crew on the tile, so a tended patch somebody
/// was **harvesting** never decayed: holding an improvement was free for exactly as long as you were
/// taking from it. Holding and taking are separate allocations now, so a band that gathers and
/// staffs no keeper watches the ground it improved revert underneath it.
///
/// **This is the single most consequential behaviour change in the arc**, and it is asserted as a
/// contrast rather than in isolation — the same patch, the same gatherers, the same turns, differing
/// only in whether one hand was put on the keeping.
#[test]
fn gathering_a_patch_does_not_hold_it_but_one_keeper_does() {
    /// Long enough to clear the tended rung's grace and bleed for several turns after it.
    const TURNS: u32 = 12;

    let progress_after = |keepers: u32| -> f32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        seat_tended_patch(&mut app, coord);
        // A gathering crew and nothing else — no verb, so no build crew, exactly the state a band
        // that finished a Cultivate and went back to harvesting is in.
        let band = spawn_forager(&mut app, tile, coord, None);
        set_maintain_workers(&mut app, band, keepers);
        run_turns_with_forage(&mut app, TURNS);
        progress_of(&app, coord)
    };

    let seated = {
        let mut app = spawn_world();
        let (_tile, coord) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, coord);
        progress_of(&app, coord)
    };

    let gathered_only = progress_after(NO_CREW_ON_THIS_ACTIVITY);
    assert!(
        gathered_only < seated,
        "a patch being gathered but not kept must revert — it did not ({gathered_only} of {seated})"
    );
    let (_, bleed) = cultivation_config(&app_free());
    // **The grace, and nothing else.** `advance_cultivation` runs before the labor arm inside a
    // turn, so the very first pass already reads an unmet demand — there is no lag to subtract on a
    // patch nobody ever kept. Every turn past the grace bleeds the rung's whole rot rate.
    let bleeding_turns = TURNS - tended_grace(&app_free());
    assert!(
        (seated - gathered_only - bleed * bleeding_turns as f32).abs() < 1e-4,
        "…and it reverts at exactly the rung's own rate: {seated} -> {gathered_only} over \
         {bleeding_turns} bleeding turns at {bleed}/turn"
    );

    // **THE KEEPING IS A POOL SIZED AGAINST THE DEMAND**, so what holds this patch is the demand in
    // whole hands — two, since the retune. One hand is *half* the keeping and therefore half the
    // rot, which is the arc's continuity working rather than a threshold.
    let demand_in_hands = app_free()
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(fixture_tender_loads());
    let kept = progress_after(demand_in_hands);
    assert_eq!(
        kept, seated,
        "a keeping pool that covers the demand holds the patch outright"
    );
    // **Half the hands is half the rot on the turn it applies**, which
    // `a_half_staffed_keeping_bleeds_at_half_the_rungs_rate` measures on its own — over a longer
    // window a half-kept patch dips, becomes *building*, and stops drawing from the pool at all, so
    // what is asserted here is only that it lands strictly between the two ends.
    let half_kept = progress_after(demand_in_hands / 2);
    assert!(
        half_kept > gathered_only && half_kept < seated,
        "half a keeping pool holds a patch longer than none and less than a full one: \
         {gathered_only} < {half_kept} < {seated}"
    );
}

/// **AND A PATCH NOBODY IS GATHERING CAN STILL BE KEPT — the other half of the same separation**
/// (`docs/plan_standing_upkeep.md` §2.2/§2.5).
///
/// The headline above says gathering does not hold a patch. Its mirror is that **holding does not
/// require gathering**: a band that finishes a Cultivate and moves its foragers to a richer stand
/// still *holds* that ground, so it still owes the rate and its `agriculture` pool must still be
/// able to pay it.
///
/// It could not. The take crew was the row's licence to exist — `set_assignment` dropped the row at
/// zero workers, `maintenance_shares` skipped what was left, and the labor loop skipped it again —
/// so the patch contributed no demand to the pool, drew no share, and bled its **full** rate with
/// keepers standing idle in the role and **no command the player could issue to aim them at it**.
/// The wire published `upkeepShortfall = demand` faithfully, so the client's under-kept warning
/// fired on a state with no remedy.
///
/// Asserted as a contrast with the same band, the same patch and the same turns, differing only in
/// whether anybody is gathering — because *"it did not bleed"* also passes for a patch that cannot
/// bleed at all.
#[test]
fn a_patch_with_no_gatherers_is_still_kept_by_the_bands_pool() {
    /// Well past the tended rung's grace, so an unfunded patch is visibly bleeding by the end.
    const TURNS: u32 = 12;

    let progress_after = |keepers: u32, gatherers_leave: bool| -> f32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        seat_tended_patch(&mut app, coord);
        let band = spawn_forager(&mut app, tile, coord, None);
        if gatherers_leave {
            unstaff_the_gatherers(&mut app, band, coord);
        }
        set_maintain_workers(&mut app, band, keepers);
        run_turns_with_forage(&mut app, TURNS);
        assert!(
            app.world
                .get::<LaborAllocation>(band)
                .expect("band exists")
                .assignments
                .iter()
                .any(|assignment| matches!(
                    assignment.target,
                    LaborTarget::Forage { tile, .. } if tile == coord
                )),
            "the band's holding of the patch must survive the turn that has no gatherers on it"
        );
        progress_of(&app, coord)
    };

    let seated = {
        let mut app = spawn_world();
        let (_tile, coord) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, coord);
        progress_of(&app, coord)
    };
    let demand_in_hands = app_free()
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(fixture_tender_loads());

    /// The gatherers move to a richer stand — the state the whole defect lives in.
    const THE_GATHERERS_LEAVE: bool = true;
    /// The same band, still harvesting, as the control the numbers are read against.
    const THE_GATHERERS_STAY: bool = false;

    let kept = progress_after(demand_in_hands, THE_GATHERERS_LEAVE);
    assert_eq!(
        kept, seated,
        "a pool that covers the demand holds a patch nobody is gathering, exactly as it holds one \
         somebody is"
    );
    // **Liveness**: the same unstaffed patch with an empty pool must still rot, or the equality
    // above would be reporting a patch that cannot bleed rather than one that is being kept.
    let unkept = progress_after(NO_CREW_ON_THIS_ACTIVITY, THE_GATHERERS_LEAVE);
    assert!(
        unkept < seated,
        "a patch nobody keeps must still revert, gatherers or no gatherers ({unkept} of {seated})"
    );
    // And the keeping is worth exactly the same to it either way: the pool is sized against what the
    // band *holds*, so whether a crew is harvesting beside it cannot move the bill.
    assert_eq!(
        progress_after(demand_in_hands, THE_GATHERERS_STAY),
        kept,
        "the keeping costs the same whether or not the patch is being gathered"
    );
    assert_eq!(
        progress_after(NO_CREW_ON_THIS_ACTIVITY, THE_GATHERERS_STAY),
        unkept,
        "…and so does going without it"
    );
}

/// **⛔ THE UPKEEP PAIR: GEAR COVERS MORE, AND THE DEMAND NEVER MOVES.**
///
/// §4.8's other half — *"upkeep is just work/turn and worker productivity is work/turn"* — routes
/// the **same** supply expression a build divides its pile by through the keeping pool: a build
/// divides, an upkeep compares. So an equipped keeper covers more of a patch's demand than a bare
/// one, and the rung's `upkeep.work_per_turn` is untouched by either — which is the build rule's
/// mirror, *a job's work requirement never changes*, stated about a rate instead of a pile.
///
/// **Both halves, because either alone passes a broken model**: *"the demand is identical"* passes
/// for a kit that does nothing at all, and *"the equipped pool covers more"* passes for a model that
/// quietly discounted the demand instead of raising the keeper.
///
/// # ⛔ AND THE NO-OP GUARD, WHICH IS THE FAILURE MODE HERE
///
/// `default_kits.agriculture` is `none`, so a site that waited to be handed a kit would resolve bare
/// and this whole seam would change nothing while every assertion above still passed. The bare arm
/// therefore names `none` **explicitly** and the equipped arm names nothing at all — so what is
/// compared is the *derivation* against a stated refusal, and a derivation that answered `none`
/// collapses the pair to two equal numbers.
///
/// **THE SELECTION IS ON THE PATCH'S OWN ROW** (`docs/plan_standing_upkeep.md` §2.7) — the site
/// decides what its keepers carry, and the `agriculture` role decides only how many of them there
/// are. Naming it on the role row instead sets nothing any more, which would take the bare arm back
/// to the derivation and collapse the pair.
#[test]
fn an_equipped_keeper_covers_more_demand_and_the_demand_is_the_same_either_way() {
    /// A staffing under the shipped `plant:tended` demand at every kit, so both arms are genuinely
    /// short and the comparison is between two live shortfalls rather than two saturated pools.
    const A_KEEPER: u32 = 1;

    /// What one arm's turn left on the patch: what its keepers put on the ground, and what the rung
    /// billed them for.
    struct Kept {
        supplied: f32,
        demand: f32,
    }

    let kept_with = |kit_id: Option<&str>| -> Kept {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        seat_tended_patch(&mut app, coord);
        let band = spawn_forager(&mut app, tile, coord, None);
        set_maintain_workers(&mut app, band, A_KEEPER);
        // **A NAMED kit wins, `none` included** — that is how a player works one site bare, and it
        // is the only way to state the bare arm without asserting the derivation away. It goes on
        // the **patch's** row: the keeping kit is per work site.
        if let Some(id) = kit_id {
            let kit = core_sim::EquipmentConfig::builtin()
                .kit(id)
                .unwrap_or_else(|| panic!("the shipped roster carries '{id}'"));
            app.world
                .get_mut::<LaborAllocation>(band)
                .expect("band exists")
                .assignments
                .iter_mut()
                .find(|assignment| {
                    matches!(assignment.target, LaborTarget::Forage { tile, .. } if tile == coord)
                })
                .expect("the fixture band carries a row on the patch")
                .upkeep_kit = Some(kit);
        }
        app.world.run_system_once(advance_labor_allocation);
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).expect("patch");
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        Kept {
            supplied: patch.upkeep_supplied,
            // **THE BILL THE KEEPERS WERE HANDED**, not the live cost — the demand
            // interpolates on the position, and the turn's own accrual has already raised it by the
            // time this reads (`ForagePatch::upkeep_demanded`).
            demand: core_sim::patch_keeping_basis(
                patch,
                &ladder,
                plant_tile_capacity(&app, coord),
                &app.world.resource::<LaborConfigHandle>().get().forage,
            ),
        }
    };

    let bare = kept_with(Some("none"));
    let derived = kept_with(None);

    // **(a) THE DEMAND IS THE SAME BILL.** Byte-identical, not merely close: nothing about a kit
    // reaches `RungUpkeep::work_per_turn`.
    assert_eq!(
        bare.demand, derived.demand,
        "a rung's standing cost is the same with the tool and without: {} against {}",
        bare.demand, derived.demand
    );
    assert!(
        bare.demand > 0.0,
        "fixture: the rung must actually bill something, or both arms are zero"
    );

    // **(b) AND THE EQUIPPED KEEPER COVERS MORE OF IT.** Strictly — a derivation that resolved
    // `none` would make these two equal, which is the silent no-op this arm exists to catch.
    assert!(
        derived.supplied > bare.supplied,
        "the derived agriculture kit must raise what a keeper supplies — {} against a bare {}. \
         Equal numbers mean the derivation resolved `none` and the change did nothing",
        derived.supplied,
        bare.supplied
    );
    assert!(
        (bare.supplied - core_sim::PER_WORKER_OUTPUT * A_KEEPER as f32).abs() < 1e-5,
        "…and a pool sent out bare supplies exactly its hands: {}",
        bare.supplied
    );
    // **Both arms are still SHORT**, so what the pair measures is coverage rather than one arm
    // saturating and hiding the difference.
    assert!(
        derived.supplied < derived.demand,
        "fixture: even the equipped keeper must be short of the demand at {A_KEEPER} hand, or the \
         comparison is between a covered pool and a covered pool"
    );
}

/// ⛔ **THE BANDS ON ONE PATCH ARE JUDGED AGAINST ONE BILL, WHICHEVER ORDER THEY ARE VISITED IN.**
///
/// `upkeep_demanded` exists because the plant demand **interpolates on the position**, so a supply
/// stamped in Population and judged by the next Logistics pass would always be measured against a
/// bill that had since risen. It was *assigned* per band, on the reasoning that the demand is
/// per-source and so every band writes the same number — and that reasoning is false the moment more
/// than one band holds a patch: **the build accrual runs inside a band's own arm**, so the position
/// moves between visits, and a band reached *after* the builders have banked their turn writes a
/// bigger bill than the keepers were billed at.
///
/// The fixture is the three roles that make that visible, spawned both ways round: a band **keeping**
/// the patch, a band **building** it, and a band merely **holding** it. Visited keeper-first the
/// keeper pays the bill at the position it read, the builders then bank their turn, and the third
/// band — which supplies nothing — overwrites the bill with the risen one. The keeping is short by
/// the difference **every turn**, on a correctly-staffed band: `neglect_turns` re-arms and the
/// published grace never resets. Visited the other way round the same three bands are fine, which is
/// what makes this a **visit-order** defect rather than an arithmetic one.
///
/// **Nobody gathers, and the patch starts part-built.** A take would draw the stand down to the
/// escapement floor and stall the Cultivate, and a stalled build leaves the position still — which
/// is the one state in which every band's stamp trivially agrees. The rows survive on the meter at
/// risk (`source_has_a_meter_at_risk`), which is what a band *holding* ground means.
#[test]
fn every_band_on_one_patch_is_judged_against_one_bill_in_either_visit_order() {
    /// Enough turns that the second Logistics pass has judged the first turn's stamps, with a
    /// margin — the shortfall this catches is per-turn and permanent, not a one-turn edge.
    const TURNS: u32 = 4;

    /// **The turn that pays for seating the meter by hand** — no band has kept the patch yet when
    /// the first decay pass judges it, so its shortfall is the fixture's and not the sim's.
    const THE_UNKEPT_OPENING_TURN: u32 = 1;

    /// **Part-built when the fixture opens**, as a fraction of the tended rung's cost — so the
    /// keeping is owed from the very first visit and every band's row survives with no take crew.
    const PART_BUILT: f32 = 0.2;

    /// What the patch is holding at the end of one turn.
    #[derive(Debug)]
    struct Held {
        supplied: f32,
        basis: f32,
        neglect: u16,
        progress: f32,
    }

    let run = |keeper_first: bool| -> Vec<Held> {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        {
            let ladder = app.world.resource::<LadderConfigHandle>().get().clone();
            let cost = cultivate_cost(&app);
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).expect("patch");
            patch.set_ladder_position(cost * PART_BUILT, &ladder);
            // The work landed with an owner, exactly as it would have in play — an unowned meter is
            // ground the build gate refuses.
            patch.owner = Some(FactionId(0));
        }
        // **The builders keep NOTHING** — the whole keeping comes from the band below, which is what
        // makes the stamps distinguishable. `spawn_builder` staffs the role by default.
        let spawn_the_builders = |app: &mut App| {
            let band = spawn_builder(app, tile, coord, Improvement::Cultivate);
            set_maintain_workers(app, band, NOBODY_KEEPING);
            stand_the_take_down(app, band);
        };
        // **The band that holds the ground** — one keeper, which covers either plant rung's whole
        // demand, so a shortfall here is the bill moving rather than the pool being thin.
        let spawn_the_keepers = |app: &mut App| {
            let band = spawn_forager_of(app, tile, coord, None, SOLE_FORAGER);
            set_maintain_workers(app, band, tended_keeping_crew());
            stand_the_take_down(app, band);
        };
        // **A band that only holds it.** It answers for the source — it carries a row on it — but
        // pays nothing toward keeping it, so any bill *it* writes is a bill nobody was handed.
        let spawn_the_holders = |app: &mut App| {
            let band = spawn_forager_of(app, tile, coord, None, SOLE_FORAGER);
            set_maintain_workers(app, band, NOBODY_KEEPING);
            stand_the_take_down(app, band);
        };
        if keeper_first {
            spawn_the_keepers(&mut app);
            spawn_the_builders(&mut app);
            spawn_the_holders(&mut app);
        } else {
            spawn_the_holders(&mut app);
            spawn_the_builders(&mut app);
            spawn_the_keepers(&mut app);
        }

        // **One opening turn, unmeasured.** The fixture seats the meter directly, so the very first
        // Logistics pass judges a patch no band has yet had a chance to keep — an artifact of
        // seating, not a state the game reaches.
        run_turns_with_forage(&mut app, THE_UNKEPT_OPENING_TURN);
        (0..TURNS)
            .map(|_| {
                run_turns_with_forage(&mut app, 1);
                let ladder = app.world.resource::<LadderConfigHandle>().get();
                let registry = app.world.resource::<ForageRegistry>();
                let patch = registry.patch(coord).expect("patch");
                Held {
                    supplied: patch.upkeep_supplied,
                    basis: core_sim::patch_keeping_basis(
                        patch,
                        &ladder,
                        plant_tile_capacity(&app, coord),
                        &app.world.resource::<LaborConfigHandle>().get().forage,
                    ),
                    neglect: patch.neglect_turns,
                    progress: patch.ladder_position(),
                }
            })
            .collect()
    };

    for (order, turns) in [("keepers first", run(true)), ("holders first", run(false))] {
        // **Liveness** — the meter must climb on every turn, or the position stands still and every
        // band's stamp agrees for a reason that has nothing to do with the fix.
        for pair in turns.windows(2) {
            assert!(
                pair[1].progress > pair[0].progress,
                "fixture ({order}): the Cultivate must bank work every turn, got {pair:?}"
            );
        }
        for (turn, held) in turns.iter().enumerate() {
            assert!(
                held.basis > 0.0,
                "fixture ({order}, turn {turn}): the meter must cost something to hold"
            );
            assert!(
                held.supplied >= held.basis,
                "({order}, turn {turn}) a staffed keeping must cover the bill it was handed — \
                 supplied {} against a basis of {}",
                held.supplied,
                held.basis
            );
            assert_eq!(
                held.neglect, 0,
                "({order}, turn {turn}) a correctly-staffed keeping is never neglect, so the \
                 published grace resets"
            );
        }
    }
}

/// **A band that pays nothing toward keeping** — its `agriculture` row stated at zero, so the whole
/// of the patch's keeping comes from the one band that staffs it.
const NOBODY_KEEPING: u32 = 0;

/// **Take the gatherers off the row without taking the row away.** Set through the assignment
/// directly rather than `set_assignment`, which *drops* a row it is handed zero workers for — and
/// dropping the row would prune the build queue entry standing on it.
fn stand_the_take_down(app: &mut App, band: bevy::prelude::Entity) {
    for assignment in &mut app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("band exists")
        .assignments
    {
        if matches!(assignment.target, LaborTarget::Forage { .. }) {
            assignment.workers = 0;
        }
    }
}

/// **ON THE TURN A CULTIVATE BANKS ITS FIRST WORK, ITS KEEPING IS ALREADY BEING PAID.**
///
/// Reported from play: a band 6% into a Cultivate with `agriculture` staffed at 1, and the pool card
/// reading *"Short 2 of the 2 work a turn this band's tended ground needs"* — the whole demand
/// unmet, on a staffed role, with nothing the player could do about it.
///
/// # It was a WITHIN-TURN ORDERING FAULT between two spellings of one question
///
/// `systems::labor::maintenance_shares` splits the band's keeping pool **before** the assignment
/// loop accrues any build, and it gated a source's claim on a **progress-only** test. On the turn a
/// build banks its first work the ground still carries nothing at claim time, so the patch was
/// skipped, its share came back `0`, and the stamp paid that zero through `patch_upkeep_supply` —
/// whose own resolver is **progress-or-verb** and therefore knew perfectly well the pool owed for
/// that meter. Capture then read the patch *after* the accrual: demand `2.0`, supplied `0.0`,
/// shortfall the lot.
///
/// Both seams read `forage::patch_keeping_meter` now, so there is no second definition left to fall
/// a turn behind.
///
/// # THE PAIR IS THE TEST — either half alone is satisfied by a broken gate
///
/// 1. a build's first turn supplies **the same work the same keeper puts on a finished rung**, and
///    is short only the honest remainder;
/// 2. **bare ground with no build declared claims nothing** — the gate must not have become
///    *"always"*, which is what a claim on ground carrying no work and starting none would be.
///
/// Restoring the progress-only test fails (1) naming `0` supplied where it wanted the keeper's whole
/// contribution, and leaves (2) passing.
#[test]
fn a_builds_first_turn_draws_the_keeping_pool_and_bare_ground_draws_nothing() {
    /// Under the shipped `plant:tended` demand at any kit, so the arm is genuinely short and the
    /// assertion is about a live remainder rather than a saturated pool.
    const ONE_KEEPER: u32 = 1;

    /// What one turn left on the patch.
    struct Kept {
        supplied: f32,
        demand: f32,
        progress: f32,
    }

    let read = |app: &App, coord: UVec2| -> Kept {
        let registry = app.world.resource::<ForageRegistry>();
        let patch = registry.patch(coord).expect("patch");
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        Kept {
            supplied: patch.upkeep_supplied,
            // Read the way the capture reads it — **after** the accrual, with no verb in hand — so
            // this is the number the pool card was quoting.
            // **THE BILL THE KEEPERS WERE HANDED**, not the live cost — the demand
            // interpolates on the position, and the turn's own accrual has already raised it by the
            // time this reads (`ForagePatch::upkeep_demanded`).
            demand: core_sim::patch_keeping_basis(
                patch,
                &ladder,
                plant_tile_capacity(app, coord),
                &app.world.resource::<LaborConfigHandle>().get().forage,
            ),
            progress: patch.ladder_position(),
        }
    };

    // (a) **A CULTIVATE ON ITS VERY FIRST TURN.** Nothing is banked when the shares are split; the
    // build's own accrual lands later in the same system.
    let building = {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
        set_maintain_workers(&mut app, band, ONE_KEEPER);
        assert_eq!(
            progress_of(&app, coord),
            0.0,
            "fixture: the meter must be untouched before the turn, or the progress term carries \
             the claim and the verb term is never exercised"
        );
        app.world.run_system_once(advance_labor_allocation);
        read(&app, coord)
    };

    // (b) **THE SAME KEEPER ON A FINISHED RUNG** — the reference the build arm is measured against,
    // so neither number is a literal that a retune could strand.
    let holding = {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        seat_tended_patch(&mut app, coord);
        let band = spawn_forager(&mut app, tile, coord, None);
        set_maintain_workers(&mut app, band, ONE_KEEPER);
        app.world.run_system_once(advance_labor_allocation);
        read(&app, coord)
    };

    // (c) **BARE GROUND, NOTHING DECLARED** — the other half of the pair.
    let wild = {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        let band = spawn_forager(&mut app, tile, coord, None);
        set_maintain_workers(&mut app, band, ONE_KEEPER);
        app.world.run_system_once(advance_labor_allocation);
        read(&app, coord)
    };

    // **Liveness first**: the build really did bank work this turn, so the claim was made on a meter
    // that was empty when the pool was split. Without this the arm passes for a build that never ran.
    assert!(
        building.progress > 0.0,
        "fixture: the Cultivate must bank work on this very turn, or nothing about the ordering is \
         under test"
    );

    // **(1) THE HEADLINE.** Non-zero, and exactly what the same keeper supplies to a finished rung —
    // §4.6a's *"a meter carrying work is billed at any fullness, to the same hands"* from the paying
    // side.
    // **THE SUPPLY MATCHES THE BILL.** The bill INTERPOLATES now, so on the turn a Cultivate banks
    // its first work the ground stood at zero when the pool was split and was billed nothing — the
    // claim is what stops the row publishing a shortfall on a staffed role, and what it claims is
    // the demand at that moment. `supplied == demand` is the invariant; `supplied > 0` was a
    // statement about the retired flat rate.
    assert!(
        building.supplied >= building.demand,
        "the turn a Cultivate banks its first work, its keeping pool must cover the bill it was \
         handed — got {} against a demand of {}",
        building.supplied,
        building.demand
    );
    // **THE SAME POOL, AT THE RATE EACH POSITION OWES** (`docs/plan_standing_upkeep.md` §2.8). The
    // supplier is the same on both sides of completion — which is §4.6a and is what this test is
    // for — but the *bill* interpolates, so a meter one turn into its rung is billed a fraction of
    // what a finished one is. It used to be the identical number, and that identity is exactly the
    // defect §2.8 names: a patch 1% into a Cultivate owed the whole rung's rate.
    assert!(
        building.demand < holding.demand,
        "a meter being raised is billed LESS than the finished rung above it: {} against {}",
        building.demand,
        holding.demand
    );
    assert!(
        holding.supplied > 0.0,
        "a finished rung is held by the pool, or the comparison below is vacuous"
    );

    // **The shortfall is the honest remainder, never more than the bill.** With the demand
    // interpolated and this pool ample for it, a turn-one build is covered outright — the number on
    // the pool card is `0`, not the whole rung's rate.
    let shortfall = (building.demand - building.supplied).max(0.0);
    assert!(
        shortfall < building.demand.max(f32::EPSILON),
        "the shortfall is what the pool could not cover, never the whole bill: short {shortfall} \
         of {} (supplied {})",
        building.demand,
        building.supplied
    );

    // **(2) THE PAIR.** Nothing on the meter and no build declared: nothing is owed, so nothing can
    // be claimed or supplied. A gate that answered *"always"* fails here and passes everything above.
    assert_eq!(
        wild.demand, 0.0,
        "a wild patch owes nothing, so there is nothing for a pool to claim against ({})",
        wild.demand
    );
    assert_eq!(
        wild.supplied, 0.0,
        "…and a keeping pool must therefore put nothing on it ({})",
        wild.supplied
    );
}

/// **AND A BUILD WHOSE KEEPING IS COVERED FROM TURN ONE NEVER ACCRUES NEGLECT AT ALL.**
///
/// The decay pass reads **last** turn's `upkeep_supplied` and clears it (Logistics runs before
/// Population), so there is a documented one-turn lag between a keeper working and that work being
/// judged. Making the claim side answer on turn one interacts with that lag, and the correct outcome
/// is the *absence* of an effect: a patch whose pool covers its demand every turn never counts a
/// single neglected turn, from the first work banked onward.
///
/// The liveness half is the same build with the role empty, which must count them — otherwise
/// *"never neglected"* is being asserted of a fixture that cannot accrue neglect in the first place.
#[test]
fn a_kept_builds_first_turns_accrue_no_neglect_and_an_unkept_ones_do() {
    /// Well past the tended rung's grace, so the unkept arm is visibly counting by the end.
    const TURNS: u32 = 6;

    let neglect_after = |keepers: u32| -> u16 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
        set_maintain_workers(&mut app, band, keepers);
        for _ in 0..TURNS {
            run_turns_with_forage(&mut app, 1);
            if keepers >= tended_keeping_crew() {
                assert_eq!(
                    neglect_turns_of(&app, coord),
                    0,
                    "a covered build must never count a neglected turn — not even the first, \
                     where the decay pass's one-turn lag reads a supply that was stamped before \
                     any work was banked"
                );
            }
        }
        assert!(
            progress_of(&app, coord) > 0.0,
            "fixture: the build must actually be running, or neither arm means anything"
        );
        neglect_turns_of(&app, coord)
    };

    assert_eq!(
        neglect_after(tended_keeping_crew()),
        0,
        "a build whose keeping covers its demand carries no neglect at all"
    );
    assert!(
        neglect_after(NO_CREW_ON_THIS_ACTIVITY) > 0,
        "…while the same build with an empty role counts its unkept turns, or the arm above is \
         reporting a patch that cannot accrue neglect"
    );
}

/// **MEETING THE DEMAND EXACTLY COSTS THE METER NOTHING, and going short costs it the rung's own
/// rate scaled by how short** — the property the retired binary flag could not express, and the
/// reason the standing cost is a *rate*. Under the flag a crew of one on a source wanting two
/// counted as fully worked, so under-crewing cost precisely nothing until it reached zero.
///
/// **Half-staffing is reachable on the SHIPPED ladder now.** Both plant demands used to sit under a
/// single worker-turn, so a fixture ladder was the only way to observe a half; the retune made them
/// whole numbers a player can staff exactly, which is most of what it was for.
///
/// **Measured over exactly ONE bleeding turn**, so the arithmetic reads directly as the rung's own
/// rate rather than as a multiple of it. The follow-on assertion is the one §4.6a changed: a meter
/// that has dipped below its cost is **still the pool's**, where it used to flip back to its
/// builders at the very moment the keeping started mattering.
///
/// # ⛔ THE PROPORTION IS IN THE SUPPLY, NOT IN THE HEAD COUNT
///
/// This read *"half the hands must be half the bleed"* while a keeper was worth a flat
/// `PER_WORKER_OUTPUT`. A keeper's supply reads the pool's kit now
/// (`docs/plan_standing_upkeep.md` §4.8), so half the **hands** is no longer half the **supply** —
/// an equipped keeper covers more than a bare one, and one keeper against a demand of `2.0` is
/// three-quarters covered rather than half.
///
/// The rule the sim actually states is unchanged and is what is asserted: the meter loses
/// `(shortfall / demand) × the rung's own rate`. So each staffing is measured against the supply it
/// genuinely puts on the ground, and the demand is asserted **byte-identical across every one of
/// them** — the upkeep half of *a job's work requirement never changes*.
#[test]
fn a_half_staffed_keeping_bleeds_in_proportion_to_the_supply_it_is_short() {
    /// One past the shipped grace: the first turn the bleed actually bites.
    const TURNS: u32 = 3;

    let lost_with = |keepers: u32| -> f32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        seat_tended_patch(&mut app, coord);
        let seated = progress_of(&app, coord);
        let band = spawn_forager(&mut app, tile, coord, None);
        set_maintain_workers(&mut app, band, keepers);
        run_turns_with_forage(&mut app, TURNS);
        seated - progress_of(&app, coord)
    };

    // **THE DEMAND, WHICH NO STAFFING AND NO KIT MOVES** — the upkeep mirror of the build rule.
    let tended = app_free()
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_demand(fixture_tender_loads());
    // The hands that cover the shipped demand outright at this pool's own kit, and one below it —
    // so the pair is *fully kept* against *genuinely short* rather than two arbitrary counts.
    let covering = (1..)
        .find(|keepers| plant_keeper_supply(*keepers) >= tended)
        .expect("some pool covers a 2.0 demand");
    assert!(
        covering >= 2,
        "fixture: covering the demand must take more than one hand, or there is no short arm"
    );
    let short = covering - 1;

    let (_, bleed) = cultivation_config(&app_free());
    let bleeding_turns = TURNS - tended_grace(&app_free());

    let unkept = lost_with(NO_CREW_ON_THIS_ACTIVITY);
    let part_kept = lost_with(short);
    let fully_kept = lost_with(covering);

    assert_eq!(
        bleeding_turns, 1,
        "fixture: exactly one bleeding turn — see the doc above"
    );
    assert!(
        (unkept - bleed * bleeding_turns as f32).abs() < 1e-4,
        "an unkept patch bleeds the rung's own rate every bleeding turn, got {unkept}"
    );
    // **THE PROPORTION**: what the meter loses is the rung's rate times the share of the demand
    // nobody supplied, and the supply is `keepers × (bare + kit)`.
    let short_fall = (tended - plant_keeper_supply(short)).max(0.0);
    assert!(
        short_fall > 0.0,
        "fixture: the short arm must genuinely be short, or this measures two zeroes"
    );
    assert!(
        (part_kept - bleed * (short_fall / tended) * bleeding_turns as f32).abs() < 1e-4,
        "a part-staffed keeping bleeds the rung's rate scaled by how short it is: {part_kept} \
         against a shortfall of {short_fall} on a demand of {tended}"
    );
    assert_eq!(
        fully_kept, 0.0,
        "and covering the demand costs the meter nothing"
    );
    // **AND THE DEMAND ITSELF NEVER MOVED** — the upkeep half of *a job's work requirement never
    // changes* (§4.8). Every arm above was billed the identical rate; what differed is only what
    // its keepers supplied against it.
    for keepers in [NO_CREW_ON_THIS_ACTIVITY, short, covering] {
        assert_eq!(
            app_free()
                .world
                .resource::<LadderConfigHandle>()
                .get()
                .rung(RungKey::PlantTended)
                .upkeep_demand(fixture_tender_loads()),
            tended,
            "a rung's demand is the same at {keepers} keepers as at any other staffing"
        );
    }

    // **AND A RUNG THAT HAS DIPPED BELOW ITS COST IS STILL THE POOL'S** — the state that used to
    // switch over to its builders (`docs/plan_standing_upkeep.md` §4.6a). It flipped into *building*
    // at 99%, so a full keeping pool stopped reaching it at exactly the moment it began needing one,
    // and topping it back up made it the pool's again — an oscillation with the player's real build
    // standing still through every cycle.
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    seat_tended_patch(&mut app, coord);
    let band = spawn_forager(&mut app, tile, coord, None);
    set_maintain_workers(&mut app, band, covering);
    // Nudge the meter under its cost, exactly as one short turn would have.
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch");
        patch.decay_ladder(bleed, &core_sim::LadderConfig::builtin());
    }
    let dipped = progress_of(&app, coord);
    assert!(
        dipped < cultivate_cost(&app),
        "fixture: the meter really is below its cost, or there is nothing under test"
    );
    run_turns_with_forage(&mut app, TURNS);
    assert!(
        (progress_of(&app, coord) - dipped).abs() < 1e-4,
        "a full keeping pool holds a DIPPED rung exactly where it is — {dipped} to {}",
        progress_of(&app, coord)
    );
    assert!(
        !app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "…and it is honestly no longer TENDED (§2.8, the bar is deleted); what the pool bought is \
         that the meter stopped falling, which is the claim under test"
    );
}

/// **`work_cost / crew` IS THE PACE AGAIN — and what can eat it is the ROT, not the rate**
/// (`docs/plan_standing_upkeep.md` §4.6a). A build crew supplies nothing toward the maintenance
/// rate; the band's keeping pool owes it for the meter being raised exactly as it does for a
/// finished one. So a **kept** Cultivate finishes in exactly its stated turns, and an **unkept** one
/// takes longer — not because its builders are paying a bill, but because the ground is going
/// backwards under them past the rung's grace.
///
/// Asserted against the ladder's own arithmetic rather than a literal, so a retune of `work_cost`
/// moves the expectation with the game.
#[test]
fn a_kept_cultivate_finishes_in_its_stated_turns_and_an_unkept_one_is_slower() {
    let stated = turns_to_prepare(&app_free());
    let finished_on = |keepers: u32| -> u32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
        set_maintain_workers(&mut app, band, keepers);
        for turn in 1..=(stated * 2) {
            run_turns_with_forage(&mut app, 1);
            if app
                .world
                .resource::<ForageRegistry>()
                .patch(coord)
                .expect("patch")
                .is_cultivated()
            {
                return turn;
            }
        }
        panic!("the Cultivate never completed within twice its stated {stated} turns");
    };

    let demand_in_hands = app_free()
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(fixture_tender_loads());
    assert_eq!(
        finished_on(demand_in_hands),
        stated,
        "a KEPT build finishes in exactly `work_cost / crew` turns — the keeping covers the rate \
         and the builders bank their whole output"
    );
    assert!(
        finished_on(NO_CREW_ON_THIS_ACTIVITY) > stated,
        "…and an unkept one is slower, because the meter rots under the builders past the grace: \
         {} vs {stated}",
        finished_on(NO_CREW_ON_THIS_ACTIVITY)
    );
}

/// **AN ABANDONED PART-BUILD STILL BLEEDS, on its own terms.** The rule is *"a meter bleeds when the
/// hands it needs are not on it"*, so walking away from a half-cleared patch costs exactly what
/// walking away from a finished one does — the rung's own `upkeep.work_per_turn` — and the cleared
/// ground grows back over. This is the constraint that rules out the simpler *"only completed rungs
/// cost anything"*, under which an abandoned investment would sit there untouched forever.
///
/// The system-level pace is pinned by [`abandoned_preparation_decays`]; this pins that the **rate**
/// is the rung's, and that the patch is billed to the **keeping pool** while it is unfinished — the
/// half-built meter §4.6a made holdable.
#[test]
fn an_abandoned_part_build_is_owed_the_keeping_pool_and_bleeds_the_rungs_rate() {
    let ladder = core_sim::LadderConfig::builtin();
    let rung = ladder.rung(RungKey::PlantTended);
    let cost = rung
        .build_cost(RUNG_COST_UNSCALED)
        .expect("the tended rung builds");
    let demand = rung.upkeep_demand(fixture_tender_loads());

    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch");
        patch.set_ladder_position(cost / 2.0, &core_sim::LadderConfig::builtin());
        patch.owner = Some(FactionId(0));
    }
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert!(
        !patch.is_cultivated(),
        "fixture: the meter is half-filled, so the rung is NOT finished"
    );
    // **THE SAME SUPPLIER EITHER WAY, AT THE RATE THE POSITION OWES** — the keeping pool owes a
    // meter carrying work from the first work banked (`docs/plan_standing_upkeep.md` §4.6a), and
    // since §2.8 what it owes interpolates, so a half-built meter asks for half the hands rather
    // than the rung's full count.
    // **The bill is quoted per tender-load of this ground** — resolved off the tile, so the four
    // readings below are one source's, not a mix of a patch's and a reference tile's.
    let tile_capacity = plant_tile_capacity(&app, coord);
    let forage = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .forage
        .clone();
    assert_eq!(
        core_sim::patch_upkeep_workers_needed(&patch, &ladder, tile_capacity, &forage),
        core_sim::patch_upkeep_demand(&patch, &ladder, tile_capacity, &forage).ceil() as u32,
        "hands to meet the demand the position actually owes, whoever is supplying it"
    );
    // Nobody is building it and nobody is keeping it, so the whole rate goes unmet.
    assert!(
        (core_sim::patch_upkeep_shortfall(&patch, &ladder, tile_capacity, &forage)
            - core_sim::patch_upkeep_demand(&patch, &ladder, tile_capacity, &forage))
        .abs()
            < 1e-6,
        "an abandoned part-build is short by the whole of what it owes — which since §2.8 is the \
         INTERPOLATED bill, not the finished rung's rate"
    );
    assert!(
        core_sim::patch_upkeep_demand(&patch, &ladder, tile_capacity, &forage) < demand,
        "…and that bill really is below the rung's own rate, or nothing about §2.8 is under test"
    );

    // And through the system: it bleeds the rung's own ROT RATE once the grace is spent — not the
    // demand it went short by. The two are separate dials since the retune
    // (`docs/plan_standing_upkeep.md` §2.4), and a part-build bleeds exactly what a walked-away-from
    // finished rung does.
    let bleed = unmaintained_bleed(ladder.rung(RungKey::PlantTended));
    let grace = tended_grace(&app);
    run_turns_untended(&mut app, grace + 1);
    let bled = cost / 2.0 - progress_of(&app, coord);
    assert!(
        (bled - bleed).abs() < 1e-5,
        "one bleeding turn takes the rung's own rate off a part-build: {bled} vs {bleed}"
    );
}

/// A throwaway world, purely to read the shipped ladder's dials from the helpers above without
/// threading an `App` through their closures.
fn app_free() -> App {
    spawn_world()
}

/// **Send the gatherers away, and nothing else** — the fixture's stand-in for
/// `assign_labor <faction> <band> forage <x> <y> 0`. It goes through `set_assignment` rather than
/// writing the row, because *what happens to the row at zero* is exactly what is under test.
fn unstaff_the_gatherers(app: &mut App, band: bevy::prelude::Entity, coord: UVec2) {
    /// A zero take needs no headroom, so the band's size cannot change the answer.
    const NO_HEADROOM_NEEDED: u32 = 0;
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("band exists")
        .set_assignment(
            LaborTarget::Forage {
                tile: coord,
                floor: FOOD_PEAK_FLOOR,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            0,
            NO_HEADROOM_NEEDED,
            None,
        );
}

/// Put `workers` on a band's **agriculture role** — the fixture's stand-in for
/// `assign_labor <faction> <band> agriculture <workers>`. Since maintenance left the tile
/// (`docs/plan_standing_upkeep.md` §2.5) the keeping is one band-level pool, spread across every
/// plant source the band works, so a fixture staffs the role rather than the patch.
fn set_maintain_workers(app: &mut App, band: bevy::prelude::Entity, workers: u32) {
    let headroom = {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("band exists");
        // **Exactly the headroom this row needs** — what every other row already holds, plus these
        // keepers. A real `assign_labor` reads the band's own working count and may refuse; a
        // fixture stating a role outright is not testing that refusal.
        let headroom = allocation.assigned_total() + workers;
        allocation.set_assignment(LaborTarget::Agriculture, workers, headroom, None);
        headroom
    };
    // **AND THE BAND HAS TO AFFORD IT**, or `LaborAllocation::normalize` trims the tail — which is
    // the very keeping role under measurement, leaving a fixture reading a pool nobody staffed.
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("band exists");
    if cohort.working.to_f32() < headroom as f32 {
        cohort.working = scalar_from_f32(headroom as f32);
    }
}

/// **MEASUREMENT HARNESS — what a band pays to HOLD its improvements, and what it loses by not.**
/// Not a guard; run with `--ignored --nocapture`. These are the numbers the shipped upkeep rates
/// should be judged on (`docs/plan_standing_upkeep.md` §2.4).
#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn probe_the_price_of_holding_a_plant_rung() {
    let ladder = core_sim::LadderConfig::builtin();
    println!("\nWHAT IT COSTS TO HOLD A *FINISHED* IMPROVEMENT, forever:");
    for (label, key) in [
        ("plant:tended", RungKey::PlantTended),
        ("plant:field", RungKey::PlantField),
    ] {
        let rung = ladder.rung(key);
        let demand = rung.upkeep_demand(fixture_tender_loads());
        let cost = rung
            .build_cost(RUNG_COST_UNSCALED)
            .expect("both plant rungs build");
        println!(
            "  {label}: {demand:.2} work/turn -> {} keeper(s); grace {} turns; \
             the rung is LOST on the first bleeding turn (progress {cost} -> {:.2}, below its own \
             cost), and the ground is fully wild again after {:.0} bleeding turns",
            rung.upkeep_crew_needed(fixture_tender_loads()),
            rung.upkeep_grace_turns(),
            cost - demand,
            cost / demand,
        );
    }

    // **A BUILD PAYS NONE OF IT.** A meter still being raised is owed its builders, so a Cultivate
    // runs at its stated pace with nobody on the keeping — printed here because the arc briefly got
    // this wrong in the other direction and quoted 34 turns for a 25-turn job.
    println!("\nWHAT A BUILD COSTS, in turns, at the reference crew:");
    {
        let stated = turns_to_prepare(&app_free());
        let finished_on = |keepers: u32| -> u32 {
            let mut app = spawn_world();
            let (tile, coord) = prime_thriving_patch(&mut app);
            grant_cultivation_knowledge(&mut app, FactionId(0));
            let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
            set_maintain_workers(&mut app, band, keepers);
            for turn in 1..=(stated * 2) {
                run_turns_with_forage(&mut app, 1);
                if app
                    .world
                    .resource::<ForageRegistry>()
                    .patch(coord)
                    .expect("patch")
                    .is_cultivated()
                {
                    return turn;
                }
            }
            0
        };
        println!(
            "  Cultivate, {} builders: stated {stated} | no keeper {} | one keeper {}",
            build_crew(&app_free()),
            finished_on(NO_CREW_ON_THIS_ACTIVITY),
            finished_on(A_KEEPER),
        );
    }

    // **What the keeper costs in FOOD depends on which term is binding**, and on a reference patch
    // it is the escapement, not the crew — so the hand put on the keeping was carrying nothing at
    // the margin. The burden is therefore in HEADS (a hand that could be on another source, a hunt
    // or a build), not in this patch's yield.
    const TURNS: u32 = 20;
    let income = |gatherers: u32, keepers: u32, floor: f32| -> f32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        seat_tended_patch(&mut app, coord);
        let band = spawn_forager_at(&mut app, tile, coord, None, gatherers, floor);
        set_maintain_workers(&mut app, band, keepers);
        run_turns_with_forage(&mut app, TURNS);
        provisions_f32(&mut app) / TURNS as f32
    };
    for (regime, floor) in [
        ("at the food peak", 0.5_f32),
        ("stripping it bare", 0.0_f32),
    ] {
        println!("\none tended patch {regime}, {TURNS} turns, food/turn:");
        for hands in [2_u32, 4, 6] {
            let all_gathering = income(hands, NO_CREW_ON_THIS_ACTIVITY, floor);
            let one_kept = income(hands - A_KEEPER, A_KEEPER, floor);
            println!(
                "  {hands} hands: all gathering {all_gathering:.3} | one on the keeping \
                 {one_kept:.3} | food cost of the keeper {:.3}",
                all_gathering - one_kept
            );
        }
    }
}

/// **A lost rung is announced — ON THE RETENTION BAR, not on the first bleed.** Crossing back below
/// the bar destroys a 25-turn investment's payoff, so the feed says so — once, on the transition, the
/// way the animal web has always announced a lost pen. The long bleed to zero that follows adds
/// nothing further.
///
/// **The edge moved, and that is the bug this arc was filed against**
/// (`docs/plan_standing_upkeep.md` §2.4): a completed meter sits exactly at its own cost, so under a
/// `progress >= cost` predicate the very first bleed of any size revoked the rung and pushed this
/// line — finish a Cultivate and the patch could be out of *tended* before its keepers were
/// assigned. The rung is held down to a stated fraction of its cost now, so the announcement lands
/// where the loss actually is.
#[test]
fn losing_a_tended_patch_pushes_one_feed_line() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    seat_tended_patch(&mut app, coord);
    let grace = tended_grace(&app);
    let survives = unmaintained_turns_before_the_rung_is_lost(&app);
    assert_eq!(
        survives,
        grace + 1,
        "fixture: with the retention bar deleted (§2.8) the rung goes on its first bleeding turn \
         past the grace — which is the edge the feed line must ride, exactly once"
    );

    run_turns_untended(&mut app, survives - 1);
    assert_eq!(
        completion_announcements(&app, "gone feral"),
        0,
        "a tended patch stays tended while its meter erodes — nothing has been lost yet"
    );

    run_turns_untended(&mut app, 1);
    assert_eq!(
        completion_announcements(&app, "gone feral"),
        1,
        "the turn the meter crosses the retention bar, the player is told"
    );

    // The rest of the bleed is not news.
    let feral_turns = turns_to_go_fully_feral(&app);
    run_turns_untended(&mut app, feral_turns);
    assert_eq!(
        completion_announcements(&app, "gone feral"),
        1,
        "the loss is announced once, not every turn of the bleed"
    );
    let entry = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .find(|e| e.label.contains("gone feral"))
        .expect("the feral line")
        .clone();
    let detail = entry.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("status=feral") && detail.contains("action=cultivate"),
        "the line rides the rung's own verb channel: {detail}"
    );
}

/// **THE CREW IS THE BUILD'S THROUGHPUT, in proportion and with NO CAP**
/// (`docs/plan_unit_costed_work.md` §1.2). Pinned at one worker, at the rung's own crew, and at four
/// times it: a test that only ever ran a full crew could not see whether the term exists, and one
/// that stopped at the rung's crew could not see that over-crewing now buys turns.
///
/// **This replaced `a_cultivate_build_accrues_in_proportion_to_its_crew`**, whose subject was the
/// retired `crew_scale` — `min(workers / crew_needed, 1)`, under which piling on hands bought
/// nothing. `crew_needed` survives as the staffing FLOOR alone
/// (`a_running_build_demands_at_least_its_crew` below).
#[test]
fn over_crewing_a_build_is_no_longer_capped() {
    let crew = build_crew(&spawn_world());
    assert!(
        crew >= 2,
        "the fixture needs a crew it can under-staff: {crew}"
    );

    let progress_after = |workers: u32| -> f32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        spawn_forager_of(&mut app, tile, coord, Some(Improvement::Cultivate), workers);
        run_turns_with_forage(&mut app, 1);
        progress_of(&app, coord)
    };

    /// The over-crewed multiple — comfortably past the rung's own crew, and small enough that a
    /// 50-unit Cultivate still takes several turns so the meter is measuring an accrual rather than
    /// a clamp.
    const OVER_CREWED: u32 = 4;

    // **Measured in WHOLE HANDS.** Nothing comes off the top: a build crew supplies nothing toward
    // the maintenance rate, which the band's keeping pool owes for this meter at any fullness
    // (`docs/plan_standing_upkeep.md` §4.6a). One turn is measured, so the rot cannot have started
    // and the reading is the accrual alone.
    let one = progress_after(1);
    let full = progress_after(crew);
    let over = progress_after(crew * OVER_CREWED);
    assert!(
        (one - PER_WORKER_OUTPUT).abs() < 1e-6,
        "one worker banks one worker-turn: {one}"
    );
    assert!(
        (full - crew as f32 * PER_WORKER_OUTPUT).abs() < 1e-6,
        "a crew of {crew} banks its {crew} worker-turns: {full}"
    );
    assert!(
        (over - (crew * OVER_CREWED) as f32 * PER_WORKER_OUTPUT).abs() < 1e-6,
        "and {OVER_CREWED}x the crew banks {OVER_CREWED}x the work — there is no cap: {over}"
    );
}

/// **A build's crew FLOORS the source's `workers_needed`** — the plant twin of a managed herd's
/// `herders_needed`. Without it the count was inverted from the *dipped* take, so committing to a
/// 25-turn improvement asked for fewer hands than gathering the same ground and flagged the second
/// worker as overstaffing.
#[test]
fn a_running_build_demands_at_least_its_take_crew() {
    /// The gathering crew the row's `workers_needed` is inverted from. **It is deliberately not the
    /// BUILD crew any more**: with the maintenance rate taxing the build
    /// (`docs/plan_standing_upkeep.md` §2.4) the build's staffing is strictly larger than the hands
    /// hauling the take, and `workers_needed` answers the *take* activity alone — the keeping's own
    /// count rides `upkeepWorkersNeeded` beside it.
    const TAKE_CREW: u32 = 2;

    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    set_forage_workers(&mut app, band, TAKE_CREW);
    run_turns_with_forage(&mut app, 1);

    let needed = app
        .world
        .get::<LaborAllocation>(band)
        .expect("band allocation")
        .last_yields[0]
        .workers_needed;
    assert!(
        needed >= TAKE_CREW,
        "a running Cultivate wants at least its gathering crew of {TAKE_CREW}, not {needed}"
    );
}

/// **THE TURNS ESTIMATE IS ANSWERED BY THE SIM, AND IT FALLS WHEN HANDS ARE ADDED** — the
/// player-facing payoff of pricing improvements in work (`docs/plan_unit_costed_work.md` §8). The
/// client cannot derive it: it holds neither the crew's output, nor the floor multiplier, nor the
/// kit. `None` is asserted positively beside it — the "no estimate" answer a source whose faction has
/// not learned the next rung's knowledge gives, which the wire renders as `-1`.
#[test]
fn the_build_estimate_is_the_sims_own_and_falls_as_hands_are_added() {
    let estimate = |workers: u32| -> Option<u32> {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        spawn_forager_of(&mut app, tile, coord, Some(Improvement::Cultivate), workers);
        run_turns_with_forage(&mut app, 1);
        published_count(
            app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .expect("patch")
                .build_turns_remaining,
        )
    };

    let crew = build_crew(&spawn_world());
    // **ONLY AN EMPTY CREW HAS NO ESTIMATE.** A crew at the rung's own maintenance rate used to be
    // quoted *never*, because the rate came off its output first; §4.6a deleted that threshold, so
    // every staffed build has a finish date and the rung's rate is the keeping pool's bill.
    assert_eq!(
        estimate(NO_CREW_ON_THIS_ACTIVITY),
        None,
        "nobody on the job has promised nothing — the one no-answer state"
    );
    let lightly = estimate(1).expect("a running build quotes a finish date");
    let fully = estimate(crew).expect("a running build quotes a finish date");
    assert!(
        fully < lightly,
        "adding hands shortens the same fixed job: {fully} at a crew of {crew} vs {lightly} at one \
         hand"
    );

    // A crew that has not learned Cultivation cannot be quoted the rung above them — the projection
    // refuses exactly where `validate_cultivate` would.
    let mut ungated = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut ungated);
    spawn_forager(&mut ungated, tile, coord, None);
    run_turns_with_forage(&mut ungated, 1);
    assert_eq!(
        ungated
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .build_turns_remaining,
        None,
        "a rung the faction's knowledge refuses is quoted no turns at all"
    );
}

/// **AN UNSTARTED SOURCE QUOTES THE JOB IT WOULD TAKE ON, AND DOUBLING THE CREW HALVES IT** —
/// `buildTurnsRemaining` is a *projection*, not "`-1` because nothing is being built"
/// (`docs/plan_unit_costed_work.md` §11).
///
/// That is the whole thesis of pricing improvements in work, and it has to be legible at the one
/// moment it drives a decision: the compose sheet is by definition looking at a patch nobody has
/// started. Same defect class, and same remedy, as `HerdTelemetryState.penUpkeep` projecting an
/// unpenned herd's running cost.
///
/// The halving is asserted as a **relation**, because the quote must track the crew rather than any
/// particular pair of literals — and the floor and kit are held equal across the two runs, so the
/// crew is the only thing that moved.
#[test]
fn an_unstarted_patch_quotes_the_next_rungs_job_and_the_quote_halves_with_the_crew() {
    let projection = |workers: u32| -> Option<u32> {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        // **Nothing queued here: the band is gathering and DECIDING**, which is by definition the
        // state a compose sheet is looking at. The quote is what the band's own `builders` pool
        // would take — since `docs/plan_standing_upkeep.md` §2.5 there is a real crew to quote and
        // the projection no longer falls back to the gatherers beside it — so the fixture stands
        // one at exactly the crew under test.
        let band = spawn_forager_of(&mut app, tile, coord, None, workers);
        app.world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band keeps its allocation")
            .assignments
            .push(LaborAssignment {
                target: LaborTarget::Builders,
                workers,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            });
        // ⛔ **AN EMPTY LEDGER IS WHAT HOLDS THE GEAR AXIS AT ITS IDENTITY HERE.** Nothing is
        // queued on this patch, so there is no entry to carry the bare kit the pace fixtures use
        // (`docs/plan_standing_upkeep.md` §4.7a ②) — and an *absent* `BandEquipment` is read as a
        // fully stocked band, so the projection would quote the roster-derived `tillage` and finish
        // half again as fast as the rung's own number. A band owning nothing has no tool to bring.
        app.world
            .entity_mut(band)
            .insert(core_sim::BandEquipment::default());
        run_turns_with_forage(&mut app, 1);
        published_count(
            app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .expect("patch")
                .build_turns_remaining,
        )
    };

    let crew = build_crew(&spawn_world());
    let quoted =
        projection(crew).expect("an unstarted patch with a crew on it quotes the next rung");
    assert_eq!(
        quoted,
        turns_to_prepare(&spawn_world()),
        "the projection is the rung's whole job at this crew — the same number a started build \
         counts down from"
    );

    // **Twice the crew, half the turns — restored** (`docs/plan_standing_upkeep.md` §4.6a). It was
    // briefly twice the *net*, the maintenance rate having come off the top first; a build crew pays
    // no rate, and on ground nobody has started there is nothing banked and so nothing to rot.
    let doubled_crew = crew * 2;
    let doubled = projection(doubled_crew).expect("a bigger crew is still quotable");
    assert_eq!(
        doubled,
        quoted.div_ceil(2),
        "twice the crew, half the turns: {quoted} at a crew of {crew}, {doubled} at {doubled_crew}"
    );

    // And a patch nobody is working is quoted nothing — there is no crew to quote at.
    let mut unworked = spawn_world();
    let (_, coord) = prime_thriving_patch(&mut unworked);
    grant_cultivation_knowledge(&mut unworked, FactionId(0));
    run_turns_with_forage(&mut unworked, 1);
    assert_eq!(
        unworked
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .build_turns_remaining,
        None,
        "no crew, no answer"
    );
}

/// **The WIRE's countdown is the turn the sim actually bites** — asserted on the published snapshot,
/// not on the seam, because the seam has its own tests above and what is at stake here is the
/// artifact the client renders "lapses in N turns" from.
///
/// The pairing is the point: a countdown that merely decremented would pass a test that only read
/// the wire. This walks the real Logistics pass and asserts that the **first** turn the published
/// remaining reads `0` is the **first** turn the meter moves — so the two cannot drift.
#[test]
fn the_published_neglect_countdown_hits_zero_on_the_turn_the_meter_moves() {
    let mut app = core_sim::build_test_app();
    app.update(); // the real Startup chain: worldgen, patch seeding, one capture.

    // Any seeded patch will do — the grace is the rung's, not the tile's.
    let coord = *app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .min_by_key(|tile| (tile.y, tile.x))
        .expect("worldgen seeds forage patches");
    seat_tended_patch(&mut app, coord);
    let seated_cost = cultivate_cost(&app);
    let grace = tended_grace(&app);

    let published = |app: &mut App| -> (bool, u32) {
        app.world
            .run_system_once(core_sim::recapture_snapshot_in_place);
        let row = app
            .world
            .resource::<core_sim::SnapshotHistory>()
            .last_snapshot()
            .expect("a capture")
            .forage_patches
            .iter()
            .find(|patch| patch.x == coord.x && patch.y == coord.y)
            .expect("the fixture patch is on the wire")
            .clone();
        (row.has_neglect_grace, row.neglect_grace_remaining)
    };

    // Un-neglected, the wire offers the whole grace plus the turn it bites on.
    assert_eq!(published(&mut app), (true, grace + 1));

    let mut first_zero = None;
    let mut first_move = None;
    for turn in 1..=(grace + 3) {
        app.world.run_system_once(advance_cultivation);
        let (has, remaining) = published(&mut app);
        assert!(has, "a tended patch always has something at risk");
        if remaining == 0 && first_zero.is_none() {
            first_zero = Some(turn);
        }
        if progress_of(&app, coord) < seated_cost && first_move.is_none() {
            first_move = Some(turn);
        }
    }
    assert_eq!(
        first_zero,
        Some(grace + 1),
        "the countdown reaches zero on the turn the penalty starts, not before"
    );
    assert_eq!(
        first_move, first_zero,
        "and that is the same turn the meter actually moves — the wire cannot drift from the gate"
    );

    // A wild patch has nothing at risk, and says so with the bool rather than a zero that would read
    // as "reverting now".
    let wild = *app
        .world
        .resource::<ForageRegistry>()
        .patches
        .iter()
        .find(|(_, patch)| {
            patch.owner.is_none() && patch.ladder_position() == core_sim::RUNG_UNSTARTED
        })
        .expect("most patches are wild")
        .0;
    app.world
        .run_system_once(core_sim::recapture_snapshot_in_place);
    let wild_row = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .last_snapshot()
        .expect("a capture")
        .forage_patches
        .iter()
        .find(|patch| patch.x == wild.x && patch.y == wild.y)
        .expect("on the wire")
        .clone();
    assert!(
        !wild_row.has_neglect_grace,
        "a wild patch has no improvement to lose"
    );
}

/// **THE BUILDING CREW'S ANSWER IS THE ONE THE SOURCE PUBLISHES.**
///
/// `build_turns_remaining` is a **per-source** field written **per assignment**, and two bands
/// routinely work one patch — one running a Cultivate, one simply gathering. Without a rule the
/// field was **last-writer-wins**, decided by the order the labor loop visits bands in, so the
/// gatherer's *projection of the next rung* landed on top of the running build's countdown and the
/// tile card quoted turns for a crew that was not building.
///
/// The two answers are held far apart on purpose — a builder of [`build_crew`] hands against a
/// **single** bystander — so the assertion cannot pass by coincidence, and the bystander is spawned
/// **second** so it is the one whose write would win.
#[test]
fn a_running_build_outranks_a_bystanders_projection_on_the_same_patch() {
    /// One hand on the take and nothing on the build — the slowest projection there is, and
    /// therefore the loudest disagreement with the builder's countdown.
    const A_LONE_BYSTANDER: u32 = 1;
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let crew = build_crew(&app);
    spawn_forager_of(&mut app, tile, coord, Some(Improvement::Cultivate), crew);
    // The bystander: same patch, no verb, and **one hand** — so its projection of the very same rung
    // is far longer than the builder's countdown. It used to be staffed at the rung's rate plus one,
    // on the retired rule that a crew at or below the rate has no finish date to quote at all; every
    // staffed crew quotes one now (§4.6a), so the fixture states the smallest crew there is and the
    // gap between the two answers is the widest it can be.
    let one_bystander = A_LONE_BYSTANDER;
    spawn_forager_of(&mut app, tile, coord, None, one_bystander);
    run_turns_with_forage(&mut app, 1);

    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    let published = patch
        .build_turns_remaining
        .expect("a patch with a running build quotes a finish date");

    // What each crew would say, from the ladder itself rather than from literals.
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let rung = ladder.rung(RungKey::PlantTended);
    let quote = |workers: u32, banked: f32| {
        let cost = rung
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the tended rung builds");
        core_sim::build_turns_remaining(
            cost,
            banked,
            rung.build_accrual(
                Some(Improvement::Cultivate),
                true,
                workers,
                core_sim::NO_BUILD_GEAR,
            ),
        )
        .expect("a staffed build quotes a finish date")
    };
    let builders_answer = quote(crew, patch.ladder_position());
    let bystanders_answer = quote(one_bystander, patch.ladder_position());
    assert!(
        bystanders_answer > builders_answer,
        "fixture: the two crews must disagree, or last-writer-wins is invisible \
         ({bystanders_answer} vs {builders_answer})"
    );
    assert_eq!(
        published_count(Some(published)),
        Some(builders_answer),
        "the patch must publish the BUILDING crew's countdown, not the bystander's projection of \
         the same rung ({bystanders_answer})"
    );
}

/// **AMONG CREWS BUILDING THE SAME SOURCE, THE SOONEST FINISH WINS.** They all fill one meter, so
/// each crew's quote counts only its own output and is therefore an over-estimate; the smallest is
/// the least wrong, and it is the one that must survive whichever order the loop visits the bands
/// in. Asserted **both ways round**, because a rule that only holds for one spawn order is the
/// last-writer-wins defect with a nicer number.
#[test]
fn the_soonest_of_two_building_crews_is_the_one_published() {
    let published_for = |big_crew_first: bool| -> u32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        let crew = build_crew(&app);
        const ONE_HAND: u32 = 1;
        let crews = if big_crew_first {
            [crew, ONE_HAND]
        } else {
            [ONE_HAND, crew]
        };
        for workers in crews {
            spawn_forager_of(&mut app, tile, coord, Some(Improvement::Cultivate), workers);
        }
        run_turns_with_forage(&mut app, 1);
        published_count(
            app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .expect("patch")
                .build_turns_remaining,
        )
        .expect("a patch with two running builds quotes a finish date")
    };

    let big_first = published_for(true);
    let small_first = published_for(false);
    assert_eq!(
        big_first, small_first,
        "the published estimate must not depend on the order the labor loop visits the bands in"
    );
}

// ---------------------------------------------------------------------------------------------
// THE BAND-LEVEL MAINTENANCE POOL (`docs/plan_standing_upkeep.md` §2.5)
// ---------------------------------------------------------------------------------------------

/// **A second tended patch inside the band's work range**, seated at a stated cost so *most-invested
/// first* has an unambiguous order to sort on. Returns its coordinate.
///
/// The demand is `flat` on both plant rungs, so two patches ask for the same work whatever they
/// cost — which is exactly what makes the **cost** the priority key rather than the demand.
fn seat_second_tended_patch(app: &mut App, near: UVec2, cost: f32) -> UVec2 {
    let work_range = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .band_work_range;
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let coord = {
        let candidates: Vec<UVec2> = app
            .world
            .query::<(&Tile, &FoodModuleTag)>()
            .iter(&app.world)
            .map(|(tile, _)| tile.position)
            .collect();
        let registry = app.world.resource::<ForageRegistry>();
        candidates
            .into_iter()
            .filter(|position| *position != near)
            .filter(|position| registry.patch(*position).is_some())
            .find(|position| {
                core_sim::grid_utils::hex_distance_wrapped(near, *position, grid.x, wrap)
                    <= work_range
            })
            .expect("a second seeded patch inside the band's work range")
    };
    declare_gathering_site(app, coord);
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch");
    patch.set_ladder_position(cost, &core_sim::LadderConfig::builtin());
    patch.owner = Some(FactionId(0));
    patch.neglect_turns = 0;
    coord
}

/// One band working **both** patches, with `keepers` on its `agriculture` role — the pool the two
/// tended patches draw on.
/// **WHAT ONE OF A BAND'S PLANT KEEPERS SUPPLIES PER TURN** — its bare `PER_WORKER_OUTPUT` plus
/// whatever the derived `agriculture` kit delivers (`docs/plan_standing_upkeep.md` §4.8: one supply
/// expression, two consumers). Read off the roster rather than stated as a literal, so retuning the
/// hoes moves every fixture below with the game.
///
/// **There is no keeping-kit picker**, so the pool derives the plant tool itself
/// ([`EquipmentConfig::keeping_kit_for_branch`]) — which is what stops the whole seam being a silent
/// no-op against `default_kits.agriculture` of `none`. The assertion below is that no-op guard.
fn plant_keeper_supply(keepers: u32) -> f32 {
    let equipment = core_sim::EquipmentConfig::builtin();
    let per_worker = equipment
        .keeping_kit_for_branch(core_sim::RungBranch::Plant, None)
        .map(|kit| {
            equipment.build_work_per_worker(
                &kit,
                &core_sim::BandEquipment::start_stocked(&equipment),
                core_sim::RungBranch::Plant,
                None,
            )
        })
        .expect("the shipped roster serves the plant web's keeping");
    assert!(
        per_worker > core_sim::NO_BUILD_GEAR,
        "fixture: the derived agriculture kit must actually deliver something — a bare \
         {per_worker} means the derivation resolved `none` and every assertion below is vacuous"
    );
    core_sim::pool_work_supply(keepers, per_worker)
}

fn spawn_band_keeping_two_patches(
    app: &mut App,
    home: bevy::prelude::Entity,
    first: UVec2,
    second: UVec2,
    keepers: u32,
    mode: core_sim::UpkeepFundMode,
) -> bevy::prelude::Entity {
    const GATHERERS: u32 = 1;
    let band = spawn_forager_at(
        app,
        home,
        first,
        None,
        GATHERERS,
        core_sim::DEFAULT_ESCAPEMENT_FLOOR,
    );
    // **The band has to afford every row it holds**, or `LaborAllocation::normalize` trims the tail
    // — which here is the very keeping role under measurement, and the test would then be reading a
    // pool nobody staffed.
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(band)
            .expect("the band was just spawned");
        cohort.working = scalar_from_f32((GATHERERS * 2 + keepers) as f32);
    }
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band was just spawned");
    allocation.assignments.push(LaborAssignment {
        target: LaborTarget::Forage {
            tile: second,
            floor: core_sim::DEFAULT_ESCAPEMENT_FLOOR,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        },
        workers: GATHERERS,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    });
    let headroom = allocation.assigned_total() + keepers;
    allocation.set_assignment(LaborTarget::Agriculture, keepers, headroom, None);
    allocation.upkeep_fund_mode = mode;
    band
}

/// What each patch's keepers supplied this turn, in the order `(first, second)`.
fn supplied_on(app: &App, first: UVec2, second: UVec2) -> (f32, f32) {
    let registry = app.world.resource::<ForageRegistry>();
    (
        registry.patch(first).expect("first patch").upkeep_supplied,
        registry
            .patch(second)
            .expect("second patch")
            .upkeep_supplied,
    )
}

/// **BOTH FUND MODES, ON A BAND THAT CANNOT COVER ITS TOTAL — and neither wastes a hand**
/// (`docs/plan_standing_upkeep.md` §2.5).
///
/// The keeping is one pool per web measured against the **sum** of what the band holds, so a band
/// short of that sum has to decide *how* it falls short. Both answers are defensible and the choice
/// is the player's:
///
/// - **spread** — proportional to demand, so everything degrades a little.
/// - **priority** — fund sources completely, **most-invested first**, so the biggest investment
///   stays whole and the marginal one rots.
///
/// **A POOL HAS NO LEFTOVER BY CONSTRUCTION**, which is the whole reason maintenance left the tile:
/// the per-source keeper crew it replaced had to round a fractional demand up to whole workers and
/// threw the remainder away, once per source. Asserted here as *the pool is fully spent under both
/// modes* — the property a per-source crew cannot have.
#[test]
fn both_fund_modes_split_a_short_pool_and_neither_wastes_a_hand() {
    /// The richer patch's meter — twice the poorer one's, so *most-invested first* has a strict
    /// order and a tie-break can never be what decides this test.
    const RICH_COST: f32 = 60.0;
    const POOR_COST: f32 = 30.0;
    /// Short of what these two patches want between them, so the pool cannot cover both and the two
    /// modes must answer differently. **It is short in SUPPLY, not in head count** — a keeper's
    /// supply reads the pool's kit since §4.8, and since the plant rungs began quoting their rate
    /// per **tender-load** the total also depends on the ground — so the fixture asserts the
    /// shortfall below rather than assuming it from the number.
    const KEEPERS: u32 = 1;

    let run = |mode: core_sim::UpkeepFundMode| -> (f32, f32, f32, f32, f32) {
        let mut app = spawn_world();
        let (tile, first) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, first);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(first).expect("patch");
            patch.set_ladder_position(RICH_COST, &core_sim::LadderConfig::builtin());
        }
        let second = seat_second_tended_patch(&mut app, first, POOR_COST);
        spawn_band_keeping_two_patches(&mut app, tile, first, second, KEEPERS, mode);
        app.world.run_system_once(advance_labor_allocation);
        let (rich, poor) = supplied_on(&app, first, second);
        // The two BILLS, which since §2.8 differ because the two positions do.
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let forage = app
            .world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .clone();
        // Both patches sit on the SAME tile's ground, so the tender-load is common to the pair and
        // the two bills differ by their positions alone — which is what this reads them for.
        let tile_capacity = plant_tile_capacity(&app, first);
        let registry = app.world.resource::<ForageRegistry>();
        let billed = |coord| {
            core_sim::patch_keeping_basis(
                registry.patch(coord).expect("patch"),
                &ladder,
                tile_capacity,
                &forage,
            )
        };
        (
            rich,
            poor,
            plant_keeper_supply(KEEPERS),
            billed(first),
            billed(second),
        )
    };

    let (rich, poor, pool, rich_bill, poor_bill) = run(core_sim::UpkeepFundMode::Spread);
    // **SPREAD IS PROPORTIONAL TO DEMAND, and since §2.8 two patches at different POSITIONS have
    // different demands.** The two used to be billed the same flat rate whatever they had cost, so
    // *"equally"* was the readable statement; now the richer meter owes more and is funded more. The
    // property is **proportionality to the bill** — read off the bills rather than off the positions,
    // because the rich one has climbed past the tended rung's top and its bill is interpolating on
    // the Field above it.
    assert!(
        rich_bill > poor_bill,
        "fixture: the richer position must owe more, or spread has nothing to be proportional to"
    );
    assert!(
        (rich / poor - rich_bill / poor_bill).abs() < 1e-3,
        "spread funds in proportion to what each owes: {rich} vs {poor} against bills {rich_bill} \
         and {poor_bill}"
    );
    assert!(
        (rich + poor - pool).abs() < 1e-5,
        "and it spends the whole pool — a pool has no leftover: {rich} + {poor} against {pool}"
    );

    let (rich, poor, pool, rich_bill, poor_bill) = run(core_sim::UpkeepFundMode::Priority);
    // **The most-invested source is funded COMPLETELY FIRST** — to **its own bill**, or to whatever
    // the pool has if that is less. Since §2.8 that bill is the source's own interpolated demand
    // rather than the rung's flat rate, which is why it is read back from the run rather than off
    // the ladder: the rich patch has climbed past the tended rung's top and owes a share of the
    // Field above it.
    assert!(
        pool < rich_bill + poor_bill,
        "fixture: the pool must be short of BOTH sources, or the two modes cannot differ — \
         {pool} against {rich_bill} + {poor_bill}"
    );
    assert!(
        (rich - pool.min(rich_bill)).abs() < 1e-5,
        "priority funds the most-invested source completely first: {rich} of {pool}, bill \
         {rich_bill}"
    );
    assert!(
        (poor - (pool - rich)).abs() < 1e-5,
        "…and the marginal one gets only what is left over — that is what the mode is for, got \
         {poor} of a {pool} pool"
    );
    assert!(
        poor < poor_bill,
        "…which must genuinely leave it short of its own bill, or the two modes are \
         indistinguishable: {poor} against {poor_bill}"
    );
    assert!(
        (rich + poor - pool).abs() < 1e-5,
        "priority spends the whole pool too: {rich} + {poor} against {pool}"
    );
}

/// **NAME THE KIT ONE PATCH IS KEPT WITH** — the whole of `upkeep_kit`, applied straight to the row
/// so a fixture measuring the split does not have to go through the command loop.
fn keep_patch_with(app: &mut App, band: bevy::prelude::Entity, patch: UVec2, kit_id: &str) {
    let kit = core_sim::EquipmentConfig::builtin()
        .kit(kit_id)
        .unwrap_or_else(|| panic!("the shipped roster carries '{kit_id}'"));
    let named = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("band exists")
        .set_upkeep_kit(&forage_target(patch), Some(kit));
    assert!(
        named,
        "fixture: the band must hold a row on {patch} for a keeping kit to land on"
    );
}

/// A `Forage` target naming `patch` — [`LaborTarget::same_source`] matches on the tile alone, so the
/// rest of the shape is only there to make one.
fn forage_target(patch: UVec2) -> LaborTarget {
    LaborTarget::Forage {
        tile: patch,
        floor: core_sim::DEFAULT_ESCAPEMENT_FLOOR,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
}

/// The tier a fresh set of `item` comes out of the item table at — read off the roster rather than
/// spelled, so a retune of the tier ids moves these fixtures with the game.
fn fresh_tier(item: &str) -> String {
    core_sim::EquipmentConfig::builtin()
        .item(item)
        .unwrap_or_else(|| panic!("the shipped item table carries '{item}'"))
        .default_tier()
        .id
        .clone()
}

/// **Give the band its own gear ledger holding exactly `sets` of hoes and nothing else** — the
/// scarcity the grouping test is about. Without an explicit ledger the labor pass invents one sized
/// to the band's head count, which is never short.
fn stock_hoes(app: &mut App, band: bevy::prelude::Entity, sets: u32) -> core_sim::BandEquipment {
    let mut ledger = core_sim::BandEquipment::default();
    ledger.stock(HOES, sets, &fresh_tier(HOES), None);
    app.world.entity_mut(band).insert(ledger.clone());
    ledger
}

/// The plant keeping tool, named once — the item `tillage` carries and the one these fixtures count.
const HOES: &str = "hoes";

/// **What one keeper of `kit` delivers per turn against `ledger`'s stock, over a pool of `keepers`**
/// — the coverage-weighted rate, resolved through the same three seams the sim's own is.
fn keeper_rate(kit_id: &str, keepers: u32, ledger: &core_sim::BandEquipment) -> f32 {
    let equipment = core_sim::EquipmentConfig::builtin();
    let kit = equipment
        .kit(kit_id)
        .unwrap_or_else(|| panic!("the shipped roster carries '{kit_id}'"));
    let gear = equipment
        .coverage(&kit, keepers as f32, ledger)
        .weighted_rate(|crew| {
            equipment.build_work_per_worker(crew, ledger, core_sim::RungBranch::Plant, None)
        });
    core_sim::build_work_per_worker_turn(gear)
}

/// **⛔ THE NEUTRALITY PROOF: MOVING THE KEEPING KIT ONTO THE WORK SITE MOVES NOTHING THAT SHIPS.**
///
/// The kit used to be one answer per band, read off the `agriculture` row, so the split was *one
/// work pool at one rate, divided in proportion to demand*. It is per site now
/// (`docs/plan_standing_upkeep.md` §2.7), so the split is *the worker pool, divided in proportion to
/// each site's own `demand ÷ its own keeper rate`*, and each site is supplied `its hands × its own
/// rate`.
///
/// **On the shipped roster every plant site resolves the same kit**, so every `r` is equal, the two
/// arithmetics are the same expression scaled by a constant, and the answer must not move by a bit.
/// This states the **retired** expression in full — `distribute_upkeep_pool` over a
/// `pool_work_supply` in WORK units — and asserts the live sim lands exactly on it.
///
/// **Exactly, not nearly.** A tolerance here would pass for a model that had quietly changed the
/// pacing by a percent, which is the one outcome this change was not allowed to have.
///
/// Both modes, because `upkeep_fund_mode` still governs the split and the two are different
/// arithmetic: `Spread` scales every demand by one coverage, `Priority` walks the slice.
#[test]
fn upkeep_kit_per_site_is_pacing_neutral_on_the_shipped_roster() {
    /// The two positions, so *most-invested first* has a strict order — the same shape
    /// `both_fund_modes_split_a_short_pool_and_neither_wastes_a_hand` measures.
    const RICH_COST: f32 = 60.0;
    const POOR_COST: f32 = 30.0;
    /// Short of what the two want between them, so the split is a live division rather than two
    /// saturated bills that would agree under any model.
    const KEEPERS: u32 = 1;

    for mode in [
        core_sim::UpkeepFundMode::Spread,
        core_sim::UpkeepFundMode::Priority,
    ] {
        let mut app = spawn_world();
        let (tile, first) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, first);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(first).expect("patch");
            patch.set_ladder_position(RICH_COST, &core_sim::LadderConfig::builtin());
        }
        let second = seat_second_tended_patch(&mut app, first, POOR_COST);
        spawn_band_keeping_two_patches(&mut app, tile, first, second, KEEPERS, mode);
        // **NEITHER ROW NAMES A KIT**, which is the case under test: every site takes its web's
        // derivation, so every rate is equal and the two splits must coincide.
        app.world.run_system_once(advance_labor_allocation);
        let (rich, poor) = supplied_on(&app, first, second);

        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let forage = app
            .world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .clone();
        let tile_capacity = plant_tile_capacity(&app, first);
        let registry = app.world.resource::<ForageRegistry>();
        let billed = |coord| {
            core_sim::patch_keeping_basis(
                registry.patch(coord).expect("patch"),
                &ladder,
                tile_capacity,
                &forage,
            )
        };
        // **THE RETIRED EXPRESSION, WRITTEN OUT.** One work pool at the band's one keeper rate, split
        // in proportion to demand, most-invested first — which is the order `maintenance_shares`
        // sorts into and the order `Priority` funds in.
        let retired = core_sim::distribute_upkeep_pool(
            plant_keeper_supply(KEEPERS),
            &[billed(first), billed(second)],
            mode,
        );
        assert!(
            retired[0] > 0.0 && retired[1] >= 0.0,
            "fixture: the retired split must fund something, or the comparison is vacuous"
        );
        assert!(
            plant_keeper_supply(KEEPERS) < billed(first) + billed(second),
            "fixture: the pool must be SHORT of both bills under {mode:?}, or a saturated split              would agree under any model — {} against {} + {}",
            plant_keeper_supply(KEEPERS),
            billed(first),
            billed(second)
        );
        assert_eq!(
            rich, retired[0],
            "the per-site split must land bit for bit on the retired per-band one under {mode:?}              when every site takes the default: {rich} against {}",
            retired[0]
        );
        assert_eq!(
            poor, retired[1],
            "…and so must the marginal source's share under {mode:?}: {poor} against {}",
            retired[1]
        );
    }
}

/// **TWO SITES ON ONE BAND, WORKED WITH TWO DIFFERENT TOOLS — each supplied and each worn at its
/// own rate** (`docs/plan_standing_upkeep.md` §2.7).
///
/// This is the thing the per-band kit could not express at all: one stored id put the same tool on
/// every site the band kept, so *hoes on the Field, bare hands on the scrub beside it* had no
/// spelling and the wear of the one tool was charged against the work of both.
///
/// # BOTH HALVES, BECAUSE EITHER ALONE PASSES A BROKEN MODEL
///
/// 1. **THE SUPPLY.** Under `Priority` the most-invested site is funded first out of the **worker**
///    pool, so a hoed leader needs fewer hands for the same bill and leaves strictly more for the
///    bare site behind it. A model that still resolved one rate for the whole web answers the same
///    number in both arms.
/// 2. **THE WEAR.** The hoes are spent on the work of the site that named them and on nothing else.
///    Calibrated against a single hoed site rather than against the config's `0.16`, so a retune of
///    the wear amount moves the reference with it — and the assertion discriminates because charging
///    both sites' work would be a materially larger number, which is asserted too.
#[test]
fn two_sites_on_one_band_are_kept_and_worn_at_their_own_kits_rates() {
    const RICH_COST: f32 = 60.0;
    const POOR_COST: f32 = 30.0;
    /// Short of the pair, so the leader's rate decides what is left for the follower — but enough
    /// hands that something reaches the follower, or *"the hoes were not charged for it"* is true
    /// for free.
    const KEEPERS: u32 = 2;
    /// The bare kit, named because *"the player chose none"* is a real selection and not an absence.
    const BARE: &str = "none";
    /// The plant keeping kit the roster derives — named explicitly on the arm that wants it, so the
    /// two arms differ in exactly one statement.
    const HOED: &str = "tillage";

    /// One run's answer: what each site was supplied, and how much condition the hoes lost.
    struct Kept {
        rich: f32,
        poor: f32,
        hoes_worn: f32,
    }

    let run = |rich_kit: &str, poor_kit: &str, mode: core_sim::UpkeepFundMode| -> Kept {
        let mut app = spawn_world();
        let (tile, first) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, first);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(first).expect("patch");
            patch.set_ladder_position(RICH_COST, &core_sim::LadderConfig::builtin());
        }
        let second = seat_second_tended_patch(&mut app, first, POOR_COST);
        let band = spawn_band_keeping_two_patches(&mut app, tile, first, second, KEEPERS, mode);
        // **Enough hoes that every keeper carries one**, so what the two arms differ in is the
        // SITES' selections and never the band's scarcity — that is the next test's subject.
        let fresh = stock_hoes(&mut app, band, KEEPERS + 1);
        keep_patch_with(&mut app, band, first, rich_kit);
        keep_patch_with(&mut app, band, second, poor_kit);
        app.world.run_system_once(advance_labor_allocation);
        let (rich, poor) = supplied_on(&app, first, second);
        let equipment = core_sim::EquipmentConfig::builtin();
        let worn = app
            .world
            .get::<core_sim::BandEquipment>(band)
            .expect("the fixture gave this band a ledger");
        Kept {
            rich,
            poor,
            hoes_worn: fresh.remaining(HOES, &equipment) - worn.remaining(HOES, &equipment),
        }
    };

    // ---- (1) THE SUPPLY -------------------------------------------------------------------------
    let hoed_leader = run(HOED, BARE, core_sim::UpkeepFundMode::Priority);
    let bare_leader = run(BARE, BARE, core_sim::UpkeepFundMode::Priority);
    assert!(
        hoed_leader.rich > bare_leader.rich,
        "a hoed site is supplied more of its own bill out of the same hands: {} against {}",
        hoed_leader.rich,
        bare_leader.rich
    );
    assert!(
        hoed_leader.rich + hoed_leader.poor > bare_leader.rich + bare_leader.poor,
        "…and the band's whole keeping rises with it, because the hands went further: {} against {}",
        hoed_leader.rich + hoed_leader.poor,
        bare_leader.rich + bare_leader.poor
    );

    // ---- (2) THE WEAR ---------------------------------------------------------------------------
    // **⛔ THE CALIBRATION COMES FROM A DIFFERENT RUN, and that is load-bearing.** Dividing the mixed
    // run's own wear by its own leader's supply would make the assertion below self-referential — it
    // holds for any wear at all, including a band-wide charge, because the divisor moves with the
    // dividend. So what one unit of upkeep work costs the hoes is read off a run where **both** sites
    // name them and every work unit is therefore theirs.
    let both_hoed = run(HOED, HOED, core_sim::UpkeepFundMode::Priority);
    let per_work = both_hoed.hoes_worn / (both_hoed.rich + both_hoed.poor);
    assert!(
        per_work > 0.0,
        "fixture: keeping with hoes must spend them, or the wear half is vacuous"
    );
    assert!(
        hoed_leader.poor > 0.0,
        "fixture: the bare site must actually be supplied something, or 'the hoes were not charged \
         for it' is true for free"
    );
    assert!(
        (hoed_leader.hoes_worn - per_work * hoed_leader.rich).abs() < 1e-4,
        "the hoes are spent on the work of the site that named them, and on nothing else: {} \
         against {}",
        hoed_leader.hoes_worn,
        per_work * hoed_leader.rich
    );
    assert!(
        hoed_leader.hoes_worn < per_work * (hoed_leader.rich + hoed_leader.poor) - 1e-4,
        "…and NOT on the bare site's work beside it — charging both would cost {}, which is what \
         the retired per-band wear kit did",
        per_work * (hoed_leader.rich + hoed_leader.poor)
    );

    // ---- AND `Spread` STILL GOVERNS THE SPLIT ---------------------------------------------------
    // Under `Spread` every site is held at the same fraction of its own bill whatever it is worked
    // with — the mode's own promise — so the two sites' supplies stay in the ratio of their bills
    // while the band's TOTAL still rises with the better tool.
    let spread_hoed = run(HOED, BARE, core_sim::UpkeepFundMode::Spread);
    let spread_bare = run(BARE, BARE, core_sim::UpkeepFundMode::Spread);
    assert!(
        spread_hoed.rich + spread_hoed.poor > spread_bare.rich + spread_bare.poor,
        "spread spends the same hands further when one site carries a tool: {} against {}",
        spread_hoed.rich + spread_hoed.poor,
        spread_bare.rich + spread_bare.poor
    );
    assert!(
        (spread_hoed.rich / spread_hoed.poor - spread_bare.rich / spread_bare.poor).abs() < 1e-3,
        "…and both arms still hold every site at the same fraction of its own bill: {} against {}",
        spread_hoed.rich / spread_hoed.poor,
        spread_bare.rich / spread_bare.poor
    );
}

/// **⛔ TWO SITES NAMING ONE KIT SHARE ITS SCARCITY — they do not each get a full set of it.**
///
/// `EquipmentConfig::coverage` answers *"of these workers, how many actually carry the kit's items,
/// given what the band owns"*. Asked once per site it **double-counts**: a band owning one set of
/// hoes, with its keepers split across two patches, would arm the keepers of each — two equipped
/// hands off one tool. So the claims are grouped by their resolved kit and coverage is taken once
/// per group, over that group's whole share of the pool
/// (`systems::labor::keeping_rates`, `docs/plan_standing_upkeep.md` §2.7).
///
/// **The assertion is the band's TOTAL keeping**, because with the pool short every hand is spent
/// and the total is exactly `keepers × the rate they were armed at`. One set of hoes among two
/// keepers arms one of them; the per-site reading would arm both.
#[test]
fn two_sites_naming_one_kit_cannot_arm_more_keepers_than_the_band_owns() {
    const RICH_COST: f32 = 60.0;
    const POOR_COST: f32 = 30.0;
    /// Two keepers and **one** set of hoes between them, which is the whole point.
    const KEEPERS: u32 = 2;
    const HOE_SETS: u32 = 1;

    for mode in [
        core_sim::UpkeepFundMode::Spread,
        core_sim::UpkeepFundMode::Priority,
    ] {
        let mut app = spawn_world();
        let (tile, first) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, first);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(first).expect("patch");
            patch.set_ladder_position(RICH_COST, &core_sim::LadderConfig::builtin());
        }
        let second = seat_second_tended_patch(&mut app, first, POOR_COST);
        let band = spawn_band_keeping_two_patches(&mut app, tile, first, second, KEEPERS, mode);
        let ledger = stock_hoes(&mut app, band, HOE_SETS);
        // **BOTH SITES NAME THE SAME KIT**, so they are one group and take one coverage between
        // them. Named rather than derived so the test states its own premise.
        keep_patch_with(&mut app, band, first, "tillage");
        keep_patch_with(&mut app, band, second, "tillage");
        app.world.run_system_once(advance_labor_allocation);
        let (rich, poor) = supplied_on(&app, first, second);

        // What the band can actually put on the ground: two keepers, one of them armed.
        let shared = core_sim::pool_work_supply(KEEPERS, 0.0)
            + (keeper_rate("tillage", KEEPERS, &ledger) - core_sim::PER_WORKER_OUTPUT)
                * KEEPERS as f32;
        let per_site = core_sim::pool_work_supply(KEEPERS, 0.0)
            + (keeper_rate("tillage", 1, &ledger) - core_sim::PER_WORKER_OUTPUT) * KEEPERS as f32;
        assert!(
            per_site > shared + 1e-5,
            "fixture: one set of hoes among {KEEPERS} keepers must actually be scarce, or the two              readings agree and nothing is under test — {per_site} against {shared}"
        );
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let forage = app
            .world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .clone();
        let tile_capacity = plant_tile_capacity(&app, first);
        let registry = app.world.resource::<ForageRegistry>();
        let billed = |coord| {
            core_sim::patch_keeping_basis(
                registry.patch(coord).expect("patch"),
                &ladder,
                tile_capacity,
                &forage,
            )
        };
        assert!(
            shared < billed(first) + billed(second),
            "fixture: the pool must be short of both bills under {mode:?}, or the hands are not all              spent and the total says nothing about the rate"
        );
        assert!(
            rich + poor <= shared + 1e-5,
            "two sites on one kit cannot arm more keepers than the band owns hoes: {} against a              shared {shared} under {mode:?}",
            rich + poor
        );
        assert!(
            rich + poor < per_site - 1e-5,
            "…and strictly below what {KEEPERS} FULLY armed keepers would put on the ground \
             ({per_site}) under {mode:?} — the ceiling a per-site coverage reads toward, and the \
             mark that the band's one tool is really being shared",
        );
    }
}

/// A band that **holds** a finished tended patch and has a `Cultivate` at the **head** of its build
/// queue on bare ground — the shape a blocked head's dilution was reported on. Both patches are
/// gathered, both draw on the one `agriculture` pool, and the whole `builders` pool stands on the
/// queued entry.
fn spawn_band_holding_one_patch_and_queueing_a_build(
    app: &mut App,
    home: bevy::prelude::Entity,
    holding: UVec2,
    build: UVec2,
    keepers: u32,
    builders: u32,
) -> bevy::prelude::Entity {
    const GATHERERS: u32 = 1;
    let band = spawn_forager_at(app, home, holding, None, GATHERERS, FOOD_PEAK_FLOOR);
    let headroom = {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band was just spawned");
        allocation
            .assignments
            .push(forage_row(build, FOOD_PEAK_FLOOR, GATHERERS));
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Builders,
            workers: builders,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
        let headroom = allocation.assigned_total() + keepers;
        allocation.set_assignment(LaborTarget::Agriculture, keepers, headroom, None);
        allocation.build_queue.push(core_sim::BuildQueueEntry {
            source: core_sim::BuildSource::Patch(build),
            declared: core_sim::BuildJob::Rung(Improvement::Cultivate),
            kit: Some(bare_builders()),
        });
        headroom
    };
    // **The band has to afford every row it holds**, or `normalize` trims the tail — which here is
    // the keeping role under measurement.
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band was just spawned");
    cohort.working = scalar_from_f32(headroom as f32);
    band
}

/// **A BLOCKED HEAD CLAIMS NO KEEPING, AND THE BAND'S REAL HOLDING IS PAID IN FULL**
/// (`docs/plan_standing_upkeep.md` §4.6a).
///
/// The claim side needs a verb term only because `maintenance_shares` runs before the accrual that
/// banks a build's first work. Narrowed to the funded head alone, that term still admitted a head
/// whose own rung **gate refuses** — which banks nothing on any turn, ever, while claiming the full
/// rung demand. Under the default `Spread` the pool is divided pro rata, so a build the ground was
/// never going to accept starved the tended ground the band actually holds.
///
/// **Both halves are asserted on one fixture, because either alone passes with the fix wrong.** A
/// claim gate that answered *never* would pass the blocked arm and fail nothing else — so the same
/// head, **unblocked**, must claim its demand exactly as before, which is the case the verb term
/// exists for in the first place.
///
/// **And the WIRE half rides with it.** The capture reads a source's demand with no verb in flight
/// (`patch_upkeep_demand(patch, NOTHING_IN_FLIGHT, …)`), which is `0` for a meter at zero — so a
/// blocked head that had been *stamped* a positive supply published `upkeepSupplied > 0` against
/// `upkeepDemand 0`, a row disagreeing with itself.
#[test]
fn a_blocked_head_claims_no_keeping_and_the_holding_beside_it_is_paid_in_full() {
    /// Enough supply to cover ONE tended rung outright and not two, so *"the holding is paid in
    /// full"* and *"the pool is diluted"* are distinguishable outcomes on the same fixture.
    const KEEPERS: u32 = 2;
    /// A staffed pool standing on the head — a blocked head is only reportable, and only dilutive,
    /// when the player has actually committed builders to it.
    const BUILDERS: u32 = 2;
    /// The holding's own meter. Any completed cost does; it is the *demand* that matters and that
    /// is the rung's, not the meter's.
    const HOLDING_COST: f32 = 40.0;
    /// **A meter at zero** — the only state the narrowing may cut, because the declaration answers
    /// for a meter at zero and nothing else (`forage::patch_build_verb`).
    const NOTHING_BANKED: f32 = core_sim::RUNG_UNSTARTED;
    /// **Work already banked on the blocked head**, as a fraction of the rung's cost.
    const PART_BUILT: f32 = 0.4;

    struct Turn {
        holding_supplied: f32,
        holding_demand: f32,
        build_supplied: f32,
        build_demand: f32,
        build_progress: f32,
        blocked_reason: String,
    }

    let run = |knows_cultivation: bool, banked_on_the_build: f32| -> Turn {
        let mut app = spawn_world();
        // The **build** target is the primed tile, because `prime_thriving_patch` is the helper that
        // guarantees a basket the tended rung can commit to — so the only thing standing between
        // this head and an open gate is the knowledge below.
        let (tile, build) = prime_thriving_patch(&mut app);
        let holding = seat_second_tended_patch(&mut app, build, HOLDING_COST);
        if banked_on_the_build > core_sim::RUNG_UNSTARTED {
            // **Work already on the ground**, which is what makes the claim the PROGRESS term's
            // rather than the verb's. A patch carrying progress is owned and committed in play, so
            // the fixture states both or the arm measures a different refusal.
            let crop = {
                let labor = app.world.resource::<LaborConfigHandle>().get();
                let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
                let map_seed = app.world.resource::<SimulationConfig>().map_seed;
                let mut query = app.world.query::<&Tile>();
                let ground = query
                    .iter(&app.world)
                    .find(|ground| ground.position == build)
                    .expect("the build tile exists")
                    .clone();
                let composition = tile_flora_composition(&flora, &labor.forage, &ground, map_seed);
                default_species_for_rung(&composition, &flora, RungKey::PlantTended)
                    .expect("prime_thriving_patch chose ground the tended rung can commit to")
            };
            let cost = cultivate_cost(&app);
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(build).expect("patch");
            patch.set_ladder_position(
                cost * banked_on_the_build,
                &core_sim::LadderConfig::builtin(),
            );
            patch.owner = Some(FactionId(0));
            patch.species = Some(crop);
        }
        if knows_cultivation {
            grant_cultivation_knowledge(&mut app, FactionId(0));
        }
        spawn_band_holding_one_patch_and_queueing_a_build(
            &mut app, tile, holding, build, KEEPERS, BUILDERS,
        );
        app.world.run_system_once(advance_labor_allocation);
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let registry = app.world.resource::<ForageRegistry>();
        let read = |coord: UVec2| -> (f32, f32) {
            let patch = registry.patch(coord).expect("patch");
            (
                patch.upkeep_supplied,
                // **Read the way the CAPTURE reads it** — the BILL the keeping answered, which is
                // what the published trio is struck from (`forage::patch_keeping_basis`). The live
                // demand beside it has already been raised by this turn's own accrual.
                core_sim::patch_keeping_basis(
                    patch,
                    &ladder,
                    plant_tile_capacity(&app, coord),
                    &app.world.resource::<LaborConfigHandle>().get().forage,
                ),
            )
        };
        let (holding_supplied, holding_demand) = read(holding);
        let (build_supplied, build_demand) = read(build);
        Turn {
            holding_supplied,
            holding_demand,
            build_supplied,
            build_demand,
            build_progress: registry.patch(build).expect("patch").ladder_position(),
            blocked_reason: registry
                .patch(build)
                .expect("patch")
                .build_blocked_reason
                .key()
                .to_string(),
        }
    };

    // --- (a) THE BLOCKED HEAD ------------------------------------------------------------------
    let blocked = run(false, NOTHING_BANKED);
    // **Liveness, and the cause**: the head really is stuck, and stuck for the reason the fixture
    // staged. Without this the arm passes for a head that was simply never at the front.
    assert_eq!(
        blocked.blocked_reason, "knowledge",
        "fixture: the head must be blocked, and by the knowledge gate — got '{}'",
        blocked.blocked_reason
    );
    assert_eq!(
        blocked.build_progress, 0.0,
        "fixture: a blocked head banks nothing, so its meter must still be at zero (got {})",
        blocked.build_progress
    );

    // **THE HEADLINE.** The band's real holding is supplied to its whole demand, because the
    // blocked head asked for nothing.
    assert!(
        blocked.holding_demand > 0.0,
        "fixture: the holding must owe something, or 'paid in full' is vacuous"
    );
    assert!(
        (blocked.holding_supplied - blocked.holding_demand).abs() < EPSILON,
        "a blocked head must not dilute the band's real holding: supplied {} of {}",
        blocked.holding_supplied,
        blocked.holding_demand
    );

    // **THE WIRE HALF** — the blocked source publishes a supply of `0` against a demand of `0`,
    // rather than a stamped share against the capture's own zero.
    assert_eq!(
        blocked.build_demand, 0.0,
        "a meter at zero owes nothing once the verb is out of the reading — that is what the \
         capture publishes ({})",
        blocked.build_demand
    );
    assert_eq!(
        blocked.build_supplied, 0.0,
        "…so the pool must have put nothing on it either, or the row disagrees with itself on the \
         wire ({})",
        blocked.build_supplied
    );

    // --- (b) THE SAME HEAD, UNBLOCKED ----------------------------------------------------------
    let open = run(true, NOTHING_BANKED);
    assert_eq!(
        open.blocked_reason, "",
        "fixture: granting the knowledge must open the gate — still blocked on '{}'",
        open.blocked_reason
    );
    assert!(
        open.build_progress > 0.0,
        "fixture: the unblocked head must bank its first work this turn, or the verb term is never \
         exercised"
    );
    // **The claim is back, and it is the rung's whole demand** — the state §4.6a's first-turn fix
    // exists for: the accrual lands *after* the split, so the ground carries nothing when the pool
    // is divided and the verb is the only thing that can speak for it.
    assert!(
        open.build_supplied >= open.build_demand,
        "the turn a build banks its first work, its keeping pool must cover the bill it was handed \
         — the demand interpolates, so on turn one that bill is honestly small ({} against {})",
        open.build_supplied,
        open.build_demand
    );
    // **SPREAD IS PROPORTIONAL TO DEMAND**, and since §2.8 a build on its first turn owes almost
    // nothing while the finished holding beside it owes its rung's whole rate — so the two are
    // funded proportionally, not equally. What this arm is really about is that the build **claims
    // at all**, which the assertion above states.
    assert!(
        open.build_supplied <= open.holding_supplied,
        "the build owes less than the finished holding, so it draws less: build {} against holding \
         {}",
        open.build_supplied,
        open.holding_supplied
    );
    // **AND THE HOLDING IS NOT STARVED BY IT** — which is the direction that matters. The build's
    // first-turn bill is nearly nothing (§2.8: it stood at zero when the pool was split), so the
    // finished holding beside it is funded in full. Dilution is real and grows with the build's
    // meter; what must never happen is a *blocked* or *unstarted* entry taking the holding's share,
    // and that is what the (a) arm above pins.
    assert!(
        open.holding_supplied >= open.holding_demand - EPSILON,
        "…and the finished holding is still covered, because a build one turn in owes almost \
         nothing — {} of {}",
        open.holding_supplied,
        open.holding_demand
    );

    // --- (c) A BLOCKED HEAD THAT HAS ALREADY BANKED WORK GOES ON CLAIMING --------------------------
    //
    // **The narrowing may only cut a meter at ZERO.** A declaration answers for a zero meter and
    // nothing else (`forage::patch_build_verb`), so a blocked head carrying progress is answered for
    // by the ground itself — and the pool owes for that work whatever the gate says, which is §4.6a's
    // *"a meter carrying work is billed at any fullness"*. A gate term that cut here would put a
    // half-built patch back where it cannot be held at all.
    let stalled = run(false, PART_BUILT);
    assert_eq!(
        stalled.blocked_reason, "knowledge",
        "fixture: this arm must still be blocked, or it is not testing the same state ('{}')",
        stalled.blocked_reason
    );
    assert!(
        stalled.build_demand > 0.0,
        "fixture: a meter carrying work owes the rung's rate ({})",
        stalled.build_demand
    );
    assert!(
        stalled.build_supplied > 0.0,
        "a blocked head with work already banked must go on claiming — the pool owes for the ground,          not for the verb ({})",
        stalled.build_supplied
    );
}

/// **THE ALLOCATION SURVIVES A CHECKPOINT, UNDER BOTH MODES** — the fund mode is `SimState`, so a
/// restored world splits its pool exactly as the original did.
///
/// It rides the band's `LaborAllocation`, which `capture_sim_state` clones whole, so this is
/// *asserted rather than assumed*: a mode that failed to round-trip would silently drop a
/// priority-funded band back to `spread` on the next rollback, and the only symptom would be a
/// Field rotting for reasons nobody could reconstruct.
#[test]
fn the_maintenance_split_survives_a_checkpoint_under_both_modes() {
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    const RICH_COST: f32 = 60.0;
    const POOR_COST: f32 = 30.0;
    const KEEPERS: u32 = 2;
    /// A band id no start profile uses, so the fixture band is unambiguous in the restored world.
    const FIXTURE_BAND_ID: core_sim::BandId = core_sim::BandId(9001);

    for mode in [
        core_sim::UpkeepFundMode::Spread,
        core_sim::UpkeepFundMode::Priority,
    ] {
        // **The FULL app**, not this file's minimal harness: `capture_sim_state` reads every
        // resource a checkpoint carries, and a partial world panics on the first one it lacks.
        let mut app = core_sim::build_test_app();
        app.update();
        let (tile, first) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, first);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(first).expect("patch");
            patch.set_ladder_position(RICH_COST, &core_sim::LadderConfig::builtin());
        }
        let second = seat_second_tended_patch(&mut app, first, POOR_COST);
        let band = spawn_band_keeping_two_patches(&mut app, tile, first, second, KEEPERS, mode);
        // **A checkpoint keys a band by its `BandId`**, so a fixture band without one is not
        // captured at all — and the restored world would then be measured with no band on it, which
        // passes a naive equality against stale scratch.
        // **AND ITS OWN EQUIPMENT LEDGER, EXPLICITLY** — a band with no `BandEquipment` component
        // resolves its kit through `start_stocked_for`'s absent-component fallback, while
        // `capture_sim_state` records `unwrap_or_default()`, i.e. an EMPTY ledger, and `restore`
        // inserts it. Live and restored would then be geared and bare respectively, which is a
        // property of the fixture rather than of the split under test. Every production band is
        // spawned with the component (`systems::worldgen`), so this makes the fixture the ordinary
        // case rather than papering over one.
        app.world.entity_mut(band).insert((
            FIXTURE_BAND_ID,
            core_sim::ResidentBand,
            core_sim::BandEquipment::start_stocked(&core_sim::EquipmentConfig::builtin()),
        ));

        app.world.run_system_once(advance_labor_allocation);
        let before = supplied_on(&app, first, second);
        assert!(
            before.0 + before.1 > 0.0,
            "{mode:?}: fixture — the pool must actually reach the patches, or the comparison below \
             is between two zeroes"
        );
        let checkpoint = capture_sim_state(&app.world);

        // Rewrite the world into a state that would split differently, then rewind it.
        {
            let mut query = app.world.query::<&mut LaborAllocation>();
            for mut allocation in query.iter_mut(&mut app.world) {
                allocation.upkeep_fund_mode = core_sim::UpkeepFundMode::default();
                allocation
                    .assignments
                    .retain(|a| !matches!(a.target, LaborTarget::Agriculture));
            }
        }
        restore_sim_state(&mut app.world, &checkpoint);

        // The supply is per-turn scratch the Logistics pass clears; clear it here so the second run
        // is measured from the same start as the first rather than accumulating onto it.
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            for coord in [first, second] {
                registry.patch_mut(coord).expect("patch").upkeep_supplied = 0.0;
            }
        }
        app.world.run_system_once(advance_labor_allocation);
        let after = supplied_on(&app, first, second);
        assert!(
            (before.0 - after.0).abs() < 1e-5 && (before.1 - after.1).abs() < 1e-5,
            "{mode:?}: a restored band splits its pool exactly as the original did — {before:?} \
             against {after:?}"
        );
        // **The fixture band, found by the role it holds** — a full app also carries the start
        // profile's own bands, and reading whichever the query visited first would assert against a
        // band this test never touched.
        let restored_mode = {
            let mut query = app.world.query::<&LaborAllocation>();
            query
                .iter(&app.world)
                .find(|allocation| allocation.workers_on(&LaborTarget::Agriculture) > 0)
                .map(|allocation| allocation.upkeep_fund_mode)
                .expect("the restored fixture band carries its keeping role")
        };
        assert_eq!(
            restored_mode, mode,
            "the fund mode itself rides the checkpoint"
        );
    }
}

/// **THE REPORTED BUG, AND WHAT REPLACED THE PATCH FOR IT.**
///
/// A completed meter sits **exactly** at its own cost, so a `progress >= cost` predicate made the
/// first bleed of any size revoke the rung — finish a Cultivate and the patch could be out of
/// *tended* before its keepers were assigned. The fix was a **retention bar**: a stamped point below
/// the cost that the rung was held down to.
///
/// **The bar is deleted, because the one-position ladder removes the CLIFF it was patching**
/// (`docs/plan_standing_upkeep.md` §2.8/§4.10). The rung really is lost on the first bleeding turn
/// past its grace again — and that is now a **rounding** rather than a cliff, because everything the
/// rung is worth interpolates on the position: a patch a hair below the tended rung's top owes a
/// hair under a whole tended patch's keeping and pays a hair under its rate, where the original
/// predicate dropped both to a wild stand's outright.
///
/// So this asserts the pair that makes the deletion safe: the rung goes the moment the position
/// dips, and what crossing that boundary costs is a fraction of a percent.
#[test]
fn losing_the_tended_rung_to_a_dip_costs_almost_nothing_because_the_rung_interpolates() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    seat_tended_patch(&mut app, coord);

    let demand_at = |app: &App| -> f32 {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let forage = app
            .world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .clone();
        let tile_capacity = plant_tile_capacity(app, coord);
        let registry = app.world.resource::<ForageRegistry>();
        core_sim::patch_upkeep_demand(
            registry.patch(coord).expect("patch"),
            &ladder,
            tile_capacity,
            &forage,
        )
    };
    let is_tended = |app: &App| -> bool {
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated()
    };

    let survives = unmaintained_turns_before_the_rung_is_lost(&app);
    let grace = tended_grace(&app);
    assert_eq!(
        survives,
        grace + 1,
        "with no bar left, the rung goes on the first bleeding turn past its own grace"
    );

    run_turns_untended(&mut app, survives - 1);
    assert!(
        is_tended(&app),
        "fixture: still tended on the turn before the first bleed lands"
    );
    let held = demand_at(&app);

    run_turns_untended(&mut app, 1);
    assert!(
        !is_tended(&app),
        "and the rung IS lost the moment the position dips — there is no bar holding it"
    );
    let dipped = demand_at(&app);

    /// How much of the rung's worth crossing the boundary may cost. One turn's bleed is a fraction
    /// of a work unit against a 50-unit rung, so the fall is a fraction of a percent; the bound is
    /// loose enough to survive a rot retune and tight enough that a re-introduced cliff — which
    /// drops the rung's worth to the wild rung's `0` — fails it outright.
    const A_ROUNDING_OF_THE_RUNG: f32 = 0.05;
    assert!(
        held > core_sim::NO_UPKEEP_DEMAND,
        "liveness: a held tended patch really does owe something, or the comparison is vacuous"
    );
    assert!(
        dipped >= held * (1.0 - A_ROUNDING_OF_THE_RUNG),
        "crossing the rung boundary must be a rounding, not a cliff: {held} -> {dipped}"
    );
}

// ---------------------------------------------------------------------------------------------
// THE ROT IS WHAT EATS A BUILD, NOT THE RATE (`docs/plan_standing_upkeep.md` §4.6a)
// ---------------------------------------------------------------------------------------------

/// **A HALF-BUILT METER NOBODY IS BUILDING IS HELD BY THE BAND'S KEEPING POOL — EXACTLY**
/// (`docs/plan_standing_upkeep.md` §4.6a). This is the first of the two states the deleted fullness
/// test made unreachable, and it was reported from ordinary play: take the builders off a Cultivate
/// at half its cost and the patch was billed to a crew that is not there, so it bled its full rate
/// with keepers standing idle in the `agriculture` role and **no command that could aim them at it**.
///
/// **The liveness half rides in the same test, on the same fixture**, because *"it holds"* also
/// passes on a meter that cannot move: the identical patch with the role unstaffed bleeds.
#[test]
fn a_half_built_patch_with_no_builders_is_held_exactly_by_a_staffed_keeping() {
    /// Past the rung's grace, so the unkept arm is genuinely bleeding rather than forgiven.
    const TURNS: u32 = 6;

    let progress_over = |keepers: u32| -> (f32, f32) {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        // **Mid-Cultivate at roughly half its cost, and NOBODY on the build.** The verb is left
        // unset: the meter carrying progress is the declaration (`forage::patch_build_verb`).
        let cost = cultivate_cost(&app);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).expect("patch");
            patch.set_ladder_position(cost / 2.0, &core_sim::LadderConfig::builtin());
            patch.owner = Some(FactionId(0));
        }
        let started = progress_of(&app, coord);
        let band = spawn_forager_of(&mut app, tile, coord, None, FORAGE_WORKERS);
        set_maintain_workers(&mut app, band, keepers);
        run_turns_with_forage(&mut app, TURNS);
        (started, progress_of(&app, coord))
    };

    let (started, kept) = progress_over(tended_keeping_crew());
    assert!(
        (kept - started).abs() < 1e-4,
        "a staffed keeping holds a half-built meter EXACTLY where it is, turn over turn: \
         {started} -> {kept}"
    );

    // …and it is holding because the demand is MET, not because nothing was billed.
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let cost = cultivate_cost(&app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch");
        patch.set_ladder_position(cost / 2.0, &core_sim::LadderConfig::builtin());
        patch.owner = Some(FactionId(0));
    }
    let band = spawn_forager_of(&mut app, tile, coord, None, FORAGE_WORKERS);
    set_maintain_workers(&mut app, band, tended_keeping_crew());
    run_turns_with_forage(&mut app, 1);
    let tile_capacity = plant_tile_capacity(&app, coord);
    let forage = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .forage
        .clone();
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch");
    assert!(
        core_sim::patch_upkeep_demand(
            patch,
            &core_sim::LadderConfig::builtin(),
            tile_capacity,
            &forage,
        ) > core_sim::NO_UPKEEP_DEMAND,
        "a meter carrying work is BILLED, at any fullness — the pool has something to cover"
    );
    assert_eq!(
        core_sim::patch_upkeep_shortfall(
            patch,
            &core_sim::LadderConfig::builtin(),
            tile_capacity,
            &forage,
        ),
        core_sim::NO_UPKEEP_DEMAND,
        "…and the staffed pool covers it in full: nothing unmet"
    );

    // **THE LIVENESS HALF.** The same fixture with the role empty bleeds, so the holding above is a
    // demand being met rather than a meter that cannot move.
    let (started, unkept) = progress_over(NO_CREW_ON_THIS_ACTIVITY);
    assert!(
        unkept < started - 1e-4,
        "…and with the role unstaffed the same meter loses ground: {started} -> {unkept}"
    );
}

/// **A COMPLETED RUNG ERODED BELOW ITS COST IS STILL FUNDED BY THE KEEPING POOL** — the second state
/// the deleted fullness test made unreachable (`docs/plan_standing_upkeep.md` §4.6a).
///
/// A rung eroding to 99% is *below its cost*, so it used to flip into **building** and stop being the
/// pool's business at the very moment it started needing one — then, topped back up, return to a pool
/// that was still short and dip again. It oscillates, and under the next slice's queue it would do so
/// while displacing the build the player actually ordered.
#[test]
fn an_eroded_rung_is_still_funded_by_the_keeping_pool() {
    /// Past the rung's grace, so an unfunded arm would genuinely bleed.
    const TURNS: u32 = 6;

    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    seat_tended_patch(&mut app, coord);
    let band = spawn_forager(&mut app, tile, coord, None);
    set_maintain_workers(&mut app, band, tended_keeping_crew());

    // One turn's rot off a completed meter, exactly as one short turn would have taken.
    let (_, bleed) = cultivation_config(&app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        registry
            .patch_mut(coord)
            .expect("patch")
            .decay_ladder(bleed, &core_sim::LadderConfig::builtin());
    }
    let eroded = progress_of(&app, coord);
    assert!(
        eroded < cultivate_cost(&app),
        "fixture: the meter really is below its cost"
    );

    run_turns_with_forage(&mut app, TURNS);
    assert!(
        (progress_of(&app, coord) - eroded).abs() < 1e-4,
        "a dipped rung is held by the pool exactly where it is: {eroded} -> {}",
        progress_of(&app, coord)
    );
    assert!(
        !app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "…and it is honestly no longer TENDED — the retention bar is deleted, so a position below \
         the rung's top is not that rung (`docs/plan_standing_upkeep.md` §2.8). What made that a \
         cliff is gone too: the payout and the keeping both interpolate on the position"
    );

    // **And the pool really is what is holding it** — unstaff the role and the same meter slides.
    set_maintain_workers(&mut app, band, NO_CREW_ON_THIS_ACTIVITY);
    run_turns_with_forage(&mut app, TURNS);
    assert!(
        progress_of(&app, coord) < eroded - 1e-4,
        "…and with the role empty it loses ground: {eroded} -> {}",
        progress_of(&app, coord)
    );
}

/// **A LONE BUILDER ON A DEAR RUNG STILL BANKS ITS WHOLE TURN.** The maintenance rate used to be
/// netted off a build's accrual, so a crew at or below it never finished — a minimum-viable-crew
/// threshold. §4.6a deleted it: the band's keeping pool owes the rate for every meter carrying work,
/// at any fullness, and a build crew supplies nothing toward it.
///
/// **What a build can still fail to out-run is the ROT** — the ground going backwards under it while
/// the keeping is short — and that is a *countdown* term, asserted on the wire in
/// `build_turns_on_the_wire.rs`. Here the claim is the meter's: with the keeping met, every staffed
/// crew climbs, and the pace is linear in the head count with no threshold anywhere in it.
#[test]
fn every_staffed_build_crew_climbs_when_the_keeping_is_met() {
    /// Past the rung's grace, so the under-staffed arm is genuinely bleeding rather than forgiven.
    const TURNS: u32 = 6;

    let progress_after = |builders: u32| -> (f32, f32) {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        // Half-built, so the meter has room to move in either direction.
        let cost = cultivate_cost(&app);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).expect("patch");
            patch.set_ladder_position(cost / 2.0, &core_sim::LadderConfig::builtin());
            patch.owner = Some(FactionId(0));
        }
        let started = progress_of(&app, coord);
        let band = spawn_forager_of(
            &mut app,
            tile,
            coord,
            Some(Improvement::Cultivate),
            builders,
        );
        set_forage_workers(&mut app, band, builders);
        // **The keeping is MET**, so the meter is not rotting and what is measured is the build's
        // own arithmetic. An unkept arm would be measuring the rot, which is the countdown's claim
        // rather than the meter's.
        set_maintain_workers(&mut app, band, tended_keeping_crew());
        run_turns_with_forage(&mut app, TURNS);
        (started, progress_of(&app, coord))
    };

    let rate_crew = tended_keeping_crew();
    assert!(
        rate_crew >= 2,
        "fixture: the rung must cost more than one hand to hold, or 'a lone builder is under the \
         old threshold' is not a case"
    );

    // **ONE HAND — which the retired threshold said banked nothing at all.**
    let (started, lone) = progress_after(1);
    assert!(
        lone > started,
        "a lone builder banks its whole turn on a rung that costs {rate_crew} hands to hold: \
         {started} -> {lone}"
    );

    // **And it is LINEAR in the head count**, with no threshold subtracted first: twice the hands
    // bank twice the work, which `crew − rate` could not satisfy at two staffings at once.
    let (_, doubled) = progress_after(2);
    assert!(
        ((doubled - started) - (lone - started) * 2.0).abs() < 1e-3,
        "two builders bank twice what one does: {} vs {}",
        doubled - started,
        lone - started
    );
}

/// **THE BUILD PACE IS `cost / crew` — RESTORED** (`docs/plan_standing_upkeep.md` §4.6a). It was the
/// arc's own headline while holding cost nothing, then briefly became `cost / (crew − rate)` while
/// the build crew supplied the maintenance rate. It does not: the keeping pool does, at any meter
/// fullness, so the crew's whole output is progress again.
///
/// Asserted at several crew sizes, so a model still subtracting anything crew-independent would fail
/// — the two agree at exactly one staffing.
#[test]
fn the_build_pace_is_the_cost_over_the_crew() {
    let cost = cultivate_cost(&spawn_world());

    let turns_at = |builders: u32| -> u32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        let band = spawn_forager_of(
            &mut app,
            tile,
            coord,
            Some(Improvement::Cultivate),
            builders,
        );
        // **The keeping is met**, so the pace under measurement is the build's own and not the
        // rot's — an unkept build is slower for a reason this test is not about.
        set_maintain_workers(&mut app, band, tended_keeping_crew());
        let mut turns = 0;
        while !app
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated()
        {
            run_turns_with_forage(&mut app, 1);
            turns += 1;
            assert!(turns < 500, "a staffed build must finish");
        }
        turns
    };

    for builders in [1_u32, 2, 5, 10] {
        let work = builders as f32 * PER_WORKER_OUTPUT;
        assert_eq!(
            turns_at(builders),
            (cost / work).ceil() as u32,
            "{builders} builders bank {work}/turn on a {cost}-unit job"
        );
    }
}

/// **A RUNG THAT ERODES BELOW ITS COST IS STILL TENDED, AND STILL THE KEEPING POOL'S — but a build
/// crew can repair it.**
///
/// Three facts about one patch at 99%, and none of them is any of the others
/// (`docs/plan_standing_upkeep.md` §2.4/§4.6a): *is the rung still achieved* is the **retention bar**
/// and decides what the ground pays out; *is there room on the meter* decides what a repair would
/// cost; and **who pays the rate no longer moves at all** — the pool holds every meter carrying
/// work. The retired third axis is what made a one-percent dip its builders' business at the moment
/// the keeping started mattering.
#[test]
fn a_rung_that_erodes_below_its_cost_is_still_held_and_can_be_repaired() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    seat_tended_patch(&mut app, coord);

    {
        let patch = app
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .clone();
        assert!(
            patch.is_cultivated(),
            "fixture: a freshly completed rung sits exactly at its own cost"
        );
    }

    // One turn's worth of rot, exactly as a short keeping pool would have taken.
    let (_, bleed) = cultivation_config(&app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        registry
            .patch_mut(coord)
            .expect("patch")
            .decay_ladder(bleed, &core_sim::LadderConfig::builtin());
    }

    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert!(
        !patch.is_cultivated(),
        "…and one bleed later there is room on the meter: that shortfall is a repair job"
    );
    assert!(
        !patch.is_cultivated(),
        "…and it is honestly no longer TENDED (§2.8, the bar is deleted) — which is what makes it \
         a repair job rather than a rung nobody can put a builder on"
    );
    assert!(
        core_sim::patch_upkeep_demand(
            &patch,
            &core_sim::LadderConfig::builtin(),
            plant_tile_capacity(&app, coord),
            &app.world.resource::<LaborConfigHandle>().get().forage,
        ) > core_sim::NO_UPKEEP_DEMAND,
        "…and it is still BILLED, so the keeping pool has something to hold — a dip does not move \
         who pays the rate (the holding itself is pinned by \
         `a_half_staffed_keeping_bleeds_at_half_the_rungs_rate`)"
    );

    // **And a build crew can actually repair it**, which is the whole point of calling it building:
    // an accrual guard on the *achieved* state would make erosion a one-way ratchet.
    let eroded = progress_of(&app, coord);
    let builders = build_crew(&app);
    spawn_forager_of(
        &mut app,
        tile,
        coord,
        Some(Improvement::Cultivate),
        builders,
    );
    run_turns_with_forage(&mut app, 1);
    assert!(
        progress_of(&app, coord) > eroded,
        "a Cultivate crew tops a rotted meter back up: {eroded} -> {}",
        progress_of(&app, coord)
    );
}

// ---------------------------------------------------------------------------------------------
// THE BUILD VERB IS DERIVED FROM THE METER (`docs/plan_standing_upkeep.md` §2.4)
// ---------------------------------------------------------------------------------------------

/// **COMPLETE → ERODE → REPAIR → COMPLETE, AND THE REPAIR IS A FRESH DECISION.**
///
/// The whole round trip on one band, and the two halves of the rule that governs it
/// (`docs/plan_standing_upkeep.md` §2.4):
///
/// - **The eroded rung still DERIVES its own verb.** `forage::patch_build_verb` reads a meter below
///   its cost as *building that rung*, so the player never has to work out **which** job a repair
///   is, and a rung that slipped is repairable without re-issuing a verb they never withdrew.
/// - **⛔ BUT NOTHING RE-ADOPTS IT.** Deriving the verb says *what* a repair would be; it does not
///   put the source back in the band's **build queue**. Repairing is a fresh decision the player
///   makes by **re-queueing** — which is the point of the change: a queue funded all-hands-on-the-head
///   would otherwise let a one-percent-eroded Field displace the build the player actually ordered,
///   be topped up, fall back below its cost, and oscillate there forever while the real build stood
///   still.
///
/// So a repair is the same two things any build is — **the declaration and the pool** — and the
/// derivation's job is only to name the rung.
#[test]
fn a_rung_completes_erodes_and_is_repaired_only_by_re_queueing_it() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));

    // The ONE declaration in this test: a wild patch, whose cultivation meter is at zero.
    let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
    // **Kept while it is raised** (§4.6a), so the build finishes in its stated turns rather than
    // fighting the rot — which is a different test's claim.
    set_maintain_workers(&mut app, band, tended_keeping_crew());
    let build_turns = turns_to_prepare(&app);
    run_turns_with_forage(&mut app, build_turns);
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "fixture: the Cultivate must complete, or there is nothing to erode"
    );
    let full = progress_of(&app, coord);

    // **Erode it.** Completion retired the queue entry, and nobody is on the keeping — so the meter
    // slips below its cost with no command involved either.
    set_forage_improvement(&mut app, band, None);
    set_forage_workers(&mut app, band, A_KEEPER);
    // **And take the keepers off**, which is the player's own `assign_labor … agriculture 0`.
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("band exists");
        allocation
            .assignments
            .retain(|a| !matches!(a.target, LaborTarget::Agriculture));
    }
    let grace = tended_grace(&app);
    run_turns_with_forage(&mut app, grace + 2);
    let eroded = progress_of(&app, coord);
    assert!(
        eroded < full,
        "fixture: the rung must actually slip: {full} -> {eroded}"
    );
    assert!(
        !app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "…and it is honestly no longer TENDED — the retention bar is deleted (§2.8), so the state a \
         repair is FOR is simply a position below the rung's top"
    );

    // **(1) THE ERODED RUNG STILL DERIVES ITS OWN VERB** — the meter below its cost names the rung
    // it is short of, so the player never has to work out *which* job a repair is.
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert_eq!(
        core_sim::patch_build_verb(&patch, None),
        Some(Improvement::Cultivate),
        "a meter below its cost declares its own rung — the player names no verb"
    );

    // ⛔ **(2) BUT NOTHING RE-ADOPTS IT** (`docs/plan_standing_upkeep.md` §2.4). Repairing an eroded
    // rung is a **fresh decision**, made by putting the source back in the queue — which is what
    // keeps a one-percent-eroded Field from displacing the build the player actually ordered off
    // the head of a pool funded all-hands-on-one.
    //
    // So a repair is two things, as any build is: the declaration, and the pool.
    let builders = build_crew(&app);
    set_forage_workers(&mut app, band, builders);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("band exists");
        assert!(
            allocation.build_queue.is_empty(),
            "fixture: completion retired the entry and nothing has re-enrolled it"
        );
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Builders,
            workers: builders,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
        assert!(allocation.enqueue_build(
            core_sim::BuildSource::Patch(coord),
            core_sim::BuildJob::Rung(Improvement::Cultivate),
        ));
    }
    // **And the keeping goes back on with them.** Since
    // §4.6a the meter is billed while it is raised, so a repair run with the role empty is racing
    // its own rot and would top out below the cost it is climbing back to.
    set_maintain_workers(&mut app, band, tended_keeping_crew());
    run_turns_with_forage(&mut app, 1);
    assert!(
        progress_of(&app, coord) > eroded,
        "re-queueing it and staffing the pool repairs it: {eroded} -> {}",
        progress_of(&app, coord)
    );

    // …and from there it climbs all the way back to full on the one declaration just made — the
    // derivation carries the rest, so the player states the repair once rather than every turn.
    run_turns_with_forage(&mut app, build_turns + 1);
    assert_eq!(
        progress_of(&app, coord),
        cultivate_cost(&app),
        "the repair completes the rung again"
    );
    assert_eq!(
        core_sim::patch_build_verb(
            &app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .expect("patch")
                .clone(),
            None
        ),
        None,
        "and a full meter declares nothing — the source is maintaining again"
    );
}

/// **A METER AT EXACTLY ZERO NEEDS A DECLARATION AGAIN** — the one state the sim cannot guess,
/// because a wild patch could climb to tended *or* be sown.
///
/// **Per METER, not per source**: a completed tended patch still needs a `sow` declaration, because
/// its field meter is at zero.
#[test]
fn a_meter_at_zero_needs_the_player_to_declare_again() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);

    // (1) A wild patch: both meters at zero, so nothing is implied and either rung is declarable.
    let wild = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert_eq!(core_sim::patch_build_verb(&wild, None), None);
    assert_eq!(
        core_sim::patch_build_verb(&wild, Some(Improvement::Cultivate)),
        Some(Improvement::Cultivate),
        "the player declares, and the sim honours it: a zero meter has no answer of its own"
    );
    // **A DECLARATION MAY NAME A RUNG ABOVE THE ONE IN FLIGHT** (`docs/plan_standing_upkeep.md`
    // §2.8). With one position there is no way to put work on the Field without the tended rung
    // beneath it being whole, so a `sow` on wild ground is a **125-unit** job that climbs the tended
    // range on the way — but the verb stays the player's, because `Sow` is the create-from-nothing
    // rung and deriving `Cultivate` here would dead-end it: Cultivate's gate wants a standing crop
    // and bare ground has none.
    assert_eq!(
        core_sim::patch_build_verb(&wild, Some(Improvement::Sow)),
        Some(Improvement::Sow),
        "…and either rung is theirs to name — this is exactly the state the sim cannot guess"
    );

    // (2) A completed tended patch: the tended meter answers for itself, the FIELD meter is at zero
    // and still needs saying.
    seat_tended_patch(&mut app, coord);
    let tended = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert_eq!(
        core_sim::patch_build_verb(&tended, None),
        None,
        "a full meter is maintaining — it declares nothing"
    );
    assert_eq!(
        core_sim::patch_build_verb(&tended, Some(Improvement::Sow)),
        Some(Improvement::Sow),
        "but the field meter is at zero, so climbing to it is still a declaration"
    );
}

/// **A FULLY FERAL PATCH CLEARS OWNER, SPECIES AND RUNG TOGETHER** — one notion of empty, not three.
///
/// The derivation's "a meter at zero needs a declaration" and `reconcile_owner`'s "nothing is left of
/// either improvement" must agree, or a patch could read as unowned while still implying a verb (or
/// the reverse), and the player would face a source the sim thinks somebody is building.
#[test]
fn a_fully_feral_patch_clears_its_owner_species_and_rung_together() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    seat_tended_patch(&mut app, coord);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch");
        patch.species = Some("wild_emmer".to_string());
    }

    let feral_turns = turns_to_go_fully_feral(&app);
    run_turns_untended(&mut app, feral_turns);

    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert_eq!(
        patch.ladder_position(),
        core_sim::RUNG_UNSTARTED,
        "fixture: the meter must have bled all the way out"
    );
    assert_eq!(patch.owner, None, "ownership lapses with the last progress");
    assert_eq!(patch.species, None, "and so does the committed crop");
    assert_eq!(
        patch.ladder_position(),
        core_sim::RUNG_UNSTARTED,
        "and the position with it — there is no stamped job left to forget"
    );
    assert!(
        !patch.is_cultivated(),
        "the rung is gone: the retention bar cleared on the way down"
    );
    assert_eq!(
        core_sim::patch_build_verb(&patch, None),
        None,
        "…so the ground implies nothing and the player must declare again — one notion of empty"
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

/// **⛔ THE PUBLISHED KEEPING TRIPLE IS INTERNALLY CONSISTENT WHEN TWO BANDS SHARE ONE SOURCE.**
///
/// `snapshot.fbs` states `upkeepDemand − upkeepSupplied == upkeepShortfall` on both the patch and the
/// herd table, and tells the client to do no arithmetic of its own. That identity is a claim about
/// **one position**: the bill is stamped first-write-wins (`ForagePatch::upkeep_demanded`) at the
/// moment the shares are struck, and the build accrual that moves the position runs *later in the
/// same band's iteration*.
///
/// `maintenance_shares` priced each claim off the **live** `patch_upkeep_demand`, so with two bands
/// on one patch — the first building it, the second keeping it — the second band's share was struck
/// after the first band's builders had banked their turn. The wire then published the first band's
/// stamp against the second band's larger payment, `demand − supplied` went **negative** while
/// `shortfall` clamped at `0`, and the keeping pool spent work the patch never owed.
///
/// **The turn a build banks its FIRST work is the sharpest form of it**: bare ground owes nothing, so
/// the stamp is an honest `0`, and any positive supply at all breaks the identity. The fixture is
/// therefore a wild patch, a builder band with **no** keepers, and a keeper band with no build —
/// asserted on the **published rows**, because the identity is a statement about the artifact.
#[test]
fn two_bands_on_one_patch_publish_a_consistent_keeping_triple() {
    /// Keepers enough to cover any bill this ground could present, so a share struck off the wrong
    /// demand is paid in full rather than clipped by a thin pool.
    const AMPLE_KEEPERS: u32 = 4;
    /// The keeping band's take crew — it works the patch, which is what puts it in the loop at all.
    const A_GATHERER: u32 = 1;

    // **The full app, not this file's `spawn_world`** — the identity under test is a property of
    // the *published* row, so the fixture needs the capture the rest of the file does without.
    let mut app = core_sim::build_test_app();
    app.update();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));

    // Band A **builds** and keeps nothing: its own `agriculture` role is empty, so every unit of
    // supply this patch receives comes from band B and the two cannot be confused.
    let crew = build_crew(&app);
    spawn_forager_of(&mut app, tile, coord, Some(Improvement::Cultivate), crew);
    // Band B **keeps** and builds nothing. Spawned second, so it is the band whose share is struck
    // after band A's builders have banked their turn — the whole mechanism.
    let keeper = spawn_forager_of(&mut app, tile, coord, None, A_GATHERER);
    set_maintain_workers(&mut app, keeper, AMPLE_KEEPERS);

    run_turns_with_forage(&mut app, 1);

    // **THE PRECONDITION** — band A really did bank work this turn, so the demand really did move
    // under band B. Without it the two readings coincide and the test passes on a dead fixture.
    let banked = progress_of(&app, coord);
    assert!(
        banked > core_sim::RUNG_UNSTARTED,
        "fixture: the builder must have banked work this turn, or the demand never moved under the \
         keeper ({banked})"
    );

    app.world
        .run_system_once(core_sim::recapture_snapshot_in_place);
    let row = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .last_snapshot()
        .expect("a capture")
        .forage_patches
        .iter()
        .find(|patch| patch.x == coord.x && patch.y == coord.y)
        .expect("the fixture patch is on the wire")
        .clone();

    assert!(
        (row.upkeep_demand - row.upkeep_supplied - row.upkeep_shortfall).abs() < 1e-4,
        "the wire states `demand − supplied == shortfall`, and a keeper paying a bill struck at a \
         later position breaks it: demand {} supplied {} shortfall {}",
        row.upkeep_demand,
        row.upkeep_supplied,
        row.upkeep_shortfall
    );
    assert!(
        row.upkeep_supplied <= row.upkeep_demand + 1e-4,
        "…and the pool never spends work the source did not owe: supplied {} against a bill of {}",
        row.upkeep_supplied,
        row.upkeep_demand
    );
}

/// **⛔ AN OFF-MAP PATCH IS SYNTHETIC GROUND, NOT BARREN GROUND — and it bleeds like any other.**
///
/// `advance_forage_regrowth` deliberately leaves a patch's `carrying_capacity` alone when its coord
/// is absent from the `TileRegistry`, *"which is what lets test harnesses build synthetic patches on
/// tiles that do not exist"*. `advance_cultivation` resolved the same absence to `NO_FORAGE_CAPACITY`
/// — while its comment claimed it looked the tile up *"exactly as `advance_forage_regrowth` looks it
/// up"* — so such a patch presented no tender-load, owed **nothing**, was never short, and kept a
/// finished Cultivate for ever with nobody on it. One absence, read two ways, in two passes of the
/// same stage.
///
/// Both arms are asserted. The on-map patch is the control: it must revert, or *"the off-map one
/// reverts too"* would be a claim about a pass that does nothing.
#[test]
fn an_off_map_patch_owes_its_keeping_and_reverts_like_any_other() {
    /// A coord well outside the harness map — the synthetic ground the regrowth pass supports.
    const OFF_THE_MAP: UVec2 = UVec2::new(9_000, 9_000);

    /// Seat a finished, unkept Cultivate and let the decay pass run past its grace. Returns
    /// `(seated, left)` work units. `off_map` puts the patch on a coord the `TileRegistry` does not
    /// carry, seeded with the same capacity the on-map ground presents.
    fn revert(off_map: bool) -> (f32, f32) {
        let mut app = spawn_world();
        let (_tile, on_map) = prime_thriving_patch(&mut app);
        let coord = if off_map { OFF_THE_MAP } else { on_map };
        if off_map {
            let capacity = plant_tile_capacity(&app, on_map);
            let mut patch = core_sim::ForagePatch::new(coord, capacity);
            patch.biomass = capacity;
            app.world
                .resource_mut::<ForageRegistry>()
                .patches
                .insert(coord, patch);
        }
        seat_tended_patch(&mut app, coord);
        let seated = progress_of(&app, coord);
        let turns = tended_grace(&app) + 2;
        run_turns_untended(&mut app, turns);
        (seated, progress_of(&app, coord))
    }

    let (control_seated, control_left) = revert(false);
    assert!(
        control_left < control_seated,
        "control: an unkept ON-MAP patch must revert ({control_seated} -> {control_left}), or this \
         test measures a pass that does nothing"
    );

    let (seated, left) = revert(true);
    assert!(
        seated > core_sim::RUNG_UNSTARTED,
        "fixture: the synthetic patch must start with a finished rung on it ({seated})"
    );
    assert!(
        left < seated,
        "a patch whose coord is off the map owes its keeping off the capacity it was SEEDED with — \
         reading that absence as barren left it holding a finished rung for ever with nobody on it \
         ({seated} -> {left})"
    );
}
