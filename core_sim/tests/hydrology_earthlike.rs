use std::collections::BTreeMap;

use bevy::app::App;
use bevy::prelude::{UVec2, World};
use bevy::MinimalPlugins;

use core_sim::{
    generate_hydrology,
    grid_utils::{
        hex_edge_corner_indices, hex_neighbors_wrapped, HEX_CORNER_COUNT, HEX_DIRECTION_COUNT,
    },
    spawn_initial_world, CultureManager, DiscoveryProgressLedger, FactionInventory,
    GenerationRegistry, HydrologyOverrides, HydrologyState, MapPresets, MapPresetsHandle,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
};
use sim_runtime::{RiverClass, TerrainTags, TerrainType};

/// The deterministic seed every assertion in this file is tuned against.
const TEST_SEED: u64 = 119_304_647;

/// The seed sweep the structural river invariants are checked across: a handful of ordinary map
/// seeds plus the tuned one, so a join bug that only shows on some drainages cannot hide.
///
/// **Never 0**: `map_seed == 0` is the "roll a seed from entropy" sentinel (`spawn_initial_world`),
/// so a 0 here would generate a different map every run and make this test flap.
const CENSUS_SEEDS: [u64; 6] = [1, 2, 3, 4, 5, TEST_SEED];

fn earthlike_world() -> World {
    earthlike_world_seeded(TEST_SEED)
}

fn earthlike_world_seeded(seed: u64) -> World {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.hydrology = HydrologyOverrides {
        river_density: Some(1.4),
        river_min_count: Some(8),
        river_max_count: Some(24),
        accumulation_threshold_factor: Some(0.2),
        source_percentile: Some(0.55),
        source_sea_buffer: Some(0.04),
        min_length: Some(8),
        fallback_min_length: Some(4),
        spacing: Some(8.0),
        uphill_gain_pct: Some(0.07),
        ..Default::default()
    };

    app.world.insert_resource(config);
    let presets = MapPresets::builtin();
    app.world
        .insert_resource(MapPresetsHandle::new(presets.clone()));
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

    generate_hydrology(&mut app.world);
    app.world
}

fn is_water(terrain: TerrainType) -> bool {
    matches!(
        terrain,
        TerrainType::DeepOcean
            | TerrainType::ContinentalShelf
            | TerrainType::CoralShelf
            | TerrainType::HydrothermalVentField
            | TerrainType::InlandSea
            | TerrainType::NavigableRiver
    )
}

