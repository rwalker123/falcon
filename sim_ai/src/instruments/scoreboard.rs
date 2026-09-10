//! **The scoreboard** — `scoreboard.jsonl`, one [`ScoreRow`] per acted tick per seat
//! (`docs/plan_ai_driver.md` §8.1). Every field is read from the `SeatView` the process already
//! holds: the seat's own frame says all of this, and no server code knows the row exists.
//!
//! The whole-seat instrument. The bench reads the row at the run's last tick as the seat's
//! result, and `deaths_by_cause` summed over the run as its guard.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sim_runtime::{WorldSnapshot, FIXED_POINT_SCALE, FOOD_CARGO_KEY};

/// The file the rows go to, under `--log-dir`.
pub const SCOREBOARD_FILE: &str = "scoreboard.jsonl";

/// **`turns_of_food`'s "not food-limited" sentinel.** Restated from
/// `core_sim::snapshot::population::NOT_FOOD_LIMITED_TURNS` (this crate cannot link the server);
/// the server's value is the authority. A seat with no band rows in view reads as this too.
pub const NOT_FOOD_LIMITED_TURNS: f32 = 999.0;

/// The `kind` token of a death on the event feed. Restated from `CommandEventKind::as_str` in
/// `core_sim/src/resources.rs`.
pub const DIED_EVENT_KIND: &str = "died";
/// The key of the cause token on a `died` event's `detail` string (`band=… count=… bracket=…
/// cause=…`, `core_sim/src/systems/population.rs`).
const DEATH_CAUSE_KEY: &str = "cause";
/// The count token on the same string: how many people the row buried.
const DEATH_COUNT_KEY: &str = "count";
/// **An event's tick is one behind the frame that first carries it.** The population systems run
/// on tick T, then `advance_tick` runs and the frame is captured at T + 1 — both inside
/// `TurnStage::Snapshot`, in that order (`core_sim/src/lib.rs`). So the deaths of the turn that
/// produced frame T are the `died` rows stamped T − 1.
pub const EVENT_TICK_LAG: u64 = 1;
const DETAIL_KEY_VALUE_SEPARATOR: char = '=';
/// **The label a sim-level command refusal carries**: `"<Kind> failed"`, from
/// `emit_command_failure` in `core_sim/src/bin/server.rs`. A command the seat gate refuses never
/// reaches the feed (it is a `command.rejected` log line); one the sim refuses — an out-of-reach
/// patch, a split below the floor — is this row, and it is the specialist's bug to count.
pub const COMMAND_FAILED_LABEL_SUFFIX: &str = " failed";

/// **The cause vocabulary.** Restated from `DeathCause::as_str` in `core_sim/src/components.rs`,
/// one word, lowercase, stable — a wire contract the client keys off as well. The bench's guard
/// reads [`DEATH_CAUSE_HUNGER`]; the rest are here so a reader knows what keys to expect.
pub const DEATH_CAUSE_HUNGER: &str = "hunger";
pub const DEATH_CAUSE_COLD: &str = "cold";
pub const DEATH_CAUSE_HEAT: &str = "heat";
pub const DEATH_CAUSE_AGE: &str = "age";
pub const DEATH_CAUSES: [&str; 4] = [
    DEATH_CAUSE_HUNGER,
    DEATH_CAUSE_COLD,
    DEATH_CAUSE_HEAT,
    DEATH_CAUSE_AGE,
];

/// One row: this seat, this tick, in the frame's own terms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreRow {
    pub tick: u64,
    pub faction: u32,
    pub population_children: u32,
    pub population_working: u32,
    pub population_elders: u32,
    /// Σ over own bands of the larder's provisions, in food units (the wire's fixed-point raw
    /// divided out).
    pub food_stock: f32,
    pub food_income: f32,
    pub food_consumption: f32,
    pub sustainable_yield: f32,
    pub actual_yield: f32,
    /// min `turns_of_food` over own bands; [`NOT_FOOD_LIMITED_TURNS`] when none is limited.
    pub runway_turns: f32,
    pub idle_workers: u32,
    pub patches_owned: usize,
    pub patches_improved: usize,
    pub herd_biomass_in_view: f32,
    pub herds_corralled: usize,
    pub intensification_knowledge: BTreeMap<String, f32>,
    pub craft_knowledge: BTreeMap<String, f32>,
    /// People who died **in the turn that produced this frame**, by cause token — a per-turn
    /// flow, not the feed's window (see [`EVENT_TICK_LAG`]).
    pub deaths_by_cause: BTreeMap<String, u32>,
    /// Victory mode id → progress, for this seat's faction (the frame is viewer-scoped).
    pub victory_progress: BTreeMap<String, f32>,
    /// Commands of this faction the sim refused in the turn that produced this frame
    /// ([`COMMAND_FAILED_LABEL_SUFFIX`] rows) — a proposal the server would not take.
    pub commands_failed: u32,
}

