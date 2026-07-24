# Neglect Sheds Animals, It Does Not Un-Tame Them

**Status:** arc open (worktree `fauna-escape`, branch `worktree-fauna-escape`). Fauna-only. Sits on
the intensification ladder (`docs/plan_intensification_ladder.md`), the pen economy
(`docs/plan_grazing_2d.md`), and the corral managed-population arc
(`docs/plan_corral_managed_population.md`). The flora side is **out of scope** — see §7.

**Scope decision:** this arc changes what *neglect* costs on the animal web. It **retires the
tameness-bleed** (`decay_under_herded`) and replaces it — and the binary corral escape — with **one
shared "animals leave" mechanic**: an under-contained managed herd sheds whole animals over its
labor capacity into a nearby **wild** herd of the same species. Tameness (`domestication_progress`)
**never decays from neglect** again. Two config tunings on one code path: pastoral (no fence) sheds
faster, a pen (fence) slower.

---

## 1. The problem this fixes

Playtest (Ray, the domesticated Rabbit Warren) surfaced three stacked failures, all traced to the
same root: **neglect attacks the wrong axis.**

1. **The requirement grows silently as the herd grows.** `herders_needed =
   ceil(animals / animals_per_herder)` (`fauna.rs:977`) is recomputed from live biomass every turn.
   A herd you had fully staffed at 4/4 becomes 4/5 **on its own** the turn it crosses an
   `animals_per_herder` multiple — nothing the player did.
2. **Under-herding bleeds *tameness*, invisibly, and the badge chatters.** `decay_under_herded`
   (`fauna.rs:2662`) decays `domestication_progress` proportional to the staffing shortfall, for both
   pastoral (`fauna.rs:2610`) and penned (`fauna.rs:2607`) herds. The "Domesticated" flag flips at
   **exactly 1.0 with no hysteresis** (`is_domesticated`, `fauna.rs:485`), so the first under-herded
   turn drops the herd below 1.0 and it reads as *wild* again — even though the herder *count* has a
   0.25 deadband. Re-staffing **stops the bleed but does not restore the lost progress** (no passive
   re-accrual exists); the player must switch back to `Tame` and re-earn it. That is the "I have to
   redo the taming, endlessly" the playtest reported. **None of it is surfaced** — no command-feed
   line, no Telling beat; the only witness is the selected-herd drawer.
3. **The structural labor-pool trap.** `herders_needed` is uncapped and grows with the herd, but
   herders come from the band's finite working-age pool (`available_workers`, `components.rs:689`;
   `set_assignment` clamps to headroom, `components.rs`). **Nothing ties the requirement to band
   size.** A herd that grows past what the band can staff **cannot be fully herded, ever** — its
   tameness bleeds every turn with no in-band remedy. This is the genuinely unwinnable case.

The design verdict from the trace: the sim math (decay, hysteresis, the one-turn Logistics-reads-
Population lag) is internally correct, but it is expressing the cost of neglect on an **invisible,
hard-to-recover stat** and can strand the player. The fix is not "add hysteresis to the flag" — it
is to **move the cost onto the visible axis (herd size)** and make tameness permanent capital.

### 1a. Why "shed animals," and why it dissolves the whole knot

- **The consequence becomes legible.** A shrinking flock is a number the player sees (biomass, food
  yield); an invisible tameness stat is not.
- **The trap disappears.** A herd too big to staff **self-corrects to a size the band can hold**
  instead of bleeding a stat forever. The unwinnable state cannot form.
- **The "redo the taming" loop disappears.** Tameness is never lost to neglect, so the badge never
  chatters and there is nothing to re-earn. **This makes the flag-hysteresis question moot on the
  fauna side** — there is no longer a decay pushing `domestication_progress` across 1.0.
- **It unifies with what the code already believes.** The corral already reverts an untended herd
  *without destroying its domestication* (`untended_corral_escapes_to_mobile` asserts
  `domestication_progress > 0.9`). This arc extends that principle — *insufficient containment →
  animals leave, tameness intact* — to the pastoral rung and expresses it as a **transfer to the
  wild web**, not a mere un-pen.
- **It creates a real equilibrium.** A herd settles at `min(ecological K, labor capacity)`. Pastoral
  output becomes **labor-gated** — grow the band to hold a bigger flock, split the herd across tiles,
  or staff fewer herders and let it shrink to free workers. That is the
  [[scarcity-drives-the-real-decision]] pillar, not a babysitting chore.

**The one honest tradeoff, accepted:** biomass *is* food, so shedding animals hits survival more
directly than an invisible stat did. But it is visible, avoidable (staff the herders) and recoverable
(re-hunt or re-tame the feral herd), so the stakes are a feature, not a regression.

