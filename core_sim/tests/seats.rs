//! **Seats against a real world's roster and a real turn queue.**
//!
//! `core_sim::seats`'s own unit tests drive the registry and the gate against hand-built rosters,
//! which is where the refusal rules belong. What they cannot state is the part that only a built world
//! has: that a seat **is** a faction of `FactionRegistry`, and that the wait set the gate reasons about
//! is the one `TurnQueue` actually produces after a submission. Both arms of that comparison come from
//! `faction_support`, the repo's two-faction fixture.
//!
//! See `.claude/rules/core_sim/factions.md` → "Seats: a connection claims the faction it drives".

mod faction_support;

use std::time::{Duration, Instant};

use core_sim::{
    ConnectionId, FactionOrders, FactionRegistry, SeatClaimRefusal, SeatRegistry, SeatTurnGate,
    SeatTurnLimits, TurnQueue, TurnWait,
};
use faction_support::{one_faction_world, two_faction_world, world_with, HOME, ONE_RIVAL, RIVAL};

/// The two client processes of a two-seat game.
const HOME_CLIENT: ConnectionId = ConnectionId(1);
const RIVAL_CLIENT: ConnectionId = ConnectionId(2);

/// Short enough to run out inside a test; the shipped value is
/// `simulation_config.json`'s `seat_turn_timeout_seconds`.
fn limits() -> SeatTurnLimits {
    SeatTurnLimits {
        submission_timeout: Duration::from_millis(50),
    }
}

fn roster(app: &bevy::prelude::App) -> Vec<core_sim::FactionId> {
    app.world.resource::<FactionRegistry>().factions().to_vec()
}

/// ⛔ **A SEAT IS A FACTION OF THIS WORLD'S ROSTER — not of the request.**
///
/// The control arm is what makes it an assertion: the *same* claim that is granted in the two-faction
/// world is refused in the one-faction world, so the roster is doing the deciding rather than the id
/// merely looking plausible.
#[test]
fn a_seat_is_a_faction_of_the_worlds_own_roster() {
    let two = two_faction_world();
    let mut seats = SeatRegistry::default();
    assert!(
        seats.claim(RIVAL, RIVAL_CLIENT, &roster(&two)).is_ok(),
        "faction 1 is a seat of the two-faction world"
    );

    let one = one_faction_world();
    let mut seats = SeatRegistry::default();
    assert_eq!(
        seats.claim(RIVAL, RIVAL_CLIENT, &roster(&one)),
        Err(SeatClaimRefusal::UnknownSeat),
        "the single-faction world seats nobody at faction 1, so there is no seat to claim"
    );
    assert!(
        seats.claim(HOME, RIVAL_CLIENT, &roster(&one)).is_ok(),
        "faction 0 is, so the same client is seated there"
    );
}

/// ⛔ **THE WAIT SET IS THE TURN QUEUE'S OWN, AND A VACANT SEAT IS NOT IN IT.**
///
/// `TurnQueue` awaits every registered faction, control-blind — that is deliberate and unchanged. What
/// the seat gate adds is *which of those the loop waits for*: with the rival seat vacant, the home
/// seat's submission is the whole turn.
#[test]
fn a_vacant_rival_is_not_waited_for_and_an_occupied_one_is() {
    let mut app = world_with(ONE_RIVAL, |_| {});
    app.world
        .resource_mut::<TurnQueue>()
        .submit_orders(HOME, FactionOrders::end_turn())
        .expect("the home seat submits");
    let awaiting = app.world.resource::<TurnQueue>().awaiting();
    assert_eq!(
        awaiting,
        vec![RIVAL],
        "the queue still awaits the rival, whoever or nobody is driving it"
    );

    let mut vacant_rival = SeatRegistry::default();
    vacant_rival
        .claim(HOME, HOME_CLIENT, &roster(&app))
        .expect("the human sits down");
    let mut gate = SeatTurnGate::default();
    assert_eq!(
        gate.assess(&awaiting, &vacant_rival, Instant::now(), limits()),
        TurnWait::Resolve,
        "nobody is sitting at the rival seat, so nothing is left to wait for"
    );
    assert_eq!(gate.deadline(), None);

    let mut occupied_rival = SeatRegistry::default();
    occupied_rival
        .claim(HOME, HOME_CLIENT, &roster(&app))
        .expect("the human sits down");
    occupied_rival
        .claim(RIVAL, RIVAL_CLIENT, &roster(&app))
        .expect("and so does the rival's client");
    let mut gate = SeatTurnGate::default();
    assert!(
        matches!(
            gate.assess(&awaiting, &occupied_rival, Instant::now(), limits()),
            TurnWait::Wait { ref silent_seats, .. } if silent_seats == &[RIVAL]
        ),
        "with the seat occupied the same queue state is a wait"
    );
    assert!(gate.deadline().is_some());
}
