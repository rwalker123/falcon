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
## Scratch prefs file for this harness — NEVER the player's `user://narrative.cfg`. See the
## prefs-isolation block in `_ready()` for the incident that made this non-negotiable.
const PREVIEW_PREFS_PATH := "user://ui_preview_prefs.cfg"
## Scratch DOCK prefs for the `BandCityPanel` this harness injects — NEVER the player's
## `user://band_city_dock.cfg`. A second file because the panel keeps its own (edge / collapsed / tab).
const PREVIEW_DOCK_PREFS_PATH := "user://ui_preview_dock_prefs.cfg"
# Force-compile MapView here so the harness also acts as a full-context compile
# check for it (autoloads are registered when the harness runs as a scene, which
# --check-only cannot do).
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")
## Preloaded for its STATIC `escape_claimant` alone (the ESC precedence chain, extracted so the order
## can be asserted without standing up the whole app scene) — Main is never instanced here.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
## Injected for ONE state (`tile_panel_band`) and released again: a selected player band's detail
## renders into this panel, so it is the only way to render the drawer's "it went over there"
## pointer line rather than the no-panel legacy fallback.
const BAND_CITY_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
const OUT_DIR := "res://ui_preview_out"
# The canvas EVERY frame renders at. Pinned rather than set once, because `project.godot` opens the
# window MAXIMIZED and the WM applies — and RE-applies — that asynchronously; see `_ensure_canvas`.
const PREVIEW_CANVAS_SIZE := Vector2i(1500, 900)
# How many frames `_ensure_canvas` / `_capture` keep re-asserting the pinned canvas while waiting for
# the WM to honour it. Bounded so a WM that refuses to shrink the window fails loudly, never hangs.
const CANVAS_PIN_MAX_FRAMES := 60
# How many CONSECUTIVE frames the window must hold the pinned canvas in `_stabilize_canvas` before the
# first state renders, and the bound on how long it waits for that. The maximize is applied — and
# RE-applied — asynchronously, so "it is the right size once" is not the same as "it stays".
const CANVAS_STABLE_FRAMES := 30
const CANVAS_STABLE_MAX_FRAMES := 600
# One manual step longer than any tween in the client, so `_settle`'s flush always reaches the end
# state (and fires the finished-callback) in a single `custom_step`.
const TWEEN_FLUSH_SECONDS := 3600.0
# Phase to seed the turn orb's calm breath at, as a fraction of `TurnOrb.PULSE_PERIOD`. The breath is
# `0.5 - 0.5 * cos(t)`, which is ZERO — its faintest, smallest instant — at phase 0, so freezing the
# clock there would render the pulse at the bottom of its range in all ~180 frames. A quarter period
# puts `cos` at 0, i.e. the breath's MIDPOINT, which is what an unfrozen frame averaged.
const TURN_ORB_PULSE_MIDPOINT_FRACTION := 0.25
# How far into the oral page-turn the live-arrival state drives its REAL tween before capturing, as a
# fraction of the panel's own duration — a chosen mid-motion phase in place of "however many frames
# the clock happened to give us". Must stay strictly inside (0, 1) to keep the tween RUNNING.
const TELLING_LIVE_TURN_FRACTION := 0.4
# The SECOND player band on the crowded hex (`_crowded_bands_fixture()[1]`, "Band Ash"). The Move
# assertion selects it deliberately: the faction default is the FIRST band, so a Move wired to
# anything but the list selection answers 301 instead.
const TILE_PANEL_MOVE_BAND_ENTITY := 302
# The Move button's face, in both hosts (the drawer's §18 button and the Band/City Orders block).
const MOVE_BUTTON_TEXT := "Move"
# Slice 1 reserved-dock probe: left-edge reservation width used to verify the HUD insets.
const RESERVED_PROBE_WIDTH := 300.0
# The crowded hex the sticky-land-selection state clicks, and a grid just large enough to contain it
# (the crowded fixtures all sit at 58, 24). Prairie steppe, matching that fixture's biome.
const STICKY_TILE := Vector2i(58, 24)
const STICKY_GRID_W := 64
const STICKY_GRID_H := 32
const STICKY_TERRAIN_ID := 11
# The deselect-keeps-the-tile state's two hexes, on the same grid: a hex carrying the lone herd, and
# an EMPTY land hex a few columns away (far enough that no marker or occupant can bleed into it, so a
# click there resolves as bare land).
const DESELECT_HERD_TILE := Vector2i(30, 16)
const DESELECT_LAND_TILE := Vector2i(34, 16)
# Park the OS cursor over empty canvas before rendering. The HUD drops its hovered-hex record (and
# with it the targeting banner's hunt forecast) whenever the pointer sits over an interactive HUD
# control — see Hud._suppress_tooltip_over_ui. Wherever the cursor happened to be when the harness
# launched would otherwise decide whether the hover states render, making them non-deterministic.
const MOUSE_PARK_POSITION := Vector2(750, 640)
# The armed hunt party for the pre-launch forecast states (4 workers, matching the spec's worked
# example: a 4-worker party fills in ~6 turns on a mammoth but ~54 on red deer).
const HUNT_FORECAST_PARTY := 4
# The dialed-in hunter count for the LOCAL hunt preview states — deliberately dialed PAST every
# ceiling in them, so the stepper clamps it back to the sheet's own whole-animal carry cap exactly as
# it would for the player (`LOCAL_HUNT_CAPPED_CREW`; these frames render 3 hunters, not 6). The point
# survives the clamp: even the clamped crew out-carries every policy ceiling here, so the HERD (not
# the hunters) is still the binding constraint — which is exactly the case where the per-turn yield
# preview earns its keep.
const LOCAL_HUNT_HUNTERS := 6
## What the stepper actually renders once `LOCAL_HUNT_HUNTERS` is dialed in: the sheet's own
## whole-animal carry cap ("max 3 workers useful here"), Red Deer food_per_animal 2.0 ÷ the band's
## 0.8 per-worker carry = 3 carriers to haul one body. The dial is clamped to it exactly as it is
## for the player, so this — not 6 — is what a guard on those frames can assert.
const LOCAL_HUNT_CAPPED_CREW := 3
## The crew the TWO-PRODUCT frames (issue #337) are composed with — the wolf's pelts-only pair and the
## oracle deer's food+trade control. Two hunters is the oracle's own no-waste point (food_per_animal
## 1.23 ÷ the band's 0.8 per-worker carry ⇒ 2 carriers haul one whole body), so the frame the trade
## components are read on carries no waste term to argue with; the wolf rides the same crew so the
## inedible quarry and the both-products control are compared at ONE party size.
const PELT_FRAME_HUNTERS := 2
## The two INVESTMENT-rung payoff faces the Wild Boar frame is judged on (issue #397), spelled out as
## literal strings rather than rebuilt from `SourceForecast.picker_products` — an assertion that
## re-derives the face through the very formatter under test asserts nothing. `→` is the payoff arrow
## (`HudComposeVocab.POLICY_PAYOFF_COMPACT`), food leads, and each half appears because the boar pays
## both; the pre-fix face was the food clause alone.
const BOAR_TAME_PAYOFF_FACE := "→ 1.48 food · 0.37 trade"
const BOAR_CORRAL_PAYOFF_FACE := "→ 2.95 food · 0.74 trade"
## Which line of a rung's two-line face carries the metric: line 0 is the rung NAME
## (`HudFormat.policy_face`), line 1 the products (`HudWidgets._policy_rung_cell` builds them in that
## order). A rung with no metric wears line 0 alone.
const POLICY_RUNG_METRIC_LINE := 1
## "no count dialed in" for `_compose_herd` — a real dial can be 0 (an unstaffed compose), so the
## sentinel has to sit outside the valid range rather than reuse 0.
const COMPOSE_COUNT_UNSET := -1
# The crowded hex's staffed-wildlife-row state: the SAME herd worked both ways at once. Two distinct
# counts so the row's meta can only read right if it SUMS them (4 + 6 = `10 🏹`) — a single shared
# number would pass even if one source were dropped.
const OCCUPANTS_HUNT_LOCAL_WORKERS := 4
const OCCUPANTS_HUNT_PARTY_WORKERS := 6
# The sim's forward-SIMULATED turns-to-fill for the 4-worker party in these states (it exports the
# answer; the client never divides). Sustain is a small renewable flow → slow; Surplus/Deplete strip the
# herd's stock headroom first → fast. The deer's Sustain trip (54) blows past the 20-turn viability
# threshold; its Surplus trip (6) does not — same herd, same party, opposite verdicts.
const MAMMOTH_SUSTAIN_TRIP_TURNS := 6
const DEER_SUSTAIN_TRIP_TURNS := 54
const DEER_SURPLUS_TRIP_TURNS := 6
const MAMMOTH_SURPLUS_TRIP_TURNS := 3
# The whole animals the 4-worker RAID delivers (HuntTripEstimate.animalsTaken) — the payload the readout
# headlines. A viable/slow raid lands a positive count; a herd at/below its policy floor lands 0 (the
# no-surplus state). Surplus/Deplete raid deeper than Sustain, so a deeper policy lands MORE animals.
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
# One animal's worth of TRADE GOODS on an EDIBLE quarry (issue #337) — a hunt pays a vector, so every
# raid cell carries a trade payload beside its food one and the readout names both. Deliberately much
# smaller than the food quantum: a deer/boar is meat first, hide second (the INEDIBLE case, where trade
# is the whole payload, is the wolf fixture below).
const RAID_TRADE_PER_ANIMAL := 0.5
# 0 = the raid ran the whole forecast horizon still delivering (a long raid), used by the no-surplus /
# collapsed fixtures where the raid also lands 0 animals.
const NEVER_FILLS_TRIP_TURNS := 0
# The Telling fixture's two authored voice registers. Named here ONLY so the harness can pin the
# preference deterministically — nothing in the client hardcodes a register (VoiceLine.register is
# free-form by design; the panel builds its toggle from what the fork actually carries).
const FORK_REGISTER_MYTHIC := "mythic"
const FORK_REGISTER_WARM := "warm"
# The Telling panel's medium rungs. Named here only so the states read; the client keys its styling
# off a table with an `oral` fallback, never off these three being exhaustive.
const TELLING_MEDIUM_ORAL := "oral"
const TELLING_MEDIUM_PAINTED := "painted"
const TELLING_MEDIUM_WRITTEN := "written"
# The pen-keeping band's entity id — its own, so its Food disclosure key (`food:<entity>`) doesn't
# collide with the reference band's.
const PEN_KEEPER_BAND_ENTITY := 906
# The reference band (`_band_fixture()`, entity 904) disclosure keys — the `[url]` meta its Food /
# Morale rows carry, i.e. what `DetailFormat.breakdown_key` builds for it.
const BAND_DISCLOSURE_FOOD := "food:904"
const BAND_DISCLOSURE_MORALE := "morale:904"
const BAND_DISCLOSURE_GROWTH := "growth:904"
# The collapsed-growth band is `_concerning_food_band_fixture`'s entity (905), not 904.
const BAND_DISCLOSURE_GROWTH_COLLAPSED := "growth:905"
# The Red Deer pen at its settled escapement point (design doc §7, MEASURED from a sim run): the
# feed the herd demands per turn, and the share of it a broke keeper managed to pay in the starving
# state. `pen_fed_fraction` < 1 ⇒ the herd is shrinking.
const PEN_UPKEEP_RED_DEER := 1.74
const PEN_FED_STARVING := 0.40
# The herder-deficit state's staffing PAIR (`herd_corral_under_herded`). The growing corral needs 2
# herders every turn to hold its tameness while only 1 is staffed, and the two numbers must DISAGREE
# for the deficit to render at all — so the fixture's `herders_needed`, the band's dialed-down hunt
# assignment and the auto-max assertion all read them from here rather than repeating bare 1s and 2s.
const UNDER_HERDED_CORRAL_HERDERS_NEEDED := 2
const UNDER_HERDED_CORRAL_HERDERS_STAFFED := 1
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
	# FREEZE ANIMATION TIME — the same treatment `map_preview` and `blend_probe` carry, and taken for
	# the same reason: a frame that varies run-to-run cannot be pixel-diffed to prove a HUD refactor
	# changed nothing. Measured before the freeze, two runs of IDENTICAL code differed byte-wise in
	# 184 of 184 frames (the turn orb's calm breath animates in every frame there is).
	#
	# What survives phase 0 was CHECKED against the draw code, not assumed:
	#   • the awaiting-expedition / targeting pulses (`0.5 + 0.5 * sin(t)` and `base + amp * sin(t)`)
	#     are MapView's, and this harness's MapView is `visible = false` — data only;
	#   • the turn orb's breath is `0.5 - 0.5 * cos(t)`, which DEGENERATES to its minimum at phase 0,
	#     so its phase is seeded to the midpoint below rather than left at 0;
	#   • the ONE tween in the whole client (TellingPanel's page turn) never advances at time_scale 0,
	#     so `_settle` drives live tweens to their END state — see `_flush_tweens`.
	# `_settle` waits on `process_frame`, which still fires at time_scale 0.
	Engine.time_scale = 0.0
	_pin_canvas(get_window())
	DirAccess.make_dir_absolute(OUT_DIR)

	# A mid-tone terrain-ish backdrop so the translucent card reads correctly.
	var bg_layer := CanvasLayer.new()
	bg_layer.layer = -10
	add_child(bg_layer)
	var bg := ColorRect.new()
	bg.color = Color(0.10, 0.15, 0.16)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg_layer.add_child(bg)

	# ---- prefs isolation — FIRST, before anything can read OR write a preference ----------------
	# THE HARNESS MUST NEVER TOUCH THE PLAYER'S PROFILE. It once did, and it cost a real debugging
	# session: a state called the persisting `toggle_victory()` while the legend was open for a
	# frame, `_save_hud_panel_prefs` wrote BOTH keys, and `legend_suppressed=false` landed in the
	# developer's `user://narrative.cfg` — so their next real game came up with the Terrain Types
	# panel visible and the shipped default looked broken. Redirect every read/write to a scratch
	# file and DELETE it, which is both the isolation and a genuine fresh profile.
	NarrativeForkPanel.config_path_override = PREVIEW_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_PREFS_PATH))
	# THE SECOND prefs file, for the same two reasons `band_panel_preview` isolates it. `tile_panel_band`
	# injects a real `BandCityPanel`, which READS its dock/collapse/TAB prefs on construction and WRITES
	# them back when the harness docks it — so without this the harness was editing the developer's
	# `user://band_city_dock.cfg` (found holding this harness's `edge=2` / `tab="band"`), AND the frame it
	# renders depended on whatever tab the previous run left behind: the panel's zone was measured
	# rendering `work` in one run and `band` in the next, off nothing but that leftover file.
	BandCityPanel.config_path_override = PREVIEW_DOCK_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_DOCK_PREFS_PATH))
	# The Telling panel restores its collapsed state in its constructor, so pin it expanded BEFORE
	# the HUD instantiates (into the scratch file, now that the override is set).
	TellingPanel.save_collapsed(false)

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)
	await get_tree().process_frame
	await get_tree().process_frame
	# Hold the canvas until the WM stops fighting it — before the first state, so no LATER settle has
	# to spend a frame on it. See `_stabilize_canvas`.
	await _stabilize_canvas()
	Input.warp_mouse(MOUSE_PARK_POSITION)

	# Seed the turn orb's calm breath at its MIDPOINT. `_pulse_time` only ever advances by `delta`,
	# which is 0 with the clock frozen, so whatever is set here is the phase every frame renders at —
	# and phase 0 is the breath's trough (alpha 0.30 / radius 44 of a 0.30..0.85 / 44..47 range), i.e.
	# a deterministic frame whose subject has faded to its faintest. Set once; nothing resets it.
	_hud.turn_orb._pulse_time = TurnOrb.PULSE_PERIOD * TURN_ORB_PULSE_MIDPOINT_FRACTION

	# The Tile-card Climate band is driven by the sim's PUBLISHED cut points, which the live
	# client adopts from the snapshot's overlays (MapSection.climateBands) via MapView. This
	# harness has no MapView, so seed TileClimate with the shipped values (polar ≤0 / boreal ≤3
	# / temperate ≤18 °C) exactly as a first snapshot would — otherwise every tile card would
	# skip the Climate row (has_bands() == false, the honest pre-publish blank).
	TileClimate.set_cut_points(0.0, 3.0, 18.0)

	# Top-bar Sedentarization meter (faction 0, soft band) — visible across all frames.
	_hud.update_sedentarization([{"faction": 0, "score": 62.0, "stage": "soft"}])

	# Top-bar demographics readout (faction 0 age structure + dependency ratio).
	_hud.update_demographics([{"faction": 0, "children": 34, "working": 51, "elders": 15}])

	# Top-bar intensification-knowledge meters (faction 0): Cultivation still learning
	# (block-glyph bar + "learning"), Herding fully mastered ("✔ known"). Visible across frames.
	_hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])

	# Top-bar Wondrous-Sites discoveries readout (faction 0). The strip keys on `site_id`, so this
	# fixture is built to prove the two cases the glyph could not distinguish:
	#   • GLYPH COLLISION — `great_peak` and `sky_arch` both ship ⛰, and must stay TWO entries:
	#     great_peak's bundled sprite, then sky_arch's ⛰ emoji (it has no art).
	#   • REPEAT INSTANCE — the second `great_peak` is a different tile, so it lifts the count to 4
	#     while adding no strip entry: the number counts instances, the strip counts kinds.
	# `verdant_basin` is the other bundled sprite. Reads `◈ Discoveries 4` + 3 marks.
	_hud.update_discoveries([{
		"faction": 0,
		"sites": [
			{"x": 12, "y": 8, "site_id": "great_peak", "category": "landmark", "display_name": "Great Peak", "glyph": "⛰"},
			{"x": 20, "y": 14, "site_id": "verdant_basin", "category": "settle_site", "display_name": "Verdant Basin", "glyph": "⛲"},
			{"x": 26, "y": 9, "site_id": "sky_arch", "category": "landmark", "display_name": "Sky Arch", "glyph": "⛰"},
			{"x": 31, "y": 17, "site_id": "great_peak", "category": "landmark", "display_name": "Great Peak", "glyph": "⛰"},
		],
	}])

	# The labor-allocation UI (Early-Game Labor slice 3b) targets the single player band;
	# seed it so the herd/tile "assign" controls resolve a band to staff.
	_hud._band_labor._player_band = _band_fixture()
	# The world's herds (Main pushes snapshot["herds"]): the Current-actions Hunt row reads the herd's
	# species from here and, when clicked, jumps to its LIVE tile (it has migrated away from the hunt
	# assignment's launch target).
	_hud.update_herds(_world_herds_fixture())
	# The world's food modules (Main pushes snapshot["food_modules"]): each Forage row leads with the
	# module's map glyph, so the panel row and the map marker read as the same resource.
	_hud.update_food_modules([
		{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"},
	])

	# State 0-fresh-profile — THE SHIPPED DEFAULT DOCK LAYOUT, rendered on the path a real player
	# travels and nothing else: prefs section erased above, HUD freshly instantiated, and the first
	# real terrain legend arriving from MapView exactly as `Main._on_overlay_legend_changed` pushes
	# it. NOTHING may call `set_suppressed` / `toggle_legend` / `toggle_victory` before this point —
	# that is the whole value of the state. The right dock must be EMPTY of both reference cards:
	# no Terrain Types, no Victory. This state is FIRST on purpose, so no later state can leak into
	# it, and it is the regression guard for "the legend is visible by default in the real game".
	_hud.update_overlay_legend(_terrain_legend_fixture())
	_hud.update_victory_state(_victory_state_fixture())
	await _settle()
	await _save("dock_fresh_profile_default")
	_assert_hud("fresh profile: Terrain Types legend is hidden",
		not _hud.terrain_legend_panel.visible)
	_assert_hud("fresh profile: Victory panel is hidden",
		not _hud.victory_panel.visible)

	# State 1 — a single band selected (GOOD state): the Occupants roster + the labor allocation panel.
	# Food + Morale are healthy, so BOTH summary rows read collapsed with a ▸ disclosure caret
	# (`Food ▸ …` / `Morale 82% ▸`) — click-to-expand, nothing auto-shown.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band")

	# State 1-foreign — a NON-player band selected. The drawer is the same `unit_summary_lines` host,
	# but almost none of it applies: morale/output/breakdowns are player-only (someone else's band is
	# not ours to read), there is no allocation panel, and the identity rows (name, size) now live in
	# the roster row above. So the check this state exists for: does the drawer collapse to an empty
	# card once `Unit`/`Size` are gone? (It keeps the bare larder Food line + Position.)
	_hud.show_unit_selection(_foreign_band_fixture())
	await _settle()
	await _save("band_foreign")

	# State 1-forage-policy — the forage allocation row carries a policy tag like Hunt does. This band
	# forages on Deplete policy, which the sim gathers past the patch's regrowth: the sim-answered
	# `overdraws` flag is true, so the row reads `Forage (71, 18) [deplete] +0.62 /turn ⚠` (amber
	# over-forage flag). The default `band` state above shows the [sustain] tag with overdraws=false.
	var forage_policy_band := _band_fixture()
	forage_policy_band["labor_assignments"] = [
		{"kind": "forage", "workers": 6, "target_x": 71, "target_y": 18, "policy": "deplete", "actual_yield": 0.62, "sustainable_yield": 0.40, "overdraws": true},
		{"kind": "scout", "workers": 2},
	]
	_hud.show_unit_selection(forage_policy_band)
	await _settle()
	await _save("forage_policy")

	# State 1-food-a — GOOD food, breakdown OPEN. The breakdown renders in a POPOVER, never inline
	# (growing the row in place is what clipped the Band panel's fixed-height band zone), so the frame
	# shows the indented `Gathered · Hunted · Eaten` rows in a small card under the row. Driven through
	# the REAL path — `meta_clicked` on the live drawer label, the exact signal a click emits.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	await _settle()
	await _save("band_food_expanded")
	_click_disclosure(BAND_DISCLOSURE_FOOD)

	# State 1-morale-a — GOOD morale, breakdown OPEN (same disclosure, same popover): the morale
	# contribution rows.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_MORALE)
	await _settle()
	await _save("band_morale_expanded")
	_click_disclosure(BAND_DISCLOSURE_MORALE)

	# State 1-growth-a — GOOD growth, breakdown OPEN. The band out-breeds its base rate (188% of
	# normal), so the row reads neutral ink and its disclosure names what is HELPING: `▲ ×1.50 larder
	# reserve` / `▲ ×1.25 larder growing`. `hunger` is neutral (the band ate) so its row is omitted
	# rather than listed as a no-op — and the multipliers read down to the headline: 1.50 × 1.25.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_GROWTH)
	await _settle()
	await _save("band_growth_expanded")
	_click_disclosure(BAND_DISCLOSURE_GROWTH)

	# State 1-growth-b — COLLAPSED growth on the concerning band (23% of normal → red row, WARN
	# caret), breakdown OPEN. All three factors are off neutral, so this is the frame that proves the
	# rows multiply out to the headline: 0.60 × 1.50 × 0.25 = 0.23. It is the whole point of the
	# export — the player already had the larder and the Food line, not the attribution.
	_hud.show_unit_selection(_collapsed_growth_band_fixture())
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_GROWTH_COLLAPSED)
	await _settle()
	await _save("band_growth_collapsed")
	_click_disclosure(BAND_DISCLOSURE_GROWTH_COLLAPSED)

	# State 1-growth-c — a REHYDRATED band: the sim publishes no fertility reading (the factors are
	# derived, not persisted), so there is NO Growth row and no caret at all. The regression this
	# guards is the tempting one — defaulting the factors to 0 and rendering "Growth: 0% of normal",
	# i.e. reading missing data as a total collapse of births.
	_hud.show_unit_selection(_unprojected_growth_band_fixture())
	await _settle()
	await _save("band_growth_unprojected")

	# State 1-food-b — CONCERNING food (net negative + low runway): the Food line net reads red and
	# its caret wears WARN rather than SIGNAL — the breakdown no longer opens itself (a popover that
	# popped on a snapshot would be worse than the clipping it replaced), so the invitation to read it
	# has to be visible on the row.
	_hud.show_unit_selection(_concerning_food_band_fixture())
	await _settle()
	await _save("band_food_concerning")

	# State 1-food-c — a band KEEPING A PEN (docs/plan_corral_managed_population.md). Its ledger has
	# THREE terms, not two: the corral grosses 5.40, the people eat 1.15, and the penned animals eat
	# 1.74 off the same larder (`pen_feed_upkeep`, the sim's own figure — the client never sums the
	# herds' upkeep itself). Net = 5.88 − 1.15 − 1.74 = +2.99, NOT the +4.73 the old two-term ledger
	# would have advertised. Breakdown popover open to show all four rows at once.
	_hud.show_unit_selection(_pen_keeper_band_fixture())
	await _settle()
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)
	await _settle()
	await _save("band_pen_feed")
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)

	# State 1-food-d — the same pen, STARVING: the band could pay only 0.70 of the 1.74 the herd
	# demands, so the pen feed row shrinks to what was actually paid while the herd wastes away (the
	# herd drawer carries the alarm — see `herd_corral_starving`). Income has fallen with the herd,
	# and the net has gone red.
	_hud.show_unit_selection(_starving_pen_band_fixture())
	await _settle()
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)
	await _settle()
	await _save("band_pen_starving")
	_click_disclosure("food:%d" % PEN_KEEPER_BAND_ENTITY)

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
	_hud._band_labor._pending_labor = {
		904: {
			"turn": 0,
			"assign": {"forage:64,20": {"kind": "forage", "workers": 6, "x": 64, "y": 20, "herd_id": "", "policy": ""}},
		}
	}
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band_pending")
	_hud._band_labor._pending_labor = {}

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
	_hud._bandpanel._send_expedition_count = 8
	_hud.show_unit_selection(cap_band)
	await _settle()
	await _save("expedition_outfit_cap")
	_hud._bandpanel._send_expedition_count = 1   # reset so later states render a fresh party stepper

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
	full_hunt["turns_of_food"] = 8.0
	_hud.show_unit_selection(full_hunt)
	await _settle()
	await _save("expedition_hunt_full")

	# State 1j — a recalled hunt party in its Returning phase: the Phase reads "Returning" and the
	# panel's Recall button flips to a disabled "Returning" (same treatment as the scout panel).
	var returning_hunt := _hunt_expedition_fixture()
	returning_hunt["expedition_phase"] = "returning"
	returning_hunt["stores"] = {"provisions": 12.0}
	returning_hunt["turns_of_food"] = 6.0
	_hud.show_unit_selection(returning_hunt)
	await _settle()
	await _save("expedition_hunt_returning")

	# State 1j2 — a DEPLETE hunt party in flight: Deplete relaunches for repeated trips, so its
	# "Next delivery" line wears the recurring ↻ marker. That ↻ must read distinct from the Deplete
	# policy glyph (⇊) elsewhere in the panel — the whole point of the marker choice.
	var deplete_hunt := _hunt_expedition_fixture()
	deplete_hunt["expedition_hunt_policy"] = "deplete"
	deplete_hunt["expedition_eta_turns"] = 9
	deplete_hunt["expedition_projected_delivery"] = 16.0
	deplete_hunt["expedition_recurring"] = true
	_hud.show_unit_selection(deplete_hunt)
	await _settle()
	await _save("expedition_hunt_recurring")

	# State 1k — the hunt launch policy picker: an idle band (short allocation panel) showing the
	# "Send expedition" outfit block — the party stepper, the scout + hunt send buttons, and the hunt
	# POLICY radio (DEPLETE selected) with its EXPEDITION hint. The expedition hints must never promise
	# HUSBANDRY — the Hunting arm accrues none, though since #337 it does bank the trade half of the
	# kill — so Deplete's line frames the rung by the PRESSURE it applies (relaunching trip after trip)
	# rather than by a craft the party cannot teach. The outfit block sits below the left dock's fold,
	# so scroll to see the hint.
	var launch_band := _band_fixture()
	launch_band["idle_workers"] = 12
	launch_band["labor_assignments"] = []
	var left_scroll: ScrollContainer = _hud.left_stack.get_parent() as ScrollContainer
	_hud._bandpanel._send_hunt_policy = "deplete"
	_hud.show_unit_selection(launch_band)
	await _settle()
	left_scroll.scroll_vertical = int(left_scroll.get_v_scroll_bar().max_value)
	await _settle()
	await _save("expedition_launch_policy")
	left_scroll.scroll_vertical = 0

	# State 1k-sustain — the SUSTAIN launch hint, which had to be rewritten when Sustain became the
	# maximum-sustainable-yield FLOW (it used to promise "one conservative harvest", a model that no
	# longer exists). It also must NOT mention domestication: only a RESIDENT band's Sustain hunt
	# builds husbandry — an expedition accrues none (the one payoff half still missing from a raid,
	# now that #337 banks its trade goods).
	_hud._bandpanel._send_hunt_policy = "sustain"
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

	# State 1d — Predators Phase 3 threat/casualty feed alerts (CommandFeedController.KIND_STYLE).
	# `predator_raid` renders a ⚔ + crimson-tinted label, `hunt_danger` a ⚠ + amber label, and a plain
	# `site_discovered` row rides alongside for contrast — the styled kinds must read as ALERTS distinct
	# from the routine receipt.
	_hud.ingest_command_events([
		{"tick": 51, "kind": "predator_raid", "label": "Wolves raided Band 2", "detail": "Lost 1.2 provisions to the pack."},
		{"tick": 51, "kind": "hunt_danger", "label": "Mammoth gored a hunter", "detail": "The hunt turned costly."},
		{"tick": 51, "kind": "site_discovered", "label": "Discovered Verdant Basin", "detail": "A routine receipt, for contrast."},
	])
	_hud.clear_selection()
	_hud.toggle_command_feed()   # hidden by default (opens on `R`) — reveal it to judge the styled rows
	await _settle()
	await _save("predator_feed")
	_hud.toggle_command_feed()   # hide again so it does not leak into later states

	# State 1e — Predators Phase 3 band readout: the Warrior-card "⚠ Predator nearby — N on guard"
	# crimson alert AND the "⚔ Lost to raids −1.20" ledger row, both lit at once. A threatening predator
	# is placed within raid range in the world-herd list so the client-derived proximity check fires; the
	# food breakdown popover is opened to show the forfeit row. The shared herd list is restored after.
	_hud.update_herds([_raiding_predator_herd_fixture()])
	var raided_band := _raided_band_fixture()
	_hud._band_labor._player_band = raided_band
	_hud.show_unit_selection(raided_band)
	await _settle()
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	await _settle()
	await _save("predator_band_raided")
	_click_disclosure(BAND_DISCLOSURE_FOOD)
	_hud.update_herds(_world_herds_fixture())   # restore the shared world-herd list

	# band_alerts (above) left _player_band as an alert-fixture band (no work_range, far from the food
	# tile); seed a NEAR band so the forage controls resolve an in-range actor.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)

	# State 2 — a food tile selected, band WITHIN forage range: the Tile card's "Assign foragers"
	# controls (a "Band:" dropdown naming the actor band + a Foragers −/+ count + an enabled **Forage**
	# button). With one player band the dropdown is a single item ("Band 1").
	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("food_tile")

	# State 2-crop — the SAME tile once a band has committed it under Cultivate/Sow (flora roster S1).
	# The "What grows here" basket row is REPLACED by a single `Crop: Wild Emmer` row: committing
	# displaces the rest of the basket, so the tile is one plant now and stating both would name
	# plants that no longer grow here. Compare side by side with `food_tile.png`, which is the
	# uncommitted basket (and the 101%-rounding test) — the two frames differ in exactly that row.
	_hud.show_tile_selection(_committed_crop_tile_fixture())
	_compose_forage(_committed_crop_tile_fixture())
	await _settle()
	await _save("food_tile_crop")

	# State 2-growing — the "What grows here" SECTION on the bare tile card (no compose sheet): a header
	# then one 🌿 row per realized plant, name + share%, in wire order (share DESC). The pair is TWO
	# "Alluvial Plain" tiles with DIFFERENT realized baskets (Wild Emmer 70% + Flax 30% vs Cotton 55% +
	# Flax 45%), so read side by side they are the visible proof that same-biome tiles no longer carry a
	# uniform per-biome roster — the per-tile realization the compose picker already shows, now on the
	# card a player gets by just inspecting a tile. Compose source reset so only the card renders.
	_hud._compose.reset_forage_source()
	_hud.show_tile_selection(_cash_basket_tile_fixture())
	await _settle()
	await _save("tile_growing_here")
	_hud.show_tile_selection(_cash_variant_basket_tile_fixture())
	await _settle()
	await _save("tile_growing_here_variant")

	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()

	# State 2-forecast — the same food tile with the Foragers stepper parked AT the forecast cap
	# (3 = the Sustain ceiling's max-useful workers, below the band's 10 idle): the `+` button is
	# DISABLED, the "max 3 workers useful here — more would be idle" note explains why, and the
	# "Expected yield" row reads the ceiling itself (+0.96 /turn = min(3 × 0.32, 0.96)).
	_hud._compose.set_forage_count(3)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_forecast_cap")

	# State 2-labor — the SAME food tile, but the actor band has only 2 idle workers, BELOW Sustain's
	# max-useful of 3: the Foragers stepper caps at 2 (LABOR, not usefulness) and the note names the
	# reason — "2 of 3 useful — free up idle workers to send more" — so a `+` gone dead at idle reads as
	# fixable by reassigning labor, not as a silent bug. The usefulness ceiling (3) is unchanged; only
	# the note differs from the usefulness-bound `forage_forecast_cap` above.
	var forage_labor_band: Dictionary = _forage_range_bands()[0].duplicate(true)
	forage_labor_band["idle_workers"] = 2
	_hud._band_labor._player_band = forage_labor_band
	_hud._compose.set_forage_band(-1)
	_hud._compose.set_forage_count(2)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_labor_bound")
	# Restore the 10-idle range band + count for the states that follow.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._compose.set_forage_band(-1)
	_hud._compose.set_forage_count(3)

	# State 2-tended — a fully-cultivated forage patch: the Tile card's cultivation row reads
	# "🌾 Tended Patch" (SIGNAL tint) with an "Ecology: Thriving" row above it. A tended
	# patch's ceilings all equal its per-worker yield, so the forecast caps the stepper at 1 worker.
	_hud.show_tile_selection(_tended_tile_fixture())
	_compose_forage(_tended_tile_fixture())
	await _settle()
	await _save("tended_tile")

	# State 2-stressed — an over-drawn (uncultivated) forage patch: the Ecology row reads a WARN-amber
	# "⚠ Stressed" right under "Forage biomass", exactly like a stressed herd's Ecology row. Proves the
	# row is NOT gated on cultivation.
	_hud._compose.set_forage_count(1)
	_hud.show_tile_selection(_stressed_tile_fixture())
	await _settle()
	await _save("food_tile_stressed")

	# ---- Climate band: rendered off the sim's PUBLISHED cut points (Climate Authority) -----------
	# The Climate row is classified by the sim's cut points (polar ≤0 / boreal ≤3 / temperate ≤18 °C),
	# NOT a client threshold. Drive the same tile card at four temperatures spanning the ladder and
	# confirm the label tracks the sim's inclusive-upper-bound bands. A cold highland reads Polar/Boreal,
	# a warm lowland reads Temperate/Tropical — and "Polar" now appears ONLY where the sim says so, which
	# is the whole point of retiring the client's own cool_min.
	_hud.show_tile_selection(_climate_tile_fixture(-6.0, "Frost Highland"))
	await _settle()
	await _save("climate_polar")
	_hud.show_tile_selection(_climate_tile_fixture(2.0, "Boreal Upland"))
	await _settle()
	await _save("climate_boreal")
	_hud.show_tile_selection(_climate_tile_fixture(12.0, "Temperate Vale"))
	await _settle()
	await _save("climate_temperate")
	_hud.show_tile_selection(_climate_tile_fixture(27.0, "Tropical Lowland"))
	await _settle()
	await _save("climate_tropical")

	# ---- Pasture: the ANIMAL-edible stock on the tile card (Grazing Phase 2a) --------------------
	# State 2-pasture-stressed — the graze drawn down into the stressed band: "Pasture 61 / 240" with a
	# WARN-amber "⚠ Stressed" under it, identical in label and tint to a stressed herd or patch. (The
	# healthy pair — "Forage biomass 84 / 120" beside "Pasture 240 / 240 · Thriving" — is on `food_tile`.)
	_hud._compose.set_forage_count(1)
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
	# The legend card ships SUPPRESSED (the player opens it with `L`), so every legend state opens it
	# and CLOSES IT AGAIN around its own frames — see `_open_legend` / `_close_legend`.
	_open_legend()
	_hud.update_overlay_legend(_pasture_legend_fixture())
	await _settle()
	await _save("pasture_legend")
	_close_legend()
	_hud.clear_selection()

	# State 2-forage-legend — the map legend for the `forage` overlay channel (rows produced by
	# MapView._build_forage_legend; see map_preview's "forage" state for the map). The twin of the
	# pasture legend, but honest about the OPPOSITE meaning of absence: NO water row (shelves carry
	# forage and ride the ramp), a single "No forage" barren row (deep ocean/glacier/lava only), and a
	# "Gathering sites: N" sub-count so the ramp reads as POTENTIAL without calling the rest dead.
	_open_legend()
	_hud.update_overlay_legend(_forage_legend_fixture())
	await _settle()
	await _save("forage_legend")
	_close_legend()
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
	_hud._compose.set_forage_count(1)
	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
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
	_hud._compose.set_forage_policy("cultivate")
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_cultivate")

	# State 2-crop-picker — THE CROP PICKER (flora roster S1), on the longest basket the sim produces
	# (5 named plants). Under 🌱 Cultivate the selection must land on the HIGHEST-SHARE LEGAL row —
	# Wild Emmer 34%, which is also the sim's own default — while River Fish and Oak Mast stay VISIBLE
	# and greyed (they climb no rung), and Ground Nut 14% stays fully pressable: a small share is a bad
	# choice, not an illegal one. Judge legibility + fit here, not on the 3-entry reference tile.
	_hud.show_tile_selection(_long_basket_tile_fixture())
	_hud._compose.set_forage_policy("cultivate")
	_hud._compose.set_forage_species("")
	_compose_forage(_long_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker")

	# THE COMMIT BUTTON MUST STAY ON SCREEN. A picker that pushes `Forage` below the sheet's fold is
	# worse than the problem the picker solved, so the picker's list scrolls WITHIN itself
	# (FLORA_CROP_LIST_MAX_HEIGHT) rather than growing the sheet. Asserted on the LONGEST basket, the
	# only case that can trip it: the sheet's own ScrollContainer must have nothing left to scroll.
	var sheet_scroll: ScrollContainer = _hud._drawercompose._compose_sheet._scroll
	var sheet_overflow: float = sheet_scroll.get_v_scroll_bar().max_value - sheet_scroll.size.y
	print("ui_preview: compose sheet overflow = %.1f (card %.1f)" % [
		sheet_overflow, _hud._drawercompose._compose_sheet._card.size.y])
	_assert_hud("a 5-plant crop picker leaves the Forage button on screen (sheet does not scroll)",
		sheet_overflow <= 1.0)
	# The height that bought those rows came from collapsing the OTHER rung's reasons — but ONLY while a
	# COMMITTING rung is composed. Assert both halves here: Sow (not composed) is one short locked line,
	# and its prerequisites are no longer spelled out on the card.
	_assert_hud("composing Cultivate collapses the locked Sow rung to one line",
		_has_label_containing(_hud._drawercompose._compose_sheet, "locked ("))
	_assert_hud("…and that rung's full prerequisites leave the card",
		not _has_label_containing(_hud._drawercompose._compose_sheet, "Seed Selection"))

	# State 2-crop-then-a / -b — THE PICKER ACTUALLY MOVES THE FORECAST. The "→ then" term used to quote
	# a species-BLIND patch number, so committing to Ground Nut showed Wild Emmer's payoff and the picker
	# appeared to change nothing above it. These two frames are the SAME tile with a DIFFERENT crop
	# selected; the assertion is that the forecast line differs between them, which is the only thing
	# that proves the substitution is wired to the selection rather than rendered once.
	_hud._compose.set_forage_count(1)
	_hud._compose.set_forage_species("wild_emmer")
	# The ARROW, not the bare word: the Cultivate policy hint on the same card also says "then".
	_compose_forage(_long_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_then_emmer")
	var then_emmer := _label_text_containing(_hud._drawercompose._compose_sheet, FORECAST_THEN_NEEDLE)

	_hud._compose.set_forage_species("ground_nut")
	_compose_forage(_long_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_then_groundnut")
	var then_groundnut := _label_text_containing(_hud._drawercompose._compose_sheet, FORECAST_THEN_NEEDLE)
	print("ui_preview: then-term  emmer=%s  ground_nut=%s" % [then_emmer, then_groundnut])
	_assert_hud("the forecast's 'then' payoff tracks the SELECTED crop",
		then_emmer != "" and then_groundnut != "" and then_emmer != then_groundnut)
	_hud._compose.set_forage_species("")

	# State 2-crop-marginal — the ALL-MARGINAL tile (RollingHills' real ratios). Every legal crop is
	# below 1.0×, so the whole list is warn-inked and the hint says why — and every row stays PRESSABLE.
	# The ratio is here to stop a bad idea being invisible, never to forbid it.
	_hud.show_tile_selection(_marginal_basket_tile_fixture())
	_hud._compose.set_forage_policy("cultivate")
	_hud._compose.set_forage_species("")
	_compose_forage(_marginal_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_marginal")

	# State 2-crop-overlong — THE SCROLL'S OWN FRAME. A SYNTHETIC 8-plant basket, longer than any tile
	# the sim can produce, so the picker's internal list actually scrolls: the visible-row cap is set so
	# every SHIPPED basket fits whole, which would otherwise leave this path rendered by nothing. The
	# `Forage` button must still be on screen — that is what the cap protects, at any basket length.
	_hud.show_tile_selection(_overlong_basket_tile_fixture())
	_hud._compose.set_forage_policy("cultivate")
	_hud._compose.set_forage_species("")
	_compose_forage(_overlong_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_overlong")
	var overlong_scroll: ScrollContainer = _hud._drawercompose._compose_sheet._scroll
	print("ui_preview: overlong-basket sheet overflow = %.1f (card %.1f)" % [
		overlong_scroll.get_v_scroll_bar().max_value - overlong_scroll.size.y,
		_hud._drawercompose._compose_sheet._card.size.y])
	_assert_hud("an 8-plant crop picker still leaves the Forage button on screen",
		overlong_scroll.get_v_scroll_bar().max_value - overlong_scroll.size.y <= 1.0)

	# ---- THE TWO ZERO-WORKER SUBMITS (playtest defect) -------------------------------------------
	# `workers == 0` means two different things depending on whether this band already works the tile,
	# and the button + the forecast line have to agree in BOTH. These frames are judged as a PAIR.
	#
	# State 2-unstaffed (A) — 0 foragers on a tile this band does NOT work. Pressing Forage would send a
	# command that changes nothing, so the button is DISABLED and still reads `Forage`; the forecast
	# drops the "Preparing: +0.00 → then +1.20" promise (an unstaffed build meter never advances, so
	# that sequence cannot arrive) and states the payoff as a condition instead. The payoff NUMBER stays
	# — it is how the player decides the tile is worth staffing at all.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._compose.reset_forage_source()
	_hud.show_tile_selection(_food_tile_fixture())
	# The FIRST compose settles the source key; the policy and count must be set after it, because a
	# source change re-seeds both from the band's standing assignment and would overwrite them.
	_compose_forage(_food_tile_fixture())
	_hud._compose.set_forage_policy("cultivate")
	_hud._compose.set_forage_count(0)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_unstaffed")
	var unstaffed_btn := _find_button_by_text(_hud._drawercompose._compose_sheet, "Forage")
	_assert_hud("0 workers on an unassigned tile disables the submit (it would be a no-op)",
		unstaffed_btn != null and unstaffed_btn.disabled)
	_assert_hud("…and the forecast drops the 'then' promise it cannot keep",
		not _has_label_containing(_hud._drawercompose._compose_sheet, "Preparing:"))
	_assert_hud("…while still showing what the tile would pay once prepared",
		_has_label_containing(_hud._drawercompose._compose_sheet, UNSTAFFED_COPY_NEEDLE))

	# State 2-unassign (B) — the SAME 0 workers on a tile this band DOES work: that is the sim's
	# unassign, not a no-op. The button stays live and is RENAMED, and the "assign to begin" line is
	# gone — it would contradict the button. What abandoning costs is already on the card in the
	# Cultivate policy hint ("It must stay staffed or it goes feral").
	_hud._band_labor._player_band = _cultivating_forage_band_fixture()
	_hud._compose.reset_forage_source()
	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	_hud._compose.set_forage_policy("cultivate")
	_hud._compose.set_forage_count(0)
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_unassign")
	var unassign_btn := _find_button_by_text(_hud._drawercompose._compose_sheet, "Unassign")
	_assert_hud("0 workers on a tile this band works stays live, renamed Unassign",
		unassign_btn != null and not unassign_btn.disabled)
	_assert_hud("…and does not also tell the player to assign foragers",
		not _has_label_containing(_hud._drawercompose._compose_sheet, UNSTAFFED_COPY_NEEDLE))

	# Restore the unassigned near band for the frames that follow.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_count(1)

	# State 2-crop-committed — the patch has already committed, and commitment is one-way until it
	# lapses: the picker is replaced by a locked READOUT naming the crop, so the UI cannot imply a
	# switch the sim will refuse.
	_hud.show_tile_selection(_committed_crop_tile_fixture())
	_hud._compose.set_forage_policy("cultivate")
	_compose_forage(_committed_crop_tile_fixture())
	await _settle()
	await _save("forage_crop_committed")

	_hud._compose.set_forage_policy("cultivate")
	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()

	# State 2-cultivate-stressed — knowledge known, but the patch is ⚠ Stressed: Cultivate stays visible
	# and greyed with the OTHER reason — "Patch is Stressed — ease workers off and let it regrow to
	# Thriving" (the ecology gate, not the knowledge one). The remedy is deliberately NOT "Sustain it":
	# a fully staffed Sustain takes the whole regrowth and holds a Stressed patch Stressed forever.
	_hud.show_tile_selection(_stressed_tile_fixture())
	_compose_forage(_stressed_tile_fixture())
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
	_hud._compose.set_forage_policy("sustain")
	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_sow_locked")
	# THE BLAST-RADIUS GUARD for the collapse. This frame exists to show a NON-composed rung's full
	# prerequisites — two reasons a player must tell apart, one fixed by practice and one only by moving
	# — while composing Sustain. Sustain commits nothing, so no crop picker competes for the height and
	# the collapse must NOT fire here. If this ever reads "locked (2 requirements unmet)", the opt-in has
	# leaked out of the committing rungs and this frame's whole subject is in a tooltip.
	_assert_hud("a non-committing compose still spells out a locked rung's prerequisites",
		_has_label_containing(_hud._drawercompose._compose_sheet, "Seed Selection"))
	_assert_hud("…and does not collapse it to a one-liner",
		not _has_label_containing(_hud._drawercompose._compose_sheet, "locked ("))

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
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("forage_sow_too_dry")

	# State 6b-sow-too-poor — the OTHER refusal, and the reason this pair is rendered together: thin
	# upland ground that IS watered. A different fault must produce a different sentence and a
	# different remedy — if these two frames read the same, the reason field is being wasted.
	_hud.show_tile_selection(_sow_too_poor_tile_fixture())
	_compose_forage(_sow_too_poor_tile_fixture())
	await _settle()
	await _save("forage_sow_too_poor")

	# State 6b-sow — QUALIFYING ground at last (alluvial plain beside fresh water — one of the 46).
	# ▦ Sow is ENABLED and selected, with NO refusal line. The forecast states a deal that is
	# deliberately shaped unlike Cultivate's: "Preparing: +0.02 /turn → then +2.40 /turn" — near-zero
	# while the crop is in the ground (pure investment; there is no standing stand to take a fraction
	# of), then 2× a tended patch. That asymmetry IS rung 3's bargain.
	_hud.show_tile_selection(_sowable_tile_fixture())
	_hud._compose.set_forage_policy("sow")
	_compose_forage(_sowable_tile_fixture())
	await _settle()
	await _save("forage_sow")

	# State 6b-crop-picker-sow — THE SAME long basket as `forage_crop_picker`, one rung up, on ground
	# that will take seed. `can_sow` is a DIFFERENT flag from `can_cultivate`, so only Wild Emmer stays
	# legal here and Hazel/Ground Nut join the greyed rows: the two frames side by side are what prove
	# the gate reads the composed rung's own flag rather than one "can be farmed" bit.
	_hud.show_tile_selection(_sowable_long_basket_tile_fixture())
	_hud._compose.set_forage_policy("sow")
	_hud._compose.set_forage_species("")
	_compose_forage(_sowable_long_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_sow")

	# State F3 fodder crop — a basket with a HAY crop under Sow. Hay Grass pays fodder, not provisions,
	# so its provisions ratio is 0 and the ordinary "N.N×" row would read it as worthless; the picker
	# instead shows "Hay Grass 30% · 1.8 hay". The provisions crop beside it (Wild Emmer) keeps its
	# unchanged "70% · 3.2×" ratio — proof a normal crop's row is untouched.
	_hud.show_tile_selection(_fodder_basket_tile_fixture())
	_hud._compose.set_forage_policy("sow")
	_hud._compose.set_forage_species("")
	_compose_forage(_fodder_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_fodder")

	# State F4 cash crop — a basket with a CASH crop under Sow. Flax pays trade, not provisions or
	# fodder, so its provisions ratio is 0 and the ordinary "N.N×" row would read it as worthless; the
	# picker instead shows "Flax 30% · 2.4 trade". The provisions crop beside it (Wild Emmer) keeps its
	# unchanged "70% · 3.2×" ratio — proof a normal crop's row is untouched (twin of the fodder frame).
	_hud.show_tile_selection(_cash_basket_tile_fixture())
	_compose_forage(_cash_basket_tile_fixture())   # settle the source key first (it changed)
	_hud._compose.set_forage_policy("sow")
	_hud._compose.set_forage_species("")
	_compose_forage(_cash_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_cash")

	# State F4 per-tile flora realization — the SECOND Alluvial Plain tile. Same biome as the frame
	# above, but a DIFFERENT realized basket (Cotton 55% + Flax 45% vs Wild Emmer 70% + Flax 30%): two
	# tiles of one biome now carry a seeded per-tile subset, not the uniform per-biome roster. Rendered
	# beside `forage_crop_picker_cash`, the pair is the visible proof of the whole slice — read both.
	_hud.show_tile_selection(_cash_variant_basket_tile_fixture())
	_compose_forage(_cash_variant_basket_tile_fixture())   # settle the source key first (it changed)
	_hud._compose.set_forage_policy("sow")
	_hud._compose.set_forage_species("")
	_compose_forage(_cash_variant_basket_tile_fixture())
	await _settle()
	await _save("forage_crop_picker_cash_variant")

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
	_hud._compose.set_forage_policy("cultivate")
	_compose_forage(_tended_tile_fixture())
	await _settle()
	await _save("forage_cultivate_done")

	# State 6b-sow-done — a COMPLETED Field with a standing Sow selection: ▦ Sow greys with "Already a
	# Field — ♻ Sustain-forage it to harvest", mirroring the finished-patch case one rung up (Cultivate is
	# greyed here too — the ground is both tended AND a Field).
	_hud.show_tile_selection(_field_tile_fixture())
	_hud._compose.set_forage_policy("sow")
	_compose_forage(_field_tile_fixture())
	await _settle()
	await _save("forage_sow_done")

	# Back to a plain Sustain compose for the range states below.
	_hud._compose.set_forage_policy("sustain")

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
	_hud._band_labor._player_band = _forage_range_bands()[1]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("food_forage_out_of_range")

	# State 2c — TWO bands at DIFFERENT distances from ONE food tile, NEAR band selected (821, 1 tile
	# away ≤ range 2): enabled **Forage**. The band-picker selection — not the tile — drives it.
	_hud._band_labor._player_bands = _forage_range_bands()
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("food_forage_band_near")

	# State 2d — same two bands, FAR band selected via the picker (822, ~21 tiles away): the SAME tile
	# now DISABLES Forage + shows the out-of-range hint, proving WHICH band is selected drives the
	# enabled-vs-disabled state (the case single-band playtest can't cover).
	_hud._compose.set_forage_band(int(_forage_range_bands()[1]["entity"]))
	_compose_forage(_food_tile_fixture())
	await _settle()
	await _save("food_forage_band_far")
	# Reset so later states resolve their usual band.
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)

	# band_alerts (above) overwrote _player_band with alert-fixture bands (which carry no hunt_reach);
	# re-seed the reference band so the herd assign controls resolve a proper band with a hunt reach.
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = []
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)

	# State 3 — a huntable herd selected on a food tile, WITHIN the band's hunt reach: the "Assign
	# hunters" controls (a "Band:" dropdown naming the actor band, a Hunters −/+ count, the
	# sustain/surplus/deplete/eradicate policy picker, and the local "Assign Local Hunt" button). A
	# Thriving herd shows a neutral ecology readout in the drawer.
	# Push both fixtures as the known-herd roster so the open-ended Attack/Defense bars have a
	# reference to normalize against (Elevation-style) — the mammoth holds the roster max.
	_hud.update_herds([_herd_fixture(), _deadly_herd_fixture()])
	_hud.show_herd_selection(_herd_fixture())
	_compose_herd(_herd_fixture())
	await _settle()
	await _save("herd_verbs")
	# ASSERT the HARMLESS case: the base Red Deer carries no combat components (all default 0), so its
	# component rows all read empty — and crucially NO "Harmless"/"Deadly" verdict word appears (words
	# don't survive the roster). The rows are the raw components, Elevation-style.
	var deer_lines := DetailFormat.herd_summary_lines(_herd_fixture(), _hud._band_labor.world_herds())
	assert(_danger_component_rows_present(deer_lines))
	assert(not _danger_verdict_word_present(deer_lines))
	assert(_danger_row_value(deer_lines, "Fights back").ends_with("0%"))

	# State 3b-danger — a DEADLY-TO-HUNT herd (a mammoth: attack 8, ferocity 0.9, aggression 0). Its
	# component rows read high Attack + high Fights back but EMPTY Aggressive — the "deadly to hunt, no
	# camp threat" story at a glance. Still no verdict word.
	_hud.show_herd_selection(_deadly_herd_fixture())
	await _settle()
	await _save("herd_danger")
	var mammoth_lines := DetailFormat.herd_summary_lines(_deadly_herd_fixture(), _hud._band_labor.world_herds())
	assert(_danger_component_rows_present(mammoth_lines))
	assert(not _danger_verdict_word_present(mammoth_lines))
	# Fights back 90%, Aggressive 0% — the split that proves strength ≠ danger.
	assert(_danger_row_value(mammoth_lines, "Fights back").ends_with("90%"))
	assert(_danger_row_value(mammoth_lines, "Aggressive").ends_with("0%"))

	# State 3b-predator (Predators Phase 1a) — a carnivore (Grey Wolf Pack, prey_sense_radius 4): a
	# predator is a HUNTER, not quarry, so the Size row reads "Big predator" (not "Big game") and the
	# wild-ceiling hint reads "Wild predator — hunt only" (not "Wild game — hunt only").
	_hud.show_herd_selection(_predator_herd_fixture())
	await _settle()
	await _save("herd_predator")
	var wolf_lines := DetailFormat.herd_summary_lines(_predator_herd_fixture(), _hud._band_labor.world_herds())
	var wolf_text := "\n".join(wolf_lines)
	assert(wolf_text.contains("Big predator"))
	assert(wolf_text.contains("Wild predator — hunt only"))
	assert(not wolf_text.contains("Big game"))
	assert(not wolf_text.contains("Wild game"))
	# A HERBIVORE (the deer, prey_sense_radius absent/0) is byte-for-byte unchanged — still "game".
	var deer_size_lines := DetailFormat.herd_summary_lines(_herd_fixture(), _hud._band_labor.world_herds())
	assert("\n".join(deer_size_lines).contains("game"))

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

	# Staffing readout (fauna neglect-escape arc) — the fix for the stale "N of M working" count. The
	# reference band (`_band_fixture`) staffs 4 herders on game_deer_07, and the count now comes from
	# that ACTUAL assignment, never from last turn's resolved `herded_fraction`.
	# FULLY STAFFED: the herd needs 4 and 4 are on it → a calm "Herders: 4 / 4" (neutral ink), no
	# consequence line. `herded_fraction` is a stale 0.4, so the OLD reconstruction would have read a
	# self-contradictory "2 / 4 — under-herded" — proving the fix.
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = []
	_hud.show_herd_selection(_fully_herded_herd_fixture())
	await _settle()
	await _save("herd_fully_herded")
	# ASSERT the corrected count: the actual staffed 4 shows, not the stale reconstruction 2.
	var fully_lines := DetailFormat.herd_summary_lines(
		_fully_herded_herd_fixture(), _hud._band_labor.world_herds(),
		_hud._band_labor.assigned_herders_for(_fully_herded_herd_fixture()["id"]))
	assert(_lines_contain(fully_lines, "Herders: 4 / 4"))
	assert(not _lines_any_contain(fully_lines, "under-herded"))

	# UNDER-HERDED: the herd now needs 6 herders but only 4 are staffed → an amber "Herders: 4 / 6 —
	# under-herded" (the ACTUAL count) plus the shed line "Under-herded — animals are drifting off.
	# Staff all 6 herders to hold the herd." — NOT the retired "tameness slipping" copy. `herded_fraction`
	# is a stale 1.0, so the OLD reconstruction would have read a calm "6 / 6" with no warning.
	_hud.show_herd_selection(_under_herded_herd_fixture())
	await _settle()
	await _save("herd_under_herded")
	var under_lines := DetailFormat.herd_summary_lines(
		_under_herded_herd_fixture(), _hud._band_labor.world_herds(),
		_hud._band_labor.assigned_herders_for(_under_herded_herd_fixture()["id"]))
	assert(_lines_contain(under_lines, "Herders: 4 / 6 — under-herded"))
	assert(_lines_any_contain(under_lines, "animals are drifting off"))
	assert(not _lines_any_contain(under_lines, "slipping"))

	# State 2d-γ self-feeding pen — a radius-2 pen (19 fenced tiles) on lush land: the fenced footprint
	# grazes the WHOLE feed, so the feed-split reads "Fed by pasture 100% · larder 0.0 food/turn" and the
	# amber Pen-feed debit row is gone. With no ring in flight, `_build_herd_assign_controls` shows the
	# "Extend pen" button (issues extend_pen at the pen anchor). Also carries the "Pen: radius 2 · 19
	# tiles" footprint row.
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_self_feeding_pen_herd_fixture())
	_compose_herd(_self_feeding_pen_herd_fixture())
	await _settle()
	await _save("herd_pen_self_feeding")

	# State 2d-γ extending pen — the SAME pen mid-extension (`pen_extend_progress` 0.6): the keeper is
	# fencing the next ring, so the "Extend pen" button is replaced by a WARN-amber "Fencing 60%" badge
	# (the pen twin of the corral-build "Building N%" meter). Partial pasture → "Fed by pasture 60% ·
	# larder 0.7 food/turn".
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_extending_pen_herd_fixture())
	_compose_herd(_extending_pen_herd_fixture())
	await _settle()
	await _save("herd_pen_extending")

	# State F3 foddered pen — the honest THREE-way feed split. The pen drew hay, so its GROSS demand
	# (`pen_upkeep` 2.0) partitions into pasture 40% (0.80 free) · hay 0.9 (`pen_hay_food`) · larder 0.3
	# (`pen_larder_bill`, the NET bread bill) — 0.80 + 0.90 + 0.30 == 2.0, the sim-pinned invariant. It
	# reads "Fed by pasture 40% · hay 0.9 · larder 0.3 food/turn"; the two-term states above
	# (`herd_domesticated` 0% · larder 1.7, `herd_pen_self_feeding` 100% · larder 0.0) show NO hay term,
	# so the two forms are provably different — and the larder term is now the true net, not the gross.
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_foddered_pen_herd_fixture())
	_compose_herd(_foddered_pen_herd_fixture())
	await _settle()
	await _save("herd_pen_foddered")

	# State 2d-δ wild ceiling — a hunt-only species. NO husbandry track in the drawer (no
	# domestication / corral / pen rows), just the dim "Wild game — hunt only" hint, and the hunt policy
	# picker offers the extractive four with NO Corral rung.
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_wild_herd_fixture())
	_compose_herd(_wild_herd_fixture())
	await _settle()
	await _save("herd_ceiling_wild")

	# State 2d-δ pastoral ceiling — tameable + roams, never pennable. The drawer KEEPS the "Husbandry
	# Domesticating 60%" row but shows "Herdable, not pennable" where the Corral rows would sit; the hunt
	# policy picker again drops the Corral rung.
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_pastoral_herd_fixture())
	_compose_herd(_pastoral_herd_fixture())
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
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
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
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	await _save("herd_corral_locked")

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
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_corral_ready_herd_fixture())
	_compose_herd(_corral_ready_herd_fixture(), COMPOSE_COUNT_UNSET, "corral")
	await _settle()
	await _save("herd_corral")

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
	# requirement of 2; `_band_fixture()` is restored immediately after the save, since `herd_corral_depleted`
	# and every state downstream document the reference band's 4.
	var under_herded_band := _band_fixture().duplicate(true)
	for entry in under_herded_band["labor_assignments"]:
		if entry is Dictionary and String((entry as Dictionary).get("kind", "")) == "hunt":
			(entry as Dictionary)["workers"] = UNDER_HERDED_CORRAL_HERDERS_STAFFED
	_hud._band_labor._player_band = under_herded_band
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_under_herded_corral_fixture())
	# The three-line auto-max idiom (`herd_hunt_automax`): open once so the rung is composed, arm the
	# one-shot, then re-open so it is consumed against the COMPOSED Corral. Arming before the first open
	# spends the one-shot on the re-seeded rung instead, and dialing an explicit count would overwrite
	# whatever auto-max produced — the frame must show auto-max REACHING the cap, not advertising it.
	_compose_herd(_under_herded_corral_fixture(), COMPOSE_COUNT_UNSET, "corral")
	_hud._compose.arm_hunt_autofill()
	_compose_herd(_under_herded_corral_fixture())
	await _settle()
	await _save("herd_corral_under_herded")
	_assert_hud("auto-max fills the corral crew to the herder-deficit cap of 2, proving it is reachable",
		_hud._compose.hunt_count() == UNDER_HERDED_CORRAL_HERDERS_NEEDED)
	# Restore the reference band (4 herders on game_deer_07) for everything downstream.
	_hud._band_labor._player_band = _band_fixture()

	# State 3d-corral-depleted — the SAME rung on a herd BELOW the pen's escapement point (K/2). The
	# managed harvest takes only the biomass standing above that point, so the payoff is honestly
	# +0.00 /turn while the feed is still 0.14 — a pure loss. The row must SHOW both zeros and turn
	# amber with "⚠ Too depleted to pen", never suppress the zero as if it were missing data.
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_depleted_corral_herd_fixture())
	_compose_herd(_depleted_corral_herd_fixture(), COMPOSE_COUNT_UNSET, "corral")
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
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_taming_herd_fixture())
	_compose_herd(_taming_herd_fixture(), COMPOSE_COUNT_UNSET, "tame")
	await _settle()
	await _save("two_meter_split")
	# THE HERD-SIDE BLAST-RADIUS GUARD. `HudWidgets.build_policy_picker` is SHARED with the hunt/expedition pickers,
	# and this frame's whole subject is the gated Corral's reason line — naming the knowledge, its live
	# percent and the practice that fills it — while TAME is the composed rung. The collapse is opt-in and
	# only the forage compose opts in, so the hunt picker must be byte-for-byte unchanged.
	_assert_hud("the hunt picker still spells out the gated Corral's reason (collapse is forage-only)",
		_has_label_containing(_hud._drawercompose._compose_sheet, "Penning"))
	_assert_hud("…and collapses nothing on the herd side",
		not _has_label_containing(_hud._drawercompose._compose_sheet, "locked ("))

	# State 6b-tame — the ◎ Tame affordance itself: a 6th option in the LOCAL hunt picker, beside
	# Sustain/Surplus/Deplete/Eradicate/Corral, ENABLED (Herding is known) and selected on a
	# pen-ceiling herd that is only 40% tamed. Now that the sim exports `pastoralYield`, Tame renders
	# the SAME dip→payoff pair as its three siblings: "Preparing: +<dip> → then +1.20 /turn" (dip from
	# `hunt_policy_ceilings["tame"]`, payoff = pastoral_yield, no feed term — Tame has no running cost).
	# Its picker button wears the `→ +1.20/turn` payoff, above Sustain's `up to +0.90/turn`.
	await _save("herd_tame")

	# State 6b-tame-stalled — the "why isn't my Tame progressing?" hint. Taming accrues ONLY while the
	# herd is Thriving, but is deliberately NOT gated on it (a herd's phase swings as you hunt it), so
	# the sim just PAUSES the meter. Silence here would recreate exactly the hidden-rule problem this
	# arc exists to kill, so the drawer says it: what stopped, why, that progress is NOT lost, and the
	# remedy (ease off — the opposite of "work harder").
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_taming_stalled_herd_fixture())
	_compose_herd(_taming_stalled_herd_fixture(), COMPOSE_COUNT_UNSET, "tame")
	await _settle()
	await _save("herd_tame_stalled")

	# TAMING-STARTUP-LAG GUARD — composing an INVESTMENT rung (Tame) on a still-WILD herd must offer the
	# ownership-INDEPENDENT would-be herder crew, not the 1-worker Tame-prep count. A wild herd's
	# `herders_needed` is ownership-gated to 0, so the take/prepare max-useful (1) used to pin the cap at 1;
	# the player could staff only 1, the herd became owned next turn needing 3, and read under-herded. The
	# fix floors the LOCAL-hunt cap on `herders_needed_if_managed` (3) for investment rungs only.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	# A band with idle workers comfortably above both caps (Tame 10, Sustain 7), so the stepper is bound by
	# USEFULNESS (the "max N useful here" note), not by the idle-labor ceiling (a different note entirely).
	var tame_cap_band := _band_fixture()
	tame_cap_band["idle_workers"] = 20
	tame_cap_band["working_age"] = 40
	_hud._band_labor._player_band = tame_cap_band
	_hud._band_labor._player_bands = [tame_cap_band]
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_tame_worker_cap_herd_fixture())
	# Tame is DIALED IN through `_compose_herd`, which survives the source-change re-seed — see its doc.
	_compose_herd(_tame_worker_cap_herd_fixture(), COMPOSE_COUNT_UNSET, "tame")
	await _settle()
	await _save("herd_tame_worker_cap")
	# Tame floors the cap on the would-be crew (10), NOT the Tame-prep useful (1): the sheet's max-useful
	# note reads "max 10 workers useful here". Pre-fix it read "max 1 worker useful here" (floored on the
	# ownership-gated herders_needed 0).
	_assert_hud("Tame offers the full would-be herder crew (max 10), not the 1-worker prep count",
		_has_label_containing(_hud._drawercompose._compose_sheet, "max 10 workers useful"))
	_assert_hud("…and not the pre-fix 1-worker cap",
		not _has_label_containing(_hud._drawercompose._compose_sheet, "max 1 worker useful"))
	# COMPANION — the EXTRACTIVE Sustain rung manages nothing, so it needs no herders: its cap is
	# take-useful only (Sustain 1.50 ÷ 0.30 = 5), and the would-be crew (3) must NOT leak into it.
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_tame_worker_cap_herd_fixture())
	_compose_herd(_tame_worker_cap_herd_fixture(), COMPOSE_COUNT_UNSET, "sustain")
	await _settle()
	await _save("herd_tame_worker_cap_sustain")
	_assert_hud("Sustain caps on its own take-useful (max 7), floored at 0",
		_has_label_containing(_hud._drawercompose._compose_sheet, "max 7 workers useful"))
	_assert_hud("…the would-be herder crew (10) does not leak into an extractive rung",
		not _has_label_containing(_hud._drawercompose._compose_sheet, "max 10 workers useful"))

	# Back to a plain Sustain compose for the band-picker / distance states below.
	_hud._compose.set_hunt_policy("sustain")
	_hud._compose.reset_hunt_source()

	# State 3f — TWO player bands: the "Assign hunters" controls' "Band:" dropdown lists both
	# (positional "Band 1" / "Band 2"). Default selection is the resolved band (Band 1, 12 idle).
	# The Hunters count is dialed to 8 and CLAMPS to 7 with `+` disabled, because the binding cap here
	# is USEFULNESS ("max 7 workers useful here"), not the band's 12 idle — the frame shows the stepper
	# answering to the sheet's own ceiling while the picker's default selection resolves.
	_hud._band_labor._player_bands = _two_player_bands()
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()   # force a fresh seed so the default selection = resolved band
	_hud.show_herd_selection(_herd_fixture())
	_compose_herd(_herd_fixture(), 8)
	await _settle()
	await _save("herd_band_picker")

	# State 3g — same, after switching the dropdown to Band 2 (only 2 idle): the picker path
	# re-caps the Hunters count to the newly-selected band's assignable workers (8 → 2, + now
	# disabled), demonstrating selection → actor band → stepper re-cap.
	var second_band: Dictionary = _two_player_bands()[1]
	_hud._compose.set_hunt_band(int(second_band["entity"]))
	_hud._compose.set_hunt_count(clampi(
		_hud._compose.hunt_count(), 0, _hud._band_labor.assignable_hunt_workers(second_band, _herd_fixture()["id"])))
	_compose_herd(_herd_fixture())
	await _settle()
	await _save("herd_band_picker_b")
	# Reset so later states render their usual single-band dropdown.
	_hud._band_labor._player_bands = []
	_hud._compose.reset_hunt_source()

	# State 3h — distance-aware herd-hunt, SINGLE far band: a lone band ~27 tiles from the herd (beyond
	# its hunt_reach 7). The affordance fully replaces the local option — the button reads "Send Hunting
	# Expedition", a distance hint shows, the stepper reads "Party", and Assign emits
	# send_hunt_expedition (party = the stepper), NOT assign_labor.
	_hud._band_labor._player_bands = [_hunt_distance_bands()[1]]   # only the FAR band
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(_hunt_distance_herd())
	_compose_herd(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_expedition")

	# State 3i — TWO bands at DIFFERENT distances from ONE herd, NEAR band selected: band 811 sits ON
	# the herd (distance 0 ≤ reach 7) → "Assign Local Hunt" + assign_labor. The band-picker selection —
	# not the herd — drives it (the resolved/default band is the near one here).
	_hud._band_labor._player_bands = _hunt_distance_bands()
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(_hunt_distance_herd())
	_compose_herd(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_band_near")

	# State 3j — same two bands, FAR band selected via the picker (entity 812, ~27 tiles away): the SAME
	# herd now offers "Send Hunting Expedition" (party cap = min(idle 6, max party 8) = 6), proving that
	# WHICH band is selected flips the label + command + band-entity target, not the herd.
	_hud._compose.set_hunt_band(int(_hunt_distance_bands()[1]["entity"]))   # FAR band
	_compose_herd(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_band_far")
	# Reset so later states render their usual single-band dropdown + default band.
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)

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
	#                    the DISABLED "Herd too lean to raid" button, exactly as 3r below (the HERD has
	#                    nothing left to give, and no party size can fix a herd with no surplus).
	#   3o eradicate   — a healthy Red Deer on Eradicate: it DELIVERS like every other rung (#337 pays each
	#                    rung the species' yield vector), and its cell ran the whole horizon still
	#                    delivering → amber LONG-RAID line + "Send Anyway (long raid)". NOT a denial:
	#                    denial is now a property of the QUARRY (pays neither product), not of the rung.
	# WARNED, not BLOCKED — and never a confirm dialog: a slow raid and a long one are real tradeoffs, so
	# they read as a price tag and stay ENABLED. The ONE blocked case is 3n's, a herd with no surplus
	# left: it would return empty at every party size, so there is no price to pay.
	_hud._band_labor._player_bands = [_hunt_preview_far_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	for state: Dictionary in _hunt_assign_forecast_states():
		var far_herd: Dictionary = state["herd"]
		_hud._compose.reset_hunt_source()    # force a fresh seed (band = resolved, policy = the herd's current)
		_hud._compose.set_hunt_band(-1)
		_hud.show_herd_selection(far_herd)
		# The policy-picker click, without the click.
		_compose_herd(far_herd, HUNT_FORECAST_PARTY, String(state["policy"]))
		await _settle()
		await _save(String(state["name"]))

	# AUTO-MAX on a policy click (expedition branch): picking a policy fills the Party to that policy's
	# max-useful cap. The mammoth's Sustain payload keeps rising to the fieldable ceiling, so a Sustain
	# click sets the party to 6 (min(plateau, idle 6)) — the "give me everything, zero idle hunters"
	# default. The compose hunt autofill is the one-shot a policy CLICK arms; the rebuild consumes it.
	var automax_herd := _partial_waste_mammoth()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(automax_herd)
	_hud._compose.set_hunt_policy("sustain")
	_hud._compose.arm_hunt_autofill()
	_compose_herd(automax_herd)
	await _settle()
	await _save("herd_hunt_expedition_automax")

	# States 3p–3s — the RAID readout (delivered payload + waste) + the party stepper capped at max-useful.
	# A hunting expedition is a greedy raid: it grabs the herd's standing surplus in a burst and comes home,
	# so the headline is the delivered PAYLOAD, and `deliveredFood` PLATEAUS with party size once the surplus
	# (not the pack) binds — that plateau IS max-useful. The clean Wild Boar carries the server's measured
	# raid (hauls its whole kill, no waste). The picker buttons read each policy's MAX food/turn, ascending.
	#   3p boar raid   — a 1-hunter raid: "delivers ≈5 Wild Boar over ≈7 turns · ~20 food" (no waste), cyan +
	#                    primary "Send Hunting Expedition"; picker "up to +10.67 / +13.33 / +14.67 /turn".
	#   3q max useful  — 2 hunters: "delivers ≈8 Wild Boar over ≈8 turns · ~32 food"; a 3rd delivers NO more
	#                    food (the surplus binds), so the stepper caps at 2 and the `+` note reads
	#                    "max 2 workers useful here — more would be idle". The silent-idle-hunter gap, closed.
	#   3r no surplus  — a herd stripped to its floor: deliveredFood = 0 at EVERY size → the raid returns
	#                    empty → red "too lean to raid" + the DISABLED "Herd too lean to raid" button (party
	#                    size can't fix it — surplus is a property of the herd, not the party).
	#   3s eradicate   — the boar on Eradicate: the whole-stock windfall comes home (#337), so the raid line
	#                    quotes its payload in BOTH products and the Send button is the ordinary one. What
	#                    Eradicate costs is the herd itself, permanently — never the payload.
	var boar := _raid_boar_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(boar)
	_compose_herd(boar)   # source_changed seeds party = 1
	await _settle()
	await _save("herd_hunt_boar_raid")

	_hud._compose.set_hunt_count(2)               # key unchanged → no re-seed; caps at the plateau (2)
	_compose_herd(boar)
	await _settle()
	await _save("herd_hunt_max_useful")

	# State 3q-travel — the SAME boar raid, staffed by a band the herd is 8 tiles away from (beyond
	# hunt_reach 7 → expedition) and carrying a move rate. `turnsToFill` is HUNTING turns only, so the
	# client adds the round-trip TRAVEL the band-agnostic estimate table can't (ceil(2 × 8 / 2) = 8): at
	# party 2 the readout reads "delivers ≈8 Wild Boar over ≈16 turns (8 hunting + 8 travel) · ~32 food",
	# and the stepper still caps at the animalsTaken plateau (2). `band_move_tiles_per_turn` now ships on the
	# wire (schema slot 124) and is decoded onto the band; this fixture carries it exactly as the decoder does.
	_hud._band_labor._player_bands = [_raid_travel_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(boar)
	_compose_herd(boar, 2)
	await _settle()
	await _save("herd_hunt_raid_travel")
	# Restore the far band (no move rate) for the remaining raid states.
	_hud._band_labor._player_bands = [_hunt_preview_far_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]

	var lean := _no_surplus_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(lean)
	_compose_herd(lean, HUNT_FORECAST_PARTY)
	await _settle()
	await _save("herd_hunt_no_surplus")

	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(boar)
	_compose_herd(boar, 2, "eradicate")
	await _settle()
	await _save("herd_hunt_eradicate")
	_hud._compose.set_hunt_policy("sustain")

	# States 3t–3v — the LABOR-BOUND note. When the herd's max-useful party exceeds the hunters you can
	# field, the `+` caps at LABOR (not usefulness), and the note names the reason AND the ceiling you're
	# working toward — "N of M useful — free up idle workers to send more". The Steppe Bison's plateau
	# DIFFERS BY POLICY (Sustain 4, Deplete 7), which is how the "of M" is shown to track the policy.
	var bison := _labor_bound_raid_herd()
	var bound_band: Dictionary = _hunt_preview_far_band().duplicate(true)
	bound_band["idle_workers"] = 3           # below Sustain's plateau of 4 AND Deplete's of 7 → labor-bound
	_hud._band_labor._player_bands = [bound_band]
	_hud._band_labor._player_band = bound_band
	#   3t Sustain — idle 3 < plateau 4 → "3 of 4 useful — free up idle workers to send more", + dead at 3.
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(bison)
	_compose_herd(bison, 3, "sustain")
	await _settle()
	await _save("herd_hunt_labor_bound")
	_assert_hud("the labor-bound frame renders the 3-hunter crew idle labor caps it at",
		_hud._compose.hunt_count() == 3)
	#   3u Deplete — SAME herd + band, policy flipped: the plateau rises to 7 → "3 of 7 useful", proving the
	#              ceiling tracks the selected policy. Key unchanged so the policy override sticks.
	_hud._compose.set_hunt_policy("deplete")
	_compose_herd(bison)
	await _settle()
	await _save("herd_hunt_labor_bound_deplete")
	#   3v Party-size-bound — the SUB-CASE where freeing idle workers would NOT help: idle 6 >= max party 2,
	#              so the party-SIZE cap binds, not idle. The note reads "2 of 4 useful — at the max party
	#              size" instead of the free-up-workers advice.
	var party_capped: Dictionary = _hunt_preview_far_band().duplicate(true)
	party_capped["idle_workers"] = 6
	party_capped["max_expedition_party_size"] = 2
	_hud._band_labor._player_bands = [party_capped]
	_hud._band_labor._player_band = party_capped
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(bison)
	_compose_herd(bison, 2, "sustain")
	await _settle()
	await _save("herd_hunt_party_size_bound")
	_assert_hud("the party-size-bound frame renders the 2-hunter crew the max party size caps it at",
		_hud._compose.hunt_count() == 2)
	# Restore the far band + sustain for the states that follow.
	_hud._band_labor._player_bands = [_hunt_preview_far_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.set_hunt_policy("sustain")

	# States 3n–3o — the same panel's LOCAL branch (herd within hunt_reach). The preview line reads the
	# crew's HONEST carry-aware delivered take in ANIMALS (delivered ÷ food_per_animal), not the
	# unquantized food rate. Red Deer fpa 2.0, band per-worker 0.8, output 0.9; Sustain ceiling 0.30,
	# Deplete 0.60. `LOCAL_HUNT_HUNTERS` is dialed in, but the stepper clamps the crew to 3 carriers
	# (`LOCAL_HUNT_CAPPED_CREW`) — and the clamp is immaterial to what these two frames show: a 3-hunter
	# crew collects 2.16 food/turn and a 6-hunter one 4.32, both far above the ceilings below, so the
	# HERD's flow ceiling is what binds and the quantized take is the same either way:
	#   3n Sustain — delivered = min(0.30×0.9, …) = 0.27 → ≈0.14 Red Deer/turn · renewable (green).
	#   3o Deplete  — delivered 0.54 > Sustain 0.27 → WARN-amber "⚠ ≈0.27 Red Deer/turn — overdraws the
	#                herd" (the same ⚠ the allocation rows use). No waste (a whole deer is carryable).
	# (The herd's `hunt_trip_estimates` ride along but are IGNORED here — a trip table answers an
	# EXPEDITION's question; a local hunt is carry arithmetic over the band's flow ceilings. Band = flow
	# arithmetic; expedition = lookup.)
	var local_herd := _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
		DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS)
	_hud._band_labor._player_bands = [_hunt_preview_local_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(local_herd)
	_compose_herd(local_herd, LOCAL_HUNT_HUNTERS)
	await _settle()
	await _save("herd_hunt_local_sustain")
	_assert_hud("the local-hunt frames render the dialed-in crew (capped), not the re-seeded 1",
		_hud._compose.hunt_count() == LOCAL_HUNT_CAPPED_CREW)

	# Flip the policy picker to Deplete — the same click path the player takes; the preview line
	# re-computes live off the new ceiling.
	_hud._compose.set_hunt_policy("deplete")
	_compose_herd(local_herd)
	await _settle()
	await _save("herd_hunt_local_overdraw")

	# The SAME local picker flipped to Eradicate — the frame the rung's HINT is judged on (issue #337).
	# Its text must describe the whole-stock windfall + the permanent end state, and must NOT claim the
	# rung yields nothing: the sim pays every rung its species' yield vector, Eradicate included.
	_hud._compose.set_hunt_policy("eradicate")
	_compose_herd(local_herd)
	await _settle()
	await _save("herd_hunt_local_eradicate")

	# States 3p–3q — the WHOLE-ANIMAL carry cap. A big-game aurochs drops as one 80-biomass body via the
	# kill-credit bank; food_per_animal 1.6 outweighs one hunter's carry (per_worker 0.80), so the cap is
	# the CARRIERS needed to haul the peak-turn drop, not ceil(smoothed-rate / per_worker). Sustain
	# (ceiling 0.74) used to read "max 1 useful" (the bug: ceil(0.74/0.80)=1) — it must now read "max 2".
	_hud._band_labor._player_bands = [_hunt_preview_local_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	var aurochs := _aurochs_big_game_fixture()
	_hud.show_herd_selection(aurochs)
	_compose_herd(aurochs, 1, "sustain")
	await _settle()
	await _save("herd_hunt_whole_animal_cap")

	# Flip to Deplete — two bodies drop on the peak turn, so the cap climbs to 4: it tracks the selected
	# policy's ceiling, exactly as the smoothed-rate cap did.
	_hud._compose.set_hunt_policy("deplete")
	_compose_herd(aurochs)
	await _settle()
	await _save("herd_hunt_whole_animal_cap_deplete")

	# States 3s–3v — the CARRY-AWARE ANIMALS-FIRST local-hunt preview (spec oracle: deer fpa 1.23, band
	# per-worker 0.8, output 1.0, Sustain ceiling 2.33). The preview line reads the crew's HONEST
	# delivered take in animals, not the unquantized food rate the crew could never carry; the policy
	# buttons read "up to X/turn" (the herd's cap, worker-independent).
	_hud._band_labor._player_bands = [_delivered_oracle_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]

	# 3s — 2 hunters land exactly one whole 1.23 deer/turn, no waste → "≈1 Red Deer/turn · renewable",
	# and the four ascending "up to +2.33 / +3.50 / +5.00 / +7.00 /turn" cap buttons.
	var oracle_clean := _delivered_oracle_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(oracle_clean)
	_compose_herd(oracle_clean, 2, "sustain")
	await _settle()
	await _save("herd_hunt_delivered_clean")

	# 3t — 1 hunter can't carry even one whole deer (0.80 < 1.23), so 35% of the kill rots →
	# "≈0.65 Red Deer/turn · ⚠ 35% wasted" (green line, amber waste suffix).
	var oracle_waste := _delivered_oracle_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(oracle_waste)
	_compose_herd(oracle_waste, 1, "sustain")
	await _settle()
	await _save("herd_hunt_delivered_waste")

	# 3u — AUTO-MAX on policy select: simulate the picker click path (autofill flag + policy set) starting
	# from a count of 1; the rebuild fills the crew to the Sustain max-useful cap (4 carriers), so the
	# stepper sits at 4 and the line reads the full ≈1.89 deer/turn with zero waste.
	var oracle_automax := _delivered_oracle_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(oracle_automax)
	_compose_herd(oracle_automax, 1, "sustain")
	_hud._compose.arm_hunt_autofill()
	_compose_herd(oracle_automax)
	await _settle()
	await _save("herd_hunt_automax")

	# 3v — big game (mammoth fpa 16, Sustain ceiling 2.4): auto-max staffs the 20 carriers, delivered
	# 2.4 → ≈0.15 mammoth/turn, and the averaging-WINDOW hint appears: "≈1 Woolly Mammoth every ~7
	# turns — the rate above is averaged over that span."
	var window_herd := _big_game_window_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(window_herd)
	_compose_herd(window_herd, 1, "sustain")
	_hud._compose.arm_hunt_autofill()
	_compose_herd(window_herd)
	await _settle()
	await _save("herd_hunt_big_game_window")

	# 3w — THE INEDIBLE QUARRY (issue #337). A wolf pays PELTS AND NO MEAT: `provisions == 0` on every
	# rung, a real trade ceiling on all four. This is the frame the whole arc is judged on. The picker's
	# four buttons must read FOUR ASCENDING TRADE numbers on their second line — `0.90 / 1.35 / 1.95 /
	# 2.70 trade` — with NO food term and NO zeros anywhere; before the fix the client read only food, so all four read `+0.00`
	# and the pack rendered as a source worth nothing. The preview line below the picker must still show a
	# per-turn ANIMAL rate (the ratio is unit-free — it divides by the TRADE quantum, since the food one
	# is honestly 0), and the averaging-window disclaimer must still appear.
	var wolf := _pelt_only_wolf_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(wolf)
	# Crew + rung go through `_compose_herd`, which dials them in AFTER the source-change re-seed.
	_compose_herd(wolf, PELT_FRAME_HUNTERS, "sustain")
	await _settle()
	await _save("herd_hunt_pelts_only")

	# 3x — the same wolf as an EXPEDITION target (band 27 tiles off). `delivers_food = false` on every
	# cell now means THE QUARRY IS INEDIBLE, not "a denial mission", so the raid line must read a real
	# delivery whose payload is trade goods — `delivers ≈5 Grey Wolf over ≈9 turns · ⇄ ~7 trade goods` —
	# and the Send button must be the ordinary primary send, NOT "brings nothing home".
	var wolf_raid := _pelt_only_wolf_raid_herd()
	_hud._band_labor._player_bands = [_hunt_preview_far_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(wolf_raid)
	_compose_herd(wolf_raid, PELT_FRAME_HUNTERS, "sustain")
	await _settle()
	await _save("herd_hunt_pelts_raid")

	# 3y — THE BOTH-PRODUCTS CONTROL: the same oracle deer, whose hide sells beside its meat. Each picker
	# button's product line must carry BOTH components with FOOD LEADING (`2.33 food · 0.34 trade`), which
	# is the half of the rule the wolf frame cannot prove. Rendered right after the wolf so the two are
	# compared directly. Both frames also judge the TWO-LINE FACE itself: the rung's name over its
	# products, so `which rung` and `what it pays` stop competing in one line of glyphs.
	_hud._band_labor._player_bands = [_delivered_oracle_band()]
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	var oracle_pair := _delivered_oracle_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(oracle_pair)
	_compose_herd(oracle_pair, PELT_FRAME_HUNTERS, "sustain")
	await _settle()
	await _save("herd_hunt_both_products")

	# 3z — THE INVESTMENT-RUNG TWIN of 3y (issue #397). The extractive rungs above have paid a pair since
	# #337, but Tame and Corral rendered a FOOD-ONLY payoff face — a Wild Boar read `→ 1.48 food` beside
	# its own extractive rungs' `0.74 food · 0.18 trade`, silently dropping a trade half the sim exports
	# (`pastoralTrade` / `corralTrade`). A prepared herd pays the same two products a hunted one does, so
	# the payoff obeys the same render-only-when-non-zero rule: both faces must name FOOD THEN TRADE.
	# Domestication is mid-ladder on purpose — Tame retires from the picker once the herd is fully tamed,
	# and Corral is knowledge-gated below that, so a frame carrying BOTH rungs necessarily has one greyed.
	# A gated rung still wears its payoff (that is the point of showing it), which this frame also proves.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 1.0, "penning": 1.0,
	}])
	var payoff_boar := _investment_pair_boar_herd()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(payoff_boar)
	_compose_herd(payoff_boar, PELT_FRAME_HUNTERS, "tame")
	await _settle()
	await _save("herd_investment_both_products")
	# The rungs are found by `HudWidgets.POLICY_RUNG_META`, never by button text: the face is an
	# empty-`text` Button under a two-Label stack, so a text search finds nothing at all.
	_assert_hud("Tame's payoff face names BOTH products, food leading",
		_policy_rung_metric(_hud._drawercompose._compose_sheet, "tame") == BOAR_TAME_PAYOFF_FACE)
	_assert_hud("Corral's payoff face names BOTH products, food leading",
		_policy_rung_metric(_hud._drawercompose._compose_sheet, "corral") == BOAR_CORRAL_PAYOFF_FACE)

	# Reset so later states render their usual single-band dropdown + default band/policy.
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud._compose.set_hunt_policy("sustain")

	# State 3d — a populated hex: the Tile card + the Occupants roster split. Three
	# player bands (turns_of_food 15 / 7 / 2 → green / amber / red vitality dots, with
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

	# State 3e-staffed — the SAME hex, with the bison actually being hunted BOTH ways at once: a
	# standing local hunt (4 workers assigned by Band Fen) and a detached hunting party of 6
	# committed to the same herd. The wildlife row's meta must read the SUM, `10 🏹`, right-aligned
	# exactly like the land row's `N 🌾` — one herd, two mechanisms, one staffing number. The drawer
	# leads with `Size: Big game`, the class that used to ride the row.
	var hunted_bands: Array = _occupied_units_fixture()
	hunted_bands[0]["labor_assignments"] = [
		{"kind": "hunt", "workers": OCCUPANTS_HUNT_LOCAL_WORKERS, "fauna_id": "game_bison_02",
			"policy": "sustain", "target_x": 58, "target_y": 24},
	]
	_hud._band_labor._player_bands = hunted_bands
	_hud._band_labor._player_band = hunted_bands[0]
	_hud._band_labor._player_expeditions = [
		{"id": "Party Fen", "entity": 401, "home_band_entity": 301,
			"size": OCCUPANTS_HUNT_PARTY_WORKERS, "expedition_mission": "hunt",
			"expedition_target_herd": "game_bison_02", "expedition_phase": "outbound",
			"current_x": 59, "current_y": 24},
	]
	_hud.show_herd_selection(_occupied_herd_fixture())
	await _settle()
	await _save("occupants_herd_staffed")
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_expeditions = []

	# ---- ONE CARD, ONE LIST, ONE DRAWER (docs/plan_tile_panel_layout.md) ------------------------
	# The hex is now a single card: a pinned chip strip, one selectable list with the LAND as its
	# first row, and one height-capped drawer that whichever row is lit fills. These six states are
	# the layout's own frames — every other tile/herd/forage state above exercises the same builders
	# through it, which is why their framing changed with this arc.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_hud._compose.set_forage_count(3)

	# tile_panel_land — the LAND row lit: chips pinned above (In sight · Hospitable · Temperate ·
	# Fertile · Verdant Basin), the land row leading the list with the tile's forage glyph + biome
	# name, and the terrain rows + "Assign foragers" compose block in the drawer beneath.
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("tile_panel_land")

	# tile_panel_no_forage — the same layout on ground that offers nothing: the land row's meta
	# reads "No forage" and the drawer carries terrain rows with NO compose block.
	_hud.show_tile_selection(_barren_tile_fixture())
	await _settle()
	await _save("tile_panel_no_forage")

	# tile_panel_herd — a herd row lit: the land row is STILL in the list above it (the land never
	# leaves), and the hunt compose block fills the one drawer.
	_hud._band_labor._player_band = _hunt_preview_local_band()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(_occupied_herd_fixture())
	await _settle()
	await _save("tile_panel_herd")

	# tile_panel_crowded — THE state this arc exists for: 3 bands + 2 herds. Every row must be
	# visible, the drawer must CAP (scrolling internally on the selected band's allocation block),
	# and the whole card must fit the dock without the dock itself scrolling.
	# The player faction really IS these three bands here, and the first of them forages this very
	# hex — so the land row must report the hex's STAFFING (`5 🌾`), not restate the module name the
	# drawer and the sheet header already carry (§20). Leaving `_player_bands` empty made the row
	# fall back to the module label and ellipsise it, which is the defect, not the fixture's intent.
	_hud._band_labor._player_bands = _crowded_bands_fixture()
	_hud.show_tile_selection(_crowded_tile_fixture())
	await _settle()
	await _save("tile_panel_crowded")
	# NO Band/City panel is injected here, so this is the legacy fallback path — it renders
	# `%AllocationPanel`, whose Orders block already carries a Move. The drawer's §18 button must NOT
	# be added on top of it, or the player would see the same order offered twice.
	_assert_hud("the no-panel fallback shows exactly ONE Move button",
		_count_buttons_by_text(_hud.allocation_panel, MOVE_BUTTON_TEXT) == 1)

	# tile_panel_no_flash — THE FLASH-MECHANISM GUARD (docs/plan_hud_decomposition.md §2a). The
	# tile-inspector "flash" on every turn-advance was `_render_selection_panel` UNCONDITIONALLY
	# tearing down + recreating the card's chips / roster rows / drawer actions even on a same-tile
	# restate where only numbers moved. That teardown is a transient the static PNG harness cannot
	# capture, so this proves the MECHANISM instead: a same-tile restate with CHANGED NUMBERS, fed
	# through the REAL per-snapshot `reapply_selection("tile", …)` path, must PATCH the existing nodes
	# in place — SAME instances, values updated — never free + rebuild them; while a genuine identity
	# change (a band entering the hex) still DOES rebuild, so the diff cannot mask a real update.
	# Proven to FAIL against the pre-fix teardown code.
	_hud.clear_selection()
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _no_flash_band_fixture(3, 0.90)
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_hud.show_tile_selection(_no_flash_tile_fixture(0.01, 84.0))
	await _settle()
	var flash_chip_ids := _child_instance_ids(_hud.tile_chips)
	var flash_row_ids := _child_instance_ids(_hud.subject_list)
	var flash_action_ids := _child_instance_ids(_hud.forage_assign_controls)
	var flash_chip_before := _chip_text(_hud.tile_chips, 1)          # the habitability chip
	var flash_summary_before := _forage_summary_text()
	# A SECOND snapshot of the SAME tile with different numbers (habitability rating, patch biomass,
	# forage worker count + rate), replayed the way Main replays MapView's payload every turn.
	_hud._band_labor._player_band = _no_flash_band_fixture(5, 1.40)
	_hud.reapply_selection("tile", _no_flash_tile_fixture(0.06, 61.0))
	await _settle()
	await _save("tile_panel_no_flash")
	_assert_hud("same-tile restate REUSES the chip nodes (no teardown)",
		_child_instance_ids(_hud.tile_chips) == flash_chip_ids and not flash_chip_ids.is_empty())
	_assert_hud("same-tile restate REUSES the roster-row nodes (no teardown)",
		_child_instance_ids(_hud.subject_list) == flash_row_ids and not flash_row_ids.is_empty())
	_assert_hud("same-tile restate REUSES the forage drawer-action nodes (no teardown)",
		_child_instance_ids(_hud.forage_assign_controls) == flash_action_ids and not flash_action_ids.is_empty())
	_assert_hud("…and the reused chip's value updated to the new number (Hospitable → Harsh)",
		_chip_text(_hud.tile_chips, 1) != flash_chip_before and _chip_text(_hud.tile_chips, 1) != "")
	_assert_hud("…and the reused drawer summary updated to the new worker count/rate",
		_forage_summary_text() != flash_summary_before and _forage_summary_text() != "")
	# The identity-change half: a band ENTERING the hex must rebuild the roster, so a masked-update
	# bug in the diff cannot survive here.
	var flash_tile_with_band := _no_flash_tile_fixture(0.06, 61.0)
	flash_tile_with_band["units"] = [_no_flash_band_fixture(5, 1.40)]
	_hud.reapply_selection("tile", flash_tile_with_band)
	await _settle()
	_assert_hud("a band entering the hex REBUILDS the roster (membership changed)",
		_child_instance_ids(_hud.subject_list) != flash_row_ids)
	_hud.clear_selection()
	_hud._band_labor._player_band = {}

	# ---- PART 2: THE COMPOSE SHEET (docs/plan_tile_panel_layout.md §10-§17) ----------------------
	# The two ~270px compose blocks left the drawer for a floating sheet. The states above are now the
	# READ state (a standing summary + `Assign … ▸`, and the drawer is visibly shorter for it); these
	# are the WRITE state.

	# tile_panel_compose_forage — the sheet open over the LAND: the full policy grid + band picker +
	# stepper + forecast + button, floating beside the selection card. The MAP MUST STILL BE VISIBLE
	# behind it — an assignment is composed AGAINST the map (work-range ring, hunt reach), so unlike
	# NarrativeForkPanel this sheet draws NO scrim.
	_hud._band_labor._player_band = _forage_range_bands()[0]
	_hud._band_labor._player_bands = []
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_hud.show_tile_selection(_food_tile_fixture())
	_compose_forage(_food_tile_fixture())
	await _settle()
	_assert_hud("the Assign button opens the compose sheet", _hud.is_compose_sheet_open())
	await _save("tile_panel_compose_forage")

	# tile_panel_compose_herd — the herd sheet on the EXPEDITION branch (the band is beyond hunt
	# reach): the raid forecast + "Send Hunting Expedition" must survive the move to the sheet intact.
	_hud._band_labor._player_bands = [_hunt_distance_bands()[1]]   # only the FAR band
	_hud._band_labor._player_band = _hud._band_labor._player_bands[0]
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(_hunt_distance_herd())
	_compose_herd(_hunt_distance_herd())
	await _settle()
	await _save("tile_panel_compose_herd")

	# tile_panel_compose_gated — a LOCKED rung inside the sheet: 🐄 Corral greyed AND its gate reasons
	# rendered right beside it. The reasons explain the greyed button, so they had to travel WITH the
	# picker; a reason left behind in the drawer would explain a button that is no longer there.
	_hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 0.35,
	}])
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud.show_herd_selection(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	await _save("tile_panel_compose_gated")

	# ---- BEHAVIOURAL ASSERTIONS (§17) -----------------------------------------------------------
	# (2) A SNAPSHOT MUST NOT CLOSE THE SHEET. `reapply_selection` runs every turn; closing on it
	# would make the sheet unusable under autoplay. Driven through the real per-snapshot path — the
	# same `reapply_selection("herd", …)` Main replays from MapView's payload — with the sheet open.
	_assert_hud("precondition: the herd sheet is open before the snapshot",
		_hud.is_compose_sheet_open())
	_hud.reapply_selection("herd", _corral_locked_herd_fixture())
	await _settle()
	_assert_hud("a snapshot re-render leaves the compose sheet OPEN",
		_hud.is_compose_sheet_open())
	# …and the SAME refresh DOES close it when the subject it is composing is gone. This half is what
	# proves the half above is not vacuous: the refresh really ran and chose to keep the sheet.
	_hud.reapply_selection("herd", _raid_boar_herd())   # a DIFFERENT herd id
	await _settle()
	_assert_hud("a snapshot that swaps the subject closes the sheet",
		not _hud.is_compose_sheet_open())
	# Re-open on the herd the targeting assertion below needs.
	_hud.show_herd_selection(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()

	# (3) STARTING A TARGETING FLOW CLOSES THE SHEET — a floating sheet over the map while the player
	# is being asked to click a hex is a trap. Driven through the real Move-band entry point.
	_hud._targeting.begin_move_band()
	await _settle()
	_assert_hud("starting move-band targeting closes the compose sheet",
		not _hud.is_compose_sheet_open())

	# (1) ESC PRECEDENCE. The chain lives in `Main.escape_claimant`, driven here with the REAL HUD's
	# own `is_compose_sheet_open()` / `is_targeting_active()` rather than hardcoded booleans. It is
	# asserted with BOTH TRUE AT ONCE — targeting is still armed above and the player then opens the
	# sheet on top of it (the drawer stays clickable during targeting, so this is a state the client
	# really reaches). Both-true is the only configuration that can tell the ORDER apart: with the
	# sheet open alone, any ordering answers "compose_sheet".
	_hud.show_herd_selection(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	_hud._targeting.begin_move_band()
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	_assert_hud("precondition: a sheet and targeting are BOTH active",
		_hud.is_compose_sheet_open() and _hud.is_targeting_active())
	_assert_hud("ESC claims the sheet AHEAD of targeting (and never the pause menu)",
		MAIN_SCRIPT.escape_claimant(false, _hud.is_compose_sheet_open(), _hud.is_targeting_active())
			== MAIN_SCRIPT.ESC_COMPOSE_SHEET)
	_hud.close_compose_sheet()
	await _settle()
	_assert_hud("…and with the sheet closed, ESC falls back through to targeting-cancel",
		MAIN_SCRIPT.escape_claimant(false, _hud.is_compose_sheet_open(), _hud.is_targeting_active())
			== MAIN_SCRIPT.ESC_TARGETING)
	_hud.cancel_active_targeting()
	await _settle()

	# (4) A WHEEL TICK OVER THE CATCHER MUST NOT DISMISS THE SHEET. The catcher is MOUSE_FILTER_STOP
	# across the whole viewport, so an idle scroll anywhere over the map lands on it — and this sheet
	# has NO SCRIM precisely because the player is still reading that map while composing. Dismissing
	# on a wheel tick would throw the composition away mid-read. Driven through the REAL handler by
	# emitting the catcher's own `gui_input`, and paired with the left-click half, which is what proves
	# the wheel half is not vacuous (i.e. that click-outside dismissal still works at all).
	_hud.show_herd_selection(_corral_locked_herd_fixture())
	_compose_herd(_corral_locked_herd_fixture())
	await _settle()
	_assert_hud("precondition: the sheet is open before the wheel tick",
		_hud.is_compose_sheet_open())
	for wheel_button in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN]:
		_hud._drawercompose._compose_sheet.gui_input.emit(_mouse_button_event(wheel_button))
	await _settle()
	_assert_hud("a wheel tick on the catcher leaves the compose sheet OPEN",
		_hud.is_compose_sheet_open())
	_hud._drawercompose._compose_sheet.gui_input.emit(_mouse_button_event(MOUSE_BUTTON_LEFT))
	await _settle()
	_assert_hud("a left-click on the catcher still CLOSES the compose sheet",
		not _hud.is_compose_sheet_open())

	# tile_panel_standing — §14's own frame: the drawer's CLOSED read state on a source the player
	# already works. The summary reuses `SourceForecast.source_yield_readout` verbatim, so it wears the same three
	# parts a Band-panel Current-actions row does — the policy glyph + crew + rate, the ⚠ overdraw
	# flag (ecological) and the "· only N of M working" overstaff note (labor). This fixture crosses
	# the two deliberately: a Deplete patch that DOES overdraw, staffed 4 where only 2 are needed.
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _standing_forage_band_fixture()
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("tile_panel_standing")

	# tile_panel_land_sticky — THE BEHAVIOURAL ASSERTION for the sticky land selection, driven
	# through the REAL client path, because the bug does not live where a hand-picked
	# `reapply_selection("tile", …)` would put it. MapView holds its OWN occupant selection, and
	# `refresh_selection_payload` answers `kind: "unit"` for as long as `selected_unit_id >= 0` — so on
	# an OCCUPIED hex the tile branch is never even reached. Hence: instance the real MapView, wire the
	# two signals Main wires, click the hex, click the LAND row, then ASK MAPVIEW what the next
	# snapshot's payload is and feed whatever it says into `reapply_selection`. Hardcoding "tile" here
	# would assert a path the bug cannot reach.
	var sticky_map: Node2D = MAP_VIEW_SCRIPT.new()
	# Data only — a visible map would render behind the HUD in this and every later frame.
	sticky_map.visible = false
	add_child(sticky_map)
	# FoW OFF, stated explicitly — this assertion DIES SILENTLY without it. `_fow_enabled` defaults
	# to `true` (it fails closed for the live client), which fog-gates every band and herd out of
	# `_tile_info_at` / `_units_on_tile` at source: the crowded hex reads "Unexplored / Unknown" and
	# both asserts below pass VACUOUSLY, with no occupant left to fail to stick to. The guard must
	# see the occupants it was written to guard.
	sticky_map.set_fow_enabled(false)
	sticky_map.display_snapshot(_sticky_map_snapshot())
	# Main's wiring, verbatim (Main._on_map_tile_selected / _on_map_unit_selected /
	# _on_hud_roster_occupant_selected).
	sticky_map.tile_selected.connect(_hud.show_tile_selection)
	sticky_map.unit_selected.connect(_hud.show_unit_selection)
	_hud.roster_occupant_selected.connect(sticky_map.select_occupant)
	sticky_map.handle_hex_click(STICKY_TILE.x, STICKY_TILE.y, MOUSE_BUTTON_LEFT)  # lands on a band
	_hud._selectioncard._on_land_row_selected()                                   # the player picks LAND
	# The next snapshot: Main asks MapView what is selected and replays it into the HUD.
	var sticky_payload: Dictionary = sticky_map.refresh_selection_payload()
	_hud.reapply_selection(String(sticky_payload.get("kind", "none")), sticky_payload.get("data", {}))
	await _settle()
	_assert_hud("land row clears MapView's occupant selection (payload is not \"unit\")",
		String(sticky_payload.get("kind", "")) != "unit")
	_assert_hud("land selection survives the next snapshot on a crowded hex",
		_hud._selection._selected_subject == "land" and _hud._selection._selected_unit.is_empty() and _hud._selection._selected_herd.is_empty())
	await _save("tile_panel_land_sticky")
	sticky_map.tile_selected.disconnect(_hud.show_tile_selection)
	sticky_map.unit_selected.disconnect(_hud.show_unit_selection)
	_hud.roster_occupant_selected.disconnect(sticky_map.select_occupant)
	sticky_map.queue_free()
	await get_tree().process_frame

	# tile_panel_deselect_keeps_tile — THE BEHAVIOURAL ASSERTION for issue #405: clicking an EMPTY hex
	# while a herd is selected must leave that hex SELECTED, not selectionless. A PNG cannot carry this
	# claim — "no selection outline" and "an outline drawn under an overlay" look identical — so it is
	# asserted on state, through the real click path, the `tile_panel_land_sticky` idiom above: a real
	# MapView, Main's signal wiring, real `handle_hex_click` calls. `_handle_entity_selection`'s clear
	# branch only arms once an occupant is selected, so the first click (the herd) is what makes the
	# second click able to fail; asserting on the empty click alone would pass vacuously.
	var deselect_map: Node2D = MAP_VIEW_SCRIPT.new()
	deselect_map.visible = false   # data only — a visible map renders behind the HUD in every later frame
	add_child(deselect_map)
	# FoW OFF, stated explicitly (the harness rule): `_fow_enabled` fails closed to `true`, and a
	# fog-gated herd would never be selected by the first click, leaving nothing to deselect.
	deselect_map.set_fow_enabled(false)
	deselect_map.display_snapshot(_deselect_map_snapshot())
	# Main's wiring for this path, verbatim (Main._on_map_tile_selected / _on_map_herd_selected /
	# _on_map_selection_cleared).
	deselect_map.tile_selected.connect(_hud.show_tile_selection)
	deselect_map.herd_selected.connect(_hud.show_herd_selection)
	deselect_map.selection_cleared.connect(_hud.clear_selection)
	deselect_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	_assert_hud("clicking a herd selects the herd AND its tile",
		deselect_map.selected_herd_id != "" and deselect_map.selected_tile == DESELECT_HERD_TILE)
	deselect_map.handle_hex_click(DESELECT_LAND_TILE.x, DESELECT_LAND_TILE.y, MOUSE_BUTTON_LEFT)
	var deselect_payload: Dictionary = deselect_map.refresh_selection_payload()
	await _settle()
	_assert_hud("deselecting a herd on an empty hex KEEPS that hex selected (#405)",
		deselect_map.selected_tile == DESELECT_LAND_TILE)
	_assert_hud("deselecting a herd clears the OCCUPANT selection",
		deselect_map.selected_herd_id == "" and deselect_map.selected_unit_id == -1)
	_assert_hud("the deselected hex falls back to its land card",
		String(deselect_payload.get("kind", "")) == "tile")
	await _save("tile_panel_deselect_keeps_tile")
	deselect_map.tile_selected.disconnect(_hud.show_tile_selection)
	deselect_map.herd_selected.disconnect(_hud.show_herd_selection)
	deselect_map.selection_cleared.disconnect(_hud.clear_selection)
	deselect_map.queue_free()
	await get_tree().process_frame

	# tile_panel_unseen — a REMEMBERED hex. Chips + the land row render (geography is remembered
	# knowledge), the herd this fixture deliberately carries does NOT, and the drawer states that
	# the contents are unknown. An empty list would be a claim of emptiness we cannot back up.
	_hud.clear_selection()
	_hud.show_tile_selection(_sight_tile_fixture(VIS_DISCOVERED))
	await _settle()
	await _save("tile_panel_unseen")

	# tile_panel_band — a PLAYER band lit while the dockable Band/City panel exists: its detail
	# renders there, so the drawer would otherwise be a blank gap. It must point at where the
	# detail went instead. (The panel is injected only for this frame and released after, so the
	# reserved edge does not follow the states below.)
	var tile_panel_band_panel: BandCityPanel = BAND_CITY_PANEL_SCENE.instantiate()
	add_child(tile_panel_band_panel)
	# Fan the panel's reservation onto the HUD as Main does, and dock it RIGHT — docked left it
	# reserves the very edge the selection card lives on and covers the frame under test.
	tile_panel_band_panel.reservation_changed.connect(func(edge: int, size: float):
		_hud.set_reserved_inset(&"band_panel", edge, size))
	tile_panel_band_panel.set_dock(SIDE_RIGHT)
	# The panel's narrow shell shows ONE zone, and its prefs are a fresh profile (see the isolation
	# block in `_ready`), so it opens on `DEFAULT_TAB` = work. This frame is about where the band
	# DETAIL went, so ask for the band zone — the same rule `band_panel_preview` carries. It used to
	# come up on `band` only because a previous run had written that tab into the PLAYER's prefs file.
	tile_panel_band_panel.set_active_tab(BandCityPanel.ZONE_BAND)
	_hud.set_band_city_panel(tile_panel_band_panel)
	# THREE player bands on this hex, and the faction default is the FIRST one — so "the band the
	# list has selected" and "the faction's default band" are DIFFERENT answers, which is the only
	# configuration in which the Move assertion below can fail (§18).
	var tile_panel_band_roster: Array = _crowded_bands_fixture()
	_hud._band_labor._player_bands = tile_panel_band_roster
	_hud._band_labor._player_band = tile_panel_band_roster[0]
	var tile_panel_band_subject: Dictionary = tile_panel_band_roster[0]
	tile_panel_band_subject["tile_info"] = _crowded_tile_fixture()
	_hud.show_unit_selection(tile_panel_band_subject)
	# The player then picks the SECOND band, through the real subject-list selection path.
	_hud._selectioncard.select_roster_occupant("unit", TILE_PANEL_MOVE_BAND_ENTITY)
	await _settle()
	await _save("tile_panel_band")

	# THE MOVE ASSERTION (§18). Driven through the drawer's REAL button — calling
	# `_targeting.begin_move_band` directly would assert the resolver, not the wiring — and the pending
	# move must name the band SELECTED IN THE LIST (302), never the faction default
	# (`_player_band`, 301), which is what a naive wiring resolves to on a crowded hex.
	var tile_panel_move_btn: Button = _find_button_by_text(_hud.allocation_panel, MOVE_BUTTON_TEXT)
	_assert_hud("the player-band drawer offers Move", tile_panel_move_btn != null)
	if tile_panel_move_btn != null:
		tile_panel_move_btn.emit_signal("pressed")
	await _settle()
	_assert_hud("Move enters move-band targeting", _hud.is_targeting_active())
	_assert_hud("…targeting the band SELECTED IN THE LIST, not the faction default",
		int(_hud._targeting._pending_move_band.get("entity", -1)) == TILE_PANEL_MOVE_BAND_ENTITY)
	_hud.cancel_active_targeting()
	await _settle()
	_hud.set_band_city_panel(null)
	_hud.set_reserved_inset(&"band_panel", SIDE_RIGHT, 0.0)
	tile_panel_band_panel.queue_free()
	await get_tree().process_frame

	# tile_panel_feed_shown — the command feed is hidden by default now (six read-only receipts, no
	# verbs) and opens on `R`. Toggled on, the dock must reflow with the selection card above it and
	# nothing clipped.
	_hud.ingest_command_events(_telling_command_receipts())
	_hud.show_tile_selection(_food_tile_fixture())
	_hud.toggle_command_feed()
	await _settle()
	await _save("tile_panel_feed_shown")
	_hud.toggle_command_feed()
	await _settle()

	# Restore the single-band compose context the states below assume.
	_hud._band_labor._player_bands = []
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud._compose.set_hunt_band(-1)
	_hud._compose.set_hunt_policy("sustain")
	_hud._compose.reset_forage_source()
	_hud._compose.set_forage_band(-1)

	# State 4 — targeting active: pressing "Move" on the band allocation panel enters
	# tile-targeting, raising the top-centre banner ("MOVE … click a destination tile").
	_hud.show_unit_selection(_band_fixture())
	_hud._targeting.begin_move_band()
	await _settle()
	await _save("targeting_banner")
	_hud.cancel_active_targeting()

	# The old states 4a–4c — the pre-launch raid forecast hanging off the TARGETING BANNER — are
	# gone with the mechanism. They existed because the herd was only known at the targeting step; the
	# band-panel launch flow now picks the quarry FIRST, inside the compose sheet, so the forecast
	# lives in the form with the real party size and policy (band_panel_preview `band_panel_compose_hunt`).

	# State 5 — quick-hunt convenience (map double-click a herd): with idle workers it
	# assigns them to hunt; with none it posts a command-feed note instead of silently
	# no-opping. Seed a fully-staffed band (0 idle) so the note renders in the Command Feed.
	var staffed_band := _band_fixture()
	staffed_band["idle_workers"] = 0
	_hud._band_labor._player_band = staffed_band
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
		{"faction": 0, "entity": 501, "size": 40, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 0},
	])
	await _settle()
	await _save("turn_orb_clear")

	# State 6a-fit — THE TURN NUMBER IS ON THE FACE, and its type size is MEASURED, not tabled
	# (`TurnOrb._turn_font_size`: step down from `TURN_FONT_SIZE_MAX` until the string fits
	# `FACE_DIAMETER * TURN_TEXT_WIDTH_FRACTION`, floored at `TURN_FONT_SIZE_MIN`). Walk 1 → 47 → 999 →
	# 1200 and assert, for each, that the rendered string is the number, that the chosen size is inside
	# the declared band, and — the point of the fit — that it actually FITS the usable chord. A 4-digit
	# turn is the case that would otherwise overflow the circle; `turn_orb_turn_4digit` is its frame.
	for probe_turn in [1, 47, 999, TURN_ORB_FOUR_DIGIT_TURN]:
		_hud.update_overlay(probe_turn, {})
		await _settle()
		_assert_turn_face_fits(probe_turn)
	# The curved `TURN` word above the number rides the same face. Its geometry is number-independent, so
	# one arithmetic check covers every probe; the 4-digit frame saved above is where the CLEARANCE between
	# the word and the widest number is judged by eye, at true size.
	_assert_turn_word_clears()
	await _save("turn_orb_turn_4digit")
	# Back to the state the following orb states describe.
	_hud.update_overlay(42, {})
	await _settle()

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
		{"faction": 0, "entity": 511, "size": 40, "turns_of_food": 999.0, "activity": "forage",
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
		{"faction": 0, "entity": 601, "size": 120, "turns_of_food": 12.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		{"faction": 0, "entity": 602, "size": 90, "turns_of_food": 999.0, "activity": "hunt",
			"current_x": 31, "current_y": 21},
		{"faction": 0, "entity": 603, "size": 60, "turns_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9},
	])
	_hud.update_band_alerts([
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
		{"faction": 0, "entity": 801, "size": 46, "turns_of_food": 1.0, "activity": "hunt",
			"current_x": 64, "current_y": 11, "idle_workers": 0,
			"labor_assignments": [
				# BOTH PRODUCTS (issue #337): the hide sells beside the meat, so the drawer's standing
				# summary must read `+0.84 /turn · ⇄ +0.12` — food leading, trade shown only because it
				# is non-zero. Same `SourceForecast.source_yield_readout` the Band panel's rows use.
				{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "policy": "corral",
					"target_x": 66, "target_y": 10, "actual_yield": 0.84, "sustainable_yield": 0.84,
					"trade_yield": 0.12, "realized_trade_yield": 0.12},
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
	# Opened here and closed at the end of THIS block (not hundreds of lines later).
	_open_legend()
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
	_close_legend()

	# ---- The Telling (docs/plan_the_telling.md) -----------------------------------------------
	# The narrative fork decision surface + the client-side end-turn gate. The fixture is the REAL
	# authored copy from core_sim/src/data/beat_definitions.json (`sedentarization.soft_drift`, the
	# `soft_drift.long_chase` wardrobe entry, nouns resolved as the sim resolves them at post time),
	# so the frame shows prose at real length rather than lorem that flatters the layout.
	_hud.clear_selection()
	_hud.update_overlay(41, {})
	# Pin the register so the run is deterministic (the preference persists in user://).
	NarrativeForkPanel.save_voice_register(FORK_REGISTER_MYTHIC)

	# State F1 — the panel, auto-opened the first time the fork appears: the narration as the hero
	# element, three choices in catalog order (the defer choice styled `ghost`, and ALWAYS enabled —
	# it is the out the gate depends on), the gloss collapsed, the voice toggle in the footer.
	_hud.update_pending_forks(_pending_forks_fixture())
	_hud.update_stance_axes(_stance_axes_fixture())
	await _settle()
	await _save("narrative_fork_panel")

	# State F2 — the SAME fork in the other register. Verifies the toggle and that the noticeably
	# shorter/looser `warm` copy lays out as well as the long `mythic` one. The registers come from
	# the fork itself, never a hardcoded list.
	_hud._turnorb._fork_panel._on_register_picked(FORK_REGISTER_WARM)
	await _settle()
	await _save("narrative_fork_panel_warm")

	# State F3 — THE GATE, and the single most important assertion in this file. With a blocking
	# fork seeded, an orb-face click must NOT advance the turn (it opens the reasons popover
	# instead), and the popover's Advance button must be DISABLED and wear the reason. This is the
	# exact inverse of `turn_orb_clear_click_advances`.
	_hud._turnorb._fork_panel.close()
	NarrativeForkPanel.save_voice_register(FORK_REGISTER_MYTHIC)
	var fork_advance_hits := [0]
	var fork_advance_cb := func() -> void: fork_advance_hits[0] += 1
	_hud.turn_orb.advance_requested.connect(fork_advance_cb)
	_hud.turn_orb._on_face_pressed()
	await _settle()
	var fork_footer := _turn_orb_advance_button()
	_assert_turn_orb("blocking fork: face click does not advance",
		fork_advance_hits[0] == 0 and _hud.turn_orb._popover_open)
	_assert_turn_orb("blocking fork: Advance is disabled",
		fork_footer != null and fork_footer.disabled)
	await _save("turn_orb_fork_blocks")
	_hud.turn_orb.advance_requested.disconnect(fork_advance_cb)
	_hud.turn_orb.toggle_popover()

	# (The old State F4 `narrative_feed` — narrative prose styled INSIDE the command feed — was
	# retired with PR-C. The feed no longer renders narrative kinds at all, so the state could only
	# ever have shown their absence; `telling_and_feed` below is its replacement and tests the
	# thing that now matters: that the receipts survive alongside real narrative volume.)

	# ---- The Telling panel (PR-C) ------------------------------------------------------------
	# States G1–G3. The dock is cleared first so the two narrative cards are judged on their own
	# chrome rather than on whatever the previous state left selected.
	# `clear_selection()` deliberately KEEPS the tile card (deselecting an occupant should not
	# forget the hex), so the tile info has to go first or the Tile card fills the dock and both
	# narrative cards get squeezed out of the frame entirely.
	_hud._selection._selected_tile_info.clear()
	_hud.clear_selection()
	_hud.reset_command_feed()
	_hud._telling.reset()

	# G1 — ORAL: the current utterance only. No page furniture, no leaf controls, no page number — oral
	# memory does not keep the previous telling, so the visible page is pinned to the NEWEST beat (the
	# fork at tick 22). Ingest the real authored copy (incl. the catalog's longest line, so a page's
	# wrap is genuinely exercised).
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events(_telling_fixture_events())
	await _settle()
	await _save("telling_panel_oral")

	# G2 — PAINTED: the accumulating wall. The SAME entries, now retained as pages you can walk FORWARD
	# through (a marks + position cue, no back control). Parked mid-way (page 3/6) so the retained
	# earlier pages and the forward-only affordance read at once. `debug_jump_to` is the NON-animating
	# park — these SETTLED end-state frames must not catch a page-turn tween mid-flight (that's what the
	# `telling_turn_*_mid` states capture on purpose).
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_PAINTED, "medium_index": 1}])
	_hud._telling.debug_jump_to(2)
	await _settle()
	await _save("telling_panel_painted")

	# G3 — WRITTEN: the full book. Page number + ‹ › leaf controls, parked on a NON-LAST page (3/6) so
	# backward leafing is visibly available (both ‹ and › active). Nothing about the copy changes
	# between the rungs (per-medium copy is a deliberate non-goal) — only the title, accent and
	# CAPABILITIES age, which is the whole point.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	_hud._telling.debug_jump_to(2)
	await _settle()
	await _save("telling_panel_written")

	# G3b — UNREAD: the yields-to-reader rule. The reader is held on an OLD page (1/6) while newer pages
	# exist; the page never turns on its own, so a subtle "a new telling waits" cue appears instead of
	# yanking them forward. (Advancing the turn — reveal_newest() — is what catches them up.)
	_hud._telling.debug_jump_to(0)
	await _settle()
	await _save("telling_panel_unread")

	# G4 — THE FRAME THAT PROVES THE SPLIT WORKED. The Telling panel holds its fixed page while the
	# command feed carries ordinary receipts: before the split, two beats filled the feed card outright
	# and pushed every receipt off screen. The receipts must be READABLE here. (Oral restored.)
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events(_telling_command_receipts())
	await _settle()
	await _save("telling_and_feed")

	# G5 — THE DEFAULT DOCK LAYOUT. The right dock holds the Telling panel ALONE: Victory and
	# Terrain Types both ship suppressed, so the narrative surface gets the full right-dock height
	# instead of the squeezed share it had while it lived under the left dock's selection cards.
	# The command feed stays on the left, which is the layout this frame exists to show.
	_hud.update_victory_state(_victory_state_fixture())
	await _settle()
	await _save("dock_default_layout")
	# The Telling panel is registered with `right_dock.add(..., 10)`, and `PanelDock._reorder`
	# reparents. Screenshotting the dock only shows it LOOKS right; assert WHERE it lives, so a
	# dropped/reordered registration (or a scene edit that re-authors it under the left dock)
	# fails here instead of silently reverting the narrative surface to the left column.
	_assert_hud("default layout: Telling panel lives in the right dock stack",
		_hud.telling_panel.get_parent() == _hud.right_stack)

	# G6 — the same frame with BOTH reference cards toggled back on (the `V` / `L` path), so the
	# right dock's stacking order — Telling, then Victory, then Terrain Types — is visible and the
	# Telling panel is seen to yield height rather than overlap.
	# Victory goes through the REAL `toggle_victory` (the `V` path, prefs write included — the harness
	# cleared the section at startup, and this toggles back below); the legend uses the harness helper.
	_hud.toggle_victory()
	_open_legend()
	_hud.update_overlay_legend(_terrain_legend_fixture())
	await _settle()
	await _save("dock_panels_revealed")
	_assert_hud("toggled on: Terrain Types legend is visible", _hud.terrain_legend_panel.visible)
	_assert_hud("toggled on: Victory panel is visible", _hud.victory_panel.visible)
	# Restore the shipped default so any later state renders the real layout.
	_hud.toggle_victory()
	_close_legend()

	# TWO-BEAT ORAL — a single speaking turn firing TWO beats (both sharing one tick, so they are ONE
	# page). The page must GROW to fit both beats + gloss with NO scrollbar — the playtest fix (the
	# strictly-fixed height scrolled the second beat out of view). Assert the inner scroll is not engaged.
	_hud._telling.reset()
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events(_telling_two_beat_oral_fixture())
	await _settle()
	_assert_hud("two-beat oral page grows to fit both beats with no scrollbar",
		not _hud._telling.debug_page_scrolls())
	await _save("telling_panel_oral_two_beats")

	# SCROLL YIELDS-TO-READER — a beyond-cap (scrolling) page must NOT yank a mid-page reader to the top on
	# an IDEMPOTENT static repaint (a retaining-medium beat arrival that leaves the visible page unmoved),
	# but MUST start at the top on a real page turn. Two tall written pages that both overflow the cap.
	_hud._telling.reset()
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	_hud.ingest_command_events(_telling_tall_pages_fixture())
	_hud._telling.debug_jump_to(0)
	await _settle()
	var telling_scroll: ScrollContainer = _hud._telling._scroll
	telling_scroll.scroll_vertical = 40   # the reader has scrolled down the tall page
	await _settle()
	_assert_hud("tall page overflows so the reader's scroll offset holds", telling_scroll.scroll_vertical == 40)
	# Idempotent repaint: a new beat arrives on a NEW tick, but written stays on page 0 (index clamped, the
	# visible page's text is unchanged) — the yields case. Must PRESERVE the reader's scroll position.
	_hud.ingest_command_events([{"tick": 2, "kind": "narrative_beat", "label": "A far-off new telling waits.", "detail": "later"}])
	_assert_hud("idempotent repaint of the same page preserves the reader's scroll position",
		telling_scroll.scroll_vertical == 40)
	# A real page turn resets the inner scroll to the top of the new page.
	_hud._telling.leaf(1)
	_assert_hud("a real page turn resets the inner scroll to the top", telling_scroll.scroll_vertical == 0)
	_hud._telling.debug_end_turn()

	# LIVE-PATH ORAL ARRIVAL — the REAL trigger, no debug hook. Drive the actual per-snapshot Hud entry
	# points (`update_voice_medium` THEN `ingest_command_events`, plus the `_refit_right_dock` a real
	# snapshot fires) with a genuinely new beat, and PROVE a running tween is created AND survives to paint
	# frames (an idempotent re-render / refit in the same cycle must not `_kill_tween` it). This is the gap
	# the mid-transition freeze states could not cover: they show the tween CAN render, not that the live
	# beat-arrival path TRIGGERS one.
	_hud._telling.reset()
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events([{"tick": 0, "kind": "narrative_beat",
		"label": "The scouts came back thinner and louder than they left, all of them saying one word: Salt Pillar Reach.",
		"detail": "sites.discovered_this_turn = 1"}])
	await _settle()   # initial population — no animation by design
	# A new snapshot: medium re-pushed unchanged (must NOT clobber), then a genuinely new beat arrives.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud.ingest_command_events([{"tick": 5, "kind": "narrative_beat",
		"label": "The portions grew smaller without anyone deciding it. That is how it always begins.",
		"detail": "provisions.total falling for 3 turns"}])
	_hud._refit_right_dock()   # a refit in the same cycle must not kill the in-flight turn tween
	_assert_hud("live oral beat-arrival creates a running page-turn tween",
		_hud._telling.debug_turn_active())
	# Advance the REAL tween to a CHOSEN mid-motion phase. Animation time is frozen (see `_ready`), so
	# awaiting frames here would advance it by exactly nothing and the state would capture the page
	# BEFORE the turn — the one frame in the run whose subject the freeze could have erased. One
	# `custom_step` of 40% of the oral dissolve keeps it genuinely in flight AND makes the phase a
	# decision instead of whatever the clock handed us (which is what made this frame drift).
	_step_tweens(TellingPanel.PAGE_TURN_DURATION_ORAL * TELLING_LIVE_TURN_FRACTION)
	_assert_hud("live oral tween survives an in-cycle refit and is still running mid-motion",
		_hud._telling.debug_turn_active())
	# The one `_settle` that must NOT flush tweens: this frame IS the mid-turn render.
	await _settle(false)
	await _save("telling_live_oral_arrival")
	_hud._telling.debug_end_turn()   # settle deterministically before the next state

	# ---- Page-turn animation: motion matures with the medium (mid-transition capture) --------------
	# The harness dumps single frames, so each state DRIVES a page turn, then FREEZES the tween at its
	# midpoint (`debug_freeze_turn_at`) so the outgoing and incoming pages COEXIST in the captured PNG —
	# proof the motion is real. Setup jumps (`debug_jump_to`) are non-animating so the measured turn
	# starts from a clean resting page. The block ends with a clean static render, so the frozen overlay
	# never leaks into a later frame.
	_hud._telling.reset()
	_hud.ingest_command_events(_telling_fixture_events())

	# WRITTEN — a horizontal SLIDE, forward: the outgoing page exits left as the incoming enters from the
	# right. Frozen mid-slide, both pages are onscreen offset horizontally, with the ‹ › book furniture.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	_hud._telling.debug_jump_to(1)
	await _settle()
	_hud._telling.leaf(1)
	_hud._telling.debug_freeze_turn_at(0.5)
	await _settle()
	await _save("telling_turn_written_mid")

	# PAINTED — the incoming page RISES from just below with a fade (new marks drifting onto the wall).
	# Frozen partway up, the incoming page sits low + faint over the fading outgoing one.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_PAINTED, "medium_index": 1}])
	_hud._telling.debug_jump_to(1)
	await _settle()
	_hud._telling.leaf(1)
	_hud._telling.debug_freeze_turn_at(0.45)
	await _settle()
	await _save("telling_turn_painted_mid")

	# ORAL — a CROSSFADE in place: a new recitation replacing the last (oral keeps no prior page). Frozen
	# at the crossover, both pages read at partial alpha in the same spot, with NO furniture.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud._telling.debug_jump_to(3)
	await _settle()
	_hud._telling.reveal_newest()
	_hud._telling.debug_freeze_turn_at(0.5)
	await _settle()
	await _save("telling_turn_oral_mid")

	# INTERRUPTION — a rapid second turn must KILL the running tween and settle to the CORRECT final page,
	# with no leftover overlay/offset. Turn 0→1, immediately 1→2, then force the settle a completed tween
	# would reach, and assert the visible page is 2 with the overlay gone.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_WRITTEN, "medium_index": 2}])
	_hud._telling.debug_jump_to(0)
	await _settle()
	_hud._telling.leaf(1)          # 0 → 1 (tween begins)
	_hud._telling.leaf(1)          # 1 → 2 immediately (must kill + restart)
	_hud._telling.debug_end_turn() # force the settle
	await _settle()
	_assert_hud("interrupted page-turn settles to the final page with no leftover overlay",
		_hud._telling.debug_visible_index() == 2 and not _hud._telling.debug_overlay_visible())
	await _save("telling_turn_interrupted")

	# Clean static state (newest oral page, no frozen overlay) before the downstream frames.
	_hud.update_voice_medium([{"faction": 0, "medium_id": TELLING_MEDIUM_ORAL, "medium_index": 0}])
	_hud._telling.reveal_newest()
	await _settle()

	# ---- Hunt/husbandry render-honesty pass (intensification ladder client UX) ----------------------
	# Fix #1 + #5 — CURRENT ACTIONS rows: a summary row headlines the honest per-turn FOOD rate
	# (sustainable, not the 0.00 pulse) + the policy/status glyphs, with NO `≈… /turn` animals-per-turn
	# cadence (that lives on the compose-preview line). Both rows must read `Hunt <species> +X /turn ♻ ●`;
	# the big-game (under-crewed) row also keeps its muted "· 1.9 wasted" note (yld.muted_note, not cadence).
	_hud.update_herds(_hunt_rhythm_herds_fixture())
	_hud.show_unit_selection(_hunt_actions_band_fixture())
	await _settle()
	await _save("hunt_actions_rhythm")
	_hud.update_herds(_world_herds_fixture())

	# Fix #2 + #1(forecast) + #6 — the LOCAL hunt compose view: the policy picker shows each rung's
	# per-turn take so Sustain < Surplus < Deplete < Eradicate reads as ASCENDING, and the live preview
	# pairs its rate with the kill-rhythm. (The stepper on a WILD herd reads "Hunters".)
	# A compact NON-food tile so the herd drawer (not a full forage tile card) lands in-frame.
	var picker_herd := _herd_fixture()
	picker_herd["tile_info"] = _compact_herd_tile_fixture()
	_hud._band_labor._player_band = _band_fixture()
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(picker_herd)
	_compose_herd(picker_herd, 3, "sustain")
	await _settle()
	await _save("hunt_picker_ascending")

	# Fix #6 — a MANAGED (corralled) herd's local crew are HERDERS, not a hunt party: the stepper reads
	# "Herders" so a pen whose workersNeeded scales with the herd doesn't look like a hunt-party bug.
	_hud._compose.reset_hunt_source()
	_hud.show_herd_selection(_domesticated_herd_fixture())
	_compose_herd(_domesticated_herd_fixture())
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

	# STALE-CLOSURE GUARD (herd) — the drawer diff-cache patches a same-SHAPE restate in place and
	# DELIBERATELY keeps the compose-open button's `pressed` closure intact. Before the fix
	# `_herd_actions_shape` omitted the herd id, so switching to a DIFFERENT herd of identical structure
	# took the PATCH path and left "Assign hunters ▸" opening the PREVIOUS herd's compose (playtest: the
	# rabbit's button opened the boar's Tame sheet). Two wild huntable herds share the "assign-button, no
	# summary" shape, so the buggy patch path fires; pressing the button must open herd B's compose, not A's.
	_hud._band_labor._player_band = _band_fixture()
	_hud._band_labor._player_bands = []
	_hud._compose.reset_hunt_source()
	var stale_herd_a := _wild_herd_fixture()
	var stale_herd_b := _wild_herd_fixture()
	stale_herd_b["id"] = "game_deer_stale_99"
	stale_herd_b["species"] = "Roe Deer"
	stale_herd_b["label"] = "Roe Deer (game_deer_stale_99)"
	# Drive the REAL drawer-actions path (`refresh_drawer_actions` calls these), settling a frame between
	# each so the diff-cache's deferred `queue_free` completes: without the settles, stale buttons linger
	# in-tree, the child-count patch test misreads, and `_find_button_by_text` grabs the wrong node.
	_hud._drawercompose._clear_herd_drawer()   # drop any prior-state button so A gets a FRESH closure
	await _settle()
	_hud._drawercompose.build_herd_drawer_actions(stale_herd_a)   # full rebuild → button opens A
	await _settle()
	_hud._drawercompose.build_herd_drawer_actions(stale_herd_b)   # same shape → the patch path under test
	await _settle()
	var stale_herd_btn := _find_button_by_text(
		_hud.herd_assign_controls, HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.HUNT_CREW_LABEL.to_lower())
	assert(stale_herd_btn != null)
	stale_herd_btn.pressed.emit()
	await _settle()
	await _save("herd_assign_button_targets_selected_herd")
	# The opened compose must be herd B (the herd now shown), never the herd A it was first wired against.
	assert(_hud._compose.kind() == ComposeState.KIND_HERD)
	assert(_hud._compose.subject() == String(stale_herd_b["id"]))
	_hud._drawercompose.close_compose_sheet()
	_hud._compose.reset_hunt_source()

	# STALE-CLOSURE GUARD (forage) — the identical diff-cache pattern on the forage drawer. Before the fix
	# the forage-actions shape omitted the tile subject key, so switching between two food tiles of the same
	# shape kept "Assign foragers ▸" opening the PREVIOUS tile's forage compose. Same drive, other drawer.
	_hud._compose.reset_forage_source()
	var stale_tile_a := _food_tile_fixture()
	var stale_tile_b := _food_tile_fixture()
	stale_tile_b["x"] = 70
	stale_tile_b["y"] = 20
	_hud._drawercompose._clear_forage_drawer()   # drop any prior-state button so tile A gets a FRESH closure
	await _settle()
	_hud._drawercompose.build_forage_drawer_actions(stale_tile_a)   # full rebuild → button opens tile A
	await _settle()
	_hud._drawercompose.build_forage_drawer_actions(stale_tile_b)   # same shape → the patch path under test
	await _settle()
	var stale_forage_btn := _find_button_by_text(
		_hud.forage_assign_controls, HudComposeVocab.COMPOSE_OPEN_BUTTON_FORMAT % HudComposeVocab.FORAGE_CREW_LABEL.to_lower())
	assert(stale_forage_btn != null)
	stale_forage_btn.pressed.emit()
	await _settle()
	await _save("forage_assign_button_targets_selected_tile")
	# The opened compose must be tile B (subject key "70,20"), never tile A ("66,10") it was first wired to.
	assert(_hud._compose.kind() == ComposeState.KIND_FORAGE)
	assert(_hud._compose.subject() == "70,20")
	_hud._drawercompose.close_compose_sheet()
	_hud._compose.reset_forage_source()

	# WORLD-BOUNDARY GUARD — `Hud.reset_world_state()`, the HUD half of the stale-world fix.
	# A freshly generated world sends `intensification_knowledge: []`, which MERGES to nothing, so the
	# previous game's "⚒ Your people know" strip survived into the new one; the Telling panel is
	# deliberately never reset per snapshot, so its old beats stayed in the book. Seed BOTH the way a
	# played-out world leaves them, then reset and assert they are gone. A PNG alone would not prove
	# this — a hidden strip and a strip that was never seeded look identical — so the frame is captured
	# for the eye and the two assertions carry the claim.
	_hud._selection._selected_tile_info.clear()
	_hud.clear_selection()
	_hud.update_intensification([{"faction": 0, "cultivation": 0.55, "herding": 1.0}])
	_hud._telling.reset()
	_hud.ingest_command_events(_telling_fixture_events())
	await _settle()
	_assert_hud("world-boundary guard seeds a knowledge strip to clear",
		_hud._topbar.intensification_label.visible)
	_assert_hud("world-boundary guard seeds a telling to clear",
		not _hud._telling._entries.is_empty())
	_hud.reset_world_state()
	await _settle()
	_assert_hud("world reset hides the knowledge strip (a new world knows nothing)",
		not _hud._topbar.intensification_label.visible)
	_assert_hud("world reset empties the Telling (a new world is a different story)",
		_hud._telling._entries.is_empty())
	await _save("world_reset")

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

	# The herd field-pair guard's verdict, ONE line for the whole run (each violation has already been
	# push_error'd against the frame it rendered in). The scanned count is part of the claim: a guard
	# that walked nothing would pass vacuously, and "0 herd dicts scanned" says so out loud.
	_assert_hud("every herd fixture keeps the herders_needed pair consistent (%d herd dicts carrying it)"
		% _herd_pair_scans, _herd_pair_violations == 0)

	get_tree().quit()

