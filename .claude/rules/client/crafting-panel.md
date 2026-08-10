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

### The bench well says how far along and why stopped as TWO lines

The well's word column is title, then the progress line — the `work` accrued against the pass's cost,
what a turn adds and the turns that implies, plus what the job has already delivered and the grade the
pile in flight fixed — and then, only
when `bench.blocked_reason` is non-empty, the refusal on **its own line beneath**, verbatim and in
`HudStyle.DANGER` while the progress line stays `INK_DIM`. **Neither fact may cost the panel the
other.** *How far along am I* and *why am I stopped* are both live questions on a stopped bench, and
the progress reading is what tells the player whether clearing the block recovers a nearly-finished
item or a barely-started one; a refusal written over it takes that away exactly when it is wanted.
A running bench adds no label at all, so nothing about its card moves for a line it does not have.

**The refusal line carries `BENCH_BLOCKED_META`**, which is the only way a claim can be scoped to it:
the reason is a sim string a harness must not predict, and the danger ink is worn by every refused row
in the ledger below.

### THE UNIT IS `work`, AND THE TURNS ARE STATED BECAUSE A WORKER-TURN IS NOT A WORKER'S TURN

The progress line reads **`3.0 of 6 work · +1.0/turn · done in 3 turns`**. It said `worker-turns`, and
that unit produced a real playtest error: a player with two crafters divided 6 by 2, expected three
turns and measured six. Bare-handed `craft_speed` is `0.5`, so two crafters deliver **1.0** a turn —
the name invited an arithmetic wrong by exactly the tool multiplier, so the unit is the recipe's own
`work` and what a turn adds is stated rather than left to be inferred.

- **The rate is `bench.rate_per_turn` rendered VERBATIM and is never re-derived.** It is
  `workers × progress_per_worker_turn × craft_speed` with `craft_speed` already resolved through the
  bounding tool or the material's bare-handed rate — the same tool-or-bare-hand join `kitTiers` exists
  to keep sim-side, and nothing else on this wire carries that factor.
- **The estimate is `ceil((work − progress) / rate_per_turn)`, computed client-side, and that is
  within the rule rather than an exception to it**: it is exact arithmetic over three published
  numbers, which is the boundary `yield-forecast.md` draws — where a closed form exists the sim ships
  the terms. A turns-remaining field beside the rate would be a second home for one fact. It floors at
  one turn and spells that one as **`done next turn`**, the plural format having produced
  *"done in 1 turns"*.

**BOTH CLAUSES ARE GATED ON `rate_per_turn > 0` *AND* AN EMPTY `blocked_reason`, WHICH ARE TWO
DIFFERENT QUESTIONS.** The rate is a property of **the crew and the tool**; whether the bench is
actually moving is `blocked_reason`'s answer, and merging them gets one of the two stopped benches
wrong. A bench **short of material** publishes its real, non-zero rate — the crew is standing there
and the tool is fine, it simply has not drawn — so a gate on the rate alone would quote *"done in 5
turns"* beside *"Short 0.6 fibre"* and promise progress that is not happening. A bench with **no crew
or a zero craft speed** publishes `0`, and there is nothing to compute at all. The harness stages both,
because a single stopped fixture cannot say which half of the gate withheld the estimate.

### THE ✕ ON THE WELL IS THE ONLY WAY OFF THE BENCH THAT IS NOT "MAKE SOMETHING ELSE"

`clear_bench <faction> <band>` was a complete, tested sim verb with no client surface emitting it, so
the only exit was pressing Make on another row — which silently spends the committed pile. The well
carries a ✕ beside the crew stepper, **absent entirely on an idle bench** (`recipe_id == ""`), and it
relays through the seam the other two bench verbs already use: panel → `CraftingPanelController` →
`HudLayer` → `Main`, no branch of its own.

