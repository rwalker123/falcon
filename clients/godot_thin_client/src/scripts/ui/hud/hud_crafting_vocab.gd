class_name HudCraftingVocab

## The MATERIALS & CRAFTING panel's vocabulary leaf (`docs/plan_crafting_and_materials.md` §7) — the
## wire keys it reads, the words it says, and the geometry the prototype measured. A DECLARATION
## BLOCK, exactly like `HudWorkVocab` and its siblings: a new label or threshold goes HERE rather than
## as a fresh `const` on the panel, which is the rule that keeps a const block from regrowing into a
## merge-conflict surface. Its one function is `apply_palette` — the tints below are `static var`
## because `HudStyle`'s palette is themed, and they are assigned there rather than initialized.
##
## **NOTHING HERE IS A REFUSAL, A GRADE, A SHORTFALL OR A LIFE WORDING.** Every one of those is
## resolved sim-side and rendered VERBATIM (`snapshot.fbs` → CRAFTING & MATERIALS: *a client must
## never re-derive a reason, a shortfall number, a step-down or a life wording*). What this file holds
## is CHROME — column heads, group heads, the empty-cell dash, the crew caption — plus the two
## ownership words the panel says when a band holds NO units of an item, which are keyed off the
## published `group` and not off any string the sim resolved.
##
## DEPENDENCY DIRECTION: a vocab leaf reads nothing (`HudConst` is the model) — `HudStyle` is the one
## exception the family already makes, and it is a leaf too, so the pair stays acyclic.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# ---- the wire's own keys ------------------------------------------------------------------------
# The band's four crafting fields, decoded onto the cohort dict (`native/src/dict/population.rs`),
# and the four per-world catalogues on the snapshot dict (`native/src/dict/subsistence.rs`).

## `PopulationCohortState.materialBatches` — one row per pile of one material AT ONE RATING. Same
## material, same per-axis band ⇒ one batch; that merge is why a band hunting deer for two hundred
## turns holds one pile of hide rather than two hundred.
const BAND_MATERIAL_BATCHES_KEY := "material_batches"
const BATCH_MATERIAL_ID_KEY := "material_id"
const BATCH_AMOUNT_KEY := "amount"
const BATCH_READINGS_KEY := "readings"
const READING_AXIS_KEY := "axis"
## The EXACT amount-weighted reading. On screen the panel says the BAND; this rides for the tooltip
## and for every downstream reader, because two `good` hides are not interchangeable.
const READING_VALUE_KEY := "value"
const READING_BAND_NAME_KEY := "band_name"

## `PopulationCohortState.bench` — one job at a time, so the panel never has to explain a queue. An
## empty `recipe_id` is an IDLE bench, which is a different statement from a BLOCKED one.
const BAND_BENCH_KEY := "bench"
const BENCH_RECIPE_ID_KEY := "recipe_id"
const BENCH_DISPLAY_NAME_KEY := "display_name"
const BENCH_WORKERS_KEY := "workers"
const BENCH_PROGRESS_KEY := "progress"
const BENCH_WORK_KEY := "work"
const BENCH_TEACHES_KEY := "teaches"
const BENCH_BLOCKED_REASON_KEY := "blocked_reason"
## **HOW THAT REFUSAL SHOULD READ, in the OFFER's own severity vocabulary** — `danger` / `neutral` /
## `good`, `""` while nothing is blocking. A bench waiting for its crew is the normal state one click
## after **Make** — the player staffs the bench — and resolves `neutral`, while a shortage, an unknown
## craft or a zero craft rate resolve `danger`. It rides beside the reason for the same reason the
## reason rides at all: **the client does not decide which refusals are serious**, and one tinting them
## all alike renders the expected state as a fault. Read through `REASON_COLORS`, the SAME table the
## ledger's offer rows resolve through, so the bench and the ledger cannot disagree about what a
## published severity looks like.
const BENCH_BLOCKED_SEVERITY_KEY := "blocked_severity"
const BENCH_ITEMS_COMPLETED_KEY := "items_completed"
const BENCH_OUTPUT_GRADE_KEY := "output_grade"
## **WHAT ONE TURN ADDS, RESOLVED SIM-SIDE** — `workers × progress_per_worker_turn × craft_speed`,
## where `craft_speed` is the equipped bench tool's rate or the material's bare-handed one. **Never
## re-derive it**: that join is the same one `kitTiers` exists to keep sim-side, and a client
## multiplying crew by work would read a bare-handed organic's 0.5 as 1.0 and promise a finish in
## half the turns it takes. `0` is a STATE — no crew, no recipe, or a craft speed of zero — and the
## refusal beside it says which.
const BENCH_RATE_PER_TURN_KEY := "rate_per_turn"
## **THE PILE ALREADY CUT, so a clear can name what it destroys.** The WITHDRAWN amounts, not the
## recipe's inputs and not a shortfall's `required` — a bench tool's material efficiency sits between
## the book and the withdrawal, and naming what will really be lost is the whole point. Empty on an
## undrawn bench.
const BENCH_DRAWN_INPUTS_KEY := "drawn_inputs"
const DRAWN_INPUT_MATERIAL_ID_KEY := "material_id"
const DRAWN_INPUT_AMOUNT_KEY := "amount"
## **THE PLAYER'S OWN RANK ON THIS BENCH** (`docs/plan_standing_upkeep.md` §4.9 item 9b) — the same
## `SourcePriority` a worked row carries, arriving as the LOWERCASE WORD the decoder converted the
## ordinal into. The vocabulary itself is `HudWorkVocab`'s (`WORK_PRIORITY_LEVELS` / `_FACES` /
## `work_priority_of`) and is deliberately NOT restated here: the bench competes with the gathering
## rows in one shedding walk, so one property means one word list — a second spelling would be the
## drift the wire's shared enum exists to prevent.
##
## `"normal"` on an unmarked bench, on an IDLE one and on a band with no bench at all, so the panel
## reads it the same way in all three.
const BENCH_PRIORITY_KEY := "priority"
## How the rank's line-two prefix is found by IDENTITY — it is a `HudWorkVocab` face this panel does
## not compose, so an assertion matching on its wording would be matching on the other module's
## spelling. Valued the LEVEL, which is what the harness actually wants to read back.
const BENCH_PRIORITY_META := "crafting_bench_priority"
## …and the handle on the LINK that opens the picker. Its face is `HudWorkVocab.WORK_INSPECT_PRIORITY`,
## the same word the work inspector's link wears, so a search by text finds a control in two panels and
## cannot say which — the `CLEAR_BENCH_META` argument, one control over.
const BENCH_PRIORITY_LINK_META := "crafting_bench_priority_link"

