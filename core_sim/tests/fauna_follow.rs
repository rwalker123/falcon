use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_herds, advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FogRevealLedger,
    FollowPolicy, ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry,
    HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget,
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    TileRegistry, WellbeingConfigHandle,
};

/// Whole-worker head-count assigned to the hunt in these ecology tests. Large enough that the
/// per-worker biomass cap never binds, so the take is set entirely by the policy ceiling (matching
/// the retired persistent-follow behavior these tests were written against).
const HUNT_WORKERS: u32 = 5000;

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
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// Pick a **stationary** game herd (route length 1) so the hunting band stays adjacent every turn,
/// set its biomass to half its cap for a clear regrowth signal, and return `(id, starting_biomass)`.
fn prime_stationary_herd(app: &mut App) -> (String, f32) {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .or_else(|| registry.herds.iter().find(|h| h.id.starts_with("game_")))
            .map(|h| h.id.clone())
            .expect("expected short-range game to spawn")
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.biomass = (herd.carrying_capacity * 0.5).max(1.0);
    (id, herd.biomass)
}

/// Spawn a band standing on the herd's tile with a Hunt labor assignment under `policy`.
fn spawn_hunter(app: &mut App, herd_id: &str, policy: FollowPolicy) -> bevy::prelude::Entity {
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .unwrap()
        .position();
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("herd tile resolves");
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                // Plenty of working-age so the assignment's whole-worker head-count is available.
                working: scalar_from_f32(HUNT_WORKERS as f32),
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
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: herd_id.to_string(),
                        policy,
                    },
                    workers: HUNT_WORKERS,
                }],
                ..Default::default()
            },
        ))
        .id()
}

fn run_turns(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_labor_allocation);
    }
}

fn biomass_of(app: &App, herd_id: &str) -> Option<f32> {
    app.world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .map(|h| h.biomass)
}

fn has_hunt_assignment(app: &App, band: bevy::prelude::Entity) -> bool {
    app.world
        .get::<LaborAllocation>(band)
        .map(|a| {
            a.assignments
                .iter()
                .any(|x| matches!(x.target, LaborTarget::Hunt { .. }))
        })
        .unwrap_or(false)
}

#[test]
fn sustain_hunt_keeps_biomass_stable() {
    let mut app = spawn_world();
    let (id, start) = prime_stationary_herd(&mut app);
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    run_turns(&mut app, 10);

    let after = biomass_of(&app, &id).expect("sustained herd should survive");
    assert!(
        after > start * 0.6 && after <= start * 1.4,
        "sustain should keep biomass ~stable: start {start}, after {after}"
    );
    // The Hunt assignment persists while the herd is in range.
    assert!(
        has_hunt_assignment(&app, band),
        "Hunt assignment should persist"
    );
}

/// **Surplus slowly declines a herd — a SLOW-breeder claim** (intensification ladder slice 8).
///
/// Surplus is a **proportional stock skim** (`surplus.take_fraction × B` = 0.10) now, not the retired
/// `1.6 × MSY` *flow*. The flow declined *every* herd unconditionally, because it was defined as a
/// multiple of regrowth; a skim declines one only when it out-takes that herd's regrowth:
///
/// ```text
/// declines  ⟺  0.10 · B  >  r · B · (1 − B/K)   ⟺  r < 0.20  at the B* = K/2 operating point
/// ```
///
/// So a **fast breeder out-breeds a 10% skim and stabilises at a lower stock** — the same real
/// property Market already has at 20% (`fauna_market`, whose two tests are pinned to a slow `r` for
/// exactly this reason), and the reason the flow had to go: a flow ceiling never accumulates, so
/// `floor(ceiling / body_mass)` was `0` forever for every animal heavier than its herd's MSY.
///
/// The spawned route-1 game this harness reuses is **fast** (fowl `r` = 0.35, which *grows* under a
/// 10% skim), so seat it at a slow-breeder rate to exercise the mechanic the test is named for.
/// Pinning `r` also makes it deterministic — the ambient per-species rate is order-dependent in the
/// shared test binary.
#[test]
fn surplus_hunt_declines() {
    /// Below the ~0.20 skim-decline threshold — deer/megafauna, the game a 10% skim really does bleed.
    const SLOW_BREEDER_R: f32 = 0.05;

    let mut app = spawn_world();
    let (id, start) = prime_stationary_herd(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.regrowth_rate = SLOW_BREEDER_R;
    }
    spawn_hunter(&mut app, &id, FollowPolicy::Surplus);
    run_turns(&mut app, 10);

    let after = biomass_of(&app, &id).expect("surplus herd should still exist after 10 turns");
    assert!(
        after < start,
        "surplus should slowly decline: start {start}, after {after}"
    );
}

#[test]
fn eradicate_hunt_drives_extinction() {
    let mut app = spawn_world();
    let (id, _start) = prime_stationary_herd(&mut app);
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Eradicate);
    run_turns(&mut app, 40);

    assert!(
        biomass_of(&app, &id).is_none(),
        "eradicate should drive the group to local extinction"
    );
    // Once the herd is gone the assignment lapses.
    assert!(
        !has_hunt_assignment(&app, band),
        "assignment should lapse after the herd despawns"
    );
}
