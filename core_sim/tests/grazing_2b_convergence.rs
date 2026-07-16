//! Grazing Phase 2b-ii — **the convergence gate.** The coupled herd↔graze system is a
//! consumer–resource pair, and those OSCILLATE or CRASH if built carelessly (the exact trap the corral
//! hit — `docs/plan_corral_managed_population.md` §3). This test runs the **real** coupled systems
//! (`advance_herds` → `advance_herd_grazing` → `advance_graze_regrowth`, the live Logistics order)
//! forward many turns from every starting state and asserts a **stable fixed point** every time:
//!
//! - from under-grazed (graze full), at equilibrium, and over-grazed (graze near floor / two herds
//!   sharing one range),
//! - for every regime — especially **fast small game** (`r` near the graze's 0.40, the danger zone),
//! - an over-grazed range **recovers** to a stable smaller herd on degraded ground and does **not**
//!   crash permanently to the stripped floor,
//! - and it is **deterministic** (the same setup run twice is bit-identical, and a herd's ecological
//!   state round-trips the rollback snapshot).
//!
//! Nothing about 2b-ii ships unless this is green (`docs/plan_grazing_2b.md` §2.2 / §9.1).

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_graze_regrowth, advance_herd_grazing, advance_herds, spawn_initial_graze,
    spawn_initial_herds, spawn_initial_world, CultureManager, DiscoveryProgressLedger,
    FactionInventory, FaunaConfigHandle, GenerationRegistry, GrazeRegistry, Herd, HerdDensityMap,
    HerdRegistry, HerdTelemetry, LadderConfigHandle, MapPresets, MapPresetsHandle,
    SimulationConfig, SimulationTick, SizeClass, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle,
};

/// A pinned earthlike map (`map_seed` is otherwise entropy — pin it, per §9). Only used to stand up a
/// real `TileRegistry` + a seeded `GrazeRegistry`; the herds under test are placed by hand.
const MAP_SEED: u64 = 119304647;

/// Turns per run — well past the ≥200 the gate requires; the fast/slow regimes both settle far sooner.
const TURNS: u32 = 300;
/// The window whose spread proves convergence: a fixed point has a vanishing band here, a limit cycle
/// a large one. Compared between an early stretch and the tail to prove the band is not *growing*.
const SETTLE_WINDOW: usize = 40;
/// The tail band's peak-to-peak span, as a fraction of its mean, must sit under this "small band"
/// (§2.2 permits a bounded micro-band from the discretization — biomass settles dead flat; the graze
/// fraction can hold a fixed ~0.2% 2-cycle — but forbids anything larger). 1% is comfortably below a
/// real limit cycle (tens of %) yet above the discretization residue.
const SMALL_BAND: f32 = 1e-2;
/// The tail band must be no larger than an earlier band (times slack) — i.e. **not growing**. A stable
/// fixed point (or fixed micro-cycle) holds flat; a true instability widens.
const NON_GROWTH_SLACK: f32 = 1.25;

fn base_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
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
    app.world.insert_resource(GrazeRegistry::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_graze);
    app
}

/// One turn of the coupled fauna Logistics chain, in the live stage order: herds roam + recompute `K`
/// from the range's graze + grow toward it → herds eat their range down → graze regrows the eaten
/// state.
fn run_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_herd_grazing);
    app.world.run_system_once(advance_graze_regrowth);
}

/// The richest pasture tile on the map (a prairie-class patch), used as the single controlled range.
/// Returns `(tile, capacity)`.
///
/// Delegates to `GrazeRegistry::richest_patch`, whose **deterministic tie-break** this test depends on:
/// every tile of the richest biome shares the maximum capacity, so picking the winner off raw `HashMap`
/// order would sample a different neighbourhood (and a different pen footprint) each process.
fn richest_pasture(app: &App) -> (UVec2, f32) {
    app.world
        .resource::<GrazeRegistry>()
        .richest_patch()
        .expect("the earthlike map seeds graze patches")
}

/// Force the graze patch on `tile` to `fraction × capacity` (the initial under-/over-grazed state).
fn set_graze(app: &mut App, tile: UVec2, fraction: f32) {
    let mut graze = app.world.resource_mut::<GrazeRegistry>();
    let patch = graze.patch_mut(tile).expect("tile has a graze patch");
    patch.biomass = (fraction * patch.carrying_capacity).clamp(0.0, patch.carrying_capacity);
}

/// Replace the live herds with `count` identical **Small** (stationary, range-0) herds parked on
/// `tile`, each with the given metabolic `fodder` and wild `regrowth_rate` and starting `biomass`.
/// Small herds don't roam (`route_len == 1`), so the range they graze is exactly this one tile — the
/// clean single-tile coupled system (`count == 2` is the "two herds share one range" case).
fn seat_herds(app: &mut App, tile: UVec2, count: usize, fodder: f32, r: f32, biomass: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    for i in 0..count {
        registry.herds.push(Herd::new(
            format!("conv_{i}"),
            "Rabbit Warren".to_string(), // any display; Small ignores the species footprint
            SizeClass::Small,
            vec![tile],
            biomass,
            biomass, // spawn K; overwritten by the ecological recompute on turn 1
            fodder,
            r,
        ));
    }
}

