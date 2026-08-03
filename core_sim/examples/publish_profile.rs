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

/// The config key holding the map dimensions, overridden by the scaling sweep.
const GRID_SIZE_KEY: &str = "grid_size";

/// Milliseconds per second, for rendering.
const MILLIS_PER_SECOND: f64 = 1_000.0;

/// Percent, for the share column.
const PERCENT: f64 = 100.0;

/// The parent scope every `publish.*` sub-label is reported as a share of.
const DIFF_LABEL: &str = "publish.diff";

/// The capture phase the busy row is expected to move: `snapshot.build`'s allocation-heavy
/// remainder is where publisher contention lands.
const BUILD_LABEL: &str = "snapshot.build";

/// The prefix every `snapshot.build` sub-label shares. Row 4 reports each of these as a share of
/// [`BUILD_LABEL`], which contains them (the profiler nests flat — see `core_sim::turn_profile`).
const BUILD_PREFIX: &str = "snapshot.build.";

/// The map sizes row 4 sweeps, smallest first. The middle one is the shipped 80×52, and the outer
/// two are a half/double of it in **each** axis, so tile count moves by 4× per step — enough to
/// separate an `O(tiles)` section from one that merely looks big at the shipped size.
const SWEEP_GRIDS: [(u32, u32); 3] = [(40, 26), (80, 52), (160, 104)];

/// Nanoseconds per second, for the per-unit columns (a per-tile cost is sub-microsecond).
const NANOS_PER_SECOND: f64 = 1_000_000_000.0;

