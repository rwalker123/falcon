//! The Telling — **memory threads, callbacks, and the maturing voice**, end to end on a real world.
//!
//! This is the slice that makes the layer *remember*: durable threads it can call back to, a
//! predicate that gates on what the player already decided, and a narrator whose medium matures
//! with the civilization.

mod telling_support;

use bevy::prelude::UVec2;

use core_sim::{
    BeatCatalogHandle, BeatLedger, DiscoveredSites, FactionId, SimulationTick, SitesConfigHandle,
};

use telling_support::{
    beats, drive_sedentarization_past_the_soft_threshold, keep_driving_settlement,
    let_settlement_lapse, roam_settle_stance, run_turn, spawn_band, spawn_world,
};

const PLAYER: FactionId = FactionId(0);
const FORK_BEAT: &str = "sedentarization.soft_drift";

fn beat_fired(app: &bevy::app::App, beat: &str) -> bool {
    app.world.resource::<BeatLedger>().has_fired(beat)
}

/// Record a site discovery the way `discover_sites` does, so `sites.discovered_this_turn` reads 1
/// on the next tick and `site.last_discovered` resolves to a real catalog entry.
fn discover_site(app: &mut bevy::app::App, pos: UVec2) -> String {
    let site_id = {
        let sites = app.world.resource::<SitesConfigHandle>().get();
        sites
            .catalog
            .keys()
            .next()
            .cloned()
            .expect("the sites catalog ships entries")
    };
    let display = {
        let sites = app.world.resource::<SitesConfigHandle>().get();
        sites
            .site(&site_id)
            .map(|def| def.display_name.clone())
            .expect("the site resolves")
    };
    app.world
        .resource_mut::<DiscoveredSites>()
        .record(PLAYER, pos, site_id);
    display
}

/// Answer the shipped fork the way `handle_answer_fork` does.
fn answer(app: &mut bevy::app::App, choice: &str) {
    let catalog = app.world.resource::<BeatCatalogHandle>().get();
    let tick = app.world.resource::<SimulationTick>().0;
    let mut ledger = app.world.resource_mut::<BeatLedger>();
    ledger
        .answer_fork(&catalog, PLAYER, FORK_BEAT, choice, tick)
        .expect("the fork is on the table");
}

// --- threads and callbacks ------------------------------------------------------------------

/// Discovering a site writes a durable `place` thread, and a beat thirty turns later calls back to
/// it **by name** — the payoff the memory ledger exists for.
#[test]
fn a_discovered_site_becomes_a_thread_the_story_returns_to_much_later() {
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 120);

    // Turn 0 spends the beat budget on the cold open; discover the site right after.
    run_turn(&mut app);
    let named = discover_site(&mut app, UVec2::new(3, 4));
    run_turn(&mut app);

    let threads: Vec<String> = app
        .world
        .resource::<BeatLedger>()
        .threads_of("place")
        .iter()
        .map(|thread| thread.name.clone())
        .collect();
    assert_eq!(
        threads,
        vec![named.clone()],
        "the site beat must promote its `place` noun into a thread"
    );

    // `memory.return_to_place` needs the thread to be 25 turns old and the campaign 30 turns in.
    for _ in 0..40 {
        run_turn(&mut app);
        if beat_fired(&app, "memory.return_to_place") {
            break;
        }
    }
    assert!(
        beat_fired(&app, "memory.return_to_place"),
        "the callback beat must land once the thread is old enough"
    );

    let callback = beats(&app)
        .into_iter()
        .find(|event| {
            event
                .detail
                .as_deref()
                .is_some_and(|d| d.contains("tier=beat"))
                && event.label.contains(&named)
        })
        .expect("the callback line names the remembered place");
    assert!(callback.label.contains(&named), "{}", callback.label);
}

/// **The thread is a snapshot.** Wiping the live discovery registry — the source the noun was
/// resolved from — must not make the callback vanish: the story remembers a place that may no
/// longer be anywhere the sim can look it up.
#[test]
fn a_thread_survives_its_source_disappearing() {
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 120);

    run_turn(&mut app);
    let named = discover_site(&mut app, UVec2::new(5, 6));
    run_turn(&mut app);
    assert_eq!(
        app.world.resource::<BeatLedger>().threads_of("place").len(),
        1
    );

    // The source is gone: nothing in the world can resolve `site.last_discovered` any more.
    *app.world.resource_mut::<DiscoveredSites>() = DiscoveredSites::default();

    for _ in 0..45 {
        run_turn(&mut app);
        if beat_fired(&app, "memory.return_to_place") {
            break;
        }
    }
    let callback = beats(&app)
        .into_iter()
        .find(|event| event.label.contains(&named));
    assert!(
        callback.is_some(),
        "a thread must not re-resolve — the callback has to survive its source going away"
    );
}

