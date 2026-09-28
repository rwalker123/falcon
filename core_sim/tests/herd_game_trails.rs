//! **Migratory herds wear game trails** — route traffic's third source (issue #215).
//!
//! A migratory herd's step along a `RoamState::Migrate` leg is recorded by the **real**
//! `advance_herds` through `RouteTrafficLog::herd_passed` and banked by the **real**
//! `routes::advance_roads`, in the order the Logistics stage runs them. The fixtures are hand-built
//! all-land worlds (the `fauna_pursuit.rs` shape) with **no graze layer**, so a herd's movement is
//! pure land geometry and the only thing under test is what its step banks.
//!
//! The report at the bottom (`#[ignore]`) is the measurement the lever's opening value was chosen
//! from — see its own doc.

use std::collections::{BTreeMap, HashMap, HashSet};

use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::World;

use core_sim::{
    advance_herds, advance_roads, traffic_ceiling, FaunaConfigHandle, Herd, HerdDensityMap,
    HerdRegistry, HerdTelemetry, LadderConfig, LadderConfigHandle, RoadRegistry, RoamState,
    RouteTrafficLog, SimulationConfig, SimulationTick, SizeClass, Tile, TileRegistry,
};

/// A shipped **migratory** species — its `def` resolves, so the herd runs the species' own cadence.
const MIGRATORY_SPECIES: &str = "Thunder Mammoths";
/// A shipped **short-range game** species, for the graze-wander control.
const GAME_SPECIES: &str = "Red Deer";

/// The fixture world's size: a strip wide enough for a multi-hex leg on one row.
const WORLD_WIDTH: u32 = 30;
const WORLD_HEIGHT: u32 = 7;
/// The row every fixture herd walks along — interior, so no neighbour search clips the map edge.
const ROW: u32 = 3;
/// Where a leg starts and where its next anchor sits — four hexes apart on one row, so a herd takes
/// several steps along it before arriving, and arrives inside the shipped disuse grace: the leg's
/// first tile has not started to bleed by the time the last is banked.
const LEG_START: UVec2 = UVec2::new(8, ROW);
const LEG_END: UVec2 = UVec2::new(12, ROW);
/// A tile no fixture herd ever reaches — the disuse control.
const FAR_TILE: UVec2 = UVec2::new(26, ROW);

/// A healthy fixture herd: biomass well above every ecology cut, and a constructor `K` it keeps
/// (`fodder_per_biomass = 0` — there is no graze layer to derive another from).
const FIXTURE_BIOMASS: f32 = 400.0;
const FIXTURE_CAPACITY: f32 = 1000.0;
const FIXTURE_FODDER_PER_BIOMASS: f32 = 0.0;
const FIXTURE_REGROWTH_RATE: f32 = 0.04;
const FIXTURE_BODY_MASS: f32 = 20.0;

/// How many loiter turns the Loiter control runs — long enough that its wander moves it, short of
/// the loiter window it is given, so it never tips into a `Migrate` leg mid-test.
const LOITER_TURNS_OBSERVED: u32 = 12;
const LOITER_WINDOW: u32 = LOITER_TURNS_OBSERVED + 4;

/// Float tolerance for a banked position — the sums here are one or two additions of the lever.
const TOLERANCE: f32 = 1e-4;

/// A flat all-land world with every resource `advance_herds` and `advance_roads` read.
fn land_world() -> World {
    let mut world = World::default();
    let mut config = SimulationConfig::builtin();
    config.grid_size = UVec2::new(WORLD_WIDTH, WORLD_HEIGHT);
    config.map_topology.wrap_horizontal = false;
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    world.insert_resource(config);
    world.insert_resource(FaunaConfigHandle::default());
    world.insert_resource(core_sim::CombatConfigHandle::default());
    world.insert_resource(LadderConfigHandle::default());
    world.insert_resource(SimulationTick::default());
    world.insert_resource(HerdRegistry::default());
    world.insert_resource(HerdTelemetry::default());
    world.insert_resource(HerdDensityMap::default());
    world.insert_resource(RoadRegistry::default());
    world.insert_resource(RouteTrafficLog::default());

    let mut tiles = Vec::with_capacity((WORLD_WIDTH * WORLD_HEIGHT) as usize);
    for y in 0..WORLD_HEIGHT {
        for x in 0..WORLD_WIDTH {
            tiles.push(
                world
                    .spawn(Tile {
                        position: UVec2::new(x, y),
                        ..Default::default()
                    })
                    .id(),
            );
        }
    }
    world.insert_resource(TileRegistry {
        tiles,
        width: WORLD_WIDTH,
        height: WORLD_HEIGHT,
    });
    world
}

