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

use core_sim::{
    advance_cultivation, advance_forage_regrowth, advance_labor_allocation, commit_payoff,
    commit_yield_ratio, default_species_for_rung, scalar_from_f32, scalar_one, scalar_zero,
    spawn_initial_forage, spawn_initial_world, tile_flora_composition, tile_forage_capacity,
    wild_payoff, CommandEventLog, CultureManager, DiscoveryProgressLedger, EcologyPhase, FactionId,
    FactionInventory, FaunaConfigHandle, FoodModuleTag, ForageRegistry, GenerationId,
    GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry, Improvement, LaborAllocation,
    LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, RungKey, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle,
    CULTIVATION_DISCOVERY_ID, FOOD, RUNG_TIMESCALE_UNSCALED,
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
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    (entity, coord)
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
        .neglect_grace_turns()
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

/// The plant rung-2 build dials — the investment dip, the build rate and the feral rate — read off
/// the ladder's `plant:tended` rung (`intensification_ladder.json`), the same seam the sim drives
/// cultivation with.
fn cultivation_config(app: &App) -> (f32, f32, f32) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let tended = ladder.rung(RungKey::PlantTended);
    (
        tended
            .yield_fraction_while_building()
            .expect("the tended rung is an investment"),
        // Staffed to the rung's full crew, so this is the rung's stated rate rather than an
        // under-crewed fraction of it (the build now scales by `min(workers / crew_needed, 1)`).
        tended.build_accrual(
            Some(Improvement::Cultivate),
            true,
            FOOD_PEAK_FLOOR,
            RUNG_TIMESCALE_UNSCALED,
            tended
                .build_crew_needed()
                .expect("the tended rung declares a crew"),
        ),
        tended.build_decay(RUNG_TIMESCALE_UNSCALED),
    )
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

/// **The investment cost.** A crew preparing ground carries `yield_fraction_while_building ×` what
/// the same crew gathering it carries — it is clearing, not gathering — and the reduced take is
/// *sustainable*, so the patch stays Thriving throughout.
///
/// **It is asked at [`SOLE_FORAGER`], not at the ample crew the rest of this file uses.**
/// `docs/plan_harvest_floor.md` §3.1 moved the dip off the take ceiling and onto crew throughput, so
/// a crew big enough to saturate the standing stock anyway pays **nothing** for the build — legibly
/// so, since the remedy is to hire four times the people. Both regimes are asserted, because either
/// alone reads as a bug.
#[test]
fn cultivate_pays_a_fraction_of_the_sustain_yield_and_keeps_the_patch_healthy() {
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
    let (fraction, _, _) = cultivation_config(&app);
    assert!(
        (cultivating_yield - fraction * sustain_yield).abs() < EPSILON,
        "preparing pays fraction × what the same crew gathers: {cultivating_yield} vs {}",
        fraction * sustain_yield
    );
    // …and the other regime: an ample crew clears the same standing surplus for no yield at all,
    // because the patch's stock — not the crew — is what binds.
    assert!(
        (one_turn_yield(Some(Improvement::Cultivate)) - one_turn_yield(None)).abs() < EPSILON,
        "a crew that saturates the stock anyway pays no dip"
    );

    // Over a full preparation the patch never leaves Thriving — the dip is drawn off the MSY ceiling,
    // so it is a sustainable take, not a depletion.
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));
    let (_, progress_per_turn, _) = cultivation_config(&app);
    run_turns_with_forage(&mut app, (1.0 / progress_per_turn).ceil() as u32);
    assert_eq!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .ecology_phase,
        EcologyPhase::Thriving,
        "the preparing take is sustainable — the patch stays healthy"
    );
}

/// **A CREW THAT STRIPS THE GROUND IT IS CLEARING BUILDS SLOWLY** — §0.3's measurement, inverted.
///
/// Before `docs/plan_harvest_floor.md` §3 the harshest draw was strictly dominant while building:
/// dipped ×0.25, *every* stance completed a 25-turn Cultivate on schedule and the deepest one paid
/// 3.8× the food for it. The floor now paces the build (`intensification::learn_multiplier`), so
/// pulling harder buys food today at the price of turns — a real trade instead of a free lunch.
///
/// Asserted as a **relation**, not a pair of literals: a 0.15 build takes materially longer than a
/// food-peak one, and it still completes (the rate is a slope, not a gate — there is no lapse state
/// left to strand a build in).
#[test]
fn a_low_floor_cultivate_takes_materially_longer_than_a_food_peak_one() {
    /// Long enough for even the shallowest swept floor to finish several times over, so a run that
    /// hits it is a genuine never-completes rather than an impatient harness.
    const PATIENCE_TURNS: u32 = 400;

    /// How much longer the deep-floor build must take before the trade counts as *material*. The
    /// arithmetic says `0.5 / 0.15 ≈ 3.3×`; the bound is deliberately loose because what is being
    /// pinned is that the pressure costs turns at all, not the exact slope (which is
    /// `learn_multiplier`'s to change).
    const MATERIALLY_LONGER: f32 = 2.0;

    let turns_to_cultivate = |floor: f32| -> u32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        spawn_forager_at(
            &mut app,
            tile,
            coord,
            Some(Improvement::Cultivate),
            FORAGE_WORKERS,
            floor,
        );
        for turn in 1..=PATIENCE_TURNS {
            run_turns_with_forage(&mut app, 1);
            if app
                .world
                .resource::<ForageRegistry>()
                .patch(coord)
                .is_some_and(|patch| patch.is_cultivated())
            {
                return turn;
            }
        }
        panic!("a build at floor {floor} never completed in {PATIENCE_TURNS} turns");
    };

    let at_the_peak = turns_to_cultivate(FOOD_PEAK_FLOOR);
    let stripping = turns_to_cultivate(0.15);
    assert!(
        stripping as f32 >= at_the_peak as f32 * MATERIALLY_LONGER,
        "a crew stripping the ground it is clearing must pay for it in turns: {stripping} vs \
         {at_the_peak} at the food peak"
    );
}

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

