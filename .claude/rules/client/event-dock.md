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
| `ui/EventDockPanel.gd` / `src/ui/EventDockPanel.tscn` | The dockable **event dock** CanvasLayer: a horizontal notification strip on `SIDE_TOP` or `SIDE_BOTTOM` that **overlays the map and reserves nothing** (see "THE BAR RESERVES NOTHING" — it is not a reserver, and it publishes neither `reservation_changed` nor `current_reservation_size()`). It is bounded horizontally by the OTHER reservers plus the HUD's own columns via `set_perpendicular_insets`. Two states — the COLLAPSED bar (`recent_count` rows, newest first, with the pinned-alert exception) and the EXPANDED turn-grouped log (World/System chips, the detail floor, the row count, the dock edge, "Earlier turns"). It accumulates `command_events` and de-duplicates on **`seq`**, prunes by TURN window against the sim's `command_events_retention_turns`, and takes client-side System notes through `note_system(label, detail, alert)`. Prefs live in a new `[events]` section of `user://narrative.cfg`, with a `config_path_override` static for the harnesses. Toggled by `R` (`Main._toggle_event_dock_visibility`) |
| `ui/hud/hud_event_vocab.gd` (`HudEventVocab`) | The importance model, as an ALL-`const` vocabulary leaf (`hud-modules.md`): `RUNG_BY_KIND` · `CHANNEL_BY_KIND` · `RUNG_STYLE` (glyph + `HudStyle` accent per rung) · `KIND_STYLE` (the threat/casualty kinds, absorbed from the retired `CommandFeedController`) · `DETAIL_STATUS_STYLE` (the `status=` token rule) · `DETAIL_FLOOR` (the three player settings as a floor on the rung ladder) · **`IGNORED_KINDS`** (the kinds the dock drops at ingest — see "A kind the dock IGNORES") plus the two client-minted kinds `KIND_SYSTEM` / `KIND_COMMAND_ECHO` and the dock's word tables and glyphs. Reads only `HudStyle`, which reads nothing, so it cannot enter a class-load cycle |

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

**`died` and `migrated` are NOTABLE, not Alert.** Bands lose elders to cold as a matter of course,
and a rung that interrupts for every one of them trains the player to stop reading the bar — the
precise failure the three-rung ladder exists to prevent. A death that *matters* (a whole band
starving out) announces itself through the starvation and morale channels that already exist. Alert
is kept for violence, for an investment going feral, and for the client's own faults.

## The demographic kinds split on HEAD-COUNT

One line settles all five:

> **`born` / `died` / `migrated` change how many people the band HAS — Notable. `came_of_age` and
> `aged` move one person between brackets and leave the total untouched — Routine.**

**Both halves were learned from play, in opposite directions**, which is why the rule is written as a
rule rather than five table rows:

- `born` shipped **Routine**, i.e. below `DEFAULT_DETAIL_LEVEL`, so a birth never appeared unless the
  player chose "Everything", where it arrived buried among forage receipts. Reported at population
  31: the counter ticked up and the bar said nothing — the failure this whole arc exists to remove.
- `came_of_age` shipped **Notable** and was reported from a playthrough as **too much noise**. It
  fires constantly while the population never moves, so it filled the default floor with rows that
  answered no question — the same "stop reading the bar" failure, reached from the other side.

**Two retired framings, both real but on the wrong axis.** "A birth is a mouth, a coming-of-age is a
new pair of hands" measured how much a turn's LABOUR changed. "Anything that touches the working-age
population" measured which BRACKET moved. A rung is neither: it asks whether the world changed in a
way worth knowing. A settlement gaining or losing a person is the plainest such change there is; a
person having a birthday is not, however consequential the bracket it moves them into.

`ui_preview` pins **both directions as a pair** — every head-count kind above the default floor,
every transition below it. Either assertion alone passes on a table that has collapsed all five onto
one rung, which is precisely the state both reports were complaining about.

## A kind the dock IGNORES is dropped at INGEST, in both inlets

