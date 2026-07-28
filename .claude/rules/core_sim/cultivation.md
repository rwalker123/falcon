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
> decision**. It is now the **`Cultivate` policy** (`FollowPolicy::Cultivate`, Forage-only) with a real
> up-front cost, and the **early-claim `claim_threshold` is removed** (it would let the player skip the
> investment — the whole point). Sustain still *teaches* the faction Cultivation knowledge; it just
> never tames a patch. The animal twin is the **`Corral` policy** — see "Corral".
- **Rung 1b — the earned-knowledge gate (`docs/plan_intensification.md` §4b).** Cultivation is a
  faction-level knowledge *learned by doing*, **never start-granted**: a **Sustain** forage on a
  **Thriving** patch accrues faction **Cultivation** knowledge (discovery `CULTIVATION_DISCOVERY_ID`
  = 2003, `forage.rs`) in the per-faction `DiscoveryProgressLedger` at
  `cultivation.knowledge_progress_per_turn` (`add_progress`, clamped to `1.0`). **A patch cannot accrue
  `cultivation_progress` until the faction *knows* Cultivation** — `advance_labor_allocation` only calls
  `accrue_cultivation` once `ledger.get_progress(faction, 2003) >= knowledge_completion_threshold`.
  Knowledge is all Sustain earns — it **never** accrues `cultivation_progress`. The `cultivation` tag →
  discovery 2003 mapping is declared in `start_profile_knowledge_tags.json` purely so it is mappable;
  **no start profile lists it**, so no faction begins knowing Cultivation.
- **The `Cultivate` policy — the investment.** In `advance_labor_allocation`'s **Forage** arm
  (Population), a patch worked under `FollowPolicy::Cultivate`:
  - **Costs a yield dip while preparing.** Its take ceiling is
    the `plant:tended` rung's `yield_fraction_while_building × sustainable_yield(..)` — a *fraction of the MSY ceiling*
    (`forage_policy_ceiling`, reusing the **shared** `sustainable_yield` helper, never a second
    formula). The crew is clearing and planting, not gathering. Because the take is a fraction of MSY
    it is **sustainable**, so the patch stays Thriving (which the accrual gate requires) — the cost is
    a pure yield dip, not a depletion.
  - **Accrues `progress_per_turn`** toward `1.0` (sets `owner` on first accrual; only the owner
    accrues), **gated** on the faction *knowing Cultivation* AND the patch being **Thriving**. If a
    gate lapses mid-run (another band overdraws the patch to Stressed) progress simply **stops accruing
    that turn** — it is not lost and the policy is not silently switched; the patch is still marked
    worked, so it doesn't decay either, and accrual resumes when it recovers.
  - **Thriving is a START gate, not a CONTINUE gate** (`validate_labor_policy`,
    `ForagePatch::cultivation_underway`). It asks whether the land is fit for a crew to *begin*
    clearing it, so **a build already underway for this faction** (`cultivation_progress >
    RUNG_UNSTARTED`, not yet complete, `owner == faction`) is **exempt from the phase check** — and
    from that check alone. The knowledge gate, the already-cultivated rejection and the
    other-faction-owner rejection all still run; the exemption is a *condition on one check*, never an
    early return past the ones below it (pinned by
    `a_paused_build_is_exempt_from_the_phase_check_and_nothing_else`, which fails against an early
    return). **This is what makes the mid-build-lapse ruling above actionable.** Adjusting the crew on
    a paused build re-issues exactly the `Cultivate` assignment the start gate refuses, so with the
    gate applied unconditionally the only executable response to a paused build was `workers == 0`
    (the one always-allowed path) — which clears `tended_this_turn` and starts the feral bleed. "Stops
    accruing, is not lost, is not switched" would have meant *abandon it or nothing*.
  - **The animal rungs have no such trap, checked rather than assumed:** `validate_tame` carries **no
    phase gate at all** (a herd's `ecology_phase` swings as it is hunted, so refusing the verb on it
    would be un-actionable churn), and `validate_sow` gates on the rung's **`site_requirement`** —
    static land that cannot lapse — plus knowledge, already-a-Field and the owner rule. Only the plant
    rung-2 gate reads a value that moves under the build.
  - **Accrues AFTER the turn's take**, so the turn pays exactly what the pre-commit forecast promised
    (forecast == actual). The turn progress reaches `1.0` is the last preparing take; the full tended
    yield starts the next turn.
  - **Marks the patch `tended_this_turn`**, so `advance_cultivation` spares a patch under active
    preparation — the investment accrues at the **full** `progress_per_turn` (25 turns at the default),
    not net-of-decay.
  - **Break-even** (defaults `fraction` 0.25, `progress_per_turn` 0.04): the dip costs ~75% of that
    patch's Sustain yield for ~25 turns ≈ `0.75 × 0.375 × 25` ≈ **7 prov** forgone; the tended patch
    then out-pays wild Sustain by `1.2 − 0.375` = **0.825 prov/turn**, recouping the investment ~8–9
    turns after completion. Cultivating is correct only if you intend to stay — the decision the free
    auto-accrual erased.
  - `ForagePatch` methods: `is_cultivated`/`accrue_cultivation`/`decay_cultivation` (the early-claim
    `claim_cultivation` is **removed**).