impl ScoreRow {
    /// Read the row for `faction` off `snapshot`, at the snapshot's own tick.
    pub fn from_snapshot(snapshot: &WorldSnapshot, faction: u32) -> Self {
        let tick = snapshot.header.tick;
        let own_bands = snapshot
            .populations
            .iter()
            .filter(|cohort| cohort.faction == faction);
        let demographics = snapshot
            .demographics
            .iter()
            .find(|row| row.faction == faction);

        let mut food_stock_raw: i64 = 0;
        let mut food_income = 0.0;
        let mut food_consumption = 0.0;
        let mut sustainable_yield = 0.0;
        let mut actual_yield = 0.0;
        let mut runway_turns: Option<f32> = None;
        let mut idle_workers = 0;
        for cohort in own_bands {
            food_stock_raw += cohort
                .stores
                .iter()
                .filter(|store| store.item == FOOD_CARGO_KEY)
                .map(|store| store.quantity)
                .sum::<i64>();
            food_income += cohort.food_income;
            food_consumption += cohort.food_consumption;
            for assignment in &cohort.labor_assignments {
                sustainable_yield += assignment.sustainable_yield;
                actual_yield += assignment.actual_yield;
            }
            runway_turns = Some(match runway_turns {
                Some(current) => current.min(cohort.turns_of_food),
                None => cohort.turns_of_food,
            });
            idle_workers += cohort.idle_workers;
        }

        let own_patches: Vec<_> = snapshot
            .forage_patches
            .iter()
            .filter(|patch| patch.owner == Some(faction))
            .collect();
        let patches_improved = own_patches
            .iter()
            .filter(|patch| patch.is_cultivated || patch.is_field)
            .count();

        let intensification_knowledge = snapshot
            .intensification_knowledge
            .iter()
            .filter(|row| row.faction == faction)
            .flat_map(|row| row.knowledges.iter())
            .map(|knowledge| (knowledge.knowledge_id.clone(), knowledge.progress))
            .collect();
        let craft_knowledge = snapshot
            .craft_knowledge
            .iter()
            .filter(|row| row.faction == faction)
            .map(|row| (row.craft_id.clone(), row.progress))
            .collect();

        let mut deaths_by_cause = BTreeMap::new();
        for event in snapshot
            .command_events
            .iter()
            .filter(|event| event.tick + EVENT_TICK_LAG == tick && event.faction == faction)
            .filter(|event| event.kind == DIED_EVENT_KIND)
        {
            let detail = event.detail.as_deref().unwrap_or_default();
            let Some(cause) = detail_token(detail, DEATH_CAUSE_KEY) else {
                continue;
            };
            let count: u32 = detail_token(detail, DEATH_COUNT_KEY)
                .and_then(|count| count.parse().ok())
                .unwrap_or_default();
            *deaths_by_cause.entry(cause.to_owned()).or_insert(0) += count;
        }

        let commands_failed = snapshot
            .command_events
            .iter()
            .filter(|event| event.tick + EVENT_TICK_LAG == tick && event.faction == faction)
            .filter(|event| event.label.ends_with(COMMAND_FAILED_LABEL_SUFFIX))
            .count() as u32;

        let victory_progress = snapshot
            .victory
            .modes
            .iter()
            .map(|mode| (mode.id.clone(), mode.progress))
            .collect();

        Self {
            tick,
            faction,
            population_children: demographics.map_or(0, |row| row.children),
            population_working: demographics.map_or(0, |row| row.working),
            population_elders: demographics.map_or(0, |row| row.elders),
            food_stock: food_stock_raw as f32 / FIXED_POINT_SCALE as f32,
            food_income,
            food_consumption,
            sustainable_yield,
            actual_yield,
            runway_turns: runway_turns.unwrap_or(NOT_FOOD_LIMITED_TURNS),
            idle_workers,
            patches_owned: own_patches.len(),
            patches_improved,
            herd_biomass_in_view: snapshot.herds.iter().map(|herd| herd.biomass).sum(),
            herds_corralled: snapshot.herds.iter().filter(|herd| herd.corralled).count(),
            intensification_knowledge,
            craft_knowledge,
            deaths_by_cause,
            victory_progress,
            commands_failed,
        }
    }
}