fn main() {
    let pinned = write_pinned_config(None);
    std::env::set_var("SIM_CONFIG_PATH", &pinned);

    println!("\nmap_seed={PINNED_MAP_SEED:#x}  warmups={WARMUP_TURNS}  frames={MEASURED_FRAMES}");
    publisher_row();
    turn_row("idle (publisher shut down)", Publisher::ShutDown);
    turn_row("busy (publisher concurrent)", Publisher::Running);
    build_breakdown_row();
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

/// Row 4: `snapshot.build`'s sub-label breakdown, swept across [`SWEEP_GRIDS`].
///
/// Two things this row does that the others do not:
///
/// * **The grids are interleaved, not batched.** All three worlds are built up front and one frame
///   of each is resolved per round, so a thermal or scheduling drift over the run lands on every
///   grid equally instead of on whichever was measured last. Sequential batches on this machine
///   drift by ~0.12 ms within a single baseline, which is the same order as the smaller sections.
/// * **Every section is reported per unit as well as per frame.** A section whose per-tile cost is
///   flat across a 16× change in tile count is genuinely `O(tiles)`; one whose per-tile cost falls
///   is fixed overhead wearing a tile-shaped disguise.
///
/// Idle, like row 2: the publisher is shut down before the warm-ups, so what is measured is the
/// turn thread alone.
fn build_breakdown_row() {
    let denominators: Vec<Denominators> = SWEEP_GRIDS.iter().map(|g| denominators_of(*g)).collect();

    let mut apps: Vec<_> = SWEEP_GRIDS
        .iter()
        .map(|grid| {
            std::env::set_var("SIM_CONFIG_PATH", write_pinned_config(Some(*grid)));
            let mut app = build_headless_app();
            app.world.resource_mut::<SnapshotHistory>().shutdown();
            for _ in 0..WARMUP_TURNS {
                run_turn(&mut app);
            }
            app
        })
        .collect();

    // Label → per-grid totals, in first-seen order so the table reads like the capture's timeline.
    let mut rows: Vec<(&'static str, Vec<Duration>)> = Vec::new();
    for _ in 0..MEASURED_FRAMES {
        for (idx, app) in apps.iter_mut().enumerate() {
            turn_profile::begin_turn();
            run_turn(app);
            for phase in turn_profile::take() {
                if phase.label != BUILD_LABEL && !phase.label.starts_with(BUILD_PREFIX) {
                    continue;
                }
                match rows.iter_mut().find(|(label, _)| *label == phase.label) {
                    Some((_, totals)) => totals[idx] += phase.total,
                    None => {
                        let mut totals = vec![Duration::ZERO; SWEEP_GRIDS.len()];
                        totals[idx] = phase.total;
                        rows.push((phase.label, totals));
                    }
                }
            }
        }
    }

    let frames = MEASURED_FRAMES as f64;
    println!("\nsnapshot.build breakdown, idle, interleaved across grids");
    print!("{:<30}", "world");
    for (w, h) in SWEEP_GRIDS.iter() {
        print!("{:>12}{:>10}", format!("{w}x{h}"), "ns/tile");
    }
    println!("{:>8}", "% build");
    for (label, totals) in &rows {
        print!("{:<30}", label.trim_start_matches("snapshot."));
        for (idx, total) in totals.iter().enumerate() {
            let ms = total.as_secs_f64() * MILLIS_PER_SECOND / frames;
            let per_tile = total.as_secs_f64() * NANOS_PER_SECOND
                / frames
                / f64::from(denominators[idx].tiles);
            print!("{ms:>12.3}{per_tile:>10.1}");
        }
        // Share of the parent at the shipped size, which is the middle grid.
        let shipped = SWEEP_GRIDS.len() / 2;
        let parent = rows
            .iter()
            .find(|(name, _)| *name == BUILD_LABEL)
            .map(|(_, t)| t[shipped])
            .unwrap_or(Duration::ZERO);
        let share = if parent.is_zero() {
            0.0
        } else {
            totals[shipped].as_secs_f64() / parent.as_secs_f64() * PERCENT
        };
        println!("{share:>8.1}");
    }

    println!("\ndenominators");
    print!("{:<30}", "count");
    for (w, h) in SWEEP_GRIDS.iter() {
        print!("{:>12}", format!("{w}x{h}"));
    }
    println!();
    for (name, pick) in Denominators::COLUMNS {
        print!("{name:<30}");
        for d in &denominators {
            print!("{:>12}", pick(d));
        }
        println!();
    }

    for mut app in apps {
        app.world.resource_mut::<SnapshotHistory>().shutdown();
    }
}

/// The counts every section's cost is divided by, read off one published snapshot per grid.
struct Denominators {
    tiles: u32,
    populations: u32,
    logistics: u32,
    power_nodes: u32,
    herds: u32,
    forage_patches: u32,
    culture_layers: u32,
    influencers: u32,
    food_modules: u32,
    /// Total named-plant shares across every patch — the *inner* loop count of both
    /// `build.patches` and `build.forage_patches`, since each share is priced at every rung. This
    /// is the denominator that separates "more tiles" from "a bigger flora roster".
    flora_shares: u32,
}

/// One row of the printed denominator census: its name and how to read it off a [`Denominators`].
type DenominatorColumn = (&'static str, fn(&Denominators) -> u32);

impl Denominators {
    /// The printed census, in the order the sections that consume them appear in the capture.
    const COLUMNS: [DenominatorColumn; 10] = [
        ("tiles", |d| d.tiles),
        ("food_modules", |d| d.food_modules),
        ("forage_patches", |d| d.forage_patches),
        ("flora shares (patch x species)", |d| d.flora_shares),
        ("logistics links", |d| d.logistics),
        ("power nodes", |d| d.power_nodes),
        ("population cohorts", |d| d.populations),
        ("herds", |d| d.herds),
        ("culture layers", |d| d.culture_layers),
        ("influencers", |d| d.influencers),
    ];
}

/// Build a throwaway world at `grid`, resolve enough turns to reach the same steady state the
/// measured run starts from, and count what the snapshot carries.
///
/// A separate app rather than the measured one because the measured one shuts its publisher down
/// **before** the warm-ups (that is what makes it the idle row), and with no publisher there is no
/// published snapshot to count.
fn denominators_of(grid: (u32, u32)) -> Denominators {
    std::env::set_var("SIM_CONFIG_PATH", write_pinned_config(Some(grid)));
    let mut app = build_headless_app();
    for _ in 0..WARMUP_TURNS {
        run_turn(&mut app);
    }
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .expect("a warmed-up world has published at least one snapshot");
    let counts = Denominators {
        tiles: snapshot.tiles.len() as u32,
        populations: snapshot.populations.len() as u32,
        logistics: snapshot.logistics.len() as u32,
        power_nodes: snapshot.power.len() as u32,
        herds: snapshot.herds.len() as u32,
        forage_patches: snapshot.forage_patches.len() as u32,
        culture_layers: snapshot.culture_layers.len() as u32,
        influencers: snapshot.influencers.len() as u32,
        food_modules: snapshot.food_modules.len() as u32,
        flora_shares: snapshot
            .forage_patches
            .iter()
            .map(|patch| patch.composition.len() as u32)
            .sum(),
    };
    app.world.resource_mut::<SnapshotHistory>().shutdown();
    counts
}

/// One entry per whole-section comparison in `PublishState::publish`, and whether that section is
/// present (i.e. judged **changed**) in this delta.
///
/// Each of these costs an unconditional clone of the whole section before the comparison that
/// usually finds it unchanged, which is what makes the count worth having.
fn changed_sections_of(delta: &WorldDelta) -> [(&'static str, bool); 38] {
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
        (
            "command_events_retention_turns",
            delta.command_events_retention_turns.is_some(),
        ),
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
/// `grid` overrides the map dimensions (row 4's sweep); `None` keeps the shipped size.
fn write_pinned_config(grid: Option<(u32, u32)>) -> PathBuf {
    let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("simulation_config.json");
    let text = std::fs::read_to_string(&shipped)
        .unwrap_or_else(|err| panic!("reading {}: {err}", shipped.display()));
    let mut config: serde_json::Value =
        serde_json::from_str(&text).expect("shipped simulation_config.json should parse");
    config[MAP_SEED_KEY] = serde_json::json!(PINNED_MAP_SEED);
    let suffix = match grid {
        Some((width, height)) => {
            config[GRID_SIZE_KEY] = serde_json::json!({ "x": width, "y": height });
            format!("_{width}x{height}")
        }
        None => String::new(),
    };

    // One file per grid, so a run's three worlds cannot race each other's config on disk.
    let out = std::env::temp_dir().join(format!("publish_profile_simulation_config{suffix}.json"));
    std::fs::write(&out, serde_json::to_string_pretty(&config).unwrap())
        .unwrap_or_else(|err| panic!("writing {}: {err}", out.display()));
    out
}