fn ladder(world: &World) -> std::sync::Arc<LadderConfig> {
    world.resource::<LadderConfigHandle>().get()
}

fn fixture_herd(id: &str, species: &str, size_class: SizeClass, route: Vec<UVec2>) -> Herd {
    let mut herd = Herd::new(
        id.to_string(),
        species.to_string(),
        size_class,
        route,
        FIXTURE_BIOMASS,
        FIXTURE_CAPACITY,
        FIXTURE_FODDER_PER_BIOMASS,
        FIXTURE_REGROWTH_RATE,
        FIXTURE_BODY_MASS,
    );
    herd.dwell_remaining = 0;
    herd
}

/// A migratory herd standing at `LEG_START`, already on its `Migrate` leg toward `LEG_END`.
fn migrating_herd() -> Herd {
    let mut herd = fixture_herd(
        "herd_migrating",
        MIGRATORY_SPECIES,
        SizeClass::Migratory,
        vec![LEG_START, LEG_END],
    );
    herd.roam = RoamState::Migrate;
    herd
}

fn add_herd(world: &mut World, herd: Herd) {
    world.resource_mut::<HerdRegistry>().herds.push(herd);
}

fn herd_pos(world: &World, id: &str) -> UVec2 {
    world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|h| h.id == id)
        .unwrap_or_else(|| panic!("herd {id} is still in the registry"))
        .current_pos
}

/// One turn of the two systems under test, in the order the Logistics stage runs them.
fn logistics_turn(world: &mut World) {
    world.run_system_once(advance_herds);
    world.run_system_once(advance_roads);
}

fn road_position(world: &World, tile: UVec2) -> Option<f32> {
    world
        .resource::<RoadRegistry>()
        .road(tile)
        .map(|road| road.position())
}

/// **A step on a `Migrate` leg banks `work_per_herd_tile` on each tile it crossed** — the tile it
/// left and the tile it entered, through the one drain every other journey goes through.
#[test]
fn a_migrate_leg_step_banks_the_herd_lever_on_the_tiles_it_crossed() {
    let mut world = land_world();
    add_herd(&mut world, migrating_herd());
    let lever = ladder(&world).route_traffic.work_per_herd_tile;

    world.run_system_once(advance_herds);
    let entered = herd_pos(&world, "herd_migrating");
    assert_ne!(entered, LEG_START, "the herd stepped along its leg");

    let journeys = world.resource::<RouteTrafficLog>().journeys.clone();
    assert_eq!(journeys.len(), 1, "one step, one journey: {journeys:?}");
    assert_eq!((journeys[0].from, journeys[0].to), (LEG_START, entered));
    assert!(
        (journeys[0].work_per_tile - lever).abs() < TOLERANCE,
        "the journey carries the herd lever, per herd and never per unit of biomass"
    );

    world.run_system_once(advance_roads);
    assert!(
        world.resource::<RouteTrafficLog>().journeys.is_empty(),
        "the accrual drained the log"
    );
    for tile in [LEG_START, entered] {
        let banked = road_position(&world, tile).expect("the crossed tile carries a road");
        assert!(
            (banked - lever).abs() < TOLERANCE,
            "{tile:?} banked {banked}, expected the lever {lever}"
        );
    }
    assert_eq!(
        world.resource::<RoadRegistry>().iter().count(),
        2,
        "nothing but the two crossed tiles was worn"
    );
}

