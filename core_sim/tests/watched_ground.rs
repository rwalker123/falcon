//! A band's watched ring keeps wild predators off (#875, `.claude/rules/core_sim/fauna.md`).
//!
//! The ring is every tile within `reach_tiles + scouting_reach` hex steps of a band that has scouts
//! reaching past its local range. A wild carnivore inside it steps out on its move turns, neither
//! `pursue` nor the roam may end a step inside it, and `pursue` ignores prey standing in it. These
//! tests drive the real `advance_herds` / `advance_predator_raids` over a real map, with a control
//! arm (no scouts) beside every claim.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};
use bevy::MinimalPlugins;
use core_sim::grid_utils::{hex_distance_wrapped, hex_neighbor};
use core_sim::shipped;
use core_sim::staffing::ScoutCrew;

use core_sim::{
    advance_herds, advance_predator_raids, scalar_from_f32, scalar_one, scalar_zero,
    spawn_initial_forage, spawn_initial_herds, spawn_initial_world, BandId, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle,
    ForageRegistry, GenerationId, GenerationRegistry, Herd, HerdDensityMap, HerdRegistry,
    HerdTelemetry, LaborAllocation, LaborConfigHandle, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, RoamState, SimulationConfig,
    SimulationTick, SizeClass, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
    WellbeingConfigHandle,
};
use sim_runtime::TerrainTags;

const WOLF: &str = "Grey Wolf Pack";
const DEER: &str = "Red Deer";
/// Tiles the scouts reach past the local range in the ring fixtures.
const REACH: u32 = 2;
/// The shipped `supply_network_config.json` `reach_tiles`.
const LOCAL_RANGE: u32 = 3;
/// The ring's radius in those fixtures.
const RING: u32 = LOCAL_RANGE + REACH;
/// The turns the movement and raid arms run.
const TURNS: u32 = 14;
const BAND_ID: u64 = 7;
const WORKING: u32 = 30;

fn arena() -> (App, UVec2, Entity) {
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
        .insert_resource(core_sim::EquipmentConfigHandle::for_a_stocked_fixture());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
    app.world
        .insert_resource(core_sim::ExtractionConfigHandle::default());
    app.world
        .insert_resource(core_sim::extraction::DepositRegistry::default());
    app.world
        .insert_resource(core_sim::BeliefRegistry::default());
    app.world
        .insert_resource(core_sim::BeliefConfigHandle::default());
    app.world
        .insert_resource(core_sim::SupplyNetworkConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_forage);

    let pos = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|h| h.position())
        .next()
        .expect("the world seeded at least one herd");
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("the land tile resolves");
    app.world.resource_mut::<HerdRegistry>().herds.clear();
    let existing: Vec<Entity> = app
        .world
        .query_filtered::<Entity, With<PopulationCohort>>()
        .iter(&app.world)
        .collect();
    for entity in existing {
        app.world.despawn(entity);
    }
    (app, pos, tile)
}

fn grid(app: &App) -> (u32, u32, bool) {
    let config = app.world.resource::<SimulationConfig>();
    (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    )
}

fn distance(app: &App, a: UVec2, b: UVec2) -> u32 {
    let (width, _, wrap) = grid(app);
    hex_distance_wrapped(a, b, width, wrap)
}

fn is_land(app: &mut App, pos: UVec2) -> bool {
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("a tile");
    !app.world
        .get::<Tile>(entity)
        .unwrap()
        .terrain_tags
        .contains(TerrainTags::WATER)
}

/// Land tiles exactly `radius` hex steps from `from`, in row-major order.
fn land_at(app: &mut App, from: UVec2, radius: u32) -> Vec<UVec2> {
    let (width, height, _) = grid(app);
    let mut found = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pos = UVec2::new(x, y);
            if distance(app, from, pos) == radius && is_land(app, pos) {
                found.push(pos);
            }
        }
    }
    found
}

/// A land neighbour of `pos` at hex distance `want` from `from`.
fn neighbour_at(app: &mut App, pos: UVec2, from: UVec2, want: u32) -> Option<UVec2> {
    let (width, height, wrap) = grid(app);
    (0..6).find_map(|dir| {
        let (x, y) = hex_neighbor(pos.x, pos.y, dir, width, height, wrap)?;
        let n = UVec2::new(x, y);
        (distance(app, n, from) == want && is_land(app, n)).then_some(n)
    })
}