---

## 2. The settled model

### 2.1 Tameness leaves with the animals that leave

`domestication_progress` is **never decayed as a stat by neglect**. `decay_under_herded` and every
call site (`fauna.rs:2607`, `:2610`) are **deleted**. Tameness is not a meter that erodes — it is a
property of the animals, and it is lost **only by the animals that leave the herd**:

- **Tameness** = a property of the lineage you earned once. Neglect does not un-teach the animals
  that stay.
- **Herders** = a *throughput cap* on how many head you can hold — "how big a tame flock can I
  keep?", not "am I stopping my animals forgetting they're tame?".
- **Surplus over that cap goes feral** — the overflow *leaves* (to an adjacent hex, roaming, wild).
  The leavers lose tame because they left; the stayers keep it. **Tameness is carried out per-animal
  by the shed** (§2.2), never reset by an event on the herd.

> **What still lowers a herd's tameness?** Nothing, as a stat — there is no meter reset anywhere. A
> herd only ever *loses animals* (which take their tameness to the wild web) or *keeps* them. This is
> exactly consistent between the two rungs: a pastoral herd and a pen both bleed tame animals out to
> the wild when under-contained, and the difference is only how fast (§2.2) and whether the herd is
> pinned (a pen stays put, a pastoral herd drifts near its band, a wild herd roams). The `owner` clear
> at `progress == 0` (`fauna.rs:544`) is removed with `decay_under_herded`; ownership now ends only
> when a herd bleeds out entirely and the empty entity despawns (§2.4).

### 2.2 One shared "animals leave" mechanic

A new `shed_uncontained_animals` (working name) replaces both `decay_under_herded` **and** the binary
corral-untended escape (`fauna.rs:2613-2679`). Each turn, in `advance_husbandry` (Logistics), for
every managed herd (`is_corralled() || owner.is_some()`):

```
capacity_animals = assigned_herders × animals_per_herder      // what your labor can hold
current_animals  = biomass / body_mass
overage          = max(0, current_animals − capacity_animals)
if overage < 1: do nothing                                    // self-limiting attractor
leaving = quantise(escape_fraction × overage)                 // whole animals, seeded RNG, ≥1 floor
move (leaving × body_mass) of biomass from this herd → a nearby WILD herd of the same species
```

- **In whole animals, not raw biomass** (Ray's call). The shed count is `escape_fraction × overage`
  rounded to whole animals, with a **min-1 floor when `overage ≥ 1`** so a small overage actually
  clears instead of asymptoting one or two over forever.
- **A fraction of the OVERAGE, not of the total** — so as the herd shrinks toward capacity, fewer
  leave each turn; it approaches capacity and **stops exactly** when `overage < 1`. No overshoot to
  zero (unless capacity itself is 0 — total abandonment, §2.4).
- **Randomized for playability, via the sim's seeded RNG** (§3.1). Not wall-clock `rand`.
- **Two tunings, one code path** (Ray's call): pastoral uses `pastoral_escape_fraction` (faster,
  no fence); a pen uses `pen_escape_fraction` (slower — the fence buys time). Same function, the
  rate is the only difference.

### 2.3 Where the animals go: a nearby wild herd (visible, not vaporized)

The escapees **leak out to an adjacent tile and join the wild web** — the player *sees* the feral
population appear, and it re-enters play (huntable, re-tameable). This is Ray's "the game is live,
nothing silently drifted off into the air." The placement rule, in order:

1. **Merge over the tile + adjacent ring.** If a wild herd of the same species sits on the managed
   herd's tile *or* any adjacent hex, the escapees **merge into the nearest one** (add
   `leaving × body_mass` to its biomass). This reinforces an existing wild population instead of
   proliferating herds, which also sidesteps the `abundance.max_total_game` cap (§5, item 2).
2. **Else spawn a new wild herd on an adjacent land tile** — the drift-out target, chosen with
   `advance_herds`' existing land-neighbour picker (`acceptable_steps` — valid land, not barren,
   in-bounds, wrap-aware), so no new placement convention is invented.
3. **Fall back to the herd's own tile** only if hemmed in (every neighbour water/barren/off-map), so
   the shed never fails to place.

**Adjacent, not same-tile, is deliberate.** Small game is *stationary* (`route_len == 1`, e.g. the
Rabbit Warren — the playtest case), so a same-tile spawn would park a wild warren permanently on top
of the tame one (two warrens stacked on one tile in the inspector — the confusing readout this arc
exists to remove). Leaking to an adjacent tile reads as *what happened* and keeps the two groups
distinct. Same-tile survives only as the hemmed-in fallback.