## Victory progress shaped as `Hud._refresh_victory_status` consumes it: no winner declared yet and
## a few modes at differing progress, so the card has real height when it is toggled on and the
## progress sort (highest first) is visible.
func _victory_state_fixture() -> Dictionary:
	return {
		"winner": {},
		"modes": [
			{"id": "cultural_ascendancy", "progress_pct": 0.42, "achieved": false},
			{"id": "great_works", "progress_pct": 0.18, "achieved": false},
			{"id": "hegemony", "progress_pct": 0.06, "achieved": false},
		],
	}

## Open / close the Terrain Types legend around a block of legend states.
##
## The card ships SUPPRESSED, so a legend state must open it — and every legend state MUST close it
## again at the end of its own block. An earlier cut opened it once and restored it ~700 lines later,
## which meant a dozen intervening states silently rendered with a non-default right dock and NO
## state anywhere exercised the shipped default. That is precisely how a default-visibility bug
## hides, so scope stays tight and local.
##
## Set through the controller rather than `Hud.toggle_legend`, which would PERSIST the choice to the
## prefs file this harness clears at startup — a harness must not write the preference it is testing.
func _open_legend() -> void:
	_hud._legend.set_suppressed(false)

func _close_legend() -> void:
	_hud._legend.set_suppressed(true)

