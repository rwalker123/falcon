class_name TribePage
extends RefCounted

## **THE TRIBE'S PAGE — the Band panel's subject one scale up from a band** (`.claude/rules/client/tribe-panel.md`).
## It builds two zones into the same `BandCityPanel`: `band` (the totalled People bar, the Food vital, the
## tribe's one scouting range, the levy ceiling) and `work` (one collapsed line per member band, then the
## tribe's shared works). A fresh instance is built per render; every act it offers is a Callable the
## controller handed in, so this file reads state and emits nothing.

## Collaborators.
var labor: HudBandLaborState = null
var members: Array = []
var tribe_id: int = HudTribeVocab.NO_TRIBE
var tribe_name: String = ""
## `Callable(band) -> String` — the client's one band-naming rule.
var name_of: Callable = Callable()
## The fold state, shared by reference with the controller: `"<tribe_id>:<band_id>" -> true` when open.
var open_folds: Dictionary = {}
var on_changed: Callable = Callable()
## `Callable(band)` — open a member band's own panel.
var on_open_band: Callable = Callable()
## `Callable(tiles: int)` — state the tribe's range.
var on_scout: Callable = Callable()
## `Callable(token: String)` — set the levy ceiling.
var on_levy: Callable = Callable()
## `Callable(work: Dictionary)` — stop one shared work.
var on_stop: Callable = Callable()
## `Callable(band) -> Control` — the band's rows exactly as the Band panel builds them.
var build_rows: Callable = Callable()
## `Callable(branch) -> String`, `Callable(x, y, branch, target_rung) -> String`.
var branch_label: Callable = Callable()
var rung_line: Callable = Callable()
## `Callable() -> ScrollContainer` (or null) — the sanctioned scroll host for a zone's stack.
var make_scroll: Callable = Callable()

# ---- the band zone -----------------------------------------------------------------------------------

func build_band_zone() -> VBoxContainer:
	var col := HudWidgets.make_zone_column()
	col.set_meta(HudTribeVocab.TRIBE_PAGE_META, tribe_id)
	var stack := col
	var scroll: Variant = make_scroll.call(HudTribeVocab.BAND_SCROLL_NAME) if make_scroll.is_valid() else null
	if scroll is ScrollContainer:
		col.add_child(scroll)
		stack = HudWidgets.make_zone_column()
		stack.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
		(scroll as ScrollContainer).add_child(stack)
	stack.add_child(_build_food_label())
	var people := _build_people_block()
	if people != null:
		stack.add_child(people)
	stack.add_child(_build_scout_block())
	stack.add_child(_build_levy_block())
	return col

## The Food vital, summed across the members: the stock is the sum, the net is the sum, and the runway
## is the summed stock over the summed meal (`food_need`) — a band's own page reads the sim's runway, and
## a tribe has no sim runway to read.
func _build_food_label() -> RichTextLabel:
	var stock := 0.0
	var net := 0.0
	var need := 0.0
	for band in members:
		stock += DetailFormat.band_provisions(band)
		net += DetailFormat.band_net_food(band)
		need += DetailFormat.band_food_need(band)
	var turns := BandFoodStatus.UNLIMITED_TURNS
	if need >= SourceForecast.FOOD_FLOW_MIN:
		turns = stock / need
	var ctx := DetailFormat.Context.new()
	ctx.food_turns = turns
	var line := HudTribeVocab.FOOD_ROW_FORMAT % [int(round(stock)), DetailFormat.food_turns_text(turns)]
	line += HudTribeVocab.FOOD_NET_FORMAT % [
		HudStyle.HEALTHY_HEX if net >= 0.0 else HudStyle.DANGER_HEX, SourceForecast.format_yield(net)]
	var label := RichTextLabel.new()
	label.bbcode_enabled = true
	label.fit_content = true
	label.scroll_active = false
	label.autowrap_mode = TextServer.AUTOWRAP_WORD
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.set_meta(HudTribeVocab.FOOD_LABEL_META, true)
	label.text = DetailFormat.detail_bbcode([line], ctx)
	return label

