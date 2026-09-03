extends RefCounted

## The band drawer and the expedition panels.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

## The checkpoints this chapter owes the walk — assertions made plus frames saved, as a FLOOR.
## See `ui_preview.gd`'s `CHAPTER_EXPECTED_CHECKPOINTS` for what it catches and why it lives here.
const EXPECTED_CHECKPOINTS := 104

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const WorldFx := preload("res://tools/ui_preview/fixtures_world.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
## `Main`'s own reservation publisher, for the one state here that docks the Band/City panel — the
## `tile_panel` / `trade` convention, so a later re-dock cannot leave this harness fanning out by a
## rule the client stopped using.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
const BAND_PANEL_RESERVER := &"band_panel"

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# The pen-keeping band's entity id — its own, so its Food disclosure key (`food:<entity>`) doesn't
# collide with the reference band's.
const PEN_KEEPER_BAND_ENTITY := 906

## What `_pen_keeper_band_fixture`'s ledger must come out at, written down rather than recomputed:
## income 5.88 (forage 0.48 + the pen's 5.40) − the people's 1.15, and NOTHING for the animals. The
## fixture paid a third term of 1.74 until the pen's food bill was retired, so the number that would
## catch a regression is +2.99, and that is precisely why this is stated as an answer.
const PEN_KEEPER_EXPECTED_NET := 4.73

## Gathered · Hunted · Eaten. A FOURTH row on this band would be the animal-feed row coming back.
const PEN_KEEPER_BREAKDOWN_ROWS := 3

## Float slack for the ledger identity — the terms are wire floats summed once, so this is a
## comparison tolerance and not a rounding rule.
const LEDGER_EPSILON := 0.005

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

## A band that KEEPS A CORRAL, and **the frame that proves keeping one costs the LARDER NOTHING.**
## Its one keeper works the penned Red Deer herd, so the sim pays the pen's managed yield (5.40) into
## the band's income — and there is no answering debit anywhere on the ledger, because the herd eats
## its fenced footprint's grass and the hay in the FODDER store, never the people's food. The ledger
## is therefore the same two terms an ordinary band's is: net = 5.88 − 1.15 = +4.73, with the pen a
## pure credit. It carried a third `pen_feed_upkeep` term of 1.74 until that field was retired.
func _pen_keeper_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["entity"] = PEN_KEEPER_BAND_ENTITY
	band["id"] = "Band 4"
	band["turns_of_food"] = 22.0
	band["food_income"] = 5.88          # forage 0.48 + the pen's gross 5.40
	band["food_consumption"] = 1.15     # the PEOPLE's meals — the ONLY standing debit
	band["fodder_store"] = 12.4         # the band's HAY larder (Flora roster F3) — what feeds the pen
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "floor": 0.5, "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1},
		# A managed source: one keeper, take == sustainable (escapement); Corral is managed, so the
		# sim-answered `overdraws` is false → no ⚠ and no overstaff note.
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "floor": 0.5, "improvement": "corral", "target_x": 70, "target_y": 17, "actual_yield": 5.40, "sustainable_yield": 5.40, "workers_needed": 1, "overdraws": false},
		{"kind": "scout", "workers": 2},
	]
	return band

## The SAME pen, STARVING: its pasture and its hay together covered only 40% of what it demanded, so
## the herd is shrinking and its yield with it (gross down to 0.84). **The band's LARDER shows none of
## that as a debit** — the shortfall is borne by the animals, not by the people — so the only trace on
## this ledger is the income falling out from under it: net = 1.32 − 1.15 = +0.17, a band whose pen is
## dying while its own books still balance. That asymmetry is exactly why the alarm lives on the herd
## drawer (`herd_corral_starving`) and not here.
func _starving_pen_band_fixture() -> Dictionary:
	var band := _pen_keeper_band_fixture()
	band["turns_of_food"] = 3.0
	band["food_income"] = 1.32          # forage 0.48 + the shrunken pen's 0.84
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
		# High-latitude cold ~-2° → "Polar" climate band (neutral Tile-card chip) — and, since #614,
		# the ⚠ and the DANGER tint ON that same chip: -2 °C is 2 ° past the 0 °C cold onset, 0.35 %/turn. That is NOT
		# incidental to this frame. The band is here BECAUSE the ground is punishing it (morale 22%,
		# `morale_climate` -0.8% "harsh climate"), so the temperature is load-bearing and the pill is
		# the missing half of the same story — the drawer says climate is dragging morale down, and
		# the chip strip now says the same climate is also killing people outright.
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
	# it. NOTHING may call `toggle_victory` before this point — that is the whole value of the state.
	# The right dock must be EMPTY of its reference card. This state is FIRST on purpose, so no later
	# state can leak into it. (The Terrain Types legend it also used to guard is retired — the map's
	# legend is the minimap picker's own popover now, and a popover has no dock-visibility default to
	# protect.)
	h._hud.update_victory_state(WorldFx.victory_state_fixture())
	await h._settle()
	await h._save("dock_fresh_profile_default")
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

	# State 1-food-c — a band KEEPING A PEN (docs/plan_corral_managed_population.md), and the frame
	# that shows a pen costing the ledger NOTHING. The corral grosses 5.40 into income and the people
	# eat 1.15; there is no `🐄 Pen feed (animals)` row, because human food is not animal feed and the
	# pen draws on its own pasture and the hay store instead. Net = 5.88 − 1.15 = +4.73. Breakdown
	# popover open, so the ABSENCE of that row is visible rather than merely asserted.
	h._hud.show_unit_selection(_pen_keeper_band_fixture())
	await h._settle()
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)
	await h._settle()
	await h._save("band_pen_keeper")
	# **THE LEDGER RECONCILES, AND IT NOW HAS THREE TERMS RATHER THAN FOUR.** A row that quietly
	# reappears — or a headline that stops equalling the rows beneath it — is invisible in a PNG, and
	# this is the fixture with a pen on it, so it is the one where a resurrected `🐄 Pen feed (animals)`
	# row would show. Both halves are claimed: the ARITHMETIC identity
	# `net == income − consumption − raid_forfeit`, evaluated against the fixture's own numbers rather
	# than against a re-run of the code under test, and the ABSENCE of any animal-feed row from the
	# breakdown the popover above just drew.
	var pen_band := _pen_keeper_band_fixture()
	h._assert_hud("a pen-keeping band's food headline is income − eaten, with no third term",
		absf(DetailFormat.band_net_food(pen_band) - PEN_KEEPER_EXPECTED_NET) < LEDGER_EPSILON)
	var pen_breakdown: Array[String] = h._hud._disclosures.food_breakdown_lines(pen_band)
	var pen_feed_row_found := false
	for line in pen_breakdown:
		if String(line).contains(DetailFormat.CORRAL_GLYPH):
			pen_feed_row_found = true
	h._assert_hud("…and its breakdown itemizes NO animal-feed row — the pen bills the larder nothing",
		not pen_feed_row_found and pen_breakdown.size() == PEN_KEEPER_BREAKDOWN_ROWS)
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)

	# State 1-food-d — the same pen, STARVING: the pen's pasture and hay covered only 40% of its demand,
	# so the herd wastes away and the band's INCOME falls with it — 5.88 down to 1.32. That collapsing
	# income is the ONLY trace the food ledger carries; the alarm itself lives on the herd drawer (see
	# `herd_corral_starving`), which is the readout that can say the animals are the ones going short.
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

	# ---- THE BAND'S HAY LEDGER --------------------------------------------------------------------
	await _hay_ledger_states()

	# ---- THE BAND'S STANDING MATERIAL BILL (`docs/plan_standing_upkeep.md` §2.7) ------------------
	# **APPENDED, never inserted.** States render into one long-lived `HudLayer`, so a block moved is a
	# set of frames changed — and this one deliberately follows the two ledgers it is the third of.
	await _standing_bill_states()

	# band_alerts (above) left _player_band as an alert-fixture band (no work_range, far from the food
	# tile); seed a NEAR band so the forage controls resolve an in-range actor.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)


