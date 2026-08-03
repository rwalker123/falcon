---
paths:
  - "core_sim/src/{forage,intensification}.rs"
  - "core_sim/tests/forage_*.rs"
---

<!-- Extracted verbatim from lines 2554-2825 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Cultivation and the `Sow` verb — the plant twin of the pen

## Cultivation (Intensification Phase 1a)

The **plant analog of animal husbandry** (`docs/plan_intensification.md` §3), evolved past the
mechanical husbandry transpose into **Rung 1a — the worker-tended, place-local tended patch**, and now
into an **explicit policy with an investment cost**. A patch carries `cultivation_progress` (0–1,
`1.0` = cultivated) + `owner: Option<FactionId>` on `ForagePatch`, mirroring a `Herd`'s
`domestication_progress`/`owner`; the checkpoint clones the whole `ForageRegistry`
(`SimState::forage`), so both rewind with a rollback. A completed patch is a **tended patch**:
**worker-tended + place-local + higher-output + feral-if-abandoned**. *Sim-only — the client readout is a follow-up.*

> **The free path is gone (design fix).** Cultivation used to accrue **silently and for free** under
> Sustain: same labor, same tile, no cost ⇒ cultivating was always correct and there was **no
> decision**. It is now the **`Cultivate` improvement** (`Improvement::Cultivate`, plant-only — a
> harvest-stance variant until issue #442 split the pressure axis from the build verb) with a real
> up-front cost, and the **early-claim `claim_threshold` is removed** (it would let the player skip the
> investment — the whole point). Gathering still *teaches* the faction Cultivation knowledge; it just
> never tames a patch. The animal twin is the **`Corral` policy** — see "Corral".
- **Rung 1b — the earned-knowledge gate (`docs/plan_intensification.md` §4b).** Cultivation is a
  faction-level knowledge *learned by doing*, **never start-granted**: **any** forage that actually
  draws a wild patch down accrues faction **Cultivation** knowledge (discovery
  `CULTIVATION_DISCOVERY_ID` = 2003, `forage.rs`) in the per-faction `DiscoveryProgressLedger`, at the
  ladder's `knowledge.progress_per_turn` **scaled by the assignment's floor**
  (`intensification::learn_multiplier` — a crew that leaves more standing learns faster, the food peak
  is ×1.0; `add_progress`, clamped to `1.0`). The old `Sustain && Thriving` pair of gates is gone —
  see "The knowledge pattern" in `intensification.md`. **A patch cannot accrue
  `cultivation_progress` until the faction *knows* Cultivation** — `advance_labor_allocation` only calls
  `accrue_cultivation` once `ledger.get_progress(faction, 2003) >= knowledge_completion_threshold`.
  Knowledge is all a plain gather earns — it **never** accrues `cultivation_progress`. The `cultivation` tag →
  discovery 2003 mapping is declared in `start_profile_knowledge_tags.json` purely so it is mappable;
  **no start profile lists it**, so no faction begins knowing Cultivation.
- **The `Cultivate` improvement — the investment.** In `advance_labor_allocation`'s **Forage** arm
  (Population), a patch worked with `Improvement::Cultivate` in flight:
  - **Costs a yield dip while preparing.** The crew carries the `plant:tended` rung's
    `yield_fraction_while_building ×` what a gathering crew of the same size carries — the dip
    multiplies **crew throughput**, never the take ceiling (`docs/plan_harvest_floor.md` §3.1). The
    crew is clearing and planting, not gathering. Two consequences follow and both are intended: a
    build costs yield **only while hands are the scarce thing** (a crew big enough to saturate the
    patch's standing stock anyway pays nothing for it — the remedy is to hire twice the people, at the
    shipped 0.50 dip),
    and the dip is **floor-independent by construction**, so there is no floor a builder can pick to
    dodge it. See "The build engine" in `intensification.md` for the measurement that forced the move.
  - **Accrues `progress_per_turn × learn_multiplier(floor)`** toward `1.0` (sets `owner` on first
    accrual; only the owner accrues), **gated** on the faction *knowing Cultivation* and on the crew
    something standing above its floor to work (`systems::labor::crew_is_working_the_source`).
  - **THERE IS NO HEALTH GATE, on the verb or on the accrual** (`docs/plan_harvest_floor.md` §3.2).
    `Cultivate` used to demand `EcologyPhase::Thriving` as a **start** gate, with an exemption for a
    build already underway (`ForagePatch::cultivation_underway`) and a whole start-vs-continue ruling
    to make the mid-build lapse survivable. All of it is deleted — the gate, the exemption, the helper
    and the ruling — because the floor replaced the cliff with a **rate**: a crew pulling hard on the
    ground it is clearing builds slowly, in proportion, and nothing lapses. The rejections that
    survive are the knowledge gate, the already-cultivated rejection, the other-faction-owner rule and
    the species gate; `server::tests::cultivate_is_accepted_on_a_stressed_patch` pins the absence
    positively, so a re-added phase check cannot regress silently.
  - **The other three rungs never had one**, which is why this is a *uniformity* rather than a
    relaxation: `validate_tame` carried no phase gate at all (a herd's `ecology_phase` swings as it is
    hunted), `validate_sow` gates on the rung's **`site_requirement`** — static land that cannot lapse
    — and `Corral` gates on ownership and the species ceiling. Rung 2 of the plant web was the only
    verb reading a value that moved under its own build.
  - **Accrues AFTER the turn's take**, so the turn pays exactly what the pre-commit forecast promised
    (forecast == actual). The turn progress reaches `1.0` is the last preparing take; the full tended
    yield starts the next turn.
  - **Marks the patch `tended_this_turn`**, so `advance_cultivation` spares a patch under active
    preparation — the investment accrues at the **full** `progress_per_turn` (25 turns at the default
    dials, at full crew **and at the food peak**), not net-of-decay.
  - **Break-even, and it is now a CROP choice** (defaults `fraction` 0.25, `progress_per_turn` 0.04).
    Measured on the reference basket below: the dip forgoes **0.527 prov/turn × 25 = 13.19 prov**
    across the build; favoring the basket's **best** staple then out-pays wild Sustain by
    **+0.625/turn** and recoups in **21.1 turns**, while favoring a **marginal** member of the same
    basket pays **+0.213/turn** and takes **61.8** — nearly 3× as long on the same tile for the same
    25 turns of work. *That spread is the decision the rung exists to make.* Cultivating is still
    correct only if you intend to stay — the decision the free auto-accrual erased.
  - `ForagePatch` methods: `is_cultivated`/`accrue_cultivation`/`decay_cultivation` (the early-claim
    `claim_cultivation` is **removed**).
- **Tended yield — a WILD STAND, gathered place-local** (slice 7 — the rung-2 correction). A tended
  patch is **worked, not passive**, and it is **still wild**: it rides a curve
  (`cultivation.tended_regrowth_gain`, folded in by **`forage::patch_ecology`** — the plant twin of
  `fauna::herd_ecology`, and the one seam every consumer resolves a patch's ecology through). **Flora
  Roster S2 retired the regrowth boost to a NEUTRAL 1.0** (`docs/plan_flora_roster.md` §4.3): once S1
  made competitor-removal explicit, a growth boost double-counted it, so tending pays through the
  **composition + conversion** of a committed crop, not the curve. **#433 fixed what "composition"
  means**: rung 2 **weeds** the tile's basket (the favored share rises to `min(1, share ×
  tended_weeding_gain)`, taken from the least abundant first) and **never touches `K`** — the retired
  concentration term cut a committed tile's capacity and discarded the remainder — and it pays
  `tended_conversion_gain` on the **favored species' term only**, which is the debt S2 recorded and
  left unpaid when it retired the regrowth boost with nothing in its place. It is gathered by the
  **ordinary `forage_take` path**, exactly like rung 1: **floor-live** (the assignment's own
  escapement floor — there is no policy axis left to be live on), **worker-capped**, and **drawn down** — so a tended patch **can
  be over-farmed** and the overdraw ⚠ fires on it. This is the exact shape a **pastoral** herd already
  had; the plant web used to collapse a rung *earlier* than the animal web, and that asymmetry was the
  bug. **A committed crop still out-yields the same patch's wild Sustain** on good ground — the
  intensification incentive, now carried by composition + conversion (guaranteed by the roster's bar,
  `core_sim/tests/flora_roster.rs`) rather than the retired boost. A *bare* tended patch (no crop)
  still pays **exactly** wild — no commitment means no weeding and no conversion gain, and
  `tended_regrowth_gain` is neutral, so every term is the identity.
  > **THE REFERENCE BASKET the measured figures in this file are taken on** — `AlluvialPlain`,
  > `K = 195`, the realization of tile `(0,0)` under seed `0xF10A_5EED_C011_0010` (the one the shipped
  > `sweep_tiles` fixtures use): `wild_emmer` 0.375 / `wild_tubers` 0.292 / `tobacco` 0.208 /
  > `wild_rice` 0.125, giving a wild basket rate of **0.0577** food and **0.0415** trade per biomass,
  > MSY 12.19 biomass/turn. Best staple `wild_emmer`, marginal member `wild_rice`. **Quote the basket
  > whenever you quote a number** — since #433 a per-tile figure means nothing without the
  > realization it came from.
  Working a completed improvement at either rung marks it `tended_this_turn` (a per-turn flag, off the
  client wire, carried across the turn boundary by the Population→Logistics lag) so the decay pass can
  tell tended from abandoned. The old
  even-split-across-all-the-owner's-bands payment in `advance_cultivation` is **retired**, as is the
  flat `tended_provisions_per_biomass` managed rate.
  - **Completion CLEARS the improvement — a completed patch is never left building.** The dip means
    "the crew is preparing ground, not gathering", which is why it is charged for the whole build; the
    moment the meter fills it stops being true *and can never become true again on this ground*, so
    `Cultivate` is a dead rung there. `advance_labor_allocation` therefore sets the completing
    assignment's `improvement` back to `None`, preserving the tile, the **committed species**, the
    worker count **and the stance**. **The completing turn still pays the dip** (the accrue-after-take
    ordering: the turn progress reaches `1.0` is the last preparing take), and the undipped ceiling
    pays from the next turn. The completion event's detail is
    `status=complete action=cultivate x=… y=…`; the `retired_policy=` token went with the constant
    (issue #442 — the pass used to rewrite `policy` to `HARVEST_POLICY_AFTER_BUILD`). **This is the
    one seam for all four build verbs** — `Sow`, `Tame` and `Corral` clear identically, from the same
    post-loop pass; **every accrue helper on both webs now returns a "this call finished it" `bool`**
    (`ForagePatch::accrue_cultivation` / `accrue_field`, `Herd::accrue_domestication`), mirroring
    `Herd::accrue_corral`, so the feed line rides the *transition* rather than a post-hoc
    `is_cultivated()` that is true for every crew once anyone has finished.
    **Clearing the verb is a separate question from announcing it**, because `cultivate`/`sow` set the
    improvement on every band working the tile — see "Completion CLEARS the improvement" in
    `intensification.md` for the once-per-source "nothing left to build" test that hands a
    non-finishing crew's verb back, and why it has to sit ahead of the Field arm's early return.
    **There is no lapsing gate left to worry about** (`docs/plan_harvest_floor.md` §3.2): a patch that
    is drawn down slows its own meter rather than stalling it, so the only way a build stops is the
    crew taking nothing at all — and then nothing is finished, so there is nothing to hand off.
- **Feral if unworked — AFTER A GRACE, NEWEST RUNG FIRST, and never silently.**
  `advance_cultivation` (`forage.rs`, `TurnStage::Logistics` alongside `advance_forage_regrowth`) is
  the **decay/feral** pass only. A patch **worked as an improvement this turn** (`tended_this_turn` —
  tending a completed patch *or* preparing one under Cultivate/Sow) is **spared** and its neglect
  counter reset; everything else counts a turn of neglect, and bleeds only past its rung's grace.
  - **The grace.** `ForagePatch::neglect_turns` counts **consecutive** un-worked turns (a single
    worked turn wipes it — it is not a lifetime budget), and the bleed applies only while it
    **exceeds** the decaying rung's `grace_turns` (`RungDef::neglect_grace_turns`). A crew re-tasked
    for a turn or two, a band that walked to answer a raid, a keeper following a herd: none of those
    cost the investment now. The animal twin is the same counter gating the shed in
    `fauna::advance_husbandry` — **one trigger, two penalties**. The predicate stays **binary and
    per-SOURCE**: a partly-crewed build accrues more slowly (the crew scale below) but is not
    *neglect*.
  - **The unwind is NEWEST-FIRST — exactly one meter per turn**, resolved through
    **`forage::patch_unwinding_rung`**: the Field while it has any progress at all, then the tended
    ground under it. **`cultivation_progress` cannot move while `field_progress > RUNG_UNSTARTED`.**
    > **The state this makes unreachable.** Bleeding both meters together knocked a *completed*
    > tended patch to `0.99` during a gap in the Sow work; once the crew returned, the running `Sow`
    > marked the patch worked every turn, so rung 2 could neither decay further nor re-accrue (only
    > `Cultivate` accrues it, and at most one improvement is ever in flight). The patch was stranded
    > one hundredth below a rung it had already paid for, **permanently**.
  - **A lost rung is ANNOUNCED**, on the edge where a completed rung crosses back below
    `RUNG_COMPLETE` — `ForagePatch::decay_cultivation`/`decay_field` return that transition, the exact
    mirror of the accrue helpers' "did this call finish it", and `forage::announce_rung_lost` pushes
    the verb's **own** feed kind (`Cultivate`/`Sow`, detail `status=feral reason=untended action=…`).
    Once, not every turn of the bleed that follows: the 25-turn payoff has already been destroyed. The
    animal twin is `fauna::announce_pen_lost`.
  - **On the wire:** `ForagePatchState.hasNeglectGrace` / `neglectGraceRemaining` — the **countdown**,
    not the counter (`0` = reverting now; a worked patch reads `grace + 1`, the honest *"walk away and
    you have this long"*), published through the *same* `patch_unwinding_rung` seam the pass bleeds
    through so the wire cannot count down against a rung the sim is not touching. `hasNeglectGrace =
    false` = a wild patch with nothing at risk, which is most of them — read the bool first, as with
    `owner`/`hasOwner`.
  - **Stage-ordering** is unchanged: Logistics runs *before* Population, so the `tended_this_turn`
    flag this pass reads was written by the labor arm **last** turn (a deliberate one-turn-lag
    carry-across-turns signal; the flag is cleared here and re-set next Population stage). Net: a
    patch worked every turn never decays; a patch whose band leaves starts counting toward its rung's
    grace one turn later.
- **The loop (the settle pull).** Sustain-forage a thriving patch → *learn* Cultivation → **choose** to
  pay the Cultivate dip for ~25 turns → the patch becomes tended → a band tending it collects the
  higher tended yield **place-locally** → move the band away and it goes feral, reverting to wild.
  Place-locality + feral + a sunk investment = the band is **pinned near its farm**: intensifying
  raises output *and* deepens the anchor.
- **`cultivate` command (repurposed)** — `cultivate <faction> <x> <y>` (`handle_cultivate`; unchanged
  proto/runtime/text plumbing, `CommandEventKind::Cultivate`) **sets the `Cultivate` improvement** on
  the band(s) already foraging that tile (`set_improvement_on_working_bands`) — the command form of
  what the client's checkbox does. It **claims nothing**, and since issue #442 it touches the
  improvement slot only, so the band's stance and its committed crop survive by construction (the
  `merge_target` helper that used to carry the crop across a whole-target rewrite is deleted). Gates
  (`validate_improvement`'s `Cultivate` arm): faction knows Cultivation, not already cultivated, not
  another faction's; plus a rejection when **no band is foraging** the tile (staff it first). **No
  health gate** — see "THERE IS NO HEALTH GATE" above.
- **`abandon_improvement <faction> forage <x> <y>` / `… hunt <herd_id>`** (`handle_abandon_improvement`;
  `AbandonImprovementCommand` proto field **46**, alias `abandon`) — the **clear** half of the four
  setting verbs, and the capability the two-axis split would otherwise have removed (see "An
  assignment has TWO axes" in `intensification.md` for why it is ungated and why it leaves the meter
  to `advance_cultivation`'s bleed).
- **Improvement validation** — `Improvement::valid_for_forage` / `valid_for_hunt`: `Cultivate`/`Sow`
  are plant-only and `Tame`/`Corral` animal-only, both stated as **exhaustive** matches so a new verb
  fails to compile until someone says which web it belongs to. `validate_improvement` rejects a
  cross-web verb (and a failed gate) with a clear failure event before touching the allocation.
  `assign_labor` validates the **stance** only; unassigning (`workers == 0`) is always allowed, so a
  player can always abandon an investment.
- **Sedentarization (folded)** — `sedentarization_tick` reads `herds.domesticated_count(faction) +
  forage.cultivated_count(faction)` for its **domestication** input: plant + animal domestication
  share the one driver (no new weight, no re-balance).
- **The build DEMANDS A CREW, and scales with it.** `plant:tended`'s `crew_needed` is **2** and
  `plant:field`'s is **3**, and both do two jobs (`RungBuild::crew_needed`):
  - **They floor the source's `workers_needed`** (`intensification::source_crew_needed`, the same
    `max(standing crew, take crew)` a managed herd's `herders_needed` has always used, with
    `LadderConfig::build_crew(improvement)` supplying the plant half). Without the floor the count came
    from the harvest alone — and while a build runs the crew's throughput is **dipped** — so committing to a
    25-turn improvement asked for **one** forager where the same wild patch under Sustain asks for two,
    and flagged the second worker as overstaffing. *Doing more work required fewer people.* **The floor
    has to reach the assign-time SEED as well as the resolved turn**, or the compose sheet and the tile
    card contradict each other in one frame — see "The crew floor is ONE definition" in
    `yield-forecast.md`.
  - **They scale the accrual**: `progress_per_turn × min(workers / crew_needed, 1)`, `herded_fraction`'s
    exact shape, inside `RungDef::build_accrual`. So **"25 turns" means "25 turns at full crew"** — a
    Cultivate run by one hand takes 50, a Sow by two takes ~38 and by one 75. Without it the crew
    demand would be a number the panel reported and the sim ignored. **The animal rungs declare no
    `crew_needed`** (a herd's crew comes from its size, not the rung) and are therefore **not**
    crew-scaled — a live asymmetry between the webs, stated rather than assumed.
  - On the wire as `ForagePatchState.cultivateCrewNeeded` / `sowCrewNeeded`, so the client can floor
    its compose-sheet cap the way it already floors a herd's on `herdersNeeded`.
- **Config.** The plant rung-2 **build dials moved to `intensification_ladder.json`**'s `plant:tended`
  rung (`build`: `progress_per_turn` 0.04 → 25 turns to prepare **at full crew**, `decay_per_turn` 0.01
  the feral-reversion rate, **`grace_turns` 2** — cleared, weeded ground keeps its clearing a couple of
  turns after the crew stops — **`crew_needed` 2** — the same two hands the reference tile's wild
  Sustain gather wants — **`yield_fraction_while_building` 0.50** — was the old `cultivating_yield_fraction` 0.25 until it was raised to match the animal rungs, which had always been 0.50,
  the investment cost: the preparing take ceiling as a fraction of the patch's Sustain/MSY ceiling), so
  the plant and animal ladders can only be tuned together (see "The Intensification Ladder"). What stays
  in `labor_config.json` `forage.cultivation` (`CultivationConfig`): **`tended_regrowth_gain`** (1.0 —
  NEUTRAL since Flora Roster S2: a tended patch's stock regrows exactly as fast as wild. It began as
  the plant twin of `husbandry.pastoral_gain`, but S1 made competitor-removal explicit, so a growth
  boost double-counted it; tending pays through composition + conversion and the rung-2 "wild <
  tended" guarantee moved to the roster. Kept as a playtest dial; only a gain *below* 1.0 is
  rejected), **`tended_weeding_gain`** (1.5 — how far rung 2 can push the favored species' share,
  `min(1, share × gain)`; the renamed `tended_concentration_gain`, which used to multiply `K` and now
  moves *composition*) and **`tended_conversion_gain`** (2.0 — rung 2's conversion multiplier on the
  **favored species' whole yield vector**, #433; the term that makes a 25-turn Cultivate pay back in
  the teens of turns instead of the eighties, and the reason a marginal favorite barely moves the
  number while a dominant one pays twice). Both validated finite and `>= 1.0`.
  **`field_concentration_gain` is RETIRED** — a Field forces the favored share to 1.0, so there is no
  gain left to tune. Plus the
  **Rung 1b earned-knowledge** levers `knowledge_progress_per_turn` (0.05 — faction Cultivation earned
  per gathered turn *at the food peak*, ~20 turns to know; the floor scales it) and
  `knowledge_completion_threshold` (1.0 = the ledger's completion value). The early-claim `claim_threshold` is **removed**. The build dials'
  invariants (`0 < progress_per_turn`, `0 < decay_per_turn < progress_per_turn` **when present**
  — `null` is how a rung says its meter does not bleed, and a parked `0` is rejected because it would
  mean the same thing while reading like a live dial — `grace_turns < 1 / progress_per_turn` (a grace
  that outlasts its own build makes walking away free), `crew_needed != Some(0)`,
  `0 < yield_fraction_while_building < 1`) are now **enforced on every load path** by
  `LadderConfig::validate()`, which owns them — as are the **knowledge** invariants
  (`knowledge_progress_per_turn > 0`, `0 < knowledge_completion_threshold <= 1`), which moved to the
  ladder with those dials in slice 4. **The levers homed here are now validated on every load path**
  (slice 7 — the old "asserted over the *builtin* only, so a `LABOR_CONFIG_PATH` override that breaks it
  is accepted silently" gap is **closed**): `LaborConfig::validate()` enforces the **plant ladder's
  monotonicity** — `field_provisions_per_biomass > tended_regrowth_gain × regrowth_rate/4 ×
  provisions_per_biomass × tended_conversion_gain` (tended < field, the payoff twin of
  `FaunaConfig::validate`'s `pen_gain > pastoral_gain > 1`). **It is evaluated at tending's SATURATED
  best case** — weeding pushed to 100% of the favored crop — because there the tended basket is the
  crop alone and the crop's own rate cancels from both sides, which is what keeps the check
  scale-free in `K` *and* independent of which species it is asked about. The `tended_regrowth_gain` check is now a **coherence floor only** — `>= 1.0`,
  not `> 1.0` — since S2 retired the "wild < tended" guarantee to the roster (`flora_roster.rs`); it
  forbids only the incoherent case of tending growing a stand *slower* than wild.
- **Intensification display snapshot (on the wire, consumed by the client-dev rendering slice next).**
  The intensification-ladder state is now exported to the FlatBuffers client stream (append-only per
  the schema discipline; `snapshot.fbs`, `sim_schema`, `snapshot.rs`), on both `WorldSnapshot` and
  `WorldDelta`:
  - **Forage patch cultivation** — a new per-tile `foragePatches:[ForagePatchState]` list
    (`snapshot_forage_patches`, from the `ForageRegistry`, stable `(y, x)` order). Per patch: tile
    `(x, y)`, `cultivationProgress:float` (0..1), `isCultivated:bool` (tended = progress ≥ 1.0),
    `owner`/`hasOwner` (tending faction; `hasOwner = false` = wild), plus `biomass`/`carryingCapacity`/
    `ecologyPhase` for optional patch-health. This is the client's first per-tile forage-patch payload
    (previously forage was visible only via `laborAssignments`).
  - **Faction ladder knowledge** — a per-faction
    `intensificationKnowledge:[IntensificationKnowledgeState{ faction, cultivation, herding,
    seedSelection, penning }]` list (`snapshot_intensification_knowledge`, from the
    `DiscoveryProgressLedger`), mirroring `sedentarization[]`. **One field per rung-transition**, so it
    reads as the ladder itself — `wild --cultivation--> tended --seedSelection--> field` and
    `wild --herding--> pastoral --penning--> pen` — each the 0..1 progress on discoveries 2003 / 2004 /
    **2005** / **2006** (the last two appended in slice 4, **append-only**: `cultivation`/`herding`
    keep their shipped slots). A faction is emitted only once it has begun learning *something* (all
    zero → skipped). Client renders these as learning/known meters like the sedentarization meter;
    the **two-meter split** (faction knowledge vs per-source build progress, §4.1 — the root UX fix)
    is the client slice, and both meters are already distinctly on the wire.
  - **Herd corral** — `HerdTelemetryState.corralled` (see the corral section above).
- **Follow-ups:** **Rung 1c — corral** (the fauna-side pen behind a `herding` gate) **shipped** — see
  "Corral (Intensification Rung 1c)" under Fauna & Wild Game. The **client _rendering_ for both ladders**
  (tile-card cultivation N% / tended-patch + Cultivation/Herding knowledge meters + herd corral
  indicator) is the **final Phase-1 slice** and remains a client-dev follow-up; the sim/schema data is
  now all on the wire (fields above).

## The `Sow` verb + the Field (Intensification rung 3) — the plant twin of the pen

**Rung 3 places a food source where you want it** (`docs/plan_intensification_ladder.md` §2, slice 5).
Once a faction knows **Seed Selection** (`SEED_SELECTION_DISCOVERY_ID` = 2005 — earned by *working
tended patches*, slice 4's `plant:tended` `earns_knowledge`; earned then, spent here), a crew working
a tile with **`Improvement::Sow`** in flight builds a **Field** on it. A Field is not a new entity: it is a
`ForagePatch` **at rung 3**, carrying its own `field_progress` meter beside `cultivation_progress` —
exactly as a `Herd` carries `corral_progress` beside `domestication_progress`. There is **no "extend
the field"**: each tile is its own patch, so you sow another field (the pen extends only because one
herd has one appetite).

- **Placed, not conjured — and SCARCITY IS THE POINT.** Rung 3 is *"I know how to take seed from a
  plant and put it somewhere else — but I do not know fertilization, so the land must already be very
  fertile, and near fresh water"*. That rule is the rung's **`site_requirement`** on the ladder record
  (`RungSiteRequirement` — the plant twin of `ceiling_required`, keyed on the **land** instead of the
  species), and both dials are levers:
  - **`min_forage_capacity: 195`** — a floor on the tile's own `tile_forage_capacity` (the *same*
    helper that sizes a wild patch and the wire's `forageCapacity`, never a Field-specific table). It
    admits exactly the **river-deposit class** — RiverDelta 210, Floodplain 205, AlluvialPlain 195 —
    and stops just above ordinary MixedWoodland (190).
  - **`requires_fresh_water: true`** — the tile must be on or beside **fresh** water
    (`forage::tile_is_fresh_watered`): `TerrainTags::FRESHWATER` on the tile, **or** a river along one
    of its six sides (`Tile::has_any_river_edge` — the hydrology edge primitive, set on *both* flanking
    hexes, so the riverbank needs no neighbour lookup), **or** a fresh-water hex next door (odd-r
    `hex_neighbors_wrapped`). A **salt coast is not water** for this — you do not farm sea spray.
  - **Measured on the standard map** (earthlike 80×52, seed 119304647, through the **real Startup
    chain**): **174 sowable tiles of 4160 (4.2%)**, against **2113** that merely bear food; over six
    seeds the mean is **197**. **The measurement only means anything with `generate_hydrology` run**:
    the rule wants fresh water, and rivers/deltas are hydrology's, so a fixture that skips it measures
    0 at every grid size and every seed. Of the tiles clearing the fertility floor, the water rule cuts
    about **40%** — the conjunction still bites, but far less hard than the fertility floor does.
    > **The "49 sowable tiles (1.2%)" this line carried until #466 was wrong, and the way it was wrong
    > is the lesson.** Both counters that produced it — `relief_sweep::sowable_and_deltas` and
    > `forage_field`'s own `spawn_world_on` — stop after `spawn_initial_world` + `generate_hydrology`,
    > skipping `apply_tag_budget_solver`, `apply_biome_palette_clamp`, `reconcile_coastal_shelf` and
    > `reconcile_food_modules`. That is an **intermediate** map: four later stages repaint terrain, and
    > they add sowable ground. Measured on the same seed the short harness reads 136 and the real chain
    > 174. **Count worldgen outcomes through `build_headless_app`**, never through a partial chain — the
    > figure was quoted in two rule files and used to argue that sowable ground was desperately scarce,
    > which it is not.
  - **Scarcity is real, but it lives in the MARKER list, not the tile count.** A player can only Forage
    where there is a curated `FoodSiteRegistry` marker (the client's `_forage_compose_available` reads
    `food_module`, which comes only from the wire's `food_modules`), and `sow` needs a band already
    foraging the tile. So the ground rung 3 can actually be built on is `markers ∩ sowable` — **130–134
    markers** per map (8% of land since #466; a flat 90 before), of which **73.8** are sowable once
    curation is biased toward fresh water, against **33.8** on pre-#466 main. See "Gathering markers
    follow the fresh water" in `worldgen.md`. *Which* tile matters ⇒ a
    band may have to **move** to farm at all. That friction is the design pillar, not a side effect.
  - **The refusal names the fault** (`SiteRefusal::{TooPoor, TooDry, TooPoorAndTooDry}` — the rung
    judges, the caller phrases) and points at **rung 4, Worked Land** (plows/irrigation, a future arc):
    *"Your people can carry seed, but not yet water or feed the land…until they learn to work the land
    itself."* Too poor and too dry are different problems with different answers (move, or wait).
  - **Rung 4 will be a LOOSER COPY of this record and nothing else** — a lower floor,
    `requires_fresh_water: false`. That is the arc's config-driven thesis paying out: a rung whose
    *placement rule* differs is a config edit (pinned by
    `a_looser_site_requirement_is_a_pure_config_edit`).
- **It needs no source below it** — the one place the two webs legitimately differ (§2). Seed travels:
  qualifying ground carrying *no forage site at all* is a legal target, and sowing it **creates** the
  patch (`ForagePatch::sown` — the tile's own biome capacity, biomass at the reseed floor, normal
  logistic regrowth). `Corral`, by contrast, needs a herd you already tamed. *(Reachability caveat,
  measured: worldgen seeds a patch on **every** food-bearing tile — `classify_food_module` tags
  essentially every biome — so on a generated map `Sow` always **upgrades an existing wild patch**. The
  create-from-nothing path is live and tested against constructed bare ground, but its input does not
  occur today. This is also the claim that the stale "~95% of tiles carry no `ForagePatch`" note above
  had made look true.)*
- **Never gated on the ground's health** — load-bearing, not a relaxation: sown ground starts at the
  reseed floor, i.e. *Collapsing* by construction, so a health gate would forbid the case the rung
  exists for. Rung 2 has since lost its own (`docs/plan_harvest_floor.md` §3.2), so this is now true
  of every rung on both webs rather than being rung 3's exception.
- **Its accrual carries NO work predicate either**, and that is the same fact stated on the other
  axis: `accrue_field`'s gate is Seed Selection and nothing else. The work predicate is what replaced
  each rung's `Thriving` gate, rung 3 never had one, and requiring it would make the
  create-from-nothing case impossible — **bare ground stands below every floor**, by construction. The
  floor still paces the build, so a crew stripping the ground it is sowing still builds nothing.
- **The investment.** The `plant:field` rung's `yield_fraction_while_building` (0.50) × the crew's own
  throughput (`docs/plan_harvest_floor.md` §3.1 — the dip multiplies hands, not the ceiling). On
  **bare** ground there is nothing for the crew to carry a fraction of, so a bare-ground sow is
  near-pure investment. `forage_field.rs` pins that as a
  **relation, not a pair of literals** — the whole build is a trickle beside the Field it buys
  (`while_building_per_turn < BUILD_TRICKLE_FRACTION × field_yield`) — because since #433 the Field's
  payout scales with the committed crop's own rate, so any literal would be true of one crop only.
- **The payout — rung 3 out-yields rung 2, or the rung is pointless.** A completed Field pays its
  workers `biomass × cultivation.field_provisions_per_biomass` (**0.02**, `labor_config.json`), the
  tended patch's *shape* at **2×** its rate, place-local and without drawing biomass down.
  `sustainable == actual` (no ⚠). **But the collection cap still binds** (slice 7): rung 3 collapses the
  *policy* axis, never the worker cap — you always carry the harvest home — so the actual take is
  `min(production, workers × per-worker throughput)`, `workers_needed` is derived, and the crop the crew
  could not carry is reported as `wasted`. **Measured production/turn on the reference basket** (see
  "Cultivation" → the callout; `AlluvialPlain`, K 195), committed to `wild_emmer`: wild Sustain
  **0.703** → tended **1.328** → Field **6.240**, needing **2 / 2 / 10** gatherers. The rung-2 crew is
  unchanged from rung 1 and that is not a typo — the gather cap is in **biomass**, and rung 2 changes
  the *rate* a take converts at, not the size of the take, so the same two people carry home nearly
  twice the food. Per-worker throughput is now **crop-dependent** (`8.0 biomass × the basket's rate`),
  so it is no longer the single 0.40 prov/worker figure this line used to quote.
- **Feral if abandoned — one rule for the whole plant web, and rung 3 goes FIRST.**
  `advance_cultivation` bleeds **one** meter per untended turn, the highest rung that still has
  progress on it, each at its own rung's `decay_per_turn` and past its own `grace_turns`
  (`plant:field`: decay 0.01, grace **1** — a standing crop is the most perishable thing on the ladder
  and wants hands every turn; `crew_needed` **3** — sowing *places* a source rather than tidying one).
  So an abandoned Field reverts to a **wild** gather patch and lapses to zero over ~100 turns, and
  only then does the tended ground beneath it begin to go. It does **not** step down to a tended patch
  on the way — that would pay the deserter rung 2's managed yield for free — but it no longer drags
  rung 2 down with it either: *the least-established improvement is the most fragile*. Ownership
  clears only once nothing is left of either meter. See "Feral if unworked" above for the grace, the
  ordering's own bug, and the feed line.
- **`sow <faction> <x> <y>` command** (`handle_sow`; `SowCommand` proto field **41**,
  `CommandEventKind::Sow`) — **sets the `Sow` improvement** on the bands already foraging that tile,
  the command form of the client's checkbox (issue #442: the stance beside it is left alone). It sows nothing outright; the seed goes in when the crew
  works the ground, so the improvement need only be checked once. Rejections, each
  distinct (`validate_sow`, shared with the `assign_labor` path): no such tile / **the land will not
  take seed** — *too thin*, *too dry*, or both, each naming the fault and pointing at rung 4 / faction
  hasn't learned **Seed Selection** ("Work tended patches to learn it") / already a Field / another
  people's ground / **no band is foraging it**. The site rule gates the **labor arm** too (both the
  seed placement and the build accrual), so the labor arm cannot farm ground the command refuses.
