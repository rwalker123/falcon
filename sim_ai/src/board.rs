//! **The demand board** — how a specialist asks for a thing another part of the brain supplies
//! (`docs/plan_ai_driver.md` §4, *"The demand board — specialists never talk to each other"*).
//!
//! A specialist posts a [`Demand`] in the loadout vocabulary — a kit by roster id, a material by
//! id — and never sees the board again: the orchestrator reads the open demands and resolves them
//! against the window's budgets ([`crate::orchestrator::Orchestrator::outfit`]), the composite
//! emits the loadout, and the board records what became of every post. The star has the
//! orchestrator at its centre; a specialist that needs another posts here and reads its plan
//! slice for what it was granted, so the web of relationships stays testable one node at a time.
//!
//! **The first customer is outfitting**, and a loadout demand expires with its window: the sim
//! opens a band's window for exactly one frame (`close_opening_window` clears every window on the
//! turn advance and nothing re-opens one), so a demand posted at tick `t` is for the window at
//! `t` and nothing later. Every transition — `posted`, `planned`, `fulfilled`, `expired` — is a
//! [`DemandRecord`] on the decision log, which is what the bench's fulfilment rate and latency
//! read.

use std::fmt;

use sim_runtime::{BandKitTiersState, PopulationCohortState};

use crate::instruments::decisions::{DecisionRecord, DecisionSink, DemandRecord};
use crate::specialists::SpecialistId;
use crate::view::SeatView;

/// **The resource vocabulary is the loadout's**: a kit by `equipment.json` roster id, a material
/// by `materials.json` id — the same words `set_starting_loadout` takes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Resource {
    Kit(String),
    /// No shipped specialist posts one yet — `Build` will, for the material a rung swallows
    /// (`plan_ai_driver.md` §4) — so the orchestrator's pre-fill is the only material line a
    /// loadout carries today; the variant is in the vocabulary because the wire's is.
    #[allow(dead_code)]
    Material(String),
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Resource::Kit(id) => write!(f, "kit:{id}"),
            Resource::Material(id) => write!(f, "material:{id}"),
        }
    }
}

/// What a specialist asks the board for.
#[derive(Debug, Clone, PartialEq)]
pub struct Demand {
    pub requester: SpecialistId,
    /// The band whose window this is for.
    pub band: u64,
    pub resource: Resource,
    pub amount: u32,
    /// The window's tick — a loadout demand expires with its window.
    pub by_tick: u64,
    /// The requester's own view of how much this matters; the orchestrator weighs it by the
    /// requester's domain weight.
    pub priority: f32,
}

/// Where a demand stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandState {
    Posted,
    /// The orchestrator granted this much of it and the composite sent the loadout.
    Planned {
        granted: u32,
    },
    /// The frame after the grant carries it.
    Fulfilled {
        granted: u32,
    },
    /// Past its window, or the sim refused the loadout (the failed-command log says why).
    Expired,
}

/// The state tokens a [`DemandRecord`] carries — what the bench's board measures group on.
pub const DEMAND_STATE_POSTED: &str = "posted";
pub const DEMAND_STATE_PLANNED: &str = "planned";
pub const DEMAND_STATE_FULFILLED: &str = "fulfilled";
pub const DEMAND_STATE_EXPIRED: &str = "expired";