`HudEventVocab.IGNORED_KINDS` is a set of KINDS the dock never stores. It is deliberately none of the
three questions above: not a rung, not a channel toggle, not a floor, and **not a render-time skip**.
An ignored kind cannot appear at any detail level — `Everything` included, the setting where every
other filter has given up — on either channel, in the bar or in the expanded log, and it occupies
neither a `seq` de-duplication slot nor a retention row.

**Both inlets filter, and a mechanism covering only one is a trap.** `ingest_events` (the sim's
`command_events`) and `note_system` (every client-side line) are independent doors into `_events`;
today's ignored kind arrives through the second, and a filter on the first alone would have looked
right and done nothing. In `ingest_events` the test sits **before** the de-duplication, so an ignored
row burns no `seq` — otherwise the day a kind stopped being ignored, the dock would swallow the first
re-ingest of every row it had previously dropped.

**AN IGNORED KIND IS NOT A RETIRED KIND**, and that is the obvious misreading to keep closed. The
event still exists and is still emitted: the sim goes on writing it, the Inspector's Logs tab
goes on printing it in full, and a mod may want to read it. This is a display filter on ONE surface.

### The System channel carries two kinds, because it carried two things

`system` used to carry both an acknowledgement and a fault under one name, so no kind-level rule
could separate them and `Advance 1 turn.` sat on the bar on turns 1 and 2 of a new game — a receipt
for a button pressed a second earlier, printed as news.

| Kind | What it is | Where it goes |
|---|---|---|
| `command_echo` | a receipt for a command this client accepted for sending — `Advance 1 turn.`, `Answered the question.`, `Stop improving (12, 8).`, every `_send_formatted_command` message | the Inspector's Logs buffer only; **the dock ignores it** |
| `system` | a FAULT or a state change — command refused, socket lost/restored, `resync requested (unapplicable delta)`, and the HUD's own feedback (`Quick-hunt · No idle workers to assign`) | the Logs buffer AND the dock's System channel |

**The boundary is: a command accepted for sending is an echo; everything else is a fault.** A
rejected or failed send stays `system` — that is exactly when the player needs to hear it — and so do
both `resync` sends, which state `KIND_SYSTEM` explicitly rather than taking the echo default: the
client sent them because a frame could not be applied, so they are not a receipt for anything the
player did.

The kind is threaded as a **defaulted parameter** down `Inspector.system_event` → `Main.
_on_inspector_system_event` → `_note_system_event` → `EventDockPanel.note_system`, so every existing
caller keeps its meaning and only the acknowledgement path opts in — one line, `_send_command`'s
accepted-send log, which is the single place in the client where a command is known to have gone.
`Main._send_runtime_command` and `Inspector.send_runtime_command` carry an `ack_kind` for the same
reason in reverse: their default IS the echo, and the resync caller overrides it.

**The HUD's own `system_note_requested` chain carries no kind.** Every note on it — the quarry
refusals, the knowledge unlock, the unanswered fork, the quick-hunt refusals — is a fault or a state
change by construction; the HUD has no acknowledgement path, so `Main` states nothing and the
`system` default stands.

`command_echo` has a `CHANNEL_BY_KIND` row even though the dock never reaches that lookup for it: the
channel a kind belongs to is a fact about the kind, independent of one surface hiding it, and without
the row, dropping it from `IGNORED_KINDS` would file command receipts on the WORLD channel.

`ui_preview` states the claim as ABSENCE FROM THE STORED POOL rather than from the rendered rows —
"ignored" is stronger than "not shown", and a rendered-scope assertion narrows silently to whatever
is on screen. Eight assertions: both inlets, the `seq` slot, the `Everything` floor with both
channels on, and — in the same batch — a genuine `system` fault and a world event that MUST survive,
without which every one of them would pass on a dock that ignored everything. Sabotage-verified by
moving the filter to `_visible_events()`, which fails the four pool-scoped assertions while the two
visible-scoped ones stay green, and by moving the ingest test after the de-duplication, which fails
the `seq`-slot one alone.

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

