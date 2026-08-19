extends Control
class_name HarvestFloorChart

## **THE HARVEST FLOOR'S INSTRUMENT** — the stock and its consequence in ONE picture
## (`docs/plan_harvest_floor.md` §7.3).
##
## The compose sheet used to state the floor twice and show it nowhere: a row of presets, a plain
## slider under them, and a stock readout somewhere else entirely. This is the merge. One y-axis —
## fraction of carrying capacity — carries all four things at once:
##
##   • the STANDING STOCK, as a band from the baseline up to `B/K`, tinted by the phase the source
##     itself reports (the old stock bar, laid down in the chart's own coordinates);
##   • the FLOOR, a horizontal line that is also the panel's PRIMARY CONTROL — drag it, or drive it
##     from the keyboard, and everything above re-reads;
##   • the PROJECTION beneath it, the stock's trajectory under this crew at this floor;
##   • the FOOD PEAK, marked where the sampled growth curve actually peaks.
##
## Behind all four, the PHASE ZONES: Collapsing / Stressed / Thriving as horizontal bands at the
## source's own cut points, so the floor is dragged against the ecology rather than against a number
## the player has to remember.
##
## Plus the LEARNING RAIL down the right edge, a gradient encoding `floor / the food peak` with a
## marker at the current value. **The rail is a fact about THESE PEOPLE ON THIS GROUND** — how fast
## this crew learns from this source — not a knowledge meter. A tile knows nothing.
##
## **THIS SCRIPT DRAWS; IT DOES NOT MODEL.** Every number it renders comes from
## `SourceForecast.floor_chart_model` — the projection walks the sim's own sampled growth curve, the
## peak is derived from those samples rather than restated as a constant, and negative samples are
## carried through as DECLINE. Nothing here re-derives an ecology.
##
## **WHY THE DRAG COMMITS ON RELEASE.** Every floor change rebuilds the whole compose sheet, which
## frees this node — and a freed node stops receiving the motion events of a drag in flight. So the
## drag emits `floor_changed(f, committed = false)` while it runs (the sheet updates the verdict and
## the crew targets in place, without a rebuild) and `committed = true` on release, keyboard step or
## preset. The alternative — rebuilding per motion event — cannot work, not merely costs more.

## The floor the player is dialling. `committed` false means "still dragging": update what can be
## updated in place and do NOT rebuild the controls, or the drag dies with this node.
signal floor_changed(floor: float, committed: bool)

## Chart box. The width is the compose sheet's content width (it expands to fill); the height is the
## budget freed by merging the stock bar and the projection into one element, and it is the smallest
## box in which a 60-turn curve, three horizontal marks and a floor flag all stay legible.
const CHART_MIN_WIDTH := 300.0
const CHART_HEIGHT := 132.0
## Plot insets. The right margin is the widest: it holds the floor's arrowhead, the learning rail and
## the rail's rotated caption, none of which may overlap the curve.
const PLOT_MARGIN_LEFT := 4.0
const PLOT_MARGIN_RIGHT := 58.0
const PLOT_MARGIN_TOP := 8.0
const PLOT_MARGIN_BOTTOM := 14.0

