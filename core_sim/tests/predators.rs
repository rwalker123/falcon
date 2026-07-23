//! Predators Phase 0 — the hunt-danger casualty path (`docs/plan_predators.md`).
//!
//! A band hunting a herd whose species can fight back (`combat.attack > 0` — mammoth, ox) takes
//! **working-age casualties** through the combat subsystem, and **Warriors mitigate them** (the first
//! live consumer of the long-inert Warrior role). These tests drive a **real**
//! `advance_labor_allocation` turn and assert on the **real cohort brackets** it leaves behind.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_forage,
    spawn_initial_herds, spawn_initial_world, CombatConfig, CommandEventLog, CreaturesConfig,
    CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfig,
    FaunaConfigHandle, FogRevealLedger, FollowPolicy, ForageRegistry, GenerationId,
    GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, TileRegistry, WellbeingConfigHandle,
};

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
    app.world.insert_resource(ForageRegistry::default());
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
    app.world.run_system_once(spawn_initial_forage);
    app
}

/// Retag a real spawned herd to a chosen species and park it on a fat, stationary standing stock, so
/// a hunt reliably resolves this turn. Returns `(herd_id, tile_entity)` for the herd's tile.
fn dangerous_herd(app: &mut App, species_display: &str) -> (String, bevy::prelude::Entity) {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.route_length() == 1)
            .or_else(|| registry.herds.first())
            .map(|h| h.id.clone())
            .expect("the world seeded at least one herd")
    };
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.species = species_display.to_string();
        // A fat standing stock so a Surplus take always resolves — combat fires after the take
        // regardless of its size, but a real hunt keeps the test honest.
        herd.carrying_capacity = herd.carrying_capacity.max(4000.0);
        herd.biomass = herd.carrying_capacity;
    }
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .position();
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("herd tile resolves");
    (id, tile)
}

/// Spawn a hunting band on `tile` with `hunters` on the herd and `warriors` standing.
fn hunting_band(
    app: &mut App,
    tile: bevy::prelude::Entity,
    working: u32,
    fauna_id: &str,
    hunters: u32,
    warriors: u32,
) -> bevy::prelude::Entity {
    let mut assignments = vec![LaborAssignment {
        target: LaborTarget::Hunt {
            fauna_id: fauna_id.to_string(),
            policy: FollowPolicy::Surplus,
        },
        workers: hunters,
    }];
    if warriors > 0 {
        assignments.push(LaborAssignment {
            target: LaborTarget::Warrior,
            workers: warriors,
        });
    }
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: working,
                children: scalar_zero(),
                working: scalar_from_f32(working as f32),
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
            LaborAllocation {
                assignments,
                ..Default::default()
            },
        ))
        .id()
}

fn working_of(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .working
        .to_f32()
}

/// Parse the fractional `killed=<k> wounded=<w>` out of the most recent `hunt_danger` feed line.
fn last_danger_casualties(app: &App) -> Option<(f32, f32)> {
    let log = app.world.resource::<CommandEventLog>();
    log.iter()
        .filter(|e| e.kind.as_str() == "hunt_danger")
        .last()
        .and_then(|e| e.detail.clone())
        .map(|detail| {
            let field = |key: &str| -> f32 {
                detail
                    .split_whitespace()
                    .find_map(|tok| tok.strip_prefix(key))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0)
            };
            (field("killed="), field("wounded="))
        })
}

/// The mammoth's shipped display name — its combat block is `{ attack 8, defense 12 }`.
const MAMMOTH: &str = "Thunder Mammoths";
/// A defaulted species (attack 0) — a harmless hunt.
const DEER: &str = "Red Deer";

