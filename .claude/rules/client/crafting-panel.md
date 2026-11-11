---
paths:
  - "clients/godot_thin_client/src/scripts/ui/hud/{CraftingPanel,CraftingPanelController,hud_crafting_vocab}.gd"
  - "clients/godot_thin_client/tools/ui_preview/chapters/crafting_bench.gd"
---

# Materials & Crafting — the panel that renders the sim's answer

`docs/plan_crafting_and_materials.md` §7. Its own free-floating surface, launched from the Band/City
panel header, showing one band's raw materials, what its kit costs to rebuild, and **why** a given
kit cannot be built right now.

## THE SIM RESOLVES THE REFUSAL; THIS PANEL RENDERS IT

Every refusal (`CraftOffer.reason` + `severity`), every shortfall number and every grade is resolved
sim-side and reproduced **verbatim**
(`.claude/rules/core_sim/crafting.md` → "On the wire"). It is the same rule `kitTiers` exists to
enforce, and it is not a style preference: the derivation needs the band's batch readings, the tool
that bounds a material, the recipe's grade seams and the item's wear quantum, and a client cannot
join those correctly from what rides the wire.

**`reason` + `severity` are the contract, not `available`.** *"Not needed yet"* is a shrug and
*"Short 4.9 bone"* is a problem; they are different strings with different severities precisely so
they can read differently, and a client deriving both from a boolean cannot tell them apart. The
panel therefore renders the reason as it arrives, tinted by the published severity, and composes no
sentence of its own — least of all *"cannot craft"*.

### The bench is several orders at once, each with its own crew (#873)

`docs/plan_labor_at_scale.md` → "The bench: several orders at once, each with a crafter limit". The sim
staffs every order each turn from the band's idle hands, highest priority first, so the panel **never
proposes a crew**: there is no `bench_crew` verb, no `crew_changed` signal, no Crafters stepper and no
`benchable_workers` ceiling. The bench well is one **header line** — `R of M orders running · C crafters ·
S spare` — and every order is its own row under it.

- **R** = orders with `crew_at_tool + crew_by_hand > 0`; **M** = every order; **C** = `bench.workers`, the
  sim's crew total; **S** = `HudBandLaborState.people_idle`, the idle figure the labor UI already tells
  the player (handed in as `PAYLOAD_IDLE_WORKERS`). None is derived by a rule of the panel's own.
- An empty queue replaces the line with `Nothing on the bench` and its sub line (`Auto` has its own).
- Bench-level wire keys are only `workers`, `orders`, `auto`, `auto_skipped`, `auto_wait_reason`, `lent_to_hunt`,
  `kit_orders` and `set_aside`. **Everything else is per order** — a bench-level read of `recipe_id`,
  `progress`, `worked`, `blocked_reason` is a read of a field that no longer exists
  (`KnowledgePanelController._bench_recipes` was one and now walks the orders).

### An order row: name line, meters, one sub line

`_build_order_row`. **Name line:** the sim's `display_name` (the recipe book's name only as a fallback),
the AUTO tag, the order's `Priority: <High|Normal|Low>` link, `made/count` inside its own `− n +`, `↑`
(never on index 0) and `✕`. **Meters:** the crew bar beside a thin progress bar. **Sub line, one line:** the
copy limit is two lines a row besides the bars, so anything longer rides a tooltip.

- **Crew bar = the work strip's `CrewSplitMarks`, generalised, not forked.** At the tool (`crew_at_tool`,
  the kitted green), by hand (`crew_by_hand`, the bare green) and **room** (`crew_cap − both`, floored at
  zero) as hollow outlined seats — `CrewSplitMarks` gained a fifth `room` segment that widens the run of
  marks but is never counted by `crew()`. The hover is `HudWorkVocab.crew_bar_words`, which gained a
  `room` clause (`room for 1 more`); the tool's name is the recipe book's name for the recipe that makes
  `bench_tool`. A recipe with no bench tool has no at-tool segment.
- **Progress bar** fills `progress / (work × max(items_in_flight, 1))` — every item in flight's work, never
  one item's. Work, `rate_per_turn` and what it teaches ride its **tooltip**, not its face.
- **Sub line:** unblocked it reads `<output_grade> · done in ≈T turns · <n> set aside`, `T =
  ceil((work_total − progress) / rate_per_turn)` floored at one turn and **omitted at a rate of zero**;
  `n` is Σ `drawn_inputs`. **Blocked, the sim's `blocked_reason` stands in its place VERBATIM, tinted by
  `blocked_severity`** through the one `REASON_COLORS` table (`danger` = the alarm, `neutral` = quiet): a
  "done in 3 turns" beside `Short 0.6 fibre` would promise progress that is not happening, and `No spare
  hands this turn` is the ordinary state one click after Make, not a fault. `lent_to_hunt > 0` appends
  `· n lent to the hunt`. The forecast of what the stock cannot cover (`short_to_finish`) rides the sub
  line's tooltip under the line itself.
- **Priority is per order**: `bench_priority <f> <b> order <i> high|normal|low`, `<i>` the order's index in
  `bench.orders` (the index `bench_order_count` / `bench_remove` / `bench_raise` address). It is a SIBLING
  verb of `work_priority`, whose grammar reads a lone trailing token as a herd id. The picker is the same
  `HudWidgets.build_work_priority_picker` the work inspector uses; `_priority_order` (view state, one open
  at a time, closed on a pick and on an index that left the queue) says which row shows it. No optimistic
  overlay: the order's `priority` is captured live and arrives on the command's own recapture.
- **Skip lives on a waiting AUTO head only when what stops it is a shortage** (`danger`): the server's
  skip check ignores the crew prompt, so a link on `No spare hands` would be refused.
- A kit order's parts are ONE box drawn where its first part sits; a part's status word is WORKING when the
  sim staffed it, WAITING on a `danger` refusal, else QUEUED.

### The rail's `N set aside for the bench` line

When `bench.set_aside` is non-empty the rail ends with ONE line, `2.2 set aside for the bench` (Σ amounts),
its per-material split (`fibre 2 · hide 0.2`) in the tooltip. It sits with the existing rail rows, not in a
section of its own.

## The readout rules the design is built on

- **THE LEDGER CARRIES NO CONDITION COLUMN — its four are Item · Owned · Rebuild costs · action.** How
  worn a thing is has ONE home: the Band panel's WORKFORCE role cards, which state the condition of
  the item behind each kit beside the role that kit sets, off `kit_item_conditions`. That is where a
  player asks *"how worn is my gear"*; this panel answers *"what does it cost to rebuild"*, and a
  second copy of condition here is one fact in two places free to disagree. What the ledger keeps of
  the item is the **Owned cell** — what the band has, and how good it is.
- **OWNERSHIP IS `count`, NEVER `remaining == 0`.** A batch that runs out of units is REMOVED, so a
  worn-out item and one the band never made both read `remaining 0` — which is why the cell is keyed
  off `count` and says what owning none MEANS, in ONE wording for every row (*Not made*), rather
  than re-deriving a step-down. **It is a statement of ownership, and that is all this surface owes:**
  worn-out and never-made are a distinction about WEAR, so they are told apart where wear is reported.
- **OWNING UNITS IS ONE LINE PER GRADE, BEST FIRST, `×3` AND A CHIP.** Counts are summed across the
  batches sharing a grade — two `good` batches at different wear are one line of `×5`, wear not being
  this panel's fact — and two grades of one item get a line each, because `×5 · excellent` would be a
  lie and every rule for collapsing to one misleads: the best flatters, the worst alarms, and the
  batch actually in service is chosen by **wear, not quality**, so it would move for a reason
  unrelated to what the row claims.
- **A STOCK ROW REPORTS THE BAND'S TOTAL OF THE MATERIAL IT MAKES — ONE number, summed across its
  batches.** This cell stated the recipe's YIELD (`→ 1 hurdles`) on the premise that *a stock recipe
  owns nothing*, and the premise was simply false: a crafted material is banked in `material_batches`
  like a gathered one, and the same panel was drawing the band's 5.0 hurdles in the rail 250px to the
  left while this cell — under a column head reading **Owned** — reported a number that was not an
  amount. It was reported from play as *"the count isn't working"*, which is the reading the column
  head invites.
- **⛔ THE PER-BATCH SPLIT AND ITS RATING CHIPS ARE THE RAIL'S FACT, SO `_build_batch_row` IS
  DELIBERATELY NOT REUSED HERE.** It was, on the reasoning that one pile deserves one builder — and
  what that imported was a 250px column's layout into a **172px** cell. At the shipped axis names
  (`hurdles` is rated `stoutness` / `span`, `materials.json`) the chip pair wraps, and two batches of
  wrapped chips cost the ledger **29px**, measured as the difference that pushed
  `crafting_panel_band_dock_collapsed`'s ledger — which clears its room by 13px — into an internal
  scroll. The chips were paying that in DUPLICATE: the rail draws the same batches at the same
  ratings, in the same panel, always visible. **And without the chips the split says nothing** —
  equipment splits by GRADE because two grades are different objects and the grade is rendered
  nowhere else, whereas two bare amounts with their ratings stripped are two indistinguishable
  numbers. What is left is the question the column head asks: how much have we got.
- **A MATERIAL IS MEASURED, NOT COUNTED — no `×` on a stock row.** Equipment is `×26` sleds; a pile is
  `22.8` hide and `9.3` cordage, at the rail's own one decimal. A cell reaching for the equipment
  column's `×n` renders `×9` for 9.3, which is the shape the harness fixture's un-whole amounts exist
  to catch. It is SIZED like that count all the same (`OWNED_COUNT_FONT_SIZE`, `HudStyle.INK`): the
  two answer the same question about different kinds of thing. A material the band banks NONE of says
  **`Not made`, the ledger's one none-wording** — the same sentence the kit and tool rows answer this
  column with. It had a third of its own, `None on hand`, for the identical fact under the identical
  head, and three spellings of *we have none* read down the column as three different states.
