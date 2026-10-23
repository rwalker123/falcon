---
paths:
  - "core_sim/src/leadership.rs"
  - "core_sim/src/leadership_config.rs"
  - "core_sim/src/leader_names.rs"
  - "core_sim/src/data/leadership_config.json"
  - "core_sim/src/data/leader_names.json"
  - "core_sim/tests/leadership.rs"
  - "core_sim/src/tribe.rs"
  - "core_sim/tests/tribe.rs"
---

# Leadership: a chief emerges from work that needs an organiser

A band is given a leader by what it has to organise, not by a button and not by winning. Design:
`docs/plan_labor_at_scale.md` ("How a chief emerges: work that needs an organiser") and
`docs/plan_civilization_steps.md` ("The chief comes before the granary"). This file states what the
code does.

## The record

Every resident band carries a `BandLeadership` (`leadership.rs`), spawned default at worldgen and at
a fission splinter: points banked **per source** (`from_builds`, `from_splits`, `from_herd`,
`from_stages`, all `f64`; `practice()` is their sum), `stages_paid: u32`, and `leader:
Option<BandLeader>`. A `BandLeader` is `{ name, kind: Chief | ElderCouncil, since_turn }`. Once a
band has a leader **nothing accrues** — every credit method is a no-op, so the bar freezes where it
stood.

## What earns points

| Source | Where it is credited | Quantum (`leadership_config.json`) |
|---|---|---|
| A finished **Cultivate, Farm, Tame or Corral** build — the rungs flagged `directed_work` (`cultivate`, `sow`, `tame`, `corral`; not a road, felling, coppicing, quarrying, a pen's ring or a herd's output commitment) | the per-band completion seam in `advance_labor_allocation`, `BuildJob::Climb` only | `practice_per_directed_build` |
| A **commanded band split** — to BOTH the parent and the new band | `split_band_from_parent` (`systems/fission.rs`), the wrapper the player's `split_band` verb runs. The breeding break-off and the independence break-away call `split_band_from_parent_with` directly and credit nothing: nobody ordered them | `practice_per_split`, each band, once |
| **Hexes walked following a herd**, in any roam state, the catch-up walk included | `advance_leadership` reads `mobility::HerdFollowSteps::hexes_of` — the very measure the mobility score's distance input drains, filled by `advance_band_movement`. No recomputation and no ordering hack: `advance_leadership` is ordered after labor and `mobility_tick` has an explicit `.after(advance_leadership)`, so the steps are there when read and still there when drained. A plain move order records no steps | `practice_per_herd_hex` per hex |
| A **settlement stage reached** — the first time a band stands in each stage above the first | `advance_leadership`, off the band's current size through `resolve_settlement_stage` and the stage's index in `settlement_stage_config.json`. `stages_paid` is the highest index already paid: a shrink and regrow does not repay, reaching two stages at once pays both, and the first stage is never paid | `practice_per_stage_reached` per stage |

A kill credits nothing. Which builds count is the rung flag `RungDef::directed_work` (config), so no
seam tests which rung it is.

## Emergence

`advance_leadership` runs right after `advance_labor_allocation` in the same turn (and before
`mobility_tick` drains the herd steps), so a build finished this turn can make a leader this turn. It
banks the herd and stage points, then: a band with `practice() >= practice_threshold` and no leader
gets one — once, permanently.

- **Kind** is the sign of the band's own culture layer on `HierarchicalEgalitarian`: below zero a
  `Chief`, zero or above an `ElderCouncil` (ties go egalitarian). It is fixed at emergence; the axis
  drifting later does not flip it. A band with no culture layer yet banks but waits, rather than
  reading a default.
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
| `src/data/leadership_config.json` (env `LEADERSHIP_CONFIG_PATH`; `leadership_config.rs`) | `practice_threshold` (10.0), `practice_per_directed_build` (3.0), `practice_per_split` (3.0), `practice_per_herd_hex` (1.0), `practice_per_stage_reached` (5.0). Strict: every key required, unknown keys rejected, each finite and `> 0`. All five are Workbench levers (`ConfigOverrideKind::Leadership`, proto 11) |
| `src/data/leader_names.json` (env `LEADER_NAMES_PATH`; `leader_names.rs`) | The leader-name pool — content, not tuning. A duplicate or empty pool is refused at load |
| `intensification_ladder.json` → `rungs[].directed_work` | `true` on `cultivate`, `sow`, `tame` and `corral` |

## Save, wire and the Telling

- **Save**: `BandRecord::leadership` (capture and restore); `SAVE_FORMAT_VERSION` 48. No `Entity` in it.
  `BandLeadership` is `SIM_STATE_COMPONENTS` in `tests/sim_state_coverage.rs`; the two handles are config.
- **Wire**: appended to `PopulationCohortState` — `leaderName`, `leaderKind` (0 none, 1 chief, 2
  elders' council), `leadershipPractice` (the sum), `leadershipThreshold` (the config threshold,
  echoed so a client can say "N of M"), and the four per-source parts `leadershipFromBuilds`,
  `leadershipFromSplits`, `leadershipFromHerd`, `leadershipFromStages` (they sum to
  `leadershipPractice`, which is what a split bar draws). Own bands only; a foreign row carries the
  defaults.