**Its tooltip names what will be destroyed, off `drawn_inputs`** — *"Clear the bench — 5 fibre · 1 hide
already cut are spent"*, and the no-pile wording on an undrawn bench. **Composed from the WITHDRAWAL,
never from the recipe's inputs**: the two differ the moment a bench tool's material efficiency applies
(a tooled sled cuts 4.8 hide against a book price of 6), and naming what will really be lost is the
whole point. **No confirmation dialog** — the tooltip states the cost, the loss is small and
recoverable, and saying consequences in text rather than popping a modal is this panel's idiom
throughout.

**THE GLYPH COLLISION IS ANSWERED THREE WAYS AT ONCE, because the card header carries the same ✕ for
closing the whole panel.** This one sits INSIDE the well's own bordered box, it is inset from the
card's right edge by the crew stepper's width, and it wears `HudStyle`'s **`armed`** variant — the
destructive treatment the pause menu's Abandon and Quit already use — against the header's quiet
ghost. Rendered, the two read as a warm-bordered chip in a boxed readout and a dark-green corner
button; the harness reaches it by `CLEAR_BENCH_META` rather than by face, a face match finding both.

## The readout rules the design is built on

- **THE LEDGER CARRIES NO CONDITION COLUMN — its four are Item · Owned · Rebuild costs · action.** How
  worn a thing is has ONE home: the Band panel's WORKFORCE role cards, which state the condition of
  the item behind each kit beside the role that kit sets, off `kit_item_conditions`. That is where a
  player asks *"how worn is my gear"*; this panel answers *"what does it cost to rebuild"*, and a
  second copy of condition here is one fact in two places free to disagree. What the ledger keeps of
  the item is the **Owned cell** — what the band has, and how good it is.
- **OWNERSHIP IS `count`, NEVER `remaining == 0`.** A batch that runs out of units is REMOVED, so a
  worn-out item and one the band never made both read `remaining 0` — which is why the cell is keyed
  off `count` and says what owning none MEANS (*Bare hands* for a kit, *Not made* for a tool) rather
  than re-deriving a step-down. **It is a statement of ownership, and that is all this surface owes:**
  worn-out and never-made are a distinction about WEAR, so they are told apart where wear is reported.