/// The graze biomass on a tile as a fraction of its capacity.
fn graze_fraction(app: &App, tile: UVec2) -> f32 {
    let graze = app.world.resource::<GrazeRegistry>();
    let patch = graze.patch(tile).expect("tile has a graze patch");
    patch.biomass / patch.carrying_capacity
}

/// Total herd biomass currently on the map (across the seated herds).
fn total_biomass(app: &App) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|h| h.biomass)
        .sum()
}

/// The peak-to-peak span of the `SETTLE_WINDOW` samples ending at `end`, as a fraction of the window's
/// mean (`0` for a constant stretch).
fn window_spread(series: &[f32], end: usize) -> f32 {
    let win = &series[end - SETTLE_WINDOW..end];
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut sum = 0.0;
    for &v in win {
        lo = lo.min(v);
        hi = hi.max(v);
        sum += v;
    }
    let mean = sum / win.len() as f32;
    if mean.abs() < 1e-9 {
        return hi - lo;
    }
    (hi - lo) / mean
}

/// Assert a stock series has settled to a **stable** band: the tail band is small AND no larger than an
/// earlier band (not growing) — a fixed point or a bounded fixed micro-cycle, never a widening one.
fn assert_stock_settled(label: &str, stock: &str, series: &[f32]) -> f32 {
    let tail = window_spread(series, series.len());
    // An earlier window, a third of the run back, to prove the band is not growing toward the tail.
    let earlier = window_spread(series, series.len() - series.len() / 3);
    assert!(
        tail < SMALL_BAND,
        "{label}: {stock} must reach a stable fixed point (small band); tail band {tail:.2e} over the \
         last {SETTLE_WINDOW} turns exceeds {SMALL_BAND:.0e}"
    );
    assert!(
        tail <= earlier * NON_GROWTH_SLACK + 1e-4,
        "{label}: {stock} band must not be GROWING (tail {tail:.2e} vs earlier {earlier:.2e}) — that \
         is a developing oscillation, not convergence"
    );
    tail
}

/// Run one controlled scenario and return the `(biomass, graze_fraction)` series over `TURNS`.
fn run_scenario(
    count: usize,
    fodder: f32,
    r: f32,
    start_biomass: f32,
    start_graze_fraction: f32,
) -> (Vec<f32>, Vec<f32>, f32) {
    let mut app = base_world();
    let (tile, cap) = richest_pasture(&app);
    seat_herds(&mut app, tile, count, fodder, r, start_biomass);
    set_graze(&mut app, tile, start_graze_fraction);

    let mut biomass_series = Vec::with_capacity(TURNS as usize);
    let mut graze_series = Vec::with_capacity(TURNS as usize);
    for _ in 0..TURNS {
        run_turn(&mut app);
        biomass_series.push(total_biomass(&app));
        graze_series.push(graze_fraction(&app, tile));
    }
    (biomass_series, graze_series, cap)
}

/// Assert a scenario reaches a stable fixed point (tail band under tolerance in BOTH stocks) and that
/// neither stock crashed to zero, then return the settled `(total_biomass, graze_fraction)`.
fn assert_converges(
    label: &str,
    count: usize,
    fodder: f32,
    r: f32,
    start_biomass: f32,
    start_graze_fraction: f32,
) -> (f32, f32) {
    let (biomass, graze, cap) = run_scenario(count, fodder, r, start_biomass, start_graze_fraction);
    let final_b = *biomass.last().unwrap();
    let final_g = *graze.last().unwrap();
    let k_max = 0.10 * cap / fodder; // r_graze·cap/4 / fodder, the flat-K ceiling
    let b_spread = assert_stock_settled(label, "herd biomass", &biomass);
    let g_spread = assert_stock_settled(label, "range graze", &graze);

    println!(
        "  {label:38}: B {start_biomass:>7.0}->{final_b:>7.0} (K_max {k_max:>7.0}, {:>4.0}%) | \
         G {start_graze_fraction:.2}->{final_g:.3} | tail band B {b_spread:.1e} G {g_spread:.1e}",
        100.0 * final_b / (k_max * count as f32).max(1.0)
    );

    assert!(
        final_b > 0.0,
        "{label}: the herd must not crash to zero — it settles at a (possibly smaller) live size"
    );
    (final_b, final_g)
}

