class_name HudCraftingVocab

## The MATERIALS & CRAFTING panel's vocabulary leaf (`docs/plan_crafting_and_materials.md` §7) — the
## wire keys it reads, the words it says, and the geometry the prototype measured. ALL-`const`, zero
## funcs, zero vars, exactly like `HudWorkVocab` and its siblings: a new label or threshold goes HERE
## rather than as a fresh `const` on the panel, which is the rule that keeps a const block from
## regrowing into a merge-conflict surface.
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
const BENCH_ITEMS_COMPLETED_KEY := "items_completed"
const BENCH_OUTPUT_GRADE_KEY := "output_grade"

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

## `MaterialShortfall` — the number a refusal names. `required` is already net of the bench tool's
## material efficiency, which is why the cost cell prefers it over the recipe's own input amount.
const SHORTFALL_MATERIAL_ID_KEY := "material_id"
const SHORTFALL_REQUIRED_KEY := "required"
const SHORTFALL_SHORT_KEY := "short"

## `PopulationCohortState.equipmentBatches`. **`count == 0` MEANS THE BAND OWNS NONE** and is the only
## honest ownership test: a batch that runs out of units is removed, so worn-out and never-made both
## read 0 here and are told apart by `life` alone.
const BAND_EQUIPMENT_BATCHES_KEY := "equipment_batches"
const EQUIPMENT_ITEM_ID_KEY := "item_id"
const EQUIPMENT_TIER_ID_KEY := "tier_id"
const EQUIPMENT_GRADE_KEY := "grade"
const EQUIPMENT_COUNT_KEY := "count"
## Condition left on the unit IN HAND, 0–100. The life BAR's fill, and nothing else — the row's words
## are `life`, in the item's own use quanta, and this number is never spoken as a percentage.
const EQUIPMENT_REMAINING_KEY := "remaining"
const EQUIPMENT_LIFE_KEY := "life"
const EQUIPMENT_LIFE_SEVERITY_KEY := "life_severity"

## The per-world catalogues, on the snapshot dict beside `kits`.
const MATERIAL_ID_KEY := "id"
const MATERIAL_CRAFT_KEY := "craft"
const MATERIAL_AXES_KEY := "axes"
const BAND_LEGEND_NAME_KEY := "name"
const RECIPE_ID_KEY := "id"
const RECIPE_DISPLAY_NAME_KEY := "display_name"
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

# ---- the three ledger groups --------------------------------------------------------------------
## `CraftOffer.group`, and the order the ledger's one table renders them in: the band's KIT first (it
## is what a party carries and what wears out), then the bench TOOLS, then the recipes that make
## STOCK rather than kit. The kit group takes no sub-head — it is the table's default group.
const GROUP_KIT := "kit"
const GROUP_TOOL := "tool"
const GROUP_STOCK := "stock"
const GROUP_ORDER: Array[String] = [GROUP_KIT, GROUP_TOOL, GROUP_STOCK]
const GROUP_HEADS := {
	GROUP_TOOL: "Bench tools — each stretches one material",
	GROUP_STOCK: "Materials — recipes that make stock, not kit",
}

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
## What the bench is making. The craft's name, then the thing.
const BENCH_TITLE_FORMAT := "%s %s"
## Its progress, in the units the sim keeps it in: worker-turns accrued against the pass's cost.
const BENCH_PROGRESS_FORMAT := "%.1f of %.0f worker-turns"
const BENCH_ITEMS_COMPLETED_FORMAT := "%d finished"
const BENCH_GRADE_FORMAT := "this pile → %s"
const BENCH_SUB_SEPARATOR := " · "
const BENCH_CREW_CAPTION := "Crafters"
const BENCH_CREW_DECREMENT := "−"
const BENCH_CREW_INCREMENT := "+"
const BENCH_TEACH_FORMAT := "Teaching %s — every one finished teaches it."
## Crafting is the FOURTH TEACHER, but a bench with no lesson to credit says nothing rather than
## saying "teaches nothing".
const BENCH_TEACH_NONE := ""

const LEDGER_COLUMN_ITEM := "Item"
const LEDGER_COLUMN_TIER := "Tier"
const LEDGER_COLUMN_LIFE := "Life left"
const LEDGER_COLUMN_COST := "Rebuild costs"
## The action column's head is deliberately blank — a column of buttons names itself.
const LEDGER_COLUMN_ACTION := ""

const MAKE_LABEL := "Make"
## The running row's button is SPENT — make IS the assignment, and one job at a time means the row
## that is running has nothing left to ask for.
const ON_BENCH_LABEL := "On the bench"
## The empty cell. A stock recipe has no equipment and therefore no life to report, and a dash is the
## honest reading of that — never a zeroed bar.
const EMPTY_CELL := "—"