# ---- THE THREE KITS (`docs/plan_hunt_through_combat.md` §4.8) ------------------------------------
#
# ⛔ **THE BAND'S `Gear` ROW AND ITS FIVE FRAMES ARE RETIRED** (`docs/plan_standing_upkeep.md` §4.9
# item 12) — `band_kit`, `band_kit_expanded`, `band_kit_bare`, `band_kit_short`,
# `band_kit_forage_short`, `BAND_DISCLOSURE_KIT` and the three ROW claims they carried (the row states
# all three kits; a spent kit reads as a WORD in danger ink; the row states the spears' shortfall on
# their own face). The row is gone from the band page, so a frame of it would render a surface that no
# longer exists.
#
# **THE POPOVER CLAIMS SURVIVE, AND THEY ARE THE ONES WORTH KEEPING.** Every cross-check the three-kit
# split exists for is about `DisclosureController.kit_breakdown_lines` — the sled quoting the HUNT's
# carry and the baskets the FORAGE web's, a dry kit's cliff sentence, a SHORT kit's coverage clause,
# an unstaffed job's silent `0 of 0` — and that producer is untouched: the crafting panel's kit ledger
# and the compose sheet's role hint both still read it. So this block is DRIVEN and PNG-less now,
# asserting on the producer directly, which is what `compose_rungs.gd`'s own gear claims already do.
#
# **NOTHING HERE MAY BE SCALED BY THE REMAINING CONDITION.** Durability and performance are
# orthogonal — a kit at 3 performs exactly as one at 97 and then stops — so the assertions below pin
# the TIER against its shipped constant rather than against anything derived from the condition, and
# a readout that drew a gradient would fail them at every condition but full.
func _kit_states() -> void:
	# State kit-a — ONE KIT DRY, the other two intact.
	var worn := BandFx.with_baskets_dry(BandFx.band_fixture())
	h._hud._band_labor._player_band = worn
	h._hud.show_unit_selection(worn)
	await h._settle()

	# State kit-b — the SAME band, breakdown read. **This frame carries the cross-check the whole
	# three-kit split exists for**: the sled's line must quote the HUNT's carry and the basket's line
	# the FORAGE web's, and neither may quote the other's. Baskets boosting the hunt is precisely the
	# defect slice 5 corrected in the sim, and rendering one tier on the other's row would carry it
	# straight back into the UI where no sim test can see it.
	var kit_popover := _kit_breakdown_text(worn)
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

	# State kit-c — EVERY kit run dry. Bare hands is a state worth showing plainly: there is no
	# replenishment path, so all three roles have stepped down and stay there.
	var bare := BandFx.with_bare_hands(BandFx.band_fixture())
	h._hud._band_labor._player_band = bare
	h._hud.show_unit_selection(bare)
	await h._settle()
	var bare_popover := _kit_breakdown_text(bare)
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

	# **THE NEGATIVE HALF, and without it every claim above is satisfied by a producer that emits
	# unconditionally.** A band that states no kit at all — every fixture predating the TOE, and the
	# state a rehydrated cohort is in — must yield NO breakdown, because a defaulted `Spears 0` would
	# report equipment destroyed that was never there. **This is what the retired `Gear` row's own
	# absence claim became**: the row is gone, so the assertion moved to the producer the crafting
	# panel and the compose sheet still read.
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	h._assert_hud("a band that states no kit yields no breakdown — never a defaulted zero",
		_kit_breakdown_text(BandFx.band_fixture()) == "")

	# State kit-d — **TEN SPEARS AMONG SEVENTEEN HUNTERS** (issue #520). Every item is live and at the
	# same wear as the equipped band far above, so a readout that only ever asks how much LIFE is left
	# renders this band and a fully-armed one identically — which is exactly what shipped. What
	# separates them is how far each item REACHES.
	var short := BandFx.with_short_spears(BandFx.band_fixture())
	h._hud._band_labor._player_band = short
	h._hud.show_unit_selection(short)
	await h._settle()
	# ⛔ **THE ROW HALF OF THIS PAIR IS RETIRED WITH THE `Gear` ROW** — it asserted
	# `KIT_COVERAGE_ROW_FORMAT` against the rendered vitals block, and that surface no longer exists.
	# The popover half below is the whole claim now.
	#
	# **AND THE SLED, WHICH GOES ROUND, STATES NOTHING.** Both items are held by the hunt crews and
	# only one is short, so a coverage clause rendered unconditionally — or one keyed on the band
	# rather than on the item — fails here and nowhere else.
	var short_popover := _kit_breakdown_text(short)
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
	var keeper_line := _kit_breakdown_line(short_popover, DetailFormat.KIT_LABEL_CROOK)
	h._assert_hud("a job NOBODY is staffed on states no shortfall — 0 of 0 is not a warning",
		not keeper_line.contains(DetailFormat.KIT_COVERAGE_SHORT_NEEDLE)
			and keeper_line.contains(String.num(BandFx.KIT_CONDITION_CROOK,
				DetailFormat.KIT_CONDITION_DECIMALS)))
	h._assert_hud("…and it keeps the SOUND glyph, where the short spears wear the warning one",
		keeper_line.contains(DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH)
			and short_spears_line.contains(DetailFormat.MORALE_CONTRIB_NEGATIVE_GLYPH))

	# State kit-e — **TWO BASKETS AMONG FOUR GATHERERS** (issue #520, the four-job denominator). The
	# hunt is perfectly equipped here and every item is at full condition, so until each row carried
	# its OWN job's head count this band was unreadable: `Σ huntCrews.workers` is the hunt's number and
	# says nothing whatever about a basket.
	var forage_short := BandFx.with_short_baskets(BandFx.band_fixture())
	h._hud._band_labor._player_band = forage_short
	h._hud.show_unit_selection(forage_short)
	await h._settle()
	# ⛔ **THE ROW HALF IS RETIRED WITH THE `Gear` ROW** — it asserted `KIT_COVERAGE_ROW_FORMAT` on the
	# rendered vitals block. The DENOMINATOR claim it made survives whole in the popover form two
	# assertions down (`KIT_COVERAGE_BREAKDOWN_FORMAT` over `KIT_FORAGE_HEADCOUNT`), which is the same
	# number asked of the same producer.
	#
	# **AND THE HUNT IS NOT DRAGGED IN WITH IT.** A client that kept the hunt's head count as every
	# job's denominator states this band's spears as `17 of 17` — silent — and its baskets as `2 of 17`,
	# so asserting the basket fraction alone would pass on the wrong denominator too. The spears
	# saying NOTHING is what pins that the two rows were divided by different numbers.
	var forage_popover := _kit_breakdown_text(forage_short)
	h._assert_hud("…and the perfectly-equipped SPEARS say nothing on the same band",
		not _kit_breakdown_line(forage_popover, DetailFormat.KIT_LABEL_SPEARS).contains(
			DetailFormat.KIT_COVERAGE_SHORT_NEEDLE))
	h._assert_hud("…and the popover states it in the four-job wording, never 'hunters'",
		_kit_breakdown_line(forage_popover, DetailFormat.KIT_LABEL_BASKETS).contains(
			DetailFormat.KIT_COVERAGE_BREAKDOWN_FORMAT % [
				int(BandFx.KIT_SHORT_BASKETS_HOLDING), int(BandFx.KIT_FORAGE_HEADCOUNT)]))
	h._hud._band_labor._player_band = BandFx.band_fixture()