/// **A whole leg wears the corridor in**: every tile from the start anchor to the next is banked,
/// the interior ones twice (the turn the herd enters and the turn it leaves), the two ends once.
#[test]
fn a_whole_leg_wears_every_corridor_tile_between_the_anchors() {
    let mut world = land_world();
    add_herd(&mut world, migrating_herd());
    let lever = ladder(&world).route_traffic.work_per_herd_tile;

    let mut visited = vec![LEG_START];
    // The leg is `LEG_END.x - LEG_START.x` steps on one row; one turn more lets it arrive and turn
    // to Loiter, which banks nothing further.
    let leg_steps = LEG_END.x - LEG_START.x;
    // The fixture's premise: the whole leg is walked before its first tile's idle count passes the
    // grace, so every tile holds exactly what was banked on it.
    let grace = ladder(&world).route_traffic.disuse_grace_turns;
    assert!(
        leg_steps <= grace,
        "the fixture leg ({leg_steps} steps) must finish inside the disuse grace ({grace})"
    );
    for _ in 0..leg_steps {
        logistics_turn(&mut world);
        visited.push(herd_pos(&world, "herd_migrating"));
    }
    assert_eq!(
        *visited.last().unwrap(),
        LEG_END,
        "the herd arrived: {visited:?}"
    );
    let herd_roam = world.resource::<HerdRegistry>().herds[0].roam;
    assert!(
        matches!(herd_roam, RoamState::Loiter { .. }),
        "arriving turns the leg into a loiter, got {herd_roam:?}"
    );

    for (i, tile) in visited.iter().enumerate() {
        let is_end = i == 0 || i == visited.len() - 1;
        let crossings = if is_end { 1.0 } else { 2.0 };
        let banked = road_position(&world, *tile).expect("a corridor tile carries a road");
        assert!(
            (banked - lever * crossings).abs() < TOLERANCE,
            "{tile:?} banked {banked}, expected {crossings} x {lever}"
        );
    }
}

/// **A graze-wandering game group banks nothing** — the same step length, the same land, and no
/// trail, because only the corridor between migratory anchors wears in.
#[test]
fn a_graze_wander_step_banks_nothing() {
    let mut world = land_world();
    add_herd(
        &mut world,
        fixture_herd(
            "game_deer",
            GAME_SPECIES,
            SizeClass::Big,
            vec![LEG_START, LEG_END],
        ),
    );
    world
        .resource_mut::<HerdRegistry>()
        .herds
        .iter_mut()
        .for_each(|h| h.current_pos = LEG_START);

    world.run_system_once(advance_herds);
    assert_ne!(
        herd_pos(&world, "game_deer"),
        LEG_START,
        "liveness: the game group really did step this turn"
    );
    assert!(
        world.resource::<RouteTrafficLog>().journeys.is_empty(),
        "a graze-wander step records no traffic"
    );
    world.run_system_once(advance_roads);
    assert_eq!(world.resource::<RoadRegistry>().iter().count(), 0);
}

/// **A loitering migratory herd banks nothing** — it mills about its anchor rather than walking a
/// line, so its wander nudges are not a corridor.
#[test]
fn a_loiter_step_banks_nothing() {
    let mut world = land_world();
    let mut herd = migrating_herd();
    herd.roam = RoamState::Loiter {
        turns_left: LOITER_WINDOW,
    };
    add_herd(&mut world, herd);

    let mut moves = 0;
    let mut last = LEG_START;
    for _ in 0..LOITER_TURNS_OBSERVED {
        world.run_system_once(advance_herds);
        assert!(
            world.resource::<RouteTrafficLog>().journeys.is_empty(),
            "a loiter step records no traffic"
        );
        let now = herd_pos(&world, "herd_migrating");
        if now != last {
            moves += 1;
            last = now;
        }
        world.run_system_once(advance_roads);
    }
    assert!(moves > 0, "liveness: the loitering herd really did wander");
    assert!(
        matches!(
            world.resource::<HerdRegistry>().herds[0].roam,
            RoamState::Loiter { .. }
        ),
        "the control never tipped into a Migrate leg"
    );
    assert_eq!(world.resource::<RoadRegistry>().iter().count(), 0);
}