## `PopulationCohortState.craftOffers` — ONE ROW PER RECIPE, ALWAYS. `reason` + `severity` are the
## contract rather than `available`: *"Not needed yet"* is a shrug and *"Short 4.9 bone"* is a
## problem, and a client deriving both from a boolean cannot tell them apart.
const BAND_CRAFT_OFFERS_KEY := "craft_offers"
const OFFER_RECIPE_ID_KEY := "recipe_id"
const OFFER_DISPLAY_NAME_KEY := "display_name"
const OFFER_GROUP_KEY := "group"
## The equipment id this recipe makes, `""` for a material recipe — the JOIN key onto the band's
## `equipment_batches`, which is what supplies the row's tier, count and life.
const OFFER_OUTPUT_ITEM_ID_KEY := "output_item_id"
const OFFER_AVAILABLE_KEY := "available"
const OFFER_REASON_KEY := "reason"
const OFFER_SEVERITY_KEY := "severity"
const OFFER_SHORTFALLS_KEY := "shortfalls"
const OFFER_ON_BENCH_KEY := "on_bench"
## **THE GROUP HEAD** — the tier this recipe would be MADE at right now, and its rank in the item's
## own tier list. The kit group splits by the name and orders its heads by the rank DESCENDING, which
## is the only honest ordering a client has: alphabetical would put Iron above Bronze.
const OFFER_OUTPUT_TIER_NAME_KEY := "output_tier_name"
const OFFER_OUTPUT_TIER_RANK_KEY := "output_tier_rank"
## **WHAT THE BAND CARRIES, SAID ONLY WHEN IT IS NEWS** — `carrying flint · poor`, `last flint set
## wore out`, `""` the rest of the time. It is the ONE route by which a tier word reaches the Owned
## cell: resolved sim-side, rendered verbatim, never composed and never re-derived here.
const OFFER_OWNED_NOTE_KEY := "owned_note"

## `MaterialShortfall` — the number a refusal names. `required` is already net of the bench tool's
## material efficiency, which is why the cost cell prefers it over the recipe's own input amount.
const SHORTFALL_MATERIAL_ID_KEY := "material_id"
const SHORTFALL_REQUIRED_KEY := "required"
const SHORTFALL_SHORT_KEY := "short"

## `PopulationCohortState.equipmentBatches`. **`count == 0` MEANS THE BAND OWNS NONE** and is the only
## honest ownership test: a batch that runs out of units is removed, so worn-out and never-made both
## read 0 here.
##
## **THE LEDGER READS NO CONDITION WORDING FROM THIS ARRAY.** How worn the gear is has one home — the
## Band panel's WORKFORCE role cards, off `kit_item_conditions` — and this panel answers what a
## rebuild costs, so `life`, `quanta_left` and `quantum_noun` have no key here. The two condition
## numbers below are read as a RANKING and as a threshold, never printed.
##
## **`tier_id` HAS NO KEY HERE EITHER.** The tier a row would be MADE at is the ledger's group head
## (`OFFER_OUTPUT_TIER_NAME_KEY`), and the tier the band CARRIES reaches the Owned cell only through
## the sim's resolved `ownedNote` — and only when the two disagree. A cell rendering this field would
## say `flint` on every row of the early game, which is exactly the column the head replaced.
const BAND_EQUIPMENT_BATCHES_KEY := "equipment_batches"
const EQUIPMENT_ITEM_ID_KEY := "item_id"
const EQUIPMENT_GRADE_KEY := "grade"
const EQUIPMENT_COUNT_KEY := "count"
## Condition left on the unit IN HAND, 0–100. Read twice and shown never: it breaks the urgency sort's
## severity ties, and `>= 100` is the "nothing spent yet" test that dims a shrug row.
const EQUIPMENT_REMAINING_KEY := "remaining"
## Which severity band that condition falls in — the urgency sort's primary key.
const EQUIPMENT_LIFE_SEVERITY_KEY := "life_severity"