## The plot's own backing, so the instrument reads as a box rather than as marks on the card.
const PLOT_BACKING_ALPHA := 0.35
## The phase ZONES — Collapsing / Stressed / Thriving as horizontal bands across the whole plot, in
## the same green/amber/red the standing stock wears. They are the FURTHEST-BACK layer and the faintest
## thing drawn: they are the ground the floor is dragged against, not a reading of their own, and at
## the stock band's own 0.10 they would compete with it for the same hue.
const ZONE_FILL_ALPHA := 0.10
## A hairline on each boundary, so the eye can find a threshold the fills alone only suggest — a floor
## is set BY these edges, and a soft gradient of alpha is not a number.
const ZONE_EDGE_ALPHA := 0.40
const ZONE_EDGE_WIDTH := 1.0
## The standing-stock band — the old stock bar, in the chart's coordinates. Low alpha because the
## projection curve is drawn over it and must stay the brighter thing.
const STOCK_BAND_ALPHA := 0.10
const STOCK_EDGE_ALPHA := 0.55
const STOCK_EDGE_WIDTH := 1.0
## The food peak: a hairline plus a small caption, in the "this is the good outcome" green.
const PEAK_LINE_ALPHA := 0.45
const PEAK_LINE_WIDTH := 1.0
const PEAK_LABEL := "best harvest"
const PEAK_LABEL_FONT_SIZE := 9
const PEAK_LABEL_OFFSET := Vector2(4.0, -3.0)
## The projection curve.
const SERIES_LINE_WIDTH := 1.6
const SERIES_AREA_ALPHA := 0.13
const SERIES_START_DOT_RADIUS := 3.0
## The floor line + the arrowhead that marks it as the thing you grab.
const FLOOR_LINE_WIDTH := 1.5
const FLOOR_ARROW_LENGTH := 7.0
const FLOOR_ARROW_HALF_HEIGHT := 5.0
## **THE FLOOR'S FLAG: THE PERCENT, THEN THE QUANTITY** — `leave 50% · 98` on a patch,
## `leave 50% · ≈11 Red Deer` on a herd. ONE format for both webs, because the order follows from what
## the control IS, not from what happens to be standing on it.
##
## **A flag on a draggable control has to move when you drag it.** Biomass has a value per
## `FLOOR_STEP`; an animal count over a K of ~21 has ~21, so an animal-first flag sits unmoved across
## a tenth of the axis and reads as a stuck control. Once the percent has to lead for that reason on
## fauna, leading with it on flora costs nothing and stops the same control swapping its terms when
## the player clicks from a patch to a herd.
##
## It is also the honest order. The sim's floor IS a K-fraction (`escapement_ceiling` = `B − floor·K`,
## quantised later at the kill) and `classify_ecology_phase`'s cut points are fractions of that same
## K — so the percent is the axis and the quantity is the gloss. WHICH UNIT that quantity is in is
## `SourceForecast.stock_face`'s business, never this script's: nothing here branches on the web.
const FLOOR_FLAG_FORMAT := "leave %d%% · %s"
const FLOOR_FLAG_STRIP := "leave nothing"
## **THE FLOOR IS CLIMBING** — appended while a build is raising this source's capacity, so the animal
## count in the flag is a number in motion rather than a standing one. It marks the DIRECTION and
## nothing else: the wire states no next-turn capacity, so a magnitude here would be invented. The
## STRIP face never takes it — a floor of nothing has no count to move.
const FLOOR_FLAG_CLIMBING_SUFFIX := " ↑"
const FLOOR_FLAG_FONT_SIZE := 10
const FLOOR_FLAG_PAD := 3.0
const FLOOR_FLAG_INSET := 5.0
## Above this the flag flips BELOW the line, or it would be drawn off the top of the plot.
const FLOOR_FLAG_FLIP_ABOVE := 0.85
const FLOOR_FLAG_ABOVE_OFFSET := -6.0
const FLOOR_FLAG_BELOW_OFFSET := 13.0
const FLOOR_FLAG_BACKING_ALPHA := 0.82

## The learning rail — brighter the more is left standing, because that is what `learn_multiplier`
## does. Drawn as ONE polygon with per-vertex colours: Godot has no gradient fill primitive, and the
## obvious substitute — a stack of thin bands — reads as STRIPES, because each band has to overlap its
## neighbour to avoid a sub-pixel seam and the overlap then doubles the alpha along it.
const RAIL_GAP := 16.0
const RAIL_WIDTH := 9.0
const RAIL_ALPHA_TOP := 0.60
const RAIL_ALPHA_BOTTOM := 0.03
const RAIL_BORDER_ALPHA := 0.25
const RAIL_MARKER_OVERHANG := 2.0
const RAIL_MARKER_HALF_HEIGHT := 1.5
const RAIL_CAPTION := "LEARNS FASTER ↑"
const RAIL_CAPTION_FONT_SIZE := 9
const RAIL_CAPTION_ALPHA := 0.55
const RAIL_CAPTION_GAP := 13.0
## The rail's marker goes inert at floor 0: nothing is left standing, so nothing is learned.
const RAIL_MARKER_INERT_ALPHA := 0.6

## The x-axis, stated once in the corner rather than as a tick scale — the shape is the reading.
const HORIZON_CAPTION_FORMAT := "%d turns"
const HORIZON_CAPTION_FONT_SIZE := 9
const HORIZON_CAPTION_INSET := Vector2(44.0, -4.0)

