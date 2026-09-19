//! **`Food`** — the food loop (`docs/plan_ai_driver.md` §4). Owns `runway_turns`; alarms
//! `FoodShort` when the minimum own-band runway is below the profile's `food.runway_floor_turns`.
//!
//! Seven rules ([`rules`]), each a pure function of one band, the view, the plan's goals and the
//! memory, each producing at most one proposal per band whose `reason` names the rule and its
//! subject — the §4 table, as shipped, in `propose` order:
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
//! - *hold the ground* — an owned patch whose standing upkeep reads short gets the band's
//!   `agriculture` pool sized to the summed plant bill; on a completed rung it is a standing bill.
//! - *upgrade the ground* — the rung known and a worked patch below it: declare the climb and
//!   staff the builders, priced by the projection ledger ([`ledger`]).
//! - *draw down to survive* — the plan in force troughs at or below zero: lower a worked forage
//!   row's harvest floor to the highest that survives, and put it back to Best once the runway
//!   clears.
//!
//! **The goal gap is the score.** Every rule projects the band's book under its change and scores
//! the progress that projection makes toward the plan's goals ([`ledger::goal_progress`]), times
//! this specialist's weight — so the arbiter ranks by goal gap closed, and a rule that needs a goal
//! and is handed none (the pass-through plan) proposes nothing.
//!
//! **Sources are ranked by the crew's expected take, not the per-worker rate** ([`sources`]). A
//! patch: the frame's `per_worker_yield` is a rate; what a crew of `n` takes is `min(n × rate,
//! ceiling)`, and the ceiling is composed from terms on the wire — `biomass ×
//! provisions_per_biomass`, the take at a zero escapement floor (`ForagePatchState::per_worker_yield`
//! docs). The sim's default floor is not published, so this is an upper bound; a herd of a few
//! animals still ranks below a stand the whole band can gather, which is the failure a per-worker
//! ranking walked into. A herd: **the sim's own crew-take curve** ([`crate::oracle`]), asked
//! through the link and cached in `SeatMemory` — its `per_worker_yield` on the wire is the kit's
//! carry, not a kill rate, and a herd the sim has not answered for is not a source.
//!
//! Every assignment is `assign_labor` with the kit left to the frame's default (`None` means the
//! job's default on the wire) — a specialist names no number the sim already owns. The floor is
//! the default too, except where *draw down to survive* states one, and a rule re-issuing a worked
//! row carries that row's floor ([`Food::assign`]).

pub mod ledger;
mod rules;
mod sources;

pub(crate) use rules::Climb;

use sim_runtime::{
    CommandPayload, HerdTelemetryState, LaborAssignmentState, PopulationCohortState,
    FIXED_POINT_SCALE, FOOD_CARGO_KEY,
};
use tracing::debug;

use super::{Proposal, Proposals, Specialist, SpecialistId, SPECIALIST_FOOD};
use crate::geometry::Tile;
use crate::ground::GroundReadings;
use crate::orchestrator::{Alarm, AlarmKind, Plan};
use crate::profile::FoodFloors;
use crate::view::{band_tile, row_key, SeatMemory, SeatView, WORKED_DEAD_AT_ONCE};
use ledger::{Book, Reassignment, BEST_FLOOR};

