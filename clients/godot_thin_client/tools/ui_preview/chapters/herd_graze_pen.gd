extends RefCounted

## Herds: grazing, the pen, corralling and taming.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const Spine := preload("res://tools/ui_preview/compose_vocab.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

const TAME_CAP_WOULD_BE_HERDERS := 30

# The Red Deer pen at its settled escapement point (design doc §7, MEASURED from a sim run): the
# feed the herd demands per turn, and the share of it a broke keeper managed to pay in the starving
# state. `pen_fed_fraction` < 1 ⇒ the herd is shrinking.
const UNDER_HERDED_CORRAL_HERDERS_NEEDED := 2

const UNDER_HERDED_CORRAL_HERDERS_STAFFED := 1

## The faction's PENNING on the two corral-gate frames and on `two_meter_split`. Named because each is
## asserted against the rendered gate reason's percent, so the fixture value and the expected string
## are one number rather than two that can drift. Deliberately DIFFERENT between the two states, so a
## frame quoting the other one's percent fails rather than passing off a shared constant.
const CORRAL_GATE_PENNING := 0.35

const TWO_METER_PENNING := 0.45

## Find the IMPROVEMENT control anywhere under `root`, by `HudWidgets.IMPROVEMENT_CONTROL_META` — its
## identity, never its face (which carries a live meter and a payoff and so changes every frame). The
## NODE TYPE is half the assertion: a `CheckBox` is offered or running (`button_pressed` tells those
## apart) and a plain `Label` is done, which is exactly the three-state contract. Returns the Control,
## so a caller can type-test it.
## The luminance gap an improvement checkbox's indicator must clear against the panel it sits on.
## 0.25 of the full range is far below what the fix delivers (~0.5 for both states) and far above what
## the defect scored: Godot's stock `unchecked` art is `#191919` at half alpha, which composites over
## `PANEL_SOLID` to a gap of ~0.001 — invisible, which is exactly the bug.
const CHECKBOX_INDICATOR_MIN_CONTRAST := 0.25

## How visible one of a CheckBox's indicator states is: composite every pixel of the art it would draw
## over `HudStyle.PANEL_SOLID` at that pixel's own alpha, and keep the largest luminance distance from
## the panel. **Measured off the art, not off a theme override**, because "the box is invisible" is a
## claim about pixels — an assertion phrased as "an override is set" would have passed on a
## `icon_normal_color` override, which a `CheckBox` ignores.
func _checkbox_indicator_contrast(control: Control, icon: String) -> float:
	var box := control as CheckBox
	if box == null:
		return 0.0
	var tex := box.get_theme_icon(icon)
	if tex == null:
		return 0.0
	var img := tex.get_image()
	if img == null:
		return 0.0
	var panel := HudStyle.PANEL_SOLID
	var panel_luminance := panel.get_luminance()
	var best := 0.0
	for y in img.get_height():
		for x in img.get_width():
			var px := img.get_pixel(x, y)
			var over := panel.lerp(Color(px.r, px.g, px.b), px.a)
			best = maxf(best, absf(over.get_luminance() - panel_luminance))
	return best

## How far the TICKED art's colour may sit from `HudStyle.SIGNAL` once brightness is divided out.
## Godot's stock tick chip is neutral grey, which scores ~0.65 on this measure, so 0.1 separates
## "recoloured to the live token" from "whatever the theme shipped" with room to spare.
const CHECKBOX_TICK_COLOUR_TOLERANCE := 0.1

## The ticked indicator's own colour, compared to `SIGNAL` with BRIGHTNESS DIVIDED OUT: take the art's
## most prominent pixel, scale it and `SIGNAL` so each has a largest channel of 1, and measure the
## distance. Brightness is normalised away because the chip's absolute level comes from the stock art
## being recoloured rather than redrawn; what is being pinned is that a running build wears the
## console's live colour instead of the stock theme's grey.
func _checkbox_tick_colour_gap(control: Control) -> float:
	var box := control as CheckBox
	if box == null:
		return 1.0
	var tex := box.get_theme_icon("checked")
	if tex == null:
		return 1.0
	var img := tex.get_image()
	if img == null:
		return 1.0
	var best := Color(0, 0, 0, 0)
	var best_weight := 0.0
	for y in img.get_height():
		for x in img.get_width():
			var px := img.get_pixel(x, y)
			var weight := px.get_luminance() * px.a
			if weight > best_weight:
				best_weight = weight
				best = px
	return _unit_channel(best).distance_to(_unit_channel(HudStyle.SIGNAL))

## A colour as a hue-only vector: its RGB scaled so the largest channel is 1. `Vector3.ONE` (neutral
## white/grey) for a black input, which reads as "no colour of its own" — the right answer for the
## degenerate case and the same answer a grey chip gives.
func _unit_channel(c: Color) -> Vector3:
	var peak: float = maxf(maxf(c.r, c.g), c.b)
	if peak <= 0.0:
		return Vector3.ONE
	return Vector3(c.r, c.g, c.b) / peak

## The same herd, STRESSED — the "why isn't my Tame progressing?" case. Taming accrues only while the
## herd is Thriving, but the verb is NOT gated on it (a herd's phase swings as you hunt it): the sim
## just PAUSES the meter. Nothing else in the HUD would tell the player, so the drawer must.
func _taming_stalled_herd_fixture() -> Dictionary:
	var fixture := HerdFx.taming_herd_fixture()
	fixture["ecology_phase"] = "stressed"
	return fixture

func _tame_worker_cap_herd_fixture() -> Dictionary:
	var fixture := HerdFx.taming_herd_fixture()
	fixture["herders_needed"] = 0
	# **THE WOULD-BE CREW HAS TO OUT-RANK THE TAKE-USEFUL for this frame to test the floor at all**,
	# and the number the take side answers QUADRUPLED when the build dip moved onto the crew
	# (`docs/plan_harvest_floor.md` §3.1): a Tame builder now needs ~27 hands to haul the same peak
	# whole-animal drop it needed 7 for. At the old 10 the floor no longer binds and the frame would
	# have been testing the take side under the floor's name.
	fixture["herders_needed_if_managed"] = TAME_CAP_WOULD_BE_HERDERS
	return fixture

## A nearly-tamed herd, FULLY STAFFED — the calm control for the staffing readout, AND the fix for the
## stale-count bug (fauna neglect-escape arc). `BandFx.band_fixture` has 4 herders (a Hunt assignment) on
## game_deer_07, and this herd needs 4, so the drawer reads a neutral "Herders: 4 / 4" — from the
## ACTUAL assigned count, not `herded_fraction`. `herded_fraction` is deliberately left STALE at 0.4
## (last turn's resolved value): the OLD code reconstructed `round(0.4 · 4) = 2` and wrongly read a
## self-contradictory "2 / 4 — under-herded", which this frame proves is gone.
func _fully_herded_herd_fixture() -> Dictionary:
	var fixture := HerdFx.taming_herd_fixture()
	fixture["domestication"] = 0.9
	HerdFx.set_managed_herders(fixture, 4)
	fixture["herded_fraction"] = 0.4
	return fixture

## The SAME herd, UNDER-HERDED — animals are drifting off (fauna neglect-escape arc; neglect no longer
## decays tameness, it sheds whole animals to the wild). The herd now needs 6 herders but `BandFx.band_fixture`
## only staffs 4, so the drawer reads the amber "Herders: 4 / 6 — under-herded" (the ACTUAL staffed
## count) plus the muted "Under-herded — animals are drifting off. Staff all 6 herders to hold the herd."
## line — NEVER the retired "tameness slipping" copy. `herded_fraction` is left STALE at 1.0: the OLD
## code reconstructed `round(1.0 · 6) = 6` and read a calm "6 / 6" with NO warning at all — the exact
## stale-reading this fix removes.
func _under_herded_herd_fixture() -> Dictionary:
	var fixture := _fully_herded_herd_fixture()
	fixture["domestication"] = 0.98
	HerdFx.set_managed_herders(fixture, 6)
	fixture["herded_fraction"] = 1.0
	return fixture

