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
| `src/data/intensification_ladder.json` | **THE INTENSIFICATION LADDER** — one grammar for both food webs (`intensification.rs`, env override **`INTENSIFICATION_LADDER_PATH`**; design `docs/plan_intensification_ladder.md` §5). A `knowledge` block (**`learn_rate` 1.0 / `lesson_costs` (a map, all eight at 20) / `completion_threshold` 1.0 / `craft_lesson_per_item` 4.0** — **A LESSON COSTS PRACTICE, AND PRACTICE IS NOT WORK**: `learn_rate` is what ONE TURN of practice at the food peak is worth, charged **once per source per turn** and scaled by the assignment's floor (`intensification::learn_multiplier`), and `lesson_costs[name]` is what that knowledge costs in those units, so `20` reads as *twenty worked turns at the food peak*. **It must NOT scale with hands** — knowledge is faction-level and credited once per source per turn, so a per-worker rate would let a faction learn ten times faster by piling hands onto one patch; *you learn by watching the practice, not by counting the hands doing it*, which is why `knowledge_accrual` takes no `workers` where `build_accrual` does. **The ledger stays normalized and the cost is a divisor at the seam** (`LadderKnowledge::ledger_credit` — `DiscoveryProgressLedger` clamps to `1.0` and is shared with great discoveries, espionage and the start profiles, so `completion_threshold` stays the ledger bar and the wire's `IntensificationKnowledgeState` fields stay `0..1`). `1.0 / 20` reproduces the retired `progress_per_turn` of `0.05` exactly — the inversion ships **pacing-neutral**. The map is keyed by the **knowledge**, not by the rung that teaches it, because a knowledge can be taught by more than one rung and a **craft by none**; `craft_lesson_per_item` is the crafting arc's dial and a *sibling* rather than a reading, because the quantum differs (per **item completed at a bench**, on the same quantum as that bench tool's wear), so `lesson_costs[craft] / craft_lesson_per_item` is a craft's length in **items** (`20 / 4` → 5). It lives here rather than in `recipes.json` so every knowledge pace in the game is tuned in one file — the same reason the ladder's own moved here in slice 4 — and it **moved with the currency** from `lesson_per_crafted_item` `0.2` rather than being left as a fraction of a normalized threshold, which is exactly the drift the consolidation existed to prevent. See `.claude/rules/core_sim/crafting.md`; **moved here in slice 4 from the two identical per-web copies** in `labor_config`'s `forage.cultivation` and `fauna_config`'s `husbandry`, once the earn path became one rung-driven seam — the number paces *both* webs, so it belongs to the ladder, exactly like the build dials) plus a flat `rungs` list; each record is one rung of one branch (`plant` = forage patches, `animal` = herds): `id`/`branch`/`order`, `verb` (the **`Improvement`** — `cultivate`/`sow`/`tame`/`corral` — that fills this rung's per-source build meter — **`null` = no verb drives this rung today, and the engine skips it**), `unlock_knowledge`/`earns_knowledge` (knowledge ids the rung gates on / **teaches when practised** — `null` = ungated / teaches nothing; **both are LIVE**: `unlock_knowledge` is what every gate resolves through, and `earns_knowledge` drives `RungDef::knowledge_accrual`, the one earn seam), `requires_rung` (the rung directly below on the ladder — the ladder is strictly sequential; **a claim about the ladder's SHAPE, not a per-source precondition** — no code reads it as one, and the per-source rule differs per branch: `corral` demands a herd you already tamed, `sow` demands a gathering site and fresh water — it used to demand no prior patch at all, which #464 reversed), `ceiling_required` (the per-species `husbandry_ceiling` gate, animal branch only), **`site_requirement`** (`{ requires_gathering_site, min_forage_capacity, requires_fresh_water }` — **what the LAND must be** for the rung to be placed on a tile; the plant twin of `ceiling_required`, keyed on the ground instead of the species. `null` = the rung asks nothing of the site, i.e. **every ANIMAL rung** — a herd carries its own site with it. **All three PLANT rungs state one** (issue #464): rungs 1–3 each `requires_gathering_site`, and rung 3 adds `requires_fresh_water` on top. `min_forage_capacity` is **0 on every shipped rung** — it carried rung 3's scarcity at 195 until the gathering-site rule took that job, and stacking both demanded a curated site that also landed on one of three biomes; it stays a live dial because **rung 4 (Farm) is the rung that needs it**. **Rung 4 IS this record with `requires_gathering_site: false` and the fertility floor put back** — that is the whole of what Farm unlocks, and it is a config edit), `build` (**`work_cost`**/**`grace_turns`** — **THE SIZE OF THE JOB IN WORK UNITS** (one unit = one worker-turn at the food peak with no gear; turns are the *output*, see "An improvement costs WORK, not turns") and the **un-worked-build neglect grace**, which is now the ANIMAL branch's alone (the shed in `fauna::advance_husbandry`): **`null` on both plant rungs**, whose penalty is an upkeep shortfall and whose grace therefore lives in `upkeep.grace_turns`. **`decay_fraction_per_turn`, `crew_needed` and `yield_fraction_while_building` are all RETIRED** — shortfall *is* the decay (`docs/plan_standing_upkeep.md` §2.4), and see "Three allocations per source" below; `null` on a rung with nothing to build), **`upkeep`** (**`work_per_turn`**/**`scaled_by`**/**`grace_turns`** — **WHAT IT COSTS TO HOLD THE RUNG, PER TURN, FOREVER** (`docs/plan_standing_upkeep.md` §2): the *rate* half of the ladder beside `build`'s *pile*, in the **same work units**, so *"what does it cost to hold this"* has one answer in one unit whichever rung is asked. `scaled_by` is a bounded coded primitive — the `behavior` idiom — `flat` (the rate as declared, the cost of the thing *existing*) or `source_head` (× the source's head count, `biomass / body_mass`: twice the animals, twice the keeping); `grace_turns` is consecutive turns of **shortfall** forgiven before the decay starts, and it is the rung's own number rather than a reading of `build.grace_turns` because a rung may be forgiving about an unworked build and strict about an unpaid bill. **BOTH PLANT RUNGS DECLARE ONE; both animal rungs are still `null`** (their neglect is the shed, not a meter). The plant numbers are the **pacing-neutral inversion** of the retired `decay_fraction_per_turn` — `0.01 × 50` and `0.01 × 75` — so a fully unmaintained improvement decays at exactly the rate it always did, and they are *not* a considered spread), and `behavior` (the bounded coded primitives `movement` ∈ `fixed|roam|drift_to_owner|pursue` — **read by `fauna::advance_herds`, the first live primitive (slice 3b)**; `pursue` (Predators Phase 2) is currently **diet-resolved** for a wild carnivore in `fauna::movement_primitive`, not assigned by a rung record, because the husbandry rungs are diet-orthogonal — `feeding` ∈ `photosynthesis|forage|self_graze`, `harvest` ∈ `worker_take|worker_tend|passive` — the last two still **parsed and validated only**). **Shipped rungs** (`build` quoted as `work_cost`/`grace`, `upkeep` as `work_per_turn`/`scaled_by`/`grace`): plant `wild`(1, earns `cultivation`)/`tended`(2, verb `cultivate`, gate `cultivation`, **earns `seed_selection`**, build `50`/**`null`**, **upkeep `0.5`/`flat`/`2`**)/**`field`(3, verb `sow`, gate `seed_selection`, earns nothing, build `75`/**`null`**, **upkeep `0.75`/`flat`/`1`**, `fixed`, site `{ requires_gathering_site true, min_forage_capacity 0, requires_fresh_water true }` → **174 of 4160 tiles clear the water rule** on the standard map, of which the **130–134 curated gathering markers** are what a band can actually reach — see "Placed, not conjured" in `cultivation.md`, and note the **49** this row carried until #466 came from a partial-chain test harness)**; animal `wild`(1, earns `herding`, `roam`)/`pastoral`(2, verb `tame`, gate `herding`, ceiling `pastoral`, **earns `penning`**, build `50`/**`2`**, upkeep **`null`**, **`drift_to_owner` + `worker_take`**)/`pen`(3, verb `corral`, gate **`penning`** (slice 4's §4.3 reshuffle — was `herding`), ceiling `pen`, **earns `foddering`** (Flora Roster F3 — running a pen teaches you to hay it; unlocks the fodder-draw, not a rung), build `75`/**`6`**, upkeep **`null`**, `fixed`). **The two webs' graces are not monotone in the same direction, and that is why the dial is per-rung**: on plants the NEWEST rung is the most fragile (a standing crop wants hands every turn; the cleared ground under it keeps its clearing longer), on animals the HIGHEST is the most forgiving (the fence does the holding). All four are playtest anchors. **The file describes what the sim does TODAY, deliberately** — later slices change behaviour by *editing it*. **Validated** — `LadderConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention): unique `(branch, id)` and `(branch, order)`, exactly one order-1 rung per branch, `requires_rung` resolving to a real same-branch rung at `order - 1` (and `null` iff `order == 1`), `verb` parsing to a real `Improvement`, `unlock_knowledge`/`earns_knowledge` resolving to a known discovery id, `0 < work_cost` finite, **`grace_turns < work_cost / reference_output` when present** — and the identical bound on **`upkeep.grace_turns`**, since either trigger's grace outlasting its own build makes the penalty evaporate — where `reference_output = SOLE_BUILDER × PER_WORKER_OUTPUT` (a grace that outlasts its own build makes walking away free for the whole span it took to build — a penalty evaporating silently, the time-axis twin of the site rule that requires nothing; one builder is the LONGEST the build can take and therefore the loosest the bound can be, which is the safe direction for a guard, and it replaced a `crew_needed` divisor when the rung stopped declaring a crew), **`upkeep.work_per_turn > 0`** finite **when the block is present** (a parked `0` is rejected because it means *"no upkeep"* while reading like a live dial; say `upkeep: null` — the same rule the retired `decay_fraction_per_turn` followed) and **`upkeep.scaled_by` parsing to a real variant** (the `behavior` idiom: an unknown token fails the *parse* rather than resolving to a default nobody chose), a `site_requirement`'s `min_forage_capacity` finite & `>= 0` **and the requirement actually requiring something** (a floor of `0` with `requires_fresh_water: false` **and `requires_gathering_site: false`** admits every tile — a placement rule that places no rule, which is how a rung's scarcity evaporates silently; say `null` instead), **`knowledge.learn_rate > 0`** finite (else nothing is ever learned and the ladder silently freezes at rung 1), **every `lesson_cost > 0`** finite (a free lesson is known before it is learned, so every gate it holds is open on turn 1), **every knowledge the ladder can teach PRICED** — each rung's `earns_knowledge` and every craft (`crafting::CRAFTS_WITH_A_DISCOVERY`); a missing entry is a load failure rather than a silent default, because a defaulted pace is a number nobody chose — **`craft_lesson_per_item > 0`** finite, and **`0 < knowledge.completion_threshold <= 1`** (at `0` every gate opens on turn 1; above `1` no gate can ever open, since the ledger clamps accrual to `1.0`) — all **stated once, for both webs**, having moved from each web's own config — and **every rung the engine names by hand (`RungKey`) present** (so a broken override cannot silently no-op a shipped rung); a broken invariant is logged at **error** level (`intensification_ladder.invalid_rejected`) and the builtin is used. See "The Intensification Ladder" |
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
  > has since **retired entirely** — see "Three allocations per source". **The build is staffed in
  > its own right**, so a deep draw cannot build with a crew it is not paying for: §0.3's defect
  > cannot recur for the stronger reason that there is no shared crew at all.
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
  `set_improvement_on_working_bands` + `validate_improvement`.
- **And CLEARED by `abandon_improvement`** (`abandon_improvement <faction> forage <x> <y>` /
  `… hunt <herd_id>`, proto field 46, alias `abandon`) — the one path that passes `None`. **It is not
  a nicety: without it the split would have removed a capability by accident.** While the build verb
  *was* the policy, changing your mind meant picking another policy, so a 25-turn commitment could
  always be walked away from; giving the stance its own control left the improvement with a set-only
  one. It:
  - is **ungated** — abandonment is not a rung transition, so no knowledge, no species ceiling, no
    site and pointedly **no `Thriving` check**. A *stalled* build on unhealthy ground is exactly when
    a player reaches for it, and copying `cultivate`'s gates would make the remedy unreachable in that
    case. Its only rejections are "not a source kind" and "nothing is being built there";
  - **fails closed at PARSE time on an unknown source kind** (`CommandParseError::UnexpectedToken`),
    matching `assign_labor`'s identical `forage`/`hunt` grammar and `cancel_order`. The kind decides
    the *arity*, so a catch-all forage arm read the tile arity for any token: `abandon_improvement 1
    foo` reported "missing argument: target_x" — an argument unrelated to the mistake — and the
    4-token form parsed clean, with the server's rejection arriving asynchronously in the feed;
  - **does not touch the meter.** Each web already has a rule for a source nobody is improving, and
    this hands the source back to it — the same state an out-of-range lapse reaches. **Plant meters
    bleed** (`advance_cultivation` applies the at-risk rung's unmet `upkeep`, so a part-prepared patch
    reverts toward `0` over ~100 unkept turns); **animal meters are kept** (`domestication_progress` is
    monotone-up since the neglect-escape arc, and neither animal rung declares an upkeep yet).
    Inventing a forfeit here would make the command
    differ from walking the band away, which is the same decision with more steps;
  - names a **source**, not a verb, because at most one improvement is ever in flight on one — and
    reports on the running verb's own feed channel (`improvement_event_kind`), so a rung's whole life
    reads on one line.
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

- **`build_accrual`** returns **the WORK UNITS this crew produces this turn** —
  `workers × PER_WORKER_OUTPUT`, the build's own crew and no floor term — **only** when
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
  > **A bar the meter is already at or past is `1`, not `None`** (`BUILD_FINISHES_IN_ONE_TURN`), and
  > the two states that reach it are one sentence: the work is already banked, or **the crew's gear
  > pays the job off outright** — `effective_build_cost` is unfloored, so a well-equipped crew drives
  > the bar below zero, and §6.2 of the plan says such a bar *"completes the build on its first worked
  > turn"*. Answering `-1` there broke the arc's own headline claim at exactly the crew size that
  > demonstrates it: on the shipped roster six geared keepers take `6 × 8.5 = 51` off a 50-unit
  > `Tame`, so the estimate fell 25 → 13 → 4 → 2 → *nothing* as hands were added. Pinned at the seam
  > and on the exported snapshot (`build_turns_closed_form.rs`), in both the projection and the live
  > stamp.
- **`LadderConfig::projected_build_turns` — the same question asked of a rung nobody has started.**
  It assembles exactly the four calls the in-flight stamp makes (`build_cost` →
  `build_work_from_gear` → `effective_build_cost` → `build_accrual`, then `build_turns_remaining`)
  against a stated `banked` and the caller's composed `eligible`, so a quote for an unstarted job
  cannot be arithmetic the running build would disagree with. It is what makes `buildTurnsRemaining`
  a **projection** rather than a `-1` — see "The build on the wire".
- **`effective_build_cost` — what the CREW BROUGHT** (issue #515, `equipment.md` → "The build
  axis"). `intensification::build_work_from_gear` sums `EquipmentConfig::build_work_per_worker` over
  the crew through the coverage seam, and the ladder subtracts it from the job: `cost − t`, with
  **nothing under it**. `intensification::NO_BUILD_GEAR` (**`0.0`**) for a crew carrying nothing that
  helps — every plant build today, and every animal one whose crew left the handling gear at camp. It touches **neither** `build_accrual` **nor** the upkeep: the
  crew's hands are worth what they are worth, and what an unkept improvement forgets is what its
  keepers did not supply, which no tool can change.
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
  **How far a kit may shrink a job is the JOBS' and the TOOLS' own dials, not a structural floor**:
  a rung's `work_cost` and an item's `EquipmentStat::BuildWork` decide it between them, and later
  work is meant to be *impractical* bare-handed — which requires that the right tool be able to
  reduce a job to a small fraction of itself. A bar at or below zero completes on the first worked
  turn, the same no-cap outcome as putting fifty hands on it; `build_fraction` divides by the **raw**
  stamped cost and `build_turns_remaining` by the crew's output, so neither reads the bar at all.
- **NO `learn_multiplier(floor)` TERM** — see "THE FLOOR CAME OFF THE BUILD RATE". `build_accrual`
  takes no floor at all, and neither does the upkeep: what an improvement loses is the work its
  keepers did not supply, which is a fact about a crew and a rung rather than about how hard anyone
  is pulling on the source.
  `accrue_field` and the `Corral` arm still omit the *work predicate* from `eligible` — rung 3 never
  had the `Thriving` gate it replaced, and bare ground stands below every floor by construction.
- **`ExtendPen` names its crew like every other build**, on the four verbs' own grammar
  (`extend_pen <faction> <x> <y> <workers>`). It rides the same `animal:pen` rung as the pen it
  widens, so it staffs the same `improvement_workers` allocation, passes the same pool gate and reads
  the same gear — a ring cannot drift from the initial build. **It sets the crew without setting a
  verb**: a built pen carries no `improvement` for one to hang off, which is why the ring is
  command-driven rather than improvement-driven in the first place. Riding the assignment's *take*
  crew, which it did while the investment dip was the only cost of a build, made widening a fence the
  one build in the game that cost nothing the moment the dip retired.
- **THE GRACE — the consecutive turns a rung forgives** before its penalty starts. Both webs count on
  a `neglect_turns: u16` (on `ForagePatch`/`Herd`) and gate different penalties on it: the plant
  meters bleed (`forage::advance_cultivation`), the animal flock sheds (`fauna::advance_husbandry`).
  **What each counts, and therefore which grace it reads, differs by branch** — the plant branch
  counts consecutive turns of **upkeep shortfall** and reads `upkeep_grace_turns()`, the animal branch
  still counts **un-worked turns** and reads `neglect_grace_turns()` (`RungBuild::grace_turns`, which
  is `null` on both plant rungs precisely so there is no second number nothing reads). **The rung is
  resolved through one seam per web** — `forage::patch_unwinding_rung` (the highest rung with progress
  on it, since the plant web unwinds newest-first) and `fauna::herd_keeping_rung` (`pen` if penned,
  else `pastoral` — *not* `herd_rung`, which would read `animal:wild` for a half-tamed herd and give
  the herd mid-investment the least forgiveness on the ladder) — and **the wire's countdown reads the
  same seam**, so a published "lapses in N turns" cannot describe a rung the sim is not acting on.
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
  The take, the build and the keeping each read their own crew off the assignment
  (`LaborAssignment::{workers, improvement_workers, maintain_workers}`), and the take path is read by
  `forage::forage_take`, `systems::hunt_take`, both forward projections and
  `fauna::forecast_expected_take` alike, and the improvement axis moves none of them — so
  **forecast == actual** for free (see "Pre-commit Yield Forecast"). **Extending** a pen (2d-β) reads
  the *same* `animal:pen` rung, so a ring can never drift from the initial build.
- **THE BUILD IS NOT IN THE TAKE AT ALL** — see "Three allocations per source", which is where the
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
  - **It clears EVERY band's verb, and announces ONCE.** The two facts are separate, and conflating
    them is what broke: the four verb commands set the improvement on *every* band working the
    source, so a completion is always a many-bands event even though only one crew's accrual crosses
    `1.0`. So (a) each build arm's **feed line** rides the *transition* — `accrue_cultivation` /
    `accrue_field` / `accrue_domestication` all answer *"did this call finish it"*, `accrue_corral`'s
    long-standing convention — and (b) a separate **"nothing left to build"** test runs once per
    worked source, *before* the arm branches by rung, and clears the verb whoever finished it. The
    placement is load-bearing: a finished Field and a penned herd take a managed branch that
    `continue`s past the build blocks entirely, so a second crew's `Sow`/`Corral` was **permanent**
    (only `abandon_improvement` could clear it) while the rung-2 shapes merely self-healed a turn
    late with a duplicate feed line.
    **And the plant test asks `is_managed()`, not `is_cultivated()`** — `Sow` needs no prior patch, so
    a Field routinely stands on ground that was never tended, where `is_cultivated()` is false and the
    Field arm's early return skips the Cultivate block: a `cultivate` on such a patch stalled forever,
    silently, with the meter frozen at zero. A rung *above* the one the verb builds is still "nothing
    left to build here", and a Field that lapses flips the answer back, since the test is re-asked
    every turn.
  > **It used to rewrite `policy` onto a module constant `HARVEST_POLICY_AFTER_BUILD`
  > (the food peak)**, because the build verb had occupied the pressure slot and completion had
  > to hand *something* back — so the sim silently replaced the player's stated policy on a turn they
  > could not predict, and each completion event carried a `retired_policy=sustain` detail. The
  > constant, its ten call sites and that detail token are all deleted (issue #442): the stance was
  > never vacated, so there is nothing to restore.

### Three allocations per source — the PLAYER states the split

**A source carries up to three independent worker allocations from a band**
(`docs/plan_standing_upkeep.md` §2.2), and the player states each:

| Activity | Field | Set by |
|---|---|---|
| **take** | `LaborAssignment::workers` | `assign_labor` |
| **build** | `LaborAssignment::improvement_workers` | `cultivate\|sow\|tame\|corral <faction> <target…> <workers>` |
| **maintain** | `LaborAssignment::maintain_workers` | `maintain <faction> <source…> <workers>` |

Each crew's work is `intensification::activity_work(workers)` = `workers × PER_WORKER_OUTPUT`. There
is no pool, no priority order and no derived share:

```text
upkeep_supplied  = maintain_workers × PER_WORKER_OUTPUT
upkeep_shortfall = max(0, upkeep_demand − upkeep_supplied)     // → decay, past grace
build_work       = build_workers × PER_WORKER_OUTPUT           // − the crew's gear, off the JOB
take             = min(take_workers × per_worker_capacity, source_offer)
```

- **They draw on one finite band, and that IS the opportunity cost.**
  `LaborAssignment::staffed_total` sums all three, `LaborAllocation::assigned_total` sums that over
  the sources, and `BandWorkforce::assigned` reports it — so `idleWorkers` nets out builders and
  keepers like anyone else. **"No cap" means no cap on ONE ACTIVITY** (fifty hands may finish a
  Cultivate in a turn), never a licence to exceed the pool.
- **THE POOL IS ENFORCED AT THE COMMAND, BY REFUSAL** (`server::crew_is_affordable`). The four
  verbs, `extend_pen` and `maintain` each check the tightest band working the source and refuse an
  order it cannot staff, naming what is idle: *"Cultivating needs 9 workers — the band has 4 idle."*
  Without it a band of five could put five on the take, five on a Cultivate and five on the keeping
  and produce fifteen worker-turns. **The check is atomic across the source's bands** (a verb sets
  its crew on *every* band working the source, so a partial application would staff one crew and not
  another off one command) and **gives back the activity's own crew before counting**, so moving a
  build from two hands to three needs one idle hand rather than three.
  - **It refuses where `assign_labor` clamps**, and the asymmetry is deliberate: a smaller *gathering*
    crew is a coherent version of the same order, while a quietly-smaller *build* crew is a
    commitment the player believes they made and would only discover from the pacing.
- **`LaborAllocation::normalize` answers the other question — the band SHRANK** — and it trims all
  three crews, tail-first, shedding **maintain → build → take** within the tail row. A band that has
  just lost people keeps gathering longest, because the keeping and the build are investments and the
  food is not. It stayed on the take crews only while the build was unbounded, which left a shrunken
  band still fielding every builder and keeper it had before.
- **A build's and a keeping's crews carry across a re-staffing** (`LaborAllocation::set_assignment`),
  for the verb's own reason: they are commitments in flight, and nudging the gatherers is not a
  statement about either.
- **Clearing the verb frees its crew** (`set_improvement` with `None`), because hands on a build that
  is not running are hands the band cannot see are free.

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
  **maintain**'s — hands to meet the demand, in its own unit. A `max` across units was always the
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

#### Completion hands the build's crew to the keeping

The turn a meter fills, the post-loop pass in `advance_labor_allocation` clears the verb **and moves
its crew**: onto `maintain_workers` if the finished rung declares an upkeep (`RungDef::declares_upkeep`),
otherwise back to the idle pool. Without the carry-over a brand-new pen starts decaying on turn one
because nobody noticed it had begun costing something — the same punishment-for-invisible-arithmetic
§2.5 exists to prevent.

**Either way it is announced**, on the finished verb's own feed channel (`improvement_feed_channel`),
so a rung's whole life reads on one line and the player can re-task. No shipped rung declares an
upkeep, so every completion frees its builders today.

### Standing upkeep — what it costs to HOLD a rung

**Every cost on the ladder used to be a *job* — a fixed pile of work you finish once. `upkeep` is the
first *rate*** (`docs/plan_standing_upkeep.md`). `RungDef::upkeep_demand(source_measure)` /
`upkeep_grace_turns()` / `upkeep_decay(shortfall, shortfall_turns)` are the seam, the exact twins of
the build's three.

- **NO SHIPPED RUNG DECLARES ONE**, which is what makes the mechanism pacing-neutral: the term
  exists, is validated, is published, and demands nothing anywhere
  (`no_shipped_rung_declares_a_standing_upkeep`). `upkeep_demand` is therefore an honest `0`
  everywhere rather than a sentinel — `HerdTelemetryState::pen_upkeep`'s rule.
- **SHORTFALL IS THE DECAY, continuously** (§2.4). Meet the demand and the net is zero and the
  improvement holds; fall short and the meter loses **exactly the work that was not supplied**, past
  the upkeep's own `grace_turns`. Half the hands a meter needs means it slides at half rate — not at
  the full neglect rate and not at nothing, which the binary `tended_this_turn` /
  `tamed_this_turn` flags could not express.
  - **WHICH HANDS those are depends on whether the rung is BUILT** — its **builders** while the meter
    is still being raised, its **keepers** once it is held (`forage::patch_upkeep_supply`; the plant
    web's reading, and the shape the animal web will take). *You cannot be billed to hold something
    you have not finished building*, which is §0's own definition of the term; and an **abandoned**
    part-build still bleeds, because the hands it needed are not on it either. The trigger moves, the
    number does not. **`ForagePatch::tended_this_turn` and
  `RungBuild::decay_fraction_per_turn` are both RETIRED** with the plant web's move onto the term
  (`cultivation.md` → "SHORTFALL IS THE DECAY"); `Herd::corralled_tended_this_turn` is still live,
  because the animal web has not moved yet.
- **The scale term is the generic piece** (§2.6). `UpkeepScale::Flat` states the rate;
  `UpkeepScale::SourceHead` multiplies it by the source's head count (`fauna::herd_head_count` =
  `biomass / body_mass`, continuous rather than whole animals — an upkeep is a rate, not a take).
  Adding a primitive is coding one thing once, after which using it is a config edit.

#### "Stop maintaining this" is a crew of ZERO

**`LaborAssignment::maintain_workers`**, set by `maintain <faction> forage <x> <y> <workers>` /
`maintain <faction> hunt <herd_id> <workers>` (proto field **56**, `MaintainCommand`) — the
standing-cost sibling of `abandon_improvement`, and deliberately its grammar verbatim, **including
its rule that an unknown source kind fails at PARSE time** rather than being read with the forage
arity.

- **It exists because hard-coded priority creates a trap** (§2.5). A source needing 5 work a turn,
  staffed for 2: both go to upkeep, it is still 3 short so it decays anyway, and the crew has spent
  itself for nothing. With the crew as the control the same position is a real decision: *hold it and
  spend the hands, or write it off and put them somewhere else.*
- **There is no boolean beside the number.** A toggle would be a second way to say what the count
  already says, and the two could disagree: a source maintained by nobody and one deliberately
  written off are the same state. `ForagePatch::maintain` / `Herd::maintain` and the `maintain:bool`
  wire field retired for that reason.
- **It writes the BAND, not the registry.** The hands are the band's, they come out of the same pool
  as its gatherers and its builders, and `idleWorkers` has to see them — none of which a boolean on
  the ground could carry. It rides the checkpoint on the band's `LaborAllocation`, like every other
  assignment field.
- **No owner gate, and no cap beyond the band's own pool**: a band can only staff a source it is
  already working, and how it splits its hands between the take, the build and the keeping is its own
  business — but it cannot staff hands it does not have, so `maintain` passes the same
  `crew_is_affordable` refusal the four verbs do. A source no band works is refused with *"assign
  workers to it first"*.

#### The standing upkeep on the wire

`ForagePatchState` / `HerdTelemetryState` each carry **`upkeepDemand`**, **`upkeepSupplied`**,
**`upkeepShortfall`** and **`upkeepWorkersNeeded`**. The first three ship rather than two, per the
`penFeedUpkeep` discipline — the sim answers and the client does zero arithmetic.

- **`upkeepDemand` follows `penUpkeep`'s rule: always meaningful, never a sentinel.** A rung with no
  upkeep publishes an honest `0`.
- **`upkeepWorkersNeeded` is the MAINTAIN activity's own `workers_needed`** —
  `ceil(demand / PER_WORKER_OUTPUT)`, in keepers — beside the TAKE activity's
  (`SourceYield::workersNeeded`, in haulers). Two counts in two units, because a `max` across units
  was the compromise a single allocation forced.
- **There is no `maintain` flag on the wire.** *"Stop maintaining this"* is a crew of zero, so the
  state rides the number the player typed.
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
| `buildTurnsRemaining` | how many more turns at the crew, floor and kit that worked this source **this turn** — and, with no build in flight, the same question asked of the rung it would climb **next** |
| `buildWorkFromGear` | what that crew's **tools** took off the job, in work units — the `t` above |
| `buildWorkPerWorkerTurn` | what **one** worker banks per turn on this source at the food peak, before the floor multiplier and before gear — `intensification::build_work_per_worker_turn`, today `PER_WORKER_OUTPUT` |

- **`workCost` is the LADDER's price, not the source's stamped one.** It is resolved at capture off
  the rung (and, for a Tame, the species) and published **whether or not a build is in flight** — the
  compose sheet has to quote a rung's price *before* the player commits, and a source nobody has
  started carries a stamped cost of `0`.
- **`buildWorkFromGear` is quoted BESIDE the raw price, never folded into it.** `workCost` stays
  the job as the ladder prices it, so a readout can say *"your hurdles: −17 work"* against a number
  that does not move under the crew's kit — and the estimate beside it already reflects the tooled
  bar. `0` = no build in flight, or nothing in the crew's hands that helps, which is every **plant**
  build today (issue #539).
- **`buildWorkPerWorkerTurn` IS THE CREW-OUTPUT TERM OF THE TURN ESTIMATE'S CLOSED FORM**, and it
  exists because `buildTurnsRemaining` beside it answers for the **committed** crew: a compose sheet
  drags a crew stepper and needs the answer for a crew the player is *proposing*.

  ```text
  gear(w)  = min(w, buildWorkSaturatingCrew) × buildWorkPerWorker      ← the band's kitTiers row
  turns(w) = ceil((workCost − workDone − gear(w)) / (w × buildWorkPerWorkerTurn × floor / foodPeak))
  ```

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
  as one pair by the client's closed form. Guards:
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
  faction, **no crew is working the source** (the labor arm never visits it, so nothing is stamped), or
  the crew's output is zero and a running build is **stalled** — a stall has no finite answer, and a
  huge number would read as a promise.
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