## Keyboard steps. Fine is the display resolution the floor is quoted at (whole percent); coarse is
## the same 5% the retired slider used, so a player who learned that granularity keeps it.
const FLOOR_KEY_STEP := 1.0 / float(SourceForecast.FLOOR_PERCENT_SCALE)
const FLOOR_KEY_STEP_COARSE := SourceForecast.FLOOR_STEP
## The focus ring, so a keyboard user can see which control the arrows are driving.
const FOCUS_RING_ALPHA := 0.5
const FOCUS_RING_WIDTH := 1.0

var _model: Dictionary = {}
var _dragging := false

func _init() -> void:
	custom_minimum_size = Vector2(CHART_MIN_WIDTH, CHART_HEIGHT)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# The floor is the primary control of the whole panel, so it has to be reachable without a mouse.
	focus_mode = Control.FOCUS_ALL
	mouse_filter = Control.MOUSE_FILTER_STOP

## Adopt a `SourceForecast.floor_chart_model`. A model with `known == false` draws nothing but its
## backing — a source with no capacity, no published curve, or a rung-3 managed one has no floor axis
## for this instrument to show.
##
## **The cursor is set from `known`, because it is the only affordance the drag has.** The whole plot
## is the drag target (grabbing a 1px line would be unusable), so nothing about the chart's shape says
## it is draggable — the pointer has to. `CURSOR_VSIZE` over the entire control is the prototype's
## `cursor: ns-resize` on the chart element, and it is deliberately NOT scoped to the floor line: the
## line is where the value IS, not where you have to press. An unknown model keeps the plain arrow,
## since there is no floor axis to drag.
func set_model(model: Dictionary) -> void:
	_model = model
	mouse_default_cursor_shape = \
		Control.CURSOR_VSIZE if _has_floor_axis() else Control.CURSOR_ARROW
	queue_redraw()

## Whether this chart has a dial at all — the one test the cursor, the drag and the keyboard share, so
## a chart that draws no floor can never be moved by one of them and not the others.
func _has_floor_axis() -> bool:
	return bool(_model.get("known", false))

## What `crew()` answers for a model that names no crew — an unknown source, which returns before the
## key is written. A SENTINEL rather than `0`, because "no crew stated" and "a crew of nobody" are
## different findings and an assertion comparing this against a stepper must fail loudly on the first.
const CREW_ABSENT := -1

## **THE CREW THIS CHART IS CURRENTLY DRAWN AGAINST**, read off the live model rather than remembered
## at build time, so a live refresh (`set_model` without a rebuild) can never leave it stale. It is
## public for the harnesses: the sheets must resolve their worker cap BEFORE composing this model, and
## the only assertion that can see a sheet which did not is chart-crew against the settled stepper
## value — a disagreement that lasts one frame and is invisible in a capture.
func crew() -> int:
	return int(_model.get("workers", CREW_ABSENT))

func _draw() -> void:
	var plot := _plot_rect()
	if plot.size.x <= 0.0 or plot.size.y <= 0.0:
		return
	var backing := HudStyle.PANEL_SOLID
	backing.a = PLOT_BACKING_ALPHA
	draw_rect(plot, backing)
	if has_focus():
		var ring := HudStyle.SIGNAL
		ring.a = FOCUS_RING_ALPHA
		draw_rect(plot, ring, false, FOCUS_RING_WIDTH)
	if not bool(_model.get("known", false)):
		return
	_draw_phase_zones(plot)
	_draw_stock_band(plot)
	_draw_peak(plot)
	_draw_projection(plot)
	_draw_floor(plot)
	_draw_learning_rail(plot)
	var font: Font = ThemeDB.fallback_font
	draw_string(font,
		Vector2(plot.position.x + plot.size.x - HORIZON_CAPTION_INSET.x,
			size.y + HORIZON_CAPTION_INSET.y),
		HORIZON_CAPTION_FORMAT % SourceForecast.PROJECTION_HORIZON_TURNS,
		HORIZONTAL_ALIGNMENT_LEFT, -1, HORIZON_CAPTION_FONT_SIZE, HudStyle.INK_FAINT)

