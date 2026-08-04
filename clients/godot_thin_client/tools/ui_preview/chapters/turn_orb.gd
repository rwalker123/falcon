extends RefCounted

## The turn orb and its attention rows.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# The species name every orb row that names a herd must quote. `Hud._herd_label_for_id` resolves
# `game_deer_07` through the roster, the current selection and the world-herd list in that order, and
# every fixture carrying that id declares the same `species` — so the alert text is asserted against the
# ONE string all three lookups answer, never against a hand-typed copy of it.
const RED_DEER_LABEL := "Red Deer"

# The unworked-rung / under-crewed state's wire numbers (`turn_orb_unworked_rung`).
# `neglectGraceRemaining` ships as `(grace + 1) - neglect`, so every one of these is a COUNTDOWN to the
# penalty, never a count of neglected turns:
#   • NEGLECT_GRACE_SOON — the tended patch has 2 turns left. Deliberately not 1: the countdown
#     interpolates `ATTENTION_TURN_PLURAL_SUFFIX`, and at 1 the suffix is empty, so a row that dropped
#     the plural entirely would still match.
#   • NEGLECT_GRACE_NOW — the wire's `0`, which is "the ground is reverting THIS turn", the most urgent
#     reading there is. It must never render as a `0`-turn countdown.
#   • NEGLECT_GRACE_FULL — what a source that IS being kept reads (the rung's whole window). The worked
#     control carries it, so its silence is the WORKED test and not an incidentally absent countdown.
#   • NEGLECT_GRACE_HERD — the animal web's twin, on the under-crewed herd; plural for the same reason.
# The third patch of the set has NO number here at all: it carries `has_neglect_grace == false`
# (nothing at risk), which is the one reading the pair of fields exists to keep distinct from the zero.
const NEGLECT_GRACE_SOON := 2

const NEGLECT_GRACE_NOW := 0

const NEGLECT_GRACE_FULL := 4

const NEGLECT_GRACE_HERD := 3

# The under-crewed herd's staffing PAIR — 2 keepers standing where the sim demands 4. Like the
# corral-deficit pair above they must DISAGREE for the alert to fire at all, and both halves are read
# back off the RENDERED row (`2 of 4 keepers — sheds in 3 turns`).
const UNDER_CREWED_HERD_STAFFED := 2

const UNDER_CREWED_HERD_NEEDED := 4

# What the orb's registry must hold in that state: THREE unworked-rung rows out of six staged patches
# (the wild one, the rival's and the worked one raise nothing) plus the ONE under-crewed herd row.
# Counted rather than searched, because a producer that alarmed on every source would satisfy every
# positive assertion in the block without this one.
const UNWORKED_EXPECTED_ROWS := 4

# Somebody else's faction — the owner of the "not ours" control patch. Derived from the player's id so
# the two can never be written equal, which would silently turn that negative control into a positive.
const RIVAL_FACTION_ID := HudConst.PLAYER_FACTION_ID + 1

func _set_forage_patches(patches: Array) -> void:
	for p in patches:
		if p is Dictionary:
			ForageFx.floorify(p)
	h._hud.update_forage_patches(patches)

## **THE RENDERED reason rows of the open popover**, in the order they are drawn, each as
## `{label, detail}` read off the two Labels themselves — never off `TurnOrb._entries`. A registry read
## would pass on a row the popover never drew, and it would also skip the sort `set_attention` applies,
## so a claim about which row sits ABOVE which could not be made against it. The popover body is a
## header, one Button per entry, and a footer whose Advance button is nested one level deeper — so the
## body's DIRECT Button children are exactly the reason rows.
func _orb_rows() -> Array:
	var rows: Array = []
	var pop = h._hud.turn_orb._popover
	if pop == null or pop.get_child_count() == 0:
		return rows
	for row_node in pop.get_child(0).get_children():
		if not (row_node is Button) or row_node.get_child_count() == 0:
			continue
		# The row is stripe · icon · text stack · jump, and the text stack is the only VBox in it, so
		# the label/detail pair is reached structurally rather than by counting siblings.
		for cell in row_node.get_child(0).get_children():
			if not (cell is VBoxContainer) or cell.get_child_count() < 2:
				continue
			rows.append({
				"label": String((cell.get_child(0) as Label).text),
				"detail": String((cell.get_child(1) as Label).text),
			})
			break
	return rows

## The rendered row whose label is EXACTLY `label`, or `null`. Rows are found by the words the player
## reads, so a producer that fired with different text is a miss rather than a silent match.
func _orb_row_with(rows: Array, label: String) -> Variant:
	for row_variant in rows:
		var row: Dictionary = row_variant
		if String(row["label"]) == label:
			return row
	return null

