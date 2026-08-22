extends RefCounted
class_name HudStyle

## Shared visual language for the HUD.
##
## Single source of truth for the dark "command console" look: the palette, the
## card chrome, and the primary/ghost/armed button treatments. Every HUD panel
## and button should build its styleboxes through here so the surfaces stay
## consistent (DRY) — see `PanelCard` (card chrome) and `Hud` (selection panel /
## targeting affordances). Pure static helpers; never instantiated.
##
## **THE PALETTE IS SWAPPABLE, THE LAYOUT IS NOT.** The colour block below is `static var` rather than
## `const` so `HudPalette.apply()` can install one of the themes at boot; every other `const` here is a
## layout number (padding, radius, alpha, font size) or a pure-black wash that works on any ground, and
## those are NOT themed. A `static var` reads identically at the call site, so no consumer changed —
## but it is not a constant expression, so `const X := HudStyle.DANGER` in another script is a parse
## error. See `ui/HudPalette.gd` for the theme roster and the derive-inside-apply rule.

# ---- palette (mirrors the targeting mockup) --------------------------------
static var GROUND        := Color(0.043, 0.067, 0.078, 1.0)   # #0b1114
static var GROUND_2      := Color(0.055, 0.086, 0.102, 1.0)   # #0e161a
## The card fill IS the solid panel fill, at this opacity — see `apply_palette`, which derives it.
const PANEL_OPACITY := 0.92
static var PANEL         := Color(0.067, 0.102, 0.118, 0.92)  # PANEL_SOLID @ PANEL_OPACITY
static var PANEL_SOLID   := Color(0.067, 0.102, 0.118, 1.0)
static var LINE          := Color(0.149, 0.212, 0.235, 1.0)   # #26363c
static var LINE_SOFT     := Color(0.106, 0.157, 0.176, 1.0)   # #1b282d
static var INK           := Color(0.914, 0.937, 0.914, 1.0)   # #e9efe9
static var INK_DIM       := Color(0.616, 0.690, 0.678, 1.0)   # #9db0ad
static var INK_FAINT     := Color(0.435, 0.514, 0.502, 1.0)   # #6f8380
static var SIGNAL        := Color(0.310, 0.878, 0.812, 1.0)   # #4fe0cf  targeting cyan
static var SIGNAL_DEEP   := Color(0.122, 0.612, 0.557, 1.0)   # #1f9c8e
## The faintest tint of SIGNAL a surface can wear — a selected row's backing, not an accent.
const SIGNAL_WASH_OPACITY := 0.14
static var SIGNAL_WASH   := Color(0.310, 0.878, 0.812, 0.14)  # SIGNAL @ SIGNAL_WASH_OPACITY
static var WARN          := Color(0.949, 0.694, 0.247, 1.0)   # #f2b13f  success / ETA
static var DANGER        := Color(0.910, 0.455, 0.416, 1.0)   # #e8746a
static var HEALTHY       := Color(0.463, 0.804, 0.502, 1.0)   # #76cd80  well-supplied / good
# The two DANGER-overlay hues (Predators Phase 3), shared by the HUD alert surfaces so the command
# feed's threat/casualty accents and the band panel's predator-nearby warning speak the SAME danger
# language as the map's `threat` / `hunt_danger` washes. Values MIRROR MapView.THREAT_OVERLAY_COLOR /
# HUNT_DANGER_OVERLAY_COLOR (the map layer keeps its own copies for the tile washes; these are the HUD
# side of the same palette). Crimson = an unprovoked raid/casualty; amber = a hunt-cost caution.
static var THREAT_ACCENT := Color(0.85, 0.16, 0.16, 1.0)      # #d92929  threat red (raid / camp menace)
static var HUNT_DANGER_ACCENT := Color(0.93, 0.52, 0.13, 1.0) # #ed8521  danger orange (cost to hunt)
## The `primary` button variant's resting fill. Named because it is the ONLY marker of "this control
## is the selected/committing one" — a policy picker's chosen rung wears it and nothing else does —
## so a test that asks "which rung is lit?" has to read it back off the stylebox.
static var BUTTON_PRIMARY_BG := Color(0.086, 0.227, 0.204, 1.0)   # #163a34
## The three button variants' resting TEXT colours, named because `apply_button` is no longer their
## only consumer: a button whose face is built from CHILD LABELS (`HudWidgets.build_policy_picker`'s
## two-line rung — two font sizes cannot live in one `Button.text`) cannot use the theme override at
## all, since `font_color` reaches a Button's own `text` and nothing else. Such a face reads its tint
## from `button_font_color` below, so a hand-built face can never drift from a themed one.
static var BUTTON_PRIMARY_TEXT := Color(0.847, 1.0, 0.973, 1.0)   # #d8fff8
static var BUTTON_ARMED_TEXT := Color(0.941, 0.765, 0.741, 1.0)   # #f0c3bd
## The three button variants' FILLS, and they are AUTHORED palette entries rather than tints derived
## off `BUTTON_PRIMARY_BG`. They were inline literals inside `apply_button` until the palette became
## swappable, which is exactly the bug that hides best: a `primary` button read correctly at rest (it
## took the palette's `BUTTON_PRIMARY_BG`) and reverted to console teal the moment the pointer entered
## it, and a `ghost` button — every secondary control in the client — was console teal always.
## Authored because each is a real decision a theme makes: how quiet a secondary control's fill is,
## how far a hover lifts it, and how warm the armed/destructive fill runs.
static var GHOST_BG := Color(0.075, 0.129, 0.122, 1.0)        # #13211f
static var GHOST_BG_HOVER := Color(0.090, 0.188, 0.161, 1.0)  # #173029
static var PRIMARY_BG_HOVER := Color(0.110, 0.275, 0.251, 1.0)  # #1c4640
static var ARMED_BG := Color(0.165, 0.110, 0.102, 1.0)        # #2a1c1a
static var ARMED_BG_HOVER := Color(0.200, 0.122, 0.114, 1.0)  # #331f1d
static var ARMED_BORDER := Color(0.353, 0.227, 0.212, 1.0)    # #5a3a36