## A PREDATOR (Predators Phase 1a): a Grey Wolf Pack — big, wild-ceiling, carnivore. `prey_sense_radius`
## 4 (`> 0`) is BOTH the "this is a predator" signal AND the map ring radius, so the drawer must read
## "Size: Big predator" (not "Big game") and "Wild predator — hunt only" (not "Wild game — hunt only").
func _predator_herd_fixture() -> Dictionary:
	var fixture := HerdFx.herd_fixture()
	fixture["id"] = "predator_wolf_01"
	fixture["label"] = "Grey Wolf Pack (predator_wolf_01)"
	fixture["species"] = "Grey Wolf Pack"
	fixture["size_class"] = "big"
	fixture["husbandry_ceiling"] = "wild"
	fixture["prey_sense_radius"] = 4
	fixture["attack"] = 5.0
	fixture["defense"] = 3.0
	fixture["ferocity"] = 0.8
	fixture["aggression"] = 0.7
	fixture["tile_info"] = HerdFx.compact_herd_tile_fixture()
	return fixture

## Assertion helpers for the Predators component rows. `_danger_component_rows_present` = all four
## component keys emitted; `_danger_verdict_word_present` = the OLD danger vocabulary must be gone (a
## word can't survive the roster); `_danger_row_value` extracts a row's value cell for the bar/% check.
func _danger_component_rows_present(lines: Array) -> bool:
	for key in ["Attack", "Defense", "Fights back", "Aggressive"]:
		if _danger_row_value(lines, key) == "":
			return false
	return true

## True when SOME produced line is EXACTLY `text` (used to pin a specific "Herders: N / M" readout).
func _lines_contain(lines: Array, text: String) -> bool:
	for line in lines:
		if String(line) == text:
			return true
	return false

## True when SOME produced line CONTAINS `text` (used to pin the shed copy / prove the old copy is gone).
func _lines_any_contain(lines: Array, text: String) -> bool:
	for line in lines:
		if String(line).contains(text):
			return true
	return false

func _danger_verdict_word_present(lines: Array) -> bool:
	for line in lines:
		var text := String(line)
		for word in ["Harmless", "Minor", "Dangerous", "Deadly"]:
			if text.contains(word):
				return true
	return false

func _danger_row_value(lines: Array, key: String) -> String:
	# The three components `Danger` is made of are INDENTED under it, so a row is matched on its key
	# with the indent stripped rather than on a bare `begins_with`. `Defense` is not one of them and
	# stands flat, which the empty prefix still finds.
	for prefix in ["%s%s: " % [DetailFormat.DANGER_COMPONENT_INDENT, key], "%s: " % key]:
		for line in lines:
			var text := String(line)
			if text.begins_with(prefix):
				return text.substr(prefix.length())
	return ""

## A PASTORAL-ceiling herd (Grazing 2d-δ): tameable + roams, but never pennable. The drawer keeps the
## domestication (Husbandry) row but shows "Herdable, not pennable" where the Corral rows would sit, and
## the hunt policy picker drops the Corral rung.
func _pastoral_herd_fixture() -> Dictionary:
	var fixture := HerdFx.herd_fixture()
	fixture["husbandry_ceiling"] = "pastoral"
	fixture["domestication"] = 0.6
	fixture["tile_info"] = HerdFx.compact_herd_tile_fixture()
	return fixture

## An OVERGRAZING herd: biomass (2100) exceeds the K (1352) its range can sustainably feed, so the
## merged pair reads "Biomass: 2100 / 1352" (current ABOVE max) and the drawer adds the WARN-amber
## "⚠ Overgrazing — range can't sustain this herd" row. The herd is drawing its range down and will
## shrink — the honest biomass > K comparison, both numbers sim-provided.
func _overgrazing_herd_fixture() -> Dictionary:
	var fixture := HerdFx.herd_fixture()
	fixture["domestication"] = 0.0
	fixture["biomass"] = 2100.0
	fixture["carrying_capacity"] = 1352.0
	fixture["graze_range_radius"] = 1
	fixture["tile_info"] = HerdFx.compact_herd_tile()
	return fixture

## A SMALL-GAME herd (radius-0 range): it grazes only its own tile, so the drawer reads "Range: 1 tile"
## (singular) and the map draws a single-hex highlight. Biomass below its small K → no overgrazing.
func _small_game_herd_fixture() -> Dictionary:
	var fixture := HerdFx.herd_fixture()
	fixture["id"] = "game_rabbit_03"
	fixture["label"] = "Rabbit Warren (game_rabbit_03)"
	fixture["species"] = "Rabbit Warren"
	fixture["size_class"] = "small"
	fixture["domestication"] = 0.0
	fixture["biomass"] = 140.0
	fixture["carrying_capacity"] = 190.0
	fixture["graze_range_radius"] = 0
	fixture["tile_info"] = HerdFx.compact_herd_tile()
	return fixture

## A composing-Corral herd that needs MORE than one keeper (Grazing 2d-δ herder deficit): the take/prepare
## max-useful for the Corral rung is 1 ("one worker suffices to prepare"), but this growing herd needs 2
## herders EVERY turn to hold its tameness — and it is currently UNDER-herded, because the STATE dials the
## band's own hunt assignment on this herd down to 1 (the base fixture staffs 4, which would hide the very
## deficit this frame exists to show). The Herders row reads "1 / 2 — under-herded" off that ACTUAL
## assignment via `HudBandLaborState.assigned_herders_for`, and the shed consequence line names 2.
## `herded_fraction` is deliberately left STALE at 0.5 and is read NOWHERE in the render path — the same
## convention `_fully_herded_fixture` / `_under_herded_fixture` carry, and the reason the old
## reconstruct-from-the-fraction reading was retired. The compose
## stepper's cap must be max(take-useful 1, herders_needed 2) = 2, so the `+` reaches 2 and the player can
## staff the maintenance crew — otherwise the corral is lost, an unwinnable trap. A wild herd carries
## `herders_needed 0`, so this floor is a no-op there.
func _under_herded_corral_fixture() -> Dictionary:
	var fixture := HerdFx.corral_ready_herd_fixture()
	# Corral is an INVESTMENT rung, and `_forecast_worker_cap`'s floor reads the ownership-INDEPENDENT
	# `herders_needed_if_managed` for those (`herders_needed` is the extractive rungs' field). The sim
	# exports the two EQUAL on an owned herd, which this one is — so a fixture setting only the first
	# floors the cap at 0 and the frame silently renders "max 1 worker useful", the very cap it exists
	# to disprove. It went unnoticed because the state used to compose Sustain by accident (#357), and
	# it is now caught by the field-pair guard rather than by a reader noticing the wrong number.
	HerdFx.set_managed_herders(fixture, UNDER_HERDED_CORRAL_HERDERS_NEEDED)
	fixture["herded_fraction"] = 0.5
	return fixture

