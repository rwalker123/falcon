extends RefCounted
class_name HudStyle

## Shared visual language for the HUD.
##
## Single source of truth for the dark "command console" look: the palette, the
## card chrome, and the primary/ghost/armed button treatments. Every HUD panel
## and button should build its styleboxes through here so the surfaces stay
## consistent (DRY) — see `PanelCard` (card chrome) and `Hud` (selection panel /
## targeting affordances). Pure static helpers; never instantiated.

# ---- palette (mirrors the targeting mockup) --------------------------------
const GROUND        := Color(0.043, 0.067, 0.078, 1.0)   # #0b1114
const GROUND_2      := Color(0.055, 0.086, 0.102, 1.0)   # #0e161a
const PANEL         := Color(0.067, 0.102, 0.118, 0.92)  # #111a1e @ 92%
const PANEL_SOLID   := Color(0.067, 0.102, 0.118, 1.0)
const LINE          := Color(0.149, 0.212, 0.235, 1.0)   # #26363c
const LINE_SOFT     := Color(0.106, 0.157, 0.176, 1.0)   # #1b282d
const INK           := Color(0.914, 0.937, 0.914, 1.0)   # #e9efe9
const INK_DIM       := Color(0.616, 0.690, 0.678, 1.0)   # #9db0ad
const INK_FAINT     := Color(0.435, 0.514, 0.502, 1.0)   # #6f8380
const SIGNAL        := Color(0.310, 0.878, 0.812, 1.0)   # #4fe0cf  targeting cyan
const SIGNAL_DEEP   := Color(0.122, 0.612, 0.557, 1.0)   # #1f9c8e
const SIGNAL_WASH   := Color(0.310, 0.878, 0.812, 0.14)
const WARN          := Color(0.949, 0.694, 0.247, 1.0)   # #f2b13f  success / ETA
const DANGER        := Color(0.910, 0.455, 0.416, 1.0)   # #e8746a
const HEALTHY       := Color(0.463, 0.804, 0.502, 1.0)   # #76cd80  well-supplied / good

# ---- The Telling: voice-medium accents -------------------------------------
# The narrator's voice AGES as the civilization crosses medium thresholds (oral -> painted ->
# written), and the accent is how that reads. RESTRAINT IS THE REQUIREMENT: the HUD is dark and
# STAYS dark — a light "parchment" panel would read as a rendering bug, not a chronicle — so the
# maturation is carried by the accent, the title and a hairline rule, nothing more. The ladder runs
# from firelight warmth toward cool ink; `oral` reuses WARN (it IS the ember tone) rather than
# adding a fourth near-identical amber, so only the two genuinely-new tones are named here.
const VOICE_PIGMENT := Color(0.784, 0.612, 0.400, 1.0)   # #c89c66  earth pigment on a cave wall
# Deliberately DESATURATED: the cool end of the ladder must read as a considered accent, never as
# the SIGNAL cyan (which means "targeting" everywhere else) nor as a greyed-out/disabled control.
const VOICE_INK     := Color(0.510, 0.635, 0.706, 1.0)   # #82a2b4  cool ink, a written record

# Hex strings for BBCode-based labels (RichTextLabel headers, command feed).
const SIGNAL_HEX := "4fe0cf"
const WARN_HEX := "f2b13f"
const DANGER_HEX := "e8746a"
const HEALTHY_HEX := "76cd80"
const INK_HEX := "e9efe9"
const INK_DIM_HEX := "9db0ad"

# ---- card chrome -----------------------------------------------------------
static func card_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = PANEL
	sb.set_corner_radius_all(10)
	sb.set_border_width_all(1)
	sb.border_color = LINE
	sb.content_margin_left = 13
	sb.content_margin_right = 13
	sb.content_margin_top = 10
	sb.content_margin_bottom = 12
	sb.shadow_color = Color(0.0, 0.0, 0.0, 0.5)
	sb.shadow_size = 10
	sb.shadow_offset = Vector2(0.0, 8.0)
	return sb

## Fully transparent stylebox — for stripping a control's default background
## (e.g. the RichTextLabel header inside a card).
static func empty_stylebox() -> StyleBoxEmpty:
	return StyleBoxEmpty.new()

# ---- nav cluster backing ---------------------------------------------------
# The bottom-left minimap + zoom rail share one rounded semi-transparent black
# panel (matches the nav prototype). Deliberately darker/plainer than a card so it
# reads as map chrome, not a content surface — hence a bespoke box, not card_stylebox.
const NAV_BACKING_OPACITY := 0.85
const NAV_BACKING_CORNER_RADIUS := 10
const NAV_BACKING_PADDING := 8

static func nav_backing_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, NAV_BACKING_OPACITY)
	sb.set_corner_radius_all(NAV_BACKING_CORNER_RADIUS)
	sb.content_margin_left = NAV_BACKING_PADDING
	sb.content_margin_right = NAV_BACKING_PADDING
	sb.content_margin_top = NAV_BACKING_PADDING
	sb.content_margin_bottom = NAV_BACKING_PADDING
	return sb

## Targeting banner chrome: a prominent cyan-bordered pill that floats at the top
## of the map while a command is choosing its target.
static func banner_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.035, 0.067, 0.075, 0.88)
	sb.set_corner_radius_all(11)
	sb.set_border_width_all(1)
	sb.border_color = SIGNAL_DEEP
	sb.content_margin_left = 14
	sb.content_margin_right = 12
	sb.content_margin_top = 9
	sb.content_margin_bottom = 9
	sb.shadow_color = Color(0.0, 0.0, 0.0, 0.55)
	sb.shadow_size = 14
	sb.shadow_offset = Vector2(0.0, 8.0)
	return sb

