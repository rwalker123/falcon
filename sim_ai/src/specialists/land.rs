//! **`Land`** — where the people are (`docs/plan_ai_driver.md` §4). Owns `patches_owned`; alarms
//! `LandShort` when a band's ground cannot feed it and nothing better is in view.
//!
//! - *blind* — fewer than `land.known_tiles_floor` known tiles within `land.horizon_tiles` of a
//!   band posts `land.scout_workers` scouts (`assign_labor … scout <n>`). **The `scout <x> <y>`
//!   verb is retired server-side** (`command.retired=ignored`, `core_sim/src/bin/server.rs`);
//!   scouting is the standing scout role, which posts vantage points around the band.
//! - *better ground* — a discovered, unowned patch within the horizon that would **pay a worker
//!   more** than the band's own ground does, while the runway is falling, proposes `move_band`
//!   with an intent that **persists until arrival**: the memory holds the target and the same
//!   intent is re-proposed each turn, which is what the commitment bonus rewards. The ranking is
//!   `per_worker_yield` — the crew's take, the quantity `Food` ranks sources on — and **not**
//!   `carrying_capacity`, which is the standing biomass the land holds; the two disagree, and a
//!   band that followed the biomass sat on the worst rate in its own neighbourhood and starved.
//! - *room* — under `Expand`, a band above `land.split_size` on ground the faction owns proposes
//!   `split_band` with half its workers.

use sim_runtime::{CommandPayload, ForagePatchState, PopulationCohortState};

use super::food::{crew_take, patch_per_worker_yield};
use super::{intent_key, Cost, Proposal, Proposals, Specialist, SpecialistId, SPECIALIST_LAND};
use crate::geometry::Tile;
use crate::orchestrator::{Alarm, AlarmKind, Plan, Stance};
use crate::profile::LandFloors;
use crate::view::{band_tile, SeatMemory, SeatView};

/// The standing scout role (`assign_labor <faction> <band> scout <workers>`).
pub const ROLE_SCOUT: &str = "scout";
pub const INTENT_SCOUT: &str = "scout";
pub const INTENT_MOVE: &str = "move";
pub const INTENT_SPLIT: &str = "split";
const REASON_BLIND: &str = "few known tiles";
const REASON_BETTER_GROUND: &str = "better ground in view";
const REASON_ROOM: &str = "room to split";
/// A split gives the new band this share of the parent's workers.
const SPLIT_SHARE_DIVISOR: u32 = 2;

pub struct Land {
    faction: u32,
    floors: LandFloors,
    /// The profile's `land_claim` weight: this specialist's scores are scaled by it.
    weight: f32,
}

impl Land {
    pub fn new(faction: u32, floors: LandFloors, weight: f32) -> Self {
        Self {
            faction,
            floors,
            weight,
        }
    }

    /// Whether a band of another faction stands on `tile`. **Ground under a rival is not better
    /// ground**: walking a band into a foreign camp is a contact, and the defection gate
    /// (`core_sim/tests/defection_contact_gate.rs`) can hand the whole band over.
    fn foreign_band_at(&self, view: &SeatView, tile: Tile) -> bool {
        view.snapshot
            .populations
            .iter()
            .any(|cohort| cohort.faction != self.faction && band_tile(cohort) == tile)
    }

