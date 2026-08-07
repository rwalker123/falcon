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

## The chapters of the state walk, in RUN ORDER — the single place that order is stated.
##
## Adding a state means editing ONE chapter; adding a chapter means adding ONE line here. That is
## the whole point of the split: two worktrees working different arcs no longer edit one file.
##
## **PATHS, NOT `preload`s, AND THAT IS THE FIX FOR A HANG.** A `preload` of a chapter carrying a
## parse error is a parse error in THIS file: the engine answers `Could not preload resource script`
## and then `Failed to load script "res://tools/ui_preview.gd"`, so the scene's root node comes up
## with **no script at all**, `_ready` never runs, no PNG is written, no `FAIL` is printed and
## `get_tree().quit()` is never reached — the process just sits there (measured once at 59 minutes)
## while a stale, plausible-looking frame set from the previous run stays on disk. Loading them at
## runtime instead keeps a broken chapter INSIDE the harness's own error handling, where
## `_instantiate_chapters` can name it, print the `FAIL` token and exit non-zero before a single
## frame is written. The compile check the `preload`s gave is not lost — every chapter is still
## loaded and instantiated, just up front and with the failure reported rather than fatal.
const CHAPTERS := [
	"res://tools/ui_preview/chapters/band_expedition.gd",
	"res://tools/ui_preview/chapters/land_readouts.gd",
	"res://tools/ui_preview/chapters/forage_crop.gd",
	"res://tools/ui_preview/chapters/forage_accounts.gd",
	"res://tools/ui_preview/chapters/improvements.gd",
	"res://tools/ui_preview/chapters/sight_fog.gd",
	"res://tools/ui_preview/chapters/herd_graze_pen.gd",
	"res://tools/ui_preview/chapters/herd_improve.gd",
	"res://tools/ui_preview/chapters/hunt.gd",
	"res://tools/ui_preview/chapters/tile_panel.gd",
	"res://tools/ui_preview/chapters/turn_orb.gd",
	"res://tools/ui_preview/chapters/docks_legend.gd",
	"res://tools/ui_preview/chapters/telling.gd",
	"res://tools/ui_preview/chapters/compose_rungs.gd",
	"res://tools/ui_preview/chapters/world_reset.gd",
	"res://tools/ui_preview/chapters/event_dock.gd",
	"res://tools/ui_preview/chapters/button_faces.gd",
]

## The one method a chapter owes the harness (see the chapter contract in
## `.claude/rules/client/test-harnesses.md`). Checked up front, because a chapter that loads but
## cannot be driven is the same lost run as one that does not load.
const CHAPTER_ENTRY_METHOD := "run"

## The hang guard, a SIBLING node in `ui_preview.tscn` rather than anything this script owns — see
## `preview_watchdog.gd` for why that placement is the whole point.
const WATCHDOG_NODE := "Watchdog"
const WATCHDOG_PROGRESS_METHOD := "note_progress"

## The run's exit status. **A clean run exits 0 and a run with any `FAIL` in it exits non-zero**, so
## the status and the output agree; nothing in `xtask`, CI or `scripts/` consumed this harness's
## status before (only agents reading its output did), so there was no consumer to break.
const EXIT_OK := 0
const EXIT_FAILED := 1

const Spine := preload("res://tools/ui_preview/compose_vocab.gd")
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")


