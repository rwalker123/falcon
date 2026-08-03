extends RefCounted
class_name WorkbenchWidgets

## All-`static`, stateless widget layer for the Workbench.
##
## Every shared control the shell and its pages draw is built here, and **state is threaded in as
## PARAMETERS — never a `_shell` back-reference**. That rule is the whole point: a "shared" layer
## that holds a handle back to its coordinator quietly becomes a second coordinator, which is how
## the HUD's god-file grew. Nothing here reads a page, a shell, or a snapshot.
##
## Chrome comes from `HudStyle` — the Workbench is the same dark console language as the HUD, not a
## second visual system. Geometry and copy come from `WorkbenchVocab`.

## Sunk well one group of rows sits in. Deliberately darker than the surface behind it, matching the
## compose sheet's readout: a group of settings is a bounded thing you read into, not a raised card.
const GROUP_BG := Color(0.0, 0.0, 0.0, 0.20)
const GROUP_CORNER_RADIUS := 6
const GROUP_PADDING_H := 12
const GROUP_PADDING_V := 10
## Gap between two parameter rows. A row is two lines tall, so this has to be clearly larger than
## the gap BETWEEN those lines or the rows blur into one another.
const ROW_GAP := 12
## Width reserved for a row's editing control, so every control in a group lines up whatever its
## label does.
##
## **IT IS SIZED BY WHAT THE LABEL COLUMN NEEDS, NOT BY THE FIELD.** No manifest value is longer
## than six characters, so every pixel over that belongs to the label beside it — and the label is
## the column that runs out first: at 132 the longest manifest label wrapped to a second line, which
## broke the alignment of every row under it. 104 is the widest setting at which no label in the
## manifest wraps at `FONT_SIZE_BODY` in a `SURFACE_WIDTH` surface (measured off the render, not the
## arithmetic — the stepper's own width is not a number you can guess).
const CONTROL_WIDTH := 104.0
## Column separation inside a row, and the gap between a row's two lines.
const ROW_COLUMN_GAP := 8
const ROW_LINE_GAP := 3
## Width the modified dot reserves. A row's second line indents by the same amount, so the hint
## hangs under the label rather than under the dot.
const MODIFIED_DOT_WIDTH := 12.0
## Gap between a group's heading and its rows.
const GROUP_HEADER_GAP := 8
## Fill behind the surface itself — a shade off `GROUND` so the Workbench reads as chrome sitting
## over the map rather than a hole in it.
const SURFACE_BG := Color(0.047, 0.075, 0.086, 0.97)


## The Workbench's own backing: a solid column with a single hairline on its trailing edge, since
## the surface is flush to the viewport edge and a full border would draw three lines nobody sees.
static func surface_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = SURFACE_BG
	sb.border_width_right = 1
	sb.border_color = HudStyle.LINE
	return sb


## The rail's backing — a touch darker than the content column beside it, which is what separates
## the two panes without a second border.
static func rail_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.22)
	sb.border_width_right = 1
	sb.border_color = HudStyle.LINE_SOFT
	return sb


## The well one parameter group draws in.
static func group_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = GROUP_BG
	sb.set_corner_radius_all(GROUP_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = HudStyle.LINE_SOFT
	sb.content_margin_left = GROUP_PADDING_H
	sb.content_margin_right = GROUP_PADDING_H
	sb.content_margin_top = GROUP_PADDING_V
	sb.content_margin_bottom = GROUP_PADDING_V
	return sb


## A standing notice at the top of a page (the tuning page's restart-scoped contract). Uses the
## chip's near-black wash under the caller's semantic tint, so a banner reads as a stated condition
## rather than an alert.
static func build_banner(text: String, tint: Color) -> PanelContainer:
	var panel := PanelContainer.new()
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(tint.r, tint.g, tint.b, 0.10)
	sb.set_corner_radius_all(4)
	sb.border_width_left = 2
	sb.border_color = tint
	sb.content_margin_left = 10
	sb.content_margin_right = 10
	sb.content_margin_top = 7
	sb.content_margin_bottom = 7
	panel.add_theme_stylebox_override("panel", sb)

	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_color_override("font_color", tint)
	label.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_HINT)
	panel.add_child(label)
	return panel


## A small upper-case section heading — the rail's group labels and a page's in-body headings.
static func build_section_label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	label.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_SECTION)
	return label


## One rail entry. `active` wears the accent bar and the lit ink; `collapsed` drops the text and
## leaves the glyph centred.
static func build_rail_entry(glyph: String, text: String, active: bool,
		collapsed: bool) -> Button:
	var button := Button.new()
	button.focus_mode = Control.FOCUS_NONE
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	button.alignment = HORIZONTAL_ALIGNMENT_CENTER if collapsed else HORIZONTAL_ALIGNMENT_LEFT
	button.text = glyph if collapsed else "%s   %s" % [glyph, text]
	button.tooltip_text = text if collapsed else ""
	button.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_RAIL)

	var ink := HudStyle.SIGNAL if active else HudStyle.INK_DIM
	button.add_theme_color_override("font_color", ink)
	button.add_theme_color_override("font_hover_color", HudStyle.INK)
	button.add_theme_color_override("font_pressed_color", HudStyle.SIGNAL)
	button.add_theme_stylebox_override("normal", _rail_entry_stylebox(active, false))
	button.add_theme_stylebox_override("hover", _rail_entry_stylebox(active, true))
	button.add_theme_stylebox_override("pressed", _rail_entry_stylebox(active, true))
	return button


