class_name HudKnowledgeVocab

## The KNOWLEDGE SCREEN's vocabulary leaf (`docs/plan_knowledge_screen.md` §3) — the words it says,
## the domains it groups by, the filters it counts and the geometry the card is laid out on. A
## DECLARATION BLOCK, the `HudCraftingVocab` shape: a new label or threshold goes HERE rather than as
## a fresh `const` on the panel, which is what keeps a const block from regrowing into a
## merge-conflict surface.
##
## **THE PLAYER-FACING WORD IS *IMPROVE*, NEVER *CLIMB A RUNG*.** "Rung" and "climb" are the
## intensification arc's INTERNAL vocabulary — `RungGates`, `SourceForecast` and
## `docs/plan_intensification_ladder.md` keep them — and they do not survive contact with a player: a
## hex asked to "climb a rung" is a metaphor the game never taught. Nothing in this file says either
## word, and that is a rule rather than an accident.
##
## ⛔ **THE COLUMNS ARE NOT DECLARED HERE EITHER — THE WIRE'S OWN ROSTER BUILDS THEM.** Land and Herds
## used to be hard-coded node lists (`DOMAIN_NODES: ["cultivation", "seed_selection"]`), and the
## reason was that their WIRE was hard-coded too: the ladder's knowledges rode as named float fields,
## so adding one meant adding a schema field. That is why the route branch's Roadbuilding and Paving
## had nowhere to appear and the header went on saying *"All 8"*. The sim publishes a
## `ladder_knowledge` roster now — one row per knowledge, carrying the branch of the rung that TEACHES
## it, that rung's ORDER, and whether any rung's `unlock_knowledge` names it — so **a knowledge added
## to `intensification_ladder.json` reaches this panel with no client edit at all**, exactly as a
## fourth craft in `recipes.json` already did.
##
## What is left here is COPY, and only copy: the player-facing name of a BRANCH (the wire says
## `plant`, a player reads *Land*), and the two authored note tables. A knowledge whose branch or note
## this file has never heard of still draws — it simply has less to say.
##
## **THE UNLOCK NOTES ARE NOT RE-AUTHORED HERE.** `FactionReadouts.KNOWLEDGE_UNLOCK_NOTES` is already
## the "what it lets you do" copy for the five ladder tracks and the panel reads it, so no two
## surfaces naming a discovery can describe it differently. That table outlived the one-shot unlock
## announcement it was written for — retired in favour of the turn orb's freshly-learned row
## (`docs/plan_knowledge_screen.md` §5) — and this panel's detail pane is its reader now. What this
## file adds is the half that existed nowhere in the client: **how a knowledge is LEARNED**
## (`PRACTISE_NOTES`), which lived only in a Rust doc comment on `intensification_ladder.json`.
##
## DEPENDENCY DIRECTION: a vocab leaf reads nothing (`HudConst` is the model) — `HudStyle` is the one
## exception the family already makes, and it is a leaf too, so the pair stays acyclic.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# ---- the domains -----------------------------------------------------------------------------
# **A DOMAIN IS A COLUMN, AND ITS SHAPE IS A PROPERTY OF THE DESCRIPTOR RATHER THAN A BRANCH IN THE
# RENDERER.** A LADDER domain draws a rail down its left edge because its nodes are ORDERED — each
# one is earned by practising the one below it — and a FAN domain draws none, because a craft is
# learned by working its material and gates recipes rather than a next step.

## A column's key IS the wire's own branch token for a ladder domain, so nothing has to be mapped
## back and forth; `craft` is the one that is not a ladder branch at all.
const DOMAIN_KEY_LAND := &"plant"
const DOMAIN_KEY_HERDS := &"animal"
const DOMAIN_KEY_ROUTES := &"route"
const DOMAIN_KEY_CRAFT := &"craft"

const DOMAIN_SHAPE_LADDER := "ladder"
const DOMAIN_SHAPE_FAN := "fan"

## Descriptor keys (`KnowledgeRoster` builds them; nothing else writes one).
const DOMAIN_KEY := "key"
const DOMAIN_LABEL := "label"
const DOMAIN_SHAPE := "shape"
const DOMAIN_NODES := "nodes"

