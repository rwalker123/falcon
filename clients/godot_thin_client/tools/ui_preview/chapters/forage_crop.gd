extends RefCounted

## Cultivate, the crop picker and sowing.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const Spine := preload("res://tools/ui_preview/compose_vocab.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

## Does any Label under `root` contain this text? The gate-reason assertions' instrument: a reason that
## has been COLLAPSED into a tooltip is no longer any label's text, so this tells a spelled-out
## prerequisite from a one-line "locked (N requirements unmet)" summary.
## The text of the first Label under `root` containing `needle` — "" when there is none. Lets a frame
## assert on a value that must CHANGE (a rung's payoff face) rather than merely be present.
## Slack allowed when asserting a control sits INSIDE its card (`_rect_contains`): a control laid out
## flush against the card's inner edge can land a sub-pixel over it and is not what "clipped" means.
const CLIP_TOLERANCE_PX := 1.0

## `forage_sow_locked`'s two gate inputs, named for the same reason: the frame asserts the rendered
## SOURCE reason against `SOW_REFUSAL_REASONS[…]` and the ABSENCE of the knowledge reason at this
## exact percent, so the fixture and the expected strings are one value each rather than two that can
## drift apart. The refusal key is `BaseFx.food_tile_fixture`'s own (`TileFx.tended_tile_fixture` inherits it).
const SOW_LOCKED_REFUSAL_KEY := "too_dry"

const SOW_LOCKED_SEED_SELECTION := 0.12

## The Label NODE carrying `needle`, for the assertions that measure WHERE a row sits rather than
## what it says. A text lookup answers the TEXT; a clipping check needs the NODE, for its rect.
func _label_node_containing(root: Node, needle: String) -> Label:
	if root == null:
		return null
	if root is Label and (root as Label).text.contains(needle):
		return root as Label
	for child in root.get_children():
		var found := _label_node_containing(child, needle)
		if found != null:
			return found
	return null

## What the VISIBLE cards stacked below `card` reserve in its dock — the harness's read-only echo of
## `DockScrollFit._height_reserved_below`, so a sizing failure can be attributed to a term rather than
## guessed at. Read-only on purpose: it must not become a second implementation the real one drifts
## from, which is why it is only ever printed, never asserted on.
func _dock_height_reserved_below(card: Control) -> float:
	var stack := card.get_parent() as VBoxContainer
	if stack == null:
		return 0.0
	var separation := float(stack.get_theme_constant("separation"))
	var reserved := 0.0
	var below := false
	for child in stack.get_children():
		if child == card:
			below = true
			continue
		var sibling := child as Control
		if not below or sibling == null or not sibling.visible:
			continue
		reserved += sibling.get_combined_minimum_size().y + separation
	return reserved

## Is `control` fully inside `rect` (both global)? Null control = false — an assertion about where a
## row sits must FAIL when the row is missing entirely, never pass vacuously. A one-pixel tolerance,
## because a control flush against the card's inner edge is not clipped.
func _rect_contains(rect: Rect2, control: Control) -> bool:
	if control == null:
		return false
	var inner := control.get_global_rect()
	return inner.position.y >= rect.position.y - CLIP_TOLERANCE_PX \
		and inner.end.y <= rect.end.y + CLIP_TOLERANCE_PX

## THE SIZING CASE FOR A COMMITTED PATCH — `realized_species_max` is 4, so a 4-plant basket is the
## WORST CASE both surfaces must fit, not an outlier, and the 3-plant reference tile is one row short
## of reaching either cap. Taken from the playtest hex that broke them: Wild Emmer 47 / Flax Fields 21
## / Hay Grass 21 / Wild Grapevine 11, committed to the emmer with the build barely started, worked by
## a band (so the sheet also carries its `Now 1` line — the row that was clipped off the top).
## The mixed accounts are deliberate: a provisions crop, two cash crops and a fodder crop put every
## row shape in one frame at the length where the height is tightest — including the **two-material**
## one, since a tended patch here keeps both cash volunteers standing and honestly quotes the fibre
## AND the grape they pay (arc #527). That is the shape a summed "materials/turn" figure would erase,
## so it needs a frame.
func _four_species_committed_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["patch_committed_species"] = "wild_emmer"
	tile["patch_committed_display_name"] = "Wild Emmer"
	tile["patch_cultivation_progress"] = 0.04
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.47,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.40, "sow_yield_ratio": 4.20,
			"cultivate_payoff": 1.39, "sow_payoff": 2.40},
		{"species": "flax_fields", "role": "cash", "display_name": "Flax Fields", "share": 0.21,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0,
			# A sown Field is 100% flax and pays fibre alone; a TENDED patch keeps the grapevine
			# volunteers, so it quotes two materials — the row shape this fixture exists to carry.
			"cultivate_material_payoff": [
				{"material_id": "fibre", "amount": 1.42}, {"material_id": "grape", "amount": 0.31}],
			"sow_material_payoff": [{"material_id": "fibre", "amount": 3.51}]},
		{"species": "hay_grass", "role": "fodder", "display_name": "Hay Grass", "share": 0.21,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0, "sow_fodder_payoff": 15.6},
		{"species": "wild_grapevine", "role": "cash", "display_name": "Wild Grapevine", "share": 0.11,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0,
			"cultivate_material_payoff": [
				{"material_id": "grape", "amount": 0.74}, {"material_id": "fibre", "amount": 0.46}],
			"sow_material_payoff": [{"material_id": "grape", "amount": 3.75}]},
	]
	return tile

## The LONGEST basket the sim can produce — a navigable hex blends the valley's basket with the
## channel's fishery, so five named plants can share one tile (RollingHills carries four). The crop
## picker must fit and stay legible at that length, which is why the sizing case gets its own fixture
## rather than being judged on the 3-entry reference tile.
func _long_basket_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.34,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 4.20,
			"cultivate_payoff": 1.35, "sow_payoff": 2.10,
			"cultivate_material_payoff": [], "sow_material_payoff": []},
		{"species": "hazel", "role": "staple", "display_name": "Hazel", "share": 0.24,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.34, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.67, "sow_payoff": 0.0,
			"cultivate_material_payoff": [], "sow_material_payoff": []},
		{"species": "river_fish", "role": "staple", "display_name": "River Fish", "share": 0.18,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "ground_nut", "role": "staple", "display_name": "Ground Nut", "share": 0.14,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.90, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.45, "sow_payoff": 0.0,
			"cultivate_material_payoff": [], "sow_material_payoff": []},
		{"species": "oak_mast", "role": "staple", "display_name": "Oak Mast", "share": 0.10,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
	]
	return tile

