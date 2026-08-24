extends RefCounted
class_name OverlayChannels

## THE OVERLAY CHANNEL REGISTRY — the one place a map overlay channel is declared client-side, and
## the merge that folds those declarations into the roster the server publishes.
##
## **Adding a channel is one row in `CHANNELS`** — the same property `WorkbenchPages.PAGES` gives the
## Workbench's pages. Nothing downstream names a channel: `OverlayPicker` builds its list from
## `roster()`, `OverlayLegend` renders whichever `legend_kind` the row declares, and neither has a
## branch that could grow one. That is the whole reason this file exists. The Inspector panel it
## replaced carried the `terrain_tags` label, description and availability test inline in its ingest
## function, and two hand-written placeholder tabs for `culture` and `military` in its refresh
## function — two places that grew a branch per channel forever.
##
## **A WIRE CHANNEL NEEDS NO ROW AT ALL.** `core_sim` publishes `overlays.channels` with a label, a
## description and a placeholder flag per channel, and `MapView._ingest_overlay_channels` holds them;
## `roster()` synthesizes a descriptor from that metadata for every key the wire names. A row here is
## for a channel the CLIENT adds (`terrain_tags`, assembled from the per-tile tag masks with no
## raster behind it), or to give a wire channel a legend kind other than the scalar ramp.
##
## **`available` AND `facts` ARE METHOD NAMES ON THE MAPVIEW, NOT `Callable`s.** A `Callable` is not a
## constant expression, so a registry holding one could not be `const` — and a non-`const` registry is
## one that whatever ingests it can mutate at runtime, which is exactly the property being bought
## here. The names are resolved through `has_method` at merge time, so a row naming a method that does
## not exist drops the channel rather than crashing the picker.
##
## **`icon` IS A TABLE WITH A LIVE FALLBACK, the `FaunaSprites` / `WonderSprites` shape.** A channel
## that names one wears it on the minimap's legend button; a channel that does not wears that
## channel's own ramp COLOUR as a swatch (`MapView.overlay_color_for`). So icons land one channel at a
## time as art appears, with no flag day and no channel ever facing a blank button — and the fallback
## is a real, exercised path rather than a guard, every row being iconless today. Two things to hold
## to when one arrives: a mark here is drawn at `LEGEND_BUTTON` size beside a `◐`, and this client has
## shipped covered glyphs that were still unreadable at exactly that size (`WorkbenchPages.PAGES` has
## the autopsy), so **render it before trusting it**; and the swatch it replaces is stating a fact
## about the map, so an icon that says less than the colour did is a step backwards.

## The legend shapes a row can ask for. They live on `OverlayLegend` because it is what renders them;
## the dependency runs registry → renderer and never back, so the two `class_name`s do not cycle.
const KIND_RAMP := OverlayLegend.KIND_RAMP
const KIND_FACTS := OverlayLegend.KIND_FACTS

## Where a client-side row sits relative to the wire's own `channel_order`.
const PLACEMENT_FIRST := 0
const PLACEMENT_LAST := 1

## The empty key — "paint no overlay". `MapView.set_overlay_channel("")` is the clear.
const NO_OVERLAY_KEY := ""

const TERRAIN_TAGS_KEY := "terrain_tags"

const CHANNELS: Array[Dictionary] = [
	{
		"key": NO_OVERLAY_KEY,
		"label": "No Overlay",
		"description": "Base map without overlays.",
		# **A RAMP, BECAUSE THE BARE MAP HAS A LEGEND TOO** — `MapView._build_terrain_legend`, the
		# biome key with per-biome tile counts. That key was the right dock's `L` card until this arc
		# retired it; it is not the same table as `terrain_tags` (biomes, not environmental tags), so
		# without this row it would have had no home at all.
		"legend_kind": KIND_RAMP,
		"placement": PLACEMENT_FIRST,
	},
	{
		"key": TERRAIN_TAGS_KEY,
		"label": "Terrain Tags",
		"description": "Colors tiles by environmental tags (blends when multiple tags apply).",
		"legend_kind": KIND_RAMP,
		# The tag channel has no wire raster — it is assembled from the per-tile tag masks — so it is
		# offered only on a world that carries them.
		"available": &"has_terrain_tag_data",
		"placement": PLACEMENT_LAST,
	},
	{
		# **THE AGGREGATE ⌃** (`docs/plan_knowledge_screen.md` §7). The map already marks a single
		# worked source that can climb a rung; this is every one of them at once. Like `terrain_tags`
		# it has no wire raster — the answer depends on what the PLAYER's faction knows, which the sim
		# does not publish a plane of — so `MapView` synthesizes it and this row does the rest.
		#
		# **AND IT IS THE FIRST `KIND_FACTS` ROW EVER WRITTEN.** The kind and `facts_for` were built in
		# Slice A for exactly this channel; nothing had exercised them until this row.
		"key": ReadyToClimb.CHANNEL_KEY,
		"label": ReadyToClimb.CHANNEL_LABEL,
		"description": ReadyToClimb.CHANNEL_DESCRIPTION,
		"legend_kind": KIND_FACTS,
		# A ramp legend would be a lie about a binary plane: there is no "more ready". What the player
		# wants is the SHAPE — how many, on which web, and whether any of it is ground nobody stands
		# on — and those are lines, not rows.
		"facts": &"ready_to_climb_facts",
		# Gated on the RASTER, not on the count: a world with sources offers the channel even when
		# nothing is ready (the legend says so), and `set_overlay_channel` would silently refuse the
		# key on a world that has none.
		"available": &"has_ready_to_climb_data",
		# **AND THE PLACEMENT IS LOAD-BEARING BECAUSE THE CHANNEL IS BUILT LAZILY.** `MapView` does not
		# synthesize this raster during the ingest (`DEFERRED_OVERLAY_BUILDERS` — a `RungGates` pass per
		# source is not turn-boundary work for a channel nobody has picked), so for most of a frame's
		# life the key is absent from `overlay_channel_order` and the wire pass cannot place it. This
		# row is what puts it in the list at all, and last is where a client-side channel belongs.
		"placement": PLACEMENT_LAST,
	},
]

