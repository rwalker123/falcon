//! **Regional game: the wet biomes and the boreal pen.** A guard against the roster's quietest
//! failure mode.
//!
//! A species reaches the map through exactly one join: its `host_biomes` keys are matched against
//! the biome names emitted by `classify_food_module`. Nothing checks that join — a key that names no
//! real biome, or a biome whose `abundance.per_biome` row is absent, does not fail `validate()` and
//! does not log. The species simply **never spawns**, on any seed, forever, and the only symptom is
//! an animal nobody ever sees. Every assertion here exists to make that silence loud.
//!
//! What it covers, all measured through the **real** Startup chain (worldgen → hydrology → tag
//! solver → palette clamp → `spawn_initial_herds`) over [`SWEEP_SEEDS`]:
//!
//! - **Silt Catfish** (`river_fish`) — the wet biomes' own game, hosting `riverine_delta` /
//!   `wetland_swamp` / `coastal_littoral` under the same `requires_adjacent_water` site rule the
//!   seal rides (see `fauna_coastal_habitat.rs`, which owns that rule's spec). Its shore invariant
//!   is asserted here too, because a catfish on dry inland ground is a bug in the same predicate.
//! - **Snow Hare Warren** (`snow_hare`) — the first `pen`-ceiling species hosting `boreal_arctic`.
//!   Before it, a northern start could reach no pen at all: the intensification ladder's middle rung
//!   was unreachable from a whole class of map positions. That is a *gameplay* invariant, so the
//!   ceiling is asserted against the roster, not just the spawn count. It hosts `boreal_arctic`
//!   **alone**, and the assertion that it never spawns on `montane_highland` is the second failure
//!   mode this file guards — see [`snow_hares_never_warren_the_highlands`].
//! - **Wild Boar** on `riverine_delta` — a newly added host on an existing species. The count is the
//!   only evidence the added key actually joined; boars spawn plentifully in their three older
//!   hosts, so a total-boar assertion would pass with the new key silently dead.
//!
//! Sweep totals, never per-seed counts: spawning is a probabilistic roll under a map-wide cap
//! (`abundance.max_total_game`), so any single seed may legitimately read zero.

use bevy::math::UVec2;

use core_sim::{
    build_headless_app, classify_food_module, FaunaConfig, FoodModule, HerdRegistry,
    HusbandryCeiling, SimulationConfig, Tile, TileRegistry, BUILTIN_FAUNA_CONFIG,
};
use sim_runtime::TerrainTags;

/// Seeds the sweep runs, at the shipped standard map size. Never 0 (the "roll from entropy"
/// sentinel). Kept identical to `fauna_coastal_habitat.rs` so the two files' measurements are
/// directly comparable.
const SWEEP_SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 119_304_647];

/// The shipped standard map dimensions.
const GRID: UVec2 = UVec2::new(80, 52);

const CATFISH_SPECIES: &str = "Silt Catfish";
const HARE_SPECIES: &str = "Snow Hare Warren";
const BOAR_SPECIES: &str = "Wild Boar";

/// Measured floor for Silt Catfish colonies across [`SWEEP_SEEDS`]. The sweep measures **20**
/// (1–6 per map) against **0** pre-change; the bound sits well under that so an ordinary abundance
/// retune doesn't trip it, while an unmatched host key (0, always, on every seed) fails loudly.
/// Catfish are the thinnest of the three — their hosts are the map's thinnest biomes and the
/// map-wide game cap is saturated (see `wet_biome_roster_report`) — so the floor is set low
/// deliberately. Re-measure with `wet_biome_roster_report` before moving it.
const MIN_CATFISH_OVER_SWEEP: usize = 6;