## THE ALL-MARGINAL TILE — RollingHills' real numbers: every crop it can grow yields LESS than simply
## gathering the tile wild (Hazel 0.67, Wild Emmer 0.60, Wild Tubers 0.49, Berry Scrub 0.35). Nothing
## is illegal and nothing is disabled — the whole list is warn-inked and every row is still pressable,
## because "this land is not worth farming" is a verdict the player must be able to read AND overrule.
## SYNTHETIC — NOT A REAL TILE. Eight named plants, longer than any basket the sim can produce today
## (the longest real one is the 5-plant navigable-hex blend). Its ONLY job is to keep the crop picker's
## internal scroll RENDERED: the visible-row cap is set so every SHIPPED basket fits without scrolling,
## which would otherwise leave that path unexercised by any frame until F5 lengthens the roster and
## someone discovers it rotted. Do not treat these species or shares as a balance reference.
func _overlong_basket_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["terrain_label"] = "Rolling Hills"
	tile["patch_composition"] = [
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.22,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 4.20,
			"cultivate_payoff": 1.35, "sow_payoff": 2.10},
		{"species": "hazel", "role": "staple", "display_name": "Hazel", "share": 0.17,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 2.20, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 1.10, "sow_payoff": 0.0},
		{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.14,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.70, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.85, "sow_payoff": 0.0},
		{"species": "river_fish", "role": "staple", "display_name": "River Fish", "share": 0.13,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "ground_nut", "role": "staple", "display_name": "Ground Nut", "share": 0.11,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.44, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.72, "sow_payoff": 0.0},
		{"species": "berry_scrub", "role": "staple", "display_name": "Berry Scrub", "share": 0.09,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.90, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.45, "sow_payoff": 0.0},
		{"species": "oak_mast", "role": "staple", "display_name": "Oak Mast", "share": 0.08,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "marsh_reed", "role": "fodder", "display_name": "Marsh Reed", "share": 0.06,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.70, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.35, "sow_payoff": 0.0},
	]
	return tile

func _marginal_basket_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["terrain_label"] = "Rolling Hills"
	tile["patch_composition"] = [
		{"species": "hazel", "role": "staple", "display_name": "Hazel", "share": 0.34,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.94, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.47, "sow_payoff": 0.0},
		{"species": "wild_emmer", "role": "staple", "display_name": "Wild Emmer", "share": 0.28,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.84, "sow_yield_ratio": 1.26,
			"cultivate_payoff": 0.42, "sow_payoff": 0.63},
		{"species": "wild_tubers", "role": "staple", "display_name": "Wild Tubers", "share": 0.22,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.68, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.34, "sow_payoff": 0.0},
		{"species": "berry_scrub", "role": "staple", "display_name": "Berry Scrub", "share": 0.16,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.49, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.25, "sow_payoff": 0.0},
	]
	return tile

## The same long basket on ground that will actually take seed — `Sow` is gated on the site, so the
## crop picker's rung-3 frame needs the sowable tile, not the reference one (a refused Sow falls back
## to Sustain and the picker would not render at all).
func _sowable_long_basket_tile_fixture() -> Dictionary:
	var tile := ForageFx.sowable_tile_fixture()
	tile["patch_composition"] = _long_basket_tile_fixture()["patch_composition"]
	return tile

## The OTHER refusal. `BaseFx.food_tile_fixture` is "too_dry" (rich prairie away from water); this is thin
## upland ground — watered, but too poor to take a crop without fertilizing. The two messages must
## differ, name different faults, and each point at the rung that lifts it.
func _sow_too_poor_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	# In range of the reference band, like `ForageFx.sowable_tile_fixture` — the refusal must be the ONLY
	# reason Sow is unavailable in this frame.
	tile["x"] = 65
	tile["y"] = 11
	tile["terrain_label"] = "Montane Highland"
	tile["tags_text"] = "Thin Soil, Fresh Water"
	tile["food_module"] = "montane_highland"
	tile["food_module_label"] = "Montane Highland"
	tile["site_name"] = ""
	tile["patch_sow_site_refusal"] = "too_poor"
	return tile

