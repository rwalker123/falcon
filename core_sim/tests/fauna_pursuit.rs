//! Predators Phase 2 — the `pursue` movement primitive (`docs/plan_predators.md` → Phase 2).
//!
//! A **wild carnivore** steps toward the nearest **clearable prey it can eat** (the trophic transpose
//! of `drift_to_owner`), instead of roaming toward its range anchor. These tests drive the **real**
//! `advance_herds` movement dispatch on a hand-built all-land world so the assertions are on the
//! herd's own `current_pos`, with full control over prey placement and distance.
//!
//! The world carries **no `GrazeRegistry`** (the `Option<Res<GrazeRegistry>>` is `None`), so movement
//! is pure land geometry — every land neighbour is an acceptable step — and the pursuit is the only
//! thing steering the wolf.
//!
//! **Why the test wolves carry an `owner`.** A carnivore's biomass tracks its prey-limited `K_pred`
//! each turn (Phase 1a): with prey outside the feeding `prey_sense_radius` (4) it gets `K_pred = 0` and
//! `regrow_biomass` clamps its biomass to `0`, and the `advance_herds` cull despawns it — the very
//! coupling Phase 1a/2 tune. That biomass ecology is tested in `predators.rs`; here we isolate the
//! **movement** primitive, so the wolves are given an `owner`, which exempts them from the despawn cull
//! (`owner.is_some()`). An `owner` **does not change `movement_primitive`** — a wild carnivore (no
//! `domestication_progress`, not corralled) still resolves to `Pursue` — so the pursuit under test is
//! unaffected; it just survives the K=0 turns long enough to be observed.

use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::World;

use core_sim::grid_utils::hex_distance_wrapped;
use core_sim::{
    advance_herds, FactionId, FaunaConfigHandle, Herd, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LadderConfigHandle, SimulationConfig, SimulationTick, SizeClass, Tile, TileRegistry,
};

const WOLF: &str = "Grey Wolf Pack";
const DEER: &str = "Red Deer";
const MAMMOTH: &str = "Thunder Mammoths";
/// A species string that resolves to no `SpeciesDef` — its `def` is `None`, so `movement_primitive`
/// treats it as a non-carnivore and it runs the plain wild `roam`. The roam-only control.
const UNRESOLVED: &str = "___not_a_real_species___";

/// A flat all-land world (`width × height`, no horizontal wrap, no graze layer) with just the
/// resources `advance_herds` reads. Every tile is `Tile::default()` (which is land — no `WATER` tag).
fn land_world(width: u32, height: u32) -> World {
    let mut world = World::default();
    let mut config = SimulationConfig::builtin();
    config.grid_size = UVec2::new(width, height);
    config.map_topology.wrap_horizontal = false;
    config.map_seed = 7;
    world.insert_resource(config);
    world.insert_resource(FaunaConfigHandle::default());
    world.insert_resource(core_sim::CombatConfigHandle::default());
    world.insert_resource(LadderConfigHandle::default());
    world.insert_resource(SimulationTick::default());
    world.insert_resource(HerdRegistry::default());
    world.insert_resource(HerdTelemetry::default());
    world.insert_resource(HerdDensityMap::default());

    let mut tiles = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
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
        width,
        height,
    });
    world
}

/// Build a herd at `start` with the given `route` (its wild-roam anchors). `fodder_per_biomass = 0`
/// so a herbivore keeps its constructor `K` (no graze layer to derive one from) and never crashes.
fn herd(id: &str, species: &str, start: UVec2, route: Vec<UVec2>) -> Herd {
    let mut h = Herd::new(
        id.to_string(),
        species.to_string(),
        SizeClass::Big,
        route,
        400.0,  // biomass
        1000.0, // carrying_capacity
        0.0,    // fodder_per_biomass (no grazing)
        0.10,   // regrowth_rate
        20.0,   // body_mass
    );
    h.current_pos = start;
    h
}

/// A wild-wolf herd that survives the K=0 turns via an `owner` (see the module note). It is **not**
/// domesticated (`domestication_progress` stays 0), so `movement_primitive` still returns `Pursue`.
fn wolf(id: &str, start: UVec2, route: Vec<UVec2>) -> Herd {
    let mut h = herd(id, WOLF, start, route);
    h.owner = Some(FactionId(0));
    h
}

fn add_herd(world: &mut World, h: Herd) {
    world.resource_mut::<HerdRegistry>().herds.push(h);
}

