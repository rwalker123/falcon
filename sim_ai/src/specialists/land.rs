//! **`Land`** — where the people are (`docs/plan_ai_driver.md` §4). Owns `patches_owned`; alarms
//! `LandShort` when a band's ground cannot feed it and nothing better is in view.
//!
//! - *blind* — fewer than `land.known_tiles_floor` known tiles within `land.horizon_tiles` of a
//!   band posts `land.scout_workers` scouts (`assign_labor … scout <n>`). **The `scout <x> <y>`
//!   verb is retired server-side** (`command.retired=ignored`, `core_sim/src/bin/server.rs`);
//!   scouting is the standing scout role, which posts vantage points around the band.
//! - *better ground* — **positions by the cluster, not the patch** (§4): a standing tile is
//!   ranked by what the band's whole crew would take from **every** workable site within its
//!   `work_range` of that tile ([`cluster_take`], the hands dealt to the best rate first up to each
//!   site's plateau), against the same reading of the tile the band stands on. While the runway
//!   is falling, the discovered, walkable, unoccupied tile within the horizon whose cluster clears
//!   the band's own by the profile's margin proposes `move_band`, with an intent that **persists
//!   until arrival**. One rich patch loses to a tile that reaches four; a band standing beside
//!   the ground it could work is not moved onto it. The rate under the reading is `Food::rate` —
//!   the crew's take, never `carrying_capacity` — and the sites are the ones `Food` will work
//!   (`food::workable_patch_at`), so the two specialists cannot disagree about ground.
//! - *room* — under `Expand`, a band above `land.split_size` on ground the faction owns proposes
//!   `split_band` with half its workers.

use sim_runtime::{CommandPayload, PopulationCohortState};

use super::food::{cluster_take, foreign_band_at, is_walkable, ClusterTake, IsDead};
use super::{
    intent_key, Cost, Memo, Proposal, Proposals, Specialist, SpecialistId, SPECIALIST_LAND,
};
use crate::board::{Demand, Resource};
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
/// **A scout's kit, after food**: a blind band sees farther with it and nothing eats it, so it
/// ranks under both of `Food`'s asks at the board.
const DEMAND_PRIORITY_SCOUT: f32 = 0.5;
/// The roster's scout kit (`equipment.json` → `wayfinding`, jobs `scout`).
const SCOUT_KIT_ID: &str = "wayfinding";

