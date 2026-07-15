use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bevy::app::App;
use bevy::prelude::{UVec2, World};
use bevy::MinimalPlugins;

use core_sim::{
    debug_drainage_census, generate_hydrology,
    grid_utils::{
        hex_edge_corner_indices, hex_neighbors_wrapped, HEX_CORNER_COUNT, HEX_DIRECTION_COUNT,
    },
    spawn_initial_world, CultureManager, DiscoveryProgressLedger, ErosionConfig, FactionInventory,
    GenerationRegistry, HydrologyOverrides, HydrologyState, MapPresets, MapPresetsHandle,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
    BUILTIN_MAP_PRESETS,
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

/// The seed of a reported playtest map (80×52 earthlike) whose navigable rivers, under the old
/// N-independent-rivers extraction, came out as a 21-hex, 2–4 hex wide **blob** of water rather
/// than a river. Carried as its own regression seed for the anti-blob invariant.
const BLOB_REGRESSION_SEED: u64 = 7_375_689_689_846_694_675;

/// Every world in this file is built with the **shipped** hydrology config — the map a player
/// actually gets. The drainage network is whatever the landscape drains, so there is no longer a
/// "tuned" override set that manufactures a different river count.
fn earthlike_world_seeded(seed: u64) -> World {
    earthlike_world_with(seed, None)
}

fn earthlike_world_with(seed: u64, hydrology: Option<HydrologyOverrides>) -> World {
    earthlike_world_full(seed, hydrology, MapPresets::builtin())
}

/// The builtin presets with every preset's `erosion` block overridden — the A/B control for the
/// fluvial-erosion census (`enabled: false` reproduces the pre-erosion heightfield exactly).
fn presets_with_erosion(erosion: &ErosionConfig) -> Arc<MapPresets> {
    let mut file: serde_json::Value =
        serde_json::from_str(BUILTIN_MAP_PRESETS).expect("builtin map presets parse");
    let patch = serde_json::json!({
        "enabled": erosion.enabled,
        "iterations": erosion.iterations,
        "erodibility": erosion.erodibility,
        "area_exponent": erosion.area_exponent,
        "slope_exponent": erosion.slope_exponent,
        "timestep": erosion.timestep,
        "min_slope": erosion.min_slope,
        "fill_epsilon": erosion.fill_epsilon,
        "diffusivity": erosion.diffusivity,
        "incision_floor": erosion.incision_floor,
        "anchor_contour_to_sea_level": erosion.anchor_contour_to_sea_level,
    });
    for preset in file["presets"]
        .as_array_mut()
        .expect("presets is an array")
        .iter_mut()
    {
        preset["erosion"] = patch.clone();
    }
    Arc::new(MapPresets::from_json_str(&file.to_string()).expect("patched map presets parse"))
}

fn earthlike_world_full(
    seed: u64,
    hydrology: Option<HydrologyOverrides>,
    presets: Arc<MapPresets>,
) -> World {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    if let Some(hydrology) = hydrology {
        config.hydrology = hydrology;
    }

    app.world.insert_resource(config);
    app.world.insert_resource(MapPresetsHandle::new(presets));
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

fn earthlike_world() -> World {
    earthlike_world_seeded(TEST_SEED)
}

/// Every seed the structural invariants sweep: the census set plus the anti-blob regression map.
fn invariant_seeds() -> impl Iterator<Item = u64> {
    CENSUS_SEEDS
        .into_iter()
        .chain(std::iter::once(BLOB_REGRESSION_SEED))
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
    // progress), so `edges / 2` plus the whole-hex navigable tail is the river's reach. A real
    // drainage network always contains a long main stem — the trunk runs from the divide to the sea.
    //
    // Re-baselined 8 → 6 when rivers began **ending at the first standing water they touch** (a river
    // now splits into a feed-in and a drain-out segment at every lake/trunk contact instead of
    // threading one segment through — see `StemEmitter::edge_touches_water`), so the longest *single*
    // segment on this seed is shorter. The invariant is unchanged: a main stem of real length exists.
    let max_len = hydrology
        .rivers
        .iter()
        .map(|r| r.edges.len() / 2 + r.navigable_hexes.len())
        .max()
        .unwrap_or(0);
    assert!(
        max_len >= 6,
        "expected a main stem of real length, got {max_len} hexes"
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

/// **The shore-hug regression.** A river must *end* at the moment it touches standing water and a new
/// river begins where it leaves — feed-in and drain-out are separate segments, never one river threaded
/// *along* the shore. Symptom before the fix: an edge with land on one bank and a lake/sea on the other
/// was still emitted, so an edge river hugged the lakeshore (measured 30 of 62 land edge-river tiles
/// sat directly against an InlandSea/NavigableRiver on the exported seed).
///
/// **Invariant chosen (holds exactly, not merely ~0):** no emitted river EDGE has terrain standing
/// water — an InlandSea or an ocean-family tile (DeepOcean/ContinentalShelf/CoralShelf/
/// HydrothermalVentField) — on *either* bank. That is precisely the half-submerged edge the fix now
/// splits on. (`NavigableRiver` is deliberately excluded: a river's own edge→navigable hand-off shares
/// exactly one edge with its trunk *by construction* — that shared bank is the join, not a shore-hug —
/// so the "V" trunk-flank symptom is measured separately by the census, not asserted here.)
#[test]
fn edge_rivers_terminate_at_water_not_along_it() {
    // The ocean/inland-sea water an emitted edge must never border. NavigableRiver is intentionally
    // absent — see the doc comment.
    fn is_standing_water(terrain: TerrainType) -> bool {
        matches!(
            terrain,
            TerrainType::DeepOcean
                | TerrainType::ContinentalShelf
                | TerrainType::CoralShelf
                | TerrainType::HydrothermalVentField
                | TerrainType::InlandSea
        )
    }

    let mut offenders = 0usize;
    let mut emitted_edges = 0usize;
    for seed in invariant_seeds() {
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
            for edge in &river.edges {
                emitted_edges += 1;
                let near = terrain_at(edge.hex);
                let far = core_sim::grid_utils::hex_neighbor(
                    edge.hex.x,
                    edge.hex.y,
                    usize::from(edge.dir),
                    width,
                    height,
                    wrap,
                )
                .map(|(x, y)| terrain_at(UVec2::new(x, y)));
                if is_standing_water(near) || far.is_some_and(is_standing_water) {
                    offenders += 1;
                    println!(
                        "seed {seed}: river {} edge {:?} dir {} borders standing water (near {near:?}, far {far:?})",
                        river.id, edge.hex, edge.dir
                    );
                }
            }
        }
    }

    assert!(
        emitted_edges > 0,
        "expected emitted river edges across the sweep"
    );
    assert_eq!(
        offenders, 0,
        "an emitted river edge borders standing water — a river must terminate INTO the water, not run along it"
    );
}

/// The class histogram the thresholds are tuned against: most river length Minor (headwaters
/// dominate a dendritic network — that is Horton's law of stream numbers, seen from the class side),
/// a Major mid-section on the bigger rivers, and a navigable trunk on the biggest drainages.
///
/// Navigable rivers are **no longer capped at "one or two"**: a real network *should* carry a
/// handful of great rivers, and `docs/plan_rivers_drainage_network.md` settles that landmass
/// bisection is a feature of the map, not a defect. The assertion is the *shape* (Minor ≫ Major, a
/// Major section exists), never a count.
#[test]
fn river_classes_are_mostly_minor_with_a_navigable_trunk_on_the_biggest_drainages() {
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
/// **vertex**. A hex can flank several river edges, so the edge mask leaves two candidate chain-ends
/// and the client would be guessing. The sim knows, and says so via `Tile::river_inflow`.
///
/// **The semantics WIDENED with the drainage network** (`docs/plan_rivers_drainage_network.md` §A):
/// `river_inflow` no longer means "this hex is a navigable chain HEAD" — it means *"a tributary
/// hands over to the channel at this vertex"*. A real network joins tributaries to trunks
/// **mid-chain**, so an edge-only tributary that lands on a great river records its hand-over on
/// that trunk hex, not only on a chain head.
///
/// Swept over the seeds, for every river that emitted edges *and* reaches a navigable channel:
///
/// 1. the tile it hands over to reports an inflow at exactly the vertices tributaries arrive at,
///    and nowhere else (a confluence at a corner can put two tributaries on one vertex),
/// 2. at that vertex, the class of the **widest** tributary arriving there, and
/// 3. that vertex is an endpoint of that river's **last emitted edge** — checked by the hex triple
///    that identifies the vertex, so a wrong corner cannot pass.
///
/// A river that was navigable from its first step emitted no edges, so it must report **no** inflow
/// at all rather than a fabricated one.
#[test]
fn every_river_inflow_is_a_real_tributary_handover_vertex() {
    let mut tiles_with_inflow = 0usize;
    let mut minor_inflows = 0usize;
    let mut major_inflows = 0usize;
    let mut shared_corner_confluences = 0usize;
    let mut mid_chain_handovers = 0usize;
    let mut chain_head_handovers = 0usize;
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

        // A vertex is identified by the THREE hexes that meet at it — a wrap-safe, model-free
        // identity. Corner `i` of `H` is spanned by sides `i` and `i + 1` (`hex_edge_corner_indices`
        // says side `d` spans corners `{d - 1, d}`), so it is shared with the neighbours in exactly
        // those two directions.
        let vertex_of = |pos: UVec2, corner: u8| -> BTreeSet<(u32, u32)> {
            let mut hexes = BTreeSet::new();
            hexes.insert((pos.x, pos.y));
            for offset in 0..2u8 {
                let dir = (usize::from(corner) + usize::from(offset)) % HEX_DIRECTION_COUNT;
                if let Some((x, y)) =
                    core_sim::grid_utils::hex_neighbor(pos.x, pos.y, dir, width, height, wrap)
                {
                    hexes.insert((x, y));
                }
            }
            hexes
        };

        // Independently of the sim's mask: which hand-overs arrive at which tile, and with what
        // class. The tile is then held to exactly that.
        let mut expected: BTreeMap<(u32, u32), BTreeMap<u8, RiverClass>> = BTreeMap::new();

        for river in &hydrology.rivers {
            let Some(inflow) = river.navigable_inflow.as_ref() else {
                if river.edges.is_empty() && !river.navigable_hexes.is_empty() {
                    navigable_from_first_step += 1;
                }
                continue;
            };
            let last = river
                .edges
                .last()
                .expect("an inflow is only recorded for a river that emitted edges");

            // (2) the class it arrives with is the class of its last emitted edge.
            assert_eq!(
                inflow.class, last.class,
                "seed {seed}: river {} hands over as {:?} but its last edge is {:?}",
                river.id, inflow.class, last.class
            );

            // (3) the hand-over vertex is an endpoint of that last emitted edge.
            let arrival = vertex_of(inflow.hex, inflow.corner);
            let [a, b] = hex_edge_corner_indices(usize::from(last.dir)).expect("dir in range");
            let endpoints = [vertex_of(last.hex, a as u8), vertex_of(last.hex, b as u8)];
            assert!(
                endpoints.contains(&arrival),
                "seed {seed}: river {} hands over at vertex {arrival:?}, which is not an endpoint \
                 of its last emitted edge ({:?} dir {}) — endpoints {endpoints:?}",
                river.id,
                last.hex,
                last.dir
            );

            // (1) the hand-over hex is a navigable channel — its own chain head, or a trunk hex
            //     mid-chain (the widened semantics). A chain short enough that its head IS its mouth
            //     is stamped `RiverDelta` (a river deposits its load where it meets the water), and
            //     that tile still carries the channel.
            let terrain = tile_at(inflow.hex).terrain;
            assert!(
                matches!(
                    terrain,
                    TerrainType::NavigableRiver | TerrainType::RiverDelta
                ),
                "seed {seed}: river {} hands over to {:?}, which is {terrain:?} — not a channel",
                river.id,
                inflow.hex
            );
            if river.navigable_hexes.first() == Some(&inflow.hex) {
                chain_head_handovers += 1;
            } else {
                mid_chain_handovers += 1;
            }

            let slot = expected
                .entry((inflow.hex.x, inflow.hex.y))
                .or_default()
                .entry(inflow.corner)
                .or_insert(RiverClass::None);
            if *slot != RiverClass::None {
                // Two tributaries handing over at the same vertex of the same hex: three hexes meet
                // at a corner, so this is a genuine confluence, and the widest wins the slot.
                shared_corner_confluences += 1;
            }
            *slot = (*slot).max(inflow.class);
        }

        // The tile says exactly what the rivers said — no extra arms, no missing ones, and the
        // widest class where two tributaries share a vertex.
        for ((x, y), corners) in &expected {
            let tile = tile_at(UVec2::new(*x, *y));
            let reported: BTreeMap<u8, RiverClass> = (0..HEX_CORNER_COUNT as u8)
                .map(|corner| (corner, tile.river_class_at_corner(corner)))
                .filter(|(_, class)| *class != RiverClass::None)
                .collect();
            assert_eq!(
                &reported, corners,
                "seed {seed}: hex ({x}, {y}) reports inflow {reported:?}, expected {corners:?}"
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

        // Nothing else on the map carries an inflow — no invented hand-overs.
        for (idx, &entity) in registry.tiles.iter().enumerate() {
            let tile = world.get::<Tile>(entity).expect("tile entity exists");
            let pos = (idx as u32 % width, idx as u32 / width);
            if !expected.contains_key(&pos) {
                assert_eq!(
                    tile.river_inflow, 0,
                    "seed {seed}: tile {pos:?} carries an inflow but no tributary hands over there"
                );
            }
        }
    }

    println!(
        "inflow census over {} seeds: hexes with an inflow={tiles_with_inflow} \
         (corners: Minor={minor_inflows}, Major={major_inflows}), \
         chain-head hand-overs={chain_head_handovers}, mid-chain hand-overs={mid_chain_handovers}, \
         shared-corner confluences={shared_corner_confluences}, \
         navigable-from-first-step (no inflow)={navigable_from_first_step}",
        CENSUS_SEEDS.len()
    );

    assert!(
        tiles_with_inflow > 0,
        "expected at least one edge chain to hand off into a navigable channel across the sweep"
    );
    assert!(
        mid_chain_handovers > 0,
        "a real drainage network joins tributaries to trunks MID-CHAIN — that is the whole point of \
         widening `river_inflow`; if none happen, the network is still a set of parallel rivers"
    );
}

/// The **anti-blob invariant**: a navigable river is a PATH of water hexes, not a lake.
///
/// The fact the renderer and a future movement system consume is the **channel-exit mask**, so that
/// is what the path property is asserted on: a hex in the middle of a chain links to exactly **2**
/// channel neighbours (upstream + downstream), an endpoint (head or mouth) to **1**, and a
/// **confluence** — where a tributary merges into a trunk, which `truncate_at_existing_channel`
/// deliberately creates — to **3**. **4 or more has no hydrological reading at all**: it means water
/// is spreading in two dimensions, which is a lake.
///
/// **Why not raw terrain adjacency (the old formulation)?** A hex chain that turns 60° puts hex `k`
/// *adjacent* to hex `k+2` — the three hexes at a bend are mutually adjacent, unavoidably. So a
/// bending chain with a tributary merging at the bend touches 4 navigable hexes while remaining a
/// perfectly good path, and the old "≤ 3 navigable neighbours" bound measures the bend, not a blob.
/// Terrain adjacency is still bounded here, at the geometric ceiling that a *chain* can reach: 2
/// chain links + one skip-adjacency from a 60° bend + one merging tributary.
///
/// Cause of the original blob, worth restating because these bounds only hold while the fix does:
/// several branches of one drainage each crossed the navigable threshold *independently* and each
/// traced its own chain to the same sink, packing side by side. Rivers merge on contact in the
/// world; now they do here too — and with a real drainage tree the branches merge *upstream* of the
/// threshold in the first place.
#[test]
fn navigable_rivers_are_paths_not_blobs() {
    /// A confluence hex legitimately links 3 channel neighbours (trunk in, trunk out, tributary in).
    const MAX_CHANNEL_LINKS: usize = 3;
    /// ...and can *touch* one more navigable hex than that, purely from a 60° bend in its own chain.
    const MAX_NAVIGABLE_NEIGHBORS: usize = MAX_CHANNEL_LINKS + 1;

    let mut total_navigable = 0usize;
    let mut confluences = 0usize;

    for seed in invariant_seeds() {
        let world = earthlike_world_seeded(seed);
        let config = world.resource::<SimulationConfig>();
        let (width, height, wrap) = (
            config.grid_size.x,
            config.grid_size.y,
            config.map_topology.wrap_horizontal,
        );
        let registry = world.resource::<TileRegistry>().clone();

        let tile_at = |pos: UVec2| -> &Tile {
            let idx = (pos.y * width + pos.x) as usize;
            world
                .get::<Tile>(registry.tiles[idx])
                .expect("tile entity exists")
        };
        let is_navigable =
            |pos: UVec2| -> bool { tile_at(pos).terrain == TerrainType::NavigableRiver };

        for y in 0..height {
            for x in 0..width {
                let pos = UVec2::new(x, y);
                if !is_navigable(pos) {
                    continue;
                }
                total_navigable += 1;

                let links = (0..HEX_DIRECTION_COUNT as u8)
                    .filter(|&dir| tile_at(pos).channel_exits(dir))
                    .filter(|&dir| {
                        core_sim::grid_utils::hex_neighbor(
                            x,
                            y,
                            usize::from(dir),
                            width,
                            height,
                            wrap,
                        )
                        .map(|(nx, ny)| is_navigable(UVec2::new(nx, ny)))
                        .unwrap_or(false)
                    })
                    .count();
                if links == MAX_CHANNEL_LINKS {
                    confluences += 1;
                }
                assert!(
                    links <= MAX_CHANNEL_LINKS,
                    "seed {seed}: navigable hex {pos:?} links to {links} navigable channel hexes — \
                     a path hex links to 2 (1 at an end, 3 at a confluence); more than \
                     {MAX_CHANNEL_LINKS} means the channel has spread into a 2D blob of water"
                );

                let neighbors = hex_neighbors_wrapped(x, y, width, height, wrap)
                    .filter(|&(nx, ny)| is_navigable(UVec2::new(nx, ny)))
                    .count();
                assert!(
                    neighbors <= MAX_NAVIGABLE_NEIGHBORS,
                    "seed {seed}: navigable hex {pos:?} touches {neighbors} navigable hexes — more \
                     than a bending chain plus a confluence can explain ({MAX_NAVIGABLE_NEIGHBORS})"
                );
            }
        }
    }

    assert!(
        total_navigable > 0,
        "expected the sweep to produce navigable rivers at all"
    );
    println!(
        "navigable-river shape census: {total_navigable} navigable hexes, \
         {confluences} channel confluences (3 links), 0 with 4+"
    );
}

// ---------------------------------------------------------------------------
// The drainage census — the measurement instrument for this arc.
//
// Asserts nothing; it reports. Run with:
//   cargo test -p core_sim --test hydrology_earthlike drainage_census -- --ignored --nocapture
// The structural properties it measures ARE asserted, by
// `the_drainage_network_has_confluences_and_obeys_hortons_laws` below.
// ---------------------------------------------------------------------------

/// Summary of one distribution.
struct Stats {
    count: usize,
    max: f64,
    p99: f64,
    p95: f64,
    p50: f64,
    mean: f64,
}

impl Stats {
    /// Nearest-rank percentiles over the sorted samples (`p50` = median).
    fn of(mut samples: Vec<f64>) -> Self {
        samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaNs in the census"));
        let count = samples.len();
        let percentile = |q: f64| -> f64 {
            if count == 0 {
                return 0.0;
            }
            let rank = ((q * count as f64).ceil() as usize).clamp(1, count);
            samples[rank - 1]
        };
        Self {
            count,
            max: samples.last().copied().unwrap_or(0.0),
            p99: percentile(0.99),
            p95: percentile(0.95),
            p50: percentile(0.50),
            mean: if count == 0 {
                0.0
            } else {
                samples.iter().sum::<f64>() / count as f64
            },
        }
    }

    fn row(&self, label: &str) -> String {
        format!(
            "  {label:<26} n={:<7} max={:<9.2} p99={:<8.2} p95={:<8.2} p50={:<7.2} mean={:.2}",
            self.count, self.max, self.p99, self.p95, self.p50, self.mean
        )
    }
}

/// A corner has 3 neighbours, so the contributor histogram runs 0..=3.
const MAX_CORNER_CONTRIBUTORS: usize = 3;

/// The two LANDSCAPE failures the fluvial-erosion arc exists to fix, measured independently of the
/// river extraction (see `core_sim/CLAUDE.md` → Rivers):
///
/// - **SPONGE** — the share of the largest landmass's tiles that touch water. A compact blob is
///   ~14%; an iso-contour of fractal noise is 48–64%, and a sponge cannot grow basins because every
///   tile is a hex or two from its own private outlet. Must FALL.
/// - **CAPTURE** — the biggest basin as a share of that landmass. 2–4% on seeds 1/3 (noise domes
///   shedding radially) vs 46% on seed 4 (one trunk captures the interior). Must RISE, and the
///   spread between seeds must NARROW.
///
/// `NavigableRiver` hexes count as **land** here, deliberately: they are the thing this arc is
/// trying to create, so counting them as water would let a successful run register as *more* sponge
/// (every new river hex would coast its neighbours) and would bisect the landmass it just drained.
/// The metric is the shoreline of the continent, not of its rivers.
struct Landscape {
    largest_landmass: usize,
    coastal: usize,
    max_basin: f64,
}

impl Landscape {
    fn of(world: &World, accumulation: &[f64]) -> Self {
        let config = world.resource::<SimulationConfig>();
        let (w, h, wrap) = (
            config.grid_size.x,
            config.grid_size.y,
            config.map_topology.wrap_horizontal,
        );
        let registry = world.resource::<TileRegistry>().clone();
        let total = (w * h) as usize;

        let is_land: Vec<bool> = (0..total)
            .map(|i| {
                let tile = world
                    .get::<Tile>(registry.tiles[i])
                    .expect("tile entity exists");
                tile.terrain == TerrainType::NavigableRiver || !is_water(tile.terrain)
            })
            .collect();

        // Largest connected landmass (hex adjacency, honouring horizontal wrap).
        let mut component = vec![usize::MAX; total];
        let mut sizes: Vec<usize> = Vec::new();
        for start in 0..total {
            if !is_land[start] || component[start] != usize::MAX {
                continue;
            }
            let id = sizes.len();
            let mut size = 0usize;
            let mut stack = vec![start];
            component[start] = id;
            while let Some(idx) = stack.pop() {
                size += 1;
                let (x, y) = ((idx as u32) % w, (idx as u32) / w);
                for (nx, ny) in hex_neighbors_wrapped(x, y, w, h, wrap) {
                    let n = (ny * w + nx) as usize;
                    if is_land[n] && component[n] == usize::MAX {
                        component[n] = id;
                        stack.push(n);
                    }
                }
            }
            sizes.push(size);
        }
        let largest_id = sizes
            .iter()
            .enumerate()
            .max_by_key(|(id, size)| (**size, std::cmp::Reverse(*id)))
            .map(|(id, _)| id);

        let mut largest_landmass = 0usize;
        let mut coastal = 0usize;
        if let Some(largest_id) = largest_id {
            for (idx, &owner) in component.iter().enumerate() {
                if owner != largest_id {
                    continue;
                }
                largest_landmass += 1;
                let (x, y) = ((idx as u32) % w, (idx as u32) / w);
                if hex_neighbors_wrapped(x, y, w, h, wrap)
                    .any(|(nx, ny)| !is_land[(ny * w + nx) as usize])
                {
                    coastal += 1;
                }
            }
        }

        Self {
            largest_landmass,
            coastal,
            // The biggest basin on the map: the largest land-corner flow accumulation, in
            // hex-equivalents — directly comparable to a tile count.
            max_basin: accumulation.iter().copied().fold(0.0, f64::max),
        }
    }

    fn coastal_pct(&self) -> f64 {
        100.0 * self.coastal as f64 / self.largest_landmass.max(1) as f64
    }

    fn basin_pct(&self) -> f64 {
        100.0 * self.max_basin / self.largest_landmass.max(1) as f64
    }
}

/// One map's drainage shape, gathered from the sim.
struct MapCensus {
    accumulation: Vec<f64>,
    discharge: Vec<f64>,
    land_contributors: [usize; MAX_CORNER_CONTRIBUTORS + 1],
    land_orders: BTreeMap<u8, usize>,
    channel_contributors: [usize; MAX_CORNER_CONTRIBUTORS + 1],
    channel_orders: BTreeMap<u8, usize>,
    segment_orders: BTreeMap<u8, usize>,
    navigable_runs: Vec<usize>,
    rivers: usize,
    minor: usize,
    major: usize,
    /// Land tiles carrying an edge river that sit hex-adjacent to InlandSea/NavigableRiver — the
    /// shore-hug proxy (was 30 of 62 before rivers began ending at first water contact).
    shore_hug: usize,
    /// Total land tiles carrying an edge river (the shore-hug denominator; the "62").
    edge_river_tiles: usize,
    /// The **"V" metric**: the most sides of a single `NavigableRiver` trunk hex flanked by an edge
    /// river (was 2 — a tributary ran up two sides of the trunk before handing over at a far corner).
    max_trunk_flank: usize,
}

impl MapCensus {
    fn of(world: &World) -> Self {
        let census = debug_drainage_census(world);
        let hydrology = world.resource::<HydrologyState>();

        // Tile-level shore-hug + trunk-flank metrics (the fix's direct targets).
        let config = world.resource::<SimulationConfig>();
        let (width, height, wrap) = (
            config.grid_size.x,
            config.grid_size.y,
            config.map_topology.wrap_horizontal,
        );
        let registry = world.resource::<TileRegistry>().clone();
        let tile_at = |pos: UVec2| -> &Tile {
            let idx = (pos.y * width + pos.x) as usize;
            world
                .get::<Tile>(registry.tiles[idx])
                .expect("tile entity exists")
        };
        let mut shore_hug = 0usize;
        let mut edge_river_tiles = 0usize;
        let mut max_trunk_flank = 0usize;
        for y in 0..height {
            for x in 0..width {
                let pos = UVec2::new(x, y);
                let tile = tile_at(pos);
                if tile.terrain == TerrainType::NavigableRiver {
                    let flank = (0..HEX_DIRECTION_COUNT as u8)
                        .filter(|&d| tile.river_class_on_side(d) != RiverClass::None)
                        .count();
                    max_trunk_flank = max_trunk_flank.max(flank);
                } else if !is_water(tile.terrain) && tile.has_any_river_edge() {
                    edge_river_tiles += 1;
                    let against_water =
                        hex_neighbors_wrapped(x, y, width, height, wrap).any(|(nx, ny)| {
                            matches!(
                                tile_at(UVec2::new(nx, ny)).terrain,
                                TerrainType::InlandSea | TerrainType::NavigableRiver
                            )
                        });
                    if against_water {
                        shore_hug += 1;
                    }
                }
            }
        }

        let mut land_contributors = [0usize; MAX_CORNER_CONTRIBUTORS + 1];
        for &c in &census.land_contributors {
            land_contributors[(c as usize).min(MAX_CORNER_CONTRIBUTORS)] += 1;
        }
        let mut channel_contributors = [0usize; MAX_CORNER_CONTRIBUTORS + 1];
        for &c in &census.channel_contributors {
            channel_contributors[(c as usize).min(MAX_CORNER_CONTRIBUTORS)] += 1;
        }
        let mut land_orders: BTreeMap<u8, usize> = BTreeMap::new();
        for &o in &census.land_orders {
            *land_orders.entry(o).or_default() += 1;
        }
        let mut channel_orders: BTreeMap<u8, usize> = BTreeMap::new();
        for &o in &census.channel_orders {
            *channel_orders.entry(o).or_default() += 1;
        }
        let mut segment_orders: BTreeMap<u8, usize> = BTreeMap::new();
        let mut minor = 0usize;
        let mut major = 0usize;
        let mut discharge = Vec::new();
        let mut navigable_runs = Vec::new();
        for river in &hydrology.rivers {
            *segment_orders.entry(river.order).or_default() += 1;
            for edge in &river.edges {
                discharge.push(f64::from(edge.discharge));
                match edge.class {
                    RiverClass::Minor => minor += 1,
                    RiverClass::Major => major += 1,
                    RiverClass::None => {}
                }
            }
            if !river.navigable_hexes.is_empty() {
                navigable_runs.push(river.navigable_hexes.len());
            }
        }

        Self {
            accumulation: census
                .land_accumulation
                .iter()
                .map(|&v| f64::from(v))
                .collect(),
            discharge,
            land_contributors,
            land_orders,
            channel_contributors,
            channel_orders,
            segment_orders,
            navigable_runs,
            rivers: hydrology.rivers.len(),
            minor,
            major,
            shore_hug,
            edge_river_tiles,
            max_trunk_flank,
        }
    }

    /// Corners where two streams meet. **Structurally capped at 2 contributors** off a sink: a corner
    /// has 3 neighbours and one of them is its own downstream, which a strict descent tree can never
    /// route back. So `2` *is* the confluence bucket, and a 3-bucket can never be populated.
    fn land_confluences(&self) -> usize {
        self.land_contributors[2..].iter().sum()
    }

    fn land_corners(&self) -> usize {
        self.land_contributors.iter().sum()
    }

    fn channel_confluences(&self) -> usize {
        self.channel_contributors[2..].iter().sum()
    }
}

fn histogram(counts: &BTreeMap<u8, usize>) -> String {
    counts
        .iter()
        .map(|(k, n)| format!("o{k}={n}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn report(label: &str, c: &MapCensus) {
    let edges = c.minor + c.major;
    let pct = |n: usize| -> f64 {
        if edges == 0 {
            0.0
        } else {
            100.0 * n as f64 / edges as f64
        }
    };
    println!("{label}");
    println!("{}", Stats::of(c.discharge.clone()).row("edge discharge"));
    println!(
        "{}",
        Stats::of(c.accumulation.clone()).row("corner accum (land)")
    );
    println!(
        "  {:<26} 0={:<6} 1={:<6} 2={:<6} 3={:<5} | confluences(=2)={} of {} land corners ({:.1}%)",
        "land contributors",
        c.land_contributors[0],
        c.land_contributors[1],
        c.land_contributors[2],
        c.land_contributors[3],
        c.land_confluences(),
        c.land_corners(),
        100.0 * c.land_confluences() as f64 / c.land_corners().max(1) as f64
    );
    println!(
        "  {:<26} 0={:<6} 1={:<6} 2={:<6} | confluences(>=2)={}",
        "channel contributors",
        c.channel_contributors[0],
        c.channel_contributors[1],
        c.channel_contributors[2],
        c.channel_confluences()
    );
    println!(
        "  {:<26} {}",
        "strahler (drainage tree)",
        histogram(&c.land_orders)
    );
    println!(
        "  {:<26} {}",
        "strahler (channel corners)",
        histogram(&c.channel_orders)
    );
    println!(
        "  {:<26} {}",
        "strahler (segments)",
        histogram(&c.segment_orders)
    );
    println!(
        "  {:<26} segments={} hexes={} runs={:?}",
        "navigable",
        c.navigable_runs.len(),
        c.navigable_runs.iter().sum::<usize>(),
        c.navigable_runs
    );
    println!(
        "  {:<26} minor={} ({:.1}%) major={} ({:.1}%) total={edges}",
        "class histogram",
        c.minor,
        pct(c.minor),
        c.major,
        pct(c.major)
    );
    println!("  {:<26} {}", "rivers (segments)", c.rivers);
    println!(
        "  {:<26} shore-hug={}/{} ({:.1}%) | max trunk flank (V)={}",
        "shore-hug + V (fix)",
        c.shore_hug,
        c.edge_river_tiles,
        if c.edge_river_tiles == 0 {
            0.0
        } else {
            100.0 * c.shore_hug as f64 / c.edge_river_tiles as f64
        },
        c.max_trunk_flank
    );
}

/// One arm of the census: every seed under one `erosion` setting. Prints the per-seed drainage
/// report and the landscape (sponge/capture) table, and returns the aggregate of both.
fn census_pass(label: &str, erosion: &ErosionConfig) -> (MapCensus, Vec<(u64, Landscape)>) {
    let mut aggregate = MapCensus {
        accumulation: Vec::new(),
        discharge: Vec::new(),
        land_contributors: [0; MAX_CORNER_CONTRIBUTORS + 1],
        land_orders: BTreeMap::new(),
        channel_contributors: [0; MAX_CORNER_CONTRIBUTORS + 1],
        channel_orders: BTreeMap::new(),
        segment_orders: BTreeMap::new(),
        navigable_runs: Vec::new(),
        rivers: 0,
        minor: 0,
        major: 0,
        shore_hug: 0,
        edge_river_tiles: 0,
        max_trunk_flank: 0,
    };
    let mut landscapes: Vec<(u64, Landscape)> = Vec::new();

    println!("\n############ EROSION {label} ############");
    println!("erosion: {erosion:?}");

    let presets = presets_with_erosion(erosion);
    for seed in CENSUS_SEEDS {
        let world = earthlike_world_full(seed, None, presets.clone());
        let config = world.resource::<SimulationConfig>();
        let (width, height) = (config.grid_size.x, config.grid_size.y);
        let c = MapCensus::of(&world);
        let landscape = Landscape::of(&world, &c.accumulation);
        report(&format!("\n--- seed {seed} ({width}x{height}) ---"), &c);
        println!(
            "  {:<26} landmass={} coastal={:.1}% | max basin={:.0} ({:.1}% of landmass)",
            "landscape",
            landscape.largest_landmass,
            landscape.coastal_pct(),
            landscape.max_basin,
            landscape.basin_pct()
        );
        landscapes.push((seed, landscape));

        aggregate.accumulation.extend(c.accumulation);
        aggregate.discharge.extend(c.discharge);
        for slot in 0..=MAX_CORNER_CONTRIBUTORS {
            aggregate.land_contributors[slot] += c.land_contributors[slot];
            aggregate.channel_contributors[slot] += c.channel_contributors[slot];
        }
        for (order, n) in c.land_orders {
            *aggregate.land_orders.entry(order).or_default() += n;
        }
        for (order, n) in c.channel_orders {
            *aggregate.channel_orders.entry(order).or_default() += n;
        }
        for (order, n) in c.segment_orders {
            *aggregate.segment_orders.entry(order).or_default() += n;
        }
        aggregate.navigable_runs.extend(c.navigable_runs);
        aggregate.rivers += c.rivers;
        aggregate.minor += c.minor;
        aggregate.major += c.major;
        aggregate.shore_hug += c.shore_hug;
        aggregate.edge_river_tiles += c.edge_river_tiles;
        aggregate.max_trunk_flank = aggregate.max_trunk_flank.max(c.max_trunk_flank);
    }

    report(
        &format!(
            "\n=== AGGREGATE over {} seeds ({label}) ===",
            CENSUS_SEEDS.len()
        ),
        &aggregate,
    );
    (aggregate, landscapes)
}

/// The two landscape metrics, A/B, in one table.
fn landscape_table(off: &[(u64, Landscape)], on: &[(u64, Landscape)]) {
    println!(
        "\n=== LANDSCAPE A/B (largest landmass per seed) ===\n\
         {:<22} | {:>26} | {:>26}",
        "", "EROSION OFF", "EROSION ON"
    );
    println!(
        "{:<22} | {:>7} {:>7} {:>10} | {:>7} {:>7} {:>10}",
        "seed", "tiles", "coastal", "basin/land", "tiles", "coastal", "basin/land"
    );
    for ((seed, a), (_, b)) in off.iter().zip(on.iter()) {
        println!(
            "{:<22} | {:>7} {:>6.1}% {:>9.1}% | {:>7} {:>6.1}% {:>9.1}%",
            seed,
            a.largest_landmass,
            a.coastal_pct(),
            a.basin_pct(),
            b.largest_landmass,
            b.coastal_pct(),
            b.basin_pct()
        );
    }
    let mean = |xs: &[(u64, Landscape)], f: fn(&Landscape) -> f64| -> f64 {
        xs.iter().map(|(_, l)| f(l)).sum::<f64>() / xs.len().max(1) as f64
    };
    let spread = |xs: &[(u64, Landscape)], f: fn(&Landscape) -> f64| -> f64 {
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for (_, l) in xs {
            lo = lo.min(f(l));
            hi = hi.max(f(l));
        }
        hi - lo
    };
    println!(
        "{:<22} | {:>7} {:>6.1}% {:>9.1}% | {:>7} {:>6.1}% {:>9.1}%",
        "MEAN",
        "",
        mean(off, Landscape::coastal_pct),
        mean(off, Landscape::basin_pct),
        "",
        mean(on, Landscape::coastal_pct),
        mean(on, Landscape::basin_pct)
    );
    println!(
        "{:<22} | {:>7} {:>6.1}% {:>9.1}% | {:>7} {:>6.1}% {:>9.1}%",
        "SPREAD (max-min)",
        "",
        spread(off, Landscape::coastal_pct),
        spread(off, Landscape::basin_pct),
        "",
        spread(on, Landscape::coastal_pct),
        spread(on, Landscape::basin_pct)
    );
}

#[test]
#[ignore = "measurement harness, not an assertion"]
fn drainage_census() {
    println!("\n=== DRAINAGE CENSUS (shipped config, earthlike, default grid) ===");
    println!("discharge unit: precipitation-weighted upstream drainage area in HEX-EQUIVALENTS");
    {
        // Print the thresholds the census actually ran at, rather than a comment that can rot.
        let shipped = SimulationConfig::builtin().hydrology;
        println!(
            "thresholds (shipped): channel_min={:?} / river_density={:?}, major>={:?}, \
             navigable>={:?}",
            shipped.channel_min_discharge,
            shipped.river_density,
            shipped.class_major_min_discharge,
            shipped.class_navigable_min_discharge
        );
    }

    // The A/B: `enabled: false` is the pre-erosion heightfield exactly, so the two arms are
    // directly comparable at identical river thresholds.
    let off_cfg = ErosionConfig {
        enabled: false,
        ..ErosionConfig::default()
    };
    let (off, off_landscapes) = census_pass("OFF (control)", &off_cfg);
    let (on, on_landscapes) = census_pass("ON (shipped)", &ErosionConfig::default());

    landscape_table(&off_landscapes, &on_landscapes);

    let runs = |c: &MapCensus| -> String {
        let max = c.navigable_runs.iter().copied().max().unwrap_or(0);
        let mean = if c.navigable_runs.is_empty() {
            0.0
        } else {
            c.navigable_runs.iter().sum::<usize>() as f64 / c.navigable_runs.len() as f64
        };
        format!(
            "segments={:<3} hexes={:<4} mean run={mean:.1} MAX RUN={max}",
            c.navigable_runs.len(),
            c.navigable_runs.iter().sum::<usize>()
        )
    };
    println!(
        "\n=== NAVIGABLE RIVERS A/B (aggregate over {} seeds) ===",
        CENSUS_SEEDS.len()
    );
    println!("  OFF: {}", runs(&off));
    println!("  ON : {}", runs(&on));
    println!();
}

/// **The structural payoff of the drainage network**, asserted rather than eyeballed.
///
/// Both properties are measured on the **whole drainage tree** (every land corner), not on the
/// thresholded channel set — so they test the *landscape routing*, and stay true whatever the class
/// thresholds are later tuned to.
///
/// 1. **Confluences exist in quantity.** A corner has 3 neighbours, one of which is its own
///    downstream — so a non-sink corner has **at most 2** contributors, and a 2-contributor corner
///    *is* a confluence. (The design doc's "the 3-contributor bucket must be populated" is
///    structurally impossible on a 3-regular lattice with a strict descent tree; the 2-bucket is the
///    confluence bucket. See the report in `docs/plan_rivers_drainage_network.md`.)
/// 2. **Strahler follows Horton's law of stream numbers** — order counts fall off geometrically.
///    Asserted on SHAPE, never on exact values: every order is strictly rarer than the one below it,
///    and the defining signature — **headwaters dominate** — is held to a real bifurcation ratio
///    (`o1 ≥ 3 × o2`; measured 3.9–8.2 across the sweep). The *upper* orders are one or two trunks
///    per map, where a ratio is small-sample noise (a real map measured `o3 = 198, o4 = 120`), so
///    they are held only to the monotone decrease.
#[test]
fn the_drainage_network_has_confluences_and_obeys_hortons_laws() {
    for seed in CENSUS_SEEDS {
        let world = earthlike_world_seeded(seed);
        let c = MapCensus::of(&world);
        let land_corners = c.land_corners();
        assert!(land_corners > 0, "seed {seed}: no land corners at all");

        // (1) A dendritic tree is MADE of confluences. A network that merely sheets off the coast has
        //     almost none. 5% of land corners is a floor well below what a gathering tree produces,
        //     chosen so this asserts the STRUCTURE, not the tuning.
        let confluences = c.land_confluences();
        assert!(
            confluences * 20 >= land_corners,
            "seed {seed}: only {confluences} confluences among {land_corners} land corners — the \
             drainage is not gathering into a tree"
        );
        assert_eq!(
            c.land_contributors[3], 0,
            "seed {seed}: a non-sink corner cannot have 3 contributors — one of its 3 neighbours is \
             its own downstream"
        );

        // (2) Horton's law of stream numbers.
        /// The measured bifurcation ratio of the first order is 3.9–8.2; a network that merely
        /// sheets off the coast would sit near 1.
        const MIN_FIRST_ORDER_BIFURCATION: usize = 3;

        let orders: Vec<(u8, usize)> = c.land_orders.iter().map(|(&o, &n)| (o, n)).collect();
        assert!(
            orders.len() >= 3,
            "seed {seed}: a real drainage tree reaches at least 3 Strahler orders, got {orders:?}"
        );
        for pair in orders.windows(2) {
            let ((lower, lower_n), (higher, higher_n)) = (pair[0], pair[1]);
            assert_eq!(higher, lower + 1, "seed {seed}: order gap in {orders:?}");
            assert!(
                lower_n > higher_n,
                "seed {seed}: Strahler order {higher} ({higher_n}) is not rarer than order {lower} \
                 ({lower_n}) — the tree does not fall off geometrically: {orders:?}"
            );
        }
        let (_, first) = orders[0];
        let (_, second) = orders[1];
        assert!(
            first >= MIN_FIRST_ORDER_BIFURCATION * second,
            "seed {seed}: order-1 streams ({first}) do not dominate order-2 ({second}) — headwaters \
             are supposed to be the bulk of a dendritic network: {orders:?}"
        );
    }
}

/// The **channel-exit mask** (`Tile::river_channel`) is what the client draws the trunk from, and it
/// exists because terrain alone cannot say which neighbours a navigable hex is chained to — a
/// renderer that armed every navigable/water neighbour drew a cross-linked **web** wherever two
/// chains ran adjacent. So the mask must be exactly the chain:
///
/// 1. **symmetric** — consecutive chain hexes agree about the channel between them,
/// 2. **connected end-to-end** — every consecutive pair in every chain is armed, so the client can
///    walk a chain from head to mouth through the bits alone,
/// 3. **reaches the water** — a chain's final hex (unless it merged into a trunk, which carries the
///    water on) exits toward the ocean / inland sea / `RiverDelta` it drains into, so the drawn
///    river does not stop one hex short of the sea,
/// 4. **anti-web** — no navigable hex exits toward a navigable hex that is not its neighbour in some
///    segment's chain. This is the invariant the whole mask exists to enforce.
#[test]
fn navigable_channel_exits_are_the_chain_and_only_the_chain() {
    let mut checked_chains = 0usize;
    let mut mouths = 0usize;

    for seed in invariant_seeds() {
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
        let step = |pos: UVec2, dir: u8| -> Option<UVec2> {
            core_sim::grid_utils::hex_neighbor(pos.x, pos.y, usize::from(dir), width, height, wrap)
                .map(|(x, y)| UVec2::new(x, y))
        };
        let opposite = |dir: u8| -> u8 { (dir + 3) % HEX_DIRECTION_COUNT as u8 };

        // Every unordered pair of hexes that some chain genuinely runs between. Anything the mask
        // links that is NOT in here is a fabricated cross-link — the web.
        let mut chain_pairs: std::collections::HashSet<(UVec2, UVec2)> =
            std::collections::HashSet::new();
        for river in &hydrology.rivers {
            for pair in river.navigable_hexes.windows(2) {
                chain_pairs.insert((pair[0], pair[1]));
                chain_pairs.insert((pair[1], pair[0]));
            }
        }

        for river in &hydrology.rivers {
            if river.navigable_hexes.is_empty() {
                continue;
            }
            checked_chains += 1;

            // (1) + (2): every consecutive pair is armed on BOTH hexes, facing each other.
            for pair in river.navigable_hexes.windows(2) {
                let (from, to) = (pair[0], pair[1]);
                let dir = (0..HEX_DIRECTION_COUNT as u8)
                    .find(|&d| step(from, d) == Some(to))
                    .expect("a contiguous chain steps between adjacent hexes");
                assert!(
                    tile_at(from).channel_exits(dir),
                    "seed {seed}: {from:?} has no channel exit toward its downstream \
                     neighbour {to:?}"
                );
                assert!(
                    tile_at(to).channel_exits(opposite(dir)),
                    "seed {seed}: {to:?} does not agree with {from:?} about the channel \
                     between them (asymmetric exit)"
                );
            }

            // (3): the chain reaches the water — unless it merged into another chain, which then
            // carries the water on (its final hex is a confluence, not a mouth).
            let last = *river.navigable_hexes.last().expect("non-empty chain");
            let upstream = river.navigable_hexes.iter().rev().nth(1).and_then(|&prev| {
                (0..HEX_DIRECTION_COUNT as u8).find(|&d| step(last, d) == Some(prev))
            });
            let merged_into_a_trunk = (0..HEX_DIRECTION_COUNT as u8).any(|dir| {
                Some(dir) != upstream
                    && tile_at(last).channel_exits(dir)
                    && step(last, dir)
                        .map(|n| tile_at(n).terrain == TerrainType::NavigableRiver)
                        .unwrap_or(false)
            });
            if !merged_into_a_trunk {
                let reaches_water = (0..HEX_DIRECTION_COUNT as u8).any(|dir| {
                    Some(dir) != upstream
                        && tile_at(last).channel_exits(dir)
                        && step(last, dir)
                            .map(|n| {
                                let t = tile_at(n).terrain;
                                is_water(t) || t == TerrainType::RiverDelta
                            })
                            .unwrap_or(false)
                });
                assert!(
                    reaches_water,
                    "seed {seed}: river {} ends at {last:?} with no channel exit toward the \
                     water it drains into — the drawn river stops one hex short of the sea",
                    river.id
                );
                mouths += 1;
            }
        }

        // (4) THE ANTI-WEB INVARIANT: a navigable hex never exits toward a navigable hex that no
        // chain actually runs to. This is what stopped the trunk rendering as a mesh of triangles.
        for y in 0..height {
            for x in 0..width {
                let pos = UVec2::new(x, y);
                let tile = tile_at(pos);
                if tile.terrain != TerrainType::NavigableRiver {
                    continue;
                }
                for dir in 0..HEX_DIRECTION_COUNT as u8 {
                    if !tile.channel_exits(dir) {
                        continue;
                    }
                    let Some(neighbor) = step(pos, dir) else {
                        continue;
                    };
                    if tile_at(neighbor).terrain != TerrainType::NavigableRiver {
                        continue; // an exit into the sea/delta at the mouth — legitimate.
                    }
                    assert!(
                        chain_pairs.contains(&(pos, neighbor)),
                        "seed {seed}: {pos:?} claims a channel to the navigable hex \
                         {neighbor:?}, but no river chain runs between them — this is the \
                         cross-link that renders the trunk as a web"
                    );
                }
            }
        }
    }

    assert!(
        checked_chains > 0,
        "expected the sweep to produce navigable chains to check"
    );
    println!(
        "channel-exit census: {checked_chains} navigable chains, {mouths} of them reaching open water"
    );
}

// ---------------------------------------------------------------------------
// Threshold sweep — the tuning instrument. Asserts nothing; run with:
//   cargo test -p core_sim --test hydrology_earthlike drainage_threshold_sweep \
//     -- --ignored --nocapture
//
// Discharge is precipitation-weighted upstream drainage area in HEX-EQUIVALENTS, so every threshold
// below is an absolute, map-size-independent value.
// ---------------------------------------------------------------------------

const SWEEP_CHANNEL_MIN: [f32; 3] = [2.0, 3.0, 5.0];
const SWEEP_MAJOR_MIN: [f32; 3] = [8.0, 12.0, 15.0];
const SWEEP_NAVIGABLE_MIN: [f32; 5] = [20.0, 25.0, 30.0, 35.0, 45.0];

/// One (channel, major, navigable) cell, aggregated over `CENSUS_SEEDS`.
#[derive(Default)]
struct SweepCell {
    rivers: usize,
    edges: usize,
    minor: usize,
    major: usize,
    navigable_segments: usize,
    navigable_hexes: usize,
    runs: Vec<usize>,
    seeds_with_navigable: usize,
    seeds_without_navigable: Vec<u64>,
}

#[test]
#[ignore = "measurement harness, not an assertion"]
fn drainage_threshold_sweep() {
    println!(
        "\n=== THRESHOLD SWEEP (earthlike 80x52, river_density=1.0, {} seeds) ===",
        CENSUS_SEEDS.len()
    );
    println!(
        "{:<6} {:<6} {:<6} | {:<6} {:<6} | {:<7} {:<7} | {:<5} {:<6} {:<5} {:<5} {:<5} | runs",
        "chan",
        "major",
        "nav",
        "rivers",
        "edges",
        "minor%",
        "major%",
        "nseg",
        "nhexes",
        "mean",
        "max",
        "seeds"
    );

    for &channel in &SWEEP_CHANNEL_MIN {
        for &major in &SWEEP_MAJOR_MIN {
            for &navigable in &SWEEP_NAVIGABLE_MIN {
                let mut cell = SweepCell::default();
                for seed in CENSUS_SEEDS {
                    let world = earthlike_world_with(
                        seed,
                        Some(HydrologyOverrides {
                            river_density: Some(1.0),
                            channel_min_discharge: Some(channel),
                            class_major_min_discharge: Some(major),
                            class_navigable_min_discharge: Some(navigable),
                            ..Default::default()
                        }),
                    );
                    let hydrology = world.resource::<HydrologyState>();
                    let mut navigable_here = 0usize;
                    for river in &hydrology.rivers {
                        cell.rivers += 1;
                        for edge in &river.edges {
                            cell.edges += 1;
                            match edge.class {
                                RiverClass::Minor => cell.minor += 1,
                                RiverClass::Major => cell.major += 1,
                                RiverClass::None => {}
                            }
                        }
                        if !river.navigable_hexes.is_empty() {
                            navigable_here += 1;
                            cell.navigable_segments += 1;
                            cell.navigable_hexes += river.navigable_hexes.len();
                            cell.runs.push(river.navigable_hexes.len());
                        }
                    }
                    if navigable_here > 0 {
                        cell.seeds_with_navigable += 1;
                    } else {
                        cell.seeds_without_navigable.push(seed);
                    }
                }

                cell.runs.sort_unstable_by(|a, b| b.cmp(a));
                let seeds = CENSUS_SEEDS.len();
                let pct = |n: usize| -> f64 {
                    if cell.edges == 0 {
                        0.0
                    } else {
                        100.0 * n as f64 / cell.edges as f64
                    }
                };
                let mean_run = if cell.runs.is_empty() {
                    0.0
                } else {
                    cell.navigable_hexes as f64 / cell.runs.len() as f64
                };
                println!(
                    "{:<6.1} {:<6.1} {:<6.1} | {:<6.1} {:<6.1} | {:<7.1} {:<7.1} | {:<5.1} {:<6.1} \
                     {:<5.1} {:<5} {:<5} | {:?}",
                    channel,
                    major,
                    navigable,
                    cell.rivers as f64 / seeds as f64,
                    cell.edges as f64 / seeds as f64,
                    pct(cell.minor),
                    pct(cell.major),
                    cell.navigable_segments as f64 / seeds as f64,
                    cell.navigable_hexes as f64 / seeds as f64,
                    mean_run,
                    cell.runs.first().copied().unwrap_or(0),
                    format!("{}/{}", cell.seeds_with_navigable, seeds),
                    cell.runs
                );
            }
        }
    }
    println!(
        "\n(rivers/edges/nseg/nhexes are PER-MAP means over {} seeds; max + runs are over the whole \
         sweep of seeds)",
        CENSUS_SEEDS.len()
    );
}
