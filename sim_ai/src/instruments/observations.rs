//! **The observation record** — `observations.jsonl`, one [`Observation`] per acted tick per seat
//! (`docs/plan_ai_driver.md` §8.4): what each specialist was looking at when it proposed.
//!
//! Written at the same point as the `ScoreRow`, **before** `decide`, so it is the view the brain
//! decided on. It carries what the two other logs do not: the plan and alarms in force, the food
//! ledger, every own band with its worked rows and the intent it is still walking under, and the
//! neighbourhood — every discovered tile within [`observation_radius`] of an own band, read through
//! the **same accessors the specialists rank on** (`specialists::food`), so the viewer shows the
//! seat what the brain saw and not a second derivation of it.
//!
//! A tile the seat has never discovered is absent, not null: the specialists filter on
//! `is_discovered` before they read anything else, and so does this.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sim_runtime::{
    BuildQueueEntryState, ForagePatchState, HerdTelemetryState, LaborAssignmentState,
    PopulationCohortState,
};

use crate::brain::BrainLens;
use crate::geometry::Tile;
use crate::instruments::decisions::GoalsRecord;
use crate::instruments::scoreboard::ScoreRow;
use crate::specialists::food::{is_food_site, patch_per_worker_yield, ROLE_HUNT};
use crate::view::{band_tile, SeatMemory, SeatView};

/// The file the records go to, under `--log-dir`.
pub const OBSERVATIONS_FILE: &str = "observations.jsonl";

/// **The least ground an observation covers around a band**, in hex steps. The profile's
/// `land.horizon_tiles` widens it (`Land` looks that far); nothing narrows it below this, so a
/// band's `work_range` neighbourhood is always on the page.
pub const OBSERVATION_RADIUS_FLOOR: u32 = 3;

/// The neighbourhood's radius: the larger of the floor and the profile's `land.horizon_tiles`.
pub fn observation_radius(horizon_tiles: u32) -> u32 {
    OBSERVATION_RADIUS_FLOOR.max(horizon_tiles)
}

/// The one line type, tagged by `kind` like the decision log's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationRecord {
    Observation(Observation),
}

/// The grid the neighbourhood's coordinates are on, so a viewer can lay hexes out and wrap them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridInfo {
    pub width: u32,
    pub height: u32,
    pub wrap_horizontal: bool,
}

/// The plan the brain holds going into this tick — adopted earlier; this tick's `decide` may
/// replace it (the replacement is the `plan` record at this tick in `decisions.jsonl`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanInForce {
    pub stance: String,
    pub since_tick: u64,
    pub budgets: BTreeMap<String, f32>,
    pub priorities: BTreeMap<String, f32>,
    /// `default` for the reason `PlanRecord::goals` carries it: a log written before goals
    /// existed still reads.
    #[serde(default)]
    pub goals: BTreeMap<String, GoalsRecord>,
}

/// An alarm raised since the plan in force, which the next plan weighs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlarmInForce {
    pub specialist: String,
    pub alarm: String,
    pub since_tick: u64,
}

/// The seat's food ledger, in the frame's units — the same sums the `ScoreRow` carries — and its
/// improved ground, counted over the whole frame (`owner == faction`), not the neighbourhood.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    pub stock: f32,
    pub income: f32,
    pub consumption: f32,
    pub runway_turns: f32,
    pub working_age: u32,
    pub idle_workers: u32,
    pub patches_owned: usize,
    pub patches_cultivated: usize,
    pub patches_field: usize,
}

/// **The climb declared on a source** — the `build_*` fields the patch and the herd row both
/// publish (`docs/plan_standing_upkeep.md` §2.5: the declaration lives on the source, and every
/// band holding the source agrees). Absent when the source has no destination rung.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceBuild {
    pub destination_rung: String,
    pub queue_position: i32,
    pub turns_remaining: i32,
    pub blocked_reason: Option<String>,
    pub kit_id: Option<String>,
}

/// **The standing upkeep an improved source charges** — the `upkeep_*` fields the patch and the
/// herd row both publish. Absent when the source demands nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceUpkeep {
    pub demand: f32,
    pub supplied: f32,
    pub shortfall: f32,
    pub workers_needed: u32,
    pub kit_id: Option<String>,
}

/// One entry of a band's build queue: the web and the source, as the wire names them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildQueueObservation {
    pub job: String,
    pub target: Option<AssignmentTarget>,
}

/// A tile, as an assignment target or a move target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TilePos {
    pub x: u32,
    pub y: u32,
}