## Six narrative beats in the `mythic` register, transcribed VERBATIM from the authored copy in
## `core_sim/src/data/beat_definitions.json` with their nouns filled in as the sim would fill them.
## Real copy, not lorem: the panel's whole job is prose, and placeholder text of the wrong length
## would make both the wrapping and the density read wrong.
##
## The first entry is `cold_open.bone_ground` — the LONGEST line in the catalog (225 chars) — so
## the multi-line wrap case is exercised in every telling frame rather than by luck.
func _telling_fixture_events() -> Array:
	return [
		{"tick": 0, "kind": "narrative_beat",
			"label": "We are 24. The ground behind us is bone, and we will not go back to it. Ahead lies a country with no names — not the hills, not the waters, not the years to come. Naming it is your work now. Walk well, and be remembered.",
			"detail": "turn.index = 0 · band.count = 24"},
		{"tick": 3, "kind": "narrative_beat",
			"label": "The scouts came back thinner and louder than they left. Salt Pillar Reach, they said, over and over, until we all knew the word.",
			"detail": "sites.discovered_this_turn = 1"},
		{"tick": 9, "kind": "narrative_beat",
			"label": "The portions grew smaller without anyone deciding it. That is how it always begins.",
			"detail": "provisions.total falling for 3 turns"},
		{"tick": 14, "kind": "narrative_beat",
			"label": "A woman pressed seed into the mud to see what it would do. The mud answered. We know a new thing.",
			"detail": "knowledge.cultivation = 1.00"},
		{"tick": 18, "kind": "narrative_beat",
			"label": "The chase is longer every season and ends in less. The aurochs were the road we walked; the road is going quiet under us.",
			"detail": "herd.ecology_phase = collapsing"},
		{"tick": 22, "kind": "narrative_fork",
			"label": "There are paths here now, worn by our own feet, going to places only we go. That is how a country becomes a home, or a trap.",
			"detail": "sedentarization.score = 41"},
	]

