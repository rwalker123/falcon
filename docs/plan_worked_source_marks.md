# Worked-Source Marks — making the ladder visible from the map

**Status:** design approved (UX prototype, 2026-07-29); the map half and the work board both **landed**. Issue #412.
Sits on `docs/plan_intensification_ladder.md` (the rungs and their gates) and composes with the
select-then-cycle work merged as PR #432 (issue #429).

## 1. The problem

Advanced rung verbs — `Cultivate`, `Sow`, `Tame`, `Corral` — are reachable only by selecting the
right band and opening the work tab. Two separate failures produce that:

- **Everything the player knows about their own work is selection-gated.**
  `BandOverlayRenderer.draw_band_work_highlights` returns immediately when `selected_unit_id < 0`,
  so every worked-tile fill, hunted-herd ring and yield label vanishes on deselect. Even while
  selected, only *one* band's work is on screen — the map can answer "what is this band doing",
  never "what are my people doing".
- **A source that can climb looks exactly like one that cannot.** The moment that makes many
  sources upgradable at once — a faction knowledge track completing — happens in the top bar,
  nowhere near the sources it changed.

## 2. The unit is the SOURCE, not the hex

A hex can hold a forage patch and several herds at once, worked by different bands at different
rungs. A tile-level mark has to pick one answer out of four, so it cannot be right.

The map already solves co-location: `SecondaryMarkerRenderer.compute_slots` assigns each wonder,
food site and herd on a hex a fixed edge slot (`_secondary_slot_lookup[key] → 0..2`, or `-1`),
caps the visible set at `SECONDARY_VISIBLE_CAP` (3), and puts a `+N` chip on the right flank for
the rest; bands own the centre as a card stack. **The marks ride that system rather than
introducing a placement scheme of their own.**

### 2.1 One ring grammar for both food webs

Today a hunted herd gets a red ring on *its own marker* while a foraged patch tints the whole hex
green — the same fact in two visual languages, only one of which survives co-location. So forage
takes the ring too, in the green it already owns:

| | mark | any player band | the selected band |
|---|---|---|---|
| **Forage** | ring on the food-site marker | thin, `FORAGE_WORKED_OUTLINE` at reduced alpha | bold ring + faint fill glow |
| **Hunt** | ring on the herd marker | thin, `HUNT_WORKED_COLOR` at reduced alpha | bold ring + the band→herd link |

The band→herd link and the per-source yield labels stay **selection-only**: N bands of links is
spaghetti, and those are what selection buys.

**The whole-hex green fill is retired.** `FORAGE_WORKED_FILL` / `FORAGE_WORKED_OUTLINE` had exactly
two call sites, both inside `draw_band_work_highlights`; nothing else in the client read them.

A **thin hex outline** survives as the only tile-level mark, on one argument: `compute_slots`
returns early below `ICON_MIN_DETAIL_RADIUS`, so at far zoom there are no markers to ring. The
outline is what remains there, and the fallback whenever a worked source is overflowed out of a
visible slot.

### 2.2 One badge per source

Under each ringed marker sits **one badge** carrying the two remaining facts:

- **Crew** — `⚒N`, total workers on *that* source across all bands.
- **Ready** — when that source's next rung is available, the badge gains a `⌃` chevron plus the verb
  glyph (`⌃▦ ⚒3`) **and its border turns `HudStyle.SIGNAL`**, so an offer reads at a glance without the
  eye having to resolve a small glyph. Otherwise the border is the quiet `HudStyle.LINE`.

> **The border carries READINESS, not ownership.** An earlier cut of this plan gave it the owning
> band's faction colour, with ink-dim for a source two factions share. That is dropped, and the
> reason is worth recording: the mark pass filters on `MapView._is_player_unit`, so **only the
> player's own work is ever marked** — a faction colour would be a constant, carrying no information,
> while spending the one channel the badge has on its most useful signal. If foreign work is ever
> surfaced, ownership needs a channel of its own rather than this one back.

One plate, not two: with three sources on a hex, two elements each is six things competing for the
same forty pixels.

**It docks below the icon, never upper-right** — the starving-pen distress badge already owns that
corner (`HERD_DISTRESS_BADGE_OFFSET_FACTOR`), and a herd can be both penned-and-starving and
ready-to-something.

### 2.3 The overflow chip carries what it hides

Three slots and a chip is the right budget, but a cap that hides state silently reads as "nothing
here" — the same failure this feature exists to fix at a different scale. So the `+N` chip **rolls
up** everything it hides, in severity order and at most two marks wide: `⚠` if a hidden source is in
trouble, `⌃` if one is ready, `⚒` if one is merely worked. Its tooltip names them in full.

