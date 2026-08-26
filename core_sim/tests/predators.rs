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

use bevy::math::UVec2;

use core_sim::{
    advance_herds, advance_labor_allocation, advance_predation, build_test_app, scalar_from_f32,
    scalar_one, scalar_zero, spawn_initial_forage, spawn_initial_herds, spawn_initial_world,
    CombatConfig, CommandEventLog, CreaturesConfig, CultureManager, Diet, DiscoveryProgressLedger,
    FactionId, FactionInventory, FaunaConfig, FaunaConfigHandle, ForageRegistry, GenerationId,
    GenerationRegistry, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation,
    LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, SimulationConfig, SimulationTick, SizeClass,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, SourcePriority, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, TileRegistry,
    WellbeingConfigHandle,
};

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
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
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
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
            floor: 0.3,
        },
        workers: hunters,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
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
/// The shipped `person` row's `combat.durability` (`creatures.json`) — how much damage a body soaks
/// before it goes down (`docs/plan_hunt_through_combat.md` §4.2). Restated here so the hand-built
/// fight below is the roster's human, not a neutral stand-in.
const PERSON_DURABILITY: f32 = 20.0;
/// The shipped mammoth's `combat.durability` (`fauna_config.json`), for the same reason.
const MAMMOTH_DURABILITY: f32 = 500.0;

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
                            durability: PERSON_DURABILITY,
                            range: RangeBand::Melee,
                            wariness: 0.0,
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
                            durability: MAMMOTH_DURABILITY,
                            range: RangeBand::Melee,
                            wariness: 0.0,
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

/// The mammoth's shipped `combat` block, verbatim — the substring these rejection fixtures swap out
/// of the builtin config. Named because a silent mismatch turns every `is_err()` below into an
/// assertion that the *unmodified* builtin fails to parse, which it never will; the
/// `the_rejection_fixtures_still_match_the_shipped_config` guard below is what makes that loud.
const MAMMOTH_COMBAT_BLOCK: &str = r#""combat": { "attack": 8.0, "defense": 12.0, "durability": 500, "wariness": 0.10, "range": "melee" }"#;

/// The wolf's shipped `combat` block, verbatim — same role, for the carnivore fixtures.
const WOLF_COMBAT_BLOCK: &str =
    r#""attack": 3.0, "defense": 3.0, "durability": 20, "wariness": 0.70, "range": "melee""#;

/// **A rejection fixture that no longer matches asserts nothing.** Every `is_err()` below works by
/// string-replacing a block of the shipped config; if the block is re-authored and the fixture is
/// not, `replace` is a no-op and the test silently starts checking that the builtin is invalid.
#[test]
fn the_rejection_fixtures_still_match_the_shipped_config() {
    for block in [MAMMOTH_COMBAT_BLOCK, WOLF_COMBAT_BLOCK] {
        assert!(
            core_sim::BUILTIN_FAUNA_CONFIG.contains(block),
            "the shipped fauna config no longer contains `{block}` — every rejection fixture keyed \
             on it is now a no-op"
        );
    }
}

