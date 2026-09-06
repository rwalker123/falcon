extends AutoSizingPanel
class_name StartingLoadoutPanel

## **THE OPENING LOADOUT PICKER** (issue #629) — the turn-one outfitting screen, composed AFTER the
## player has seen the generated map. A spawning band owns nothing at all (`equipment.json` ships
## `start_stock_fraction: 0.0` and no material declares a start stock), so this window is the ONE
## source of a campaign's starting gear and material.
##
## **IT IS A READOUT AND A WRITE SURFACE AT ONCE, AND THE READOUT IS THE POINT.** Two budgets sit
## side by side because they buy different things out of one decision — hands carrying gear, and a
## pile to build gear FROM — and the third column is the only place a player can see what the second
## budget is actually worth. Its counts answer *"how many could I make if I spent the whole pile on
## this one thing"*, never a simultaneous build plan; `BUILDS_NOTE` is that sentence, on screen.
##
## **NOTHING HERE IS BLOCKING.** The window closing is the SIM's business — it shuts on the first
## turn advance whether or not an order was sent — so this panel warns and never prevents. It is
## DISMISSIBLE (the player has to be able to pan, zoom and read tiles before committing) and comes
## back through its own reopen pill and through the turn orb's row. `End Turn` is untouched.
##
## **THE SIM IS AUTHORITATIVE ABOUT WHETHER IT CLOSED.** The commit control sends the order and puts
## the card away optimistically; if the sim REFUSES, `opening_loadout.open` is still `true` on the
## next frame and the controller re-opens the panel on the refusal notice rather than assuming the
## order landed.
##
## **THIS IS THE FREE-FLOATING CASE, hence `AutoSizingPanel`**
## (`.claude/rules/client/panel-framework.md`): the card is measured against the ROOM — the viewport
## minus every reserved edge strip and every overlay — so `PanelCard` + `DockScrollFit` is the wrong
## half of the pair and would misbehave silently. Both axes are fitted explicitly because this node
## is a plain `Control` and no child's minimum ever reaches it.
##
## **ONE NODE CARRIES BOTH STATES.** The card and the reopen pill are two children of this panel and
## exactly one is visible; the fit measures whichever is showing and `_place` puts the card in the
## middle of the room and the pill at the top of it. A second free-floating node for the pill would
## be a second thing to place, to fit and to tear down on a world rebuild.
##
## The words, the wire keys and the measured geometry live in `HudLoadoutVocab`; the ARITHMETIC (what
## a pile builds, what a budget has left) lives in `StartingLoadoutController`, which is also the
## only thing that holds the allocation. This panel renders a payload and emits intents.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

## The ✕ was pressed — put the card away, leave the window open.
signal dismissed
## The reopen pill was pressed.
signal reopened
## A kit's stepper moved: the kit's id and the count it should now stand at. **The panel never
## clamps** — the controller owns the budget and answers with a fresh payload.
signal kit_count_changed(kit_id: String, count: int)
## A material's stepper moved, in units.
signal material_units_changed(material_id: String, units: int)
## The commit control was pressed. Carries nothing: the controller holds the allocation.
signal commit_requested

# ---- the render payload's keys (this panel's contract with its controller) ----------------------

## `[{id, display_name, jobs, uses, count}]` — the kit roster in the config's own order, `none`
## already excluded, each row carrying the count it currently stands at.
const PAYLOAD_KITS := "kits"
## `[{id, label, color, units}]` — one row per pickable material, in the profile's own order, each
## already carrying its swatch so the legend and the recipe rows cannot resolve a different one.
const PAYLOAD_MATERIALS := "materials"
## `[{id, display_name, work, count, inputs}]` — the craftable recipes, reachable first, where
## `inputs` is `[{material_id, amount, color}]`.
const PAYLOAD_RECIPES := "recipes"
## `{spent, budget}` for each meter.
const PAYLOAD_KIT_BUDGET := "kit_budget"
const PAYLOAD_MATERIAL_BUDGET := "material_budget"
const BUDGET_SPENT := "spent"
const BUDGET_TOTAL := "total"

var _card: PanelContainer = null
var _pill: Button = null
var _scroll: ScrollContainer = null
var _body: VBoxContainer = null
var _header: VBoxContainer = null
var _columns: HBoxContainer = null
var _footer: HBoxContainer = null
var _fit_pending: bool = false

## The last payload rendered, so a re-fit after a room change has something to measure.
var _payload: Dictionary = {}