## ⛔ **WHAT A PLAYER CALLS EACH LADDER BRANCH.** The wire says `plant` / `animal` / `route`, which is
## the SIM's vocabulary; these are the words on the column heads. **A LABEL TABLE AND NOTHING ELSE** —
## it declares no nodes, so a knowledge added to a branch already listed here needs no edit, and one
## added to a branch that is NOT listed still draws (see `domain_label`).
const DOMAIN_BRANCH_LABELS := {
	DOMAIN_KEY_LAND: "Land",
	DOMAIN_KEY_HERDS: "Herds",
	DOMAIN_KEY_ROUTES: "Roads",
}

## The CRAFT column's label. Its nodes are not declared either: they come off the wire's own
## `craft_knowledge` vector in the order the sim published it, so a fourth craft appearing in
## `recipes.json` needs no client edit.
const DOMAIN_CRAFT_LABEL := "Craft"

## **A COLUMN APPEARS THE TURN ITS FIRST KNOWLEDGE DOES, AND AN EMPTY ONE IS NEVER DRAWN**
## (`KnowledgeRoster` drops it). An empty column is worse than a missing one: it teaches the player
## that a whole area of the game is closed to them when in truth it does not exist yet. War and
## Telling have no ladder branch at all, so they have no column; **Roads gained one the moment the
## route branch started teaching something**, with no edit beyond the label above.

## What a branch this file has no word for reads as — the wire's own token, capitalized. It is the
## honest answer rather than a blank head, and it is what keeps an unlisted branch DRAWABLE: the
## panel is built from the roster, so a column must never depend on a client table having heard of it.
static func domain_label(branch: StringName) -> String:
	return String(DOMAIN_BRANCH_LABELS.get(branch, String(branch).capitalize()))

## The caption under a domain head, saying what SHAPE the column is and so what the reader is looking
## at. A few words, because a head that explained itself in a clause would read as part of the list.
const DOMAIN_SHAPE_NOTES := {
	DOMAIN_SHAPE_LADDER: "one step at a time",
	DOMAIN_SHAPE_FAN: "learned by working it",
}

# ---- a node's three states -------------------------------------------------------------------
# **THREE, AND THE THIRD IS DRAWN.** `not_begun` is what the faction page's knowledge block used to
# SKIP (`if progress <= 0.0: continue`), and that skip is what made the whole ladder invisible to a
# new player: a faction that has learned nothing rendered an EMPTY zone, so nothing on screen said
# there was anything to learn at all. A track at `0.0` is SHOWN, GREYED.

const NODE_STATE_KNOWN := "known"
const NODE_STATE_LEARNING := "learning"
const NODE_STATE_NOT_BEGUN := "not_begun"

## Node keys (`KnowledgeRoster` builds them).
const NODE_KEY := "key"
const NODE_LABEL := "label"
const NODE_DOMAIN := "domain"
const NODE_STATE := "state"
## `0..1`. For a craft track that is the wire's `progress / completion_threshold`, so the client draws
## no scale of its own — a meter whose denominator was guessed would disagree with the sim's own
## reading of the same track.
const NODE_PROGRESS := "progress"
## Is this knowledge KNOWN with nothing using it — see `KnowledgeRoster`.
const NODE_UNSPENT := "unspent"
## **Can "unspent" even be ASKED of this node?** `false` for a knowledge that unlocks nothing, which
## has no "using it" to be without. Carried explicitly rather than folded into `NODE_UNSPENT` so the
## filter's count and the detail pane's line can tell *"nothing is using it"* apart from *"there is
## nothing for anything to use"*.
const NODE_UNSPENT_TESTABLE := "unspent_testable"
## Was this knowledge completed THIS turn — see `KnowledgePanelController`.
const NODE_NEW := "new_this_turn"
## What the knowledge lets the faction's hands do, one sentence.
## `FactionReadouts.KNOWLEDGE_UNLOCK_NOTES` for a ladder track; `CRAFT_UNLOCK_NOTE_FORMAT` for a craft.
const NODE_NOTE := "note"
## How it is learned — `PRACTISE_NOTES` / `CRAFT_PRACTISE_NOTE`.
const NODE_PRACTISE := "practise"
## The improvement verb this knowledge gates (`SourceForecast.IMPROVEMENT_*`), `""` when it gates
## none. The handle the unspent test uses, and it is taken from `RungGates.RUNG_KNOWLEDGE_TRACKS`
## rather than restated, so what this panel calls "using it" cannot drift from what the compose sheet
## calls "allowed".
const NODE_UNLOCKS := "unlocks"
## How many of the faction's own sources STAND ON what this knowledge unlocked. **A COUNT, NEVER A
## JUMP**: a discovery unlocks a verb across the whole map, so there is no one hex for a jump to land
## on — which is why the knowledge rows are non-locating (§1).
##
## **THERE IS DELIBERATELY NO "…AND N MORE COULD" COUNT BESIDE IT.** The obvious candidate — every
## source NOT already standing on the rung — is not what "could" means: a patch on ground that will
## never take seed cannot take a Sow, and answering that needs each source's own site gates. A number
## the panel cannot back is worse than no number, so the unspent line states the verdict alone.
const NODE_IN_USE_COUNT := "in_use_count"

