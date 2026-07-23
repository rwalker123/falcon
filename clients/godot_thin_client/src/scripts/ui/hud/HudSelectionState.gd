class_name HudSelectionState
extends RefCounted

## "What is the player looking at" — the selection triplet (tile / unit / herd), which KIND of
## subject row is lit, the assembled Occupants roster for the current hex, and the sticky-selection
## guard. Pure DATA: it never holds a scene node or a `%Name` lookup. Every selection transition that
## used to be a scattered `_selected_* =` assignment on `HudLayer` is now exactly ONE mutator call
## here, and each mutator emits `changed(reason)` so a future consumer can diff an identity change
## from a restate. Nothing consumes the signal yet (Phase 0 emits it; Phase 2 subscribes).
##
## Dictionaries/Arrays are returned BY REFERENCE from the read accessors — this matches the HUD's
## existing in-place-mutation behaviour (e.g. `unit().clear()`), so callers must NOT assume a copy.

signal changed(reason: StringName)

# Which KIND of subject row is lit. `unit()` / `herd()` stay authoritative for WHICH unit/herd; this
# only picks the drawer. Aliased back onto `HudLayer` so every existing `SUBJECT_*` reference works.
const SUBJECT_LAND := "land"
const SUBJECT_UNIT := "unit"
const SUBJECT_HERD := "herd"

var _selected_tile_info: Dictionary = {}
var _selected_unit: Dictionary = {}
var _selected_herd: Dictionary = {}
var _selected_subject: String = SUBJECT_LAND
# The assembled Occupants roster for the current hex (full unit markers + herd dicts).
var _roster_units: Array = []
var _roster_herds: Array = []
# The hex the player last EXPLICITLY chose a subject on, so the auto-select rule can tell a fresh hex
# (pick a default) from a decided one (preserve). `(-1, -1)` = no choice yet, matching no real hex.
var _subject_choice_tile: Vector2i = Vector2i(-1, -1)

# ---- Read accessors (backing value returned by reference — no deep copy) --------------------------

func tile_info() -> Dictionary:
	return _selected_tile_info

func unit() -> Dictionary:
	return _selected_unit

func herd() -> Dictionary:
	return _selected_herd

func subject() -> String:
	return _selected_subject

func roster_units() -> Array:
	return _roster_units

func roster_herds() -> Array:
	return _roster_herds

func choice_tile() -> Vector2i:
	return _subject_choice_tile

func has_unit() -> bool:
	return not _selected_unit.is_empty()

func has_herd() -> bool:
	return not _selected_herd.is_empty()

func is_land() -> bool:
	return _selected_subject == SUBJECT_LAND

# ---- Mutators (each emits `changed(reason)`) ------------------------------------------------------

## A NEW hex was picked: the land is the lit subject and both occupant dicts are cleared. This is the
## sticky-selection invariant — choosing LAND clears occupants — and it reassigns the tile.
func select_tile(tile: Dictionary) -> void:
	_selected_tile_info = tile
	_selected_unit = {}
	_selected_herd = {}
	_selected_subject = SUBJECT_LAND
	changed.emit(&"tile")

## Refresh the tile dict alone, WITHOUT touching the occupant selection or subject — used when a
## unit/herd selection carries fresh tile_info for the same hex it already occupies.
func set_tile_info(tile: Dictionary) -> void:
	_selected_tile_info = tile
	changed.emit(&"tile")

## A unit is the lit subject: set it, drop any herd, light the UNIT drawer. (The tile is set
## separately by the caller — it belongs to the occupant, not this transition.)
func select_unit(u: Dictionary) -> void:
	_selected_unit = u
	_selected_herd = {}
	_selected_subject = SUBJECT_UNIT
	changed.emit(&"unit")

## A herd is the lit subject: set it, drop any unit, light the HERD drawer.
func select_herd(h: Dictionary) -> void:
	_selected_herd = h
	_selected_unit = {}
	_selected_subject = SUBJECT_HERD
	changed.emit(&"herd")

## The land row is chosen (or the selected occupant vanished): clear both occupant dicts and light
## the LAND drawer, but KEEP the tile — unlike `select_tile`, no new hex was picked.
func select_land() -> void:
	_selected_unit = {}
	_selected_herd = {}
	_selected_subject = SUBJECT_LAND
	changed.emit(&"land")

func set_subject(kind: String) -> void:
	_selected_subject = kind
	changed.emit(&"subject")

func set_roster(units: Array, herds: Array) -> void:
	_roster_units = units
	_roster_herds = herds
	changed.emit(&"roster")

func note_choice_tile(tile: Vector2i) -> void:
	_subject_choice_tile = tile
	changed.emit(&"choice")

func clear() -> void:
	_selected_tile_info = {}
	_selected_unit = {}
	_selected_herd = {}
	_selected_subject = SUBJECT_LAND
	_roster_units = []
	_roster_herds = []
	_subject_choice_tile = Vector2i(-1, -1)
	changed.emit(&"clear")