- **`cultivated_count` counts Fields** (`ForagePatch::is_managed`), so the sedentarization
  domestication signal cannot read rung 3 as *less* domesticated than rung 2 (a bare-ground Field
  carries no cultivation meter at all).
- **Persistence** — `field_progress` is its own meter beside `cultivation_progress` (mirroring
  `Herd::corral_progress` beside `domestication_progress`), and rides the checkpoint's whole-registry
  clone, so a rollback rewinds a half-sown Field.
- **On the wire (slice 6a — append-only, slots 36–44):** `ForagePatchState` carries
  `fieldProgress:float` + `isField:bool` (the rung-3 meter and the completed rung — read the *bool*,
  never infer a rung from the float) beside the already-shipped `cultivationProgress`/`isCultivated`,
  so the client has **both** plant meters for the §4.1 two-meter split; `ceilingSow:float` +
  `fieldYield:float` (Sow's payoff, the twin of `tendedYield`; its **dip** is the appended
  `sowBuildFraction`, which multiplies the CREW's throughput since `docs/plan_harvest_floor.md` §3.1,
  and `ceilingSow`/`ceilingCultivate`
  are retired `(deprecated)` slots — two rungs still keep two numbers, so a retune of one cannot move
  the other); and **`sowSiteRefusal:string`** — `""` when the ground takes seed, else
  `"too_poor"` / `"too_dry"` / `"too_poor_and_too_dry"` ([`SiteRefusal::as_str`], free-form per the
  `species`/`ecologyPhase` convention). That last one ships **the answer, not a bool**: only ~1% of
  tiles are sowable, so *"why can't I sow here?"* is the live question, and the client can re-derive
  nothing (it holds neither the capacity table nor the hydrology). The capture resolves it through the
  **same** `RungSiteRequirement::refusal` seam the command and the labor arm gate on — pinned by
  `the_exported_sow_site_refusal_is_the_verdict_the_command_acts_on`, so the wire cannot disagree with
  the gate.
- **On the client (slice 6):** the native reader — now
  `clients/godot_thin_client/native/src/dict/subsistence.rs::forage_patches_to_array`, not the old
  `lib.rs` home — surfaces all five as dict keys: `field_progress` / `is_field` /
  `sow_build_fraction` (the appended fraction that replaced the retired `ceiling_sow`, issue #442) /
  `field_yield` / `sow_site_refusal` (the last optional), beside the already-shipped
  `cultivation_progress` / `is_cultivated`. **Two spellings reach GDScript and they are not
  interchangeable:** `HudBandLaborState.forage_patch_lookup()` holds the keys **bare**, while the
  `tile_info` dict the tile card and compose sheet read carries a `patch_` prefix — except for the
  cultivation pair, which `MapView` stamps bare there too. Read whichever the caller's dict uses.

See Also: "Cultivation (Intensification Phase 1a)" (the rung below), "Corral (Intensification Rung 1c)"
(the animal rung 3 this mirrors), "The Intensification Ladder" (the engine + the config).

---

