//! **THE WORK PARTY** — what a Hunt or Forage row becomes once its source drifts past the distance
//! the band's own hands reach (`docs/plan_civilization_steps.md` §One work party).
//!
//! # A PARTY IS NOT AN ENTITY
//!
//! It is state on the labor assignment that staffed it. An expedition is a *detached* cohort with
//! its own component and no `ResidentBand`; a work party is the opposite by design — *the workers
//! are still the band's, they are just somewhere else* — so there is no split, no move order, no
//! follow order and no merge. The sim stands the party **at the source**: a Hunt party is wherever
//! the herd is this turn, a Forage party stands on its patch. The row that staffed it is the row
//! that reports it.
//!
//! # ⛔ THE LOCAL IDENTITY IS THE WHOLE POINT
//!
//! A source the band's hands already reach acquires **no party at all**, and every number on that
//! row is what it was before this module existed. Far work falls out of the same model rather than
//! sitting beside it, and that is only true while the zero-distance case is bit-for-bit unchanged —
//! see `systems::labor::tests::a_local_forage_row_is_unchanged_by_the_work_party`.
//!
//! # WHAT DISTANCE COSTS
//!
//! Three things, each with its own seam here:
//!
//! 1. **Porters, out of the party itself** ([`porters`]). Beyond the reach a link holds itself open
//!    at, somebody has to walk the goods, and that somebody is one of the workers. The share rises
//!    with distance until the whole party is carrying and produces nothing — *a range cap nobody
//!    had to pick a number for*.
//! 2. **Friction on what comes home** ([`arriving_fraction`]) — the supply network's own
//!    `friction`, extended over the tiles the flow crosses unaided, rather than a second loss term
//!    minted for this path.
//! 3. **A transit delay** ([`transit_turns`]) — the first goods arrive after the party has walked
//!    out, and steadily after that. A pipeline, not a trip: there is no haul command, no chosen
//!    destination and no launch gate anywhere in this module.
//!
//! **Roads cut the first two to nothing.** The free reach is widened by whatever road runs between
//! the band and the source (`supply::free_pooling_reach_tiles`), and automatic pooling is the only
//! road traffic there is — so a posting worked steadily wears a trail that eventually covers the
//! distance, at which point the porters go back to producing and the link pools for free. The
//! distance bite and its escape hatch are one mechanism.

use bevy::math::UVec2;
use serde::{Deserialize, Serialize};

/// **A source the band's own hands reach** — no travel, and therefore no party. The value the
/// local identity is asserted against.
pub const NO_TRAVEL: u32 = 0;

/// **Nothing is lost on the way home** — the retained fraction of a flow that crosses no unaided
/// tiles. The local case, and what a road-covered link is restored to.
pub const EVERYTHING_ARRIVES: f32 = 1.0;

/// An empty pack: the party is carrying nothing home.
pub const NOTHING_IN_THE_PACK: f32 = 0.0;

/// Nobody is carrying rather than working.
pub const NO_PORTERS: u32 = 0;

/// The party's upkeep is fully covered — nothing has to be carried out to it.
pub const NO_DEFICIT: f32 = 0.0;