/// The Cultivate policy accrues the **full** `progress_per_turn` while worked (the decay pass spares a
/// patch under active preparation), completes in `1 / progress_per_turn` turns, and the completed
/// patch then pays the full tended yield — strictly more than the wild Sustain skim it replaced.
#[test]
fn cultivate_completes_then_pays_the_tended_yield() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let band = spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));
    let (_, progress_per_turn, _) = cultivation_config(&app);

    // Progress accrues at the full rate — no net-of-decay drag while the crew is working it.
    run_turns_with_forage(&mut app, 3);
    let built = progress_of(&app, coord);
    assert!(
        (built - 3.0 * progress_per_turn).abs() < 1e-5,
        "an actively-prepared patch accrues the full progress_per_turn: {built}"
    );

    let turns_to_prepare = (1.0 / progress_per_turn).ceil() as u32;
    run_turns_with_forage(&mut app, turns_to_prepare);
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
    let sustain_yield = steady_turn_yield(None, 3 + turns_to_prepare + 1);
    assert!(
        tended_yield > sustain_yield,
        "a tended patch out-pays the wild Sustain gather — the payoff the 25 turns bought: \
         {tended_yield} vs {sustain_yield}"
    );
    assert_eq!(
        app.world
            .get::<LaborAllocation>(band)
            .unwrap()
            .last_yields
            .len(),
        1
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
    spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));
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
    spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));
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

    // The state a completed preparation leaves behind: cultivated, owned, and flagged worked-this-turn
    // (the labor arm sets the flag the turn it completes, so the next Logistics decay pass spares it).
    let biomass_before = {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.cultivation_progress = 1.0;
        patch.owner = Some(FactionId(0));
        patch.tended_this_turn = true;
        patch.biomass
    };
    grant_cultivation_knowledge(&mut app, FactionId(0));
    // Sustain, not Cultivate: this test reads the finished rung's *harvest*, on a patch seated
    // already-complete — the rung a band that really built it is retired onto (issue #420).
    spawn_forager(&mut app, tile, coord, None);
    assert_eq!(provisions_f32(&mut app), 0.0, "larder starts empty");

    // The decay pass pays nothing and spares the worked patch.
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
        patch.cultivation_progress = 1.0;
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

    // Keep neglecting it → progress fully decays and ownership lapses (~1/decay_per_turn turns).
    let (_, _, decay) = cultivation_config(&app);
    run_turns_untended(&mut app, (1.0 / decay).ceil() as u32 + 2);
    let patch_registry = app.world.resource::<ForageRegistry>();
    let patch = patch_registry.patch(coord).unwrap();
    assert_eq!(patch.cultivation_progress, 0.0, "feral patch fully reverts");
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
    let band = spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));

    run_turns_with_forage(&mut app, 5);
    let banked = progress_of(&app, coord);
    assert!(banked > 0.0 && banked < 1.0, "part-prepared: {banked}");

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
    let (_, _, decay) = cultivation_config(&app);
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
///   hand the verb back — otherwise the crew that lost the race keeps paying
///   `yield_fraction_while_building` on prepared ground, which is issue #420 all over again for the
///   second band.
#[test]
fn a_completed_cultivation_announces_once_and_clears_every_bands_verb() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let first = spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));
    let second = spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));
    // A token second crew. `Cultivate` accrues per *assignment*, not per worker, so one hand is
    // enough to hold the verb — and two full `FORAGE_WORKERS` crews draw the patch out of Thriving,
    // which stalls the very meter this test needs to finish.
    set_forage_workers(&mut app, second, TOKEN_SECOND_CREW);

    // Long enough for the meter to fill however the two crews' accruals interleave, plus the turn a
    // band that did not finish it needs to notice (its clear is decided at the top of its own
    // iteration, so a crew processed *before* the finisher clears on the following turn).
    let (_, progress_per_turn, _) = cultivation_config(&app);
    let turns = (1.0 / progress_per_turn).ceil() as u32 + 1;
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
    for (label, band) in [("the finisher", first), ("the second crew", second)] {
        assert_eq!(
            app.world.get::<LaborAllocation>(band).unwrap().assignments[0].improvement,
            None,
            "{label} must hand the verb back — there is nothing left to cultivate here"
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
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    let patch = registry.patch_mut(coord).expect("patch");
    patch.cultivation_progress = 1.0;
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

    // One worked turn — the flag the labor arm would set — and the counter is back to nothing.
    app.world
        .resource_mut::<ForageRegistry>()
        .patch_mut(coord)
        .expect("patch")
        .tended_this_turn = true;
    run_turns_untended(&mut app, 1);
    assert_eq!(
        neglect_turns_of(&app, coord),
        0,
        "a worked turn forgives the neglect outright"
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
    let grace = tended_grace(&app);

    run_turns_untended(&mut app, grace);
    assert_eq!(
        progress_of(&app, coord),
        1.0,
        "the last forgiven turn leaves the meter untouched"
    );

    run_turns_untended(&mut app, 1);
    let (_, _, decay) = cultivation_config(&app);
    assert!(
        (progress_of(&app, coord) - (1.0 - decay)).abs() < 1e-6,
        "the first turn past the grace bleeds exactly one turn's decay: {}",
        progress_of(&app, coord)
    );
}

/// **A lost rung is announced.** Crossing back below `1.0` destroys a 25-turn investment's payoff, so
/// the feed says so — once, on the transition, the way the animal web has always announced a lost pen.
/// The long bleed to zero that follows adds nothing further.
#[test]
fn losing_a_tended_patch_pushes_one_feed_line() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    seat_tended_patch(&mut app, coord);
    let grace = tended_grace(&app);

    run_turns_untended(&mut app, grace);
    assert_eq!(
        completion_announcements(&app, "gone feral"),
        0,
        "nothing is announced while the grace holds — nothing has been lost"
    );

    run_turns_untended(&mut app, 1);
    assert_eq!(
        completion_announcements(&app, "gone feral"),
        1,
        "the turn the patch reverts, the player is told"
    );

    // The rest of the bleed is not news.
    let (_, _, decay) = cultivation_config(&app);
    run_turns_untended(&mut app, (1.0 / decay).ceil() as u32 + 2);
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

/// **The build accrues at `min(workers / crew_needed, 1)` — the crew is a real multiplier, not a
/// label.** Pinned at BOTH full crew and half crew, because a test that only ever runs a full crew
/// cannot see whether the multiplier exists at all.
#[test]
fn a_cultivate_build_accrues_in_proportion_to_its_crew() {
    let crew = {
        let app = spawn_world();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        ladder
            .rung(RungKey::PlantTended)
            .build_crew_needed()
            .expect("the tended rung declares a crew")
    };
    assert!(
        crew >= 2,
        "the fixture needs a crew it can under-staff: {crew}"
    );

    let progress_after = |workers: u32| -> f32 {
        let mut app = spawn_world();
        let (tile, coord) = prime_thriving_patch(&mut app);
        grant_cultivation_knowledge(&mut app, FactionId(0));
        let band = spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));
        set_forage_workers(&mut app, band, workers);
        run_turns_with_forage(&mut app, 1);
        progress_of(&app, coord)
    };

    let (_, full_rate, _) = cultivation_config(&spawn_world());
    let full = progress_after(crew);
    let half = progress_after(1);
    assert!(
        (full - full_rate).abs() < 1e-6,
        "a full crew builds at the rung's stated rate: {full} vs {full_rate}"
    );
    assert!(
        (half - full_rate / crew as f32).abs() < 1e-6,
        "one worker of a crew of {crew} builds at 1/{crew} of it: {half}"
    );
}

/// **A build's crew FLOORS the source's `workers_needed`** — the plant twin of a managed herd's
/// `herders_needed`. Without it the count was inverted from the *dipped* take, so committing to a
/// 25-turn improvement asked for fewer hands than gathering the same ground and flagged the second
/// worker as overstaffing.
#[test]
fn a_running_build_demands_at_least_its_crew() {
    let crew = {
        let app = spawn_world();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        ladder
            .rung(RungKey::PlantTended)
            .build_crew_needed()
            .expect("the tended rung declares a crew")
    };

    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let band = spawn_forager(&mut app, tile, coord, Some(Improvement::Cultivate));
    set_forage_workers(&mut app, band, crew);
    run_turns_with_forage(&mut app, 1);

    let needed = app
        .world
        .get::<LaborAllocation>(band)
        .expect("band allocation")
        .last_yields[0]
        .workers_needed;
    assert!(
        needed >= crew,
        "a running Cultivate wants at least its crew of {crew}, not {needed}"
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
        if progress_of(&app, coord) < 1.0 && first_move.is_none() {
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
        .find(|(_, patch)| patch.owner.is_none() && patch.cultivation_progress == 0.0)
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
