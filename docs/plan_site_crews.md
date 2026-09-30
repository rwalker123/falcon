# Plan: Site Crews — a site's own crew keeps it

**Status:** design, decided with the maintainer (issue #751). It replaces #718's per-pool tool
allocation, which a prototype showed to be micromanagement the priority system exists to avoid.

**It amends** `docs/plan_standing_upkeep.md` §2.2, §2.5 and §2.9 (keeping as band pools, the fund
mode, the shedding order) and `docs/plan_pool_toe.md` (the per-pool TOE and its settlement). Where
those documents and this one disagree, this one is current.

## 0. The change, in one line

**A site has one crew, and that crew keeps the site before it collects from it.** The Agriculture,
Husbandry and Quarrywork keeping pools retire. The **Builders** and **Roadwork** pools stay.

---

## 1. Why the pools can go

`plan_standing_upkeep.md` §2.5 moved keeping off the tile for one reason: a separate keeper crew on a
site has to round a fractional demand up to whole workers and **wastes the rest**. A pool has no
leftover by construction.

**One crew that both keeps and collects wastes nothing either.** Whatever the keeping does not need
is collected. So the pool's only advantage is gone, and what it cost stays:

- the player staffs a site in two places (its row, and a pool that serves it at a share they cannot
  set);
- a pool needs a fund mode (Spread / Priority) to decide whose keeping falls short;
- a pool needs its own tool settlement (`plan_pool_toe.md`), its own idle-keeper readout and its own
  shed steps.

**Roads are the exception, and they stay a pool.** A road collects nothing, so a crew on one road
tile has no second use for its leftover. The better answer is to treat a road as a **range** rather
than a hex — one worker able to keep half a road tile should cover two — but how is undecided and out
of scope here. Until then Roadwork keeps its pool and its fund mode.

---

## 2. The model

### 2.1 Keep first, then collect

A site's crew is its existing **take row** — `forage` on a patch, `hunt` on a herd, `extract` on a
working. Each turn:

```text
keep_rate    = work one of this crew's hands delivers keeping, with the tools it was issued (§2.3)
keep_hands   = min(crew, upkeep_demand / keep_rate)        // fractional
take_hands   = crew − keep_hands                           // fractional
kept         = keep_hands × keep_rate
take         = the source's ordinary take, run on take_hands
```

- **A crew short of the demand keeps what it can and collects nothing.** The shortfall rots the site
  at the rung's own rate after the grace, exactly as a short pool did (`plan_standing_upkeep.md`
  §2.4). The row shows `⚠`.
- **A site with 0 crew is not kept at all.** That includes a half-built meter: a Cultivate queued on
  a patch nobody works rots while the builders raise it. The row's `⚠` is what says so, and adding
  a hand is the remedy.
- **It is not the retired `maintain` trap.** That crew also kept first, but the old test was binary,
  so 2 hands on a 5-work pen bought nothing. Rot is now `(shortfall / demand) × rate`, so they slow
  the decay by two-fifths, and the row says `keeps 2 of 5 ⚠` beside the stepper that fixes it.
- **The totals do not move.** A Field owes 4 work a turn, so 4 bare hands hold it and harvest
  nothing. Under the pool the same band paid the same 4 hands out of Agriculture. The cost is
  unchanged; the row now states it.

> ⛔ **THE TAKE RUNS ON FRACTIONAL HANDS.** `forage_take`, `hunt_take` and `deposit_take` take a
> whole-number crew today. Rounding `keep_hands` up to whole workers to feed them would bring back
> the exact waste §1 removes, so each take accepts a fractional hand count. All three are linear in
> hands. `hunt_take`'s fight is resolved per party, not per hunter, so a fractional engage is a
> rate and not a fraction of a person.

### 2.2 Tamed herds and pens

**The hunt row on a herd is its crew.** There is no separate cull row today, and none is added. The
row pays the herd's keeping first (head count ÷ `animals_per_herder`), then culls with the rest.

- **A pen changes the bill, not who pays it.** The pen's hurdles upkeep is claimed at the row's
  Priority; its longer grace and lower escape rate apply as today; hay feed (`settle_pen_hay`) is
  untouched.
- **A herd whose crew is short sheds**, by the same `shed_uncontained_animals` rule a short
  Husbandry pool triggered.
