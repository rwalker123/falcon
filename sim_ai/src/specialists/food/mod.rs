//! **`Food`** — the food loop (`docs/plan_ai_driver.md` §4). Owns `runway_turns`; alarms
//! `FoodShort` when the minimum own-band runway is below the profile's `food.runway_floor_turns`.
//!
//! Five rules ([`rules`]), each a pure function of one band, the view, the plan's goals and the
//! memory, each producing at most one proposal per band whose `reason` names the rule and its
//! subject — the §4 table, as shipped:
//!
//! - *negative income* — income below consumption, or idle hands, reassigns to the sources with
//!   the highest take per crew; a row that is overused, dead, or no crew is useful on is the row
//!   emptied first.
//! - *feed while moving* — a band with a move in force works what will fall *outside* its new
//!   range before it leaves; never a freshly split band, which must not strip the parent's ground.
//! - *split to feed* — after reassignment the band is still short and a smaller band could work a
//!   site just out of reach: split toward it, and walk the child there when it appears.
//! - *spare hands into hunts* — income at or near the goal puts the surplus onto a herd, which is
//!   what opens penning.
//! - *upgrade the ground* — the rung known and a worked patch below it: declare the climb and
//!   staff the builders, priced by the projection ledger ([`ledger`]).
//!
//! **The goal gap is the score.** Every rule projects the band's book under its change and scores
//! the progress that projection makes toward the plan's goals ([`ledger::goal_progress`]), times
//! this specialist's weight — so the arbiter ranks by goal gap closed, and a rule that needs a goal
//! and is handed none (the pass-through plan) proposes nothing.
//!
//! **Sources are ranked by the crew's expected take, not the per-worker rate** ([`sources`]). The
//! frame's `per_worker_yield` is a rate; what a crew of `n` takes is `min(n × rate, ceiling)`, and
//! the ceiling is composed from terms on the wire — `biomass × provisions_per_biomass`, the take at a
//! zero escapement floor (`ForagePatchState::per_worker_yield` docs). The sim's default floor is
//! not published, so this is an upper bound; a herd of a few animals still ranks below a stand the
//! whole band can gather, which is the failure a per-worker ranking walked into.
//!
//! Every assignment is `assign_labor` with the kit and floor left to the frame's defaults (`None`
//! means the job's default on the wire) — a specialist names no number the sim already owns.

pub mod ledger;
mod rules;
mod sources;

use sim_runtime::{
    CommandPayload, LaborAssignmentState, PopulationCohortState, FIXED_POINT_SCALE, FOOD_CARGO_KEY,
};
use tracing::debug;

use super::{Proposals, Specialist, SpecialistId, SPECIALIST_FOOD};
use crate::geometry::Tile;
use crate::orchestrator::{Alarm, AlarmKind, Plan};
use crate::profile::FoodFloors;
use crate::view::{band_tile, SeatMemory, SeatView, WORKED_DEAD_AT_ONCE};
use ledger::Book;

pub(crate) use sources::{
    crew_take, is_food_site, patch_per_worker_yield, workable_patch_at, SourceKey,
};
use sources::{hunting_kits_held, Source};