func _ready() -> void:
	super()
	name = "StartingLoadoutPanel"
	# The panel eats its own clicks and only its own: a press on a stepper must never also select the
	# hex behind it, and a press one pixel outside must still reach `MapView._unhandled_input`.
	mouse_filter = Control.MOUSE_FILTER_STOP
	target_width = HudLoadoutVocab.PANEL_WIDTH
	min_height = HudLoadoutVocab.PANEL_MIN_HEIGHT
	bottom_margin = HudLoadoutVocab.VIEWPORT_MARGIN
	# `_place()` centres the CARD in its room, so the height fit's ceiling is the room's whole height
	# and is taken off the room rect — the card is never moved in order to be measured.
	centred_in_room = true
	visible = false

	_card = PanelContainer.new()
	_card.name = "LoadoutCard"
	_card.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_card.mouse_filter = Control.MOUSE_FILTER_STOP
	_card.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	add_child(_card)

	var column := VBoxContainer.new()
	column.name = "LoadoutColumn"
	column.add_theme_constant_override("separation", 0)
	_card.add_child(column)

	_header = VBoxContainer.new()
	_header.name = "LoadoutHeader"
	_header.add_theme_constant_override("separation", HudLoadoutVocab.BUDGET_ROW_SEPARATION)
	column.add_child(_wrap_padded(_header,
		HudLoadoutVocab.HEADER_PADDING_H, HudLoadoutVocab.HEADER_PADDING_V))
	column.add_child(_rule(HudStyle.LINE))

	# ONE scroll around the three columns, for `KnowledgePanel`'s reason: this card is measured
	# against real room, and a short window genuinely can leave less of it than nine kit rows need.
	_scroll = ScrollContainer.new()
	_scroll.name = "LoadoutScroll"
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.name = "LoadoutBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.add_child(_body)

	_columns = HBoxContainer.new()
	_columns.name = "LoadoutColumns"
	_columns.add_theme_constant_override("separation", HudLoadoutVocab.COLUMN_SEPARATION)
	_columns.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var columns_host := _wrap_padded(_columns,
		HudLoadoutVocab.COLUMNS_PADDING_H, HudLoadoutVocab.COLUMNS_PADDING_V)
	columns_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_child(columns_host)

	column.add_child(_rule(HudStyle.LINE))
	_footer = HBoxContainer.new()
	_footer.name = "LoadoutFooter"
	_footer.add_theme_constant_override("separation", HudLoadoutVocab.FOOTER_SEPARATION)
	column.add_child(_wrap_padded(_footer,
		HudLoadoutVocab.FOOTER_PADDING_H, HudLoadoutVocab.FOOTER_PADDING_V))

	# The dismissed state. A child of this panel rather than a second free-floating node, so one fit
	# and one placement serve both states — see the docstring.
	_pill = Button.new()
	_pill.name = "LoadoutReopenPill"
	_pill.text = HudLoadoutVocab.REOPEN_LABEL
	_pill.tooltip_text = HudLoadoutVocab.REOPEN_TOOLTIP
	_pill.focus_mode = Control.FOCUS_NONE
	_pill.visible = false
	# It FILLS the panel, and the collapsed fit shrinks the panel to the pill's own minimum — which
	# together are what centre it. Left at its natural size inside a card-width panel it would sit at
	# that panel's top-LEFT corner, a third of a screen off centre, while every measurement said it
	# was placed correctly.
	_pill.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_pill.add_theme_font_size_override("font_size", HudLoadoutVocab.REOPEN_FONT_SIZE)
	HudStyle.apply_button(_pill, "primary")
	_pill.pressed.connect(func() -> void: reopened.emit())
	add_child(_pill)

# ---- public API -------------------------------------------------------------

## Rebuild the card against `payload` (see the `PAYLOAD_*` keys) and show it EXPANDED.
func render(payload: Dictionary) -> void:
	_payload = payload
	_card.visible = true
	_pill.visible = false
	HudWidgets.clear_children(_header)
	HudWidgets.clear_children(_columns)
	HudWidgets.clear_children(_footer)
	_build_header()
	_build_columns(payload)
	_build_footer()
	# **VISIBLE BEFORE THE FIT, and that is load-bearing**: `Container._sort_children` early-returns
	# on a hidden subtree, so a card kept hidden until it had been measured would never lay its
	# content out and would measure the unwrapped lower bound forever.
	visible = true
	# …and it shows at its NOMINAL width BEFORE the frame it is measured in, so the height read a frame
	# from now is a function of the width the content was actually laid out at. The test is the width
	# itself rather than `has_fitted_width()`, because a card returning from the COLLAPSED state is
	# fitted — to the reopen pill — and measuring three columns at a pill's width reports the wrapping
	# of a layout that is about to be thrown away.
	target_width = HudLoadoutVocab.PANEL_WIDTH
	if _fitted_width < target_width:
		fit_width(0.0, 0.0)
	refit()