# ---- The Telling: voice-medium accents -------------------------------------
# The narrator's voice AGES as the civilization crosses medium thresholds (oral -> painted ->
# written), and the accent is how that reads. RESTRAINT IS THE REQUIREMENT: the HUD is dark and
# STAYS dark — a light "parchment" panel would read as a rendering bug, not a chronicle — so the
# maturation is carried by the accent, the title and a hairline rule, nothing more. The ladder runs
# from firelight warmth toward cool ink; `oral` reuses WARN (it IS the ember tone) rather than
# adding a fourth near-identical amber, so only the two genuinely-new tones are named here.
static var VOICE_PIGMENT := Color(0.784, 0.612, 0.400, 1.0)   # #c89c66  earth pigment on a cave wall
# Deliberately DESATURATED: the cool end of the ladder must read as a considered accent, never as
# the SIGNAL cyan (which means "targeting" everywhere else) nor as a greyed-out/disabled control.
static var VOICE_INK     := Color(0.510, 0.635, 0.706, 1.0)   # #82a2b4  cool ink, a written record

# Hex strings for BBCode-based labels (RichTextLabel headers, command feed). DERIVED from the
# colours above by `apply_palette` — never edit one of these to change a tint.
static var SIGNAL_HEX := "4fe0cf"
static var WARN_HEX := "f2b13f"
static var DANGER_HEX := "e8746a"
static var HEALTHY_HEX := "76cd80"
static var INK_HEX := "e9efe9"
static var INK_DIM_HEX := "9db0ad"

# ---- theme installation ----------------------------------------------------
## Install one theme's palette. Called by `HudPalette.apply()` at boot (an autoload, so BEFORE the
## first Control exists — the palette is restart-to-apply and nothing here rebuilds live UI).
##
## `p` carries the **26 AUTHORED** colours; everything below the assignment block is **DERIVED**, and
## the derivation lives HERE rather than in a static-var initializer on purpose. An initializer runs
## when this script is loaded, which is before `apply_palette` has ever been called, so a derived
## initializer would freeze at the default theme's value and silently never update.
##
## Deriving `PANEL` also fixes a drift hazard the two hand-written literals carried: the card fill and
## the solid panel fill are ONE colour at two opacities, and only one of them was ever edited.
static func apply_palette(p: Dictionary) -> void:
	GROUND = p["GROUND"]
	GROUND_2 = p["GROUND_2"]
	PANEL_SOLID = p["PANEL_SOLID"]
	LINE = p["LINE"]
	LINE_SOFT = p["LINE_SOFT"]
	INK = p["INK"]
	INK_DIM = p["INK_DIM"]
	INK_FAINT = p["INK_FAINT"]
	SIGNAL = p["SIGNAL"]
	SIGNAL_DEEP = p["SIGNAL_DEEP"]
	WARN = p["WARN"]
	DANGER = p["DANGER"]
	HEALTHY = p["HEALTHY"]
	THREAT_ACCENT = p["THREAT_ACCENT"]
	HUNT_DANGER_ACCENT = p["HUNT_DANGER_ACCENT"]
	BUTTON_PRIMARY_BG = p["BUTTON_PRIMARY_BG"]
	BUTTON_PRIMARY_TEXT = p["BUTTON_PRIMARY_TEXT"]
	BUTTON_ARMED_TEXT = p["BUTTON_ARMED_TEXT"]
	GHOST_BG = p["GHOST_BG"]
	GHOST_BG_HOVER = p["GHOST_BG_HOVER"]
	PRIMARY_BG_HOVER = p["PRIMARY_BG_HOVER"]
	ARMED_BG = p["ARMED_BG"]
	ARMED_BG_HOVER = p["ARMED_BG_HOVER"]
	ARMED_BORDER = p["ARMED_BORDER"]
	VOICE_PIGMENT = p["VOICE_PIGMENT"]
	VOICE_INK = p["VOICE_INK"]
	# --- derived ---
	PANEL = Color(PANEL_SOLID, PANEL_OPACITY)
	SIGNAL_WASH = Color(SIGNAL, SIGNAL_WASH_OPACITY)
	SIGNAL_HEX = SIGNAL.to_html(false)
	WARN_HEX = WARN.to_html(false)
	DANGER_HEX = DANGER.to_html(false)
	HEALTHY_HEX = HEALTHY.to_html(false)
	INK_HEX = INK.to_html(false)
	INK_DIM_HEX = INK_DIM.to_html(false)
	# The GENERATED ICON RASTERS bake INK/SIGNAL into pixels, so a palette swap has to drop them; each
	# rebuilds on the next styled control. A raster is the one thing in this file a re-assignment
	# cannot reach — it is already pixels by the time the theme changes.
	_checkbox_unchecked = null
	_checkbox_unchecked_disabled = null
	_checkbox_checked = null
	_checkbox_checked_disabled = null
	_slider_grabber = null

# ---- card chrome -----------------------------------------------------------
## The card's corner radius, named because the dropdown POPUP wears it too — a menu that opens off a
## card is the same surface, and two literals would drift.
const CARD_CORNER_RADIUS := 10
## Every drop shadow in this file is PURE BLACK at low alpha, and that is deliberately not a palette
## entry: a shadow is an absence of light, so it is the one fill that is correct under any ground.
const SHADOW_COLOR := Color(0.0, 0.0, 0.0, 0.5)
## The modal's shadow runs one step deeper than a card's — it floats further off the surface.
const DIALOG_SHADOW_COLOR := Color(0.0, 0.0, 0.0, 0.55)

