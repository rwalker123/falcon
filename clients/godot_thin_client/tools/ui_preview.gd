extends Node

## Dev-only UI preview harness.
##
## Instances the real HudLayer with canned selection data, renders each state,
## and saves a PNG to `ui_preview_out/` in the project. Lets us iterate on HUD /
## selection-panel / targeting styling without a running server or manual
## screenshots. Not part of the game — run explicitly:
##
##   godot --path . res://tools/ui_preview.tscn
##
## then read ui_preview_out/*.png.

const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")
# Force-compile MapView here so the harness also acts as a full-context compile
# check for it (autoloads are registered when the harness runs as a scene, which
# --check-only cannot do).
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")
const OUT_DIR := "res://ui_preview_out"
# Slice 1 reserved-dock probe: left-edge reservation width used to verify the HUD insets.
const RESERVED_PROBE_WIDTH := 300.0
# Park the OS cursor over empty canvas before rendering. The HUD drops its hovered-hex record (and
# with it the targeting banner's hunt forecast) whenever the pointer sits over an interactive HUD
# control — see Hud._suppress_tooltip_over_ui. Wherever the cursor happened to be when the harness
# launched would otherwise decide whether the hover states render, making them non-deterministic.
const MOUSE_PARK_POSITION := Vector2(750, 640)
# The armed hunt party for the pre-launch forecast states (4 workers, matching the spec's worked
# example: a 4-worker party fills in ~6 turns on a mammoth but ~54 on red deer).
const HUNT_FORECAST_PARTY := 4
# The dialed-in hunter count for the LOCAL hunt preview states. 6 hunters × 0.8 provisions = 4.8, well
# above every policy ceiling here, so the HERD (not the hunters) is the binding constraint — which is
# exactly the case where the per-turn yield preview earns its keep.
const LOCAL_HUNT_HUNTERS := 6
# The sim's forward-SIMULATED turns-to-fill for the 4-worker party in these states (it exports the
# answer; the client never divides). Sustain is a small renewable flow → slow; Surplus/Market strip the
# herd's stock headroom first → fast. The deer's Sustain trip (54) blows past the 20-turn viability
# threshold; its Surplus trip (6) does not — same herd, same party, opposite verdicts.
const MAMMOTH_SUSTAIN_TRIP_TURNS := 6
const DEER_SUSTAIN_TRIP_TURNS := 54
const DEER_SURPLUS_TRIP_TURNS := 6
const MAMMOTH_SURPLUS_TRIP_TURNS := 3
# The whole animals the 4-worker RAID delivers (HuntTripEstimate.animalsTaken) — the payload the readout
# headlines. A viable/slow raid lands a positive count; a herd at/below its policy floor lands 0 (the
# no-surplus state). Surplus/Market raid deeper than Sustain, so a deeper policy lands MORE animals.
const MAMMOTH_SUSTAIN_ANIMALS := 8
const DEER_SUSTAIN_ANIMALS := 6
const DEER_SURPLUS_ANIMALS := 12
const NO_SURPLUS_ANIMALS := 0
# The server's measured Wild Boar raid (K=1433, body 50, B=1010, 4 food/hunter): 1 hunter → 5 animals /
# 7 turns, 2 → 8 / 8, 3 → 8 / 4. animalsTaken PLATEAUS at 8 (party 2), so max-useful = 2 hunters — the
# frame the "delivers ≈5 boar over ≈7 turns" readout and the stepper-cap-at-plateau are judged on.
const BOAR_RAID_ANIMALS := [5, 8, 8, 8, 8, 8, 8, 8]
const BOAR_RAID_TURNS := [7, 8, 4, 3, 3, 3, 3, 3]
const BOAR_FOOD_PER_ANIMAL := 4.0
# 0 = the raid ran the whole forecast horizon still delivering (a long raid), used by the no-surplus /
# collapsed fixtures where the raid also lands 0 animals.
const NEVER_FILLS_TRIP_TURNS := 0
# The pen-keeping band's entity id — its own, so the force-expanded Food breakdown override
# (`_breakdown_expanded` is keyed `food:<entity>`) doesn't collide with the reference band's.
const PEN_KEEPER_BAND_ENTITY := 906
# The Red Deer pen at its settled escapement point (design doc §7, MEASURED from a sim run): the
# feed the herd demands per turn, and the share of it a broke keeper managed to pay in the starving
# state. `pen_fed_fraction` < 1 ⇒ the herd is shrinking.
const PEN_UPKEEP_RED_DEER := 1.74
const PEN_FED_STARVING := 0.40
# The three fog-of-war states MapView tags onto tile_info (mirrors Hud.VISIBILITY_*).
const VIS_ACTIVE := "active"
const VIS_DISCOVERED := "discovered"
const VIS_UNEXPLORED := "unexplored"

# Hex-edge river fixtures. The wire mask is 12 bits, 2 bits per odd-r direction, in the SIM's
# direction order (clockwise from E: 0=E, 1=SE, 2=SW, 3=W, 4=NW, 5=NE) — built here with the
# same RiverEdges vocabulary the UI decodes with, so the fixture can't drift from the contract.
const RIVER_MASK_NONE := 0
# Minor on E + SE — one class, so one row.
const RIVER_MASK_SINGLE_CLASS := (
	(RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 0))
	| (RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 1))
)
# Major on NE + NW, Minor on SW — the two-class case: "Major River: NE, NW" then "Minor River: SW".
const RIVER_MASK_TWO_CLASS := (
	(RiverEdges.CLASS_MAJOR << (RiverEdges.BITS_PER_DIRECTION * 5))
	| (RiverEdges.CLASS_MAJOR << (RiverEdges.BITS_PER_DIRECTION * 4))
	| (RiverEdges.CLASS_MINOR << (RiverEdges.BITS_PER_DIRECTION * 2))
)

var _hud: HudLayer

