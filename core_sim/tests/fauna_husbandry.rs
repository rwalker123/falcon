//! Phase E husbandry: a sustained Sustain hunt on a Thriving herd tames it into domesticated
//! livestock (emergent accrual + decay), which then yields steady provisions and is immune to the
//! overhunting collapse. Uses the source-centric labor allocation (a Hunt assignment) that replaced
//! the retired persistent follow.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_herds, advance_husbandry, advance_labor_allocation, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FogRevealLedger,
    FollowPolicy, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, WellbeingConfigHandle, FOOD,
};

/// Whole-worker head-count assigned to the hunt — large enough that the per-worker biomass cap
/// never binds, so a Sustain hunt takes exactly the net regrowth (herd stays Thriving → accrues).
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
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// A stationary game herd (route length 1) primed to half its cap → Thriving and a clean
/// domestication candidate. Returns its id.
fn prime_thriving_herd(app: &mut App) -> String {
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
    id
}

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
                working: scalar_from_f32(HUNT_WORKERS as f32),
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
            },
        ))
        .id()
}

/// One full turn's fauna pipeline in real stage order: Logistics (herds regrow, husbandry upkeep)
/// then Population (labor allocation resolves the hunt + accrues husbandry).
fn run_turns_with_hunt(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
        app.world.run_system_once(advance_labor_allocation);
    }
}

/// Turns with no active band: only the Logistics-stage systems run.
fn run_turns_untended(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
    }
}

fn progress_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.domestication_progress)
        .unwrap_or(0.0)
}

/// Total provisions carried by faction 0's bands (food is band-local now, so the husbandry yield
/// lands in the owner's cohort larders, not the faction pool).
fn provisions(app: &mut App) -> i64 {
    provisions_f32(app).round() as i64
}

/// Un-rounded total FOOD carried by faction 0's bands — needed to observe sub-1 fractional yields
/// that the rounding `provisions` helper would collapse to zero.
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

/// A sustained Sustain hunt on a Thriving herd tames it: progress climbs to 1.0 (domesticated) and
/// the hunter's faction owns it.
#[test]
fn sustain_hunt_domesticates_thriving_herd() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Sustain);

    // net accrual = progress_per_turn(0.04) - decay(0.01) = 0.03/turn → ~34 turns to 1.0.
    run_turns_with_hunt(&mut app, 45);

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("domesticated herd persists");
    assert!(
        herd.is_domesticated(),
        "sustained Sustain hunt should domesticate: progress {}",
        herd.domestication_progress
    );
    assert_eq!(herd.owner, Some(FactionId(0)), "the hunter owns the herd");
    assert_eq!(registry.domesticated_count(FactionId(0)), 1);
}

/// Only a Sustain hunt tames; an Eradicate hunt never accrues husbandry.
#[test]
fn eradicate_hunt_does_not_domesticate() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    spawn_hunter(&mut app, &id, FollowPolicy::Eradicate);
    run_turns_with_hunt(&mut app, 10);
    assert_eq!(
        progress_of(&app, &id),
        0.0,
        "eradicate accrues no husbandry"
    );
}

/// Husbandry progress decays and ownership lapses once the herd isn't being tended.
#[test]
fn progress_decays_without_sustained_hunt() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let band = spawn_hunter(&mut app, &id, FollowPolicy::Sustain);
    run_turns_with_hunt(&mut app, 6);
    let built = progress_of(&app, &id);
    assert!(built > 0.0, "some progress should have accrued");

    // Stop hunting, then let husbandry decay run.
    app.world.despawn(band);
    run_turns_untended(&mut app, 6);
    let decayed = progress_of(&app, &id);
    assert!(
        decayed < built,
        "progress should decay: {built} -> {decayed}"
    );

    // Decay all the way down clears ownership.
    run_turns_untended(&mut app, 60);
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("herd still exists");
    assert_eq!(herd.domestication_progress, 0.0);
    assert_eq!(herd.owner, None, "ownership lapses at zero progress");
}

/// A domesticated (managed) herd is immune to the overhunting collapse: driven below the Allee
/// threshold it recovers logistically instead of crashing to extinction.
#[test]
fn domesticated_herd_is_collapse_immune() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let low = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0)); // sets owner + progress = 1.0 → domesticated
                                                // Below the 15% collapse threshold — a wild herd here would crash.
        let low = herd.carrying_capacity * 0.10;
        herd.biomass = low;
        low
    };

    run_turns_untended(&mut app, 10);

    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry
        .find(&id)
        .expect("a domesticated herd never collapses to extinction");
    assert!(
        herd.biomass > low,
        "managed herd should recover, not crash: {low} -> {}",
        herd.biomass
    );
}

/// A domesticated herd yields steady provisions to its owner each turn without depleting.
#[test]
fn domesticated_herd_yields_provisions() {
    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    let biomass_before = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0));
        herd.biomass
    };
    assert_eq!(provisions(&mut app), 0);

    app.world.run_system_once(advance_husbandry);

    assert!(
        provisions(&mut app) > 0,
        "a domesticated herd should pay its owner provisions"
    );
    // The yield is a sustainable harvest — it does not reduce the herd.
    let after = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .biomass;
    assert_eq!(
        after, biomass_before,
        "husbandry yield must not deplete biomass"
    );
}

/// Regression (fully-fractional FOOD income): a domesticated herd whose per-turn yield is below
/// 1.0 provisions (biomass 30 × `provisions_per_biomass` 0.01 = 0.3) must still credit the owner's
/// larder.
#[test]
fn sub_unit_husbandry_yield_credits_larder() {
    const SUB_UNIT_BIOMASS: f32 = 30.0;

    let mut app = spawn_world();
    let id = prime_thriving_herd(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.claim_domestication(FactionId(0));
        herd.biomass = SUB_UNIT_BIOMASS;
    }
    assert_eq!(provisions_f32(&mut app), 0.0, "larder starts empty");

    app.world.run_system_once(advance_husbandry);

    let larder = provisions_f32(&mut app);
    assert!(
        larder > 0.0 && larder < 1.0,
        "a sub-1 husbandry yield must credit a positive fractional amount (got {larder})"
    );
}