## Put the card away and leave the reopen pill in its place. **NOT `dismiss()`** — the window is
## still open as far as the sim is concerned, and the name has to say which of the two states this
## is; `close()` is the one that takes the whole surface off screen.
func collapse() -> void:
	_card.visible = false
	_pill.visible = true
	visible = true
	if _scroll != null:
		_scroll.scroll_vertical = 0
	refit()

## The window has shut (the sim says `open == false`, or the world was rebuilt). Everything goes.
func close() -> void:
	visible = false
	_card.visible = false
	_pill.visible = false
	_payload = {}
	if _scroll != null:
		_scroll.scroll_vertical = 0
	HudWidgets.clear_children(_header)
	HudWidgets.clear_children(_columns)
	HudWidgets.clear_children(_footer)

func is_open() -> bool:
	return visible

## True while the full card is showing (as opposed to the reopen pill).
func is_expanded() -> bool:
	return visible and _card != null and _card.visible

## The `PanelContainer` that DRAWS the card. A real Container, so its combined minimum is the honest
## measure of whether the card is holding its content or quietly growing out of itself.
func card() -> PanelContainer:
	return _card

## The reopen pill, for the harnesses' assertions.
func reopen_pill() -> Button:
	return _pill

## Re-fit to content and re-place, across TWO frames.
##
## **FRAME ONE waits for the rebuilt body to be laid out at all**; a measurement taken in the same
## frame the body was rebuilt reports the PREVIOUS content's wrapping.
##
## ⛔ **FRAME TWO IS THE ONE THAT IS EASY TO LEAVE OUT, AND IT IS WHY THE HEIGHT IS READ AFTER THE
## WIDTH FIT RATHER THAN BESIDE IT.** `fit_width` resolves the card to its content's width, which is
## NOT `target_width` — the three columns want 916 against a 900 nominal — and applying a width does
## not lay the body out; the container sorts on the next layout pass. So a height read in the same
## pass is the wrapping of a column that no longer exists, and this card is full of `AUTOWRAP_WORD_SMART`
## labels (the subtitle, both column notes, every empty notice) whose minimum HEIGHT is a function of
## the width they were last laid out at. Measured against a stale narrow width they report close to
## one word per line, which is a card hundreds of pixels taller than its content with all of it as
## dead space under the columns.
##
## **`ComposeSheet.refit` shipped exactly this bug** and was fixed exactly this way — see
## `.claude/rules/client/harness-ui-preview.md` → "a latent fit race was fixed". It is repeated here
## rather than shared because the two cards have different chrome and different collapsed states.
##
## `_fit_pending` spans BOTH frames, so a re-entrant `refit()` cannot interleave halves; every exit
## path clears it.
func refit() -> void:
	if not visible or _fit_pending or _body == null:
		return
	_fit_pending = true
	await get_tree().process_frame
	if not visible or _body == null:
		_fit_pending = false
		return
	if _fit_collapsed():
		_fit_pending = false
		return
	_fit_expanded_width()
	await get_tree().process_frame
	_fit_pending = false
	if not visible or _body == null:
		return
	# **RE-CHECKED, because the card can be dismissed BETWEEN the two frames** — the collapse's own
	# `refit()` was dropped by `_fit_pending`, so this is the call that has to notice. Without it the
	# pill would be left wearing a 900px panel, which is the state the collapsed branch exists to
	# prevent.
	if _fit_collapsed():
		return
	_fit_expanded_height()

## The COLLAPSED fit — the reopen pill and nothing else. Answers whether it applied, so both frames of
## `refit` can ask the same question and get the same answer.
func _fit_collapsed() -> bool:
	if _card.visible:
		return false
	var room := _room()
	# The collapsed state is a BUTTON, and it brings its own minimum — no card chrome, no scroll
	# gutter and no nominal width, or the pill would be dressed in 900px of invisible panel that
	# still eats every click behind it.
	var pill_min := _pill.get_combined_minimum_size()
	# **THE NOMINAL WIDTH MOVES WITH THE STATE.** `fit_width` never resolves below `target_width`, so
	# leaving it at the card's would keep the collapsed panel a full card wide with the pill drawn in
	# its corner.
	target_width = pill_min.x
	max_width = maxf(room.size.x, pill_min.x)
	fit_width(pill_min.x, 0.0)
	max_height = room.size.y
	min_height = pill_min.y
	fit_to_content(pill_min.y, 0.0)
	_place()
	return true