func _ready() -> void:
	get_window().size = Vector2i(1500, 900)
	DirAccess.make_dir_absolute(OUT_DIR)

	# A mid-tone terrain-ish backdrop so the translucent card reads correctly.
	var bg_layer := CanvasLayer.new()
	bg_layer.layer = -10
	add_child(bg_layer)
	var bg := ColorRect.new()
	bg.color = Color(0.10, 0.15, 0.16)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg_layer.add_child(bg)

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)
	await get_tree().process_frame
	await get_tree().process_frame
	Input.warp_mouse(MOUSE_PARK_POSITION)

	# Top-bar Sedentarization meter (faction 0, soft band) — visible across all frames.
	_hud.update_sedentarization([{"faction": 0, "score": 62.0, "stage": "soft"}])

	# Top-bar demographics readout (faction 0 age structure + dependency ratio).
	_hud.update_demographics([{"faction": 0, "children": 34, "working": 51, "elders": 15}])

	# Top-bar intensification-knowledge meters (faction 0): Cultivation still learning
	# (block-glyph bar + "learning"), Herding fully mastered ("✔ known"). Visible across frames.
	_hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])

	# Top-bar Wondrous-Sites discoveries readout (faction 0): a landmark + a settle-site, so
	# the count reads `◈ Discoveries 2  ⛰ ⛲` and the distinct glyphs show.
	_hud.update_discoveries([{
		"faction": 0,
		"sites": [
			{"x": 12, "y": 8, "site_id": "great_peak", "category": "landmark", "display_name": "Great Peak", "glyph": "⛰"},
			{"x": 20, "y": 14, "site_id": "verdant_basin", "category": "settle_site", "display_name": "Verdant Basin", "glyph": "⛲"},
		],
	}])

	# The labor-allocation UI (Early-Game Labor slice 3b) targets the single player band;
	# seed it so the herd/tile "assign" controls resolve a band to staff.
	_hud._player_band = _band_fixture()
	# The world's herds (Main pushes snapshot["herds"]): the Current-actions Hunt row reads the herd's
	# species from here and, when clicked, jumps to its LIVE tile (it has migrated away from the hunt
	# assignment's launch target).
	_hud.update_herds(_world_herds_fixture())
	# The world's food modules (Main pushes snapshot["food_modules"]): each Forage row leads with the
	# module's map glyph, so the panel row and the map marker read as the same resource.
	_hud.update_food_modules([
		{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"},
	])

	# State 1 — a single band selected (GOOD state): the Occupants roster + the labor allocation panel.
	# Food + Morale are healthy, so BOTH summary rows read collapsed with a ▸ disclosure caret
	# (`Food ▸ …` / `Morale 82% ▸`) — click-to-expand, nothing auto-shown.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band")

	# State 1-foreign — a NON-player band selected. The drawer is the same `_unit_summary_lines` host,
	# but almost none of it applies: morale/output/breakdowns are player-only (someone else's band is
	# not ours to read), there is no allocation panel, and the identity rows (name, size) now live in
	# the roster row above. So the check this state exists for: does the drawer collapse to an empty
	# card once `Unit`/`Size` are gone? (It keeps the bare larder Food line + Position.)
	_hud.show_unit_selection(_foreign_band_fixture())
	await _settle()
	await _save("band_foreign")

	# State 1-forage-policy — the forage allocation row carries a policy tag like Hunt does. This band
	# forages on Market policy, which the sim gathers past the patch's regrowth, so actual_yield (0.62)
	# exceeds sustainable_yield (0.40): the row reads `Forage (71, 18) [market] +0.62 /turn ⚠` (amber
	# over-forage flag). The default `band` state above shows the [sustain] tag with no warning.
	var forage_policy_band := _band_fixture()
	forage_policy_band["labor_assignments"] = [
		{"kind": "forage", "workers": 6, "target_x": 71, "target_y": 18, "policy": "market", "actual_yield": 0.62, "sustainable_yield": 0.40},
		{"kind": "scout", "workers": 2},
	]
	_hud.show_unit_selection(forage_policy_band)
	await _settle()
	await _save("forage_policy")

	# State 1-food-a — GOOD food, breakdown force-EXPANDED. The good band's breakdown is hidden by
	# default (net positive, long runway); the static harness can't click the Food disclosure, so we
	# force the per-band expand override to confirm the click-expanded layout renders (indented
	# `Gathered · Hunted · Eaten` sub-line under Food) without clipping.
	_hud._breakdown_expanded = {"food:904": true}
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band_food_expanded")
	_hud._breakdown_expanded = {}

	# State 1-morale-a — GOOD morale, breakdown force-EXPANDED (same disclosure as Food): forcing the
	# per-band morale override opens the collapsed-by-default morale contribution sub-lines.
	_hud._breakdown_expanded = {"morale:904": true}
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band_morale_expanded")
	_hud._breakdown_expanded = {}

	# State 1-food-b — CONCERNING food (net negative + low runway): the Food line net reads red and
	# the category breakdown is AUTO-shown (no click needed), mirroring the morale breakdown.
	_hud.show_unit_selection(_concerning_food_band_fixture())
	await _settle()
	await _save("band_food_concerning")

	# State 1-food-c — a band KEEPING A PEN (docs/plan_corral_managed_population.md). Its ledger has
	# THREE terms, not two: the corral grosses 5.40, the people eat 1.15, and the penned animals eat
	# 1.74 off the same larder (`pen_feed_upkeep`, the sim's own figure — the client never sums the
	# herds' upkeep itself). Net = 5.88 − 1.15 − 1.74 = +2.99, NOT the +4.73 the old two-term ledger
	# would have advertised. Breakdown force-expanded to show all four rows at once.
	_hud._breakdown_expanded = {"food:%d" % PEN_KEEPER_BAND_ENTITY: true}
	_hud.show_unit_selection(_pen_keeper_band_fixture())
	await _settle()
	await _save("band_pen_feed")

	# State 1-food-d — the same pen, STARVING: the band could pay only 0.70 of the 1.74 the herd
	# demands, so the pen feed row shrinks to what was actually paid while the herd wastes away (the
	# herd drawer carries the alarm — see `herd_corral_starving`). Income has fallen with the herd,
	# and the net has gone red.
	_hud._breakdown_expanded = {"food:%d" % PEN_KEEPER_BAND_ENTITY: true}
	_hud.show_unit_selection(_starving_pen_band_fixture())
	await _settle()
	await _save("band_pen_starving")
	_hud._breakdown_expanded = {}

	# State 1b — an all-idle band: no assignments, every worker idle. The allocation panel
	# shows just the Scout + Warrior rows (both at 0) under the Working/Idle header.
	var idle_band := _band_fixture()
	idle_band["activity"] = "idle"
	idle_band["idle_workers"] = 16
	idle_band["labor_assignments"] = []
	_hud.show_unit_selection(idle_band)
	await _settle()
	await _save("band_idle")

	# State 1p — optimistic pending feedback: a fresh forage assignment (6 workers to a new
	# tile) is in flight before the snapshot confirms. The panel shows an amber "· pending"
	# Forage row and the Idle count reflects it immediately (16 − [5+4+2+2+6=19] clamps to 0).
	# (Seeds the HUD-local pending map directly to mimic a just-issued assign_labor.)
	_hud._pending_labor = {
		904: {
			"turn": 0,
			"assign": {"forage:64,20": {"kind": "forage", "workers": 6, "x": 64, "y": 20, "herd_id": "", "policy": ""}},
		}
	}
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band_pending")
	_hud._pending_labor = {}

	# State 1e — a scouting expedition selected in its awaiting-orders phase: the drawer shows the
	# dedicated expedition readout (Mission / Phase "Awaiting orders" / Party / Provisions) and the
	# Recall + Move panel with the amber awaiting callout, instead of the labor-allocation UI.
	_hud.show_unit_selection(_expedition_fixture())
	await _settle()
	await _save("expedition_panel")

	# State 1f — the same expedition after Recall, now in its returning phase: the panel's button
	# reads "Returning" (disabled) instead of a grayed-out "Recall", and the awaiting callout is
	# gone. The drawer Phase row reads "Returning".
	var returning_expedition := _expedition_fixture()
	returning_expedition["expedition_phase"] = "returning"
	_hud.show_unit_selection(returning_expedition)
	await _settle()
	await _save("expedition_returning")

	# State 1g — outfit party cap: a resident band with 16 idle workers but a server party cap of 8.
	# The "Send scouting expedition" Party stepper maxes at min(idle 16, cap 8) = 8 — dialed to 8, the
	# + is disabled, confirming the stepper clamps to the CAP, not to idle.
	var cap_band := _band_fixture()
	cap_band["idle_workers"] = 16
	cap_band["max_expedition_party_size"] = 8
	cap_band["labor_assignments"] = []   # all 16 working-age workers read idle
	_hud._send_expedition_count = 8
	_hud.show_unit_selection(cap_band)
	await _settle()
	await _save("expedition_outfit_cap")
	_hud._send_expedition_count = 1   # reset so later states render a fresh party stepper

	# State 1h — a hunting expedition (PR 2, §2b) selected in its Hunting phase: the panel shows the
	# hunt readout (Mission "Hunting expedition", Target herd, Policy, Carried 8 / 16, Party) +
	# Recall/Move.
	_hud.show_unit_selection(_hunt_expedition_fixture())
	await _settle()
	await _save("expedition_hunt_panel")

	# State 1i — a FULL hunt party (carried at the carry ceiling): the Carried row reads "16 / 16 …
	# · FULL" and the Phase is Delivering (it heads home when full).
	var full_hunt := _hunt_expedition_fixture()
	full_hunt["expedition_phase"] = "delivering"
	full_hunt["stores"] = {"provisions": 16.0}
	full_hunt["days_of_food"] = 8.0
	_hud.show_unit_selection(full_hunt)
	await _settle()
	await _save("expedition_hunt_full")

	# State 1j — a recalled hunt party in its Returning phase: the Phase reads "Returning" and the
	# panel's Recall button flips to a disabled "Returning" (same treatment as the scout panel).
	var returning_hunt := _hunt_expedition_fixture()
	returning_hunt["expedition_phase"] = "returning"
	returning_hunt["stores"] = {"provisions": 12.0}
	returning_hunt["days_of_food"] = 6.0
	_hud.show_unit_selection(returning_hunt)
	await _settle()
	await _save("expedition_hunt_returning")

	# State 1k — the hunt launch policy picker: an idle band (short allocation panel) showing the
	# "Send expedition" outfit block — the party stepper, the scout + hunt send buttons, and the hunt
	# POLICY radio (MARKET selected) with its EXPEDITION hint. The expedition hints must promise
	# neither husbandry nor trade goods: the Hunting arm credits FOOD ONLY, so Market's line says the
	# party "still hauls home food, not trade goods" — unlike a resident band's Market hunt, which does
	# sell the take. The outfit block sits below the left dock's fold, so scroll to see the hint.
	var launch_band := _band_fixture()
	launch_band["idle_workers"] = 12
	launch_band["labor_assignments"] = []
	var left_scroll: ScrollContainer = _hud.left_stack.get_parent() as ScrollContainer
	_hud._send_hunt_policy = "market"
	_hud.show_unit_selection(launch_band)
	await _settle()
	left_scroll.scroll_vertical = int(left_scroll.get_v_scroll_bar().max_value)
	await _settle()
	await _save("expedition_launch_policy")
	left_scroll.scroll_vertical = 0

	# State 1k-sustain — the SUSTAIN launch hint, which had to be rewritten when Sustain became the
	# maximum-sustainable-yield FLOW (it used to promise "one conservative harvest", a model that no
	# longer exists). It also must NOT mention domestication: only a RESIDENT band's Sustain hunt
	# builds husbandry — an expedition's take is food only.
	_hud._send_hunt_policy = "sustain"
	_hud.show_unit_selection(launch_band)
	await _settle()
	left_scroll.scroll_vertical = int(left_scroll.get_v_scroll_bar().max_value)
	await _settle()
	await _save("expedition_launch_policy_sustain")
	left_scroll.scroll_vertical = 0

	# State 1a — a well-fed but demoralized band: healthy food (∞) yet morale 0.22
	# (< critical), so the drawer's Morale line reads a red 22%. Discontent drags
	# Output to 56% (red) and the itemized morale breakdown + recovery guidance show.
	_hud.show_unit_selection(_low_morale_band_fixture())
	await _settle()
	await _save("band_low_morale")

	# State 1b — band alerts: seed previous sizes, then a snapshot that raises all
	# three alert kinds (starving red / losing-population amber / idle quiet).
	_hud.update_band_alerts(_band_alert_baseline())
	_hud.update_band_alerts(_band_alert_fixture())
	await _settle()
	await _save("band_alerts")

	# State 1c — Wondrous Sites: the top-bar `◈ Discoveries` readout plus a `SiteDiscovered`
	# command-feed entry (server-provided kind/label render generically). Confirms both surfaces.
	_hud.ingest_command_events([
		{"tick": 42, "kind": "site_discovered", "label": "Discovered Verdant Basin", "detail": "A settle-site revealed at (20, 14)."},
	])
	_hud.clear_selection()
	await _settle()
	await _save("discoveries")

	# band_alerts (above) left _player_band as an alert-fixture band (no work_range, far from the food
	# tile); seed a NEAR band so the forage controls resolve an in-range actor.
	_hud._player_band = _forage_range_bands()[0]
	_hud._player_bands = []
	_hud._forage_assign_key = ""
	_hud._forage_assign_band = -1

	# State 2 — a food tile selected, band WITHIN forage range: the Tile card's "Assign foragers"
	# controls (a "Band:" dropdown naming the actor band + a Foragers −/+ count + an enabled **Forage**
	# button). With one player band the dropdown is a single item ("Band 1").
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("food_tile")

	# State 2-forecast — the same food tile with the Foragers stepper parked AT the forecast cap
	# (3 = the Sustain ceiling's max-useful workers, below the band's 10 idle): the `+` button is
	# DISABLED, the "max 3 workers useful here — more would be idle" note explains why, and the
	# "Expected yield" row reads the ceiling itself (+0.96 /turn = min(3 × 0.32, 0.96)).
	_hud._forage_assign_count = 3
	_hud._build_forage_assign_controls(_food_tile_fixture())
	await _settle()
	await _save("forage_forecast_cap")

	# State 2-tended — a fully-cultivated forage patch: the Tile card's cultivation row reads
	# "🌾 Tended Patch" (SIGNAL tint) with an "Ecology: Thriving" row above it. A tended
	# patch's ceilings all equal its per-worker yield, so the forecast caps the stepper at 1 worker.
	_hud.show_tile_selection(_tended_tile_fixture())
	await _settle()
	await _save("tended_tile")

	# State 2-stressed — an over-drawn (uncultivated) forage patch: the Ecology row reads a WARN-amber
	# "⚠ Stressed" right under "Forage biomass", exactly like a stressed herd's Ecology row. Proves the
	# row is NOT gated on cultivation.
	_hud._forage_assign_count = 1
	_hud.show_tile_selection(_stressed_tile_fixture())
	await _settle()
	await _save("food_tile_stressed")

	# ---- Pasture: the ANIMAL-edible stock on the tile card (Grazing Phase 2a) --------------------
	# State 2-pasture-stressed — the graze drawn down into the stressed band: "Pasture 61 / 240" with a
	# WARN-amber "⚠ Stressed" under it, identical in label and tint to a stressed herd or patch. (The
	# healthy pair — "Forage biomass 84 / 120" beside "Pasture 240 / 240 · Thriving" — is on `food_tile`.)
	_hud._forage_assign_count = 1
	_hud.show_tile_selection(_overgrazed_tile_fixture())
	await _settle()
	await _save("tile_pasture_stressed")

	# State 2-pasture-none — a GLACIER: the biome carries no pasture at all, so the sim holds no patch
	# and the card prints NOTHING about pasture. "0 / 0" would be a lie of a different kind — a starved
	# pasture rather than an absent one — and this frame is the guard against it.
	_hud.show_tile_selection(_no_pasture_tile_fixture())
	await _settle()
	await _save("tile_pasture_none")

	# State 2-pasture-legend — the map legend for the `pasture` overlay channel (rows produced by
	# MapView._build_pasture_legend; see map_preview's "pasture" state for the map itself). The barren
	# tones sit OFF the straw→grass ramp: dead ground and water are their own rows, so "no pasture at
	# all" can never be read as "poor pasture".
	_hud.update_overlay_legend(_pasture_legend_fixture())
	await _settle()
	await _save("pasture_legend")
	_hud.clear_selection()

	# State 2-forage-legend — the map legend for the `forage` overlay channel (rows produced by
	# MapView._build_forage_legend; see map_preview's "forage" state for the map). The twin of the
	# pasture legend, but honest about the OPPOSITE meaning of absence: NO water row (shelves carry
	# forage and ride the ramp), a single "No forage" barren row (deep ocean/glacier/lava only), and a
	# "Gathering sites: N" sub-count so the ramp reads as POTENTIAL without calling the rest dead.
	_hud.update_overlay_legend(_forage_legend_fixture())
	await _settle()
	await _save("forage_legend")
	_hud.clear_selection()

	# ---- Hex-edge rivers on the Tile card (ui/RiverEdges.gd, the shared text formatter) -----------
	# State 2-river-both — the interesting case: a tile whose sides carry BOTH classes. The card must
	# read "Major River: NE, NW" then "Minor River: SW" — Major first (the bigger river reads first),
	# directions in compass order from NE clockwise, NOT the sim's bit order (which starts at E).
	_hud.show_tile_selection(_river_tile_fixture(RIVER_MASK_TWO_CLASS))
	await _settle()
	await _save("river_tile_both")

	# State 2-river-minor — a single-class tile: one "Minor River: E, SE" row, no Major row.
	_hud.show_tile_selection(_river_tile_fixture(RIVER_MASK_SINGLE_CLASS))
	await _settle()
	await _save("river_tile_minor")

	# State 2-river-none — mask 0: NO river row at all (not an empty "River:" label).
	_hud.show_tile_selection(_river_tile_fixture(RIVER_MASK_NONE))
	await _settle()
	await _save("river_tile_none")

	# ---- Cultivate: the forage INVESTMENT rung (gated, then unlocked) ----------------------------
	# State 2-cultivate-locked — the faction has NOT finished learning Cultivation (the top-bar meter
	# reads "Cultivation ▰▰▰… learning"): the 🌱 Cultivate option is still SHOWN in the picker, greyed,
	# with "🌱 Cultivate — Cultivation knowledge 55% — ♻ Sustain-forage a Thriving patch to learn it"
	# spelled out under the row. The player learns the rung exists, how far along the track is, AND the
	# action that finishes it, BEFORE they can use it.
	_hud._forage_assign_count = 1
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("forage_cultivate_locked")

	# Learning Cultivation crosses 0.55 → 1.0 between snapshots: the one-shot command-feed nudge fires
	# ("Cultivation learned — The Cultivate policy is now available on Thriving patches."), visible in
	# the left-dock Command Feed card in every frame from here on.
	_hud.update_intensification([{"faction": 0, "cultivation": 1.0, "herding": 1.0}])

	# State 2-cultivate — knowledge known + a Thriving patch: 🌱 Cultivate is ENABLED and selected. The
	# forecast states the DEAL instead of a single number — "Preparing: +0.24 /turn → then +1.20 /turn"
	# (ceiling_cultivate → tended_yield) — and the stepper caps at 1 worker (a managed source needs one).
	_hud.show_tile_selection(_food_tile_fixture())
	_hud._forage_assign_policy = "cultivate"
	_hud._build_forage_assign_controls(_food_tile_fixture())
	await _settle()
	await _save("forage_cultivate")

	# State 2-cultivate-stressed — knowledge known, but the patch is ⚠ Stressed: Cultivate stays visible
	# and greyed with the OTHER reason — "Patch is Stressed — ease workers off and let it regrow to
	# Thriving" (the ecology gate, not the knowledge one). The remedy is deliberately NOT "Sustain it":
	# a fully staffed Sustain takes the whole regrowth and holds a Stressed patch Stressed forever.
	_hud.show_tile_selection(_stressed_tile_fixture())
	await _settle()
	await _save("forage_cultivate_stressed")

	# ---- Sow + the Field: plant RUNG 3 (slice 6b) -------------------------------------------------
	# State 6b-sow-locked — Seed Selection is only 12% learned, so ▦ Sow greys. On this ordinary
	# prairie the ground ALSO refuses seed, so this is the MULTI-reason layout and — more to the point
	# — it shows the two reasons a player must tell apart: one is fixed by PRACTICE (work a Tended
	# Patch), the other only by MOVING somewhere else. No other rung on either ladder has the latter.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.12, "penning": 0.0,
	}])
	_hud._forage_assign_policy = "sustain"
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("forage_sow_locked")

	# Seed Selection completes → the one-shot feed nudge fires ("Seed Selection learned — The Sow
	# policy is now available — but only on rich, well-watered ground.").
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 0.0,
	}])

	# State 6b-sow-too-dry — knowledge KNOWN, and still refused: this prairie is rich but dry. THE
	# WHOLE POINT of the sim shipping a reason rather than a bool — only ~46 of 4160 tiles (1.1%) will
	# take seed, so "why can't I sow here?" is *the* question rung 3 provokes, and the client cannot
	# re-derive the answer (it has neither the biome capacity table nor the hydrology). The line must
	# name the fault (dry), not just refuse, and point at the rung that lifts it.
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("forage_sow_too_dry")

	# State 6b-sow-too-poor — the OTHER refusal, and the reason this pair is rendered together: thin
	# upland ground that IS watered. A different fault must produce a different sentence and a
	# different remedy — if these two frames read the same, the reason field is being wasted.
	_hud.show_tile_selection(_sow_too_poor_tile_fixture())
	await _settle()
	await _save("forage_sow_too_poor")

	# State 6b-sow — QUALIFYING ground at last (alluvial plain beside fresh water — one of the 46).
	# ▦ Sow is ENABLED and selected, with NO refusal line. The forecast states a deal that is
	# deliberately shaped unlike Cultivate's: "Preparing: +0.02 /turn → then +2.40 /turn" — near-zero
	# while the crop is in the ground (pure investment; there is no standing stand to take a fraction
	# of), then 2× a tended patch. That asymmetry IS rung 3's bargain.
	_hud.show_tile_selection(_sowable_tile_fixture())
	_hud._forage_assign_policy = "sow"
	_hud._build_forage_assign_controls(_sowable_tile_fixture())
	await _settle()
	await _save("forage_sow")

	# State 6b-sowing — the rung-3 BUILD meter: the Field row reads "Sowing 45%", following the pen's
	# "Building 40%" / the fence's "Fencing 60%" convention. It sits BESIDE the "Cultivation 🌾 Tended
	# Patch" row: the patch carries TWO independent meters, and both are the SOURCE's own.
	_hud.show_tile_selection(_sowing_tile_fixture())
	await _settle()
	await _save("forage_field_building")

	# State 6b-field — the COMPLETED Field, top of the plant ladder. The row must read "▦ Field" in
	# SIGNAL cyan — a visibly DIFFERENT THING from "🌾 Tended Patch" (different word, different glyph),
	# not a bigger percentage. That is the whole test of rung 3's readout.
	_hud.show_tile_selection(_field_tile_fixture())
	await _settle()
	await _save("forage_field")

	# State 6b-cultivate-done — a COMPLETED Tended Patch with a standing Cultivate selection: the build is
	# DONE, so Cultivate is a dead-end no-op. 🌱 Cultivate greys with "Already a Tended Patch — ♻
	# Sustain-forage it to harvest", the composed policy falls back to Sustain, and the "Preparing → then"
	# prep line is GONE (the forecast now reads the Sustain harvest, +/turn). This is the fix for the panel
	# lying: Cultivate used to stay enabled and keep paying the low prep dip on a finished patch.
	_hud.show_tile_selection(_tended_tile_fixture())
	_hud._forage_assign_policy = "cultivate"
	_hud._build_forage_assign_controls(_tended_tile_fixture())
	await _settle()
	await _save("forage_cultivate_done")

	# State 6b-sow-done — a COMPLETED Field with a standing Sow selection: ▦ Sow greys with "Already a
	# Field — ♻ Sustain-forage it to harvest", mirroring the finished-patch case one rung up (Cultivate is
	# greyed here too — the ground is both tended AND a Field).
	_hud.show_tile_selection(_field_tile_fixture())
	_hud._forage_assign_policy = "sow"
	_hud._build_forage_assign_controls(_field_tile_fixture())
	await _settle()
	await _save("forage_sow_done")

	# Back to a plain Sustain compose for the range states below.
	_hud._forage_assign_policy = "sustain"

	# States 2-fog-a/b/c — the three SIGHT states. The player must always be able to tell "there is
	# nothing here" apart from "I can't see what's here", so the Tile card leads with a `Sight:` row and
	# an unseen hex REPLACES its Occupants roster with a statement instead of rendering an empty one.
	#   2-fog-a  Active      — `Sight: In sight` (cyan), full live card (the food_tile above).
	#   2-fog-b  Discovered  — a remembered hex that DOES carry a herd: the herd must NOT be listed and
	#                          the Occupants card must read "out of sight · …bands and herds move".
	#                          (MapView fog-gates herds out of tile_info at source; the HUD re-reads the
	#                          same visibility_state flag, so it's honest even fed a leaky dict — which
	#                          is exactly what this fixture is.)
	#   2-fog-c  Unexplored  — never seen: `Sight: Unexplored` + "Nobody has been here."
	_hud.show_tile_selection(_sight_tile_fixture(VIS_ACTIVE))
	await _settle()
	await _save("tile_sight_active")

	_hud.clear_selection()
	_hud.show_tile_selection(_sight_tile_fixture(VIS_DISCOVERED))
	await _settle()
	await _save("tile_sight_remembered")

	_hud.clear_selection()
	_hud.show_tile_selection(_sight_tile_fixture(VIS_UNEXPLORED))
	await _settle()
	await _save("tile_sight_unexplored")
	_hud.clear_selection()

	# States 2-fog-d/e/f — the UNIT half of the fog rule:
	#     hidden == tile not visible AND unit is not ours.
	#   2-fog-d  YOUR OWN expedition on an UNEXPLORED hex → STILL listed and selectable. This is the
	#            regression guard for the load-bearing exception: the sim excludes expeditions from fog
	#            reveal (discovery is comm-range gated), so your own party ROUTINELY stands on an
	#            Unexplored tile — a plain visibility gate would delete it from the map/roster exactly
	#            while you're using it. The roster also warns that you still can't see anything ELSE there.
	#   2-fog-e  A FOREIGN band on a fogged (Remembered) hex → NOT listed; Occupants reads out-of-sight.
	#   2-fog-f  The same foreign band on a VISIBLE hex → listed normally (neutral dot, no allocation).
	_hud.show_tile_selection(_own_expedition_unexplored_tile())
	await _settle()
	await _save("tile_sight_own_expedition")

	_hud.clear_selection()
	_hud.show_tile_selection(_foreign_band_tile(VIS_DISCOVERED))
	await _settle()
	await _save("tile_sight_foreign_hidden")

	_hud.clear_selection()
	_hud.show_tile_selection(_foreign_band_tile(VIS_ACTIVE))
	await _settle()
	await _save("tile_sight_foreign_visible")
	_hud.clear_selection()

	# State 2b — the same food tile, single FAR band (~21 tiles away, beyond work_range 2): foraging is
	# stationary gathering with NO expedition fallback, so the Forage button is DISABLED and an
	# out-of-range hint shows ("(66,10) is 21 tiles away — beyond this band's forage range (2)").
	_hud._player_band = _forage_range_bands()[1]
	_hud._player_bands = []
	_hud._forage_assign_key = ""
	_hud._forage_assign_band = -1
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("food_forage_out_of_range")

	# State 2c — TWO bands at DIFFERENT distances from ONE food tile, NEAR band selected (821, 1 tile
	# away ≤ range 2): enabled **Forage**. The band-picker selection — not the tile — drives it.
	_hud._player_bands = _forage_range_bands()
	_hud._player_band = _hud._player_bands[0]
	_hud._forage_assign_key = ""
	_hud._forage_assign_band = -1
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("food_forage_band_near")

	# State 2d — same two bands, FAR band selected via the picker (822, ~21 tiles away): the SAME tile
	# now DISABLES Forage + shows the out-of-range hint, proving WHICH band is selected drives the
	# enabled-vs-disabled state (the case single-band playtest can't cover).
	_hud._forage_assign_band = int(_forage_range_bands()[1]["entity"])
	_hud._build_forage_assign_controls(_food_tile_fixture())
	await _settle()
	await _save("food_forage_band_far")
	# Reset so later states resolve their usual band.
	_hud._player_bands = []
	_hud._forage_assign_key = ""
	_hud._forage_assign_band = -1

	# band_alerts (above) overwrote _player_band with alert-fixture bands (which carry no hunt_reach);
	# re-seed the reference band so the herd assign controls resolve a proper band with a hunt reach.
	_hud._player_band = _band_fixture()
	_hud._player_bands = []
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1

	# State 3 — a huntable herd selected on a food tile, WITHIN the band's hunt reach: the "Assign
	# hunters" controls (a "Band:" dropdown naming the actor band, a Hunters −/+ count, the
	# sustain/surplus/market/eradicate policy picker, and the local "Assign Local Hunt" button). A
	# Thriving herd shows a neutral ecology readout in the drawer.
	_hud.show_herd_selection(_herd_fixture())
	await _settle()
	await _save("herd_verbs")

	# State 3b — an overhunted herd: the ecology readout warns "⚠ Collapsing" in red.
	_hud.show_herd_selection(_collapsing_herd_fixture())
	await _settle()
	await _save("herd_collapsing")

	# State 3b-graze — the ecological carrying-capacity readout (Grazing Phase 2b-iii). A HEALTHY herd:
	# the drawer shows the merged "Biomass: 1480 / 2150" current/max pair (how many animals vs the ceiling
	# the land sets) + a separate "Range: 7 tiles" row — with NO overgrazing warning (biomass ≤ K).
	_hud.show_herd_selection(_grazing_healthy_herd_fixture())
	await _settle()
	await _save("herd_grazing_healthy")

	# State 3b-overgraze — the same rows, but biomass (2100) > K (1352): the pair reads "Biomass: 2100 /
	# 1352" (current > max) and the WARN-amber "⚠ Overgrazing — range can't sustain this herd" row
	# appears beneath. It shows ONLY when biomass exceeds K — the honest sim-number comparison, not a
	# re-derived ecology model.
	_hud.show_herd_selection(_overgrazing_herd_fixture())
	await _settle()
	await _save("herd_overgrazing")

	# State 3b-smallgame — a radius-0 herd (small game grazes only its own tile): "Range: 1 tile"
	# (singular), and the map draws a single-hex highlight rather than a ring.
	_hud.show_herd_selection(_small_game_herd_fixture())
	await _settle()
	await _save("herd_grazing_small_game")

	# State 3c — a domesticated + corralled herd: the drawer shows "Husbandry 🐄 Domesticated"
	# AND "Corral 🐄 Corralled" (SIGNAL tint), the herd end of the intensification ladder — plus the
	# amber "Pen feed -1.74 /turn" row, the running cost a penned (non-grazing) herd costs its keeper.
	_hud.show_herd_selection(_domesticated_herd_fixture())
	await _settle()
	await _save("herd_domesticated")

	# State 3c-starving — the same pen, UNDERFED (`pen_fed_fraction` 0.40): the herd is shrinking
	# every turn and the drawer says so in red — "Corral ⚠ Starving — 40% fed" replaces the penned
	# badge, and the Pen feed row names the shortfall ("only 40% paid"). Biomass is visibly down.
	_hud.show_herd_selection(_starving_pen_herd_fixture())
	await _settle()
	await _save("herd_corral_starving")

	# Staffing readout — the fix for the silent "🐄 Domesticated but Penning stalled" playtest bug.
	# FULLY STAFFED: a near-tamed herd with every needed herder present (`herded_fraction` 1.0) reads a
	# calm "Herders: 4 / 4" (neutral ink) and no consequence line — it holds its tameness and earns
	# Penning normally.
	_hud.show_herd_selection(_fully_herded_herd_fixture())
	await _settle()
	await _save("herd_fully_herded")

	# UNDER-HERDED: the SAME herd with only half the needed herders (`herded_fraction` 0.5). Its
	# tameness is slipping, so the drawer says so loudly even though domestication 0.98 rounds to
	# "Domesticating 100%": an amber "Herders: 2 / 4 — under-herded" row plus the muted "Tameness
	# slipping — teaching Herding, not Penning. Staff all 4 herders to hold it." consequence line.
	_hud.show_herd_selection(_under_herded_herd_fixture())
	await _settle()
	await _save("herd_under_herded")

	# State 2d-γ self-feeding pen — a radius-2 pen (19 fenced tiles) on lush land: the fenced footprint
	# grazes the WHOLE feed, so the feed-split reads "Fed by pasture 100% · larder 0.0 food/turn" and the
	# amber Pen-feed debit row is gone. With no ring in flight, `_build_herd_assign_controls` shows the
	# "Extend pen" button (issues extend_pen at the pen anchor). Also carries the "Pen: radius 2 · 19
	# tiles" footprint row.
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_self_feeding_pen_herd_fixture())
	_hud._build_herd_assign_controls(_self_feeding_pen_herd_fixture())
	await _settle()
	await _save("herd_pen_self_feeding")

	# State 2d-γ extending pen — the SAME pen mid-extension (`pen_extend_progress` 0.6): the keeper is
	# fencing the next ring, so the "Extend pen" button is replaced by a WARN-amber "Fencing 60%" badge
	# (the pen twin of the corral-build "Building N%" meter). Partial pasture → "Fed by pasture 60% ·
	# larder 0.7 food/turn".
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_extending_pen_herd_fixture())
	_hud._build_herd_assign_controls(_extending_pen_herd_fixture())
	await _settle()
	await _save("herd_pen_extending")

	# State 2d-δ wild ceiling — a hunt-only species. NO husbandry track in the drawer (no
	# domestication / corral / pen rows), just the dim "Wild game — hunt only" hint, and the hunt policy
	# picker offers the extractive four with NO Corral rung.
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_wild_herd_fixture())
	_hud._build_herd_assign_controls(_wild_herd_fixture())
	await _settle()
	await _save("herd_ceiling_wild")

	# State 2d-δ pastoral ceiling — tameable + roams, never pennable. The drawer KEEPS the "Husbandry
	# Domesticating 60%" row but shows "Herdable, not pennable" where the Corral rows would sit; the hunt
	# policy picker again drops the Corral rung.
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_pastoral_herd_fixture())
	_hud._build_herd_assign_controls(_pastoral_herd_fixture())
	await _settle()
	await _save("herd_ceiling_pastoral")

	# ---- Corral: the hunt INVESTMENT rung (gated, then enabled) ----------------------------------
	# State 3c-corral-locked-both — BOTH halves of the Corral gate unmet: the MULTI-reason layout — a
	# "🐄 Corral needs:" header with one indented "· <reason>" bullet per unmet prerequisite.
	#
	# THE §4.3 GATE RESHUFFLE IS WHAT THIS FRAME NOW GUARDS. Corral is gated on **PENNING** (35%), NOT
	# on Herding — Herding gates Tame alone. The two reasons are also deliberately DIFFERENT KINDS:
	#   · a KNOWLEDGE reason — "Your people know Penning 35%", fixed by PRACTICE (♻ Sustain-hunt a
	#     tamed herd), and whose meter lives in the top-bar knowledge strip.
	#   · a SOURCE reason — "This herd is 40% tamed", fixed by the 🐾 Tame VERB, and whose meter lives
	#     in this herd's own drawer.
	# Herding is fully known here precisely so the frame proves Corral is NOT keyed to it.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 0.35,
	}])
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_corral_locked_herd_fixture())
	await _settle()
	await _save("herd_corral_locked_both")

	# State 3c-corral-locked — the SAME herd (domestication 0.4) once Penning is fully known: only the
	# SOURCE half of the gate remains, so 🐄 Corral greys with the single compact one-liner
	# "🐄 Corral — This herd is 40% tamed — 🐾 Tame it to finish".
	#
	# That remedy is the single most load-bearing copy fix in this slice. It used to read "♻ Sustain-hunt
	# this Thriving herd to finish taming it" — the exact hidden rule this whole arc exists to kill.
	# Sustain has not tamed anything since slice 3a; the Tame VERB does.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 1.0,
	}])
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_corral_locked_herd_fixture())
	await _settle()
	await _save("herd_corral_locked")

	# State 3d-corral — a fully-domesticated, not-yet-penned herd with the pen 40% built: 🐄 Corral is
	# ENABLED and selected, the forecast states the deal ("Preparing: +0.23 /turn → then +1.05 /turn
	# before feed", ceiling_corral → corral_yield, stepper capped at the 1 keeper a managed source
	# needs), and the drawer carries the "Corral: Building 40%" row — the herd twin of the tile's
	# "Cultivation N%".
	#
	# "before feed", not a number: `corral_yield` is the GROSS take, and the pen's feed is a separate
	# debit — but the sim exports `pen_upkeep` as 0 for a herd that is not penned YET (there is no pen
	# to feed), so the pre-build feed figure is NOT on the wire. Rather than fake a projection the row
	# says the payoff is gross, and the picker's Corral hint below it spells out that the animals eat
	# from the larder every turn. (A penned herd's row DOES subtract its real exported upkeep.)
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_corral_ready_herd_fixture())
	_hud._hunt_assign_policy = "corral"
	_hud._build_herd_assign_controls(_corral_ready_herd_fixture())
	await _settle()
	await _save("herd_corral")

	# State 3d-corral-depleted — the SAME rung on a herd BELOW the pen's escapement point (K/2). The
	# managed harvest takes only the biomass standing above that point, so the payoff is honestly
	# +0.00 /turn while the feed is still 0.14 — a pure loss. The row must SHOW both zeros and turn
	# amber with "⚠ Too depleted to pen", never suppress the zero as if it were missing data.
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_depleted_corral_herd_fixture())
	_hud._hunt_assign_policy = "corral"
	_hud._build_herd_assign_controls(_depleted_corral_herd_fixture())
	await _settle()
	await _save("herd_corral_depleted")

	# ---- THE INTENSIFICATION LADDER, slice 6b -----------------------------------------------------
	# THE TWO-METER SPLIT (docs/plan_intensification_ladder.md §4.1) — the headline of this slice, and
	# the frame it is judged on. Two meters advance from one action and they are DIFFERENT KINDS of
	# thing; this state puts both on screen at once so the distinction can actually be seen:
	#   • FACTION KNOWLEDGE — the top-bar strip, prefixed "⚒ Your people know:". Herding ✔ known,
	#     Penning still learning at 45%. This is your PEOPLE's craft: faction-wide, permanent, earned
	#     by practice. It appears NOWHERE else — never in the drawer below.
	#   • PER-SOURCE PROGRESS — this herd's own "Husbandry: Domesticating 40%" row, down in its
	#     drawer. Local to THIS animal, and it decays if abandoned.
	# The bridge between them is the gated 🐄 Corral's reason line, which names the knowledge, its live
	# percent, and the practice that fills it — the one line that teaches the whole ladder.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.12, "penning": 0.45,
	}])
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_taming_herd_fixture())
	_hud._hunt_assign_policy = "tame"
	_hud._build_herd_assign_controls(_taming_herd_fixture())
	await _settle()
	await _save("two_meter_split")

	# State 6b-tame — the 🐾 Tame affordance itself: a 6th option in the LOCAL hunt picker, beside
	# Sustain/Surplus/Market/Eradicate/Corral, ENABLED (Herding is known) and selected on a
	# pen-ceiling herd that is only 40% tamed. Its forecast row is deliberately NOT the dip→payoff
	# pair the other investment rungs render: there is no `pastoralYield` on the wire and nothing to
	# quote (taming's payoff is a faster regrowth a worker must still harvest, not a managed rate), so
	# the row shows the REAL exported dip and states the payoff in words. Fabricating a "then +Y" — or
	# deriving 1.5× client-side — would be the client re-deriving the ecology model.
	await _save("herd_tame")

	# State 6b-tame-stalled — the "why isn't my Tame progressing?" hint. Taming accrues ONLY while the
	# herd is Thriving, but is deliberately NOT gated on it (a herd's phase swings as you hunt it), so
	# the sim just PAUSES the meter. Silence here would recreate exactly the hidden-rule problem this
	# arc exists to kill, so the drawer says it: what stopped, why, that progress is NOT lost, and the
	# remedy (ease off — the opposite of "work harder").
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_taming_stalled_herd_fixture())
	_hud._hunt_assign_policy = "tame"
	_hud._build_herd_assign_controls(_taming_stalled_herd_fixture())
	await _settle()
	await _save("herd_tame_stalled")

	# Back to a plain Sustain compose for the band-picker / distance states below.
	_hud._hunt_assign_policy = "sustain"
	_hud._hunt_assign_key = ""

	# State 3f — TWO player bands: the "Assign hunters" controls' "Band:" dropdown lists both
	# (positional "Band 1" / "Band 2"). Default selection is the resolved band (Band 1, 12 idle);
	# the Hunters count is dialed up to 8 (< cap 12, so + stays enabled).
	_hud._player_bands = _two_player_bands()
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""   # force a fresh seed so the default selection = resolved band
	_hud.show_herd_selection(_herd_fixture())
	_hud._hunt_assign_count = 8
	_hud._build_herd_assign_controls(_herd_fixture())
	await _settle()
	await _save("herd_band_picker")

	# State 3g — same, after switching the dropdown to Band 2 (only 2 idle): the picker path
	# re-caps the Hunters count to the newly-selected band's assignable workers (8 → 2, + now
	# disabled), demonstrating selection → actor band → stepper re-cap.
	var second_band: Dictionary = _two_player_bands()[1]
	_hud._hunt_assign_band = int(second_band["entity"])
	_hud._hunt_assign_count = clampi(
		_hud._hunt_assign_count, 0, _hud._assignable_hunt_workers(second_band, _herd_fixture()["id"]))
	_hud._build_herd_assign_controls(_herd_fixture())
	await _settle()
	await _save("herd_band_picker_b")
	# Reset so later states render their usual single-band dropdown.
	_hud._player_bands = []
	_hud._hunt_assign_key = ""

	# State 3h — distance-aware herd-hunt, SINGLE far band: a lone band ~27 tiles from the herd (beyond
	# its hunt_reach 7). The affordance fully replaces the local option — the button reads "Send Hunting
	# Expedition", a distance hint shows, the stepper reads "Party", and Assign emits
	# send_hunt_expedition (party = the stepper), NOT assign_labor.
	_hud._player_bands = [_hunt_distance_bands()[1]]   # only the FAR band
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_expedition")

	# State 3i — TWO bands at DIFFERENT distances from ONE herd, NEAR band selected: band 811 sits ON
	# the herd (distance 0 ≤ reach 7) → "Assign Local Hunt" + assign_labor. The band-picker selection —
	# not the herd — drives it (the resolved/default band is the near one here).
	_hud._player_bands = _hunt_distance_bands()
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_band_near")

	# State 3j — same two bands, FAR band selected via the picker (entity 812, ~27 tiles away): the SAME
	# herd now offers "Send Hunting Expedition" (party cap = min(idle 6, max party 8) = 6), proving that
	# WHICH band is selected flips the label + command + band-entity target, not the herd.
	_hud._hunt_assign_band = int(_hunt_distance_bands()[1]["entity"])   # FAR band
	_hud._build_herd_assign_controls(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_band_far")
	# Reset so later states render their usual single-band dropdown + default band.
	_hud._player_bands = []
	_hud._player_band = _band_fixture()
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1

	# States 3k–3o — the HERD-PANEL hunt forecast, EXPEDITION branch. This is the second entry point
	# into a hunting expedition (herd-first): the herd is beyond the band's hunt_reach, so the panel
	# composes party + policy and sends immediately — no targeting step, so the banner's forecast never
	# appears. The forecast therefore renders LIVE above the button (the block re-renders on every
	# stepper tick / policy click) from the SAME helpers the banner uses: a PURE LOOKUP into the herd's
	# `hunt_trip_estimates` cell for (policy, party size). The client does no arithmetic here — the sim
	# forward-simulated each trip and exported the turns. Party 4:
	#   3k viable      — Sustain on a Thunder Mammoth: the sim's cell says 6 turns → cyan line, normal
	#                    primary "Send Hunting Expedition" button.
	#   3l not viable  — Sustain on Red Deer: 54 turns > warn 20 → amber line + the button itself goes
	#                    "armed" and names the cost: "Send Anyway (≈54 turns)".
	#   3m surplus     — the SAME Red Deer on Surplus: a Surplus party strips the herd's stock headroom
	#                    rather than living off its renewable flow, so the sim's cell says ~6 turns —
	#                    VIABLE. (The old bug re-derived the trip from the band's flow ceiling and scared
	#                    the player off a perfectly good trip; only the sim's own row knows.)
	#   3n never fills — a collapsing Wild Fowl flock: every cell is `turns_to_fill = 0` → red line +
	#                    armed "Send Anyway — party returns empty" (the HERD has nothing left to give).
	#   3o eradicate   — a healthy Red Deer on Eradicate: the sim marks the cell `delivers_food = false`
	#                    → amber DENIAL line + "Send (delivers no food)". Intent, not failure.
	# Never disabled, never a confirm dialog: the player can always send; this is a price tag, not a gate.
	_hud._player_bands = [_hunt_preview_far_band()]
	_hud._player_band = _hud._player_bands[0]
	for state: Dictionary in _hunt_assign_forecast_states():
		var far_herd: Dictionary = state["herd"]
		_hud._hunt_assign_key = ""    # force a fresh seed (band = resolved, policy = the herd's current)
		_hud._hunt_assign_band = -1
		_hud.show_herd_selection(far_herd)
		_hud._hunt_assign_count = HUNT_FORECAST_PARTY
		_hud._hunt_assign_policy = String(state["policy"])   # the policy-picker click, without the click
		_hud._build_herd_assign_controls(far_herd)
		await _settle()
		await _save(String(state["name"]))

	# States 3p–3s — the RAID readout in ANIMALS + the party stepper capped at max-useful. A hunting
	# expedition is a greedy raid: it grabs the herd's standing surplus in a burst and comes home, so the
	# headline is the PAYLOAD, and `animalsTaken` PLATEAUS with party size once the surplus (not the
	# pack) binds — that plateau IS max-useful. The Wild Boar carries the server's measured raid.
	#   3p boar raid   — a 1-hunter raid: "delivers ≈5 Wild Boar over ≈7 turns · ~20 food", cyan +
	#                    primary "Send Hunting Expedition".
	#   3q max useful  — 2 hunters: "delivers ≈8 Wild Boar over ≈8 turns · ~32 food"; a 3rd raids NO more
	#                    animals (the surplus binds), so the stepper caps at 2 and the `+` note reads
	#                    "max 2 workers useful here — more would be idle". The silent-idle-hunter gap, closed.
	#   3r no surplus  — a herd stripped to its floor: animalsTaken = 0 at EVERY size → the raid returns
	#                    empty → red "too lean to raid" + the DISABLED "Herd too lean to raid" button (party
	#                    size can't fix it — surplus is a property of the herd, not the party).
	#   3s eradicate   — the boar on Eradicate: delivers no food BY DESIGN → amber "Send (delivers no
	#                    food)", ENABLED (blocking a denial mission would ban it outright).
	var boar := _raid_boar_herd()
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(boar)
	_hud._build_herd_assign_controls(boar)   # source_changed seeds party = 1
	await _settle()
	await _save("herd_hunt_boar_raid")

	_hud._hunt_assign_count = 2               # key unchanged → no re-seed; caps at the plateau (2)
	_hud._build_herd_assign_controls(boar)
	await _settle()
	await _save("herd_hunt_max_useful")

	# State 3q-travel — the SAME boar raid, staffed by a band the herd is 8 tiles away from (beyond
	# hunt_reach 7 → expedition) and carrying a move rate. `turnsToFill` is HUNTING turns only, so the
	# client adds the round-trip TRAVEL the band-agnostic estimate table can't (ceil(2 × 8 / 2) = 8): at
	# party 2 the readout reads "delivers ≈8 Wild Boar over ≈16 turns (8 hunting + 8 travel) · ~32 food",
	# and the stepper still caps at the animalsTaken plateau (2). `band_move_tiles_per_turn` now ships on the
	# wire (schema slot 124) and is decoded onto the band; this fixture carries it exactly as the decoder does.
	_hud._player_bands = [_raid_travel_band()]
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(boar)
	_hud._hunt_assign_count = 2
	_hud._build_herd_assign_controls(boar)
	await _settle()
	await _save("herd_hunt_raid_travel")
	# Restore the far band (no move rate) for the remaining raid states.
	_hud._player_bands = [_hunt_preview_far_band()]
	_hud._player_band = _hud._player_bands[0]

	var lean := _no_surplus_herd()
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(lean)
	_hud._hunt_assign_count = HUNT_FORECAST_PARTY
	_hud._build_herd_assign_controls(lean)
	await _settle()
	await _save("herd_hunt_no_surplus")

	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(boar)
	_hud._hunt_assign_count = 2
	_hud._build_herd_assign_controls(boar)   # seeds sustain; key now = boar id
	_hud._hunt_assign_policy = "eradicate"
	_hud._build_herd_assign_controls(boar)   # key unchanged → the eradicate policy sticks
	await _settle()
	await _save("herd_hunt_eradicate")
	_hud._hunt_assign_policy = "sustain"

	# States 3n–3o — the same panel's LOCAL branch (herd within hunt_reach). A local hunt has NO carry
	# cap, so turns-to-fill is meaningless; the live number that decides a standing assignment is its
	# per-turn food yield:  min(workers × 0.8, ceiling(policy)) × output_multiplier (0.9 here — a
	# resident band applies its morale/discontent productivity modifier at payout, an expedition does
	# not). Red Deer: Sustain ceiling 0.30, Market ceiling 0.60.
	#   3n Sustain, 6 hunters — min(4.8, 0.30) × 0.9 = +0.27 /turn, == the sustainable yield → income-
	#                           green "· renewable", no flag.
	#   3o Market,  6 hunters — min(4.8, 0.60) × 0.9 = +0.54 /turn > sustainable 0.27 → WARN-amber with
	#                           the same ⚠ the allocation rows use: "overdraws the herd".
	# (The herd's `hunt_trip_estimates` ride along but are IGNORED here — a trip table answers an
	# EXPEDITION's question; a local hunt is arithmetic over the band's flow ceilings. Band = flow
	# arithmetic; expedition = lookup.)
	var local_herd := _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
		DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS)
	_hud._player_bands = [_hunt_preview_local_band()]
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(local_herd)
	_hud._hunt_assign_count = LOCAL_HUNT_HUNTERS
	_hud._build_herd_assign_controls(local_herd)
	await _settle()
	await _save("herd_hunt_local_sustain")

	# Flip the policy picker to Market — the same click path the player takes; the preview line
	# re-computes live off the new ceiling.
	_hud._hunt_assign_policy = "market"
	_hud._build_herd_assign_controls(local_herd)
	await _settle()
	await _save("herd_hunt_local_overdraw")

	# States 3p–3q — the WHOLE-ANIMAL carry cap. A big-game aurochs drops as one 80-biomass body via the
	# kill-credit bank; food_per_animal 1.6 outweighs one hunter's carry (per_worker 0.80), so the cap is
	# the CARRIERS needed to haul the peak-turn drop, not ceil(smoothed-rate / per_worker). Sustain
	# (ceiling 0.74) used to read "max 1 useful" (the bug: ceil(0.74/0.80)=1) — it must now read "max 2".
	_hud._player_bands = [_hunt_preview_local_band()]
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud._hunt_assign_policy = "sustain"
	var aurochs := _aurochs_big_game_fixture()
	_hud.show_herd_selection(aurochs)
	_hud._hunt_assign_count = 1
	_hud._build_herd_assign_controls(aurochs)
	await _settle()
	await _save("herd_hunt_whole_animal_cap")

	# Flip to Market — two bodies drop on the peak turn, so the cap climbs to 4: it tracks the selected
	# policy's ceiling, exactly as the smoothed-rate cap did.
	_hud._hunt_assign_policy = "market"
	_hud._build_herd_assign_controls(aurochs)
	await _settle()
	await _save("herd_hunt_whole_animal_cap_market")

	# Reset so later states render their usual single-band dropdown + default band/policy.
	_hud._player_bands = []
	_hud._player_band = _band_fixture()
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud._hunt_assign_policy = "sustain"

	# State 3d — a populated hex: the Tile card + the Occupants roster split. Three
	# player bands (days_of_food 15 / 7 / 2 → green / amber / red vitality dots, with
	# harvest / scout / idle activity glyphs) under Bands (3), and one stressed herd
	# (amber ecology dot) under Wildlife (1). Auto-selects the first band, so the
	# drawer shows its Rations and the Scout verb.
	_hud.show_tile_selection(_occupied_tile_fixture())
	await _settle()
	await _save("occupants_band")

	# State 3e — the same hex with the wildlife row selected: the drawer swaps to the
	# herd's Species / Biomass and the Hunt / Follow + policy verbs.
	_hud.show_herd_selection(_occupied_herd_fixture())
	await _settle()
	await _save("occupants_herd")

	# State 4 — targeting active: pressing "Move" on the band allocation panel enters
	# tile-targeting, raising the top-centre banner ("MOVE … click a destination tile").
	_hud.show_unit_selection(_band_fixture())
	_hud._on_move_band_pressed()
	await _settle()
	await _save("targeting_banner")
	_hud.cancel_active_targeting()

	# States 4a–4c — the PRE-LAUNCH HUNT FORECAST. A hunt expedition is armed (4 workers, Sustain);
	# the player is now hovering a herd, and the banner's second line says what the trip would cost
	# BEFORE the click commits. The turns are LOOKED UP in the hovered herd's `hunt_trip_estimates`
	# (the sim forward-simulated the trip; the client divides nothing). Under Sustain the party lives
	# off a small renewable flow, so the answer is entirely herd-dependent. Same party, three herds:
	#   4a viable      — Thunder Mammoth: the sim's cell says 6 turns → within the 20-turn warn line
	#   4b not viable  — Red Deer:        the sim's cell says 54 turns → past the warn line
	#   4c never fills — a collapsing Wild Fowl flock: `turns_to_fill = 0` → the party would return empty
	for state: Dictionary in _hunt_forecast_states():
		_hud.show_unit_selection(_band_fixture())
		_hud._on_send_hunt_expedition_pressed(_band_fixture(), HUNT_FORECAST_PARTY, "sustain")
		_hud.show_tooltip(state["tile"])
		await _settle()
		await _save(String(state["name"]))
		_hud.cancel_active_targeting()
		_hud.show_tooltip({})

	# State 5 — quick-hunt convenience (map double-click a herd): with idle workers it
	# assigns them to hunt; with none it posts a command-feed note instead of silently
	# no-opping. Seed a fully-staffed band (0 idle) so the note renders in the Command Feed.
	var staffed_band := _band_fixture()
	staffed_band["idle_workers"] = 0
	_hud._player_band = staffed_band
	_hud.show_tile_selection(_food_tile_fixture())
	_hud.quick_assign_hunters("game_bison_02")
	await _settle()
	await _save("quick_hunt_note")

	# State 6 — turn orb, ALL-CLEAR: a player band with zero idle workers → empty
	# attention registry → the orb calm-pulses (dashed cyan arc), the caption reads
	# "Turn 42 · ▸ all clear", and no badge shows.
	_hud.clear_selection()
	_hud.update_overlay(42, {})
	_hud.update_band_alerts([
		{"faction": 0, "entity": 501, "size": 40, "days_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 0},
	])
	await _settle()
	await _save("turn_orb_clear")

	# State 6b — turn orb, EMPTY registry, orb-face CLICK: advancing must always be possible
	# from the orb, so with nothing to triage the click ADVANCES the turn directly and opens NO
	# popover (the old bug opened a tall blank box whose Advance affordance was pushed off-screen,
	# trapping the player). Assert the emitted advance signal (the harness can't run a real turn)
	# and that no popover opened; the saved frame must show the calm pulse with no blank box.
	var advance_hits := [0]
	var advance_cb := func() -> void: advance_hits[0] += 1
	_hud.turn_orb.advance_requested.connect(advance_cb)
	_hud.turn_orb._on_face_pressed()
	await _settle()
	_assert_turn_orb("empty click advances", advance_hits[0] == 1 and not _hud.turn_orb._popover_open)
	await _save("turn_orb_clear_click_advances")

	# State 6c — turn orb, NON-EMPTY registry: the click opens the reasons popover, and the
	# popover's `Advance ▸` footer button emits advance_requested (unchanged behavior). Seed one
	# attention entry, open via the face click, then fire the footer button and assert the emit.
	advance_hits[0] = 0
	_hud.update_band_alerts([
		{"faction": 0, "entity": 511, "size": 40, "days_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 5},
	])
	_hud.turn_orb._on_face_pressed()
	await _settle()
	var opened := _hud.turn_orb._popover_open
	var footer_btn := _turn_orb_advance_button()
	var had_footer := footer_btn != null
	if had_footer:
		footer_btn.pressed.emit()   # frees the popover (advance closes it)
	await _settle()
	_assert_turn_orb("non-empty popover + footer advances",
		opened and had_footer and advance_hits[0] == 1 and not _hud.turn_orb._popover_open)
	_hud.turn_orb.advance_requested.disconnect(advance_cb)

	# State 7 — turn orb, ALL THREE ATTENTION KINDS (the folded-in Alerts panel): a first
	# snapshot seeds prior band sizes so "losing population" has a baseline, then the live
	# snapshot fires one of each producer — Band 1 starving (days 3 < critical → critical/red),
	# Band 2 shrank 90→78 with emigrants (losing population → warn/amber), Band 3 has idle
	# workers (warn/amber). The badge reads "3", the pulse stops, and the popover (opened here)
	# lists all three with the starving/critical row sorted to the TOP, each with a Jump row.
	# A starving EXPEDITION is interleaved between the bands to verify the bands-only numbering:
	# it produces NO attention entry (never "Band N starving") and does not shift Band 2/Band 3's
	# positional numbers — the idle-workers row still reads "Band 3", matching the picker/header.
	_hud.update_band_alerts([
		{"faction": 0, "entity": 601, "size": 120, "days_of_food": 12.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		{"faction": 0, "entity": 602, "size": 90, "days_of_food": 999.0, "activity": "hunt",
			"current_x": 31, "current_y": 21},
		{"faction": 0, "entity": 603, "size": 60, "days_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9},
	])
	_hud.update_band_alerts([
		# Band 1 — starving (3 days of food, below critical).
		{"faction": 0, "entity": 601, "size": 120, "days_of_food": 3.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		# A detached hunt expedition, also starving — must NOT emit a "Band N starving" entry and
		# must NOT consume a band number (Band 2/Band 3 below stay 2 and 3).
		{"faction": 0, "entity": 650, "size": 6, "days_of_food": 2.0, "is_expedition": true,
			"expedition_mission": "hunt", "expedition_phase": "hunting", "home_band_entity": 601,
			"current_x": 25, "current_y": 18},
		# Band 2 — losing population: 90 → 78, well-fed but 12 emigrated last turn → "people leaving".
		{"faction": 0, "entity": 602, "size": 78, "days_of_food": 999.0, "morale": 0.30,
			"morale_cause": 1, "last_emigrated": 12, "activity": "hunt", "current_x": 31, "current_y": 21},
		# Band 3 — idle labor: 4 working-age workers unassigned.
		{"faction": 0, "entity": 603, "size": 60, "days_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9, "idle_workers": 4},
	])
	_hud.turn_orb.open_popover()
	await _settle()
	await _save("turn_orb_attention")

	# State 7b — turn orb, AWAITING-ORDERS producer: an expedition parked at its objective is a
	# demand on the player (it burns provisions doing nothing), structurally the same class as idle
	# workers — so it produces its OWN attention row per party. Here: one band with idle workers
	# (the two producers must coexist) + FOUR awaiting parties (a scout and a hunt party name their
	# objective; the 4th trips the ATTENTION_AWAITING_MAX_ROWS cap → an aggregate "+1 more awaiting
	# orders" row). A non-awaiting (outbound) expedition proves only `awaiting` produces a row. The
	# popover must still fit above the orb with its `Advance ▸` footer on-screen.
	_hud.turn_orb.set_attention([])   # drop State 7's registry so this frame is only these rows
	_hud.update_band_alerts([
		{"faction": 0, "entity": 701, "size": 60, "days_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9, "idle_workers": 4},
		{"faction": 0, "entity": 751, "size": 6, "days_of_food": 9.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 39, "current_y": 26},
		# The hunt party names its OBJECTIVE by species (game_deer_07 → "Red Deer" via the world-herd
		# list pushed above), not the raw fauna id — the row has to be actionable at a glance.
		{"faction": 0, "entity": 752, "size": 5, "days_of_food": 7.0, "is_expedition": true,
			"expedition_mission": "hunt", "expedition_phase": "awaiting", "home_band_entity": 701,
			"expedition_target_herd": "game_deer_07", "current_x": 64, "current_y": 11},
		{"faction": 0, "entity": 753, "size": 4, "days_of_food": 6.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 18, "current_y": 44},
		{"faction": 0, "entity": 754, "size": 4, "days_of_food": 5.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "awaiting", "home_band_entity": 701,
			"current_x": 51, "current_y": 8},
		{"faction": 0, "entity": 755, "size": 6, "days_of_food": 9.0, "is_expedition": true,
			"expedition_mission": "scout", "expedition_phase": "outbound", "home_band_entity": 701,
			"current_x": 33, "current_y": 30},
	])
	_hud.turn_orb.open_popover()
	await _settle()
	await _save("turn_orb_awaiting_orders")

	# State 7c — turn orb, STARVING-PEN producer: the band that keeps the pen could not pay its feed,
	# so the penned herd is shrinking every turn and 25 turns of investment are draining away. Two
	# rows here ON PURPOSE, and they are NOT the same alert twice: the empty larder is one cause with
	# two different losses — the PEOPLE are starving (critical, jumps to the band) and the HERD is
	# starving (warn, jumps to the herd, where the fed fraction + feed cost are). Only one shouts.
	_hud.turn_orb.set_attention([])
	_hud.update_herds([_starving_pen_herd_fixture()])
	_hud.update_band_alerts([
		{"faction": 0, "entity": 801, "size": 46, "days_of_food": 1.0, "activity": "hunt",
			"current_x": 64, "current_y": 11, "idle_workers": 0,
			"labor_assignments": [
				{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "policy": "corral",
					"target_x": 66, "target_y": 10, "actual_yield": 0.84, "sustainable_yield": 0.84},
			]},
	])
	_hud.turn_orb.open_popover()
	await _settle()
	await _save("turn_orb_starving_pen")
	_hud.update_herds(_world_herds_fixture())   # restore the shared world-herd list

	_hud.turn_orb.toggle_popover()   # close, so later states render without it

	# State 8 — reserved-space docking (Slice 1 refactor): a left-edge reservation of
	# RESERVED_PROBE_WIDTH px insets the whole HUD (LayoutRoot.offset_left), so the top/bottom
	# bars start that much further right — mirroring how the docked Inspector shrinks the play
	# space. Save the inset frame, then release it (size 0) and save the restored frame.
	_hud.clear_selection()
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, RESERVED_PROBE_WIDTH)
	await _settle()
	await _save("reserved_dock")
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, 0.0)
	await _settle()
	await _save("reserved_dock_cleared")

	# Terrain-legend sort control (base terrain legend, key == "terrain"). Several
	# biomes of varying tile counts so the default count-desc order + the Name/Count
	# sort toggles + sort persistence across a regen push are all visible. Rendered
	# before the full-screen icon probe below so the right-dock legend isn't covered.
	_hud.update_overlay_legend(_terrain_legend_fixture())
	await _settle()
	await _save("terrain_legend_count_desc")  # default: Count, high→low

	# Click "Name" → alphabetical A→Z.
	_hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_NAME)
	await _settle()
	await _save("terrain_legend_name_asc")

	# Click "Name" again → Z→A.
	_hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_NAME)
	await _settle()
	await _save("terrain_legend_name_desc")

	# Click "Count" → back to count, and again → low→high.
	_hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_COUNT)
	_hud._on_legend_sort_pressed(HudLayer.LEGEND_SORT_FIELD_COUNT)
	await _settle()
	await _save("terrain_legend_count_asc")

	# Simulate a map regen (fresh terrain-legend push): the chosen sort (count asc)
	# must persist, not snap back to the default.
	_hud.update_overlay_legend(_terrain_legend_fixture())
	await _settle()
	await _save("terrain_legend_persist")

	# ---- Hunt/husbandry render-honesty pass (intensification ladder client UX) ----------------------
	# Fix #1 + #5 — CURRENT ACTIONS rows: the honest per-turn RATE (sustainable, not the 0.00 pulse)
	# paired with the kill-RHYTHM from body_mass (a fast animal "≈1.3 Marsh Fowl/turn"; a big one "≈1
	# Woolly Mammoth / 7 turns"), and the muted "· 1.9 wasted" under-crewed note on the big-game row.
	_hud.update_herds(_hunt_rhythm_herds_fixture())
	_hud.show_unit_selection(_hunt_actions_band_fixture())
	await _settle()
	await _save("hunt_actions_rhythm")
	_hud.update_herds(_world_herds_fixture())

	# Fix #2 + #1(forecast) + #6 — the LOCAL hunt compose view: the policy picker shows each rung's
	# per-turn take so Sustain < Surplus < Market < Eradicate reads as ASCENDING, and the live preview
	# pairs its rate with the kill-rhythm. (The stepper on a WILD herd reads "Hunters".)
	# A compact NON-food tile so the herd drawer (not a full forage tile card) lands in-frame.
	var picker_herd := _herd_fixture()
	picker_herd["tile_info"] = _compact_herd_tile_fixture()
	_hud._player_band = _band_fixture()
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_policy = "sustain"
	_hud._hunt_assign_count = 3
	_hud.show_herd_selection(picker_herd)
	_hud._build_herd_assign_controls(picker_herd)
	await _settle()
	await _save("hunt_picker_ascending")

	# Fix #6 — a MANAGED (corralled) herd's local crew are HERDERS, not a hunt party: the stepper reads
	# "Herders" so a pen whose workersNeeded scales with the herd doesn't look like a hunt-party bug.
	_hud._hunt_assign_key = ""
	_hud.show_herd_selection(_domesticated_herd_fixture())
	_hud._build_herd_assign_controls(_domesticated_herd_fixture())
	await _settle()
	await _save("hunt_crew_herders")

	# Fix #4 — LEARNING knowledge visibility: Penning at 34% (0 < value < 1) must climb WITH its % in
	# the top-bar strip, not be absent-until-100. Seed Selection mid-climb too; Cultivation/Herding ✔.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "seed_selection": 0.6, "herding": 1.0, "penning": 0.34}])
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("knowledge_penning_climbing")
	# Restore the default strip for any later frame.
	_hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])

	# Icon probe last, on a top layer with its own backdrop (rendering is warm by
	# now), so every food glyph is captured via the map's draw path.
	var probe_layer := CanvasLayer.new()
	probe_layer.layer = 100
	add_child(probe_layer)
	var probe_bg := ColorRect.new()
	probe_bg.color = Color(0.06, 0.09, 0.10)
	probe_bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	probe_layer.add_child(probe_bg)
	var probe := preload("res://tools/icon_probe.gd").new()
	probe_layer.add_child(probe)
	await _settle()
	await _save("food_icons")

	get_tree().quit()

