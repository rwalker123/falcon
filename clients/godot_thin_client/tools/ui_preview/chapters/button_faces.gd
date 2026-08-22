extends RefCounted

## The two-line button face and its state colour (issue #383).
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.
##
## It runs late — after the event dock — and it is OFFSCREEN: every face renders into a `SubViewport`
## and no `_save` is taken, so it adds no frame and the frame set's bit-identity claim is untouched.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 16

const Q := preload("res://tools/ui_preview/node_query.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# ---- the TWO-LINE BUTTON FACE and its state colour (issue #383) ---------------------------------
# A `Button` renders its own `text` in ONE font size, so a face that needs two sizes is built from
# child `Label`s beside an empty-`text` Button (`HudWidgets._policy_rung_cell`, `_crew_target_pill`).
# Those Labels are independent nodes: `add_theme_color_override("font_color", …)` reaches a Button's
# own `text` and NOTHING else, and Godot's automatic disabled-state fade works the same way — so a
# state coloured through the theme alone moves the BOX and leaves both lines at full brightness, i.e.
# an unavailable control that reads as available. `HudStyle.button_font_color` is asked ONCE and both
# lines are painted from that answer, which is what keeps the two in step; the rationale is in
# `.claude/rules/client/labor-ui.md` and `sprites-widgets.md`. What was missing is anything that FAILS
# when the next state or variant skips it, and only the disabled/variant case shows it, so an ordinary
# screenshot renders fine either way.
#
# **THE CLAIM IS MADE ON PIXELS, NOT ON "AN OVERRIDE IS SET"** — the checkbox-indicator lesson
# (`_checkbox_indicator_contrast`): an override-shaped assertion passes on a control whose override
# reaches nothing the widget actually draws with, so what is asserted here is that the tint reaches the
# RENDERED GLYPHS. Each face is therefore RENDERED — into an offscreen `SubViewport`, so the shipped
# frame set keeps its count and its byte-for-byte identity — and the PEAK LUMINANCE inside each line's
# own rect is read back, enabled against disabled.
#
# **PIXELS CANNOT SAY HOW A LINE GOT DIM, and that is why the modulate claim sits beside them rather
# than inside them.** A face dimmed by `modulate` — the double-dim `_policy_rung_cell`'s own note
# rejects, since it multiplies the BOX the disabled stylebox has already faded — reads to a luminance
# measure exactly like a properly tinted one, and would pass every reading below. So the rejected shape
# is refused by its own assertion (`_face_modulate_is_identity`), not by the fade.

## How many lines a two-line face has. Named so the size guard reads as the claim it is — "the probe
## measured BOTH lines" — rather than as an index that happens to be 2.
const TWO_LINE_FACE_LINES := 2

## The offscreen canvas the probe faces are rendered on: wide enough that neither face is clipped at
## its own minimum size, small enough to sample pixel by pixel for free.
const TWO_LINE_FACE_PROBE_SIZE := Vector2i(320, 96)

## The variant the probe faces are styled with. `ghost` is the resting treatment both live builders
## reach for, and its enabled ink is `HudStyle.INK` — the brightest answer the table gives, so the
## drop to `INK_FAINT` is the case with the most room to be measured wrongly.
const TWO_LINE_FACE_PROBE_VARIANT := "ghost"

## Line 2's text on the rung probe. The STRING is immaterial — what is measured is that a second line
## exists, carries glyphs, and moves with the first — but it has to be long enough to put ink under
## the sampler.
const TWO_LINE_FACE_PROBE_METRIC := "→ 1.48 food"

## The two crew counts the caller-half probe builds its pills from. They differ so the row renders BOTH
## targets, and `workers` is set to the hold one so exactly one pill comes out SELECTED — which is what
## makes "the two pills wear different answers" a real claim rather than a tautology.
const TWO_LINE_FACE_PROBE_CLEAR_CREW := 2
const TWO_LINE_FACE_PROBE_HOLD_CREW := 5

## The peak luminance an ENABLED line must reach for the dim comparison below it to mean anything.
## `HudStyle.INK` is ~0.93 and the second line at `POLICY_PICKER_METRIC_ALPHA` composites to ~0.70,
## while either button box sits at ~0.10 — so this bar fails on a line that rendered no glyphs at all
## instead of passing vacuously, and clears both real readings with room for glyph antialiasing.
const TWO_LINE_FACE_MIN_INK := 0.35

## How far a line's peak luminance must FALL between the enabled and the disabled face. By the palette
## `INK` → `INK_FAINT` is ~0.43 on line 1 and ~0.32 on line 2; a third of the smaller of those leaves
## room for antialiasing while staying far above the ~0.01 the box's own fade contributes to a rect
## that is mostly glyph.
const TWO_LINE_FACE_MIN_DIM := 0.10

## One rung CELL, built the way the sanctioned caller builds it: the tint is the shared table's answer
## for this button's state, asked ONCE and handed in. A caller that skipped the table and coloured
## through `apply_button` alone would leave both Labels wherever the last derivation left them, which
## is what the pixels measure.
func _two_line_rung_face(disabled: bool) -> Control:
	var btn := Button.new()
	btn.disabled = disabled
	HudStyle.apply_button(btn, TWO_LINE_FACE_PROBE_VARIANT)
	return HudWidgets._policy_rung_cell(btn,
		HudFormat.floor_preset_face(SourceForecast.FLOOR_PRESET_PEAK),
		TWO_LINE_FACE_PROBE_METRIC,
		HudStyle.button_font_color(TWO_LINE_FACE_PROBE_VARIANT, disabled))

## The pill twin of `_two_line_rung_face`. `apply_pill_button` writes a `disabled` stylebox and NO font
## colours at all, so this control's disabled state is exactly the "the box fades and the face does
## not" shape — which is why the pixel claim is asked of it rather than of the box.
func _two_line_pill_face(disabled: bool) -> Control:
	var btn := Button.new()
	btn.disabled = disabled
	HudStyle.apply_pill_button(btn)
	return HudWidgets._crew_target_pill(btn, str(TWO_LINE_FACE_PROBE_HOLD_CREW),
		HudComposeVocab.CREW_TARGET_HOLD_LABEL,
		HudStyle.button_font_color(TWO_LINE_FACE_PROBE_VARIANT, disabled))

## Render ONE face alone on the probe canvas and read back the peak luminance inside each of its lines'
## own rects, in the order they are drawn. Empty when the probe cannot be captured, which fails the
## size guard rather than silently comparing nothing.
func _two_line_face_luma(probe: SubViewport, host: Control, face: Control) -> PackedFloat32Array:
	for child in host.get_children():
		host.remove_child(child)
		child.queue_free()
	host.add_child(face)
	await h._settle(false)
	var out := PackedFloat32Array()
	var texture := probe.get_texture()
	var image: Image = texture.get_image() if texture != null else null
	if image == null:
		return out
	for label in _face_label_nodes(face):
		out.append(_peak_luma(image, label.get_global_rect()))
	return out

## The brightest pixel in `rect`. The ink is the brightest thing inside a line's own rect on this HUD
## (near-white glyphs on a near-black box), so the peak IS the reading — and it is unmoved by the box's
## alpha changing under it, which a contrast-against-the-panel measure would have conflated with the
## fade being asserted.
func _peak_luma(image: Image, rect: Rect2) -> float:
	var x0 := maxi(0, int(floor(rect.position.x)))
	var y0 := maxi(0, int(floor(rect.position.y)))
	var x1 := mini(image.get_width(), int(ceil(rect.end.x)))
	var y1 := mini(image.get_height(), int(ceil(rect.end.y)))
	var best := 0.0
	for y in range(y0, y1):
		for x in range(x0, x1):
			best = maxf(best, image.get_pixel(x, y).get_luminance())
	return best

## Every `Label` under `root`, in draw order — the node twin of `_face_lines`, which answers their text.
func _face_label_nodes(root: Node) -> Array[Label]:
	var found: Array[Label] = []
	if root == null:
		return found
	if root is Label:
		found.append(root as Label)
	for child in root.get_children():
		found.append_array(_face_label_nodes(child))
	return found

## Is every `CanvasItem` in this face at modulate IDENTITY? **The claim the luminance readings cannot
## make.** `modulate` inherits to children, so dimming a whole cell with it looks right in a pixel
## measure and is the shape `_policy_rung_cell`'s note rejects: it multiplies the BOX too, which the
## disabled stylebox has already faded, so the rung comes out dimmed twice. Asked of both states, since
## a face that dims this way does it only when disabled.
func _face_modulate_is_identity(root: Node) -> bool:
	if root == null:
		return true
	if root is CanvasItem:
		var item := root as CanvasItem
		if not item.modulate.is_equal_approx(Color.WHITE) \
				or not item.self_modulate.is_equal_approx(Color.WHITE):
			return false
	for child in root.get_children():
		if not _face_modulate_is_identity(child):
			return false
	return true

## The colour each line of a face will DRAW with, resolved through the theme chain rather than read as
## an override — the question a `Label` actually answers when it rasterises its glyphs.
func _face_line_colours(root: Node) -> Array[Color]:
	var out: Array[Color] = []
	for label in _face_label_nodes(root):
		out.append(label.get_theme_color("font_color"))
	return out

## **THE GUARD.** Two halves, and they cover different seams: the CALLER (a shipped builder resolves
## its tint through the shared table, and both lines wear that one answer) and the STATE (a face built
## for a DISABLED button renders both lines dim, which is the half no live caller reaches today and so
## the half nothing else can see).
func run(harness) -> void:
	h = harness

	var probe := SubViewport.new()
	probe.size = TWO_LINE_FACE_PROBE_SIZE
	probe.transparent_bg = false
	probe.disable_3d = true
	probe.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	h.add_child(probe)
	# A KNOWN GROUND under the faces: every reading below is a luminance, and the project's default
	# clear colour is not this HUD's panel.
	var ground := ColorRect.new()
	ground.color = HudStyle.PANEL_SOLID
	ground.set_anchors_preset(Control.PRESET_FULL_RECT)
	probe.add_child(ground)
	var host := CenterContainer.new()
	host.set_anchors_preset(Control.PRESET_FULL_RECT)
	probe.add_child(host)

	# HALF 1 — THE CALLER, through the REAL public builder. `build_crew_targets` is the one shipped
	# two-line hand-built face (the policy rung's metric line came off the picker with the numbers, so
	# its second line is built but unused today). Both pills are asked for at once because the pair IS
	# the claim: the selected one must wear the `primary` answer and the resting one the `ghost` answer,
	# so a face that hard-coded either colour fails on the other.
	var targets := HudWidgets.build_crew_targets({
			"crew_to_clear": TWO_LINE_FACE_PROBE_CLEAR_CREW,
			"crew_to_hold": TWO_LINE_FACE_PROBE_HOLD_CREW,
		}, TWO_LINE_FACE_PROBE_HOLD_CREW,
		func(_count: int) -> void: pass)
	host.add_child(targets)
	await h._settle(false)
	for spec in [
		[HudWidgets.CREW_TARGET_HOLD, "primary", "the target the crew is ON"],
		[HudWidgets.CREW_TARGET_CLEAR, "ghost", "a target the crew is NOT on"],
	]:
		var what := String(spec[2])
		var pill := Q.find_crew_target(targets, String(spec[0]))
		var lines: Array[Color] = []
		if pill != null:
			# The face's Labels are siblings of the Button under the pill's CELL, not children of it.
			lines = _face_line_colours(pill.get_parent())
		h._assert_hud("%s renders a two-line face to colour" % what,
			lines.size() == TWO_LINE_FACE_LINES)
		if lines.size() != TWO_LINE_FACE_LINES:
			continue
		var want := HudStyle.button_font_color(String(spec[1]))
		h._assert_hud("…and its LEAD line is the shared table's `%s` answer, not a colour of its own"
				% String(spec[1]),
			lines[0].is_equal_approx(want))
		h._assert_hud("…and %s's second line is that SAME answer at the metric alpha — one tint, derived"
				% what,
			lines[1].is_equal_approx(
				Color(want, want.a * HudWorkVocab.POLICY_PICKER_METRIC_ALPHA)))
	host.remove_child(targets)
	targets.queue_free()

	# HALF 2 — THE STATE, measured on rendered pixels. Both builders are covered: the crew pill because
	# it is live, the policy rung because its cell is the face the invariant was written for and a rung
	# is one caller away from being disabled again.
	for spec in [
		["the policy picker's rung", Callable(self, "_two_line_rung_face")],
		["a crew-target pill", Callable(self, "_two_line_pill_face")],
	]:
		var what := String(spec[0])
		var make: Callable = spec[1]
		var lit_face: Control = make.call(false)
		var dim_face: Control = make.call(true)
		# **THE HALF THE PIXELS CANNOT MAKE** — see the banner. A cell dimmed through `modulate` passes
		# every luminance reading below and is the double-dim the invariant forbids.
		h._assert_hud("%s dims through its TINT, not through `modulate` — no double-dim on the box" % what,
			_face_modulate_is_identity(lit_face) and _face_modulate_is_identity(dim_face))
		# **THE PIXEL HALF NEEDS A RENDERER, AND NOTHING ELSE HERE DOES.** Under `--headless` the
		# probe's `SubViewport` reads back a null texture, so every reading below would fail on a
		# clean tree — a missing viewport, not a face that drew wrong (`ui_preview._capture`'s rule).
		# The `modulate` claim above is structural and still runs. A null image with a REAL renderer
		# behind it stays a failure: `_two_line_face_luma` answers an empty array and the size guard
		# below names it.
		if h._is_headless():
			push_warning("ui_preview: no renderer — skipping %s's pixel probe; run without --headless" % what)
			continue
		var lit: PackedFloat32Array = await _two_line_face_luma(probe, host, lit_face)
		var dim: PackedFloat32Array = await _two_line_face_luma(probe, host, dim_face)
		h._assert_hud("%s: the probe read BOTH lines in BOTH states" % what,
			lit.size() == TWO_LINE_FACE_LINES and dim.size() == TWO_LINE_FACE_LINES)
		if lit.size() != TWO_LINE_FACE_LINES or dim.size() != TWO_LINE_FACE_LINES:
			continue
		# The precondition: without it the two dim claims pass on a face that drew no glyphs at all.
		h._assert_hud("%s draws live ink on both lines while ENABLED (%.2f / %.2f)"
				% [what, lit[0], lit[1]],
			lit[0] >= TWO_LINE_FACE_MIN_INK and lit[1] >= TWO_LINE_FACE_MIN_INK)
		h._assert_hud("%s: line 1 goes DIM with its button (%.2f → %.2f)" % [what, lit[0], dim[0]],
			lit[0] - dim[0] >= TWO_LINE_FACE_MIN_DIM)
		h._assert_hud("%s: line 2 goes dim TOO — not left bright beside a faded box (%.2f → %.2f)"
				% [what, lit[1], dim[1]],
			lit[1] - dim[1] >= TWO_LINE_FACE_MIN_DIM)

	probe.queue_free()
	await h.get_tree().process_frame