// --- the arc's thesis: one sim, two different stories ----------------------------------------

const TRAIL_ENDURES: &str = "identity.trail_endures";
const TRAIL_FORSAKEN: &str = "identity.trail_forsaken";
const WALLS_PROMISED: &str = "identity.walls_promised";

/// The turns each identity beat waits after the answer, read off the catalog rather than restated.
fn identity_gate(app: &bevy::app::App, beat: &str) -> u32 {
    let catalog = app.world.resource::<BeatCatalogHandle>().get();
    let mut gates = Vec::new();
    catalog
        .find(beat)
        .expect("the identity beat")
        .when
        .collect_answered_gates(&mut gates);
    gates
        .into_iter()
        .map(|(_, _, turns)| turns)
        .max()
        .expect("the beat gates on an answer")
}

/// Cross the threshold, answer, then run `turns` more under `regime`.
fn play_out(
    choice: &str,
    regime: fn(&mut bevy::app::App, FactionId, u32),
    turns: u32,
) -> bevy::app::App {
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 300);
    drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);
    answer(&mut app, choice);
    regime(&mut app, PLAYER, turns);
    app
}

/// **Kept the word.** A people that declared the trail and then *stopped driving settlement* still
/// reads as walking it (`stance.roam_settle < 0`), so `identity.trail_endures` is honest and fires.
#[test]
fn keeping_the_word_brings_trail_endures_not_trail_forsaken() {
    let app = play_out("yes_trail", let_settlement_lapse, 45);

    let stance = roam_settle_stance(&app, PLAYER);
    assert!(
        stance < 0.0,
        "a people that stopped settling must read as still roaming, got {stance}"
    );
    assert!(
        beat_fired(&app, TRAIL_ENDURES),
        "the word was kept — the beat that says so must land"
    );
    assert!(
        !beat_fired(&app, TRAIL_FORSAKEN),
        "the two beats are complementary; only one may ever fire"
    );
}

/// **Broke the word.** A people that declared the trail and then went on settling anyway reads as
/// settling (`stance.roam_settle >= 0`). `identity.trail_endures` would be the voice claiming
/// something the simulation contradicts, so it must stay silent and `trail_forsaken` speaks instead.
#[test]
fn breaking_the_word_brings_trail_forsaken_not_trail_endures() {
    let app = play_out("yes_trail", keep_driving_settlement, 45);

    let stance = roam_settle_stance(&app, PLAYER);
    assert!(
        stance >= 0.0,
        "a people that kept settling must read as settling, got {stance}"
    );
    assert!(
        !beat_fired(&app, TRAIL_ENDURES),
        "the voice must NEVER claim the word was kept while the sim says it was not"
    );
    assert!(
        beat_fired(&app, TRAIL_FORSAKEN),
        "the honest beat for a broken word must land instead"
    );
}

/// **The payoff.** Two runs of the *same* simulation under the *same* post-answer regime, differing
/// only in how the player answered one fork, tell different stories: the trail branch gets
/// `identity.trail_endures` and never sees `identity.walls_promised`, and the root branch gets
/// exactly the mirror.
#[test]
fn one_sim_two_stories_the_answer_decides_which_beat_finds_you() {
    for (choice, expected, forbidden) in [
        ("yes_trail", TRAIL_ENDURES, WALLS_PROMISED),
        ("no_root", WALLS_PROMISED, TRAIL_ENDURES),
    ] {
        // Identical treatment on both branches — the *answer* is the only difference, which is the
        // whole thesis. Lapsing leaves `roam_settle` at ≈ −0.4 after `yes_trail` and ≈ +0.4 after
        // `no_root`, so each branch's beat is honest about the people it describes.
        let app = play_out(choice, let_settlement_lapse, 45);

        assert!(
            beat_fired(&app, expected),
            "answering {choice:?} must eventually bring {expected}"
        );
        assert!(
            !beat_fired(&app, forbidden),
            "answering {choice:?} must never bring {forbidden} — that is the other player's story"
        );
        assert_eq!(
            app.world.resource::<BeatLedger>().answer(FORK_BEAT),
            Some(choice)
        );
    }
}