## The one People bar, totalled across the members.
func _build_people_block() -> VBoxContainer:
	var children := 0
	var working := 0
	var idle := 0
	var elders := 0
	var total := 0
	var away := 0
	var parts: Dictionary = {}
	for band in members:
		children += int(band.get("children", 0))
		elders += int(band.get("elders", 0))
		total += int(band.get("size", 0))
		var split := labor.people_split(band)
		working += int(split["working"])
		idle += int(split["idle"])
		away += labor.band_party_workers(band)
		var band_parts := labor.working_breakdown(band)
		for label in band_parts:
			parts[label] = int(parts.get(label, 0)) + int(band_parts[label])
	if total <= 0:
		return null
	var segments := HudWidgets.people_segments(children, working, idle, elders, parts,
		HudWorkVocab.IDLE_CARD_NUMBER_FACTION_FORMAT % idle)
	var block := HudWidgets.make_zone_block()
	block.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PEOPLE,
		HudWidgets.people_head_readout(total, away), null, HudStyle.INK_DIM,
		HudWorkVocab.PEOPLE_AWAY_TOOLTIP_FORMAT % away if away > 0 else ""))
	block.add_child(HudWidgets.build_composition_bar(segments))
	var chip := Label.new()
	chip.text = HudTribeVocab.BANDS_CHIP_FORMAT % members.size()
	chip.add_theme_font_size_override("font_size", HudWorkVocab.COMPOSITION_KEY_FONT_SIZE)
	chip.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	block.add_child(HudWidgets.build_composition_key(segments, chip))
	return block

## The tribe's one scouting range: the stepper, then one line saying how the edge is covered.
func _build_scout_block() -> VBoxContainer:
	var block := HudWidgets.make_zone_block()
	block.add_child(HudWidgets.zone_head(HudTribeVocab.SCOUT_HEAD, ""))
	var anchor := TribeRoster.address_band(members)
	var tiles := int(anchor.get(HudTribeVocab.KEY_SCOUTING_RANGE, 0))
	var stepper := HBoxContainer.new()
	stepper.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
	HudWidgets.add_range_stepper(stepper, tiles, HudBandLaborState.scouting_range_max(anchor),
		func(n: int) -> void: on_scout.call(n))
	block.add_child(stepper)
	var line := scout_edge_line(tiles)
	if line.text != "":
		var label := HudWidgets.alloc_hint_label(line.text)
		label.set_meta(HudTribeVocab.SCOUT_LINE_META, line.text)
		if line.short:
			label.add_theme_color_override("font_color", HudStyle.WARN)
		block.add_child(label)
	return block

## `{text, short}` — "N scouts ride the tribe's edge" when every member's stretch reaches the range,
## else the first short stretch named. Members with a stretch of 0 sit inside the others and are ignored.
func scout_edge_line(tiles: int) -> Dictionary:
	if tiles <= 0:
		return {"text": "", "short": false}
	var riders := 0
	for band in members:
		var row := _scout_row(band)
		if row.is_empty() or int(row.get(HudTribeVocab.ROW_SCOUTING_STRETCH, 0)) <= 0:
			continue
		riders += int(row.get("workers", 0))
		var reach := int(row.get(HudTribeVocab.ROW_SCOUTING_REACH, 0))
		if reach < tiles:
			return {"text": HudTribeVocab.SCOUT_SHORT_FORMAT % [String(name_of.call(band)), reach, tiles],
				"short": true}
	return {"text": HudTribeVocab.SCOUT_EDGE_ONE if riders == 1 else HudTribeVocab.SCOUT_EDGE_FORMAT % riders,
		"short": false}

static func _scout_row(band: Dictionary) -> Dictionary:
	for row_variant in HudBandLaborState.labor_assignments_of(band):
		if row_variant is Dictionary and String((row_variant as Dictionary).get(
				HudTribeVocab.ROW_KIND, "")).to_lower() == HudConst.LABOR_KIND_SCOUT:
			return row_variant
	return {}