    /// The best discovered, unowned, unoccupied patch within the horizon, and what it would pay a
    /// worker — the richest ground that beats a per-worker rate of `above`.
    ///
    /// ⛔ **Ground is ranked on what it pays a worker, never on its `carrying_capacity`.** The
    /// capacity is the stand's standing biomass `K` — what the land can *hold* — and the rate is
    /// what a crew *takes off it*; on the shipped bench seeds the tile carrying the most biomass in
    /// a neighbourhood paid the worst rate in it, by more than 2×, so a band ranking on capacity sat
    /// still and starved while three better tiles stood one step away. Capacity survives only as the
    /// tiebreak below, where it says the true thing it can say: between two tiles that pay a worker
    /// the same, the richer one sustains the bigger band.
    fn better_patch<'v>(
        &self,
        view: &'v SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        above: f32,
    ) -> Option<(&'v ForagePatchState, f32)> {
        let grid = view.grid();
        let here = band_tile(band);
        view.snapshot
            .forage_patches
            .iter()
            .filter(|patch| patch.owner.is_none())
            .filter(|patch| {
                let tile = Tile::new(patch.x, patch.y);
                tile != here
                    && view.is_discovered(tile)
                    && !self.foreign_band_at(view, tile)
                    && grid.distance(here, tile) <= self.floors.horizon_tiles
            })
            .map(|patch| (patch, patch_per_worker_yield(memory, band, patch)))
            .filter(|(_, per_worker)| *per_worker > above)
            .max_by(|(a, a_rate), (b, b_rate)| {
                a_rate
                    .total_cmp(b_rate)
                    .then_with(|| a.carrying_capacity.total_cmp(&b.carrying_capacity))
            })
    }

    /// What a worker would take off the ground the band stands on; nothing when it stands on no
    /// patch — bare ground pays a crew nothing whatever it carries.
    fn own_per_worker_yield(
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> f32 {
        view.patch_at(band_tile(band))
            .map_or(0.0, |patch| patch_per_worker_yield(memory, band, patch))
    }

    /// **What this band would harvest off the ground it stands on, per turn**: its whole working-age
    /// crew at that ground's per-worker rate, capped by what the stand hands over in a turn.
    fn harvest_here(view: &SeatView, memory: &SeatMemory, band: &PopulationCohortState) -> f32 {
        view.patch_at(band_tile(band)).map_or(0.0, |patch| {
            crew_take(
                band.working_age,
                patch_per_worker_yield(memory, band, patch),
                patch.biomass * patch.provisions_per_biomass,
            )
        })
    }

    fn scouts_posted(band: &PopulationCohortState) -> u32 {
        band.labor_assignments
            .iter()
            .filter(|row| row.kind == ROLE_SCOUT)
            .map(|row| row.workers)
            .sum()
    }

    /// *Blind*: too few known tiles around the band posts scouts, once.
    pub fn blind(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        let floor = self.floors.known_tiles_floor;
        let known = memory.known_tiles_within(
            view,
            band_tile(band),
            self.floors.horizon_tiles,
            view.tick(),
        ) as u32;
        if known >= floor || Self::scouts_posted(band) >= self.floors.scout_workers {
            return None;
        }
        Some(Proposal {
            commands: vec![CommandPayload::AssignLabor {
                faction_id: self.faction,
                band_id: Some(band.band_id),
                role: ROLE_SCOUT.to_owned(),
                workers: self.floors.scout_workers,
                target_x: None,
                target_y: None,
                fauna_id: None,
                policy: None,
                species: None,
                floor: None,
                kit_id: None,
                take_species: Vec::new(),
            }],
            intent: intent_key(SPECIALIST_LAND, INTENT_SCOUT, band.band_id),
            score: (floor - known) as f32 / floor as f32 * self.weight,
            cost: Cost {
                workers: self.floors.scout_workers,
                bands: vec![band.band_id],
            },
            reason: REASON_BLIND.to_owned(),
        })
    }

    /// *Better ground*: a richer patch in view while the runway falls — or the target the band is
    /// already walking to.
    pub fn better_ground(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        let own = Self::own_per_worker_yield(view, memory, band);
        let (target, target_rate) = match memory.move_target(band.band_id) {
            // Persist until arrival, while the ground is still worth it.
            Some(target) => {
                let patch = view
                    .patch_at(target)
                    .filter(|patch| patch.owner.is_none_or(|owner| owner == self.faction))
                    .filter(|_| !self.foreign_band_at(view, target))?;
                let rate = patch_per_worker_yield(memory, band, patch);
                (rate > own).then_some((patch, rate))?
            }
            None => {
                if !memory.runway_falling(band.band_id, band.turns_of_food) {
                    return None;
                }
                self.better_patch(view, memory, band, own)?
            }
        };
        Some(Proposal {
            commands: vec![CommandPayload::MoveBand {
                faction_id: self.faction,
                band_id: Some(band.band_id),
                target_x: target.x,
                target_y: target.y,
            }],
            intent: intent_key(SPECIALIST_LAND, INTENT_MOVE, band.band_id),
            score: (target_rate - own) / target_rate * self.weight,
            cost: Cost {
                workers: 0,
                bands: vec![band.band_id],
            },
            reason: format!("{REASON_BETTER_GROUND}: {},{}", target.x, target.y),
        })
    }

    /// *Room*: under `Expand`, a large band on owned ground splits.
    pub fn room(
        &self,
        view: &SeatView,
        plan: &Plan,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        if plan.stance != Stance::Expand || band.size <= self.floors.split_size || band.is_traveling
        {
            return None;
        }
        let owned = view
            .patch_at(band_tile(band))
            .is_some_and(|patch| patch.owner == Some(self.faction));
        if !owned {
            return None;
        }
        let workers = band.working_age / SPLIT_SHARE_DIVISOR;
        if workers == 0 {
            return None;
        }
        Some(Proposal {
            commands: vec![CommandPayload::SplitBand {
                faction_id: self.faction,
                band_id: Some(band.band_id),
                workers,
            }],
            intent: intent_key(SPECIALIST_LAND, INTENT_SPLIT, band.band_id),
            score: (band.size - self.floors.split_size) as f32 / band.size as f32 * self.weight,
            cost: Cost {
                workers,
                bands: vec![band.band_id],
            },
            reason: REASON_ROOM.to_owned(),
        })
    }

    /// The alarm: a band's ground cannot feed it and nothing better is in view.
    ///
    /// ⛔ **A rate against a rate.** What the band harvests here is provisions *per turn* and
    /// `food_consumption` is what it eats *per turn*; the stock this used to read —
    /// `carrying_capacity`, the standing biomass — is neither, and being two orders of magnitude
    /// larger than a band's appetite it meant the alarm could essentially never fire.
    pub fn alarm(&self, view: &SeatView, memory: &SeatMemory) -> Option<Alarm> {
        let short = view.own_bands(self.faction).any(|band| {
            let own = Self::own_per_worker_yield(view, memory, band);
            Self::harvest_here(view, memory, band) < band.food_consumption
                && self.better_patch(view, memory, band, own).is_none()
        });
        short.then_some(Alarm {
            specialist: SPECIALIST_LAND,
            kind: AlarmKind::LandShort,
            since_tick: view.tick(),
        })
    }
}

