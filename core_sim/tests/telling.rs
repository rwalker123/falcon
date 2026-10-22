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

    // The opening guidance shares turn 0 on the ambient tier; the cold open is the beat that
    // glosses `turn.index`.
    let fired: Vec<_> = beats(&app)
        .into_iter()
        .filter(|e| {
            e.detail
                .as_deref()
                .is_some_and(|d| d.contains("turn.index="))
        })
        .collect();
    assert_eq!(fired.len(), 1, "exactly one opening beat fires on turn 0");
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

/// Right after the cold open, the opening guidance tells the player that food comes from the ground
/// around a band and that Split is the answer to outgrowing it — once, with the numbers behind it.
#[test]
fn the_opening_guidance_points_at_food_and_the_split_once() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 31);
    for _ in 0..12 {
        run_turn(&mut app);
    }

    let is_guidance = |label: &str| label.contains("split the band");
    let fired: Vec<_> = beats(&app)
        .into_iter()
        .filter(|e| is_guidance(&e.label))
        .collect();
    assert_eq!(fired.len(), 1, "the guidance is a `once` beat: {fired:?}");
    let entry = &fired[0];
    assert_eq!(
        entry.tick, 0,
        "it lands beside the cold open, so the game-start card can show it"
    );
    let detail = entry.detail.as_deref().expect("gloss present");
    assert!(detail.contains("provisions.total="), "{detail}");
    assert!(detail.contains("band.count="), "{detail}");
    assert!(detail.contains("tier=ambient"), "{detail}");
    assert!(app
        .world
        .resource::<BeatLedger>()
        .has_fired("guidance.food_and_the_split"));
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

// --- leadership (#859) -----------------------------------------------------------------------

/// Hand the faction's first unled resident band a leader of `kind` named `name`, the way
/// `advance_leadership` leaves one (`since_turn` is the tick the next turn samples).
fn give_a_band_a_leader(
    app: &mut bevy::app::App,
    faction: FactionId,
    kind: core_sim::LeaderKind,
    name: &str,
) {
    let band = {
        let mut query = app.world.query_filtered::<(
            bevy::prelude::Entity,
            &core_sim::PopulationCohort,
            Option<&core_sim::BandLeadership>,
        ), bevy::prelude::With<core_sim::ResidentBand>>();
        let mut unled: Vec<_> = query
            .iter(&app.world)
            .filter(|(_, cohort, led)| {
                cohort.faction == faction && led.is_none_or(|l| l.leader.is_none())
            })
            .map(|(entity, _, _)| entity)
            .collect();
        unled.sort();
        unled
            .into_iter()
            .next()
            .expect("the world has an unled resident band of the faction")
    };
    let since_turn = app.world.resource::<core_sim::SimulationTick>().0;
    app.world.entity_mut(band).insert(core_sim::BandLeadership {
        leader: Some(core_sim::BandLeader {
            name: name.to_string(),
            kind,
            since_turn,
        }),
        ..core_sim::BandLeadership::default()
    });
}

/// **A chief emerging fires the chief beat the turn it appears, dressed in the leader's name** --
/// and only that turn: the signal reads the leader's `since_turn`, so a standing leader is not news.
#[test]
fn a_chief_emerging_fires_the_chief_beat_with_the_leaders_name() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 31);
    for _ in 0..3 {
        run_turn(&mut app);
    }
    assert!(
        !app.world
            .resource::<BeatLedger>()
            .has_fired("leadership.chief_rises"),
        "fixture: no chief yet, no chief beat"
    );

    give_a_band_a_leader(&mut app, FactionId(0), core_sim::LeaderKind::Chief, "Orrin");
    run_turn(&mut app);

    assert!(app
        .world
        .resource::<BeatLedger>()
        .has_fired("leadership.chief_rises"));
    assert!(
        !app.world
            .resource::<BeatLedger>()
            .has_fired("leadership.council_forms"),
        "a chief is not a council"
    );
    let lines: Vec<_> = beats(&app)
        .into_iter()
        .filter(|e| e.label.contains("Orrin"))
        .collect();
    assert_eq!(lines.len(), 1, "one line, naming the leader: {lines:?}");
    assert!(
        !lines[0].label.contains('{'),
        "no unrendered placeholder: {:?}",
        lines[0].label
    );

    for _ in 0..6 {
        run_turn(&mut app);
    }
    assert_eq!(
        beats(&app)
            .iter()
            .filter(|e| e.label.contains("Orrin"))
            .count(),
        1,
        "a standing leader is not news the turn after"
    );
}