func _fit_expanded_width() -> void:
	var room := _room()
	min_height = HudLoadoutVocab.PANEL_MIN_HEIGHT
	target_width = HudLoadoutVocab.PANEL_WIDTH
	max_width = maxf(room.size.x, target_width)
	fit_width(_body.get_combined_minimum_size().x,
		HudStyle.card_stylebox().get_minimum_size().x + _scroll_gutter())

func _fit_expanded_height() -> void:
	var room := _room()
	# The height fit's ceiling is the WHOLE room and the card does not move to be measured —
	# `centred_in_room` is how the base class is told so.
	max_height = room.size.y
	fit_to_content(_body.get_combined_minimum_size().y + _chrome_height(),
		HudStyle.card_stylebox().get_minimum_size().y, _scroll)
	_place()

# ---- header -----------------------------------------------------------------

func _build_header() -> void:
	var title_row := HBoxContainer.new()
	title_row.add_theme_constant_override("separation", HudLoadoutVocab.HEADER_SEPARATION)
	var title := Label.new()
	title.text = HudLoadoutVocab.PANEL_TITLE.to_upper()
	title.add_theme_font_size_override("font_size", HudLoadoutVocab.TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK)
	title_row.add_child(title)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	title_row.add_child(spacer)

	var dismiss := Button.new()
	dismiss.text = HudLoadoutVocab.DISMISS_GLYPH
	dismiss.tooltip_text = HudLoadoutVocab.DISMISS_TOOLTIP
	dismiss.focus_mode = Control.FOCUS_NONE
	HudStyle.apply_button(dismiss, "ghost")
	dismiss.pressed.connect(func() -> void: dismissed.emit())
	title_row.add_child(dismiss)
	_header.add_child(title_row)

	_header.add_child(_caption(HudLoadoutVocab.PANEL_SUBTITLE, HudStyle.INK_DIM,
		HudLoadoutVocab.SUBTITLE_FONT_SIZE, true))

# ---- the three columns ------------------------------------------------------

func _build_columns(payload: Dictionary) -> void:
	_columns.add_child(_build_kits_column(payload))
	_columns.add_child(_column_seam())
	_columns.add_child(_build_materials_column(payload))
	_columns.add_child(_column_seam())
	_columns.add_child(_build_recipes_column(payload))

## COLUMN 1 — the kits. **Every row starts at 0**: picking is the whole point of the screen, so a
## pre-filled kit column would answer the one question it exists to ask.
func _build_kits_column(payload: Dictionary) -> Control:
	var col := _column(HudLoadoutVocab.KITS_HEAD, HudLoadoutVocab.KITS_NOTE)
	var budget: Dictionary = payload.get(PAYLOAD_KIT_BUDGET, {})
	col.add_child(_budget_meter(HudLoadoutVocab.BUDGET_KITS, budget, _kit_bar_segments(budget)))
	var rows: Array = payload.get(PAYLOAD_KITS, [])
	if rows.is_empty():
		col.add_child(_caption(HudLoadoutVocab.EMPTY_NOTICE, HudStyle.INK_FAINT,
			HudLoadoutVocab.ROW_NOTE_FONT_SIZE, true))
		return col
	var remaining := int(budget.get(BUDGET_TOTAL, 0)) - int(budget.get(BUDGET_SPENT, 0))
	for row_variant in rows:
		if row_variant is Dictionary:
			col.add_child(_kit_row(row_variant as Dictionary, remaining > 0))
	return col