/// **The elapsed-time gate is real, not decoration.** `identity.trail_endures` says *"we have kept
/// our word to it"* — absurd the turn after the word is given. `min_turns_since: 20` is what makes
/// the beat mean anything, so this pins that it holds the beat back and then releases it.
///
/// Runs the **kept-the-word** scenario, so the beat is one that genuinely *can* fire and the only
/// thing holding it back is the clock.
#[test]
fn the_identity_beat_waits_the_declared_turns_after_the_answer() {
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 300);
    drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);
    answer(&mut app, "yes_trail");

    let gate = identity_gate(&app, TRAIL_ENDURES);
    assert!(
        gate > 0,
        "the authored beat must declare an elapsed-time gate"
    );
    let answered_at = app
        .world
        .resource::<BeatLedger>()
        .answered_at(FORK_BEAT)
        .expect("the answering tick is recorded");

    // Inside the gate: the beat must stay silent, however many turns the engine gets.
    while app.world.resource::<SimulationTick>().0 < answered_at + gate as u64 - 1 {
        let_settlement_lapse(&mut app, PLAYER, 1);
        assert!(
            !beat_fired(&app, TRAIL_ENDURES),
            "the beat fired {} turns after the answer, inside its {gate}-turn gate",
            app.world.resource::<SimulationTick>().0 - answered_at
        );
    }

    // Past it, the beat lands (allowing for the tier budget/cooldown to give it a turn).
    for _ in 0..20 {
        let_settlement_lapse(&mut app, PLAYER, 1);
        if beat_fired(&app, TRAIL_ENDURES) {
            break;
        }
    }
    assert!(
        beat_fired(&app, TRAIL_ENDURES),
        "once the gate elapses the beat must land"
    );
    assert!(
        app.world
            .resource::<BeatLedger>()
            .fired_ticks(TRAIL_ENDURES)
            .iter()
            .all(|tick| tick.saturating_sub(answered_at) >= gate as u64),
        "every firing must be at least the gate's turns after the answer"
    );
}

// --- the maturing voice ----------------------------------------------------------------------

/// Crossing a medium threshold advances the narrator's medium and fires the medium-advance beat
/// **exactly once** — and the medium never steps back down afterwards.
#[test]
fn crossing_a_medium_threshold_advances_the_voice_and_fires_its_beat_once() {
    const PAINTED: &str = "voice.medium_painted";
    let mut app = spawn_world();
    spawn_band(&mut app, PLAYER, 300);

    assert_eq!(
        app.world
            .resource::<BeatLedger>()
            .medium_for(PLAYER)
            .map(|medium| medium.index),
        None,
        "the medium is only attained once a turn has run"
    );

    drive_sedentarization_past_the_soft_threshold(&mut app, PLAYER);
    for _ in 0..10 {
        run_turn(&mut app);
        if beat_fired(&app, PAINTED) {
            break;
        }
    }

    let medium = app
        .world
        .resource::<BeatLedger>()
        .medium_for(PLAYER)
        .cloned()
        .expect("a turn has run");
    assert_eq!(medium.id, "painted", "{medium:?}");
    assert_eq!(medium.index, 1);
    assert!(
        beat_fired(&app, PAINTED),
        "advancing the medium must be narrated"
    );

    let painted_lines = beats(&app)
        .into_iter()
        .filter(|event| {
            event
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("voice.medium_index"))
        })
        .count();
    assert_eq!(
        painted_lines, 1,
        "the medium-advance beat fires exactly once"
    );

    // The civilization comes apart, but a people that learned to paint does not forget.
    telling_support::undomesticate_all(&mut app);
    for _ in 0..25 {
        telling_support::set_surplus(&mut app, PLAYER, 0);
        run_turn(&mut app);
    }
    let held = app
        .world
        .resource::<BeatLedger>()
        .medium_for(PLAYER)
        .cloned()
        .expect("still attained");
    assert_eq!(held.index, 1, "the medium must never regress");
    assert_eq!(
        beats(&app)
            .into_iter()
            .filter(|event| event
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("voice.medium_index")))
            .count(),
        1,
        "and it must not re-fire when the signal wobbles back across the threshold"
    );
}