static func _rail_entry_stylebox(active: bool, hovered: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	if active:
		sb.bg_color = HudStyle.SIGNAL_WASH
	elif hovered:
		sb.bg_color = Color(1.0, 1.0, 1.0, 0.04)
	else:
		sb.bg_color = Color(0.0, 0.0, 0.0, 0.0)
	# The accent bar IS the active marker; an inactive entry reserves the same width so switching
	# pages never shifts the label by two pixels.
	sb.border_width_left = int(WorkbenchVocab.RAIL_ACTIVE_BAR_WIDTH)
	sb.border_color = HudStyle.SIGNAL if active else Color(0.0, 0.0, 0.0, 0.0)
	sb.content_margin_left = 10
	sb.content_margin_right = 8
	sb.content_margin_top = WorkbenchVocab.RAIL_ENTRY_PADDING_V
	sb.content_margin_bottom = WorkbenchVocab.RAIL_ENTRY_PADDING_V
	return sb


## The numeric editor a parameter row is driven by. Typed from the manifest: an `int` param steps in
## whole units and shows none, a `float` shows enough places to represent its own step.
static func build_number_field(value: float, minimum: float, maximum: float, step: float,
		is_int: bool) -> SpinBox:
	var spin := SpinBox.new()
	spin.min_value = minimum
	spin.max_value = maximum
	spin.step = step
	spin.value = value
	spin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spin.select_all_on_focus = true
	spin.rounded = is_int
	if not is_int:
		spin.step = step
	spin.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_VALUE)

	# The BOX belongs to the frame `wrap_number_field` puts around the whole control, so the text half
	# draws no border of its own — two boxes inside one control is the stock look this replaces.
	var line := spin.get_line_edit()
	line.add_theme_color_override("font_color", HudStyle.INK)
	line.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_VALUE)
	line.add_theme_stylebox_override("normal", StyleBoxEmpty.new())
	line.add_theme_stylebox_override("read_only", StyleBoxEmpty.new())
	var focus := StyleBoxFlat.new()
	focus.bg_color = HudStyle.SIGNAL_WASH
	line.add_theme_stylebox_override("focus", focus)
	_apply_stepper_chrome(spin)
	return spin


## Put the field and its stepper in ONE box, `width` wide.
##
## A `SpinBox` draws its text half and its arrows as separate controls, so bordering the text half
## (the obvious thing) leaves the arrows stranded outside the control they drive. The frame carries
## the border instead and the field inside it carries none, which is what makes the pair read as a
## single editable value — and it is where a row's column width is set, since the frame is the thing
## a row lays out.
static func wrap_number_field(spin: SpinBox, width: float) -> PanelContainer:
	var frame := PanelContainer.new()
	frame.add_theme_stylebox_override("panel", _field_stylebox(HudStyle.LINE))
	frame.custom_minimum_size.x = width
	frame.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	frame.add_child(spin)
	return frame


## **THE STOCK STEPPER IS DRAWN FOR A LIGHT SURFACE, like the CheckBox indicator before it** (see
## `.claude/rules/client/sprites-widgets.md` → "The checkbox indicator"). A `SpinBox`'s up/down half
## wears the default theme's raised BUTTON styleboxes and near-black arrow art, so on this console it
## renders as a pale slab glued to the right of a dark field, with arrows that vanish into it.
##
## Unlike a CheckBox, though, a SpinBox modulates its arrows: `up_icon_modulate` and friends are real
## `Control` theme colours the stepper honours, so the art is KEPT and only recoloured — `INK_DIM` at
## rest, `SIGNAL` under the cursor, the same lit-on-hover language every other Workbench control
## speaks. The button backgrounds are erased entirely (the field's own border is the control's edge;
## a second box inside it draws a line nobody needs) and the field/buttons separator is a `LINE_SOFT`
## hairline, which is what keeps the arrows reading as part of the field rather than beside it.
static func _apply_stepper_chrome(spin: SpinBox) -> void:
	for direction in ["up", "down"]:
		spin.add_theme_color_override("%s_icon_modulate" % direction, HudStyle.INK_DIM)
		spin.add_theme_color_override("%s_hover_icon_modulate" % direction, HudStyle.SIGNAL)
		spin.add_theme_color_override("%s_pressed_icon_modulate" % direction, HudStyle.SIGNAL)
		spin.add_theme_color_override("%s_disabled_icon_modulate" % direction, HudStyle.INK_FAINT)
		for state in ["", "_hovered", "_pressed", "_disabled"]:
			spin.add_theme_stylebox_override("%s_background%s" % [direction, state],
				StyleBoxEmpty.new())
	var separator := StyleBoxFlat.new()
	separator.bg_color = HudStyle.LINE_SOFT
	spin.add_theme_stylebox_override("field_and_buttons_separator", separator)
	spin.add_theme_stylebox_override("up_down_buttons_separator", StyleBoxEmpty.new())


