//! **Do rivers issue out of the alpine ranges?** Measurement for issue #291, which reported that
//! they do not and explicitly asked for verification before the report is treated as a defect.
//! Run manually:
//! `cargo test -p core_sim --release --test alpine_headwaters -- --ignored --nocapture`
//!
//! # Every figure here is against a NULL, because raw counts cannot settle the claim
//!
//! "Not enough rivers come out of the mountains" is not answered by counting mountain-sourced
//! rivers: alpine ground is a small share of the map, so a small count is the *expected* result of
//! a network with no mountain preference at all. Each metric is therefore reported as an
//! **enrichment** — what was observed, over what the same map would give if rivers ignored relief:
//!
//! - **`> 1`** — rivers favour the mountains, which is what a precipitation-weighted descent on
//!   real relief is supposed to produce (high ground is wet ground).
//! - **`≈ 1`** — the ranges are hydrologically invisible: a headwater is no likelier there than on
//!   a random flat.
//! - **`< 1`** — the ranges actively *repel* drainage.
//!
//! # Zero rivers ON alpine tiles is arithmetic, not a defect — do not report it as the finding
//!
//! Mountains are floored above `MountainsConfig::elevation_base` by `restamp_elevation`, so on the
//! filled-surface steepest descent **every alpine corner is a local maximum**: its catchment is
//! only what fits above it inside the ribbon. The shipped `alpine_relief_threshold = 1.85` puts
//! that ribbon at `D = 1`, i.e. **3 tiles wide**, so an alpine corner gathers at most ~1-2 hexes
//! ≈ 0.7-1.4 hex-equivalents against `river_channel_min_discharge = 3.0`. An alpine tile *cannot*
//! carry a channel, on any seed, and measuring 0 there confirms the arithmetic rather than finding
//! a bug.
//!
//! The question that is actually open is what happens **just below** the crest — whether the water
//! coming off a range becomes a river within a few hexes (the ranges drain, the threshold simply
//! sits below the alpine core) or whether ranges sit in a river desert (a real defect). That is
//! what [`Distances`] measures, and why the histogram tail must start outside the whole 9-tile
//! mountain belt rather than at the alpine core.
//!
//! # WHAT THIS MEASURED (issue #291) — the report is false as stated, a different defect is real
//!
//! **Rivers DO issue out of the ranges.** Source placement is strongly enriched just outside them —
//! peaking at **3.4x chance 5 hexes from the alpine core** — and suppressed to 0.05-0.45x within 3
//! hexes. The ranges are the headwater ground; the water is mountain water.
//!
//! **What is real is a channel-free COLLAR**, so a river appears out on the plain, visually detached
//! from the range feeding it. An alpine tile sits **6.20 hexes from the nearest river against 3.31
//! for average land**, and ranges >= 10 tiles run to a **28-hex** worst case.
//!
//! **Root cause, isolated by [`what_dries_the_alpine_belt`]: the mountain belt casts its rain shadow
//! onto ITSELF.** Each mountain tile adds `rain_shadow_strength x relief` to a shadow subtracted
//! from everything downwind — including the rest of a 9-tile belt — and the shipped
//! `rain_shadow_strength = 0.65` at alpine relief >= 1.85 is ~**1.20 per tile** against a 2.0 clamp,
//! decaying at only `rain_shadow_decay = 0.04`, while `windward_moisture_bonus = 0.12` lifts only
//! ~0.22. Shadow outweighs orographic lift ~5:1, so **82.8% of alpine tiles receive literally zero
//! rain** while the belt's windward strip reaches 0.818, the map maximum — bimodal, not merely dry.
//!
//! Turning the rain shadow off **inverts the precipitation profile** (mountains become the wettest
//! ground: 0.360 vs 0.213 for land, bone-dry share 82.8% -> **0.0%**) and largely closes the collar
//! (near-range source enrichment 0.28x/0.45x/1.63x -> **2.11x/2.65x/3.50x** at distance 2/3/4).
//! Turning **interior aridity** off instead wets the whole map but leaves the belt relatively dry
//! and the collar intact — so the shadow is the cause and interior aridity is not.
//!
//! **The zero-on-alpine figure does NOT move** (0.00% -> 0.05% with the shadow off), confirming the
//! section above: that half is ribbon geometry against the channel threshold, not climate. Two
//! separate mechanisms, and only the second is a defect.