func _kit_row(row: Dictionary, can_add: bool) -> Control:
	var kit_id := String(row.get("id", ""))
	var count := int(row.get("count", 0))
	var block := VBoxContainer.new()
	block.add_theme_constant_override("separation", 0)
	block.set_meta(HudLoadoutVocab.KIT_ROW_META, kit_id)

	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudLoadoutVocab.ROW_SEPARATION)
	var name_label := Label.new()
	name_label.text = String(row.get("display_name", kit_id))
	name_label.add_theme_font_size_override("font_size", HudLoadoutVocab.ROW_FONT_SIZE)
	name_label.add_theme_color_override("font_color",
		HudStyle.INK if count > 0 else HudStyle.INK_DIM)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(name_label)
	_add_stepper(line, count, can_add,
		func(next: int) -> void: kit_count_changed.emit(kit_id, next))
	block.add_child(line)

	block.add_child(_caption(String(row.get("jobs_text", "")), HudStyle.INK_FAINT,
		HudLoadoutVocab.ROW_NOTE_FONT_SIZE))
	block.add_child(_caption(String(row.get("uses_text", "")), HudStyle.INK_FAINT,
		HudLoadoutVocab.ROW_NOTE_FONT_SIZE))
	return block

## COLUMN 2 — the resources. **It opens on the profile's published defaults**, not on zero: the sim
## suggests a starting pile, and a player who touches nothing still walks out with something to
## build from.
func _build_materials_column(payload: Dictionary) -> Control:
	var col := _column(HudLoadoutVocab.RESOURCES_HEAD, HudLoadoutVocab.RESOURCES_NOTE)
	var budget: Dictionary = payload.get(PAYLOAD_MATERIAL_BUDGET, {})
	var rows: Array = payload.get(PAYLOAD_MATERIALS, [])
	col.add_child(_budget_meter(HudLoadoutVocab.BUDGET_MATERIALS, budget,
		_material_bar_segments(rows, budget)))
	if rows.is_empty():
		col.add_child(_caption(HudLoadoutVocab.EMPTY_NOTICE, HudStyle.INK_FAINT,
			HudLoadoutVocab.ROW_NOTE_FONT_SIZE, true))
		return col
	var remaining := int(budget.get(BUDGET_TOTAL, 0)) - int(budget.get(BUDGET_SPENT, 0))
	for row_variant in rows:
		if row_variant is Dictionary:
			col.add_child(_material_row(row_variant as Dictionary, remaining > 0))
	return col

func _material_row(row: Dictionary, can_add: bool) -> Control:
	var material_id := String(row.get("id", ""))
	var units := int(row.get("units", 0))
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudLoadoutVocab.SWATCH_SEPARATION)
	line.set_meta(HudLoadoutVocab.MATERIAL_ROW_META, material_id)
	line.add_child(_swatch(row.get("color", HudStyle.INK_FAINT)))
	var name_label := Label.new()
	name_label.text = String(row.get("label", material_id))
	name_label.add_theme_font_size_override("font_size", HudLoadoutVocab.ROW_FONT_SIZE)
	name_label.add_theme_color_override("font_color",
		HudStyle.INK if units > 0 else HudStyle.INK_DIM)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(name_label)
	_add_stepper(line, units, can_add,
		func(next: int) -> void: material_units_changed.emit(material_id, next))
	return line

## COLUMN 3 — what the pile builds. The legend rides ABOVE the list because it is the key for the
## swatches on every row below it, and it names EVERY pickable material in the resources column's own
## order — including the ones nothing on the list is made of, which is itself a reading.
func _build_recipes_column(payload: Dictionary) -> Control:
	var col := _column(HudLoadoutVocab.BUILDS_HEAD, HudLoadoutVocab.BUILDS_NOTE)
	col.add_child(_legend(payload.get(PAYLOAD_MATERIALS, [])))
	var rows: Array = payload.get(PAYLOAD_RECIPES, [])
	if rows.is_empty():
		col.add_child(_caption(HudLoadoutVocab.EMPTY_NOTICE, HudStyle.INK_FAINT,
			HudLoadoutVocab.ROW_NOTE_FONT_SIZE, true))
		return col
	for row_variant in rows:
		if row_variant is Dictionary:
			col.add_child(_recipe_row(row_variant as Dictionary))
	return col

func _legend(materials: Array) -> Control:
	var block := VBoxContainer.new()
	block.add_theme_constant_override("separation", HudLoadoutVocab.BUDGET_ROW_SEPARATION)
	block.add_child(_caption(HudLoadoutVocab.LEGEND_HEAD.to_upper(), HudStyle.INK_FAINT,
		HudLoadoutVocab.LEGEND_FONT_SIZE))
	var flow := HFlowContainer.new()
	flow.add_theme_constant_override("h_separation", HudLoadoutVocab.LEGEND_SEPARATION)
	for material_variant in materials:
		if not (material_variant is Dictionary):
			continue
		var material: Dictionary = material_variant
		var entry := HBoxContainer.new()
		entry.add_theme_constant_override("separation", HudLoadoutVocab.SWATCH_SEPARATION)
		entry.set_meta(HudLoadoutVocab.LEGEND_ENTRY_META, String(material.get("id", "")))
		entry.add_child(_swatch(material.get("color", HudStyle.INK_FAINT)))
		entry.add_child(_caption(String(material.get("label", "")), HudStyle.INK_DIM,
			HudLoadoutVocab.LEGEND_FONT_SIZE))
		flow.add_child(entry)
	block.add_child(flow)
	return block