## The glyph in front of a node's name. Geometric, not emoji, for the reason every other glyph in
## this HUD is (`BandCityPanel` → "chrome glyphs"): they render reliably at any font.
const NODE_GLYPHS := {
	NODE_STATE_KNOWN: "●",
	NODE_STATE_LEARNING: "◐",
	NODE_STATE_NOT_BEGUN: "○",
}

## What a node's right-hand cell reads in each state. `learning` takes a percent
## (`LEARNING_VALUE_FORMAT`).
const NODE_VALUE_KNOWN := "known"
const NODE_VALUE_NOT_BEGUN := "not begun"
const LEARNING_VALUE_FORMAT := "%s %d%%"

## The meter beside a learning node's percent, in `HudFormat.meter_bar` cells. It reads
## `FactionReadouts.KNOWLEDGE_METER_CELLS` rather than declaring a second one: this panel and the
## faction page draw the SAME track, and two constants is how they come to disagree about what
## half-learned looks like.
const METER_CELLS := FactionReadouts.KNOWLEDGE_METER_CELLS

## The clause a KNOWN node wears when nothing is using it. **NOT "never used"** — nothing in the sim
## or the client records that a verb was ever exercised, and a persisted latch would make a claim
## that cannot survive a reinstall. What IS derivable is the present tense, and the label says
## exactly that much (`docs/plan_knowledge_screen.md` §2).
const UNSPENT_CLAUSE := "nothing is using it"
const UNSPENT_MARK := "◇"

# ---- the filters -----------------------------------------------------------------------------
# **COUNTS OVER ONE LIST, AND A NON-MATCHING NODE DIMS RATHER THAN DISAPPEARS.** The shape of the
# tree is most of what the screen teaches — two short ladders and a fan — and a filter that removed
# rows would take that away every time it was used, leaving the player reading a list whose length
# they cannot place.

const FILTER_ALL := &"all"
const FILTER_LEARNING := &"learning"
const FILTER_CLOSE := &"close"
const FILTER_UNUSED := &"unused"
const FILTER_NEW := &"new"

const FILTER_SPEC_KEY := "key"
const FILTER_SPEC_LABEL := "label"

## The five, in the order they are drawn: the whole list, then the three questions a player asks of
## it, then what just happened.
const FILTERS: Array[Dictionary] = [
	{FILTER_SPEC_KEY: FILTER_ALL, FILTER_SPEC_LABEL: "All"},
	{FILTER_SPEC_KEY: FILTER_LEARNING, FILTER_SPEC_LABEL: "Learning now"},
	{FILTER_SPEC_KEY: FILTER_CLOSE, FILTER_SPEC_LABEL: "Close"},
	{FILTER_SPEC_KEY: FILTER_UNUSED, FILTER_SPEC_LABEL: "Ready · unused"},
	{FILTER_SPEC_KEY: FILTER_NEW, FILTER_SPEC_LABEL: "New this turn"},
]

## What `close` means: a LEARNING track at or past this fraction. A **threshold on the same 0..1
## scale every node carries**, so a craft's `progress / completion_threshold` and a ladder track's
## raw meter are asked the same question. 0.60 is the prototype's, and it is deliberately well short
## of complete: "close" answers *what would finish if I kept at it*, which a 0.9 bar answers for
## almost nothing.
const CLOSE_FRACTION := 0.60