use std::sync::Arc;

use bevy::app::App;
use bevy::prelude::{UVec2, World};
use bevy::MinimalPlugins;

use core_sim::{
    generate_hydrology,
    grid_utils::{hex_neighbor, hex_neighbors_wrapped},
    spawn_initial_world, CultureManager, DiscoveryProgressLedger, FactionInventory,
    GenerationRegistry, HydrologyState, LaborConfig, LaborConfigHandle, LadderConfig,
    LadderConfigHandle, MapPresets, MapPresetsHandle, MoistureRaster, SimulationConfig,
    SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
    BUILTIN_MAP_PRESETS,
};
use sim_runtime::TerrainType;

/// The deterministic seed the rest of the worldgen suite is tuned against.
const TEST_SEED: u64 = 119_304_647;

/// The same six-seed sweep `relief_sweep` and `hydrology_earthlike` use, so this census is directly
/// comparable to theirs. **Never 0** — `map_seed == 0` means "roll from entropy".
const SEEDS: [u64; 6] = [1, 2, 3, 4, 5, TEST_SEED];

/// The statistics grid, shared with `relief_sweep::ALPINE_GRID`: big enough that a 3-tile-thick
/// range still has room for a catchment below it.
const BIG_GRID: UVec2 = UVec2::new(384, 288);

/// A range below this many tiles is a lone peak or a stub, not a cordillera. It has essentially no
/// catchment, so "it grew no river" says nothing about the drainage network — the reported claim is
/// about *ranges*, and this is where the per-range figures stop counting noise.
const SUBSTANTIAL_RANGE_TILES: usize = 10;

/// Distances beyond this are pooled into the tail of the distance histograms. Set wide enough to
/// cover the whole mountain belt: `belt_width_tiles = 4` dilates the fold boundary into a 9-tile
/// belt of which only the middle ~3 read `AlpineMountain`, so a river descending off a crest crosses
/// several tiles of foothill skirt before it can gather anything. A tail that starts *inside* that
/// skirt pools away the answer.
const DISTANCE_TAIL: u32 = 10;

/// The shipped hydrology levers, restated so this file can report *why* a catchment does or does
/// not become a channel instead of only reporting that it did not. A hex contributes
/// `RIVER_BASE_RUNOFF + RIVER_MOISTURE_WEIGHT * precip` hex-equivalents, so a fully-wet hex
/// contributes exactly `1.0` and a channel needs `RIVER_CHANNEL_MIN_DISCHARGE` of them upstream.
/// Kept in sync with `core_sim/src/data/map_presets.json`; the census prints the implied catchment
/// so a drift shows up as a nonsense figure rather than a silently wrong conclusion.
const RIVER_BASE_RUNOFF: f64 = 0.2;
const RIVER_MOISTURE_WEIGHT: f64 = 0.8;
const RIVER_CHANNEL_MIN_DISCHARGE: f64 = 3.0;

/// The mask-driven highland biomes — every terrain the mountain mask can raise (`core_sim/CLAUDE.md`
/// -> "Highland biomes are mask-driven"). `AlpineMountain` is the range *core*; the rest are its
/// shoulders and the other relief the same belt produces. Reported alongside the alpine figure
/// because a river that starts on the foothill immediately below a peak still "comes out of the
/// mountains" in the sense the report meant.
const RELIEF_TERRAIN: [TerrainType; 5] = [
    TerrainType::RollingHills,
    TerrainType::HighPlateau,
    TerrainType::AlpineMountain,
    TerrainType::KarstHighland,
    TerrainType::ActiveVolcanoSlope,
];

fn is_water(t: TerrainType) -> bool {
    matches!(
        t,
        TerrainType::DeepOcean
            | TerrainType::ContinentalShelf
            | TerrainType::CoralShelf
            | TerrainType::HydrothermalVentField
            | TerrainType::InlandSea
            | TerrainType::NavigableRiver
    )
}

