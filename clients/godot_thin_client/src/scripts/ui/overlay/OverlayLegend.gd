class_name OverlayLegend

## THE OVERLAY LEGEND RENDERER — one channel's title, description and readout, built into a container
## the caller owns.
##
## **EVERYTHING HERE IS `static` AND STATELESS**, the `SourceForecast` shape: no node held, no view
## held, no cached channel. Every input arrives as a parameter, which is what makes it safe to call
## from the picker's popover today and from anywhere else later.
##
## **NO CHANNEL IS NAMED IN THIS FILE, and none may be.** It renders whichever `legend_kind` the
## descriptor declares:
##
## | kind | renders |
## |---|---|
## | `KIND_RAMP` | the swatch/label/value rows of the channel's own legend — the scalar ramp's Low / Average / High, or whatever rows `MapView` built for a channel with a legend of its own (pasture and forage both have one) |
## | `KIND_FACTS` | the count lines the descriptor's provider answered, one per row |
## | `KIND_NONE` | the title and description alone |
##
## A channel that wants a different readout declares a kind, never a branch here. The panel this
## replaced had the opposite shape: two channels' titles and descriptions were written out by hand in
## a refresh function, so a third would have been a third pair of literals.
##
## **THE RAMP ROWS ARE `MapView`'s OWN, NOT RE-DERIVED HERE.** They arrive through
## `overlay_legend_changed` / `MapView.current_overlay_legend()`, the dict the right-dock legend card
## already renders. Re-deriving min/avg/max from `overlay_stats_for_key` instead would report the
## map-wide minimum for every channel, which for pasture and forage is the sea — the exact reading
## those two channels' own legend builders exist to avoid (`overlay-channels.md` → "Zero pasture is
## NOT low pasture"). One producer, two surfaces, no way for them to disagree.

const KIND_NONE := &"none"
const KIND_RAMP := &"ramp"
const KIND_FACTS := &"facts"

## The marker a channel wears when the sim is publishing a shape but no telemetry behind it.
const STUB_MARKER := "stub data"

const TITLE_FONT_SIZE := 13
const BODY_FONT_SIZE := 12
const ROW_FONT_SIZE := 12
const ROW_SEPARATION := 3
const SWATCH_SIZE := Vector2(11.0, 11.0)
const SWATCH_GAP := 7
## The legend's own width floor, so a one-row channel and a five-row one open the same popover.
const MIN_WIDTH := 236.0

const EMPTY_RAMP_TEXT := "No samples received yet."

## Render `descriptor` into `body`, replacing whatever it held.
##
## `legend` is `MapView`'s dict for the ACTIVE channel (`{title, description, rows:[{color, label,
## value_text}]}`) and is read only by `KIND_RAMP`; `facts` is what `OverlayChannels.facts_for`
## answered and is read only by `KIND_FACTS`. Passing the wrong one is inert rather than wrong.
static func render(body: VBoxContainer, descriptor: Dictionary, legend: Dictionary, facts: PackedStringArray) -> void:
	# **`remove_child` BEFORE `queue_free`, and it is not tidiness.** `queue_free` deletes at the END
	# of the frame, so the outgoing rows are still counted while the incoming ones are added — the
	# container's minimum height DOUBLES for that frame. A `Control` whose parent is not a container
	# resolves a too-small anchor box by growing and WRITING THE GROWN SIZE BACK INTO ITS OFFSETS, so
	# the next frame's smaller minimum no longer shrinks it: the doubling is permanent, and it
	# compounds on every re-render.
	for child in body.get_children():
		body.remove_child(child)
		child.queue_free()
	body.add_theme_constant_override("separation", ROW_SEPARATION)
	body.custom_minimum_size = Vector2(MIN_WIDTH, 0.0)

	body.add_child(_title_row(descriptor))

	var description := String(descriptor.get("description", "")).strip_edges()
	if description != "":
		body.add_child(_body_label(description, HudStyle.INK_DIM))

	match StringName(descriptor.get("legend_kind", KIND_RAMP)):
		KIND_NONE:
			return
		KIND_FACTS:
			for line in facts:
				body.add_child(_body_label(String(line), HudStyle.INK))
		_:
			_render_ramp(body, legend)

## The channel's name, plus the stub marker when it is one.
static func _title_row(descriptor: Dictionary) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", SWATCH_GAP)
	var title := Label.new()
	title.text = String(descriptor.get("label", ""))
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title.add_theme_font_size_override("font_size", TITLE_FONT_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK)
	row.add_child(title)
	if bool(descriptor.get("placeholder", false)):
		var stub := Label.new()
		stub.text = STUB_MARKER
		stub.add_theme_font_size_override("font_size", BODY_FONT_SIZE)
		stub.add_theme_color_override("font_color", HudStyle.WARN)
		row.add_child(stub)
	return row

static func _render_ramp(body: VBoxContainer, legend: Dictionary) -> void:
	var rows: Array = legend.get("rows", [])
	if rows.is_empty():
		body.add_child(_body_label(EMPTY_RAMP_TEXT, HudStyle.INK_FAINT))
		return
	for entry in rows:
		if typeof(entry) != TYPE_DICTIONARY:
			continue
		body.add_child(_ramp_row(entry as Dictionary))

static func _ramp_row(entry: Dictionary) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", SWATCH_GAP)

	var swatch := ColorRect.new()
	swatch.custom_minimum_size = SWATCH_SIZE
	swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	swatch.color = entry.get("color", Color.WHITE)
	row.add_child(swatch)

	var label := Label.new()
	label.text = String(entry.get("label", ""))
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.add_theme_font_size_override("font_size", ROW_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.INK_DIM)
	row.add_child(label)

	var value_text := String(entry.get("value_text", "")).strip_edges()
	if value_text != "":
		var value := Label.new()
		value.text = value_text
		value.add_theme_font_size_override("font_size", ROW_FONT_SIZE)
		value.add_theme_color_override("font_color", HudStyle.INK)
		row.add_child(value)
	return row

static func _body_label(text: String, color: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.custom_minimum_size = Vector2(MIN_WIDTH, 0.0)
	label.add_theme_font_size_override("font_size", BODY_FONT_SIZE)
	label.add_theme_color_override("font_color", color)
	return label