## The per-world catalogues, on the snapshot dict beside `kits`.
const MATERIAL_ID_KEY := "id"
const MATERIAL_CRAFT_KEY := "craft"
const MATERIAL_AXES_KEY := "axes"
const BAND_LEGEND_NAME_KEY := "name"
const RECIPE_ID_KEY := "id"
const RECIPE_DISPLAY_NAME_KEY := "display_name"
## **THE CRAFT THIS RECIPE NEEDS** (`RecipeDefState.craft`) — the join from a craft KNOWLEDGE track to
## the things that craft makes. Decoded all along (`native/src/dict/subsistence.rs`) and unread until
## the knowledge screen: `KnowledgeRoster.craft_is_in_use` walks the book for recipes of one craft and
## asks whether the faction holds or is making any of their outputs, which is the whole of that
## panel's "nothing is using it" verdict for the craft half.
const RECIPE_CRAFT_KEY := "craft"
const RECIPE_GROUP_KEY := "group"
const RECIPE_INPUTS_KEY := "inputs"
const RECIPE_OUTPUTS_KEY := "outputs"
const RECIPE_INPUT_MATERIAL_ID_KEY := "material_id"
const RECIPE_INPUT_AMOUNT_KEY := "amount"
const RECIPE_OUTPUT_MATERIAL_ID_KEY := "material_id"
const RECIPE_OUTPUT_EQUIPMENT_ID_KEY := "equipment_id"
const RECIPE_OUTPUT_AMOUNT_KEY := "amount"
const CRAFT_KNOWLEDGE_FACTION_KEY := "faction"
const CRAFT_KNOWLEDGE_CRAFT_ID_KEY := "craft_id"
## "Bone-working" — the id, hyphenated and capitalized, RESOLVED SIM-SIDE. The client never maps a
## craft id to English itself.
const CRAFT_KNOWLEDGE_DISPLAY_NAME_KEY := "display_name"
const CRAFT_KNOWLEDGE_KNOWN_KEY := "known"
const CRAFT_KNOWLEDGE_PROGRESS_KEY := "progress"
## What `progress` has to reach. It rides so the client draws NO scale of its own — a meter whose
## denominator was guessed would disagree with the sim's own reading of the same track.
const CRAFT_KNOWLEDGE_THRESHOLD_KEY := "completion_threshold"

# ---- the ledger's groups, and the heads they fold under -------------------------------------------
## `CraftOffer.group`. The KIT group SPLITS by `output_tier_name` — one head per tier, ordered by
## `output_tier_rank` descending — and the other two take one head each; all three kinds are built by
## the same head builder so they read as one family.
const GROUP_KIT := "kit"
const GROUP_TOOL := "tool"
const GROUP_STOCK := "stock"
## The order the two NON-tier groups follow the tier heads in: the bench TOOLS, then the recipes that
## make STOCK rather than kit.
const GROUP_ORDER: Array[String] = [GROUP_TOOL, GROUP_STOCK]
## **A HEAD IS A NAME AND A CARET, AND NOTHING ELSE.** These carried a trailing explanation
## (`Bench tools — each stretches one material`) until the heads became foldable: a caret invites a
## click, and a clause after the name reads as part of what is being folded away. The rationale it
## carried belongs in `.claude/rules/client/crafting-panel.md`, not on screen.
const GROUP_HEADS := {
	GROUP_TOOL: "Bench tools",
	GROUP_STOCK: "Materials",
}

## **EVERY HEAD FOLDS**, and the caret is what says which way. A folded group keeps its head and
## hides its rows, so the ledger can be narrowed to the one group being worked on without losing the
## way back.
const GROUP_HEAD_CARET_OPEN := "▾"
const GROUP_HEAD_CARET_FOLDED := "▸"
const GROUP_HEAD_FORMAT := "%s %s"
## How a head is found by IDENTITY rather than by its face — the `HudWidgets.POLICY_RUNG_META` idiom.
## The meta carries the head's own name, which is also the key its fold state is held under.
const GROUP_HEAD_META := "crafting_group_head"

# ---- the words ----------------------------------------------------------------------------------
const PANEL_TITLE := "⚒ Materials & Crafting"
const LAUNCH_GLYPH := "⚒"
const LAUNCH_TOOLTIP := "Materials & Crafting"
const CLOSE_GLYPH := "✕"
const CLOSE_TOOLTIP := "Close"
const BAND_FIELD_KEY := "Band:"
const BAND_PICKER_TOOLTIP := "Which band's materials and bench"
const CYCLE_PREV_GLYPH := "◀"
const CYCLE_NEXT_GLYPH := "▶"
const CYCLE_PREV_TOOLTIP := "Previous band"
const CYCLE_NEXT_TOOLTIP := "Next band"
const CYCLE_COUNT_FORMAT := "%d / %d"

