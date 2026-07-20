//! The Telling — the **fork** tier, end to end on a real world.
//!
//! A fork posts a decision instead of a feed line, is marked fired only when *answered*, writes the
//! player's declared stance offset, and re-colours later beats. The shipped fork
//! (`sedentarization.soft_drift`) rides the rising crossing of sedentarization 40, so every test
//! here drives the real score across it through the real systems.

mod telling_support;

use core_sim::{BeatCatalogHandle, BeatConfigHandle, BeatLedger, FactionId, ForkAnswerError};

use telling_support::{
    beats, drive_sedentarization_past_the_soft_threshold, fork_events, run_turn, spawn_band,
    spawn_world,
};

const FORK_BEAT: &str = "sedentarization.soft_drift";
const PLAYER: FactionId = FactionId(0);

/// Answer a pending fork the way `handle_answer_fork` does, without standing up the command server.
fn answer(
    app: &mut bevy::app::App,
    choice: &str,
) -> Result<core_sim::ForkResolution, ForkAnswerError> {
    let catalog = app.world.resource::<BeatCatalogHandle>().get();
    let tick = app.world.resource::<core_sim::SimulationTick>().0;
    let mut ledger = app.world.resource_mut::<BeatLedger>();
    ledger.answer_fork(&catalog, PLAYER, FORK_BEAT, choice, tick)
}

fn pending(app: &bevy::app::App) -> Vec<String> {
    app.world
        .resource::<BeatLedger>()
        .pending_forks_for(PLAYER)
        .map(|fork| fork.beat_id.clone())
        .collect()
}

fn stance_offset(app: &bevy::app::App, axis: &str) -> f32 {
    app.world
        .resource::<BeatLedger>()
        .stance()
        .get(axis)
        .map(|value| value.to_f32())
        .unwrap_or(0.0)
}

/// Crossing the threshold posts a **pending fork**, not a feed line — and the beat is deliberately
/// **not** marked fired, because a fork fires when it is answered.
#[test]
fn crossing_the_threshold_posts_a_pending_fork_rather_than_a_feed_line() {
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 300);
    drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);

    assert_eq!(
        pending(&app),
        vec![FORK_BEAT.to_string()],
        "the crossing must put the question on the table"
    );
    assert!(
        !app.world.resource::<BeatLedger>().has_fired(FORK_BEAT),
        "a fork is fired when ANSWERED, not when posted"
    );
    assert!(
        !beats(&app).iter().any(|entry| entry
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("tier=fork"))),
        "a fork must not push a narrative-beat line to the feed"
    );

    let ledger = app.world.resource::<BeatLedger>();
    let fork = ledger.pending_forks_for(PLAYER).next().expect("posted");
    let config = app.world.resource::<BeatConfigHandle>().get();
    // Every register is rendered at post time — the register is a live user toggle.
    for register in &config.voice.registers {
        let line = fork
            .rendered
            .get(register)
            .unwrap_or_else(|| panic!("register {register} rendered"));
        assert!(
            !line.is_empty() && !line.contains('{'),
            "{register}: {line}"
        );
    }
    // Exactly one defer — the explicit out the client's turn gate depends on.
    assert_eq!(fork.choices.iter().filter(|c| c.is_defer).count(), 1);
    assert!(fork.choices.len() >= 2);
    // The gloss carries the real sampled score behind the question.
    let score = fork
        .gloss
        .iter()
        .find(|(signal, _)| signal == "sedentarization.score")
        .map(|(_, value)| value.to_f32())
        .expect("the fork glosses the score it fired on");
    assert!(score >= 40.0, "{score}");

    // Pending is not re-asked every turn.
    for _ in 0..5 {
        run_turn(&mut app);
    }
    assert_eq!(pending(&app).len(), 1, "a fork on the table is asked once");
}

/// Answering writes the declared stance offset, marks the beat fired, pushes the echo, and clears
/// the fork from `pending`.
#[test]
fn answering_writes_the_stance_offset_marks_fired_and_echoes() {
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 300);
    drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);

    let resolution = answer(&mut app, "yes_trail").expect("the fork is answerable");
    assert_eq!(resolution.choice_id, "yes_trail");

    // The declared offset **opposes** the accreted signal — the player is resisting their own
    // drift, which is exactly what the signal + offset split exists to represent.
    let offset = stance_offset(&app, "roam_settle");
    assert!((offset - (-0.4)).abs() < 1e-3, "{offset}");

    assert!(app.world.resource::<BeatLedger>().has_fired(FORK_BEAT));
    assert_eq!(
        app.world.resource::<BeatLedger>().answer(FORK_BEAT),
        Some("yes_trail")
    );
    assert!(
        pending(&app).is_empty(),
        "an answered fork leaves the table"
    );

    // The answer is part of the story record, not a silent state change.
    let config = app.world.resource::<BeatConfigHandle>().get();
    let echo = resolution.echo_line(&config.voice.default_register);
    assert!(!echo.is_empty() && !echo.contains('{'), "{echo}");

    // Rejections are distinct and legible.
    assert_eq!(
        answer(&mut app, "yes_trail"),
        Err(ForkAnswerError::NoPendingFork),
        "the fork is off the table now"
    );
}