impl DemandState {
    /// The token the record carries.
    pub fn as_str(self) -> &'static str {
        match self {
            DemandState::Posted => DEMAND_STATE_POSTED,
            DemandState::Planned { .. } => DEMAND_STATE_PLANNED,
            DemandState::Fulfilled { .. } => DEMAND_STATE_FULFILLED,
            DemandState::Expired => DEMAND_STATE_EXPIRED,
        }
    }

    fn granted(self) -> Option<u32> {
        match self {
            DemandState::Planned { granted } | DemandState::Fulfilled { granted } => Some(granted),
            DemandState::Posted | DemandState::Expired => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub demand: Demand,
    pub posted_tick: u64,
    pub state: DemandState,
}

/// The board: every demand this seat has posted, and what became of it.
#[derive(Debug, Default)]
pub struct Board {
    entries: Vec<Entry>,
}

impl Board {
    /// Post `demands` at `tick`, recording each as `posted`.
    pub fn post(&mut self, tick: u64, demands: Vec<Demand>, sink: &mut dyn DecisionSink) {
        for demand in demands {
            let entry = Entry {
                demand,
                posted_tick: tick,
                state: DemandState::Posted,
            };
            record(sink, tick, &entry);
            self.entries.push(entry);
        }
    }

    /// The posted demands for `band` whose window has not passed at `tick`.
    pub fn open_for(&self, band: u64, tick: u64) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |entry| {
            entry.demand.band == band
                && entry.state == DemandState::Posted
                && tick <= entry.demand.by_tick
        })
    }

    /// The orchestrator's answer for `band`'s open demands, **in the order [`Board::open_for`]
    /// gave them**: a grant above zero is `Planned`, a grant of zero is `Expired` — nothing was
    /// sent for it, so nothing can fulfil it.
    pub fn plan(
        &mut self,
        tick: u64,
        band: u64,
        grants: &[(Resource, u32)],
        sink: &mut dyn DecisionSink,
    ) {
        let mut grants = grants.iter();
        for entry in self.entries.iter_mut().filter(|entry| {
            entry.demand.band == band
                && entry.state == DemandState::Posted
                && tick <= entry.demand.by_tick
        }) {
            let Some((resource, granted)) = grants.next() else {
                break;
            };
            debug_assert_eq!(
                resource, &entry.demand.resource,
                "grants follow the open order"
            );
            entry.state = if *granted > 0 {
                DemandState::Planned { granted: *granted }
            } else {
                DemandState::Expired
            };
            record(sink, tick, entry);
        }
    }

    /// What the frame at `tick` says about every planned demand: `Fulfilled` when the band
    /// carries the grant, `Expired` when the frame after its window still does not (the sim
    /// refused the loadout); a `Posted` demand past its window is `Expired` too.
    pub fn settle(&mut self, tick: u64, view: &SeatView, sink: &mut dyn DecisionSink) {
        for entry in &mut self.entries {
            let past_window = tick > entry.demand.by_tick;
            let next = match entry.state {
                DemandState::Posted if past_window => DemandState::Expired,
                DemandState::Planned { granted } => {
                    let held = view
                        .band(entry.demand.band)
                        .is_some_and(|band| carries(view, band, &entry.demand.resource, granted));
                    if held {
                        DemandState::Fulfilled { granted }
                    } else if past_window {
                        DemandState::Expired
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };
            entry.state = next;
            record(sink, tick, entry);
        }
    }

    /// A full frame at `tick` replaced the view: demands posted later are for a world that no
    /// longer exists.
    pub fn forget_after(&mut self, tick: u64) {
        self.entries.retain(|entry| entry.posted_tick <= tick);
    }

    #[cfg(test)]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

fn record(sink: &mut dyn DecisionSink, tick: u64, entry: &Entry) {
    sink.record(DecisionRecord::Demand(DemandRecord {
        tick,
        requester: entry.demand.requester.to_owned(),
        band: entry.demand.band,
        resource: entry.demand.resource.to_string(),
        amount: entry.demand.amount,
        state: entry.state.as_str().to_owned(),
        granted: entry.state.granted(),
    }));
}

/// **The roster's item-less kit**, whose row in a band's `kit_tiers` is the bare hand on every
/// axis — the reading an armed kit is above.
pub const BARE_KIT_ID: &str = "none";

/// **Whether `band` holds `kit` armed** — the `hunting_kits_held` reading generalised to any
/// kit: its `kit_tiers` row (*"What one kit would grant THIS band, at its current wear … the
/// RESOLVED answer"*) is above the `none` row on **any** axis a kit grants — `attack`,
/// `hunt_carry_per_worker_biomass`, `forage_carry_per_worker_biomass`, `scout_vantage_range` or
/// `build_work_per_worker` — because a gathering kit grants no attack and a stalking kit no
/// baskets, so one axis would read every other kit as bare.
pub fn kit_armed(band: &PopulationCohortState, kit: &str) -> bool {
    let row = |id: &str| band.kit_tiers.iter().find(|tier| tier.kit_id == id);
    let (Some(kit), Some(bare)) = (row(kit), row(BARE_KIT_ID)) else {
        return false;
    };
    let axes = |tier: &BandKitTiersState| {
        [
            tier.attack,
            tier.hunt_carry_per_worker_biomass,
            tier.forage_carry_per_worker_biomass,
            tier.scout_vantage_range,
            tier.build_work_per_worker,
        ]
    };
    axes(kit)
        .iter()
        .zip(axes(bare))
        .any(|(armed, bare)| *armed > bare)
}

/// Whether `band` carries `granted` of `resource`: a kit when its tier is armed
/// ([`kit_armed`]) **or** its `equipment_batches` carry every item of the kit's `item_ids` at
/// `count ≥ granted`; a material when `material_batches` carry at least `granted` units.
fn carries(
    view: &SeatView,
    band: &PopulationCohortState,
    resource: &Resource,
    granted: u32,
) -> bool {
    match resource {
        Resource::Kit(kit_id) => {
            if kit_armed(band, kit_id) {
                return true;
            }
            view.snapshot
                .kits
                .iter()
                .find(|kit| &kit.id == kit_id)
                .is_some_and(|kit| {
                    !kit.item_ids.is_empty()
                        && kit.item_ids.iter().all(|item| {
                            band.equipment_batches
                                .iter()
                                .filter(|batch| &batch.item_id == item)
                                .map(|batch| batch.count)
                                .sum::<u32>()
                                >= granted
                        })
                })
        }
        Resource::Material(material_id) => {
            band.material_batches
                .iter()
                .filter(|batch| &batch.material_id == material_id)
                .map(|batch| batch.amount)
                .sum::<f32>()
                >= granted as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruments::decisions::VecSink;
    use crate::specialists::food::tests::{
        a_view, kit_tier, ARMED_ATTACK, BAND, BARE_ATTACK, BARE_KIT, FORAGE_KIT, HUNT_KIT,
        HUNT_KIT_ITEM, TICK,
    };
    use crate::specialists::{SPECIALIST_FOOD, SPECIALIST_LAND};
    use sim_runtime::{EquipmentBatchState, MaterialBatchState};

    fn demand(requester: SpecialistId, resource: Resource, amount: u32) -> Demand {
        Demand {
            requester,
            band: BAND,
            resource,
            amount,
            by_tick: TICK,
            priority: 1.0,
        }
    }

    fn records(sink: &VecSink) -> Vec<(u64, String, String, Option<u32>)> {
        sink.0
            .iter()
            .filter_map(|record| match record {
                DecisionRecord::Demand(demand) => Some((
                    demand.tick,
                    demand.resource.clone(),
                    demand.state.clone(),
                    demand.granted,
                )),
                _ => None,
            })
            .collect()
    }

    /// posted → planned → fulfilled, with a record at every step; a grant of zero expires at
    /// once; a planned grant the next frame does not carry expires with the window.
    #[test]
    fn a_demand_round_trips_posted_planned_fulfilled_and_is_recorded_at_every_step() {
        let mut board = Board::default();
        let mut sink = VecSink::default();
        board.post(
            TICK,
            vec![
                demand(SPECIALIST_FOOD, Resource::Kit(HUNT_KIT.to_owned()), 9),
                demand(SPECIALIST_FOOD, Resource::Kit(FORAGE_KIT.to_owned()), 8),
                demand(SPECIALIST_FOOD, Resource::Material("bone".to_owned()), 3),
            ],
            &mut sink,
        );
        assert_eq!(board.open_for(BAND, TICK).count(), 3);
        assert_eq!(
            board.open_for(BAND, TICK + 1).count(),
            0,
            "the window is one frame"
        );
        board.plan(
            TICK,
            BAND,
            &[
                (Resource::Kit(HUNT_KIT.to_owned()), 9),
                (Resource::Kit(FORAGE_KIT.to_owned()), 0),
                (Resource::Material("bone".to_owned()), 3),
            ],
            &mut sink,
        );
        assert_eq!(board.open_for(BAND, TICK).count(), 0);
        // The next frame: the stalking kit resolves armed, the bone is banked, the baskets were
        // never granted.
        let mut view = a_view();
        view.snapshot.header.tick = TICK + 1;
        {
            let band = &mut view.snapshot.populations[0];
            band.kit_tiers = vec![
                kit_tier(HUNT_KIT, ARMED_ATTACK),
                kit_tier(FORAGE_KIT, BARE_ATTACK),
                kit_tier(BARE_KIT, BARE_ATTACK),
            ];
            band.material_batches = vec![MaterialBatchState {
                material_id: "bone".to_owned(),
                amount: 3.0,
                ..Default::default()
            }];
        }
        board.settle(TICK + 1, &view, &mut sink);
        let states: Vec<(String, String, Option<u32>)> = board
            .entries()
            .iter()
            .map(|entry| {
                (
                    entry.demand.resource.to_string(),
                    entry.state.as_str().to_owned(),
                    entry.state.granted(),
                )
            })
            .collect();
        assert_eq!(
            states,
            vec![
                ("kit:big_game".to_owned(), "fulfilled".to_owned(), Some(9)),
                ("kit:gathering".to_owned(), "expired".to_owned(), None),
                ("material:bone".to_owned(), "fulfilled".to_owned(), Some(3)),
            ]
        );
        let recorded = records(&sink);
        assert_eq!(
            recorded.len(),
            3 + 3 + 2,
            "posted ×3, planned/expired ×3, settled ×2"
        );
        assert_eq!(
            recorded[0],
            (TICK, "kit:big_game".to_owned(), "posted".to_owned(), None)
        );
        assert_eq!(
            recorded[3],
            (
                TICK,
                "kit:big_game".to_owned(),
                "planned".to_owned(),
                Some(9)
            )
        );
        assert_eq!(
            recorded[6],
            (
                TICK + 1,
                "kit:big_game".to_owned(),
                "fulfilled".to_owned(),
                Some(9)
            )
        );
        // A planned grant the sim refused: the frame after the window carries nothing.
        let mut refused = Board::default();
        refused.post(
            TICK,
            vec![demand(
                SPECIALIST_LAND,
                Resource::Kit("wayfinding".to_owned()),
                1,
            )],
            &mut sink,
        );
        refused.plan(
            TICK,
            BAND,
            &[(Resource::Kit("wayfinding".to_owned()), 1)],
            &mut sink,
        );
        refused.settle(TICK + 1, &a_view(), &mut sink);
        assert_eq!(refused.entries()[0].state, DemandState::Expired);
        // A post nobody planned is expired once its window has passed.
        let mut unplanned = Board::default();
        unplanned.post(
            TICK,
            vec![demand(
                SPECIALIST_FOOD,
                Resource::Kit(HUNT_KIT.to_owned()),
                1,
            )],
            &mut sink,
        );
        unplanned.settle(TICK, &a_view(), &mut sink);
        assert_eq!(unplanned.entries()[0].state, DemandState::Posted);
        unplanned.settle(TICK + 1, &a_view(), &mut sink);
        assert_eq!(unplanned.entries()[0].state, DemandState::Expired);
        unplanned.forget_after(TICK - 1);
        assert!(unplanned.entries().is_empty());
    }

    /// The batches path: a kit whose tier row is not on the wire is carried when every item it
    /// lists is held at the granted count.
    #[test]
    fn a_kit_is_carried_by_its_tier_or_by_every_item_it_lists() {
        let mut view = a_view();
        let band = &mut view.snapshot.populations[0];
        band.kit_tiers.clear();
        band.equipment_batches = vec![EquipmentBatchState {
            item_id: HUNT_KIT_ITEM.to_owned(),
            count: 2,
            ..Default::default()
        }];
        let band = &view.snapshot.populations[0];
        assert!(!kit_armed(band, HUNT_KIT), "no tiers on the wire");
        assert!(carries(&view, band, &Resource::Kit(HUNT_KIT.to_owned()), 2));
        assert!(!carries(
            &view,
            band,
            &Resource::Kit(HUNT_KIT.to_owned()),
            3
        ));
        assert!(!carries(
            &view,
            band,
            &Resource::Kit(BARE_KIT.to_owned()),
            1
        ));
        // An armed forage kit reads on the carry axis, not the attack.
        let mut armed = a_view();
        for tier in &mut armed.snapshot.populations[0].kit_tiers {
            if tier.kit_id == FORAGE_KIT {
                tier.forage_carry_per_worker_biomass = 8.0;
            }
        }
        assert!(kit_armed(&armed.snapshot.populations[0], FORAGE_KIT));
        assert!(!kit_armed(&armed.snapshot.populations[0], BARE_KIT));
    }
}
