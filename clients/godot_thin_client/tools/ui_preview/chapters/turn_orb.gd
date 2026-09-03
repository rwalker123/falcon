extends RefCounted

## The turn orb and its attention rows.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 56

const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const KnowledgeFx := preload("res://tools/ui_preview/fixtures_knowledge.gd")
## The test tree's one transcription of the sim's rung derivation: a fixture states its standing
## rung off its own flags through this, and re-stamps after any mutation of them.
const RungFx := preload("res://tools/ui_preview/fixtures_rung.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# The species name every orb row that names a herd must quote. `Hud._herd_label_for_id` resolves
# `game_deer_07` through the roster, the current selection and the world-herd list in that order, and
# every fixture carrying that id declares the same `species` — so the alert text is asserted against the
# ONE string all three lookups answer, never against a hand-typed copy of it.
const RED_DEER_LABEL := "Red Deer"

# The COVERED herd's own id and species. Distinct from the deer's so the row COUNT is not the only
# thing separating the two halves of the pair — a row naming this species is a failure that says which
# herd it came from.
const KEPT_HERD_ID := "game_boar_11"

const KEPT_HERD_LABEL := "Wild Boar"

# The under-kept state's wire numbers (`turn_orb_under_kept`).
# `neglectGraceRemaining` ships as `(grace + 1) - neglect`, so every one of these is a COUNTDOWN to the
# penalty, never a count of neglected turns:
#   • NEGLECT_GRACE_SOON — the tended patch has 2 turns left. Deliberately not 1: the countdown
#     interpolates `ATTENTION_TURN_PLURAL_SUFFIX`, and at 1 the suffix is empty, so a row that dropped
#     the plural entirely would still match.
#   • NEGLECT_GRACE_NOW — the wire's `0`, which is "the ground is reverting THIS turn", the most urgent
#     reading there is. It must never render as a `0`-turn countdown.
#   • NEGLECT_GRACE_FULL — what a source the pool COVERS reads (the rung's whole window). The kept
#     control carries it, so its silence is the SHORTFALL test and not an incidentally absent countdown.
#   • NEGLECT_GRACE_HERD — the animal web's twin, on the under-kept herd; plural for the same reason.
# The third patch of the set has NO number here at all: it carries `has_neglect_grace == false`
# (nothing at risk), which is the one reading the pair of fields exists to keep distinct from the zero.
const NEGLECT_GRACE_SOON := 2

const NEGLECT_GRACE_NOW := 0

const NEGLECT_GRACE_FULL := 4

const NEGLECT_GRACE_HERD := 3

# **THE KEEPING BILLS THE CONTROL SET IS STAGED AT** — `intensification_ladder.json`'s own
# `upkeep.work_per_turn` for the two plant rungs, both `scaled_by: flat`, so these are the ladder's
# numbers verbatim on every patch in the game.
const PLANT_TENDED_UPKEEP_DEMAND := 2.0

const PLANT_FIELD_UPKEEP_DEMAND := 4.0

# What the band's Agriculture pool actually paid each SHORT patch. Deliberately NOT zero: the row
# quotes the SHORTFALL, so a supplied of nothing would make the shortfall and the demand the same
# number and a producer quoting the wrong one of the two would read correct.
const PLANT_TENDED_UPKEEP_SUPPLIED := 0.5

const PLANT_FIELD_UPKEEP_SUPPLIED := 1.0

# `ceil(demand / PER_WORKER_OUTPUT)` — what the sim publishes as `upkeepWorkersNeeded`, and the
# producer's "does this source cost anything to hold at all" half. A `0` here is a wild source, which
# is why the wild control carries one.
const PLANT_TENDED_KEEPERS_WANTED := 2

const PLANT_FIELD_KEEPERS_WANTED := 4

# The crew standing on the WORKED patch and on the two herds. It is the TAKE crew in both cases and
# the producer no longer reads it at all — which is the point: the worked-and-short patch and the
# covered-but-idle one are what prove the gate moved off it.
const UNDER_KEPT_TAKE_CREW := 2

# The keepers the two herds ASK for. The covered herd's pool meets its bill with fewer hands than this
# — that is the reported defect exactly — so this number must sit ABOVE `UNDER_KEPT_TAKE_CREW` on the
# covered herd or its silence would prove nothing.
const UNDER_KEPT_HERD_KEEPERS := 4

# What the orb's registry must hold in that state: THREE under-kept plant rows out of six staged
# patches (the wild one, the rival's and the COVERED one raise nothing) plus the ONE under-kept herd
# row out of two. Counted rather than searched, because a producer that alarmed on every source would
# satisfy every positive assertion in the block without this one.
const UNDER_KEPT_EXPECTED_ROWS := 4

# Somebody else's faction — the owner of the "not ours" control patch. Derived from the player's id so
# the two can never be written equal, which would silently turn that negative control into a positive.
const RIVAL_FACTION_ID := HudConst.PLAYER_FACTION_ID + 1

# The two turns the knowledge block walks. Distinct from every other turn this chapter drives, so a
# diff that failed to roll shows up as a claim about the wrong turn rather than as a silent pass.
const KNOWLEDGE_TURN_LEARNED := 610

const KNOWLEDGE_TURN_NEXT := 611

# **FOUR LADDER TRACKS THAT UNLOCK SOMETHING TO STAND ON**, taught in one turn, so the producer has
# to emit four rows — and so all four are left UNSPENT, which is what makes the absence claim below a
# real one. `foddering` is deliberately absent: the ROSTER marks it a capability rather than a step
# (no rung's `unlock_knowledge` names it), so it can never be unspent and would weaken that claim.
# The route branch's two are left out for a different reason — a fifth and sixth row would only make
# the exact-count claim below harder to read.
const KNOWLEDGE_TRACKS_TAUGHT := {
	"cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0,
}

# One `X learned` row per track and NOTHING ELSE. The band half is emptied and the one calm band
# contributes nothing, so this is the whole registry: a fifth row is either a band producer firing or
# the retired unspent-backlog row coming back.
const KNOWLEDGE_LEARNED_ROWS := 4

# The placeholder `ATTENTION_KNOWLEDGE_LEARNED_LABEL_FORMAT` opens with, and the only thing about
# that format this chapter spells. `_learned_suffix` strips it to get the tail every producer-1 row
# must end with, so the rows are found by the format the client really composes rather than by a
# hand-typed copy of one of its outputs.
const KNOWLEDGE_LEARNED_FORMAT_TOKEN := "%s"

# The affordance a non-locating row that a panel branch answers must wear. It is `TurnOrb`'s own
# literal — the orb composes it inline — so this is the one string here that is a transcription, and
# it is the whole assertion: a knowledge kind missing from `ATTENTION_KINDS_WITH_A_PANEL` renders a
# BLANK here, and a kind missing from `TurnOrbController`'s branch renders this and does nothing.
const OPEN_AFFORDANCE := "Open ▸"

## GUARD: the orb's KNOWLEDGE half is producing nothing at all right now. Asked of the PRODUCER,
## not of the rendered rows, because it has to be answerable with the popover shut — which is the
## state most of this chapter's band claims are made in.
func _assert_knowledge_half_silent(claim: String) -> void:
	var rows := AttentionController.knowledge_attention(h._hud.knowledge_panel().nodes())
	h._assert_turn_orb("%s (%d rows)" % [claim, rows.size()], rows.is_empty())

## The tail of `ATTENTION_KNOWLEDGE_LEARNED_LABEL_FORMAT` — see `KNOWLEDGE_LEARNED_FORMAT_TOKEN`.
## A `func` rather than a `const` because `substr` is not a constant expression.
static func _learned_suffix() -> String:
	return HudAttentionVocab.ATTENTION_KNOWLEDGE_LEARNED_LABEL_FORMAT.substr(
		KNOWLEDGE_LEARNED_FORMAT_TOKEN.length())

## Every rendered row whose label matches `"<X> learned"` — producer 1's output, found by the
## FORMAT rather than by kind, so a row that fired with the wrong words is a miss.
func _learned_rows(rows: Array) -> Array:
	var found: Array = []
	for row_variant in rows:
		var row: Dictionary = row_variant
		var label := String(row["label"])
		if label.ends_with(_learned_suffix()):
			found.append(row)
	return found

## Push the player faction's ladder tracks and let the whole HUD settle. `update_intensification` is
## the seam that rolls the knowledge screen's turn diff, and `update_overlay` ahead of it is what
## carries the turn that diff rolls AGAINST — the same order `Main` dispatches them in.
## **THE ROSTER IS PUSHED FIRST**: it declares what there is to learn, and without it the knowledge
## screen has no ladder nodes at all — so no track could ever be reported learned.
func _teach(turn: int, tracks: Dictionary) -> void:
	h._hud.update_ladder_knowledge(KnowledgeFx.ladder_roster())
	h._hud.update_overlay(turn, {})
	h._hud.update_intensification([KnowledgeFx.progress_row(HudConst.PLAYER_FACTION_ID, tracks)])
	await h._settle()

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
			# **AND THE AFFORDANCE**, which is the last child of the row's own HBox: `Jump →` for a
			# locating row, `Open ▸` for a non-locating kind that a panel branch answers, and EMPTY for
			# one that neither locates nor opens. Read here rather than asserted off the kind, because
			# the failure this catches is a row that WEARS the affordance and does nothing when pressed.
			var jump := ""
			var last: Node = row_node.get_child(0).get_child(row_node.get_child(0).get_child_count() - 1)
			if last is Label:
				jump = String((last as Label).text)
			rows.append({
				"label": String((cell.get_child(0) as Label).text),
				"detail": String((cell.get_child(1) as Label).text),
				"jump": jump,
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

## The SAME penned herd, UNDER-KEPT (`turn_orb_under_kept`): its Husbandry pool did not cover this
## herd's bill, so the shed clock has started and `neglect_grace_remaining` is counting down. The
## shortfall comes off `HerdFx.domesticated_herd_fixture`, which stages a pen paid half its rate.
##
## **IT IS FULLY FED, ON PURPOSE.** A starving pen fires `_starving_pen_attention`'s own row off the very
## same herd, and the row COUNT is the negative control for the whole block — two producers on one herd
## would make it unreadable and would let an over-eager scan hide inside the total. Its tile is
## the world-herd list's `(68, 15)` (matching the band's hunt assignment) and deliberately not the
## kept patch's `(66, 10)`, so the two webs' jump targets stay distinguishable.
func _under_kept_herd_fixture() -> Dictionary:
	var fixture := HerdFx.domesticated_herd_fixture()
	fixture["x"] = 68
	fixture["y"] = 15
	HerdFx.set_managed_herders(fixture, UNDER_KEPT_HERD_KEEPERS)
	fixture["has_neglect_grace"] = true
	fixture["neglect_grace_remaining"] = NEGLECT_GRACE_HERD
	return fixture

## **THE HERD THE POOL COVERS, AND IT IS THE REPORTED DEFECT ITSELF** (`turn_orb_under_kept`). Same
## rung, same species roster, same fed pen — and its Husbandry share meets its whole bill, so
## `upkeepShortfall` is zero and the orb must say NOTHING about it.
##
## **ITS HUNT PARTY IS SMALLER THAN ITS KEEPER DEMAND, WHICH IS WHAT MAKES IT A CONTROL RATHER THAN A
## FIXTURE.** The retired test compared the take crew against `upkeepWorkersNeeded`, so this herd —
## covered, calm, and hunted by two hands where the sim names four keepers — is exactly the shape it
## fired on every turn. A shortfall test is silent here; the take-crew test is not.
##
## It stands on its own tile so the two herds' jumps stay distinguishable, and carries its own id and
## species so a row about either one names which.
func _kept_herd_fixture() -> Dictionary:
	var fixture := HerdFx.domesticated_herd_fixture()
	fixture["id"] = KEPT_HERD_ID
	fixture["species"] = KEPT_HERD_LABEL
	fixture["x"] = 69
	fixture["y"] = 15
	HerdFx.set_managed_herders(fixture, UNDER_KEPT_HERD_KEEPERS)
	# The pool covered it: supplied MEETS demand, so the published shortfall is nothing at all.
	fixture["upkeep_supplied"] = HerdFx.ANIMAL_PEN_UPKEEP_DEMAND
	fixture["upkeep_shortfall"] = 0.0
	# The rung's whole grace window, i.e. what a herd reads on a turn its keeping was met — so its
	# silence can only come from the shortfall and never from an incidentally absent countdown.
	fixture["has_neglect_grace"] = true
	fixture["neglect_grace_remaining"] = NEGLECT_GRACE_FULL
	return fixture

## **THE UNDER-KEPT CONTROL SET** (`turn_orb_under_kept`) — six patches in the wire shape
## `forage_patches_to_array` produces, of which only THREE may raise a row. Every field here is one the
## producer actually reads, and each patch differs from the one above it in exactly ONE of them, so a
## failure names the condition that broke rather than "the fixture changed":
##   (70,20) tended · ours · short · grace 2 · WORKED  → a row, counting down
##   (71,20) FIELD  · ours · short · grace 0           → a row, the penalty biting NOW
##   (72,20) tended · ours · short · NO grace          → a row with no countdown at all
##   (73,20) WILD   · ours · owes nothing              → silent: nothing has been built here to lose
##   (74,20) tended · a RIVAL's · short                → silent: a rival's ground is not our alarm
##   (66,10) tended · ours · COVERED · UNWORKED        → silent: the pool paid its bill
##
## **THE LAST TWO OF THOSE ARE THE PAIR THE FIX IS ABOUT.** The retired test asked *is anybody
## FORAGING this patch*, so (66,10) — kept, and nobody gathering on it — alarmed every turn, while
## (70,20) — harvested, and its keeping underpaid — was silent. The crew moved onto the short patch
## and the kept one lost its crew for exactly that reason; the producer reads neither now.
##
## The covered control carries the FULL grace window rather than omitting the pair, so its silence can
## only come from the shortfall — an absent countdown would have silenced it for the wrong reason.
func _neglect_patches_fixture() -> Array:
	return RungFx.stamp_patches([
		{"x": 70, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"upkeep_demand": PLANT_TENDED_UPKEEP_DEMAND,
			"upkeep_supplied": PLANT_TENDED_UPKEEP_SUPPLIED,
			"upkeep_shortfall": PLANT_TENDED_UPKEEP_DEMAND - PLANT_TENDED_UPKEEP_SUPPLIED,
			"upkeep_workers_needed": PLANT_TENDED_KEEPERS_WANTED,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_SOON},
		{"x": 71, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": true,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"upkeep_demand": PLANT_FIELD_UPKEEP_DEMAND,
			"upkeep_supplied": PLANT_FIELD_UPKEEP_SUPPLIED,
			"upkeep_shortfall": PLANT_FIELD_UPKEEP_DEMAND - PLANT_FIELD_UPKEEP_SUPPLIED,
			"upkeep_workers_needed": PLANT_FIELD_KEEPERS_WANTED,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_NOW},
		{"x": 72, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"upkeep_demand": PLANT_TENDED_UPKEEP_DEMAND,
			"upkeep_supplied": PLANT_TENDED_UPKEEP_SUPPLIED,
			"upkeep_shortfall": PLANT_TENDED_UPKEEP_DEMAND - PLANT_TENDED_UPKEEP_SUPPLIED,
			"upkeep_workers_needed": PLANT_TENDED_KEEPERS_WANTED,
			"has_neglect_grace": false, "neglect_grace_remaining": 0},
		# A wild patch stands on no rung, so it owes nothing and is asked for nobody — the `0` keeper
		# count is the sim's own "this source costs nothing to hold", which is the producer's own gate.
		{"x": 73, "y": 20, "ecology_phase": "thriving", "is_cultivated": false, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"upkeep_demand": 0.0, "upkeep_supplied": 0.0, "upkeep_shortfall": 0.0,
			"upkeep_workers_needed": 0,
			"has_neglect_grace": false, "neglect_grace_remaining": 0},
		{"x": 74, "y": 20, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": RIVAL_FACTION_ID,
			"upkeep_demand": PLANT_TENDED_UPKEEP_DEMAND,
			"upkeep_supplied": PLANT_TENDED_UPKEEP_SUPPLIED,
			"upkeep_shortfall": PLANT_TENDED_UPKEEP_DEMAND - PLANT_TENDED_UPKEEP_SUPPLIED,
			"upkeep_workers_needed": PLANT_TENDED_KEEPERS_WANTED,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_NOW},
		{"x": 66, "y": 10, "ecology_phase": "thriving", "is_cultivated": true, "is_field": false,
			"has_owner": true, "owner": HudConst.PLAYER_FACTION_ID,
			"upkeep_demand": PLANT_TENDED_UPKEEP_DEMAND,
			"upkeep_supplied": PLANT_TENDED_UPKEEP_DEMAND,
			"upkeep_shortfall": 0.0,
			"upkeep_workers_needed": PLANT_TENDED_KEEPERS_WANTED,
			"has_neglect_grace": true, "neglect_grace_remaining": NEGLECT_GRACE_FULL},
	])

## The under-kept detail line the producer must compose for one source, built from the VOCABULARY and
## the fixture's own numbers rather than from `AttentionController`'s own composer — an expectation
## made out of the code under test can only agree with itself.
func _expected_under_kept_detail(role: String, shortfall: float, clause: String) -> String:
	return HudAttentionVocab.ATTENTION_UNDER_KEPT_DETAIL_FORMAT % [
		role, DetailFormat.format_work_units(shortfall), clause]

## The CONSEQUENCE half of an under-kept detail — everything after the pool's own bill, which is the
## only place a countdown can appear. The bill itself carries digits now, so the *renders no countdown
## at all* claim is made about this tail; it splits on the same separator the row was joined with, so
## a reworded format cannot leave the two reading different halves.
func _under_kept_clause(detail: String) -> String:
	var cut := detail.rfind(HudAttentionVocab.ATTENTION_CLAUSE_SEPARATOR)
	if cut < 0:
		return detail
	return detail.substr(cut + HudAttentionVocab.ATTENTION_CLAUSE_SEPARATOR.length())

## ---- THE CREW HAND-OFF INGEST (Producer 8) -----------------------------------------------------
## The turn the hand-off fixtures are stamped for, and one well inside the sim's retention window
## behind it. `command_events` is per-frame HISTORY: a delta carries the rows appended since the
## client's cursor, a FULL snapshot carries the whole retained ring — so both ticks arrive in one
## array on every connect and on every resync, and only the event's own `tick` tells them apart.
const HANDOFF_TURN := 61

const HANDOFF_OLD_TURN := HANDOFF_TURN - 9

## Two hand-offs really did happen this turn, and they are DIFFERENT rows (one crew carried onto the
## rung's keeping, one freed), so a producer that de-duplicated on the wrong thing collapses them.
const HANDOFF_ROWS_THIS_TURN := 2

## The sim's own `status=` / `action=` detail shapes, spelled out here rather than composed through
## `AttentionController`'s own tokens: an expectation built from the code under test can only agree
## with itself. Copied from `systems::labor`'s completion pass.
const HANDOFF_DETAIL_CARRIED := "status=carried_to_upkeep action=build_complete improvement=cultivate workers=3"

const HANDOFF_DETAIL_FREED := "status=freed action=build_complete improvement=corral workers=2"

## A full snapshot's ring, in the shape the decoder hands over: this turn's two hand-offs plus older
## ones the retention window still holds, each carrying its own `tick` and its own monotonic `seq`.
## The old rows are hand-offs in every other respect — same action token, same status tokens — because
## a filter that keyed on anything but the tick would let them through.
func _handoff_ring_fixture() -> Array:
	return [
		{"tick": HANDOFF_OLD_TURN, "seq": 401, "kind": "cultivate",
			"label": "4 of your cultivate crew stay on (12, 8) to keep it",
			"detail": HANDOFF_DETAIL_CARRIED},
		{"tick": HANDOFF_OLD_TURN + 3, "seq": 402, "kind": "corral",
			"label": "1 of your corral crew are free — Red Deer keeps itself",
			"detail": HANDOFF_DETAIL_FREED},
		{"tick": HANDOFF_TURN, "seq": 403, "kind": "cultivate",
			"label": "3 of your cultivate crew stay on (31, 18) to keep it",
			"detail": HANDOFF_DETAIL_CARRIED},
		{"tick": HANDOFF_TURN, "seq": 404, "kind": "corral",
			"label": "2 of your corral crew are free — Wild Boar keeps itself",
			"detail": HANDOFF_DETAIL_FREED},
	]

## Just this turn's two rows — what a mid-tick RECAPTURE delta re-ships, every row since the turn's
## baseline, at their own unchanged `seq`s.
func _handoff_recapture_fixture() -> Array:
	var ring := _handoff_ring_fixture()
	return [ring[2], ring[3]]

## How many hand-off rows the producer would put on the orb right now.
func _handoff_row_count() -> int:
	var rows := 0
	for item_variant in h._hud._attention.build_band_attention([], []):
		var item: Dictionary = item_variant
		if String(item.get("kind", "")) == HudAttentionVocab.ATTENTION_KIND_CREW_HANDOFF:
			rows += 1
	return rows

## **THE INGEST IS A WINDOW ON ONE TURN, AND IT HAS TO SURVIVE BOTH FRAME SHAPES.** It read EVERY row
## whose detail carried the action token, off whatever array the frame happened to bring — so a full
## snapshot re-dated the last twenty turns of hand-offs to now and flooded the orb, and a recapture
## delta announced this turn's twice. Two filters, two questions (WHICH TURN, and SEEN ALREADY), and
## neither substitutes for the other: the ring exercises the first, the recapture the second.
##
## The rows are capped at `ATTENTION_HANDOFF_MAX_ROWS` with an overflow row, so the fixture's older
## rows are chosen to push the count PAST that cap — a flood that stayed under it would be counted
## correctly and still be wrong.
func _assert_handoff_ingest_windows_on_one_turn() -> void:
	var prior_turn: int = h._hud._band_labor.current_turn()
	h._hud._band_labor.set_turn(HANDOFF_TURN)
	h._hud._attention.ingest_command_events(_handoff_ring_fixture(), HANDOFF_TURN)
	var after_ring := _handoff_row_count()
	h._assert_hud(
		"a full snapshot's whole retained ring announces only THIS turn's hand-offs (got %d, want %d)"
			% [after_ring, HANDOFF_ROWS_THIS_TURN],
		after_ring == HANDOFF_ROWS_THIS_TURN)
	# **THE RECAPTURE, on top of the ring the client has already taken.** Same turn, same `seq`s, so
	# nothing new has happened and the count must not move. Asserted AFTER the ring rather than on a
	# clean slate, because that is the live order — a delta always lands on an already-ingested turn.
	h._hud._attention.ingest_command_events(_handoff_recapture_fixture(), HANDOFF_TURN)
	var after_recapture := _handoff_row_count()
	h._assert_hud(
		"…and a mid-tick recapture re-shipping them announces each ONCE (got %d, want %d)"
			% [after_recapture, HANDOFF_ROWS_THIS_TURN],
		after_recapture == HANDOFF_ROWS_THIS_TURN)
	# THE VACUITY GUARD, and it is not decorative: both claims above are satisfied by a producer that
	# has stopped ingesting hand-offs at all. Advancing the turn drops the window and the SAME rows,
	# re-stamped for the new turn, must be announced again.
	h._hud._band_labor.set_turn(HANDOFF_TURN + 1)
	var re_stamped := _handoff_recapture_fixture()
	for row_variant in re_stamped:
		(row_variant as Dictionary)["tick"] = HANDOFF_TURN + 1
	h._hud._attention.ingest_command_events(re_stamped, HANDOFF_TURN + 1)
	var after_next_turn := _handoff_row_count()
	h._assert_hud("…and the next turn's own hand-offs are still announced (got %d, want %d)"
		% [after_next_turn, HANDOFF_ROWS_THIS_TURN],
		after_next_turn == HANDOFF_ROWS_THIS_TURN)
	# **RESTORE, and the CLEAR only happens on a turn CHANGE** — an empty array ingested against the
	# turn already held leaves the window exactly as it was, which would leak three rows into every
	# state after this one.
	h._hud._band_labor.set_turn(prior_turn)
	h._hud._attention.ingest_command_events([], prior_turn)

# ---- THE DECLINE REASON ON FREEZING GROUND (issue #614) ----------------------------------------
#
# `_decline_reason` could say four things — starving, people leaving, a morale cause, low morale —
# and a band freezing to death hits NONE of them: temperature mortality is food-independent and
# leaves morale clamped at 100 %. The `losing population` row therefore rendered with an EMPTY detail
# line, which is exactly the reported *"I knew people were dying, but I didn't know why."*

## Five bands, each isolating one branch of the priority order. Every one of them SHRINKS between the
## baseline and the live fixture, so the row exists in all five cases and only its reason differs.
const DECLINE_COLD_ENTITY := 801       # the reported case: freezing, benign on every other axis
const DECLINE_HEAT_ENTITY := 802       # the symmetric tail
const DECLINE_MILD_ENTITY := 803       # survivable ground — the negative, still says nothing
const DECLINE_BOTH_ENTITY := 804       # freezing AND emigrating — lethal must win
const DECLINE_STARVING_ENTITY := 805   # freezing AND starving — an empty larder must still win

const DECLINE_COLD_TILE := Vector2i(14, 5)
const DECLINE_HEAT_TILE := Vector2i(15, 5)
const DECLINE_MILD_TILE := Vector2i(16, 5)

## Read against the prologue's shipped tuning — the survivable band is `[0.0, 40.0]` °C, two
## independent onsets rather than a spread around an ambient. Deliberately well past the onset on
## each side: this state is about which REASON answers, not about the rate's printing.
##
## The heat reading is above what worldgen can produce today (it tops out near 31 °C); the heat onset
## is calibrated to the range issue #622 opens up, so a "corrected" reachable value would fall inside
## the survivable band and this state would silently stop covering the heat tail.
const DECLINE_LETHAL_COLD_TEMPERATURE := -4.0
const DECLINE_LETHAL_HEAT_TEMPERATURE := 45.0
const DECLINE_SURVIVABLE_TEMPERATURE := 18.0

## Morale at its ceiling — the value a freezing band really carries, and the reason the morale
## branches cannot answer for it.
const DECLINE_FULL_MORALE := 1.0
## Below `BandFoodStatus.critical_turns()`, so the starving branch genuinely fires.
const DECLINE_STARVING_TURNS := 2.0
const DECLINE_BASELINE_SIZE := 60
const DECLINE_LIVE_SIZE := 54
const DECLINE_EMIGRATED := 6

## The prior snapshot, so every band below has a size to have shrunk FROM — the producer reads
## `prev_band_sizes`, and without this pass there is no `losing population` row to inspect at all.
func _decline_baseline_fixture() -> Array:
	var bands: Array = []
	for band in _decline_live_fixture():
		var prior: Dictionary = band.duplicate()
		prior["size"] = DECLINE_BASELINE_SIZE
		bands.append(prior)
	return bands

## The live snapshot. **The first three are benign on every axis but temperature** — full larder, no
## emigrants, morale 1.0, no morale cause — which is what makes the cold one the reported screen and
## the mild one an honest negative. The last two carry a SECOND cause each, so a lethal branch
## inserted at the wrong priority still answers a non-empty string and would look fixed.
func _decline_live_fixture() -> Array:
	return [
		_decline_band(DECLINE_COLD_ENTITY, DECLINE_COLD_TILE),
		_decline_band(DECLINE_HEAT_ENTITY, DECLINE_HEAT_TILE),
		_decline_band(DECLINE_MILD_ENTITY, DECLINE_MILD_TILE),
		_decline_band(DECLINE_BOTH_ENTITY, DECLINE_COLD_TILE, DECLINE_EMIGRATED),
		_decline_band(DECLINE_STARVING_ENTITY, DECLINE_COLD_TILE, 0, DECLINE_STARVING_TURNS),
	]

func _decline_band(entity: int, tile: Vector2i, emigrated: int = 0,
		turns: float = BandFoodStatus.UNLIMITED_TURNS) -> Dictionary:
	return {
		"faction": 0, "entity": entity, "size": DECLINE_LIVE_SIZE,
		"turns_of_food": turns, "morale": DECLINE_FULL_MORALE,
		"morale_cause": DetailFormat.MORALE_CAUSE_NONE,
		"last_emigrated": emigrated, "activity": "forage",
		"current_x": tile.x, "current_y": tile.y,
	}

## The `losing population` row this band produced, or `{}` — read off the registry the ORB was HANDED
## (`TurnOrb._entries`), not by re-running the producer.
##
## ⛔ **RE-RUNNING IT HERE WOULD ANSWER NOTHING, AND SILENTLY.** Producer 2 compares the live size
## against `_band_labor.prev_band_sizes()`, which `update_band_alerts` OVERWRITES with the live sizes
## immediately after building the attention array — so a second call sees every band at its own
## current size, finds no decline anywhere, and hands back an empty list that looks like "no such
## row". The build is a one-shot on the pre-ingest sizes; the registry is where its output lives.
func _decline_row_for(entity: int) -> Dictionary:
	for item_variant in h._hud.turn_orb._entries:
		var item: Dictionary = item_variant
		if String(item.get("kind", "")) == HudAttentionVocab.ATTENTION_KIND_LOSING_POPULATION \
				and int(item.get("owner", -1)) == entity:
			return item
	return {}

func _decline_detail_for(entity: int) -> String:
	return String(_decline_row_for(entity).get("detail", "<no losing-population row>"))

## The live band entry itself, so the preconditions can be asserted off the same fixture the row was
## built from rather than restated as literals here.
func _decline_band_for(entity: int) -> Dictionary:
	for band_variant in _decline_live_fixture():
		var band: Dictionary = band_variant
		if int(band.get("entity", -1)) == entity:
			return band
	return {}

func run(harness) -> void:
	h = harness
	# **THE FACTION IS TAUGHT NOTHING FOR THE BAND STATES, AND THAT IS A CONTROL, NOT A TIDY-UP.**
	# The knowledge screen's producer (`docs/plan_knowledge_screen.md` §5) rides the SAME registry these
	# band rows do, and it is FACTION-wide rather than per-band. The walk's chapters push tracks without
	# always advancing the turn, so the screen's diff has not rolled since whichever of them last did —
	# and the first turn tick HERE rolls everything they taught in between onto one orb. Measured before
	# it was cleared: a `Cultivation learned` row riding State 6, which would make the ALL-CLEAR states
	# not clear and would put a fifth row inside the under-kept block's negative-control COUNT — a count
	# whose whole job is to say no producer over-fired. Staged deliberately at the END of the chapter
	# instead.
	#
	# **AND IT IS PUT BACK ON THE WAY OUT — the tracks are SHARED WALK STATE, not this chapter's.**
	# `compose_rungs` runs three chapters later and renders its hunt-compose frames against whatever
	# knowledge it inherits: `RungGates` gates the rung options on it, so a faction taught nothing
	# offers a different ladder, a different crew requirement and a different forecast. Leaving the
	# tracks cleared moved four of those frames — `hunt_picker_ascending`, `hunt_crew_herders`,
	# `herd_assign_button_targets_selected_herd`, `hunt_actions_rhythm` — into judging a compose sheet
	# under knowledge the chapter never meant to change. Captured by VALUE, because `faction_tracks`
	# hands back the retained dict itself rather than a copy.
	var inherited_tracks: Dictionary = h._hud._topbar.faction_tracks(
		HudConst.PLAYER_FACTION_ID).duplicate()
	h._hud.update_intensification([KnowledgeFx.progress_row(HudConst.PLAYER_FACTION_ID, {})])
	await h._settle()
	_assert_knowledge_half_silent("the band states run with the knowledge producer silent")

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
				# The drawer's standing summary reads through the same
				# `SourceForecast.source_yield_readout` the Band panel's rows use. (It carried a second,
				# trade-goods clause until arc #527 retired that account.)
				{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5,
					"improvement": "corral",
					"target_x": 66, "target_y": 10, "actual_yield": 0.84, "sustainable_yield": 0.84},
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

	# State 7d — turn orb, THE UNDER-KEPT producers, both webs (issue #442; the TEST corrected against
	# `docs/plan_standing_upkeep.md` §2.5). A source whose keeping the band's pool did not cover is a
	# loss the WORK BOARD structurally cannot report: that board lists ASSIGNMENTS, and an under-kept
	# source may have none, so it is ABSENT from the board rather than flagged on it. The orb is the
	# generic "something needs you" hub — and the URGENCY rides the row's own words, not a standing
	# counter the player would learn to watch.
	#
	# The fixture is built as a set of CONTROLS, because every claim here is about which sources produce
	# a row and which do not:
	#   (70,20) tended, owned, SHORT, grace 2, WORKED → a row, counting down
	#   (71,20) FIELD,  owned, SHORT, grace 0         → a row, the penalty biting NOW
	#   (72,20) tended, owned, SHORT, NO grace        → a row with NO countdown at all (the bool's whole job)
	#   (73,20) WILD,   owned, owes nothing           → NO row: nothing has been built here to lose
	#   (74,20) tended, NOT ours, SHORT               → NO row: a rival's ground is not our alarm
	#   (66,10) tended, owned, COVERED, UNWORKED      → NO row: the pool paid its bill
	#   (68,15) penned herd, SHORT                    → a row
	#   (69,15) penned herd, COVERED, hunt party < its keeper demand → NO row
	#
	# **THE TWO PAIRS ARE THE POINT.** The retired tests compared a TAKE crew against a KEEPING demand
	# — "is anybody foraging this patch" on the plant web, the hunt party against `upkeepWorkersNeeded`
	# on the animal one — so the covered patch and the covered herd both alarmed while the short
	# harvested patch stayed silent. Restore either test and one half of each pair fails.
	h._hud.turn_orb.toggle_popover()
	h._hud.turn_orb.set_attention([])
	_set_forage_patches(_neglect_patches_fixture())
	h._set_world_herds([_under_kept_herd_fixture(), _kept_herd_fixture()])
	h._hud.update_band_alerts([
		{"faction": 0, "entity": 811, "size": 40, "turns_of_food": 99.0, "activity": "forage",
			"current_x": 66, "current_y": 10, "idle_workers": 0,
			"labor_assignments": [
				# The crew stands on the SHORT patch, not the covered one — the half of the plant pair
				# a take-crew test reads backwards.
				{"kind": "forage", "workers": UNDER_KEPT_TAKE_CREW, "target_x": 70, "target_y": 20,
					"floor": 0.5,
					"improvement": "", "actual_yield": 1.20, "sustainable_yield": 1.20},
				# The UNDER-KEPT herd, and the attribution the walk needs — a herd carries no owner.
				{"kind": "hunt", "workers": UNDER_KEPT_TAKE_CREW, "fauna_id": "game_deer_07",
					"floor": 0.5, "improvement": "",
					"target_x": 68, "target_y": 15, "actual_yield": 0.60, "sustainable_yield": 0.60},
				# The COVERED herd, attributed the same way, and hunted by fewer hands than its keeper
				# demand names — which is precisely the shape the retired test fired on.
				{"kind": "hunt", "workers": UNDER_KEPT_TAKE_CREW, "fauna_id": KEPT_HERD_ID,
					"floor": 0.5, "improvement": "",
					"target_x": 69, "target_y": 15, "actual_yield": 0.55, "sustainable_yield": 0.55},
			]},
	])
	h._hud.turn_orb.open_popover()
	await h._settle()
	await h._save("turn_orb_under_kept")
	var neglect_rows := _orb_rows()
	for row in neglect_rows:
		print("ui_preview: orb row  %s | %s" % [String(row["label"]), String(row["detail"])])
	var lapsing_soon: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNDER_KEPT_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"], 70, 20])
	var lapsing_now: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNDER_KEPT_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["sow"], 71, 20])
	var no_grace: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNDER_KEPT_LABEL_FORMAT % [
			HudComposeVocab.IMPROVEMENT_DONE_LABELS["cultivate"], 72, 20])
	# **THE PLANT PAIR'S POSITIVE HALF.** This patch is being HARVESTED and its keeping is underpaid,
	# which is the case the retired crew test was silent on.
	h._assert_hud("a WORKED Tended Patch whose keeping is short raises a row naming the rung and the hex",
		lapsing_soon != null)
	# **THE COUNTDOWN, at N > 0, BESIDE THE POOL THE ROW SENDS THE PLAYER TO.** The number is the wire's
	# own `(grace + 1) - neglect` and the bill is its published shortfall; the client does no arithmetic
	# on either, so a row quoting anything else means someone re-derived it.
	var soon_detail := _expected_under_kept_detail(HudWorkVocab.ROLE_NAME_AGRICULTURE,
		PLANT_TENDED_UPKEEP_DEMAND - PLANT_TENDED_UPKEEP_SUPPLIED,
		HudAttentionVocab.ATTENTION_LAPSE_SOON_FORMAT % [
			NEGLECT_GRACE_SOON, HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX])
	h._assert_hud("…whose detail names the Agriculture pool, its bill in WORK, and the countdown — `%s`"
		% soon_detail,
		lapsing_soon != null and String(lapsing_soon["detail"]) == soon_detail)
	# **AND AT ZERO, which is NOT "nothing at risk".** `0` is the wire's "the penalty is biting NOW" —
	# the most urgent reading there is — so it must never render as a `0`-turn countdown. The FIELD's own
	# bill rides it, so a producer quoting one rung's rate on every row fails here rather than above.
	var now_detail := _expected_under_kept_detail(HudWorkVocab.ROLE_NAME_AGRICULTURE,
		PLANT_FIELD_UPKEEP_DEMAND - PLANT_FIELD_UPKEEP_SUPPLIED,
		HudAttentionVocab.ATTENTION_LAPSE_NOW)
	h._assert_hud("a rung at grace 0 says the ground is reverting NOW, never `in 0 turns`",
		lapsing_now != null and String(lapsing_now["detail"]) == now_detail)
	# **AND THE BOOL, which is the whole reason the pair is two fields.** `has_neglect_grace == false`
	# means no countdown was published; rendered as one it would collide with the biting-now zero and
	# read as the loudest row on the card. Asserted by DIGITS over the CONSEQUENCE half alone — the
	# bill before it is a number, so a whole-line digit test would now fail on a correct row.
	h._assert_hud("a source with NO neglect grace renders no countdown at all — not even a zero",
		no_grace != null and not _contains_digit(_under_kept_clause(String(no_grace["detail"]))))
	# THE THREE PLANT NEGATIVE CONTROLS, counted rather than searched: a producer that alarmed on
	# everything would satisfy every positive assertion above. The COVERED patch is the reported defect
	# — kept by the pool, worked by nobody — and the retired test raised a row on it every turn.
	h._assert_hud("a wild patch, a rival's ground and a COVERED rung raise nothing (%d rows, not %d)"
		% [neglect_rows.size(), _neglect_patches_fixture().size()],
		neglect_rows.size() == UNDER_KEPT_EXPECTED_ROWS)
	# THE ANIMAL HALF — attributed through the band's own assignment, because a herd carries no owner on
	# the wire.
	var herd_row: Variant = _orb_row_with(neglect_rows,
		HudAttentionVocab.ATTENTION_UNDER_KEPT_HERD_LABEL_FORMAT % RED_DEER_LABEL)
	var herd_detail := _expected_under_kept_detail(HudWorkVocab.ROLE_NAME_HUSBANDRY,
		HerdFx.ANIMAL_PEN_UPKEEP_DEMAND - HerdFx.ANIMAL_PEN_UPKEEP_SUPPLIED,
		HudAttentionVocab.ATTENTION_SHED_SOON_FORMAT % [
			NEGLECT_GRACE_HERD, HudAttentionVocab.ATTENTION_TURN_PLURAL_SUFFIX])
	h._assert_hud("a herd whose Husbandry pool came up short raises a row naming the pool and the bill",
		herd_row != null and String(herd_row["detail"]) == herd_detail)
	# **THE ANIMAL PAIR'S NEGATIVE HALF, AND IT IS THE PLAYTEST REPORT ITSELF.** This herd's pool covers
	# it (`upkeepShortfall == 0`) while its hunting party is smaller than the keeper count the sim names
	# — which is the ordinary state of every managed herd, and what the retired take-crew test alarmed
	# on. The row COUNT above already forbids it; this names WHICH herd, so a failure is legible.
	h._assert_hud("a herd the pool COVERS raises no row, however small its hunting party",
		_orb_row_with(neglect_rows,
			HudAttentionVocab.ATTENTION_UNDER_KEPT_HERD_LABEL_FORMAT % KEPT_HERD_LABEL) == null)
	# **AND THE ORB ASKS THE SAME QUESTION THE CARD ASKS.** Both surfaces now call
	# `SourceForecast.is_under_kept`, so the claim is the GATE rather than the wording: asserted over
	# BOTH herds, since a gate that answered `true` for everything would satisfy the first half alone.
	h._assert_hud("…and the orb's gate IS the card's gate, on both herds",
		SourceForecast.is_under_kept(_under_kept_herd_fixture(),
			HudComposeVocab.BARE_FORECAST_PREFIX)
		and not SourceForecast.is_under_kept(_kept_herd_fixture(),
			HudComposeVocab.BARE_FORECAST_PREFIX))
	# MOST URGENT FIRST — the rows are sorted on the wire's countdown, so the ground reverting NOW sits
	# above the one with turns left. `ATTENTION_UNDER_KEPT_MAX_ROWS` caps the list, and a cap that kept an
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

	await _assert_knowledge_producers(inherited_tracks)

	# ---- THE CREW HAND-OFF INGEST IS A WINDOW ON ONE TURN (Producer 8) -----------------------------
	# **PNG-LESS AND DRIVEN, because a picture cannot make either claim.** Both failures render a
	# perfectly ordinary popover — the rows are correctly shaped, correctly worded and correctly
	# inked; there are simply too many of them, and only counting says so. So the producer is asked
	# directly, over an events array in each of the two shapes the wire really delivers.
	_assert_handoff_ingest_windows_on_one_turn()