fn step(world: &mut World) {
    world.run_system_once(advance_herds);
}

fn pos_of(world: &World, id: &str) -> UVec2 {
    world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|h| h.id == id)
        .unwrap_or_else(|| panic!("herd {id} was culled unexpectedly"))
        .current_pos
}

fn width_of(world: &World) -> u32 {
    world.resource::<SimulationConfig>().grid_size.x
}

/// **A wolf pursues prey out BEYOND its feeding disk, ignoring its roam anchor.** The wolf's wild
/// route anchor is far to the WEST, so a plain roam would carry it there; a clearable deer sits to the
/// EAST at distance 6 — inside `pursuit_radius` (8) but *outside* the feeding `prey_sense_radius` (4).
/// The wolf turns east and closes on the deer, while an otherwise-identical roam-only herd walks west
/// to the anchor. This is the canonical Phase-2 behaviour and the transient-zero-prey fix in one.
#[test]
fn a_wolf_pursues_prey_beyond_its_feeding_disk_ignoring_its_roam_anchor() {
    let mut world = land_world(40, 5);
    let wolf_start = UVec2::new(20, 2);
    let deer_at = UVec2::new(26, 2);
    add_herd(
        &mut world,
        wolf(
            "wolf",
            wolf_start,
            vec![UVec2::new(20, 2), UVec2::new(2, 2)],
        ), // anchor to the WEST
    );
    add_herd(&mut world, herd("deer", DEER, deer_at, vec![deer_at])); // stationary prey to the EAST

    let w = width_of(&world);
    let start_dist = hex_distance_wrapped(wolf_start, deer_at, w, false);
    assert!(
        start_dist > 4 && start_dist <= 8,
        "the deer starts beyond the feeding disk (4) but inside pursuit range (8): {start_dist}"
    );

    let mut dists = vec![start_dist];
    for _ in 0..16 {
        dists.push(hex_distance_wrapped(
            pos_of(&world, "wolf"),
            deer_at,
            w,
            false,
        ));
        step(&mut world);
    }
    dists.push(hex_distance_wrapped(
        pos_of(&world, "wolf"),
        deer_at,
        w,
        false,
    ));

    // Non-increasing while it is still closing (distance > 1); once adjacent it may settle onto / off
    // the prey tile, which is not a retreat from the hunt.
    assert!(
        dists.windows(2).all(|d| d[0] <= 1 || d[1] <= d[0]),
        "the wolf never backs away from the prey while closing on it: {dists:?}"
    );
    assert!(
        *dists.iter().min().unwrap() <= 1,
        "the wolf closes all the way onto the deer rather than roaming to its western anchor: \
         {dists:?}"
    );
    assert!(
        pos_of(&world, "wolf").x > wolf_start.x,
        "the wolf moved EAST toward the prey, not WEST toward its range anchor"
    );

    // Control: the SAME herd shape that is not a carnivore roams to its western anchor instead.
    let mut roam = land_world(40, 5);
    add_herd(
        &mut roam,
        herd(
            "roamer",
            UNRESOLVED,
            wolf_start,
            vec![UVec2::new(20, 2), UVec2::new(2, 2)],
        ),
    );
    for _ in 0..16 {
        step(&mut roam);
    }
    assert!(
        pos_of(&roam, "roamer").x < wolf_start.x,
        "the roam-only control walks WEST to its anchor — the wolf's east turn is the pursuit"
    );
}

/// **A wolf tracks a deer that keeps moving.** The deer roams steadily east (its own route anchor is
/// far east); the wolf, whose own route is a single stationary tile, moves *only* by pursuit. Wolf and
/// deer share the same one-hex cadence, so a pursuing wolf holds station within pursuit range rather
/// than being left behind, and follows the deer east across the map.
#[test]
fn a_wolf_tracks_a_deer_that_keeps_moving() {
    let mut world = land_world(48, 5);
    let wolf_start = UVec2::new(10, 2);
    let deer_start = UVec2::new(14, 2);
    add_herd(&mut world, wolf("wolf", wolf_start, vec![wolf_start]));
    add_herd(
        &mut world,
        herd(
            "deer",
            DEER,
            deer_start,
            vec![deer_start, UVec2::new(44, 2)],
        ),
    );

    let w = width_of(&world);
    let start_dist = hex_distance_wrapped(wolf_start, deer_start, w, false);

    let mut max_dist = start_dist;
    for _ in 0..24 {
        step(&mut world);
        let d = hex_distance_wrapped(pos_of(&world, "wolf"), pos_of(&world, "deer"), w, false);
        max_dist = max_dist.max(d);
    }

    assert!(
        max_dist <= start_dist + 1,
        "the wolf tracks the moving deer within ~pursuit range (never falls behind): \
         max {max_dist} vs start {start_dist}"
    );
    assert!(
        pos_of(&world, "wolf").x > wolf_start.x && pos_of(&world, "deer").x > deer_start.x,
        "both walked east — the wolf followed the deer rather than idling on empty ground"
    );
}

