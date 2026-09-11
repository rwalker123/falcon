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
use sim_runtime::{LaborAssignmentState, PopulationCohortState};

use crate::brain::BrainLens;
use crate::geometry::Tile;
use crate::instruments::scoreboard::ScoreRow;
use crate::specialists::food::{is_food_site, patch_per_worker_yield, ROLE_HUNT};
use crate::specialists::land::INTENT_MOVE;
use crate::specialists::{intent_key, SPECIALIST_LAND};
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
}

/// An alarm raised since the plan in force, which the next plan weighs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlarmInForce {
    pub specialist: String,
    pub alarm: String,
    pub since_tick: u64,
}

/// The seat's food ledger, in the frame's units — the same sums the `ScoreRow` carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    pub stock: f32,
    pub income: f32,
    pub consumption: f32,
    pub runway_turns: f32,
    pub working_age: u32,
    pub idle_workers: u32,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentObservation {
    pub job: String,
    pub target: Option<AssignmentTarget>,
    pub workers: u32,
    pub actual_yield: f32,
    pub sustainable_yield: f32,
    /// The sim's crew-take plateau on a hunt row; 0 on every other row.
    pub hunt_useful_workers: u32,
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
    /// The intent the band is still walking under (`land:move:<band>`), when the memory holds a
    /// move target for it — the commitment the arbiter will reward this tick.
    pub intent_in_force: Option<String>,
    pub move_target: Option<TilePos>,
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
                    intent_in_force: move_target
                        .map(|_| intent_key(SPECIALIST_LAND, INTENT_MOVE, band.band_id)),
                    move_target: move_target.map(|tile| TilePos {
                        x: tile.x,
                        y: tile.y,
                    }),
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
    }
}

/// A row whose target coordinates mean nothing: the standing roles and the maintenance pools
/// name no tile, and the wire carries `0, 0` for them.
fn is_untargeted_role(row: &LaborAssignmentState) -> bool {
    UNTARGETED_ROLES.contains(&row.kind.as_str())
}

/// The `assign_labor` roles that take a worker count and no target
/// (`sim_runtime::command_text`, the `scout | warrior | agriculture | husbandry | roadwork |
/// builders` arm).
const UNTARGETED_ROLES: [&str; 6] = [
    "scout",
    "warrior",
    "agriculture",
    "husbandry",
    "roadwork",
    "builders",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::{Alarm, AlarmKind, Budget, Plan, Stance};
    use crate::profile::NO_MEMORY_DECAY;
    use crate::specialists::food::tests::{a_view, BAND, FACTION, HERE, TICK};
    use crate::specialists::SPECIALIST_FOOD;
    use crate::view::VISIBILITY_ACTIVE;
    use sim_runtime::{LaborAssignmentState, PopulationCohortState};

    const HORIZON: u32 = 3;
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
            ..Default::default()
        }];
        let index = view.grid().index(NEVER_SEEN).expect("on the grid");
        view.snapshot.visibility_raster.samples[index] = 0;
        view
    }

    fn a_plan() -> Plan {
        Plan {
            stance: Stance::Consolidate,
            budgets: BTreeMap::from([(SPECIALIST_FOOD, Budget { worker_share: 0.75 })]),
            priorities: BTreeMap::from([(SPECIALIST_FOOD, 0.9)]),
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
        let memory = SeatMemory::new(NO_MEMORY_DECAY);
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
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY);
        view.snapshot.header.tick = EARLIER_TICK;
        memory.observe(&view, FACTION);
        view.snapshot.header.tick = TICK;
        memory.observe(&view, FACTION);
        memory.record_choices(
            TICK - 1,
            BTreeSet::new(),
            [sim_runtime::CommandPayload::MoveBand {
                faction_id: FACTION,
                band_id: Some(BAND),
                target_x: 4,
                target_y: 2,
            }]
            .iter(),
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
        let record =
            ObservationRecord::Observation(capture(&view, &SeatMemory::new(NO_MEMORY_DECAY)));
        let line = serde_json::to_string(&record).expect("serialises");
        assert!(!line.contains('\n'));
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["kind"], "observation");
        let back: ObservationRecord = serde_json::from_str(&line).expect("parses");
        assert_eq!(back, record);
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
}
