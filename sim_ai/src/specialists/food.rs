//! **`Food`** — the food loop (`docs/plan_ai_driver.md` §4). Owns `runway_turns`; alarms
//! `FoodShort` when the minimum own-band runway is below the profile's `food.runway_floor_turns`.
//!
//! Three considerations, each a pure function of one band and the view:
//!
//! - *idle hands* — a band with `idle_workers > 0` assigns them to the best-yield source in reach:
//!   a discovered forage patch within `work_range`, or a huntable herd within `hunt_reach`, ranked
//!   by the frame's own `per_worker_yield` forecast. The all-Pass control starves because nobody
//!   is ever assigned; this is the consideration that has to beat it.
//! - *runway* — under the alarm, the band's lowest-yielding worked row is emptied onto its highest.
//! - *overuse* — a row whose `actual_yield` exceeds its `sustainable_yield` (the intensification
//!   arc's overhunting signal, read off the frame), or a hunt row whose `hunt_useful_workers` is
//!   `0` (*no crew is useful here*, the sim's own verdict), is emptied onto the next-best source.
//!
//! **Sources are ranked by the crew's expected take, not the per-worker rate.** The frame's
//! `per_worker_yield` is a rate; what a crew of `n` takes is `min(n × rate, ceiling)`, and the
//! ceiling is composed from terms on the wire — `biomass × provisions_per_biomass`, the take at a
//! zero escapement floor (`ForagePatchState::per_worker_yield` docs). The sim's default floor is
//! not published, so this is an upper bound; a herd of a few animals still ranks below a stand the
//! whole band can gather, which is the failure a per-worker ranking walked into.
//!
//! Every command is `assign_labor`, with the kit and floor left to the frame's defaults (`None`
//! means the job's default on the wire) — a specialist names no number the sim already owns.

use sim_runtime::{CommandPayload, LaborAssignmentState, PopulationCohortState};
use tracing::{debug, info};

use super::{intent_key, Cost, Proposal, Proposals, Specialist, SpecialistId, SPECIALIST_FOOD};
use crate::geometry::Tile;
use crate::orchestrator::{Alarm, AlarmKind, Plan};
use crate::profile::FoodFloors;
use crate::view::{band_tile, row_key, SeatMemory, SeatView};

/// The `assign_labor` roles this specialist staffs — the `kind` vocabulary of
/// `LaborAssignmentState` (`sim_runtime/src/command_text.rs`).
pub const ROLE_FORAGE: &str = "forage";
pub const ROLE_HUNT: &str = "hunt";
/// The intent kinds.
pub const INTENT_ASSIGN: &str = "assign";
pub const INTENT_RUNWAY: &str = "runway";
pub const INTENT_RELIEVE: &str = "relieve";
/// The considerations, as the decision log names them.
const REASON_IDLE_HANDS: &str = "idle hands";
const REASON_RUNWAY: &str = "runway below floor";
const REASON_OVERUSE: &str = "source overused";
const REASON_DEAD_ROW: &str = "no useful crew";

/// A source a band can be assigned to.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceKey {
    Patch(Tile),
    Herd(String),
}

impl SourceKey {
    /// The intent subject: `x,y` for a patch, the herd id for a herd.
    fn subject(&self) -> String {
        match self {
            SourceKey::Patch(tile) => format!("{},{}", tile.x, tile.y),
            SourceKey::Herd(id) => id.clone(),
        }
    }

    /// The source as the decision log names it: `forage 5,4` / `hunt herd_9`.
    fn describe(&self) -> String {
        match self {
            SourceKey::Patch(_) => format!("{ROLE_FORAGE} {}", self.subject()),
            SourceKey::Herd(_) => format!("{ROLE_HUNT} {}", self.subject()),
        }
    }