## A DOMESTICATED but DEPLETED herd (biomass below the pen's escapement point, K/2): the pen's
## harvest takes only the biomass standing ABOVE K/2, so `corral_yield` is honestly **0.00** — penning
## this herd would eat 0.14 food/turn and pay nothing until it rebuilds. The zero is the whole point
## of the frame: it must render in full (never blanked or em-dashed) and be EMPHASIZED, because a
## player who pens this herd on a hidden zero has been misled by the UI.
func _depleted_corral_herd_fixture() -> Dictionary:
	var fixture := HerdFx.corral_ready_herd_fixture()
	fixture["biomass"] = 260.0
	fixture["ecology_phase"] = "stressed"
	fixture["corral_progress"] = 0.0
	# Everything scales off the shrunken herd — including the dip, which is a share of its MSY.
	fixture["per_worker_yield"] = 0.10
	# Override the inherited ceiling table's two rows this frame reads — Sustain (the extractive
	# baseline) and the Corral DIP. The herd's ceilings live only in `hunt_policy_ceilings` now, so
	# a depleted variant must restate them here rather than shadowing them with flat scalars.
	var depleted_ceilings: Dictionary = (fixture["hunt_policy_ceilings"] as Dictionary).duplicate()
	depleted_ceilings["sustain"] = 0.10
	fixture["hunt_policy_ceilings"] = depleted_ceilings
	# The dip is a SHARE of whatever stance is held, so a shrunken herd needs no dip override at all —
	# the 0.05-against-0.10 this used to restate is exactly the inherited 0.256 fraction of the new
	# Sustain ceiling. That the override became unnecessary is the fraction form paying for itself.
	fixture["corral_yield"] = 0.0     # below K/2 → the escapement harvest takes NOTHING
	fixture["pen_upkeep"] = 0.14      # …and it would still have to be fed
	return fixture

## A SELF-FEEDING pen on lush land (Grazing 2d-γ): a radius-2 fenced footprint (19 tiles) whose grazing
## covers the herd's entire feed, so `pen_pasture_fraction` 1.0 and the NET larder bill `pen_larder_bill`
## is 0 (the GROSS `pen_upkeep` stays 1.74). The feed-split row reads "Fed by pasture 100% · larder 0.0
## food/turn" and the amber Pen-feed
## debit row disappears (nothing left to haul). This is the state the Extend-pen affordance renders on —
## a built pen, no ring in flight (`pen_extend_progress` 0), so `_build_herd_assign_controls` shows the
## "Extend pen" button.
func _self_feeding_pen_herd_fixture() -> Dictionary:
	var fixture := HerdFx.domesticated_herd_fixture()
	fixture["pen_radius"] = 2
	fixture["pen_footprint_tiles"] = 19
	fixture["pen_pasture_fraction"] = 1.0
	# `pen_upkeep` stays the realistic GROSS (inherited 1.74); the FOOTPRINT grazes the whole demand, so
	# the net FOOD-larder bill is 0 → "100% · larder 0.0" and the Pen-feed debit row disappears.
	# Invariant: gross × pasture(1.0) + hay(0) + larder(0) == gross(1.74).
	fixture["pen_larder_bill"] = 0.0
	fixture["pen_hay_food"] = 0.0
	fixture["pen_extend_progress"] = 0.0
	return fixture

## The SAME pen mid-EXTENSION (Grazing 2d-γ): the keeper is fencing the next ring, so
## `pen_extend_progress` is 0.6 and `_build_herd_assign_controls` replaces the "Extend pen" button with
## a WARN-amber "Fencing 60%" badge. Partial pasture (60%) so the feed-split reads "60% · larder N.N".
func _extending_pen_herd_fixture() -> Dictionary:
	var fixture := HerdFx.domesticated_herd_fixture()
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_pasture_fraction"] = 0.6
	# `pen_upkeep` stays the realistic GROSS (inherited 1.74); the footprint grazes 60%, so the net
	# FOOD-larder bill is gross × (1 − 0.6) = 0.696 → "60% · larder 0.7", no hay.
	# Invariant: gross(1.74) × pasture(0.6) + hay(0) + larder(0.696) == gross(1.74).
	fixture["pen_larder_bill"] = 0.696
	fixture["pen_hay_food"] = 0.0
	fixture["pen_extend_progress"] = 0.6
	return fixture

## A FODDERED pen (Flora roster F3): the pen knows Foddering and drew hay, so its feed is a THREE-way
## split, all food units. GROSS demand `pen_upkeep` = 2.0 partitions into: pasture 40%
## (`pen_pasture_fraction` 0.40 → 0.80 food grazed free), hay 0.90 (`pen_hay_food`, the food it
## displaced from the larder), and the NET bread bill 0.30 (`pen_larder_bill`). The frame PROVES the
## sim-pinned invariant, not a hand-picked answer: 0.80 + 0.90 + 0.30 == 2.0 (gross). Feed-split reads
## "Fed by pasture 40% · hay 0.9 · larder 0.3 food/turn"; the Pen-feed row states the same 0.3 net bill.
func _foddered_pen_herd_fixture() -> Dictionary:
	var fixture := HerdFx.domesticated_herd_fixture()
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_upkeep"] = 2.0        # realistic GROSS (upkeep_per_biomass × biomass scale)
	fixture["pen_pasture_fraction"] = 0.40
	fixture["pen_hay_food"] = 0.90
	fixture["pen_larder_bill"] = 0.30  # 2.0 − (2.0 × 0.40) − 0.90 == 0.30
	fixture["pen_extend_progress"] = 0.0
	return fixture

