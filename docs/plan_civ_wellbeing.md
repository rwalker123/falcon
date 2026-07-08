# Civilization Wellbeing: Morale, Discontent & Its Consequences

Status: **Phase 1 in progress** (this branch). Later phases deferred by design — the
architecture is built now so they slot in without a rewrite.

## Why this exists

A band's fate should depend on *many* civilization factors — not just terrain and food, but
(eventually) education, technology, government type, culture, leadership. Those factors converge
into a wellbeing signal (**morale**), morale produces **discontent**, and discontent drives
**consequences** — productivity loss, people relocating, and (later) revolution. This document
fixes the *architecture* so every future factor and consequence is an addition, never a rewrite.

Design stance (agreed):
- **Morale never kills.** Low morale relocates people or drags output — it does not cause faction
  population loss or death. Population loss stays with starvation / cold (see `core_sim/CLAUDE.md`
  Population & Demographics).
- **Births are morale-independent.** Contentment doesn't change procreation; births remain a
  demographic function of the fertility-age (working) bracket, food, and surplus.
- **People don't disappear** — a discontented worker either **migrates** to a better, reachable
  band of the same faction, or **stays** (and drags productivity). Nothing evaporates.
- **Every consequence is multi-factor** — discontent is *one* input to productivity, alongside
  future education/tech/government inputs.

## Architecture — three extensible layers

```
   FACTORS ─────────────►  MORALE  ────►  DISCONTENT  ────►  CONSEQUENCES
 (named contributors)    (0..1 scalar)   (fraction +        (productivity · migration
  terrain, climate,                       grievance state)    · [future] revolution)
  settling, unrest,
  [future: food,
  education, tech,
  government, …])
```

### Layer 1 — Factors (inputs → morale)
Morale trends toward an equilibrium set by the **sum of named contributions**. Each contributor is
`(id, signed value)`. Adding a factor = registering a contributor; the morale update never gets
rewritten.
- **Phase 1 (active):** the existing real inputs — `settling` (+ base growth), `terrain`
  habitability, `climate` (temperature vs the tolerance band), `unrest` (crisis + culture).
- **Reserved slots (unimplemented):** `nutrition`/food, `education`, `technology`, `government`,
  `crowding`, `culture`, `leadership`.
- **Free win:** this contributor list *is* the per-band **morale breakdown** — the UI can itemize
  "why morale is moving" directly from it (the transparency we'd otherwise bolt on separately).

### Layer 2 — Discontent (aggregate state)
Derived from morale, but a **first-class, accumulating state** — this is the seam that lets
revolution arrive later without restructuring.
- `discontent_fraction = g(morale)` — the share of the band that is unhappy, **weighted toward the
  working (fertility) bracket** (mobile, prospect-seeking); dependents (children/elders) are
  affected at a reduced fraction.
- `grievance` — a **severity × duration accumulator**: how bad, for how long. Phase 1 only *feeds*
  productivity + migration and is *read* nowhere else; it exists so that a later phase can trigger
  **revolution** on sustained, trapped, rock-bottom grievance without adding new plumbing.

### Layer 3 — Consequences (pluggable reactions to discontent)
Each keys off the discontent state independently; adding one doesn't touch the others.
- **Productivity — a modifier stack.** `output = base × Π(modifiers)`. Discontent is modifier #1;
  education / technology / government type are future modifiers. Applied to every economic yield
  (forage, hunt, follow, husbandry) at payout, using the band's current state. Productivity is
  multi-factor from day one — discontent is simply the first contributor.
- **Migration — tech-gated relocation.** Each turn the discontented seek the best **eligible** band
  of the same faction **within reach**, where *reach scales with the civilization's movement /
  transport technology* (early bands are stuck close; advanced ones send people far). Eligible = a
  band meaningfully happier than a bare threshold. Found → they **relocate** (their band shrinks,
  the destination grows — a high-morale band is a population *magnet*). None reachable → they
  **stay**, still discontented, still dragging productivity. Population is conserved within the
  faction.
- **Revolution — deferred.** Sustained, trapped, rock-bottom `grievance` → an uprising far more
  serious than productivity loss (loss of control / band schism / etc.). Trigger and effect are
  **not** implemented in Phase 1; the `grievance` state they read is.

## Phase plan

**Phase 1 (this branch) — the spine + the first factor/consequences.**
- Morale computed through the Layer-1 **contributor structure** populated with today's real inputs.
- Discontent fraction (working-weighted) + the `grievance` accumulator (populated, feeds only
  productivity + migration).
- Productivity **modifier stack** with discontent as the sole current entry.
- Tech-gated **migration** (relocate-or-stay; conserved within faction).
- Births decoupled from morale; **no** morale-driven faction population loss.
- Client: productivity readout, morale breakdown / recovery guidance, action hints.
- Extension points present and **empty**: extra factors, extra productivity modifiers, revolution.

**Phase 2+ (deferred, no rewrite required).**
- Additional morale factors: nutrition, education, technology, government type, culture.
- Additional productivity modifiers (education/tech/government).
- **Revolution** consequence off the `grievance` accumulator.
- Richer emigrant destinations (settlements, cross-faction), and reach tied to concrete movement
  tech tiers.

## See Also
- `core_sim/CLAUDE.md` — Population & Demographics (births/deaths, the morale computation this
  generalizes), Fauna & Wild Game (follow/hunt yields the productivity modifier scales).
- `clients/godot_thin_client/CLAUDE.md` — band readouts (morale, habitability, the new productivity
  + recovery UI).
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — the manual; wellbeing/discontent as a
  player-facing system.