func _recipe_row(row: Dictionary) -> Control:
	var count := int(row.get("count", 0))
	var reachable := count > 0
	var block := VBoxContainer.new()
	block.add_theme_constant_override("separation", 0)
	block.set_meta(HudLoadoutVocab.RECIPE_ROW_META, String(row.get("id", "")))
	# **DIMMED, NEVER HIDDEN.** A recipe the pile cannot reach is the column's most useful row: it is
	# what tells a player they are one unit of hide short of a tunic.
	block.modulate.a = 1.0 if reachable else HudLoadoutVocab.UNREACHABLE_ALPHA

	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudLoadoutVocab.ROW_SEPARATION)
	var name_label := Label.new()
	name_label.text = String(row.get("display_name", ""))
	name_label.add_theme_font_size_override("font_size", HudLoadoutVocab.ROW_FONT_SIZE)
	name_label.add_theme_color_override("font_color",
		HudStyle.INK if reachable else HudStyle.INK_DIM)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(name_label)
	var count_label := Label.new()
	count_label.text = (HudLoadoutVocab.RECIPE_COUNT_FORMAT % count) if reachable \
		else HudLoadoutVocab.RECIPE_COUNT_NONE
	count_label.set_meta(HudLoadoutVocab.RECIPE_COUNT_META, count)
	count_label.add_theme_font_size_override("font_size", HudLoadoutVocab.ROW_FONT_SIZE)
	count_label.add_theme_color_override("font_color",
		HudStyle.SIGNAL if reachable else HudStyle.INK_FAINT)
	line.add_child(count_label)
	block.add_child(line)

	var cost := HFlowContainer.new()
	cost.add_theme_constant_override("h_separation", HudLoadoutVocab.LEGEND_SEPARATION)
	for input_variant in row.get("inputs", []):
		if not (input_variant is Dictionary):
			continue
		var input: Dictionary = input_variant
		var pair := HBoxContainer.new()
		pair.add_theme_constant_override("separation", HudLoadoutVocab.SWATCH_SEPARATION)
		pair.add_child(_swatch(input.get("color", HudStyle.INK_FAINT)))
		pair.add_child(_caption(HudLoadoutVocab.RECIPE_INPUT_AMOUNT_FORMAT
			% HudLoadoutVocab.amount_text(float(input.get("amount", 0.0))),
			HudStyle.INK_DIM, HudLoadoutVocab.ROW_NOTE_FONT_SIZE))
		cost.add_child(pair)
	# The work value rides with the costs: a recipe's price is its materials AND the hands it takes.
	cost.add_child(_caption(HudLoadoutVocab.RECIPE_WORK_FORMAT
		% HudLoadoutVocab.amount_text(float(row.get("work", 0.0))),
		HudStyle.INK_FAINT, HudLoadoutVocab.ROW_NOTE_FONT_SIZE))
	block.add_child(cost)
	return block

# ---- footer -----------------------------------------------------------------

## The commit control. **Its face is a CONSTANT** — see `HudLoadoutVocab.COMMIT_CLEAR_LABEL`: an
## apply is a replacement the player may revise until the turn advances, so a label conditioned on
## what is unspent would name a consequence the press does not have.
func _build_footer() -> void:
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_footer.add_child(spacer)
	var commit := Button.new()
	commit.text = HudLoadoutVocab.COMMIT_CLEAR_LABEL
	commit.tooltip_text = HudLoadoutVocab.COMMIT_TOOLTIP
	commit.focus_mode = Control.FOCUS_NONE
	commit.set_meta(HudLoadoutVocab.COMMIT_BUTTON_META, true)
	commit.add_theme_font_size_override("font_size", HudLoadoutVocab.COMMIT_FONT_SIZE)
	HudStyle.apply_button(commit, "primary")
	commit.pressed.connect(func() -> void: commit_requested.emit())
	_footer.add_child(commit)

# ---- the budget meters ------------------------------------------------------