const DIGIT_CHARACTERS := "0123456789"

## Does this rendered string carry ANY digit? The "renders no countdown at all" claim is asserted on
## DIGITS rather than on an absent phrase, so no rewording of a number — `0`, `in 0 turns`, `0 left` —
## can satisfy it.
func _contains_digit(text: String) -> bool:
	for i in text.length():
		if DIGIT_CHARACTERS.contains(text[i]):
			return true
	return false

## A 4-digit turn — the widest the face has to hold, and the case a fixed font size would clip.
const TURN_ORB_FOUR_DIGIT_TURN := 1200

## The slice the frozen clock is stepped by when driving the orb's resolve animation. It IS the orb's
## own per-frame clamp, and taking it from there is load-bearing rather than tidy: the orb caps how
## much of the animation ONE call may advance, so a harness stepping in bigger slices would silently
## advance less than it asked for and capture the wrong phase.
const TURN_ORB_ANIM_STEP_SEC := TurnOrb.RESOLVE_MAX_STEP_SEC

## Enough steps for the WORST path — the fail-open timeout, then a full scatter and re-form — plus a
## margin, so the cap only trips on an animation that genuinely cannot terminate.
const TURN_ORB_RESOLVE_MAX_STEPS := int((TurnOrb.RESOLVE_TIMEOUT_SEC + TurnOrb.RESOLVE_SCATTER_SEC \
	+ TurnOrb.RESOLVE_REFORM_SEC) / TURN_ORB_ANIM_STEP_SEC) * 2

## Where in a revolution `turn_orb_resolving` is captured: far enough past the scatter that the digits
## are unmistakably OFF their resting places and the sweep arc is unmistakably rotated.
const TURN_ORB_ORBIT_CAPTURE_FRACTION := 0.15

## GUARD: the turn number on the orb face must BE the turn, be sized inside the declared band, and fit
## the face's usable chord. Measured against the button's own font, exactly as `_turn_font_size` does —
## the alternative (eyeballing turn 1200) is how a clipped number ships.
func _assert_turn_face_fits(expected_turn: int) -> void:
	var orb = h._hud.turn_orb
	var face: Button = orb._face
	var text := face.text
	var size := face.get_theme_font_size("font_size")
	var font := face.get_theme_font("font")
	var budget: float = TurnOrb.FACE_DIAMETER * TurnOrb.TURN_TEXT_WIDTH_FRACTION
	var width: float = font.get_string_size(text, HORIZONTAL_ALIGNMENT_CENTER, -1, size).x
	var ok := text == str(expected_turn) \
		and size >= TurnOrb.TURN_FONT_SIZE_MIN and size <= TurnOrb.TURN_FONT_SIZE_MAX \
		and width <= budget + 1.0
	h._assert_turn_orb("turn %d on the face reads '%s' at %dpx, %.0f of %.0f wide" % [
		expected_turn, text, size, width, budget], ok)

## GUARD: the curved `TURN` word inside the face must not wrap around the circle, must not cross the
## face's edge, and must draw exactly when there IS a number for it to label. Drawn pixels cannot be
## asserted, so assert the ARITHMETIC — via the SAME `turn_word_metrics()` the draw reads, so this
## cannot pass while the renderer computes something else.
func _assert_turn_word_clears() -> void:
	var orb = h._hud.turn_orb
	var metrics: Dictionary = orb.turn_word_metrics()
	var arc_angle: float = metrics["arc_angle"]
	var outer_reach: float = float(metrics["radius"]) + float(metrics["glyph_height"])
	var face_radius: float = TurnOrb.FACE_DIAMETER * 0.5
	h._assert_turn_orb("curved '%s' spans %.0f° (ceiling %.0f°)" % [
			TurnOrb.TURN_WORD, rad_to_deg(arc_angle), rad_to_deg(TurnOrb.TURN_WORD_MAX_ARC_ANGLE)],
		arc_angle > 0.0 and arc_angle < TurnOrb.TURN_WORD_MAX_ARC_ANGLE)
	h._assert_turn_orb("curved '%s' reaches %.1f of the face's %.1f radius" % [
			TurnOrb.TURN_WORD, outer_reach, face_radius], outer_reach <= face_radius)
	# THE VISIBILITY RULE CHANGED, so these two state the new one. Hovering used to swap the number out
	# for the advance glyph and take the word with it; the number now NEVER leaves the face (the hint
	# glyph carries the affordance instead), so hover must not hide the word...
	var was_hovered: bool = orb._face_hovered
	orb._set_face_hovered(true)
	var shown_on_hover = orb._show_turn_word()
	orb._set_face_hovered(was_hovered)
	h._assert_turn_orb("curved '%s' stays while the face is hovered" % TurnOrb.TURN_WORD, shown_on_hover)
	# ...and the ONE case where there is no number to label is the resolve animation, which scatters it
	# onto the orbit ring. Driven through the REAL gate (a face click), then settled back.
	var restore_turn: int = orb._turn
	orb._on_face_pressed()
	var hidden_while_scattered = not orb._show_turn_word()
	h._assert_turn_orb("curved '%s' hides while the number is scattered" % TurnOrb.TURN_WORD,
		hidden_while_scattered)
	await _settle_turn_orb_resolve(restore_turn + 1)
	h._hud.update_overlay(restore_turn, {})
	await h._settle()