const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")
## Scratch prefs file for this harness — NEVER the player's `user://narrative.cfg`. See the
## prefs-isolation block in `_ready()` for the incident that made this non-negotiable.
const PREVIEW_PREFS_PATH := "user://ui_preview_prefs.cfg"
## Scratch DOCK prefs for the `BandCityPanel` this harness injects — NEVER the player's
## `user://band_city_dock.cfg`. A second file because the panel keeps its own (edge / collapsed / tab).
const PREVIEW_DOCK_PREFS_PATH := "user://ui_preview_dock_prefs.cfg"
## Scratch prefs for the `EventDockPanel` — a THIRD scratch file, for the same reason as the second:
## the dock reads its edge / row count / detail floor on construction and writes them back as the
## harness walks them, so without this a frame would depend on what the last run left behind AND the
## walk would land in the developer's real `user://narrative.cfg`.
const PREVIEW_EVENT_PREFS_PATH := "user://ui_preview_event_prefs.cfg"
# Force-compile MapView here so the harness also acts as a full-context compile
# check for it (autoloads are registered when the harness runs as a scene, which
# --check-only cannot do).
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")
## Preloaded for its STATICS alone — Main is never instanced here. Each was extracted so an ORDER or a
## wording `Main` owns can be asserted without standing up the whole app scene: `escape_claimant` (the
## ESC precedence chain), `format_abandon_improvement`, and `apply_event_dock_frame` (the event dock's
## reset → current turn → retention → ingest sequence).
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
# Park the OS cursor over empty canvas before rendering. The HUD drops its hovered-hex record (and
# with it the targeting banner's hunt forecast) whenever the pointer sits over an interactive HUD
# control — see Hud._suppress_tooltip_over_ui. Wherever the cursor happened to be when the harness
# launched would otherwise decide whether the hover states render, making them non-deterministic.
const MOUSE_PARK_POSITION := Vector2(750, 640)


var _hud: HudLayer

## The hang guard from `ui_preview.tscn`, or `null` if the scene has lost the node — the harness must
## still run in that case, since the guard is a safety net and not a dependency.
var _watchdog: Node = null

## Every `FAIL` this run has printed, from any of the sinks below. **The exit status is derived from
## it**, so the two signals a reader might use — `grep FAIL` and `$?` — can never disagree.
var _failures := 0

## Every compose spine captured this run, keyed by the sheet it came from (see `_record_compose_spine`).
## A DICT rather than two fields because the parity assertion is about the RELATION between them, and a
## missing capture must fail loudly rather than compare an empty array against another empty one.
var _compose_spines := {}

## The drawn-down hay meadow staged by `floor_chart_drawn_down`, HELD for the herd-Allee chapter to
## read its plant curve against — the one frame-to-frame handoff that crosses a chapter boundary.
##
## A member rather than a local because it is genuinely shared: the herd chart's claim is that a herd
## below its Allee threshold falls where a PATCH at a low floor merely holds, and the patch half of
## that comparison is this exact dictionary. It cannot be rebuilt at the point of use either —
## `floorify` mutates in place and the harness floorifies this dict several times on its way through
## the render, so a freshly-built twin would not be the object the earlier frames were judged on.
var _floor_chart_drawn_patch := {}


## The harness's ONE gate into the HUD for a source fixture: everything goes through `ForageFx.floorify`
## first, so no state can accidentally hand the panel a retired per-stance table (which would render
## as a silent zero rather than as a failure).
func _show_herd(herd: Dictionary) -> void:
	_hud.show_herd_selection(ForageFx.floorify(herd))

func _show_tile(tile: Dictionary) -> void:
	_hud.show_tile_selection(ForageFx.floorify(tile, HudComposeVocab.FORAGE_FORECAST_PREFIX))

func _set_world_herds(herds: Array) -> void:
	for h in herds:
		if h is Dictionary:
			ForageFx.floorify(h)
	_hud.update_herds(herds)


## The hang guard from the scene, checked for its method rather than assumed: calling a missing
## method on an untyped `Node` is a runtime error, and an error raised HERE would abort `_ready`
## exactly the way the guard exists to survive.
func _resolve_watchdog() -> Node:
	var node := get_node_or_null(WATCHDOG_NODE)
	if node != null and node.has_method(WATCHDOG_PROGRESS_METHOD):
		return node
	push_warning(("ui_preview: no %s node in the scene — the run has NO hang guard. Restore it from "
		+ "tools/ui_preview.tscn (see preview_watchdog.gd).") % WATCHDOG_NODE)
	return null

## A sign of life for the hang guard. Called as each chapter starts and each frame is saved, which is
## every few hundred milliseconds in a healthy run.
func _note_progress() -> void:
	if _watchdog != null:
		_watchdog.note_progress()