- **THE YIELD RIDES WITH THE COST, ON ITS OWN LINE — `4 wood · 0.5 hide` over `→ 1 hurdles`.**
  `inputs → output` is what an arrow means; alone in the Owned cell it named a relation with nothing
  on its left, which is why it read as a badge nobody could parse. Rendered on a MATERIAL output and
  on no other — a kit recipe's output is the row's own name, and an arrow back at the title says
  nothing. **The break is DELIBERATE and not a wrap**: the shipped two-input recipe measures ~157px
  at `COST_FONT_SIZE` against a 140px `COLUMN_COST_WIDTH`, so appending the yield to the inputs' flow
  stranded the arrow at the head of a wrapped line. The cell is a VBox of the flow and the yield
  instead — a width that fixed it breaks again at three inputs, and widening the cost column would
  take from the Item column on EVERY row to pay for a fact only stock rows carry.
- **THE GRADE CHIP'S TINT IS THE BAND'S POSITION IN THE PUBLISHED LEGEND, never a match on its name.**
  `characteristic_bands` rides the wire ascending and the material rail already takes its
  strength/weakness reading off its ENDS; the chip resolves by INDEX — last rung `HEALTHY`, the rung
  below it `SIGNAL`, the first `INK_FAINT`, anything between `INK_DIM`. A `{"excellent": HEALTHY}`
  table would be a client-side copy of a vocabulary the sim owns and would render every chip neutral
  the day a band is renamed or a fifth added. **A grade the legend does not contain draws no chip** —
  a start-stocked unit publishes `""`, and a spawn's kit was never on a bench and makes no quality
  claim.
- **SORTED BY URGENCY — worn first, untouched last, DIMMED rather than hidden.** The player's real
  question is *"what am I about to lose?"*, so the ledger opens on the answer; a kit you own and
  never use is information too. The key is the published `life_severity` rank, then `remaining`
  ascending, so a spent row leads its severity band. **Condition therefore decides the ORDER while
  the table prints none of it** — a ranking is a different use from a readout and restates nothing.
  It ranks on the item's FIRST batch, in wire order: the sort and the shrug test are readings of
  wear, which is the axis this table does not report, while the Owned cell reads every batch.

## ONE SHORT ROW PER THING MADE; ITS RECIPES LIVE BEHIND A LINK

**The ledger groups the published `CraftOffer`s by what they make — the sim's own row key.** An
equipment recipe keys by its `outputItemId`, so Spears is ONE row whether it is pointed with bone or
knapped from stone. A material recipe (empty `outputItemId`) keys by the material its first material
output names, resolved through the recipe book (`RecipeDefState.outputs[].materialId`) because the
offer carries no material id — so two recipes making one material are one row, with the same link,
popup and picker an item gets. Rows come out in the order each thing's first offer appears on the
wire, and the row's name is `displayName`, which the sim publishes as the thing's name on every one
of its offers. **The row is deliberately no richer than it was** — three richer layouts (recipe chips, every
recipe on its own line, fold-out lines) were rejected as overloading the player before this one was
approved. Do not add a column, a chip or a second line to the row beyond what is below.

**THE ROW DESCRIBES ITS SUGGESTED OFFER.** Exactly one offer per row carries `suggested`, and the SIM
picks it — the offer already on the bench, else the recipe this band last started for the thing if it
can still be made, else the first that can, else the last started, else the first in book order. The Costs cell and the refusal line
under Make are that offer's. The Owned cell is the one part read off every offer's
item instead: it counts every batch of the item, whatever recipe made it.

**The sections are the three groups — `Gear` · `Bench tools` · `Materials` — one head each.** The gear
group does not split by tier: a Spears row carrying a `plain` bone recipe and a `flint` knapped one
has no single tier to be sorted under. Everything else about the heads is unchanged:

- **All three are built by ONE head builder** so they read as one family, and a head is a name and a
  caret — a clause after the name reads as part of what is being folded away.
- **The whole head is the click target** — a `Button` stripped of its chrome, not a Label with a
  glyph beside it, because a head that responded only on its caret is a head you have to aim at. A
  folded head keeps its name, swaps `▾` for `▸` and DIMS, which is the difference between folding a
  group away and losing it.
- **The fold state is a plain member keyed by head NAME, and it does not breach
  "`render(payload)` is its whole input"** — it is VIEW state with exactly the standing of the scroll
  offset the panel already carries across a rebuild.

**NO TIER WORD REACHES THE OWNED CELL AT ALL.** The cell states count and grade and nothing else —
no `tier_id`: a row that must stay short has no line for it, and it would put the internal tier word
`plain` in front of the player (the schema has no `ownedNote`, and the sim publishes none).
**Which tier the band holds is answered in one place — the recipe popup's per-recipe Owned
column**, on an item whose recipes make different tiers. **A recipe's own tier is named by its
`recipeLabel`** — `Bone`, `Flint`, `Withy` — in the popup and the picker, and nowhere on the row.

### The `N recipes` link, and the popup it opens

**An item with more than one offer carries a `2 recipes` link under its name**, flat and underlined
in the signal ink; a single-recipe row carries none. **The link REPLACES the role line rather than
stacking under it**, which keeps every row two lines tall: a kit row's role line names the craft that
makes it, and an item with several recipes has several crafts, so the row cannot name one honestly. A
single-recipe row keeps its role line exactly as before.

