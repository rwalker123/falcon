//! **EVERY TEST WORLD MUST BE BUILT ON THE PINNED HARNESS MAP.**
//!
//! `simulation_config.json` ships `map_seed: 0`, and worldgen reads that as *"draw a seed from
//! entropy"* — correct for a New Game, and fatal for a fixture. `core_sim::build_headless_app()` is
//! the **production** world builder (it is what `bin/server.rs` boots the real server with), so a
//! test that calls it directly generates **a different map on every run**: a hash over every
//! `(x, y, terrain)` differed on all ten sampled runs and the curated gathering-site list moved
//! between 129 and 133 entries.
//!
//! **What that costs is intermittent failure that looks like a real defect.** A fixture that asks
//! the map for "a cultivable gathering site" or "a bare sowable tile" gets different terrain each
//! run, so an assertion satisfied by whichever biome the search happened to land on passes most runs
//! and fails on the map where it does not — and the failure names the assertion, never the map.
//!
//! `core_sim::build_test_app()` is `build_headless_app` with `HARNESS_MAP_SEED` pinned. Nothing else
//! about it differs, and **the production builder is deliberately left alone**: pinning a seed in
//! there would make every New Game deterministic, which is changing the game to fix the tooling.

use std::{fs, path::PathBuf};

/// The forbidden call, as it appears in source. Written with its opening paren so this file's own
/// prose — and the doc comments on the very builder it guards — do not trip it.
const FORBIDDEN_CALL: &str = "build_headless_app(";

/// The one builder a test may use, named in the failure message so the fix is the next thing the
/// reader sees.
const THE_ONE_BUILDER: &str = "core_sim::build_test_app()";

/// **This file exempts itself**, because it necessarily contains the token it forbids.
const SELF: &str = "harness_map_seed_guard.rs";

/// **The marker that divides production code from test code inside a `src/` file.** `server.rs`
/// carries both, and the two are told apart by **role** rather than by line number: everything from
/// this attribute to the end of the file is the `#[cfg(test)] mod`, and everything above it ships.
const TEST_MODULE_MARKER: &str = "#[cfg(test)]";