## TWO beats sharing ONE tick — a single speaking turn that said two things, so they form ONE page.
## Reproduces the playtest bug (the fixed-height page scrolled the second beat off instead of growing).
func _telling_two_beat_oral_fixture() -> Array:
	return [
		{"tick": 6, "kind": "narrative_beat",
			"label": "We have stopped catching rabbits and started keeping them. A fence, a little grass, and they breed under our own eyes.",
			"detail": "husbandry.penning = 0.34"},
		{"tick": 6, "kind": "narrative_beat",
			"label": "We are more now than we were when we left the bone ground. The children born on this road have never slept anywhere else.",
			"detail": "band.count = 31"},
	]

## TWO tall pages (ticks 0 and 1, seven distinct long beats each) that BOTH overflow `PAGE_MAX_HEIGHT`, so
## the inner ScrollContainer actually scrolls — the fixture the yields-to-reader scroll test needs (a page
## that fits the cap can hold no non-zero scroll offset to preserve).
func _telling_tall_pages_fixture() -> Array:
	var out: Array = []
	var long := "The chase is longer every season and ends in less; the aurochs were the road we walked, and the road is going quiet under our own feet."
	for tick in [0, 1]:
		for i in range(7):
			out.append({"tick": tick, "kind": "narrative_beat",
				"label": "%d. %s" % [i, long], "detail": "beat %d of tick %d" % [i, tick]})
	return out