/// What a worked row is on: a tile, or a herd; a role with no target (scout, the pools) has none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssignmentTarget {
    Tile(TilePos),
    Herd { herd_id: String },
}

/// One worked row, with the readout the client's Forage / Hunt sheets show for it: what it
/// produced, what it could sustainably, whether the crew is the right size, and what the player
/// stated on it (kit, floor, plants, the build it is declared for).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentObservation {
    pub job: String,
    pub target: Option<AssignmentTarget>,
    pub workers: u32,
    pub actual_yield: f32,
    pub sustainable_yield: f32,
    /// The sim's crew-take plateau on a hunt row; 0 on every other row.
    pub hunt_useful_workers: u32,
    /// The fewest workers that would have produced this turn's take (`workers > workers_needed`
    /// is the overstaffing signal); 0 when the row produced nothing.
    pub workers_needed: u32,
    /// What the source offered that the crew could not collect — the understaffing signal.
    pub wasted_yield: f32,
    /// The sim's overhunting ⚠: the take draws the source below its floor.
    pub overdraws: bool,
    /// The `equipment.json` roster id the crew works under, resolved; `None` on a band-wide role,
    /// which has no kit axis.
    pub kit_id: Option<String>,
    /// The escapement floor, as a fraction of the source's `K` (0 on a band-wide role).
    pub floor: f32,
    /// The plant a `Cultivate`/`Sow` on this patch commits to; `None` for the tile's own pick.
    pub species: Option<String>,
    /// The take selection — the plants carried home; empty is the whole basket.
    pub take_species: Vec<String>,
    /// The build verb declared on this row (`cultivate` / `sow` / …); `None` when none.
    pub improvement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandObservation {
    pub band_id: u64,
    pub x: u32,
    pub y: u32,
    pub size: u32,
    pub working_age: u32,
    pub idle_workers: u32,
    pub turns_of_food: f32,
    pub food_income: f32,
    pub food_consumption: f32,
    pub work_range: u32,
    pub hunt_reach: u32,
    pub is_traveling: bool,
    pub assignments: Vec<AssignmentObservation>,
    /// The intent the band is still walking under (`land:move:<band>`, `food:settle:<band>`),
    /// when the memory holds a move target for it — the commitment the arbiter will reward this
    /// tick.
    pub intent_in_force: Option<String>,
    pub move_target: Option<TilePos>,
    /// The site this band was split off toward (`SeatMemory::born_by_split`), while it has not
    /// reached it — so a viewer can mark a child band.
    #[serde(default)]
    pub born_by_split: Option<TilePos>,
    /// The band's build queue, in the band's order; the declaration itself is on the source row.
    pub build_queue: Vec<BuildQueueObservation>,
}

/// A herd standing on a neighbourhood tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HerdObservation {
    pub id: String,
    pub species: String,
    pub biomass: f32,
    pub per_worker_yield: f32,
    pub huntable: bool,
    pub corralled: bool,
    pub corral_progress: f32,
    pub build: Option<SourceBuild>,
    pub upkeep: Option<SourceUpkeep>,
}

/// One discovered tile within the radius of an own band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileObservation {
    pub x: u32,
    pub y: u32,
    /// The terrain's variant name, from the frame's `tiles` row; `None` when the frame carries no
    /// row for the tile.
    pub terrain: Option<String>,
    /// Whether a crew can be put to work here (`food::is_food_site`) — the eligibility `Food` and
    /// `Land` both filter on.
    pub food_site: bool,
    pub forage_biomass: Option<f32>,
    pub carrying_capacity: Option<f32>,
    /// The frame's published per-worker forecast for the patch.
    pub per_worker_yield: Option<f32>,
    /// What the nearest own band would rank this ground at (`food::patch_per_worker_yield`): its
    /// realized rate on it, else the web's prior, else the forecast.
    pub rated_per_worker_yield: Option<f32>,
    pub owner: Option<u32>,
    pub cultivated: bool,
    pub field: bool,
    /// The two climbs' meters (`cultivation_progress` / `field_progress`); `None` without a patch.
    pub cultivation_progress: Option<f32>,
    pub field_progress: Option<f32>,
    pub build: Option<SourceBuild>,
    pub upkeep: Option<SourceUpkeep>,
    pub herd: Option<HerdObservation>,
    /// The last tick the seat's memory saw the tile actively (undecayed); `None` without a memory.
    pub last_seen_tick: Option<u64>,
    pub nearest_own_band_distance: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub tick: u64,
    pub faction: u32,
    pub radius: u32,
    pub grid: GridInfo,
    pub plan: Option<PlanInForce>,
    pub alarms: Vec<AlarmInForce>,
    pub ledger: Ledger,
    pub bands: Vec<BandObservation>,
    pub neighborhood: Vec<TileObservation>,
}