/// **WHERE A BAND'S WORKERS ARE STANDING WHEN THEY ARE NOT STANDING WITH THE BAND** — one per
/// far Hunt/Forage row, stamped fresh every turn on the row that staffed it.
///
/// Every field but [`Self::pack_food`] and [`Self::turns_to_first_arrival`] is **restamped each
/// turn** from the source's live position, which is why a Hunt party follows its herd without a
/// follow order. The two exceptions are the pipeline's own state: the party walks out once, and
/// what it has gathered on the way rides in its pack until the line opens.
///
/// It is **outside [`crate::components::LaborAssignment`]'s equality** for `last_yields`' reason:
/// where the workers are standing is a fact about the world, not about the order the player gave,
/// and a rollback record or a command no-op guard that compared it would report *nothing changed*
/// as a change every turn the herd moved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkParty {
    /// **The tile the workers stand on** — the source's own position, re-read every turn. There is
    /// no pathfinding: the party's position *is* the source's.
    pub position: UVec2,
    /// Every hand this posting holds — the row's `workers`, carrying and working alike.
    pub workers: u32,
    /// Of those, the ones walking goods instead of working the source ([`porters`]).
    pub porters: u32,
    /// **The modelled distance, measured to the APRON rather than to the hex**
    /// ([`travel_tiles`]) — what the readout states and what the transit delay is taken on.
    pub travel_tiles: u32,
    /// The tiles the flow crosses with nobody but the porters holding it open ([`porter_tiles`]) —
    /// what the porter share and the friction are both charged on. `0` once a road covers the run.
    pub porter_tiles: u32,
    /// How long the walk out takes ([`transit_turns`]). Reported so the row can say when its line
    /// opens; the countdown itself is [`Self::turns_to_first_arrival`].
    pub transit_turns: u32,
    /// **Turns left before the first goods land.** Set to [`Self::transit_turns`] when the party
    /// forms, counted down once per turn, and `0` for the whole steady life of the posting after
    /// that. It is never re-raised: the party walks out once, and a herd drifting further afterward
    /// costs porters and friction rather than a second walk.
    pub turns_to_first_arrival: u32,
    /// **What the party is carrying and has not delivered yet**, in food. It accumulates while
    /// [`Self::turns_to_first_arrival`] counts down and is delivered whole on the turn the line
    /// opens — so nothing produced on the walk out is lost, and a posting that ends early walks
    /// home with its pack ([`WorkParty::hand_over_pack`]).
    pub pack_food: f32,
    /// **What the party ate out of its own take this turn** — `min(take, upkeep)`.
    ///
    /// ⛔ **It is not a second meal.** These are the band's own people and the band's population
    /// consumption already feeds them wherever they stand; what this records is that the food was
    /// eaten *at the source*, so it never had to be carried and pays no friction. Charging it again
    /// here would bill the band twice for the same mouths.
    pub ate: f32,
    /// **What the party's upkeep still wants after its own take** — the food the band has to carry
    /// out to it. `0` on a posting that feeds itself; the whole of the upkeep for a posting whose
    /// take is not edible (fibre, stone, wood), which is the case the rule exists to produce
    /// without a per-job exemption anywhere.
    pub deficit: f32,
    /// **THE STEADY PER-TURN RATE ARRIVING AT THE HOME BAND** — the amortized number the work row
    /// prints, so a near row and a far row are comparable figures on one board. Amortized-over-the-
    /// cycle and steady-state coincide here deliberately: there is one number, not two.
    pub net_rate_home: f32,
}

impl WorkParty {
    /// **Open a posting at `position`.** The pipeline starts empty and the walk out starts now.
    pub fn walking_out(position: UVec2, transit_turns: u32) -> Self {
        Self {
            position,
            transit_turns,
            turns_to_first_arrival: transit_turns,
            ..Self::default()
        }
    }

    /// Has the walk out finished — is the line open and flowing?
    pub fn line_is_open(&self) -> bool {
        self.turns_to_first_arrival == NO_TRAVEL
    }

    /// **Everything the party is carrying, handed over and the pack emptied** — what the fold-back
    /// settles into the band, and what lands on the turn the line opens.
    pub fn hand_over_pack(&mut self) -> f32 {
        std::mem::replace(&mut self.pack_food, NOTHING_IN_THE_PACK)
    }
}

/// **THE MODELLED DISTANCE, MEASURED TO THE APRON** — `max(0, hex_distance − band_work_range)`.
///
/// A source inside the band's own work range costs no travel at all, which is what makes today's
/// local hunt and forage *fall out of* the one model instead of sitting beside it. Distance is in
/// hex steps ([`crate::grid_utils::hex_distance_wrapped`]), like every other radius in the sim.
pub fn travel_tiles(distance: u32, band_work_range: u32) -> u32 {
    distance.saturating_sub(band_work_range)
}

/// **THE TILES SOMEBODY HAS TO WALK** — `max(0, hex_distance − free_reach)`, where `free_reach` is
/// the supply network's `reach_tiles` **widened by whatever road runs between the endpoints**
/// (`supply::free_pooling_reach_tiles`).
///
/// ⛔ **This, not [`travel_tiles`], is what porters and friction are charged on**, and the
/// difference is the promotion path the design turns on: *within reach the link is free — that is
/// what `reach_tiles` means*. A trail worn by the posting's own traffic widens that reach until it
/// covers the run, at which point this is `0`, the porters go back to producing and the goods
/// arrive whole. The travel distance is unchanged by the road; what the road removes is the cost.
pub fn porter_tiles(distance: u32, free_reach: u32) -> u32 {
    distance.saturating_sub(free_reach)
}