func run(harness) -> void:
	h = harness

	# ---- Cultivate: the forage INVESTMENT rung (gated, then unlocked) ----------------------------
	# State 2-cultivate-locked — **THE KNOWLEDGE-SUPPRESSION RULE'S OWN FRAME, on the plant web.** The
	# faction has NOT finished learning Cultivation (the top-bar meter reads "Cultivation ▰▰▰…
	# learning") and the patch is Thriving and wild, so KNOWLEDGE is the only thing blocking the rung —
	# and this sheet renders no improvement control at all for that.
	#
	# **THE FRAME'S SUBJECT MOVED WITH THE RULE, and it is a progression rather than a hole.** It used
	# to be the gated control's reason line ("🌱 Your people know Cultivation 55% — ♻ forage a wild
	# patch to learn it"). That sentence was both redundant and vacuous HERE: the aside two rows up
	# states the same lesson live and quantified, and its remedy — forage a wild patch — names the very
	# work this sheet is composing, so it told the player to do what they were in the middle of doing.
	# What the frame shows now is the pair that has to hold TOGETHER: nothing is offered that the sim
	# would refuse, and the aside is still naming the lesson being earned. A SOURCE gate is untouched
	# and still leads a control — `improvement_offered_gated` and `forage_sow_locked` are those frames.
	h._hud._compose.set_forage_count(1)
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("forage_cultivate_locked")
	# **ASKED OF THE WHOLE CONTROL FAMILY, not of the Cultivate rung.** `ForageFx.find_improvement_control`
	# answers null for a rung merely spelled differently, so a per-rung form of this passes on a sheet
	# that renders some OTHER rung's control; `IMPROVEMENT_CONTROL_META` rides all four states the
	# widget can be in, so this says "no improvement control, of any rung, in any state".
	h._assert_hud("a rung blocked ONLY on knowledge renders NO improvement control on this sheet",
		Q.find_meta_node(h._hud._drawercompose._compose_sheet,
			HudWidgets.IMPROVEMENT_CONTROL_META) == null)
	# The visible symptom of getting this wrong, and why it is asserted separately: dropping the reason
	# WITHOUT suppressing the control leaves an unchecked, live box over a live crop list — the sheet
	# inviting a commitment the sim rejects, which is strictly worse than the line that was cut.
	h._assert_hud("…and no crop list beneath it, the sheet offering nothing it cannot commit",
		ForageFx.find_crop_row(h._hud._drawercompose._compose_sheet, ForageFx.GATED_CROP_NEEDLE) == null)
	# **THIS IS WHAT MAKES THE REMOVAL A PROGRESSION.** The rung is not merely hidden: the aside names
	# the very craft whose absence suppressed the control, live, in the same frame. Read BY META — the
	# aside's siblings move with the floor too, so a whole-aside search says nothing about this line.
	h._assert_hud("…while the aside still names the lesson being earned, so the rung is not silent",
		Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(
			String(SourceForecast.RUNG_LESSONS[SourceForecast.SOURCE_KIND_FORAGE][
				SourceForecast.IMPROVEMENT_NONE])))

	# Learning Cultivation crosses 0.55 → 1.0 between snapshots: the one-shot command-feed nudge fires
	# ("Cultivation learned — The Cultivate policy is now available on Thriving patches."), visible in
	# the left-dock Command Feed card in every frame from here on.
	h._hud.update_intensification([{"faction": 0, "cultivation": 1.0, "herding": 1.0}])

	# State 2-cultivate — knowledge known + a Thriving patch: 🌱 Cultivate is ENABLED and selected. The
	# forecast states the DEAL instead of a single number — "Preparing: +0.24 /turn → then +1.20 /turn"
	# (ceiling_cultivate → tended_yield) — and the stepper caps at 1 worker (a managed source needs one).
	h._show_tile(BaseFx.food_tile_fixture())
	h._hud._compose.set_forage_improvement("cultivate")
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("forage_cultivate")
	# THE FORAGE HALF of the compose-order invariant (see `Spine.compose_spine`): capture this sheet's control
	# spine, to be compared against the local-hunt sheet's when that renders further down.
	#
	# **CAPTURED HERE AND NOT ON `food_tile`, WHERE IT USED TO BE — the spine must be taken where the
	# sheet carries every control it can carry.** `food_tile` renders at Cultivation 55%, i.e. a rung
	# blocked on KNOWLEDGE ALONE, and this sheet now builds no improvement control for that; comparing
	# that three-control spine against the local hunt's four would fail an ORDER assertion for a reason
	# that has nothing to do with order. This state is the same sheet one snapshot later, with the
	# knowledge complete and the rung composed — so both spines are full, and the equality is a real
	# claim about sequence again.
	h._record_compose_spine(Spine.COMPOSE_SPINE_KEY_FORAGE)

	# State 2-crop-picker — THE CROP PICKER (flora roster S1), on the longest basket the sim produces
	# (5 named plants). Under 🌱 Cultivate the selection must land on the HIGHEST-SHARE LEGAL row —
	# Wild Emmer 34%, which is also the sim's own default — while River Fish and Oak Mast stay VISIBLE
	# and greyed (they climb no rung), and Ground Nut 14% stays fully pressable: a small share is a bad
	# choice, not an illegal one. Judge legibility + fit here, not on the 3-entry reference tile.
	h._show_tile(_long_basket_tile_fixture())
	h._hud._compose.set_forage_improvement("cultivate")
	h._hud._compose.set_forage_species("")
	h._compose_forage(_long_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_picker")

	# THE COMMIT BUTTON MUST STAY ON SCREEN. A picker that pushes `Forage` below the sheet's fold is
	# worse than the problem the picker solved, so the picker's list scrolls WITHIN itself
	# (FLORA_CROP_LIST_MAX_HEIGHT) rather than growing the sheet. Asserted on the LONGEST basket, the
	# only case that can trip it: the sheet's own ScrollContainer must have nothing left to scroll.
	var sheet_scroll: ScrollContainer = h._hud._drawercompose._compose_sheet._scroll
	var sheet_overflow: float = sheet_scroll.get_v_scroll_bar().max_value - sheet_scroll.size.y
	print("ui_preview: compose sheet overflow = %.1f (card %.1f)" % [
		sheet_overflow, h._hud._drawercompose._compose_sheet._card.size.y])
	h._assert_hud("a 5-plant crop picker leaves the Forage button on screen (sheet does not scroll)",
		sheet_overflow <= 1.0)
	# The height that bought those rows used to come from COLLAPSING the other rung's gate reasons; the
	# improvement control bought it outright (issue #442) by offering ONE rung instead of six, so no
	# other rung's prerequisites are on the card to collapse in the first place. That is the claim now.
	#
	# ASKED OF THE CONTROLS, not of the sheet's text. This searched the whole sheet for the words "Seed
	# Selection" — which passes if the sheet failed to open at all, and would go on passing if a second
	# rung rendered wearing any other reason. The claim is about how many improvement CONTROLS there
	# are, so it counts them: the composed Cultivate, and nothing for the rung above it.
	h._assert_hud("only ONE improvement is offered, so no second rung's prerequisites crowd the card",
		ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CULTIVATE) != null
		and ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_SOW) == null)
	# **THE HEADER IS THE RUNG'S, and this is the CULTIVATE half of the pair.** Cultivate only weeds
	# the favored share upward and leaves the rest of the basket standing, so "commit" overstates what
	# the rung does — the word is true of Sow alone, and `forage_crop_picker_sow` is where it is
	# asserted. Either frame alone would pass a header hard-wired to the string it happens to expect.
	h._assert_hud("the Cultivate picker asks which crop to TEND, not which to commit to",
		Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			HudFloraVocab.FLORA_CROP_TEND_HEADER.to_upper()))

	# State 2-crop-then-a / -b — THE PICKER ACTUALLY MOVES THE PAYOFF. The payoff term used to quote
	# a species-BLIND patch number, so committing to Ground Nut showed Wild Emmer's payoff and the picker
	# appeared to change nothing above it. These two frames are the SAME tile with a DIFFERENT crop
	# selected; the assertion is that the payoff differs between them, which is the only thing
	# that proves the substitution is wired to the selection rather than rendered once.
	#
	# **READ OFF THE READOUT'S DEAL BLOCK, by meta.** The payoff has moved twice — a deal line beneath
	# the box, then the box's own face, and now the PER TURN readout — and this pair has to keep
	# proving the substitution wherever it lands, so it follows the number rather than the widget.
	# The FACE's absence is asserted beside it: the pair would otherwise pass on a sheet quoting the
	# crop in two places, which is exactly the two-numbers-one-question defect being removed.
	h._hud._compose.set_forage_count(1)
	h._hud._compose.set_forage_species("wild_emmer")
	h._compose_forage(_long_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_then_emmer")
	var then_emmer = Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet)
	var emmer_face = ForageFx.improvement_face(
		h._hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)

	h._hud._compose.set_forage_species("ground_nut")
	h._compose_forage(_long_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_then_groundnut")
	var then_groundnut = Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet)
	print("ui_preview: deal-block  emmer=%s  ground_nut=%s" % [then_emmer, then_groundnut])
	var payoff_key = String(HudComposeVocab.IMPROVEMENT_PAYOFF_ROW_LABELS[
		SourceForecast.IMPROVEMENT_CULTIVATE]).to_upper()
	h._assert_hud("the readout's payoff row tracks the SELECTED crop",
		then_emmer.contains(payoff_key) and then_groundnut.contains(payoff_key)
			and then_emmer != then_groundnut)
	h._assert_hud("…and neither crop's payoff is restated on the box's face",
		not emmer_face.contains(ForageFx.IMPROVEMENT_PAYOFF_NEEDLE)
			and not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
				SourceForecast.IMPROVEMENT_CULTIVATE).contains(ForageFx.IMPROVEMENT_PAYOFF_NEEDLE))
	h._hud._compose.set_forage_species("")

	# State 2-crop-marginal — the ALL-MARGINAL tile (RollingHills' real ratios). Every legal crop is
	# below 1.0×, so the whole list is warn-inked and the hint says why — and every row stays PRESSABLE.
	# The ratio is here to stop a bad idea being invisible, never to forbid it.
	h._show_tile(_marginal_basket_tile_fixture())
	h._hud._compose.set_forage_improvement("cultivate")
	h._hud._compose.set_forage_species("")
	h._compose_forage(_marginal_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_marginal")

	# State 2-crop-overlong — THE SCROLL'S OWN FRAME. A SYNTHETIC 8-plant basket, longer than any tile
	# the sim can produce, so the picker's internal list actually scrolls: the visible-row cap is set so
	# every SHIPPED basket fits whole, which would otherwise leave this path rendered by nothing. The
	# `Forage` button must still be on screen — that is what the cap protects, at any basket length.
	h._show_tile(_overlong_basket_tile_fixture())
	h._hud._compose.set_forage_improvement("cultivate")
	h._hud._compose.set_forage_species("")
	h._compose_forage(_overlong_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_picker_overlong")
	var overlong_scroll: ScrollContainer = h._hud._drawercompose._compose_sheet._scroll
	print("ui_preview: overlong-basket sheet overflow = %.1f (card %.1f)" % [
		overlong_scroll.get_v_scroll_bar().max_value - overlong_scroll.size.y,
		h._hud._drawercompose._compose_sheet._card.size.y])
	h._assert_hud("an 8-plant crop picker still leaves the Forage button on screen",
		overlong_scroll.get_v_scroll_bar().max_value - overlong_scroll.size.y <= 1.0)

	# ---- THE TWO ZERO-WORKER SUBMITS (playtest defect) -------------------------------------------
	# `workers == 0` means two different things depending on whether this band already works the tile,
	# and the button + the forecast line have to agree in BOTH. These frames are judged as a PAIR.
	#
	# State 2-unstaffed (A) — 0 foragers on a tile this band does NOT work. Pressing Forage would send a
	# command that changes nothing, so the button is DISABLED and still reads `Forage`. The payoff
	# NUMBER stays on the running box's face — it is how the player decides the tile is worth staffing
	# at all — and there is no longer a SEQUENCE beside it to be wrong about at zero crew: the deal
	# line's today/dip terms are what a zero crew made unreachable, and only the payoff survived it.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._compose.reset_forage_source()
	h._show_tile(BaseFx.food_tile_fixture())
	# The FIRST compose settles the source key; the policy and count must be set after it, because a
	# source change re-seeds both from the band's standing assignment and would overwrite them.
	h._compose_forage(BaseFx.food_tile_fixture())
	h._hud._compose.set_forage_improvement("cultivate")
	h._hud._compose.set_forage_count(0)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("forage_unstaffed")
	# By META — the commit verb follows the patch's rung now, and a bare "Forage" literal here would
	# be a second, silent spelling of that rule.
	var unstaffed_btn = Q.compose_commit_button(h._hud._drawercompose._compose_sheet)
	h._assert_hud("0 workers on an unassigned tile disables the submit (it would be a no-op)",
		unstaffed_btn != null and unstaffed_btn.disabled)
	# **THE PAYOFF'S TWO HALVES, ASSERTED AS A PAIR.** Absence from the face alone is vacuous —
	# deleting the payoff outright would satisfy it — so the same frame asserts it reads in the PER
	# TURN readout, which is how the player decides the tile is worth staffing at all.
	h._assert_hud("no payoff is restated on the improvement box's face",
		not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_CULTIVATE)
			.contains(ForageFx.IMPROVEMENT_PAYOFF_NEEDLE))
	h._assert_hud("…while what the tile would pay once prepared reads in the readout's payoff row",
		Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet).contains(
			String(HudComposeVocab.IMPROVEMENT_PAYOFF_ROW_LABELS[
				SourceForecast.IMPROVEMENT_CULTIVATE]).to_upper()))
	# The block is that ONE row at every crew — see `improvement_running_plant` for why the count is
	# what pins the retired baseline rather than a `contains`.
	h._assert_hud("…as the block's ONLY row, at a crew of zero as at any other",
		Readout.improvement_deal_rows(h._hud._drawercompose._compose_sheet) == 1)
	# **THE BUILDING CAPTION AT A CREW THAT REACHES NOTHING — two reasons for one answer.** A composed
	# build suppresses the floor walk outright, and a zero crew reaches no holding rate either, so a
	# caption composing the arrow's key unconditionally fails here whichever of the two it read.
	# `improvement_running_plant` is where the SUPPRESSION alone is pinned, against a crew that does
	# reach its floor.
	h._assert_hud("…under a caption keying the dip alone, this crew reaching no holding rate",
		Readout.yields_header(h._hud._drawercompose._compose_sheet)
			== SourceForecast.YIELD_ROW_HEADER_WHILE_BUILDING.to_upper())
	# **A CREW OF ZERO IS BUILDING NOTHING, AND THE ASIDE MAY NOT SAY OTHERWISE.** `learn_multiplier`
	# is a function of the FLOOR alone, so at the food peak it reads ×1.00 no matter who is assigned —
	# and this frame has a composed Cultivate with NOBODY on it. The build half is gated on the same
	# work predicate the lesson is, which is a fact about the sim rather than a display nicety: build
	# accrual and knowledge accrual share one multiplier and one `crew_is_working_the_source` gate.
	# Asserted on this frame because it is the only one that pairs a live build with an empty crew.
	h._assert_hud("an unstaffed build claims no build rate — nobody is building it",
		not Readout.teaching_line(h._hud._drawercompose._compose_sheet).to_lower().contains("building at"))

	# State 2-unassign (B) — the SAME 0 workers on a tile this band DOES work: that is the sim's
	# unassign, not a no-op. The button stays live and is RENAMED, and the "assign to begin" line is
	# gone — it would contradict the button. What abandoning costs is already on the card in the
	# Cultivate policy hint ("It must stay staffed or it goes feral").
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture()
	h._hud._compose.reset_forage_source()
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	h._hud._compose.set_forage_improvement("cultivate")
	h._hud._compose.set_forage_count(0)
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("forage_unassign")
	var unassign_btn = Q.find_button_by_text(h._hud._drawercompose._compose_sheet, "Unassign")
	h._assert_hud("0 workers on a tile this band works stays live, renamed Unassign",
		unassign_btn != null and not unassign_btn.disabled)
	# …and the improvement control is SUPPRESSED here, which is the other half of the same judgement:
	# offering to START a build in the act of abandoning the source says two opposite things at once.
	# Asked of the whole control family, so a rung merely spelled differently cannot satisfy it.
	h._assert_hud("…and offers no rung to start while it is handing the source back",
		Q.find_meta_node(h._hud._drawercompose._compose_sheet,
			HudWidgets.IMPROVEMENT_CONTROL_META) == null)

	# Restore the unassigned near band for the frames that follow.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_count(1)

	# State 2-crop-committed — the patch has already committed. The commitment is one-way until it
	# lapses, so the picker becomes a LOCKED READOUT — but a readout OF THE WHOLE BASKET, with the
	# committed row marked, not a lone crop name: a bare name beside a tile card listing three plants
	# had the two panels of one tile disagreeing about what grows there, and read as "this tile is Wild
	# Grain now" (issue #433 deleted exactly that belief). The double-open is the same one the two states above spell out — the
	# `reset_forage_source()` just above makes the first open a SOURCE CHANGE, which re-seeds the policy
	# from the band's standing assignment (Sustain) and threw the `cultivate` away. This state had been
	# silently rendering the Sustain sheet, which carries no crop block at all, so the readout it exists
	# to judge was not in the frame.
	#
	# IT IS ALSO THE SIZING CASE FOR BOTH SURFACES, which is why it runs on the 4-species basket and on
	# a band that WORKS the tile: `realized_species_max` is 4, and the 3-species tile is one row short
	# of the caps the committed block used to blow — the compose sheet's fixed 560px ceiling and the
	# drawer's cap against the dock. A committed patch's block went from ONE line to FOUR rows, so both
	# surfaces gained ~66px at once, and neither fixture in the shipped set could reach them.
	h._hud._band_labor._player_band = BandFx.cultivating_forage_band_fixture()
	# **THE STOCKPILE PUSH THAT USED TO SIT HERE IS GONE, AND WITH IT A LAYOUT TERM** (issue #381).
	# The left-dock Stockpiles card sat below the tile card and was hidden until a faction carried
	# stock, so seeding stock here was what put a reserved sibling into `DockScrollFit`'s measurement.
	# That card is retired, and the band-scoped Trade row that replaced it went with the trade account
	# itself (arc #527), so nothing in the HUD reads `faction_inventory` and `HudLayer.update_stockpiles`
	# no longer exists. The drawer's cap is now measured against a left dock holding the tile card and
	# the default-hidden command feed — which IS the layout the player has, and a slightly roomier one
	# than this state was originally tuned against.
	# WHAT THE COMMITTED BLOCK COSTS THE DRAWER, measured rather than reasoned about: the SAME tile
	# with the commitment stripped, so the printed pair is the before/after of one change on one
	# layout. Both surfaces grew at once, and a sizing claim about either is worth only its number.
	var uncommitted_twin := _four_species_committed_tile_fixture()
	uncommitted_twin.erase("patch_committed_species")
	uncommitted_twin.erase("patch_committed_display_name")
	h._show_tile(uncommitted_twin)
	await h._settle()
	print("ui_preview: uncommitted drawer body=%.1f" % h._hud.subject_body.get_combined_minimum_size().y)
	# …and what it cost against the OLD render, which showed the `Crop:` row INSTEAD of the basket. A
	# basket-less committed patch cannot occur (the sim only commits to a member of the basket), so
	# this is a measurement fixture and never a saved frame — it exists to put a number on the growth.
	var old_render_twin := _four_species_committed_tile_fixture()
	old_render_twin.erase("patch_composition")
	h._show_tile(old_render_twin)
	await h._settle()
	print("ui_preview: pre-change (crop row, no basket) drawer body=%.1f"
		% h._hud.subject_body.get_combined_minimum_size().y)
	h._show_tile(_four_species_committed_tile_fixture())
	h._compose_forage(_four_species_committed_tile_fixture())
	h._hud._compose.set_forage_improvement("cultivate")
	h._compose_forage(_four_species_committed_tile_fixture())
	await h._settle()
	await h._save("forage_crop_committed")
	# `alloc_section_label` upper-cases its text, so the header is matched in the case it RENDERS in.
	h._assert_hud("a committed patch's picker is a locked readout under the committed-crop header",
		Q.has_label_containing(h._hud._drawercompose._compose_sheet,
				HudFloraVocab.FLORA_CROP_COMMITTED_HEADER.to_upper())
			and Q.has_label_containing(h._hud._drawercompose._compose_sheet,
				HudFloraVocab.FLORA_CROP_COMMITTED_HINT))
	# THE BUG THIS STATE NOW GUARDS: the readout lists the WHOLE basket, in the tile card's own order.
	# Asserting the committed name alone passed while the other two plants were being suppressed.
	var committed_rows: Array[Button] = []
	for basket_crop in ["Wild Emmer", "Flax Fields", "Hay Grass", "Wild Grapevine"]:
		committed_rows.append(ForageFx.find_crop_row(h._hud._drawercompose._compose_sheet, basket_crop))
	h._assert_hud("…listing every plant in the basket, not just the committed one",
		not committed_rows.has(null))
	var committed_all_locked := true
	for row in committed_rows:
		committed_all_locked = committed_all_locked and row != null and row.disabled
	h._assert_hud("…with every row locked (the commitment is one-way until it lapses)", committed_all_locked)
	# `Readout.rung_is_selected` reads the `normal` stylebox's fill, which `apply_button` writes from the
	# VARIANT — the one mark of selection that survives the disabled treatment, which is the whole
	# reason `selected_when_disabled` is passed here. Written for policy rungs, true of any
	# `apply_button`-styled button, and the only reading that can tell marked-and-locked from locked.
	h._assert_hud("…and the committed crop marked as the standing choice",
		committed_rows[0] != null and Readout.rung_is_selected(committed_rows[0]))
	h._assert_hud("…while the rest of the basket is not",
		committed_rows[1] != null and not Readout.rung_is_selected(committed_rows[1]))
	# ---- BOTH SURFACES MUST FIT THE 4-ROW BLOCK -------------------------------------------------
	# Printed as well as asserted: when one of these fails, the numbers say WHICH ceiling bit (the
	# sheet's own cap, the viewport, or the dock's remaining room), which a bare false cannot.
	var committed_sheet: ComposeSheet = h._hud._drawercompose._compose_sheet
	var committed_sheet_scroll: ScrollContainer = committed_sheet._scroll
	var committed_sheet_overflow: float = committed_sheet_scroll.get_v_scroll_bar().max_value \
		- committed_sheet_scroll.size.y
	print("ui_preview: committed sheet card=%.1f body=%.1f overflow=%.1f viewport=%.1f" % [
		committed_sheet._card.size.y, committed_sheet._body.get_combined_minimum_size().y,
		committed_sheet_overflow, h.get_viewport().get_visible_rect().size.y])
	h._assert_hud("a 4-species committed block does not make the compose sheet scroll internally",
		committed_sheet_overflow <= 1.0)
	# Clipping is the SYMPTOM the player reported (the top of the sheet off the card, the Forage button
	# sliced), and a scroll-extent check alone would not see a control sitting outside the card, so
	# the two ends of the sheet are measured against the card's own rect. The TOP end is the `Band:`
	# field key — the first control every compose sheet opens with, since the standing-crew line that
	# used to lead was retired.
	var committed_first_row := _label_node_containing(committed_sheet, HudWorkVocab.BAND_PICKER_LABEL)
	h._assert_hud("…and the band picker the sheet opens with is inside the card",
		_rect_contains(committed_sheet._card.get_global_rect(), committed_first_row))
	h._assert_hud("…and so is the Forage button it ends with",
		_rect_contains(committed_sheet._card.get_global_rect(),
			Q.compose_commit_button(committed_sheet)))
	# The TILE CARD's drawer is the other surface the same 4 rows pushed past its cap. Internal
	# scrolling is BY DESIGN here (a crowded hex must scroll inside the drawer rather than drag the
	# dock), so the assertion is not "never scrolls" — it is that THIS content, which fits the room
	# the dock has left, is not being capped short of it.
	var drawer_scroll: ScrollContainer = h._hud.subject_scroll
	var drawer_overflow: float = drawer_scroll.get_v_scroll_bar().max_value - drawer_scroll.size.y
	# The same three terms `DockScrollFit.fit_height` caps against, printed so a failure says WHICH
	# ran out — the dock's height, the rows above the drawer, or the cards reserved below it.
	var drawer_top_in_dock: float = drawer_scroll.global_position.y - h._hud.left_dock_scroll.global_position.y
	var drawer_reserved_below = _dock_height_reserved_below(h._hud.tile_panel)
	print("ui_preview: committed drawer cap=%.1f body=%.1f overflow=%.1f dock=%.1f top=%.1f reserved=%.1f available=%.1f" % [
		drawer_scroll.custom_minimum_size.y, h._hud.subject_body.get_combined_minimum_size().y,
		drawer_overflow, h._hud.left_dock_scroll.size.y, drawer_top_in_dock, drawer_reserved_below,
		h._hud.left_dock_scroll.size.y - drawer_top_in_dock - drawer_reserved_below])
	h._assert_hud("…nor does it make the tile card's drawer scroll, with dock room to spare",
		drawer_overflow <= 1.0)
	# Restore the unstaffed near band + the 3-species tile for the frames that follow.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_count(1)

	h._hud._compose.set_forage_improvement("cultivate")
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()

	# State 2-cultivate-stressed — knowledge known, but the patch is ⚠ Stressed: Cultivate stays visible
	# and greyed with the OTHER reason — "Patch is Stressed — ease workers off and let it regrow to
	# Thriving" (the ecology gate, not the knowledge one). The remedy is deliberately NOT "Sustain it":
	# a fully staffed Sustain takes the whole regrowth and holds a Stressed patch Stressed forever.
	h._show_tile(TileFx.stressed_tile_fixture())
	h._compose_forage(TileFx.stressed_tile_fixture())
	await h._settle()
	await h._save("forage_cultivate_stressed")

	# ---- Sow + the Field: plant RUNG 3 (slice 6b) -------------------------------------------------
	# State 6b-sow-locked — Seed Selection is only 12% learned AND this ordinary prairie refuses seed,
	# so BOTH kinds of reason are live at once: one fixed by PRACTICE (work a Tended Patch), one only
	# by MOVING somewhere else. No other rung on either ladder has the latter.
	#
	# **THAT PAIR IS WHY THIS FRAME PINS THE SUPPRESSION RULE, not merely the survival of it** — and it
	# is the frame that was asserting the DEFECT. The sheet used to delete the knowledge reason
	# unconditionally and render the source one alone, on the premise that the aside states that lesson
	# live two rows up. Reported from play: a lone reason reads as THE reason, so a tended patch at
	# Seed Selection 77% on dry ground claimed the knowledge was in hand and the water was all that
	# stood in the way — the message for a player who HAS Seed Selection. The premise is conditional
	# too: the aside names the lesson only while the crew is actually working the source, and on that
	# frame it read "Teaching nothing".
	#
	# The knowledge reason is now dropped ONLY when it is the sole one. Here BOTH render: the knowledge
	# reason leads (the near-term one a player can move), the ground's refusal keeps the note slot
	# beneath. They are different decisions — *you do not know how yet* means wait, *this ground will
	# never take seed* means move on.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0,
		"seed_selection": SOW_LOCKED_SEED_SELECTION, "penning": 0.0,
	}])
	# **THE TILE HAS TO BE A TENDED ONE NOW** (issue #442). Only ONE improvement is ever offered — the
	# source's next rung — so on a WILD patch with Cultivation known, Cultivate is what the control
	# offers and Sow is not reached at all. A tended patch has its rung-2 built, which makes Sow the
	# next rung and puts this frame's subject back on screen. That is the change working, not a loss:
	# the old picker showed all six rungs at once and had to grey four of them to say so.
	h._hud._compose.set_forage_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.set_forage_improvement("")
	h._hud._compose.reset_forage_source()
	h._show_tile(TileFx.tended_tile_fixture())
	h._compose_forage(TileFx.tended_tile_fixture())
	await h._settle()
	await h._save("forage_sow_locked")
	# A rung blocked on the SOURCE still TEACHES IN FULL — the reason is the control's own text, which
	# is the whole point of showing a gated improvement rather than hiding it.
	var sow_box = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "sow")
	h._assert_hud("a SOURCE-gated improvement is SHOWN, never hidden — the rung stays discoverable",
		sow_box != null and not (sow_box is CheckBox))
	var sow_knowledge_reason := HudFloraVocab.GATE_REASON_SEED_SELECTION_KNOWLEDGE_FORMAT % [
		HudFormat.progress_percent(SOW_LOCKED_SEED_SELECTION),
		FoodIcons.for_floor_zone(SourceForecast.FLOOR_ZONE_PEAK)]
	h._assert_hud("…with the KNOWLEDGE prerequisite leading as the control's OWN text — the one a player can move",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, SourceForecast.IMPROVEMENT_SOW)
			== HudComposeVocab.IMPROVEMENT_GATED_FORMAT % [
				FoodIcons.for_policy(SourceForecast.IMPROVEMENT_SOW), sow_knowledge_reason])
	# **AND THE SOURCE GATE SURVIVES BENEATH IT — asserted as the PAIR, because either alone is the
	# bug.** Only the lead line would mean the ground's permanent refusal had been swallowed; only the
	# presence of the knowledge reason somewhere would be satisfied by the old lead-with-the-source
	# rendering. Asked of the whole sheet, since a reason "renders" wherever it lands.
	h._assert_hud("…and the ground's own refusal still renders beneath it, not swallowed by the lead",
		Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			String(HudFloraVocab.SOW_REFUSAL_REASONS[SOW_LOCKED_REFUSAL_KEY])))
	# …and the rung BELOW it reads as the state it left behind, not as a second greyed option.
	h._assert_hud("…above a DONE label for the rung already built",
		ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet, "cultivate") is Label)

	# Seed Selection completes → the one-shot feed nudge fires ("Seed Selection learned — The Sow
	# policy is now available — but only on rich, well-watered ground.").
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0,
	}])

	# State 6b-sow-too-dry — knowledge KNOWN, and still refused: this prairie is rich but dry. THE
	# WHOLE POINT of the sim shipping a reason rather than a bool — only ~46 of 4160 tiles (1.1%) will
	# take seed, so "why can't I sow here?" is *the* question rung 3 provokes, and the client cannot
	# re-derive the answer (it has neither the biome capacity table nor the hydrology). The line must
	# name the fault (dry), not just refuse, and point at the rung that lifts it.
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("forage_sow_too_dry")

	# State 6b-sow-too-poor — the OTHER refusal, and the reason this pair is rendered together: thin
	# upland ground that IS watered. A different fault must produce a different sentence and a
	# different remedy — if these two frames read the same, the reason field is being wasted.
	h._show_tile(_sow_too_poor_tile_fixture())
	h._compose_forage(_sow_too_poor_tile_fixture())
	await h._settle()
	await h._save("forage_sow_too_poor")

	# State 6b-sow — QUALIFYING ground at last (alluvial plain beside fresh water — one of the 46).
	# ▦ Sow is ENABLED and selected, with NO refusal line. The forecast states a deal that is
	# deliberately shaped unlike Cultivate's: "Preparing: +0.02 /turn → then +2.40 /turn" — near-zero
	# while the crop is in the ground (pure investment; there is no standing stand to take a fraction
	# of), then 2× a tended patch. That asymmetry IS rung 3's bargain.
	# **THE THREE-LINE IDIOM, because this frame's own subject is a SELECTED Sow.** The first compose
	# settles the source key — a source change re-seeds the improvement from the band's standing
	# assignment — so setting the rung before it left the box UNCHECKED for as long as this state has
	# existed, quietly contradicting the sentence above. Composing it is what puts the rung's ONE deal
	# row — its `ONCE SOWN` payoff — into the readout, and the bargain's asymmetry is then read across
	# two REGISTERS of that box rather than across two rows of the deal: the dipped headline take
	# against that payoff, which is exactly what the assertion below compares.
	h._show_tile(ForageFx.sowable_tile_fixture())
	h._compose_forage(ForageFx.sowable_tile_fixture())
	h._hud._compose.set_forage_improvement("sow")
	h._compose_forage(ForageFx.sowable_tile_fixture())
	await h._settle()
	await h._save("forage_sow")
	# **THE SOW RUNG'S HALF OF THE PAYOFF CLAIM, and the rung the readout rows were unproven on.** No
	# sowable fixture carried a build fraction until this pass — `seed_forage_rows` ignores the
	# `patch_ceiling_sow` shorthand on a re-seed, so `improvement_forecast` answered `{}` and Sow
	# quoted no deal on any frame in the corpus. The pair is asserted in the same shape the other
	# rungs take: nothing on the face, the terms in the readout under this rung's own key.
	h._assert_hud("a composed Sow states its payoff under the ONCE SOWN key, not on its box",
		Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet).contains(
			String(HudComposeVocab.IMPROVEMENT_PAYOFF_ROW_LABELS[
				SourceForecast.IMPROVEMENT_SOW]).to_upper()))
	h._assert_hud("…and the box's own face carries none of it",
		not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_SOW).contains(ForageFx.IMPROVEMENT_PAYOFF_NEEDLE))
	# **THE ASYMMETRY THAT IS RUNG 3's BARGAIN, read across the two registers rather than described.**
	# A bare sow has no standing stand to take a fraction of, so the crew carries almost nothing while
	# the crop is in the ground and the Field then pays 2× a tended patch. Asserted as `<` against the
	# HEADLINE take, not against literals: the claim is the ORDER of the two terms, and pinning either
	# magnitude would pin this fixture's arithmetic.
	var sow_payoff := Readout.improvement_deal_value(h._hud._drawercompose._compose_sheet)
	var sow_now := Readout.yields_account_number(
		h._hud._drawercompose._compose_sheet, SourceForecast.YIELD_ACCOUNT_FOOD)
	print("ui_preview: sow deal  while-building=%s  once-sown=%s" % [sow_now, sow_payoff])
	h._assert_hud("…far above the dipped take the same sheet is quoting while it builds",
		sow_payoff != Readout.DEAL_ROW_ABSENT
			and sow_now != Readout.YIELDS_ACCOUNT_ABSENT
			and float(sow_now) < float(sow_payoff.split(" ")[0]))

	# State 6b-crop-picker-sow — THE SAME long basket as `forage_crop_picker`, one rung up, on ground
	# that will take seed. `can_sow` is a DIFFERENT flag from `can_cultivate`, so only Wild Emmer stays
	# legal here and Hazel/Ground Nut join the greyed rows: the two frames side by side are what prove
	# the gate reads the composed rung's own flag rather than one "can be farmed" bit.
	h._show_tile(_sowable_long_basket_tile_fixture())
	h._hud._compose.set_forage_improvement("sow")
	h._hud._compose.set_forage_species("")
	h._compose_forage(_sowable_long_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_picker_sow")
	# **THE SOW HALF OF THE HEADER PAIR.** A Field has no volunteers — the rung forces the favored
	# species to 100% of the stand — so committing is exactly what this picker does, and the word that
	# is wrong one rung down is right here. Both halves, or a single hard-wired string passes one.
	h._assert_hud("the Sow picker asks which crop to COMMIT to, the word its own rung earns",
		Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			HudFloraVocab.FLORA_CROP_PICKER_HEADER.to_upper()))
	h._assert_hud("…and not the Cultivate rung's tending question",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			HudFloraVocab.FLORA_CROP_TEND_HEADER.to_upper()))

	# State F3 fodder crop — a basket with a HAY crop under Sow. Hay Grass pays fodder, not provisions,
	# so its provisions ratio is 0 and the ordinary "N.N×" row would read it as worthless; the picker
	# instead shows "Hay Grass 30% · 1.8 hay". The provisions crop beside it (Wild Emmer) keeps its
	# unchanged "70% · 3.2×" ratio — proof a normal crop's row is untouched.
	h._show_tile(ForageFx.fodder_basket_tile_fixture())
	h._hud._compose.set_forage_improvement("sow")
	h._hud._compose.set_forage_species("")
	h._compose_forage(ForageFx.fodder_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_picker_fodder")

	# State F4 cash crop — a basket with a CASH crop under Sow. Flax pays a MATERIAL, not provisions or
	# fodder, so its provisions ratio is 0 and the ordinary "N.N×" row would read it as worthless; the
	# picker instead shows "Flax 30% · 0.72 fibre". The provisions crop beside it (Wild Emmer) keeps its
	# unchanged "70% · 3.2×" ratio and — a sown Field being 100% its crop — quotes NO material at all,
	# which is the Sow half of the two-rungs-differ pair (arc #527).
	h._show_tile(ForageFx.cash_basket_tile_fixture())
	h._compose_forage(ForageFx.cash_basket_tile_fixture())   # settle the source key first (it changed)
	h._hud._compose.set_forage_improvement("sow")
	h._hud._compose.set_forage_species("")
	h._compose_forage(ForageFx.cash_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_picker_cash")

	# **STATE F5 — THE CASH-CROP GATHER SHEET, and the frame this whole pass exists for.** A tile 32%
	# cotton and 26% tobacco composed a forage sheet reading `0.24 → 0.18 FOOD · — FODDER` and never
	# mentioned the fibre and tobacco the gather actually banks: `_forage_yield_model` passed FOUR
	# arguments to `yield_rows` where its hunt twin passed five, so the plant web never got the
	# material vector the animal web had. Reported from a screenshot.
	#
	# **NO IMPROVEMENT COMPOSED** — this is the WILD rung, a plain gather, which is the state a player
	# is in before they have decided anything. The crop picker's per-plant material rows are a
	# different question on a different control (`land-readouts.md`); this row is what the crew brings
	# home from the ground as it stands.
	h._show_tile(ForageFx.cash_crop_gather_tile_fixture())
	h._compose_forage(ForageFx.cash_crop_gather_tile_fixture())   # settle the source key first
	h._hud._compose.set_forage_improvement("")
	h._hud._compose.set_forage_species("")
	h._compose_forage(ForageFx.cash_crop_gather_tile_fixture())
	await h._settle()
	await h._save("forage_cash_crop_gather")
	# **THREE CLAIMS AND A CONTROL.** The two materials must both appear — a fixture with one would
	# pass against a producer that summed the vector — their SUM must not, and the FOOD row must still
	# read, or "quote the materials" is satisfied by a sheet that stopped quoting the food.
	var cash_yields := Readout.yields_text(h._hud._drawercompose._compose_sheet)
	var cash_crew: int = h._hud._compose.forage_count()
	var fibre := float(cash_crew) * ForageFx.CASH_PATCH_FIBRE_PER_WORKER
	var tobacco := float(cash_crew) * ForageFx.CASH_PATCH_TOBACCO_PER_WORKER
	h._assert_hud("the cash-crop gather composes a crew at all", cash_crew > 0)
	h._assert_hud("the forage sheet quotes the fibre its gather banks (got \"%s\")" % cash_yields,
		cash_yields.contains(SourceForecast.format_magnitude(fibre))
			and cash_yields.contains(ForageFx.CASH_PATCH_FIBRE_ID.to_upper()))
	h._assert_hud("…and the tobacco beside it, as its OWN row",
		cash_yields.contains(SourceForecast.format_magnitude(tobacco))
			and cash_yields.contains(ForageFx.CASH_PATCH_TOBACCO_ID.to_upper()))
	# **THE "NOT SUMMED" CLAIM IS STRUCTURAL, NOT NUMERIC.** A needle for the sum's DIGITS is a
	# coincidence waiting to happen — this sheet already prints the food row's `after` reading, which
	# collided with it exactly once — so the claim is that each material has a ROW OF ITS OWN, read
	# back by account. A producer that summed the vector could only ever answer one row.
	h._assert_hud("…as two SEPARATE rows, never one summed figure",
		Readout.yields_account_number(h._hud._drawercompose._compose_sheet,
			ForageFx.CASH_PATCH_FIBRE_ID) != Readout.YIELDS_ACCOUNT_ABSENT
		and Readout.yields_account_number(h._hud._drawercompose._compose_sheet,
			ForageFx.CASH_PATCH_TOBACCO_ID) != Readout.YIELDS_ACCOUNT_ABSENT)
	h._assert_hud("…while the FOOD row still reads, so the materials did not replace it",
		cash_yields.contains(SourceForecast.YIELD_ACCOUNT_UNITS[
			SourceForecast.YIELD_ACCOUNT_FOOD].to_upper()))

	# State F4 per-tile flora realization — the SECOND Alluvial Plain tile. Same biome as the frame
	# above, but a DIFFERENT realized basket (Cotton 55% + Flax 45% vs Wild Emmer 70% + Flax 30%): two
	# tiles of one biome now carry a seeded per-tile subset, not the uniform per-biome roster. Rendered
	# beside `forage_crop_picker_cash`, the pair is the visible proof of the whole slice — read both.
	h._show_tile(ForageFx.cash_variant_basket_tile_fixture())
	h._compose_forage(ForageFx.cash_variant_basket_tile_fixture())   # settle the source key first (it changed)
	h._hud._compose.set_forage_improvement("sow")
	h._hud._compose.set_forage_species("")
	h._compose_forage(ForageFx.cash_variant_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_picker_cash_variant")

	# Issue #419 — THE SAME CASH BASKET ONE RUNG DOWN, which had no frame at all before this. Two
	# defects were invisible without it:
	#   1. Every row printed as non-food-only (`Wild Emmer 70% · 0.4 trade`), because "cash crop" was
	#      detected from `trade_payoff > 0` and EVERY staple carried the flat 0.005 trade token.
	#   2. The row quoted `sow_*` — a FIELD payoff — on the Cultivate rung, so flax advertised what a
	#      sown field pays instead of what a tended patch does.
	# It must now read `Wild Emmer 70% · 2.7× · 0.04 fibre` and `Flax 30% · 0.3× · 0.29 fibre`: the
	# ratio the rung exists to compare is back on every row, each row states every account it pays,
	# and the numbers are the tended rung's own. **The emmer's fibre is the Cultivate half of the
	# two-rungs-differ pair** (arc #527) — a tended grain patch keeps its flax volunteers standing and
	# honestly quotes their fibre, where its sown Field quotes none. Flax's food ratio is a warn-inked
	# LOSS and that is correct — rung 2 weeds rather than replaces, so committing to flax really does
	# surrender calories, which is the cost its material clause is the benefit of.
	h._show_tile(ForageFx.cash_basket_tile_fixture())
	h._compose_forage(ForageFx.cash_basket_tile_fixture())   # settle the source key first (it changed)
	h._hud._compose.set_forage_improvement("cultivate")
	h._hud._compose.set_forage_species("")
	h._compose_forage(ForageFx.cash_basket_tile_fixture())
	await h._settle()
	await h._save("forage_crop_picker_cash_cultivate")

	# State 6b-sowing — the rung-3 BUILD meter: the Field row reads "Sowing 45%", following the pen's
	# "Building 40%" / the fence's "Fencing 60%" convention. It sits BESIDE the "Cultivation 🌾 Tended
	# Patch" row: the patch carries TWO independent meters, and both are the SOURCE's own.
	h._show_tile(ForageFx.sowing_tile_fixture())
	await h._settle()
	await h._save("forage_field_building")

	# State 6b-field — the COMPLETED Field, top of the plant ladder. The row must read "▦ Field" in
	# SIGNAL cyan — a visibly DIFFERENT THING from "🌾 Tended Patch" (different word, different glyph),
	# not a bigger percentage. That is the whole test of rung 3's readout.
	h._show_tile(ForageFx.field_tile_fixture())
	await h._settle()
	await h._save("forage_field")

	# State 6b-cultivate-done — a COMPLETED Tended Patch with a standing Cultivate selection: the build is
	# DONE, so Cultivate is a dead-end no-op. 🌱 Cultivate greys with "Already a Tended Patch — ♻
	# Sustain-forage it to harvest", the composed policy falls back to Sustain, and the "Preparing → then"
	# prep line is GONE (the forecast now reads the Sustain harvest, +/turn). This is the fix for the panel
	# lying: Cultivate used to stay enabled and keep paying the low prep dip on a finished patch.
	h._show_tile(TileFx.tended_tile_fixture())
	h._hud._compose.set_forage_improvement("cultivate")
	h._compose_forage(TileFx.tended_tile_fixture())
	await h._settle()
	await h._save("forage_cultivate_done")