impl Observation {
    /// Read the observation off the view the brain is about to decide on. `row` is this tick's
    /// `ScoreRow` (the ledger is its sums); `lens` is what the brain holds beyond the frame.
    pub fn capture(view: &SeatView, row: &ScoreRow, lens: &BrainLens<'_>) -> Self {
        let faction = row.faction;
        let grid = view.grid();
        let radius = observation_radius(lens.horizon_tiles);
        // A brain without a memory rates ground on the frame's forecast alone — which is what an
        // empty memory answers.
        let no_memory = SeatMemory::default();
        let memory = lens.memory.unwrap_or(&no_memory);
        let own_bands: Vec<&PopulationCohortState> = view.own_bands(faction).collect();

        let bands = own_bands
            .iter()
            .map(|band| {
                let move_target = memory.move_target(band.band_id);
                let born_by_split = memory.born_by_split(band.band_id);
                BandObservation {
                    band_id: band.band_id,
                    x: band.current_x,
                    y: band.current_y,
                    size: band.size,
                    working_age: band.working_age,
                    idle_workers: band.idle_workers,
                    turns_of_food: band.turns_of_food,
                    food_income: band.food_income,
                    food_consumption: band.food_consumption,
                    work_range: band.work_range,
                    hunt_reach: band.hunt_reach,
                    is_traveling: band.is_traveling,
                    assignments: band
                        .labor_assignments
                        .iter()
                        .map(assignment_observation)
                        .collect(),
                    intent_in_force: memory.move_intent(band.band_id).map(str::to_owned),
                    move_target: move_target.map(|tile| TilePos {
                        x: tile.x,
                        y: tile.y,
                    }),
                    born_by_split: born_by_split.map(|birth| TilePos {
                        x: birth.target.x,
                        y: birth.target.y,
                    }),
                    build_queue: band
                        .build_queue
                        .iter()
                        .map(build_queue_observation)
                        .collect(),
                }
            })
            .collect();

        let mut tiles: BTreeSet<Tile> = BTreeSet::new();
        for band in &own_bands {
            tiles.extend(grid.disk(band_tile(band), radius));
        }
        let mut neighborhood: Vec<TileObservation> = tiles
            .into_iter()
            .filter(|tile| view.is_discovered(*tile))
            .map(|tile| {
                let (nearest_band, distance) = own_bands
                    .iter()
                    .map(|band| (*band, grid.distance(band_tile(band), tile)))
                    .min_by_key(|(_, distance)| *distance)
                    .expect("a neighbourhood tile is within radius of some own band");
                let patch = view.patch_at(tile);
                let herd = view
                    .snapshot
                    .herds
                    .iter()
                    .find(|herd| herd.x == tile.x && herd.y == tile.y)
                    .map(|herd| HerdObservation {
                        id: herd.id.clone(),
                        species: herd.species.clone(),
                        biomass: herd.biomass,
                        per_worker_yield: herd.per_worker_yield,
                        huntable: herd.huntable,
                        corralled: herd.corralled,
                        corral_progress: herd.corral_progress,
                        build: herd_build(herd),
                        upkeep: herd_upkeep(herd),
                    });
                TileObservation {
                    x: tile.x,
                    y: tile.y,
                    terrain: view
                        .snapshot
                        .tiles
                        .iter()
                        .find(|row| row.x == tile.x && row.y == tile.y)
                        .map(|row| format!("{:?}", row.terrain)),
                    food_site: is_food_site(view, tile),
                    forage_biomass: patch.map(|patch| patch.biomass),
                    carrying_capacity: patch.map(|patch| patch.carrying_capacity),
                    per_worker_yield: patch.map(|patch| patch.per_worker_yield),
                    rated_per_worker_yield: patch
                        .map(|patch| patch_per_worker_yield(memory, nearest_band, patch)),
                    owner: patch.and_then(|patch| patch.owner),
                    cultivated: patch.is_some_and(|patch| patch.is_cultivated),
                    field: patch.is_some_and(|patch| patch.is_field),
                    cultivation_progress: patch.map(|patch| patch.cultivation_progress),
                    field_progress: patch.map(|patch| patch.field_progress),
                    build: patch.and_then(patch_build),
                    upkeep: patch.and_then(patch_upkeep),
                    herd,
                    last_seen_tick: lens.memory.and_then(|memory| memory.last_seen(tile)),
                    nearest_own_band_distance: distance,
                }
            })
            .collect();
        neighborhood.sort_by_key(|tile| (tile.y, tile.x));

        Self {
            tick: row.tick,
            faction,
            radius,
            grid: GridInfo {
                width: grid.width,
                height: grid.height,
                wrap_horizontal: grid.wrap_horizontal,
            },
            plan: lens.plan.map(|plan| PlanInForce {
                stance: plan.stance.as_str().to_owned(),
                since_tick: plan.since_turn,
                budgets: plan.budgets_record(),
                priorities: plan.priorities_record(),
                goals: plan.goals_record(),
            }),
            alarms: lens
                .alarms
                .iter()
                .map(|alarm| AlarmInForce {
                    specialist: alarm.specialist.to_owned(),
                    alarm: alarm.kind.as_str().to_owned(),
                    since_tick: alarm.since_tick,
                })
                .collect(),
            ledger: Ledger {
                stock: row.food_stock,
                income: row.food_income,
                consumption: row.food_consumption,
                runway_turns: row.runway_turns,
                working_age: own_bands.iter().map(|band| band.working_age).sum(),
                idle_workers: row.idle_workers,
                patches_owned: row.patches_owned,
                patches_cultivated: owned_patches(view, faction)
                    .filter(|patch| patch.is_cultivated)
                    .count(),
                patches_field: owned_patches(view, faction)
                    .filter(|patch| patch.is_field)
                    .count(),
            },
            bands,
            neighborhood,
        }
    }
}