## Advance the orb's resolve animation by `seconds` of frozen-clock time, in slices the orb will
## actually honour. One big call would be clamped to `RESOLVE_MAX_STEP_SEC` and quietly under-advance.
func _step_turn_orb_anim(seconds: float) -> void:
	var remaining: float = seconds
	while remaining > 0.0:
		var slice: float = minf(remaining, TURN_ORB_ANIM_STEP_SEC)
		h._hud.turn_orb._advance_resolve_animation(slice)
		remaining -= slice

## Drive the turn orb out of its resolving gate the way a server answer does — a `set_turn` with a
## DIFFERENT value — and prove the animation actually terminates.
##
## THE CLOCK IS FROZEN HERE (`Engine.time_scale = 0`), so the orb's `_process` sees `delta == 0` and
## the re-form would never finish on its own: the same hazard `_flush_tweens` handles for the one Tween
## in the client. Step the phase machine by a fixed slice instead — deterministic, and it is the REAL
## `_advance_resolve_animation`, so a phase that cannot terminate fails right here instead of hanging
## the orb in the game.
func _settle_turn_orb_resolve(answer_turn: int) -> void:
	var orb = h._hud.turn_orb
	h._hud.update_overlay(answer_turn, {})
	for _i in range(TURN_ORB_RESOLVE_MAX_STEPS):
		if not orb.is_resolving():
			await h._settle()
			return
		orb._advance_resolve_animation(TURN_ORB_ANIM_STEP_SEC)
		await h.get_tree().process_frame
	# Through the harness's sink, not a bare `push_error`: the run's exit status is derived from the
	# failure tally, so a FAIL printed around it would be a red run reporting success.
	h._assert_turn_orb("the resolve gate never lifted in %d steps of %.2fs" % [
		TURN_ORB_RESOLVE_MAX_STEPS, TURN_ORB_ANIM_STEP_SEC], false)

## The SAME penned herd, UNDER-CREWED (`turn_orb_unworked_rung`): 2 keepers standing where the sim
## demands 4, so the shed clock has started and `neglect_grace_remaining` is counting down.
##
## **IT IS FULLY FED, ON PURPOSE.** A starving pen fires `_starving_pen_attention`'s own row off the very
## same herd, and the row COUNT is the negative control for the whole block — two producers on one herd
## would make it unreadable and would let an over-eager unworked scan hide inside the total. Its tile is
## the world-herd list's `(68, 15)` (matching the band's hunt assignment) and deliberately not the
## worked patch's `(66, 10)`, so the two webs' jump targets stay distinguishable.
func _under_crewed_herd_fixture() -> Dictionary:
	var fixture := HerdFx.domesticated_herd_fixture()
	fixture["x"] = 68
	fixture["y"] = 15
	HerdFx.set_managed_herders(fixture, UNDER_CREWED_HERD_NEEDED)
	fixture["has_neglect_grace"] = true
	fixture["neglect_grace_remaining"] = NEGLECT_GRACE_HERD
	return fixture