/// The council twin: an elders' council fires the council beat, naming its speaker.
#[test]
fn a_council_forming_fires_the_council_beat_with_the_speakers_name() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 31);
    for _ in 0..3 {
        run_turn(&mut app);
    }
    give_a_band_a_leader(
        &mut app,
        FactionId(0),
        core_sim::LeaderKind::ElderCouncil,
        "Hale",
    );
    run_turn(&mut app);

    assert!(app
        .world
        .resource::<BeatLedger>()
        .has_fired("leadership.council_forms"));
    assert!(
        !app.world
            .resource::<BeatLedger>()
            .has_fired("leadership.chief_rises"),
        "a council is not a chief"
    );
    assert!(
        beats(&app).iter().any(|e| e.label.contains("Hale")),
        "the speaker is named"
    );
}

/// **A council beat names the council's speaker even when a chief emerged the same turn.** The tier
/// budget lets one `beat` through a turn, so the chief beat is held back by its own cooldown (a
/// first chief fired it two turns earlier) and the council beat lands. The second chief sits on the
/// lower band id with the same `since_turn`, so a kind-blind "newest leader" resolver would name
/// the chief in the council beat.
#[test]
fn a_council_beat_names_its_speaker_not_a_chief_who_emerged_alongside() {
    let mut app = spawn_world();
    for _ in 0..3 {
        spawn_band(&mut app, FactionId(0), 31);
    }
    for _ in 0..3 {
        run_turn(&mut app);
    }
    give_a_band_a_leader(&mut app, FactionId(0), core_sim::LeaderKind::Chief, "Orrin");
    run_turn(&mut app);
    assert!(app
        .world
        .resource::<BeatLedger>()
        .has_fired("leadership.chief_rises"));
    // Two turns on: past the beat tier's own cooldown (2), inside the chief beat's (3).
    run_turn(&mut app);

    give_a_band_a_leader(&mut app, FactionId(0), core_sim::LeaderKind::Chief, "Yves");
    give_a_band_a_leader(
        &mut app,
        FactionId(0),
        core_sim::LeaderKind::ElderCouncil,
        "Hale",
    );
    run_turn(&mut app);

    let lines = beats(&app);
    assert!(
        lines.iter().any(|e| e.label.contains("Hale")),
        "the council beat names its speaker: {lines:?}"
    );
    assert!(
        !lines.iter().any(|e| e.label.contains("Yves")),
        "the held-back chief is not named in the council beat: {lines:?}"
    );
}

/// A led band lost the same turn another band gains a leader must not hide the new leader: the
/// old signal diffed standing counts, which netted to zero here, and the beat never fired.
#[test]
fn losing_a_led_band_the_same_turn_does_not_mask_a_new_chief() {
    let mut app = spawn_world();
    spawn_band(&mut app, FactionId(0), 31);
    spawn_band(&mut app, FactionId(0), 31);
    for _ in 0..3 {
        run_turn(&mut app);
    }
    give_a_band_a_leader(&mut app, FactionId(0), core_sim::LeaderKind::Chief, "Orrin");
    run_turn(&mut app);
    assert!(app
        .world
        .resource::<BeatLedger>()
        .has_fired("leadership.chief_rises"));
    for _ in 0..4 {
        // past the beat's cooldown
        run_turn(&mut app);
    }

    let led = {
        let mut query = app
            .world
            .query::<(bevy::prelude::Entity, &core_sim::BandLeadership)>();
        query
            .iter(&app.world)
            .find(|(_, l)| l.leader.is_some())
            .map(|(entity, _)| entity)
            .expect("one band is led")
    };
    app.world.despawn(led);
    give_a_band_a_leader(&mut app, FactionId(0), core_sim::LeaderKind::Chief, "Brin");
    run_turn(&mut app);
    assert!(
        beats(&app).iter().any(|e| e.label.contains("Brin")),
        "the new chief is named though a led band was lost: {:?}",
        beats(&app)
    );
}