/// **Herd traffic stops at the traffic ceiling** — the top of `route:trail`. A herd wears at most a
/// trail, never a billed road.
#[test]
fn herd_traffic_stops_at_the_traffic_ceiling() {
    let mut world = land_world();
    add_herd(&mut world, migrating_herd());
    let ladder = ladder(&world);
    let ceiling = traffic_ceiling(&ladder);
    let lever = ladder.route_traffic.work_per_herd_tile;
    // Half a lever below the ceiling, so one crossing would overshoot it uncapped.
    let seated = ceiling - lever / 2.0;
    world
        .resource_mut::<RoadRegistry>()
        .road_or_trail(LEG_START, &ladder)
        .set_position(seated, &ladder);

    logistics_turn(&mut world);
    let banked = road_position(&world, LEG_START).expect("the road is still there");
    assert!(
        (banked - ceiling).abs() < TOLERANCE,
        "herd traffic banked {banked}, capped at the ceiling {ceiling} (uncapped would be {})",
        seated + lever
    );
}

/// **Herd traffic counts as traffic for disuse** — a herd coming back resets the idle count, so its
/// trail is not bled, while an identical road nothing walked loses the disuse rate.
#[test]
fn herd_traffic_resets_disuse() {
    let mut world = land_world();
    add_herd(&mut world, migrating_herd());
    let ladder = ladder(&world);
    let grace = ladder.route_traffic.disuse_grace_turns;
    let loss = ladder.route_traffic.disuse_loss_per_turn;
    let lever = ladder.route_traffic.work_per_herd_tile;
    let ceiling = traffic_ceiling(&ladder);
    // Low enough that one crossing does not reach the ceiling, so the banked amount is exact.
    let seated = (ceiling - lever) / 2.0;
    // Long past the grace — both roads would bleed this turn if nothing walked them.
    let idle = u16::try_from(grace).expect("the grace fits the counter") + 3;
    {
        let mut registry = world.resource_mut::<RoadRegistry>();
        for tile in [LEG_START, FAR_TILE] {
            let road = registry.road_or_trail(tile, &ladder);
            road.set_position(seated, &ladder);
            road.idle_turns = idle;
        }
    }

    logistics_turn(&mut world);
    let registry = world.resource::<RoadRegistry>();
    let walked = registry.road(LEG_START).expect("the herd's tile");
    assert_eq!(walked.idle_turns, 0, "the herd's step reset the idle count");
    assert!(
        (walked.position() - (seated + lever)).abs() < TOLERANCE,
        "the herd's tile banked its crossing and bled nothing: {}",
        walked.position()
    );
    let unwalked = registry.road(FAR_TILE).expect("the control tile");
    assert!(
        (unwalked.position() - (seated - loss)).abs() < TOLERANCE,
        "liveness: the unwalked control really did bleed the disuse rate: {}",
        unwalked.position()
    );
}

/// The worn tile's road, read once — panics if the tile was pruned.
fn road_idles(world: &World, tile: UVec2) -> (f32, u16, Option<u16>) {
    world
        .resource::<RoadRegistry>()
        .road(tile)
        .map(|r| (r.position(), r.idle_turns, r.herd_idle_turns))
        .unwrap_or_else(|| panic!("the road at {tile:?} is still in the registry"))
}

/// **A herd-worn tile holds its wear past the people trail's whole lifetime, and bleeds only once
/// the herd grace has run out.** One Migrate step wears `LEG_START`; the herd then leaves the map
/// for good and only the road pass runs. Through every turn up to the herd grace the tile keeps
/// exactly what the herd banked — well past the ~44 turns a people trail lives — and on the first
/// turn past it the ordinary flat loss takes one `disuse_loss_per_turn`.
#[test]
fn a_herd_worn_tile_holds_until_the_herd_grace_runs_out() {
    let mut world = land_world();
    add_herd(&mut world, migrating_herd());
    let ladder = ladder(&world);
    let lever = ladder.route_traffic.work_per_herd_tile;
    let people_grace = ladder.route_traffic.disuse_grace_turns;
    let herd_grace = ladder.route_traffic.herd_disuse_grace_turns;
    let loss = ladder.route_traffic.disuse_loss_per_turn;
    // The people trail's lifetime from this tile's wear: the grace, then the loss to zero.
    let people_lifetime = people_grace + (lever / loss).ceil() as u32;
    assert!(
        herd_grace > people_lifetime,
        "the fixture's premise: the herd grace ({herd_grace}) outlasts a people trail's life \
         ({people_lifetime})"
    );

    logistics_turn(&mut world);
    world.resource_mut::<HerdRegistry>().herds.clear();
    assert_eq!(
        road_idles(&world, LEG_START).2,
        Some(0),
        "the herd reset its clock"
    );

    for _ in 0..herd_grace {
        world.run_system_once(advance_roads);
        let (position, _, _) = road_idles(&world, LEG_START);
        assert!(
            (position - lever).abs() < TOLERANCE,
            "the game trail holds its wear while the herd grace runs: {position}"
        );
    }
    let (_, idle, herd_idle) = road_idles(&world, LEG_START);
    assert!(
        u32::from(idle) > people_grace,
        "the people grace ran out long ago"
    );
    assert_eq!(herd_idle.map(u32::from), Some(herd_grace));

    world.run_system_once(advance_roads);
    let (position, _, _) = road_idles(&world, LEG_START);
    assert!(
        (position - (lever - loss)).abs() < TOLERANCE,
        "one turn past the herd grace the flat loss bites: {position}"
    );
}