## **THE FACTION `Kit` ROW AND ITS SHORT-BAND COUNT ARE RETIRED** (`docs/plan_standing_upkeep.md`
## §4.9 item 12) with `_assert_faction_kit_counts_the_short_band`. The row was an alert and a
## drill-down over durabilities that never aggregated; the crafting panel's kit ledger states the
## items in full, and what replaced the `⚠ 1 band` → *which band* path is the event dock's
## `material_shortfall` Alert, which names the band and carries a jump to it.


# ---- THE BAND'S STANDING MATERIAL BILL (`docs/plan_standing_upkeep.md` §2.7) ---------------------
#
# **WORK WAS NEVER THE WHOLE PRICE, and the drawer said nothing about the other half.** A pen frays
# its fence every turn it stands; a road washes out. This is the row that says so, and it is the
# Fodder row beat for beat — one summary naming the good in the worst state, with the three terms
# that explain it behind a caret.
#
# **IT TOOK THE `Gear` ROW'S HEIGHT**, which is why the block above it lost five frames and this one
# gains two: net zero rows in every tier, and the tier keeps a fact rather than trading one away.

## The good the shipped ladder eats (`animal:pen`'s `upkeep.materials`), and this band's three terms
## written as the ANSWERS the row has to come out at: a 0.03-a-turn gap against a shelf of 2 is 66
## turns of runway. **Comfortably clear of the warn line, deliberately** — the alarm is the faction
## page's and the event dock's, and this frame is the row in its ordinary state.
const BILL_MATERIAL := "hurdles"
const BILL_NEED := 0.05
const BILL_INCOME := 0.02
const BILL_STORE := 2.0
## …and the SHIPPED DEFAULT'S own income, which is none at all. Material income is empty on every
## band the sim produces today, so `0.0` is not an edge of this readout — it is its ordinary state,
## and `BILL_INCOME` above is the one that cannot happen yet. On the strict `> 0` boundary the row it
## drew was `▼ -0  Arriving` in WARN amber: a debit sign, a debit glyph and a warning ink, all three
## over the number zero. Kept as its own const rather than folded into the block above because the
## block above is the row in its ordinary MID-CLIMB state and both readings are worth a frame.
const BILL_INCOME_NONE := 0.0
## Its disclosure key — `DetailFormat.breakdown_key(kind, band)`'s shape over the bill band's own
## entity, so the caret cannot collide with the reference band's Food / Morale / Growth popovers.
const BAND_DISCLOSURE_UPKEEP := "upkeep:908"

## A band that HOLDS something which eats a good. Its own entity, for the disclosure key's sake.
func _standing_bill_band_fixture() -> Dictionary:
	var band := BandFx.band_fixture()
	band["entity"] = 908
	band["id"] = "Band 12"
	band["material_upkeep_need"] = [{"material_id": BILL_MATERIAL, "amount": BILL_NEED}]
	band["material_upkeep_income"] = [{"material_id": BILL_MATERIAL, "amount": BILL_INCOME}]
	band["material_store"] = [{"material_id": BILL_MATERIAL, "amount": BILL_STORE}]
	return band

func _standing_bill_states() -> void:
	var owing := _standing_bill_band_fixture()
	h._hud._band_labor._player_band = owing
	h._hud.show_unit_selection(owing)
	await h._settle()
	await h._save("band_standing_bill")
	var vitals := String(h._hud.occupant_detail.get_parsed_text())
	# **THE ROW NAMES ONE GOOD AND ITS SHELF.** The needle carries the STOCK as well as the label, so
	# it cannot be satisfied by a row that drew the key over a defaulted reading — and it names a GOOD
	# rather than a total, which is what keeps this from being the retired `Trade:` scalar rebuilt out
	# of its own replacement.
	var want := "%s %s" % [DetailFormat.format_trimmed(BILL_STORE, DetailFormat.MATERIAL_BILL_DECIMALS), BILL_MATERIAL]
	h._assert_hud("the band's Upkeep row states the good and its shelf — \"%s\"" % want,
		vitals.contains(HudDisclosureVocab.DETAIL_ROW_UPKEEP) and vitals.contains(want))
	# **AND THE RUNWAY BESIDE IT**, which is the whole second term: a shelf with no runway says how
	# much you have and never how long it lasts. 2 / (0.05 − 0.02) = 66 turns, written out here rather
	# than asked of the code under test.
	h._assert_hud("…and the runway the shelf buys against the gap (%s)"
			% DetailFormat.food_turns_text(BILL_STORE / (BILL_NEED - BILL_INCOME)),
		vitals.contains(DetailFormat.food_turns_text(BILL_STORE / (BILL_NEED - BILL_INCOME))))

	# …and the drill-down, driven through the REAL `meta_clicked` the row's own text carries.
	_click_disclosure(BAND_DISCLOSURE_UPKEEP)
	await h._settle()
	await h._save("band_standing_bill_expanded")
	var popover := _popover_text()
	h._assert_hud("the Upkeep caret opens a card headed by the good (%s)"
		% DetailFormat.material_bill_heading(BILL_MATERIAL),
		popover.contains(DetailFormat.material_bill_heading(BILL_MATERIAL)))
	# **THREE TERMS, AND THE SUMMARY STATED ONE OF THEM.** A breakdown that merely restated the row
	# would leave the question it exists for — *is this arriving as fast as it goes* — unanswered.
	for term in [
		[DetailFormat.MATERIAL_LABEL_WANTED, BILL_NEED],
		[DetailFormat.MATERIAL_LABEL_ARRIVING, BILL_INCOME],
		[DetailFormat.MATERIAL_LABEL_STORE, BILL_STORE],
	]:
		var label := String(term[0])
		var amount := DetailFormat.format_trimmed(float(term[1]), DetailFormat.MATERIAL_BILL_DECIMALS)
		h._assert_hud("…its %s term reads %s" % [label, amount],
			_kit_breakdown_line(popover, label).contains(amount))
	_click_disclosure(BAND_DISCLOSURE_UPKEEP)

	# **…AND THE SAME BAND WITH NOTHING ARRIVING**, which is what every band on the shipped sim looks
	# like: no bench finishes hurdles yet, so the Arriving term is exactly `0`. A zero income is not a
	# debit — `SourceForecast._rate_sign`'s rule, and the same one the food and fodder rows beside this
	# one have always used. The WHOLE row is compared rather than a `contains`: sign, glyph and label are
	# three separate decisions and a needle that checked one of them would pass while the other two
	# stayed wrong. Composed from the format constants and the fixture number, never by asking
	# `material_bill_row` what it thinks it prints.
	#
	# ⛔ **PNG-LESS AND UN-DRIVEN, WHICH IS NOT THE DEFAULT AND IS MEASURED.** Staged as a fourth
	# SELECTION with its own disclosure clicks, this state left `compose_band_switch_forage` — eight
	# chapters downstream — pressing its `Band:` picker into a popup that had been freed under the
	# probe, five failures deep, reproducibly. The claims here are about a PRODUCER and a FORMATTER, so
	# nothing about them needs a selection at all; asking the two directly makes the state cost the walk
	# no HUD state and no frames.
	var dry := _standing_bill_band_fixture()
	dry["material_upkeep_income"] = [{"material_id": BILL_MATERIAL, "amount": BILL_INCOME_NONE}]
	var dry_want := "%s%s %s%s  %s" % [DetailFormat.MORALE_BREAKDOWN_INDENT,
		DetailFormat.MORALE_CONTRIB_POSITIVE_GLYPH,
		SourceForecast.RATE_SIGN_POSITIVE,
		DetailFormat.format_trimmed(BILL_INCOME_NONE, DetailFormat.MATERIAL_BILL_DECIMALS),
		DetailFormat.MATERIAL_LABEL_ARRIVING]
	var dry_lines: Array[String] = h._hud._disclosures.material_upkeep_breakdown_lines(dry)
	var dry_row := _kit_breakdown_line("\n".join(dry_lines), DetailFormat.MATERIAL_LABEL_ARRIVING)
	h._assert_hud("nothing arriving reads \"%s\" — a credit of zero, never a debit of it"
			% dry_want.strip_edges(), dry_row == dry_want)
	# **AND THE INK FOLLOWS THE GLYPH**, which is the third of the three and the one a parsed string
	# cannot see: `detail_bbcode` picks the tint off the ▲/▼, so the claim is made on the MARKUP that
	# renderer produces. Amber here would be the HUD warning a player about an account that is simply
	# not moving.
	h._assert_hud("…and it is tinted HEALTHY, not the WARN amber a debit takes",
		DetailFormat.detail_bbcode(dry_lines).contains(
			"[color=#%s]%s[/color]" % [HudStyle.HEALTHY_HEX, dry_want]))

	# **THE NEGATIVE HALF — a band that owes no good draws NO row and registers NO caret.** Without it
	# both frames above pass on a producer that emits unconditionally. It is the one place this readout
	# parts company with Fodder, which keeps a dormant dash: a standing bill is a CONSEQUENCE of what
	# you have built, so there is no *"you could have this"* story a dormant form would tell.
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud.show_unit_selection(BandFx.band_fixture())
	await h._settle()
	h._assert_hud("a band that owes no good draws no Upkeep row at all",
		Readout.detail_excerpt(h._hud.occupant_detail.text, HudDisclosureVocab.DETAIL_ROW_UPKEEP)
			== Readout.DETAIL_EXCERPT_ABSENT)
	h._assert_hud("…and registers no caret for it either",
		not h._hud._disclosures.state().has(HudDisclosureVocab.DETAIL_ROW_UPKEEP))