## `[label] n` on a filter pill. The count rides ON the pill rather than beside it because the pills
## ARE the counts — a player reads the row to find out how many of each there are, and a pill whose
## number was elsewhere would have to be pressed to answer that.
const FILTER_PILL_FORMAT := "%s %d"
const FILTER_ROW_LABEL := "Show"

## What the body says under a filter that matches nothing. It is not an error — a faction with
## nothing close to finished is the ordinary early state — so it names the filter rather than
## apologising. **The nodes are still drawn** (dimmed); this line is a caption over them, which is
## what stops a zero-match filter reading as a broken screen.
const FILTER_EMPTY_FORMAT := "Nothing is %s right now."
const FILTER_EMPTY_CLAUSES := {
	FILTER_LEARNING: "being learned",
	FILTER_CLOSE: "close to finished",
	FILTER_UNUSED: "known and unused",
	FILTER_NEW: "newly learned",
}

# ---- the header's tally ----------------------------------------------------------------------
## The counts the title bar states, in the order it states them: what you have, what is in hand,
## what is untouched, and the one that is a nudge rather than a fact.
const TALLY_KNOWN_FORMAT := "%d known"
const TALLY_LEARNING_FORMAT := "%d learning"
const TALLY_NOT_BEGUN_FORMAT := "%d not begun"
const TALLY_UNSPENT_FORMAT := "%d unspent"
const TALLY_SEPARATOR := " · "

# ---- the detail pane -------------------------------------------------------------------------
## **IT IS A READING, NOT A PLANNER.** No queue, no research order, no pathing, and nothing here is a
## button. If the pane reads as somewhere you SPEND something it has taught the wrong thing: a
## discovery is earned by practice, so the only way to get one is to go and do the work.
const DETAIL_PLACEHOLDER_HEAD := "What your people know"
const DETAIL_PLACEHOLDER_BODY := "Pick anything on the left to see what it lets your hands do, and how it is learned."

const DETAIL_HEAD_UNLOCKS := "What it lets you do"
const DETAIL_HEAD_PRACTISE := "How it is learned"
const DETAIL_HEAD_WHERE := "Where, now"

## The `Where, now` lines for a KNOWN ladder track — how many of the faction's own sources stand on
## what it unlocked, and, when none do, how many could.
const DETAIL_WHERE_IN_USE_FORMAT := "%d of your sources stand on it."
const DETAIL_WHERE_IN_USE_ONE := "1 of your sources stands on it."
const DETAIL_WHERE_UNSPENT_NONE := "None of your sources use it yet."
const DETAIL_WHERE_CRAFT_IN_USE := "Your people are holding or making things that are made of it."
const DETAIL_WHERE_CRAFT_UNSPENT := "Nothing your people hold or are making is made of it."
## What `Where, now` says about a knowledge that unlocks nothing to stand on. It is not an absence —
## the knowledge is working — so the line states the effect rather than a count of zero.
const DETAIL_WHERE_UNLOCKLESS := "It works wherever the thing it changes happens; there is nothing to stand on it."
## A node that is not known yet has no "where" at all, and saying "0 sources" about one would read as
## a shortfall rather than as a thing not yet learned.
const DETAIL_NEEDS_HEAD := "Not learned yet"
const DETAIL_NEEDS_LEARNING_FORMAT := "Learned %d%% of the way."
const DETAIL_NEEDS_NOT_BEGUN := "Your people have not started on this."

