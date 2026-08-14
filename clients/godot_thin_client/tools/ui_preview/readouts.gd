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

## **DOES ANY READING STATE A `now → after` TRANSITION?** — asked of the row's own faces, through the
## same glyph the widget draws (`SourceForecast.YIELD_AFTER_GLYPH`, which `YIELD_AFTER_FORMAT` is
## written in terms of), so the mark looked for and the mark rendered are one string.
##
## It is the ROW's half of a claim the caption cannot make alone: a sheet whose caption dropped
## `now → after` while the readings kept their arrows is exactly the mismatch the two used to be
## composed separately enough to produce, and a header assertion passes right through it.
static func yields_show_a_transition(root: Node) -> bool:
	return yields_text(root).contains(SourceForecast.YIELD_AFTER_GLYPH)

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

## The READOUT's IMPROVEMENT-DEAL block as one string — its key and its value joined, so the payoff
## row reads `ONCE TENDED 1.39 food`. Found by `HudWidgets.IMPROVEMENT_DEAL_META`, its identity: the
## row is a key/value Label pair at two sizes carrying live numbers, so there is no single `text` to
## match — and a needle search across the sheet would find whichever register happened to hold the
## same magnitude.
##
## "" when no deal block rendered, which is a REAL reading and half of what every payoff assertion is
## made of: "the payoff left the face" also passes on a sheet that lost the payoff altogether, so a
## claim about the face's absence must be paired with a `contains` here, which "" fails.
static func improvement_deal_text(root: Node) -> String:
	var block := Q.find_meta_node(root, HudWidgets.IMPROVEMENT_DEAL_META)
	return " ".join(face_lines(block)) if block != null else ""

## What `improvement_deal_value` answers when no deal block rendered. A SENTINEL, not "": an absent
## block and a row whose value is empty are different findings, and a claim comparing the deal's
## magnitude against another register must fail loudly on the first rather than parse `""` into a
## `0.0` that happens to satisfy it.
const DEAL_ROW_ABSENT := "<row absent>"

## The deal row's VALUE ALONE — the payoff terms without their key. It takes NO key parameter,
## because the block is exactly one row: a key argument with one possible value is the same
## unused-API liability the builder's collapsed array parameter was.
static func improvement_deal_value(root: Node) -> String:
	var block := Q.find_meta_node(root, HudWidgets.IMPROVEMENT_DEAL_META)
	if block == null:
		return DEAL_ROW_ABSENT
	var lines := face_lines(block)
	return lines[1] if lines.size() >= 2 else DEAL_ROW_ABSENT

## HOW MANY ROWS the deal block renders — `0` when it does not render at all.
##
## **IT EXISTS TO PIN A REMOVAL.** The block briefly carried a second row stating the crew's undipped
## take, and it went because the baseline is visible by simply unticking the box — and because on a
## crew that saturates the source the dip costs nothing, so the row printed the SAME numbers as the
## headline directly above it, under a crew note saying each carries half as much. No text assertion
## can catch that coming back: a re-added baseline row satisfies every `contains` claim on this
## block, and its numbers are legitimate. The COUNT is the claim.
static func improvement_deal_rows(root: Node) -> int:
	var block := Q.find_meta_node(root, HudWidgets.IMPROVEMENT_DEAL_META)
	return block.get_child_count() if block != null else 0

## Every magnitude-looking token in `text`, as strings — the tokens a reader would compare between
## two registers of the readout. Deliberately textual: what matters is whether the same NUMBER
## appears twice on screen, which is a claim about what is printed rather than about two floats.
static func magnitudes_in(text: String) -> Array[String]:
	var found: Array[String] = []
	for token in text.split(" ", false):
		if token.is_valid_float() and token.contains("."):
			found.append(token)
	return found