## Load and instantiate every chapter, reporting each failure rather than dying of it.
##
## Returns the drivable chapters in `CHAPTERS` order; `_failures` says whether the roster is whole,
## and the caller must NOT walk a partial one — a run missing a chapter renders a frame set whose
## gaps are invisible on disk, which is the same lie as a half-written one.
func _instantiate_chapters() -> Array:
	var chapters: Array = []
	for path in CHAPTERS:
		# **`can_instantiate()` IS THE TEST, AND A NULL CHECK IS NOT.** `load` on a chapter that does
		# not compile prints the engine's own `Parse Error` lines and then answers a NON-null,
		# non-functional `GDScript` — calling `new()` on it raises `Nonexistent function 'new'`,
		# which ABORTS this function, and GDScript answers an aborted call with the return type's
		# default. The caller would then walk an EMPTY roster and report a clean run (measured:
		# 1 PNG, no FAIL, exit 0). Ask whether the script can be instantiated instead.
		var script: Resource = ResourceLoader.load(path) if ResourceLoader.exists(path) else null
		if script == null or not (script is GDScript) or not (script as GDScript).can_instantiate():
			_fail(("chapter — %s did not load. Either the file is missing or it does not compile; "
				+ "the engine's Parse Error lines above name the line. No frame has been rendered.")
				% path)
			continue
		var chapter: Object = (script as GDScript).new()
		if chapter == null:
			_fail("chapter — %s loaded but could not be instantiated. No frame has been rendered." % path)
			continue
		if not chapter.has_method(CHAPTER_ENTRY_METHOD):
			_fail(("chapter — %s has no `%s(harness)`, so the walk cannot drive it. No frame has been "
				+ "rendered.") % [path, CHAPTER_ENTRY_METHOD])
			continue
		chapters.append(chapter)
	return chapters


## The ONE failure sink, so `_failures` cannot drift from what was printed. Every caller passes the
## text AFTER the `FAIL` token, which is what the output scanning keys on.
func _fail(message: String) -> void:
	_failures += 1
	push_error("ui_preview: FAIL %s" % message)


## **THE ONLY WAY OUT OF THIS HARNESS.** Every path that ends the run comes through here, so the
## status is derived from the run's own tally in exactly one place and the hang guard is stood down
## before shutdown (a slow teardown is not a stall).
func _finish() -> void:
	if _watchdog != null:
		_watchdog.disarm()
	if _failures > 0:
		print("ui_preview: RUN FAILED — %d failure(s); see the FAIL lines above" % _failures)
	else:
		print("ui_preview: run complete — no failures")
	get_tree().quit(EXIT_FAILED if _failures > 0 else EXIT_OK)


