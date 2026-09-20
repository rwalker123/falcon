---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/KnowledgePanel.gd"
  - "clients/godot_thin_client/tools/ui_preview/fixtures_knowledge.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/KnowledgePanelController.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/KnowledgeRoster.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/hud_knowledge_vocab.gd"
  - "clients/godot_thin_client/tools/ui_preview/chapters/knowledge_panel.gd"
---

# The knowledge screen — what your people know

`docs/plan_knowledge_screen.md` §3 and §4. Its own free-floating surface, launched from the Band/City
panel header's action bar beside the `⚒`, showing every knowledge the faction has, is learning, or has
not begun — and the ones it has earned and is not using.

The problem it answers: the intensification ladder's knowledge was earned by practice, announced once
into the event dock's System channel, and otherwise invisible. A player was never told a track
finished, never told what it lets their hands do, and never told they were sitting on a discovery they
had not spent. **The announcement now lands on the TURN ORB instead, and the orb's rows open this
screen** — see `turn-orb.md`, and the entry point below.

## IT IS A READING, NOT A PLANNER — and that is the review question for any change here

No queue, no research order, no pathing, no "next" button, nothing clickable in the tech-tree sense.
A discovery is earned by PRACTICE — you get Penning by keeping tamed herds — so a screen that offered
a plan would teach the exact opposite of how the game works. Selecting a node opens a reading of it
and nothing else.

**The one thing every change to this panel has to be asked: does it read as somewhere you SPEND
something?** If it does, it has taught the wrong thing.

## THE PLAYER-FACING WORD IS *IMPROVE*, NEVER *CLIMB A RUNG*

"Rung" and "climb" are the intensification arc's INTERNAL vocabulary — `RungGates`, `SourceForecast`
and `docs/plan_intensification_ladder.md` keep them, and should. They do not survive contact with a
player: a hex asked to "climb a rung" is a metaphor the game never taught. Nothing in
`HudKnowledgeVocab` says either word, and that is a rule rather than an accident — it is also why the
overlay channel this arc's slice D shipped is `ready_for_improvement` and not the `ready_to_climb` it
was designed under.

## The three node states, and why the third is DRAWN

`known` · `learning` (0..1) · `not begun`. **A track at `0.0` is SHOWN, GREYED.**

`FactionRollup._build_knowledge_block` skipped those outright (`if progress <= 0.0: continue`), and
that skip is what made the whole ladder invisible to a new player: a faction that had learned nothing
rendered an EMPTY zone, so nothing on screen said there was anything to learn at all. Removing it is
half the value of this arc, which is why `KnowledgeRoster` walks the ROSTER — what there *is* to learn
— rather than the faction's progress row, which really does arrive empty on turn one and is absent
altogether for a faction that has learned nothing.

**The craft half takes its `0..1` from the sim's own denominator**, `progress / completion_threshold`,
and a `known` craft reads `1.0` whatever its raw progress says: the sim's `known` flag is the
authority on completion, not an inequality re-derived here.

## "UNSPENT" IS DERIVED, IS NEVER PERSISTED, AND DOES NOT MEAN "NEVER USED"

Nothing in the sim or the client records that a verb was ever exercised, and a persisted latch would
make a claim that cannot survive a reinstall. So the question asked is the one the shipped fields can
answer — **is anything using this RIGHT NOW** — and the label follows the meaning: *"nothing is using
it"*, never *"never used"*. Arguably the better signal anyway: it comes BACK if the player abandons
the thing, where a latch would go quiet forever after one use.

- **A ladder knowledge is in use when one of the faction's sources STANDS ON the step it unlocked** —
  `SourceForecast.improvement_is_done`, i.e. the source's `current_rung` at or above that step.
  **At-or-ABOVE is what kills a per-verb-FLAG test**: a patch reached by `Sow` carries `is_field` and
  no `is_cultivated`, so a flag test reports a faction with a working field as not using its
  Cultivation.
- **A craft knowledge is in use when the faction holds, or is making, something made of it** — any
  recipe of that craft (`RecipeDefState.craft`) whose output the bands carry, or which is on a bench.
  **It is NOT "does a recipe of this craft exist in the ledger"**: the crafting panel publishes ONE
  ROW PER RECIPE, ALWAYS, so mere presence is true of every craft on every turn and would answer
  *in use* for all of them forever. The bench arm is what stops a faction building its first loom
  reading unspent for the whole time it is being built.
- **A knowledge that unlocks nothing cannot be unspent at all.** `foddering` changes what a pen may
  draw on rather than unlocking a step, so there is no source that could stand on it.
- **Only a KNOWN node can be unspent.** A track at 40% has nothing standing on it either, and counting
  that would put every unlearned thing in the launcher's nudge.

**Ownership is `count` / `amount`, never `remaining`** — the crafting panel's own rule: a batch that
runs out of units is REMOVED, so a worn-out item and one never made both read `remaining 0`.

### `unspent_testable` IS TWO QUESTIONS, AND THE CONFIG ANSWERS THE FIRST

`unspent_testable` is `is_step` **and** a verb resolved by inverting `RungGates.RUNG_KNOWLEDGE_TRACKS`
— that table is what the compose sheet gates on, so reading it backwards is what makes "using it" here
and "allowed" there the same question.

**The two halves catch different faults.** `is_step` comes off the LADDER and is what distinguishes
`foddering` — a capability no rung waits on — from a step. The verb lookup is the client's own table,
and a route rung missing from it would read as a knowledge unlocking nothing at all; that is why
`grade` / `pave` are in `RUNG_KNOWLEDGE_TRACKS` beside the four food-web verbs.

