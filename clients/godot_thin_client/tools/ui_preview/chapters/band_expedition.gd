extends RefCounted

## The band drawer and the expedition panels.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")
const WorldFx := preload("res://tools/ui_preview/fixtures_world.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# The pen-keeping band's entity id — its own, so its Food disclosure key (`food:<entity>`) doesn't
# collide with the reference band's.
const PEN_KEEPER_BAND_ENTITY := 906

# The reference band (`BandFx.band_fixture()`, entity 904) disclosure keys — the `[url]` meta its Food /
# Morale rows carry, i.e. what `DetailFormat.breakdown_key` builds for it.
const BAND_DISCLOSURE_FOOD := "food:904"

const BAND_DISCLOSURE_MORALE := "morale:904"

const BAND_DISCLOSURE_GROWTH := "growth:904"

# The collapsed-growth band is `_concerning_food_band_fixture`'s entity (905), not 904.
const BAND_DISCLOSURE_GROWTH_COLLAPSED := "growth:905"

# ---- THE FILL TARGET's fixture (`docs/plan_hunt_through_combat.md` §5.2) ------------------------
# **THE RATE HAS TO DIVIDE EXACTLY, and that is the whole design of this fixture.** The pre-launch
# turn count for a target is a PROPORTION on the sim's own `animals_taken ÷ turns_to_fill`, so on a
# row whose quotient is not whole every assertion below would have to be a range — and a range check
# passes on an arithmetic that is merely close, which is precisely the divergence these states exist
# to bound. A party of `w` lands `w × FILL_TARGET_RAID_RATE / FILL_TARGET_RAID_PARTY` animals a turn
# for `FILL_TARGET_RAID_TURNS` turns, so the party of 4 the frames compose lands exactly 20 a turn and
# the party of 2 exactly 10 — which is what makes "halving the party doubles the wait" an equality.
const FILL_TARGET_RAID_PARTY := 4

const FILL_TARGET_RAID_TURNS := 6

const FILL_TARGET_RAID_RATE := 20

const FILL_TARGET_RAID_ANIMALS := FILL_TARGET_RAID_TURNS * FILL_TARGET_RAID_RATE

# The animals-taken row keeps RISING with party size, deliberately: a plateau would let
# `SourceForecast.expedition_useful_cap` cap the stepper below the party these states compose, and
# every turn count would then be answered for a row the frames never render.
const FILL_TARGET_RAID_ANIMALS_ROW := [30, 60, 90, 120, 150, 180, 210, 240]

const FILL_TARGET_RAID_TURNS_ROW := [6, 6, 6, 6, 6, 6, 6, 6]

# The LAUNCHED party's own orders (`expedition_hunt_targeted`): the whole animals it is waiting for,
# and the species those animals are — the drawer names the target in the QUARRY's units, so the
# assertion has to know both halves to build a needle no other row can satisfy.
const TARGETED_PARTY_FILL_TARGET := 50

const TARGETED_PARTY_QUARRY := "Red Deer"

# The detail row's KEY, which is what `Readout.detail_excerpt` seeks: the leading half of
# `SourceForecast.FILL_TARGET_ORDERS_FORMAT`, restated here only because a `const` cannot split one.
# A reworded row does not pass quietly — the excerpt answers `DETAIL_EXCERPT_ABSENT` and both
# assertions below fail naming the row they could not find.
const FILL_TARGET_DETAIL_KEY := "Fill target"

## Find a Button by its face anywhere under `root` — the harness presses the REAL control the player
## presses, so an assertion covers the wiring and not just the handler it would have called.
## Drive a Food/Morale disclosure the way a CLICK does: emit `meta_clicked` on the live drawer label
## with the very `[url]` meta its own text carries, so the bound handler + anchor run exactly as they
## do in the game. Toggling: a second call on the same key dismisses the popover.
func _click_disclosure(key: String) -> void:
	var meta := HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + key
	var label = _find_meta_label(h._hud, meta)
	if label == null:
		push_warning("ui_preview: no detail label offering '%s' — disclosure not rendered?" % meta)
		return
	label.meta_clicked.emit(meta)

func _find_meta_label(node: Node, meta: String) -> RichTextLabel:
	if node is RichTextLabel and (node as RichTextLabel).text.contains("[url=%s]" % meta):
		return node
	for child in node.get_children():
		var found := _find_meta_label(child, meta)
		if found != null:
			return found
	return null

## A NON-player band (faction 1): what a rival's cohort actually looks like on the wire — an identity,
## a size, a position, and nothing of ours to read (no morale/output/labor/flow fields). Backs the
## `band_foreign` state, which exists to prove the drawer doesn't collapse to an empty card now that
## the identity rows moved into the roster row.
func _foreign_band_fixture() -> Dictionary:
	return {
		"id": "Ashen Kin",
		"size": 96,
		"entity": 977,
		"faction": 1,
		"pos": [71, 18],
		"current_x": 71,
		"current_y": 18,
		"activity": "forage",
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"tile_info": {
			"x": 71, "y": 18,
			"terrain_label": "Prairie Steppe",
			"visibility_state": "active",
		},
	}

## A band that KEEPS A CORRAL: the third term of the food ledger. Its one keeper works the penned
## Red Deer herd (the sim pays the pen's GROSS managed yield, 5.40), and the herd eats 1.74/turn off
## the band's larder — `pen_feed_upkeep`, exported by the sim (`PopulationCohortState.penFeedUpkeep`)
## precisely so the client never has to sum it. Numbers are the design doc's measured Red Deer pen at
## its escapement operating point (B* = K/2): gross 5.40, feed 1.74, net 3.66.
func _pen_keeper_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["entity"] = PEN_KEEPER_BAND_ENTITY
	band["id"] = "Band 4"
	band["turns_of_food"] = 22.0
	band["food_income"] = 5.88          # forage 0.48 + the pen's gross 5.40
	band["food_consumption"] = 1.15     # the PEOPLE's meals
	band["pen_feed_upkeep"] = 1.74      # the ANIMALS' feed — a debit in neither row above
	band["fodder_store"] = 12.4         # the band's HAY larder (Flora roster F3) — feeds the pen
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "floor": 0.5, "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1},
		# A managed source: one keeper, take == sustainable (escapement); Corral is managed, so the
		# sim-answered `overdraws` is false → no ⚠ and no overstaff note.
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5, "improvement": "corral", "target_x": 70, "target_y": 17, "actual_yield": 5.40, "sustainable_yield": 5.40, "workers_needed": 1, "overdraws": false},
		{"kind": "scout", "workers": 2},
	]
	return band

## The SAME pen, underfed: the band's income has collapsed (a shrinking herd yields less — gross
## 1.90) and it could hand over only 0.70 of the 1.74 the herd demanded. `pen_feed_upkeep` is what
## was actually PAID (the sim's `LocalStore::take` partial-payment primitive), so the ledger still
## balances against the larder; the herd carries the shortfall as `pen_fed_fraction` 0.40.
## Net = 1.32 − 1.15 − 0.70 = −0.53 — the death spiral the readout exists to make visible: the herd
## shrinks, so it yields less, so there is less to feed it with.
func _starving_pen_band_fixture() -> Dictionary:
	var band := _pen_keeper_band_fixture()
	band["turns_of_food"] = 3.0
	band["food_income"] = 1.32          # forage 0.48 + the shrunken pen's 0.84
	band["pen_feed_upkeep"] = 0.70      # PAID, not demanded — the herd starves for the difference
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "floor": 0.5, "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1, "overdraws": false},
		# BOTH PRODUCTS on the drawer's standing summary (issue #337): the hide sells beside the meat,
		# so the one-line summary must read `+0.84 /turn · ⇄ +0.12` — food leading, trade only because
		# it is non-zero. It comes from the SAME `SourceForecast.source_yield_readout` the Band panel's
		# rows use, so the two surfaces cannot state different products for one assignment.
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5, "improvement": "corral", "target_x": 70, "target_y": 17, "actual_yield": 0.84, "sustainable_yield": 0.84, "workers_needed": 1, "overdraws": false, "trade_yield": 0.12, "realized_trade_yield": 0.12},
		{"kind": "scout", "workers": 2},
	]
	return band

