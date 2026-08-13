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
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle,
    CULTIVATION_DISCOVERY_ID, FOOD, PER_WORKER_OUTPUT, RUNG_COST_UNSCALED, UNSCALED_UPKEEP,
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
        // payoff functions, so "the crop wins" is the sim's verdict, not a re-derivation. On the
        // standard map this lands on rich river-lowland (AlluvialPlain ~1.35×).
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .filter(|(tile, _)| registry.patch(tile.position).is_some())
            .find(|(tile, _)| {
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

/// Check or clear the **improvement** on a band's (single) Forage assignment — what the client's
/// checkbox does. Since issue #442 this touches only the improvement slot: the stance and the crew
/// stay put, and completion clears the box itself.
fn set_forage_improvement(
    app: &mut App,
    band: bevy::prelude::Entity,
    improvement: Option<Improvement>,
) {
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("band forages");
    allocation
        .assignments
        .first_mut()
        .expect("a Forage assignment")
        .improvement = improvement;
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
fn spawn_builder(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    improvement: Improvement,
) -> bevy::prelude::Entity {
    let crew = build_crew(app);
    spawn_forager_of(app, tile, patch, Some(improvement), crew)
}

/// **One keeper** — what either plant rung's sub-worker demand rounds up to
/// (`the_upkeep_crew_needed_is_the_demand_in_whole_workers`), and therefore the whole cost of holding
/// a completed improvement.
///
/// **The build fixtures deliberately staff NONE.** A meter still being raised is owed its *builders*,
/// not its keepers (`docs/plan_standing_upkeep.md` §2.4), so a Cultivate runs at its stated pace with
/// nobody on the keeping — and the completion hand-off then moves the builders onto it, so a band
/// never has to think about the transition at all. That is what
/// `the_reference_crew_finishes_a_cultivate_in_its_stated_turns_with_no_keeper` pins.
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
                working: scalar_from_f32((foragers + foragers) as f32),
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
                    target: LaborTarget::Forage {
                        tile: patch,
                        floor: policy,
                        species: None,
                    },
                    workers: foragers,
                    improvement,
                    kit: None,
                    // **The same crew staffs the build** — what this fixture meant when one
                    // crew did every job (`docs/plan_standing_upkeep.md` §2.2).
                    improvement_workers: improvement.map_or(NO_CREW_ON_THIS_ACTIVITY, |_| foragers),
                    // **NO KEEPER, and that is the point.** A meter still being raised is owed its
                    // BUILDERS (`docs/plan_standing_upkeep.md` §2.4), so these fixtures measure a
                    // build's stated pace with nobody on the keeping — and once it completes, the
                    // hand-off moves the build's crew onto the keeping by itself.
                }],
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
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch");
    patch.upkeep_supplied = core_sim::patch_upkeep_demand(patch, &ladder);
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
        .map(|p| p.cultivation_progress)
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
            UNSCALED_UPKEEP
        ) > 0.0,
        "the build fixtures must staff a crew ABOVE the maintenance rate, or every pace assertion \
         below compares nothing to nothing"
    );
    (
        // The crew IS the throughput now (`docs/plan_unit_costed_work.md` §1.2), so this reads at
        // the head count the build fixtures actually staff — computing it at any other would
        // describe a build nobody here is running.
        tended.build_accrual(
            Some(Improvement::Cultivate),
            true,
            build_crew(app),
            UNSCALED_UPKEEP,
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
/// # AND IT IS TWO HANDS **ABOVE THE MAINTENANCE RATE**
///
/// The rate is a tax on building (§2.4): it is owed while the meter is being raised, and below the
/// meter's cost the *build crew* is what supplies it. A bare two hands is **exactly** `plant:tended`'s
/// rate, so it would net zero — the meter would hold, forever, and every pace assertion below would
/// pass **vacuously** by comparing nothing to nothing. Staffing above the rate keeps the fixture's
/// *net* at the two worker-turns these tests were written against.
fn build_crew(app: &App) -> u32 {
    /// The net supply the build fixtures are paced against — the staffing the shipped `work_cost`
    /// was priced at.
    const NET_WORKER_TURNS: u32 = 2;
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(UNSCALED_UPKEEP)
        .saturating_add(NET_WORKER_TURNS)
}

/// **THE CREW THAT EXACTLY MEETS `plant:tended`'S MAINTENANCE RATE** — the minimum-viable-build
/// threshold. At or below it a build banks nothing at all and has no finish date; every net figure
/// in this file is measured above it (`docs/plan_standing_upkeep.md` §2.4).
fn tended_rate_crew() -> u32 {
    core_sim::LadderConfig::builtin()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(UNSCALED_UPKEEP)
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
    let erodable = cost - rung.retention_bar(cost);
    let bleed = unmaintained_bleed(rung);
    // The rung is lost the turn the meter falls **below** the bar, so eroding exactly the erodable
    // amount still holds it — hence `floor + 1` rather than `ceil`.
    rung.upkeep_grace_turns() + (erodable / bleed).floor() as u32 + 1
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
    // The tile's own basket, resolved the way the sim does: a patch under construction has weeded
    // nothing, so it must still read the whole thing and convert at its plain average.
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    let ground = app.world.get::<Tile>(tile_entity).expect("the tile");
    let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    assert_eq!(
        core_sim::patch_composition(&patch, &composition, &labor.forage).as_ref(),
        composition.as_ref(),
        "and it is still the mixed basket it started as"
    );
    let wild = core_sim::ForagePatch::new(coord, patch.carrying_capacity);
    assert_eq!(
        core_sim::patch_provisions_per_biomass(&patch, &composition, &flora, &labor.forage),
        core_sim::patch_provisions_per_biomass(&wild, &composition, &flora, &labor.forage),
        "and it still converts at the wild basket average"
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
            patch.cultivation_progress
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
    // progress already banked is *held* (the crew is still there, so the decay pass spares it).
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
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
        patch.complete_cultivation(FactionId(0));
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
        patch.complete_cultivation(FactionId(0));
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
        patch.cultivation_progress,
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

    run_turns_with_forage(&mut app, 5);
    let banked = progress_of(&app, coord);
    assert!(
        banked > 0.0 && banked < cultivate_cost(&app),
        "part-prepared: {banked} work units of {}",
        cultivate_cost(&app)
    );

    // The `tended_this_turn` flag is a deliberate one-turn-lag signal (Logistics runs before
    // Population), so the first Logistics pass after the band leaves still sees the flag set from its
    // last worked turn and spares the patch. Decay bites from the turn after that.
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
        let declared = app.world.get::<LaborAllocation>(band).unwrap().assignments[0].improvement;
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
    let retain_bar = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .retention_bar(cost);
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch");
    patch.cultivation_progress = cost;
    patch.cultivation_cost = cost;
    patch.cultivation_retain_bar = retain_bar;
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
        .upkeep_crew_needed(UNSCALED_UPKEEP);
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

/// **MEETING THE DEMAND EXACTLY COSTS THE METER NOTHING, and going short costs it the rung's own
/// rate scaled by how short** — the property the retired binary flag could not express, and the
/// reason the standing cost is a *rate*. Under the flag a crew of one on a source wanting two
/// counted as fully worked, so under-crewing cost precisely nothing until it reached zero.
///
/// **Half-staffing is reachable on the SHIPPED ladder now.** Both plant demands used to sit under a
/// single worker-turn, so a fixture ladder was the only way to observe a half; the retune made them
/// whole numbers a player can staff exactly, which is most of what it was for.
///
/// **Measured over exactly ONE bleeding turn, and that bound is the model rather than convenience.**
/// The keeping pool supplies a source only while its meter is **full** — the instant it dips the
/// source is *building* again and is owed **builders** (`forage::patch_is_maintaining`,
/// `docs/plan_standing_upkeep.md` §2.4). So the pool's job is to prevent the first dip; a second
/// bleeding turn would be measuring a patch the keepers can no longer reach, which the follow-on
/// assertion pins in its own right.
#[test]
fn a_half_staffed_keeping_bleeds_at_half_the_rungs_rate() {
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

    let demand_in_hands = app_free()
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(UNSCALED_UPKEEP);
    assert!(
        demand_in_hands >= 2 && demand_in_hands.is_multiple_of(2),
        "fixture: the shipped demand must divide evenly, or there is no exact half to staff"
    );
    let (_, bleed) = cultivation_config(&app_free());
    let bleeding_turns = TURNS - tended_grace(&app_free());

    let unkept = lost_with(NO_CREW_ON_THIS_ACTIVITY);
    let half_kept = lost_with(demand_in_hands / 2);
    let fully_kept = lost_with(demand_in_hands);

    assert_eq!(
        bleeding_turns, 1,
        "fixture: exactly one bleeding turn — see the doc above"
    );
    assert!(
        (unkept - bleed * bleeding_turns as f32).abs() < 1e-4,
        "an unkept patch bleeds the rung's own rate every bleeding turn, got {unkept}"
    );
    assert!(
        (unkept - half_kept * 2.0).abs() < 1e-4,
        "half the hands must be half the bleed: {unkept} unkept vs {half_kept} half-kept"
    );
    assert_eq!(
        fully_kept, 0.0,
        "and meeting the demand exactly costs the meter nothing"
    );

    // **AND ONCE IT HAS DIPPED, THE POOL CANNOT REACH IT.** A patch below its meter's cost is
    // *building*, so it is owed **builders** — the keepers' hands go to the band's other, still-full
    // sources. This is the sharp edge of "one state test, two costs": the keeping's whole job is to
    // prevent the first dip, and recovering from one needs a `Cultivate` crew.
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    seat_tended_patch(&mut app, coord);
    let band = spawn_forager(&mut app, tile, coord, None);
    set_maintain_workers(&mut app, band, demand_in_hands);
    // Nudge the meter under its cost, exactly as one short turn would have.
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch");
        patch.cultivation_progress -= bleed;
    }
    let dipped = progress_of(&app, coord);
    run_turns_with_forage(&mut app, TURNS);
    assert!(
        progress_of(&app, coord) < dipped,
        "a dipped patch is BUILDING: a full keeping pool no longer holds it, because its hands are \
         the build's — {dipped} to {}",
        progress_of(&app, coord)
    );
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "…and it is still TENDED while that happens: the state test and the retention bar are \
         orthogonal axes"
    );
}

/// **A BUILD IS NOT A KEEPING — NOBODY MAINTAINS GROUND THAT IS STILL BEING CLEARED.**
///
/// A meter still being raised is owed its **builders**; only a *finished* rung is owed keepers
/// (`docs/plan_standing_upkeep.md` §2.4, `forage::patch_upkeep_supply`). So a Cultivate runs at its
/// stated pace — `work_cost / crew` — with **no keeper at all**, and staffing one changes nothing
/// about the build.
///
/// This exists because the arc briefly got it wrong in the other direction: resolving a mid-build
/// meter's demand as a *maintain* demand billed a crew to hold a tended patch that did not exist
/// yet, and turned the reference 25-turn Cultivate into 34. Asserted against the ladder's own
/// arithmetic rather than a literal, so a retune of `work_cost` moves the expectation with the game.
#[test]
fn the_reference_crew_finishes_a_cultivate_in_its_stated_turns_with_no_keeper() {
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

    assert_eq!(
        finished_on(NO_CREW_ON_THIS_ACTIVITY),
        stated,
        "a build with no keeper finishes in exactly `work_cost / crew` turns — nothing is bleeding \
         off a meter its own builders are filling"
    );
    assert_eq!(
        finished_on(A_KEEPER),
        stated,
        "…and a keeper standing beside it changes nothing, because the meter is not owed one yet"
    );
}

/// **AN ABANDONED PART-BUILD STILL BLEEDS, on its own terms.** The rule is *"a meter bleeds when the
/// hands it needs are not on it"*, so walking away from a half-cleared patch costs exactly what
/// walking away from a finished one does — the rung's own `upkeep.work_per_turn` — and the cleared
/// ground grows back over. This is the constraint that rules out the simpler *"only completed rungs
/// cost anything"*, under which an abandoned investment would sit there untouched forever.
///
/// The system-level pace is pinned by [`abandoned_preparation_decays`]; this pins the **rate** is the
/// rung's, and that the patch is owed *builders* rather than keepers while it is unfinished.
#[test]
fn an_abandoned_part_build_is_owed_its_builders_and_bleeds_the_rungs_rate() {
    let ladder = core_sim::LadderConfig::builtin();
    let rung = ladder.rung(RungKey::PlantTended);
    let cost = rung
        .build_cost(RUNG_COST_UNSCALED)
        .expect("the tended rung builds");
    let demand = rung.upkeep_demand(UNSCALED_UPKEEP);

    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("patch");
        patch.cultivation_progress = cost / 2.0;
        patch.cultivation_cost = cost;
        patch.owner = Some(FactionId(0));
    }
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert!(
        !core_sim::patch_is_maintaining(&patch),
        "fixture: a half-filled meter is BUILDING, not maintaining"
    );
    // **THE SAME RATE EITHER WAY** — the maintenance rate never lapses; below the meter's cost it is
    // the *build crew* that supplies it (`docs/plan_standing_upkeep.md` §2.4). So the published
    // `workers_needed` is the same count on both sides of completion, and mid-build it reads as the
    // **minimum viable build crew**: at or below it the meter holds or rots rather than advancing.
    assert_eq!(
        core_sim::patch_upkeep_workers_needed(&patch, &ladder),
        demand.ceil() as u32,
        "hands to meet the demand, whoever is supplying it"
    );
    // Nobody is building it and nobody is keeping it, so the whole rate goes unmet.
    assert!(
        (core_sim::patch_upkeep_shortfall(&patch, &ladder) - demand).abs() < 1e-6,
        "an abandoned part-build is short by the whole rate"
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

/// Put `workers` on a band's **agriculture role** — the fixture's stand-in for
/// `assign_labor <faction> <band> agriculture <workers>`. Since maintenance left the tile
/// (`docs/plan_standing_upkeep.md` §2.5) the keeping is one band-level pool, spread across every
/// plant source the band works, so a fixture staffs the role rather than the patch.
fn set_maintain_workers(app: &mut App, band: bevy::prelude::Entity, workers: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("band exists")
        .add_role_workers(LaborTarget::Agriculture, workers);
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
        let demand = rung.upkeep_demand(UNSCALED_UPKEEP);
        let cost = rung
            .build_cost(RUNG_COST_UNSCALED)
            .expect("both plant rungs build");
        println!(
            "  {label}: {demand:.2} work/turn -> {} keeper(s); grace {} turns; \
             the rung is LOST on the first bleeding turn (progress {cost} -> {:.2}, below its own \
             cost), and the ground is fully wild again after {:.0} bleeding turns",
            rung.upkeep_crew_needed(UNSCALED_UPKEEP),
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
    assert!(
        survives > grace + 1,
        "fixture: the rung must outlast its own first bleeding turn, or this test cannot tell the \
         new edge from the old one"
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

    // **Measured in NET hands** — above the maintenance rate, which the build crew is also paying
    // (`docs/plan_standing_upkeep.md` §2.4). A crew at or below the rate banks nothing at all, which
    // `a_build_crew_at_the_maintenance_rate_holds_the_meter_where_it_is` pins on its own.
    let rate_crew = app_free()
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(UNSCALED_UPKEEP);
    let one = progress_after(rate_crew + 1);
    let full = progress_after(crew);
    let over = progress_after(rate_crew + (crew - rate_crew) * OVER_CREWED);
    assert!(
        (one - PER_WORKER_OUTPUT).abs() < 1e-6,
        "one worker ABOVE the rate banks one worker-turn: {one}"
    );
    assert!(
        (full - (crew - rate_crew) as f32 * PER_WORKER_OUTPUT).abs() < 1e-6,
        "a crew of {crew} banks its {} net worker-turns: {full}",
        crew - rate_crew
    );
    assert!(
        (over - ((crew - rate_crew) * OVER_CREWED) as f32 * PER_WORKER_OUTPUT).abs() < 1e-6,
        "and {OVER_CREWED}x the NET crew banks {OVER_CREWED}x the work — there is no cap: {over}"
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
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .build_turns_remaining
    };

    let crew = build_crew(&spawn_world());
    let rate_crew = tended_rate_crew();
    // **AT OR BELOW THE MAINTENANCE RATE THERE IS NO ESTIMATE**, and that is the honest answer
    // rather than a huge number: the crew holds the meter where it is or takes it backwards, so the
    // job has no finish date at all (`docs/plan_standing_upkeep.md` §2.4).
    assert_eq!(
        estimate(rate_crew),
        None,
        "a crew exactly at the rate is quoted no finish date — it will never arrive"
    );
    let lightly = estimate(rate_crew + 1).expect("a running build quotes a finish date");
    let fully = estimate(crew).expect("a running build quotes a finish date");
    assert!(
        fully < lightly,
        "adding hands shortens the same fixed job: {fully} at a crew of {crew} vs {lightly} at one \
         hand above the rate"
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
        // No improvement in flight: this crew is gathering, and deciding.
        spawn_forager_of(&mut app, tile, coord, None, workers);
        run_turns_with_forage(&mut app, 1);
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .build_turns_remaining
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

    // **Twice the NET, half the turns.** Doubling the head count does not halve the job any more —
    // the maintenance rate comes off the top first (`docs/plan_standing_upkeep.md` §2.4).
    let rate_crew = tended_rate_crew();
    let doubled_net = rate_crew + (crew - rate_crew) * 2;
    let doubled = projection(doubled_net).expect("a bigger crew is still quotable");
    assert_eq!(
        doubled,
        quoted.div_ceil(2),
        "twice the NET supply, half the turns: {quoted} at a crew of {crew}, {doubled} at \
         {doubled_net}"
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
    let mut app = core_sim::build_headless_app();
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
            patch.owner.is_none() && patch.cultivation_progress == core_sim::RUNG_UNSTARTED
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
/// The two answers are held far apart on purpose — a builder of [`build_crew`] hands against a lone
/// gatherer — so the assertion cannot pass by coincidence, and the gatherer is spawned **second** so
/// it is the one whose write would win.
#[test]
fn a_running_build_outranks_a_bystanders_projection_on_the_same_patch() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let crew = build_crew(&app);
    spawn_forager_of(&mut app, tile, coord, Some(Improvement::Cultivate), crew);
    // The bystander: same patch, no verb, and a crew **one hand above the maintenance rate** — so
    // its projection of the very same rung is far longer than the builder's countdown, while still
    // being a projection at all (at or below the rate there is no finish date to quote).
    let one_bystander = tended_rate_crew() + 1;
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
            rung.build_accrual(Some(Improvement::Cultivate), true, workers, UNSCALED_UPKEEP),
        )
        .expect("a staffed build quotes a finish date")
    };
    let builders_answer = quote(crew, patch.cultivation_progress);
    let bystanders_answer = quote(one_bystander, patch.cultivation_progress);
    assert!(
        bystanders_answer > builders_answer,
        "fixture: the two crews must disagree, or last-writer-wins is invisible \
         ({bystanders_answer} vs {builders_answer})"
    );
    assert_eq!(
        published, builders_answer,
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
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .build_turns_remaining
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
    let retain_bar = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .retention_bar(cost);
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch");
    patch.cultivation_progress = cost;
    patch.cultivation_cost = cost;
    patch.cultivation_retain_bar = retain_bar;
    patch.owner = Some(FactionId(0));
    patch.neglect_turns = 0;
    coord
}

/// One band working **both** patches, with `keepers` on its `agriculture` role — the pool the two
/// tended patches draw on.
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
        },
        workers: GATHERERS,
        improvement: None,
        kit: None,
        improvement_workers: NO_CREW_ON_THIS_ACTIVITY,
    });
    allocation.add_role_workers(LaborTarget::Agriculture, keepers);
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
    /// Half of what two tended patches want (2 work each on the shipped ladder), so the pool is
    /// genuinely short and the two modes must answer differently.
    const KEEPERS: u32 = 2;

    let run = |mode: core_sim::UpkeepFundMode| -> (f32, f32, f32) {
        let mut app = spawn_world();
        let (tile, first) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, first);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(first).expect("patch");
            let bar = patch.cultivation_retain_bar / patch.cultivation_cost;
            patch.cultivation_progress = RICH_COST;
            patch.cultivation_cost = RICH_COST;
            patch.cultivation_retain_bar = RICH_COST * bar;
        }
        let second = seat_second_tended_patch(&mut app, first, POOR_COST);
        spawn_band_keeping_two_patches(&mut app, tile, first, second, KEEPERS, mode);
        app.world.run_system_once(advance_labor_allocation);
        let (rich, poor) = supplied_on(&app, first, second);
        (rich, poor, core_sim::activity_work(KEEPERS))
    };

    let (rich, poor, pool) = run(core_sim::UpkeepFundMode::Spread);
    assert!(
        (rich - poor).abs() < 1e-5,
        "spread funds two equal demands equally, whatever they cost: {rich} vs {poor}"
    );
    assert!(
        (rich + poor - pool).abs() < 1e-5,
        "and it spends the whole pool — a pool has no leftover: {rich} + {poor} against {pool}"
    );

    let (rich, poor, pool) = run(core_sim::UpkeepFundMode::Priority);
    assert!(
        (rich - pool).abs() < 1e-5,
        "priority funds the most-invested source completely first: {rich} of {pool}"
    );
    assert!(
        poor.abs() < 1e-5,
        "…and the marginal one rots — that is what the mode is for, got {poor}"
    );
    assert!(
        (rich + poor - pool).abs() < 1e-5,
        "priority spends the whole pool too: {rich} + {poor} against {pool}"
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
        let mut app = core_sim::build_headless_app();
        app.update();
        let (tile, first) = prime_thriving_patch(&mut app);
        seat_tended_patch(&mut app, first);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(first).expect("patch");
            let bar = patch.cultivation_retain_bar / patch.cultivation_cost;
            patch.cultivation_progress = RICH_COST;
            patch.cultivation_cost = RICH_COST;
            patch.cultivation_retain_bar = RICH_COST * bar;
        }
        let second = seat_second_tended_patch(&mut app, first, POOR_COST);
        let band = spawn_band_keeping_two_patches(&mut app, tile, first, second, KEEPERS, mode);
        // **A checkpoint keys a band by its `BandId`**, so a fixture band without one is not
        // captured at all — and the restored world would then be measured with no band on it, which
        // passes a naive equality against stale scratch.
        app.world
            .entity_mut(band)
            .insert((FIXTURE_BAND_ID, core_sim::ResidentBand));

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

/// **THE REPORTED BUG, DIRECTLY: a completed rung is not lost the instant its meter dips.**
///
/// A completed meter sits **exactly** at its own cost, so a `progress >= cost` predicate made the
/// first bleed of any size revoke the rung — finish a Cultivate and the patch could be out of
/// *tended* before its keepers were assigned. No grace and no rate could fix it, because the loss
/// was a **threshold test rather than a rate** (`docs/plan_standing_upkeep.md` §2.4).
///
/// The rung is *earned* at `progress >= cost` and *held* down to a stated fraction of that cost, so
/// a wholly unmaintained patch now keeps its rung for most of a season while the meter erodes.
/// **Both the survival and the eventual loss are asserted**, because "it never reverts" is the other
/// way to break this and would pass a survival-only test.
#[test]
fn a_completed_tended_patch_survives_many_unmaintained_turns_before_it_is_lost() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    seat_tended_patch(&mut app, coord);

    let survives = unmaintained_turns_before_the_rung_is_lost(&app);
    let grace = tended_grace(&app);
    assert!(
        survives > grace + 1,
        "the whole point: the rung must outlast its own FIRST bleeding turn, which is all the old \
         predicate gave it ({survives} turns against a grace of {grace})"
    );

    run_turns_untended(&mut app, survives - 1);
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert!(
        patch.is_cultivated(),
        "a tended patch stays tended while its meter erodes — {} of {}",
        patch.cultivation_progress,
        patch.cultivation_cost
    );
    assert!(
        patch.cultivation_progress < patch.cultivation_cost,
        "…and the meter really is eroding, or this test is asserting nothing"
    );

    run_turns_untended(&mut app, 1);
    assert!(
        !app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "and it IS lost, at the retention bar — the rung is held, not immortal"
    );
}