    /// The `assign_labor` role — the web — this source is worked under.
    fn role(&self) -> &'static str {
        match self {
            SourceKey::Patch(_) => ROLE_FORAGE,
            SourceKey::Herd(_) => ROLE_HUNT,
        }
    }

    /// The memory key of this source's row on `band` ([`crate::view::row_key`]).
    fn row_key(&self, band_id: u64) -> String {
        match self {
            SourceKey::Patch(tile) => row_key(band_id, ROLE_FORAGE, tile.x, tile.y, ""),
            SourceKey::Herd(id) => row_key(band_id, ROLE_HUNT, 0, 0, id),
        }
    }

    fn of_row(row: &LaborAssignmentState) -> Option<Self> {
        match row.kind.as_str() {
            ROLE_FORAGE => Some(SourceKey::Patch(Tile::new(row.target_x, row.target_y))),
            ROLE_HUNT => Some(SourceKey::Herd(row.fauna_id.clone())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Source {
    key: SourceKey,
    /// The rate a crew is ranked on: what this band has **realized** on the source, if it has
    /// worked it, else the frame's forecast.
    per_worker_yield: f32,
    /// The take at a zero floor: `biomass × provisions_per_biomass`.
    ceiling: f32,
}

impl Source {
    /// What a crew of `hands` takes: `min(hands × rate, ceiling)`.
    fn expected(&self, hands: u32) -> f32 {
        (hands as f32 * self.per_worker_yield).min(self.ceiling)
    }
}

pub struct Food {
    faction: u32,
    floors: FoodFloors,
    /// The profile's `food_security` weight: this specialist's scores are scaled by it.
    weight: f32,
}

impl Food {
    pub fn new(faction: u32, floors: FoodFloors, weight: f32) -> Self {
        Self {
            faction,
            floors,
            weight,
        }
    }

    /// Whether `band`'s row on `key` has paid nothing for `dead_row_turns` turns, or the sim has
    /// said no crew is useful on it.
    /// Whether `band`'s row on `key` has realized less than `poor_yield_fraction` of `forecast`
    /// per worker for `dead_row_turns` turns — or the sim has said no crew is useful on it.
    fn is_dead(
        &self,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        key: &SourceKey,
        forecast: f32,
    ) -> bool {
        memory
            .realized(&key.row_key(band.band_id))
            .is_some_and(|realized| {
                realized.worked_turns >= self.floors.dead_row_turns
                    && realized.per_worker < self.floors.poor_yield_fraction * forecast
            })
    }

    /// The rate to rank `key` on: what the band realized there; else what the seat has realized
    /// across that web (a hunt's published rate is not what a bare-handed crew takes, and a seat
    /// that has measured one herd knows that about the next); else `forecast`.
    fn rate(
        memory: &SeatMemory,
        band: &PopulationCohortState,
        key: &SourceKey,
        forecast: f32,
    ) -> f32 {
        memory
            .realized(&key.row_key(band.band_id))
            .map(|realized| realized.per_worker)
            .or_else(|| memory.realized_for_kind(key.role()))
            .unwrap_or(forecast)
    }

    /// The frame's forecast for the source under `key`, when it is in the frame.
    fn forecast_for(view: &SeatView, key: &SourceKey) -> Option<f32> {
        match key {
            SourceKey::Patch(tile) => view.patch_at(*tile).map(|patch| patch.per_worker_yield),
            SourceKey::Herd(id) => view
                .snapshot
                .herds
                .iter()
                .find(|herd| &herd.id == id)
                .map(|herd| herd.per_worker_yield),
        }
    }

    /// **A gathering site**: the plant rung's `site_requirement` (`plant_rung_site_refusal`,
    /// `core_sim/src/bin/server.rs`) is the tile carrying a food module, and a patch row is
    /// published for ground that carries none — a crew sent there is refused *"nobody gathers
    /// here"*.
    fn is_food_site(view: &SeatView, tile: Tile) -> bool {
        view.snapshot
            .food_modules
            .iter()
            .any(|site| site.x == tile.x && site.y == tile.y)
    }

    /// The sources within `band`'s reach that the seat has discovered and has not found dead.
    fn reachable_sources(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Vec<Source> {
        let grid = view.grid();
        let here = band_tile(band);
        let mut sources: Vec<Source> = view
            .snapshot
            .forage_patches
            .iter()
            .filter(|patch| patch.per_worker_yield > 0.0)
            .filter(|patch| {
                let tile = Tile::new(patch.x, patch.y);
                view.is_discovered(tile)
                    && Self::is_food_site(view, tile)
                    && grid.distance(here, tile) <= band.work_range
            })
            .map(|patch| {
                let key = SourceKey::Patch(Tile::new(patch.x, patch.y));
                Source {
                    per_worker_yield: Self::rate(memory, band, &key, patch.per_worker_yield),
                    key,
                    ceiling: patch.biomass * patch.provisions_per_biomass,
                }
            })
            .filter(|source| {
                let forecast = Self::forecast_for(view, &source.key).unwrap_or_default();
                !self.is_dead(memory, band, &source.key, forecast)
            })
            .collect();
        sources.extend(
            view.snapshot
                .herds
                .iter()
                .filter(|herd| herd.huntable && herd.per_worker_yield > 0.0)
                .filter(|herd| grid.distance(here, Tile::new(herd.x, herd.y)) <= band.hunt_reach)
                .map(|herd| {
                    let key = SourceKey::Herd(herd.id.clone());
                    Source {
                        per_worker_yield: Self::rate(memory, band, &key, herd.per_worker_yield),
                        key,
                        ceiling: herd.biomass * herd.provisions_per_biomass,
                    }
                })
                .filter(|source| {
                    let forecast = Self::forecast_for(view, &source.key).unwrap_or_default();
                    !self.is_dead(memory, band, &source.key, forecast)
                }),
        );
        debug!(
            band = band.band_id,
            at = ?here,
            work_range = band.work_range,
            hunt_reach = band.hunt_reach,
            patches_in_frame = view.snapshot.forage_patches.len(),
            patches_in_reach = view
                .snapshot
                .forage_patches
                .iter()
                .filter(|patch| grid.distance(here, Tile::new(patch.x, patch.y)) <= band.work_range)
                .count(),
            patches_discovered_in_reach = view
                .snapshot
                .forage_patches
                .iter()
                .filter(|patch| {
                    let tile = Tile::new(patch.x, patch.y);
                    view.is_discovered(tile) && grid.distance(here, tile) <= band.work_range
                })
                .count(),
            visibility_here = view.visibility(here),
            raster = ?(grid.width, grid.height, view.snapshot.visibility_raster.samples.len()),
            sources = ?sources
                .iter()
                .map(|source| (source.key.describe(), source.per_worker_yield, source.ceiling))
                .collect::<Vec<_>>(),
            rows = ?band
                .labor_assignments
                .iter()
                .map(|row| (row.kind.clone(), row.workers, row.actual_yield, row.sustainable_yield, row.hunt_useful_workers))
                .collect::<Vec<_>>(),
            "sources in reach"
        );
        sources
    }

    /// The source a crew of `hands` takes the most from, other than `except`; none when nothing
    /// in reach would pay.
    fn best_source<'s>(
        sources: &'s [Source],
        hands: u32,
        except: Option<&SourceKey>,
    ) -> Option<&'s Source> {
        sources
            .iter()
            .filter(|source| except != Some(&source.key))
            .filter(|source| source.expected(hands) > 0.0)
            .max_by(|a, b| a.expected(hands).total_cmp(&b.expected(hands)))
    }

    fn assign(
        &self,
        band: &PopulationCohortState,
        key: &SourceKey,
        workers: u32,
    ) -> CommandPayload {
        let (role, target_x, target_y, fauna_id) = match key {
            SourceKey::Patch(tile) => (ROLE_FORAGE, Some(tile.x), Some(tile.y), None),
            SourceKey::Herd(id) => (ROLE_HUNT, None, None, Some(id.clone())),
        };
        CommandPayload::AssignLabor {
            faction_id: self.faction,
            band_id: Some(band.band_id),
            role: role.to_owned(),
            workers,
            target_x,
            target_y,
            fauna_id,
            policy: None,
            species: None,
            floor: None,
            kit_id: None,
            take_species: Vec::new(),
        }
    }

    /// Workers already on `key`'s row, if the band works it.
    fn workers_on(band: &PopulationCohortState, key: &SourceKey) -> u32 {
        band.labor_assignments
            .iter()
            .find(|row| SourceKey::of_row(row).as_ref() == Some(key))
            .map_or(0, |row| row.workers)
    }

    /// **This specialist's slice of the plan**: the workers its share of the seat's working-age
    /// pool comes to this turn — the same arithmetic the arbiter charges, so a proposal sized to
    /// it is never `over_budget`. A specialist reads its budget, never the profile.
    fn budget_workers(&self, view: &SeatView, plan: &Plan) -> u32 {
        let pool: u32 = view
            .own_bands(self.faction)
            .map(|band| band.working_age)
            .sum();
        (plan.worker_share(SPECIALIST_FOOD) * pool as f32).floor() as u32
    }

    /// *Idle hands*: idle workers onto the best source in reach, as many as the budget allows —
    /// the rest go next turn.
    pub fn idle_hands(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        if band.idle_workers == 0 || band.is_traveling {
            return None;
        }
        let hands = band.idle_workers.min(self.budget_workers(view, plan));
        if hands == 0 {
            return None;
        }
        let sources = self.reachable_sources(view, memory, band);
        let best = Self::best_source(&sources, hands, None)?;
        let workers = Self::workers_on(band, &best.key) + hands;
        info!(
            band = band.band_id,
            source = %best.key.describe(),
            per_worker_yield = best.per_worker_yield,
            hands,
            "idle hands"
        );
        Some(Proposal {
            commands: vec![self.assign(band, &best.key, workers)],
            intent: intent_key(SPECIALIST_FOOD, INTENT_ASSIGN, band.band_id),
            score: band.idle_workers as f32 / band.working_age.max(1) as f32 * self.weight,
            cost: Cost {
                workers: hands,
                bands: vec![band.band_id],
            },
            reason: format!("{REASON_IDLE_HANDS}: {}", best.key.describe()),
        })
    }

    /// *Runway*: below the floor, the lowest-yield worked row is emptied onto the highest — as
    /// far as the budget reaches.
    pub fn runway(
        &self,
        view: &SeatView,
        plan: &Plan,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        let floor = self.floors.runway_floor_turns;
        if band.turns_of_food >= floor || band.is_traveling {
            return None;
        }
        let budget = self.budget_workers(view, plan);
        let worked: Vec<&LaborAssignmentState> = band
            .labor_assignments
            .iter()
            .filter(|row| row.workers > 0 && SourceKey::of_row(row).is_some())
            .collect();
        let yield_per_worker = |row: &LaborAssignmentState| row.actual_yield / row.workers as f32;
        let lowest = worked
            .iter()
            .min_by(|a, b| yield_per_worker(a).total_cmp(&yield_per_worker(b)))?;
        let highest = worked
            .iter()
            .max_by(|a, b| yield_per_worker(a).total_cmp(&yield_per_worker(b)))?;
        if std::ptr::eq(*lowest, *highest) {
            return None;
        }
        let (low_key, high_key) = (SourceKey::of_row(lowest)?, SourceKey::of_row(highest)?);
        let moved = lowest.workers.min(budget);
        if moved == 0 {
            return None;
        }
        Some(Proposal {
            commands: vec![
                self.assign(band, &low_key, lowest.workers - moved),
                self.assign(band, &high_key, highest.workers + moved),
            ],
            intent: intent_key(SPECIALIST_FOOD, INTENT_RUNWAY, band.band_id),
            score: (floor - band.turns_of_food) / floor * self.weight,
            cost: Cost {
                workers: moved,
                bands: vec![band.band_id],
            },
            reason: format!(
                "{REASON_RUNWAY}: {} -> {}",
                low_key.describe(),
                high_key.describe()
            ),
        })
    }

    /// *Overuse*: a row taking more than its source regrows — or a hunt row the sim says no crew
    /// is useful on — moves to the next-best source.
    pub fn overuse(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Vec<Proposal> {
        if band.is_traveling {
            return Vec::new();
        }
        let budget = self.budget_workers(view, plan);
        let sources = self.reachable_sources(view, memory, band);
        band.labor_assignments
            .iter()
            .filter(|row| row.workers > 0)
            .filter_map(|row| {
                let key = SourceKey::of_row(row)?;
                let overused = row.actual_yield > row.sustainable_yield;
                let dead = (row.kind == ROLE_HUNT && row.hunt_useful_workers == 0)
                    || self.is_dead(
                        memory,
                        band,
                        &key,
                        Self::forecast_for(view, &key).unwrap_or_default(),
                    );
                let (reason, score) = if overused {
                    (
                        REASON_OVERUSE,
                        (row.actual_yield - row.sustainable_yield) / row.actual_yield,
                    )
                } else if dead {
                    (REASON_DEAD_ROW, 1.0)
                } else {
                    return None;
                };
                let moved = row.workers.min(budget);
                if moved == 0 {
                    return None;
                }
                let next = Self::best_source(&sources, moved, Some(&key))?;
                Some(Proposal {
                    commands: vec![
                        self.assign(band, &key, row.workers - moved),
                        self.assign(band, &next.key, Self::workers_on(band, &next.key) + moved),
                    ],
                    intent: intent_key(SPECIALIST_FOOD, INTENT_RELIEVE, key.subject()),
                    score: score * self.weight,
                    cost: Cost {
                        workers: moved,
                        bands: vec![band.band_id],
                    },
                    reason: format!("{reason}: {} -> {}", key.describe(), next.key.describe()),
                })
            })
            .collect()
    }

    /// The alarm: the minimum own-band runway is below the floor.
    pub fn alarm(&self, view: &SeatView) -> Option<Alarm> {
        let short = view
            .own_bands(self.faction)
            .any(|band| band.turns_of_food < self.floors.runway_floor_turns);
        short.then_some(Alarm {
            specialist: SPECIALIST_FOOD,
            kind: AlarmKind::FoodShort,
            since_tick: view.tick(),
        })
    }
}

impl Specialist for Food {
    fn id(&self) -> SpecialistId {
        SPECIALIST_FOOD
    }

    fn propose(&mut self, view: &SeatView, plan: &Plan, memory: &SeatMemory) -> Proposals {
        let mut out = Proposals {
            proposals: Vec::new(),
            alarm: self.alarm(view),
        };
        for band in view.own_bands(self.faction) {
            out.proposals
                .extend(self.idle_hands(view, plan, memory, band));
            out.proposals.extend(self.runway(view, plan, band));
            out.proposals.extend(self.overuse(view, plan, memory, band));
        }
        out
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::profile::{AiProfiles, NO_MEMORY_DECAY};
    use crate::view::{VISIBILITY_ACTIVE, VISIBILITY_DISCOVERED};
    use sim_runtime::{ForagePatchState, HerdTelemetryState, WorldSnapshot};

    pub const FACTION: u32 = 3;
    pub const BAND: u64 = 7001;
    pub const TICK: u64 = 41;
    /// The band stands here; the raster is 8×6, wrapped, everything discovered.
    pub const HERE: Tile = Tile::new(3, 2);
    pub const WORK_RANGE: u32 = 2;
    pub const HUNT_REACH: u32 = 5;
    const NEAR_PATCH: Tile = Tile::new(4, 2);
    const RICH_PATCH: Tile = Tile::new(2, 3);
    const FAR_PATCH: Tile = Tile::new(7, 5);
    const HERD_ID: &str = "herd_9";
    const HERD_AT: Tile = Tile::new(5, 4);

    /// The saturated fixture, pinned: one own band with idle hands, three patches (near, rich,
    /// out of reach) and one huntable herd, the raster all discovered.
    pub fn a_view() -> SeatView {
        let mut snapshot: WorldSnapshot =
            sim_runtime::fixture::saturated_snapshot().expect("the fixture builds");
        snapshot.header.tick = TICK;
        snapshot.header.wrap_horizontal = true;
        snapshot.visibility_raster.width = 8;
        snapshot.visibility_raster.height = 6;
        snapshot.visibility_raster.samples = vec![VISIBILITY_DISCOVERED; 48];
        snapshot.visibility_raster.samples[(HERE.y * 8 + HERE.x) as usize] = VISIBILITY_ACTIVE;
        snapshot.populations = vec![PopulationCohortState {
            faction: FACTION,
            band_id: BAND,
            current_x: HERE.x,
            current_y: HERE.y,
            size: 30,
            working_age: 17,
            idle_workers: 17,
            work_range: WORK_RANGE,
            hunt_reach: HUNT_REACH,
            turns_of_food: 14.0,
            is_traveling: false,
            is_expedition: false,
            labor_assignments: Vec::new(),
            ..Default::default()
        }];
        // Each stand's ceiling (`biomass × provisions_per_biomass`) is 1.5× its capacity, so a
        // 17-hand crew is capped at 30 on the near patch and takes its full 34 on the rich one.
        let patch = |tile: Tile, per_worker_yield: f32, carrying_capacity: f32| ForagePatchState {
            x: tile.x,
            y: tile.y,
            owner: None,
            per_worker_yield,
            carrying_capacity,
            biomass: carrying_capacity * 1.5,
            provisions_per_biomass: 1.0,
            ..Default::default()
        };
        snapshot.forage_patches = vec![
            patch(NEAR_PATCH, 1.0, 20.0),
            patch(RICH_PATCH, 2.0, 40.0),
            patch(FAR_PATCH, 9.0, 90.0),
        ];
        snapshot.food_modules = [NEAR_PATCH, RICH_PATCH, FAR_PATCH]
            .iter()
            .map(|tile| sim_runtime::FoodModuleState {
                x: tile.x,
                y: tile.y,
                ..snapshot.food_modules[0].clone()
            })
            .collect();
        snapshot.herds = vec![HerdTelemetryState {
            id: HERD_ID.to_owned(),
            x: HERD_AT.x,
            y: HERD_AT.y,
            huntable: true,
            per_worker_yield: 1.5,
            // A few animals: the best per-worker rate in reach, and a ceiling of 2.
            biomass: 2.0,
            provisions_per_biomass: 1.0,
            ..snapshot.herds[0].clone()
        }];
        SeatView {
            snapshot,
            last_acted_tick: None,
        }
    }

    /// A plan funding Food at `share` of the pool, priority 1.
    pub fn plan_with_food_share(share: f32) -> Plan {
        Plan {
            stance: crate::orchestrator::Stance::Consolidate,
            budgets: std::collections::BTreeMap::from([(
                SPECIALIST_FOOD,
                crate::orchestrator::Budget {
                    worker_share: share,
                },
            )]),
            priorities: std::collections::BTreeMap::from([(SPECIALIST_FOOD, 1.0)]),
            since_turn: TICK,
        }
    }

    pub fn food() -> Food {
        let profile = AiProfiles::builtin().profile("forager").unwrap().clone();
        Food::new(FACTION, profile.food, profile.weight("food_security"))
    }

    fn own_band(view: &SeatView) -> &PopulationCohortState {
        view.own_bands(FACTION).next().unwrap()
    }

    #[test]
    fn idle_hands_go_to_the_richest_source_in_reach() {
        let view = a_view();
        let proposal = food()
            .idle_hands(
                &view,
                &plan_with_food_share(1.0),
                &SeatMemory::new(NO_MEMORY_DECAY),
                own_band(&view),
            )
            .expect("a proposal");
        assert_eq!(proposal.intent, "food:assign:7001");
        assert!(proposal.score > 0.0);
        assert_eq!(proposal.cost.workers, 17);
        assert_eq!(proposal.cost.bands, vec![BAND]);
        // Half the pool funded: 8 of the 17 go now, and the rest next turn.
        let capped = food()
            .idle_hands(
                &view,
                &plan_with_food_share(0.5),
                &SeatMemory::new(NO_MEMORY_DECAY),
                own_band(&view),
            )
            .expect("a capped proposal");
        assert_eq!(capped.cost.workers, 8);
        assert!(matches!(
            capped.commands[0],
            CommandPayload::AssignLabor { workers: 8, .. }
        ));
        assert!(
            food()
                .idle_hands(
                    &view,
                    &Plan::pass_through(TICK),
                    &SeatMemory::new(NO_MEMORY_DECAY),
                    own_band(&view)
                )
                .is_none(),
            "no share, no hands"
        );
        match &proposal.commands[0] {
            CommandPayload::AssignLabor {
                role,
                workers,
                target_x,
                target_y,
                kit_id,
                floor,
                ..
            } => {
                assert_eq!(role, ROLE_FORAGE);
                assert_eq!(*workers, 17);
                assert_eq!(
                    (*target_x, *target_y),
                    (Some(RICH_PATCH.x), Some(RICH_PATCH.y))
                );
                assert_eq!(kit_id, &None, "the frame's default kit");
                assert_eq!(floor, &None, "the sim's default floor");
            }
            other => panic!("not an assignment: {other:?}"),
        }
    }

    #[test]
    fn sources_are_ranked_by_what_the_crew_takes_not_the_per_worker_rate() {
        let mut view = a_view();
        view.snapshot.visibility_raster.samples[(RICH_PATCH.y * 8 + RICH_PATCH.x) as usize] = 0;
        let proposal = food()
            .idle_hands(
                &view,
                &plan_with_food_share(1.0),
                &SeatMemory::new(NO_MEMORY_DECAY),
                own_band(&view),
            )
            .unwrap();
        assert!(
            matches!(&proposal.commands[0], CommandPayload::AssignLabor { role, target_x: Some(x), .. } if role == ROLE_FORAGE && *x == NEAR_PATCH.x),
            "a herd of two animals cannot feed a crew of 17, whatever its per-worker rate: {proposal:?}"
        );
        // A big herd is a different matter: 17 × 1.5 = 25.5 beats the near patch's 17.
        view.snapshot.herds[0].biomass = 100.0;
        let proposal = food()
            .idle_hands(
                &view,
                &plan_with_food_share(1.0),
                &SeatMemory::new(NO_MEMORY_DECAY),
                own_band(&view),
            )
            .unwrap();
        assert!(
            matches!(&proposal.commands[0], CommandPayload::AssignLabor { role, fauna_id: Some(id), .. } if role == ROLE_HUNT && id == HERD_ID),
            "{proposal:?}"
        );
        view.snapshot.herds.clear();
        let proposal = food()
            .idle_hands(
                &view,
                &plan_with_food_share(1.0),
                &SeatMemory::new(NO_MEMORY_DECAY),
                own_band(&view),
            )
            .unwrap();
        assert!(
            matches!(&proposal.commands[0], CommandPayload::AssignLabor { target_x: Some(x), .. } if *x == NEAR_PATCH.x),
            "never the far patch, however rich"
        );
    }

    #[test]
    fn nothing_idle_or_a_travelling_band_proposes_nothing() {
        let mut view = a_view();
        view.snapshot.populations[0].idle_workers = 0;
        assert!(food()
            .idle_hands(
                &view,
                &plan_with_food_share(1.0),
                &SeatMemory::new(NO_MEMORY_DECAY),
                own_band(&view)
            )
            .is_none());
        view.snapshot.populations[0].idle_workers = 3;
        view.snapshot.populations[0].is_traveling = true;
        assert!(food()
            .idle_hands(
                &view,
                &plan_with_food_share(1.0),
                &SeatMemory::new(NO_MEMORY_DECAY),
                own_band(&view)
            )
            .is_none());
    }

    #[test]
    fn runway_below_the_floor_alarms_and_moves_the_worst_row_onto_the_best() {
        let mut view = a_view();
        let band = &mut view.snapshot.populations[0];
        band.turns_of_food = 2.0;
        band.labor_assignments = vec![
            LaborAssignmentState {
                kind: ROLE_FORAGE.into(),
                target_x: NEAR_PATCH.x,
                target_y: NEAR_PATCH.y,
                workers: 4,
                actual_yield: 2.0,
                ..Default::default()
            },
            LaborAssignmentState {
                kind: ROLE_HUNT.into(),
                fauna_id: HERD_ID.into(),
                workers: 5,
                actual_yield: 10.0,
                ..Default::default()
            },
        ];
        let specialist = food();
        assert_eq!(
            specialist.alarm(&view).map(|alarm| alarm.kind),
            Some(AlarmKind::FoodShort)
        );
        let proposal = specialist
            .runway(&view, &plan_with_food_share(1.0), own_band(&view))
            .expect("a proposal");
        assert_eq!(proposal.intent, "food:runway:7001");
        assert!(proposal.score > 0.0);
        assert_eq!(proposal.commands.len(), 2, "one assign_labor pair");
        assert!(
            matches!(&proposal.commands[0], CommandPayload::AssignLabor { role, workers: 0, .. } if role == ROLE_FORAGE)
        );
        assert!(
            matches!(&proposal.commands[1], CommandPayload::AssignLabor { role, workers: 9, .. } if role == ROLE_HUNT)
        );
        view.snapshot.populations[0].turns_of_food = 20.0;
        assert!(specialist.alarm(&view).is_none());
        assert!(specialist
            .runway(&view, &plan_with_food_share(1.0), own_band(&view))
            .is_none());
    }

    #[test]
    fn ground_without_a_food_module_is_not_a_gathering_site() {
        let mut view = a_view();
        view.snapshot
            .food_modules
            .retain(|site| site.x != RICH_PATCH.x || site.y != RICH_PATCH.y);
        let proposal = food()
            .idle_hands(
                &view,
                &plan_with_food_share(1.0),
                &SeatMemory::new(NO_MEMORY_DECAY),
                own_band(&view),
            )
            .unwrap();
        assert!(
            matches!(&proposal.commands[0], CommandPayload::AssignLabor { target_x: Some(x), .. } if *x == NEAR_PATCH.x),
            "the rich patch carries no food module, so nobody can gather there: {proposal:?}"
        );
    }

    #[test]
    fn a_row_realizing_a_fraction_of_its_forecast_is_dead_after_the_profile_turns_and_avoided() {
        let mut view = a_view();
        view.snapshot.herds[0].biomass = 100.0;
        // Twelve hunters on a herd forecast at 1.5/worker bring home 0.12 a turn: 0.01/worker.
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            kind: ROLE_HUNT.into(),
            fauna_id: HERD_ID.into(),
            workers: 12,
            actual_yield: 0.12,
            sustainable_yield: 1.35,
            hunt_useful_workers: 12,
            ..Default::default()
        }];
        view.snapshot.populations[0].idle_workers = 5;
        let specialist = food();
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY);
        let turns = specialist.floors.dead_row_turns;
        for _ in 0..turns - 1 {
            memory.observe(&view, FACTION);
        }
        assert!(
            specialist
                .overuse(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
                .is_empty(),
            "one turn short of the profile's patience"
        );
        memory.observe(&view, FACTION);
        let proposals =
            specialist.overuse(&view, &plan_with_food_share(1.0), &memory, own_band(&view));
        assert_eq!(proposals.len(), 1);
        assert!(
            proposals[0].reason.starts_with(REASON_DEAD_ROW),
            "{}",
            proposals[0].reason
        );
        assert!(
            matches!(&proposals[0].commands[1], CommandPayload::AssignLabor { role, target_x: Some(x), .. } if role == ROLE_FORAGE && *x == RICH_PATCH.x)
        );
        // And the idle hands no longer consider the herd, whatever its forecast.
        let idle = specialist
            .idle_hands(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
            .unwrap();
        assert!(
            matches!(&idle.commands[0], CommandPayload::AssignLabor { role, .. } if role == ROLE_FORAGE)
        );
        // Nor a second herd it has never worked: the seat has measured what hunting pays it.
        view.snapshot.herds.push(sim_runtime::HerdTelemetryState {
            id: "herd_10".to_owned(),
            ..view.snapshot.herds[0].clone()
        });
        let idle = specialist
            .idle_hands(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
            .unwrap();
        assert!(
            matches!(&idle.commands[0], CommandPayload::AssignLabor { role, .. } if role == ROLE_FORAGE),
            "{idle:?}"
        );
    }

    #[test]
    fn a_hunt_row_no_crew_is_useful_on_is_relieved_onto_the_next_best_source() {
        let mut view = a_view();
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            kind: ROLE_HUNT.into(),
            fauna_id: HERD_ID.into(),
            workers: 12,
            actual_yield: 0.0,
            sustainable_yield: 0.02,
            hunt_useful_workers: 0,
            ..Default::default()
        }];
        let proposals = food().overuse(
            &view,
            &plan_with_food_share(1.0),
            &SeatMemory::new(NO_MEMORY_DECAY),
            own_band(&view),
        );
        assert_eq!(
            proposals.len(),
            1,
            "actual ≤ sustainable, but the sim says no crew is useful"
        );
        assert!(proposals[0].reason.starts_with(REASON_DEAD_ROW));
        assert!(
            matches!(&proposals[0].commands[1], CommandPayload::AssignLabor { role, workers: 12, target_x: Some(x), .. } if role == ROLE_FORAGE && *x == RICH_PATCH.x)
        );
        view.snapshot.populations[0].labor_assignments[0].hunt_useful_workers = 4;
        assert!(
            food()
                .overuse(
                    &view,
                    &plan_with_food_share(1.0),
                    &SeatMemory::new(NO_MEMORY_DECAY),
                    own_band(&view)
                )
                .is_empty(),
            "a useful crew is left alone"
        );
    }

    #[test]
    fn an_overused_row_is_relieved_onto_the_next_best_source() {
        let mut view = a_view();
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            kind: ROLE_HUNT.into(),
            fauna_id: HERD_ID.into(),
            workers: 6,
            actual_yield: 8.0,
            sustainable_yield: 2.0,
            ..Default::default()
        }];
        let proposals = food().overuse(
            &view,
            &plan_with_food_share(1.0),
            &SeatMemory::new(NO_MEMORY_DECAY),
            own_band(&view),
        );
        assert_eq!(proposals.len(), 1);
        let proposal = &proposals[0];
        assert_eq!(proposal.intent, format!("food:relieve:{HERD_ID}"));
        assert!(
            (proposal.score - 0.75 * 0.9).abs() < 1e-6,
            "overuse fraction × weight"
        );
        assert!(
            matches!(&proposal.commands[1], CommandPayload::AssignLabor { role, workers: 6, target_x: Some(x), .. } if role == ROLE_FORAGE && *x == RICH_PATCH.x)
        );
    }
}
