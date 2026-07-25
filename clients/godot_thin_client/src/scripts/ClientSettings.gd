extends Node

## The first general client-settings store — a ConfigFile wrapper over
## `user://client_settings.cfg`. Deliberately NO `class_name`: the autoload name
## `ClientSettings` would clash with it. Modelled on `ui/BandCityPanel.gd`'s
## `_load_prefs`/`_save_prefs` idiom (incl. `config_path_override` for test isolation).
##
## Holds the map pan/zoom speed multipliers: BASE unit speeds live as consts in
## `MapView.gd`, and these multipliers scale them live at each input site. Written
## by the Options pane, read live by `MapView` (keyboard + trackpad pan, all zoom paths).

const CONFIG_PATH := "user://client_settings.cfg"
const SECTION := "map"
const PAN_KEY := "pan_speed_multiplier"
const ZOOM_KEY := "zoom_speed_multiplier"

const PAN_SPEED_MIN := 0.25
const PAN_SPEED_MAX := 3.0
const PAN_SPEED_DEFAULT := 1.0

const ZOOM_SPEED_MIN := 0.25
const ZOOM_SPEED_MAX := 3.0
const ZOOM_SPEED_DEFAULT := 1.0

## Slider granularity for the Options UI.
const SPEED_STEP := 0.05

## The scratch override when a harness/test set one, else the player's file.
static var config_path_override := ""

var pan_speed_multiplier: float = PAN_SPEED_DEFAULT
var zoom_speed_multiplier: float = ZOOM_SPEED_DEFAULT

signal changed

func _ready() -> void:
	_load()

func _load() -> void:
	var cfg := ConfigFile.new()
	cfg.load(_config_path())   # ignore error — a missing file just keeps the defaults
	pan_speed_multiplier = clampf(
		float(cfg.get_value(SECTION, PAN_KEY, PAN_SPEED_DEFAULT)),
		PAN_SPEED_MIN, PAN_SPEED_MAX)
	zoom_speed_multiplier = clampf(
		float(cfg.get_value(SECTION, ZOOM_KEY, ZOOM_SPEED_DEFAULT)),
		ZOOM_SPEED_MIN, ZOOM_SPEED_MAX)

func set_pan_speed_multiplier(v: float) -> void:
	pan_speed_multiplier = clampf(v, PAN_SPEED_MIN, PAN_SPEED_MAX)
	_save()
	changed.emit()

func set_zoom_speed_multiplier(v: float) -> void:
	zoom_speed_multiplier = clampf(v, ZOOM_SPEED_MIN, ZOOM_SPEED_MAX)
	_save()
	changed.emit()

func restore_defaults() -> void:
	pan_speed_multiplier = PAN_SPEED_DEFAULT
	zoom_speed_multiplier = ZOOM_SPEED_DEFAULT
	_save()
	changed.emit()

func _save() -> void:
	var cfg := ConfigFile.new()
	cfg.load(_config_path())   # preserve any other sections; ignore load errors
	cfg.set_value(SECTION, PAN_KEY, pan_speed_multiplier)
	cfg.set_value(SECTION, ZOOM_KEY, zoom_speed_multiplier)
	cfg.save(_config_path())

## The prefs file actually used — the scratch override when a harness set one, else the player's.
static func _config_path() -> String:
	return config_path_override if config_path_override != "" else CONFIG_PATH
