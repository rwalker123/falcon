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
## Also holds the HUD THEME (`[ui] theme`), and that one is RESTART-TO-APPLY: `_ready` installs the
## saved palette through `HudPalette.apply()` and the setter deliberately does NOT. This autoload runs
## before the main scene is instantiated, so the palette is in place before the first Control exists
## and no panel ever has to be rebuilt for it; applying a theme mid-session would need exactly that
## rebuild pass, which is why the Options row says so instead.

## `HudPalette` is preloaded rather than reached by its global class name because this script is an
## autoload with no `class_name` of its own, and the palette has to be installable from `_ready`.
const HudPalette := preload("res://src/scripts/ui/HudPalette.gd")

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
## The theme id the player last CHOSE. What is on screen this session is `HudPalette.applied_id`, and
## between a pick and the next launch the two differ — which is the whole state the Options caption
## reports.
var theme: String = HudPalette.DEFAULT_THEME

signal changed

func _ready() -> void:
	_load()
	# Restart-to-apply, and this is the restart: an autoload's `_ready` precedes the main scene, so the
	# palette is installed before any Control is built. It runs even when the saved theme IS the
	# default, because `HudStyle`/`MapView`'s DERIVED values (the card fill, the `*_HEX` strings, the
	# overlay table) only exist once `apply_palette` has run.
	HudPalette.apply(theme)

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

## Persist the chosen theme. **It does NOT install it** — the palette is read once at boot, so a live
## swap would leave every already-built Control wearing the old one. The Options row states the
## restart requirement instead of this setter hiding it.
func set_theme(v: String) -> void:
	theme = _valid_theme(v)
	_save()
	changed.emit()


## A theme id the roster still contains, else the default — a hand-edited or downlevel settings file
## must not stop the client from starting.
func _valid_theme(v: String) -> String:
	return v if HudPalette.ids().has(v) else HudPalette.DEFAULT_THEME


func restore_defaults() -> void:
	pan_speed_multiplier = PAN_SPEED_DEFAULT
	zoom_speed_multiplier = ZOOM_SPEED_DEFAULT
	fog_of_war_enabled = FOG_OF_WAR_DEFAULT
	ui_scale = UI_SCALE_DEFAULT
	theme = HudPalette.DEFAULT_THEME
	_save()
	changed.emit()

func _save() -> void:
	var cfg := ConfigFile.new()
	cfg.load(_config_path())   # preserve any other sections; ignore load errors
	cfg.set_value(SECTION, PAN_KEY, pan_speed_multiplier)
	cfg.set_value(SECTION, ZOOM_KEY, zoom_speed_multiplier)
	cfg.set_value(SECTION, FOG_OF_WAR_KEY, fog_of_war_enabled)
	cfg.set_value(UI_SECTION, UI_SCALE_KEY, ui_scale)
	cfg.set_value(UI_SECTION, THEME_KEY, theme)
	cfg.save(_config_path())

## The prefs file actually used — the scratch override when a harness set one, else the player's.
static func _config_path() -> String:
	return config_path_override if config_path_override != "" else CONFIG_PATH