func _ready() -> void:
	_watchdog = _resolve_watchdog()
	# **THE WHOLE CHAPTER ROSTER IS LOADED AND INSTANTIATED BEFORE ANYTHING RENDERS.** Discovering a
	# broken chapter partway through the walk leaves a HALF-WRITTEN frame set beside the previous
	# run's leftovers — 200-odd fresh PNGs and 70 stale ones, indistinguishable from a real run — so
	# the roster is proven drivable first and a bad one costs zero frames.
	var chapters := _instantiate_chapters()
	# **THE ROSTER IS CHECKED BY COUNT, not merely by the failures reported above**, because an
	# unforeseen runtime error INSIDE `_instantiate_chapters` would abort it — and GDScript answers
	# an aborted (non-coroutine) call with the return type's DEFAULT, so the caller sails on. That is
	# not hypothetical: the first cut of this fix called `new()` on a chapter that had not compiled,
	# which aborted the loop, returned an empty Array, and rendered ONE frame while printing no FAIL
	# and exiting 0. A short roster is a failed run whatever produced it.
	if _failures > 0 or chapters.size() != CHAPTERS.size():
		if _failures == 0:
			_fail(("chapters — only %d of %d chapters came back drivable, so the walk would render an "
				+ "incomplete frame set. No frame has been rendered.") % [chapters.size(), CHAPTERS.size()])
		_finish()
		return

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
	# **IGNORE, because this rect STANDS IN FOR THE MAP.** A `ColorRect` defaults to
	# `MOUSE_FILTER_STOP`, so as a full-rect backdrop it silently swallowed every press the HUD did
	# not take — which the real client has nothing equivalent to (`MapView` is a `Node2D` and picks
	# hexes out of `_unhandled_input`). It made the event dock's click-through test unrunnable: the
	# control press on open canvas never reached `_unhandled_input` either, so "the bar consumed it"
	# was true of everywhere.
	bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
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
	# THE THIRD prefs file, same two reasons: the event dock persists its edge, row count, detail
	# floor and channels, and the `event_dock_*` states walk all four.
	EventDockPanel.config_path_override = PREVIEW_EVENT_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_EVENT_PREFS_PATH))
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
	_hud._band_labor._player_band = BandFx.band_fixture()
	# The world's herds (Main pushes snapshot["herds"]): the Current-actions Hunt row reads the herd's
	# species from here and, when clicked, jumps to its LIVE tile (it has migrated away from the hunt
	# assignment's launch target).
	_set_world_herds(HerdFx.world_herds_fixture())
	# The world's KIT ROSTER (Main pushes snapshot["kits"] + the two job defaults): every compose
	# sheet's Kit picker is built from it, so it is prologue rather than chapter state — a roster
	# seeded per arc would give one chapter's sheets a picker and the next chapter's none.
	_hud.update_kit_roster(BandFx.kit_roster_fixture(),
		BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE)
	# The world's food modules (Main pushes snapshot["food_modules"]): each Forage row leads with the
	# module's map glyph, so the panel row and the map marker read as the same resource.
	_hud.update_food_modules([
		{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"},
	])

	# The state walk, in the ONE order that reproduces the frame set.
	for chapter in chapters:
		_note_progress()
		await chapter.run(self)

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

	_finish()


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


## Settle the HUD for a capture. `finish_tweens = false` is for the two callers that must NOT have
## every live tween driven to its end: the ONE state that must capture a page turn IN MOTION (it steps
## the tween itself, so the phase is chosen rather than raced), and the assertion blocks that settle
## layout WITHOUT capturing a frame — there is no screenshot to finish anything for, and flushing
## would fire tween-finished callbacks mid-suite and move frames captured later in the run.
func _settle(finish_tweens: bool = true) -> void:
	# The hang guard's sign of life. `_settle` rather than `_save`, because it is what EVERY state
	# reaches — including the PNG-less assertion blocks, which would otherwise look like a stall.
	_note_progress()
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


## Open the COMPOSE SHEET on a source and render its compose block there.
##
## Part 2 of docs/plan_tile_panel_layout.md moved `%ForageAssignControls` / `%HerdAssignControls` out
## of the drawer into a floating sheet, so a state that exists to judge the picker/stepper/forecast/
## gate-reasons has to OPEN it — the drawer now shows only the standing summary + `Assign … ▸`.
## These two calls replace the direct `_hud._build_*_assign_controls(...)` the states used before;
## the builders still run, just against the sheet's content container.
##
## **IT GOES THROUGH `ForageFx.floorify`, LIKE ITS HERD TWIN.** Most states pass a FRESH fixture here rather
## than the object `_show_tile` already converted, so the sheet was being built from a dict the
## adapter had never seen. That was invisible while the adapter only rewrote ceilings — the fixture
## builders seed those themselves — and stopped being invisible the moment the adapter also had to
## seed the growth terms: every compose sheet opened this way lost its chart.
func _compose_forage(tile_info: Dictionary) -> void:
	_hud._drawercompose.open_forage_compose(
		ForageFx.floorify(tile_info, HudComposeVocab.FORAGE_FORECAST_PREFIX))

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
## `improvement` is the SECOND AXIS (issue #442) — a build verb is no longer a value of `policy`, so a
## frame that used to dial `policy: "tame"` dials this instead, and may dial a STANCE beside it. The
## re-open contract is unchanged: dial after the first open, then re-open so the render sees it.
func _compose_herd(herd: Dictionary, count: int = Spine.COMPOSE_COUNT_UNSET,
		floor: float = ForageFx.COMPOSE_FLOOR_UNSET, improvement: String = "") -> void:
	# The compose sheet is where the pair actually BITES (`_forecast_worker_cap`'s floor), and a herd
	# can be composed without ever being the selected subject — so check the argument here too, not
	# only through the per-frame scan in `_save`.
	_guard_herd_fields(herd, "compose_herd")
	ForageFx.floorify(herd)
	_hud._drawercompose.open_herd_compose(herd)
	if count == Spine.COMPOSE_COUNT_UNSET and floor == ForageFx.COMPOSE_FLOOR_UNSET and improvement == "":
		return
	if count != Spine.COMPOSE_COUNT_UNSET:
		_hud._compose.set_hunt_count(count)
	if floor != ForageFx.COMPOSE_FLOOR_UNSET:
		_hud._compose.set_hunt_floor(floor)
	if improvement != "":
		_hud._compose.set_hunt_improvement(improvement)
	_hud._drawercompose.open_herd_compose(herd)


## The LAND drawer's `Assign … ▸` button. Found STRUCTURALLY — `%ForageAssignControls` holds at most a
## standing-summary `HFlowContainer` and this one Button (`build_forage_drawer_actions`) — for the same
## reason the identity finders in `node_query.gd` exist: its face carries the crew noun under test.
func _forage_open_button() -> Button:
	for child in _hud.forage_assign_controls.get_children():
		if child is Button:
			return child as Button
	return null


func _assert_abandon_emits(kind: String, improvement: String, want_line: String) -> void:
	var sheet := _hud._drawercompose._compose_sheet
	var box := ForageFx.find_improvement_control(sheet, improvement)
	if not (box is CheckBox):
		_assert_hud("abandon (%s): a running improvement box to uncheck" % kind, false)
		return
	var captured: Array[Dictionary] = []
	var sink := func(payload: Dictionary) -> void: captured.append(payload)
	_hud.improvement_requested.connect(sink)
	(box as CheckBox).button_pressed = false
	# **SETTLE BEFORE LOOKING FOR THE COMMIT BUTTON.** Unchecking rebuilds the whole control block,
	# and the builder clears its host with `queue_free()` — a DEFERRED removal, so until the frame
	# ends the stale nodes are still children and a synchronous search finds the OLD button first.
	# Pressing that one runs a closure built against the pre-uncheck composition, which emits nothing
	# (`composed == standing`) and reads exactly like "the abandon path is not wired up".
	await _settle()
	# By META, not by face: the forage commit's verb now follows the patch's rung (`Forage` on wild
	# ground, `Tend` on a managed one), so a text match here would encode an assumption about the
	# fixture's rung that has nothing to do with what this probe is testing.
	var commit := Q.compose_commit_button(sheet)
	if commit == null:
		_hud.improvement_requested.disconnect(sink)
		_assert_hud("abandon (%s): the sheet's commit button" % kind, false)
		return
	commit.pressed.emit()
	_hud.improvement_requested.disconnect(sink)
	if captured.is_empty():
		_assert_hud("abandon (%s): unchecking a running build emits a command" % kind, false)
		return
	var line := String(MAIN_SCRIPT.format_abandon_improvement(captured[0]).get("line", ""))
	print("ui_preview: abandon %s -> %s" % [kind, line])
	_assert_hud("unchecking a running %s build transmits `%s`" % [kind, want_line],
		line == want_line)
	# Committing also fired `assign_labor`, whose OPTIMISTIC pending entry would tint every later
	# frame's rows amber. Drop it — the probe is about the command, not about the overlay.
	_hud._band_labor._pending_labor.clear()


## Capture the open sheet's spine under `key`, and assert the shared HEAD on the spot so a failure
## names the sheet that broke rather than only the pair. An EMPTY spine fails too — a sheet that never
## opened would otherwise make the parity comparison vacuously true.
func _record_compose_spine(key: String) -> void:
	var spine := Spine.compose_spine(_hud._drawercompose._compose_sheet)
	_compose_spines[key] = spine
	_assert_hud("the %s compose sheet opens band → stance → crew (spine %s)" % [key, str(spine)],
		spine.slice(0, Spine.COMPOSE_SPINE_HEAD.size()) == Spine.COMPOSE_SPINE_HEAD)


# ---- the run's assertion sink + the input-chain stand-in ----------------------------------------


## Set by `_unhandled_input` below; read only by `_preview_press_reaches_map`.
var _unhandled_press_seen: bool = false

## THE STAND-IN FOR MAPVIEW'S HIT-TESTING. This harness instances no MapView, so the click-through
## test needs something at the end of the input chain to notice a press the GUI did not consume —
## and `_unhandled_input` is the exact callback MapView uses for it.
func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and (event as InputEventMouseButton).pressed:
		_unhandled_press_seen = true


func _assert_hud(label: String, ok: bool) -> void:
	if ok:
		print("ui_preview: PASS hud — ", label)
	else:
		_fail("hud — %s" % label)

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

var _herd_pair_scans := 0
var _herd_pair_violations := 0

## Walk everything reachable from `subject` and check the pair on every dict that carries either half.
## Deliberately a SCAN and not a per-fixture assertion: a guard you have to remember to call for each
## new fixture is the same failure mode as remembering to set the second field.
func _guard_herd_fields(subject: Variant, where: String, depth: int = 0) -> void:
	if depth > HerdFx.HERD_SCAN_MAX_DEPTH:
		return
	if subject is Array:
		for item in (subject as Array):
			_guard_herd_fields(item, where, depth + 1)
		return
	if not (subject is Dictionary):
		return
	var dict: Dictionary = subject
	if dict.has(HerdFx.HERDERS_NEEDED_KEY) or dict.has(HerdFx.HERDERS_NEEDED_IF_MANAGED_KEY):
		_herd_pair_scans += 1
		var needed := int(dict.get(HerdFx.HERDERS_NEEDED_KEY, 0))
		var if_managed := int(dict.get(HerdFx.HERDERS_NEEDED_IF_MANAGED_KEY, 0))
		if if_managed < needed:
			_herd_pair_violations += 1
			_fail(("herd fields — %s herd \"%s\" declares %s %d but %s %d. "
				+ "The would-be crew can never be SMALLER than the ownership-gated one, and on a herd "
				+ "with herders (i.e. a managed one) the sim exports them EQUAL — the investment rungs' "
				+ "worker cap floors on the second field, so half-setting the pair silently caps the "
				+ "crew at the take-side count.") % [where, String(dict.get("id", "?")),
				HerdFx.HERDERS_NEEDED_KEY, needed, HerdFx.HERDERS_NEEDED_IF_MANAGED_KEY, if_managed])
		elif needed > 0 and if_managed != needed:
			# The OTHER half of the invariant, and the one a `>=` test lets through. The gate is the
			# ONLY difference between the two sim functions, so a NON-ZERO gated count already says the
			# herd passed the gate — it is corralled or owned — and the would-be crew is then computed
			# from the same species and headcount by the same arithmetic. A bigger would-be crew is not
			# a conservative fixture, it is an impossible herd: it claims managing this herd would cost
			# MORE than managing it already does.
			_herd_pair_violations += 1
			_fail(("herd fields — %s herd \"%s\" declares %s %d and %s %d. Once "
				+ "%s is above zero the herd IS managed, and the would-be crew is the SAME crew — the "
				+ "sim's two functions differ only by the ownership gate this herd has already passed, "
				+ "so they must be EQUAL here. Set both through HerdFx.set_managed_herders; only a still-WILD "
				+ "tameable herd may carry a larger would-be crew, and its gated count is 0.")
				% [where, String(dict.get("id", "?")), HerdFx.HERDERS_NEEDED_KEY, needed,
				HerdFx.HERDERS_NEEDED_IF_MANAGED_KEY, if_managed, HerdFx.HERDERS_NEEDED_KEY])
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


func _assert_turn_orb(label: String, ok: bool) -> void:
	if ok:
		print("ui_preview: PASS turn-orb — ", label)
	else:
		_fail("turn-orb — %s" % label)


## **NO ROW OF AN OPEN COMPOSE SHEET MAY DEMAND MORE THAN THE CARD'S USABLE WIDTH.** The card is an
## `AutoSizingPanel`, i.e. a plain `Control`: a child's minimum width never reaches it, so a row wider
## than the card does not push the card open — it renders past the card's own rect, and whichever
## clip or edge it meets first cuts it off. That is what a screenshot showed and an assertion did not:
## the local-hunt sheet's three intent presets demanded 384px inside a card declared 340, and the
## widest rows (the 3-up preset grid, the full-width commit button) were the ones sliced.
##
## The check is a MEASUREMENT against the card's real width, not against the nominal `CARD_WIDTH` —
## the fix is that the card GROWS, so pinning the constant would fail every sheet that legitimately
## did. Usable = the card's fitted width less the panel stylebox's left+right margins and the scroll
## gutter, i.e. exactly the room `_fit_width` promised the rows.
##
## The HEADER row is measured with the body's: it sits outside the scroll and carries the subject's
## name, so a long species is content the card must be wide enough for too.
func _assert_compose_sheet_fits(state: String) -> void:
	var sheet: ComposeSheet = _hud._drawercompose._compose_sheet
	if sheet == null or not sheet.visible:
		_assert_hud("%s has an open compose sheet to measure" % state, false)
		return
	var style := HudStyle.card_stylebox()
	var gutter := sheet._scroll.get_v_scroll_bar().get_combined_minimum_size().x
	var usable: float = sheet._card.size.x - style.content_margin_left - style.content_margin_right \
		- gutter
	var rows: Array[Control] = [sheet._header_row]
	for child in sheet._body.get_children():
		if child is Control:
			rows.append(child as Control)
	var worst: float = 0.0
	var worst_row := ""
	for row in rows:
		var demand := row.get_combined_minimum_size().x
		if demand > worst:
			worst = demand
			worst_row = "%s %s" % [row.get_class(), Q.widest_control_face(row)]
	_assert_hud("%s: the widest compose row fits the card (%.0f demanded, %.0f usable of a %.0f card) — %s"
		% [state, worst, usable, sheet._card.size.x, worst_row],
		worst <= usable + Spine.COMPOSE_FIT_SLACK)
	_assert_compose_sheet_card_holds_its_content(state)


## **THE CARD MUST BE AT LEAST AS TALL AS THE PANEL IT DRAWS — the height twin of the rule above, and
## it failed the same way.** `refit` composed the card's chrome from `_header`, the title label, where
## the header ROW is a title beside a taller ✕ button (41 against 20 at the shipped faces). So the card
## was fitted 21px short of what its content demands, and — because an `AutoSizingPanel` is a plain
## `Control` while the `PanelContainer` inside it is a real `Container` — the panel silently grew 9px
## out the bottom of the card (the 12px `CARD_EXTRA_PADDING` absorbing the rest). Nothing clipped in a
## roomy window; what broke was the card's own rect, which is what `_place_card` clamps against the
## viewport and what `fit_to_content` compares to decide whether the sheet must scroll. On a short
## window the sheet therefore ran past the bottom of the screen with the scroll still DISABLED, and the
## commit button was sliced — reported from play on `Hunt Here`.
##
## **THE PANEL'S OWN MINIMUM IS THE HONEST THING TO MEASURE, and re-deriving the chrome would not be.**
## Godot aggregates it from the real children, so it is an independent answer; an assertion written out
## of the same header + separation + margin expression `refit` computes would agree with `refit` by
## construction and pass with the bug fully restored.
##
## It holds in BOTH regimes rather than needing a viewport-clamped branch: where the sheet is genuinely
## taller than the room beneath it, `fit_to_content` turns the internal scroll on, and a scrolling
## `ScrollContainer` stops propagating its child's height — so the panel's minimum collapses and the
## card contains it again. A card clamped short with the scroll still off is exactly the failure.
func _assert_compose_sheet_card_holds_its_content(state: String) -> void:
	var sheet: ComposeSheet = _hud._drawercompose._compose_sheet
	# FAIL rather than skip: this is a regression guard, so a silent `return` is indistinguishable
	# from a pass — and a sheet or panel that has gone missing IS the refactor it exists to catch.
	if sheet == null or sheet._panel == null:
		_assert_hud("%s has a compose panel to measure" % state, false)
		return
	var demanded: float = sheet._panel.get_combined_minimum_size().y
	_assert_hud("%s: the compose card is as tall as the panel it draws (%.0f demanded, %.0f card, scroll %s)"
		% [state, demanded, sheet._card.size.y,
			"on" if sheet._scroll.vertical_scroll_mode != ScrollContainer.SCROLL_MODE_DISABLED else "off"],
		sheet._card.size.y >= demanded - Spine.COMPOSE_FIT_SLACK)