## **DOES THE DEAL ROW REPEAT A NUMBER THE YIELDS ROW ALREADY PRINTS?** The defect that retired the
## baseline row: at a crew that saturates the source the dip costs nothing, so the undipped take and
## the dipped headline were the same figure printed twice, one under a caption saying they differ.
## Asked here rather than in each chapter so the two webs cannot answer it differently.
##
## `false` when either register is absent — a missing block cannot repeat anything, and the callers
## pin its presence separately.
static func deal_repeats_a_yields_number(root: Node) -> bool:
	var deal := improvement_deal_value(root)
	if deal == DEAL_ROW_ABSENT:
		return false
	var takes := magnitudes_in(yields_text(root))
	for magnitude in magnitudes_in(deal):
		if takes.has(magnitude):
			return true
	return false

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

## **THE KEEPING-ROW READERS ARE RETIRED** (`docs/plan_standing_upkeep.md` §2.5).
## `has_crew_row_maintain` / `crew_row_maintain_note` / `maintain_verdict_text` read a stepper and a
## verdict on a compose-sheet row that no longer exists: maintenance left the tile, so the keeping is
## a band-wide standing role and a sheet composes no keeping crew. What a source's keeping costs is
## stated by `DetailFormat.at_risk_lines`, which lands in the land card and the herd drawer — and
## only where it is going UNPAID, the standing bill having been retired with the `Keeping:` row
## (issue #545): a rung whose keeping is met says nothing, so a bare row IS the good news.

## The verdict's SENTENCE — the row's Labels joined (the severity dot is a Label of the row too, so it
## leads). Found by the same meta as the severity below, because the row's two halves are one claim and
## a needle search across the sheet would match whichever line happened to carry the same number. "" when
## no verdict rendered, which fails a `contains` assertion rather than satisfying it.
static func verdict_text(root: Node) -> String:
	var node := Q.find_meta_node(root, HudWidgets.VERDICT_META)
	return " ".join(face_lines(node)) if node != null else ""

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

## **THE BUILDERS STEPPER — the SECOND one on the sheet, which `stepper_value` above cannot reach.**
## That reader takes the first `−` it finds, which is the TAKE crew's, so a claim about the build
## crew's own value or its `+` needs a reader scoped to the BUILD CREW ROW
## (`HudWidgets.BUILD_CREW_ROW_META`, the row `_mount_build_crew_row` stamps).
##
## They exist because the two steppers now share ONE pool
## (`HudBandLaborState.source_crew_pool_forage` / `_hunt`): "dropping the take crew immediately raises
## the build's ceiling" is a claim about the OTHER stepper's `+`, and nothing here could see it.
const BUILD_CREW_VALUE_ABSENT := -1

static func build_crew_value(root: Node) -> int:
	var minus := _build_crew_minus(root)
	if minus == null:
		return BUILD_CREW_VALUE_ABSENT
	var siblings := minus.get_parent().get_children()
	var value: Node = siblings[siblings.find(minus) + 1]
	return int((value as Label).text) if value is Label else BUILD_CREW_VALUE_ABSENT

## Can the BUILDERS stepper still take a hand? `false` where the row is not rendered at all, which is
## the honest answer for a control offering no build to staff.
static func build_crew_can_add(root: Node) -> bool:
	var plus := build_crew_plus(root)
	return plus != null and not plus.disabled

## The BUILDERS stepper's `+` itself, so a state can PRESS it rather than write the model behind it —
## the press is what runs the ceiling clamp in the sheet's own handler.
static func build_crew_plus(root: Node) -> Button:
	var minus := _build_crew_minus(root)
	if minus == null:
		return null
	var siblings := minus.get_parent().get_children()
	var index := siblings.find(minus) + STEPPER_MINUS_TO_PLUS
	if index >= siblings.size():
		return null
	var plus: Node = siblings[index]
	return plus as Button if plus is Button else null

## The `−` and the `+` sit either side of the value Label (`HudWidgets.add_stepper_controls`).
const STEPPER_MINUS_TO_PLUS := 2

static func _build_crew_minus(root: Node) -> Button:
	var row := Q.find_meta_node(root, HudWidgets.BUILD_CREW_ROW_META)
	if row == null:
		return null
	var minus := Q.find_button_by_text(row, Spine.COMPOSE_STEPPER_MINUS_FACE)
	return minus if minus != null and minus.get_parent() != null else null

