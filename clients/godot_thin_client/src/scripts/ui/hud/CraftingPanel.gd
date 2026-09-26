extends AutoSizingPanel
class_name CraftingPanel

## **THE MATERIALS & CRAFTING PANEL** (`docs/plan_crafting_and_materials.md` §7) — where a band's raw
## materials live, what a kit costs to rebuild, and why a given kit cannot be built right now. Its own
## surface, launched from the Band/City panel header, with the band named by a picker.
##
## **IT RENDERS THE SIM'S ANSWER AND DERIVES NOTHING.** Every refusal (`CraftOffer.reason` +
## `severity`), every shortfall number, every grade and every life wording is resolved sim-side and
## reproduced VERBATIM here — that is the rule `snapshot.fbs` states in as many words, and the reason
## the reason is published at all: the derivation needs the band's batch readings, the tool that
## bounds a material, the recipe's grade seams and the item's wear quantum, and a client cannot join
## those correctly. A panel that re-derived *"cannot craft"* from `available` would also lose the one
## distinction the whole vocabulary exists for — **"Not needed yet" is a shrug and "Short 4.9 bone" is
## a problem**, and they are different strings with different severities precisely so they can read
## differently.
##
## **THIS PANEL ANSWERS "WHAT DOES IT COST TO REBUILD", SO IT CARRIES NO CONDITION READOUT.** How worn
## the gear is has one home and it is the Band panel's WORKFORCE role cards, which state the condition
## of the item behind each kit beside the role that kit sets. The ledger's four columns are Item ·
## Owned · Rebuild costs · action, and `EquipmentBatchState.life` is not among them: a second copy of
## condition here would be the same fact stated twice, in two places free to disagree.
##
## **ONE SHORT ROW PER ITEM, AND ITS RECIPES LIVE BEHIND A LINK.** The ledger groups the published
## `CraftOffer`s by `outputItemId` (a material recipe is its own row), so Spears is ONE row whether it
## is pointed with bone or knapped from stone. The row's cells describe the SUGGESTED offer — the one
## the sim marked, which is exactly one per row — and an item with more than one recipe carries an
## `N recipes` link under its name that opens a read-only popup comparing them. **Make on such a row
## opens a picker under it** rather than starting a build, because the recipe cannot be changed once
## the build starts; Make on a single-recipe row starts it straight away, as it always did. The
## sections are the three groups — `Kit` · `Bench tools` · `Materials` — and nothing sorts a row under
## a tier any more: a row carrying both a plain and a flint recipe has no one tier to be sorted under.
##
## **NO TIER WORD REACHES THE OWNED CELL AT ALL.** It states count and grade, nothing else: no
## `tier_id`, and not the sim's `ownedNote` (still published, rendered nowhere). Which tier the band
## holds is answered in the recipe popup's Owned column, on an item whose recipes make different tiers,
## and a recipe's own tier is named by its `recipeLabel` there and in the picker.
##
## **OWNERSHIP IS `count`, NEVER `remaining == 0`.** A batch that runs out of units is removed, so a
## worn-out item and one the band never made both read `remaining 0` — which is why the Owned cell is
## keyed off `count` and states the CONSEQUENCE of owning none (`Not made`, one wording for every
## group) rather than a step-down. Owning units reads **one line per GRADE**, because a band may hold one item at two and
## `×5 · excellent` would be a lie.
##
## **MAKE STAGES THE JOB; THE PLAYER STAFFS IT.** Pressing Make emits `make_requested` (→
## `set_bench`) and recruits nobody, so *"No one at the bench"* is the ordinary state one click later
## and the `− n +` stepper (`crew_changed` → `bench_crew`) is the only thing that picks the number. The
## running row's button reads *On the bench* and is spent, and the well's ✕ emits
## `clear_bench_requested` (→ `clear_bench`) — the only way off a bench that is not "make something
## else", which spends the drawn pile without saying so. One job at a time, so this panel never has to
## explain a queue — and there is no Crafter role card on the Band panel: crafting always has a
## subject, so it is staffed at the bench like a worked source rather than through a standing role.
##
## **THIS IS THE FREE-FLOATING CASE, hence `AutoSizingPanel`** (`.claude/rules/client/panel-framework.md`):
## the card is measured against the ROOM — the viewport MINUS every reserved edge strip, which is the
## rect the map and the HUD are already living in — rather than against a dock's remaining height, so
## `PanelCard` + `DockScrollFit` is the wrong half of the pair and would misbehave silently. Both axes
## are fitted explicitly because this node is a plain `Control` and no child minimum ever reaches it.
##
## **PROVENANCE IS DEFERRED AND MUST ARRIVE AS A POPOVER.** Where a batch came from and what it earns
## per turn is a second question; putting it on the rail row made the rail a wall of prose. When it
## lands it is a `DisclosureController`-style popover off the material row, never an inline expansion
## — `band-readouts.md` records that as a correctness rule, inline growth having sliced the rows
## beneath a `clip_contents` host once already.
##
## The words, the wire keys and the measured geometry live in `HudCraftingVocab`; the panel holds no
## const block of its own beyond the payload contract below.

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# --- the panel's OWN signals; `CraftingPanelController` connects and translates each -------------
## The ✕ was pressed.
signal closed
## A band was picked out of the dropdown, by entity.
signal band_selected(entity: int)
## An arrow was pressed: -1 walks back, +1 walks forward.
signal cycle_requested(delta: int)
## Make was pressed on a row — `set_bench <faction> <band> recipe <id>`.
signal make_requested(recipe_id: String)
## The bench stepper moved — `bench_crew <faction> <band> workers <n>`.
signal crew_changed(workers: int)
## The bench's ✕ was pressed — `clear_bench <faction> <band>`. The job comes off, the crew returns to
## the idle pool and the pile already drawn is spent, which is why the button's tooltip names it.
signal clear_bench_requested
## A rung of the bench's rank picker was pressed — `bench_priority <faction> <band> high|normal|low`
## (`docs/plan_standing_upkeep.md` §4.9 item 9b). `level` is one of `HudWorkVocab`'s three tokens,
## already normalized, so nothing between here and the socket re-spells it.
##
## **A SIBLING VERB, NOT A `work_priority` TOKEN**: `work_priority`'s grammar reads a lone trailing
## token as a herd id, so `work_priority … bench low` would be ambiguous with a herd named `bench`.
signal bench_priority_requested(level: String)

# ---- the render payload's keys (this panel's contract with its controller) ----------------------
const PAYLOAD_BAND := "band"
const PAYLOAD_BAND_LABEL := "band_label"
## 1-based, for the `n / N` count beside the arrows. The arrows and the dropdown are DEAD today —
## there is exactly one player band — and that is the shipped convention, not a bug.
const PAYLOAD_BAND_INDEX := "band_index"
const PAYLOAD_BAND_COUNT := "band_count"
## `[{label, entity}]`, in roster order.
const PAYLOAD_BAND_OPTIONS := "band_options"
const PAYLOAD_MATERIALS := "materials"
## The shared rating vocabulary, ascending — the rail reads only its two ENDS, to decide which chips
## read as a strength and which as a weakness.
const PAYLOAD_BAND_LEGEND := "band_legend"
const PAYLOAD_RECIPES := "recipes"
const PAYLOAD_CRAFT_KNOWLEDGE := "craft_knowledge"
## The ceiling the crew stepper may raise the bench to — the band's idle workers **plus the crew
## already at the bench** (`HudBandLaborState.benchable_workers`), which is a different question from
## "how many are idle" and is why the key's name outlives its meaning. See `_build_crew_stepper`.
const PAYLOAD_IDLE_WORKERS := "idle_workers"

var _card: PanelContainer = null
var _scroll: ScrollContainer = null
var _body: VBoxContainer = null
var _header: HBoxContainer = null
var _rail: VBoxContainer = null
var _main: VBoxContainer = null
var _fit_pending: bool = false

## The last payload rendered, so a re-fit after a viewport change has something to measure.
var _payload: Dictionary = {}

## "No scroll offset is waiting to be restored." A real offset is `>= 0`, so the sentinel has to sit
## outside that range rather than at the top of the ledger, which is a place a player can genuinely be.
const SCROLL_UNSET := -1
## The ledger's scroll offset, carried by `render` across its rebuild and re-applied by `refit` once
## the card's height is settled. See `render`.
var _pending_scroll: int = SCROLL_UNSET

## **WHICH GROUP HEADS ARE FOLDED, keyed by head name.** It is VIEW state and not snapshot state, so
## it does not breach `render(payload)`-is-the-whole-input: it has exactly the standing of the scroll
## offset above, which the panel already carries across a rebuild. Held by NAME rather than by index
## so it survives a band switch, whose ledger may hold a different set of heads — and so folding `Kit`
## on one band leaves it folded on the next, which is what a reader who has stopped looking at a group
## meant.
var _folded: Dictionary = {}

## "No row." A row key is an item id or a stock recipe's own key, never empty, so `""` is free.
const NO_ROW := ""

## **WHICH ROW'S RECIPE POPUP IS OPEN**, by row key — VIEW state with `_folded`'s standing, carried
## across the per-snapshot rebuild so a turn tick does not close a comparison the player is reading.
## The popup itself is ONE persistent `PopupPanel`: a Window cannot change the ledger's height, and it
## closes itself on a click outside it and on `Esc`, which is the whole of that behaviour for free.
var _popup_row: String = NO_ROW
var _recipes_popup: PopupPanel = null
var _recipes_popup_body: VBoxContainer = null
## The link the open popup hangs under. Rebuilt on every render, so it is re-captured by the row that
## draws it and the popup is re-anchored once the fit has settled.
var _popup_anchor: Control = null
## **THE CLICK THAT CLOSED THE POPUP MUST NOT REOPEN IT.** A press outside a popup closes it before the
## GUI pass sees the press — and if that press landed on the popup's own link, the link would then see
## an empty `_popup_row` and open it again, so a second click of the link could never close it. The
## frame and the row the popup last closed on are what tell that press apart from a fresh one.
var _popup_closed_frame: int = -1
var _popup_closed_row: String = NO_ROW

## **WHICH ROW'S MAKE PICKER IS OPEN, AND WHICH RECIPE IS CHOSEN IN IT.** One picker at a time. Dropped
## on a render whose row has disappeared or gone on the bench, and a chosen recipe that is no longer
## available falls back to the default rule (`_default_choice`).
var _picker_row: String = NO_ROW
var _picker_choice: String = ""

## **IS THE BENCH'S RANK PICKER SHOWING?** VIEW state with exactly the standing of `_folded` above and
## of the scroll offset: it is not on the wire, it survives the per-snapshot rebuild, and `render` is
## still the whole of this panel's data input.
##
## A plain `bool`, and the work inspector's three-valued `_work_picker_open` — which this used to be
## contrasted against — is retired outright (`docs/plan_standing_upkeep.md` §4.9 item 12d): that state
## existed only to keep the floor picker and the rank picker from both reserving height in one
## fixed-size zone, and the inspector is a viewport-measured card that draws both at once now. This
## well keeps its `bool` because it has no second expansion to be exclusive WITH, which was always the
## real reason.
var _priority_open: bool = false

func _ready() -> void:
	super()
	name = "CraftingPanel"
	# The panel eats its own clicks and only its own: a press on the ledger must never also select
	# the hex behind it, and a press one pixel outside must still reach `MapView._unhandled_input`.
	mouse_filter = Control.MOUSE_FILTER_STOP
	target_width = HudCraftingVocab.PANEL_WIDTH
	min_height = HudCraftingVocab.PANEL_MIN_HEIGHT
	bottom_margin = HudCraftingVocab.VIEWPORT_MARGIN
	# `_place()` CENTRES this card in its room, so the height fit's ceiling is the room's whole height
	# and is taken off the room rect — the card is never moved in order to be measured. See `refit`.
	centred_in_room = true
	visible = false

	_card = PanelContainer.new()
	_card.name = "CraftingCard"
	_card.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_card.mouse_filter = Control.MOUSE_FILTER_STOP
	_card.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	add_child(_card)

	var column := VBoxContainer.new()
	column.name = "CraftingColumn"
	column.add_theme_constant_override("separation", 0)
	_card.add_child(column)

	_header = HBoxContainer.new()
	_header.name = "CraftingHeader"
	_header.add_theme_constant_override("separation", HudCraftingVocab.HEADER_SEPARATION)
	column.add_child(_header)

	var rule := Panel.new()
	rule.custom_minimum_size = Vector2(0.0, HudCraftingVocab.COLUMN_SEPARATOR_THICKNESS)
	rule.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(rule)

	# ONE scroll around the whole body, and it is not a breach of the no-`ScrollContainer` rule the
	# Band panel keeps: that rule is about content whose height feeds back into a FIXED reservation.
	# This card is measured against the viewport, so its ceiling is real room — and a short window
	# genuinely can leave less of it than the ledger needs.
	_scroll = ScrollContainer.new()
	_scroll.name = "CraftingScroll"
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.name = "CraftingBody"
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.add_child(_body)

	var zones := HBoxContainer.new()
	zones.name = "CraftingZones"
	zones.add_theme_constant_override("separation", 0)
	zones.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_child(zones)

	_rail = VBoxContainer.new()
	_rail.name = "MaterialsRail"
	_rail.custom_minimum_size = Vector2(HudCraftingVocab.RAIL_WIDTH, 0.0)
	_rail.size_flags_horizontal = Control.SIZE_FILL
	_rail.add_theme_constant_override("separation", 0)
	zones.add_child(_wrap_padded(_rail, HudCraftingVocab.RAIL_PADDING_H, HudCraftingVocab.RAIL_PADDING_V))

	var seam := Panel.new()
	seam.custom_minimum_size = Vector2(HudCraftingVocab.COLUMN_SEPARATOR_THICKNESS, 0.0)
	seam.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
	seam.mouse_filter = Control.MOUSE_FILTER_IGNORE
	zones.add_child(seam)

	_main = VBoxContainer.new()
	_main.name = "BenchAndLedger"
	_main.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_main.add_theme_constant_override("separation", HudCraftingVocab.SECTION_SEPARATION)
	var main_host := _wrap_padded(_main, HudCraftingVocab.MAIN_PADDING_H, HudCraftingVocab.MAIN_PADDING_V)
	main_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	zones.add_child(main_host)

# ---- public API -------------------------------------------------------------

