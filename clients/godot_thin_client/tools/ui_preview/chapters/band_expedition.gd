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

# The expedition drawer's `Move` face — the CONTROL that must survive when the founding button is
# withheld, so an absence claim cannot be satisfied by a panel that built nothing at all. Spelled
# here rather than shared with `tile_panel.gd`'s copy: a chapter owns the fixtures only it uses.
const EXPEDITION_MOVE_BUTTON_TEXT := "Move"

# The reference band (`BandFx.band_fixture()`, entity 904) disclosure keys — the `[url]` meta its Food /
# Morale rows carry, i.e. what `DetailFormat.breakdown_key` builds for it.
const BAND_DISCLOSURE_FOOD := "food:904"

const BAND_DISCLOSURE_MORALE := "morale:904"

const BAND_DISCLOSURE_GROWTH := "growth:904"

# The collapsed-growth band is `_concerning_food_band_fixture`'s entity (905), not 904.
const BAND_DISCLOSURE_GROWTH_COLLAPSED := "growth:905"

# ---- THE LAUNCHED HUNT PARTY'S ORDERS ROW --------------------------------------------------------
# The detail row's KEY, which is what `Readout.detail_excerpt` seeks: the leading half of
# `DetailFormat.EXPEDITION_ORDERS_ROW_FORMAT`, restated here only because a `const` cannot split one.
# A reworded row does not pass quietly — the excerpt answers `DETAIL_EXCERPT_ABSENT` and every
# assertion below fails naming the row it could not find.
#
# **IT WAS `Leaves standing`, THEN A MERGED TWO-CLAUSE ROW, AND IT IS THE SAME ROW.** The fill target
# that shared it is retired (issue #491), so the row states the floor alone; it stays a merged-shaped
# `Orders:` row because the parties inspector strip budgeted for ONE row here — see
# `DetailFormat.expedition_orders_line`.
const EXPEDITION_ORDERS_DETAIL_KEY := "Orders"

# ---- THE IN-FLIGHT DENIAL RAID (`docs/plan_denial_raid.md` §3) -----------------------------------
# The party size the frame renders, and the row of `HerdFx`'s denial table it therefore reads. Named
# so the expected sentence is composed from the SAME index the fixture is, rather than from a literal
# that would drift the first time the table is re-tuned.
const DENIAL_PARTY_SIZE := 5

## The species the shared world-herd list names, i.e. what the verdict must be phrased about.
const DENIAL_TARGET_QUARRY := "Red Deer"

## How long the sim's raid projection runs (`expeditionForecastHorizonTurns`) and a walk out to the
## quarry, for the driven `horizon`-verdict claims. They are deliberately DIFFERENT numbers, so a
## sentence that shifted by the wrong term — or by none — is a different string.
const DENIAL_HORIZON_TURNS := BandFx.FORECAST_HORIZON_TURNS
const DENIAL_HORIZON_OUTBOUND_TURNS := 7

## The two row KEYS a denial party must NOT render, each because the mission has no such thing: the
## hunt party's ORDERS row — a floor it never chose — and a delivery it is not making.
const DENIAL_ABSENT_ORDERS_KEY := EXPEDITION_ORDERS_DETAIL_KEY

const DENIAL_ABSENT_DELIVERY_KEY := "Next delivery"

## …and the one it MUST, near-empty: the little the raid banks on the way home.
const DENIAL_CARRIED_DETAIL_KEY := "Carried"