/// Config validation rejects the illegitimate combat/aggression dials.
#[test]
fn fauna_config_rejects_illegitimate_combat_dials() {
    // **A zero `combat.defense` is now LEGAL and authored** — it is the hard gate of
    // `docs/plan_hunt_through_combat.md` §4.2, and `0` is the meaningful statement *"no protection at
    // all"* that the five small-game rows carry (a bare-handed band can still take a rabbit). What is
    // rejected is a NEGATIVE one, which would hand an attacker free damage.
    let negative_defense = FaunaConfig::from_json_str(&core_sim::BUILTIN_FAUNA_CONFIG.replace(
        MAMMOTH_COMBAT_BLOCK,
        r#""combat": { "attack": 8.0, "defense": -1.0, "durability": 500, "wariness": 0.10, "range": "melee" }"#,
    ));
    assert!(
        negative_defense.is_err(),
        "a negative combat.defense must be rejected"
    );

    // **A zero `combat.durability` IS rejected** — it is the attrition denominator
    // (`units_down = damage / durability`), so `0` would let one point of damage bring down every
    // animal in the engagement. Unlike `defense` there is no coherent zero: a body that soaks nothing
    // is not "unprotected", it is absent.
    let zero_durability = FaunaConfig::from_json_str(&core_sim::BUILTIN_FAUNA_CONFIG.replace(
        MAMMOTH_COMBAT_BLOCK,
        r#""combat": { "attack": 8.0, "defense": 12.0, "durability": 0.0, "wariness": 0.10, "range": "melee" }"#,
    ));
    assert!(
        zero_durability.is_err(),
        "combat.durability of 0 (the attrition denominator) must be rejected"
    );

    // A negative attack is rejected.
    let negative_attack = FaunaConfig::from_json_str(&core_sim::BUILTIN_FAUNA_CONFIG.replace(
        MAMMOTH_COMBAT_BLOCK,
        r#""combat": { "attack": -1.0, "defense": 12.0, "durability": 500, "wariness": 0.10, "range": "melee" }"#,
    ));
    assert!(
        negative_attack.is_err(),
        "a negative combat.attack must be rejected"
    );

    // **A wariness outside [0, 1] is rejected — and a NaN one especially**
    // (`docs/plan_hunt_through_combat.md` §3.1). Since slice 7 every species carries a real value, so
    // a retune is a live way to author a bad one. `fauna::animals_that_stay` clamps with `.min(1.0)`,
    // which would hide an over-1 slip; a `NaN` is worse than hidden, because `NaN <= 0.0` is false so
    // the zero-identity early return is skipped and `NaN.min(1.0)` is `1.0` in Rust — every engaged
    // animal retreats and the species' take is **silently zero on every hunt**.
    for bad in [
        r#""combat": { "attack": 8.0, "defense": 12.0, "durability": 500, "wariness": 1.5, "range": "melee" }"#,
        r#""combat": { "attack": 8.0, "defense": 12.0, "durability": 500, "wariness": -0.1, "range": "melee" }"#,
        r#""combat": { "attack": 8.0, "defense": 12.0, "durability": 500, "wariness": null, "range": "melee" }"#,
    ] {
        assert!(
            FaunaConfig::from_json_str(
                &core_sim::BUILTIN_FAUNA_CONFIG.replace(MAMMOTH_COMBAT_BLOCK, bad)
            )
            .is_err(),
            "a combat.wariness outside [0, 1] must be rejected: {bad}"
        );
    }

    // Aggression outside [0, 1] is rejected.
    let bad_aggression = FaunaConfig::from_json_str(&core_sim::BUILTIN_FAUNA_CONFIG.replace(
        MAMMOTH_COMBAT_BLOCK,
        &format!("\"aggression\": 1.5,\n      {MAMMOTH_COMBAT_BLOCK}"),
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

// ---------------------------------------------------------------------------------------------------
// Phase 1a — carnivore herds: diet + prey-limited carrying capacity + the predation draw-down.
// ---------------------------------------------------------------------------------------------------

/// The shipped carnivore. `attack 3` clears deer/boar (defense ≤ 3) but not aurochs (6) or mammoth (12).
const WOLF: &str = "Grey Wolf Pack";
/// A herbivore whose `combat.defense` (1.0) the wolf's attack clears — prey.
const DEER: &str = "Red Deer";
/// A herbivore whose `combat.defense` (6.0) the wolf's attack does NOT clear — not prey (idea 7).
const AUROCHS: &str = "Wild Aurochs";

/// A minimal app carrying just the three resources `advance_predation` reads — no world needed, since
/// predation is a pure draw-down over the `HerdRegistry` keyed on config levers.
fn predation_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.world.insert_resource(SimulationConfig::builtin());
    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app
}

/// Seat a hand-built **stationary** (Small, single-tile) herd of `species` at `pos`. `fodder_per_biomass`
/// is `0.0` (so a herbivore's `K` stays its constant `cap` and a wolf never grazes) and `body_mass`
/// is irrelevant to predation; `r` is the cached wild regrowth rate the prey index reads.
fn seat(app: &mut App, id: &str, species: &str, pos: UVec2, biomass: f32, cap: f32, r: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.push(Herd::new(
        id.to_string(),
        species.to_string(),
        SizeClass::Small,
        vec![pos],
        biomass,
        cap,
        0.0,
        r,
        30.0,
    ));
}

/// The current biomass of the herd with `id`.
fn biomass_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|h| h.id == id)
        .map(|h| h.biomass)
        .unwrap_or_else(|| panic!("herd {id} present"))
}