static func card_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = PANEL
	sb.set_corner_radius_all(CARD_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = LINE
	sb.content_margin_left = 13
	sb.content_margin_right = 13
	sb.content_margin_top = 10
	sb.content_margin_bottom = 12
	sb.shadow_color = SHADOW_COLOR
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
	# Pure black, not a palette entry: the nav cluster reads as a HOLE in the map rather than as a
	# surface with a colour, which is true under every theme.
	sb.bg_color = Color(0.0, 0.0, 0.0, NAV_BACKING_OPACITY)
	sb.set_corner_radius_all(NAV_BACKING_CORNER_RADIUS)
	sb.content_margin_left = NAV_BACKING_PADDING
	sb.content_margin_right = NAV_BACKING_PADDING
	sb.content_margin_top = NAV_BACKING_PADDING
	sb.content_margin_bottom = NAV_BACKING_PADDING
	return sb

## How solid the targeting banner's fill sits over the map. The fill itself is `GROUND` — the banner
## is the console's own ground floated over the world, and it was a hand-written near-copy of that
## colour until the palette became swappable, which left it console-teal under every theme.
const BANNER_OPACITY := 0.88

## Targeting banner chrome: a prominent SIGNAL-bordered pill that floats at the top
## of the map while a command is choosing its target.
static func banner_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(GROUND, BANNER_OPACITY)
	sb.set_corner_radius_all(11)
	sb.set_border_width_all(1)
	sb.border_color = SIGNAL_DEEP
	sb.content_margin_left = 14
	sb.content_margin_right = 12
	sb.content_margin_top = 9
	sb.content_margin_bottom = 9
	sb.shadow_color = DIALOG_SHADOW_COLOR
	sb.shadow_size = 14
	sb.shadow_offset = Vector2(0.0, 8.0)
	return sb

# ---- chips -----------------------------------------------------------------
# A CHIP is a pinned one-word standing condition (the selection card's Sight / Habitability /
# Climate / Tags / Site strip): a pill that reads as a label wearing a state colour, never as a
# button. Hence the near-black wash + a hairline border at CHIP_BORDER_ALPHA of the passed tint —
# a full-strength border would compete with the text it frames. The radius is deliberately far past
# the chip's own height so the ends are true semicircles at any font size.
## Pure black at low alpha, and NOT a palette entry: a chip darkens whatever ground it sits on
## rather than stating a colour of its own, which is right under every theme.
const CHIP_BG := Color(0.0, 0.0, 0.0, 0.25)
const CHIP_BORDER_ALPHA := 0.4
const CHIP_CORNER_RADIUS := 999
const CHIP_PADDING_X := 7
const CHIP_PADDING_Y := 2

## Pill chrome for one chip, bordered in the caller's semantic tint.
static func chip_stylebox(border: Color) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = CHIP_BG
	sb.set_corner_radius_all(CHIP_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = Color(border.r, border.g, border.b, CHIP_BORDER_ALPHA)
	sb.content_margin_left = CHIP_PADDING_X
	sb.content_margin_right = CHIP_PADDING_X
	sb.content_margin_top = CHIP_PADDING_Y
	sb.content_margin_bottom = CHIP_PADDING_Y
	return sb

# ---- the compose sheet's readout box + its crew-target pills ---------------
# A READOUT is a bounded well sunk into the panel: the take, the verdict and the asides are the
# ANSWER to everything composed above them, and until they were boxed they read as three unrelated
# lines floating in the panel's bottom half. Hence a recessed fill (darker than the panel it sits on,
# so the box reads as sunk rather than raised — the opposite of the role card's `GROUND_2`) inside a
# `LINE_SOFT` hairline, at the small radius a well wants.
## Pure black at low alpha, palette-independent for the same reason as `CHIP_BG`: a well is a
## RECESS, drawn by darkening the surface it is cut into.
const READOUT_BG := Color(0.0, 0.0, 0.0, 0.22)
const READOUT_CORNER_RADIUS := 3
const READOUT_PADDING_H := 11
const READOUT_PADDING_V := 9

## The compose sheet's readout well — see `READOUT_BG`.
static func readout_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = READOUT_BG
	sb.set_corner_radius_all(READOUT_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = LINE_SOFT
	sb.content_margin_left = READOUT_PADDING_H
	sb.content_margin_right = READOUT_PADDING_H
	sb.content_margin_top = READOUT_PADDING_V
	sb.content_margin_bottom = READOUT_PADDING_V
	return sb

# The DASHED rule that cuts the readout's quietest register off from the two above it. Dashed rather
# than solid on purpose: a solid hairline is a DIVISION between two blocks of equal standing (what
# `hairline_stylebox` draws), and the aside is not that — it is a footnote to what is above it. Godot
# has no dashed border on any StyleBox, so it is drawn; the geometry lives here beside the palette it
# draws in, like the chip and nav-backing blocks above.
const DASHED_RULE_DASH := 3.0
const DASHED_RULE_GAP := 3.0
# The rule CONTROL's height — the row the dashes are drawn down the middle of. The dashes themselves
# are Godot's thin-line primitive, i.e. one DEVICE pixel whatever this HUD's canvas stretch is doing;
# see `HudWidgets.build_dashed_rule` for why a width-1 line here is invisible.
const DASHED_RULE_HEIGHT := 1.0

# The crew TARGET pill: the chip's geometry (a radius far past the control's height, so the ends are
# true semicircles) on a control that is PRESSED rather than read, hence more room for the pointer
# than a chip's 7/2 and a border that lifts on hover. The `primary` variant's fill marks the target
# the crew is already standing on, exactly as it marks the selected preset.
const PILL_CORNER_RADIUS := 999
const PILL_PADDING_H := 9
const PILL_PADDING_V := 4
## How far a disabled pill's fill is pulled back — the same fraction `apply_button` fades a disabled
## button's fill by, so the two treatments read as one "unavailable".
const PILL_DISABLED_FILL_ALPHA := 0.4

static func _pill_stylebox(bg: Color, border: Color) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.set_corner_radius_all(PILL_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = border
	sb.content_margin_left = PILL_PADDING_H
	sb.content_margin_right = PILL_PADDING_H
	sb.content_margin_top = PILL_PADDING_V
	sb.content_margin_bottom = PILL_PADDING_V
	return sb

## Pill chrome for a crew target. `selected` is "the crew is already on this number", which wears the
## same `BUTTON_PRIMARY_BG` fill every other chosen control in this HUD does — so "which one am I on?"
## is one question with one answer whether the control is a rung, a preset or a target.
static func apply_pill_button(button: Button, selected: bool = false) -> void:
	if button == null:
		return
	var bg := BUTTON_PRIMARY_BG if selected else CHIP_BG
	var border := SIGNAL_DEEP if selected else LINE_SOFT
	button.add_theme_stylebox_override("normal", _pill_stylebox(bg, border))
	button.add_theme_stylebox_override("hover", _pill_stylebox(bg, SIGNAL_DEEP))
	button.add_theme_stylebox_override("pressed", _pill_stylebox(bg, SIGNAL_DEEP))
	button.add_theme_stylebox_override("disabled",
		_pill_stylebox(Color(bg.r, bg.g, bg.b, bg.a * PILL_DISABLED_FILL_ALPHA), LINE_SOFT))
	var focus := _pill_stylebox(bg, SIGNAL)
	focus.draw_center = false
	button.add_theme_stylebox_override("focus", focus)

## How much of a QUIET pill's fill and border survive while it is unselected: none of it. Named rather
## than written as a bare `0.0` so the rest state reads as a deliberate suppression of the chrome
## `apply_pill_button` draws, rather than as a colour somebody forgot to fill in.
const PILL_QUIET_ALPHA := 0.0

## A pill that says SELECTED and nothing else — the forage sheet's species chips. Selected is
## `apply_pill_button`'s own filled, bordered pill, so *which one am I on?* keeps ONE answer across
## this HUD; UNSELECTED draws no fill and no border at all, i.e. plain text sitting in the row.
##
## **THE GEOMETRY IS IDENTICAL IN BOTH STATES, which is why the quiet box is drawn TRANSPARENT rather
## than not drawn at all.** A `StyleBoxEmpty` carries no content margins, so a chip would lose its
## padding the moment it was deselected and the whole row would jump on every toggle.
##
## **THE HOVER KEEPS THE RESTING PILL'S CHROME, and that is the one thing a bare label cannot do.** A
## control with no decoration at rest says nothing about being pressable, so the box comes back under
## the pointer — an AFFORDANCE, not a state, gone again the moment the pointer leaves.
static func apply_pill_toggle(button: Button, selected: bool = false) -> void:
	if button == null:
		return
	if selected:
		apply_pill_button(button, true)
		return
	var quiet := _pill_stylebox(
		Color(CHIP_BG, PILL_QUIET_ALPHA), Color(LINE_SOFT, PILL_QUIET_ALPHA))
	button.add_theme_stylebox_override("normal", quiet)
	button.add_theme_stylebox_override("disabled", quiet)
	button.add_theme_stylebox_override("hover", _pill_stylebox(CHIP_BG, SIGNAL_DEEP))
	button.add_theme_stylebox_override("pressed", _pill_stylebox(CHIP_BG, SIGNAL_DEEP))
	var focus := _pill_stylebox(CHIP_BG, SIGNAL)
	focus.draw_center = false
	button.add_theme_stylebox_override("focus", focus)

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

## A standalone hairline RULE inside a card — the same LINE_SOFT 1px `header_stylebox` draws under
## a title, for a divider between two blocks of one card's body (the selection card's list ↔ drawer
## boundary). The caller owns the thickness (a `custom_minimum_size.y` on the node it styles).
static func hairline_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = LINE_SOFT
	return sb

# ---- Band/City panel zone chrome -------------------------------------------
# The bordered-card treatment the panel's standing-role cards and its work-inspector strip share, plus
# the flat row backing the work board's rows draw. Geometry lives here beside the stylebox that reads
# it, exactly as the nav-backing and chip blocks above do.
const ROLE_CARD_PADDING := 6
const ROLE_CARD_CORNER_RADIUS := 4
const WORK_ROW_PADDING_H := 4
const WORK_ROW_PADDING_V := 2

## A standing-role CARD (Scout / Warrior): a bordered, rounded, slightly-raised panel. The border is
## what makes a role read as "a standing role" rather than as one more worked source in a list.
static func role_card_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = GROUND_2
	sb.set_border_width_all(1)
	sb.border_color = LINE
	sb.set_corner_radius_all(ROLE_CARD_CORNER_RADIUS)
	sb.content_margin_left = ROLE_CARD_PADDING
	sb.content_margin_right = ROLE_CARD_PADDING
	sb.content_margin_top = ROLE_CARD_PADDING
	sb.content_margin_bottom = ROLE_CARD_PADDING
	return sb

## The work board's row backing: a SIGNAL wash while the row's inspector is open, fully transparent at
## rest. Padding only — a row must draw at exactly `WORK_ROW_HEIGHT` or the page overflows its zone.
static func work_row_stylebox(open: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = SIGNAL_WASH if open else Color(0.0, 0.0, 0.0, 0.0)
	sb.content_margin_left = WORK_ROW_PADDING_H
	sb.content_margin_right = WORK_ROW_PADDING_H
	sb.content_margin_top = WORK_ROW_PADDING_V
	sb.content_margin_bottom = WORK_ROW_PADDING_V
	return sb

## The inspector strip under a work-board / parties row — the role card's chrome, reused so a strip
## and a card read as the same kind of raised surface.
static func work_inspector_stylebox() -> StyleBoxFlat:
	return role_card_stylebox()

# ---- buttons ---------------------------------------------------------------
const BUTTON_CORNER_RADIUS := 7
const BUTTON_PADDING_H := 11
const BUTTON_PADDING_V := 9
## How far a DISABLED button's fill is pulled back. Alpha rather than a second set of fills: a locked
## control is the same control seen through less light, so one number covers all three variants.
const BUTTON_DISABLED_FILL_ALPHA := 0.4

static func _button_stylebox(bg: Color, border: Color) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.set_corner_radius_all(BUTTON_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = border
	sb.content_margin_left = BUTTON_PADDING_H
	sb.content_margin_right = BUTTON_PADDING_H
	sb.content_margin_top = BUTTON_PADDING_V
	sb.content_margin_bottom = BUTTON_PADDING_V
	return sb

## How far a SELECTED-yet-disabled button's text is pulled back from its live colour. It keeps the
## variant's HUE (so the control still reads as the current choice) at reduced strength (so it still
## reads as unavailable) instead of collapsing to the `INK_FAINT` a never-chosen disabled control
## gets — see `button_font_color`'s `selected` argument.
const BUTTON_SELECTED_DISABLED_TEXT_ALPHA := 0.72

## THE ONE PLACE a button's text colour is decided, for BOTH kinds of face: the themed one
## (`apply_button` feeds it straight into `font_color`) and the hand-built one (a stack of child
## Labels, which the theme cannot reach — see `BUTTON_PRIMARY_TEXT`). A caller that builds its own
## face asks here rather than repeating a colour, so a new state colour added below arrives on every
## face at once instead of only on the themed half.
##
## `selected` is read ONLY when `disabled`, for a control that is simultaneously the current choice
## and un-actionable. Fading such a control to `INK_FAINT` like any other locked one erases the only
## mark that says which one is current.
##
## **Its ONE remaining caller is the CROP PICKER's committed row** (`DrawerComposeController`), where
## a committed patch shows its whole basket as a locked READOUT with the standing crop still marked.
## The policy picker's standing-but-gated rung — the state this was originally written for (#420) —
## is gone with issue #442: a stance is never gated and never retires, and an improvement is a
## checkbox that becomes a label, which cannot be selected-and-locked. Do not delete the flag on that
## news; the crop picker's need is a genuinely different one.
static func button_font_color(variant: String = "ghost", disabled: bool = false,
		selected: bool = false) -> Color:
	var live: Color
	match variant:
		"primary":
			live = BUTTON_PRIMARY_TEXT
		"armed":
			live = BUTTON_ARMED_TEXT
		_:  # "ghost"
			live = INK
	if not disabled:
		return live
	if not selected:
		return INK_FAINT
	return Color(live, live.a * BUTTON_SELECTED_DISABLED_TEXT_ALPHA)

## THE FILL/BORDER QUARTET ONE VARIANT IS DRAWN FROM — every value a palette entry, none of them a
## literal. Six of these were inline `Color(...)` literals until the palette became swappable, and the
## resulting bug is the one worth remembering: a `primary` button read correctly AT REST (it took
## `BUTTON_PRIMARY_BG` from the palette) and snapped back to console teal the instant it was hovered,
## while a `ghost` button — every secondary control in the client — was console teal always. **A
## themed colour lives in the palette; it never lives as a literal inside a styling helper.**
static func _variant_colors(variant: String) -> Dictionary:
	match variant:
		"primary":
			return {"bg": BUTTON_PRIMARY_BG, "bg_hover": PRIMARY_BG_HOVER,
				"border": SIGNAL_DEEP, "border_hover": SIGNAL}
		"armed":
			return {"bg": ARMED_BG, "bg_hover": ARMED_BG_HOVER,
				"border": ARMED_BORDER, "border_hover": DANGER}
		_:  # "ghost"
			return {"bg": GHOST_BG, "bg_hover": GHOST_BG_HOVER,
				"border": LINE, "border_hover": SIGNAL_DEEP}

## Every stylebox one variant needs, keyed by the theme item name a `Button` reads it under. Split out
## of `apply_button` because `apply_option_button` dresses an `OptionButton`'s FACE in the identical
## chrome, and a second construction of the same five boxes is a drift waiting to happen.
static func button_styleboxes(variant: String = "ghost",
		selected_when_disabled: bool = false) -> Dictionary:
	var c := _variant_colors(variant)
	var bg: Color = c["bg"]
	var hover := _button_stylebox(c["bg_hover"], c["border_hover"])
	var disabled_border: Color = c["border"]
	if not selected_when_disabled:
		disabled_border = LINE_SOFT
	var focus := _button_stylebox(bg, SIGNAL)
	focus.draw_center = false
	return {
		"normal": _button_stylebox(bg, c["border"]),
		"hover": hover,
		"pressed": _button_stylebox(c["bg_hover"], c["border_hover"]),
		"disabled": _button_stylebox(Color(bg, bg.a * BUTTON_DISABLED_FILL_ALPHA), disabled_border),
		"focus": focus,
	}

## Apply one of the button treatments: "primary" (the main action, cyan),
## "ghost" (secondary), or "armed" (an action awaiting cancellation).
##
## `selected_when_disabled` styles the disabled state as "the current choice, unavailable" rather
## than "locked": the variant's own border survives instead of fading to `LINE_SOFT`, and the text
## keeps its hue (`button_font_color`'s `selected`). Its one caller is the crop picker's committed
## row — see `button_font_color`. It changes NOTHING while the button is enabled, so it is safe to
## set before `disabled` is known.
static func apply_button(button: Button, variant: String = "ghost",
		selected_when_disabled: bool = false) -> void:
	if button == null:
		return
	var text := button_font_color(variant)
	var boxes := button_styleboxes(variant, selected_when_disabled)
	for item in boxes:
		button.add_theme_stylebox_override(item, boxes[item])

	button.add_theme_color_override("font_color", text)
	button.add_theme_color_override("font_hover_color", INK)
	button.add_theme_color_override("font_pressed_color", text)
	button.add_theme_color_override("font_focus_color", INK)
	button.add_theme_color_override("font_disabled_color",
		button_font_color(variant, true, selected_when_disabled))

# ---- native selectors: the OptionButton face AND its popup -----------------
## **A DROPDOWN IS TWO SURFACES, AND STYLING ONLY THE FIRST IS THE BUG.** The face is a `Button` and
## takes `apply_button` like any other; the LIST is a `PopupMenu` living on its own `Window`, reached
## through `get_popup()`, and nothing an override sets on the OptionButton reaches it. The client
## applies no `Theme` resource (see "The checkbox indicator" in the rule file), so an unstyled popup
## wears Godot's stock light-grey menu — the `AcceptDialog` failure one widget over, and the reason
## every kit picker and overlay selector opened a grey menu over a dark console.
const POPUP_PADDING := 4
## A menu row's highlight is squarer than the panel that frames it — a full card radius on a row a few
## pixels tall reads as a lozenge.
const POPUP_HOVER_CORNER_RADIUS := 4

static func popup_panel_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = PANEL_SOLID
	sb.set_corner_radius_all(CARD_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = LINE
	sb.content_margin_left = POPUP_PADDING
	sb.content_margin_right = POPUP_PADDING
	sb.content_margin_top = POPUP_PADDING
	sb.content_margin_bottom = POPUP_PADDING
	sb.shadow_color = SHADOW_COLOR
	sb.shadow_size = 10
	return sb

## The row under the pointer. `BUTTON_PRIMARY_BG` rather than a wash of its own: a menu row being
## chosen is the same act as a primary button being pressed, and this HUD spells that one way.
static func popup_hover_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = BUTTON_PRIMARY_BG
	sb.set_corner_radius_all(POPUP_HOVER_CORNER_RADIUS)
	return sb

## Dress an `OptionButton`: the ghost chrome on its face — the SAME five styleboxes `apply_button`
## installs, taken from `button_styleboxes` so the two cannot drift — and the console's own menu on the
## popup behind it. Call it at every `OptionButton.new()` site.
##
## The popup's stock RADIO/CHECK icons are left alone: they are art, not colours, and the mark on the
## current row still reads against `PANEL_SOLID`.
static func apply_option_button(picker: OptionButton) -> void:
	if picker == null:
		return
	apply_button(picker, "ghost")
	var popup := picker.get_popup()
	if popup == null:
		return
	popup.add_theme_stylebox_override("panel", popup_panel_stylebox())
	popup.add_theme_stylebox_override("hover", popup_hover_stylebox())
	popup.add_theme_color_override("font_color", INK)
	popup.add_theme_color_override("font_hover_color", INK)
	popup.add_theme_color_override("font_disabled_color", INK_FAINT)
	popup.add_theme_color_override("font_separator_color", LINE_SOFT)

# ---- sliders ---------------------------------------------------------------
## **A SLIDER IS A TRACK, A FILL AND AN ICON**, and the icon is why this needs a raster rather than
## three styleboxes. `slider` is the unfilled groove and `grabber_area*` the filled part behind the
## handle — both `StyleBox`es, both themeable — but the HANDLE is a theme ICON, so it is unmodulated
## stock art exactly like the `CheckBox` indicator, and the `icon_*_color` family cannot reach it.
## Same fix as `apply_checkbox`: generate the art in its final palette colour and cache it.
const SLIDER_TRACK_THICKNESS := 4
const SLIDER_TRACK_CORNER_RADIUS := 2
## The generated handle. 14px sits a touch above the track's 4 so the grabber reads as sitting ON the
## groove rather than in it.
const SLIDER_GRABBER_SIZE := 14
## Kept clear of the raster's edge so the antialiased rim is not clipped flat.
const SLIDER_GRABBER_INSET := 1.0
## Coverage samples per axis within one grabber pixel — a hard circular edge at 14px stairsteps badly,
## the same reason `CHECKBOX_INDICATOR_SUPERSAMPLE` exists.
const SLIDER_GRABBER_SUPERSAMPLE := 4

## The handle raster, built once. **Dropped by `apply_palette`** alongside the four checkbox rasters:
## it bakes `INK` into pixels, so a palette installed after it was generated would leave the one stale
## colour on screen.
static var _slider_grabber: ImageTexture = null

static func _slider_grabber_texture() -> ImageTexture:
	if _slider_grabber != null:
		return _slider_grabber
	var img := Image.create_empty(SLIDER_GRABBER_SIZE, SLIDER_GRABBER_SIZE, false,
		Image.FORMAT_RGBA8)
	var samples := SLIDER_GRABBER_SUPERSAMPLE
	var per_pixel := float(samples * samples)
	var center := float(SLIDER_GRABBER_SIZE) * 0.5
	var radius := center - SLIDER_GRABBER_INSET
	for y in SLIDER_GRABBER_SIZE:
		for x in SLIDER_GRABBER_SIZE:
			var covered := 0.0
			for sy in samples:
				for sx in samples:
					var px := x + (sx + 0.5) / float(samples)
					var py := y + (sy + 0.5) / float(samples)
					if Vector2(px - center, py - center).length() <= radius:
						covered += 1.0
			img.set_pixel(x, y, Color(INK.r, INK.g, INK.b, INK.a * covered / per_pixel))
	_slider_grabber = ImageTexture.create_from_image(img)
	return _slider_grabber

static func _slider_track_stylebox(fill: Color) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = fill
	sb.set_corner_radius_all(SLIDER_TRACK_CORNER_RADIUS)
	sb.content_margin_top = SLIDER_TRACK_THICKNESS * 0.5
	sb.content_margin_bottom = SLIDER_TRACK_THICKNESS * 0.5
	return sb

## Dress a `Slider`: a `LINE_SOFT` groove, the travelled part filled in `SIGNAL_DEEP` (`SIGNAL` while
## the handle is held, the same bright/deep pair the ghost button's border uses), and the generated
## `INK` handle. Call it at every slider construction site.
##
## `grabber_disabled` takes the same raster: no slider in the client is ever disabled, and a second
## `INK_FAINT` raster would be cache and code for a state that does not arise.
static func apply_slider(slider: Slider) -> void:
	if slider == null:
		return
	slider.add_theme_stylebox_override("slider", _slider_track_stylebox(LINE_SOFT))
	slider.add_theme_stylebox_override("grabber_area", _slider_track_stylebox(SIGNAL_DEEP))
	slider.add_theme_stylebox_override("grabber_area_highlight", _slider_track_stylebox(SIGNAL))
	var grabber := _slider_grabber_texture()
	slider.add_theme_icon_override("grabber", grabber)
	slider.add_theme_icon_override("grabber_highlight", grabber)
	slider.add_theme_icon_override("grabber_disabled", grabber)

# ---- modal dialogs ---------------------------------------------------------
## **A CONFIRM PROMPT IS A CARD, NOT A SYSTEM WINDOW.** Same root cause as the checkbox below: the
## client applies no `Theme` resource, so an `AcceptDialog` wears Godot's stock chrome — a light-grey
## surface, a stock-blue focus ring and a `Confirm` title bar with an ✕ — over a near-black console.
## It read as another application's dialog dropped onto the HUD.
##
## **THE TITLE BAR IS REMOVED, NOT RESTYLED** (`borderless`). Two reasons, and the second is the one
## that decided it. A title bar on an embedded subwindow is drawn by the VIEWPORT, not by the dialog:
## `Viewport._sub_window_update` reads `title_font`/`title_color`/`title_height`/`embedded_border`
## off the **`Window`** theme type, and `add_theme_*_override` on an `AcceptDialog` is resolved
## against `AcceptDialog` — so those overrides are accepted and then never read. That is the same
## silent no-op `apply_checkbox` documents, one class boundary over. And a bar reading `Confirm`
## above a one-line question says nothing the question does not: the prompt IS the heading, and the
## ✕ is a third way to say Cancel. `borderless` deletes the whole decoration in one flag, which is
## why this is a suppression rather than a restyle.
##
## The OK button takes `primary` — this HUD's committing/chosen mark — and Cancel takes `ghost`, so
## the irreversible half is the lit one and backing out is the quiet one.

## The modal's surface. A card, but OPAQUE (`PANEL_SOLID`, not the 92% `PANEL`): a prompt sits over
## a dense work board, and text read through a translucent panel is the one place that fill costs
## legibility.
const DIALOG_PADDING_H := 18
const DIALOG_PADDING_V := 16
const DIALOG_CORNER_RADIUS := 10
## A prompt is read ONCE, deliberately, and it is the only text on screen that matters while it is
## up — so one step above the work board's 13px row rather than at it.
const DIALOG_BODY_FONT_SIZE := 14

static func dialog_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = PANEL_SOLID
	sb.set_corner_radius_all(DIALOG_CORNER_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = LINE
	sb.content_margin_left = DIALOG_PADDING_H
	sb.content_margin_right = DIALOG_PADDING_H
	sb.content_margin_top = DIALOG_PADDING_V
	sb.content_margin_bottom = DIALOG_PADDING_V
	sb.shadow_color = SHADOW_COLOR
	sb.shadow_size = 10
	sb.shadow_offset = Vector2(0.0, 8.0)
	return sb

## Dress an `AcceptDialog` (or `ConfirmationDialog`) as this console's modal — see the block above.
## Call it on every dialog the client pops, so there is one confirm surface rather than one per site.
static func apply_dialog(dialog: AcceptDialog) -> void:
	if dialog == null:
		return
	dialog.borderless = true
	dialog.add_theme_stylebox_override("panel", dialog_stylebox())
	var body := dialog.get_label()
	if body != null:
		body.add_theme_color_override("font_color", INK)
		body.add_theme_font_size_override("font_size", DIALOG_BODY_FONT_SIZE)
	apply_button(dialog.get_ok_button(), "primary")
	if dialog is ConfirmationDialog:
		apply_button((dialog as ConfirmationDialog).get_cancel_button(), "ghost")

# ---- checkboxes ------------------------------------------------------------
## **THE INDICATOR GODOT'S DEFAULT THEME CANNOT DRAW ON THIS HUD.** The client applies no `Theme`
## resource at all (`minimal_theme.tres` in this folder is an `@tool` EDITOR theme, referenced by
## nothing), so a `CheckBox` wears the stock art — which is designed for a LIGHT surface: `unchecked`
## is a FILLED near-black rounded square (`#191919` at 50% alpha) and `checked` is a light square
## carrying a dark tick. On the console's near-black panels the unchecked one is invisible: it
## reserves its width and draws nothing, so the improvement control's offered row read as a line of
## prose with no control on it.
##
## **THE COLOUR HAS TO BE IN THE PIXELS, and that is the trap here.** `CheckBox` draws its indicator
## itself, from the `checked`/`unchecked` THEME ICONS, **unmodulated** — the `icon_*_color` items
## reach a `Button`'s `icon` PROPERTY and nothing else, so `add_theme_color_override("icon_normal_
## color", …)` on a CheckBox is a silent no-op (the first cut of this fix shipped exactly that and
## rendered a stark white box). Tinting is therefore impossible through the theme, and each of the
## four indicator textures is rasterised in its final palette colour below:
##
##   UNCHECKED — the stock art is REPLACED by an empty outlined box drawn in `INK_DIM`. An OUTLINE,
##       not a fill, because an empty box is what "you may tick this" looks like; a filled one would
##       read as already ticked. `INK_DIM` rather than `INK` so the offer sits a step behind the
##       running state without disappearing.
##   CHECKED — the stock tick art is KEPT and RECOLOURED to `SIGNAL`, this HUD's live-state colour
##       (the Sight chip, the selection accent, the turn orb's calm pulse). The tick shape comes free
##       and the box reads as lit, which is the job: this control is how a player sees whether a
##       25-turn build is running, so "is it on?" must be answerable at a glance.
##   The two `_disabled` twins mirror those — the outline in `INK_FAINT`, and the stock disabled tick
##       art recoloured to `SIGNAL` (its own greyness supplies the "unavailable" reading). Neither
##       state arises today (a gated rung is a Label, and a running build is never gated), but leaving
##       them on the invisible stock art would be a hole waiting for the next caller.
##
## Called per control rather than through a project theme because the improvement control is the
## client's ONLY `CheckBox` — the Options pane's toggles are `CheckButton`s, a different widget with
## its own art — and the client applies no theme resource to hang it on anyway. If a second checkbox
## ever appears it calls this; that is what keeps the treatment in one place.

## The generated indicator matches the stock icon's 16px exactly, so swapping it moves no metrics.
const CHECKBOX_INDICATOR_SIZE := 16
## Stroke thickness and outer corner radius of that box, in indicator pixels. The radius echoes the
## rounded corners of the stock tick art it alternates with.
const CHECKBOX_INDICATOR_BORDER := 2.0
const CHECKBOX_INDICATOR_RADIUS := 3.0
## Coverage samples per axis within one indicator pixel. A hard-edged rounded corner rasterised at
## 16px stairsteps badly, so each pixel's alpha is the fraction of it the outline covers.
const CHECKBOX_INDICATOR_SUPERSAMPLE := 4

# The four indicator textures, built once on the first styled checkbox: the compose sheet rebuilds
# its controls on every selection change, and these are fixed 16×16 rasters.
static var _checkbox_unchecked: ImageTexture = null
static var _checkbox_unchecked_disabled: ImageTexture = null
static var _checkbox_checked: ImageTexture = null
static var _checkbox_checked_disabled: ImageTexture = null

## Is the sample point inside the indicator's rounded box, shrunk by `inset` on every side? The
## standard rounded-rect test: clamp the point into the box's straight-edged core, then ask whether it
## lies within `radius` of that core.
static func _checkbox_box_covers(px: float, py: float, inset: float, radius: float) -> bool:
	var core_min := inset + radius
	var core_max := float(CHECKBOX_INDICATOR_SIZE) - inset - radius
	if core_max < core_min:
		return false
	return Vector2(px - clampf(px, core_min, core_max),
		py - clampf(py, core_min, core_max)).length() <= radius

## The empty outlined box that stands in for the stock unchecked art, in `tint`.
static func _checkbox_outline(tint: Color) -> ImageTexture:
	var img := Image.create_empty(CHECKBOX_INDICATOR_SIZE, CHECKBOX_INDICATOR_SIZE, false,
		Image.FORMAT_RGBA8)
	var samples := CHECKBOX_INDICATOR_SUPERSAMPLE
	var per_pixel := float(samples * samples)
	var inner_radius: float = maxf(CHECKBOX_INDICATOR_RADIUS - CHECKBOX_INDICATOR_BORDER, 0.0)
	for y in CHECKBOX_INDICATOR_SIZE:
		for x in CHECKBOX_INDICATOR_SIZE:
			var covered := 0.0
			for sy in samples:
				for sx in samples:
					var px := x + (sx + 0.5) / float(samples)
					var py := y + (sy + 0.5) / float(samples)
					if _checkbox_box_covers(px, py, 0.0, CHECKBOX_INDICATOR_RADIUS) \
							and not _checkbox_box_covers(px, py, CHECKBOX_INDICATOR_BORDER,
								inner_radius):
						covered += 1.0
			img.set_pixel(x, y, Color(tint.r, tint.g, tint.b, tint.a * covered / per_pixel))
	return ImageTexture.create_from_image(img)

## The stock tick art multiplied through `tint`, keeping its alpha. The art is a light rounded square
## carrying a DARK tick, so the product is a `tint`-coloured chip with the tick still cut out of it —
## which is why recolouring beats drawing our own: the tick's shape is the part worth keeping.
## `copy_from` because `Texture2D.get_image()` can hand back the theme's own image, and mutating that
## would recolour every CheckBox the engine draws.
static func _checkbox_recoloured(source: Texture2D, tint: Color) -> ImageTexture:
	var img := Image.new()
	img.copy_from(source.get_image())
	img.decompress()
	img.convert(Image.FORMAT_RGBA8)
	for y in img.get_height():
		for x in img.get_width():
			var px := img.get_pixel(x, y)
			img.set_pixel(x, y, Color(px.r * tint.r, px.g * tint.g, px.b * tint.b, px.a * tint.a))
	return ImageTexture.create_from_image(img)

## Apply the console's checkbox treatment — see the block comment above for why the art is replaced
## rather than tinted.
static func apply_checkbox(box: CheckBox) -> void:
	if box == null:
		return
	if _checkbox_unchecked == null:
		# Read the stock tick art BEFORE any override lands on this box, which is why this runs here
		# and not at load: resolving a theme icon needs a control to resolve it against.
		_checkbox_unchecked = _checkbox_outline(INK_DIM)
		_checkbox_unchecked_disabled = _checkbox_outline(INK_FAINT)
		_checkbox_checked = _checkbox_recoloured(box.get_theme_icon("checked"), SIGNAL)
		_checkbox_checked_disabled = _checkbox_recoloured(
			box.get_theme_icon("checked_disabled"), SIGNAL)
	box.add_theme_icon_override("unchecked", _checkbox_unchecked)
	box.add_theme_icon_override("unchecked_disabled", _checkbox_unchecked_disabled)
	box.add_theme_icon_override("checked", _checkbox_checked)
	box.add_theme_icon_override("checked_disabled", _checkbox_checked_disabled)
	# The FONT overrides do land — the face is the Button's own `text` — so the row reads in the
	# console's ink rather than the stock theme's off-white.
	box.add_theme_color_override("font_color", INK)
	box.add_theme_color_override("font_hover_color", INK)
	box.add_theme_color_override("font_pressed_color", INK)
	box.add_theme_color_override("font_hover_pressed_color", INK)
	box.add_theme_color_override("font_focus_color", INK)
	box.add_theme_color_override("font_disabled_color", INK_FAINT)

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
	# just without the at-rest box. It reads the palette entry rather than repeating its value, so a
	# link and a ghost button can never light up in two different colours.
	sb.bg_color = GHOST_BG_HOVER
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
