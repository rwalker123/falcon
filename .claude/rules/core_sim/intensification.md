---
paths:
  - "core_sim/src/{intensification,knowledge_ledger}.rs"
  - "core_sim/src/data/intensification_ladder.json"
---

<!-- Extracted verbatim from lines 43-43;2063-2198 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# The intensification ladder

## Config files

| File | Purpose |
|------|---------|
| `src/data/intensification_ladder.json` | **THE INTENSIFICATION LADDER** — one grammar for both food webs (`intensification.rs`, env override **`INTENSIFICATION_LADDER_PATH`**; design `docs/plan_intensification_ladder.md` §5). A `knowledge` block (**`learn_rate` 1.0 / `lesson_costs` (a map, all eight at 20) / `completion_threshold` 1.0 / `craft_lesson_per_item` 4.0** — **A LESSON COSTS PRACTICE, AND PRACTICE IS NOT WORK**: `learn_rate` is what ONE TURN of practice at the food peak is worth, charged **once per source per turn** and scaled by the assignment's floor (`intensification::learn_multiplier`), and `lesson_costs[name]` is what that knowledge costs in those units, so `20` reads as *twenty worked turns at the food peak*. **It must NOT scale with hands** — knowledge is faction-level and credited once per source per turn, so a per-worker rate would let a faction learn ten times faster by piling hands onto one patch; *you learn by watching the practice, not by counting the hands doing it*, which is why `knowledge_accrual` takes no `workers` where `build_accrual` does. **The ledger stays normalized and the cost is a divisor at the seam** (`LadderKnowledge::ledger_credit` — `DiscoveryProgressLedger` clamps to `1.0` and is shared with great discoveries, espionage and the start profiles, so `completion_threshold` stays the ledger bar and the wire's `IntensificationKnowledgeState` fields stay `0..1`). `1.0 / 20` reproduces the retired `progress_per_turn` of `0.05` exactly — the inversion ships **pacing-neutral**. The map is keyed by the **knowledge**, not by the rung that teaches it, because a knowledge can be taught by more than one rung and a **craft by none**; `craft_lesson_per_item` is the crafting arc's dial and a *sibling* rather than a reading, because the quantum differs (per **item completed at a bench**, on the same quantum as that bench tool's wear), so `lesson_costs[craft] / craft_lesson_per_item` is a craft's length in **items** (`20 / 4` → 5). It lives here rather than in `recipes.json` so every knowledge pace in the game is tuned in one file — the same reason the ladder's own moved here in slice 4 — and it **moved with the currency** from `lesson_per_crafted_item` `0.2` rather than being left as a fraction of a normalized threshold, which is exactly the drift the consolidation existed to prevent. See `.claude/rules/core_sim/crafting.md`; **moved here in slice 4 from the two identical per-web copies** in `labor_config`'s `forage.cultivation` and `fauna_config`'s `husbandry`, once the earn path became one rung-driven seam — the number paces *both* webs, so it belongs to the ladder, exactly like the build dials) plus a flat `rungs` list; each record is one rung of one branch (`plant` = forage patches, `animal` = herds): `id`/`branch`/`order`, `verb` (the **`Improvement`** — `cultivate`/`sow`/`tame`/`corral` — that fills this rung's per-source build meter — **`null` = no verb drives this rung today, and the engine skips it**), `unlock_knowledge`/`earns_knowledge` (knowledge ids the rung gates on / **teaches when practised** — `null` = ungated / teaches nothing; **both are LIVE**: `unlock_knowledge` is what every gate resolves through, and `earns_knowledge` drives `RungDef::knowledge_accrual`, the one earn seam), `requires_rung` (the rung directly below on the ladder — the ladder is strictly sequential; **a claim about the ladder's SHAPE, not a per-source precondition** — no code reads it as one, and the per-source rule differs per branch: `corral` demands a herd you already tamed, `sow` demands a gathering site and fresh water — it used to demand no prior patch at all, which #464 reversed), `ceiling_required` (the per-species `husbandry_ceiling` gate, animal branch only), **`site_requirement`** (`{ requires_gathering_site, min_forage_capacity, requires_fresh_water }` — **what the LAND must be** for the rung to be placed on a tile; the plant twin of `ceiling_required`, keyed on the ground instead of the species. `null` = the rung asks nothing of the site, i.e. **every ANIMAL rung** — a herd carries its own site with it. **All three PLANT rungs state one** (issue #464): rungs 1–3 each `requires_gathering_site`, and rung 3 adds `requires_fresh_water` on top. `min_forage_capacity` is **0 on every shipped rung** — it carried rung 3's scarcity at 195 until the gathering-site rule took that job, and stacking both demanded a curated site that also landed on one of three biomes; it stays a live dial because **rung 4 (Farm) is the rung that needs it**. **Rung 4 IS this record with `requires_gathering_site: false` and the fertility floor put back** — that is the whole of what Farm unlocks, and it is a config edit), `build` (**`work_cost`**/**`grace_turns`** — **THE SIZE OF THE JOB IN WORK UNITS** (one unit = one worker-turn at the food peak with no gear; turns are the *output*, see "An improvement costs WORK, not turns") and the **un-worked-build neglect grace**, which **no shipped rung declares** any more: every one of them counts turns of upkeep SHORTFALL instead, so its grace lives in `upkeep.grace_turns` and a second number here would be a dial nothing reads. **`decay_fraction_per_turn`, `crew_needed` and `yield_fraction_while_building` are all RETIRED** — shortfall *is* the decay (`docs/plan_standing_upkeep.md` §2.4), and see "ONE allocation per source, and BOTH standing pools are the band's" below; `null` on a rung with nothing to build), **`upkeep`** (**`work_per_turn`**/**`scaled_by`**/**`meter_decay`**/**`grace_turns`** — **WHAT IT COSTS TO HOLD THE RUNG, PER TURN, FOREVER** (`docs/plan_standing_upkeep.md` §2): the *rate* half of the ladder beside `build`'s *pile*, in the **same work units**, so *"what does it cost to hold this"* has one answer in one unit whichever rung is asked. `scaled_by` is a bounded coded primitive — the `behavior` idiom — `flat` (the rate as declared, the cost of the thing *existing*, which is what a patch owes because a patch is ONE TILE) or `source_load` (× the source's own load reading in whatever unit the rung quotes its rate in — the animal rungs quote per **keeper-load**, `head count / animals_per_herder`, which is what lets one rate say *a shepherd minds 300 sheep and a cowherd 80 cattle*; deliberately NOT a per-head rate, which would invent a 45-herder steppe megaherd out of the unit alone); `grace_turns` is consecutive turns of **shortfall** forgiven before the decay starts, and it is the rung's own number rather than a reading of `build.grace_turns` because a rung may be forgiving about an unworked build and strict about an unpaid bill. **`meter_decay` (`per_turn` / `retain_fraction`) IS THE THIRD AND FOURTH DIAL, and it is what decoupled the rot from the demand**: `per_turn` is what a WHOLLY unmaintained rung loses off its meter each turn (the loss is proportional — `(shortfall / demand) × per_turn` — so half the hands means half the rot), and `retain_fraction` is how far the meter may erode before the rung is REVOKED, as a fraction of that rung's own cost. Before the split the shortfall *was* the decay, so raising a demand made the improvement rot faster in exact proportion; and a completed meter sits exactly at its cost, so the first bleed of any size took the rung away. **`meter_decay` is NULL on both animal rungs** — an under-kept flock sheds animals at `fauna_config`'s own escape fractions, which are already the rate, and a second one here would be two numbers for one mechanic. **ALL FOUR BUILT RUNGS DECLARE AN `upkeep`.** The plant DEMANDS are whole numbers a player can staff exactly (`2` and `4`); their ROT RATES are the pacing-neutral inversion of the retired `decay_fraction_per_turn` (`0.01 × 50` and `0.01 × 75`), so the retune is provably neutral on the decay axis; the animal rates invert the retired `herders_needed` head count (`1.0` per keeper-load, so `ceil(demand)` is the count every species always asked for); and every `upkeep.grace_turns` is that rung's own former `build.grace_turns` moved across unchanged), and `behavior` (the bounded coded primitives `movement` ∈ `fixed|roam|drift_to_owner|pursue` — **read by `fauna::advance_herds`, the first live primitive (slice 3b)**; `pursue` (Predators Phase 2) is currently **diet-resolved** for a wild carnivore in `fauna::movement_primitive`, not assigned by a rung record, because the husbandry rungs are diet-orthogonal — `feeding` ∈ `photosynthesis|forage|self_graze`, `harvest` ∈ `worker_take|worker_tend|passive` — the last two still **parsed and validated only**). **Shipped rungs** (`build` quoted as `work_cost`/`grace`, `upkeep` as `work_per_turn`/`scaled_by`/`grace`): plant `wild`(1, earns `cultivation`)/`tended`(2, verb `cultivate`, gate `cultivation`, **earns `seed_selection`**, build `50`/**`null`**, **upkeep `2.0`/`flat`/rot `0.5` held to `0.75`/grace `2`** — a completed tended patch survives **28** wholly unmaintained turns)/**`field`(3, verb `sow`, gate `seed_selection`, earns nothing, build `75`/**`null`**, **upkeep `4.0`/`flat`/rot `0.75` held to `0.75`/grace `1`** — a completed Field survives **27**, `fixed`, site `{ requires_gathering_site true, min_forage_capacity 0, requires_fresh_water true }` → **174 of 4160 tiles clear the water rule** on the standard map, of which the **130–134 curated gathering markers** are what a band can actually reach — see "Placed, not conjured" in `cultivation.md`, and note the **49** this row carried until #466 came from a partial-chain test harness)**; animal `wild`(1, earns `herding`, `roam`)/`pastoral`(2, verb `tame`, gate `herding`, ceiling `pastoral`, **earns `penning`**, build `50`/**`null`**, **upkeep `1.0`/`source_load`/`2`**, **`drift_to_owner` + `worker_take`**)/`pen`(3, verb `corral`, gate **`penning`** (slice 4's §4.3 reshuffle — was `herding`), ceiling `pen`, **earns `foddering`** (Flora Roster F3 — running a pen teaches you to hay it; unlocks the fodder-draw, not a rung), build `75`/**`null`**, **upkeep `1.0`/`source_load`/`6`** — the same rate as pastoral, because a penned animal is not less work to mind, and a LONGER grace, which is what the fence buys (the turns-side statement of `pen_escape_fraction < pastoral_escape_fraction`), `fixed`). **The two webs' graces are not monotone in the same direction, and that is why the dial is per-rung**: on plants the NEWEST rung is the most fragile (a standing crop wants hands every turn; the cleared ground under it keeps its clearing longer), on animals the HIGHEST is the most forgiving (the fence does the holding). All four are playtest anchors. **The file describes what the sim does TODAY, deliberately** — later slices change behaviour by *editing it*. **Validated** — `LadderConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention): unique `(branch, id)` and `(branch, order)`, exactly one order-1 rung per branch, `requires_rung` resolving to a real same-branch rung at `order - 1` (and `null` iff `order == 1`), `verb` parsing to a real `Improvement`, `unlock_knowledge`/`earns_knowledge` resolving to a known discovery id, `0 < work_cost` finite, **`grace_turns < work_cost / reference_output` when present** — and the identical bound on **`upkeep.grace_turns`**, since either trigger's grace outlasting its own build makes the penalty evaporate — where `reference_output = SOLE_BUILDER × PER_WORKER_OUTPUT` (a grace that outlasts its own build makes walking away free for the whole span it took to build — a penalty evaporating silently, the time-axis twin of the site rule that requires nothing; one builder is the LONGEST the build can take and therefore the loosest the bound can be, which is the safe direction for a guard, and it replaced a `crew_needed` divisor when the rung stopped declaring a crew), **`upkeep.work_per_turn > 0`** finite **when the block is present** (a parked `0` is rejected because it means *"no upkeep"* while reading like a live dial; say `upkeep: null` — the same rule the retired `decay_fraction_per_turn` followed), **`upkeep.meter_decay.per_turn > 0`** finite and **`upkeep.meter_decay.retain_fraction` within `0..=1`** when that block is present (a rate of `0` says *"this never rots"*, which the config already says by declaring no `meter_decay` at all; a fraction above `1` would revoke the rung on the turn it completed, which is the very defect the bar exists to fix), and **`upkeep.scaled_by` parsing to a real variant** (the `behavior` idiom: an unknown token fails the *parse* rather than resolving to a default nobody chose), a `site_requirement`'s `min_forage_capacity` finite & `>= 0` **and the requirement actually requiring something** (a floor of `0` with `requires_fresh_water: false` **and `requires_gathering_site: false`** admits every tile — a placement rule that places no rule, which is how a rung's scarcity evaporates silently; say `null` instead), **`knowledge.learn_rate > 0`** finite (else nothing is ever learned and the ladder silently freezes at rung 1), **every `lesson_cost > 0`** finite (a free lesson is known before it is learned, so every gate it holds is open on turn 1), **every knowledge the ladder can teach PRICED** — each rung's `earns_knowledge` and every craft (`crafting::CRAFTS_WITH_A_DISCOVERY`); a missing entry is a load failure rather than a silent default, because a defaulted pace is a number nobody chose — **`craft_lesson_per_item > 0`** finite, and **`0 < knowledge.completion_threshold <= 1`** (at `0` every gate opens on turn 1; above `1` no gate can ever open, since the ledger clamps accrual to `1.0`) — all **stated once, for both webs**, having moved from each web's own config — and **every rung the engine names by hand (`RungKey`) present** (so a broken override cannot silently no-op a shipped rung); a broken invariant is logged at **error** level (`intensification_ladder.invalid_rejected`) and the builtin is used. See "The Intensification Ladder" |
## The Intensification Ladder

**One grammar for both food webs** (`intensification.rs`, config `src/data/intensification_ladder.json`;
authoritative design: `docs/plan_intensification_ladder.md`). Plants and animals climb the *same*
three-rung ladder — rung 1 you take what's there, rung 2 you manage the wild source in place, rung 3 you
control its reproduction — and every rung-transition is the same **Cultivate-shaped verb**: pick it → the
source pays a **reduced** yield while the crew prepares rather than harvests → a **per-source build
meter** climbs → it decays if you walk away → at the job's declared cost the source steps up a rung.

### An improvement costs WORK, not turns

**A rung declares a fixed [`RungBuild::work_cost`] in WORK UNITS; a crew produces work units per
turn; TURNS ARE THE OUTPUT** (`docs/plan_unit_costed_work.md`).

```text
work_this_turn = workers × PER_WORKER_OUTPUT × learn_multiplier(floor)
progress      += work_this_turn                       // absolute units on the source's own meter
cost           = rung.work_cost × source_cost_multiplier          // the RAW job — what is stamped
t              = Σ over the crew (EquipmentStat::BuildWork)       // what the TOOLS take off it
effective_cost = cost − t
complete when  progress >= effective_cost             // …and the meter is then set to `cost`
```

**One unit is one worker-turn at the food peak with no gear** (`intensification::PER_WORKER_OUTPUT`
= 1.0), so `work_cost: 50` reads itself and needs no second dial to interpret. It is deliberately
**not** a config lever: a tunable worker output would be a second authority over the same pacing, and
the cost side is the one this arc exists to expose. Worker output is written as a **sum of terms**
with exactly one term today — knowledge does **not** feed throughput, it reaches it through the tools
it unlocks — so a future buff mechanic has somewhere to land.

**What the normalized `0..1` meter made unreachable**, and this is why it was inverted rather than
retuned: every improvement on both webs was literally the same 25-turn job (all four rungs declared
`progress_per_turn: 0.04` against a `RUNG_COMPLETE` of `1.0`), a rung up the ladder could only be a
*bigger* job by declaring the crew *worse at it*, a tool could only be a multiplier and a multiplier
cannot be scale-sensitive, and gear cost was size-blind because every build totalled `1.0`.

- **`RUNG_COMPLETE` is RETIRED. Each rung has its own completion value**, and — because
  `is_cultivated()` / `is_field()` / `is_domesticated()` have ~a hundred call sites all over both
  webs and cannot each take a config — **every meter carries a stored companion cost**:
  `ForagePatch::cultivation_cost` / `field_cost`, `Herd::domestication_cost` / `corral_cost` /
  `pen_extend_cost`. The accrual seam **stamps** the live resolved cost while the meter is incomplete
  and **never re-stamps once complete**, so a later config retune that raises a price cannot silently
  *un*-cultivate ground the player already paid for; decay floors the meter at `RUNG_UNSTARTED` and
  resets the companion with it. Every predicate is `cost > RUNG_UNSTARTED && progress >= cost` — the
  positive-cost half is load-bearing, since `0 >= 0` would read every wild source on the map as
  finished. The costs ride the checkpoint for free (`SimState` clones the whole
  `ForageRegistry`/`HerdRegistry`).
- **THERE IS NO CREW CAP.** `crew_scale` (`min(workers / crew_needed, 1)`) and `FULL_CREW_SCALE` are
  **deleted**: fifty workers finish a Cultivate in a turn, and that is allowed. The constraint is
  opportunity cost across systems, not a rule forbidding a play style — today only food pushes back;
  crafting throughput, defence and trade arrive as those systems land. **`crew_needed` itself is gone
  too**: the player states a build's crew outright (`docs/plan_standing_upkeep.md` §2.2), so there is
  no rung-level staffing left to floor a blended head count with — see "Three allocations per
  source".
- **The ANIMAL web's turn counts MOVED, and that is the point.** Both animal rungs were once
  crew-**BLIND** — a `Tame` took 25 turns whether two hands or twenty worked the herd. Every build is
  crew-scaled now, and the crew is **the player's own number** on the verb rather than a reading of
  the herd's keepers, so animal pacing is a staffing decision like the plant web's. The wild→pen
  climb is **knowledge-paced**: the two ~20-turn lessons dominate the two build legs.
- **The plant web is PACING-NEUTRAL at the reference crew**, which is this slice's own proof:
  `50 / crew 2 = 25` turns for a Cultivate and `75 / crew 3 = 25` for a Sow, exactly as before. The
  animal costs (50, 75) are a **reference-crew choice** — 2 keepers and 3 — rather than a derivation,
  because those rungs had no crew to multiply by.
- **THE GEAR LANDS ON THE JOB, NOT ON THE CREW** (`docs/plan_unit_costed_work.md` §6). A tool takes
  a fixed number of work units off the *cost*; it does **not** multiply the accrual. A multiplier
  cancels the cost (`turns_geared / turns_bare = w / (w + h)` for any job), so it would save the same
  *percentage* of turns on a garden and on a farm alike — the shape the arc exists to escape.
  Subtracted, the job's own size decides, and **the tool never names an improvement**. See
  `equipment.md` → "The build axis".
  - **The STAMPED companion cost stays the RAW job**, un-tooled and stable, because that is what the
    four completion predicates compare against — a bar that moved with the crew's kit would
    *un*-complete a rung when a tool wore out. The offset applies at the **completion comparison
    only**, and `forage::banked_or_paid_off` then sets the meter to the raw cost: the jump is the
    units the tool pre-paid, which keeps the published fraction at exactly `1.0` and makes the gear's
    wear charge (billed off the meter's own delta) come to the whole job.
  - **`build_turns_remaining` reads the EFFECTIVE bar**, or the estimate lies to a geared crew;
    **the upkeep reads neither.** Better tools make a build arrive sooner; they do not make an
    unkept one forget more slowly, because what it forgets is what its keepers did not supply.
- **On the wire the meter is still a `0..1` fraction.** `intensification::build_fraction` divides at
  **capture**, against the source's *own stamped* cost, so `cultivationProgress` / `fieldProgress` /
  `corralProgress` / `domestication` and `isCultivated` / `isField` / `corralled` are unchanged in
  type, meaning and range and every shipped readout keeps working. See "The build on the wire".

**The ladder is DATA over a bounded set of coded primitives.** A rung is a [`RungDef`] record, the ladder
is a list, and adding a rung that recombines existing primitives is a one-record edit. See the
`intensification_ladder.json` row in Configuration Files for the record shape and the shipped rungs.

### An assignment has TWO axes: a floor and an improvement

**The rung-transition verbs are not a fifth, sixth, seventh and eighth harvest policy** (issue #442,
`docs/plan_investment_rung_toggle.md`). A labor assignment carries two independent facts, and they
live in two slots:

| Axis | Question | Type | Values | Where |
|---|---|---|---|---|
| **Pressure** | how hard do I pull? | one continuous `f32` | the **escapement floor**, a fraction of `K` in `0.0..=1.0` | `LaborTarget::{Forage,Hunt}.floor` |
| **Improvement** | what am I building? | optional, at most one | plant `Cultivate` → `Sow`; animal `Tame` → `Corral` | `LaborAssignment.improvement` |

- **The floor is never written by the sim.** Completion clears `improvement`; the floor stays exactly
  as the player set it (see "Completion CLEARS the improvement" below).
- **A deep floor beside a running build is LEGAL**, and it is not gated. It is *priced*.
  > **THE PRICE IS THE BUILD'S OWN SPEED** (`docs/plan_harvest_floor.md` §3, slice 3). Both the build
  > accrual and the knowledge accrual are multiplied by
  > **`intensification::learn_multiplier(floor) = floor / MSY_BIOMASS_FRACTION`** — normalised so the
  > food peak is ×1.0, which is why nothing needed retuning at the floor a fresh assignment carries.
  > A crew pulling hard on the source it is improving builds *slowly*, in proportion; a crew stripping
  > it builds nothing at all.
  >
  > **This closes the balance question this callout used to leave OPEN**, and the two halves of the
  > fix are separate. The old justification was that the ecology would discipline a harsh builder —
  > the meter accrued only while the source was `Thriving`, and the dip rode the harsher stance's
  > larger ceiling — and `core_sim/src/forage/stance_probe.rs` (`#[ignore]`d; run with
  > `cargo test -p core_sim --lib stance_probe -- --ignored --nocapture --test-threads=1`) measured it
  > failing. Dipped ×0.25, **every plant floor stayed Thriving for the whole build and all four
  > completed in exactly 25 turns**, with the harshest paying **3.8× the food** for it; on animals the
  > stall was real only for a fast breeder primed to `K/2` and did not generalise from a full herd;
  > and rung 3 had no health gate at all, so every floor finished on schedule on both webs.
  >
  > **Two things changed, and neither is a retune.** (1) The `Thriving` gate is **deleted** on both
  > webs and replaced by the rate above — a slope where there was a cliff, with no lapse state left to
  > hold progress across. (2) **The dip moved off the ceiling and onto crew throughput** (§3.1), and
  > has since **retired entirely** — see "ONE allocation per source, and BOTH standing pools are the
  > band's". **The build is staffed in its own right, on the band's `builders` pool**, so a deep draw
  > cannot build with a crew it is not paying for: §0.3's defect cannot recur for the stronger reason
  > that there is no shared crew at all.
  >
  > **(3) And the FLOOR then came off the build rate.** The rate above described people pulling on the
  > source; a build crew is not pulling, and a build crew on a source nobody is harvesting has no
  > floor to read. `learn_multiplier` is a knowledge term now, and the two tests that pinned the
  > build's floor sensitivity
  > (`fauna_husbandry::a_low_floor_tame_takes_materially_longer_than_a_food_peak_one` and its plant
  > twin) retired with it. Pacing at the default floor is unchanged, which
  > `taking_the_floor_off_the_build_rate_is_pacing_neutral_at_the_food_peak` proves.
  >
  > **Rung 3's "missing" health gate is now correct by uniformity** — no rung has one; every rung has
  > a rate.
- **Kind-exclusivity is exhaustive, not a complement.** `Improvement::valid_for_forage` /
  `valid_for_hunt` are two exhaustive matches, so a new verb fails to compile until someone states its
  web; the retired `valid_for_*` stance predicates were hand-written `!matches!` complements that would
  have defaulted a new verb to legal on **both**.
- **The stance-set predicates `EXTRACTIVE` / `is_investment()` are deleted.** A set-membership predicate over one
  enum is unnecessary once the sets are different types. Its doc recorded two hand-written lists that
  had rotted (`send_hunt_expedition`'s launch gate silently accepted `tame`, and
  `hunt_expedition_floor`'s `matches!` was missing it too); **both guarantees are now type-level** —
  `ExpeditionMission::Hunt` carries a **floor** (an `f32`), which cannot *name* a build verb, so the launch
  gate rejects one through the ordinary parse and the unreachable-arm `debug_assert!` is gone.
- **Commands.** `assign_labor … [floor] <workers>` sets the FLOOR — a `0.0..=1.0` fraction of `K`, not
  a stance word (the four are refused at parse with `CommandParseError::RetiredStanceToken`) — and
  **never touches the improvement** — which is what makes a *paused* build re-staffable (`validate_labor_policy` no longer
  sees the verb, so the `Cultivate` start gate is not re-asserted). The improvement is set by its own
  verb: `cultivate` / `sow` / `tame` / `corral`, all four routing through
  `queue_build_on_working_bands` + `validate_improvement`. **None of them names a crew** — see
  "THE QUEUE IS THE DECLARATION" below.

> #### THE BUILD VERB IS **DERIVED FROM THE METER**, and the stored one is only a declaration
>
> `forage::patch_build_verb` / `fauna::herd_build_verb` are the one seam
> (`docs/plan_standing_upkeep.md` §2.4). Per **meter**, not per source:
>
> | meter | state | who declares |
> |---|---|---|
> | **zero** | nothing banked | **the player**, through the band's **build queue** — a wild patch could climb to tended *or* be sown, and the sim cannot guess |
> | **between zero and its cost** | building that rung, **implied** | nobody — the progress banked on it *is* the answer |
> | **at its cost** | nothing left to raise | nobody — the entry **leaves the queue** |
>
> **Newest meter first**, exactly as `patch_unwinding_rung` resolves: a Field with any progress on it
> governs the tended ground beneath, so a `Cultivate` declared on a Field is **dead** rather than
> stalled, and a slipping Field can never re-imply a Cultivate.
>
> **What it fixed.** `RungDef::build_accrual` banks nothing unless the rung's verb is in flight, and
> completion freed the declaration — so a completed rung that eroded back below its cost re-entered
> the *building* state with nothing set, and the derivation is what lets the player see **which** job
> a repair is without working it out.
>
> ⛔ **BUT AN ERODED RUNG IS NOT RE-ADOPTED** (`docs/plan_standing_upkeep.md` §2.4). Deriving the verb
> says *what* a repair would be; it does not put the source back in the **queue**. Repairing is a
> fresh decision the player makes by re-queueing — which is what keeps a one-percent-eroded Field
> from displacing the build they actually ordered off the head of a pool funded all-hands-on-one.
> `forage_cultivation::an_eroded_rung_is_still_funded_by_the_keeping_pool` and its repair twin pin
> both halves.
>
> **RETIRED with it: `abandon_improvement`** (the command, the alias, `handle_abandon_improvement`,
> `describe_source`, `improvement_event_kind`, and proto field 46 — **reserved, never reused**). It
> existed to let a player walk away from a 25-turn commitment while the *verb* was the commitment;
> the commitment is the **hands** now. A command that cleared a *derived* value would either do
> nothing or fight the derivation, and both are worse than not having it.
>
> > #### THE UNDO IS `unqueue`, AND `abandon` PUTS THE WHOLE SOURCE DOWN
> >
> > A declaration used to be unwithdrawable: `cultivate <f> <x> <y> 0` *set* `improvement =
> > Some(Cultivate)` with a crew of zero, and `patch_build_verb` honours a declaration whenever its
> > meter is at zero — so the source read as **building, forever, with no builders and no undo**.
> >
> > A declaration carries no crew at all now (§2.5), so **withdrawing it is an ordinary list edit**:
> > `unqueue <faction> <x> <y>` / `unqueue <faction> <herd_id>` drops the queue entry and leaves the
> > row, its take crew, its kit and the meter exactly as they are. `abandon`, on the same source
> > grammar, is the heavier one — it drops the band's whole **holding**, row and entry together, and
> > leaves the meter to rot back down at the rung's own rate. Neither destroys anything on the spot,
> > which is why neither asks for a confirmation.
>
> **RETIRED with it too: the "nothing left to build" test** (`forage_rung_already_built` /
> `hunt_rung_already_built`). Its one job was to clear a verb the sim would otherwise have driven on a
> finished rung — a stale declaration now derives to `None` on its own.

- **On the wire:** `LaborAssignment.improvement:string` (`""` = a pure harvest) beside the
  now-retired `policy`; the per-source ceiling lists have since gone entirely (a continuous floor
  cannot be enumerated), and so have the two build fractions that replaced them — a building crew
  takes nothing, so there is no factor left to publish. See "The standing upkeep on the wire" for
  what rides those tables now.

### The build engine — THE seam both tracks call

`RungDef::build_accrual(improvement, eligible, workers)` / `build_cost(cost_multiplier)`, plus
`LadderConfig::effective_build_cost(cost, gear_work)`, are the **single** source of a rung's build
math, and `upkeep_demand` / `upkeep_decay` / `upkeep_grace_turns` of its standing cost. Both food webs
call them instead of reaching for their own bespoke accrue/cost/decay levers, so the two ladders
**cannot drift apart numerically** — that is the whole reason the dials moved out of
`labor_config`/`fauna_config` and into the ladder. (`build_decay` is **retired** with the
`decay_fraction_per_turn` it read: shortfall is the decay.)

- **`build_accrual`** returns **the WORK UNITS THE BAND'S BUILDERS PRODUCE this turn** —
  `builders × (PER_WORKER_OUTPUT + gear per worker)`, the pool's whole head count and no floor term,
  and only for the
  source at the **head** of that band's queue — **only** when
  `improvement` **is** the rung's own `verb` *and* the caller's rung-specific gates hold (`eligible` —
  knows the unlock knowledge, **the crew took something**, species ceiling allows, faction owns it);
  otherwise `0`. **A rung with `verb: null` is never driven** — which is what keeps the two `wild`
  rungs (nothing to build) out of the engine.
- **`build_cost` — the completion target**, `work_cost × cost_multiplier`, `None` for a rung with
  nothing to build. The caller stamps it onto the source's companion cost field and completes the
  rung when the meter reaches it.
- **`build_turns_remaining(cost, done, work_this_turn)`** is the one place `ceil((cost − done) /
  work)` lives, so the wire's `buildTurnsRemaining` cannot drift from the meter it describes. `None`
  means **no estimate**, and only a **stall** earns it — a crew producing nothing has no finite
  answer, and a huge number would read as a promise.
  > **A cost the meter is already at or past is `1`, not `None`** (`BUILD_FINISHES_IN_ONE_TURN`) —
  > the work is already banked, so one more turn finishes it. **Answering `-1` there would break the
  > arc's headline claim**, and it is pinned at the seam and on the exported snapshot
  > (`build_turns_closed_form.rs`), in both the projection and the live stamp.
  >
  > **The OTHER state that used to reach this branch is gone with the subtraction**: a crew whose gear
  > "paid the job off outright" drove the bar below zero and completed on turn one having banked
  > almost nothing. A job is only finished by work someone did now, so the sole route to one turn is
  > out-producing what remains.
- **`LadderConfig::projected_build_turns` — the same question asked of a rung nobody has started.**
  It assembles exactly the four calls the in-flight stamp makes (`build_cost` →
  `build_work_from_gear` → `effective_build_cost` → `build_accrual`, then `build_turns_remaining`)
  against a stated `banked` and the caller's composed `eligible`, so a quote for an unstarted job
  cannot be arithmetic the running build would disagree with. It is what makes `buildTurnsRemaining`
  a **projection** rather than a `-1` — see "The build on the wire".
> #### ⛔ RETIRED — `effective_build_cost`, and with it the idea that a tool shrinks a job
>
> **A KIT RAISES WHAT A WORKER DELIVERS PER TURN. A JOB'S WORK REQUIREMENT NEVER CHANGES.** A 50-work
> Cultivate costs 50 work with hoes, without hoes, and with any tool that ever ships; gear decides how
> fast the pile is worked off and never how big it is (`docs/plan_standing_upkeep.md` §4.8).
>
> The retired form was `effective_build_cost(cost, gear_work) = cost − workers × gear`, with nothing
> under it. **Two things were wrong with it, and neither is the degenerate case** — that a large enough
> gear value finishes a job outright is a CONFIG problem and never an argument about the model:
>
> - **It granted the kit's help as a LUMP, once, against the target** — however long the job ran. A
>   tool is used every turn it is held, so its help belongs on the rate.
> - **It has nothing to subtract from on an UPKEEP**, which is a rate rather than a pile. So gear could
>   only ever touch builds, and making a hoe matter to *keeping* a Field would have needed a second,
>   unrelated mechanism beside it.
>
> **One supply expression now feeds both accounts** — `pool_work_supply(workers, gear_per_worker)` =
> `workers × (PER_WORKER_OUTPUT + gear)`. A build divides its pile by it to get turns; an upkeep
> compares its demand against it to see whether it is covered. What that gives up is scale-sensitivity:
> a multiple saves the same *percentage* of turns on a garden and a farm alike, where a subtraction
> nearly freed a small job and barely dented a large one. That is accepted.
>
> **THE CONSTANT WAS A UNIT CONVERSION AND AN EXACT ROUND TRIP, NOT A TUNING CHOICE.** `build_work`
> shipped at `8.5` meaning *units off the job, per worker* — and that 8.5 was itself minted from a
> still earlier `build_rate` **×1.5** multiplier on the crew's output. This model is a per-worker
> output term again, so inverting the mint needs no reference crew and no reference job:
> `PER_WORKER_OUTPUT + build_work = 1.5` → **`build_work = 0.5`**. **Hoes are +0.5 build work per
> worker per turn; hurdles are +0.5** — the same tools they always were, and an equipped worker is
> 1.5× a bare one. Carrying 8.5 across unchanged would have meant a worker delivering **nine and a
> half times** a bare one, which is why the old number is meaningless in the new units. Every number
> here is provisional until the arc's tuning spread (§4.14).
>
> **A NUMBER TRAVELS WITH THE ITEM THAT OWNS IT.** Write "hoes are +0.5 build work per worker per
> turn", never "`build_work` is 0.5" — the bare value names no owner and no per-what, and that
> ambiguity has already cost one detour.

- **`gear_work_supply` — what the CREW BROUGHT, as a RATE** (issue #515, `equipment.md` → "The build
  axis"). It sums `EquipmentConfig::build_work_per_worker` over the crew through the coverage seam and
  is a **readout** of what the pool's kits add per turn. `intensification::NO_BUILD_GEAR` (**`0.0`**)
  for a crew carrying nothing that helps.
  **THE GEAR TERM IS QUOTED AT THE BUILD'S OWN CREW**, and so is the coverage behind it
  (`docs/plan_standing_upkeep.md` §2.2). `build_work_per_worker` is a **rate per worker**, so the
  count it multiplies has to be the workers actually doing the job — which means
  `systems::labor` resolves a **second `KitCoverage`** over the builders, beside the take crew's.
  Every other tier that coverage answers (the hunt haul, the pen collection, the gather carry) is a
  *take* rate and is rightly averaged over the take crew; this one is a **sum off the job**, so the
  average and the count must be over the same people or the product is neither. Averaged over six
  gatherers and multiplied by two builders, one set of hurdles takes a third of what those two
  builders are carrying — and since `effective_build_cost` is unfloored, the mirror-image error (a
  band-wide count on a one-hand build) lets a lone builder beside a large party pay a whole rung off
  outright. `the_gear_offset_scales_with_the_build_crew_and_ignores_the_take_crew` pins all three
  readings: it scales with the builders, it saturates at the units held, and it does **not** move
  when only the gathering crew does.
  **A kit may not shrink a job at all** — how much faster it is worked off is the tools' own dial, and
  later work being *impractical* bare-handed is expressed by the multiple, never by the pile getting
  smaller. `build_fraction` and `build_turns_remaining` both read the raw stamped cost, because there
  is no longer a second cost for them to disagree about.

  **AND THE SAME SUPPLY REACHES KEEPING NOW.** `maintenance_shares` pools
  `workers × (PER_WORKER_OUTPUT + gear)` for the web's keeping role, so an equipped keeper covers more
  of a rung's demand — the demand itself is untouched, which is the build rule's mirror one account
  over. `tillage` gained the `agriculture` job and `hurdling` the `husbandry` one so the pools have
  something to derive; the derivation is per web, needing no player pick, exactly as the builders row's
  is per entry. **`default_kits` for both is `none`, and a stored `none` could beat that derivation
  the way it did on the builders row** — the same fork guards all three now.

  > **KNOWN HOLE — a keeping tool never wears.** `WearQuantum` has `BuildProgress` and no upkeep
  > quantum, so a hoe raises what a keeper supplies **forever, free**. A wear quantum for upkeep is a
  > new charge site and would move `_comment_durability`'s tuning and the `headline_wear` readouts,
  > so it is not in this slice.
- **NO `learn_multiplier(floor)` TERM** — see "THE FLOOR CAME OFF THE BUILD RATE". `build_accrual`
  takes no floor at all, and neither does the upkeep: what an improvement loses is the work its
  keepers did not supply, which is a fact about a crew and a rung rather than about how hard anyone
  is pulling on the source.
  `accrue_field` and the `Corral` arm still omit the *work predicate* from `eligible` — rung 3 never
  had the `Thriving` gate it replaced, and bare ground stands below every floor by construction.
- **`ExtendPen` names a QUEUE KIND, on the four verbs' own grammar** (`extend_pen <faction> <x> <y>`).
  It rides the same `animal:pen` rung as the pen it widens, so it waits in the same queue, is funded
  by the same `builders` pool and reads the same gear — a ring cannot drift from the initial build.
  **It is the one entry kind that names no rung verb**: a built pen carries no meter for
  `herd_build_verb` to answer from, so `BuildJob::ExtendPen` states it instead. That is exactly the
  gap the queue had to fill when the ring stopped naming a crew. Riding the assignment's *take* crew,
  which it did while the investment dip was the only cost of a build, made widening a fence the one
  build in the game that cost nothing the moment the dip retired.
- **THE GRACE — the consecutive turns a rung forgives** before its penalty starts. Both webs count on
  a `neglect_turns: u16` (on `ForagePatch`/`Herd`) and gate different penalties on it: the plant
  meters bleed (`forage::advance_cultivation`), the animal flock sheds (`fauna::advance_husbandry`).
  **Both count the same thing** — consecutive turns of **upkeep shortfall**, read through
  `upkeep_grace_turns()`. `RungBuild::grace_turns` (the *un-worked-build* trigger) is `null` on all
  four shipped rungs precisely so there is no second number nothing reads. **The rung is resolved
  through one seam per web** — `forage::patch_unwinding_rung` and `fauna::herd_keeping_rung`, both
  *"the newest meter with progress on it"* (the plant web unwinds newest-first, and a half-raised pen
  is already the thing at risk; neither is `herd_rung`/`patch_rung`, which answer which rung is
  *completed* and would give a source mid-investment the least forgiveness on the ladder) — and **the
  wire's countdown reads the same seam**, so a published "lapses in N turns" cannot describe a rung
  the sim is not acting on.
  `intensification::neglect_grace_remaining` owns the arithmetic: `(grace + 1) − neglect`, floored at
  zero, so **`0` means the penalty is biting now** and the client subtracts nothing.
- **The COST MULTIPLIER — the rung owns the mechanic, the source is priced.** It prices the **job**
  and nothing else: `build_cost` reads it, and the upkeep deliberately does not. A rung's bleed used
  to be a fraction of its own `work_cost`, so it rode the multiplier and the build:decay ratio stayed
  invariant per source for free; **shortfall is the decay now**, and what an improvement loses is what
  its keepers did not supply — a fact about the crew and the rung, not about how big the job was. A
  Steppe Runner is five times the work to tame and forgets at whatever rate its own upkeep names; the
  two are simply different questions. Today the only multiplier is a species' **`taming_cost_multiplier`** on
  `animal:pastoral` (`FaunaConfig::taming_cost_multiplier_for`, resolved live by display name); every
  other caller passes **`RUNG_COST_UNSCALED`** (the plant `tended` patch and `field`, the `pen` and
  its `ExtendPen` rings — penning is a flat job for every species: a fence is a fence). See "The
  `Tame` verb" for the inversion from the retired `taming_rate`.
- **The per-source state does not move.** `ForagePatch::cultivation_progress`,
  `Herd::domestication_progress` and `Herd::corral_progress` stay where they live — each now beside a
  **stored companion cost** — and the engine supplies the *amount*, while the source owns its meter,
  the clamp to that cost, and the side-effects of completing it (ownership, `corralled_at`, the feed
  line).
- **Callers.** Accrual: the `Cultivate`, **`Tame`** and `Corral` arms of `advance_labor_allocation`
  (Population) — the *same* call, once per rung. Decay: `forage::advance_cultivation` and
  `fauna::advance_husbandry` (both Logistics; **the one-turn lag is deliberate** — each reads what the
  labor arm wrote *last* turn: `ForagePatch::upkeep_supplied` on the plant side, and the surviving
  `Herd::corralled_tended_this_turn` flag on the animal side until its own slice lands); a pen is
  lost outright with its herd rather than bleeding a meter.
  The take reads its crew off the row (`LaborAssignment::workers`) and both standing pools off their
  own band-level rows (`LaborAllocation::workers_on(&LaborTarget::{Builders, Agriculture,
  Husbandry})`), and the take path is read by
  `forage::forage_take`, `systems::hunt_take`, both forward projections and
  `fauna::forecast_expected_take` alike, and the improvement axis moves none of them — so
  **forecast == actual** for free (see "Pre-commit Yield Forecast"). **Extending** a pen (2d-β) reads
  the *same* `animal:pen` rung, so a ring can never drift from the initial build.
- **THE BUILD IS NOT IN THE TAKE AT ALL** — see "ONE allocation per source, and BOTH standing pools
  are the band's", which is where the
  retired `yield_fraction_while_building` went. Neither the ceiling nor the crew carries a build
  term, which is what makes `SourceYieldForecast::ceiling_at` linear in terms already on the wire and
  so composable by the client — see "THE CEILING LISTS ARE RETIRED" in `yield-forecast.md`.
- **Completion CLEARS the improvement — ONE seam for all four rungs.** A build verb only means "the
  crew is preparing, not harvesting", so once a meter fills it names a rung that can never accomplish
  anything more on that source and the crew's whole budget would be spent on it forever, for nothing. Each of the four
  accrual arms records the completing assignment's index, and a single post-loop pass in
  `advance_labor_allocation` sets that assignment's `improvement` back to `None`, preserving the
  source, the committed species, the worker count **and the floor**. **The completing turn still
  pays the build's whole price** (accrual is after the take), so the crew starts gathering again the
  next turn; the
  pass runs **before** the lapsed-assignment removal, which invalidates indices. A rung whose gate
  merely **lapses mid-build** is untouched — nothing completed, so the source keeps its verb and its
  progress.
  - **It announces ONCE, and it no longer has to clear anybody's verb.** The four verb commands set
    the declaration on *every* band working the source, so a completion is always a many-bands event
    even though only one crew's accrual crosses the bar — which is why each build arm's **feed line**
    rides the *transition* (`accrue_cultivation` / `accrue_field` / `accrue_domestication` all answer
    *"did this call finish it"*, `accrue_corral`'s long-standing convention). The **clearing** half is
    gone: a stale declaration on a finished meter derives to `None`, so the second crew's `Sow` that
    used to be permanent, and the `cultivate` on a wild-sown Field that used to stall forever, are
    both unreachable by construction rather than by a test that had to be asked in the right place.
  > **It used to rewrite `policy` onto a module constant `HARVEST_POLICY_AFTER_BUILD`
  > (the food peak)**, because the build verb had occupied the pressure slot and completion had
  > to hand *something* back — so the sim silently replaced the player's stated policy on a turn they
  > could not predict, and each completion event carried a `retired_policy=sustain` detail. The
  > constant, its ten call sites and that detail token are all deleted (issue #442): the stance was
  > never vacated, so there is nothing to restore.

### ONE allocation per source, and BOTH standing pools are the band's

**A source row states the TAKE, and nothing else** (`docs/plan_standing_upkeep.md` §2.5):

| Activity | Where the hands are | Set by | On the wire |
|---|---|---|---|
| **take** | `LaborAssignment::workers` on the source's row | `assign_labor <faction> <band> forage\|hunt …` | `workers` |
| **keeping** | `LaborTarget::Agriculture` / `LaborTarget::Husbandry`, a **band-level row** | `assign_labor <faction> <band> agriculture\|husbandry <n>` | a row of `laborAssignments`, `kind` = the role |
| **building** | `LaborTarget::Builders`, a **band-level row** | `assign_labor <faction> <band> builders <n>` | likewise, `kind = "builders"` |

**Both standing crews left the tile, one slice apart, and for the same reason**: an indivisible
supplier meeting a per-source demand wastes whatever it does not spend, and the waste grows as gear
makes a hand worth more. A pool has no leftover by construction.

> #### THE QUEUE IS THE DECLARATION, and a verb no longer names a crew
>
> `cultivate` / `sow` / `tame` / `corral` / `extend_pen` **append an entry** to
> `LaborAllocation::build_queue` and staff nobody. The whole `builders` pool goes on
> `build_queue[0]` until that entry's meter fills, then on the next — **all hands on the head**.
>
> | type | is | why not a `LaborTarget` |
> |---|---|---|
> | `BuildSource::{Patch, Herd}` | which source an entry names | a target carries the take crew's **floor** and **species**, which are facts about gathering rather than about building |
> | `BuildJob::Rung(Improvement)` | the player's declaration, answering only for a meter at zero | — |
> | `BuildJob::ExtendPen` | the pen ring, which names no rung | a built pen has no meter for the derived verb to answer from |
>
> - **At most ONE entry per source per band**, and re-issuing a verb **replaces `declared` in place
>   and keeps its position** — correcting `cultivate` → `sow` must not cost the player their place.
> - **An entry requires a ROW.** A verb reaches only bands that already work the source, and a row
>   that dies takes its entry with it — enforced at the seams (`drop_source_row`, `clear_kinds`) and
>   swept per turn by `LaborAllocation::prune_build_queue`, so no seam can be missed.
> - **Nothing enrols itself**, and completion **retires** the entry, which hands the pool to whatever
>   the player put next.
> - **An entry whose declared job is ALREADY STANDING is retired too**, on the finished verb's own
>   feed channel (`status=already_built action=build_retired`) — see the callout below.
> - **⛔ `build_queue` is part of `LaborAllocation`'s hand-written `PartialEq`.** Equality is *intent*,
>   and the queue and its **order** are as much intent as the head counts: left out, two allocations
>   with different queues compare equal, so the rollback record and the command no-op guard would
>   both report *nothing changed* about the one input the whole funding rule reads.
>
> **Spread is not offered, and the asymmetry with the keeping is honest.** An under-kept improvement
> degrades toward a threshold you can stay above, so spreading a short keeping pool loses nothing
> while you recover; splitting a builder pool across three jobs just means nothing finishes. A queue
> removes the choice rather than offering a bad one.

> #### ⛔ A DEAD ENTRY PARKS THE POOL FOR EVER, SO IT IS RETIRED AND ANNOUNCED
>
> **Two bands of one faction on one source is the ordinary result of a single command**, not an
> edge: `queue_build_on_working_bands` enqueues on **every** band with workers on that source. When
> one of them finishes the rung, the other's entry declares a job that no longer exists — and
> nothing retired it. `build_workers` is gated on the **declaration** while the arms that consume it
> read the **derived verb**, so the survivor's whole `builders` pool was aimed at a head no arm
> claimed: it banked nothing, `completed` never fired, and `prune_build_queue` only drops entries
> whose *row* is gone. Silently, and for the life of the band. On a patch it was worse than idle —
> the *"nothing left to build"* projection of the **next** rung was consumed by the chain as the
> dead head's own span, so the entry published a finish date that would never arrive and mis-dated
> every entry behind it.
>
> `systems::labor::retire_entries_already_built` runs post-loop **beside the completion pass**, in
> the same shape and the same place, so the next entry becomes the head on the schedule a real
> completion gives it and the chain below dates a queue with no dead entry in it. It pushes the
> completion line's twin on the verb's own channel — *"… is already built — your builders move to
> the next job"*, `status=already_built action=build_retired job=<verb>` — because the player
> staffed those builders and is owed the reason the job left the list.
>
> **The test is "this rung is already achieved", NEVER "the derived verb is `None`"**
> (`forage::patch_rung_already_built` / `fauna::herd_rung_already_built`; a ring's is
> `!Herd::pen_extending`, since a ring has no rung of its own to complete). A verb also derives
> `None` for a source with nothing banked and nothing declared, which is a live entry that has
> simply not started.
>
> **And the predicate is the METER'S OWN FULLNESS, not the retain bar.** It asks exactly what
> `patch_build_verb` / `herd_build_verb` ask — `cultivation_meter_full` / `field_meter_full` /
> `is_domesticated` / `corral_meter_full` — so the two can never disagree about whether there is
> work left. `is_cultivated()` (what `validate_cultivate` asks the *player*) compares against the
> **retain bar**, which sits below the cost, so it answers *already built* for a meter that has
> eroded between the two — a rung the builders are legitimately repairing, whose entry retiring
> would cancel the repair.

> #### THE WIRE STILL SAYS WHAT IS BEING RAISED HERE — derived, not stored
>
> `LaborAssignmentState.improvement` survives, and the sim **resolves it at capture** from that
> band's queue entry: the derived verb (`cultivate`/`sow`/`tame`/`corral`) or the literal
> `"extend_pen"` for a ring, `""` when the band has nothing queued there. So a client still reads
> *what is being raised* off the row and still does **no arithmetic** to get it — the `penFeedUpkeep`
> discipline — while there is exactly one authority for the fact.
>
> **`improvementWorkers` is `(deprecated)` in place**, beside `maintainWorkers`; FlatBuffers field
> ids are positional, so the slots stay and the sim stops writing them. Where the crews are is the
> **rows** above. The place in the line and the finish date are the **source's** own fields —
> `buildQueuePosition` beside `buildTurnsRemaining`.
>
> **`species` is the same class of field** and rides on: the player states a crop on `assign_labor`
> and could not read it back. It is **not** the patch's `ForagePatchState.committedSpecies` — that is
> what the *ground* is committed to and is set only once a crew has worked it, while this is the
> selection the player made, which exists from the moment they make it and rides the rollback record.
> Pinned on the **encoded envelope** (not the capture) by
> `core_sim/tests/source_crews_on_the_wire.rs`, at counts that all differ — rows wired to one source
> would pass a fixture that staffed the same number everywhere.

Each crew's work is `intensification::activity_work(workers)` = `workers × PER_WORKER_OUTPUT`:

```text
upkeep_supplied  = this source's share of the band's keeping POOL (§2.5)
upkeep_shortfall = max(0, upkeep_demand − upkeep_supplied)     // → decay, at the rung's rate
build_work       = (this source is the queue's HEAD) ? builders × PER_WORKER_OUTPUT : 0
                                                                // − the POOL's gear, off the JOB
take             = min(take_workers × per_worker_capacity, source_offer)
```

> #### A SOURCE ROW IS THE BAND'S **HOLDING**, so it survives losing its take crew
>
> `LaborAllocation::set_assignment` used to drop the row outright at `workers == 0`, which made the
> take crew a source's licence to exist — and therefore **re-coupled the take to the keeping**, the
> one separation §2.2 is for. A band that finished a Field and moved its gatherers to a richer patch
> lost the row, so the Field put no demand into the `agriculture` pool, drew no share, and bled its
> **full** rate with keepers standing idle in the role and **no command that could aim them at it**.
> The wire published `upkeepShortfall = demand` faithfully, so the client's under-kept warning fired
> on a state with no remedy. It is the mirror of the arc's own headline: you could neither gather the
> patch **nor** keep it.
>
> **The rule, in one sentence: a source row lasts as long as the band still has something there.**
>
> - **`is_source()` splits the two kinds of row.** A band-wide **role** *is* its head count, so
>   `assign_labor … scout|agriculture 0` still removes it. A **Forage/Hunt** row is the band's
>   holding of that patch or herd, so zero gatherers only unstaffs the take: the row survives with its
>   improvement, its build crew and its kit. **A row is never created at zero** — unassigning ground
>   the band never worked still says nothing.
> - **What "something there" means is the GROUND's answer, not the row's**:
>   `systems::source_has_a_meter_at_risk` — a meter carrying progress
>   (`forage::patch_unwinding_rung` / `fauna::herd_keeping_rung`), which is exactly what the pool
>   funds and what the decay pass bleeds. A wild stand and an unowned herd answer `false`.
> - **Asked at two moments, deliberately the same question at both.** The **command** asks it the
>   instant the take goes to zero, so unstaffing a wild patch clears the row on the spot instead of
>   leaving a `+0.00` row to age out; the **turn** asks it again in each source arm of
>   `advance_labor_allocation`, so a holding whose meter finally rots away is retired without the
>   player touching it. **A QUEUE ENTRY IS A HOLDING TOO**, and is part of both tests: a `Sow`
>   declared on bare ground has no meter yet and may have no gatherers either — the create-from-
>   nothing case the rung exists for — so the ground's answer alone would abandon a build just
>   ordered.
> - **Rows cannot accumulate for the life of a game**: a holding is retired the turn it empties, and
>   the ordinary out-of-range / past-the-leash lapses reach a zero-crew row like any other, because
>   the loop now visits it.
> - **Deriving the demand from OWNERSHIP was the alternative and was rejected.** `ForagePatch::owner`
>   / `Herd::owner` are **factions**, not bands, so ownership cannot say *which* band keeps a patch —
>   every band in range would claim it and the `+=` stamp would double-supply it. The assignment list
>   is the only thing in the sim that answers *"whose"*, which is why the fix is to stop throwing the
>   row away rather than to look somewhere else.
> - **A zero-take row takes nothing, is raised only when the band's builders reach it, and learns
>   NOTHING.** The
>   takes and the wear quanta fall out to zero on their own; the **lesson** does not, because it is
>   credited once per assignment rather than per worker, so `systems::labor` gates all four earn sites
>   on the take crew being present (`credit_managed_rung_lesson` takes it as its `eligible`). Free
>   knowledge from a patch nobody works is the defect that gate exists to prevent.
> - **Unstaffing the gatherers no longer abandons the build beside them.** The row survives, so the
>   `Cultivate` and its entry do too — `assign_labor … 0` is *"stop gathering"*, and `unqueue` /
>   `abandon` are how you walk away from a build. Pinned by
>   `forage_cultivation::a_patch_with_no_gatherers_is_still_kept_by_the_bands_pool` (kept, and the
>   liveness half that it still rots unfunded) and
>   `components::tests::a_role_row_still_goes_at_zero_and_an_unworked_source_is_never_created`.

- **Every row draws on one finite band, and that IS the opportunity cost.**
  `LaborAssignment::staffed_total` is a row's take, `LaborAllocation::assigned_total` sums it over
  every row — **the three standing roles included, because they are rows** — and
  `BandWorkforce::assigned` reports it, so `idleWorkers` nets out builders and keepers like anyone
  else. **"No cap" means no cap on ONE ROLE** (fifty builders may finish a Cultivate in a turn),
  never a licence to exceed the pool.
- **`assign_labor` IS THE ONE ENFORCEMENT, and it CLAMPS.** A role's stepper clamps against the
  band's idle hands exactly as scout's and warrior's do.
  - **RETIRED: `server::crew_is_affordable` / `emit_crew_unaffordable` / `ActivityCrew` /
    `LaborAllocation::idle_for`.** They existed only for the five build verbs' affordability gate,
    and that gate went with the crew it refused: a verb states *what* to raise and never *who* raises
    it, so there is no number left to refuse and no "which of this row's crews am I overwriting"
    question to answer.
- **`LaborAllocation::normalize` answers the other question — the band SHRANK** — and it trims
  **tail-first**, one crew per row, dropping a row that empties. **There is no within-row shedding
  order any more**: the build→take order existed while a row carried two crews, and the building and
  the keeping are rows of their own now, so where each falls in the shedding order is where the
  player put it in the list — which is a statement the player can make and the old rule was not.
- **A queued build survives a re-staffing BY CONSTRUCTION** (`LaborAllocation::set_assignment` does
  not touch the queue at all), where it used to have to carry a field across a row it rebuilds.

#### `yield_fraction_while_building` IS RETIRED, and so is `crew_needed`

- **The dip.** It said *"this crew is preparing ground, not gathering"* — true of a **shared** crew
  and of nothing else. With the allocations separate, what a Cultivate costs is *the people who are
  clearing instead of gathering*, and the gatherers beside them carry exactly what they carried
  before. Four config numbers went with it, along with a term nobody chose (the plant rungs sat at
  `0.25` for years purely because that was the pre-move `cultivating_yield_fraction`).
  `LadderConfig::build_dip`, `BuildDips` and the four `*BuildFraction` wire fields are gone; the wire
  slots stay `(deprecated)`, because FlatBuffers field ids are positional.
- **The cost stopped depending on a regime the player cannot see.** Under the dip a crew big enough
  to saturate the source's standing stock paid *nothing* for its build (the ceiling bound it either
  way) while a thin crew paid the full fraction.
- **`crew_needed`** was a staffing *floor* under the source's published `workers_needed`, needed only
  because that count was inverted out of a **dipped take**. With each activity stating its own crew
  there is no blended count for a floor to raise. `RungDef::build_crew_needed`,
  `LadderConfig::build_crew`, `source_crew_needed`, `NO_CREW`, `NO_BUILD_CREW` and the
  `cultivateCrewNeeded` / `sowCrewNeeded` wire slots all retired with it.
- **`workers_needed` STAYS, per activity.** `SourceYield::workers_needed` is the **take**'s own count
  — hands to haul the offer; `upkeepWorkersNeeded` (`RungDef::upkeep_crew_needed`) is the
  **keeping**'s — hands to meet *this source's* demand, in its own unit. A `max` across units was always the
  compromise a single allocation forced, and it is what made a row read `workersNeeded: 1` beside
  `wastedYield: 0.80`. `herdersNeeded` keeps its own field and no longer folds in.

#### THE FLOOR CAME OFF THE BUILD RATE

`learn_multiplier(floor)` no longer scales `RungDef::build_accrual`; the seam does not take a floor at
all.

- **The shipped rule was written for a shared crew.** *"A crew pulling hard on the source it is
  improving builds slowly"* describes people who are pulling. A build crew is not — and a build crew
  on a source **nobody is harvesting** has no floor to read, so the term would have to be invented
  from a default nobody chose.
- **`learn_multiplier` keeps scaling knowledge accrual**, where *how much you leave standing shapes
  what you learn* still holds. Its name stops lying.
- **`MANAGED_SOURCE_FLOOR` and `MANAGED_SOURCE_IS_TENDED` are gone.** They existed because rung-3
  builds had no real floor to pass; the one thing they still meant — a managed rung's *lesson* runs at
  the food peak and its keeper is always working — is stated once by
  `systems::labor::credit_managed_rung_lesson`.
- **Upkeep never reads the floor either.** It is charged against raw worker-turns: a route has no
  escapement floor at all, and an upkeep that read one could not be applied to this arc's own target.
- **PACING IS UNCHANGED at the default floor**, because `learn_multiplier` is exactly `×1.0` at the
  food peak — the floor a fresh assignment carries. Only sub-peak floors build faster.
  `taking_the_floor_off_the_build_rate_is_pacing_neutral_at_the_food_peak` is the proof, asserted
  against the retired arithmetic rather than a remembered number.
- **The grace bound loosened with it.** `grace_turns < work_cost / reference_output` lost its
  `crew_needed` divisor, so the reference is [`SOLE_BUILDER`] — the longest the build can take, and
  therefore the loosest the bound can be, which is the safe direction for a guard. Every shipped rung
  still clears it by an order of magnitude (`the_shipped_graces_clear_the_loosened_bound`).

#### Completion RETIRES THE QUEUE ENTRY — and frees nobody, because nobody was standing there

The turn a meter fills, the post-loop pass in `advance_labor_allocation` removes that source's entry
from `LaborAllocation::build_queue`, which hands the **whole pool** to whatever the player put next.
The **row is untouched** — the take crew, the tile, its committed species and the stance all stay as
they are. It is **announced** on the finished verb's own feed channel (`improvement_feed_channel`), so
a rung's whole life reads on one line and the player sees the head move.

**Two hand-offs are retired here, not one.**

- **The carry-over onto the web's keeping role is gone** (`docs/plan_standing_upkeep.md` §2.3), and
  with it `RungKey::upkeep_role` and `LaborAllocation::add_role_workers`. It existed so that a
  brand-new pen did not start decaying on turn one because nobody had noticed it had begun costing
  something — **and §4.6a makes that unreachable**: the keeping bill starts at the **first work
  banked**, not at completion, so a player who built the thing at all was already paying to hold it.
- **And there is no crew to free** (§2.5). The builders are a band-level pool that never stood on the
  source, so completion moves nobody anywhere; what it moves is the **head of the queue**.

**`RungDef::declares_upkeep` survives** as the predicate the keeping seams read — it is no longer a
fork at completion, and there is no branch left for it to choose between.

### Standing upkeep — what it costs to HOLD a rung

**Every cost on the ladder used to be a *job* — a fixed pile of work you finish once. `upkeep` is the
first *rate*** (`docs/plan_standing_upkeep.md`).

> #### THREE DIALS, NOT ONE — the decay is DECOUPLED from the demand
>
> `upkeep` answers **three different questions**, and welding any two of them together is what made
> the term unretunable:
>
> | | question | seam |
> |---|---|---|
> | **demand** | how much work per turn does *holding* this want | `RungUpkeep::work_per_turn` → `RungDef::upkeep_demand` |
> | **decay rate** | once rotting, *how fast* | `RungMeterDecay::per_turn` → `RungDef::upkeep_decay` |
> | **grace** | how long under-supplied before rot begins | `RungUpkeep::grace_turns` |
>
> ```text
> rot = (shortfall / demand) × decay_per_turn, past the grace   // the KEEPING came up short
> net = build_work − rot
>   net > 0  →  the meter CLIMBS
>   net = 0  →  the meter HOLDS exactly where it is
>   net < 0  →  it LOSES GROUND — work already bought, bleeding
> ```
>
> **THE RATE IS OWED ALWAYS, AND ALWAYS BY THE SAME POOL** (`docs/plan_standing_upkeep.md` §4.6a).
> The band's keeping owes it for **every meter carrying work, at any fullness** — from the first work
> banked until the last — and a **build crew supplies nothing toward it**: its whole output is
> progress, so the pace is `work_cost / crew` again.
>
> **The meter's FULLNESS used to be the who-pays test** (`forage::patch_is_maintaining` /
> `fauna::herd_is_maintaining`, both deleted), and §2.4 carries the two autopsies. A **half-built**
> meter whose builders left could not be held at all — it was billed to a crew that was not there and
> bled its full rate with keepers idle in the role and no command that could aim them at it; and a
> **held** rung eroding to 99% flipped into *building*, so it stopped being the pool's business at the
> moment it started needing it, and would have displaced the player's real build off the head of the
> next slice's queue. There is no third concept either: an earlier cut gave an unfinished meter its
> own demand (`meter_raising_demand`), the same rate under a second name.
>
> **`shortfall` used to BE the decay**, so raising a demand made the improvement rot faster in exact
> proportion and neither number could move. Splitting them is what let the plant demands become whole
> numbers a player can staff exactly (`2` and `4`) while the rot rates stayed precisely where they
> were (`0.5` and `0.75`) — provably pacing-neutral on the decay axis
> (`forage::tests::the_plant_rot_rates_are_exactly_what_the_retired_decay_fraction_bled`).
>
> **`meter_decay` is NULL on both animal rungs, and that is not an omission**: an under-kept flock
> **sheds animals** at `fauna_config`'s own `pen_escape_fraction` / `pastoral_escape_fraction`, which
> are already the rate. A second one on the rung would be two numbers for one mechanic, free to
> disagree. Only the shortfall **fraction** is shared — `intensification::upkeep_shortfall_fraction`,
> stated once, applied at each web's own rate.

> #### THE COUNTDOWN HAS FIVE ANSWERS, AND FOUR OF THEM ARE NEGATIVE
>
> `intensification::BuildTurns` is what a source stores;
> `build_turns_estimate(cost, done, balance, gate_holds, builders)` fills the first four, and the
> band's **chain pass** (`systems::labor::publish_build_chain`) mints the fifth:
>
> | stored | wire | meaning |
> |---|---|---|
> | `Some(Turns(n))` | `n` | a real finish date — **chained** behind everything above it in the band's queue |
> | `Some(Holding)` | `BUILD_METER_HOLDS` (**`-2`**) | **the meter holds exactly where it is** — `build_work − rot` is exactly `0` |
> | `Some(Rotting)` | `BUILD_METER_ROTS` (**`-3`**) | **the meter is going backwards** — that balance is negative |
> | `Some(Blocked)` | `BUILD_QUEUE_BLOCKED` (**`-4`**) | **the queue is stuck here** — the pool is staffed and standing on this entry, and its own gate refuses it |
> | `None` | `NO_BUILD_TURNS_ESTIMATE` (**`-1`**) | there is genuinely no answer |
>
> **All four negatives are ANSWERS or the absence of one, never a mix**, and the three the player can
> act on are the middle group. Folded into `-1` they rendered as **no line at all** on the tile card
> and the herd drawer, visible only to a compose sheet that happened to redo the comparison itself.
>
> > #### ⛔ `-4` IS NOT `-1`, AND THE DIFFERENCE IS WHAT THE PLAYER IS BEING TOLD
> >
> > `-1` is the **absence** of an answer and renders as no line — which is exactly the silence a
> > blocked queue must not be read as. The player has committed a pool, the pool is producing
> > nothing, and everything behind the head is waiting on a build that cannot advance.
> >
> > **THE REMEDY IS OFF THE BUILD LINE ENTIRELY.** The measured case is the animal web's own:
> > a half-tamed herd with an empty `husbandry` role advances three turns and freezes, because the
> > hunters draw the flock to their floor, the unmet keeping suppresses its regrowth, and the
> > `Tame`'s escapement gate never reopens. **Adding builders does nothing.** A surface showing `-4`
> > must pair it with that source's own `upkeepShortfall` / `neglectGraceRemaining`, because the
> > sentence is *staff the keeping* — `assign_labor <f> <b> husbandry <n>`.
> >
> > **ONLY THE HEAD, WITH A STAFFED POOL, IS `-4`.** A *waiting* entry whose gate refuses may well be
> > eligible by the time it reaches the head, so it publishes the honest `-1` and stops the chain
> > there: we cannot date what is behind an unanswerable entry. A blocked head, by contrast,
> > propagates — every entry below it publishes `-4` too, because nothing below a head that never
> > finishes finishes either.
>
> > #### THE CHAIN — a waiting entry gets a real finish date, not a "queued" badge
> >
> > The queue is deterministic (the head takes the whole pool until it fills, then the next one
> > does), so an entry's turns are **the sum of everything above it plus its own span at the full
> > pool**. Under §4.6a a waiting entry's meter is held by the *keeping*, not by the builders, so that
> > chained number is **exact** rather than an estimate that drifts — and the bad news propagates for
> > free, because the first entry that cannot name a number is what every entry below it publishes.
> >
> > **A source with NO entry is quoted at the BACK of the line** (`cumulative + its own span`), which
> > is where a newly queued build would actually go. Quoting it as though it went to the head would
> > over-promise the compose sheet by the whole queue.
> >
> > **`buildQueuePosition` rides the same winner** as `buildTurnsRemaining` and `buildWorkFromGear`,
> > `-1` when no band has queued the source. Without it a chained date is an exact number with no
> > explanation: the player cannot tell forty turns of work from eight turns of work behind four
> > other jobs.
> >
> > **`EstimateStanding::Blocked` sits between `Rots` and `Silent`**, which continues the one stated
> > rule — *more net supply is better news* — rather than starting a second: a band rotting a meter is
> > at least supplying something to it, a blocked queue supplies nothing at all, and silence is the
> > absence of an answer.
>
> > #### ⛔ EVERY ENTRY KIND RECORDS A `BuildQuote`, THE PEN RING INCLUDED
> >
> > `publish_build_chain` mints `-4` for a head it has **no quote for**, so an entry kind that
> > forgets to record one does not merely go undated: it tells the player its builders are standing
> > on a refused gate, and `carried` then hands that same `-4` to **every other source the band
> > works**, every turn, while the entry accrues perfectly normally.
> >
> > The pen ring was that kind. It is resolved in the Hunt arm's corralled-herd **tend branch**,
> > which `continue`s before every one of the four rung arms and before the *"nothing left to
> > build"* projection, so none of the five `build_quotes.push` sites was on its path. The tend
> > branch now records the ring's own terms — the pen rung's bar at the pool's gear, the ring meter,
> > the balance at the **full pool**, and `Herd::pen_extending` as its `gate_holds` — and a ring at
> > the head publishes an ordinary countdown like any other build.
> >
> > **The quote reads the bar the ring just cleared on the turn it completes**, not the live meter:
> > `accrue_pen_extension` resets `pen_extend_progress` to `RUNG_UNSTARTED` at completion, so quoting
> > the field there would publish the span of a whole *new* ring on the very turn the old one
> > finished.
> >
> > **The alternative — exempting `ExtendPen` from the `Blocked` mint — was rejected**: it trades a
> > wrong answer for no answer, and the player loses a date the model can state. `BUILD_QUEUE_BLOCKED`
> > keeps meaning exactly *the builders are standing on this entry and its own gate refuses it*.
>
> **AND THE TWO NON-FINISHING STATES COST THE PLAYER DIFFERENTLY, so they are two values.** At the
> rot exactly the meter stands still — which is **not always a failure**: with no builders and the
> keeping met it is a player **parking** a half-built improvement, held indefinitely at no risk,
> which §2.4 exists to make possible. Below it the decay pass takes back work the player already
> bought, and *that* is unambiguously bad. One `-2` for both said *"never finishes"* about a build
> treading water and one losing ground alike — and a client rendering them yellow and red cannot
> derive the difference, because the difference is a **sign**.
>
> **The split is a SIGN, so the estimate reads the unfloored net.** `RungDef::build_accrual` is
> the crew's own output — a meter may only ever be *added* to, since the bleed is the decay pass's
> off the same meter. `RungDef::build_balance` is that output **less the rot**
> (`RungDef::meter_rot`), off the same `RungDef::build_supply` gate, and it is what every countdown
> site passes. The meter takes the accrual, the countdown takes the balance; one gate, two readings.
>
> **The rot does not vary with the build crew**, which is what lets a compose sheet re-price a
> *proposed* crew against it and land on the sim's own answer for the committed one — and it is
> published as `meterRotPerTurn` because the client can derive neither the grace state nor the rung's
> decay rate.
>
> > #### ⛔ THE ROT IS WHAT THE **NEXT** PASS WILL BLEED — and reading it backwards withheld a fact
> >
> > Logistics runs before Population, so within one turn `T`:
> >
> > ```text
> > Logistics(T):   bleeds  decay(fraction(supplied(T−1)), neglect(T−1) + 1)   ← LAST turn's supply
> > Population(T):  stamps  supplied(T);  publishes  decay(fraction(supplied(T)), neglect(T) + 1)
> > ```
> >
> > Two lines, **one expression, one turn apart** — so what is published at `T` is exactly what
> > `Logistics(T+1)` bleeds. Measured on `plant:tended` (grace `2`), on a half-built meter with nobody
> > on it, the published rot at each turn against the movement of the **next**:
> >
> > | turn | published rot | `neglectGraceRemaining` | `buildTurnsRemaining` | meter moved that turn | meter moved NEXT turn |
> > |---|---|---|---|---|---|
> > | 1 | `0.00` | 2 | `-2` | `+0.00` | `+0.00` |
> > | 2 | `0.50` | 1 | `-3` | `+0.00` | `−0.50` |
> > | 3 | `0.50` | 0 | `-3` | `−0.50` | `−0.50` |
> > | 4+ | `0.50` | 0 | `-3` | `−0.50` | `−0.50` |
> >
> > **THE BLEED IS ALREADY DETERMINED WHEN IT IS PUBLISHED, and that is the non-obvious part a future
> > reader will re-derive incorrectly.** The next pass judges the supply *this* turn has just stamped,
> > so a shortfall standing at `T` cannot be undone by anything the player does at `T+1`. The backward
> > reading therefore **withheld a fact** rather than declining to predict one — it published `0.00`
> > and `-2` on turn 2, *the meter holds*, about a meter that was already going to lose `0.50`. It
> > looks like the cautious reading; it is the wrong one.
> >
> > **It cannot over-warn**: a positive rot needs a shortfall in the just-stamped supply, which is
> > exactly the condition the next pass tests. Restore the keeping and both go to `0` on the same turn.
> >
> > **What it gives up, deliberately**: it is not *"what the meter just did"*. On a turn the keeping is
> > **restored** the meter still loses the previous turn's shortfall while this reads `0` — correct,
> > because that loss is already spent and the next pass takes nothing. A surface wanting the turn's
> > realised cost reads the meter, not this.
> >
> > Pinned on the encoded snapshot by
> > `build_turns_on_the_wire.rs::the_published_rot_is_exactly_what_the_next_decay_pass_bleeds`, whose
> > **rescue arm** is what makes the other two mean anything: a form that always predicted a bleed
> > would pass the boundary and the steady state alike.
>
> #### ⛔ THE NO-ANSWER BOUNDARY IS **WORK BANKED**, NOT HANDS ON THE JOB
>
> | state | answer |
> |---|---|
> | no build in flight, or the rung's own gate refuses it | `None` |
> | a build in flight, meter at **zero**, no builders | `None` — nobody has promised anything yet |
> | otherwise | the sign of `build_work − rot` |
>
> **A meter carrying work has promised something — the player paid for it.** The rule used to be
> *"an **unstaffed** source reads `None`, because nobody has promised anything"*, which was written
> when unstaffed meant *nobody has declared anything*. Since §4.6a a half-built meter with nobody on
> it is exactly *the meter holds* (the keeping covers it) or *the meter is losing ground* (it does
> not) — which is what the two sentinels mean.
>
> **That is not a nicety: on the shipped ladder ZERO BUILDERS IS WHERE THE TWO STATES LIVE.** Both
> plant rot rates (`0.5`, `0.75`) are **below one worker-turn**, so a *staffed* plant build always
> out-runs its own rot; and neither animal rung declares a `meter_decay` at all, so nothing eats an
> animal build. Under the old boundary the pair was unreachable in ordinary play, and the only way to
> exercise them was a fixture rung with an invented `meter_decay` — a fixture bent until the
> assertion passed. **A refused GATE still reads `None`**, at any staffing and any meter: a build
> that is not running has promised nothing. The **projection** answers on the same rule.
>
> Pinned on the encoded snapshot, four states pairwise distinct, by
> `core_sim/tests/build_turns_on_the_wire.rs`, **on the shipped ladder**: a half-built `plant:tended`
> meter with no builders and the `agriculture` role staffed → `-2`, the same meter with the role
> empty and the grace spent → `-3`, and a refused gate → `-1`. **The exact-equality arm is the one
> that carries the test**: rotting is reached by a `< 0` comparison and holding by falling through
> it, so a suite staged only *below* the line passes with both wired to the same branch.

- **THE MAINTENANCE RATE IS NOT A TAX ON BUILDING — `work_cost / crew` IS the pace** (§4.6a). A build
  crew supplies nothing toward the rate, so `RungDef::build_accrual` is `activity_work(workers)` and
  a lone builder banks a whole worker-turn on the dearest rung on the ladder. **There is no
  minimum-viable-crew threshold**; the only no-estimate state a staffed build can reach is the
  countdown's, and its term is the **rot**. `LadderConfig::projected_build_turns` nets the same rot,
  and on ground nobody has started there is nothing banked and therefore nothing to rot — which is how
  issue #545's repro (one builder against `plant:tended`'s demand of `2.0`, quoted `-3`) resolves to
  an honest 50 turns. **`<rung>UpkeepDemand` still answers what holding the quoted rung will cost the
  keeping pool** — a real cost the player must see before committing — it is simply not netted off the
  build; see "A price without the rate that eats it is not a quote".
- **ALL FOUR MANAGED RUNGS DECLARE ONE.** `upkeep_demand` is an honest `0` on the two `wild` rungs
  rather than a sentinel — `HerdTelemetryState::pen_upkeep`'s rule.
- **THE DECAY IS PROPORTIONAL, continuously** (§2.4). Meet the demand and the net is zero and the
  improvement holds; fall short and the meter loses `shortfall_fraction × the rung's own rate`, past
  the upkeep's own `grace_turns`. Half the hands a meter needs means it slides at half rate — not at
  the full neglect rate and not at nothing, which the binary `tended_this_turn` /
  `tamed_this_turn` flags could not express.
  - **THE HANDS ARE ALWAYS THE BAND'S KEEPING POOL** (`forage::patch_upkeep_supply` /
    `fauna::herd_upkeep_supply`, one rule and two seams of the same shape), from the **first work
    banked** until the last. An **abandoned** part-build still owes, and can now still be *held*.
    **The verb names the meter** on both webs, so a `Sow` or a `Corral` answers for the rung it is
    starting from its first turn — the supply is stamped in Population and read by the next Logistics
    pass, so it has to describe the meter that pass will judge. (Since the fullness test went, the
    resolved meter's *identity* no longer changes what is supplied; what it still decides is that
    ground with **nothing** on it is billed nothing.)
  - **NEITHER WHAT IS OWED NOR WHO PAYS IT MOVES.** The rate is the same on both sides of completion
    and on both webs, and so is the payer. *"You cannot be billed to hold something you have not
    finished building"* is **deleted** — you can, and that is what makes a half-built meter holdable
    at all. A tended patch eroded to 99% is short of its cost, still tended, and still the pool's:
    three facts, none of them each other.
  - **THE PENALTY DIFFERS BY WEB, and only the penalty does.** A plant meter **bleeds** the shortfall
    (`forage::advance_cultivation`); an animal flock **sheds** the animals the missing hands cannot
    hold (`fauna::advance_husbandry`, `uncontained_overage` = `shortfall_in_loads ×
    animals_per_herder`). One trigger, one grace, two currencies. **`ForagePatch::tended_this_turn`,
    `Herd::herded_fraction` and `RungBuild::decay_fraction_per_turn` are all RETIRED** with the two
    webs' moves onto the term (`cultivation.md` → "SHORTFALL IS THE DECAY"; `husbandry.md` → "THE
    KEEPER DEMAND IS AN UPKEEP RATE"). `Herd::corralled_tended_this_turn` survives, because it gates
    the pen's **feed** rather than its keeping — a separate account with a separate currency.
- **The scale term is the generic piece** (§2.6). `UpkeepScale::Flat` states the rate — the cost of
  the thing *existing*, which is what a patch owes because a patch is one tile.
  `UpkeepScale::SourceLoad` multiplies it by the source's own **load** reading in whatever unit the
  rung quotes its rate in; the animal rungs quote **per keeper-load** (`fauna::herd_keeper_loads` =
  `head count / animals_per_herder`), which is what lets one rate say *a shepherd minds 300 sheep and
  a cowherd 80 cattle*. It is deliberately not a per-**head** rate: that says *"one keeper per 100
  fowl but one per 2 boar"* and invents a 45-herder steppe megaherd that is a pure artifact of the
  unit — the measurement error `animals_per_herder` exists to prevent, one level up. Adding a
  primitive is coding one thing once, after which using it is a config edit.

#### A RUNG IS NOT LOST THE INSTANT ITS METER DIPS

**A completed meter sits *exactly* at its own cost**, so a `progress >= cost` predicate made the very
first bleed of any size revoke the rung: finish a Cultivate and the patch could be out of *tended*
before its keepers were assigned. No grace and no rate could fix that, because the loss was a
**threshold test rather than a rate** — which is the bug this half of the arc was filed against.

- **The rung's ACHIEVED state and the meter's FULLNESS are two facts now**, and the seam that
  separates them is the predicate itself: `ForagePatch::is_cultivated` / `is_field` compare against a
  **stamped retention bar** rather than against the stamped cost. That is what let the loss point
  move without touching the ~hundred call sites that ask *"is this patch tended"*.
- **The bar is `retain_fraction × cost`, stamped at COMPLETION** (`RungDef::retention_bar`, both
  shipped plant rungs at **0.75**) — a fraction, so it survives a cost retune, and stamped, so the
  predicate needs no config in scope. It doubles as the *achieved* marker: `RUNG_UNSTARTED` means the
  rung was never earned, so the predicate needs no `cost > 0` guard of its own.
- **The rung is still EARNED at `progress >= cost`.** Only losing it moved. Crossing back below the
  bar **clears** the bar, which is what makes the loss stick: the patch has to be re-earned at the
  full cost from wherever its meter landed.
- **What that buys, measured on the shipped ladder**: a completed **tended patch** survives **28**
  wholly unmaintained turns and a **Field 27**, against `grace + 1` — three and two — before. Re-earning
  the rung then costs only the `12.5` / `18.75` work that rotted. Pinned by
  `forage_cultivation::a_completed_tended_patch_survives_many_unmaintained_turns_before_it_is_lost`,
  which asserts the eventual loss too: *"it never reverts"* is the other way to break this.
- **The animal rungs need no bar**, and their absence is the same fact from the other side:
  `domestication_progress` is monotone-up (the neglect-escape arc retired its bleed) and a pen is
  held by `corralled_at`, a stored flag rather than a meter — so no animal rung can be lost by a
  meter dipping at all.
- **A rung's BENEFIT stays binary on the achieved state.** A half-eroded tended patch pays exactly
  what a full one pays. Scaling a rung's payout with its meter is a real proposal and a much larger
  one; it is deliberately not this.
- **AND THE BAR IS ORTHOGONAL TO THE BUILDING/MAINTAINING STATE TEST.** *Building vs maintaining* is
  the meter's fullness and decides **who pays the rate**; *is the rung still achieved* is the bar and
  decides **what the ground pays out**. A patch at 99% is building and still tended. Folding them
  would make a rung's loss and a rung's repair the same edge — which is why the accrual guards and the
  "nothing left to build" test all read fullness (`ForagePatch::cultivation_meter_full` /
  `field_meter_full`, `Herd::corral_meter_full`), and the hundred *is this ground tended* call sites
  all read the bar.

#### THE BUILDERS ARE A BAND-LEVEL POOL TOO, and the QUEUE is what they work

**One role for both webs** — `LaborTarget::Builders`, staffed through
`assign_labor <faction> <band> builders <n>` like every other standing role and published as a
`laborAssignments` row of `kind = "builders"` (§2.5). It is one pool rather than two because **a
build is a job where a keeping is a standing charge**, and the queue already says which web is being
worked; a second axis would ask the player to state the same thing twice.

- **THE WHOLE POOL GOES ON THE HEAD.** `systems::labor` resolves `builders` and `pool_gear` once per
  band before the assignment loop, and each source arm takes `build_work = pool_work` **iff** this
  source is `build_queue[0]`'s source and the arm's rung is that entry's resolved job — otherwise
  `0`. The pace is `work_cost / builders`, and **the only term that eats it is the ROT** (§4.6a):
  a build supplies nothing toward the keeping rate, so nothing re-nets it.
- **⛔ A BLOCKED HEAD STAYS AT THE HEAD.** It is not skipped, not reordered and not passed over —
  it publishes `-4` and says loudly that it is stuck. Passing over it would quietly fund something
  the player did not put first, and would hide the one state whose remedy is off the build line
  entirely.
- **THE GEAR IS THE POOL'S OWN, read off the `builders` row like every other role's tier.** One
  `KitCoverage`, resolved over the pool, at the pool's count. It used to ride the *source row's* kit
  — a `Corral` priced off whatever the hunt row was carrying — because the builders stood on the
  tile.
  - **⛔ THERE ARE TWO BUILDERS KITS, ONE PER WEB, AND THE POOL DERIVES ITS OWN PER QUEUE ENTRY.**
    `tillage` carries the hoes and `hurdling` the hurdles; a `build_work` effect names the branch it
    serves, so a hoe takes nothing off a `Tame` and hurdles take nothing off a Cultivate. A queue
    item is one job, so the kit is resolved from **the entry being worked** — the head's branch is
    what is actually funded, and everything below it is dated at the gear it will be raised with.
    A kit **named** on the `builders` row overrides that, `none` included. `equipment.md` → "THE
    BUILDERS' KIT IS DERIVED PER QUEUE ENTRY" owns the seam; the guard is
    `equipment_config::tests::every_branch_of_the_ladder_has_a_builders_kit_that_serves_only_it`,
    which replaced *"every kit that supplies `build_work` offers the `builders` job"* — no longer the
    invariant, since `husbandry` keeps its hurdles for the **hunt** (a pen is collected on
    `pen_carry`) and gave the building up.
  - **`default_kits.builders` is `none`, as a FALL-BACK rather than the answer.** It is what the pool
    resolves when the row named nothing and either the queue is empty or no roster entry serves that
    web. **The shipped opening moves here**: a start-stocked band's builders are geared from turn one
    on both webs, where before this slice every build was bare-handed unless the player named a kit.
- **THE QUEUE'S THREE INPUTS**, all on one source grammar — **two integer tokens name a tile, one
  token names a herd id**, the tile form parsed first:

  | verb | proto | drops | leaves alone |
  |---|---|---|---|
  | `abandon <faction> <source…>` | 57 | the band's **holding**: the row *and* its entry, on every band working it | the **meters** — the ground rots back down at the rung's own rate, so nothing is destroyed on the spot and nothing is confirmed |
  | `unqueue <faction> <source…>` | 58 | the queue entry only | the row, its take crew, its kit and the meter |
  | `build_order <faction> <band> <source…> <position>` | 59 | — | moves that band's entry to a 0-based `position`, **clamped** to the queue's length |

  **`abandon` is one bit per source, never a number** — disposal rather than a smaller share, so the
  per-source *funding* lever stays deleted. It exists because §2.4 bills a meter from the first work
  banked, so a half-built patch the player has lost interest in otherwise draws keepers forever.
  **Proto 46 stays reserved** for the retired `abandon_improvement`, which was a different verb.

  **`build_order` is the queue's defining input.** With the whole pool on the head, the order *is*
  the funding decision — re-ordering is the one input a list can carry that a stepper cannot, and
  without it *all hands on the head* cannot be steered at all.

#### MAINTENANCE IS A BAND-LEVEL POOL, not a crew on the tile

**Two standing roles, one per web** — `LaborTarget::Agriculture` keeps every tended patch and Field
the band works, `LaborTarget::Husbandry` every pastoral herd and pen. Staffed through
`assign_labor <faction> <band> agriculture|husbandry <workers>` like Scout and Warrior, published as
ordinary rows of `laborAssignments` with those `kind`s, shed by `normalize` and checkpointed like any
other row (§2.5).

- **WHY IT LEFT THE TILE: an indivisible supplier meeting a per-source demand WASTES what it does not
  spend.** A patch asking for `2.0` work staffed by three hands throws one away, once per source, and
  the waste grows as gear makes a hand worth more. **A pool has no leftover by construction** — every
  unit either meets a demand or is still in the pool
  (`intensification::tests::a_short_pool_is_spent_whole_under_both_modes`).
- **One role per WEB because the two webs are already separate ladders** — this is their existing
  split, not a new axis. (`RungKey::upkeep_role`, which read it off `RungKey::branch`, retired with
  the completion hand-off that was its only caller.)
- **The band's demand is the SUM** over everything it holds on that web, and **every meter carrying
  work draws, at any fullness** (`systems::labor::maintenance_shares`, §4.6a). A source claims a share
  through **`forage::patch_keeping_meter(patch, improvement)`** / **`fauna::herd_keeping_meter`** — one
  function, three callers each: the claim gate, `*_upkeep_demand` and `*_upkeep_supply`.

  > **⛔ IT USED TO BE `source_has_a_meter_at_risk`, AND THAT WAS A SECOND DEFINITION THAT DRIFTED.**
  > That seam is **progress-only**; the payment side (`patch_upkeep_supply` → `patch_keeping_meter`) is
  > **progress-OR-verb**. Reported from play: a band 6% into a Cultivate with Agriculture staffed read
  > `Short 2 of the 2 work` — i.e. **supplied 0.0 on a staffed role**.
  >
  > **A within-turn ordering fault.** `maintenance_shares` runs *before* the assignment loop banks the
  > meter, so on the turn a build banks its FIRST work the patch still reads `progress == 0`, is skipped,
  > and gets a share of zero — which the payment side then dutifully pays, having correctly worked out
  > that the pool owes for this meter from turn one. Capture reads the post-accrual patch: demand 2.0,
  > supplied 0.0.
  >
  > **The animal web had it too, with a different cause**: `owner` is recorded on the first accrual,
  > which happens after the shares are split, so a herd mid-Tame read as wild and claimed nothing.
  >
  > **`patch_unwinding_rung` / `herd_keeping_rung` are now that function asked with NO VERB**
  > (`NOTHING_IN_FLIGHT`), not a parallel spelling — so the decay pass, the snapshot and the wire
  > countdowns are byte-identical while the claim side gained the verb term. `source_has_a_meter_at_risk`
  > survives as the **row-survival** seam alone, and its doc says so.

  **THE VERB TERM IS NARROWED TO THE FUNDED HEAD, and that is deliberate.** Taking it straight from the
  queue entry — the literal "same input as the payment side" — makes *every waiting entry* claim its full
  demand, and `Spread` funds proportionally, so two queued-but-unfunded builds would dilute the share of
  the Field the band actually holds. That is a new way to starve a real holding. `source_banking_its_first_work`
  restricts it to `build_queue[0]` when `builders > 0`, mirroring the assignment loop's own `build_workers`
  rule. **Claim-side verb ⊆ payment-side verb**, so the two cannot disagree in the direction that caused
  the bug.
  > **Accepted cost:** a **blocked** head bills the keeping pool while its meter never advances. It
  > self-announces as `BUILD_QUEUE_BLOCKED` on the wire and the player has explicitly staffed builders
  > at it.

  So what the pool funds, what the decay pass bleeds and what keeps a holding alive cannot come to be
  three questions.
  - **HOLDS, not harvests.** A row's eligibility is the *ground's* answer and never its take crew.
    `maintenance_shares` used to skip rows at `workers == 0`, which made a finished improvement's
    keeping depend on somebody still gathering it — see "A SOURCE ROW IS THE BAND'S HOLDING" above.
- **THE SHORTFALL SPLIT IS A PER-BAND PLAYER OPTION** — `LaborAllocation::upkeep_fund_mode`
  (`intensification::UpkeepFundMode`), set by `upkeep_mode <faction> <band> spread|priority` (proto
  field **56**, `UpkeepModeCommand`, reusing the retired `MaintainCommand`'s slot):
  - **`spread`** — proportional to demand, so everything degrades a little. The **default**, because
    it is what an unstated policy means: nobody is singled out.
  - **`priority`** — fund sources completely until the pool runs out, **most-invested first**, so the
    biggest investments stay whole and the marginal ones rot.
- **The ORDER IS TOTAL, because a checkpoint has to reproduce it.** `distribute_upkeep_pool` funds in
  **slice order** and the *caller* sorts — most-invested first on the at-risk meter's **stored cost**
  (`forage::patch_at_risk_cost` / `fauna::herd_at_risk_cost`), tie-broken on a stable per-source key
  (a tile's coordinates, a herd's id). The ladder owns the arithmetic and the web owns the ranking,
  because *"most invested"* is a per-web reading. **The stored cost rather than the live progress**:
  a meter eroding under a shortfall would otherwise slide *down* the priority order exactly as it
  started to need the hands.
- **It rides the checkpoint** on the band's `LaborAllocation`, which `capture_sim_state` clones whole
  — asserted rather than assumed by
  `forage_cultivation::the_maintenance_split_survives_a_checkpoint_under_both_modes`.
- **RETIRED with it**: `LaborAssignment::maintain_workers`, `ActivityCrew::Maintain`,
  `LaborAllocation::set_maintain_workers`, the `maintain` command and `MaintainCommand`. The wire slot
  `maintainWorkers` is `(deprecated)` in place — FlatBuffers field ids are positional.
- **AND THE BUILD CREW FOLLOWED IT OFF THE TILE ONE SLICE LATER**, taking
  `LaborAssignment::improvement` / `improvement_workers`, `ActivityCrew`, `LaborAllocation::idle_for`,
  `set_improvement` / `set_build_workers`, `server::crew_is_affordable` /
  `emit_crew_unaffordable`, `systems::labor::quoted_build_crew` and the five verbs' trailing
  `<workers>` argument (proto fields `reserved`) with it. `improvementWorkers` is `(deprecated)` in
  place beside `maintainWorkers`, for the same positional reason.

#### The standing upkeep on the wire

`ForagePatchState` / `HerdTelemetryState` each carry **`upkeepDemand`**, **`upkeepSupplied`**,
**`upkeepShortfall`** and **`upkeepWorkersNeeded`**. The first three ship rather than two, per the
`penFeedUpkeep` discipline — the sim answers and the client does zero arithmetic.

- **`upkeepDemand` follows `penUpkeep`'s rule: always meaningful, never a sentinel.** A rung with no
  upkeep publishes an honest `0`.
- **IT ANSWERS FOR THE RUNG THE SOURCE IS ON, WHICH IS WHY A QUOTE CANNOT READ IT** — see "A price
  without the rate that eats it is not a quote" below, the pair that closes that gap.
- **`upkeepWorkersNeeded` is the MAINTAIN activity's own `workers_needed`** —
  `ceil(demand / PER_WORKER_OUTPUT)`, in keepers — beside the TAKE activity's
  (`SourceYield::workersNeeded`, in haulers). Two counts in two units, because a `max` across units
  was the compromise a single allocation forced.
- **THE PER-SOURCE QUARTET SURVIVED THE MOVE, and it answers a better question.** `upkeepSupplied` is
  now that source's **share of the band's pool**, so the trio stopped answering *"did you staff this
  one"* and started answering *"where is my pooled shortfall landing"*.
- **There is no `maintain` flag and no per-source keeper crew on the wire.** The band's own keeping is
  a row of `laborAssignments` (`kind` `"agriculture"` / `"husbandry"`), and how it splits when short
  is `PopulationCohortState.upkeepFundMode` — the same token `upkeep_mode` takes.
- **`upkeepSupplied` / `upkeepShortfall` are transient per-turn scratch on the source**, stamped once
  per worked source by `advance_labor_allocation` (before the arm branches by rung, so a Field's early
  return cannot skip it) and cleared by the Logistics decay pass — exactly `buildTurnsRemaining`'s
  cycle, and for its reason: they describe *this* turn's keepers.
- **The four `*BuildFraction` slots and the two `*CrewNeeded` slots are `(deprecated)`** and no
  longer written; the client's native reader stops inserting their dict keys and inserts the upkeep
  quartet instead. The GDScript that reads the retired keys is a separate pass.

### The knowledge pattern — practise rung N, unlock rung N+1

**The one rule** (`docs/plan_intensification_ladder.md` §4, slice 4): **working a source teaches the
knowledge its *current rung* declares in `earns_knowledge`.** "Practising rung N" means *working a
source that stands on rung N* — **not** *"using rung N's verb"*. So the **same hunt** teaches
**Herding** on a wild herd and **Penning** on a tamed one: *you learn herding by managing wild herds,
penning by managing tamed ones*.

**`RungDef::knowledge_accrual(floor, eligible, knowledge)` is THE earn seam** — the twin of
`build_accrual`: the rung names the lesson **and how much of it**, the caller credits the ledger. It
replaced the two hard-coded per-web `Sustain && Thriving → <ID>` branches, so `earns_knowledge` went
from declarative (slice 2) to live, for **every** rung including the wild ones. Callers resolve the
rung via `fauna::herd_rung` / `forage::patch_rung`, read once per source in
`advance_labor_allocation`'s Hunt/Forage arms and credited through the one
`systems::labor::credit_rung_lesson` helper **inside each rung branch** — the amount is only knowable
once the source's own branch is reached — a Field and a pen answer `eligible` differently from a wild
stand — so the single pre-branch call the slice-4 shape used could not survive.

#### A LESSON COSTS PRACTICE — and practice is NOT work

**The build half prices a job in work units; the lesson half is the same inversion in a deliberately
SEPARATE currency** (`docs/plan_unit_costed_work.md` §2), and **naming them apart is what stops anyone
adding them**:

```text
practice_this_turn = learn_rate × learn_multiplier(floor)   // per SOURCE per turn, NOT per worker
ledger_credit      = practice_this_turn / lesson_cost        // the ledger stays 0..1
```

| | **work units** | **practice units** |
|---|---|---|
| earned by | a **worker-turn** on the source | a **turn** the source is worked |
| scales with hands? | **yes** — that is what the build arc is for | **no** |
| scaled by the floor? | yes (`learn_multiplier`) | yes (`learn_multiplier`) |
| tools contribute? | yes | no |
| spent on | a per-source build meter | the faction knowledge ledger |

> **LEARNING MUST NOT SCALE WITH HANDS, and that is a rule rather than an omission.** Knowledge is
> faction-level and credited **once per source per turn** (`credit_rung_lesson`), so a per-worker rate
> would let a faction learn ten times faster by piling hands onto one patch — the build arc's no-cap
> decision *without* the opportunity-cost brake that justifies it, since a second lesson costs nothing
> extra. **You learn by watching the practice, not by counting the hands doing it.**
> `RungDef::knowledge_accrual` therefore takes no `workers` argument at all, where
> `build_accrual` does; the asymmetry is pinned by
> `a_lesson_is_paid_per_worked_turn_while_a_build_is_paid_per_worker`.

- **THE LEDGER STAYS NORMALIZED — the cost is a DIVISOR at the seam.**
  `DiscoveryProgressLedger::add_progress` clamps to `1.0` and is shared with great discoveries,
  espionage and the start profiles, so widening its unit would be a large blast radius for no gain.
  `knowledge.completion_threshold` stays the ledger bar, the wire's `IntensificationKnowledgeState`
  fields stay `0..1`, and the per-knowledge cost divides inside
  **`LadderKnowledge::ledger_credit`** — the one place `practice / lesson_cost` lives, so a bench and a
  rung cannot come to disagree about what a lesson costs.
- **`lesson_costs` is keyed by the KNOWLEDGE, not by the rung that teaches it**, because that is
  whose property the cost is: a knowledge can in principle be taught by more than one rung, and a
  **craft is taught by no rung at all**, so hanging the number off a rung record would make the same
  lesson cost two different things depending on where it was practised.

Three rules ride the seam:
- **Restraint is a RATE, not a predicate** (§4.2 as amended by `docs/plan_harvest_floor.md` §3). The
  practice is `knowledge.learn_rate × learn_multiplier(floor)`, so a crew that leaves more
  standing learns faster *in proportion*, with the food peak at ×1.0. It replaced a **step** at the
  peak (`components::floor_teaches`, now deleted): "teaches / does not teach" is not a question the
  model can answer any more. Non-degenerate at both ends — `floor = 0` strips the source and learns
  nothing because the rate is zero; `floor = 1.0` leaves it all standing and learns nothing because
  nothing is above the floor. A floor just *under* `1.0` on a full source therefore learns at nearly
  **×2 while taking almost nothing**: that is the trade the dial exists to offer taken to its limit,
  it self-limits (the source has to stand above the floor at all), and it is not a defect.
- **`eligible` is the caller's composed gate, and it carries the WORK PREDICATE** —
  `systems::labor::crew_is_working_the_source`, which asks whether anything stands above this
  assignment's floor. That replaced the `EcologyPhase::Thriving` term both earn sites used to carry.
  > **It is the escapement ROOM, in biomass, read pre-take and pre-quantisation — not `take > 0`.**
  > On plants the two coincide (a gather is continuous). On animals they do not, and the difference is
  > an artifact: `quantise_animal_take` rounds to whole animals, so a herd whose room is 60 biomass
  > against an 80-unit body hands over nothing this turn while the crew tracks, culls and handles it
  > exactly as before. Reading `AnimalTake::killed == 0` as *"not working"* would make the learning and
  > build rates depend on **`body_mass`** — big-bodied species pacing several times slower for a reason
  > nobody designed. The room still separates the two cases the gate exists for: *at or below your
  > floor, you are watching* (room `0`) from *surplus not yet banked into a whole body* (room `> 0`).
  **A rung-3 managed source is the exception**: a Field and a pen are *tended* and are never drawn
  down, so there is no escapement room to ask about and no floor the keeper chose. Both go through
  `systems::labor::credit_managed_rung_lesson`, which states that fact once — the lesson runs at the
  food peak (where `learn_multiplier` is `×1.0`) and the keeper is always working — instead of two
  named constants passed positionally at four call sites.
- **The two webs learn separately** (§4.2) — free, not enforced: the lesson is read off the source's
  own rung, and a rung belongs to exactly one branch, so a hunt can only ever reach an `animal`
  knowledge. A master rancher isn't automatically a farmer.

**The gate.** `intensification::knows(ledger, faction, discovery, threshold)` is **THE** knowledge
check — it retired the five inlined `get_progress(faction, ID) >= threshold` spellings (both labor
arms, the `cultivate`/`corral` assignment validators, and `extend_pen`), and the `tame` validator + the
`Tame` labor arm were built on it from the start. `threshold` stays a parameter to keep the helper a
pure comparison, but **there is now exactly one value any caller passes**: the ladder's
`knowledge.completion_threshold`. Every gate resolves its discovery off the **rung record**
(`unlock_discovery_id()`), never a hard-coded id, so a gate cannot drift from the rung the labor arm
accrues against.

**The dials are the ladder's** (`knowledge.learn_rate` **1.0** / per-knowledge `lesson_costs`
**20** / `completion_threshold` **1.0** → ~20 turns per lesson; `1.0 / 20` is the retired
`progress_per_turn` of `0.05` exactly, which is the inversion's own pacing proof). They **moved here
from the two identical per-web copies** (`labor_config`'s `forage.cultivation`, `fauna_config`'s
`husbandry`) once the earn path became one seam: a number that paces *both* webs belongs to the
ladder, exactly like the build dials. `LadderConfig::validate` states each bound **once** for both
webs — `learn_rate > 0` and finite (else the ladder silently freezes at rung 1), every `lesson_cost`
`> 0` and finite (a free lesson is known before it is learned, so every gate it holds is open on turn
1), and `0 < completion_threshold <= 1` (at `0` every gate is open on turn 1, above `1` no gate can
ever open since the ledger clamps to `1.0`).

> **EVERY LESSON THE SIM CAN TEACH MUST BE PRICED, and a missing entry is a LOAD FAILURE.**
> `validate_lesson_cost_coverage` walks every rung's `earns_knowledge` *and*
> `crafting::CRAFTS_WITH_A_DISCOVERY`, so a knowledge with no `lesson_costs` entry refuses to load
> rather than being paced by whatever a fallback happened to be — a number nobody chose, on a
> knowledge nobody could find the dial for, which is the parked-`0` failure mode in a new costume.
> Both readers (`knowledge_accrual`, `credit_craft_lesson`) therefore treat the map as total.
>
> **All eight are 20.** The spread — rung-3's `seed_selection`/`penning` dearer and `foddering`
> dearer again — is a later config-only slice.

**A fourth dial sits beside them, and it is a sibling rather than a reading**:
`craft_lesson_per_item` (**4.0** practice units, so `20 / 4` → 5 items per craft) paces the **crafts**
(`crafting.md`), which are earned per *item completed at a bench* rather than per turn worked. It is
here for the same reason the others are — every knowledge pace in the game is tuned in one file — and
it is a separate number because there is no floor to scale it by and no turn to charge it on. **It
moved with the currency rather than being left alone**: as `lesson_per_crafted_item` `0.2` it was a
fraction of a normalized threshold, and leaving it that way while its sibling became a cost is
precisely the drift the slice-4 consolidation existed to prevent. The three **crafts** are not rungs:
nothing in this file earns them, and `discovery_id_for` reaches them only by delegating to
`crafting::craft_discovery_id`, so a knowledge with no rung is still nameable by a start profile and
by a config — and, since this slice, is still **priced** by one.

**The pacing consequence — measured** (`fauna_husbandry::the_full_wild_to_pen_climb_is_paced_by_practising_each_rung`,
Wild Boar, on that fixture's herd): a pen is a **four-leg, ~46-turn climb** — Sustain-hunt wild →
**Herding** (20) → `Tame` (3) → Sustain-hunt the *pastoral* herd → **Penning** (20) → `Corral` (3).
The **Penning leg is new** (§4.3): pre-slice-4 Herding gated `Corral` directly. **Intended** — one
knowledge per transition, and you cannot skip a rung you have not practised.

> **The two BUILD legs collapsed from ~32 and ~25 turns once improvements were priced in work**
> (`docs/plan_unit_costed_work.md` §1.2), and the climb is now emphatically **knowledge-paced**: the
> lessons are crew-blind (credited once per source per turn) while a build's turns are
> `work_cost / crew output`, and an animal build's crew is the herd's own `herders_needed` — a real
> boar herd wants enough keepers to clear a 62.5-unit `Tame` in a handful of turns. **Under-crewing
> it to slow the build down is not available**: an under-herded flock *sheds*, so the keeper crew
> belongs to the herd rather than to the player. That is the accepted consequence of removing the
> crew cap, not a regression — the rung was **crew-blind** before, taking 25 turns whether two hands
> or twenty worked the herd.

### Behavior primitives — `movement` is live; `feeding`/`harvest` are still declarative

`behavior` is config over **coded** primitives (bounded enums): `movement` ∈ `fixed | roam |
drift_to_owner | pursue`, `feeding` ∈ `photosynthesis | forage | self_graze`, `harvest` ∈ `worker_take |
worker_tend | passive`. A rung that recombines existing primitives is pure config; a rung needing a
*new* primitive codes that one primitive once, after which it too is config.

- **`movement` IS READ** (slice 3b — the first primitive the engine applies): `fauna::advance_herds`
  resolves each herd's rung and dispatches on it, which is what makes §3's proximity spine
  (`roam` → `drift_to_owner` → `fixed`) a **config diff** rather than a code branch. `drift_to_owner`
  is the primitive slice 3b coded; see "Herd movement is a rung primitive" under Fauna & Wild Game for
  its ordering, its fallbacks, and the overgrazing tension it creates.
- **`pursue` is diet-resolved, not rung-assigned** (Predators Phase 2): `advance_herds` overlays it via
  `fauna::movement_primitive` on a **wild carnivore** (the trophic transpose of `drift_to_owner`, over
  the same shared `relocate_toward_resource` step). It is a first-class named primitive in the
  vocabulary, but **no rung record carries it today** — the husbandry rungs are diet-orthogonal
  (`animal:wild` is one rung shared by a deer and a wolf), so a carnivore's food-seeking movement can't
  be a husbandry-rung field yet. See "Herd movement is a rung primitive" under Fauna & Wild Game.
- **`feeding` / `harvest` are parsed and validated only** — the seam later slices switch on. `harvest:
  passive` is now **unused by every shipped rung**: retiring passive-free pastoral (§3, slice 3b) left
  no rung that pays without workers. The variant stays as vocabulary for a future rung that genuinely
  does.

### The config states TODAY's truth, deliberately

The whole thesis is that **later slices change behaviour by editing the JSON**, so the shipped file
describes the sim as it is, not as it will be:

| | rung 1 | rung 2 | rung 3 |
|---|---|---|---|
| **plant** | `wild` — no verb; **earns `cultivation`**; **`site_requirement` { requires_gathering_site }** (the branch's scarcity starts at rung 1 — gathering itself is site-bound) | `tended` — verb **`cultivate`**, gate `cultivation`, **earns `seed_selection`** (slice 4), **site rule inherited from rung 1** (Cultivate improves ground you already gather) | `field` — verb **`sow`**, gate **`seed_selection`** (slice 5 — the consumer that knowledge was earned for), **`site_requirement` { requires_gathering_site, requires_fresh_water }** (a watered gathering site — rung 3 moves seed, it cannot carry water, and it does not yet break unfamiliar ground; the 195 fertility floor went when the site rule took over its scarcity job, #464), earns nothing (`irrigation`/`rotation` = rung 4 Worked Land, parked); `movement: fixed` |
| **animal** | `wild` — no verb; **earns `herding`**; `movement: roam` | `pastoral` — verb **`tame`**, gate `herding`, ceiling `pastoral`, **earns `penning`** (slice 4); **`movement: drift_to_owner`, `harvest: worker_take`** (slice 3b — was `roam`/`passive`) | `pen` — verb **`corral`**, gate **`penning`** (slice 4 — was `herding`), ceiling `pen`, **earns `foddering`** (Flora Roster F3 — unlocks the pen's fodder-draw; `selective_breeding` = rung 4, still parked); `movement: fixed` |

Three consequences to keep straight, **all settled by slice 4**: `earns_knowledge` is **live, not
declarative** — every rung's lesson is read through `RungDef::knowledge_accrual` off the rung the source
stands on, and the per-web `knowledge_progress_per_turn` copies that used to drive it are gone (see
"The knowledge pattern"); **one knowledge per transition** — Herding gates `tame` **only**, `penning`
gates `corral` + `extend_pen` (the §4.3 reshuffle; pinned by `builtin_ladder_describes_todays_rungs`,
which asserts no two rungs share an unlock gate); and **both the build dials *and* the knowledge dials
now live here**, so the two webs can only be tuned — and paced — together.

### A RUNG ACHIEVED BUT SHORT IS REPAIRABLE — the gate tests the METER, not the rung (§4.7)

Reported from play: a Tended patch decayed to 99% and **there was no way to get it back to 100%**.
Four gates composed into a deadlock — completion retires the queue entry; no entry means no builders
can be aimed at it; `cultivate` refused; and the client offered no `⌃`. With keeping paid there is no
decay either, so the meter could not even fall out of the trap. The only escape was to *stop* paying
keeping, bleed to the retention bar, lose the rung, and re-buy it.

**§2.4 says it should be repairable** — *"repairing it is a fresh decision the player makes by putting
it back in the queue"* — and the sim already agreed in two places whose comments were written to
protect exactly this: the accrue guard is *"the METER, not the rung… refusing it here would make
erosion a one-way ratchet"*, and `patch_rung_already_built` uses meter fullness *"because a meter that
has eroded between the two is a rung the builders are legitimately repairing."* **Both were
unreachable, because completion removed the entry before either could matter.**

**The command's refusal now tests the meter, matching what the accrue path always did.** Locks 1 and 2
then need nothing: re-queueing recreates the entry and the entry brings the builders — verified rather
than assumed.

| verb | gate before | verdict |
|---|---|---|
| `cultivate` | `is_cultivated()` — the **retention bar** (37.5 of 50) | **the bug** → `cultivation_meter_full()` |
| `sow` | `is_field()` — the retention bar | **the same bug on rung 3**, found by the sweep → `field_meter_full()` |
| `corral` | `is_corralled()` — the fence flag | wrong SHAPE, no behaviour change (flag and meter always agree); aligned anyway so a pen meter that ever learns to erode is repairable by the same rule |
| `tame` | `is_domesticated()` | already correct — that predicate **is** `progress >= cost` |
| `extend_pen` | `pen_extending` / `at_max` | no deadlock; the meter resets on completion and three paths clear the flag |

**A FULL meter must still refuse** — that is the pair, and the case the *"already cultivated — forage
it to tend it"* message exists for. Weakening the refusal into nothing would be the other bug.

**One reporting defect rode along.** The board published `≈1 turn` **forever** for such a patch: the
quote was pushed for the derived verb regardless of the queue, and the unqueued tail dated it at the
**full pool** — a confident countdown for a build with no entry and no builders. A running quote is now
pushed only for a source that carries a rung entry, so the field stays at `-1`. **`-1` here means the
gate refused, at any staffing** — `eligible` takes no crew count — which is why the client must not
render it as a crew problem.

**The animal half is untestable and the fixture says so**: `domestication_progress` is monotone and no
animal rung declares a `meter_decay`, so "eroded but achieved" cannot be produced there. The refusal is
asserted on both webs; the accept only on the plant one.

### A KEEPING TOOL WEARS NOW, ON THE WORK IT SUPPLIED

Once gear fed the keeping pool (§4.8's upkeep half), a hoe raised what a keeper supplied **forever,
free** — `WearQuantum` had `BuildProgress` and no upkeep quantum. `UpkeepWork` closes it.

- **Charged on work SUPPLIED, not demand and not head count** — the value `patch_upkeep_supply` /
  `herd_upkeep_supply` returns, which the distributor already caps at the demand. So an over-large pool
  spends only what it was asked for, and a pool with nothing at risk claims no share and wears nothing.
- **`0.16` per work unit, and there is NO conversion to invert** — unlike `build_work`'s `0.5` (an exact
  round trip) this quantum never existed. What the model does supply is that **both quanta count the
  same unit**, so a tool holds one life measured in work whichever account spends it: `100 / 0.16 = 625`
  work units, identical to the build reading. Keeper-vs-builder life then matches by construction rather
  than by a target. **Provisional; §4.14 owns the balance.**
- **No item's headline gauge changed** — both entries are appended and `headline_wear` is the first, so
  hoes still lead with the build quantum and hurdles with the slaughter.

### A BLOCKED BUILD PUBLISHES **WHICH CONJUNCT REFUSED** (`docs/plan_standing_upkeep.md` §4.7)

`BUILD_QUEUE_BLOCKED` (`-4`) is minted in exactly one place — `publish_build_chain`'s `None` arm —
under one predicate:

```text
Blocked ⟺ position == 0 ∧ builders > 0 ∧ (the rung's own gate refused ∨ no quote this turn)
```

The pool is standing on the head of the queue, spending its worker-turns on an entry that banks
nothing, and **everything behind it inherits the block**. Reported from play: a Tame sat at
`Blocked 32%` with no cause on any surface, the player fixed the only thing the UI named — the keeping
shortfall — and it stayed blocked. The real cause was an empty escapement room.

**So the sentinel now travels with a CAUSE.** `BuildGate` replaces the old `BuildQuote::gate_holds`
bool — deliberately, because a bool beside an enum is two producers of one verdict — and its `key()`
is published as `buildBlockedReason` on `ForagePatchState` / `HerdTelemetryState`, `""` when the entry
is not blocked, **carried down the queue** so an entry behind a blocked head reports the head's own
reason rather than a second guess at its own.

| key | the conjunct that refused |
|---|---|
| `knowledge` | the rung's `unlock_discovery_id()` is not known |
| `escapement` | `crew_is_working_the_source(biomass − floor × K)` — no room above the floor |
| `no_crop` | no committed species (Cultivate) / no commitment (Sow) |
| `species_ceiling` | `can_domesticate()` / `can_pen()` — one fact about the animal, two rungs |
| `rung_below` | Corral on a herd that is not tamed |
| `owned_by_other` | the source is another faction's |
| `site` | the land does not admit the rung |
| `ring_idle` | a pen-ring entry with no extension running |
| `undeclared` | the meter's rung is not the one this entry declared — a DEAD entry |
| `unworked` | **not a conjunct** — no quote at all, the band's row on the source having lapsed |

**THE KEY SET IS READ OFF THE ARMS, NOT AUTHORED.** Each is a term of some rung's own `eligible`, and
two of them (`site`, `undeclared`) are terms of `sow_permitted` assembled OUTSIDE `accrue_field` — a
reader who only traced the Tame arm would have missed both. `species_ceiling` is the one deliberate
merge: the player's response to *this beast climbs no further* does not differ by rung, and the caller
already knows which rung it asked about.

**IT IS A CAUSE, NEVER A SENTENCE.** No player-facing prose lives in Rust; the client owns wording and
its table carries a fallback, so a key this sim adds later renders honestly rather than blankly.

> **THE KEEPING SHORTFALL IS NOT ONE OF THESE, and the client rule that said it was is corrected.**
> It is not a conjunct of any `eligible`; it reaches a build only by suppressing regrowth until the
> stock falls under the floor, which is the `escapement` key one step later. `selection-card.md` →
> "THE BLOCKED ROW NAMES THE REMEDY" carries the autopsy.

### AN UNNAMED `builders` KIT STORES **NOTHING**, OR THE PER-ENTRY DERIVATION IS DEAD

`handle_assign_labor` resolves a kit for every staffed row, and for `builders` with no `kit` token it
stored `default_kits.builders` — `"none"`. `EquipmentConfig::builders_kit_for` applies *a named row kit
wins* first, so that stored `none` beat the per-branch derivation §4.6b exists for, and **the pool
built bare-handed on every job**. Not cosmetic: `BuildersGear::resolve` reads the same field.

The client was already right — `BandPanelController._commanded_role_kit_id` emits no `kit` token on
that row precisely so the derivation stays live — and the server was filling the slot in on the way
past. The fork lives in `handle_assign_labor`'s `crew_kit` rather than in `default_kit_for_target`,
because the question is *what does this command store*, not *which kit is the absent one*; that helper
also serves the raid path, which has no derivation to defer to. **An explicit `kit <id>` still stores
and still wins.**

**IT SURVIVED BECAUSE EVERY FIXTURE HAND-BUILT THE ROW** (`kit: Some(bare_builders())`), so no test
ever drove `assign_labor … builders <n>` into `builders_kit`. The test that closes it drives the real
command path on both webs **and** asserts an explicit `kit none` is still honoured — without that third
case, "never store anything" satisfies the pair and silently deletes the override.

### The build on the wire — the fraction stays, the WORK is appended

**The meter stores absolute work units; the wire keeps publishing a `0..1` fraction, and the sim
divides at capture** (`docs/plan_unit_costed_work.md` §8 — the client does zero arithmetic, the
`penFeedUpkeep` discipline). `ForagePatchState.cultivationProgress` / `fieldProgress`,
`HerdTelemetryState.corralProgress` / `domestication` and the `isCultivated` / `isField` /
`corralled` bools are **unchanged in type, meaning and range**, so every shipped readout keeps
working untouched. `intensification::build_fraction` is the one divisor, and it divides by the
source's **own stamped** cost so a finished rung reads exactly `1.0` beside a predicate that already
says so.

**THE FRACTION AND THE WORK PAIR MUST BE CAPTURED FROM THE SAME FRAME.** A build meter accrues in
`advance_labor_allocation`, at **Population**, so anything the capture reads out of a display cache
written *earlier in the turn* is a turn behind the `*WorkDone` beside it. On the animal web that
cache is `HerdTelemetry` (Logistics), and `domestication` / `corralled` / `corralProgress` were taken
from it: a finished Tame shipped as *"50 / 50 work (99%)"* — the same meter stated twice, from two
turns, in one sentence. All three now read the live `Herd`; `husbandry.md` → "A herd row is assembled
from TWO frames" owns the provenance table. The plant web never had the defect — `ForagePatch` is
captured straight from the registry — and the guard against a recurrence on either web is the
equality itself: **`<rung>Progress == build_fraction(<rung>WorkDone, <rung>WorkCost)` in the frame it
ships in**, asserted on a real resolved turn in `core_sim/tests/build_turns_closed_form.rs`, on the
turn each build completes.

Appended (append-only) on both tables:

| Field | Answers |
|---|---|
| `cultivationWorkDone` / `cultivationWorkCost`, `fieldWorkDone` / `fieldWorkCost` | the plant meters in **work units**, and what each job costs |
| `tameWorkDone` / `tameWorkCost`, `corralWorkDone` / `corralWorkCost` | the animal pair, the Tame carrying the species' own cost multiplier |
| `buildTurnsRemaining` | how many more turns at the band's own builders pool — **chained** behind everything above this entry in that band's queue — and, with nothing queued here, the same question asked at the **back of the line** for the rung it would climb next. Five states: a count, `-2` the meter holds, `-3` the meter rots, `-4` the queue is blocked here, `-1` no answer |
| `buildQueuePosition` | this source's 0-based place in the winning band's build queue, `-1` when no band has queued it. Rides the **same winner** as the two fields above — they are read as one set |
| `buildWorkFromGear` | what that crew's **tools** took off the job, in work units — the `t` above |
| `buildWorkPerWorkerTurn` | what **one** worker banks per turn on this source at the food peak, before the floor multiplier and before gear — `intensification::build_work_per_worker_turn`, today `PER_WORKER_OUTPUT` |
| `cultivationUpkeepDemand` / `fieldUpkeepDemand`, `tameUpkeepDemand` / `corralUpkeepDemand` | what **that rung** costs to hold, per turn — the **rate** half of the same quote the `*WorkCost` beside it gives the **pile** half of |

- **`workCost` is the LADDER's price, not the source's stamped one.** It is resolved at capture off
  the rung (and, for a Tame, the species) and published **whether or not a build is in flight** — the
  compose sheet has to quote a rung's price *before* the player commits, and a source nobody has
  started carries a stamped cost of `0`.

> #### A PRICE WITHOUT THE RATE THAT EATS IT IS NOT A QUOTE
>
> **`<rung>UpkeepDemand` is `workCost`'s rule applied to the second term of the same quote**: the
> LADDER's `upkeep.work_per_turn` for that rung, resolved at capture, published whether or not a
> build is in flight. Four slots, one per built rung, read by the **same rung-picking rule** as the
> `*WorkCost` they sit beside (the assignment's `improvement`, else the rung above the one the
> source's published state stands on) — so the price, the meter and the rate always name one rung.
>
> **`upkeepDemand` cannot be that number, and appending rather than widening it is deliberate.** That
> field answers *"what is this source billed right now"* and resolves through the rung the source
> **stands on or is raising** (`forage::patch_unwinding_rung` / `fauna::herd_keeping_rung`) — which is
> what the decay pass bleeds and what the keeping readouts count down. On a source with no progress on
> any meter there is no such rung, so it publishes an honest `0`, and **that is exactly the source a
> compose sheet is looking at**. Netting a quote against it subtracts nothing, so the sheet promised
> `workCost / crew` turns for a build whose crew is under the rate: a wild patch, `Cultivate`, one
> builder, *"≈50 turns"* — and a meter that sits at `0 / 50` forever, because `plant:tended` demands
> `2.0` a turn and `1 − 2` is negative. **The price and the outcome were computed against different
> rungs.**
>
> The two coincide the moment a build is in flight and differ only before it starts, which is why the
> guard is a **no-progress** fixture on the encoded envelope
> (`build_turns_on_the_wire.rs::an_unstarted_patch_publishes_the_quoted_rungs_upkeep_where_the_billed_one_is_zero`)
> — a mid-build fixture passes with the gap wide open. The *agreement* is pinned too, on both webs,
> because two seams for one rate will otherwise drift.
>
> **The animal web hid it**, and that is the tell rather than an exemption: `herd_keeping_rung`
> answers for any **owned** herd, so a part-tamed herd's demand was published and its quote was right.
> A **wild** herd — the one a Tame is composed against — had the same hole. `tameUpkeepDemand` /
> `corralUpkeepDemand` carry the herd's own **keeper load**, ownership-independent, for
> `herdersNeededIfManaged`'s reason: a quote has to exist before the herd is anyone's.
>
> **`buildTurnsRemaining` WAS wrong, and §4.6a is what fixed it.** The projection netted the quoted
> rung's *rate* off the crew, so one builder against a demand of `2.0` published `-3` **the meter
> rots** about a build that finishes perfectly well in 50 turns (issue #545). The rate is the keeping
> pool's bill; what the projection nets now is the **rot**, which is `0` on ground nobody has started.
> The client's closed form nets the published `meterRotPerTurn` for the same reason and lands on the
> same number.
- **`buildWorkFromGear` is quoted BESIDE the raw price, never folded into it.** `workCost` stays
  the job as the ladder prices it, so a readout can say *"your hurdles: −17 work"* against a number
  that does not move under the crew's kit — and the estimate beside it already reflects the tooled
  bar. `0` = no build in flight, or nothing in the crew's hands that helps — which since the hoes
  shipped means a pool the player deliberately sent out bare, or one carrying the *other* web's tool.
- **`buildWorkPerWorkerTurn` IS THE CREW-OUTPUT TERM OF THE TURN ESTIMATE'S CLOSED FORM**, and it
  exists because `buildTurnsRemaining` beside it answers for the **committed** crew: a compose sheet
  drags a crew stepper and needs the answer for a crew the player is *proposing*.

  ```text
  gear(w)  = min(w, buildWorkSaturatingCrew) × buildWorkPerWorker      ← the band's kitTiers row
  turns(w) = ceil((workCost − workDone − gear(w))
                  / (w × buildWorkPerWorkerTurn − meterRotPerTurn))
  ```

  **The divisor's second term is the ROT, and there is no floor factor.** `<rung>UpkeepDemand` is the
  keeping pool's bill and is never netted off a build (§4.6a); `learn_multiplier(floor)` came off the
  build accrual with the crews' separation, so the crew term is the head count and nothing else. Both
  producers must carry the same expression or the sheet and the card disagree at every floor.

  - **The GEAR pair rides `PopulationCohortState.kitTiers[]`, not a source row**, because both of its
    terms — units held, and each unit's reach — are facts about the **band's ledger**. That is what
    lets a rung nobody has started be quoted at all, and what makes the sheet's **kit picker**
    re-price the estimate: picking a different kit reads a different row. The gear term
    saturates because coverage arms a **prefix** of a party (`EquipmentConfig::build_work_saturating_crew`),
    so an eleventh keeper with ten sets of hurdles between them takes nothing further off the job.
  - **The per-worker term is published rather than left a client `1.0`** because
    `intensification::build_work_per_worker_turn` is deliberately a **sum of terms** with exactly one
    term today (`docs/plan_unit_costed_work.md` §5) — the day a buff mechanic adds a second, the
    client tracks it with no change of its own.
  - **`buildTurnsRemaining` is unchanged and still required.** The tile card and the herd drawer have
    no stepper and go on rendering it; the sheet draws the curve, the card states the answer. At the
    committed crew and floor the two agree **exactly**, and so do the gear term and
    `buildWorkFromGear` — which is the whole safety argument for letting the client evaluate any of
    it. Pinned on the exported snapshot by `core_sim/tests/build_turns_closed_form.rs`, in both
    places, across the saturated and the linear gear regime.
  - **The food peak is NOT published.** The client holds `SourceForecast.FLOOR_FOOD_PEAK`, which must
    equal `fauna::MSY_BIOMASS_FRACTION` (`learn_multiplier(floor) = floor / MSY_BIOMASS_FRACTION`);
    the two are separate literals in separate languages, and the same test pins them together by
    **parsing `SourceForecast.gd` for its own `const`** — the `tuning_manifest_drift.rs` shape. It
    read a third Rust transcription of the client's value until PR #544's review: that asserted the
    sim against itself, so an edit to the GDScript fired nothing while this sentence claimed a guard.
- **`buildTurnsRemaining` IS A PROJECTION, never `-1`-because-nothing-is-being-built.** It is stamped
  by the labor arm (the only place the crew, the floor and the kit are all in hand) as transient
  per-turn scratch on `tended_this_turn`'s cycle, and cleared by the next turn's Logistics decay pass
  so an abandoned build stops publishing a finish date. With a verb in flight it is
  `build_turns_remaining` counting down the running meter; with **none** it is
  `LadderConfig::projected_build_turns` on the rung the source would climb next, at that same crew,
  floor and kit, from the work already banked on that rung.
  > **The compose sheet is BY DEFINITION looking at a source nobody has started**, so a sentinel there
  > withheld the one readout that makes this arc legible — *turns are an output; add hands and watch
  > them fall* — at the exact moment the player is deciding. That is the same defect
  > `HerdTelemetryState.penUpkeep` already fixed on the animal web (`husbandry.md`: *"a **projection**
  > for an unpenned herd, the **live** demand for a penned one… always meaningful, never
  > `0`-because-unpenned"*), and it takes the same remedy. The client still cannot compute it — it
  > holds neither the crew's output, nor the floor multiplier, nor the kit's coverage-weighted
  > contribution — so the sim answers.
- **IT IS A PER-SOURCE FIELD WRITTEN PER ASSIGNMENT, so several bands can answer for one source** —
  and the rule that decides between them is `systems::labor::BuildEstimateClaims`, not the order the
  labor loop happens to visit bands in. **A running build beats a projection** (a band merely
  gathering a patch another band is Cultivating published its quote of the *next* rung over the
  running build's countdown); **among running builds the soonest finish wins**, since every crew
  fills the same meter and each quote counts only its own output, so the smallest is the least wrong;
  and **a stall never displaces a moving crew, but still claims the source** — *"no estimate"* is the
  running build's own answer. **`buildWorkFromGear` rides the same winner**, because the two are read
  as one pair by the client's closed form.
  > **THE ORDER IS ONE STATEMENT: MORE NET SUPPLY IS BETTER NEWS** — `Turns(n)` (smaller `n` sooner)
  > → `Holding` → `Rotting` → silence. It is a `#[derive(Ord)]` on the private
  > `systems::labor::EstimateStanding`, so it is **total** and equal standings never displace one
  > another, which is what makes the published answer independent of the order the labor loop visits
  > bands in.
  >
  > **Holding above rotting is derived, not a taste call.** Among running builds the soonest finish
  > wins, and for one source a sooner finish *is* a larger net supply; the three non-count states
  > continue that same line past zero rather than starting a second rule. A crew holding the meter is
  > strictly closer to a finish than one destroying the work banked on it. Silence sits last because
  > it is the absence of an answer — which is also why the standing is its own enum rather than an
  > `Ord` on `Option<BuildTurns>`, whose derived order puts `None` **first** and would let silence
  > beat every answer.
  Guards:
  `forage_cultivation::{a_running_build_outranks_a_bystanders_projection_on_the_same_patch,
  the_soonest_of_two_building_crews_is_the_one_published}`, the second asserted under **both** spawn
  orders — a rule that holds for one order is last-writer-wins with a nicer number.
- **The projection resolves the next rung through the ladder's OWN order** — `RungKey::above`, an
  exhaustive match, composed onto the two seams that already answer *where does this source stand*
  (`forage::patch_rung_key`, `fauna::herd_rung_key`, the key-shaped halves of `patch_rung`/`herd_rung`).
  A new rung fails to compile until someone states its place in the climb, and
  `the_coded_climb_matches_the_shipped_ladders_own_order` pins the coded climb against the shipped
  records' `order` **in both directions**, so a `above` that answered `None` for everything — which
  would silently turn every projection back into "no estimate" — cannot pass.
- **A PROJECTION MUST NEVER QUOTE A RUNG THE COMMAND WOULD REFUSE.** Turns for a Sow on ground
  `validate_sow` rejects is the `sowSiteRefusal` failure mode in a new costume, so the quote carries
  the gates the verb would be judged by: the rung's `unlock_knowledge`, its `site_requirement` (through
  the same `rung_site_refusal` closure the running `Sow` is gated by), the tile's basket through
  `resolve_committed_species`, the species' `ceiling_required`, and **ownership** — which the live
  arms leave to `accrue_cultivation`/`accrue_domestication`, because a quote for a source another
  people are improving is a job this faction cannot take. Where it cannot answer a term it publishes
  `-1`: under-promising beats quoting a job the gates would refuse.
  - **It is the RUNG-2 arms that carry the work predicate**, exactly as the live ones do — rung 3's
    `eligible` omits `crew_is_working_the_source` on both webs (`accrue_field`'s reason: bare ground
    stands below every floor), so requiring escapement room would make the create-from-nothing rung
    unquotable.
- **`-1` now means there is genuinely NO ANSWER** (`sim_schema::NO_BUILD_TURNS_ESTIMATE`): the source
  is at the top of its ladder (a Field, a penned herd), one of the gates above refuses it for this
  faction, or **the rung's meter is at zero and nobody is building it** — nothing banked and nobody on
  it has promised nothing. A source with **work already banked** is otherwise never this, staffed or
  not: it publishes `-2` or `-3`, per "THE NO-ANSWER BOUNDARY IS WORK BANKED".
  > **A source nobody is standing on is NOT one of those.** The gate's work term
  > (`systems::labor::crew_is_working_the_source`) reads the **escapement room** — `max(0, B −
  > floor·K)`, a fact about the *stock against the assignment's floor* — and not about crew presence.
  > A patch nobody gathers regrows toward `K`, so its room is large, its gate is open, and an
  > abandoned half-built meter publishes an honest `-3`. **Only the animal web can lose that room and
  > not get it back**, because the hunters' draw and the suppressed regrowth of an unkept flock pin it
  > at the floor together — an *eligibility* stall no term the countdown is struck from can see. See
  > `.claude/rules/core_sim/husbandry.md` → "THE REGROWTH SUPPRESSION CLOSES A LOOP".
- **`workCost` and the turns must name ONE rung**, because they are read as a pair (*"50 work, ≈25
  turns"*). Both rungs' costs ship per source, so the client picks: the verb on
  `LaborAssignment.improvement`, or — when that is empty — the rung above the one the source's own
  published state says it stands on (`isCultivated`/`isField`, `domestication`/`corralled`). That is
  the same rung the projection resolved, derived from the same wire fields.
- **The projection reads the work already BANKED on that rung**, not a fresh `0`. On an unstarted
  source the two coincide; on one whose build the player abandoned, quoting the whole job again would
  contradict the `workDone`/`workCost` pair published beside it.

See Also: "Cultivation (Intensification Phase 1a)" (the plant rung 2), "Corral (Intensification Rung 1c)"
(the animal rung 3), "The husbandry yield ladder" (what each rung *pays*, which this arc does **not**
unify — animals pay flow-MSY against `r`, plants pay a flat rate without draw-down).

---