const RAIL_HEAD := "On hand"
const BENCH_HEAD := "The bench"
## What the rail says when the band holds no material at all. Not an error — a band that has banked
## nothing yet simply has nothing on hand.
const RAIL_EMPTY := "Nothing banked yet"
## The craft track on a material group's header: the meter, then the craft's own name, then how far
## along it is. `known` is a WORD rather than 100%, because a learned craft is not a fuller meter.
const CRAFT_TRACK_FORMAT := "%s %s · %s"
const CRAFT_TRACK_KNOWN := "known"
const CRAFT_TRACK_PROGRESS_FORMAT := "%d%%"
const CRAFT_TRACK_METER_CELLS := 5
## How a batch's amount reads. Two decimals would be noise on a pile measured in tens.
const BATCH_AMOUNT_FORMAT := "%.1f"
## One characteristic chip: the AXIS, then how that axis rates. **THE BAND RATES THE AXIS, NOT THE
## MATERIAL** — `tough: excellent` says the toughness is excellent and makes no claim about the hide,
## which is what lets ordinary quality words coexist with there being no best hide.
const CHARACTERISTIC_CHIP_FORMAT := "%s: %s"

const BENCH_IDLE_TITLE := "Nothing on the bench"
const BENCH_IDLE_SUB := "Press Make on a row below to put it up."
## What the bench is making: **the THING first, then the craft, with a separator between them.** The
## two nouns were juxtaposed (`Tanning Hurdles`) until it was reported as a different item from the
## `Hurdles` row in the ledger below — and the collision is not that row's bad luck, it is the shape:
## every craft name in the book is a gerund, so `Weaving Baskets` reads as a verb phrase and
## `Bone-working Crook` as a compound noun. The separator is the `·` the sub line and the cost cell
## already join clauses with. **A bench whose craft the sim has not published prints the thing alone**
## rather than a dangling separator.
const BENCH_TITLE_FORMAT := "%s · %s"
## Its progress, in the units the sim keeps it in: the recipe's own `work`, accrued against the
## pass's cost. **THE UNIT IS `work`, NOT `worker-turns`** — a worker-turn is not what a worker does
## in a turn (bare-handed `craft_speed` is 0.5, so two crafters deliver 1.0), and the old name
## invited a division by the crew that is wrong by exactly the tool multiplier. What a turn really
## adds is published, and says so on the line beside this one.
const BENCH_PROGRESS_FORMAT := "%.1f of %.0f work"
## What a turn adds, rendered VERBATIM from `rate_per_turn` — never recomputed from the crew.
const BENCH_RATE_FORMAT := "+%.1f/turn"
## **THE FINISH ESTIMATE, `ceil((work − progress) / rate_per_turn)`** — exact arithmetic over three
## published numbers, which is the boundary the sim's forecast rule draws: where a closed form
## exists it ships the terms and the client evaluates them. *"done in 1 turn"* reads poorly, so one
## turn is spelled as the next one.
const BENCH_ESTIMATE_FORMAT := "done in %d turns"
const BENCH_ESTIMATE_NEXT_TURN := "done next turn"
## A bench about to finish still owes a turn to do it in, so the estimate floors at one rather than
## claiming a job is already done.
const BENCH_ESTIMATE_MIN_TURNS := 1
const BENCH_ITEMS_COMPLETED_FORMAT := "%d finished"
const BENCH_GRADE_FORMAT := "this pile → %s"
const BENCH_SUB_SEPARATOR := " · "
const BENCH_CREW_CAPTION := "Crafters"
const BENCH_CREW_DECREMENT := "−"
const BENCH_CREW_INCREMENT := "+"