## **THE KNOWLEDGE SCREEN'S ORB ROW** (`docs/plan_knowledge_screen.md` §5, slice C) — one row per
## discovery that finished this turn, **and nothing else at all**.
##
## **THE FIXTURE TEACHES FOUR TRACKS IN ONE TURN AND LEAVES ALL FOUR UNSPENT**, which is what lets
## the block make its two claims with one staging. Taught: the producer must render four rows, one
## per track and each naming its own — an aggregated producer fails the count, and one that named the
## first track four times passes the count and fails the naming. Unspent: the registry must hold
## those four rows and NO backlog row, §5's aggregate `"N discoveries unspent"` having been built and
## then cut (see `AttentionController.knowledge_attention`).
##
## Then the turn ticks with nothing newly taught and the same four still unspent: the row goes quiet
## and the orb reads ALL-CLEAR. That is the DISJOINT half — an implementation that latched the
## announcement fails the first claim, and one that re-added the backlog row fails the second, and
## the all-clear beside a full backlog is the property the cut exists to buy.
func _assert_knowledge_producers(inherited_tracks: Dictionary) -> void:
	# **THE BAND HALF IS EMPTIED AT THE CACHE, NOT BY INGESTING A CALM BAND**, so the registry the
	# popover draws is exactly the knowledge half. A calm-band `update_band_alerts` was the first cut
	# and it is not inert: `ingest_snapshot_bands` overwrites the walk's `player_band` / `player_bands`
	# / `prev_band_sizes`, and `compose_rungs` — three chapters later — renders its hunt-compose frames
	# against that roster's idle workers. It moved `hunt_crew_herders` and `hunt_picker_ascending` into
	# judging a crew stepper against a band this chapter had swapped underneath them.
	#
	# Clearing the CACHE also leaves the chapter tidier than it found it: `TurnOrb.set_attention([])`
	# only empties the NODE, so the under-kept block's four rows stayed in `_band_attention` and any
	# later `_push_attention` would resurrect them.
	h._hud.turn_orb.set_attention([])
	h._hud._turnorb.set_band_attention([])
	# **THE TURN IS SHARED WALK STATE TOO.** This block drives turns of its own, and the chapters after
	# it render whatever the walk is left on — `docks_legend`'s `reserved_dock` puts the orb FACE in a
	# frame, so a chapter that wandered off and stayed there changes that number for a reason that has
	# nothing to do with what it was testing. Captured here rather than at the top of `run`: what has to
	# be handed on is the turn the chapter WOULD have ended on, not the one it started with.
	var handed_on_turn: int = h._hud._band_labor.current_turn()
	await _teach(KNOWLEDGE_TURN_LEARNED, KNOWLEDGE_TRACKS_TAUGHT)
	h._hud.turn_orb.open_popover()
	await h._settle()
	await h._save("turn_orb_knowledge")
	var rows := _orb_rows()
	for row in rows:
		print("ui_preview: orb row  %s | %s | %s" % [
			String(row["label"]), String(row["detail"]), String(row["jump"])])
	# **PRODUCER 1 IS ONE ROW PER TRACK.** Counted, not searched: a producer that emitted one row for
	# the whole turn would still satisfy a "is `Cultivation learned` present" test.
	var learned := _learned_rows(rows)
	h._assert_hud("knowledge — %d tracks finishing in one turn raise %d rows, one each (got %d)"
			% [KNOWLEDGE_TRACKS_TAUGHT.size(), KNOWLEDGE_LEARNED_ROWS, learned.size()],
		learned.size() == KNOWLEDGE_LEARNED_ROWS)
	# …and each names its OWN discovery, in the words the knowledge screen's column uses. Asserted over
	# every taught track rather than one of them, so a producer naming the first track four times passes
	# the count above and fails here.
	var all_named := true
	for track in KNOWLEDGE_TRACKS_TAUGHT:
		var wanted := HudAttentionVocab.ATTENTION_KNOWLEDGE_LEARNED_LABEL_FORMAT % String(
			KnowledgeFx.label_for(track))
		if _orb_row_with(rows, wanted) == null:
			all_named = false
	h._assert_hud("…each naming its own discovery, `%s`-style"
			% (HudAttentionVocab.ATTENTION_KNOWLEDGE_LEARNED_LABEL_FORMAT % String(
				KnowledgeFx.label_for(KnowledgeFx.KNOWLEDGE_CULTIVATION))),
		all_named)
	# **THE ROW WEARS `Open ▸`, AND THAT IS A CLAIM ABOUT TWO LISTS AT ONCE** — the kind must be in
	# `HudAttentionVocab.ATTENTION_KINDS_WITH_A_PANEL` (or the affordance is blank) and it must have a
	# branch in `TurnOrbController._on_turn_orb_panel_requested` (or the affordance is a lie). Where the
	# press LANDS is the panel's own state and is asserted in `chapters/knowledge_panel.gd`.
	var every_row_opens := true
	for row in rows:
		if String(row["jump"]) != OPEN_AFFORDANCE:
			every_row_opens = false
	h._assert_hud("…and every knowledge row is non-locating, wearing `%s` rather than a jump"
			% OPEN_AFFORDANCE,
		every_row_opens)
	# **AND THE ORB SAYS NOTHING ABOUT THE UNSPENT BACKLOG — the retired second producer.** §5 asked for
	# an aggregate `"N discoveries unspent"` row; it was built and cut, because a STANDING row never goes
	# away and the orb never returns to its calm all-clear pulse. Asserted as an EXACT ROW COUNT over a
	# faction that is sitting on four unspent discoveries, so re-adding the row in any wording fails
	# here. Its precondition is the next line: without genuinely-unspent discoveries this claim is
	# vacuous, and a fixture whose sources happened to use the knowledge would satisfy it for free.
	h._assert_hud("knowledge — the registry holds exactly those %d rows and no backlog row (got %d)"
			% [KNOWLEDGE_LEARNED_ROWS, rows.size()],
		rows.size() == KNOWLEDGE_LEARNED_ROWS)
	var unspent: int = h._hud.knowledge_panel().unspent_count()
	h._assert_hud("…and the faction really IS sitting on %d unspent discoveries, so that is not vacuous"
			% unspent,
		unspent == KNOWLEDGE_LEARNED_ROWS)

	# **THE NEXT TURN, WITH NOTHING NEWLY TAUGHT.** The row answers *this turn*, so it has to go quiet
	# again — while the same four discoveries are still unspent, which is what makes this the other half
	# of the claim above rather than a repeat of it: the orb goes ALL-CLEAR on a faction with a full
	# backlog, which is the whole reason the backlog row was cut.
	await _teach(KNOWLEDGE_TURN_NEXT, KNOWLEDGE_TRACKS_TAUGHT)
	h._hud.turn_orb.open_popover()
	await h._settle()
	var next_rows := _orb_rows()
	h._assert_hud("knowledge — the next turn teaches nothing, so no discovery is announced (got %d)"
			% _learned_rows(next_rows).size(),
		_learned_rows(next_rows).is_empty())
	h._assert_hud("…and the orb is ALL-CLEAR beside %d unspent discoveries (got %d rows)"
			% [h._hud.knowledge_panel().unspent_count(), next_rows.size()],
		next_rows.is_empty() and h._hud.knowledge_panel().unspent_count() == KNOWLEDGE_LEARNED_ROWS)

	# **RESTORE WHAT THE CHAPTER INHERITED — the TURN as well as the tracks** (see the note at the top
	# of `run`, and the one beside `handed_on_turn`).
	#
	# **THE TRACKS GO BACK TWICE, AT TWO DIFFERENT TURNS, and that is what leaves the producer silent.**
	# The screen's diff only rolls when the turn MOVES, so one push would roll the taught set out of the
	# baseline and report whatever the inherited tracks hold that the taught four did not — a
	# `Foddering learned` row landing on some later chapter's frame. The second push rolls the inherited
	# set against itself and so reports nothing, whatever it happens to contain.
	h._hud.turn_orb.toggle_popover()
	await _teach(handed_on_turn - 1, inherited_tracks)
	await _teach(handed_on_turn, inherited_tracks)
	_assert_knowledge_half_silent("the chapter hands on with the knowledge producer silent again")
	h._hud.turn_orb.set_attention([])

	# State 7z — **"I KNEW PEOPLE WERE DYING, BUT I DIDN'T KNOW WHY."** (issue #614) The reported
	# playtest case, reproduced exactly: a band on ground the sim is freezing, with a FULL LARDER, NO
	# emigrants and morale at 100 %. Temperature mortality reaches none of `_decline_reason`'s tests —
	# it is food-independent and leaves morale clamped — so the `losing population` row rendered with
	# an EMPTY detail line, which IS the bug: *people are dying* with the why blank.
	#
	# The detail is the claim, so it is asserted rather than left to the frame; a blank line and a
	# filled one are a few pixels apart in a popover, and the blank one is what shipped.
	h._hud.turn_orb.set_attention([])
	h._hud._band_labor.set_tile_temperatures({
		Vector2i(DECLINE_COLD_TILE.x, DECLINE_COLD_TILE.y): DECLINE_LETHAL_COLD_TEMPERATURE,
		Vector2i(DECLINE_HEAT_TILE.x, DECLINE_HEAT_TILE.y): DECLINE_LETHAL_HEAT_TEMPERATURE,
		Vector2i(DECLINE_MILD_TILE.x, DECLINE_MILD_TILE.y): DECLINE_SURVIVABLE_TEMPERATURE,
	})
	h._hud.update_band_alerts(_decline_baseline_fixture())
	h._hud.update_band_alerts(_decline_live_fixture())
	h._hud.turn_orb.open_popover()
	await h._settle()
	await h._save("turn_orb_decline_lethal_cold")
	h._assert_hud("a band shrinking on FREEZING ground says so, where the row used to say nothing",
		_decline_detail_for(DECLINE_COLD_ENTITY) == HudAttentionVocab.DECLINE_REASON_LETHAL_COLD)
	h._assert_hud("…the heat tail names ITS tail, not the cold one",
		_decline_detail_for(DECLINE_HEAT_ENTITY) == HudAttentionVocab.DECLINE_REASON_LETHAL_HEAT)
	# **THE THREE PRECONDITIONS THAT MAKE THE CLAIM MEAN ANYTHING.** Without them the row could be
	# reading "starving" or "people leaving" and passing for the wrong reason entirely — the fixture
	# is deliberately benign on every other axis, which is what made the original blank possible.
	h._assert_hud("precondition: the freezing band is NOT starving, NOT emigrating and at full morale",
		_decline_band_for(DECLINE_COLD_ENTITY).get("turns_of_food") == BandFoodStatus.UNLIMITED_TURNS \
			and int(_decline_band_for(DECLINE_COLD_ENTITY).get("last_emigrated", 0)) == 0 \
			and float(_decline_band_for(DECLINE_COLD_ENTITY).get("morale", 0.0)) == DECLINE_FULL_MORALE)
	# …and the other half of the gate: SURVIVABLE ground with the same benign band still says nothing,
	# so the new branch cannot be firing on every shrinking band.
	h._assert_hud("…while the same band on survivable ground still reports no reason at all",
		_decline_detail_for(DECLINE_MILD_ENTITY) == "")
	# **LETHAL OUTRANKS EMIGRATION, AND ONLY AN EMPTY LARDER OUTRANKS LETHAL.** The freezing band here
	# also has emigrants and a morale cause, so a branch inserted in the wrong place still answers a
	# non-empty string and would look fixed.
	h._assert_hud("lethal ground outranks people-leaving on a band doing both",
		_decline_detail_for(DECLINE_BOTH_ENTITY) == HudAttentionVocab.DECLINE_REASON_LETHAL_COLD)
	h._assert_hud("…and starving still outranks lethal ground",
		_decline_detail_for(DECLINE_STARVING_ENTITY) == HudAttentionVocab.DECLINE_REASON_STARVING)
	h._hud.turn_orb.toggle_popover()   # the orb's only close seam; leaves the walk as it found it
	h._hud.turn_orb.set_attention([])
	h._hud._band_labor.set_tile_temperatures({})