# ---- how each knowledge is LEARNED (authored client vocabulary) -----------------------------
## **THE HALF THAT EXISTED NOWHERE IN THE CLIENT.** The rule lived only in a Rust doc comment on
## `intensification_ladder.json` (`_comment_earns_knowledge`, one per rung), and a player was never
## told any of it — which is most of why the ladder read as something that happened TO them.
##
## **IT IS AUTHORED, NOT DERIVED, AND THAT IS FORCED.** The ladder's `earns_knowledge` field is not
## on the wire: the client sees a faction's PROGRESS on a track and the verb each track gates
## (`RungGates.RUNG_KNOWLEDGE_TRACKS`), but nothing that says which step TEACHES it. So these five
## sentences are a transcription of the config, which is authoritative — `plant:wild` earns
## `cultivation`, `plant:tended` earns `seed_selection`, `animal:wild` earns `herding`,
## `animal:pastoral` earns `penning`, `animal:pen` earns `foddering`. **Re-read the config, do not
## re-word these, if a rung's `earns_knowledge` ever moves.**
##
## The two "leave more standing" clauses are the `learn_multiplier` (`_comment_knowledge`: practice
## scales with `floor / MSY_BIOMASS_FRACTION`, and a crew that strips a source to nothing learns
## nothing) — and they appear on exactly the two tracks earned by DRAWING from a source. A rung-3
## source is tended rather than drawn from, so the floor axis has collapsed there
## (`intensification::MANAGED_SOURCE_FLOOR`) and the clause would be a lie on `penning` /
## `seed_selection` / `foddering`.
##
## The voice is the unlock notes' — what your people do, in the second person — so the two lines of
## the detail pane read as one paragraph rather than as two data fields.
const PRACTISE_NOTES := {
	"cultivation": "Your people learn it by gathering from wild patches — leave more standing and they learn faster.",
	"seed_selection": "Your people learn it by working patches they have already cultivated.",
	"herding": "Your people learn it by hunting wild herds — leave more standing and they learn faster.",
	"penning": "Your people learn it by keeping herds they have already tamed.",
	"foddering": "Your people learn it by keeping a herd in a pen.",
	# **THE ROUTE BRANCH'S TWO.** Neither carries a "leave more standing" clause: a road is not drawn
	# from, so there is no floor axis for practice to scale with — what teaches these is the road
	# being USED and being HELD. `route:trail` earns `roadbuilding`, `route:dirt_road` earns `paving`.
	"roadbuilding": "Your people learn it from a trail their own traffic keeps wearing in.",
	"paving": "Your people learn it by keeping a dirt road in good repair.",
}

## The craft half, which needs no table: every craft is learned the same way, and the sim charges the
## lesson per ITEM FINISHED rather than per turn worked (`intensification_ladder.json` →
## `_comment_knowledge_crafting`), which is the one thing a player has to know to make it happen.
const CRAFT_PRACTISE_NOTE := "Your people learn it by finishing things at the bench that are made of it."
## What a craft lets you do. The sim resolves the craft's DISPLAY NAME; this is the only sentence the
## client composes about one, and it says the thing a craft actually buys.
const CRAFT_UNLOCK_NOTE_FORMAT := "Things made of %s can be worked at a bench."

## ⛔ **`UNLOCKLESS_TRACKS` IS RETIRED — *step or capability* FALLS OUT OF THE CONFIG NOW.**
##
## It was a declared set holding exactly `foddering`, and it had to be declared because the client
## could not tell a knowledge that gates nothing from one somebody forgot to wire up. The roster's
## `is_step` answers that from the ladder itself — *does any rung's `unlock_knowledge` name this* —
## so `foddering` hangs off the bottom of the Herds column because the config says no rung waits on
## it, rather than because this file says so. A knowledge that stops gating a rung stops being a step
## with no second table to remember.
##
## The key the roster row carries it under.
const ROSTER_IS_STEP := "is_step"
## …and the rest of a roster row, which `KnowledgeRoster` reads and nothing else writes.
const ROSTER_KNOWLEDGE_ID := "knowledge_id"
const ROSTER_DISPLAY_NAME := "display_name"
const ROSTER_BRANCH := "branch"
const ROSTER_ORDER := "order"

# ---- the words -------------------------------------------------------------------------------
const PANEL_TITLE := "What your people know"
## The launcher's face. **A PLACEHOLDER ON PURPOSE**: the shipped icon is a drawn CAIRN
## (`docs/plan_knowledge_screen.md` §1) and the art is a separate piece of work, so the launcher
## ships on the same text-glyph seam `ACTION_CRAFTING`'s `⚒` uses. `▲` is the cairn's silhouette and
## is geometric, so it renders reliably where an emoji would not.
const LAUNCH_GLYPH := "▲"
const LAUNCH_TOOLTIP := "What your people know"
const CLOSE_GLYPH := "✕"
const CLOSE_TOOLTIP := "Close"