fn seat(app: &mut App, id: &str, species: &str, route: Vec<UVec2>) {
    app.world
        .resource_mut::<HerdRegistry>()
        .herds
        .push(Herd::new(
            id.to_string(),
            species.to_string(),
            SizeClass::Small,
            route,
            120.0,
            120.0,
            0.0,
            0.15,
            30.0,
            shipped::root_standing(shipped::animal()),
        ));
}

fn herd_pos(app: &App, id: &str) -> UVec2 {
    app.world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|herd| herd.id == id)
        .expect("the herd is still on the map")
        .position()
}

/// A resident band on `tile`; with `reach > 0` its Scout row reaches that far past the local range.
fn band(app: &mut App, tile: Entity, reach: u32) -> Entity {
    let mut allocation = LaborAllocation::default();
    if reach > 0 {
        allocation.set_scouting_range(reach, None);
        allocation.apply_scouting(ScoutCrew {
            kitted: 0,
            bare: reach,
            reach,
        });
    }
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: WORKING,
                children: scalar_zero(),
                working: scalar_from_f32(WORKING as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                fed_party_workers: Default::default(),
                last_food_spoiled: 0.0,
                last_food_kept: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_cohesion: Default::default(),
                last_fertility_factors: Default::default(),
                last_breeding: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    BandId(BAND_ID),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
                predators_kept_off: 0,
            },
            allocation,
            BandId(BAND_ID),
            ResidentBand,
        ))
        .id()
}

fn kept_off(app: &App, band: Entity) -> u32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .predators_kept_off
}

fn step(app: &mut App) {
    app.world.run_system_once(advance_herds);
}

/// A fixture band at `pos` and a wolf at `wolf_pos` with the given route, with or without scouts.
fn world_with(
    reach: u32,
    wolf_route: impl FnOnce(&mut App, UVec2) -> (UVec2, Vec<UVec2>),
) -> (App, Entity, UVec2, UVec2) {
    let (mut app, pos, tile) = arena();
    let band = band(&mut app, tile, reach);
    let (wolf_pos, route) = wolf_route(&mut app, pos);
    seat(&mut app, "wolf", WOLF, route);
    (app, band, pos, wolf_pos)
}

/// A deer on a land neighbour of the wolf, inside the ring where one exists. A pack with no prey in
/// its sensing disk has no carrying capacity and is despawned, so every pack here keeps company.
fn prey_beside(app: &mut App, wolf: UVec2, camp: UVec2) -> UVec2 {
    let (width, height, wrap) = grid(app);
    let neighbours: Vec<UVec2> = (0..6)
        .filter_map(|dir| hex_neighbor(wolf.x, wolf.y, dir, width, height, wrap))
        .map(|(x, y)| UVec2::new(x, y))
        .filter(|&n| is_land(app, n))
        .collect();
    let inside = neighbours
        .iter()
        .copied()
        .find(|&n| distance(app, n, camp) <= RING);
    let deer = inside
        .or(neighbours.first().copied())
        .expect("a land neighbour");
    seat(app, "deer", DEER, vec![deer]);
    deer
}

/// A wolf standing `RING - 1` from the band (inside the ring, stationary otherwise).
fn wolf_inside(app: &mut App, camp: UVec2) -> (UVec2, Vec<UVec2>) {
    let spot = land_at(app, camp, RING - 1)
        .into_iter()
        .find(|&pos| {
            // Room to step outward: a land neighbour farther from camp.
            neighbour_at(app, pos, camp, RING).is_some()
        })
        .expect("a land tile inside the ring with a way out");
    (spot, vec![spot])
}

#[test]
fn a_pack_inside_the_ring_steps_out_on_its_move_turn_and_a_bandless_world_leaves_it_be() {
    let (mut watched, band, camp, start) = world_with(REACH, wolf_inside);
    prey_beside(&mut watched, start, camp);
    step(&mut watched);
    let after = herd_pos(&watched, "wolf");
    assert!(
        distance(&watched, after, camp) > distance(&watched, start, camp),
        "the pack stepped farther from the watching camp"
    );
    assert_eq!(kept_off(&watched, band), 1, "and the band is credited once");

    // Control: no scouts, no ring - the same pack pursues the deer beside it exactly as it always did.
    let (mut control, band, camp, start) = world_with(0, wolf_inside);
    let deer = prey_beside(&mut control, start, camp);
    step(&mut control);
    assert_eq!(
        herd_pos(&control, "wolf"),
        deer,
        "no scouts, no ring: the unwatched pursuit, step for step"
    );
    assert_eq!(kept_off(&control, band), 0);
}