/// Measured floor for Snow Hare warrens across [`SWEEP_SEEDS`]. The sweep measures **37** (4–8 per
/// map) against **0** pre-change. Same convention as [`MIN_CATFISH_OVER_SWEEP`].
///
/// **The figure dropped from 66 when the host narrowed** — the hare used to also host
/// `montane_highland`, which is why git history shows a higher number and a floor of 20. That host
/// was removed because it put warrens on arid `CanyonBadlands`; see
/// [`snow_hares_never_warren_the_highlands`], which is the *real* guard here.
const MIN_HARES_OVER_SWEEP: usize = 12;

/// Measured floor for Wild Boar groups sitting on a `riverine_delta` tile — the newly added host —
/// across [`SWEEP_SEEDS`]. The sweep measures **54** (5–15 per map) against **0** pre-change (the
/// key did not exist, so *no* boar could stand on a delta). Same convention as
/// [`MIN_CATFISH_OVER_SWEEP`].
const MIN_DELTA_BOARS_OVER_SWEEP: usize = 15;

/// One herd site on a generated map: which species, where, and the terrain facts the site rules
/// care about.
struct HerdSite {
    species: String,
    position: UVec2,
    is_land: bool,
    has_adjacent_water: bool,
    biome: Option<FoodModule>,
}

/// One generated map's reading: every herd on it, plus the map-wide total so cap saturation is
/// visible.
struct Survey {
    herds: Vec<HerdSite>,
}

impl Survey {
    fn count(&self, species: &str) -> usize {
        self.herds.iter().filter(|h| h.species == species).count()
    }

    fn sites<'a>(&'a self, species: &'a str) -> impl Iterator<Item = &'a HerdSite> + 'a {
        self.herds.iter().filter(move |h| h.species == species)
    }

    fn delta_boars(&self) -> usize {
        self.sites(BOAR_SPECIES)
            .filter(|h| h.biome == Some(FoodModule::RiverineDelta))
            .count()
    }
}

/// Generate `seed`'s map through the real Startup chain and read back every herd site on it.
///
/// Startup is driven **once**, by hand. `app.update()` would run it too — `Main::run_main` owns the
/// `Local<bool>` gating the startup labels and `world.run_schedule(Startup)` never touches it — so
/// doing both double-runs worldgen, and `spawn_initial_world` has no idempotency guard. This survey
/// reads herds at placement time, so no turn is resolved and `run_schedule` alone is correct.
fn survey(seed: u64) -> Survey {
    let mut app = build_headless_app();

    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = GRID;
    let wrap = config.map_topology.wrap_horizontal;
    app.world.insert_resource(config);

    app.world.run_schedule(bevy::app::Startup);

    let width = GRID.x;
    let height = GRID.y;
    let registry = app.world.resource::<TileRegistry>().clone();
    let tile_at = |pos: UVec2| -> Option<&Tile> {
        registry
            .index(pos.x, pos.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
    };
    let water_at = |pos: UVec2| -> bool {
        tile_at(pos).is_some_and(|tile| tile.terrain_tags.contains(TerrainTags::WATER))
    };
    let borders_water = |pos: UVec2| -> bool {
        core_sim::grid_utils::hex_neighbors_wrapped(pos.x, pos.y, width, height, wrap)
            .any(|(nx, ny)| water_at(UVec2::new(nx, ny)))
    };

    let herds = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|herd| {
            let position = herd.position();
            HerdSite {
                species: herd.species.clone(),
                position,
                is_land: !water_at(position),
                has_adjacent_water: borders_water(position),
                biome: tile_at(position).and_then(classify_food_module),
            }
        })
        .collect();

    Survey { herds }
}

fn sweep() -> Vec<(u64, Survey)> {
    SWEEP_SEEDS
        .map(|seed| (seed, survey(seed)))
        .into_iter()
        .collect()
}

/// **The shipped roster still validates.** `load_fauna_config_from_env` logs a broken invariant at
/// error level and then *silently* falls back to the builtin, so a green simulation says nothing
/// about the file on disk. This parses `BUILTIN_FAUNA_CONFIG` through the same
/// `from_json_str` → `validate()` path a deployment would and surfaces the error text.
#[test]
fn the_shipped_fauna_config_validates() {
    FaunaConfig::from_json_str(BUILTIN_FAUNA_CONFIG)
        .expect("the shipped fauna_config.json must parse and validate");
}