/// The value of `key=` on a space-delimited `key=value` detail string — the parse
/// `core_sim/tests/demographic_events.rs` and the client's feed both use.
fn detail_token<'a>(detail: &'a str, key: &str) -> Option<&'a str> {
    detail
        .split_whitespace()
        .filter_map(|token| token.split_once(DETAIL_KEY_VALUE_SEPARATOR))
        .find(|(k, _)| *k == key)
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_runtime::{CohortStoreState, CommandEventState};

    /// **Provisions per store entry, in food units, before the wire's fixed-point scaling.**
    const PROVISIONS_PER_ENTRY: f32 = 2.5;
    const HUNGER_DEATHS_THIS_TURN: u32 = 3;
    const AGE_DEATHS_THIS_TURN: u32 = 1;
    const HUNGER_DEATHS_LAST_TURN: u32 = 9;

    /// The saturated fixture, with one faction's rows pinned by hand so the sums are checkable:
    /// the first two `populations` rows belong to it, each with one provisions entry, and the
    /// event feed carries four `died` rows — two from the turn that produced the frame (one
    /// hunger, one age), one from the turn before, one stamped with the frame's own tick (which
    /// no real frame carries: it would belong to the *next* frame), and one of another faction's.
    fn fixture_for(faction: u32) -> WorldSnapshot {
        let mut snapshot = sim_runtime::fixture::saturated_snapshot().expect("the fixture builds");
        let tick = snapshot.header.tick;
        let other_faction = faction + 1;
        for (index, cohort) in snapshot.populations.iter_mut().enumerate() {
            cohort.faction = if index < 2 { faction } else { other_faction };
            cohort.stores = vec![CohortStoreState {
                item: FOOD_CARGO_KEY.to_owned(),
                quantity: (PROVISIONS_PER_ENTRY * FIXED_POINT_SCALE as f32) as i64,
            }];
        }
        for row in &mut snapshot.demographics {
            row.faction = other_faction;
        }
        snapshot.demographics[0].faction = faction;
        for row in &mut snapshot.intensification_knowledge {
            row.faction = other_faction;
        }
        snapshot.intensification_knowledge[0].faction = faction;
        for row in &mut snapshot.craft_knowledge {
            row.faction = other_faction;
        }
        snapshot.craft_knowledge[0].faction = faction;
        for patch in &mut snapshot.forage_patches {
            patch.owner = Some(other_faction);
        }
        snapshot.forage_patches[0].owner = Some(faction);
        snapshot.forage_patches[0].is_cultivated = false;
        snapshot.forage_patches[0].is_field = false;
        snapshot.forage_patches[1].owner = Some(faction);
        snapshot.forage_patches[1].is_cultivated = true;
        let died = |tick: u64, faction: u32, cause: &str, count: u32| CommandEventState {
            tick,
            kind: DIED_EVENT_KIND.to_owned(),
            faction,
            label: String::new(),
            detail: Some(format!(
                "band=1 count={count} bracket=working cause={cause}"
            )),
            seq: 0,
        };
        let this_turn = tick - EVENT_TICK_LAG;
        snapshot.command_events = vec![
            died(
                this_turn,
                faction,
                DEATH_CAUSE_HUNGER,
                HUNGER_DEATHS_THIS_TURN,
            ),
            died(this_turn, faction, DEATH_CAUSE_AGE, AGE_DEATHS_THIS_TURN),
            died(
                this_turn - 1,
                faction,
                DEATH_CAUSE_HUNGER,
                HUNGER_DEATHS_LAST_TURN,
            ),
            died(tick, faction, DEATH_CAUSE_HUNGER, HUNGER_DEATHS_LAST_TURN),
            died(
                this_turn,
                other_faction,
                DEATH_CAUSE_HUNGER,
                HUNGER_DEATHS_THIS_TURN,
            ),
        ];
        snapshot
    }

    #[test]
    fn the_row_sums_the_factions_own_rows_and_nothing_else() {
        const FACTION: u32 = 3;
        let snapshot = fixture_for(FACTION);
        let row = ScoreRow::from_snapshot(&snapshot, FACTION);
        let own: Vec<_> = snapshot.populations.iter().take(2).collect();

        assert_eq!(row.tick, snapshot.header.tick);
        assert_eq!(row.faction, FACTION);
        assert_eq!(row.population_children, snapshot.demographics[0].children);
        assert_eq!(row.population_working, snapshot.demographics[0].working);
        assert_eq!(row.population_elders, snapshot.demographics[0].elders);
        assert_eq!(row.food_stock, PROVISIONS_PER_ENTRY * 2.0);
        assert_eq!(
            row.food_income,
            own.iter().map(|cohort| cohort.food_income).sum::<f32>()
        );
        assert_eq!(
            row.food_consumption,
            own.iter()
                .map(|cohort| cohort.food_consumption)
                .sum::<f32>()
        );
        assert_eq!(
            row.actual_yield,
            own.iter()
                .flat_map(|cohort| cohort.labor_assignments.iter())
                .map(|assignment| assignment.actual_yield)
                .sum::<f32>()
        );
        assert_eq!(
            row.sustainable_yield,
            own.iter()
                .flat_map(|cohort| cohort.labor_assignments.iter())
                .map(|assignment| assignment.sustainable_yield)
                .sum::<f32>()
        );
        assert_eq!(
            row.runway_turns,
            own[0].turns_of_food.min(own[1].turns_of_food)
        );
        assert_eq!(
            row.idle_workers,
            own.iter().map(|cohort| cohort.idle_workers).sum::<u32>()
        );
        assert_eq!(row.patches_owned, 2);
        assert_eq!(row.patches_improved, 1);
        assert_eq!(
            row.herd_biomass_in_view,
            snapshot.herds.iter().map(|herd| herd.biomass).sum::<f32>()
        );
        assert_eq!(
            row.herds_corralled,
            snapshot.herds.iter().filter(|herd| herd.corralled).count()
        );
        assert_eq!(
            row.intensification_knowledge.len(),
            snapshot.intensification_knowledge[0].knowledges.len()
        );
        assert_eq!(
            row.craft_knowledge,
            BTreeMap::from([(
                snapshot.craft_knowledge[0].craft_id.clone(),
                snapshot.craft_knowledge[0].progress
            )])
        );
        assert_eq!(
            row.deaths_by_cause,
            BTreeMap::from([
                (DEATH_CAUSE_HUNGER.to_owned(), HUNGER_DEATHS_THIS_TURN),
                (DEATH_CAUSE_AGE.to_owned(), AGE_DEATHS_THIS_TURN),
            ]),
            "this turn's deaths only, this faction's only"
        );
        assert_eq!(row.victory_progress.len(), snapshot.victory.modes.len());
        assert_eq!(row.commands_failed, 0, "no refusal rows in the fixture");
    }

    #[test]
    fn a_refused_command_is_counted_from_its_failed_label_in_the_turn_that_produced_the_frame() {
        const FACTION: u32 = 3;
        let mut snapshot = fixture_for(FACTION);
        let tick = snapshot.header.tick;
        let failed = |tick: u64, faction: u32| CommandEventState {
            tick,
            kind: "forage".to_owned(),
            faction,
            label: format!("Harvest{COMMAND_FAILED_LABEL_SUFFIX}"),
            detail: Some("assign_labor: no gathering site".to_owned()),
            seq: 0,
        };
        snapshot.command_events.extend([
            failed(tick - EVENT_TICK_LAG, FACTION),
            failed(tick - EVENT_TICK_LAG, FACTION),
            failed(tick - EVENT_TICK_LAG - 1, FACTION),
            failed(tick - EVENT_TICK_LAG, FACTION + 1),
        ]);
        assert_eq!(
            ScoreRow::from_snapshot(&snapshot, FACTION).commands_failed,
            2
        );
    }

    #[test]
    fn a_faction_with_no_rows_reads_as_empty_and_not_food_limited() {
        const ABSENT: u32 = 999_999;
        let row = ScoreRow::from_snapshot(&fixture_for(1), ABSENT);
        assert_eq!(row.population_working, 0);
        assert_eq!(row.food_stock, 0.0);
        assert_eq!(row.runway_turns, NOT_FOOD_LIMITED_TURNS);
        assert_eq!(row.patches_owned, 0);
        assert!(row.deaths_by_cause.is_empty());
    }

    #[test]
    fn a_row_round_trips_through_one_json_line() {
        let row = ScoreRow::from_snapshot(&fixture_for(2), 2);
        let line = serde_json::to_string(&row).expect("serialises");
        assert!(!line.contains('\n'));
        let back: ScoreRow = serde_json::from_str(&line).expect("parses");
        assert_eq!(back, row);
    }

    #[test]
    fn the_cause_vocabulary_is_the_servers_four_tokens() {
        assert_eq!(DEATH_CAUSES.len(), 4);
        assert_eq!(
            detail_token("band=1 count=2 cause=hunger", "cause"),
            Some("hunger")
        );
        assert_eq!(detail_token("band=1 count=2", "cause"), None);
    }
}
