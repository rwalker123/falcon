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
| `ui/EventDockPanel.gd` / `src/ui/EventDockPanel.tscn` | The dockable **event dock** CanvasLayer: a horizontal notification strip on `SIDE_TOP` or `SIDE_BOTTOM` that **overlays the map and reserves nothing** (see "THE BAR RESERVES NOTHING" — it is not a reserver, and it publishes neither `reservation_changed` nor `current_reservation_size()`). **What it does publish is `dock_changed(edge)`, `occupancy_changed(edge, extent)` and — its one PER-ROW signal — `band_work_tab_requested(band_id)`** (see "A cut row offers the way to what it cut"); the first two are the opposite direction from a reservation and neither is to be mistaken for one: the first says where the bar WENT, so `Main` can re-measure what displaces it there; the second says how deep it is DRAWN, for the free-floating cards that are placed by arithmetic and so cannot be drawn under it (see "Overlaying costs two things a reserved strip never had to pay"). It is bounded horizontally by the OTHER reservers plus the HUD's own columns via `set_perpendicular_insets`, and pushed inboard on its OWN axis past whatever reserves the edge it is docked to via `set_edge_offset` (see "On a shared edge the panel keeps the rim"). Two states — the COLLAPSED bar (`recent_count` rows, newest first, with the pinned-alert exception) and the EXPANDED turn-grouped log (World/System chips, the detail floor, the row count, the dock edge, "Earlier turns"). It accumulates `command_events` and de-duplicates on **`seq`**, prunes by TURN window against the sim's `command_events_retention_turns`, and takes client-side System notes through `note_system(label, detail, alert)`. Prefs live in a new `[events]` section of `user://narrative.cfg`, with a `config_path_override` static for the harnesses. Toggled by `R` (`Main._toggle_event_dock_visibility`) |
| `ui/hud/hud_event_vocab.gd` (`HudEventVocab`) | The importance model, as an ALL-`const` vocabulary leaf (`hud-modules.md`): `RUNG_BY_KIND` · `CHANNEL_BY_KIND` · `RUNG_STYLE` (glyph + `HudStyle` accent per rung) · `KIND_STYLE` (the threat/casualty kinds, absorbed from the retired `CommandFeedController`) · `DETAIL_STATUS_STYLE` (the `status=` token rule — accent AND rung, the only override above `RUNG_BY_KIND`) · `DETAIL_STATUS_WORK_LINK` + `WORK_TAB_LINK_TEXT` (which of those tokens offers the Work-tab jump) · `DETAIL_FLOOR` (the three player settings as a floor on the rung ladder) · **`IGNORED_KINDS`** (the kinds the dock drops at ingest — see "A kind the dock IGNORES") plus the two client-minted kinds `KIND_SYSTEM` / `KIND_COMMAND_ECHO` and the dock's word tables and glyphs. Reads only `HudStyle`, which reads nothing, so it cannot enter a class-load cycle |

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