## Ordinary command receipts for the split frame — the transactional acknowledgements that used to
## be pushed off the feed by two beats. Deliberately MORE than one, so "the feed is legible again"
## is something the frame can actually show rather than imply.
func _telling_command_receipts() -> Array:
	return [
		{"tick": 22, "kind": "command", "label": "Assign labor", "detail": "6 foragers → (27, 26)"},
		{"tick": 22, "kind": "command", "label": "Assign labor", "detail": "3 hunters → Aurochs Herd"},
		{"tick": 23, "kind": "command", "label": "Move band", "detail": "Band 1 → (28, 25)"},
		{"tick": 23, "kind": "site_discovered", "label": "Salt Pillar Reach", "detail": "Wondrous site at (31, 22)"},
	]

## Settle the HUD for a capture. `finish_tweens = false` is for the ONE state that must capture a page
## turn IN MOTION; it steps the tween itself so the phase is chosen rather than raced.
func _settle(finish_tweens: bool = true) -> void:
	await _ensure_canvas()
	if finish_tweens:
		_flush_tweens()
	await get_tree().process_frame
	# Force a synchronous frame rather than awaiting `RenderingServer.frame_post_draw`.
	# Under the dummy rendering backend (which `--headless` selects on Godot 4.5) no
	# real draw ever posts, so that await never returns and the harness hangs. force_draw
	# just no-ops there, so a stray headless run fails fast in `_save` instead of hanging.
	RenderingServer.force_draw()
	await get_tree().process_frame

