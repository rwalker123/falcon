class_name LineageWorld
extends RefCounted

## **THE TWO WORLD CONSTANTS THE FAMILY LIMIT IS PRICED WITH** (issue #691) — the campaign section's
## `lineage_people_per_line` (K: the people one founding line carries) and `lineage_free_breeding_at`
## (the head-count at which a people breeds freely for good), echoed off the live demographics config
## so the client never restates it. `0` on the wire means "unchanged" (a delta that does not carry
## them), so an ABSENT or zero value leaves the last one standing, exactly as `FactionNames` does.
##
## STATIC for the reason `FactionNames` is: its readers (the Family limit popover, the split sheet)
## live on different owners, and `Main` is the only writer. PER WORLD — `Main._reset_per_world_state`
## clears it.

static var _people_per_line: int = 0
static var _free_breeding_at: int = 0

## Ingest a snapshot/delta frame: a stated (positive) constant replaces the held one.
static func update(frame: Dictionary) -> void:
	var per_line := int(frame.get("lineage_people_per_line", 0))
	if per_line > 0:
		_people_per_line = per_line
	var free := int(frame.get("lineage_free_breeding_at", 0))
	if free > 0:
		_free_breeding_at = free

## WORLD BOUNDARY: forget both.
static func reset() -> void:
	_people_per_line = 0
	_free_breeding_at = 0

## K — people per founding line; 0 until the world has stated it.
static func people_per_line() -> int:
	return _people_per_line

## The head-count at which a people breeds freely for good; 0 until stated.
static func free_breeding_at() -> int:
	return _free_breeding_at