## GUARD: **the Fodder summary fits the drawer column on ONE line** — the wrap this reshape exists to
## remove. It is invisible in a PNG (two lines of a rendered vitals block look exactly like two rows)
## and invisible to a `contains`, so it is MEASURED: the row's natural, unwrapped run in the label's
## own font at its own size, plus the table gutter, against the width the label was actually given.
##
## The run is cut out of the parsed text by the row that FOLLOWS it rather than by a newline, because
## `[table]` rows carry none — see `HAY_ROW_FOLLOWERS`. The label is found by the very `[url]` meta
## the Fodder row carries, so the measurement cannot be taken on some other surface's label.
func _assert_fodder_row_fits() -> void:
	var meta := HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + HAY_DISCLOSURE_FODDER
	var drawer := _find_meta_label(h._hud, meta)
	if drawer == null:
		h._fail("the Fodder wrap guard found no drawer label offering '%s'" % meta)
		return
	var text := drawer.get_parsed_text()
	var start := text.find(HAY_ROW_NEEDLE)
	if start < 0:
		h._fail("the Fodder wrap guard found no Fodder row in the drawer (got: %s)" % text)
		return
	var stop := text.length()
	for follower_variant in HAY_ROW_FOLLOWERS:
		var at := text.find(String(follower_variant), start + HAY_ROW_NEEDLE.length())
		if at > start:
			stop = mini(stop, at)
	var run := text.substr(start, stop - start)
	var font := drawer.get_theme_font(DRAWER_FONT_THEME_KEY)
	var font_size := drawer.get_theme_font_size(DRAWER_FONT_SIZE_THEME_KEY)
	var gutter := float(drawer.get_theme_constant(DRAWER_TABLE_SEPARATION_THEME_KEY))
	var needed: float = font.get_string_size(run, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x + gutter
	var available := drawer.size.x
	h._assert_hud("the Fodder summary fits the drawer on one line — \"%s\" measures %.0fpx of a %.0fpx column"
		% [run, needed, available], needed <= available)

## The open breakdown popover's text — the RENDERED disclosure, not the producer's return, so the
## assertions above cover the click, the payload stash and the popover's own restate.
func _popover_text() -> String:
	var label = h._hud._disclosures._breakdown_popover_label
	return "" if label == null else (label as RichTextLabel).get_parsed_text()

## The same popover as RAW BBCODE. `detail_bbcode` decides a breakdown row's ink from the sign glyph
## it carries, and `get_parsed_text` above has already thrown the `[color=…]` runs away — so a claim
## about the TINT can only be made here.
func _popover_markup() -> String:
	var label = h._hud._disclosures._breakdown_popover_label
	return "" if label == null else (label as RichTextLabel).text

## The band's kit breakdown as one string — the PRODUCER's return, read directly. The `Gear` row that
## used to open this as a popover is retired (`docs/plan_standing_upkeep.md` §4.9 item 12), so there
## is no click to cover any more; what survives is the composition, which the crafting panel's ledger
## and the compose sheet's role hint both read. `compose_rungs.gd` asks it the same way.
func _kit_breakdown_text(band: Dictionary) -> String:
	return "\n".join(h._hud._disclosures.kit_breakdown_lines(band))

## ONE kit's breakdown line out of the popover, by the kit's NAME. Split per line rather than matched
## across the whole popover, because the three lines carry the same shape and a whole-popover
## `contains` could be satisfied by the WRONG kit's row — which is the exact substitution these
## assertions exist to catch.
func _kit_breakdown_line(popover: String, label: String) -> String:
	for line in popover.split("\n"):
		if String(line).contains(label):
			return String(line)
	return ""


# ---- THE BAND'S HAY LEDGER ----------------------------------------------------------------------
# **A BARE STOCK COULD NOT ANSWER THE ONE QUESTION A KEEPER HAS.** The Fodder row read `Fodder: 100.0`
# and nothing else, so a band whose pens were quietly outgrowing their fenced footprints looked
# identical to one with hay to spare — right up until animals started dying. These states are the row
# after it grew the Food line's other three beats: the need, the harvest that answers it, and the
# runway between them.
#
# The three bands are ONE BLOCK deliberately, and the assertions below are made over all four
# line-sets AT ONCE (the covered band, the short band, the empty-store band, and a forager band that
# must sprout no row at all). A covered band checked in a frame of its own — three chapters away from
# the band that is short — is exactly the gap this arc has already lost defects into: every claim here
# is a CONTRAST, and a contrast asserted one half at a time is not asserted.

## The pen-keeping band's hay ledger, three ways. Its own entity, so the Food disclosure key cannot
## collide with the reference band's.
const HAY_BAND_ENTITY := 907

## THE SHORT BAND — the slow trap, made a number. Its pens owe 6.0 hay a turn and its Fields grow 5.0,
## so the store is draining at 1.0/turn and the need clause wears the band zone's WARN amber. The
## stock is comfortable and that is the POINT: 100 hay and 100 turns of it looks like plenty, and the
## only thing on the row that says otherwise is the pair of rates.
const HAY_NEED_SHORT := 6.0

const HAY_INCOME_SHORT := 5.0

const HAY_STORE_SHORT := 100.0

## 100 / (6.0 - 5.0) — stated as the sim's own answer, not recomputed here. The client never divides
## for this: `turnsOfFodder` comes off `larder_runway_turns`, the very function that answers
## `turnsOfFood`.
const HAY_TURNS_SHORT := 100.0

## THE COVERED BAND — the same pens on better Fields: 4.0 owed, 6.0 grown. Nothing is draining, so the
## sim publishes the NO-DRAIN SENTINEL and the row reads the infinity glyph through the identical
## renderer the Food line's `(∞)` uses. **This is the frame that proves 999 never leaks through as a
## number.**
const HAY_NEED_COVERED := 4.0

const HAY_INCOME_COVERED := 6.0

## THE EMPTY-STORE BAND — **the case the old gate hid, and the reason the gate changed.** Pens owing
## 6.0 a turn, no Fields at all, and an EMPTY hay store: `fodder_store == 0`, so a store-only gate
## rendered no Fodder row on the one band in the game that most needed one. It is the loudest state
## the row has — amber need, no growing clause, and a runway of zero.
const HAY_STORE_EMPTY := 0.0

const HAY_TURNS_EMPTY := 0.0

## The row's key, matched bare — the needle for "did the Fodder row render at all", which is the whole
## of the gate claim.
const HAY_ROW_NEEDLE := "Fodder"

## The SUMMARY as it reads once the BBCode is stripped, written as an ANSWER rather than recomposed
## from `BAND_FODDER_ROW_FORMAT` — an assertion that rebuilds the format from its own parts passes
## whatever that format says, a doubled runway and a swapped pair included. Two terms and only two:
## the stock and the runway, exactly the Food row's shape.
const HAY_SHORT_SUMMARY_NEEDLE := "Fodder: 100.0  (100 turns)"

## …and the same row on the COVERED band, whose larder is not draining: the shared infinity glyph
## where the turn count sits, through the very renderer the Food line's `(∞)` goes through.
const HAY_COVERED_SUMMARY_NEEDLE := "Fodder: 100.0  (∞)"

## **THE RETIRED INLINE RATES.** They rode the row itself — `· need 6.0/turn · growing 5.0/turn` —
## and wrapped it to two lines in the narrow drawer column. Matched on the WORDS rather than the
## numbers, so a re-tuned fixture cannot make the absence claim pass for the wrong reason.
const HAY_RETIRED_NEED_WORD := "need "

const HAY_RETIRED_INCOME_WORD := "growing"

## The two breakdown rows the pull-down carries instead, as `fodder_breakdown_lines` produces them:
## `fodder_income` in and `fodder_need` out, at the fodder account's ONE decimal. Written as answers
## for the same reason the summary is — and asserted BOTH in the popover and, negatively, against
## every produced line, since "in the disclosure" and "not on the row" are two different claims and
## an inline append satisfies only the first.
const HAY_BREAKDOWN_GROWN_NEEDLE := "▲ +5.0  Grown"

const HAY_BREAKDOWN_PENS_NEEDLE := "▼ -6.0  Pens"

## The Fodder disclosure's `[url]` meta for the hay band — what a click emits, and the needle proving
## the row registered a caret at all rather than merely rendering.
const HAY_DISCLOSURE_FODDER := "fodder:%d" % HAY_BAND_ENTITY

## The raw sentinel, which must appear on NO band's row in any spelling. `999` is infinity and
## `DetailFormat.food_turns_text` is the one place in the client that knows it.
const HAY_SENTINEL_NEEDLE := "999"

## The `RichTextLabel` theme keys the WRAP guard measures in — the label's OWN font, size and the
## gutter its `[table=2]` spends between key and value cells, never a hardcoded face: a measurement
## taken in a font the label does not draw in is not a measurement of anything.
const DRAWER_FONT_THEME_KEY := "normal_font"

const DRAWER_FONT_SIZE_THEME_KEY := "normal_font_size"

const DRAWER_TABLE_SEPARATION_THEME_KEY := "table_h_separation"

## The rows that can FOLLOW Fodder in the drawer, whichever comes first — the cut that bounds its run
## in the parsed text. `[table]` rows carry no line break into `get_parsed_text()`, so the whole vitals
## block comes back as one string and a run bounded by the wrong row measures two rows as one.
const HAY_ROW_FOLLOWERS := [HudDisclosureVocab.DETAIL_ROW_UPKEEP,
	HudDisclosureVocab.DETAIL_ROW_MORALE]

func _hay_band_fixture(store: float, need: float, income: float, turns: float) -> Dictionary:
	var band := _pen_keeper_band_fixture()
	band["entity"] = HAY_BAND_ENTITY
	band["id"] = "Band 5"
	band["fodder_store"] = store
	band["fodder_need"] = need
	band["fodder_income"] = income
	band["turns_of_fodder"] = turns
	return band

func _hay_short_band_fixture() -> Dictionary:
	return _hay_band_fixture(HAY_STORE_SHORT, HAY_NEED_SHORT, HAY_INCOME_SHORT, HAY_TURNS_SHORT)

func _hay_covered_band_fixture() -> Dictionary:
	return _hay_band_fixture(HAY_STORE_SHORT, HAY_NEED_COVERED, HAY_INCOME_COVERED,
		BandFoodStatus.UNLIMITED_TURNS)

func _hay_empty_store_band_fixture() -> Dictionary:
	return _hay_band_fixture(HAY_STORE_EMPTY, HAY_NEED_SHORT, 0.0, HAY_TURNS_EMPTY)

func _hay_ledger_states() -> void:
	# State hay-a — THE SHORT BAND. `Fodder: 100.0  (100 turns)` — the Food row's shape exactly, a
	# stock and a runway, with the two flows behind the caret. A hundred turns of fodder is a
	# comfortable-looking stock and this band is draining: what says so is the runway falling under the
	# shared thresholds, the same thing that says it on the Food line.
	h._hud.show_unit_selection(_hay_short_band_fixture())
	await h._settle()
	await h._save("band_hay_short")
	# **THE WRAP, MEASURED** — this is the state the long row wrapped in, so it is the state that has
	# to prove the new one does not.
	_assert_fodder_row_fits()

	# State hay-b — THE COVERED BAND, the same pens with Fields that out-grow them. The runway reads
	# the infinity glyph — the sim's 999 sentinel, rendered by the very function that renders the Food
	# line's, which is the whole reason there is no second constant and no second branch anywhere in
	# the client for "turns of buffer left".
	h._hud.show_unit_selection(_hay_covered_band_fixture())
	await h._settle()
	await h._save("band_hay_covered")

	# State hay-c — THE EMPTY STORE, **the band the old gate could not show.** Pens owing 6.0 a turn,
	# no fodder Fields, nothing stockpiled. `fodder_store == 0` made a store-only gate false, so the
	# row that would have said *you owe 6.0 a turn and grow none of it* never rendered — on the one
	# band in the game that had to see it. The widened gate is `has fodder OR owes a bill`, and this is
	# the second half of it.
	h._hud.show_unit_selection(_hay_empty_store_band_fixture())
	await h._settle()
	await h._save("band_hay_empty_store")

	# **FOUR LINE-SETS, ONE BLOCK.** Each claim below is a CONTRAST — draining against covered,
	# rendered against absent, in the pull-down against on the row — and a contrast checked one half at
	# a time is not checked at all: the runway alone passes on a row that always reads a number, the
	# gate alone passes on a row that renders for every band in the game. So all four bands are
	# produced here and compared against each other, and the disclosure's rows are judged in this same
	# block rather than in the open-popover frame alone — an inline append renders a perfectly
	# plausible popover beside a row that also carries the rows.
	var short_lines := _band_lines(_hay_short_band_fixture())
	var covered_lines := _band_lines(_hay_covered_band_fixture())
	var empty_lines := _band_lines(_hay_empty_store_band_fixture())
	var forager_lines := _band_lines(BandFx.band_fixture())
	h._assert_hud("the Fodder row is the Food row's two terms — stock and runway (%s)"
		% HAY_SHORT_SUMMARY_NEEDLE,
		_lines_any_contain(short_lines, HAY_SHORT_SUMMARY_NEEDLE))
	h._assert_hud("…and the covered band's runway is the shared %s, in the same two-term shape"
		% DetailFormat.FOOD_UNLIMITED_GLYPH,
		_lines_any_contain(covered_lines, HAY_COVERED_SUMMARY_NEEDLE))
	# **THE ROW STATES NO RATES AT ALL NOW.** The inline `need` / `growing` pair is what wrapped this
	# row to two lines in the drawer; claimed on every one of the four line-sets, because a rate
	# restored on one state and not another is exactly the shape a single frame misses.
	h._assert_hud("…no band states a rate on the row itself — the inline need/growing pair is gone",
		not _lines_any_contain(short_lines, HAY_RETIRED_NEED_WORD)
		and not _lines_any_contain(short_lines, HAY_RETIRED_INCOME_WORD)
		and not _lines_any_contain(covered_lines, HAY_RETIRED_NEED_WORD)
		and not _lines_any_contain(empty_lines, HAY_RETIRED_NEED_WORD))
	# **THE FLOWS ARE IN THE PULL-DOWN, AND ONLY THERE.** Registered, never appended — inline growth in
	# a fixed-height zone is what clipped the Band panel once already, which is why the negative half
	# of this claim is made over the produced LINES.
	var short_breakdown: Array[String] = h._hud._disclosures.fodder_breakdown_lines(
		_hay_short_band_fixture())
	h._assert_hud("the two flows are the disclosure's rows: %s / %s"
		% [HAY_BREAKDOWN_GROWN_NEEDLE, HAY_BREAKDOWN_PENS_NEEDLE],
		_lines_any_contain(short_breakdown, HAY_BREAKDOWN_GROWN_NEEDLE)
		and _lines_any_contain(short_breakdown, HAY_BREAKDOWN_PENS_NEEDLE))
	h._assert_hud("…and NONE of them is appended to the row's own lines",
		not _lines_any_contain(short_lines, HAY_BREAKDOWN_GROWN_NEEDLE)
		and not _lines_any_contain(short_lines, HAY_BREAKDOWN_PENS_NEEDLE))
	# **THE INVITATION IS CLAIMED ON THE RENDERED SURFACE, NOT ON THE LINES.** A producer emits plain
	# `Key: value` strings; the clickable `[url]` run is `detail_bbcode`'s, drawn from the disclosure
	# state this row registered — so the only honest place to ask whether the caret exists is the live
	# drawer, which is showing the empty-store band this block last selected (all three hay fixtures
	# share one entity, so they share one key).
	h._assert_hud("…which the row invites through the same caret meta Food uses (%s)"
		% HAY_DISCLOSURE_FODDER,
		_find_meta_label(h._hud,
			HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + HAY_DISCLOSURE_FODDER) != null)
	# **THE GATE, BOTH WAYS AT ONCE.** A band with pens, a bill and an EMPTY store must render the
	# row — that is the case the store-only gate hid — while a forager band with no animals must still
	# render none. Restoring the old gate fails the first half; deleting the gate fails the second.
	h._assert_hud("a band owing fodder with an EMPTY store still renders its Fodder row",
		_lines_any_contain(empty_lines, HAY_ROW_NEEDLE))
	# **AND THE FORAGER BAND STILL RENDERS ONE — DORMANT.** This half was the gate's other face until
	# the row went unconditional: it asserted that a band with no animals sprouted NO Fodder row, and
	# an invisible account is one a player never learns exists. The row is there now, and the claims
	# about what it looks like are the dormant block further down.
	h._assert_hud("…and a forager band with no animals renders the row DORMANT rather than not at all",
		_lines_any_contain(forager_lines, HAY_ROW_NEEDLE))
	# **AND THE SENTINEL NEVER REACHES THE GLASS.** The covered band's runway is 999 on the wire and
	# the infinity glyph on the row; a client that printed the number would look entirely plausible in
	# a PNG.
	h._assert_hud("the no-drain sentinel renders as %s, never as the raw 999"
		% DetailFormat.FOOD_UNLIMITED_GLYPH,
		_lines_any_contain(covered_lines, DetailFormat.FOOD_UNLIMITED_GLYPH)
		and not _lines_any_contain(covered_lines, HAY_SENTINEL_NEEDLE)
		and not _lines_any_contain(short_lines, HAY_SENTINEL_NEEDLE))

	# **THE BAND AND ITS PEN IN ONE FRAME.** The two readouts are the same hay bill at two scales — the
	# band's `fodder_need` is the SIM's sum of the GAPS its pens' footprints leave, and a pen row
	# states what is left of its own gap after the hay it drew — and they render on different
	# surfaces, so a frame carrying only one of them cannot show them disagreeing. **They are not the
	# same number and must not be read as a total and its parts**: the gap is not on the wire per pen,
	# so nothing here sums the pen rows into the band's. Here the Band/City
	# dock states the band's ledger while the tile drawer states the starving pen's own share of it:
	# the pen's `Fed:` row reads `⚠ 47% — 40% pasture · 7% fodder · needs 11.3 more/turn`, the band's
	# `Fodder:` row the ledger that shortfall is drawn against. **One word across both** — the pen row
	# says `fodder`, as the band's store row always has.
	var hay_panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(hay_panel)
	await h.get_tree().process_frame
	hay_panel.reservation_changed.connect(func(edge: int, size: float) -> void:
		MAIN_SCRIPT.push_hud_strip(h._hud, BAND_PANEL_RESERVER, edge, size,
			MAIN_SCRIPT.band_dock_overlays_hud(edge, size, h._hud, hay_panel)))
	# Docked RIGHT: a vertical dock is the TALL tier, which keeps the Fodder row STANDALONE rather than
	# merging it onto the Food line — the merge is the SHORT tier's trade, and `band_panel_preview`
	# owns that half.
	hay_panel.set_dock(SIDE_RIGHT)
	hay_panel.set_active_tab(BandCityPanel.ZONE_BAND)
	h._hud.set_band_city_panel(hay_panel)
	# The band goes in through the REAL selection path, so the dock renders it exactly as a click
	# would; the herd is then lit in the tile drawer, which swaps the drawer's subject and leaves the
	# dock's band where it is. That is the whole trick of the frame — two subjects, two surfaces.
	h._hud.update_band_alerts([_hay_short_band_fixture()])
	h._hud.show_unit_selection(_hay_short_band_fixture())
	await h._settle()
	h._show_herd(HerdFx.starving_pen_herd_fixture())
	await h._settle()
	await h._save("band_hay_and_pen")
	# Release the dock and hand the reference band back, the restore idiom this chapter already uses
	# for its raid block: every state after this renders into the SAME long-lived `HudLayer`, and a
	# stranded reserved edge moves frames in later chapters for a reason unrelated to hay.
	h._hud.set_band_city_panel(null)
	hay_panel.queue_free()
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud.clear_selection()
	await h._settle()

	# State hay-d — **THE PULL-DOWN OPEN**, appended after the dock is released so nothing before it
	# moves. The two flows the row stopped carrying render in the shared POPOVER, in a card under the
	# clicked row: `▲ +5.0 Grown` / `▼ -6.0 Pens`, the Food breakdown's arrows and indent at the
	# fodder account's own one decimal. Driven through the REAL path — `meta_clicked` on the live
	# drawer label with the very meta its own text carries — so the frame covers the registration, the
	# caret and the click wiring rather than a hand-built popover.
	h._hud.show_unit_selection(_hay_short_band_fixture())
	await h._settle()
	_click_disclosure(HAY_DISCLOSURE_FODDER)
	await h._settle()
	await h._save("band_hay_breakdown")
	# **AND THE POPOVER IS ASKED WHAT IT HOLDS.** The block above proved the rows exist and are not on
	# the row; this proves the click actually put THEM on screen — a disclosure registered with an
	# empty payload renders a card with nothing in it and looks fine in a thumbnail.
	var fodder_popover := _popover_text()
	h._assert_hud("the opened Fodder pull-down holds both flows (%s)" % fodder_popover.replace("\n", " · "),
		fodder_popover.contains(HAY_BREAKDOWN_GROWN_NEEDLE)
		and fodder_popover.contains(HAY_BREAKDOWN_PENS_NEEDLE))
	# Close it again: the popover is a long-lived Window on the shared HUD, and a stranded one sits
	# over every frame after this for a reason unrelated to fodder.
	_click_disclosure(HAY_DISCLOSURE_FODDER)
	h._hud.clear_selection()
	await h._settle()

	await _fodder_dormant_states()

# ---- THE DORMANT FODDER ROW — the state the old gate rendered as nothing at all ------------------

## The two values this block stages the faction's Foddering at: part-learned (the craft is missing,
## and the row says how far along) and learned (the craft is there and the band still keeps no pen).
## The track's own KEY is read from the vocabulary the gate reason reads
## (`HudFloraVocab.KNOWLEDGE_TRACK_FODDERING`), so a renamed track cannot leave this pushing a key
## nothing consults and quietly staging the WRONG half of the two-sentence claim.
const FODDER_KNOWLEDGE_PART := 0.35

const FODDER_KNOWLEDGE_LEARNED := 1.0

## The dormant row as it lands in the produced lines, VALUE AND TINT TOGETHER. This is the one needle
## in the block deliberately recomposed rather than written as a flat answer: the claim IS the dim
## treatment, so the hex has to be the one `HudStyle` actually publishes — a literal would keep
## passing after the palette moved, which is the failure this frame family exists to catch. The
## em-dash comes off the producer's own const for the `STOCK_UNKNOWN_GLYPH` reason: one value, so the
## glyph searched for and the glyph drawn cannot drift.
const FODDER_DORMANT_ROW_NEEDLE := HudDisclosureVocab.DETAIL_ROW_FODDER + ": [color=#%s]%s[/color]"

## The live row's own value, as the CONTRAST — a band with a fodder economy must still read its two
## terms in this same frame. Without it every dormant claim below passes on a build that dimmed every
## Fodder row in the game.
const FODDER_LIVE_ROW_NEEDLE := HAY_SHORT_SUMMARY_NEEDLE

## What a dormant row must NEVER read: the live format on an empty larder. `0.0` in full ink beside a
## healthy `(∞)` is a band that has fodder and is fine — the exact opposite of what this state means,
## and the reading a bare gate deletion would have shipped.
const FODDER_DORMANT_ZERO_NEEDLE := HudDisclosureVocab.DETAIL_ROW_FODDER + ": 0.0"

## The band with no animals — the one the old gate rendered nothing for. It is `BandFx.band_fixture`
## unchanged, which carries no fodder key of any kind: the dormant state is what a band looks like
## when the sim has never had a fodder figure to send about it, not a fixture staged to be empty.
func _forager_band_fixture() -> Dictionary:
	return BandFx.band_fixture()

## The player faction's Foddering, pushed the way the snapshot pushes it. Every OTHER track goes in
## at zero with it — `_ingest_intensification` replaces the faction's whole row — which is also what
## the restore at the end of this block relies on.
func _push_foddering(progress: float) -> void:
	h._hud.update_intensification([{
		"faction": HudConst.PLAYER_FACTION_ID,
		"knowledges": {HudFloraVocab.KNOWLEDGE_TRACK_FODDERING: progress},
	}])

## The `Fodder:` row's registered hover, off a context the producer has just filled. The tooltip is
## keyed by the ROW and `DetailFormat.block_tooltip` is what joins it onto a label — so asking the
## CONTEXT here and asking the LABELS below are two different claims, and the second is the one that
## catches a host that never attached it.
func _fodder_row_tooltip(band: Dictionary) -> String:
	var ctx := DetailFormat.Context.new()
	h._hud._banddetail.unit_summary_lines(band, "", ctx)
	return String(ctx.row_tooltips.get(HudDisclosureVocab.DETAIL_ROW_FODDER, ""))

## The Band/City dock's vitals label — the first `RichTextLabel` under the panel, which its band zone
## is. Re-found after every render rather than held: that label is rebuilt per render, so a handle
## taken before one points at a freed node.
func _panel_vitals_label(node: Node) -> RichTextLabel:
	if node is RichTextLabel:
		return node as RichTextLabel
	for child in node.get_children():
		var found := _panel_vitals_label(child)
		if found != null:
			return found
	return null

## **STATE hay-e — A LIVE LARDER AND A DORMANT ONE, IN ONE FRAME.**
##
## The Fodder row was GATED on `has fodder OR owes a bill`, so a band with no pens rendered nothing
## here — the account was invisible on precisely the bands whose player has never met it. It is
## unconditional now and the gate picks the FORM: the two terms where there is a larder, a dim
## em-dash where there is not.
##
## **THE TWO STATES SHARE ONE RENDER, and that is the whole point of the frame.** The dim treatment
## is a claim about a DIFFERENCE — this row is quieter than that one — and a difference photographed
## one half at a time is not photographed. So the DOCK holds the hay band's live row while the
## DRAWER holds a forager band's dormant one: the `band_hay_and_pen` trick with a band on the far
## side of it instead of a herd. `show_unit_selection` renders both hosts, then `render_band` puts a
## different subject back into the dock alone.
##
## **AND THE TWO REASONS A ROW CAN BE DORMANT ARE DIFFERENT NEWS.** A faction without Foddering
## CANNOT bank hay at any price — the craft is a whole rung away — while one that simply keeps no pen
## is not blocked by anything. Both sentences are asserted, over the same fixture with the faction's
## knowledge moved between them, because a single tooltip claim passes on a build that states one
## sentence in both states.
func _fodder_dormant_states() -> void:
	_push_foddering(FODDER_KNOWLEDGE_PART)
	var panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(panel)
	await h.get_tree().process_frame
	panel.reservation_changed.connect(func(edge: int, size: float) -> void:
		MAIN_SCRIPT.push_hud_strip(h._hud, BAND_PANEL_RESERVER, edge, size,
			MAIN_SCRIPT.band_dock_overlays_hud(edge, size, h._hud, panel)))
	# Docked RIGHT — a vertical dock is the TALL tier, which keeps the Fodder row STANDALONE. The
	# SHORT tier trades that row for a stock clause on the Food line and states no dormant anything,
	# which is a different claim and `band_panel_preview`'s.
	panel.set_dock(SIDE_RIGHT)
	panel.set_active_tab(BandCityPanel.ZONE_BAND)
	# **THE SELECTION COMES FIRST, AND THE PANEL IS INJECTED AFTER IT — the order IS the trick.** A
	# selected player band is the DOCK's subject when a dock exists, and the Occupants drawer then
	# renders a one-line pointer at it (`SubjectDrawerController`), so a docked HUD has exactly ONE
	# band-detail surface and cannot show two bands at once. Selecting while no panel is injected
	# takes the drawer's own fallback path — the path every hay state above renders on — and nothing
	# in `set_band_city_panel` or `render_band` re-renders the drawer behind it. So the forager band
	# stays on the drawer, dormant, while the dock is handed the hay band's live row.
	h._hud.show_unit_selection(_forager_band_fixture())
	await h._settle()
	h._hud.set_band_city_panel(panel)
	h._hud._bandpanel.render_band(_hay_short_band_fixture())
	await h._settle()
	await h._save("band_fodder_dormant")
	# **THE FRAME'S OWN PRECONDITION, ASSERTED.** If the drawer had flipped to the band-panel pointer
	# the PNG would look perfectly tidy and would carry ONE fodder row instead of two — the contrast
	# silently gone, which is the failure this whole frame family exists to prevent.
	h._assert_hud("the frame really holds BOTH surfaces — the drawer still renders the forager band",
		h._hud.occupant_detail.text.contains(HudDisclosureVocab.DETAIL_ROW_FODDER))

	var forager_lines := _band_lines(_forager_band_fixture())
	# **THE CARET STATE IS READ HERE, BETWEEN THE TWO PRODUCTIONS.** `unit_summary_lines` clears the
	# controller's rows on entry, so this dictionary describes the LAST band produced and nothing
	# else — read it after the hay band and the dormant claim becomes a claim about the hay band.
	var dormant_registered: bool = h._hud._disclosures.state().has(
		HudDisclosureVocab.DETAIL_ROW_FODDER)
	var hay_lines := _band_lines(_hay_short_band_fixture())
	var live_registered: bool = h._hud._disclosures.state().has(
		HudDisclosureVocab.DETAIL_ROW_FODDER)
	var dormant_needle := FODDER_DORMANT_ROW_NEEDLE % [
		HudStyle.INK_DIM_HEX, DetailFormat.FODDER_DORMANT_VALUE]
	# **THE DIM DASH, AND THE LIVE PAIR BESIDE IT.** Dropping the dim treatment fails the first;
	# dimming every Fodder row in the game fails the second.
	h._assert_hud("a band with no fodder economy renders the row DIM, as %s" % dormant_needle,
		_lines_any_contain(forager_lines, dormant_needle))
	h._assert_hud("…while a band that HAS one still reads its two terms in full (%s)"
		% FODDER_LIVE_ROW_NEEDLE,
		_lines_any_contain(hay_lines, FODDER_LIVE_ROW_NEEDLE))
	# **AND IT IS A DASH RATHER THAN A ZERO.** `Fodder: 0.0  (∞)` is what the live format renders on
	# an empty larder, and in full ink beside a healthy infinity it reads as *this band has fodder and
	# is fine* — the reading a bare gate deletion ships, and the one no frame would catch.
	h._assert_hud("…and never the live format's %s, which would read as a measurement"
		% FODDER_DORMANT_ZERO_NEEDLE,
		not _lines_any_contain(forager_lines, FODDER_DORMANT_ZERO_NEEDLE))
	# **NO CARET ON THE DORMANT ROW.** There is nothing behind it — `fodder_breakdown_lines` produces
	# no rows for a band with neither flow — so it must offer no click at all. Claimed on the RENDERED
	# surface as well as on the lines, since the clickable run is `detail_bbcode`'s and not the
	# producer's: the drawer is showing the forager band this state selected.
	# **NO CARET, ASKED OF THE REGISTRATION — because the produced LINES cannot answer it.** A
	# producer emits plain `Key: value` strings and `detail_bbcode` is what draws the clickable run,
	# so a `[url=` search over these lines is vacuous: it passes on a row that registered a full
	# disclosure. (Measured — a dormant branch wrongly registering one left that search green.) What
	# decides the caret is the controller's own per-render state, and the LIVE band's registration
	# beside it is what stops "no caret" passing on a build that registers nothing at all.
	h._assert_hud("…and registers no disclosure, having nothing to put behind one",
		not dormant_registered)
	h._assert_hud("…while the band with a larder registers its two flows as usual",
		live_registered)
	h._assert_hud("…which the rendered drawer agrees with (no Fodder disclosure meta on it)",
		_find_meta_label(h._hud, HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX
			+ DetailFormat.breakdown_key(HudDisclosureVocab.BREAKDOWN_KIND_FODDER,
				_forager_band_fixture())) == null)

	# **WHY IT IS DIM, SENTENCE ONE: THE CRAFT IS MISSING.** The faction is at 35% Foddering, so the
	# hay in its meadows is unbankable, and the row says so in the words the forage panel already uses
	# — the SHARED clause out of `GATE_REASON_WILD_FODDER_FORMAT`, with the live percent in it.
	var locked_hover := _fodder_row_tooltip(_forager_band_fixture())
	var locked_expected := DetailFormat.FODDER_LOCKED_TOOLTIP_FORMAT % [
		HudFormat.progress_percent(FODDER_KNOWLEDGE_PART),
		FoodIcons.for_policy(SourceForecast.IMPROVEMENT_CORRAL)]
	h._assert_hud("the dormant row says WHY, in the forage panel's own words: %s" % locked_hover,
		locked_hover == locked_expected)
	# **AND IT ATTACHES.** A registered hover with no host to carry it never reaches a cursor, and
	# `[hint=…]` does not parse in this build — so the claim is made on the LABELS. The DOCK is the
	# half `%OccupantDetail` cannot answer for: the two hosts attach the block hover separately, and
	# the frame above deliberately has a LIVE band in the dock, whose label must therefore carry
	# nothing. Both directions, one host: empty on the live band, the sentence on the dormant one.
	h._assert_hud("…and the drawer label actually carries it",
		h._hud.occupant_detail.tooltip_text.contains(locked_expected))
	var live_vitals := _panel_vitals_label(panel)
	h._assert_hud("…while the dock, showing a LIVE band, offers no hover at all",
		live_vitals != null and live_vitals.tooltip_text == "")
	h._hud._bandpanel.render_band(_forager_band_fixture())
	await h._settle()
	var dormant_vitals := _panel_vitals_label(panel)
	h._assert_hud("…and the dock's own label carries it once the DORMANT band is its subject",
		dormant_vitals != null and dormant_vitals.tooltip_text.contains(locked_expected))

	# **SENTENCE TWO: NOTHING IS WRONG.** Learn Foddering and the same band's row is still dormant —
	# it keeps no pen and grows no fodder crop — but the reason is calm and descriptive, and it must
	# not be the lock's sentence. Asserted as an INEQUALITY against sentence one as well as an
	# equality, since two states sharing one sentence is the defect being guarded.
	_push_foddering(FODDER_KNOWLEDGE_LEARNED)
	h._hud.show_unit_selection(_forager_band_fixture())
	await h._settle()
	var calm_hover := _fodder_row_tooltip(_forager_band_fixture())
	h._assert_hud("a band that KNOWS Foddering and keeps no pen reads calm instead: %s" % calm_hover,
		calm_hover == DetailFormat.FODDER_DORMANT_TOOLTIP)
	h._assert_hud("…which is NOT the lock's sentence — the two reasons are different news",
		calm_hover != locked_hover)
	# …and the row itself is unchanged: knowing the craft does not hand the band a larder.
	h._assert_hud("…and the row is dim either way — the craft is not a fodder economy",
		_lines_any_contain(_band_lines(_forager_band_fixture()), dormant_needle))

	# Release the dock and put the faction's knowledge back to the untouched zeros every chapter after
	# this one inherits — a stranded reserved edge moves later frames, and a stranded knowledge row
	# would silently unlock gates in chapters that stage their own.
	h._hud.set_band_city_panel(null)
	panel.queue_free()
	_push_foddering(0.0)
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud.clear_selection()
	await h._settle()

## The band vitals rows as the producer emits them — BBCode intact, so a claim can be made about the
## TINT as well as the words. `terrain_label` is the morale row's payload and is irrelevant here.
func _band_lines(band: Dictionary) -> Array[String]:
	return h._hud._banddetail.unit_summary_lines(band, "")

## Does any line contain this needle? The chapter's own copy of the idiom `herd_graze_pen` uses — a
## one-line predicate is not worth sharing across chapters.
func _lines_any_contain(lines: Array[String], needle: String) -> bool:
	for line in lines:
		if String(line).contains(needle):
			return true
	return false