## Drive every live tween to its END state. THIS IS WHAT MAKES THE TIME FREEZE SAFE: a shader's `TIME`
## still evaluates at phase 0, but a Tween at `time_scale = 0` never advances AT ALL, so a page turn
## would be pinned at `progress = 0` — the OUTGOING page fully opaque and the incoming one at alpha 0,
## i.e. a frame that renders the page BEFORE the turn it exists to show. Stepping past the duration
## also fires the finished-callback, so the panel's own `_end_turn` settle runs exactly as it does live.
func _flush_tweens() -> void:
	for tween in get_tree().get_processed_tweens():
		if tween.is_valid() and tween.is_running():
			tween.custom_step(TWEEN_FLUSH_SECONDS)

## Advance every live tween by a FIXED slice — a deliberately chosen mid-motion phase, for the state
## that captures a page turn in flight. Deterministic because the clock contributes nothing.
func _step_tweens(seconds: float) -> void:
	for tween in get_tree().get_processed_tweens():
		if tween.is_valid() and tween.is_running():
			tween.custom_step(seconds)

## Hold the window at the pinned canvas. Deliberately does NOT touch `content_scale_size` /
## `content_scale_factor` (the same call `map_preview` makes): `project.godot` stretches `canvas_items`
## with an `expand` aspect, so pinning those would re-project every frame — a mass pixel change, not a
## race fix. The race is a window mode/size problem.
func _pin_canvas(win: Window) -> void:
	win.mode = Window.MODE_WINDOWED
	win.size = PREVIEW_CANVAS_SIZE

## Hold the window at the pinned canvas and WAIT for the WM to honour it, before anything is captured.
## `project.godot` opens MAXIMIZED and macOS applies — and RE-applies — that asynchronously, many
## frames in, so the bare `get_window().size = …` this harness used to do in `_ready` was a RACE that
## did not stay won. Measured on two clean runs of identical code: one came back with 177 of its 184
## frames at the monitor's 5120x1410 instead of the intended 1500x900 while the other rendered all 184
## at 1500x900 — so the two runs disagreed on the HUD's LAYOUT, not merely its pixels, and most frames
## were being judged at a width the HUD never ships at.
func _ensure_canvas() -> void:
	for _i in range(CANVAS_PIN_MAX_FRAMES):
		if get_window().size == PREVIEW_CANVAS_SIZE and get_window().mode == Window.MODE_WINDOWED:
			return
		_pin_canvas(get_window())
		await get_tree().process_frame

## Settle the window ONCE, in `_ready`, before any state renders — and take the maximize DELIBERATELY
## on the way, which is what closes the last of the drift.
##
## `project.godot` opens the window MAXIMIZED and macOS applies that asynchronously, so whether a run
## ever passed through the monitor-sized window was a COIN FLIP — and it is a coin flip the pixels
## remember. Measured over four runs with the clock already frozen and the canvas pinned: runs that
## never maximized and runs that did formed two byte-DISTINCT clusters, differing by ±1 on the
## antialiased edges of ~85 frames (`window/stretch` is `canvas_items` with an `expand` aspect, so the
## stretch scale swings 0.78 → 2.67 → 0.78 across a maximize and the rasterized-glyph/coverage state
## does not come back bit-identical). Dodging the maximize is not available — a late one still landed
## mid-run after 30 stable frames — so ASK for it, then undo it: every run now takes the same path.
## Four consecutive runs came back 184/184 byte-identical, with zero mid-run re-pins.
func _stabilize_canvas() -> void:
	get_window().mode = Window.MODE_MAXIMIZED
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size != PREVIEW_CANVAS_SIZE:
			break
		await get_tree().process_frame
	# Restore and HOLD: the maximize is re-applied asynchronously, so "the right size once" is not the
	# same as "it stays" — wait for CANVAS_STABLE_FRAMES consecutive good frames. After this every
	# `_ensure_canvas` returns without awaiting, so each state gets the same number of layout passes.
	var stable := 0
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size == PREVIEW_CANVAS_SIZE and get_window().mode == Window.MODE_WINDOWED:
			stable += 1
			if stable >= CANVAS_STABLE_FRAMES:
				return
		else:
			stable = 0
			_pin_canvas(get_window())
		await get_tree().process_frame
	push_error("ui_preview: the window never held the pinned %s canvas — frames will drift" % PREVIEW_CANVAS_SIZE)

## The viewport image, GUARANTEED to be the pinned canvas (or an integer HiDPI multiple of it). The
## WM's deferred maximize can resize the render target between a settle and a capture, so re-pin and
## re-draw until the geometry is the canvas's, then give up loudly rather than save a bad frame. With
## `content_scale_*` deliberately unpinned the captured image matches the WINDOW, not the viewport's
## logical `expand` rect — so the guard measures against the window-sized canvas.
func _capture(name: String) -> Image:
	for _i in range(CANVAS_PIN_MAX_FRAMES):
		var image := get_viewport().get_texture().get_image()
		if image == null:
			# No image to read back — the dummy renderer (i.e. someone ran this with
			# `--headless`, which selects it on Godot 4.5). Capture is impossible, but
			# the compile/scene gate still passed. Run WITHOUT `--headless` for PNGs.
			push_warning("ui_preview: null image (dummy renderer?) — skipping %s.png; run without --headless to capture" % name)
			return null
		var w := image.get_width()
		var h := image.get_height()
		if w % PREVIEW_CANVAS_SIZE.x == 0 and h % PREVIEW_CANVAS_SIZE.y == 0 \
				and w / PREVIEW_CANVAS_SIZE.x == h / PREVIEW_CANVAS_SIZE.y:
			return image
		_pin_canvas(get_window())
		await get_tree().process_frame
		RenderingServer.force_draw()
		await get_tree().process_frame
	push_error("ui_preview: viewport never came back to the pinned %s canvas for %s" % [PREVIEW_CANVAS_SIZE, name])
	return null

func _save(name: String) -> void:
	# Check the herd fixtures RENDERING IN THIS FRAME, so a half-set field pair fails against the state
	# it silently mis-renders rather than against nothing at all.
	_guard_frame_herd_fields(name)
	var image: Image = await _capture(name)
	if image == null:
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

## The Telling: a pending fork on the wire, in the per-faction shape the native decoder produces
## (`[{faction, forks: [...]}]`). Copy is verbatim from beat_definitions.json —
## `sedentarization.soft_drift` / `soft_drift.long_chase` — with `{beast.plural}` resolved the way
## the sim resolves nouns at post time, so the frame judges REAL prose at REAL length.
func _pending_forks_fixture() -> Array:
	return [{
		"faction": 0,
		"forks": [{
			"beat_id": "sedentarization.soft_drift",
			"wardrobe_id": "soft_drift.long_chase",
			"posted_tick": 41,
			"narration": [
				{"register": FORK_REGISTER_MYTHIC, "text": "Three seasons, and each one we chased the mammoths and left the seed-ground unturned. The children do not remember a walled night. At the fires, they have begun to call us the People of the Long Chase. Is that who we are?"},
				{"register": FORK_REGISTER_WARM, "text": "Three seasons now, all of them spent following the mammoths, and nobody's turned the seed-ground once. The children have never slept behind a wall. People have started calling us the People of the Long Chase. Is that us?"},
			],
			"choices": [
				{"choice_id": "yes_trail", "is_defer": false, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "We are the trail"},
					{"register": FORK_REGISTER_WARM, "text": "Yes — we're trail people"},
				]},
				{"choice_id": "no_root", "is_defer": false, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "We were meant to root"},
					{"register": FORK_REGISTER_WARM, "text": "No — we were meant to settle"},
				]},
				# Exactly one choice carries is_defer, and the SERVER computes it — the client reads
				# the flag and never re-derives which choice writes nothing.
				{"choice_id": "defer", "is_defer": true, "label": [
					{"register": FORK_REGISTER_MYTHIC, "text": "Say nothing"},
					{"register": FORK_REGISTER_WARM, "text": "Let it lie for now"},
				]},
			],
			"gloss": [
				{"signal": "sedentarization.score", "value": 41.0},
				{"signal": "stance.roam_settle", "value": -0.18},
			],
		}],
	}]

func _stance_axes_fixture() -> Array:
	return [{"faction": 0, "axes": [{"axis": "roam_settle", "value": -0.18}]}]

## Open the COMPOSE SHEET on a source and render its compose block there.
##
## Part 2 of docs/plan_tile_panel_layout.md moved `%ForageAssignControls` / `%HerdAssignControls` out
## of the drawer into a floating sheet, so a state that exists to judge the picker/stepper/forecast/
## gate-reasons has to OPEN it — the drawer now shows only the standing summary + `Assign … ▸`.
## These two calls replace the direct `_hud._build_*_assign_controls(...)` the states used before;
## the builders still run, just against the sheet's content container.
func _compose_forage(tile_info: Dictionary) -> void:
	_hud._drawercompose.open_forage_compose(tile_info)

## Open the herd compose sheet, optionally DIALING IN a count and/or policy.
##
## `count` / `policy` are applied AFTER the first open, and that ordering is the whole point:
## opening the compose on a DIFFERENT herd re-seeds both off the band's standing staffing
## (`DrawerComposeController._build_herd_assign_controls`'s `source_changed` branch → `seed_hunt`),
## so a `set_hunt_count`/`set_hunt_policy` made BEFORE the first open is silently thrown away —
## the bug that rendered the documented 6-hunter local-hunt frames with 1 hunter (#357). The
## second open re-renders with `hunt_key` unchanged, so `source_changed` is false and the dialed
## values survive into the render (still subject to the stepper's own cap clamp, as in the game).
## Omit both to render whatever the re-seed produces — which is what the raid states deliberately want.
func _compose_herd(herd: Dictionary, count: int = COMPOSE_COUNT_UNSET, policy: String = "") -> void:
	# The compose sheet is where the pair actually BITES (`_forecast_worker_cap`'s floor), and a herd
	# can be composed without ever being the selected subject — so check the argument here too, not
	# only through the per-frame scan in `_save`.
	_guard_herd_fields(herd, "compose_herd")
	_hud._drawercompose.open_herd_compose(herd)
	if count == COMPOSE_COUNT_UNSET and policy == "":
		return
	if count != COMPOSE_COUNT_UNSET:
		_hud._compose.set_hunt_count(count)
	if policy != "":
		_hud._compose.set_hunt_policy(policy)
	_hud._drawercompose.open_herd_compose(herd)

## A synthetic PRESSED mouse-button event, for driving a Control's real `gui_input` handler. The
## harness has no OS input, so this is how a click/wheel gesture is put through the shipped code path
## rather than calling the handler's effect directly.
func _mouse_button_event(button_index: int) -> InputEventMouseButton:
	var event := InputEventMouseButton.new()
	event.button_index = button_index
	event.pressed = true
	return event

## Find a Button by its face anywhere under `root` — the harness presses the REAL control the player
## presses, so an assertion covers the wiring and not just the handler it would have called.
## Drive a Food/Morale disclosure the way a CLICK does: emit `meta_clicked` on the live drawer label
## with the very `[url]` meta its own text carries, so the bound handler + anchor run exactly as they
## do in the game. Toggling: a second call on the same key dismisses the popover.
func _click_disclosure(key: String) -> void:
	var meta := HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + key
	var label := _find_meta_label(_hud, meta)
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

## Does any Label under `root` contain this text? The gate-reason assertions' instrument: a reason that
## has been COLLAPSED into a tooltip is no longer any label's text, so this tells a spelled-out
## prerequisite from a one-line "locked (N requirements unmet)" summary.
## The text of the first Label under `root` containing `needle` — "" when there is none. Lets a frame
## assert on a value that must CHANGE (the forecast's "→ then" term) rather than merely be present.
## The forecast's dip→payoff arrow. Deliberately NOT the bare word "then": the Cultivate policy hint
## rendered on the same card ("…then a much higher tended yield") contains it too.
const FORECAST_THEN_NEEDLE := "→ then"
## The unstaffed forecast's opening, in its SHORT form ("Assign foragers — +1.20 /turn").
const UNSTAFFED_COPY_NEEDLE := "Assign foragers —"

func _label_text_containing(root: Node, needle: String) -> String:
	if root == null:
		return ""
	if root is Label and (root as Label).text.contains(needle):
		return (root as Label).text
	for child in root.get_children():
		var found := _label_text_containing(child, needle)
		if found != "":
			return found
	return ""

func _has_label_containing(root: Node, text: String) -> bool:
	if root == null:
		return false
	if root is Label and (root as Label).text.contains(text):
		return true
	for child in root.get_children():
		if _has_label_containing(child, text):
			return true
	return false

func _find_button_by_text(root: Node, text: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).text == text:
		return root as Button
	for child in root.get_children():
		var found := _find_button_by_text(child, text)
		if found != null:
			return found
	return null

## The METRIC line of a policy rung's two-line face — `→ 1.48 food · 0.37 trade`, the products line
## the payoff/cap assertions read. The rung is found by `HudWidgets.POLICY_RUNG_META`, its identity,
## and NEVER by button text: the face lives on a two-Label stack beside an empty-`text` Button, so
## `_find_button_by_text` finds nothing at all here. "" when the rung is absent from the picker or
## wears its name alone (no metric).
func _policy_rung_metric(root: Node, policy: String) -> String:
	var btn := _find_policy_rung(root, policy)
	if btn == null:
		return ""
	# The face's Labels are siblings of the Button under the rung's CELL, not children of it.
	var lines := _face_lines(btn.get_parent())
	return lines[POLICY_RUNG_METRIC_LINE] if lines.size() > POLICY_RUNG_METRIC_LINE else ""

func _find_policy_rung(root: Node, policy: String) -> Button:
	if root == null:
		return null
	if root is Button and (root as Button).get_meta(HudWidgets.POLICY_RUNG_META, "") == policy:
		return root as Button
	for child in root.get_children():
		var found := _find_policy_rung(child, policy)
		if found != null:
			return found
	return null

## Every Label text under `root`, in tree order — the rung face's lines as they are stacked.
func _face_lines(root: Node) -> Array[String]:
	var lines: Array[String] = []
	if root == null:
		return lines
	if root is Label:
		lines.append((root as Label).text)
	for child in root.get_children():
		lines.append_array(_face_lines(child))
	return lines

## How many Buttons under `root` wear this face — the "is the same order offered twice?" test.
func _count_buttons_by_text(root: Node, text: String) -> int:
	if root == null:
		return 0
	var total := 1 if (root is Button and (root as Button).text == text) else 0
	for child in root.get_children():
		total += _count_buttons_by_text(child, text)
	return total

## Same shape as `_assert_turn_orb`, for dock-card visibility. A PNG shows what a frame looks like;
## these say what it MUST be, so a default regression fails loudly in the run log instead of waiting
## for someone to notice a card that should not be there.
func _assert_hud(label: String, ok: bool) -> void:
	if ok:
		print("ui_preview: PASS hud — ", label)
	else:
		push_error("ui_preview: FAIL hud — %s" % label)

# ---- the herd herders_needed FIELD-PAIR guard ---------------------------------------------------
# The sim exports TWO herder counts per herd and the client reads DIFFERENT ones by rung, so a fixture
# that sets only one is a silent lie rather than an error:
#   • `herders_needed` — OWNERSHIP-GATED (`fauna::herd_herders_needed`): 0 unless the herd is
#     corralled or owned. The extractive rungs' field, and what the drawer's "Herders A / N" row reads.
#   • `herders_needed_if_managed` — ownership-INDEPENDENT (`fauna::would_be_herders_needed`): the crew
#     the herd WOULD owe, 0 only for a species that can never be tamed. `DrawerComposeController`'s
#     `_forecast_worker_cap` floor reads THIS one for the INVESTMENT rungs (Tame / Corral).
# `_under_herded_corral_fixture` set only the first; the investment floor therefore read 0, the crew
# cap collapsed to 1, and the frame rendered the exact opposite of the cap it documents — with nothing
# logged anywhere, because both keys are optional and `get(…, 0)` is a legal answer.
#
# THE INVARIANT, from the sim, not from guesswork: `would_be_herders_needed` is identical to
# `herd_herders_needed` except its gate, so the two agree on every herd EXCEPT a not-yet-owned tameable
# one (where the gated field is 0 and the would-be crew is real — that is `_tame_worker_cap_herd_fixture`,
# 0 and 10, deliberately). A herd whose gated count is `> 0` is by definition managed (corralled or
# owned) and therefore tameable, so the ungated field takes the same branch:
#     herders_needed > 0  ⇒  herders_needed_if_managed == herders_needed
# and, in general, `herders_needed_if_managed >= herders_needed`. Pinned sim-side by
# `core_sim/src/snapshot/mod.rs`'s would-be-crew export test (its three cases are exactly these).
const HERDERS_NEEDED_KEY := "herders_needed"
const HERDERS_NEEDED_IF_MANAGED_KEY := "herders_needed_if_managed"
## Deep-scan bound. Fixtures are trees, but a bound turns a future self-referencing one into a stop
## rather than an infinite walk.
const HERD_SCAN_MAX_DEPTH := 8

var _herd_pair_scans := 0
var _herd_pair_violations := 0

## Walk everything reachable from `subject` and check the pair on every dict that carries either half.
## Deliberately a SCAN and not a per-fixture assertion: a guard you have to remember to call for each
## new fixture is the same failure mode as remembering to set the second field.
func _guard_herd_fields(subject: Variant, where: String, depth: int = 0) -> void:
	if depth > HERD_SCAN_MAX_DEPTH:
		return
	if subject is Array:
		for item in (subject as Array):
			_guard_herd_fields(item, where, depth + 1)
		return
	if not (subject is Dictionary):
		return
	var dict: Dictionary = subject
	if dict.has(HERDERS_NEEDED_KEY) or dict.has(HERDERS_NEEDED_IF_MANAGED_KEY):
		_herd_pair_scans += 1
		var needed := int(dict.get(HERDERS_NEEDED_KEY, 0))
		var if_managed := int(dict.get(HERDERS_NEEDED_IF_MANAGED_KEY, 0))
		if if_managed < needed:
			_herd_pair_violations += 1
			push_error(("ui_preview: FAIL herd fields — %s herd \"%s\" declares %s %d but %s %d. "
				+ "The would-be crew can never be SMALLER than the ownership-gated one, and on a herd "
				+ "with herders (i.e. a managed one) the sim exports them EQUAL — the investment rungs' "
				+ "worker cap floors on the second field, so half-setting the pair silently caps the "
				+ "crew at the take-side count.") % [where, String(dict.get("id", "?")),
				HERDERS_NEEDED_KEY, needed, HERDERS_NEEDED_IF_MANAGED_KEY, if_managed])
		elif needed > 0 and if_managed != needed:
			# The OTHER half of the invariant, and the one a `>=` test lets through. The gate is the
			# ONLY difference between the two sim functions, so a NON-ZERO gated count already says the
			# herd passed the gate — it is corralled or owned — and the would-be crew is then computed
			# from the same species and headcount by the same arithmetic. A bigger would-be crew is not
			# a conservative fixture, it is an impossible herd: it claims managing this herd would cost
			# MORE than managing it already does.
			_herd_pair_violations += 1
			push_error(("ui_preview: FAIL herd fields — %s herd \"%s\" declares %s %d and %s %d. Once "
				+ "%s is above zero the herd IS managed, and the would-be crew is the SAME crew — the "
				+ "sim's two functions differ only by the ownership gate this herd has already passed, "
				+ "so they must be EQUAL here. Set both through _set_managed_herders; only a still-WILD "
				+ "tameable herd may carry a larger would-be crew, and its gated count is 0.")
				% [where, String(dict.get("id", "?")), HERDERS_NEEDED_KEY, needed,
				HERDERS_NEEDED_IF_MANAGED_KEY, if_managed, HERDERS_NEEDED_KEY])
	for value in dict.values():
		_guard_herd_fields(value, where, depth + 1)

## Every herd dictionary the HUD is holding as this frame renders — the world list, the selected
## subject and roster, the tile's occupants, and the bands (whose `tile_info` carries herds too).
func _guard_frame_herd_fields(state: String) -> void:
	_guard_herd_fields(_hud._band_labor._world_herds, state)
	_guard_herd_fields(_hud._band_labor._player_band, state)
	_guard_herd_fields(_hud._band_labor._player_bands, state)
	_guard_herd_fields(_hud._band_labor._panel_band, state)
	_guard_herd_fields(_hud._selection._selected_herd, state)
	_guard_herd_fields(_hud._selection._roster_herds, state)
	_guard_herd_fields(_hud._selection._selected_tile_info, state)

## A 4-digit turn — the widest the face has to hold, and the case a fixed font size would clip.
const TURN_ORB_FOUR_DIGIT_TURN := 1200

## GUARD: the turn number on the orb face must BE the turn, be sized inside the declared band, and fit
## the face's usable chord. Measured against the button's own font, exactly as `_turn_font_size` does —
## the alternative (eyeballing turn 1200) is how a clipped number ships.
func _assert_turn_face_fits(expected_turn: int) -> void:
	var orb := _hud.turn_orb
	var face: Button = orb._face
	var text := face.text
	var size := face.get_theme_font_size("font_size")
	var font := face.get_theme_font("font")
	var budget: float = TurnOrb.FACE_DIAMETER * TurnOrb.TURN_TEXT_WIDTH_FRACTION
	var width: float = font.get_string_size(text, HORIZONTAL_ALIGNMENT_CENTER, -1, size).x
	var ok := text == str(expected_turn) \
		and size >= TurnOrb.TURN_FONT_SIZE_MIN and size <= TurnOrb.TURN_FONT_SIZE_MAX \
		and width <= budget + 1.0
	_assert_turn_orb("turn %d on the face reads '%s' at %dpx, %.0f of %.0f wide" % [
		expected_turn, text, size, width, budget], ok)

## GUARD: the curved `TURN` word inside the face must not wrap around the circle, must not cross the
## face's edge, and must vanish when the face swaps the number out for the advance glyph. Drawn pixels
## cannot be asserted, so assert the ARITHMETIC — via the SAME `turn_word_metrics()` the draw reads, so
## this cannot pass while the renderer computes something else.
func _assert_turn_word_clears() -> void:
	var orb := _hud.turn_orb
	var metrics: Dictionary = orb.turn_word_metrics()
	var arc_angle: float = metrics["arc_angle"]
	var outer_reach: float = float(metrics["radius"]) + float(metrics["glyph_height"])
	var face_radius: float = TurnOrb.FACE_DIAMETER * 0.5
	_assert_turn_orb("curved '%s' spans %.0f° (ceiling %.0f°)" % [
			TurnOrb.TURN_WORD, rad_to_deg(arc_angle), rad_to_deg(TurnOrb.TURN_WORD_MAX_ARC_ANGLE)],
		arc_angle > 0.0 and arc_angle < TurnOrb.TURN_WORD_MAX_ARC_ANGLE)
	_assert_turn_orb("curved '%s' reaches %.1f of the face's %.1f radius" % [
			TurnOrb.TURN_WORD, outer_reach, face_radius], outer_reach <= face_radius)
	# The hover rule: with an EMPTY registry the face swaps to `GLYPH`, and the word must go with it.
	var was_hovered: bool = orb._face_hovered
	orb._set_face_hovered(true)
	var hidden_on_glyph := not orb._show_turn_word()
	orb._set_face_hovered(was_hovered)
	_assert_turn_orb("curved '%s' hides while the face shows the advance glyph" % TurnOrb.TURN_WORD,
		hidden_on_glyph)

func _assert_turn_orb(label: String, ok: bool) -> void:
	if ok:
		print("ui_preview: PASS turn-orb — ", label)
	else:
		push_error("ui_preview: FAIL turn-orb — %s" % label)

## The instance ids of a container's direct children, so an assertion can prove a restate REUSED the
## same nodes (in-place patch) rather than freeing + recreating them (teardown).
func _child_instance_ids(node: Node) -> Array:
	var ids: Array = []
	if node != null:
		for child in node.get_children():
			ids.append(child.get_instance_id())
	return ids

## The face text of the chip at `index` in the pinned chip strip (each chip is a PanelContainer whose
## first child is its Label).
func _chip_text(strip: Node, index: int) -> String:
	if strip == null or index < 0 or index >= strip.get_child_count():
		return ""
	var chip := strip.get_child(index)
	if chip.get_child_count() == 0:
		return ""
	var label := chip.get_child(0) as Label
	return label.text if label != null else ""

