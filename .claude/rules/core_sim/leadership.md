---
paths:
  - "core_sim/src/leadership.rs"
  - "core_sim/src/chief_proposals.rs"
  - "core_sim/src/leadership_config.rs"
  - "core_sim/src/leader_names.rs"
  - "core_sim/src/data/leadership_config.json"
  - "core_sim/src/data/leader_names.json"
  - "core_sim/tests/leadership.rs"
  - "core_sim/src/tribe.rs"
  - "core_sim/tests/tribe.rs"
  - "core_sim/tests/leadership_consent.rs"
  - "core_sim/src/food_orders.rs"
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
- **Arrival takeover** (#860): the first managing pass over a band with a leader gives it an order for
  every kind in `labor_config.json` -> `band_orders.ordered_kinds` (owner Chief, `starting_target`),
  and `advance_leadership` ends by draining `LaborAllocation::unannounced_takeover` into one
  `Extraction`-kind feed line per band, `.with_band(band)`: "Orrin is chief of Bitterbrook — he takes
  over the band's work: he keeps 20 turns of food, 30 stone and 30 wood on hand." / "Bitterbrook's elders take over the band's work: they keep …". Mechanism:
  `work-party.md` -> "A chief keeps an ordered material on hand".

## Config

| File | Purpose |
|---|---|
| `src/data/leadership_config.json` (env `LEADERSHIP_CONFIG_PATH`; `leadership_config.rs`) | `practice_threshold` (10.0), `practice_per_directed_build` (3.0), `practice_per_split` (3.0), `practice_per_herd_hex` (1.0), `practice_per_stage_reached` (5.0); and the tribe levers `tribe_weight_origin` (3.0), `tribe_weight_likeness` (3.0), `tribe_weight_tie` (2.0), `tribe_weight_belief` (1.0), `tribe_weight_acting` (3.0), `tribe_weight_monument` (2.0), `tribe_points_fade_turns` (100.0), `tribe_score_threshold` (100.0); and the reach levers `reach_chief_people` (150.0), `reach_paramount_bands` (4.0), `over_reach_efficiency` (0.5). Strict: every key required, unknown keys rejected; the practice levers, the fade turns, the threshold and the two reaches finite and `> 0`, the six weights finite and `>= 0`, `over_reach_efficiency` finite and in `(0, 1]`. All are Workbench levers (`ConfigOverrideKind::Leadership`, proto 11) |
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
  defaults. Reach rides after the tribe fields (below).
- **The Telling**: signals `leadership.chiefs_emerged_this_turn` / `leadership.councils_emerged_this_turn`
  and nouns `leadership.newest_chief` / `leadership.newest_council` (kind-scoped, so a chief and a
  council emerging together each name their own leader; see `telling.md`), driving the beats
  `leadership.chief_rises` and
  `leadership.council_forms` (not `once`; each band may get one). The prose says the band grew too
  many, too spread out and too much on the move for everyone to just know what to do.

## The band judges its leader by what he delivers

Design: `docs/plan_civilization_steps.md` -> "Directed labor and consent" and `docs/plan_labor_at_scale.md`
-> "Band orders - the chief". **Strain comes from the leader failing to deliver, not from how much he
directs; when goals are met the leader gains popularity.** The leader is credited and blamed for the
band's overall results whoever crews the rows: a player-crewed ("Crew it yourself") order counts exactly
like a chief-owned one.

**The term.** `MoraleContributions::leadership` (`MoraleFactor::Leadership`, `MoraleCause::Leadership`,
wire cause `6`), signed. Only a band with a leader (`BandLeadership::leader.is_some()`) has it; every other
band reads `0`. Priced inside `simulate_population`'s per-band loop by `leadership::judge_leader`: the
hunger half needs this turn's meal deficit, which is struck there. The band's opening
`DemographicState` is built before morale, and `meal_deficit_fraction` / `deficit_fraction_of` are the same
function `advance_demographics` kills with, so the term and the deaths read one deficit.

Per turn, for a led band:

| Part | Reads | Term |
|---|---|---|
| Each order in `LaborAllocation::orders` (every kind, either owner; fodder only while the band pens hay-eaters) | `food_orders::OrderReadings::figures` - `on_hand`, `target`, `take`, `draw`, the same figures `PopulationCohortState.bandOrders` publishes (`snapshot::population::band_order_rows` calls it too) | **Met** (`on_hand >= target`): `+popularity_per_met_order`. **Falling** (`on_hand < target` and `take < draw`): `-strain_per_falling_order x patience`. Below target but filling: `0` |
| The people go short at the meal | the real starvation deficit (`deficit_fraction > 0`), never income against consumption | `-starvation_strain x min(1, deficit / starvation_full_at) x patience` |

The order readings are one turn old (`LaborAllocation` resolves after `simulate_population`), the same lag the
fertility food-flow reading has.

**A build the chief proposed and the player approved counts once.** When the approved build completes
(`chief_proposals::refresh_proposals`, the turn's pass, sees the improvement at or above the rung it raised)
`ChiefProposalBook` banks one `met_builds_fresh` for the band; `advance_leadership` ends by moving it to
`met_builds_ready`, which `judge_leader` adds to `met_orders` on the next judgement (the same
`popularity_per_met_order`) and so to `last_leadership.met_orders`, which the Hierarchical drift reads: the
same morale and the same drift as a met order, once, and nothing while it is building. A build abandoned
before it completes is dropped with no penalty. `chief_proposals.rs` owns it; `work-party.md` -> "A chief
proposes a build" has the rest.

**Patience.** `max(0, 1 + egalitarian_sensitivity x clamp(axis / CULTURE_TRAIT_SPAN, -1, 1))` on the band's
own culture layer's `HierarchicalEgalitarian` value (negative = hierarchical); a band with no layer reads
patience 1. It scales the negative parts only, so egalitarian bands turn on a failing leader sooner.

**Delivery drifts the band toward Hierarchical.** `simulate_population` parks the counts on
`PopulationCohort::last_leadership` (`LeadershipReading { met_orders, falling_orders, hungry }`).
`advance_leadership` ends by subtracting `drift_per_met_order x met_orders` from the band layer's MODIFIER on
the axis (`CultureManager::drift_band_toward_hierarchical`, clamped to the span; the modifier, never the
resolved value, as contact drift and the personality lean). Failing orders drift nothing the other way, and
a leader's kind stays what it was at emergence. The drift lives in `advance_leadership` because that is the
leadership system that already holds the culture manager (`ResMut`); a separate system tripped the
ambiguity gate against most of the Population stage.

**Config.** `wellbeing_config.json` -> `leadership` (`LeadershipMoraleConfig`; validated finite, `>= 0`,
`starvation_full_at > 0`; Workbench rows in `tuning_manifest.json` under Wellbeing). All OPENING VALUES and
PLAYTEST DIALS:

| Lever | Opening |
|---|---|
| `popularity_per_met_order` | 0.002 |
| `strain_per_falling_order` | 0.004 |
| `starvation_strain` | 0.03 |
| `starvation_full_at` | 0.1 (share of the meal) |
| `egalitarian_sensitivity` | 0.5 |
| `drift_per_met_order` | 0.0005 (trait units) |

**Wire and save.** `PopulationCohortState.moraleLeadership:long` (appended last; fixed-point; native dict key
`morale_leadership`), part of the breakdown whose signed sum is `moraleDelta`; `moraleCause` `6` names it.
Not on the foreign-row allow-list, so a rival's row reads `0`. `SAVE_FORMAT_VERSION` 68 (`MoraleContributions`
gained `leadership`, `PopulationCohort` gained `last_leadership`).

**The Telling.** Signals `leadership.bands_hungry_under_leader` (led bands whose people went short at this
turn's meal), `leadership.bands_leader_failing` (led bands whose term is negative) and
`leadership.bands_leader_delivering` (term positive), each a count read off the cohort fields above. Two
ambient, edge-gated (`crosses` rising through 1) beats: `leadership.leader_cannot_feed_us` and
`leadership.leader_delivers`; the prose names no leader kind, so one line serves a chief and a council.
Tests: `core_sim/tests/leadership_consent.rs`.

## A leader's reach

Design: `docs/plan_labor_at_scale.md` ("A leader can only run so much well"). Code:
`LeaderReach` and its constructors in `leadership.rs`. Each leader has a **reach** (how much it runs
well) and a **load** (how much it actually runs). Past the reach orders are carried out worse, never
refused. Reach is a pure function of current state and config: no sim state, nothing saved.

| Level | Reach | Load |
|---|---|---|
| Band leader (chief or elders' council) | `reach_chief_people` people | the band's total people (`PopulationCohort::size`, the headcount the tribe code ranks bands by) |
| Tribe leader (paramount chief or council of chiefs) | `reach_paramount_bands` member bands | the tribe's member count |

Kind (hierarchical or egalitarian) does not change reach. `LeaderReach::orders_carried_out(config)` is `1` when
`load <= reach` (and for load 0), else
`(reach + over_reach_efficiency * (load - reach)) / load`, always in `(0, 1]`: reach 4, load 5,
efficiency 0.5 gives 0.9. `tribe_orders_carried_out_for_member` is the tribe leader's factor times
`over_reach_efficiency` again when the member band has no leader of its own, the same lossy shape.
Tax, aid and tribe staffing read `orders_carried_out`.

## A tribe is earned: close bands that share a gathering place, one of them chiefed

Design: `docs/plan_labor_at_scale.md` ("Leaders: four levels of orders", level 2). Code:
`core_sim/src/tribe.rs`, `tests/tribe.rs`. The tribe itself is sim state; each own band's
progress toward one is on the wire ("On the wire" below).

**Local.** Two bands are *local* on a turn when they are one people, in the same supply network
(id not 0), holding the same belief anchor tile (`PopulationCohort::belief_anchor`, the urn on the
map; `belief.md`). A fission child inherits its parent's anchor and joins its network at once, so
locality alone is not a tribe: it has to be earned.

**The points bank.** Every local pair has a bank of points (`TribeRegistry::points`, per pair,
checkpoint state). Each turn a pair is local and at least one of its two bands has a leader
(`BandLeadership.leader.is_some()`), the bank gains the pair's **rate** (`points_per_turn`, a pure
function of `CloseInputs` and the config): the sum over six inputs, each in `[0, 1]`, of
`weight x input`. The bank is clamped at `tribe_score_threshold` (100.0) and a pair is *close* when
it holds that much. A local pair with no leader on either side holds its bank, so a pair that never
had a chief stays at 0; nothing counts toward a tribe until a chief exists.

| Input (points per turn at 1) | Reads | Lever (opening) |
|---|---|---|
| Shared origin | 1 if the two bands' `PopulationCohort::founding_lines` share any `LineId::origin_band` (both descend from one starting band: a split from one another or from a common ancestor), else 0 | `tribe_weight_origin` 3.0 |
| Culture likeness | `likeness(a, b) = clamp(1 - gap / (2 * CULTURE_TRAIT_SPAN), 0, 1)` with `gap = culture::max_axis_gap` of the two bands' own culture layers: the one existing culture-distance metric, normalised by the full axis range. 0 if either band has no layer | `tribe_weight_likeness` 3.0 |
| Knowing each other | `ConnectionLedger::edges_between(a, b)`: the larger of the two directions' `Connection::strength` (`FULL_TIE` = 1) | `tribe_weight_tie` 2.0 |
| Belief on the shared urn | `CultureConfig::anchor_weight(BeliefRegistry::get(anchor))`, the weight belief already uses (`belief / (belief + belief_half_saturation)`), at the pair's shared anchor | `tribe_weight_belief` 1.0 |
| A kept monument on the shared urn | `kept_monument`: 1 while the anchor tile's `TileImprovementRegistry` entry HOLDS a rung `belief_config.json` -> `improvement_belief` lists (the shipped list is `belief:monument`, `belief.md`), else 0. The engine names no rung: the list that says which rungs add belief also says which make a monument. Held, not built: a half-raised monument (`partial_credit: on_completion`) reads 0, and an unkept one reads 0 the turn it decays to `gathering_ground` | `tribe_weight_monument` 2.0 |
| Acting as a tribe | `acting_as_a_tribe(same_tribe)`: 0 for now, and 0 for any pair outside one tribe. The tribe-level orders (#862, #864) and shared works are what will feed it | `tribe_weight_acting` 3.0 |

**Fading.** A pair that is not local loses `tribe_score_threshold / tribe_points_fade_turns` points
a turn (opening: 1 a turn, a full bank empties in 100 turns apart), clamped at 0. **Never reset on
parting.** An entry is pruned at 0 or when either band no longer exists.

The opening numbers: a pair from one origin band, alike and fully tied, earns 3 + 3 + 2 = 8 a turn
(a little more with belief), so it forms a tribe 13 turns after the first chief (`ceil(100 / 8)`).
Different origins with a full tie earn 5 (20 turns); different origins with no tie earn 3 (34
turns); a half-alike kin pair earns 6.5 (16 turns). A kept monument adds 2 to any of them. A pair
that parts keeps its bank minus 1% of a full one a turn, so a short parting is re-earned in a turn
or two and a long one has to be rebuilt.

**Who forms one.** Within each (people, network, anchor) set of local bands, take the connected
components of the close pairs. A component is a tribe-candidate iff it has `MIN_TRIBE_BANDS` (2,
"several" by definition, not a lever) or more bands AND at least one member has a `BandLeader` of its
own. Two close bands with no chief never form, however high the score; they form the turn one is
crowned. A tribe whose group loses its last chiefed band dissolves, as does one whose members are no
longer close or no longer local to one another (a band that parts leaves the same turn; its bank
fades rather than resets).

**Identity across turns** (`resolve_matches`, pure and unit-pinned) runs on those candidate groups.
The `TribeRegistry` resource holds tribes (`TribeId`, people, member `BandId`s, name, leader),
`next_id`, the points banks and a per-people cumulative `formed` count. Each tribe, in ascending
id order, picks the group of its own people that holds the most of its previous members (ties: the
group whose lowest `BandId` is lowest). Where several tribes picked one group, the one with more
previous members in it keeps it (ties: the lower `TribeId`); a loser does not try a second group and
dissolves. So a network split keeps the tribe on the majority side and the other side (2+ close bands,
one chiefed) forms a new tribe; two tribes that come to share a network and anchor stay two until
their cross pairs have banked the points to be close, then the one holding more survives and the other
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

**On the wire.** Every own band's progress toward a tribe (a chiefless band can join one) rides
`PopulationCohortState`, appended last: `tribeThreshold` (`tribe_score_threshold`), `tribeWeights` (a `TribeClosenessState`: the
six weights, the most points per turn each input can add) and `tribePairs` (one `TribePairState` per local partner
band of the band's people, partner id ascending: `partnerBand`, `partnerName`, `points`
(the banked points, on the `tribeThreshold` scale), `contribution`, each input times its weight: its
points per turn, summing to the pair's rate, and `partnerHasLeader`, whether the partner has a
leader of its own: points bank only while at least one of a pair does, so a chiefless band reads its
turns-remaining estimate off it). Parted
pairs are not published. Membership in a *formed* tribe is separate and is published to every member
band, chiefless ones included, as `tribeName`, `tribeMemberNames` (every member's name, largest
band first, ties by band id), `tribeLeaderName` and `tribeLeaderKind`
(`sim_schema::TRIBE_LEADER_*`: 0 none, 1 paramount chief, 2 council of chiefs); all four are empty
in no tribe. Whether a pair is already in one tribe is not on the wire: a band in a tribe shows the
tribe, not progress. **Reach** is appended after `tribeLeaderKind` (floats, 0 = nothing to show):
`leaderReach` / `leaderLoad` (people; set only for an own band with a leader of its own),
`tribeLeaderReach` / `tribeLeaderLoad` (member bands; set for every own member band of a formed
tribe, the `tribeLeaderName` rule) and `ordersCarriedOut` (the factor this band's orders arrive at:
`tribe_orders_carried_out_for_member` for a tribe member, the band leader's own
`orders_carried_out` for a chiefed band in no tribe, 0 with neither leader nor tribe) and `leaderOrdersCarriedOut` (appended last: the band's OWN leader's
`LeaderReach::orders_carried_out` for its reach against its load, in or out of a tribe, 0 with no leader of
its own; own bands only). A foreign
band and an expedition party carry the defaults; the capture builds them in
`snapshot::population::reach_readout`. A foreign band and an expedition party carry the defaults of
the progress fields (threshold 0, which a client reads as nothing to show) and of the tribe fields
too; an own band the registry has not scored yet carries the progress defaults. The scoring lives in
`TribeRegistry` (`scores`, the inputs of this turn's local pairs, and `standing`), rebuilt from
scratch each turn by `advance_tribes`, rather than in a derived resource: the registry is
checkpoint state, so a snapshot published right after a load still shows progress. The capture
reads it through `snapshot::population::tribe_progress`, which takes the set of live bands with a
leader (`BandLeaderLookup`) for `partnerHasLeader`.

**Checkpoint.** `SimState::tribes` (`TribeRegistry`: tribes, per-pair `points`, `next_id`, `formed`,
this turn's `scores` and `standing`;
`SIM_STATE_RESOURCES`); `SAVE_FORMAT_VERSION` 66.
