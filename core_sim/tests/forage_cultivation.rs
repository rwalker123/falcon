//! Cultivation as an **explicit policy with an investment cost** (Intensification Rung 1a/1b).
//!
//! Sustain-foraging a Thriving patch **teaches the faction Cultivation** (Rung 1b knowledge, earned by
//! doing) but no longer tames the patch — the old free auto-accrual is gone, because "same labor, same
//! tile, no cost" made cultivating unconditionally correct and erased the decision. Cultivating is now
//! the `FollowPolicy::Cultivate` policy: while preparing, the patch yields only
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
    FactionInventory, FaunaConfigHandle, FogRevealLedger, FollowPolicy, FoodModuleTag,
    ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle,
    LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, RungKey,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit, Tile,
    TileRegistry, WellbeingConfigHandle, CULTIVATION_DISCOVERY_ID, FOOD, RUNG_TIMESCALE_UNSCALED,
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

/// Float slack for provisions comparisons (fixed-point conversion + multiplication order).
const EPSILON: f32 = 1e-4;

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
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// A `FoodModuleTag` tile that carries a seeded patch. Primes the patch to half its cap (Thriving,
/// with regrowth headroom) so the take is a clean MSY skim. Returns the tile entity + its coord.
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
        let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .filter(|(tile, _)| registry.patch(tile.position).is_some())
            .find(|(tile, _)| {
                let composition = tile_flora_composition(&flora, &labor.forage, tile);
                let Some(species) =
                    default_species_for_rung(&composition, &flora, RungKey::PlantTended)
                else {
                    return false;
                };
                let share = composition
                    .iter()
                    .find(|entry| entry.species == species)
                    .map_or(0.0, |entry| entry.share);
                let capacity = tile_forage_capacity(&labor.forage, tile);
                let payoff = commit_payoff(
                    tile.position,
                    capacity,
                    &species,
                    share,
                    &flora,
                    &labor.forage,
                    1.0,
                    RungKey::PlantTended,
                );
                let wild = wild_payoff(tile.position, capacity, &flora, &labor.forage, 1.0);
                commit_yield_ratio(payoff, wild) > 1.0
            })
            .map(|(tile, _)| tile.position)
            .expect("a FoodModuleTag tile whose default crop out-yields the wild basket")
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.biomass = patch.carrying_capacity * 0.5;
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
    }
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    (entity, coord)
}

/// Switch a band's (single) Forage assignment to `policy` — what the client's picker does, and what a
/// player does the turn an improvement finishes and they want to start harvesting it rather than
/// building it.
fn set_forage_policy(app: &mut App, band: bevy::prelude::Entity, policy: FollowPolicy) {
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("band forages");
    let assignment = allocation
        .assignments
        .first_mut()
        .expect("a Forage assignment");
    let LaborTarget::Forage {
        policy: current, ..
    } = &mut assignment.target
    else {
        panic!("the fixture band forages");
    };
    *current = policy;
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

/// One turn's forage pipeline in stage order: Logistics (regrowth, cultivation decay) then Population
/// (labor allocation resolves the take and accrues the investment).
fn run_turns_with_forage(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_forage_regrowth);
        app.world.run_system_once(advance_cultivation);
        app.world.run_system_once(advance_labor_allocation);
    }
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
        tended.build_accrual(FollowPolicy::Cultivate, true, RUNG_TIMESCALE_UNSCALED),
        tended.build_decay(RUNG_TIMESCALE_UNSCALED),
    )
}

/// One turn of the pipeline under `policy` on a fresh identical world; returns the provisions the
/// band was paid. Lets a test compare the Cultivate **dip** against the Sustain baseline without
/// re-deriving the MSY formula anywhere.
fn one_turn_yield(policy: FollowPolicy) -> f32 {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, policy);
    run_turns_with_forage(&mut app, 1);
    provisions_f32(&mut app)
}

/// **The free path is gone.** Sustain-foraging a Thriving patch still teaches the faction Cultivation
/// (knowledge is earned by doing), but it never accrues `cultivation_progress` — not even once the
/// faction knows Cultivation. Cultivating costs something now, and the player must choose to pay it.
#[test]
fn sustain_forage_teaches_cultivation_but_never_tames_the_patch() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

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

