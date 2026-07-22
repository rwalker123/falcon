//! **Grey Seals on cold coasts** — the habitat pinhole fix, and the shore predicate it rides on.
//!
//! Seals used to host **only** `coastal_littoral`, which on a generated map is a handful of
//! `RiverDelta` tiles at abundance 0.10 shared with Wild Fowl — an expected count of ~0–1 colonies
//! per map. They now also host `boreal_arctic` (abundance 0.12, plentiful), gated by the per-species
//! site rule `adjacent_water: "salt"`, so they seat on arctic/boreal **shorelines** — and only on
//! SALT ones — never on inland tundra and never beside a landlocked freshwater lake.
//!
//! The cold half comes for free from `host_biomes` — there is deliberately **no** second climate
//! gate here (`climate_band_for_temperature` is the single climate authority). The only new concept
//! is water adjacency, and it **reads** the coastline geometry rather than editing terrain.

use std::collections::BTreeMap;

use bevy::math::UVec2;

use core_sim::{
    build_headless_app, classify_food_module, FaunaConfig, FoodModule, HerdRegistry,
    ShoreRequirement, SimulationConfig, Tile, TileRegistry,
};
use sim_runtime::TerrainTags;

/// Seeds the sweep runs, at the shipped standard map size. Never 0 (the "roll from entropy"
/// sentinel).
const SWEEP_SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 119_304_647];

/// The shipped standard map dimensions.
const GRID: UVec2 = UVec2::new(80, 52);

/// The seal's snapshot species string (the roster's `display_name`).
const SEAL_SPECIES: &str = "Grey Seals";

/// Measured floor for seals spawned across [`SWEEP_SEEDS`]. The sweep measures **14** post-change
/// (0–4 per map) against **2** pre-change (0–1 per map, the delta pinhole); the bound sits well
/// under the measured value so an ordinary retune doesn't trip it, while the old regime still fails
/// it loudly. Re-measure with `seal_habitat_report` before moving it.
const MIN_SEALS_OVER_SWEEP: usize = 8;

/// The tiles a seal colony occupies on one generated map, paired with whether each is land and
/// whether it borders **salt** water (the seal's `adjacent_water: salt` site rule).
struct SealSite {
    id: String,
    position: UVec2,
    is_land: bool,
    has_adjacent_salt_water: bool,
}

/// One generated map's reading: where its seals are, and how much ground the new host actually
/// offers them (water-adjacent `boreal_arctic` land — the habitat the site rule admits).
struct Survey {
    seals: Vec<SealSite>,
    boreal_shore_tiles: usize,
}

/// Generate `seed`'s map through the **real** Startup chain (worldgen → hydrology → tag solver →
/// palette clamp → `spawn_initial_herds`) and report every Grey Seals herd site on it.
fn seal_sites(seed: u64, turns: u32) -> Vec<SealSite> {
    survey(seed, turns).seals
}