func _settle() -> void:
	await get_tree().process_frame
	# Force a synchronous frame rather than awaiting `RenderingServer.frame_post_draw`.
	# Under the dummy rendering backend (which `--headless` selects on Godot 4.5) no
	# real draw ever posts, so that await never returns and the harness hangs. force_draw
	# just no-ops there, so a stray headless run fails fast in `_save` instead of hanging.
	RenderingServer.force_draw()
	await get_tree().process_frame

func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		# No image to read back — the dummy renderer (i.e. someone ran this with
		# `--headless`, which selects it on Godot 4.5). Capture is impossible, but
		# the compile/scene gate still passed. Run WITHOUT `--headless` for PNGs.
		push_warning("ui_preview: null image (dummy renderer?) — skipping %s.png; run without --headless to capture" % name)
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("ui_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("ui_preview: saved ", name, ".png")

## Walk the open reasons popover to its `Advance ▸` footer button (last body row's child).
func _turn_orb_advance_button() -> Button:
	var pop := _hud.turn_orb._popover
	if pop == null or pop.get_child_count() == 0:
		return null
	var body := pop.get_child(0)
	if body.get_child_count() == 0:
		return null
	var footer := body.get_child(body.get_child_count() - 1)
	if footer.get_child_count() == 0:
		return null
	var btn := footer.get_child(0)
	return btn as Button

func _assert_turn_orb(label: String, ok: bool) -> void:
	if ok:
		print("ui_preview: PASS turn-orb — ", label)
	else:
		push_error("ui_preview: FAIL turn-orb — %s" % label)

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

func _band_fixture() -> Dictionary:
	return {
		"id": "Band 2",
		"size": 148,
		"entity": 904,
		"faction": 0,
		"pos": [71, 18],
		# Good food state: a long larder runway (≥ warn) + positive net (0.94 − 0.68 = +0.26) → the
		# Food line reads "… · +0.26 /turn" and the category breakdown is collapsed (clickable open).
		"days_of_food": 22.0,
		# Good morale (≥ warn, not falling) → the Morale row is collapsed with a ▸ caret. The signed
		# Layer-1 contributions (above the breakdown epsilon) give the disclosure real content on expand.
		"morale": 0.82,
		"morale_settling": 0.012,
		"morale_terrain": -0.010,
		"morale_climate": -0.006,
		"stores": {"provisions": 84.0},
		# Early-Game Labor (slice 3b): 16 working-age workers, 3 idle, split across a
		# Forage tile, a Hunt herd, and the Scout + Warrior band-wide roles.
		"working_age": 16,
		"idle_workers": 3,
		# Server's hard party-size cap (expedition config, default 8) — the outfit stepper maxes at
		# min(idle, this).
		"max_expedition_party_size": 8,
		# Global config levers echoed on every cohort. They are DISPLAY levers — neither computes
		# a trip length. The targeting banner's turns-to-fill is a PURE LOOKUP into the target herd's
		# `hunt_trip_estimates` (the sim forward-simulates the trip and exports the answer); the client
		# does ZERO arithmetic for an expedition and never divides a carry cap by a rate.
		#   expedition_viability_warn_turns — the viable/not-viable threshold applied to turns_to_fill.
		#   hunt_per_worker_provisions      — one hunter's throughput, used ONLY by the resident-band
		#     LOCAL hunt preview, which IS arithmetic: min(workers × 0.8, band_ceiling) × output_mult.
		# Band = flow arithmetic; expedition = lookup.
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		# Per-worker carry (shipped 4.0): the forecast shows the HAUL a filled pack delivers as
		# party × this (blessed party×lever arithmetic, NOT the turns-to-fill lookup).
		"expedition_per_worker_carry": 4.0,
		"work_range": 2,
		# Hunt reach (work_range + hunt leash) — large enough here that BOTH the reference herd_fixture
		# (9 tiles from this band's pos) and the occupied-hex herd (16 tiles) stay WITHIN reach, so those
		# herd states render the LOCAL "Assign Local Hunt" controls (the far-herd expedition path has its
		# own dedicated fixtures, _hunt_distance_bands).
		"hunt_reach": 16,
		"scout_reveal_radius": 2,
		"activity": "forage",
		# Band food flow (Food summary line): total income across the worked sources vs the cohort's
		# consumption. Net = 0.94 − 0.68 = +0.26 (positive → larder growing), shown green on the Food
		# line. Per-source actual/sustainable yields live on the assignments below; the hunt overdraws
		# (0.46 > 0.20) so its allocation row shows the ⚠ flag; forage (actual == sustainable) never does.
		# The Gathered/Hunted breakdown sums the assignment actual_yields (0.48 / 0.46) by kind.
		"food_income": 0.94,
		"food_consumption": 0.68,
		# `workers_needed` is the overstaffing axis, INDEPENDENT of the overdraw (⚠) axis — the two
		# rows below deliberately cross them so one frame proves both:
		#   • forage: 5 assigned but only 1 needed (the patch's ceiling caps the take) → the amber
		#     "· only 1 of 5 working" note, and NO ⚠ (actual == sustainable, perfectly renewable).
		#   • hunt: 4 assigned, 4 needed → no note, but it DOES overdraw (0.46 > 0.20) → the ⚠.
		"labor_assignments": [
			{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "policy": "sustain", "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1},
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20, "workers_needed": 4},
			{"kind": "scout", "workers": 2},
			{"kind": "warrior", "workers": 2},
		],
		"tile_info": {
			"x": 71, "y": 18,
			"terrain_label": "Freshwater Marsh",
			"tags_text": "Freshwater, Wetland",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A band that KEEPS A CORRAL: the third term of the food ledger. Its one keeper works the penned
## Red Deer herd (the sim pays the pen's GROSS managed yield, 5.40), and the herd eats 1.74/turn off
## the band's larder — `pen_feed_upkeep`, exported by the sim (`PopulationCohortState.penFeedUpkeep`)
## precisely so the client never has to sum it. Numbers are the design doc's measured Red Deer pen at
## its escapement operating point (B* = K/2): gross 5.40, feed 1.74, net 3.66.
func _pen_keeper_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = PEN_KEEPER_BAND_ENTITY
	band["id"] = "Band 4"
	band["days_of_food"] = 22.0
	band["food_income"] = 5.88          # forage 0.48 + the pen's gross 5.40
	band["food_consumption"] = 1.15     # the PEOPLE's meals
	band["pen_feed_upkeep"] = 1.74      # the ANIMALS' feed — a debit in neither row above
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "policy": "sustain", "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1},
		# A managed source: one keeper, take == sustainable (escapement), so no ⚠ and no overstaff note.
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "policy": "corral", "target_x": 70, "target_y": 17, "actual_yield": 5.40, "sustainable_yield": 5.40, "workers_needed": 1},
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
	band["days_of_food"] = 3.0
	band["food_income"] = 1.32          # forage 0.48 + the shrunken pen's 0.84
	band["pen_feed_upkeep"] = 0.70      # PAID, not demanded — the herd starves for the difference
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "policy": "sustain", "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1},
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "policy": "corral", "target_x": 70, "target_y": 17, "actual_yield": 0.84, "sustainable_yield": 0.84, "workers_needed": 1},
		{"kind": "scout", "workers": 2},
	]
	return band