fn assignment_observation(row: &LaborAssignmentState) -> AssignmentObservation {
    let target = if !row.fauna_id.is_empty() {
        Some(AssignmentTarget::Herd {
            herd_id: row.fauna_id.clone(),
        })
    } else if row.kind == ROLE_HUNT || is_untargeted_role(row) {
        None
    } else {
        Some(AssignmentTarget::Tile(TilePos {
            x: row.target_x,
            y: row.target_y,
        }))
    };
    AssignmentObservation {
        job: row.kind.clone(),
        target,
        workers: row.workers,
        actual_yield: row.actual_yield,
        sustainable_yield: row.sustainable_yield,
        hunt_useful_workers: row.hunt_useful_workers,
        workers_needed: row.workers_needed,
        wasted_yield: row.wasted_yield,
        overdraws: row.overdraws,
        kit_id: non_empty(&row.kit_id),
        floor: row.floor,
        species: non_empty(&row.species),
        take_species: row.take_species.clone(),
        improvement: non_empty(&row.improvement),
    }
}

/// The wire's `""` is "none" on every string it uses as an optional.
fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

/// The patches this faction owns, over the whole frame.
fn owned_patches(view: &SeatView, faction: u32) -> impl Iterator<Item = &ForagePatchState> {
    view.snapshot
        .forage_patches
        .iter()
        .filter(move |patch| patch.owner == Some(faction))
}

fn build_queue_observation(entry: &BuildQueueEntryState) -> BuildQueueObservation {
    let target = if !entry.fauna_id.is_empty() {
        Some(AssignmentTarget::Herd {
            herd_id: entry.fauna_id.clone(),
        })
    } else if entry.kind == ROLE_HUNT {
        None
    } else {
        Some(AssignmentTarget::Tile(TilePos {
            x: entry.target_x,
            y: entry.target_y,
        }))
    };
    BuildQueueObservation {
        job: entry.kind.clone(),
        target,
    }
}

/// The declared climb on a patch: present iff a destination rung is named.
fn patch_build(patch: &ForagePatchState) -> Option<SourceBuild> {
    source_build(
        &patch.build_destination_rung,
        patch.build_queue_position,
        patch.build_turns_remaining,
        &patch.build_blocked_reason,
        &patch.build_kit_id,
    )
}

fn herd_build(herd: &HerdTelemetryState) -> Option<SourceBuild> {
    source_build(
        &herd.build_destination_rung,
        herd.build_queue_position,
        herd.build_turns_remaining,
        &herd.build_blocked_reason,
        &herd.build_kit_id,
    )
}