// ---------------------------------------------------------------------------------------------
// THE MAINTENANCE RATE IS A TAX ON BUILDING (`docs/plan_standing_upkeep.md` §2.4)
// ---------------------------------------------------------------------------------------------

/// **A CREW EXACTLY AT THE MAINTENANCE RATE HOLDS THE METER WHERE IT IS — one below it ROTS.**
///
/// The rate is owed **always**, while building and while held alike; what the meter's state decides
/// is only *who supplies it*. Below its cost that is the **build crew**, so:
///
/// ```text
/// net > 0  →  the surplus is BUILD PROGRESS
/// net = 0  →  the meter HOLDS exactly where it is
/// net < 0  →  it ROTS, in proportion to the shortfall
/// ```
///
/// **This is a real minimum-viable-crew threshold rather than a slow build**, which is why it is
/// asserted at all three points: a model that merely slowed down would show a positive, shrinking
/// gain at the threshold instead of a flat line, and going backwards below it.
#[test]
fn a_build_crew_at_the_maintenance_rate_holds_the_meter_where_it_is() {
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
            patch.cultivation_progress = cost / 2.0;
            patch.cultivation_cost = cost;
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
        run_turns_with_forage(&mut app, TURNS);
        (started, progress_of(&app, coord))
    };

    let rate_crew = tended_rate_crew();
    assert!(
        rate_crew >= 2,
        "fixture: the rate must want more than one hand, or 'below it' is unreachable"
    );

    let (started, held) = progress_after(rate_crew);
    assert!(
        (held - started).abs() < 1e-4,
        "a crew exactly at the rate holds the meter EXACTLY where it is: {started} -> {held}"
    );

    let (started, rotted) = progress_after(rate_crew - 1);
    assert!(
        rotted < started,
        "one hand below the rate and the meter goes BACKWARDS: {started} -> {rotted}"
    );

    let (started, built) = progress_after(rate_crew + 1);
    assert!(
        built > started,
        "one hand above it and the surplus is progress: {started} -> {built}"
    );
}