## **THE TIER CHIP WHEN THE BAND OWNS NO UNITS**, keyed off the published `group` and nothing else.
## The wire states ownership (`count`) and states the life wording (`Worn out` / `Never made`); what
## it does not carry is a chip for a tier that does not exist, because there are no units at a tier.
## So the chip says what owning none MEANS for that group: a kit you are without is bare hands, a
## tool you never built is not made. It is a statement of ownership, not a re-derived step-down.
const TIER_CHIP_KIT_NONE := "Bare hands"
const TIER_CHIP_TOOL_NONE := "Not made"
## A held batch: the tier, then the craft grade it was made at. A start-stocked unit was never on a
## bench and carries no grade, so it shows the tier alone.
const TIER_CHIP_GRADED_FORMAT := "%s · %s"
## A STOCK recipe makes no equipment, so its Tier cell states what a pass yields instead.
const TIER_CHIP_STOCK_FORMAT := "→ %s %s"

## `CraftOffer.severity` and `EquipmentBatchState.lifeSeverity` — two vocabularies on purpose (a life
## bar is a fuel gauge, an offer is an invitation, so `good` means nothing on a bar).
const SEVERITY_DANGER := "danger"
const SEVERITY_NEUTRAL := "neutral"
const SEVERITY_GOOD := "good"
const LIFE_SEVERITY_HEALTHY := "healthy"
const LIFE_SEVERITY_WARN := "warn"
const LIFE_SEVERITY_DANGER := "danger"

## The tint each published severity renders in. Unknown ⇒ the quiet ink, so an unrecognised severity
## degrades to a neutral row rather than to an alarm.
const REASON_COLORS := {
	SEVERITY_DANGER: HudStyle.DANGER,
	SEVERITY_GOOD: HudStyle.SIGNAL,
}
const LIFE_COLORS := {
	LIFE_SEVERITY_DANGER: HudStyle.DANGER,
	LIFE_SEVERITY_WARN: HudStyle.WARN,
	LIFE_SEVERITY_HEALTHY: HudStyle.HEALTHY,
}
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
const CHIP_HIGH_COLOR := HudStyle.SIGNAL
const CHIP_LOW_COLOR := HudStyle.INK_FAINT
const CHIP_NEUTRAL_COLOR := HudStyle.INK_DIM

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

## The ledger's five columns. ITEM expands into the slack; the other four are fixed so the table
## reads as columns rather than as five independently-wrapping stacks.
const COLUMN_ITEM_MIN_WIDTH := 150.0
const COLUMN_TIER_WIDTH := 104.0
const COLUMN_LIFE_WIDTH := 132.0
const COLUMN_COST_WIDTH := 140.0
const COLUMN_ACTION_WIDTH := 132.0
const COLUMN_SEPARATION := 10

const LIFE_BAR_HEIGHT := 4.0
const LIFE_BAR_CORNER_RADIUS := 2
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
const BENCH_TEACH_FONT_SIZE := 11
const CREW_COUNT_FONT_SIZE := 18
const CREW_CAPTION_FONT_SIZE := 10
const CREW_BUTTON_SIZE := 24.0
const CREW_BUTTON_FONT_SIZE := 13
const COLUMN_HEAD_FONT_SIZE := 10
const ITEM_NAME_FONT_SIZE := 14
const ITEM_ROLE_FONT_SIZE := 11
const TIER_CHIP_FONT_SIZE := 10
const LIFE_TEXT_FONT_SIZE := 11
const COST_FONT_SIZE := 12
const ACTION_FONT_SIZE := 12
const REASON_FONT_SIZE := 11
const GROUP_HEAD_FONT_SIZE := 10

## The letter-spacing effect the prototype's uppercase eyebrows carry is not expressible in Godot's
## Label, so the heads are simply uppercased — the same treatment `HudWidgets.alloc_section_label`
## already gives every zone head in this HUD.

## The Item cell's second line — what this row IS, every one of them a JOIN of published fields
## rather than an authored table: a tool names the material it bounds (`materials[].tool_item_id`), a
## stock recipe names the characteristic its input is judged on (`inputs[].reads_axis`), and a kit
## row names the craft that makes it (`RecipeDefState.craft`, whose display name is resolved
## sim-side). A row whose join finds nothing simply shows no second line.
const ROLE_TOOL_FORMAT := "Bench tool — %s"
const ROLE_STOCK_FORMAT := "Reads %s %s"

## The cost cell's per-material clause: the amount, then the material.
const COST_CLAUSE_FORMAT := "%s %s"
const COST_SEPARATOR := " · "
## A cost amount reads whole where it is whole — a recipe asking for 12 fibre should not say 12.0.
const COST_AMOUNT_DECIMALS := 1
const COST_WHOLE_EPSILON := 0.05
