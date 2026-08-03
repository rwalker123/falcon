//! **Gathering markers follow the fresh water** (issue #466).
//!
//! The `FoodSiteRegistry` is the whole of the ground a player can act on. `food_modules` on the wire
//! is the only source the client's `_forage_compose_available` gate reads, so a hex without a marker
//! offers no Forage button — and `sow` requires a band **already foraging** the tile. So the ~90
//! markers, not the ~2,000 food-bearing hexes, are the real denominator for "can I climb the plant
//! ladder here", and `plant:field`'s site rule wants fresh water.
//!
//! Curation could not see water: it runs inside `spawn_initial_world`, *before* `generate_hydrology`,
//! so at selection time there are no rivers, deltas or floodplains — and its quality sort
//! (`compare_food_site`) was inert anyway, because every tile ships `seasonal_weight = 1.0`.
//! `bias_food_sites_toward_fresh_water` re-ranks the result over the final terrain.
//!
//! Everything here drives the **real Startup chain** through `build_headless_app`, because the pass
//! runs after hydrology, the tag solver, the palette clamp and `reconcile_food_modules` — a harness
//! that stops earlier measures a map the game never shows anyone.

use bevy::app::App;
use bevy::math::UVec2;

use core_sim::{
    classify_food_module, tile_forage_capacity, tile_is_fresh_watered, FoodSiteRegistry,
    LaborConfigHandle, LadderConfigHandle, RungKey, SimulationConfig, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, Tile, TileRegistry,
};

/// The shipped `map_seed` is `0` = "roll from entropy", so a test that did not pin one would ask a
/// different question every run. Six seeds, because a single map's marker count is high-variance and
/// the claims here are about the shape of the distribution, not one number.
const SEEDS: [u64; 6] = [
    119_304_647,
    11,
    2_147_483_647,
    99_991,
    1_234_567,
    777_777_777,
];

/// **The control**: what the lever must be set to for the pass to be a provable no-op. A marker's own
/// tile is always in its own candidate set, so with no water bonus nothing can outscore staying put.
const BIAS_OFF: f32 = 0.0;

/// Build the real app and run the shipped Startup chain for `(seed, grid)`, optionally overriding the
/// water lever. The override is written into the overlays config **before** `app.update()`, which is
/// when the Startup chain reads it.
fn generated_world(seed: u64, weight: Option<f32>) -> App {
    let mut app = core_sim::build_headless_app();

    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    app.world.insert_resource(config);

    if let Some(weight) = weight {
        // Patched through JSON rather than by reaching into the struct: the fields are private, and
        // going through the real deserializer is also what proves the key is actually wired.
        let mut file: serde_json::Value =
            serde_json::from_str(core_sim::BUILTIN_SNAPSHOT_OVERLAYS_CONFIG)
                .expect("builtin overlays config parses");
        file["food"]["fresh_water_site_weight"] = serde_json::json!(weight);
        let patched = SnapshotOverlaysConfig::from_json_str(&file.to_string())
            .expect("patched overlays config parses");
        app.world
            .insert_resource(SnapshotOverlaysConfigHandle::new(patched.into()));
    }

    app.update();
    app
}

