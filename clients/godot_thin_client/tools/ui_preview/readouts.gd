## Reading structured values back out of rendered HUD text.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

const Spine := preload("res://tools/ui_preview/compose_vocab.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")

const CREW_TARGET_ABSENT := -1

## The teaching line's two halves, as the needles that tell them apart: a lesson still being earned
## leads with the verb, and a lesson already known states only the BUILD the same multiplier paces.
const TEACHING_LESSON_NEEDLE := "Teaching"

const TEACHING_BUILD_NEEDLE := "Building at ×"

## Is this rung the SELECTED one? Read off the `normal` stylebox's fill, which `HudStyle.apply_button`
## writes from the variant — `BUTTON_PRIMARY_BG` is the one marker of "this is the chosen rung". It is
## read here rather than the `disabled` box because a rung can now be selected AND gated at once
## (issue #420): Godot then DRAWS the disabled box, but the variant the button was styled with is
## still recorded on `normal`, so this answers "which rung is lit?" in both states.
static func rung_is_selected(btn: Button) -> bool:
	if btn == null:
		return false
	var box := btn.get_theme_stylebox("normal")
	return box is StyleBoxFlat \
		and (box as StyleBoxFlat).bg_color.is_equal_approx(HudStyle.BUTTON_PRIMARY_BG)

## The COUNT a crew target is offering, read off the face it renders — or `CREW_TARGET_ABSENT` when
## that target is not rendered at all. The two answers are different claims: `0` says "nothing needs
## clearing", absent says "this source's crew cannot be priced".
static func crew_target_count(root: Node, key: String) -> int:
	var button := Q.find_crew_target(root, key)
	if button == null:
		return CREW_TARGET_ABSENT
	# **READ OFF THE META, NEVER THE FACE.** The pill's face is a two-Label stack over an
	# empty-`text` Button (a count and its label at one size are one undifferentiated phrase), so the
	# old `button.text.split(" ")[0]` finds an empty string here — and `int("")` is 0, which is a REAL
	# reading of this control ("nothing needs clearing"). It would have passed silently.
	return int(button.get_meta(HudWidgets.CREW_TARGET_COUNT_META, CREW_TARGET_ABSENT))

## The READOUT's yields row as one string — every Label in it, joined. The row is found by
## `HudWidgets.YIELDS_ROW_META`, its identity: its face is a flow of Labels at three sizes (the
## number, the unit + its route, the take's qualifier), so there is no single `text` to match and a
## needle search across the sheet would find whichever Label happened to hold it. "" when no readout
## rendered, which fails a `contains` assertion rather than satisfying it.
static func yields_text(root: Node) -> String:
	var row := Q.find_meta_node(root, HudWidgets.YIELDS_ROW_META)
	return " ".join(face_lines(row)) if row != null else ""

## ONE ACCOUNT'S NUMBER out of the yields row — the reading beside the unit `account` names, or
## `YIELDS_ACCOUNT_ABSENT` when that account renders no row at all. The two answers are different
## claims, and both matter here: a locked account still HAS a row (unit kept, number replaced by the
## em-dash), so an assertion that could not tell "muted" from "gone" would pass on a row the panel had
## silently dropped — which is the hidden gate this repo forbids.
##
## Structural, like the spine walk: a reading is a number Label followed by its unit Label inside one
## `HBoxContainer`, and the unit renders UPPERCASE. A `contains` over `yields_text` cannot do this
## job — `—` and `0.00` both appear in other registers of the same box.
const YIELDS_ACCOUNT_ABSENT := "<absent>"

static func yields_account_number(root: Node, account: String) -> String:
	var row := Q.find_meta_node(root, HudWidgets.YIELDS_ROW_META)
	if row == null:
		return YIELDS_ACCOUNT_ABSENT
	for pair in row.get_children():
		var lines := face_lines(pair)
		if lines.size() >= 2 and lines[1] == account.to_upper():
			return lines[0]
	return YIELDS_ACCOUNT_ABSENT

## The aside's LOCKED-ACCOUNT line alone, by its own meta — the twin of `teaching_line`, and separate
## for the same reason: its siblings move with the floor while this one does not, so a whole-aside
## comparison is satisfied by them and testifies about this sentence in neither direction.
static func locked_account_line(root: Node) -> String:
	var node := Q.find_meta_node(root, HudWidgets.READOUT_LOCKED_ACCOUNT_META)
	return (node as Label).text if node is Label else ""

## The readout's HEADER — the caption over the yields row, carrying the unit and (when the readings
## state one) the key to their arrow. It is the row's SIBLING, not a Label inside it, which is what
## keeps `yields_text` reading only the numbers: asserting "the unit is not repeated per account"
## against a string that included the header would pass on a row that repeated nothing and a header
## that said everything. "" when no readout rendered.
static func yields_header(root: Node) -> String:
	var row := Q.find_meta_node(root, HudWidgets.YIELDS_ROW_META)
	if row == null or row.get_parent() == null:
		return ""
	var index := row.get_index()
	if index <= 0:
		return ""
	var caption := row.get_parent().get_child(index - 1)
	return (caption as Label).text if caption is Label else ""