## **TAKING THE JOB OFF THE BENCH — `clear_bench <faction> <band>`.** Offered only on a bench that
## HAS a job: an idle bench has nothing to clear, so the control is absent rather than dead.
##
## **IT WEARS THE `armed` TREATMENT, WHICH IS WHAT KEEPS IT APART FROM THE HEADER'S ✕.** The card
## header carries the same glyph for "close this panel"; this one destroys committed materials. They
## are told apart by three things at once — this one sits INSIDE the bench well's bordered box, it is
## inset from the card's right edge by the crew stepper, and it is drawn in the destructive variant
## the pause menu's Abandon / Quit already use rather than the header's quiet ghost.
const CLEAR_BENCH_GLYPH := "✕"
## **THE TOOLTIP NAMES WHAT WILL BE DESTROYED, off `drawn_inputs`.** Composed from the WITHDRAWAL, so
## it states what the store really loses; a tooltip built from the recipe's inputs would name a
## different number the moment a bench tool's material efficiency applies. There is no confirmation
## dialog — the cost is stated in text, which is this panel's idiom for a consequence.
const CLEAR_BENCH_TOOLTIP_FORMAT := "Clear the bench — %s already cut are spent"
## What it says when nothing has been withdrawn yet: the bench can still be cleared, and clearing it
## costs nothing.
const CLEAR_BENCH_TOOLTIP_NOTHING := "Clear the bench — nothing has been cut yet"
## One drawn material, and the separator between two of them — the cost cell's own clause shape, so
## the pile reads the way a rebuild cost does.
const CLEAR_BENCH_CLAUSE_FORMAT := "%s %s"
const CLEAR_BENCH_SEPARATOR := " · "
## How it is found by IDENTITY — and here that is not a convenience but the only way: the header's
## close button wears the SAME glyph, so a search by face finds two buttons and cannot say which is
## which.
const CLEAR_BENCH_META := "crafting_clear_bench"
## **THE REFUSAL IS A LINE OF ITS OWN, BENEATH THE PROGRESS LINE** — how far along the job is and why
## it is stopped are two facts, and a stopped bench raises both at once. How the blocked line is found
## by IDENTITY, since its text is a sim string the client cannot predict: an assertion about the
## refusal that matched on wording would be matching on the sim's spelling, and one that scanned for
## the danger ink would also catch every refused row in the ledger below.
const BENCH_BLOCKED_META := "crafting_bench_blocked"
const BENCH_TEACH_FORMAT := "Teaching %s — every one finished teaches it."
## Crafting is the FOURTH TEACHER, but a bench with no lesson to credit says nothing rather than
## saying "teaches nothing".
const BENCH_TEACH_NONE := ""

## **FOUR COLUMNS: Item · Owned · Rebuild costs · action.** There is no condition column — the role
## cards on the Band panel state how worn each kit's item is, and this table states what replacing it
## costs. **Tier is not a column either**: it could only ever say `flint` for the whole early game, so
## it is a foldable group HEAD and what the column says instead is what the band actually owns.
const LEDGER_COLUMN_ITEM := "Item"
const LEDGER_COLUMN_OWNED := "Owned"
const LEDGER_COLUMN_COST := "Rebuild costs"
## The action column's head is deliberately blank — a column of buttons names itself.
const LEDGER_COLUMN_ACTION := ""

const MAKE_LABEL := "Make"
## The running row's button is SPENT — one job at a time, so the row already on the bench has nothing
## left to ask for. (What it is missing is a CREW, not a second press of Make, and the well's stepper
## is where that is asked.)
const ON_BENCH_LABEL := "On the bench"
## The empty cell. A recipe with no inputs has no cost to report, and a dash is the honest reading of
## that — never a zero.
const EMPTY_CELL := "—"

## **WHEN THE BAND OWNS NO UNITS, ONE WORDING FOR EVERY ROW**, keyed off `count` — never off
## `remaining == 0`, since a spent batch is REMOVED and worn-out and never-made both read zero
## condition.
##
## ⛔ **IT ANSWERS THE COLUMN IT SITS UNDER, WHICH IS `Owned`.** This split into two consequence
## wordings — `Bare hands` for a kit, `Not made` for a bench tool — and the kit half was nonsense the
## moment it was visible: *"Owned: Bare hands"* tells a player they own their own hands. It went
## unnoticed for as long as it did only because a spawn stocked a party's worth of every kit, so the
## cell never had to say *none* about a kit at all; removing the default stock put it on eleven rows
## at once. **A consequence wording still has to be an answer to the question in the column head.**
## `Not made` is one — it says the band has none and never built any. `×0` is the same fact and the
## worse sentence, which is why the cell is words rather than a count.
##
## The kit/tool distinction survives as **colour, not wording** (`DANGER` against `INK_FAINT` at the
## call site): a kit the band lacks has people working bare-handed right now, a bench tool it never
## built does not.
const OWNED_NONE := "Not made"
## **ONE LINE PER GRADE**, counts summed across the batches that share one — two `good` batches at
## different wear are one line of `×5`, wear not being this panel's fact. Best grade first.
const OWNED_COUNT_FORMAT := "×%d"
## **WHAT THE BAND HOLDS OF A CRAFTED MATERIAL** — no wording of its own, because the pile reads as
## ONE TOTAL in the rail's own `BATCH_AMOUNT_FORMAT` (`9.3`), summed across the material's batches.
## **A stock recipe owns something and the panel knew it all along** — this cell stated the recipe's
## YIELD (`→ 1 hurdles`) under a column head that says *Owned*, while the band's 5.0 hurdles sat in
## `material_batches` being drawn in the rail 250px to its left. The yield moved to the Rebuild-costs
## cell, where an arrow has a left-hand side (see `COST_YIELD_FORMAT`).
##
## **NO RATING CHIPS AND NO PER-BATCH SPLIT HERE** — both are the RAIL's fact, drawn 250px wide and
## always visible, and at shipped axis names (`stoutness` / `span`) a chip pair wraps inside
## `COLUMN_OWNED_WIDTH`, so the duplicate cost the ledger real height. Without the chips the split
## would be two indistinguishable numbers anyway; equipment splits by GRADE because a grade is
## rendered nowhere else.
##
## **THE AMOUNT IS FRACTIONAL AND THE `×` IS DELIBERATELY ABSENT.** Equipment is counted (`×26`
## sleds); a material is measured — 22.8 hide, 5.0 hurdles — and an `×5` here would be a lie the first
## turn a pile is not whole. It is SIZED like the count (`OWNED_COUNT_FONT_SIZE`) all the same: the two
## answer the same question about different kinds of thing.
const OWNED_STOCK_NONE := "None on hand"
## How the Owned cell is found by IDENTITY. It carries the row's own item id, so a claim about what
## reaches the CELL (a tier word, an owned note) can be scoped to the cell rather than to the ledger —
## the group HEAD is a tier word by design, and a panel-wide text scan cannot tell the two apart.
const OWNED_CELL_META := "crafting_owned_cell"