/// A full earthlike world with the herd registry **empty** (so tests seat herds by hand). Returns the
/// app plus a guaranteed **land tile** (a tile a real herd spawned on) to seat the fixtures on.
fn seeded_world() -> (App, UVec2) {
    let mut app = spawn_world();
    let land = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .map(|h| h.position())
            .next()
            .expect("the world seeded at least one herd")
    };
    app.world.resource_mut::<HerdRegistry>().herds.clear();
    (app, land)
}

/// A wolf pack next to a healthy deer herd draws the deer's biomass **down** (predation take > 0).
#[test]
fn a_wolf_pack_draws_a_nearby_deer_herd_down() {
    let mut app = predation_app();
    seat(
        &mut app,
        "pred_wolf",
        WOLF,
        UVec2::new(10, 10),
        120.0,
        120.0,
        0.15,
    );
    seat(
        &mut app,
        "game_deer",
        DEER,
        UVec2::new(11, 10),
        1200.0,
        1200.0,
        0.10,
    );

    let before = biomass_of(&app, "game_deer");
    app.world.run_system_once(advance_predation);
    let after = biomass_of(&app, "game_deer");
    assert!(
        after < before,
        "wolf predation must draw the deer herd down: {before} -> {after}"
    );
}

/// **Idea 7, for free.** A wolf (`attack 3`) does not count a mammoth (`defense 12`) or an aurochs
/// (`defense 6`) as prey — its predation ignores them (zero draw), because the pure `attack ≥ defense`
/// rule leaves them out of its prey set.
#[test]
fn a_wolf_ignores_prey_it_cannot_bring_down() {
    let mut app = predation_app();
    seat(
        &mut app,
        "pred_wolf",
        WOLF,
        UVec2::new(10, 10),
        120.0,
        120.0,
        0.15,
    );
    seat(
        &mut app,
        "game_mammoth",
        MAMMOTH,
        UVec2::new(10, 10),
        9000.0,
        12000.0,
        0.04,
    );
    seat(
        &mut app,
        "game_aurochs",
        AUROCHS,
        UVec2::new(11, 10),
        900.0,
        1000.0,
        0.09,
    );

    let mammoth_before = biomass_of(&app, "game_mammoth");
    let aurochs_before = biomass_of(&app, "game_aurochs");
    app.world.run_system_once(advance_predation);
    assert_eq!(
        biomass_of(&app, "game_mammoth"),
        mammoth_before,
        "a wolf must not touch a mammoth (defense 12 > attack 3)"
    );
    assert_eq!(
        biomass_of(&app, "game_aurochs"),
        aurochs_before,
        "a wolf must not touch an aurochs (defense 6 > attack 3)"
    );
}

/// A **carnivore-free** map leaves predation a pure no-op: every herbivore's biomass is byte-identical
/// after `advance_predation` (pins that the herbivore path did not regress).
#[test]
fn a_carnivore_free_map_leaves_predation_a_no_op() {
    let mut app = predation_app();
    seat(
        &mut app,
        "game_deer",
        DEER,
        UVec2::new(10, 10),
        1200.0,
        1200.0,
        0.10,
    );
    seat(
        &mut app,
        "game_aurochs",
        AUROCHS,
        UVec2::new(11, 10),
        900.0,
        1000.0,
        0.09,
    );
    seat(
        &mut app,
        "game_mammoth",
        MAMMOTH,
        UVec2::new(12, 10),
        9000.0,
        12000.0,
        0.04,
    );

    let before: Vec<f32> = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|h| h.biomass)
        .collect();
    app.world.run_system_once(advance_predation);
    let after: Vec<f32> = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|h| h.biomass)
        .collect();
    assert_eq!(
        before, after,
        "no carnivore ⇒ predation must not touch any herd"
    );
}