static func _field_stylebox(border: Color) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.30)
	sb.set_corner_radius_all(4)
	sb.set_border_width_all(1)
	sb.border_color = border
	sb.content_margin_left = 8
	sb.content_margin_right = 6
	sb.content_margin_top = 4
	sb.content_margin_bottom = 4
	return sb


## The dot marking a row as overridden. Always built (never conditionally added), so a row's label
## never shifts sideways when it becomes dirty — it only changes colour.
static func build_modified_dot() -> Label:
	var dot := Label.new()
	dot.text = WorkbenchVocab.MODIFIED_GLYPH
	dot.custom_minimum_size.x = MODIFIED_DOT_WIDTH
	dot.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_HINT)
	dot.add_theme_color_override("font_color", Color(0.0, 0.0, 0.0, 0.0))
	return dot


## Recolour a modified dot for its row's state.
static func set_modified(dot: Label, modified: bool) -> void:
	if dot == null:
		return
	dot.add_theme_color_override("font_color",
		HudStyle.WARN if modified else Color(0.0, 0.0, 0.0, 0.0))


## A caption in the quietest register — a row's hint, a default readout, a status line.
static func build_caption(text: String, tint: Color = HudStyle.INK_FAINT) -> Label:
	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_color_override("font_color", tint)
	label.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_HINT)
	return label


## A row's name, in body ink.
static func build_row_label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", HudStyle.INK)
	label.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_BODY)
	return label


## ONE PARAMETER, TWO LINES:
##
##     ● Label                    [ 30.0 ▴▾ ]
##       Hottest band of the temperature field.  ·  default 30.0 °C
##
## Line one is the name and the control; line two is one wrapped sentence carrying the hint and,
## after a separator in a lighter register, the default. The second line is deliberately NOT two
## columns — see `WorkbenchVocab.TUNING_HINT_LINE_FORMAT` for why the column version failed at this
## surface's width.
##
## `dot` and `spin` are built by the caller and threaded in, so this layer never owns a row's state.
static func build_param_row(label_text: String, hint: String, default_readout: String,
		dot: Label, spin: SpinBox) -> Control:
	var row := VBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", ROW_LINE_GAP)

	var top := HBoxContainer.new()
	top.add_theme_constant_override("separation", ROW_COLUMN_GAP)
	top.add_child(dot)
	var name_label := build_row_label(label_text)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	top.add_child(name_label)
	top.add_child(wrap_number_field(spin, CONTROL_WIDTH))
	row.add_child(top)

	var bottom := HBoxContainer.new()
	bottom.add_theme_constant_override("separation", ROW_COLUMN_GAP)
	# The hint hangs under the LABEL, not under the dot — the dot is a margin mark, not a column.
	var indent := Control.new()
	indent.custom_minimum_size.x = MODIFIED_DOT_WIDTH
	bottom.add_child(indent)
	bottom.add_child(build_hint_line(hint, default_readout))
	row.add_child(bottom)
	return row


## The row's second line: hint and default as ONE wrapped run, tinted apart.
##
## It is a `RichTextLabel` because the tint has to change MID-STRING — a `Label` can only colour the
## whole of itself, which is what forced the two into separate controls (and separate columns) in the
## first place. `fit_content` with the height threaded from the content is what keeps it honest
## inside a `VBoxContainer`: a RichTextLabel's default minimum height is a fixed guess, not its text.
static func build_hint_line(hint: String, default_readout: String) -> RichTextLabel:
	var text := RichTextLabel.new()
	text.bbcode_enabled = true
	text.fit_content = true
	text.scroll_active = false
	text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text.mouse_filter = Control.MOUSE_FILTER_IGNORE
	text.add_theme_font_size_override("normal_font_size", WorkbenchVocab.FONT_SIZE_HINT)
	text.text = WorkbenchVocab.TUNING_HINT_LINE_FORMAT % [
		HudStyle.INK_FAINT.to_html(false), hint,
		HudStyle.INK_DIM.to_html(false), WorkbenchVocab.TUNING_HINT_SEPARATOR, default_readout,
	]
	return text


## Format a parameter value for display: whole units for an `int`, otherwise enough decimal places
## to represent the parameter's own step (so a 0.005-step param does not read as "0.01").
static func format_value(value: float, step: float, is_int: bool, unit: String = "") -> String:
	var text: String
	if is_int:
		text = str(int(round(value)))
	else:
		var places := 0
		var scale := step
		while scale < 1.0 and places < 4:
			scale *= 10.0
			places += 1
		text = String.num(value, places)
	return text if unit.is_empty() else "%s %s" % [text, unit]