## Rebuild the whole panel against `payload` (see the `PAYLOAD_*` keys) and show it.
func render(payload: Dictionary) -> void:
	_payload = payload
	# **THE REBUILD MAY NOT COST THE PLAYER HIS PLACE IN THE LEDGER.** This is a rebuild rather than a
	# diff, and it runs on EVERY snapshot — so a player scrolled down to the bench tools would be thrown
	# back to the top of the table once a turn. The offset is carried across the rebuild instead of the
	# rebuild being skipped, because every turn tick genuinely changes the payload (the bench's
	# progress, an item's life) and a skip would do nothing on the frames that actually shake.
	_pending_scroll = _scroll.scroll_vertical if visible and _scroll != null else SCROLL_UNSET
	HudWidgets.clear_children(_header)
	HudWidgets.clear_children(_rail)
	HudWidgets.clear_children(_main)
	_popup_anchor = null
	_reconcile_recipe_view(payload)
	_build_header(payload)
	_build_rail(payload)
	_build_bench(payload)
	_build_ledger(payload)
	# **VISIBLE BEFORE THE FIT, and that is load-bearing**: `Container._sort_children` early-returns
	# on a hidden subtree, so a card kept hidden until it had been measured would never lay its
	# content out and would measure the unwrapped lower bound forever.
	visible = true
	# …and on its FIRST mount it shows at its NOMINAL width, so the height read a frame from now is a
	# function of the width the content was actually laid out at. `fit_width(0, 0)` is the base class's
	# way of saying "apply the nominal": with no content measurement it can only resolve to
	# `target_width`. **A card that has already been fitted is left where it is** — it is at a perfectly
	# good width to lay content out at (the width the fit below will most likely settle on again), and
	# snapping it back to the nominal would draw one whole frame at a width the card is about to leave.
	if not has_fitted_width():
		fit_width(0.0, 0.0)
	refit()

func dismiss() -> void:
	visible = false
	_payload = {}
	# A card that is opened again is a fresh reading and opens at the top of the ledger, so nothing
	# survives the dismissal — least of all an offset into a table that has been torn down.
	_pending_scroll = SCROLL_UNSET
	# …and the rank picker is an EXPANSION rather than a preference, so it closes with the card. It is
	# deliberately unlike `_folded`, which survives a dismissal because a reader who has stopped looking
	# at a ledger group means it.
	_priority_open = false
	# …and so are an item's popup and its picker: both are transient readings of one ledger.
	_picker_row = NO_ROW
	_picker_choice = ""
	_close_recipes_popup()
	if _scroll != null:
		_scroll.scroll_vertical = 0
	HudWidgets.clear_children(_header)
	HudWidgets.clear_children(_rail)
	HudWidgets.clear_children(_main)

func is_open() -> bool:
	return visible

## The `PanelContainer` that DRAWS the card. A real Container, so its combined minimum is the honest
## measure of whether the card is holding its content or quietly growing out of itself.
func card() -> PanelContainer:
	return _card

## The recipes popup, or `null` before any link was ever pressed. It is a Window, so a search of the
## card's tree does not reach its content through `card()`.
func recipes_popup() -> PopupPanel:
	return _recipes_popup

## Re-fit to content and re-place. Coalesced across one frame: the content's height is a function of
## the card's width, so a measurement taken in the same frame the body was rebuilt reports the
## PREVIOUS content's wrapping.
func refit() -> void:
	if not visible or _fit_pending or _body == null:
		return
	_fit_pending = true
	await get_tree().process_frame
	_fit_pending = false
	if not visible or _body == null:
		return
	var room := _room()
	var chrome := HudStyle.card_stylebox().get_minimum_size()
	max_width = maxf(room.size.x, target_width)
	fit_width(_body.get_combined_minimum_size().x, chrome.x + _scroll_gutter())
	# **THE HEIGHT FIT'S CEILING IS THE WHOLE ROOM, AND THE CARD DOES NOT MOVE TO BE MEASURED.**
	# `centred_in_room` is how the base class is told so: this card is centred by `_place()` below, so
	# the room it may spend is the room's own height rather than the room beneath wherever it currently
	# sits. Fitting a centred card against the room BELOW it throws away everything above it —
	# measured, a ledger with room for every row was clamped to four of them by exactly the height its
	# own centring had put above it — and the older answer to that, parking the card at the top of the
	# room first, put it there for the whole frame this fit awaits.
	max_height = room.size.y
	fit_to_content(_body.get_combined_minimum_size().y + _header_height(), chrome.y, _scroll)
	# The offset `render` carried across its rebuild, restored now the fit has settled the card's height
	# and `fit_to_content` has decided whether the ledger scrolls at all. **Only into a ledger that
	# still scrolls** — a rebuild whose table now fits its room has nowhere left to be scrolled to, and
	# the fit has just said so by disabling the scroll and returning it to the top. A re-fit that
	# follows no rebuild has nothing pending and leaves the player's scroll where he left it.
	if _pending_scroll != SCROLL_UNSET and _scroll != null:
		if _scroll.vertical_scroll_mode != ScrollContainer.SCROLL_MODE_DISABLED:
			_scroll.scroll_vertical = _pending_scroll
		_pending_scroll = SCROLL_UNSET
	_place()
	# The popup hangs under a link this render rebuilt, so it is re-anchored now the card has settled.
	_sync_recipes_popup()

# ---- header -----------------------------------------------------------------

func _build_header(payload: Dictionary) -> void:
	var title := Label.new()
	title.text = HudCraftingVocab.PANEL_TITLE.to_upper()
	title.add_theme_font_size_override("font_size", HudCraftingVocab.TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK)
	_header.add_child(title)

	# The `Band:` field row — `HudWidgets.build_field_key` + `build_option_picker`, the same pair the
	# compose sheets' own `Band:` row mounts, flanked by the Band/City panel's `◀ n / N ▶` cycler.
	# They answer different questions: the arrows WALK the roster when sweeping every band for worn
	# kit, the dropdown JUMPS when you know which one you want.
	var field := HBoxContainer.new()
	field.add_theme_constant_override("separation", HudCraftingVocab.COLUMN_SEPARATION)
	field.add_child(HudWidgets.build_field_key(HudCraftingVocab.BAND_FIELD_KEY))

	var count := int(payload.get(PAYLOAD_BAND_COUNT, 0))
	var index := int(payload.get(PAYLOAD_BAND_INDEX, 0))
	var prev := _icon_button(HudCraftingVocab.CYCLE_PREV_GLYPH, HudCraftingVocab.CYCLE_PREV_TOOLTIP)
	prev.disabled = count <= 1
	prev.pressed.connect(func() -> void: cycle_requested.emit(-1))
	field.add_child(prev)

	var options: Array = payload.get(PAYLOAD_BAND_OPTIONS, [])
	var entries: Array = []
	for option_variant in options:
		if not (option_variant is Dictionary):
			continue
		var option: Dictionary = option_variant
		var entity := int(option.get("entity", -1))
		entries.append({
			"label": String(option.get("label", "")),
			"on_pick": func() -> void: band_selected.emit(entity),
		})
	var picker := HudWidgets.build_option_picker(entries, maxi(index - 1, 0),
		String(payload.get(PAYLOAD_BAND_LABEL, "")), HudCraftingVocab.BAND_PICKER_TOOLTIP)
	picker.size_flags_horizontal = Control.SIZE_FILL
	picker.custom_minimum_size = Vector2(HudCraftingVocab.BAND_PICKER_MIN_WIDTH, 0.0)
	field.add_child(picker)

	var nxt := _icon_button(HudCraftingVocab.CYCLE_NEXT_GLYPH, HudCraftingVocab.CYCLE_NEXT_TOOLTIP)
	nxt.disabled = count <= 1
	nxt.pressed.connect(func() -> void: cycle_requested.emit(1))
	field.add_child(nxt)

	var count_label := Label.new()
	count_label.text = HudCraftingVocab.CYCLE_COUNT_FORMAT % [index, count]
	count_label.add_theme_font_size_override("font_size", HudCraftingVocab.CRAFT_TRACK_FONT_SIZE)
	count_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	field.add_child(count_label)
	_header.add_child(field)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_header.add_child(spacer)

	var close := _icon_button(HudCraftingVocab.CLOSE_GLYPH, HudCraftingVocab.CLOSE_TOOLTIP)
	close.pressed.connect(func() -> void: closed.emit())
	_header.add_child(close)

# ---- the materials rail -----------------------------------------------------

## **ONE GROUP PER MATERIAL, ONE ROW PER BATCH — and nothing else.** No provenance, no per-turn rate,
## and no catalogue of materials the world does not yet contain: a group appears because the band
## HOLDS some of that material, which is what "On hand" means.
func _build_rail(payload: Dictionary) -> void:
	_rail.add_child(_zone_head(HudCraftingVocab.RAIL_HEAD))
	var band: Dictionary = payload.get(PAYLOAD_BAND, {})
	var batches_by_material := _batches_by_material(band)
	var rendered := 0
	for material_variant in payload.get(PAYLOAD_MATERIALS, []):
		if not (material_variant is Dictionary):
			continue
		var material: Dictionary = material_variant
		var material_id := String(material.get(HudCraftingVocab.MATERIAL_ID_KEY, ""))
		var batches: Array = batches_by_material.get(material_id, [])
		if batches.is_empty():
			continue
		_rail.add_child(_build_material_group(material, batches, payload))
		rendered += 1
	if rendered == 0:
		var empty := Label.new()
		empty.text = HudCraftingVocab.RAIL_EMPTY
		empty.add_theme_font_size_override("font_size", HudCraftingVocab.CRAFT_TRACK_FONT_SIZE)
		empty.add_theme_color_override("font_color", HudStyle.INK_FAINT)
		_rail.add_child(empty)

func _build_material_group(material: Dictionary, batches: Array, payload: Dictionary) -> Control:
	var group := VBoxContainer.new()
	group.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)
	var pad := MarginContainer.new()
	pad.add_theme_constant_override("margin_top", HudCraftingVocab.RAIL_GROUP_PADDING_V)
	pad.add_theme_constant_override("margin_bottom", HudCraftingVocab.RAIL_GROUP_PADDING_V)
	pad.add_child(group)

	# **A MATERIAL HEADS THE GROUP AND ITS CRAFT RIDES THE HEADER.** Fibre → Weaving, Hide → Tanning,
	# Bone → Bone-working: the rail IS the track list, so there is no second surface to build and no
	# way for the two to disagree about what a material is.
	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", HudCraftingVocab.COLUMN_SEPARATION)
	var name_label := Label.new()
	name_label.text = String(material.get(HudCraftingVocab.MATERIAL_ID_KEY, "")).capitalize().to_upper()
	name_label.add_theme_font_size_override("font_size", HudCraftingVocab.MATERIAL_NAME_FONT_SIZE)
	name_label.add_theme_color_override("font_color", HudStyle.INK)
	head.add_child(name_label)
	var head_spacer := Control.new()
	head_spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head_spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	head.add_child(head_spacer)
	var track := _craft_track_label(String(material.get(HudCraftingVocab.MATERIAL_CRAFT_KEY, "")), payload)
	if track != null:
		head.add_child(track)
	group.add_child(head)

	for batch_variant in batches:
		if batch_variant is Dictionary:
			group.add_child(_build_batch_row(batch_variant, payload))
	return pad

## The craft's meter and how far along it is — `▰▰▱▱▱ Tanning · 41%`, or `· known` once it is learned.
## The denominator is the PUBLISHED `completion_threshold`, never a scale invented here, and the
## craft's name is the sim's own `display_name` — the client never maps a craft id to English.
func _craft_track_label(craft_id: String, payload: Dictionary) -> Label:
	if craft_id == "":
		return null
	var track := _craft_track_of(craft_id, payload)
	if track.is_empty():
		return null
	var threshold := float(track.get(HudCraftingVocab.CRAFT_KNOWLEDGE_THRESHOLD_KEY, 0.0))
	var progress := float(track.get(HudCraftingVocab.CRAFT_KNOWLEDGE_PROGRESS_KEY, 0.0))
	var known := bool(track.get(HudCraftingVocab.CRAFT_KNOWLEDGE_KNOWN_KEY, false))
	var fraction := clampf(progress / threshold, 0.0, 1.0) if threshold > 0.0 else 0.0
	if known:
		fraction = 1.0
	var label := Label.new()
	label.text = HudCraftingVocab.CRAFT_TRACK_FORMAT % [
		HudFormat.meter_bar(fraction * HudConst.PROGRESS_PERCENT_SCALE,
			HudCraftingVocab.CRAFT_TRACK_METER_CELLS),
		String(track.get(HudCraftingVocab.CRAFT_KNOWLEDGE_DISPLAY_NAME_KEY, "")),
		HudCraftingVocab.CRAFT_TRACK_KNOWN if known
			else HudCraftingVocab.CRAFT_TRACK_PROGRESS_FORMAT % HudFormat.progress_percent(fraction),
	]
	label.add_theme_font_size_override("font_size", HudCraftingVocab.CRAFT_TRACK_FONT_SIZE)
	label.add_theme_color_override("font_color",
		HudStyle.SIGNAL if known else HudStyle.INK_FAINT)
	return label

## One BATCH: how much of it there is, and how each of the material's axes rates. **THE BAND RATES
## THE AXIS, NOT THE MATERIAL** — `tough: excellent · supple: poor` is a mammoth hide, excellent at
## being tough, which is right for a sled and wrong for cordage.
func _build_batch_row(batch: Dictionary, payload: Dictionary) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudCraftingVocab.COLUMN_SEPARATION)
	var amount := Label.new()
	amount.text = HudCraftingVocab.BATCH_AMOUNT_FORMAT % float(batch.get(HudCraftingVocab.BATCH_AMOUNT_KEY, 0.0))
	amount.add_theme_font_size_override("font_size", HudCraftingVocab.BATCH_AMOUNT_FONT_SIZE)
	amount.add_theme_color_override("font_color", HudStyle.INK)
	amount.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	row.add_child(amount)

	var chips := HFlowContainer.new()
	chips.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for reading_variant in batch.get(HudCraftingVocab.BATCH_READINGS_KEY, []):
		if not (reading_variant is Dictionary):
			continue
		var reading: Dictionary = reading_variant
		var band_name := String(reading.get(HudCraftingVocab.READING_BAND_NAME_KEY, ""))
		var chip := _chip(HudCraftingVocab.CHARACTERISTIC_CHIP_FORMAT % [
			String(reading.get(HudCraftingVocab.READING_AXIS_KEY, "")), band_name],
			_band_name_color(band_name, payload), HudCraftingVocab.CHIP_FONT_SIZE)
		chips.add_child(chip)
	row.add_child(chips)
	return row