/// The `assign_labor` roles this specialist staffs — the `kind` vocabulary of
/// `LaborAssignmentState` (`sim_runtime/src/command_text.rs`).
pub const ROLE_FORAGE: &str = "forage";
/// Also the kit job a herd is worked under — `KitOptionState::jobs` names the verbs by the same
/// words as the labor roles (*"any of `"hunt"`, `"forage"`, `"scout"`, `"warrior"`"*).
pub const ROLE_HUNT: &str = "hunt";
/// The band-wide build pool: `assign_labor <faction> <band> builders <n>`, whose whole output goes
/// on the head of the band's build queue (`command_text.rs` → `assign_labor`).
pub const ROLE_BUILDERS: &str = "builders";
/// The intent kinds, one per rule (*split to feed* has two: the split, then the child's settle).
pub const INTENT_ASSIGN: &str = "assign";
pub const INTENT_FEED_MOVE: &str = "feed_move";
pub const INTENT_SPLIT: &str = "split";
pub const INTENT_SETTLE: &str = "settle";
pub const INTENT_HUNT: &str = "hunt";
pub const INTENT_UPGRADE: &str = "upgrade";

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
                // The sim's own verdict, kept as `WORKED_DEAD_AT_ONCE`: no crew was useful on this
                // row, so it carries no per-worker measurement to hold against the forecast.
                realized.worked_turns == WORKED_DEAD_AT_ONCE
                    || (realized.worked_turns >= self.floors.dead_row_turns
                        && realized.per_worker.is_some_and(|per_worker| {
                            per_worker < self.floors.poor_yield_fraction * forecast
                        }))
            })
    }

    /// The rate to rank `key` on: what the band realized **on that source**; else what the seat has
    /// realized across that web (a hunt's published rate is not what a bare-handed crew takes, and a
    /// seat that has measured one herd knows that about the next); else `forecast`.
    ///
    /// ⛔ **The web's prior may weigh a source down, never veto it.** `best_source` drops anything a
    /// crew would take nothing from, so a prior of `0.0` would strike out every source of its kind —
    /// one species' bad turn condemning the whole web, permanently, since the mean does not decay.
    /// A prior that is not positive therefore says nothing, and the source falls back to its own
    /// forecast. (A row nobody was useful on no longer reaches the prior at all: it is not folded in
    /// as a measurement, [`SeatMemory::observe`].)
    pub(crate) fn rate(
        memory: &SeatMemory,
        band: &PopulationCohortState,
        key: &SourceKey,
        forecast: f32,
    ) -> f32 {
        memory
            .realized(&key.row_key(band.band_id))
            .and_then(|realized| realized.per_worker)
            .or_else(|| {
                memory
                    .realized_for_kind(key.role())
                    .filter(|prior| *prior > 0.0)
            })
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

    /// **What a worked `row` pays this band per worker today**: the frame's own `actual_yield`
    /// over the crew — the income a hand leaving it costs. Not [`Food::rate`], which is what a
    /// *next* assignment is expected to take and falls back to a forecast: a row nobody is useful
    /// on realizes nothing, and leaving it costs nothing, whatever the herd was forecast to pay.
    fn row_rate(row: &LaborAssignmentState) -> f32 {
        if row.workers == 0 {
            0.0
        } else {
            row.actual_yield / row.workers as f32
        }
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
                    && is_food_site(view, tile)
                    && grid.distance(here, tile) <= band.work_range
            })
            .map(|patch| Source {
                key: SourceKey::Patch(Tile::new(patch.x, patch.y)),
                tile: Tile::new(patch.x, patch.y),
                per_worker_yield: patch_per_worker_yield(memory, band, patch),
                ceiling: patch.biomass * patch.provisions_per_biomass,
            })
            .filter(|source| {
                let forecast = Self::forecast_for(view, &source.key).unwrap_or_default();
                !self.is_dead(memory, band, &source.key, forecast)
            })
            .collect();
        // A herd is a source only for a band holding hunting gear ([`hunting_kits_held`]).
        let kits_held = hunting_kits_held(view, band);
        sources.extend(
            view.snapshot
                .herds
                .iter()
                .filter(|_| kits_held > 0)
                .filter(|herd| herd.huntable && herd.per_worker_yield > 0.0)
                .filter(|herd| grid.distance(here, Tile::new(herd.x, herd.y)) <= band.hunt_reach)
                .map(|herd| {
                    let key = SourceKey::Herd(herd.id.clone());
                    Source {
                        per_worker_yield: Self::rate(memory, band, &key, herd.per_worker_yield),
                        key,
                        tile: Tile::new(herd.x, herd.y),
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
            hunting_kits_held = kits_held,
            patches_in_frame = view.snapshot.forage_patches.len(),
            visibility_here = view.visibility(here),
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

    /// The source a crew of `hands` takes the most from, other than any in `except` (the rows
    /// the crew is leaving); none when nothing in reach would pay.
    fn best_source<'s>(
        sources: &'s [Source],
        hands: u32,
        except: &[SourceKey],
    ) -> Option<&'s Source> {
        sources
            .iter()
            .filter(|source| !except.contains(&source.key))
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
        self.assign_role(band, role, workers, target_x, target_y, fauna_id)
    }

    /// `assign_labor` on a band-wide role (`builders`, …): no target, no kit, no floor.
    fn assign_pool(
        &self,
        band: &PopulationCohortState,
        role: &str,
        workers: u32,
    ) -> CommandPayload {
        self.assign_role(band, role, workers, None, None, None)
    }

    /// `policy` is left `None`: the field is **retired** — *"a labor assignment carries a `floor`,
    /// not a stance … the server ignores it"* (`CommandPayload::AssignLabor::policy`); the balanced
    /// take is the sim's default floor, which `floor: None` selects.
    fn assign_role(
        &self,
        band: &PopulationCohortState,
        role: &str,
        workers: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        fauna_id: Option<String>,
    ) -> CommandPayload {
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

    /// Workers on the band-wide `role` pool.
    fn workers_in_pool(band: &PopulationCohortState, role: &str) -> u32 {
        band.labor_assignments
            .iter()
            .filter(|row| row.kind == role)
            .map(|row| row.workers)
            .sum()
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

    /// The band's food book off the frame: the larder (`stores[FOOD_CARGO_KEY]`, the wire's
    /// fixed-point divided out as the scoreboard does), its income and its consumption.
    fn book(band: &PopulationCohortState) -> Book {
        let raw: i64 = band
            .stores
            .iter()
            .filter(|store| store.item == FOOD_CARGO_KEY)
            .map(|store| store.quantity)
            .sum();
        Book {
            stock: raw as f32 / FIXED_POINT_SCALE as f32,
            income: band.food_income,
            consumption: band.food_consumption,
        }
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

    /// The five rules per own band, in the table's order. Each yields at most one proposal per
    /// band; the arbiter's one-order-per-band rule keeps one, ranked by goal gap closed.
    fn propose(&mut self, view: &SeatView, plan: &Plan, memory: &SeatMemory) -> Proposals {
        let mut out = Proposals {
            proposals: Vec::new(),
            alarm: self.alarm(view),
        };
        for band in view.own_bands(self.faction) {
            let (assign, carried) = self.assess_income(view, plan, memory, band);
            out.proposals.extend(assign);
            out.proposals
                .extend(self.feed_while_moving(view, plan, memory, band));
            out.proposals
                .extend(self.split_to_feed(view, plan, memory, band, &carried));
            out.proposals.extend(self.settle(view, plan, memory, band));
            out.proposals
                .extend(self.spare_hands_into_hunts(view, plan, memory, band, &carried));
            out.proposals
                .extend(self.upgrade_the_ground(view, plan, memory, band, &carried));
        }
        out
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::orchestrator::{FoodGoals, Goals, GroundRung};
    use crate::profile::{AiProfiles, NO_MEMORY_DECAY};
    use crate::view::{VISIBILITY_ACTIVE, VISIBILITY_DISCOVERED};
    use sim_runtime::{
        BandKitTiersState, CohortStoreState, ForagePatchState, HerdTelemetryState, KitOptionState,
        WorldSnapshot,
    };

    pub const FACTION: u32 = 3;
    pub const BAND: u64 = 7001;
    pub const TICK: u64 = 41;
    /// The band stands here; the raster is 8×6, wrapped, everything discovered.
    pub const HERE: Tile = Tile::new(3, 2);
    pub const WORK_RANGE: u32 = 2;
    pub const HUNT_REACH: u32 = 5;
    pub const NEAR_PATCH: Tile = Tile::new(4, 2);
    pub const RICH_PATCH: Tile = Tile::new(2, 3);
    pub const FAR_PATCH: Tile = Tile::new(7, 5);
    pub const HERD_ID: &str = "herd_9";
    pub const HERD_AT: Tile = Tile::new(5, 4);
    /// The band's book: fourteen turns in the larder at what thirty people eat, earning nothing
    /// yet — the runway the fixture's `turns_of_food` states.
    pub const CONSUMPTION: f32 = 6.0;
    pub const STOCK: f32 = 84.0;
    /// The turns a pending split waits for its child in these tests.
    pub const SETTLE: u32 = 3;
    /// The sim's split floors as the band echoes them (`expedition_config.json` → `settle`):
    /// the working-age the child must clear, and what the parent must keep.
    pub const FOUNDING_FLOOR: u32 = 4;
    pub const PARENT_FLOOR: u32 = 6;
    /// The roster's hunting kit and the weapon it carries (`equipment.json` → `big_game`); the
    /// fixture band's tier under it is a fresh spear's, so a herd is a source for it
    /// ([`hunting_kits_held`]).
    pub const HUNT_KIT: &str = "big_game";
    pub const HUNT_KIT_ITEM: &str = "spears";
    /// The roster's gathering kit — a kit whose items are not hunting gear.
    pub const FORAGE_KIT: &str = "gathering";
    pub const FORAGE_KIT_ITEM: &str = "baskets";
    /// The roster's item-less kit, offered on every job — the bare hand's reading.
    pub const BARE_KIT: &str = "none";
    /// The `creatures.json` `person` row's attack, which every kit resolves to with nothing live
    /// in it; and what a fresh spear grants (`PopulationCohortState::hunter_attack`'s two readings).
    pub const BARE_ATTACK: f32 = 1.0;
    pub const ARMED_ATTACK: f32 = 20.0;

    /// What `kit` grants the fixture band: `attack` and nothing else.
    pub fn kit_tier(kit: &str, attack: f32) -> BandKitTiersState {
        BandKitTiersState {
            kit_id: kit.to_owned(),
            attack,
            ..Default::default()
        }
    }

    pub fn memory() -> SeatMemory {
        SeatMemory::new(NO_MEMORY_DECAY, SETTLE)
    }

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
            food_income: 0.0,
            food_consumption: CONSUMPTION,
            stores: vec![CohortStoreState {
                item: FOOD_CARGO_KEY.to_owned(),
                quantity: (STOCK * FIXED_POINT_SCALE as f32) as i64,
            }],
            is_traveling: false,
            is_expedition: false,
            labor_assignments: Vec::new(),
            founding_min_workers: FOUNDING_FLOOR,
            founding_parent_min_workers: PARENT_FLOOR,
            kit_tiers: vec![
                kit_tier(HUNT_KIT, ARMED_ATTACK),
                kit_tier(FORAGE_KIT, BARE_ATTACK),
                kit_tier(BARE_KIT, BARE_ATTACK),
            ],
            ..Default::default()
        }];
        let kit = |id: &str, jobs: &[&str], items: &[&str]| KitOptionState {
            id: id.to_owned(),
            jobs: jobs.iter().map(|job| (*job).to_owned()).collect(),
            item_ids: items.iter().map(|item| (*item).to_owned()).collect(),
            ..Default::default()
        };
        snapshot.kits = vec![
            kit(HUNT_KIT, &[ROLE_HUNT], &[HUNT_KIT_ITEM]),
            kit(FORAGE_KIT, &[ROLE_FORAGE], &[FORAGE_KIT_ITEM]),
            kit(BARE_KIT, &[ROLE_HUNT, ROLE_FORAGE], &[]),
        ];
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
        // No faction has learned anything: the fixture's saturated rows are for nobody.
        snapshot.intensification_knowledge.clear();
        SeatView {
            snapshot,
            last_acted_tick: None,
        }
    }

    /// The forager's goals, as the fixture plans carry them.
    pub fn goals() -> FoodGoals {
        FoodGoals::from_levers(&AiProfiles::builtin().profile("forager").unwrap().goals)
    }

    /// A plan funding Food at `share` of the pool, priority 1, with the forager's goals.
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
            goals: std::collections::BTreeMap::from([(SPECIALIST_FOOD, Goals::Food(goals()))]),
            since_turn: TICK,
        }
    }

    /// `plan_with_food_share`, toward `rung`.
    pub fn plan_toward(share: f32, rung: GroundRung) -> Plan {
        let mut plan = plan_with_food_share(share);
        plan.goals.insert(
            SPECIALIST_FOOD,
            Goals::Food(FoodGoals {
                ground_rung: rung,
                ..goals()
            }),
        );
        plan
    }

    pub fn food() -> Food {
        let profile = AiProfiles::builtin().profile("forager").unwrap().clone();
        Food::new(FACTION, profile.food, profile.weight("food_security"))
    }

    pub fn own_band(view: &SeatView) -> &PopulationCohortState {
        view.own_bands(FACTION).next().unwrap()
    }

    #[test]
    fn the_alarm_is_the_runway_floor() {
        let mut view = a_view();
        view.snapshot.populations[0].turns_of_food = 2.0;
        assert_eq!(
            food().alarm(&view).map(|alarm| alarm.kind),
            Some(AlarmKind::FoodShort)
        );
        view.snapshot.populations[0].turns_of_food = 20.0;
        assert!(food().alarm(&view).is_none());
    }

    #[test]
    fn the_book_is_read_off_the_frame_in_the_scoreboards_units() {
        let view = a_view();
        let book = Food::book(own_band(&view));
        assert_eq!(book.stock, STOCK);
        assert_eq!(book.consumption, CONSUMPTION);
        assert_eq!(book.income, 0.0);
    }

    /// Under the pass-through plan — no budget, no goals — no rule proposes anything: the scripted
    /// brain that plan serves has no `Food` specialist, and a goal-less rule has nothing to score.
    #[test]
    fn the_pass_through_plan_proposes_nothing_from_any_rule() {
        let mut view = a_view();
        view.snapshot.populations[0].turns_of_food = 2.0;
        let proposals = food().propose(&view, &Plan::pass_through(TICK), &memory());
        assert!(proposals.proposals.is_empty(), "{:?}", proposals.proposals);
        // Budget without goals is the same silence.
        let mut plan = plan_with_food_share(1.0);
        plan.goals.clear();
        let proposals = food().propose(&view, &plan, &memory());
        assert!(proposals.proposals.is_empty(), "{:?}", proposals.proposals);
    }
}