## **THE PHASE ZONES** (§7.3) — Collapsing / Stressed / Thriving as horizontal bands, drawn FIRST so
## everything else stands on them. The bands and the floor line are in one coordinate system on
## purpose: a floor is a fraction of `K` and so is a phase boundary, so the player drags the line
## against the bands and reads "this crew stops in Stressed" off the picture instead of a number.
##
## The zones come from `SourceForecast.phase_zones` — the SOURCE's own cut points, off the wire — and
## are tinted through the same `DetailFormat.ecology_tier_color` the stock band and the roster dot
## wear, so no second phase palette exists to drift. A source whose cuts the wire did not state draws
## no zones at all rather than a guessed ladder.
func _draw_phase_zones(plot: Rect2) -> void:
	var zones: Array = _model.get("phase_zones", [])
	for entry in zones:
		var zone: Dictionary = entry
		var top := _y(plot, float(zone.get("high", 0.0)))
		var bottom := _y(plot, float(zone.get("low", 0.0)))
		var tier := DetailFormat.ecology_tier_color(String(zone.get("phase", "")))
		var fill := tier
		fill.a = ZONE_FILL_ALPHA
		draw_rect(Rect2(plot.position.x, top, plot.size.x, bottom - top), fill)
		# ONE hairline per real THRESHOLD: a band's upper edge is where the phase word changes hands,
		# except on the topmost band, whose `1.0` is the plot's own ceiling and separates nothing.
		if float(zone.get("high", 0.0)) >= 1.0:
			continue
		var edge := tier
		edge.a = ZONE_EDGE_ALPHA
		draw_line(Vector2(plot.position.x, top), Vector2(plot.position.x + plot.size.x, top),
			edge, ZONE_EDGE_WIDTH)

## THE STANDING STOCK, as a band from the baseline to `B/K` — the stock bar, merged. Its tint is the
## source's OWN reported phase (`DetailFormat.ecology_tier_color`), the same green/amber/red the
## roster dot wears, so the two surfaces can never disagree about one herd's health.
func _draw_stock_band(plot: Rect2) -> void:
	var stock := float(_model.get("stock_fraction", 0.0))
	var tier := DetailFormat.ecology_tier_color(String(_model.get("phase", "")))
	var fill := tier
	fill.a = STOCK_BAND_ALPHA
	var top := _y(plot, stock)
	draw_rect(Rect2(plot.position.x, top, plot.size.x, _y(plot, 0.0) - top), fill)
	var edge := tier
	edge.a = STOCK_EDGE_ALPHA
	draw_line(Vector2(plot.position.x, top), Vector2(plot.position.x + plot.size.x, top),
		edge, STOCK_EDGE_WIDTH)

## THE FOOD PEAK — read off the sampled curve (`growth_peak_fraction`), never restated as the floor
## constant beside it: one number derived two ways is how the mark and the curve start disagreeing.
func _draw_peak(plot: Rect2) -> void:
	var peak := float(_model.get("peak_fraction", SourceForecast.FLOOR_FOOD_PEAK))
	var y := _y(plot, peak)
	var tint := HudStyle.HEALTHY
	tint.a = PEAK_LINE_ALPHA
	draw_line(Vector2(plot.position.x, y), Vector2(plot.position.x + plot.size.x, y),
		tint, PEAK_LINE_WIDTH)
	draw_string(ThemeDB.fallback_font,
		Vector2(plot.position.x + PEAK_LABEL_OFFSET.x, y + PEAK_LABEL_OFFSET.y),
		PEAK_LABEL, HORIZONTAL_ALIGNMENT_LEFT, -1, PEAK_LABEL_FONT_SIZE, tint)

## THE PROJECTION — the stock's trajectory under this crew at this floor. A curve that falls to the
## baseline and stays there is a source being emptied; on a herd past its Allee point that is
## extinction, which is exactly why the model does not clamp its negative regrowth samples.
func _draw_projection(plot: Rect2) -> void:
	var series: PackedFloat32Array = _model.get("series", PackedFloat32Array())
	if series.size() < 2:
		return
	var points := PackedVector2Array()
	var step := plot.size.x / float(series.size() - 1)
	for i in range(series.size()):
		points.push_back(Vector2(plot.position.x + float(i) * step, _y(plot, series[i])))
	var area := PackedVector2Array(points)
	area.push_back(Vector2(points[points.size() - 1].x, _y(plot, 0.0)))
	area.push_back(Vector2(points[0].x, _y(plot, 0.0)))
	var fill := HudStyle.SIGNAL
	fill.a = SERIES_AREA_ALPHA
	draw_colored_polygon(area, fill)
	draw_polyline(points, HudStyle.SIGNAL, SERIES_LINE_WIDTH, true)
	draw_circle(points[0], SERIES_START_DOT_RADIUS, HudStyle.SIGNAL)

