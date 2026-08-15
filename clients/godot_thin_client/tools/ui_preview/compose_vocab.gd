## The compose sheets' shared vertical grammar.
##
## Lifted out of `tools/ui_preview.gd` — pure, harness-free helpers, so that adding a state
## to one arc does not touch the same file as adding a state to another. See
## `.claude/rules/client/test-harnesses.md`.

## "no count dialed in" for `_compose_herd` — a real dial can be 0 (an unstaffed compose), so the
## sentinel has to sit outside the valid range rather than reuse 0.
const COMPOSE_COUNT_UNSET := -1
# The crowded hex's staffed-wildlife-row state: the SAME herd worked both ways at once. Two distinct
# counts so the row's meta can only read right if it SUMS them (4 + 6 = `10 🏹`) — a single shared
# number would pass even if one source were dropped.

const COMPOSE_SPINE_BAND := "band"

const COMPOSE_SPINE_POLICY := "policy"

const COMPOSE_SPINE_STEPPER := "stepper"

const COMPOSE_SPINE_IMPROVEMENT := "improvement"

## **BOTH THE `keeping` AND THE `builders` TAGS ARE RETIRED** (`docs/plan_standing_upkeep.md` §2.5).
## A source carried three worker allocations at the arc's widest and carries ONE now: maintenance left
## the tile for the `agriculture` / `husbandry` roles, and the build left it for the `builders` pool,
## a verb having stopped naming a crew at all. No sheet mounts either stepper, so no spine can carry
## either tag — and the claim that replaced them is `Readout.stepper_count`, which asserts there is no
## SECOND stepper whatever it might be tagged.

## **THE ROWS A SHEET RENDERS OR NOT ACCORDING TO ITS SOURCE, never according to its WEB.** Empty
## today: the keeping row was its only member and went with the per-source keeping crew. The parity
## check still drops this set before comparing, so a future source-conditional row has a home and the
## check stays a claim about ORDER.
const COMPOSE_SPINE_SOURCE_CONDITIONAL: Array[String] = []

## What EVERY compose sheet must open with — both webs, and the hunt sheet's local and expedition
## branches alike. The expedition branch builds no improvement control (a detached party builds
## nothing), so the shared claim is the HEAD; the two LOCAL sheets are additionally compared in full.
const COMPOSE_SPINE_HEAD: Array[String] = [
	COMPOSE_SPINE_BAND, COMPOSE_SPINE_POLICY, COMPOSE_SPINE_STEPPER,
]

## The three sheets whose spines this run captures, as `_compose_spines` keys. Named consts because the
## capture sites and the parity check sit ~1,600 lines apart, and a typo in either would silently
## compare a spine against nothing.
const COMPOSE_SPINE_KEY_FORAGE := "forage"

## The `−` face `HudWidgets.add_stepper_controls` gives every stepper's decrement button (U+2212, not a
## hyphen). It is the one structural handle on a stepper row — unlike a rung or an improvement box, a
## stepper carries no meta — so the walk below finds it by that face.
const COMPOSE_STEPPER_MINUS_FACE := "−"

## The open compose sheet's spine, in tree order. Each recognized control is tagged and NOT descended
## into: a rung's cell holds Labels, an improvement control holds its own rows, and neither is a spine
## control in its own right. A policy PICKER emits one tag however many rungs it holds.
static func compose_spine(root: Node) -> Array[String]:
	var spine: Array[String] = []
	collect_compose_spine(root, spine)
	return spine

static func collect_compose_spine(node: Node, spine: Array[String]) -> void:
	if node == null:
		return
	if node is Control and (node as Control).has_meta(HudWidgets.IMPROVEMENT_CONTROL_META):
		spine.append(COMPOSE_SPINE_IMPROVEMENT)
		return
	if node is OptionButton:
		spine.append(COMPOSE_SPINE_BAND)
		return
	if node is Button and (node as Button).has_meta(HudWidgets.POLICY_RUNG_META):
		if spine.is_empty() or spine[spine.size() - 1] != COMPOSE_SPINE_POLICY:
			spine.append(COMPOSE_SPINE_POLICY)
		return
	if node is Button and (node as Button).text == COMPOSE_STEPPER_MINUS_FACE:
		spine.append(COMPOSE_SPINE_STEPPER)
		return
	for child in node.get_children():
		collect_compose_spine(child, spine)

## A pixel of slack, so a row that lands exactly on the card's inner edge is not a failure. Anything
## that actually clips overruns by whole glyphs, never by a rounding remainder.
const COMPOSE_FIT_SLACK := 1.0