fn source_build(
    destination_rung: &str,
    queue_position: i32,
    turns_remaining: i32,
    blocked_reason: &str,
    kit_id: &str,
) -> Option<SourceBuild> {
    non_empty(destination_rung).map(|destination_rung| SourceBuild {
        destination_rung,
        queue_position,
        turns_remaining,
        blocked_reason: non_empty(blocked_reason),
        kit_id: non_empty(kit_id),
    })
}

/// The standing upkeep on a patch: present iff it demands anything.
fn patch_upkeep(patch: &ForagePatchState) -> Option<SourceUpkeep> {
    source_upkeep(
        patch.upkeep_demand,
        patch.upkeep_supplied,
        patch.upkeep_shortfall,
        patch.upkeep_workers_needed,
        &patch.upkeep_kit_id,
    )
}

fn herd_upkeep(herd: &HerdTelemetryState) -> Option<SourceUpkeep> {
    source_upkeep(
        herd.upkeep_demand,
        herd.upkeep_supplied,
        herd.upkeep_shortfall,
        herd.upkeep_workers_needed,
        &herd.upkeep_kit_id,
    )
}

fn source_upkeep(
    demand: f32,
    supplied: f32,
    shortfall: f32,
    workers_needed: u32,
    kit_id: &str,
) -> Option<SourceUpkeep> {
    (demand > 0.0).then(|| SourceUpkeep {
        demand,
        supplied,
        shortfall,
        workers_needed,
        kit_id: non_empty(kit_id),
    })
}

/// A row whose target coordinates mean nothing: the standing roles and the maintenance pools
/// name no tile, and the wire carries `0, 0` for them.
fn is_untargeted_role(row: &LaborAssignmentState) -> bool {
    UNTARGETED_ROLES.contains(&row.kind.as_str())
}