## Header treatment: transparent fill with a hairline divider under the title,
## giving each card its "title bar" separation from the body.
static func header_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.draw_center = false
	sb.border_width_bottom = 1
	sb.border_color = LINE_SOFT
	sb.content_margin_top = 1
	sb.content_margin_bottom = 7
	sb.content_margin_left = 0
	sb.content_margin_right = 0
	return sb

# ---- buttons ---------------------------------------------------------------
static func _button_stylebox(bg: Color, border: Color) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.set_corner_radius_all(7)
	sb.set_border_width_all(1)
	sb.border_color = border
	sb.content_margin_left = 11
	sb.content_margin_right = 11
	sb.content_margin_top = 9
	sb.content_margin_bottom = 9
	return sb

## Apply one of the button treatments: "primary" (the main action, cyan),
## "ghost" (secondary), or "armed" (an action awaiting cancellation).
static func apply_button(button: Button, variant: String = "ghost") -> void:
	if button == null:
		return
	var bg_normal: Color
	var bg_hover: Color
	var border_normal: Color
	var border_hover: Color
	var text: Color
	match variant:
		"primary":
			bg_normal = Color(0.086, 0.227, 0.204, 1.0)   # #163a34
			bg_hover = Color(0.110, 0.275, 0.251, 1.0)    # #1c4640
			border_normal = SIGNAL_DEEP
			border_hover = SIGNAL
			text = Color(0.847, 1.0, 0.973, 1.0)          # #d8fff8
		"armed":
			bg_normal = Color(0.165, 0.110, 0.102, 1.0)   # #2a1c1a
			bg_hover = Color(0.200, 0.122, 0.114, 1.0)    # #331f1d
			border_normal = Color(0.353, 0.227, 0.212, 1.0)  # #5a3a36
			border_hover = DANGER
			text = Color(0.941, 0.765, 0.741, 1.0)        # #f0c3bd
		_:  # "ghost"
			bg_normal = Color(0.075, 0.129, 0.122, 1.0)   # #13211f
			bg_hover = Color(0.090, 0.188, 0.161, 1.0)    # #173029
			border_normal = LINE
			border_hover = SIGNAL_DEEP
			text = INK

	button.add_theme_stylebox_override("normal", _button_stylebox(bg_normal, border_normal))
	button.add_theme_stylebox_override("hover", _button_stylebox(bg_hover, border_hover))
	button.add_theme_stylebox_override("pressed", _button_stylebox(bg_hover, border_hover))
	var disabled := _button_stylebox(Color(bg_normal.r, bg_normal.g, bg_normal.b, 0.4), LINE_SOFT)
	button.add_theme_stylebox_override("disabled", disabled)
	var focus := _button_stylebox(bg_normal, SIGNAL)
	focus.draw_center = false
	button.add_theme_stylebox_override("focus", focus)

	button.add_theme_color_override("font_color", text)
	button.add_theme_color_override("font_hover_color", INK)
	button.add_theme_color_override("font_pressed_color", text)
	button.add_theme_color_override("font_focus_color", INK)
	button.add_theme_color_override("font_disabled_color", INK_FAINT)

# ---- inline link buttons ---------------------------------------------------
# Padding around an inline link's text. Deliberately far tighter than the boxed
# `_button_stylebox` chrome (11 × 9) so a clickable label keeps a plain label's
# footprint and never grows the row it shares with other widgets (e.g. the Band
# panel's Current-actions rows, whose label sits beside a −/+ worker stepper).
const LINK_PADDING_X := 4
const LINK_PADDING_Y := 2
const LINK_CORNER_RADIUS := 5

static func _link_hover_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	# The ghost button's hover fill/border — same "this is actionable" language,
	# just without the at-rest box.
	sb.bg_color = Color(0.090, 0.188, 0.161, 1.0)   # #173029
	sb.set_corner_radius_all(LINK_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = SIGNAL_DEEP
	sb.content_margin_left = LINK_PADDING_X
	sb.content_margin_right = LINK_PADDING_X
	sb.content_margin_top = LINK_PADDING_Y
	sb.content_margin_bottom = LINK_PADDING_Y
	return sb

static func _link_rest_stylebox() -> StyleBoxEmpty:
	var sb := StyleBoxEmpty.new()
	# Same content margins as the hover skin, so the text does not shift on hover.
	sb.content_margin_left = LINK_PADDING_X
	sb.content_margin_right = LINK_PADDING_X
	sb.content_margin_top = LINK_PADDING_Y
	sb.content_margin_bottom = LINK_PADDING_Y
	return sb

## Apply the **inline link** treatment to a Button: reads as a plain label at rest
## (no box), tints its background + border and lifts its text to `SIGNAL` on hover,
## with a pointing-hand cursor — so an in-row label advertises itself as clickable
## without shouting like a full ghost button. `base_color` is the at-rest font color,
## so a caller's semantic tint (e.g. `WARN` on a pending row) survives.
static func apply_link_button(button: Button, base_color: Color = INK) -> void:
	if button == null:
		return
	button.add_theme_stylebox_override("normal", _link_rest_stylebox())
	button.add_theme_stylebox_override("hover", _link_hover_stylebox())
	button.add_theme_stylebox_override("pressed", _link_hover_stylebox())
	button.add_theme_stylebox_override("disabled", _link_rest_stylebox())
	var focus := _link_hover_stylebox()
	focus.draw_center = false
	button.add_theme_stylebox_override("focus", focus)
	button.add_theme_color_override("font_color", base_color)
	button.add_theme_color_override("font_hover_color", SIGNAL)
	button.add_theme_color_override("font_pressed_color", SIGNAL)
	button.add_theme_color_override("font_focus_color", base_color)
	button.add_theme_color_override("font_disabled_color", INK_FAINT)
	button.focus_mode = Control.FOCUS_NONE
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