## Where a band name sits in the shared vocabulary decides how its chip reads: the TOP rung is a
## strength, the BOTTOM a weakness, everything between stays quiet. Both ends come from the published
## legend rather than from a cut point typed here — the sim owns those, and a client with its own
## would disagree with the word beside them.
func _band_name_color(band_name: String, payload: Dictionary) -> Color:
	var legend: Array = payload.get(PAYLOAD_BAND_LEGEND, [])
	if legend.is_empty() or band_name == "":
		return HudCraftingVocab.CHIP_NEUTRAL_COLOR
	var first := ""
	var last := ""
	if legend[0] is Dictionary:
		first = String((legend[0] as Dictionary).get(HudCraftingVocab.BAND_LEGEND_NAME_KEY, ""))
	if legend[legend.size() - 1] is Dictionary:
		last = String((legend[legend.size() - 1] as Dictionary).get(HudCraftingVocab.BAND_LEGEND_NAME_KEY, ""))
	if band_name == last:
		return HudCraftingVocab.CHIP_HIGH_COLOR
	if band_name == first:
		return HudCraftingVocab.CHIP_LOW_COLOR
	return HudCraftingVocab.CHIP_NEUTRAL_COLOR

# ---- the bench --------------------------------------------------------------

## **WHAT IS BEING MADE, HOW FAR ALONG IT IS, WHY IT IS STOPPED, WHO IS ON IT AND WHAT IT TEACHES.**
## Each of those is its own line, so none of them can cost the panel another. An idle bench says
## so in as many words — `""` for `recipe_id` is IDLE, which is a different statement from a bench
## that has a recipe and a `blocked_reason`.
func _build_bench(payload: Dictionary) -> void:
	var band: Dictionary = payload.get(PAYLOAD_BAND, {})
	var bench: Dictionary = band.get(HudCraftingVocab.BAND_BENCH_KEY, {})
	var section := VBoxContainer.new()
	section.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)
	section.add_child(_zone_head(HudCraftingVocab.BENCH_HEAD))

	var well := PanelContainer.new()
	well.add_theme_stylebox_override("panel", HudStyle.readout_stylebox())
	var inner := VBoxContainer.new()
	inner.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)
	well.add_child(inner)

	var recipe_id := String(bench.get(HudCraftingVocab.BENCH_RECIPE_ID_KEY, ""))
	var top := HBoxContainer.new()
	top.add_theme_constant_override("separation", HudCraftingVocab.HEADER_SEPARATION)
	var words := VBoxContainer.new()
	words.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	words.add_theme_constant_override("separation", 0)

	var title := Label.new()
	title.add_theme_font_size_override("font_size", HudCraftingVocab.BENCH_TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK)
	var sub := Label.new()
	sub.add_theme_font_size_override("font_size", HudCraftingVocab.BENCH_SUB_FONT_SIZE)
	sub.add_theme_color_override("font_color", HudStyle.INK_DIM)
	var blocked := ""
	var blocked_severity := ""
	if recipe_id == "":
		title.text = HudCraftingVocab.BENCH_IDLE_TITLE
		sub.text = HudCraftingVocab.BENCH_IDLE_SUB
	else:
		var recipe := _recipe_of(recipe_id, payload)
		var craft_name := _craft_display_name(String(recipe.get(HudCraftingVocab.MATERIAL_CRAFT_KEY, "")), payload)
		var made := String(bench.get(HudCraftingVocab.BENCH_DISPLAY_NAME_KEY, recipe_id))
		# **THE THING, THEN THE CRAFT** — and the craft only when the sim published one, so an
		# unresolved track prints `Hurdles` rather than a title opening on a separator.
		title.text = made if craft_name == "" \
			else HudCraftingVocab.BENCH_TITLE_FORMAT % [made, craft_name]
		sub.text = _bench_sub_line(bench)
		blocked = String(bench.get(HudCraftingVocab.BENCH_BLOCKED_REASON_KEY, ""))
		blocked_severity = String(bench.get(HudCraftingVocab.BENCH_BLOCKED_SEVERITY_KEY, ""))
	words.add_child(title)
	# **THE RANK LEADS LINE TWO, exactly as a worked row's does** (`docs/plan_standing_upkeep.md` §4.9
	# item 9b) — `High priority · ` / `Low priority · ` in the tier's ink, and a Normal bench prints
	# NOTHING, its line two staying byte-identical to what it printed before the mark existed.
	#
	# **A SEPARATE `Label`, not a splice into the sub line's string.** The sub line is composed by
	# `_bench_sub_line` out of the job's own clauses, and a prefix spliced in would sit inside a string
	# that producer measures and joins; its own node keeps the two apart, which is `BandPanelController`'s
	# rule for the same mark one panel over. **The prefix carries its own separator**, so the row's
	# separation is zero and line two's spacing is stated in one place rather than half in a string.
	#
	# It renders on an IDLE bench too — the rank is a standing statement about the BENCH rather than
	# about the job on it, which is exactly when a player says *"the axes go first"*.
	var priority := HudWorkVocab.work_priority_of(
		bench.get(HudCraftingVocab.BENCH_PRIORITY_KEY, HudWorkVocab.WORK_PRIORITY_NORMAL))
	var line_two := HBoxContainer.new()
	# ZERO, and for `WORK_ROW_PRIORITY_SEPARATION`'s reason: the prefix carries its own ` · ` inside
	# its text, so line two's spacing is stated in one place instead of half in a string and half in a
	# container constant. The gap before the link is the EXPANDING sub line, not a separation.
	line_two.add_theme_constant_override("separation", HudWorkVocab.WORK_ROW_PRIORITY_SEPARATION)
	var prefix_text := HudWorkVocab.work_row_priority_prefix(priority)
	if prefix_text != "":
		var prefix := Label.new()
		prefix.text = prefix_text
		prefix.add_theme_font_size_override("font_size", HudCraftingVocab.BENCH_SUB_FONT_SIZE)
		prefix.add_theme_color_override("font_color", HudWorkVocab.work_priority_ink(priority))
		# Found by IDENTITY and valued the LEVEL — the face is a `HudWorkVocab` string this panel does
		# not compose, so an assertion matching on its wording would be matching the other module's
		# spelling.
		prefix.set_meta(HudCraftingVocab.BENCH_PRIORITY_META, priority)
		line_two.add_child(prefix)
	sub.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line_two.add_child(sub)
	# **AND THE CONTROL RIDES THE SAME LINE, WHICH IS WHAT MAKES IT FREE.** A row of its own cost the
	# well ~24px on EVERY bench, forever, and the collapsed-band-dock state had less headroom than
	# that: `crafting_panel_band_dock_collapsed`'s ledger fitted its 1072px room with under one row to
	# spare, and a fourth line in this well pushed it into an internal scroll it is asserted not to
	# need. Line two always exists, so hanging the link off its trailing edge spends nothing — and the
	# rank is line two's subject anyway, the mark leading the very same line.
	line_two.add_child(_build_priority_link())
	words.add_child(line_two)
	# **HOW FAR ALONG AND WHY STOPPED ARE TWO FACTS, AND NEITHER MAY EAT THE OTHER.** The refusal is
	# the OFFER vocabulary plus the crew's own, resolved sim-side and rendered VERBATIM — a bench with
	# a full pile and nobody on it is stopped too. It gets its own line UNDER the progress line rather
	# than replacing it: the banked work is what says whether clearing the block recovers a
	# nearly-finished item or a barely-started one, which is the question a stopped bench raises. A
	# bench that is running adds no label at all, so nothing about its card moves.
	#
	# **AND WHICH REFUSALS ARE SERIOUS IS THE SIM'S ANSWER, NOT THIS PANEL'S.** A bench waiting for
	# its crew is the NORMAL state one click after Make — the player staffs the bench — so it arrives
	# `neutral` and reads in the quiet ink, while a shortage or an unknown craft arrives `danger` and
	# keeps the alarm. Resolved through the SAME `REASON_COLORS` table the ledger's offer rows use, the
	# published `blocked_severity` being that same vocabulary; tinting every reason alike is what made
	# the expected state read as a fault.
	if blocked != "":
		var reason := Label.new()
		reason.text = blocked
		reason.add_theme_font_size_override("font_size", HudCraftingVocab.BENCH_BLOCKED_FONT_SIZE)
		reason.add_theme_color_override("font_color", HudCraftingVocab.REASON_COLORS.get(
			blocked_severity, HudCraftingVocab.REASON_COLOR_QUIET))
		# Found by IDENTITY rather than by its face — the reason is a sim string this panel cannot
		# predict, so a claim about the blocked line can only be scoped to the node that carries it.
		reason.set_meta(HudCraftingVocab.BENCH_BLOCKED_META, true)
		words.add_child(reason)
	top.add_child(words)
	# **NOTHING TO CLEAR ON AN IDLE BENCH**, so the control is absent rather than dead — and it is
	# built BEFORE the stepper, which insets it from the card's right edge and keeps it away from the
	# header's own ✕. See `_build_clear_button`.
	if recipe_id != "":
		top.add_child(_build_clear_button(bench))
	top.add_child(_build_crew_stepper(bench, payload, recipe_id != ""))
	inner.add_child(top)

	var work := float(bench.get(HudCraftingVocab.BENCH_WORK_KEY, 0.0))
	var progress := float(bench.get(HudCraftingVocab.BENCH_PROGRESS_KEY, 0.0))
	var fraction := clampf(progress / work, 0.0, 1.0) if work > 0.0 else 0.0
	inner.add_child(_bar(fraction, HudStyle.SIGNAL, HudCraftingVocab.BENCH_BAR_HEIGHT))

	var teaches := String(bench.get(HudCraftingVocab.BENCH_TEACHES_KEY, ""))
	if recipe_id != "" and teaches != "":
		var teach := Label.new()
		# **CRAFTING IS THE FOURTH TEACHER** — the lesson is charged per ITEM COMPLETED, so this is
		# what the bench is buying besides the thing itself.
		teach.text = HudCraftingVocab.BENCH_TEACH_FORMAT % _craft_display_name(teaches, payload)
		teach.add_theme_font_size_override("font_size", HudCraftingVocab.BENCH_TEACH_FONT_SIZE)
		teach.add_theme_color_override("font_color", HudStyle.INK_FAINT)
		inner.add_child(teach)

	if _priority_open:
		# **REUSED, NEVER FORKED.** `build_work_priority_picker` takes the pick as a `Callable` and the
		# lit rung as the level itself, so it needs nothing of the work board's — and one builder is what
		# keeps the bench's rank and a worked row's rank from growing two faces for one property.
		inner.add_child(HudWidgets.build_work_priority_picker(func(level: String) -> void:
			_commit_priority(level), priority))

	section.add_child(well)
	_main.add_child(section)

## The bench's second line, in the units the sim keeps the job in: the recipe's own `work` accrued
## against the pass's cost, then what a turn adds and when that finishes it, then what the job has
## already delivered and the grade the pile in flight fixed.
##
## **THE UNIT IS `work`, AND THAT RENAME IS THE POINT OF THE OTHER TWO CLAUSES.** It read
## `worker-turns`, and a player with two crafters divided 6 by 2, expected three turns and measured
## six: a worker-turn is not what a worker does in a turn, because bare-handed `craft_speed` is 0.5.
## So the rate is stated rather than left to be inferred — **VERBATIM from `rate_per_turn`, never
## re-derived**, that product being the tool-or-bare-hand join the sim owns — and the turns it implies
## are stated beside it.
##
## **BOTH NEW CLAUSES ARE GATED ON A RATE ABOVE ZERO *AND* AN EMPTY REFUSAL, WHICH ARE TWO DIFFERENT
## QUESTIONS.** The rate is a property of the crew and the tool: a bench short of material publishes
## its real, non-zero rate — the crew and the tool are fine, it simply has not drawn — while
## `blocked_reason` is what says whether the bench is actually moving. Quoting *"done in 3 turns"*
## beside *"Short 0.6 fibre"* would promise progress that is not happening. A bench with no crew or no
## craft speed publishes `0` instead, and then there is nothing to compute either.
func _bench_sub_line(bench: Dictionary) -> String:
	var work := float(bench.get(HudCraftingVocab.BENCH_WORK_KEY, 0.0))
	var progress := float(bench.get(HudCraftingVocab.BENCH_PROGRESS_KEY, 0.0))
	var parts: Array[String] = [HudCraftingVocab.BENCH_PROGRESS_FORMAT % [progress, work]]
	var rate := float(bench.get(HudCraftingVocab.BENCH_RATE_PER_TURN_KEY, 0.0))
	var blocked := String(bench.get(HudCraftingVocab.BENCH_BLOCKED_REASON_KEY, ""))
	if rate > 0.0 and blocked == "":
		parts.append(HudCraftingVocab.BENCH_RATE_FORMAT % rate)
		parts.append(_bench_estimate_clause(work - progress, rate))
	var completed := int(bench.get(HudCraftingVocab.BENCH_ITEMS_COMPLETED_KEY, 0))
	if completed > 0:
		parts.append(HudCraftingVocab.BENCH_ITEMS_COMPLETED_FORMAT % completed)
	var grade := String(bench.get(HudCraftingVocab.BENCH_OUTPUT_GRADE_KEY, ""))
	if grade != "":
		parts.append(HudCraftingVocab.BENCH_GRADE_FORMAT % grade)
	return HudCraftingVocab.BENCH_SUB_SEPARATOR.join(parts)

## **`ceil(remaining / rate)`, FLOORED AT ONE TURN.** The arithmetic is the client's deliberately —
## it is exact over three numbers the wire already carries, which is where the sim's forecast rule
## puts the split, and a turns-remaining field beside the rate would be a second home for one fact. A
## bench whose pass is already covered still needs a turn to finish it, so the floor states the next
## turn rather than a completion that has not happened.
func _bench_estimate_clause(remaining: float, rate: float) -> String:
	var turns := maxi(int(ceilf(remaining / rate)), HudCraftingVocab.BENCH_ESTIMATE_MIN_TURNS)
	if turns == HudCraftingVocab.BENCH_ESTIMATE_MIN_TURNS:
		return HudCraftingVocab.BENCH_ESTIMATE_NEXT_TURN
	return HudCraftingVocab.BENCH_ESTIMATE_FORMAT % turns