#[test]
fn a_pack_keeps_off_the_ring_over_many_turns() {
    let (mut app, band, camp, start) = world_with(REACH, wolf_inside);
    prey_beside(&mut app, start, camp);
    for _ in 0..TURNS {
        step(&mut app);
    }
    assert!(
        distance(&app, herd_pos(&app, "wolf"), camp) > RING,
        "the pack ends outside the ring"
    );
    assert_eq!(
        kept_off(&app, band),
        1,
        "and the deer it can no longer chase inside the ring is still the ring's doing"
    );
}

#[test]
fn a_pack_outside_does_not_step_into_the_ring() {
    // The pack stands just outside the ring; its route anchor lies inside it.
    let route = |app: &mut App, camp: UVec2| {
        let spot = land_at(app, camp, RING + 1)
            .into_iter()
            .find(|&pos| neighbour_at(app, pos, camp, RING).is_some())
            .expect("a land tile just outside the ring with a way in");
        let inner = land_at(app, camp, RING - 2)
            .into_iter()
            .next()
            .expect("a land tile well inside the ring");
        (spot, vec![spot, inner])
    };
    let (mut watched, band, camp, start) = world_with(REACH, route);
    prey_beside(&mut watched, start, camp);
    for _ in 0..TURNS {
        step(&mut watched);
        assert!(
            distance(&watched, herd_pos(&watched, "wolf"), camp) > RING,
            "the pack never ends a step inside the ring"
        );
    }
    assert_eq!(herd_pos(&watched, "wolf"), start, "refused every time");
    assert_eq!(kept_off(&watched, band), 1, "the refusal is credited");

    // Liveness: with no ring the same pack walks toward its anchor.
    let (mut control, _, camp, start) = world_with(0, route);
    prey_beside(&mut control, start, camp);
    for _ in 0..TURNS {
        step(&mut control);
    }
    assert!(
        distance(&control, herd_pos(&control, "wolf"), camp) < distance(&control, start, camp),
        "liveness: without the ring the pack closes on its prey and its anchor"
    );
}

/// A refused step is the pack hemmed in: a graze-wanderer whose waypoint lies inside the ring moves
/// on to its next waypoint instead of retrying the refused step for ever.
#[test]
fn a_refused_graze_wander_step_moves_the_pack_on_to_its_next_waypoint() {
    let route = |app: &mut App, camp: UVec2| {
        let spot = land_at(app, camp, RING + 1)
            .into_iter()
            .find(|&pos| neighbour_at(app, pos, camp, RING).is_some())
            .expect("a land tile just outside the ring with a way in");
        let inner = land_at(app, camp, RING - 2)
            .into_iter()
            .next()
            .expect("a land tile well inside the ring");
        let outside = neighbour_at(app, spot, camp, RING + 2)
            .or_else(|| neighbour_at(app, spot, camp, RING + 1))
            .expect("a land neighbour clear of the ring");
        (spot, vec![spot, inner, outside])
    };
    let (mut watched, band, camp, start) = world_with(REACH, route);
    prey_beside(&mut watched, start, camp);
    let mut moved = false;
    for _ in 0..TURNS {
        step(&mut watched);
        let at = herd_pos(&watched, "wolf");
        assert!(
            distance(&watched, at, camp) > RING,
            "the pack never ends a step inside the ring"
        );
        moved |= at != start;
    }
    assert!(
        moved,
        "the refused waypoint is skipped: the pack moves on instead of freezing at the ring's edge"
    );
    assert!(
        kept_off(&watched, band) <= 1,
        "one pack, credited at most once"
    );

    // Control: no scouts, the same route - the pack walks to the inner waypoint.
    let (mut control, _, camp, start) = world_with(0, route);
    prey_beside(&mut control, start, camp);
    for _ in 0..TURNS {
        step(&mut control);
    }
    assert!(
        distance(&control, herd_pos(&control, "wolf"), camp) <= RING,
        "liveness: unwatched, the same route takes the pack into the ground the ring guards"
    );
}

