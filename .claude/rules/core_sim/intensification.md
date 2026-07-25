---
paths:
  - "core_sim/src/{intensification,knowledge_ledger}.rs"
  - "core_sim/src/data/intensification_ladder.json"
---

<!-- Extracted verbatim from core_sim/CLAUDE.md lines 1984-2119.
     Routing table and shared vocabulary live in core_sim/CLAUDE.md.
     Regenerate with scripts/split_core_sim_claude_md.sh -->

# The Intensification Ladder

**One grammar for both food webs** (`intensification.rs`, config `src/data/intensification_ladder.json`;
authoritative design: `docs/plan_intensification_ladder.md`). Plants and animals climb the *same*
three-rung ladder — rung 1 you take what's there, rung 2 you manage the wild source in place, rung 3 you
control its reproduction — and every rung-transition is the same **Cultivate-shaped verb**: pick it → the
source pays a **reduced** yield while the crew prepares rather than harvests → a **per-source build
meter** climbs → it decays if you walk away → at `1.0` the source steps up a rung.

**The ladder is DATA over a bounded set of coded primitives.** A rung is a [`RungDef`] record, the ladder
is a list, and adding a rung that recombines existing primitives is a one-record edit. See the
`intensification_ladder.json` row in Configuration Files for the record shape and the shipped rungs.

## The build engine — THE seam both tracks call

`RungDef::build_accrual(policy, eligible)` / `build_decay()` / `yield_fraction_while_building()` are the
**single** source of a rung's build math. Both food webs call them instead of reaching for their own
bespoke accrue/decay/dip levers, so the two ladders **cannot drift apart numerically** — that is the
whole reason the dials moved out of `labor_config`/`fauna_config` and into the ladder.