## The levy ceiling, a four-way option picker.
func _build_levy_block() -> VBoxContainer:
	var block := HudWidgets.make_zone_block()
	block.add_child(HudWidgets.zone_head(HudTribeVocab.LEVY_HEAD, ""))
	var ceiling := TribeRoster.levy_ceiling(members)
	var entries: Array = []
	var selected := HudWidgets.NO_ENTRY_SELECTED
	for i in range(HudTribeVocab.LEVY_LEVELS.size()):
		var level: Dictionary = HudTribeVocab.LEVY_LEVELS[i]
		if int(level["ceiling"]) == ceiling:
			selected = i
		var token := String(level["token"])
		entries.append({"label": String(level["label"]), "on_pick": func() -> void: on_levy.call(token)})
	var face := String(HudTribeVocab.LEVY_LEVELS[selected]["label"]) if selected >= 0 else ""
	var picker := HudWidgets.build_option_picker(entries, selected, face, HudTribeVocab.LEVY_TOOLTIP)
	picker.set_meta(HudTribeVocab.LEVY_PICKER_META, ceiling)
	block.add_child(picker)
	return block

# ---- the work zone -----------------------------------------------------------------------------------

func build_work_zone() -> VBoxContainer:
	var col := HudWidgets.make_zone_column()
	col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
	var stack := col
	var scroll: Variant = make_scroll.call(HudTribeVocab.WORK_SCROLL_NAME) if make_scroll.is_valid() else null
	if scroll is ScrollContainer:
		col.add_child(scroll)
		stack = HudWidgets.make_zone_column()
		stack.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
		stack.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
		(scroll as ScrollContainer).add_child(stack)
	stack.add_child(_build_bands_section())
	stack.add_child(_build_works_section())
	return col

func _fold_key(band: Dictionary) -> String:
	return "%d:%d" % [tribe_id, int(band.get("band_id", -1))]

func _build_bands_section() -> VBoxContainer:
	var block := HudWidgets.make_zone_block()
	block.add_child(HudWidgets.zone_head(HudTribeVocab.BANDS_HEAD, str(members.size())))
	for band in members:
		block.add_child(_build_band_line(band))
		if open_folds.has(_fold_key(band)):
			var rows := MarginContainer.new()
			rows.add_theme_constant_override("margin_left", HudTribeVocab.BAND_ROWS_INDENT)
			rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			if build_rows.is_valid():
				var body: Variant = build_rows.call(band)
				if body is Control:
					rows.add_child(body as Control)
			block.add_child(rows)
	return block

## One collapsed line: the fold, the band's underlined name (which drills in), the sub-line, and the crew.
func _build_band_line(band: Dictionary) -> HBoxContainer:
	var split := TribeRoster.crew_split(labor, band)
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudTribeVocab.BAND_LINE_SEPARATION)
	line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.set_meta(HudTribeVocab.BAND_LINE_META, int(band.get("band_id", -1)))
	var key := _fold_key(band)
	var is_open := open_folds.has(key)
	line.add_child(build_fold_button(is_open,
		HudTribeVocab.FOLD_CLOSED_TIP if is_open else HudTribeVocab.FOLD_OPEN_TIP, func() -> void:
			if open_folds.has(key):
				open_folds.erase(key)
			else:
				open_folds[key] = true
			on_changed.call()))
	var text := VBoxContainer.new()
	text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text.add_theme_constant_override("separation", 0)
	var band_name := String(name_of.call(band))
	var link := HudWidgets.build_inline_link(band_name, HudStyle.INK, func() -> void: on_open_band.call(band))
	link.add_theme_font_size_override("font_size", HudTribeVocab.BAND_LINE_FONT_SIZE)
	link.alignment = HORIZONTAL_ALIGNMENT_LEFT
	link.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	link.tooltip_text = HudTribeVocab.OPEN_BAND_TIP % band_name
	link.set_meta(HudTribeVocab.NAME_LINK_META, band_name)
	var underline := StyleBoxFlat.new()
	underline.draw_center = false
	underline.border_width_bottom = HudTribeVocab.NAME_UNDERLINE_WIDTH
	underline.border_color = HudStyle.INK_FAINT
	underline.content_margin_left = 0.0
	underline.content_margin_right = 0.0
	underline.content_margin_top = 0.0
	underline.content_margin_bottom = HudTribeVocab.NAME_UNDERLINE_GAP
	link.add_theme_stylebox_override("normal", underline)
	text.add_child(link)
	var sub := Label.new()
	var sub_text := sub_line(split)
	sub.text = sub_text
	sub.add_theme_font_size_override("font_size", HudTribeVocab.SUB_LINE_FONT_SIZE)
	sub.add_theme_color_override("font_color", HudStyle.INK_DIM)
	sub.clip_text = true
	sub.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	sub.set_meta(HudTribeVocab.SUB_LINE_META, sub_text)
	text.add_child(sub)
	line.add_child(text)
	line.add_child(_build_crew(band_name, split))
	return line