## **THE UNWORKED-RUNG CONTROL SET** (`turn_orb_unworked_rung`) — six patches in the wire shape
## `forage_patches_to_array` produces, of which only THREE may raise a row. Every field here is one the
## producer actually reads, and each patch differs from the one above it in exactly ONE of them, so a
## failure names the condition that broke rather than "the fixture changed":
##   (70,20) tended · ours · unworked · grace 2      → a row, counting down
##   (71,20) FIELD  · ours · unworked · grace 0      → a row, the penalty biting NOW
##   (72,20) tended · ours · unworked · NO grace     → a row with no countdown at all
##   (73,20) WILD   · ours · unworked                → silent: nothing has been built here to lose
##   (74,20) tended · a RIVAL's · unworked           → silent: a rival's ground is not our alarm
##   (66,10) tended · ours · WORKED by the band      → silent: it is being kept
## The worked control carries the FULL grace window rather than omitting the pair, so its silence can
## only come from the crew on it — an absent countdown would have silenced it for the wrong reason.
func _neglect_patches_fixture() -> Array:
	return [
		{"x": 70, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_SOON},
		{"x": 71, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": true,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_NOW},
		{"x": 72, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": false, "neglect_grace_remaining": 0},
		{"x": 73, "y": 20, "ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": false, "neglect_grace_remaining": 0},
		{"x": 74, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": RIVAL_FACTION_ID,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_NOW},
		{"x": 66, "y": 10, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_FULL},
	]

func run(harness) -> void:
	h = harness

	# State 6 — turn orb, ALL-CLEAR: a player band with zero idle workers → empty
	# attention registry → the orb calm-pulses (dashed cyan arc), the caption reads
	# "Turn 42 · ▸ all clear", and no badge shows.
	h._hud.clear_selection()
	h._hud.update_overlay(42, {})
	h._hud.update_band_alerts([
		{"faction": 0, "entity": 501, "size": 40, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 0},
	])
	await h._settle()
	await h._save("turn_orb_clear")

	# State 6a-fit — THE TURN NUMBER IS ON THE FACE, and its type size is MEASURED, not tabled
	# (`TurnOrb._turn_font_size`: step down from `TURN_FONT_SIZE_MAX` until the string fits
	# `FACE_DIAMETER * TURN_TEXT_WIDTH_FRACTION`, floored at `TURN_FONT_SIZE_MIN`). Walk 1 → 47 → 999 →
	# 1200 and assert, for each, that the rendered string is the number, that the chosen size is inside
	# the declared band, and — the point of the fit — that it actually FITS the usable chord. A 4-digit
	# turn is the case that would otherwise overflow the circle; `turn_orb_turn_4digit` is its frame.
	for probe_turn in [1, 47, 999, TURN_ORB_FOUR_DIGIT_TURN]:
		h._hud.update_overlay(probe_turn, {})
		await h._settle()
		_assert_turn_face_fits(probe_turn)
	# The curved `TURN` word above the number rides the same face. Its geometry is number-independent, so
	# one arithmetic check covers every probe; the 4-digit frame saved above is where the CLEARANCE between
	# the word and the widest number is judged by eye, at true size.
	await _assert_turn_word_clears()
	await h._save("turn_orb_turn_4digit")
	# Back to the state the following orb states describe.
	h._hud.update_overlay(42, {})
	await h._settle()

	# State 6b — turn orb, EMPTY registry, orb-face CLICK: advancing must always be possible
	# from the orb, so with nothing to triage the click ADVANCES the turn directly and opens NO
	# popover (the old bug opened a tall blank box whose Advance affordance was pushed off-screen,
	# trapping the player). Assert the emitted advance signal (the harness can't run a real turn)
	# and that no popover opened. THE CLICK NOW ALSO RAISES THE RESOLVING GATE, so the saved frame
	# shows the gate at t=0: a dimmed face, the number just beginning to break apart, and the ring's
	# rotating sweep arc where the calm pulse was.
	var advance_hits := [0]
	var advance_cb := func() -> void: advance_hits[0] += 1
	h._hud.turn_orb.advance_requested.connect(advance_cb)
	h._hud.turn_orb._on_face_pressed()
	await h._settle()
	h._assert_turn_orb("empty click advances", advance_hits[0] == 1 and not h._hud.turn_orb._popover_open)
	# THE BUG THE GATE EXISTS FOR, and the one thing a PNG can never show: mashing the face used to
	# queue N advances while the server was still resolving turn 1. A second press must emit NOTHING.
	h._hud.turn_orb._on_face_pressed()
	await h._settle()
	h._assert_turn_orb("a second click while resolving emits no advance",
		advance_hits[0] == 1 and h._hud.turn_orb.is_resolving())
	# The footer is the SECOND way to advance, so it wears the second block reason: `_advance_block_label`
	# returns "Resolving…" here where a fork would make it "Answer first to advance". Opened
	# programmatically (the face click is gated) and closed again, so the frame below is orb-only.
	h._hud.turn_orb.open_popover()
	await h._settle()
	var resolving_footer: Button = h._turn_orb_advance_button()
	h._assert_turn_orb("the popover's Advance wears the resolving reason and is disabled",
		resolving_footer != null and resolving_footer.disabled
		and resolving_footer.text == TurnOrb.ADVANCE_RESOLVING_LABEL)
	h._hud.turn_orb.toggle_popover()
	await h._settle()
	await h._save("turn_orb_clear_click_advances")

	# State 6b-resolving — THE IN-PROGRESS FRAME, mid-orbit on the very gate State 6b just raised:
	# the old number has broken apart into evenly-spaced glyphs riding a ring inside the face, the
	# ring itself carries a rotating sweep arc in the accent (NOT the calm pulse, which would say
	# "nothing needs you" mid-turn), the face is dimmed and the `TURN` word is gone with the number.
	# The clock is frozen, so the phase is STEPPED to a chosen point rather than raced.
	_step_turn_orb_anim(TurnOrb.RESOLVE_SCATTER_SEC
		+ TurnOrb.RESOLVE_ORBIT_PERIOD * TURN_ORB_ORBIT_CAPTURE_FRACTION)
	await h._settle()
	await h._save("turn_orb_resolving")

	# Answer the turn the way the server does and let the re-form finish, so the gate is DOWN for the
	# states below (6c clicks the face again and expects the popover) — then restore turn 42.
	await _settle_turn_orb_resolve(43)
	h._assert_turn_orb("the gate lifts once the re-formed number lands",
		not h._hud.turn_orb.is_resolving() and h._hud.turn_orb._face.text == "43")
	h._hud.update_overlay(42, {})
	await h._settle()

	# State 6c — turn orb, NON-EMPTY registry: the click opens the reasons popover, and the
	# popover's `Advance ▸` footer button emits advance_requested (unchanged behavior). Seed one
	# attention entry, open via the face click, then fire the footer button and assert the emit.
	advance_hits[0] = 0
	h._hud.update_band_alerts([
		{"faction": 0, "entity": 511, "size": 40, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 5},
	])
	h._hud.turn_orb._on_face_pressed()
	await h._settle()
	var opened = h._hud.turn_orb._popover_open
	var footer_btn: Button = h._turn_orb_advance_button()
	var had_footer := footer_btn != null
	if had_footer:
		footer_btn.pressed.emit()   # frees the popover (advance closes it)
	await h._settle()
	h._assert_turn_orb("non-empty popover + footer advances",
		opened and had_footer and advance_hits[0] == 1 and not h._hud.turn_orb._popover_open)
	# The footer is the OTHER advance emitter, so it raises the same gate — lower it before anything
	# below renders the orb, and put the turn back where the following states describe it.
	await _settle_turn_orb_resolve(43)
	h._hud.update_overlay(42, {})
	await h._settle()
	h._hud.turn_orb.advance_requested.disconnect(advance_cb)

	# State 6d — THE HOVER HINT, both halves of it. The turn NUMBER never leaves the face, so the
	# affordance is a small glyph BELOW it that appears on hover and names what the click will do —
	# and the two clicks are different, so the two glyphs are different. Here the registry is still
	# State 6c's (one idle-workers row), so hovering must show the up-caret `▴`: the reasons popover
	# opens ABOVE the orb, and promising `‣‣` would promise an advance this click does not perform.
	h._hud.turn_orb._set_face_hovered(true)
	await h._settle()
	h._assert_turn_orb("hovering a non-empty orb hints review, not advance",
		h._hud.turn_orb._hint_glyph == TurnOrb.HINT_GLYPH_REVIEW and h._hud.turn_orb._face.text == "42")
	await h._save("turn_orb_hint_review")

	# ...and with an EMPTY registry the same hover shows `‣‣`, because THAT click does advance. The
	# number stays on the face in both frames — that is the whole change.
	h._hud.update_band_alerts([
		{"faction": 0, "entity": 501, "size": 40, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 0},
	])
	await h._settle()
	h._assert_turn_orb("hovering an all-clear orb hints advance",
		h._hud.turn_orb._hint_glyph == TurnOrb.HINT_GLYPH_ADVANCE and h._hud.turn_orb._face.text == "42")
	await h._save("turn_orb_hint_advance")

	# ...and the same hover at the WIDEST number, which is the tight case for the stack: the face is
	# 74px and now carries the curved `TURN`, a number and this hint. Turn 1200 steps the number down
	# to 23px, so it is the frame where the hint has the MOST room below it; `turn_orb_hint_advance`
	# above (turn 42, a 30px number) is the least. Both clearances are judged here, at true size.
	h._hud.update_overlay(TURN_ORB_FOUR_DIGIT_TURN, {})
	await h._settle()
	await h._save("turn_orb_hint_4digit")
	h._hud.update_overlay(42, {})
	h._hud.turn_orb._set_face_hovered(false)
	await h._settle()

	# State 7 — turn orb, ALL THREE ATTENTION KINDS (the folded-in Alerts panel): a first
	# snapshot seeds prior band sizes so "losing population" has a baseline, then the live
	# snapshot fires one of each producer — Band 1 starving (days 3 < critical → critical/red),
	# Band 2 shrank 90→78 with emigrants (losing population → warn/amber), Band 3 has idle
	# workers (warn/amber). The badge reads "3", the pulse stops, and the popover (opened here)
	# lists all three with the starving/critical row sorted to the TOP, each with a Jump row.
	# A starving EXPEDITION is interleaved between the bands to verify the bands-only numbering:
	# it produces NO attention entry (never "Band N starving") and does not shift Band 2/Band 3's
	# positional numbers — the idle-workers row still reads "Band 3", matching the picker/header.
	h._hud.update_band_alerts([
		{"faction": 0, "entity": 601, "size": 120, "turns_of_food": 12.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		{"faction": 0, "entity": 602, "size": 90, "turns_of_food": 999.0, "activity": "hunt",
			"current_x": 31, "current_y": 21},
		{"faction": 0, "entity": 603, "size": 60, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9},
	])
	h._hud.update_band_alerts([
		# Band 1 — starving (3 turns of food, below critical).
		{"faction": 0, "entity": 601, "size": 120, "turns_of_food": 3.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		# A detached hunt expedition, also starving — must NOT emit a "Band N starving" entry and
		# must NOT consume a band number (Band 2/Band 3 below stay 2 and 3).
		{"faction": 0, "entity": 650, "size": 6, "turns_of_food": 2.0, "is_expedition": true,
			"expedition_mission": "hunt", "expedition_phase": "hunting", "home_band_entity": 601,
			"current_x": 25, "current_y": 18},
		# Band 2 — losing population: 90 → 78, well-fed but 12 emigrated last turn → "people leaving".
		{"faction": 0, "entity": 602, "size": 78, "turns_of_food": 999.0, "morale": 0.30,
			"morale_cause": 1, "last_emigrated": 12, "activity": "hunt", "current_x": 31, "current_y": 21},
		# Band 3 — idle labor: 4 working-age workers unassigned.
		{"faction": 0, "entity": 603, "size": 60, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9, "idle_workers": 4},
	])
	h._hud.turn_orb.open_popover()
	await h._settle()
	await h._save("turn_orb_attention")

	# State 7b — turn orb, AWAITING-ORDERS producer: an expedition parked at its objective is a
	# demand on the player (it burns provisions doing nothing), structurally the same class as idle
	# workers — so it produces its OWN attention row per party. Here: one band with idle workers
	# (the two producers must coexist) + FOUR awaiting parties (a scout and a hunt party name their
	# objective; the 4th trips the ATTENTION_AWAITING_MAX_ROWS cap → an aggregate "+1 more awaiting
	# orders" row). A non-awaiting (outbound) expedition proves only `awaiting` produces a row. The
	# popover must still fit above the orb with its `Advance ▸` footer on-screen.
	h._hud.turn_orb.set_attention([])   # drop State 7's registry so this frame is only these rows
	h._hud.update_band_alerts([
		{"faction": 0, "entity": 701, "size": 60, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9, "idle_workers": 4},
		{"faction": 0, "entity": 751, "size": 6, "turns_of_food": 9.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 39, "current_y": 26},
		# The hunt party names its OBJECTIVE by species (game_deer_07 → "Red Deer" via the world-herd
		# list pushed above), not the raw fauna id — the row has to be actionable at a glance.
		{"faction": 0, "entity": 752, "size": 5, "turns_of_food": 7.0, "is_expedition": true,
			"expedition_mission": "hunt", "expedition_phase": "awaiting", "home_band_entity": 701,
			"expedition_target_herd": "game_deer_07", "current_x": 64, "current_y": 11},
		{"faction": 0, "entity": 753, "size": 4, "turns_of_food": 6.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 18, "current_y": 44},
		{"faction": 0, "entity": 754, "size": 4, "turns_of_food": 5.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 51, "current_y": 8},
		{"faction": 0, "entity": 755, "size": 6, "turns_of_food": 9.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "outbound", "home_band_entity": 701,
			"current_x": 33, "current_y": 30},
	])
	h._hud.turn_orb.open_popover()
	await h._settle()
	await h._save("turn_orb_awaiting_orders")

	# State 7c — turn orb, STARVING-PEN producer: the band that keeps the pen could not pay its feed,
	# so the penned herd is shrinking every turn and 25 turns of investment are draining away. Two
	# rows here ON PURPOSE, and they are NOT the same alert twice: the empty larder is one cause with
	# two different losses — the PEOPLE are starving (critical, jumps to the band) and the HERD is
	# starving (warn, jumps to the herd, where the fed fraction + feed cost are). Only one shouts.
	h._hud.turn_orb.set_attention([])
	h._set_world_herds([HerdFx.starving_pen_herd_fixture()])
	h._hud.update_band_alerts([
		{"faction": 0, "entity": 801, "size": 46, "turns_of_food": 1.0, "activity": "hunt",
			"current_x": 64, "current_y": 11, "idle_workers": 0,
			"labor_assignments": [
				# BOTH PRODUCTS (issue #337): the hide sells beside the meat, so the drawer's standing
				# summary must read `+0.84 /turn · ⇄ +0.12` — food leading, trade shown only because it
				# is non-zero. Same `SourceForecast.source_yield_readout` the Band panel's rows use.
				{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5,
					"improvement": "corral",
					"target_x": 66, "target_y": 10, "actual_yield": 0.84, "sustainable_yield": 0.84,
					"trade_yield": 0.12, "realized_trade_yield": 0.12},
			]},
	])
	h._hud.turn_orb.open_popover()
	await h._settle()
	await h._save("turn_orb_starving_pen")
	# **THIS PRODUCER WAS DEAD, AND THE FRAME COULD NOT SAY SO** (issue #442). It found its pens by
	# `policy == "corral"`, and the axis split made `policy` always one of the four STANCES — the build
	# verb moved to `improvement` — so the test could never again be true and no starving pen had been
	# reported since. A PNG of an orb with one row in it looks entirely reasonable, which is why the
	# assertion is the only thing that could have caught it. Read off the RENDERED rows.
	var pen_rows := _orb_rows()
	h._assert_hud("the starving-pen producer still fires after the stance/improvement split",
		_orb_row_with(pen_rows, HudAttentionVocab.ATTENTION_PEN_LABEL_FORMAT % RED_DEER_LABEL) != null)
	h._set_world_herds(HerdFx.world_herds_fixture())   # restore the shared world-herd list

	h._hud.turn_orb.toggle_popover()   # close, so later states render without it

	# State 7d — turn orb, THE UNWORKED-RUNG + UNDER-CREWED producers (issue #442). A built rung nobody
	# is working is the one loss the WORK BOARD structurally cannot report: that board lists
	# ASSIGNMENTS, and an unworked patch has none, so it is ABSENT from the board rather than flagged on
	# it. The orb is the generic "something needs you" hub, so this is where it has to live — and the
	# URGENCY rides the row's own words, not a standing counter the player would learn to watch.
	#
	# The fixture is built as a set of CONTROLS, because every claim here is about which sources produce
	# a row and which do not:
	#   (70,20) tended, owned, unworked, grace 2   → a row, counting down
	#   (71,20) FIELD,  owned, unworked, grace 0   → a row, the penalty biting NOW
	#   (72,20) tended, owned, unworked, NO grace  → a row with NO countdown at all (the bool's whole job)
	#   (73,20) WILD,   owned, unworked            → NO row: nothing has been built here to lose
	#   (74,20) tended, NOT ours                   → NO row: a rival's ground is not our alarm
	#   (66,10) tended, owned, WORKED by the band  → NO row: it is being kept
	h._hud.turn_orb.toggle_popover()
	h._hud.turn_orb.set_attention([])
	_set_forage_patches(_neglect_patches_fixture())
	h._set_world_herds([_under_crewed_herd_fixture()])
	h._hud.update_band_alerts([
		{"faction": 0, "entity": 811, "size": 40, "turns_of_food": 99.0, "activity": "forage",
			"current_x": 66, "current_y": 10, "idle_workers": 0,
			"labor_assignments": [
				# The WORKED control — the same rung on the same kind of ground, kept.
				{"kind": "forage", "workers": 2, "target_x": 66, "target_y": 10, "floor": 0.5,
					"improvement": "", "actual_yield": 1.20, "sustainable_yield": 1.20},
				# The UNDER-CREWED herd: 2 keepers where the sim asks 4.
				{"kind": "hunt", "workers": UNDER_CREWED_HERD_STAFFED, "fauna_id": "game_deer_07",
					"floor": 0.5, "improvement": "",
					"target_x": 68, "target_y": 15, "actual_yield": 0.60, "sustainable_yield": 0.60},
			]},
	])
	h._hud.turn_orb.open_popover()
	await h._settle()
	await h._save("turn_orb_unworked_rung")
	var neglect_rows := _orb_rows()
	for row in neglect_rows:
		print("ui_preview: orb row  %s | %s" % [String(row["label"]), String(row["detail"])])
	var lapsing_soon: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNWORKED_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"], 70, 20])
	var lapsing_now: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNWORKED_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["sow"], 71, 20])
	var no_grace: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNWORKED_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"], 72, 20])
	h._assert_hud("an unworked Tended Patch raises a row naming the rung and the hex",
		lapsing_soon != null)
	# **THE COUNTDOWN, at N > 0.** The number is the wire's own `(grace + 1) - neglect`; the client does
	# no subtraction, so a row quoting anything else means someone re-derived it.
	h._assert_hud("…whose urgency is IN THE TEXT — `%s`"
		% (HudAttentionVocab.ATTENTION_LAPSE_SOON_FORMAT % [
			NEGLECT_GRACE_SOON, HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX]),
		lapsing_soon != null and String(lapsing_soon["detail"]) == (
			HudAttentionVocab.ATTENTION_LAPSE_SOON_FORMAT % [
				NEGLECT_GRACE_SOON, HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX]))
	# **AND AT ZERO, which is NOT "nothing at risk".** `0` is the wire's "the penalty is biting NOW" —
	# the most urgent reading there is — so it must never render as a `0`-turn countdown.
	h._assert_hud("a rung at grace 0 says the ground is reverting NOW, never `in 0 turns`",
		lapsing_now != null and String(lapsing_now["detail"]) == HudAttentionVocab.ATTENTION_LAPSE_NOW)
	# **AND THE BOOL, which is the whole reason the pair is two fields.** `has_neglect_grace == false`
	# means nothing is at risk; rendered as a countdown it would collide with the biting-now zero and
	# read as the loudest row on the card. Asserted by DIGITS, so no phrasing of a number can pass.
	h._assert_hud("a source with NO neglect grace renders no countdown at all — not even a zero",
		no_grace != null and not _contains_digit(String(no_grace["detail"])))
	# THE THREE NEGATIVE CONTROLS, counted rather than searched: a producer that alarmed on everything
	# would satisfy every positive assertion above.
	h._assert_hud("a wild patch, a rival's ground and a WORKED rung raise nothing (%d rows, not %d)"
		% [neglect_rows.size(), _neglect_patches_fixture().size()],
		neglect_rows.size() == UNWORKED_EXPECTED_ROWS)
	# THE ANIMAL HALF — under-crewed rather than unworked, because a herd carries no owner on the wire
	# and only the band's own assignment can attribute it.
	var herd_row: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNDER_CREWED_LABEL_FORMAT % RED_DEER_LABEL)
	h._assert_hud("a managed herd below its keeper count raises a row naming both counts",
		herd_row != null and String(herd_row["detail"]) == (
			HudAttentionVocab.ATTENTION_UNDER_CREWED_DETAIL_FORMAT % [
				UNDER_CREWED_HERD_STAFFED, UNDER_CREWED_HERD_NEEDED,
				HudAttentionVocab.ATTENTION_SHED_SOON_FORMAT % [
					NEGLECT_GRACE_HERD, HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX]]))
	# MOST URGENT FIRST — the rows are sorted on the wire's countdown, so the ground reverting NOW sits
	# above the one with turns left. `ATTENTION_UNWORKED_MAX_ROWS` caps the list, and a cap that kept an
	# arbitrary three would be worse than none.
	# BOTH ROWS PINNED PRESENT FIRST. `find()` answers -1 for a missing row, and -1 is less than every
	# real index — so the bare comparison PASSES when the biting-now row is absent, which is the one
	# failure this assertion exists to catch. Presence is carried by the earlier row assertions, but an
	# assertion that reads as an ordering claim must not be satisfiable by an absence.
	var now_at := neglect_rows.find(lapsing_now)
	var soon_at := neglect_rows.find(lapsing_soon)
	h._assert_hud("…and the biting-now row sorts above the one still counting down",
		now_at >= 0 and soon_at >= 0 and now_at < soon_at)
	h._hud.turn_orb.toggle_popover()
	_set_forage_patches([])              # restore: no patches for the states below
	h._set_world_herds(HerdFx.world_herds_fixture())   # restore the shared world-herd list
	h._hud.turn_orb.set_attention([])