/// One arm of the rain-shadow A/B. The belt could be dry because it **shadows itself** (each
/// mountain tile adds `rain_shadow_strength × relief` to a shadow subtracted from everything
/// downwind, including the rest of the belt) or merely because it sits **inland**
/// (`interior_aridity_strength`). Those call for opposite fixes, and only turning one term off
/// distinguishes them.
#[derive(Clone, Copy)]
struct Climate {
    label: &'static str,
    /// `None` keeps the shipped value.
    rain_shadow_strength: Option<f64>,
    interior_aridity_strength: Option<f64>,
}

const SHIPPED: Climate = Climate {
    label: "SHIPPED",
    rain_shadow_strength: None,
    interior_aridity_strength: None,
};
const NO_RAIN_SHADOW: Climate = Climate {
    label: "rain shadow OFF",
    rain_shadow_strength: Some(0.0),
    interior_aridity_strength: None,
};
const NO_INTERIOR_ARIDITY: Climate = Climate {
    label: "interior aridity OFF",
    rain_shadow_strength: None,
    interior_aridity_strength: Some(0.0),
};

fn presets_with(climate: Climate) -> Arc<MapPresets> {
    if climate.rain_shadow_strength.is_none() && climate.interior_aridity_strength.is_none() {
        return MapPresets::builtin();
    }
    let mut file: serde_json::Value =
        serde_json::from_str(BUILTIN_MAP_PRESETS).expect("builtin map presets parse");
    for preset in file["presets"].as_array_mut().expect("presets").iter_mut() {
        let block = preset["biomes"].as_object_mut().expect("biomes");
        if let Some(v) = climate.rain_shadow_strength {
            block.insert("rain_shadow_strength".into(), v.into());
        }
        if let Some(v) = climate.interior_aridity_strength {
            block.insert("interior_aridity_strength".into(), v.into());
        }
    }
    Arc::new(MapPresets::from_json_str(&file.to_string()).expect("patched presets parse"))
}

/// The shipped pipeline, seeded and sized. The question is what the player's map actually drains,
/// so the default arm patches nothing.
fn world_with(seed: u64, grid: UVec2, presets: Arc<MapPresets>) -> World {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = grid;
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
    app.world
        .insert_resource(LaborConfigHandle::new(LaborConfig::builtin()));
    app.world
        .insert_resource(LadderConfigHandle::new(LadderConfig::builtin()));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();
    generate_hydrology(&mut app.world);
    app.world
}

/// The grid's terrain, flattened once so the passes below index rather than re-query the ECS.
struct Terrain {
    width: u32,
    height: u32,
    wrap: bool,
    terrain: Vec<TerrainType>,
    has_river_edge: Vec<bool>,
    /// Per-hex precipitation in `[0, 1]` straight off the worldgen `MoistureRaster` — the same
    /// field `CornerField::accumulate` seeds discharge from.
    precip: Vec<f32>,
}

impl Terrain {
    fn of(world: &World) -> Self {
        let config = world.resource::<SimulationConfig>();
        let (width, height, wrap) = (
            config.grid_size.x,
            config.grid_size.y,
            config.map_topology.wrap_horizontal,
        );
        let registry = world.resource::<TileRegistry>();
        let total = (width * height) as usize;
        let mut terrain = Vec::with_capacity(total);
        let mut has_river_edge = Vec::with_capacity(total);
        for i in 0..total {
            let tile = world
                .get::<Tile>(registry.tiles[i])
                .expect("tile entity exists");
            terrain.push(tile.terrain);
            has_river_edge.push(tile.has_any_river_edge());
        }
        let moisture = world.resource::<MoistureRaster>();
        assert_eq!(
            moisture.values.len(),
            total,
            "MoistureRaster must cover the grid — hydrology falls back to uniform precip when it \
             does not, which would make every moisture figure below a fabrication"
        );
        Self {
            width,
            height,
            wrap,
            terrain,
            has_river_edge,
            precip: moisture.values.clone(),
        }
    }

    fn len(&self) -> usize {
        self.terrain.len()
    }