## THE FLOOR — the line, its grab arrow and the value it stands for. Drawn LAST of the plot marks so
## the control is never behind the data it governs.
func _draw_floor(plot: Rect2) -> void:
	var floor_value := SourceForecast.clamp_floor(float(_model.get("floor", 0.0)))
	var y := _y(plot, floor_value)
	var right := plot.position.x + plot.size.x
	draw_line(Vector2(plot.position.x, y), Vector2(right, y), HudStyle.INK, FLOOR_LINE_WIDTH)
	draw_colored_polygon(PackedVector2Array([
		Vector2(right, y),
		Vector2(right + FLOOR_ARROW_LENGTH, y - FLOOR_ARROW_HALF_HEIGHT),
		Vector2(right + FLOOR_ARROW_LENGTH, y + FLOOR_ARROW_HALF_HEIGHT),
	]), HudStyle.INK)
	var capacity := float(_model.get("capacity", 0.0))
	var text := FLOOR_FLAG_STRIP if floor_value <= SourceForecast.FLOOR_MIN \
		else _floor_flag_text(floor_value, floor_value * capacity)
	var font: Font = ThemeDB.fallback_font
	var text_size := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, FLOOR_FLAG_FONT_SIZE)
	var flag_x := plot.position.x + FLOOR_FLAG_INSET
	var flag_y := y + (FLOOR_FLAG_BELOW_OFFSET if floor_value > FLOOR_FLAG_FLIP_ABOVE \
		else FLOOR_FLAG_ABOVE_OFFSET)
	var backing := HudStyle.GROUND
	backing.a = FLOOR_FLAG_BACKING_ALPHA
	draw_rect(Rect2(flag_x - FLOOR_FLAG_PAD, flag_y - text_size.y + FLOOR_FLAG_PAD,
		text_size.x + FLOOR_FLAG_PAD * 2.0, text_size.y), backing)
	draw_string(font, Vector2(flag_x, flag_y), text,
		HORIZONTAL_ALIGNMENT_LEFT, -1, FLOOR_FLAG_FONT_SIZE, HudStyle.INK)

## The flag's words for a floor that leaves something standing. **No branch on the web**: the quantity
## comes from `stock_face`, which the at-floor verdict under this chart also reads, so the two can
## only ever name the threshold identically. See `FLOOR_FLAG_FORMAT` for why the percent leads.
func _floor_flag_text(floor_value: float, floor_stock: float) -> String:
	var text := FLOOR_FLAG_FORMAT % [SourceForecast.floor_percent(floor_value),
		SourceForecast.stock_face(floor_stock, float(_model.get("body_mass", 0.0)),
			String(_model.get("quarry", "")))]
	return text + FLOOR_FLAG_CLIMBING_SUFFIX if bool(_model.get("floor_climbing", false)) else text

## THE LEARNING RAIL — `learn_multiplier` as a gradient, brightest where the most is left standing,
## with a marker at the floor. It answers "are these people, on this ground, contributing to the
## ladder, and how strongly?" — which is a fact about the assignment, not about the faction.
func _draw_learning_rail(plot: Rect2) -> void:
	var x := plot.position.x + plot.size.x + RAIL_GAP
	var top := _y(plot, 1.0)
	var bottom := _y(plot, 0.0)
	var bright := HudStyle.SIGNAL
	bright.a = RAIL_ALPHA_TOP
	var faint := HudStyle.SIGNAL
	faint.a = RAIL_ALPHA_BOTTOM
	draw_polygon(
		PackedVector2Array([
			Vector2(x, top), Vector2(x + RAIL_WIDTH, top),
			Vector2(x + RAIL_WIDTH, bottom), Vector2(x, bottom),
		]),
		PackedColorArray([bright, bright, faint, faint]))
	var border := HudStyle.SIGNAL
	border.a = RAIL_BORDER_ALPHA
	draw_rect(Rect2(x, top, RAIL_WIDTH, bottom - top), border, false)
	var floor_value := SourceForecast.clamp_floor(float(_model.get("floor", 0.0)))
	var marker := HudStyle.SIGNAL
	if floor_value <= SourceForecast.FLOOR_MIN:
		marker = HudStyle.INK_FAINT
		marker.a = RAIL_MARKER_INERT_ALPHA
	var marker_y := _y(plot, floor_value)
	draw_rect(Rect2(x - RAIL_MARKER_OVERHANG, marker_y - RAIL_MARKER_HALF_HEIGHT,
		RAIL_WIDTH + RAIL_MARKER_OVERHANG * 2.0, RAIL_MARKER_HALF_HEIGHT * 2.0), marker)
	var caption := HudStyle.SIGNAL
	caption.a = RAIL_CAPTION_ALPHA
	# **ROTATED THE WAY THE GRADIENT RUNS** — bottom-to-top, so the caption's ↑ points at the bright end
	# it names. The other rotation reads the words downward with an up-arrow at the end of them.
	draw_set_transform(Vector2(x + RAIL_WIDTH + RAIL_CAPTION_GAP, top + (bottom - top) * 0.5),
		-PI * 0.5, Vector2.ONE)
	var font: Font = ThemeDB.fallback_font
	var caption_width := font.get_string_size(RAIL_CAPTION, HORIZONTAL_ALIGNMENT_LEFT, -1,
		RAIL_CAPTION_FONT_SIZE).x
	draw_string(font, Vector2(-caption_width * 0.5, 0.0), RAIL_CAPTION,
		HORIZONTAL_ALIGNMENT_LEFT, -1, RAIL_CAPTION_FONT_SIZE, caption)
	draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)

