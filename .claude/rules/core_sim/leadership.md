---
paths:
  - "core_sim/src/leadership.rs"
  - "core_sim/src/leadership_config.rs"
  - "core_sim/src/leader_names.rs"
  - "core_sim/src/data/leadership_config.json"
  - "core_sim/src/data/leader_names.json"
  - "core_sim/tests/leadership.rs"
---

# Leadership: a chief emerges from directed work that succeeds

A band is given a leader by what it has done, not by a button. Design: `docs/plan_labor_at_scale.md`
("Leaders: four levels of orders", "How a chief emerges") and `docs/plan_civilization_steps.md`
("The chief comes before the granary"). This file states what the code does.

## The record

Every resident band carries a `BandLeadership { practice: f64, leader: Option<BandLeader> }`
(`leadership.rs`), spawned default at worldgen and at a fission splinter. A `BandLeader` is
`{ name, kind: Chief | ElderCouncil, since_turn }`. A detached party never holds practice of its own:
it credits its **home band** (`Expedition::home_band`).

## How practice is earned

Each success adds a config quantum to `practice` (`BandLeadership::credit`). Once the band has a
leader nothing accrues (the value is kept).

| Source | Where it is credited | Quantum |
|---|---|---|
| A **finished build** of a rung flagged `directed_work` (the `cultivate`, `tame` and `corral` rungs) | the per-band completion seam in `advance_labor_allocation`, `BuildJob::Climb` only — not `ExtendPen`, not `SetHerdOutput`, not a road | `practice_per_directed_build` |
| A **big-game hunt that lands its kill**: `fight.fought && fight.brought_down > 0` on a species flagged `big_game` (`aurochs`, `mammoth`) | `settle_hunt_band_side` (a resident band's hunt and a work party's alike) and, for the home band, the scout's roadside kill and the denial raid in `advance_expeditions` | `practice_per_big_game_kill`, **once per hunt turn, not per animal** |

The two flags are config (`RungDef::directed_work`, `SpeciesDef::big_game`), so the rule never tests
which rung or species it is. A pen's take that is not a fight (`fought == false`) credits nothing; a pen whose animal still gores settles through the same seam by the same predicate.

A battle won and monument work are further sources named by the design; each belongs to the arc that
introduces that success (battles, monuments) and credits through the same `credit`.

## Emergence

`advance_leadership` runs right after `advance_labor_allocation` (which follows
`advance_expeditions`) in the same turn, so a success credited this turn can make a leader this turn.
A band with `practice >= practice_threshold` and no leader gets one — once, permanently.

- **Kind** is the sign of the band's own culture layer on `HierarchicalEgalitarian`: below zero a
  `Chief`, zero or above an `ElderCouncil` (ties go egalitarian). It is fixed at emergence; the axis
  drifting later does not flip it. A band with no culture layer yet waits, rather than reading a default.
- **Name** comes from `leader_names.json` through `permuted_name` (shared with `band_names.rs`), under
  its own `LEADER_NAME_SALT`. The permutation is seeded by the map seed and the **index is the band
  id**, so the name is deterministic, independent of emergence order, and unique among a world's
  bands until the pool is exhausted (then the cycle suffix). A council has a named person too — its
  speaker — in the same field.
- **Feed**: a `CommandEventKind::LeaderEmerged` entry `.with_band(band)`, detail
  `kind=chief|elder_council name=<name>`, label "Orrin is chief" / "The elders' council forms; Orrin
  speaks for it".

## Config

| File | Purpose |
|---|---|
| `src/data/leadership_config.json` (env `LEADERSHIP_CONFIG_PATH`; `leadership_config.rs`) | `practice_threshold` (3.0), `practice_per_directed_build` (1.0), `practice_per_big_game_kill` (1.0). Strict: every key required, unknown keys rejected, each `> 0` and finite. Workbench levers (`ConfigOverrideKind::Leadership`, proto 11) |
| `src/data/leader_names.json` (env `LEADER_NAMES_PATH`; `leader_names.rs`) | The leader-name pool — content, not tuning. A duplicate or empty pool is refused at load |
| `intensification_ladder.json` → `rungs[].directed_work`; `fauna_config.json` → `species.<id>.big_game` | the two flags above |

## Save, wire and the Telling

- **Save**: `BandRecord::leadership` (capture and restore); `SAVE_FORMAT_VERSION` 48. No `Entity` in it.
  `BandLeadership` is `SIM_STATE_COMPONENTS` in `tests/sim_state_coverage.rs`; the two handles are config.
- **Wire**: four fields appended to `PopulationCohortState` — `leaderName`, `leaderKind` (0 none, 1
  chief, 2 elders' council), `leadershipPractice`, `leadershipThreshold` (the config threshold, echoed
  so a client can say "N of M"). Own bands only; a foreign row carries the defaults.
- **The Telling**: signals `leadership.chiefs_emerged_this_turn` / `leadership.councils_emerged_this_turn`
  and noun `leadership.newest_leader` (see `telling.md`), driving the beats `leadership.chief_rises` and
  `leadership.council_forms` (not `once`; each band may get one).