## The CREW ROW's label — `HUNTERS` / `HERDERS` / `FORAGERS`, the crew noun the sheet resolved off the
## composed improvement axis. By meta rather than by text, because the sheet's EYEBROW two rows above
## carries the same noun in the same case (`ASSIGN HUNTERS`), so a search would match it and pass
## without ever reaching the crew row. "" when there is no crew row.
## The READOUT's ASIDE as one string — its lines joined. Found by `HudWidgets.READOUT_ASIDE_META`,
## its identity: every line is a plain Label at one size, so there is no distinguishing face, and the
## teaching line's own text carries a live multiplier that a needle would have to be re-tuned against
## every time a fixture's floor moved. "" when no aside rendered.
static func readout_aside_text(root: Node) -> String:
	var block := Q.find_meta_node(root, HudWidgets.READOUT_ASIDE_META)
	return " ".join(face_lines(block)) if block != null else ""

## The teaching line ALONE, by its own meta. Its aside siblings move with the floor too, so a
## whole-aside comparison is satisfied by them and cannot testify about this sentence — proven, by
## blanking the note and watching the aside-wide form still pass.
static func teaching_line(root: Node) -> String:
	var node := Q.find_meta_node(root, HudWidgets.READOUT_TEACHING_META)
	return (node as Label).text if node is Label else ""

static func crew_row_label(root: Node) -> String:
	var node := Q.find_meta_node(root, HudWidgets.CREW_ROW_LABEL_META)
	return (node as Label).text if node is Label else ""

## The crew row's BUILD-DIP note, by its own meta — `""` when none rendered, which is a real reading
## and half of what the note is asserted on: a line that appears on every sheet claims nothing. Not
## found by text, and not by scanning the row: the row LABEL sits beside it and renders either way.
static func crew_row_dip_note(root: Node) -> String:
	var node := Q.find_meta_node(root, HudWidgets.CREW_ROW_DIP_META)
	return (node as Label).text if node is Label else ""

## The verdict's SEVERITY (`SourceForecast.VERDICT_*`), which is its assertable half — the sentence
## carries turn counts and percentages that move with the fixture. "" when no verdict rendered.
static func verdict_severity(root: Node) -> String:
	var node := Q.find_meta_node(root, HudWidgets.VERDICT_META)
	return String((node as Control).get_meta(HudWidgets.VERDICT_META, "")) if node != null else ""

## Every Label text under `root`, in tree order — the rung face's lines as they are stacked.
static func face_lines(root: Node) -> Array[String]:
	var lines: Array[String] = []
	if root == null:
		return lines
	if root is Label:
		lines.append((root as Label).text)
	for child in root.get_children():
		lines.append_array(face_lines(child))
	return lines

# ---- the compose sheets' SHARED VERTICAL GRAMMAR ------------------------------------------------
# The forage sheet and the hunt sheet ask the same two questions in the same act — WHICH STANCE, and
# WITH HOW MANY PEOPLE — and they must ask them in the same order, because a player moving between the
# two is reading one control layout, not two. The hunt sheet used to put its crew stepper directly
# under the band picker, i.e. staff first and decide after; both now read
#   band picker → stance picker → (hint) → crew stepper → … → improvement.
#
# A FRAME CANNOT HOLD THAT CLAIM. Two PNGs side by side show the order to a human who thinks to look,
# and nothing fails when one of them moves — which is exactly how they drifted apart. So the invariant
# is asserted as a SPINE: the ordered structural controls of the open sheet, with the prose between
# them (hints, cap notes, forecasts, gate reasons, the plant web's crop rows) deliberately EXCLUDED.
# The two webs legitimately say different things in different places; what must match is the order in
# which the controls come.

## The crew a stepper is SHOWING — the value Label `HudWidgets.add_stepper_controls` lays between the
## `−` and the `+`. Structural, like the spine walk: a stepper carries no meta, so it is found by that
## `−` face and the value is its next sibling.
##
## It exists because "the frame renders the N-worker crew" was being asserted against
## `ComposeState.hunt_count()` — the model the harness itself had just dialed. That is a real test of
## the CLAMP (the render writes the clamped count back), but it is not a test of the readout, and a
## stepper drawing any other number would pass it. `STEPPER_VALUE_ABSENT` on a missing stepper, so a
## sheet that never opened fails an equality claim rather than satisfying it.
const STEPPER_VALUE_ABSENT := -1

static func stepper_value(root: Node) -> int:
	var minus := Q.find_button_by_text(root, Spine.COMPOSE_STEPPER_MINUS_FACE)
	if minus == null or minus.get_parent() == null:
		return STEPPER_VALUE_ABSENT
	var siblings := minus.get_parent().get_children()
	var index := siblings.find(minus)
	if index < 0 or index + 1 >= siblings.size():
		return STEPPER_VALUE_ABSENT
	var value: Node = siblings[index + 1]
	return int((value as Label).text) if value is Label else STEPPER_VALUE_ABSENT

## The index of the `Key: value` row with this key, or -1. Matches the key EXACTLY (up to the
## `DetailFormat` separator) so `Foraging` cannot be found by a row that merely mentions it.
static func detail_row_index(lines: Array[String], key: String) -> int:
	var prefix := key + DetailFormat.DETAIL_KV_SEPARATOR
	for index in lines.size():
		if lines[index].begins_with(prefix):
			return index
	return -1