- **`build_accrual`** returns `progress_per_turn × timescale` **only** when `policy` **is** the rung's
  own `verb` *and* the caller's rung-specific gates hold (`eligible` — knows the unlock knowledge,
  source healthy, species ceiling allows, faction owns it); otherwise `0`. **A rung with `verb: null`
  is never driven** — which is what keeps the two `wild` rungs (nothing to build) out of the engine.
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
  The dip: `forage::forage_policy_ceiling`'s `Cultivate` arm and `fauna::hunt_policy_ceiling`'s
  **`Tame`** and `Corral` arms — so **forecast == actual** for free (see "Pre-commit Yield
  Forecast"). **Extending** a pen (2d-β) reads the *same* `animal:pen` rung, so a ring can never drift
  from the initial build.

## The knowledge pattern — practise rung N, unlock rung N+1

**The one rule** (`docs/plan_intensification_ladder.md` §4, slice 4): **working a source teaches the
knowledge its *current rung* declares in `earns_knowledge`.** "Practising rung N" means *working a
source that stands on rung N* — **not** *"using rung N's verb"*. So the **same Sustain hunt** teaches
**Herding** on a wild herd and **Penning** on a tamed one: *you learn herding by managing wild herds,
penning by managing tamed ones*.

**`RungDef::knowledge_earned(policy, eligible)` is THE earn seam** — the twin of `build_accrual`: the
rung names the lesson, the caller credits the ledger. It replaced the two hard-coded per-web
`Sustain && Thriving → <ID>` branches, so `earns_knowledge` went from declarative (slice 2) to live,
for **every** rung including the wild ones. Callers resolve the rung via `fauna::herd_rung` /
`forage::patch_rung`, both read once per source in `advance_labor_allocation`'s Hunt/Forage arms
(**before** the arms branch, so every rung reaches the earn path uniformly).

Three rules ride the seam:
- **Only stewardship teaches** (§4.2) — `FollowPolicy::teaches_knowledge`, defined against the
  `EXTRACTIVE` grouping: **Sustain** teaches (the one extractive rung that only takes the regrowth)
  and so do the investment verbs (`Cultivate`/`Tame`/`Corral` — managing *is* the practice);
  **Surplus/Market/Eradicate teach nothing, at any rung** (they overdraw — slaughtering isn't
  practice).
- **You learn from a healthy source** — `eligible` is the `EcologyPhase::Thriving` gate both shipped
  earn sites already had, preserved unchanged.
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

## Behavior primitives — `movement` is live; `feeding`/`harvest` are still declarative

`behavior` is config over **coded** primitives (bounded enums): `movement` ∈ `fixed | roam |
drift_to_owner`, `feeding` ∈ `photosynthesis | forage | self_graze`, `harvest` ∈ `worker_take |
worker_tend | passive`. A rung that recombines existing primitives is pure config; a rung needing a
*new* primitive codes that one primitive once, after which it too is config.

- **`movement` IS READ** (slice 3b — the first primitive the engine applies): `fauna::advance_herds`
  resolves each herd's rung and dispatches on it, which is what makes §3's proximity spine
  (`roam` → `drift_to_owner` → `fixed`) a **config diff** rather than a code branch. `drift_to_owner`
  is the primitive slice 3b coded; see "Herd movement is a rung primitive" under Fauna & Wild Game for
  its ordering, its fallbacks, and the overgrazing tension it creates.
- **`feeding` / `harvest` are parsed and validated only** — the seam later slices switch on. `harvest:
  passive` is now **unused by every shipped rung**: retiring passive-free pastoral (§3, slice 3b) left
  no rung that pays without workers. The variant stays as vocabulary for a future rung that genuinely
  does.

## The config states TODAY's truth, deliberately

The whole thesis is that **later slices change behaviour by editing the JSON**, so the shipped file
describes the sim as it is, not as it will be:

| | rung 1 | rung 2 | rung 3 |
|---|---|---|---|
| **plant** | `wild` — no verb; **earns `cultivation`** | `tended` — verb **`cultivate`**, gate `cultivation`, **earns `seed_selection`** (slice 4) | `field` — verb **`sow`**, gate **`seed_selection`** (slice 5 — the consumer that knowledge was earned for), **`site_requirement` { min_forage_capacity 195, requires_fresh_water }** (the land must already be rich + watered — rung 3 moves seed, it cannot fertilize), earns nothing (`irrigation`/`rotation` = rung 4 Worked Land, parked); `movement: fixed` |
| **animal** | `wild` — no verb; **earns `herding`**; `movement: roam` | `pastoral` — verb **`tame`**, gate `herding`, ceiling `pastoral`, **earns `penning`** (slice 4); **`movement: drift_to_owner`, `harvest: worker_take`** (slice 3b — was `roam`/`passive`) | `pen` — verb **`corral`**, gate **`penning`** (slice 4 — was `herding`), ceiling `pen`, **earns `foddering`** (Flora Roster F3 — unlocks the pen's fodder-draw; `selective_breeding` = rung 4, still parked); `movement: fixed` |

Three consequences to keep straight, **all settled by slice 4**: `earns_knowledge` is **live, not
declarative** — every rung's lesson is read through `RungDef::knowledge_earned` off the rung the source
stands on, and the per-web `knowledge_progress_per_turn` copies that used to drive it are gone (see
"The knowledge pattern"); **one knowledge per transition** — Herding gates `tame` **only**, `penning`
gates `corral` + `extend_pen` (the §4.3 reshuffle; pinned by `builtin_ladder_describes_todays_rungs`,
which asserts no two rungs share an unlock gate); and **both the build dials *and* the knowledge dials
now live here**, so the two webs can only be tuned — and paced — together.

See Also: "Cultivation (Intensification Phase 1a)" (the plant rung 2), "Corral (Intensification Rung 1c)"
(the animal rung 3), "The husbandry yield ladder" (what each rung *pays*, which this arc does **not**
unify — animals pay flow-MSY against `r`, plants pay a flat rate without draw-down).

---