/// A refused `Migrate` step ends the leg (the hemmed-in branch) and leaves no heading into the ring.
#[test]
fn a_refused_migrate_step_ends_the_leg_and_clears_the_heading() {
    let route = |app: &mut App, camp: UVec2| {
        let spot = land_at(app, camp, RING + 1)
            .into_iter()
            .find(|&pos| neighbour_at(app, pos, camp, RING).is_some())
            .expect("a land tile just outside the ring with a way in");
        let inner = land_at(app, camp, RING - 2)
            .into_iter()
            .next()
            .expect("a land tile well inside the ring");
        (spot, vec![spot, inner])
    };
    let migrating = |reach: u32| {
        let (mut app, band, camp, start) = world_with(reach, route);
        prey_beside(&mut app, start, camp);
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let wolf = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == "wolf")
            .unwrap();
        wolf.roam = RoamState::Migrate;
        (app, band, camp, start)
    };
    let roam_of = |app: &App| {
        let herd = app
            .world
            .resource::<HerdRegistry>()
            .herds
            .iter()
            .find(|herd| herd.id == "wolf")
            .unwrap()
            .clone();
        (herd.roam, herd.next_pos)
    };

    let (mut watched, _, camp, start) = migrating(REACH);
    step(&mut watched);
    let (roam, next_pos) = roam_of(&watched);
    assert_eq!(herd_pos(&watched, "wolf"), start, "the step was refused");
    assert!(
        matches!(roam, RoamState::Loiter { .. }),
        "a refused migrate step ends the leg: {roam:?}"
    );
    assert_eq!(next_pos, None, "no heading arrow points into the ring");
    for _ in 0..TURNS {
        step(&mut watched);
        assert!(
            distance(&watched, herd_pos(&watched, "wolf"), camp) > RING,
            "never inside the ring"
        );
        if let Some(next) = roam_of(&watched).1 {
            assert!(distance(&watched, next, camp) > RING, "heading stays clear");
        }
    }
}

#[test]
fn prey_standing_inside_the_ring_is_not_chased() {
    // The pack stands outside the ring within pursuit range of a deer inside it.
    let route = |app: &mut App, camp: UVec2| {
        let spot = land_at(app, camp, RING + 2)
            .into_iter()
            .next()
            .expect("a land tile outside the ring");
        (spot, vec![spot])
    };
    let (mut app, band, camp, start) = world_with(REACH, route);
    let deer = land_at(&mut app, camp, RING - 1)
        .into_iter()
        .min_by_key(|&pos| distance(&app, pos, start))
        .expect("a land tile inside the ring");
    seat(&mut app, "deer", DEER, vec![deer]);
    for _ in 0..TURNS {
        step(&mut app);
    }
    assert_eq!(
        herd_pos(&app, "wolf"),
        start,
        "the pack does not chase prey inside the ring"
    );
    assert_eq!(kept_off(&app, band), 1, "the excluded prey is credited");

    // Liveness: unwatched, the same pack walks toward that deer.
    let (mut control, _, camp, start) = world_with(0, route);
    let deer = land_at(&mut control, camp, RING - 1)
        .into_iter()
        .min_by_key(|&pos| distance(&control, pos, start))
        .unwrap();
    seat(&mut control, "deer", DEER, vec![deer]);
    for _ in 0..TURNS {
        step(&mut control);
    }
    assert!(
        distance(&control, herd_pos(&control, "wolf"), deer) < distance(&control, start, deer),
        "liveness: without the ring the pack pursues the deer"
    );
}

/// Run `TURNS` of movement then a raid pass each turn, and return the working-age people left.
fn survivors(reach: u32) -> f32 {
    let route = |app: &mut App, camp: UVec2| {
        let spot = land_at(app, camp, 4)
            .into_iter()
            .next()
            .expect("a land tile four away");
        (spot, vec![spot])
    };
    let (mut app, band, camp, _) = world_with(reach, route);
    // Prey on the camp itself draws an unwatched pack straight to the band.
    seat(&mut app, "deer", DEER, vec![camp]);
    for _ in 0..TURNS {
        step(&mut app);
        app.world.run_system_once(advance_predator_raids);
    }
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .working
        .to_f32()
}

#[test]
fn a_band_with_scouts_on_duty_is_not_raided_and_one_without_is() {
    assert_eq!(
        survivors(REACH),
        WORKING as f32,
        "a pack kept outside the ring never reaches raid radius"
    );
    assert!(
        survivors(0) < WORKING as f32,
        "liveness: the same pack, unwatched, raids the band"
    );
}