/// A band hunting a mammoth with **0 warriors** loses working-age population; the **same hunt with
/// warriors loses strictly fewer**, and the split shifts toward wounded as the party grows.
#[test]
fn warriors_reduce_the_deaths_of_a_dangerous_mammoth_hunt() {
    // --- No warriors: real casualties.
    let mut bare = spawn_world();
    let (herd, tile) = dangerous_herd(&mut bare, MAMMOTH);
    let bare_band = hunting_band(&mut bare, tile, 60, &herd, 4, 0);
    let before = working_of(&bare, bare_band);
    bare.world.run_system_once(advance_labor_allocation);
    let after_bare = working_of(&bare, bare_band);
    let killed_bare = before - after_bare;
    assert!(
        killed_bare > 0.0,
        "a mammoth (attack 8) hunt with no warriors must cost working-age lives: {killed_bare}"
    );
    let (feed_killed_bare, feed_wounded_bare) =
        last_danger_casualties(&bare).expect("a dangerous hunt pushes a hunt_danger feed line");
    assert!(feed_killed_bare + feed_wounded_bare > 0.0);

    // --- Many warriors: strictly fewer deaths, more wounded.
    let mut guarded = spawn_world();
    let (herd_g, tile_g) = dangerous_herd(&mut guarded, MAMMOTH);
    let guarded_band = hunting_band(&mut guarded, tile_g, 60, &herd_g, 4, 12);
    let before_g = working_of(&guarded, guarded_band);
    guarded.world.run_system_once(advance_labor_allocation);
    let killed_guarded = before_g - working_of(&guarded, guarded_band);

    assert!(
        killed_guarded < killed_bare,
        "warriors must reduce deaths: {killed_bare} (0 warriors) -> {killed_guarded} (12 warriors)"
    );

    // The wounded SHARE rises as the party grows (severity shifts toward recoverable) — read from the
    // fractional feed detail, which the bracket loss (killed) alone cannot show.
    let (kg, wg) = last_danger_casualties(&guarded).expect("guarded hunt still narrates");
    let guarded_wound_share = wg / (kg + wg).max(1e-6);
    let bare_wound_share = feed_wounded_bare / (feed_killed_bare + feed_wounded_bare).max(1e-6);
    assert!(
        guarded_wound_share > bare_wound_share,
        "the wounded share should RISE as warriors rise: bare {bare_wound_share}, guarded {guarded_wound_share}"
    );
}

/// A band hunting a **deer** (default `attack 0`) loses **nobody** — byte-identical to today.
#[test]
fn a_harmless_deer_hunt_costs_no_lives() {
    let mut app = spawn_world();
    let (herd, tile) = dangerous_herd(&mut app, DEER);
    let band = hunting_band(&mut app, tile, 60, &herd, 4, 0);
    let before = working_of(&app, band);
    app.world.run_system_once(advance_labor_allocation);
    let after = working_of(&app, band);
    assert_eq!(
        before, after,
        "a deer (attack 0) hunt must not touch working-age population: {before} -> {after}"
    );
    // ...and no hunt-danger feed line is pushed.
    assert!(
        last_danger_casualties(&app).is_none(),
        "a harmless hunt must push no hunt_danger event"
    );
}

/// Config validation rejects the illegitimate combat/aggression dials.
#[test]
fn fauna_config_rejects_illegitimate_combat_dials() {
    // A zero `combat.defense` — a `0/0` in the kill/wound split — is rejected.
    let zero_defense = FaunaConfig::from_json_str(&core_sim::BUILTIN_FAUNA_CONFIG.replace(
        r#""combat": { "attack": 8.0, "defense": 12.0, "range": "melee" }"#,
        r#""combat": { "attack": 8.0, "defense": 0.0, "range": "melee" }"#,
    ));
    assert!(
        zero_defense.is_err(),
        "combat.defense of 0 (a denominator) must be rejected"
    );

    // A negative attack is rejected.
    let negative_attack = FaunaConfig::from_json_str(&core_sim::BUILTIN_FAUNA_CONFIG.replace(
        r#""combat": { "attack": 8.0, "defense": 12.0, "range": "melee" }"#,
        r#""combat": { "attack": -1.0, "defense": 12.0, "range": "melee" }"#,
    ));
    assert!(
        negative_attack.is_err(),
        "a negative combat.attack must be rejected"
    );

    // Aggression outside [0, 1] is rejected.
    let bad_aggression = FaunaConfig::from_json_str(&core_sim::BUILTIN_FAUNA_CONFIG.replace(
        r#""husbandry_ceiling": "wild",
      "combat": { "attack": 8.0, "defense": 12.0, "range": "melee" }"#,
        r#""husbandry_ceiling": "wild",
      "aggression": 1.5,
      "combat": { "attack": 8.0, "defense": 12.0, "range": "melee" }"#,
    ));
    assert!(
        bad_aggression.is_err(),
        "an aggression outside [0, 1] must be rejected"
    );
}

/// The resolver-tuning and creatures loaders reject their own broken dials.
#[test]
fn combat_and_creatures_configs_reject_broken_dials() {
    assert!(
        CombatConfig::from_json_str(r#"{ "lethality": 0.0, "disengage_fraction": 0.5 }"#).is_err(),
        "a zero lethality (bloodless fights) must be rejected"
    );
    assert!(
        CombatConfig::from_json_str(r#"{ "lethality": 1.0, "disengage_fraction": 1.5 }"#).is_err(),
        "a disengage_fraction above 1 must be rejected"
    );
    assert!(
        CreaturesConfig::from_json_str(
            r#"{ "creatures": { "wolf": { "combat": { "attack": 3.0, "defense": 2.0 } } } }"#
        )
        .is_err(),
        "a roster missing the base person must be rejected"
    );
}