## The forage drawer's standing-summary text (the first child of `%ForageAssignControls` is the
## summary HFlowContainer; its first child is the main status Label).
func _forage_summary_text() -> String:
	var controls := _hud.forage_assign_controls
	if controls == null or controls.get_child_count() == 0:
		return ""
	var flow := controls.get_child(0)
	if flow.get_child_count() == 0:
		return ""
	var label := flow.get_child(0) as Label
	return label.text if label != null else ""

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
		"turns_of_food": 22.0,
		# Good morale (≥ warn, not falling) → the Morale row is collapsed with a ▸ caret. The signed
		# Layer-1 contributions (above the breakdown epsilon) give the disclosure real content on expand.
		"morale": 0.82,
		"morale_settling": 0.012,
		"morale_terrain": -0.010,
		"morale_climate": -0.006,
		# Thriving growth (docs/plan_population_growth_model.md): fed (hunger 1.0, so that factor is
		# neutral and its row is omitted), a saturated larder (reserve 1.5) and net-positive food
		# (trend 1.25) → 1.0 × 1.5 × 1.25 = 188% of normal. Reads neutral ink — normal growth is
		# normal, not a "good" — and its disclosure shows what is HELPING, which is the good-state
		# case the row must still be openable in.
		"fertility_hunger": 1.0,
		"fertility_reserve": 1.5,
		"fertility_trend": 1.25,
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
		# line. Per-source actual/sustainable yields live on the assignments below.
		# The Gathered/Hunted breakdown sums the assignment actual_yields (0.48 / 0.46) by kind.
		"food_income": 0.94,
		"food_consumption": 0.68,
		# `workers_needed` is the overstaffing axis, INDEPENDENT of the overdraw (⚠) axis — the two
		# rows below deliberately cross them so one frame proves both, AND proves the ⚠ now keys off the
		# sim-answered `overdraws` bool, not the client-derived `actual > sustainable`:
		#   • forage: 5 assigned but only 1 needed (the patch's ceiling caps the take) → the amber
		#     "· only 1 of 5 working" note, and NO ⚠ (Sustain patch, overdraws=false).
		#   • hunt: 4 assigned, 4 needed → no overstaff note. `actual_yield 0.46 > sustainable_yield 0.20`
		#     (a banked whole animal cashed on this KILL turn), yet `overdraws=false` under Sustain → the
		#     row reads CLEAN, NO ⚠. Under the old client test this row false-tripped the flag — the fix.
		"labor_assignments": [
			{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "policy": "sustain", "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1, "overdraws": false},
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20, "workers_needed": 4, "overdraws": false},
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
	band["turns_of_food"] = 22.0
	band["food_income"] = 5.88          # forage 0.48 + the pen's gross 5.40
	band["food_consumption"] = 1.15     # the PEOPLE's meals
	band["pen_feed_upkeep"] = 1.74      # the ANIMALS' feed — a debit in neither row above
	band["fodder_store"] = 12.4         # the band's HAY larder (Flora roster F3) — feeds the pen
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "policy": "sustain", "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1},
		# A managed source: one keeper, take == sustainable (escapement); Corral is managed, so the
		# sim-answered `overdraws` is false → no ⚠ and no overstaff note.
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "policy": "corral", "target_x": 70, "target_y": 17, "actual_yield": 5.40, "sustainable_yield": 5.40, "workers_needed": 1, "overdraws": false},
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
		{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "policy": "sustain", "actual_yield": 0.48, "sustainable_yield": 0.48, "workers_needed": 1, "overdraws": false},
		# BOTH PRODUCTS on the drawer's standing summary (issue #337): the hide sells beside the meat,
		# so the one-line summary must read `+0.84 /turn · ⇄ +0.12` — food leading, trade only because
		# it is non-zero. It comes from the SAME `SourceForecast.source_yield_readout` the Band panel's
		# rows use, so the two surfaces cannot state different products for one assignment.
		{"kind": "hunt", "workers": 1, "fauna_id": "game_deer_07", "policy": "corral", "target_x": 70, "target_y": 17, "actual_yield": 0.84, "sustainable_yield": 0.84, "workers_needed": 1, "overdraws": false, "trade_yield": 0.12, "realized_trade_yield": 0.12},
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
	var fixture := _band_fixture()
	fixture["fertility_hunger"] = 0.0
	fixture["fertility_reserve"] = 0.0
	fixture["fertility_trend"] = 0.0
	return fixture

func _concerning_food_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 905
	band["id"] = "Band 3"
	band["turns_of_food"] = 4.0
	band["food_income"] = 0.30
	band["food_consumption"] = 0.95
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "target_x": 71, "target_y": 18, "actual_yield": 0.15, "sustainable_yield": 0.15, "overdraws": false},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.15, "sustainable_yield": 0.20, "overdraws": false},
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
		"turns_of_food": 9.0,
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
	var fixture := _band_fixture()
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

## The near band of `_forage_range_bands`, ALREADY WORKING the (66,10) food tile — the fixture behind
## the drawer's standing-assignment summary (§14). The assignment deliberately crosses the two
## INDEPENDENT flags the summary shares with a Band-panel Current-actions row: `overdraws` true (a
## Deplete patch drawing past regrowth — the ecological ⚠) AND 4 workers where 2 are needed (the labor
## "· only 2 of 4 working" note). `realized_yield` is the steady average the summary headlines.
## The near band of `_forage_range_bands`, ALREADY WORKING the (66,10) food tile at a MODEST staffing —
## the fixture behind the compose sheet's UNASSIGN state. Deliberately separate from
## `_standing_forage_band_fixture`, whose assignment is tuned to trip the drawer summary's overdraw and
## overstaff flags; this one is a plain, healthy Cultivate crew, so the unassign frame is judged on the
## button/forecast pair and nothing else.
func _cultivating_forage_band_fixture() -> Dictionary:
	var band: Dictionary = _forage_range_bands()[0]
	band["labor_assignments"] = [{
		"kind": "forage", "workers": 1, "target_x": 66, "target_y": 10, "policy": "cultivate",
		"actual_yield": 0.24, "sustainable_yield": 0.96, "realized_yield": 0.24,
		"workers_needed": 1, "overdraws": false,
	}]
	return band

func _standing_forage_band_fixture() -> Dictionary:
	var band: Dictionary = _forage_range_bands()[0]
	band["labor_assignments"] = [{
		"kind": "forage", "workers": 4, "target_x": 66, "target_y": 10, "policy": "deplete",
		"actual_yield": 2.74, "sustainable_yield": 0.96, "realized_yield": 2.74,
		"workers_needed": 2, "overdraws": true,
	}]
	return band

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
	herd["hunt_trip_estimates"] = _raid_estimate_table(
		BOAR_RAID_TURNS, BOAR_RAID_ANIMALS, BOAR_FOOD_PER_ANIMAL)
	return herd

## A raid estimate TABLE from a per-party Sustain (turns, animals) pair (index i = a party of i+1). The
## deeper policies raid to a lower floor, so they take MORE animals (Surplus < Deplete < Eradicate) — the
## per-policy ASCENDING the picker buttons read. **Eradicate DELIVERS** — it takes the most animals and
## banks the whole-stock windfall (issue #337 redefined `delivers_food`: it means the QUARRY IS EDIBLE,
## not "this rung is a denial mission", and a boar is edible on every rung). Every cell also carries the
## trade twin, since a hunt pays a VECTOR: `delivers_trade` + `delivered_trade = animals × tpa`.
## The per-policy bumps are illustrative fixture data; the live sim exports the real per-floor counts.
func _raid_estimate_table(turns_row: Array, animals_row: Array, fpa: float,
		tpa: float = RAID_TRADE_PER_ANIMAL) -> Dictionary:
	var table := {}
	for i in animals_row.size():
		var turns := int(turns_row[i])
		var base := int(animals_row[i])
		# A CLEAN raid: the party hauls its whole kill home, so delivered_food = animals × fpa, waste 0.
		# delivered_food is the PRIMARY payload the client headlines + the field the max-useful scan and
		# "too lean" test read — every cell must carry it.
		for entry in [["sustain", 0], ["surplus", 2], ["deplete", 3], ["eradicate", 5]]:
			var animals: int = base + int(entry[1])
			table["%s:%d" % [String(entry[0]), i + 1]] = {
				"turns_to_fill": turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": float(animals) * fpa,
				"delivered_trade": float(animals) * tpa, "wasted_food": 0.0,
			}
	return table

## A raid herd whose max-useful party DIFFERS BY POLICY, to prove the labor-bound note's "of M" tracks
## the selected policy: Sustain's animalsTaken keeps rising through a party of 4 (then plateaus), Deplete's
## through a party of 7. A band that can field only 3 hunters is labor-bound under BOTH — so the note reads
## "3 of 4 useful" on Sustain and "3 of 7 useful" on Deplete, the same herd, only the policy changed.
func _labor_bound_raid_herd() -> Dictionary:
	var herd := _assign_preview_herd("game_bison_09", "Steppe Bison", "thriving", 0.30, 0, 0)
	herd["food_per_animal"] = 4.0
	var sustain_animals := [3, 5, 7, 9, 9, 9, 9, 9]     # plateau at party 4
	var surplus_animals := [4, 6, 8, 10, 12, 12, 12, 12] # plateau at party 5
	var deplete_animals := [5, 7, 9, 11, 13, 15, 17, 17]  # plateau at party 7
	var fpa := 4.0    # matches food_per_animal above; clean raid → delivered = animals × fpa, waste 0
	var table := {}
	for i in sustain_animals.size():
		var w := i + 1
		# Every rung DELIVERS, Eradicate included (issue #337 — `delivers_food` is about the species,
		# and a bison is edible on every rung). Both products on every cell.
		for entry in [["sustain", sustain_animals[i], 8], ["surplus", surplus_animals[i], 6],
				["deplete", deplete_animals[i], 5], ["eradicate", int(deplete_animals[i]) + 2, 4]]:
			var animals: int = int(entry[1])
			table["%s:%d" % [String(entry[0]), w]] = {
				"turns_to_fill": int(entry[2]), "delivers_food": true, "delivers_trade": true,
				"animals_taken": animals, "delivered_food": float(animals) * fpa,
				"delivered_trade": float(animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0}
	herd["hunt_trip_estimates"] = table
	return herd

## A herd stripped to its policy floor: EVERY (policy, party) cell delivers 0 animals, so the raid comes
## home empty at any size — the one non-viable case (surplus is a property of the HERD, not the party, so
## no party size fixes it). The button must be DISABLED with the "too lean to raid" reason.
func _no_surplus_herd() -> Dictionary:
	var herd := _assign_preview_herd("game_rabbit_02", "Rabbit Warren", "thriving", 0.05, 0, 0)
	herd["size_class"] = "small"
	# The herd is at its floor: no surplus at ANY party size → delivered_food 0 everywhere, so the raid
	# comes home empty and the button DISABLES ("too lean — no surplus above this policy's floor").
	var table := {}
	for w in range(1, 9):
		# The species is edible and its pelts sell — it is the HERD that has nothing left, so both
		# `delivers_*` flags are true on every rung and BOTH payloads are 0. That is what makes this
		# the "too lean" case rather than the "denial mission" one (issue #337).
		for policy in ["sustain", "surplus", "deplete", "eradicate"]:
			table["%s:%d" % [policy, w]] = {
				"turns_to_fill": 0, "delivers_food": true, "delivers_trade": true,
				"animals_taken": 0, "delivered_food": 0.0, "delivered_trade": 0.0,
				"wasted_food": 0.0,
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
			# THE PARTIAL-WITH-WASTE case: a Thunder Mammoth is big game (16 food/animal), and a party of
			# 4 can't carry a whole one — it kills the 1-animal surplus and hauls only 4 food, wasting 12.
			# So the line reads a brisk-but-lossy "delivers ≈1 Thunder Mammoth over ≈6 turns · ~4 food ·
			# ⚠ 75% wasted" (cyan headline + amber waste), and the button STAYS ENABLED (a partial is a
			# real delivery, the waste % is just informative). This is the case the whole pass exists for.
			"name": "herd_hunt_forecast_viable",
			"policy": "sustain",
			"herd": _partial_waste_mammoth(),
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
			# Eradicate DELIVERS (#337): every rung is paid the species' yield vector, so this row carries
			# a real payload and the client must NOT read a denial off the policy string. A denial is a
			# quarry that pays neither product — see `_pelt_only_wolf_raid_herd` for the inedible case.
			"name": "herd_hunt_forecast_eradicate",
			"policy": "eradicate",
			"herd": _assign_preview_herd("game_deer_07", "Red Deer", "thriving", 0.30,
				DEER_SUSTAIN_TRIP_TURNS, DEER_SURPLUS_TRIP_TURNS,
				DEER_SUSTAIN_ANIMALS, DEER_SURPLUS_ANIMALS),
		},
	]

## The partial-with-waste raid herd: a Thunder Mammoth (16 food/animal) whose standing surplus is ONE
## animal. Any fieldable party kills that 1 animal but cannot carry a whole mammoth — a party of `w` hauls
## ~`w` food and wastes the rest — so `delivered_food` rises with party size while `animals_taken` stays 1.
## At the composed party of 4: delivered 4, wasted 12 → 75% wasted, button ENABLED. The per-policy turns
## descend Sustain(6) > Surplus(4) > Deplete(3) > Eradicate(2) so the picker's max-food/turn caps read
## ASCENDING. This is
## exactly the case the old `animals_taken`-based "too lean" test and plateau scan got wrong (a leading 1).
func _partial_waste_mammoth() -> Dictionary:
	var herd := _assign_preview_herd("game_mammoth_11", "Thunder Mammoth", "thriving", 2.7,
		MAMMOTH_SUSTAIN_TRIP_TURNS, MAMMOTH_SURPLUS_TRIP_TURNS,
		MAMMOTH_SUSTAIN_ANIMALS, MAMMOTH_SUSTAIN_ANIMALS)
	var fpa := 16.0
	herd["food_per_animal"] = fpa
	# Eradicate rides the SAME loop as the other three (#337): it is paid the species' yield vector like
	# every rung, so a mammoth is edible on Eradicate too. It merely raids fastest (2 turns), which keeps
	# the picker's max-food/turn caps ascending. It used to carry a hand-built `delivers_food = false`
	# cell — a denial state the sim can no longer produce for an edible quarry.
	var policy_turns := {"sustain": 6, "surplus": 4, "deplete": 3, "eradicate": 2}
	var table := {}
	for w in range(1, 9):
		var delivered := minf(float(w), fpa)     # each hunter hauls ~1 food of the 16-food kill
		for policy in policy_turns:
			table["%s:%d" % [policy, w]] = {
				"turns_to_fill": int(policy_turns[policy]), "delivers_food": true,
				"animals_taken": 1, "delivered_food": delivered, "wasted_food": fpa - delivered,
			}
	herd["hunt_trip_estimates"] = table
	return herd

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

## The oracle band for the carry-aware delivered/waste preview: per-worker 0.8, output 1.0 (so the
## rendered numbers match the spec oracle EXACTLY — no morale modifier muddying them), sitting ON the
## herd (local branch), with plenty of idle workers so the big-game auto-max (20 carriers) isn't
## labor-bound.
func _delivered_oracle_band() -> Dictionary:
	return {
		"id": "Band 1", "entity": 840, "faction": 0, "size": 120,
		"current_x": 66, "current_y": 10, "pos": [66, 10],
		"working_age": 30, "idle_workers": 26,
		"hunt_reach": 7, "work_range": 2, "max_expedition_party_size": 8,
		"hunt_per_worker_provisions": 0.8,
		"output_multiplier": 1.0,
		"activity": "hunt", "labor_assignments": [],
	}

## The spec oracle deer: food_per_animal 1.23, Sustain flow ceiling 2.33, per-worker 0.8, output 1.0.
##   1 worker  → can't carry one whole 1.23 deer → delivered 0.80, ≈0.65 deer/turn · ⚠ 35% wasted
##   2 workers → lands exactly one whole deer/turn, no waste → ≈1 deer/turn · renewable
##   4 workers → the Sustain-max cap, delivered 2.33 → ≈1.89 deer/turn, no waste
## Ascending `hunt_policy_ceilings` so the "up to X/turn" cap buttons read Sustain < Surplus < Deplete <
## Eradicate; husbandry ceiling "wild" keeps the picker to the four extractive rungs.
func _delivered_oracle_herd() -> Dictionary:
	return {
		"id": "game_deer_07", "label": "Red Deer (game_deer_07)", "species": "Red Deer",
		"size_class": "big", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 820.0,
		"husbandry_ceiling": "wild",
		"food_per_animal": 1.23,
		"per_worker_yield": 0.8,
		"hunt_policy_ceilings": {
			"sustain": 2.33, "surplus": 3.5, "deplete": 5.0, "eradicate": 7.0,
		},
		# THE SECOND PRODUCT (issue #337). A deer is edible AND its hide sells, so it pays BOTH: the
		# picker's four rungs must read food-then-trade (food leading), never food alone. The trade
		# ceilings are the food ones times the species' hide-to-meat ratio, so they ascend together.
		"trade_per_animal": 0.18,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.34, "surplus": 0.51, "deplete": 0.73, "eradicate": 1.02,
		},
		"tile_info": _plain_herd_tile_info(),
	}

## THE INEDIBLE QUARRY (issue #337) — a wolf pack: `provisions == 0` on every rung, a real TRADE yield
## on all four. It is the frame the whole arc is judged on. Before the fix the client read only food, so
## this herd rendered `+0.00` on every picker button and a source "worth nothing"; it must now read four
## ASCENDING trade numbers, NO food line anywhere, and no zeros. Every food-denominated field is
## deliberately 0/absent — `food_per_animal` too — so anything that still divides by a food quantum
## divides by zero and shows up in the frame rather than hiding.
func _pelt_only_wolf_herd() -> Dictionary:
	return {
		"id": "game_wolf_03", "label": "Grey Wolf (game_wolf_03)", "species": "Grey Wolf",
		"size_class": "medium", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 240.0,
		"husbandry_ceiling": "wild",
		"prey_sense_radius": 4,
		"food_per_animal": 0.0,
		"per_worker_yield": 0.0,
		"hunt_policy_ceilings": {
			"sustain": 0.0, "surplus": 0.0, "deplete": 0.0, "eradicate": 0.0,
		},
		"trade_per_animal": 1.40,
		"per_worker_trade": 0.45,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.90, "surplus": 1.35, "deplete": 1.95, "eradicate": 2.70,
		},
		"tile_info": _plain_herd_tile_info(),
	}

## The wolf's RAID table: `delivers_food = false` (an INEDIBLE quarry, NOT a denial policy) beside
## `delivers_trade = true` on every rung, so the expedition line must read a real delivery in trade
## goods rather than the "denial mission" the old `delivers_food`-only branch would have called it.
func _pelt_only_wolf_raid_herd() -> Dictionary:
	var herd := _pelt_only_wolf_herd()
	var table := {}
	var animals_row := [3, 5, 6, 6, 6, 6, 6, 6]
	for i in animals_row.size():
		for entry in [["sustain", 0, 9], ["surplus", 1, 7], ["deplete", 2, 6], ["eradicate", 4, 5]]:
			var animals: int = int(animals_row[i]) + int(entry[1])
			table["%s:%d" % [String(entry[0]), i + 1]] = {
				"turns_to_fill": int(entry[2]),
				"delivers_food": false, "delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": 0.0, "wasted_food": 0.0,
				"delivered_trade": float(animals) * 1.40,
			}
	herd["hunt_trip_estimates"] = table
	return herd

## A big-game herd for the averaging-WINDOW hint: food_per_animal 16, Sustain flow ceiling 2.4 → one whole
## mammoth lands only every ceil(16/2.4)=7 turns, so the delivered ≈0.15/turn rate carries the "≈1 … every
## ~7 turns" span line. The whole-animal cap needs 20 carriers to haul one 16-food body, and auto-max
## staffs them (band idle 26).
func _big_game_window_herd() -> Dictionary:
	return {
		"id": "game_mammoth_01", "label": "Woolly Mammoth (game_mammoth_01)",
		"species": "Woolly Mammoth",
		"size_class": "big", "huntable": true, "ecology_phase": "thriving",
		"x": 66, "y": 10, "biomass": 3200.0,
		"husbandry_ceiling": "wild",
		"food_per_animal": 16.0,
		"per_worker_yield": 0.8,
		"hunt_policy_ceilings": {
			"sustain": 2.4, "surplus": 3.6, "deplete": 5.0, "eradicate": 7.0,
		},
		# Ivory sells (issue #337) — a live herd carries the trade half of its vector too.
		"trade_per_animal": 2.4,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.36, "surplus": 0.54, "deplete": 0.75, "eradicate": 1.05,
		},
		"tile_info": _plain_herd_tile_info(),
	}

## THE FLASH GUARD's tile (docs/plan_hud_decomposition.md §2a): an active, foraged prairie hex with
## the full chip set (sight · habitability · climate · tags · site) and a standing forage patch, so a
## restate with a different `habitability` (Hospitable → Harsh) and `patch_biomass` proves the chips +
## land row + drawer patch in place instead of tearing down. Same coords across restates — the same
## HEX, only its numbers move.
func _no_flash_tile_fixture(habitability: float, biomass: float) -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"habitability": habitability,
		"temperature": 18.0,
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"site_name": "Verdant Basin",
		"patch_ecology_phase": "thriving",
		"patch_biomass": biomass,
		"patch_carrying_capacity": 120.0,
		"patch_per_worker_yield": 0.32,
		"patch_ceiling_sustain": 0.96,
		"patch_ceiling_surplus": 1.92,
		"patch_ceiling_deplete": 2.88,
		"patch_ceiling_eradicate": 4.80,
	}

