extends Node

## The first general client-settings store — a ConfigFile wrapper over
## `user://client_settings.cfg`. Deliberately NO `class_name`: the autoload name
## `ClientSettings` would clash with it. Modelled on `ui/BandCityPanel.gd`'s
## `_load_prefs`/`_save_prefs` idiom (incl. `config_path_override` for test isolation).
##
## Holds the map pan/zoom speed multipliers: BASE unit speeds live as consts in
## `MapView.gd`, and these multipliers scale them live at each input site. Written
## by the Options pane, read live by `MapView` (keyboard + trackpad pan, all zoom paths).
##
## Also holds the fog-of-war PREFERENCE. That one is NOT a render flag: fog of war is
## SERVER-authoritative (the sim owns `fog_enabled` and gates both the herd list and the
## visibility raster on it), so this is only what the player last ASKED for. `Main` turns a
## change here into a `set_fog` command and renders from the snapshot's answer. Nothing may
## write this key FROM a snapshot — that closes the loop into an echo.
##
## Also holds the INTERFACE SCALE (`[ui] ui_scale`), the size everything the player reads is drawn
## at. It is not a map setting and does not live in `[map]`: `UiScaler` turns it into the window's
## `content_scale_factor`, which shrinks the logical viewport so every UI anchor re-lays-out larger,
## and `MapView` counter-scales itself by its reciprocal so the world underneath holds still.
##
## Also holds the HUD THEME (`[ui] theme`), and that one only PERSISTS here: `_ready` installs the
## saved palette through `HudPalette.apply()` and the setter deliberately does NOT. This autoload runs
## before the main scene is instantiated, so the palette is in place before the first Control exists
## and no panel is ever restyled afterwards; a pick therefore reaches the screen only when the Options
## row's "Apply now" re-installs the palette and reloads the scene (`GameLaunch.apply_theme_now`).
##
## Also holds the FRAME-RATE CAP (`[display] max_fps`) and the governor that applies it. Without a cap
## Godot redraws at the display refresh (120 Hz on a ProMotion laptop) and every frame runs the
## whole-screen terrain shader, so an idle client heats the machine. The cap is the player's pick from
## `MAX_FPS_CHOICES` (0 = unlimited, Godot's own meaning); `UNFOCUSED_MAX_FPS` is a fixed, harder cap
## while the app is in the background. See `_frame_governor_active` for why only the game is governed.
##
## Also holds the MAP-LAYER TOGGLES (`[map_toggles]`, one bool per `MapToggles.ROWS` key) — the
## minimap's `MAP LAYERS` popover writes them and each layer's renderer reads them. A key the file
## does not hold falls back to its REGISTRY default, so a new toggle needs no migration here.

## `HudPalette` is preloaded rather than reached by its global class name because this script is an
## autoload with no `class_name` of its own, and the palette has to be installable from `_ready`.
const HudPalette := preload("res://src/scripts/ui/HudPalette.gd")
## Preloaded for the same reason as `HudPalette`: the toggle registry the `[map_toggles]` section is
## keyed by.
const MapTogglesRegistry := preload("res://src/scripts/ui/overlay/MapToggles.gd")

const CONFIG_PATH := "user://client_settings.cfg"
const SECTION := "map"
## The interface scale is a CLIENT-CHROME setting, not a map one, so it gets its own section rather
## than being filed under `[map]` beside the pan/zoom multipliers.
const UI_SECTION := "ui"
const PAN_KEY := "pan_speed_multiplier"
const ZOOM_KEY := "zoom_speed_multiplier"
const FOG_OF_WAR_KEY := "fog_of_war_enabled"
const UI_SCALE_KEY := "ui_scale"
const THEME_KEY := "theme"
## The frame-rate cap is a DISPLAY setting: not map navigation (`[map]`) and not client chrome (`[ui]`).
const DISPLAY_SECTION := "display"
const MAX_FPS_KEY := "max_fps"
## The map-layer toggles get their own section: they are a set of independent layers keyed by the
## registry, not map-navigation multipliers.
const MAP_TOGGLES_SECTION := "map_toggles"

const PAN_SPEED_MIN := 0.25
const PAN_SPEED_MAX := 3.0
const PAN_SPEED_DEFAULT := 1.0

const ZOOM_SPEED_MIN := 0.25
const ZOOM_SPEED_MAX := 3.0
const ZOOM_SPEED_DEFAULT := 1.0

## Fog of war ships ON: a new player should meet a map they have to explore.
const FOG_OF_WAR_DEFAULT := true