## A CONCERNING food state: net-negative flow (income 0.30 < consumption 0.95 → net −0.65) and a
## low larder runway (4 days). Both trip `_food_is_concerning`, so the category breakdown auto-shows
## under a red net figure without any click.
func _concerning_food_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 905
	band["id"] = "Band 3"
	band["days_of_food"] = 4.0
	band["food_income"] = 0.30
	band["food_consumption"] = 0.95
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "target_x": 71, "target_y": 18, "actual_yield": 0.15, "sustainable_yield": 0.15},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.15, "sustainable_yield": 0.20},
		{"kind": "scout", "workers": 2},
	]
	return band

## A scouting expedition (docs/plan_exploration_and_sites.md §2) in its awaiting-orders phase:
## a detached party (is_expedition) carrying a mission/phase + party size + provisions. The drawer
## renders the dedicated expedition readout + Recall/Move panel, not the labor-allocation UI.
func _expedition_fixture() -> Dictionary:
	return {
		"id": "Scouts 1",
		"size": 6,
		"entity": 7001,
		"faction": 0,
		"pos": [80, 30],
		"days_of_food": 9.0,
		"stores": {"provisions": 48.0},
		"is_expedition": true,
		"expedition_mission": "scout",
		"expedition_phase": "awaiting",
		"tile_info": {
			"x": 80, "y": 30,
			"terrain_label": "Highland Tundra",
			"tags_text": "Cold, Exposed",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

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
		"days_of_food": 4.0,
		# Carried 8 of a 16 carry cap → "Carried 8 / 16".
		"stores": {"provisions": 8.0},
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_07",
		"expedition_hunt_policy": "surplus",
		"expedition_carry_cap": 16.0,
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
	var fixture := _band_fixture()
	fixture["id"] = "Band 5"
	fixture["entity"] = 905
	fixture["days_of_food"] = 999.0
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
		{"faction": 0, "entity": 101, "size": 60, "days_of_food": 12.0, "activity": "harvest", "current_x": 71, "current_y": 18},
		{"faction": 0, "entity": 102, "size": 90, "days_of_food": 999.0, "activity": "hunt", "current_x": 40, "current_y": 22},
		{"faction": 0, "entity": 103, "size": 45, "days_of_food": 999.0, "activity": "harvest", "current_x": 12, "current_y": 9},
	]

func _band_alert_fixture() -> Array:
	return [
		# Starving: 3 days of food (< critical) → red alert.
		{"faction": 0, "entity": 101, "size": 60, "days_of_food": 3.0, "activity": "harvest", "current_x": 71, "current_y": 18,
			"harvest": {"band_label": "Band Fen"}},
		# Losing population to relocation: size 90 → 78, well-fed (∞) but discontented and
		# 12 people emigrated last turn → amber alert "losing population — people leaving".
		{"faction": 0, "entity": 102, "size": 78, "days_of_food": 999.0, "morale": 0.30, "morale_cause": 1, "last_emigrated": 12, "activity": "hunt", "current_x": 40, "current_y": 22,
			"harvest": {"band_label": "Band Ash"}},
		# Idle labor: quiet low-priority alert.
		{"faction": 0, "entity": 103, "size": 45, "days_of_food": 999.0, "activity": "idle", "current_x": 12, "current_y": 9},
	]

## Two player bands (multi-band split is deferred, but the assign controls' band-picker must
## handle N). Different idle_workers so switching the dropdown visibly re-caps the worker
## stepper; neither hunts the deer herd, so the cap for a fresh source == idle_workers.
func _two_player_bands() -> Array:
	# hunt_reach 6 keeps both bands WITHIN local reach of the (66,10) herd (distances 0 and 3), so the
	# band-picker states test the LOCAL-hunt re-cap (the distance-aware expedition path is exercised by
	# _hunt_distance_bands below).
	return [
		{"entity": 801, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 12, "hunt_reach": 6, "activity": "forage", "labor_assignments": []},
		{"entity": 802, "faction": 0, "size": 40, "current_x": 68, "current_y": 12,
			"working_age": 6, "idle_workers": 2, "hunt_reach": 6, "activity": "hunt", "labor_assignments": []},
	]

## Distance-aware herd-hunt (docs/plan_exploration_and_sites.md §2b): two player bands at DIFFERENT
## distances from ONE herd — a NEAR band ON the herd tile (within hunt_reach → LOCAL hunt) and a FAR
## band ~27 tiles away (beyond reach → hunting EXPEDITION). Proves the SELECTED band (band-picker)
## drives the local-vs-expedition label + command + band-entity target — the case single-band
## playtest can't surface. Both carry idle workers + a party cap so either verb is dialable.
func _hunt_distance_bands() -> Array:
	return [
		{"entity": 811, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 10, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "forage", "labor_assignments": []},
		{"entity": 812, "faction": 0, "size": 80, "current_x": 86, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "hunt", "labor_assignments": []},
	]

## Range-aware forage: two player bands at DIFFERENT distances from the (66,10) food tile — a NEAR band
## 1 tile away (within work_range 2 → forage ENABLED) and a FAR band ~21 tiles away (beyond range →
## forage DISABLED + out-of-range hint). Foraging is stationary gathering, so out-of-range has NO
## expedition fallback — just a disabled button. Proves the SELECTED band (band-picker) drives the
## enabled-vs-disabled state — the case single-band playtest can't surface.
func _forage_range_bands() -> Array:
	return [
		{"entity": 821, "faction": 0, "size": 120, "current_x": 67, "current_y": 10,
			"working_age": 14, "idle_workers": 10, "work_range": 2, "activity": "forage", "labor_assignments": []},
		{"entity": 822, "faction": 0, "size": 80, "current_x": 80, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "work_range": 2, "activity": "forage", "labor_assignments": []},
	]

## The herd the distance-aware states select — the same (66,10) herd but a NON-food tile_info, so the
## Tile card drops its "Assign foragers" block and the hunt button + distance hint sit in-frame.
func _hunt_distance_herd() -> Dictionary:
	var herd := _herd_fixture()
	herd["tile_info"] = _plain_herd_tile_info()
	return herd

## A Wild Boar carrying the server's MEASURED raid (K=1433, body 50, B=1010, 4 food/hunter): 1 hunter →
## 5 animals / 7 turns, 2 → 8 / 8, 3 → 8 / 4. `animalsTaken` plateaus at 8 (party 2), so max-useful = 2.
## The frame the "delivers ≈5 Wild Boar over ≈7 turns" readout and the stepper-cap-at-plateau are judged
## on. `food_per_animal` = 4 so the readout appends the food total (~20 at 5 animals, ~32 at 8).
func _raid_boar_herd() -> Dictionary:
	var herd := _assign_preview_herd("game_boar_04", "Wild Boar", "thriving", 0.30, 0, 0)
	herd["food_per_animal"] = BOAR_FOOD_PER_ANIMAL
	herd["hunt_trip_estimates"] = _raid_estimate_table(BOAR_RAID_TURNS, BOAR_RAID_ANIMALS)
	return herd

## A raid estimate TABLE from a per-party Sustain (turns, animals) pair (index i = a party of i+1). The
## deeper policies raid to a lower floor, so they take MORE animals (Surplus < Market < Eradicate) — the
## per-policy ASCENDING the picker buttons read. Eradicate takes the most but delivers NO food (denial —
## `delivers_food = false`). The per-policy bumps are illustrative fixture data; the live sim exports the
## real per-floor counts.
func _raid_estimate_table(turns_row: Array, animals_row: Array) -> Dictionary:
	var table := {}
	for i in animals_row.size():
		var turns := int(turns_row[i])
		var base := int(animals_row[i])
		table["sustain:%d" % (i + 1)] = {
			"turns_to_fill": turns, "delivers_food": true, "animals_taken": base,
		}
		table["surplus:%d" % (i + 1)] = {
			"turns_to_fill": turns, "delivers_food": true, "animals_taken": base + 2,
		}
		table["market:%d" % (i + 1)] = {
			"turns_to_fill": turns, "delivers_food": true, "animals_taken": base + 3,
		}
		table["eradicate:%d" % (i + 1)] = {
			"turns_to_fill": turns, "delivers_food": false, "animals_taken": base + 5,
		}
	return table

## A herd stripped to its policy floor: EVERY (policy, party) cell delivers 0 animals, so the raid comes
## home empty at any size — the one non-viable case (surplus is a property of the HERD, not the party, so
## no party size fixes it). The button must be DISABLED with the "too lean to raid" reason.
func _no_surplus_herd() -> Dictionary:
	var herd := _assign_preview_herd("game_rabbit_02", "Rabbit Warren", "thriving", 0.05, 0, 0)
	herd["size_class"] = "small"
	var table := {}
	for w in range(1, 9):
		for policy in ["sustain", "surplus", "market"]:
			table["%s:%d" % [policy, w]] = {
				"turns_to_fill": 0, "delivers_food": true, "animals_taken": 0,
			}
		table["eradicate:%d" % w] = {
			"turns_to_fill": 0, "delivers_food": false, "animals_taken": 0,
		}
	herd["hunt_trip_estimates"] = table
	return herd

## A hex in a given SIGHT state, deliberately carrying a herd in ALL THREE — including the unseen
## ones, where MapView would never have put one (it fog-gates `_herds_on_tile` at source). Feeding the
## HUD a "leaky" dict on purpose proves the HUD's own gate: on a Discovered/Unexplored hex it must
## refuse to list the herd and must say the contents are unknown, rather than showing an empty roster
## (which would read as "nothing here" — the exact lie this slice exists to kill).
func _sight_tile_fixture(visibility_state: String) -> Dictionary:
	var tile := _food_tile_fixture()
	tile["visibility_state"] = visibility_state
	tile["herds"] = [_herd_fixture()]
	tile["herd_count"] = 1
	return tile

## YOUR OWN scouting expedition standing on an UNEXPLORED hex — the case the fog rule must NOT break.
## The tile carries the party AND a herd; the herd is redacted (nobody can see it), but the party stays.
func _own_expedition_unexplored_tile() -> Dictionary:
	var tile := _sight_tile_fixture(VIS_UNEXPLORED)
	tile["units"] = [_expedition_fixture()]
	tile["unit_count"] = 1
	return tile

## A FOREIGN band (faction 1) on a hex in the given sight state. On an unseen hex it must vanish from
## the roster (it is not ours); on a visible hex it lists normally with a neutral dot.
func _foreign_band_tile(visibility_state: String) -> Dictionary:
	var tile := _food_tile_fixture()
	tile["visibility_state"] = visibility_state
	tile["units"] = [{
		"id": "Rival Band",
		"entity": 6001,
		"faction": 1,
		"size": 63,
		"pos": [66, 10],
		"activity": "forage",
	}]
	tile["unit_count"] = 1
	return tile

## A NON-food hex under the herd, so the Tile card drops its "Assign foragers" block and the herd's
## assign controls (stepper + policy + forecast + button) sit fully in-frame.
func _plain_herd_tile_info() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## The herd-panel EXPEDITION forecast states (herd beyond hunt_reach), each also naming the composed
## POLICY — because the policy is half the key (`"<policy>:<party_workers>"`) the forecast looks up in
## the herd's `hunt_trip_estimates`. Re-deriving a Surplus trip from the BAND's flow ceiling instead of
## reading the sim's row was the bug these cover.
func _hunt_assign_forecast_states() -> Array:
	return [
		{
			# A brisk raid: Sustain on a Thunder Mammoth delivers ≈8 animals in 6 turns → cyan line,
			# primary "Send Hunting Expedition".
			"name": "herd_hunt_forecast_viable",
			"policy": "sustain",
			"herd": _assign_preview_herd("game_mammoth_11", "Thunder Mammoth", "thriving", 2.7,
				MAMMOTH_SUSTAIN_TRIP_TURNS, MAMMOTH_SURPLUS_TRIP_TURNS,
				MAMMOTH_SUSTAIN_ANIMALS, MAMMOTH_SUSTAIN_ANIMALS),
		},
		{
			# A SLOW raid: Sustain on a Red Deer still delivers ≈6 animals, but over 54 turns — past the
			# band's warn threshold (20) → amber "⚠ … — a slow raid" + "Send Anyway (≈54 turns)".
			"name": "herd_hunt_forecast_slow",
			"policy": "sustain",
			"herd": _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
		{
			# The SAME Red Deer on Surplus: a Surplus raid strips deeper (≈12 animals) and comes home in
			# ~6 turns — a brisk, richer raid. Reading the sim's row, never re-deriving it.
			"name": "herd_hunt_forecast_surplus",
			"policy": "surplus",
			"herd": _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
		{
			# No surplus: a collapsing Wild Fowl flock is at/below its floor → animalsTaken = 0, the raid
			# returns empty → red "too lean to raid" + the DISABLED "Herd too lean to raid" button.
			"name": "herd_hunt_forecast_no_surplus",
			"policy": "sustain",
			"herd": _assign_preview_herd("game_fowl_03", "Wild Fowl", "collapsing", 0.0,
				NEVER_FILLS_TRIP_TURNS, NEVER_FILLS_TRIP_TURNS,
				NO_SURPLUS_ANIMALS, NO_SURPLUS_ANIMALS),
		},
		{
			# Eradicate: the sim marks the row `delivers_food = false` — a DENIAL mission delivers no
			# food BY DESIGN (the client never infers that from the policy string). Stays ENABLED.
			"name": "herd_hunt_forecast_eradicate",
			"policy": "eradicate",
			"herd": _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
	]

## A forecast herd (carrying BOTH sim-exported per-policy ceiling tables) as a SELECTED herd — i.e. on
## a plain tile, the way `show_herd_selection` receives it — rather than as a hovered hex.
func _assign_preview_herd(id: String, species: String, phase: String, sustain_ceiling: float,
		trip_turns: int, surplus_trip_turns: int,
		sustain_animals: int = 0, surplus_animals: int = 0) -> Dictionary:
	var herd := _forecast_herd(id, species, phase, sustain_ceiling, trip_turns, surplus_trip_turns,
		sustain_animals, surplus_animals)
	herd["huntable"] = true
	herd["tile_info"] = _plain_herd_tile_info()
	return herd

## The band the herd-panel EXPEDITION preview states staff: it carries the forecast levers (the global
## config values echoed on every cohort) and sits at (86,24) — ~27 tiles from the (66,10) herd, beyond
## its hunt_reach 7, so every herd resolves to the expedition branch.
func _hunt_preview_far_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 831, "faction": 0, "size": 80,
		"current_x": 86, "current_y": 24, "pos": [86, 24],
		"working_age": 10, "idle_workers": 6,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		# Per-worker carry (shipped 4.0) → the forecast's HAUL = party × this.
		"expedition_per_worker_carry": 4.0,
		"activity": "forage", "labor_assignments": [],
	}

## A band 8 tiles from the (66,10) herd (beyond hunt_reach 7 → expedition) carrying a MOVE RATE, so the
## raid forecast's round-trip travel is exercised: ceil(2 × 8 / 2) = 8 travel turns added to the hunting
## turns. `band_move_tiles_per_turn` now ships on the wire (schema slot 124) and is decoded onto the band;
## this carries the same value the decoder surfaces.
func _raid_travel_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 833, "faction": 0, "size": 80,
		"current_x": 66, "current_y": 18, "pos": [66, 18],
		"working_age": 10, "idle_workers": 6,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"expedition_viability_warn_turns": 20,
		"expedition_per_worker_carry": 4.0,
		"band_move_tiles_per_turn": 2,
		"activity": "forage", "labor_assignments": [],
	}

## The band the herd-panel LOCAL preview states staff: it sits ON the (66,10) herd (distance 0 ≤ reach
## 7 → local branch) and runs at a REDUCED `output_multiplier` (0.9), so the yield preview visibly
## applies the band's morale/discontent productivity modifier — the one term that makes a resident
## hunt's take differ from an expedition's.
func _hunt_preview_local_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 832, "faction": 0, "size": 120,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 14, "idle_workers": 10,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 0.9,
		"activity": "hunt", "labor_assignments": [],
	}

func _food_tile_fixture() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		# Fertile steppe: low drain → "Hospitable" (green Tile-card row).
		"habitability": 0.01,
		# Mid-latitude ~18° → "Temperate" climate band (neutral Tile-card row).
		"temperature": 18.0,
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
		# A discovered Wondrous Site on this tile → the Tile card shows a "Site: …" line.
		"site_name": "Verdant Basin",
		# Forage patch being worked toward cultivation → the Tile card's "Cultivation 60%" row.
		"cultivation_progress": 0.6,
		"is_cultivated": false,
		"patch_has_owner": true,
		"patch_owner": 0,
		"patch_ecology_phase": "thriving",
		# Standing forage stock vs the patch ceiling (sim default capacity 120) → the Tile card's
		# "Forage biomass 84 / 120" row, the patch counterpart to a herd's Biomass row.
		"patch_biomass": 84.0,
		"patch_carrying_capacity": 120.0,
		# Pre-commit yield forecast (food/turn at THIS biomass, exported at output_multiplier 1.0).
		# Sustain's ceiling admits ceil(0.96 / 0.32) = 3 useful foragers — below band 821's 10 idle
		# workers, so the Foragers stepper caps at 3 and shows the "max 3 workers useful here" note.
		# The higher-policy ceilings admit 6 / 9 / 15, so switching policy visibly moves the cap.
		"patch_per_worker_yield": 0.32,
		"patch_ceiling_sustain": 0.96,
		"patch_ceiling_surplus": 1.92,
		"patch_ceiling_market": 2.88,
		"patch_ceiling_eradicate": 4.80,
		# The Cultivate INVESTMENT rung: while the patch is being prepared it pays only a fraction of
		# its Sustain ceiling (the dip the player is buying with), then flips to the tended yield.
		# Both are food/turn at output_multiplier 1.0, like the ceilings above.
		"patch_ceiling_cultivate": 0.24,
		"patch_tended_yield": 1.20,
		# Plant RUNG 3 — the Field + the Sow verb. This reference tile is ordinary prairie steppe:
		# rich enough to forage, but it will NOT take seed (rung 3 moves seed, it cannot fertilize or
		# irrigate), so the sim's `sow_site_refusal` verdict rides here and the Sow option is gated
		# with the reason. Only ~1% of a real map is sowable, so REFUSED is the common case and is
		# deliberately the default fixture; `_sowable_tile_fixture` is the exception.
		"patch_field_progress": 0.0,
		"patch_is_field": false,
		"patch_ceiling_sow": 0.0,
		"patch_field_yield": 0.0,
		"patch_sow_site_refusal": "too_dry",
		# The GRAZE (pasture) layer — the ANIMAL-edible twin of the forage patch above (Grazing Phase
		# 2a). Prairie steppe is the reference pasture: capacity 240, standing full, hence Thriving.
		# Rendered as the `Pasture` / `Pasture ecology` rows right under `Forage biomass`, so the card
		# states the two facts side by side: what HUMANS can eat here, and what ANIMALS can eat here.
		"graze_biomass": 240.0,
		"graze_capacity": 240.0,
		"graze_ecology_phase": "thriving",
	}