/// **A road no herd has ever crossed bleeds exactly as it did before game trails** — the people
/// grace alone decides, and its herd clock never starts.
#[test]
fn a_people_only_road_bleeds_exactly_as_before() {
    let mut world = land_world();
    let ladder = ladder(&world);
    let grace = ladder.route_traffic.disuse_grace_turns;
    let loss = ladder.route_traffic.disuse_loss_per_turn;
    let seated = traffic_ceiling(&ladder);
    world
        .resource_mut::<RoadRegistry>()
        .road_or_trail(FAR_TILE, &ladder)
        .set_position(seated, &ladder);

    for _ in 0..grace {
        world.run_system_once(advance_roads);
        let (position, _, herd_idle) = road_idles(&world, FAR_TILE);
        assert!((position - seated).abs() < TOLERANCE, "inside the grace");
        assert_eq!(herd_idle, core_sim::NO_HERD_HAS_CROSSED);
    }
    world.run_system_once(advance_roads);
    let (position, _, _) = road_idles(&world, FAR_TILE);
    assert!(
        (position - (seated - loss)).abs() < TOLERANCE,
        "one turn past the people grace it loses the flat rate: {position}"
    );
}

// ---------------------------------------------------------------------------------------------
// The measurement the opening value was chosen from.
// ---------------------------------------------------------------------------------------------

/// The shipped standard map, generated through the real Startup chain.
const REPORT_GRID: UVec2 = UVec2::new(80, 52);
const REPORT_SEEDS: [u64; 6] = [core_sim::HARNESS_MAP_SEED, 11, 4242, 90210, 7, 13];
/// Enough turns for several full migration cycles of every herd.
const REPORT_TURNS: u32 = 600;

/// One migratory herd's step on a `Migrate` leg, as observed from outside the turn.
struct ObservedStep {
    turn: u32,
    herd: String,
    from: UVec2,
    to: UVec2,
}