/// **HOW MANY OF THE PARTY ARE CARRYING RATHER THAN WORKING** —
/// `round(workers × fraction_per_tile × porter_tiles)`, clamped to the party.
///
/// Distance is paid in **workers, out of the party itself**: a distant node needs people to walk
/// the goods and there is nobody else to be. At some range the whole party is carrying and produces
/// nothing, which is a range cap nobody had to pick a number for — the clamp, not a lever.
pub fn porters(workers: u32, porter_tiles: u32, fraction_per_tile: f32) -> u32 {
    if porter_tiles == NO_TRAVEL || workers == NO_PORTERS {
        return NO_PORTERS;
    }
    let share = (fraction_per_tile * porter_tiles as f32).clamp(0.0, EVERYTHING_ARRIVES);
    (workers as f32 * share).round().min(workers as f32) as u32
}

/// **HOW LONG THE WALK OUT TAKES** — `ceil(travel_tiles / tiles_per_turn)`, in turns.
///
/// Taken on the *travel* distance rather than the porter distance because it is the party's own
/// walk, not the goods' crossing: a road that widens the free reach has not moved the source.
/// `0` tiles per turn cannot happen (`labor_config` validates it positive), but is treated as
/// *arrives the same turn* rather than dividing by zero.
pub fn transit_turns(travel_tiles: u32, tiles_per_turn: u32) -> u32 {
    if travel_tiles == NO_TRAVEL || tiles_per_turn == 0 {
        return NO_TRAVEL;
    }
    travel_tiles.div_ceil(tiles_per_turn)
}

/// **WHAT SURVIVES THE JOURNEY HOME** — the supply network's own `friction` compounded over every
/// tile the flow crosses unaided, `(1 − friction)^porter_tiles`.
///
/// It is deliberately the **network's** loss term extended over distance rather than a second one
/// minted for this path: goods moving between a band and its party are the same fact as goods
/// moving between two bands, and two independent leak rates would drift. `1.0` inside the free
/// reach, which is the local identity.
pub fn arriving_fraction(porter_tiles: u32, friction: f32) -> f32 {
    if porter_tiles == NO_TRAVEL {
        return EVERYTHING_ARRIVES;
    }
    (EVERYTHING_ARRIVES - friction.clamp(0.0, EVERYTHING_ARRIVES)).powi(porter_tiles as i32)
}

/// **THE PARTY'S OWN UPKEEP** — `workers × the per-worker draw population.rs already charges`.
/// There is deliberately no second food rate for a party: it is the same people eating the same
/// amount, somewhere else.
pub fn party_upkeep(workers: u32, per_worker_draw: f32) -> f32 {
    workers as f32 * per_worker_draw
}

/// **WHO EATS, AND WHICH WAY THE FOOD FLOWS** — one settlement of one turn's take, and the whole of
/// the rule: *the take feeds the party first; the remainder is surplus, and the shortfall is a
/// deficit the supply line covers.*
///
/// It asks nothing about the job. A hunt party's take happens to be edible and a stone party's does
/// not; hunting is not privileged, and a branch on target kind here would be the design violated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartyFlow {
    /// What the party's mouths want this turn ([`party_upkeep`]).
    pub upkeep: f32,
    /// What survives the trip home ([`arriving_fraction`]).
    pub arriving: f32,
}

/// One turn's take, split into who got what.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoodSettlement {
    /// Eaten at the source, out of the take.
    pub ate: f32,
    /// **What the home band is credited** — always the home band, never whichever band the party
    /// happens to be standing next to.
    pub home: f32,
    /// Upkeep the take did not cover.
    pub deficit: f32,
}

impl PartyFlow {
    /// Settle `produced` food taken at the source.
    ///
    /// ⛔ **THE EATEN SHARE IS CREDITED HOME**, and that is the term to read twice. The band's
    /// population consumption already feeds these workers — they never left the cohort — so food
    /// they ate *at the source* is food the band did not have to carry out, not food that vanished.
    /// Netting it out here as well would charge the band twice for one meal and make a far posting
    /// look like a loss it is not. What eating on the spot actually buys is the friction on that
    /// share, which is why only the **surplus** is multiplied down.
    pub fn settle_food(&self, produced: f32) -> FoodSettlement {
        let produced = produced.max(0.0);
        let ate = produced.min(self.upkeep.max(0.0));
        let surplus = produced - ate;
        FoodSettlement {
            ate,
            home: ate + surplus * self.arriving,
            deficit: (self.upkeep - ate).max(NO_DEFICIT),
        }
    }