/// The three roster changes are declared coherently: each new species names hosts that have an
/// abundance row (no row ⇒ probability 0 ⇒ never spawns), and the snow hare carries the `pen`
/// ceiling that is its entire reason for existing.
#[test]
fn the_wet_and_cold_rows_declare_reachable_hosts() {
    let fauna = FaunaConfig::builtin();

    let catfish = fauna
        .species
        .get("river_fish")
        .expect("the roster defines river_fish");
    assert!(
        catfish.requires_adjacent_water,
        "a river fish must sit on a shore"
    );
    assert!(
        !catfish.migratory,
        "the site rule is never applied on the migratory placement path"
    );

    let hare = fauna
        .species
        .get("snow_hare")
        .expect("the roster defines snow_hare");
    assert_eq!(
        hare.husbandry_ceiling,
        HusbandryCeiling::Pen,
        "the snow hare is what makes the pen rung reachable from a boreal start"
    );
    assert!(
        hare.hosts_biome("boreal_arctic"),
        "without boreal_arctic the hare gives the north nothing"
    );
    assert!(
        !hare.hosts_biome("montane_highland"),
        "montane_highland admits arid CanyonBadlands through the classifier fallback — a snow hare \
         must not host it (see snow_hares_never_warren_the_highlands)"
    );
    assert!(
        fauna
            .species
            .values()
            .filter(|s| s.husbandry_ceiling == HusbandryCeiling::Pen)
            .any(|s| s.hosts_biome("boreal_arctic")),
        "boreal_arctic must offer at least one pen-capable species"
    );

    let boar = fauna.species.get("boar").expect("the roster defines boar");
    assert!(
        boar.hosts_biome("riverine_delta"),
        "boars were given the delta as a host"
    );

    for (key, biome) in [
        ("river_fish", "riverine_delta"),
        ("river_fish", "wetland_swamp"),
        ("river_fish", "coastal_littoral"),
        ("snow_hare", "boreal_arctic"),
        ("boar", "riverine_delta"),
    ] {
        assert!(
            fauna.species[key].hosts_biome(biome),
            "{key} should host {biome}"
        );
        assert!(
            fauna.abundance.probability_for(biome) > 0.0,
            "{biome} needs a positive abundance row or {key} can never spawn there"
        );
    }
}

/// **Silt Catfish actually reach the map, and only ever from a shore.** The count guards the
/// host-key join; the shore assertion guards the site rule the species rides on.
#[test]
fn catfish_colonise_the_wet_biomes_from_the_shore() {
    let mut total = 0;
    for (seed, survey) in sweep() {
        total += survey.count(CATFISH_SPECIES);
        for site in survey.sites(CATFISH_SPECIES) {
            assert!(
                site.is_land,
                "seed {seed}: a catfish colony at {:?} sits on open water, not a bank",
                site.position
            );
            assert!(
                site.has_adjacent_water,
                "seed {seed}: a catfish colony at {:?} borders no water — an inland fish",
                site.position
            );
        }
    }
    assert!(
        total >= MIN_CATFISH_OVER_SWEEP,
        "catfish spawned {total} times over {} seeds (floor {MIN_CATFISH_OVER_SWEEP}) — the wet \
         biomes' own game is not reaching the map",
        SWEEP_SEEDS.len()
    );
}

/// **The boreal pen is reachable.** Snow hares have to be *on the map* for the north's pen rung to
/// exist in practice, not merely in the roster.
#[test]
fn snow_hares_warren_the_cold_biomes() {
    let total: usize = sweep()
        .iter()
        .map(|(_, survey)| survey.count(HARE_SPECIES))
        .sum();
    assert!(
        total >= MIN_HARES_OVER_SWEEP,
        "snow hares spawned {total} times over {} seeds (floor {MIN_HARES_OVER_SWEEP}) — a boreal \
         start has no pennable species",
        SWEEP_SEEDS.len()
    );
}