- **A herd part-way through a Tame is kept from the first work banked**, by its row's crew. The
  unkept-animal-build stall (`plan_standing_upkeep.md` §6) is then answered on the row that causes
  it: the remedy is the crew stepper beside the `⚠`.

### 2.3 Tools

**Keeping tools and take kits never overlap.** Keeping tools (hoes, crook, stone-dressing,
earthmoving) raise **work** through `build_work`; take kits (baskets, spears, sled, wedges, axe on its
take side) raise the **take**. `EquipmentConfig::validate` already refuses an item with `build_work`
in a take kit.

So a site's requirement is **two independent lines**:

| Line | Who carries it | How it is filled |
|---|---|---|
| **take kit** | the take hands | the player's pick, through the existing per-head item budget — unchanged |
| **keeping tools** | the keep hands | the tools serving the site's own branch at its own rung, settled by Priority |

**The order, which has no loop:**

1. **Plan.** `keep_hands` as if equipped, at the rate the band's own ledger allows with no coverage
   (the existing `fully_equipped_keeper_rate` rule).
2. **Claim.** The site asks for one unit of each keeping tool per planned keep hand.
3. **Settle** each tool band-wide by Priority (§2.4).
4. **Resolve.** The site's `keep_rate` is the coverage-weighted rate of what it got, and §2.1 runs
   on it. A site short of tools spends **more of its own hands** keeping and collects less.

**The pool's step 5 (bare top-up) retires.** It existed to put idle pool keepers on a site's unmet
deficit. A site crew covers its own deficit first by construction.

### 2.4 Two priority marks per row

| Mark | Shown | Ranks |
|---|---|---|
| **Priority** | always | the site crew's claim on scarce tools and materials |
| **Build** | only while a build is queued on that site | that build's claim on scarce tools and materials |

- **A build no longer borrows its site row's Priority** (fixes #719). The Build mark is stored on the
  queue entry and defaults to Normal.
- **The build queue shows each entry's Build mark read-only.** It is set on the site's row.
- **Settlement:** High, then Normal, then Low. Inside a tier, site crews are served before the build
  — the existing keep-before-build rule (`plan_pool_toe.md` §2.2). A Roadwork claim ranks at its
  road's Priority as today.
- **There is no per-pool or per-tool player allocation.** The two marks are the only levers.

### 2.5 Groundwork sites

A working is kept by its own `extract` crew, the same shape as a patch. The free floors
(`extraction:gathering`, `forestry:deadfall`) owe no upkeep, like `plant:wild`, so their whole crew
collects.

### 2.6 What a band shrinking sheds (replaces `plan_standing_upkeep.md` §2.9's keeper steps)

Keeping hands now sit **inside** site rows, so the order splits a row at its keeping line:

| Step | Before | After |
|---|---|---|
| 3 | a keeper above demand — Agriculture, Husbandry, Roadwork, Quarrywork | a **Roadwork** keeper above demand |
| 5 | thin the least-productive source with two or more hands | the same, but only a hand **above that site's keeping need** — thinning never causes rot |
| 8 | a keeper below demand — Agriculture, Husbandry, Roadwork | a Roadwork keeper below demand, **then** a site hand below its keeping need, least-productive improved site first |

Every other step is unchanged. "Least productive" keeps its two levels (pays anything at all, then
food per worker), read off the row's realized take — which is already net of the hands it spent
keeping.

---

## 3. On screen — the Work tab

**Prototype (direction, not pixels):** https://claude.ai/artifact/SSnyqDZ93HpBS5xPdg7pnn. Where it
shows roads with their own crews, this document wins: Roadwork is a pool.

- **One spinner per site.** The row's crew stepper is the only staffing control for that site. Its
  second line says where the work goes: `Tended patch · keeps 2 of 2 · 1 harvesting`.
- **Marks on the row:** `⚠` when the crew keeps less than the demand, with the work-units sentence in
  the hover; `ⓘ More tools would speed this up.` when the keeping tools are short but the work is
  covered.