## An OVERGRAZED pasture: the standing graze has been drawn deep into the stressed band, so the
## `Pasture ecology` row reads a WARN-amber "⚠ Stressed" — the SAME label + tint a stressed herd or a
## stressed forage patch gets (one ecology vocabulary, one styling path). Nothing eats graze until
## Phase 2b, so this state cannot occur in a live 2a map; it renders the path the tint will take.
func _overgrazed_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["x"] = 68
	tile["graze_biomass"] = 61.0
	tile["graze_ecology_phase"] = "stressed"
	return tile

## Ground that carries NO pasture at all (a glacier — the biome's graze capacity is a stated 0, so the
## sim holds no patch there and the tile carries no graze fields). The card must print NOTHING about
## pasture here — never "0 / 0", which would read as a starved pasture rather than an absent one.
func _no_pasture_tile_fixture() -> Dictionary:
	return {
		"x": 66, "y": 3,
		"terrain_label": "Glacier",
		"tags_text": "Polar",
		"visibility_state": "active",
		"habitability": 0.09,
		"temperature": -14.0,
	}

## A plain (no forage patch) tile carrying hex-EDGE rivers on some of its sides. Deliberately
## bare of food-module keys so the Tile card is just the terrain-intrinsic rows and the river
## row(s) read unobstructed.
func _river_tile_fixture(river_mask: int) -> Dictionary:
	return {
		"x": 9, "y": 36,
		"terrain_label": "Sinkhole Field",
		"tags_text": "none",
		"visibility_state": "active",
		"habitability": 0.03,
		"temperature": 15.0,
		"river_edges": river_mask,
	}