- **Tended yield — a WILD STAND, gathered place-local** (slice 7 — the rung-2 correction). A tended
  patch is **worked, not passive**, and it is **still wild**: it rides a curve
  (`cultivation.tended_regrowth_gain`, folded in by **`forage::patch_ecology`** — the plant twin of
  `fauna::herd_ecology`, and the one seam every consumer resolves a patch's ecology through). **Flora
  Roster S2 retired the regrowth boost to a NEUTRAL 1.0** (`docs/plan_flora_roster.md` §4.3): once S1
  made concentration explicit, a growth boost double-counted competitor-removal, so tending now pays
  through **concentration + conversion** (a committed crop), not the curve. It is gathered by the
  **ordinary `forage_take` path**, exactly like rung 1: **policy-live**
  (Sustain/Surplus/Deplete/Eradicate), **worker-capped**, and **drawn down** — so a tended patch **can
  be over-farmed** and the overdraw ⚠ fires on it. This is the exact shape a **pastoral** herd already
  had; the plant web used to collapse a rung *earlier* than the animal web, and that asymmetry was the
  bug. **A committed crop still out-yields the same patch's wild Sustain** on good ground — the
  intensification incentive, now carried by concentration + conversion (guaranteed by the roster's bar,
  `core_sim/tests/flora_roster.rs`) rather than the retired boost. A *bare* tended patch (no crop) now
  pays exactly wild (measured on `AlluvialPlain`, K = 195: wild **0.61** = bare tended **0.61**
  prov/turn; a committed Wild Emmer rises above it via conversion).
  Working a completed improvement at either rung marks it `tended_this_turn` (a per-turn flag, off the
  client wire, carried across the turn boundary by the Population→Logistics lag) so the decay pass can
  tell tended from abandoned. The old
  even-split-across-all-the-owner's-bands payment in `advance_cultivation` is **retired**, as is the
  flat `tended_provisions_per_biomass` managed rate.
  - **Completion RETIRES the build verb — a completed patch is never left on `Cultivate`.** The dip
    means "the crew is preparing ground, not gathering", which is why it is charged for the whole
    build; the moment the meter fills it stops being true *and can never become true again on this
    ground*, so `Cultivate` is a dead rung there. `advance_labor_allocation` therefore rewrites the
    completing assignment onto the harvest rung — the module constant `HARVEST_POLICY_AFTER_BUILD`
    (`FollowPolicy::Sustain`), declared once so all four investment rungs can only hand off to the
    same place — preserving the tile, the **committed species** and the worker count; only the verb
    changes. **The completing turn still pays the dip** (the accrue-after-take ordering: the turn
    progress reaches `1.0` is the last preparing take), and the harvest rung pays from the next turn.
    The completion event carries the handoff as `retired_policy=sustain` beside its
    `status=complete action=cultivate x=… y=…` detail. **This is the one seam for all four build
    verbs** — `Sow`, `Tame` and `Corral` retire identically, from the same post-loop pass; the plant
    rung-3 helper `accrue_field` returns a completion `bool` for it, mirroring `Herd::accrue_corral`.
    A gate that **lapses mid-build** is untouched by this and still keeps its build verb (a patch that
    drops out of Thriving holds its progress and simply stops accruing) — nothing is finished there,
    so there is nothing to hand off.
- **Feral if unworked** — `advance_cultivation` (`forage.rs`, `TurnStage::Logistics` alongside
  `advance_forage_regrowth`) is the **decay/feral** pass only. A patch **worked as an improvement this
  turn** (`tended_this_turn` — tending a completed patch *or* preparing one under Cultivate) is
  **spared**; everything else decays by `decay_per_turn`. So an **untended cultivated** patch **goes
  feral** (drops below `1.0` → reverts to a wild gather patch, then decays to 0 over
  ~`1/decay_per_turn` turns; owner clears at 0) and an **abandoned part-prepared** patch loses its
  investment the same way. **Stage-ordering:** Logistics runs *before* Population, so the
  `tended_this_turn` flag this pass reads was written by the labor arm **last** turn (a deliberate
  one-turn-lag carry-across-turns signal; the flag is cleared here and re-set next Population stage).
  Net: a patch worked every turn never decays; a patch whose band leaves reverts one turn later.
- **The loop (the settle pull).** Sustain-forage a thriving patch → *learn* Cultivation → **choose** to
  pay the Cultivate dip for ~25 turns → the patch becomes tended → a band tending it collects the
  higher tended yield **place-locally** → move the band away and it goes feral, reverting to wild.
  Place-locality + feral + a sunk investment = the band is **pinned near its farm**: intensifying
  raises output *and* deepens the anchor.
- **`cultivate` command (repurposed)** — `cultivate <faction> <x> <y>` (`handle_cultivate`; unchanged
  proto/runtime/text plumbing, `CommandEventKind::Cultivate`) now **sets the `Cultivate` policy** on
  the band(s) already foraging that tile (`set_policy_on_working_bands`) — the command form of what the
  client's policy picker does. It **claims nothing**. Gates (shared with `assign_labor` via
  `validate_labor_policy`): faction knows Cultivation, patch is **Thriving** *unless this faction
  already has a build underway on it* (the start-gate/continue-gate rule above), not already
  cultivated, not another faction's; plus a rejection when **no band is foraging** the tile (staff it
  first).