### Which sources count as the faction's — THREE webs, three answers, and the wire forces each

**A forage patch carries `owner` / `has_owner`, so an ownership scan of every patch is attributable**
— the same test `AttentionController._under_kept_rung_attention` makes, and the reason it can run
outside the band loop. **A herd carries no owner field client-side at all**, so the only way to say
"ours" is through a band's own HUNT ASSIGNMENTS, which is exactly why `_starving_pen_attention` and
`_under_kept_herd_attention` walk assignments instead of `world_herds()`.

**A ROAD IS THE THIRD SHAPE, and the only one where ownership is a BAND.** A road tile carries its
keeper on the row (`has_keeper` / `keeper_band_id`) and has no labor row at all, so
`_kept_roads` joins that id against the player's own band roster — the bool read first, because `0` is
a real `BandId`. A road nobody keeps is not ours whatever rung it holds: the whole free floor has no
keeper, and a worn trail on the doorstep is not evidence anybody learned Roadbuilding.

The road rows reach the HUD through `HudLayer.update_road_network` → `HudBandLaborState.set_roads`, the
road twin of `update_forage_patches`, and `MapView` keeps its own per-tile index for the map draw and
the tile card (those are per-hex questions; this one is a whole-list question).

**A route verb's rung is read through a DIFFERENT FIELD NAME, and that is the one asymmetry.** A patch
and a herd publish their standing as `current_rung`; a road publishes its as `rung`. So
`_roads_standing_on` makes `improvement_is_done`'s own comparison — `rung_at_or_above(standing, the
rung this verb builds)` — rather than a second rule, which keeps *"is anything using this"* one
question on three webs.

`KnowledgePanelController` resolves all three and hands the arrays to the roster, so the derivation
stays pure. The live herd dict is the authority, never the assignment's launch-time copy: herds
migrate.

**The consequence worth knowing: a PEN whose keepers were all reassigned drops out of the animal
scan**, so Penning can read unspent while the fence still stands. That is the present-tense reading
doing its job — nobody is working it — and it is the same blindness every other herd-scoped producer
in this HUD has.

## THE FILTERS DIM, THEY DO NOT HIDE — and the count and the dimming are ONE predicate

`all` · `learning` · `close` (≥ `CLOSE_FRACTION`, 0.60) · `unused` · `new this turn`, as counts over
one list. A non-matching node keeps its place at `FILTERED_OUT_ALPHA`: the shape of the tree — two
short ladders and a fan — is most of what this screen teaches, and a filter that removed rows would
take that away every time it was used.

**`KnowledgeRoster.matches` serves both the pill's COUNT and the row's DIMMING.** A separate count and
a separate dim both look right on their own while disagreeing, which is the failure that arrangement
produces silently.

**`close` is a subset of LEARNING, not of everything.** A `known` node sits at 1.0 and would pass a
bare `progress >= CLOSE_FRACTION`, putting every finished track into a filter whose whole question is
*what would finish if I kept at it*.

**"New this turn" implies KNOWN.** The controller's diff is a set of keys that FINISHED during the
turn, and within one turn a track cannot un-finish — but a world boundary or a rehydrated save can
hand back a roster in which one of them is no longer complete, and a node marked new while reading
`not begun` is a sentence about nothing. It shipped that way first: an empty tracks row after a
completion rendered `New this turn 1` over a faction that knew nothing.

### The turn diff is ONE diff over BOTH webs, and it is the ONLY one left

The ladder tracks and the craft tracks arrive through different ingests, so a diff per ingest would
make the LAND row's "new" and the CRAFT row's "new" two different rules — and the one that
drifted would be invisible, both rendering as a plausible pill count. One diff over the SAME roster
the panel draws cannot disagree with what is on screen.

**It deliberately did not reuse `FactionReadouts._announce_knowledge_unlock`'s diff**, which answered
a different question: that one was fire-once-EVER per faction+track and survived across turns, because
a nudge repeated is noise. This one is "since the turn ticked" and has to go quiet again next turn.
That other diff is **gone** — the announcement it fed was retired in favour of the turn orb's
freshly-learned row, and the row is built off THIS diff, through the roster. So the client now has one
"a track just completed" detector rather than two, which is the point: two surfaces reporting one
event from two independently-derived diffs is how they come to disagree about which turn it happened
on.

**NO PRIOR VALUE MEANS NO DISCOVERY — and the grain is PER KEY, not per pass.** A fresh connect or a
rehydrated save arrives with tracks already complete and nothing to compare them against, so what has
never been observed seeds the baseline and reports nothing; otherwise every discovery a returning
player ever made lights up as new. (`_announce_knowledge_unlock` guarded the same trap with the same
rule before it was retired.) The empty baseline is distinguished from a faction that knows nothing by
an explicit `UNSEEN_TURN` sentinel rather than by an empty dictionary.

> #### ⛔ "THE FIRST PASS LEARNS NOTHING" WAS THE WRONG GRAIN, AND THE CRAFT HALF PAID FOR IT
>
> **A SECTION CAN ARRIVE LATER THAN THE FIRST PASS.** `Main` dispatches `update_intensification`
> before `update_crafting_catalogues`, and both roll this diff — so the baseline was seeded while
> `_craft_knowledge` was still empty, and the later same-turn refresh early-returned on
> `turn == _diff_turn` and never repaired it. On the next tick every craft the faction had known for
> a hundred turns was in `known_now`, absent from the baseline, and therefore *new*: one
> `"<Craft> learned"` row apiece. It held for the ladder half and failed for the craft half, which is
> the shape a per-pass rule will always have.
>
> `_seen_keys` records every key the roster has EVER carried, and a key not in it cannot be fresh —
> it is folded into the baseline instead, including on the same-turn path, which is the one the
> catalogues actually take. That generalises: any future section that lands after the seeding pass
> gets the same treatment for free.

**A second snapshot inside one turn must not re-arm it.** The server re-captures after every command,
and the baseline is the TURN's rather than the frame's — but a key seen for the FIRST time in such a
snapshot still joins the baseline, or it reports as new on the next tick.

## ⛔ THE PANEL BUILDS ITSELF FROM THE CONFIG, AND FIXING THAT MEANT FIXING THE WIRE

Ray, on finding his three new road knowledges missing: *"I would hope that panel would be more dynamic
and be able to create itself from the configuration files of knowledge."*

**The Craft row always worked that way** — its nodes come off `craft_knowledge` in the order the sim
published them, so a fourth craft in `recipes.json` needs no client edit. **Land and Herds were
hard-coded lists, and the reason was that their WIRE was hard-coded too**: the ladder's knowledges rode
as five named `float` fields on `IntensificationKnowledgeState`, so adding a knowledge meant adding a
schema field. That is exactly why the route branch's Roadbuilding and Paving had nowhere to appear and
the header went on saying *"All 8"*.

**So the fix is at the wire, and the panel follows.** The sim publishes a `ladder_knowledge` ROSTER —
one row per knowledge the ladder teaches — and every domain row is built from it:

| | derived from |
|---|---|
| which row | the **branch** of the rung that teaches it (`earns_knowledge`): `plant`→Land, `animal`→Herds, `route`→Roads; crafts ride their own list |
| order within it | that rung's **order**, bottom step first |
| step vs. capability | whether any rung's `unlock_knowledge` names it — `is_step` |

⛔ **`UNLOCKLESS_TRACKS` IS RETIRED, AND THAT IS THE POINT OF `is_step`.** It was a DECLARED set holding
exactly `foddering`, and it had to be declared because the client could not tell a knowledge that gates
nothing from one somebody forgot to wire up. The ladder answers that itself now, so `foddering` hangs
off the end of the Herds ladder because the config says no rung waits on it — and a knowledge that
STOPS gating a rung stops being a step with no second table to remember.

**What is left in the client is COPY, and only copy**: `HudKnowledgeVocab.DOMAIN_BRANCH_LABELS` (the
wire says `plant`, a player reads *Land*) and the two authored note tables. A branch the label table
has never heard of still draws — `domain_label` falls back to the capitalized wire token — because the
panel is built from the roster and a row must never depend on a client table knowing about it.

### DOMAINS ARE ROWS, IT IS NOT A GRAPH, AND AN EMPTY ONE IS NEVER DRAWN

The rung engine models ~4 steps per web and grows by adding BRANCHES, so the screen never needs pan,
zoom or edge routing — and a graph view would spend its whole budget drawing eight nodes' worth of
empty space.

**They were COLUMNS, and the measurement is why they are not** (`docs/plan_knowledge_rows.md` §1):
**domains are the axis that GROWS, and the shipped layout put them on the axis that cannot scroll.**
A column carried a 210px floor and a 20px separation beside a 300px pinned detail pane, so the card's
content minimum was a straight function of the branch count — `230 × domains + 336`. That is ~1,716px
at today's six, and **2,636–3,096 at the ten to twelve `docs/plan_civilization_steps.md` commits**, on
a 1,920 viewport that docked panels have already taken a bite out of. Ten to twelve is the planned
shape, not the stress case.

In rows, **width is a function of ladder DEPTH** — which the design caps at ~4 rungs and forbids
growing — so one fixed card width fits every branch count, and a new branch costs one ROW of height
on an axis that already scrolls. Measured in `knowledge_panel_stress`: **24 branches, 75 knowledges,
card minimum 546 against a fixed 820.**

> ⛔ **THE CLAMP WAS COMPUTED CORRECTLY AND THEN OVERRULED BY THE LAYOUT.** It looks as though `refit`
> already handled this. `KnowledgePanel.refit` raised `max_width` to the room and
> `AutoSizingPanel.fit_width` did clamp the request to it — but `_apply_width` writes that number to
> `size.x`, and **Godot will not render a Control below its `get_combined_minimum_size()`**. The card's
> combined minimum ran straight through `KnowledgeScroll`, whose `horizontal_scroll_mode` was
> `SCROLL_MODE_DISABLED`, and **a ScrollContainer that cannot scroll an axis propagates its child's
> full minimum on that axis rather than absorbing it.** No clamp would have made the columns fit; the
> content minimum itself had to come down. The horizontal axis is `SCROLL_MODE_AUTO` now, which is what
> makes the card genuinely shrinkable — it is required rather than cosmetic, and turning it back off
> restores the defect.

**The shipped ladder teaches SEVEN** — Land: `cultivation`, `seed_selection`. Herds: `herding`,
`penning`, `foddering`. **Roads: `roadbuilding`, `paving`.** Craft: whatever `craft_knowledge`
publishes. **War and Telling have no ladder branch, so they have no row** — a row appears the turn its
first branch teaches something, which is precisely how Roads got one with no client edit beyond its
label. An empty row is worse than a missing one: it teaches the player that a whole area of the game
is closed to them when in truth it does not exist yet.

### THE CARD IS A FIXED SIZE, AND THE RESERVED DETAIL BLOCK IS WHAT MAKES IT ONE

A reading is wider and taller than a bare ladder row, so a card fitted to its CONTENT narrows on every
close and widens on every open — and this card is CENTRED in its room, so that is a lurch in both
directions from the middle of the screen on every click. Two mechanisms, and both are load-bearing:

- **`refit` applies `PANEL_WIDTH`, never the content's demand.** `target_width` is the panel's ACTUAL
  width now rather than the nominal floor it was, clamped only by the room
  (`clampf(room.size.x, PANEL_MIN_WIDTH, PANEL_WIDTH)`), so `fit_width(0, 0)` has nothing left to fit.
- **The detail block is mounted in BOTH states at `DETAIL_BLOCK_MIN_HEIGHT`** — open, and holding the
  placeholder. The body's minimum height therefore does not change when a knowledge is opened or
  closed; the gap simply moves from the bottom of the list to under the open row. That constant is
  MEASURED against the tallest open reading the shipped copy produces, not chosen: a shorter reserve
  does not break the layout, it only lets the card breathe again.

Asserted as the CONSEQUENCE rather than as either mechanism (`_assert_card_does_not_breathe`), so it
survives a different implementation of the same promise: **both axes, on the open AND on the toggle
back**. Width alone passes with the height reserve deleted; opening alone passes with a card that
grows and never comes back. Measured at 1920×1080: **820 × 477 in all three states.**

### SELECTION IS A TOGGLE, AND IT NEEDS NO NEW STATE

`PAYLOAD_SELECTED` is a knowledge key whose EMPTY STRING already means *nothing is selected* — the
panel renders the placeholder for it — so a toggle is "set the key, or set it back to empty". Pressing
the open chip closes it; pressing a different one MOVES the reading. **Only one is ever open**, and
that is not fussiness: several at once would make the panel's height a function of how much the player
had poked at it, on a card centred in its room.

`Escape` closes it too, and the reading carries a `✕`. The `✕` emits its own `detail_closed` rather
than `node_selected` with the open key — a close routed through the toggle happens to work only
because of what is currently selected. `Main.escape_claimant` gained `ESC_KNOWLEDGE_DETAIL`, ranked
after `work_inspector` (that is a DIALOG; this is a paragraph inside a card, so it is the outermost of
the dismissible surfaces) and ahead of `ESC_PAUSE` (a surface with an explicit dismiss answers ESC
before ESC means "leave the game"). ⛔ **It claims the key only when a READING is open, never merely
because the screen is** — with the screen up and nothing selected ESC still falls through to the pause
menu, which is what `close_detail` returning `false` and `is_knowledge_detail_open` reading the
SELECTION rather than the panel's tree are for.

> ⛔ **THE ORB'S HAND-OVER FORCES OPEN AND MUST NEVER TOGGLE.** `open_on_filter` sets `_open = true`
> and leaves `_selected` alone. Now that selection toggles, an external open routed through
> `_on_node_selected` would **close** the row in the one case where the player already had that exact
> knowledge open — the one case where the orb's row appears to do nothing. It takes no key parameter
> either, the orb handing over a FILTER and nothing else today.

⛔ **THE ROSTER CARRIES NO FACTION, AND THE PROGRESS LIST DOES.** A faction that has learned nothing has
no `intensification_knowledge` row at all — the sim skips it — so a roster carried on that row would
leave a new player's screen EMPTY, which is the exact regression this whole arc exists to have fixed.
Two sections, and the split is load-bearing.

⛔ **THE ROSTER IS ALSO WHAT THE TURN DIFF WALKS.** `_diff_model` carries it for the same reason
`model` does: without it the diff sees no ladder nodes, `_seen_keys` never learns them, and no ladder
discovery can ever be reported as new.

**A LADDER domain draws the rail BETWEEN its rungs; the CRAFT fan draws none.** Its nodes are ORDERED
— each earned by practising the one below — and the connector between two chips is what says so; a
craft is learned by working its material and gates recipes rather than a next step. **That is a
property of the domain descriptor, not a branch in the renderer.** It is the column rail's rule
rotated ninety degrees, and only the ladder's connector carries `RAIL_META`, so *"a ladder draws its
rail and the craft fan draws none"* stays a claim about the TREE rather than about a pixel.

**A knowledge that gates nothing wears a capsule** reading `gates nothing` — `foddering` today. It
hangs off the end of its ladder and the capsule is what stops it reading as one more step. Drawn from
`NODE_UNSPENT_TESTABLE`, which is the config's own `is_step` crossed with the verb lookup, never a
client list of exceptions. **That puts the verb lookup's own blind spot on the FACE**: a ladder branch
absent from `RungGates.RUNG_KNOWLEDGE_TRACKS` reads `unspent_testable = false`, so every one of its
knowledges wears the capsule — the same fault this file already records under *`unspent_testable` is
two questions*, now visible without a click rather than only in the reading's `Where, now` line.

## A NODE CHIP IS A `PanelContainer` WITH `gui_input`, NEVER A `Button`

**Both halves of this were shipped wrong first, and both are invisible to a bounds assertion.** A
Button is not a Container, so a `glyph + name` face parented to one is NEVER LAID OUT — the children
pile up at the origin and the chip's height stops being a function of its content — and a `flat`
Button ignores its `normal` stylebox outright, so the SELECTED state was an override reaching nothing
the widget draws. It rendered, at the wrong height, with no visible selection.

`BandCityPanel._make_tab_button` records the identical finding for the identical reason. Follow it.

**The selection is the chip's own stylebox** — the faint wash this HUD gives a live selection inside a
`SIGNAL` border on all four sides, with identical content margins in both states so selecting a chip
never moves the ones beside it. A border rather than the column layout's leading bar: a bar down one
edge said *this row of a column*, and on a horizontal run it reads as a connector to whatever is left
of it.

**The chips ride in an `HFlowContainer`, and that is what keeps the card bounded.** It wraps, so its
own minimum width is only its WIDEST CHILD — one chip — rather than the sum of the ladder. An
`HBoxContainer` would put the whole ladder back into the card's minimum, which is the column layout's
defect on the other axis. The connectors carry the spacing (both flow separations are zero) so a
wrapped chip lands at the same pitch as one that did not wrap.

### THE UNSPENT STATE HAS THREE CARRIERS NOW, AND ALL THREE ARE WIRED

The whole point of the state is that the player has not noticed it, so it has to be legible without a
click — and the clause row that used to sit under the node's name (`◇ nothing is using it`) has
nowhere to go under a chip. So:

- **the `◇` mark ON the chip**, in `WARN`, the tint the tally's unspent clause takes;
- **the chip's own `tooltip_text`** — the unlock note, with the clause APPENDED rather than replacing
  it, the two saying different things;
- **the reading's state line**, `Known · nothing is using it`.

Dropping any one of them is a state the player can only find by opening the reading, which is the
defect the clause row existed to prevent.

### THE EMPTY-FILTER NOTE RIDES UNDER THE FILTER PILLS

It hung in the pinned detail pane because a banner drawn ACROSS the columns would have read as a
replacement for the list rather than as a note about it. There is no pinned pane to hang in now, and
the header answers the same objection better: it is a note about the FILTER, and the filter is the row
directly above it. `EMPTY_NOTE_META` and `FILTER_EMPTY_FORMAT` are unchanged.

## The inline reading uses `FactionReadouts`' copy, and authors only the half that did not exist

Three side-by-side sections, in the prototype's order — **does · where · how**
(`docs/knowledge_rows_ux_proposal.html` → `detailFor`), which is the design. The pinned pane read
does · how · where; laid out as three columns, *where it stands now* belongs beside *what it lets you
do* rather than after the practice note.

- **What it lets you do** — `FactionReadouts.KNOWLEDGE_UNLOCK_NOTES`, READ rather than re-authored, so
  the reading and any other surface naming a discovery cannot describe it differently. **That table
  outlived the announcement it was written for**: the one-shot System note is retired (§5) and the
  table is not, this reading being its reader now. See `band-readouts.md`.
- **Where, now** — a COUNT of the faction's sources standing on it. **Never a jump**: a discovery
  unlocks a verb across the whole map, so there is no one hex for `focus_on_tile` to land on, which is
  why the knowledge rows are non-locating. **A node not yet learned has no "where" at all**, so the
  kicker itself changes to `DETAIL_NEEDS_HEAD`: "0 sources" would read as a shortfall rather than as a
  thing not yet learned.
- **How it is learned** — `HudKnowledgeVocab.PRACTISE_NOTES`, which is the half that existed NOWHERE
  in the client. The rule lived only in a Rust doc comment on `intensification_ladder.json`
  (`_comment_earns_knowledge`, one per rung) and a player was never told any of it, which is most of
  why the ladder read as something that happened TO them.

**The state line above them keeps the block METER**, which the chip could not (`METER_CELLS` is still
live for it) — and with it the `progress * HudConst.PROGRESS_PERCENT_SCALE` conversion, `meter_bar`
grading a `0..100` score where every node's progress is `0..1`. Dropping that is how the faction
page's meters shipped EMPTY, indistinguishable from an unstarted track beside a live percent.

**One column's body width is DERIVED, never typed** — `PANEL_WIDTH` less the card's chrome and the
scroll gutter, less the block's indent and right margin, less the bar and its gutter, less the gutters
between the columns, shared out between `DETAIL_SECTION_COUNT`. The chrome and gutter terms are not
optional: a width derived from `PANEL_WIDTH` alone makes the reading's minimum the card's OUTER width,
which is wider than its interior, so the horizontal scrollbar would show on every frame of a card that
fits.

**`PRACTISE_NOTES` IS AUTHORED — but it is COPY now, not structure.** The roster carries the branch and
order of the teaching rung, so the client no longer needs a table to know WHERE a knowledge sits; what
these sentences add is HOW it is practised, which the wire does not carry. They are a transcription of
the config, which is authoritative — `plant:wild` earns `cultivation`, `plant:tended` earns
`seed_selection`, `animal:wild` earns `herding`, `animal:pastoral` earns `penning`, `animal:pen` earns
`foddering`, `route:trail` earns `roadbuilding`, `route:dirt_road` earns `paving`. **Re-read the
config, do not re-word them, if a rung's `earns_knowledge` ever moves.**

⛔ **A KNOWLEDGE WITH NO NOTE STILL DRAWS, AND SO DOES ITS SECTION.** Absence leaves the column with
less to say; it never removes the section and never removes the node. That is what keeps the roster wire-driven: if a missing sentence could suppress a row
entry, adding a knowledge would be a client edit again.

**The player-facing NAME is the sim's** (`display_name`, resolved from the knowledge id: underscores to
spaces, each word capitalized), read back through `FactionReadouts.knowledge_label`. The hard-coded
`KNOWLEDGE_TRACK_LABELS` table is **retired** — it doubled as the declared track list, and both of its
jobs are the wire's now.

**The two "leave more standing" clauses are the `learn_multiplier`** and appear on exactly the two
tracks earned by DRAWING from a source. A rung-3 source is tended rather than drawn from, so the floor
axis has collapsed there and the clause would be a lie on the other three.

## THE LAUNCHER, AND THE PIP THAT IS NOT PART OF THE DESCRIPTOR

A second `register_action` entry beside `ACTION_CRAFTING` — the same
`{id, glyph, tooltip, enabled, sprite}` descriptor, the same `action_invoked` edge, the same three
mounts. **The second entry is what proves the registry is one**: it took a descriptor and a relay and
no geometry at all. `knowledge_requested` is its named relay, and unlike the `⚒`'s it resolves NOTHING
— knowledge is per-FACTION, so there is no subject to look up and no empty-subject case to guard.

### THE FACE IS BUNDLED ART, ON THE BUTTON'S OWN `icon` SEAM

The launcher wears the drawn CAIRN (`assets/icons/hud/cairn.png`, resolved by
`HudSprites.for_mark(HudKnowledgeVocab.LAUNCH_MARK)`), and it wears it as the `Button.icon` PROPERTY,
never as a child — a `Button` is not a Container, so a child of one lays out by anchors and would draw
beside the face rather than on it. `ACTION_SPEC_SPRITE` is the descriptor key that carries it, and it
sits INSIDE the descriptor contract rather than beside it like the pip does: the texture is resolved
ONCE by the registrant at wiring time, so the mount rebuild copies a value and never performs a
lookup. Art OR glyph, never both — the sprite branch leaves `text` empty, and `LAUNCH_GLYPH` (`▲`) is
now the FALLBACK face rather than a placeholder, drawn when `for_mark` returns `null`. The `⚒` passes
no sprite, which is what makes the parameter an option on the descriptor rather than a new
requirement.

**`expand_icon` IS OFF here and ON for the compose sheet's quarry picker, and the difference is the
button's WIDTH.** Godot's expanded-icon layout fits the art into the box left after the stylebox
padding *and* after a further subtraction of `icon_max_width`; on a row-wide picker that is
imperceptible, on a 24px face it is a negative number, and the first cut of this rendered the cairn as
a two-pixel speck. `icon_max_width` alone sizes the art exactly, so nothing is lost by turning
expansion off.

**An art face is re-padded, because the ghost chrome pads for a LABEL.**
`HudStyle.BUTTON_PADDING_H/V` are 11 and 9 — a glyph simply overflows that box, an icon does not — so
`_repad_button_for_sprite` re-asks `HudStyle.button_styleboxes` for the same ghost set and changes
only the content margins to `ICON_BUTTON_SPRITE_PADDING`. That constant is DERIVED
(`(ICON_BUTTON_SIZE - ICON_BUTTON_ICON_MAX_WIDTH) / 2`), so an art-bearing action's minimum comes out
at exactly `ICON_BUTTON_SIZE` and it cannot grow the icon family apart from a glyph one. Verified in a
frame at all three mounts, the collapsed rail included, in `knowledge_launcher_mark{,_rail,_bar}.png`
— and with the pip over it, which is the normal case here rather than an edge one.

**The PIP is pushed through its own seam (`set_action_pip`), and that separation is load-bearing.**
`register_action`'s contract is that a descriptor is DECLARED at wiring time and never a function of
snapshot state, which is what keeps the bar's geometry off the render's hot path; a pip is restated
every turn. So:

- it is stored on `_action_pips`, which **survives a mount rebuild** — the buttons are thrown away
  wholesale whenever the panel re-homes its actions (a dock change, a collapse), and a count that
  lived only on the node would vanish on a dock flip and come back on the next turn tick;
- it is drawn as an **anchored, mouse-transparent child INSIDE the button's own rect**. A Button is
  not a Container, so such a child contributes nothing to the parent's minimum size — which is exactly
  the property wanted: a badge that took layout width would make the action bar's minimum a function
  of a snapshot count.

### OPENING THE SCREEN DOES NOT CLEAR THE PIP

§4 says it "clears when the screen is opened". What actually clears an unspent count is USING the
knowledge, and a pip that went quiet on a look would tell the player they had dealt with something
they had not. The count is derived fresh every push (`unspent_count`), so it goes away exactly when a
source starts standing on the discovery — the honest trigger, and the one the state's own definition
already gives.

### …AND IT IS PUSHED FROM THE KNOWLEDGE INGESTS AS WELL AS FROM `update_band_alerts`

`Main` dispatches each snapshot section INDEPENDENTLY and only when it CHANGED, so a delta whose
`populations` are byte-identical skips `update_band_alerts` entirely — and that was the one seam the
pip was pushed from. A turn that finishes a track and moves nobody would leave the count a turn stale
on the one surface that exists to announce it. `update_intensification` and
`update_crafting_catalogues` push it too. Populations move on nearly every turn, and "nearly" is what
made this latent rather than absent.

## THE ORB'S ROW OPENS THIS SCREEN ON A FILTER, AND `open()` COULD NOT DO IT

`open_on_filter(filter)` exists because **the live filter is CONTROLLER state that survives a close**,
deliberately — which node the player is reading and which filter they set outlast a turn tick, exactly
as the crafting ledger's fold state does. So the launcher's plain `open()` reopens on whatever the
player last set, and a row that has just said *"Penning learned"* would land them on a list that need
not contain it.

- **`knowledge_learned` → the `new` filter**, the list holding the discovery the row just named.
  `TurnOrbController` owns that mapping; this controller just takes a filter.
- **IT OPENS, IT NEVER TOGGLES.** The launcher glyph is a toggle because pressing it means *show me /
  hide it*; pressing an attention row means *take me to this*, and a press that closed the screen
  because it happened to be open already would answer a question nobody asked. It re-renders either
  way, so an already-open panel redraws on the new filter.
- **The SELECTION is left alone.** A filter is a question about the list, not about the node being
  read, and throwing a reading away to answer it would lose the one thing the reading is for.

`nodes()` is the other seam slice C added: the flattened roster, exposed rather than re-derived per
reader, because the walk behind it resolves the faction's patches, herds, kit and bench. The rows
draw it, the pip counts it and the orb's row is built off it — one derivation, so no two of the three
can answer differently about one discovery.

**AND ONE WALK PER SNAPSHOT, WHICH IS WHY `unspent_count_of(roster)` EXISTS.**
`HudLayer._refresh_knowledge_readouts` asks two questions of one snapshot — the pip's number and the
orb's row — and building `nodes()` for each is a second walk of the whole player world for one
answer, on a seam that runs on every delta carrying populations, knowledge or catalogues. It shipped
that way first and **the render harness caught it as a flake**: `band_panel_preview`'s queue
auto-scroll gesture is bounded by a frame budget, and the extra per-snapshot walk was enough to leave
it 53px short of the 56px it drives for — two failures in five runs against none in four at `main`,
and none in five once the walk was shared. `unspent_count()` is the same expression over its own
`nodes()`, so the two entry points cannot drift.

**Asserted, never screenshotted** (`_assert_opens_on_filter`): a screen opened on the wrong filter
renders a perfectly ordinary card. The claim is read off the DRAWN chrome — the lit pill is the one
whose `normal` stylebox carries an opaque fill, every quiet pill sitting at `HudStyle.PILL_QUIET_ALPHA`.
**The screen is parked on a DIFFERENT filter first, through the real pill**, which is what makes the
landing falsifiable: the whole job of `open_on_filter` is to override retained view state, so a
fixture that opened a panel already sitting on `new` would pass with the branch deleted. The block
ends by proving the entry point is not redundant — parked on `unused` again, a plain launcher open
comes back on `unused`.

## THE KNOW TAB IS DELETED, AND SETTLING AND DISCOVERIES ARE REHOMED RATHER THAN RETIRED

The faction page drops to three zones — Faction · Work · Parties — so `FactionRollup.build_knowledge_zone`
and `_build_knowledge_block` are gone, along with `ZONE_KNOWLEDGE`, `ZONE_KNOWLEDGE_WIDTH`,
`ZONE_TAB_KNOWLEDGE`, `FACTION_HEADER_KNOWLEDGE` and `FACTION_KNOWLEDGE_KNOWN`.

**Settling and Discoveries move to the `band` zone. Neither is knowledge** — neither is earned by
practice and neither unlocks a verb — which is exactly why they did not follow the craft tracks out.
What they state is what the faction has BECOME and what it has FOUND, and "who is this faction" is
that zone's question, so they belong there on the merits rather than merely being left over.

**The height tier came with them and was RE-MEASURED rather than carried over.**
`FACTION_BAND_FULL_MIN_HEIGHT` is 480: the full block reads **461px** and the two boxes the panel
offers are **396** on a wide horizontal dock and **941** on a tall side one. It was GUESSED at 400
first, which was wrong in both directions — below the 461 the block needs, so a box between the two
would have taken the full branch and clipped, and clearing the wide dock's own 396 by 4px, which is
not a threshold but a coincidence.

**The shell threshold follows by derivation**: it is a sum over the LIVE zone list, so the page's flip
moved from 1569 to the 1190 a band's three cost. The harness's equality claim now pins the SEPARATOR
COUNT — two gaps between three columns — which is the term that was wrong when the page had four.

## Verification

`tools/ui_preview/chapters/knowledge_panel.gd`, and **most of it is PNG-less on purpose**. Every claim
this screen makes renders as a plausible picture whatever it says — a pill reading `2`, a greyed row,
the clause *"nothing is using it"*, a `3` on the pip — so the derivation is asked of `KnowledgeRoster`
directly with models staged in the chapter, and the frames are for the LAYOUT alone.

**The fixtures derive their standing rung and never state it** (`fixtures_rung.gd`), the whole test
tree's rule: `improvement_is_done` reads one wire field, so a hand-built source that omits
`current_rung` reads as *nothing has been built here* — a plausible frame with every other assertion
green.

**A herd fixture is keyed `id`, not `herd_id`.** `HudBandLaborState.find_world_herd` matches on `id`,
so the other spelling is invisible to the assignment walk and every animal claim reads "nothing is
using it" for a reason that has nothing to do with the code under test. It cost a run.

**The chip and the filter pill are driven with REAL POINTER INPUT**, never `pressed.emit()` — the
chip has no signal of its own to fake, and the harness contract's reason applies either way: an
emitted signal passes on a control that is covered, zero-size or filtered out of the hit test, which
is exactly the shape this chip shipped in first.

**`fixtures_knowledge.gd` is the shared roster fixture**, in the wire's own shapes, used by the
`knowledge_panel` / `turn_orb` / `herd_graze_pen` chapters and by `band_panel_preview`. It is a
TRANSCRIPTION of the shipped ladder and deliberately not derived — a fixture that recomputed the roster
would pass against a producer that had stopped producing one. That the transcription matches the config
is the SIM's claim (`the_published_roster_places_every_knowledge_the_ladder_teaches`); what the fixture
proves is that the client renders whatever roster arrives.

⛔ **AND A HARNESS THAT PUSHES NO ROSTER RENDERS NO LADDER ROWS**, which is the honest consequence of
the panel building itself. The `ui_preview` prologue pushes it once so every chapter has one.

**THE FALSIFICATION RUNS IN THE REMOVAL DIRECTION** (`_assert_the_roster_builds_the_rows`, claim 4):
a knowledge dropped from the roster leaves the panel and the tally shrinks with it. It needs no config
file, and nothing in the client names the dropped knowledge — so if the node survives, the panel is
drawing from something other than the wire.

⛔ **TWO SHAPES, AND MIXING THEM IS SILENT.** The wire's per-faction row carries its tracks under
`knowledges`; every GATE in this client (`RungGates`, `RungLadder.track`) takes the flat
`{track: 0..1}` map `faction_tracks` hands back. A gate handed the wire row reads every track as `0`,
which renders a perfectly ordinary frame with every rung honestly refused — so `band_panel_preview`
keeps `_standing_knowledge_row` and `_standing_knowledge_tracks` under separate names.

**Frames:** `knowledge_panel` (the whole screen, mixed states — four domain rows, the rails between
their chips, the craft fan with none) · `knowledge_panel_untouched` (**the frame this arc is about** —
a faction that knows nothing, every node drawn and greyed, where the old faction-page block rendered
an empty zone) · `knowledge_panel_detail` (a node selected, the reading open UNDER ITS OWN ROW, its
three sections and the `SIGNAL` bar) · `knowledge_panel_filtered` (a filter live, so the DIMMING is in
a frame) · **`knowledge_panel_stress`** (24 synthetic ladder branches — §1's own stress figure and
twice the planned shape — with the card still at its fixed width and the list scrolling instead) ·
`knowledge_launcher_mark` / `_rail` / `_bar` (the CAIRN on the launcher's face, one per action mount —
subject row, collapsed rail, bar — with the pip over it on the first two, which is the normal state
for this action rather than an edge one).

**The stress roster is built in the chapter from `KnowledgeFx.ladder_roster()`'s ROW SHAPE**, not from
a config and not from the sim: the panel builds itself from whatever roster arrives, so a synthetic
one is exactly what tests that. It asserts the card's WIDTH *and* `card().get_combined_minimum_size().x`,
because a card can be SET to 820 while demanding more — which is precisely how the old clamp was
overruled. It restores the shipped roster afterwards; this HUD is long-lived and every later chapter
is written against the real ladder.

## A world that arrives already knowing things must not announce them

`_update_learned_this_turn` answers *"what became known since the turn ticked"* by comparing the
roster's KNOWN set against `_known_at_turn_start`. Three pieces carry it: `_seen_keys` (every key the
roster has ever declared), `_known_at_turn_start` (the baseline) and `_diff_turn` (the turn the
baseline belongs to).

**The baseline is seeded from ONE frame, and `Main` does not deliver a knowledge world in one
frame.** `Main._apply_snapshot` dispatches sections independently and only when they CHANGED, in a
fixed order: the ladder's ROSTER (`update_ladder_knowledge`, a per-world constant) before the
faction's PROGRESS row (`update_intensification`), with `update_crafting_catalogues` later still —
**and all three refresh the readouts**, so the diff can roll against a half-populated model.

**That is what shipped the "learned" storm after a load.** Loading a save reloads `Main.tscn`, so
every controller is a NEW object with `_diff_turn` at `UNSEEN_TURN` — `reset_world_state` is not
even involved, and a partial reset was never the candidate. The first frame of the loaded world then
rolled the diff twice:

| | `_seen_keys` | `_known_at_turn_start` |
|---|---|---|
| after the ROSTER (refresh #1, turn 71) | all 7 knowledges | **empty** — no progress had arrived |
| after the PROGRESS row (refresh #2, same turn) | unchanged | **still empty** |

The second pass is where the repair should have happened, and it could not: the same-turn fold was
over keys seen for the **FIRST time**, and those keys had been spent one refresh earlier. Turn 72
then found three long-finished tracks known and absent from the baseline, and the orb announced
*"Cultivation learned"*, *"Seed Selection learned"* and *"Herding learned"* for knowledge earned
forty turns before the save. Reproduced against the reported save (`thirdsave.shdw`, turn 71) by
driving the real entry points in `Main`'s order.

**A new game hides it completely**, which is why it survived so long: turn one's tracks are all
`0.0`, so an empty baseline is the truth and the bug has nothing to say.

**The rule that actually holds is about the TURN NUMBER, not about novelty.** A discovery is resolved
by the sim at a turn boundary and the frame reporting it carries the NEW turn number, so it goes
through the transition branch and is announced there. **Nothing the client does can make a track
become known while the turn number stands still** — an optimistic reconcile invents no knowledge, and
a load republishes the SAME turn (`recapture_snapshot_in_place`). So anything that becomes known on a
later frame of the baseline's own turn is a SECTION ARRIVING, and the fold is over every known key.
`_learned_this_turn` is still left standing by that branch, so an announcement survives every later
frame of its own turn.

**The craft row takes the identical path** and is the case the narrow fold was written for —
`update_crafting_catalogues` lands after `update_intensification` on the same turn. Widening the fold
subsumes it; both are guarded, as siblings, in `ui_preview`'s `knowledge_panel` chapter.

### Verify

`chapters/knowledge_panel.gd` → `_assert_loaded_world_is_not_learned`, the ladder twin of
`_assert_late_catalogue_is_not_learned`. **It asserts the POSITIVE first**: the roster really is
carrying all three tracks as `known` on the seeded frame — without that leg, "nothing was announced"
passes on a fixture that staged nothing at all. Then no `learned_this_turn` entry, no `new_this_turn`
on the roster node, no row out of `AttentionController.knowledge_attention` (the orb is asked in its
own terms, since the orb is the surface that shouted), and finally a track completing AFTER the load
that must still be announced — a fix that merely went quiet would pass every other leg.

Sabotage-verified: narrowing the fold back to `first_seen` fails 5 of those legs, with the three
non-vacuous legs still passing.