/// **How often is a corridor tile crossed, and does its trail survive between crossings?**
///
/// Runs the full turn on each seed and watches every migratory herd. A step is a turn that began
/// with the herd on a `Migrate` leg and moved it — the same condition `advance_herds` records on.
/// A **pass** over a tile is a run of banks on it by one herd with no gap longer than one turn (a
/// herd banks an interior corridor tile twice, on consecutive turns). Reports the gap between one
/// herd's successive passes over the same tile, and how the shipped `route_traffic` levers leave the
/// corridor tiles in the real `RoadRegistry`: the share that ever reached the trail rung, the turns
/// from a tile's first herd crossing to its first turn at the rung, and how many trail tiles stand at
/// the end — on the corridor and on the whole map, since bands wear roads too.
///
/// Run with `cargo test -p core_sim --test herd_game_trails --release -- --ignored --nocapture`.
#[test]
#[ignore = "report-only measurement; run with --ignored --nocapture"]
fn report_migration_corridor_pass_cadence() {
    let mut all_gaps: Vec<u32> = Vec::new();
    let mut all_banks_per_pass: Vec<u32> = Vec::new();
    let mut total_herds = 0usize;
    let mut total_corridor_tiles = 0usize;
    let mut total_at_ceiling = 0usize;
    let mut total_ever_at_ceiling = 0usize;
    let mut max_position_seen = 0.0f32;
    let mut all_turns_to_trail: Vec<u32> = Vec::new();

    for seed in REPORT_SEEDS {
        let mut app = core_sim::build_test_app();
        {
            let mut config = app.world.resource::<SimulationConfig>().clone();
            config.map_preset_id = "earthlike".to_string();
            config.map_seed = seed;
            config.grid_size = REPORT_GRID;
            app.world.insert_resource(config);
        }
        // The first update runs Startup (worldgen, herd spawn) and the opening turn.
        app.update();
        let ceiling = traffic_ceiling(&app.world.resource::<LadderConfigHandle>().get());

        let snapshot = |app: &bevy::app::App| -> BTreeMap<String, (UVec2, RoamState)> {
            app.world
                .resource::<HerdRegistry>()
                .herds
                .iter()
                .filter(|h| h.size_class == SizeClass::Migratory)
                .map(|h| (h.id.clone(), (h.current_pos, h.roam)))
                .collect()
        };
        let mut before = snapshot(&app);
        total_herds += before.len();
        let herds_at_start = before.len();
        // **The trails worn before the game began** — every road standing at the rung after Startup
        // and the opening turn is a stamped corridor tile (one turn of any traffic cannot wear a
        // trail from nothing).
        let seeded: HashSet<UVec2> = app
            .world
            .resource::<RoadRegistry>()
            .iter()
            .filter(|(_, r)| r.position() >= ceiling - TOLERANCE)
            .map(|(tile, _)| tile)
            .collect();
        let mut steps: Vec<ObservedStep> = Vec::new();
        let mut corridor: HashMap<UVec2, f32> = HashMap::new();
        let mut ever_at_ceiling: HashSet<UVec2> = HashSet::new();
        // The turn each corridor tile was first crossed by a herd, and first stood at the rung.
        let mut first_crossed: HashMap<UVec2, u32> = HashMap::new();
        let mut first_at_ceiling: HashMap<UVec2, u32> = HashMap::new();

        for turn in 1..=REPORT_TURNS {
            core_sim::run_turn(&mut app);
            let after = snapshot(&app);
            for (id, (from, roam)) in &before {
                let Some((to, _)) = after.get(id) else {
                    continue;
                };
                if *roam == RoamState::Migrate && to != from {
                    steps.push(ObservedStep {
                        turn,
                        herd: id.clone(),
                        from: *from,
                        to: *to,
                    });
                    corridor.entry(*from).or_default();
                    corridor.entry(*to).or_default();
                    first_crossed.entry(*from).or_insert(turn);
                    first_crossed.entry(*to).or_insert(turn);
                }
            }
            let roads = app.world.resource::<RoadRegistry>();
            for (tile, max_pos) in corridor.iter_mut() {
                let pos = roads.road(*tile).map_or(0.0, |r| r.position());
                *max_pos = max_pos.max(pos);
                if pos >= ceiling - TOLERANCE {
                    ever_at_ceiling.insert(*tile);
                    first_at_ceiling.entry(*tile).or_insert(turn);
                }
            }
            before = after;
        }

        // Group banks into passes per (tile, herd).
        let mut banks: HashMap<(UVec2, String), Vec<u32>> = HashMap::new();
        for step in &steps {
            for tile in [step.from, step.to] {
                banks
                    .entry((tile, step.herd.clone()))
                    .or_default()
                    .push(step.turn);
            }
        }
        let mut seed_gaps = Vec::new();
        for turns in banks.values() {
            let mut passes: Vec<(u32, u32)> = Vec::new(); // (first turn, bank count)
            let mut last_turn: Option<u32> = None;
            for &t in turns {
                match (last_turn, passes.last_mut()) {
                    (Some(prev), Some(pass)) if t - prev <= 1 => pass.1 += 1,
                    _ => passes.push((t, 1)),
                }
                last_turn = Some(t);
            }
            for pair in passes.windows(2) {
                seed_gaps.push(pair[1].0 - pair[0].0);
            }
            all_banks_per_pass.extend(passes.iter().map(|p| p.1));
        }
        let roads = app.world.resource::<RoadRegistry>();
        let at_ceiling = corridor
            .keys()
            .filter(|tile| {
                roads
                    .road(**tile)
                    .is_some_and(|r| r.position() >= ceiling - TOLERANCE)
            })
            .count();
        let map_trails = roads
            .iter()
            .filter(|(_, r)| r.position() >= ceiling - TOLERANCE)
            .count();
        let seeded_standing = seeded
            .iter()
            .filter(|tile| {
                roads
                    .road(**tile)
                    .is_some_and(|r| r.position() >= ceiling - TOLERANCE)
            })
            .count();
        let live_step_tiles: Vec<UVec2> = steps.iter().flat_map(|s| [s.from, s.to]).collect();
        let live_on_seeded = live_step_tiles
            .iter()
            .filter(|t| seeded.contains(t))
            .count();
        println!(
            "seed {seed}: seeded trails after the opening turn={}, of them still a trail at turn \
             {REPORT_TURNS}={seeded_standing}; migratory herds {herds_at_start} -> {}; live Migrate \
             step tiles on a seeded trail {live_on_seeded}/{} ({:.0}%)",
            seeded.len(),
            before.len(),
            live_step_tiles.len(),
            100.0 * live_on_seeded as f64 / live_step_tiles.len().max(1) as f64,
        );
        let mut seed_turns_to_trail: Vec<u32> = first_at_ceiling
            .iter()
            .map(|(tile, at)| at - first_crossed[tile])
            .collect();
        seed_turns_to_trail.sort_unstable();
        let seed_max = corridor.values().copied().fold(0.0f32, f32::max);
        max_position_seen = max_position_seen.max(seed_max);
        seed_gaps.sort_unstable();
        println!(
            "seed {seed}: {} migratory herds, {} corridor tiles, {} steps, pass gaps n={} \
             min={:?} median={:?} max={:?}, corridor tiles at the ceiling now={at_ceiling} \
             ever={} ({:.0}%), turns-to-trail median={:?}, trail tiles on the whole map \
             now={map_trails}, max position {seed_max:.2}",
            before.len(),
            corridor.len(),
            steps.len(),
            seed_gaps.len(),
            seed_gaps.first(),
            seed_gaps.get(seed_gaps.len() / 2),
            seed_gaps.last(),
            ever_at_ceiling.len(),
            100.0 * ever_at_ceiling.len() as f64 / corridor.len().max(1) as f64,
            seed_turns_to_trail.get(seed_turns_to_trail.len() / 2),
        );
        all_turns_to_trail.extend(seed_turns_to_trail);
        total_corridor_tiles += corridor.len();
        total_at_ceiling += at_ceiling;
        total_ever_at_ceiling += ever_at_ceiling.len();
        all_gaps.extend(seed_gaps);
    }

    all_gaps.sort_unstable();
    all_turns_to_trail.sort_unstable();
    let ttt = |p: usize| {
        all_turns_to_trail
            .get(all_turns_to_trail.len() * p / 100)
            .copied()
    };
    let mean = all_gaps.iter().map(|&g| f64::from(g)).sum::<f64>() / all_gaps.len().max(1) as f64;
    let pct = |p: usize| all_gaps.get(all_gaps.len() * p / 100).copied();
    let mut bank_hist: BTreeMap<u32, usize> = BTreeMap::new();
    for b in &all_banks_per_pass {
        *bank_hist.entry(*b).or_default() += 1;
    }
    println!(
        "ALL: {total_herds} herds, {total_corridor_tiles} corridor tiles; same-herd pass gap \
         n={} mean={mean:.1} p10={:?} p25={:?} median={:?} p75={:?} p90={:?} min={:?} max={:?}; \
         banks per pass {bank_hist:?}; corridor tiles at the ceiling at the end \
         {total_at_ceiling}, ever {total_ever_at_ceiling} ({:.0}%); turns from first herd crossing \
         to the trail rung n={} p10={:?} median={:?} p90={:?}; max position {max_position_seen:.2}",
        all_gaps.len(),
        pct(10),
        pct(25),
        pct(50),
        pct(75),
        pct(90),
        all_gaps.first(),
        all_gaps.last(),
        100.0 * total_ever_at_ceiling as f64 / total_corridor_tiles.max(1) as f64,
        all_turns_to_trail.len(),
        ttt(10),
        ttt(50),
        ttt(90),
    );
}
