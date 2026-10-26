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
`docs/plan_civilization_steps.md` ("The chief comes first"). This file states what the
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
| `src/data/leadership_config.json` (env `LEADERSHIP_CONFIG_PATH`; `leadership_config.rs`) | `practice_threshold` (10.0), `practice_per_directed_build` (3.0), `practice_per_split` (3.0), `practice_per_herd_hex` (1.0), `practice_per_stage_reached` (5.0); and the tribe levers `tribe_weight_origin` (3.0), `tribe_weight_likeness` (3.0), `tribe_weight_tie` (2.0), `tribe_weight_history` (5.0), `tribe_weight_belief` (1.0), `tribe_weight_acting` (3.0), `tribe_weight_monument` (2.0), `tribe_history_build_turns` (25.0), `tribe_history_fade_turns` (100.0), `tribe_score_threshold` (9.0). Strict: every key required, unknown keys rejected; the practice levers, the two history turns and the threshold finite and `> 0`, the seven weights finite and `>= 0`. All are Workbench levers (`ConfigOverrideKind::Leadership`, proto 11) |
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

## A tribe is earned: close bands that share a gathering place, one of them chiefed

Design: `docs/plan_labor_at_scale.md` ("Leaders: four levels of orders", level 2). Code:
`core_sim/src/tribe.rs`, `tests/tribe.rs`. The tribe itself is sim state; each own band's
progress toward one is on the wire ("On the wire" below).

**Local.** Two bands are *local* on a turn when they are one people, in the same supply network
(id not 0), holding the same belief anchor tile (`PopulationCohort::belief_anchor`, the urn on the
map; `belief.md`). A fission child inherits its parent's anchor and joins its network at once, so
locality alone is not a tribe: it has to be earned.

**The closeness score.** Each turn every local pair gets `closeness` (`tribe.rs`, a pure function of
`CloseInputs` and the config): the sum over seven inputs, each in `[0, 1]`, of `weight x input`. A pair
is *close* at `tribe_score_threshold` (9.0). Only local pairs are scored, since only a local pair can
share a tribe-candidate group.