/// **Determinism.** Two identical predation setups draw bit-identical prey biomass.
#[test]
fn predation_is_deterministic() {
    let run = || -> Vec<f32> {
        let mut app = predation_app();
        // Two wolves and two deer at varied distances — a non-trivial proportional split.
        seat(
            &mut app,
            "pred_a",
            WOLF,
            UVec2::new(10, 10),
            120.0,
            120.0,
            0.15,
        );
        seat(
            &mut app,
            "pred_b",
            WOLF,
            UVec2::new(12, 11),
            90.0,
            120.0,
            0.15,
        );
        seat(
            &mut app,
            "game_d1",
            DEER,
            UVec2::new(11, 10),
            1200.0,
            1200.0,
            0.10,
        );
        seat(
            &mut app,
            "game_d2",
            DEER,
            UVec2::new(12, 10),
            900.0,
            1200.0,
            0.10,
        );
        app.world.run_system_once(advance_predation);
        app.world
            .resource::<HerdRegistry>()
            .herds
            .iter()
            .map(|h| h.biomass)
            .collect()
    };
    assert_eq!(
        run(),
        run(),
        "predation must be bit-identical across identical runs"
    );
}

/// A wolf pack's carrying capacity **tracks its prey**: with a deer herd in range its `K_pred` is high
/// and the pack grows toward it; remove the prey and `K_pred → 0`, the pack collapses and is
/// **despawned** from the registry (idea 6 — no game, they leave/die).
#[test]
fn a_wolf_packs_capacity_tracks_its_prey_and_it_despawns_when_prey_vanish() {
    let (mut app, land) = seeded_world();
    seat(&mut app, "pred_wolf", WOLF, land, 60.0, 120.0, 0.15);
    seat(&mut app, "game_deer", DEER, land, 1200.0, 1200.0, 0.10);

    // Grow the pack toward its prey-derived K over a few turns.
    let start = biomass_of(&app, "pred_wolf");
    for _ in 0..6 {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_predation);
    }
    let wolf = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id == "pred_wolf")
            .cloned()
            .expect("the wolf pack is alive while prey are present")
    };
    assert!(
        wolf.carrying_capacity > 0.0,
        "with deer in range the wolf's K_pred must be positive: {}",
        wolf.carrying_capacity
    );
    assert!(
        wolf.biomass > start,
        "the pack must grow toward its prey-derived K: {start} -> {}",
        wolf.biomass
    );

    // Remove all prey — the wolf's K_pred collapses to 0, `regrow_biomass`'s clamp drives its biomass
    // down, and the extinction `retain` despawns it.
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .retain(|h| h.id != "game_deer");
    app.world.run_system_once(advance_herds);
    assert!(
        !app.world
            .resource::<HerdRegistry>()
            .herds
            .iter()
            .any(|h| h.id == "pred_wolf"),
        "with no prey in range the pack's K_pred → 0 and it must despawn"
    );
}

/// The dedicated predator pass seats **only carnivores**, and the herbivore short-range pool excludes
/// them (so the wolf never seats from — or immigrates through — the prey budget).
#[test]
fn the_carnivore_pool_is_separate_from_the_herbivore_pool() {
    let fauna = FaunaConfig::builtin();
    let carnivores = fauna.carnivore_species_for_biome("temperate_forest");
    assert!(
        carnivores.iter().any(|(_, def)| def.display_name == WOLF),
        "the wolf must be in the carnivore pool for a biome it hosts"
    );
    let herbivores = fauna.game_species_for_biome("temperate_forest");
    assert!(
        !herbivores.iter().any(|(_, def)| def.display_name == WOLF),
        "the wolf must NOT be in the herbivore short-range/immigration pool"
    );
    assert!(
        herbivores.iter().any(|(_, def)| def.display_name == DEER),
        "the herbivore pool must still carry its herbivores (deer)"
    );
}