/// The `assign_labor` roles that take a worker count and no target. The authority for a **published
/// snapshot row** is `LaborTarget::kind`'s band-wide arm (`core_sim/src/snapshot/population.rs`),
/// which leaves `target_x` / `target_y` at the wire default `0, 0` for every one of them — a row
/// whose kind is missing here is recorded as a crew working tile `0, 0`.
const UNTARGETED_ROLES: [&str; 7] = [
    "scout",
    "warrior",
    "agriculture",
    "husbandry",
    "roadwork",
    "quarrywork",
    "builders",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::{
        Alarm, AlarmKind, Budget, FoodGoals, Goals, GroundRung, Plan, Stance,
    };
    use crate::profile::NO_MEMORY_DECAY;
    use crate::specialists::food::tests::{a_view, BAND, FACTION, HERE, TICK};
    use crate::specialists::land::INTENT_MOVE;
    use crate::specialists::{intent_key, Memo, SPECIALIST_FOOD, SPECIALIST_LAND};
    use crate::view::VISIBILITY_ACTIVE;
    use sim_runtime::{BuildQueueEntryState, LaborAssignmentState, PopulationCohortState};

    const HORIZON: u32 = 3;
    /// The turns a pending split waits for its child in these tests.
    const SETTLE: u32 = 3;
    const FOREIGN_FACTION: u32 = FACTION + 1;
    const FOREIGN_BAND: u64 = BAND + 1;
    /// A tile inside the radius the seat has never discovered.
    const NEVER_SEEN: Tile = Tile::new(3, 0);
    /// The rich patch of the food fixture, given to the rival here.
    const FOREIGN_PATCH: Tile = Tile::new(2, 3);
    const EARLIER_TICK: u64 = TICK - 5;

    /// The food fixture's view, with a rival band and a rival-owned patch, one worked row on the
    /// own band, and one tile inside the radius left undiscovered.
    fn a_view_with_a_rival() -> SeatView {
        let mut view = a_view();
        view.snapshot.populations.push(PopulationCohortState {
            faction: FOREIGN_FACTION,
            band_id: FOREIGN_BAND,
            current_x: 0,
            current_y: 0,
            ..Default::default()
        });
        for patch in &mut view.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == FOREIGN_PATCH {
                patch.owner = Some(FOREIGN_FACTION);
            }
        }
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            kind: "forage".into(),
            target_x: 4,
            target_y: 2,
            workers: 3,
            actual_yield: 0.6,
            sustainable_yield: 0.9,
            workers_needed: 2,
            wasted_yield: 0.1,
            kit_id: "basket".into(),
            floor: 0.5,
            take_species: vec!["wild_emmer".into()],
            improvement: "cultivate".into(),
            ..Default::default()
        }];
        view.snapshot.populations[0].build_queue = vec![BuildQueueEntryState {
            kind: "forage".into(),
            target_x: 4,
            target_y: 2,
            fauna_id: String::new(),
        }];
        if let Some(patch) = view
            .snapshot
            .forage_patches
            .iter_mut()
            .find(|patch| patch.x == 4 && patch.y == 2)
        {
            patch.owner = Some(FACTION);
            patch.cultivation_progress = 0.25;
            patch.build_destination_rung = "tended".into();
            patch.build_turns_remaining = 3;
            patch.build_kit_id = "digging_stick".into();
            patch.upkeep_demand = 1.5;
            patch.upkeep_supplied = 1.0;
            patch.upkeep_shortfall = 0.5;
            patch.upkeep_workers_needed = 2;
        }
        let index = view.grid().index(NEVER_SEEN).expect("on the grid");
        view.snapshot.visibility_raster.samples[index] = 0;
        view
    }

    fn a_plan() -> Plan {
        Plan {
            stance: Stance::Consolidate,
            budgets: BTreeMap::from([(SPECIALIST_FOOD, Budget { worker_share: 0.75 })]),
            priorities: BTreeMap::from([(SPECIALIST_FOOD, 0.9)]),
            goals: BTreeMap::from([(
                SPECIALIST_FOOD,
                Goals::Food(FoodGoals {
                    net_income_per_turn: 1.0,
                    runway_turns: 12.0,
                    ground_rung: GroundRung::Field,
                }),
            )]),
            since_turn: EARLIER_TICK,
        }
    }

    fn capture(view: &SeatView, memory: &SeatMemory) -> Observation {
        let plan = a_plan();
        let alarms = [Alarm {
            specialist: SPECIALIST_FOOD,
            kind: AlarmKind::FoodShort,
            since_tick: TICK - 1,
        }];
        let row = ScoreRow::from_snapshot(&view.snapshot, FACTION);
        Observation::capture(
            view,
            &row,
            &BrainLens {
                plan: Some(&plan),
                alarms: &alarms,
                memory: Some(memory),
                horizon_tiles: HORIZON,
            },
        )
    }

    #[test]
    fn the_radius_is_the_floor_or_the_horizon_whichever_is_wider() {
        assert_eq!(observation_radius(0), OBSERVATION_RADIUS_FLOOR);
        assert_eq!(
            observation_radius(OBSERVATION_RADIUS_FLOOR + 2),
            OBSERVATION_RADIUS_FLOOR + 2
        );
    }

    #[test]
    fn only_own_bands_appear_and_the_neighborhood_is_the_discovered_disk_around_them() {
        let view = a_view_with_a_rival();
        let memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        let observation = capture(&view, &memory);

        assert_eq!(observation.tick, TICK);
        assert_eq!(observation.faction, FACTION);
        assert_eq!(observation.radius, HORIZON.max(OBSERVATION_RADIUS_FLOOR));
        assert_eq!(
            observation
                .bands
                .iter()
                .map(|b| b.band_id)
                .collect::<Vec<_>>(),
            vec![BAND],
            "the rival's band is not this seat's"
        );
        let band = &observation.bands[0];
        assert_eq!((band.x, band.y), (HERE.x, HERE.y));
        assert_eq!(band.assignments.len(), 1);
        assert_eq!(
            band.assignments[0].target,
            Some(AssignmentTarget::Tile(TilePos { x: 4, y: 2 }))
        );
        assert_eq!(band.intent_in_force, None, "nothing being walked to");

        let expected: BTreeSet<Tile> = view
            .grid()
            .disk(HERE, observation.radius)
            .into_iter()
            .filter(|tile| *tile != NEVER_SEEN)
            .collect();
        let got: BTreeSet<Tile> = observation
            .neighborhood
            .iter()
            .map(|tile| Tile::new(tile.x, tile.y))
            .collect();
        assert_eq!(got, expected, "the disk, minus the tile never seen");
        assert!(
            !got.contains(&Tile::new(7, 5)),
            "the far patch is outside the radius"
        );
        assert!(observation
            .neighborhood
            .iter()
            .all(|tile| tile.nearest_own_band_distance <= observation.radius));

        let foreign = observation
            .neighborhood
            .iter()
            .find(|tile| Tile::new(tile.x, tile.y) == FOREIGN_PATCH)
            .expect("the rival's patch is in the neighbourhood");
        assert_eq!(foreign.owner, Some(FOREIGN_FACTION));
        assert!(foreign.food_site);
        assert_eq!(foreign.per_worker_yield, Some(2.0));
        assert_eq!(
            foreign.rated_per_worker_yield,
            Some(2.0),
            "an unworked patch is rated at the forecast"
        );
        let bare = observation
            .neighborhood
            .iter()
            .find(|tile| Tile::new(tile.x, tile.y) == HERE)
            .expect("the band's own tile");
        assert_eq!(bare.forage_biomass, None, "no patch row here");
        assert!(!bare.food_site);
        let with_herd = observation
            .neighborhood
            .iter()
            .find(|tile| tile.herd.is_some())
            .expect("the herd at (5,4) is within radius");
        assert_eq!((with_herd.x, with_herd.y), (5, 4));
        assert_eq!(with_herd.herd.as_ref().unwrap().id, "herd_9");

        let plan = observation.plan.as_ref().expect("the plan in force");
        assert_eq!(plan.stance, "consolidate");
        assert_eq!(plan.since_tick, EARLIER_TICK);
        assert_eq!(plan.budgets.get(SPECIALIST_FOOD), Some(&0.75));
        assert_eq!(observation.alarms.len(), 1);
        assert_eq!(observation.alarms[0].alarm, "food_short");
        assert_eq!(observation.ledger.working_age, 17);
        assert_eq!(observation.ledger.idle_workers, 17);
    }

    #[test]
    fn last_seen_and_the_intent_in_force_read_from_memory() {
        let mut view = a_view_with_a_rival();
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        view.snapshot.header.tick = EARLIER_TICK;
        memory.observe(&view, FACTION);
        view.snapshot.header.tick = TICK;
        memory.observe(&view, FACTION);
        memory.record_choices(
            TICK - 1,
            [(
                intent_key(SPECIALIST_LAND, INTENT_MOVE, BAND),
                Some(Memo::Move {
                    band: BAND,
                    target: Tile::new(4, 2),
                    from: HERE,
                }),
            )]
            .into_iter(),
        );
        let observation = capture(&view, &memory);
        let at = |x, y| {
            observation
                .neighborhood
                .iter()
                .find(|tile| (tile.x, tile.y) == (x, y))
                .expect("in the neighbourhood")
        };
        assert_eq!(
            view.visibility(HERE),
            VISIBILITY_ACTIVE,
            "the band's tile is in active sight"
        );
        assert_eq!(at(HERE.x, HERE.y).last_seen_tick, Some(TICK));
        assert_eq!(
            at(4, 2).last_seen_tick,
            Some(EARLIER_TICK),
            "discovered-only ground keeps its first sighting"
        );
        let band = &observation.bands[0];
        assert_eq!(band.intent_in_force.as_deref(), Some("land:move:7001"));
        assert_eq!(band.move_target, Some(TilePos { x: 4, y: 2 }));
        assert_eq!(band.born_by_split, None);
        let plan = observation.plan.as_ref().expect("the lens carried a plan");
        assert_eq!(plan.goals[SPECIALIST_FOOD].ground_rung, "field");
        assert_eq!(plan.goals[SPECIALIST_FOOD].runway_turns, 12.0);
        // A child band the memory holds a birth for is marked with its site.
        memory.record_choices(
            TICK,
            [(
                "food:split:7001".to_owned(),
                Some(Memo::Split {
                    band: BAND,
                    target: Tile::new(6, 4),
                    workers: 5,
                }),
            )]
            .into_iter(),
        );
        let mut next = a_view_with_a_rival();
        next.snapshot.header.tick = TICK + 1;
        next.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: BAND + 100,
            current_x: HERE.x,
            current_y: HERE.y,
            ..Default::default()
        });
        memory.observe(&next, FACTION);
        let observation = capture(&next, &memory);
        let child = observation
            .bands
            .iter()
            .find(|band| band.band_id == BAND + 100)
            .expect("the child is an own band");
        assert_eq!(child.born_by_split, Some(TilePos { x: 6, y: 4 }));
    }

    #[test]
    fn without_a_lens_the_plan_is_null_and_nothing_is_remembered() {
        let view = a_view_with_a_rival();
        let row = ScoreRow::from_snapshot(&view.snapshot, FACTION);
        let observation = Observation::capture(&view, &row, &BrainLens::default());
        assert_eq!(observation.plan, None);
        assert!(observation.alarms.is_empty());
        assert_eq!(observation.radius, OBSERVATION_RADIUS_FLOOR);
        assert!(observation
            .neighborhood
            .iter()
            .all(|tile| tile.last_seen_tick.is_none()));
    }

    #[test]
    fn a_record_round_trips_through_one_json_line_with_its_kind_tag() {
        let view = a_view_with_a_rival();
        let record = ObservationRecord::Observation(capture(
            &view,
            &SeatMemory::new(NO_MEMORY_DECAY, SETTLE),
        ));
        let line = serde_json::to_string(&record).expect("serialises");
        assert!(!line.contains('\n'));
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["kind"], "observation");
        let back: ObservationRecord = serde_json::from_str(&line).expect("parses");
        assert_eq!(back, record);
    }

    /// The worked row carries the client's Forage/Hunt readout — staffing signals, kit, floor,
    /// take selection, the declared build — and the source it works carries the declared climb
    /// and its upkeep; the band's build queue names the source; the ledger counts the seat's
    /// improved ground over the whole frame.
    #[test]
    fn the_worked_row_and_its_source_carry_the_readout_fields() {
        let view = a_view_with_a_rival();
        let observation = capture(&view, &SeatMemory::new(NO_MEMORY_DECAY, SETTLE));
        let band = &observation.bands[0];
        let row = &band.assignments[0];
        assert_eq!(row.workers_needed, 2);
        assert_eq!(row.wasted_yield, 0.1);
        assert!(!row.overdraws);
        assert_eq!(row.kit_id.as_deref(), Some("basket"));
        assert_eq!(row.floor, 0.5);
        assert_eq!(row.species, None, "the tile's own pick");
        assert_eq!(row.take_species, vec!["wild_emmer"]);
        assert_eq!(row.improvement.as_deref(), Some("cultivate"));
        assert_eq!(band.build_queue.len(), 1);
        assert_eq!(
            band.build_queue[0].target,
            Some(AssignmentTarget::Tile(TilePos { x: 4, y: 2 }))
        );
        let worked = observation
            .neighborhood
            .iter()
            .find(|tile| tile.x == 4 && tile.y == 2)
            .expect("the worked tile is in the neighbourhood");
        assert_eq!(worked.cultivation_progress, Some(0.25));
        let build = worked.build.as_ref().expect("a climb is declared");
        assert_eq!(build.destination_rung, "tended");
        assert_eq!(build.turns_remaining, 3);
        assert_eq!(build.kit_id.as_deref(), Some("digging_stick"));
        assert_eq!(build.blocked_reason, None);
        let upkeep = worked.upkeep.as_ref().expect("the source charges upkeep");
        assert_eq!(upkeep.shortfall, 0.5);
        assert_eq!(upkeep.workers_needed, 2);
        assert_eq!(upkeep.kit_id, None);
        let unworked = observation
            .neighborhood
            .iter()
            .find(|tile| tile.x == FOREIGN_PATCH.x && tile.y == FOREIGN_PATCH.y)
            .expect("the rival's patch");
        assert!(unworked.build.is_none() && unworked.upkeep.is_none());
        assert_eq!(unworked.cultivation_progress, Some(0.0));
        assert_eq!(observation.ledger.patches_owned, 1);
        assert_eq!(observation.ledger.patches_cultivated, 0);
        assert_eq!(observation.ledger.patches_field, 0);
    }

    #[test]
    fn a_hunt_row_targets_its_herd_and_a_scout_row_targets_nothing() {
        let hunt = assignment_observation(&LaborAssignmentState {
            kind: ROLE_HUNT.into(),
            fauna_id: "herd_9".into(),
            workers: 4,
            hunt_useful_workers: 2,
            ..Default::default()
        });
        assert_eq!(
            hunt.target,
            Some(AssignmentTarget::Herd {
                herd_id: "herd_9".into()
            })
        );
        assert_eq!(hunt.hunt_useful_workers, 2);
        let scout = assignment_observation(&LaborAssignmentState {
            kind: "scout".into(),
            workers: 1,
            ..Default::default()
        });
        assert_eq!(scout.target, None);
    }

    /// ⛔ **Every band-wide role, not most of them.** A role missing from [`UNTARGETED_ROLES`]
    /// falls through to the tile arm and records a crew working `0, 0` — the wire default the
    /// band-wide arm of `LaborTarget::kind` leaves those rows at — which the viewer then outlines
    /// and badges as a worked hex nobody works.
    #[test]
    fn every_band_wide_role_targets_nothing() {
        for kind in UNTARGETED_ROLES {
            let row = assignment_observation(&LaborAssignmentState {
                kind: kind.into(),
                workers: 2,
                ..Default::default()
            });
            assert_eq!(row.target, None, "{kind} names no tile");
        }
    }
}