## The three pre-launch hunt-forecast states, each a hovered hex carrying one huntable herd whose
## exported `hunt_trip_estimates` row (the sim's forward-simulated turns-to-fill, which the banner
## LOOKS UP — it computes nothing) puts the same 4-worker Sustain party in a different place:
## comfortably viable, viable-but-a-trap, and never fills.
func _hunt_forecast_states() -> Array:
	return [
		{
			# A brisk raid: ≈8 animals over ≈6 turns.
			"name": "hunt_forecast_viable",
			"tile": _herd_hover_tile(_forecast_herd(
				"game_mammoth_11", "Thunder Mammoth", "thriving", 2.7,
				MAMMOTH_SUSTAIN_TRIP_TURNS, MAMMOTH_SURPLUS_TRIP_TURNS,
				MAMMOTH_SUSTAIN_ANIMALS, MAMMOTH_SUSTAIN_ANIMALS
			)),
		},
		{
			# A slow raid: ≈6 animals, but over 54 turns (past the warn line) → amber "a slow raid".
			"name": "hunt_forecast_slow",
			"tile": _herd_hover_tile(_forecast_herd(
				"game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS
			)),
		},
		{
			"name": "hunt_forecast_no_surplus",
			# A collapsing (sub-Allee) flock at/below its floor: animalsTaken = 0, so the raid would
			# come home empty → red "too lean to raid".
			"tile": _herd_hover_tile(_forecast_herd(
				"game_fowl_03", "Wild Fowl", "collapsing", 0.0,
				NEVER_FILLS_TRIP_TURNS, NEVER_FILLS_TRIP_TURNS,
				NO_SURPLUS_ANIMALS, NO_SURPLUS_ANIMALS
			)),
		},
	]