/// Config validation rejects the illegitimate carnivore/predator-block dials (one per bound, mirroring
/// the ferocity/aggression rejection tests).
#[test]
fn fauna_config_rejects_illegitimate_predator_dials() {
    let builtin = core_sim::BUILTIN_FAUNA_CONFIG;

    // A carnivore with a `0` prey_per_biomass — an infinite-K denominator — is rejected.
    assert!(
        FaunaConfig::from_json_str(
            &builtin.replace(r#""prey_per_biomass": 0.3,"#, r#""prey_per_biomass": 0.0,"#)
        )
        .is_err(),
        "a carnivore's prey_per_biomass of 0 (a denominator) must be rejected"
    );

    // A carnivore whose attack clears no defense (empty prey set) is rejected.
    assert!(
        FaunaConfig::from_json_str(&builtin.replace(
            WOLF_COMBAT_BLOCK,
            r#""attack": 0.0, "defense": 3.0, "durability": 20, "wariness": 0.70, "range": "melee""#,
        ))
        .is_err(),
        "a carnivore with combat.attack 0 must be rejected"
    );

    // predation_escapement_fraction outside (0, 0.5) is rejected.
    assert!(
        FaunaConfig::from_json_str(&builtin.replace(
            r#""predation_escapement_fraction": 0.15,"#,
            r#""predation_escapement_fraction": 0.6,"#,
        ))
        .is_err(),
        "a predation_escapement_fraction ≥ the prey MSY point must be rejected"
    );

    // A carnivore with a non-positive prey_ratio — a pack count of 0, an incoherent predator — is
    // rejected (mirroring the prey_per_biomass / carnivore-attack bounds).
    assert!(
        FaunaConfig::from_json_str(
            &builtin.replace(r#""prey_ratio": 0.10,"#, r#""prey_ratio": 0.0,"#)
        )
        .is_err(),
        "a carnivore's prey_ratio of 0 (no packs seeded) must be rejected"
    );
    assert!(
        FaunaConfig::from_json_str(
            &builtin.replace(r#""prey_ratio": 0.10,"#, r#""prey_ratio": -0.1,"#)
        )
        .is_err(),
        "a carnivore's negative prey_ratio must be rejected"
    );

    // A prey_sense_radius of 0 (a disk that senses no prey) is rejected.
    assert!(
        FaunaConfig::from_json_str(
            &builtin.replace(r#""prey_sense_radius": 4"#, r#""prey_sense_radius": 0"#)
        )
        .is_err(),
        "a prey_sense_radius of 0 must be rejected"
    );

    // A predator min_spacing of 0 (stacked packs) is rejected.
    assert!(
        FaunaConfig::from_json_str(
            &builtin.replace(r#""min_spacing": 6,"#, r#""min_spacing": 0,"#)
        )
        .is_err(),
        "a predators.min_spacing of 0 must be rejected"
    );

    // A per-biome predator probability above 1 is rejected.
    assert!(
        FaunaConfig::from_json_str(&builtin.replace(
            r#""savanna_grassland": 0.02,"#,
            r#""savanna_grassland": 1.5,"#,
        ))
        .is_err(),
        "a predators.per_biome probability above 1 must be rejected"
    );
}

// ---- The prey-derived spawn count (Predators Phase 1a) ---------------------------------------------

/// The standard map for the predator-count sweep (mirrors `fauna_wet_biome_roster`'s `GRID`).
const PRED_GRID: UVec2 = UVec2::new(80, 52);
/// The predator-count sweep seeds (mirrors `fauna_wet_biome_roster`'s `SWEEP_SEEDS`).
const PRED_SWEEP_SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 119_304_647];
/// The id prefix `fauna::spawn_predators` stamps on every seeded pack (mirrors the private
/// `fauna::PREDATOR_ID_PREFIX`), so a test can count packs apart from prey.
const PRED_ID_PREFIX: &str = "pred_";

/// One seed's predator/prey census, read off the real Startup chain.
struct PredCensus {
    /// Herbivore herds the wolf's `attack` clears (its prey set), map-wide.
    prey_herds: usize,
    /// `pred_`-prefixed packs actually seeded.
    packs: usize,
    /// The prey-derived target `round(prey_herds × prey_ratio)`.
    target: usize,
    /// The prey-derived `K` at each seeded pack's tile, measured with `carnivore_k_at` against the
    /// spawn-time prey index — the exact gate `spawn_predator_group_at` places under.
    pack_k: Vec<f32>,
}

/// Generate `seed`'s standard earthlike map through the real Startup chain (worldgen → … →
/// `spawn_initial_herds` → `spawn_predators`) and census its prey vs its seeded packs. Startup is
/// driven **once, by hand** (the `fauna_wet_biome_roster::survey` idiom — `app.update()` would
/// double-run worldgen).
fn predator_census(seed: u64) -> PredCensus {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = PRED_GRID;
    app.world.insert_resource(config);
    app.world.run_schedule(bevy::app::Startup);

    let fauna = FaunaConfig::builtin();
    let wolf = fauna
        .species_by_display(WOLF)
        .expect("the wolf is in the shipped roster");

    let width = PRED_GRID.x;
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;

    let registry = app.world.resource::<HerdRegistry>();
    // The spawn-time prey index (herbivore herds), the same layer the spawn gate measured its `K`
    // against — herds have not moved since Startup only spawned them.
    let prey_index = core_sim::build_prey_index(&registry.herds, &fauna);
    let mut prey_herds = 0usize;
    let mut packs = 0usize;
    let mut pack_k = Vec::new();
    for herd in &registry.herds {
        if herd.id.starts_with(PRED_ID_PREFIX) {
            packs += 1;
            pack_k.push(core_sim::carnivore_k_at(
                herd.current_pos,
                wolf.combat.attack,
                wolf.prey_per_biomass,
                &prey_index,
                fauna.predators.prey_sense_radius,
                width,
                wrap,
            ));
            continue;
        }
        // Prey = a herbivore herd whose defense the wolf's attack clears (the same rule the K seam and
        // advance_predation read). An unresolved species (never on a real map) defaults to herbivore
        // with the default combat defense — the sim's own fallback.
        let def = fauna.species_by_display(&herd.species);
        let is_carnivore = def.map(|d| d.diet) == Some(Diet::Carnivore);
        let defense = def.map_or_else(
            || core_sim::CombatStats::default().defense,
            |d| d.combat.defense,
        );
        if !is_carnivore && defense <= wolf.combat.attack {
            prey_herds += 1;
        }
    }
    let target = (prey_herds as f32 * wolf.prey_ratio).round() as usize;
    PredCensus {
        prey_herds,
        packs,
        target,
        pack_k,
    }
}

/// **The predator count SCALES WITH THE PREY BASE** (Predators Phase 1a) — the permanent guard that
/// `spawn_predators` seats `round(eligible_prey_herds × prey_ratio)` packs, not a fixed absolute cap.
/// Measured on [`PRED_SWEEP_SEEDS`] (earthlike 80×52, `prey_ratio 0.10`): prey herds 97–108 →
/// **target 10–11**, packs **11 / 6 / 10 / 11 / 10 / 11** — five seeds hit the prey-derived target
/// exactly, and seed 2 is **placement-starved** (the shipped `min_spacing 6` + low per-biome
/// probabilities cannot seat 11 *well-spaced* packs on that particular winner distribution). So the
/// invariants are:
/// - **the derived target is a real ceiling** — `packs ≤ target + 1` on every seed (this *fails* under
///   the retired fixed `max_packs 4` whenever `target > 5`, so it is exactly the "cap is prey-derived"
///   claim); and
/// - **the target genuinely sizes the population** — a **majority** of seeds reach it (`packs ≥
///   target − 1`), so the count tracks the prey base rather than a placement floor; and
/// - the sweep seats a non-zero total.
///
/// The number that varies with a retune is which seeds are placement-starved, so the majority bound is
/// deliberately loose. **If more seeds starve, the lever is `min_spacing` / `predators.per_biome`, not
/// this test** (raising the placement ceiling toward the prey-derived target).
///
/// **The prey-gated spawn (Phase 1a) is now co-active** — a winning tile only seats a species whose
/// prey-derived `K` reaches its minimum spawn biomass — and it is a **measured no-op on these seeds**:
/// prey is dense enough on the standard earthlike map that every wolf-host tile that wins already
/// clears the gate, so the counts are byte-identical to the pre-gate sweep (11 / 6 / 10 / 11 / 10 /
/// 11). The gate can only ever *lower* a count (never raise one), so the `packs ≤ target + 1` ceiling
/// still holds; on a prey-sparse map it is what would pull `packs` further below `target`, which is
/// correct — a viable pack near game beats a stranded one (see
/// `every_spawned_predator_lands_where_the_prey_can_feed_it`). If a future prey retune starves the
/// majority bound, relax it: the ceiling + the no-stranded-pack invariant are the load-bearing guards.
#[test]
fn the_predator_count_scales_with_the_prey_base() {
    let mut total_packs = 0usize;
    let mut reached_target = 0usize;
    for seed in PRED_SWEEP_SEEDS {
        let census = predator_census(seed);
        total_packs += census.packs;
        assert!(
            census.packs <= census.target + 1,
            "seed {seed}: {} packs seeded exceeds the prey-derived target {} (from {} prey herds) — \
             the count must be prey-CAPPED, not a fixed absolute",
            census.packs,
            census.target,
            census.prey_herds,
        );
        if census.packs + 1 >= census.target {
            reached_target += 1;
        }
    }
    assert!(
        total_packs > 0,
        "the sweep must seat at least one predator pack across {} seeds",
        PRED_SWEEP_SEEDS.len(),
    );
    assert!(
        reached_target * 2 >= PRED_SWEEP_SEEDS.len(),
        "a majority of seeds must REACH the prey-derived target (only {reached_target}/{} did) — \
         else the count is placement-floored, not prey-sized",
        PRED_SWEEP_SEEDS.len(),
    );
}

/// **NO STRANDED PACK** (Predators Phase 1a — the direct encoding of the prey-gated spawn). Every
/// seeded pack lands on a tile whose prey-derived `K` reaches at least the wolf's **minimum spawn
/// biomass** (the low end of its `biomass` range), so no pack is stillborn on prey-sparse ground with
/// `K ≈ 0` that the extinction `retain` would despawn within a handful of turns. Measured with the SAME
/// `carnivore_k_at` formula the live per-turn K and the spawn gate share, against the spawn-time prey
/// index — swept over every [`PRED_SWEEP_SEEDS`] seed and every seeded pack.
#[test]
fn every_spawned_predator_lands_where_the_prey_can_feed_it() {
    let fauna = FaunaConfig::builtin();
    let wolf = fauna
        .species_by_display(WOLF)
        .expect("the wolf is in the shipped roster");
    let min_spawn = wolf.min_spawn_biomass();
    let mut total = 0usize;
    for seed in PRED_SWEEP_SEEDS {
        let census = predator_census(seed);
        for k in census.pack_k {
            total += 1;
            assert!(
                k >= min_spawn,
                "seed {seed}: a pack spawned on a tile whose prey-derived K {k} is below the wolf's \
                 minimum spawn biomass {min_spawn} — the prey gate must never seat a stranded pack",
            );
        }
    }
    assert!(
        total > 0,
        "the sweep must seat at least one pack for the invariant to mean anything",
    );
}

/// The per-seed prey/wolf numbers, for eyeballing the new prey-derived dynamic. Run with
/// `cargo test -p core_sim --test predators -- --ignored --nocapture predator_count_report`.
#[test]
#[ignore = "measurement report, run explicitly with --nocapture"]
fn predator_count_report() {
    println!("prey-derived predator count — earthlike 80×52, prey_ratio 0.10");
    println!("seed            prey_herds  target  packs");
    for seed in PRED_SWEEP_SEEDS {
        let c = predator_census(seed);
        println!(
            "{seed:<14}  {:>10}  {:>6}  {:>5}",
            c.prey_herds, c.target, c.packs
        );
    }
}