## `N jobs · N for the tribe · N idle`, zero parts dropped.
static func sub_line(split: Dictionary) -> String:
	var parts: Array[String] = []
	var jobs := int(split["jobs"])
	if jobs > 0:
		parts.append(HudTribeVocab.JOBS_ONE if jobs == 1 else HudTribeVocab.JOBS_FORMAT % jobs)
	if int(split["tribe_rows"]) > 0:
		parts.append(HudTribeVocab.FOR_TRIBE_FORMAT % int(split["tribe_rows"]))
	if int(split["idle"]) > 0:
		parts.append(HudTribeVocab.IDLE_FORMAT % int(split["idle"]))
	return HudTribeVocab.SUB_LINE_SEPARATOR.join(parts) if not parts.is_empty() else HudTribeVocab.NO_JOBS_LINE

## The crew number over the three-way crew bar: at home | for the tribe | idle.
func _build_crew(band_name: String, split: Dictionary) -> VBoxContainer:
	var home := int(split["home"])
	var tribe := int(split["tribe"])
	var idle := int(split["idle"])
	var sentence := HudTribeVocab.CREW_TIP_FORMAT % [band_name, home, tribe, idle]
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 0)
	column.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var number := HudWidgets.build_crew_number(home + tribe + idle)
	number.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	number.size_flags_horizontal = Control.SIZE_SHRINK_END
	column.add_child(number)
	var segments: Array = []
	if home > 0:
		segments.append({"count": home, "color": HudStyle.INK_DIM, "tooltip": sentence})
	if tribe > 0:
		segments.append({"count": tribe, "color": HudStyle.SIGNAL, "tooltip": sentence})
	if idle > 0:
		segments.append({"count": idle, "color": HudStyle.WARN, "tooltip": sentence})
	if segments.is_empty():
		segments.append({"count": 1, "color": HudStyle.LINE, "tooltip": sentence})
	var bar := HudWidgets.build_composition_bar(segments)
	bar.custom_minimum_size.x = HudTribeVocab.CREW_BAR_WIDTH
	bar.size_flags_horizontal = Control.SIZE_SHRINK_END
	bar.tooltip_text = sentence
	bar.set_meta(HudTribeVocab.CREW_BAR_META, sentence)
	column.add_child(bar)
	return column

## The fold button the Work zone's section chevrons use: a drawn triangle laid over a flat button.
static func build_fold_button(expanded: bool, tooltip: String, on_press: Callable) -> Button:
	var button := Button.new()
	button.tooltip_text = tooltip
	button.focus_mode = Control.FOCUS_NONE
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	button.custom_minimum_size = Vector2(HudTribeVocab.FOLD_BUTTON_WIDTH, HudTribeVocab.FOLD_BUTTON_HEIGHT)
	button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	HudStyle.apply_button(button, "ghost")
	var triangle := DisclosureTriangle.new()
	triangle.side = HudTribeVocab.FOLD_TRIANGLE_SIDE
	triangle.expanded = expanded
	triangle.color = HudStyle.INK_DIM
	triangle.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	button.add_child(triangle)
	button.set_meta(HudTribeVocab.FOLD_META, expanded)
	button.pressed.connect(func() -> void: on_press.call())
	return button

