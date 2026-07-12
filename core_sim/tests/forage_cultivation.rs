//! Phase 1a cultivation (Rung 1a — worker-tended, place-local tended patch): a sustained Sustain
//! forage on a Thriving patch tames it into a cultivated crop (emergent accrual + decay). A completed
//! "tended" patch is **worked, not passive**: it pays only the band that TENDS it (its Forage
//! assignment, place-local) a higher-than-wild yield WITHOUT being drawn down, and if no band tends it
//! for a turn it goes **feral** (decays back below the cultivated threshold, reverting to a wild
//! gather patch). The plant mirror of `fauna_husbandry.rs`; world setup mirrors it too.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_cultivation, advance_forage_regrowth, advance_labor_allocation, scalar_from_f32,
    scalar_one, scalar_zero, spawn_initial_forage, spawn_initial_world, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle,
    FogRevealLedger, FollowPolicy, FoodModuleTag, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment,
    LaborConfigHandle, LaborTarget, LocalStore, MapPresets, MapPresetsHandle, MoraleCause,
    PopulationCohort, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, Tile, TileRegistry, WellbeingConfigHandle,
    CULTIVATION_DISCOVERY_ID, FOOD,
};

/// Grant faction-level **Cultivation** knowledge (Rung 1b) directly via the ledger, so patch
/// cultivation is unlocked. Rung-1a tests seed this since they exercise the *tended-patch* mechanic,
/// not the earned-knowledge gate (which has its own tests below). Mirrors how other knowledge-gated
/// tests set up prerequisites through the `DiscoveryProgressLedger`.
fn grant_cultivation_knowledge(app: &mut App, faction: FactionId) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, CULTIVATION_DISCOVERY_ID, scalar_one());
}

/// Whole-worker head-count assigned to the forage — large enough that the per-worker gather cap
/// never binds (the accrual hook is independent of the take, but this keeps the patch productive).
const FORAGE_WORKERS: u32 = 5000;

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
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// A `FoodModuleTag` tile that carries a seeded patch. Primes the patch to half its cap (Thriving,
/// with regrowth headroom) so a Sustain forage keeps it Thriving and accruing. Returns the tile
/// entity + its coord.
fn prime_thriving_patch(app: &mut App) -> (bevy::prelude::Entity, UVec2) {
    let coord = {
        let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .map(|(tile, _)| tile.position)
            .find(|pos| registry.patch(*pos).is_some())
            .expect("a FoodModuleTag tile with a seeded patch")
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.biomass = patch.carrying_capacity * 0.5;
    }
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
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
                    },
                    workers: FORAGE_WORKERS,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// One turn's forage pipeline in stage order: Logistics (regrowth, cultivation upkeep) then
/// Population (labor allocation resolves the forage + accrues cultivation).
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

/// A sustained Sustain forage on a Thriving patch cultivates it: progress climbs to 1.0 and the
/// foraging faction owns it. Net accrual = progress_per_turn(0.04) − decay(0.01) = 0.03/turn.
#[test]
fn sustain_forage_cultivates_thriving_patch() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    run_turns_with_forage(&mut app, 45);

    let registry = app.world.resource::<ForageRegistry>();
    let patch = registry.patch(coord).expect("patch persists");
    assert!(
        patch.is_cultivated(),
        "a sustained Sustain forage should cultivate: progress {}",
        patch.cultivation_progress
    );
    assert_eq!(
        patch.owner,
        Some(FactionId(0)),
        "the forager owns the patch"
    );
    assert_eq!(registry.cultivated_count(FactionId(0)), 1);
}

/// The per-turn net is exactly progress_per_turn − decay_per_turn while Sustain-foraged, and pure
/// decay once untended.
#[test]
fn cultivation_nets_accrual_minus_decay_then_decays() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    // Head-start the progress (and ownership) so the Logistics decay bites every turn — a patch at
    // exactly 0 would have its first-turn decay floored at 0, muddying the exact-net check.
    const START: f32 = 0.2;
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.cultivation_progress = START;
        patch.owner = Some(FactionId(0));
    }
    grant_cultivation_knowledge(&mut app, FactionId(0));
    let band = spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    // A few tended turns: net +0.03/turn (accrual 0.04 in Population − decay 0.01 in Logistics).
    const TENDED_TURNS: u32 = 5;
    run_turns_with_forage(&mut app, TENDED_TURNS);
    let built = progress_of(&app, coord);
    let expected = START + (0.04f32 - 0.01f32) * TENDED_TURNS as f32;
    assert!(
        (built - expected).abs() < 1e-4,
        "net accrual should be (progress − decay)/turn: got {built}, expected {expected}"
    );

    // Stop foraging → pure decay.
    app.world.despawn(band);
    run_turns_untended(&mut app, 2);
    let decayed = progress_of(&app, coord);
    assert!(
        decayed < built && (built - decayed - 0.02).abs() < 1e-4,
        "untended patch should decay by decay_per_turn/turn: {built} -> {decayed}"
    );
}