## **THE WAY OFF THE BENCH THAT IS NOT "MAKE SOMETHING ELSE".** Until this existed the only exit was
## pressing Make on another row, which silently spends the committed pile; `clear_bench` has been a
## complete sim verb the whole time with nothing here to emit it.
##
## **THE TOOLTIP NAMES WHAT IT DESTROYS, off the published `drawn_inputs`** — the amounts the store
## really lost, never the recipe's inputs, which differ from the withdrawal the moment a bench tool's
## material efficiency applies. A bench that has drawn nothing says so instead of listing nothing.
## There is no confirmation dialog: the cost is stated in text, the loss is small and recoverable, and
## saying consequences rather than popping a modal is this panel's idiom throughout.
##
## **IT IS THE `armed` VARIANT AND IT SITS LEFT OF THE STEPPER, both to keep it apart from the card
## header's ✕**, which wears the same glyph for a completely different act. Inside the well's own
## bordered box, inset from the card's right edge by the stepper's width, and drawn in the
## destructive treatment rather than the header's quiet ghost.
func _build_clear_button(bench: Dictionary) -> Control:
	var button := Button.new()
	button.text = HudCraftingVocab.CLEAR_BENCH_GLYPH
	button.tooltip_text = _clear_bench_tooltip(bench)
	button.focus_mode = Control.FOCUS_NONE
	button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	button.custom_minimum_size = Vector2(HudCraftingVocab.ICON_BUTTON_SIZE,
		HudCraftingVocab.ICON_BUTTON_SIZE)
	button.add_theme_font_size_override("font_size", HudCraftingVocab.CHIP_FONT_SIZE)
	HudStyle.apply_button(button, "armed")
	# Found by identity rather than by face — the header's close button is the same glyph.
	button.set_meta(HudCraftingVocab.CLEAR_BENCH_META, true)
	button.pressed.connect(func() -> void: clear_bench_requested.emit())
	return button

## The pile a clear would spend, in the cost cell's own clause shape. Empty `drawn_inputs` is an
## undrawn bench, which is a statement rather than a missing list.
func _clear_bench_tooltip(bench: Dictionary) -> String:
	var clauses: Array[String] = []
	for input_variant in bench.get(HudCraftingVocab.BENCH_DRAWN_INPUTS_KEY, []):
		if not (input_variant is Dictionary):
			continue
		var input: Dictionary = input_variant
		clauses.append(HudCraftingVocab.CLEAR_BENCH_CLAUSE_FORMAT % [
			_amount_text(float(input.get(HudCraftingVocab.DRAWN_INPUT_AMOUNT_KEY, 0.0))),
			String(input.get(HudCraftingVocab.DRAWN_INPUT_MATERIAL_ID_KEY, ""))])
	if clauses.is_empty():
		return HudCraftingVocab.CLEAR_BENCH_TOOLTIP_NOTHING
	return HudCraftingVocab.CLEAR_BENCH_TOOLTIP_FORMAT % HudCraftingVocab.CLEAR_BENCH_SEPARATOR.join(clauses)

## **THE `Priority` LINK — the work inspector's own word, its own builder, one panel over.** It wears
## `HudWorkVocab.WORK_INSPECT_PRIORITY` rather than a spelling of its own: the two links open the SAME
## `build_work_priority_picker` over the same three levels, and a second word for one control kind is
## free to drift. Reached by meta, since a face search would find a control in two panels.
func _build_priority_link() -> Button:
	var link := HudWidgets.build_inline_link(HudWorkVocab.WORK_INSPECT_PRIORITY, HudStyle.INK,
		func() -> void: _toggle_priority_picker())
	link.set_meta(HudCraftingVocab.BENCH_PRIORITY_LINK_META, true)
	return link

## Show or hide the rank picker, and re-render against the payload already held — the `_toggle_fold`
## idiom, for its reason: this card is rebuilt on every snapshot anyway, so a hidden subtree waiting to
## be shown would be a second representation of one bit.
func _toggle_priority_picker() -> void:
	_priority_open = not _priority_open
	if not _payload.is_empty():
		render(_payload)

## **THE RANK COMMAND** — `bench_priority <faction> <band> high|normal|low`.
##
## ⛔ **NO OPTIMISTIC OVERLAY AND THEREFORE NO ROLLBACK HANDLE**, which is `work_priority`'s rule one
## surface over: `BenchState.priority` is captured LIVE off the bench the command mutates and the
## server re-captures after every command, so the new mark arrives on THIS command's own recapture. A
## client-side pending copy would be a second statement of one value.
##
## The picker CLOSES on the pick, exactly as the work inspector's does: the well has said its piece,
## and a picker left open over a value that has not landed yet invites a second press at one button.
func _commit_priority(level: String) -> void:
	_priority_open = false
	bench_priority_requested.emit(HudWorkVocab.work_priority_of(level))
	if not _payload.is_empty():
		render(_payload)

## The `− n +` crew stepper. **It spends the same pool `assign_labor` does** — a crew at the bench is
## not gathering — so `+` greys out at the ceiling rather than sending a command the sim will clamp.
##
## **THE IDLE COUNT NETS THE BENCH OUT, SO THE CREW IS ADDED BACK TO REACH THIS CEILING.**
## `PopulationCohortState.idleWorkers` is `working_age − assigned − bench.workers` (the sim's
## `BandWorkforce::idle()`, the same seam `assign_labor` clamps against), and `effective_idle`
## subtracts the crew for the same reason — a worker at the bench is assigned labor. But re-crewing
## does not have to free those hands first: the crew already standing at the bench stays put while
## the job is swapped, which is `BandWorkforce::benchable()`. The payload therefore carries
## `idle + bench.workers`, and capping the stepper at idle alone would pin it to the crew on it.
func _build_crew_stepper(bench: Dictionary, payload: Dictionary, running: bool) -> Control:
	var column := VBoxContainer.new()
	column.alignment = BoxContainer.ALIGNMENT_CENTER
	column.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)

	var workers := int(bench.get(HudCraftingVocab.BENCH_WORKERS_KEY, 0))
	var ceiling := maxi(int(payload.get(PAYLOAD_IDLE_WORKERS, 0)), workers)
	var stepper := HBoxContainer.new()
	stepper.alignment = BoxContainer.ALIGNMENT_CENTER
	stepper.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)

	var minus := _crew_button(HudCraftingVocab.BENCH_CREW_DECREMENT)
	minus.disabled = not running or workers <= 0
	minus.pressed.connect(func() -> void: crew_changed.emit(workers - 1))
	stepper.add_child(minus)

	var count := Label.new()
	count.text = str(workers)
	count.add_theme_font_size_override("font_size", HudCraftingVocab.CREW_COUNT_FONT_SIZE)
	count.add_theme_color_override("font_color", HudStyle.INK)
	count.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	count.custom_minimum_size = Vector2(HudCraftingVocab.CREW_BUTTON_SIZE, 0.0)
	stepper.add_child(count)

	var plus := _crew_button(HudCraftingVocab.BENCH_CREW_INCREMENT)
	plus.disabled = not running or workers >= ceiling
	plus.pressed.connect(func() -> void: crew_changed.emit(workers + 1))
	stepper.add_child(plus)
	column.add_child(stepper)

	var caption := Label.new()
	caption.text = HudCraftingVocab.BENCH_CREW_CAPTION.to_upper()
	caption.add_theme_font_size_override("font_size", HudCraftingVocab.CREW_CAPTION_FONT_SIZE)
	caption.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	caption.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	column.add_child(caption)
	return column

# ---- the ledger -------------------------------------------------------------

## **ONE TABLE IN FOLDABLE SECTIONS, ONE ROW PER ITEM, AND EVERY ROW IS A JOIN.** `CraftOffer.
## outputItemId` is the key twice over: it groups an item's offers into ONE row, and it joins that row
## onto `equipment_batches` for the grades and the counts. The suggested offer supplies the row's
## cost and refusal. Neither the offers nor the batches can answer alone — which is why the
## ledger is built here rather than off either array on its own.
func _build_ledger(payload: Dictionary) -> void:
	var band: Dictionary = payload.get(PAYLOAD_BAND, {})
	var batches_by_item := _equipment_by_item(band)
	# **THE SAME GROUPING THE RAIL BUILDS, BUILT ONCE** — the Owned cell of a stock row reports the
	# band's pile of the material that row makes, which is the rail's own fact read from the ledger.
	var batches_by_material := _batches_by_material(band)

	var table := VBoxContainer.new()
	table.add_theme_constant_override("separation", 0)
	table.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	table.add_child(_build_column_heads())
	table.add_child(_rule(HudStyle.LINE))

	for section in _ledger_sections(band):
		var head_name := String(section["head"])
		table.add_child(_build_group_head(head_name))
		table.add_child(_rule(HudStyle.LINE))
		if _is_folded(head_name):
			# A folded group keeps its head and hides its rows — the way back is the caret, which is
			# what a head buys that a column never could.
			continue
		# **SORTED BY URGENCY — worn first, untouched last.** The player's real question is "what am I
		# about to lose?", so the ledger opens on the answer; a full-life row is DIMMED to the bottom
		# rather than hidden, because a kit you own and never use is information too.
		var rows: Array = section["rows"]
		rows.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
			return _urgency_key(a["offer"], batches_by_item) < _urgency_key(b["offer"], batches_by_item))
		for row in rows:
			table.add_child(_build_ledger_row(row, batches_by_item, batches_by_material, payload))
			# **THE PICKER OPENS DIRECTLY UNDER ITS ROW, INSIDE THE LEDGER**, spanning the table's width —
			# the choice sits beside the thing being chosen for, and it pushes the rows below it down
			# rather than floating over them.
			if String(row["key"]) == _picker_row:
				table.add_child(_build_make_picker(row, payload))
			# A hairline UNDER each row rather than separation between them: the rule is what makes a
			# four-column row read across, and separation alone leaves four stacks side by side.
			table.add_child(_rule(HudStyle.LINE_SOFT))
	_main.add_child(table)

## **THE SECTIONS, IN THE ORDER THEY RENDER: `Kit`, `Bench tools`, `Materials`** — the three
## published groups, one head each. Each section holds ROWS, not offers (`_ledger_rows`).
func _ledger_sections(band: Dictionary) -> Array:
	var by_group := {}
	for row in _ledger_rows(band):
		var group := String(row["group"])
		if not by_group.has(group):
			by_group[group] = []
		by_group[group].append(row)
	var sections: Array = []
	for group in HudCraftingVocab.GROUP_ORDER:
		var rows: Array = by_group.get(group, [])
		if rows.is_empty():
			continue
		sections.append({"head": String(HudCraftingVocab.GROUP_HEADS[group]), "rows": rows})
	return sections

## **ONE ROW PER ITEM.** The band's offers grouped by `outputItemId` — a spear pointed with bone and a
## spear knapped from stone are one row — and a MATERIAL recipe (empty `outputItemId`) is a row of its
## own keyed by its recipe. Rows come out in the order each item's FIRST offer appears on the wire.
##
## Each row is `{key, group, name, offers, offer}`: `offer` is the SUGGESTED one — the sim marks exactly
## one per row — and it is what the row's cost and refusal are read off. The row's name is
## that offer's `displayName`, which the sim publishes as the ITEM's name on every one of its offers.
func _ledger_rows(band: Dictionary) -> Array:
	var rows: Array = []
	var by_key := {}
	for offer_variant in band.get(HudCraftingVocab.BAND_CRAFT_OFFERS_KEY, []):
		if not (offer_variant is Dictionary):
			continue
		var offer: Dictionary = offer_variant
		var key := _row_key_of(offer)
		if not by_key.has(key):
			var row := {
				"key": key,
				"group": String(offer.get(HudCraftingVocab.OFFER_GROUP_KEY, "")),
				"offers": [],
				"offer": offer,
			}
			by_key[key] = row
			rows.append(row)
		var held: Dictionary = by_key[key]
		held["offers"].append(offer)
		if bool(offer.get(HudCraftingVocab.OFFER_SUGGESTED_KEY, false)):
			held["offer"] = offer
	return rows

## The row an offer belongs to: its item, or — for a recipe that makes a material — its own recipe.
func _row_key_of(offer: Dictionary) -> String:
	var item_id := String(offer.get(HudCraftingVocab.OFFER_OUTPUT_ITEM_ID_KEY, ""))
	if item_id != "":
		return item_id
	return HudCraftingVocab.LEDGER_STOCK_ROW_KEY_FORMAT % String(
		offer.get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, ""))

## The row named `key` in the payload's band, `{}` when it is gone.
func _row_in(payload: Dictionary, key: String) -> Dictionary:
	if key == NO_ROW:
		return {}
	for row in _ledger_rows(payload.get(PAYLOAD_BAND, {})):
		if String(row["key"]) == key:
			return row
	return {}

func _row_on_bench(row: Dictionary) -> bool:
	for offer in row["offers"]:
		if bool((offer as Dictionary).get(HudCraftingVocab.OFFER_ON_BENCH_KEY, false)):
			return true
	return false

func _row_available(row: Dictionary) -> bool:
	for offer in row["offers"]:
		if bool((offer as Dictionary).get(HudCraftingVocab.OFFER_AVAILABLE_KEY, false)):
			return true
	return false

## **THE RECIPE THE PICKER OPENS ON**: the suggested one when it can be made now, else the first that
## can, else nothing. The sim's own pick already prefers an available recipe, so the fallback matters
## only when the suggestion and the store disagree.
func _default_choice(row: Dictionary) -> String:
	var suggested: Dictionary = row["offer"]
	if bool(suggested.get(HudCraftingVocab.OFFER_AVAILABLE_KEY, false)):
		return String(suggested.get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, ""))
	for offer in row["offers"]:
		if bool((offer as Dictionary).get(HudCraftingVocab.OFFER_AVAILABLE_KEY, false)):
			return String((offer as Dictionary).get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, ""))
	return ""

func _choice_is_available(row: Dictionary, recipe_id: String) -> bool:
	for offer in row["offers"]:
		var candidate: Dictionary = offer
		if String(candidate.get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, "")) == recipe_id:
			return bool(candidate.get(HudCraftingVocab.OFFER_AVAILABLE_KEY, false))
	return false