/// Species regimes to sweep, `(label, fodder, wild r)`. The migratory megafauna rate (0.04) and the
/// big-game rate (0.10) bracket the slow end; the fast small-game rate (0.35 — approaching the graze's
/// own 0.40) is the oscillation danger zone the gate exists for.
const REGIMES: [(&str, f32, f32); 4] = [
    ("fast small game (rabbit r=0.35)", 0.10, 0.35),
    ("big game (deer r=0.10)", 0.05, 0.10),
    ("slow megafauna (mammoth r=0.04)", 0.011, 0.04),
    ("hottest breeder (r=0.40 == graze)", 0.10, 0.40),
];

#[test]
fn every_species_converges_from_every_starting_state() {
    // Under-grazed (graze full, small herd), at equilibrium-ish, and a herd starting far above its
    // range's capacity — every path must settle, for every regime including the fast danger zone.
    for (label, fodder, r) in REGIMES {
        println!("=== {label} ===");
        // Under-grazed: full graze, a small starting herd grows INTO its range.
        let (b_under, _) = assert_converges(
            &format!("{label} / under-grazed"),
            1,
            fodder,
            r,
            0.10 * 240.0 / fodder * 0.25, // ~quarter of K_max on a prairie tile
            1.00,
        );
        // Over-populated: a herd far above what the range can feed, on full graze — must fall to K.
        let (b_over, _) = assert_converges(
            &format!("{label} / over-populated"),
            1,
            fodder,
            r,
            0.10 * 240.0 / fodder * 3.0, // 3× K_max
            1.00,
        );
        // The two paths converge to the SAME fixed point (a herd's size is set by its range, not its
        // history) — the defining property of carrying capacity.
        assert!(
            (b_under - b_over).abs() <= b_over.max(1.0) * 1e-2,
            "{label}: under- and over-populated starts must converge to the same K \
             (under {b_under}, over {b_over})"
        );
    }
}

#[test]
fn an_overgrazed_range_recovers_to_a_stable_smaller_herd_not_a_crash() {
    // The §2.1 / §7 requirement: a range pushed over its limit (graze near the reseed floor) must
    // settle at a stable herd on degraded-but-LIVE ground — never lock into the stripped floor. The
    // escapement floor (0.25) is what makes this hold; without it the range collapses to ~0.028·cap.
    for (label, fodder, r) in REGIMES {
        let (final_b, final_g) = assert_converges(
            &format!("{label} / OVER-grazed (graze 0.12)"),
            1,
            fodder,
            r,
            0.10 * 240.0 / fodder, // a full K_max herd dropped onto a near-stripped range
            0.12,
        );
        // "Degraded, not crashed": the range recovers to comfortably above the escapement floor
        // (0.25), and the herd holds a real fraction of K_max rather than a stripped remnant.
        assert!(
            final_g > 0.20,
            "{label}: an overgrazed range must recover above the stripped floor, got graze {final_g:.3}"
        );
        assert!(
            final_b > 0.10 * 240.0 / fodder * 0.5,
            "{label}: the recovered herd is a real (>50% K_max) population on degraded ground, not a \
             remnant: {final_b}"
        );
    }
}

#[test]
fn two_herds_sharing_one_range_settle_without_crashing() {
    // The over-subscribed commons: two herds grazing the SAME tile each independently size to the full
    // range flow, so their combined draw over-grazes it. The system must still settle (no runaway
    // oscillation, no crash) at a stable shared state — a smaller herd apiece on a drawn-down range.
    for (label, fodder, r) in REGIMES {
        let (final_b, final_g) = assert_converges(
            &format!("{label} / two herds share one tile"),
            2,
            fodder,
            r,
            0.10 * 240.0 / fodder * 0.25, // each starts modest, grows into the shared range
            1.00,
        );
        assert!(
            final_b > 0.0 && final_g > 0.20,
            "{label}: shared range stays live"
        );
    }
}

#[test]
fn the_coupled_system_is_deterministic() {
    // Determinism is the foundation of rollback reproducibility: the same controlled scenario run twice
    // must be **bit-identical** (the per-turn herd RNG is seeded from `map_seed ^ tick ^ id-hash`, and
    // the graze draw-down is drawn in a fixed `HerdRegistry` order). Two full runs, compared exactly.
    // (The *persistence* half — that the dynamic `carrying_capacity`, the per-species `regrowth_rate`
    // and `fodder_per_biomass` survive the snapshot mirror unchanged — is asserted by
    // `snapshot.rs::herd_round_trips_through_snapshot`.)
    let (a_b, a_g, _) = run_scenario(2, 0.10, 0.35, 300.0, 0.6);
    let (b_b, b_g, _) = run_scenario(2, 0.10, 0.35, 300.0, 0.6);
    assert_eq!(
        a_b, b_b,
        "identical setups must produce identical biomass series"
    );
    assert_eq!(
        a_g, b_g,
        "identical setups must produce identical graze series"
    );
}