## The merged, ordered channel list: every key the picker offers, each a COMPLETE descriptor, so no
## caller reads `MapView`'s channel dictionaries itself.
##
## Order is `PLACEMENT_FIRST` rows, then the wire's own `channel_order`, then `PLACEMENT_LAST` rows —
## each key taken once, by whichever pass reaches it first. A registry row the wire also publishes
## (the empty key is both) keeps its registry metadata and the wire's position.
static func roster(view: Object) -> Array[Dictionary]:
	var available: Dictionary = _available_rows(view)
	var keys: Array[String] = []
	for row in CHANNELS:
		var first_key := String(row.get("key", ""))
		if available.has(first_key) and int(row.get("placement", PLACEMENT_LAST)) == PLACEMENT_FIRST:
			keys.append(first_key)
	for wire_key in _wire_order(view):
		if not keys.has(String(wire_key)):
			keys.append(String(wire_key))
	for row in CHANNELS:
		var last_key := String(row.get("key", ""))
		if available.has(last_key) and not keys.has(last_key):
			keys.append(last_key)
	var out: Array[Dictionary] = []
	for key in keys:
		out.append(descriptor_for(view, key, available))
	return out

## One complete descriptor: the registry row (when there is one) over the wire's own metadata.
## `available_rows` is the pre-filtered registry `roster()` already built; pass `{}` to look it up.
static func descriptor_for(view: Object, key: String, available_rows: Dictionary = {}) -> Dictionary:
	var rows: Dictionary = available_rows if not available_rows.is_empty() else _available_rows(view)
	var row: Dictionary = rows.get(key, {})
	var descriptor: Dictionary = {
		"key": key,
		"label": _wire_string(view, &"overlay_channel_labels", key, key.capitalize()),
		"description": _wire_string(view, &"overlay_channel_descriptions", key, ""),
		"legend_kind": KIND_RAMP,
		"placeholder": _wire_flag(view, &"overlay_placeholder_flags", key),
		"facts": &"",
		"icon": "",
	}
	for field in row.keys():
		var value: Variant = row[field]
		# An empty registry string means "say nothing here", not "blank the wire's answer".
		if value is String and String(value) == "":
			continue
		descriptor[field] = value
	return descriptor

## The fact lines a `KIND_FACTS` row's provider answers, and an empty list for every other row.
static func facts_for(view: Object, descriptor: Dictionary) -> PackedStringArray:
	var probe := StringName(descriptor.get("facts", &""))
	if probe == &"" or view == null or not view.has_method(probe):
		return PackedStringArray()
	var result: Variant = view.call(probe)
	return result if result is PackedStringArray else PackedStringArray()

## The registry rows whose `available` predicate passes, keyed by channel key.
static func _available_rows(view: Object) -> Dictionary:
	var out: Dictionary = {}
	for row in CHANNELS:
		if _is_available(view, row):
			out[String(row.get("key", ""))] = row
	return out

static func _is_available(view: Object, row: Dictionary) -> bool:
	var probe := StringName(row.get("available", &""))
	if probe == &"":
		return true
	if view == null or not view.has_method(probe):
		return false
	return bool(view.call(probe))

static func _wire_order(view: Object) -> PackedStringArray:
	if view == null:
		return PackedStringArray()
	var value: Variant = view.get(&"overlay_channel_order")
	return value if value is PackedStringArray else PackedStringArray()

static func _wire_string(view: Object, member: StringName, key: String, fallback: String) -> String:
	if view == null:
		return fallback
	var table: Variant = view.get(member)
	if not (table is Dictionary) or not (table as Dictionary).has(key):
		return fallback
	return String((table as Dictionary)[key])

static func _wire_flag(view: Object, member: StringName, key: String) -> bool:
	if view == null:
		return false
	var table: Variant = view.get(member)
	if not (table is Dictionary):
		return false
	return bool((table as Dictionary).get(key, false))