## Find a Button by its face anywhere under `root` — the harness presses the REAL control the player
## presses, so an assertion covers the wiring and not just the handler it would have called.
## Drive a Food/Morale disclosure the way a CLICK does: emit `meta_clicked` on the live drawer label
## with the very `[url]` meta its own text carries, so the bound handler + anchor run exactly as they
## do in the game. Toggling: a second call on the same key dismisses the popover.
func _click_disclosure(key: String) -> void:
	var meta := HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + key
	var label = _find_meta_label(h._hud, meta)
	if label == null:
		# **A CLICK THAT NEVER HAPPENED IS A FAILED PRECONDITION, NOT AN ADVISORY.** The states this
		# drives assert what the popover holds and what the drawer no longer does, and the second half
		# passes on a drawer that rendered no disclosure at all — so warning here left the block
		# claiming its result vacuously, with the frame quietly showing the un-clicked state.
		h._fail("no detail label offering '%s' — the disclosure was never rendered, so nothing was clicked" % meta)
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
		# The drawer's standing summary comes from the SAME `SourceForecast.source_yield_readout` the
		# Band panel's rows use, so the two surfaces cannot state different products for one
		# assignment. (It carried a second, trade-goods clause until arc #527 retired that account.)
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5, "improvement": "corral", "target_x": 70, "target_y": 17, "actual_yield": 0.84, "sustainable_yield": 0.84, "workers_needed": 1, "overdraws": false},
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
## A launched DENIAL raid, built off the hunt party so the only differences are the mission's own.
##
## **`expedition_floor` 0.0 IS ON IT DELIBERATELY** — it is what the sim really publishes for this
## mission (which has no such lever), so a fixture omitting it would let the absent `Orders:` row pass
## on a party that simply carried no field. The delivery trio is absent because a denial party
## genuinely publishes none.
##
## **IT CARRIES A `band_id` AND ITS OWN `kit_id`, and both are what make its `Collapse:` row reachable.**
## A detached party is a band, so the collapse forecast is a QUERY asked about IT — and a party holding
## `HudConst.NO_BAND_ID` is one the asker correctly refuses to compose a question about, which renders
## the row's placeholder forever. The kit is the one it was outfitted with at launch, which is what the
## sim prices its whole life from.
func _denial_expedition_fixture() -> Dictionary:
	return BandFx.with_band_id({
		"id": "Raiders 1",
		"size": DENIAL_PARTY_SIZE,
		"entity": 7104,
		"faction": 0,
		"kit_id": BandFx.KIT_DEFAULT_HUNT,
		"pos": [67, 16],
		"turns_of_food": 3.0,
		# A rounding error against what it killed — the mission's own cost, stated rather than hidden.
		"stores": {"provisions": 2.0},
		"is_expedition": true,
		"expedition_mission": "deny",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_07",
		"expedition_carry_cap": 16.0,
		"expedition_floor": 0.0,
		"tile_info": {
			"x": 67, "y": 16,
			"terrain_label": "Prairie Steppe",
			"tags_text": "Fertile",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	})

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
	# The returning panel is still a PANEL, not an empty host: Move stays beside the disabled
	# "Returning" button, so a phase branch that silently built nothing is caught here.
	h._assert_hud("a returning party still gets its Move action",
		Q.find_button_by_text(h._hud.allocation_panel, EXPEDITION_MOVE_BUTTON_TEXT) != null)

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

	# State 1j3 — **A LAUNCHED PARTY UNDER A STATED TRIP BOUND.** The row this adds is the sim's own
	# answer for which stop will end the raid, in the same words the pre-launch readout uses.
	# **`expeditionTripBound` is the AUTHORITY once a party is out** — the sheet's estimate is a
	# projection over a SAMPLED party and floor, this is the sim's forward simulation of the party's
	# REAL orders — which is why it is rendered rather than the sheet's estimate being remembered.
	# The frame staged a FILL TARGET until issue #491 retired that lever; the bound is what is left of
	# it, and `pack_full` is the stop the raid it staged would really have reached.
	var bounded_hunt := _hunt_expedition_fixture()
	bounded_hunt["expedition_trip_bound"] = SourceForecast.TRIP_BOUND_PACK_FULL
	h._hud.show_unit_selection(bounded_hunt)
	await h._settle()
	await h._save("expedition_hunt_bounded")
	# **THE ROW IS READ THROUGH `Readout.detail_excerpt`, not searched for whole.** `detail_bbcode`
	# splits a `Key: value` line into two spans, so the rendered source never contains the line
	# contiguously. Excerpt from the KEY and assert the VALUE is what follows it.
	var orders_row := Readout.detail_excerpt(h._hud.occupant_detail.text,
		EXPEDITION_ORDERS_DETAIL_KEY)
	h._assert_hud("a launched party states the ONE order it carries — the floor it was given",
		orders_row.contains(HudComposeVocab.FLOOR_VALUE_FORMAT % SourceForecast.floor_percent(
			SourceForecast.DEFAULT_HARVEST_FLOOR)))
	h._assert_hud("…and the sim's own answer for which stop will end its raid",
		h._hud.occupant_detail.text.contains(SourceForecast.TRIP_BOUND_CLAUSES[
			SourceForecast.TRIP_BOUND_PACK_FULL]))
	# **THE `""` BOUND IS NOT `horizon`, AND IT RENDERS NOTHING.** A party already walking a load home
	# is not raiding toward a stop, so the row must be ABSENT rather than reading a stop it does not
	# have — and this negative is only a claim because the state above shows the presence.
	var unbounded_hunt := _hunt_expedition_fixture()
	h._hud.show_unit_selection(unbounded_hunt)
	await h._settle()
	h._assert_hud("a party the sim states no bound for says nothing about a stop",
		not h._hud.occupant_detail.text.contains(SourceForecast.TRIP_BOUND_CLAUSES[
				SourceForecast.TRIP_BOUND_FLOOR])
			and not h._hud.occupant_detail.text.contains(SourceForecast.TRIP_BOUND_CLAUSES[
				SourceForecast.TRIP_BOUND_PACK_FULL]))
	# …and it still states its ORDERS row, the floor being an order every hunt party carries.
	h._assert_hud("…but still states the floor it is holding",
		Readout.detail_excerpt(h._hud.occupant_detail.text, EXPEDITION_ORDERS_DETAIL_KEY).contains(
			HudComposeVocab.FLOOR_VALUE_FORMAT % SourceForecast.floor_percent(
				SourceForecast.DEFAULT_HARVEST_FLOOR)))

	# State 1j4 — **AN IN-FLIGHT DENIAL RAID** (`docs/plan_denial_raid.md` §3). The third mission, and
	# its drawer is judged on what it does NOT say as much as on what it does: a denial party publishes
	# no delivery ETA and has no floor and no fill target, so the `Orders:` row (which carries both) and
	# the `Next delivery` line must both be absent, and the collapse verdict stands where the ETA stands
	# on a hunt party.
	var deny_party := _denial_expedition_fixture()
	h._hud.show_unit_selection(deny_party)
	await h._settle()
	await h._save("expedition_denial_panel")
	var deny_text: String = h._hud.occupant_detail.text
	h._assert_hud("a denial party names its MISSION",
		deny_text.contains(HudExpeditionVocab.EXPEDITION_MISSION_LABELS[
			HudExpeditionVocab.EXPEDITION_MISSION_DENY]))
	# **THE VERDICT, COMPOSED FROM THE FIXTURE'S OWN ROW** — the party of `DENIAL_PARTY_SIZE` reads the
	# table's row for that size, so the expected sentence is stated from the harness's side and the two
	# arrive at one string from opposite ends.
	var party_low: int = HerdFx.DENIAL_COLLAPSE_LOW[DENIAL_PARTY_SIZE - 1]
	var party_high: int = HerdFx.DENIAL_COLLAPSE_HIGH[DENIAL_PARTY_SIZE - 1]
	var deny_verdict: String = SourceForecast.DENIAL_VERDICTS[
		SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY]["line"] % DENIAL_TARGET_QUARRY
	# **AN IN-FLIGHT VERDICT QUOTES THE AT-THE-HERD SPAN, AND SAYS SO.** The launch sheet adds the
	# outbound walk and reads "…from launch"; this party has already left and its remaining walk is not
	# on the wire, so the drawer quotes the sim's own raiding turns UNSHIFTED under a clause that names
	# them. Neither surface leaves the span to be inferred — that was the defect.
	var party_turns: int = HerdFx.DENIAL_COLLAPSE_TURNS[DENIAL_PARTY_SIZE - 1]
	deny_verdict += SourceForecast.DENIAL_TURNS_LEAD_FORMAT % [
		SourceForecast.DENIAL_TURNS_ONE_FORMAT % party_turns,
		SourceForecast.DENIAL_SPAN_OF_RAIDING]
	deny_verdict += SourceForecast.DENIAL_SPREAD_RANGE_FORMAT % [party_low, party_high]
	h._assert_hud("…and states the COLLAPSE VERDICT over its RAIDING turns, expectation first — \"%s\""
			% deny_verdict,
		deny_text.contains(deny_verdict))
	# …and the launch-clock wording is nowhere on it: a party already out must not be told its collapse
	# band starts when it leaves. Asserted as the pairing negative to the claim above, since a clause
	# builder that emitted neither span would satisfy that one alone only by accident.
	h._assert_hud("…and never the FROM-LAUNCH span, which is the launch sheet's",
		not deny_text.contains(SourceForecast.DENIAL_SPAN_FROM_LAUNCH))
	# **THE HUNT-ONLY READOUTS ARE ABSENT, and that is the mission's specification.** Its
	# `expedition_floor` reads `0.0` because it HAS no such order; rendering the row would put a lever
	# on screen the command grammar cannot express.
	h._assert_hud("…and renders NO orders row (no floor) and NO delivery ETA",
		not deny_text.contains(DENIAL_ABSENT_ORDERS_KEY)
			and not deny_text.contains(DENIAL_ABSENT_DELIVERY_KEY))
	# It still states what it hauled home, which reads near-empty — the mission's own cost, and the
	# row a suppression would have hidden.
	h._assert_hud("…while still stating the little it carries",
		deny_text.contains(DENIAL_CARRIED_DETAIL_KEY))

	# **THE PNG-LESS HALF: the verdict's structure, driven directly.** None of these three can be seen
	# in a frame — each is about a sentence the fixtures above never produce — and each fails on a
	# DIFFERENT mutation.
	#
	# (a) A `repelled` outcome carrying a full turn band still quotes NO number. The party never gets
	# there, so a turn count would be a promise the sim did not make.
	var repelled := {
		"available": true, "outcome": SourceForecast.DENIAL_OUTCOME_REPELLED,
		"turns": 4, "low": 3, "high": 5, "animals": 0, "food": 0.0, "wasted": 0.0,
	}
	h._assert_hud("a repelled verdict names the PARTY's problem and quotes no turn count",
		SourceForecast.denial_verdict_text(repelled, DENIAL_TARGET_QUARRY)
			== SourceForecast.DENIAL_VERDICTS[
				SourceForecast.DENIAL_OUTCOME_REPELLED]["line"] % DENIAL_TARGET_QUARRY)
	# (b) **A BLANK TURN COUNT NEVER RENDERS WITHOUT ITS OUTCOME.** A `past_recovery` row the
	# projection bounded on neither end still names the outcome and simply appends no clause — the
	# whole reason the outcome LEADS the sentence and the number is a clause on it.
	var unbounded := {
		"available": true, "outcome": SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY,
		"turns": 0, "low": 0, "high": 0, "animals": 0, "food": 0.0, "wasted": 0.0,
	}
	h._assert_hud("a collapse the forecast cannot bound still names its outcome, with no bare number",
		SourceForecast.denial_verdict_text(unbounded, DENIAL_TARGET_QUARRY)
			== SourceForecast.DENIAL_VERDICTS[
				SourceForecast.DENIAL_OUTCOME_PAST_RECOVERY]["line"] % DENIAL_TARGET_QUARRY)
	# (c) The two degenerate bands the range must collapse: low == high reads as ONE number, and a
	# positive low beside a `0` high reads "on a good run" rather than promising the good draw.
	h._assert_hud("a degenerate band reads one number, and an unbounded expectation falls to the good run",
		SourceForecast.denial_turns_phrase({"low": 4, "high": 4, "turns": 4})
				== SourceForecast.DENIAL_TURNS_ONE_FORMAT % 4
			and SourceForecast.denial_turns_phrase({"low": 3, "high": 0, "turns": 0})
				== SourceForecast.DENIAL_TURNS_ONE_FORMAT % 3)
	# (d) **THE HORIZON VERDICT SAYS HOW LONG THE FORECAST IS, IN ITS OWN SPAN.** "Still standing when
	# the forecast runs out" names a clock the player cannot see — the same hedge the hunt sheet's
	# "away many turns" was — so where the cohort carries the lever the sentence quotes it. Two
	# spans, one lever, asserted by EQUALITY against sentences spelled out HERE: the in-flight drawer
	# has no band and so states the RAIDING turns unshifted, while a launch sheet adds the outbound
	# walk and says "from launch". The pair is the claim — a builder that ignored `travel` satisfies
	# the first alone, and one that always shifted satisfies the second alone.
	var horizon_in_flight := {
		"available": true, "outcome": SourceForecast.DENIAL_OUTCOME_HORIZON,
		"turns": 0, "low": 0, "high": 0, "animals": 0, "food": 0.0, "wasted": 0.0,
		SourceForecast.DENIAL_TRAVEL_KEY: SourceForecast.DENIAL_TRAVEL_UNKNOWN,
		SourceForecast.DENIAL_HORIZON_TURNS_KEY: DENIAL_HORIZON_TURNS,
	}
	var horizon_from_launch := horizon_in_flight.duplicate()
	horizon_from_launch[SourceForecast.DENIAL_TRAVEL_KEY] = DENIAL_HORIZON_OUTBOUND_TURNS
	h._assert_hud("a horizon verdict states the forecast's LENGTH, in the span it is quoting",
		SourceForecast.denial_verdict_text(horizon_in_flight, DENIAL_TARGET_QUARRY)
				== "%s is still standing after %d turns of raiding" % [
					DENIAL_TARGET_QUARRY, DENIAL_HORIZON_TURNS]
			and SourceForecast.denial_verdict_text(horizon_from_launch, DENIAL_TARGET_QUARRY)
				== "%s is still standing after %d turns from launch" % [
					DENIAL_TARGET_QUARRY, DENIAL_HORIZON_TURNS + DENIAL_HORIZON_OUTBOUND_TURNS])
	# …and with no lever on the wire it keeps the hedge rather than quoting a zero — the one reading
	# worse than "when the forecast runs out".
	var horizon_no_lever := horizon_in_flight.duplicate()
	horizon_no_lever[SourceForecast.DENIAL_HORIZON_TURNS_KEY] = SourceForecast.FORECAST_HORIZON_UNKNOWN
	h._assert_hud("…and falls back to the hedge where the cohort carries no horizon at all",
		SourceForecast.denial_verdict_text(horizon_no_lever, DENIAL_TARGET_QUARRY)
			== String(SourceForecast.DENIAL_VERDICTS[
				SourceForecast.DENIAL_OUTCOME_HORIZON]["line"]) % DENIAL_TARGET_QUARRY)

	# State 1k — the hunt launch policy picker: an idle band (short allocation panel) showing the
	# "Send expedition" outfit block — the party stepper, the scout + hunt send buttons, and the hunt
	# POLICY radio (DEPLETE selected) with its EXPEDITION hint. The expedition hints must never promise
	# HUSBANDRY — the Hunting arm accrues none — so Deplete's line frames the rung by the PRESSURE it
	# applies (relaunching trip after trip)
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
	# builds husbandry — an expedition accrues none.
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

	# **HAND THE REFERENCE BAND BACK, exactly as the retired FILL-TARGET block did on its way out.**
	# Four raid frames stood here until issue #491 removed the lever they showed, and their tail put
	# `_player_band` / `_player_bands` back where the rest of this chapter's walk expects them. Every
	# state that follows renders into the SAME long-lived `HudLayer`, and `update_band_alerts` keeps a
	# losing-population diff against the last roster pushed — so deleting the block without its restore
	# moves frames in later chapters for a reason that has nothing to do with the lever. Measured: it
	# moved the three `band_kit_*` frames, which come back byte-identical with this restore in place.
	h._hud._compose.reset_hunt_source()
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	await h._settle()

	# ---- THE THREE KITS (`docs/plan_hunt_through_combat.md` §4.8) --------------------------------
	await _kit_states()

	# band_alerts (above) left _player_band as an alert-fixture band (no work_range, far from the food
	# tile); seed a NEAR band so the forage controls resolve an in-range actor.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)


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

	# State kit-d — **TEN SPEARS AMONG SEVENTEEN HUNTERS** (issue #520). Every item is live and at the
	# same wear as the equipped band far above, so a readout that only ever asks how much LIFE is left
	# renders this band and a fully-armed one identically — which is exactly what shipped. What
	# separates them is how far each item REACHES.
	var short := BandFx.with_short_spears(BandFx.band_fixture())
	h._hud._band_labor._player_band = short
	h._hud.show_unit_selection(short)
	await h._settle()
	_click_disclosure(BAND_DISCLOSURE_KIT)
	await h._settle()
	await h._save("band_kit_short")
	# **THE ROW STATES THE FRACTION AND THE POPOVER STATES THE SENTENCE**, the split the whole Kit
	# readout is built on — the row says which side of a line the item is on, the popover has room to
	# say what that costs. Asserted as a pair, because a row that lost its marker still leaves a
	# perfectly plausible popover behind it and vice versa.
	var short_face := DetailFormat.KIT_COVERAGE_ROW_FORMAT % [
		String.num(BandFx.KIT_CONDITION_SPEARS, DetailFormat.KIT_CONDITION_DECIMALS),
		int(BandFx.KIT_SHORT_SPEARS_ARMED), int(BandFx.KIT_HUNT_HEADCOUNT)]
	h._assert_hud("the Gear row states the SPEARS' shortfall on their own face: %s" % short_face,
		String(h._hud.occupant_detail.get_parsed_text()).contains(short_face))
	# **AND THE SLED, WHICH GOES ROUND, STATES NOTHING.** Both items are held by the hunt crews and
	# only one is short, so a coverage clause rendered unconditionally — or one keyed on the band
	# rather than on the item — fails here and nowhere else.
	var short_popover := _kit_popover_text()
	var short_spears_line := _kit_breakdown_line(short_popover, DetailFormat.KIT_LABEL_SPEARS)
	var short_sled_line := _kit_breakdown_line(short_popover, DetailFormat.KIT_LABEL_SLED)
	var coverage_clause := DetailFormat.KIT_COVERAGE_BREAKDOWN_FORMAT % [
		int(BandFx.KIT_SHORT_SPEARS_ARMED), int(BandFx.KIT_HUNT_HEADCOUNT)]
	h._assert_hud("…and the popover says how few of them carry one:%s" % coverage_clause,
		short_spears_line.contains(coverage_clause)
			and not short_sled_line.contains(coverage_clause))
	# **A SHORT ITEM IS NOT A SPENT ONE.** The spears work perfectly for whoever holds them, so the row
	# must not borrow the cliff's wording — that word means the role has stepped down for good.
	h._assert_hud("…and a SHORT kit is never called bare hands — it works for whoever holds it",
		not short_spears_line.contains(DetailFormat.KIT_BARE_HANDS_SUFFIX))
	# **THE UNSTAFFED JOB SAYS NOTHING, and it is asserted on the SAME frame as a live shortfall** —
	# the two zeros are one glance apart and only a frame carrying both can show the readout telling
	# them apart. This band keeps no pen, so its handling gear reaches nobody because nobody needed
	# it; a reader that divided by that head count, or read the numerator alone, would light up a
	# perfectly sound item at `0 of 0`.
	var keeper_line := _kit_breakdown_line(short_popover, DetailFormat.KIT_LABEL_HUSBANDRY_GEAR)
	h._assert_hud("a job NOBODY is staffed on states no shortfall — 0 of 0 is not a warning",
		not keeper_line.contains(DetailFormat.KIT_COVERAGE_SHORT_NEEDLE)
			and keeper_line.contains(String.num(BandFx.KIT_CONDITION_HUSBANDRY_GEAR,
				DetailFormat.KIT_CONDITION_DECIMALS)))
	h._assert_hud("…and it keeps the SOUND glyph, where the short spears wear the warning one",
		keeper_line.contains(DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH)
			and short_spears_line.contains(DetailFormat.MORALE_CONTRIB_NEGATIVE_GLYPH))
	_click_disclosure(BAND_DISCLOSURE_KIT)
	_assert_faction_kit_counts_the_short_band(short)

	# State kit-e — **TWO BASKETS AMONG FOUR GATHERERS** (issue #520, the four-job denominator). The
	# hunt is perfectly equipped here and every item is at full condition, so until each row carried
	# its OWN job's head count this band was unreadable: `Σ huntCrews.workers` is the hunt's number and
	# says nothing whatever about a basket.
	var forage_short := BandFx.with_short_baskets(BandFx.band_fixture())
	h._hud._band_labor._player_band = forage_short
	h._hud.show_unit_selection(forage_short)
	await h._settle()
	_click_disclosure(BAND_DISCLOSURE_KIT)
	await h._settle()
	await h._save("band_kit_forage_short")
	var forage_face := DetailFormat.KIT_COVERAGE_ROW_FORMAT % [
		String.num(BandFx.KIT_CONDITION_BASKETS, DetailFormat.KIT_CONDITION_DECIMALS),
		int(BandFx.KIT_SHORT_BASKETS_HOLDING), int(BandFx.KIT_FORAGE_HEADCOUNT)]
	h._assert_hud("the BASKETS state their shortfall against the FORAGE head count: %s" % forage_face,
		String(h._hud.occupant_detail.get_parsed_text()).contains(forage_face))
	# **AND THE HUNT IS NOT DRAGGED IN WITH IT.** A client that kept the hunt's head count as every
	# job's denominator states this band's spears as `17 of 17` — silent — and its baskets as `2 of 17`,
	# so asserting the basket fraction alone would pass on the wrong denominator too. The spears
	# saying NOTHING is what pins that the two rows were divided by different numbers.
	var forage_popover := _kit_popover_text()
	h._assert_hud("…and the perfectly-equipped SPEARS say nothing on the same band",
		not _kit_breakdown_line(forage_popover, DetailFormat.KIT_LABEL_SPEARS).contains(
			DetailFormat.KIT_COVERAGE_SHORT_NEEDLE))
	h._assert_hud("…and the popover states it in the four-job wording, never 'hunters'",
		_kit_breakdown_line(forage_popover, DetailFormat.KIT_LABEL_BASKETS).contains(
			DetailFormat.KIT_COVERAGE_BREAKDOWN_FORMAT % [
				int(BandFx.KIT_SHORT_BASKETS_HOLDING), int(BandFx.KIT_FORAGE_HEADCOUNT)]))
	_click_disclosure(BAND_DISCLOSURE_KIT)
	h._hud._band_labor._player_band = BandFx.band_fixture()

## **THE FACTION PAGE COUNTED DRY BANDS ALONE, so a partly-armed band read as equipped there** — the
## reassuring-direction error this whole issue exists to remove, on the surface a player uses to find
## WHICH band needs gear (issue #520).
##
## **DRIVEN AND PNG-LESS, on `FactionRollup._kit_line` directly.** That page is `band_panel_preview`'s
## to render, and staging a short band in its roster would move a frame for a claim that is one string
## — while the rollup itself is a pure function of the band list, so the honest test is to hand it two
## lists. **Asserted as a PAIR**: the alert alone passes on a rollup that flags every band, and
## `all equipped` alone passes on the bug fully restored.
func _assert_faction_kit_counts_the_short_band(short: Dictionary) -> void:
	var sound := BandFx.with_equipped_kit(BandFx.band_fixture())
	var sound_line := FactionRollup._kit_line([sound], h._hud._disclosures)
	var short_line := FactionRollup._kit_line([short], h._hud._disclosures)
	h._assert_hud("a faction of soundly-equipped bands still reads '%s'"
		% HudWorkVocab.FACTION_KIT_ALL_EQUIPPED,
		sound_line.contains(HudWorkVocab.FACTION_KIT_ALL_EQUIPPED))
	h._assert_hud("…and a band SHORT of a kit is counted, not reported as equipped",
		short_line.contains(HudWorkVocab.FACTION_ALERT_GLYPH)
			and not short_line.contains(HudWorkVocab.FACTION_KIT_ALL_EQUIPPED))

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