## Interface scale bounds. The floor is where the smallest HUD type is still legible; the ceiling is
## what the densest panel can grow to before the 1920x1080 design viewport can no longer hold it
## (at 1.5 the logical canvas is 1280x720). 1.0 ships — the size every panel was authored at.
const UI_SCALE_MIN := 0.75
const UI_SCALE_MAX := 1.50
const UI_SCALE_DEFAULT := 1.0

## The caps a player may pick, in the order the Options row lists them. 0 is UNLIMITED, the value
## `Engine.max_fps` itself uses for "no cap". A saved value outside this list falls back to the default.
const MAX_FPS_CHOICES := [30, 60, 120, 0]
const MAX_FPS_DEFAULT := 60
## The rate while the app has lost focus. Fixed, not a setting; it applies even to an Unlimited pick.
const UNFOCUSED_MAX_FPS := 10

## Slider granularity for the Options UI.
const SPEED_STEP := 0.05
## …and the interface scale's own. Its own const rather than a second reader of `SPEED_STEP`: the
## two happen to agree today, but they answer different questions (a multiplier's granularity and a
## type-size increment) and one must be free to move without dragging the other.
const UI_SCALE_STEP := 0.05

## The scratch override when a harness/test set one, else the player's file.
static var config_path_override := ""

var pan_speed_multiplier: float = PAN_SPEED_DEFAULT
var zoom_speed_multiplier: float = ZOOM_SPEED_DEFAULT
var fog_of_war_enabled: bool = FOG_OF_WAR_DEFAULT
var ui_scale: float = UI_SCALE_DEFAULT
## The theme id the player last CHOSE. What is on screen is `HudPalette.applied_id`, and between a
## pick and the apply that installs it the two differ — which is the whole state the Options caption
## reports.
var theme: String = HudPalette.DEFAULT_THEME
## The map-layer toggles the player has SET, key -> bool. A key absent here is at its registry
## default (`is_map_toggle_on`), so this holds only what the file held or the player changed.
var map_toggles: Dictionary = {}
var max_fps: int = MAX_FPS_DEFAULT

## True only when the client booted as the GAME (the project's main scene is the current scene).
## The preview harnesses (`tools/*.tscn`) run this same project and load this autoload, and an
## unfocused 10 fps throttle would slow those windows ~10x and could trip their watchdogs. Harnesses
## that instantiate LandingScreen/Main as children are not the `current_scene`, so they stay ungoverned.
var _frame_governor_active := false
var _app_focused := true

signal changed

func _ready() -> void:
	_load()
	# The BOOT install, the twin of `GameLaunch.apply_theme_now`'s: an autoload's `_ready` precedes the
	# main scene, so the palette is in place before any Control is built. It runs even when the saved theme IS the
	# default, because `HudStyle`/`MapView`'s DERIVED values (the card fill, the `*_HEX` strings, the
	# overlay table) only exist once `apply_palette` has run.
	HudPalette.apply(theme)
	# Deferred: `current_scene` is not set until the main scene has been instantiated, after autoloads.
	_decide_frame_governor.call_deferred()

func _decide_frame_governor() -> void:
	var scene := get_tree().current_scene
	var main_scene := String(ProjectSettings.get_setting("application/run/main_scene", ""))
	_frame_governor_active = scene != null and scene.scene_file_path == main_scene
	_apply_frame_cap()

func _notification(what: int) -> void:
	if what == NOTIFICATION_APPLICATION_FOCUS_OUT:
		_app_focused = false
		_apply_frame_cap()
	elif what == NOTIFICATION_APPLICATION_FOCUS_IN:
		_app_focused = true
		_apply_frame_cap()

## Push the effective cap to the engine: the player's pick while focused, `UNFOCUSED_MAX_FPS` otherwise.
## A no-op unless the governor is active (see `_frame_governor_active`).
func _apply_frame_cap() -> void:
	if not _frame_governor_active:
		return
	Engine.max_fps = max_fps if _app_focused else UNFOCUSED_MAX_FPS

func _load() -> void:
	var cfg := ConfigFile.new()
	cfg.load(_config_path())   # ignore error — a missing file just keeps the defaults
	pan_speed_multiplier = clampf(
		float(cfg.get_value(SECTION, PAN_KEY, PAN_SPEED_DEFAULT)),
		PAN_SPEED_MIN, PAN_SPEED_MAX)
	zoom_speed_multiplier = clampf(
		float(cfg.get_value(SECTION, ZOOM_KEY, ZOOM_SPEED_DEFAULT)),
		ZOOM_SPEED_MIN, ZOOM_SPEED_MAX)
	fog_of_war_enabled = bool(cfg.get_value(SECTION, FOG_OF_WAR_KEY, FOG_OF_WAR_DEFAULT))
	ui_scale = clampf(
		float(cfg.get_value(UI_SECTION, UI_SCALE_KEY, UI_SCALE_DEFAULT)),
		UI_SCALE_MIN, UI_SCALE_MAX)
	theme = _valid_theme(String(cfg.get_value(UI_SECTION, THEME_KEY, HudPalette.DEFAULT_THEME)))
	max_fps = _valid_max_fps(int(cfg.get_value(DISPLAY_SECTION, MAX_FPS_KEY, MAX_FPS_DEFAULT)))
	map_toggles = {}
	for row in MapTogglesRegistry.ROWS:
		var key := String(row[MapTogglesRegistry.KEY])
		if cfg.has_section_key(MAP_TOGGLES_SECTION, key):
			map_toggles[key] = bool(cfg.get_value(MAP_TOGGLES_SECTION, key))