- **OWNING UNITS IS ONE LINE PER GRADE, BEST FIRST, `×3` AND A CHIP.** Counts are summed across the
  batches sharing a grade — two `good` batches at different wear are one line of `×5`, wear not being
  this panel's fact — and two grades of one item get a line each, because `×5 · excellent` would be a
  lie and every rule for collapsing to one misleads: the best flatters, the worst alarms, and the
  batch actually in service is chosen by **wear, not quality**, so it would move for a reason
  unrelated to what the row claims. A STOCK recipe owns nothing and states what a pass yields instead
  (`→ 6 cordage`, from the recipe's outputs).
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

## TIER IS A FOLDABLE GROUP HEAD, AND IT IS THE HEAD THAT MAY DISAGREE WITH THE CELL

**The head is the tier a row would be MADE at; the cell is what the band HAS.** A column could only
ever spend its width saying `flint` on every row for the whole early game; a head says it once and can
**fold away**, which is what a column can never do. The two can disagree, and that disagreement is the
readout — a Clubs row under **Bronze** whose cell reads *carrying flint · poor*.

- **The `kit` group SPLITS by `outputTierName`**, one head per distinct tier, ordered by
  **`outputTierRank` DESCENDING** — newest first. The rank is the sim's, and it is the client's only
  honest ordering: alphabetical would put Iron above Bronze. On the shipped one-tier roster that is a
  single `Flint` head over every kit row; once minerals land it is `Bronze` above it. A recipe makes
  the best tier the faction knows, so a row MOVES between heads rather than splitting.
- **`tool` heads `Bench tools` and `stock` heads `Materials`**, after the tier heads, and all three are
  built by ONE head builder so they read as one family. Both carried a trailing explanation
  (`Bench tools — each stretches one material`) until the heads became foldable: a caret invites a
  click, and a clause after the name reads as part of what is being folded away. The head is a name
  and a caret.
- **The whole head is the click target** — a `Button` stripped of its chrome, not a Label with a
  glyph beside it, because a head that responded only on its caret is a head you have to aim at. A
  folded head keeps its name, swaps `▾` for `▸` and DIMS, which is the difference between folding a
  group away and losing it.
- **The fold state is a plain member keyed by head NAME, and it does not breach
  "`render(payload)` is its whole input"** — it is VIEW state with exactly the standing of the scroll
  offset the panel already carries across a rebuild. Keyed by name rather than by index so it survives
  a band switch, whose ledger may hold a different set of heads in a different order.

**`ownedNote` IS THE ONLY ROUTE BY WHICH A TIER WORD REACHES THE CELL.** It is the sim's resolved
*"what you carry is older than what you could now make"* line (`carrying flint · poor`, `last flint
set wore out`), published only when it is news, rendered VERBATIM on its own line under the grade
lines in the warn ink. Nothing composes one, nothing re-derives one, and the row's `tier_id` is
rendered nowhere — a cell showing it would be the retired column again, one field lower down.

> The design's fuel-gauge rule is unchanged and still governs every surface that *does* show
> condition (`docs/plan_crafting_and_materials.md` §7): a spear at 34% is exactly as deadly as one at
> 100%, so condition reads as a discrete chip in **turns left** and never as a percentage. What moved
> is which surfaces it applies to, not the rule.

### The shrug's dimming is "nothing spent", not "comfortable"

`_is_shrug` requires a NEUTRAL offer severity **and** an item at full condition
(`remaining >= 100`). The first cut required neutral + a `healthy` life severity, and that dimmed a
sled at 42 turns left and a husbandry kit at 28 — real gear in active use, reading as though there
were nothing to think about. `healthy` covers most of an item's life; the shrug is the item that has
never been touched.

**It reaches "untouched" through the NUMBER, not by matching the wording.** `remaining` is published
on a 0–100 scale and every shipped tier's `starting_durability` is 100, so full condition is exactly
the sim's own `Untouched`. Matching the string would be parsing a resolved wording this panel may
render and must not read as data. A tier shipping a lower `starting_durability` simply never dims,
which is the conservative direction — nothing is hidden, one row merely reads at full strength.

## The ledger row is a JOIN, and neither half can answer alone

`CraftOffer.outputItemId` is the key. The **offer** supplies the name, the group, the refusal, the
shortfalls, the group HEAD (`outputTierName` + `outputTierRank`) and the owned NOTE;
**`equipment_batches` grouped by `itemId`** supplies the grades and the counts — ALL of an item's
batches, not the first, since one item may be held at two grades — plus `life_severity` + `remaining`,
which the urgency sort ranks on and nothing renders; the **recipe book** supplies the rebuild cost.
That is why the table is built from all three rather than off any one array.

**FOUR published `equipmentBatches` fields are read by NO client surface** — `life`, `quanta_left`
and `quantum_noun` (the condition WORDING and its unit), and **`tier_id`**. The first three are still
correct and still on the wire; the role cards answer condition off `kit_item_conditions` instead. The
fourth is deliberate: the tier a row would be made at is the group HEAD, and the tier the band carries
reaches the cell only through the sim's resolved `ownedNote`, so a cell reading `tier_id` would say
`flint` on every row of the early game — the column the head replaced.

- **The cost cell prefers a shortfall's `required` over the recipe's input amount** where the offer
  publishes one: it is already net of the bench tool's material efficiency, and it is the number the
  refusal beside it was computed against. A short material is tinted so the eye finds it without
  reading the button.
- **The Owned cell when the band owns none is keyed off the published `group`** — `Bare hands` for a
  kit, `Not made` for a tool. What the panel says is what owning none MEANS for that group, which is a
  statement of ownership rather than a re-derived step-down; `×0` is the same fact and the worse
  sentence.
- **The Item cell's second line is a join of published fields, never an authored table**: a TOOL
  names the material it bounds (`materials[].tool_item_id`), a STOCK recipe names the characteristic
  its input is judged on (`inputs[].reads_axis`), and a KIT row names the craft that makes it, at the
  sim's own `display_name`. A join that finds nothing renders no second line.

## MAKE IS THE ASSIGNMENT — which is why there is no Crafter role card

Pressing **Make** emits `set_bench <faction> <band> recipe <id>` and the sim draws idle workers onto
the job; the running row's button reads *On the bench* and is spent; the `− n +` stepper emits
`bench_crew <faction> <band> workers <n>`; the well's ✕ emits `clear_bench <faction> <band>`. **One
job at a time**, so the panel never has to explain a queue — and `clear_bench` names the band alone
for the same reason, there being no job argument to disambiguate.

Scout and Warrior are standing roles with nothing to point at; crafting always has a subject — the
thing being made — so it is staffed at the bench like a worked source. That also sidesteps a measured
constraint: the Band panel's WORKFORCE zone reads 326px against a 275px box and its column split sits
*exactly* on `band_panel_preview`'s levelness floor (`band-city-panel.md` → "The band zone's tier
reads the whole STACKING BUDGET"), so a third role card there would have to be paid for by
re-authoring that split.

**`set_bench` sends the recipe and NOT a crew.** The grammar admits `[workers <n>]`, and omitting it
is the point: a client-chosen crew would be a second answer to a question the sim already answers.

### The stepper's ceiling is `idle + the crew at the bench`, which is NOT the band's idle count

**IDLE MEANS IDLE.** `PopulationCohortState.idleWorkers` is `working_age − assigned − bench.workers`
— the sim's `BandWorkforce::idle()`, the one seam every head-count reading resolves through,
including the clamp `assign_labor` makes — because a band's people are spent on the labor assignments
AND on the bench, and a bench crew is not a `LaborTarget`. `HudBandLaborState.effective_idle` makes
the same subtraction over its optimistic overlay, so the WORKFORCE zone's three "n idle" sites and
`FactionRollup`'s faction total report hands that are actually free.

**The stepper asks a different question and so takes a different number**
(`HudBandLaborState.benchable_workers` = `effective_idle + bench.workers`, the sim's
`BandWorkforce::benchable()`). Re-crewing a bench does not require freeing its crew first — those
hands stay put while the job is swapped — so a ceiling of `idle` alone would pin the stepper to the
crew already standing there and make `+` dead on every running bench.

> `effective_idle` STAYS a local computation rather than reading the published `idleWorkers`: the `+`
> steppers gate on an OPTIMISTIC idle, so a just-issued assign counts before the turn resolves, and
> that overlay (`pending_labor`) is exactly what the wire's answer cannot carry. The bench crew has no
> such overlay — a `bench_crew` edit shows on the next snapshot — so the published crew is the honest
> third term.

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
material row, the idiom this client already has for Food / Morale / Growth / Trade / Kit.

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

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/hud/CraftingPanel.gd` | The surface (`AutoSizingPanel`): header + `Band:` picker/cycler, the 250px material rail, the bench well and its crew stepper, and the four-column ledger (Item · Owned · Rebuild costs · action) in FOLDABLE groups — one head per kit TIER, rank-descending, then `Bench tools` and `Materials`, all three built by one head builder. Emits `closed` / `band_selected` / `cycle_requested` / `make_requested` / `crew_changed` / `clear_bench_requested` and holds no snapshot state — `render(payload)` is its whole input. Builds its rows as a VBox of `HBoxContainer`s rather than a `GridContainer` because a GROUP HEAD spans the width and a grid cannot span. **The ACTION column is sized by the refusal under the button, not by the button** — a published `reason` is a whole clause, and it was the column's 132px that wrapped every one of them onto two lines and inflated every row |
| `ui/hud/CraftingPanelController.gd` | `RefCounted` controller: owns the panel node (parented into the HUD CanvasLayer — a `RefCounted` cannot `add_child`), holds the four per-world catalogues, resolves the subject band by ENTITY each snapshot, and turns the panel's six signals into `set_bench_requested` / `bench_crew_requested` / `clear_bench_requested`. It also carries the **room** the card is bounded by (`HudLayer.floating_room`, handed in through `setup` and set on the panel as `room_bounds`) — handed down rather than looked up, a `RefCounted` reaching into its host's tree being the coupling this pattern exists to avoid — and exposes **`refit_room()`**, which `Hud.set_overlay_inset` calls when that room changes shape under an open card. `HudLayer` holds it as `_crafting`, relays all three signals, and refreshes it from the same per-snapshot seam the Band/City dock uses |
| `ui/hud/hud_crafting_vocab.gd` | The vocabulary leaf (`HudCraftingVocab`) — ALL-`const`, zero funcs, zero vars: the wire keys, the chrome words, the severity→tint table and the geometry measured off the prototype. **Nothing here is a refusal, a grade, a shortfall, a tier word or an owned note** — those are the sim's — and there is no condition wording at all, that reading belonging to the role cards. It holds the two OWNERSHIP words (`Bare hands` / `Not made`, keyed off the published `group`), the four legend-INDEX tints the grade chip resolves through, the bench's `work` unit + rate + finish-estimate formats (including the singular `done next turn`) and the ✕'s two tooltip shapes, and the caret pair + `GROUP_HEAD_META` / `OWNED_CELL_META` / `CLEAR_BENCH_META` the harness reaches a head, a cell and the clear button by — the last of those being the only route to a button whose face the card header also wears. `BandCityPanel` reads its `LAUNCH_GLYPH` / `LAUNCH_TOOLTIP` back, so the registered launcher and the panel it opens cannot drift |
| `tools/ui_preview/chapters/crafting_bench.gd` | The harness chapter: the prototype's own band (every ledger state at once), a bare band with an idle bench, the bench-bound band that separates idle from benchable, and the RESERVED-EDGE state. Its assertions are the claims no picture can carry — the refusal rendered verbatim with its number, the urgency order, the shrug dimmed **while a used row is not** (asserted as a pair, since a panel dimming every neutral row satisfies the first half alone), and the ownership/condition pair: the Owned cell states ownership **and** no condition wording reaches the ledger, over a fixture that deliberately keeps publishing `life` so the negative is not vacuous. **`crafting_panel_two_tiers` + `crafting_panel_group_folded` are the OWNED readout**, and they need a second tier because the shipped one-tier roster can never make a head disagree with a cell and therefore can never produce an `ownedNote` at all: a `Bronze` head over Clubs the band still carries in flint, above a `Flint` head over Traps. Five claims, every one a PAIRING, since a one-sided assertion passes on a panel that lost the thing entirely — two grades render a line each **while** a single-grade item renders exactly one; the note is verbatim on the row that has one **and** the row that has none carries nothing beside its grades (phrased as *everything that is not a count or a legend word*, because a `has()` on the first row is satisfied by a panel COMPOSING a note of its own); `Bare hands` on a kit **and** `Not made` on a tool; folding hides ITS rows **while** another group's stay visible, the head remains, and the reverse toggle restores them; and no tier word reaches an Owned CELL except through the note — scoped to the cells by `OWNED_CELL_META`, since the head is a tier word by design and a panel-wide scan cannot tell the two apart, over a fixture publishing a `tier_id` on every batch it owns. Sabotage-verified five ways, each failing a DISJOINT subset: collapsing the cell to one grade line fails the two-grade claim alone; dropping the note fails the verbatim claim alone; **composing the note client-side out of `tier_id` + `grade` — the forbidden implementation — fails the no-note row AND the tier-word negative, and nothing else**; printing one ownership wording everywhere fails that pair alone; a fold that never bites fails the hides-its-rows claim and the folded-caret claim while the other-group and reverse-toggle halves correctly stay green; and a fold that hides the WHOLE table fails only the other-group half, which is what that half is for. **`crafting_panel_reserved_edges` is the height bound**: `Main` is never instanced here, so the chapter pushes a left and a bottom reservation into `Hud.set_reserved_inset` by hand (the `event_dock` chapter's idiom), asserts the card's rect sits inside `layout_root`'s on both axes with its ledger scrolling internally, and releases them again. **The "it got shorter than state 1" claim is the vacuity guard** — every rect test passes trivially on a card that already fitted. **`crafting_panel_event_bar` is the OVERLAY bound**, and it is a different failure: it injects a real `EventDockPanel` docked TOP, connects `occupancy_changed` to `Hud.set_overlay_inset`, opens the card while the bar is SUPPRESSED, and only then un-suppresses it — so what is under test is the re-fit, not the opening. It keeps the bottom reservation, because a card centred in a tall room clears a top bar for free and the collision only exists once the ledger fills its room; that is why it was reported from play and not from here. Two vacuity guards carry it: the card's pre-bar top edge lay inside the bar's band, and the two rects share a horizontal one. Sabotage-verified by dropping the overlay term from the room's top edge — the clearance claim fails at `card top 12 vs bar bottom 66` while both guards stay green. **Three more states stand a REAL `BandCityPanel` up** rather than pushing a depth, since the reservation, the collapse and the HUD's yield verdict all have to be the panel's own answers: `crafting_panel_band_dock_bottom` (docked BOTTOM, fanned out through `Main.band_dock_overlays_hud` + `Main.push_hud_strip` so the harness restates neither), `crafting_panel_co_edge_bottom` (the bar on that SAME edge, displaced past it) and `crafting_panel_band_dock_collapsed` (railed under an OPEN card, so the room GROWS — the direction a re-fit that only ever shrank would pass). Each pairs a clearance claim with "no room is left over while rows are still hidden" and declares which of the two fit cases it is staging; the co-edge one additionally states the MAXIMUM's premise as the inequality it rests on. **A fourth, PNG-less, isolates the reserved half**: every move above changes both registries at once, so a bare reservation is pushed under a second id and the open card must shorten for it. Sabotage-verified three ways, each failing a DISJOINT subset — dropping `_refit_floating_cards` from `set_reserved_inset` fails only that isolated pair (`1030 → 1030 for a 360 strip`); dropping the overlay half of `push_hud_strip` puts the card 274px through the strip (`card bottom 1091 vs strip top 817`), which is the reported defect, while the co-edge frame correctly stays green because the bar's extent already covers the panel; and taking `_edge_offset` out of `occupied_extent()` fails the co-edge frame alone (`card bottom 805 vs bar top 751`). **A last PNG-less block is the RE-RENDER pair, and its two halves need opposite fixtures** — a ledger only scrolls when it did not fit, and a card that did not fit is already at the room's top edge, where a park moves it nowhere. So the RECT is asked of the bare band in an undocked room (card top 450 against a room top of 12, and 999px wide against a 960 nominal — both stated as preconditions, since a card filling its room or never wider than its nominal cannot be seen to jump), read the instant `refresh_snapshot` returns rather than after it settles, and the OFFSET is asked of the prototype's band in a room a reservation has shortened, ticked with `_ticked_crafting_band` because Next Turn changes the payload. Sabotage-verified: parking before the fit fails the rect claim alone at `450 → 12`, re-applying the nominal width unconditionally fails it alone at `999 → 960`, and `centred_in_room = false` fails three — the two "no room left over while rows are hidden" claims (the clamp the ceiling protects against) plus the filled card's rect, which the re-render RATCHETS (330 tall before it, 549 after — a card clamped by its own centring, then re-clamped from the higher top edge that left it). **The scroll carry-across is the one thing NO sabotage fails**: measured, the offset already survives the tear-down for the reason recorded above, so the assertion pins the property rather than the mechanism. **The BENCH block is two stopped fixtures and two PNG-less arithmetic ones**, because every claim about the finish estimate needs a partner: the crew-of-zero bench (rate `0`) beside the short-of-material one (a real rate, stopped anyway) is what says which half of the gate withheld the estimate; a remainder that only a CEILING rounds up beside a bench inside one turn of done, which is where the wording changes; the ✕ present on a job beside its absence on an idle bench; the drawn tooltip beside the undrawn one; and the press asserted to emit `clear_bench_requested` **and not** `set_bench_requested`, since a mis-wired button satisfies a bare "something was emitted". The fixtures carry the sim's own two new fields, and the running bench's numbers are chosen so a crew-derived rate renders a DIFFERENT sentence rather than the same one. Its six sabotages are recorded in `harness-ui-preview.md` |

---