pub(crate) use sources::{
    best_sustained_cluster_within, cluster_take, cluster_take_sustained, foreign_band_at,
    herd_kit_id, honest_ceiling, is_food_site, is_walkable, kit_units_held, patch_per_worker_yield,
    sustained_hands, workable_patch_at, ClusterTake, IsDead, SourceKey,
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
/// **The role that pays a tended patch's standing upkeep** — the job the upkeep row's kit
/// (`tillage`, jobs `builders` + `agriculture`) serves; `assign_labor … agriculture <n>`, a
/// band-wide pool like `builders`.
pub const ROLE_AGRICULTURE: &str = "agriculture";
/// The intent kinds, one per rule (*split to feed* has two: the split, then the child's settle).
pub const INTENT_ASSIGN: &str = "assign";
pub const INTENT_FEED_MOVE: &str = "feed_move";
pub const INTENT_SPLIT: &str = "split";
pub const INTENT_SETTLE: &str = "settle";
pub const INTENT_HUNT: &str = "hunt";
pub const INTENT_UPGRADE: &str = "upgrade";
pub const INTENT_DRAWDOWN: &str = "drawdown";
pub const INTENT_HOLD: &str = "hold";

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

    /// Whether `band`'s row on `key` is **dead**: its window has run and it realized under
    /// `poor_yield_fraction` of what it was forecast over it (`Realized::dead_under` in
    /// `view.rs` — the window is the turns one kill takes at the row's crew,
    /// `food.dead_row_turns` on a patch) — or the sim
    /// has said no crew is useful on it (`WORKED_DEAD_AT_ONCE`).
    ///
    /// ⛔ **A dead patch is dead while it stands at its floor.** A patch pays nothing for a turn
    /// when it has been stripped to the Best floor and its regrowth is a hair — the frame's own
    /// word, `biomass ≤ BEST_FLOOR × carrying_capacity` — and that is what the verdict says;
    /// once the stand has regrown above the floor there is food to take again and the record no
    /// longer condemns it. Kept for good, the verdict blacklisted every stripped patch in reach
    /// one by one (seed 50: 60,9 at t7 and t9), rule 1 fell silent from t14 to t45 with income
    /// under consumption, and the band starved from a larder of 54. A herd's verdict stands as
    /// long as its record does: its forecast is the curve, not the stand.
    fn is_dead(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        key: &SourceKey,
    ) -> bool {
        let poor = memory
            .realized(&key.row_key(band.band_id))
            .is_some_and(|realized| realized.dead_under(self.floors.poor_yield_fraction));
        match key {
            SourceKey::Patch(tile) => {
                poor && view
                    .patch_at(*tile)
                    .is_none_or(|patch| patch.biomass <= BEST_FLOOR * patch.carrying_capacity)
            }
            SourceKey::Herd(_) => poor,
        }
    }

    /// The row's accounting for a reason (`took R of E expected over W turns`), when the band
    /// has a record of it.
    fn accounting(memory: &SeatMemory, band: &PopulationCohortState, key: &SourceKey) -> String {
        memory
            .realized(&key.row_key(band.band_id))
            .map(|realized| realized.accounting())
            .unwrap_or_default()
    }

    /// The rate to rank a **patch** under `key` on: what the band realized **on that source**;
    /// else what the seat has realized across that web; else `forecast`. **A herd is ranked on
    /// the sim's crew-take curve, never on what it realized** — a hunt pays in whole animals, so
    /// a zero turn is a turn before the kill, not a rate — and this answers `forecast` for one.
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
        if matches!(key, SourceKey::Herd(_)) {
            return forecast;
        }
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

    /// **What a worked `row` pays this band per worker today** — the income a hand leaving it
    /// costs, and the figure a row is "lowest" by. A **patch** row: the frame's own
    /// `actual_yield` over the crew. A **hunt** row pays in whole animals, so its turn's take is
    /// not a rate: while its window runs (`worked_turns < window`, [`crate::view::Realized`]) it
    /// is priced at its forecast — the curve's likely at its crew, per hand — and once the window
    /// has run at what it realized over it (`realized_sum / worked_turns`, per hand); a row
    /// nobody was useful on (`WORKED_DEAD_AT_ONCE`), or one with no curve, at its `actual_yield`
    /// like a patch. Not [`Food::rate`], which is what a *next* assignment is expected to take.
    /// Priced per turn, seed 54's one hunter read as the lowest row on every zero turn and was
    /// bounced onto a full patch and back every other turn from t26 to t45, each bounce the
    /// band's one order, so the upgrade it should have made was `conflict` for twenty turns.
    fn row_rate(
        memory: &SeatMemory,
        band: &PopulationCohortState,
        row: &LaborAssignmentState,
    ) -> f32 {
        if row.workers == 0 {
            return 0.0;
        }
        let crew = row.workers as f32;
        if row.kind == ROLE_HUNT {
            let record = memory.realized(&Self::row_key_of(band.band_id, row));
            let curve = memory.crew_take(band.band_id, &row.fauna_id);
            if let (Some(record), Some(curve)) = (record, curve) {
                if record.worked_turns != WORKED_DEAD_AT_ONCE && record.window > 0 {
                    return if record.worked_turns < record.window {
                        curve.likely(row.workers) / crew
                    } else {
                        record.realized_sum / record.worked_turns as f32 / crew
                    };
                }
            }
        }
        row.actual_yield / crew
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
                crew_cap: None,
                curve: None,
            })
            .filter(|source| !self.is_dead(view, memory, band, &source.key))
            .collect();
        // A herd is a source only for a band holding a unit of the kit that herd is hunted
        // under ([`herd_kit_id`], [`kit_units_held`]), and credits no more hands than the units
        // it holds **less the hands the sim reads as useful on other herds under the same kit**
        // (`min(workers, hunt_useful_workers)` per row — a row no crew is useful on is the row
        // rule 1 empties first, and its hands free their units). The units are the band's, not
        // the herd's, so one spear arms one hunter on one herd, not one on each: the band-wide
        // "holds any hunting kit" reading sent twelve hands after one spear, and the per-herd
        // count alone sent a second hunter to the aurochs while the first stood on the deer
        // (bench seed 19, t2–t3). **And only once the sim has answered for it**: a herd's
        // forecast is the crew-take curve in memory (`SeatMemory::crew_take`), never the row's
        // `per_worker_yield` — that is the kit's carry (0.8 on every herd in view), and ranked on
        // it five hunters read 4.0 a turn off a boar that paid 0.24 (seed 54). No curve, no
        // source, this turn.
        let kits_held = hunting_kits_held(view, band);
        let kit_of_row = |row: &LaborAssignmentState| {
            view.snapshot
                .herds
                .iter()
                .find(|herd| herd.id == row.fauna_id)
                .map(|herd| herd_kit_id(view, herd))
        };
        let committed_elsewhere = |herd: &HerdTelemetryState, kit: &str| -> u32 {
            band.labor_assignments
                .iter()
                .filter(|row| row.kind == ROLE_HUNT && row.fauna_id != herd.id)
                .filter(|row| kit_of_row(row) == Some(kit))
                .map(|row| row.workers.min(row.hunt_useful_workers))
                .sum()
        };
        sources.extend(
            view.snapshot
                .herds
                .iter()
                .filter(|herd| herd.huntable && herd.per_worker_yield > 0.0)
                .filter(|herd| grid.distance(here, Tile::new(herd.x, herd.y)) <= band.hunt_reach)
                .filter_map(|herd| {
                    let kit = herd_kit_id(view, herd);
                    let units = kit_units_held(view, band, kit)
                        .map(|units| units.saturating_sub(committed_elsewhere(herd, kit)));
                    if units == Some(0) {
                        return None;
                    }
                    let curve = memory.crew_take(band.band_id, &herd.id)?.clone();
                    let plateau = curve.plateau();
                    let key = SourceKey::Herd(herd.id.clone());
                    Some(Source {
                        per_worker_yield: curve.likely(1),
                        key,
                        tile: Tile::new(herd.x, herd.y),
                        ceiling: herd.biomass * herd.provisions_per_biomass,
                        crew_cap: Some(units.map_or(plateau, |units| units.min(plateau))),
                        curve: Some(curve),
                    })
                })
                .filter(|source| !self.is_dead(view, memory, band, &source.key)),
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
                .map(|source| (source.key.describe(), source.per_worker_yield, source.ceiling, source.crew_cap))
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
        // ⛔ **A WORKED ROW'S FLOOR RIDES ALONG.** `floor: None` is the wire default, Best — so a
        // rule re-issuing a row it is reducing or topping up would silently undo *draw down to
        // survive* (bench seed 23: the t32 upgrade's `assign_labor forage 47,5 2` put the row it
        // had drawn down at t31 back to Best, and t34 drew it down again). A row the band does
        // not work yet has no floor to keep and takes the default.
        let floor = band
            .labor_assignments
            .iter()
            .find(|row| row.workers > 0 && SourceKey::of_row(row).as_ref() == Some(key))
            .map(|row| row.floor);
        self.assign_role(band, role, workers, target_x, target_y, fauna_id, floor)
    }

    /// `assign_labor` on a band-wide role (`builders`, …): no target, no kit, no floor.
    fn assign_pool(
        &self,
        band: &PopulationCohortState,
        role: &str,
        workers: u32,
    ) -> CommandPayload {
        self.assign_role(band, role, workers, None, None, None, None)
    }

    /// `assign_labor` on a forage patch **at a stated harvest floor** — *draw down to survive*'s
    /// command; `None` is the wire default, Best.
    fn assign_at_floor(
        &self,
        band: &PopulationCohortState,
        tile: Tile,
        workers: u32,
        floor: Option<f32>,
    ) -> CommandPayload {
        self.assign_role(
            band,
            ROLE_FORAGE,
            workers,
            Some(tile.x),
            Some(tile.y),
            None,
            floor,
        )
    }

    /// `policy` is left `None`: the field is **retired** — *"a labor assignment carries a `floor`,
    /// not a stance … the server ignores it"* (`CommandPayload::AssignLabor::policy`); the balanced
    /// take is the sim's default floor, which `floor: None` selects — every rule but *draw down to
    /// survive* leaves it so.
    #[allow(clippy::too_many_arguments)]
    fn assign_role(
        &self,
        band: &PopulationCohortState,
        role: &str,
        workers: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        fauna_id: Option<String>,
        floor: Option<f32>,
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
            floor,
            kit_id: None,
            take_species: Vec::new(),
        }
    }

    /// The memory key of `row` on `band_id` ([`row_key`]).
    fn row_key_of(band_id: u64, row: &LaborAssignmentState) -> String {
        row_key(
            band_id,
            &row.kind,
            row.target_x,
            row.target_y,
            &row.fauna_id,
        )
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

    /// The seven rules per own band, in the table's order. Each yields at most one proposal per
    /// band; the arbiter, ranking by goal gap closed, keeps every one whose claims — the labor
    /// rows it sets, the band's move, its split (`Cost::claimed`) — no higher-ranked proposal has
    /// taken. `ground` is the turn's land reading per band (`ground::read_all`); only the outfit's
    /// hoe estimate reads it — the rules do not consume it.
    fn propose(
        &mut self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        ground: &GroundReadings,
    ) -> Proposals {
        let mut out = Proposals {
            proposals: Vec::new(),
            alarm: self.alarm(view),
            demands: Vec::new(),
        };
        for band in view.own_bands(self.faction) {
            out.demands
                .extend(self.outfit_demands(view, memory, band, ground.get(&band.band_id)));
            // **The plan in force**: the band's book plus every change rules 1–5 propose this
            // turn, which is what *draw down to survive* projects.
            let (assign, carried) = self.assess_income(view, plan, memory, band);
            out.proposals.extend(assign);
            out.proposals.extend(self.settle(view, plan, memory, band));
            // *Hold the ground* comes before *upgrade the ground*: holding what the band has
            // beats declaring the next rung.
            let ruled: [Option<(Proposal, Reassignment)>; 5] = [
                self.feed_while_moving_change(view, plan, memory, band),
                self.split_to_feed_change(view, plan, memory, band, &carried),
                self.spare_hands_into_hunts_change(view, plan, memory, band, &carried),
                self.hold_the_ground_change(view, plan, memory, band, &carried),
                self.upgrade_the_ground_change(view, plan, memory, band, &carried),
            ];
            let mut in_force = vec![carried];
            for (proposal, change) in ruled.into_iter().flatten() {
                out.proposals.push(proposal);
                in_force.push(change);
            }
            out.proposals
                .extend(self.draw_down_to_survive(view, plan, memory, band, &in_force));
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
        BandKitTiersState, CohortStoreState, EquipmentBatchState, ForagePatchState,
        HerdTelemetryState, KitOptionState, WorldSnapshot,
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
    /// fixture band's tier under it is a fresh spear's ([`hunting_kits_held`]), and it holds
    /// [`HUNT_KIT_UNITS`] of the weapon, so the herd is a source for a crew that size
    /// ([`kit_units_held`]).
    pub const HUNT_KIT: &str = "big_game";
    pub const HUNT_KIT_ITEM: &str = "spears";
    /// Spears enough for the whole band: the fixture's herd is bounded by its animals, not its
    /// gear, unless a test says otherwise.
    pub const HUNT_KIT_UNITS: u32 = 17;
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

    /// The forager's `food.dead_row_turns`, the window a patch row is judged over.
    pub const PATCH_WINDOW: u32 = 4;

    pub fn memory() -> SeatMemory {
        SeatMemory::new(NO_MEMORY_DECAY, SETTLE, PATCH_WINDOW)
    }

    /// One animal of the fixture's herd is one food: a kill window of one turn at any crew
    /// that takes a whole food.
    pub const FIXTURE_BODY_FOOD: f32 = 1.0;

    /// **A memory the sim has answered for every herd in `view`** — with the curve the linear
    /// model read before the oracle existed: `min(n × per_worker_yield, biomass ×
    /// provisions_per_biomass)` for `n` in `1..=HUNT_KIT_UNITS`. The fixture's herd (1.5 a hand,
    /// a ceiling of its biomass) keeps its arithmetic, with the curve in place of the rate.
    pub fn memory_forecasting(view: &SeatView) -> SeatMemory {
        let mut memory = memory();
        for herd in &view.snapshot.herds {
            let ceiling = herd.biomass * herd.provisions_per_biomass;
            let points: Vec<(u32, f32)> = (1..=HUNT_KIT_UNITS)
                .map(|n| (n, (n as f32 * herd.per_worker_yield).min(ceiling)))
                .collect();
            memory.remember_crew_take(
                BAND,
                &herd.id,
                crate::oracle::curve_of(&points, FIXTURE_BODY_FOOD),
            );
        }
        memory
    }

    /// The raster the fixture world is: 8 wide, 6 high, wrapped.
    pub const RASTER_WIDTH: u32 = 8;
    pub const RASTER_HEIGHT: u32 = 6;

    /// The saturated fixture, pinned: one own band with idle hands, three patches (near, rich,
    /// out of reach) and one huntable herd, the raster all discovered and all dry land.
    pub fn a_view() -> SeatView {
        let mut snapshot: WorldSnapshot =
            sim_runtime::fixture::saturated_snapshot().expect("the fixture builds");
        snapshot.header.tick = TICK;
        snapshot.header.wrap_horizontal = true;
        snapshot.visibility_raster.width = RASTER_WIDTH;
        snapshot.visibility_raster.height = RASTER_HEIGHT;
        snapshot.visibility_raster.samples =
            vec![VISIBILITY_DISCOVERED; (RASTER_WIDTH * RASTER_HEIGHT) as usize];
        // Every tile a walkable row: a band may stand anywhere on the fixture.
        let land = snapshot.tiles[0].clone();
        snapshot.tiles = (0..RASTER_WIDTH * RASTER_HEIGHT)
            .map(|index| sim_runtime::TileState {
                x: index % RASTER_WIDTH,
                y: index / RASTER_WIDTH,
                terrain_tags: sim_runtime::TerrainTags::empty(),
                ..land.clone()
            })
            .collect();
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
            equipment_batches: vec![EquipmentBatchState {
                item_id: HUNT_KIT_ITEM.to_owned(),
                count: HUNT_KIT_UNITS,
                ..Default::default()
            }],
            ..Default::default()
        }];
        let kit = |id: &str, jobs: &[&str], items: &[&str], attack: f32| KitOptionState {
            id: id.to_owned(),
            jobs: jobs.iter().map(|job| (*job).to_owned()).collect(),
            item_ids: items.iter().map(|item| (*item).to_owned()).collect(),
            attack,
            ..Default::default()
        };
        snapshot.kits = vec![
            kit(HUNT_KIT, &[ROLE_HUNT], &[HUNT_KIT_ITEM], ARMED_ATTACK),
            kit(FORAGE_KIT, &[ROLE_FORAGE], &[FORAGE_KIT_ITEM], BARE_ATTACK),
            kit(BARE_KIT, &[ROLE_HUNT, ROLE_FORAGE], &[], BARE_ATTACK),
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
            default_kit_id: HUNT_KIT.to_owned(),
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
        let proposals = food().propose(
            &view,
            &Plan::pass_through(TICK),
            &memory(),
            &GroundReadings::default(),
        );
        assert!(proposals.proposals.is_empty(), "{:?}", proposals.proposals);
        // Budget without goals is the same silence.
        let mut plan = plan_with_food_share(1.0);
        plan.goals.clear();
        let proposals = food().propose(&view, &plan, &memory(), &GroundReadings::default());
        assert!(proposals.proposals.is_empty(), "{:?}", proposals.proposals);
    }
}