## `CraftOffer.severity` and `EquipmentBatchState.lifeSeverity` — two vocabularies on purpose (an
## offer is an invitation, a condition band is a reading of wear, so `good` means nothing on the
## second). Only the offer's is rendered here; the life one is the urgency sort's key.
const SEVERITY_DANGER := "danger"
const SEVERITY_NEUTRAL := "neutral"
const SEVERITY_GOOD := "good"
const LIFE_SEVERITY_HEALTHY := "healthy"
const LIFE_SEVERITY_WARN := "warn"
const LIFE_SEVERITY_DANGER := "danger"

## The tint each published severity renders in — **ONE table, read by every surface that renders a
## sim-resolved refusal**: the ledger's offer rows and the bench well's blocked line. A second table
## for the bench would be a second opinion about what `danger` looks like, and the bench's severity is
## published in this exact vocabulary precisely so it does not need one.
## **BUILT IN `apply_palette` AT THE END OF THIS BLOCK, NOT HERE** — its values are themed `HudStyle`
## colours, which are `static var`s: a `const` table is a parse error against one, and a static-var
## initializer would freeze at whatever palette was loaded before the theme was installed. Every named
## tint below it is assigned there for the same reason.
static var REASON_COLORS := {}
## What an unrecognised severity renders in — and `neutral`'s own tint, which is the same answer: a
## refusal the sim did not mark as a problem reads QUIET rather than as an alarm. Named so both
## readers of `REASON_COLORS` fall back to one ink instead of each spelling the default itself.
static var REASON_COLOR_QUIET: Color = Color()
## How the urgency sort ranks a row: worn first, untouched last. Read off the PUBLISHED life
## severity, so the order the player sees is the sim's own reading of what is running out.
const LIFE_SEVERITY_RANK := {
	LIFE_SEVERITY_DANGER: 0,
	LIFE_SEVERITY_WARN: 1,
	LIFE_SEVERITY_HEALTHY: 2,
}
const LIFE_SEVERITY_RANK_UNKNOWN := 3

## The rail's chips: the TOP band of the shared rating vocabulary reads as a strength, the BOTTOM as
## a weakness, and everything between stays quiet. Both ends come from the published legend
## (`characteristic_bands`, ascending) rather than from a threshold typed here — the sim owns the cut
## points, and a client with its own would disagree with the word beside them.
static var CHIP_HIGH_COLOR: Color = Color()
static var CHIP_LOW_COLOR: Color = Color()
static var CHIP_NEUTRAL_COLOR: Color = Color()

## **THE OWNED GRADE CHIP'S TINT IS THE BAND'S POSITION IN THE PUBLISHED LEGEND, never a match on its
## name.** `characteristic_bands` rides the wire ascending and the rail above already takes its
## strength/weakness reading off the legend's ENDS; a `{"excellent": HEALTHY}` table here would be a
## client-side copy of a vocabulary the sim owns, and would silently render every chip neutral the day
## a band is renamed or a fifth one is added. Resolved by INDEX: the last rung reads as the best work
## the ladder has, the rung below it as good work, the first as the bottom of the ladder, and anything
## between stays quiet. A grade the legend does not contain renders NO chip at all — a start-stocked
## unit publishes `""`, and a spawn's kit was never on a bench and makes no quality claim.
static var OWNED_GRADE_TOP_COLOR: Color = Color()
static var OWNED_GRADE_HIGH_COLOR: Color = Color()
static var OWNED_GRADE_LOW_COLOR: Color = Color()
static var OWNED_GRADE_MID_COLOR: Color = Color()

## The `ownedNote`'s own tint. It is news rather than an alarm — the band is carrying something older
## than what it could now make — so it reads in the warn ink, one step short of a refusal's danger.
static var OWNED_NOTE_COLOR: Color = Color()

## Install the current `HudStyle` palette into this file's tints. Called by `HudPalette.apply()` after
## `HudStyle.apply_palette`; it takes no palette of its own, because none of these is a colour in its
## own right — each is one HUD ink re-stated in the crafting panel's vocabulary.
static func apply_palette() -> void:
	REASON_COLORS = {
		SEVERITY_DANGER: HudStyle.DANGER,
		SEVERITY_GOOD: HudStyle.SIGNAL,
	}
	REASON_COLOR_QUIET = HudStyle.INK_FAINT
	CHIP_HIGH_COLOR = HudStyle.SIGNAL
	CHIP_LOW_COLOR = HudStyle.INK_FAINT
	CHIP_NEUTRAL_COLOR = HudStyle.INK_DIM
	OWNED_GRADE_TOP_COLOR = HudStyle.HEALTHY
	OWNED_GRADE_HIGH_COLOR = HudStyle.SIGNAL
	OWNED_GRADE_LOW_COLOR = HudStyle.INK_FAINT
	OWNED_GRADE_MID_COLOR = HudStyle.INK_DIM
	OWNED_NOTE_COLOR = HudStyle.WARN