Whether it merges or spawns, the receiving wild herd carries **wild** husbandry state
(`owner = None`, `domestication_progress = 0`, `corralled_at = None`) — the escapees are a fresh wild
group, whatever their origin stock.

### 2.4 Total abandonment goes fully feral (both rungs)

**Zero herders/keeper ⇒ `herded_fraction == 0` ⇒ shortfall 1 ⇒ the whole flock sheds.** No special
"escape" branch is needed; abandonment is just the `herded_fraction == 0` limit of the same mechanic.
But shedding only reaches zero if the managed herd's own regrowth doesn't refill it — so
**a fully-abandoned herd's regrowth is suppressed** (the mechanism an untended pen already uses):

- **Pen** — an untended pen already does not regrow (`pen_fed_fraction = NOT_FED`), so it sheds to
  the extinction floor over several turns (pen rate — the fence slows it).
- **Pastoral (unfenced)** — a herd with **zero herders** (`herded_fraction == 0`) likewise has its
  regrowth suppressed, so it too sheds all the way down. Without this it would regrow into the shed
  and settle at an equilibrium (~0.6·K), persisting as a shrunken tame herd that leaks strays every
  turn forever — rejected: a herd with literally nobody minding it should not stay "yours" in
  perpetuity. **Full abandonment goes fully feral on both rungs; this is deliberate symmetry.**

**Partial neglect is different, and stays tame.** A herd with *some* herders (`herded_fraction > 0`,
i.e. understaffed but not abandoned) keeps normal regrowth, so it sheds down to and **holds at its
labor-supported capacity** — a stable smaller *tame* herd, `owner` intact. That equilibrium is the
whole point of §1a: you either grow the band to hold a bigger flock or accept a smaller one; you do
not lose the herd for being one herder short.

**The herd bleeds out entirely, then despawns — no remnant, no meter reset.** A fully-abandoned herd
keeps shedding animals to the wild every turn until it is **empty** (below one animal), at which point
the empty entity **despawns**. Crucially it **stays owned/managed while it bleeds** — ownership is
*not* cleared at the extinction floor — so it keeps shedding at its rung's rate all the way down
instead of stopping at the floor with an ownerless-but-tame husk. The tameness is not reset; it walks
off with the animals (§2.1). When the entity despawns, all its former biomass is already in wild herds
(the shed placed it there turn by turn), so nothing is lost.

- **A pen** pushes its "the herd has drifted off — the pen is lost" feed line when it despawns
  (preserving the current non-silence of pen destruction; the fence goes with the despawned entity —
  no separate `corralled_at`/fence reset is needed once the herd is gone).
- This makes **Tame and Corral identical on abandonment**: both bleed out to the wild and vanish; the
  only difference is the pen bleeds slower (`pen_escape_fraction`) and is pinned while it does.

This still requires a *shed* (leaving animals) to empty the herd, so a pen a keeper merely *starves*
to the extinction floor (feed path, §2.5, no animals leaving) keeps its pen and recovers when fed —
starvation and abandonment stay distinct.

> **This is consistent with "tameness is permanent" (§2.1), not an exception to it.** The herd never
> *forgets*. It loses its *animals* to the wild web one shed at a time — each leaving group becomes a
> fresh wild herd (`domestication_progress = 0`) — until there is no herd left. Ownership ends because
> there are no animals left to own, never because a tameness stat decayed.

### 2.5 What is NOT changed

- **The pen's feed-starvation path stays.** `starve_underfed_pen` (`fauna.rs:2601`) is a *feed*
  mechanic (can the keeper pay the larder bill?), orthogonal to *herding* (are there enough hands?).
  Both can apply to a pen in one turn. Unchanged.
- **`herders_needed` and its hysteresis stay.** It is now the **capacity-gate display** ("what
  you'd need to hold them all") and feeds `capacity_animals`. The 0.25 deadband
  (`stabilize_herders_needed`, `fauna.rs:671`) still damps ±1 flicker. Since tameness no longer
  chatters, the deadband is no longer load-bearing for tameness — but it keeps the readout steady, so
  it stays.
- **The one-turn Logistics-reads-Population lag stays.** `advance_husbandry` reads last turn's
  staffing (`herded_fraction`, written by the labor arm) before the current turn's assignment
  applies. Under the old model that guaranteed one forced turn of *tameness* decay per boundary
  crossing; under the new model it means at most one turn of *shedding* before the player's added
  herder takes effect — a far gentler, self-correcting, and now *visible* consequence.

---

## 3. Implementation subtleties (settle before coding)

### 3.1 Seeded RNG — determinism is a hard constraint

The sim is deterministic under rollback (replays, multi-session, the whole snapshot round-trip
depend on it). "Randomize the shed" **must** draw from the world seed stream, exactly like
`advance_herds`' per-herd movement RNG (`map_seed ^ tick ^ SALT ^ fnv(herd.id)`, see
`core_sim/CLAUDE.md` → "Movement is deterministic under rollback"). A wall-clock `rand()` would break
determinism. Same playability benefit, zero correctness cost. Pin with a two-run bit-identical test.