func set_pan_speed_multiplier(v: float) -> void:
	pan_speed_multiplier = clampf(v, PAN_SPEED_MIN, PAN_SPEED_MAX)
	_save()
	changed.emit()

func set_zoom_speed_multiplier(v: float) -> void:
	zoom_speed_multiplier = clampf(v, ZOOM_SPEED_MIN, ZOOM_SPEED_MAX)
	_save()
	changed.emit()

func set_fog_of_war_enabled(v: bool) -> void:
	fog_of_war_enabled = v
	_save()
	changed.emit()

func set_ui_scale(v: float) -> void:
	ui_scale = clampf(v, UI_SCALE_MIN, UI_SCALE_MAX)
	_save()
	changed.emit()

## Persist the chosen theme. **It does NOT install it** — installing without rebuilding would leave
## every already-built Control wearing the old palette. The Options row says the pick is not applied
## yet and offers the rebuild, instead of this setter hiding the question.
func set_theme(v: String) -> void:
	theme = _valid_theme(v)
	_save()
	changed.emit()


## Persist the frame-rate cap and apply it at once (when the governor is active).
func set_max_fps(v: int) -> void:
	max_fps = _valid_max_fps(v)
	_save()
	_apply_frame_cap()
	changed.emit()

## Is the map layer `key` on? The player's saved choice, else the registry's default.
func is_map_toggle_on(key: String) -> bool:
	if map_toggles.has(key):
		return bool(map_toggles[key])
	return MapTogglesRegistry.default_for(key)

## Turn the map layer `key` on or off, persist it, and tell every listener (MapView redraws).
func set_map_toggle(key: String, on: bool) -> void:
	map_toggles[key] = on
	_save()
	changed.emit()


## A theme id the roster still contains, else the default — a hand-edited or downlevel settings file
## must not stop the client from starting.
func _valid_theme(v: String) -> String:
	return v if HudPalette.ids().has(v) else HudPalette.DEFAULT_THEME


## A cap the choice list contains, else the default: a hand-edited file must not set an odd rate.
func _valid_max_fps(v: int) -> int:
	return v if MAX_FPS_CHOICES.has(v) else MAX_FPS_DEFAULT


func restore_defaults() -> void:
	pan_speed_multiplier = PAN_SPEED_DEFAULT
	zoom_speed_multiplier = ZOOM_SPEED_DEFAULT
	fog_of_war_enabled = FOG_OF_WAR_DEFAULT
	ui_scale = UI_SCALE_DEFAULT
	theme = HudPalette.DEFAULT_THEME
	max_fps = MAX_FPS_DEFAULT
	map_toggles = {}
	_save()
	_apply_frame_cap()
	changed.emit()

func _save() -> void:
	var cfg := ConfigFile.new()
	cfg.load(_config_path())   # preserve any other sections; ignore load errors
	cfg.set_value(SECTION, PAN_KEY, pan_speed_multiplier)
	cfg.set_value(SECTION, ZOOM_KEY, zoom_speed_multiplier)
	cfg.set_value(SECTION, FOG_OF_WAR_KEY, fog_of_war_enabled)
	cfg.set_value(UI_SECTION, UI_SCALE_KEY, ui_scale)
	cfg.set_value(UI_SECTION, THEME_KEY, theme)
	cfg.set_value(DISPLAY_SECTION, MAX_FPS_KEY, max_fps)
	# The section is rewritten whole, so a restore-to-defaults (an empty `map_toggles`) clears the
	# saved choices rather than leaving the last ones in the file.
	if cfg.has_section(MAP_TOGGLES_SECTION):
		cfg.erase_section(MAP_TOGGLES_SECTION)
	for key in map_toggles:
		cfg.set_value(MAP_TOGGLES_SECTION, String(key), bool(map_toggles[key]))
	cfg.save(_config_path())

## The prefs file actually used — the scratch override when a harness set one, else the player's.
static func _config_path() -> String:
	return config_path_override if config_path_override != "" else CONFIG_PATH