- **The Telling**: signals `leadership.chiefs_emerged_this_turn` / `leadership.councils_emerged_this_turn`
  and nouns `leadership.newest_chief` / `leadership.newest_council` (kind-scoped, so a chief and a
  council emerging together each name their own leader; see `telling.md`), driving the beats
  `leadership.chief_rises` and
  `leadership.council_forms` (not `once`; each band may get one). The prose says the band grew too
  many, too spread out and too much on the move for everyone to just know what to do.

## A tribe forms when linked bands share a gathering place

Design: `docs/plan_labor_at_scale.md` ("Leaders: four levels of orders", level 2). Code:
`core_sim/src/tribe.rs`, `tests/tribe.rs`. No wire, snapshot or client change: the tribe is sim state
until a later slice puts it on screen.

**Who forms one.** Each turn `advance_tribes` groups the resident bands by `(people, supply network
id, belief anchor tile)`, skipping a band in network 0 or with no anchor
(`PopulationCohort::belief_anchor`, the urn on the map; `belief.md`). Any group of
`MIN_TRIBE_BANDS` (2, "several" by definition, not a lever) or more is a tribe. A band needs no chief
of its own to join.

**Identity across turns** (`resolve_matches`, pure and unit-pinned). The `TribeRegistry` resource
holds tribes (`TribeId`, people, member `BandId`s, name, leader), `next_id` and a per-people
cumulative `formed` count. Each tribe, in ascending id order, picks the group of its own people that
holds the most of its previous members (ties: the group whose lowest `BandId` is lowest). Where
several tribes picked one group, the one with more previous members in it keeps it (ties: the lower
`TribeId`); a loser does not try a second group and dissolves. So a network split keeps the tribe on
the majority side and the other side (2+ bands) forms a new tribe; two tribes merging into one group
leave the survivor and dissolve the other, whose bands then read as joining. Membership becomes the
group's current bands and is diffed for join/leave. A tribe none of whose members stands in a group
of its people dissolves.

**The leader.** A new tribe gets a `TribeLeader { name, kind, since_turn }` once, at formation. Kind
comes from the **largest member band's** (most people; ties lowest `BandId`) own culture on
`HierarchicalEgalitarian`: below zero `ParamountChief`, else `CouncilOfChiefs`. It never flips with
later drift; a largest band with no culture layer yet delays formation a turn. The name is drawn from
the band-leader pool (`leader_names.json`) by `name_for_tribe(map_seed, tribe_id)`, a permutation under
`LEADER_NAME_SALT ^ TRIBE_LEADER_NAME_SALT` indexed by the tribe id, so tribe leaders are
deterministic and independent of band leaders' names. A dissolved tribe's leader is gone; a re-formed
tribe is a new tribe with a new id and leader. Leaders carry no stats. The tribe's **name** is
"<largest band's name> tribe", fixed at formation.

**What the player is told.** Four `CommandEventKind`s, each filed under the tribe's people with
`with_band`. Bands are spelled in labels as `Band <id>` (`systems::band_label`) and in details as
`band=<BandId.0>`, so the client swaps in the roster name; no detail carries `name=`.

| Kind | Label | Detail | Against |
|---|---|---|---|
| `TribeFormed` | "Reedwater tribe forms; Orrin is paramount chief" / "Reedwater tribe forms; a council of chiefs gathers, Orrin speaks for it" | `tribe=<id> kind=paramount_chief\|council_of_chiefs leader=<name>` | the largest band |
| `TribeBandJoined` | "Band 4 joins the Reedwater tribe" | `tribe=<id> band=<BandId>` | that band |
| `TribeBandLeft` | "Band 4 leaves the Reedwater tribe" (a band that no longer exists is not announced) | `tribe=<id> band=<BandId>` | that band |
| `TribeDissolved` | "The Reedwater tribe breaks up" | `tribe=<id>` | the largest former band still standing |

The Telling signal `tribes.formed_this_turn` is `TribeRegistry::formed_by(faction)` diffed against
last turn's total, which rides `CumulativeTotals::tribes_formed` under `internal.tribes.formed_total`;
it drives the `tribe.forms` beat.

**Schedule.** `advance_tribes` is ordered `.after(sedentarization_tick)`, the end of the Population
stage; `balance_supply_networks` (Logistics) precedes it by stage. The edge is that late because the
system reads `PopulationCohort` and files on `CommandEventLog`, which most of the stage writes, and
the ambiguity gate rejects a system left unordered against them (an earlier
`.after(simulate_population)` / `.after(advance_leadership)` placement produced 13 conflicts). From
the end of the spine and the pressure/sedentarization tail it follows `simulate_population` (which
refreshes the anchors), `advance_leadership`, migration, defection, independence and the culture
splits.

**Checkpoint.** `SimState::tribes` (`TribeRegistry`, `SIM_STATE_RESOURCES`); `SAVE_FORMAT_VERSION` 55.