# ---- geometry, measured off the prototype -------------------------------------------------------
## The panel's NOMINAL width. It is a floor, not a cap: the card refits to its content through
## `AutoSizingPanel.fit_width`, so a long recipe name widens the card instead of clipping the table.
const PANEL_WIDTH := 960.0
const PANEL_MIN_HEIGHT := 240.0
## Clearance kept between the card and the viewport edges — the same margin `BandComposeFloat` keeps.
const VIEWPORT_MARGIN := 12.0

## The materials rail, fixed. The main column takes whatever is left.
const RAIL_WIDTH := 250.0
const RAIL_PADDING_H := 18
const RAIL_PADDING_V := 20
const MAIN_PADDING_H := 22
const MAIN_PADDING_V := 20
const COLUMN_SEPARATOR_THICKNESS := 1.0

const HEADER_SEPARATION := 18
const HEADER_PADDING_H := 16
const HEADER_PADDING_V := 13
const ICON_BUTTON_SIZE := 26.0
## The band picker's own width. **`build_option_picker` sets `clip_text` and turns
## `fit_to_longest_item` OFF**, so its minimum width is the arrow alone and a picker left to its
## minimum in a non-expanding row renders as a bare caret with the band's name clipped to nothing.
## The prototype's own 150.
const BAND_PICKER_MIN_WIDTH := 150.0

## The ledger's four columns. ITEM expands into the slack; the other three are fixed so the table
## reads as columns rather than as four independently-wrapping stacks.
const COLUMN_ITEM_MIN_WIDTH := 150.0
## The prototype's OWNED column. It is wider than the 104 the retired Tier column took because it
## carries a count and a grade chip on one line, and `ownedNote` — a whole clause — under them.
const COLUMN_OWNED_WIDTH := 172.0
const COLUMN_COST_WIDTH := 140.0
## **THE ACTION COLUMN IS SIZED BY THE REFUSAL, NOT BY THE BUTTON.** `Make` is 40-odd pixels wide;
## what sets this number is the published `reason` under it, which is a whole clause — *"Hide +
## tanning frame → excellent"*, *"Short 5.0 fibre · Short 1.0 hide"* — autowrapped to exactly this
## width. At the 132 it shared with the retired condition column those clauses ran to two lines and
## every row in the table paid for it, so retiring that column bought the width back here rather than
## narrowing the card.
const COLUMN_ACTION_WIDTH := 210.0
const COLUMN_SEPARATION := 10

const BAR_CORNER_RADIUS := 2
const BENCH_BAR_HEIGHT := 3.0
const ROW_SEPARATION := 4
const GROUP_HEAD_TOP_MARGIN := 14
const LEDGER_ROW_PADDING_V := 7
const RAIL_GROUP_PADDING_V := 10
const SECTION_SEPARATION := 22

## The dim a row wears when its offer is a shrug rather than a problem — *"Not needed yet"*, sorted
## last and DIMMED rather than hidden, because a kit you own and never use is information too.
const DIMMED_ROW_ALPHA := 0.55

const TITLE_FONT_SIZE := 12
const ZONE_HEAD_FONT_SIZE := 10
const MATERIAL_NAME_FONT_SIZE := 11
const CRAFT_TRACK_FONT_SIZE := 10
const BATCH_AMOUNT_FONT_SIZE := 14
const CHIP_FONT_SIZE := 10
const BENCH_TITLE_FONT_SIZE := 15
const BENCH_SUB_FONT_SIZE := 12
## The refusal reads at the progress line's own size rather than the teach line's: it sits directly
## under the progress line and the two are the same weight of fact, so a smaller one would read as a
## footnote to the job rather than as the reason it is not moving.
const BENCH_BLOCKED_FONT_SIZE := 12
const BENCH_TEACH_FONT_SIZE := 11
const CREW_COUNT_FONT_SIZE := 18
const CREW_CAPTION_FONT_SIZE := 10
const CREW_BUTTON_SIZE := 24.0
const CREW_BUTTON_FONT_SIZE := 13
const COLUMN_HEAD_FONT_SIZE := 10
const ITEM_NAME_FONT_SIZE := 14
const ITEM_ROLE_FONT_SIZE := 11
const OWNED_CHIP_FONT_SIZE := 10
## The `×3` beside the chip — the count is the number the eye goes to, so it reads a size up from the
## grade chip rather than matching it.
const OWNED_COUNT_FONT_SIZE := 13
const OWNED_NOTE_FONT_SIZE := 10
const EMPTY_CELL_FONT_SIZE := 11
const COST_FONT_SIZE := 12
const ACTION_FONT_SIZE := 12
const REASON_FONT_SIZE := 11
const GROUP_HEAD_FONT_SIZE := 10