- **Policy validation** — `FollowPolicy::valid_for_forage` / `valid_for_hunt`: `Cultivate` is
  Forage-only and `Corral` Hunt-only. `handle_assign_labor` rejects an invalid combo (and a failed
  gate) with a clear failure event before touching the allocation; unassigning (`workers == 0`) is
  always allowed, so a player can always abandon an investment.
- **Sedentarization (folded)** — `sedentarization_tick` reads `herds.domesticated_count(faction) +
  forage.cultivated_count(faction)` for its **domestication** input: plant + animal domestication
  share the one driver (no new weight, no re-balance).
- **Config.** The plant rung-2 **build dials moved to `intensification_ladder.json`**'s `plant:tended`
  rung (`build`: `progress_per_turn` 0.04 → 25 turns to prepare, `decay_per_turn` 0.01 the
  feral-reversion rate, **`yield_fraction_while_building` 0.25** — the old `cultivating_yield_fraction`,
  the investment cost: the preparing take ceiling as a fraction of the patch's Sustain/MSY ceiling), so
  the plant and animal ladders can only be tuned together (see "The Intensification Ladder"). What stays
  in `labor_config.json` `forage.cultivation` (`CultivationConfig`): **`tended_regrowth_gain`** (1.0 —
  NEUTRAL since Flora Roster S2: a tended patch's stock regrows exactly as fast as wild. It began as
  the plant twin of `husbandry.pastoral_gain`, but S1 made competitor-removal explicit as concentration,
  so a growth boost double-counted it; tending now pays through concentration + conversion and the
  rung-2 "wild < tended" guarantee moved to the roster. Kept as a playtest dial; only a gain *below* 1.0
  is rejected), plus the
  **Rung 1b earned-knowledge** levers `knowledge_progress_per_turn` (0.05 — faction Cultivation earned
  per Sustain-forage-Thriving turn, ~20 turns to know) and `knowledge_completion_threshold` (1.0 = the
  ledger's completion value). The early-claim `claim_threshold` is **removed**. The build dials'
  invariants (`0 < progress_per_turn`, `0 <= decay_per_turn < progress_per_turn`,
  `0 < yield_fraction_while_building < 1`) are now **enforced on every load path** by
  `LadderConfig::validate()`, which owns them — as are the **knowledge** invariants
  (`knowledge_progress_per_turn > 0`, `0 < knowledge_completion_threshold <= 1`), which moved to the
  ladder with those dials in slice 4. **The levers homed here are now validated on every load path**
  (slice 7 — the old "asserted over the *builtin* only, so a `LABOR_CONFIG_PATH` override that breaks it
  is accepted silently" gap is **closed**): `LaborConfig::validate()` enforces the **plant ladder's
  monotonicity** — `field_provisions_per_biomass > gain × regrowth_rate/4 × provisions_per_biomass`
  (tended < field, scale-free in `K`, the payoff twin of `FaunaConfig::validate`'s `pen_gain >
  pastoral_gain > 1`). The `tended_regrowth_gain` check is now a **coherence floor only** — `>= 1.0`,
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
a tile under **`FollowPolicy::Sow`** builds a **Field** on it. A Field is not a new entity: it is a
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
  - **Measured on the standard map** (earthlike 80×52, seed 119304647): **49 sowable tiles of 4160
    (1.2%)** post the "divides, not valleys" arc — **35** on the pre-arc dome — against **2328** tiles
    that merely bear food. (The historical **46** figure predates that arc.) **The measurement only
    means anything with `generate_hydrology` run**: the rule wants fresh water, and rivers/deltas are
    hydrology's, so a fixture that skips it measures 0 at every grid size and every seed. The
    **conjunction is still doing the work** (pre-arc measurement: 337 tiles cleared the fertility
    floor and the water rule cut 291 of them, 86%). Few sowable tiles ⇒ *which* tile matters ⇒ a band may have to **move** to
    farm at all. That friction is the design pillar, not a side effect.
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
- **Not gated on Thriving, unlike Cultivate** — load-bearing, not a relaxation: sown ground starts at
  the reseed floor, i.e. *Collapsing* by construction, so a health gate would forbid the case the rung
  exists for. You *tend* a healthy wild stand; you *plant* bare ground. (`Tame` draws the same line.)
- **The investment.** The `plant:field` rung's `yield_fraction_while_building` (0.25) × what the ground
  would otherwise pay: the MSY dip on a wild patch (via `forage_policy_ceiling`), and the **managed**
  dip on a tended patch being upgraded (0.25 × its tended harvest — `forage_forecast` and the labor
  arm both read the one shared `field_yield_fraction_while_building`). On **bare** ground that is a
  fraction of nothing, so a bare-ground sow is near-pure investment: **~0.13 prov/turn across its
  25-turn build against the 2.1/turn the Field then pays** (measured, `forage_field.rs`).
- **The payout — rung 3 out-yields rung 2, or the rung is pointless.** A completed Field pays its
  workers `biomass × cultivation.field_provisions_per_biomass` (**0.02**, `labor_config.json`), the
  tended patch's *shape* at **2×** its rate, place-local and without drawing biomass down.
  `sustainable == actual` (no ⚠). **But the collection cap still binds** (slice 7): rung 3 collapses the
  *policy* axis, never the worker cap — you always carry the harvest home — so the actual take is
  `min(production, workers × per-worker throughput)`, `workers_needed` is derived, and the crop the crew
  could not carry is reported as `wasted`. **Measured production/turn on `AlluvialPlain` (K 195):**
  wild Sustain **0.61** → tended **1.22** → Field **3.90**, needing **2 / 4 / 10** gatherers
  respectively at 0.40 prov/worker.
- **Feral if abandoned — one rule for the whole plant web.** `advance_cultivation` bleeds **both**
  improvement meters at their own rung's `decay_per_turn` on any untended turn, so an abandoned Field
  reverts to a **wild** gather patch (after the pass's deliberate one-turn lag) and both meters lapse
  to zero over ~100 turns, ownership clearing only once nothing is left. It does **not** step down to a
  tended patch on the way: that would pay the deserter rung 2's managed yield for free, and *an
  improvement you stop working goes back to the wild* is the plant web's only story here.
- **`sow <faction> <x> <y>` command** (`handle_sow`; `SowCommand` proto field **41**,
  `CommandEventKind::Sow`) — **sets the `Sow` policy** on the bands already foraging that tile, the
  command form of the client's policy picker. It sows nothing outright; the seed goes in when the crew
  works the ground, so `assign_labor … sow` places a Field on identical terms. Rejections, each
  distinct (`validate_sow`, shared with the `assign_labor` path): no such tile / **the land will not
  take seed** — *too thin*, *too dry*, or both, each naming the fault and pointing at rung 4 / faction
  hasn't learned **Seed Selection** ("Work tended patches to learn it") / already a Field / another
  people's ground / **no band is foraging it**. The site rule gates the **labor arm** too (both the
  seed placement and the build accrual), so `assign_labor … sow` cannot farm ground the command
  refuses.
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
  `fieldYield:float` (Sow's "preparing X → then Y" pair, the twins of `ceilingCultivate`/`tendedYield`
  — `ceilingSow` is its **own** field for `ceilingTame`'s reason: two investment rungs on one branch
  must never share a ceiling); and **`sowSiteRefusal:string`** — `""` when the ground takes seed, else
  `"too_poor"` / `"too_dry"` / `"too_poor_and_too_dry"` ([`SiteRefusal::as_str`], free-form per the
  `species`/`ecologyPhase` convention). That last one ships **the answer, not a bool**: only ~1% of
  tiles are sowable, so *"why can't I sow here?"* is the live question, and the client can re-derive
  nothing (it holds neither the capacity table nor the hydrology). The capture resolves it through the
  **same** `RungSiteRequirement::refusal` seam the command and the labor arm gate on — pinned by
  `the_exported_sow_site_refusal_is_the_verdict_the_command_acts_on`, so the wire cannot disagree with
  the gate.
- **On the client (slice 6):** the native reader — now
  `clients/godot_thin_client/native/src/dict/subsistence.rs::forage_patches_to_array`, not the old
  `lib.rs` home — surfaces all five as dict keys: `field_progress` / `is_field` / `ceiling_sow` /
  `field_yield` / `sow_site_refusal` (the last optional), beside the already-shipped
  `cultivation_progress` / `is_cultivated`. **Two spellings reach GDScript and they are not
  interchangeable:** `HudBandLaborState.forage_patch_lookup()` holds the keys **bare**, while the
  `tile_info` dict the tile card and compose sheet read carries a `patch_` prefix — except for the
  cultivation pair, which `MapView` stamps bare there too. Read whichever the caller's dict uses.

See Also: "Cultivation (Intensification Phase 1a)" (the rung below), "Corral (Intensification Rung 1c)"
(the animal rung 3 this mirrors), "The Intensification Ladder" (the engine + the config).

---