| Input | Reads | Lever (opening) |
|---|---|---|
| Shared origin | 1 if the two bands' `PopulationCohort::founding_lines` share any `LineId::origin_band` (both descend from one starting band: a split from one another or from a common ancestor), else 0 | `tribe_weight_origin` 3.0 |
| Culture likeness | `likeness(a, b) = clamp(1 - gap / (2 * CULTURE_TRAIT_SPAN), 0, 1)` with `gap = culture::max_axis_gap` of the two bands' own culture layers: the one existing culture-distance metric, normalised by the full axis range. 0 if either band has no layer | `tribe_weight_likeness` 3.0 |
| Knowing each other | `ConnectionLedger::edges_between(a, b)`: the larger of the two directions' `Connection::strength` (`FULL_TIE` = 1) | `tribe_weight_tie` 2.0 |
| Shared history | `TribeRegistry::shared_history`: a per-pair stock in `(0, 1]`, `+ 1 / tribe_history_build_turns` each turn the pair is local and at least one of the two bands has a leader (`BandLeadership.leader.is_some()`), unchanged on a local turn where neither has one, `- 1 / tribe_history_fade_turns` each turn it is not local, clamped. **Never reset on parting.** An entry is pruned at 0 or when either band no longer exists. Checkpoint state | `tribe_weight_history` 5.0; `tribe_history_build_turns` 25.0; `tribe_history_fade_turns` 100.0 |
| Belief on the shared urn | `CultureConfig::anchor_weight(BeliefRegistry::get(anchor))`, the weight belief already uses (`belief / (belief + belief_half_saturation)`), at the pair's shared anchor | `tribe_weight_belief` 1.0 |
| A kept monument on the shared urn | `kept_monument`: 1 while the anchor tile's `TileImprovementRegistry` entry HOLDS a rung `belief_config.json` -> `improvement_belief` lists (the shipped list is `belief:monument`, `belief.md`), else 0. The engine names no rung: the list that says which rungs add belief also says which make a monument. Held, not built: a half-raised monument (`partial_credit: on_completion`) reads 0, and an unkept one reads 0 the turn it decays to `gathering_ground` | `tribe_weight_monument` 2.0 |
| Acting as a tribe | `acting_as_a_tribe(same_tribe)`: 0 for now, and 0 for any pair outside one tribe. The tribe-level orders (#862, #864) and shared works are what will feed it | `tribe_weight_acting` 3.0 |

The opening numbers: a fresh split (kin 3 + identical culture 3 + full tie 2 = 8, plus a little
belief) is under 9 on the split turn, and closes after about five turns of shared history; bands
with different origins and a full tie need about 0.8 of the history (20 turns); different origins
with no tie can never close (3 + 5 = 8) unless a kept monument stands on their urn (+ 2: they close
at 0.8 of the history, 20 turns); a half-alike kin pair needs half the history. A kept monument
also closes a fresh split on the split turn (8 + 2). With no chief on either side a local pair
builds no history at all, so nothing counts toward a tribe until a chief exists. A pair that parts
keeps its history minus 1% a turn, so a short parting re-closes at once on return and a long one has
to rebuild it.

**Who forms one.** Within each (people, network, anchor) set of local bands, take the connected
components of the close pairs. A component is a tribe-candidate iff it has `MIN_TRIBE_BANDS` (2,
"several" by definition, not a lever) or more bands AND at least one member has a `BandLeader` of its
own. Two close bands with no chief never form, however high the score; they form the turn one is
crowned. A tribe whose group loses its last chiefed band dissolves, as does one whose members are no
longer close or no longer local to one another (a band that parts leaves the same turn; its history
fades rather than resets).

**Identity across turns** (`resolve_matches`, pure and unit-pinned) runs on those candidate groups.
The `TribeRegistry` resource holds tribes (`TribeId`, people, member `BandId`s, name, leader),
`next_id`, the shared histories and a per-people cumulative `formed` count. Each tribe, in ascending
id order, picks the group of its own people that holds the most of its previous members (ties: the
group whose lowest `BandId` is lowest). Where several tribes picked one group, the one with more
previous members in it keeps it (ties: the lower `TribeId`); a loser does not try a second group and
dissolves. So a network split keeps the tribe on the majority side and the other side (2+ close bands,
one chiefed) forms a new tribe; two tribes that come to share a network and anchor stay two until
their cross pairs have built the history to be close, then the one holding more survives and the other
dissolves, its bands reading as joining. Membership becomes the group's current bands and is diffed
for join/leave. A tribe with no group of its people dissolves.

**The leader is a member's chief, promoted.** Not a new person: the tribe's `TribeLeader { name,
kind, since_turn, band }` is the `BandLeader` of the **largest chiefed member band** (most people;
ties lowest `BandId`), same name; `Chief` becomes `ParamountChief` and `ElderCouncil` becomes
`CouncilOfChiefs`, and `band` records whose it is. If that band is no longer a member, the leader is
re-promoted from the current largest chiefed member (`since_turn` = that turn), with no event of its
own. The tribe's **name** is "<largest member band's name> tribe" (largest by people overall, chiefed
or not), fixed at formation. A dissolved tribe's leader is gone; a re-formed tribe is a new tribe
with a new id. Leaders carry no stats.

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
refreshes the anchors), `advance_leadership` (so a band crowned this turn counts this turn),
migration, defection, independence and the culture splits.

**On the wire.** A chiefed own band's progress toward a tribe rides `PopulationCohortState`,
appended last: `tribeStanding` (0 local, 1 no urn, 2 no supply network, 3 alone;
`sim_schema::TRIBE_STANDING_*`), `tribeThreshold` (`tribe_score_threshold`),
`tribeHistoryBuildTurns` (`tribe_history_build_turns`), `tribeWeights` (a `TribeClosenessState`: the
seven weights, the most each input can add) and `tribePairs` (one `TribePairState` per local partner
band of the band's people, partner id ascending: `partnerBand`, `partnerName`, raw
`history`, and `contribution`, each input times its weight, summing to the pair's score). Parted
pairs are not published, and neither is whether the partner has a chief: a chiefless partner is
listed like any other. Membership in a *formed* tribe is separate and is published to every member
band, chiefless ones included, as `tribeName`, `tribeMemberNames` (every member's name, largest
band first, ties by band id), `tribeLeaderName` and `tribeLeaderKind`
(`sim_schema::TRIBE_LEADER_*`: 0 none, 1 paramount chief, 2 council of chiefs); all four are empty
in no tribe. Whether a pair is already in one tribe is not on the wire: a band in a tribe shows the
tribe, not progress. A chiefless own band, a foreign band and an expedition party carry the
defaults of the progress fields (threshold 0, which a client reads as nothing to show); a foreign
band and a party carry the defaults of the tribe fields too. The scoring lives in
`TribeRegistry` (`scores`, the inputs of this turn's local pairs, and `standing`), rebuilt from
scratch each turn by `advance_tribes`, rather than in a derived resource: the registry is
checkpoint state, so a snapshot published right after a load still shows progress. The capture
reads it through `snapshot::population::tribe_progress`.

**Checkpoint.** `SimState::tribes` (`TribeRegistry`: tribes, shared histories, `next_id`, `formed`,
this turn's `scores` and `standing`;
`SIM_STATE_RESOURCES`); `SAVE_FORMAT_VERSION` 56.