impl Specialist for Land {
    fn id(&self) -> SpecialistId {
        SPECIALIST_LAND
    }

    fn propose(&mut self, view: &SeatView, plan: &Plan, memory: &SeatMemory) -> Proposals {
        let mut out = Proposals {
            proposals: Vec::new(),
            alarm: self.alarm(view, memory),
        };
        for band in view.own_bands(self.faction) {
            out.proposals.extend(self.blind(view, memory, band));
            out.proposals.extend(self.better_ground(view, memory, band));
            out.proposals.extend(self.room(view, plan, band));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::Budget;
    use crate::profile::{AiProfiles, NO_MEMORY_DECAY};
    use crate::specialists::food::tests::{a_view, BAND, FACTION, HERE, TICK};
    use std::collections::{BTreeMap, BTreeSet};

    fn land(profile: &str) -> Land {
        let profile = AiProfiles::builtin().profile(profile).unwrap().clone();
        Land::new(FACTION, profile.land, profile.weight("land_claim"))
    }

    fn plan(stance: Stance) -> Plan {
        Plan {
            stance,
            budgets: BTreeMap::from([(SPECIALIST_LAND, Budget { worker_share: 1.0 })]),
            priorities: BTreeMap::from([(SPECIALIST_LAND, 1.0)]),
            since_turn: TICK,
        }
    }

    fn own_band(view: &SeatView) -> &PopulationCohortState {
        view.own_bands(FACTION).next().unwrap()
    }

    #[test]
    fn a_blind_band_posts_scouts_and_a_seeing_one_does_not() {
        let view = a_view();
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY);
        // Nothing observed yet: every tile is unknown.
        let proposal = land("forager")
            .blind(&view, &memory, own_band(&view))
            .expect("blind");
        assert_eq!(proposal.intent, "land:scout:7001");
        assert!(
            (proposal.score - 0.3).abs() < 1e-6,
            "full shortfall × land_claim"
        );
        assert!(
            matches!(&proposal.commands[0], CommandPayload::AssignLabor { role, workers: 1, .. } if role == ROLE_SCOUT)
        );
        // The whole raster is discovered: a radius-3 disk of 37 tiles is well past the floor of 12.
        memory.observe(&view, FACTION);
        assert!(land("forager")
            .blind(&view, &memory, own_band(&view))
            .is_none());
        // The rover's floor of 30 within radius 5 (91 tiles on a 48-tile map: 48 known) is met too.
        assert!(land("rover")
            .blind(&view, &memory, own_band(&view))
            .is_none());
    }

    #[test]
    fn scouts_already_posted_are_not_posted_again() {
        let mut view = a_view();
        view.snapshot.populations[0].labor_assignments = vec![sim_runtime::LaborAssignmentState {
            kind: ROLE_SCOUT.into(),
            workers: 1,
            ..Default::default()
        }];
        let memory = SeatMemory::new(NO_MEMORY_DECAY);
        assert!(land("forager")
            .blind(&view, &memory, own_band(&view))
            .is_none());
    }

    #[test]
    fn better_ground_needs_a_falling_runway_and_then_persists_until_arrival() {
        let view = a_view();
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY);
        memory.observe(&view, FACTION);
        let specialist = land("forager");
        assert!(
            specialist
                .better_ground(&view, &memory, own_band(&view))
                .is_none(),
            "no runway remembered, so it is not falling"
        );
        memory.remember_runways(&view, FACTION);
        let mut later = a_view();
        later.snapshot.populations[0].turns_of_food -= 1.0;
        let proposal = specialist
            .better_ground(&later, &memory, own_band(&later))
            .expect("a richer patch in view");
        assert_eq!(proposal.intent, "land:move:7001");
        assert_eq!(proposal.cost.bands, vec![BAND]);
        assert!(
            matches!(
                proposal.commands[0],
                CommandPayload::MoveBand {
                    target_x: 2,
                    target_y: 3,
                    ..
                }
            ),
            "the richest in reach: {:?}",
            proposal.commands[0]
        );
        // Accepted: the memory learns the target and the intent persists, runway or no runway.
        memory.record_choices(
            TICK,
            BTreeSet::from([proposal.intent.clone()]),
            proposal.commands.iter(),
        );
        let again = specialist
            .better_ground(&view, &memory, own_band(&view))
            .expect("persists");
        assert_eq!(again.intent, proposal.intent);
        assert_eq!(again.commands, proposal.commands);
        // Arrived: the target clears and nothing is proposed.
        let mut arrived = a_view();
        arrived.snapshot.populations[0].current_x = 2;
        arrived.snapshot.populations[0].current_y = 3;
        memory.observe(&arrived, FACTION);
        assert!(specialist
            .better_ground(&arrived, &memory, own_band(&arrived))
            .is_none());
    }

