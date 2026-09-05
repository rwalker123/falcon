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
## | `KIND_RAMP` | the swatch/label/value rows of the channel's own legend — the scalar ramp's Low / Average / High, or whatever rows `MapView` built for a channel with a legend of its own (pasture, forage and the bare map's biome key all have one) |
## | `KIND_FACTS` | the count lines the descriptor's provider answered, one per row |
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

const KIND_RAMP := &"ramp"
const KIND_FACTS := &"facts"

## The SWATCH SHAPES a ramp row can ask for, declared on the row itself. Same property the
## `legend_kind` table above has and for the same reason: a row states what it IS, and this file
## draws whichever shape it is handed. **No channel may be named here** — a swatch that looked like
## the mark its channel draws would otherwise cost an `if key ==` per channel forever.
##
## `SWATCH_KIND_SOLID` is the DEFAULT, so every row written before this existed keeps its flat
## `color` with no edit; a texture row still wins over both (see `_swatch`).
const SWATCH_KIND_SOLID := &"solid"
## A DIAGONAL HATCH, for a channel whose mark on the map is drawn rather than filled. The row carries
## the colour and the direction — **it does not declare them here** — so the swatch is struck from the
## same constants the map's own pass is (`MapView.TEMPERATURE_HATCH_COLOR` /
## `TEMPERATURE_HATCH_DIRECTION`), the rule the pasture and forage ramps already keep. An optional
## `edge_color` outlines the box, for a mark the map draws a contour around as well.
const SWATCH_KIND_HATCHED := &"hatched"

## The marker a channel wears when the sim is publishing a shape but no telemetry behind it.
const STUB_MARKER := "stub data"

const TITLE_FONT_SIZE := 13
const BODY_FONT_SIZE := 12
const ROW_FONT_SIZE := 12
const ROW_SEPARATION := 3
const SWATCH_SIZE := Vector2(11.0, 11.0)
## A TEXTURE swatch is bigger than a colour one, and has to be: it is a hex-masked tile, so most of
## its box is transparent and the material only reads once the hex itself does. At `SWATCH_SIZE` the
## biomes were indistinguishable smudges.
const TEXTURE_SWATCH_SIZE := Vector2(20.0, 20.0)
## …and a HATCHED swatch is bigger again, for the same reason and settled the same way — RENDERED at
## both sizes and looked at, `OverlayChannels` carrying the autopsy on glyphs that were unreadable at
## exactly `SWATCH_SIZE`. **Measured, not assumed: at `SWATCH_SIZE` this is a red SPECKLE, not
## hatching.** The edge takes 2 px off each side, leaving ~7 px of interior for lines 1.5 px wide, and
## what lands is a dither/checkerboard that reads dirtier than the solid block it replaced — the
## opposite of the thing a hatched swatch exists to state. At 20 px the lines resolve as
## unmistakably diagonal with the contour still legible around them.
##
## It matches `TEXTURE_SWATCH_SIZE` rather than `SWATCH_SIZE`, so a legend mixing this row with flat
## ones has a slightly ragged swatch column. That is the accepted cost: a pattern needs room to be a
## pattern, and an illegible swatch is a worse readout than an uneven margin.
const HATCHED_SWATCH_SIZE := Vector2(20.0, 20.0)
## Perpendicular distance between hatch lines inside the swatch, and their weight. These are the
## SWATCH's geometry, not the map's: a hex at play zoom is several times this box, so reusing the
## map's spacing would put one line in the swatch. The COLOUR and the ANGLE are the map's and arrive
## on the row.
const HATCHED_SWATCH_LINE_SPACING := 4.0
const HATCHED_SWATCH_LINE_WIDTH := 1.5
## The optional edge's weight — heavier than the hatch, mirroring the map, where the contour is the
## mark that survives zooming out.
const HATCHED_SWATCH_EDGE_WIDTH := 2.0
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
	row.add_child(_swatch(entry))

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