**`band_founded` is ALERT, and `died` / `migrated` sitting at Notable is exactly why it has to be
said out loud** (issue #510). Notable is for things that happen to a band as a matter of course — a
death, a migration, a party reaching its objective (`expedition_arrived`, one table row away). A
founding is the opposite on every count: rare, player-initiated, and the first act in the band economy
that cannot be undone. The same kind carries the command's REFUSALS, which belong there too — a
refused irreversible order is as loud as a taken one.

**`trade_delivered` is NOTABLE, and it is the one expedition event that happens where OTHER PEOPLE
live** (arc #527). That novelty is what earns it a kind of its own sim-side; it is not what decides
its rung, because the ladder asks how LOUDLY, not how new. A shipment landing sits exactly beside
`expedition_arrived`: a party reached where it was going and did the thing it was sent to do, turns
after the player asked for it. Alert is for violence, for an investment lost and for an irreversible
player-initiated act — none of which a delivery is. **A REFUSED shipment is not this kind at all**:
a rejected command rides `system`, which is already Alert.

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
2. **`DETAIL_STATUS_STYLE`** — a rung going feral, an assignment dropped for want of people and a
   crew merely cut all ride their VERB's own kind (`cultivate` / `sow` / `forage` / `hunt`),
   deliberately, so a rung's whole life reads on one channel. That makes the LOSS the same kind as
   the COMPLETION before it, which `KIND_STYLE` structurally cannot separate. The sim's own
   `status=` token can, and it also **sets the row's rung** — see "The `status=` token sets the rung,
   and it is the only thing above the kind" below.
   **Matched as a whole space-delimited `key=value` fragment, never a bare substring** — the sim
   writes `"status=feral reason=untended …"`, and a substring test on `feral` would also fire on a
   species key or a tile label containing the word.
3. **`RUNG_STYLE`** — the default glyph + accent for everything else.

Only an Alert's LABEL carries its accent; a Routine one recedes to `INK_DIM` and a Notable one stays
on the shared ink. The rung reads off the rail and the glyph — tinting every label would turn the bar
into a colour chart and cost the alerts the contrast that makes them alerts.

## The `status=` token sets the rung, and it is the only thing above the kind

`_resolve_rung` asks `DETAIL_STATUS_STYLE` first and `RUNG_BY_KIND` second. The rung rides **inside
the style entry** rather than in a table beside it: two tables would be two memberships, and a token
added to one and forgotten in the other renders a row wearing the loss accent at its kind's own rung
— which looks perfectly right in any frame and is invisible at the player's floor.

| token | rung | mark | why |
|---|---|---|---|
| `status=feral` | Alert | `⚠` | a rung has reverted; the investment is gone |
| `status=lapsed` | Alert | `⚠` | the labor row was destroyed outright and its queued build went with it |
| `status=trimmed` | Notable | `▾` | the source is still worked, by fewer hands than the player set |
| `status=pruned` | Notable | `▾` | the crew still stands there; what it TAKES was narrowed |
| `status=stalled` | Notable | `▾` | the crafting bench lost its LAST hand; the job stops and keeps everything |

**Notable is the ladder's own answer for a crew that was merely cut.** Routine is *"bracket
transitions, and receipts for things the player asked for"*, and a cut crew is the opposite of a
receipt — the player asked for six and got three. Notable is *"the world changed in a way worth
knowing"*. Not Alert: this ladder puts a DEATH at Notable (see "The demographic kinds split on
HEAD-COUNT"), and a shed crew is a consequence of one.

**The two Notable tokens were silent for a release, and the absence is what the split fixes.**
`trimmed` and `pruned` ride `forage` / `hunt` / `cultivate` / `corral`, every one of which is
`RUNG_ROUTINE`, against a `DEFAULT_DETAIL_LEVEL` of `RUNG_NOTABLE` — so `systems::labor::
announce_shed_crew` announced a band going 6 → 3 to nobody on default settings, which reads from the
player's side as the number they had just set moving on its own.

### `stalled` is a THIRD Notable token, because neither existing one was true

The crafting bench joined the shedding order, and a short band that takes its **last** hand stops the
job — the recipe, the progress, the finished count and the **materials already drawn** all stay, and
re-staffing resumes where it left off. `systems::labor::announce_shed_bench` needed a token neither
existing one could carry:

- **`trimmed` says *the crew is smaller and the source is still worked*.** A bench at zero is not
  worked, so on the last hand that is false.
- **`lapsed` says *the row is GONE and its investment with it*, and is ranked ALERT for exactly that
  reason.** Nothing here is destroyed, so it would be false **and** would shout — about a state one
  command undoes.

So `stalled` ranks with `trimmed`: **Notable, `▾`, `WARN`**. That is the invariant above doing its
job rather than a second decision — the rung was chosen on the ladder's own words and the mark
followed from it.

**A bench that still has hands on it IS a `trimmed`**, in that token's own terms, and reuses it with
`kind=bench`. The third token exists only for the state neither describes, and the harness stages
both bench lines so the new one cannot quietly become every bench row.

⛔ **IT NEEDED A `DETAIL_STATUS_STYLE` ROW AT ALL BECAUSE `craft` IS NOT IN `RUNG_BY_KIND`** — so it
takes `DEFAULT_RUNG` (`RUNG_ROUTINE`), under the dock's own `DEFAULT_DETAIL_LEVEL`. Without the row a
craft crew disappearing announces itself to nobody, which is precisely the defect the `trimmed` /
`pruned` split was added to close one web over. **It also takes a `DETAIL_STATUS_WORK_LINK` row**: the
sim changed a labor row unasked, the bench is staffed from the Work tab like any other crew, and
`announce_shed_bench` writes the `band=` the jump needs.

### ⛔ THE GLYPH TRACKS THE RUNG, or the split is filter-only and unreadable

All four tokens wore `⚠` for a release. The rungs above were **right the whole time** and did real
work in filtering — and were **invisible on the line**, because two rows at two different rungs drew
the same mark in the same amber. Reported from play as *"losing hunts and scouts is an alert but
foragers are notable"*, which is not the rule at all: the rule is **trimmed-vs-lapsed**, and it only
LOOKS like a kind split because a scout usually stands one or two hands and lapses on the first shed,
where a forage row trims several times first. A player who cannot see the rule infers an arbitrary one.

So the mark is the **rung's**, not the status's, and both consts are named for their rungs
(`HudEventVocab.STATUS_SHED_GLYPH` / `STATUS_REDUCED_GLYPH`):

- **`⚠` is exclusive to the ALERT pair** — and to `RUNG_ALERT`'s own ladder style and the ALERT-rung
  `hunt_danger` accent, which are the only other `⚠` in this file. It means *something is wrong*
  everywhere else in this HUD; a mark that also rides a routine crew cut means nothing anywhere.
- **`▾` is the NOTABLE pair's** — one mark for both, because they are **one class**: a trim cuts the
  hands, a prune narrows what the hands still there take, two mechanisms and one sentence to the
  player (*you asked for more than you are getting*). A per-status pictogram would put the glyph back
  to tracking the MECHANISM, which is the thing this rule exists to stop.

**THE COLOUR DOES NOT MOVE, AND THAT IS HALF THE DECISION.** Both pairs stay `HudStyle.WARN`. The
glyph carries the rung; the amber carries *this is not good news* — a trim is still unwelcome and
still the player's to reverse, so demoting it would have traded an over-loud row for an invisible one,
which is the defect this dock was built to close (see "The two Notable tokens were silent" above).

**NOTHING ELSE MOVED**: no rung, no `RUNG_ORDER`, no `DETAIL_FLOOR`, no `DETAIL_STATUS_WORK_LINK`, no
filtering. It is a render fix for a ladder that was already correct.

**A status added later at `RUNG_NOTABLE` wearing `⚠` is the defect again**, and
`chapters/event_dock.gd` asserts the iff over the whole table rather than over the four tokens it
stages — a row-level claim would pass on exactly that addition.

### A cut row offers the way to what it cut

A row whose `status=` token is in `DETAIL_STATUS_WORK_LINK` (`trimmed`, `lapsed`, `pruned`,
`stalled` — a labor row the sim changed unasked; **not** `feral`, which changes a source and leaves
the work board alone)
draws a `Work tab` link and emits `band_work_tab_requested(band_id)`. It is the dock's **first and
only per-row signal**; every other one it publishes is about the strip.

- **The link appears only where the detail carries a `band=` token.** A jump has to name a band, and
  the client will not recover one by reading `foragers at (60, 0)` out of a rendered label —
  recovering data from prose is the drift the whole `key=value` contract exists to prevent. Absent,
  the row still renders, still at its Notable rung, with no link.
- **The labor system writes it, through one seam.** `systems::labor::band_detail_token` appends
  `band={id}` at `announce_shed_crew` and at the three lapse sites beside it (`out_of_range`,
  `herd_gone`, `out_of_leash`), and `server.rs`'s `status=pruned` line writes its own. It is the
  band's **durable `BandId`**, never the ECS entity — both are `u64`, which is the confusion
  `xtask/src/command_guard.rs` exists because someone once shipped.
- **A cohort with no durable id still emits its line and renders linkless**, which is the demographic
  feed's own rule rather than a fabricated `band=0`. The band-wide roles (`kind=scout` / `warrior` /
  `builders`) name no source at all, which is the second reason the band is stated rather than
  inferred from the row.
- **It carries a `band_id`, not an `entity`.** The dock's only handle on a band is the wire's durable
  `BandId`; `BandPanelController.show_work_tab` takes the client-local entity. **The roster is the
  only place the two meet**, so the join happens exactly once, in `HudLayer.show_band_work_tab`
  (`HudBandLaborState.player_band_by_band_id`) — the dock never learns an entity and the panel never
  learns a `band_id`. `Main` relays, as it does for every other dock signal.
- **The route is the compose sheet's, not a second one** — `jump_to_band_entity`, whose note forbids
  a second way to make a band the subject. Everything downstream of the join (jump only when the
  subject is not already that band, tab set AFTER the jump, an unresolvable band still gets the tab)
  is `show_work_tab`'s own contract; see `labor-ui.md` → "THERE IS NO CHECKBOX ANY MORE".
- **The link is a `Button`, and it must not be `clip_text`.** Clipping keeps a Button's text out of
  its minimum size, which beside a `SIZE_EXPAND_FILL` label means it is allocated exactly zero pixels
  — it shipped that way for one build, passing every count-and-press assertion while rendering
  nothing. A row's natural width is allowed to grow instead, which is what the unclipped detail label
  beside it has always done; `MIN_STRIP_WIDTH` is a stated constant rather than a reading of the live
  card, and `EventRows` clips its contents. `ui_preview` asserts a drawn width, not just a count.

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

### …but a detail the sim wrote as a SENTENCE is shown verbatim

**Every COMMAND REFUSAL takes that shape.** `emit_command_failure` puts the sim's own explanation in
the `detail` slot — *"Scouts 2 cannot start a life here — nobody at home could point at that place —
a founding site must join one of your bands across ground your people have mapped."* — and the token
walk splits on spaces, so it rejoined that sentence as a column of capitalised words separated by
` · `. `detail_phrase` therefore short-circuits on prose (`_is_prose_detail`).

**A SINGLE BARE TOKEN IS NOT PROSE**, which is half the test: `herd_gone` is an identifier and must
keep reaching the screen as `Herd gone` through the generic fallback. Nor is a detail carrying a
`key=value` fragment or a `(x,y)` coordinate anywhere in it — one token of the machine contract makes
the whole string one. So the rule is *more than one word, and no contract token anywhere*.

It surfaced with `band_founded`'s refusals (issue #510) and it was never specific to them: it fixes
every command failure the sim emits, all of which had been rendering that way.

**Assert the two as a PAIR** — `event_dock_band_founded` carries a refusal and a founding side by
side, and the token row is what proves the walk still runs for details that really are the contract.

**STILL OPEN — a band id in a DETAIL is not joined to its roster name.** That founding's detail
renders `… · Parent 71204 · …`: the `band=` join is `_row_label`'s and is LABEL-scoped, so a second
band id carried as a detail token reaches the player as a raw durable id. Closing it means teaching
`detail_phrase` which keys are band-valued and giving it the roster, which it cannot have today —
it is `static` and takes no roster. The same detail also renders its site as `X 39 · Y 26`, the
existing treatment of every `x=`/`y=` pair in the corpus rather than anything this kind does
differently.

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

Four details:

- **Resolved at RENDER time, not stamped at ingest** (`_row_label`), so a roster change relabels rows
  already held and a row that arrives before the first `set_band_labels` is not stuck with the
  fallback forever.
- **EVERY token that names a band is swapped, not just `band=`** (arc #527, `BAND_ID_TOKEN_LABELS`).
  A shipment's landing line names the receiving band through the sim's own fallback spelling and
  repeats the id as `destination=<id>`; without a second entry the feed would print one band's raw id
  beside the `Bound for Band 2` the parties strip prints for the same party — two surfaces naming one
  band differently, which is the defect the `band=` swap exists to remove. A table is what stops it
  regrowing once per producer, and each token is its own ROLE: a line can name the sender as `band=`
  and the receiver as `destination=` at once, and sharing a key would rewrite one with the other's
  name.
- **A SECOND TOKEN NEEDS A SECOND FORMAT STRING, and the two shipped differ in CASE.** The sim writes
  a band as `Band 3` (`systems::population::band_label`) and a shipment's destination as `band 30`
  (`ExpeditionMission::destination_display`'s last-resort tier, which is the normal path today
  because bands have no names). `SIM_DESTINATION_LABEL_FORMAT` is therefore separate from
  `SIM_BAND_LABEL_FORMAT` rather than a reuse — a shared format would have looked right and never
  fired, which is exactly the silent no-op `SIM_BAND_LABEL_FORMAT`'s own note warns about.
- **The substitution stops at a DIGIT BOUNDARY.** A plain `String.replace` of `Band 3` finds the
  `Band 3` inside `Band 30` first and corrupts the label to `Band 10`. The sim names exactly one band
  per label today, so no live event reaches that trap — which is why `ui_preview`'s fixture
  CONSTRUCTS it (`"Four left Band 3 for Band 30"`) rather than quoting one. It also pins the honest
  limitation: only the band a token NAMES is substituted; a band in the label that no token names
  keeps whatever the sim called it.

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
docked right, plus the HUD's own columns. **Both of the bar's bounds are read FROM the reservers and
neither is contributed to them** — the perpendicular one here, the bar's own axis in
`_update_event_dock_edge_offset` (see "On a shared edge the panel keeps the rim").

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
- **It must say how deep it is drawn** — `occupancy_changed(edge, extent)` and `occupied_extent()`.
  A container draws a docked panel underneath the bar for free; a FREE-FLOATING card places itself by
  arithmetic against a rect and is drawn *through* it, which is how the Materials & Crafting header
  came to be rendered underneath a top-docked bar. `Main` relays the signal to `Hud.set_overlay_inset`,
  which shrinks **only** the free-floating room (`panel-framework.md` → "An overlay is a second kind
  of neighbour"). **This is not a reservation growing back and must not be turned into one**: nothing
  here enters `_reservations`, `MapView` is untouched and the HUD's own layout does not move — the
  map still renders under the strip, which is the whole point of the section above.
  `extent` is the strip's own depth PLUS `_edge_offset`, i.e. absolute from the screen edge, so a
  reader can inset a rect by it without also knowing what displaced the bar; it is 0 while suppressed,
  and it does NOT drop when the bar is empty, the depth being content-independent by design.
  It is emitted from `_apply_dock_layout`, the one choke point the edge, the displacement, the row
  count, expanding, suppressing and the viewport's own resize already run through.

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

### …and FLOORED, which is the same measurement read from below

The cap's rule is "no content is ever squeezed", and **nothing was enforcing it downward**. The strip
takes whatever the perpendicular insets leave, and those are three FIXED logical widths — a docked
panel's reservation plus the HUD's two authored columns (360 + 344) — so the band collapses as the
logical viewport shrinks. Measured with the Band panel docked LEFT at `ui_scale` 1.35: a 1422px
viewport, insets 740 / 344, **a 338px band**, against a card whose own combined minimum is **406**. The
card drew 68px outside the strip it was given and `EventRows` (`clip_contents`) silently cut its
labels. As with the panel above this surfaced through issue #490 but is not a scale defect — the same
arithmetic reaches it at 1.0 in a window near 1200px.

**`MIN_STRIP_WIDTH` is the CARD's minimum, not the widest ROW's.** It is **407** —
`CARD_BAR_MIN_WIDTH` (387) + `CARD_CHROME_WIDTH` (20), measured off the live card rather than summed
from a fixture. The cap is a different measurement (widest shipped row 537 + expander 86 + chrome 31
= **654**), and the two must not be conflated.

It was 654 briefly, on the reasoning that the floor and the cap were "one measurement read from both
ends". That is wrong, and it cost a defect: the failure the floor prevents is the card overflowing
the strip it was given, and the card's minimum is 407, so setting the floor at 654 forced an
**overhang across the whole `[407, 654)` band where none was needed**. At a 1200px logical viewport
with nothing docked the band is 1200 − 360 − 344 = 496; `clampf(496, 654, …)` yielded 654 and hung
the bar 79px over each HUD column — the exact overlap `Main._update_event_dock_insets` exists to
prevent. In that band the strip both fits inside its insets and clears the card; the only cost is
`clip_contents` truncating the longest label. **A symmetrical-looking derivation is not evidence:
ask what the bound is protecting, and measure THAT.**

**Below the floor the strip OVERHANGS its insets rather than clipping its rows**, symmetrically about
the band it was offered, then clamped inside the viewport. `band - width` goes negative exactly when
the floor binds, so the one expression that centres a capped strip in a wide band overhangs a floored
one. The floor also yields to the window (`minf(MIN_STRIP_WIDTH, window.x)`): a viewport narrower than
one row has nowhere to put the overhang, and a strip hanging off the screen edge loses the same text
the floor exists to save.

**That overhang is a deliberate trade, and below 407 it is the only one available.** A band under the
card's own minimum has no arrangement in which the bar both clears every HUD column and states a row,
so the choice is between overlapping a column and printing an unreadable sliver — and a bar that
cannot be read has stopped being a notification. It is the one place the strip's "clears every
column" property is knowingly given up, and the band it is given up over is now as narrow as the
card's minimum allows. **Above 407 rows clip instead — clipping a long label is acceptable, drawing
over a dock column is not.**

`event_dock_narrow_band` is the frame that holds this: it stages the 496px band by reservation and
asserts the strip's ends against the **viewport and the insets**, never against `MIN_STRIP_WIDTH`. An
assertion phrased in the floor's own terms would have stayed green through the whole defect.

### …and a FLOOR IS ONLY HONOURED IF NO ROW OUTGROWS IT

The floor above says what the narrowest strip is; it says nothing about whether the CARD fits inside
one, and for a long time a row could demand more than the whole budget. Reported from play, with the
overhang drawn across the Telling card: `Engaged 0.34 · Fled 0.068 · Carried biomass 0 · Wasted
biomass 0 · Hunters killed 0.007 · Hunters wounded 0.02 · Fight · Wild Aurochs`.

**IT WAS NOT THE STRIP BEING COMPUTED TOO WIDE.** `Main._update_event_dock_insets` already adds
`Hud.right_column_width()` to the right inset, so the strip's computed right edge lands just left of
the right dock, and every assertion about that edge was green. What overflowed was CONTENT, along a
chain in which every link is doing what it was built to do:

1. `_make_event_row`'s MAIN label clips (`clip_text` + `OVERRUN_TRIM_ELLIPSIS`); the DETAIL label
   beside it did not, and an unclipped `Label` reports its whole unwrapped string as its minimum
   width — **824px drawn** for the phrase above, measured.
2. That becomes the ROW's minimum, and so `EventRows`'.
3. In the expanded log it also climbs through `_log_scroll`, whose `horizontal_scroll_mode` is
   `SCROLL_MODE_DISABLED` — a `ScrollContainer` propagates a child's minimum on the axis it does not
   scroll.
4. The `PanelContainer` is a `Control`, so it is clamped UP to that minimum, and `_root` — a plain
   `Control` with no `clip_contents` — lets it hang out over whatever is drawn beside the strip.

**THE BOUND IS DERIVED FROM `MIN_STRIP_WIDTH`, because that is the width the card has to honour.**
`EventDockPanel.DETAIL_MAX_WIDTH` = `ROW_BUDGET_WIDTH` (407 − 20 chrome − 89 expander = **298**) less
`ROW_FURNITURE_WIDTH` (**154**, measured) less `DETAIL_CAP_SLACK` (8) = **136**. Measured after: the
widest row demands 290 of its 298 and the card 399 of the 407 floor.

- **`ROW_FURNITURE_WIDTH` IS MEASURED ON A ROW THAT CARRIES A `Work tab` LINK, which is the widest
  furniture a row can have.** It was 80 first, measured on a `died` row (70) and a `hunt` row (79) —
  **neither of which mounts a link.** `_make_event_row` appends `_make_work_tab_link` to the same
  `HBoxContainer` for any row whose `status=` is one `DETAIL_STATUS_WORK_LINK` names and which
  carries a `band=` id, and that control is deliberately NOT `clip_text`, so its whole word is in the
  row's minimum: 154 = a link-less row's 79 + the link's own 66 + one `ROW_ITEM_SEPARATION`.
  Budgeted at 80, the reported shed row (`foragers at (44, 18) cut to 1 — too few workers`) demanded
  364 against 298 and dragged the card to 473 against the 407 floor — **the same overflow, still open
  for the exact row the bug was reported on.** The two link-less figures differ by 9px between kinds
  as well, since `GLYPH_COLUMN_WIDTH` is a `custom_minimum_size` floor rather than a clip and an
  emoji kind glyph draws past it, so the figure must come from the widest row either way.
- **Lowering the cap to 136 costs nothing on an ordinary strip**, because the cap bounds what a row
  may DEMAND rather than what it draws: the growth half below hands the label whatever slack the
  strip really has. What it changes is only which rows take the growth branch — and at the floor,
  where there is no slack, a link-bearing row now fits instead of dragging the card outside its strip.

#### ⛔ The natural width is measured in the units the budget is derived in

`_make_detail_label` compares a phrase's natural width against `DETAIL_MAX_WIDTH`, a figure derived
entirely from DRAWN measurements — so the natural width has to be a drawn one too, and
`Label.get_minimum_size()` is not. **A `Label` shapes its text against its THEME CACHE, and that
cache is filled on `NOTIFICATION_THEME_CHANGED`, which a control that has never entered the tree has
never received.** A detached `Label` therefore measures at the default theme's `font_size` (16)
whatever `add_theme_font_size_override` was handed: `Cold` reports 33 against the 27 it draws at
`DETAIL_FONT_SIZE`, and the reported hunt phrase reports 1015 against the 824 it draws.

Left as it was, a phrase whose drawn width is comfortably inside the budget could measure over it,
take the growth branch, and gain a `clip_text` and a minimum it does not need — contradicting "a
detail that fits is returned untouched" below, which is a pixel claim. `EventDockPanel.natural_label_width`
asks the FONT instead (`get_theme_font` / `get_theme_font_size` resolve through the theme chain
rather than the cache and are correct while detached, and `Font.get_string_size` matches the drawn
`Label`'s own minimum to the pixel). Both of a row's naturals go through it, so the two are weighted
on one scale; `event_dock.gd`'s probes call it too, which is what let the chapter's short-detail
claim go back to a WIDTH comparison after the skew had forced it down to a flags check.

### ⛔ …AND THE BOUND IS A FLOOR ON THE ROW'S MINIMUM, NOT THE WIDTH THE PHRASE IS DRAWN AT

A `custom_minimum_size` alone is a hard COLUMN: an `HBoxContainer` hands a non-expanding child
exactly its minimum, so the first build of this fix ellipsised a long phrase at **every** strip
width. The reported hunt row read `Engaged 0.34 · Fled 0.068 · Ca…` on an otherwise near-empty
1280px bar — beside a main label (`The <species> hunt`, from `expeditions.rs hunt_report_event`)
that wants about 145 of it. Bounding what a row DEMANDS and choosing what it DRAWS are two
decisions, and only the first belongs to the constant.

So the label also carries `SIZE_EXPAND_FILL` and `HORIZONTAL_ALIGNMENT_RIGHT`. Slack in the row
flows back into it, and the right alignment is what keeps the glyphs where they were: unexpanded the
box was exactly its text and the main label's own expansion pushed it against the row's trailing
end; expanded, the box grows LEFTWARD while its right edge stays put.

**A DETAIL THAT FITS IS RETURNED UNTOUCHED, and that is a pixel claim rather than tidiness.** Its own
minimum is its whole text, so it can neither push the row past its share nor ever need trimming, and
every flag would be inert — but not free: **`clip_text` alone changes the `Label`'s draw path enough
to re-antialias glyphs that have not moved.** Measured, that was the difference between eighteen dock
frames and byte-identical.

#### The two labels are weighted by what each WANTS, never by what each LACKS

⛔ **`BoxContainer`'s arithmetic is the whole reason.** An expander's final size is
`available × (its ratio / total ratio)` — **the share IS the size, not a bonus added to its
minimum** — and a child whose share comes out below its own minimum is dropped from the stretch pool
and handed exactly that minimum.

Weighted by "unmet need" (`natural − cap`) the detail carries a large minimum beside a small ratio,
so its share fell under 210 and it was dropped **every time**: measured on the shipped default frame,
a row with 828px of slack to give drew `Wounded 1 · Warriors 3 · Grey Wolf` as `… · Grey …` while the
label beside it took 813. That is the hard-column defect again, reached from the other side, and it
is invisible to every claim phrased as "wider than the cap".

Weighted by each label's **natural width**, both are allotted
`available × natural / Σ naturals` — so when the row can pay for both, which is nearly every row on
nearly every strip, each clears its own natural and BOTH draw in full. When it cannot, both give up
the same FRACTION rather than one being trimmed to nothing, and a detail squeezed under its bound
falls back on `DETAIL_MAX_WIDTH`. On this canvas the reported row now clears the ~1107 a row is
given — the phrase is 824 drawn rather than the 1015 a detached probe reported — so the pair is
pinned on the MEDIUM row, which fits at every ordinary width, and on the floored strip, where the
shortfall is real and both labels give up the same fraction of it.

**A CONSTANT RATIO CANNOT EXPRESS THIS**, which is why it is computed per row: the weighting is a
property of what the row holds, and the dock ships both shapes (a short label beside a long phrase,
and a long label beside a short one) — they want opposite constants.

**Both naturals are read BEFORE their labels are clipped**, since a `clip_text` label reports the
one-pixel floor. `STRETCH_WEIGHT_FLOOR` covers the empty label, so a row's ratio total is never zero.

**THE LOG'S FOOT WAS THE SECOND, INDEPENDENT SOURCE, and it was found by measuring rather than by
looking.** `_log_foot` was an `HBoxContainer`, so its minimum was the `Earlier turns` button PLUS the
retention sentence — **399** with the log open, 12px past what the floor leaves the card, on a
sentence whose digits grow with the retention setting. It is an `HFlowContainer` now, the HEAD's own
treatment: a flow container's minimum is its WIDEST CHILD rather than their sum (**277**), and a
squeeze wraps the sentence under the button instead of widening the card. Nothing is lost and nothing
moves at any ordinary width.

**The other candidates were measured and left alone**: `_log_head` is already an `HFlowContainer`
(75), the per-turn group heads are one short `Turn N` at the smallest type size in the dock, and
`_make_message_row`'s longest string (`All events — turns 91–91`) is well inside a row's budget. The
`Work tab` link keeps its deliberate non-`clip_text` — a clipped `Button` beside an expanding label
is allotted zero pixels and renders as nothing at all, and its header's reasoning is untouched. **The
fix for it was the BUDGET, not the link**: its column is in `ROW_FURNITURE_WIDTH` now.

`event_dock_long_detail` / `event_dock_long_detail_log` / `event_dock_long_detail_floored` are the
frames. Each asserts the DRAWN card inside `_root` **and** the card's combined MINIMUM inside
`MIN_STRIP_WIDTH` — the rects say what this window did, the minimum says what the card would demand
of the narrowest strip it can ever be handed, and only the second survives a change of viewport.
Three things keep those claims from being vacuous, and each was a real gap first: a SHORT detail on
the same frame is asserted to draw its whole phrase (or the cap could be a fixed column trimming
every row in the game); one staged row is asserted to MOUNT a `Work tab` link (or the block proves
nothing about the row shape the bug came from); and `ROW_FURNITURE_WIDTH` is asserted to cover the
widest furniture actually measured on the bar, so a row that grows a control fails on the term that
went stale rather than on a card rect three assertions later.

## The strip yields to the map, and the reservation never depends on content

Two separate rules, both learned elsewhere in this HUD:

- **`_cross_axis_size()` is clamped to `MAX_STRIP_HEIGHT_FRACTION` of the window** and the log
  scrolls internally past it — the same bound `DockScrollFit` put on the command feed, for the same
  reason: a log that can eat the viewport has stopped being a notification. This is the prototype's
  second finding. `ui_preview` asserts BOTH ways the dock can grow (the widest bar with the log
  closed, and the log open, which collapses the bar to one title line) against the cap, as a pair —
  they are alternatives rather than addends, so neither is the worst case by inspection.

  **That clamp is measured against the WHOLE viewport, and on a shared edge the displaced strip
  therefore reads `[_edge_offset, _edge_offset + cross]` with nothing bounding the pair** —
  `BandCityPanel.MAX_WIDE_HEIGHT_FRACTION` (0.6) and this one (0.5) sum to 1.1 of the window. **What
  holds the line is that neither fraction is ever the binding term** — but the margin by which that
  is true is now thin, and both of the premises it used to rest on have been retired.

  Both heights are still dominated by absolute caps. The strip's is unchanged: a one-line title bar
  + `LOG_HEIGHT` + the section gap = **304**. The panel's is no longer `PANEL_HEIGHT_WIDE` — a
  horizontal dock sizes itself as `_horizontal_panel_height()` = `_body_budget() +
  _shell_chrome_height()`, whose worst case is the ONE-column body (360) plus the narrow shell's tab
  bar, i.e. **395**. The two-column body is the shorter of the two (335), so the tall case is the
  narrow window, which is also the case the co-edge pair cares about.

  **And the viewport floor of 1080 is gone.** It used to hold because `project.godot` stretches
  `canvas_items` from a 1920×1080 base with an `expand` aspect, so a short WINDOW yielded a WIDE
  canvas and never a short one — a 1500×500 window laid out at 3240×1080. The interface-scale
  setting divides that: `content_scale_factor` is `ui_scale`, so the logical viewport is the stretch
  result over the scale, and at `UI_SCALE_MAX` (1.50) the floor is **720**, not 1080. See
  `interface-scale.md`.

  Worked at the worst case — `ui_scale` 1.5 on a 16:9 window gives a 1280×720 logical viewport; a
  TOP dock's span is 1280 − 704 = 576, under `wide_shell_min_width()`, so the panel is in the narrow
  shell at 395; a co-edge dock with the log open is 304. The pair occupies **699 of 720**. Neither
  fraction binds (0.6 × 720 = 432 > 395; 0.5 × 720 = 360 > 304), so the invariant still holds — on
  **21px** of slack rather than the 416 the old derivation enjoyed. A further ~20px in `LOG_HEIGHT`
  or in either body budget overflows it.

  **`event_dock_co_edge_expanded` cannot see this.** It runs at `ui_scale` 1.0 on a 1152-px harness
  canvas and prints 488 of slack, which is a true number for the frame it measures and not the worst
  case. The frame is scale-blind by construction. So the printed slack is evidence about the
  unscaled layout only — do not read it as headroom for the pair.

  **A repro stated in window pixels is still not a repro** — nothing in either panel's layout sees
  the window; it sees the stretch result divided by the scale.
- **The strip's cross-axis size reads only the preference, the expanded flag and the viewport** —
  never the event list. It is `recent_count` rows tall whether or not it has that many events. This
  is `BandCityPanel`'s rule, learned there as a map flicker on every `+` press; here an arriving
  event every turn would be a far worse offender. It outlived the reservation it was written for:
  a strip that resizes per event still shifts the rows the player is reading.

## On a shared edge the panel keeps the rim and the bar is DISPLACED

Reported from live play with a screenshot: with the Band/City panel and the bar on the SAME edge,
the bar drew straight over the panel. The perpendicular insets below cannot reach it — LEFT/RIGHT is
a different axis — and neither can `RESERVER_PRIORITY`, which is the reservers' stacking order and
which this dock must not join. So the bar's OWN axis gets the treatment `BandCityPanel.set_edge_offset`
already gives the panel: **`Main._update_event_dock_edge_offset` sums every reserver currently on the
edge the bar is docked to and pushes the total to `EventDockPanel.set_edge_offset`**, which starts
the strip that far in from its own rim (`_apply_dock_layout` writes `near`/`far` against both anchor
branches — the top and bottom cases are separate arithmetic, which is why the harness asserts each).
The panel keeps the screen edge; the bar sits BELOW it on a top dock and ABOVE it on a bottom one,
and the strip is pushed inboard, never shrunk.

**DISPLACED IS NOT RESERVING, and the distinction has to survive.** The dock still takes no space
from the map or the HUD, still has no entry in `Main._reservations`, and still has no row in
`RESERVER_PRIORITY` (`{inspector: 0, workbench: 0, band_panel: 1}`) — the offset is a one-way read
OFF that table, not a row in it. **That is also why the sum needs no priority test where
`_update_band_panel_edge_offset` does**: the band panel is itself a reserver and has to know which
co-edge reservers sit inboard of it, whereas the dock occupies nothing, so nothing can ever stack
against it and it is by construction the innermost thing on its edge.

**`dock_changed(edge)` exists because that absence WAS half the bug.** A dock chip moves the bar to
the other horizontal edge, which changes WHICH reservers it must clear — and nothing in
`_apply_reservation`'s fan-out can see it, since no reservation changed. `Main` connects the signal
to `_on_event_dock_dock_changed`, which re-runs the measurement; the offset is otherwise recomputed
on **every** `_apply_reservation` (a co-edge panel arriving, moving, collapsing or hiding) and seeded
once at connect time, exactly like the perpendicular insets. The edge rides the signal for
legibility, but the measurement re-reads `get_dock()` — ONE reader, so the offset can never be
computed against a stale edge.

> An earlier design DID reserve, at priority 0 so it would hug the edge, with the band panel offset
> inboard by `_update_band_panel_edge_offset()`. **That reservation is history** — see "THE BAR
> RESERVES NOTHING" — and adding a row for `event_dock` would reintroduce the full-width reservation
> that shipped black bars. What survives from it is the opposite arrangement: the panel holds the
> rim, and the thing that reserves nothing is the thing that moves.

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
are AUTHORED" has the table and the reasons). The right side used to be two regions in one column —
the dock and, above it, the readout block, so `right_column_width()` was the wider authored minimum of
the pair. **The readout block is retired (issue #450)**, so it is one region now; the block is still
why the rule is authored rather than measured, having had no minimum of its own and been pure text
width, which is precisely the measurement that must not decide a panel's edge.

### The bar never pushes the HUD down

The HUD keeps its full height: the right dock sits exactly where it did before the
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

**The CO-EDGE displacement is pinned the same way, and for the same reason: an overlapping strip
renders a perfectly plausible bar**, which is exactly how it reached live play. Five frames —
`event_dock_co_edge_top` / `event_dock_co_edge_bottom` (both edges, since the two branches of
`_apply_dock_layout` write different offsets against different anchors, so a fix reaching only
`SIDE_TOP` must fail) / `event_dock_co_edge_collapsed` / `event_dock_co_edge_control` /
**`event_dock_co_edge_expanded`** (co-edge TOP with the log OPEN — the other four are all the
COLLAPSED bar, so the tallest displaced strip had never been rendered or measured; it carries
`_assert_strip_within_viewport`, the far-edge claim described under "The strip yields to the map")
— each judged
as a **rect non-overlap against a real `BandCityPanel`**, again behind a negative control taken
first on the same two live nodes (at zero offset the rects genuinely DO overlap). Two of them are
the claims a naive fix would pass: **collapsing the panel brings the bar back down with it** (the
offset is a live read of what the panel currently reserves — 360 open, 46 railed — not a latched
per-edge constant, and a bar that stayed put would strand a band of dead map), and **a panel on the
OTHER horizontal edge displaces the bar not at all** (without which an offset that summed every
reserver regardless of edge would pass everything else). `_assert_bar_clears_co_edge` guards
vacuity on the **horizontal** band — the opposite axis to `_assert_bar_clears`, since two things on
one horizontal edge share a vertical band for free, and the strip is centred and capped, so a panel
narrower than the gap either side would make the claim about nothing. `Main` is never instanced in
the harness, so the chapter restates the sum (`_preview_push_event_dock_edge_offset`) — reading the
live panel's own `get_dock()` / `current_reservation_size()`, and with no priority test, matching
`Main`.

**Each clearance claim is made where it BITES**, through `_assert_bar_clears`, which refuses to pass
on a pair that shares no vertical band. The HUD's regions sit in different bands — a bottom bar is in
the `BottomBar`'s (nav backing, turn orb), and a top bar in the `ContentRow`'s — which since issue #450
deleted the `TopBar` is where the two DOCKS begin, so the TOP claim is made against the RIGHT DOCK
where it used to be made against the readout block above it. "These rects do not overlap" is true for
free of most pairs, and the first version of that block passed with the fix reverted. The
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
  `CommandFeedController` reference (`FactionReadouts`' knowledge unlock, `TurnOrbController`'s
  unanswered fork, `TargetingController`'s two quarry refusals). It resolves to
  `HudLayer.note_system_event` → the `system_note_requested` signal → `Main` → the dock. The HUD
  emits rather than reaching for a panel it does not own — the coordinator mediates, as everywhere
  else.

## The material half's two kinds (`docs/plan_standing_upkeep.md` §4.9 item 12)

**NEITHER COULD HAVE INHERITED A RUNG, AND ONE OF THEM DOES NOT TAKE ITS KIND'S.** `kit_life` and
`material_shortfall` are the sim's two new `CommandEventKind`s, and importance is resolved entirely
client-side — **no schema change**.

- **`material_shortfall` → `RUNG_ALERT`, and IT NAMES THE BAND.** An investment is about to start
  coming apart for want of a good, which is the Alert rung's own description. The sim edge-gates it on
  `LaborAllocation::material_shortfall_warned`, so the rung never means "every turn".
- **`kit_life` → `RUNG_NOTABLE` in `RUNG_BY_KIND`, and that entry is the FALLBACK.** The line's real
  rung is the `life_readout` SEAM it crossed, which the sim writes as `severity=warn|danger` —
  resolved by `snapshot::crafting::life_severity` off `equipment.json`'s own `warn_fraction` 0.34 /
  `danger_fraction` 0.10. ⛔ **No threshold is invented on this side.** The quieter of the two is the
  honest default for a line carrying no token at all.

**THE SPLIT RIDES `DETAIL_STATUS_STYLE`, NOT A PARALLEL TABLE.** That table already overrides a kind's
rung from a whole space-delimited `key=value` fragment, which is exactly the job — so `severity=warn`
(Notable, `▾`) and `severity=danger` (Alert, `⚠`) are rows in it, and the table's own rule that **the
glyph tracks the rung** covers them for free. `_detail_status_key` matches whole fragments, so a
`severity=` token can only match the field it names. It is why that table's contract is `key=value` and
not `status=`.

**`status=outrunning` IS IN THE TABLE TOO, AND IT AGREES WITH ITS KIND RATHER THAN OVERRIDING IT.**
`material_shortfall` is already Alert; the row exists for the two things only a member of that table
gets — the `⚠` its rung names, and eligibility for `DETAIL_STATUS_WORK_LINK`, which
`_detail_status_key` returns no token for otherwise. **Warn amber, not the raid crimson**, exactly as
`feral` and `lapsed`: a loss the player can still head off by crafting or by holding less.

> ### ⛔ WITHOUT THE `Work tab` LINK THE MATERIAL ALERT NAMES NO BAND AT ALL
>
> The sim's label is *"Hurdles is running out"* — no band in it — so `SIM_BAND_LABEL_FORMAT` has
> nothing to rewrite, and `band` sits in `DETAIL_KEY_HIDDEN` on the premise that the label already said
> it. **This kind exists to replace the faction `Gear` row's `⚠ 1 band` → *which band* drill-down,
> which was a JUMP**, so the link is not a convenience: without it the discovery path would have been
> deleted rather than moved. `announce_material_shortfall` writes `band=` as the durable `BandId`.
>
> **The Work tab is the right destination rather than merely an available one.** The bill is what this
> band's improvements demand, and what the player can DO about it is on that tab: staff the bench that
> makes the good, or stop holding a rung. A `kit_life` row deliberately offers NO such jump — a worn
> spear is not a labor row the sim changed unasked, and a link mounted on every new kind means nothing.

**Both kinds are stated in `CHANNEL_BY_KIND`** rather than left to `DEFAULT_CHANNEL`, for the reason
that table gives about `KIND_COMMAND_ECHO`: a channel is a fact about the kind, and these two are the
ones most easily mistaken for client chatter, since the remedy for both is something the PLAYER does.

**Frame:** `event_dock_material` (`chapters/event_dock.gd`) carries all three rows at once — the whole
claim is that the two kit seams read APART, and a split asserted one row at a time passes on a client
that files every `kit_life` line at one rung.

## ⛔ A DEATH'S RUNG IS ITS BRACKET, and half of the old invariant was reversed to get there

`died` was pinned at `RUNG_NOTABLE` for every bracket. The justification beside it was written
**entirely about elders** — *"bands lose elders to cold as a matter of course, and a rung that
interrupts for every one of them trains the player to stop reading the bar"* — and never made a case
for workers or children, because there is none: a worker dying is labour lost whatever killed them,
and a child is the band's next worker. It then leant on an escape hatch: *"a death that MATTERS (a
band starving out) announces itself through the starvation and morale channels that already exist."*

**Issue #614 is the case that disproves the escape hatch.** Temperature mortality is FOOD-INDEPENDENT
and leaves morale clamped at 100 %, so a band freezing its workers away announces itself through
neither channel — the count fell and the bar carried one Notable line the player had every reason to
skim.

So the kind SPLITS ON THE `bracket=` TOKEN, riding the mechanism `kit_life`'s `severity=` split
already established (`DETAIL_STATUS_STYLE`, matched on a whole space-delimited fragment):

| `bracket=` | rung | why |
|---|---|---|
| `elder` | **Notable** — unlisted, falls through to the kind | unchanged, and deliberately so until an elder means something mechanically |
| `working` | **Alert** | labour lost, whatever killed them |
| `child` | **Alert** | the band's next worker |

**The tokens are the sim's own** (`systems::population`'s `DeathBracket` — `child` / `working` /
`elder`, written by the single `bracket=` site in the workspace). Invent a fourth spelling and the
split silently stops matching.

**These two rows wear `HudStyle.DANGER`, not the `WARN` every other row in that table carries.** The
amber ones are an investment lost or a crew cut, which the player can still reverse; a person is
dead. The glyph tracks the rung exactly as the table's own rule requires — `⚠` on the promoted pair,
and the elder row keeps the ladder's Notable `✦`, which is what makes the split visible rather than
merely filtered.

**The elder row is the CONTROL in `ui_preview`, not decoration.** Without it, promoting the whole kind
passes every other claim in that state and throws away the reason the kind was Notable in the first
place — and the run proves it: promoting `died` outright also broke a pre-existing pinned-alert
assertion elsewhere in the suite.

### `cause=heat` — the temperature term's other tail needs its own word

The sim grew `DeathCause::Heat` when the temperature term became two tails, so a death on hot ground
reports `cause=heat` and labels "3 workers died of heat in Band 1". **It rides the `died` event's
detail string, so there is no schema change** — but the client's `DETAIL_VALUE_LABELS` had `cold` and
not `heat`, and an unlisted value falls through to `_english`, which capitalises. The row would have
read `Heat` in the middle of a sentence: legible, and not English.

So `"heat": "heat"` sits lower-case beside `"cold"`, for the reason the whole lower-case cluster
there exists — **the phrase CONTINUES the label, it does not head a column.** `ui_preview` asserts the
rendered phrase both ways (contains `heat`, does NOT contain `Heat`), because a check for presence
alone is satisfied by the capitalised fallback.

> #### ⛔ A `cause=heat` OR `cause=cold` ROW IS NEVER AN ELDER, AND A FIXTURE MUST NOT MAKE ONE
>
> Elders always report `cause=age` on a lethal tile: the flat old-age term (20 %/turn) outweighs even
> the elder-weighted temperature ceiling (15 %), so the temperature causes can only ever appear on
> **child and worker** rows. That is pre-existing and correct — it is what stops a band burying its
> elders from reading as a weather report — and it is **not** to be special-cased. It only matters
> here because an elder fixture written to exercise this vocabulary would fail for a reason that has
> nothing to do with it.