#[test]
fn earthlike_preset_generates_rivers() {
    let world = earthlike_world();
    let config = world.resource::<SimulationConfig>();
    let (w, h, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    let hydrology = world.resource::<HydrologyState>();
    assert!(
        !hydrology.rivers.is_empty(),
        "expected earthlike preset to generate rivers"
    );

    // Length is denominated in hexes: a corner step covers one hex side (~half a hex of downstream
    // progress), so `edges / 2` plus the whole-hex navigable tail is the river's reach.
    let max_len = hydrology
        .rivers
        .iter()
        .map(|r| r.edges.len() / 2 + r.navigable_hexes.len())
        .max()
        .unwrap_or(0);
    assert!(
        max_len >= 8,
        "expected at least one river to reach config minimum length, got {max_len}"
    );

    // Every accepted river carries classified edges or a navigable chain — never nothing.
    for river in &hydrology.rivers {
        assert!(
            !river.edges.is_empty() || !river.navigable_hexes.is_empty(),
            "river {} carries no geometry",
            river.id
        );
        for edge in &river.edges {
            assert!(
                edge.class.is_some(),
                "river {} has an unclassified edge",
                river.id
            );
            assert!(edge.hex.x < w && edge.hex.y < h);
            assert!(usize::from(edge.dir) < 6);
        }
    }
    let _ = wrap;
}

/// The class histogram the thresholds are tuned against: most river length Minor, a meaningful
/// Major mid-section on the bigger rivers, and only the largest one or two rivers per map going
/// Navigable (a navigable river bisects a landmass, so it must stay rare).
#[test]
fn river_classes_are_mostly_minor_with_a_rare_navigable_trunk() {
    let world = earthlike_world();
    let hydrology = world.resource::<HydrologyState>();

    let mut minor = 0usize;
    let mut major = 0usize;
    let mut navigable_rivers = 0usize;
    let mut navigable_hexes = 0usize;
    for river in &hydrology.rivers {
        for edge in &river.edges {
            match edge.class {
                RiverClass::Minor => minor += 1,
                RiverClass::Major => major += 1,
                RiverClass::None => panic!("river {} has an unclassified edge", river.id),
            }
        }
        if !river.navigable_hexes.is_empty() {
            navigable_rivers += 1;
            navigable_hexes += river.navigable_hexes.len();
        }
    }

    let total_edges = minor + major;
    assert!(total_edges > 0, "expected classified river edges");
    println!(
        "river class histogram: minor={minor} major={major} \
         (minor {:.0}% of edges), navigable_rivers={navigable_rivers} \
         navigable_hexes={navigable_hexes}, rivers={}",
        100.0 * minor as f32 / total_edges as f32,
        hydrology.rivers.len()
    );

    assert!(
        minor > major,
        "most river length must be Minor (minor={minor}, major={major})"
    );
    assert!(
        major > 0,
        "the bigger rivers must develop a Major mid-section"
    );
    assert!(
        navigable_rivers <= 2,
        "navigable rivers bisect landmasses and must stay rare, got {navigable_rivers}"
    );
}

/// A navigable river is a body of water: its hexes must be `NavigableRiver` terrain, form one
/// unbroken hex chain, and end where they meet the water body they drain into.
///
/// Swept across `CENSUS_SEEDS`, because this is also what proves the hex tracer can still route
/// downhill to the sea from the join anchor — the hex the edge chain hands off to (see
/// `navigable_chain_joins_the_edge_chain_on_a_shared_edge`).
#[test]
fn navigable_hex_chains_are_contiguous_and_terminate_at_water() {
    let mut checked = 0usize;
    for seed in CENSUS_SEEDS {
        let world = earthlike_world_seeded(seed);
        let config = world.resource::<SimulationConfig>();
        let (width, height, wrap) = (
            config.grid_size.x,
            config.grid_size.y,
            config.map_topology.wrap_horizontal,
        );
        let registry = world.resource::<TileRegistry>().clone();
        let hydrology = world.resource::<HydrologyState>();

        let terrain_at = |pos: UVec2| -> TerrainType {
            let idx = (pos.y * width + pos.x) as usize;
            world
                .get::<Tile>(registry.tiles[idx])
                .expect("tile entity exists")
                .terrain
        };

        for river in &hydrology.rivers {
            if river.navigable_hexes.is_empty() {
                continue;
            }
            checked += 1;

            for pair in river.navigable_hexes.windows(2) {
                let contiguous = hex_neighbors_wrapped(pair[0].x, pair[0].y, width, height, wrap)
                    .any(|(x, y)| UVec2::new(x, y) == pair[1]);
                assert!(
                    contiguous,
                    "seed {seed}: river {} navigable chain breaks between {:?} and {:?}",
                    river.id, pair[0], pair[1]
                );
            }

            // Every hex of the chain is water — either stamped NavigableRiver, or the mouth, which
            // the hydrology pass turns into a RiverDelta (a river deposits its load where it meets
            // the sea).
            let last = *river.navigable_hexes.last().expect("non-empty");
            for &pos in &river.navigable_hexes {
                let terrain = terrain_at(pos);
                if pos == last {
                    assert!(
                        matches!(
                            terrain,
                            TerrainType::NavigableRiver | TerrainType::RiverDelta
                        ),
                        "seed {seed}: river {} mouth {pos:?} is {terrain:?}",
                        river.id
                    );
                } else {
                    assert_eq!(
                        terrain,
                        TerrainType::NavigableRiver,
                        "seed {seed}: river {} chain hex {pos:?} was not stamped",
                        river.id
                    );
                }
            }

            // ...and the chain ends at the water body it drains into.
            let reaches_water = hex_neighbors_wrapped(last.x, last.y, width, height, wrap)
                .any(|(x, y)| is_water(terrain_at(UVec2::new(x, y))));
            assert!(
                reaches_water,
                "seed {seed}: river {} navigable chain ends at {last:?}, which borders no water",
                river.id
            );
        }
    }
    assert!(
        checked > 0,
        "expected at least one navigable river across the seed sweep"
    );

    println!("navigable rivers checked: {checked}");
}

/// The per-tile mask is the gameplay primitive: for every traced edge, BOTH flanking hexes must
/// report the same class on their respective sides.
#[test]
fn per_tile_river_edge_mask_is_symmetric() {
    let world = earthlike_world();
    let config = world.resource::<SimulationConfig>();
    let (width, height, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    let registry = world.resource::<TileRegistry>().clone();
    let hydrology = world.resource::<HydrologyState>();

    let tile_at = |pos: UVec2| -> &Tile {
        let idx = (pos.y * width + pos.x) as usize;
        world
            .get::<Tile>(registry.tiles[idx])
            .expect("tile entity exists")
    };

    let mut edges_checked = 0usize;
    for river in &hydrology.rivers {
        for edge in &river.edges {
            let (nx, ny) = core_sim::grid_utils::hex_neighbor(
                edge.hex.x,
                edge.hex.y,
                usize::from(edge.dir),
                width,
                height,
                wrap,
            )
            .expect("a traced edge has both hexes on the map");
            let neighbor = UVec2::new(nx, ny);
            let opposite = (edge.dir + 3) % 6;

            assert_eq!(
                tile_at(edge.hex).river_class_on_side(edge.dir),
                edge.class,
                "near hex {:?} disagrees with edge dir {}",
                edge.hex,
                edge.dir
            );
            assert_eq!(
                tile_at(neighbor).river_class_on_side(opposite),
                edge.class,
                "far hex {neighbor:?} disagrees with near hex {:?}",
                edge.hex
            );
            edges_checked += 1;
        }
    }
    assert!(edges_checked > 0, "expected river edges to check");
}

/// NavigableRiver is WATER-tagged, so it must never be treated as land: it must not be a shelf
/// magnet, and it must survive the tag solver's water-reduction pass.
#[test]
fn navigable_rivers_are_water_tagged() {
    let world = earthlike_world();
    let registry = world.resource::<TileRegistry>().clone();

    let mut navigable = 0usize;
    for &entity in &registry.tiles {
        let tile = world.get::<Tile>(entity).expect("tile entity exists");
        if tile.terrain != TerrainType::NavigableRiver {
            continue;
        }
        navigable += 1;
        assert!(
            tile.terrain_tags.contains(TerrainTags::WATER),
            "navigable river at {:?} is not WATER-tagged",
            tile.position
        );
        assert!(
            tile.terrain_tags.contains(TerrainTags::FRESHWATER),
            "navigable river at {:?} is not FRESHWATER-tagged",
            tile.position
        );
    }
    println!("navigable river tiles: {navigable}");
}

/// **The join invariant.** Where a river outgrows the edge model, the edge chain and the
/// `NavigableRiver` hex chain must share an **edge**, not merely a corner.
///
/// Three hexes meet at every corner, so a first navigable hex chosen from the edge the tracer
/// *stopped at* (rather than the last edge it *emitted*) can be the third hex — one the emitted
/// chain never touches. The two chains then meet only at a point, the first navigable hex carries
/// no river-edge bits at all, and the tributary visibly dead-ends at the trunk.
///
/// So: the first navigable hex must be one of the two hexes flanking the **last emitted edge**,
/// and — the fact the renderer actually consumes — that hex's `river_edges` mask must report a
/// real river class on the side facing that edge's other flanking hex.
#[test]
fn navigable_chain_joins_the_edge_chain_on_a_shared_edge() {
    let mut segments_with_navigable = 0usize;
    let mut disconnected = 0usize;
    let mut empty_mask = 0usize;
    let mut navigable_from_first_step = 0usize;
    let mut total_rivers = 0usize;
    let mut total_navigable_hexes = 0usize;

    for seed in CENSUS_SEEDS {
        let world = earthlike_world_seeded(seed);
        let config = world.resource::<SimulationConfig>();
        let (width, height, wrap) = (
            config.grid_size.x,
            config.grid_size.y,
            config.map_topology.wrap_horizontal,
        );
        let registry = world.resource::<TileRegistry>().clone();
        let hydrology = world.resource::<HydrologyState>();

        let tile_at = |pos: UVec2| -> &Tile {
            let idx = (pos.y * width + pos.x) as usize;
            world
                .get::<Tile>(registry.tiles[idx])
                .expect("tile entity exists")
        };

        total_rivers += hydrology.rivers.len();
        for river in &hydrology.rivers {
            let Some(&first) = river.navigable_hexes.first() else {
                continue;
            };
            segments_with_navigable += 1;
            total_navigable_hexes += river.navigable_hexes.len();

            let Some(last) = river.edges.last() else {
                // A river that crossed the navigable threshold on its very first step emitted no
                // edges, so there is no edge to join to. Nothing to assert.
                navigable_from_first_step += 1;
                continue;
            };

            let (nx, ny) = core_sim::grid_utils::hex_neighbor(
                last.hex.x,
                last.hex.y,
                usize::from(last.dir),
                width,
                height,
                wrap,
            )
            .expect("a traced edge has both hexes on the map");
            let far = UVec2::new(nx, ny);

            // 1. The first navigable hex flanks the last emitted edge (shares an EDGE with it).
            let side = if first == last.hex {
                Some(last.dir)
            } else if first == far {
                Some((last.dir + 3) % 6)
            } else {
                None
            };
            let Some(side) = side else {
                disconnected += 1;
                println!(
                    "seed {seed}: river {} joins at {first:?}, which flanks neither hex of its last \
                     edge ({:?} dir {})",
                    river.id, last.hex, last.dir
                );
                continue;
            };

            // 2. ...so the mask on that side is populated — the exact fact the renderer reads to
            //    draw the tributary into the trunk.
            let class = tile_at(first).river_class_on_side(side);
            if class == RiverClass::None {
                empty_mask += 1;
                println!(
                    "seed {seed}: river {} first navigable hex {first:?} carries no river edge on \
                     side {side}",
                    river.id
                );
            }
        }
    }

    println!(
        "join census over {} seeds: rivers={total_rivers}, \
         navigable-carrying segments={segments_with_navigable} \
         ({total_navigable_hexes} navigable hexes), disconnected joins={disconnected}, \
         empty masks={empty_mask}, navigable-from-first-step={navigable_from_first_step}",
        CENSUS_SEEDS.len()
    );

    assert!(
        segments_with_navigable > 0,
        "expected at least one navigable river across the seed sweep"
    );
    assert_eq!(
        disconnected, 0,
        "navigable chains must join the edge chain across a shared edge, not a bare corner"
    );
    assert_eq!(
        empty_mask, 0,
        "the first navigable hex must carry the last emitted river edge in its mask"
    );
}

/// The **inflow invariant** — the fact the renderer needs and the edge mask cannot supply.
///
/// An edge river runs *along* a side, corner to corner: it does not stop mid-edge, it stops at a
/// **vertex**. A trunk hex can flank several river edges (a real map has one flanking three), so
/// the edge mask leaves two candidate chain-ends and the client would be guessing. The sim knows,
/// and says so via `Tile::river_inflow`.
///
/// Swept over `CENSUS_SEEDS`, for every river that emitted edges *and* went navigable, the first
/// navigable **tile** — what the renderer actually reads — must report:
///
/// 1. an inflow at the corner where that river's edge chain terminated, and nowhere else: the set
///    of corners with a class is exactly the set of chain-ends arriving at that tile (usually one;
///    a confluence at a corner or two tributaries into one trunk head can add more),
/// 2. at that corner, the class of the **widest** tributary arriving there — for a lone tributary,
///    exactly the class of its last emitted edge,
/// 3. and that corner is an endpoint of that river's last emitted edge (the chains meet where the
///    water leaves the edge model).
///
/// A river that was navigable from its first step emitted no edges, so it must report **no** inflow
/// at all rather than a fabricated one.
#[test]
fn the_first_navigable_hex_reports_the_edge_chains_terminal_corner() {
    let mut tiles_with_inflow = 0usize;
    let mut minor_inflows = 0usize;
    let mut major_inflows = 0usize;
    let mut shared_corner_confluences = 0usize;
    let mut navigable_from_first_step = 0usize;

    for seed in CENSUS_SEEDS {
        let world = earthlike_world_seeded(seed);
        let config = world.resource::<SimulationConfig>();
        let (width, height, wrap) = (
            config.grid_size.x,
            config.grid_size.y,
            config.map_topology.wrap_horizontal,
        );
        let registry = world.resource::<TileRegistry>().clone();
        let hydrology = world.resource::<HydrologyState>();

        let tile_at = |pos: UVec2| -> &Tile {
            let idx = (pos.y * width + pos.x) as usize;
            world
                .get::<Tile>(registry.tiles[idx])
                .expect("tile entity exists")
        };

        // Independently of the sim's mask: which chain-ends arrive at which trunk tile, and with
        // what class. This is what the tile is then held to.
        let mut expected: BTreeMap<(u32, u32), BTreeMap<u8, RiverClass>> = BTreeMap::new();

        for river in &hydrology.rivers {
            let Some(&first) = river.navigable_hexes.first() else {
                continue;
            };
            let tile = tile_at(first);

            let Some(last) = river.edges.last() else {
                // Navigable from its first step: no edge chain, so no tributary to join.
                navigable_from_first_step += 1;
                assert_eq!(
                    tile.river_inflow, 0,
                    "seed {seed}: river {} emitted no edges, so its first navigable hex {first:?} \
                     must report no inflow",
                    river.id
                );
                continue;
            };

            // The corner this river arrives at, as the tile reports it: exactly one of the two
            // endpoints of its last emitted edge must carry a class at least as wide as that edge.
            // (Take the edge as seen from `first` — the side facing its other flanking hex — then
            // read off the two corners that side spans.)
            let (nx, ny) = core_sim::grid_utils::hex_neighbor(
                last.hex.x,
                last.hex.y,
                usize::from(last.dir),
                width,
                height,
                wrap,
            )
            .expect("a traced edge has both hexes on the map");
            let side = if first == last.hex {
                usize::from(last.dir)
            } else {
                assert_eq!(
                    first,
                    UVec2::new(nx, ny),
                    "seed {seed}: river {} joins at a hex that flanks neither end of its last edge",
                    river.id
                );
                usize::from(last.dir) + HEX_DIRECTION_COUNT / 2
            } % HEX_DIRECTION_COUNT;
            let endpoints = hex_edge_corner_indices(side).expect("side is in range");

            let arrivals: Vec<usize> = endpoints
                .iter()
                .copied()
                .filter(|&corner| tile.river_class_at_corner(corner as u8) >= last.class)
                .collect();
            assert_eq!(
                arrivals.len(),
                1,
                "seed {seed}: river {} arrives as {:?} on side {side} of {first:?} (corners \
                 {endpoints:?}), but the tile's inflow mask {:#06x} names {} of them",
                river.id,
                last.class,
                tile.river_inflow,
                arrivals.len()
            );
            let corner = arrivals[0] as u8;

            let slot = expected
                .entry((first.x, first.y))
                .or_default()
                .entry(corner)
                .or_insert(RiverClass::None);
            if *slot != RiverClass::None {
                // Two tributaries terminating at the same vertex of the same trunk hex: three hexes
                // meet at a corner, so this is a genuine confluence, and the widest wins the slot.
                shared_corner_confluences += 1;
            }
            *slot = (*slot).max(last.class);
        }

        // The tile says exactly what the traces said — no extra arms, no missing ones, and the
        // widest class where two tributaries share a vertex.
        for ((x, y), corners) in &expected {
            let tile = tile_at(UVec2::new(*x, *y));
            let reported: BTreeMap<u8, RiverClass> = (0..HEX_CORNER_COUNT as u8)
                .map(|corner| (corner, tile.river_class_at_corner(corner)))
                .filter(|(_, class)| *class != RiverClass::None)
                .collect();
            assert_eq!(
                &reported, corners,
                "seed {seed}: trunk hex ({x}, {y}) reports inflow {reported:?}, expected {corners:?}"
            );
            tiles_with_inflow += 1;
            for class in reported.values() {
                match class {
                    RiverClass::Minor => minor_inflows += 1,
                    RiverClass::Major => major_inflows += 1,
                    RiverClass::None => unreachable!("filtered above"),
                }
            }
        }

        // Nothing else on the map carries an inflow — it is set on trunk heads only.
        for (idx, &entity) in registry.tiles.iter().enumerate() {
            let tile = world.get::<Tile>(entity).expect("tile entity exists");
            let pos = (idx as u32 % width, idx as u32 / width);
            if !expected.contains_key(&pos) {
                assert_eq!(
                    tile.river_inflow, 0,
                    "seed {seed}: tile {pos:?} carries an inflow but no edge chain ends there"
                );
            }
        }
    }

    println!(
        "inflow census over {} seeds: trunk hexes with an inflow={tiles_with_inflow} \
         (corners: Minor={minor_inflows}, Major={major_inflows}), \
         shared-corner confluences={shared_corner_confluences}, \
         navigable-from-first-step (no inflow)={navigable_from_first_step}",
        CENSUS_SEEDS.len()
    );

    assert!(
        tiles_with_inflow > 0,
        "expected at least one edge chain to hand off into a navigable trunk across the sweep"
    );
}