The whole per-frame sequence is `Main.apply_event_dock_frame(dock, snapshot, is_delta)` — **RESET →
CURRENT TURN → RETENTION → INGEST**, in one function because every arrow in it is load-bearing and a
sequence spread across a fan-out is one where nobody can see the order. It is `static` and takes the
dock so `ui_preview` can drive the shipped sequence itself; an assertion that re-typed the order in
the harness would pass on whatever order `Main` chose, which is the only thing under test.

The clear comes first, and on a full frame only: `if not is_delta: reset()`, **before** that
snapshot's events are ingested.

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

### …so THE CURRENT TURN IS SET AFTER THE CLEAR, NOT BEFORE IT

`reset()` sets `_current_turn = -1` and `set_current_turn` only ever **raises** it, so a stamp taken
ahead of the clear is simply erased. What the dock then calls "now" is whatever the newest INGESTED
event's tick happened to be — or `-1` on an empty ring, where `_prune()` no-ops entirely until the
next snapshot. A resync at turn 500 whose newest retained event is turn 495 leaves a client-side
`note_system` posted before the next frame stamped `T495` and grouped under Turn 495 in the expanded
log; on an empty ring it is stamped with nothing at all.

The two orderings are not in tension — the clear stays first for the rollback reason above, and the
turn follows it — which is exactly why they live in one function rather than at two points of a
fan-out, where this ordering was wrong for the life of the feature.

`ui_preview` asserts it through `apply_event_dock_frame` on both ring states, reading `_current_turn`
and the stored row's `tick` rather than the rendered rows (the stamp is applied at ingest; a
render-scoped read narrows to whichever turn groups the log is drawing). Sabotage-verified by putting
`set_current_turn` back ahead of the clear: four assertions fail, and the fixture's
"the frame's own event is retained" premise correctly stays green, since it is a claim about the
retention window and not about the order.

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

## THE BAR RESERVES NOTHING — IT OVERLAYS THE MAP

It reserved `SIDE_TOP`/`SIDE_BOTTOM` at first, like `BandCityPanel`. **A reservation is FULL WIDTH,
and this strip is not**: it is bounded to the centre band and capped at `MAX_STRIP_WIDTH`, so the map
was pushed down across the whole window to make room for something that filled only the middle of it,
and the ends came back as bare background — black bars either side of the bar, reported from live
play. A notification strip is not furniture the game area has to make room for.

So the dock is **not a reserver at all**: no `MapView` inset, no `Hud` inset, no entry in
`_reservations`, no row in `RESERVER_PRIORITY`, and **no `reservation_changed` signal or
`current_reservation_size()` method** — the API is gone rather than publishing a zero, because a
zero-sized reservation and no reservation look identical from outside and the next reader would wire
one back by reflex. `MAP_ONLY_RESERVERS`, which existed only to keep this dock's strip off the HUD,
went with it.

**What did NOT change**: `_update_event_dock_insets` reads the OTHER reservers, so the horizontal
bound is untouched — the bar still starts past whatever is docked left and stops short of what is
docked right, plus the HUD's own columns.

### Overlaying costs two things a reserved strip never had to pay

- **It must eat its own clicks.** With no reservation, `MapView`'s hit-testing covers the whole
  viewport again, so a press on the bar would otherwise ALSO select the hex beneath it. `MapView`
  picks out of `_unhandled_input`, so a `MOUSE_FILTER_STOP` control over the pointer consumes the
  press first. Both `_root` and the card set it **explicitly**: `STOP` is the `Control` default, but
  this is the first element where that default is load-bearing rather than incidental, and an
  `IGNORE` added later for some hover effect would silently reintroduce click-through.
  `ui_preview` drives real presses through `Viewport.push_input` against its own `_unhandled_input`
  — **sampled across the whole rect, not just the centre**, because the centre lands on a row
  (a `PanelContainer`, `STOP` by default) and passes even with the root and card set to `IGNORE`.
- **It must be opaque.** Reserved chrome sits on the HUD's own background; an overlay sits on
  terrain, which can be snow or desert. The card fills with `HudStyle.PANEL_SOLID` (alpha 1.0), not
  the translucent `PANEL` every docked card uses. `event_dock_over_bright_terrain` is the frame that
  holds that to account.