    fn pos(&self, i: usize) -> UVec2 {
        UVec2::new((i as u32) % self.width, (i as u32) / self.width)
    }

    fn idx(&self, pos: UVec2) -> usize {
        (pos.y * self.width + pos.x) as usize
    }

    fn at(&self, pos: UVec2) -> TerrainType {
        self.terrain[self.idx(pos)]
    }

    fn is_alpine(&self, pos: UVec2) -> bool {
        self.at(pos) == TerrainType::AlpineMountain
    }

    fn is_relief(&self, pos: UVec2) -> bool {
        RELIEF_TERRAIN.contains(&self.at(pos))
    }

    fn is_land(&self, pos: UVec2) -> bool {
        !is_water(self.at(pos))
    }

    fn neighbors(&self, pos: UVec2) -> impl Iterator<Item = UVec2> + '_ {
        hex_neighbors_wrapped(pos.x, pos.y, self.width, self.height, self.wrap)
            .map(|(x, y)| UVec2::new(x, y))
    }

    /// The two hexes flanking a canonical river edge — the edge runs *between* them, so a river
    /// that starts here starts on whichever of the two the terrain question is being asked about.
    fn edge_flanks(&self, hex: UVec2, dir: u8) -> [Option<UVec2>; 2] {
        let across = hex_neighbor(
            hex.x,
            hex.y,
            dir as usize,
            self.width,
            self.height,
            self.wrap,
        )
        .map(|(x, y)| UVec2::new(x, y));
        [Some(hex), across]
    }

    /// Hex distance from every tile to the nearest tile satisfying `seed`, by multi-source BFS.
    /// `u32::MAX` where the map contains no such tile.
    fn distance_from(&self, seed: impl Fn(UVec2) -> bool) -> Vec<u32> {
        let mut dist = vec![u32::MAX; self.len()];
        let mut frontier: Vec<UVec2> = Vec::new();
        for (i, slot) in dist.iter_mut().enumerate() {
            let pos = self.pos(i);
            if seed(pos) {
                *slot = 0;
                frontier.push(pos);
            }
        }
        let mut step = 0u32;
        while !frontier.is_empty() {
            step += 1;
            let mut next = Vec::new();
            for pos in frontier.drain(..) {
                for n in self.neighbors(pos) {
                    let ni = self.idx(n);
                    if dist[ni] == u32::MAX {
                        dist[ni] = step;
                        next.push(n);
                    }
                }
            }
            frontier = next;
        }
        dist
    }
}

/// A tail-pooled distance histogram plus the sample count, so two distributions measured over
/// different-sized populations can be compared band by band.
#[derive(Clone, Default)]
struct Distances {
    buckets: Vec<usize>,
    total: usize,
}

impl Distances {
    fn new() -> Self {
        Self {
            buckets: vec![0; DISTANCE_TAIL as usize + 1],
            total: 0,
        }
    }

    fn push(&mut self, d: u32) {
        if d == u32::MAX {
            return;
        }
        self.buckets[d.min(DISTANCE_TAIL) as usize] += 1;
        self.total += 1;
    }

    fn merge(&mut self, other: &Self) {
        for (slot, n) in self.buckets.iter_mut().zip(&other.buckets) {
            *slot += n;
        }
        self.total += other.total;
    }