## The letter-spacing effect the prototype's uppercase eyebrows carry is not expressible in Godot's
## Label, so the heads are simply uppercased — the same treatment `HudWidgets.alloc_section_label`
## already gives every zone head in this HUD.

## The Item cell's second line — what this row IS, every one of them a JOIN of published fields
## rather than an authored table: a tool names the material it bounds (`materials[].tool_item_id`), a
## stock recipe names the CRAFT and the characteristic its input is SPENT worst-first on
## (`inputs[].reads_axis`), and a kit row names the craft that makes it (`RecipeDefState.craft`, whose
## display name is resolved sim-side). A row whose join finds nothing simply shows no second line.
##
## **THE STOCK LINE NAMES NO MATERIAL, and leads with the craft like every other row.** It read
## `Reads hide suppleness` until it was reported as meaningless, and both halves of that were wrong:
## two bare nouns collide with no separator between them, and the *material* is the one fact the row
## already states twice over — the Rebuild-costs cell beside it says `4 wood · 2 hide`, and the sim's
## own refusal under the button says `Hide, no tanning frame`. Leading with the craft is also what
## keeps line two a CATEGORY on every row — the stock row was the only one whose line two was a
## sentence.
##
## **⛔ THE AXIS IS A SPEND ORDER HERE, NOT A QUALITY. THIS LINE SAID `%s · quality from %s` AND THE
## CLAIM WAS FALSE ON EVERY ROW IT COULD EVER RENDER ON.** It was justified as "the whole of *there is
## no best hide*: the same pelt makes a good sled (reads toughness) and poor hurdles (reads
## suppleness)" — and *poor hurdles* is the dead claim. **A grade is a property of EQUIPMENT**: it is
## stamped on an equipment batch and read back off it, while a `MaterialBatch` carries its own
## `characteristics` and has nowhere to put one. So `RecipeDef::grade_for` resolves NO grade for a
## recipe whose outputs are all materials, `CraftOffer.outputGrade` publishes `""`, and the refusal
## renders `Hide, no tanning frame` with no dangling arrow. Every set of hurdles the bench ever makes
## carries the recipe's own `stoutness 0.6 / span 0.6`, whatever hide went into it.
##
## **THE AXIS STAYS ON THE LINE BECAUSE IT STILL DOES SOMETHING REAL.** `reads` is OVERLOADED
## sim-side: `systems/crafting.rs::spend_axis` falls through the same field to pick WHICH batches of
## the input are consumed, worst-first on that axis. On a material-only recipe only that half
## survives — the bench eats your least supple hide and leaves the supple one alone — and it is a fact
## the player can learn nowhere else on this panel, so dropping the clause would cost him it. The line
## states the half that is true.
##
## **THIS FORMAT RENDERS ONLY ON A MATERIAL-ONLY RECIPE, AND NEEDS NO GATE OF ITS OWN.** Its one
## caller is `CraftingPanel._role_line`'s `group == GROUP_STOCK` branch, and `stock` is the wire's own
## word for *"this recipe makes a material"* — the sim derives it as `output_equipment_id().is_none()`
## (`snapshot/crafting.rs::group_of`), the same predicate as `outputs_only_materials()` and the same
## one that makes `outputItemId` empty. A second test against `outputGrade == ""` would be BROADER
## than the fact wanted, that field also emptying on a recipe that reads no characteristic at all.
const ROLE_TOOL_FORMAT := "Bench tool — %s"
const ROLE_STOCK_FORMAT := "%s · worst %s spent first"
## The same line on a recipe whose craft the sim has not published — the axis clause alone,
## capitalized, rather than a leading separator with nothing before it.
const ROLE_STOCK_NO_CRAFT_FORMAT := "Worst %s spent first"

## The cost cell's per-material clause: the amount, then the material.
const COST_CLAUSE_FORMAT := "%s %s"
const COST_SEPARATOR := " · "
## **WHAT A PASS YIELDS, ON ITS OWN LINE UNDER WHAT IT COSTS** — `4 wood · 2 hide` and then
## `→ 1 hurdles` beneath it. It sits in the cost cell rather than in the Owned cell because
## `inputs → output` is what an arrow means: alone in a cell, with nothing on its left, `→ 1 hurdles`
## named a relation whose other side was missing. The break is DELIBERATE and not a wrap — the cell
## stacks the inputs' flow and this label (see `CraftingPanel._build_cost_cell`), because the shipped
## two-input recipe overruns `COLUMN_COST_WIDTH` and a wrapped flow would strand the arrow at the head
## of line two. Rendered on a recipe whose output is a MATERIAL and on no other — a kit recipe's output
## is the row's own name, and restating it would be the arrow pointing at the title.
const COST_YIELD_FORMAT := "→ %s %s"
## A cost amount reads whole where it is whole — a recipe asking for 12 fibre should not say 12.0.
const COST_AMOUNT_DECIMALS := 1
const COST_WHOLE_EPSILON := 0.05