## The strip is CAPPED and CENTRED, not stretched

On an ultrawide the bar spanned the whole band between the columns, so a row's label sat at one end
of two feet of screen and its detail at the other and the pair read as two unrelated things.
`MAX_STRIP_WIDTH` bounds it and the strip centres in whatever band is left. (That bound is also what
made the reservation untenable — see "THE BAR RESERVES NOTHING" above.)

**The number is chosen against two measurements and the larger wins.** The widest row the shipped
fixtures produce at the current font sizes is **537px** — a predator raid, `Grey wolves took two from
Ashfoot` beside `Wounded 1 · Warriors 3 · Grey Wolf` — which with the expander (86) and the card
chrome (31) needs **654**. (It measured 594/711 when this was first written, against a row that still
read `Killed 2.000 · …`; hiding the label's own `killed` and trimming the wire's `{:.3}` shortened it.
The **code comment on `MAX_STRIP_WIDTH` is the current figure** — re-measure there, not here, if the
font sizes move again.) But the band between the two HUD columns at the project's own
1920 base canvas is **1216**, and the ordinary case must render *unchanged*: a cap below that would
shrink the bar on every desktop to fix a complaint about ultrawides. So the cap sits just above the
base-canvas band at **1280** — no content is ever squeezed, nothing moves at 1920 or below, and past
it the strip stops growing.

`ui_preview` asserts BOTH halves — at the normal canvas the strip
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
- **The strip's cross-axis size reads only the preference, the expanded flag and the viewport** —
  never the event list. It is `recent_count` rows tall whether or not it has that many events. This
  is `BandCityPanel`'s rule, learned there as a map flicker on every `+` press; here an arriving
  event every turn would be a far worse offender. It outlived the reservation it was written for:
  a strip that resizes per event still shifts the rows the player is reading.

## On a shared edge the bar sits at the rim

**There is no priority row for this dock** — it is not a reserver, so `RESERVER_PRIORITY`
(`{inspector: 0, band_panel: 1}`) does not name it. The bar simply draws against its chosen edge,
which puts it outboard of anything docked there, and the band panel's position relative to the map
never changes when the bar grows a row because the bar takes no room from it.

> An earlier design DID reserve, at priority 0 so it would hug the edge, with the band panel offset
> inboard by `_update_band_panel_edge_offset()`. That is history — see "THE BAR RESERVES NOTHING" —
> and re-adding a row for `event_dock` would reintroduce the full-width reservation that shipped
> black bars.

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
- **It moves where the strip is DRAWN.** The dock reserves nothing to move, and the
  content-independence rule above still holds: the insets read reservations and authored column
  widths, never the event list.

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

### The bar never pushes the HUD down

The HUD keeps its full height: the readouts and the right dock sit exactly where they did before the
dock existed, because the bar lives BESIDE the HUD's furniture rather than above it.

Reported live in between: the bar reserved `SIDE_TOP` against the HUD, `LayoutRoot` absorbed it, and
everything shifted down. The fix at the time was `Main.MAP_ONLY_RESERVERS`, a list of reservers whose
strip reached `MapView` and not `Hud`. **That constant is gone** — the dock stopped reserving from
either surface a step later, and a one-member list for a member that no longer reserves is a
mechanism with nothing to do.

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
  chatter* is not: it fills the Logs tab's buffer AND rides out to the dock's System channel. The
  Commands tab that used to mirror it has since been deleted, so the dock is the only surface a
  player sees it on.
- **The three controllers that posted a client-side note take a `Callable` note sink** rather than a
  `CommandFeedController` reference (`TopBarReadouts`' knowledge unlock, `TurnOrbController`'s
  unanswered fork, `TargetingController`'s two quarry refusals). It resolves to
  `HudLayer.note_system_event` → the `system_note_requested` signal → `Main` → the dock. The HUD
  emits rather than reaching for a panel it does not own — the coordinator mediates, as everywhere
  else.