/// The defer branch: it commits to nothing, and its `rearm_after_turns` lifts the `once` guard so
/// the question comes back.
#[test]
fn deferring_re_arms_the_once_beat_so_the_question_returns() {
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 300);
    drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);

    let rearm_turns = {
        let catalog = app.world.resource::<BeatCatalogHandle>().get();
        catalog
            .find(FORK_BEAT)
            .and_then(|beat| beat.defer_choice().and_then(|c| c.rearm_after_turns))
            .expect("the shipped defer re-arms")
    };

    answer(&mut app, "defer").expect("defer is answerable");
    assert_eq!(
        stance_offset(&app, "roam_settle"),
        0.0,
        "deferring commits to nothing"
    );
    assert!(
        app.world.resource::<BeatLedger>().has_fired(FORK_BEAT),
        "answering — even with defer — fires the beat"
    );

    // The `once` guard holds until the re-arm tick, then the trigger can post it again.
    for _ in 0..rearm_turns {
        run_turn(&mut app);
    }
    assert!(
        pending(&app).is_empty(),
        "no new crossing has happened yet, so nothing should be pending"
    );

    // Drop the score back under the threshold and drive it up again: with the guard lifted, the
    // beat is eligible and the fresh rising crossing re-posts it.
    telling_support::undomesticate_all(&mut app);
    telling_support::set_surplus(&mut app, PLAYER, 0);
    for _ in 0..60 {
        run_turn(&mut app);
    }
    let dropped = app
        .world
        .resource::<core_sim::SedentarizationScore>()
        .score(PLAYER);
    assert!(
        dropped < 40.0,
        "the score must fall back to re-arm the edge, got {dropped}"
    );
    drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);
    assert_eq!(
        pending(&app),
        vec![FORK_BEAT.to_string()],
        "a deferred fork returns once it re-arms and the trigger fires again"
    );
}

/// **The safety valve.** A fork nobody answers auto-resolves to its defer choice — the server never
/// waits on a player, so `pending` can never accumulate for an AI or unattended faction.
#[test]
fn a_fork_left_pending_past_the_expiry_auto_resolves_to_defer() {
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 300);
    drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);
    assert_eq!(pending(&app).len(), 1);

    let expire_turns = app
        .world
        .resource::<BeatConfigHandle>()
        .get()
        .budget
        .fork_expire_turns;

    for _ in 0..expire_turns {
        run_turn(&mut app);
    }

    assert!(pending(&app).is_empty(), "the valve must clear the table");
    assert_eq!(
        app.world.resource::<BeatLedger>().answer(FORK_BEAT),
        Some("defer"),
        "expiry resolves to the defer choice, exactly as a player defer would"
    );
    assert_eq!(
        stance_offset(&app, "roam_settle"),
        0.0,
        "an unanswered fork must never commit the player to a stance"
    );
    let expired = fork_events(&app);
    assert_eq!(expired.len(), 1, "the auto-defer is announced, not silent");
    assert!(expired[0]
        .detail
        .as_deref()
        .is_some_and(|d| d.contains("resolved=expired")));
}

/// **Re-colouring**: the answer a player gave to the fork changes how a *later, unrelated* beat
/// reads. The same collapsing herd, the same trigger, the same world — but a player who declared
/// "we are the trail" is shown the road going quiet, and one who declared "we were meant to root"
/// is shown that there is less reason to follow anything.
///
/// The comparison is between the two answers rather than against fixed dressings, because the
/// player's *behaviour* also accretes into the stance: at this point in the run the world itself
/// has drifted settle-ward, and the claim under test is that **the declaration moves the reading**,
/// not that it overwhelms the drift.
#[test]
fn the_answer_to_a_fork_re_colours_a_later_beat() {
    /// `weight(the_chase_thins) / weight(less_reason_to_follow)` for a run that answered `choice`.
    fn roam_reading_ratio(choice: &str) -> f32 {
        let mut app = spawn_world();
        spawn_band(&mut app, PLAYER, 300);
        drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);
        answer(&mut app, choice).expect("answerable");

        let config = app.world.resource::<BeatConfigHandle>().get();
        let catalog = app.world.resource::<BeatCatalogHandle>().get();
        let beat = catalog.find("ecology.herd_collapsing").expect("beat");
        // Effective stance = the world's accreted signal PLUS the offset the answer just declared,
        // so this reads the run's real score rather than a synthetic one.
        let score = app
            .world
            .resource::<core_sim::SedentarizationScore>()
            .score(PLAYER) as f64;
        let sample =
            core_sim::SignalSample::from_pairs([("sedentarization.score".to_string(), score)]);
        let stance = core_sim::telling::stance::effective_stance(
            &config,
            &sample,
            app.world.resource::<BeatLedger>().stance(),
        );
        let resolved = std::collections::BTreeMap::from([(
            "beast".to_string(),
            core_sim::Noun::Named {
                name: "Red Deer".to_string(),
                plural: "Red Deer".to_string(),
                adjective: "deer".to_string(),
            },
        )]);
        let candidates = core_sim::telling::select::weigh_wardrobe(
            beat,
            &resolved,
            None,
            &std::collections::BTreeMap::new(),
            0,
            &stance,
            &config.selection,
        );
        let weight_of = |id: &str| {
            candidates
                .iter()
                .find(|c| c.entry.id == id)
                .map(|c| c.weight)
                .unwrap_or_else(|| panic!("{id} should survive weighing"))
        };
        weight_of("collapse.the_chase_thins") / weight_of("collapse.less_reason_to_follow")
    }

    let trail = roam_reading_ratio("yes_trail");
    let root = roam_reading_ratio("no_root");
    assert!(
        trail > root,
        "declaring for the trail must weigh the roam reading of a collapse higher than declaring \
         for the ground does ({trail} vs {root})"
    );
    assert!(
        root < 1.0,
        "having declared for the ground, the settle reading should lead: {root}"
    );
}