/// **THE BUILD PACE IS `cost / NET`, NOT `cost / crew`** — the identity this arc deliberately
/// changed. `work_cost / crew` was the arc's own headline while holding cost nothing; the
/// maintenance rate is a tax on building, so the crew's *surplus* over it is what banks.
///
/// Asserted at several crew sizes so a model that merely subtracted a constant number of *turns*
/// (rather than taxing the rate) would fail: the two agree at exactly one staffing.
#[test]
fn the_build_pace_is_the_cost_over_the_net_supply() {
    let rate_crew = tended_rate_crew();
    let cost = cultivate_cost(&spawn_world());

    let turns_at = |builders: u32| -> u32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        spawn_forager_of(
            &mut app,
            tile,
            coord,
            Some(Improvement::Cultivate),
            builders,
        );
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
            assert!(turns < 500, "a build above the rate must finish");
        }
        turns
    };

    for over in [1_u32, 2, 5, 10] {
        let builders = rate_crew + over;
        let net = over as f32 * PER_WORKER_OUTPUT;
        assert_eq!(
            turns_at(builders),
            (cost / net).ceil() as u32,
            "{builders} builders net {net}/turn on a {cost}-unit job"
        );
    }
}

/// **A RUNG THAT ERODES BELOW ITS COST IS BUILDING AGAIN — and it is still TENDED while it is.**
///
/// The two axes are orthogonal and must stay so (`docs/plan_standing_upkeep.md` §2.4): *building vs
/// maintaining* is the meter's **fullness** and decides who pays the rate; *is the rung still
/// achieved* is the **retention bar** and decides what the ground pays out. A patch at 99% is a
/// repair job on a tended patch — folding the two would make a rung's loss and a rung's repair the
/// same edge.
#[test]
fn a_rung_that_erodes_below_its_cost_is_building_again() {
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
            core_sim::patch_is_maintaining(&patch),
            "fixture: a freshly completed rung is MAINTAINING — its meter is full"
        );
    }

    // One turn's worth of rot, exactly as a short keeping pool would have taken.
    let (_, bleed) = cultivation_config(&app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        registry
            .patch_mut(coord)
            .expect("patch")
            .cultivation_progress -= bleed;
    }

    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert!(
        !core_sim::patch_is_maintaining(&patch),
        "…and one bleed later it is BUILDING: that shortfall is a repair job"
    );
    assert!(
        patch.is_cultivated(),
        "…while remaining TENDED — the retention bar is a separate axis and is nowhere near"
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

/// **COMPLETE → ERODE → REPAIR → COMPLETE, WITH NO COMMAND ISSUED AT ANY POINT.**
///
/// `RungDef::build_accrual` banks nothing unless the rung's verb is in flight, and completion frees
/// the declaration — so before the derivation a rung that slipped could not be repaired until the
/// player re-issued `cultivate`. They never withdrew that intent. **A meter carrying progress IS the
/// declaration**, so what a player owes a slipping rung is *hands*, not a command.
///
/// The whole round trip runs here on one band, issuing the verb exactly **once** at the very start.
#[test]
fn a_rung_completes_erodes_and_repairs_with_no_command_after_the_first() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));

    // The ONE declaration in this test: a wild patch, whose cultivation meter is at zero.
    let band = spawn_builder(&mut app, tile, coord, Improvement::Cultivate);
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

    // **Erode it.** The builders were freed at completion, and nobody is on the keeping — so the
    // meter slips below its cost with no command involved either.
    set_forage_improvement(&mut app, band, None);
    set_forage_workers(&mut app, band, A_KEEPER);
    // **And take the keepers off.** The completion hand-off moved the builders onto the band's
    // `agriculture` role, which is exactly what stops a fresh rung decaying — so a test about
    // erosion has to unstaff it, which is the player's own `assign_labor … agriculture 0`.
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
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("patch")
            .is_cultivated(),
        "…while staying tended, which is the state a repair is FOR"
    );

    // **THE DERIVED VERB IS BACK, with no command.**
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("patch")
        .clone();
    assert_eq!(
        core_sim::patch_build_verb(&patch, None),
        Some(Improvement::Cultivate),
        "a meter below its cost declares its own rung — the player re-issues nothing"
    );

    // **Adding HANDS is the whole of what the player does.** The declaration slot is left empty on
    // purpose: if the repair needed it, this would not move.
    let builders = build_crew(&app);
    set_forage_workers(&mut app, band, builders);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("band exists");
        assert_eq!(
            allocation.assignments[0].improvement, None,
            "fixture: no declaration is stored — the derivation is doing the work"
        );
        allocation.assignments[0].improvement_workers = builders;
    }
    run_turns_with_forage(&mut app, 1);
    assert!(
        progress_of(&app, coord) > eroded,
        "hands alone repair it: {eroded} -> {}",
        progress_of(&app, coord)
    );

    // …and it climbs all the way back to full, still with nothing re-issued.
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
        patch.cultivation_progress,
        core_sim::RUNG_UNSTARTED,
        "fixture: the meter must have bled all the way out"
    );
    assert_eq!(patch.owner, None, "ownership lapses with the last progress");
    assert_eq!(patch.species, None, "and so does the committed crop");
    assert_eq!(
        patch.cultivation_cost,
        core_sim::RUNG_UNSTARTED,
        "and the stamped job with it — a wild patch quotes no price"
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
