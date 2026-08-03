---
paths:
  - "clients/godot_thin_client/src/scripts/ui/EventDockPanel.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_event_vocab.gd"
  - "clients/godot_thin_client/src/ui/EventDockPanel.tscn"
---

# The event dock — the client's notification surface

Issue #272. The spec of record is `docs/event_dock_ux_proposal.html`, including its interactive
prototype: the filter/pin/expand logic here is a port of that `<script>` block, and the two
"what building it changed" findings at the bottom of §03 are the two failure modes this file exists
to keep closed.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/EventDockPanel.gd` / `src/ui/EventDockPanel.tscn` | The dockable **event dock** CanvasLayer: a horizontal notification strip on `SIDE_TOP` or `SIDE_BOTTOM`, reserving its edge through the registry (`reservation_changed(edge, size)` → `Main._apply_reservation(&"event_dock", …)`) exactly as `BandCityPanel` does. Two states — the COLLAPSED bar (`recent_count` rows, newest first, with the pinned-alert exception) and the EXPANDED turn-grouped log (World/System chips, the detail floor, the row count, the dock edge, "Earlier turns"). It accumulates `command_events` and de-duplicates on **`seq`**, prunes by TURN window against the sim's `command_events_retention_turns`, and takes client-side System notes through `note_system(label, detail, alert)`. Prefs live in a new `[events]` section of `user://narrative.cfg`, with a `config_path_override` static for the harnesses. Toggled by `R` (`Main._toggle_event_dock_visibility`) |
| `ui/hud/hud_event_vocab.gd` (`HudEventVocab`) | The importance model, as an ALL-`const` vocabulary leaf (`hud-modules.md`): `RUNG_BY_KIND` · `CHANNEL_BY_KIND` · `RUNG_STYLE` (glyph + `HudStyle` accent per rung) · `KIND_STYLE` (the threat/casualty kinds, absorbed from the retired `CommandFeedController`) · `DETAIL_STATUS_STYLE` (the `status=` token rule) · `DETAIL_FLOOR` (the three player settings as a floor on the rung ladder) plus the dock's word tables and glyphs. Reads only `HudStyle`, which reads nothing, so it cannot enter a class-load cycle |

## Three questions, kept apart

Collapsing any two of them is what turns a notification system into one undifferentiated stream, so
`HudEventVocab` answers them in three separate tables:

| | Question | Table |
|---|---|---|
| **Channel** | where did it come from? | `CHANNEL_BY_KIND` — `world` (the sim) vs `system` (the client's own plumbing) |
| **Rung** | how loudly does it say it? | `RUNG_BY_KIND` — `alert` / `notable` / `routine`, one per kind |
| **Detail floor** | how much does the player want? | `DETAIL_FLOOR` — a floor on the rung ladder, admitting its own rung and everything louder |

**The floor is a floor, not a second taxonomy.** That is what keeps the preference to three legible
options rather than a checklist of twenty-seven kinds, and it is why the channel toggles sit
*alongside* it: a player who wants world events but not socket chatter says so without touching the
floor.

**`died`, `migrated` and `came_of_age` are NOTABLE, not Alert.** Bands lose elders to cold as a
matter of course, and a rung that interrupts for every one of them trains the player to stop reading
the bar — the precise failure the three-rung ladder exists to prevent. A death that *matters* (a
whole band starving out) announces itself through the starvation and morale channels that already
exist. Alert is kept for violence, for an investment going feral, and for the client's own faults.

**`born` is NOTABLE too, and it shipped Routine — which was wrong at the play-test.** Routine sits
below `DEFAULT_DETAIL_LEVEL`, so a birth never appeared at all unless the player switched to
"Everything", where it arrived buried among forage receipts. Ray hit exactly that at population 31:
the counter ticked up and the bar said nothing, which is the failure this whole arc exists to remove.

The reasoning that put it there ("a birth is a mouth, a coming-of-age is a new pair of hands") was
describing a real difference on the wrong axis. **The rung is not a measure of how much a turn's
LABOUR changed** — it is whether the world changed in a way worth knowing. `born`, `came_of_age` and
`died` are one family by that test: the settlement visibly changing size is the plainest such change
there is, and the most legible sign it is alive at all. Routine keeps what it is actually for —
receipts for verbs the player asked for.

## A row's accent is resolved most-specific-first

Three layers, and each exists because the one below it cannot express the case:

1. **`KIND_STYLE`** — `predator_raid` → `⚔` + `HudStyle.THREAT_ACCENT`, `hunt_danger` → `⚠` +
   `HudStyle.HUNT_DANGER_ACCENT`. Carried over verbatim from the retired command feed so a raid row
   still wears the same crimson as the map's `threat` overlay wash and a hunt-danger row the same
   amber as `hunt_danger` — the bar accent and the map wash speak one danger language.
2. **`DETAIL_STATUS_STYLE`** — a rung going feral and an assignment dropped for want of people ride
   their VERB's own kind (`cultivate` / `sow` / `forage` / `hunt`), deliberately, so a rung's whole
   life reads on one channel. That makes the LOSS the same kind as the COMPLETION before it, which
   `KIND_STYLE` structurally cannot separate. The sim's own `status=` token can, and it also
   **promotes the row to the Alert rung** (§02 lists both tokens under Alert).
   **Matched as a whole space-delimited `key=value` fragment, never a bare substring** — the sim
   writes `"status=feral reason=untended …"`, and a substring test on `feral` would also fire on a
   species key or a tile label containing the word.
3. **`RUNG_STYLE`** — the default glyph + accent for everything else.

Only an Alert's LABEL carries its accent; a Routine one recedes to `INK_DIM` and a Notable one stays
on the shared ink. The rung reads off the rail and the glyph — tinting every label would turn the bar
into a colour chart and cost the alerts the contrast that makes them alerts.

## The wire tokens are a CONTRACT; what the player sees is rendered from them

A `detail` arrives as space-delimited `key=value` fragments — `category=settle_site at (64,36)`,
`band=3 count=4 direction=out`. **They are sized for a parser, not for a reader**, and they exist so
the client can do work with them: join `band=` to the roster name, promote a `status=` row to the
Alert rung. The dock printed them verbatim at first, so a row read `category=settle_site at (64,36)`
on a player-facing bar — an internal identifier where prose belongs.

`EventDockPanel.detail_phrase` renders them, over three tables in `HudEventVocab`:

| Layer | Does | Example |
|---|---|---|
| `DETAIL_KEY_HIDDEN` | drops keys the LABEL already carries — `band`, `count`, `expedition`, `killed` | `band=3 count=4 direction=out` → `departed` |
| `DETAIL_VALUE_LABELS` | the enumerated values in English, used VERBATIM (several are deliberately lowercase, reading as a phrase continuing the label) | `settle_site` → `Settle site`, `out` → `departed` |
| the generic fallback | underscores → spaces, first letter capitalised | `herd_gone` → `Herd gone` |

**The fallback is the load-bearing layer, not the safety net.** Kinds and tokens are added
server-side with no schema change, so a value with no table row is the *common* case over time — a
raw identifier reaching the screen has to be impossible by construction rather than by anyone
remembering to add a row. `ui_preview` states both guarantees as general properties — no Label the dock renders may contain a
`=`, and no rendered detail may carry a trailing-zero decimal — each with preconditions so it cannot
pass vacuously. **The second one caught the harness twice**: first the fixture had no `{:.3}` number
in it at all, and then, once it did, the row carrying it sat outside the log's five-turn window, so
the on-screen scan walked rows that never could have failed. It now scans `detail_phrase` over the
whole retained pool as well as the rendered labels, which is the complete property and cannot drift
out of view.

Three details in the walk are not obvious:

- **A NUMERIC value keeps its key, an enumerated one does not.** `Killed 2.000` is meaningless
  without the key; `Category Settle site` is worse for having it.
- **A bare word continues the previous value.** The sim writes `species=Grey Wolf`, so a naive
  space-split drops the `Wolf`. The one exception is `DETAIL_FILLER_WORDS` (`at`), which is grammar
  the ` · ` join already supplies.
- **Coordinates stay** — re-spaced to `(64, 36)`. They were the one part of the raw detail worth
  keeping.
- **A number is TRIMMED, never rounded** (`_trimmed_number`). The sim writes casualties with
  `{:.3}` — honest on the wire, where a `Scalar` really can be fractional, and debug output on a
  notification bar: `Killed 2.000` is a float where the player is owed a count. Trailing zeros and a
  bare decimal point come off, so `2.000` → `2` while `1.750` → `1.75`. Rounding would state a
  precision the sim did not, and a casualty count reading `2` when the sim said `1.5` is a lie the
  player cannot detect. The early return on "no decimal point at all" is load-bearing: `rstrip("0")`
  on a bare `100` answers `1`.
- **`killed` is hidden and `wounded` is not.** The label already says "cost the party three lives",
  so `Killed 3.000` beside it says one thing twice in two notations; `wounded` is the half the label
  never carries.

**The RAW string stays the input to `DETAIL_STATUS_STYLE`.** That rule matches whole `key=value`
fragments against the wire text and must never start matching prose, which is why the phrase is
built at RENDER time and never stored back onto the event.

## De-duplication is on `seq`, and that fixed a real bug

Every consumer used to key on the synthesized signature `"%d|%s|%s|%s" % [tick, kind, label, detail]`
(`CommandFeedController`, `TellingPanel`, `Inspector`). **Two identical events in the same turn
therefore collapsed into one** — two wolf packs raiding the same band on the same turn were reported
as a single raid. `seq` is a monotonic per-event int, so it cannot.

**`seq` is ONE-BASED and `0` is a SENTINEL** meaning "this row never went through
`CommandEventLog::push`" — and it is also the FlatBuffers default for an absent field. Keyed on, every
such row would collide onto one; so a row with `seq <= 0` takes the signature fallback instead. That
fallback is deliberately a degrade path, not a second mechanism: it exists so a mixed frame cannot
crash or duplicate every row every turn, and it carries the old collapse-two-identical-rows bug for
exactly the rows that give it no better key.

`ui_preview`'s `event_dock_*` block asserts all three halves — that the fixture's two byte-identical
turn-47 raids survive as two, that a `seq`-less row ingested twice lands once, and that two rows both
carrying `seq: 0` do not collide.

## THE DOCK IS CLEARED ON EVERY FULL SNAPSHOT, AND THAT IS CORRECTNESS

`Main._apply_snapshot`: `if not is_delta: _event_dock_invoke("reset")`, **before** that snapshot's
events are ingested.

`CommandEventLog` is checkpoint state, so a **rollback** restores it *including* its `next_seq`
counter — the replayed events therefore reuse sequence numbers the client has already seen. A
rollback publishes a FULL frame. Without the clear the dock suppresses every replayed row as a
duplicate `seq` and goes on showing a plausible but stale log, with nothing anywhere reporting a
fault. It is the same guard `Main` used to apply as `reset_command_feed`, repointed.

**The order is part of the contract**: the same frame carries the backfill, so clearing *after* the
dispatch would wipe what just landed. Nothing is lost by clearing first — a full snapshot carries the
whole retained ring — except the client's own System-channel notes, and a full frame is rare enough
(first frame, resync, `new_game`, rollback) for that to be the right trade against a silently wrong
log.

**For the same reason the retention trim keys on `tick`, never on `seq` arithmetic** — `seq` is not
monotonic across a rollback. A world change needs no separate clear either: it always arrives on a
full snapshot.

`ui_preview` drives the regression directly: a batch, a `reset()` (what the full frame does), then
rows REUSING those `seq` values with different labels, asserting the new rows are what the dock holds
and the replaced ones are gone. Sabotage-verified by making `reset()` keep `_seen_seq`.

## The dock names a band the way the rest of the HUD does

**The snapshot carries no band NAME.** The sim writes a positional `Band <BandId>` into a demographic
event's label and repeats the id as a `band=` detail token — precisely so the client can re-label the
row. The client's own name is a **roster position** (`HudFormat.band_display_name`) and the sim's is a
**durable id**; the two routinely disagree, and the token is the only thing that can join them.

The join needs the roster, which the HUD owns, and the dock is `Main`'s panel — so
`HudLayer.update_band_alerts` publishes `band_labels_changed({band_id: name})` and `Main` relays it.
The sim's label is never changed; neither surface reaches into the other.

Two details:

- **Resolved at RENDER time, not stamped at ingest** (`_row_label`), so a roster change relabels rows
  already held and a row that arrives before the first `set_band_labels` is not stuck with the
  fallback forever.
- **The substitution stops at a DIGIT BOUNDARY.** A plain `String.replace` of `Band 3` finds the
  `Band 3` inside `Band 30` first and corrupts the label to `Band 10`. The sim names exactly one band
  per label today, so no live event reaches that trap — which is why `ui_preview`'s fixture
  CONSTRUCTS it (`"Four left Band 3 for Band 30"`) rather than quoting one. It also pins the honest
  limitation: only the band the token NAMES is substituted; a second band in the same label keeps
  whatever the sim called it.

## `command_events` is per-frame history, so the dock ACCUMULATES

A delta carries only the rows appended since the baseline; a full snapshot carries the whole retained
ring. So re-ingesting a full snapshot's ring is harmless — it is the backfill for a player who
connected mid-session, and the `seq` de-duplication absorbs the overlap.

Re-ingesting is harmless, but it is **not** what happens: the dock is cleared on every full frame
first, for the rollback reason in the section above. The two are not in tension — the clear costs
nothing on an ordinary full snapshot precisely *because* that frame carries the whole ring, and the
one thing it does cost (the client's own System-channel notes) is the deliberate trade against a
silently stale log. A world boundary needs no separate clear of its own; it always arrives on a full
snapshot and is covered by the same line.

**Retention is measured in TURNS, not entries.** Add a birth, a death and a coming-of-age per band
per turn and a fixed entry ring evicts a wolf raid inside two turns — the cap would quietly eat
exactly the events it exists to preserve. A turn is also the unit the player thinks in and the unit
the log groups by. The sim is the authority (`command_events_retention_turns`); the client's
`DEFAULT_RETENTION_TURNS` only bounds accumulation before it has heard.

That window is **hot-reloadable server-side**, so it is read per snapshot rather than latched at boot,
and `Main` pushes it **before** the frame's events — a frame can both narrow the window and carry the
events that must be trimmed against the new value. The native decoder withholds a `0`, which is the
FlatBuffers default and means "not stated", so the client's own default stands rather than a retention
window of zero silently emptying the log.

`TellingPanel` keeps claiming the narrative kinds through **`handles_kind()`**, and the dock skips
whatever that claims. The test stays there, so a kind can never be claimed by both surfaces or
dropped by both.

## The bar is STRICTLY the collapsed state

Opening the log replaces the rows with a one-line title. At four rows the bar printed the log's own
newest turn-group a second time, directly beneath it — the same four events twice, six inches apart.
That is the first of the prototype's two findings and it must not come back; `ui_preview`'s
`event_dock_top_expanded` asserts the bar is exactly one row while expanded.

**Newest first, with one exception: an unread Alert pins to the leading slot** and holds it until the
log is opened. You get chronology *and* you never lose the raid to two forage receipts arriving in
the same turn. A new alert re-arms the pin (unread is a property of the alert, not of the dock);
`set_expanded(true)` is what marks alerts read, because the pin exists to survive until the player
has actually had a chance to look.

The pin is tracked as an **`order`** (the ingest counter), never as the record itself: two events can
carry equal field values — that is the very bug `seq` de-duplication fixes — so identity here has to
be an id, not a `==` on a Dictionary.

## The strip is CAPPED and CENTRED, not stretched

On an ultrawide the bar spanned the whole band between the columns, so a row's label sat at one end
of two feet of screen and its detail at the other and the pair read as two unrelated things.
`MAX_STRIP_WIDTH` bounds it and the strip centres in whatever band is left.

**The number is chosen against two measurements and the larger wins.** The widest row the shipped
fixtures produce at the current font sizes is **594px** — a predator raid, `A Grey Wolf raid cost 2
lives` beside `Killed 2.000 · Wounded 1.000 · Warriors 3 · Grey Wolf` — which with the expander (86)
and the card chrome (31) needs **711**. But the band between the two HUD columns at the project's own
1920 base canvas is **1216**, and the ordinary case must render *unchanged*: a cap below that would
shrink the bar on every desktop to fix a complaint about ultrawides. So the cap sits just above the
base-canvas band at **1280** — no content is ever squeezed, nothing moves at 1920 or below, and past
it the strip stops growing.

It is **cross-axis only**, the same rule as the perpendicular insets: it moves where the strip is
drawn, never what it reserves. `ui_preview` asserts BOTH halves — at the normal canvas the strip
equals the band (1216) and at an ultrawide it equals the cap and is centred — because a cap
hard-wired on fails the first and one hard-wired off fails the second.

## The strip yields to the map, and the reservation never depends on content

Two separate rules, both learned elsewhere in this HUD:

- **`_cross_axis_size()` is clamped to `MAX_STRIP_HEIGHT_FRACTION` of the window** and the log
  scrolls internally past it — the same bound `DockScrollFit` put on the command feed, for the same
  reason: a log that can eat the viewport has stopped being a notification. This is the prototype's
  second finding. `ui_preview` asserts BOTH ways the dock can grow (the widest bar with the log
  closed, and the log open, which collapses the bar to one title line) against the cap, as a pair —
  they are alternatives rather than addends, so neither is the worst case by inspection.
- **The reserved size reads only the preference, the expanded flag and the viewport** — never the
  event list. The bar reserves `recent_count` rows whether or not it has that many events. This is
  `BandCityPanel`'s rule, learned there as a map flicker on every `+` press; here an arriving event
  every turn would be a far worse offender.

## On a shared edge the bar hugs the screen edge

`Main.RESERVER_PRIORITY` is `{event_dock: 0, inspector: 1, band_panel: 2}`. A thin strip on the rim
reads as chrome, and putting it outermost means the band panel's position relative to the map never
changes when the bar grows a row. Nothing else was needed: `_update_band_panel_edge_offset()` already
sums the lower-priority co-edge reservers, so the offset falls out of the existing mechanism, and the
dock itself needs no `set_edge_offset` because priority 0 always hugs.

(The Inspector is always `SIDE_LEFT`, so it never actually shares an edge with a top/bottom-only
dock; its number is there to keep the order total.)

## …and it lives BETWEEN the vertical docks, which is a different axis entirely

Priority does not help here, and expecting it to is what shipped the bug: a `SIDE_TOP` bar spanning
the raw window, drawn at layer 104 over the `SIDE_LEFT` band panel at 103, covering its tab bar.
TOP and LEFT are not co-edge, so `_update_band_panel_edge_offset` correctly ignored the pairing and
no amount of renumbering would have moved either one.

The fix is on the **perpendicular** axis: `Main._update_event_dock_insets()` sums the live
`SIDE_LEFT` and `SIDE_RIGHT` reservation totals (every reserver except the dock itself, so the
Inspector counts too) and pushes them to `EventDockPanel.set_perpendicular_insets`, which applies
them as `offset_left` / `offset_right` on its root. The bar starts right of whatever is docked left
and stops left of whatever is docked right. A left/right panel owns its full-height column; the
horizontal strip lives in the band between them.

- **Recomputed on EVERY `_apply_reservation`, not just the dock's own** — the band panel changing
  edge, collapsing or hiding has to move the bar — and seeded once at connect time so a session that
  boots already docked is right on frame one (including one that boots with the bar suppressed, which
  reserves nothing and would otherwise leave a full-width strip waiting behind `R`).
- **It moves where the strip is DRAWN, never what it RESERVES.** `current_reservation_size()` is
  untouched, so the content-independence rule above still holds.

Also note it is not a stacking-order problem: raising or lowering `LAYER_INDEX` would only decide
*which* panel is hidden by the other. The band panel is a legitimate occupant of that column and the
bar has no business in it.

### …and a RESERVATION is only half the bound

The first cut of that fix bounded the bar against edge **reservers**, and the HUD's own side columns
are not reservers — so with nothing docked left or right the bar spanned the window again and sat on
top of `Turn N` / `Units` / `Sedentarization` / `Pop`. The columns live INSIDE whatever strip the
docks reserved, so the two terms **add**: `inset = reservation total + Hud.{left,right}_column_width()`.

Both column widths are **authored, not measured** (`panel-framework.md` → "The HUD's own side columns
are AUTHORED" has the table and the reasons). The right side is two regions in one column — the dock
and, above it, the readout block — so `right_column_width()` is the wider authored minimum of the
pair; the readout block had no minimum of its own and was pure text width, which is precisely the
measurement that must not decide a panel's edge.

### The bar also stops pushing the HUD down

`&"event_dock"` is in `Main.MAP_ONLY_RESERVERS`, so its reservation reaches `MapView` and **not**
`Hud`. Reported live: the bar reserved `SIDE_TOP` against the HUD, `LayoutRoot` absorbed it, and the
readouts and the right dock all sat lower than they had before the dock existed. The map half stays —
map content genuinely does hide behind the strip — and the HUD keeps its full height because the bar
now lives beside its furniture rather than above it.

**The layout Ray picked**, and what the two halves together produce:

```
┌──────────┬───────────────────┬──────────┐
│ Band     │ ⚔ T47 Wolves…     │ Turn 1   │
│ panel    │ ✦ T47 Came of age │ Units: 1 │
│          ├───────────────────┤ Sedent…  │
│ Work     │                   │ Pop 30   │
│ Parties  │      MAP          ├──────────┤
│          │                   │ AT THE   │
│          │                   │ FIRE     │
└──────────┴───────────────────┴──────────┘
```

`ui_preview` pins this with a real `BandCityPanel` docked LEFT — a literal would prove nothing about
two rects actually clearing each other — across `event_dock_inset_left_panel` and
`event_dock_inset_bottom_panel` (both edges, since the bug was about the horizontal axis and a fix
reaching only `SIDE_TOP` must fail), against a **negative control taken first on the same two live
nodes**: at zero inset the rects genuinely do overlap, so the assertion is not satisfiable by two
panels that happen never to meet. `event_dock_bottom` carries the zero case, so an inset hard-wired
to a constant cannot pass either.

**Each clearance claim is made where it BITES**, through `_assert_bar_clears`, which refuses to pass
on a pair that shares no vertical band. The HUD's regions sit in different bands — a bottom bar is in
the `BottomBar`'s (nav backing, turn orb), a top bar in the `TopBar`'s (the readout block), and only a
bar tall enough to reach the `ContentRow` can touch the two docks — so "these rects do not overlap" is
true for free of most pairs, and the first version of that block passed with the fix reverted. The
"nothing moves down" assertion had the same defect for the same reason (it was taken while the dock
was on the BOTTOM edge, where `offset_top` is not the offset at risk) and is now taken on `SIDE_TOP`
against both offsets, with the negative control that another reserver's strip DOES move the HUD.

## Preferences live in `[events]` of `user://narrative.cfg`

`edge` · `recent_count` (2) · `detail_level` (`notable`) · `channels` · `suppressed`. **Not a third
prefs file** — that file already holds the voice register, the Telling's collapsed state and
`[hud_panels]`, and the panel rules say not to add another.

`config_path_override` is a static, and it falls through to `NarrativeForkPanel.config_path()` when
unset — so a harness that has already isolated `narrative.cfg` (every one of them has) gets this
section isolated with it, and the explicit override is there for one that wants the dock's own walk
kept out of the Telling's scratch file too.

**`suppressed` migrates from `[hud_panels] command_feed_suppressed`**, read once and then ERASED, so
a player who had opened the retired feed with `R` lands on the bar that replaces it open too and a
stale key can never overwrite a later choice. Note the *default* flipped: the feed shipped hidden
(six read-only receipts in a dock column that had the verbs in it); the bar ships **visible**,
because a notification the player has to go and find is the thing this arc exists to remove.

## What this replaced

- **The left-dock command feed is gone** — `CommandFeedPanel` (the card), its nodes in
  `HudLayer.tscn`, and `ui/hud/CommandFeedController.gd`. Its `KIND_STYLE` alert table and its
  `status=` token rule moved into `HudEventVocab`; the left dock is the selection card's again.
- **The Inspector's `[SIM]` command stream is gone** — see `inspector-panels.md`. Its *console
  chatter* is not: it still fills the debug console AND now rides out to the dock's System channel.
- **The three controllers that posted a client-side note take a `Callable` note sink** rather than a
  `CommandFeedController` reference (`TopBarReadouts`' knowledge unlock, `TurnOrbController`'s
  unanswered fork, `TargetingController`'s two quarry refusals). It resolves to
  `HudLayer.note_system_event` → the `system_note_requested` signal → `Main` → the dock. The HUD
  emits rather than reaching for a panel it does not own — the coordinator mediates, as everywhere
  else.