## **THE VIEW STATE IS CHECKED AGAINST EVERY NEW PAYLOAD BEFORE IT IS DRAWN.** A picker whose row has
## gone, or whose item has gone on the bench, closes — there is nothing left to choose; a chosen
## recipe that is no longer available falls back to the default rule rather than leaving Start
## pointed at a build the sim would refuse; a popup whose row is gone or has one recipe left closes.
func _reconcile_recipe_view(payload: Dictionary) -> void:
	if _picker_row != NO_ROW:
		var row := _row_in(payload, _picker_row)
		if row.is_empty() or (row["offers"] as Array).size() < 2 or _row_on_bench(row):
			_picker_row = NO_ROW
			_picker_choice = ""
		elif not _choice_is_available(row, _picker_choice):
			_picker_choice = _default_choice(row)
	if _popup_row != NO_ROW:
		var popped := _row_in(payload, _popup_row)
		if popped.is_empty() or (popped["offers"] as Array).size() < 2:
			_close_recipes_popup()

func _is_folded(head_name: String) -> bool:
	return bool(_folded.get(head_name, false))

## The sort key: the PUBLISHED life severity first (worn before running-down before comfortable),
## then how much condition is left, so a spent row leads its severity band. A row with no equipment
## behind it (a stock recipe) sorts after everything with condition to report.
##
## **CONDITION STILL DECIDES THE ORDER THOUGH THE LEDGER PRINTS NONE OF IT.** The player's question
## here is "what does it cost to rebuild", and the rows worth rebuilding first are the worn ones — so
## `life_severity` and `remaining` are read as a RANKING, which is a different use from the readout
## the role cards own and does not restate it anywhere on screen.
func _urgency_key(offer: Dictionary, batches_by_item: Dictionary) -> float:
	var batch := _batch_for(offer, batches_by_item)
	if batch.is_empty():
		return float(HudCraftingVocab.LIFE_SEVERITY_RANK_UNKNOWN) * HudConst.PROGRESS_PERCENT_SCALE
	var rank: int = HudCraftingVocab.LIFE_SEVERITY_RANK.get(
		String(batch.get(HudCraftingVocab.EQUIPMENT_LIFE_SEVERITY_KEY, "")),
		HudCraftingVocab.LIFE_SEVERITY_RANK_UNKNOWN)
	return float(rank) * HudConst.PROGRESS_PERCENT_SCALE \
		+ clampf(float(batch.get(HudCraftingVocab.EQUIPMENT_REMAINING_KEY, 0.0)), 0.0,
			HudConst.PROGRESS_PERCENT_SCALE)

func _build_column_heads() -> Control:
	var row := _ledger_row_container()
	var heads := [HudCraftingVocab.LEDGER_COLUMN_ITEM, HudCraftingVocab.LEDGER_COLUMN_OWNED,
		HudCraftingVocab.LEDGER_COLUMN_COST, HudCraftingVocab.LEDGER_COLUMN_ACTION]
	var widths := [0.0, HudCraftingVocab.COLUMN_OWNED_WIDTH, HudCraftingVocab.COLUMN_COST_WIDTH,
		HudCraftingVocab.COLUMN_ACTION_WIDTH]
	for i in range(heads.size()):
		var label := Label.new()
		label.text = String(heads[i]).to_upper()
		label.add_theme_font_size_override("font_size", HudCraftingVocab.COLUMN_HEAD_FONT_SIZE)
		label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
		row.add_child(_column_cell(label, float(widths[i]), i == 0))
	return row

## **ONE HEAD BUILDER FOR ALL THREE SECTIONS — `Kit`, `Bench tools`, `Materials` — so they read as
## one family.** A caret leads it and the whole head is the
## click target, which is why it is a `Button` stripped of its chrome rather than a Label with a
## button beside it: a head that only responded on its glyph would be a head you have to aim at.
##
## The folded head DIMS. It is still there and still says what it is hiding, which is the difference
## between folding a group away and losing it.
func _build_group_head(head_name: String) -> Control:
	var host := MarginContainer.new()
	host.add_theme_constant_override("margin_top", HudCraftingVocab.GROUP_HEAD_TOP_MARGIN)
	var folded := _is_folded(head_name)
	var head := Button.new()
	head.text = (HudCraftingVocab.GROUP_HEAD_FORMAT % [
		HudCraftingVocab.GROUP_HEAD_CARET_FOLDED if folded else HudCraftingVocab.GROUP_HEAD_CARET_OPEN,
		head_name]).to_upper()
	head.flat = true
	head.focus_mode = Control.FOCUS_NONE
	head.alignment = HORIZONTAL_ALIGNMENT_LEFT
	head.add_theme_stylebox_override("normal", HudStyle.empty_stylebox())
	head.add_theme_stylebox_override("hover", HudStyle.empty_stylebox())
	head.add_theme_stylebox_override("pressed", HudStyle.empty_stylebox())
	head.add_theme_stylebox_override("focus", HudStyle.empty_stylebox())
	head.add_theme_font_size_override("font_size", HudCraftingVocab.GROUP_HEAD_FONT_SIZE)
	head.add_theme_color_override("font_color",
		HudStyle.INK_FAINT if folded else HudStyle.INK_DIM)
	head.add_theme_color_override("font_hover_color", HudStyle.INK)
	head.add_theme_color_override("font_pressed_color", HudStyle.INK)
	# Reached by IDENTITY, never by its face — the `HudWidgets.POLICY_RUNG_META` idiom. The meta is the
	# head's own name, which is also the key its fold state is held under.
	head.set_meta(HudCraftingVocab.GROUP_HEAD_META, head_name)
	head.pressed.connect(func() -> void: _toggle_fold(head_name))
	host.add_child(head)
	return host

## Fold or unfold a group and re-render the panel against the payload it is already holding. A rebuild
## rather than a `visible` flip because the ledger is rebuilt on every snapshot anyway, and a hidden
## subtree waiting to be shown is a second representation of the same state.
func _toggle_fold(head_name: String) -> void:
	_folded[head_name] = not _is_folded(head_name)
	if not _payload.is_empty():
		render(_payload)

## One ledger row, for one ITEM. **Its cells describe the SUGGESTED offer** — the recipe the sim
## would start and the picker opens on — while the Owned cell reads every batch of the item, whatever
## recipe made it.
func _build_ledger_row(ledger_row: Dictionary, batches_by_item: Dictionary,
		batches_by_material: Dictionary, payload: Dictionary) -> Control:
	var offer: Dictionary = ledger_row["offer"]
	var row := _ledger_row_container()
	var group := String(offer.get(HudCraftingVocab.OFFER_GROUP_KEY, ""))
	var batch := _batch_for(offer, batches_by_item)
	row.add_child(_column_cell(_build_item_cell(ledger_row, payload), 0.0, true))
	var owned := _build_owned_cell(offer, _batches_for(offer, batches_by_item), group, payload,
		batches_by_material)
	owned.set_meta(HudCraftingVocab.OWNED_CELL_META,
		String(offer.get(HudCraftingVocab.OFFER_OUTPUT_ITEM_ID_KEY, "")))
	row.add_child(_column_cell(owned, HudCraftingVocab.COLUMN_OWNED_WIDTH, false))
	row.add_child(_column_cell(_build_cost_cell(offer, payload), HudCraftingVocab.COLUMN_COST_WIDTH, false))
	row.add_child(_column_cell(_build_action_cell(ledger_row), HudCraftingVocab.COLUMN_ACTION_WIDTH, false))

	# **THE SHRUG IS DIMMED, NOT HIDDEN.** "Not needed yet" arrives with its own severity precisely so
	# it can read as the shrug it is — a neutral offer on a kit that is not worn is nothing to do, and
	# styling it like a shortage would make a problem out of a non-problem.
	if _is_shrug(ledger_row, batch):
		row.modulate = Color(1.0, 1.0, 1.0, HudCraftingVocab.DIMMED_ROW_ALPHA)
	return row

## **A ROW THE PLAYER HAS NOTHING TO DO ABOUT — a kit they own and have never used.** Two published
## facts, no threshold of ours: the sim called the offer NEUTRAL (so it is not a shortage and not a
## first tool), and the item is at FULL condition, meaning nothing has been spent on it.
##
## **"Nothing spent" is the test, not "comfortable".** A `healthy` life severity covers a sled at 42
## turns left, which is a real thing the player is using and must read at full strength; the shrug is
## the item that has never been touched. `remaining` is the published 0–100 condition of the unit in
## hand, so `>= 100` is exactly the sim's own `Untouched` — reached from the number rather than by
## matching its WORDING, which is a resolved string this panel may render and must not parse. A tier
## shipping a lower `starting_durability` simply never dims, which is the conservative direction:
## nothing is hidden, one row merely reads at full strength.
func _is_shrug(ledger_row: Dictionary, batch: Dictionary) -> bool:
	if _row_on_bench(ledger_row):
		return false
	var offer: Dictionary = ledger_row["offer"]
	if String(offer.get(HudCraftingVocab.OFFER_SEVERITY_KEY, "")) != HudCraftingVocab.SEVERITY_NEUTRAL:
		return false
	if batch.is_empty() or int(batch.get(HudCraftingVocab.EQUIPMENT_COUNT_KEY, 0)) <= 0:
		return false
	return float(batch.get(HudCraftingVocab.EQUIPMENT_REMAINING_KEY, 0.0)) \
		>= HudConst.PROGRESS_PERCENT_SCALE

## The item's name, then ONE second line: the `N recipes` link on an item with more than one recipe,
## else the role line a single-recipe row has always carried. **The link REPLACES the role line rather
## than stacking under it**, which is what keeps every row two lines tall: a kit row's role line names
## the craft that makes it, and an item with several recipes has several crafts, so the row cannot
## name one of them honestly — the popup is where the recipes are told apart.
func _build_item_cell(ledger_row: Dictionary, payload: Dictionary) -> Control:
	var offer: Dictionary = ledger_row["offer"]
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 0)
	var name_label := Label.new()
	name_label.text = String(offer.get(HudCraftingVocab.OFFER_DISPLAY_NAME_KEY, ""))
	name_label.add_theme_font_size_override("font_size", HudCraftingVocab.ITEM_NAME_FONT_SIZE)
	name_label.add_theme_color_override("font_color", HudStyle.INK)
	column.add_child(name_label)
	var recipe_count := (ledger_row["offers"] as Array).size()
	if recipe_count > 1:
		column.add_child(_build_recipes_link(String(ledger_row["key"]), recipe_count))
		return column
	var role := _role_line(offer, payload)
	if role != "":
		var role_label := Label.new()
		role_label.text = role
		role_label.add_theme_font_size_override("font_size", HudCraftingVocab.ITEM_ROLE_FONT_SIZE)
		role_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
		column.add_child(role_label)
	return column

## What this row IS, joined out of published fields: a TOOL names the material it bounds, a STOCK
## recipe names the characteristic its input is SPENT worst-first on, and a KIT row names the craft
## that makes it. A join that finds nothing renders no second line rather than an invented one.
##
## **THE STOCK BRANCH IS ALREADY THE MATERIAL-ONLY GATE.** `group == "stock"` is the wire's own
## `"this recipe makes a material"` — derived sim-side as `output_equipment_id().is_none()`, the same
## predicate behind `outputs_only_materials()` and behind an empty `outputItemId` — so a recipe
## reaching here can never stamp a grade, and the axis it names is a SPEND ORDER. See
## `HudCraftingVocab.ROLE_STOCK_FORMAT` for why the clause survived the quality claim that did not.
func _role_line(offer: Dictionary, payload: Dictionary) -> String:
	var group := String(offer.get(HudCraftingVocab.OFFER_GROUP_KEY, ""))
	var recipe := _recipe_of(String(offer.get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, "")), payload)
	if group == HudCraftingVocab.GROUP_TOOL:
		var item_id := String(offer.get(HudCraftingVocab.OFFER_OUTPUT_ITEM_ID_KEY, ""))
		for material_variant in payload.get(PAYLOAD_MATERIALS, []):
			if not (material_variant is Dictionary):
				continue
			var material: Dictionary = material_variant
			if String(material.get("tool_item_id", "")) == item_id and item_id != "":
				return HudCraftingVocab.ROLE_TOOL_FORMAT % String(material.get(
					HudCraftingVocab.MATERIAL_ID_KEY, ""))
	var craft_name := _craft_display_name(
		String(recipe.get(HudCraftingVocab.MATERIAL_CRAFT_KEY, "")), payload)
	if group == HudCraftingVocab.GROUP_STOCK:
		for input_variant in recipe.get(HudCraftingVocab.RECIPE_INPUTS_KEY, []):
			if not (input_variant is Dictionary):
				continue
			var input: Dictionary = input_variant
			var axis := String(input.get("reads_axis", ""))
			if axis == "":
				continue
			# **THE CRAFT LEADS AND THE MATERIAL IS ABSENT.** The material is named twice already on
			# this row — the cost cell and the sim's own refusal — so the axis is the only fact this
			# line adds, and leading with the craft keeps line two a category on every row. What the
			# axis buys the player here is the DRAW ORDER: the bench spends his worst stock on it
			# first and leaves the good stock standing.
			if craft_name == "":
				return HudCraftingVocab.ROLE_STOCK_NO_CRAFT_FORMAT % axis
			return HudCraftingVocab.ROLE_STOCK_FORMAT % [craft_name, axis]
	return craft_name

## **WHAT THE BAND HAS, AND HOW GOOD IT IS.** Three cases, and only one of them is a count:
##
## - A **stock** recipe reports the band's TOTAL of the material it makes — one number, summed across
##   its batches, with no `×` and no rating chips: the per-batch ratings are the material rail's fact
##   and this cell would only duplicate them. See `_build_stock_owned_cell`.
## - Owning **none** states the CONSEQUENCE rather than the arithmetic — `Not made`, the SAME wording
##   for a kit and for a bench tool, because the cell has to answer the column it sits under and that
##   column is `Owned`. The `group` still picks the chip's INK (a lacked kit is `DANGER`, a never-built
##   bench tool `INK_FAINT`), so the distinction survives as urgency rather than as a second sentence.
##   Keyed off `count`, NEVER off `remaining == 0`: a spent batch is removed, so worn-out and
##   never-made both read zero condition.
## - Owning **units** reads one line per GRADE, best first, with the counts of the batches sharing a
##   grade summed — two `good` batches at different wear are one line of `×5`, wear not being this
##   panel's fact. **TWO GRADES GET A LINE EACH**: `×5 · excellent` would be a lie, and every rule for
##   collapsing to one misleads — the best flatters, the worst alarms, and the batch actually in
##   service is chosen by wear rather than by quality, so it would move for a reason unrelated to what
##   the row claims.
##
## **NO TIER WORD REACHES THIS CELL AT ALL** — no `tier_id`, and not the sim's `ownedNote` either,
## though it still rides the wire: that note was written for the retired tier heads, it cost a row that
## must stay short a line, and it put the internal word `plain` in front of the player. Which tier the
## band holds is answered in the recipe popup's Owned column, on an item whose recipes make different
## tiers.
func _build_owned_cell(offer: Dictionary, batches: Array, group: String, payload: Dictionary,
		batches_by_material: Dictionary) -> Control:
	if group == HudCraftingVocab.GROUP_STOCK:
		return _build_stock_owned_cell(offer, payload, batches_by_material)

	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)
	var lines := _owned_lines(batches, payload)
	if lines.is_empty():
		# **ONE WORDING, TWO COLOURS.** The cell sits under `Owned`, so it answers that question for
		# every group — a bench tool and a kit alike read `Not made`. What still differs is urgency,
		# and that is the chip's ink: a kit the band lacks has people working bare-handed this turn.
		var none_ink := HudStyle.INK_FAINT if group == HudCraftingVocab.GROUP_TOOL else HudStyle.DANGER
		column.add_child(_chip(HudCraftingVocab.OWNED_NONE, none_ink,
			HudCraftingVocab.OWNED_CHIP_FONT_SIZE))
	for line_variant in lines:
		var line: Dictionary = line_variant
		column.add_child(_owned_line(int(line["count"]), String(line["grade"]), payload))
	return column

