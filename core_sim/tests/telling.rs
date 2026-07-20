//! The Telling: beats fire once, on real sim state, dressed in live nouns, and land in the command
//! feed with a gloss showing the numbers behind the line. The **fork** tier has its own suite in
//! `telling_fork.rs`; the shared world harness lives in `telling_support`.

mod telling_support;

use core_sim::{BeatLedger, FactionId};

use telling_support::{
    beats, domesticate, resident_people, run_turn, set_surplus, spawn_band, spawn_world,
};

/// Turn 0 fires the opening beat, and the band count is interpolated into the copy.
#[test]
fn turn_zero_fires_the_cold_open_with_the_band_count_interpolated() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 31);
    let people = resident_people(&mut app, FactionId(0));
    run_turn(&mut app);

    let fired = beats(&app);
    assert_eq!(fired.len(), 1, "exactly the opening beat fires on turn 0");
    let entry = &fired[0];
    assert!(
        entry.label.contains(&people.to_string()),
        "the band count ({people}) must be interpolated into the line: {:?}",
        entry.label
    );
    assert!(
        !entry.label.contains('{'),
        "no unrendered placeholder may reach the player: {:?}",
        entry.label
    );
    let detail = entry.detail.as_deref().expect("gloss present");
    assert!(detail.contains(&format!("band.count={people}")), "{detail}");
    assert!(detail.contains("turn.index=0"), "{detail}");
    assert!(detail.contains("tier=beat"), "{detail}");

    // `once: true` — it never fires again.
    assert!(app
        .world
        .resource::<BeatLedger>()
        .has_fired("opening.cold_open"));
    for _ in 0..10 {
        run_turn(&mut app);
    }
    assert_eq!(
        beats(&app)
            .iter()
            .filter(|e| e
                .detail
                .as_deref()
                .is_some_and(|d| d.contains("turn.index=")))
            .count(),
        1,
        "the cold open is a `once` beat"
    );
}

/// Same seed, same world, same run → identical beat *and* wardrobe selection.
#[test]
fn selection_is_reproducible_across_two_runs_of_the_same_seed() {
    fn transcript() -> Vec<(String, Option<String>)> {
        let mut app = spawn_world();
        spawn_band(&mut app, FactionId(0), 42);
        domesticate(&mut app, FactionId(0), 3);
        for _ in 0..25 {
            set_surplus(&mut app, FactionId(0), 300);
            run_turn(&mut app);
        }
        beats(&app)
            .into_iter()
            .map(|e| (e.label, e.detail))
            .collect()
    }

    let first = transcript();
    let second = transcript();
    assert!(!first.is_empty(), "the run must produce some narration");
    assert_eq!(
        first, second,
        "a fixed seed must reproduce the same beats and the same dressings"
    );
}

/// A beat whose every wardrobe entry is excluded (its required noun is unresolvable) must emit
/// nothing **and must not be marked fired** — it can still land once the world can dress it.
#[test]
fn a_beat_with_no_dressable_wardrobe_does_not_fire() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 30);
    // No sites are ever discovered here, so `discovery.site_found` can never resolve `place`.
    for _ in 0..15 {
        run_turn(&mut app);
    }
    assert!(
        !app.world
            .resource::<BeatLedger>()
            .has_fired("discovery.site_found"),
        "an undressable beat must not be marked fired"
    );
}