## A herd carrying the two DIFFERENT things the sim exports for the two DIFFERENT actors:
##   `hunt_policy_ceilings` — the BAND's renewable FLOW ceiling {policy → provisions/turn}. The local
##       hunt preview is pure arithmetic over it (Sustain's entry IS the herd's sustainable yield).
##   `hunt_trip_estimates` — the sim's forward-SIMULATED expedition trip answers, keyed
##       `"<policy>:<party_workers>"` → `{turns_to_fill, delivers_food}`. An expedition's trip is NOT a
##       rate division (on Surplus/Market the ceiling is a *stock* the party strips in a turn or two,
##       then it crawls at the regrowth trickle), so the client looks the answer up and does no math.
##       `turns_to_fill == 0` → won't fill within the horizon; `delivers_food == false` → denial.
## `trip_turns` is the simulated turns-to-fill for the 4-worker party these states dial in.
func _forecast_herd(id: String, species: String, phase: String, sustain_ceiling: float,
		trip_turns: int = 0, surplus_trip_turns: int = 0,
		sustain_animals: int = 0, surplus_animals: int = 0) -> Dictionary:
	return {
		"id": id,
		"label": "%s (%s)" % [species, id],
		"species": species,
		"size_class": "big",
		"huntable": true,
		"ecology_phase": phase,
		"x": 66, "y": 10,
		"biomass": 820.0,
		# One animal's worth of FOOD (provisions), `HerdTelemetryState.foodPerAnimal` — drives the
		# kill-rhythm on the local-hunt preview (food ÷ food).
		"food_per_animal": 2.0,
		# A LIVE herd carries BOTH forecast field sets, so this fixture must too (they were split
		# across two disjoint fixtures once, which hid every interaction between them):
		#   • the bare `per_worker_yield` / `ceiling_*` pre-commit fields, which drive the shared
		#     `_forecast_inputs` → cap + "Expected yield" / "Preparing → then" row, and
		#   • `hunt_policy_ceilings` / `hunt_trip_estimates` below (the BAND flow ceiling and the
		#     sim's forward-simulated EXPEDITION trip answers).
		# Per-worker matches the band's `hunt_per_worker_provisions` (0.8) and the ceilings match the
		# band ceilings, because the sim exports one hunt model — the two paths must agree.
		"per_worker_yield": 0.8,
		"ceiling_sustain": sustain_ceiling,
		"ceiling_surplus": sustain_ceiling * 4.0,
		"ceiling_market": sustain_ceiling * 2.0,
		"ceiling_eradicate": 0.0,
		"hunt_policy_ceilings": {
			"sustain": sustain_ceiling,
			"surplus": sustain_ceiling * 4.0,
			"market": sustain_ceiling * 2.0,
			"eradicate": 0.0,
		},
		"hunt_trip_estimates": {
			"sustain:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": trip_turns, "delivers_food": true,
				"animals_taken": sustain_animals,
			},
			"surplus:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true,
				"animals_taken": surplus_animals,
			},
			"market:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true,
				"animals_taken": surplus_animals,
			},
			# Denial: the sim says so via `delivers_food`, the client never infers it from the policy.
			"eradicate:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": 0, "delivers_food": false, "animals_taken": surplus_animals,
			},
		},
	}

## The hovered-hex payload MapView.tile_hovered delivers (Hud.show_tooltip): the herds the hex carries.
func _herd_hover_tile(herd: Dictionary) -> Dictionary:
	var tile := _food_tile_fixture()
	tile["herds"] = [herd]
	return tile

## An over-drawn, UNCULTIVATED forage patch: the Tile card's "Ecology" row must still render
## (the phase gates cultivation, so it always shows on a patch) as a WARN-amber "⚠ Stressed".
## Biomass is well below capacity, mirroring a patch foraged past its regrowth.
func _stressed_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["cultivation_progress"] = 0.0
	tile["is_cultivated"] = false
	tile["patch_ecology_phase"] = "stressed"
	tile["patch_biomass"] = 22.0
	return tile

## A fully-tended forage patch: the Tile card shows the "🌾 Tended Patch" badge (SIGNAL tint)
## plus an "Ecology" row, instead of the in-progress "Cultivation N%".
func _tended_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["x"] = 67
	tile["y"] = 11
	tile["cultivation_progress"] = 1.0
	tile["is_cultivated"] = true
	tile["patch_ecology_phase"] = "thriving"
	# A TENDED patch reports every policy ceiling == per_worker_yield, so max-useful collapses to 1
	# worker regardless of policy — the stepper caps at 1 ("max 1 workers useful here").
	tile["patch_ceiling_sustain"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_surplus"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_market"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_eradicate"] = tile["patch_per_worker_yield"]
	return tile

## QUALIFYING GROUND for `Sow` — an alluvial plain beside fresh water, i.e. one of the ~46 tiles of
## 4160 (1.1%) on the standard map that will actually take seed. `patch_sow_site_refusal` is "" (the
## sim's verdict: no fault), so the ▦ Sow option ENABLES once Seed Selection is known. The Sow
## forecast pair is deliberately asymmetric with Cultivate's: `ceiling_sow` is ~0 because a sown
## patch has no standing crop to take a fraction of (a bare-ground sow is PURE investment), and
## `field_yield` is 2× the tended yield — the payoff that makes the ladder's top plant rung worth it.
func _sowable_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	# Kept WITHIN the reference band's forage range (it sits on 66,10 with work_range 2) so the Forage
	# button ENABLES: this state exists to judge the Sow affordance, and an out-of-range tile disables
	# the button for an unrelated reason and hides exactly what the frame is for.
	tile["x"] = 67
	tile["y"] = 11
	tile["terrain_label"] = "Alluvial Plain"
	tile["tags_text"] = "Fertile, Fresh Water"
	tile["food_module"] = "riverine_delta"
	tile["food_module_label"] = "Riverine Delta"
	tile["site_name"] = ""
	# The ground answers the site requirement: rich enough AND watered. No refusal.
	tile["patch_sow_site_refusal"] = ""
	tile["patch_ceiling_sow"] = 0.02
	tile["patch_field_yield"] = 2.40
	return tile

## The OTHER refusal. `_food_tile_fixture` is "too_dry" (rich prairie away from water); this is thin
## upland ground — watered, but too poor to take a crop without fertilizing. The two messages must
## differ, name different faults, and each point at the rung that lifts it.
func _sow_too_poor_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	# In range of the reference band, like `_sowable_tile_fixture` — the refusal must be the ONLY
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

## A patch mid-SOW: the rung-3 build meter is running, so the Field row reads "Sowing 45%". It sits
## BESIDE the Cultivation row (this ground was tended first) — the two meters are independent and
## both are the SOURCE's own, which is the per-source half of the two-meter split.
func _sowing_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["cultivation_progress"] = 1.0
	tile["is_cultivated"] = true
	tile["patch_field_progress"] = 0.45
	tile["patch_is_field"] = false
	return tile

## A COMPLETED Field — the top of the plant ladder. The row must read "▦ Field" (SIGNAL), a visibly
## DIFFERENT THING from "🌾 Tended Patch", not a bigger percentage.
func _field_tile_fixture() -> Dictionary:
	var tile := _sowing_tile_fixture()
	tile["patch_field_progress"] = 1.0
	tile["patch_is_field"] = true
	# A completed Field reports every ceiling == per_worker_yield (a managed source needs one worker),
	# exactly as a tended patch does — so the stepper caps at 1.
	tile["patch_ceiling_sustain"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_surplus"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_market"] = tile["patch_per_worker_yield"]
	tile["patch_ceiling_eradicate"] = tile["patch_per_worker_yield"]
	return tile

## A herd mid-TAME on a pen-ceiling species: the 🐾 Tame rung is available and selected, the herd's
## OWN meter reads 40% (`domestication`), and Corral is still gated on Penning. This is the frame the
## TWO-METER SPLIT is judged on — see the `two_meter_split` state.
func _taming_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["husbandry_ceiling"] = "pen"
	fixture["domestication"] = 0.4
	fixture["ecology_phase"] = "thriving"
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## The same herd, STRESSED — the "why isn't my Tame progressing?" case. Taming accrues only while the
## herd is Thriving, but the verb is NOT gated on it (a herd's phase swings as you hunt it): the sim
## just PAUSES the meter. Nothing else in the HUD would tell the player, so the drawer must.
func _taming_stalled_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["ecology_phase"] = "stressed"
	return fixture

## A nearly-tamed herd, FULLY STAFFED — the calm control for the staffing readout. Domestication is
## near-complete and `herded_fraction` is 1.0 (every needed herder present), so the herd holds its
## tameness and earns Penning normally: the drawer shows a neutral "Herders: 4 / 4" with NO warning.
func _fully_herded_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["domestication"] = 0.9
	fixture["herders_needed"] = 4
	fixture["herded_fraction"] = 1.0
	return fixture

## The SAME herd, UNDER-HERDED — the playtest bug made visible. Only half the needed herders are on it
## (`herded_fraction` 0.5), so its tameness is slipping: domestication decays, the herd will drop back
## to WILD and stop earning Penning. `domestication` sits at 0.98 (rounds to "Domesticating 100%", the
## exact reading that used to look fine), so the drawer must NOT read as OK — the amber "Herders: 2 / 4
## — under-herded" row and the muted "Tameness slipping — teaching Herding, not Penning…" line carry it.
func _under_herded_herd_fixture() -> Dictionary:
	var fixture := _fully_herded_herd_fixture()
	fixture["domestication"] = 0.98
	fixture["herded_fraction"] = 0.5
	return fixture

## The world's herd list (Main pushes snapshot["herds"]). Named because the turn-orb starving-pen
## state swaps in its own list and must restore this one.
func _world_herds_fixture() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120, "ecology_phase": "stressed", "food_per_animal": 2.0},
	]

## Two herds a band works at once — a FAST animal (several a turn) and a BIG one (one every several
## turns) — so the Current-actions rows can show both kill-RHYTHMs. `food_per_animal` is in PROVISIONS
## (`HerdTelemetryState.foodPerAnimal`, the decoded key), matched to the assignment's food rate:
## mammoth 16 food/animal ÷ 2.4 food/turn ≈ 7 turns; fowl 2.0 ÷ 2.6 ≈ 1.3/turn.
func _hunt_rhythm_herds_fixture() -> Array:
	return [
		{"id": "game_fowl_01", "species": "Marsh Fowl", "x": 71, "y": 18, "food_per_animal": 2.0},
		{"id": "game_mammoth_01", "species": "Woolly Mammoth", "x": 70, "y": 17, "food_per_animal": 16.0},
	]

## A band worked on TWO hunt sources — the render-honesty frame for the kill-RHYTHM (fix #1) and the
## under-crewed `wastedYield` note (fix #5). Row 1 is a FAST animal (its honest per-turn rate reads as
## several a turn); row 2 a BIG animal whose `actualYield` is 0.00 THIS turn — the "+0.00 /turn" lie
## the row used to headline — and which is under-crewed, so the muted "· N wasted" note shows.
func _hunt_actions_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["labor_assignments"] = [
		# Fast: honest rate 2.60/turn, body 2.0 → 1.3 animals/turn → "≈1.3 Marsh Fowl/turn". A fast
		# Sustain animal takes several a turn, so actual ≈ sustainable (no overdraw ⚠).
		{"kind": "hunt", "workers": 3, "fauna_id": "game_fowl_01", "policy": "sustain",
			"target_x": 71, "target_y": 18, "actual_yield": 2.60, "sustainable_yield": 2.60,
			"workers_needed": 3},
		# Big: honest rate 2.40/turn (the sim's measured Mammoth Sustain), 16 food/animal → ceil(16/2.4)
		# = 7 → "≈1 Woolly Mammoth / 7 turns". actual_yield 0.00 = a wait turn of the kill pulse (the old
		# lie the row used to headline). Under-crewed → the muted "· 1.9 wasted".
		{"kind": "hunt", "workers": 2, "fauna_id": "game_mammoth_01", "policy": "sustain",
			"target_x": 70, "target_y": 17, "actual_yield": 0.00, "sustainable_yield": 2.40,
			"workers_needed": 5, "wasted_yield": 1.9},
	]
	return band

func _herd_fixture() -> Dictionary:
	return {
		"id": "game_deer_07",
		"label": "Red Deer (game_deer_07)",
		"species": "Red Deer",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "thriving",
		"domestication": 0.4,
		"x": 66, "y": 10,
		"biomass": 820.0,
		# Ecological carrying capacity + grazing range (Grazing Phase 2b-iii): the numbers that explain
		# the herd's size. Big game roams a radius-1 range (7 tiles); on good steppe it caps ~2150, well
		# above this herd's 820 biomass, so the drawer reads the healthy "Biomass: 820 / 2150" pair with
		# no overgrazing warning. The dedicated grazing states below dial in overgrazed / small-game.
		"carrying_capacity": 2150.0,
		"graze_range_radius": 1,
		"route_length": 3,
		# One animal's worth of FOOD (provisions) — `HerdTelemetryState.foodPerAnimal`, the exact key the
		# decoder now emits. The kill-rhythm divides it by the food rate (both provisions): 2.0
		# food/animal vs a 0.90/turn Sustain take reads "≈1 Red Deer / 3 turns".
		"food_per_animal": 2.0,
		# Pre-commit yield forecast — the SAME field names the forage patch carries (food/turn at this
		# herd's biomass, at output_multiplier 1.0). Sustain admits ceil(0.90 / 0.30) = 3 useful
		# hunters, below the reference band's 7 assignable (3 idle + the 4 it already has on this
		# herd), so the Hunters stepper caps at 3 with the "max 3 workers useful here" note.
		"per_worker_yield": 0.30,
		"ceiling_sustain": 0.90,
		"ceiling_surplus": 1.80,
		"ceiling_market": 2.70,
		"ceiling_eradicate": 4.50,
		# The Corral INVESTMENT rung (the herd twin of the patch's Cultivate pair): the dip yield paid
		# while the pen is being built, then the yield the penned herd pays.
		"ceiling_corral": 0.23,
		"corral_yield": 1.05,
		"corral_progress": 0.0,
		# The TAME rung's dip. There is no flat `ceilingTame` scalar on the wire — the Tame ceiling
		# rides the `hunt_policy_ceilings` LIST (the sim exports a row for every one of the six
		# `FollowPolicy::HUNT_POLICIES`), so this is the shape the decoder actually produces and the
		# only place `_tame_forecast_bbcode` can read the number from. The extractive rows match the
		# flat ceilings above, because the sim exports ONE hunt model and the two views must agree.
		"hunt_policy_ceilings": {
			"sustain": 0.90,
			"surplus": 1.80,
			"market": 2.70,
			"eradicate": 4.50,
			"tame": 0.23,
			"corral": 0.23,
		},
		"tile_info": _food_tile_fixture(),
	}