/// **The investment cost.** A patch worked under `Cultivate` pays only
/// `cultivating_yield_fraction × the Sustain (MSY) yield` — the crew is preparing ground, not
/// gathering — and the reduced take is *sustainable*, so the patch stays Thriving throughout.
#[test]
fn cultivate_pays_a_fraction_of_the_sustain_yield_and_keeps_the_patch_healthy() {
    let sustain_yield = one_turn_yield(FollowPolicy::Sustain);
    let cultivating_yield = one_turn_yield(FollowPolicy::Cultivate);
    assert!(
        sustain_yield > 0.0,
        "baseline Sustain yield must be positive"
    );

    let mut app = spawn_world();
    let (fraction, _, _) = cultivation_config(&app);
    assert!(
        (cultivating_yield - fraction * sustain_yield).abs() < EPSILON,
        "preparing pays fraction × the Sustain yield: {cultivating_yield} vs {}",
        fraction * sustain_yield
    );

    // Over a full preparation the patch never leaves Thriving — the dip is drawn off the MSY ceiling,
    // so it is a sustainable take, not a depletion.
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, FollowPolicy::Cultivate);
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
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);
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

    set_forage_policy(&mut app, band, FollowPolicy::Cultivate);
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
    assert_eq!(
        core_sim::patch_provisions_per_biomass(&patch, &flora, &labor.forage),
        labor.forage.provisions_per_biomass,
        "and it still converts at the wild basket average"
    );
}

/// The Cultivate policy accrues the **full** `progress_per_turn` while worked (the decay pass spares a
/// patch under active preparation), completes in `1 / progress_per_turn` turns, and the completed
/// patch then pays the full tended yield — strictly more than the wild Sustain skim it replaced.
#[test]
fn cultivate_completes_then_pays_the_tended_yield() {
    let sustain_yield = one_turn_yield(FollowPolicy::Sustain);

    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Cultivate);
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

    // **Harvest it to read the payoff.** `Cultivate` is the *build* verb: its dip means "the crew is
    // preparing ground, not gathering", which stays true once the ground is ready — so a completed
    // patch left on `Cultivate` still pays the dip, exactly as `Tame` does on an already-tamed herd.
    // (Slice 7: the retired managed branch ignored the policy and paid the flat rate, so this test
    // used to read the payoff without ever switching off the build verb.) The player switches to a
    // harvest policy; so does the test.
    set_forage_policy(&mut app, band, FollowPolicy::Sustain);
    let before = provisions_f32(&mut app);
    run_turns_with_forage(&mut app, 1);
    let tended_yield = provisions_f32(&mut app) - before;
    assert!(
        tended_yield > sustain_yield,
        "a tended patch out-pays the wild Sustain skim — the payoff the 25 turns bought: \
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
    spawn_forager(&mut app, tile, coord, FollowPolicy::Cultivate);
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
    spawn_forager(&mut app, tile, coord, FollowPolicy::Cultivate);
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
    // Sustain, not Cultivate: this test reads the finished rung's *harvest*, and the build verb pays
    // its dip whether or not there is anything left to build.
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);
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

    // No forager band → the patch is never worked. One untended Logistics turn reverts it to wild.
    run_turns_untended(&mut app, 1);
    assert!(
        !app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_cultivated(),
        "an untended tended patch reverts to a wild gather patch after one turn"
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
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Cultivate);

    run_turns_with_forage(&mut app, 5);
    let banked = progress_of(&app, coord);
    assert!(banked > 0.0 && banked < 1.0, "part-prepared: {banked}");

    // The `tended_this_turn` flag is a deliberate one-turn-lag signal (Logistics runs before
    // Population), so the first Logistics pass after the band leaves still sees the flag set from its
    // last worked turn and spares the patch. Decay bites from the turn after that.
    app.world.despawn(band);
    const ABANDONED_TURNS: u32 = 3;
    const SPARED_LAG_TURNS: u32 = 1;
    run_turns_untended(&mut app, ABANDONED_TURNS);
    let (_, _, decay) = cultivation_config(&app);
    let decayed = progress_of(&app, coord);
    let expected_decay = decay * (ABANDONED_TURNS - SPARED_LAG_TURNS) as f32;
    assert!(
        (banked - decayed - expected_decay).abs() < 1e-5,
        "an abandoned preparation decays by decay_per_turn/turn (after the one-turn flag lag): \
         {banked} -> {decayed}"
    );
}
