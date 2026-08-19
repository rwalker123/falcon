//! **THE HERD'S PUBLISHED COUNTDOWN IS THE SHED IT CLAIMS TO COUNT DOWN.**
//!
//! `fauna::herd_neglect_grace_remaining` fills `HerdTelemetryState.neglectGraceRemaining`, and
//! `advance_husbandry` gates the shed on `RungDef::upkeep_grace_turns`. Those have to be the *same*
//! grace. They were not: the readout asked for the **build's** grace, which every shipped rung
//! declares `null`, so it resolved to `NO_NEGLECT_GRACE` and a herd at zero neglect published
//! `(0 + 1) − 0 = 1` — *"sheds in 1 turn"* — at **every** staffing, a fully-kept herd included, while
//! the shed itself waited out `animal:pastoral`'s upkeep grace of 2.
//!
//! The plant twin already read the upkeep's grace and said why
//! (`forage::patch_neglect_grace_remaining`); this file is the animal half of that pairing, and it is
//! deliberately the same shape as
//! `forage_cultivation::the_published_neglect_countdown_hits_zero_on_the_turn_the_meter_moves`.
//!
//! **Both halves are needed and neither is sufficient.** A constant would pass the fully-kept
//! assertion; a countdown that merely decremented from anywhere would pass the unkept one. Only the
//! pair pins the number to the mechanic.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;

use core_sim::{
    advance_husbandry, build_test_app, recapture_snapshot_in_place, FaunaConfigHandle,
    HerdRegistry, LadderConfigHandle, RungKey, SnapshotHistory,
};

/// **The species the fixture reshapes its herd into** — a roster row that will actually tame
/// (`tame_outright` refuses a `wild`-ceiling herd) and that declares an `animals_per_herder`, which
/// is the denominator the pastoral rung's `source_load` upkeep rides. The same row
/// `herd_row_one_frame.rs` pins, for the same reason.
const PASTORAL_SPECIES: &str = "Wild Boar";

/// **Keepers supplying exactly what the herd owes** — the state the defect misreported.
const FULLY_KEPT: f32 = 1.0;

/// **Nobody on the herd at all** — the state the countdown is supposed to be counting.
const NOBODY_KEEPING: f32 = 0.0;

/// Turns to hold a fully-kept herd for, past the point the grace would have elapsed had the keeping
/// been unmet — long enough that a countdown quietly ticking under a met keeping would show.
const HELD_TURNS: u32 = 4;

/// A headless world whose one game herd is a tamed, **unpenned** [`PASTORAL_SPECIES`] flock standing
/// at its carrying capacity, owned by the viewer.
///
/// **Tamed to the VIEWER on purpose**: `herd_is_visible` passes an owned herd unconditionally, so the
/// row cannot be dropped by fog and turn an assertion into a pass for the wrong reason. **Full to
/// capacity** because the shed is measured in *whole animals* (`MIN_ESCAPE_ANIMALS`) — a thin flock's
/// overage can round away and the second arm would never see its penalty bite.
fn world_with_a_kept_pastoral_herd() -> (App, String) {
    let mut app = build_test_app();
    app.update();

    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|herd| herd.id.starts_with("game_"))
            .map(|herd| herd.id.clone())
            .expect("the map seeded short-range game")
    };
    let species = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .species_by_display(PASTORAL_SPECIES)
        .expect("the shipped roster carries the fixture species")
        .clone();
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the herd the id came from");
        herd.species = PASTORAL_SPECIES.to_string();
        herd.body_mass = species.body_mass;
        herd.husbandry_ceiling = species.husbandry_ceiling;
        assert!(
            herd.tame_outright(viewer, &core_sim::LadderConfig::builtin()),
            "the fixture species must actually tame"
        );
        herd.biomass = herd.carrying_capacity;
    }
    (app, id)
}

/// **The grace the SHED waits out**, read off the shipped ladder rather than restated as a literal,
/// so a retune moves this test with the game. The fixture herd is tamed and unpenned, so
/// `herd_keeping_rung` answers `animal:pastoral`.
fn shed_grace(app: &App) -> u32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .rung(RungKey::AnimalPastoral)
        .upkeep_grace_turns()
}

