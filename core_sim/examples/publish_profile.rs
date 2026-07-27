//! Ad-hoc harness for the snapshot-publication budget (`.claude/rules/core_sim/turn-profiling.md`).
//!
//! Prints the three rows that arc is measured in, all from the standard recipe — the shipped 80×52
//! `earthlike` / `late_forager_tribe` config with the map seed **pinned**:
//!
//! 1. **publisher** — the per-frame `publish.*` breakdown, drained from
//!    `SnapshotHistory::last_publish_profile()`, plus a census of which whole-section comparisons
//!    actually differ on a steady-state turn.
//! 2. **idle row** — `run_turn` and `snapshot.build` with the publisher **shut down**, i.e. what a
//!    turn executes when nothing competes with it. This is interactive play, where a player's turns
//!    are seconds apart and the publisher is long finished.
//! 3. **busy row** — the same two figures with turns resolving back to back and the publisher
//!    concurrently working. This is a batched `turn 100`, a benchmark, or a test loop, and it is the
//!    only row where publication's cost is *felt* by the turn thread.
//!
//! ```text
//! cargo run --release -p core_sim --example publish_profile
//! ```
//!
//! Three properties of the recipe the numbers depend on:
//!
//! * **The seed is pinned here, not in the shipped config.** `map_seed: 0` means "roll from
//!   entropy", so an unpinned run measures a different world every time. The pinned copy is built
//!   fresh from the current shipped file on every run rather than kept on disk, because a stale
//!   `SIM_CONFIG_PATH` copy is a boot panic, not a fallback.
//! * **The idle row shuts the publisher down** (`SnapshotHistory::shutdown`, shipped API) rather
//!   than sleeping between turns. A sleep lets the core idle down and inflates *every* phase on
//!   both threads by ~2.3×.
//! * **Nothing here spins on a clock.** A control thread calling `Instant::now()` in a loop contends
//!   on the macOS timebase and fabricates milliseconds of phantom slowdown in the thread under test.
//!
//! Reading the publisher profile drains the publisher's queue, so row 1 is an idle-row measurement
//! of the publisher's own work; that is deliberate, and it is why the busy row measures the turn
//! thread instead.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use core_sim::{build_headless_app, run_turn, turn_profile, SnapshotHistory};
use sim_runtime::WorldDelta;

/// Turns resolved before measurement starts, so the profile is a steady-state frame rather than a
/// world's first publication (which encodes a full snapshot instead of a delta).
const WARMUP_TURNS: usize = 5;

/// Frames averaged. The standard figure in `turn-profiling.md`.
const MEASURED_FRAMES: usize = 30;

/// A fixed, non-zero world seed. Any non-zero value serves; it only has to be the *same* one across
/// runs being compared.
const PINNED_MAP_SEED: u64 = 0x5EED_C0DE;

/// The config key holding the world seed.
const MAP_SEED_KEY: &str = "map_seed";

/// Milliseconds per second, for rendering.
const MILLIS_PER_SECOND: f64 = 1_000.0;

/// Percent, for the share column.
const PERCENT: f64 = 100.0;

/// The parent scope every `publish.*` sub-label is reported as a share of.
const DIFF_LABEL: &str = "publish.diff";

/// The capture phase the busy row is expected to move: `snapshot.build`'s allocation-heavy
/// remainder is where publisher contention lands.
const BUILD_LABEL: &str = "snapshot.build";

fn main() {
    let pinned = write_pinned_config();
    std::env::set_var("SIM_CONFIG_PATH", &pinned);

    println!("\nmap_seed={PINNED_MAP_SEED:#x}  warmups={WARMUP_TURNS}  frames={MEASURED_FRAMES}");
    publisher_row();
    turn_row("idle (publisher shut down)", Publisher::ShutDown);
    turn_row("busy (publisher concurrent)", Publisher::Running);
}

/// Whether the turn row runs against a live publisher.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Publisher {
    Running,
    ShutDown,
}