- **Sections**, each collapsible, header reading `N on work`:
  - **BUILD QUEUE** — a `Builders` line with the pool spinner, then the queue with read-only Build
    marks;
  - **AGRICULTURE** — every harvest row;
  - **HUSBANDRY** — every hunt row;
  - **ROADWORK** — its pool line with the Spread/Priority pill (#721), then the roads;
  - **GROUNDWORK** — every extract row.
- **The Gathering filter chips go.** The sections are the filter.
- **The Agriculture, Husbandry and Groundwork pool cards go**, and with them the idle-keeper line and
  the pool TOE hover (#724's ask is answered by the row's `⚠` / `ⓘ`).

---

## 4. On the wire and in commands

**Retire:**

- `PopulationCohortState.poolCrew` and `poolToe` lines for `agriculture`, `husbandry` and
  `quarrywork`. Roadwork and builders keep theirs.
- `quarryworkDemand` / `quarryworkSupplied` / `quarryworkShortfall`. (Agriculture and Husbandry never
  had such a triple.)
- `assign_labor … agriculture|husbandry|quarrywork <workers>` and the matching `LaborTarget`
  variants.
- `upkeep_mode` stays, and now governs Roadwork only.

FlatBuffers fields are positional, so a retired field stays in the schema and publishes empty or
zero, as `plan_pool_toe.md` §4 did for the pool kit ids. No fallback code.

**Keep:** each source's `upkeepDemand` / `upkeepSupplied` / `upkeepShortfall` / `upkeepWorkersNeeded`.
`upkeepSupplied` now means *what this site's own crew kept*.

**Append:**

- per source (patch, herd, working): `upkeepHands:float` — the crew's hands spent keeping, which the
  row's second line reads; and `upkeepToolsShort:bool` for the `ⓘ`;
- per build queue entry: `buildPriority` (`high` / `normal` / `low`);
- command `build_priority <faction_id> <band_id> <x> <y> high|normal|low | build_priority
  <faction_id> <band_id> <herd_id> high|normal|low`, the same shape as `work_priority`.

**`sim_ai`** sizes the `agriculture` pool today from the patches' upkeep readouts
(`specialists/food/rules.rs`). It moves to sizing each harvest and hunt row's crew to cover keeping
plus take, or AI bands stop keeping their improvements.

---

## 5. Slices

1. **Sim.** Keep-first split per site (§2.1); fractional take hands; the per-site tool claim and
   settlement (§2.3); `BuildQueueEntry` priority and its claim (§2.4); retire the three pools and
   their claims; the shedding steps (§2.6). Proves: a Field's crew of 4 bare hands keeps it and
   harvests nothing; a crew of 5 keeps it and harvests one hand's worth; a site with 0 crew rots; a
   tool-short site keeps with more hands and collects less; a `Low` build loses a hoe to a `Normal`
   site and a `High` build wins one; a shrinking band thins above the keeping line before cutting
   into it; Roadwork is bit-identical to today.
2. **Wire and commands.** §4 end to end — schema, state, snapshot, runtime payloads, command text,
   server handlers, `command_guard` drives; golden re-recorded; decode guard.
3. **Client.** The Work tab sections, the row's second line and marks, the Build mark, the removed
   pool cards and filter chips; harness claims for a kept, a short and a tool-short row.
4. **AI.** `sim_ai` crew sizing.

**Rule files move with the code they describe**, in the slice that changes it, because they state
what is built. The ones that describe the keeping pools as built: core_sim `intensification.md`,
`cultivation.md`, `husbandry.md`, `extraction.md`, `equipment.md`, `routes.md`, `fauna.md`,
`graze.md`, `yield-forecast.md`, `ai-driver.md`; client `band-city-panel.md`, `labor-ui.md`,
`extraction-workings.md`, `roads.md`, `selection-card.md`, `turn-orb.md`, `herd-readouts.md`,
`native-extension.md`, and the band-panel and ui-preview harness docs. The `extraction-workings.md`
routing row in both client hub copies names the Groundwork pool and changes with it.

Closes #719 and #724. #721's pill survives on Roadwork only.

---

## See Also

- `docs/plan_standing_upkeep.md` — the keeping model this moves onto the site; §2.4's rot, grace and
  retention bar are unchanged
- `docs/plan_pool_toe.md` — the settlement this narrows to Roadwork and Builders
- `docs/plan_unit_costed_work.md` — why gear changes work per turn, never the work a job requires
- `.claude/rules/core_sim/husbandry.md` — the shed and the pen, which a site crew now triggers
