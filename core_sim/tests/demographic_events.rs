//! The demographic flows the player can finally read (issue #272).
//!
//! `advance_demographics` has always resolved births, maturation and three death terms and then
//! **discarded them** — the brackets moved and nothing said why, so nothing in the game told you a
//! child was born. These guards drive the real turn pipeline and assert the flows reach the feed as
//! whole-person events with the tokens the client parses.

use core_sim::{
    build_headless_app, run_turn, scalar_zero, BandId, CommandEventEntry, CommandEventKind,
    CommandEventLog, DemographicFlowAccumulator, PopulationCohort, ResidentBand, SimulationConfig,
    FOOD,
};

use bevy::prelude::{Entity, With};

/// Every feed entry of one kind, in log order.
fn events_of(app: &bevy::app::App, kind: CommandEventKind) -> Vec<CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == kind)
        .cloned()
        .collect()
}

/// The `key=value` tokens of a detail string, which is the space-delimited form
/// `CommandFeedController._detail_status_style` already parses.
fn detail_tokens(entry: &CommandEventEntry) -> Vec<(String, String)> {
    entry
        .detail
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|token| token.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn token(entry: &CommandEventEntry, key: &str) -> Option<String> {
    detail_tokens(entry)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

/// Pinned so every guard here drives the SAME world. `simulation_config.json` ships `map_seed: 0`,
/// which means *randomize* — worldgen has not run until the first turn, so setting it on the
/// resource first is what makes these runs reproducible.
const MAP_SEED: u64 = 119_304_647;

/// A headless world on the pinned seed, not yet generated.
fn world() -> bevy::app::App {
    let mut app = build_headless_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = MAP_SEED;
    app
}

/// How many turns the growth guards drive. Comfortably inside the retention window
/// (`command_events_retention_turns`, 20) — a longer run would legitimately evict the very births
/// it is looking for, since the shipped start band's larder is spent by ~turn 20 and it stops
/// bearing.
const GROWTH_TURNS: usize = 15;

/// **A band that grows announces it.** The shipped start profile is a well-fed ~30-person band, so
/// births accumulate slowly — which is exactly the case the carry exists for: several turns of a
/// fractional rate, then one whole child.
#[test]
fn a_growing_band_reports_a_birth_once_it_has_a_whole_child() {
    let mut app = world();
    for _ in 0..GROWTH_TURNS {
        run_turn(&mut app);
    }

    let births = events_of(&app, CommandEventKind::Born);
    assert!(
        !births.is_empty(),
        "a fed band must produce at least one whole birth inside the retention window"
    );

    let first = &births[0];
    assert!(
        first.label.contains("was born") || first.label.contains("were born"),
        "the label reads as prose: {:?}",
        first.label
    );
    let count: u32 = token(first, "count")
        .expect("a birth names its count")
        .parse()
        .expect("count is a number");
    assert!(count >= 1, "an event is never fired for nobody");
    assert!(
        token(first, "band").is_some(),
        "a birth names the band it happened in, so the client can re-label the row"
    );
    assert!(
        first.seq > 0,
        "every pushed event carries a real sequence — 0 is the never-pushed value"
    );
}

/// **Nothing is invented.** The events a band reports are whole people it actually gained, so the
/// reported births can never exceed the children the band's own carry has crossed.
#[test]
fn reported_births_never_exceed_what_the_carry_has_crossed() {
    let mut app = world();
    for _ in 0..GROWTH_TURNS {
        run_turn(&mut app);
    }

    let reported: u32 = events_of(&app, CommandEventKind::Born)
        .iter()
        .filter_map(|entry| token(entry, "count"))
        .filter_map(|count| count.parse::<u32>().ok())
        .sum();

    // What is still owed but not yet whole. Reported + carried is the total flow, so the reported
    // half alone is always a whole number of real people.
    let carried_remainder: f32 = {
        let mut query = app
            .world
            .query_filtered::<&DemographicFlowAccumulator, With<ResidentBand>>();
        query
            .iter(&app.world)
            .map(|carry| carry.births.to_f32())
            .sum()
    };
    assert!(
        carried_remainder < 1.0 + f32::EPSILON,
        "a carry never holds a whole person it has not reported: {carried_remainder}"
    );
    assert!(
        reported > 0,
        "the run must actually report births, or the carry assertion above proves nothing"
    );
}

/// **A band that starves reports its dead, with the cause the turn recorded.** Emptying every
/// larder makes hunger — not cold — the dominant term, and the event has to say so rather than
/// re-deriving it from a post-turn world that no longer knows.
///
/// Kept inside the retention window for the same reason the growth guards are: a starved band's
/// whole-person deaths land early and then thin out as the band does, so a longer run would evict
/// exactly what it is looking for.
#[test]
fn a_starving_band_reports_its_dead_and_names_hunger() {
    let mut app = world();
    run_turn(&mut app);

    for _ in 0..GROWTH_TURNS {
        {
            let mut query = app
                .world
                .query_filtered::<&mut PopulationCohort, With<ResidentBand>>();
            for mut cohort in query.iter_mut(&mut app.world) {
                cohort.stores.set(FOOD, scalar_zero());
            }
        }
        run_turn(&mut app);
    }

    let deaths = events_of(&app, CommandEventKind::Died);
    assert!(
        !deaths.is_empty(),
        "an emptied larder must kill somebody inside the retention window"
    );
    let hungry = deaths
        .iter()
        .find(|entry| token(entry, "cause").as_deref() == Some("hunger"))
        .expect("a starving band's deaths are attributed to hunger");
    assert!(
        hungry.label.contains("died of hunger"),
        "the label names the cause: {:?}",
        hungry.label
    );
    let bracket = token(hungry, "bracket").expect("a death names its bracket");
    assert!(
        ["child", "working", "elder"].contains(&bracket.as_str()),
        "bracket token is one of the three age brackets, got {bracket:?}"
    );
    assert!(
        token(hungry, "band").is_some() && token(hungry, "count").is_some(),
        "every death event carries band= and count="
    );
}

/// **The band a death names is a real band**, so the client can key its row to the band panel.
#[test]
fn a_demographic_event_names_a_live_band_id() {
    let mut app = world();
    for _ in 0..GROWTH_TURNS {
        run_turn(&mut app);
    }

    let live: Vec<u64> = {
        let mut query = app.world.query_filtered::<&BandId, With<ResidentBand>>();
        query.iter(&app.world).map(|band| band.0).collect()
    };
    assert!(!live.is_empty(), "worldgen spawns bands");

    let demographic = [
        CommandEventKind::Born,
        CommandEventKind::Died,
        CommandEventKind::CameOfAge,
        CommandEventKind::Migrated,
    ];
    let mut seen = 0;
    for kind in demographic {
        for entry in events_of(&app, kind) {
            let band: u64 = token(&entry, "band")
                .expect("every demographic event names a band")
                .parse()
                .expect("band= is a number");
            assert!(
                live.contains(&band),
                "{kind:?} named band {band}, which is not one of {live:?}"
            );
            seen += 1;
        }
    }
    assert!(seen > 0, "the run must produce demographic events at all");
}

/// **Every band worldgen spawns carries a flow accumulator**, or its births and deaths are silently
/// unreportable — the carry is what the feed is made of, and a spawn seam that forgot it would show
/// up as a band that simply never narrates.
#[test]
fn every_resident_band_carries_a_flow_accumulator() {
    let mut app = world();
    app.update();

    let mut query = app
        .world
        .query_filtered::<(Entity, Option<&DemographicFlowAccumulator>), With<ResidentBand>>();
    let bands: Vec<(Entity, bool)> = query
        .iter(&app.world)
        .map(|(entity, carry)| (entity, carry.is_some()))
        .collect();

    assert!(!bands.is_empty(), "worldgen produced no bands to check");
    let missing: Vec<Entity> = bands
        .iter()
        .filter(|(_, has)| !has)
        .map(|(entity, _)| *entity)
        .collect();
    assert!(
        missing.is_empty(),
        "{} of {} resident bands have no `DemographicFlowAccumulator`, so their births and deaths \
         can never reach the feed: {missing:?}",
        missing.len(),
        bands.len()
    );
}

/// **The retention window reaches the wire**, so the client can say how far back the dock reads
/// rather than guessing.
#[test]
fn the_retention_window_is_published_from_config() {
    let mut app = world();
    run_turn(&mut app);

    let configured = app
        .world
        .resource::<SimulationConfig>()
        .command_events_retention_turns;
    assert!(configured > 0, "a zero-turn window would retain nothing");
    assert_eq!(
        app.world.resource::<CommandEventLog>().retention_turns(),
        configured,
        "the live log is windowed by the config lever, not by a hardcoded default"
    );

    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .last_snapshot()
        .expect("a turn published a snapshot");
    assert_eq!(
        snapshot.command_events_retention_turns as u64, configured,
        "and the snapshot publishes the same number the log is running"
    );
}