func run(harness) -> void:
	h = harness

	# State 3 — a huntable herd selected on a food tile, WITHIN the band's hunt reach: the "Assign
	# hunters" controls (a "Band:" dropdown naming the actor band, a Hunters −/+ count, the
	# sustain/surplus/deplete/eradicate policy picker, and the local "Hunt Here" button). A
	# Thriving herd shows a neutral ecology readout in the drawer.
	# Push both fixtures as the known-herd roster so the open-ended Attack/Defense bars have a
	# reference to normalize against (Elevation-style) — the mammoth holds the roster max.
	h._set_world_herds([HerdFx.herd_fixture(), HerdFx.deadly_herd_fixture()])
	h._show_herd(HerdFx.herd_fixture())
	h._compose_herd(HerdFx.herd_fixture())
	await h._settle()
	await h._save("herd_verbs")
	# THE PAIR. Without a wild herd asserted here, "commits with the herders' verb" is satisfied by a
	# button hard-coded the OTHER way — the same bug with the sides swapped.
	h._assert_hud("a WILD herd is still staffed by HUNTERS and still commits `Hunt Here`",
		Readout.crew_row_label(h._hud._drawercompose._compose_sheet)
				== HudComposeVocab.HUNT_CREW_LABEL.to_upper()
			and Q.compose_commit_button(h._hud._drawercompose._compose_sheet) != null
			and Q.compose_commit_button(h._hud._drawercompose._compose_sheet).text
				== HudComposeVocab.ASSIGN_LOCAL_HUNT_BUTTON)
	# ASSERT the HARMLESS case: the base Red Deer carries no combat components (all default 0), so its
	# component rows all read empty — and crucially NO "Harmless"/"Deadly" verdict word appears (words
	# don't survive the roster). The rows are the raw components, Elevation-style.
	var deer_lines = DetailFormat.herd_summary_lines(HerdFx.herd_fixture(), h._hud._band_labor.world_herds())
	assert(_danger_component_rows_present(deer_lines))
	assert(not _danger_verdict_word_present(deer_lines))
	assert(_danger_row_value(deer_lines, "Fights back").ends_with("0%"))

	# State 3b-danger — a DEADLY-TO-HUNT herd (a mammoth: attack 8, ferocity 0.9, aggression 0). Its
	# component rows read high Attack + high Fights back but EMPTY Aggressive — the "deadly to hunt, no
	# camp threat" story at a glance. Still no verdict word.
	h._show_herd(HerdFx.deadly_herd_fixture())
	await h._settle()
	await h._save("herd_danger")
	var mammoth_lines = DetailFormat.herd_summary_lines(HerdFx.deadly_herd_fixture(), h._hud._band_labor.world_herds())
	assert(_danger_component_rows_present(mammoth_lines))
	assert(not _danger_verdict_word_present(mammoth_lines))
	# Fights back 90%, Aggressive 0% — the split that proves strength ≠ danger.
	assert(_danger_row_value(mammoth_lines, "Fights back").ends_with("90%"))
	assert(_danger_row_value(mammoth_lines, "Aggressive").ends_with("0%"))
	# **THE ANSWER LEADS AND ITS THREE FACTORS INDENT UNDER IT.** `Hunt` is `attack × ferocity` and
	# `Threat` is `attack × aggression`, so exactly three of the four components compose the derived
	# row — Defense is in neither and stays FLAT, above it, with the other facts about what this herd
	# is. Asserting the indent per row rather than the block's order alone: the claim is "Defense does
	# not contribute", and a reordering that indented all four would satisfy any pure ordering test.
	var mammoth_text = "\n".join(mammoth_lines)
	var indent := DetailFormat.DANGER_COMPONENT_INDENT
	h._assert_hud("Danger's three factors are indented under it",
		mammoth_text.contains("%s%s: " % [indent, DetailFormat.DANGER_ATTACK_ROW])
			and mammoth_text.contains("%s%s: " % [indent, DetailFormat.DANGER_FEROCITY_ROW])
			and mammoth_text.contains("%s%s: " % [indent, DetailFormat.DANGER_AGGRESSION_ROW]))
	h._assert_hud("…while Defense stays flat, being in neither product",
		mammoth_text.contains("\n%s: " % DetailFormat.DANGER_DEFENSE_ROW)
			and not mammoth_text.contains("%s%s: " % [indent, DetailFormat.DANGER_DEFENSE_ROW]))
	h._assert_hud("…and the derived row LEADS them rather than trailing four equal-weight inputs",
		mammoth_text.find("%s: " % DetailFormat.DANGER_DERIVED_ROW)
			< mammoth_text.find("%s%s: " % [indent, DetailFormat.DANGER_ATTACK_ROW]))
	# **THE INDENT MUST NOT COLLIDE WITH THE FULL-WIDTH SUB-LINE PREFIX.** `detail_bbcode` routes any
	# line beginning with `MORALE_BREAKDOWN_INDENT` out of the KV table and into a full-width branch,
	# which would leave these bars starting at three different x positions — and a bar that shares no
	# column measures nothing. Both halves asserted: the prefixes cannot collide, AND the row really
	# did render as a table cell, which is the fact the first half exists to protect.
	h._assert_hud("the danger indent cannot be swallowed by the full-width sub-line branch",
		not indent.begins_with(DetailFormat.MORALE_BREAKDOWN_INDENT))
	h._assert_hud("…so an indented factor still renders as a KV table cell, bars in one column",
		DetailFormat.detail_bbcode(mammoth_lines, DetailFormat.Context.new()).contains(
			"[cell][color=#%s]%s%s" % [HudStyle.INK_DIM_HEX, indent, DetailFormat.DANGER_ATTACK_ROW]))

	# State 3b-predator (Predators Phase 1a) — a carnivore (Grey Wolf Pack, prey_sense_radius 4): a
	# predator is a HUNTER, not quarry, so the Size row reads "Big predator" (not "Big game") and the
	# wild-ceiling hint reads "Wild predator — hunt only" (not "Wild game — hunt only").
	h._show_herd(_predator_herd_fixture())
	await h._settle()
	await h._save("herd_predator")
	var wolf_lines = DetailFormat.herd_summary_lines(_predator_herd_fixture(), h._hud._band_labor.world_herds())
	var wolf_text = "\n".join(wolf_lines)
	assert(wolf_text.contains("Big predator"))
	assert(wolf_text.contains("Wild predator — hunt only"))
	assert(not wolf_text.contains("Big game"))
	assert(not wolf_text.contains("Wild game"))
	# A HERBIVORE (the deer, prey_sense_radius absent/0) is byte-for-byte unchanged — still "game".
	var deer_size_lines = DetailFormat.herd_summary_lines(HerdFx.herd_fixture(), h._hud._band_labor.world_herds())
	assert("\n".join(deer_size_lines).contains("game"))

	# State 3b — an overhunted herd: the ecology readout warns "⚠ Collapsing" in red.
	h._show_herd(HerdFx.collapsing_herd_fixture())
	await h._settle()
	await h._save("herd_collapsing")

	# State 3b-graze — the ecological carrying-capacity readout (Grazing Phase 2b-iii). A HEALTHY herd:
	# the drawer shows the merged "Herd: 15 / 22 · Thriving" pair (animals standing vs the ceiling the
	# land sets, its ecology phase riding the row) + a separate "Range: 7 tiles" row — with NO
	# overgrazing warning (biomass ≤ K).
	h._show_herd(HerdFx.grazing_healthy_herd_fixture())
	await h._settle()
	await h._save("herd_grazing_healthy")
	# ASSERT THE UNIT, not just that a pair rendered. 1480 biomass ÷ 100 body_mass = 15 animals against
	# 2150 ÷ 100 = 22 — so a row that silently kept counting biomass would read "1480 / 2150" and fail
	# BOTH halves. The phase clause is asserted on the same line because folding the standalone
	# `Ecology` row into this one is the other half of the change; if it split back out, the row would
	# read a bare "15 / 22" and this fails while a plain contains("15 / 22") would not.
	#
	# **`_assert_hud`, NOT a bare `assert`, and deliberately** — the bare form the danger rows above
	# use halts this harness on failure instead of reporting one: a headless run breaks into the
	# debugger and hangs until it is killed, so the failing line is only findable in a stack trace on
	# stderr. Measured while sabotage-checking this very assertion.
	var graze_lines = DetailFormat.herd_summary_lines(
		HerdFx.grazing_healthy_herd_fixture(), h._hud._band_labor.world_herds())
	var graze_text = "\n".join(graze_lines)
	h._assert_hud("the herd's stock row counts ANIMALS against its ceiling, phase riding the row",
		graze_text.contains("Herd: 15 / 22 · Thriving"))
	h._assert_hud("…so neither the biomass number nor its label survives anywhere on the card",
		not graze_text.contains("1480") and not graze_text.contains("Biomass"))
	h._assert_hud("…and no standalone Ecology row does either — the phase is stated once, on the stock",
		not graze_text.contains("Ecology:"))

	# State 3b-overgraze — the same rows, but biomass (2100) > K (1352): the pair reads "Herd: 21 / 14"
	# (current > max) and the WARN-amber "⚠ Overgrazing — range can't sustain this herd" row appears
	# beneath. It shows ONLY when biomass exceeds K — the honest sim-number comparison, not a
	# re-derived ecology model.
	h._show_herd(_overgrazing_herd_fixture())
	await h._settle()
	await h._save("herd_overgrazing")
	# ASSERT THE OVERSHOOT SURVIVES THE UNIT CHANGE. A `current > max` pair is the whole reason this is
	# a pair and not a fill percentage, and dividing both sides by a body could have been written to
	# clamp. 2100 ÷ 100 = 21 against 1352 ÷ 100 = 14 (13.52, rounded) — still the wrong way round.
	var overgraze_text = "\n".join(DetailFormat.herd_summary_lines(
		_overgrazing_herd_fixture(), h._hud._band_labor.world_herds()))
	h._assert_hud("an overgrazed herd still reads current ABOVE max, in animals",
		overgraze_text.contains("Herd: 21 / 14"))
	h._assert_hud("…with the warning that says what the inverted pair costs",
		overgraze_text.contains(DetailFormat.OVERGRAZING_WARNING))

	# State 3b-smallgame — a radius-0 herd (small game grazes only its own tile): "Range: 1 tile"
	# (singular), and the map draws a single-hex highlight rather than a ring.
	h._show_herd(_small_game_herd_fixture())
	await h._settle()
	await h._save("herd_grazing_small_game")

	# State 3c — a domesticated + corralled herd: the drawer shows "Husbandry 🐄 Domesticated"
	# AND "Corral 🐄 Corralled" (SIGNAL tint), the herd end of the intensification ladder — plus the
	# amber "Pen feed -1.74 /turn" row, the running cost a penned (non-grazing) herd costs its keeper.
	h._show_herd(HerdFx.domesticated_herd_fixture())
	await h._settle()
	await h._save("herd_domesticated")

	# State 3c-starving — the same pen, UNDERFED (`pen_fed_fraction` 0.40): the herd is shrinking
	# every turn and the drawer says so in red — "Corral ⚠ Starving — 40% fed" replaces the penned
	# badge, and the Pen feed row names the shortfall ("only 40% paid"). Biomass is visibly down.
	h._show_herd(HerdFx.starving_pen_herd_fixture())
	await h._settle()
	await h._save("herd_corral_starving")

	# Staffing readout (fauna neglect-escape arc) — the fix for the stale "N of M working" count. The
	# reference band (`BandFx.band_fixture`) staffs 4 herders on game_deer_07, and the count now comes from
	# that ACTUAL assignment, never from last turn's resolved `herded_fraction`.
	# FULLY STAFFED: the herd needs 4 and 4 are on it → a calm "Herders: 4 / 4" (neutral ink), no
	# consequence line. `herded_fraction` is a stale 0.4, so the OLD reconstruction would have read a
	# self-contradictory "2 / 4 — under-herded" — proving the fix.
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._band_labor._player_bands = []
	h._show_herd(_fully_herded_herd_fixture())
	await h._settle()
	await h._save("herd_fully_herded")
	# ASSERT the corrected count: the actual staffed 4 shows, not the stale reconstruction 2.
	var fully_lines = DetailFormat.herd_summary_lines(
		_fully_herded_herd_fixture(), h._hud._band_labor.world_herds(),
		h._hud._band_labor.assigned_herders_for(_fully_herded_herd_fixture()["id"]))
	assert(_lines_contain(fully_lines, "Herders: 4 / 4"))
	assert(not _lines_any_contain(fully_lines, "under-herded"))

	# UNDER-HERDED: the herd now needs 6 herders but only 4 are staffed → an amber "Herders: 4 / 6 —
	# under-herded" (the ACTUAL count) plus the shed line "Under-herded — animals are drifting off.
	# Staff all 6 herders to hold the herd." — NOT the retired "tameness slipping" copy. `herded_fraction`
	# is a stale 1.0, so the OLD reconstruction would have read a calm "6 / 6" with no warning.
	h._show_herd(_under_herded_herd_fixture())
	await h._settle()
	await h._save("herd_under_herded")
	var under_lines = DetailFormat.herd_summary_lines(
		_under_herded_herd_fixture(), h._hud._band_labor.world_herds(),
		h._hud._band_labor.assigned_herders_for(_under_herded_herd_fixture()["id"]))
	assert(_lines_contain(under_lines, "Herders: 4 / 6 — under-herded"))
	assert(_lines_any_contain(under_lines, "animals are drifting off"))
	assert(not _lines_any_contain(under_lines, "slipping"))

	# State 2d-γ self-feeding pen — a radius-2 pen (19 fenced tiles) on lush land: the fenced footprint
	# grazes the WHOLE feed, so the feed-split reads "Fed by pasture 100% · larder 0.0 food/turn" and the
	# amber Pen-feed debit row is gone. With no ring in flight, `_build_herd_assign_controls` shows the
	# "Extend pen" button (issues extend_pen at the pen anchor). Also carries the "Pen: radius 2 · 19
	# tiles" footprint row.
	h._hud._compose.reset_hunt_source()
	h._show_herd(_self_feeding_pen_herd_fixture())
	h._compose_herd(_self_feeding_pen_herd_fixture())
	await h._settle()
	await h._save("herd_pen_self_feeding")

	# State 2d-γ extending pen — the SAME pen mid-extension (`pen_extend_progress` 0.6): the keeper is
	# fencing the next ring, so the "Extend pen" button is replaced by a WARN-amber "Fencing 60%" badge
	# (the pen twin of the corral-build "Building N%" meter). Partial pasture → "Fed by pasture 60% ·
	# larder 0.7 food/turn".
	h._hud._compose.reset_hunt_source()
	h._show_herd(_extending_pen_herd_fixture())
	h._compose_herd(_extending_pen_herd_fixture())
	await h._settle()
	await h._save("herd_pen_extending")

	# State F3 foddered pen — the honest THREE-way feed split. The pen drew hay, so its GROSS demand
	# (`pen_upkeep` 2.0) partitions into pasture 40% (0.80 free) · hay 0.9 (`pen_hay_food`) · larder 0.3
	# (`pen_larder_bill`, the NET bread bill) — 0.80 + 0.90 + 0.30 == 2.0, the sim-pinned invariant. It
	# reads "Fed by pasture 40% · hay 0.9 · larder 0.3 food/turn"; the two-term states above
	# (`herd_domesticated` 0% · larder 1.7, `herd_pen_self_feeding` 100% · larder 0.0) show NO hay term,
	# so the two forms are provably different — and the larder term is now the true net, not the gross.
	h._hud._compose.reset_hunt_source()
	h._show_herd(_foddered_pen_herd_fixture())
	h._compose_herd(_foddered_pen_herd_fixture())
	await h._settle()
	await h._save("herd_pen_foddered")

	# State 2d-δ wild ceiling — a hunt-only species. NO husbandry track in the drawer (no
	# domestication / corral / pen rows), just the dim "Wild game — hunt only" hint, and the hunt policy
	# picker offers the extractive four with NO Corral rung.
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.wild_herd_fixture())
	h._compose_herd(HerdFx.wild_herd_fixture())
	await h._settle()
	await h._save("herd_ceiling_wild")

	# State 2d-δ pastoral ceiling — tameable + roams, never pennable. The drawer KEEPS the "Husbandry
	# Domesticating 60%" row but shows "Herdable, not pennable" where the Corral rows would sit; the hunt
	# policy picker again drops the Corral rung.
	h._hud._compose.reset_hunt_source()
	h._show_herd(_pastoral_herd_fixture())
	h._compose_herd(_pastoral_herd_fixture())
	await h._settle()
	await h._save("herd_ceiling_pastoral")

	# ---- Corral: the hunt INVESTMENT rung, gated then ungated -------------------------------------
	# THE PAIR IS ONE A/B: the same FULLY TAMED herd, and the only thing that moves between the two
	# frames is the faction's Penning. That is the whole claim — Corral is gated on PENNING and on
	# nothing else — and it is why Herding is fully known in both.
	#
	# **WHAT MOVES BETWEEN THE HALVES IS NOW THE CONTROL'S EXISTENCE, not its shape.** A gated Corral
	# can only ever be gated on KNOWLEDGE here (the SOURCE half is unreachable — see below), and this
	# sheet renders no control for a knowledge-only gate, so the A/B reads "no control" → "a live box"
	# rather than "a Label" → "a live box". The claim it exists to make is unchanged and, if anything,
	# sharper: the ANIMAL is identical across the two, so Penning alone is what produces the offer.
	#
	# **THE FIXTURE HAD TO CHANGE, not the description** (issue #442). These two frames used to stage a
	# 40%-tamed herd and document a gated Corral wearing its "This herd is 40% tamed" SOURCE reason; on
	# a herd that is not yet tamed the control now offers 🐾 Tame — the next rung — so no Corral gate
	# rendered in either frame and both described a subject they no longer showed. A gated Corral needs
	# Corral to BE the next rung, which needs Tame retired, which needs the herd fully tamed. That is
	# also why the SOURCE half of the gate is no longer reachable in this control at all: the moment it
	# would apply, Tame is what is offered instead, and the remedy is a checkbox rather than a sentence.
	#
	# State 3c-corral-gated — **THE SUPPRESSION RULE'S OWN FRAME, on the animal web**, and the herd is
	# what makes it worth having beside the plant one: the animal is READY and the people are not, so
	# nothing about the source explains the missing control. 🐄 Corral renders NOT AT ALL — the reason
	# it would carry ("Your people know Penning 35% — ♻ hunt a tamed herd to learn it") is the lesson
	# the aside is already stating live, and its remedy names the very hunt this sheet is composing.
	# What DOES render is the ◎ Pastoral DONE label for the rung this herd has climbed, which is what
	# keeps the absence specific rather than the whole control family having vanished.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": CORRAL_GATE_PENNING,
	}])
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.corral_locked_herd_fixture())
	h._compose_herd(HerdFx.corral_locked_herd_fixture())
	await h._settle()
	await h._save("herd_corral_gated")
	var corral_gated = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_CORRAL)
	# **THE FAILURE THIS CATCHES IS AN OFFER, not a hidden Label.** Suppressing the reason without
	# suppressing the control leaves an unchecked, live `Pen this herd` box on a
	# faction 35% of the way through Penning — a commitment the sim rejects — so the assertion is
	# ABSENCE, and the DONE label below is what proves the sheet did not simply fail to build.
	h._assert_hud("a Corral blocked ONLY on knowledge renders NO improvement control on this sheet",
		corral_gated == null)
	# The whole reason string, so this is safe to ask of the SHEET AT LARGE where the bare word
	# "Penning" is not (it also appears in the top-bar strip and in a hint's craft clause — exactly how
	# the `two_meter_split` assertion below once passed for the wrong reason). Suppressed must mean it
	# appears NOWHERE, including in the note slot beneath a control.
	h._assert_hud("…and the knowledge reason it would have carried appears nowhere on the sheet",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			HudFloraVocab.GATE_REASON_PENNING_KNOWLEDGE_FORMAT % [
				HudFormat.progress_percent(CORRAL_GATE_PENNING),
				FoodIcons.for_floor_zone(SourceForecast.FLOOR_ZONE_PEAK)]))
	# …and the removal is a progression rather than a hole, on this web too: the ASIDE is naming the
	# craft this herd's standing rung teaches — penning — in the same frame, live.
	h._assert_hud("…while the aside still names the lesson being earned, so the rung is not silent",
		Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(
			String(SourceForecast.RUNG_LESSONS[SourceForecast.SOURCE_KIND_HERD][
				SourceForecast.IMPROVEMENT_TAME])))
	# …and the rung it has already climbed reads as the STATE it is, above the one it cannot start.
	h._assert_hud("…beneath the DONE label for the rung this herd has climbed",
		ForageFx.improvement_face(h._hud._drawercompose._compose_sheet, HudConst.LABOR_POLICY_TAME).contains(
			String(HudComposeVocab.IMPROVEMENT_DONE_LABELS[HudConst.LABOR_POLICY_TAME])))

	# State 3c-corral-ungated — the SAME herd once Penning is fully known. Nothing about the ANIMAL
	# changed, so if the box does not go live the gate is keyed to something it should not be.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 1.0,
	}])
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.corral_locked_herd_fixture())
	h._compose_herd(HerdFx.corral_locked_herd_fixture())
	await h._settle()
	await h._save("herd_corral_ungated")
	var corral_ungated = ForageFx.find_improvement_control(h._hud._drawercompose._compose_sheet,
		SourceForecast.IMPROVEMENT_CORRAL)
	h._assert_hud("Penning alone unlocks Corral — the same herd now offers it as a live choice",
		corral_ungated is CheckBox and not (corral_ungated as CheckBox).disabled
		and not (corral_ungated as CheckBox).button_pressed)
	# **AND THE ASIDE STOPS TEACHING IT, in the same breath.** This is the animal half of the A/B the
	# plant web runs on `forage_lesson_known`: nothing about the herd moved between the two frames, so
	# the gated one above naming `penning` and this one naming nothing is the whole claim that the line
	# reads the FACTION and not just the rung. No build is composed here, so no half of the sentence
	# survives — a line, not a blank.
	h._assert_hud("…and the aside stops teaching a craft the faction has finished learning",
		Readout.teaching_line(h._hud._drawercompose._compose_sheet) == "")
	# **AN UNTICKED BOX HAS TO BE THERE TO BE TICKED.** Godot's stock `unchecked` art is a FILLED
	# near-black square drawn for a LIGHT surface, so on this console it reserved its width and painted
	# nothing: an offer that read as a line of prose with no control on it. Measure the thing that was
	# actually wrong — CONTRAST against the panel — rather than the presence of an override: the first
	# cut of the fix set `icon_normal_color`, which a CheckBox ignores entirely, and an override-shaped
	# assertion would have passed on it.
	h._assert_hud("an offered rung's box is VISIBLE against the panel, not black on black",
		_checkbox_indicator_contrast(corral_ungated, "unchecked")
		>= CHECKBOX_INDICATOR_MIN_CONTRAST)
	# The ticked half needs a DIFFERENT question asked of it: the stock `checked` art is a light chip and
	# already clears the contrast bar, so re-using that measure here would pass with the fix removed —
	# a vacuous assertion. What the designer asked for is that a running build be unmistakable, so pin
	# the HUE: ticked reads in `SIGNAL`, the colour this HUD uses for nothing but live state.
	h._assert_hud("…and the ticked art reads in SIGNAL, so a running build is unmistakably running",
		_checkbox_tick_colour_gap(corral_ungated) <= CHECKBOX_TICK_COLOUR_TOLERANCE)

	# State 3d-corral — a fully-domesticated, not-yet-penned herd with the pen 40% built: 🐄 Corral is
	# ENABLED and selected, the forecast states the deal ("Preparing: +0.23 /turn → then +1.50 /turn
	# − 0.34 feed", the `corral` ceiling row → corral_yield minus the projected pen_upkeep, stepper capped at the
	# 1 keeper a managed source needs), and the drawer carries the "Corral: Building 40%" row — the
	# herd twin of the tile's "Cultivation N%". The picker's 🐄 Corral button wears the `→ +1.50/turn`
	# PAYOFF (corral_yield), above ◎ Tame's `→ +1.20/turn` and Sustain's `up to +0.90/turn`.
	#
	# `pen_upkeep` is the feed this pen WOULD demand once built — the sim projects it at the herd's
	# current biomass (on the same basis as `corral_yield`), so the pre-commit row subtracts the real
	# running cost rather than saying "before feed".
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.corral_ready_herd_fixture())
	h._compose_herd(HerdFx.corral_ready_herd_fixture(), Spine.COMPOSE_COUNT_UNSET, ForageFx.COMPOSE_FLOOR_UNSET, "corral")
	await h._settle()
	await h._save("herd_corral")
	# **THE COMMIT VERB FOLLOWS THE CREW NOUN ON THIS WEB TOO, and it did not.** `_herd_crew_noun` has
	# always resolved Hunters/Herders off the standing rung, and the eyebrow, the stepper and the
	# drawer's open button all followed — but the commit button was HARD-CODED, so an `ASSIGN HERDERS`
	# sheet over a `Herders` stepper committed with `Hunt Here`. Reported from play. Asserted with the
	# stepper beside it, because the claim is that the two agree, not merely that the button changed.
	h._assert_hud("a managed herd's sheet is staffed by HERDERS",
		Readout.crew_row_label(h._hud._drawercompose._compose_sheet)
			== HudComposeVocab.HERD_CREW_LABEL.to_upper())
	h._assert_hud("…and commits with their own verb, not the hunt one",
		Q.compose_commit_button(h._hud._drawercompose._compose_sheet) != null
			and Q.compose_commit_button(h._hud._drawercompose._compose_sheet).text
				== HudComposeVocab.ASSIGN_LOCAL_HERD_BUTTON)

	# State 3d-corral-under-herded — the HERDER-DEFICIT cap fix. A composing-Corral herd needs 2 herders
	# every turn to hold its tameness, but the Corral rung's take/prepare max-useful is 1. The compose
	# stepper's cap must be max(1, herders_needed 2) = 2, so the `+` reaches 2 and the maintenance crew is
	# staffable (an under-herded corral is otherwise an unwinnable trap). The drawer's Herders row reads
	# "1 / 2 — under-herded" and the shed consequence line ("animals are drifting off. Staff all 2 herders
	# to hold the herd." — NEVER the retired "tameness slipping" copy) names 2 — the SAME herders_needed
	# the cap uses.
	# Auto-max (a policy click arms the compose hunt autofill) fills the crew to the corrected cap of 2.
	#
	# THE STAFFING IS DIALED DOWN FOR THIS STATE ONLY. The drawer's "Herders A / N" row reads the band's
	# ACTUAL hunt assignment on this herd (`assigned_herders_for`), and the reference band staffs 4 — which
	# renders a FULLY-herded corral and hides the very shortfall the cap floor exists to fix. The row and
	# the cap floor have to DISAGREE for the deficit to be visible, so this band staffs 1 against the herd's
	# requirement of 2; `BandFx.band_fixture()` is restored immediately after the save, since `herd_corral_depleted`
	# and every state downstream document the reference band's 4.
	var under_herded_band := BandFx.band_fixture().duplicate(true)
	for entry in under_herded_band["labor_assignments"]:
		if entry is Dictionary and String((entry as Dictionary).get("kind", "")) == "hunt":
			(entry as Dictionary)["workers"] = UNDER_HERDED_CORRAL_HERDERS_STAFFED
	h._hud._band_labor._player_band = under_herded_band
	h._hud._compose.reset_hunt_source()
	h._show_herd(_under_herded_corral_fixture())
	# The three-line auto-max idiom (`herd_hunt_automax`): open once so the rung is composed, arm the
	# one-shot, then re-open so it is consumed against the COMPOSED Corral. Arming before the first open
	# spends the one-shot on the re-seeded rung instead, and dialing an explicit count would overwrite
	# whatever auto-max produced — the frame must show auto-max REACHING the cap, not advertising it.
	h._compose_herd(_under_herded_corral_fixture(), Spine.COMPOSE_COUNT_UNSET, ForageFx.COMPOSE_FLOOR_UNSET, "corral")
	h._hud._compose.arm_hunt_autofill()
	h._compose_herd(_under_herded_corral_fixture())
	await h._settle()
	await h._save("herd_corral_under_herded")
	# The claim is REACHABILITY, and it is the herder FLOOR that guarantees it: auto-max fills to the
	# cap, and the cap is never below the crew the sim demands. It is no longer EQUAL to that crew,
	# because the cap is now the SELECTED STANCE's take-useful raised to the floor (issue #442) rather
	# than a build verb's 1-worker prep count raised to it — a crew building a pen still hunts, and the
	# stance says how hard. Asserting equality would pin the old overload's arithmetic.
	h._assert_hud("auto-max fills the corral crew to at least the herder deficit, proving it is reachable",
		h._hud._compose.hunt_count() >= UNDER_HERDED_CORRAL_HERDERS_NEEDED)
	# Restore the reference band (4 herders on game_deer_07) for everything downstream.
	h._hud._band_labor._player_band = BandFx.band_fixture()

	# State 3d-corral-depleted — the SAME rung on a herd BELOW the pen's escapement point (K/2). The
	# managed harvest takes only the biomass standing above that point, so the payoff is honestly
	# +0.00 /turn while the feed is still 0.14 — a pure loss. The face must SHOW both zeros and carry
	# the WARN "⚠ Too depleted to pen" note, never suppress the zero as if it were missing data.
	h._hud._compose.reset_hunt_source()
	h._show_herd(_depleted_corral_herd_fixture())
	h._compose_herd(_depleted_corral_herd_fixture(), Spine.COMPOSE_COUNT_UNSET, ForageFx.COMPOSE_FLOOR_UNSET, "corral")
	await h._settle()
	await h._save("herd_corral_depleted")
	# **THE WARNING HAS OUTLIVED TWO HOMES FOR THE ZERO IT EXPLAINS** — a deal LINE, then the control's
	# face, now the readout's payoff row — and has stayed on the improvement control's own note slot
	# throughout (the slot the paused-build line uses), because it is a warning about the RUNG. This
	# frame is the only one that produces it, so it is where a silent loss would show. The zero is
	# asserted beside it: a note over a suppressed payoff warns about a number the player cannot see.
	h._assert_hud("a pen that would pay nothing says so, in the note slot under its own box",
		Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			HudComposeVocab.IMPROVEMENT_DEAL_DEPLETED_NOTE))
	h._assert_hud("…beside a readout row that still states the zero payoff and the feed it would eat",
		Readout.improvement_deal_text(h._hud._drawercompose._compose_sheet)
			.contains(SourceForecast.PICKER_FOOD_PRODUCT_FORMAT
				% SourceForecast.format_magnitude(0.0)))
	h._assert_hud("…and the box's own face carries no payoff to state it a second time",
		not ForageFx.improvement_face(h._hud._drawercompose._compose_sheet,
			SourceForecast.IMPROVEMENT_CORRAL).contains(ForageFx.IMPROVEMENT_PAYOFF_NEEDLE))

	# ---- THE INTENSIFICATION LADDER, slice 6b -----------------------------------------------------
	# THE TWO-METER SPLIT (docs/plan_intensification_ladder.md §4.1) — the headline of this slice, and
	# the frame it is judged on. Two meters advance from one action and they are DIFFERENT KINDS of
	# thing; this state puts both on screen at once so the distinction can actually be seen:
	#   • FACTION KNOWLEDGE — the top-bar strip, prefixed "⚒ Your people know:". Herding ✔ known,
	#     Penning still learning at 45%. This is your PEOPLE's craft: faction-wide, permanent, earned
	#     by practice. It appears NOWHERE else — never in the drawer below.
	#   • PER-SOURCE PROGRESS — this herd's own "Husbandry: 🐄 Domesticated" row, down in its drawer.
	#     Local to THIS animal, and it decays if abandoned.
	# The bridge between them is the readout's ASIDE — the teaching line, which names the craft this
	# herd's standing rung is earning, live, on the sheet where the work is composed.
	#
	# **THAT BRIDGE USED TO BE THE GATED 🐄 CORRAL'S REASON LINE, and it is gone from this sheet.** A
	# knowledge-only gate renders no control here at all, so the sheet no longer carries a knowledge
	# PERCENT anywhere — that number lives in the top-bar strip alone, which is the first assertion
	# below. The reason string itself is unchanged and still rendered by every other surface that shows
	# a gate (`RungGates.hunt_gates` is untouched); what is pinned here now is the aside.
	#
	# **THE HERD IS FULLY TAMED, and it has to be** (issue #442). The frame staged a 40%-tamed herd
	# while the bridge line was a rung of the policy picker; the improvement control offers the NEXT
	# rung, so on a part-tamed herd it offers 🐾 Tame and no gate reason renders at all — the frame kept
	# its subject in its comment and lost it on screen. Retiring Tame is what makes Corral the rung on
	# offer, and a Corral gated on Penning is the only shape this bridge has. It also sharpens the
	# contrast the frame exists for: the ANIMAL is ready and the PEOPLE are not, so the two meters are
	# unmistakably about different things.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.12,
		"penning": TWO_METER_PENNING,
	}])
	h._hud._compose.reset_hunt_source()
	h._show_herd(HerdFx.fully_tamed_herd_fixture())
	h._compose_herd(HerdFx.fully_tamed_herd_fixture())
	await h._settle()
	await h._save("two_meter_split")
	# THE TWO-METER SPLIT'S OWN INVARIANT, asked of the three SPECIFIC controls that carry it. The pair
	# that stood here searched the WHOLE SHEET for the word "Penning" and was matching the Sustain
	# HINT's craft clause, not the gate reason it claimed to test — the shape this sweep exists to
	# remove. Each half below names its own surface, so a regression says which one moved.
	# **THE FACTION HALF IS ASKED OF THE MODEL NOW.** It read the top-bar `⚒ Your people know:` strip,
	# which is retired with the top-right block (issue #450); the craft's surface is the Band/City
	# dock's KNOWLEDGE tab, which this harness does not instantiate. What the split actually claims is
	# that the craft is held FACTION-SCOPED while the animal's own progress is held per-herd, and the
	# cache is where the faction scope now lives — so the pairing below is unweakened, and the leak
	# half (the third assertion) never depended on a strip at all.
	# By EQUALITY against the fixture's own PART-LEARNED value, which is the split itself: the faction
	# is 45% of the way to Penning while THIS animal is fully tamed, so the two meters cannot be
	# confused for one reading. The strip's own test was mere presence (it renders a track at any
	# progress above zero) — the number is the stronger claim and the one the fixture was built for.
	h._assert_hud("the FACTION's craft is held faction-scoped at %d%%, off any one animal" % int(
			TWO_METER_PENNING * HudConst.PROGRESS_PERCENT_SCALE),
		is_equal_approx(h._hud._topbar.faction_knowledge(HudConst.PLAYER_FACTION_ID,
			HudFloraVocab.KNOWLEDGE_TRACK_PENNING), TWO_METER_PENNING))
	h._assert_hud("…and THIS HERD's own progress lives in its own drawer's Husbandry row",
		Q.has_label_containing(h._hud.occupant_detail,
			DetailFormat.husbandry_label(DetailFormat.HUSBANDRY_PROGRESS_COMPLETE)))
	h._assert_hud("…and no knowledge percent leaks into the drawer, where it would read as a stat of the animal",
		not Q.has_label_containing(h._hud.occupant_detail,
			String(FactionReadouts.KNOWLEDGE_TRACK_LABELS[HudFloraVocab.KNOWLEDGE_TRACK_PENNING])))
	# **THE GATED-CORRAL BRIDGE ASSERTION IS REMOVED, NOT WEAKENED.** It read the gated Corral control's
	# own face — "Your people know Penning 45% — ♻ hunt a tamed herd to learn it" — and the compose
	# sheet renders no control at all for a knowledge-only gate, so its subject no longer occurs here.
	# The suppression itself is pinned on `herd_corral_gated` (absence + the reason nowhere on the
	# sheet); the reason STRING is still `RungGates.hunt_gates`' and still rendered wherever a gate
	# reason is shown outside this sheet.
	#
	# THE BRIDGE, as it now stands — the aside's teaching line, read BY META so neither the hint text
	# nor the top-bar strip can satisfy it. It names the CRAFT and not the percent, which is the honest
	# claim: the sheet is where the lesson is being earned, the strip is where its progress is read.
	h._assert_hud("…and the aside's teaching line is the bridge: the craft this herd's rung teaches",
		Readout.teaching_line(h._hud._drawercompose._compose_sheet).contains(
			String(SourceForecast.RUNG_LESSONS[SourceForecast.SOURCE_KIND_HERD][
				SourceForecast.IMPROVEMENT_TAME])))

	# State 6b-tame — the ◎ Tame affordance itself: a 6th option in the LOCAL hunt picker, beside
	# Sustain/Surplus/Deplete/Eradicate/Corral, ENABLED (Herding is known) and selected on a
	# pen-ceiling herd that is only 40% tamed. Now that the sim exports `pastoralYield`, Tame renders
	# the SAME dip→payoff pair as its three siblings: "Preparing: +<dip> → then +1.20 /turn" (dip from
	# `hunt_policy_ceilings["tame"]`, payoff = pastoral_yield, no feed term — Tame has no running cost).
	# Its picker button wears the `→ +1.20/turn` payoff, above Sustain's `up to +0.90/turn`.
	await h._save("herd_tame")

	# State 6b-tame-stalled — the "why isn't my Tame progressing?" hint. Taming accrues ONLY while the
	# herd is Thriving, but is deliberately NOT gated on it (a herd's phase swings as you hunt it), so
	# the sim just PAUSES the meter. Silence here would recreate exactly the hidden-rule problem this
	# arc exists to kill, so the drawer says it: what stopped, why, that progress is NOT lost, and the
	# remedy (ease off — the opposite of "work harder").
	h._hud._compose.reset_hunt_source()
	h._show_herd(_taming_stalled_herd_fixture())
	h._compose_herd(_taming_stalled_herd_fixture(), Spine.COMPOSE_COUNT_UNSET, ForageFx.COMPOSE_FLOOR_UNSET, "tame")
	await h._settle()
	await h._save("herd_tame_stalled")

	# TAMING-STARTUP-LAG GUARD — composing an INVESTMENT rung (Tame) on a still-WILD herd must offer the
	# ownership-INDEPENDENT would-be herder crew, not the 1-worker Tame-prep count. A wild herd's
	# `herders_needed` is ownership-gated to 0, so the take/prepare max-useful (1) used to pin the cap at 1;
	# the player could staff only 1, the herd became owned next turn needing 3, and read under-herded. The
	# fix floors the LOCAL-hunt cap on `herders_needed_if_managed` (3) for investment rungs only.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	# A band with idle workers comfortably above both caps (Tame 30, Sustain 7), so the stepper is bound by
	# USEFULNESS (the "max N useful here" note), not by the idle-labor ceiling (a different note entirely).
	var tame_cap_band := BandFx.band_fixture()
	tame_cap_band["idle_workers"] = TAME_CAP_WOULD_BE_HERDERS * 2
	tame_cap_band["working_age"] = TAME_CAP_WOULD_BE_HERDERS * 3
	h._hud._band_labor._player_band = tame_cap_band
	h._hud._band_labor._player_bands = [tame_cap_band]
	h._hud._compose.reset_hunt_source()
	h._show_herd(_tame_worker_cap_herd_fixture())
	# Tame is DIALED IN through `_compose_herd`, which survives the source-change re-seed — see its doc.
	h._compose_herd(_tame_worker_cap_herd_fixture(), Spine.COMPOSE_COUNT_UNSET, ForageFx.COMPOSE_FLOOR_UNSET, "tame")
	await h._settle()
	await h._save("herd_tame_worker_cap")
	# Tame floors the cap on the would-be crew (10), NOT the Tame-prep useful (1): the sheet's max-useful
	# note reads "max 10 workers useful here". Pre-fix it read "max 1 worker useful here" (floored on the
	# ownership-gated herders_needed 0).
	h._assert_hud("Tame offers the full would-be herder crew (max %d), not the 1-worker prep count"
		% TAME_CAP_WOULD_BE_HERDERS,
		Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			"max %d workers useful" % TAME_CAP_WOULD_BE_HERDERS))
	h._assert_hud("…and not the pre-fix 1-worker cap",
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet, "max 1 worker useful"))
	# COMPANION — the EXTRACTIVE Sustain rung manages nothing, so it needs no herders: its cap is
	# take-useful only (Sustain 1.50 ÷ 0.30 = 5), and the would-be crew (3) must NOT leak into it.
	h._hud._compose.reset_hunt_source()
	h._show_herd(_tame_worker_cap_herd_fixture())
	h._compose_herd(_tame_worker_cap_herd_fixture(), Spine.COMPOSE_COUNT_UNSET, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("herd_tame_worker_cap_sustain")
	h._assert_hud("Sustain caps on its own take-useful (max 7), floored at 0",
		Q.has_label_containing(h._hud._drawercompose._compose_sheet, "max 7 workers useful"))
	h._assert_hud("…the would-be herder crew (%d) does not leak into an extractive rung"
		% TAME_CAP_WOULD_BE_HERDERS,
		not Q.has_label_containing(h._hud._drawercompose._compose_sheet,
			"max %d workers useful" % TAME_CAP_WOULD_BE_HERDERS))