/// Row 1: the publisher's own per-frame breakdown, and the changed-section census.
fn publisher_row() {
    let mut app = build_headless_app();
    for _ in 0..WARMUP_TURNS {
        run_turn(&mut app);
    }

    // Phases in first-seen order, so the printed table reads like the publisher's timeline.
    let mut totals: Vec<(&'static str, Duration, u32)> = Vec::new();
    let mut changed_sections: Vec<(&'static str, u32)> = Vec::new();
    for _ in 0..MEASURED_FRAMES {
        run_turn(&mut app);
        let history = app.world.resource::<SnapshotHistory>();
        for phase in history.last_publish_profile() {
            match totals
                .iter_mut()
                .find(|(label, _, _)| *label == phase.label)
            {
                Some((_, total, calls)) => {
                    *total += phase.total;
                    *calls += phase.calls;
                }
                None => totals.push((phase.label, phase.total, phase.calls)),
            }
        }
        if let Some(delta) = history.last_delta() {
            for (label, present) in changed_sections_of(&delta) {
                match changed_sections.iter_mut().find(|(name, _)| *name == label) {
                    Some((_, count)) => *count += u32::from(present),
                    None => changed_sections.push((label, u32::from(present))),
                }
            }
        }
    }

    let frames = MEASURED_FRAMES as f64;
    let diff_total = totals
        .iter()
        .find(|(label, _, _)| *label == DIFF_LABEL)
        .map(|(_, total, _)| *total)
        .unwrap_or(Duration::ZERO);

    println!("\npublisher, per frame");
    println!(
        "{:<34} {:>9} {:>9} {:>9}",
        "label", "ms/frame", "% of diff", "calls/f"
    );
    for (label, total, calls) in &totals {
        let share = if diff_total.is_zero() {
            0.0
        } else {
            total.as_secs_f64() / diff_total.as_secs_f64() * PERCENT
        };
        println!(
            "{:<34} {:>9.3} {:>9.1} {:>9.2}",
            label,
            total.as_secs_f64() * MILLIS_PER_SECOND / frames,
            share,
            f64::from(*calls) / frames
        );
    }

    let sections = changed_sections.len();
    let ever: Vec<&(&str, u32)> = changed_sections
        .iter()
        .filter(|(_, count)| *count > 0)
        .collect();
    println!(
        "\nwhole-section comparisons: {sections} sections, {} of them present in at least one of \
         the {MEASURED_FRAMES} deltas",
        ever.len()
    );
    for (label, count) in ever {
        println!("{label:<34} {count:>9} / {MEASURED_FRAMES}");
    }
}

/// Rows 2 and 3: `run_turn` wall clock and `snapshot.build`, with and without a live publisher.
fn turn_row(name: &str, publisher: Publisher) {
    let mut app = build_headless_app();
    if publisher == Publisher::ShutDown {
        app.world.resource_mut::<SnapshotHistory>().shutdown();
    }
    for _ in 0..WARMUP_TURNS {
        run_turn(&mut app);
    }

    let mut turn_total = Duration::ZERO;
    let mut build_total = Duration::ZERO;
    for _ in 0..MEASURED_FRAMES {
        turn_profile::begin_turn();
        let started = Instant::now();
        run_turn(&mut app);
        turn_total += started.elapsed();
        build_total += turn_profile::take()
            .iter()
            .find(|phase| phase.label == BUILD_LABEL)
            .map(|phase| phase.total)
            .unwrap_or(Duration::ZERO);
    }

    let frames = MEASURED_FRAMES as f64;
    println!("\n{name}");
    println!(
        "{:<34} {:>9.3}",
        "run_turn",
        turn_total.as_secs_f64() * MILLIS_PER_SECOND / frames
    );
    println!(
        "{:<34} {:>9.3}",
        BUILD_LABEL,
        build_total.as_secs_f64() * MILLIS_PER_SECOND / frames
    );
}

/// One entry per whole-section comparison in `PublishState::publish`, and whether that section is
/// present (i.e. judged **changed**) in this delta.
///
/// Each of these costs an unconditional clone of the whole section before the comparison that
/// usually finds it unchanged, which is what makes the count worth having.
fn changed_sections_of(delta: &WorldDelta) -> [(&'static str, bool); 37] {
    [
        ("power_metrics", delta.power_metrics.is_some()),
        (
            "great_discovery_definitions",
            delta.great_discovery_definitions.is_some(),
        ),
        (
            "great_discovery_telemetry",
            delta.great_discovery_telemetry.is_some(),
        ),
        ("knowledge_metrics", delta.knowledge_metrics.is_some()),
        ("knowledge_timeline", delta.knowledge_timeline.is_some()),
        ("victory", delta.victory.is_some()),
        ("capability_flags", delta.capability_flags.is_some()),
        ("campaign_profiles", delta.campaign_profiles.is_some()),
        ("command_events", delta.command_events.is_some()),
        ("pending_forks", delta.pending_forks.is_some()),
        ("stance_axes", delta.stance_axes.is_some()),
        ("voice_medium", delta.voice_medium.is_some()),
        ("faction_inventory", delta.faction_inventory.is_some()),
        ("sedentarization", delta.sedentarization.is_some()),
        ("discovered_sites", delta.discovered_sites.is_some()),
        ("demographics", delta.demographics.is_some()),
        ("forage_patches", delta.forage_patches.is_some()),
        (
            "intensification_knowledge",
            delta.intensification_knowledge.is_some(),
        ),
        ("herds", delta.herds.is_some()),
        ("food_modules", delta.food_modules.is_some()),
        ("crisis_telemetry", delta.crisis_telemetry.is_some()),
        ("crisis_overlay", delta.crisis_overlay.is_some()),
        ("moisture_raster", delta.moisture_raster.is_some()),
        ("elevation_overlay", delta.elevation_overlay.is_some()),
        ("climate_bands", delta.climate_bands.is_some()),
        ("start_marker", delta.start_marker.is_some()),
        ("axis_bias", delta.axis_bias.is_some()),
        ("sentiment", delta.sentiment.is_some()),
        ("corruption", delta.corruption.is_some()),
        ("terrain", delta.terrain.is_some()),
        ("logistics_raster", delta.logistics_raster.is_some()),
        ("sentiment_raster", delta.sentiment_raster.is_some()),
        ("corruption_raster", delta.corruption_raster.is_some()),
        ("culture_raster", delta.culture_raster.is_some()),
        ("military_raster", delta.military_raster.is_some()),
        ("culture_tensions", delta.culture_tensions.is_some()),
        ("visibility_raster", delta.visibility_raster.is_some()),
    ]
}

/// Copy the **current** shipped `simulation_config.json` with the seed pinned, and return its path.
fn write_pinned_config() -> PathBuf {
    let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("simulation_config.json");
    let text = std::fs::read_to_string(&shipped)
        .unwrap_or_else(|err| panic!("reading {}: {err}", shipped.display()));
    let mut config: serde_json::Value =
        serde_json::from_str(&text).expect("shipped simulation_config.json should parse");
    config[MAP_SEED_KEY] = serde_json::json!(PINNED_MAP_SEED);

    let out = std::env::temp_dir().join("publish_profile_simulation_config.json");
    std::fs::write(&out, serde_json::to_string_pretty(&config).unwrap())
        .unwrap_or_else(|err| panic!("writing {}: {err}", out.display()));
    out
}