**A ready source is deliberately NOT promoted into a visible slot.** Slot fill is sequential
precisely so icons never jump between frames; reordering on a state change would make a herd swap
corners the turn a knowledge track completes. The chip plus the re-click cycle covers the same need
without breaking that invariant.

## 3. "Ready" is the compose sheet's own test, reused

A worked source is **ready** when its next rung's verb would be selectable right now — the test
`_forage_policy_gates` / `_hunt_policy_gates` already run to decide whether the compose sheet greys
a rung, plus the ceiling and crop-legality passes beside them:

1. **Offered** — hunt: the husbandry ceiling (`wild` / `pastoral` / `pen`) admits the rung. Forage:
   at least one composition entry with `can_cultivate` / `can_sow`, and for `Sow` an empty
   `sow_site_refusal`.
2. **Ungated** — the gate function returns no reason for that rung: knowledge complete, the
   per-source prerequisite met, the rung not already finished.
3. **Not already running** — the source's current policy is not that verb. Mid-`Cultivate` is
   progress, not opportunity.

One mark per source, **highest rung first** — the ordering `_work_source_rung` already depends on,
and for the same reason: a herd that can be corralled can also technically be re-tamed, and marking
the lower rung would erase the distinction the mark exists to draw.

### 3.2 A rung UNDER WAY is the other half of the same axis

Condition 3 above is right and its first consequence was wrong. Excluding the verb in flight left the
**in-progress case with no mark at all**, so a patch you were actively cultivating looked exactly like
one nobody had touched — while the untouched patch beside it advertised `⌃`. The state the player is
*waiting on* was the one state the map did not report.

`RungGates.rung_in_progress` is the twin answer: `{policy, glyph, progress}` for the verb currently
being worked. The badge and the work row show **whichever of the two applies**, and they are mutually
exclusive by construction — `next_rung_ready` excludes the verb in flight, `rung_in_progress` answers
only for it.

- **Face:** `<verb glyph><percent>%` (`🌱42%`), with **no chevron**. `⌃` means *you could start this*,
  and the work has started. The percent is the whole point — it is what moves every turn, and the only
  number answering "how much longer?".
- **Colour:** `HudStyle.SIGNAL_DEEP`, the ready cyan one step deeper. Ready and building are one axis
  in two states, so they share a colour family — bright says *act now*, deep says *already under way*.
  A separate hue would file them as unrelated facts, and amber is spoken for by trouble.
- **Keyed on the POLICY, not on a non-zero meter.** A half-built patch nobody works is not "in
  progress"; its standing rung is what the rung glyph reports. Each investment verb names the meter it
  fills (`cultivation_progress` / `field_progress` / `domestication` / `corral_progress`) in one place,
  so no caller can read the wrong meter and report a confident wrong number.

### 3.1 The mark needs its own chrome, not its own glyph

The verb glyphs and the standing-rung glyphs collide: `▦` is both "Sow" and "this is a Field", `🐄`
is both "Corral" and "this is a Pen". A bare `▦` on a marker would read as **done**, the opposite of
the message. So the `⌃` chevron is the new word and the glyph only names the rung.

**Cyan, not amber.** `HudStyle.WARN` is trouble in this HUD (overdraw, understaffing, starving pen);
`HudStyle.SIGNAL` is live-and-worth-your-attention. Colouring an opportunity amber trains the player
to read good news as a warning.

**Absent, never greyed**, inheriting the compose sheet's own ceiling rule: greying implies a
reachable prerequisite, and no amount of knowledge pens an animal whose ceiling is `pastoral`. A
source with no chevron has nothing to offer, so a chevron always means "you can do this now".

## 4. Where the derivation lives

**Only the KNOWLEDGE is pushed; the derivation stays in `RungGates`.** The plan here originally called
for deriving a per-source mark model in HUD-land and pushing that to `MapView`. Measured against the
code, `MapView` was missing far less than assumed: it already holds `forage_patch_lookup`, `herds` and
every band's `labor_assignments`, so the one input it lacks is **faction knowledge**. It therefore
takes the raw `{track: progress}` row (`Hud.faction_knowledge_changed` → `Main` →
`set_faction_knowledge`, mirroring the `labor_pending_changed` path) and asks `RungGates` itself.

That is strictly better than the model push: one derivation instead of two, no per-frame dict of
digested state, and the rung rules stay in exactly one place for all three surfaces. `RungGates` being
all-`static` and stateless is what makes a renderer calling it fine — the rule a renderer must obey is
"hold no HUD controller or state model", not "call no shared logic".