## THE FLASH GUARD's band: a player band foraging the no-flash hex with `workers` on it at `yield_val`
## food/turn, so the drawer renders a standing summary (`♻ N foragers · +X /turn`) and the land row a
## `N 🌾` staffing meta — both of which must UPDATE in place (not rebuild) when the numbers change.
## Sustain + `overdraws:false` and no `workers_needed` keep the summary's SHAPE stable across restates
## (no warn/overstaff labels appear/disappear), so only values move.
func _no_flash_band_fixture(workers: int, yield_val: float) -> Dictionary:
	return {
		"id": "Band Steady",
		"size": 30,
		"entity": 909,
		"faction": 0,
		"pos": [66, 10],
		"current_x": 66, "current_y": 10,
		"activity": "forage",
		"working_age": 16,
		"idle_workers": maxi(0, 16 - workers),
		"work_range": 3,
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"output_multiplier": 1.0,
		"labor_assignments": [
			{"kind": "forage", "workers": workers, "target_x": 66, "target_y": 10,
				"policy": "sustain", "actual_yield": yield_val, "sustainable_yield": yield_val, "overdraws": false},
		],
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
		"patch_ceiling_deplete": 2.88,
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
		# WHAT GROWS HERE (flora roster F1) — the named plants this tile's forage capacity decomposes
		# into. Wire order (share DESC, then species key ASC) is preserved verbatim by the card.
		# The shares are chosen so NAIVE rounding totals 101% (46 + 30 + 25): the card must absorb the
		# remainder into the largest share and render 45 / 30 / 25 — this fixture IS the rounding test.
		# `can_cultivate` / `can_sow` are SPECIES-GLOBAL rung legality (flora roster S1), deliberately
		# mixed here so the crop picker has a greyed row in every frame: Oak Mast climbs nothing (a wild
		# harvest forever) and Ground Nut tends but never sows. `*_yield_ratio` is what committing PAYS
		# relative to gathering wild, on the CORRECTED scale (the sim's ratio omitted
		# `tended_regrowth_gain`, understating every Cultivate figure by exactly 2×) — so above 1.0 is
		# now the NORM and these read: a strong crop (2.40×), an honest middle one (1.70×) and the 0
		# sentinel on the greyed rows. `*_payoff` is the same rung's provisions/turn committed to THAT
		# species, and it is what the compose sheet's "→ then" term quotes once a crop is picked: the two
		# rows differ (1.20 vs 0.85), which is what makes the selection visibly move the forecast.
		"patch_composition": [
			{"species": "wild_grain", "display_name": "Wild Grain", "share": 0.455,
				"can_cultivate": true, "can_sow": true,
				"cultivate_yield_ratio": 2.40, "sow_yield_ratio": 4.20,
				"cultivate_payoff": 1.20, "sow_payoff": 2.40},
			{"species": "ground_nut", "display_name": "Ground Nut", "share": 0.295,
				"can_cultivate": true, "can_sow": false,
				"cultivate_yield_ratio": 1.70, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.85, "sow_payoff": 0.0},
			{"species": "oak_mast", "display_name": "Oak Mast", "share": 0.25,
				"can_cultivate": false, "can_sow": false,
				"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
				"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		],
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
## A tile whose Climate row is under test: same card as `_food_tile_fixture`, only the
## `temperature` (and a label) vary, so the ONLY thing moving between the four climate_* frames
## is the band the sim's cut points classify that temperature into.
func _climate_tile_fixture(temperature: float, terrain_label: String) -> Dictionary:
	var tile := _food_tile_fixture()
	tile["temperature"] = temperature
	tile["terrain_label"] = terrain_label
	return tile


## A patch a band has COMMITTED to one crop (flora roster S1) — the `_food_tile_fixture` basket, plus
## the two committed fields the sim sets when Cultivate/Sow takes. It deliberately KEEPS the
## composition list: the wire still carries the biome's basket, and the card must choose the Crop row
## over it rather than merely fall back to it when the basket is absent.
func _committed_crop_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["patch_committed_species"] = "wild_emmer"
	tile["patch_committed_display_name"] = "Wild Emmer"
	return tile

## The LONGEST basket the sim can produce — a navigable hex blends the valley's basket with the
## channel's fishery, so five named plants can share one tile (RollingHills carries four). The crop
## picker must fit and stay legible at that length, which is why the sizing case gets its own fixture
## rather than being judged on the 3-entry reference tile.
func _long_basket_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "display_name": "Wild Emmer", "share": 0.34,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 4.20,
			"cultivate_payoff": 1.35, "sow_payoff": 2.10},
		{"species": "hazel", "display_name": "Hazel", "share": 0.24,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.34, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.67, "sow_payoff": 0.0},
		{"species": "river_fish", "display_name": "River Fish", "share": 0.18,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "ground_nut", "display_name": "Ground Nut", "share": 0.14,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.90, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.45, "sow_payoff": 0.0},
		{"species": "oak_mast", "display_name": "Oak Mast", "share": 0.10,
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
	var tile := _food_tile_fixture()
	tile["terrain_label"] = "Rolling Hills"
	tile["patch_composition"] = [
		{"species": "wild_emmer", "display_name": "Wild Emmer", "share": 0.22,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 4.20,
			"cultivate_payoff": 1.35, "sow_payoff": 2.10},
		{"species": "hazel", "display_name": "Hazel", "share": 0.17,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 2.20, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 1.10, "sow_payoff": 0.0},
		{"species": "wild_tubers", "display_name": "Wild Tubers", "share": 0.14,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.70, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.85, "sow_payoff": 0.0},
		{"species": "river_fish", "display_name": "River Fish", "share": 0.13,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "ground_nut", "display_name": "Ground Nut", "share": 0.11,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 1.44, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.72, "sow_payoff": 0.0},
		{"species": "berry_scrub", "display_name": "Berry Scrub", "share": 0.09,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.90, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.45, "sow_payoff": 0.0},
		{"species": "oak_mast", "display_name": "Oak Mast", "share": 0.08,
			"can_cultivate": false, "can_sow": false,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0},
		{"species": "marsh_reed", "display_name": "Marsh Reed", "share": 0.06,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.70, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.35, "sow_payoff": 0.0},
	]
	return tile


func _marginal_basket_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["terrain_label"] = "Rolling Hills"
	tile["patch_composition"] = [
		{"species": "hazel", "display_name": "Hazel", "share": 0.34,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.94, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.47, "sow_payoff": 0.0},
		{"species": "wild_emmer", "display_name": "Wild Emmer", "share": 0.28,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 0.84, "sow_yield_ratio": 1.26,
			"cultivate_payoff": 0.42, "sow_payoff": 0.63},
		{"species": "wild_tubers", "display_name": "Wild Tubers", "share": 0.22,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.68, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.34, "sow_payoff": 0.0},
		{"species": "berry_scrub", "display_name": "Berry Scrub", "share": 0.16,
			"can_cultivate": true, "can_sow": false,
			"cultivate_yield_ratio": 0.49, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.25, "sow_payoff": 0.0},
	]
	return tile

## The same long basket on ground that will actually take seed — `Sow` is gated on the site, so the
## crop picker's rung-3 frame needs the sowable tile, not the reference one (a refused Sow falls back
## to Sustain and the picker would not render at all).
func _sowable_long_basket_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["patch_composition"] = _long_basket_tile_fixture()["patch_composition"]
	return tile

## A basket with a FODDER crop (Flora roster F3): Hay Grass pays HAY, not provisions, so its provisions
## payoff/ratio read 0 and the `N.N×` row would call it worthless. `sow_fodder_payoff` (1.8) is >0, so
## the picker shows "Hay Grass 30% · 1.8 hay" — valuable in its own account — while the provisions crop
## beside it (Wild Emmer, `sow_yield_ratio` 3.2) keeps its unchanged "70% · 3.2×" ratio. On sowable
## ground so both rows are `can_sow`-legal and pressable — a fodder crop is a legal, valuable choice.
func _fodder_basket_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "display_name": "Wild Emmer", "share": 0.70,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 3.20,
			"cultivate_payoff": 1.35, "sow_payoff": 1.60, "sow_fodder_payoff": 0.0},
		{"species": "hay_grass", "display_name": "Hay Grass", "share": 0.30,
			"can_cultivate": false, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0, "sow_fodder_payoff": 1.8},
	]
	return tile

## A basket with a CASH crop (Flora roster F4): Flax pays TRADE, not provisions or fodder, so its
## provisions payoff/ratio AND its fodder payoff read 0 and the `N.N×` row would call it worthless.
## `sow_trade_payoff` (2.4) is >0, so the picker shows "Flax 30% · 2.4 trade" — valuable in its own
## account — while the provisions crop beside it (Wild Emmer, `sow_yield_ratio` 3.2) keeps its unchanged
## "70% · 3.2×" ratio. On sowable ground so both rows are `can_sow`-legal and pressable — a cash crop is
## a legal, valuable choice. Field-rung only, exactly like fodder.
func _cash_basket_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["patch_composition"] = [
		{"species": "wild_emmer", "display_name": "Wild Emmer", "share": 0.70,
			"can_cultivate": true, "can_sow": true,
			"cultivate_yield_ratio": 2.70, "sow_yield_ratio": 3.20,
			"cultivate_payoff": 1.35, "sow_payoff": 1.60,
			"sow_fodder_payoff": 0.0, "sow_trade_payoff": 0.0},
		{"species": "flax", "display_name": "Flax", "share": 0.30,
			"can_cultivate": false, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0,
			"sow_fodder_payoff": 0.0, "sow_trade_payoff": 2.4},
	]
	return tile

## PER-TILE FLORA REALIZATION (Flora roster F4) — the SECOND Alluvial Plain tile. Same biome as
## `_cash_basket_tile_fixture` (both "Alluvial Plain"), but a DIFFERENT realized basket: two tiles of
## one biome no longer carry the uniform per-biome roster, they carry a seeded per-tile SUBSET. This
## one is cash-DOMINANT — Cotton 55% + Flax 45%, both cash crops paying trade — where its twin was
## grain-dominant (Wild Emmer 70% + Flax 30%). Rendered beside it, the pair is the visible proof that
## same-biome tiles realize different species/shares. A different coord so it reads as its own tile.
func _cash_variant_basket_tile_fixture() -> Dictionary:
	var tile := _sowable_tile_fixture()
	tile["x"] = 68
	tile["y"] = 12
	tile["patch_composition"] = [
		{"species": "cotton", "display_name": "Cotton", "share": 0.55,
			"can_cultivate": false, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0,
			"sow_fodder_payoff": 0.0, "sow_trade_payoff": 3.6},
		{"species": "flax", "display_name": "Flax", "share": 0.45,
			"can_cultivate": false, "can_sow": true,
			"cultivate_yield_ratio": 0.0, "sow_yield_ratio": 0.0,
			"cultivate_payoff": 0.0, "sow_payoff": 0.0,
			"sow_fodder_payoff": 0.0, "sow_trade_payoff": 2.4},
	]
	return tile


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

## A herd carrying the two DIFFERENT things the sim exports for the two DIFFERENT actors:
##   `hunt_policy_ceilings` — the BAND's renewable FLOW ceiling {policy → provisions/turn}. The local
##       hunt preview is pure arithmetic over it (Sustain's entry IS the herd's sustainable yield).
##   `hunt_trip_estimates` — the sim's forward-SIMULATED expedition trip answers, keyed
##       `"<policy>:<party_workers>"` → `{turns_to_fill, delivers_food, delivers_trade, …}`. An
##       expedition's trip is NOT a rate division (on Surplus/Deplete the ceiling is a *stock* the party
##       strips in a turn or two, then it crawls at the regrowth trickle), so the client looks the answer
##       up and does no math. `turns_to_fill == 0` → won't fill within the horizon; `delivers_food ==
##       false` says the QUARRY IS INEDIBLE (#337), and only `delivers_food AND delivers_trade` both
##       false is a denial mission — the raid banks whichever half the species pays.
## `trip_turns` is the simulated turns-to-fill for the 4-worker party these states dial in.
func _forecast_herd(id: String, species: String, phase: String, sustain_ceiling: float,
		trip_turns: int = 0, surplus_trip_turns: int = 0,
		sustain_animals: int = 0, surplus_animals: int = 0) -> Dictionary:
	# A CLEAN raid: the party hauls its whole kill home, so delivered_food = animals × food_per_animal
	# and nothing rots. `delivered_food` is now the PRIMARY payload the client headlines (and the field
	# the "too lean" test / max-useful scan read), so every fixture cell must carry it; a partial-with-
	# waste cell is built explicitly (see `_partial_waste_mammoth`).
	var fpa := 2.0
	var sustain_delivered := float(sustain_animals) * fpa
	var surplus_delivered := float(surplus_animals) * fpa
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
		# kill-rhythm on the local-hunt preview (food ÷ food). Matches `fpa` above (the clean delivered).
		"food_per_animal": fpa,
		# A LIVE herd carries BOTH forecast field sets, so this fixture must too (they were split
		# across two disjoint fixtures once, which hid every interaction between them):
		#   • `per_worker_yield` + the `hunt_policy_ceilings` table, which drive the shared
		#     `SourceForecast.forecast_inputs` → cap + "Expected yield" / "Preparing → then" row, and
		#   • `hunt_trip_estimates` below (the sim's forward-simulated EXPEDITION trip answers).
		# Per-worker matches the band's `hunt_per_worker_provisions` (0.8) and the ceilings ARE the
		# band ceilings, because the sim exports one hunt model — the two paths must agree.
		"per_worker_yield": 0.8,
		# Eradicate's ceiling was `0.0` — the retired "denial yields nothing" premise written as a number,
		# which rendered the rung's picker face as `💀 +0.00` and its local preview as a zero take. #337
		# pays every rung the species' vector, and Eradicate empties the standing stock, so it is the
		# DEEPEST floor and frees the most: 8× the Sustain flow here.
		"hunt_policy_ceilings": {
			"sustain": sustain_ceiling,
			"surplus": sustain_ceiling * 4.0,
			"deplete": sustain_ceiling * 2.0,
			"eradicate": sustain_ceiling * 8.0,
		},
		# The trade half of each rung's ceiling (issue #337), a fixed fraction of the food one.
		"trade_per_animal": fpa * 0.15,
		"per_worker_trade": 0.12,
		"hunt_policy_trade_ceilings": {
			"sustain": sustain_ceiling * 0.15,
			"surplus": sustain_ceiling * 4.0 * 0.15,
			"deplete": sustain_ceiling * 2.0 * 0.15,
			"eradicate": sustain_ceiling * 8.0 * 0.15,
		},
		"hunt_trip_estimates": {
			"sustain:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": sustain_animals,
				"delivered_food": sustain_delivered,
				"delivered_trade": float(sustain_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
			},
			"surplus:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
			},
			"deplete:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": surplus_trip_turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
			},
			# Eradicate DELIVERS (issue #337): `delivers_food` says the quarry is EDIBLE, not that the
			# rung is a denial mission, and an Eradicate raid banks the whole-stock windfall.
			"eradicate:%d" % HUNT_FORECAST_PARTY: {
				"turns_to_fill": 0, "delivers_food": true, "delivers_trade": true,
				"animals_taken": surplus_animals,
				"delivered_food": surplus_delivered,
				"delivered_trade": float(surplus_animals) * RAID_TRADE_PER_ANIMAL, "wasted_food": 0.0,
			},
		},
	}

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
	tile["patch_ceiling_deplete"] = tile["patch_per_worker_yield"]
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
	tile["patch_ceiling_deplete"] = tile["patch_per_worker_yield"]
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

## THE BOTH-PRODUCTS INVESTMENT PAYOFF (issue #397) — a Wild Boar, edible AND worth its hide/bristles,
## on a pen-ceiling species so BOTH investment rungs are offered. Its four extractive rungs already pay
## a pair; the numbers here are the OTHER pair, the one the payoff faces render:
##   Tame   → `pastoral_yield` 1.48 food + `pastoral_trade` 0.37 trade  ⇒ `→ 1.48 food · 0.37 trade`
##   Corral → `corral_yield`   2.95 food + `corral_trade`   0.74 trade  ⇒ `→ 2.95 food · 0.74 trade`
## The food halves are the boar figures from the issue's own report (where the faces read `→ 1.48 food`
## and `→ 2.95 food` and the trade halves were dropped); each trade half is a quarter of its food half,
## the boar's hide-to-meat ratio, so the pair ascends together up the ladder exactly as the extractive
## caps do. Ordering is the ladder's: Sustain (0.90) < Tame (1.48) < Corral (2.95).
## `domestication` stays mid-ladder (0.4) because Tame RETIRES from the picker at full domestication
## while Corral is gated below it — the only way both rungs appear at once, with Corral greyed and
## still wearing its payoff.
func _investment_pair_boar_herd() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["id"] = "game_boar_11"
	fixture["label"] = "Wild Boar (game_boar_11)"
	fixture["species"] = "Wild Boar"
	fixture["size_class"] = "medium"
	fixture["pastoral_yield"] = 1.48
	fixture["pastoral_trade"] = 0.37
	fixture["corral_yield"] = 2.95
	fixture["corral_trade"] = 0.74
	return fixture

## The same herd, STRESSED — the "why isn't my Tame progressing?" case. Taming accrues only while the
## herd is Thriving, but the verb is NOT gated on it (a herd's phase swings as you hunt it): the sim
## just PAUSES the meter. Nothing else in the HUD would tell the player, so the drawer must.
func _taming_stalled_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["ecology_phase"] = "stressed"
	return fixture

## A still-WILD but tameable herd (pen ceiling) for the taming-startup-lag guard. It is NOT yet managed,
## so its OWNERSHIP-GATED `herders_needed` is 0 — but its ownership-INDEPENDENT would-be herder crew
## (`herders_needed_if_managed`, from biomass) is 10, set DELIBERATELY ABOVE this herd's Sustain
## take-useful (7, driven by the carry model) so the "no leak" companion is meaningful: composing Tame
## floors the cap UP to the 10-crew, while composing the extractive Sustain must stay at its own 7 — a
## crew-floor leak into Sustain would instead bump it to 10, which the companion asserts does NOT happen.
func _tame_worker_cap_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["herders_needed"] = 0
	fixture["herders_needed_if_managed"] = 10
	return fixture

## A nearly-tamed herd, FULLY STAFFED — the calm control for the staffing readout, AND the fix for the
## stale-count bug (fauna neglect-escape arc). `_band_fixture` has 4 herders (a Hunt assignment) on
## game_deer_07, and this herd needs 4, so the drawer reads a neutral "Herders: 4 / 4" — from the
## ACTUAL assigned count, not `herded_fraction`. `herded_fraction` is deliberately left STALE at 0.4
## (last turn's resolved value): the OLD code reconstructed `round(0.4 · 4) = 2` and wrongly read a
## self-contradictory "2 / 4 — under-herded", which this frame proves is gone.
func _fully_herded_herd_fixture() -> Dictionary:
	var fixture := _taming_herd_fixture()
	fixture["domestication"] = 0.9
	_set_managed_herders(fixture, 4)
	fixture["herded_fraction"] = 0.4
	return fixture

## The SAME herd, UNDER-HERDED — animals are drifting off (fauna neglect-escape arc; neglect no longer
## decays tameness, it sheds whole animals to the wild). The herd now needs 6 herders but `_band_fixture`
## only staffs 4, so the drawer reads the amber "Herders: 4 / 6 — under-herded" (the ACTUAL staffed
## count) plus the muted "Under-herded — animals are drifting off. Staff all 6 herders to hold the herd."
## line — NEVER the retired "tameness slipping" copy. `herded_fraction` is left STALE at 1.0: the OLD
## code reconstructed `round(1.0 · 6) = 6` and read a calm "6 / 6" with NO warning at all — the exact
## stale-reading this fix removes.
func _under_herded_herd_fixture() -> Dictionary:
	var fixture := _fully_herded_herd_fixture()
	fixture["domestication"] = 0.98
	_set_managed_herders(fixture, 6)
	fixture["herded_fraction"] = 1.0
	return fixture

## Set BOTH herder counts on a MANAGED herd fixture. The sim exports them EQUAL there (see the
## field-pair guard above `_guard_herd_fields`), and setting them one at a time is precisely the
## mistake the guard exists to catch — so managed fixtures set them together, through this.
## A still-WILD but tameable herd is the one case where they differ and writes them by hand
## (`_tame_worker_cap_herd_fixture`: gated 0, would-be 10).
func _set_managed_herders(fixture: Dictionary, needed: int) -> void:
	fixture[HERDERS_NEEDED_KEY] = needed
	fixture[HERDERS_NEEDED_IF_MANAGED_KEY] = needed

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

## A band worked on TWO hunt sources — the render-honesty frame for the summary row's honest per-turn
## FOOD rate (fix #1) and the under-crewed `wastedYield` note (fix #5). Row 1 is a FAST animal; row 2 a
## BIG animal whose `actualYield` is 0.00 THIS turn — the "+0.00 /turn" lie the row used to headline —
## and which is under-crewed, so the muted "· N wasted" note shows. Neither row shows a `≈… /turn`
## animals-per-turn cadence: on a summary row the sustainable food rate is enough.
func _hunt_actions_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["labor_assignments"] = [
		# Fast: honest rate 2.60/turn. A Sustain animal → the sim-answered `overdraws` is false (no ⚠).
		{"kind": "hunt", "workers": 3, "fauna_id": "game_fowl_01", "policy": "sustain",
			"target_x": 71, "target_y": 18, "actual_yield": 2.60, "sustainable_yield": 2.60,
			"workers_needed": 3, "overdraws": false},
		# Big: honest rate 2.40/turn (the sim's measured Mammoth Sustain). actual_yield 0.00 = a wait turn
		# of the kill pulse (the old lie the row used to headline). Under-crewed → the muted "· 1.9 wasted".
		# Sustain → overdraws false, so no ⚠.
		{"kind": "hunt", "workers": 2, "fauna_id": "game_mammoth_01", "policy": "sustain",
			"target_x": 70, "target_y": 17, "actual_yield": 0.00, "sustainable_yield": 2.40,
			"workers_needed": 5, "wasted_yield": 1.9, "overdraws": false},
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
		# Pre-commit yield forecast (food/turn at this herd's biomass, at output_multiplier 1.0).
		# Sustain admits ceil(0.90 / 0.30) = 3 useful hunters, below the reference band's 7 assignable
		# (3 idle + the 4 it already has on this herd), so the Hunters stepper caps at 3 with the
		# "max 3 workers useful here" note.
		"per_worker_yield": 0.30,
		# The two INVESTMENT rungs' PAYOFFS — the food/turn each rung pays ONCE prepared (the pastoral
		# MSY after taming, the pen's sustained rate once built), NOT the during-build dip. Ordered
		# Sustain (0.90) < Tame (1.20) < Corral (1.50) so the picker's `→ +Y/turn` payoff buttons read
		# as an ascending ladder, both clearly above Sustain's `up to +0.90/turn` cap.
		"corral_yield": 1.50,
		"pastoral_yield": 1.20,
		"corral_progress": 0.0,
		# EVERY ceiling — the four extractive rungs plus the Tame/Corral DIPS — rides this ONE list;
		# the herd has no flat `ceiling*` scalars on the wire any more (deprecated schema slots). The
		# sim exports a row for every one of the six `FollowPolicy::HUNT_POLICIES`, so this is the
		# shape the decoder produces and where `SourceForecast.forecast_inputs` reads every herd ceiling.
		"hunt_policy_ceilings": {
			"sustain": 0.90,
			"surplus": 1.80,
			"deplete": 2.70,
			"eradicate": 4.50,
			"tame": 0.23,
			"corral": 0.23,
		},
		# The TRADE half of the same list (issue #337) — the decoder fills both dicts in one pass over
		# the one wire list, so a fixture that carries the food rows must carry these or the picker
		# under-reports what the rung pays.
		"trade_per_animal": 0.30,
		"per_worker_trade": 0.05,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.14,
			"surplus": 0.27,
			"deplete": 0.41,
			"eradicate": 0.68,
			"tame": 0.03,
			"corral": 0.03,
		},
		"tile_info": _food_tile_fixture(),
	}

## A DEADLY-TO-HUNT herd (Predators Phase 0): a woolly mammoth — high attack (8) and high ferocity
## (0.9, it fights back), but aggression 0 (a grazer never attacks unprovoked). Its drawer shows high
## Attack + Fights back bars and an EMPTY Aggressive bar — the split that proves strength ≠ danger.
## Compact tile so the component/husbandry rows land in-frame.
func _deadly_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["id"] = "game_mammoth_02"
	fixture["label"] = "Woolly Mammoth (game_mammoth_02)"
	fixture["species"] = "Woolly Mammoth"
	fixture["husbandry_ceiling"] = "wild"
	fixture["attack"] = 8.0
	fixture["defense"] = 12.0
	fixture["ferocity"] = 0.9
	fixture["aggression"] = 0.0
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## A PREDATOR (Predators Phase 1a): a Grey Wolf Pack — big, wild-ceiling, carnivore. `prey_sense_radius`
## 4 (`> 0`) is BOTH the "this is a predator" signal AND the map ring radius, so the drawer must read
## "Size: Big predator" (not "Big game") and "Wild predator — hunt only" (not "Wild game — hunt only").
func _predator_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
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
	fixture["tile_info"] = _compact_herd_tile_fixture()
	return fixture

## Predators Phase 3 — a band UNDER an active raid, both legibility surfaces lit at once:
##   • `raid_radius` 3 (the sim's echoed `predators.raid_radius`) + a VISIBLE camp-menacing predator
##     placed one tile off the band's [71,18] in the world-herd list (`_raiding_predator_herd_fixture`)
##     → the Warrior card's live crimson "⚠ Predator nearby — N on guard" alert.
##   • `raid_forfeit` 1.20 (`PopulationCohortState.raidForfeit`, food lost to raids THIS turn) → the
##     "⚔ Lost to raids −1.20" food-ledger row and a net dragged negative the turn the raid landed.
## Reuses entity 904, so `BAND_DISCLOSURE_FOOD` opens its ledger popover.
func _raided_band_fixture() -> Dictionary:
	var band := _band_fixture()
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
	var prefix := "%s: " % key
	for line in lines:
		var text := String(line)
		if text.begins_with(prefix):
			return text.substr(prefix.length())
	return ""

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
##   Deplete ceiling 1.86: two bodies drop on the peak turn → ceil((floor(1.86/1.6)+1) × 1.6 / 0.80) =
##     ceil(3.2 / 0.80) = 4 → the cap tracks the selected policy's ceiling upward.
func _aurochs_big_game_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["id"] = "game_aurochs_04"
	fixture["label"] = "Wild Aurochs (game_aurochs_04)"
	fixture["species"] = "Wild Aurochs"
	fixture["husbandry_ceiling"] = "wild"
	fixture["food_per_animal"] = 1.6
	fixture["per_worker_yield"] = 0.80
	fixture["hunt_policy_ceilings"] = {
		"sustain": 0.74, "surplus": 1.20, "deplete": 1.86, "eradicate": 2.60,
	}
	fixture["trade_per_animal"] = 0.24
	fixture["per_worker_trade"] = 0.12
	fixture["hunt_policy_trade_ceilings"] = {
		"sustain": 0.11, "surplus": 0.18, "deplete": 0.28, "eradicate": 0.39,
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

## Ground that offers NOTHING to gather: no food module, no patch. The land row's meta must read
## "No forage" (not a blank), and the drawer must carry terrain rows with no compose block.
func _barren_tile_fixture() -> Dictionary:
	return {
		"x": 71, "y": 4,
		"terrain_label": "Rocky Regolith",
		"tags_text": "none",
		"visibility_state": "active",
		"habitability": 0.07,
		"temperature": 2.0,
		"food_module": "",
		"food_module_label": "",
		"height_display": "62 ▮▮▮▮▮▯▯▯",
	}

## THE CROWDED HEX — 3 bands + 2 herds, i.e. six subject rows once the land is counted. The state
## the height cap is judged on: every row visible, the drawer capped, the dock not scrolling.
func _crowded_tile_fixture() -> Dictionary:
	var tile := _food_tile_fixture()
	tile["x"] = 58
	tile["y"] = 24
	tile["units"] = _crowded_bands_fixture()
	tile["herds"] = _crowded_herds_fixture()
	return tile

## Three player bands on the crowded hex, spanning the food tiers (green / amber / red dots) and
## carrying real labor so the auto-selected band's drawer renders a full allocation block — which is
## what makes the cap do any work at all.
func _crowded_bands_fixture() -> Array:
	return [
		{"id": "Band Fen", "entity": 301, "faction": 0, "size": 120, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 62, "idle_workers": 9,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 15.0, "morale": 0.72,
			"activity": "forage", "stores": {"provisions": 180.0},
			"food_income": 3.2, "food_consumption": 2.4,
			"labor_assignments": [
				{"kind": "forage", "workers": 5, "target_x": 58, "target_y": 24, "policy": "sustain",
					"actual_yield": 0.96, "sustainable_yield": 0.96, "realized_yield": 0.96,
					"workers_needed": 5, "overdraws": false},
			]},
		{"id": "Band Ash", "entity": 302, "faction": 0, "size": 86, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 44, "idle_workers": 4,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 7.0, "morale": 0.51,
			"activity": "scout", "stores": {"provisions": 40.0}, "labor_assignments": []},
		{"id": "Band Bryn", "entity": 303, "faction": 0, "size": 54, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 27, "idle_workers": 0,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 2.0, "morale": 0.30,
			"activity": "idle", "stores": {"provisions": 8.0}, "labor_assignments": []},
	]

## Two herds sharing the crowded hex — a stressed bison (amber dot) and a thriving boar (green), so
## the Wildlife group is genuinely plural and the ecology dots differ down the list.
func _crowded_herds_fixture() -> Array:
	return [
		_occupied_herd_only(),
		{
			"id": "game_boar_04",
			"label": "Wild Boar (game_boar_04)",
			"species": "Wild Boar",
			"size_class": "medium",
			"huntable": true,
			"ecology_phase": "thriving",
			"domestication": 0.0,
			"biomass": 1010.0,
			"carrying_capacity": 1433.0,
			"graze_range_radius": 1,
			"x": 58, "y": 24,
		},
	]

## The MapView snapshot behind `tile_panel_land_sticky` — the crowded hex's OWN bands and herds on a
## grid just big enough to hold it, so MapView's `_tile_info_at` / `_units_on_tile` see exactly what
## the HUD fixture describes. Nothing is redacted because the caller turns FoW OFF explicitly — a
## fresh MapView now defaults to fog ON, and this fixture carries no visibility raster, so every
## occupant would be gated out and the assertion would pass on an empty hex.
func _sticky_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(STICKY_GRID_W * STICKY_GRID_H)
	terrain.fill(STICKY_TERRAIN_ID)
	return {
		"grid": {"width": STICKY_GRID_W, "height": STICKY_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": _crowded_bands_fixture(),
		"herds": _crowded_herds_fixture(),
	}

## The MapView snapshot behind `tile_panel_deselect_keeps_tile` — ONE herd and no bands, so the first
## click resolves as a herd rather than a band (a band would exercise `selected_unit_id`, the other
## half of the same clear branch, but not the herd case the issue was reported on) and `DESELECT_LAND_TILE`
## is genuinely bare. Same grid as the sticky fixture; fog is turned off by the caller.
func _deselect_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(STICKY_GRID_W * STICKY_GRID_H)
	terrain.fill(STICKY_TERRAIN_ID)
	return {
		"grid": {"width": STICKY_GRID_W, "height": STICKY_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [],
		"herds": [{
			"id": "game_deer_405",
			"label": "Red Deer (game_deer_405)",
			"species": "Red Deer",
			"size_class": "big",
			"huntable": true,
			"ecology_phase": "thriving",
			"domestication": 0.0,
			"biomass": 1480.0,
			"carrying_capacity": 2150.0,
			"graze_range_radius": 1,
			"x": DESELECT_HERD_TILE.x, "y": DESELECT_HERD_TILE.y,
		}],
	}

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
			"turns_of_food": 15.0, "activity": "harvest", "stores": {"provisions": 180.0}},
		{"id": "Band Ash", "entity": 302, "faction": 0, "size": 86, "pos": [58, 24],
			"turns_of_food": 7.0, "activity": "scout", "stores": {"provisions": 40.0}},
		{"id": "Band Bryn", "entity": 303, "faction": 0, "size": 54, "pos": [58, 24],
			"turns_of_food": 2.0, "activity": "idle", "stores": {"provisions": 8.0}},
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
	var fixture := _corral_ready_herd_fixture()
	# Corral is an INVESTMENT rung, and `_forecast_worker_cap`'s floor reads the ownership-INDEPENDENT
	# `herders_needed_if_managed` for those (`herders_needed` is the extractive rungs' field). The sim
	# exports the two EQUAL on an owned herd, which this one is — so a fixture setting only the first
	# floors the cap at 0 and the frame silently renders "max 1 worker useful", the very cap it exists
	# to disprove. It went unnoticed because the state used to compose Sustain by accident (#357), and
	# it is now caught by the field-pair guard rather than by a reader noticing the wrong number.
	_set_managed_herders(fixture, UNDER_HERDED_CORRAL_HERDERS_NEEDED)
	fixture["herded_fraction"] = 0.5
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
	# Grazing 2d-γ — a radius-1 pen on POOR footprint: its fenced land covers NONE of the feed
	# (`pen_pasture_fraction` 0.0), so the whole GROSS demand falls on the FOOD larder as the net bill
	# (`pen_larder_bill` == gross, no hay). Feed-split reads "Fed by pasture 0% · larder 1.7 food/turn".
	# Invariant: gross × pasture(0) + hay(0) + larder(1.74) == gross(1.74).
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_pasture_fraction"] = 0.0
	fixture["pen_larder_bill"] = PEN_UPKEEP_RED_DEER
	fixture["pen_hay_food"] = 0.0
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
	# Override the inherited ceiling table's two rows this frame reads — Sustain (the extractive
	# baseline) and the Corral DIP. The herd's ceilings live only in `hunt_policy_ceilings` now, so
	# a depleted variant must restate them here rather than shadowing them with flat scalars.
	var depleted_ceilings: Dictionary = (fixture["hunt_policy_ceilings"] as Dictionary).duplicate()
	depleted_ceilings["sustain"] = 0.10
	depleted_ceilings["corral"] = 0.05
	fixture["hunt_policy_ceilings"] = depleted_ceilings
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
## covers the herd's entire feed, so `pen_pasture_fraction` 1.0 and the NET larder bill `pen_larder_bill`
## is 0 (the GROSS `pen_upkeep` stays 1.74). The feed-split row reads "Fed by pasture 100% · larder 0.0
## food/turn" and the amber Pen-feed
## debit row disappears (nothing left to haul). This is the state the Extend-pen affordance renders on —
## a built pen, no ring in flight (`pen_extend_progress` 0), so `_build_herd_assign_controls` shows the
## "Extend pen" button.
func _self_feeding_pen_herd_fixture() -> Dictionary:
	var fixture := _domesticated_herd_fixture()
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
	var fixture := _domesticated_herd_fixture()
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
	var fixture := _domesticated_herd_fixture()
	fixture["pen_radius"] = 1
	fixture["pen_footprint_tiles"] = 7
	fixture["pen_upkeep"] = 2.0        # realistic GROSS (upkeep_per_biomass × biomass scale)
	fixture["pen_pasture_fraction"] = 0.40
	fixture["pen_hay_food"] = 0.90
	fixture["pen_larder_bill"] = 0.30  # 2.0 − (2.0 × 0.40) − 0.90 == 0.30
	fixture["pen_extend_progress"] = 0.0
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