/// **Seat this herd's keeping at `fraction` of what it owes.** `advance_husbandry` clears
/// `upkeep_supplied` after reading it (the Population→Logistics lag the labor arm writes across), so
/// a herd meant to stay kept has to be re-seated every turn.
fn seat_keeping(app: &mut App, id: &str, fraction: f32) {
    let supplied = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry.find(id).expect("the fixture herd survives");
        fraction * core_sim::herd_upkeep_demand(herd, &fauna, &ladder)
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry
        .herds
        .iter_mut()
        .find(|herd| herd.id == id)
        .expect("the fixture herd survives")
        .upkeep_supplied = supplied;
}

/// **The countdown and its bool, off the ENCODED buffer** — the artifact the client renders "sheds in
/// N turns" from, not the seam that produced it.
fn published_countdown(app: &mut App, id: &str) -> (bool, u32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    app.world.run_system_once(recapture_snapshot_in_place);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.herds())
        .expect("the subsistence section carries the herd list")
        .iter()
        .find(|herd| herd.id().unwrap_or_default() == id)
        .expect("the fixture herd is on the wire — it is OWNED, which passes the fog gate");
    (row.hasNeglectGrace(), row.neglectGraceRemaining())
}

fn live_biomass(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("the fixture herd survives")
        .biomass
}

/// **A HERD WHOSE KEEPING IS MET PUBLISHES THE WALK-AWAY COUNTDOWN, NOT A WARNING.**
///
/// `neglect_grace_remaining`'s contract at zero neglect is `grace + 1` — *"walk away and you have
/// this long"* — which is the plant web's convention and a true, useful reading rather than a state
/// needing a special case. What it must never be is **`1`**, the number the build grace produced: a
/// fully-staffed herd reading *"sheds in 1 turn"* is the defect, and it is asserted by name so a
/// regression cannot hide behind a generic inequality.
#[test]
fn a_fully_kept_herd_publishes_the_walk_away_countdown() {
    let (mut app, id) = world_with_a_kept_pastoral_herd();
    let grace = shed_grace(&app);
    let flock = live_biomass(&app, &id);

    for _ in 0..HELD_TURNS {
        seat_keeping(&mut app, &id, FULLY_KEPT);
        app.world.run_system_once(advance_husbandry);

        let (has_grace, remaining) = published_countdown(&mut app, &id);
        assert!(has_grace, "a tamed herd always has a flock at risk");
        assert_ne!(
            remaining, 1,
            "a fully kept herd must not publish 'sheds in 1 turn' — that is the build grace \
             (null on every rung, so NO_NEGLECT_GRACE) reaching the wire"
        );
        assert_eq!(
            remaining,
            grace + 1,
            "a kept herd offers the whole grace plus the turn the shed would bite on"
        );
        assert_eq!(
            live_biomass(&app, &id),
            flock,
            "and it loses nothing while its keeping is met"
        );
    }
}

/// **AND AN UNKEPT ONE REACHES ZERO ON THE TURN THE SHED ACTUALLY FIRES.**
///
/// This is the half that pins the countdown to the mechanic: it walks the real `advance_husbandry`
/// pass and requires the **first** turn the published remaining reads `0` to be the **first** turn
/// the flock actually loses animals. Any constant — including the honest-looking `grace + 1` the
/// first test asserts — fails here.
#[test]
fn an_unkept_herd_counts_down_to_the_turn_it_sheds() {
    let (mut app, id) = world_with_a_kept_pastoral_herd();
    let grace = shed_grace(&app);

    // Un-neglected, before any turn resolves: the whole grace plus the turn it bites on.
    assert_eq!(published_countdown(&mut app, &id), (true, grace + 1));

    let mut first_zero = None;
    let mut first_shed = None;
    for turn in 1..=(grace + 2) {
        let before = live_biomass(&app, &id);
        seat_keeping(&mut app, &id, NOBODY_KEEPING);
        app.world.run_system_once(advance_husbandry);

        let (has_grace, remaining) = published_countdown(&mut app, &id);
        assert!(has_grace, "an abandoned herd still has a flock at risk");
        if remaining == 0 && first_zero.is_none() {
            first_zero = Some(turn);
        }
        if live_biomass(&app, &id) < before && first_shed.is_none() {
            first_shed = Some(turn);
        }
    }

    assert_eq!(
        first_zero,
        Some(grace + 1),
        "the countdown reaches zero on the turn the shed starts, not before"
    );
    assert_eq!(
        first_shed, first_zero,
        "and that is the turn animals actually leave — the wire cannot drift from the gate"
    );
}
