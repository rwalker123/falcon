# Investment rungs are a second axis, not a sixth stance

**Issue #442.** Decision + design note for the interaction model, the wire representation of
`policy`, and the compose sheet's preparing→then forecast.

**Decision: make the investment rungs a parallel toggle.** A labor assignment carries two
independent facts — *how hard am I pulling on this source* and *what am I building on it* — and
today both are crammed into one `policy` field.

---

## 1. Why, in one artifact

The complaint in the issue is about reading: committing to an improvement makes your harvest stance
read as un-chosen, and on completion the UI tells you to go back to a stance you never meant to
leave. That is real, but the decisive evidence is in the sim:

```rust
// core_sim/src/systems/labor.rs
const HARVEST_POLICY_AFTER_BUILD: FollowPolicy = FollowPolicy::Sustain;
```

Ten call sites, and its own doc comment states the reason: once a build completes "the assignment is
stranded on a dead rung", so completion "retires the build verb" onto a harvest rung. **The sim
silently rewrites the player's stated policy** at a turn the player cannot predict. That is not a
feature of the ladder; it is the cost of the overload. Give the build verb its own slot and the
constant has nothing left to do.

The sim has already half-admitted the split. `FollowPolicy::EXTRACTIVE` is the stance set and
`is_investment()` is defined as *the complement of it* — with a doc comment recording that
hand-written lists of the build verbs "rot" and that two had already rotted before the predicate was
factored out. This change makes that conceptual split structural.

## 2. The model

| Axis | Question | Type | Values |
|---|---|---|---|
| **Stance** | how hard do I pull? | one-of (radio) | `Sustain` · `Surplus` · `Deplete` · `Eradicate` |
| **Improvement** | what am I building? | optional, at most one | plant: `Cultivate` → `Sow` · animal: `Tame` → `Corral` |

**At most one improvement is ever in flight, and it is always the source's next rung**, because the
rungs are strictly ordered: you cannot Sow ground you have not tended, and a tended patch has
nothing left to cultivate. So the improvement control is a single checkbox naming the next rung —
not a second radio group — and the picker does not grow.

### 2.1 A non-Sustain stance IS allowed while building

The player may hold any stance while an improvement runs. Deplete-while-cultivating is sayable, and
it **defeats itself through the ecology rather than through a gate**: the build meter accrues only
while the source is Thriving, and Deplete is what drives it out of Thriving. The consequence is
already implemented on both webs, and the vocabulary for saying so already exists
(`_tame_stalled_hint` states exactly this on the animal side).

Forbidding it was the alternative. Rejected: it re-creates in the UI the very coupling this change
removes from the model, and it needs a new gate to enforce something the ecology already enforces.

### 2.2 The dip generalises

Today the during-build yield is `yield_fraction_while_building × the **Sustain** (MSY) ceiling`
(`components.rs`), and Sustain is hardcoded because it is the only stance a builder can be in.

**Now: `yield_fraction_while_building × the SELECTED STANCE's ceiling.`** The same formula with the
constant removed. This is what makes §2.1 self-punishing without a rule: a Deplete builder takes
more now, leaves Thriving sooner, and stalls their own meter.

## 3. The compose sheet

**Stance row** — four rungs, radio, unchanged in appearance. It never moves on its own.

**Improvement row** — one of three states, in this order of precedence:

1. **Offered** — an unchecked checkbox naming the next rung and its terms
   (`🌱 Cultivate this patch · ~25 turns · then 1.20 food/turn`). Gate reasons render beneath it
   exactly as they do on a rung today; a gated improvement is shown, unchecked, and explained.
2. **Running** — checked, with the meter and a remaining-turns estimate
   (`🌱 Cultivating — 60% · ~10 turns left`). If the source has left Thriving, a WARN line states
   the pause, its cause and the ease-off remedy.
3. **Done** — a static state label (`🌾 Tended Patch`), with the *next* rung's checkbox beneath it
   if there is one. Nothing to uncheck, nothing to clear.

**The forecast states the whole deal**, because the baseline is now still on screen:

```
today      Preparing: +0.24 /turn → then +1.20 /turn
proposed   +0.96 → +0.24 while building → +1.20 /turn
```

The `+0.96` is the number today's line cannot show — it is what the investment *costs*, and it was
missing precisely because the stance had been vacated. Render the middle term WARN-amber; it is the
dip.

## 4. Both webs, one grammar

| Slot | Plant | Animal |
|---|---|---|
| Stance | Sustain · Surplus · Deplete · Eradicate | *identical* |
| Improvement | `Cultivate` → `Sow` | `Tame` → `Corral` |
| Done label | 🌾 Tended Patch → ▦ Field | ◎ Pastoral → 🐄 Penned |
| Pauses when | the patch leaves Thriving | the herd leaves Thriving |
| Running cost | none | **the pen eats fodder every turn** |

The last row is the only asymmetry and it is **pre-existing** — a penned herd cannot graze, so
someone feeds it. It rides the Corral done-state label because a standing obligation belongs with
the standing state, and it is the one place the two webs must not be made to match.

`Sow` keeps its existing asymmetry too: it places a Field even where no patch existed (seed
travels), so on sowable wild ground BOTH `Cultivate` and `Sow` can be offered. The
"one checkbox, the next rung" rule resolves this the way `RungGates.next_rung_ready` already does —
**highest rung first**, so such ground offers `Sow`.

## 5. The wire

| Field | Today | Proposed |
|---|---|---|
| `LaborAssignment.policy` | the stance *or* the build verb | the stance, always — and never rewritten by the sim |
| `LaborAssignment.improvement` | — | appended: `""` \| `cultivate` \| `sow` \| `tame` \| `corral` |
| per-policy ceiling rows | six — four stances + two build dips | **four**; the dip is derived per §2.2 |

`FollowPolicy` narrows to the four extractive rungs. The four build verbs become their own type
(`Improvement`), which deletes `is_investment()`, `EXTRACTIVE`, and the "complement of" reasoning —
a set-membership predicate is not needed once the sets are different types.

**No migration burden.** Nothing is shipped, so an assignment carrying `policy: "cultivate"` never
has to be read back (root `CLAUDE.md` → no back-compat).

### Command grammar

`assign_labor <faction> <band> <kind> <target…> [<stance>] <workers>` keeps its shape; the stance
token narrows to the four. The improvement is set by its own verb — the server already exposes
`tame` / `sow` convenience verbs, and they generalise to the whole set rather than being bypassed
as they are today.

## 6. What this deletes

Each item exists *only* to undo the overload; none is doing work the game needs.

- **`HARVEST_POLICY_AFTER_BUILD` and its ten call sites** — §1.
- **The selected-and-gated rung state (#420)** — a whole rendering mode (keeping a disabled button's
  selected hue) that exists because a completed build leaves the picker on a dead rung. A checkbox
  that becomes a label cannot be in that state.
- **The re-staffing gap** — `validate_labor_policy` rejects a Cultivate assign on a non-Thriving
  patch, and changing the crew re-issues exactly that command, so today the crew of a *paused* build
  cannot be changed. Crew size becomes a stance-side edit that never re-asserts the improvement.
- **`INVESTMENT_POLICIES`, `forecast["investment"]`, `FORECAST_PAYOFF_KEYS`-as-a-rung-test** — every
  client surface that re-splits one field into "is this a build or a stance?" is answering a
  question the wire should have answered.

## 7. Sequencing

One PR. A half-migrated wire — `policy` narrowed on one side but not the other — is worse than
either end state, and the deletions in §6 are what keep the change from being a net addition.

Sim/schema half first (`server-dev`), client half consuming it (`client-dev`).