**The link opens a small read-only `PopupPanel` anchored just under it**, over the ledger:
`Recipe · Costs · Makes · Lasts`, plus `Owned`. Every cell is the sim's: `recipeLabel`, the offer's
inputs with a short material in danger ink (the Costs cell's own builder), the grade chip plus
`makes` (the Owned cell's own chip), `lasts`, and `ownedAtTier` as `×n` or `—` for zero.

- **THE OWNED COLUMN APPEARS ONLY WHEN SOME OFFER IN THE ROW HAS `ownedAtTier >= 0`.** The sim
  publishes `-1` (`OWNED_AT_TIER_UNATTRIBUTED`) on every offer of an item whose recipes all make the
  SAME tier — a reed basket and a withy basket are one item at one tier, and nothing recorded which
  recipe made a unit. The column is then OMITTED rather than filled with dashes or with the item
  total: the row's own Owned cell already states the total, and restating it per recipe line would
  make two identical baskets look as if they came from different recipes. `-1` is not "none"; `0` is
  a real count and reads `—`.
- **A cost cell there is `RECIPE_COST_WIDTH` wide, never the ledger's 140.** In the popup's grid an
  unbounded flow shrinks to one clause per line, and in a comparison of recipes a wrapped cost breaks
  the line-per-recipe reading. The shipped three-input knapped spear fits on one line.
- **ONE popup at a time**, and it closes three ways: a click outside it and `Esc` (the Window's own
  behaviour, converging on the panel's teardown through `popup_hide`), and a second click of its link.
  **The link fires on PRESS**, and the frame and row the popup last closed on are remembered, because
  a press outside a popup closes it before the GUI pass sees the press — so a press landing on the
  popup's own link would otherwise find it already closed and open it again.
- **It is a Window, so it cannot change the ledger's height**, and it is ONE persistent node emptied
  on close — a closed popup says nothing a search of the panel can find.

### Make on several recipes opens a PICKER under the row

**Make on a single-recipe row sends that recipe at once**, as it always did. **Make on a row with
several opens a panel INSIDE the ledger, directly under the row**, spanning its width, because the
recipe cannot be changed once the build starts:

- **`Make <item> with which recipe?`**, then one RADIO line per offer: its label, its cost, and either
  `<grade> · <makes> · lasts <lasts>` (each clause only when published) or, when the offer is not
  `queueable` (an unlearned craft), its `reason` in the published severity's ink with the radio
  DISABLED. A known recipe short of material is a LIVE radio — see "THE QUEUE GATE IS `queueable`".
- **Preselected**: the suggested offer if it is queueable, else the first queueable one.
- **The footer says so** — *The recipe can't be changed once the build starts.* — then **Cancel** and
  **Start**. Start emits `enqueue_requested` with the CHOSEN offer's `recipeId` and the count the
  picker was opened with (the same `bench_enqueue` a single-recipe Make sends) and closes the
  picker; Cancel closes it; Make pressed again on the same row closes it. **One picker at a time**,
  and opening one closes the popup, which would only cover it.
- **The same picker serves a SUGGESTION's Queue** on an item with several recipes — drawn under the
  suggestion row rather than the ledger row, its Start queuing the suggestion's WHOLE count
  (`_picker_count`) rather than Make's one. Both surfaces key the picker by the item id, so
  `_picker_in_suggestions` says which one asked.
- **Make is LIVE when ANY offer in the row is `queueable`**, not only when the suggested one is, so
  a row is never stranded behind a disabled button while one of its recipes could be queued. **It
  stays live on a row already queued or on the bench** — another press
  is another order, which is what a queue is for; it read a spent *On the bench* while the bench held
  one job, and that label is retired.
- **The radio is `HudStyle.apply_radio`**, a DRAWN ring — `sprites-widgets.md` → "The checkbox
  indicator" records why the stock art cannot be recoloured into one.

### The view state carried across the rebuild

`render(payload)` stays the panel's whole data input; the card carries VIEW bits across each
per-snapshot rebuild, and the recipes add five to `_folded` and `_priority_open`: **`_popup_row`**
(the row whose popup is open), **`_picker_row`** and **`_picker_choice`** (the row whose picker is
open, and the recipe chosen in it), **`_picker_count`** (what Start queues) and
**`_picker_in_suggestions`** (which list the picker hangs under). `_reconcile_recipe_view` checks them
against every payload before drawing: **a picker whose row has gone or dropped to one recipe closes,
and so does one opened from a suggestion the sim no longer publishes**; a suggestion's picker takes
the suggestion's CURRENT count; and **a chosen recipe that is no longer queueable falls back to the
default rule**, so Start never points at a build the sim would refuse. The popup is re-filled and
re-anchored under its rebuilt link once the refit settles. All of them are cleared on dismissal
(`_reset_picker`), the same as the rank picker — they are readings of one ledger, not preferences.

> The design's fuel-gauge rule is unchanged and still governs every surface that *does* show
> condition (`docs/plan_crafting_and_materials.md` §7): a spear at 34% is exactly as deadly as one at
> 100%, so condition reads as a discrete chip in **turns left** and never as a percentage. What moved
> is which surfaces it applies to, not the rule.

### The shrug's dimming is "nothing spent", not "comfortable"

`_is_shrug` requires a NEUTRAL offer severity **and** an item at full condition
(`remaining >= 100`). The first cut required neutral + a `healthy` life severity, and that dimmed a
sled at 42 turns left and a set of hurdles at 28 — real gear in active use, reading as though there
were nothing to think about. `healthy` covers most of an item's life; the shrug is the item that has
never been touched.

**It reaches "untouched" through the NUMBER, not by matching the wording.** `remaining` is published
on a 0–100 scale and every shipped tier's `starting_durability` is 100, so full condition is exactly
the sim's own `Untouched`. Matching the string would be parsing a resolved wording this panel may
render and must not read as data. A tier shipping a lower `starting_durability` simply never dims,
which is the conservative direction — nothing is hidden, one row merely reads at full strength.

**The dim falls on the row's information, never its controls.** `_dim_shrug` fades the item's name
and role line, the Owned and cost cells, and the reason line under the button; the Make button and
the `N recipes` link, and every node above either, stay at full alpha. That is why the item cell dims
LABEL BY LABEL rather than as a column: on a multi-recipe row the column holds the link. The first
cut modulated the whole row, which faded a LIVE Make into the look of a disabled one — a player read
an untouched Hoes row with a flint recipe open as "can't make it". The chapter asserts that every
`BaseButton` on a shrug row has a modulate product up the tree of `1.0` while the row's name is
below it, on the single-recipe Traps row and again (`crafting_panel_shrug_recipes`) with Traps given
a second recipe so the link is present.

## The ledger row is a JOIN, and neither half can answer alone

`CraftOffer.outputItemId` is the key, twice over: it groups an item's offers into ONE row (a material
row keys by its material instead — see "ONE SHORT ROW PER THING MADE"), and it joins that row onto the
batches. The **suggested offer** supplies the name, the group, the refusal and the shortfalls
(`outputTierName`, `outputTierRank` and `ownedNote` are not on the wire);
**`equipment_batches` grouped by `itemId`** supplies the grades and the counts — ALL of an item's
batches, not the first, since one item may be held at two grades — plus `life_severity` + `remaining`,
which the urgency sort ranks on and nothing renders; the **recipe book** supplies the rebuild cost.
That is why the table is built from all three rather than off any one array.

**FOUR published `equipmentBatches` fields are read by NO client surface** — `life`, `quanta_left`
and `quantum_noun` (the condition WORDING and its unit), and **`tier_id`**. The first three are still
correct and still on the wire; the role cards answer condition off `kit_item_conditions` instead. The
fourth is deliberate: the tier a recipe makes is named by its `recipeLabel` in the popup, and the tier
the band carries is the popup's per-recipe Owned column — no tier word reaches the row's cell at all —
so a cell reading `tier_id` would say `plain` on almost every row of the early game.

- **The cost cell prefers a shortfall's `required` over the recipe's input amount** where the offer
  publishes one: it is already net of the bench tool's material efficiency, and it is the number the
  refusal beside it was computed against. A short material is tinted so the eye finds it without
  reading the button.
- **The Owned cell when the band owns none reads `Not made`, whatever the row is** — kit, bench tool
  or crafted material. What the published `group` keys is the cell's INK, not its wording: a kit the
  band lacks is `DANGER` because people are working bare-handed this turn, a tool or a pile it never
  made is `INK_FAINT`. The wording is a statement of ownership rather than a re-derived step-down;
  `×0` is the same fact and the worse sentence.
- **The Item cell's second line is a join of published fields, never an authored table**: a TOOL
  names the material it bounds (`materials[].tool_item_id`), a STOCK recipe names its CRAFT and the
  characteristic its input is SPENT worst-first on (`inputs[].reads_axis`), and a KIT row names the
  craft that makes it, at the sim's own `display_name`. A join that finds nothing renders no second
  line.
- **THE STOCK LINE NAMES NO MATERIAL, AND LEADS WITH THE CRAFT LIKE EVERY OTHER ROW** —
  `Tanning · worst suppleness spent first`. It read `Reads hide suppleness` until it was reported as
  meaningless, and both halves of that were wrong: two bare nouns collide with no separator between
  them, and the MATERIAL is the one fact the row already states twice — the cost cell beside it says
  `4 wood · 0.5 hide`, and the sim's own refusal under the button says `Hide, no tanning frame`.
  Leading with the craft is also what keeps line two a CATEGORY on every row — the stock row was the
  only one whose line two was a sentence, which is part of why it read as an error.

### ⛔ THE AXIS ON THAT LINE IS A SPEND ORDER, AND IT WAS PUBLISHED AS A QUALITY

The line said `Tanning · quality from suppleness`, and the claim was FALSE on every row it could ever
render on. It was justified here as *"the AXIS, which is the whole of `there is no best hide`: the
same pelt makes a good sled (reads toughness) and poor hurdles (reads suppleness)"* — and **`poor
hurdles` is the dead claim**. A grade is a property of EQUIPMENT: it is stamped on an equipment batch
and read back off it, while a `MaterialBatch` carries its own `characteristics` and has nowhere to
put one. So `RecipeDef::grade_for` resolves no grade for a recipe whose outputs are all materials,
`CraftOffer.outputGrade` publishes `""`, and the refusal renders `Hide, no tanning frame` with no
dangling arrow. Every set of hurdles the bench makes carries the recipe's own `stoutness 0.6 /
span 0.6`, whatever hide went into it — reported from play as a row promising `good` and delivering
one reading forever.

**THE AXIS SURVIVED THE CLAIM BECAUSE IT STILL DOES SOMETHING REAL, and dropping it would cost the
player a fact he can learn nowhere else on this panel.** `reads` is OVERLOADED sim-side:
`systems/crafting.rs::spend_axis` falls through the same field to pick WHICH batches of the input are
consumed, worst-first on that axis (`.claude/rules/core_sim/crafting.md`). On a material-only recipe
only that half applies — the bench eats your least supple hide and leaves the supple one standing —
so the line states the half that is true.

**THE STOCK BRANCH IS ALREADY THE MATERIAL-ONLY GATE, and a second test would be a second statement
of it.** `ROLE_STOCK_FORMAT`'s one caller is `_role_line`'s `group == GROUP_STOCK` branch, and `stock`
is the wire's own word for *"this recipe makes a material"*: the sim derives it as
`output_equipment_id().is_none()` (`snapshot/crafting.rs::group_of`), which is the same predicate as
`outputs_only_materials()` and the same one that leaves `outputItemId` empty. **`outputGrade == ""` is
the wrong instrument** even though it correlates today — the schema gives it TWO cases, a
material-only recipe *and* a recipe that reads no characteristic at all, so it is broader than the
fact wanted and would silence the clause on an alloy that legitimately has one.

**THE HONEST WORDING IS WIDER, AND THE COLUMN WAS MEASURED BEFORE IT WAS CHOSEN.** At
`ITEM_ROLE_FONT_SIZE` the shipped worst case grows `Tanning · quality from suppleness` (180px) to
`Tanning · worst suppleness spent first` (202px), and the card is an `AutoSizingPanel` fitted to
content against the room — measured at 1074px wide in a 1920px room, with the ITEM column taking the
table's slack at 165px against the role label's own 157px minimum. So the clause is paid for by ~22px
of card, not by a clipped sentence. **The material is still absent from it**: the row names that twice
already, and an ADJECTIVE form (*"your least supple hide"*) would be the client mapping a published
axis noun to English, which is the rule `quantumNoun` states one field over.

## MAKE QUEUES AN ORDER, THE SIM STAFFS IT — and there is still no Crafter role card

`docs/plan_crafting_and_materials.md` §7 → "The queue". Pressing **Make** emits
`bench_enqueue <faction> <band> recipe <id> count 1` (`HudCraftingVocab.MAKE_ORDER_COUNT`); a suggestion's
Queue sends its whole count. **No `workers` token is ever sent** — on an enqueue or a kit enqueue — because
the sim staffs the order and a client-chosen crew would be a second answer. The queue gate is `queueable`
(next section); a Make always adds an order.

Each order row carries its own count stepper, `↑` and `✕`: `−` is dead at `made + 1` (a count at or below
`made` would finish the order; stopping it is the ✕), `↑` is absent on index 0, and the ✕'s hover says
whether a set-aside pile is lost (`ORDER_REMOVE_TOOLTIP_DRAWN` / `_UNDRAWN`). Every control keeps the
order's published index, which is the `order` the verbs address.

### THE QUEUE GATE IS `queueable`, NOT `available`

Anything the band KNOWS how to make may be queued; materials and tools no longer gate the queue. Each
craft offer publishes two booleans: **`queueable`** (every required craft is learned — the server's own
enqueue test) and **`available`** (a pass can run NOW). Every "may the player queue this" gate reads
`queueable`: the ledger's Make (`_row_queueable`), the suggestion's Queue, the Make picker's radios and
its default/validity choice (`_default_choice`, `_choice_is_available`). `available` is no longer read by
any panel gate. A queueable-but-short SUGGESTION therefore shows a LIVE Queue and no refusal under it (that line renders
only under a disabled button, i.e. an unlearned craft) and its muted `shortfalls` line tells the material
story; a ledger row keeps the suggested offer's `reason` under Make as before, live button or not, and an
order's `short_to_finish` covers the queue.

### A RAW MATERIAL THAT WILL STOP A CRAFT IS NAMED ON THE ORDER AND ON THE SUGGESTION

Issue #777. `blocked_reason` says an order is being SKIPPED now; it says nothing of an order that is
running but will run dry. Two published fields carry the forecast, both in the `shortfalls` row shape
(`material_id` / `required` / `held` / `short`):

- **`orders[].short_to_finish`** — the stock on hand will not cover every pass the order still has to
  draw (queue-order claims). **It is a FORECAST, not a block**: `blocked_reason` stays the only
  "skipped" field. A queue row with an EMPTY `blocked_reason` and a non-empty `short_to_finish` carries a
  second line in the slot the blocked reason uses — the two are mutually exclusive, so the row stays at
  two lines. Every order is a row now, and the row's copy limit is two lines besides the bars, so the
  forecast rides the TOOLTIP of the order's sub line, under the line itself.
- **`craft_suggestions[].shortfalls`** — the WHOLE suggested count against the stock the queue leaves.
  A muted line in the words column under the consequence (`SUGGESTION_SHORTFALL_META` = the item id).
  The refusal under a disabled Queue button is unchanged; this is the material story for the count.

**The words** (`hud_crafting_vocab.gd`): `Short 6.0 wood` on an order; `Short 12.0 wood for all 6` on a
suggestion (6 = the suggestion's count). Several materials join their `Short X m` parts with ` · ` (the
suggestion's `for all N` follows ONCE). No "send a crew" tail: the Work-tab remedy is the event dock's
link, not copy on every row. The number is the sim's `short` formatted to one decimal
(`SHORTFALL_DECIMALS`, so `6.0`, not `_amount_text`'s `6`), never summed or recomputed. **Both lines are
`INK_FAINT`, the same muted ink as the order's blocked reason** — colour belongs to the status word, and a
second tint for a second shortage story was rejected. A line wider than its column trims with an
ellipsis and carries the full text as its tooltip, so nothing is dropped.

**The delivered count has ONE home, the `made/count` face** of each order row; no client surface reads
`itemsCompleted`.

## SUGGESTIONS — WHAT TO MAKE NEXT, RANKED BY WHO IS GOING WITHOUT

`docs/plan_crafting_and_materials.md` §7 → "Suggestions". `PopulationCohortState.craftSuggestions`,
decoded as `craft_suggestions` (`dict/population.rs`), opens the MAIN column, above the bench —
the panel's first answer to *what should I make*. **Ranked sim-side** (workers going without the
item's tool, ties by item id) and rendered in that order; the panel neither re-sorts nor re-scores,
and it is **not capped** — every suggestion is a row inside the card's internal scroll.

**An empty list draws NOTHING, not even its head.** Nobody going without is the ordinary state, and a
permanent line reporting good news would be a row spent on every frame.

Each row is TWO lines and one button (`_build_suggestion_row`, keyed `SUGGESTION_META` = item id):

- **`<item> ×<count>`** — the name is the item's offers' `displayName`, the count the sim's WHOLE
  shortfall net of what is already queued (a suggestion the queue covers is simply not published).
- **ONE consequence line, in the shortage's own unit, in WARN** — the declared-build wording comes FIRST
  (`N builders, for <label>`, see "A declared build names itself on the suggestion"), then `+X work a turn` where the published
  `work_per_turn > 0` (gear that adds build or keeping work), else the people going without:
  `3 hunters without`. The noun is the work vocabulary's own crew noun where EVERY source shares a job
  that has one (`hunt` → `Hunters`, `forage` → `Harvesters`), else
  `workers` — a suggestion drawn from two jobs is about workers, and naming one would undercount the
  other. No noun is minted here: `scout`, `warrior`, `extract` and `roadwork` have no plural crew noun
  in the client, so they read `workers`.
- **The sources ride the row's hover**, one line each — who (crew noun or role-card name), where (on a
  fauna, at a tile, `· material`, `(keeping)` on a site claim), units missing, workers without, and
  `+X work a turn` where it applies. A list of them would take the row past two lines.
- **`Queue`** — **whether the item can be MADE is the OFFER's question, not the suggestion's**, so the
  row joins the item's ledger row (`_row_in`, keyed by item id): live when any of its recipes is
  `queueable`, short of material or not; a single-recipe item sends `bench_enqueue … count <the
  suggestion's count>`; an item with several opens the Make picker under the suggestion. **An item
  whose craft is unlearned keeps its row** with a dead button and the suggested offer's own `reason`
  under it in the FAINT ink — the suggestion still says who is going without, and the refusal says
  why queueing it cannot happen yet.

**Coming up (#778).** `craft_suggestions[]` also carries `upcoming_count`, `expires_in_turns` and
`lead_turns` (`dict/population.rs`; the wear forecast, `docs/plan_crafting_and_materials.md` →
"Coming up"). The panel does not re-sort: the sim ranks coming-up-only entries after every shortage.

- **Coming-up-only** (`sources` empty, `upcoming_count > 0`; `_is_coming_up_only`) rows sit under a
  small "Coming up" subhead, drawn only when one exists, before the first of them. Same row, same
  Queue offer join as a shortage row, tagged `SUGGESTION_COMING_UP_META` = item id. Line two is
  `wear out in ~N turns` (`expires_in_turns` rounded UP, `~1 turn` at one) in `INK_DIM`, never WARN:
  nobody is without it yet. The hover is one sentence, the bench needing about `lead_turns` turns,
  so start now.
- **Merged** (sources AND `upcoming_count > 0`) keeps its WARN shortage line and appends
  ` · +N wearing out`.
- `upcoming_count == 0` shows nothing forecast-related, whatever `expires_in_turns`/`lead_turns` say.
- Preview: `crafting_coming_up.png` (`_coming_up_state`, one merged, one coming-up-only).

### IDLE MEANS IDLE, and the header's `S spare` is that same figure

`PopulationCohortState.idleWorkers` is `working_age − assigned − bench.workers` — the sim's
`BandWorkforce::idle()`, the one seam every head-count reading resolves through, because a bench crew is
not a `LaborTarget`. `HudBandLaborState.people_idle` (over `effective_idle`) is what the WORKFORCE zone's
"n idle" sites and `FactionRollup` report, and it is what the bench header's `S spare` reads. `effective_idle`
stays a local computation: the `+` steppers gate on an OPTIMISTIC idle, and the bench crew has no such
overlay (the sim staffs it), so the published crew is the honest third term.

## It is the FREE-FLOATING case, hence `AutoSizingPanel`

`panel-framework.md`'s test: the card is measured against the viewport, not against a dock's
remaining height, so `PanelCard` + `DockScrollFit` is the wrong half of the pair and would misbehave
silently. Both axes are fitted explicitly, the node being a plain `Control` that no child minimum
ever reaches.

**THE VIEWPORT IS NOT THE ROOM ONCE ANYTHING IS DOCKED.** A docked panel reserves a strip of one
screen edge and the map and the HUD both live in the remainder (`panel-framework.md` →
"Reserved-edge docking"); a free-floating card measured against `get_visible_rect()` is measured
against a rectangle nothing else in the client is using, so it grows over the dock. The seam is
**`AutoSizingPanel.room_bounds`**, handed down `HudLayer` → `setup(host, band_labor, room_bounds)` →
the panel node. `available_room(margin)` and `fit_to_content`'s ceiling both come off that one rect,
so the placement and the height fit cannot disagree about how much room there is. It is opt-in and
`null` keeps the raw viewport, which is right for a card that IS a reserver: the Inspector reserves
its own edge and must be measured against the whole window.

**AND THE RESERVED EDGES ARE NOT THE WHOLE ANSWER EITHER** — the room handed in is the HUD's
**`FloatingRoom`**, not `LayoutRoot`. The event bar reserves nothing and is drawn over a band of its
edge, and a docked panel is drawn UNDER it by its container while this card, placed by arithmetic,
was drawn THROUGH it: reported in play as the panel's own `⚒ MATERIALS & CRAFTING` title rendered
underneath a top-docked bar. `FloatingRoom` is `LayoutRoot` pulled further off every such overlay,
so one rect still answers both questions (`panel-framework.md` → "An overlay is a second kind of
neighbour").

**A HORIZONTAL BAND DOCK IS THE SAME KIND OF NEIGHBOUR, AND IT IS THE PANEL THIS CARD IS LAUNCHED
FROM.** The HUD does not yield a horizontal dock's strip — the band card is drawn over the HUD's own
containers rather than pushing them aside — so `_reservations` names nothing there and `LayoutRoot`
runs the full height of the window. This card measured against that and was drawn straight through
the panel: reported in play as the ledger sliced mid-row through `Wayfinding gear`. The strip reaches
`FloatingRoom` through the OVERLAY registry instead, which is what `Main.push_hud_strip` exists to
keep paired (`panel-framework.md` → "The two registries are complements").

**AND THE ROOM MOVES UNDER AN OPEN CARD, FROM EITHER REGISTRY.** The bar toggles on `R`, flips edge
and changes row count; the panel docks, changes edge, collapses and is released. Both writers of
`FloatingRoom` therefore end in `Hud._refit_floating_cards` → `CraftingPanelController.refit_room()`,
and a room that only bit at opening time leaves the reported frame exactly as broken.

**Shrinking the card costs nothing but a shorter scroll viewport** — `fit_to_content` turns the
internal scroll on exactly when the content did not fit the room it was given, so a ledger that
outgrows a short room scrolls rather than the card growing out of the room.

**THE CARD IS THE MIN OF ITS ROOM AND ITS CONTENT, WHICH IS WHAT "IT USES THE ROOM" MEANS HERE.** An
overflowing ledger fills the room exactly and scrolls inside it; one that fits is its content's
height with room to spare. Neither reading is the claim on its own — *"it clears the panel"* passes
on a card shrunk to nothing — so the property worth stating is that **no room is left over while rows
are still hidden**, with the scroll's own liveness saying which of the two cases a frame is in.
**The WIDTH is deliberately not part of it**: the ledger's Owned, Rebuild-costs and action columns are
fixed and only the Item column expands, so a card widened toward a short-and-wide room would spend
every pixel on the one column that does not wrap and buy back no height at all.

**THE HEIGHT FIT'S CEILING IS THE ROOM'S OWN HEIGHT, AND THE CARD NEVER MOVES TO BE MEASURED.**
`AutoSizingPanel.centred_in_room` is how the base class is told which of its two placements this is:
the card is centred by `_place()` after the fit, so what it may spend is the whole room rather than
the room beneath wherever it happens to be sitting. The constraint behind that is permanent —
**fitting a centred card against the room BELOW it throws away everything above it**, measured as a
ledger with room for every row clamped to four of them by exactly the height its own centring had put
above it — and what satisfies it is arithmetic on the room rect rather than a position the card is
moved to.

**NOTHING `render` DOES TO THE CARD BEFORE ITS MEASURING FRAME IS INVISIBLE, WHICH IS WHY IT DOES
NOTHING.** `refit` awaits a whole process frame before it can measure (the content's height being a
function of the card's width), so anything set on the way in is DRAWN for that frame. A card parked at
the top of the room to be measured is therefore a card that visibly jumps there and back on every
snapshot, and a card snapped to its nominal width to be laid out is one that visibly narrows and
widens again — the two halves of *"the card shakes when I press Next Turn"*. The ceiling comes off the
room instead, and the nominal width is applied on the FIRST mount only (`has_fitted_width`), a card
that has already been fitted being at a perfectly good width to lay content out at. **Both are pinned
by the re-render state**, which reads the card's rect the instant `refresh_snapshot` returns — i.e.
mid-`render`, at that await, which is the only place such a jump exists.

**AND THE REBUILD CARRIES THE PLAYER'S PLACE IN THE LEDGER ACROSS ITSELF.** `render` reads the scroll
offset before it tears the table down and `refit` writes it back once the fit has settled — but only
into a ledger that still scrolls, a table that now fits its room having nowhere to be scrolled to.
**Measured, today's tear-down does not lose it on its own**: `clear_children` empties the header, the
rail and the main column, all of them GRANDchildren of the `ScrollContainer`, so the scroll's own
child never leaves the tree and no layout pass ever observes empty content. The carry-across is what
makes the guarantee a property of this panel rather than of that arrangement.

## Launching it: one entry on the panel's ACTION BAR, and it carries no subject

A `register_action(ACTION_CRAFTING, LAUNCH_GLYPH, LAUNCH_TOOLTIP)` on `BandCityPanel`'s action
registry — which holds every verb the panel offers and renders it beside the cycler on a horizontal
dock and on its own bar row under the subject on a vertical one, built either way with the same
`_make_icon_button` the collapse toggle and the `◀`/`▶` arrows use. **The registry is
subject-independent chrome**, so ONE button serves a band page and the faction page, and the band
zone's 300px budget is untouched. It is registered through the ordinary seam rather than
special-cased — this panel's launcher gets no branch of its own and re-homes with every other action
when the dock edge changes — and `crafting_requested` is a relay of `action_invoked(ACTION_CRAFTING)`.
See `band-city-panel.md` → "The action registry is ONE list with TWO mount points" for the opposite
scarce axes that decide the mount.

Which band it opens on is `BandPanelController`'s answer, not the panel's: from a band page that
band, from the faction page the last band loaded — which is sitting on the model already, because
`render_faction` deliberately never touches `_panel_band`. It cannot be empty either: `render_faction`
returns early on an empty roster and `refresh_snapshot` hides the whole panel at zero bands, so a
visible header always has a band behind it.

**The two controllers never hold each other.** `BandCityPanel.crafting_requested` →
`BandPanelController.crafting_requested(band)` → `HudLayer` → `CraftingPanelController.toggle_for`,
and the panel's own two command signals relay back out through `HudLayer` to `Main`. That mediation
is the coordinator pattern `hud-modules.md` requires; a direct edge between the two controllers is
the thing it exists to prevent.

## The subject is a BAND ENTITY, re-resolved every snapshot

Holding the dict would freeze the bench's progress and the ledger's life the moment a turn ticked;
holding the entity and looking it up in `player_bands()` keeps the panel live, exactly as
`BandPanelController._resolve_panel_band` keeps the dock live. A band that leaves the roster CLOSES
the panel rather than stranding it on a band that no longer exists — and **a world change closes it
too**, because a new world renumbers entities from the same low range and a panel left open would
silently re-resolve onto a different band's bench (`HudLayer.reset_world_state`).

**The catalogues live on the controller, not on a state model.** `hud-modules.md`'s test is whether
two or more clusters read a field; exactly one reads these. They are ingested as ONE call
(`update_crafting_catalogues`) for the reason the kit roster is: a recipe book without its materials
renders a rail with no craft tracks and costs in materials the panel cannot name. The dispatch is
gated on **`craft_knowledge`**, the one of the four that moves in play — a craft is LEARNED, the
other three are per-world constants — and the other three are passed as `null` rather than `[]` when
the frame does not carry them, so absence means unchanged instead of "the world has no materials".

## PROVENANCE IS DEFERRED, and when it lands it is a POPOVER

Where a batch came from and what it earns per turn — *Hunted — Mammoth, 8 turns ago · +0.42/turn* —
is a second question, and putting it on the rail row made the rail a wall of prose that buried the
numbers a recipe actually reads. It belongs in a `DisclosureController`-style popover off the
material row, the idiom this client already has for Food / Morale / Growth / Kit.

**It must be a popover rather than an inline expansion, and `band-readouts.md` records that as a
correctness rule rather than a style one**: expanding inline grew a label *after* its zone had picked
a height tier, and the `clip_contents` host silently sliced the rows beneath it. A Window cannot
change a zone's height.

## What the rail deliberately does not show

One group per material the band HOLDS, one row per batch, its amount and its characteristic **bands**.
No provenance, no per-turn rate, and **no catalogue of materials the world does not yet contain** —
"On hand" means on hand.

**THE BAND RATES THE AXIS, NOT THE MATERIAL.** `tough: excellent · supple: poor` is a mammoth hide —
excellent at being tough, which is right for a sled and wrong for cordage. It is not a claim that the
hide is good, which is what lets ordinary quality words coexist with there being no best hide. Which
chips read as a strength and which as a weakness comes from the ENDS of the published legend
(`characteristic_bands`, ascending), never from a cut point typed client-side — the sim owns those,
and a client with its own would disagree with the word beside them.

The group header carries the material's craft and its track: the meter is
`progress / completion_threshold`, both published, so the client draws no scale of its own, and the
craft's name is the sim's `display_name` because the client never maps a craft id to English.

## AUTO-CRAFT — the bench's Auto switch, the `AUTO` tag, Skip, and the dimmed suggestion (issue #779)

The sim keeps a band's queue filled from the Make next list while `bench.auto` is on: it fills only an
EMPTY queue, with the top suggestion for its whole count, as an order carrying `auto`. A short auto
order WAITS (the bench never walks down the list); Skip (only on a waiting auto head) passes its item
over until it can be drawn. Switching off clears the skips and touches neither orders nor crew. Wire:
`bench.auto`, `bench.auto_skipped` (item ids), `orders[].auto`; commands `bench_auto <f> <band>
on|off` and `bench_auto_skip <f> <band>`, relayed panel → `CraftingPanelController` → `HudLayer` →
`Main.format_bench_auto` / `format_bench_auto_skip` exactly as `bench_priority` is.

- **Why Auto queued nothing** is `bench.auto_wait_reason` (sim-resolved; `""` unless Auto is on, the bench
  has room and the top suggestion cannot be queued, e.g. `Spears: Short 0.4 bone`). On an IDLE bench it is
  the well's sub line, `Auto: <reason>.` (`AUTO_WAIT_FORMAT`), falling back to `Auto: nothing to make.`
  when empty. On a bench WITH orders it is a dim autowrapped note under the header well (meta
  `AUTO_WAIT_NOTE_META`), absent when the reason is empty. Auto queues whenever the bench has room; it
  needs no idle hand.
- **An order row's sub line** is one ellipsized line; the whole refusal rides its tooltip (a neutral
  `Every hand is needed first: 4 holding rows, 2 scouting` is longer than `No spare hands this turn`).
- **The switch** is a pill (30x15 track, 11px knob, both `AUTO_SWITCH_*`) over an `AUTO` caption in the
  Made/Crafters caption size. On, track/knob/caption take `HudStyle.WARN`; off, `INK_FAINT`. It is a
  trailing control of the bench header line, and renders on an IDLE bench too. A click asks for the OPPOSITE state; no optimistic overlay.
- **The `AUTO` tag** is a thin amber-bordered box after an order's name, on order rows (meta valued the
  order index).
- **Skip** shows only when `bench.auto`, `orders[0].auto` and that order's `blocked_severity` is `danger`; an
  amber underlined link directly under the order's blocked line.
- **A skipped suggestion** (item in `auto_skipped`) dims its name and consequence and swaps the red
  shortfall line for a faint `Auto skipped — back when <materials> is in`, the materials being the item's
  suggested offer's `shortfalls` for ONE item (or `Auto takes it back when the bench is free` when none
  are short). Its Queue stays live.
- **An idle bench with Auto on** reads `Auto: nothing to make.` in place of the Make-a-row prompt; the
  `No one at the bench` crew prompt is the sim's blocked line and is untouched.

### A declared build names itself on the suggestion

A suggestion's `sources` may carry `kind: "build"` lines (a declared build whose builders are short of
the item, with `job` and the target keys). When one is present and every other line is a build line, the
amber consequence reads `N builders, for <label>` (`SUGGESTION_BUILD_FORMAT`; `1 builder, for <label>` at
exactly one, `SUGGESTION_BUILD_ONE_FORMAT`), N the first line's `workers_without`
(`CraftingPanel._first_build_source`). **This wording takes precedence over `+X work a turn`**: a
builder's gear always publishes `work_per_turn > 0` (build kits are productivity), so checking work
first would make this wording unreachable and hide which build the gear is for. **The label is the
build's own name with no position** (`HudWorkVocab.build_site_subject(job, x, y, "")`, e.g.
`Tame (12, 4)`): a declared build is named by the site it is raised on, not by a place in a line. Any
non-build source (take, site, other pool) keeps its ordinary wording, being the more urgent fact.