/// Every Rust source the guard walks, in a stable order (a directory read is not ordered), paired
/// with the line the scan may start at.
///
/// - **Both integration suites** (`core_sim/tests`, `integration_tests/tests`) are wholly test code,
///   so the scan starts at line 1.
/// - **`core_sim/src/bin/server.rs`** is a mixed file: it holds ~70 tests *and* the two calls that
///   boot the real server (`main`, and the New Game rebuild). Those two must keep the production
///   builder — a New Game gets a random map, and that is the game working correctly — so the scan
///   starts at its `#[cfg(test)]` marker. That is a **role** boundary, not a line-number exemption:
///   move the module and the boundary moves with it.
///
/// Other `src/` files are not scanned: their unit tests assemble worlds by hand, and `mapgen.rs`'s
/// one caller takes an explicit seed as a parameter.
///
/// # ⛔ THE WALK IS RECURSIVE, AND THE SHARED HELPERS ARE WHY
///
/// A flat `read_dir` skipped every **subdirectory** of the two suites, and three exist:
/// `integration_tests/tests/common/`, `integration_tests/tests/fixtures/` and
/// `core_sim/tests/telling_support/`. `common/mod.rs` is pulled in by every integration test through
/// `mod common;`, so an offending call added *there* would put the whole suite back on a random map
/// while this guard stayed green — the fifty-ninth file, arriving through the one door the guard was
/// not watching.
fn scanned_sources() -> Vec<(PathBuf, usize)> {
    /// Every `.rs` under `dir`, at any depth.
    fn collect_rs(dir: &PathBuf, out: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(dir).unwrap_or_else(|err| {
            panic!("the test directory {} is readable: {err}", dir.display())
        });
        for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
            if path.is_dir() {
                collect_rs(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .expect("core_sim sits in the workspace root");
    let mut scanned: Vec<(PathBuf, usize)> = Vec::new();
    for dir in [
        manifest.join("tests"),
        workspace.join("integration_tests/tests"),
    ] {
        let mut found = Vec::new();
        collect_rs(&dir, &mut found);
        scanned.extend(found.into_iter().map(|path| (path, 0usize)));
    }
    scanned.sort();

    let server = manifest.join("src/bin/server.rs");
    let source = fs::read_to_string(&server)
        .unwrap_or_else(|err| panic!("{} is readable: {err}", server.display()));
    let module_start = source
        .lines()
        .position(|line| line == TEST_MODULE_MARKER)
        .unwrap_or_else(|| {
            panic!(
                "{} no longer carries a `{TEST_MODULE_MARKER}` module — either its tests moved (drop \
                 it from this guard) or the marker changed shape, and the guard has silently stopped \
                 scanning ~70 call sites",
                server.display()
            )
        });
    scanned.push((server, module_start));

    assert!(
        !scanned.is_empty(),
        "no sources found — the guard would be vacuous"
    );
    scanned
}

/// **A NEW TEST CANNOT SILENTLY GO BACK ON THE RANDOM MAP.** The migration that pinned the seed
/// touched 58 files; without this, the fifty-ninth reintroduces the flake and nobody notices until
/// it fails on somebody else's branch.
///
/// **Comment lines are exempt on purpose.** Several fixtures describe the turn pipeline as
/// *"`build_headless_app`'s Startup chain"*, which is still exactly what it is — the guard is about
/// what a test *calls*, so it reads code and leaves prose alone.
#[test]
fn no_test_builds_a_world_on_the_unpinned_production_builder() {
    let mut offenders: Vec<String> = Vec::new();
    for (path, scan_from) in scanned_sources() {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if name == SELF {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()));
        for (index, line) in source.lines().enumerate().skip(scan_from) {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.contains(FORBIDDEN_CALL) {
                offenders.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these tests build their world on the PRODUCTION builder, which honours \
         `simulation_config.json`'s `map_seed: 0` and therefore generates a DIFFERENT MAP EVERY RUN \
         — call {THE_ONE_BUILDER} instead, which is the same builder with `HARNESS_MAP_SEED` \
         pinned. If a fixture needs particular ground, state that ground (set the tile's terrain); \
         do not go back to hoping the map supplies it:\n  {}",
        offenders.join("\n  ")
    );
}

/// **THE GUARD MUST BE ABLE TO SEE THE THING IT FORBIDS**, or it is a test that passes because it is
/// looking at nothing. A source-walking guard has three silent failure modes — a path that resolves
/// to an empty directory, a needle that no longer matches the code it is meant to catch, and a role
/// boundary that skips the whole of the file it was meant to narrow — and [`scanned_sources`]'s own
/// assertions cover only the first and third.
#[test]
fn the_guard_would_catch_an_offender() {
    let staged = format!("    let mut app = {FORBIDDEN_CALL});");
    assert!(
        !staged.trim_start().starts_with("//") && staged.contains(FORBIDDEN_CALL),
        "the needle no longer matches a real call site, so the guard above cannot fail"
    );
    // …and the exemption really is comment-shaped, or every fixture's prose would be an offender.
    let prose = format!("    // the {FORBIDDEN_CALL}) Startup chain");
    assert!(
        prose.trim_start().starts_with("//"),
        "the comment exemption must key off the line, not off the token"
    );

    // **AND THE WALK REALLY DESCENDS.** The suites' shared helpers live in subdirectories
    // (`integration_tests/tests/common/`, `.../fixtures/`, `core_sim/tests/telling_support/`), and a
    // flat `read_dir` — which is what this guard had — reads as a full scan while silently missing
    // every one of them. Asserted by *depth* rather than by naming a directory, so moving or
    // renaming a helper module cannot quietly turn the walk flat again.
    let roots = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("core_sim sits in the workspace root")
            .join("integration_tests/tests"),
    ];
    let nested = scanned_sources()
        .into_iter()
        .filter(|(path, _)| {
            roots.iter().any(|root| {
                path.starts_with(root) && path.parent().is_some_and(|parent| parent != root)
            })
        })
        .count();
    assert!(
        nested > 0,
        "the walk must descend into the suites' subdirectories — a shared `common/mod.rs` is the \
         one file whose offence would reach every integration test at once"
    );

    // **AND THE MIXED FILE IS ACTUALLY NARROWED, NOT SKIPPED.** `server.rs` is the one source whose
    // scan starts part-way down; a boundary that landed past its last test would leave ~70 call
    // sites unguarded and this file would still be green.
    let (server, scan_from) = scanned_sources()
        .into_iter()
        .find(|(path, _)| path.ends_with("src/bin/server.rs"))
        .expect("the mixed production/test file is in the scan set");
    let source = fs::read_to_string(&server).expect("server.rs is readable");
    let lines: Vec<&str> = source.lines().collect();
    assert!(
        scan_from > 0 && scan_from < lines.len(),
        "the role boundary must fall inside the file, or the narrowing is meaningless"
    );
    assert!(
        lines[..scan_from]
            .iter()
            .any(|line| line.contains(FORBIDDEN_CALL)),
        "nothing above the boundary calls the production builder — either `main` stopped using it \
         (and the exclusion is now pointless) or the boundary has drifted below the calls it exists \
         to protect"
    );
    assert!(
        lines[scan_from..]
            .iter()
            .any(|line| line.contains("build_test_app(")),
        "no test below the boundary builds a world, so the narrowed scan is guarding nothing"
    );
}

/// **THE PIN ITSELF, NOT JUST ITS CALL SITES.** The guard above is satisfied by every test calling
/// [`core_sim::build_test_app`] — and would go on being satisfied if that function quietly stopped
/// pinning anything. This is the liveness half: **two worlds built the same way must be the same
/// world.**
///
/// Asserted as the pair, because either half alone is weak. The *resolved* seed catches the pin being
/// dropped or overwritten (worldgen replaces a `0` with an entropy draw and writes the result back to
/// `SimulationConfig::map_seed`, so reading it after the first `update()` is reading what the map was
/// actually generated from). The *terrain* comparison catches the seed being honoured while something
/// else in worldgen goes non-deterministic, which no seed check can see.
///
/// The hash is order-independent — a sum over per-tile hashes — because the tile query's order is not
/// part of the claim and asserting on it would fail for a reason that is not this one.
#[test]
fn two_test_worlds_are_the_same_world() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn fingerprint() -> (u64, u64, usize) {
        let mut app = core_sim::build_test_app();
        app.update();
        let resolved = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let mut terrain: u64 = 0;
        let mut tiles = 0usize;
        let mut query = app.world.query::<&core_sim::Tile>();
        for tile in query.iter(&app.world) {
            let mut hasher = DefaultHasher::new();
            (tile.position.x, tile.position.y, tile.terrain as u32).hash(&mut hasher);
            terrain = terrain.wrapping_add(hasher.finish());
            tiles += 1;
        }
        (resolved, terrain, tiles)
    }

    let (first_seed, first_terrain, tiles) = fingerprint();
    assert!(
        tiles > 0,
        "fixture: a world with no tiles fingerprints as 0 whatever worldgen did"
    );
    assert_eq!(
        first_seed,
        core_sim::HARNESS_MAP_SEED,
        "the harness seed did not survive worldgen — it resolved the `map_seed == 0` entropy branch \
         instead, so every fixture is back on a random map"
    );

    let (second_seed, second_terrain, second_tiles) = fingerprint();
    assert_eq!(second_seed, first_seed);
    assert_eq!(second_tiles, tiles);
    assert_eq!(
        first_terrain, second_terrain,
        "two worlds built through `build_test_app` differ in their terrain, so worldgen is \
         non-deterministic for a fixed seed — a far bigger finding than the pin this file guards"
    );
}