/// **A prey-starved wolf moves identically to a plain roam.** With no clearable prey anywhere, the
/// `pursue` dispatch builds an empty target list, so it falls straight through to the wild roam — the
/// *same* `advance_herd_roam(.., None, ..)` call the `roam` dispatch makes. Asserted against a
/// roam-only control with the identical herd shape, seed and route: the position sequences match
/// step-for-step, proving no behaviour regression on a prey-free map.
#[test]
fn a_prey_starved_wolf_moves_identically_to_a_plain_roam() {
    let route = vec![UVec2::new(20, 2), UVec2::new(2, 2)];
    let start = UVec2::new(20, 2);

    let mut pursue = land_world(40, 5);
    add_herd(&mut pursue, wolf("roamer", start, route.clone())); // carnivore, but no prey

    let mut roam = land_world(40, 5);
    add_herd(&mut roam, herd("roamer", UNRESOLVED, start, route.clone())); // plain wild roam

    let mut moved = false;
    for _ in 0..24 {
        step(&mut pursue);
        step(&mut roam);
        let p = pos_of(&pursue, "roamer");
        assert_eq!(
            p,
            pos_of(&roam, "roamer"),
            "a prey-starved wolf's step is byte-identical to the plain roam"
        );
        moved |= p != start;
    }
    assert!(
        moved,
        "the control actually roamed (the identity is not a trivial both-frozen match)"
    );
}

/// **A wolf ignores prey it cannot eat.** The only nearby herd is a mammoth (`defense 12`), which the
/// wolf's `attack 3` never clears (`attack_clears_defense` false), so it is not a pursuit target and
/// the wolf falls to its plain roam — moving identically to a wolf that is entirely alone, and never
/// stepping toward the mammoth.
#[test]
fn a_wolf_ignores_prey_it_cannot_eat() {
    let route = vec![UVec2::new(20, 2), UVec2::new(2, 2)];
    let start = UVec2::new(20, 2);
    let mammoth_at = UVec2::new(26, 2); // to the EAST — the opposite way from the wolf's anchor

    let mut with_mammoth = land_world(40, 5);
    add_herd(&mut with_mammoth, wolf("wolf", start, route.clone()));
    add_herd(
        &mut with_mammoth,
        herd("mammoth", MAMMOTH, mammoth_at, vec![mammoth_at]),
    );

    let mut alone = land_world(40, 5);
    add_herd(&mut alone, wolf("wolf", start, route.clone()));

    for _ in 0..16 {
        step(&mut with_mammoth);
        step(&mut alone);
        assert_eq!(
            pos_of(&with_mammoth, "wolf"),
            pos_of(&alone, "wolf"),
            "an uneatable mammoth does not attract the wolf — same path as a lone wolf"
        );
    }
    assert!(
        pos_of(&with_mammoth, "wolf").x < start.x,
        "the wolf roamed WEST to its anchor, never EAST toward the mammoth it cannot take"
    );
}

/// **Pursuit is deterministic.** The primitive is a pure, totally-tie-broken greedy step
/// (`resource_step_order`), with seeded RNG only in the roam fall-through — so two identical
/// wolf-and-prey worlds produce identical wolf position sequences.
#[test]
fn pursuit_is_deterministic() {
    let build = || {
        let mut world = land_world(40, 5);
        add_herd(
            &mut world,
            wolf(
                "wolf",
                UVec2::new(20, 2),
                vec![UVec2::new(20, 2), UVec2::new(2, 2)],
            ),
        );
        let deer_at = UVec2::new(26, 2);
        add_herd(&mut world, herd("deer", DEER, deer_at, vec![deer_at]));
        world
    };

    let mut a = build();
    let mut b = build();
    for _ in 0..15 {
        step(&mut a);
        step(&mut b);
        assert_eq!(
            pos_of(&a, "wolf"),
            pos_of(&b, "wolf"),
            "two identical pursuits walk the same path"
        );
    }
}