    fn share(&self, band: usize) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.buckets[band] as f64 / self.total as f64
        }
    }

    fn mean(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        let sum: usize = self.buckets.iter().enumerate().map(|(d, n)| d * n).sum();
        sum as f64 / self.total as f64
    }

    fn render(&self) -> String {
        self.buckets
            .iter()
            .enumerate()
            .map(|(d, n)| {
                let key = if d as u32 == DISTANCE_TAIL {
                    format!("{d}+")
                } else {
                    d.to_string()
                };
                format!("{key}={n}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// One alpine range — a connected component of `AlpineMountain` tiles — and whether the drainage
/// network found it.
struct Range {
    size: usize,
    /// Hex distance from the range to the nearest tile carrying a river edge. **The decisive
    /// number**: a drained range has a river a few hexes below it; a range in a river desert does
    /// not.
    to_river: u32,
}

struct AlpineCensus {
    land: usize,
    alpine: usize,
    relief: usize,
    rivers: usize,
    alpine_sourced: usize,
    relief_sourced: usize,
    /// Distance from each river source to the nearest alpine tile (observed).
    source_to_alpine: Distances,
    /// Distance from each *land tile* to the nearest alpine tile — the **null** the line above is
    /// read against. Without it, "most sources are far from a range" is unreadable: most land is
    /// far from a range too.
    land_to_alpine: Distances,
    /// Distance from each alpine tile to the nearest river-edge tile (observed) ...
    alpine_to_river: Distances,
    /// ... and the same over all land, its null.
    land_to_river: Distances,
    alpine_with_river: usize,
    other_land_with_river: usize,
    /// Every alpine tile's precipitation, and every land tile's — kept as distributions rather than
    /// means because the mean cannot tell "the belt is uniformly dry" from "the belt shadows itself"
    /// (`mapgen.rs` adds `rain_shadow_strength × relief` per mountain tile to a shadow that clamps
    /// at 2.0 and is subtracted from tiles *downwind*, so a 9-tile belt should read as a wet
    /// windward strip and a dead lee — bimodal, not tight).
    alpine_precip: Vec<f32>,
    land_precip: Vec<f32>,
    /// Summed precipitation by hex distance from the nearest alpine tile, with the tile counts to
    /// divide it by — the profile across the belt, which is what a self-shadowing belt shows as a
    /// dip at the core recovering outward.
    precip_by_distance: Vec<(f64, usize)>,
    ranges: Vec<Range>,
}

impl AlpineCensus {
    fn of(world: &World) -> Self {
        let terrain = Terrain::of(world);
        let hydrology = world.resource::<HydrologyState>();
        let to_alpine = terrain.distance_from(|p| terrain.is_alpine(p));
        let to_river = terrain.distance_from(|p| terrain.has_river_edge[terrain.idx(p)]);

        let mut land = 0usize;
        let mut alpine = 0usize;
        let mut relief = 0usize;
        let mut alpine_with_river = 0usize;
        let mut other_land_with_river = 0usize;
        let mut alpine_precip: Vec<f32> = Vec::new();
        let mut land_precip: Vec<f32> = Vec::new();
        let mut precip_by_distance = vec![(0.0f64, 0usize); DISTANCE_TAIL as usize + 1];
        let mut land_to_alpine = Distances::new();
        let mut land_to_river = Distances::new();
        let mut alpine_to_river = Distances::new();
        for i in 0..terrain.len() {
            let pos = terrain.pos(i);
            if !terrain.is_land(pos) {
                continue;
            }
            land += 1;
            land_to_alpine.push(to_alpine[i]);
            land_to_river.push(to_river[i]);
            land_precip.push(terrain.precip[i]);
            if to_alpine[i] != u32::MAX {
                let band = &mut precip_by_distance[to_alpine[i].min(DISTANCE_TAIL) as usize];
                band.0 += f64::from(terrain.precip[i]);
                band.1 += 1;
            }
            let wet = terrain.has_river_edge[i];
            if terrain.is_alpine(pos) {
                alpine += 1;
                alpine_precip.push(terrain.precip[i]);
                alpine_to_river.push(to_river[i]);
                if wet {
                    alpine_with_river += 1;
                }
            } else if wet {
                other_land_with_river += 1;
            }
            if terrain.is_relief(pos) {
                relief += 1;
            }
        }

        // A river's SOURCE is the first edge of its stem: `RiverSegment::edges` runs upstream ->
        // downstream, and main-stem decomposition means every stem starts either at a true
        // headwater or at the top of a tributary — both are "where a river begins".
        let mut rivers = 0usize;
        let mut alpine_sourced = 0usize;
        let mut relief_sourced = 0usize;
        let mut source_to_alpine = Distances::new();
        for river in &hydrology.rivers {
            let Some(first) = river.edges.first() else {
                continue;
            };
            rivers += 1;
            let flanks: Vec<UVec2> = terrain
                .edge_flanks(first.hex, first.dir)
                .into_iter()
                .flatten()
                .collect();
            if flanks.iter().any(|&p| terrain.is_alpine(p)) {
                alpine_sourced += 1;
            }
            if flanks.iter().any(|&p| terrain.is_relief(p)) {
                relief_sourced += 1;
            }
            if let Some(nearest) = flanks.iter().map(|&p| to_alpine[terrain.idx(p)]).min() {
                source_to_alpine.push(nearest);
            }
        }

        // Alpine connected components, tagged with how far the nearest river runs.
        let mut seen = vec![false; terrain.len()];
        let mut ranges = Vec::new();
        for start in 0..terrain.len() {
            let start_pos = terrain.pos(start);
            if seen[start] || !terrain.is_alpine(start_pos) {
                continue;
            }
            let mut size = 0usize;
            let mut nearest = u32::MAX;
            let mut stack = vec![start_pos];
            seen[start] = true;
            while let Some(pos) = stack.pop() {
                size += 1;
                nearest = nearest.min(to_river[terrain.idx(pos)]);
                for n in terrain.neighbors(pos) {
                    let ni = terrain.idx(n);
                    if !seen[ni] && terrain.is_alpine(n) {
                        seen[ni] = true;
                        stack.push(n);
                    }
                }
            }
            ranges.push(Range {
                size,
                to_river: nearest,
            });
        }

        Self {
            land,
            alpine,
            relief,
            rivers,
            alpine_sourced,
            relief_sourced,
            source_to_alpine,
            land_to_alpine,
            alpine_to_river,
            land_to_river,
            alpine_with_river,
            other_land_with_river,
            alpine_precip,
            land_precip,
            precip_by_distance,
            ranges,
        }
    }

    /// Share of river sources that are alpine, over share of land that is alpine. See the module
    /// docs: this, not the raw count, is what the report is really a claim about.
    fn alpine_enrichment(&self) -> f64 {
        enrichment(self.alpine_sourced, self.rivers, self.alpine, self.land)
    }

    fn relief_enrichment(&self) -> f64 {
        enrichment(self.relief_sourced, self.rivers, self.relief, self.land)
    }
}

/// Precipitation below this reads as **no rain at all** — the humidity pass clamps to `[0, 1]`, so a
/// tile the accumulated rain shadow drove negative lands on exactly `0.0`, and the share of tiles
/// sitting there is the signature that separates a self-shadowed belt from a merely dry one.
const PRECIP_BONE_DRY: f32 = 0.01;

/// A precipitation distribution, because the mean hides the shape that matters here.
struct PrecipStats {
    mean: f64,
    median: f32,
    p90: f32,
    max: f32,
    bone_dry_share: f64,
}

impl PrecipStats {
    fn of(values: &[f32]) -> Self {
        if values.is_empty() {
            return Self {
                mean: 0.0,
                median: 0.0,
                p90: 0.0,
                max: 0.0,
                bone_dry_share: 0.0,
            };
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q) as usize];
        Self {
            mean: values.iter().map(|v| f64::from(*v)).sum::<f64>() / values.len() as f64,
            median: at(0.5),
            p90: at(0.9),
            max: *sorted.last().expect("non-empty"),
            bone_dry_share: values.iter().filter(|v| **v < PRECIP_BONE_DRY).count() as f64
                / values.len() as f64,
        }
    }

    fn render(&self) -> String {
        format!(
            "mean {:.3}  median {:.3}  p90 {:.3}  max {:.3}  bone-dry {:.1}%",
            self.mean,
            self.median,
            self.p90,
            self.max,
            100.0 * self.bone_dry_share,
        )
    }
}

fn enrichment(hits: usize, trials: usize, population: usize, universe: usize) -> f64 {
    let observed = hits as f64 / trials.max(1) as f64;
    let expected = population as f64 / universe.max(1) as f64;
    if expected == 0.0 {
        0.0
    } else {
        observed / expected
    }
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 {
        0.0
    } else {
        100.0 * n as f64 / d as f64
    }
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

fn measure(grid: UVec2, tag: &str, climate: Climate) {
    let presets = presets_with(climate);
    println!("\n=== {tag} ({}x{}) ===", grid.x, grid.y);

    let mut alpine_enrich = Vec::new();
    let mut relief_enrich = Vec::new();
    let mut agg_land = 0usize;
    let mut agg_alpine = 0usize;
    let mut agg_rivers = 0usize;
    let mut agg_alpine_sourced = 0usize;
    let mut agg_relief_sourced = 0usize;
    let mut agg_alpine_wet = 0usize;
    let mut agg_other_wet = 0usize;
    let mut agg_other_land = 0usize;
    let mut agg_source_to_alpine = Distances::new();
    let mut agg_land_to_alpine = Distances::new();
    let mut agg_alpine_to_river = Distances::new();
    let mut agg_land_to_river = Distances::new();
    let mut agg_ranges = 0usize;
    let mut big_range_to_river: Vec<u32> = Vec::new();
    let mut agg_alpine_precip: Vec<f32> = Vec::new();
    let mut agg_land_precip: Vec<f32> = Vec::new();
    let mut agg_precip_by_distance = vec![(0.0f64, 0usize); DISTANCE_TAIL as usize + 1];

    for seed in SEEDS {
        let world = world_with(seed, grid, presets.clone());
        let c = AlpineCensus::of(&world);
        println!(
            "seed {seed:>10}: land {:>6}  alpine {:>5} ({:>4.1}%)  rivers {:>4}  \
             alpine-sourced {:>4}  mean range->river {:>4.1} hexes",
            c.land,
            c.alpine,
            pct(c.alpine, c.land),
            c.rivers,
            c.alpine_sourced,
            mean(
                &c.ranges
                    .iter()
                    .filter(|r| r.size >= SUBSTANTIAL_RANGE_TILES && r.to_river != u32::MAX)
                    .map(|r| f64::from(r.to_river))
                    .collect::<Vec<_>>()
            ),
        );

        alpine_enrich.push(c.alpine_enrichment());
        relief_enrich.push(c.relief_enrichment());
        agg_land += c.land;
        agg_alpine += c.alpine;
        agg_rivers += c.rivers;
        agg_alpine_sourced += c.alpine_sourced;
        agg_relief_sourced += c.relief_sourced;
        agg_alpine_wet += c.alpine_with_river;
        agg_other_wet += c.other_land_with_river;
        agg_other_land += c.land - c.alpine;
        agg_source_to_alpine.merge(&c.source_to_alpine);
        agg_land_to_alpine.merge(&c.land_to_alpine);
        agg_alpine_to_river.merge(&c.alpine_to_river);
        agg_land_to_river.merge(&c.land_to_river);
        agg_ranges += c.ranges.len();
        agg_alpine_precip.extend(&c.alpine_precip);
        agg_land_precip.extend(&c.land_precip);
        for (slot, band) in agg_precip_by_distance.iter_mut().zip(&c.precip_by_distance) {
            slot.0 += band.0;
            slot.1 += band.1;
        }
        for r in c
            .ranges
            .iter()
            .filter(|r| r.size >= SUBSTANTIAL_RANGE_TILES)
        {
            big_range_to_river.push(r.to_river);
        }
    }

    println!("--- aggregate over {} seeds ---", SEEDS.len());
    println!(
        "  alpine share of land        {:>6.2}%   ({agg_alpine} / {agg_land})",
        pct(agg_alpine, agg_land)
    );
    println!(
        "  alpine share of sources     {:>6.2}%   ({agg_alpine_sourced} / {agg_rivers})",
        pct(agg_alpine_sourced, agg_rivers)
    );
    println!(
        "  ALPINE ENRICHMENT           {:>6.2}x   (per-seed mean; 1.0 = no mountain preference)",
        mean(&alpine_enrich)
    );
    println!(
        "  relief share of sources     {:>6.2}%   ({agg_relief_sourced} / {agg_rivers})",
        pct(agg_relief_sourced, agg_rivers)
    );
    println!(
        "  RELIEF ENRICHMENT           {:>6.2}x",
        mean(&relief_enrich)
    );
    println!(
        "  river edges on alpine land  {:>6.2}%   vs {:>5.2}% on other land",
        pct(agg_alpine_wet, agg_alpine),
        pct(agg_other_wet, agg_other_land)
    );

    // Is the belt DRY, or is the threshold simply coarser than the terrain it is drawn on? A wet
    // belt whose channels still start 5 hexes out means the levers disagree about scale; a dry belt
    // means the climate pass is starving its own mountains, which is a generating input and the
    // only one of the two that should be fixed by changing worldgen.
    let alpine_stats = PrecipStats::of(&agg_alpine_precip);
    let land_stats = PrecipStats::of(&agg_land_precip);
    let hexes_needed =
        |p: f64| RIVER_CHANNEL_MIN_DISCHARGE / (RIVER_BASE_RUNOFF + RIVER_MOISTURE_WEIGHT * p);
    println!("  -- precipitation: is the belt DRY, or does it SHADOW ITSELF? --");
    println!("     alpine   {}", alpine_stats.render());
    println!("     all land {}", land_stats.render());
    println!(
        "     hexes of catchment for a channel: {:.1} on the belt vs {:.1} on ordinary land",
        hexes_needed(alpine_stats.mean),
        hexes_needed(land_stats.mean),
    );
    let profile: Vec<String> = agg_precip_by_distance
        .iter()
        .enumerate()
        .map(|(d, (sum, n))| {
            let key = if d as u32 == DISTANCE_TAIL {
                format!("{d}+")
            } else {
                d.to_string()
            };
            let m = if *n == 0 { 0.0 } else { sum / *n as f64 };
            format!("{key}={m:.3}")
        })
        .collect();
    println!(
        "     mean precip by distance from the core: {}",
        profile.join(" ")
    );

    println!("  -- how far a SOURCE is from a range, against the null of all land --");
    println!("     sources  {}", agg_source_to_alpine.render());
    println!("     all land {}", agg_land_to_alpine.render());
    let bands: Vec<String> = (0..=DISTANCE_TAIL as usize)
        .map(|b| {
            let obs = agg_source_to_alpine.share(b);
            let exp = agg_land_to_alpine.share(b);
            let e = if exp == 0.0 { 0.0 } else { obs / exp };
            format!("{b}={e:.2}x")
        })
        .collect();
    println!("     enrich   {}", bands.join(" "));
    println!(
        "     mean: sources {:.2} hexes from a range, all land {:.2}",
        agg_source_to_alpine.mean(),
        agg_land_to_alpine.mean()
    );

    println!("  -- how far a RANGE is from a river, against the null of all land --");
    println!("     alpine   {}", agg_alpine_to_river.render());
    println!("     all land {}", agg_land_to_river.render());
    println!(
        "     mean: alpine tile {:.2} hexes from a river, any land tile {:.2}",
        agg_alpine_to_river.mean(),
        agg_land_to_river.mean()
    );

    let reachable: Vec<f64> = big_range_to_river
        .iter()
        .filter(|d| **d != u32::MAX)
        .map(|d| f64::from(*d))
        .collect();
    let mut sorted = reachable.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    println!(
        "  ranges >= {SUBSTANTIAL_RANGE_TILES} tiles          {} of {agg_ranges}; \
         distance to nearest river: mean {:.1}, median {:.1}, max {:.0}",
        big_range_to_river.len(),
        mean(&reachable),
        sorted.get(sorted.len() / 2).copied().unwrap_or(0.0),
        sorted.last().copied().unwrap_or(0.0),
    );
}

#[test]
#[ignore]
fn alpine_headwater_census() {
    measure(SimulationConfig::builtin().grid_size, "shipped", SHIPPED);
    measure(BIG_GRID, "statistics grid", SHIPPED);
}

/// **Which term dries the belt?** The census above measures that alpine ground is bone-dry; it
/// cannot say why. These two arms turn off one climate term each, so the mechanism is isolated by
/// measurement rather than inferred from reading the humidity loop.
#[test]
#[ignore]
fn what_dries_the_alpine_belt() {
    let grid = BIG_GRID;
    for climate in [SHIPPED, NO_RAIN_SHADOW, NO_INTERIOR_ARIDITY] {
        measure(grid, climate.label, climate);
    }
}