    #[test]
    fn room_splits_a_large_band_on_owned_ground_under_expand_only() {
        let mut view = a_view();
        let specialist = land("rover");
        let memory = SeatMemory::new(NO_MEMORY_DECAY);
        let _ = &memory;
        view.snapshot.populations[0].size = 30;
        view.snapshot.populations[0].working_age = 17;
        assert!(
            specialist
                .room(&view, &plan(Stance::Expand), own_band(&view))
                .is_none(),
            "standing on no owned patch"
        );
        view.snapshot.forage_patches.push(ForagePatchState {
            x: HERE.x,
            y: HERE.y,
            owner: Some(FACTION),
            carrying_capacity: 30.0,
            ..Default::default()
        });
        let proposal = specialist
            .room(&view, &plan(Stance::Expand), own_band(&view))
            .expect("room");
        assert_eq!(proposal.intent, "land:split:7001");
        assert!(matches!(
            proposal.commands[0],
            CommandPayload::SplitBand { workers: 8, .. }
        ));
        assert_eq!(proposal.cost.workers, 8);
        assert!(specialist
            .room(&view, &plan(Stance::Consolidate), own_band(&view))
            .is_none());
        assert!(
            land("forager")
                .room(&view, &plan(Stance::Expand), own_band(&view))
                .is_none(),
            "below the forager's split size of 40"
        );
    }