### 3.2 The shortfall is `herded_fraction` — no need to reconstruct the assigned count

The capacity framing in §2.2 is the *conceptual* model; the implementation is simpler. `herded_fraction`
(`= min(1, assigned/needed)`, the same field `decay_under_herded` read as `herded_last_turn`) **is the
staffing shortfall already normalized**: `overage/current_animals ≈ (1 − herded_fraction)`, because
`current_animals ≈ needed × animals_per_herder` and `capacity ≈ assigned × animals_per_herder`. So the
shed reduces to:

```
overage_animals  = max(0, 1 − herded_last_turn) × current_animals
leaving_animals  = quantise(escape_fraction × overage_animals)   // ≥1 floor when overage ≥ 1
```

No `herders_needed`, no assigned-count reconstruction, no new persisted state. A fully-staffed herd
reads `herded_last_turn ≥ 1` ⇒ overage 0 ⇒ no shed. **Total abandonment falls out for free**: a pen
nobody works has `herded_fraction` reset to `NOT_HERDED` (0.0) ⇒ shortfall 1 ⇒ the whole flock sheds
(§2.4), so the shed needs no separate "untended" branch — it is the `herded_fraction == 0` limit of
the same formula.

### 3.3 Whole-animal quantization floor

`quantise(escape_fraction × overage)` rounds down; without a floor a shrinking overage rounds to 0
and stalls one or two animals over capacity forever. Rule: **if `overage ≥ 1`, at least 1 animal
leaves.** Name the `1` as a constant (`MIN_ESCAPE_ANIMALS` or similar) per [[no-magic-numbers]].

### 3.4 Config levers (all named, per [[no-magic-numbers]])