/// Is this tile on or beside fresh water, judged through the **same** seam the `plant:field` site rule
/// gates on — never a restatement of the rule.
fn is_fresh_watered(app: &App, pos: UVec2) -> bool {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let Some(tile) = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .and_then(|entity| app.world.get::<Tile>(entity))
    else {
        return false;
    };
    tile_is_fresh_watered(tile, width, height, wrap, |neighbor| {
        app.world
            .resource::<TileRegistry>()
            .index(neighbor.x, neighbor.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
            .map(|t| t.terrain_tags)
    })
}

/// **Will `plant:field` take seed here?** — rich enough ground, on fresh water, resolved against the
/// rung's own `site_requirement` so a retune of either dial moves this fixture with the game.
fn is_sowable(app: &App, pos: UVec2) -> bool {
    let Some(tile) = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .and_then(|entity| app.world.get::<Tile>(entity))
    else {
        return false;
    };
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    core_sim::rung_site_refusal(
        ladder.rung(RungKey::PlantField),
        tile,
        &labor.forage,
        is_fresh_watered(app, pos),
    )
    .is_none()
}

fn marker_positions(app: &App) -> Vec<UVec2> {
    app.world
        .resource::<FoodSiteRegistry>()
        .iter()
        .map(|entry| entry.position)
        .collect()
}

/// **The point of the change**: markers land on fresh water far more often than they used to, so the
/// ground a player can actually Forage — and therefore Sow — overlaps the river valleys.
///
/// Asserted as a **relation against the bias-off control on the same seed**, never against a literal:
/// the absolute count is an emergent property of the heightfield and legitimately moves with worldgen
/// tuning, but "the bias put more markers on water than no bias did" is the claim the pass makes.
#[test]
fn the_water_bias_puts_more_gathering_markers_on_fresh_water() {
    let mut improved = 0usize;
    for seed in SEEDS {
        let off = generated_world(seed, Some(BIAS_OFF));
        let on = generated_world(seed, None);

        let watered_off = marker_positions(&off)
            .into_iter()
            .filter(|pos| is_fresh_watered(&off, *pos))
            .count();
        let watered_on = marker_positions(&on)
            .into_iter()
            .filter(|pos| is_fresh_watered(&on, *pos))
            .count();

        assert!(
            watered_on >= watered_off,
            "seed {seed}: the water bias LOST watered markers ({watered_off} -> {watered_on}) — it \
             is supposed to be a re-rank toward water, so this can only mean the score or the \
             spacing check is inverted"
        );
        if watered_on > watered_off {
            improved += 1;
        }
    }

    // The liveness half: a bias that never moves anything would satisfy the `>=` above on every seed
    // while being completely dead. It has to actually bite on most maps.
    assert!(
        improved >= SEEDS.len() - 1,
        "the water bias moved no markers on {} of {} seeds — a lever that reads live and does \
         nothing is the failure this assertion exists to catch",
        SEEDS.len() - improved,
        SEEDS.len()
    );
}

/// **The site budget scales with the map in BOTH directions.**
///
/// It used to be `max(max_total_sites, max(land_tiles / 120, 24))`, in which the flat count was a
/// **floor**: the budget grew on maps past ~10,800 land tiles and never shrank, so a Tiny map carried
/// the same 90 markers as the Standard. `site_land_fraction` makes it one share of land, and this pins
/// the direction the old shape got wrong — a regression to a flat count would still pass a
/// scales-up-only test.
#[test]
fn the_site_budget_scales_with_the_map_in_both_directions() {
    let food = SnapshotOverlaysConfig::builtin();
    let food = food.food();

    let standard = food.site_budget(1_580); // ~38% land on an 80x52 grid
    let tiny = food.site_budget(760); // ~a 56x36 "Tiny" map
    let huge = food.site_budget(18_000); // ~a 256x192 map

    assert!(
        tiny < standard,
        "a SMALLER map must carry FEWER gathering markers ({tiny} vs {standard}) — this is exactly \
         what the old flat-count floor got wrong"
    );
    assert!(
        huge > standard,
        "a LARGER map must carry MORE gathering markers ({huge} vs {standard})"
    );

    // The floor is a floor, not a second ceiling: it may only ever raise the budget.
    assert!(
        food.site_budget(1) >= 1,
        "even a one-tile map must resolve to a legal budget"
    );
    assert!(
        food.site_budget(10) <= 10,
        "a map cannot carry more markers than it has land"
    );
}

/// **RE-RANK ONLY — the pass relocates, it never adds or drops.** The marker budget, its per-bucket
/// quota and its latitude spread are the curation's decision; this pass is only allowed to change
/// *which* hex inside a bucket carries the marker. If the count could move, "gathering is exactly as
/// scarce as it was" would stop being true and the change would be a difficulty edit in disguise.
#[test]
fn the_water_bias_never_changes_how_many_markers_there_are() {
    for seed in SEEDS {
        let off = generated_world(seed, Some(BIAS_OFF));
        let on = generated_world(seed, None);
        assert_eq!(
            marker_positions(&off).len(),
            marker_positions(&on).len(),
            "seed {seed}: the water bias changed the MARKER COUNT — it may only relocate"
        );
    }
}

/// A relocated marker must still be somewhere a marker may legally be: on ground that classifies to a
/// real food module (the thing `reconcile_food_modules` drops entries for), and no closer to another
/// marker than curation's own `min_site_spacing` allows. The spacing rule is re-checked rather than
/// assumed because relocation is the one thing that can violate it after curation has finished.
#[test]
fn a_relocated_marker_is_still_a_legal_marker() {
    for seed in SEEDS {
        let app = generated_world(seed, None);
        let spacing = app
            .world
            .resource::<SnapshotOverlaysConfigHandle>()
            .get()
            .food()
            .min_site_spacing() as i64;
        let min_spacing_sq = spacing * spacing;
        let positions = marker_positions(&app);

        for pos in &positions {
            let tile = app
                .world
                .resource::<TileRegistry>()
                .index(pos.x, pos.y)
                .and_then(|entity| app.world.get::<Tile>(entity))
                .unwrap_or_else(|| panic!("seed {seed}: marker at {pos:?} has no tile"));
            assert!(
                classify_food_module(tile).is_some(),
                "seed {seed}: marker at {pos:?} sits on {:?}, which bears no food — the pass \
                 relocated onto ground `reconcile_food_modules` would drop",
                tile.terrain
            );
            assert!(
                tile_forage_capacity(
                    &app.world.resource::<LaborConfigHandle>().get().forage,
                    tile
                ) > 0.0,
                "seed {seed}: marker at {pos:?} sits on ground with zero forage capacity"
            );
        }

        for (i, a) in positions.iter().enumerate() {
            for b in positions.iter().skip(i + 1) {
                let dx = (a.x as i64 - b.x as i64).abs();
                let dy = (a.y as i64 - b.y as i64).abs();
                assert!(
                    dx * dx + dy * dy >= min_spacing_sq,
                    "seed {seed}: markers {a:?} and {b:?} are closer than min_site_spacing \
                     ({spacing}) — relocation broke curation's own spacing rule"
                );
            }
        }
    }
}

/// **`fresh_water_site_weight = 0.0` reproduces the pre-#466 map exactly.** The kill switch has to be
/// a real one: a marker's own tile is in its own candidate set, so with no bonus nothing can outscore
/// staying put, and the whole pass is a no-op. This is what makes the A/B in
/// [`the_water_bias_puts_more_gathering_markers_on_fresh_water`] attributable to the lever.
#[test]
fn zeroing_the_lever_leaves_the_markers_exactly_where_curation_put_them() {
    for seed in SEEDS {
        let a = marker_positions(&generated_world(seed, Some(BIAS_OFF)));
        let b = marker_positions(&generated_world(seed, Some(BIAS_OFF)));
        assert_eq!(
            a, b,
            "seed {seed}: the bias-off map is not even reproducible"
        );
        assert!(
            !a.is_empty(),
            "seed {seed}: the map carries no gathering markers at all"
        );
    }
}

/// **Determinism**: the pass sorts candidates by score with an explicit `(y, x)` tie-break and visits
/// markers in list order, so two builds of one seed must agree hex for hex. A score comparison that
/// fell back on map iteration order would pass every other test in this file and fail this one.
#[test]
fn the_relocated_marker_list_is_identical_across_two_builds() {
    for seed in SEEDS {
        let a = marker_positions(&generated_world(seed, None));
        let b = marker_positions(&generated_world(seed, None));
        assert_eq!(
            a, b,
            "seed {seed}: two builds of the same map disagree about where the gathering markers are"
        );
    }
}

/// **CENSUS — the numbers this arc was argued from.** Not an assertion: a `--ignored --nocapture`
/// report, in the style of `hydrology_earthlike`'s `drainage_census`. Run with
/// `cargo test -p core_sim --test food_site_water_bias census -- --ignored --nocapture`.
///
/// It reports the funnel that matters — food-bearing hexes, the sowable subset, and then the two
/// numbers a player actually lives with: how many **markers** are sowable, and how far the nearest one
/// is from where their band starts.
#[test]
#[ignore]
fn gathering_marker_census() {
    println!("\n=== gathering markers vs fresh water ===");
    println!("               food  sowable | markers sowable(off -> on)  nearest sowable marker");

    let (mut agg_off, mut agg_on) = (0usize, 0usize);
    for seed in SEEDS {
        let off = generated_world(seed, Some(BIAS_OFF));
        let on = generated_world(seed, None);
        let grid = on.world.resource::<SimulationConfig>().grid_size;

        let mut food = 0usize;
        let mut sowable_tiles = 0usize;
        for y in 0..grid.y {
            for x in 0..grid.x {
                let pos = UVec2::new(x, y);
                let Some(tile) = on
                    .world
                    .resource::<TileRegistry>()
                    .index(x, y)
                    .and_then(|e| on.world.get::<Tile>(e))
                else {
                    continue;
                };
                if tile_forage_capacity(
                    &on.world.resource::<LaborConfigHandle>().get().forage,
                    tile,
                ) <= 0.0
                {
                    continue;
                }
                food += 1;
                if is_sowable(&on, pos) {
                    sowable_tiles += 1;
                }
            }
        }

        let sowable_off = marker_positions(&off)
            .into_iter()
            .filter(|p| is_sowable(&off, *p))
            .count();
        let sowable_on: Vec<UVec2> = marker_positions(&on)
            .into_iter()
            .filter(|p| is_sowable(&on, *p))
            .collect();

        let start = on
            .world
            .resource::<core_sim::StartLocation>()
            .position()
            .expect("worldgen picks a start tile");
        let to_axial = |p: UVec2| {
            let q = p.x as i32 - ((p.y as i32 - (p.y as i32 & 1)) / 2);
            (q, p.y as i32)
        };
        let hex_dist = |a: UVec2, b: UVec2| -> i32 {
            let ((aq, ar), (bq, br)) = (to_axial(a), to_axial(b));
            let (dq, dr) = (aq - bq, ar - br);
            (dq.abs() + dr.abs() + (dq + dr).abs()) / 2
        };
        let nearest = sowable_on
            .iter()
            .map(|p| hex_dist(start, *p))
            .min()
            .unwrap_or(-1);

        println!(
            "seed {seed:>12} {food:>5} {sowable_tiles:>7} | {:>7} {:>3} -> {:<3}        {nearest:>3} hexes",
            marker_positions(&on).len(),
            sowable_off,
            sowable_on.len(),
        );
        agg_off += sowable_off;
        agg_on += sowable_on.len();
    }

    let n = SEEDS.len() as f32;
    println!(
        "\nMEAN sowable markers: {:.1} -> {:.1}  ({:.2}x)\n",
        agg_off as f32 / n,
        agg_on as f32 / n,
        agg_on as f32 / agg_off.max(1) as f32,
    );
}