    #[test]
    fn the_alarm_is_ground_that_cannot_feed_the_band_with_nothing_better_in_view() {
        let mut view = a_view();
        let memory = SeatMemory::new(NO_MEMORY_DECAY);
        view.snapshot.populations[0].food_consumption = 100.0;
        assert!(
            land("forager").alarm(&view, &memory).is_none(),
            "the rich patch is better ground, so no alarm yet"
        );
        // A rival standing on the rich patch takes it off the table too.
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION + 1,
            band_id: BAND + 1,
            current_x: 2,
            current_y: 3,
            ..Default::default()
        });
        assert!(
            land("forager").alarm(&view, &memory).is_none(),
            "the near patch (1.0 a worker) still pays more than the bare ground here"
        );
        for patch in &mut view.snapshot.forage_patches {
            patch.owner = Some(FACTION + 1);
        }
        assert_eq!(
            land("forager")
                .alarm(&view, &memory)
                .map(|alarm| alarm.kind),
            Some(AlarmKind::LandShort)
        );
    }

    /// ⛔ **The alarm compares a rate to a rate.** Against `carrying_capacity` — a standing biomass
    /// two orders of magnitude larger than a band's appetite — it could essentially never fire.
    #[test]
    fn the_alarm_weighs_what_the_crew_harvests_per_turn_not_the_biomass_standing_here() {
        let mut view = a_view();
        let memory = SeatMemory::new(NO_MEMORY_DECAY);
        // The band's own ground: a great deal of biomass standing, paying a worker almost nothing.
        // Nothing else is on the table, so only this ground answers the question.
        view.snapshot.forage_patches = vec![ForagePatchState {
            x: HERE.x,
            y: HERE.y,
            owner: None,
            per_worker_yield: 0.05,
            carrying_capacity: 195.0,
            biomass: 292.5,
            provisions_per_biomass: 1.0,
            ..Default::default()
        }];
        let band = &mut view.snapshot.populations[0];
        band.working_age = 17;
        band.food_consumption = 4.0;
        assert_eq!(
            land("forager")
                .alarm(&view, &memory)
                .map(|alarm| alarm.kind),
            Some(AlarmKind::LandShort),
            "17 hands × 0.05 = 0.85 a turn against 4.0 eaten a turn"
        );
        // The same 195 of standing biomass, now paying a worker enough to feed the band.
        view.snapshot.forage_patches[0].per_worker_yield = 1.0;
        assert!(
            land("forager").alarm(&view, &memory).is_none(),
            "17 hands × 1.0 covers the 4.0 eaten, and the biomass never moved"
        );
    }

    /// ⛔ **Better ground is the ground that pays a worker more, not the ground carrying the most
    /// biomass.** On the bench's seed 23 the band's own tile held the most `carrying_capacity` in
    /// its neighbourhood and the worst `per_worker_yield` in it — so ranking on capacity found
    /// nothing better than where it stood, and the band sat there and starved.
    #[test]
    fn better_ground_follows_the_per_worker_yield_and_not_the_carrying_capacity() {
        let mut view = a_view();
        // Here: the richest stand in reach, and the worst rate in it.
        view.snapshot.forage_patches.push(ForagePatchState {
            x: HERE.x,
            y: HERE.y,
            owner: None,
            per_worker_yield: 0.25,
            carrying_capacity: 195.0,
            biomass: 292.5,
            provisions_per_biomass: 1.0,
            ..Default::default()
        });
        // One step away: less biomass standing, more of it reaching a worker.
        let runner_up_stand = Tile::new(4, 2);
        let best_rate = Tile::new(2, 3);
        for patch in &mut view.snapshot.forage_patches {
            let tile = Tile::new(patch.x, patch.y);
            if tile == runner_up_stand {
                patch.per_worker_yield = 0.55;
                patch.carrying_capacity = 150.0;
            } else if tile == best_rate {
                patch.per_worker_yield = 0.56;
                patch.carrying_capacity = 70.0;
            }
        }
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY);
        memory.observe(&view, FACTION);
        memory.remember_runways(&view, FACTION);
        view.snapshot.populations[0].turns_of_food -= 1.0;
        let proposal = land("forager")
            .better_ground(&view, &memory, own_band(&view))
            .expect("two tiles in reach pay a worker more than this one does");
        assert!(
            matches!(
                proposal.commands[0],
                CommandPayload::MoveBand { target_x, target_y, .. }
                    if Tile::new(target_x, target_y) == best_rate
            ),
            "the best rate, not the most biomass: {:?}",
            proposal.commands[0]
        );
        // The same ranking with the two best rates tied: the tiebreak is what the land holds.
        for patch in &mut view.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == best_rate {
                patch.per_worker_yield = 0.55;
            }
        }
        let proposal = land("forager")
            .better_ground(&view, &memory, own_band(&view))
            .expect("still better ground");
        assert!(
            matches!(
                proposal.commands[0],
                CommandPayload::MoveBand { target_x, target_y, .. }
                    if Tile::new(target_x, target_y) == runner_up_stand
            ),
            "tied on rate, the richer stand sustains the bigger band: {:?}",
            proposal.commands[0]
        );
    }
}