func _plot_rect() -> Rect2:
	return Rect2(
		PLOT_MARGIN_LEFT,
		PLOT_MARGIN_TOP,
		maxf(size.x - PLOT_MARGIN_LEFT - PLOT_MARGIN_RIGHT, 0.0),
		maxf(size.y - PLOT_MARGIN_TOP - PLOT_MARGIN_BOTTOM, 0.0))

## Fraction of capacity → y. Up is more standing, which is the direction "leave more" reads in.
func _y(plot: Rect2, fraction: float) -> float:
	return plot.position.y + (1.0 - clampf(fraction, 0.0, 1.0)) * plot.size.y

func _gui_input(event: InputEvent) -> void:
	# **A chart with no floor axis accepts nothing.** It draws only its backing, so a press on one
	# would otherwise emit a floor for a source that has none — and the sheet would commit it. The
	# guard is the same `_has_floor_axis` the cursor reads, so the pointer cannot promise a drag the
	# handler then refuses.
	if not _has_floor_axis():
		return
	if event is InputEventMouseButton:
		var button := event as InputEventMouseButton
		if button.button_index != MOUSE_BUTTON_LEFT:
			return
		if button.pressed:
			_dragging = true
			grab_focus()
			_emit_from_y(button.position.y, false)
		elif _dragging:
			_dragging = false
			# COMMIT on release — the rebuild this triggers frees this node, which is precisely why
			# it cannot happen while the drag is live.
			_emit_from_y(button.position.y, true)
		accept_event()
	elif event is InputEventMouseMotion and _dragging:
		_emit_from_y((event as InputEventMouseMotion).position.y, false)
		accept_event()
	elif event is InputEventKey and (event as InputEventKey).pressed:
		_handle_key(event as InputEventKey)

## Keyboard access to the same dial. Arrows step (Shift takes the coarse step), Home strips the
## source and End leaves it untouched — the two ends a player reaches for deliberately.
func _handle_key(key: InputEventKey) -> void:
	var floor_value := SourceForecast.clamp_floor(float(_model.get("floor", 0.0)))
	var step := FLOOR_KEY_STEP_COARSE if key.shift_pressed else FLOOR_KEY_STEP
	match key.keycode:
		KEY_UP, KEY_RIGHT:
			_emit_floor(floor_value + step, true)
		KEY_DOWN, KEY_LEFT:
			_emit_floor(floor_value - step, true)
		KEY_HOME:
			_emit_floor(SourceForecast.FLOOR_MIN, true)
		KEY_END:
			_emit_floor(SourceForecast.FLOOR_MAX, true)
		_:
			return
	accept_event()

func _emit_from_y(local_y: float, committed: bool) -> void:
	var plot := _plot_rect()
	if plot.size.y <= 0.0:
		return
	_emit_floor(1.0 - (local_y - plot.position.y) / plot.size.y, committed)

## Quantised to whole percent — the resolution `SourceForecast.floor_percent` displays at, so the
## flag, the presets' selection test and the command all agree on where the dial is.
func _emit_floor(floor_value: float, committed: bool) -> void:
	var quantised := float(SourceForecast.floor_percent(floor_value)) \
		/ float(SourceForecast.FLOOR_PERCENT_SCALE)
	_model["floor"] = quantised
	queue_redraw()
	emit_signal("floor_changed", quantised, committed)