### A bench tool row states what it buys; a bench-tool suggestion states why it pays

**The tool row's second line** is `Fibre crafts 4× faster · 15% less fibre · up to excellent`
(`CraftingPanel._tool_benefit`, copy in `HudCraftingVocab.ROLE_TOOL_*`), and its hover is one sentence
comparing to bare hands (`With a loom, fibre work goes 4× as fast as by hand, uses 15% less fibre, and
can reach excellent quality (bare hands top out at good).`). Every number is joined off the wire:

- **speed** = the tool's `craft_speed` over the material's `hand_working_rate`, rounded to 0.1, whole
  multiples print with no decimal (4×, 3.2×);
- **saving** = `1 − craft_material_efficiency` as a whole percent; the clause is absent when it rounds to 0;
- **quality** = the band name the tool's `craft_quality_ceiling` falls in, looked up in the published
  `characteristic_bands` legend (`_band_name_at`) - no band table of the panel's own; the bare-hand tail
  reads the material's `hand_working_quality_ceiling` the same way, and drops when the material is not
  hand-workable.

The tool's three craft stats are on no typed field: they ride `SubsistenceSection.equipmentConfigJson`
(`items[id].tiers[0].effects[{stat, equipped}]`, else the item's own effects). `Hud.update_equipment_config`
forwards the blob to `CraftingPanelController.set_equipment_config`, which parses it into the payload's
`equipment_config`. **Nothing is guessed**: with no config, no hand rate, or a missing stat the row falls
back to `Bench tool — fibre` (`ROLE_TOOL_FORMAT`).

**A bench-tool suggestion** carries `bench_turns_saved` / `bench_build_turns` and one source of kind
`bench` (`SOURCE_KIND_BENCH`) whose `material` is the one the tool bounds. Its reason line is
`Saves about 36 bench turns on hide work (takes 24 to make)` (whole turns, singular at one), shown on the
row in the quiet ink (nobody is short, so no amber) and as the row's hover. A bench source never reaches
`N workers without` or the per-source `missing, without` tooltip line (`_bench_source`, checked first in
`_suggestion_consequence` and `_suggestion_tooltip`).
Previewed by `crafting_bench.gd`'s `_bench_tool_state` (`crafting_bench_tool.png`).

## A WHOLE KIT IS ONE BENCH ORDER (issue #780)

`.claude/rules/core_sim/crafting.md` → kit orders is the sim half. The panel's half is three surfaces
over four wire terms — `kit_needs[]` and `materials_free[]` on the cohort, `bench.kit_orders[]` and
`orders[].kit_order` on the bench — and three verbs relayed like every bench verb (panel signal →
`CraftingPanelController` → `HudLayer` → `Main.format_bench_enqueue_kit` / `_remove_kit` / `_raise_kit`).

- **A KITS group leads the ledger, above Gear**, one row per `kit_needs` entry in config order, folded by
  the same head builder. The row reads the kit's items and job noun (`Spears · Sled — hunters`), what is
  equipped against what is claimed (shown `e of c` with an `equipped` chip, or a `None equipped` / `Not
  used` pill), one kit's rebuild cost at each item's `suggested` recipe (with `each` under it) and one
  reason under Make (`Not in use`, `<Item> N short`, or `All N <noun> equipped`; see "A kit row reads
  CLAIMS" below). **No kit row is ever dimmed** — a covered kit can still be ordered.
- **A KIT ORDER IS A COUNT OF KITS, never netted against need.** Make opens a PICKER under the row:
  `Make how many <Kit>s?` with a `− N +` stepper (default 1, never below 1) and the columns ITEM · OWNED ·
  MAKE · RECIPE. OWNED is `×<units>` in dim ink, the Gear cell's own reading (the sum of the item's
  equipment batch `count`s); MAKE is N for every item. The RECIPE cell lists only recipes whose
  `craftOffers` entry is `queueable` — a radio per recipe with its cost, the bare cost for a sole one.
  **The footer's shortage is exact arithmetic over published terms**, the estimate precedent: passes =
  `ceil(N / output_per_pass)` and `need[m] = Σ passes × amount` against `materials_free`, one decimal
  (`Short 2.2 hide for all 3`, danger), else `Queues <items × N> items as one kit order` (dim).
  **Need never caps the order or disables Start** (Ray: *"We shouldn't prevent a kit from being built.
  The number should be the number of new kits to build"*). The only disable is an item with no queueable
  recipe — its offer's reason sits in the RECIPE cell in its severity tint — because the sim refuses a
  kit missing an item's recipe. `Start` sends `bench_enqueue_kit … kit <id> kits <n> recipe <id> …`, one
  recipe per item. The picker's state (`_kit_picker`, `_kit_count`, `_kit_recipes`) is VIEW state,
  checked against every payload like `_picker_row`.
- **The queue draws a kit order as ONE box**, where its first part sits: a status word (WORKING when the sim staffed any part,
  WAITING when every part has a `danger` refusal, else QUEUED), `<Kit> ×N`
  (`bench.kit_orders[].kits`), a progress bar and `made of total` (`done + Σ made` over `done + Σ count`),
  the parts summed on a sub-line while folded, and the WHOLE kit's ↑ (absent at index 0) and ✕. Unfolded,
  each part has a `− made/count +` addressing it by its own order index, and no ↑ or ✕.
- **A kit row reads CLAIMS, not the headcount.** Each `kit_needs[].items[]` carries `claimed` (a float:
  units actually claimed on this kit by rows, pools, sites and declared builds) and `short` (an int: whole
  units missing on those claims, before the bench's queue is netted off - the figure Make Next starts
  from), beside `have` and `queued`. `kit_needs[].people` counts everyone on the kit's jobs against every
  kit those jobs could use, which read `6 hunters without traps` with no row using traps and `1 without
  hoes` beside a Sow that was four hoes short. Per item, need is `ceil(claimed) x workers_per_unit`
  (`CraftingPanel._kit_item_claim`; an item under `KIT_CLAIMED_MIN` is unclaimed). The row has three states:
  - **no item claimed** - the reason is a quiet `Not in use` (`INK_FAINT`, never red, no `N without`) and
    the Owned cell the faint `Not used` pill;
  - **some item `short > 0`** - `<Item> N short` per short item (the item's own display name as a label,
    since names are mixed singular and plural and there is no pluraliser), joined by ` · `, with `, Q coming`
    where `queued - finished > 0` (units the queue still owes) and then `, F finished, arrives next turn`
    where `finished > 0` (units made at the bench and parked to the top of next turn; `queued` includes them,
    so the ink rule still reads the TOTAL `queued`): `DANGER` while any short item is not covered by its queue, `INK_DIM` once every one is
    (`Spears 2 short · Sled 5 short, 1 coming` / `Baskets 1 short, 1 coming` / `Spears 1 short, 1 finished, arrives next turn`);
  - **claimed and nothing short** - `All N <noun> equipped` in `INK_FAINT`.

  The Owned cell is `X of Y equipped` for the weakest claimed item (largest `need - equipped`), Y being the
  claim rounded up; `None equipped` (`DANGER`) when something is claimed and nothing held. **Kit ORDERING is
  unchanged: the order count is the player's and is never capped by need.** Pinned by `crafting_bench`
  frames `crafting_kits_ledger` (partial queue, fully queued, all equipped and not in use at once) and
  `crafting_kit_settled_short` (8 crooks held, 9 claimed: `8 of 9`, `Crook 1 short`).
- **A Make Next entry can name a MATERIAL** (a declared build short of the goods it draws): its `item_id` is
  the bare material id (`hurdles`) while its ledger row is keyed `material:hurdles`.
  `CraftingPanel._row_in` falls back to the `material:` key after an item's own row, and
  `_row_key_of` keys a stock offer by the published `output_material_id` before the recipe-book lookup.
  Without the join the Queue button was dead (`ledger_row.is_empty()`), the name was the raw id and the
  reason read kit wording. A material entry is named by the offer's `display_name` ("Hurdles"), states goods
  (`2.1 more Hurdles, for the top job`, or the queue row's own label for a later build; the amount is the
  sources' `missing_units` summed), its hover lines read `<who>: N short` with no crew count, and Queue sends
  the material's recipe and the suggestion's whole count, as the Materials row's Make does. Pinned by
  `crafting_bench` frame `crafting_material_suggestion`.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/CraftingPanel.gd` | The surface (`AutoSizingPanel`): header + `Band:` picker/cycler, the 250px material rail, the SUGGESTIONS at the top of the main column (`_build_suggestions`, see "SUGGESTIONS"), the bench well (the header line) and one row per order (`_build_queue` / `_build_order_row`, see "An order row"), with the rail's set-aside line, and the four-column ledger (Item · Owned · Rebuild costs · action) in FOLDABLE groups — `Gear`, `Bench tools`, `Materials`, all three built by one head builder — ONE ROW PER THING MADE, the offers grouped by `outputItemId` or, for a material recipe, by the material it makes, and the row's cells read off the SUGGESTED offer, with the `N recipes` link, its read-only `PopupPanel` and the Make picker under the row (see "ONE SHORT ROW PER THING MADE"). Emits `closed` / `band_selected` / `cycle_requested` / `enqueue_requested(recipe_id, count)` / `order_count_changed(order, count)` / `order_remove_requested(order)` / `order_raise_requested(order)` / `bench_priority_requested(order, level)` and holds no snapshot state apart from the VIEW bits it carries across its per-snapshot rebuild (`_folded`, `_priority_order`, `_popup_row`, `_picker_row`, `_picker_choice`, `_picker_count`, `_picker_in_suggestions`) — `render(payload)` is its whole input. Builds its rows as a VBox of `HBoxContainer`s rather than a `GridContainer` because a GROUP HEAD spans the width and a grid cannot span. **The ACTION column is sized by the refusal under the button, not by the button** — a published `reason` is a whole clause, and it was the column's 132px that wrapped every one of them onto two lines and inflated every row |
| `ui/hud/CraftingPanelController.gd` | `RefCounted` controller: owns the panel node (parented into the HUD CanvasLayer — a `RefCounted` cannot `add_child`), holds the four per-world catalogues, resolves the subject band by ENTITY each snapshot, and turns the panel's signals into `bench_enqueue_requested` / `bench_order_count_requested` / `bench_remove_requested` / `bench_raise_requested` / `bench_priority_requested` — the bench verbs (priority naming an `order`), each naming the band by its durable `band_id` and the queue edits an `order` index. It also carries the **room** the card is bounded by (`HudLayer.floating_room`, handed in through `setup` and set on the panel as `room_bounds`) — handed down rather than looked up, a `RefCounted` reaching into its host's tree being the coupling this pattern exists to avoid — and exposes **`refit_room()`**, which `Hud.set_overlay_inset` calls when that room changes shape under an open card. `HudLayer` holds it as `_crafting`, relays all of those signals, and refreshes it from the same per-snapshot seam the Band/City dock uses |
| `ui/hud/hud_crafting_vocab.gd` | The vocabulary leaf (`HudCraftingVocab`) — ALL-`const`, zero funcs, zero vars: the wire keys (including the queue's `orders` row keys and `ORDER_HEAD_INDEX`, and the `craft_suggestions` row and source keys), the chrome words (the queue's status words, steppers, ↑/✕ tooltips and metas valued by ORDER INDEX; the suggestions' head, title/consequence/source formats, the `workers` fallback noun and their metas; `MAKE_ORDER_COUNT`), the severity→tint table and the geometry measured off the prototype. **Nothing here is a refusal, a grade, a shortfall or a tier word** — those are the sim's — and there is no condition wording at all, that reading belonging to the role cards. It holds the ONE OWNERSHIP word (`Not made`, every group's answer to the `Owned` head — what the published `group` keys is the chip's INK, not its wording), the stock row's craft-led `worst <axis> spent first` line — which said `quality from <axis>` until a material-only recipe was found to stamp no grade at all, the axis surviving on it as the SPEND ORDER `reads` also decides (see “THE AXIS ON THAT LINE IS A SPEND ORDER”) — and the cost cell's `→ <n> <material>` yield clause, the four legend-INDEX tints the grade chip resolves through, the order rows' keys (priority, crew at tool / by hand / cap, items in flight, work, rate, grade, bench tool, drawn inputs, lent) and formats (header line, `done in ≈T turns`, `n set aside`, `Priority: …`, `n lent to the hunt`, the rail's set-aside line) with their index-valued metas, the ✕'s two tooltip shapes, the LEVELS themselves being `HudWorkVocab`'s and deliberately not restated — the `blocked_severity` key and the ONE `REASON_COLORS` table (plus its `REASON_COLOR_QUIET` fallback) that both the order rows and the ledger's offer rows tint through, so the two cannot hold different opinions of what a published severity looks like, the five recipe keys the ledger's grouping reads (`recipe_label` / `makes` / `lasts` / `suggested` / `owned_at_tier`, plus `OWNED_AT_TIER_UNATTRIBUTED`), the link / popup / picker words and their metas (`RECIPES_LINK_META`, `RECIPES_POPUP_COLUMN_META`, `PICKER_META` / `PICKER_OPTION_META` / `PICKER_START_META` / `PICKER_CANCEL_META`, `MAKE_BUTTON_META`), the three section heads (`GROUP_HEADS`, `GROUP_ORDER` = kit · tool · stock), and the caret pair + `GROUP_HEAD_META` / `OWNED_CELL_META` the harness reaches a head and a cell by. `BandCityPanel` reads its `LAUNCH_GLYPH` / `LAUNCH_TOOLTIP` back, so the registered launcher and the panel it opens cannot drift |
| `tools/ui_preview/chapters/crafting_bench.gd` | The harness chapter: the prototype's own band (every ledger state at once — its running bench publishes THREE orders — baskets High at the loom with two items in flight, spears Normal by hand, a sled blocked `No spare hands this turn` — plus a non-empty `set_aside`), a bare band with an idle bench (an empty queue), and the RESERVED-EDGE state. The band-dock states use an idle bench (`_dock_band`) because they stage whether the LEDGER fits a room. Its assertions are the claims no picture can carry — the refusal rendered verbatim with its number, the urgency order, the shrug dimmed **while a used row is not** (asserted as a pair, since a panel dimming every neutral row satisfies the first half alone), and the ownership/condition pair: the Owned cell states ownership **and** no condition wording reaches the ledger, over a fixture that deliberately keeps publishing `life` so the negative is not vacuous. **The STOCK row's readout is a pair straddling one save**: state 1 asserts the Cordage cell reports the band's whole pile as ONE total — the fixture's two batches summed, read back through the rail's `BATCH_AMOUNT_FORMAT` over deliberately un-whole amounts (6.5 + 2.8 = 9.3), asserted as the cell's ENTIRE contents so a cell that kept the rail's per-batch lines or its chips fails here — with the not-a-count claim named separately, since a cell reaching for the equipment column's `×n` renders `×9` and that is a different mistake from rendering the wrong pile, the yield closing the REBUILD-COSTS cell and no arrow left in the Owned one (both halves, since a panel that never moved the yield passes the first and one that dropped it passes the second). Its other half is **PNG-less and re-opens the SAME band with its store emptied**: the pile reads the ledger's one none-wording, `Not made`, and the cost cell still names the yield. **A SECOND PAIR RIDES THAT SAME SAVE — the material-only row's TWO SENTENCES**, and it is asked by EQUALITY throughout because the defect it guards was a sentence that was individually plausible and a `contains` would have passed on it. The stock row's line two must read the spend-order clause **and** the retired `quality from <axis>` one must be nowhere in the panel (the first alone passes on a panel printing both, the second alone on one that dropped line two entirely), with a looser negative on the bare word `quality` behind them, since a rewording that kept the claim in other words is the same lie. The other half is the sim's: the stock row's refusal must carry no `→ grade` tail **while the KIT row beside it still does** — the fixture pairs `Reed, no loom` with `Reed, no loom → fair`, the SAME bench material and the SAME missing loom, so the two differ in the tail and in nothing else and a build that stripped grade words everywhere fails there while every claim above stays green. A third names the TOOL row's `Bench tool — fibre`, the group the stock branch's edit must not have reached. **The fixture's own cordage reason was corrected in the same pass**: it read `Reed .70 → strong`, a payload no sim can resolve now that `grade_for` returns `None` for a material-only recipe. Sabotage-verified twice, disjointly — restoring the retired format fails exactly the three role-line claims (naming `Weaving · quality from strong` in the found text) while both equipment claims stay green, and dropping the axis clause altogether fails the spend-order EQUALITY **alone**, both negatives correctly staying green, which is the demonstration that a negative cannot stand in for the equality. The empty half is staged by emptying the STORE rather than by adding a second stock RECIPE, for two reasons — a second recipe makes the pairing a property of the fixture, and the extra ledger row puts `crafting_panel_band_dock_collapsed`, which stages the ledger FITTING its room, one row over the line it exists to sit under (measured: it fails that precondition). **`crafting_panel_two_tiers` + `crafting_panel_group_folded` are the OWNED readout**, on the shipped second tier: a band that can knap `flint` and still carries PLAIN clubs, so the band holds two tiers at once and every owned batch publishes a `tier_id` a cell could leak. Both spear recipes can be made there, which is also what the Make picker's Start claim needs. The fold is taken on the `Gear` head with `Loom` under `Bench tools` as the group that must stay. Five claims, every one a PAIRING, since a one-sided assertion passes on a panel that lost the thing entirely — two grades render a line each **while** a single-grade item renders exactly one; an Owned cell carries nothing beside its grades (phrased as *everything that is not a count or a legend word*, so a line COMPOSED client-side fails it); `Not made` reaching the cell at all, one wording for every group; folding hides ITS rows **while** another group's stay visible, the head remains, and the reverse toggle restores them; and no Owned CELL carries any tier word — scoped to the cells by `OWNED_CELL_META`, since the popup and the picker name tiers by design and a panel-wide scan cannot tell them apart, behind a precondition that the fixture publishes a `tier_id` on its batches, and asked of BOTH the two-tier and the reference band. Its positive half is the recipe popup's per-tier Owned column (`×6` / `—` on Spears). Sabotage-verified by rendering each batch's `tier_id` in the Owned cell beside its grade lines: exactly three claims fail — the tier-word negative on BOTH bands (two-tier `["plain", "flint", "flint", "flint"]`, reference `["plain" ×7]`) and the nothing-beside-its-grades claim (`["flint", "flint", "flint"]`) — while the popup's per-tier Owned column (`×6` / `—`) stays green. **`crafting_bench_priority` is the RANK's one frame, and it is one frame on purpose**: an IDLE bench (no job, so no ✕) carrying a HIGH mark on its line two with the picker OPEN and its `High` rung lit against two unlit ones — every new thing at once, a frame per state putting the defect in the gap between them. **A marked bench and an unmarked one cannot share a PNG**, this card rendering one band's bench, so the unmarked half is a CLAIM made on the reference band in the same block and PAIRED with the marked one: *prints a prefix* alone passes on a panel that always prints one, *prints nothing* alone on a panel that lost the mark entirely. The command is asserted on the LINE (`bench_priority 0 4971 low`) rather than on the payload dict — the work board's own idiom, so the verb and the token order are judged as the socket would see them — for **every** level, a commit hard-wired to `normal` satisfying any single-level claim; and each press asserts what did NOT go out, a mis-wired rung emitting `bench_remove` satisfying a bare *something was emitted*. The link and the rungs are driven as REAL POINTER INPUT through `InputProbe`, `pressed.emit()` being unable to see a covered, disabled, zero-size or IGNORE-filtered control. Sabotage-verified three ways, each failing a DISJOINT subset: a commit hard-wired to `normal` fails the `high` and `low` line claims alone (the `normal` one correctly staying green); hiding the control on an idle bench fails the link-to-press guard, the idle-bench render claim and the lit-rung claim while the MARK claims stay green; and never rendering the prefix fails the two marked claims and the LOW-ink one while the *a Normal bench prints nothing* negative correctly stays green. **`crafting_panel_reserved_edges` is the height bound**: `Main` is never instanced here, so the chapter pushes a left and a bottom reservation into `Hud.set_reserved_inset` by hand (the `event_dock` chapter's idiom), asserts the card's rect sits inside `layout_root`'s on both axes with its ledger scrolling internally, and releases them again. **The "it got shorter than state 1" claim is the vacuity guard** — every rect test passes trivially on a card that already fitted. **`crafting_panel_event_bar` is the OVERLAY bound**, and it is a different failure: it injects a real `EventDockPanel` docked TOP, connects `occupancy_changed` to `Hud.set_overlay_inset`, opens the card while the bar is SUPPRESSED, and only then un-suppresses it — so what is under test is the re-fit, not the opening. It keeps the bottom reservation, because a card centred in a tall room clears a top bar for free and the collision only exists once the ledger fills its room; that is why it was reported from play and not from here. Two vacuity guards carry it: the card's pre-bar top edge lay inside the bar's band, and the two rects share a horizontal one. Sabotage-verified by dropping the overlay term from the room's top edge — the clearance claim fails at `card top 12 vs bar bottom 66` while both guards stay green. **Three more states stand a REAL `BandCityPanel` up** rather than pushing a depth, since the reservation, the collapse and the HUD's yield verdict all have to be the panel's own answers: `crafting_panel_band_dock_bottom` (docked BOTTOM, fanned out through `Main.band_dock_overlays_hud` + `Main.push_hud_strip` so the harness restates neither), `crafting_panel_co_edge_bottom` (the bar on that SAME edge, displaced past it) and `crafting_panel_band_dock_collapsed` (railed under an OPEN card, so the room GROWS — the direction a re-fit that only ever shrank would pass). Each pairs a clearance claim with "no room is left over while rows are still hidden" and declares which of the two fit cases it is staging; the co-edge one additionally states the MAXIMUM's premise as the inequality it rests on. **A fourth, PNG-less, isolates the reserved half**: every move above changes both registries at once, so a bare reservation is pushed under a second id and the open card must shorten for it. Sabotage-verified three ways, each failing a DISJOINT subset — dropping `_refit_floating_cards` from `set_reserved_inset` fails only that isolated pair (`1030 → 1030 for a 360 strip`); dropping the overlay half of `push_hud_strip` puts the card 274px through the strip (`card bottom 1091 vs strip top 817`), which is the reported defect, while the co-edge frame correctly stays green because the bar's extent already covers the panel; and taking `_edge_offset` out of `occupied_extent()` fails the co-edge frame alone (`card bottom 805 vs bar top 751`). **A last PNG-less block is the RE-RENDER pair, and its two halves need opposite fixtures** — a ledger only scrolls when it did not fit, and a card that did not fit is already at the room's top edge, where a park moves it nowhere. So the RECT is asked of the bare band in an undocked room (card top 450 against a room top of 12, and 999px wide against a 960 nominal — both stated as preconditions, since a card filling its room or never wider than its nominal cannot be seen to jump), read the instant `refresh_snapshot` returns rather than after it settles, and the OFFSET is asked of the prototype's band in a room a reservation has shortened, ticked with `_ticked_crafting_band` because Next Turn changes the payload. Sabotage-verified: parking before the fit fails the rect claim alone at `450 → 12`, re-applying the nominal width unconditionally fails it alone at `999 → 960`, and `centred_in_room = false` fails three — the two "no room left over while rows are hidden" claims (the clamp the ceiling protects against) plus the filled card's rect, which the re-render RATCHETS (330 tall before it, 549 after — a card clamped by its own centring, then re-clamped from the higher top edge that left it). **The scroll carry-across is the one thing NO sabotage fails**: measured, the offset already survives the tear-down for the reason recorded above, so the assertion pins the property rather than the mechanism. **The BENCH block is two stopped fixtures and two PNG-less arithmetic ones**, because every claim about the finish estimate needs a partner: the crew-of-zero bench (rate `0`) beside the short-of-material one (a real rate, stopped anyway) is what says which half of the gate withheld the estimate; a remainder that only a CEILING rounds up beside a bench inside one turn of done, which is where the wording changes; the ✕ present on a job beside its absence on an idle bench; the drawn tooltip beside the undrawn one; and the press asserted to emit `bench_remove_requested` at order 0 **and not** `bench_enqueue_requested`, since a mis-wired button satisfies a bare "something was emitted". The fixtures carry the sim's own two new fields, and the running bench's numbers are chosen so a crew-derived rate renders a DIFFERENT sentence rather than the same one. **Those same two frames are the TINT pairing**, since a panel with one colour for every refusal passes either half alone: the crewless bench's reason renders in the quiet ink (asserted `== REASON_COLOR_QUIET` **and** `!= DANGER`) while the short bench's keeps the alarm. A **third, PNG-less** fixture is what separates the two implementations — a shortfall SENTENCE published at `neutral`, a payload no sim resolves, so a panel re-deriving the tint from the wording tints it `danger` and one reading the field does not; the picture is identical either way, which is why it has no frame. Sabotage-verified by restoring the always-`DANGER` tint: the crewless claim and the mismatch claim fail, the shortage claim and every other bench claim stay green. Its six other sabotages are recorded in `harness-ui-preview.md`, and so is the ONE-ROW-PER-THING-MADE block — `crafting_recipes_popup` and `crafting_make_picker` beside `crafting_panel`, the ledger at rest. **`crafting_queue_suggestions` + `crafting_suggestion_picker` are the QUEUE and the SUGGESTIONS**, appended last (`_queue_and_suggestion_states`): one band whose bench holds three orders — the head in the well at `1/3`, a PAUSED flint-spear order (drawn, not the head) and a QUEUED sled — and three suggestions in rank order: clubs short of bone (a LIVE Queue — the craft is known — with its muted `Short 12.0 wood for all 4` line; the dead-button case is staged separately with `queueable=false`), spears on two TAKE rows (`3 hunters without`, two recipes so Queue opens the picker under the suggestion), sled costing WORK (`+1.6 work a turn`). The claims: the head's `made/count` and stepper ride the WELL and no row repeats it; one row per order behind it at its PUBLISHED index; PAUSED vs QUEUED; `−` dead exactly at `made + 1`; no ↑ on the head; each ✕'s pile tooltip; the pausing-raise hover; published order; two lines per suggestion; the sources hover by equality; and every control PRESSED as real pointer input with the LINE asserted (`bench_order_count … order 0 count 4`, `bench_raise … order 2`, `bench_remove … order 1`, `bench_enqueue … recipe sled count 2`, Start → `recipe spears_flint count 3`, Make → `count 1`). **`crafting_queue_short_head` is the SHORT HEAD** (`_short_head_state`): baskets short of fibre at index 0, flint spears WORKED at index 1 (`worked: 1`, every bench scalar the spears order's, `on_bench` following it), a sled queued. The claims are pairs on the two places an index can go wrong — the well titles the worked order and shows its `1/3`; the skipped head is a queue row ABOVE the sled reading `WAITING` (the status word in DANGER) with its muted `Short … fibre` reason and no ↑; and the head ROW's `+` and ✕ send `order 0` while the WELL's send `order 1`, since a panel still hard-wired to the head would send 0 from both |

---

## An order lends hands to a hunt, in its sub line (hunting by need)

`BenchOrderState.lentToHunt` (`orders[i].lent_to_hunt`) is that order's share of the next-turn hunting muster;
`BenchState.lentToHunt` (`bench.lent_to_hunt`) is their sum and is what `HudBandLaborState` nets out of the
bench crew. `rate_per_turn` already subtracts the lent hands, so the estimate needs no change. The row's sub
line ends `· n lent to the hunt` (`ORDER_LENT_FORMAT`) and nothing at 0. Frame: `crafting_bench_lent_to_hunt`
(`chapters/crafting_bench.gd`, with a lending-nothing negative).