    /// **WHAT THE BAND MUST HAVE IN ITS LARDER TO HOLD THIS POSTING OPEN** — the deficit grossed up
    /// by the friction the outbound leg loses, because goods flow **both ways** along the tie and
    /// the outbound leg is no cheaper than the inbound one.
    ///
    /// A party whose band cannot cover this walks home ([`crate::systems::labor`]'s fold-back): the
    /// cost of misjudging a distance is the posting ending and the food already spent, not people
    /// dying off-screen.
    pub fn larder_needed_to_supply(&self, deficit: f32) -> f32 {
        if deficit <= NO_DEFICIT {
            return NO_DEFICIT;
        }
        if self.arriving <= 0.0 {
            return f32::INFINITY;
        }
        deficit / self.arriving
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The apron measurement: inside the band's work range there is no travel at all, and past it
    /// the distance is measured from the apron rather than from the band's own hex.
    #[test]
    fn travel_is_measured_to_the_apron() {
        assert_eq!(travel_tiles(0, 2), NO_TRAVEL);
        assert_eq!(travel_tiles(2, 2), NO_TRAVEL);
        assert_eq!(travel_tiles(3, 2), 1);
        assert_eq!(travel_tiles(9, 2), 7);
    }

    /// Porters are charged on the tiles beyond the **free reach**, so a run a road covers costs
    /// none at all — the promotion path, asserted.
    #[test]
    fn a_road_that_covers_the_run_takes_the_porters_to_zero() {
        let workers = 6;
        let fraction = 0.12;
        // Six tiles out, free reach 3: three tiles walked unaided.
        assert_eq!(porters(workers, porter_tiles(6, 3), fraction), 2);
        // The same six tiles, with a road holding the link open the whole way.
        assert_eq!(porters(workers, porter_tiles(6, 6), fraction), NO_PORTERS);
    }

    /// The range cap: far enough out, every hand is carrying and the posting produces nothing. It
    /// falls out of the clamp rather than being a picked number.
    #[test]
    fn distance_eventually_takes_the_whole_party() {
        let workers = 8;
        let fraction = 0.12;
        assert_eq!(porters(workers, porter_tiles(12, 3), fraction), 8);
    }

    #[test]
    fn the_walk_out_rounds_up_and_is_free_in_range() {
        assert_eq!(transit_turns(NO_TRAVEL, 1), NO_TRAVEL);
        assert_eq!(transit_turns(4, 1), 4);
        assert_eq!(transit_turns(5, 2), 3);
    }

    #[test]
    fn friction_compounds_per_unaided_tile_and_is_free_in_reach() {
        assert_eq!(arriving_fraction(NO_TRAVEL, 0.05), EVERYTHING_ARRIVES);
        let two = arriving_fraction(2, 0.05);
        assert!((two - 0.9025).abs() < 1e-5, "got {two}");
    }

    /// A food take covers the party and the surplus goes home; the eaten share pays no friction
    /// because it never travelled.
    #[test]
    fn a_food_take_feeds_the_party_and_the_surplus_flows_home() {
        let flow = PartyFlow {
            upkeep: 2.0,
            arriving: 0.5,
        };
        let settled = flow.settle_food(10.0);
        assert_eq!(settled.ate, 2.0);
        assert_eq!(settled.deficit, NO_DEFICIT);
        assert_eq!(settled.home, 2.0 + 8.0 * 0.5);
    }

    /// No per-job exemption: a take of nothing edible runs the full deficit, and the food has to
    /// come out to it.
    #[test]
    fn a_take_that_is_not_food_runs_the_full_deficit() {
        let flow = PartyFlow {
            upkeep: 3.0,
            arriving: 0.5,
        };
        let settled = flow.settle_food(0.0);
        assert_eq!(settled.ate, 0.0);
        assert_eq!(settled.home, 0.0);
        assert_eq!(settled.deficit, 3.0);
        // Both ways along the tie: the outbound leg loses the same friction the inbound one does.
        assert_eq!(flow.larder_needed_to_supply(settled.deficit), 6.0);
    }

    /// ⛔ The identity this whole module is built around: at zero distance nothing is taken, nothing
    /// is lost, and the take arrives home whole on the turn it was made.
    #[test]
    fn the_local_case_is_the_identity() {
        let flow = PartyFlow {
            upkeep: 4.0,
            arriving: arriving_fraction(porter_tiles(2, 3), 0.05),
        };
        assert_eq!(flow.arriving, EVERYTHING_ARRIVES);
        assert_eq!(flow.settle_food(9.0).home, 9.0);
        assert_eq!(porters(5, porter_tiles(2, 3), 0.12), NO_PORTERS);
        assert_eq!(transit_turns(travel_tiles(2, 2), 1), NO_TRAVEL);
    }
}
