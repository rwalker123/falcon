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
| `src/data/intensification_ladder.json` | **THE INTENSIFICATION LADDER** — one grammar for both food webs (`intensification.rs`, env override **`INTENSIFICATION_LADDER_PATH`**; design `docs/plan_intensification_ladder.md` §5). A `knowledge` block (**`progress_per_turn` 0.05 / `completion_threshold` 1.0** — the **base** pace of EVERY rung's `earns_knowledge` (scaled per call by the assignment's floor, `intensification::learn_multiplier`, so 0.05 is what the food peak pays) and the bar at which a faction may act on one, ~20 turns per lesson at the peak; **moved here in slice 4 from the two identical per-web copies** in `labor_config`'s `forage.cultivation` and `fauna_config`'s `husbandry`, once the earn path became one rung-driven seam — the number paces *both* webs, so it belongs to the ladder, exactly like the build dials) plus a flat `rungs` list; each record is one rung of one branch (`plant` = forage patches, `animal` = herds): `id`/`branch`/`order`, `verb` (the **`Improvement`** — `cultivate`/`sow`/`tame`/`corral` — that fills this rung's per-source build meter — **`null` = no verb drives this rung today, and the engine skips it**), `unlock_knowledge`/`earns_knowledge` (knowledge ids the rung gates on / **teaches when practised** — `null` = ungated / teaches nothing; **both are LIVE**: `unlock_knowledge` is what every gate resolves through, and `earns_knowledge` drives `RungDef::knowledge_accrual`, the one earn seam), `requires_rung` (the rung directly below on the ladder — the ladder is strictly sequential; **a claim about the ladder's SHAPE, not a per-source precondition** — no code reads it as one, and the per-source rule differs per branch: `corral` demands a herd you already tamed, `sow` demands no prior patch at all), `ceiling_required` (the per-species `husbandry_ceiling` gate, animal branch only), **`site_requirement`** (`{ min_forage_capacity, requires_fresh_water }` — **what the LAND must be** for the rung to be placed on a tile; the plant twin of `ceiling_required`, keyed on the ground instead of the species. `null` = the rung asks nothing of the site, i.e. every rung but `plant:field`. **Rung 4 (Worked Land) will be a looser copy of this record and nothing else**), `build` (`progress_per_turn`/`decay_per_turn`/**`grace_turns`**/**`crew_needed`**/**`yield_fraction_while_building`** — the per-source meter's **full-crew, food-peak** rate (the accrual is scaled by the assignment's floor too), its abandon-decay (**`null` = this rung's meter does not bleed at all**, which is the whole animal branch), the **neglect grace** (consecutive un-worked turns forgiven before the rung's penalty starts — a meter bleed on plants, the shed on animals: one trigger, two penalties), the **build crew** (`null` = this web sizes its crew off the SOURCE, not the rung — both animal rungs, where `herders_needed` comes from the herd's own size; on the plant rungs it both floors the source's `workers_needed` and scales the accrual by `min(workers/crew_needed, 1)`), and the **investment dip**, which multiplies the CREW'S THROUGHPUT while it prepares instead of harvests — never the take ceiling, `docs/plan_harvest_floor.md` §3.1; `null` on a rung with nothing to build), and `behavior` (the bounded coded primitives `movement` ∈ `fixed|roam|drift_to_owner|pursue` — **read by `fauna::advance_herds`, the first live primitive (slice 3b)**; `pursue` (Predators Phase 2) is currently **diet-resolved** for a wild carnivore in `fauna::movement_primitive`, not assigned by a rung record, because the husbandry rungs are diet-orthogonal — `feeding` ∈ `photosynthesis|forage|self_graze`, `harvest` ∈ `worker_take|worker_tend|passive` — the last two still **parsed and validated only**). **Shipped rungs** (`build` quoted as `progress`/`decay`/`grace`/`crew`/`dip`): plant `wild`(1, earns `cultivation`)/`tended`(2, verb `cultivate`, gate `cultivation`, **earns `seed_selection`**, build `0.04`/`0.01`/**`2`**/**`2`**/`0.25`)/**`field`(3, verb `sow`, gate `seed_selection`, earns nothing, build `0.04`/`0.01`/**`1`**/**`3`**/`0.25`, `fixed`, site `{ min_forage_capacity 195, requires_fresh_water true }` → **49 sowable tiles of 4160** on the standard map)**; animal `wild`(1, earns `herding`, `roam`)/`pastoral`(2, verb `tame`, gate `herding`, ceiling `pastoral`, **earns `penning`**, build `0.04`/**`null`**/**`2`**/**`null`**/`0.50`, **`drift_to_owner` + `worker_take`**)/`pen`(3, verb `corral`, gate **`penning`** (slice 4's §4.3 reshuffle — was `herding`), ceiling `pen`, **earns `foddering`** (Flora Roster F3 — running a pen teaches you to hay it; unlocks the fodder-draw, not a rung), build `0.04`/**`null`**/**`6`**/**`null`**/`0.50`, `fixed`). **The two webs' graces are not monotone in the same direction, and that is why the dial is per-rung**: on plants the NEWEST rung is the most fragile (a standing crop wants hands every turn; the cleared ground under it keeps its clearing longer), on animals the HIGHEST is the most forgiving (the fence does the holding). All four are playtest anchors. **The file describes what the sim does TODAY, deliberately** — later slices change behaviour by *editing it*. **Validated** — `LadderConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention): unique `(branch, id)` and `(branch, order)`, exactly one order-1 rung per branch, `requires_rung` resolving to a real same-branch rung at `order - 1` (and `null` iff `order == 1`), `verb` parsing to a real `Improvement`, `unlock_knowledge`/`earns_knowledge` resolving to a known discovery id, `0 < progress_per_turn`, `0 < decay_per_turn < progress_per_turn` **when present** (a `null` means the meter does not bleed; a parked **`0` is rejected** because it means the same thing while reading like a live dial — that is exactly how `animal:pastoral`'s dead `0.01` survived for slices, documenting a tameness-bleed the sim does not have), **`grace_turns < 1 / progress_per_turn`** (a grace that outlasts its own build makes walking away free for the whole span it took to build — a penalty evaporating silently, the time-axis twin of the site rule that requires nothing), **`crew_needed != Some(0)`** (a build wanting nobody would accrue at `min(workers/0, 1)`, i.e. never — say `null` for *no crew model*), `0 < yield_fraction_while_building < 1`, a `site_requirement`'s `min_forage_capacity` finite & `>= 0` **and the requirement actually requiring something** (a floor of `0` with `requires_fresh_water: false` admits every tile — a placement rule that places no rule, which is how a rung's scarcity evaporates silently; say `null` instead), **`knowledge.progress_per_turn > 0`** (else nothing is ever learned and the ladder silently freezes at rung 1) and **`0 < knowledge.completion_threshold <= 1`** (at `0` every gate opens on turn 1; above `1` no gate can ever open, since the ledger clamps accrual to `1.0`) — both **stated once, for both webs**, having moved from each web's own config — and **every rung the engine names by hand (`RungKey`) present** (so a broken override cannot silently no-op a shipped rung); a broken invariant is logged at **error** level (`intensification_ladder.invalid_rejected`) and the builtin is used. See "The Intensification Ladder" |
## The Intensification Ladder

**One grammar for both food webs** (`intensification.rs`, config `src/data/intensification_ladder.json`;
authoritative design: `docs/plan_intensification_ladder.md`). Plants and animals climb the *same*
three-rung ladder — rung 1 you take what's there, rung 2 you manage the wild source in place, rung 3 you
control its reproduction — and every rung-transition is the same **Cultivate-shaped verb**: pick it → the
source pays a **reduced** yield while the crew prepares rather than harvests → a **per-source build
meter** climbs → it decays if you walk away → at `1.0` the source steps up a rung.

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
  > hold progress across. (2) **The dip moved off the ceiling and onto crew throughput** (§3.1, below),
  > so it is floor-independent *by construction*: it no longer touches the floor's term at all, and
  > there is no floor a builder can pick to dodge it. What remains of the old measurement is a fact
  > about the *fixture*, not the web, and the tests that pinned the stall have been retargeted onto
  > what survives (`fauna_husbandry::a_low_floor_tame_takes_materially_longer_than_a_food_peak_one`
  > and its plant twin in `forage_cultivation`).
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
- **Commands.** `assign_labor … [stance] <workers>` sets the stance and **never touches the
  improvement** — which is what makes a *paused* build re-staffable (`validate_labor_policy` no longer
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
    bleed** (`advance_cultivation` applies the rung's `decay_per_turn = 0.01`, so a part-prepared patch
    reverts toward `0` over ~100 turns); **animal meters are kept** (`domestication_progress` is
    monotone-up since the neglect-escape arc, and `animal:pen`'s `decay_per_turn` is `0` — nothing
    reads `build_decay` on the animal branch at all). Inventing a forfeit here would make the command
    differ from walking the band away, which is the same decision with more steps;
  - names a **source**, not a verb, because at most one improvement is ever in flight on one — and
    reports on the running verb's own feed channel (`improvement_event_kind`), so a rung's whole life
    reads on one line.
- **On the wire:** `LaborAssignment.improvement:string` (`""` = a pure harvest) beside the
  now-retired `policy`; the per-source ceiling lists have since gone entirely (a continuous floor cannot be enumerated) and the two
  dips ship as fractions — see "Pre-commit Yield Forecast".

### The build engine — THE seam both tracks call

`RungDef::build_accrual(improvement, eligible, floor, timescale, workers)` / `build_decay(timescale)` /
`yield_fraction_while_building()` are the
**single** source of a rung's build math. Both food webs call them instead of reaching for their own
bespoke accrue/decay/dip levers, so the two ladders **cannot drift apart numerically** — that is the
whole reason the dials moved out of `labor_config`/`fauna_config` and into the ladder.

- **`build_accrual`** returns
  `progress_per_turn × learn_multiplier(floor) × timescale × crew_scale(workers)` **only** when
  `improvement` **is** the rung's own `verb` *and* the caller's rung-specific gates hold (`eligible` —
  knows the unlock knowledge, **the crew took something**, species ceiling allows, faction owns it);
  otherwise `0`. **A rung with `verb: null` is never driven** — which is what keeps the two `wild`
  rungs (nothing to build) out of the engine.
- **`learn_multiplier(floor)` — the same rate the lesson rides** (`docs/plan_harvest_floor.md` §3).
  See "The knowledge pattern" for the shape and both degenerate ends; what matters here is that it
  scales the **accrual only**. `build_decay` takes the same `timescale` and deliberately **not** the
  floor: decay happens on turns nobody works the source, so there is no assignment and no floor in
  play, and scaling it would multiply by a number that does not exist in that state. **Two build
  sites deliberately pass a floor that is not the assignment's**: `accrue_field` and the `Corral` arm
  omit the work predicate from `eligible` (rung 3 never had the `Thriving` gate it replaced, and bare
  ground stands below every floor by construction), and `ExtendPen` passes
  `MANAGED_SOURCE_FLOOR` because a ring is only ever built around a herd whose floor axis has already
  collapsed.
- **`crew_scale` — `min(workers / crew_needed, 1)`, `herded_fraction`'s exact shape** (and a rung with
  `crew_needed: null` is unscaled by crew, which is both animal rungs today). Full crew builds at the
  rung's stated rate, half a crew takes twice as long, over-crewing buys nothing — so
  **`progress_per_turn` is a FULL-CREW rate** and the ladder's "25 turns" means "25 turns at full
  crew". It rides *inside* the one build seam rather than at the call sites, because the other half of
  the same dial — `RungDef::build_crew_needed` flooring the source's `workers_needed` — is read
  elsewhere, and a demand the panel reports while the sim ignores it is worse than no demand at all.
- **`neglect_grace_turns` — the consecutive un-worked turns a rung forgives** before its penalty
  starts, the twin of `build_decay` on the *time* axis. Both webs count neglect the same way (a
  `neglect_turns: u16` on `ForagePatch`/`Herd`, reset by any turn the source's upkeep requirement was
  met) and gate different penalties on it: the plant meters bleed
  (`forage::advance_cultivation`), the animal flock sheds (`fauna::advance_husbandry`). **The rung is
  resolved through one seam per web** — `forage::patch_unwinding_rung` (the highest rung with progress
  on it, since the plant web unwinds newest-first) and `fauna::herd_keeping_rung` (`pen` if penned,
  else `pastoral` — *not* `herd_rung`, which would read `animal:wild` for a half-tamed herd and give
  the herd mid-investment the least forgiveness on the ladder) — and **the wire's countdown reads the
  same seam**, so a published "lapses in N turns" cannot describe a rung the sim is not acting on.
  `intensification::neglect_grace_remaining` owns the arithmetic: `(grace + 1) − neglect`, floored at
  zero, so **`0` means the penalty is biting now** and the client subtracts nothing.
- **`timescale` — the rung owns the mechanic, the source scales it** (slice 3c). `build_accrual` and
  `build_decay` take the **same** factor, so it dilates a source's whole build *timescale* and the
  rung's build:decay ratio is invariant. Today the only scaler is a species' **`taming_rate`** on
  `animal:pastoral` (`FaunaConfig::taming_rate_for`, resolved live by display name); every other
  caller passes **`RUNG_TIMESCALE_UNSCALED`** (the plant `tended` patch, the `pen` and its `ExtendPen`
  rings — penning is a flat build for every species). See "The `Tame` verb" for why scaling both is
  load-bearing.
- **The per-source state does not move.** `ForagePatch::cultivation_progress`,
  `Herd::domestication_progress` and `Herd::corral_progress` stay where they live: the engine supplies the *amount*, and the source owns its meter, the clamp to
  `RUNG_COMPLETE`, and the side-effects of completing it (ownership, `corralled_at`, the feed line).
- **Callers.** Accrual: the `Cultivate`, **`Tame`** and `Corral` arms of `advance_labor_allocation`
  (Population) — the *same* call, once per rung. Decay: `forage::advance_cultivation` and
  `fauna::advance_husbandry` (both Logistics; **the one-turn lag is deliberate** — each reads a flag
  the labor arm wrote *last* turn: `ForagePatch::tended_this_turn` / `Herd::tamed_this_turn`); the pen
  has **no** decay (`decay_per_turn: 0.0` — an untended pen escapes outright rather than bleeding).
  The dip: **one seam, `LadderConfig::build_dip(improvement)`**, read by `forage::forage_take`,
  `systems::hunt_take`, both forward projections and `fauna::forecast_expected_take` alike — so
  **forecast == actual** for free (see "Pre-commit Yield Forecast"). **Extending** a pen (2d-β) reads
  the *same* `animal:pen` rung, so a ring can never drift from the initial build.
- **THE DIP MULTIPLIES THE CREW, NOT THE CEILING** (`docs/plan_harvest_floor.md` §3.1). A take is
  `min(workers × per_worker × dip, max(0, B − floor·K))`: the crew is clearing ground or gentling a
  herd, not gathering, so it carries `yield_fraction_while_building ×` what a harvesting crew of the
  same size carries.
  - **It is floor-independent by construction**, which is the fix §0.3 called for rather than a patch
    on it. Multiplying the *ceiling* meant a deeper floor offered a bigger stock, so a fraction of it
    still filled the baskets and every stance completed a 25-turn Cultivate on schedule while the
    harshest paid 3.8× the food. The dip now does not touch the floor's term at all, so there is no
    floor a builder can pick to dodge it.
  - **It is also legible**: at 25% carry it takes four times the people to clear the same standing
    surplus. The corollary is real and intended — **a crew big enough to saturate the source's stock
    anyway pays nothing for the build**, because hands were not the scarce thing. Both regimes are
    asserted (`labor_yield_tests::{cultivate,corral}_policy_pays_the_dip_…`).
  - **A ceiling therefore carries no dip at all**, which is what makes `SourceYieldForecast::ceiling_at`
    linear in terms already on the wire and so composable by the client — see "THE CEILING LISTS ARE
    RETIRED" in `yield-forecast.md`. `BuildDips` and the `*BuildFraction` wire fields stay; what
    changed is which term the client multiplies.
- **Completion CLEARS the improvement — ONE seam for all four rungs.** A build verb only means "the
  crew is preparing, not harvesting", so once a meter fills it names a rung that can never accomplish
  anything more on that source and the dip would be charged forever for nothing. Each of the four
  accrual arms records the completing assignment's index, and a single post-loop pass in
  `advance_labor_allocation` sets that assignment's `improvement` back to `None`, preserving the
  source, the committed species, the worker count **and the stance**. **The completing turn still
  pays the dip** (accrual is after the take), so the undipped ceiling starts paying the next turn; the
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

Three rules ride the seam:
- **Restraint is a RATE, not a predicate** (§4.2 as amended by `docs/plan_harvest_floor.md` §3). The
  amount is `knowledge.progress_per_turn × learn_multiplier(floor)`, so a crew that leaves more
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
  down, so there is no escapement room to ask about and no floor the keeper chose. They pass
  `intensification::MANAGED_SOURCE_FLOOR` (the peak, ×1.0) and `MANAGED_SOURCE_IS_TENDED`.
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

**The dials are the ladder's** (`knowledge.progress_per_turn` 0.05 / `completion_threshold` 1.0 →
~20 turns per lesson). They **moved here from the two identical per-web copies** (`labor_config`'s
`forage.cultivation`, `fauna_config`'s `husbandry`) once the earn path became one seam: a number that
paces *both* webs belongs to the ladder, exactly like the build dials. `LadderConfig::validate` now
states each bound **once** for both webs (`progress_per_turn > 0` — else the ladder silently freezes
at rung 1; `0 < completion_threshold <= 1` — at `0` every gate is open on turn 1, above `1` no gate
can ever open since the ledger clamps to `1.0`).

**The pacing consequence — measured** (`fauna_husbandry::the_full_wild_to_pen_climb_is_paced_by_practising_each_rung`,
Wild Boar): a pen is a **four-leg, ~97-turn climb** — Sustain-hunt wild → **Herding** (20) → `Tame`
(32, at the boar's `taming_rate` 0.8) → Sustain-hunt the *pastoral* herd → **Penning** (20) →
`Corral` (25). The **Penning leg is new** (§4.3): pre-slice-4 Herding gated `Corral` directly, so the
climb was ~77 turns. **Intended** — one knowledge per transition, and you cannot skip a rung you have
not practised.

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
| **plant** | `wild` — no verb; **earns `cultivation`** | `tended` — verb **`cultivate`**, gate `cultivation`, **earns `seed_selection`** (slice 4) | `field` — verb **`sow`**, gate **`seed_selection`** (slice 5 — the consumer that knowledge was earned for), **`site_requirement` { min_forage_capacity 195, requires_fresh_water }** (the land must already be rich + watered — rung 3 moves seed, it cannot fertilize), earns nothing (`irrigation`/`rotation` = rung 4 Worked Land, parked); `movement: fixed` |
| **animal** | `wild` — no verb; **earns `herding`**; `movement: roam` | `pastoral` — verb **`tame`**, gate `herding`, ceiling `pastoral`, **earns `penning`** (slice 4); **`movement: drift_to_owner`, `harvest: worker_take`** (slice 3b — was `roam`/`passive`) | `pen` — verb **`corral`**, gate **`penning`** (slice 4 — was `herding`), ceiling `pen`, **earns `foddering`** (Flora Roster F3 — unlocks the pen's fodder-draw; `selective_breeding` = rung 4, still parked); `movement: fixed` |

Three consequences to keep straight, **all settled by slice 4**: `earns_knowledge` is **live, not
declarative** — every rung's lesson is read through `RungDef::knowledge_accrual` off the rung the source
stands on, and the per-web `knowledge_progress_per_turn` copies that used to drive it are gone (see
"The knowledge pattern"); **one knowledge per transition** — Herding gates `tame` **only**, `penning`
gates `corral` + `extend_pen` (the §4.3 reshuffle; pinned by `builtin_ladder_describes_todays_rungs`,
which asserts no two rungs share an unlock gate); and **both the build dials *and* the knowledge dials
now live here**, so the two webs can only be tuned — and paced — together.

See Also: "Cultivation (Intensification Phase 1a)" (the plant rung 2), "Corral (Intensification Rung 1c)"
(the animal rung 3), "The husbandry yield ladder" (what each rung *pays*, which this arc does **not**
unify — animals pay flow-MSY against `r`, plants pay a flat rate without draw-down).

---