## A WILD-ceiling herd (Grazing 2d-δ): hunt-only. The drawer shows NO husbandry track (no
## domestication / corral / pen rows) — just the "Wild game — hunt only" hint — and the hunt policy
## picker drops the Corral rung.
func _wild_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["husbandry_ceiling"] = "wild"
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## A BIG-GAME wild herd whose WHOLE-ANIMAL body outweighs one hunter's carry — the frame the peak-turn
## carry cap is judged on. An aurochs is one 80-biomass body dropped whole by the kill-credit bank;
## `food_per_animal` 1.6 is that body in food, and one hunter carries only `per_worker_yield` 0.80. So a
## lone hunter carrying an aurochs WASTES half — the panel must say TWO hunters are useful, not one.
##   Sustain ceiling 0.74: old cap = ceil(0.74 / 0.80) = 1 (the bug); new cap =
##     ceil((floor(0.74 / 1.6) + 1) × 1.6 / 0.80) = ceil(1.6 / 0.80) = 2 → "max 2 workers useful".
##   Market ceiling 1.86: two bodies drop on the peak turn → ceil((floor(1.86/1.6)+1) × 1.6 / 0.80) =
##     ceil(3.2 / 0.80) = 4 → the cap tracks the selected policy's ceiling upward.
func _aurochs_big_game_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["id"] = "game_aurochs_04"
	fixture["label"] = "Wild Aurochs (game_aurochs_04)"
	fixture["species"] = "Wild Aurochs"
	fixture["husbandry_ceiling"] = "wild"
	fixture["food_per_animal"] = 1.6
	fixture["per_worker_yield"] = 0.80
	fixture["ceiling_sustain"] = 0.74
	fixture["ceiling_surplus"] = 1.20
	fixture["ceiling_market"] = 1.86
	fixture["ceiling_eradicate"] = 2.60
	fixture["hunt_policy_ceilings"] = {
		"sustain": 0.74, "surplus": 1.20, "market": 1.86, "eradicate": 2.60,
	}
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## A compact NON-food tile_info (like the domesticated/hunt-distance herds) so the tile card stays
## short and the herd drawer's husbandry rows land in-frame rather than below the dock scroll fold.
func _compact_herd_tile_fixture() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## A PASTORAL-ceiling herd (Grazing 2d-δ): tameable + roams, but never pennable. The drawer keeps the
## domestication (Husbandry) row but shows "Herdable, not pennable" where the Corral rows would sit, and
## the hunt policy picker drops the Corral rung.
func _pastoral_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["husbandry_ceiling"] = "pastoral"
	fixture["domestication"] = 0.6
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## A hex with an occupant stack: 3 player bands + 1 herd, for the Occupants roster.
func _occupied_tile_fixture() -> Dictionary:
	return {
		"x": 58, "y": 24,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
		"units": _occupied_units_fixture(),
		"herds": [_occupied_herd_only()],
	}

## Three player bands sharing the hex, spanning the food-status tiers (green /
## amber / red) and distinct activities (harvest / scout / idle glyphs).
func _occupied_units_fixture() -> Array:
	return [
		{"id": "Band Fen", "entity": 301, "faction": 0, "size": 120, "pos": [58, 24],
			"days_of_food": 15.0, "activity": "harvest", "stores": {"provisions": 180.0}},
		{"id": "Band Ash", "entity": 302, "faction": 0, "size": 86, "pos": [58, 24],
			"days_of_food": 7.0, "activity": "scout", "stores": {"provisions": 40.0}},
		{"id": "Band Bryn", "entity": 303, "faction": 0, "size": 54, "pos": [58, 24],
			"days_of_food": 2.0, "activity": "idle", "stores": {"provisions": 8.0}},
	]

## The stressed herd sharing the occupied hex (amber ecology dot).
func _occupied_herd_only() -> Dictionary:
	return {
		"id": "game_bison_02",
		"label": "Steppe Bison (game_bison_02)",
		"species": "Steppe Bison",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "stressed",
		"domestication": 0.0,
		"biomass": 240.0,
		"x": 58, "y": 24,
	}

## The occupied hex's herd carrying its tile_info, so show_herd_selection renders
## the full roster with the wildlife row selected.
func _occupied_herd_fixture() -> Dictionary:
	var herd := _occupied_herd_only()
	herd["tile_info"] = _occupied_tile_fixture()
	return herd

func _collapsing_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["biomass"] = 96.0
	fixture["ecology_phase"] = "collapsing"
	fixture["domestication"] = 0.0
	return fixture

## A compact NON-food tile_info (like the corral fixtures) so the Tile card stays short and the herd
## drawer's Biomass (current/max) / Range (+ overgrazing) rows land in-frame rather than below the fold.
func _compact_herd_tile() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}

## A HEALTHY grazing herd (Grazing Phase 2b-iii): big game (radius-1 range → "Range: 7 tiles") whose
## biomass sits below the K its range supports, so the merged "Biomass: 1480 / 2150" current/max pair
## reads current < max with NO overgrazing warning. domestication 0 keeps the frame focused on the rows.
func _grazing_healthy_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 0.0
	fixture["biomass"] = 1480.0
	fixture["carrying_capacity"] = 2150.0
	fixture["graze_range_radius"] = 1
	fixture["tile_info"] = _compact_herd_tile()
	return fixture

## An OVERGRAZING herd: biomass (2100) exceeds the K (1352) its range can sustainably feed, so the
## merged pair reads "Biomass: 2100 / 1352" (current ABOVE max) and the drawer adds the WARN-amber
## "⚠ Overgrazing — range can't sustain this herd" row. The herd is drawing its range down and will
## shrink — the honest biomass > K comparison, both numbers sim-provided.
func _overgrazing_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 0.0
	fixture["biomass"] = 2100.0
	fixture["carrying_capacity"] = 1352.0
	fixture["graze_range_radius"] = 1
	fixture["tile_info"] = _compact_herd_tile()
	return fixture

## A SMALL-GAME herd (radius-0 range): it grazes only its own tile, so the drawer reads "Range: 1 tile"
## (singular) and the map draws a single-hex highlight. Biomass below its small K → no overgrazing.
func _small_game_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["id"] = "game_rabbit_03"
	fixture["label"] = "Rabbit Warren (game_rabbit_03)"
	fixture["species"] = "Rabbit Warren"
	fixture["size_class"] = "small"
	fixture["domestication"] = 0.0
	fixture["biomass"] = 140.0
	fixture["carrying_capacity"] = 190.0
	fixture["graze_range_radius"] = 0
	fixture["tile_info"] = _compact_herd_tile()
	return fixture

## A still-WILD herd (domestication 0.4) on the same compact tile as the corral-ready one: the Corral
## rung is gated on the herd half of its prerequisite, so the picker greys it with "Herd must be
## domesticated" (the faction already knows Herding).
func _corral_locked_herd_fixture() -> Dictionary:
	var fixture := _corral_ready_herd_fixture()
	fixture["domestication"] = 0.4
	fixture["corral_progress"] = 0.0
	return fixture

## A fully-domesticated herd whose pen is HALF-BUILT (not yet corralled): the Corral investment rung
## is available (knowledge + domestication both satisfied) and under way, so the hunt picker offers
## 🐄 Corral and the drawer reads "Corral: Building 40%". Compact non-food tile_info (like the
## domesticated fixture) so the Tile card stays short and the drawer rows land in-frame.
func _corral_ready_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 1.0
	fixture["corralled"] = false
	fixture["corral_progress"] = 0.4
	# `pen_upkeep` is the feed this pen WOULD demand once built (the sim projects it at the herd's
	# current biomass, on the same basis as `corral_yield`) — so the pre-commit row can quote the
	# real running cost at the moment the player decides, rather than saying "before feed".
	fixture["pen_upkeep"] = 0.34
	fixture["tile_info"] = {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

func _domesticated_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 1.0
	# A fully-domesticated herd is penned: the drawer adds a "🐄 Corralled" row.
	fixture["corralled"] = true
	# A PENNED herd is a managed population — it eats from its keeper's larder every turn. Fully fed
	# here (`pen_fed_fraction` 1.0), so the drawer reads the healthy "🐄 Corralled" badge plus the
	# amber "Pen feed: -1.74 /turn" standing debit.
	fixture["pen_upkeep"] = PEN_UPKEEP_RED_DEER
	fixture["pen_fed_fraction"] = 1.0
	# Grazing 2d-γ — a radius-1 pen on POOR footprint: its fenced land covers NONE of the feed, so the
	# feed-split reads "Fed by pasture 0% · larder 1.7 food/turn" and the full larder bill still stands.
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_pasture_fraction"] = 0.0
	fixture["pen_extend_progress"] = 0.0
	# Compact NON-food tile_info (like the hunt-distance herd) so the tile card stays short and
	# the drawer's Husbandry + Corral rows land in-frame rather than below the dock scroll fold.
	fixture["tile_info"] = {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

## A DOMESTICATED but DEPLETED herd (biomass below the pen's escapement point, K/2): the pen's
## harvest takes only the biomass standing ABOVE K/2, so `corral_yield` is honestly **0.00** — penning
## this herd would eat 0.14 food/turn and pay nothing until it rebuilds. The zero is the whole point
## of the frame: it must render in full (never blanked or em-dashed) and be EMPHASIZED, because a
## player who pens this herd on a hidden zero has been misled by the UI.
func _depleted_corral_herd_fixture() -> Dictionary:
	var fixture := _corral_ready_herd_fixture()
	fixture["biomass"] = 260.0
	fixture["ecology_phase"] = "stressed"
	fixture["corral_progress"] = 0.0
	# Everything scales off the shrunken herd — including the dip, which is a share of its MSY.
	fixture["per_worker_yield"] = 0.10
	fixture["ceiling_sustain"] = 0.10
	fixture["ceiling_corral"] = 0.05
	fixture["corral_yield"] = 0.0     # below K/2 → the escapement harvest takes NOTHING
	fixture["pen_upkeep"] = 0.14      # …and it would still have to be fed
	return fixture

## The SAME penned herd, STARVING: its keeper paid only 40% of the 1.74/turn feed, so the herd is
## shrinking (`pen.starve_shrink_rate × (1 − fed) × biomass`) every turn and its yield with it. The
## drawer must say so loudly — the Corral row drops its badge for a red "⚠ Starving — 40% fed", and
## the Pen feed row names the shortfall. Biomass is down from the fed fixture's 820 to show the herd
## has actually lost ground.
func _starving_pen_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
	fixture["biomass"] = 310.0
	fixture["pen_fed_fraction"] = PEN_FED_STARVING
	return fixture

## A SELF-FEEDING pen on lush land (Grazing 2d-γ): a radius-2 fenced footprint (19 tiles) whose grazing
## covers the herd's entire feed, so `pen_pasture_fraction` 1.0 and the offset larder bill `pen_upkeep`
## is 0. The feed-split row reads "Fed by pasture 100% · larder 0.0 food/turn" and the amber Pen-feed
## debit row disappears (nothing left to haul). This is the state the Extend-pen affordance renders on —
## a built pen, no ring in flight (`pen_extend_progress` 0), so `_build_herd_assign_controls` shows the
## "Extend pen" button.
func _self_feeding_pen_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
	fixture["pen_radius"] = 2
	fixture["pen_footprint_tiles"] = 19
	fixture["pen_pasture_fraction"] = 1.0
	fixture["pen_upkeep"] = 0.0
	fixture["pen_extend_progress"] = 0.0
	return fixture

## The SAME pen mid-EXTENSION (Grazing 2d-γ): the keeper is fencing the next ring, so
## `pen_extend_progress` is 0.6 and `_build_herd_assign_controls` replaces the "Extend pen" button with
## a WARN-amber "Fencing 60%" badge. Partial pasture (60%) so the feed-split reads "60% · larder N.N".
func _extending_pen_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_pasture_fraction"] = 0.6
	fixture["pen_upkeep"] = 0.70
	fixture["pen_extend_progress"] = 0.6
	return fixture

## A base terrain legend (key == "terrain") shaped exactly like
## MapView._build_terrain_legend's output: rows carry color/label/value_text plus
## the numeric `count` the sort control keys off. Counts are deliberately varied
## and out of both name/count order so the sorting is obvious.
## MapView._build_pasture_legend's output, transcribed from the map_preview "pasture" state (it prints
## the legend dict) so the two harnesses cannot disagree. The swatch colors are read off MapView's own
## constants rather than restated, so a ramp retune moves the legend with the map.
func _pasture_legend_fixture() -> Dictionary:
	var poor: Color = MAP_VIEW_SCRIPT.PASTURE_POOR_COLOR
	var rich: Color = MAP_VIEW_SCRIPT.PASTURE_RICH_COLOR
	return {
		"key": "pasture",
		"title": "Pasture (Graze Capacity)",
		"description": "Graze capacity — the ANIMAL-edible stock (grass and browse; humans cannot digest it).\nStanding stock 100% of capacity across 346 pasture tiles.",
		"rows": [
			{"color": poor.lerp(rich, 8.0 / 240.0), "label": "Poorest pasture", "value_text": "8 graze"},
			{"color": poor.lerp(rich, 138.0 / 240.0), "label": "Average pasture", "value_text": "138 graze"},
			{"color": rich, "label": "Richest pasture", "value_text": "240 graze"},
			{"color": MAP_VIEW_SCRIPT.PASTURE_DEAD_COLOR, "label": "Barren ground", "value_text": "50 tiles"},
			{"color": MAP_VIEW_SCRIPT.PASTURE_WATER_COLOR, "label": "Water", "value_text": "72 tiles"},
		],
		"stats": {"min": 8.0, "avg": 138.0, "max": 240.0},
	}

func _forage_legend_fixture() -> Dictionary:
	# The HUMAN-food twin of the pasture legend. NOTE the differences that are the whole point: there is
	# NO water row (coastal shelves carry forage and ride the ramp), the barren row is the honest
	# "No forage" (deep ocean/glacier/lava only), and the description carries the gathering-sites
	# sub-count — the tiles actually forageable today, a subset of the potential the ramp paints.
	var poor: Color = MAP_VIEW_SCRIPT.FORAGE_POOR_COLOR
	var rich: Color = MAP_VIEW_SCRIPT.FORAGE_RICH_COLOR
	return {
		"key": "forage",
		"title": "Forage (Human Food Capacity)",
		"description": "The HUMAN-edible potential of this land — seeds, nuts, tubers, fruit, and fish.\nGathering sites: 18 tiles.",
		"rows": [
			{"color": poor.lerp(rich, 5.0 / 195.0), "label": "Poorest forage", "value_text": "5 food"},
			{"color": poor.lerp(rich, 92.0 / 195.0), "label": "Average forage", "value_text": "92 food"},
			{"color": rich, "label": "Richest forage", "value_text": "195 food"},
			{"color": MAP_VIEW_SCRIPT.FORAGE_BARREN_COLOR, "label": "No forage", "value_text": "63 tiles"},
		],
		"stats": {"min": 5.0, "avg": 92.0, "max": 195.0},
	}

func _terrain_legend_fixture() -> Dictionary:
	return {
		"key": "terrain",
		"title": "Terrain Types",
		"description": "Biomes present on this map (5).",
		"rows": [
			{"color": Color("3a6f3a"), "label": "Prairie", "value_text": "412 tiles", "count": 412},
			{"color": Color("2a4a7a"), "label": "Deep Ocean", "value_text": "980 tiles", "count": 980},
			{"color": Color("c8b26a"), "label": "Desert", "value_text": "137 tiles", "count": 137},
			{"color": Color("2f5f2f"), "label": "Mixed Woodland", "value_text": "268 tiles", "count": 268},
			{"color": Color("8a8a8a"), "label": "Alpine", "value_text": "54 tiles", "count": 54},
		],
		"stats": {},
	}