func _build_works_section() -> VBoxContainer:
	var block := HudWidgets.make_zone_block()
	var shared := TribeRoster.works(members)
	block.add_child(HudWidgets.zone_head(HudTribeVocab.WORKS_HEAD, str(shared.size()) if not shared.is_empty() else ""))
	if shared.is_empty():
		block.add_child(HudWidgets.alloc_hint_label(HudTribeVocab.NO_WORKS_LINE))
		return block
	var names := TribeRoster.names_by_band_id(members, name_of)
	for work_variant in shared:
		if work_variant is Dictionary:
			block.add_child(_build_work_row(work_variant, names))
	return block

## "handed to Greyfen" / "handed to Greyfen, then Pinewold", named off `bands` (nearest first).
static func handed_to(work: Dictionary, names: Dictionary) -> String:
	var who: Array[String] = []
	for band_id in work.get(HudTribeVocab.WORK_BANDS, []):
		who.append(String(names.get(int(band_id), "")))
	if who.is_empty():
		return ""
	return HudTribeVocab.HANDED_ONE_FORMAT % HudTribeVocab.HANDED_MANY_SEPARATOR.join(who)

func _build_work_row(work: Dictionary, names: Dictionary) -> PanelContainer:
	var x := int(work.get(HudTribeVocab.WORK_X, -1))
	var y := int(work.get(HudTribeVocab.WORK_Y, -1))
	var branch := String(work.get(HudTribeVocab.WORK_BRANCH, ""))
	var row := PanelContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(false))
	row.set_meta(HudTribeVocab.WORK_ROW_META, "%d,%d:%s" % [x, y, branch])
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", HudTribeVocab.BAND_LINE_SEPARATION)
	row.add_child(line)
	var col := VBoxContainer.new()
	col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	col.add_theme_constant_override("separation", HudTribeVocab.WORK_ROW_SEPARATION)
	line.add_child(col)
	var title := Label.new()
	title.text = HudTribeVocab.WORK_NAME_FORMAT % [String(branch_label.call(branch)), x, y]
	title.add_theme_font_size_override("font_size", HudTribeVocab.BAND_LINE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK)
	title.clip_text = true
	title.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	col.add_child(title)
	var rung := Label.new()
	rung.text = String(rung_line.call(x, y, branch, String(work.get(HudTribeVocab.WORK_TARGET_RUNG, ""))))
	rung.add_theme_font_size_override("font_size", HudTribeVocab.SUB_LINE_FONT_SIZE)
	rung.add_theme_color_override("font_color", HudStyle.INK_DIM)
	col.add_child(rung)
	var hands := Label.new()
	var workers := int(work.get(HudTribeVocab.WORK_WORKERS, 0))
	var count := HudTribeVocab.WORKERS_ONE if workers == 1 else HudTribeVocab.WORKERS_FORMAT % workers
	var handed := handed_to(work, names)
	hands.text = count if handed == "" else HudTribeVocab.SUB_LINE_SEPARATOR.join(
		PackedStringArray([handed, count]))
	hands.add_theme_font_size_override("font_size", HudTribeVocab.SUB_LINE_FONT_SIZE)
	hands.add_theme_color_override("font_color", HudStyle.INK_DIM)
	hands.clip_text = true
	hands.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	col.add_child(hands)
	var stop := Button.new()
	stop.text = HudTribeVocab.STOP_LABEL
	stop.tooltip_text = HudTribeVocab.STOP_TIP
	stop.focus_mode = Control.FOCUS_NONE
	stop.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	HudStyle.apply_button(stop, "ghost")
	stop.add_theme_color_override("font_color", HudStyle.DANGER)
	stop.pressed.connect(func() -> void: on_stop.call(work))
	line.add_child(stop)
	return row