## A CONCERNING food state: net-negative flow (income 0.30 < consumption 0.95 → net −0.65) and a
## low larder runway (4 days). Both trip `DetailFormat.food_is_concerning`, so the category breakdown auto-shows
## under a red net figure without any click.
## The band the growth model exists for: its income has collapsed and it is now eating short off a
## nearly-empty larder. All three factors are off neutral at once, which is what makes this the frame
## that proves the breakdown MULTIPLIES out to its headline (0.60 × 1.05 × 0.25 = 0.16 → "16% of
## normal", below `fertility.critical` → a RED row under a WARN caret).
##
## It is derived from the concerning-food band rather than being that band: a band four turns from
## empty is still eating FULL today, so a `hunger` below 1.0 there would be an incoherent fixture.
func _collapsed_growth_band_fixture() -> Dictionary:
	var band := _concerning_food_band_fixture()
	band["turns_of_food"] = 1.0
	band["stores"] = {"provisions": 0.6}
	band["fertility_hunger"] = 0.60    # ate 60% of what it wanted
	band["fertility_reserve"] = 1.05   # almost nothing banked
	band["fertility_trend"] = 0.25     # income gone — the shipped deficit floor
	return band

## A band whose fertility has NOT been projected — a rehydrated cohort, before the next tick. The
## sim publishes the all-zero not-projected sentinel (a computed `reserve` is ≥ 1 by construction, so
## a zero reserve cannot be a real reading), and the drawer must answer with NO Growth row rather
## than a fabricated 0%.
func _unprojected_growth_band_fixture() -> Dictionary:
	var fixture := BandFx.band_fixture()
	fixture["fertility_hunger"] = 0.0
	fixture["fertility_reserve"] = 0.0
	fixture["fertility_trend"] = 0.0
	return fixture

func _concerning_food_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["entity"] = 905
	band["id"] = "Band 3"
	band["turns_of_food"] = 4.0
	band["food_income"] = 0.30
	band["food_consumption"] = 0.95
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "target_x": 71, "target_y": 18, "actual_yield": 0.15, "sustainable_yield": 0.15, "overdraws": false},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "floor": 0.5, "target_x": 70, "target_y": 17, "actual_yield": 0.15, "sustainable_yield": 0.20, "overdraws": false},
		{"kind": "scout", "workers": 2},
	]
	return band