The knowledge row is still a dict pushed in from another surface keyed by tracks a new world reuses,
so it is cleared in `reset_world_state` — the third shape named in
`.claude/rules/core_sim/world-handoff.md`.

The gate functions themselves move out of `DrawerComposeController` into a new all-`static`,
stateless `ui/hud/RungGates.gd`, with faction knowledge threaded in as a parameter. The map layer,
the work board and the compose sheet all need the same answer, and a renderer must not depend on a
compose controller — shared-layers-first, per `.claude/rules/client/hud-modules.md`. One definition,
so the three surfaces cannot disagree about what is climbable.

## 5. The work board

Co-location costs the board nothing: its rows are already per-source (a forage row per tile, a hunt
row per herd, through `effective_worker_map`), so four sources on one hex are four rows and always
were. It needs the third mark:

- The row already separates `marks` (the verb in flight) from `rung_glyph` (the standing rung).
  **Ready is the third axis — the rung on offer** — and takes the same chevron, right-aligned before
  the rate so the eye finds a column of them.
- **The severity stripe is untouched.** It means WARN (overdrawing, overstaffed) or SIGNAL (pending
  edit); folding an opportunity in would give the one control for finding trouble two meanings.
- The zone head gains a `⌃N ready` filter chip beside the attention chip — separate count, same
  mechanism. It is what makes the knowledge-completion moment legible: eleven rows light up at once
  and the chip says `⌃11`.

## 6. Reaching the source — how this composes with #429 / #432

**This feature SIGNALS; the select-then-cycle work REACHES.** PR #432 made a re-click on a selected
hex advance through `_selection_cycle_on_tile` — every band, then every herd, then the **land** —
derived from the selected occupant's identity and wrapping around. The overflow chip is where the two
meet: the chip says *something is in here*, the cycle is how the player gets to it.

Two co-location defects surfaced while designing this. Neither is introduced here; both are made
visible by marks that point at a specific source, and both are fixed as part of this work:

- **Yield labels stack.** `_queue_yield_label` anchors every label at the *hex centre* for both webs,
  so two hunted herds on one hex draw two rates at the identical point. Labels move to the slot their
  source drew in, falling back to the hex centre when the source has no visible slot.
- **The forage jump named no subject.** `BandPanelController.focus_labor_source` names the herd
  exactly for a hunt row (`roster_occupant_selected("herd", herd_id)`), but the forage branch only
  focused the tile and let the hex's auto-pick decide — so on a shared hex it opened a band or a herd
  instead of the patch, jumping to a place but not to a thing. It now names the LAND, the patch's own
  subject, through the same third `(kind, id)` kind the panel's land row and the map's
  select-then-cycle already use.
- **A third, found while marking expedition quarries:** the tile outline under every mark was
  hardcoded to the forage green, so a HUNTED herd's hex was outlined in the gather colour. It takes
  the source's own colour now, like the ring above it.

## 6a. A hunting expedition's quarry

A hunt **expedition** carries its target on the COHORT (`expedition_target_herd`), not in a
`labor_assignments` row — a detached party follows one herd, so the sim puts the quarry on the party
itself. A mark pass that only walks assignments therefore misses it entirely, which is how the first
cut shipped: the party crossed the map and nothing said what it was walking to.

Its quarry takes the same red ring and crew badge a locally-hunted herd wears, because **the mark
describes the source, not who reached it**. The party's people sum into that source's crew alongside
any resident band hunting it — one source, one number.

**Marked at every phase, `outbound` included.** "This herd is already claimed" is exactly what the
player needs before committing a second crew, and a party three turns out has claimed it as surely as
one standing on it.

## 7. Deliberately out of scope

- **Clicking a badge to select its source.** `_secondary_slot_lookup` now makes per-slot hit-testing
  derivable, and it would supersede #432's known gap (`_herd_at_point` hit-tests every herd against
  the hex *centre*, so it always returns `herds[0]` whichever slot the sprite drew in). But the
  re-click cycle already reaches every source including the land, so this is an improvement rather
  than a requirement.
- **Marks on unworked sources.** A sowable tile nobody stands on is arguably the most valuable thing
  to surface — 46 of 4160 tiles are sowable on the standard map — but that is *prospecting*, and it
  wants an overlay channel, not a badge on every hex.

## 8. See also

- `docs/plan_intensification_ladder.md` — the rungs, their knowledge gates and the site requirement.
- `.claude/rules/client/overlay-channels.md` — the selected-band overlay pass these marks extend.
- `.claude/rules/client/map-markers.md` — the slot system the marks ride.
- `.claude/rules/client/map-renderers.md` — select-then-cycle (#432).
