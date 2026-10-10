//! **HUNTING BY NEED** (`docs/plan_roaming_bands.md` §Hunting by need, #798): a band in migration
//! mode hunts when it needs meat, not at a fixed rate.
//!
//! A hunt row with `move_with_herd` is a *need row*. It holds no standing workers; each turn the
//! labor pass decides whether the band sends a crew to it NEXT turn ([`assess_need`]), mustered from
//! the band's own hands for that one turn ([`crate::LaborAllocation::muster_donors`]) and handed
//! back at the end of the pass. The same assessment is what the snapshot capture publishes from, so
//! the board and the turn cannot disagree.

use crate::fauna::{
    hunt_crew_take_curve, hunt_crew_turns_to_kill, hunt_useful_crew, HuntCrewCurveInputs,
    NO_USEFUL_CREW,
};
use crate::snapshot::NOT_FOOD_LIMITED_TURNS;

/// The wire's `turnsUntilHunt` when a crew will not be sent however long the band waits: the larder
/// is not food-limited, or no crew can be raised. `u32::MAX` rather than `0` because `0` already
/// means *a crew goes out now*.
pub const NO_HUNT_NEEDED: u32 = u32::MAX;

/// The least `turnsUntilHunt` a waiting row publishes: it is waiting, so the crew is at least a turn
/// away.
const MIN_TURNS_UNTIL_HUNT: u32 = 1;

/// **IS THIS SOURCE WITHIN THE BAND'S WORK RANGE?** — the geometric test a donor row must pass, on
/// the one threshold ([`crate::work_party::party_begins_past`]) `post_a_party` posts a party past. A
/// source the band cannot place (a role row, a herd gone, no band position) is not local.
/// `grid` is `(width, wrap_horizontal)`.
pub(crate) fn source_is_local(
    target: &crate::LaborTarget,
    herds: &crate::HerdRegistry,
    band_pos: Option<bevy::math::UVec2>,
    labor: &crate::LaborConfig,
    grid: (u32, bool),
) -> bool {
    let (Some(band), Some(source)) = (
        band_pos,
        crate::systems::party_source_position(target, herds),
    ) else {
        return false;
    };
    crate::grid_utils::hex_distance_wrapped(band, source, grid.0, grid.1)
        <= crate::work_party::party_begins_past(labor)
}

/// What the band decides about one need row for next turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeedAssessment {
    /// The crew [`hunt_useful_crew`] prices over the musterable pool; `0` when none can be raised.
    pub crew: u32,
    /// Turns until that crew brings an animal down, counting the herd's current wounds; `0` when no
    /// crew can be raised.
    pub turns_to_kill: u32,
    /// Whether the band sends the crew next turn.
    pub send: bool,
    /// On a waiting row, turns until a crew goes out if nothing changes ([`NO_HUNT_NEEDED`] when it
    /// never will); `0` on a row that is sending.
    pub turns_until_hunt: u32,
}

impl NeedAssessment {
    /// No crew can be raised: nothing is sent and nothing is waited for.
    const NO_CREW: Self = Self {
        crew: NO_USEFUL_CREW,
        turns_to_kill: 0,
        send: false,
        turns_until_hunt: NO_HUNT_NEEDED,
    };
}

/// **THE NEED DECISION FOR ONE ROW** — `inputs` is the hunt crew curve's own input bundle with
/// `max_workers` set to the musterable pool; `runway` is the band's food runway, the very value the
/// snapshot publishes as `turnsOfFood`; `margin` is `follow.need_margin_turns`.
///
/// * a crew of [`hunt_useful_crew`] of the curve is what would go out; none → nothing is sent;
/// * an animal **already partly down** is finished (`finishing`, below);
/// * otherwise a crew goes out iff `runway <= turns_to_kill + margin`, and a runway at the
///   [`NOT_FOOD_LIMITED_TURNS`] sentinel never triggers it.
///
/// # ⛔ `finishing` IS "THE CREW WAS OUT THIS TURN, THE HERD HAS WOUNDS PENDING, AND NO KILL LANDED"
///
/// It is not simply `wounds.pending() > 0`. A crew that kills a mammoth deals more damage than the
/// body holds, and the excess banks toward the NEXT animal (`DamageLedger::strike`), so a herd is
/// left with a small `pending` after almost every kill. Read as "the animal is partly down" that
/// remainder would send a full crew after the next mammoth the turn after each kill, whatever the
/// larder said, for ever. The rule that means what it says is *a hunt in progress*: the band sent a
/// crew this turn, the animal took damage, and it is not down yet. The caller states that fact; this
/// function only obeys it.
pub fn assess_need(
    inputs: &HuntCrewCurveInputs<'_>,
    horizon: u32,
    runway: f32,
    margin: f32,
    finishing: bool,
) -> NeedAssessment {
    let crew = hunt_useful_crew(&hunt_crew_take_curve(inputs));
    if crew == NO_USEFUL_CREW {
        return NeedAssessment::NO_CREW;
    }
    let turns_to_kill = hunt_crew_turns_to_kill(inputs, crew, horizon);
    let reported_turns = turns_to_kill.unwrap_or(0);
    if finishing {
        return NeedAssessment {
            crew,
            turns_to_kill: reported_turns,
            send: true,
            turns_until_hunt: 0,
        };
    }
    let food_limited = runway < NOT_FOOD_LIMITED_TURNS;
    let Some(turns) = turns_to_kill.filter(|_| food_limited) else {
        return NeedAssessment {
            crew,
            turns_to_kill: reported_turns,
            send: false,
            turns_until_hunt: NO_HUNT_NEEDED,
        };
    };
    let slack = runway - turns as f32 - margin;
    if slack <= 0.0 {
        return NeedAssessment {
            crew,
            turns_to_kill: turns,
            send: true,
            turns_until_hunt: 0,
        };
    }
    NeedAssessment {
        crew,
        turns_to_kill: turns,
        send: false,
        turns_until_hunt: (slack.ceil() as u32).max(MIN_TURNS_UNTIL_HUNT),
    }
}