# ---- geometry --------------------------------------------------------------------------------
## The card's NOMINAL width; `AutoSizingPanel` fits the real one to the content, so this is the width
## the first frame is laid out at rather than a cap.
const PANEL_WIDTH := 900.0
const PANEL_MIN_HEIGHT := 260.0
const VIEWPORT_MARGIN := 12.0

## One domain column's floor. Wide enough for the longest node name plus its glyph, its meter and its
## percent on one line — a node name that wrapped would break the column's read as a ladder.
const COLUMN_MIN_WIDTH := 210.0
const DETAIL_WIDTH := 300.0
const COLUMN_SEPARATOR_THICKNESS := 1.0
const COLUMNS_PADDING_H := 18
const COLUMNS_PADDING_V := 18
const DETAIL_PADDING_H := 20
const DETAIL_PADDING_V := 18
const HEADER_SEPARATION := 14
const HEADER_PADDING_H := 18
const HEADER_PADDING_V := 14
const FILTER_ROW_SEPARATION := 6
const NODE_ROW_SEPARATION := 8
const NODE_ROW_PADDING_V := 5
## The row's own side padding. It is what the SELECTED row's leading bar and wash are drawn inside, so
## the two states have identical content margins and selecting a row never moves the column.
const NODE_ROW_PADDING_H := 6
## The SELECTED row's leading accent bar. A bar rather than an underline: this is a column of rows, and
## an underline under one of them reads as a rule between two rows rather than as a selection.
const NODE_SELECTED_BAR_THICKNESS := 2
## How far the unspent clause is indented, so it reads as a note about the row above rather than as a
## row of its own — the node glyph's own column plus the face's separation.
const NODE_CLAUSE_INDENT := 16
const DOMAIN_SEPARATION := 6
const COLUMN_SEPARATION := 20
const DETAIL_SECTION_SEPARATION := 12

## The ladder RAIL down a ladder column's left edge: the vertical hairline, and the gap between it
## and the node glyphs. It is what says *these are steps in an order*; the craft column draws none.
const RAIL_THICKNESS := 1.0
const RAIL_GUTTER := 10

## How much a node dimmed by the filter keeps. Non-matching nodes DIM rather than disappear, and this
## is how far — far enough to read as "not this" at a glance, not so far as to be unreadable, since
## the point of dimming instead of hiding is that the whole tree stays legible.
const FILTERED_OUT_ALPHA := 0.35

const TITLE_FONT_SIZE := 12
const TALLY_FONT_SIZE := 11
const FILTER_FONT_SIZE := 10
const DOMAIN_HEAD_FONT_SIZE := 10
const DOMAIN_SHAPE_FONT_SIZE := 9
const NODE_NAME_FONT_SIZE := 13
const NODE_VALUE_FONT_SIZE := 11
const NODE_CLAUSE_FONT_SIZE := 10
const DETAIL_TITLE_FONT_SIZE := 15
const DETAIL_HEAD_FONT_SIZE := 9
const DETAIL_BODY_FONT_SIZE := 12
const EMPTY_FONT_SIZE := 11

## Identity handles, so a harness finds a control by what it IS rather than by the face it happens to
## wear — the `HudWidgets.POLICY_RUNG_META` idiom. Each carries the thing's own key.
const NODE_META := "knowledge_node"
const FILTER_META := "knowledge_filter"
const DOMAIN_META := "knowledge_domain"
const RAIL_META := "knowledge_rail"
const TALLY_META := "knowledge_tally"
const EMPTY_NOTE_META := "knowledge_empty_note"

## The tint each state's name and glyph take. `not_begun` is `INK_FAINT` — GREYED, not hidden.
##
## A `static var` rather than a `const` because `HudStyle`'s palette is THEMED and is assigned at
## `apply_palette` time — the same contract `HudCraftingVocab` has, and `HudPalette` calls both.
static var NODE_INKS := {
	NODE_STATE_KNOWN: HudStyle.SIGNAL,
	NODE_STATE_LEARNING: HudStyle.INK,
	NODE_STATE_NOT_BEGUN: HudStyle.INK_FAINT,
}

static func apply_palette() -> void:
	NODE_INKS = {
		NODE_STATE_KNOWN: HudStyle.SIGNAL,
		NODE_STATE_LEARNING: HudStyle.INK,
		NODE_STATE_NOT_BEGUN: HudStyle.INK_FAINT,
	}