## **THE BAND'S TOTAL OF THE MATERIAL THIS ROW MAKES — ONE NUMBER, summed across its batches.**
##
## The cell used to state the recipe's YIELD here (`→ 1 hurdles`) on the premise that *a stock recipe
## owns nothing*, which was simply false: a crafted material is banked in `material_batches` like any
## other, and the same panel was already drawing the band's 5.0 hurdles in the rail 250px to the left
## while this cell — under a column head reading **Owned** — reported a number that was not an amount.
## The yield moved to the cost cell, where `inputs → output` gives the arrow its left-hand side.
##
## **THE PER-BATCH RATINGS ARE THE RAIL'S FACT, NOT THIS CELL'S**, which is why `_build_batch_row` is
## deliberately NOT reused here. Reusing it imported a 250px column's layout into a 172px one: at the
## shipped axis names (`hurdles` is rated `stoutness` / `span`) the chip pair wraps, and a two-batch
## pile then cost the ledger ~29px — measured as the difference that pushed the collapsed band-dock
## ledger past its room. The chips were paying that in duplicate: the rail shows the same batches at
## the same ratings, in the same panel, always visible.
##
## **AND WITHOUT THE CHIPS THE SPLIT MEANS NOTHING.** Equipment splits by GRADE because two grades are
## genuinely different objects and the grade appears nowhere else; strip a pile's ratings and its two
## lines are two indistinguishable numbers. So the split collapses with them, and what is left is the
## question a column headed *Owned* asks: how much have we got.
##
## **THE AMOUNT IS FRACTIONAL AND CARRIES NO `×`** (`BATCH_AMOUNT_FORMAT`, the rail's own spelling), but
## it is sized like the equipment column's count — a material total and a kit count are the same kind
## of number and should read as one.
func _build_stock_owned_cell(offer: Dictionary, payload: Dictionary,
		batches_by_material: Dictionary) -> Control:
	var recipe := _recipe_of(String(offer.get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, "")), payload)
	var output := _stock_output(recipe)
	if output.is_empty():
		return _empty_cell()
	var batches: Array = batches_by_material.get(
		String(output.get(HudCraftingVocab.RECIPE_OUTPUT_MATERIAL_ID_KEY, "")), [])
	if batches.is_empty():
		# **A MATERIAL THE BAND HOLDS NONE OF STATES THAT, not `0.0`** — and it states it in the
		# ledger's ONE none-wording, the same `Not made` a kit and a bench tool answer this column
		# with. What is different about a pile is its ink, not its sentence.
		return _chip(HudCraftingVocab.OWNED_NONE, HudStyle.INK_FAINT,
			HudCraftingVocab.OWNED_CHIP_FONT_SIZE)
	var total := 0.0
	for batch_variant in batches:
		if batch_variant is Dictionary:
			total += float((batch_variant as Dictionary).get(HudCraftingVocab.BATCH_AMOUNT_KEY, 0.0))
	var held := Label.new()
	held.text = HudCraftingVocab.BATCH_AMOUNT_FORMAT % total
	held.add_theme_font_size_override("font_size", HudCraftingVocab.OWNED_COUNT_FONT_SIZE)
	held.add_theme_color_override("font_color", HudStyle.INK)
	return held

## **THE OUTPUT ROW THAT MAKES A RECIPE A STOCK RECIPE** — the first output that names a MATERIAL, and
## `{}` on a recipe whose outputs are equipment. Read by both surfaces that state the pass's yield: the
## Owned cell joins on its material id and the cost cell prints its amount beside it, and one lookup is
## what keeps the two from disagreeing about which output the arrow points at.
func _stock_output(recipe: Dictionary) -> Dictionary:
	for output_variant in recipe.get(HudCraftingVocab.RECIPE_OUTPUTS_KEY, []):
		if not (output_variant is Dictionary):
			continue
		var output: Dictionary = output_variant
		if String(output.get(HudCraftingVocab.RECIPE_OUTPUT_MATERIAL_ID_KEY, "")) != "":
			return output
	return {}

## One owned line: the count, then the grade as a chip. **A grade the published legend does not
## contain renders NO chip** — a start-stocked unit publishes `""`, and a spawn's kit was never on a
## bench and makes no quality claim to render.
func _owned_line(count: int, grade: String, payload: Dictionary) -> Control:
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)
	var count_label := Label.new()
	count_label.text = HudCraftingVocab.OWNED_COUNT_FORMAT % count
	count_label.add_theme_font_size_override("font_size", HudCraftingVocab.OWNED_COUNT_FONT_SIZE)
	count_label.add_theme_color_override("font_color", HudStyle.INK)
	line.add_child(count_label)
	if _grade_rung(grade, payload) >= 0:
		line.add_child(_chip(grade, _grade_color(grade, payload),
			HudCraftingVocab.OWNED_CHIP_FONT_SIZE))
	return line

## **THE BATCHES COLLAPSED TO ONE LINE PER GRADE, BEST FIRST.** Counts are summed across the batches
## sharing a grade because wear is what separates those batches and wear is not this panel's fact; the
## grades themselves are kept apart because they are genuinely different objects.
##
## A grade outside the published legend has no rung, so it sorts last — it is a claim about nothing
## rather than a bad one, and it renders without a chip.
func _owned_lines(batches: Array, payload: Dictionary) -> Array:
	var counts := {}
	for batch_variant in batches:
		if not (batch_variant is Dictionary):
			continue
		var batch: Dictionary = batch_variant
		var count := int(batch.get(HudCraftingVocab.EQUIPMENT_COUNT_KEY, 0))
		# `count`, never `remaining == 0` — the two states a zero condition can mean are told apart on
		# the wire and must be told apart here.
		if count <= 0:
			continue
		var grade := String(batch.get(HudCraftingVocab.EQUIPMENT_GRADE_KEY, ""))
		counts[grade] = int(counts.get(grade, 0)) + count
	var grades: Array = counts.keys()
	grades.sort_custom(func(a: String, b: String) -> bool:
		return _grade_rung(a, payload) > _grade_rung(b, payload))
	var lines: Array = []
	for grade in grades:
		lines.append({"count": int(counts[grade]), "grade": String(grade)})
	return lines

## Where a grade sits in the published `characteristic_bands` legend, ascending — `-1` for a name the
## legend does not carry. **The legend is the sim's, and this is a lookup in it rather than a table of
## grade names typed here**: a client-side copy would disagree with the sim the day a band is renamed.
func _grade_rung(grade: String, payload: Dictionary) -> int:
	if grade == "":
		return -1
	var legend: Array = payload.get(PAYLOAD_BAND_LEGEND, [])
	for i in range(legend.size()):
		if not (legend[i] is Dictionary):
			continue
		if String((legend[i] as Dictionary).get(HudCraftingVocab.BAND_LEGEND_NAME_KEY, "")) == grade:
			return i
	return -1

## **THE CHIP'S TINT IS THE RUNG'S POSITION IN THE LEGEND, never a match on its name.** The last rung
## is the best work the ladder has, the one below it is good work, the first is the bottom of it, and
## anything between stays quiet — so a fifth band added sim-side lands in the quiet middle instead of
## rendering every chip neutral, which is what a `{"excellent": HEALTHY}` table here would do.
func _grade_color(grade: String, payload: Dictionary) -> Color:
	var legend: Array = payload.get(PAYLOAD_BAND_LEGEND, [])
	var rung := _grade_rung(grade, payload)
	var top := legend.size() - 1
	if rung == top:
		return HudCraftingVocab.OWNED_GRADE_TOP_COLOR
	if rung == top - 1:
		return HudCraftingVocab.OWNED_GRADE_HIGH_COLOR
	if rung == 0:
		return HudCraftingVocab.OWNED_GRADE_LOW_COLOR
	return HudCraftingVocab.OWNED_GRADE_MID_COLOR

## **WHAT REBUILDING COSTS, WITH THE SHORTFALL MARKED.** The amounts are the recipe's own inputs,
## except where the offer publishes a shortfall for that material — there the `required` figure is
## used, because it is already net of the bench tool's material efficiency and is the number the
## refusal beside it was computed against. A short material is tinted so the eye finds it without
## reading the button.
##
## **A STOCK RECIPE'S YIELD IS THIS CELL'S SECOND LINE — `4 wood · 2 hide` over `→ 1 hurdles`.** It
## hung in the Owned cell until the arrow was reported as meaningless, which it was: `→ 1 hurdles`
## alone in a cell names a relation with nothing on its left. Here the inputs ARE the left-hand side,
## which is the only place in the panel where an arrow reads. It renders on a MATERIAL output and on
## no other — a kit recipe's output is the row's own name, and an arrow back at the title says nothing.
##
## **THE SECOND LINE IS DELIBERATE, NOT A WRAP**, which is why the cell is a VBox of the inputs' flow
## and the yield rather than one flow holding both. The shipped two-input `hurdles` costs about 157px
## at `COST_FONT_SIZE` against a `COLUMN_COST_WIDTH` of 140, so appending the yield to the flow put the
## arrow alone at the head of a wrapped line — and any width that fixed that breaks again on a
## three-input recipe, while widening the cost column would steal from the Item column on EVERY row to
## pay for a fact only stock rows carry. Stacked, the inputs sit directly above the arrow, which is the
## left-hand side moving the yield here was for.
func _build_cost_cell(offer: Dictionary, payload: Dictionary) -> Control:
	var recipe := _recipe_of(String(offer.get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, "")), payload)
	var shortfalls := {}
	for shortfall_variant in offer.get(HudCraftingVocab.OFFER_SHORTFALLS_KEY, []):
		if shortfall_variant is Dictionary:
			var shortfall: Dictionary = shortfall_variant
			shortfalls[String(shortfall.get(HudCraftingVocab.SHORTFALL_MATERIAL_ID_KEY, ""))] = shortfall
	var cell := VBoxContainer.new()
	cell.add_theme_constant_override("separation", 0)
	var flow := HFlowContainer.new()
	var inputs: Array = recipe.get(HudCraftingVocab.RECIPE_INPUTS_KEY, [])
	var yielded := _stock_output(recipe)
	if inputs.is_empty():
		return _empty_cell()
	cell.add_child(flow)
	for i in range(inputs.size()):
		if not (inputs[i] is Dictionary):
			continue
		var input: Dictionary = inputs[i]
		var material_id := String(input.get(HudCraftingVocab.RECIPE_INPUT_MATERIAL_ID_KEY, ""))
		var amount := float(input.get(HudCraftingVocab.RECIPE_INPUT_AMOUNT_KEY, 0.0))
		var short: bool = shortfalls.has(material_id)
		if short:
			amount = float((shortfalls[material_id] as Dictionary).get(
				HudCraftingVocab.SHORTFALL_REQUIRED_KEY, amount))
		var clause := Label.new()
		clause.text = HudCraftingVocab.COST_CLAUSE_FORMAT % [_amount_text(amount), material_id]
		if i < inputs.size() - 1:
			clause.text += HudCraftingVocab.COST_SEPARATOR
		clause.add_theme_font_size_override("font_size", HudCraftingVocab.COST_FONT_SIZE)
		clause.add_theme_color_override("font_color", HudStyle.DANGER if short else HudStyle.INK_DIM)
		flow.add_child(clause)
	# The yield is a SEPARATE label under the flow rather than another clause in it, and it carries no
	# ` · `: the arrow is its own separator, and its own line keeps it under the inputs it points out
	# of instead of wherever the flow's wrap happens to leave it.
	if not yielded.is_empty():
		var made := Label.new()
		made.text = HudCraftingVocab.COST_YIELD_FORMAT % [
			_amount_text(float(yielded.get(HudCraftingVocab.RECIPE_OUTPUT_AMOUNT_KEY, 0.0))),
			String(yielded.get(HudCraftingVocab.RECIPE_OUTPUT_MATERIAL_ID_KEY, ""))]
		made.add_theme_font_size_override("font_size", HudCraftingVocab.COST_FONT_SIZE)
		made.add_theme_color_override("font_color", HudStyle.INK_FAINT)
		cell.add_child(made)
	return cell