## A row's swatch, in the shape the ROW declares: the TEXTURE the map paints it with if it has one,
## else the `swatch_kind` it asked for — hatched for a mark the map DRAWS, and otherwise the flat
## colour every row has always had. Texture first because it is a property of the map's paint mode
## rather than a request, and a textured biome row has no business also being hatched.
##
## **A ROW CARRIES A TEXTURE ONLY WHERE THE MAP IS TEXTURED** — today that is the bare map's biome
## key, and only while the `T` toggle is on (`MapView._build_terrain_legend`). Every scalar channel
## paints a flat lerp of its own ramp, so a colour really is what those rows are drawn with. The
## fallback is a live path, not a guard: a biome the atlas has no layer for answers `null` here.
static func _swatch(entry: Dictionary) -> Control:
	var texture: Variant = entry.get("texture", null)
	if texture is Texture2D:
		var art := TextureRect.new()
		art.texture = texture
		# **`EXPAND_IGNORE_SIZE`, or the swatch is the TEXTURE's size.** A `TextureRect` reports its
		# texture's dimensions as its own minimum by default, and `custom_minimum_size` is a FLOOR
		# rather than a cap — so the hex art came through at its full cache resolution and one biome
		# filled the card. This is what makes the minimum the box asked for.
		art.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		art.custom_minimum_size = TEXTURE_SWATCH_SIZE
		art.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		art.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		art.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		return art
	if StringName(entry.get("swatch_kind", SWATCH_KIND_SOLID)) == SWATCH_KIND_HATCHED:
		return _hatched_swatch(entry)
	var swatch := ColorRect.new()
	swatch.custom_minimum_size = SWATCH_SIZE
	swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	swatch.color = entry.get("color", Color.WHITE)
	return swatch

## A row whose map mark is DRAWN rather than filled: diagonal hatch, optionally boxed by the contour
## the map draws around the same ground.
##
## **THE COLOUR AND THE ANGLE COME OFF THE ROW, NEVER FROM THIS FILE.** They are the map pass's own
## constants, handed over by whoever built the legend, so the swatch and the hexes cannot drift into
## two different reds or two different angles — the rule the pasture and forage ramps keep by sharing
## `_pasture_ramp_color` / `_forage_ramp_color` with their swatches.
##
## **`clip_contents` IS WHAT KEEPS THE LINES IN THE BOX**, rather than per-line chord arithmetic: the
## lines are drawn longer than the box's diagonal and the `Control` trims them. That is what makes the
## swatch honest at ANY angle a row cares to send, without a clipping rule that only holds at 45°.
static func _hatched_swatch(entry: Dictionary) -> Control:
	var swatch := Control.new()
	swatch.custom_minimum_size = HATCHED_SWATCH_SIZE
	swatch.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	swatch.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	swatch.clip_contents = true
	var hatch: Color = entry.get("hatch_color", entry.get("color", Color.WHITE))
	var direction: Vector2 = entry.get("hatch_direction", Vector2.ONE.normalized())
	var edge: Color = entry.get("edge_color", Color(0.0, 0.0, 0.0, 0.0))
	swatch.draw.connect(func() -> void: _draw_hatched_swatch(swatch, hatch, direction, edge))
	return swatch

## Fill `target` with parallel hatch lines along `direction`, spaced across its whole diagonal so the
## box is covered whatever its size, and outline it in `edge` when the row sent one.
static func _draw_hatched_swatch(target: Control, hatch: Color, direction: Vector2, edge: Color) -> void:
	var box := Rect2(Vector2.ZERO, target.size)
	var center: Vector2 = box.size * 0.5
	var line: Vector2 = direction.normalized()
	var normal := Vector2(-line.y, line.x)
	# The box's diagonal bounds every chord through it, so a line of this length always crosses the
	# whole box and `clip_contents` trims the overhang.
	var span: float = box.size.length()
	var half_span: float = span * 0.5
	var offset: float = -half_span
	while offset <= half_span:
		var anchor: Vector2 = center + normal * offset
		target.draw_line(anchor - line * span, anchor + line * span, hatch, HATCHED_SWATCH_LINE_WIDTH)
		offset += HATCHED_SWATCH_LINE_SPACING
	if edge.a > 0.0:
		target.draw_rect(box, edge, false, HATCHED_SWATCH_EDGE_WIDTH)

static func _body_label(text: String, color: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.custom_minimum_size = Vector2(MIN_WIDTH, 0.0)
	label.add_theme_font_size_override("font_size", BODY_FONT_SIZE)
	label.add_theme_color_override("font_color", color)
	return label
