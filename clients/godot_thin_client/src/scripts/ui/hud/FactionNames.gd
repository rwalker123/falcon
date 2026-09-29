class_name FactionNames
extends RefCounted

## **EVERY FACTION'S NAME, AS THE SIM MINTED IT** — the campaign section's `faction_names`
## (`[{faction, name}]`, world-visible, every faction). The ONE store of that table; the ONE resolver
## over it is `FactionMark.faction_name`, which every surface naming a people calls (the event dock's
## `from=`/`to=` join, the trade tab's counterparty mark, the faction page header).
##
## **STATIC, because its readers span owners**: the event dock is a CanvasLayer `Main` owns, the
## faction mark is a static helper with no HUD in hand, and the faction page lives on the Band/City
## panel. A per-instance model on `HudLayer` would need threading into all three. It holds nothing
## but the table, and `Main` is its only writer.
##
## **PER WORLD** — `Main._reset_per_world_state` clears it, so a new world never names its peoples
## with the previous game's names while its own table is in flight
## (`.claude/rules/core_sim/world-handoff.md`).

## `{faction id: name}`. Empty until a snapshot carries the table.
static var _names: Dictionary = {}

## INGEST the table. A non-Array leaves the last value standing — this HUD's catalogue-setter
## convention (absence means unchanged, never "no factions"). A present table REPLACES the store
## wholesale: the sim publishes every faction on every carrying frame. A row with an empty name is
## dropped, so the resolver's fallback answers for it rather than an empty string.
static func update(names_variant: Variant) -> void:
	if not (names_variant is Array):
		return
	var names := {}
	for entry_variant in names_variant:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var name := String(entry.get("name", "")).strip_edges()
		if name == "":
			continue
		names[int(entry.get("faction", HudConst.NO_FACTION_ID))] = name
	_names = names

## WORLD BOUNDARY: forget every name.
static func reset() -> void:
	_names = {}

## The published name for `faction`, or `""` when the table has no row for it. Callers go through
## `FactionMark.faction_name`, which owns the fallback wording.
static func name_of(faction: int) -> String:
	return String(_names.get(faction, ""))