/// The full reading for `seed` after resolving `turns` full turns — see [`Survey`].
///
/// `turns == 0` reads herds at the moment they are **placed**; a positive `turns` also resolves
/// `advance_herds`, which is what makes the shore rule testable as a *standing* property rather
/// than a placement-time one. A seal's `route_len` is `[1, 1]`, so its single roam anchor **is**
/// its spawn tile and `step_herd_toward` never moves it — the colony is a fixed haul-out. That is
/// the whole reason the shore invariant survives a turn, so it is asserted, not assumed.
fn survey(seed: u64, turns: u32) -> Survey {
    let mut app = build_headless_app();

    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = GRID;
    let wrap = config.map_topology.wrap_horizontal;
    app.world.insert_resource(config);

    // Run the Startup chain in shipped order (worldgen → hydrology → tag solver → palette clamp →
    // reconcile → `spawn_initial_herds`), then resolve `turns` full turns so `advance_herds` gets
    // its chance to move the colony off its spawn tile.
    //
    // `app.update()` runs Startup itself on its **first** call: `Main::run_main` owns the
    // `Local<bool>` that gates the startup labels, and `world.run_schedule(Startup)` never touches
    // that local, so the chain here runs twice whenever a turn is resolved. That is a no-op — every
    // Startup spawner guards on an already-built world, `spawn_initial_world` included (see
    // `core_sim/tests/worldgen_startup_idempotent.rs`).
    app.world.run_schedule(bevy::app::Startup);
    for _ in 0..turns {
        app.update();
    }

    let width = GRID.x;
    let height = GRID.y;
    let registry = app.world.resource::<TileRegistry>().clone();
    let tags_at = |pos: UVec2| -> TerrainTags {
        registry
            .index(pos.x, pos.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
            .map(|tile| tile.terrain_tags)
            .unwrap_or_else(TerrainTags::empty)
    };
    let water_at = |pos: UVec2| -> bool { tags_at(pos).contains(TerrainTags::WATER) };
    // **Salt, not merely wet** — `WATER` WITHOUT `FRESHWATER`, the same rule `TileWorld::is_ocean`
    // states in `hydrology.rs`. A plain any-`WATER` test is what let a lakeside seal pass: an
    // `InlandSea` (or a navigable river) is `WATER | FRESHWATER`, so it satisfied the old predicate
    // AND the old assertion, and the invariant held vacuously.
    let salt_water_at = |pos: UVec2| -> bool {
        let tags = tags_at(pos);
        tags.contains(TerrainTags::WATER) && !tags.contains(TerrainTags::FRESHWATER)
    };

    let borders_salt_water = |pos: UVec2| -> bool {
        core_sim::grid_utils::hex_neighbors_wrapped(pos.x, pos.y, width, height, wrap)
            .any(|(nx, ny)| salt_water_at(UVec2::new(nx, ny)))
    };

    let seals = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .filter(|herd| herd.species == SEAL_SPECIES)
        .map(|herd| {
            let position = herd.position();
            SealSite {
                id: herd.id.clone(),
                position,
                is_land: !water_at(position),
                has_adjacent_salt_water: borders_salt_water(position),
            }
        })
        .collect();

    let boreal_shore_tiles = registry
        .tiles
        .iter()
        .filter_map(|&entity| app.world.get::<Tile>(entity))
        .filter(|tile| {
            classify_food_module(tile) == Some(FoodModule::BorealArctic)
                && !tile.terrain_tags.contains(TerrainTags::WATER)
                && borders_salt_water(tile.position)
        })
        .count();

    Survey {
        seals,
        boreal_shore_tiles,
    }
}

/// **The core invariant: zero inland seals, ever.** Every spawned colony sits on land that borders
/// **salt** water — the shore predicate, applied through the real spawn path.
///
/// The salt half closes a **vacuity** hole: with an any-`WATER` helper the assertion was satisfied by
/// an `InlandSea` or a navigable river (both `WATER | FRESHWATER`) — i.e. by exactly the tiles the
/// old predicate wrongly admitted — so a lakeside seal passed it.
///
/// **It is still not the discriminating guard, and must not be relied on as one.** Measured over
/// [`SWEEP_SEEDS`], a seal-hosting land tile bordering *only* fresh water is **0–10** against
/// **59–100** salt-shore ones, and the map-wide game cap seats just 2–3 colonies per map — so the
/// buggy case is a few percent per seed and this sweep passes on the **old** predicate by luck. The
/// regression that actually fails on the bug is
/// `fauna::tests::the_spawn_path_seats_seals_on_an_ocean_shore_and_never_on_a_lakeshore`, which
/// drives the real spawn path on a fixture where fresh-only shore is the *only* ground on offer.
/// This test remains the standing whole-map invariant.
#[test]
fn seals_spawn_only_on_water_adjacent_land() {
    for seed in SWEEP_SEEDS {
        for site in seal_sites(seed, 0) {
            assert!(
                site.is_land,
                "seed {seed}: a seal colony at {:?} sits on water, not a haul-out shore",
                site.position
            );
            assert!(
                site.has_adjacent_salt_water,
                "seed {seed}: a seal colony at {:?} borders no SALT water — a seal is a marine \
                 forager, and a freshwater lake or river is not a coast it can haul out on",
                site.position
            );
        }
    }
}

/// **The shore rule is a STANDING property, not a placement-time one.** The spawn filter only seats
/// a colony on a shore; nothing there stops `advance_herds` walking it inland on turn 1. What stops
/// it is the roster: a seal's `route_len` is `[1, 1]`, so its one roam anchor **is** its spawn tile,
/// `step_herd_toward` is handed its own position, and the colony is a fixed haul-out — a rookery the
/// animals swim out from, not a herd that wanders overland.
///
/// **It asserts the colony does not MOVE, not merely that it is still near water** — and that
/// distinction is the whole value of the test. Re-checking `has_adjacent_salt_water` after some turns
/// passes on the old `[1, 2]` roster too: this map's coastline is convoluted enough (~53% of the
/// largest landmass is coastal) that a one-hex wander almost always lands on *another* shore tile,
/// so the invariant survived on **geometry luck rather than design** and the assertion discriminated
/// nothing. Pinning the position is what actually fails on `[1, 2]`, where `build_short_route` picks
/// a second anchor with **no** site rule applied and the colony walks to it.
#[test]
fn a_seal_rookery_never_moves_off_its_haul_out() {
    const TURNS: u32 = 12;

    for seed in SWEEP_SEEDS {
        let placed: BTreeMap<String, UVec2> = seal_sites(seed, 0)
            .into_iter()
            .map(|site| (site.id, site.position))
            .collect();

        for site in seal_sites(seed, TURNS) {
            let Some(&spawned_at) = placed.get(&site.id) else {
                continue; // Immigration can seat a new colony mid-run; it has no turn-0 reading.
            };
            assert_eq!(
                site.position, spawned_at,
                "seed {seed}: seal colony {} left its haul-out ({spawned_at:?} → {:?}) within \
                 {TURNS} turns — a rookery is a fixed site",
                site.id, site.position
            );
            assert!(
                site.is_land && site.has_adjacent_salt_water,
                "seed {seed}: seal colony {} no longer sits on a shore at {:?}",
                site.id,
                site.position
            );
        }
    }
}

/// **The pinhole is cleared.** Hosting only `coastal_littoral` put the whole sweep at 1 colony
/// (~0 per map); cold coasts put it well above [`MIN_SEALS_OVER_SWEEP`]. Deliberately a sweep total,
/// not a per-seed assertion — spawning is a probabilistic roll under a map-wide cap.
#[test]
fn seals_clear_the_delta_pinhole() {
    let total: usize = SWEEP_SEEDS
        .iter()
        .map(|&seed| seal_sites(seed, 0).len())
        .sum();
    assert!(
        total >= MIN_SEALS_OVER_SWEEP,
        "seals spawned {total} times over {} seeds — the delta-pinhole regime (~0–1 per map) is back",
        SWEEP_SEEDS.len()
    );
}

/// The roster still offers seals in **both** hosts, and declares the site rule that makes the cold
/// one a *shoreline* rather than the whole tundra.
#[test]
fn the_seal_row_hosts_cold_coasts_under_the_shore_rule() {
    let fauna = FaunaConfig::builtin();
    let seal = fauna
        .species
        .get("seal")
        .expect("the roster defines the seal");

    assert_eq!(
        seal.adjacent_water,
        ShoreRequirement::Salt,
        "a seal must haul out on a SALT shore — it is a marine forager, so a landlocked lake is not \
         a coast"
    );
    for biome in ["boreal_arctic", "coastal_littoral"] {
        assert!(seal.hosts_biome(biome), "seals should host {biome}");
        assert!(
            fauna.abundance.probability_for(biome) > 0.0,
            "{biome} needs a positive abundance row or the seal can never spawn there"
        );
    }
}

/// **A migratory species requiring adjacent water is refused at load — at EVERY non-`None` kind.**
/// The migratory placement path picks anchors off `host_biomes` alone and never applies the site
/// rule, so the combination would be *silently ignored* — the unhandled state is made
/// unrepresentable instead.
#[test]
fn validate_rejects_a_migratory_species_requiring_adjacent_water() {
    for requirement in [
        ShoreRequirement::Any,
        ShoreRequirement::Salt,
        ShoreRequirement::Fresh,
    ] {
        let mut config = (*FaunaConfig::builtin()).clone();
        let seal = config
            .species
            .get_mut("seal")
            .expect("the roster defines the seal");
        seal.migratory = true;
        seal.adjacent_water = requirement;

        let err = config
            .validate()
            .expect_err("migratory + a shore rule must be refused");
        let message = err.to_string();
        assert!(
            message.contains("adjacent_water"),
            "the rejection must name the offending field, got: {message}"
        );
        assert!(
            message.contains(requirement.as_str()),
            "the rejection must name the offending value ({}), got: {message}",
            requirement.as_str()
        );
    }
}

/// **A migratory species with NO shore rule is still accepted.** `None` is the default and must stay
/// compatible with `migratory: true` — otherwise every migratory row in the roster would be refused.
#[test]
fn validate_accepts_a_migratory_species_without_a_shore_rule() {
    let mut config = (*FaunaConfig::builtin()).clone();
    let seal = config
        .species
        .get_mut("seal")
        .expect("the roster defines the seal");
    seal.migratory = true;
    seal.adjacent_water = ShoreRequirement::None;

    config
        .validate()
        .expect("migratory without a site rule is a perfectly ordinary roster row");
}

/// Measurement probe (`--ignored --nocapture`): seals per seed and the count of water-adjacent
/// `boreal_arctic` tiles the new host offers. Re-run it before retuning [`MIN_SEALS_OVER_SWEEP`].
#[test]
#[ignore]
fn seal_habitat_report() {
    let mut total = 0;
    for seed in SWEEP_SEEDS {
        let survey = survey(seed, 0);
        total += survey.seals.len();
        println!(
            "seed {seed}: {} seal colonies, {} water-adjacent boreal_arctic tiles",
            survey.seals.len(),
            survey.boreal_shore_tiles
        );
    }
    println!("total over {} seeds: {total}", SWEEP_SEEDS.len());
}
