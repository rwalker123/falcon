//! **What a snapshot capture costs, measured rather than predicted.**
//!
//! The per-herd estimate tables were once ~95% of capture, and that number is the whole reason the
//! forecast query channel exists: a per-(band, herd) answer could not be a repricing of the tables,
//! because the tables were already the dominant cost of the turn's dominant phase. Retiring them is
//! the payoff, and a payoff quoted from arithmetic is not a payoff.
//!
//! Printed, never asserted. A pinned millisecond count would fail on a faster laptop, a slower CI
//! box and every unrelated worldgen retune, and chasing it would teach the next reader nothing. What
//! is durable is the **method**: the same map, fully revealed, several captures, the profiler's own
//! labels.
//!
//! ```text
//! cargo test -p core_sim --test capture_cost -- --ignored --nocapture
//! ```
//!
//! **Read the numbers as debug-build numbers** unless run with `--release`. They are ~15× the
//! release figures (`turn-profiling.md`), so the ratio between phases is the signal and the absolute
//! value is not.

use std::time::Duration;

use core_sim::{build_headless_app, run_turn, turn_profile, HerdRegistry, SimulationConfig};

/// Captures timed per run. Enough to see past a single unlucky turn without making the harness
/// tedious to sit through in debug.
const SAMPLES: usize = 5;

/// Turns run before timing starts. The first capture is a **full** snapshot against an empty
/// baseline and every later one is a delta, so timing the first would measure a case that happens
/// once per world rather than the steady state the turn loop actually lives in.
const WARMUP_TURNS: usize = 2;

/// The capture phases this harness reports, parent first. `snapshot.build` is the whole capture;
/// `snapshot.build.herds` is the pass that carried both estimate tables, and is therefore the one
/// the query channel is meant to move.
const REPORTED_PHASES: [&str; 3] = [
    "snapshot.build",
    "snapshot.build.herds",
    "snapshot.build.forage_patches",
];

fn mean(samples: &[Duration]) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    samples.iter().sum::<Duration>() / samples.len() as u32
}

#[test]
#[ignore = "measurement harness — run with --ignored --nocapture"]
fn measure_snapshot_capture() {
    let mut app = build_headless_app();
    // **Fully revealed**, because fog is what decides how many herds reach the capture at all: the
    // herd list is filtered against the viewer's visibility, so a fogged map measures a fraction of
    // the work and flatters whatever is being measured. This is the worst case, which is the only
    // case worth quoting.
    app.world.resource_mut::<SimulationConfig>().fog_enabled = false;

    for _ in 0..WARMUP_TURNS {
        run_turn(&mut app);
    }

    let grid = app.world.resource::<SimulationConfig>().grid_size;
    // Counted off the same telemetry the capture filters, so the number quoted beside the timing is
    // the population that timing actually paid for.
    let herds = app.world.resource::<HerdRegistry>();
    let total_herds = herds.entries().len();
    let huntable = herds
        .snapshot_entries()
        .iter()
        .filter(|entry| entry.huntable)
        .count();

    let mut samples: Vec<Vec<Duration>> = vec![Vec::new(); REPORTED_PHASES.len()];
    for _ in 0..SAMPLES {
        turn_profile::begin_turn();
        run_turn(&mut app);
        let phases = turn_profile::take();
        for (index, wanted) in REPORTED_PHASES.iter().enumerate() {
            let total = phases
                .iter()
                .find(|phase| phase.label == *wanted)
                .map(|phase| phase.total)
                .unwrap_or(Duration::ZERO);
            samples[index].push(total);
        }
    }

    let build = "release build";
    let profile = if cfg!(debug_assertions) {
        "DEBUG build (~15× slower than release — read the ratios, not the absolutes)"
    } else {
        build
    };
    println!("\n=== snapshot capture cost ===");
    println!("map {}×{}, fog off", grid.x, grid.y);
    println!("{total_herds} herds, {huntable} huntable");
    println!("{SAMPLES} captures after {WARMUP_TURNS} warm-up turns, {profile}\n");
    println!("{:<34} {:>10}", "phase", "mean ms");
    for (index, label) in REPORTED_PHASES.iter().enumerate() {
        println!(
            "{:<34} {:>10.2}",
            label,
            mean(&samples[index]).as_secs_f64() * 1000.0
        );
    }
    let whole = mean(&samples[0]).as_secs_f64();
    if whole > 0.0 {
        println!(
            "\nherds are {:.1}% of the capture",
            mean(&samples[1]).as_secs_f64() / whole * 100.0
        );
    }
}