/// **`Land` passes no dead-row judgement** — those are `Food`'s levers (`food.dead_row_turns`,
/// `food.poor_yield_fraction`), and a dead source already reads its realized rate, which is what
/// made it dead, so the cluster weighs it down without a verdict.
const NEVER_DEAD: &IsDead<'static> = &|_, _| false;

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

    /// **What the band would take standing on `tile`**: its whole working-age crew dealt across
    /// every workable site within `work_range` of it ([`cluster_take`]).
    fn cluster_at(
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        tile: Tile,
    ) -> ClusterTake {
        cluster_take(view, memory, band, tile, band.working_age, NEVER_DEAD)
    }

    /// The best standing tile within the horizon, and its cluster reading — the tile whose
    /// cluster clears `own` by `land.better_ground_gain_fraction` of itself, and which is not the
    /// tile the band stands on, not the tile it most recently left ([`SeatMemory::left_from`]),
    /// is walkable ([`is_walkable`]) and under no foreign band. Ties: nearer first, then the
    /// lower `(x, y)`.
    ///
    /// ⛔ **Two guards, because the margin alone did not stop the oscillation.** On bench seed 11
    /// the band walked 20,8 → 18,8 → 20,8 → 18,8, dropping its rows on every arrival: the rate it
    /// ranks on is the *realized* one where it has worked (`patch_per_worker_yield`), and a patch
    /// it has just stripped realizes little, while the tile it left is rated on its last realized
    /// figure — or, once that record thins, the frame's fresh forecast — so the ground behind it
    /// always looked better than the ground under it. The margin refuses a move that buys nearly
    /// nothing; the departure memory refuses the one move the margin cannot judge, back onto the
    /// tile whose reading is a forecast rather than the rate the band is realizing now.
    fn better_cluster(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        own: f32,
    ) -> Option<(Tile, ClusterTake)> {
        let grid = view.grid();
        let here = band_tile(band);
        let left = memory.left_from(band.band_id);
        let margin = self.floors.better_ground_gain_fraction;
        grid.disk(here, self.floors.horizon_tiles)
            .into_iter()
            .filter(|tile| {
                *tile != here
                    && left != Some(*tile)
                    && view.is_discovered(*tile)
                    && is_walkable(view, *tile)
                    && !foreign_band_at(view, self.faction, *tile)
            })
            .map(|tile| (tile, Self::cluster_at(view, memory, band, tile)))
            .filter(|(_, cluster)| {
                cluster.total > own && cluster.total - own >= margin * cluster.total
            })
            .max_by(|(a, a_cluster), (b, b_cluster)| {
                a_cluster
                    .total
                    .total_cmp(&b_cluster.total)
                    .then_with(|| grid.distance(here, *b).cmp(&grid.distance(here, *a)))
                    .then_with(|| (b.x, b.y).cmp(&(a.x, a.y)))
            })
    }

    fn scouts_posted(band: &PopulationCohortState) -> u32 {
        band.labor_assignments
            .iter()
            .filter(|row| row.kind == ROLE_SCOUT)
            .map(|row| row.workers)
            .sum()
    }

    /// The known tiles within the horizon of `band`, and *blind*'s floor.
    fn known_and_floor(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> (u32, u32) {
        let known = memory.known_tiles_within(
            view,
            band_tile(band),
            self.floors.horizon_tiles,
            view.tick(),
        ) as u32;
        (known, self.floors.known_tiles_floor)
    }

    /// **What `Land` asks the board for when `band`'s outfitting window is open**: a scout kit
    /// per scout *blind* would post, when the band is blind (`board.rs`).
    pub fn outfit_demands(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Vec<Demand> {
        let open = band
            .loadout_window
            .as_ref()
            .is_some_and(|window| window.open);
        let (known, floor) = self.known_and_floor(view, memory, band);
        if !open || known >= floor {
            return Vec::new();
        }
        vec![Demand {
            requester: SPECIALIST_LAND,
            band: band.band_id,
            resource: Resource::Kit(SCOUT_KIT_ID.to_owned()),
            amount: self.floors.scout_workers,
            by_tick: view.tick(),
            priority: DEMAND_PRIORITY_SCOUT,
        }]
    }

    /// *Blind*: too few known tiles around the band posts scouts, once.
    pub fn blind(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        let (known, floor) = self.known_and_floor(view, memory, band);
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
            memo: None,
        })
    }

    /// *Better ground*: a tile whose cluster out-takes the band's own while the runway falls —
    /// or the target the band is already walking to, while its cluster still out-takes the
    /// band's own.
    pub fn better_ground(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        let here = band_tile(band);
        let own = Self::cluster_at(view, memory, band, here).total;
        let (target, cluster) = match memory.move_target(band.band_id) {
            // Persist until arrival, while the ground is still worth it.
            Some(target) => {
                if foreign_band_at(view, self.faction, target) {
                    return None;
                }
                let cluster = Self::cluster_at(view, memory, band, target);
                (cluster.total > own).then_some((target, cluster))?
            }
            None => {
                if !memory.runway_falling(band.band_id, band.turns_of_food) {
                    return None;
                }
                self.better_cluster(view, memory, band, own)?
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
            score: (cluster.total - own) / cluster.total * self.weight,
            cost: Cost {
                workers: 0,
                bands: vec![band.band_id],
            },
            reason: format!(
                "{REASON_BETTER_GROUND}: {} sites at {},{} take {:.1}/turn vs {own:.1} here",
                cluster.sites.len(),
                target.x,
                target.y,
                cluster.total
            ),
            memo: Some(Memo::Move {
                band: band.band_id,
                target,
                from: here,
            }),
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
            memo: None,
        })
    }

    /// The alarm: a band's ground cannot feed it and nothing better is in view — both read as
    /// the cluster the band's crew would work from where it stands.
    ///
    /// ⛔ **A rate against a rate.** What the band takes here is provisions *per turn* and
    /// `food_consumption` is what it eats *per turn*; the stock this used to read —
    /// `carrying_capacity`, the standing biomass — is neither, and being two orders of magnitude
    /// larger than a band's appetite it meant the alarm could essentially never fire.
    pub fn alarm(&self, view: &SeatView, memory: &SeatMemory) -> Option<Alarm> {
        let short = view.own_bands(self.faction).any(|band| {
            let own = Self::cluster_at(view, memory, band, band_tile(band)).total;
            own < band.food_consumption && self.better_cluster(view, memory, band, own).is_none()
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
            demands: Vec::new(),
        };
        for band in view.own_bands(self.faction) {
            out.demands.extend(self.outfit_demands(view, memory, band));
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
    use crate::specialists::food::tests::{
        a_view, BAND, FACTION, FAR_PATCH, HERE, RICH_PATCH, SETTLE, TICK, WORK_RANGE,
    };
    use sim_runtime::{ForagePatchState, TerrainTags};
    use std::collections::BTreeMap;

    fn land(profile: &str) -> Land {
        let profile = AiProfiles::builtin().profile(profile).unwrap().clone();
        Land::new(FACTION, profile.land, profile.weight("land_claim"))
    }

    fn plan(stance: Stance) -> Plan {
        Plan {
            stance,
            budgets: BTreeMap::from([(SPECIALIST_LAND, Budget { worker_share: 1.0 })]),
            priorities: BTreeMap::from([(SPECIALIST_LAND, 1.0)]),
            goals: BTreeMap::new(),
            since_turn: TICK,
        }
    }

    fn own_band(view: &SeatView) -> &PopulationCohortState {
        view.own_bands(FACTION).next().unwrap()
    }

    /// Put a food module on `tile`, so a crew sent there is not refused *"nobody gathers here"* —
    /// the eligibility `Food` and `Land` both read (`is_food_site`).
    fn make_a_gathering_site(view: &mut SeatView, tile: Tile) {
        let site = sim_runtime::FoodModuleState {
            x: tile.x,
            y: tile.y,
            ..view
                .snapshot
                .food_modules
                .first()
                .cloned()
                .unwrap_or_default()
        };
        view.snapshot.food_modules.push(site);
    }

    /// A workable patch on `tile` paying `rate` a worker with a take ceiling of `ceiling`
    /// (`biomass × provisions_per_biomass`), so its plateau is `ceil(ceiling / rate)` hands.
    fn add_site(view: &mut SeatView, tile: Tile, rate: f32, ceiling: f32) {
        view.snapshot.forage_patches.push(ForagePatchState {
            x: tile.x,
            y: tile.y,
            owner: None,
            per_worker_yield: rate,
            carrying_capacity: ceiling,
            biomass: ceiling,
            provisions_per_biomass: 1.0,
            ..Default::default()
        });
        make_a_gathering_site(view, tile);
    }

    /// The fixture with none of its patches: bare ground everywhere, the band on `HERE`.
    fn bare() -> SeatView {
        let mut view = a_view();
        view.snapshot.forage_patches.clear();
        view.snapshot.food_modules.clear();
        view
    }

    /// A memory in which the band's runway fell since last turn.
    fn falling(view: &SeatView) -> SeatMemory {
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        memory.observe(view, FACTION);
        let mut before = SeatView {
            snapshot: view.snapshot.clone(),
            last_acted_tick: None,
        };
        before.snapshot.populations[0].turns_of_food += 1.0;
        memory.remember_runways(&before, FACTION);
        memory
    }

    fn move_target(proposal: &Proposal) -> Tile {
        match proposal.commands[0] {
            CommandPayload::MoveBand {
                target_x, target_y, ..
            } => Tile::new(target_x, target_y),
            ref other => panic!("not a move: {other:?}"),
        }
    }

    /// **The spec's fixture.** One rich patch three tiles off — a plateau of five hands paying
    /// 15 — and three poorer patches on the row three tiles the other way, each a plateau of six
    /// paying 1.5 a hand, all of them outside the band's `work_range` of 2 from `HERE`. The
    /// three are spread so that **only** the cluster tile reaches all of them (a nearer tile
    /// reaches two), and the rich patch sits on the far row so no tile reaches it and any of
    /// them: the two readings never mix. (`EAST` is three steps from `HERE` on the raster's
    /// top row; `NORTH` three on its bottom row.)
    const EAST: Tile = Tile::new(5, 0);
    const NORTH: Tile = Tile::new(3, 5);
    const NORTH_SITES: [Tile; 3] = [Tile::new(1, 5), Tile::new(3, 5), Tile::new(5, 5)];

    fn a_cluster_north_and_a_rich_patch_east() -> SeatView {
        let mut view = bare();
        add_site(&mut view, EAST, 3.0, 15.0);
        for site in NORTH_SITES {
            add_site(&mut view, site, 1.5, 9.0);
        }
        let grid = view.grid();
        for site in NORTH_SITES.iter().chain([EAST].iter()) {
            assert!(
                grid.distance(HERE, *site) > WORK_RANGE,
                "{site:?} is out of reach"
            );
        }
        for site in NORTH_SITES {
            assert!(grid.distance(NORTH, site) <= WORK_RANGE);
        }
        view
    }

    #[test]
    fn a_blind_band_posts_scouts_and_a_seeing_one_does_not() {
        let view = a_view();
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
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

    /// A blind band with its window open asks the board for a scout kit per scout *blind*
    /// posts; a seeing band, or a closed window, asks for nothing.
    #[test]
    fn a_blind_band_asks_the_board_for_a_scout_kit_and_a_seeing_one_does_not() {
        let mut view = a_view();
        view.snapshot.populations[0].loadout_window = Some(sim_runtime::BandLoadoutWindowState {
            open: true,
            kit_budget: 17,
            ..Default::default()
        });
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        let demands = land("forager").outfit_demands(&view, &memory, own_band(&view));
        assert_eq!(demands.len(), 1);
        assert_eq!(demands[0].resource, Resource::Kit("wayfinding".to_owned()));
        assert_eq!(demands[0].amount, 1, "the forager posts one scout");
        assert_eq!(demands[0].requester, SPECIALIST_LAND);
        assert_eq!(demands[0].by_tick, TICK);
        assert_eq!(demands[0].priority, DEMAND_PRIORITY_SCOUT);
        memory.observe(&view, FACTION);
        assert!(land("forager")
            .outfit_demands(&view, &memory, own_band(&view))
            .is_empty());
        view.snapshot.populations[0].loadout_window = None;
        let blind = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        assert!(land("forager")
            .outfit_demands(&view, &blind, own_band(&view))
            .is_empty());
    }

    #[test]
    fn scouts_already_posted_are_not_posted_again() {
        let mut view = a_view();
        view.snapshot.populations[0].labor_assignments = vec![sim_runtime::LaborAssignmentState {
            kind: ROLE_SCOUT.into(),
            workers: 1,
            ..Default::default()
        }];
        let memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        assert!(land("forager")
            .blind(&view, &memory, own_band(&view))
            .is_none());
    }

    /// §4: *"One rich patch two tiles from three poor ones loses to a tile that reaches all
    /// four."* Seventeen hands on the east patch plateau at five for 15 a turn; on the north tile
    /// they spread 6/6/5 over three sites for 25.5. Without the three, the east patch is the
    /// cluster, and the band walks to the nearest tile that reaches it; standing on the north
    /// tile, nothing in view beats where it stands.
    #[test]
    fn better_ground_is_the_tile_whose_cluster_takes_the_most_not_the_richest_patch() {
        let view = a_cluster_north_and_a_rich_patch_east();
        let specialist = land("forager");
        let proposal = specialist
            .better_ground(&view, &falling(&view), own_band(&view))
            .expect("the cluster");
        assert_eq!(move_target(&proposal), NORTH, "{}", proposal.reason);
        assert_eq!(
            proposal.reason,
            "better ground in view: 3 sites at 3,5 take 25.5/turn vs 0.0 here"
        );
        assert_eq!(proposal.intent, "land:move:7001");
        assert_eq!(
            proposal.memo,
            Some(Memo::Move {
                band: BAND,
                target: NORTH,
                from: HERE
            })
        );
        // The three poorer patches gone: the east patch is the only ground, reached from the
        // nearest tile within two of it.
        let mut only_east = view.clone_view();
        only_east
            .snapshot
            .forage_patches
            .retain(|patch| Tile::new(patch.x, patch.y) == EAST);
        let proposal = specialist
            .better_ground(&only_east, &falling(&only_east), own_band(&only_east))
            .expect("the east patch");
        let target = move_target(&proposal);
        let grid = only_east.grid();
        assert!(
            grid.distance(target, EAST) <= WORK_RANGE && grid.distance(HERE, target) == 1,
            "the nearest tile reaching it: {target:?} ({})",
            proposal.reason
        );
        assert!(
            proposal
                .reason
                .starts_with("better ground in view: 1 sites at"),
            "{}",
            proposal.reason
        );
        // Standing on the north tile: the cluster is the band's own, and nothing beats it.
        let mut standing = view.clone_view();
        standing.snapshot.populations[0].current_x = NORTH.x;
        standing.snapshot.populations[0].current_y = NORTH.y;
        assert!(specialist
            .better_ground(&standing, &falling(&standing), own_band(&standing))
            .is_none());
    }

    /// ⛔ **Better ground must out-take the band's own by the profile's margin, and is never the
    /// tile the band just left.** On bench seed 11 the band walked 20,8 ↔ 18,8 four times in
    /// eight turns, dropping its rows on every arrival: the ground it stood on was rated on what
    /// it had just stripped, the ground it had left on a forecast, so each always out-paid the
    /// other by a hair.
    #[test]
    fn better_ground_needs_the_margin_and_never_walks_back_to_the_tile_just_left() {
        // The band's own cluster: one site under it paying 17 hands 17. The rich patch is
        // reachable only from a tile the band would have to walk to — one step toward it, from
        // where the band's own site is still in reach for the hands the rich patch does not need.
        let view_with_east = |ceiling: f32| {
            let mut view = bare();
            add_site(&mut view, HERE, 1.0, 17.0);
            add_site(&mut view, EAST, 3.0, ceiling);
            view
        };
        let specialist = land("forager");
        // East pays 6 to two hands and the other fifteen take 15 here: 21 against 17 is 19% of
        // the target — under the forager's quarter.
        let close = view_with_east(6.0);
        assert!(
            specialist
                .better_ground(&close, &falling(&close), own_band(&close))
                .is_none(),
            "distinctness is not improvement"
        );
        // East pays 30 to ten hands and the other seven take 7 here: 37 against 17 clears it.
        let clear = view_with_east(30.0);
        let proposal = specialist
            .better_ground(&clear, &falling(&clear), own_band(&clear))
            .expect("a margin's worth better");
        let target = move_target(&proposal);
        let grid = clear.grid();
        assert!(
            grid.distance(HERE, target) == 1 && grid.distance(target, EAST) <= WORK_RANGE,
            "{}",
            proposal.reason
        );
        assert!(
            proposal
                .reason
                .starts_with("better ground in view: 2 sites at")
                && proposal.reason.ends_with("take 37.0/turn vs 17.0 here"),
            "{}",
            proposal.reason
        );
        assert_eq!(
            proposal.memo,
            Some(Memo::Move {
                band: BAND,
                target,
                from: HERE
            })
        );
        // The band moved 4,2 → here last turn; 4,2 still out-takes here by any margin, and is
        // not offered — the tile it just left is the one reading it cannot trust — so the next
        // tile that reaches the east patch is.
        let mut just_left = falling(&clear);
        just_left.record_choices(
            TICK - 1,
            [(
                "land:move:7001".to_owned(),
                Some(Memo::Move {
                    band: BAND,
                    target: HERE,
                    from: target,
                }),
            )]
            .into_iter(),
        );
        // The next frame: the band stands on the tile it moved to, so the move is done and only
        // the departure is remembered.
        just_left.observe(&clear, FACTION);
        let elsewhere = specialist
            .better_ground(&clear, &just_left, own_band(&clear))
            .expect("another tile reaches the east patch");
        assert_ne!(move_target(&elsewhere), target, "{}", elsewhere.reason);
        assert!(
            specialist
                .better_ground(&clear, &falling(&clear), own_band(&clear))
                .is_some(),
            "with no departure remembered, the same ground is offered"
        );
    }

    #[test]
    fn better_ground_needs_a_falling_runway_and_then_persists_until_arrival() {
        let view = a_cluster_north_and_a_rich_patch_east();
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        memory.observe(&view, FACTION);
        let specialist = land("forager");
        assert!(
            specialist
                .better_ground(&view, &memory, own_band(&view))
                .is_none(),
            "no runway remembered, so it is not falling"
        );
        memory.remember_runways(&view, FACTION);
        let mut later = view.clone_view();
        later.snapshot.populations[0].turns_of_food -= 1.0;
        let proposal = specialist
            .better_ground(&later, &memory, own_band(&later))
            .expect("a cluster in view");
        assert_eq!(proposal.intent, "land:move:7001");
        assert_eq!(proposal.cost.bands, vec![BAND]);
        assert_eq!(move_target(&proposal), NORTH, "{}", proposal.reason);
        // Accepted: the memory learns the target and the intent persists, runway or no runway.
        memory.record_choices(TICK, [(proposal.intent.clone(), proposal.memo)].into_iter());
        let again = specialist
            .better_ground(&view, &memory, own_band(&view))
            .expect("persists");
        assert_eq!(again.intent, proposal.intent);
        assert_eq!(again.commands, proposal.commands);
        // Arrived: the target clears and nothing is proposed.
        let mut arrived = view.clone_view();
        arrived.snapshot.populations[0].current_x = NORTH.x;
        arrived.snapshot.populations[0].current_y = NORTH.y;
        memory.observe(&arrived, FACTION);
        assert!(specialist
            .better_ground(&arrived, &memory, own_band(&arrived))
            .is_none());
    }

    /// A band may not stand on water (`ensure_land_tile` refuses a `move_band` onto a
    /// `WATER`-tagged tile), and the frame says which tiles are: the north tile flooded, the
    /// best dry tile reaches two of the three sites — 18 a turn, still ahead of the east patch.
    #[test]
    fn better_ground_never_stands_a_band_on_water() {
        let mut view = a_cluster_north_and_a_rich_patch_east();
        for row in &mut view.snapshot.tiles {
            if Tile::new(row.x, row.y) == NORTH {
                row.terrain_tags = TerrainTags::WATER;
            }
        }
        let proposal = land("forager")
            .better_ground(&view, &falling(&view), own_band(&view))
            .expect("a dry tile reaching the cluster");
        let target = move_target(&proposal);
        assert_ne!(target, NORTH, "{}", proposal.reason);
        assert!(
            proposal
                .reason
                .starts_with("better ground in view: 2 sites at")
                && proposal.reason.contains("take 18.0/turn"),
            "{}",
            proposal.reason
        );
    }

    #[test]
    fn room_splits_a_large_band_on_owned_ground_under_expand_only() {
        let mut view = a_view();
        let specialist = land("rover");
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

    /// The alarm reads the cluster from where the band stands: the fixture's near and rich
    /// patches are both in reach, so 17 hands take 34 there without moving. A band eating more
    /// than any tile in view offers is short; a rival standing on the rich patch takes it off
    /// the table; ground the rival owns is nobody's cluster. (The fixture's far patch is out of
    /// this world: through the horizontal wrap a tile three steps off reaches it.)
    #[test]
    fn the_alarm_is_ground_that_cannot_feed_the_band_with_nothing_better_in_view() {
        let mut view = a_view();
        view.snapshot
            .forage_patches
            .retain(|patch| Tile::new(patch.x, patch.y) != FAR_PATCH);
        let memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        view.snapshot.populations[0].food_consumption = 30.0;
        assert!(
            land("forager").alarm(&view, &memory).is_none(),
            "34 a turn from where it stands covers 30"
        );
        view.snapshot.populations[0].food_consumption = 100.0;
        assert_eq!(
            land("forager")
                .alarm(&view, &memory)
                .map(|alarm| alarm.kind),
            Some(AlarmKind::LandShort),
            "nothing in view feeds a hundred"
        );
        // A rival standing on the rich patch takes it off the table: 17 on the near patch is 17.
        view.snapshot.populations[0].food_consumption = 30.0;
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION + 1,
            band_id: BAND + 1,
            current_x: RICH_PATCH.x,
            current_y: RICH_PATCH.y,
            ..Default::default()
        });
        assert_eq!(
            land("forager")
                .alarm(&view, &memory)
                .map(|alarm| alarm.kind),
            Some(AlarmKind::LandShort)
        );
        view.snapshot.populations[0].food_consumption = 17.0;
        assert!(
            land("forager").alarm(&view, &memory).is_none(),
            "the near patch (1.0 a worker) still feeds seventeen"
        );
        for patch in &mut view.snapshot.forage_patches {
            patch.owner = Some(FACTION + 1);
        }
        assert_eq!(
            land("forager")
                .alarm(&view, &memory)
                .map(|alarm| alarm.kind),
            Some(AlarmKind::LandShort),
            "the rival's ground is not the band's cluster"
        );
    }

    /// ⛔ **GROUND `Food` CANNOT WORK IS NOT BETTER GROUND.** `assign_labor … forage` is refused
    /// *"nobody gathers here"* off a food module, so a rich patch that carries none is ground the
    /// band would stand beside and be refused every assignment on. It fails twice over: *better
    /// ground* walks the band there, and the same patch, counted as "something better in view",
    /// is what suppresses the `land_short` alarm that would have moved it somewhere it could eat.
    #[test]
    fn a_rich_patch_that_is_no_gathering_site_neither_attracts_a_move_nor_suppresses_the_alarm() {
        let mut view = bare();
        // Only one patch in the world, three steps away, paying far more than the bare ground
        // here — and carrying no food module.
        view.snapshot.forage_patches = vec![ForagePatchState {
            x: EAST.x,
            y: EAST.y,
            owner: None,
            per_worker_yield: 9.0,
            carrying_capacity: 90.0,
            biomass: 135.0,
            provisions_per_biomass: 1.0,
            ..Default::default()
        }];
        view.snapshot.populations[0].food_consumption = 4.0;

        let specialist = land("forager");
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        memory.observe(&view, FACTION);
        memory.remember_runways(&view, FACTION);
        view.snapshot.populations[0].turns_of_food -= 1.0;
        assert!(
            specialist
                .better_ground(&view, &memory, own_band(&view))
                .is_none(),
            "a falling runway, and still nowhere the band could gather"
        );
        assert_eq!(
            specialist.alarm(&view, &memory).map(|alarm| alarm.kind),
            Some(AlarmKind::LandShort),
            "nothing workable in view is the alarm, not a reason to stay quiet"
        );

        // The same patch, now a gathering site: both answers flip.
        make_a_gathering_site(&mut view, EAST);
        let proposal = specialist
            .better_ground(&view, &memory, own_band(&view))
            .expect("a workable patch is better ground");
        assert!(
            view.grid().distance(move_target(&proposal), EAST) <= WORK_RANGE,
            "{}",
            proposal.reason
        );
        assert!(specialist.alarm(&view, &memory).is_none());
    }

    /// ⛔ **The alarm compares a rate to a rate.** Against `carrying_capacity` — a standing biomass
    /// two orders of magnitude larger than a band's appetite — it could essentially never fire.
    #[test]
    fn the_alarm_weighs_what_the_crew_harvests_per_turn_not_the_biomass_standing_here() {
        let mut view = bare();
        let memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
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
        make_a_gathering_site(&mut view, HERE);
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
    /// nothing better than where it stood, and the band sat there and starved. Under the cluster
    /// reading the rate is what the hands are dealt to: two patches out of reach in opposite
    /// directions, the band walks toward the one paying a worker more, however little it holds.
    #[test]
    fn better_ground_follows_the_per_worker_yield_and_not_the_carrying_capacity() {
        let mut view = bare();
        // Here: the richest stand in reach, and the worst rate in it.
        add_site(&mut view, HERE, 0.25, 292.5);
        // Three steps east: less biomass standing, more of it reaching a worker. Three steps
        // west: a great deal standing, and a poor rate.
        let west = Tile::new(0, 2);
        add_site(&mut view, EAST, 0.56, 105.0);
        add_site(&mut view, west, 0.30, 225.0);
        let proposal = land("forager")
            .better_ground(&view, &falling(&view), own_band(&view))
            .expect("two tiles in reach pay a worker more than this one does");
        let target = move_target(&proposal);
        let grid = view.grid();
        assert!(
            grid.distance(target, EAST) <= WORK_RANGE && grid.distance(target, west) > WORK_RANGE,
            "the best rate, not the most biomass: {}",
            proposal.reason
        );
    }

    /// The test-only clone a fixture view needs: `SeatView` is not `Clone`.
    trait CloneView {
        fn clone_view(&self) -> SeatView;
    }

    impl CloneView for SeatView {
        fn clone_view(&self) -> SeatView {
            SeatView {
                snapshot: self.snapshot.clone(),
                last_acted_tick: self.last_acted_tick,
            }
        }
    }
}