## A hunting expedition (PR 2, docs/plan_exploration_and_sites.md §2b): a detached party following a
## migratory herd. mission "hunt" + a target herd + carried food (its own kills). The drawer renders
## the hunt readout (target herd + carried food + phase) + Recall/Move.
func _hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 1",
		"size": 5,
		"entity": 7101,
		"faction": 0,
		"pos": [64, 22],
		"turns_of_food": 4.0,
		# Carried 8 of a 16 carry cap → "Carried 8 / 16".
		"stores": {"provisions": 8.0},
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_07",
		"expedition_hunt_policy": "surplus",
		"expedition_carry_cap": 16.0,
		# In-flight next-delivery forecast: 12 food arrives in 6 turns. Surplus is one-shot, so the
		# party folds home after delivering → not recurring (no ↻).
		"expedition_eta_turns": 6,
		"expedition_projected_delivery": 12.0,
		"expedition_recurring": false,
		"tile_info": {
			"x": 64, "y": 22,
			"terrain_label": "Prairie Steppe",
			"tags_text": "Fertile",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A well-fed band whose morale has collapsed on a harsh tile: food is not limited
## (∞) but morale 0.22 sits below the critical threshold, so the Morale row reads red.
func _low_morale_band_fixture() -> Dictionary:
	var fixture := BandFx.band_fixture()
	fixture["id"] = "Band 5"
	fixture["entity"] = 905
	fixture["turns_of_food"] = 999.0
	fixture["stores"] = {"provisions": 260.0}
	fixture["morale"] = 0.22
	# Falling morale driven by the harsh cavern terrain: the drawer shows
	# "Morale: 22% ▼ — harsh terrain (Karst Cavern Mouth)".
	fixture["morale_delta"] = -0.010
	fixture["morale_cause"] = 1  # Terrain
	# Civilization Wellbeing (docs/plan_civ_wellbeing.md): discontent drags Output to 56%
	# (< critical → red), and the four signed Layer-1 contributions (sum = morale_delta)
	# drive the itemized breakdown. People are relocating (last_emigrated > 0).
	fixture["output_multiplier"] = 0.56
	fixture["discontent_fraction"] = 0.44
	fixture["last_emigrated"] = 6
	fixture["morale_settling"] = 0.010   # +1.0%  settling (positive base growth)
	fixture["morale_terrain"] = -0.012   # −1.2%  harsh terrain
	fixture["morale_climate"] = -0.008   # −0.8%  harsh climate
	fixture["morale_unrest"] = 0.0       # below epsilon → row omitted
	# Its GROWTH, by contrast, is fine — fed, well-stocked, income covering the drain (1.0 × 1.50 ×
	# 1.0 = 150% of normal, so only the reserve row lists). That contrast is the point of having it
	# here: births are morale-INDEPENDENT in this model, so a miserable band on harsh ground must not
	# read as a band that has stopped breeding.
	fixture["fertility_hunger"] = 1.0
	fixture["fertility_reserve"] = 1.50
	fixture["fertility_trend"] = 1.0
	fixture["tile_info"] = {
		"x": 44, "y": 61,
		"terrain_label": "Karst Cavern Mouth",
		"tags_text": "Subsurface, Harsh",
		"visibility_state": "active",
		# Cavern habitability (~0.0825) lands in the Harsh band → amber Tile-card row.
		"habitability": 0.0825,
		# High-latitude cold ~-2° → "Polar" climate band (neutral Tile-card row).
		"temperature": -2.0,
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

## Prior-snapshot band sizes so the "losing population" alert has a baseline to
## compare against (Band Ash drops 90 → 78 in the live fixture below).
func _band_alert_baseline() -> Array:
	return [
		{"faction": 0, "entity": 101, "size": 60, "turns_of_food": 12.0, "activity": "harvest", "current_x": 71, "current_y": 18},
		{"faction": 0, "entity": 102, "size": 90, "turns_of_food": 999.0, "activity": "hunt", "current_x": 40, "current_y": 22},
		{"faction": 0, "entity": 103, "size": 45, "turns_of_food": 999.0, "activity": "harvest", "current_x": 12, "current_y": 9},
	]

func _band_alert_fixture() -> Array:
	return [
		# Starving: 3 turns of food (< critical) → red alert.
		{"faction": 0, "entity": 101, "size": 60, "turns_of_food": 3.0, "activity": "harvest", "current_x": 71, "current_y": 18,
			"harvest": {"band_label": "Band Fen"}},
		# Losing population to relocation: size 90 → 78, well-fed (∞) but discontented and
		# 12 people emigrated last turn → amber alert "losing population — people leaving".
		{"faction": 0, "entity": 102, "size": 78, "turns_of_food": 999.0, "morale": 0.30, "morale_cause": 1, "last_emigrated": 12, "activity": "hunt", "current_x": 40, "current_y": 22,
			"harvest": {"band_label": "Band Ash"}},
		# Idle labor: quiet low-priority alert.
		{"faction": 0, "entity": 103, "size": 45, "turns_of_food": 999.0, "activity": "idle", "current_x": 12, "current_y": 9},
	]

## Predators Phase 3 — a band UNDER an active raid, both legibility surfaces lit at once:
##   • `raid_radius` 3 (the sim's echoed `predators.raid_radius`) + a VISIBLE camp-menacing predator
##     placed one tile off the band's [71,18] in the world-herd list (`_raiding_predator_herd_fixture`)
##     → the Warrior card's live crimson "⚠ Predator nearby — N on guard" alert.
##   • `raid_forfeit` 1.20 (`PopulationCohortState.raidForfeit`, food lost to raids THIS turn) → the
##     "⚔ Lost to raids −1.20" food-ledger row and a net dragged negative the turn the raid landed.
## Reuses entity 904, so `BAND_DISCLOSURE_FOOD` opens its ledger popover.
func _raided_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["raid_radius"] = 3
	band["raid_forfeit"] = 1.20
	return band

## The VISIBLE predator the raided band can see: one tile off its [71,18] (hex distance 1, well inside
## `raid_radius` 3). `prey_sense_radius > 0` marks it a predator and `attack × aggression > 0` marks it a
## camp menace — the exact THREAT product `_band_predator_threat_present` (and the map overlay) key off.
func _raiding_predator_herd_fixture() -> Dictionary:
	return {
		"id": "predator_wolf_02",
		"species": "Grey Wolf Pack",
		"x": 70, "y": 18,
		"prey_sense_radius": 4,
		"attack": 5.0,
		"aggression": 0.7,
		"food_per_animal": 0.0,
	}

func run(harness) -> void:
	h = harness

	# State 0-fresh-profile — THE SHIPPED DEFAULT DOCK LAYOUT, rendered on the path a real player
	# travels and nothing else: prefs section erased above, HUD freshly instantiated, and the first
	# real terrain legend arriving from MapView exactly as `Main._on_overlay_legend_changed` pushes
	# it. NOTHING may call `set_suppressed` / `toggle_legend` / `toggle_victory` before this point —
	# that is the whole value of the state. The right dock must be EMPTY of both reference cards:
	# no Terrain Types, no Victory. This state is FIRST on purpose, so no later state can leak into
	# it, and it is the regression guard for "the legend is visible by default in the real game".
	h._hud.update_overlay_legend(TileFx.terrain_legend_fixture())
	h._hud.update_victory_state(WorldFx.victory_state_fixture())
	await h._settle()
	await h._save("dock_fresh_profile_default")
	h._assert_hud("fresh profile: Terrain Types legend is hidden",
		not h._hud.terrain_legend_panel.visible)
	h._assert_hud("fresh profile: Victory panel is hidden",
		not h._hud.victory_panel.visible)

	# State 1 — a single band selected (GOOD state): the Occupants roster + the labor allocation panel.
	# Food + Morale are healthy, so BOTH summary rows read collapsed with a ▸ disclosure caret
	# (`Food ▸ …` / `Morale 82% ▸`) — click-to-expand, nothing auto-shown.
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	await h._save("band")

	# State 1-foreign — a NON-player band selected. The drawer is the same `unit_summary_lines` host,
	# but almost none of it applies: morale/output/breakdowns are player-only (someone else's band is
	# not ours to read), there is no allocation panel, and the identity rows (name, size) now live in
	# the roster row above. So the check this state exists for: does the drawer collapse to an empty
	# card once `Unit`/`Size` are gone? (It keeps the bare larder Food line + Position.)
	h._hud.show_unit_selection(_foreign_band_fixture())
	await h._settle()
	await h._save("band_foreign")

	# State 1-forage-policy — the forage allocation row carries a policy tag like Hunt does. This band
	# forages on Deplete policy, which the sim gathers past the patch's regrowth: the sim-answered
	# `overdraws` flag is true, so the row reads `Forage (71, 18) [deplete] +0.62 /turn ⚠` (amber
	# over-forage flag). The default `band` state above shows the [sustain] tag with overdraws=false.
	var forage_policy_band := BandFx.band_fixture()
	forage_policy_band["labor_assignments"] = [
		{"kind": "forage", "workers": 6, "target_x": 71, "target_y": 18, "floor": 0.15, "actual_yield": 0.62, "sustainable_yield": 0.40, "overdraws": true},
		{"kind": "scout", "workers": 2},
	]
	h._hud.show_unit_selection(forage_policy_band)
	await h._settle()
	await h._save("forage_policy")

	# State 1-food-a — GOOD food, breakdown OPEN. The breakdown renders in a POPOVER, never inline
	# (growing the row in place is what clipped the Band panel's fixed-height band zone), so the frame
	# shows the indented `Gathered · Hunted · Eaten` rows in a small card under the row. Driven through
	# the REAL path — `meta_clicked` on the live drawer label, the exact signal a click emits.
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	await h._settle()
	await h._save("band_food_expanded")
	_click_disclosure(BAND_DISCLOSURE_FOOD)

	# State 1-morale-a — GOOD morale, breakdown OPEN (same disclosure, same popover): the morale
	# contribution rows.
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	_click_disclosure(BAND_DISCLOSURE_MORALE)
	await h._settle()
	await h._save("band_morale_expanded")
	_click_disclosure(BAND_DISCLOSURE_MORALE)

	# State 1-growth-a — GOOD growth, breakdown OPEN. The band out-breeds its base rate (188% of
	# normal), so the row reads neutral ink and its disclosure names what is HELPING: `▲ ×1.50 larder
	# reserve` / `▲ ×1.25 larder growing`. `hunger` is neutral (the band ate) so its row is omitted
	# rather than listed as a no-op — and the multipliers read down to the headline: 1.50 × 1.25.
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	_click_disclosure(BAND_DISCLOSURE_GROWTH)
	await h._settle()
	await h._save("band_growth_expanded")
	_click_disclosure(BAND_DISCLOSURE_GROWTH)

	# State 1-growth-b — COLLAPSED growth on the concerning band (23% of normal → red row, WARN
	# caret), breakdown OPEN. All three factors are off neutral, so this is the frame that proves the
	# rows multiply out to the headline: 0.60 × 1.50 × 0.25 = 0.23. It is the whole point of the
	# export — the player already had the larder and the Food line, not the attribution.
	h._hud.show_unit_selection(_collapsed_growth_band_fixture())
	await h._settle()
	_click_disclosure(BAND_DISCLOSURE_GROWTH_COLLAPSED)
	await h._settle()
	await h._save("band_growth_collapsed")
	_click_disclosure(BAND_DISCLOSURE_GROWTH_COLLAPSED)

	# State 1-growth-c — a REHYDRATED band: the sim publishes no fertility reading (the factors are
	# derived, not persisted), so there is NO Growth row and no caret at all. The regression this
	# guards is the tempting one — defaulting the factors to 0 and rendering "Growth: 0% of normal",
	# i.e. reading missing data as a total collapse of births.
	h._hud.show_unit_selection(_unprojected_growth_band_fixture())
	await h._settle()
	await h._save("band_growth_unprojected")

	# State 1-food-b — CONCERNING food (net negative + low runway): the Food line net reads red and
	# its caret wears WARN rather than SIGNAL — the breakdown no longer opens itself (a popover that
	# popped on a snapshot would be worse than the clipping it replaced), so the invitation to read it
	# has to be visible on the row.
	h._hud.show_unit_selection(_concerning_food_band_fixture())
	await h._settle()
	await h._save("band_food_concerning")

	# State 1-food-c — a band KEEPING A PEN (docs/plan_corral_managed_population.md). Its ledger has
	# THREE terms, not two: the corral grosses 5.40, the people eat 1.15, and the penned animals eat
	# 1.74 off the same larder (`pen_feed_upkeep`, the sim's own figure — the client never sums the
	# herds' upkeep itself). Net = 5.88 − 1.15 − 1.74 = +2.99, NOT the +4.73 the old two-term ledger
	# would have advertised. Breakdown popover open to show all four rows at once.
	h._hud.show_unit_selection(_pen_keeper_band_fixture())
	await h._settle()
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)
	await h._settle()
	await h._save("band_pen_feed")
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)

	# State 1-food-d — the same pen, STARVING: the band could pay only 0.70 of the 1.74 the herd
	# demands, so the pen feed row shrinks to what was actually paid while the herd wastes away (the
	# herd drawer carries the alarm — see `herd_corral_starving`). Income has fallen with the herd,
	# and the net has gone red.
	h._hud.show_unit_selection(_starving_pen_band_fixture())
	await h._settle()
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)
	await h._settle()
	await h._save("band_pen_starving")
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)

	# State 1b — an all-idle band: no assignments, every worker idle. The allocation panel
	# shows just the Scout + Warrior rows (both at 0) under the Working/Idle header.
	var idle_band := BandFx.band_fixture()
	idle_band["activity"] = "idle"
	idle_band["idle_workers"] = 16
	idle_band["labor_assignments"] = []
	h._hud.show_unit_selection(idle_band)
	await h._settle()
	await h._save("band_idle")

	# State 1p — optimistic pending feedback: a fresh forage assignment (6 workers to a new
	# tile) is in flight before the snapshot confirms. The panel shows an amber "· pending"
	# Forage row and the Idle count reflects it immediately (16 − [5+4+2+2+6=19] clamps to 0).
	# (Seeds the HUD-local pending map directly to mimic a just-issued assign_labor.)
	h._hud._band_labor._pending_labor = {
		904: {
			"turn": 0,
			"assign": {"forage:64,20": {"kind": "forage", "workers": 6, "x": 64, "y": 20, "herd_id": "", "floor": 0.5}},
		}
	}
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	await h._save("band_pending")
	h._hud._band_labor._pending_labor = {}

	# State 1e — a scouting expedition selected in its awaiting-orders phase: the drawer shows the
	# dedicated expedition readout (Mission / Phase "Awaiting orders" / Party / Provisions) and the
	# Recall + Move panel with the amber awaiting callout, instead of the labor-allocation UI.
	h._hud.show_unit_selection(BandFx.expedition_fixture())
	await h._settle()
	await h._save("expedition_panel")

	# State 1f — the same expedition after Recall, now in its returning phase: the panel's button
	# reads "Returning" (disabled) instead of a grayed-out "Recall", and the awaiting callout is
	# gone. The drawer Phase row reads "Returning".
	var returning_expedition := BandFx.expedition_fixture()
	returning_expedition["expedition_phase"] = "returning"
	h._hud.show_unit_selection(returning_expedition)
	await h._settle()
	await h._save("expedition_returning")

	# State 1g — outfit party cap: a resident band with 16 idle workers but a server party cap of 8.
	# The "Send scouting expedition" Party stepper maxes at min(idle 16, cap 8) = 8 — dialed to 8, the
	# + is disabled, confirming the stepper clamps to the CAP, not to idle.
	var cap_band := BandFx.band_fixture()
	cap_band["idle_workers"] = 16
	cap_band["max_expedition_party_size"] = 8
	cap_band["labor_assignments"] = []   # all 16 working-age workers read idle
	h._hud._bandpanel._send_expedition_count = 8
	h._hud.show_unit_selection(cap_band)
	await h._settle()
	await h._save("expedition_outfit_cap")
	h._hud._bandpanel._send_expedition_count = 1   # reset so later states render a fresh party stepper

	# State 1h — a hunting expedition (PR 2, §2b) selected in its Hunting phase: the panel shows the
	# hunt readout (Mission "Hunting expedition", Target herd, Policy, Carried 8 / 16, Party) +
	# Recall/Move.
	h._hud.show_unit_selection(_hunt_expedition_fixture())
	await h._settle()
	await h._save("expedition_hunt_panel")

	# State 1i — a FULL hunt party (carried at the carry ceiling): the Carried row reads "16 / 16 …
	# · FULL" and the Phase is Delivering (it heads home when full).
	var full_hunt := _hunt_expedition_fixture()
	full_hunt["expedition_phase"] = "delivering"
	full_hunt["stores"] = {"provisions": 16.0}
	full_hunt["turns_of_food"] = 8.0
	h._hud.show_unit_selection(full_hunt)
	await h._settle()
	await h._save("expedition_hunt_full")

	# State 1j — a recalled hunt party in its Returning phase: the Phase reads "Returning" and the
	# panel's Recall button flips to a disabled "Returning" (same treatment as the scout panel).
	var returning_hunt := _hunt_expedition_fixture()
	returning_hunt["expedition_phase"] = "returning"
	returning_hunt["stores"] = {"provisions": 12.0}
	returning_hunt["turns_of_food"] = 6.0
	h._hud.show_unit_selection(returning_hunt)
	await h._settle()
	await h._save("expedition_hunt_returning")

	# State 1j2 — a DEPLETE hunt party in flight: Deplete relaunches for repeated trips, so its
	# "Next delivery" line wears the recurring ↻ marker. That ↻ must read distinct from the Deplete
	# policy glyph (⇊) elsewhere in the panel — the whole point of the marker choice.
	var deplete_hunt := _hunt_expedition_fixture()
	deplete_hunt["expedition_hunt_policy"] = "deplete"
	deplete_hunt["expedition_eta_turns"] = 9
	deplete_hunt["expedition_projected_delivery"] = 16.0
	deplete_hunt["expedition_recurring"] = true
	h._hud.show_unit_selection(deplete_hunt)
	await h._settle()
	await h._save("expedition_hunt_recurring")

	# State 1j3 — **A LAUNCHED PARTY UNDER A FILL TARGET** (`docs/plan_hunt_through_combat.md` §5.2).
	# The two rows this adds are the in-flight twins of the sheet's two controls: the ORDER it was
	# given (`Fill target: 50 Red Deer`, beside `Leaves standing`) and the sim's own answer for that
	# order (the bound clause). **`expeditionTripBound` is the AUTHORITY once a party is out** — the
	# pre-launch turn count is a proportion the client composes, this is the sim's forward simulation
	# of the party's REAL orders — which is why it is rendered rather than the sheet's estimate being
	# remembered.
	var targeted_hunt := _hunt_expedition_fixture()
	targeted_hunt["expedition_fill_target"] = TARGETED_PARTY_FILL_TARGET
	targeted_hunt["expedition_trip_bound"] = SourceForecast.TRIP_BOUND_FILL_TARGET
	h._hud.show_unit_selection(targeted_hunt)
	await h._settle()
	await h._save("expedition_hunt_targeted")
	# **THE ROW IS READ THROUGH `Readout.detail_excerpt`, not searched for whole.** `detail_bbcode`
	# splits a `Key: value` line into two spans, so the rendered source never contains the line
	# contiguously — and the bare number would be no better a needle, `50` appearing in
	# `Leaves standing: 50%` two rows up. Excerpt from the KEY and assert the VALUE is what follows it.
	var fill_target_row := Readout.detail_excerpt(h._hud.occupant_detail.text, FILL_TARGET_DETAIL_KEY)
	h._assert_hud("a launched party states the fill target it was given, in the quarry's own units",
		fill_target_row.contains("%d %s" % [TARGETED_PARTY_FILL_TARGET, TARGETED_PARTY_QUARRY]))
	h._assert_hud("…and the sim's own answer for which stop will end its raid",
		h._hud.occupant_detail.text.contains(SourceForecast.TRIP_BOUND_CLAUSES[
			SourceForecast.TRIP_BOUND_FILL_TARGET]))
	# **THE `""` BOUND IS NOT `horizon`, AND IT RENDERS NOTHING.** A party already walking a load home
	# is not raiding toward a stop, so the row must be ABSENT rather than reading a stop it does not
	# have — and `expedition_hunt_panel` above is the frame that shows the absence, which is only a
	# claim because this state shows the presence.
	var untargeted_hunt := _hunt_expedition_fixture()
	h._hud.show_unit_selection(untargeted_hunt)
	await h._settle()
	h._assert_hud("a party the sim states no bound for says nothing about a stop",
		not h._hud.occupant_detail.text.contains(SourceForecast.TRIP_BOUND_CLAUSES[
				SourceForecast.TRIP_BOUND_FILL_TARGET])
			and not h._hud.occupant_detail.text.contains(SourceForecast.TRIP_BOUND_CLAUSES[
				SourceForecast.TRIP_BOUND_PACK_FULL]))
	h._assert_hud("…but still states its fill target ORDER, `none` being a real order",
		Readout.detail_excerpt(h._hud.occupant_detail.text, FILL_TARGET_DETAIL_KEY).contains("none"))

	# State 1k — the hunt launch policy picker: an idle band (short allocation panel) showing the
	# "Send expedition" outfit block — the party stepper, the scout + hunt send buttons, and the hunt
	# POLICY radio (DEPLETE selected) with its EXPEDITION hint. The expedition hints must never promise
	# HUSBANDRY — the Hunting arm accrues none, though since #337 it does bank the trade half of the
	# kill — so Deplete's line frames the rung by the PRESSURE it applies (relaunching trip after trip)
	# rather than by a craft the party cannot teach. The outfit block sits below the left dock's fold,
	# so scroll to see the hint.
	var launch_band := BandFx.band_fixture()
	launch_band["idle_workers"] = 12
	launch_band["labor_assignments"] = []
	var left_scroll: ScrollContainer = h._hud.left_stack.get_parent() as ScrollContainer
	h._hud._bandpanel._send_hunt_floor = ForageFx.DEEP_DRAW_FLOOR
	h._hud.show_unit_selection(launch_band)
	await h._settle()
	left_scroll.scroll_vertical = int(left_scroll.get_v_scroll_bar().max_value)
	await h._settle()
	await h._save("expedition_launch_policy")
	left_scroll.scroll_vertical = 0

	# State 1k-sustain — the SUSTAIN launch hint, which had to be rewritten when Sustain became the
	# maximum-sustainable-yield FLOW (it used to promise "one conservative harvest", a model that no
	# longer exists). It also must NOT mention domestication: only a RESIDENT band's Sustain hunt
	# builds husbandry — an expedition accrues none (the one payoff half still missing from a raid,
	# now that #337 banks its trade goods).
	h._hud._bandpanel._send_hunt_floor = SourceForecast.FLOOR_FOOD_PEAK
	h._hud.show_unit_selection(launch_band)
	await h._settle()
	left_scroll.scroll_vertical = int(left_scroll.get_v_scroll_bar().max_value)
	await h._settle()
	await h._save("expedition_launch_policy_sustain")
	left_scroll.scroll_vertical = 0

	# State 1a — a well-fed but demoralized band: healthy food (∞) yet morale 0.22
	# (< critical), so the drawer's Morale line reads a red 22%. Discontent drags
	# Output to 56% (red) and the itemized morale breakdown + recovery guidance show.
	h._hud.show_unit_selection(_low_morale_band_fixture())
	await h._settle()
	await h._save("band_low_morale")

	# State 1b — band alerts: seed previous sizes, then a snapshot that raises all
	# three alert kinds (starving red / losing-population amber / idle quiet).
	h._hud.update_band_alerts(_band_alert_baseline())
	h._hud.update_band_alerts(_band_alert_fixture())
	await h._settle()
	await h._save("band_alerts")

	# State 1c — Wondrous Sites: the top-bar `◈ Discoveries` readout. The `site_discovered` event is
	# pushed alongside it because a real snapshot carries both; the HUD's own consumer of that array
	# is the Telling now (the event dock is `Main`'s panel — see the `event_dock_*` block).
	h._hud.ingest_command_events([
		{"tick": 42, "kind": "site_discovered", "label": "Discovered Verdant Basin", "detail": "A settle-site revealed at (20, 14)."},
	])
	h._hud.clear_selection()
	await h._settle()
	await h._save("discoveries")

	# (State 1d — `predator_feed` — is RETIRED with the left-dock command feed it rendered. The
	# threat/casualty alert styling it judged moved into `HudEventVocab.KIND_STYLE` and is judged on
	# `event_dock_bottom` / `event_dock_pinned_alert` at the end of this run.)

	# State 1e — Predators Phase 3 band readout: the Warrior-card "⚠ Predator nearby — N on guard"
	# crimson alert AND the "⚔ Lost to raids −1.20" ledger row, both lit at once. A threatening predator
	# is placed within raid range in the world-herd list so the client-derived proximity check fires; the
	# food breakdown popover is opened to show the forfeit row. The shared herd list is restored after.
	h._set_world_herds([_raiding_predator_herd_fixture()])
	var raided_band := _raided_band_fixture()
	h._hud._band_labor._player_band = raided_band
	h._hud.show_unit_selection(raided_band)
	await h._settle()
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	await h._settle()
	await h._save("predator_band_raided")
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	h._set_world_herds(HerdFx.world_herds_fixture())   # restore the shared world-herd list

	# ---- THE FILL TARGET (`docs/plan_hunt_through_combat.md` §5.2) ------------------------------
	# **The defect these states close was found in PLAY, not in review**: a raid ends when its pack
	# fills, the pack is measured in carry and the take in reach, so `turns_to_fill` reduced to
	# `per_worker_carry / (engage_rate × body_mass)` and PARTY SIZE CANCELLED OUT. Eight hunters after
	# a Wild Fowl flock reported "away ≈43 turns" and four would have reported the same — the stepper
	# was not a weak lever, it was structurally not one.
	#
	# The fixture's rate divides EXACTLY (a party of 4 lands 20 animals a turn for 6 turns), which is
	# what lets the turn counts below be asserted as equalities rather than as ranges.
	await _fill_target_states()

	# ---- THE THREE KITS (`docs/plan_hunt_through_combat.md` §4.8) --------------------------------
	await _kit_states()

	# band_alerts (above) left _player_band as an alert-fixture band (no work_range, far from the food
	# tile); seed a NEAR band so the forage controls resolve an in-range actor.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)