New, in `fauna_config.json` `husbandry` (the neglect economy's home):

- `pastoral_escape_fraction` — fraction of the overage that leaves a pastoral herd per turn.
- `pen_escape_fraction` — the same for a pen; **validated `< pastoral_escape_fraction`** (the fence
  is slower, and stating the invariant makes "pen faster than open range" unrepresentable).
- A randomization band around each (e.g. `escape_fraction_jitter`) — how much the seeded draw varies
  the fraction turn-to-turn, for playability.
- `MIN_ESCAPE_ANIMALS` — the whole-animal convergence floor (a named constant, not config, unless a
  reason to tune it appears).

**Retired:** the whole `decay_under_herded` path. Its rung `build_decay` reference and the
`taming_rate`-scaled decay it applied are gone from the neglect path (`taming_rate` still scales the
`Tame` *build*, unchanged).

---

## 4. Surfacing (the silent-neglect fix)

Neglect must stop being visible only in the drawer.

1. **Command-feed notice on the edge**, not every turn: fire a `CommandEventKind` line the turn a
   managed herd *becomes* under-contained (or its requirement grows past its staffing) —
   *"Rabbit Warren — not enough herders; animals are drifting off (shrinking toward the N you can
   hold)."* Edge-gated like the pen-starvation feed line (`fauna.rs:2716`), which is the precedent.
   A second line when a fresh wild herd appears from the shed is optional but on-theme ("a wild
   warren has split off").
2. **A warning icon in the worker/assignment panel** (Ray's minimum bar) — the herd reads as
   under-contained wherever it is listed, not only when its drawer is open. Client slice.
3. **Fix the panel's staged-vs-resolved confusion** (the "5 needed · only 2 of 5 working" the
   playtest photographed). The client reconstructs "working" as `round(herded_fraction × needed)`
   from last turn's *resolved* fraction, so it contradicts the freshly-assigned herder count for one
   turn. Show the **staged** assignment ("5 assigned — takes effect next turn") rather than a
   reconstruction of last turn's resolved fraction. Client slice; may want the actual assigned count
   on the wire (§3.2 option b) if we don't want the client reconstructing anything.

---

## 5. Open items to resolve at implementation time

1. **Wild-herd merge/spawn helpers.** The placement rule is settled (§2.3: merge over tile+adjacent,
   else spawn on an adjacent land tile via `acceptable_steps`, same-tile fallback). What remains is to
   reuse the existing helpers rather than reinvent them — read `spawn_game_group_at` / the immigration
   `repopulate_fauna` path (for constructing a fresh wild `Herd`) and `advance_herds`' `acceptable_steps`
   (for the adjacent-land pick) and wire the shed through them.
2. **Map-wide game cap interaction.** Short-range game is capped at `abundance.max_total_game`
   (`core_sim/CLAUDE.md` → Fauna spawning). A shed that *spawns* a new wild herd must decide whether
   it counts against that cap or is exempt (it is player-caused, not worldgen). Merge-when-present
   (§2.3 step 1) sidesteps this for the common case; the spawn branch should be exempt from the cap
   (player-caused feral animals must not be silently suppressed because the map is at its game cap).
3. **Shed rate feel (§3.4 tuning).** How many turns to shed a large overage — punishing-fast vs
   forgiving-slow. A playtest dial; ship a sensible default and measure.
4. **`herded_fraction` semantics post-arc.** It stops driving decay; it still feeds `capacity_animals`
   and the panel readout. Confirm nothing else reads it as a decay input once `decay_under_herded` is
   gone.

---

## 6. Slice plan

- [ ] **1 — Server: the shed mechanic.** Replace `decay_under_herded` (and the binary corral escape)
  with the shared `shed_uncontained_animals` in `advance_husbandry`: whole-animal overage shed at the
  per-rung `escape_fraction`, seeded-RNG randomized, min-1 floor, merged/spawned into a same-tile
  wild herd; tameness decay deleted; total abandonment as the `capacity = 0` limit; pen loss on
  shed-to-zero. New config levers + validation. Retire the tameness-bleed tests
  (`an_under_herded_tamed_herd_decays_proportionally_and_recovers`,
  `a_properly_herded_tamed_herd_does_not_decay_under_a_harvest_policy`) and add: convergence to
  capacity from an over-stocked start, no tameness change under neglect, pen sheds slower than
  pastoral, abandonment sheds to wild + loses the pen, determinism (two-run bit-identical). **Owner:
  server-dev, once this doc is approved.**
- [ ] **2 — Server: the feed-line notice.** Edge-gated command-feed line when a managed herd becomes
  under-contained / its requirement outgrows its staffing (mirror the pen-starvation edge gate). Ship
  with slice 1 or immediately after.
- [ ] **3 — Client: surfacing.** Warning icon in the worker/assignment panel; fix the
  staged-vs-resolved "N of M working" readout to show the staged assignment; render the new feed
  line. Consumes whatever slice 1/2 put on the wire. **Owner: client-dev.** Per [[pr-and-commit-authority]]
  the client half must exist before the arc is merge-ready.

---

## 7. Flora is out of scope (and why)

Cultivation was traced for parity and is **structurally safe from this failure**, so no flora analog
ships here:

- **No scaling worker requirement.** A cultivated patch's decay is spared by a single boolean
  (`ForagePatch::tended_this_turn`, `forage.rs:195`) — one worker holds a patch of any size. The
  labor-pool trap **cannot form** on the plant side.
- **No proportional-shortfall decay.** Flora neglect is binary (worked ⇒ spared entirely, unworked ⇒
  full rung decay, `advance_cultivation`, `forage.rs:1090`); there is no "working it but still
  slipping" regime, which was the core of the complaint.
- **Neglect already reverts gracefully** to a wild gather patch (still fully usable, always
  recoverable), which is the plant analog of what this arc builds for animals. The honest flora analog
  of "shed animals to a wild herd" is *not* "shed biomass" — flora's neglect axis is the cultivation
  *meter*, not the mass — it is the already-shipped "revert to a wild stand."

Two latent items flora *shares* — the **no-hysteresis rung flag** (chatter at exactly 1.0) and
**silent neglect** — do not bite on the plant side (one worker holds any patch, so nothing oscillates
while being worked). If parity is wanted later, the only worthwhile carry-overs are the two
*surfacing* items (a feed line when a tended patch/Field reverts to wild; a warning icon), filed as
separate follow-ups. The runaway trap this arc fixes is **fauna-only**.

---

## 8. See also

- `docs/plan_corral_managed_population.md` — the pen economy + the escape/starve semantics this
  arc reworks.
- `docs/plan_intensification_ladder.md` — the rung engine; `Tame` is how tameness is *earned* (this
  arc makes it permanent once earned).
- `docs/plan_grazing_2d.md` — the fenced-footprint pen this shed mechanic un-fences on abandonment.
- `core_sim/CLAUDE.md` → "Herding is standing labor" (the `herders_needed` / `decay_under_herded`
  model this supersedes) and "Corral" (the escape/starve pass edited here).
