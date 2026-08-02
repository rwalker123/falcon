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

`born` is Routine while `came_of_age` is Notable for the same kind of reason: a birth is a mouth, a
coming-of-age is a new pair of hands, and only the second changes what the player can do this turn.

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
ring. So re-ingesting a full snapshot's ring is harmless (it is the backfill for a player who
connected mid-session) and there is **no per-full-snapshot reset**. The only legitimate clear is a
**world boundary** — `Main._reset_per_world_state` → `EventDockPanel.reset()` — because a new world
is not another snapshot of the same history. Resetting per full frame would additionally throw away
the client's own System-channel events every time the sim restated its ring.

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