/// Rung 1a: a tended (cultivated) patch pays the band that TENDS it (its Forage assignment), via the
/// labor arm — **place-local** — WITHOUT drawing biomass down, and is not wild gather-drawn. The
/// `advance_cultivation` pass itself pays nothing now (it only decays untended patches — the even
/// split across all the owner's bands is retired).
#[test]
fn tended_patch_pays_tending_band_without_depletion() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);

    // Claim the patch as a cultivated crop for the foraging faction.
    let biomass_before = {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.claim_cultivation(FactionId(0));
        patch.biomass
    };
    // The owner band tends it (a Forage assignment on the cultivated patch). It knows Cultivation so
    // the tending Sustain forage can re-accrue the patch after the decay pass.
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);
    assert_eq!(provisions_f32(&mut app), 0.0, "larder starts empty");

    // The decay pass alone pays nothing — tended food is paid by the tending band's labor, not here.
    app.world.run_system_once(advance_cultivation);
    assert_eq!(
        provisions_f32(&mut app),
        0.0,
        "advance_cultivation no longer pays a cultivated patch's owner (even-split retired)"
    );

    // The tending band's labor resolves the tended yield place-local, without depleting biomass. (The
    // decay pass above briefly dropped progress below 1.0, but the tending Sustain forage re-accrues
    // it to cultivated and pays the tended yield — the tender keeps its farm alive.)
    app.world.run_system_once(advance_labor_allocation);
    let paid = provisions_f32(&mut app);
    assert!(
        paid > 0.0,
        "the tending band is paid the tended yield via its Forage assignment: {paid}"
    );
    assert_eq!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .biomass,
        biomass_before,
        "a tended patch is a managed harvest — biomass is not drawn down"
    );
    assert!(
        app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_cultivated(),
        "the tending band keeps the patch cultivated (re-accrues after the decay pass)"
    );
}

/// Rung 1a feral loop: a cultivated patch with no band tending it goes feral through the real
/// Logistics pipeline — `advance_cultivation` decays it below the cultivated threshold (reverting to
/// a wild gather patch) and it fully reverts over ~1/decay_per_turn turns (owner cleared).
#[test]
fn untended_cultivated_patch_goes_feral() {
    let mut app = spawn_world();
    let (_tile, coord) = prime_thriving_patch(&mut app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(coord).unwrap();
        patch.claim_cultivation(FactionId(0));
    }

    // No forager band → the patch is never tended. One untended Logistics turn reverts it to wild.
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
    let decay = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .forage
        .cultivation
        .decay_per_turn;
    run_turns_untended(&mut app, (1.0 / decay).ceil() as u32 + 2);
    let patch_registry = app.world.resource::<ForageRegistry>();
    let patch = patch_registry.patch(coord).unwrap();
    assert_eq!(patch.cultivation_progress, 0.0, "feral patch fully reverts");
    assert_eq!(patch.owner, None, "ownership lapses once fully feral");
    assert_eq!(
        patch_registry.cultivated_count(FactionId(0)),
        0,
        "no cultivated patches remain"
    );
}

/// Rung 1b (earned-knowledge gate): a faction that does NOT yet know Cultivation and Sustain-forages a
/// Thriving patch **builds faction Cultivation knowledge** (in the `DiscoveryProgressLedger`) but its
/// patch does **not** gain any `cultivation_progress` — knowledge is earned first, tended patches are
/// gated behind it.
#[test]
fn sustain_forage_earns_cultivation_knowledge_but_gates_patch() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    // No knowledge granted: the faction must learn Cultivation by foraging.
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    // A handful of turns — fewer than needed to complete the knowledge (0.05/turn vs threshold 1.0).
    run_turns_with_forage(&mut app, 5);

    let learned = app
        .world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FactionId(0), CULTIVATION_DISCOVERY_ID)
        .to_f32();
    assert!(
        learned > 0.0,
        "Sustain-forage teaches the faction Cultivation: ledger progress {learned}"
    );
    assert!(
        learned < 1.0,
        "5 turns should not yet complete Cultivation knowledge: {learned}"
    );

    let patch_progress = progress_of(&app, coord);
    assert_eq!(
        patch_progress, 0.0,
        "before knowing Cultivation, no patch may accrue cultivation_progress: {patch_progress}"
    );
    assert!(
        !app.world
            .resource::<ForageRegistry>()
            .patch(coord)
            .unwrap()
            .is_cultivated(),
        "an un-earned patch is not cultivated"
    );
}

/// Rung 1b: once the faction knows Cultivation (knowledge crosses the completion threshold), a Sustain
/// forage on a Thriving patch begins accruing `cultivation_progress` again — the gate opens.
#[test]
fn known_cultivation_unlocks_patch_accrual() {
    let mut app = spawn_world();
    let (tile, coord) = prime_thriving_patch(&mut app);
    grant_cultivation_knowledge(&mut app, FactionId(0));
    spawn_forager(&mut app, tile, coord, FollowPolicy::Sustain);

    run_turns_with_forage(&mut app, 5);

    let patch_progress = progress_of(&app, coord);
    assert!(
        patch_progress > 0.0,
        "with Cultivation known, the patch accrues cultivation_progress: {patch_progress}"
    );
}