## **THE FILL TARGET** (`docs/plan_hunt_through_combat.md` §5.2) — the party-side twin of the harvest
## floor, and the client half of the fix for a raid whose length was a species constant with no lever.
##
## Four frames plus two PNG-less blocks, and the split is deliberate: a PICTURE can show that a
## control exists and what it reads, but only an ASSERTION can show that the numbers on it agree with
## the sim's, and only a byte-comparison can show that the untargeted raid is *unchanged*.
func _fill_target_states() -> void:
	var band: Dictionary = BandFx.hunt_distance_bands()[1]      # the FAR band — beyond hunt reach
	h._hud._band_labor._player_bands = [band]
	h._hud._band_labor._player_band = band
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	var herd := _fill_target_raid_herd()
	h._show_herd(herd)
	h._compose_herd(herd, FILL_TARGET_RAID_PARTY, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	await h._save("fill_target_off")
	var sheet: Control = h._hud._drawercompose._compose_sheet
	# **PRECONDITION, not decoration.** Every claim below is about a control on THIS sheet, and a
	# fixture whose party or floor lands on a different estimate row would make all of them vacuous.
	h._assert_hud("the fill-target fixture composes the party its rate is stated for",
		h._hud._compose.hunt_count() == FILL_TARGET_RAID_PARTY)
	h._assert_hud("the untargeted raid runs the turns the fixture states",
		Readout.fill_target_turns(sheet) == FILL_TARGET_RAID_TURNS)
	h._assert_hud("…with no target set, so the pack is what fills",
		Readout.fill_target_value(sheet) == SourceForecast.NO_FILL_TARGET
			and h._hud._compose.hunt_fill_target() == SourceForecast.NO_FILL_TARGET)
	h._assert_hud("…and the verdict names the PACK as the stop",
		Readout.verdict_text(sheet).contains(
			SourceForecast.TRIP_BOUND_CLAUSES[SourceForecast.TRIP_BOUND_PACK_FULL]))

	# TICK IT. The checkbox is the only way in or out of a target — the stepper bottoms out at one
	# hunting turn — and it seeds the SHORTEST real trip, which is what makes the first press state
	# the axis's own unit instead of a number nothing chose.
	h._assert_hud("the fill-target checkbox is reachable", _toggle_fill_target(sheet))
	await h._settle()
	sheet = h._hud._drawercompose._compose_sheet
	await h._save("fill_target_one_turn")
	h._assert_hud("ticking the box asks for ONE turn's animals",
		Readout.fill_target_value(sheet) == FILL_TARGET_RAID_RATE)
	h._assert_hud("…so the trip is one hunting turn, not six",
		Readout.fill_target_turns(sheet) == 1)
	# **THE POINT OF THE WHOLE SLICE**: the readout no longer merely reports a shorter number, it says
	# WHICH stop produced it — and the pack clause must be GONE, or the two stops are indistinguishable
	# in exactly the way §5.2 says they must not be.
	h._assert_hud("…and the verdict now names the FILL TARGET, not the pack",
		Readout.verdict_text(sheet).contains(
				SourceForecast.TRIP_BOUND_CLAUSES[SourceForecast.TRIP_BOUND_FILL_TARGET])
			and not Readout.verdict_text(sheet).contains(
				SourceForecast.TRIP_BOUND_CLAUSES[SourceForecast.TRIP_BOUND_PACK_FULL]))
	# The PAYLOAD follows the target: a raid that comes home early brings home less. Asserted because a
	# target that shortened the trip while still promising the full haul would be the most attractive
	# possible reading and a straightforwardly false one.
	h._assert_hud("…and the payload is the targeted haul, not the untargeted one",
		Readout.yields_text(sheet).contains("≈%d" % FILL_TARGET_RAID_RATE)
			and not Readout.yields_text(sheet).contains("≈%d" % FILL_TARGET_RAID_ANIMALS))

	# STEP UP TWICE — each press buys exactly one more turn of hunting, which is the unit the control
	# is denominated in and the reason its step is not one animal.
	h._hud._compose.set_hunt_fill_target(FILL_TARGET_RAID_RATE * 3)
	h._compose_herd(herd)
	await h._settle()
	sheet = h._hud._drawercompose._compose_sheet
	await h._save("fill_target_three_turns")
	h._assert_hud("three turns' animals cost three hunting turns",
		Readout.fill_target_value(sheet) == FILL_TARGET_RAID_RATE * 3
			and Readout.fill_target_turns(sheet) == 3)

	# **THE PARTY STEPPER IS A TRIP-LENGTH LEVER AGAIN**, which is the defect §5.1 reports. Hold ONE
	# target and HALVE the party: the rate halves, so the trip doubles. Under the old model this
	# assertion was unwritable — party size cancelled out of `turns_to_fill` exactly, so four hunters
	# and sixteen both reported 31 turns on a Wild Fowl flock.
	#
	# **The target is TWO turns' animals at the full party, not three**, and the reason is the identity
	# itself: at half the party the untargeted raid brings home exactly `RATE × 3`, so a target of that
	# size is at capacity and folds back to NO TARGET — the assertion would be measuring the untargeted
	# trip in both halves and passing on nothing.
	h._hud._compose.set_hunt_fill_target(FILL_TARGET_RAID_RATE * 2)
	h._compose_herd(herd)
	await h._settle()
	sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("two turns' animals cost two turns at the full party",
		Readout.fill_target_value(sheet) == FILL_TARGET_RAID_RATE * 2
			and Readout.fill_target_turns(sheet) == 2)
	h._hud._compose.set_hunt_count(FILL_TARGET_RAID_PARTY / 2)
	h._compose_herd(herd)
	await h._settle()
	sheet = h._hud._drawercompose._compose_sheet
	h._assert_hud("…and halving the party doubles that wait — the stepper is a trip-length lever again",
		Readout.fill_target_value(sheet) == FILL_TARGET_RAID_RATE * 2
			and Readout.fill_target_turns(sheet) == 4)
	h._hud._compose.set_hunt_count(FILL_TARGET_RAID_PARTY)

	# A FLOOR-BOUND RAID — the OTHER sentence §5.2 asks for, and the reason the bound is a
	# discriminator rather than a decoration: the same control, the same kind of turn count, a
	# different stop. Its clause must be the floor's and NOT the pack's.
	var floor_bound := _floor_bound_raid_herd()
	h._show_herd(floor_bound)
	h._compose_herd(floor_bound, FILL_TARGET_RAID_PARTY, SourceForecast.FLOOR_FOOD_PEAK)
	await h._settle()
	sheet = h._hud._drawercompose._compose_sheet
	await h._save("fill_target_floor_bound")
	h._assert_hud("a floor-bound raid says the HERD ran out, not the pack",
		Readout.verdict_text(sheet).contains(
				SourceForecast.TRIP_BOUND_CLAUSES[SourceForecast.TRIP_BOUND_FLOOR])
			and not Readout.verdict_text(sheet).contains(
				SourceForecast.TRIP_BOUND_CLAUSES[SourceForecast.TRIP_BOUND_PACK_FULL]))

	_assert_fill_target_arithmetic(band, herd)
	_assert_fill_target_command()

	# Leave the compose state as the block found it, so nothing downstream inherits a target.
	h._hud._drawercompose.close_compose_sheet()
	h._hud._compose.reset_hunt_source()
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	await h._settle()


## **THE PRE-LAUNCH TURN COUNT IS A PROPORTION ON THE SIM'S OWN ANSWER, AND THIS BOUNDS ITS ERROR.**
##
## The sim cannot preview a TARGETED trip — `huntTripEstimates` samples floor × party size only — so
## the client divides the row's own `animals_taken` over `turns_to_fill`. That is exact while the
## raid's per-turn take is flat (engagement and carry do not move as the herd draws down) and it is
## simply not consulted where the take is NOT flat, because there the herd hits the FLOOR first and
## the trip ends on the other bound. This block pins the exact half: on a fixture whose rate divides
## evenly, asking for `k` turns' animals must answer exactly `k` for EVERY k the raid can reach — so
## the client's arithmetic reproduces the sim's own pair rather than merely approximating it.
##
## A picture cannot carry any of this, which is why it is PNG-less.
func _assert_fill_target_arithmetic(band: Dictionary, herd: Dictionary) -> void:
	var exact := true
	for k in range(1, FILL_TARGET_RAID_TURNS):
		if SourceForecast.raid_target_turns(FILL_TARGET_RAID_RATE * k,
				FILL_TARGET_RAID_ANIMALS, FILL_TARGET_RAID_TURNS) != k:
			exact = false
	h._assert_hud("k turns' animals cost exactly k turns at a constant rate (k = 1..%d)"
		% (FILL_TARGET_RAID_TURNS - 1), exact)
	# **THE IDENTITY THAT MAKES THE SLICE SAFE TO LAND** (§5.2): a target at or above what the raid
	# already brings home IS the untargeted raid — the sim's `raid_load` hands the pack straight back —
	# so the client must answer with the very same forecast rather than with a second, nearly-equal
	# one. Compared as whole dictionaries, not field by field: a field-wise check would silently miss
	# whichever field the target branch grew next.
	var untargeted := SourceForecast.hunt_trip_forecast(band, ForageFx.floorify(herd.duplicate(true)),
		SourceForecast.FLOOR_FOOD_PEAK, FILL_TARGET_RAID_PARTY,
		h._hud._band_labor.grid_width(), h._hud._band_labor.wrap_horizontal(),
		SourceForecast.NO_FILL_TARGET)
	var at_capacity := SourceForecast.hunt_trip_forecast(band, ForageFx.floorify(herd.duplicate(true)),
		SourceForecast.FLOOR_FOOD_PEAK, FILL_TARGET_RAID_PARTY,
		h._hud._band_labor.grid_width(), h._hud._band_labor.wrap_horizontal(),
		FILL_TARGET_RAID_ANIMALS)
	h._assert_hud("a target AT the untargeted haul is the untargeted raid, exactly",
		_forecast_matches(untargeted, at_capacity))
	var above := SourceForecast.hunt_trip_forecast(band, ForageFx.floorify(herd.duplicate(true)),
		SourceForecast.FLOOR_FOOD_PEAK, FILL_TARGET_RAID_PARTY,
		h._hud._band_labor.grid_width(), h._hud._band_labor.wrap_horizontal(),
		FILL_TARGET_RAID_ANIMALS * 2)
	h._assert_hud("…and so is a target ABOVE it", _forecast_matches(untargeted, above))
	# The clamp says the same thing about the CONTROL: a target the raid would ignore must not sit on
	# the sheet as a number, or the lever looks set and is not — the §5.1 defect in miniature.
	h._assert_hud("a target at or above the haul folds back to NO TARGET on the control",
		SourceForecast.clamp_fill_target(FILL_TARGET_RAID_ANIMALS, FILL_TARGET_RAID_RATE,
				FILL_TARGET_RAID_ANIMALS) == SourceForecast.NO_FILL_TARGET
			and SourceForecast.clamp_fill_target(FILL_TARGET_RAID_ANIMALS + 1,
				FILL_TARGET_RAID_RATE, FILL_TARGET_RAID_ANIMALS) == SourceForecast.NO_FILL_TARGET)


## **THE COMMAND CARRIES THE TARGET, AND OMITS IT WHEN THERE IS NONE.** `send_hunt_expedition`'s
## grammar is positional (`… <fauna_id> [floor] [fill_target]`), so both halves are the claim: a
## targeted launch must append the token, and an untargeted one must emit the line it emitted before
## the lever existed — the byte-identity half of §5.2, at the command layer.
func _assert_fill_target_command() -> void:
	var payload := {
		"faction": HudConst.PLAYER_FACTION_ID, "band_id": 812, "party_workers": 4,
		"fauna_id": "game_deer_07", "fauna_label": "Red Deer",
		"floor": SourceForecast.FLOOR_FOOD_PEAK,
	}
	var untargeted := String(h.MAIN_SCRIPT.format_send_hunt_expedition(payload).get("line", ""))
	payload["fill_target"] = SourceForecast.NO_FILL_TARGET
	h._assert_hud("a fill target of 0 emits the untargeted line, token and all",
		String(h.MAIN_SCRIPT.format_send_hunt_expedition(payload).get("line", "")) == untargeted)
	payload["fill_target"] = FILL_TARGET_RAID_RATE
	var targeted: Dictionary = h.MAIN_SCRIPT.format_send_hunt_expedition(payload)
	h._assert_hud("…and a real target rides as the trailing token",
		String(targeted.get("line", "")) == "%s %d" % [untargeted, FILL_TARGET_RAID_RATE])
	# The feed receipt names BOTH orders, because a raid is ordered with both and a receipt quoting
	# one describes a mission the player did not give.
	h._assert_hud("…with the command feed naming the target beside the floor",
		String(targeted.get("message", "")).contains(str(FILL_TARGET_RAID_RATE)))


## Two `hunt_trip_forecast` answers, compared WHOLE. `==` on a Dictionary is identity in GDScript, so
## the comparison has to walk the keys — and it walks BOTH key sets, since a branch that ADDED a key
## would otherwise pass a one-directional scan.
func _forecast_matches(a: Dictionary, b: Dictionary) -> bool:
	if a.size() != b.size():
		return false
	for key in a:
		if not b.has(key) or a[key] != b[key]:
			return false
	return true


## **THE FILL TARGET's quarry** — the distance herd's shape with a raid table whose per-turn rate is a
## whole number (see `FILL_TARGET_RAID_RATE`), so the targeted trip lengths the states assert are
## equalities rather than ranges. `pack_full` is its bound: nothing about the herd runs out, the party
## simply cannot carry more — which is the contrast the floor-bound twin below exists against.
func _fill_target_raid_herd() -> Dictionary:
	var herd := HerdFx.herd_fixture()
	herd["tile_info"] = HerdFx.plain_herd_tile_info()
	herd["hunt_trip_estimates"] = HerdFx.raid_estimate_table(
		FILL_TARGET_RAID_TURNS_ROW, FILL_TARGET_RAID_ANIMALS_ROW, float(herd["food_per_animal"]))
	return herd


## The SAME raid, stopped by the HERD instead of by the party — the other sentence §5.2 asks the
## readout to be able to say. Only the bound differs from `_fill_target_raid_herd`, and that is the
## point: the two frames carry identical numbers and must still read as different decisions, which is
## the claim a turn count alone structurally cannot make.
func _floor_bound_raid_herd() -> Dictionary:
	var herd := HerdFx.herd_fixture()
	herd["id"] = "game_deer_08"
	herd["tile_info"] = HerdFx.plain_herd_tile_info()
	herd["hunt_trip_estimates"] = HerdFx.raid_estimate_table(
		FILL_TARGET_RAID_TURNS_ROW, FILL_TARGET_RAID_ANIMALS_ROW, float(herd["food_per_animal"]),
		HerdFx.RAID_TRADE_PER_ANIMAL, SourceForecast.TRIP_BOUND_FLOOR)
	return herd


## Press the fill-target control's checkbox — the ONLY way to set or clear a target, since its stepper
## bottoms out at one hunting turn rather than at zero. Returns false when the control is absent.
func _toggle_fill_target(root: Node) -> bool:
	var block := Q.find_meta_node(root, HudWidgets.FILL_TARGET_META)
	if block == null:
		return false
	var box := _find_first_checkbox(block)
	if box == null:
		return false
	box.button_pressed = not box.button_pressed
	return true

func _find_first_checkbox(root: Node) -> CheckBox:
	if root is CheckBox:
		return root as CheckBox
	for child in root.get_children():
		var found := _find_first_checkbox(child)
		if found != null:
			return found
	return null


# ---- THE THREE KITS (`docs/plan_hunt_through_combat.md` §4.8) ------------------------------------
# The kit disclosure key for the kitted band — `DetailFormat.breakdown_key(kind, band)`'s shape, over
# the reference band's own entity so it cannot collide with its Food/Morale/Growth popovers.
const BAND_DISCLOSURE_KIT := "kit:904"

## **THE KITS WERE INVISIBLE, AND A PLAYER COULD NOT SEE THEIR EQUIPMENT DYING.** Three consumables
## ship — spears raising `attack`, a SLED carrying the hunt, BASKETS carrying the forage web — and all
## six wire fields arrived and were dropped. These four frames are the readout, and the split is
## deliberate: the ROW answers *how long have I got and which side of the line am I on*, and only the
## DISCLOSURE has room for *what each one is doing for me, and what happens when it stops*.
##
## **NOTHING HERE MAY BE SCALED BY THE REMAINING CONDITION.** Durability and performance are
## orthogonal — a kit at 3 performs exactly as one at 97 and then stops — so the assertions below pin
## the TIER against its shipped constant rather than against anything derived from the condition, and
## a readout that drew a gradient would fail them at every condition but full.
func _kit_states() -> void:
	# State kit-a — ONE KIT DRY, the other two intact. The row reads two live conditions and one
	# DANGER-inked word, which is the whole of what a glance has to deliver: two clocks and one loss.
	var worn := BandFx.with_baskets_dry(BandFx.band_fixture())
	h._hud._band_labor._player_band = worn
	h._hud.show_unit_selection(worn)
	await h._settle()
	await h._save("band_kit")
	# **THE FACES ARE READ OFF THE PARSED TEXT, THE INK OFF THE SOURCE**, and the split is forced by
	# the row itself: each kit's condition is wrapped in its own `[color]` span, so the rendered
	# source never contains `Spears 87` contiguously — while the parsed text, having dropped every
	# tag, cannot testify about a colour. Two readings of one label, each asked what it can answer.
	var kit_row := String(h._hud.occupant_detail.get_parsed_text())
	h._assert_hud("the Kit row states all three kits, live ones by condition",
		kit_row.contains("%s %s" % [DetailFormat.KIT_LABEL_SPEARS,
				String.num(BandFx.KIT_CONDITION_SPEARS, DetailFormat.KIT_CONDITION_DECIMALS)])
			and kit_row.contains("%s %s" % [DetailFormat.KIT_LABEL_SLED,
				String.num(BandFx.KIT_CONDITION_SLED, DetailFormat.KIT_CONDITION_DECIMALS)]))
	# **A SPENT KIT READS AS A WORD, NOT A ZERO.** The number is not the point — which side of the
	# cliff the role is on is — and a `0` beside two live conditions reads as a quantity on the same
	# scale rather than as a state change. The DANGER span is asserted with the kit's own NAME in
	# front of it, because this label carries other red runs (a negative food net) that a bare hex
	# search would match.
	h._assert_hud("…and a spent kit reads as a WORD, in DANGER ink",
		h._hud.occupant_detail.text.contains("%s [color=#%s]%s" % [DetailFormat.KIT_LABEL_BASKETS,
			HudStyle.DANGER_HEX, DetailFormat.KIT_DRY_FACE]))

	# State kit-b — the SAME band, disclosure OPEN. **This frame carries the cross-check the whole
	# three-kit split exists for**: the sled's line must quote the HUNT's carry and the basket's line
	# the FORAGE web's, and neither may quote the other's. Baskets boosting the hunt is precisely the
	# defect slice 5 corrected in the sim, and rendering one tier on the other's row would carry it
	# straight back into the UI where no sim test can see it.
	_click_disclosure(BAND_DISCLOSURE_KIT)
	await h._settle()
	await h._save("band_kit_expanded")
	var kit_popover := _kit_popover_text()
	var sled_line := _kit_breakdown_line(kit_popover, DetailFormat.KIT_LABEL_SLED)
	var basket_line := _kit_breakdown_line(kit_popover, DetailFormat.KIT_LABEL_BASKETS)
	var hunt_carry := String.num(BandFx.KIT_HUNT_CARRY_EQUIPPED, DetailFormat.KIT_CARRY_DECIMALS)
	var forage_carry := String.num(BandFx.KIT_FORAGE_CARRY_BARE, DetailFormat.KIT_CARRY_DECIMALS)
	h._assert_hud("the SLED's line quotes the HUNT's carry (%s) and never the forage web's (%s)"
		% [hunt_carry, forage_carry],
		sled_line.contains(hunt_carry) and not sled_line.contains(forage_carry))
	h._assert_hud("the BASKETS' line quotes the FORAGE web's carry (%s) and never the hunt's (%s)"
		% [forage_carry, hunt_carry],
		basket_line.contains(forage_carry) and not basket_line.contains(hunt_carry))
	h._assert_hud("the SPEARS' line quotes the attack tier they set (%s)"
		% String.num(BandFx.KIT_ATTACK_EQUIPPED, DetailFormat.KIT_CONDITION_DECIMALS),
		_kit_breakdown_line(kit_popover, DetailFormat.KIT_LABEL_SPEARS).contains(
			DetailFormat.KIT_ROLE_ATTACK_FORMAT % String.num(BandFx.KIT_ATTACK_EQUIPPED,
				DetailFormat.KIT_CONDITION_DECIMALS)))
	# **THE CLIFF SENTENCE IS WHAT STOPS THE CONDITIONS READING AS A PERFORMANCE GRADIENT.** Without
	# it a player paces their hunting against `87` and `54` as if they were rates.
	h._assert_hud("…and the popover says the condition is a clock, not a rate",
		kit_popover.contains(DetailFormat.KIT_BREAKDOWN_CLIFF_NOTE))
	h._assert_hud("…and only the spent kit is called out as bare hands",
		basket_line.contains(DetailFormat.KIT_BARE_HANDS_SUFFIX)
			and not sled_line.contains(DetailFormat.KIT_BARE_HANDS_SUFFIX))
	_click_disclosure(BAND_DISCLOSURE_KIT)

	# State kit-c — EVERY kit run dry. Bare hands is a state worth showing plainly: there is no
	# replenishment path, so all three roles have stepped down and stay there.
	var bare := BandFx.with_bare_hands(BandFx.band_fixture())
	h._hud._band_labor._player_band = bare
	h._hud.show_unit_selection(bare)
	await h._settle()
	_click_disclosure(BAND_DISCLOSURE_KIT)
	await h._settle()
	await h._save("band_kit_bare")
	var bare_popover := _kit_popover_text()
	h._assert_hud("a band with nothing left states bare hands on all three roles",
		_kit_breakdown_line(bare_popover, DetailFormat.KIT_LABEL_SPEARS).contains(
				DetailFormat.KIT_BARE_HANDS_SUFFIX)
			and _kit_breakdown_line(bare_popover, DetailFormat.KIT_LABEL_SLED).contains(
				DetailFormat.KIT_BARE_HANDS_SUFFIX)
			and _kit_breakdown_line(bare_popover, DetailFormat.KIT_LABEL_BASKETS).contains(
				DetailFormat.KIT_BARE_HANDS_SUFFIX))
	h._assert_hud("…and the two carries STILL do not swap: the sled reads %s, the baskets %s"
		% [String.num(BandFx.KIT_HUNT_CARRY_BARE, DetailFormat.KIT_CARRY_DECIMALS),
			String.num(BandFx.KIT_FORAGE_CARRY_BARE, DetailFormat.KIT_CARRY_DECIMALS)],
		_kit_breakdown_line(bare_popover, DetailFormat.KIT_LABEL_SLED).contains(
				String.num(BandFx.KIT_HUNT_CARRY_BARE, DetailFormat.KIT_CARRY_DECIMALS))
			and _kit_breakdown_line(bare_popover, DetailFormat.KIT_LABEL_BASKETS).contains(
				String.num(BandFx.KIT_FORAGE_CARRY_BARE, DetailFormat.KIT_CARRY_DECIMALS)))
	_click_disclosure(BAND_DISCLOSURE_KIT)

	# **THE NEGATIVE HALF, and without it the three frames above are satisfied by a row that renders
	# unconditionally.** A band that states no kit at all — every fixture predating the TOE, and the
	# state a rehydrated cohort is in — must render NO Kit row, because a defaulted `Spears 0` would
	# report equipment destroyed that was never there. It is asserted rather than rendered: the
	# reference band's own frame (`band`, far above) is the picture.
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	h._assert_hud("a band that states no kit renders no Kit row — never a defaulted zero",
		Readout.detail_excerpt(h._hud.occupant_detail.text, HudDisclosureVocab.DETAIL_ROW_KIT)
			== Readout.DETAIL_EXCERPT_ABSENT)

## The open breakdown popover's text — the RENDERED disclosure, not the producer's return, so the
## assertions above cover the click, the payload stash and the popover's own restate.
func _kit_popover_text() -> String:
	var label = h._hud._disclosures._breakdown_popover_label
	return "" if label == null else (label as RichTextLabel).get_parsed_text()

## ONE kit's breakdown line out of the popover, by the kit's NAME. Split per line rather than matched
## across the whole popover, because the three lines carry the same shape and a whole-popover
## `contains` could be satisfied by the WRONG kit's row — which is the exact substitution these
## assertions exist to catch.
func _kit_breakdown_line(popover: String, label: String) -> String:
	for line in popover.split("\n"):
		if String(line).contains(label):
			return String(line)
	return ""