## A meter is the REMAINDER and a stacked bar, and nothing else. Printing "28 of 30 packed" beside
## "2 / 30 left" states one fact twice, and the bar already draws the spent half.
func _budget_meter(key: String, budget: Dictionary, segments: Array) -> Control:
	var block := VBoxContainer.new()
	block.add_theme_constant_override("separation", HudLoadoutVocab.BUDGET_ROW_SEPARATION)
	block.set_meta(HudLoadoutVocab.BUDGET_METER_META, key)
	var total := int(budget.get(BUDGET_TOTAL, 0))
	var remaining := total - int(budget.get(BUDGET_SPENT, 0))
	var label := Label.new()
	label.text = HudLoadoutVocab.BUDGET_REMAINING_FORMAT % [remaining, total]
	label.add_theme_font_size_override("font_size", HudLoadoutVocab.BUDGET_FONT_SIZE)
	# A budget with nothing left is not a problem — it is a finished decision — so it reads in the
	# calm signal ink rather than in a warning colour.
	label.add_theme_color_override("font_color",
		HudStyle.INK_DIM if remaining > 0 else HudStyle.SIGNAL)
	block.add_child(label)
	var bar := HudWidgets.build_composition_bar(segments)
	bar.custom_minimum_size = Vector2(0.0, HudLoadoutVocab.BUDGET_BAR_HEIGHT)
	block.add_child(bar)
	return block

## The KIT bar is ONE spent segment against its remainder. Kits are interchangeable pairs of hands
## and there is nothing to tell apart, so stacking them by kit would introduce a second colour
## vocabulary next to the material legend — see `HudLoadoutVocab`'s docstring.
func _kit_bar_segments(budget: Dictionary) -> Array:
	var total := int(budget.get(BUDGET_TOTAL, 0))
	var spent := int(budget.get(BUDGET_SPENT, 0))
	var segments: Array = []
	if spent > 0:
		segments.append({"key": HudLoadoutVocab.BUDGET_KITS, "count": spent,
			"color": HudLoadoutVocab.BUDGET_SPENT_COLOR})
	# **A REMAINDER OF ZERO DRAWS NOTHING.** `build_composition_bar` floors every segment's stretch
	# ratio at `COMPOSITION_MIN_RATIO` so a one-person segment stays a visible sliver, which means a
	# zero-count segment handed to it renders as a sliver of *nothing left* on a budget that is fully
	# spent — a bar that never quite fills.
	var remaining := total - spent
	if remaining > 0:
		segments.append({"key": "", "count": remaining,
			"color": HudLoadoutVocab.BUDGET_REMAINDER_COLOR})
	return segments

## The MATERIAL bar stacks by material, in the swatch colours the legend and the recipe rows use — so
## the bar is a picture of the pile in exactly the colours the third column prices it in.
func _material_bar_segments(rows: Array, budget: Dictionary) -> Array:
	var segments: Array = []
	var spent := 0
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		var units := int(row.get("units", 0))
		if units <= 0:
			continue
		spent += units
		segments.append({"key": String(row.get("id", "")), "count": units,
			"color": row.get("color", HudStyle.INK_FAINT),
			"tooltip": String(row.get("label", ""))})
	var remaining := int(budget.get(BUDGET_TOTAL, 0)) - spent
	if remaining > 0:
		segments.append({"key": "", "count": remaining,
			"color": HudLoadoutVocab.BUDGET_REMAINDER_COLOR})
	return segments

# ---- small builders ---------------------------------------------------------

func _column(head_text: String, note_text: String) -> VBoxContainer:
	var col := VBoxContainer.new()
	col.custom_minimum_size = Vector2(HudLoadoutVocab.COLUMN_MIN_WIDTH, 0.0)
	col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	col.add_theme_constant_override("separation", HudLoadoutVocab.COLUMN_ROW_SEPARATION)
	var head := Label.new()
	head.text = head_text.to_upper()
	head.add_theme_font_size_override("font_size", HudLoadoutVocab.COLUMN_HEAD_FONT_SIZE)
	head.add_theme_color_override("font_color", HudStyle.INK_DIM)
	col.add_child(head)
	# **AN EMPTY NOTE DRAWS NOTHING.** A caption node holding `""` still takes a row of layout, which
	# is a blank line under one column's head and not under the other two — the ragged column the
	# builds note's deletion exists to remove.
	if not note_text.is_empty():
		col.add_child(_caption(note_text, HudStyle.INK_FAINT,
			HudLoadoutVocab.COLUMN_NOTE_FONT_SIZE, true))
	return col