## How many characters of a detail card's BBCode `detail_excerpt` returns around the key it found —
## enough to carry the row's whole value cell into the run log, short enough not to swallow the rows
## either side of it (which is what would let an assertion match a neighbour's number).
const DETAIL_EXCERPT_CHARS := 96

## What `detail_excerpt` answers when the key is not on the card at all. A SENTINEL, not "", because
## an absent row and a row with an empty value are different findings and a `contains` assertion must
## fail loudly on the first rather than quietly on the second.
const DETAIL_EXCERPT_ABSENT := "<row absent>"

## A readable slice of a rendered detail card's BBCode around one row key — for the run log, so a
## failing assertion shows what the card actually SAID rather than only that it disagreed.
##
## It is also the only honest way to ASSERT on such a row: `detail_bbcode` splits a `Key: value` line
## into two colour spans, so the rendered source never contains the line contiguously, and the bare
## value is no better a needle (a `50` appears in any percentage two rows up). Excerpt from the KEY,
## then assert on what follows it.
static func detail_excerpt(bbcode: String, key: String) -> String:
	var at := bbcode.find(key)
	if at < 0:
		return DETAIL_EXCERPT_ABSENT
	return bbcode.substr(at, DETAIL_EXCERPT_CHARS)

## **THE PRE-LAUNCH FIGHT'S TWO LINES** (`docs/plan_hunt_through_combat.md` §2.1 / §6.5), each read by
## its OWN meta. Two readers rather than one, because the lines are composed from disjoint wire terms
## — `engageRate` against `hunterAttack` / `defense` / `durability` — so a single handle would let one
## regress while an assertion on the other went on passing.
##
## `""` when the line is absent, which is a REAL reading and half of what each is asserted on: a pen
## and the whole plant web must render neither, and a `contains` assertion fails on `""` rather than
## being satisfied by it.
static func hunt_gate_line(root: Node) -> String:
	var node := Q.find_meta_node(root, HudWidgets.HUNT_GATE_META)
	return (node as RichTextLabel).get_parsed_text() if node is RichTextLabel else ""

## **THE SPLIT PARTY'S LINE** (issue #520), by its OWN meta for the reason the pair above have theirs:
## it is composed from `huntCrews` where the gate is composed from `hunterAttack` against the herd's
## pair, so one handle would let either regress behind an assertion on the other. `""` when absent,
## which is the shipped reading for every uniformly-equipped band and half of what is asserted.
static func hunt_crew_split_line(root: Node) -> String:
	var node := Q.find_meta_node(root, HudWidgets.HUNT_CREW_SPLIT_META)
	return (node as RichTextLabel).get_parsed_text() if node is RichTextLabel else ""

## What `hunt_gate_blocked` answers when NO gate line rendered at all. A third state, not a `false`:
## "the sheet says the fight is winnable" and "the sheet says nothing about the fight" are different
## findings, and collapsing them would let a vanished line pass an is-not-blocked assertion.
const HUNT_GATE_ABSENT := -1

const HUNT_GATE_WINNABLE := 0

const HUNT_GATE_BLOCKED := 1

## **IS THE FIGHT UNWINNABLE, STRUCTURALLY?** — off the gate line's own meta value, never off its
## words. The refusal and the effort figure are ONE line in two states, so a text match would have to
## re-type the copy it is checking and would pass the moment either sentence was reworded.
static func hunt_gate_blocked(root: Node) -> int:
	var node := Q.find_meta_node(root, HudWidgets.HUNT_GATE_META)
	if node == null:
		return HUNT_GATE_ABSENT
	return HUNT_GATE_BLOCKED if bool((node as Control).get_meta(HudWidgets.HUNT_GATE_META, false)) \
		else HUNT_GATE_WINNABLE

## The index of the `Key: value` row with this key, or -1. Matches the key EXACTLY (up to the
## `DetailFormat` separator) so `Foraging` cannot be found by a row that merely mentions it.
static func detail_row_index(lines: Array[String], key: String) -> int:
	var prefix := key + DetailFormat.DETAIL_KV_SEPARATOR
	for index in lines.size():
		if lines[index].begins_with(prefix):
			return index
	return -1