/// **No snow hare on an arid desert canyon** — the bug a live playtest found, and the reason
/// `snow_hare` hosts `boreal_arctic` **alone**.
///
/// `host_biomes` names a `FoodModule`, and a module is a *bucket* of terrains, not one terrain — so
/// a species cannot target a single `TerrainType`. `montane_highland` is the bucket that bites:
/// `CanyonBadlands` carries `ARID | HIGHLAND` and falls through to the fallback arm of
/// `classify_food_module_from_traits`, where the `HIGHLAND` test is evaluated **before** the `ARID`
/// one — so an arid badland classifies as `montane_highland` and a *snow* hare warrens in a desert
/// canyon. `boreal_arctic`, by contrast, is an **explicit** arm (BorealTaiga | Tundra |
/// PeriglacialSteppe | SeasonalSnowfield) and is exactly the hare's range.
///
/// Dropping the module costs the occasional alpine warren and no gameplay: `montane_highland`
/// already carries `crag_goat` at a `pen` ceiling, so the pen-rung gap this species closes was
/// `boreal_arctic`'s alone.
///
/// **This assertion, not the count, is the guard** — a re-added `montane_highland` would raise the
/// sweep total and sail past [`MIN_HARES_OVER_SWEEP`] while putting hares back in the canyons. The
/// same trap waits for the next cold-climate species pointed at `montane_highland`.
#[test]
fn snow_hares_never_warren_the_highlands() {
    for (seed, survey) in sweep() {
        for site in survey.sites(HARE_SPECIES) {
            assert_ne!(
                site.biome,
                Some(FoodModule::MontaneHighland),
                "seed {seed}: a snow hare warren at {:?} sits on montane_highland — which admits \
                 arid CanyonBadlands through the classifier's HIGHLAND-before-ARID fallback",
                site.position
            );
        }
    }
}

/// **The boar's new host joined.** Asserted on delta tiles specifically: boars are plentiful in
/// their three older hosts, so any total-boar bound would pass with `riverine_delta` dead.
#[test]
fn boars_take_the_river_deltas() {
    let total: usize = sweep().iter().map(|(_, s)| s.delta_boars()).sum();
    assert!(
        total >= MIN_DELTA_BOARS_OVER_SWEEP,
        "boars spawned on riverine_delta {total} times over {} seeds (floor \
         {MIN_DELTA_BOARS_OVER_SWEEP}) — the added host key is not joining",
        SWEEP_SEEDS.len()
    );
}

/// Measurement probe (`--ignored --nocapture`): per-seed counts for the three changed rows, plus the
/// map-wide herd total against `abundance.max_total_game`. If the totals sit at the cap, new species
/// are **displacing** old ones rather than adding — re-read the floors here before trusting them.
#[test]
#[ignore]
fn wet_biome_roster_report() {
    let cap = FaunaConfig::builtin().abundance.max_total_game;
    let (mut catfish, mut hares, mut delta_boars, mut herds) = (0, 0, 0, 0);
    for (seed, survey) in sweep() {
        catfish += survey.count(CATFISH_SPECIES);
        hares += survey.count(HARE_SPECIES);
        delta_boars += survey.delta_boars();
        herds += survey.herds.len();
        println!(
            "seed {seed}: {} herds (cap {cap}) | {} catfish | {} hares | {} boars ({} on delta)",
            survey.herds.len(),
            survey.count(CATFISH_SPECIES),
            survey.count(HARE_SPECIES),
            survey.count(BOAR_SPECIES),
            survey.delta_boars(),
        );
    }
    println!(
        "totals over {} seeds: {herds} herds | {catfish} catfish | {hares} hares | {delta_boars} \
         delta boars",
        SWEEP_SEEDS.len()
    );
}
