//! Predators Phase 0 — the hunt-danger casualty path (`docs/plan_predators.md`).
//!
//! A band hunting a herd whose species can fight back (`combat.attack > 0` — mammoth, ox) takes
//! **working-age casualties** through the combat subsystem — a net-new mortality path. The hunting
//! party answers the danger itself (via the hunters' own equipment, TOE deferred); Warriors are a
//! band-wide guard and do **not** mitigate a hunt. These tests drive a **real**
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

/// Spawn a hunting band on `tile` with `hunters` assigned to the herd.
fn hunting_band(
    app: &mut App,
    tile: bevy::prelude::Entity,
    working: u32,
    fauna_id: &str,
    hunters: u32,
) -> bevy::prelude::Entity {
    let assignments = vec![LaborAssignment {
        target: LaborTarget::Hunt {
            fauna_id: fauna_id.to_string(),
            policy: FollowPolicy::Surplus,
        },
        workers: hunters,
    }];
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

/// The mammoth's shipped display name — `{ attack 8, defense 12, ferocity 0.9 }`: strong AND it
/// fights back, so it is deadly to hunt.
const MAMMOTH: &str = "Thunder Mammoths";
/// A fully-defaulted species (attack 0, ferocity 0) — a harmless hunt. **Not a deer:** since the
/// roster split, a deer has a small nonzero effective attack (`0.8 × 0.15`), so "harms nobody" must
/// use a species with no combat block at all.
const RABBIT: &str = "Rabbit Warren";

/// A band hunting a **mammoth** (effective attack `8 × 0.9`) loses working-age population over a
/// turn, and the hunt-danger feed line reports a `killed`/`wounded` split (both modelled from day one).
#[test]
fn a_dangerous_mammoth_hunt_costs_working_age_lives() {
    let mut app = spawn_world();
    let (herd, tile) = dangerous_herd(&mut app, MAMMOTH);
    let band = hunting_band(&mut app, tile, 60, &herd, 4);
    let before = working_of(&app, band);
    app.world.run_system_once(advance_labor_allocation);
    let after = working_of(&app, band);
    let killed = before - after;
    assert!(
        killed > 0.0,
        "a mammoth (attack 8 × ferocity 0.9) hunt must cost working-age lives: {before} -> {after}"
    );

    let (feed_killed, feed_wounded) =
        last_danger_casualties(&app).expect("a dangerous hunt pushes a hunt_danger feed line");
    assert!(feed_killed > 0.0, "some hunters are killed: {feed_killed}");
    assert!(
        feed_wounded > 0.0,
        "some hunters are wounded: {feed_wounded}"
    );
    // The bracket loss (working-age removal) matches the feed's reported dead.
    assert!((killed - feed_killed).abs() < 1e-2);
}

/// A band hunting a **rabbit** (no combat block → attack 0, ferocity 0) loses **nobody**.
#[test]
fn a_harmless_hunt_costs_no_lives() {
    let mut app = spawn_world();
    let (herd, tile) = dangerous_herd(&mut app, RABBIT);
    let band = hunting_band(&mut app, tile, 60, &herd, 4);
    let before = working_of(&app, band);
    app.world.run_system_once(advance_labor_allocation);
    let after = working_of(&app, band);
    assert_eq!(
        before, after,
        "a harmless (attack 0) hunt must not touch working-age population: {before} -> {after}"
    );
    // ...and no hunt-danger feed line is pushed.
    assert!(
        last_danger_casualties(&app).is_none(),
        "a harmless hunt must push no hunt_danger event"
    );
}

/// **Ferocity scales hunt-danger**: a strong animal that mostly *flees* (low ferocity) inflicts far
/// fewer casualties than the same strength at high ferocity — the adapter feeds the resolver
/// `attack × ferocity`, so the two differ only in that product. Asserted via a direct `resolve_fight`
/// comparison (the two effective attacks a high-attack beast would present at low vs high ferocity).
#[test]
fn ferocity_scales_the_danger_of_a_hunt() {
    use core_sim::{
        resolve_fight, CombatStats, CombatTuning, Contingent, ContingentId, FightPayload, Force,
        ForceId, Posture, RangeBand,
    };

    // Same strong beast (attack 8, defense 12), same party — only ferocity differs.
    let band_losses_at = |ferocity: f32| -> f32 {
        let payload = FightPayload {
            sides: vec![
                Force {
                    id: ForceId(0),
                    posture: Posture::Aggressor,
                    contingents: vec![Contingent {
                        kind: ContingentId::from("person"),
                        count: 4.0,
                        profile: CombatStats {
                            attack: 1.0,
                            defense: 1.0,
                            range: RangeBand::Melee,
                        },
                    }],
                },
                Force {
                    id: ForceId(1),
                    posture: Posture::Defender,
                    contingents: vec![Contingent {
                        kind: ContingentId::from("beast"),
                        count: 1.0,
                        profile: CombatStats {
                            attack: 8.0 * ferocity, // the adapter's `attack × ferocity`
                            defense: 12.0,
                            range: RangeBand::Melee,
                        },
                    }],
                },
            ],
            terrain: vec![],
            seed: 0,
        };
        let out = resolve_fight(&payload, &CombatTuning::default());
        out.results
            .iter()
            .find(|r| r.force == ForceId(0))
            .map(|r| r.killed + r.wounded)
            .unwrap_or(0.0)
    };

    let timid = band_losses_at(0.15);
    let fierce = band_losses_at(0.9);
    assert!(
        fierce > timid * 2.0,
        "a fierce beast must be far deadlier than a fleeing one: timid {timid}, fierce {fierce}"
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

    // Ferocity outside [0, 1] is rejected (the mammoth is the only 0.9 in the roster).
    let bad_ferocity = FaunaConfig::from_json_str(
        &core_sim::BUILTIN_FAUNA_CONFIG.replace(r#""ferocity": 0.9"#, r#""ferocity": 1.5"#),
    );
    assert!(
        bad_ferocity.is_err(),
        "a ferocity outside [0, 1] must be rejected"
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