## The vertical rule between two columns. **It is `HudStyle.LINE`, the same ink as the horizontal
## rules above and below the body, and NOT `hairline_stylebox()`** — a hairline is `LINE_SOFT`, which
## measured within a few values of the card's own `PANEL_SOLID` and vanished entirely at this HUD's
## fractional canvas scale. A seam nobody can see is not a quieter seam; it is no seam.
func _column_seam() -> Control:
	var seam := Panel.new()
	seam.custom_minimum_size = Vector2(HudLoadoutVocab.COLUMN_SEAM_THICKNESS, 0.0)
	var box := StyleBoxFlat.new()
	box.bg_color = HudStyle.LINE
	seam.add_theme_stylebox_override("panel", box)
	seam.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return seam

func _swatch(color: Variant) -> ColorRect:
	var rect := ColorRect.new()
	rect.color = color if color is Color else HudStyle.INK_FAINT
	rect.custom_minimum_size = HudLoadoutVocab.SWATCH_SIZE
	rect.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return rect

## The `− n +` control, in this panel's own compact metric. `HudWidgets.add_stepper_controls` is the
## one stepper primitive in the HUD; a hand-rolled pair of buttons here would be a second one.
func _add_stepper(row: HBoxContainer, count: int, plus_enabled: bool, on_change: Callable) -> void:
	HudWidgets.add_stepper_controls(row, count, plus_enabled, on_change, true, {
		HudWidgets.STEPPER_METRIC_BUTTON_WIDTH: HudLoadoutVocab.STEPPER_BUTTON_WIDTH,
		HudWidgets.STEPPER_METRIC_VALUE_WIDTH: HudLoadoutVocab.STEPPER_VALUE_WIDTH,
		HudWidgets.STEPPER_METRIC_PADDING_H: HudLoadoutVocab.STEPPER_PADDING_H,
	})

func _caption(text: String, ink: Color, font_size: int, wrap: bool = false) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", ink)
	if wrap:
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	return label

func _rule(color: Color) -> Control:
	var rule := Panel.new()
	rule.custom_minimum_size = Vector2(0.0, HudLoadoutVocab.RULE_THICKNESS)
	var box := StyleBoxFlat.new()
	box.bg_color = color
	rule.add_theme_stylebox_override("panel", box)
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return rule

func _wrap_padded(content: Control, padding_h: int, padding_v: int) -> MarginContainer:
	var host := MarginContainer.new()
	host.add_theme_constant_override("margin_left", padding_h)
	host.add_theme_constant_override("margin_right", padding_h)
	host.add_theme_constant_override("margin_top", padding_v)
	host.add_theme_constant_override("margin_bottom", padding_v)
	host.add_child(content)
	return host

# ---- geometry ---------------------------------------------------------------

## The room the card may use — the viewport MINUS every reserved edge strip and every overlay, which
## the controller hands over as `room_bounds`. A card measured against the whole window grows under a
## docked panel's strip and under the event bar.
func _room() -> Rect2:
	return available_room(HudLoadoutVocab.VIEWPORT_MARGIN)

## The card centres in the room; the reopen pill sits at the TOP of it, where it cannot be mistaken
## for a control belonging to whatever the player dismissed the card to look at.
func _place() -> void:
	var room := _room()
	var x := room.position.x + maxf((room.size.x - size.x) * 0.5, 0.0)
	if _card != null and _card.visible:
		position = Vector2(x, room.position.y + maxf((room.size.y - size.y) * 0.5, 0.0))
		return
	position = Vector2(x, room.position.y + HudLoadoutVocab.REOPEN_PADDING)

## Everything in the card that is NOT inside the scroll: the header block, the footer block and the
## two hairlines between them.
func _chrome_height() -> float:
	var height := HudLoadoutVocab.RULE_THICKNESS * 2.0
	if _header != null and _header.get_parent() is Control:
		height += (_header.get_parent() as Control).get_combined_minimum_size().y
	if _footer != null and _footer.get_parent() is Control:
		height += (_footer.get_parent() as Control).get_combined_minimum_size().y
	return height

## The room the vertical scrollbar needs, whether or not it is currently shown. Reserved
## unconditionally: the ceiling here is the ROOM, so a taller or shorter window turns the internal
## scrollbar on and off, and a gutter reserved only while scrolling would jump the card's width.
func _scroll_gutter() -> float:
	if _scroll == null:
		return 0.0
	return _scroll.get_v_scroll_bar().get_combined_minimum_size().x