## **MAKE STAGES THE JOB, AND A REFUSAL NAMES ITS NUMBER.** On a single-recipe row the button puts that
## recipe on the bench at once and leaves the crew to the stepper; on a row with several it opens the
## picker under the row instead (`_toggle_picker`), because the recipe cannot be changed once the
## build starts. The button is LIVE when ANY of the row's recipes can be made — the suggested one may
## be the one that cannot — and a row with a recipe on the bench is spent and reads *On the bench*.
## Under it, the suggested offer's `reason` VERBATIM in the tint its published `severity` picked —
## *"Short 4.9 bone"*, never *"cannot craft"*, and never a sentence composed here.
func _build_action_cell(ledger_row: Dictionary) -> Control:
	var offer: Dictionary = ledger_row["offer"]
	var offers: Array = ledger_row["offers"]
	var key := String(ledger_row["key"])
	var column := VBoxContainer.new()
	column.alignment = BoxContainer.ALIGNMENT_BEGIN
	column.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)
	var running := _row_on_bench(ledger_row)
	var button := Button.new()
	button.text = HudCraftingVocab.ON_BENCH_LABEL if running else HudCraftingVocab.MAKE_LABEL
	button.focus_mode = Control.FOCUS_NONE
	button.add_theme_font_size_override("font_size", HudCraftingVocab.ACTION_FONT_SIZE)
	HudStyle.apply_button(button, "primary")
	button.disabled = running or not _row_available(ledger_row)
	# Found by IDENTITY, valued the row key — every row's button wears the same face.
	button.set_meta(HudCraftingVocab.MAKE_BUTTON_META, key)
	if not button.disabled:
		if offers.size() == 1:
			var recipe_id := String(offer.get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, ""))
			button.pressed.connect(func() -> void: make_requested.emit(recipe_id))
		else:
			button.pressed.connect(func() -> void: _toggle_picker(key))
	column.add_child(button)

	var reason := String(offer.get(HudCraftingVocab.OFFER_REASON_KEY, ""))
	if reason != "":
		var why := Label.new()
		why.text = reason
		why.add_theme_font_size_override("font_size", HudCraftingVocab.REASON_FONT_SIZE)
		why.add_theme_color_override("font_color", HudCraftingVocab.REASON_COLORS.get(
			String(offer.get(HudCraftingVocab.OFFER_SEVERITY_KEY, "")),
			HudCraftingVocab.REASON_COLOR_QUIET))
		why.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		why.custom_minimum_size = Vector2(HudCraftingVocab.COLUMN_ACTION_WIDTH, 0.0)
		column.add_child(why)
	return column

# ---- an item's recipes: the link, the popup, the Make picker ----------------

## **THE `N recipes` LINK** — flat, underlined, in the signal ink, under the item's name. It fires on
## the PRESS rather than the release so that a press which has just closed the popup can be recognised
## as the same click (`_on_recipes_link`).
func _build_recipes_link(key: String, recipe_count: int) -> LinkButton:
	var link := LinkButton.new()
	link.text = HudCraftingVocab.RECIPES_LINK_FORMAT % recipe_count
	link.underline = LinkButton.UNDERLINE_MODE_ALWAYS
	link.focus_mode = Control.FOCUS_NONE
	link.action_mode = BaseButton.ACTION_MODE_BUTTON_PRESS
	link.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	link.add_theme_font_size_override("font_size", HudCraftingVocab.RECIPES_LINK_FONT_SIZE)
	link.add_theme_color_override("font_color", HudStyle.SIGNAL)
	link.add_theme_color_override("font_hover_color", HudStyle.INK)
	link.add_theme_color_override("font_pressed_color", HudStyle.INK)
	link.set_meta(HudCraftingVocab.RECIPES_LINK_META, key)
	link.pressed.connect(func() -> void: _on_recipes_link(key, link))
	if key == _popup_row:
		_popup_anchor = link
	return link

## Open the row's popup, or close it when it is already the open one. **One popup at a time**, so
## opening another row's simply moves it.
func _on_recipes_link(key: String, link: Control) -> void:
	if _popup_row == key:
		_close_recipes_popup()
		return
	# The press that closed this very popup a moment ago, arriving at its own link: a toggle, not a
	# request to open it again.
	if _popup_closed_row == key and _popup_closed_frame == Engine.get_process_frames():
		return
	_popup_row = key
	_popup_anchor = link
	_sync_recipes_popup()

func _close_recipes_popup() -> void:
	if _popup_row == NO_ROW:
		return
	_popup_closed_row = _popup_row
	_popup_closed_frame = Engine.get_process_frames()
	_popup_row = NO_ROW
	_popup_anchor = null
	if _recipes_popup != null and _recipes_popup.visible:
		_recipes_popup.hide()
	if _recipes_popup_body != null:
		# Emptied rather than left behind a hidden Window, so nothing a closed popup said can be found
		# by a search of this panel.
		HudWidgets.clear_children(_recipes_popup_body)

## A click outside the popup or `Esc` closed it — the Window's own behaviour, converging on the same
## teardown as the link's second click.
func _on_recipes_popup_hidden() -> void:
	_close_recipes_popup()

## Fill the popup from the payload and hang it under its link. Called on open and after every refit, so
## a turn tick restates the numbers under a popup the player is reading and the popup follows its link.
func _sync_recipes_popup() -> void:
	if _popup_row == NO_ROW:
		return
	var row := _row_in(_payload, _popup_row)
	if row.is_empty() or _popup_anchor == null or not is_instance_valid(_popup_anchor):
		_close_recipes_popup()
		return
	var popup := _ensure_recipes_popup()
	HudWidgets.clear_children(_recipes_popup_body)
	_fill_recipes_popup(row, _payload)
	var xform := _popup_anchor.get_screen_transform()
	var below := Vector2i(xform * Vector2(0.0, _popup_anchor.size.y + HudCraftingVocab.RECIPES_POPUP_GAP))
	if popup.visible:
		# Already up: restated in place, so a refit does not hide and re-show it.
		popup.position = below
		popup.reset_size()
	else:
		popup.popup(Rect2i(below, Vector2i.ZERO))

## The popup itself: ONE `PopupPanel`, built on first use and kept. A Window, so it cannot change the
## ledger's height — the reason a comparison table can open over the rows without pushing them.
func _ensure_recipes_popup() -> PopupPanel:
	if _recipes_popup != null and is_instance_valid(_recipes_popup):
		return _recipes_popup
	var popup := PopupPanel.new()
	popup.name = "RecipesPopup"
	popup.add_theme_stylebox_override("panel", HudStyle.popup_panel_stylebox())
	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", HudCraftingVocab.RECIPES_POPUP_ROW_SEPARATION)
	body.set_meta(HudCraftingVocab.RECIPES_POPUP_META, true)
	popup.add_child(body)
	popup.popup_hide.connect(_on_recipes_popup_hidden)
	add_child(popup)
	_recipes_popup = popup
	_recipes_popup_body = body
	return popup

## **ONE LINE PER RECIPE, AND AN OWNED COLUMN ONLY WHERE A COUNT PER RECIPE EXISTS.** The columns are
## Recipe · Costs · Makes · Lasts, plus Owned when some offer in the row carries a real `ownedAtTier`.
## When every offer carries `OWNED_AT_TIER_UNATTRIBUTED` — the substitutes, which make one item at one
## tier — the column is left out rather than filled with dashes: the row's own Owned cell already says
## how many the band holds, and restating that total per recipe would claim the recipes made them.
func _fill_recipes_popup(row: Dictionary, payload: Dictionary) -> void:
	var offers: Array = row["offers"]
	var title := Label.new()
	title.text = (HudCraftingVocab.RECIPES_POPUP_TITLE_FORMAT % [
		String((row["offer"] as Dictionary).get(HudCraftingVocab.OFFER_DISPLAY_NAME_KEY, "")),
		offers.size()]).to_upper()
	title.add_theme_font_size_override("font_size", HudCraftingVocab.RECIPES_POPUP_TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_recipes_popup_body.add_child(title)

	var show_owned := false
	for offer in offers:
		if int((offer as Dictionary).get(HudCraftingVocab.OFFER_OWNED_AT_TIER_KEY,
				HudCraftingVocab.OWNED_AT_TIER_UNATTRIBUTED)) != HudCraftingVocab.OWNED_AT_TIER_UNATTRIBUTED:
			show_owned = true
	var heads: Array[String] = [HudCraftingVocab.RECIPES_COLUMN_RECIPE,
		HudCraftingVocab.RECIPES_COLUMN_COST, HudCraftingVocab.RECIPES_COLUMN_MAKES,
		HudCraftingVocab.RECIPES_COLUMN_LASTS]
	if show_owned:
		heads.append(HudCraftingVocab.RECIPES_COLUMN_OWNED)

	var grid := GridContainer.new()
	grid.columns = heads.size()
	grid.add_theme_constant_override("h_separation", HudCraftingVocab.RECIPES_POPUP_COLUMN_SEPARATION)
	grid.add_theme_constant_override("v_separation", HudCraftingVocab.RECIPES_POPUP_ROW_SEPARATION)
	for head in heads:
		var head_label := _popup_text(head.to_upper(), HudStyle.INK_FAINT,
			HudCraftingVocab.RECIPES_POPUP_HEAD_FONT_SIZE)
		head_label.set_meta(HudCraftingVocab.RECIPES_POPUP_COLUMN_META, head)
		grid.add_child(head_label)
	for offer_variant in offers:
		var offer: Dictionary = offer_variant
		var cost := _build_cost_cell(offer, payload)
		cost.custom_minimum_size = Vector2(HudCraftingVocab.RECIPE_COST_WIDTH, 0.0)
		var cells: Array[Control] = [
			_popup_text(String(offer.get(HudCraftingVocab.OFFER_RECIPE_LABEL_KEY, "")), HudStyle.INK,
				HudCraftingVocab.RECIPES_POPUP_CELL_FONT_SIZE),
			cost,
			_popup_makes_cell(offer, payload),
			_popup_text(String(offer.get(HudCraftingVocab.OFFER_LASTS_KEY, "")), HudStyle.INK_DIM,
				HudCraftingVocab.RECIPES_POPUP_CELL_FONT_SIZE),
		]
		if show_owned:
			var owned := int(offer.get(HudCraftingVocab.OFFER_OWNED_AT_TIER_KEY,
				HudCraftingVocab.OWNED_AT_TIER_UNATTRIBUTED))
			cells.append(_popup_text(HudCraftingVocab.OWNED_COUNT_FORMAT % owned if owned > 0
					else HudCraftingVocab.RECIPES_OWNED_NONE, HudStyle.INK,
				HudCraftingVocab.RECIPES_POPUP_CELL_FONT_SIZE))
		for i in range(cells.size()):
			cells[i].set_meta(HudCraftingVocab.RECIPES_POPUP_COLUMN_META, heads[i])
			grid.add_child(cells[i])
	_recipes_popup_body.add_child(grid)

## A recipe's Makes cell: the grade chip — the SAME chip the Owned cell draws, tinted by the grade's
## place in the published legend — and then what it makes. A grade the legend does not carry draws no
## chip, exactly as in the Owned cell.
func _popup_makes_cell(offer: Dictionary, payload: Dictionary) -> Control:
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudCraftingVocab.ROW_SEPARATION)
	var grade := String(offer.get(HudCraftingVocab.OFFER_OUTPUT_GRADE_KEY, ""))
	if _grade_rung(grade, payload) >= 0:
		line.add_child(_chip(grade, _grade_color(grade, payload), HudCraftingVocab.OWNED_CHIP_FONT_SIZE))
	line.add_child(_popup_text(String(offer.get(HudCraftingVocab.OFFER_MAKES_KEY, "")), HudStyle.INK_DIM,
		HudCraftingVocab.RECIPES_POPUP_CELL_FONT_SIZE))
	return line

func _popup_text(text: String, ink: Color, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", ink)
	return label

## Open the picker under a row, or close it when it is the open one — Make pressed twice is a change of
## mind. **One picker at a time**, and opening one closes the popup: the picker is where the choice is
## made, so a comparison table left hanging over it would only cover it.
func _toggle_picker(key: String) -> void:
	if _picker_row == key:
		_picker_row = NO_ROW
		_picker_choice = ""
	else:
		_picker_row = key
		_picker_choice = _default_choice(_row_in(_payload, key))
		_close_recipes_popup()
	if not _payload.is_empty():
		render(_payload)

## **THE MAKE PICKER**, spanning the ledger under its row. One radio line per recipe — its label, its
## cost, and either what it makes or, when it cannot be made now, the sim's own reason with the radio
## DISABLED. The footer says the recipe is fixed once the build starts, and Start sends the CHOSEN
## recipe, which is the only way a row's non-suggested recipe ever reaches the bench.
func _build_make_picker(row: Dictionary, payload: Dictionary) -> Control:
	var key := String(row["key"])
	var host := MarginContainer.new()
	host.add_theme_constant_override("margin_bottom", HudCraftingVocab.PICKER_MARGIN_BOTTOM)
	host.set_meta(HudCraftingVocab.PICKER_META, key)
	var box := PanelContainer.new()
	var style := HudStyle.readout_stylebox()
	style.border_color = HudStyle.SIGNAL_DEEP
	style.set_border_width_all(HudCraftingVocab.PICKER_BORDER_WIDTH)
	style.content_margin_left = HudCraftingVocab.PICKER_PADDING_H
	style.content_margin_right = HudCraftingVocab.PICKER_PADDING_H
	style.content_margin_top = HudCraftingVocab.PICKER_PADDING_V
	style.content_margin_bottom = HudCraftingVocab.PICKER_PADDING_V
	box.add_theme_stylebox_override("panel", style)
	host.add_child(box)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", HudCraftingVocab.PICKER_LINE_SEPARATION)
	box.add_child(column)

	var heading := _popup_text(HudCraftingVocab.PICKER_HEADING_FORMAT % String(
		(row["offer"] as Dictionary).get(HudCraftingVocab.OFFER_DISPLAY_NAME_KEY, "")), HudStyle.INK,
		HudCraftingVocab.PICKER_HEADING_FONT_SIZE)
	column.add_child(heading)

	var group := ButtonGroup.new()
	for offer in row["offers"]:
		column.add_child(_build_picker_option(offer, group, payload))

	column.add_child(_rule(HudStyle.LINE_SOFT))
	var footer := HBoxContainer.new()
	footer.add_theme_constant_override("separation", HudCraftingVocab.COLUMN_SEPARATION)
	var note := _popup_text(HudCraftingVocab.PICKER_LOCK_NOTE, HudStyle.INK_FAINT,
		HudCraftingVocab.PICKER_NOTE_FONT_SIZE)
	note.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	footer.add_child(note)
	var cancel := Button.new()
	cancel.text = HudCraftingVocab.PICKER_CANCEL
	cancel.focus_mode = Control.FOCUS_NONE
	cancel.add_theme_font_size_override("font_size", HudCraftingVocab.ACTION_FONT_SIZE)
	HudStyle.apply_button(cancel, "ghost")
	cancel.set_meta(HudCraftingVocab.PICKER_CANCEL_META, key)
	cancel.pressed.connect(func() -> void: _toggle_picker(key))
	footer.add_child(cancel)
	var start := Button.new()
	start.text = HudCraftingVocab.PICKER_START
	start.focus_mode = Control.FOCUS_NONE
	start.add_theme_font_size_override("font_size", HudCraftingVocab.ACTION_FONT_SIZE)
	HudStyle.apply_button(start, "primary")
	start.disabled = _picker_choice == ""
	start.set_meta(HudCraftingVocab.PICKER_START_META, key)
	start.pressed.connect(func() -> void: _start_chosen_recipe())
	footer.add_child(start)
	column.add_child(footer)
	return host

## One recipe as a radio line. The radio is DISABLED when the recipe cannot be made now, and its line
## then states the sim's reason where a buildable one states what it would make.
func _build_picker_option(offer: Dictionary, group: ButtonGroup, payload: Dictionary) -> Control:
	var recipe_id := String(offer.get(HudCraftingVocab.OFFER_RECIPE_ID_KEY, ""))
	var available := bool(offer.get(HudCraftingVocab.OFFER_AVAILABLE_KEY, false))
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudCraftingVocab.COLUMN_SEPARATION)
	var radio := CheckBox.new()
	radio.text = String(offer.get(HudCraftingVocab.OFFER_RECIPE_LABEL_KEY, ""))
	radio.button_group = group
	radio.focus_mode = Control.FOCUS_NONE
	radio.custom_minimum_size = Vector2(HudCraftingVocab.PICKER_LABEL_WIDTH, 0.0)
	radio.add_theme_font_size_override("font_size", HudCraftingVocab.PICKER_OPTION_FONT_SIZE)
	HudStyle.apply_radio(radio)
	# Set BEFORE the handler is connected, so restating the choice on a rebuild is not a pick.
	radio.button_pressed = recipe_id == _picker_choice
	radio.disabled = not available
	radio.set_meta(HudCraftingVocab.PICKER_OPTION_META, recipe_id)
	radio.toggled.connect(func(on: bool) -> void:
		if on:
			_picker_choice = recipe_id)
	line.add_child(radio)

	var cost := _build_cost_cell(offer, payload)
	cost.custom_minimum_size = Vector2(HudCraftingVocab.RECIPE_COST_WIDTH, 0.0)
	line.add_child(cost)

	var summary: Label
	if available:
		summary = _popup_text(_picker_summary(offer), HudStyle.INK_FAINT,
			HudCraftingVocab.PICKER_OPTION_FONT_SIZE)
	else:
		summary = _popup_text(String(offer.get(HudCraftingVocab.OFFER_REASON_KEY, "")),
			HudCraftingVocab.REASON_COLORS.get(String(offer.get(HudCraftingVocab.OFFER_SEVERITY_KEY, "")),
				HudCraftingVocab.REASON_COLOR_QUIET), HudCraftingVocab.PICKER_OPTION_FONT_SIZE)
	summary.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	summary.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	line.add_child(summary)
	return line

## `<grade> · <makes> · lasts <lasts>`, each clause present only when the sim published it — a recipe
## resolving no grade leads with what it makes.
func _picker_summary(offer: Dictionary) -> String:
	var parts: Array[String] = []
	var grade := String(offer.get(HudCraftingVocab.OFFER_OUTPUT_GRADE_KEY, ""))
	if grade != "":
		parts.append(grade)
	var makes := String(offer.get(HudCraftingVocab.OFFER_MAKES_KEY, ""))
	if makes != "":
		parts.append(makes)
	var lasts := String(offer.get(HudCraftingVocab.OFFER_LASTS_KEY, ""))
	if lasts != "":
		parts.append(HudCraftingVocab.PICKER_LASTS_FORMAT % lasts)
	return HudCraftingVocab.PICKER_SUMMARY_SEPARATOR.join(parts)

## **START SENDS THE CHOSEN RECIPE AND CLOSES THE PICKER.** `set_bench` with the recipe the radio names,
## which is the same command a single-recipe row's Make sends — the picker only decides which one.
func _start_chosen_recipe() -> void:
	var chosen := _picker_choice
	_picker_row = NO_ROW
	_picker_choice = ""
	if chosen != "":
		make_requested.emit(chosen)
	if not _payload.is_empty():
		render(_payload)

# ---- the joins --------------------------------------------------------------

## The band's material batches, grouped by material id in wire order.
func _batches_by_material(band: Dictionary) -> Dictionary:
	var grouped := {}
	for batch_variant in band.get(HudCraftingVocab.BAND_MATERIAL_BATCHES_KEY, []):
		if not (batch_variant is Dictionary):
			continue
		var batch: Dictionary = batch_variant
		var material_id := String(batch.get(HudCraftingVocab.BATCH_MATERIAL_ID_KEY, ""))
		if not grouped.has(material_id):
			grouped[material_id] = []
		grouped[material_id].append(batch)
	return grouped

## The band's equipment batches, grouped by item id in wire order. **ALL of them, not the first** — a
## band may hold one item at two grades, and the Owned cell gives each of them a line. **A row per item
## always arrives** — the sim publishes a `count: 0` row for every config item the band owns none of —
## so a lookup that misses means the item is not in the config at all, never that the band has none.
func _equipment_by_item(band: Dictionary) -> Dictionary:
	var by_item := {}
	for batch_variant in band.get(HudCraftingVocab.BAND_EQUIPMENT_BATCHES_KEY, []):
		if not (batch_variant is Dictionary):
			continue
		var batch: Dictionary = batch_variant
		var item_id := String(batch.get(HudCraftingVocab.EQUIPMENT_ITEM_ID_KEY, ""))
		if not by_item.has(item_id):
			by_item[item_id] = []
		by_item[item_id].append(batch)
	return by_item

func _batches_for(offer: Dictionary, batches_by_item: Dictionary) -> Array:
	var item_id := String(offer.get(HudCraftingVocab.OFFER_OUTPUT_ITEM_ID_KEY, ""))
	if item_id == "":
		return []
	return batches_by_item.get(item_id, [])

## The one batch the urgency SORT and the shrug test read — the item's first, in wire order. They are
## rankings over `life_severity` / `remaining`, i.e. over WEAR, and wear is the axis this panel does
## not report; the Owned cell reads every batch instead (`_batches_for`).
func _batch_for(offer: Dictionary, batches_by_item: Dictionary) -> Dictionary:
	var batches := _batches_for(offer, batches_by_item)
	return batches[0] if not batches.is_empty() else {}

func _recipe_of(recipe_id: String, payload: Dictionary) -> Dictionary:
	for recipe_variant in payload.get(PAYLOAD_RECIPES, []):
		if recipe_variant is Dictionary and String((recipe_variant as Dictionary).get(
				HudCraftingVocab.RECIPE_ID_KEY, "")) == recipe_id:
			return recipe_variant
	return {}

func _craft_track_of(craft_id: String, payload: Dictionary) -> Dictionary:
	for track_variant in payload.get(PAYLOAD_CRAFT_KNOWLEDGE, []):
		if track_variant is Dictionary and String((track_variant as Dictionary).get(
				HudCraftingVocab.CRAFT_KNOWLEDGE_CRAFT_ID_KEY, "")) == craft_id:
			return track_variant
	return {}

## A craft's name as the SIM spells it. `""` when the track has not been published, because a client
## that fell back to prettifying the id would invent a second spelling of the same craft.
func _craft_display_name(craft_id: String, payload: Dictionary) -> String:
	return String(_craft_track_of(craft_id, payload).get(
		HudCraftingVocab.CRAFT_KNOWLEDGE_DISPLAY_NAME_KEY, ""))

# ---- small builders ---------------------------------------------------------

func _zone_head(text: String) -> Label:
	var label := Label.new()
	label.text = text.to_upper()
	label.add_theme_font_size_override("font_size", HudCraftingVocab.ZONE_HEAD_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	return label

func _icon_button(glyph: String, tooltip: String) -> Button:
	var button := Button.new()
	button.text = glyph
	button.tooltip_text = tooltip
	button.focus_mode = Control.FOCUS_NONE
	button.custom_minimum_size = Vector2(HudCraftingVocab.ICON_BUTTON_SIZE, HudCraftingVocab.ICON_BUTTON_SIZE)
	button.add_theme_font_size_override("font_size", HudCraftingVocab.CHIP_FONT_SIZE)
	HudStyle.apply_button(button, "ghost")
	return button

func _crew_button(glyph: String) -> Button:
	var button := Button.new()
	button.text = glyph
	button.focus_mode = Control.FOCUS_NONE
	button.custom_minimum_size = Vector2(HudCraftingVocab.CREW_BUTTON_SIZE, HudCraftingVocab.CREW_BUTTON_SIZE)
	button.add_theme_font_size_override("font_size", HudCraftingVocab.CREW_BUTTON_FONT_SIZE)
	HudStyle.apply_button(button, "ghost")
	return button

## A bordered chip — the shared `HudStyle.chip_stylebox`, so a tier and a characteristic read as
## members of the same family the selection card's chips belong to rather than as a lookalike.
func _chip(text: String, tint: Color, font_size: int) -> Control:
	var host := PanelContainer.new()
	host.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	# **SHRINK ON BOTH AXES.** The shared chip radius is far past the control's own height so the ends
	# are true semicircles — which is right at a chip's natural height and becomes an ELLIPSE the
	# moment a cell stretches it, and a table cell stretches everything it holds by default.
	host.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	host.add_theme_stylebox_override("panel", HudStyle.chip_stylebox(tint))
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", tint)
	host.add_child(label)
	return host

## A two-Panel bar: a track and a fill anchored to `fraction` of it. Drawn rather than glyphed
## because this one is a CONTINUOUS reading (a bench's progress, an item's remaining condition), where
## `HudFormat.meter_bar`'s block glyphs are the right tool for a rung count — the craft track above
## uses exactly that.
func _bar(fraction: float, color: Color, height: float) -> Control:
	var track := Panel.new()
	track.custom_minimum_size = Vector2(0.0, height)
	track.add_theme_stylebox_override("panel", _bar_stylebox(HudStyle.LINE_SOFT))
	track.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var fill := Panel.new()
	fill.add_theme_stylebox_override("panel", _bar_stylebox(color))
	fill.anchor_left = 0.0
	fill.anchor_top = 0.0
	fill.anchor_right = clampf(fraction, 0.0, 1.0)
	fill.anchor_bottom = 1.0
	fill.offset_left = 0.0
	fill.offset_top = 0.0
	fill.offset_right = 0.0
	fill.offset_bottom = 0.0
	fill.mouse_filter = Control.MOUSE_FILTER_IGNORE
	track.add_child(fill)
	return track

## A full-width hairline. `LINE` under a head, `LINE_SOFT` under a row — the same two weights the
## prototype's ledger draws, and both are `HudStyle`'s own.
func _rule(color: Color) -> Control:
	var rule := Panel.new()
	rule.custom_minimum_size = Vector2(0.0, HudCraftingVocab.COLUMN_SEPARATOR_THICKNESS)
	var box := StyleBoxFlat.new()
	box.bg_color = color
	rule.add_theme_stylebox_override("panel", box)
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return rule

func _bar_stylebox(color: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = color
	box.set_corner_radius_all(HudCraftingVocab.BAR_CORNER_RADIUS)
	return box

func _empty_cell() -> Control:
	var label := Label.new()
	label.text = HudCraftingVocab.EMPTY_CELL
	label.add_theme_font_size_override("font_size", HudCraftingVocab.EMPTY_CELL_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	return label

## One ledger row's host: four cells, the first expanding, on a hairline that separates it from the
## next. The table is a column of these rather than a `GridContainer` because a GROUP HEAD spans the
## whole width and a grid cannot span.
func _ledger_row_container() -> HBoxContainer:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", HudCraftingVocab.COLUMN_SEPARATION)
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return row

func _column_cell(content: Control, width: float, expand: bool) -> Control:
	var host := MarginContainer.new()
	host.add_theme_constant_override("margin_top", HudCraftingVocab.LEDGER_ROW_PADDING_V)
	host.add_theme_constant_override("margin_bottom", HudCraftingVocab.LEDGER_ROW_PADDING_V)
	if expand:
		host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		host.custom_minimum_size = Vector2(HudCraftingVocab.COLUMN_ITEM_MIN_WIDTH, 0.0)
	else:
		host.size_flags_horizontal = Control.SIZE_FILL
		host.custom_minimum_size = Vector2(width, 0.0)
	host.add_child(content)
	return host

func _wrap_padded(content: Control, padding_h: int, padding_v: int) -> MarginContainer:
	var host := MarginContainer.new()
	host.add_theme_constant_override("margin_left", padding_h)
	host.add_theme_constant_override("margin_right", padding_h)
	host.add_theme_constant_override("margin_top", padding_v)
	host.add_theme_constant_override("margin_bottom", padding_v)
	host.add_child(content)
	return host

## A cost/output amount, whole where it is whole — a recipe asking for 12 fibre should not say 12.0.
func _amount_text(amount: float) -> String:
	if absf(amount - roundf(amount)) < HudCraftingVocab.COST_WHOLE_EPSILON:
		return str(int(roundf(amount)))
	return String.num(amount, HudCraftingVocab.COST_AMOUNT_DECIMALS)

# ---- geometry ---------------------------------------------------------------

## The room the card may use. Unlike `BandComposeFloat` this panel is not anchored to another card —
## it is its own surface and is centred in what is left.
##
## **"WHAT IS LEFT" IS THE ROOM NOTHING ELSE HAS CLAIMED, NOT THE RAW VIEWPORT.** Two different
## neighbours can take it away. A docked panel RESERVES a strip of one screen edge and the map and
## the HUD both live in the remainder; the event bar reserves nothing but is DRAWN over a band of
## its edge. A card measured against the whole window grows over the first and under the second —
## both were reported in play, the second as the panel's own title drawn through a top-docked bar.
## `AutoSizingPanel.room_bounds` is the one seam for both: the controller hands the panel the HUD's
## `FloatingRoom`, which is `LayoutRoot` (inset by every reservation) pulled further off every
## overlay, and `available_room` applies this panel's own clearance to it.
func _room() -> Rect2:
	return available_room(HudCraftingVocab.VIEWPORT_MARGIN)

func _place() -> void:
	var room := _room()
	position = Vector2(
		room.position.x + maxf((room.size.x - size.x) * 0.5, 0.0),
		room.position.y + maxf((room.size.y - size.y) * 0.5, 0.0))

func _header_height() -> float:
	if _header == null:
		return 0.0
	return _header.get_combined_minimum_size().y + HudCraftingVocab.COLUMN_SEPARATOR_THICKNESS

## The room the vertical scrollbar needs, whether or not it is currently shown. Reserved
## unconditionally: the ceiling here is the VIEWPORT, so a taller or shorter window turns the internal
## scrollbar on and off, and a gutter reserved only while scrolling would jump the card's width.
func _scroll_gutter() -> float:
	if _scroll == null:
		return 0.0
	return _scroll.get_v_scroll_bar().get_combined_minimum_size().x
